use quick_xml::events::attributes::AttrError;

#[derive(Debug)]
pub enum ParsingError {
    NamespacePrefixAlreadyUsed,
    WrongToken,
    TagNotFound,
    QuickXml(quick_xml::Error), 
    Eof
}
impl From<AttrError> for ParsingError {
    fn from(value: AttrError) -> Self {
        Self::QuickXml(value.into())
    }
}
impl From<quick_xml::Error> for ParsingError {
    fn from(value: quick_xml::Error) -> Self {
        Self::QuickXml(value)
    }
}
