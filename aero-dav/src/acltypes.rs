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
    CurrentUserPrivilegeSet(PrivilegeSet),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Principal,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Violation {
    /// (DAV:number-of-matches-within-limits): The number of matching
    /// principals must fall within server-specific, predefined limits.
    /// For example, this condition might be triggered if a search
    /// specification would cause the return of an extremely large number
    /// of responses.
    NumberOfMatchesWithinLimits,
    
    // TODO: not a complete list
}

/// Privileges
///
/// Ability to perform a given method on a resource MUST be controlled by
/// one or more privileges. [...]
/// 
/// --- Notes on "privilege aggregation":
/// 
/// Server implementations are free to aggregate the predefined
/// privileges (defined above in Sections 3.1-3.10) subject to the
/// following limitations:
///
/// DAV:read-acl MUST NOT contain DAV:read, DAV:write, DAV:write-acl,
/// DAV:write-properties, DAV:write-content, or DAV:read-current-user-
/// privilege-set.
///
/// DAV:write-acl MUST NOT contain DAV:write, DAV:read, DAV:read-acl, or
/// DAV:read-current-user-privilege-set.
///
/// DAV:read-current-user-privilege-set MUST NOT contain DAV:write,
/// DAV:read, DAV:read-acl, or DAV:write-acl.
///
/// DAV:write MUST NOT contain DAV:read, DAV:read-acl, or DAV:read-
/// current-user-privilege-set.
///
/// DAV:read MUST NOT contain DAV:write, DAV:write-acl, DAV:write-
/// properties, or DAV:write-content.
///
/// DAV:write MUST contain DAV:bind, DAV:unbind, DAV:write-properties and
/// DAV:write-content.
// NOTE: for consistency, we implement serialization/deserialization of all the
// privileges defined by the RFC, but currently most of them are not used or
// supported further.
#[derive(Debug, PartialEq, Clone)]
pub enum Privilege {
    /// The read privilege controls methods that return information about the
    /// state of the resource, including the resource's properties.  Affected
    /// methods include GET and PROPFIND.  Any implementation-defined
    /// privilege that also controls access to GET and PROPFIND must be
    /// aggregated under DAV:read - if an ACL grants access to DAV:read, the
    /// client may expect that no other privilege needs to be granted to have
    /// access to GET and PROPFIND.  Additionally, the read privilege MUST
    /// control the OPTIONS method.
    /// 
    /// <!ELEMENT read EMPTY>
    Read,

    /// The write privilege controls methods that lock a resource or modify
    /// the content, dead properties, or (in the case of a collection)
    /// membership of the resource, such as PUT and PROPPATCH.  Note that
    /// state modification is also controlled via locking (see section 5.3 of
    /// [RFC2518]), so effective write access requires that both write
    /// privileges and write locking requirements are satisfied.  Any
    /// implementation-defined privilege that also controls access to methods
    /// modifying content, dead properties or collection membership must be
    /// aggregated under DAV:write, e.g., if an ACL grants access to
    /// DAV:write, the client may expect that no other privilege needs to be
    /// granted to have access to PUT and PROPPATCH.
    /// 
    /// <!ELEMENT write EMPTY>
    Write,

    /// The DAV:write-properties privilege controls methods that modify the
    /// dead properties of the resource, such as PROPPATCH.  Whether this
    /// privilege may be used to control access to any live properties is
    /// determined by the implementation.  Any implementation-defined
    /// privilege that also controls access to methods modifying dead
    /// properties must be aggregated under DAV:write-properties - e.g., if
    /// an ACL grants access to DAV:write-properties, the client can safely
    /// expect that no other privilege needs to be granted to have access to
    /// PROPPATCH.
    /// 
    /// <!ELEMENT write-properties EMPTY>
    WriteProperties,

    /// The DAV:write-content privilege controls methods that modify the
    /// content of an existing resource, such as PUT.  Any implementation-
    /// defined privilege that also controls access to content must be
    /// aggregated under DAV:write-content - e.g., if an ACL grants access to
    /// DAV:write-content, the client can safely expect that no other
    /// privilege needs to be granted to have access to PUT.  Note that PUT -
    /// when applied to an unmapped URI - creates a new resource and
    /// therefore is controlled by the DAV:bind privilege on the parent
    /// collection.
    /// 
    /// <!ELEMENT write-content EMPTY>
    WriteContent,

    /// The DAV:unlock privilege controls the use of the UNLOCK method by a
    /// principal other than the lock owner (the principal that created a
    /// lock can always perform an UNLOCK).  While the set of users who may
    /// lock a resource is most commonly the same set of users who may modify
    /// a resource, servers may allow various kinds of administrators to
    /// unlock resources locked by others.  Any privilege controlling access
    /// by non-lock owners to UNLOCK MUST be aggregated under DAV:unlock.
    /// 
    /// A lock owner can always remove a lock by issuing an UNLOCK with the
    /// correct lock token and authentication credentials.  That is, even if
    /// a principal does not have DAV:unlock privilege, they can still remove
    /// locks they own.  Principals other than the lock owner can remove a
    /// lock only if they have DAV:unlock privilege and they issue an UNLOCK
    /// with the correct lock token.  Lock timeout is not affected by the
    /// DAV:unlock privilege.
    /// 
    /// <!ELEMENT unlock EMPTY>
    Unlock,

    /// The DAV:read-acl privilege controls the use of PROPFIND to retrieve
    /// the DAV:acl property of the resource.
    /// 
    /// <!ELEMENT read-acl EMPTY>
    ReadAcl,

    /// The DAV:read-current-user-privilege-set privilege controls the use of
    /// PROPFIND to retrieve the DAV:current-user-privilege-set property of
    /// the resource.
    /// 
    /// Clients are intended to use this property to visually indicate in
    /// their UI items that are dependent on the permissions of a resource,
    /// for example, by graying out resources that are not writable.
    /// 
    /// This privilege is separate from DAV:read-acl because there is a need
    /// to allow most users access to the privileges permitted the current
    /// user (due to its use in creating the UI), while the full ACL contains
    /// information that may not be appropriate for the current authenticated
    /// user.  As a result, the set of users who can view the full ACL is
    /// expected to be much smaller than those who can read the current user
    /// privilege set, and hence distinct privileges are needed for each.
    /// 
    /// <!ELEMENT read-current-user-privilege-set EMPTY>
    ReadCurrentUserPrivilegeSet,

    /// The DAV:write-acl privilege controls use of the ACL method to modify
    /// the DAV:acl property of the resource.
    /// 
    /// <!ELEMENT write-acl EMPTY>
    WriteAcl,

    /// The DAV:bind privilege allows a method to add a new member URL to the
    /// specified collection (for example via PUT or MKCOL).  It is ignored
    /// for resources that are not collections.
    /// 
    /// <!ELEMENT bind EMPTY>
    Bind,

    /// The DAV:unbind privilege allows a method to remove a member URL from
    /// the specified collection (for example via DELETE or MOVE).  It is
    /// ignored for resources that are not collections.
    /// 
    /// <!ELEMENT unbind EMPTY>
    Unbind,

    /// DAV:all is an aggregate privilege that contains the entire set of
    /// privileges that can be applied to the resource.
    /// 
    /// <!ELEMENT all EMPTY>
    All,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PrivilegeSet(pub Vec<Privilege>);

#[derive(Debug, PartialEq, Clone)]
pub enum User {
    Unauthenticated,
    Authenticated(dav::Href),
}
