use std::collections::BTreeMap;
use std::ops::Bound::{self, Excluded, Included, Unbounded};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use sodiumoxide::{crypto::hash, hex};
use tokio::sync::Notify;

use crate::storage::*;

/// This implementation is very inneficient, and not completely correct
/// Indeed, when the connector is dropped, the memory is freed.
/// It means that when a user disconnects, its data are lost.
/// It's intended only for basic debugging, do not use it for advanced tests...

#[derive(Debug, Default)]
pub struct MemDb(tokio::sync::Mutex<HashMap<String, Arc<MemStore>>>);
impl MemDb {
    pub fn new() -> Self {
        Self(tokio::sync::Mutex::new(HashMap::new()))
    }

    pub async fn user(&self, username: &str) -> Arc<MemStore> {
        let mut global_storage = self.0.lock().await;
        global_storage
            .entry(username.to_string())
            .or_insert(Arc::new(MemStore::new(username)))
            .clone()
    }
}

#[derive(Debug, Clone)]
enum InternalData {
    Tombstone,
    Value(Vec<u8>),
}
impl InternalData {
    fn to_alternative(&self) -> Alternative {
        match self {
            Self::Tombstone => Alternative::Tombstone,
            Self::Value(x) => Alternative::Value(x.clone()),
        }
    }
}

#[derive(Debug)]
struct InternalRowVal {
    data: Vec<(u64, InternalData)>,
    change: Arc<Notify>,
}
impl std::default::Default for InternalRowVal {
    fn default() -> Self {
        Self {
            data: vec![],
            change: Arc::new(Notify::new()),
        }
    }
}
impl InternalRowVal {
    fn max_version(&self) -> u64 {
        self.data.iter().map(|(v, _)| *v).max().unwrap_or(0)
    }

    fn concurrent_values(&self) -> Vec<Alternative> {
        self.data.iter().map(|(_, d)| d.to_alternative()).collect()
    }

    fn to_concurrent_row_val(&self, shard: &str, sort: &str) -> ConcurrentRowVal {
        ConcurrentRowVal {
            row_ref: RowRef::new(shard, sort).with_causality(self.max_version().to_string()),
            value: self.concurrent_values(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SeenMarker {
    seen: BTreeMap<String, u64>,
}

impl SeenMarker {
    fn encode(&self) -> String {
        use base64::prelude::*;
        let bytes = rmp_serde::to_vec(self).expect("SeenMarker encode");
        BASE64_STANDARD.encode(&bytes)
    }

    fn decode(s: &str) -> Self {
        use base64::prelude::*;
        let bytes = BASE64_STANDARD.decode(s).expect("valid seen_marker");
        rmp_serde::from_slice::<SeenMarker>(&bytes).expect("valid seen_marker")
    }
}

#[derive(Debug, Default, Clone)]
struct InternalBlobVal {
    data: Vec<u8>,
    metadata: HashMap<String, String>,
}
impl InternalBlobVal {
    fn to_blob_val(&self, bref: &BlobRef) -> BlobVal {
        BlobVal {
            blob_ref: bref.clone(),
            meta: self.metadata.clone(),
            value: self.data.clone(),
        }
    }
    fn etag(&self) -> String {
        let digest = hash::hash(self.data.as_ref());
        let buff = digest.as_ref();
        let hexstr = hex::encode(buff);
        format!("\"{}\"", hexstr)
    }
}

type ArcRow = Arc<RwLock<Row>>;
type ArcBlob = Arc<RwLock<BTreeMap<String, InternalBlobVal>>>;

#[derive(Debug)]
struct Row {
    table: HashMap<String, BTreeMap<String, InternalRowVal>>,
    change: Arc<Notify>,
}

#[derive(Clone, Debug)]
pub struct MemStore {
    unicity: Vec<u8>,
    row: ArcRow,
    blob: ArcBlob,
}

impl MemStore {
    pub fn new(user: &str) -> Self {
        tracing::debug!("initialize membuilder for {}", user);
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.extend_from_slice(user.as_bytes());
        Self {
            unicity,
            row: Arc::new(RwLock::new(Row {
                table: HashMap::new(),
                change: Arc::new(Notify::new()),
            })),
            blob: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

fn prefix_last_bound(prefix: &str) -> Bound<String> {
    let mut sort_end = prefix.to_string();
    match sort_end.pop() {
        None => Unbounded,
        Some(ch) => {
            let nc = char::from_u32(ch as u32 + 1).unwrap();
            sort_end.push(nc);
            Excluded(sort_end)
        }
    }
}

#[async_trait]
impl IStore for MemStore {
    async fn row_fetch(&self, shard: &str, sort: &str) -> Result<ConcurrentRowVal, StorageError> {
        tracing::trace!(shard=%shard, sort=%sort, command="row_fetch");
        let store = self.row.read().or(Err(StorageError::Internal))?;
        let v = store
            .table
            .get(shard)
            .ok_or(StorageError::NotFound)?
            .get(sort)
            .ok_or(StorageError::NotFound)?;
        Ok(v.to_concurrent_row_val(shard, sort))
    }

    async fn row_fetch_batch<'a>(
        &self,
        select: &Selector<'a>,
    ) -> Result<Vec<ConcurrentRowVal>, StorageError> {
        tracing::trace!(select=%select, command="row_fetch_batch");
        let store = self.row.read().or(Err(StorageError::Internal))?;
        let (shard, sorts) = select_keys(&store.table, select);
        Ok(sorts
            .into_iter()
            .map(|sort| {
                let v = store.table.get(&shard).unwrap().get(&sort).unwrap();
                v.to_concurrent_row_val(&shard, &sort)
            })
            .collect::<Vec<_>>())
    }

    async fn row_delete_batch<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        tracing::trace!(select=%select, command="row_delete_batch");
        // read the current causality for the selected keys, then insert a
        // tombstone that supersedes each key
        let del = self
            .row_fetch_batch(select)
            .await?
            .into_iter()
            .map(|v| RowVal::deleted(v.row_ref))
            .collect::<Vec<_>>();
        self.row_update(del).await
    }

    async fn row_update(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        tracing::trace!(entries=%values.iter().map(|v| v.row_ref.to_string()).collect::<Vec<_>>().join(","), command="row_update");
        let mut store = self.row.write().or(Err(StorageError::Internal))?;
        for v in values.into_iter() {
            let shard = v.row_ref.uid.shard;
            let sort = v.row_ref.uid.sort;

            let val = match v.value {
                Alternative::Value(x) => InternalData::Value(x),
                Alternative::Tombstone => InternalData::Tombstone,
            };

            let cauz = match v.row_ref.causality.map(|v| v.parse::<u64>()) {
                Some(Ok(v)) => v,
                _ => 0,
            };

            let bt = store.table.entry(shard).or_default();
            let intval = bt.entry(sort).or_default();

            let max_version = intval.max_version();
            intval.data = std::mem::take(&mut intval.data)
                .into_iter()
                .filter(|(ver, _)| *ver > cauz)
                .collect::<Vec<_>>();

            intval.data.push((max_version + 1, val));
            intval.change.notify_waiters();
            store.change.notify_waiters();
        }
        Ok(())
    }

    async fn row_poll(&self, value: &RowRef) -> Result<ConcurrentRowVal, StorageError> {
        tracing::trace!(entry=%value, command="row_poll");
        let shard = &value.uid.shard;
        let sort = &value.uid.sort;
        let cauz = match value.causality.as_ref().map(|v| v.parse::<u64>()) {
            Some(Ok(v)) => v,
            _ => 0,
        };

        let notify_me = {
            let mut store = self.row.write().or(Err(StorageError::Internal))?;
            let intval = store
                .table
                .get_mut(shard)
                .and_then(|bt| bt.get_mut(sort))
                .ok_or(StorageError::NotFound)?;

            if intval.max_version() > cauz {
                return Ok(intval.to_concurrent_row_val(shard, sort));
            }
            intval.change.clone()
        };

        notify_me.notified().await;
        self.row_fetch(shard, sort).await
    }

    async fn row_poll_range<'a>(
        &self,
        select: &RangeSelector<'a>,
        seen_marker: Option<&str>,
    ) -> Result<PollRangeResult, StorageError> {
        tracing::trace!(select=%select, command="row_poll_range");
        let prev_seen = seen_marker.map(SeenMarker::decode);
        loop {
            let change = {
                let store = self.row.read().or(Err(StorageError::Internal))?;
                let (shard, sorts) = select_keys(&store.table, &select.as_selector());
                let vals = sorts
                    .into_iter()
                    .map(|sort| {
                        let intv = store.table.get(&shard).unwrap().get(&sort).unwrap();
                        (sort, intv)
                    })
                    .collect::<Vec<_>>();

                let mut new_vals = vec![];
                let mut seen = BTreeMap::new();
                for (sort, intv) in vals {
                    seen.insert(sort.clone(), intv.max_version());
                    let is_new = match &prev_seen {
                        None => true,
                        Some(prev) => intv.max_version() > *prev.seen.get(&sort).unwrap_or(&0),
                    };
                    if is_new {
                        new_vals.push(intv.to_concurrent_row_val(&shard, &sort));
                    }
                }

                if prev_seen.is_none() || !new_vals.is_empty() {
                    return Ok(PollRangeResult {
                        value: new_vals,
                        seen_marker: SeenMarker { seen }.encode(),
                    });
                }

                store.change.clone()
            };

            change.notified().await;
        }
    }

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError> {
        tracing::trace!(entry=%blob_ref, command="blob_fetch");
        let store = self.blob.read().or(Err(StorageError::Internal))?;
        store
            .get(&blob_ref.0)
            .ok_or(StorageError::NotFound)
            .map(|v| v.to_blob_val(blob_ref))
    }
    async fn blob_insert(&self, blob_val: BlobVal) -> Result<String, StorageError> {
        tracing::trace!(entry=%blob_val.blob_ref, command="blob_insert");
        let mut store = self.blob.write().or(Err(StorageError::Internal))?;
        let entry = store.entry(blob_val.blob_ref.0.clone()).or_default();
        entry.data = blob_val.value.clone();
        entry.metadata = blob_val.meta.clone();

        Ok(entry.etag())
    }
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError> {
        tracing::trace!(src=%src, dst=%dst, command="blob_copy");
        let mut store = self.blob.write().or(Err(StorageError::Internal))?;
        let blob_src = store.entry(src.0.clone()).or_default().clone();
        store.insert(dst.0.clone(), blob_src);
        Ok(())
    }
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError> {
        tracing::trace!(prefix = prefix, command = "blob_list");
        let store = self.blob.read().or(Err(StorageError::Internal))?;
        let last_bound = prefix_last_bound(prefix);
        let blist = store
            .range((Included(prefix.to_string()), last_bound))
            .map(|(k, _)| BlobRef(k.to_string()))
            .collect::<Vec<_>>();
        Ok(blist)
    }
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError> {
        tracing::trace!(entry=%blob_ref, command="blob_rm");
        let mut store = self.blob.write().or(Err(StorageError::Internal))?;
        store.remove(&blob_ref.0);
        Ok(())
    }

    fn unique(&self) -> UnicityBuffer {
        UnicityBuffer(self.unicity.clone())
    }
}

/// Returns keys of `store` that are selected by `select`, as pairs of (shard, sort).
/// These are guaranteed to be valid keys of `store`, associated with a value.
fn select_keys<'a, V>(
    store: &HashMap<String, BTreeMap<String, V>>,
    select: &Selector<'a>,
) -> (String, Vec<String>) {
    match select {
        Selector::Range {
            shard,
            sort_begin,
            sort_end,
        } => (
            shard.to_string(),
            store
                .get(*shard)
                .map(|bt| {
                    bt.range((
                        sort_begin
                            .map(|b| Included(b.to_string()))
                            .unwrap_or(Unbounded),
                        sort_end
                            .map(|e| Excluded(e.to_string()))
                            .unwrap_or(Unbounded),
                    ))
                    .map(|(sort, _)| sort.clone())
                    .collect()
                })
                .unwrap_or(vec![]),
        ),
        Selector::List { shard, sort_list } => (
            shard.to_string(),
            sort_list
                .iter()
                .filter_map(|sort| {
                    let bt = store.get(*shard)?;
                    let _ = bt.get(*sort)?;
                    Some(sort.to_string())
                })
                .collect::<Vec<_>>(),
        ),
        Selector::Prefix { shard, sort_prefix } => {
            let last_bound = prefix_last_bound(sort_prefix);
            let sorts = store
                .get(*shard)
                .map(|bt| {
                    bt.range((Included(sort_prefix.to_string()), last_bound))
                        .map(|(sort, _)| sort.clone())
                        .collect()
                })
                .unwrap_or(vec![]);
            (shard.to_string(), sorts)
        }
        Selector::Single { shard, sort } => (
            shard.to_string(),
            store
                .get(*shard)
                .and_then(|bt| bt.get(*sort))
                .map(|_| vec![sort.to_string()])
                .unwrap_or(vec![]),
        ),
    }
}
