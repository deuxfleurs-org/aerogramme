use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::Result;
use boitalettres::server::accept::addr::AddrStream;
use futures::future::BoxFuture;
use tower::Service;

use crate::connection::Connection;
use crate::mailstore::Mailstore;

pub struct Instance {
    pub mailstore: Arc<Mailstore>
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
        Box::pin(async { 
            Ok(Connection::new(ms)) 
        })
    }
}


