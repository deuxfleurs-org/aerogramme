pub enum Error {
    /// Name:  lock-token-matches-request-uri
    ///
    /// Use with:  409 Conflict
    ///
    /// Purpose:  (precondition) -- A request may include a Lock-Token header
    /// to identify a lock for the UNLOCK method.  However, if the
    /// Request-URI does not fall within the scope of the lock identified
    /// by the token, the server SHOULD use this error.  The lock may have
    /// a scope that does not include the Request-URI, or the lock could
    /// have disappeared, or the token may be invalid.
    LockTokenMatchesRequestUri,

    /// Name:  lock-token-submitted (precondition)
    ///
    /// Use with:  423 Locked
    ///
    /// Purpose:  The request could not succeed because a lock token should
    /// have been submitted.  This element, if present, MUST contain at
    /// least one URL of a locked resource that prevented the request.  In
    /// cases of MOVE, COPY, and DELETE where collection locks are
    /// involved, it can be difficult for the client to find out which
    /// locked resource made the request fail -- but the server is only
    /// responsible for returning one such locked resource.  The server
    /// MAY return every locked resource that prevented the request from
    /// succeeding if it knows them all.
    ///
    /// <!ELEMENT lock-token-submitted (href+) >
    LockTokenSubmitted(Vec<String>),
    NoConflictingLock,
    NoExternalEntities,
    PreservedLiveProperties,
    PropfindFiniteDepth,
    Calendar(u64),
}

/// 14.1.  activelock XML Element
///
/// Name:   activelock
///
/// Purpose:   Describes a lock on a resource.
/// <!ELEMENT activelock (lockscope, locktype, depth, owner?, timeout?,
///           locktoken?, lockroot)>
pub struct ActiveLock {
    lockscope: u64,
    locktype: u64,
    depth: Depth,
    owner: Option<u64>,
    timeout: Option<u64>,
}

/// allprop XML Element
///
/// Name:   allprop
///
/// Purpose:   Specifies that all names and values of dead properties and
/// the live properties defined by this document existing on the
/// resource are to be returned.
///
/// <!ELEMENT allprop EMPTY >
pub struct AllProp{}

/// collection XML Element
///
/// Name:   collection
///
/// Purpose:   Identifies the associated resource as a collection.  The
/// DAV:resourcetype property of a collection resource MUST contain
/// this element.  It is normally empty but extensions may add sub-
/// elements.
///
/// <!ELEMENT collection EMPTY >
pub struct Collection{}

/// depth XML Element
///
/// Name:   depth
///
/// Purpose:   Used for representing depth values in XML content (e.g.,
/// in lock information).
///
/// Value:   "0" | "1" | "infinity"
///
/// <!ELEMENT depth (#PCDATA) >
pub enum Depth {
    Zero,
    One,
    Infinity
}

/// 14.6.  exclusive XML Element
///
/// Name:   exclusive
///
/// Purpose:   Specifies an exclusive lock.
/// 
/// <!ELEMENT exclusive EMPTY >
pub struct Exclusive {}

pub struct Href(String);

pub struct Status(String);

pub struct ResponseDescription(String);

pub struct Location(Href);

/// 14.18.  prop XML Element
///
/// Name:   prop
///
/// Purpose:   Contains properties related to a resource.
///
/// Description:   A generic container for properties defined on
/// resources.  All elements inside a 'prop' XML element MUST define
/// properties related to the resource, although possible property
/// names are in no way limited to those property names defined in
/// this document or other standards.  This element MUST NOT contain
/// text or mixed content.
///
/// <!ELEMENT prop ANY >
pub struct Prop {
    something: u64,
}

/// propstat XML Element
///
/// Name:   propstat
///
/// Purpose:   Groups together a prop and status element that is
/// associated with a particular 'href' element.
///
/// Description:   The propstat XML element MUST contain one prop XML
/// element and one status XML element.  The contents of the prop XML
/// element MUST only list the names of properties to which the result
/// in the status element applies.  The optional precondition/
/// postcondition element and 'responsedescription' text also apply to
/// the properties named in 'prop'.
///
/// <!ELEMENT propstat (prop, status, error?, responsedescription?) >
pub struct PropStat {
    prop: Prop,
    status: Status,
    error: Option<Error>,
    responsedescription: Option<ResponseDescription>,
}

/// 14.24.  response XML Element
///
/// Name:   response
///
/// Purpose:   Holds a single response describing the effect of a method
/// on resource and/or its properties.
///
/// Description:   The 'href' element contains an HTTP URL pointing to a
/// WebDAV resource when used in the 'response' container.  A
/// particular 'href' value MUST NOT appear more than once as the
/// child of a 'response' XML element under a 'multistatus' XML
/// element.  This requirement is necessary in order to keep
/// processing costs for a response to linear time.  Essentially, this
/// prevents having to search in order to group together all the
/// responses by 'href'.  There are, however, no requirements
/// regarding ordering based on 'href' values.  The optional
/// precondition/postcondition element and 'responsedescription' text
/// can provide additional information about this resource relative to
/// the request or result.
///
/// <!ELEMENT response (href, ((href*, status)|(propstat+)),
///                     error?, responsedescription? , location?) >
pub struct Response {
    href: Vec<Href>,
    status: Status,
    propstat: Vec<PropStat>,
    error: Option<Error>,
    responsedescription: Option<ResponseDescription>,
    location: Option<u64>,
}

/// 14.16.  multistatus XML Element
///
/// Name:   multistatus
///
/// Purpose:   Contains multiple response messages.
///
/// Description:   The 'responsedescription' element at the top level is
/// used to provide a general message describing the overarching
/// nature of the response.  If this value is available, an
/// application may use it instead of presenting the individual
/// response descriptions contained within the responses.
///
/// <!ELEMENT multistatus (response*, responsedescription?)  >
pub struct Multistatus {
    responses: Vec<Response>,
    responsedescription: Option<ResponseDescription>,
}


