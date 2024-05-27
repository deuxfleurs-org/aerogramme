use super::types as dav;

//@FIXME required for a full DAV implementation
// See section 7.1 of the CalDAV RFC
// It seems it's mainly due to the fact that the REPORT method is re-used.
// https://datatracker.ietf.org/doc/html/rfc4791#section-7.1
//
// Defines (required by CalDAV):
// - REPORT method
// - expand-property root report method
//
// Defines (required by Sync):
// - limit, nresults
// - supported-report-set

// This property identifies the reports that are supported by the
// resource.
//
// <!ELEMENT supported-report-set (supported-report*)>
// <!ELEMENT supported-report report>
// <!ELEMENT report ANY>
// ANY value: a report element type

#[derive(Debug, PartialEq, Clone)]
pub enum Report<E: dav::Extension> {
    VersionTree,    // Not yet implemented
    ExpandProperty, // Not yet implemented
    Extension(E::ReportType),
}

/// Limit
/// <!ELEMENT limit         (nresults) >
#[derive(Debug, PartialEq, Clone)]
pub struct Limit(pub NResults);

/// NResults
/// <!ELEMENT nresults      (#PCDATA) >
#[derive(Debug, PartialEq, Clone)]
pub struct NResults(pub u64);
