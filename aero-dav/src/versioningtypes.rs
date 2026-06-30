use super::types as dav;

/**
 * # RFC 3253 - WebDAV versioning
 *
 * This RFC intends to keep an history of the different versions of a resource.
 * It is not a feature directly used by CalDAV/CardDAV but it introduces
 * new WebDAV generic concepts (like the "REPORT" notion/method).
 *
 * Defines (required by CalDAV):
 * - REPORT method
 * - expand-property root report method
 *
 * Defines (required by Sync):
 * - limit, nresults
 * - supported-report-set
 *
 * This implementation is partial.
 */

// This property identifies the reports that are supported by the
// resource.
//
// <!ELEMENT supported-report-set (supported-report*)>
// <!ELEMENT supported-report report>
// <!ELEMENT report ANY>
// ANY value: a report element type

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    SupportedReportSet,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property<E: dav::Extension> {
    SupportedReportSet(Vec<SupportedReport<E>>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct SupportedReport<E: dav::Extension>(pub ReportName<E>);

#[derive(Debug, PartialEq, Clone)]
pub enum ReportName<E: dav::Extension> {
    VersionTree,
    ExpandProperty,
    Extension(E::ReportTypeName),
}

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
