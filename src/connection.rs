use std::sync::Arc;
use std::task::{Context, Poll};

use boitalettres::errors::Error as BalError;
use boitalettres::proto::{Request,Response};
use futures::future::BoxFuture;
use tower::Service;

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
        let mailstore = self.mailstore.clone();
        Box::pin(async move {
            use imap_codec::types::{
                command::CommandBody,
                response::{Capability, Data},
            };

            let r = match req.body {
                CommandBody::Capability => {
                    let capabilities = vec![Capability::Imap4Rev1, Capability::Idle];
                    let body = vec![Data::Capability(capabilities)];
                    Response::ok(
                        "Pre-login capabilities listed, post-login capabilities have more.",
                    )?
                    .with_body(body)
                }
                CommandBody::Login {
                    username,
                    password,
                } => {
                    let (u, p) = match (String::try_from(username), String::try_from(password)) {
                      (Ok(u), Ok(p)) => (u, p),
                      _ => { return Response::bad("Invalid characters") }
                    };

                    tracing::debug!(user = %u, "command.login");
                    let creds = match mailstore.login_provider.login(&u, &p).await {
                        Err(_) => { return Response::no("[AUTHENTICATIONFAILED] Authentication failed.") }
                        Ok(c) => c,
                    };

                    Response::ok("Logged in")?
                }
                _ => Response::bad("Error in IMAP command received by server.")?,
            };

            Ok(r)
        })
    }
}


