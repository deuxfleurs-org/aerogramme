//! DAV Resources for carddav:
//! addressbook namespace, addressbook collection, and addressbook contact item.

use anyhow::Result;
use futures::{future::BoxFuture, future::FutureExt};
use futures::stream::{Stream, StreamExt, TryStreamExt};

use aero_collections::{
    dav::collection::Collection,
    user::User,
};
use aero_dav::acltypes as acl;
use aero_dav::cardtypes as card;
use aero_dav::coretypes as dav;
use aero_dav::realization::{self as all, All};
use aero_dav::versioningtypes as vers;

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

/// The addressbook namespace of a user. It contains addressbook collections.
#[derive(Clone)]
pub(crate) struct AddressbookListNode {
    list: Vec<String>,
}
impl AddressbookListNode {
    pub async fn new(user: &User) -> Result<Self> {
        let list = user.calendars.dav.list().await?;
        Ok(Self { list })
    }
}
impl DavNode for AddressbookListNode {
    fn clone_node(&self) -> Box<dyn DavNode> {
        Box::new(self.clone())
    }

    fn children<'a>(&self, _user: &'a User) -> BoxFuture<'a, Vec<String>> {
        let list = self.list.clone();
        async move { list }.boxed()
    }

    fn child_node<'a>(&self, user: &'a User, name: &str) -> BoxFuture<'a, IOResult<ChildNode>> {
        let addrbookname = name.to_string();
        async {
            let col = user
                .addressbooks
                .dav
                .open(&addrbookname)
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e))?;
            match col {
                //@FIXME: allow creating new calendar nodes
                None => Ok(ChildNode::CannotCreate),
                Some(col) => {
                    Ok(ChildNode::Existing(
                        Box::new(DavStoredCollectionNode(AddressbookNode {
                            col,
                            addrbookname,
                        })) as Box<dyn DavNode>
                    ))
                }
            }
        }.boxed()
    }

    fn path(&self, user: &User) -> String {
        format!("/{}/addressbook/", user.username)
    }

    fn supported_properties(&self, _user: &User) -> dav::PropName<All> {
        dav::PropName(vec![
            dav::PropertyRequest::DisplayName,
            dav::PropertyRequest::ResourceType,
            dav::PropertyRequest::GetContentType,
            dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                acl::PropertyRequest::CurrentUserPrivilegeSet,
            )),
        ])
    }
    fn properties<'a>(&'a mut self, user: &'a User, prop: dav::PropName<All>) -> BoxFuture<'a, Vec<PropertyResult>> {
        async move {
            let mut v = vec![];
            for n in prop.0 {
                let res = match n {
                    dav::PropertyRequest::DisplayName =>
                        Ok(dav::Property::DisplayName(format!("{} addressbooks", user.username))),
                    dav::PropertyRequest::ResourceType =>
                        Ok(dav::Property::ResourceType(vec![dav::ResourceType::Collection])),
                    dav::PropertyRequest::GetContentType =>
                        Ok(dav::Property::GetContentType("httpd/unix-directory".into())),
                    dav::PropertyRequest::Extension(all::PropertyRequest::Acl(
                        acl::PropertyRequest::CurrentUserPrivilegeSet,
                    )) =>
                        Ok(dav::Property::Extension(all::Property::Acl(
                            acl::Property::CurrentUserPrivilegeSet(
                                acl::PrivilegeSet(vec![acl::Privilege::All])
                            )
                        ))),
                    v => Err(v),
                };
                v.push(res)
            }
            v
        }.boxed()
    }

    fn dav_header(&self) -> String {
        "1, access-control, addressbook".into()
    }

    fn content_type(&self) -> &str {
        "text/plain"
    }
}

/// An addressbook collection. It contains addressbook objects.
#[derive(Clone)]
pub(crate) struct AddressbookNode {
    col: Collection,
    addrbookname: String,
}
impl DavStoredCollection for AddressbookNode {
    fn collection(&self) -> &Collection {
        &self.col
    }

    fn collection_mut(&mut self) -> &mut Collection {
        &mut self.col
    }

    fn display_name(&self) -> String {
        format!("{} addressbook", self.addrbookname)
    }

    fn path(&self, user: &User) -> String {
        format!("/{}/addressbook/{}/", user.username, self.addrbookname)
    }

    fn content_type(&self) -> &str {
        // TODO: is this correct?
        "httpd/unix-directory"
    }
    
    fn mk_child_node(&self, filename: &str) -> Box<dyn DavNode> {
        Box::new(DavObjectNode(AddressbookObject {
            col: self.col.clone(),
            addrbookname: self.addrbookname.clone(),
            filename: filename.to_string(),
        }))
    }

    fn additional_resource_types(&self) -> Vec<dav::ResourceType<All>> {
        vec![
            dav::ResourceType::Extension(all::ResourceType::Card(
                card::ResourceType::Addressbook,
            )),
        ]
    }

    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>> {
        vec![
            dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                card::PropertyRequest::SupportedCollationSet
            ))
        ]         
    }

    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult> {
        async move {
            match prop {
                dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                    card::PropertyRequest::SupportedCollationSet
                )) => {
                    Ok(dav::Property::Extension(all::Property::Card(
                        card::Property::SupportedCollationSet(vec![
                            card::SupportedCollation(card::Collation::UnicodeCaseMap),
                            card::SupportedCollation(card::Collation::AsciiCaseMap),
                        ])
                    )))
                },
                _ => Err(prop.clone()),
            }
        }.boxed()
    }

    fn additional_supported_reports(&self) -> Vec<vers::SupportedReport<All>> {
        vec![
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Card(card::ReportTypeName::Multiget),
            )),
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Card(card::ReportTypeName::Query),
            )),
        ]
    }

    fn additional_report<'a>(&'a mut self, user: &'a User, report: &'a vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async {
            match report {
                vers::Report::Extension(all::ReportType::Card(card::ReportType::Multiget(m))) => {
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
                                not_found.push(h)
                        }
                    }

                    Ok(ReportResponse::Ok(
                        multistatus::Builder::new()
                            .with_propfind_nodes(user, selector_to_propfind(m.selector.clone()), ok_node)
                            .await
                            .with_not_found(not_found)
                            .build()))
                },

                vers::Report::Extension(all::ReportType::Card(card::ReportType::Query(q))) => {
                    let children_nodes: Vec<_> = self
                        .col
                        .index()
                        .idx_by_filename
                        .keys()
                        .map(|name| self.mk_child_node(name))
                        .collect();

                    let (ok_node, limit_reached) =
                        apply_limit(apply_filter(children_nodes, &q.filter), &q.limit).await?;
                    let mut status = multistatus::Builder::new()
                        .with_propfind_nodes(user, selector_to_propfind(q.selector.clone()), ok_node)
                        .await;
                    if limit_reached {
                        status = status.with_limit_reached(dav::Href(self.path(user)))
                    }
                    Ok(ReportResponse::Ok(status.build()))
                },

                _ => Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
            }
        }.boxed()
    }
    
    fn additional_dav_headers(&self) -> Vec<String> {
        vec!["addressbook".to_string()]
    }
}

/// An addressbok object, which may or may not exist in storage. It is a single object.
#[derive(Clone)]
pub(crate) struct AddressbookObject {
    col: Collection,
    addrbookname: String,
    filename: String,
}

impl DavObject for AddressbookObject {
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
            "/{}/addressbook/{}/{}",
            user.username, self.addrbookname, self.filename
        )
    }

    fn content_type(&self) -> &str {
        "text/vcard"
    }

    fn additional_supported_properties(&self) -> Vec<dav::PropertyRequest<All>> {
        vec![
            dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                card::PropertyRequest::SupportedCollationSet
            ))
        ]
    }

    fn additional_property<'a>(&'a mut self, prop: &'a dav::PropertyRequest<All>) -> BoxFuture<'a, PropertyResult> {
        let this = self.clone();
        async move {
            match prop {
                dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                    card::PropertyRequest::SupportedCollationSet
                )) => {
                    Ok(dav::Property::Extension(all::Property::Card(
                        card::Property::SupportedCollationSet(vec![
                            card::SupportedCollation(card::Collation::UnicodeCaseMap),
                            card::SupportedCollation(card::Collation::AsciiCaseMap),
                        ])
                    )))
                },
                // This is not a "real" property (it cannot be queried by
                // PROPFIND), but is queried internally by addressbook reports.
                dav::PropertyRequest::Extension(all::PropertyRequest::Card(
                    card::PropertyRequest::AddressData(req),
                )) => {
                    let blob_id = this.blob_id().ok_or(prop.clone())?;
                    let vcard = this.col.get(blob_id).await.or(Err(prop.clone()))?;
                    let filtered_vcard = match &req.prop_kind {
                        None | Some(card::PropKind::AllProp) => vcard,
                        Some(prop_kind) => {
                            let filtered = aero_vcard::parse_lossy(vcard.as_slice())
                                .into_iter()
                                .filter_map(|line| aero_vcard::filter::property(&line, prop_kind));

                            let mut buf = vec![];
                            aero_vcard::write(&mut buf, filtered).or(Err(prop.clone()))?;
                            buf
                        }
                    };
                    
                    Ok(dav::Property::Extension(all::Property::Card(
                        card::Property::AddressData(card::AddressDataPayload {
                            payload: String::from_utf8(filtered_vcard).or(Err(prop.clone()))?,
                            content_type: Default::default(),
                            version: Default::default(),
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
                all::ReportTypeName::Card(card::ReportTypeName::Multiget),
            )),
            vers::SupportedReport(vers::ReportName::Extension(
                all::ReportTypeName::Card(card::ReportTypeName::Query),
            )),
        ]
    }

    fn report<'a>(&'a mut self, user: &'a User, report: vers::Report<All>) -> BoxFuture<'a, IOResult<ReportResponse>> {
        async {
            match report {
                vers::Report::Extension(all::ReportType::Card(card::ReportType::Multiget(m))) => {
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
                            .build()))
                },

                vers::Report::Extension(all::ReportType::Card(card::ReportType::Query(q))) => {
                    let nodes = vec![Box::new(DavObjectNode(self.clone())) as Box<dyn DavNode>];

                    let (ok_node, limit_reached) =
                        apply_limit(apply_filter(nodes, &q.filter), &q.limit).await?;

                    let mut status = multistatus::Builder::new()
                        .with_propfind_nodes(user, selector_to_propfind(q.selector.clone()), ok_node)
                        .await;
                    if limit_reached {
                        status = status.with_limit_reached(dav::Href(self.path(user)))
                    }
                    Ok(ReportResponse::Ok(status.build()))
                },

                _ => Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
            }
        }.boxed()
    }

    fn additional_dav_headers(&self) -> Vec<String> {
        vec!["addressbook".to_string()]
    }
}

fn selector_to_propfind(s: Option<card::AddressbookSelector<All>>) -> dav::PropFind<All> {
    match s {
        None | Some(card::AddressbookSelector::AllProp) => dav::PropFind::AllProp(None),
        Some(card::AddressbookSelector::PropName) => dav::PropFind::PropName,
        Some(card::AddressbookSelector::Prop(inner)) => dav::PropFind::Prop(inner),
    }
}

//@FIXME equally naive implementation as cal_resource::apply_filter
fn apply_filter<'a>(
    nodes: Vec<Box<dyn DavNode>>,
    filter: &'a card::Filter,
) -> impl Stream<Item = std::result::Result<Box<dyn DavNode>, std::io::Error>> + Unpin + 'a {
    futures::stream::iter(nodes).filter_map(move |single_node| async move {
        // Get vCard
        let chunks: Vec<_> = match single_node.content().try_collect().await {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let raw_vcard = chunks.iter().fold(Vec::new(), |mut acc, single_chunk| {
            acc.extend_from_slice(single_chunk.as_ref());
            acc
        });
        // Parse vCard
        let vcard = aero_vcard::parse_lossy(raw_vcard.as_slice());
        if aero_vcard::query::object_matches_filter(vcard.as_slice(), filter) {
            Some(Ok(single_node))
        } else {
            None
        }
    }).boxed()
}

// returns (items within limit, limit_reached?)
async fn apply_limit(
    mut items: impl Stream<Item = std::result::Result<Box<dyn DavNode>, std::io::Error>> + Unpin,
    limit: &Option<card::Limit>,
) -> std::result::Result<(Vec<Box<dyn DavNode>>, bool), std::io::Error> {
    let mut items_keep = vec![];
    while let Some(item_res) = items.next().await {
        let item = item_res?;
        match limit {
            Some(card::Limit { nresults }) if *nresults <= items_keep.len() as u64 =>
                return Ok((items_keep, true)),
            _ =>
                items_keep.push(item),
        }
    }
    Ok((items_keep, false))
}
