use super::xml;

/// It's how we implement a DAV extension
/// (That's the dark magic part...)
pub trait Extension: std::fmt::Debug + PartialEq + Clone {
    type Error: xml::Node<Self::Error>;
    type Property: xml::Node<Self::Property>;
    type PropertyRequest: xml::Node<Self::PropertyRequest>;
    type ResourceType: xml::Node<Self::ResourceType>;
    type ReportType: xml::Node<Self::ReportType>;
    type ReportTypeName: xml::Node<Self::ReportTypeName>;
    type Multistatus: xml::Node<Self::Multistatus>;
}
