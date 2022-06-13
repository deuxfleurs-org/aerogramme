use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use boitalettres::server::accept::addr::AddrStream;
use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use futures::future::FutureExt;
use tower::Service;

use crate::mailstore::Mailstore;
use crate::session;

pub struct Instance {
    pub mailstore: Arc<Mailstore>,
}
impl Instance {
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        Self { mailstore }
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
        let ms = self.mailstore.clone();
        async { Ok(Connection::new(ms)) }.boxed()
    }
}

pub struct Connection {
    session: session::Manager,
}
impl Connection {
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        Self { session: session::Manager::new(mailstore) }
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



