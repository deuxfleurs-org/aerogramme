mod session;
mod flow;
mod command;

use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use boitalettres::server::accept::addr::AddrStream;
use boitalettres::server::Server as ImapServer;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use tower::Service;

use crate::LoginProvider;

/// Server is a thin wrapper to register our Services in BàL
pub struct Server(ImapServer<AddrIncoming, service::Instance>);
pub async fn new(
    config: ImapConfig,
    login: Arc<dyn LoginProvider + Send + Sync>,
) -> Result<Server> {

    //@FIXME add a configuration parameter
    let incoming = AddrIncoming::new(config.bind_addr).await?;
    let imap = ImapServer::new(incoming).serve(service::Instance::new(login.clone()));

    tracing::info!("IMAP activated, will listen on {:#}", self.imap.incoming.local_addr);
    Server(imap)
}
impl Server {
    pub async fn run(&self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        tracing::info!("IMAP started!");
        tokio::select! {
            s = self => s?,
            _ = must_exit.changed() => tracing::info!("Stopped IMAP server"),
        }

        Ok(())
    }
}

//---

/// Instance is the main Tokio Tower service that we register in BàL.
/// It receives new connection demands and spawn a dedicated service.
struct Instance {
    login_provider: Arc<dyn LoginProvider + Send + Sync>,
}
impl Instance {
    pub fn new(login_provider: Arc<dyn LoginProvider + Send + Sync>) -> Self {
        Self { login_provider }
    }
}
impl<'a> Service<&'a AddrStream> for Instance {
    type Response = Connection;
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
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
    pub fn new(login_provider: Arc<dyn LoginProvider + Send + Sync>) -> Self {
        Self {
            session: session::Manager::new(login_provider),
        }
    }
}
impl Service<Request> for Connection {
    type Response = Response;
    type Error = BalError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        tracing::debug!("Got request: {:#?}", req);
        self.session.process(req)
    }
}
