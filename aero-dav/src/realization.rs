use super::acltypes as acl;
use super::caltypes as cal;
use super::error;
use super::synctypes as sync;
use super::types as dav;
use super::xml;

#[derive(Debug, PartialEq, Clone)]
pub struct Disabled(());
impl xml::QRead<Disabled> for Disabled {
    async fn qread(_xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        Err(error::ParsingError::Recoverable)
    }
}
impl xml::QWrite for Disabled {
    async fn qwrite(
        &self,
        _xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        unreachable!()
    }
}

/// The base WebDAV
///
/// Any extension is disabled through an object we can't build
/// due to a private inner element.
#[derive(Debug, PartialEq, Clone)]
pub struct Core {}
impl dav::Extension for Core {
    type Error = Disabled;
    type Property = Disabled;
    type PropertyRequest = Disabled;
    type ResourceType = Disabled;
    type ReportType = Disabled;
}

// WebDAV with the base Calendar implementation (RFC4791)
#[derive(Debug, PartialEq, Clone)]
pub struct Calendar {}
impl dav::Extension for Calendar {
    type Error = cal::Violation;
    type Property = cal::Property;
    type PropertyRequest = cal::PropertyRequest;
    type ResourceType = cal::ResourceType;
    type ReportType = cal::ReportType<Calendar>;
}

// ACL
#[derive(Debug, PartialEq, Clone)]
pub struct Acl {}
impl dav::Extension for Acl {
    type Error = Disabled;
    type Property = acl::Property;
    type PropertyRequest = acl::PropertyRequest;
    type ResourceType = acl::ResourceType;
    type ReportType = Disabled;
}

// All merged
#[derive(Debug, PartialEq, Clone)]
pub struct All {}
impl dav::Extension for All {
    type Error = cal::Violation;
    type Property = Property;
    type PropertyRequest = PropertyRequest;
    type ResourceType = ResourceType;
    type ReportType = ReportType<All>;
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property {
    Cal(cal::Property),
    Acl(acl::Property),
}
impl xml::QRead<Property> for Property {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::Property::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Property::Cal),
        }
        acl::Property::qread(xml).await.map(Property::Acl)
    }
}
impl xml::QWrite for Property {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    Cal(cal::PropertyRequest),
    Acl(acl::PropertyRequest),
}
impl xml::QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::PropertyRequest::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyRequest::Cal),
        }
        acl::PropertyRequest::qread(xml)
            .await
            .map(PropertyRequest::Acl)
    }
}
impl xml::QWrite for PropertyRequest {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Cal(cal::ResourceType),
    Acl(acl::ResourceType),
}
impl xml::QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::ResourceType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ResourceType::Cal),
        }
        acl::ResourceType::qread(xml).await.map(ResourceType::Acl)
    }
}
impl xml::QWrite for ResourceType {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ReportType<E: dav::Extension> {
    Cal(cal::ReportType<E>),
    Sync(sync::SyncCollection<E>),
}
impl<E: dav::Extension> xml::QRead<ReportType<E>> for ReportType<E> {
    async fn qread(
        xml: &mut xml::Reader<impl xml::IRead>,
    ) -> Result<ReportType<E>, error::ParsingError> {
        match cal::ReportType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ReportType::Cal),
        }
        sync::SyncCollection::qread(xml).await.map(ReportType::Sync)
    }
}
impl<E: dav::Extension> xml::QWrite for ReportType<E> {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Sync(s) => s.qwrite(xml).await,
        }
    }
}
