use anyhow::Result;
use futures::Stream;
use futures::future::BoxFuture;

use aero_dav::types as dav;
use aero_dav::realization::All;
use aero_collections::user::User;

type ArcUser = std::sync::Arc<User>;
pub(crate) type Content = Box<dyn Stream<Item=Result<u64>>>;

pub(crate) enum PutPolicy {
    CreateOnly,
    ReplaceEtags(String),
}

/// A DAV node should implement the following methods
/// @FIXME not satisfied by BoxFutures but I have no better idea currently
pub(crate) trait DavNode: Send {
    // recurence, filesystem hierarchy
    /// This node direct children
    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>>;
    /// Recursively fetch a child (progress inside the filesystem hierarchy)
    fn fetch<'a>(&self, user: &'a ArcUser, path: &'a [&str], create: bool) -> BoxFuture<'a, Result<Box<dyn DavNode>>>;

    // node properties
    /// Get the path
    fn path(&self, user: &ArcUser) -> String;
    /// Get the supported WebDAV properties
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All>;
    /// Get the values for the given properties
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>>;
    /// Put an element (create or update)
    fn put(&self, policy: PutPolicy, stream: Content) -> BoxFuture<Result<()>>;
    /// Get content
    //fn content(&self) -> TryStream;

    //@FIXME maybe add etag, maybe add a way to set content

    /// Utility function to get a propname response from a node
    fn response_propname(&self, user: &ArcUser) -> dav::Response<All> {
        dav::Response {
            status_or_propstat: dav::StatusOrPropstat::PropStat(
                dav::Href(self.path(user)), 
                vec![
                    dav::PropStat {
                        status: dav::Status(hyper::StatusCode::OK), 
                        prop: dav::AnyProp(self.supported_properties(user).0.into_iter().map(dav::AnyProperty::Request).collect()),
                        error: None,
                        responsedescription: None,
                    }
                ],
            ),
            error: None,
            location: None,
            responsedescription: None
        }
    }

    /// Utility function to get a prop response from a node & a list of propname
    fn response_props(&self, user: &ArcUser, props: dav::PropName<All>) -> dav::Response<All> {
        let mut prop_desc = vec![];
        let (found, not_found): (Vec<_>, Vec<_>) = self.properties(user, props).into_iter().partition(|v| matches!(v, dav::AnyProperty::Value(_)));

        // If at least one property has been found on this object, adding a HTTP 200 propstat to
        // the response
        if !found.is_empty() {
            prop_desc.push(dav::PropStat { 
                status: dav::Status(hyper::StatusCode::OK), 
                prop: dav::AnyProp(found),
                error: None,
                responsedescription: None,
            });
        }

        // If at least one property can't be found on this object, adding a HTTP 404 propstat to
        // the response
        if !not_found.is_empty() {
            prop_desc.push(dav::PropStat { 
                status: dav::Status(hyper::StatusCode::NOT_FOUND), 
                prop: dav::AnyProp(not_found),
                error: None,
                responsedescription: None,
            })
        }

        // Build the finale response
        dav::Response {
            status_or_propstat: dav::StatusOrPropstat::PropStat(dav::Href(self.path(user)), prop_desc),
            error: None,
            location: None,
            responsedescription: None
        }
    }
}

