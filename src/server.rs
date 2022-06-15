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

use crate::service;
use crate::lmtp::*;
use crate::config::*;
use crate::login::{ldap_provider::*, static_provider::*, *};
use crate::mailbox::Mailbox;

pub struct Server {
    lmtp_server: Option<Arc<LmtpServer>>,
    imap_server: ImapServer<AddrIncoming, service::Instance>,
}

impl Server {
    pub async fn new(config: Config) -> Result<Self> {
        let lmtp_config = config.lmtp.clone(); //@FIXME
        let login = authenticator(config)?;

        let lmtp = lmtp_config.map(|cfg| LmtpServer::new(cfg, login.clone()));

        let incoming = AddrIncoming::new("127.0.0.1:4567").await?;
        let imap = ImapServer::new(incoming).serve(service::Instance::new(login.clone()));

        Ok(Self {
            lmtp_server: lmtp,
            imap_server: imap,
        })
    }


    pub async fn run(self) -> Result<()> {
        //tracing::info!("Starting server on {:#}", self.imap.incoming.local_addr);
        tracing::info!("Starting Aerogramme...");

        let (exit_signal, provoke_exit) = watch_ctrl_c();
        let exit_on_err = move |err: anyhow::Error| {
            error!("Error: {}", err);
            let _ = provoke_exit.send(true);
        };


        try_join!(async {
            match self.lmtp_server.as_ref() {
                None => Ok(()),
                Some(s) => s.run(exit_signal.clone()).await,
            }
        },
        //@FIXME handle ctrl + c
        async {
            self.imap_server.await?;
            Ok(())
        }
        )?;


        Ok(())
    }
}

fn authenticator(config: Config) -> Result<Arc<dyn LoginProvider + Send + Sync>> {
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
    Ok(lp)
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
