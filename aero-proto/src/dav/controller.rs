use anyhow::Result;
use futures::stream::{StreamExt, TryStreamExt};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyStream;
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper::body::Incoming;
use hyper::{body::Bytes, Request, Response};

use aero_collections::{davdag::Token, user::User};
use aero_dav::caltypes as cal;
use aero_dav::realization::{self, All};
use aero_dav::synctypes as sync;
use aero_dav::types as dav;
use aero_dav::versioningtypes as vers;
use aero_ical::query::is_component_match;

use crate::dav::codec;
use crate::dav::codec::{depth, deserialize, serialize, text_body};
use crate::dav::node::DavNode;
use crate::dav::resource::{RootNode, BASE_TOKEN_URI};

pub(super) type ArcUser = std::sync::Arc<User>;
pub(super) type HttpResponse = Response<UnsyncBoxBody<Bytes, std::io::Error>>;

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

pub(crate) struct Controller {
    node: Box<dyn DavNode>,
    user: std::sync::Arc<User>,
    req: Request<Incoming>,
}
impl Controller {
    pub(crate) async fn route(
        user: std::sync::Arc<User>,
        req: Request<Incoming>,
    ) -> Result<HttpResponse> {
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

        let ctrl = Self { node, user, req };

        match method.as_str() {
            "OPTIONS" => Ok(Response::builder()
                .status(200)
                .header("DAV", "1")
                .header("Allow", "HEAD,GET,PUT,OPTIONS,DELETE,PROPFIND,PROPPATCH,MKCOL,COPY,MOVE,LOCK,UNLOCK,MKCALENDAR,REPORT")
                .body(codec::text_body(""))?),
            "HEAD" => {
                tracing::warn!("HEAD not correctly implemented");
                Ok(Response::builder()
                    .status(404)
                    .body(codec::text_body(""))?)
            },
            "GET" => ctrl.get().await,
            "PUT" => ctrl.put().await,
            "DELETE" => ctrl.delete().await,
            "PROPFIND" => ctrl.propfind().await,
            "REPORT" => ctrl.report().await,
            _ => Ok(Response::builder()
                .status(501)
                .body(codec::text_body("HTTP Method not implemented"))?),
        }
    }

    // --- Per-method functions ---

    /// REPORT has been first described in the "Versioning Extension" of WebDAV
    /// It allows more complex queries compared to PROPFIND
    ///
    /// Note: current implementation is not generic at all, it is heavily tied to CalDAV.
    /// A rewrite would be required to make it more generic (with the extension system that has
    /// been introduced in aero-dav)
    async fn report(self) -> Result<HttpResponse> {
        let status = hyper::StatusCode::from_u16(207)?;

        let cal_report = match deserialize::<vers::Report<All>>(self.req).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(err=?e, "unable to decode REPORT body");
                return Ok(Response::builder()
                    .status(400)
                    .body(text_body("Bad request"))?);
            }
        };

        // Internal representation that will handle processed request
        let (mut ok_node, mut not_found) = (Vec::new(), Vec::new());
        let calprop: Option<cal::CalendarSelector<All>>;
        let extension: Option<realization::Multistatus>;

        // Extracting request information
        match cal_report {
            vers::Report::Extension(realization::ReportType::Cal(cal::ReportType::Multiget(m))) => {
                // Multiget is really like a propfind where Depth: 0|1|Infinity is replaced by an arbitrary
                // list of URLs
                // Getting the list of nodes
                for h in m.href.into_iter() {
                    let maybe_collected_node = match Path::new(h.0.as_str()) {
                        Ok(Path::Abs(p)) => RootNode {}
                            .fetch(&self.user, p.as_slice(), false)
                            .await
                            .or(Err(h)),
                        Ok(Path::Rel(p)) => self
                            .node
                            .fetch(&self.user, p.as_slice(), false)
                            .await
                            .or(Err(h)),
                        Err(_) => Err(h),
                    };

                    match maybe_collected_node {
                        Ok(v) => ok_node.push(v),
                        Err(h) => not_found.push(h),
                    };
                }
                calprop = m.selector;
                extension = None;
            }
            vers::Report::Extension(realization::ReportType::Cal(cal::ReportType::Query(q))) => {
                calprop = q.selector;
                extension = None;
                ok_node = apply_filter(self.node.children(&self.user).await, &q.filter)
                    .try_collect()
                    .await?;
            }
            vers::Report::Extension(realization::ReportType::Sync(sync_col)) => {
                calprop = Some(cal::CalendarSelector::Prop(sync_col.prop));

                if sync_col.limit.is_some() {
                    tracing::warn!("limit is not supported, ignoring");
                }
                if matches!(sync_col.sync_level, sync::SyncLevel::Infinite) {
                    tracing::debug!("aerogramme calendar collections are not nested");
                }

                let token = match sync_col.sync_token {
                    sync::SyncTokenRequest::InitialSync => None,
                    sync::SyncTokenRequest::IncrementalSync(token_raw) => {
                        // parse token
                        if token_raw.len() != BASE_TOKEN_URI.len() + 48 {
                            anyhow::bail!("invalid token length")
                        }
                        let token = token_raw[BASE_TOKEN_URI.len()..]
                            .parse()
                            .or(Err(anyhow::anyhow!("can't parse token")))?;
                        Some(token)
                    }
                };
                // do the diff
                let new_token: Token;
                (new_token, ok_node, not_found) = match self.node.diff(token).await {
                    Ok(t) => t,
                    Err(e) => match e.kind() {
                        std::io::ErrorKind::NotFound => return Ok(Response::builder()
                            .status(410)
                            .body(text_body("Diff failed, token might be expired"))?),
                        _ => return Ok(Response::builder()
                                .status(500)
                                .body(text_body("Server error, maybe this operation is not supported on this collection"))?),
                    },
                };
                extension = Some(realization::Multistatus::Sync(sync::Multistatus {
                    sync_token: sync::SyncToken(format!("{}{}", BASE_TOKEN_URI, new_token)),
                }));
            }
            _ => {
                return Ok(Response::builder()
                    .status(501)
                    .body(text_body("Not implemented"))?)
            }
        };

        // Getting props
        let props = match calprop {
            None | Some(cal::CalendarSelector::AllProp) => Some(dav::PropName(ALLPROP.to_vec())),
            Some(cal::CalendarSelector::PropName) => None,
            Some(cal::CalendarSelector::Prop(inner)) => Some(inner),
        };

        serialize(
            status,
            Self::multistatus(&self.user, ok_node, not_found, props, extension).await,
        )
    }

    /// PROPFIND is the standard way to fetch WebDAV properties
    async fn propfind(self) -> Result<HttpResponse> {
        let depth = depth(&self.req);
        if matches!(depth, dav::Depth::Infinity) {
            return Ok(Response::builder()
                .status(501)
                .body(text_body("Depth: Infinity not implemented"))?);
        }

        let status = hyper::StatusCode::from_u16(207)?;

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
        if matches!(depth, dav::Depth::One | dav::Depth::Infinity) {
            nodes.extend(self.node.children(&self.user).await);
        }
        nodes.push(self.node);

        // Expand properties request
        let propname = match propfind {
            dav::PropFind::PropName => None,
            dav::PropFind::AllProp(None) => Some(dav::PropName(ALLPROP.to_vec())),
            dav::PropFind::AllProp(Some(dav::Include(mut include))) => {
                include.extend_from_slice(&ALLPROP);
                Some(dav::PropName(include))
            }
            dav::PropFind::Prop(inner) => Some(inner),
        };

        // Not Found is currently impossible considering the way we designed this function
        let not_found = vec![];
        serialize(
            status,
            Self::multistatus(&self.user, nodes, not_found, propname, None).await,
        )
    }

    async fn put(self) -> Result<HttpResponse> {
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

    async fn delete(self) -> Result<HttpResponse> {
        self.node.delete().await?;
        let response = Response::builder()
            .status(204)
            //.header("content-type", "application/xml; charset=\"utf-8\"")
            .body(text_body(""))?;
        Ok(response)
    }

    // --- Common utility functions ---
    /// Build a multistatus response from a list of DavNodes
    async fn multistatus(
        user: &ArcUser,
        nodes: Vec<Box<dyn DavNode>>,
        not_found: Vec<dav::Href>,
        props: Option<dav::PropName<All>>,
        extension: Option<realization::Multistatus>,
    ) -> dav::Multistatus<All> {
        // Collect properties on existing objects
        let mut responses: Vec<dav::Response<All>> = match props {
            Some(props) => {
                futures::stream::iter(nodes)
                    .then(|n| n.response_props(user, props.clone()))
                    .collect()
                    .await
            }
            None => nodes
                .into_iter()
                .map(|n| n.response_propname(user))
                .collect(),
        };

        // Register not found objects only if relevant
        if !not_found.is_empty() {
            responses.push(dav::Response {
                status_or_propstat: dav::StatusOrPropstat::Status(
                    not_found,
                    dav::Status(hyper::StatusCode::NOT_FOUND),
                ),
                error: None,
                location: None,
                responsedescription: None,
            });
        }

        // Build response
        dav::Multistatus::<All> {
            responses,
            responsedescription: None,
            extension,
        }
    }
}

/// Path is a voluntarily feature limited
/// compared to the expressiveness of a UNIX path
/// For example getting parent with ../ is not supported, scheme is not supported, etc.
/// More complex support could be added later if needed by clients
enum Path<'a> {
    Abs(Vec<&'a str>),
    Rel(Vec<&'a str>),
}
impl<'a> Path<'a> {
    fn new(path: &'a str) -> Result<Self> {
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
}

//@FIXME naive implementation, must be refactored later
use futures::stream::Stream;
fn apply_filter<'a>(
    nodes: Vec<Box<dyn DavNode>>,
    filter: &'a cal::Filter,
) -> impl Stream<Item = std::result::Result<Box<dyn DavNode>, std::io::Error>> + 'a {
    futures::stream::iter(nodes).filter_map(move |single_node| async move {
        // Get ICS
        let chunks: Vec<_> = match single_node.content().try_collect().await {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let raw_ics = chunks.iter().fold(String::new(), |mut acc, single_chunk| {
            let str_fragment = std::str::from_utf8(single_chunk.as_ref());
            acc.extend(str_fragment);
            acc
        });

        // Parse ICS
        let ics = match icalendar::parser::read_calendar(&raw_ics) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(err=?e, "Unable to parse ICS in calendar-query");
                return Some(Err(std::io::Error::from(std::io::ErrorKind::InvalidData)));
            }
        };

        // Do checks
        // @FIXME: icalendar does not consider VCALENDAR as a component
        // but WebDAV does...
        // Build a fake VCALENDAR component for icalendar compatibility, it's a hack
        let root_filter = &filter.0;
        let fake_vcal_component = icalendar::parser::Component {
            name: cal::Component::VCalendar.as_str().into(),
            properties: ics.properties,
            components: ics.components,
        };
        tracing::debug!(filter=?root_filter, "calendar-query filter");

        // Adjust return value according to filter
        match is_component_match(
            &fake_vcal_component,
            &[fake_vcal_component.clone()],
            root_filter,
        ) {
            true => Some(Ok(single_node)),
            _ => None,
        }
    })
}
