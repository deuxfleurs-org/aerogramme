use std::borrow::Cow;

use quick_xml::events::{Event, BytesStart, BytesDecl, BytesText};
use quick_xml::events::attributes::AttrError;
use quick_xml::name::{Namespace, QName, PrefixDeclaration, ResolveResult, ResolveResult::*};
use quick_xml::reader::NsReader;
use tokio::io::AsyncBufRead;

use super::types::*;

#[derive(Debug)]
pub enum ParsingError {
    NamespacePrefixAlreadyUsed,
    WrongToken,
    TagNotFound,
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
const DAV_NS: ResolveResult = Bound(Namespace(DAV_URN));

pub struct PeekRead<T: AsyncBufRead+Unpin> {
    evt: Event<'static>,
    rdr: NsReader<T>,
    buf: Vec<u8>,
}
impl<T: AsyncBufRead+Unpin> PeekRead<T> {
    async fn new(mut rdr: NsReader<T>) -> Result<Self, ParsingError> {
        let mut buf: Vec<u8> = vec![];
        let evt = rdr.read_event_into_async(&mut buf).await?.into_owned();
        buf.clear();
        Ok(Self { evt, rdr, buf })
    }

    fn peek(&self) -> &Event<'static> {
        &self.evt
    }
    // skip tag, some tags can't be skipped like end, text, cdata
    async fn skip(&mut self) -> Result<(), ParsingError> {
        match &self.evt {
            Event::Start(b) => {
                let _span = self.rdr.read_to_end_into_async(b.to_end().name(), &mut self.buf).await?;
                self.next().await
            },
            Event::Empty(_) | Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_) => self.next().await,
            _ => return Err(ParsingError::WrongToken),
        }
    }

    // read one more tag
    async fn next(&mut self) -> Result<(), ParsingError> {
       let evt = self.rdr.read_event_into_async(&mut self.buf).await?.into_owned(); 
       self.buf.clear();
       self.evt = evt;
       Ok(())
    }
}

pub trait QReadable<T: AsyncBufRead+Unpin>: Sized {
    async fn read(xml: &mut PeekRead<T>) -> Result<Self, ParsingError>;
}

impl<E: Extension, T: AsyncBufRead+Unpin> QReadable<T> for PropFind<E> {
    async fn read(xml: &mut PeekRead<T>) -> Result<PropFind<E>, ParsingError> {

        // Find propfind
        loop {
            match xml.peek() {
                Event::Start(b) if b.local_name().into_inner() == &b"propfind"[..] => break,
                _ => xml.skip().await?,
            }
        }
        xml.next().await?;

        // Find any tag
        let propfind = loop {
            match xml.peek() {
                Event::Start(b) | Event::Empty(b) if b.local_name().into_inner() == &b"allprop"[..] => {
                    unimplemented!()
                },
                Event::Start(b) if b.local_name().into_inner() == &b"prop"[..] => {
                    unimplemented!();
                },
                Event::Empty(b) if b.local_name().into_inner() == &b"propname"[..] => break PropFind::PropName,
                _ => xml.skip().await?,
            }
        };
        xml.next().await?;

        // Close tag
        loop {
            match xml.peek() {
                Event::End(b) if b.local_name().into_inner() == &b"propfind"[..] => break,
                _ => xml.skip().await?,
            }
        }

        Ok(propfind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_propfind() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?><rando/><garbage><old/></garbage><D:propfind xmlns:D="DAV:"><D:propname/></D:propfind>"#;

        let mut rdr = PeekRead::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = PropFind::<NoExtension>::read(&mut rdr).await.unwrap();
        assert!(matches!(got, PropFind::PropName));
    }
}
