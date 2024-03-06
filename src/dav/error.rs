use quick_xml::events::attributes::AttrError;

#[derive(Debug)]
pub enum ParsingError {
    MissingChild,
    NamespacePrefixAlreadyUsed,
    WrongToken,
    TagNotFound,
    Utf8Error(std::str::Utf8Error),
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
impl From<std::str::Utf8Error> for ParsingError {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::Utf8Error(value)
    }
}
