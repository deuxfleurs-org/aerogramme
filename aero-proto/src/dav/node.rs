use anyhow::{anyhow, Result};
use futures::io::AsyncReadExt;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{BoxStream, StreamExt, TryStreamExt};
use hyper::body::Bytes;

use aero_collections::{
    dav::collection::Collection,
    dav::davindex::{BlobId, Etag, Token},
};
use aero_collections::user::User;
use aero_dav::realization::All;
use aero_dav::coretypes as dav;

pub(crate) type Content<'a> = BoxStream<'a, std::result::Result<Bytes, std::io::Error>>;
pub(crate) type PropertyStream<'a> = BoxStream<'a, PropertyResult>;
pub(crate) type PropertyResult =
    std::result::Result<dav::Property<All>, dav::PropertyRequest<All>>;

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
    fn children<'a>(&self, user: &'a User) -> BoxFuture<'a, Vec<Box<dyn DavNode>>>;
    /// Recursively fetch a child (progress inside the filesystem hierarchy)
    fn fetch<'a>(
        &self,
        user: &'a User,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, Result<Box<dyn DavNode>>>;

    // node properties
    /// Get the path
    fn path(&self, user: &User) -> String;
    /// Get the supported WebDAV properties
    fn supported_properties(&self, user: &User) -> dav::PropName<All>;
    /// Get the values for the given properties
    fn properties(&self, user: &User, prop: dav::PropName<All>) -> PropertyStream<'static>;
    /// Get the value of the DAV header to return
    fn dav_header(&self) -> String;

    /// Put an element (create or update)
    fn put<'a>(
        &'a mut self,
        policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>>;
    /// Content type of the element
    fn content_type(&self) -> &str;
    /// Get ETag
    fn etag(&self) -> BoxFuture<'_, Option<Etag>>;
    /// Get content
    fn content<'a>(&self) -> Content<'a>;
    /// Delete
    fn delete(&self) -> BoxFuture<'_, std::result::Result<(), std::io::Error>>;
    /// Sync
    fn diff<'a>(
        &self,
        sync_token: Option<Token>,
    ) -> BoxFuture<
        'a,
        std::result::Result<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>), std::io::Error>,
    >;

    /// Utility function to get a propname response from a node
    fn response_propname(&self, user: &User) -> dav::Response<All> {
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
        user: &User,
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

// ---- DavNode sub-traits that factor generic WebDAV behaviors

/// A `DavObject` is a `DavNode` that represents an object (and not a collection).
///
/// `DavObject` implements `DavNode`, providing generic WebDAV behavior for
/// objects, which can be further extended by implementors of `DavObject` (with
/// in particular `additional_supported_properties` and `additional_property`).
pub(crate) trait DavObject: Send + Sync + Clone {
    /// Access to the underlying collection store containing this object
    fn collection(&self) -> &Collection;
    fn collection_mut(&mut self) -> &mut Collection;

    /// Filename of the object in the collection
    fn filename(&self) -> &str;

    /// WebDAV path at which this object is located
    fn path(&self, user: &User) -> String;

    /// Content type of the object.
    fn content_type(&self) -> &str;

    /// Additional properties supported by this object, on top of base WebDAV properties.
    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>>;

    /// Getter for the value of properties listed by `additional_supported_properties`.
    fn additional_property<'a>(&self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult>;

    /// Helper to get the id of the object contents in the store.
    /// Returns `None` if the object does not exist in the store.
    fn blob_id(&self) -> Option<BlobId> {
        self.collection()
            .index()
            .idx_by_filename
            .get(self.filename())
            .cloned()
    }
}

impl<T: DavObject + 'static> DavNode for T {
    fn fetch<'a>(
        &self,
        _user: &'a User,
        path: &'a [&str],
        _create: bool,
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

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<Box<dyn DavNode>>> {
        async { vec![] }.boxed()
    }

    fn path(&self, user: &User) -> String {
        DavObject::path(self, user)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        let mut props = vec![];
        if self.blob_id().is_some() {
            props.extend_from_slice(&[
                dav::PropertyRequest::DisplayName,
                dav::PropertyRequest::ResourceType,
                dav::PropertyRequest::GetEtag,
            ]);
            props.extend_from_slice(&self.additional_supported_properties());
        }
        dav::PropName(props)
    }
    fn properties(&self, _user: &User, prop: dav::PropName<All>) -> PropertyStream<'static> {
        let this = self.clone();
        futures::stream::iter(prop.0)
            .then(move |n| {
                let this = this.clone();

                async move {
                    if let Ok(prop) = this.additional_property(&n).await {
                        return Ok(prop)
                    }

                    let prop = match &n {
                        dav::PropertyRequest::DisplayName => {
                            dav::Property::DisplayName(format!("{}", this.filename()))
                        }
                        dav::PropertyRequest::ResourceType => dav::Property::ResourceType(vec![]),
                        dav::PropertyRequest::GetContentType => {
                            dav::Property::GetContentType(this.content_type().to_string())
                        }
                        dav::PropertyRequest::GetEtag => {
                            let etag = this.etag().await.ok_or(n.clone())?;
                            dav::Property::GetEtag(etag)
                        }
                        _ => return Err(n),
                    };
                    Ok(prop)
                }
            })
            .boxed()
    }

    fn put<'a>(
        &'a mut self,
        policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, std::result::Result<Etag, std::io::Error>> {
        async {
            let blob_id = self.blob_id();
            match policy {
                PutPolicy::CreateOnly if blob_id.is_some() => {
                    return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
                }
                PutPolicy::ReplaceEtag(etag) if blob_id.is_some() => {
                    let existing_etag = self
                        .etag()
                        .await
                        .ok_or(std::io::Error::new(std::io::ErrorKind::Other, "Etag error"))?;
                    if etag != existing_etag.as_str() {
                        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
                    }
                }
                _ => (),
            }

            //@FIXME for now, our storage interface does not allow streaming,
            // so we load everything in memory
            let mut evt = Vec::new();
            let mut reader = stream.into_async_read();
            reader
                .read_to_end(&mut evt)
                .await
                .or(Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe)))?;
            let filename = self.filename().to_string();
            let (_token, (_, etag)) = self
                .collection_mut()
                .put(&filename, evt.as_ref())
                .await
                .or(Err(std::io::ErrorKind::Interrupted))?;
            self.collection_mut()
                .sync()
                .await
                .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(etag)
        }
        .boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        if let Some(blob_id) = self.blob_id() {
            //@FIXME for now, our storage interface does not allow streaming,
            // so we load everything in memory
            let col = self.collection().clone();
            let blob = async move {
                let raw = col
                    .get(blob_id)
                    .await
                    .or(Err(std::io::Error::from(std::io::ErrorKind::Interrupted)))?;

                Ok(hyper::body::Bytes::from(raw))
            };
            futures::stream::once(Box::pin(blob)).boxed()
        } else {
            futures::stream::once(futures::future::err(std::io::Error::from(
                std::io::ErrorKind::Unsupported,
            )))
            .boxed()
        }
    }

    fn content_type(&self) -> &str {
        self.content_type()
    }

    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        if let Some(blob_id) = self.blob_id() { 
            let col = self.collection().clone();

            async move {
                col
                    .index()
                    .table
                    .get(&blob_id)
                    .map(|(_, etag)| etag.to_string())
            }
            .boxed()
        } else {
            async { None }.boxed()
        }
    }

    fn delete(&self) -> BoxFuture<'_, std::result::Result<(), std::io::Error>> {
        let blob_id = match self.blob_id() {
            None => {
                // Nothing to delete
                return async { Ok(()) }.boxed()
            },
            Some(blob_id) => blob_id,
        };

        let mut col = self.collection().clone();

        async move {
            let _token = match col.delete(blob_id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err=?e, "delete object node");
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
                }
            };
            col
                .sync()
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

    fn dav_header(&self) -> String {
        "1, access-control".into()
    }
}
