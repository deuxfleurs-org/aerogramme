use futures::Future;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use tokio::io::{AsyncBufRead, AsyncWrite};

use super::error::ParsingError;

// Constants
pub const DAV_URN: &[u8] = b"DAV:";
pub const CAL_URN: &[u8] = b"urn:ietf:params:xml:ns:caldav";
pub const CARD_URN: &[u8] = b"urn:ietf:params:xml:ns:carddav";

// Async traits
pub trait IWrite = AsyncWrite + Unpin + Send;
pub trait IRead = AsyncBufRead + Unpin;

// Serialization/Deserialization traits
pub trait QWrite {
    fn qwrite(
        &self,
        xml: &mut Writer<impl IWrite>,
    ) -> impl Future<Output = Result<(), quick_xml::Error>> + Send;
}
pub trait QRead<T> {
    fn qread(xml: &mut Reader<impl IRead>) -> impl Future<Output = Result<T, ParsingError>>;
}

// The representation of an XML node in Rust
pub trait Node<T> = QRead<T> + QWrite + std::fmt::Debug + PartialEq + Clone + Sync;

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
            start.extend_attributes(
                self.ns_to_apply
                    .iter()
                    .map(|(k, n)| (k.as_str(), n.as_str())),
            );
            self.ns_to_apply.clear()
        }
        start
    }
}

/// Transform an XML stream of characters into a Rust object
pub struct Reader<T: IRead> {
    pub rdr: NsReader<T>,
    cur: Event<'static>,
    prev: Event<'static>,
    parents: Vec<Event<'static>>,
    buf: Vec<u8>,
}
impl<T: IRead> Reader<T> {
    pub async fn new(mut rdr: NsReader<T>) -> Result<Self, ParsingError> {
        let mut buf: Vec<u8> = vec![];
        let cur = rdr.read_event_into_async(&mut buf).await?.into_owned();
        let parents = vec![];
        let prev = Event::Eof;
        buf.clear();
        Ok(Self {
            cur,
            prev,
            parents,
            rdr,
            buf,
        })
    }

    /// read one more tag
    /// do not expose it publicly
    async fn next(&mut self) -> Result<Event<'static>, ParsingError> {
        let evt = self
            .rdr
            .read_event_into_async(&mut self.buf)
            .await?
            .into_owned();
        self.buf.clear();
        self.prev = std::mem::replace(&mut self.cur, evt);
        Ok(self.prev.clone())
    }

    /// skip a node at current level
    /// I would like to make this one private but not ready
    pub async fn skip(&mut self) -> Result<Event<'static>, ParsingError> {
        //println!("skipping inside node {:?} value {:?}", self.parents.last(), self.cur);
        match &self.cur {
            Event::Start(b) => {
                let _span = self
                    .rdr
                    .read_to_end_into_async(b.to_end().name(), &mut self.buf)
                    .await?;
                self.next().await
            }
            Event::End(_) => Err(ParsingError::WrongToken),
            Event::Eof => Err(ParsingError::Eof),
            _ => self.next().await,
        }
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
            return false;
        }

        match extr_ns {
            ResolveResult::Bound(v) => v.into_inner() == ns,
            _ => false,
        }
    }

    pub fn parent_has_child(&self) -> bool {
        matches!(self.parents.last(), Some(Event::Start(_)) | None)
    }

    fn ensure_parent_has_child(&self) -> Result<(), ParsingError> {
        match self.parent_has_child() {
            true => Ok(()),
            false => Err(ParsingError::Recoverable),
        }
    }

    pub fn peek(&self) -> &Event<'static> {
        &self.cur
    }

    pub fn previous(&self) -> &Event<'static> {
        &self.prev
    }

    // NEW API
    pub async fn tag_string(&mut self) -> Result<String, ParsingError> {
        self.ensure_parent_has_child()?;

        let mut acc = String::new();
        loop {
            match self.peek() {
                Event::CData(unescaped) => {
                    acc.push_str(std::str::from_utf8(unescaped.as_ref())?);
                    self.next().await?
                }
                Event::Text(escaped) => {
                    acc.push_str(escaped.unescape()?.as_ref());
                    self.next().await?
                }
                Event::End(_) | Event::Start(_) | Event::Empty(_) => return Ok(acc),
                _ => self.next().await?,
            };
        }
    }

    pub async fn maybe_read<N: Node<N>>(
        &mut self,
        t: &mut Option<N>,
        dirty: &mut bool,
    ) -> Result<(), ParsingError> {
        if !self.parent_has_child() {
            return Ok(());
        }

        match N::qread(self).await {
            Ok(v) => {
                *t = Some(v);
                *dirty = true;
                Ok(())
            }
            Err(ParsingError::Recoverable) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn maybe_push<N: Node<N>>(
        &mut self,
        t: &mut Vec<N>,
        dirty: &mut bool,
    ) -> Result<(), ParsingError> {
        if !self.parent_has_child() {
            return Ok(());
        }

        match N::qread(self).await {
            Ok(v) => {
                t.push(v);
                *dirty = true;
                Ok(())
            }
            Err(ParsingError::Recoverable) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn find<N: Node<N>>(&mut self) -> Result<N, ParsingError> {
        self.ensure_parent_has_child()?;

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
        // We can't find anything inside a self-closed tag
        if !self.parent_has_child() {
            return Ok(None);
        }

        loop {
            // Try parse
            match N::qread(self).await {
                Err(ParsingError::Recoverable) => (),
                otherwise => return otherwise.map(Some),
            }

            // Skip or stop
            match self.peek() {
                Event::End(_) => return Ok(None),
                _ => self.skip().await?,
            };
        }
    }

    pub async fn collect<N: Node<N>>(&mut self) -> Result<Vec<N>, ParsingError> {
        let mut acc = Vec::new();
        if !self.parent_has_child() {
            return Ok(acc);
        }

        loop {
            match N::qread(self).await {
                Err(ParsingError::Recoverable) => match self.peek() {
                    Event::End(_) => return Ok(acc),
                    _ => {
                        self.skip().await?;
                    }
                },
                Ok(v) => acc.push(v),
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn open(&mut self, ns: &[u8], key: &str) -> Result<Event<'static>, ParsingError> {
        //println!("try open tag {:?}, on {:?}", key, self.peek());
        let evt = match self.peek() {
            Event::Empty(_) if self.is_tag(ns, key) => {
                // hack to make `prev_attr` works
                // here we duplicate the current tag
                // as in other words, we virtually moved one token
                // which is useful for prev_attr and any logic based on
                // self.prev + self.open() on empty nodes
                self.prev = self.cur.clone();
                self.cur.clone()
            }
            Event::Start(_) if self.is_tag(ns, key) => self.next().await?,
            _ => return Err(ParsingError::Recoverable),
        };

        //println!("open tag {:?}", evt);
        self.parents.push(evt.clone());
        Ok(evt)
    }

    pub async fn open_start(
        &mut self,
        ns: &[u8],
        key: &str,
    ) -> Result<Event<'static>, ParsingError> {
        //println!("try open start tag {:?}, on {:?}", key, self.peek());
        let evt = match self.peek() {
            Event::Start(_) if self.is_tag(ns, key) => self.next().await?,
            _ => return Err(ParsingError::Recoverable),
        };

        //println!("open start tag {:?}", evt);
        self.parents.push(evt.clone());
        Ok(evt)
    }

    pub async fn maybe_open(
        &mut self,
        ns: &[u8],
        key: &str,
    ) -> Result<Option<Event<'static>>, ParsingError> {
        match self.open(ns, key).await {
            Ok(v) => Ok(Some(v)),
            Err(ParsingError::Recoverable) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn maybe_open_start(
        &mut self,
        ns: &[u8],
        key: &str,
    ) -> Result<Option<Event<'static>>, ParsingError> {
        match self.open_start(ns, key).await {
            Ok(v) => Ok(Some(v)),
            Err(ParsingError::Recoverable) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn prev_attr(&self, attr: &str) -> Option<String> {
        match &self.prev {
            Event::Start(bs) | Event::Empty(bs) => match bs.try_get_attribute(attr) {
                Ok(Some(attr)) => attr
                    .decode_and_unescape_value(&self.rdr)
                    .ok()
                    .map(|v| v.into_owned()),
                _ => None,
            },
            _ => None,
        }
    }

    // find stop tag
    pub async fn close(&mut self) -> Result<Event<'static>, ParsingError> {
        //println!("close tag {:?}", self.parents.last());

        // Handle the empty case
        if !self.parent_has_child() {
            self.parents.pop();
            return self.next().await;
        }

        // Handle the start/end case
        loop {
            match self.peek() {
                Event::End(_) => {
                    self.parents.pop();
                    return self.next().await;
                }
                _ => self.skip().await?,
            };
        }
    }
}
