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

use anyhow::{anyhow, Result};
use futures::stream::StreamExt;
use futures::{future::BoxFuture, future::FutureExt};

use aero_collections::{
    dav::collection::Collection,
    dav::davindex::{Etag, Token},
    user::User,
};
use aero_dav::acltypes as acl;
use aero_dav::caltypes as cal;
use aero_dav::realization::{self as all, All};
use aero_dav::coretypes as dav;
use aero_dav::versioningtypes as vers;

use crate::dav::node::{
    Content, DavNode,
    DavObject, DavObjectNode,
    DavStoredCollection, DavStoredCollectionNode,
    PutPolicy, PropertyResult,
};

/// The root of the webdav filesystem
#[derive(Clone)]
pub(crate) struct RootNode {}
impl DavNode for RootNode {
    fn fetch<'a>(
        &self,
        user: &'a User,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let this = self.clone();
            return async { Ok(Box::new(this) as Box<dyn DavNode>) }.boxed();
        }

        if path[0] == user.username {
            let child = Box::new(HomeNode {});
            return child.fetch(user, &path[1..], create);
        }

        //@NOTE: We can't create a node at this level
        async { Err(anyhow!("Not found")) }.boxed()
    }

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![Box::new(HomeNode {}) as Box<dyn DavNode>] }.boxed()
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

    fn put<'a>(
        &'a mut self,
        _policy: PutPolicy,
        _stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported)).boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        futures::stream::once(futures::future::err(std::io::Error::from(
            std::io::ErrorKind::Unsupported,
        )))
        .boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }

    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        async { None }.boxed()
    }

    fn delete<'a>(&'a mut self) -> BoxFuture<'a, std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }

    fn diff<'a>(
        &'a mut self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }

    fn dav_header(&self) -> String {
        "1".into()
    }
}

/// The homedir collection of a user. It contains namespaces.
#[derive(Clone)]
pub(crate) struct HomeNode {}
impl DavNode for HomeNode {
    fn fetch<'a>(
        &self,
        user: &'a User,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        if path[0] == "calendar" {
            return async move {
                let child = Box::new(CalendarListNode::new(user).await?);
                child.fetch(user, &path[1..], create).await
            }
            .boxed();
        }

        //@NOTE: we can't create a node at this level
        async { Err(anyhow!("Not found")) }.boxed()
    }

    fn children<'a>(&self, user: &'a User) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async {
            CalendarListNode::new(user)
                .await
                .map(|c| vec![Box::new(c) as Box<dyn DavNode>])
                .unwrap_or(vec![])
        }
        .boxed()
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

    fn put<'a>(
        &'a mut self,
        _policy: PutPolicy,
        _stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported)).boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        futures::stream::once(futures::future::err(std::io::Error::from(
            std::io::ErrorKind::Unsupported,
        )))
        .boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }

    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        async { None }.boxed()
    }

    fn delete<'a>(&'a mut self) -> BoxFuture<'a, std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    fn diff<'a>(
        &'a mut self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
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
    fn fetch<'a>(
        &self,
        user: &'a User,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        async move {
            //@FIXME: we should create a node if the open returns a "not found".
            let cal = user
                .calendars
                .dav
                .open(path[0])
                .await?
                .ok_or(anyhow!("Not found"))?;
            let child = Box::new(DavStoredCollectionNode(CalendarNode {
                col: cal,
                calname: path[0].to_string(),
            }));
            child.fetch(user, &path[1..], create).await
        }
        .boxed()
    }

    fn children<'a>(&self, user: &'a User) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        let list = self.list.clone();
        async move {
            //@FIXME maybe we want to be lazy here?!
            futures::stream::iter(list.iter())
                .filter_map(|name| async move {
                    user.calendars
                        .dav
                        .open(name)
                        .await
                        .ok()
                        .flatten()
                        .map(|v| (name, v))
                })
                .map(|(name, cal)| {
                    Box::new(DavStoredCollectionNode(CalendarNode {
                        col: cal,
                        calname: name.to_string(),
                    })) as Box<dyn DavNode>
                })
                .collect::<Vec<Box<dyn DavNode>>>()
                .await
        }
        .boxed()
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

    fn put<'a>(
        &'a mut self,
        _policy: PutPolicy,
        _stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported)).boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        futures::stream::once(futures::future::err(std::io::Error::from(
            std::io::ErrorKind::Unsupported,
        )))
        .boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }

    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        async { None }.boxed()
    }

    fn delete<'a>(&'a mut self) -> BoxFuture<'a, std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    fn diff<'a>(
        &'a mut self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }

    fn dav_header(&self) -> String {
        "1, access-control, calendar-access".into()
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
