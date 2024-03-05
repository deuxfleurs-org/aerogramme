use std::borrow::Cow;
use std::future::Future;

use quick_xml::events::{Event, BytesStart, BytesDecl, BytesText};
use quick_xml::events::attributes::AttrError;
use quick_xml::name::{Namespace, QName, PrefixDeclaration, ResolveResult, ResolveResult::*};
use quick_xml::reader::NsReader;
use tokio::io::AsyncBufRead;

use super::types::*;
use super::error::*;

/*
// --- Traits ----

trait Reader = AsyncBufRead+Unpin+'static;

trait Decodable: Extension {
    async fn decode_propreq(xml: &mut PeekRead<impl Reader>) -> Result<Option<Self::PropertyRequest>, ParsingError>;
}
impl Decodable for NoExtension {
    async fn decode_propreq(xml: &mut PeekRead<impl Reader>) -> Result<Option<Disabled>, ParsingError> {
        Ok(None)
    }
}

pub trait QReadable<T: Reader>: Sized {
    async fn read(xml: &mut PeekRead<T>) -> Result<Self, ParsingError>;
}

// --- Peek read with namespaces

const DAV_URN: &[u8] = b"DAV:";
const CALDAV_URN: &[u8] = b"urn:ietf:params:xml:ns:caldav";
const CARDDAV_URN: &[u8] = b"urn:ietf:params:xml:ns:carddav";
//const XML_URN: &[u8] = b"xml";
pub struct PeekRead<T: Reader> {
    evt: Event<'static>,
    rdr: NsReader<T>,
    buf: Vec<u8>,
}
impl<T: Reader> PeekRead<T> {
    async fn new(mut rdr: NsReader<T>) -> Result<Self, ParsingError> {
        let mut buf: Vec<u8> = vec![];
        let evt = rdr.read_event_into_async(&mut buf).await?.into_owned();
        buf.clear();
        Ok(Self { evt, rdr, buf })
    }

    fn peek(&self) -> &Event<'static> {
        &self.evt
    }

    /// skip tag. Can't skip end, can't skip eof.
    async fn skip(&mut self) -> Result<Event<'static>, ParsingError> {
        match &self.evt {
            Event::Start(b) => {
                let _span = self.rdr.read_to_end_into_async(b.to_end().name(), &mut self.buf).await?;
                self.next().await
            },
            Event::End(_) => Err(ParsingError::WrongToken),
            Event::Eof => Err(ParsingError::Eof),
            _ => self.next().await,
        }
    }

    /// read one more tag
    async fn next(&mut self) -> Result<Event<'static>, ParsingError> {
       let evt = self.rdr.read_event_into_async(&mut self.buf).await?.into_owned(); 
       self.buf.clear();
       let old_evt = std::mem::replace(&mut self.evt, evt);
       Ok(old_evt)
    }


    /// check if this is the desired tag
    fn is_tag(&self, ns: &[u8], key: &str) -> bool {
        let qname = match self.peek() {
            Event::Start(bs) | Event::Empty(bs) => bs.name(),
            Event::End(be) => be.name(),
            _ => return false,
        };
    
        let (extr_ns, local) = self.rdr.resolve_element(qname);

        if local.into_inner() != key.as_bytes() {
            return false
        }
        
        match extr_ns {
            ResolveResult::Bound(v) => v.into_inner() == ns,
            _ => false,
        }
    }

    /// find start tag
    async fn tag_start(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        loop {
            match self.peek() {
                Event::Start(b) if self.is_tag(ns, key) => break,
                _ => { self.skip().await?; },
            }
        }
        self.next().await
    }

    // find stop tag
    async fn tag_stop(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        loop {
            match self.peek() {
                Event::End(b) if self.is_tag(ns, key) => break,
                _ => { self.skip().await?; },
            }
        }
        self.next().await
    }
}

// ----- Decode ----

impl<E: Decodable, T: Reader> QReadable<T> for PropFind<E> {
    async fn read(xml: &mut PeekRead<T>) -> Result<PropFind<E>, ParsingError> {
        // Find propfind
        xml.tag_start(DAV_URN, "propfind").await?;

        // Find any tag
        let propfind: PropFind<E> = loop {
            match xml.peek() {
                Event::Start(_) if xml.is_tag(DAV_URN, "allprop") => {
                    xml.tag_start(DAV_URN, "allprop").await?;
                    let r = PropFind::AllProp(Some(Include::read(xml).await?));
                    xml.tag_stop(DAV_URN, "allprop").await?;
                    break r
                },
                Event::Start(_) if xml.is_tag(DAV_URN, "prop") => {
                    xml.tag_start(DAV_URN, "prop").await?;
                    let r = PropFind::Prop(PropName::read(xml).await?);
                    xml.tag_stop(DAV_URN, "prop").await?;
                    break r
                },
                Event::Empty(_) if xml.is_tag(DAV_URN, "allprop") => {
                    xml.next().await?;
                    break PropFind::AllProp(None)
                },
                Event::Empty(_) if xml.is_tag(DAV_URN, "propname") => {
                    xml.next().await?;
                    break PropFind::PropName
                },
                _ => { xml.skip().await?; },
            }
        };

        // Close tag
        xml.tag_stop(DAV_URN, "propfind").await?;

        Ok(propfind)
    }
}


impl<E: Decodable, T: Reader> QReadable<T> for Include<E> {
    async fn read(xml: &mut PeekRead<T>) -> Result<Include<E>, ParsingError> {
        xml.tag_start(DAV_URN, "include").await?;
        let mut acc: Vec<PropertyRequest<E>> = Vec::new();
        loop {
            match xml.peek() {
                Event::Start(_) | Event::Empty(_) => acc.push(PropertyRequest::read(xml).await?),
                Event::End(_) if xml.is_tag(DAV_URN, "include") => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "include").await?;
        Ok(Include(acc))
    }
}

impl<E: Decodable, T: Reader> QReadable<T> for PropName<E> {
    async fn read(xml: &mut PeekRead<T>) -> Result<PropName<E>, ParsingError> {
        xml.tag_start(DAV_URN, "prop").await?;
        let mut acc: Vec<PropertyRequest<E>> = Vec::new();
        loop {
            match xml.peek() {
                Event::Start(_) | Event::Empty(_) => acc.push(PropertyRequest::read(xml).await?),
                Event::End(_) if xml.is_tag(DAV_URN, "prop") => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(PropName(acc))
    }
}

impl<E: Decodable, T: Reader> QReadable<T> for PropertyRequest<E> {
    async fn read(xml: &mut PeekRead<T>) -> Result<PropertyRequest<E>, ParsingError> {
        loop {
            let (need_close, bs) = match xml.peek() {
                Event::Start(b) => (true, b),
                Event::Empty(b) => (false, b),
                _ => { 
                    xml.skip().await?; 
                    continue
                },
            };

            let mut maybe_res = None;

            // Option 1: a pure DAV property
            let (ns, loc) = xml.rdr.resolve_element(bs.name());
            if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
                maybe_res = match loc.into_inner() {
                    b"creationdate" => Some(PropertyRequest::CreationDate),
                    b"displayname" => Some(PropertyRequest::DisplayName),
                    b"getcontentlanguage" => Some(PropertyRequest::GetContentLanguage),
                    b"getcontentlength" => Some(PropertyRequest::GetContentLength),
                    b"getetag" => Some(PropertyRequest::GetEtag),
                    b"getlastmodified" => Some(PropertyRequest::GetLastModified),
                    b"lockdiscovery" => Some(PropertyRequest::LockDiscovery),
                    b"resourcetype" => Some(PropertyRequest::ResourceType),
                    b"supportedlock" => Some(PropertyRequest::SupportedLock),
                    _ => None,
                };
            }

            // Option 2: an extension property
            if maybe_res.is_none() {
                maybe_res = E::decode_propreq(xml).await?.map(PropertyRequest::Extension);
            }

            // In any cases, we must close the opened tag
            if need_close {
                xml.skip().await?; 
            }

            // Return if something is found - otherwise loop
            if let Some(res) = maybe_res {
                return Ok(res)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_propfind_propname() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
<rando/>
<garbage><old/></garbage>
<D:propfind xmlns:D="DAV:">
    <D:propname/>
</D:propfind>
"#;

        let mut rdr = PeekRead::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = PropFind::<NoExtension>::read(&mut rdr).await.unwrap();
        assert!(matches!(got, PropFind::PropName));
    }
/*
    #[tokio::test]
    async fn basic_propfind_prop() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
<rando/>
<garbage><old/></garbage>
<D:propfind xmlns:D="DAV:">
    <D:prop>
        <displayname/>
        <getcontentlength/>
        <getcontenttype/>
        <getetag/>
        <getlastmodified/>
        <resourcetype/>
        <supportedlock/>
    </D:prop>
</D:propfind>
"#;

        let mut rdr = PeekRead::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = PropFind::<NoExtension>::read(&mut rdr).await.unwrap();
        assert_eq!(got, PropFind::Prop(PropName(vec![
            PropertyRequest::DisplayName,
            PropertyRequest::GetContentLength,
            PropertyRequest::GetContentType,
            PropertyRequest::GetEtag,
            PropertyRequest::GetLastModified,
            PropertyRequest::ResourceType,
            PropertyRequest::SupportedLock,
        ])));
    }
    */
}
*/
