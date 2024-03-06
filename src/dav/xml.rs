use tokio::io::{AsyncWrite, AsyncBufRead};
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::name::{Namespace, QName, PrefixDeclaration, ResolveResult, ResolveResult::*};
use quick_xml::reader::NsReader;

use super::error::ParsingError;

// Constants
pub const DAV_URN: &[u8] = b"DAV:";
pub const CAL_URN: &[u8] = b"urn:ietf:params:xml:ns:caldav";
pub const CARD_URN: &[u8] = b"urn:ietf:params:xml:ns:carddav";

// Async traits
pub trait IWrite = AsyncWrite + Unpin;
pub trait IRead = AsyncBufRead + Unpin + 'static;

// Serialization/Deserialization traits
pub trait QWrite {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), quick_xml::Error>; 
}
pub trait QRead<T> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<T, ParsingError>;
}

// The representation of an XML node in Rust
pub trait Node<T> = QRead<T> + QWrite + std::fmt::Debug + PartialEq;

// ---------------

/// Transform a Rust object into an XML stream of characters
pub struct Writer<T: IWrite> {
    pub q: quick_xml::writer::Writer<T>,
    pub ns_to_apply: Vec<(String, String)>,
}
impl<T: IWrite> Writer<T> {
    pub fn create_dav_element(&mut self, name: &str) -> BytesStart<'static> {
        self.create_ns_element("D", name)
    }
    pub fn create_cal_element(&mut self, name: &str) -> BytesStart<'static> {
        self.create_ns_element("C", name)
    }

    fn create_ns_element(&mut self, ns: &str, name: &str) -> BytesStart<'static> {
        let mut start = BytesStart::new(format!("{}:{}", ns, name));
        if !self.ns_to_apply.is_empty() {
            start.extend_attributes(self.ns_to_apply.iter().map(|(k, n)| (k.as_str(), n.as_str())));
            self.ns_to_apply.clear()
        }
        start
    }
}

/// Transform an XML stream of characters into a Rust object
pub struct Reader<T: IRead> {
    pub rdr: NsReader<T>,
    evt: Event<'static>,
    buf: Vec<u8>,
}
impl<T: IRead> Reader<T> {
    pub async fn new(mut rdr: NsReader<T>) -> Result<Self, ParsingError> {
        let mut buf: Vec<u8> = vec![];
        let evt = rdr.read_event_into_async(&mut buf).await?.into_owned();
        buf.clear();
        Ok(Self { evt, rdr, buf })
    }

    pub fn peek(&self) -> &Event<'static> {
        &self.evt
    }

    /// skip tag. Can't skip end, can't skip eof.
    pub async fn skip(&mut self) -> Result<Event<'static>, ParsingError> {
        println!("skip on {:?}", &self.evt);
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
    pub async fn next(&mut self) -> Result<Event<'static>, ParsingError> {
       let evt = self.rdr.read_event_into_async(&mut self.buf).await?.into_owned(); 
       self.buf.clear();
       let old_evt = std::mem::replace(&mut self.evt, evt);
       Ok(old_evt)
    }


    /// check if this is the desired tag
    pub fn is_tag(&self, ns: &[u8], key: &str) -> bool {
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

    /*
     * Disabled
    /// maybe find start tag
    pub async fn maybe_tag_start(&mut self, ns: &[u8], key: &str) -> Result<Option<Event<'static>>, ParsingError> {
        println!("maybe start tag {}", key);
        let peek = self.peek();
        match peek {
            Event::Start(_) | Event::Empty(_) if self.is_tag(ns, key) => Ok(Some(self.next().await?)),
            _ => Ok(None),
        }
    }

    /// find start tag
    pub async fn tag_start(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        loop {
            match self.peek() {
                Event::Start(b) if self.is_tag(ns, key) => break,
                _ => { self.skip().await?; },
            }
        }
        self.next().await
    }
    */

    // find stop tag
    pub async fn tag_stop(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        println!("search stop tag {}", key);
        loop {
            match self.peek() {
                Event::End(b) if self.is_tag(ns, key) => break,
                _ => { self.skip().await?; },
            }
        }
        self.next().await
    }

    pub async fn tag_string(&mut self) -> Result<String, ParsingError> {
        let mut acc = String::new();
        loop {
            match self.peek() {
                Event::CData(unescaped) => {
                    acc.push_str(std::str::from_utf8(unescaped.as_ref())?);
                    self.next().await?
                },
                Event::Text(escaped) => {
                    acc.push_str(escaped.unescape()?.as_ref());
                    self.next().await?
                }
                Event::End(_) | Event::Start(_) | Event::Empty(_) => return Ok(acc),
                _ => self.next().await?,
            };
        }
    }

    // NEW API
    pub async fn maybe_read<N: Node<N>>(&mut self, t: &mut Option<N>, dirty: &mut bool) -> Result<(), ParsingError> {
        match N::qread(self).await {
            Ok(v) => { 
                *t = Some(v); 
                *dirty = true;
                Ok(()) 
            },
            Err(ParsingError::Recoverable) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn maybe_push<N: Node<N>>(&mut self, t: &mut Vec<N>, dirty: &mut bool) -> Result<(), ParsingError> {
        match N::qread(self).await {
            Ok(v) => { 
                t.push(v); 
                *dirty = true;
                Ok(()) 
            },
            Err(ParsingError::Recoverable) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn find<N: Node<N>>(&mut self) -> Result<N, ParsingError> {
        loop {
            // Try parse
            match N::qread(self).await {
                Err(ParsingError::Recoverable) => (),
                otherwise => return otherwise,
            }

            // If recovered, skip the element
            self.skip().await?;
        }
    }

    pub async fn maybe_find<N: Node<N>>(&mut self) -> Result<Option<N>, ParsingError> {
        loop {
            // Try parse
            match N::qread(self).await {
                Err(ParsingError::Recoverable) => (),
                otherwise => return otherwise.map(Some),
            }

            match self.peek() {
                Event::End(_) => return Ok(None),
                _ => self.skip().await?,
            };
        }
    }

    pub async fn collect<N: Node<N>>(&mut self) -> Result<Vec<N>, ParsingError> {
        let mut acc = Vec::new();
        loop {
            match N::qread(self).await {
                Err(ParsingError::Recoverable) => match self.peek() {
                    Event::End(_) => return Ok(acc),
                    _ => {
                        self.skip().await?;
                    },
                },
                Ok(v) => acc.push(v),
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn open(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        if self.is_tag(ns, key) {
            return self.next().await
        }
        return Err(ParsingError::Recoverable);
    }
}

