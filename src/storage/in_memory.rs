use futures::FutureExt;
use crate::storage::*;

#[derive(Clone, Debug, Hash)]
pub struct FullMem {}
pub struct MemStore {}
pub struct MemRef {}
pub struct MemValue {}

impl IBuilders for FullMem {
    fn row_store(&self) -> Result<RowStore, StorageError> {
        unimplemented!();
    }

    fn blob_store(&self) -> Result<BlobStore, StorageError> {
        unimplemented!();
    }

    fn url(&self) -> &str {
        return "mem://unimplemented;"
    }
}

impl IRowStore for MemStore {
    fn row(&self, partition: &str, sort: &str) -> RowRef {
        unimplemented!();
    }
}

impl IRowRef for MemRef {
    fn clone_boxed(&self) -> RowRef {
        unimplemented!();
    }

    fn set_value(&self, content: Vec<u8>) -> RowValue {
        unimplemented!();
    }
    fn fetch(&self) -> AsyncResult<RowValue> {
        unimplemented!();
    }
    fn rm(&self) -> AsyncResult<()> {
        unimplemented!();
    }
    fn poll(&self) -> AsyncResult<RowValue> {
        async {
            let rv: RowValue = Box::new(MemValue{});
            Ok(rv)
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


