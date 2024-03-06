use quick_xml::events::attributes::AttrError;

#[derive(Debug)]
pub enum ParsingError {
    MissingChild,
    NamespacePrefixAlreadyUsed,
    WrongToken,
    TagNotFound,
    InvalidValue,
    Utf8Error(std::str::Utf8Error),
    QuickXml(quick_xml::Error), 
    Chrono(chrono::format::ParseError),
    Int(std::num::ParseIntError),
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
impl From<chrono::format::ParseError> for ParsingError {
    fn from(value: chrono::format::ParseError) -> Self {
        Self::Chrono(value)
    }
}

impl From<std::num::ParseIntError> for ParsingError {
    fn from(value: std::num::ParseIntError) -> Self {
        Self::Int(value)
    }
}
