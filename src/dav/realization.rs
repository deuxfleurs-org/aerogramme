use super::types as dav;
use super::caltypes as cal;
use super::xml;
use super::error;

#[derive(Debug, PartialEq)]
pub struct Disabled(());
impl xml::QRead<Disabled> for Disabled {
    async fn qread(&self, xml: &mut xml::Reader<impl xml::IRead>) -> Result<Option<Self>, error::ParsingError> {
        unreachable!();
    }
}
impl xml::QWrite for Disabled {
    async fn qwrite(&self, xml: &mut xml::Writer<impl xml::IWrite>) -> Result<(), quick_xml::Error> {
        unreachable!();
    }
}

/// The base WebDAV 
///
/// Any extension is kooh is disabled through an object we can't build
/// due to a private inner element.
pub struct Core {}
impl dav::Extension for Core {
    type Error = Disabled;
    type Property = Disabled;
    type PropertyRequest = Disabled;
    type ResourceType = Disabled;
}

/*
// WebDAV with the base Calendar implementation (RFC4791)
pub struct CalendarMin {}
impl dav::Extension for CalendarMin
{
    type Error = cal::Violation;
    type Property = cal::Property;
    type PropertyRequest = cal::PropertyRequest;
    type ResourceType = cal::ResourceType;
}
*/
