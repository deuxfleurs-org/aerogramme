use anyhow::Result;
use http_body_util::combinators::BoxBody;
use hyper::body::Incoming;
use hyper::{Request, Response, body::Bytes};

use aero_collections::user::User;
use aero_dav::types as dav;
use aero_dav::realization::All;
use aero_dav::caltypes as cal;

use crate::dav::codec::{serialize, deserialize, depth, text_body};
use crate::dav::node::DavNode;
use crate::dav::resource::RootNode;
use crate::dav::codec;

type ArcUser = std::sync::Arc<User>;

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
    pub(crate) async fn route(user: std::sync::Arc<User>, req: Request<Incoming>) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
        let path = req.uri().path().to_string();
        let path_segments: Vec<_> = path.split("/").filter(|s| *s != "").collect();
        let method = req.method().as_str().to_uppercase();

        let node = match (RootNode {}).fetch(&user, &path_segments).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(err=?e, "dav node fetch failed");
                return Ok(Response::builder()
                    .status(404)
                    .body(codec::text_body("Resource not found"))?)
            }
        };
        let ctrl = Self { node, user, req };

        match method.as_str() {
            "OPTIONS" => Ok(Response::builder()
                .status(200)
                .header("DAV", "1")
                .header("Allow", "HEAD,GET,PUT,OPTIONS,DELETE,PROPFIND,PROPPATCH,MKCOL,COPY,MOVE,LOCK,UNLOCK,MKCALENDAR,REPORT")
                .body(codec::text_body(""))?),
            "HEAD" | "GET" => {
                tracing::warn!("HEAD+GET not correctly implemented");
                Ok(Response::builder()
                    .status(404)
                    .body(codec::text_body(""))?)
            },
            "PUT" => {
                todo!();
            },
            "DELETE" => {
                todo!();
            },
            "PROPFIND" => ctrl.propfind().await,
            "REPORT" => ctrl.report().await,
            _ => Ok(Response::builder()
                .status(501)
                .body(codec::text_body("HTTP Method not implemented"))?),
        }
    }


    // --- Public API ---

    /// REPORT has been first described in the "Versioning Extension" of WebDAV
    /// It allows more complex queries compared to PROPFIND
    ///
    /// Note: current implementation is not generic at all, it is heavily tied to CalDAV.
    /// A rewrite would be required to make it more generic (with the extension system that has
    /// been introduced in aero-dav)
    async fn report(self) -> Result<Response<BoxBody<Bytes, std::io::Error>>> { 
        let status = hyper::StatusCode::from_u16(207)?;

        let report = match deserialize::<cal::Report<All>>(self.req).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(err=?e, "unable to decode REPORT body");
                return Ok(Response::builder()
                          .status(400)
                          .body(text_body("Bad request"))?)
            }
        };

        // Multiget is really like a propfind where Depth: 0|1|Infinity is replaced by an arbitrary
        // list of URLs
        let multiget = match report {
            cal::Report::Multiget(m) => m,
            _ => return Ok(Response::builder()
                           .status(501)
                           .body(text_body("Not implemented"))?),
        };

        // Getting the list of nodes
        let (mut ok_node, mut not_found) = (Vec::new(), Vec::new());
        for h in multiget.href.into_iter() {
            let maybe_collected_node = match Path::new(h.0.as_str()) {
                Ok(Path::Abs(p)) => RootNode{}.fetch(&self.user, p.as_slice()).await.or(Err(h)),
                Ok(Path::Rel(p)) => self.node.fetch(&self.user, p.as_slice()).await.or(Err(h)),
                Err(_) => Err(h),
            };

            match maybe_collected_node {
                Ok(v) => ok_node.push(v),
                Err(h) => not_found.push(h),
            };
        }

        // Getting props
        let props = match multiget.selector {
            None | Some(cal::CalendarSelector::AllProp) => Some(dav::PropName(ALLPROP.to_vec())),
            Some(cal::CalendarSelector::PropName) => None,
            Some(cal::CalendarSelector::Prop(inner)) => Some(inner),
        };

        serialize(status, Self::multistatus(&self.user, ok_node, not_found, props))
    }

    /// PROPFIND is the standard way to fetch WebDAV properties
    async fn propfind(self) -> Result<Response<BoxBody<Bytes, std::io::Error>>> { 
        let depth = depth(&self.req);
        if matches!(depth, dav::Depth::Infinity) {
            return Ok(Response::builder()
                .status(501)
                .body(text_body("Depth: Infinity not implemented"))?)    
        }

        let status = hyper::StatusCode::from_u16(207)?;

        // A client may choose not to submit a request body.  An empty PROPFIND
        // request body MUST be treated as if it were an 'allprop' request.
        // @FIXME here we handle any invalid data as an allprop, an empty request is thus correctly
        // handled, but corrupted requests are also silently handled as allprop.
        let propfind = deserialize::<dav::PropFind<All>>(self.req).await.unwrap_or_else(|_| dav::PropFind::<All>::AllProp(None));
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
            },
            dav::PropFind::Prop(inner) => Some(inner),
        };

        // Not Found is currently impossible considering the way we designed this function
        let not_found = vec![];
        serialize(status, Self::multistatus(&self.user, nodes, not_found, propname))
    }

    // --- Internal functions ---
    /// Utility function to build a multistatus response from 
    /// a list of DavNodes
    fn multistatus(user: &ArcUser, nodes: Vec<Box<dyn DavNode>>, not_found: Vec<dav::Href>, props: Option<dav::PropName<All>>) -> dav::Multistatus<All> {
        // Collect properties on existing objects
        let mut responses: Vec<dav::Response<All>> = match props {
            Some(props) => nodes.into_iter().map(|n| n.response_props(user, props.clone())).collect(),
            None => nodes.into_iter().map(|n| n.response_propname(user)).collect(),
        };

        // Register not found objects only if relevant
        if !not_found.is_empty() {
            responses.push(dav::Response {
                status_or_propstat: dav::StatusOrPropstat::Status(not_found, dav::Status(hyper::StatusCode::NOT_FOUND)),
                error: None,
                location: None,
                responsedescription: None,
            });
        }

        // Build response
        dav::Multistatus::<All> {
            responses,
            responsedescription: None,
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
            return Ok(Path::Abs(path_segments))
        }
        Ok(Path::Rel(path_segments))
    }
}
