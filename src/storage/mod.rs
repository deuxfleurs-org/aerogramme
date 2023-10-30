pub trait RowStore<K: RowRef, E> {
    fn new_row_ref(partition: &str, sort: &str) -> K<E>;
}

pub trait RowRef {
    fn set_value(&self, content: &[u8]) -> RowValue;
    async fn get(&self) -> Result<RowValue, E>;
    async fn rm(&self) -> Result<(), E>;
    async fn obs(&self) -> Result<Option<RowValue>, ()>;
}

pub trait RowValue {
    fn row_ref(&self) -> RowRef;
    fn content(&self) -> Vec<u8>;
    async fn put(&self) -> Result<(), E>;
}

/*
    async fn get_many_keys(&self, keys: &[K]) -> Result<Vec<V>, ()>;
    async fn put_many_keys(&self, values: &[V]) -> Result<(), ()>;
}*/

pub trait BlobStore {
    fn new_blob_ref(key: &str) -> BlobRef;
    async fn list(&self) -> ();
}

pub trait BlobRef {
    async fn put(&self, key: &str, body: &[u8]) -> ();
    async fn copy(&self, dst: &BlobRef) -> ();
    async fn rm(&self, key: &str);
}
