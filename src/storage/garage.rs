use crate::storage::*;

#[derive(Clone, Debug, Hash)]
pub struct GrgCreds {}
pub struct GrgStore {}
pub struct GrgRef {}
pub struct GrgValue {}

impl IBuilders for GrgCreds {
    fn row_store(&self) -> Result<RowStore, StorageError> {
        unimplemented!();
    }

    fn blob_store(&self) -> Result<BlobStore, StorageError> {
        unimplemented!();
    }

    fn url(&self) -> &str {
        return "grg://unimplemented;"
    }
}

impl IRowStore for GrgStore {
    fn row(&self, partition: &str, sort: &str) -> RowRef {
        unimplemented!();
    }

    fn select(&self, selector: Selector) -> AsyncResult<Vec<RowValue>> {
        unimplemented!();
    }
}

impl IRowRef for GrgRef {
    fn clone_boxed(&self) -> RowRef {
        unimplemented!();
    }

    fn key(&self) -> (&str, &str) {
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
        unimplemented!();
    }
}

impl IRowValue for GrgValue {
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




/*
/// A custom S3 region, composed of a region name and endpoint.
/// We use this instead of rusoto_signature::Region so that we can
/// derive Hash and Eq


#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub endpoint: String,
}

impl Region {
    pub fn as_rusoto_region(&self) -> rusoto_signature::Region {
        rusoto_signature::Region::Custom {
            name: self.name.clone(),
            endpoint: self.endpoint.clone(),
        }
    }
}
*/

/*
pub struct Garage {
    pub s3_region: Region,
    pub k2v_region: Region,

    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
}

impl StoreBuilder<> for Garage {
    fn row_store(&self) -> 
}

pub struct K2V {}
impl RowStore for K2V {

}*/
