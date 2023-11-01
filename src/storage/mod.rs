/*
 * T1 : Filter
 * T2 : Range
 * T3 : Atom
 */

/*
 * My idea: we can encapsulate the causality token
 * into the object system so it is not exposed.
 *
 * This abstraction goal is to leverage all the semantic of Garage K2V+S3,
 * to be as tailored as possible to it ; it aims to be a zero-cost abstraction
 * compared to when we where directly using the K2V+S3 client.
 */


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

pub enum Error {
    NotFound,
    Internal,
}

// ------ Store
pub trait RowStore {
    fn new_row(&self, partition: &str, sort: &str) -> impl RowRef;
    fn new_row_batch(&self, partition: &str, filter: Selector) -> impl RowRefBatch;
    fn new_blob(&self, key: &str) -> impl BlobRef;
    fn new_blob_list(&self) -> Vec<impl BlobRef>; 
}

// ------- Row
pub trait RowRef {
    fn to_value(&self, content: &[u8]) -> impl RowValue;
    async fn get(&self) -> Result<impl RowValue, Error>;
    async fn rm(&self) -> Result<(), Error>;
    async fn poll(&self) -> Result<Option<impl RowValue>, Error>;
}

pub trait RowValue {
    fn row_ref(&self) -> impl RowRef;
    fn content(&self) -> ConcurrentValues;
    async fn persist(&self) -> Result<(), Error>;
}

// ------ Row batch
pub trait RowRefBatch {
    fn to_values(&self, content: Vec<&[u8]>) -> impl RowValueBatch;
    fn into_independant(&self) -> Vec<impl RowRef>;
    async fn get(&self) -> Result<impl RowValueBatch, Error>;
    async fn rm(&self) -> Result<(), Error>;
}

pub trait RowValueBatch {
    fn into_independant(&self) -> Vec<impl RowValue>;
    fn content(&self) -> Vec<ConcurrentValues>;
    async fn persist(&self) -> Result<(), Error>;
}

// ----- Blobs
pub trait BlobRef {
    fn set_value(&self, content: &[u8]) -> impl BlobValue;
    async fn get(&self) -> impl BlobValue;
    async fn copy(&self, dst: &impl BlobRef) -> ();
    async fn rm(&self, key: &str) -> ();
}

pub trait BlobValue {
    async fn persist();
}
