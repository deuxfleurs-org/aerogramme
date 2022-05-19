use anyhow::{bail, Result};
use std::sync::Arc;

use rusoto_signature::Region;

use crate::config::*;
use crate::login::{ldap_provider::*, static_provider::*, *};
use crate::mailbox::Mailbox;

pub struct Server {
    pub login_provider: Box<dyn LoginProvider>,
}

impl Server {
    pub fn new(config: Config) -> Result<Arc<Self>> {
        let s3_region = Region::Custom {
            name: config.aws_region.clone(),
            endpoint: config.s3_endpoint,
        };
        let k2v_region = Region::Custom {
            name: config.aws_region,
            endpoint: config.k2v_endpoint,
        };
        let login_provider: Box<dyn LoginProvider> = match (config.login_static, config.login_ldap)
        {
            (Some(st), None) => Box::new(StaticLoginProvider::new(st, k2v_region, s3_region)?),
            (None, Some(ld)) => Box::new(LdapLoginProvider::new(ld, k2v_region, s3_region)?),
            (Some(_), Some(_)) => bail!("A single login provider must be set up in config file"),
            (None, None) => bail!("No login provider is set up in config file"),
        };
        Ok(Arc::new(Self { login_provider }))
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        let creds = self.login_provider.login("lx", "plop").await?;

        let mut mailbox = Mailbox::new(&creds, "TestMailbox".to_string()).await?;

        mailbox.test().await?;

        Ok(())
    }
}
