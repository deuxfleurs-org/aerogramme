/*
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 *
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 */

use std::hash::{Hash, Hasher};
use futures::future::BoxFuture;

pub mod in_memory;
pub mod garage;

pub enum Alternative {
    Tombstone,
    Value(Vec<u8>),
}
type ConcurrentValues = Vec<Alternative>;

#[derive(Clone, Debug, PartialEq)]
pub enum OrphanRowRef {
    Garage(garage::GrgOrphanRowRef),
    Memory(in_memory::MemOrphanRowRef),
}

pub enum Selector<'a> {
    Range { shard_key: &'a str, begin: &'a str, end: &'a str },
    List (Vec<(&'a str, &'a str)>), // list of (shard_key, sort_key)
    Prefix { shard_key: &'a str, prefix: &'a str },
}

#[derive(Debug)]
pub enum StorageError {
    NotFound,
    Internal,
    IncompatibleOrphan,
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage Error: ");
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
    fn row_store(&self) -> Result<RowStore, StorageError>;
    fn blob_store(&self) -> Result<BlobStore, StorageError>;
    fn url(&self) -> &str;
}
pub type Builders = Box<dyn IBuilders + Send + Sync>;
impl Clone for Builders {
    fn clone(&self) -> Self {
        // @FIXME write a real implementation with a box_clone function
        Box::new(in_memory::FullMem{})
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
    /*fn clone_boxed(&self) -> RowRef;*/
    fn to_orphan(&self) -> OrphanRowRef;
    fn key(&self) -> (&str, &str);
    fn set_value(&self, content: Vec<u8>) -> RowValue;
    fn fetch(&self) -> AsyncResult<RowValue>;
    fn rm(&self) -> AsyncResult<()>;
    fn poll(&self) -> AsyncResult<RowValue>;
}
pub type RowRef = Box<dyn IRowRef + Send + Sync>;
/*impl Clone for RowRef {
    fn clone(&self) -> Self {
        return self.clone_boxed()
    }
}*/


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
