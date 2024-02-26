use std::net::SocketAddr;

use anyhow::Result;
use hyper::service::service_fn;
use hyper::{Request, Response, body::Bytes};
use hyper::server::conn::http1 as http;
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::DavUnsecureConfig;
use crate::login::ArcLoginProvider;

pub struct Server {
    bind_addr: SocketAddr,
    login_provider: ArcLoginProvider,
}

pub fn new_unsecure(config: DavUnsecureConfig, login: ArcLoginProvider) -> Server {
    Server {
        bind_addr: config.bind_addr,
        login_provider: login,
    }
}

impl Server {
    pub async fn run(self: Self, mut must_exit: watch::Receiver<bool>) -> Result<()> {
        let tcp = TcpListener::bind(self.bind_addr).await?;
        tracing::info!("DAV server listening on {:#}", self.bind_addr);

        let mut connections = FuturesUnordered::new();
        while !*must_exit.borrow() {
            let wait_conn_finished = async {
                if connections.is_empty() {
                    futures::future::pending().await
                } else {
                    connections.next().await
                }
            };
            let (socket, remote_addr) = tokio::select! {
                a = tcp.accept() => a?,
                _ = wait_conn_finished => continue,
                _ = must_exit.changed() => continue,
            };
            tracing::info!("DAV: accepted connection from {}", remote_addr);
            let stream = TokioIo::new(socket);
            let conn = tokio::spawn(async {
                //@FIXME should create a generic "public web" server on which "routers" could be
                //abitrarily bound
                //@FIXME replace with a handler supporting http2 and TLS
                match http::Builder::new().serve_connection(stream, service_fn(router)).await {
                    Err(e) => tracing::warn!(err=?e, "connection failed"),
                    Ok(()) => tracing::trace!("connection terminated with success"),
                }
            });
            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("DAV server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }
}

async fn router(req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>> {
    let url_exploded: Vec<_> = req.uri().path().split(",").collect();
    match url_exploded {
        _ => unimplemented!(),
    }
    Ok(Response::new(Full::new(Bytes::from("Hello World!"))))
}
