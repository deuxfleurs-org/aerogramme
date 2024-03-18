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
use aero_dav::types as dav;
use aero_dav::caltypes as cal;
use aero_dav::realization::Calendar;
use aero_dav::xml as dxml;

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
    let node = match Box::new(RootNode {}).fetch(&user, &path_segments) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(err=?e, "dav node fetch failed");
            return Ok(Response::builder()
                .status(404)
                .body(text_body("Resource not found"))?)
        }
    };

    match method.as_str() {
        "OPTIONS" => return Ok(Response::builder()
            .status(200)
            .header("DAV", "1")
            .body(text_body(""))?),
        "PROPFIND" => propfind(user, req, node).await,
        _ => return Ok(Response::builder()
            .status(501)
            .body(text_body("Not implemented"))?),
    }
}

/// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/'>
/// <D:prop><D:getcontenttype/><D:resourcetype/><D:displayname/><A:calendar-color/>
/// </D:prop></D:propfind>
const SUPPORTED_PROPNAME: [dav::PropertyRequest<Calendar>; 2] = [
    dav::PropertyRequest::DisplayName,
    dav::PropertyRequest::ResourceType,
];

async fn propfind(user: std::sync::Arc<User>, req: Request<Incoming>, node: Box<dyn DavNode>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> { 
    let depth = depth(&req);


    /*let supported_propname = vec![
        dav::PropertyRequest::DisplayName,
        dav::PropertyRequest::ResourceType,
    ];*/

    // A client may choose not to submit a request body.  An empty PROPFIND
    // request body MUST be treated as if it were an 'allprop' request.
    // @FIXME here we handle any invalid data as an allprop, an empty request is thus correctly
    // handled, but corrupted requests are also silently handled as allprop.
    let propfind = deserialize::<dav::PropFind<Calendar>>(req).await.unwrap_or_else(|_| dav::PropFind::<Calendar>::AllProp(None));
    tracing::debug!(recv=?propfind, "inferred propfind request");

    if matches!(propfind, dav::PropFind::PropName) {
        return serialize(node.multistatus_name(&user, depth));
    }

    let propname = match propfind {
        dav::PropFind::PropName => unreachable!(),
        dav::PropFind::AllProp(None) => dav::PropName(SUPPORTED_PROPNAME.to_vec()),
        dav::PropFind::AllProp(Some(dav::Include(mut include))) => {
            include.extend_from_slice(&SUPPORTED_PROPNAME);
            dav::PropName(include)
        },
        dav::PropFind::Prop(inner) => inner,
    };

    serialize(node.multistatus_val(&user, &propname, depth))
}

#[allow(dead_code)]
async fn collections(_user: std::sync::Arc<User>, _req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>> {
    unimplemented!();
}


// ---- HTTP DAV Binding

use futures::stream::TryStreamExt;
use http_body_util::BodyStream;
use http_body_util::StreamBody;
use http_body_util::combinators::BoxBody;
use hyper::body::Frame;
use tokio_util::sync::PollSender;
use std::io::{Error, ErrorKind};
use futures::sink::SinkExt;
use tokio_util::io::{SinkWriter, CopyToBytes};

fn depth(req: &Request<impl hyper::body::Body>) -> dav::Depth {
    match req.headers().get("Depth").map(hyper::header::HeaderValue::to_str) {
        Some(Ok("0")) => dav::Depth::Zero,
        Some(Ok("1")) => dav::Depth::One,
        _ => dav::Depth::Infinity,
    }
}

fn text_body(txt: &'static str) -> BoxBody<Bytes, std::io::Error> {
    BoxBody::new(Full::new(Bytes::from(txt)).map_err(|e| match e {}))
}

fn serialize<T: dxml::QWrite + Send + 'static>(elem: T) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);

    // Build the writer
    tokio::task::spawn(async move {
        let sink = PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::BrokenPipe));
        let mut writer = SinkWriter::new(CopyToBytes::new(sink));
        let q = quick_xml::writer::Writer::new_with_indent(&mut writer, b' ', 4);
        let ns_to_apply = vec![ ("xmlns:D".into(), "DAV:".into()), ("xmlns:C".into(), "urn:ietf:params:xml:ns:caldav".into()) ];
        let mut qwriter = dxml::Writer { q, ns_to_apply };
        let decl = quick_xml::events::BytesDecl::from_start(quick_xml::events::BytesStart::from_content("xml encoding='utf-8' version='1.0'", 0));
        match qwriter.q.write_event_async(quick_xml::events::Event::Decl(decl)).await {
            Ok(_) => (),
            Err(e) => tracing::error!(err=?e, "unable to write XML declaration <?xml ... >"),
        }
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
async fn deserialize<T: dxml::Node<T>>(req: Request<Incoming>) -> Result<T> {
    let stream_of_frames = BodyStream::new(req.into_body());
    let stream_of_bytes = stream_of_frames
        .try_filter_map(|frame| async move { Ok(frame.into_data().ok()) })
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));
    let async_read = tokio_util::io::StreamReader::new(stream_of_bytes);
    let async_read = std::pin::pin!(async_read);
    let mut rdr = dxml::Reader::new(quick_xml::reader::NsReader::from_reader(async_read)).await?;
    let parsed = rdr.find::<T>().await?;
    Ok(parsed)
}

//---

type ArcUser = std::sync::Arc<User>;
trait DavNode: Send {
    // ------- specialized logic

    // recurence
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>>;
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>>;

    // node properties
    fn path(&self, user: &ArcUser) -> String;
    fn name(&self, user: &ArcUser) -> String;
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar>;
    fn properties(&self, user: &ArcUser, props: &dav::PropName<Calendar>) -> dav::PropValue<Calendar>;

    // ----- common

    /// building DAV responses
    fn multistatus_name(&self, user: &ArcUser, depth: dav::Depth) -> dav::Multistatus<Calendar, dav::PropName<Calendar>> {
        let mut names = vec![(".".into(), self.supported_properties(user))];
        if matches!(depth, dav::Depth::One | dav::Depth::Infinity) {
            names.extend(self.children(user).iter().map(|c| (format!("./{}", c.name(user)), c.supported_properties(user))));
        }

        dav::Multistatus::<Calendar, dav::PropName<Calendar>> {
            responses: names.into_iter().map(|(url, names)| dav::Response {
                status_or_propstat: dav::StatusOrPropstat::PropStat(
                    dav::Href(url),
                    vec![dav::PropStat {
                        prop: names,
                        status: dav::Status(hyper::StatusCode::OK),
                        error: None,
                        responsedescription: None,
                    }],
                ),
                error: None,
                location: None,
                responsedescription: None,
            }).collect(),
            responsedescription: None,
        }
    }

    fn multistatus_val(&self, user: &ArcUser, props: &dav::PropName<Calendar>, depth: dav::Depth) -> dav::Multistatus<Calendar, dav::PropValue<Calendar>> {
        let mut values = vec![(".".into(), self.properties(user, props))];
        if matches!(depth, dav::Depth::One | dav::Depth::Infinity) {
            values.extend(self
                .children(user)
                .iter()
                .map(|c| (format!("./{}", c.name(user)), c.properties(user, props)))
            );
        }

        dav::Multistatus::<Calendar, dav::PropValue<Calendar>> {
            responses: values.into_iter().map(|(url, propval)| dav::Response {
                status_or_propstat: dav::StatusOrPropstat::PropStat(
                    dav::Href(url),
                    vec![dav::PropStat {
                        prop: propval,
                        status: dav::Status(hyper::StatusCode::OK),
                        error: None,
                        responsedescription: None,
                    }],
                ),
                error: None,
                location: None,
                responsedescription: None,
            }).collect(),
            responsedescription: None,
        }
    }
}

struct RootNode {}
impl DavNode for RootNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        if path[0] == user.username {
            let child = Box::new(HomeNode {});
            return child.fetch(user, &path[1..])
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        todo!();
    }

    fn name(&self, _user: &ArcUser) -> String {
        "/".into()
    }
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(HomeNode { })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: &dav::PropName<Calendar>) -> dav::PropValue<Calendar> {
        dav::PropValue(prop.0.iter().filter_map(|n| match n {
            dav::PropertyRequest::DisplayName => Some(dav::Property::DisplayName("DAV Root".to_string())),
            dav::PropertyRequest::ResourceType => Some(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
            _ => None,
        }).collect())
    }
}

struct HomeNode {}
impl DavNode for HomeNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        if path[0] == "calendar" {
            let child = Box::new(CalendarListNode {});
            return child.fetch(user, &path[1..])
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        todo!();
    }

    fn name(&self, user: &ArcUser) -> String {
        format!("{}/", user.username)
    }
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(CalendarListNode { })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: &dav::PropName<Calendar>) -> dav::PropValue<Calendar> {
        dav::PropValue(prop.0.iter().filter_map(|n| match n {
            dav::PropertyRequest::DisplayName => Some(dav::Property::DisplayName(format!("{} home", user.username))),
            dav::PropertyRequest::ResourceType => Some(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
            _ => None,
        }).collect())
    }
}

struct CalendarListNode {}
impl DavNode for CalendarListNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        //@FIXME hardcoded logic
        if path[0] == "personal" {
            let child = Box::new(CalendarNode { name: "personal".to_string() });
            return child.fetch(user, &path[1..])
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        todo!();
    }

    fn name(&self, _user: &ArcUser) -> String {
        "calendar/".into()
    }
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(CalendarNode { name: "personal".into() })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: &dav::PropName<Calendar>) -> dav::PropValue<Calendar> {
        dav::PropValue(prop.0.iter().filter_map(|n| match n {
            dav::PropertyRequest::DisplayName => Some(dav::Property::DisplayName(format!("{} calendars", user.username))),
            dav::PropertyRequest::ResourceType => Some(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
            _ => None,
        }).collect())
    }
}

struct CalendarNode {
    name: String,
}
impl DavNode for CalendarNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        //@FIXME hardcoded logic
        if path[0] == "something.ics" {
            let child = Box::new(EventNode { file: "something.ics".to_string() });
            return child.fetch(user, &path[1..])
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        todo!();
    }

    fn name(&self, _user: &ArcUser) -> String {
        format!("{}/", self.name)
    }
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(EventNode { file: "something.ics".into() })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: &dav::PropName<Calendar>) -> dav::PropValue<Calendar> {
        dav::PropValue(prop.0.iter().filter_map(|n| match n {
            dav::PropertyRequest::DisplayName => Some(dav::Property::DisplayName(format!("{} calendar", self.name))),
            dav::PropertyRequest::ResourceType => Some(dav::Property::ResourceType(vec![
                dav::ResourceType::Collection,
                dav::ResourceType::Extension(cal::ResourceType::Calendar),
            ])),
            _ => None,
        }).collect())
    }
}

struct EventNode {
    file: String,
}
impl DavNode for EventNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        todo!();
    }

    fn name(&self, _user: &ArcUser) -> String {
        self.file.to_string()
    }
    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<Calendar> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: &dav::PropName<Calendar>) -> dav::PropValue<Calendar> {
        dav::PropValue(prop.0.iter().filter_map(|n| match n {
            dav::PropertyRequest::DisplayName => Some(dav::Property::DisplayName(format!("{} event", self.file))),
            dav::PropertyRequest::ResourceType => Some(dav::Property::ResourceType(vec![])),
            _ => None,
        }).collect())
    }
}


