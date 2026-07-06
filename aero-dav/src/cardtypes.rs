use super::extension::Extension;
use super::coretypes as dav;
use super::xml::WithDefault;

/**
 * # CardDAV extension to WebDAV (RFC6352)
 *
 * Allow addressbook synchronization, editing, viewing across devices.
 *
 * We aim to fully implement the specification here.
 *
 * ## References
 *
 * Official RFC
 * https://datatracker.ietf.org/doc/html/rfc6352
 */

// --- (REPORT PART) ---
#[derive(Debug, PartialEq, Clone)]
pub enum ReportTypeName {
    Query,
    Multiget,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ReportType<E: Extension> {
    Query(AddressbookQuery<E>),
    Multiget(AddressbookMultiget<E>),
}

// ----- Hooks -----
#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Addressbook,
}

/// Check the matching Property object for documentation
#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    AddressbookDescription,
    SupportedAddressData,
    MaxResourceSize,
    AddressbookHomeSet,
    PrincipalAddress,
    SupportedCollationSet,
    AddressData(AddressDataRequest),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property {
    /// Name:  addressbook-description
    ///
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    ///
    /// Purpose:  Provides a human-readable description of the address book
    ///    collection.
    ///
    /// Value:  Any text.
    /// 
    /// Protected:  SHOULD NOT be protected so that users can specify a
    ///    description.
    /// 
    /// COPY/MOVE behavior:  This property value SHOULD be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  This property contains a description of the address
    ///    book collection that is suitable for presentation to a user.  The
    ///    xml:lang attribute can be used to add a language tag for the value
    ///    of this property.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT addressbook-description (#PCDATA)>
    /// <!-- PCDATA value: string -->
    /// 
    /// Example:
    /// 
    /// <C:addressbook-description xml:lang="fr-CA"
    ///    xmlns:C="urn:ietf:params:xml:ns:carddav"
    /// >Adresses de Oliver Daboo</C:addressbook-description>
    AddressbookDescription { lang: Option<String>, text: String },

    /// Name:  supported-address-data
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    /// 
    /// Purpose:  Specifies what media types are allowed for address object
    ///    resources in an address book collection.
    /// 
    /// Protected:  MUST be protected as it indicates the level of support
    ///    provided by the server.
    /// 
    /// COPY/MOVE behavior:  This property value MUST be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  The CARDDAV:supported-address-data property is used to
    ///    specify the media type supported for the address object resources
    ///    contained in a given address book collection (e.g., vCard version
    ///        3.0).  Any attempt by the client to store address object resources
    ///    with a media type not listed in this property MUST result in an
    ///    error, with the CARDDAV:supported-address-data precondition
    ///    (Section 6.3.2.1) being violated.  In the absence of this
    ///    property, the server MUST only accept data with the media type
    ///    "text/vcard" and vCard version 3.0, and clients can assume that is
    ///    all the server will accept.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT supported-address-data (address-data-type+)>
    /// 
    /// <!ELEMENT address-data-type EMPTY>
    /// <!ATTLIST address-data-type content-type CDATA "text/vcard"
    ///                       version CDATA "3.0">
    /// <!-- content-type value: a MIME media type -->
    /// <!-- version value: a version string -->
    /// 
    /// Example:
    /// 
    /// <C:supported-address-data
    ///    xmlns:C="urn:ietf:params:xml:ns:carddav">
    ///   <C:address-data-type content-type="text/vcard" version="3.0"/>
    /// </C:supported-address-data>
    SupportedAddressData(Vec<AddressDataType>),

    /// Name:  max-resource-size
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    /// 
    /// Purpose:  Provides a numeric value indicating the maximum size in
    ///    octets of a resource that the server is willing to accept when an
    ///    address object resource is stored in an address book collection.
    /// 
    /// Value:  Any text representing a numeric value.
    /// 
    /// Protected:  MUST be protected as it indicates limits provided by the
    ///    server.
    /// 
    /// COPY/MOVE behavior:  This property value MUST be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  The CARDDAV:max-resource-size is used to specify a
    ///    numeric value that represents the maximum size in octets that the
    ///    server is willing to accept when an address object resource is
    ///    stored in an address book collection.  Any attempt to store an
    ///    address book object resource exceeding this size MUST result in an
    ///    error, with the CARDDAV:max-resource-size precondition
    ///    (Section 6.3.2.1) being violated.  In the absence of this
    ///    property, the client can assume that the server will allow storing
    ///    a resource of any reasonable size.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT max-resource-size (#PCDATA)>
    /// <!-- PCDATA value: a numeric value (positive decimal integer) -->
    /// 
    /// Example:
    /// 
    /// <C:max-resource-size xmlns:C="urn:ietf:params:xml:ns:carddav"
    /// >102400</C:max-resource-size>
    MaxResourceSize(u64),

    /// Name:  addressbook-home-set
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    /// 
    /// Purpose:  Identifies the URL of any WebDAV collections that contain
    ///    address book collections owned by the associated principal
    ///    resource.
    /// 
    /// Protected:  MAY be protected if the server has fixed locations in
    ///    which address books are created.
    /// 
    /// COPY/MOVE behavior:  This property value MUST be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  The CARDDAV:addressbook-home-set property is meant to
    ///    allow users to easily find the address book collections owned by
    ///    the principal.  Typically, users will group all the address book
    ///    collections that they own under a common collection.  This
    ///    property specifies the URL of collections that are either address
    ///    book collections or ordinary collections that have child or
    ///    descendant address book collections owned by the principal.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT addressbook-home-set (DAV:href*)>
    /// 
    /// Example:
    /// 
    /// <C:addressbook-home-set xmlns:D="DAV:"
    ///    xmlns:C="urn:ietf:params:xml:ns:carddav">
    ///   <D:href>/bernard/addresses/</D:href>
    /// </C:addressbook-home-set>
    AddressbookHomeSet(dav::Href),

    /// Name:  principal-address
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    /// 
    /// Purpose:  Identifies the URL of an address object resource that
    ///    corresponds to the user represented by the principal.
    /// 
    /// Protected:  MAY be protected if the server provides a fixed location
    ///    for principal addresses.
    /// 
    /// COPY/MOVE behavior:  This property value MUST be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  The CARDDAV:principal-address property is meant to
    ///    allow users to easily find contact information for users
    ///    represented by principals on the system.  This property specifies
    ///    the URL of the resource containing the corresponding contact
    ///    information.  The resource could be an address object resource in
    ///    an address book collection, or it could be a resource in a
    ///    "regular" collection.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT principal-address (DAV:href)>
    /// 
    /// Example:
    /// 
    /// <C:principal-address xmlns:D="DAV:"
    ///    xmlns:C="urn:ietf:params:xml:ns:carddav">
    ///    <D:href>/system/cyrus.vcf</D:href>
    /// </C:principal-address>
    PrincipalAddress(dav::Href),

    /// Name:  supported-collation-set
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:carddav
    /// 
    /// Purpose:  Identifies the set of collations supported by the server
    ///    for text matching operations.
    /// 
    /// Protected:  MUST be protected as it indicates support provided by the
    ///    server.
    /// 
    /// COPY/MOVE behavior:  This property value MUST be preserved in COPY
    ///    and MOVE operations.
    /// 
    /// allprop behavior:  SHOULD NOT be returned by a PROPFIND DAV:allprop
    ///    request.
    /// 
    /// Description:  The CARDDAV:supported-collation-set property contains
    ///    two or more CARDDAV:supported-collation elements that specify the
    ///    identifiers of the collations supported by the server.
    /// 
    /// Definition:
    /// 
    /// <!ELEMENT supported-collation-set (
    ///       supported-collation
    ///       supported-collation
    ///       supported-collation*)>
    /// <!-- Both "i;ascii-casemap" and "i;unicode-casemap"
    ///      will be present -->
    /// 
    /// <!ELEMENT supported-collation (#PCDATA)>
    /// 
    /// Example:
    /// 
    /// <C:supported-collation-set
    ///   xmlns:C="urn:ietf:params:xml:ns:carddav">
    ///   <C:supported-collation>i;ascii-casemap</C:supported-collation>
    ///   <C:supported-collation>i;octet</C:supported-collation>
    ///   <C:supported-collation>i;unicode-casemap</C:supported-collation>
    /// </C:supported-collation-set>
    SupportedCollationSet(Vec<SupportedCollation>),

    /// See AddressDataPayload
    AddressData(AddressDataPayload),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Violation {
    /// (CARDDAV:supported-address-data-conversion): The resource targeted
    /// by the GET request can be converted to the media type specified in
    /// the Accept request header included with the request.
    SupportedAddressDataConversion,

    /// (CARDDAV:supported-address-data): The resource submitted in the
    /// PUT request, or targeted by a COPY or MOVE request, MUST be a
    /// supported media type (i.e., vCard) for address object resources.
    SupportedAddressData,

    /// (CARDDAV:valid-address-data): The resource submitted in the PUT
    /// request, or targeted by a COPY or MOVE request, MUST be valid data
    /// for the media type being specified (i.e., MUST contain valid vCard
    /// data).
    ValidAddressData,

    /// (CARDDAV:no-uid-conflict): The resource submitted in the PUT
    /// request, or targeted by a COPY or MOVE request, MUST NOT specify a
    /// vCard UID property value already in use in the targeted address
    /// book collection or overwrite an existing address object resource
    /// with one that has a different UID property value.  Servers SHOULD
    /// report the URL of the resource that is already making use of the
    /// same UID property value in the DAV:href element.
    ///
    /// <!ELEMENT no-uid-conflict (DAV:href)>
    NoUidConflict(dav::Href),

    /// (CARDDAV:addressbook-collection-location-ok): In a COPY or MOVE
    /// request, when the Request-URI is an address book collection, the
    /// URI targeted by the Destination HTTP Request header MUST identify
    /// a location where an address book collection can be created.
    AddressbookCollectionLocationOk,

    /// (CARDDAV:max-resource-size): The resource submitted in the PUT
    /// request, or targeted by a COPY or MOVE request, MUST have a size
    /// in octets less than or equal to the value of the
    /// CARDDAV:max-resource-size property value (Section 6.2.3) on the
    /// address book collection where the resource will be stored.
    MaxResourceSize,

    /// If the client chooses a collation not supported by the server, the
    /// server MUST respond with a CARDDAV:supported-collation precondition
    /// error response.
    SupportedCollation,

    /// Servers MUST fail with the CARDDAV:supported-filter precondition if
    /// an address book REPORT request uses a CARDDAV:prop-filter or
    /// CARDDAV:param-filter XML element that makes reference to a non-
    /// standard vCard property or parameter name on which the server does
    /// not support queries.
    /// ------
    /// (CARDDAV:supported-filter): The CARDDAV:prop-filter (see
    /// Section 10.5.1) and CARDDAV:param-filter (see Section 10.5.2) XML
    /// elements used in the CARDDAV:filter XML element (see Section 10.5)
    /// in the REPORT request only make reference to vCard properties and
    /// parameters for which queries are supported by the server.  That
    /// is, if the CARDDAV:filter element attempts to reference an
    /// unsupported vCard property or parameter, this precondition is
    /// violated.  A server SHOULD report the CARDDAV:prop-filter or
    /// CARDDAV:param-filter for which it does not provide support.
    /// 
    /// <!ELEMENT supported-filter (prop-filter*,
    ///                             param-filter*)>
    SupportedFilter {
        prop_filters: Vec<PropFilter>,
        param_filters: Vec<ParamFilter>,
    },

    ///@FIXME should not be here but in RFC3744
    /// (DAV:number-of-matches-within-limits): The number of matching
    /// principals must fall within server-specific, predefined limits.
    /// For example, this condition might be triggered if a search
    /// specification would cause the return of an extremely large number
    /// of responses.
    NumberOfMatchesWithinLimits,
}

/// Name:  addressbook-query
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Defines a report for querying address book data
///
/// Description:  See Section 8.6.
///
/// Definition:
///
/// <!ELEMENT addressbook-query ((DAV:allprop |
///                               DAV:propname |
///                               DAV:prop)?, filter, limit?)>
#[derive(Debug, PartialEq, Clone)]
pub struct AddressbookQuery<E: Extension> {
    pub selector: Option<AddressbookSelector<E>>,
    pub filter: Filter,
    pub limit: Option<Limit>,
}

/// Name:  address-data
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:
///
///    1.  The parts of an address object resource that should be
///        returned by a given address book REPORT request, and the media
///        type and version for the returned data; or
///
/// Description:  When used in an address book REPORT request, the
///    CARDDAV:address-data XML element specifies which parts of address
///    object resources need to be returned in the response.  If the
///    CARDDAV:address-data XML element doesn't contain any CARDDAV:prop
///    elements, address object resources will be returned in their
///    entirety.  Additionally, a media type and version can be specified
///    to request that the server return the data in that format if
///    possible.
///
/// Note:  The CARDDAV:address-data XML element is specified in requests
///    and responses inside the DAV:prop XML element as if it were a
///    WebDAV property.  However, the CARDDAV:address-data XML element is
///    not a WebDAV property and as such it is not returned in PROPFIND
///    responses nor used in PROPPATCH requests.
///
/// Definition:
///
/// <!ELEMENT address-data (allprop | prop*)>
///
/// when nested in the DAV:prop XML element in an address book
/// REPORT request to specify which parts of address object
/// resources should be returned in the response;
///
/// <!ATTLIST address-data content-type CDATA "text/vcard"
///                        version CDATA "3.0">
/// <!-- content-type value: a MIME media type -->
/// <!-- version value: a version string -->
#[derive(Debug, PartialEq, Clone)]
pub struct AddressDataRequest {
    pub prop_kind: Option<PropKind>,
    pub content_type: WithDefault<ContentType>,
    pub version: WithDefault<Version>,
}

/// Name:  address-data
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:
///
///    2.  The content of an address object resource in a response to an
///        address book REPORT request.
///
/// Description:
///
///    When used in an address book REPORT response, the
///    CARDDAV:address-data XML element specifies the content of an
///    address object resource.  Given that XML parsers normalize the
///    two-character sequence CRLF (US-ASCII decimal 13 and US-ASCII
///    decimal 10) to a single LF character (US-ASCII decimal 10), the CR
///    character (US-ASCII decimal 13) MAY be omitted in address object
///    resources specified in the CARDDAV:address-data XML element.
///    Furthermore, address object resources specified in the
///    CARDDAV:address-data XML element MAY be invalid per their media
///    type specification if the CARDDAV:address-data XML element part of
///    the address book REPORT request did not specify required vCard
///    properties (e.g., UID, etc.) or specified a CARDDAV:prop XML
///    element with the "novalue" attribute set to "yes".
///
/// Note:  The CARDDAV:address-data XML element is specified in requests
///    and responses inside the DAV:prop XML element as if it were a
///    WebDAV property.  However, the CARDDAV:address-data XML element is
///    not a WebDAV property and as such it is not returned in PROPFIND
///    responses nor used in PROPPATCH requests.
///
/// Note:  The address data embedded within the CARDDAV:address-data XML
///    element MUST follow the standard XML character data encoding
///    rules, including use of &lt;, &gt;, &amp; etc., entity encoding or
///    the use of a <![CDATA[ ... ]]> construct.  In the latter case, the
///    vCard data cannot contain the character sequence "]]>", which is
///    the end delimiter for the CDATA section.
///
/// Definition:
///
/// <!ELEMENT address-data (#PCDATA)>
/// <!-- PCDATA value: address data -->
///
/// when nested in the DAV:prop XML element in an address book
/// REPORT response to specify the content of a returned
/// address object resource.
///
/// <!ATTLIST address-data content-type CDATA "text/vcard"
///                       version CDATA "3.0">
/// <!-- content-type value: a MIME media type -->
/// <!-- version value: a version string -->
#[derive(Debug, PartialEq, Clone)]
pub struct AddressDataPayload {
    pub payload: String,
    pub content_type: WithDefault<ContentType>,
    pub version: WithDefault<Version>,
}

/// Name:  addressbook-multiget
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  CardDAV report used to retrieve specific address objects
///    via their URIs.
///
/// Description:  See Section 8.7.
///
/// Definition:
///
/// <!ELEMENT addressbook-multiget ((DAV:allprop |
///                                  DAV:propname |
///                                  DAV:prop)?,
///                                  DAV:href+)>
#[derive(Debug, PartialEq, Clone)]
pub struct AddressbookMultiget<E: Extension> {
    pub selector: Option<AddressbookSelector<E>>,
    pub href: Vec<dav::Href>,
}

// -------- Inner XML elements ---------

/// <!ELEMENT address-data-type EMPTY>
/// <!ATTLIST address-data-type content-type CDATA "text/vcard"
///                       version CDATA "3.0">
/// <!-- content-type value: a MIME media type -->
/// <!-- version value: a version string -->
#[derive(Debug, PartialEq, Clone)]
pub struct AddressDataType {
    pub content_type: WithDefault<ContentType>,
    pub version: WithDefault<Version>,
}

/// Some of the reports defined in this section do text matches of
/// character strings provided by the client and compared to stored
/// address data.  Since vCard data is by default encoded in the UTF-8
/// charset and may include characters outside of the US-ASCII charset
/// range in some property and parameter values, there is a need to
/// ensure that text matching follows well-defined rules.
///
/// To deal with this, this specification makes use of the IANA Collation
/// Registry defined in [RFC4790] to specify collations that may be used
/// to carry out the text comparison operations with a well-defined rule.
///
/// Collations supported by the server MUST support "equality" and
/// "substring" match operations as per [RFC4790], Section 4.2, including
/// the "prefix" and "suffix" options for "substring" matching.  CardDAV
/// uses these match options for "equals", "contains", "starts-with", and
/// "ends-with" match operations.
///
/// CardDAV servers are REQUIRED to support the "i;ascii-casemap"
/// [RFC4790] and "i;unicode-casemap" [RFC5051] collations and MAY
/// support other collations.
///
/// Servers MUST advertise the set of collations that they support via
/// the CARDDAV:supported-collation-set property defined on any resource
/// that supports reports that use collations.
///
/// In the absence of a collation explicitly specified by the client, or
/// if the client specifies the "default" collation identifier (as
/// defined in [RFC4790], Section 3.1), the server MUST default to using
/// "i;unicode-casemap" as the collation.
///
/// Wildcards (as defined in [RFC4790], Section 3.2) MUST NOT be used in
/// the collation identifier.
///
/// If the client chooses a collation not supported by the server, the
/// server MUST respond with a CARDDAV:supported-collation precondition
/// error response.
#[derive(Debug, PartialEq, Clone)]
pub struct SupportedCollation(pub Collation);

#[derive(Default, Debug, PartialEq, Clone)]
pub enum Collation {
    #[default]
    UnicodeCaseMap,
    AsciiCaseMap,
    Unknown(String),
}
impl Collation {
    pub fn as_str<'a>(&'a self) -> &'a str {
        match self {
            Self::UnicodeCaseMap => "i;unicode-casemap",
            Self::AsciiCaseMap => "i;ascii-casemap",
            Self::Unknown(c) => c.as_str(),
        }
    }
    pub fn new(v: String) -> Self {
        match v.as_str() {
            "i;unicode-casemap" => Self::UnicodeCaseMap,
            "i;ascii-casemap" => Self::AsciiCaseMap,
            _ => Self::Unknown(v),
        }
    }
}

/// Name:  filter
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Determines which matching objects are returned.
///
/// Description:  The "filter" element specifies the search filter used
///    to match address objects that should be returned by a report.  The
///    "test" attribute specifies whether any (logical OR) or all
///    (logical AND) of the prop-filter tests need to match in order for
///    the overall filter to match.
///
/// Definition:
///
/// <!ELEMENT filter (prop-filter*)>
///
/// <!ATTLIST filter test (anyof | allof) "anyof">
/// <!-- test value:
///           anyof logical OR for prop-filter matches
///           allof logical AND for prop-filter matches -->
#[derive(Debug, PartialEq, Clone)]
pub struct Filter {
    pub prop_filters: Vec<PropFilter>,
    pub test: WithDefault<FilterTest>,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum FilterTest {
    #[default]
    AnyOf,
    AllOf,
}
impl FilterTest {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AnyOf => "anyof",
            Self::AllOf => "allof",
        }
    }
}
impl std::str::FromStr for FilterTest {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "anyof" => Ok(Self::AnyOf),
            "allof" => Ok(Self::AllOf),
            _ => Err(()),
        }
    }
}

/// Name:  prop-filter
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Limits the search to specific vCard properties.
///
/// Description:  The CARDDAV:prop-filter XML element specifies search
///    criteria on a specific vCard property (e.g., "NICKNAME").  An
///    address object is said to match a CARDDAV:prop-filter if:
///
///    *  A vCard property of the type specified by the "name" attribute
///       exists, and the CARDDAV:prop-filter is empty, or it matches any
///       specified CARDDAV:text-match or CARDDAV:param-filter
///       conditions.  The "test" attribute specifies whether any
///       (logical OR) or all (logical AND) of the text-filter and param-
///       filter tests need to match in order for the overall filter to
///       match.
///
///    or:
///
///    *  A vCard property of the type specified by the "name" attribute
///       does not exist, and the CARDDAV:is-not-defined element is
///       specified.
///
///    vCard allows a "group" prefix to appear before a property name in
///    the vCard data.  When the "name" attribute does not specify a
///    group prefix, it MUST match properties in the vCard data without a
///    group prefix or with any group prefix.  When the "name" attribute
///    includes a group prefix, it MUST match properties that have
///    exactly the same group prefix and name.  For example, a "name" set
///    to "TEL" will match "TEL", "X-ABC.TEL", "X-ABC-1.TEL" vCard
///    properties.  A "name" set to "X-ABC.TEL" will match an "X-ABC.TEL"
///    vCard property only, it will not match "TEL" or "X-ABC-1.TEL".
///
/// Definition:
///
/// <!ELEMENT prop-filter (is-not-defined |
///                        (text-match*, param-filter*))>
///
/// <!ATTLIST prop-filter name CDATA #REQUIRED
///                       test (anyof | allof) "anyof">
/// <!-- name value: a vCard property name (e.g., "NICKNAME")
///   test value:
///       anyof logical OR for text-match/param-filter matches
///       allof logical AND for text-match/param-filter matches -->
#[derive(Debug, PartialEq, Clone)]
pub struct PropFilter {
    pub name: PropertyName,
    // XXX "test" is only relevant when rules is PropFilterRules::Match
    pub test: WithDefault<FilterTest>,
    pub rules: PropFilterRules,
}
#[derive(Debug, PartialEq, Clone)]
pub enum PropFilterRules {
    // "CARDDAV:prop-filter is empty"
    Empty,
    // last case, "CARDDAV:is-not-defined element is specified"
    IsNotDefined,
    Match {
        text_match: Vec<TextMatch>,
        param_filter: Vec<ParamFilter>,
    }
}

/// Name:  text-match
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Specifies a substring match on a vCard property or
///    parameter value.
///
/// Description:  The CARDDAV:text-match XML element specifies text used
///    for a substring match against the vCard property or parameter
///    value specified in an address book REPORT request.
///
///    The "collation" attribute is used to select the collation that the
///    server MUST use for character string matching.  In the absence of
///    this attribute, the server MUST use the "i;unicode-casemap"
///    collation.
///
///    The "negate-condition" attribute is used to indicate that this
///    test returns a match if the text matches, when the attribute value
///    is set to "no", or return a match if the text does not match, if
///    the attribute value is set to "yes".  For example, this can be
///    used to match components with a CATEGORIES property not set to
///    PERSON.
///
///    The "match-type" attribute is used to indicate the type of match
///    operation to use.  Possible choices are:
///
///    -  "equals" - an exact match to the target string
///
///    -  "contains" - a substring match, matching anywhere within the
///       target string
///
///    -  "starts-with" - a substring match, matching only at the start
///       of the target string
///
///    -  "ends-with" - a substring match, matching only at the end of
///       the target string
///
/// Definition:
///
/// <!ELEMENT text-match (#PCDATA)>
/// <!-- PCDATA value: string -->
///
/// <!ATTLIST text-match
///    collation        CDATA "i;unicode-casemap"
///    negate-condition (yes | no) "no"
///    match-type (equals|contains|starts-with|ends-with) "contains">
#[derive(Debug, PartialEq, Clone)]
pub struct TextMatch {
    pub collation: WithDefault<Collation>,
    pub negate_condition: WithDefault<NegateCondition>,
    pub match_type: WithDefault<TextMatchType>,
    pub text: String,
}
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum TextMatchType {
    Equals,
    #[default]
    Contains,
    StartsWith,
    EndsWith,
}
impl TextMatchType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::Contains => "contains",
            Self::StartsWith => "starts-with",
            Self::EndsWith => "ends-with",
        }
    }
}
impl std::str::FromStr for TextMatchType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "equals" => Ok(Self::Equals),
            "contains" => Ok(Self::Contains),
            "starts-with" => Ok(Self::StartsWith),
            "ends-with" => Ok(Self::EndsWith),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Default)]
pub enum NegateCondition {
    Yes,
    #[default]
    No,
}
impl NegateCondition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}
impl std::str::FromStr for NegateCondition {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "yes" => Ok(Self::Yes),
            "no" => Ok(Self::No),
            _ => Err(()),
        }
    }
}

/// Name:  param-filter
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Limits the search to specific parameter values.
///
/// Description:  The CARDDAV:param-filter XML element specifies search
///    criteria on a specific vCard property parameter (e.g., TYPE) in
///    the scope of a given CARDDAV:prop-filter.  A vCard property is
///    said to match a CARDDAV:param-filter if:
///
///    *  A parameter of the type specified by the "name" attribute
///       exists, and the CARDDAV:param-filter is empty, or it matches
///       the CARDDAV:text-match conditions if specified.
///
///    or:
///
///    *  A parameter of the type specified by the "name" attribute does
///       not exist, and the CARDDAV:is-not-defined element is specified.
///
/// Definition:
///
/// <!ELEMENT param-filter (is-not-defined | text-match)?>
///
/// <!ATTLIST param-filter name CDATA #REQUIRED>
/// <!-- name value: a property parameter name (e.g., "TYPE") -->
#[derive(Debug, PartialEq, Clone)]
pub struct ParamFilter {
    pub name: PropertyParameterName,
    pub rules: Option<ParamFilterMatch>,
}
#[derive(Debug, PartialEq, Clone)]
pub enum ParamFilterMatch {
    IsNotDefined,
    Match(TextMatch),
}

/// Name:  is-not-defined
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Specifies that a match should occur if the enclosing vCard
///    property or parameter does not exist.
///
/// Description:  The CARDDAV:is-not-defined XML element specifies that a
///    match occurs if the enclosing vCard property or parameter value
///    specified in an address book REPORT request does not exist in the
///    address data being tested.
///
/// Definition:
///
/// <!ELEMENT is-not-defined EMPTY>
/* CURRENTLY INLINED */

/// Name:  limit
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Specifies different types of limits that can be applied to
///    the results returned by the server.
///
/// Description:  The CARDDAV:limit XML element can be used to specify
///    different types of limits that the client can request the server
///    to apply to the results returned by the server.  Currently, only
///    the CARDDAV:nresults limit can be used; other types of limit could
///    be defined in the future.
///
/// Definition:
///
/// <!ELEMENT limit (nresults)>
///
/// ------
/// Name:  nresults
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Specifies a limit on the number of results returned by the
///    server.
///
/// Description:  The CARDDAV:nresults XML element contains a requested
///    maximum number of DAV:response elements to be returned in the
///    response body of a query.  The server MAY disregard this limit.
///    The value of this element is an unsigned integer.
///
/// Definition:
///
/// <!ELEMENT nresults (#PCDATA)>
/// <!-- nresults value: unsigned integer, must be digits -->
#[derive(Debug, PartialEq, Clone)]
pub struct Limit {
    pub nresults: u64,
}

/// Used by AddressbookQuery & AddressbookMultiget
#[derive(Debug, PartialEq, Clone)]
pub enum AddressbookSelector<E: Extension> {
    AllProp,
    PropName,
    Prop(dav::PropName<E>),
}

/// Name:  allprop
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Specifies that all vCard properties shall be returned.
///
/// Description:  This element can be used when the client wants all
///    vCard properties of components returned by a report.
///
/// Definition:
///
/// <!ELEMENT allprop EMPTY>
///
/// Note: The CARDDAV:allprop element defined here has the same name as
/// the DAV:allprop element defined in WebDAV.  However, the
/// CARDDAV:allprop element defined here uses the
/// "urn:ietf:params:xml:ns:carddav" namespace, as opposed to the "DAV:"
/// namespace used for the DAV:allprop element defined in WebDAV.
// FIXME PropKind::Prop only represents non-empty vecs; callers must use
// Option<PropKind> to match the RFC's "allprop | prop*".
// (and same with caldav's PropKind)
#[derive(Debug, PartialEq, Clone)]
pub enum PropKind {
    AllProp,
    Prop(Vec<CardProp>),
}

/// Name:  prop
///
/// Namespace:  urn:ietf:params:xml:ns:carddav
///
/// Purpose:  Defines which vCard properties to return in the response.
///
/// Description:  The "name" attribute specifies the name of the vCard
///    property to return (e.g., "NICKNAME").  The "novalue" attribute
///    can be used by clients to request that the actual value of the
///    property not be returned (if the "novalue" attribute is set to
///    "yes").  In that case, the server will return just the vCard
///    property name and any vCard parameters and a trailing ":" without
///    the subsequent value data.
///
///    vCard allows a "group" prefix to appear before a property name in
///    the vCard data.  When the "name" attribute does not specify a
///    group prefix, it MUST match properties in the vCard data without a
///    group prefix or with any group prefix.  When the "name" attribute
///    includes a group prefix, it MUST match properties that have
///    exactly the same group prefix and name.  For example, a "name" set
///    to "TEL" will match "TEL", "X-ABC.TEL", and "X-ABC-1.TEL" vCard
///    properties.  A "name" set to "X-ABC.TEL" will match an "X-ABC.TEL"
///    vCard property only; it will not match "TEL" or "X-ABC-1.TEL".
///
/// Definition:
///
/// <!ELEMENT prop EMPTY>
///
/// <!ATTLIST prop name CDATA #REQUIRED
///            novalue (yes | no) "no">
/// <!-- name value: a vCard property name -->
/// <!-- novalue value: "yes" or "no" -->
///
/// Note: The CARDDAV:prop element defined here has the same name as the
/// DAV:prop element defined in WebDAV.  However, the CARDDAV:prop
/// element defined here uses the "urn:ietf:params:xml:ns:carddav"
/// namespace, as opposed to the "DAV:" namespace used for the DAV:prop
/// element defined in WebDAV.
#[derive(Debug, PartialEq, Clone)]
pub struct CardProp {
    pub name: PropertyName,
    pub novalue: WithDefault<NoValue>,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub enum NoValue {
    Yes,
    #[default]
    No,
}
impl NoValue {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::No => "no",
        }
    }
}
impl std::str::FromStr for NoValue {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "yes" => Ok(Self::Yes),
            "no" => Ok(Self::No),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ContentType(pub String);
impl Default for ContentType {
    fn default() -> Self {
        Self("text/vcard".to_string())
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Version(pub String);
impl Default for Version {
    fn default() -> Self {
        Self("3.0".to_string())
    }
}

/// A property parameter name (e.g. "TYPE")
#[derive(Debug, PartialEq, Clone)]
pub struct PropertyParameterName(pub String);

/// A vCard property name (e.g. "NICKNAME").
/// Can also include a "group" prefix, e.g. "X-ABC.NICKNAME".
#[derive(Debug, PartialEq, Clone)]
pub struct PropertyName {
    pub group: Option<String>, 
    pub name: String,
}

impl std::str::FromStr for PropertyName {
    type Err = ();
    // FIXME this only splits on '.' and does not try to validate
    // that the input is using proper property name syntax.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(".") {
            None => Ok(PropertyName { group: None, name: s.to_string() }),
            Some((group, name)) => Ok(PropertyName {
                group: Some(group.to_string()),
                name: name.to_string(),
            })
        }
    }
}
impl std::fmt::Display for PropertyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.group {
            None => write!(f, "{}", self.name),
            Some(g) => write!(f, "{}.{}", g, self.name),
        }
    }
}
