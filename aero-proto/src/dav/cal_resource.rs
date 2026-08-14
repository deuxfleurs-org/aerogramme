//! DAV Resources for calendars:
//! calendar namespace, calendar collection, and calendar event.

use anyhow::Result;
use futures::{future::BoxFuture, future::FutureExt};
use futures::stream::{Stream, StreamExt, TryStreamExt};

use aero_collections::{
    dav::collection::Collection,
    user::User,
};
use aero_dav::caltypes as cal;
use aero_dav::realization::{self as all, All};
use aero_dav::coretypes as dav;
use aero_dav::versioningtypes as vers;
use aero_ical::query::is_component_match;

use crate::dav::codec::Path;
use crate::dav::multistatus;
use crate::dav::node::{
    ChildNode,
    DavNode,
    DavObject, DavObjectNode,
    DavStoredCollection, DavStoredCollectionNode,
    IOResult,
    PropertyResult,
    ReportResponse,
};

/// The calendar namespace of a user. It contains calendar collections.
#[derive(Clone)]
pub(crate) struct CalendarListNode {
    list: Vec<String>,
}
impl CalendarListNode {
    pub async fn new(user: &User) -> Result<Self> {
        let list = user.calendars.dav.list().await?;
        Ok(Self { list })
    }
}
impl DavNode for CalendarListNode {
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<String>> {
        let list = self.list.clone();
        async move { list }.boxed()
    }

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
        let calname = name.to_string();
        async {
            let col = user
                .calendars
                .dav
                .open(&calname)
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e))?;
            match col {
                //@FIXME: allow creating new calendar nodes
                None => Ok(ChildNode::CannotCreate),
                Some(col) => {
                    Ok(ChildNode::Existing(
                        Box::new(DavStoredCollectionNode(CalendarNode {
                            col,
                            calname,
                        })) as Box<dyn DavNode>
                    ))
                }
            }
        }.boxed()
    }

    fn path(&self, user: &User) -> String {
        format!("/{}/calendar/", user.username)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
        ])
    }
    fn properties<'a>(&'a mut self, user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async move {
            let mut v = vec![];
            for n in prop.0 {
                let res = match n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName(format!("{} calendars", user.username))),
                    dav::PropertyRequest::ResourceType =>
                        Ok(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType("httpd/unix-directory".into())),
                    v => Err(v),
                };
                v.push(res)
            }
            v
        }.boxed()
    }

    fn dav_header(&self) -> String {
        "1, 3, access-control, calendar-access".into()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

/// A calendar collection. It contains calendar events.
#[derive(Clone)]
pub(crate) struct CalendarNode {
    col: Collection,
    calname: String,
}
impl DavStoredCollection for CalendarNode {
    fn collection(&self) -> &Collection {
        &self.col
    }

    fn collection_mut(&mut self) -> &mut Collection {
        &mut self.col
    }

    fn display_name(&self) -> String {
        format!("{} calendar", self.calname)
    }

    fn path(&self, user: &User) -> String {
        format!("/{}/calendar/{}/", user.username, self.calname)
    }

    fn content_type(&self) -> &str {
        //dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
        //@FIXME seems wrong but seems to be what Thunderbird expects...
        "text/calendar"
    }
    
    fn mk_child_node(&self, filename: &str) -> Box<dyn DavNode> {
        Box::new(DavObjectNode(CalendarEventNode {
            col: self.col.clone(),
            calname: self.calname.clone(),
            filename: filename.to_string(),
        }))
    }

    fn additional_resource_types(&self) -> Vec<dav::ResourceType<All>> {
        vec![
            dav::ResourceType::Extension(all::ResourceType::Cal(
                cal::ResourceType::Calendar,
            )),
        ]
    }

    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>> {
        vec![
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::SupportedCalendarComponentSet,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::SupportedCollationSet,
            )),
        ]            
    }

    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult> {
        async move {
            match prop {
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                    cal::PropertyRequest::SupportedCalendarComponentSet,
                )) => Ok(dav::Property::Extension(all::Property::Cal(
                    cal::Property::SupportedCalendarComponentSet(vec![
                        cal::CompSupport(cal::Component::VEvent),
                        cal::CompSupport(cal::Component::VTodo),
                        cal::CompSupport(cal::Component::VJournal),
                    ]),
                ))),
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                    cal::PropertyRequest::SupportedCollationSet,
                )) => Ok(dav::Property::Extension(all::Property::Cal(
                    cal::Property::SupportedCollationSet(vec![
                        cal::SupportedCollation(cal::Collation::AsciiCaseMap),
                        cal::SupportedCollation(cal::Collation::Octet),
                    ])
                ))),
                _ => Err(prop.clone()),
            }
        }.boxed()
    }

    fn additional_supported_reports(&self) -> Vec<vers::SupportedReport<All>> {
        vec![
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Cal(cal::ReportTypeName::Multiget),
            )),
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Cal(cal::ReportTypeName::Query),
            )),
        ]
    }

    fn additional_report<'a>(&'a mut self, user: &'a User, report: &'a vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async {
            match report {
                vers::Report::Extension(all::ReportType::Cal(cal::ReportType::Multiget(m))) => {
                    // Multiget is really like a propfind where Depth: 0|1|Infinity is replaced by an arbitrary
                    // list of URLs
                    let (mut ok_node, mut not_found) = (Vec::new(), Vec::new());
                    let self_path = self.path(user);
                    let self_path = Path::new(self_path.as_str()).unwrap();
                    for h in m.href.iter().cloned() {
                        let filename = Path::new(h.0.as_str())
                            .ok()
                            .and_then(|p| p.relativize(&self_path))
                            .and_then(|p| p.as_single_name());
                        
                        match filename {
                            Some(name) if self.col.index().idx_by_filename.contains_key(name) =>
                                ok_node.push(self.mk_child_node(name)),
                            _ =>
                                not_found.push(h),
                        }
                    }

                    Ok(ReportResponse::Ok(
                        multistatus::Builder::new()
                            .with_propfind_nodes(user, selector_to_propfind(m.selector.clone()), ok_node)
                            .await
                            .with_not_found(not_found)
                            .build()
                    ))
                },

                vers::Report::Extension(all::ReportType::Cal(cal::ReportType::Query(q))) => {
                    let children_nodes: Vec<_> = self
                        .col
                        .index()
                        .idx_by_filename
                        .keys()
                        .map(|name| self.mk_child_node(name))
                        .collect();
                    
                    let ok_node = apply_filter(children_nodes, &q.filter).try_collect().await?;
                    Ok(ReportResponse::Ok(
                        multistatus::Builder::new()
                            .with_propfind_nodes(user, selector_to_propfind(q.selector.clone()), ok_node)
                            .await
                            .build()
                    ))
                },
                
                _ => Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
            }
        }.boxed()
    }
    
    fn additional_dav_headers(&self) -> Vec<String> {
        vec!["calendar-access".to_string()]
    }
}

/// A calendar event, which may or may not exist in storage. It is a single object.
#[derive(Clone)]
pub(crate) struct CalendarEventNode {
    col: Collection,
    calname: String,
    filename: String,
}

impl DavObject for CalendarEventNode {
    fn collection(&self) -> &Collection {
        &self.col
    }

    fn collection_mut(&mut self) -> &mut Collection {
        &mut self.col
    }

    fn filename(&self) -> &str {
        &self.filename
    }

    fn path(&self, user: &User) -> String {
        format!(
            "/{}/calendar/{}/{}",
            user.username, self.calname, self.filename
        )
    }

    fn content_type(&self) -> &str {
        "text/calendar"
    }

    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>> {
        vec![
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::SupportedCollationSet,
            )),
        ]
    }

    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult> {
        let this = self.clone();
        async move {
            match prop {
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                    cal::PropertyRequest::SupportedCollationSet,
                )) => Ok(dav::Property::Extension(all::Property::Cal(
                    cal::Property::SupportedCollationSet(vec![
                        cal::SupportedCollation(cal::Collation::AsciiCaseMap),
                        cal::SupportedCollation(cal::Collation::Octet),
                    ])
                ))),
                // This is not a "real" property (it cannot be queried by
                // PROPFIND), but is queried internally by calendar reports.
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                    cal::PropertyRequest::CalendarData(req),
                )) => {
                    let blob_id = this.blob_id().ok_or(prop.clone())?;
                    let bytes = this.col.get(blob_id).await.or(Err(prop.clone()))?;
                    let ics = String::from_utf8(bytes).or(Err(prop.clone()))?;

                    let new_ics = match &req.comp {
                        None => ics,
                        Some(prune_comp) => {
                            // parse content
                            let ics = match icalendar::parser::read_calendar(&ics) {
                                Ok(v) => v,
                                Err(e) => {
                                    tracing::warn!(err=?e, "Unable to parse ICS in calendar-query");
                                    return Err::<_, dav::PropertyRequest<_>>(prop.clone())
                                }
                            };

                            // build a fake vcal component for caldav compat
                            let fake_vcal_component = icalendar::parser::Component {
                                name: cal::Component::VCalendar.as_str().into(),
                                properties: ics.properties,
                                components: ics.components,
                            };

                            // rebuild component
                            let new_comp = match aero_ical::prune::component(&fake_vcal_component, prune_comp) {
                                Some(v) => v,
                                None => return Err(prop.clone()),
                            };

                            // reserialize
                            format!("{}", icalendar::parser::Calendar { properties: new_comp.properties, components: new_comp.components })
                        },
                    };

                    Ok(dav::Property::Extension(all::Property::Cal(
                        cal::Property::CalendarData(cal::CalendarDataPayload {
                            mime: Default::default(),
                            payload: new_ics,
                        }),
                    )))
                },
                _ => Err(prop.clone()),
            }
        }.boxed()
    }

    fn supported_reports(&self) -> Vec<vers::SupportedReport<All>> {
        vec![
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Cal(cal::ReportTypeName::Multiget),
            )),
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Cal(cal::ReportTypeName::Query),
            )),
        ]
    }

    fn report<'a>(&'a mut self, user: &'a User, report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async {
            match report {
                vers::Report::Extension(all::ReportType::Cal(cal::ReportType::Multiget(m))) => {
                    // On a single object, multiget must contain a single URL pointing to this object
                    let self_path = self.path(user);
                    let self_path = Path::new(self_path.as_str()).unwrap();
                    let self_node = Box::new(DavObjectNode(self.clone())) as Box<dyn DavNode>;
                    let (mut ok_node, mut not_found) = (Vec::new(), Vec::new());
                    for h in m.href.into_iter() {
                        if Path::new(h.0.as_str())
                            .is_ok_and(|p| p.relativize(&self_path).is_some_and(|p| p.is_empty()))
                        {
                            ok_node.push(self_node.clone_node())
                        } else {
                            not_found.push(h)
                        }
                    }
                    Ok(ReportResponse::Ok(
                        multistatus::Builder::new()
                            .with_propfind_nodes(user, selector_to_propfind(m.selector.clone()), ok_node)
                            .await
                            .with_not_found(not_found)
                            .build()
                    ))
                },

                vers::Report::Extension(all::ReportType::Cal(cal::ReportType::Query(q))) => {
                    let nodes = vec![Box::new(DavObjectNode(self.clone())) as Box<dyn DavNode>];

                    let ok_node = apply_filter(nodes, &q.filter).try_collect().await?;
                    Ok(ReportResponse::Ok(
                        multistatus::Builder::new()
                            .with_propfind_nodes(user, selector_to_propfind(q.selector.clone()), ok_node)
                            .await
                            .build()
                    ))
                },
                
                _ => Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
            }
        }.boxed()
    }

    fn additional_dav_headers(&self) -> Vec<String> {
        vec!["calendar-access".to_string()]
    }
}

fn selector_to_propfind(s: Option<cal::CalendarSelector<All>>) -> dav::PropFind<All> {
    match s {
        None | Some(cal::CalendarSelector::AllProp) => dav::PropFind::AllProp(None),
        Some(cal::CalendarSelector::PropName) => dav::PropFind::PropName,
        Some(cal::CalendarSelector::Prop(inner)) => dav::PropFind::Prop(inner),
    }
}

//@FIXME naive implementation, must be refactored later
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
