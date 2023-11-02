use crate::storage::*;

#[derive(Clone, Debug)]
pub struct GrgCreds {}
pub struct GrgStore {}
pub struct GrgRef {}
pub struct GrgValue {}

impl IRowBuilder for GrgCreds {
    fn row_store(&self) -> Result<RowStore, Error> {
        unimplemented!();
    }
}

impl IRowStore for GrgStore {
    fn new_row(&self, partition: &str, sort: &str) -> RowRef {
        unimplemented!();
    }
}

impl IRowRef for GrgRef {
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
