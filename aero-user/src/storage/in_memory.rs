use std::collections::BTreeMap;
use std::ops::Bound::{self, Excluded, Included, Unbounded};
use std::sync::RwLock;

use sodiumoxide::{crypto::hash, hex};
use tokio::sync::Notify;

use crate::storage::*;

/// This implementation is very inneficient, and not completely correct
/// Indeed, when the connector is dropped, the memory is freed.
/// It means that when a user disconnects, its data are lost.
/// It's intended only for basic debugging, do not use it for advanced tests...

#[derive(Debug, Default)]
pub struct MemDb(tokio::sync::Mutex<HashMap<String, Arc<MemBuilder>>>);
impl MemDb {
    pub fn new() -> Self {
        Self(tokio::sync::Mutex::new(HashMap::new()))
    }

    pub async fn builder(&self, username: &str) -> Arc<MemBuilder> {
        let mut global_storage = self.0.lock().await;
        global_storage
            .entry(username.to_string())
            .or_insert(MemBuilder::new(username))
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
    data: Vec<InternalData>,
    version: u64,
    change: Arc<Notify>,
}
impl std::default::Default for InternalRowVal {
    fn default() -> Self {
        Self {
            data: vec![],
            version: 1,
            change: Arc::new(Notify::new()),
        }
    }
}
impl InternalRowVal {
    fn concurrent_values(&self) -> Vec<Alternative> {
        self.data.iter().map(InternalData::to_alternative).collect()
    }

    fn to_row_val(&self, row_ref: RowRef) -> RowVal {
        RowVal {
            row_ref: row_ref.with_causality(self.version.to_string()),
            value: self.concurrent_values(),
        }
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

type ArcRow = Arc<RwLock<HashMap<String, BTreeMap<String, InternalRowVal>>>>;
type ArcBlob = Arc<RwLock<BTreeMap<String, InternalBlobVal>>>;

#[derive(Clone, Debug)]
pub struct MemBuilder {
    unicity: Vec<u8>,
    row: ArcRow,
    blob: ArcBlob,
}

impl MemBuilder {
    pub fn new(user: &str) -> Arc<Self> {
        tracing::debug!("initialize membuilder for {}", user);
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.extend_from_slice(user.as_bytes());
        Arc::new(Self {
            unicity,
            row: Arc::new(RwLock::new(HashMap::new())),
            blob: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }
}

#[async_trait]
impl IBuilder for MemBuilder {
    async fn build(&self) -> Result<Store, StorageError> {
        Ok(Box::new(MemStore {
            row: self.row.clone(),
            blob: self.blob.clone(),
        }))
    }

    fn unique(&self) -> UnicityBuffer {
        UnicityBuffer(self.unicity.clone())
    }
}

pub struct MemStore {
    row: ArcRow,
    blob: ArcBlob,
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

impl MemStore {
    fn row_rm_single(&self, entry: &RowRef) -> Result<(), StorageError> {
        tracing::trace!(entry=%entry, command="row_rm_single");
        let mut store = self.row.write().or(Err(StorageError::Internal))?;
        let shard = &entry.uid.shard;
        let sort = &entry.uid.sort;

        let cauz = match entry.causality.as_ref().map(|v| v.parse::<u64>()) {
            Some(Ok(v)) => v,
            _ => 0,
        };

        let bt = store.entry(shard.to_string()).or_default();
        let intval = bt.entry(sort.to_string()).or_default();

        if cauz == intval.version {
            intval.data.clear();
        }
        intval.data.push(InternalData::Tombstone);
        intval.version += 1;
        intval.change.notify_waiters();

        Ok(())
    }
}

#[async_trait]
impl IStore for MemStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError> {
        tracing::trace!(select=%select, command="row_fetch");
        let store = self.row.read().or(Err(StorageError::Internal))?;
        let vals = select_keys(&store, select)
            .into_iter()
            .map(|row_ref| {
                let v = store
                    .get(&row_ref.uid.shard)
                    .unwrap()
                    .get(&row_ref.uid.sort)
                    .unwrap();
                v.to_row_val(row_ref)
            })
            .collect::<Vec<_>>();

        if let Selector::Single(_) = select {
            if vals.is_empty() {
                return Err(StorageError::NotFound)
            }
        }

        Ok(vals)
    }

    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        tracing::trace!(select=%select, command="row_rm");
        let refs = {
            let store = self.row.read().or(Err(StorageError::Internal))?;
            select_keys(&store, select)
        };
        for rref in refs {
            self.row_rm_single(&rref)?;
        }
        Ok(())
    }

    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        tracing::trace!(entries=%values.iter().map(|v| v.row_ref.to_string()).collect::<Vec<_>>().join(","), command="row_insert");
        let mut store = self.row.write().or(Err(StorageError::Internal))?;
        for v in values.into_iter() {
            let shard = v.row_ref.uid.shard;
            let sort = v.row_ref.uid.sort;

            let val = match v.value.into_iter().next() {
                Some(Alternative::Value(x)) => x,
                _ => vec![],
            };

            let cauz = match v.row_ref.causality.map(|v| v.parse::<u64>()) {
                Some(Ok(v)) => v,
                _ => 0,
            };

            let bt = store.entry(shard).or_default();
            let intval = bt.entry(sort).or_default();

            if cauz == intval.version {
                intval.data.clear();
            }
            intval.data.push(InternalData::Value(val));
            intval.version += 1;
            intval.change.notify_waiters();
        }
        Ok(())
    }
    async fn row_poll(&self, value: &RowRef) -> Result<RowVal, StorageError> {
        tracing::trace!(entry=%value, command="row_poll");
        let shard = &value.uid.shard;
        let sort = &value.uid.sort;
        let cauz = match value.causality.as_ref().map(|v| v.parse::<u64>()) {
            Some(Ok(v)) => v,
            _ => 0,
        };

        let notify_me = {
            let mut store = self.row.write().or(Err(StorageError::Internal))?;
            let bt = store.entry(shard.to_string()).or_default();
            let intval = bt.entry(sort.to_string()).or_default();

            if intval.version != cauz {
                return Ok(intval.to_row_val(value.clone()));
            }
            intval.change.clone()
        };

        notify_me.notified().await;

        let res = self.row_fetch(&Selector::Single(value)).await?;
        res.into_iter().next().ok_or(StorageError::NotFound)
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
}

/// Returns keys of `store` that are selected by `select`. The returned RowRefs
/// are guaranteed to be valid keys of `store`.
fn select_keys<'a, 'b, V>(store: &'b HashMap<String, BTreeMap<String, V>>, select: &'b Selector<'a>) ->
    Vec<RowRef>
{
    match select {
        Selector::Range {
            shard,
            sort_begin,
            sort_end,
        } =>
            store
               .get(*shard)
               .map(|bt|
                    bt
                    .range((
                        sort_begin.map(|b| Included(b.to_string())).unwrap_or(Unbounded),
                        sort_end.map(|e| Excluded(e.to_string())).unwrap_or(Unbounded),
                    ))
                    .map(|(key, _)| RowRef::new(shard, key))
                    .collect())
               .unwrap_or(vec![]),
        Selector::List(rlist) =>
            rlist
            .iter()
            .filter_map(|row_ref| {
                let bt = store.get(&row_ref.uid.shard)?;
                let _ = bt.get(&row_ref.uid.sort)?;
                Some(row_ref.clone())
            })
            .collect::<Vec<_>>(),
        Selector::Prefix { shard, sort_prefix } => {
            let last_bound = prefix_last_bound(sort_prefix);
            store
                .get(*shard)
                .map(|bt|
                     bt
                     .range((Included(sort_prefix.to_string()), last_bound))
                     .map(|(key, _)| RowRef::new(shard, key))
                     .collect())
                .unwrap_or(vec![])
        }
        Selector::Single(row_ref) =>
            store
                .get(&row_ref.uid.shard)
                .and_then(|bt| bt.get(&row_ref.uid.sort))
                .map(|_| vec![(*row_ref).clone()])
                .unwrap_or(vec![]),
    }
}
