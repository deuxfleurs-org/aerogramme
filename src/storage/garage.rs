use crate::storage::*;

pub struct GrgCreds {}
pub struct GrgStore {}
pub struct GrgRef {}
pub struct GrgValue {}

pub struct GrgTypes {}
impl Sto for GrgTypes {
    type Builder=GrgCreds;
    type Store=GrgStore;
    type Ref=GrgRef;
    type Value=GrgValue;
}

impl RowBuilder<GrgTypes> for GrgCreds {
    fn row_store(&self) -> GrgStore {
        unimplemented!();
    }
}

impl RowStore<GrgTypes> for GrgStore {
    fn new_row(&self, partition: &str, sort: &str) -> GrgRef {
        unimplemented!();
    }
}

impl RowRef<GrgTypes> for GrgRef {
    fn set_value(&self, content: Vec<u8>) -> GrgValue {
        unimplemented!();
    }
    async fn fetch(&self) -> Result<GrgValue, Error> {
        unimplemented!();
    }
    async fn rm(&self) -> Result<(), Error> {
        unimplemented!();
    }
    async fn poll(&self) -> Result<Option<GrgValue>, Error> {
        unimplemented!();
    }
}

impl RowValue<GrgTypes> for GrgValue {
    fn to_ref(&self) -> GrgRef {
        unimplemented!();
    }
    fn content(&self) -> ConcurrentValues {
        unimplemented!();
    }
    async fn push(&self) -> Result<(), Error> {
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
