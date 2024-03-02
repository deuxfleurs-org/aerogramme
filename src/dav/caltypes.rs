use chrono::{DateTime,Utc};

use super::types as Dav;

//@FIXME for now, we skip the ACL part

pub struct CalExtension {
    pub root: bool
}
impl Dav::Extension for CalExtension {
    type Error = Violation;
    type Property = Property;
    type PropertyRequest = Property; //@FIXME
    type ResourceType = ResourceType;
}

// ----- Root elements -----

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
pub struct MkCalendar<E: Dav::Extension>(Dav::Set<E>);


/// If a response body for a successful request is included, it MUST
/// be a CALDAV:mkcalendar-response XML element.
///
/// <!ELEMENT mkcalendar-response ANY>
pub struct MkCalendarResponse(());

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
pub struct CalendarQuery<T: Dav::Extension> {
    selector: Option<CalendarSelector<T>>,
    filter: Filter,
    timezone: Option<TimeZone>,
}

// ----- Hooks -----
pub enum ResourceType {
    Calendar,
}

pub enum PropertyRequest {
    CalendarDescription,
    CalendarTimezone,
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
    CalendarDescription(String),

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
    SupportedCalendarComponentSet(Vec<Component>),

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
    SupportedCalendarData(Vec<CalendarDataSupport>),

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

    /// (DAV:needs-privilege): The DAV:bind privilege MUST be granted to
    /// the current user on the parent collection of the Request-URI.
    NeedsPrivilege,

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

    /// The CALDAV:filter XML element (see
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
pub struct SupportedCollation(String);

/// calendar-data specialization for Property
pub struct CalendarDataSupport {
    content_type: String,
    version: String,
}

pub enum CalendarSelector<T: Dav::Extension> {
    AllProp,
    PropName,
    Prop(Dav::PropName<T>),
}

pub struct CompFilter {}

pub struct ParamFilter {}

pub struct PropFilter {}

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
pub struct TimeZone(String);

pub struct Filter {}

pub enum Component {
    VEvent,
    VTodo,
}
