use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use futures::try_join;
use log::*;
use tokio::sync::watch;

use crate::auth;
use crate::config::*;
use crate::dav;
use crate::imap;
use crate::lmtp::*;
use crate::login::ArcLoginProvider;
use crate::login::{demo_provider::*, ldap_provider::*, static_provider::*};

pub struct Server {
    lmtp_server: Option<Arc<LmtpServer>>,
    imap_unsecure_server: Option<imap::Server>,
    imap_server: Option<imap::Server>,
    auth_server: Option<auth::AuthServer>,
    dav_unsecure_server: Option<dav::Server>,
    pid_file: Option<PathBuf>,
}

impl Server {
    pub async fn from_companion_config(config: CompanionConfig) -> Result<Self> {
        tracing::info!("Init as companion");
        let login = Arc::new(StaticLoginProvider::new(config.users).await?);

        let lmtp_server = None;
        let imap_unsecure_server = Some(imap::new_unsecure(config.imap, login.clone()));
        Ok(Self {
            lmtp_server,
            imap_unsecure_server,
            imap_server: None,
            auth_server: None,
            dav_unsecure_server: None,
            pid_file: config.pid,
        })
    }

    pub async fn from_provider_config(config: ProviderConfig) -> Result<Self> {
        tracing::info!("Init as provider");
        let login: ArcLoginProvider = match config.users {
            UserManagement::Demo => Arc::new(DemoLoginProvider::new()),
            UserManagement::Static(x) => Arc::new(StaticLoginProvider::new(x).await?),
            UserManagement::Ldap(x) => Arc::new(LdapLoginProvider::new(x)?),
        };

        let lmtp_server = config.lmtp.map(|lmtp| LmtpServer::new(lmtp, login.clone()));
        let imap_unsecure_server = config
            .imap_unsecure
            .map(|imap| imap::new_unsecure(imap, login.clone()));
        let imap_server = config
            .imap
            .map(|imap| imap::new(imap, login.clone()))
            .transpose()?;
        let auth_server = config
            .auth
            .map(|auth| auth::AuthServer::new(auth, login.clone()));
        let dav_unsecure_server = config
            .dav_unsecure
            .map(|dav_config| dav::new_unsecure(dav_config, login.clone()));

        Ok(Self {
            lmtp_server,
            imap_unsecure_server,
            imap_server,
            dav_unsecure_server,
            auth_server,
            pid_file: config.pid,
        })
    }

    pub async fn run(self) -> Result<()> {
        let pid = std::process::id();
        tracing::info!(pid = pid, "Starting main loops");

        // write the pid file
        if let Some(pid_file) = self.pid_file {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(pid_file)?;
            file.write_all(pid.to_string().as_bytes())?;
            drop(file);
        }

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
                match self.imap_unsecure_server {
                    None => Ok(()),
                    Some(s) => s.run(exit_signal.clone()).await,
                }
            },
            async {
                match self.imap_server {
                    None => Ok(()),
                    Some(s) => s.run(exit_signal.clone()).await,
                }
            },
            async {
                match self.auth_server {
                    None => Ok(()),
                    Some(a) => a.run(exit_signal.clone()).await,
                }
            },
            async {
                match self.dav_unsecure_server {
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
