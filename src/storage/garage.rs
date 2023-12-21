use crate::storage::*;
use serde::Serialize;
use aws_sdk_s3::{
    self as s3,
    error::SdkError,
    operation::get_object::GetObjectError,
};

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

#[async_trait]
impl IBuilder for GarageBuilder {
    async fn build(&self) -> Result<Store, StorageError> {
        let creds = s3::config::Credentials::new(
            self.conf.aws_access_key_id.clone(), 
            self.conf.aws_secret_access_key.clone(), 
            None, 
            None, 
            "aerogramme"
        );

        let config = aws_config::from_env()
            .region(aws_config::Region::new(self.conf.region.clone()))
            .credentials_provider(creds)
            .endpoint_url(self.conf.s3_endpoint.clone())
            .load()
            .await;

        let s3_client = aws_sdk_s3::Client::new(&config);
        Ok(Box::new(GarageStore { 
            s3_bucket: self.conf.bucket.clone(),
            s3: s3_client 
        }))
    }
    fn unique(&self) -> UnicityBuffer {
        UnicityBuffer(self.unicity.clone())
    }
}

pub struct GarageStore {
    s3_bucket: String,
    s3: s3::Client,
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
        let maybe_out =  self.s3
            .get_object()
            .bucket(self.s3_bucket.to_string())
            .key(blob_ref.0.to_string())
            .send()
            .await;

        let object_output = match maybe_out {
            Ok(output) => output,
            Err(SdkError::ServiceError(x)) => match x.err() {
                GetObjectError::NoSuchKey(_) => return Err(StorageError::NotFound),
                e => {
                    tracing::warn!("Blob Fetch Error, Service Error: {}", e);
                    return Err(StorageError::Internal);
                },
            },
            Err(e) => {
                tracing::warn!("Blob Fetch Error, {}", e);
                return Err(StorageError::Internal);
            },
        };

        let buffer = match object_output.body.collect().await {
            Ok(aggreg) => aggreg.to_vec(),
            Err(e) => {
                tracing::warn!("Fetching body failed with {}", e);
                return Err(StorageError::Internal);
            }
        };

        Ok(BlobVal::new(blob_ref.clone(), buffer))
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

