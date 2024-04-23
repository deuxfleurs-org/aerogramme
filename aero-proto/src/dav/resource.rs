use std::sync::Arc;
type ArcUser = std::sync::Arc<User>;

use anyhow::{anyhow, bail, Result};
use futures::stream::{TryStream, TryStreamExt, StreamExt};
use futures::io::AsyncReadExt;
use futures::{future::BoxFuture, future::FutureExt};

use aero_collections::{user::User, calendar::Calendar, davdag::{BlobId, IndexEntry, Etag}};
use aero_dav::types as dav;
use aero_dav::caltypes as cal;
use aero_dav::acltypes as acl;
use aero_dav::realization::{All, self as all};

use crate::dav::node::{DavNode, PutPolicy, Content};
use super::node::PropertyStream;

#[derive(Clone)]
pub(crate) struct RootNode {}
impl DavNode for RootNode {
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
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
        async { vec![Box::new(HomeNode { }) as Box<dyn DavNode>] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        "/".into()
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(acl::PropertyRequest::CurrentUserPrincipal)),
        ])
    }

    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let user = user.clone();
        futures::stream::iter(prop.0).map(move |n| {
            let prop = match n {
                dav::PropertyRequest::DisplayName => dav::Property::DisplayName("DAV Root".to_string()),
                dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![
                    dav::ResourceType::Collection,
                ]),
                dav::PropertyRequest::GetContentType => dav::Property::GetContentType("httpd/unix-directory".into()),
                dav::PropertyRequest::Extension(all::PropertyRequest::Acl(acl::PropertyRequest::CurrentUserPrincipal)) =>
                    dav::Property::Extension(all::Property::Acl(acl::Property::CurrentUserPrincipal(acl::User::Authenticated(dav::Href(HomeNode{}.path(&user)))))),
                v => return Err(v),
            };
            Ok(prop)
        }).boxed()
    }

    fn put<'a>(&'a self, _policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        todo!()
    }

    fn content(&self) -> Content<'static> {
        futures::stream::once(futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported))).boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

#[derive(Clone)]
pub(crate) struct HomeNode {}
impl DavNode for HomeNode {
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed()
        }

        if path[0] == "calendar" {
            return async move {
                let child = Box::new(CalendarListNode::new(user).await?);
                child.fetch(user, &path[1..], create).await
            }.boxed();
        }
    
        //@NOTE: we can't create a node at this level
        async { Err(anyhow!("Not found")) }.boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { 
            CalendarListNode::new(user).await
                .map(|c| vec![Box::new(c) as Box<dyn DavNode>])
                .unwrap_or(vec![]) 
        }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/", user.username)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarHomeSet)),
        ])
    }
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let user = user.clone();

        futures::stream::iter(prop.0).map(move |n| {
            let prop = match n {
                dav::PropertyRequest::DisplayName => dav::Property::DisplayName(format!("{} home", user.username)),
                dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![
                    dav::ResourceType::Collection,
                    dav::ResourceType::Extension(all::ResourceType::Acl(acl::ResourceType::Principal)),
                ]),
                dav::PropertyRequest::GetContentType => dav::Property::GetContentType("httpd/unix-directory".into()),
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarHomeSet)) => 
                    dav::Property::Extension(all::Property::Cal(cal::Property::CalendarHomeSet(dav::Href(
                        //@FIXME we are hardcoding the calendar path, instead we would want to use
                        //objects
                        format!("/{}/calendar/", user.username)
                    )))),
                v => return Err(v),
            };
            Ok(prop)
        }).boxed()
    }

    fn put<'a>(&'a self, _policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        todo!()
    }
    
    fn content(&self) -> Content<'static> {
        futures::stream::once(futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported))).boxed()
    }


    fn content_type(&self) -> &str {
        "text/plain"
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
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed();
        }

        async move {
            //@FIXME: we should create a node if the open returns a "not found".
            let cal = user.calendars.open(user, path[0]).await?.ok_or(anyhow!("Not found"))?;
            let child = Box::new(CalendarNode { 
                col: cal,
                calname: path[0].to_string()
            });
            child.fetch(user, &path[1..], create).await
        }.boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        let list = self.list.clone();
        async move {
            //@FIXME maybe we want to be lazy here?!
            futures::stream::iter(list.iter())
                .filter_map(|name| async move {
                    user.calendars.open(user, name).await
                        .ok()
                        .flatten()
                        .map(|v| (name, v))
                })
                .map(|(name, cal)| Box::new(CalendarNode { 
                    col: cal,
                    calname: name.to_string(),
                }) as Box<dyn DavNode>)
                .collect::<Vec<Box<dyn DavNode>>>()
                .await
        }.boxed()
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

        futures::stream::iter(prop.0).map(move |n| {
            let prop = match n {
                dav::PropertyRequest::DisplayName => dav::Property::DisplayName(format!("{} calendars", user.username)),
                dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![dav::ResourceType::Collection]),
                dav::PropertyRequest::GetContentType => dav::Property::GetContentType("httpd/unix-directory".into()),
                v => return Err(v),
            };
            Ok(prop)
        }).boxed()
    }

    fn put<'a>(&'a self, _policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        todo!()
    }

    fn content(&self) -> Content<'static> {
        futures::stream::once(futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported))).boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

#[derive(Clone)]
pub(crate) struct CalendarNode {
    col: Arc<Calendar>,
    calname: String,
}
impl DavNode for CalendarNode {
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed()
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
                },
                (None, true) => {
                    let child = Box::new(CreateEventNode {
                        col: col.clone(),
                        calname,
                        filename: path[0].to_string(),
                    });
                    child.fetch(user, &path[1..], create).await
                }, 
                _ => Err(anyhow!("Not found")),
            }

        }.boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        let col = self.col.clone();
        let calname = self.calname.clone();

        async move {
            col.dag().await.idx_by_filename.iter().map(|(filename, blob_id)| {
                Box::new(EventNode { 
                    col: col.clone(),
                    calname: calname.clone(),
                    filename: filename.to_string(),
                    blob_id: *blob_id,
                }) as Box<dyn DavNode>
            }).collect()
        }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/", user.username, self.calname)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::SupportedCalendarComponentSet)),
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let calname = self.calname.to_string();

        futures::stream::iter(prop.0).map(move |n| {
            let prop = match n {
                dav::PropertyRequest::DisplayName =>  dav::Property::DisplayName(format!("{} calendar", calname)),
                dav::PropertyRequest::ResourceType =>  dav::Property::ResourceType(vec![
                    dav::ResourceType::Collection,
                    dav::ResourceType::Extension(all::ResourceType::Cal(cal::ResourceType::Calendar)),
                ]),
                //dav::PropertyRequest::GetContentType => dav::AnyProperty::Value(dav::Property::GetContentType("httpd/unix-directory".into())),
                //@FIXME seems wrong but seems to be what Thunderbird expects...
                dav::PropertyRequest::GetContentType => dav::Property::GetContentType("text/calendar".into()),
                dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::SupportedCalendarComponentSet))
                    => dav::Property::Extension(all::Property::Cal(cal::Property::SupportedCalendarComponentSet(vec![
                        cal::CompSupport(cal::Component::VEvent),
                    ]))),
                v => return Err(v),
            };
            Ok(prop)
        }).boxed()
    }

    fn put<'a>(&'a self, _policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        todo!()
    }

    fn content<'a>(&'a self) -> Content<'static> {
        futures::stream::once(futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported))).boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

#[derive(Clone)]
pub(crate) struct EventNode {
    col: Arc<Calendar>,
    calname: String,
    filename: String,
    blob_id: BlobId,
}
impl EventNode {
    async fn etag(&self) -> Result<Etag> {
        self.col.dag().await.table.get(&self.blob_id).map(|(_, _, etag)| etag.to_string()).ok_or(anyhow!("Missing blob id in index"))
    }
}

impl DavNode for EventNode {
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed()
        }

        async { Err(anyhow!("Not supported: can't create a child on an event node")) }.boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/{}", user.username, self.calname, self.filename)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetEtag,
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarData(cal::CalendarDataRequest::default()))),
        ])
    }
    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let this = self.clone();

        futures::stream::iter(prop.0).then(move |n| {
            let this = this.clone();

            async move {
                let prop = match &n {
                    dav::PropertyRequest::DisplayName => dav::Property::DisplayName(format!("{} event", this.filename)),
                    dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![]),
                    dav::PropertyRequest::GetContentType => dav::Property::GetContentType("text/calendar".into()),
                    dav::PropertyRequest::GetEtag => dav::Property::GetEtag("\"abcdefg\"".into()),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Cal(cal::PropertyRequest::CalendarData(_req))) => {
                        let ics = String::from_utf8(this.col.get(this.blob_id).await.or(Err(n.clone()))?).or(Err(n.clone()))?;
                        
                        dav::Property::Extension(all::Property::Cal(cal::Property::CalendarData(cal::CalendarDataPayload { 
                            mime: None, 
                            payload: ics,
                        })))
                    },
                    _ => return Err(n),
                };
                Ok(prop)
            }
        }).boxed()
    }

    fn put<'a>(&'a self, policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        async {
            let existing_etag = self.etag().await?;
            match policy {
                PutPolicy::CreateOnly => bail!("Already existing"),
                PutPolicy::ReplaceEtag(etag) if etag != existing_etag.as_str() => bail!("Would overwrite something we don't know"),
                _ => ()
            };

            //@FIXME for now, our storage interface does not allow streaming,
            // so we load everything in memory
            let mut evt = Vec::new();
            let mut reader = stream.into_async_read();
            reader.read_to_end(&mut evt).await.unwrap();
            let (_token, entry) = self.col.put(self.filename.as_str(), evt.as_ref()).await?;
            Ok(entry.2)
        }.boxed()
    }

    fn content<'a>(&'a self) -> Content<'static> {
        //@FIXME for now, our storage interface does not allow streaming,
        // so we load everything in memory
        let calendar = self.col.clone();
        let blob_id = self.blob_id.clone();
        let r = async move {
            let content = calendar.get(blob_id).await.or(Err(std::io::Error::from(std::io::ErrorKind::Interrupted)));
            Ok(hyper::body::Bytes::from(content?))
        };
        futures::stream::once(Box::pin(r)).boxed()
    }

    fn content_type(&self) -> &str {
        "text/calendar"
    }
}

#[derive(Clone)]
pub(crate) struct CreateEventNode {
    col: Arc<Calendar>,
    calname: String,
    filename: String,
}
impl DavNode for CreateEventNode {
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = Box::new(self.clone()) as Box<dyn DavNode>;
            return async { Ok(node) }.boxed()
        }

        async { Err(anyhow!("Not supported: can't create a child on an event node")) }.boxed()
    }

    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![] }.boxed()
    }

    fn path(&self, user: &ArcUser) -> String {
        format!("/{}/calendar/{}/{}", user.username, self.calname, self.filename)
    }

    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All> {
        dav::PropName(vec![])
    }

    fn properties(&self, _user: &ArcUser, prop: dav::PropName<All>) ->  PropertyStream<'static> {
        futures::stream::iter(vec![]).boxed()
    }

    fn put<'a>(&'a self, _policy: PutPolicy, stream: Content<'a>) -> BoxFuture<'a, Result<Etag>> {
        //@NOTE: policy might not be needed here: whatever we put, there is no known entries here
        
        async {
            //@FIXME for now, our storage interface does not allow for streaming
            let mut evt = Vec::new();
            let mut reader = stream.into_async_read();
            reader.read_to_end(&mut evt).await.unwrap();
            let (_token, entry) = self.col.put(self.filename.as_str(), evt.as_ref()).await?;
            Ok(entry.2)
        }.boxed()
    }

    fn content(&self) -> Content<'static> {
        futures::stream::once(futures::future::err(std::io::Error::from(std::io::ErrorKind::Unsupported))).boxed()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}
