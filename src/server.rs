use anyhow::{bail, Result};
use std::sync::Arc;

use rusoto_signature::Region;

use crate::config::*;
use crate::login::{ldap_provider::*, static_provider::*, *};
use crate::mailbox::Mailbox;

use boitalettres::proto::{Request, Response};
use boitalettres::server::accept::addr::{AddrIncoming, AddrStream};
use boitalettres::server::Server as ImapServer;

use std::pin::Pin;
use std::task::{Context, Poll};
use tower::Service;
use futures::future::BoxFuture;

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
    type Error = anyhow::Error;
    type Future = BoxFuture<'static, Result<Self::Response>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        tracing::debug!("Got request: {:#?}", req);
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
                    username: _,
                    password: _,
                } => Response::ok("Logged in")?,
                _ => Response::bad("Error in IMAP command received by server.")?,
            };

            Ok(r)
        })
    }
}

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




pub struct Mailstore {
    pub login_provider: Box<dyn LoginProvider + Send + Sync>,
}
impl Mailstore {
    pub fn new(config: Config) -> Result<Arc<Self>> {
        let s3_region = Region::Custom {
            name: config.aws_region.clone(),
            endpoint: config.s3_endpoint,
        };
        let k2v_region = Region::Custom {
            name: config.aws_region,
            endpoint: config.k2v_endpoint,
        };
        let login_provider: Box<dyn LoginProvider + Send + Sync> = match (config.login_static, config.login_ldap)
        {
            (Some(st), None) => Box::new(StaticLoginProvider::new(st, k2v_region, s3_region)?),
            (None, Some(ld)) => Box::new(LdapLoginProvider::new(ld, k2v_region, s3_region)?),
            (Some(_), Some(_)) => bail!("A single login provider must be set up in config file"),
            (None, None) => bail!("No login provider is set up in config file"),
        };
        Ok(Arc::new(Self { login_provider }))
    }
}



pub struct Server {
    pub incoming: AddrIncoming,
    pub mailstore: Arc<Mailstore>,
}
impl Server {
    pub async fn new(config: Config) -> Result<Self> {
        Ok(Self { 
            incoming: AddrIncoming::new("127.0.0.1:4567").await?,
            mailstore: Mailstore::new(config)?,
        })
    }

    pub async fn run(self: Self) -> Result<()> {
        tracing::info!("Starting server on {:#}", self.incoming.local_addr);
        let server = ImapServer::new(self.incoming).serve(Instance::new(self.mailstore.clone()));
        let _ = server.await?;

        /*let creds = self.login_provider.login("quentin", "poupou").await?;
        let mut mailbox = Mailbox::new(&creds, "TestMailbox".to_string()).await?;
        mailbox.test().await?;*/

        Ok(())
    }
}
