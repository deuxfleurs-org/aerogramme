use anyhow::Result;
use std::sync::Arc;

use crate::config::*;
use crate::instance;
use crate::mailstore;

use boitalettres::server::accept::addr::AddrIncoming;
use boitalettres::server::Server as ImapServer;

pub struct Server {
    pub incoming: AddrIncoming,
    pub mailstore: Arc<mailstore::Mailstore>,
}
impl Server {
    pub async fn new(config: Config) -> Result<Self> {
        Ok(Self {
            incoming: AddrIncoming::new("127.0.0.1:4567").await?,
            mailstore: mailstore::Mailstore::new(config)?,
        })
    }

    pub async fn run(self: Self) -> Result<()> {
        tracing::info!("Starting server on {:#}", self.incoming.local_addr);

        /*let creds = self
            .mailstore
            .login_provider
            .login("quentin", "poupou")
            .await?;*/
        //let mut mailbox = Mailbox::new(&creds, "TestMailbox".to_string()).await?;
        //mailbox.test().await?;

        let server =
            ImapServer::new(self.incoming).serve(instance::Instance::new(self.mailstore.clone()));
        let _ = server.await?;

        Ok(())
    }
}
