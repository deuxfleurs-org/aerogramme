use anyhow::{bail, Result};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::BodyStream;
use http_body_util::Full;
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper::body::Incoming;
use hyper::{body::Bytes, Request, Response};
use std::io::{Error, ErrorKind};
use tokio_util::io::{CopyToBytes, SinkWriter};
use tokio_util::sync::PollSender;

use super::controller::HttpResponse;
use super::node::PutPolicy;
use aero_collections::unique_ident::UniqueIdent;
use aero_dav::coretypes as dav;
use aero_dav::xml as dxml;

pub struct SyncTokenUri(pub UniqueIdent);

impl SyncTokenUri {
    /// Base URI for sync tokens.
    ///
    /// Why "https://aerogramme.0"?
    /// Because tokens must be valid URI.
    /// And numeric TLD are ~mostly valid in URI (check the .42 TLD experience)
    /// and at the same time, they are not used sold by the ICANN and there is no plan to use them.
    /// So I am sure that the URL remains invalid, avoiding leaking requests to an hardcoded URL in the
    /// future.
    /// The best option would be to make it configurable ofc, so someone can put a domain name
    /// that they control, it would probably improve compatibility (maybe some WebDAV spec tells us
    /// how to handle/resolve this URI but I am not aware of that...). But that's not the plan for
    /// now. So here we are: https://aerogramme.0.
    pub const BASE_URI: &str = "https://aerogramme.0/sync/";
}

impl std::str::FromStr for SyncTokenUri {
    type Err = String;
    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw.len() != Self::BASE_URI.len() + 48 {
            return Err("invalid token length".to_string())
        }
        let id = raw[Self::BASE_URI.len()..]
            .parse::<UniqueIdent>()
            .or_else(|_| Err("cannot parse token".to_string()))?;
        Ok(Self(id))
    }
}

impl std::fmt::Display for SyncTokenUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", Self::BASE_URI, self.0)
    }
}

/// Path is a voluntarily feature limited
/// compared to the expressiveness of a UNIX path
/// For example getting parent with ../ is not supported, scheme is not supported, etc.
/// More complex support could be added later if needed by clients
#[derive(Clone)]
pub enum Path<'a> {
    Abs(Vec<&'a str>),
    Rel(Vec<&'a str>),
}
impl<'a> Path<'a> {
    pub fn new(path: &'a str) -> Result<Self> {
        // This check is naive, it does not aim at detecting all fully qualified
        // URL or protect from any attack, its only goal is to help debugging.
        if path.starts_with("http://") || path.starts_with("https://") {
            anyhow::bail!("Full URL are not supported")
        }

        let path_segments: Vec<_> = path.split("/").filter(|s| *s != "" && *s != ".").collect();
        if path.starts_with("/") {
            return Ok(Path::Abs(path_segments));
        }
        Ok(Path::Rel(path_segments))
    }

    pub fn relativize(&self, base: &Self) -> Option<Self> {
        use Path::*;
        fn strip_common_prefix<'a, 'b>(mut s1: &'b[&'a str], mut s2: &'b[&'a str]) -> (&'b[&'a str], &'b[&'a str]) {
            while !s1.is_empty() && !s2.is_empty() {
                if s1[0] == s2[0] {
                    s1 = &s1[1..];
                    s2 = &s2[1..];
                } else {
                    break
                }
            }
            (s1, s2)
        }
        match (&base, &self) {
            (Abs(_), Rel(_)) => Some(self.clone()),
            (Abs(v1), Abs(v2)) | (Rel(v1), Rel(v2)) => {
                let (s1, s2) = strip_common_prefix(v1.as_slice(), v2.as_slice());
                if s1.is_empty() {
                    Some(Rel(s2.to_vec()))
                } else {
                    None
                }
            }
            (Rel(_), Abs(_)) => None,
        }
    }

    pub fn as_single_name(&self) -> Option<&'a str> {
        match self {
            Path::Abs(v) if v.len() == 1 => Some(v[0]),
            Path::Rel(v) if v.len() == 1 => Some(v[0]),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Path::Abs(v) | Path::Rel(v) => v.is_empty(),
        }
    }
}

pub(crate) fn depth(req: &Request<impl hyper::body::Body>) -> Option<dav::Depth> {
    match req
        .headers()
        .get("Depth")
        .map(hyper::header::HeaderValue::to_str)
    {
        Some(Ok("0")) => Some(dav::Depth::Zero),
        Some(Ok("1")) => Some(dav::Depth::One),
        Some(Ok("Infinity")) => Some(dav::Depth::Infinity),
        _ => None,
    }
}

pub(crate) fn put_policy(req: &Request<impl hyper::body::Body>) -> Result<PutPolicy> {
    if let Some(maybe_txt_etag) = req
        .headers()
        .get("If-Match")
        .map(hyper::header::HeaderValue::to_str)
    {
        let etag = maybe_txt_etag?;
        let dquote_count = etag.chars().filter(|c| *c == '"').count();
        if dquote_count != 2 {
            bail!("Either If-Match value is invalid or it's not supported (only single etag is supported)");
        }

        return Ok(PutPolicy::ReplaceEtag(etag.into()));
    }

    if let Some(maybe_txt_etag) = req
        .headers()
        .get("If-None-Match")
        .map(hyper::header::HeaderValue::to_str)
    {
        let etag = maybe_txt_etag?;
        if etag == "*" {
            return Ok(PutPolicy::CreateOnly);
        }
        bail!("Either If-None-Match value is invalid or it's not supported (only asterisk is supported)")
    }

    Ok(PutPolicy::OverwriteAll)
}

pub(crate) fn text_body(txt: &'static str) -> UnsyncBoxBody<Bytes, std::io::Error> {
    UnsyncBoxBody::new(Full::new(Bytes::from(txt)).map_err(|e| match e {}))
}

pub(crate) fn text_body_owned(txt: String) -> UnsyncBoxBody<Bytes, std::io::Error> {
    UnsyncBoxBody::new(Full::new(Bytes::from(txt)).map_err(|e| match e {}))
}

//@FIXME should some of this logic be moved to aero-dav? (there is coupling
// where `ns_to_apply` below needs to match the `create_*_element` methods in
// aero_dav::xml::Writer...)
pub(crate) fn serialize<T: dxml::QWrite + Send + 'static>(
    status_ok: hyper::StatusCode,
    elem: T,
) -> Result<HttpResponse> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(1);

    // Build the writer
    tokio::task::spawn(async move {
        let sink = PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::BrokenPipe));
        let mut writer = SinkWriter::new(CopyToBytes::new(sink));
        let q = quick_xml::writer::Writer::new_with_indent(&mut writer, b' ', 4);
        let ns_to_apply = vec![
            ("xmlns:D".into(), "DAV:".into()),
            ("xmlns:C".into(), "urn:ietf:params:xml:ns:caldav".into()),
            ("xmlns:CD".into(), "urn:ietf:params:xml:ns:carddav".into()),
        ];
        let mut qwriter = dxml::Writer { q, ns_to_apply };
        let decl =
            quick_xml::events::BytesDecl::from_start(quick_xml::events::BytesStart::from_content(
                "xml version=\"1.0\" encoding=\"utf-8\"",
                0,
            ));
        match qwriter
            .q
            .write_event_async(quick_xml::events::Event::Decl(decl))
            .await
        {
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
    let boxed_body = UnsyncBoxBody::new(stream);

    let response = Response::builder()
        .status(status_ok)
        .header("content-type", "application/xml; charset=\"utf-8\"")
        .body(boxed_body)?;

    Ok(response)
}

/// Deserialize a request body to an XML request
pub(crate) async fn deserialize<T: dxml::Node<T>>(req: Request<Incoming>) -> Result<T> {
    let stream_of_frames = BodyStream::new(req.into_body());
    let stream_of_bytes = stream_of_frames
        .map_ok(|frame| frame.into_data())
        .map(|obj| match obj {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "conversion error",
            )),
            Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
        });
    let async_read = tokio_util::io::StreamReader::new(stream_of_bytes);
    let async_read = std::pin::pin!(async_read);
    let mut rdr = dxml::Reader::new(quick_xml::reader::NsReader::from_reader(async_read)).await?;
    let parsed = rdr.find::<T>().await?;
    Ok(parsed)
}
