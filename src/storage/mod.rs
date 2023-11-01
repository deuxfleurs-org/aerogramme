/*
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 *
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 */

mod in_memory;
mod garage;

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

pub trait RowRealization: Sized {
    type Store: RowStore<Self>;
    type Ref: RowRef<Self>;
    type Value: RowValue<Self>;
}

// ------ Row Builder
pub trait RowBuilder<R: RowRealization> 
{
    fn row_store(&self) -> R::Store;
}

// ------ Row Store
pub trait RowStore<R: RowRealization> 
{
    fn new_row(&self, partition: &str, sort: &str) -> R::Ref;
}

// ------- Row Item
pub trait RowRef<R: RowRealization> 
{
    fn set_value(&self, content: Vec<u8>) -> R::Value;
    async fn fetch(&self) -> Result<R::Value, Error>;
    async fn rm(&self) -> Result<(), Error>;
    async fn poll(&self) -> Result<Option<R::Value>, Error>;
}

pub trait RowValue<R: RowRealization> 
{
    fn to_ref(&self) -> R::Ref;
    fn content(&self) -> ConcurrentValues;
    async fn push(&self) -> Result<(), Error>;
}
