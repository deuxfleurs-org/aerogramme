use anyhow::Result;
use async_trait::async_trait;

use crate::config::*;
use crate::login::*;

pub struct LdapLoginProvider {
    // TODO
}

impl LdapLoginProvider {
    pub fn new(config: LoginLdapConfig) -> Result<Self> {
        unimplemented!()
    }
}

#[async_trait]
impl LoginProvider for LdapLoginProvider {
    async fn login(&self, username: &str, password: &str) -> Result<Credentials> {
        unimplemented!()
    }
}
