use std::sync::Arc;

use anyhow::Result;
use futures::try_join;
use log::*;
use tokio::sync::watch;

use crate::config::*;
use crate::imap;
use crate::lmtp::*;
use crate::login::ArcLoginProvider;
use crate::login::{ldap_provider::*, static_provider::*};

pub struct Server {
    lmtp_server: Option<Arc<LmtpServer>>,
    imap_server: Option<imap::Server>,
}

impl Server {
    pub async fn from_companion_config(config: CompanionConfig) -> Result<Self> {
        let login = Arc::new(StaticLoginProvider::new(config.users).await?);

        let lmtp_server = None;
        let imap_server = Some(imap::new(config.imap, login.clone()).await?);
        Ok(Self { lmtp_server, imap_server })
    }

    pub async fn from_provider_config(config: ProviderConfig) -> Result<Self> {
        let login: ArcLoginProvider = match config.users {
            UserManagement::Static(x) => Arc::new(StaticLoginProvider::new(x).await?),
            UserManagement::Ldap(x) => Arc::new(LdapLoginProvider::new(x)?),
        };

        let lmtp_server = Some(LmtpServer::new(config.lmtp, login.clone()));
        let imap_server = Some(imap::new(config.imap, login.clone()).await?);

        Ok(Self { lmtp_server, imap_server })
    }

    pub async fn run(self) -> Result<()> {
        tracing::info!("Starting Aerogramme...");

        let (exit_signal, provoke_exit) = watch_ctrl_c();
        let _exit_on_err = move |err: anyhow::Error| {
            error!("Error: {}", err);
            let _ = provoke_exit.send(true);
        };

        try_join!(
            async {
                match self.lmtp_server.as_ref() {
                    None => Ok(()),
                    Some(s) => s.run(exit_signal.clone()).await,
                }
            },
            async {
                match self.imap_server {
                    None => Ok(()),
                    Some(s) => s.run(exit_signal.clone()).await,
                }
            }
        )?;

        Ok(())
    }
}

pub fn watch_ctrl_c() -> (watch::Receiver<bool>, Arc<watch::Sender<bool>>) {
    let (send_cancel, watch_cancel) = watch::channel(false);
    let send_cancel = Arc::new(send_cancel);
    let send_cancel_2 = send_cancel.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
        info!("Received CTRL+C, shutting down.");
        send_cancel.send(true).unwrap();
    });
    (watch_cancel, send_cancel_2)
}
