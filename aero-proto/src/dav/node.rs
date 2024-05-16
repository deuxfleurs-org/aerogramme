use anyhow::Result;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{BoxStream, StreamExt};
use hyper::body::Bytes;

use aero_collections::davdag::Etag;
use aero_dav::realization::All;
use aero_dav::types as dav;

use super::controller::ArcUser;

pub(crate) type Content<'a> = BoxStream<'a, std::result::Result<Bytes, std::io::Error>>;
pub(crate) type PropertyStream<'a> =
    BoxStream<'a, std::result::Result<dav::Property<All>, dav::PropertyRequest<All>>>;

pub(crate) enum PutPolicy {
    OverwriteAll,
    CreateOnly,
    ReplaceEtag(String),
}

/// A DAV node should implement the following methods
/// @FIXME not satisfied by BoxFutures but I have no better idea currently
pub(crate) trait DavNode: Send {
    // recurence, filesystem hierarchy
    /// This node direct children
    fn children<'a>(&self, user: &'a ArcUser) -> BoxFuture<'a, Vec<Box<dyn DavNode>>>;
    /// Recursively fetch a child (progress inside the filesystem hierarchy)
    fn fetch<'a>(
        &self,
        user: &'a ArcUser,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>>;

    // node properties
    /// Get the path
    fn path(&self, user: &ArcUser) -> String;
    /// Get the supported WebDAV properties
    fn supported_properties(&self, user: &ArcUser) -> dav::PropName<All>;
    /// Get the values for the given properties
    fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> PropertyStream<'static>;
    //fn properties(&self, user: &ArcUser, prop: dav::PropName<All>) -> Vec<dav::AnyProperty<All>>;
    /// Put an element (create or update)
    fn put<'a>(
        &'a self,
        policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>>;
    /// Content type of the element
    fn content_type(&self) -> &str;
    /// Get ETag
    fn etag(&self) -> BoxFuture<Option<Etag>>;
    /// Get content
    fn content<'a>(&self) -> Content<'a>;
    /// Delete
    fn delete(&self) -> BoxFuture<std::result::Result<(), std::io::Error>>;

    //@FIXME maybe add etag, maybe add a way to set content

    /// Utility function to get a propname response from a node
    fn response_propname(&self, user: &ArcUser) -> dav::Response<All> {
        dav::Response {
            status_or_propstat: dav::StatusOrPropstat::PropStat(
                dav::Href(self.path(user)),
                vec![dav::PropStat {
                    status: dav::Status(hyper::StatusCode::OK),
                    prop: dav::AnyProp(
                        self.supported_properties(user)
                            .0
                            .into_iter()
                            .map(dav::AnyProperty::Request)
                            .collect(),
                    ),
                    error: None,
                    responsedescription: None,
                }],
            ),
            error: None,
            location: None,
            responsedescription: None,
        }
    }

    /// Utility function to get a prop response from a node & a list of propname
    fn response_props(
        &self,
        user: &ArcUser,
        props: dav::PropName<All>,
    ) -> BoxFuture<'static, dav::Response<All>> {
        //@FIXME we should make the DAV parsed object a stream...
        let mut result_stream = self.properties(user, props);
        let path = self.path(user);

        async move {
            let mut prop_desc = vec![];
            let (mut found, mut not_found) = (vec![], vec![]);
            while let Some(maybe_prop) = result_stream.next().await {
                match maybe_prop {
                    Ok(v) => found.push(dav::AnyProperty::Value(v)),
                    Err(v) => not_found.push(dav::AnyProperty::Request(v)),
                }
            }

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
                status_or_propstat: dav::StatusOrPropstat::PropStat(dav::Href(path), prop_desc),
                error: None,
                location: None,
                responsedescription: None,
            }
        }
        .boxed()
    }
}
