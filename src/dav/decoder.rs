use std::borrow::Cow;
use std::future::Future;

use quick_xml::events::{Event, BytesStart, BytesDecl, BytesText};
use quick_xml::events::attributes::AttrError;
use quick_xml::name::{Namespace, QName, PrefixDeclaration, ResolveResult, ResolveResult::*};
use quick_xml::reader::NsReader;
use tokio::io::AsyncBufRead;

use super::types::*;
use super::error::ParsingError;
use super::xml::{Node, QRead, Reader, IRead, DAV_URN, CAL_URN};

//@TODO (1) Rewrite all objects as Href,
// where we return Ok(None) instead of trying to find the object at any cost.
// Add a xml.find<E: Qread>() -> Result<Option<E>, ParsingError> or similar for the cases we
// really need the object
// (2) Rewrite QRead and replace Result<Option<_>, _> with Result<_, _>, not found being a possible
// error.
// (3) Rewrite vectors with xml.collect<E: QRead>() -> Result<Vec<E>, _>
// (4) Something for alternatives would be great but no idea yet

// ---- ROOT ----

/// Propfind request
impl<E: Extension> QRead<PropFind<E>> for PropFind<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        // Find propfind
        xml.open(DAV_URN, "propfind").await?;

        // Find any tag
        let propfind: PropFind<E> = loop {
            match xml.peek() {
                Event::Start(_) if xml.is_tag(DAV_URN, "allprop") => {
                    xml.open(DAV_URN, "allprop").await?;
                    let includ = xml.maybe_find::<Include<E>>().await?;
                    let r = PropFind::AllProp(includ);
                    xml.tag_stop(DAV_URN, "allprop").await?;
                    break r
                },
                Event::Start(_) if xml.is_tag(DAV_URN, "prop") => {
                    break PropFind::Prop(xml.find::<PropName<E>>().await?);
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

/// PROPPATCH request
impl<E: Extension> QRead<PropertyUpdate<E>> for PropertyUpdate<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "propertyupdate").await?;
        let collected_items = xml.collect::<PropertyUpdateItem<E>>().await?;
        xml.tag_stop(DAV_URN, "propertyupdate").await?;
        Ok(PropertyUpdate(collected_items))
    }
}

/// Generic response
impl<E: Extension, N: Node<N>> QRead<Multistatus<E,N>> for Multistatus<E,N> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "multistatus").await?;
        let mut responses = Vec::new();
        let mut responsedescription = None;

        loop {
            let mut dirty = false;
            xml.maybe_push(&mut responses, &mut dirty).await?;
            xml.maybe_read(&mut responsedescription, &mut dirty).await?;
            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.tag_stop(DAV_URN, "multistatus").await?;
        Ok(Multistatus { responses, responsedescription })
    }
}

// LOCK REQUEST
impl QRead<LockInfo> for LockInfo {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockinfo").await?;
        let (mut m_scope, mut m_type, mut owner) = (None, None, None);
        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut m_scope, &mut dirty).await?;
            xml.maybe_read::<LockType>(&mut m_type, &mut dirty).await?;
            xml.maybe_read::<Owner>(&mut owner, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        xml.tag_stop(DAV_URN, "lockinfo").await?;
        match (m_scope, m_type) {
            (Some(lockscope), Some(locktype)) => Ok(LockInfo { lockscope, locktype, owner }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

// LOCK RESPONSE
impl<E: Extension> QRead<PropValue<E>> for PropValue<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "prop").await?;
        let mut acc = xml.collect::<Property<E>>().await?;
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(PropValue(acc))
    }
}


/// Error response
impl<E: Extension> QRead<Error<E>> for Error<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "error").await?;
        let violations = xml.collect::<Violation<E>>().await?;
        xml.tag_stop(DAV_URN, "error").await?;
        Ok(Error(violations))
    }
}



// ---- INNER XML
impl<E: Extension, N: Node<N>> QRead<Response<E,N>> for Response<E,N> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "response").await?;
        let (mut status, mut error, mut responsedescription, mut location) = (None, None, None, None);
        let mut href = Vec::new();
        let mut propstat = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read::<Status>(&mut status, &mut dirty).await?;
            xml.maybe_push::<Href>(&mut href, &mut dirty).await?;
            xml.maybe_push::<PropStat<E,N>>(&mut propstat, &mut dirty).await?;
            xml.maybe_read::<Error<E>>(&mut error, &mut dirty).await?;
            xml.maybe_read::<ResponseDescription>(&mut responsedescription, &mut dirty).await?;
            xml.maybe_read::<Location>(&mut location, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => { xml.skip().await? },
                };
            }
        }

        xml.tag_stop(DAV_URN, "response").await?;
        match (status, &propstat[..], &href[..]) {
            (Some(status), &[], &[_, ..]) => Ok(Response { 
                status_or_propstat: StatusOrPropstat::Status(href, status), 
                error, responsedescription, location,
            }),
            (None, &[_, ..], &[_, ..]) => Ok(Response {
                status_or_propstat: StatusOrPropstat::PropStat(href.into_iter().next().unwrap(), propstat),
                error, responsedescription, location,
            }),
            (Some(_), &[_, ..], _) => Err(ParsingError::InvalidValue),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl<E: Extension, N: Node<N>> QRead<PropStat<E,N>> for PropStat<E,N> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "propstat").await?;

        let (mut m_prop, mut m_status, mut error, mut responsedescription) = (None, None, None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<N>(&mut m_prop, &mut dirty).await?;
            xml.maybe_read::<Status>(&mut m_status, &mut dirty).await?;
            xml.maybe_read::<Error<E>>(&mut error, &mut dirty).await?;
            xml.maybe_read::<ResponseDescription>(&mut responsedescription, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.tag_stop(DAV_URN, "propstat").await?;
        match (m_prop, m_status) {
            (Some(prop), Some(status)) => Ok(PropStat { prop, status, error, responsedescription }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<Status> for Status {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "status").await?;
        let fullcode = xml.tag_string().await?;
        let txtcode = fullcode.splitn(3, ' ').nth(1).ok_or(ParsingError::InvalidValue)?;
        let code = http::status::StatusCode::from_bytes(txtcode.as_bytes()).or(Err(ParsingError::InvalidValue))?;
        xml.tag_stop(DAV_URN, "status").await?;
        Ok(Status(code))
    }
}

impl QRead<ResponseDescription> for ResponseDescription {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "responsedescription").await?;
        let cnt = xml.tag_string().await?;
        xml.tag_stop(DAV_URN, "responsedescription").await?;
        Ok(ResponseDescription(cnt))
    }
}

impl QRead<Location> for Location {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "location").await?;
        let href = xml.find::<Href>().await?;
        xml.tag_stop(DAV_URN, "location").await?;
        Ok(Location(href))
    }
}

impl<E: Extension> QRead<PropertyUpdateItem<E>> for PropertyUpdateItem<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match Remove::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(PropertyUpdateItem::Remove),
        }
        Set::qread(xml).await.map(PropertyUpdateItem::Set)
    }
}

impl<E: Extension> QRead<Remove<E>> for Remove<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "remove").await?;
        let propname = xml.find::<PropName<E>>().await?;
        xml.tag_stop(DAV_URN, "remove").await?;
        Ok(Remove(propname))
    }
}

impl<E: Extension> QRead<Set<E>> for Set<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "set").await?;
        let propvalue = xml.find::<PropValue<E>>().await?;
        xml.tag_stop(DAV_URN, "set").await?;
        Ok(Set(propvalue))
    }
}

impl<E: Extension> QRead<Violation<E>> for Violation<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let bs = match xml.peek() {
            Event::Start(b) | Event::Empty(b) => b,
            _ => return Err(ParsingError::Recoverable),
        };

        // Option 1: a pure DAV property
        let (ns, loc) = xml.rdr.resolve_element(bs.name());
        if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
            match loc.into_inner() {
                b"lock-token-matches-request-uri" => {
                    xml.next().await?;
                    return Ok(Violation::LockTokenMatchesRequestUri)
                },
                b"lock-token-submitted" => {
                    xml.next().await?;
                    let links = xml.collect::<Href>().await?;
                    xml.tag_stop(DAV_URN, "lock-token-submitted").await?;
                    return Ok(Violation::LockTokenSubmitted(links))
                },
                b"no-conflicting-lock" => {
                    // start tag
                    xml.next().await?;
                    let links = xml.collect::<Href>().await?;
                    xml.tag_stop(DAV_URN, "no-conflicting-lock").await?;
                    return Ok(Violation::NoConflictingLock(links))
                },
                b"no-external-entities" => {
                    xml.next().await?;
                    return Ok(Violation::NoExternalEntities)
                },
                b"preserved-live-properties" => {
                    xml.next().await?;
                    return Ok(Violation::PreservedLiveProperties)
                },
                b"propfind-finite-depth" => {
                    xml.next().await?;
                    return Ok(Violation::PropfindFiniteDepth)
                },
                b"cannot-modify-protected-property" => {
                    xml.next().await?;
                    return Ok(Violation::CannotModifyProtectedProperty)
                },
                _ => (),
            };
        }

        // Option 2: an extension property, delegating
        E::Error::qread(xml).await.map(Violation::Extension)
    }
}

impl<E: Extension> QRead<Include<E>> for Include<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "include").await?;
        let acc = xml.collect::<PropertyRequest<E>>().await?;
        xml.tag_stop(DAV_URN, "include").await?;
        Ok(Include(acc))
    }
}

impl<E: Extension> QRead<PropName<E>> for PropName<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "prop").await?;
        let acc = xml.collect::<PropertyRequest<E>>().await?;
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(PropName(acc))
    }
}

impl<E: Extension> QRead<PropertyRequest<E>> for PropertyRequest<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let bs = match xml.peek() {
            Event::Start(b) | Event::Empty(b) => b,
            _ => return Err(ParsingError::Recoverable),
        };

        // Option 1: a pure core DAV property
        let (ns, loc) = xml.rdr.resolve_element(bs.name());
        if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
            let maybe_res = match loc.into_inner() {
                b"creationdate" => Some(PropertyRequest::CreationDate),
                b"displayname" => Some(PropertyRequest::DisplayName),
                b"getcontentlanguage" => Some(PropertyRequest::GetContentLanguage),
                b"getcontentlength" => Some(PropertyRequest::GetContentLength),
                b"getcontenttype" => Some(PropertyRequest::GetContentType),
                b"getetag" => Some(PropertyRequest::GetEtag),
                b"getlastmodified" => Some(PropertyRequest::GetLastModified),
                b"lockdiscovery" => Some(PropertyRequest::LockDiscovery),
                b"resourcetype" => Some(PropertyRequest::ResourceType),
                b"supportedlock" => Some(PropertyRequest::SupportedLock),
                _ => None,
            };
            // Close the current tag if we read something
            if let Some(res) = maybe_res {
                xml.skip().await?; 
                return Ok(res)
            }
        }

        // Option 2: an extension property, delegating
        E::PropertyRequest::qread(xml).await.map(PropertyRequest::Extension)
    }
}

impl<E: Extension> QRead<Property<E>> for Property<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        use chrono::{DateTime, FixedOffset, TimeZone};

        let bs = match xml.peek() {
            Event::Start(b) | Event::Empty(b) => b,
            _ => return Err(ParsingError::Recoverable),
        };

        // Option 1: a pure core DAV property
        let (ns, loc) = xml.rdr.resolve_element(bs.name());
        if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
            match loc.into_inner() {
                b"creationdate" => {
                    xml.next().await?;
                    let datestr = xml.tag_string().await?;
                    return Ok(Property::CreationDate(DateTime::parse_from_rfc3339(datestr.as_str())?))
                },
                b"displayname" => {
                    xml.next().await?;
                    return Ok(Property::DisplayName(xml.tag_string().await?))
                },
                b"getcontentlanguage" => {
                    xml.next().await?;
                    return Ok(Property::GetContentLanguage(xml.tag_string().await?))
                },
                b"getcontentlength" => {
                    xml.next().await?;
                    let cl = xml.tag_string().await?.parse::<u64>()?;
                    return Ok(Property::GetContentLength(cl))
                },
                b"getcontenttype" => {
                    xml.next().await?;
                    return Ok(Property::GetContentType(xml.tag_string().await?))
                },
                b"getetag" => {
                    xml.next().await?;
                    return Ok(Property::GetEtag(xml.tag_string().await?))
                },
                b"getlastmodified" => {
                    xml.next().await?;
                    let datestr = xml.tag_string().await?;
                    return Ok(Property::CreationDate(DateTime::parse_from_rfc2822(datestr.as_str())?))
                },
                b"lockdiscovery" => {
                    xml.next().await?;
                    let acc = xml.collect::<ActiveLock>().await?;
                    xml.tag_stop(DAV_URN, "lockdiscovery").await?;
                    return Ok(Property::LockDiscovery(acc))
                },
                b"resourcetype" => {
                    xml.next().await?;
                    let acc = xml.collect::<ResourceType<E>>().await?;
                    xml.tag_stop(DAV_URN, "resourcetype").await?;
                    return Ok(Property::ResourceType(acc))
                },
                b"supportedlock" => {
                    xml.next().await?;
                    let acc = xml.collect::<LockEntry>().await?;
                    xml.tag_stop(DAV_URN, "supportedlock").await?;
                    return Ok(Property::SupportedLock(acc))
                },
                _ => (),
            };
        }

        // Option 2: an extension property, delegating
        E::Property::qread(xml).await.map(Property::Extension)
    }
}

impl QRead<ActiveLock> for ActiveLock {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "activelock").await?;
        let (mut m_scope, mut m_type, mut m_depth, mut owner, mut timeout, mut locktoken, mut m_root) = 
            (None, None, None, None, None, None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut m_scope, &mut dirty).await?;
            xml.maybe_read::<LockType>(&mut m_type, &mut dirty).await?;
            xml.maybe_read::<Depth>(&mut m_depth, &mut dirty).await?;
            xml.maybe_read::<Owner>(&mut owner, &mut dirty).await?;
            xml.maybe_read::<Timeout>(&mut timeout, &mut dirty).await?;
            xml.maybe_read::<LockToken>(&mut locktoken, &mut dirty).await?;
            xml.maybe_read::<LockRoot>(&mut m_root, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => { xml.skip().await?; },
                }
            }
        }

        xml.tag_stop(DAV_URN, "activelock").await?;
        match (m_scope, m_type, m_depth, m_root) {
            (Some(lockscope), Some(locktype), Some(depth), Some(lockroot)) =>
                Ok(ActiveLock { lockscope, locktype, depth, owner, timeout, locktoken, lockroot }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<Depth> for Depth {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "depth").await?;
        let depth_str = xml.tag_string().await?;
        xml.tag_stop(DAV_URN, "depth").await?;
        match depth_str.as_str() {
            "0" => Ok(Depth::Zero),
            "1" => Ok(Depth::One),
            "infinity" => Ok(Depth::Infinity),
            _ => Err(ParsingError::WrongToken),
        }
    }
}

impl QRead<Owner> for Owner {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "owner").await?;
        
        let mut owner = Owner::Unknown;
        loop {
            match xml.peek() {
                Event::Text(_) | Event::CData(_) => {
                    let txt = xml.tag_string().await?;
                    if matches!(owner, Owner::Unknown) {
                        owner = Owner::Txt(txt);
                    }
                }
                Event::Start(_) | Event::Empty(_) => {
                    match Href::qread(xml).await {
                        Ok(href) => { owner = Owner::Href(href); },
                        Err(ParsingError::Recoverable) => { xml.skip().await?; },
                        Err(e) => return Err(e),
                    }
                }
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        };
        xml.tag_stop(DAV_URN, "owner").await?;
        Ok(owner)
    }
}

impl QRead<Timeout> for Timeout {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        const SEC_PFX: &str = "SEC_PFX";
        xml.open(DAV_URN, "timeout").await?;
        
        let timeout = match xml.tag_string().await?.as_str() {
            "Infinite" => Timeout::Infinite,
            seconds => match seconds.strip_prefix(SEC_PFX) {
                Some(secs) => Timeout::Seconds(secs.parse::<u32>()?),
                None => return Err(ParsingError::InvalidValue),
            },
        };

        xml.tag_stop(DAV_URN, "timeout").await?;
        Ok(timeout)
    }
}

impl QRead<LockToken> for LockToken {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "locktoken").await?;
        let href = Href::qread(xml).await?;
        xml.tag_stop(DAV_URN, "locktoken").await?;
        Ok(LockToken(href))
    }
}

impl QRead<LockRoot> for LockRoot {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockroot").await?;
        let href = Href::qread(xml).await?;
        xml.tag_stop(DAV_URN, "lockroot").await?;
        Ok(LockRoot(href))
    }
}

impl<E: Extension> QRead<ResourceType<E>> for ResourceType<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match xml.peek() {
            Event::Empty(b) if xml.is_tag(DAV_URN, "collection") => {
                xml.next().await?;
                Ok(ResourceType::Collection)
            },
            _ => E::ResourceType::qread(xml).await.map(ResourceType::Extension),
        }
    }
}

impl QRead<LockEntry> for LockEntry {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockentry").await?;
        let (mut maybe_scope, mut maybe_type) = (None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut maybe_scope, &mut dirty).await?;
            xml.maybe_read::<LockType>(&mut maybe_type, &mut dirty).await?;
            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.tag_stop(DAV_URN, "lockentry").await?;
        match (maybe_scope, maybe_type) {
            (Some(lockscope), Some(locktype)) => Ok(LockEntry { lockscope, locktype }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<LockScope> for LockScope {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockscope").await?;

        let lockscope = loop {
            match xml.peek() {
                Event::Empty(_) if xml.is_tag(DAV_URN, "exclusive") => {
                    xml.next().await?;
                    break LockScope::Exclusive
                },
                Event::Empty(_) if xml.is_tag(DAV_URN, "shared") => {
                    xml.next().await?;
                    break LockScope::Shared
                }
                _ => xml.skip().await?,
            };
        };

        xml.tag_stop(DAV_URN, "lockscope").await?;
        Ok(lockscope)
    }
}

impl QRead<LockType> for LockType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "locktype").await?;

        let locktype = loop {
            match xml.peek() {
                Event::Empty(b) if xml.is_tag(DAV_URN, "write") => {
                    xml.next().await?;
                    break LockType::Write
                }
                _ => xml.skip().await?,
            };
        };
        xml.tag_stop(DAV_URN, "locktype").await?;
        Ok(locktype)
    }
}

impl QRead<Href> for Href {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "href").await?;
        let mut url = xml.tag_string().await?;
        xml.tag_stop(DAV_URN, "href").await?;
        Ok(Href(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dav::realization::Core;

    #[tokio::test]
    async fn basic_propfind_propname() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
<rando/>
<garbage><old/></garbage>
<D:propfind xmlns:D="DAV:">
    <D:propname/>
</D:propfind>
"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<PropFind::<Core>>().await.unwrap();

        assert_eq!(got, PropFind::<Core>::PropName);
    }

    #[tokio::test]
    async fn basic_propfind_prop() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
<rando/>
<garbage><old/></garbage>
<D:propfind xmlns:D="DAV:">
    <D:prop>
        <D:displayname/>
        <D:getcontentlength/>
        <D:getcontenttype/>
        <D:getetag/>
        <D:getlastmodified/>
        <D:resourcetype/>
        <D:supportedlock/>
    </D:prop>
</D:propfind>
"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<PropFind::<Core>>().await.unwrap();

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

    #[tokio::test]
    async fn rfc_lock_error() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
     <D:error xmlns:D="DAV:">
       <D:lock-token-submitted>
         <D:href>/locked/</D:href>
       </D:lock-token-submitted>
     </D:error>"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<Error::<Core>>().await.unwrap();

        assert_eq!(got, Error(vec![
            Violation::LockTokenSubmitted(vec![
                Href("/locked/".into())
            ])
        ]));
    }


    #[tokio::test]
    async fn rfc_propertyupdate() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
     <D:propertyupdate xmlns:D="DAV:"
             xmlns:Z="http://ns.example.com/standards/z39.50/">
       <D:set>
         <D:prop>
           <Z:Authors>
             <Z:Author>Jim Whitehead</Z:Author>
             <Z:Author>Roy Fielding</Z:Author>
           </Z:Authors>
         </D:prop>
       </D:set>
       <D:remove>
         <D:prop><Z:Copyright-Owner/></D:prop>
       </D:remove>
     </D:propertyupdate>"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<PropertyUpdate::<Core>>().await.unwrap();

        assert_eq!(got, PropertyUpdate(vec![
            PropertyUpdateItem::Set(Set(PropValue(vec![]))),
            PropertyUpdateItem::Remove(Remove(PropName(vec![]))),
        ]));
    }

    #[tokio::test]
    async fn rfc_lockinfo() {
        let src = r#"
<?xml version="1.0" encoding="utf-8" ?>
<D:lockinfo xmlns:D='DAV:'>
    <D:lockscope><D:exclusive/></D:lockscope>
    <D:locktype><D:write/></D:locktype>
    <D:owner>
        <D:href>http://example.org/~ejw/contact.html</D:href>
    </D:owner>
</D:lockinfo>
"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<LockInfo>().await.unwrap();

        assert_eq!(got, LockInfo {
            lockscope: LockScope::Exclusive,
            locktype: LockType::Write,
            owner: Some(Owner::Href(Href("http://example.org/~ejw/contact.html".into()))),
        });
    }

    #[tokio::test]
    async fn rfc_multistatus_name() {
        let src = r#"
<?xml version="1.0" encoding="utf-8" ?>
    <multistatus xmlns="DAV:">
       <response>
         <href>http://www.example.com/container/</href>
         <propstat>
           <prop xmlns:R="http://ns.example.com/boxschema/">
             <R:bigbox/>
             <R:author/>
             <creationdate/>
             <displayname/>
             <resourcetype/>
             <supportedlock/>
           </prop>
           <status>HTTP/1.1 200 OK</status>
         </propstat>
       </response>
       <response>
         <href>http://www.example.com/container/front.html</href>
         <propstat>
           <prop xmlns:R="http://ns.example.com/boxschema/">
             <R:bigbox/>
             <creationdate/>
             <displayname/>
             <getcontentlength/>
             <getcontenttype/>
             <getetag/>
             <getlastmodified/>
             <resourcetype/>
             <supportedlock/>
           </prop>
           <status>HTTP/1.1 200 OK</status>
         </propstat>
       </response>
     </multistatus>
"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes())).await.unwrap();
        let got = rdr.find::<Multistatus::<Core, PropName<Core>>>().await.unwrap();

        /*assert_eq!(got, Multistatus {
            responses: vec![
                Response {
                    status_or_propstat: 
                },
                Response {},
            ],
            responsedescription: None,
        });*/

    }

}
