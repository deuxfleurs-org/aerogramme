use anyhow::Result;
use futures::stream::{StreamExt, TryStreamExt};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyStream;
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper::body::Incoming;
use hyper::{body::Bytes, Request, Response};

use aero_collections::user::User;
use aero_dav::realization::All;
use aero_dav::coretypes as dav;
use aero_dav::versioningtypes as vers;

use crate::dav::codec;
use crate::dav::codec::{depth, deserialize, serialize, text_body, text_body_owned};
use crate::dav::multistatus::multistatus;
use crate::dav::node::{DavNode, ReportResponse};
use crate::dav::resource::RootNode;

pub(super) type HttpResponse = Response<UnsyncBoxBody<Bytes, std::io::Error>>;

pub(crate) struct Controller {
    node: Box<dyn DavNode>,
    user: User,
    req: Request<Incoming>,
}
impl Controller {
    pub(crate) async fn route(user: User, req: Request<Incoming>) -> Result<HttpResponse> {
        let path = req.uri().path().to_string();
        let path_segments: Vec<_> = path.split("/").filter(|s| *s != "").collect();
        let method = req.method().as_str().to_uppercase();

        let can_create = matches!(method.as_str(), "PUT" | "MKCOL" | "MKCALENDAR");
        let node = match (RootNode {}).fetch(&user, &path_segments, can_create).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(err=?e, "dav node fetch failed");
                return Ok(Response::builder()
                    .status(404)
                    .body(codec::text_body("Resource not found"))?);
            }
        };

        let dav_hdrs = node.dav_header();
        let ctrl = Self { node, user, req };

        match method.as_str() {
            "OPTIONS" => Ok(Response::builder()
                .status(200)
                .header("DAV", dav_hdrs)
                .header("Allow", "HEAD,GET,PUT,OPTIONS,DELETE,PROPFIND,PROPPATCH,MKCOL,COPY,MOVE,LOCK,UNLOCK,MKCALENDAR,REPORT")
                .body(codec::text_body(""))?),
            "HEAD" => {
                tracing::warn!("HEAD might not be correctly implemented: should return ETags & co");
                Ok(Response::builder()
                    .status(200)
                    .body(codec::text_body(""))?)
            },
            "GET" => ctrl.get().await,
            "PUT" => ctrl.put().await,
            "DELETE" => ctrl.delete().await,
            "PROPFIND" => ctrl.propfind().await,
            // RFC 3253 (Versioning) introduces the (extensible) REPORT method
            "REPORT" => ctrl.report().await,
            // base webdav:
            //@TODO: PROPPATCH
            //@TODO: MKCOL
            //@TODO: COPY
            //@TODO: MOVE
            // caldav:
            //@TODO: MKCALENDAR
            _ => Ok(Response::builder()
                .status(501)
                .body(codec::text_body("HTTP Method not implemented"))?),
        }
    }

    // --- Per-method functions ---

    /// REPORT has been first described in the "Versioning Extension" of WebDAV.
    /// It allows for more complex queries compared to PROPFIND.
    /// There is no common behavior for the method, it is purely defined in extensions.
    async fn report(mut self) -> Result<HttpResponse> {
        let req_report = match deserialize::<vers::Report<All>>(self.req).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(err=?e, "unable to decode REPORT body");
                return Ok(Response::builder()
                    .status(400)
                    .body(text_body("Bad request"))?);
            }
        };

        match self.node.report(&self.user, req_report).await? {
            ReportResponse::Ok(multistatus) => serialize(
                // 207 Multi-Status
                hyper::StatusCode::from_u16(207)?,
                multistatus,
            ),
            ReportResponse::Err((status, msg)) => 
                Ok(Response::builder()
                   .status(status)
                   .body(text_body_owned(msg))?),
        }
    }

    /// PROPFIND is the standard way to fetch WebDAV properties
    async fn propfind(self) -> Result<HttpResponse> {
        let depth = depth(&self.req).unwrap_or(dav::Depth::Zero);
        // The specification allows "depth: infinity" to not be implemented. "In
        // practice, support for infinite-depth requests MAY be disabled, due to
        // the performance and security concerns associated with this behavior."
        if matches!(depth, dav::Depth::Infinity) {
            return Ok(Response::builder()
                .status(501)
                .body(text_body("Depth: Infinity not implemented"))?);
        }

        // 207 Multi-Status
        let status = hyper::StatusCode::from_u16(207)?;

        // A client may submit a 'propfind' XML element in the body of the
        // request method describing what information is being requested.
        // A client may choose not to submit a request body.  An empty PROPFIND
        // request body MUST be treated as if it were an 'allprop' request.
        // @FIXME here we handle any invalid data as an allprop, an empty request is thus correctly
        // handled, but corrupted requests are also silently handled as allprop.
        let propfind = deserialize::<dav::PropFind<All>>(self.req)
            .await
            .unwrap_or_else(|_| dav::PropFind::<All>::AllProp(None));
        tracing::debug!(recv=?propfind, "inferred propfind request");

        // Collect nodes as PROPFIND is not limited to the targeted node
        let mut nodes = vec![];
        if matches!(depth, dav::Depth::One) {
            nodes.extend(self.node.children_nodes(&self.user).await?);
        }
        nodes.push(self.node);

        // `not_found` is used to indicate nodes that were requested but not found.
        // This cannot happen with this function.
        let not_found = vec![];
        serialize(
            status,
            multistatus(&self.user, nodes, not_found, propfind, None).await,
        )
    }

    async fn put(mut self) -> Result<HttpResponse> {
        let put_policy = codec::put_policy(&self.req)?;

        let stream_of_frames = BodyStream::new(self.req.into_body());
        let stream_of_bytes = stream_of_frames
            .map_ok(|frame| frame.into_data())
            .map(|obj| match obj {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(_)) => Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "conversion error",
                )),
                Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
            })
            .boxed();

        let etag = match self.node.put(put_policy, stream_of_bytes).await {
            Ok(etag) => etag,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                tracing::warn!("put pre-condition failed");
                let response = Response::builder().status(412).body(text_body(""))?;
                return Ok(response);
            }
            Err(e) => Err(e)?,
        };

        let response = Response::builder()
            .status(201)
            .header("ETag", etag)
            //.header("content-type", "application/xml; charset=\"utf-8\"")
            .body(text_body(""))?;

        Ok(response)
    }

    async fn get(self) -> Result<HttpResponse> {
        let stream_body = StreamBody::new(self.node.content().map_ok(|v| Frame::data(v)));
        let boxed_body = UnsyncBoxBody::new(stream_body);

        let mut builder = Response::builder().status(200);
        builder = builder.header("content-type", self.node.content_type());
        if let Some(etag) = self.node.etag().await {
            builder = builder.header("etag", etag);
        }
        let response = builder.body(boxed_body)?;

        Ok(response)
    }

    async fn delete(mut self) -> Result<HttpResponse> {
        self.node.delete().await?;
        let response = Response::builder()
            .status(204)
            //.header("content-type", "application/xml; charset=\"utf-8\"")
            .body(text_body(""))?;
        Ok(response)
    }
}

