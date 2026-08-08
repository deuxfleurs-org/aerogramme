use aero_collections::user::User;
use aero_dav::acltypes as acl;
use aero_dav::coretypes as dav;
use aero_dav::realization::{self as all, All};

use crate::dav::node::DavNode;

/// Include-list of properties returned by 'allprop' when used in PROPFIND and
/// reports that piggy-back on PROPFIND (eg Caldav & Carddav reports).
const ALLPROP: [dav::PropertyRequest<All>; 10] = [
    dav::PropertyRequest::CreationDate,
    dav::PropertyRequest::DisplayName,
    dav::PropertyRequest::GetContentLanguage,
    dav::PropertyRequest::GetContentLength,
    dav::PropertyRequest::GetContentType,
    dav::PropertyRequest::GetEtag,
    dav::PropertyRequest::GetLastModified,
    dav::PropertyRequest::LockDiscovery,
    dav::PropertyRequest::ResourceType,
    dav::PropertyRequest::SupportedLock,
];

pub struct Builder {
    inner: dav::Multistatus<All>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            inner: dav::Multistatus {
                responses: vec![],
                responsedescription: None,
                extension: None,
            }
        }
    }

    #[allow(dead_code)]
    pub fn with_description(mut self, desc: String) -> Self {
        self.inner.responsedescription = Some(dav::ResponseDescription(desc));
        self
    }

    pub fn with_extension(mut self, ext: all::Multistatus) -> Self {
        self.inner.extension = Some(ext);
        self
    }

    pub async fn with_propfind_nodes(
        mut self,
        user: &User,
        propfind: dav::PropFind<All>,
        nodes: Vec<Box<dyn DavNode>>,
    ) -> Self {
        // Collect properties on nodes
        match &propfind {
            // Request a list of names of all the properties defined on the
            // resource, by using the 'propname' element.
            dav::PropFind::PropName =>
                for node in nodes {
                    self.inner.responses.push(node.response_propname(user))
                },

            // Request property values for those properties defined in this
            // specification (at a minimum) plus dead properties, by using the
            // 'allprop' element (the 'include' element can be used with
            // 'allprop' to instruct the server to also include additional live
            // properties that may not have been returned otherwise),
            //
            // Note that 'allprop' does not return values for all live
            // properties. Instead, WebDAV clients can use propname requests to
            // discover what live properties exist, and request named properties
            // when retrieving values.
            dav::PropFind::AllProp(include) =>
                for mut node in nodes {
                    let mut props: Vec<_> = node
                        .supported_properties(user)
                        .0
                        .into_iter()
                        .filter(|p| ALLPROP.contains(p))
                        .collect();
                    if let Some(dav::Include(include)) = include {
                        props.extend_from_slice(include);
                    }
                    self.inner
                        .responses
                        .push(node.response_props(user, dav::PropName(props)).await)
                },

            // Request particular property values, by naming the properties
            // desired within the 'prop' element (the ordering of properties in
            // here MAY be ignored by the server),
            dav::PropFind::Prop(inner) =>
                for mut node in nodes {
                    self.inner
                        .responses
                        .push(node.response_props(user, inner.clone()).await)
                },
        }
        self
    }

    pub fn with_not_found(mut self, not_found: Vec<dav::Href>) -> Self {
        if !not_found.is_empty() {
            self.inner.responses.push(dav::Response {
                status_or_propstat: dav::StatusOrPropstat::Status(
                    not_found,
                    dav::Status(hyper::StatusCode::NOT_FOUND),
                ),
                error: None,
                location: None,
                responsedescription: None,
            });
        }
        self
    }

    pub fn with_limit_reached(mut self, href: dav::Href) -> Self {
        self.inner.responses.push(dav::Response {
            status_or_propstat: dav::StatusOrPropstat::Status(
                vec![href],
                dav::Status(hyper::http::StatusCode::INSUFFICIENT_STORAGE),
            ),
            error: Some(dav::Error(vec![
                dav::Violation::Extension(
                    all::Error::Acl(acl::Violation::NumberOfMatchesWithinLimits)
                ),
            ])),
            responsedescription: None,
            location: None,
        });
        self
    }

    pub fn build(self) -> dav::Multistatus<All> {
        tracing::debug!(multistatus=?self.inner, "multistatus response");
        self.inner        
    }
}
