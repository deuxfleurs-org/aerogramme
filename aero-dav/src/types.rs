#![allow(dead_code)]
use std::fmt::Debug;

use chrono::{DateTime,FixedOffset};
use super::xml;

/// It's how we implement a DAV extension
/// (That's the dark magic part...)
pub trait Extension: std::fmt::Debug + PartialEq + Clone {
    type Error: xml::Node<Self::Error>;
    type Property: xml::Node<Self::Property>;
    type PropertyRequest: xml::Node<Self::PropertyRequest>;
    type ResourceType: xml::Node<Self::ResourceType>;
}

/// 14.1.  activelock XML Element
///
/// Name:   activelock
///
/// Purpose:   Describes a lock on a resource.
/// <!ELEMENT activelock (lockscope, locktype, depth, owner?, timeout?,
///           locktoken?, lockroot)>
#[derive(Debug, PartialEq, Clone)]
pub struct ActiveLock {
    pub lockscope: LockScope,
    pub locktype: LockType,
    pub depth: Depth,
    pub owner: Option<Owner>,
    pub timeout: Option<Timeout>,
    pub locktoken: Option<LockToken>,
    pub lockroot: LockRoot,
}

/// 14.3 collection XML Element
///
/// Name:   collection
///
/// Purpose:   Identifies the associated resource as a collection.  The
/// DAV:resourcetype property of a collection resource MUST contain
/// this element.  It is normally empty but extensions may add sub-
/// elements.
///
/// <!ELEMENT collection EMPTY >
#[derive(Debug, PartialEq)]
pub struct Collection{}

/// 14.4 depth XML Element
///
/// Name:   depth
///
/// Purpose:   Used for representing depth values in XML content (e.g.,
/// in lock information).
///
/// Value:   "0" | "1" | "infinity"
///
/// <!ELEMENT depth (#PCDATA) >
#[derive(Debug, PartialEq, Clone)]
pub enum Depth {
    Zero,
    One,
    Infinity
}

/// 14.5 error XML Element
///
/// Name:   error
///
/// Purpose:   Error responses, particularly 403 Forbidden and 409
///   Conflict, sometimes need more information to indicate what went
///  wrong.  In these cases, servers MAY return an XML response body
///   with a document element of 'error', containing child elements
///   identifying particular condition codes.
///
/// Description:   Contains at least one XML element, and MUST NOT
///  contain text or mixed content.  Any element that is a child of the
///   'error' element is considered to be a precondition or
///   postcondition code.  Unrecognized elements MUST be ignored.
///
/// <!ELEMENT error ANY >
#[derive(Debug, PartialEq, Clone)]
pub struct Error<E: Extension>(pub Vec<Violation<E>>);
#[derive(Debug, PartialEq, Clone)]
pub enum Violation<E: Extension> {
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
    LockTokenSubmitted(Vec<Href>),

    /// Name:  no-conflicting-lock (precondition)
    ///
    /// Use with:  Typically 423 Locked
    ///
    /// Purpose:  A LOCK request failed due the presence of an already
    /// existing conflicting lock.  Note that a lock can be in conflict
    /// although the resource to which the request was directed is only
    /// indirectly locked.  In this case, the precondition code can be
    /// used to inform the client about the resource that is the root of
    /// the conflicting lock, avoiding a separate lookup of the
    /// "lockdiscovery" property.
    ///
    /// <!ELEMENT no-conflicting-lock (href)* >
    NoConflictingLock(Vec<Href>),

    /// Name:  no-external-entities
    ///
    /// Use with:  403 Forbidden
    ///
    /// Purpose:  (precondition) -- If the server rejects a client request
    /// because the request body contains an external entity, the server
    /// SHOULD use this error.
    NoExternalEntities,

    /// Name:  preserved-live-properties
    ///
    /// Use with:  409 Conflict
    ///
    /// Purpose:  (postcondition) -- The server received an otherwise-valid
    /// MOVE or COPY request, but cannot maintain the live properties with
    /// the same behavior at the destination.  It may be that the server
    /// only supports some live properties in some parts of the
    /// repository, or simply has an internal error.
    PreservedLiveProperties,

    /// Name:  propfind-finite-depth
    ///
    /// Use with:  403 Forbidden
    ///
    /// Purpose:  (precondition) -- This server does not allow infinite-depth
    /// PROPFIND requests on collections.
    PropfindFiniteDepth,

    
    /// Name:  cannot-modify-protected-property
    ///
    /// Use with:  403 Forbidden
    ///
    /// Purpose:  (precondition) -- The client attempted to set a protected
    /// property in a PROPPATCH (such as DAV:getetag).  See also
    /// [RFC3253], Section 3.12.
    CannotModifyProtectedProperty,

    /// Specific errors
    Extension(E::Error),
}

/// 14.6.  exclusive XML Element
///
/// Name:   exclusive
///
/// Purpose:   Specifies an exclusive lock.
/// 
/// <!ELEMENT exclusive EMPTY >
#[derive(Debug, PartialEq)]
pub struct Exclusive {}

/// 14.7.  href XML Element
///
/// Name:   href
///
/// Purpose:   MUST contain a URI or a relative reference.
///
/// Description:   There may be limits on the value of 'href' depending
///  on the context of its use.  Refer to the specification text where
///   'href' is used to see what limitations apply in each case.
///
/// Value:   Simple-ref
///
/// <!ELEMENT href (#PCDATA)>
#[derive(Debug, PartialEq, Clone)]
pub struct Href(pub String);


/// 14.8.  include XML Element
///
/// Name:   include
///
/// Purpose:   Any child element represents the name of a property to be
/// included in the PROPFIND response.  All elements inside an
/// 'include' XML element MUST define properties related to the
/// resource, although possible property names are in no way limited
/// to those property names defined in this document or other
/// standards.  This element MUST NOT contain text or mixed content.
///
/// <!ELEMENT include ANY >
#[derive(Debug, PartialEq, Clone)]
pub struct Include<E: Extension>(pub Vec<PropertyRequest<E>>);

/// 14.9.  location XML Element
///
/// Name:   location
///
/// Purpose:   HTTP defines the "Location" header (see [RFC2616], Section
/// 14.30) for use with some status codes (such as 201 and the 300
/// series codes).  When these codes are used inside a 'multistatus'
/// element, the 'location' element can be used to provide the
/// accompanying Location header value.
///
/// Description:   Contains a single href element with the same value
/// that would be used in a Location header.
///
/// <!ELEMENT location (href)>
#[derive(Debug, PartialEq, Clone)]
pub struct Location(pub Href);

/// 14.10.  lockentry XML Element
///
/// Name:   lockentry
///
/// Purpose:   Defines the types of locks that can be used with the
/// resource.
///
/// <!ELEMENT lockentry (lockscope, locktype) >
#[derive(Debug, PartialEq, Clone)]
pub struct LockEntry {
    pub lockscope: LockScope,
    pub locktype: LockType,
}

/// 14.11.  lockinfo XML Element
///
/// Name:   lockinfo
///
/// Purpose:   The 'lockinfo' XML element is used with a LOCK method to
/// specify the type of lock the client wishes to have created.
///
/// <!ELEMENT lockinfo (lockscope, locktype, owner?)  >
#[derive(Debug, PartialEq, Clone)]
pub struct LockInfo {
    pub lockscope: LockScope,
    pub locktype: LockType,
    pub owner: Option<Owner>,
}

/// 14.12.  lockroot XML Element
///
/// Name:   lockroot
///
/// Purpose:   Contains the root URL of the lock, which is the URL
/// through which the resource was addressed in the LOCK request.
///
/// Description:   The href element contains the root of the lock.  The
/// server SHOULD include this in all DAV:lockdiscovery property
/// values and the response to LOCK requests.
///
/// <!ELEMENT lockroot (href) >
#[derive(Debug, PartialEq, Clone)]
pub struct LockRoot(pub Href);

/// 14.13.  lockscope XML Element
///
/// Name:   lockscope
///
/// Purpose:   Specifies whether a lock is an exclusive lock, or a shared
/// lock.
/// <!ELEMENT lockscope (exclusive | shared) >
#[derive(Debug, PartialEq, Clone)]
pub enum LockScope {
    Exclusive,
    Shared
}

/// 14.14.  locktoken XML Element
///
/// Name:   locktoken
///
/// Purpose:   The lock token associated with a lock.
/// 
/// Description:   The href contains a single lock token URI, which
///    refers to the lock.
///
/// <!ELEMENT locktoken (href) >
#[derive(Debug, PartialEq, Clone)]
pub struct LockToken(pub Href);

/// 14.15.  locktype XML Element
///
/// Name:   locktype
///
/// Purpose:   Specifies the access type of a lock.  At present, this
/// specification only defines one lock type, the write lock.
///
/// <!ELEMENT locktype (write) >
#[derive(Debug, PartialEq, Clone)]
pub enum LockType {
    /// 14.30.  write XML Element
    ///
    /// Name:   write
    ///
    /// Purpose:   Specifies a write lock.
    ///
    ///
    /// <!ELEMENT write EMPTY >
    Write
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
#[derive(Debug, PartialEq, Clone)]
pub struct Multistatus<E: Extension> {
    pub responses: Vec<Response<E>>,
    pub responsedescription: Option<ResponseDescription>,
}

/// 14.17.  owner XML Element
///
/// Name:   owner
///
/// Purpose:   Holds client-supplied information about the creator of a
/// lock.
///
/// Description:   Allows a client to provide information sufficient for
/// either directly contacting a principal (such as a telephone number
/// or Email URI), or for discovering the principal (such as the URL
/// of a homepage) who created a lock.  The value provided MUST be
/// treated as a dead property in terms of XML Information Item
/// preservation.  The server MUST NOT alter the value unless the
/// owner value provided by the client is empty.  For a certain amount
/// of interoperability between different client implementations, if
/// clients have URI-formatted contact information for the lock
/// creator suitable for user display, then clients SHOULD put those
/// URIs in 'href' child elements of the 'owner' element.
///
/// Extensibility:   MAY be extended with child elements, mixed content,
/// text content or attributes.
///
/// <!ELEMENT owner ANY >
//@FIXME might need support for an extension
#[derive(Debug, PartialEq, Clone)]
pub enum Owner {
    Txt(String),
    Href(Href),
    Unknown,
}

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
#[derive(Debug, PartialEq, Clone)]
pub struct PropName<E: Extension>(pub Vec<PropertyRequest<E>>);

#[derive(Debug, PartialEq, Clone)]
pub struct PropValue<E: Extension>(pub Vec<Property<E>>);

#[derive(Debug, PartialEq, Clone)]
pub struct AnyProp<E: Extension>(pub Vec<AnyProperty<E>>);

/// 14.19.  propertyupdate XML Element
///
/// Name:   propertyupdate
///
/// Purpose:   Contains a request to alter the properties on a resource.
///
/// Description:   This XML element is a container for the information
/// required to modify the properties on the resource.
///
/// <!ELEMENT propertyupdate (remove | set)+ >
#[derive(Debug, PartialEq, Clone)]
pub struct PropertyUpdate<E: Extension>(pub Vec<PropertyUpdateItem<E>>);

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyUpdateItem<E: Extension> {
    Remove(Remove<E>),
    Set(Set<E>),
}

/// 14.2 allprop XML Element
///
/// Name:   allprop
///
/// Purpose:   Specifies that all names and values of dead properties and
/// the live properties defined by this document existing on the
/// resource are to be returned.
///
/// <!ELEMENT allprop EMPTY >
///
/// ---
///
/// 14.21.  propname XML Element
///
/// Name:   propname
///
/// Purpose:   Specifies that only a list of property names on the
/// resource is to be returned.
///
/// <!ELEMENT propname EMPTY >
///
/// ---
///
/// 14.20.  propfind XML Element
///
/// Name:   propfind
///
/// Purpose:   Specifies the properties to be returned from a PROPFIND
/// method.  Four special elements are specified for use with
/// 'propfind': 'prop', 'allprop', 'include', and 'propname'.  If
/// 'prop' is used inside 'propfind', it MUST NOT contain property
/// values.
///
/// <!ELEMENT propfind ( propname | (allprop, include?) | prop ) >
#[derive(Debug, PartialEq, Clone)]
pub enum PropFind<E: Extension> {
    PropName,
    AllProp(Option<Include<E>>),
    Prop(PropName<E>),
}

/// 14.22 propstat XML Element
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
///
/// ---
///
///
#[derive(Debug, PartialEq, Clone)]
pub struct PropStat<E: Extension> {
    pub prop: AnyProp<E>,
    pub status: Status,
    pub error: Option<Error<E>>,
    pub responsedescription: Option<ResponseDescription>,
}


/// 14.23.  remove XML Element
///
/// Name:   remove
///
/// Purpose:   Lists the properties to be removed from a resource.
///
/// Description:   Remove instructs that the properties specified in prop
/// should be removed.  Specifying the removal of a property that does
/// not exist is not an error.  All the XML elements in a 'prop' XML
/// element inside of a 'remove' XML element MUST be empty, as only
/// the names of properties to be removed are required.
///
/// <!ELEMENT remove (prop) >
#[derive(Debug, PartialEq, Clone)]
pub struct Remove<E: Extension>(pub PropName<E>);

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
///
/// --- rewritten as ---
/// <!ELEMENT response ((href+, status)|(href, propstat+), error?, responsedescription?, location?>
#[derive(Debug, PartialEq, Clone)]
pub enum StatusOrPropstat<E: Extension> {
    // One status, multiple hrefs...
    Status(Vec<Href>, Status),
    // A single href, multiple properties...
    PropStat(Href, Vec<PropStat<E>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Response<E: Extension> {
    pub status_or_propstat: StatusOrPropstat<E>,
    pub error: Option<Error<E>>,
    pub responsedescription: Option<ResponseDescription>,
    pub location: Option<Location>,
}

/// 14.25.  responsedescription XML Element
///
/// Name:   responsedescription
///
/// Purpose:   Contains information about a status response within a
/// Multi-Status.
///
/// Description:   Provides information suitable to be presented to a
/// user.
///
/// <!ELEMENT responsedescription (#PCDATA) >
#[derive(Debug, PartialEq, Clone)]
pub struct ResponseDescription(pub String);

/// 14.26.  set XML Element
///
/// Name:   set
///
/// Purpose:   Lists the property values to be set for a resource.
///
/// Description:   The 'set' element MUST contain only a 'prop' element.
/// The elements contained by the 'prop' element inside the 'set'
/// element MUST specify the name and value of properties that are set
/// on the resource identified by Request-URI.  If a property already
/// exists, then its value is replaced.  Language tagging information
/// appearing in the scope of the 'prop' element (in the "xml:lang"
/// attribute, if present) MUST be persistently stored along with the
/// property, and MUST be subsequently retrievable using PROPFIND.
///
/// <!ELEMENT set (prop) >
#[derive(Debug, PartialEq, Clone)]
pub struct Set<E: Extension>(pub PropValue<E>);

/// 14.27.  shared XML Element
///
/// Name:   shared
///
/// Purpose:   Specifies a shared lock.
///
///
/// <!ELEMENT shared EMPTY >
#[derive(Debug, PartialEq, Clone)]
pub struct Shared {}


/// 14.28.  status XML Element
/// 
/// Name:   status
///
/// Purpose:   Holds a single HTTP status-line.
///
/// Value:   status-line (defined in Section 6.1 of [RFC2616])
/// 
/// <!ELEMENT status (#PCDATA) >
//@FIXME: Better typing is possible with an enum for example
#[derive(Debug, PartialEq, Clone)]
pub struct Status(pub http::status::StatusCode);

/// 14.29.  timeout XML Element
///
/// Name:   timeout
///
/// Purpose:   The number of seconds remaining before a lock expires.
///
/// Value:   TimeType (defined in Section 10.7)
///
///
/// <!ELEMENT timeout (#PCDATA) >
///
/// TimeOut = "Timeout" ":" 1#TimeType
/// TimeType = ("Second-" DAVTimeOutVal | "Infinite")
///             ; No LWS allowed within TimeType
/// DAVTimeOutVal = 1*DIGIT
///
/// Clients MAY include Timeout request headers in their LOCK requests.
/// However, the server is not required to honor or even consider these
/// requests.  Clients MUST NOT submit a Timeout request header with any
/// method other than a LOCK method.
///
/// The "Second" TimeType specifies the number of seconds that will
/// elapse between granting of the lock at the server, and the automatic
/// removal of the lock.  The timeout value for TimeType "Second" MUST
/// NOT be greater than 2^32-1.
#[derive(Debug, PartialEq, Clone)]
pub enum Timeout {
    Seconds(u32),
    Infinite,
}


/// 15.  DAV Properties
///
/// For DAV properties, the name of the property is also the same as the
/// name of the XML element that contains its value.  In the section
/// below, the final line of each section gives the element type
/// declaration using the format defined in [REC-XML].  The "Value"
/// field, where present, specifies further restrictions on the allowable
/// contents of the XML element using BNF (i.e., to further restrict the
/// values of a PCDATA element).
///
/// A protected property is one that cannot be changed with a PROPPATCH
/// request.  There may be other requests that would result in a change
/// to a protected property (as when a LOCK request affects the value of
/// DAV:lockdiscovery).  Note that a given property could be protected on
/// one type of resource, but not protected on another type of resource.
///
/// A computed property is one with a value defined in terms of a
/// computation (based on the content and other properties of that
/// resource, or even of some other resource).  A computed property is
/// always a protected property.
///
/// COPY and MOVE behavior refers to local COPY and MOVE operations.
///
/// For properties defined based on HTTP GET response headers (DAV:get*),
/// the header value could include LWS as defined in [RFC2616], Section
/// 4.2.  Server implementors SHOULD strip LWS from these values before
/// using as WebDAV property values.
#[derive(Debug, PartialEq, Clone)]
pub enum AnyProperty<E: Extension> {
    Request(PropertyRequest<E>),
    Value(Property<E>),
}

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest<E: Extension> {
    CreationDate,
    DisplayName,
    GetContentLanguage,
    GetContentLength,
    GetContentType,
    GetEtag,
    GetLastModified,
    LockDiscovery,
    ResourceType,
    SupportedLock,
    Extension(E::PropertyRequest),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property<E: Extension> {
    /// 15.1.  creationdate Property
    ///
    /// Name:   creationdate
    ///
    /// Purpose:   Records the time and date the resource was created.
    ///
    /// Value:   date-time (defined in [RFC3339], see the ABNF in Section
    /// 5.6.)
    ///
    /// Protected:   MAY be protected.  Some servers allow DAV:creationdate
    /// to be changed to reflect the time the document was created if that
    /// is more meaningful to the user (rather than the time it was
    /// uploaded).  Thus, clients SHOULD NOT use this property in
    /// synchronization logic (use DAV:getetag instead).
    ///
    /// COPY/MOVE behavior:   This property value SHOULD be kept during a
    /// MOVE operation, but is normally re-initialized when a resource is
    /// created with a COPY.  It should not be set in a COPY.
    ///
    /// Description:   The DAV:creationdate property SHOULD be defined on all
    /// DAV compliant resources.  If present, it contains a timestamp of
    /// the moment when the resource was created.  Servers that are
    /// incapable of persistently recording the creation date SHOULD
    /// instead leave it undefined (i.e. report "Not Found").
    ///
    /// <!ELEMENT creationdate (#PCDATA) >
    CreationDate(DateTime<FixedOffset>),

    /// 15.2.  displayname Property
    /// 
    /// Name:   displayname
    ///
    /// Purpose:   Provides a name for the resource that is suitable for
    /// presentation to a user.
    ///
    /// Value:   Any text.
    ///
    /// Protected:   SHOULD NOT be protected.  Note that servers implementing
    /// [RFC2518] might have made this a protected property as this is a
    /// new requirement.
    ///
    /// COPY/MOVE behavior:   This property value SHOULD be preserved in COPY
    /// and MOVE operations.
    ///
    /// Description:   Contains a description of the resource that is
    /// suitable for presentation to a user.  This property is defined on
    /// the resource, and hence SHOULD have the same value independent of
    /// the Request-URI used to retrieve it (thus, computing this property
    /// based on the Request-URI is deprecated).  While generic clients
    /// might display the property value to end users, client UI designers
    /// must understand that the method for identifying resources is still
    /// the URL.  Changes to DAV:displayname do not issue moves or copies
    /// to the server, but simply change a piece of meta-data on the
    /// individual resource.  Two resources can have the same DAV:
    /// displayname value even within the same collection.
    ///
    /// <!ELEMENT displayname (#PCDATA) >
    DisplayName(String),


    /// 15.3.  getcontentlanguage Property
    ///
    /// Name:   getcontentlanguage
    ///
    /// Purpose:   Contains the Content-Language header value (from Section
    /// 14.12 of [RFC2616]) as it would be returned by a GET without
    /// accept headers.
    ///
    /// Value:   language-tag (language-tag is defined in Section 3.10 of
    /// [RFC2616])
    ///
    /// Protected:   SHOULD NOT be protected, so that clients can reset the
    /// language.  Note that servers implementing [RFC2518] might have
    /// made this a protected property as this is a new requirement.
    ///
    /// COPY/MOVE behavior:   This property value SHOULD be preserved in COPY
    /// and MOVE operations.
    ///
    /// Description:   The DAV:getcontentlanguage property MUST be defined on
    /// any DAV-compliant resource that returns the Content-Language
    /// header on a GET.
    ///
    /// <!ELEMENT getcontentlanguage (#PCDATA) >
    GetContentLanguage(String),

    /// 15.4.  getcontentlength Property
    ///
    /// Name:   getcontentlength
    ///
    /// Purpose:   Contains the Content-Length header returned by a GET
    /// without accept headers.
    ///
    /// Value:   See Section 14.13 of [RFC2616].
    ///
    /// Protected:   This property is computed, therefore protected.
    ///
    /// Description:   The DAV:getcontentlength property MUST be defined on
    /// any DAV-compliant resource that returns the Content-Length header
    /// in response to a GET.
    ///
    /// COPY/MOVE behavior:   This property value is dependent on the size of
    /// the destination resource, not the value of the property on the
    /// source resource.
    ///
    /// <!ELEMENT getcontentlength (#PCDATA) >
    GetContentLength(u64),

    /// 15.5.  getcontenttype Property
    ///
    /// Name:   getcontenttype
    ///
    /// Purpose:   Contains the Content-Type header value (from Section 14.17
    /// of [RFC2616]) as it would be returned by a GET without accept
    /// headers.
    ///
    /// Value:   media-type (defined in Section 3.7 of [RFC2616])
    ///
    /// Protected:   Potentially protected if the server prefers to assign
    /// content types on its own (see also discussion in Section 9.7.1).
    ///
    /// COPY/MOVE behavior:   This property value SHOULD be preserved in COPY
    /// and MOVE operations.
    ///
    /// Description:   This property MUST be defined on any DAV-compliant
    /// resource that returns the Content-Type header in response to a
    /// GET.
    ///
    /// <!ELEMENT getcontenttype (#PCDATA) >
    GetContentType(String),

    /// 15.6.  getetag Property
    ///
    /// Name:   getetag
    ///
    /// Purpose:   Contains the ETag header value (from Section 14.19 of
    /// [RFC2616]) as it would be returned by a GET without accept
    /// headers.
    ///
    /// Value:   entity-tag (defined in Section 3.11 of [RFC2616])
    ///
    /// Protected:  MUST be protected because this value is created and
    /// controlled by the server.
    ///
    /// COPY/MOVE behavior:   This property value is dependent on the final
    /// state of the destination resource, not the value of the property
    /// on the source resource.  Also note the considerations in
    /// Section 8.8.
    ///
    /// Description:   The getetag property MUST be defined on any DAV-
    /// compliant resource that returns the Etag header.  Refer to Section
    /// 3.11 of RFC 2616 for a complete definition of the semantics of an
    /// ETag, and to Section 8.6 for a discussion of ETags in WebDAV.
    ///
    /// <!ELEMENT getetag (#PCDATA) >
    GetEtag(String),

    /// 15.7.  getlastmodified Property
    ///
    /// Name:   getlastmodified
    ///
    /// Purpose:   Contains the Last-Modified header value (from Section
    /// 14.29 of [RFC2616]) as it would be returned by a GET method
    /// without accept headers.
    ///
    /// Value:   rfc1123-date (defined in Section 3.3.1 of [RFC2616])
    ///
    /// Protected:   SHOULD be protected because some clients may rely on the
    /// value for appropriate caching behavior, or on the value of the
    /// Last-Modified header to which this property is linked.
    ///
    /// COPY/MOVE behavior:   This property value is dependent on the last
    /// modified date of the destination resource, not the value of the
    /// property on the source resource.  Note that some server
    /// implementations use the file system date modified value for the
    /// DAV:getlastmodified value, and this can be preserved in a MOVE
    /// even when the HTTP Last-Modified value SHOULD change.  Note that
    /// since [RFC2616] requires clients to use ETags where provided, a
    /// server implementing ETags can count on clients using a much better
    /// mechanism than modification dates for offline synchronization or
    /// cache control.  Also note the considerations in Section 8.8.
    ///
    /// Description:   The last-modified date on a resource SHOULD only
    /// reflect changes in the body (the GET responses) of the resource.
    /// A change in a property only SHOULD NOT cause the last-modified
    /// date to change, because clients MAY rely on the last-modified date
    /// to know when to overwrite the existing body.  The DAV:
    /// getlastmodified property MUST be defined on any DAV-compliant
    /// resource that returns the Last-Modified header in response to a
    /// GET.
    ///
    /// <!ELEMENT getlastmodified (#PCDATA) >
    GetLastModified(DateTime<FixedOffset>),

    /// 15.8.  lockdiscovery Property
    ///
    /// Name:   lockdiscovery
    ///
    /// Purpose:   Describes the active locks on a resource
    ///
    /// Protected:   MUST be protected.  Clients change the list of locks
    /// through LOCK and UNLOCK, not through PROPPATCH.
    ///
    /// COPY/MOVE behavior:   The value of this property depends on the lock
    /// state of the destination, not on the locks of the source resource.
    /// Recall that locks are not moved in a MOVE operation.
    ///
    /// Description:   Returns a listing of who has a lock, what type of lock
    /// he has, the timeout type and the time remaining on the timeout,
    /// and the associated lock token.  Owner information MAY be omitted
    /// if it is considered sensitive.  If there are no locks, but the
    /// server supports locks, the property will be present but contain
    /// zero 'activelock' elements.  If there are one or more locks, an
    /// 'activelock' element appears for each lock on the resource.  This
    /// property is NOT lockable with respect to write locks (Section 7).
    ///
    /// <!ELEMENT lockdiscovery (activelock)* >
    LockDiscovery(Vec<ActiveLock>),

    
    /// 15.9.  resourcetype Property
    ///
    /// Name:   resourcetype
    ///
    /// Purpose:   Specifies the nature of the resource.
    ///
    /// Protected:   SHOULD be protected.  Resource type is generally decided
    /// through the operation creating the resource (MKCOL vs PUT), not by
    /// PROPPATCH.
    ///
    /// COPY/MOVE behavior:   Generally a COPY/MOVE of a resource results in
    /// the same type of resource at the destination.
    ///
    /// Description:   MUST be defined on all DAV-compliant resources.  Each
    /// child element identifies a specific type the resource belongs to,
    /// such as 'collection', which is the only resource type defined by
    /// this specification (see Section 14.3).  If the element contains
    /// the 'collection' child element plus additional unrecognized
    /// elements, it should generally be treated as a collection.  If the
    /// element contains no recognized child elements, it should be
    /// treated as a non-collection resource.  The default value is empty.
    /// This element MUST NOT contain text or mixed content.  Any custom
    /// child element is considered to be an identifier for a resource
    /// type.
    ///
    /// Example: (fictional example to show extensibility)
    /// 
    ///   <x:resourcetype xmlns:x="DAV:">
    ///       <x:collection/>
    ///       <f:search-results xmlns:f="http://www.example.com/ns"/>
    ///   </x:resourcetype>
    ResourceType(Vec<ResourceType<E>>),

    /// 15.10.  supportedlock Property
    ///
    /// Name:   supportedlock
    ///
    /// Purpose:   To provide a listing of the lock capabilities supported by
    /// the resource.
    ///
    /// Protected:   MUST be protected.  Servers, not clients, determine what
    /// lock mechanisms are supported.
    /// COPY/MOVE behavior:   This property value is dependent on the kind of
    /// locks supported at the destination, not on the value of the
    /// property at the source resource.  Servers attempting to COPY to a
    /// destination should not attempt to set this property at the
    /// destination.
    ///
    /// Description:   Returns a listing of the combinations of scope and
    /// access types that may be specified in a lock request on the
    /// resource.  Note that the actual contents are themselves controlled
    /// by access controls, so a server is not required to provide
    /// information the client is not authorized to see.  This property is
    /// NOT lockable with respect to write locks (Section 7).
    ///
    /// <!ELEMENT supportedlock (lockentry)* >
    SupportedLock(Vec<LockEntry>),

    /// Any extension
    Extension(E::Property),
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType<E: Extension> {
    Collection,
    Extension(E::ResourceType),
}
