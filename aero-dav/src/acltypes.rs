use super::coretypes as dav;

/** 
 * # WebDAV ACL & ACL Current Principal Extension
 *
 * The 1000-feet view: ACL introduces concepts that are useful, but we don't use/implement it
 * directly. Based on this concept, we implement a simple RFC that can return the homedir URL.
 *
 * ## RFC 3744 - WebDAV ACL
 *
 * It introduces the notion of principals, the principal is a name that define "who you are" (aka.
 * authentication, identity). Then a concept of permissions (aka authorization, called access control here)
 * can be written to give permission to "users" (ie. "principals") on resources.
 *
 * For now, we are not interested by the "authorization" part of WebDAV, only by the idea that now
 * we have a notion of "identity", and an "identity" is an URL. This URL, at least in our case, is
 * seen as the "home directory" of the user. That's really what is interesting us about ACL in
 * Aerogramme: the idea that we have authentication, and that authenticated users have their own
 * home directory.
 *
 * Will probably not implement:
 *   - Section 3 privileges (eg. DAV:read, DAV:write-properties, DAV:bind, etc.).
 *   - Section 5 access control properties (eg. DAV:owner, DAV:supported-privilege-setm etc.)
 *   - Section 6 (how ACL must be evaluated)
 *   - Section 7 (how other methods behave in presence of ACL)
 *   - Section 8 (a dedicated ACL method to configure - write - ACL on resources)
 *
 * **Not implemented** but could be considered if required by a client:
 *   - Section 4, Principal Properties (eg. DAV:principal-URL, etc.)
 *   - Section 9, the REPORT method with DAV:principal-match as a way to find the principal URL (aka home directory).
 *     - The CalDAV spec recommend we discover the homedir this way
 *     - But Thunderbird uses RFC5397 instead, which is simpler and better
 *
 * This implementation is missing
 *
 * ## RFC 5397 - WebDAV ACL Current Principal Extension
 *
 * It introduces the "current user principal" DAV property. 
 * This information is used by Thunderbird.
 *
 * This implementation should be complete.
 */

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    Owner,
    CurrentUserPrincipal,
    CurrentUserPrivilegeSet,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property {
    Owner(dav::Href),
    CurrentUserPrincipal(User),
    CurrentUserPrivilegeSet(Vec<Privilege>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Principal,
}

/// Not implemented, it's a placeholder
#[derive(Debug, PartialEq, Clone)]
pub struct Privilege(());

#[derive(Debug, PartialEq, Clone)]
pub enum User {
    Unauthenticated,
    Authenticated(dav::Href),
}
