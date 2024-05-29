use std::sync::Arc;
type ArcUser = std::sync::Arc<User>;

use anyhow::{anyhow, Result};
use futures::io::AsyncReadExt;
use futures::stream::{StreamExt, TryStreamExt};
use futures::{future::BoxFuture, future::FutureExt};

use aero_collections::{
    calendar::Calendar,
    davdag::{BlobId, Etag, SyncChange, Token},
    user::User,
};
use aero_dav::acltypes as acl;
use aero_dav::caltypes as cal;
use aero_dav::realization::{self as all, All};
use aero_dav::synctypes as sync;
use aero_dav::types as dav;
use aero_dav::versioningtypes as vers;

use super::node::PropertyStream;
use crate::dav::node::{Content, DavNode, PutPolicy};

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
pub const BASE_TOKEN_URI: &str = "https://aerogramme.0/sync/";

#[derive(Clone)]
pub(crate) struct RootNode {}
impl DavNode for RootNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
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

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![Box::new(HomeNode {}) as Box<dyn DavNode>] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        "/".into()
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                acl::PropertyRequest::CurrentUserPrincipal,
            )),
        ])
    }

    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let user = user.clone();
        futures::stream::iter(prop.0)
            .map(move |n| {
                let prop = match n {
                    dav::PropertyRequest::DisplayName => {
                        dav::Property::DisplayName("DAV Root".to_string())
                    }
                    dav::PropertyRequest::ResourceType => {
                        dav::Property::ResourceType(vec![dav::ResourceType::Collection])
                    }
                    dav::PropertyRequest::GetContentType => {
                        dav::Property::GetContentType("httpd/unix-directory".into())
                    }
                    dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                        acl::PropertyRequest::CurrentUserPrincipal,
                    )) => dav::Property::Extension(all::Property::Acl(
                        acl::Property::CurrentUserPrincipal(acl::User::Authenticated(dav::Href(
                            HomeNode {}.path(&user),
                        ))),
                    )),
                    v => return Err(v),
                };
                Ok(prop)
            })
            .boxed()
    }

    fn put<'a>(
        &'a self,
        _policy: PutPolicy,
        stream: Content<'a>,
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

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        async { None }.boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }

    fn diff<'a>(
        &self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }
}

#[derive(Clone)]
pub(crate) struct HomeNode {}
impl DavNode for HomeNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
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

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async {
            CalendarListNode::new(user)
                .await
                .map(|c| vec![Box::new(c) as Box<dyn DavNode>])
                .unwrap_or(vec![])
        }
        .boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/", user.username)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::CalendarHomeSet,
            )),
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let user = user.clone();

        futures::stream::iter(prop.0)
            .map(move |n| {
                let prop = match n {
                    dav::PropertyRequest::DisplayName => {
                        dav::Property::DisplayName(format!("{} home", user.username))
                    }
                    dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![
                        dav::ResourceType::Collection,
                        dav::ResourceType::Extension(all::ResourceType::Acl(
                            acl::ResourceType::Principal,
                        )),
                    ]),
                    dav::PropertyRequest::GetContentType => {
                        dav::Property::GetContentType("httpd/unix-directory".into())
                    }
                    dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                        cal::PropertyRequest::CalendarHomeSet,
                    )) => dav::Property::Extension(all::Property::Cal(
                        cal::Property::CalendarHomeSet(dav::Href(
                            //@FIXME we are hardcoding the calendar path, instead we would want to use
                            //objects
                            format!("/{}/calendar/", user.username),
                        )),
                    )),
                    v => return Err(v),
                };
                Ok(prop)
            })
            .boxed()
    }

    fn put<'a>(
        &'a self,
        _policy: PutPolicy,
        stream: Content<'a>,
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

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        async { None }.boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    fn diff<'a>(
        &self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }
}

#[derive(Clone)]
pub(crate) struct CalendarListNode {
    list: Vec<String>,
}
impl CalendarListNode {
    async fn new(user: &ArcUser) -> Result<Self> {
        let list = user.calendars.list(user).await?;
        Ok(Self { list })
    }
}
impl DavNode for CalendarListNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
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
                .open(user, path[0])
                .await?
                .ok_or(anyhow!("Not found"))?;
            let child = Box::new(CalendarNode {
                col: cal,
                calname: path[0].to_string(),
            });
            child.fetch(user, &path[1..], create).await
        }
        .boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        let list = self.list.clone();
        async move {
            //@FIXME maybe we want to be lazy here?!
            futures::stream::iter(list.iter())
                .filter_map(|name| async move {
                    user.calendars
                        .open(user, name)
                        .await
                        .ok()
                        .flatten()
                        .map(|v| (name, v))
                })
                .map(|(name, cal)| {
                    Box::new(CalendarNode {
                        col: cal,
                        calname: name.to_string(),
                    }) as Box<dyn DavNode>
                })
                .collect::<Vec<Box<dyn DavNode>>>()
                .await
        }
        .boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/", user.username)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let user = user.clone();

        futures::stream::iter(prop.0)
            .map(move |n| {
                let prop = match n {
                    dav::PropertyRequest::DisplayName => {
                        dav::Property::DisplayName(format!("{} calendars", user.username))
                    }
                    dav::PropertyRequest::ResourceType => {
                        dav::Property::ResourceType(vec![dav::ResourceType::Collection])
                    }
                    dav::PropertyRequest::GetContentType => {
                        dav::Property::GetContentType("httpd/unix-directory".into())
                    }
                    v => return Err(v),
                };
                Ok(prop)
            })
            .boxed()
    }

    fn put<'a>(
        &'a self,
        _policy: PutPolicy,
        stream: Content<'a>,
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

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        async { None }.boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    fn diff<'a>(
        &self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }
}

#[derive(Clone)]
pub(crate) struct CalendarNode {
    col: Arc<Calendar>,
    calname: String,
}
impl DavNode for CalendarNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        let col = self.col.clone();
        let calname = self.calname.clone();
        async move {
            match (col.dag().await.idx_by_filename.get(path[0]), create) {
                (Some(blob_id), _) => {
                    let child = Box::new(EventNode {
                        col: col.clone(),
                        calname,
                        filename: path[0].to_string(),
                        blob_id: *blob_id,
                    });
                    child.fetch(user, &path[1..], create).await
                }
                (None, true) => {
                    let child = Box::new(CreateEventNode {
                        col: col.clone(),
                        calname,
                        filename: path[0].to_string(),
                    });
                    child.fetch(user, &path[1..], create).await
                }
                _ => Err(anyhow!("Not found")),
            }
        }
        .boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        let col = self.col.clone();
        let calname = self.calname.clone();

        async move {
            col.dag()
                .await
                .idx_by_filename
                .iter()
                .map(|(filename, blob_id)| {
                    Box::new(EventNode {
                        col: col.clone(),
                        calname: calname.clone(),
                        filename: filename.to_string(),
                        blob_id: *blob_id,
                    }) as Box<dyn DavNode>
                })
                .collect()
        }
        .boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/", user.username, self.calname)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::SupportedCalendarComponentSet,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Sync(
                sync::PropertyRequest::SyncToken,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                vers::PropertyRequest::SupportedReportSet,
            )),
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let calname = self.calname.to_string();
        let col = self.col.clone();

        futures::stream::iter(prop.0)
            .then(move |n| {
                let calname = calname.clone();
                let col = col.clone();

                async move {
                    let prop = match n {
                        dav::PropertyRequest::DisplayName => {
                            dav::Property::DisplayName(format!("{} calendar", calname))
                        }
                        dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![
                            dav::ResourceType::Collection,
                            dav::ResourceType::Extension(all::ResourceType::Cal(
                                cal::ResourceType::Calendar,
                            )),
                        ]),
                        //dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
                        //@FIXME seems wrong but seems to be what Thunderbird expects...
                        dav::PropertyRequest::GetContentType => {
                            dav::Property::GetContentType("text/calendar".into())
                        }
                        dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                            cal::PropertyRequest::SupportedCalendarComponentSet,
                        )) => dav::Property::Extension(all::Property::Cal(
                            cal::Property::SupportedCalendarComponentSet(vec![
                                cal::CompSupport(cal::Component::VEvent),
                                cal::CompSupport(cal::Component::VTodo),
                                cal::CompSupport(cal::Component::VJournal),
                            ]),
                        )),
                        dav::PropertyRequest::Extension(all::PropertyRequest::Sync(
                            sync::PropertyRequest::SyncToken,
                        )) => match col.token().await {
                            Ok(token) => dav::Property::Extension(all::Property::Sync(
                                sync::Property::SyncToken(sync::SyncToken(format!(
                                    "{}{}",
                                    BASE_TOKEN_URI, token
                                ))),
                            )),
                            _ => return Err(n.clone()),
                        },
                        dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                            vers::PropertyRequest::SupportedReportSet,
                        )) => dav::Property::Extension(all::Property::Vers(
                            vers::Property::SupportedReportSet(vec![
                                vers::SupportedReport(vers::ReportName::Extension(
                                    all::ReportTypeName::Cal(cal::ReportTypeName::Multiget),
                                )),
                                vers::SupportedReport(vers::ReportName::Extension(
                                    all::ReportTypeName::Cal(cal::ReportTypeName::Query),
                                )),
                                vers::SupportedReport(vers::ReportName::Extension(
                                    all::ReportTypeName::Sync(sync::ReportTypeName::SyncCollection),
                                )),
                            ]),
                        )),
                        v => return Err(v),
                    };
                    Ok(prop)
                }
            })
            .boxed()
    }

    fn put<'a>(
        &'a self,
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

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        async { None }.boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    fn diff<'a>(
        &self,
        sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        let col = self.col.clone();
        let calname = self.calname.clone();
        async move {
            let sync_token = match sync_token {
                Some(v) => v,
                None => {
                    let token = col
                        .token()
                        .await
                        .or(Err(std::io::Error::from(std::io::ErrorKind::Interrupted)))?;
                    let ok_nodes = col
                        .dag()
                        .await
                        .idx_by_filename
                        .iter()
                        .map(|(filename, blob_id)| {
                            Box::new(EventNode {
                                col: col.clone(),
                                calname: calname.clone(),
                                filename: filename.to_string(),
                                blob_id: *blob_id,
                            }) as Box<dyn DavNode>
                        })
                        .collect();

                    return Ok((token, ok_nodes, vec![]));
                }
            };
            let (new_token, listed_changes) = match col.diff(sync_token).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::info!(err=?e, "token resolution failed, maybe a forgotten token");
                    return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
                }
            };

            let mut ok_nodes: Vec<Box<dyn DavNode>> = vec![];
            let mut rm_nodes: Vec<dav::Href> = vec![];
            for change in listed_changes.into_iter() {
                match change {
                    SyncChange::Ok((filename, blob_id)) => {
                        let child = Box::new(EventNode {
                            col: col.clone(),
                            calname: calname.clone(),
                            filename,
                            blob_id,
                        });
                        ok_nodes.push(child);
                    }
                    SyncChange::NotFound(filename) => {
                        rm_nodes.push(dav::Href(filename));
                    }
                }
            }

            Ok((new_token, ok_nodes, rm_nodes))
        }
        .boxed()
    }
}

#[derive(Clone)]
pub(crate) struct EventNode {
    col: Arc<Calendar>,
    calname: String,
    filename: String,
    blob_id: BlobId,
}

impl DavNode for EventNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        async {
            Err(anyhow!(
                "Not supported: can't create a child on an event node"
            ))
        }
        .boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!(
            "/{}/calendar/{}/{}",
            user.username, self.calname, self.filename
        )
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetEtag,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::CalendarData(cal::CalendarDataRequest::default()),
            )),
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let this = self.clone();

        futures::stream::iter(prop.0)
            .then(move |n| {
                let this = this.clone();

                async move {
                    let prop = match &n {
                        dav::PropertyRequest::DisplayName => {
                            dav::Property::DisplayName(format!("{} event", this.filename))
                        }
                        dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![]),
                        dav::PropertyRequest::GetContentType => {
                            dav::Property::GetContentType("text/calendar".into())
                        }
                        dav::PropertyRequest::GetEtag => {
                            let etag = this.etag().await.ok_or(n.clone())?;
                            dav::Property::GetEtag(etag)
                        }
                        dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                                cal::PropertyRequest::CalendarData(req),
                                )) => {
                            let ics = String::from_utf8(
                                this.col.get(this.blob_id).await.or(Err(n.clone()))?,
                                )
                                .or(Err(n.clone()))?;

                            let new_ics = match &req.comp {
                                None => ics,
                                Some(prune_comp) => {
                                    // parse content
                                    let ics = match icalendar::parser::read_calendar(&ics) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            tracing::warn!(err=?e, "Unable to parse ICS in calendar-query");
                                            return Err(n.clone())
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
                                        None => return Err(n.clone()),
                                    };

                                    // reserialize
                                    format!("{}", icalendar::parser::Calendar { properties: new_comp.properties, components: new_comp.components })
                                },
                            };



                            dav::Property::Extension(all::Property::Cal(
                                cal::Property::CalendarData(cal::CalendarDataPayload {
                                    mime: None,
                                    payload: new_ics,
                                }),
                            ))
                        }
                        _ => return Err(n),
                    };
                    Ok(prop)
                }
            })
            .boxed()
    }

    fn put<'a>(
        &'a self,
        policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        async {
            let existing_etag = self
                .etag()
                .await
                .ok_or(std::io::Error::new(std::io::ErrorKind::Other, "Etag error"))?;
            match policy {
                PutPolicy::CreateOnly => {
                    return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
                }
                PutPolicy::ReplaceEtag(etag) if etag != existing_etag.as_str() => {
                    return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
                }
                _ => (),
            };

            //@FIXME for now, our storage interface does not allow streaming,
            // so we load everything in memory
            let mut evt = Vec::new();
            let mut reader = stream.into_async_read();
            reader
                .read_to_end(&mut evt)
                .await
                .or(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)))?;
            let (_token, entry) = self
                .col
                .put(self.filename.as_str(), evt.as_ref())
                .await
                .or(Err(std::io::ErrorKind::Interrupted))?;
            self.col
                .opportunistic_sync()
                .await
                .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(entry.2)
        }
        .boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        //@FIXME for now, our storage interface does not allow streaming,
        // so we load everything in memory
        let calendar = self.col.clone();
        let blob_id = self.blob_id.clone();
        let calblob = async move {
            let raw_ics = calendar
                .get(blob_id)
                .await
                .or(Err(std::io::Error::from(std::io::ErrorKind::Interrupted)))?;

            Ok(hyper::body::Bytes::from(raw_ics))
        };
        futures::stream::once(Box::pin(calblob)).boxed()
    }

    fn content_type(&self) -> &str {
        "text/calendar"
    }

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        let calendar = self.col.clone();

        async move {
            calendar
                .dag()
                .await
                .table
                .get(&self.blob_id)
                .map(|(_, _, etag)| etag.to_string())
        }
        .boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        let calendar = self.col.clone();
        let blob_id = self.blob_id.clone();

        async move {
            let _token = match calendar.delete(blob_id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err=?e, "delete event node");
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
                }
            };
            calendar
                .opportunistic_sync()
                .await
                .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(())
        }
        .boxed()
    }
    fn diff<'a>(
        &self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }
}

#[derive(Clone)]
pub(crate) struct CreateEventNode {
    col: Arc<Calendar>,
    calname: String,
    filename: String,
}
impl DavNode for CreateEventNode {
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        async {
            Err(anyhow!(
                "Not supported: can't create a child on an event node"
            ))
        }
        .boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!(
            "/{}/calendar/{}/{}",
            user.username, self.calname, self.filename
        )
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![])
    }

    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        futures::stream::iter(vec![]).boxed()
    }

    fn put<'a>(
        &'a self,
        _policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        //@NOTE: policy might not be needed here: whatever we put, there is no known entries here

        async {
            //@FIXME for now, our storage interface does not allow for streaming
            let mut evt = Vec::new();
            let mut reader = stream.into_async_read();
            reader.read_to_end(&mut evt).await.unwrap();
            let (_token, entry) = self
                .col
                .put(self.filename.as_str(), evt.as_ref())
                .await
                .or(Err(std::io::ErrorKind::Interrupted))?;
            self.col
                .opportunistic_sync()
                .await
                .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(entry.2)
        }
        .boxed()
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

    fn etag(&self) -> BoxFuture<Option<Etag>> {
        async { None }.boxed()
    }

    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>> {
        // Nothing to delete
        async { Ok(()) }.boxed()
    }
    fn diff<'a>(
        &self,
        _sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    > {
        async { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }.boxed()
    }
}
