use crate::storage::*;
use std::collections::{HashMap, BTreeMap};
use std::ops::Bound::{Included, Unbounded, Excluded};
use std::sync::{Arc, RwLock};

/// This implementation is very inneficient, and not completely correct
/// Indeed, when the connector is dropped, the memory is freed.
/// It means that when a user disconnects, its data are lost.
/// It's intended only for basic debugging, do not use it for advanced tests...

pub type ArcRow = Arc<RwLock<HashMap<String, BTreeMap<String, Vec<u8>>>>>;
pub type ArcBlob = Arc<RwLock<HashMap<String, Vec<u8>>>>;

#[derive(Clone, Debug)]
pub struct MemBuilder {
    user: String,
    url: String,
    row: ArcRow,
    blob: ArcBlob,
}

impl IBuilder for MemBuilder {
    fn build(&self) -> Box<dyn IStore> {
        Box::new(MemStore {
            row: self.row.clone(),
            blob: self.blob.clone(),
        })
    } 
}

pub struct MemStore {
    row: ArcRow,
    blob: ArcBlob,
}

impl MemStore {
    fn inner_fetch(&self, row_ref: &RowRef) -> Result<Vec<u8>, StorageError> {
        Ok(self.row
            .read()
            .or(Err(StorageError::Internal))?
            .get(&row_ref.uid.shard)
            .ok_or(StorageError::NotFound)?
            .get(&row_ref.uid.sort)
            .ok_or(StorageError::NotFound)?
            .clone())
    }
}

#[async_trait]
impl IStore for MemStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError> {
        match select {
            Selector::Range { shard, sort_begin, sort_end } => {
                Ok(self.row
                    .read()
                    .or(Err(StorageError::Internal))?
                    .get(*shard)
                    .ok_or(StorageError::NotFound)?
                    .range((Included(sort_begin.to_string()), Included(sort_end.to_string())))
                    .map(|(k, v)| RowVal {
                        row_ref: RowRef { uid: RowUid { shard: shard.to_string(), sort: k.to_string() }, causality: Some("c".to_string()) },
                        value: vec![Alternative::Value(v.clone())],
                    })
                    .collect::<Vec<_>>())
            },
            Selector::List(rlist) => {
                let mut acc = vec![];
                for row_ref in rlist {
                    let bytes = self.inner_fetch(row_ref)?;
                    let row_val = RowVal { 
                        row_ref: row_ref.clone(), 
                        value: vec![Alternative::Value(bytes)] 
                    };
                    acc.push(row_val);
                }
                Ok(acc)
            },
            Selector::Prefix { shard, sort_prefix } => {
                let mut sort_end = sort_prefix.to_string();
                let last_bound = match sort_end.pop() {
                    None => Unbounded,
                    Some(ch) => {
                        let nc = char::from_u32(ch as u32 + 1).unwrap();
                        sort_end.push(nc);
                        Excluded(sort_end)
                    }
                };

                Ok(self.row
                    .read()
                    .or(Err(StorageError::Internal))?
                    .get(*shard)
                    .ok_or(StorageError::NotFound)?
                    .range((Included(sort_prefix.to_string()), last_bound))
                    .map(|(k, v)| RowVal {
                        row_ref: RowRef { uid: RowUid { shard: shard.to_string(), sort: k.to_string() }, causality: Some("c".to_string()) },
                        value: vec![Alternative::Value(v.clone())],
                    })
                    .collect::<Vec<_>>())
            },
            Selector::Single(row_ref) => {
                let bytes = self.inner_fetch(row_ref)?;
                Ok(vec![RowVal{ row_ref: row_ref.clone(), value: vec![Alternative::Value(bytes)]}])
            }
        }
    }

    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        unimplemented!();
    }

    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        unimplemented!();

    }
    async fn row_poll(&self, value: RowRef) -> Result<RowVal, StorageError> {
        unimplemented!();
    }

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError> {
        unimplemented!();

    }
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<BlobVal, StorageError> {
        unimplemented!();

    }
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError> {
        unimplemented!();
    }
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError> {
        unimplemented!();
    }
}
