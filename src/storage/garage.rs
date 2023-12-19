use crate::storage::*;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct GarageConf {
    pub region: String,
    pub s3_endpoint: String,
    pub k2v_endpoint: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

#[derive(Clone, Debug)]
pub struct GarageBuilder {
    conf: GarageConf,
    unicity: Vec<u8>,
}

impl GarageBuilder {
    pub fn new(conf: GarageConf) -> anyhow::Result<Arc<Self>> {
        let mut unicity: Vec<u8> = vec![];
        unicity.extend_from_slice(file!().as_bytes());
        unicity.append(&mut rmp_serde::to_vec(&conf)?);
        Ok(Arc::new(Self { conf, unicity }))
    } 
}

impl IBuilder for GarageBuilder {
    fn build(&self) -> Result<Store, StorageError> {
        unimplemented!();
    }
    fn unique(&self) -> UnicityBuffer {
        UnicityBuffer(self.unicity.clone())
    }
}

pub struct GarageStore {
    dummy: String,
}

#[async_trait]
impl IStore for GarageStore {
    async fn row_fetch<'a>(&self, select: &Selector<'a>) -> Result<Vec<RowVal>, StorageError> {
        unimplemented!();
    }
    async fn row_rm<'a>(&self, select: &Selector<'a>) -> Result<(), StorageError> {
        unimplemented!();
    }

    async fn row_insert(&self, values: Vec<RowVal>) -> Result<(), StorageError> {
        unimplemented!();

    }
    async fn row_poll(&self, value: &RowRef) -> Result<RowVal, StorageError> {
        unimplemented!();
    }

    async fn row_rm_single(&self, entry: &RowRef) -> Result<(), StorageError> {
        unimplemented!();
    }

    async fn blob_fetch(&self, blob_ref: &BlobRef) -> Result<BlobVal, StorageError> {
        unimplemented!();

    }
    async fn blob_insert(&self, blob_val: &BlobVal) -> Result<(), StorageError> {
        unimplemented!();
    }
    async fn blob_copy(&self, src: &BlobRef, dst: &BlobRef) -> Result<(), StorageError> {
        unimplemented!();

    }
    async fn blob_list(&self, prefix: &str) -> Result<Vec<BlobRef>, StorageError> {
        unimplemented!();
    }
    async fn blob_rm(&self, blob_ref: &BlobRef) -> Result<(), StorageError> {
        unimplemented!();
    }
}

