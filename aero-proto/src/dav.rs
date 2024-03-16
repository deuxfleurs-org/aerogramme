use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use base64::Engine;
use hyper::service::service_fn;
use hyper::{Request, Response, body::Bytes};
use hyper::server::conn::http1 as http;
use hyper::rt::{Read, Write};
use hyper::body::Incoming;
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;
use tokio::net::TcpStream;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::io::AsyncWriteExt;
use rustls_pemfile::{certs, private_key};

use aero_user::config::{DavConfig, DavUnsecureConfig};
use aero_user::login::ArcLoginProvider;
use aero_collections::user::User;
use aero_dav::types::{PropFind, Multistatus, PropValue, ResponseDescription};
use aero_dav::realization::{Core, Calendar};
use aero_dav::xml as dav;

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
                    continue
                }
            };

            let login = self.login_provider.clone();
            let conn = tokio::spawn(async move {
                //@FIXME should create a generic "public web" server on which "routers" could be
                //abitrarily bound
                //@FIXME replace with a handler supporting http2 and TLS

                match http::Builder::new().serve_connection(stream, service_fn(|req: Request<hyper::body::Incoming>| {
                    let login = login.clone();
                    tracing::info!("{:?} {:?}", req.method(), req.uri());
                    auth(login, req)
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

use http_body_util::BodyExt;

//@FIXME We should not support only BasicAuth
async fn auth(
    login: ArcLoginProvider,
    req: Request<Incoming>, 
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {

    tracing::info!("headers: {:?}", req.headers());
    let auth_val = match req.headers().get(hyper::header::AUTHORIZATION) {
        Some(hv) => hv.to_str()?,
        None => {
            tracing::info!("Missing authorization field");
            return Ok(Response::builder()
                .status(401)
                .header("WWW-Authenticate", "Basic realm=\"Aerogramme\"")
                .body(text_body("Missing Authorization field"))?)
        },
    };

    let b64_creds_maybe_padded = match auth_val.split_once(" ") {
        Some(("Basic", b64)) => b64,
        _ => {
            tracing::info!("Unsupported authorization field");
            return Ok(Response::builder()
                .status(400)
                .body(text_body("Unsupported Authorization field"))?)
        },
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
        Err(_) => {
            tracing::info!(user=username, "Wrong credentials");
            return Ok(Response::builder()
                .status(401)
                .header("WWW-Authenticate", "Basic realm=\"Aerogramme\"")
                .body(text_body("Wrong credentials"))?)
        },
    };

    // Build a user
    let user = User::new(username.into(), creds).await?;

    // Call router with user
    router(user, req).await 
}

async fn router(user: std::sync::Arc<User>, req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let path = req.uri().path().to_string();
    let path_segments: Vec<_> = path.split("/").filter(|s| *s != "").collect();
    let method = req.method().as_str().to_uppercase();

    match (method.as_str(), path_segments.as_slice()) {
        ("PROPFIND", []) => propfind_root(user, req).await,
        (_, [ username, ..]) if *username != user.username => return Ok(Response::builder()
            .status(403)
            .body(text_body("Accessing other user ressources is not allowed"))?),
        ("PROPFIND", [ _ ]) => propfind_home(user, &req).await,
        ("PROPFIND", [ _, "calendar" ]) => propfind_all_calendars(user, &req).await,
        ("PROPFIND", [ _, "calendar", colname ]) => propfind_this_calendar(user, &req, colname).await,
        ("PROPFIND", [ _, "calendar", colname, event ]) => propfind_event(user, req, colname, event).await,
        _ => return Ok(Response::builder()
            .status(501)
            .body(text_body("Not implemented"))?),
    }
}

/// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/'>
/// <D:prop><D:getcontenttype/><D:resourcetype/><D:displayname/><A:calendar-color/>
/// </D:prop></D:propfind>

async fn propfind_root(user: std::sync::Arc<User>, req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> { 
    tracing::info!("root");

    let r = deserialize::<PropFind<Core>>(req).await?;
    println!("r: {:?}", r);
    serialize(Multistatus::<Core, PropValue<Core>> {
        responses: vec![],
        responsedescription: Some(ResponseDescription("hello world".to_string())),
    })
}

async fn propfind_home(user: std::sync::Arc<User>, req: &Request<impl hyper::body::Body>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    tracing::info!("user home");
    Ok(Response::new(text_body("Hello World!")))
}

async fn propfind_all_calendars(user: std::sync::Arc<User>, req: &Request<impl hyper::body::Body>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    tracing::info!("calendar");
    Ok(Response::new(text_body("Hello World!")))
}

async fn propfind_this_calendar(
    user: std::sync::Arc<User>, 
    req: &Request<Incoming>,
    colname: &str
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    tracing::info!(name=colname, "selected calendar");
    Ok(Response::new(text_body("Hello World!")))
}

async fn propfind_event(
    user: std::sync::Arc<User>, 
    req: Request<Incoming>,
    colname: &str,
    event: &str,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    tracing::info!(name=colname, obj=event, "selected event");
    Ok(Response::new(text_body("Hello World!")))
}


#[allow(dead_code)]
async fn collections(_user: std::sync::Arc<User>, _req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>> {
    unimplemented!();
}


use futures::stream::TryStreamExt;
use http_body_util::{BodyStream, Empty};
use http_body_util::StreamBody;
use http_body_util::combinators::BoxBody;
use hyper::body::Frame;
use tokio_util::sync::PollSender;
use std::io::{Error, ErrorKind};
use futures::sink::SinkExt;
use tokio_util::io::{SinkWriter, CopyToBytes};


fn text_body(txt: &'static str) -> BoxBody<Bytes, std::io::Error> {
    BoxBody::new(Full::new(Bytes::from(txt)).map_err(|e| match e {}))
}

fn serialize<T: dav::QWrite + Send + 'static>(elem: T) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);

    // Build the writer
    tokio::task::spawn(async move {
        let sink = PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::BrokenPipe));
        let mut writer = SinkWriter::new(CopyToBytes::new(sink));
        let q = quick_xml::writer::Writer::new_with_indent(&mut writer, b' ', 4);
        let ns_to_apply = vec![ ("xmlns:D".into(), "DAV:".into()) ];
        let mut qwriter = dav::Writer { q, ns_to_apply };
        match elem.qwrite(&mut qwriter).await {
            Ok(_) => tracing::debug!("fully serialized object"),
            Err(e) => tracing::error!(err=?e, "failed to serialize object"),
        }
    });


    // Build the reader
    let recv = tokio_stream::wrappers::ReceiverStream::new(rx);
    let stream = StreamBody::new(recv.map(|v| Ok(Frame::data(v))));
    let boxed_body = BoxBody::new(stream);

    let response = Response::builder()
        .status(hyper::StatusCode::OK)
        .body(boxed_body)?;

    Ok(response)
}


/// Deserialize a request body to an XML request
async fn deserialize<T: dav::Node<T>>(req: Request<Incoming>) -> Result<T> {
    let stream_of_frames = BodyStream::new(req.into_body());
    let stream_of_bytes = stream_of_frames
        .try_filter_map(|frame| async move { Ok(frame.into_data().ok()) })
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));
    let async_read = tokio_util::io::StreamReader::new(stream_of_bytes);
    let async_read = std::pin::pin!(async_read);
    let mut rdr = dav::Reader::new(quick_xml::reader::NsReader::from_reader(async_read)).await?;
    let parsed = rdr.find::<T>().await?;
    Ok(parsed)
}
