use super::types as dav;
use super::caltypes::*;
use super::xml;
use super::error;

// ---- ROOT ELEMENTS ---

// ---- EXTENSIONS ---
impl xml::QRead<Violation> for Violation {
    async fn qread(&self, xml: &mut xml::Reader<impl xml::IRead>) -> Result<Option<Self>, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<Property> for Property {
    async fn qread(&self, xml: &mut xml::Reader<impl xml::IRead>) -> Result<Option<Self>, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<PropertyRequest> for PropertyRequest {
    async fn qread(&self, xml: &mut xml::Reader<impl xml::IRead>) -> Result<Option<Self>, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<ResourceType> for ResourceType {
    async fn qread(&self, xml: &mut xml::Reader<impl xml::IRead>) -> Result<Option<Self>, error::ParsingError> {
        unreachable!();
    }
}

// ---- INNER XML ----