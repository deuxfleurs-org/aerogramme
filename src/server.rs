use anyhow::{bail, Result};
use std::sync::Arc;

use rusoto_signature::Region;

use crate::config::*;
use crate::login::{ldap_provider::*, static_provider::*, *};
use crate::mailbox::Mailbox;

use boitalettres::proto::{Request, Response};
use boitalettres::server::accept::addr::{AddrIncoming, AddrStream};
use boitalettres::server::Server as ImapServer;
use tracing_subscriber;

use std::task::{Context, Poll};
use tower::Service;
use std::future::Future;
use std::pin::Pin;

use std::error::Error;

pub struct Server {
    pub login_provider: Box<dyn LoginProvider>,
}

async fn handle_req(req: Request) -> Result<Response> {
    tracing::debug!("Got request: {:#?}", req);
    Ok(Response::ok("Done")?)
}

struct Echo;

impl Service<Request> for Echo {
  type Response = Response;
  type Error = Box<dyn Error + Send + Sync>;
  type Future = Pin<Box<dyn futures::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

  fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request) -> Self::Future {
    Box::pin(Echo::handle_req(req))
  }
}

impl Echo {
  async fn handle_req(req: Request) -> Result<Response, Box<dyn Error + Send + Sync>> {
    println!("Got request: {:#?}", req);
    Ok(Response::ok("Done").unwrap())
  }
}

impl Server {
    pub fn new(config: Config) -> Result<Arc<Self>> {
        let s3_region = Region::Custom {
            name: config.aws_region.clone(),
            endpoint: config.s3_endpoint,
        };
        let k2v_region = Region::Custom {
            name: config.aws_region,
            endpoint: config.k2v_endpoint,
        };
        let login_provider: Box<dyn LoginProvider> = match (config.login_static, config.login_ldap)
        {
            (Some(st), None) => Box::new(StaticLoginProvider::new(st, k2v_region, s3_region)?),
            (None, Some(ld)) => Box::new(LdapLoginProvider::new(ld, k2v_region, s3_region)?),
            (Some(_), Some(_)) => bail!("A single login provider must be set up in config file"),
            (None, None) => bail!("No login provider is set up in config file"),
        };
        Ok(Arc::new(Self { login_provider }))
    }

    pub async fn run(self: &Arc<Self>) -> Result<()> {
        // tracing_subscriber::fmt::init();

        let incoming = AddrIncoming::new("127.0.0.1:4567").await?;

        
        let make_service = tower::service_fn(|addr: &AddrStream| {
            tracing::debug!(remote_addr = %addr.remote_addr, local_addr = %addr.local_addr, "accept");
            //let service = tower::ServiceBuilder::new().service_fn(handle_req);
            //let service = tower::service_fn(handle_req);
            let service = Echo;
            futures::future::ok::<_, std::convert::Infallible>(service)
            //service
        });


        //println!("{:?}", make_service);
        let server = ImapServer::new(incoming).serve(make_service);
        let _ = server.await?;

        /*let creds = self.login_provider.login("quentin", "poupou").await?;

        let mut mailbox = Mailbox::new(&creds, "TestMailbox".to_string()).await?;

        mailbox.test().await?;*/

        Ok(())
    }
}
