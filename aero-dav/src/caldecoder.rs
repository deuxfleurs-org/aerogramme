use super::types as dav;
use super::caltypes::*;
use super::xml;
use super::error;

// ---- ROOT ELEMENTS ---
impl<E: dav::Extension> xml::QRead<MkCalendar<E>> for MkCalendar<E> {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension, N: xml::Node<N>> xml::QRead<MkCalendarResponse<E,N>> for MkCalendarResponse<E,N> {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension> xml::QRead<CalendarQuery<E>> for CalendarQuery<E> {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension> xml::QRead<CalendarMultiget<E>> for CalendarMultiget<E> {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<FreeBusyQuery> for FreeBusyQuery {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}


// ---- EXTENSIONS ---
impl xml::QRead<Violation> for Violation {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<Property> for Property {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<PropertyRequest> for PropertyRequest {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

impl xml::QRead<ResourceType> for ResourceType {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}

// ---- INNER XML ----
impl xml::QRead<SupportedCollation> for SupportedCollation {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        unreachable!();
    }
}
