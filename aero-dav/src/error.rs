use quick_xml::events::attributes::AttrError;

#[derive(Debug)]
pub enum ParsingError {
    Recoverable,
    MissingChild,
    MissingAttribute,
    NamespacePrefixAlreadyUsed,
    WrongToken,
    TagNotFound,
    InvalidValue,
    Utf8Error(std::str::Utf8Error),
    QuickXml(quick_xml::Error),
    Chrono(chrono::format::ParseError),
    Int(std::num::ParseIntError),
    Eof,
}
impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Recoverable => write!(f, "Recoverable"),
            Self::MissingChild => write!(f, "Missing child"),
            Self::MissingAttribute => write!(f, "Missing attribute"),
            Self::NamespacePrefixAlreadyUsed => write!(f, "Namespace prefix already used"),
            Self::WrongToken => write!(f, "Wrong token"),
            Self::TagNotFound => write!(f, "Tag not found"),
            Self::InvalidValue => write!(f, "Invalid value"),
            Self::Utf8Error(_) => write!(f, "Utf8 Error"),
            Self::QuickXml(_) => write!(f, "Quick XML error"),
            Self::Chrono(_) => write!(f, "Chrono error"),
            Self::Int(_) => write!(f, "Number parsing error"),
            Self::Eof => write!(f, "Found EOF while expecting data"),
        }
    }
}
impl std::error::Error for ParsingError {}
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
