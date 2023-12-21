/*
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 *
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 */

pub mod in_memory;
pub mod garage;

use std::sync::Arc;
use std::hash::Hash;
use std::collections::HashMap;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum Alternative {
    Tombstone,
    Value(Vec<u8>),
}
type ConcurrentValues = Vec<Alternative>;

#[derive(Debug, Clone)]
pub enum StorageError {
    NotFound,
    Internal,
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage Error: ")?;
        match self {
            Self::NotFound => f.write_str("Item not found"),
            Self::Internal => f.write_str("An internal error occured"),
        }
    }
}
impl std::error::Error for StorageError {}

#[derive(Debug, Clone, PartialEq)]
pub struct RowUid {
    pub shard: String,
    pub sort: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowRef {
    pub uid: RowUid,
    pub causality: Option<String>,
}
impl std::fmt::Display for RowRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RowRef({}, {}, {:?})", self.uid.shard, self.uid.sort, self.causality)
    }
}

impl RowRef {
    pub fn new(shard: &str, sort: &str) -> Self {
        Self {
            uid: RowUid { 
                shard: shard.to_string(),
                sort: sort.to_string(),
            },
            causality: None,
        }
    }
    pub fn with_causality(mut self, causality: String) -> Self {
        self.causality = Some(causality);
        self
    }
}

#[derive(Debug, Clone)]
pub struct RowVal {
    pub row_ref: RowRef,
    pub value: ConcurrentValues,
}

impl RowVal {
    pub fn new(row_ref: RowRef, value: Vec<u8>) -> Self {
        Self {
            row_ref,
            value: vec![Alternative::Value(value)],
        }
    }
}


#[derive(Debug, Clone)]
pub struct BlobRef(pub String);
impl BlobRef {
    pub fn new(key: &str) -> Self {
        Self(key.to_string())
    }
}
impl std::fmt::Display for BlobRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlobRef({})", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct BlobVal {
    pub blob_ref: BlobRef,
    pub meta: HashMap<String, String>,
    pub value: Vec<u8>,
}
impl BlobVal {
    pub fn new(blob_ref: BlobRef, value: Vec<u8>) -> Self {
        Self {
            blob_ref, value,
            meta: HashMap::new(),
        }
    }

    pub fn with_meta(mut self, k: String, v: String) -> Self {
        self.meta.insert(k, v);
        self
    }
}

#[derive(Debug)]
pub enum Selector<'a> {
    Range { shard: &'a str, sort_begin: &'a str, sort_end: &'a str },
    List (Vec<RowRef>), // list of (shard_key, sort_key)
    Prefix { shard: &'a str, sort_prefix: &'a str },
    Single(&'a RowRef),
}
impl<'a> std::fmt::Display for Selector<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Range { shard, sort_begin, sort_end } => write!(f, "Range({}, [{}, {}[)", shard, sort_begin, sort_end),
            Self::List(list) => write!(f, "List({:?})", list),
            Self::Prefix { shard, sort_prefix } => write!(f, "Prefix({}, {})", shard, sort_prefix),
            Self::Single(row_ref) => write!(f, "Single({})", row_ref),
        }
    }
}

#[async_trait]
pub trait IStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError>;
    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError>;
    async fn row_rm_single(&self, entry: &RowRef) -> Result<(), StorageError>;
    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError>;
    async fn row_poll(&self, value: &RowRef) -> Result<RowVal, StorageError>;

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError>;
    async fn blob_insert(&self, blob_val: &BlobVal) -> Result<(), StorageError>;
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError>;
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError>;
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError>;
}

#[derive(Clone,Debug,PartialEq,Eq,Hash)]
pub struct UnicityBuffer(Vec<u8>);

#[async_trait]
pub trait IBuilder: std::fmt::Debug {
    async fn build(&self) -> Result<Store, StorageError>;

    /// Returns an opaque buffer that uniquely identifies this builder
    fn unique(&self) -> UnicityBuffer;
}

pub type Builder = Arc<dyn IBuilder + Send + Sync>;
pub type Store = Box<dyn IStore + Send + Sync>;
