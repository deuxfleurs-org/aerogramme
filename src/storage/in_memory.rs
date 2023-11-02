use crate::storage::*;

pub struct MemCreds {}
pub struct MemStore {}
pub struct MemRef {}
pub struct MemValue {}

impl IRowBuilder for MemCreds {
    fn row_store(&self) -> MemStore {
        unimplemented!();
    }
}

impl IRowStore for MemStore {
    fn new_row(&self, partition: &str, sort: &str) -> MemRef {
        unimplemented!();
    }
}

impl IRowRef for MemRef {
    fn set_value(&self, content: Vec<u8>) -> MemValue {
        unimplemented!();
    }
    async fn fetch(&self) -> Result<MemValue, Error> {
        unimplemented!();
    }
    async fn rm(&self) -> Result<(), Error> {
        unimplemented!();
    }
    async fn poll(&self) -> Result<Option<MemValue>, Error> {
        unimplemented!();
    }
}

impl IRowValue for MemValue {
    fn to_ref(&self) -> MemRef {
        unimplemented!();
    }
    fn content(&self) -> ConcurrentValues {
        unimplemented!();
    }
    async fn push(&self) -> Result<(), Error> {
        unimplemented!();
    }
}


