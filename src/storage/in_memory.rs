use crate::storage::*;

pub struct MemCreds {}
pub struct MemStore {}
pub struct MemRef {}
pub struct MemValue {}

pub struct MemTypes {}
impl RowRealization for MemTypes {
    type Store=MemStore;
    type Ref=MemRef;
    type Value=MemValue;
}

impl RowBuilder<MemTypes> for MemCreds {
    fn row_store(&self) -> MemStore {
        unimplemented!();
    }
}

impl RowStore<MemTypes> for MemStore {
    fn new_row(&self, partition: &str, sort: &str) -> MemRef {
        unimplemented!();
    }
}

impl RowRef<MemTypes> for MemRef {
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

impl RowValue<MemTypes> for MemValue {
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


