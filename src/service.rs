use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use boitalettres::server::accept::addr::AddrStream;
use futures::future::BoxFuture;
use futures::future::FutureExt;
use tower::Service;

use crate::session;
use crate::LoginProvider;

pub struct Instance {
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

pub struct Connection {
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
