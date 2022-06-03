use std::sync::Arc;
use std::task::{Context, Poll};

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request, Response};
use futures::future::BoxFuture;
use imap_codec::types::command::CommandBody;
use tower::Service;

use crate::command;
use crate::mailstore::Mailstore;

pub struct Connection {
    pub mailstore: Arc<Mailstore>,
}
impl Connection {
    pub fn new(mailstore: Arc<Mailstore>) -> Self {
        Self { mailstore }
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
        let cmd = command::Command::new(self.mailstore.clone());
        Box::pin(async move {
            match req.body {
                CommandBody::Capability => cmd.capability().await,
                CommandBody::Login { username, password } => cmd.login(username, password).await,
                _ => Response::bad("Error in IMAP command received by server."),
            }
        })
    }
}
