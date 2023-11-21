use futures::FutureExt;
use crate::storage::*;

#[derive(Clone, Debug, Hash)]
pub struct FullMem {}
pub struct MemStore {}
pub struct MemRef {}
pub struct MemValue {}

#[derive(Clone, Debug, PartialEq)]
pub struct MemOrphanRowRef {}

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

    fn select(&self, selector: Selector) -> AsyncResult<Vec<RowValue>> {
        unimplemented!()
    }

    fn rm(&self, selector: Selector) -> AsyncResult<()> {
        unimplemented!();
    }

    fn from_orphan(&self, orphan: OrphanRowRef) -> Result<RowRef, StorageError> {
        unimplemented!();
    }
}

impl IRowRef for MemRef {
    fn to_orphan(&self) -> OrphanRowRef {
        unimplemented!()
    }

    fn key(&self) -> (&str, &str) {
        unimplemented!();
    }

    /*fn clone_boxed(&self) -> RowRef {
        unimplemented!();
    }*/

    fn set_value(&self, content: &[u8]) -> RowValue {
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

impl std::fmt::Debug for MemRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!();
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

impl std::fmt::Debug for MemValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!();
    }
}
