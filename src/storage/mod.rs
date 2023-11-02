/*
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 *
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 */

use futures::future::BoxFuture;

pub mod in_memory;
pub mod garage;

pub enum Selector<'a> {
    Range{ begin: &'a str, end: &'a str },
    Filter(u64),
}

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
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Storage Error: ");
        match self {
            Self::NotFound => f.write_str("Item not found"),
            Self::Internal => f.write_str("An internal error occured"),
        }
    }
}
impl std::error::Error for StorageError {}

pub struct Engine {
    pub bucket: String,
    pub builders: Builder,
}
impl Clone for Engine {
    fn clone(&self) -> Self {
        Engine {
            bucket: "test".into(),
            builders: Box::new(in_memory::FullMem{})
        }
    }
}
impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("bucket", &self.bucket).finish()
    }
}

// Utils
pub type AsyncResult<'a, T> = BoxFuture<'a, Result<T, StorageError>>;

pub trait IBuilder {
    fn row_store(&self) -> Result<RowStore, StorageError>;
    fn blob_store(&self) -> Result<BlobStore, StorageError>;
}
pub type Builder = Box<dyn IBuilder + Send + Sync>;

// ------ Row 
pub trait IRowStore
{
    fn new_row(&self, partition: &str, sort: &str) -> RowRef;
}
pub type RowStore = Box<dyn IRowStore + Sync + Send>;

pub trait IRowRef 
{
    fn set_value(&self, content: Vec<u8>) -> RowValue;
    fn fetch(&self) -> AsyncResult<RowValue>;
    fn rm(&self) -> AsyncResult<()>;
    fn poll(&self) -> AsyncResult<Option<RowValue>>;
}
pub type RowRef = Box<dyn IRowRef>;

pub trait IRowValue
{
    fn to_ref(&self) -> RowRef;
    fn content(&self) -> ConcurrentValues;
    fn push(&self) -> AsyncResult<()>;
}
pub type RowValue = Box<dyn IRowValue>;

// ------- Blob 
pub trait IBlobStore
{
    fn new_blob(&self, key: &str) -> BlobRef;
    fn list(&self) -> AsyncResult<Vec<BlobRef>>;
}
pub type BlobStore = Box<dyn IBlobStore + Send + Sync>;

pub trait IBlobRef
{
    fn set_value(&self, content: Vec<u8>) -> BlobValue;
    fn fetch(&self) -> AsyncResult<BlobValue>;
    fn copy(&self, dst: &BlobRef) -> AsyncResult<()>;
    fn rm(&self) -> AsyncResult<()>;
}
pub type BlobRef = Box<dyn IBlobRef>;

pub trait IBlobValue {
    fn to_ref(&self) -> BlobRef;
    fn push(&self) -> AsyncResult<()>;
}
pub type BlobValue = Box<dyn IBlobValue>;
