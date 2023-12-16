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

use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use futures::future::BoxFuture;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum Alternative {
    Tombstone,
    Value(Vec<u8>),
}
type ConcurrentValues = Vec<Alternative>;

#[derive(Debug)]
pub enum StorageError {
    NotFound,
    Internal,
}

#[derive(Debug, Clone)]
pub struct RowUid {
    shard: String,
    sort: String,
}

#[derive(Debug, Clone)]
pub struct RowRef {
    uid: RowUid,
    causality: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RowVal {
    row_ref: RowRef,
    value: ConcurrentValues,
}

#[derive(Debug, Clone)]
pub struct BlobRef(String);

#[derive(Debug, Clone)]
pub struct BlobVal {
    blob_ref: BlobRef,
    meta: HashMap<String, String>,
    value: Vec<u8>,
}

pub enum Selector<'a> {
    Range { shard: &'a str, sort_begin: &'a str, sort_end: &'a str },
    List (Vec<RowRef>), // list of (shard_key, sort_key)
    Prefix { shard: &'a str, sort_prefix: &'a str },
    Single(RowRef),
}

#[async_trait]
pub trait IStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError>;
    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError>;
    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError>;
    async fn row_poll(&self, value: RowRef) -> Result<RowVal, StorageError>;

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError>;
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<BlobVal, StorageError>;
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError>;
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError>;
}

pub trait IBuilder {
    fn build(&self) -> Box<dyn IStore>;
}






/*
#[derive(Clone, Debug, PartialEq)]
pub enum OrphanRowRef {
    Garage(garage::GrgOrphanRowRef),
    Memory(in_memory::MemOrphanRowRef),
}




impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage Error: ")?;
        match self {
            Self::NotFound => f.write_str("Item not found"),
            Self::Internal => f.write_str("An internal error occured"),
            Self::IncompatibleOrphan => f.write_str("Incompatible orphan"),
        }
    }
}
impl std::error::Error for StorageError {}

// Utils
pub type AsyncResult<'a, T> = BoxFuture<'a, Result<T, StorageError>>;

// ----- Builders
pub trait IBuilders {
    fn box_clone(&self) -> Builders;
    fn row_store(&self) -> Result<RowStore, StorageError>;
    fn blob_store(&self) -> Result<BlobStore, StorageError>;
    fn url(&self) -> &str;
}
pub type Builders = Box<dyn IBuilders + Send + Sync>;
impl Clone for Builders {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
impl std::fmt::Debug for Builders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("aerogramme::storage::Builder")
    }
}
impl PartialEq for Builders {
    fn eq(&self, other: &Self) -> bool {
        self.url() == other.url()
    }
}
impl Eq for Builders {}
impl Hash for Builders {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.url().hash(state);
    }
}

// ------ Row 
pub trait IRowStore
{
    fn row(&self, partition: &str, sort: &str) -> RowRef;
    fn select(&self, selector: Selector) -> AsyncResult<Vec<RowValue>>;
    fn rm(&self, selector: Selector) -> AsyncResult<()>;
    fn from_orphan(&self, orphan: OrphanRowRef) -> Result<RowRef, StorageError>;
}
pub type RowStore = Box<dyn IRowStore + Sync + Send>;

pub trait IRowRef: std::fmt::Debug
{
    fn to_orphan(&self) -> OrphanRowRef;
    fn key(&self) -> (&str, &str);
    fn set_value(&self, content: &[u8]) -> RowValue;
    fn fetch(&self) -> AsyncResult<RowValue>;
    fn rm(&self) -> AsyncResult<()>;
    fn poll(&self) -> AsyncResult<RowValue>;
}
pub type RowRef<'a> = Box<dyn IRowRef + Send + Sync + 'a>;

pub trait IRowValue: std::fmt::Debug
{
    fn to_ref(&self) -> RowRef;
    fn content(&self) -> ConcurrentValues;
    fn push(&self) -> AsyncResult<()>;
}
pub type RowValue = Box<dyn IRowValue + Send + Sync>;

// ------- Blob 
pub trait IBlobStore
{
    fn blob(&self, key: &str) -> BlobRef;
    fn list(&self, prefix: &str) -> AsyncResult<Vec<BlobRef>>;
}
pub type BlobStore = Box<dyn IBlobStore + Send + Sync>;

pub trait IBlobRef
{
    fn set_value(&self, content: Vec<u8>) -> BlobValue;
    fn key(&self) -> &str;
    fn fetch(&self) -> AsyncResult<BlobValue>;
    fn copy(&self, dst: &BlobRef) -> AsyncResult<()>;
    fn rm(&self) -> AsyncResult<()>;
}
pub type BlobRef = Box<dyn IBlobRef + Send + Sync>;

pub trait IBlobValue {
    fn to_ref(&self) -> BlobRef;
    fn get_meta(&self, key: &str) -> Option<&[u8]>;
    fn set_meta(&mut self, key: &str, val: &str);
    fn content(&self) -> Option<&[u8]>;
    fn push(&self) -> AsyncResult<()>;
}
pub type BlobValue = Box<dyn IBlobValue + Send + Sync>;
*/
