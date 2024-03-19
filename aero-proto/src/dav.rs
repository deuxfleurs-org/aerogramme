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
use rustls_pemfile::{certs, private_key};

use aero_user::config::{DavConfig, DavUnsecureConfig};
use aero_user::login::ArcLoginProvider;
use aero_collections::user::User;
use aero_dav::types as dav;
use aero_dav::caltypes as cal;
use aero_dav::acltypes as acl;
use aero_dav::realization::{All, self as all};
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
            .header("ALLOW", "HEAD,GET,PUT,OPTIONS,DELETE,PROPFIND,PROPPATCH,MKCOL,COPY,MOVE,LOCK,UNLOCK")
            .body(text_body(""))?),
        "HEAD" | "GET" => {
            tracing::warn!("HEAD+GET not correctly implemented");
            return Ok(Response::builder()
                .status(404)
                .body(text_body(""))?)
        },
        "PROPFIND" => propfind(user, req, node).await,
        _ => return Ok(Response::builder()
            .status(501)
            .body(text_body("Not implemented"))?),
    }
}

/// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/'>
///   <D:prop>
///     <D:getcontenttype/>
///     <D:resourcetype/>
///     <D:displayname/>
///     <A:calendar-color/>
///   </D:prop>
/// </D:propfind>


/// <D:propfind xmlns:D='DAV:' xmlns:A='http://apple.com/ns/ical/' xmlns:C='urn:ietf:params:xml:ns:caldav'>
///   <D:prop>
///     <D:resourcetype/>
///     <D:owner/>
///     <D:displayname/>
///     <D:current-user-principal/>
///     <D:current-user-privilege-set/>
///     <A:calendar-color/>
///     <C:calendar-home-set/>
///   </D:prop>
/// </D:propfind>

const ALLPROP: [dav::PropertyRequest<All>; 10] = [
    dav::PropertyRequest::CreationDate,
    dav::PropertyRequest::DisplayName,
    dav::PropertyRequest::GetContentLanguage,
    dav::PropertyRequest::GetContentLength,
    dav::PropertyRequest::GetContentType,
    dav::PropertyRequest::GetEtag,
    dav::PropertyRequest::GetLastModified,
    dav::PropertyRequest::LockDiscovery,
    dav::PropertyRequest::ResourceType,
    dav::PropertyRequest::SupportedLock,
];

async fn propfind(user: std::sync::Arc<User>, req: Request<Incoming>, node: Box<dyn DavNode>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> { 
    let depth = depth(&req);
    let status = hyper::StatusCode::from_u16(207)?;

    // A client may choose not to submit a request body.  An empty PROPFIND
    // request body MUST be treated as if it were an 'allprop' request.
    // @FIXME here we handle any invalid data as an allprop, an empty request is thus correctly
    // handled, but corrupted requests are also silently handled as allprop.
    let propfind = deserialize::<dav::PropFind<All>>(req).await.unwrap_or_else(|_| dav::PropFind::<All>::AllProp(None));
    tracing::debug!(recv=?propfind, "inferred propfind request");

    if matches!(propfind, dav::PropFind::PropName) {
        return serialize(status, node.multistatus_name(&user, depth));
    }

    let propname = match propfind {
        dav::PropFind::PropName => unreachable!(),
        dav::PropFind::AllProp(None) => dav::PropName(ALLPROP.to_vec()),
        dav::PropFind::AllProp(Some(dav::Include(mut include))) => {
            include.extend_from_slice(&ALLPROP);
            dav::PropName(include)
        },
        dav::PropFind::Prop(inner) => inner,
    };

    serialize(status, node.multistatus_val(&user, propname, depth))
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

fn serialize<T: dxml::QWrite + Send + 'static>(status_ok: hyper::StatusCode, elem: T) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);

    // Build the writer
    tokio::task::spawn(async move {
        let sink = PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::BrokenPipe));
        let mut writer = SinkWriter::new(CopyToBytes::new(sink));
        let q = quick_xml::writer::Writer::new_with_indent(&mut writer, b' ', 4);
        let ns_to_apply = vec![ ("xmlns:D".into(), "DAV:".into()), ("xmlns:C".into(), "urn:ietf:params:xml:ns:caldav".into()) ];
        let mut qwriter = dxml::Writer { q, ns_to_apply };
        let decl = quick_xml::events::BytesDecl::from_start(quick_xml::events::BytesStart::from_content("xml version=\"1.0\" encoding=\"utf-8\"", 0));
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
        .status(status_ok)
        .header("content-type", "application/xml; charset=\"utf-8\"")
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
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All>;
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>>;

    // ----- common

    /// building DAV responses
    fn multistatus_name(&self, user: &ArcUser, depth: dav::Depth) -> dav::Multistatus<All> {
        let mut names = vec![(self.path(user), self.supported_properties(user))];
        if matches!(depth, dav::Depth::One | dav::Depth::Infinity) {
            names.extend(self.children(user).iter().map(|c| (c.path(user), c.supported_properties(user))));
        }

        dav::Multistatus::<All> {
            responses: names.into_iter().map(|(url, names)| dav::Response {
                status_or_propstat: dav::StatusOrPropstat::PropStat(
                    dav::Href(url),
                    vec![dav::PropStat {
                        prop: dav::AnyProp(names.0.into_iter().map(dav::AnyProperty::Request).collect()),
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

    fn multistatus_val(&self, user: &ArcUser, props: dav::PropName<All>, depth: dav::Depth) -> dav::Multistatus<All> {
        // Collect properties
        let mut values = vec![(self.path(user), self.properties(user, props.clone()))];
        if matches!(depth, dav::Depth::One | dav::Depth::Infinity) {
            values.extend(self
                .children(user)
                .iter()
                .map(|c| (c.path(user), c.properties(user, props.clone())))
            );
        }

        // Separate FOUND from NOT FOUND
        let values: Vec<_> = values.into_iter().map(|(path, anyprop)| {
            let mut prop_desc = vec![];
            let (found, not_found): (Vec<_>, Vec<_>) = anyprop.into_iter().partition(|v| matches!(v, dav::AnyProperty::Value(_)));
            if !found.is_empty() {
                prop_desc.push((hyper::StatusCode::OK, dav::AnyProp(found)))
            }
            if !not_found.is_empty() {
                prop_desc.push((hyper::StatusCode::NOT_FOUND, dav::AnyProp(not_found)))
            }
            (path, prop_desc)
        }).collect();

        // Build response
        dav::Multistatus::<All> {
            responses: values.into_iter().map(|(url, propdesc)| dav::Response {
                status_or_propstat: dav::StatusOrPropstat::PropStat(
                    dav::Href(url),
                    propdesc.into_iter().map(|(status, prop)| dav::PropStat {
                        prop,
                        status: dav::Status(status),
                        error: None,
                        responsedescription: None,
                    }).collect(),
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
        "/".into()
    }

    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(HomeNode { })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(acl::PropertyRequest::CurrentUserPrincipal)),
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>> {
        prop.0.into_iter().map(|n| match n {
            dav::PropertyRequest::DisplayName => dav::AnyProperty::Value(dav::Property::DisplayName("DAV Root".to_string())),
            dav::PropertyRequest::ResourceType => dav::AnyProperty::Value(dav::Property::ResourceType(vec![
                dav::ResourceType::Collection,
            ])),
            dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(acl::PropertyRequest::CurrentUserPrincipal)) =>
                dav::AnyProperty::Value(dav::Property::Extension(all::Property::Acl(acl::Property::CurrentUserPrincipal(acl::User::Authenticated(dav::Href(HomeNode{}.path(user))))))),
            v => dav::AnyProperty::Request(v),
        }).collect()
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
        format!("/{}/", user.username)
    }

    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(CalendarListNode { })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarHomeSet)),
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>> {
        prop.0.into_iter().map(|n| match n {
            dav::PropertyRequest::DisplayName => dav::AnyProperty::Value(dav::Property::DisplayName(format!("{} home", user.username))),
            dav::PropertyRequest::ResourceType => dav::AnyProperty::Value(dav::Property::ResourceType(vec![
                dav::ResourceType::Collection,
                dav::ResourceType::Extension(all::ResourceType::Acl(acl::ResourceType::Principal)),
            ])),
            dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarHomeSet)) => 
                dav::AnyProperty::Value(dav::Property::Extension(all::Property::Cal(cal::Property::CalendarHomeSet(dav::Href(CalendarListNode{}.path(user)))))),
            v => dav::AnyProperty::Request(v),
        }).collect()
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
        format!("/{}/calendar/", user.username)
    }

    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(CalendarNode { name: "personal".into() })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>> {
        prop.0.into_iter().map(|n| match n {
            dav::PropertyRequest::DisplayName => dav::AnyProperty::Value(dav::Property::DisplayName(format!("{} calendars", user.username))),
            dav::PropertyRequest::ResourceType => dav::AnyProperty::Value(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
            dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
            v => dav::AnyProperty::Request(v),
        }).collect()
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
            let child = Box::new(EventNode { 
                calendar: self.name.to_string(),
                event_file: "something.ics".to_string(),
            });
            return child.fetch(user, &path[1..])
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/", user.username, self.name)
    }

    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![Box::new(EventNode { calendar: self.name.to_string(), event_file: "something.ics".into() })]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>> {
        prop.0.into_iter().map(|n| match n {
            dav::PropertyRequest::DisplayName =>  dav::AnyProperty::Value(dav::Property::DisplayName(format!("{} calendar", self.name))),
            dav::PropertyRequest::ResourceType =>  dav::AnyProperty::Value(dav::Property::ResourceType(vec![
                dav::ResourceType::Collection,
                dav::ResourceType::Extension(all::ResourceType::Cal(cal::ResourceType::Calendar)),
            ])),
            //dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
            //@FIXME seems wrong but seems to be what Thunderbird expects...
            dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("text/calendar".into())),
            v => dav::AnyProperty::Request(v),
        }).collect()
    }
}

struct EventNode {
    calendar: String,
    event_file: String,
}
impl DavNode for EventNode {
    fn fetch(self: Box<Self>, user: &ArcUser, path: &[&str]) -> Result<Box<dyn DavNode>> {
        if path.len() == 0 {
            return Ok(self)
        }

        Err(anyhow!("Not found"))
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/{}", user.username, self.calendar, self.event_file)
    }

    fn children(&self, user: &ArcUser) -> Vec<Box<dyn DavNode>> {
        vec![]
    }
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>> {
        prop.0.into_iter().map(|n| match n {
            dav::PropertyRequest::DisplayName => dav::AnyProperty::Value(dav::Property::DisplayName(format!("{} event", self.event_file))),
            dav::PropertyRequest::ResourceType => dav::AnyProperty::Value(dav::Property::ResourceType(vec![])),
            dav::PropertyRequest::GetContentType =>  dav::AnyProperty::Value(dav::Property::GetContentType("text/calendar".into())),
            v => dav::AnyProperty::Request(v),
        }).collect()
    }
}


