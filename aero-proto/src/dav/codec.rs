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
use aero_dav::types as dav;
use aero_dav::xml as dxml;

pub(crate) fn depth(req: &Request<impl hyper::body::Body>) -> dav::Depth {
    match req
        .headers()
        .get("Depth")
        .map(hyper::header::HeaderValue::to_str)
    {
        Some(Ok("0")) => dav::Depth::Zero,
        Some(Ok("1")) => dav::Depth::One,
        Some(Ok("Infinity")) => dav::Depth::Infinity,
        _ => dav::Depth::Zero,
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
