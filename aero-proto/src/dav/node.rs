use futures::io::AsyncReadExt;
use futures::future::{BoxFuture, FutureExt};
use futures::stream::{BoxStream, StreamExt, TryStreamExt};
use hyper::body::Bytes;

use aero_collections::{
    dav::collection::Collection,
    dav::davindex::{BlobId, Etag, SyncChange, Token},
};
use aero_collections::user::User;
use aero_dav::realization::{self as all, All};
use aero_dav::coretypes as dav;
use aero_dav::synctypes as sync;
use aero_dav::versioningtypes as vers;

use crate::dav::codec::SyncTokenUri;
use crate::dav::multistatus::multistatus;

pub(crate) type IOResult<T> = std::result::Result<T, std::io::Error>;
pub(crate) type Content<'a> = BoxStream<'a, IOResult<Bytes>>;
pub(crate) type PropertyResult =
    std::result::Result<dav::Property<All>, dav::PropertyRequest<All>>;

pub(crate) enum PutPolicy {
    OverwriteAll,
    CreateOnly,
    ReplaceEtag(String),
}

pub(crate) enum ChildNode {
    Existing(Box<dyn DavNode>),
    Creating(Box<dyn DavNode>),
    CannotCreate,
}

/// The response to a REPORT operation.
///
/// RFC 3253 does not define a common format for report responses, but in
/// practice, all the ones we implement return a Multistatus in case of success.
pub(crate) enum ReportResponse {
    Ok(dav::Multistatus<All>),
    Err((hyper::StatusCode, String))
}

/// A DAV node should implement the following methods
/// @FIXME not satisfied by BoxFutures but I have no better idea currently
pub(crate) trait DavNode: Send {
    // --- recurence, filesystem hierarchy

    /// Clone self as a new node.
    fn clone_node(&self) -> Box<dyn DavNode>;

    /// List direct children of this node
    fn children<'a>(&self, user: &'a User) -> BoxFuture<'a, Vec<String>>;

    /// Create a `DavNode` for a child of this node.
    ///
    /// - If `name` is in `self.children()`, this function must return
    ///   `ChildNode::Existing()` with the corresponding node.
    /// - If `name` is not in `self.children()`:
    ///   + if the node supports creating new children, this must return
    ///     `ChildNode::Creating()` with a node for the new child.
    ///   + if the node does not support adding children, this must return
    ///     `ChildNode::CannotCreate`.
    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>>;

    // --- node properties

    /// Get the path
    fn path(&self, user: &User) -> String;
    /// Get the supported WebDAV properties
    fn supported_properties(&self, user: &User) -> dav::PropName<All>;
    /// Get the values for the given properties
    fn properties<'a>(&'a mut self, user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>>;
    /// Get the value of the DAV header to return
    fn dav_header(&self) -> String;
    /// Content type of the element
    fn content_type(&self) -> &str;

    // --- operations (with default impls, intended to be redefined)

    /// Put an element (create or update)
    fn put<'a>(
        &'a mut self,
        _policy: PutPolicy,
        _stream: Content<'a>,
    ) -> BoxFuture<'a, IOResult<Etag>> {
        async { Err(unsupported()) }.boxed()
    }
    /// Get ETag
    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        async { None }.boxed()
    }
    /// Get content
    fn content<'a>(&self) -> Content<'a> {
        futures::stream::once(futures::future::err(unsupported())).boxed()
    }
    /// Delete
    fn delete<'a>(&'a mut self) -> BoxFuture<'a, IOResult<()>> {
        async { Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)) }.boxed()
    }
    /// Report (introduced by RFC 3253)
    /// NOTE: to handle reports it is not enough to define `report`, the
    /// property vers::Property::SupportedReportSet must be set to the list of
    /// supported report types (in `supported_properties` and `property`).
    fn report<'a>(&'a mut self, _user: &'a User, _report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async { Err(unsupported()) }.boxed()
    }

    // --- derived utility functions, not intended to be redefined

    fn children_nodes<'a>(&self, user: &'a User) -> BoxFuture<'a, IOResult<Vec<Box<dyn DavNode>>>> {
        let this = self.clone_node();
        async move {
            let mut res = vec![];
            for name in this.children(user).await {
                match this.child_node(user, &name).await? {
                    ChildNode::Existing(n) => res.push(n),
                    _ => unreachable!(),
                }
            }
            Ok(res)
        }.boxed()
    }
    
    /// Recursively fetch a child (progress inside the filesystem hierarchy)
    fn fetch<'a>(
        &self,
        user: &'a User,
        path: &'a [&str],
        create: bool,
    ) -> BoxFuture<'a, IOResult<Box<dyn DavNode>>> {
        if path.len() == 0 {
            let node = self.clone_node();
            return async { Ok(node) }.boxed();
        }

        let this = self.clone_node();
        async move {
            let child = match this.child_node(user, path[0]).await? {
                ChildNode::Existing(n) => n,
                ChildNode::Creating(n) if create => n,
                ChildNode::Creating(_) => return Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                ChildNode::CannotCreate =>
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "can't create child on this node",
                    )),
            };
            child.fetch(user, &path[1..], create).await
        }.boxed()
    }

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
    fn response_props<'a>(
        &'a mut self,
        user: &'a User,
        props: dav::PropName<All>,
    ) -> BoxFuture<'a, dav::Response<All>> {
        //@FIXME we should make the DAV parsed object a stream...
        let path = self.path(user);
        let prop_results = self.properties(user, props);

        async move {
            let mut prop_desc = vec![];
            let (mut found, mut not_found) = (vec![], vec![]);
            for maybe_prop in prop_results.await {
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

fn unsupported() -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::Unsupported)
}

// ---- DavNode sub-traits that factor generic WebDAV behaviors

/// A `DavObject` is a `DavNode` that represents an object (and not a collection).
///
/// `DavObject` implements `DavNode`, providing generic WebDAV behavior for
/// objects, which can be further extended by implementors of `DavObject` (with
/// in particular `additional_supported_properties` and `additional_property`).
pub(crate) trait DavObject: Send + Sync + Clone + 'static {
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
    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult>;

    /// Supported report types.
    fn supported_reports(&self) -> Vec<vers::SupportedReport<All>>;

    /// Handler for reports.
    fn report<'a>(&'a mut self, user: &'a User, report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>>;

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

#[derive(Clone)]
pub(crate) struct DavObjectNode<T>(pub T);

impl<T: DavObject> DavNode for DavObjectNode<T>
{
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }
    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<String>> {
        async { vec![] }.boxed()
    }
    fn child_node<'a>(&self, _user: &'a User, _name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
        async { Ok(ChildNode::CannotCreate) }.boxed()
    }

    fn path(&self, user: &User) -> String {
        self.0.path(user)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        let mut props = vec![];
        if self.0.blob_id().is_some() {
            props.extend_from_slice(&[
                dav::PropertyRequest::DisplayName,
                dav::PropertyRequest::ResourceType,
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                    vers::PropertyRequest::SupportedReportSet,
                )),
            ]);
            props.extend_from_slice(&self.0.additional_supported_properties());
        }
        dav::PropName(props)
    }
    fn properties<'a>(&'a mut self, _user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async move {
            let mut v = vec![];
            for n in prop.0 {
                if let Ok(prop) = self.0.additional_property(&n).await {
                    v.push(Ok(prop));
                    continue
                }

                let res = match &n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName(format!("{}", self.0.filename()))),
                    dav::PropertyRequest::ResourceType =>
                        Ok(dav::Property::ResourceType(vec![])),
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType(self.content_type().to_string())),
                    dav::PropertyRequest::GetEtag =>
                        self.etag().await.ok_or(n.clone()).map(dav::Property::GetEtag),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                        vers::PropertyRequest::SupportedReportSet,
                    )) =>
                        Ok(dav::Property::Extension(all::Property::Vers(
                            vers::Property::SupportedReportSet(self.0.supported_reports())
                        ))),
                    _ => Err(n),
                };
                v.push(res)
            }
            v
        }.boxed()
    }

    fn dav_header(&self) -> String {
        "1, access-control".into()
    }

    fn content_type(&self) -> &str {
        self.0.content_type()
    }

    fn put<'a>(
        &'a mut self,
        policy: PutPolicy,
        stream: Content<'a>,
    ) -> BoxFuture<'a, IOResult<Etag>> {
        async {
            let blob_id = self.0.blob_id();
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
            let filename = self.0.filename().to_string();
            let (_token, (_, etag)) = self
                .0
                .collection_mut()
                .put(&filename, evt.as_ref())
                .await
                .or(Err(std::io::ErrorKind::Interrupted))?;
            self.0
                .collection_mut()
                .sync()
                .await
                .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(etag)
        }
        .boxed()
    }

    fn content<'a>(&self) -> Content<'a> {
        if let Some(blob_id) = self.0.blob_id() {
            //@FIXME for now, our storage interface does not allow streaming,
            // so we load everything in memory
            // NOTE: we need to clone the collection so that the returned
            // `Content` outlives the lifetime of `&self`. Cloning collections
            // is cheap so this is fine.
            let col = self.0.collection().clone();
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

    fn etag(&self) -> BoxFuture<'_, Option<Etag>> {
        if let Some(blob_id) = self.0.blob_id() { 
            let col = self.0.collection().clone();

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

    fn delete<'a>(&'a mut self) -> BoxFuture<'a, IOResult<()>> {
        let blob_id = match self.0.blob_id() {
            None => {
                // Nothing to delete
                return async { Ok(()) }.boxed()
            },
            Some(blob_id) => blob_id,
        };

        async move {
            let _token = match self.0.collection_mut().delete(blob_id).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err=?e, "delete object node");
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
                }
            };
            self.0.collection_mut()
                  .sync()
                  .await
                  .or(Err(std::io::ErrorKind::ConnectionReset))?;
            Ok(())
        }
        .boxed()
    }

    fn report<'a>(&'a mut self, user: &'a User, report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        self.0.report(user, report)
    }
}

/// A `DavStoredCollection` is a `DavNode` that represents a collection backed by
/// a Bayou DAV store.
///
/// `DavStoredCollection` implements `DavNode`, implementing base WebDAV
/// behavior, including WebDAV Sync, which can be further extended by
/// implementors of `DavStoredCollection` through the `additional_*` methods.
pub(crate) trait DavStoredCollection: Send + Sync + Clone + 'static {
    /// Access to the underlying collection store
    fn collection(&self) -> &Collection;
    fn collection_mut(&mut self) -> &mut Collection;

    /// Collection display name
    fn display_name(&self) -> String;

    /// WebDAV path at which this collection is located
    fn path(&self, user: &User) -> String;

    /// Collection content-type
    fn content_type(&self) -> &str;
    
    /// Create a `DavNode` instance for an element of the collection.
    /// It is possible for `filename` not to be in the collection already
    /// (if creating a new element).
    fn mk_child_node(&self, filename: &str) -> Box<dyn DavNode>;

    /// Additional resource types advertised by this collection.
    fn additional_resource_types(&self) -> Vec<dav::ResourceType<All>>; 

    /// Additional properties supported by this collection, on top of base WebDAV properties.
    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>>;

    /// Getter for the value of properties listed by `additional_supported_properties`.
    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult>;

    /// Additional report types advertised by this collection, on top of sync reports.
    fn additional_supported_reports(&self) -> Vec<vers::SupportedReport<All>>;

    /// Handler for reports declared by `additional_supported_reports`.
    fn additional_report<'a>(&'a mut self, user: &'a User, report: &'a vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>>;

    /// Additional Dav headers to advertise.
    fn additional_dav_headers(&self) -> Vec<String>;
}

#[derive(Clone)]
pub struct DavStoredCollectionNode<T>(pub T);

impl<T: DavStoredCollection> DavNode for DavStoredCollectionNode<T>
{
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<String>> {
        let res = self.0
                .collection()
                .index()
                .idx_by_filename
                .keys()
                .map(|name| name.to_string())
                .collect();
        async move { res }.boxed()
    }

    fn child_node<'a>(&self, _user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
        let exists = self.0.collection().index().idx_by_filename.contains_key(name);
        let node = self.0.mk_child_node(name);
        async move {
            if exists {
                Ok(ChildNode::Existing(node))
            } else {
                Ok(ChildNode::Creating(node))
            }
        }.boxed()
    }

    fn path(&self, user: &User) -> String {
        self.0.path(user)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        let mut props = vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Sync(
                sync::PropertyRequest::SyncToken,
            )),
            dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                vers::PropertyRequest::SupportedReportSet,
            )),
        ];
        props.extend_from_slice(&self.0.additional_supported_properties());
        dav::PropName(props)
    }

    fn properties<'a>(&'a mut self, _user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async {
            let mut v = vec![];
            for n in prop.0 {
                if let Ok(prop) = self.0.additional_property(&n).await {
                    v.push(Ok(prop));
                    continue
                }
                    
                let res = match n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName(self.0.display_name().clone())),
                    dav::PropertyRequest::ResourceType => {
                        let mut typ = vec![dav::ResourceType::Collection];
                        typ.extend(self.0.additional_resource_types());
                        Ok(dav::Property::ResourceType(typ))
                    },
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType(self.content_type().to_string())),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Sync(
                        sync::PropertyRequest::SyncToken,
                    )) => match self.0.collection_mut().token().await {
                        Ok(token) => Ok(dav::Property::Extension(all::Property::Sync(
                            sync::Property::SyncToken(sync::SyncToken(
                                SyncTokenUri(token).to_string()
                            ))))),
                        _ => Err(n.clone()),
                    },
                    dav::PropertyRequest::Extension(all::PropertyRequest::Vers(
                        vers::PropertyRequest::SupportedReportSet,
                    )) => {
                        let mut reports = vec![
                            vers::SupportedReport(vers::ReportName::Extension(
                                all::ReportTypeName::Sync(sync::ReportTypeName::SyncCollection),
                            )),
                        ];
                        reports.extend(self.0.additional_supported_reports());
                        Ok(dav::Property::Extension(all::Property::Vers(
                            vers::Property::SupportedReportSet(reports)
                        )))
                    },
                    v => Err(v),
                };
                v.push(res)
            }
            v
        }.boxed()
    }
    
    fn dav_header(&self) -> String {
        let mut parts = vec!["1, access-control".to_string()];
        parts.extend(self.0.additional_dav_headers());
        parts.join(", ")
    }

    fn content_type(&self) -> &str {
        self.0.content_type()
    }

    fn report<'a>(&'a mut self, user: &'a User, report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async move {
            if let Ok(res) = self.0.additional_report(user, &report).await {
                return Ok(res)
            }

            match report {
                vers::Report::Extension(all::ReportType::Sync(sync_col)) => {
                    if sync_col.limit.is_some() {
                        tracing::warn!("limit is not supported, ignoring");
                    }
                    if matches!(sync_col.sync_level, sync::SyncLevel::Infinite) {
                        tracing::debug!("aerogramme calendar collections are not nested");
                    }

                    let token = match sync_col.sync_token {
                        sync::SyncTokenRequest::InitialSync => None,
                        sync::SyncTokenRequest::IncrementalSync(token_raw) => {
                            let token = token_raw.parse::<SyncTokenUri>()
                                .map_err(|msg| std::io::Error::new(std::io::ErrorKind::Other, msg))?
                                .0;
                            Some(token)
                        }
                    };
                    // do the diff
                    let (new_token, ok_node, not_found) = match self.diff(token).await {
                        Ok(t) => t,
                        Err(e) => match e.kind() {
                            std::io::ErrorKind::NotFound =>
                                return Ok(ReportResponse::Err((
                                    hyper::StatusCode::GONE,
                                    "Diff failed, token might be expired".to_string(),
                                ))),
                            _ =>
                                return Err(e),
                        },
                    };
                    let extension = Some(all::Multistatus::Sync(sync::Multistatus {
                        sync_token: sync::SyncToken(SyncTokenUri(new_token).to_string()),
                    }));
                    
                    Ok(ReportResponse::Ok(multistatus(
                        user,
                        ok_node,
                        not_found,
                        dav::PropFind::Prop(sync_col.prop),
                        extension
                    ).await))
                },
                _ => Err(unsupported())
            }
        }.boxed()
    }
}

impl <T: DavStoredCollection> DavStoredCollectionNode<T> {
    /// Helper function, used to compute Sync reports. Computes a diff of
    /// changes since a given sync token.
    fn diff<'a>(
        &'a mut self,
        sync_token: Option<Token>,
    ) -> BoxFuture<'a, IOResult<(Token, Vec<Box<dyn DavNode>>, Vec<dav::Href>)>> {
        async move {
            let sync_token = match sync_token {
                Some(v) => v,
                None => {
                    let token = self
                        .0
                        .collection_mut()
                        .token()
                        .await
                        .or(Err(std::io::Error::from(std::io::ErrorKind::Interrupted)))?;
                    let ok_nodes = self
                        .0
                        .collection()
                        .index()
                        .idx_by_filename
                        .iter()
                        .map(|(filename, _)| self.0.mk_child_node(filename))
                        .collect();

                    return Ok((token, ok_nodes, vec![]));
                }
            };
            let (new_token, listed_changes) = match self.0.collection_mut().diff(sync_token).await {
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
                    SyncChange::Ok((filename, _)) => {
                        ok_nodes.push(self.0.mk_child_node(&filename));
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
