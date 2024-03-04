use std::borrow::Cow;

use im::HashMap;
use quick_xml::events::{BytesStart, BytesText};
use quick_xml::events::attributes::AttrError;
use quick_xml::name::PrefixDeclaration;
use quick_xml::reader::Reader;
use tokio::io::AsyncBufRead;

use super::types::*;

pub enum ParsingError {
    NamespacePrefixAlreadyUsed,
    WrongToken,
    QuickXml(quick_xml::Error)
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

const DAV_URN: &[u8] = b"DAV:";
const CALDAV_URN: &[u8] = b"urn:ietf:params:xml:ns:caldav";
const CARDDAV_URN: &[u8] = b"urn:ietf:params:xml:ns:carddav";
const XML_URN: &[u8] = b"xml";

#[derive(PartialEq, Clone)]
pub enum XmlNamespace {
    None,
    Dav,
    CalDav,
    CardDav,
    Xml,
    Unknown(Vec<u8>),
}
impl From<&[u8]> for XmlNamespace {
    fn from(value: &[u8]) -> Self {
        match value {
            [] => Self::None,
            DAV_URN => Self::Dav,
            CALDAV_URN => Self::CalDav,
            CARDDAV_URN => Self::CardDav,
            XML_URN => Self::Xml,
            v => Self::Unknown(v.into()),
        }
    }
}

/// Context must stay cheap to clone
/// as we are cloning it from one fonction to another
#[derive(Clone)]
pub struct Context<'a, E: Extension + Clone> {
    pub aliases: HashMap<&'a [u8], XmlNamespace>,
    phantom: std::marker::PhantomData<E>,
}
impl<'a, E: Extension + Clone> Context<'a, E> {
    /// External buffer
    pub fn new() -> Self {
        Self { 
            aliases: HashMap::new(),
            phantom: std::marker::PhantomData 
        }
    }

    pub fn ns_scan(&mut self, token: &'a BytesStart<'a>) -> Result<(XmlNamespace, &[u8]), ParsingError> {
        // Register namespace aliases from attributes (aka namespace bindings)
        for attr_res in token.attributes() {
            let attr = attr_res?;
            match attr.key.as_namespace_binding() {
                None => (),
                Some(PrefixDeclaration::Named(prefix)) => self.ns_alias(attr.value.as_ref(), prefix.as_ref())?,
                Some(PrefixDeclaration::Default) => self.ns_default(attr.value.as_ref())?,
            }
        }

        // Decompose tag name
        let (key, maybe_prefix) = token.name().decompose();
        let ns = self.ns_resolve(maybe_prefix.map(|p| p.into_inner()).unwrap_or(&b""[..]));

        Ok((ns, key.into_inner()))
    }

    fn ns_default(&mut self, fqns: &[u8]) -> Result<(), ParsingError> {
        self.ns_alias(fqns, &b""[..])
    }

    fn ns_alias(&mut self, fqns: &[u8], alias: &'a [u8]) -> Result<(), ParsingError> {
        let parsed_ns = XmlNamespace::from(fqns);
        if let Some(reg_fqns) = self.aliases.get(alias) {
            if *reg_fqns != parsed_ns {
                return Err(ParsingError::NamespacePrefixAlreadyUsed)
            }
        }
        self.aliases.insert(alias, parsed_ns);
        Ok(())
    }

    // If the namespace is not found in the alias table (binding table)
    // we suppose it's a fully qualified namespace (fqns)
    fn ns_resolve(&self, prefix: &[u8]) -> XmlNamespace {
        match self.aliases.get(prefix) {
            Some(fqns) => fqns.clone(),
            None => XmlNamespace::from(prefix),
        }
    }
}

trait DavReader<'a> {
    async fn doctype(&self) -> Result<(), ParsingError>;
    async fn tag(&self) -> Result<BytesStart<'a>, ParsingError>;
    async fn txt(&self) -> Result<Cow<'a, u8>, ParsingError>;
}
/*impl<'a, I: AsyncBufRead+Unpin> DavReader<'a> for Reader<I> {
    async fn doctype(&self) -> Result<(), ParsingError> {
    }
    async fn tag(&self) -> Result<BytesStart<'a>, ParsingError> {
    }
    async fn txt(&self) -> Result<Cow<'a, u8>, ParsingError> {
    }
}*/

pub async fn propfind<E: Extension+Clone>(
    xml: &mut Reader<impl AsyncBufRead+Unpin>, 
    ctx: Context<'_, E>,
    buf: &mut Vec<u8>,
) -> Result<PropFind<E>, ParsingError> {
    let local = ctx.clone();

    match xml.read_event_into_async(buf).await? {
        _ => unimplemented!(),
    }

    unimplemented!();
}
