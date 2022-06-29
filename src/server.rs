use std::sync::Arc;

use anyhow::{bail, Result};
use futures::{try_join, StreamExt};
use log::*;
use tokio::sync::watch;

use crate::config::*;
use crate::imap;
use crate::lmtp::*;
use crate::login::ArcLoginProvider;
use crate::login::{ldap_provider::*, static_provider::*, Region};

pub struct Server {
    lmtp_server: Option<Arc<LmtpServer>>,
    imap_server: Option<imap::Server>,
}

impl Server {
    pub async fn new(config: Config) -> Result<Self> {
        let (login, lmtp_conf, imap_conf) = build(config)?;

        let lmtp_server = lmtp_conf.map(|cfg| LmtpServer::new(cfg, login.clone()));
        let imap_server = match imap_conf {
            Some(cfg) => Some(imap::new(cfg, login.clone()).await?),
            None => None,
        };

        Ok(Self {
            lmtp_server,
            imap_server,
        })
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

fn build(config: Config) -> Result<(ArcLoginProvider, Option<LmtpConfig>, Option<ImapConfig>)> {
    let s3_region = Region {
        name: config.aws_region.clone(),
        endpoint: config.s3_endpoint,
    };
    let k2v_region = Region {
        name: config.aws_region,
        endpoint: config.k2v_endpoint,
    };

    let lp: ArcLoginProvider = match (config.login_static, config.login_ldap) {
        (Some(st), None) => Arc::new(StaticLoginProvider::new(st, k2v_region, s3_region)?),
        (None, Some(ld)) => Arc::new(LdapLoginProvider::new(ld, k2v_region, s3_region)?),
        (Some(_), Some(_)) => {
            bail!("A single login provider must be set up in config file")
        }
        (None, None) => bail!("No login provider is set up in config file"),
    };

    Ok((lp, config.lmtp, config.imap))
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
