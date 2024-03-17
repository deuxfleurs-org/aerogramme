use super::types as dav;
use super::caltypes as cal;
use super::xml;
use super::error;

#[derive(Debug, PartialEq, Clone)]
pub struct Disabled(());
impl xml::QRead<Disabled> for Disabled {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        Err(error::ParsingError::Recoverable)
    }
}
impl xml::QWrite for Disabled {
    fn qwrite(&self, _xml: &mut xml::Writer<impl xml::IWrite>) -> impl futures::Future<Output = Result<(), quick_xml::Error>> + Send {
        async { unreachable!(); }
    }
}

/// The base WebDAV 
///
/// Any extension is kooh is disabled through an object we can't build
/// due to a private inner element.
#[derive(Debug, PartialEq, Clone)]
pub struct Core {}
impl dav::Extension for Core {
    type Error = Disabled;
    type Property = Disabled;
    type PropertyRequest = Disabled;
    type ResourceType = Disabled;
}

// WebDAV with the base Calendar implementation (RFC4791)
#[derive(Debug, PartialEq, Clone)]
pub struct Calendar {}
impl dav::Extension for Calendar
{
    type Error = cal::Violation;
    type Property = cal::Property;
    type PropertyRequest = cal::PropertyRequest;
    type ResourceType = cal::ResourceType;
}

