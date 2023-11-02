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

// ------ Row Builder
pub trait IRowBuilder
{
    fn row_store(&self) -> RowStore;
}
pub type RowBuilder = Box<dyn IRowBuilder>;

// ------ Row Store
pub trait IRowStore
{
    fn new_row(&self, partition: &str, sort: &str) -> RowRef;
}
type RowStore = Box<dyn IRowStore>;

// ------- Row Item
pub trait IRowRef 
{
    fn set_value(&self, content: Vec<u8>) -> RowValue;
    async fn fetch(&self) -> Result<RowValue, Error>;
    async fn rm(&self) -> Result<(), Error>;
    async fn poll(&self) -> Result<Option<RowValue>, Error>;
}
type RowRef = Box<dyn IRowRef>;

pub trait IRowValue
{
    fn to_ref(&self) -> RowRef;
    fn content(&self) -> ConcurrentValues;
    async fn push(&self) -> Result<(), Error>;
}
type RowValue = Box<dyn IRowValue>;
