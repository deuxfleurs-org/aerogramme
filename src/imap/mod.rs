mod command;
mod flow;
mod mailbox_view;
mod session;

use std::task::{Context, Poll};

use anyhow::Result;
//use boitalettres::errors::Error as BalError;
//use boitalettres::proto::{Request, Response};
//use boitalettres::server::accept::addr::AddrIncoming;
//use boitalettres::server::accept::addr::AddrStream;
//use boitalettres::server::Server as ImapServer;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use tokio::sync::watch;

use crate::config::ImapConfig;
use crate::login::ArcLoginProvider;

/// Server is a thin wrapper to register our Services in BàL
pub struct Server{}

pub async fn new(config: ImapConfig, login: ArcLoginProvider) -> Result<Server> {
    unimplemented!();
    /*    let incoming = AddrIncoming::new(config.bind_addr).await?;
    tracing::info!("IMAP activated, will listen on {:#}", incoming.local_addr);

    let imap = ImapServer::new(incoming).serve(Instance::new(login.clone()));
    Ok(Server(imap))*/
}

impl Server {
    pub async fn run(self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        tracing::info!("IMAP started!");
        unimplemented!();
        /*tokio::select! {
            s = self.0 => s?,
            _ = must_exit.changed() => tracing::info!("Stopped IMAP server"),
        }

        Ok(())*/
    }
}

//---
/*
/// Instance is the main Tokio Tower service that we register in BàL.
/// It receives new connection demands and spawn a dedicated service.
struct Instance {
    login_provider: ArcLoginProvider,
}

impl Instance {
    pub fn new(login_provider: ArcLoginProvider) -> Self {
        Self { login_provider }
    }
}

impl<'a> Service<&'a AddrStream> for Instance {
    type Response = Connection;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, addr: &'a AddrStream) -> Self::Future {
        tracing::info!(remote_addr = %addr.remote_addr, local_addr = %addr.local_addr, "accept");
        let lp = self.login_provider.clone();
        async { Ok(Connection::new(lp)) }.boxed()
    }
}

//---

/// Connection is the per-connection Tokio Tower service we register in BàL.
/// It handles a single TCP connection, and thus has a business logic.
struct Connection {
    session: session::Manager,
}

impl Connection {
    pub fn new(login_provider: ArcLoginProvider) -> Self {
        Self {
            session: session::Manager::new(login_provider),
        }
    }
}

impl Service<Request> for Connection {
    type Response = Response;
    type Error = BalError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        tracing::debug!("Got request: {:#?}", req.command);
        self.session.process(req)
    }
}
*/
