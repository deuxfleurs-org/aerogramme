use aero_collections::user::User;
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

/// Build a multistatus response from a list of DavNodes
pub(crate) async fn multistatus(
    user: &User,
    nodes: Vec<Box<dyn DavNode>>,
    not_found: Vec<dav::Href>,
    propfind: dav::PropFind<All>,
    extension: Option<all::Multistatus>,
) -> dav::Multistatus<All> {
    let mut responses: Vec<dav::Response<All>> = vec![];

    // Collect properties on existing objects
    match &propfind {
        // Request a list of names of all the properties defined on the
        // resource, by using the 'propname' element.
        dav::PropFind::PropName =>
            for node in nodes {
                responses.push(node.response_propname(user))
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
                responses.push(node.response_props(user, dav::PropName(props)).await)
            },

        // Request particular property values, by naming the properties
        // desired within the 'prop' element (the ordering of properties in
        // here MAY be ignored by the server),
        dav::PropFind::Prop(inner) =>
            for mut node in nodes {
                responses.push(node.response_props(user, inner.clone()).await)
            },
    }

    // Register not found objects only if relevant
    if !not_found.is_empty() {
        responses.push(dav::Response {
            status_or_propstat: dav::StatusOrPropstat::Status(
                not_found,
                dav::Status(hyper::StatusCode::NOT_FOUND),
            ),
            error: None,
            location: None,
            responsedescription: None,
        });
    }

    // Build response
    let multistatus = dav::Multistatus::<All> {
        responses,
        responsedescription: None,
        extension,
    };

    tracing::debug!(multistatus=?multistatus, "multistatus response");
    multistatus
}
