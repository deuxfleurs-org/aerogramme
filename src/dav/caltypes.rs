#![allow(dead_code)]

/*
use chrono::{DateTime,Utc};
use super::types as dav;

//@FIXME ACL (rfc3744) is missing, required
//@FIXME Versioning (rfc3253) is missing, required
//@FIXME WebDAV sync (rfc6578) is missing, optional
// For reference, SabreDAV guide gives high-level & real-world overview:
// https://sabre.io/dav/building-a-caldav-client/
// For reference, non-official extensions documented by SabreDAV:
// https://github.com/apple/ccs-calendarserver/tree/master/doc/Extensions


// ----- Root elements -----

// --- (MKCALENDAR PART) ---

/// If a request body is included, it MUST be a CALDAV:mkcalendar XML
/// element.  Instruction processing MUST occur in the order
/// instructions are received (i.e., from top to bottom).
/// Instructions MUST either all be executed or none executed.  Thus,
/// if any error occurs during processing, all executed instructions
/// MUST be undone and a proper error result returned.  Instruction
/// processing details can be found in the definition of the DAV:set
/// instruction in Section 12.13.2 of [RFC2518].
///
/// <!ELEMENT mkcalendar (DAV:set)>
pub struct MkCalendar<E: dav::Extension>(pub dav::Set<E>);


/// If a response body for a successful request is included, it MUST
/// be a CALDAV:mkcalendar-response XML element.
///
/// <!ELEMENT mkcalendar-response ANY>
///
/// ----
/// 
/// ANY is not satisfying, so looking at RFC5689
/// https://www.rfc-editor.org/rfc/rfc5689.html#section-5.2
/// 
/// Definition:
///
/// <!ELEMENT mkcol-response (propstat+)>
pub struct MkCalendarResponse<E: dav::Extension>(pub Vec<dav::PropStat<E>>);

// --- (REPORT PART) ---

/// Name:  calendar-query
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Defines a report for querying calendar object resources.
/// 
/// Description:  See Section 7.8.
/// 
/// Definition:
///
/// <!ELEMENT calendar-query ((DAV:allprop |
///                            DAV:propname |
///                            DAV:prop)?, filter, timezone?)>
pub struct CalendarQuery<E: dav::Extension> {
    pub selector: Option<CalendarSelector<E>>,
    pub filter: Filter,
    pub timezone: Option<TimeZone>,
}

///   Name:  calendar-multiget
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  CalDAV report used to retrieve specific calendar object
/// resources.
///
/// Description:  See Section 7.9.
///
/// Definition:
///
/// <!ELEMENT calendar-multiget ((DAV:allprop |
///                               DAV:propname |
///                               DAV:prop)?, DAV:href+)>
pub struct CalendarMultiget<E: dav::Extension> {
    pub selector: Option<CalendarSelector<E>>,
    pub href: Vec<dav::Href>,
}

/// Name:  free-busy-query
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  CalDAV report used to generate a VFREEBUSY to determine
/// busy time over a specific time range.
///
/// Description:  See Section 7.10.
///
/// Definition:
/// <!ELEMENT free-busy-query (time-range)>
pub struct FreeBusyQuery(pub TimeRange);

// ----- Hooks -----
pub enum ResourceType {
    Calendar,
}

/// Check the matching Property object for documentation
pub enum PropertyRequest {
    CalendarDescription,
    CalendarTimezone,
    SupportedCalendarComponentSet,
    SupportedCalendarData,
    MaxResourceSize,
    MinDateTime,
    MaxDateTime,
    MaxInstances,
    MaxAttendeesPerInstance,
    SupportedCollationSet,    
    CalendarData(CalendarDataRequest),
}
pub enum Property {
    /// Name:  calendar-description
    /// 
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a human-readable description of the calendar
    /// collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MAY be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).  An xml:lang attribute indicating the human
    /// language of the description SHOULD be set for this property by
    /// clients or through server provisioning.  Servers MUST return any
    /// xml:lang attribute if set for the property.
    ///
    /// Description:  If present, the property contains a description of the
    /// calendar collection that is suitable for presentation to a user.
    /// If not present, the client should assume no description for the
    /// calendar collection.
    ///
    /// Definition:
    ///
    /// <!ELEMENT calendar-description (#PCDATA)>
    /// PCDATA value: string
    ///
    /// Example:
    ///
    /// <C:calendar-description xml:lang="fr-CA"
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav"
    /// >Calendrier de Mathilde Desruisseaux</C:calendar-description>
    CalendarDescription {
        lang: Option<String>,
        text: String,
    },

    /// 5.2.2.  CALDAV:calendar-timezone Property
    ///
    /// Name:  calendar-timezone
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Specifies a time zone on a calendar collection.
    ///
    /// Conformance:  This property SHOULD be defined on all calendar
    /// collections.  If defined, it SHOULD NOT be returned by a PROPFIND
    /// DAV:allprop request (as defined in Section 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:calendar-timezone property is used to
    /// specify the time zone the server should rely on to resolve "date"
    /// values and "date with local time" values (i.e., floating time) to
    /// "date with UTC time" values.  The server will require this
    /// information to determine if a calendar component scheduled with
    /// "date" values or "date with local time" values overlaps a CALDAV:
    /// time-range specified in a CALDAV:calendar-query REPORT.  The
    /// server will also require this information to compute the proper
    /// FREEBUSY time period as "date with UTC time" in the VFREEBUSY
    /// component returned in a response to a CALDAV:free-busy-query
    /// REPORT request that takes into account calendar components
    /// scheduled with "date" values or "date with local time" values.  In
    /// the absence of this property, the server MAY rely on the time zone
    /// of their choice.
    ///
    /// Note:  The iCalendar data embedded within the CALDAV:calendar-
    /// timezone XML element MUST follow the standard XML character data
    /// encoding rules, including use of &lt;, &gt;, &amp; etc. entity
    /// encoding or the use of a <![CDATA[ ... ]]> construct.  In the
    /// later case, the iCalendar data cannot contain the character
    /// sequence "]]>", which is the end delimiter for the CDATA section.
    ///
    ///    Definition:
    ///
    /// <!ELEMENT calendar-timezone (#PCDATA)>
    ///     PCDATA value: an iCalendar object with exactly one VTIMEZONE component.
    ///
    /// Example:
    ///
    /// <C:calendar-timezone
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav">BEGIN:VCALENDAR
    /// PRODID:-//Example Corp.//CalDAV Client//EN
    /// VERSION:2.0
    /// BEGIN:VTIMEZONE
    /// TZID:US-Eastern
    /// LAST-MODIFIED:19870101T000000Z
    /// BEGIN:STANDARD
    /// DTSTART:19671029T020000
    /// RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10
    /// TZOFFSETFROM:-0400
    /// TZOFFSETTO:-0500
    /// TZNAME:Eastern Standard Time (US &amp; Canada)
    /// END:STANDARD
    /// BEGIN:DAYLIGHT
    /// DTSTART:19870405T020000
    /// RRULE:FREQ=YEARLY;BYDAY=1SU;BYMONTH=4
    /// TZOFFSETFROM:-0500
    /// TZOFFSETTO:-0400
    /// TZNAME:Eastern Daylight Time (US &amp; Canada)
    /// END:DAYLIGHT
    /// END:VTIMEZONE
    /// END:VCALENDAR
    /// </C:calendar-timezone>
    //@FIXME we might want to put a buffer here or an iCal parsed object
    CalendarTimezone(String),

    /// Name:  supported-calendar-component-set
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Specifies the calendar component types (e.g., VEVENT,
    /// VTODO, etc.) that calendar object resources can contain in the
    /// calendar collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:supported-calendar-component-set property is
    /// used to specify restrictions on the calendar component types that
    /// calendar object resources may contain in a calendar collection.
    /// Any attempt by the client to store calendar object resources with
    /// component types not listed in this property, if it exists, MUST
    /// result in an error, with the CALDAV:supported-calendar-component
    /// precondition (Section 5.3.2.1) being violated.  Since this
    /// property is protected, it cannot be changed by clients using a
    /// PROPPATCH request.  However, clients can initialize the value of
    /// this property when creating a new calendar collection with
    /// MKCALENDAR.  The empty-element tag <C:comp name="VTIMEZONE"/> MUST
    /// only be specified if support for calendar object resources that
    /// only contain VTIMEZONE components is provided or desired.  Support
    /// for VTIMEZONE components in calendar object resources that contain
    /// VEVENT or VTODO components is always assumed.  In the absence of
    /// this property, the server MUST accept all component types, and the
    /// client can assume that all component types are accepted.
    ///
    /// Definition:
    ///
    /// <!ELEMENT supported-calendar-component-set (comp+)>
    ///
    /// Example:
    ///
    /// <C:supported-calendar-component-set
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav">
    ///     <C:comp name="VEVENT"/>
    ///     <C:comp name="VTODO"/>
    /// </C:supported-calendar-component-set>
    SupportedCalendarComponentSet(Vec<CompSupport>),

    ///  Name:  supported-calendar-data
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Specifies what media types are allowed for calendar object
    /// resources in a calendar collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:supported-calendar-data property is used to
    /// specify the media type supported for the calendar object resources
    /// contained in a given calendar collection (e.g., iCalendar version
    /// 2.0).  Any attempt by the client to store calendar object
    /// resources with a media type not listed in this property MUST
    /// result in an error, with the CALDAV:supported-calendar-data
    /// precondition (Section 5.3.2.1) being violated.  In the absence of
    /// this property, the server MUST only accept data with the media
    /// type "text/calendar" and iCalendar version 2.0, and clients can
    /// assume that the server will only accept this data.
    ///
    /// Definition:
    ///
    /// <!ELEMENT supported-calendar-data (calendar-data+)>
    ///
    /// Example:
    ///
    /// <C:supported-calendar-data
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav">
    ///     <C:calendar-data content-type="text/calendar" version="2.0"/>
    /// </C:supported-calendar-data>
    ///
    /// -----
    ///
    /// <!ELEMENT calendar-data EMPTY>
    ///
    /// when nested in the CALDAV:supported-calendar-data property
    /// to specify a supported media type for calendar object
    /// resources;
    SupportedCalendarData(Vec<CalendarDataEmpty>),

    ///  Name:  max-resource-size
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a numeric value indicating the maximum size of a
    /// resource in octets that the server is willing to accept when a
    /// calendar object resource is stored in a calendar collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:max-resource-size is used to specify a
    /// numeric value that represents the maximum size in octets that the
    /// server is willing to accept when a calendar object resource is
    /// stored in a calendar collection.  Any attempt to store a calendar
    /// object resource exceeding this size MUST result in an error, with
    /// the CALDAV:max-resource-size precondition (Section 5.3.2.1) being
    /// violated.  In the absence of this property, the client can assume
    /// that the server will allow storing a resource of any reasonable
    /// size.
    ///
    /// Definition:
    ///
    /// <!ELEMENT max-resource-size (#PCDATA)>
    /// PCDATA value: a numeric value (positive integer)
    ///
    ///    Example:
    ///
    /// <C:max-resource-size xmlns:C="urn:ietf:params:xml:ns:caldav">
    /// 102400
    /// </C:max-resource-size>
    MaxResourceSize(u64),

    /// CALDAV:min-date-time Property
    ///
    /// Name:  min-date-time
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a DATE-TIME value indicating the earliest date and
    /// time (in UTC) that the server is willing to accept for any DATE or
    /// DATE-TIME value in a calendar object resource stored in a calendar
    /// collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:min-date-time is used to specify an
    /// iCalendar DATE-TIME value in UTC that indicates the earliest
    /// inclusive date that the server is willing to accept for any
    /// explicit DATE or DATE-TIME value in a calendar object resource
    /// stored in a calendar collection.  Any attempt to store a calendar
    /// object resource using a DATE or DATE-TIME value earlier than this
    /// value MUST result in an error, with the CALDAV:min-date-time
    /// precondition (Section 5.3.2.1) being violated.  Note that servers
    /// MUST accept recurring components that specify instances beyond
    /// this limit, provided none of those instances have been overridden.
    /// In that case, the server MAY simply ignore those instances outside
    /// of the acceptable range when processing reports on the calendar
    /// object resource.  In the absence of this property, the client can
    /// assume any valid iCalendar date may be used at least up to the
    /// CALDAV:max-date-time value, if that is defined.
    ///
    /// Definition:
    ///
    /// <!ELEMENT min-date-time (#PCDATA)>
    /// PCDATA value: an iCalendar format DATE-TIME value in UTC
    ///
    /// Example:
    ///
    /// <C:min-date-time xmlns:C="urn:ietf:params:xml:ns:caldav">
    /// 19000101T000000Z
    /// </C:min-date-time>
    MinDateTime(DateTime<Utc>),

    /// CALDAV:max-date-time Property
    ///
    /// Name:  max-date-time
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a DATE-TIME value indicating the latest date and
    /// time (in UTC) that the server is willing to accept for any DATE or
    /// DATE-TIME value in a calendar object resource stored in a calendar
    /// collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:max-date-time is used to specify an
    /// iCalendar DATE-TIME value in UTC that indicates the inclusive
    /// latest date that the server is willing to accept for any date or
    /// time value in a calendar object resource stored in a calendar
    /// collection.  Any attempt to store a calendar object resource using
    /// a DATE or DATE-TIME value later than this value MUST result in an
    /// error, with the CALDAV:max-date-time precondition
    /// (Section 5.3.2.1) being violated.  Note that servers MUST accept
    /// recurring components that specify instances beyond this limit,
    /// provided none of those instances have been overridden.  In that
    /// case, the server MAY simply ignore those instances outside of the
    /// acceptable range when processing reports on the calendar object
    /// resource.  In the absence of this property, the client can assume
    /// any valid iCalendar date may be used at least down to the CALDAV:
    /// min-date-time value, if that is defined.
    ///
    /// Definition:
    ///
    /// <!ELEMENT max-date-time (#PCDATA)>
    /// PCDATA value: an iCalendar format DATE-TIME value in UTC
    ///
    /// Example:
    ///
    /// <C:max-date-time xmlns:C="urn:ietf:params:xml:ns:caldav">
    /// 20491231T235959Z
    /// </C:max-date-time>
    MaxDateTime(DateTime<Utc>),

    /// CALDAV:max-instances Property
    ///
    /// Name:  max-instances
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a numeric value indicating the maximum number of
    /// recurrence instances that a calendar object resource stored in a
    /// calendar collection can generate.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:max-instances is used to specify a numeric
    /// value that indicates the maximum number of recurrence instances
    /// that a calendar object resource stored in a calendar collection
    /// can generate.  Any attempt to store a calendar object resource
    /// with a recurrence pattern that generates more instances than this
    /// value MUST result in an error, with the CALDAV:max-instances
    /// precondition (Section 5.3.2.1) being violated.  In the absence of
    /// this property, the client can assume that the server has no limits
    /// on the number of recurrence instances it can handle or expand.
    ///
    /// Definition:
    ///
    /// <!ELEMENT max-instances (#PCDATA)>
    /// PCDATA value: a numeric value (integer greater than zero)
    ///
    /// Example:
    ///
    /// <C:max-instances xmlns:C="urn:ietf:params:xml:ns:caldav">
    /// 100
    /// </C:max-instances>
    MaxInstances(u64),

    ///  CALDAV:max-attendees-per-instance Property
    ///
    /// Name:  max-attendees-per-instance
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Provides a numeric value indicating the maximum number of
    /// ATTENDEE properties in any instance of a calendar object resource
    /// stored in a calendar collection.
    ///
    /// Conformance:  This property MAY be defined on any calendar
    /// collection.  If defined, it MUST be protected and SHOULD NOT be
    /// returned by a PROPFIND DAV:allprop request (as defined in Section
    /// 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:max-attendees-per-instance is used to
    /// specify a numeric value that indicates the maximum number of
    /// iCalendar ATTENDEE properties on any one instance of a calendar
    /// object resource stored in a calendar collection.  Any attempt to
    /// store a calendar object resource with more ATTENDEE properties per
    /// instance than this value MUST result in an error, with the CALDAV:
    /// max-attendees-per-instance precondition (Section 5.3.2.1) being
    /// violated.  In the absence of this property, the client can assume
    /// that the server can handle any number of ATTENDEE properties in a
    /// calendar component.
    ///
    /// Definition:
    ///
    /// <!ELEMENT max-attendees-per-instance (#PCDATA)>
    /// PCDATA value: a numeric value (integer greater than zero)
    ///
    /// Example:
    ///
    /// <C:max-attendees-per-instance
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav">
    /// 25
    /// </C:max-attendees-per-instance>
    MaxAttendeesPerInstance(u64),

    ///  Name:  supported-collation-set
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Identifies the set of collations supported by the server
    /// for text matching operations.
    ///
    /// Conformance:  This property MUST be defined on any resource that
    /// supports a report that does text matching.  If defined, it MUST be
    /// protected and SHOULD NOT be returned by a PROPFIND DAV:allprop
    /// request (as defined in Section 12.14.1 of [RFC2518]).
    ///
    /// Description:  The CALDAV:supported-collation-set property contains
    /// zero or more CALDAV:supported-collation elements, which specify
    /// the collection identifiers of the collations supported by the
    /// server.
    ///
    /// Definition:
    ///
    /// <!ELEMENT supported-collation-set (supported-collation*)>
    /// <!ELEMENT supported-collation (#PCDATA)>
    ///
    /// Example:
    ///
    /// <C:supported-collation-set
    ///     xmlns:C="urn:ietf:params:xml:ns:caldav">
    ///   <C:supported-collation>i;ascii-casemap</C:supported-collation>
    ///   <C:supported-collation>i;octet</C:supported-collation>
    /// </C:supported-collation-set>
    SupportedCollationSet(Vec<SupportedCollation>),

    /// Name:  calendar-data
    ///
    /// Namespace:  urn:ietf:params:xml:ns:caldav
    ///
    /// Purpose:  Specified one of the following:
    ///
    /// 1.  A supported media type for calendar object resources when
    ///     nested in the CALDAV:supported-calendar-data property;
    ///
    /// 2.  The parts of a calendar object resource should be returned by
    ///     a calendaring report;
    ///
    /// 3.  The content of a calendar object resource in a response to a
    ///     calendaring report.
    ///
    /// Description:  When nested in the CALDAV:supported-calendar-data
    /// property, the CALDAV:calendar-data XML element specifies a media
    /// type supported by the CalDAV server for calendar object resources.
    ///
    /// When used in a calendaring REPORT request, the CALDAV:calendar-
    /// data XML element specifies which parts of calendar object
    /// resources need to be returned in the response.  If the CALDAV:
    /// calendar-data XML element doesn't contain any CALDAV:comp element,
    /// calendar object resources will be returned in their entirety.
    ///
    /// Finally, when used in a calendaring REPORT response, the CALDAV:
    /// calendar-data XML element specifies the content of a calendar
    /// object resource.  Given that XML parsers normalize the two-
    /// character sequence CRLF (US-ASCII decimal 13 and US-ASCII decimal
    /// 10) to a single LF character (US-ASCII decimal 10), the CR
    /// character (US-ASCII decimal 13) MAY be omitted in calendar object
    /// resources specified in the CALDAV:calendar-data XML element.
    /// Furthermore, calendar object resources specified in the CALDAV:
    /// calendar-data XML element MAY be invalid per their media type
    /// specification if the CALDAV:calendar-data XML element part of the
    /// calendaring REPORT request did not specify required properties
    /// (e.g., UID, DTSTAMP, etc.), or specified a CALDAV:prop XML element
    /// with the "novalue" attribute set to "yes".
    ///
    /// Note:  The CALDAV:calendar-data XML element is specified in requests
    /// and responses inside the DAV:prop XML element as if it were a
    /// WebDAV property.  However, the CALDAV:calendar-data XML element is
    /// not a WebDAV property and, as such, is not returned in PROPFIND
    /// responses, nor used in PROPPATCH requests.
    /// 
    /// Note:  The iCalendar data embedded within the CALDAV:calendar-data
    /// XML element MUST follow the standard XML character data encoding
    /// rules, including use of &lt;, &gt;, &amp; etc. entity encoding or
    /// the use of a <![CDATA[ ... ]]> construct.  In the later case, the
    /// iCalendar data cannot contain the character sequence "]]>", which
    /// is the end delimiter for the CDATA section.
    CalendarData(CalendarDataPayload),
}

pub enum Violation {
    /// (DAV:resource-must-be-null): A resource MUST NOT exist at the
    /// Request-URI;
    ResourceMustBeNull,

    /// (CALDAV:calendar-collection-location-ok): The Request-URI MUST
    /// identify a location where a calendar collection can be created;
    CalendarCollectionLocationOk,
    
    /// (CALDAV:valid-calendar-data): The time zone specified in CALDAV:
    /// calendar-timezone property MUST be a valid iCalendar object
    /// containing a single valid VTIMEZONE component.
    ValidCalendarData,

    ///@FIXME should not be here but in RFC3744
    /// !!! ERRATA 1002 !!!
    /// (DAV:need-privileges): The DAV:bind privilege MUST be granted to
    /// the current user on the parent collection of the Request-URI.
    NeedPrivileges,

    ///  (CALDAV:initialize-calendar-collection): A new calendar collection
    /// exists at the Request-URI.  The DAV:resourcetype of the calendar
    /// collection MUST contain both DAV:collection and CALDAV:calendar
    /// XML elements.
    InitializeCalendarCollection,

    /// (CALDAV:supported-calendar-data): The resource submitted in the
    /// PUT request, or targeted by a COPY or MOVE request, MUST be a
    /// supported media type (i.e., iCalendar) for calendar object
    /// resources;
    SupportedCalendarData,

    /// (CALDAV:valid-calendar-object-resource): The resource submitted in
    /// the PUT request, or targeted by a COPY or MOVE request, MUST obey
    /// all restrictions specified in Section 4.1 (e.g., calendar object
    /// resources MUST NOT contain more than one type of calendar
    /// component, calendar object resources MUST NOT specify the
    /// iCalendar METHOD property, etc.);
    ValidCalendarObjectResource,

    /// (CALDAV:supported-calendar-component): The resource submitted in
    /// the PUT request, or targeted by a COPY or MOVE request, MUST
    /// contain a type of calendar component that is supported in the
    /// targeted calendar collection;
    SupportedCalendarComponent,

    /// (CALDAV:no-uid-conflict): The resource submitted in the PUT
    /// request, or targeted by a COPY or MOVE request, MUST NOT specify
    /// an iCalendar UID property value already in use in the targeted
    /// calendar collection or overwrite an existing calendar object
    /// resource with one that has a different UID property value.
    /// Servers SHOULD report the URL of the resource that is already
    /// making use of the same UID property value in the DAV:href element;
    ///
    /// <!ELEMENT no-uid-conflict (DAV:href)>
    NoUidConflict(Dav::Href),

    /// (CALDAV:max-resource-size): The resource submitted in the PUT
    /// request, or targeted by a COPY or MOVE request, MUST have an octet
    /// size less than or equal to the value of the CALDAV:max-resource-
    /// size property value (Section 5.2.5) on the calendar collection
    /// where the resource will be stored;
    MaxResourceSize,

    /// (CALDAV:min-date-time): The resource submitted in the PUT request,
    /// or targeted by a COPY or MOVE request, MUST have all of its
    /// iCalendar DATE or DATE-TIME property values (for each recurring
    /// instance) greater than or equal to the value of the CALDAV:min-
    /// date-time property value (Section 5.2.6) on the calendar
    /// collection where the resource will be stored;
    MinDateTime,
    
    /// (CALDAV:max-date-time): The resource submitted in the PUT request,
    /// or targeted by a COPY or MOVE request, MUST have all of its
    /// iCalendar DATE or DATE-TIME property values (for each recurring
    /// instance) less than the value of the CALDAV:max-date-time property
    /// value (Section 5.2.7) on the calendar collection where the
    /// resource will be stored;
    MaxDateTime,

    /// (CALDAV:max-instances): The resource submitted in the PUT request,
    /// or targeted by a COPY or MOVE request, MUST generate a number of
    /// recurring instances less than or equal to the value of the CALDAV:
    /// max-instances property value (Section 5.2.8) on the calendar
    /// collection where the resource will be stored;
    MaxInstances,

    /// (CALDAV:max-attendees-per-instance): The resource submitted in the
    /// PUT request, or targeted by a COPY or MOVE request, MUST have a
    /// number of ATTENDEE properties on any one instance less than or
    /// equal to the value of the CALDAV:max-attendees-per-instance
    /// property value (Section 5.2.9) on the calendar collection where
    /// the resource will be stored;
    MaxAttendeesPerInstance,

    /// (CALDAV:valid-filter): The CALDAV:filter XML element (see
    /// Section 9.7) specified in the REPORT request MUST be valid.  For
    /// instance, a CALDAV:filter cannot nest a <C:comp name="VEVENT">
    /// element in a <C:comp name="VTODO"> element, and a CALDAV:filter
    /// cannot nest a <C:time-range start="..." end="..."> element in a
    /// <C:prop name="SUMMARY"> element.
    ValidFilter,

    /// (CALDAV:supported-filter): The CALDAV:comp-filter (see
    /// Section 9.7.1), CALDAV:prop-filter (see Section 9.7.2), and
    /// CALDAV:param-filter (see Section 9.7.3) XML elements used in the
    /// CALDAV:filter XML element (see Section 9.7) in the REPORT request
    /// only make reference to components, properties, and parameters for
    /// which queries are supported by the server, i.e., if the CALDAV:
    /// filter element attempts to reference an unsupported component,
    /// property, or parameter, this precondition is violated.  Servers
    /// SHOULD report the CALDAV:comp-filter, CALDAV:prop-filter, or
    /// CALDAV:param-filter for which it does not provide support.
    ///
    /// <!ELEMENT supported-filter (comp-filter*,
    ///                             prop-filter*,
    ///                             param-filter*)>
    SupportedFilter {
        comp: Vec<CompFilter>,
        prop: Vec<PropFilter>,
        param: Vec<ParamFilter>,
    },

    /// (DAV:number-of-matches-within-limits): The number of matching
    /// calendar object resources must fall within server-specific,
    /// predefined limits.  For example, this condition might be triggered
    /// if a search specification would cause the return of an extremely
    /// large number of responses.
    NumberOfMatchesWithinLimits,
}

// -------- Inner XML elements ---------

/// Some of the reports defined in this section do text matches of
/// character strings provided by the client and are compared to stored
/// calendar data.  Since iCalendar data is, by default, encoded in the
/// UTF-8 charset and may include characters outside the US-ASCII charset
/// range in some property and parameter values, there is a need to
/// ensure that text matching follows well-defined rules.
///
/// To deal with this, this specification makes use of the IANA Collation
/// Registry defined in [RFC4790] to specify collations that may be used
/// to carry out the text comparison operations with a well-defined rule.
/// 
/// The comparisons used in CalDAV are all "substring" matches, as per
/// [RFC4790], Section 4.2.  Collations supported by the server MUST
/// support "substring" match operations.
/// 
/// CalDAV servers are REQUIRED to support the "i;ascii-casemap" and
/// "i;octet" collations, as described in [RFC4790], and MAY support
/// other collations.
/// 
/// Servers MUST advertise the set of collations that they support via
/// the CALDAV:supported-collation-set property defined on any resource
/// that supports reports that use collations.
///
/// Clients MUST only use collations from the list advertised by the
/// server.
///
/// In the absence of a collation explicitly specified by the client, or
/// if the client specifies the "default" collation identifier (as
/// defined in [RFC4790], Section 3.1), the server MUST default to using
/// "i;ascii-casemap" as the collation.
///
/// Wildcards (as defined in [RFC4790], Section 3.2) MUST NOT be used in
/// the collation identifier.
/// 
/// If the client chooses a collation not supported by the server, the
/// server MUST respond with a CALDAV:supported-collation precondition
/// error response.
pub struct SupportedCollation(pub Collation);

/// <!ELEMENT calendar-data (#PCDATA)>
/// PCDATA value: iCalendar object
///
/// when nested in the DAV:prop XML element in a calendaring
/// REPORT response to specify the content of a returned
/// calendar object resource.
pub struct CalendarDataPayload {
    pub mime: Option<CalendarDataSupport>,
    pub payload: String,
}

/// <!ELEMENT calendar-data (comp?,
///                          (expand | limit-recurrence-set)?,
///                          limit-freebusy-set?)>
///
/// when nested in the DAV:prop XML element in a calendaring
/// REPORT request to specify which parts of calendar object
/// resources should be returned in the response;
pub struct CalendarDataRequest {
    pub mime: Option<CalendarDataSupport>,
    pub comp: Option<Comp>,
    pub recurrence: Option<RecurrenceModifier>,
    pub limit_freebusy_set: Option<LimitFreebusySet>,
}

/// calendar-data specialization for Property
///
/// <!ELEMENT calendar-data EMPTY>
///
/// when nested in the CALDAV:supported-calendar-data property
/// to specify a supported media type for calendar object
/// resources;
pub struct CalendarDataEmpty(pub Option<CalendarDataSupport>);

/// <!ATTLIST calendar-data content-type CDATA "text/calendar"
///                         version CDATA "2.0">
/// content-type value: a MIME media type
/// version value: a version string
/// attributes can be used on all three variants of the
/// CALDAV:calendar-data XML element.
pub struct CalendarDataSupport {
    pub content_type: String,
    pub version: String,
}

/// Name:  comp
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Defines which component types to return.
///
/// Description:  The name value is a calendar component name (e.g.,
/// VEVENT).
///
/// Definition:
///
/// <!ELEMENT comp ((allprop | prop*), (allcomp | comp*))>
/// <!ATTLIST comp name CDATA #REQUIRED>
/// name value: a calendar component name
///
/// Note:  The CALDAV:prop and CALDAV:allprop elements have the same name
/// as the DAV:prop and DAV:allprop elements defined in [RFC2518].
/// However, the CALDAV:prop and CALDAV:allprop elements are defined
/// in the "urn:ietf:params:xml:ns:caldav" namespace instead of the
/// "DAV:" namespace.
pub struct Comp {
    pub name: Component,
    pub additional_rules: Option<CompInner>,
}
pub struct CompInner {
    pub prop_kind: PropKind,
    pub comp_kind: CompKind,
}

/// For SupportedCalendarComponentSet
///
/// Definition:
///
/// <!ELEMENT supported-calendar-component-set (comp+)>
///
/// Example:
///
/// <C:supported-calendar-component-set
///     xmlns:C="urn:ietf:params:xml:ns:caldav">
///     <C:comp name="VEVENT"/>
///     <C:comp name="VTODO"/>
/// </C:supported-calendar-component-set>
pub struct CompSupport(pub Component);

/// Name:  allcomp
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies that all components shall be returned.
///
/// Description:  The CALDAV:allcomp XML element can be used when the
/// client wants all types of components returned by a calendaring
/// REPORT request.
/// 
/// Definition:
///
/// <!ELEMENT allcomp EMPTY>
pub enum CompKind {
    AllComp,
    Comp(Vec<Comp>),
}

/// Name:  allprop
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies that all properties shall be returned.
///
/// Description:  The CALDAV:allprop XML element can be used when the
/// client wants all properties of components returned by a
/// calendaring REPORT request.
///
/// Definition:
///
/// <!ELEMENT allprop EMPTY>
///
/// Note:  The CALDAV:allprop element has the same name as the DAV:
/// allprop element defined in [RFC2518].  However, the CALDAV:allprop
/// element is defined in the "urn:ietf:params:xml:ns:caldav"
/// namespace instead of the "DAV:" namespace.
pub enum PropKind {
    AllProp,
    Prop(Vec<CalProp>),
}

/// Name:  prop
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Defines which properties to return in the response.
///
/// Description:  The "name" attribute specifies the name of the calendar
/// property to return (e.g., ATTENDEE).  The "novalue" attribute can
/// be used by clients to request that the actual value of the
/// property not be returned (if the "novalue" attribute is set to
/// "yes").  In that case, the server will return just the iCalendar
/// property name and any iCalendar parameters and a trailing ":"
/// without the subsequent value data.
///
/// Definition:
/// <!ELEMENT prop EMPTY>
/// <!ATTLIST prop name CDATA #REQUIRED novalue (yes | no) "no">
/// name value: a calendar property name
/// novalue value: "yes" or "no"
///
/// Note:  The CALDAV:prop element has the same name as the DAV:prop
/// element defined in [RFC2518].  However, the CALDAV:prop element is
/// defined in the "urn:ietf:params:xml:ns:caldav" namespace instead
/// of the "DAV:" namespace.
pub struct CalProp {
    pub name: ComponentProperty,
    pub novalue: Option<bool>,
}

pub enum RecurrenceModifier {
    Expand(Expand),
    LimitRecurrenceSet(LimitRecurrenceSet),
}

/// Name:  expand
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Forces the server to expand recurring components into
/// individual recurrence instances.
///
/// Description:  The CALDAV:expand XML element specifies that for a
/// given calendaring REPORT request, the server MUST expand the
/// recurrence set into calendar components that define exactly one
/// recurrence instance, and MUST return only those whose scheduled
/// time intersect a specified time range.
/// 
/// The "start" attribute specifies the inclusive start of the time
/// range, and the "end" attribute specifies the non-inclusive end of
/// the time range.  Both attributes are specified as date with UTC
/// time value.  The value of the "end" attribute MUST be greater than
/// the value of the "start" attribute.
///
/// The server MUST use the same logic as defined for CALDAV:time-
/// range to determine if a recurrence instance intersects the
/// specified time range.
///
/// Recurring components, other than the initial instance, MUST
/// include a RECURRENCE-ID property indicating which instance they
/// refer to.
///
/// The returned calendar components MUST NOT use recurrence
/// properties (i.e., EXDATE, EXRULE, RDATE, and RRULE) and MUST NOT
/// have reference to or include VTIMEZONE components.  Date and local
/// time with reference to time zone information MUST be converted
/// into date with UTC time.
///
/// Definition:
///
/// <!ELEMENT expand EMPTY>
/// <!ATTLIST expand start CDATA #REQUIRED
///                  end   CDATA #REQUIRED>
/// start value: an iCalendar "date with UTC time"
/// end value: an iCalendar "date with UTC time"
pub struct Expand(pub DateTime<Utc>, pub DateTime<Utc>);

/// CALDAV:limit-recurrence-set XML Element
///
/// Name:  limit-recurrence-set
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies a time range to limit the set of "overridden
/// components" returned by the server.
///
/// Description:  The CALDAV:limit-recurrence-set XML element specifies
/// that for a given calendaring REPORT request, the server MUST
/// return, in addition to the "master component", only the
/// "overridden components" that impact a specified time range.  An
/// overridden component impacts a time range if its current start and
/// end times overlap the time range, or if the original start and end
/// times -- the ones that would have been used if the instance were
/// not overridden -- overlap the time range.
///
/// The "start" attribute specifies the inclusive start of the time
/// range, and the "end" attribute specifies the non-inclusive end of
/// the time range.  Both attributes are specified as date with UTC
/// time value.  The value of the "end" attribute MUST be greater than
/// the value of the "start" attribute.
///
/// The server MUST use the same logic as defined for CALDAV:time-
/// range to determine if the current or original scheduled time of an
/// "overridden" recurrence instance intersects the specified time
/// range.
///
/// Overridden components that have a RANGE parameter on their
/// RECURRENCE-ID property may specify one or more instances in the
/// recurrence set, and some of those instances may fall within the
/// specified time range or may have originally fallen within the
/// specified time range prior to being overridden.  If that is the
/// case, the overridden component MUST be included in the results, as
/// it has a direct impact on the interpretation of instances within
/// the specified time range.
///
/// Definition:
///
/// <!ELEMENT limit-recurrence-set EMPTY>
/// <!ATTLIST limit-recurrence-set start CDATA #REQUIRED
///                                end   CDATA #REQUIRED>
/// start value: an iCalendar "date with UTC time"
/// end value: an iCalendar "date with UTC time"
pub struct LimitRecurrenceSet(pub DateTime<Utc>, pub DateTime<Utc>);

/// Name:  limit-freebusy-set
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies a time range to limit the set of FREEBUSY values
/// returned by the server.
///
/// Description:  The CALDAV:limit-freebusy-set XML element specifies
/// that for a given calendaring REPORT request, the server MUST only
/// return the FREEBUSY property values of a VFREEBUSY component that
/// intersects a specified time range.
///
/// The "start" attribute specifies the inclusive start of the time
/// range, and the "end" attribute specifies the non-inclusive end of
/// the time range.  Both attributes are specified as "date with UTC
/// time" value.  The value of the "end" attribute MUST be greater
/// than the value of the "start" attribute.
///
/// The server MUST use the same logic as defined for CALDAV:time-
/// range to determine if a FREEBUSY property value intersects the
/// specified time range.
///
/// Definition:
/// <!ELEMENT limit-freebusy-set EMPTY>
/// <!ATTLIST limit-freebusy-set start CDATA #REQUIRED
///                              end   CDATA #REQUIRED>
/// start value: an iCalendar "date with UTC time"
/// end value: an iCalendar "date with UTC time"
pub struct LimitFreebusySet(pub DateTime<Utc>, pub DateTime<Utc>);

/// Used by CalendarQuery & CalendarMultiget
pub enum CalendarSelector<E: dav::Extension> {
    AllProp,
    PropName,
    Prop(dav::PropName<E>),
}

/// Name:  comp-filter
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies search criteria on calendar components.
///
/// Description:  The CALDAV:comp-filter XML element specifies a query
/// targeted at the calendar object (i.e., VCALENDAR) or at a specific
/// calendar component type (e.g., VEVENT).  The scope of the
/// CALDAV:comp-filter XML element is the calendar object when used as
/// a child of the CALDAV:filter XML element.  The scope of the
/// CALDAV:comp-filter XML element is the enclosing calendar component
/// when used as a child of another CALDAV:comp-filter XML element.  A
/// CALDAV:comp-filter is said to match if:
///
///   *  The CALDAV:comp-filter XML element is empty and the calendar
///      object or calendar component type specified by the "name"
///      attribute exists in the current scope;
///
///   or:
///
///   *  The CALDAV:comp-filter XML element contains a CALDAV:is-not-
///      defined XML element and the calendar object or calendar
///      component type specified by the "name" attribute does not exist
///      in the current scope;
///
///   or:
///
///   *  The CALDAV:comp-filter XML element contains a CALDAV:time-range
///      XML element and at least one recurrence instance in the
///      targeted calendar component is scheduled to overlap the
///      specified time range, and all specified CALDAV:prop-filter and
///      CALDAV:comp-filter child XML elements also match the targeted
///      calendar component;
///
///   or:
///
///   *  The CALDAV:comp-filter XML element only contains CALDAV:prop-
///      filter and CALDAV:comp-filter child XML elements that all match
///      the targeted calendar component.
///
/// Definition:
/// <!ELEMENT comp-filter (is-not-defined | (time-range?,
///                        prop-filter*, comp-filter*))>
///
///      <!ATTLIST comp-filter name CDATA #REQUIRED>
///      name value: a calendar object or calendar component
///                  type (e.g., VEVENT)
pub struct CompFilter {
    pub name: Component,
    // Option 1 = None, Option 2, 3, 4 = Some
    pub additional_rules: Option<CompFilterRules>,
}
pub enum CompFilterRules {
    // Option 2
    IsNotDefined,
    // Options 3 & 4
    Matches(CompFilterMatch),
}
pub struct CompFilterMatch {
    pub time_range: Option<TimeRange>,
    pub prop_filter: Vec<PropFilter>,
    pub comp_filter: Vec<CompFilter>,
}

/// Name:  prop-filter
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
/// 
/// Purpose:  Specifies search criteria on calendar properties.
///
/// Description:  The CALDAV:prop-filter XML element specifies a query
/// targeted at a specific calendar property (e.g., CATEGORIES) in the
/// scope of the enclosing calendar component.  A calendar property is
/// said to match a CALDAV:prop-filter if:
///
///   *  The CALDAV:prop-filter XML element is empty and a property of
///      the type specified by the "name" attribute exists in the
///      enclosing calendar component;
///
///   or:
///
///   *  The CALDAV:prop-filter XML element contains a CALDAV:is-not-
///      defined XML element and no property of the type specified by
///      the "name" attribute exists in the enclosing calendar
///      component;
///
///   or:
///
///   *  The CALDAV:prop-filter XML element contains a CALDAV:time-range
///      XML element and the property value overlaps the specified time
///      range, and all specified CALDAV:param-filter child XML elements
///      also match the targeted property;
///
///   or:
///
///   *  The CALDAV:prop-filter XML element contains a CALDAV:text-match
///      XML element and the property value matches it, and all
///      specified CALDAV:param-filter child XML elements also match the
///      targeted property;
///
/// Definition:
///
///      <!ELEMENT prop-filter (is-not-defined |
///                             ((time-range | text-match)?,
///                               param-filter*))>
///
///      <!ATTLIST prop-filter name CDATA #REQUIRED>
///      name value: a calendar property name (e.g., ATTENDEE)
pub struct PropFilter {
    pub name: Component,
    // None = Option 1, Some() = Option 2, 3 & 4
    pub additional_rules: Option<PropFilterRules>,
}
pub enum PropFilterRules {
    // Option 2
    IsNotDefined,
    // Options 3 & 4
    Match(PropFilterMatch),
}
pub struct PropFilterMatch {
    pub time_range: Option<TimeRange>,
    pub time_or_text: Option<TimeOrText>,
    pub param_filter: Vec<ParamFilter>,
}
pub enum TimeOrText {
    Time(TimeRange),
    Text(TextMatch),
}

///  Name:  text-match
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies a substring match on a property or parameter
/// value.
///
/// Description:  The CALDAV:text-match XML element specifies text used
/// for a substring match against the property or parameter value
/// specified in a calendaring REPORT request.
///
/// The "collation" attribute is used to select the collation that the
/// server MUST use for character string matching.  In the absence of
/// this attribute, the server MUST use the "i;ascii-casemap"
/// collation.
///
/// The "negate-condition" attribute is used to indicate that this
/// test returns a match if the text matches when the attribute value
/// is set to "no", or return a match if the text does not match, if
/// the attribute value is set to "yes".  For example, this can be
/// used to match components with a STATUS property not set to
/// CANCELLED.
///
/// Definition:
/// <!ELEMENT text-match (#PCDATA)>
/// PCDATA value: string
///  <!ATTLIST text-match collation        CDATA "i;ascii-casemap"
///  negate-condition (yes | no) "no">
pub struct TextMatch {
    pub collation: Option<Collation>,
    pub negate_condition: Option<bool>,
    pub text: String,
}

/// Name:  param-filter
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Limits the search to specific parameter values.
///
/// Description:  The CALDAV:param-filter XML element specifies a query
/// targeted at a specific calendar property parameter (e.g.,
/// PARTSTAT) in the scope of the calendar property on which it is
/// defined.  A calendar property parameter is said to match a CALDAV:
/// param-filter if:
///
///   *  The CALDAV:param-filter XML element is empty and a parameter of
///      the type specified by the "name" attribute exists on the
///      calendar property being examined;
///
///   or:
///
///   *  The CALDAV:param-filter XML element contains a CALDAV:is-not-
///      defined XML element and no parameter of the type specified by
///      the "name" attribute exists on the calendar property being
///      examined;
///
/// Definition:
///
///      <!ELEMENT param-filter (is-not-defined | text-match?)>
///
///      <!ATTLIST param-filter name CDATA #REQUIRED>
///        name value: a property parameter name (e.g., PARTSTAT)
pub struct ParamFilter {
    pub name: PropertyParameter,
    pub additional_rules: Option<ParamFilterMatch>,
}
pub enum ParamFilterMatch {
    IsNotDefined,
    Match(TextMatch),
}

/// CALDAV:is-not-defined XML Element
///
/// Name:  is-not-defined
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies that a match should occur if the enclosing
/// component, property, or parameter does not exist.
///
/// Description:  The CALDAV:is-not-defined XML element specifies that a
/// match occurs if the enclosing component, property, or parameter
/// value specified in a calendaring REPORT request does not exist in
/// the calendar data being tested.
///
/// Definition:
/// <!ELEMENT is-not-defined EMPTY>
/* CURRENTLY INLINED */



/// Name:  timezone
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies the time zone component to use when determining
/// the results of a report.
///
/// Description:  The CALDAV:timezone XML element specifies that for a
/// given calendaring REPORT request, the server MUST rely on the
/// specified VTIMEZONE component instead of the CALDAV:calendar-
/// timezone property of the calendar collection, in which the
/// calendar object resource is contained to resolve "date" values and
/// "date with local time" values (i.e., floating time) to "date with
/// UTC time" values.  The server will require this information to
/// determine if a calendar component scheduled with "date" values or
/// "date with local time" values intersects a CALDAV:time-range
/// specified in a CALDAV:calendar-query REPORT.
///
/// Note:  The iCalendar data embedded within the CALDAV:timezone XML
/// element MUST follow the standard XML character data encoding
/// rules, including use of &lt;, &gt;, &amp; etc. entity encoding or
/// the use of a <![CDATA[ ... ]]> construct.  In the later case, the
///
/// iCalendar data cannot contain the character sequence "]]>", which
/// is the end delimiter for the CDATA section.
///
/// Definition:
///
/// <!ELEMENT timezone (#PCDATA)>
/// PCDATA value: an iCalendar object with exactly one VTIMEZONE
pub struct TimeZone(pub String);

/// Name:  filter
///
/// Namespace:  urn:ietf:params:xml:ns:caldav
///
/// Purpose:  Specifies a filter to limit the set of calendar components
/// returned by the server.
///
/// Description:  The CALDAV:filter XML element specifies the search
/// filter used to limit the calendar components returned by a
/// calendaring REPORT request.
///
/// Definition:
/// <!ELEMENT filter (comp-filter)>
pub struct Filter(pub CompFilter);

/// Name: time-range
///
/// Definition:
///
/// <!ELEMENT time-range EMPTY>
/// <!ATTLIST time-range start CDATA #IMPLIED
///                      end   CDATA #IMPLIED>
/// start value: an iCalendar "date with UTC time"
/// end value: an iCalendar "date with UTC time"
pub enum TimeRange {
    OnlyStart(DateTime<Utc>),
    OnlyEnd(DateTime<Utc>),
    FullRange(DateTime<Utc>, DateTime<Utc>),
}

// ----------------------- ENUM ATTRIBUTES ---------------------

/// Known components
pub enum Component {
    VCalendar,
    VJournal,
    VFreeBusy,
    VEvent,
    VTodo,
    VAlarm,
    VTimeZone,
    Unknown(String),
}
impl Component {
    pub fn as_str<'a>(&'a self) -> &'a str {
        match self {
            Self::VCalendar => "VCALENDAR",
            Self::VJournal => "VJOURNAL",
            Self::VFreeBusy => "VFREEBUSY",
            Self::VEvent => "VEVENT",
            Self::VTodo => "VTODO",
            Self::VAlarm => "VALARM",
            Self::VTimeZone => "VTIMEZONE",
            Self::Unknown(c) => c,
        }
    }
}

/// name="VERSION", name="SUMMARY", etc.
/// Can be set on different objects: VCalendar, VEvent, etc.
/// Might be replaced by an enum later
pub struct ComponentProperty(pub String);

/// like PARSTAT
pub struct PropertyParameter(pub String);
impl PropertyParameter {
    pub fn as_str<'a>(&'a self) -> &'a str {
        self.0.as_str()
    }
}

#[derive(Default)]
pub enum Collation {
    #[default]
    AsciiCaseMap,
    Octet,
    Unknown(String),
}
impl Collation {
    pub fn as_str<'a>(&'a self) -> &'a str {
        match self {
            Self::AsciiCaseMap => "i;ascii-casemap",
            Self::Octet => "i;octet",
            Self::Unknown(c) => c.as_str(),
        }
    }
}*/
