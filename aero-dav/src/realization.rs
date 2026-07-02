use super::acltypes as acl;
use super::caltypes as cal;
use super::cardtypes as card;
use super::error;
use super::extension::Extension;
use super::synctypes as sync;
use super::versioningtypes as vers;
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
impl Extension for Core {
    type Error = Disabled;
    type Property = Disabled;
    type PropertyRequest = Disabled;
    type ResourceType = Disabled;
    type ReportType = Disabled;
    type ReportTypeName = Disabled;
    type Multistatus = Disabled;
}

// WebDAV with the base Calendar implementation (RFC4791)
#[derive(Debug, PartialEq, Clone)]
pub struct Calendar {}
impl Extension for Calendar {
    type Error = cal::Violation;
    type Property = cal::Property;
    type PropertyRequest = cal::PropertyRequest;
    type ResourceType = cal::ResourceType;
    type ReportType = cal::ReportType<Calendar>;
    type ReportTypeName = cal::ReportTypeName;
    type Multistatus = Disabled;
}

// WebDAV with the base CardDAV implementation (RFC6352)
#[derive(Debug, PartialEq, Clone)]
pub struct Addressbook {}
impl Extension for Addressbook {
    type Error = card::Violation;
    type Property = card::Property;
    type PropertyRequest = card::PropertyRequest;
    type ResourceType = card::ResourceType;
    type ReportType = card::ReportType<Addressbook>;
    type ReportTypeName = card::ReportTypeName;
    type Multistatus = Disabled;
}

// ACL
#[derive(Debug, PartialEq, Clone)]
pub struct Acl {}
impl Extension for Acl {
    type Error = Disabled;
    type Property = acl::Property;
    type PropertyRequest = acl::PropertyRequest;
    type ResourceType = acl::ResourceType;
    type ReportType = Disabled;
    type ReportTypeName = Disabled;
    type Multistatus = Disabled;
}

// All merged
#[derive(Debug, PartialEq, Clone)]
pub struct All {}
impl Extension for All {
    type Error = cal::Violation;
    type Property = Property<All>;
    type PropertyRequest = PropertyRequest;
    type ResourceType = ResourceType;
    type ReportType = ReportType<All>;
    type ReportTypeName = ReportTypeName;
    type Multistatus = Multistatus;
}

#[derive(Debug, PartialEq, Clone)]
pub enum Property<E: Extension> {
    Cal(cal::Property),
    Card(card::Property),
    Acl(acl::Property),
    Sync(sync::Property),
    Vers(vers::Property<E>),
}
impl<E: Extension> xml::QRead<Property<E>> for Property<E> {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::Property::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Property::<E>::Cal),
        }
        match card::Property::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Property::<E>::Card),
        }
        match acl::Property::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Property::Acl),
        }
        match sync::Property::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Property::Sync),
        }
        vers::Property::qread(xml).await.map(Property::Vers)
    }
}
impl<E: Extension> xml::QWrite for Property<E> {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Card(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
            Self::Sync(s) => s.qwrite(xml).await,
            Self::Vers(v) => v.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum PropertyRequest {
    Cal(cal::PropertyRequest),
    Card(card::PropertyRequest),
    Acl(acl::PropertyRequest),
    Sync(sync::PropertyRequest),
    Vers(vers::PropertyRequest),
}
impl xml::QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::PropertyRequest::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyRequest::Cal),
        }
        match card::PropertyRequest::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyRequest::Card),
        }
        match acl::PropertyRequest::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyRequest::Acl),
        }
        match sync::PropertyRequest::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyRequest::Sync),
        }
        vers::PropertyRequest::qread(xml)
            .await
            .map(PropertyRequest::Vers)
    }
}
impl xml::QWrite for PropertyRequest {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Card(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
            Self::Sync(s) => s.qwrite(xml).await,
            Self::Vers(v) => v.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ResourceType {
    Cal(cal::ResourceType),
    Card(card::ResourceType),
    Acl(acl::ResourceType),
}
impl xml::QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::ResourceType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ResourceType::Cal),
        }
        match card::ResourceType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ResourceType::Card),
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
            Self::Card(c) => c.qwrite(xml).await,
            Self::Acl(a) => a.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ReportType<E: Extension> {
    Cal(cal::ReportType<E>),
    Card(card::ReportType<E>),
    Sync(sync::SyncCollection<E>),
}
impl<E: Extension> xml::QRead<ReportType<E>> for ReportType<E> {
    async fn qread(
        xml: &mut xml::Reader<impl xml::IRead>,
    ) -> Result<ReportType<E>, error::ParsingError> {
        match cal::ReportType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ReportType::Cal),
        }
        match card::ReportType::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ReportType::Card),
        }
        sync::SyncCollection::qread(xml).await.map(ReportType::Sync)
    }
}
impl<E: Extension> xml::QWrite for ReportType<E> {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Card(c) => c.qwrite(xml).await,
            Self::Sync(s) => s.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum ReportTypeName {
    Cal(cal::ReportTypeName),
    Card(card::ReportTypeName),
    Sync(sync::ReportTypeName),
}
impl xml::QRead<ReportTypeName> for ReportTypeName {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        match cal::ReportTypeName::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ReportTypeName::Cal),
        }
        match card::ReportTypeName::qread(xml).await {
            Err(error::ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(ReportTypeName::Card),
        }
        sync::ReportTypeName::qread(xml)
            .await
            .map(ReportTypeName::Sync)
    }
}
impl xml::QWrite for ReportTypeName {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Cal(c) => c.qwrite(xml).await,
            Self::Card(c) => c.qwrite(xml).await,
            Self::Sync(s) => s.qwrite(xml).await,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Multistatus {
    Sync(sync::Multistatus),
}

impl xml::QWrite for Multistatus {
    async fn qwrite(
        &self,
        xml: &mut xml::Writer<impl xml::IWrite>,
    ) -> Result<(), quick_xml::Error> {
        match self {
            Self::Sync(s) => s.qwrite(xml).await,
        }
    }
}

impl xml::QRead<Multistatus> for Multistatus {
    async fn qread(xml: &mut xml::Reader<impl xml::IRead>) -> Result<Self, error::ParsingError> {
        sync::Multistatus::qread(xml).await.map(Self::Sync)
    }
}
