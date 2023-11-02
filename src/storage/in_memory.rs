use futures::FutureExt;
use crate::storage::*;

#[derive(Clone, Debug)]
pub struct MemCreds {}
pub struct MemStore {}
pub struct MemRef {}
pub struct MemValue {}

impl IRowBuilder for MemCreds {
    fn row_store(&self) -> RowStore {
        unimplemented!();
    }
}

impl IRowStore for MemStore {
    fn new_row(&self, partition: &str, sort: &str) -> RowRef {
        unimplemented!();
    }
}

impl IRowRef for MemRef {
    fn set_value(&self, content: Vec<u8>) -> RowValue {
        unimplemented!();
    }
    fn fetch(&self) -> AsyncResult<RowValue> {
        unimplemented!();
    }
    fn rm(&self) -> AsyncResult<()> {
        unimplemented!();
    }
    fn poll(&self) -> AsyncResult<Option<RowValue>> {
        async {
            Ok(None)
        }.boxed()
    }
}

impl IRowValue for MemValue {
    fn to_ref(&self) -> RowRef {
        unimplemented!();
    }
    fn content(&self) -> ConcurrentValues {
        unimplemented!();
    }
    fn push(&self) -> AsyncResult<()> {
        unimplemented!();
    }
}


