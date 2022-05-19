use anyhow::Result;
use async_trait::async_trait;

use crate::config::*;
use crate::login::*;

pub struct LdapLoginProvider {
    // TODO
}

impl LdapLoginProvider {
    pub fn new(_config: LoginLdapConfig) -> Result<Self> {
        unimplemented!()
    }
}

#[async_trait]
impl LoginProvider for LdapLoginProvider {
    async fn login(&self, _username: &str, _password: &str) -> Result<Credentials> {
        unimplemented!()
    }
}
