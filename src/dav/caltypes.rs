use super::types::*;


pub struct CalExtension {}
impl Extension for CalExtension {
    type Error = Violation;
    type Property = Property;
    type PropertyRequest = Property; //@FIXME
}

pub enum Violation {
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
    SupportedFilter,
}


pub enum Property {
    CalendarDescription,
    CalendarTimezone,
}
