use anyhow::Result;
use futures::stream::{StreamExt, TryStreamExt};
use http_body_util::combinators::{BoxBody, UnsyncBoxBody};
use http_body_util::BodyStream;
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper::body::Incoming;
use hyper::{body::Bytes, Request, Response};

use aero_collections::user::User;
use aero_dav::caltypes as cal;
use aero_dav::realization::All;
use aero_dav::types as dav;

use crate::dav::codec;
use crate::dav::codec::{depth, deserialize, serialize, text_body};
use crate::dav::node::{DavNode, PutPolicy};
use crate::dav::resource::RootNode;

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

        let report = match deserialize::<cal::Report<All>>(self.req).await {
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

        // Extracting request information
        match report {
            cal::Report::Multiget(m) => {
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
            }
            cal::Report::Query(q) => {
                calprop = q.selector;
                ok_node = apply_filter(self.node.children(&self.user).await, &q.filter)
                    .try_collect()
                    .await?;
            }
            cal::Report::FreeBusy(_) => {
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
            Self::multistatus(&self.user, ok_node, not_found, props).await,
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
            Self::multistatus(&self.user, nodes, not_found, propname).await,
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

//@FIXME move somewhere else
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
        match is_component_match(&[fake_vcal_component], root_filter) {
            true => Some(Ok(single_node)),
            _ => None,
        }
    })
}

fn prop_date(
    properties: &[icalendar::parser::Property],
    name: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    properties
        .iter()
        .find(|candidate| candidate.name.as_str() == name)
        .map(|p| p.val.as_str())
        .map(|raw_time| {
            tracing::trace!(raw_time = raw_time, "VEVENT raw time");
            NaiveDateTime::parse_from_str(raw_time, cal::ICAL_DATETIME_FMT)
                .ok()
                .map(|v| v.and_utc())
        })
        .flatten()
}

fn is_properties_match(props: &[icalendar::parser::Property], filters: &[cal::PropFilter]) -> bool {
    filters.iter().all(|single_filter| {
        // Find the property
        let single_prop = props
            .iter()
            .find(|candidate| candidate.name.as_str() == single_filter.name.0.as_str());
        match (&single_filter.additional_rules, single_prop) {
            (None, Some(_)) | (Some(cal::PropFilterRules::IsNotDefined), None) => true,
            (None, None)
            | (Some(cal::PropFilterRules::IsNotDefined), Some(_))
            | (Some(cal::PropFilterRules::Match(_)), None) => false,
            (Some(cal::PropFilterRules::Match(pattern)), Some(prop)) => {
                // check value
                match &pattern.time_or_text {
                    Some(cal::TimeOrText::Time(time_range)) => {
                        let maybe_parsed_date = NaiveDateTime::parse_from_str(
                            prop.val.as_str(),
                            cal::ICAL_DATETIME_FMT,
                        )
                        .ok()
                        .map(|v| v.and_utc());

                        let parsed_date = match maybe_parsed_date {
                            None => return false,
                            Some(v) => v,
                        };

                        // see if entry is in range
                        let is_in_range = match time_range {
                            cal::TimeRange::OnlyStart(after) => &parsed_date >= after,
                            cal::TimeRange::OnlyEnd(before) => &parsed_date <= before,
                            cal::TimeRange::FullRange(after, before) => {
                                &parsed_date >= after && &parsed_date <= before
                            }
                        };
                        if !is_in_range {
                            return false;
                        }

                        // if you are here, this subcondition is valid
                    }
                    Some(cal::TimeOrText::Text(txt_match)) => {
                        //@FIXME ignoring collation
                        let is_match = match txt_match.negate_condition {
                            None | Some(false) => {
                                prop.val.as_str().contains(txt_match.text.as_str())
                            }
                            Some(true) => !prop.val.as_str().contains(txt_match.text.as_str()),
                        };
                        if !is_match {
                            return false;
                        }
                    }
                    None => (), // if not filter on value is set, continue
                };

                // check parameters
                pattern.param_filter.iter().all(|single_param_filter| {
                    let maybe_param = prop.params.iter().find(|candidate| {
                        candidate.key.as_str() == single_param_filter.name.as_str()
                    });

                    match (maybe_param, &single_param_filter.additional_rules) {
                        (Some(_), None) => true,
                        (None, None) => false,
                        (Some(_), Some(cal::ParamFilterMatch::IsNotDefined)) => false,
                        (None, Some(cal::ParamFilterMatch::IsNotDefined)) => true,
                        (None, Some(cal::ParamFilterMatch::Match(_))) => false,
                        (Some(param), Some(cal::ParamFilterMatch::Match(txt_match))) => {
                            let param_val = match &param.val {
                                Some(v) => v,
                                None => return false,
                            };

                            match txt_match.negate_condition {
                                None | Some(false) => {
                                    param_val.as_str().contains(txt_match.text.as_str())
                                }
                                Some(true) => !param_val.as_str().contains(txt_match.text.as_str()),
                            }
                        }
                    }
                })
            }
        }
    })
}

fn is_in_time_range(
    properties: &[icalendar::parser::Property],
    time_range: &cal::TimeRange,
) -> bool {
    //@FIXME too naive: https://datatracker.ietf.org/doc/html/rfc4791#section-9.9

    let (dtstart, dtend) = match (
        prop_date(properties, "DTSTART"),
        prop_date(properties, "DTEND"),
    ) {
        (Some(start), None) => (start, start),
        (None, Some(end)) => (end, end),
        (Some(start), Some(end)) => (start, end),
        _ => {
            tracing::warn!("unable to extract DTSTART and DTEND from VEVENT");
            return false;
        }
    };

    tracing::trace!(event_start=?dtstart, event_end=?dtend, filter=?time_range, "apply filter on VEVENT");
    match time_range {
        cal::TimeRange::OnlyStart(after) => &dtend >= after,
        cal::TimeRange::OnlyEnd(before) => &dtstart <= before,
        cal::TimeRange::FullRange(after, before) => &dtend >= after && &dtstart <= before,
    }
}

use chrono::NaiveDateTime;
fn is_component_match(
    components: &[icalendar::parser::Component],
    filter: &cal::CompFilter,
) -> bool {
    // Find the component among the list
    let maybe_comp = components
        .iter()
        .find(|candidate| candidate.name.as_str() == filter.name.as_str());

    // Filter according to rules
    match (maybe_comp, &filter.additional_rules) {
        (Some(_), None) => true,
        (None, Some(cal::CompFilterRules::IsNotDefined)) => true,
        (None, None) => false,
        (Some(_), Some(cal::CompFilterRules::IsNotDefined)) => false,
        (None, Some(cal::CompFilterRules::Matches(_))) => false,
        (Some(component), Some(cal::CompFilterRules::Matches(matcher))) => {
            // check time range
            if let Some(time_range) = &matcher.time_range {
                if !is_in_time_range(component.properties.as_ref(), time_range) {
                    return false;
                }
            }

            // check properties
            if !is_properties_match(component.properties.as_ref(), matcher.prop_filter.as_ref()) {
                return false;
            }

            // check inner components
            matcher.comp_filter.iter().all(|inner_filter| {
                is_component_match(component.components.as_ref(), &inner_filter)
            })
        }
    }
}
