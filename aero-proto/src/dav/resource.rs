//! This modules defines the WebDAV filesystem structure exposed by aerogramme
//! 
//! /                                 root
//! ├── alice                         homedir for user "alice"
//! │   └── calendar                  calendar namespace
//! │   │   └── Personal              default calendar collection
//! │   │       └── event1.ics        calendar event
//! │   │       └── ...
//! │   └── addressbook               addressbook namespace
//! │       └── Personal              default addressbook collection
//! ├── bob                           homedir for user "bob"
//! │   └── ...

use futures::{future::BoxFuture, future::FutureExt};

use aero_collections::user::User;
use aero_dav::acltypes as acl;
use aero_dav::caltypes as cal;
use aero_dav::cardtypes as card;
use aero_dav::realization::{self as all, All};
use aero_dav::coretypes as dav;

use crate::dav::node::{
    ChildNode,
    DavNode,
    IOResult,
    PropertyResult,
};
use crate::dav::cal_resource::CalendarListNode;
use crate::dav::card_resource::AddressbookListNode;

// FIXME: must advertise support of webdav 3 for carddav (cf dav_header)

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

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
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
        async { vec!["calendar".to_string(), "addressbook".to_string()] }.boxed()
    }

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
        if name == "calendar" {
            async move {
                let callist = CalendarListNode::new(user)
                    .await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e))?; 
                let node = Box::new(callist) as Box<dyn DavNode>;
                Ok(ChildNode::Existing(node))
            }.boxed()
        } else if name == "addressbook" { 
            async move {
                let cardlist = AddressbookListNode::new(user)
                    .await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e))?;
                let node = Box::new(cardlist) as Box<dyn DavNode>;
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
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                acl::PropertyRequest::CurrentUserPrivilegeSet,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Cal(
                cal::PropertyRequest::CalendarHomeSet,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                card::PropertyRequest::AddressbookHomeSet,
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
                    dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                        acl::PropertyRequest::CurrentUserPrivilegeSet,
                    )) =>
                        Ok(dav::Property::Extension(all::Property::Acl(
                            acl::Property::CurrentUserPrivilegeSet(
                                acl::PrivilegeSet(vec![acl::Privilege::All]))
                        ))),
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
                    dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                        card::PropertyRequest::AddressbookHomeSet,
                    )) => Ok(dav::Property::Extension(all::Property::Card(
                        card::Property::AddressbookHomeSet(dav::Href(
                            //@FIXME same as above?
                            format!("/{}/addressbook/", user.username),
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
        "1, 3, access-control, calendar-access, addressbook".into()
    }
}
