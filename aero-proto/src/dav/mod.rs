mod codec;
mod controller;
mod middleware;
mod node;
mod resource;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use futures::future::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use hyper::rt::{Read, Write};
use hyper::server::conn::http1 as http;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use rustls_pemfile::{certs, private_key};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

use aero_user::config::{DavConfig, DavUnsecureConfig};
use aero_user::login::ArcLoginProvider;

use crate::dav::controller::Controller;

pub struct Server {
    bind_addr: SocketAddr,
    login_provider: ArcLoginProvider,
    tls: Option<TlsAcceptor>,
}

pub fn new_unsecure(config: DavUnsecureConfig, login: ArcLoginProvider) -> Server {
    Server {
        bind_addr: config.bind_addr,
        login_provider: login,
        tls: None,
    }
}

pub fn new(config: DavConfig, login: ArcLoginProvider) -> Result<Server> {
    let loaded_certs = certs(&mut std::io::BufReader::new(std::fs::File::open(
        config.certs,
    )?))
    .collect::<Result<Vec<_>, _>>()?;
    let loaded_key = private_key(&mut std::io::BufReader::new(std::fs::File::open(
        config.key,
    )?))?
    .unwrap();

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(loaded_certs, loaded_key)?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    Ok(Server {
        bind_addr: config.bind_addr,
        login_provider: login,
        tls: Some(acceptor),
    })
}

trait Stream: Read + Write + Send + Unpin {}
impl<T: Unpin + AsyncRead + AsyncWrite + Send> Stream for TokioIo<T> {}

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
            tracing::info!("Accepted connection from {}", remote_addr);
            let stream = match self.build_stream(socket).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err=?e, "TLS acceptor failed");
                    continue;
                }
            };

            let login = self.login_provider.clone();
            let conn = tokio::spawn(async move {
                //@FIXME should create a generic "public web" server on which "routers" could be
                //abitrarily bound
                //@FIXME replace with a handler supporting http2 and TLS

                match http::Builder::new()
                    .serve_connection(
                        stream,
                        service_fn(|req: Request<hyper::body::Incoming>| {
                            let login = login.clone();
                            tracing::info!("{:?} {:?}", req.method(), req.uri());
                            async {
                                match middleware::auth(login, req, |user, request| {
                                    async { Controller::route(user, request).await }.boxed()
                                })
                                .await
                                {
                                    Ok(v) => Ok(v),
                                    Err(e) => {
                                        tracing::error!(err=?e, "internal error");
                                        Response::builder()
                                            .status(500)
                                            .body(codec::text_body("Internal error"))
                                    }
                                }
                            }
                        }),
                    )
                    .await
                {
                    Err(e) => tracing::warn!(err=?e, "connection failed"),
                    Ok(()) => tracing::trace!("connection terminated with success"),
                }
            });
            connections.push(conn);
        }
        drop(tcp);

        tracing::info!("Server shutting down, draining remaining connections...");
        while connections.next().await.is_some() {}

        Ok(())
    }

    async fn build_stream(&self, socket: TcpStream) -> Result<Box<dyn Stream>> {
        match self.tls.clone() {
            Some(acceptor) => {
                let stream = acceptor.accept(socket).await?;
                Ok(Box::new(TokioIo::new(stream)))
            }
            None => Ok(Box::new(TokioIo::new(socket))),
        }
    }
}

// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/'>
//   <D:prop>
//     <D:getcontenttype/>
//     <D:resourcetype/>
//     <D:displayname/>
//     <A:calendar-color/>
//   </D:prop>
// </D:propfind>

// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/' xmlns:C='urn:ietf:params:xml:ns:caldav'>
//   <D:prop>
//     <D:resourcetype/>
//     <D:owner/>
//     <D:displayname/>
//     <D:current-user-principal/>
//     <D:current-user-privilege-set/>
//     <A:calendar-color/>
//     <C:calendar-home-set/>
//   </D:prop>
// </D:propfind>

// <D:propfind xmlns:D='DAV:' xmlns:C='urn:ietf:params:xml:ns:caldav' xmlns:CS='http://calendarserver.org/ns/'>
//   <D:prop>
//     <D:resourcetype/>
//     <D:owner/>
//     <D:current-user-principal/>
//     <D:current-user-privilege-set/>
//     <D:supported-report-set/>
//     <C:supported-calendar-component-set/>
//     <CS:getctag/>
//   </D:prop>
// </D:propfind>

// <C:calendar-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
//   <D:prop>
//     <D:getetag/>
//     <C:calendar-data/>
//    </D:prop>
//    <D:href>/alice/calendar/personal/something.ics</D:href>
//  </C:calendar-multiget>
