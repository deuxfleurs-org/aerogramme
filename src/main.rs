mod bayou;
mod config;
mod cryptoblob;
mod login;
mod mailbox;
mod time;
mod uidindex;

use anyhow::{bail, Result};
use std::sync::Arc;

use rusoto_signature::Region;

use config::*;
use login::{ldap_provider::*, static_provider::*, *};
use mailbox::Mailbox;

#[tokio::main]
async fn main() {
    if let Err(e) = main2().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn main2() -> Result<()> {
    let config = read_config("mailrage.toml".into())?;

    let main = Main::new(config)?;
    main.run().await
}

struct Main {
    pub s3_region: Region,
    pub k2v_region: Region,
    pub login_provider: Box<dyn LoginProvider>,
}

impl Main {
    fn new(config: Config) -> Result<Arc<Self>> {
        let s3_region = Region::Custom {
            name: config.s3_region,
            endpoint: config.s3_endpoint,
        };
        let k2v_region = Region::Custom {
            name: config.k2v_region,
            endpoint: config.k2v_endpoint,
        };
        let login_provider: Box<dyn LoginProvider> = match (config.login_static, config.login_ldap)
        {
            (Some(st), None) => Box::new(StaticLoginProvider::new(st, k2v_region.clone())?),
            (None, Some(ld)) => Box::new(LdapLoginProvider::new(ld)?),
            (Some(_), Some(_)) => bail!("A single login provider must be set up in config file"),
            (None, None) => bail!("No login provider is set up in config file"),
        };
        Ok(Arc::new(Self {
            s3_region,
            k2v_region,
            login_provider,
        }))
    }

    async fn run(self: &Arc<Self>) -> Result<()> {
        let creds = self.login_provider.login("lx", "plop").await?;

        let mut mailbox = Mailbox::new(
            self.k2v_region.clone(),
            self.s3_region.clone(),
            creds.clone(),
            "TestMailbox".to_string(),
        )
        .await?;

        mailbox.test().await?;

        Ok(())
    }
}
