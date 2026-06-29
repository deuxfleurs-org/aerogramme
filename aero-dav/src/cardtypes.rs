use super::coretypes as dav;

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
}

// -------- Inner XML elements ---------

/// <!ELEMENT address-data-type EMPTY>
/// <!ATTLIST address-data-type content-type CDATA "text/vcard"
///                       version CDATA "3.0">
/// <!-- content-type value: a MIME media type -->
/// <!-- version value: a version string -->
#[derive(Debug, PartialEq, Clone)]
pub struct AddressDataType {
    pub content_type: String,
    pub version: String,
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
