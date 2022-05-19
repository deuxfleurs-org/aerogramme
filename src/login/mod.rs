pub mod ldap_provider;
pub mod static_provider;

use anyhow::Result;
use async_trait::async_trait;

use crate::cryptoblob::Key as SymmetricKey;

#[derive(Clone, Debug)]
pub struct Credentials {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub bucket: String,
    pub master_key: SymmetricKey,
}

#[async_trait]
pub trait LoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials>;
}
