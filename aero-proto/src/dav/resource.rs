//! This modules defines the WebDAV filesystem structure exposed by aerogramme
//! 
//! /                                 root
//! ├── alice                         homedir for user "alice"
//! │   └── calendar                  calendar namespace
//! │       └── Personal              default calendar collection
//! │           └── event1.ics        calendar event
//! │           └── ...
//! ├── bob                           homedir for user "bob"
//! │   └── ...

use anyhow::Result;
use futures::{future::BoxFuture, future::FutureExt};

use aero_collections::{
    dav::collection::Collection,
    user::User,
};
use aero_dav::acltypes as acl;
use aero_dav::caltypes as cal;
use aero_dav::realization::{self as all, All};
use aero_dav::coretypes as dav;
use aero_dav::versioningtypes as vers;

use crate::dav::node::{
    ChildNode,
    DavNode,
    DavObject, DavObjectNode,
    DavStoredCollection, DavStoredCollectionNode,
    PropertyResult,
};

/// The root of the webdav filesystem
#[derive(Clone)]
pub(crate) struct RootNode {}
impl DavNode for RootNode {
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }

    fn children<'a>(&self, user: &'a User) -> BoxFuture<'a, Vec<String>> {
        async { vec![user.username.to_string()] }.boxed()
    }

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, Result<ChildNode>> {
        let node =
            if name == user.username {
                ChildNode::Existing(Box::new(HomeNode {}) as Box<dyn DavNode>)
            } else {
                ChildNode::CannotCreate
            };
        async move { Ok(node) }.boxed()
    }

    fn path(&self, _user: &User) -> String {
        "/".into()
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                acl::PropertyRequest::CurrentUserPrincipal,
            )),
        ])
    }

    fn properties<'a>(&'a mut self, user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async move {
            let mut v = vec![];
            for n in prop.0 {
                let res = match n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName("DAV Root".to_string())),
                    dav::PropertyRequest::ResourceType =>
                        Ok(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType("httpd/unix-directory".into())),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                        acl::PropertyRequest::CurrentUserPrincipal,
                    )) => Ok(dav::Property::Extension(all::Property::Acl(
                        acl::Property::CurrentUserPrincipal(acl::User::Authenticated(dav::Href(
                            HomeNode {}.path(&user),
                        ))),
                    ))),
                    v => Err(v),
                };
                v.push(res)
            }
            v
        }.boxed()
    }

    fn dav_header(&self) -> String {
        "1".into()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

/// The homedir collection of a user. It contains namespaces.
#[derive(Clone)]
pub(crate) struct HomeNode {}
impl DavNode for HomeNode {
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<String>> {
        async { vec!["calendar".to_string()] }.boxed()
    }

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, Result<ChildNode>> {
        if name == "calendar" {
            async move {
                let node = Box::new(CalendarListNode::new(user).await?) as Box<dyn DavNode>;
                Ok(ChildNode::Existing(node))
            }.boxed()
        } else {
            async move { Ok(ChildNode::CannotCreate) }.boxed()
        }
    }

    fn path(&self, user: &User) -> String {
        format!("/{}/", user.username)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::CalendarHomeSet,
            )),
        ])
    }
    fn properties<'a>(&'a mut self, user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async move {
            let mut v = vec![];
            for n in prop.0 {
                let res = match n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName(format!("{} home", user.username))),
                    dav::PropertyRequest::ResourceType => Ok(dav::Property::ResourceType(vec![
                        dav::ResourceType::Collection,
                        dav::ResourceType::Extension(all::ResourceType::Acl(
                            acl::ResourceType::Principal,
                        )),
                    ])),
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType("httpd/unix-directory".into())),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                        cal::PropertyRequest::CalendarHomeSet,
                    )) => Ok(dav::Property::Extension(all::Property::Cal(
                        cal::Property::CalendarHomeSet(dav::Href(
                            //@FIXME we are hardcoding the calendar path, instead we would want to use
                            //objects
                            format!("/{}/calendar/", user.username),
                        )),
                    ))),
                    v => Err(v),
                };
                v.push(res);
            }
            v
        }.boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }

    fn dav_header(&self) -> String {
        "1, access-control, calendar-access".into()
    }
}

/// The calendar namespace of a user. It contains calendar collections.
#[derive(Clone)]
pub(crate) struct CalendarListNode {
    list: Vec<String>,
}
impl CalendarListNode {
    async fn new(user: &User) -> Result<Self> {
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

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, Result<ChildNode>> {
        let calname = name.to_string();
        async {
            let col = user
                .calendars
                .dav
                .open(&calname)
                .await?;
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
        "1, access-control, calendar-access".into()
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
                cal::PropertyRequest::CalendarData(cal::CalendarDataRequest::default()),
            )),
        ]
    }

    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult> {
        let this = self.clone();
        async move {
            match prop {
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
}
