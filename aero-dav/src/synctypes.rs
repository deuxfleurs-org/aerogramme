use super::types as dav;
use super::versioningtypes as vers;

// RFC 6578
// https://datatracker.ietf.org/doc/html/rfc6578

//@FIXME add SyncTokenRequest to PropertyRequest
//@FIXME add SyncToken to Property
//@FIXME add SyncToken to Multistatus

///  Name:  sync-collection
///
/// Namespace:  DAV:
///
/// Purpose:  WebDAV report used to synchronize data between client and
/// server.
///
/// Description:  See Section 3.
///
/// <!ELEMENT sync-collection (sync-token, sync-level, limit?, prop)>
///
/// <!-- DAV:limit defined in RFC 5323, Section 5.17 -->
/// <!-- DAV:prop defined in RFC 4918, Section 14.18 -->

#[derive(Debug, PartialEq, Clone)]
pub struct SyncCollection<E: dav::Extension> {
    pub sync_token: SyncTokenRequest,
    pub sync_level: SyncLevel,
    pub limit: Option<vers::Limit>,
    pub prop: dav::PropName<E>,
}

/// Name:  sync-token
///
/// Namespace:  DAV:
///
/// Purpose:  The synchronization token provided by the server and
/// returned by the client.
///
/// Description:  See Section 3.
///
/// <!ELEMENT sync-token CDATA>
///
/// <!-- Text MUST be a URI -->
/// Used by multistatus
#[derive(Debug, PartialEq, Clone)]
pub struct SyncToken(pub String);

/// Used by propfind and report sync-collection
#[derive(Debug, PartialEq, Clone)]
pub enum SyncTokenRequest {
    InitialSync,
    IncrementalSync(String),
}

/// Name:  sync-level
///
/// Namespace:  DAV:
///
/// Purpose:  Indicates the "scope" of the synchronization report
/// request.
///
/// Description:  See Section 3.3.
#[derive(Debug, PartialEq, Clone)]
pub enum SyncLevel {
    One,
    Infinite,
}
