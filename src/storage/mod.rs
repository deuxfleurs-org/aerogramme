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
pub enum Error {
    NotFound,
    Internal,
}

pub struct Engine {
    pub bucket: String,
    pub row: RowBuilder,
}
impl Clone for Engine {
    fn clone(&self) -> Self {
        Engine {
            bucket: "test".into(),
            row: Box::new(in_memory::MemCreds{})
        }
    }
}
impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("bucket", &self.bucket).finish()
    }
}

// A result
pub type AsyncResult<'a, T> = BoxFuture<'a, Result<T, Error>>;

// ------ Row Builder
pub trait IRowBuilder
{
    fn row_store(&self) -> Result<RowStore, Error>;
}
pub type RowBuilder = Box<dyn IRowBuilder + Send + Sync>;

// ------ Row Store
pub trait IRowStore
{
    fn new_row(&self, partition: &str, sort: &str) -> RowRef;
}
pub type RowStore = Box<dyn IRowStore>;

// ------- Row Item
pub trait IRowRef 
{
    fn set_value(&self, content: Vec<u8>) -> RowValue;
    fn fetch(&self) -> AsyncResult<RowValue>;
    fn rm(&self) -> AsyncResult<()>;
    fn poll(&self) -> AsyncResult<Option<RowValue>>;
}
type RowRef = Box<dyn IRowRef>;

pub trait IRowValue
{
    fn to_ref(&self) -> RowRef;
    fn content(&self) -> ConcurrentValues;
    fn push(&self) -> AsyncResult<()>;
}
type RowValue = Box<dyn IRowValue>;
