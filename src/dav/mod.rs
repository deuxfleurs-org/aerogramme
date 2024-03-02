mod types;
mod caltypes;
mod acltypes;
mod versioningtypes;
mod encoder;
mod calencoder;

use std::net::SocketAddr;

use anyhow::{anyhow, Result};
use base64::Engine;
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
use crate::user::User;

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
            tracing::info!("Accepted connection from {}", remote_addr);
            let stream = TokioIo::new(socket);
            let login = self.login_provider.clone();
            let conn = tokio::spawn(async move {
                //@FIXME should create a generic "public web" server on which "routers" could be
                //abitrarily bound
                //@FIXME replace with a handler supporting http2 and TLS
                match http::Builder::new().serve_connection(stream, service_fn(|req: Request<hyper::body::Incoming>| {
                    let login = login.clone();
                    async move {
                        auth(login, req).await
                    }
                })).await {
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
}

//@FIXME We should not support only BasicAuth
async fn auth(
    login: ArcLoginProvider,
    req: Request<impl hyper::body::Body>, 
) -> Result<Response<Full<Bytes>>> {

    let auth_val = match req.headers().get("Authorization") {
        Some(hv) => hv.to_str()?,
        None => return Ok(Response::builder()
            .status(401)
            .body(Full::new(Bytes::from("Missing Authorization field")))?),
    };

    let b64_creds_maybe_padded = match auth_val.split_once(" ") {
        Some(("Basic", b64)) => b64,
        _ => return Ok(Response::builder()
            .status(400)
            .body(Full::new(Bytes::from("Unsupported Authorization field")))?),
    };

    // base64urlencoded may have trailing equals, base64urlsafe has not
    // theoretically authorization is padded but "be liberal in what you accept"
    let b64_creds_clean = b64_creds_maybe_padded.trim_end_matches('=');

    // Decode base64
    let creds = base64::engine::general_purpose::STANDARD_NO_PAD.decode(b64_creds_clean)?;
    let str_creds = std::str::from_utf8(&creds)?;
    
    // Split username and password
    let (username, password) = str_creds
        .split_once(':')
        .ok_or(anyhow!("Missing colon in Authorization, can't split decoded value into a username/password pair"))?;

    // Call login provider
    let creds = match login.login(username, password).await {
        Ok(c) => c,
        Err(e) => return Ok(Response::builder()
            .status(401)
            .body(Full::new(Bytes::from("Wrong credentials")))?),
    };

    // Build a user
    let user = User::new(username.into(), creds).await?;

    // Call router with user
    router(user, req).await 
}

async fn router(user: std::sync::Arc<User>, req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>> {
    let path_segments: Vec<_> = req.uri().path().split("/").filter(|s| *s != "").collect();
    match path_segments.as_slice() {
        [] => tracing::info!("root"),
        [ username, ..] if *username != user.username => return Ok(Response::builder()
            .status(403)
            .body(Full::new(Bytes::from("Accessing other user ressources is not allowed")))?),
        [ _ ] => tracing::info!("user home"),
        [ _, "calendar" ] => tracing::info!("user calendars"),
        [ _, "calendar", colname ] => tracing::info!(name=colname, "selected calendar"),
        [ _, "calendar", colname, member ] => tracing::info!(name=colname, obj=member, "selected event"),
        _ => return Ok(Response::builder()
            .status(404)
            .body(Full::new(Bytes::from("Resource not found")))?),
    }
    Ok(Response::new(Full::new(Bytes::from("Hello World!"))))
}

async fn collections(user: std::sync::Arc<User>, req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>> {
    unimplemented!();
}
