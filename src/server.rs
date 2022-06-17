use std::sync::Arc;

use boitalettres::server::accept::addr::AddrIncoming;
use boitalettres::server::accept::addr::AddrStream;
use boitalettres::server::Server as ImapServer;

use anyhow::{bail, Result};
use futures::{try_join, StreamExt};
use log::*;
use rusoto_signature::Region;
use tokio::sync::watch;
use tower::Service;

use crate::config::*;
use crate::lmtp::*;
use crate::login::{ldap_provider::*, static_provider::*, *};
use crate::mailbox::Mailbox;
use crate::imap;

pub struct Server {
    lmtp_server: Option<Arc<LmtpServer>>,
    imap_server: Option<imap::Server>,
}

impl Server {
    pub async fn new(config: Config) -> Result<Self> {
        let (login, lmtp_conf, imap_conf) = build(config)?;

        let lmtp_server = lmtp_conf.map(|cfg| LmtpServer::new(cfg, login.clone()));
        let imap_server = imap_conf.map(|cfg| imap::new(cfg, login.clone()));

        Ok(Self { lmtp_server, imap_server })
    }

    pub async fn run(self) -> Result<()> {
        tracing::info!("Starting Aerogramme...");

        let (exit_signal, provoke_exit) = watch_ctrl_c();
        let exit_on_err = move |err: anyhow::Error| {
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
                match self.imap_server.as_ref() {
                    None => Ok(()),
                    Some(s) => s.run(exit_signal.clone()).await,
                }
            }
        )?;

        Ok(())
    }
}

fn build(config: Config) -> Result<(Arc<dyn LoginProvider + Send + Sync>, Option<LmtpConfig>, Option<ImapConfig>> {
    let s3_region = Region::Custom {
        name: config.aws_region.clone(),
        endpoint: config.s3_endpoint,
    };
    let k2v_region = Region::Custom {
        name: config.aws_region,
        endpoint: config.k2v_endpoint,
    };

    let lp: Arc<dyn LoginProvider + Send + Sync> = match (config.login_static, config.login_ldap) {
        (Some(st), None) => Arc::new(StaticLoginProvider::new(st, k2v_region, s3_region)?),
        (None, Some(ld)) => Arc::new(LdapLoginProvider::new(ld, k2v_region, s3_region)?),
        (Some(_), Some(_)) => {
            bail!("A single login provider must be set up in config file")
        }
        (None, None) => bail!("No login provider is set up in config file"),
    };

    Ok(lp, self.lmtp_config, self.imap_config)
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
