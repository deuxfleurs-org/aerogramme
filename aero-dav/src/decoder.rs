use chrono::DateTime;
use quick_xml::events::Event;

use super::error::ParsingError;
use super::types::*;
use super::xml::{IRead, Node, QRead, Reader, DAV_URN};

//@TODO (1) Rewrite all objects as Href,
// where we return Ok(None) instead of trying to find the object at any cost.
// Add a xml.find<E: Qread>() -> Result<Option<E>, ParsingError> or similar for the cases we
// really need the object
// (2) Rewrite QRead and replace Result<Option<_>, _> with Result<_, _>, not found being a possible
// error.
// (3) Rewrite vectors with xml.collect<E: QRead>() -> Result<Vec<E>, _>
// (4) Something for alternatives like xml::choices on some lib would be great but no idea yet

// ---- ROOT ----

/// Propfind request
impl<E: Extension> QRead<PropFind<E>> for PropFind<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "propfind").await?;
        let propfind: PropFind<E> = loop {
            // allprop
            if let Some(_) = xml.maybe_open(DAV_URN, "allprop").await? {
                xml.close().await?;
                let includ = xml.maybe_find::<Include<E>>().await?;
                break PropFind::AllProp(includ);
            }

            // propname
            if let Some(_) = xml.maybe_open(DAV_URN, "propname").await? {
                xml.close().await?;
                break PropFind::PropName;
            }

            // prop
            let (mut maybe_prop, mut dirty) = (None, false);
            xml.maybe_read::<PropName<E>>(&mut maybe_prop, &mut dirty)
                .await?;
            if let Some(prop) = maybe_prop {
                break PropFind::Prop(prop);
            }

            // not found, skipping
            xml.skip().await?;
        };
        xml.close().await?;

        Ok(propfind)
    }
}

/// PROPPATCH request
impl<E: Extension> QRead<PropertyUpdate<E>> for PropertyUpdate<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "propertyupdate").await?;
        let collected_items = xml.collect::<PropertyUpdateItem<E>>().await?;
        xml.close().await?;
        Ok(PropertyUpdate(collected_items))
    }
}

/// Generic response
impl<E: Extension> QRead<Multistatus<E>> for Multistatus<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "multistatus").await?;
        let mut responses = Vec::new();
        let mut responsedescription = None;
        let mut extension = None;

        loop {
            let mut dirty = false;
            xml.maybe_push(&mut responses, &mut dirty).await?;
            xml.maybe_read(&mut responsedescription, &mut dirty).await?;
            xml.maybe_read(&mut extension, &mut dirty).await?;
            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        Ok(Multistatus {
            responses,
            responsedescription,
            extension,
        })
    }
}

// LOCK REQUEST
impl QRead<LockInfo> for LockInfo {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockinfo").await?;
        let (mut m_scope, mut m_type, mut owner) = (None, None, None);
        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut m_scope, &mut dirty)
                .await?;
            xml.maybe_read::<LockType>(&mut m_type, &mut dirty).await?;
            xml.maybe_read::<Owner>(&mut owner, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        xml.close().await?;
        match (m_scope, m_type) {
            (Some(lockscope), Some(locktype)) => Ok(LockInfo {
                lockscope,
                locktype,
                owner,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

// LOCK RESPONSE
impl<E: Extension> QRead<PropValue<E>> for PropValue<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        println!("---- propvalue");
        xml.open(DAV_URN, "prop").await?;
        let acc = xml.collect::<Property<E>>().await?;
        xml.close().await?;
        Ok(PropValue(acc))
    }
}

/// Error response
impl<E: Extension> QRead<Error<E>> for Error<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "error").await?;
        let violations = xml.collect::<Violation<E>>().await?;
        xml.close().await?;
        Ok(Error(violations))
    }
}

// ---- INNER XML
impl<E: Extension> QRead<Response<E>> for Response<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "response").await?;
        let (mut status, mut error, mut responsedescription, mut location) =
            (None, None, None, None);
        let mut href = Vec::new();
        let mut propstat = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read::<Status>(&mut status, &mut dirty).await?;
            xml.maybe_push::<Href>(&mut href, &mut dirty).await?;
            xml.maybe_push::<PropStat<E>>(&mut propstat, &mut dirty)
                .await?;
            xml.maybe_read::<Error<E>>(&mut error, &mut dirty).await?;
            xml.maybe_read::<ResponseDescription>(&mut responsedescription, &mut dirty)
                .await?;
            xml.maybe_read::<Location>(&mut location, &mut dirty)
                .await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        match (status, &propstat[..], &href[..]) {
            (Some(status), &[], &[_, ..]) => Ok(Response {
                status_or_propstat: StatusOrPropstat::Status(href, status),
                error,
                responsedescription,
                location,
            }),
            (None, &[_, ..], &[_, ..]) => Ok(Response {
                status_or_propstat: StatusOrPropstat::PropStat(
                    href.into_iter().next().unwrap(),
                    propstat,
                ),
                error,
                responsedescription,
                location,
            }),
            (Some(_), &[_, ..], _) => Err(ParsingError::InvalidValue),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl<E: Extension> QRead<PropStat<E>> for PropStat<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "propstat").await?;

        let (mut m_any_prop, mut m_status, mut error, mut responsedescription) =
            (None, None, None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<AnyProp<E>>(&mut m_any_prop, &mut dirty)
                .await?;
            xml.maybe_read::<Status>(&mut m_status, &mut dirty).await?;
            xml.maybe_read::<Error<E>>(&mut error, &mut dirty).await?;
            xml.maybe_read::<ResponseDescription>(&mut responsedescription, &mut dirty)
                .await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        match (m_any_prop, m_status) {
            (Some(prop), Some(status)) => Ok(PropStat {
                prop,
                status,
                error,
                responsedescription,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<Status> for Status {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "status").await?;
        let fullcode = xml.tag_string().await?;
        let txtcode = fullcode
            .splitn(3, ' ')
            .nth(1)
            .ok_or(ParsingError::InvalidValue)?;
        let code = http::status::StatusCode::from_bytes(txtcode.as_bytes())
            .or(Err(ParsingError::InvalidValue))?;
        xml.close().await?;
        Ok(Status(code))
    }
}

impl QRead<ResponseDescription> for ResponseDescription {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "responsedescription").await?;
        let cnt = xml.tag_string().await?;
        xml.close().await?;
        Ok(ResponseDescription(cnt))
    }
}

impl QRead<Location> for Location {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "location").await?;
        let href = xml.find::<Href>().await?;
        xml.close().await?;
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
        xml.close().await?;
        Ok(Remove(propname))
    }
}

impl<E: Extension> QRead<Set<E>> for Set<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "set").await?;
        let propvalue = xml.find::<PropValue<E>>().await?;
        xml.close().await?;
        Ok(Set(propvalue))
    }
}

impl<E: Extension> QRead<Violation<E>> for Violation<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(DAV_URN, "lock-token-matches-request-uri")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Violation::LockTokenMatchesRequestUri)
        } else if xml
            .maybe_open(DAV_URN, "lock-token-submitted")
            .await?
            .is_some()
        {
            let links = xml.collect::<Href>().await?;
            xml.close().await?;
            Ok(Violation::LockTokenSubmitted(links))
        } else if xml
            .maybe_open(DAV_URN, "no-conflicting-lock")
            .await?
            .is_some()
        {
            let links = xml.collect::<Href>().await?;
            xml.close().await?;
            Ok(Violation::NoConflictingLock(links))
        } else if xml
            .maybe_open(DAV_URN, "no-external-entities")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Violation::NoExternalEntities)
        } else if xml
            .maybe_open(DAV_URN, "preserved-live-properties")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Violation::PreservedLiveProperties)
        } else if xml
            .maybe_open(DAV_URN, "propfind-finite-depth")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Violation::PropfindFiniteDepth)
        } else if xml
            .maybe_open(DAV_URN, "cannot-modify-protected-property")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Violation::CannotModifyProtectedProperty)
        } else {
            E::Error::qread(xml).await.map(Violation::Extension)
        }
    }
}

impl<E: Extension> QRead<Include<E>> for Include<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "include").await?;
        let acc = xml.collect::<PropertyRequest<E>>().await?;
        xml.close().await?;
        Ok(Include(acc))
    }
}

impl<E: Extension> QRead<PropName<E>> for PropName<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "prop").await?;
        let acc = xml.collect::<PropertyRequest<E>>().await?;
        xml.close().await?;
        Ok(PropName(acc))
    }
}

impl<E: Extension> QRead<AnyProp<E>> for AnyProp<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "prop").await?;
        let acc = xml.collect::<AnyProperty<E>>().await?;
        xml.close().await?;
        Ok(AnyProp(acc))
    }
}

impl<E: Extension> QRead<AnyProperty<E>> for AnyProperty<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match Property::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Self::Value),
        }
        PropertyRequest::qread(xml).await.map(Self::Request)
    }
}

impl<E: Extension> QRead<PropertyRequest<E>> for PropertyRequest<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let maybe = if xml.maybe_open(DAV_URN, "creationdate").await?.is_some() {
            Some(PropertyRequest::CreationDate)
        } else if xml.maybe_open(DAV_URN, "displayname").await?.is_some() {
            Some(PropertyRequest::DisplayName)
        } else if xml
            .maybe_open(DAV_URN, "getcontentlanguage")
            .await?
            .is_some()
        {
            Some(PropertyRequest::GetContentLanguage)
        } else if xml.maybe_open(DAV_URN, "getcontentlength").await?.is_some() {
            Some(PropertyRequest::GetContentLength)
        } else if xml.maybe_open(DAV_URN, "getcontenttype").await?.is_some() {
            Some(PropertyRequest::GetContentType)
        } else if xml.maybe_open(DAV_URN, "getetag").await?.is_some() {
            Some(PropertyRequest::GetEtag)
        } else if xml.maybe_open(DAV_URN, "getlastmodified").await?.is_some() {
            Some(PropertyRequest::GetLastModified)
        } else if xml.maybe_open(DAV_URN, "lockdiscovery").await?.is_some() {
            Some(PropertyRequest::LockDiscovery)
        } else if xml.maybe_open(DAV_URN, "resourcetype").await?.is_some() {
            Some(PropertyRequest::ResourceType)
        } else if xml.maybe_open(DAV_URN, "supportedlock").await?.is_some() {
            Some(PropertyRequest::SupportedLock)
        } else {
            None
        };

        match maybe {
            Some(pr) => {
                xml.close().await?;
                Ok(pr)
            }
            None => E::PropertyRequest::qread(xml)
                .await
                .map(PropertyRequest::Extension),
        }
    }
}

impl<E: Extension> QRead<Property<E>> for Property<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        // Core WebDAV properties
        if xml
            .maybe_open_start(DAV_URN, "creationdate")
            .await?
            .is_some()
        {
            let datestr = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::CreationDate(DateTime::parse_from_rfc3339(
                datestr.as_str(),
            )?));
        } else if xml
            .maybe_open_start(DAV_URN, "displayname")
            .await?
            .is_some()
        {
            let name = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::DisplayName(name));
        } else if xml
            .maybe_open_start(DAV_URN, "getcontentlanguage")
            .await?
            .is_some()
        {
            let lang = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::GetContentLanguage(lang));
        } else if xml
            .maybe_open_start(DAV_URN, "getcontentlength")
            .await?
            .is_some()
        {
            let cl = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::GetContentLength(cl));
        } else if xml
            .maybe_open_start(DAV_URN, "getcontenttype")
            .await?
            .is_some()
        {
            let ct = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::GetContentType(ct));
        } else if xml.maybe_open_start(DAV_URN, "getetag").await?.is_some() {
            let etag = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::GetEtag(etag));
        } else if xml
            .maybe_open_start(DAV_URN, "getlastmodified")
            .await?
            .is_some()
        {
            let datestr = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::GetLastModified(DateTime::parse_from_rfc2822(
                datestr.as_str(),
            )?));
        } else if xml
            .maybe_open_start(DAV_URN, "lockdiscovery")
            .await?
            .is_some()
        {
            let acc = xml.collect::<ActiveLock>().await?;
            xml.close().await?;
            return Ok(Property::LockDiscovery(acc));
        } else if xml
            .maybe_open_start(DAV_URN, "resourcetype")
            .await?
            .is_some()
        {
            let acc = xml.collect::<ResourceType<E>>().await?;
            xml.close().await?;
            return Ok(Property::ResourceType(acc));
        } else if xml
            .maybe_open_start(DAV_URN, "supportedlock")
            .await?
            .is_some()
        {
            let acc = xml.collect::<LockEntry>().await?;
            xml.close().await?;
            return Ok(Property::SupportedLock(acc));
        }

        // Option 2: an extension property, delegating
        E::Property::qread(xml).await.map(Property::Extension)
    }
}

impl QRead<ActiveLock> for ActiveLock {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "activelock").await?;
        let (
            mut m_scope,
            mut m_type,
            mut m_depth,
            mut owner,
            mut timeout,
            mut locktoken,
            mut m_root,
        ) = (None, None, None, None, None, None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut m_scope, &mut dirty)
                .await?;
            xml.maybe_read::<LockType>(&mut m_type, &mut dirty).await?;
            xml.maybe_read::<Depth>(&mut m_depth, &mut dirty).await?;
            xml.maybe_read::<Owner>(&mut owner, &mut dirty).await?;
            xml.maybe_read::<Timeout>(&mut timeout, &mut dirty).await?;
            xml.maybe_read::<LockToken>(&mut locktoken, &mut dirty)
                .await?;
            xml.maybe_read::<LockRoot>(&mut m_root, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => {
                        xml.skip().await?;
                    }
                }
            }
        }

        xml.close().await?;
        match (m_scope, m_type, m_depth, m_root) {
            (Some(lockscope), Some(locktype), Some(depth), Some(lockroot)) => Ok(ActiveLock {
                lockscope,
                locktype,
                depth,
                owner,
                timeout,
                locktoken,
                lockroot,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<Depth> for Depth {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "depth").await?;
        let depth_str = xml.tag_string().await?;
        xml.close().await?;
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
                Event::Start(_) | Event::Empty(_) => match Href::qread(xml).await {
                    Ok(href) => {
                        owner = Owner::Href(href);
                    }
                    Err(ParsingError::Recoverable) => {
                        xml.skip().await?;
                    }
                    Err(e) => return Err(e),
                },
                Event::End(_) => break,
                _ => {
                    xml.skip().await?;
                }
            }
        }
        xml.close().await?;
        Ok(owner)
    }
}

impl QRead<Timeout> for Timeout {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        const SEC_PFX: &str = "Second-";
        xml.open(DAV_URN, "timeout").await?;

        let timeout = match xml.tag_string().await?.as_str() {
            "Infinite" => Timeout::Infinite,
            seconds => match seconds.strip_prefix(SEC_PFX) {
                Some(secs) => Timeout::Seconds(secs.parse::<u32>()?),
                None => return Err(ParsingError::InvalidValue),
            },
        };

        xml.close().await?;
        Ok(timeout)
    }
}

impl QRead<LockToken> for LockToken {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "locktoken").await?;
        let href = xml.find::<Href>().await?;
        xml.close().await?;
        Ok(LockToken(href))
    }
}

impl QRead<LockRoot> for LockRoot {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockroot").await?;
        let href = xml.find::<Href>().await?;
        xml.close().await?;
        Ok(LockRoot(href))
    }
}

impl<E: Extension> QRead<ResourceType<E>> for ResourceType<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "collection").await?.is_some() {
            xml.close().await?;
            return Ok(ResourceType::Collection);
        }

        E::ResourceType::qread(xml)
            .await
            .map(ResourceType::Extension)
    }
}

impl QRead<LockEntry> for LockEntry {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockentry").await?;
        let (mut maybe_scope, mut maybe_type) = (None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read::<LockScope>(&mut maybe_scope, &mut dirty)
                .await?;
            xml.maybe_read::<LockType>(&mut maybe_type, &mut dirty)
                .await?;
            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        match (maybe_scope, maybe_type) {
            (Some(lockscope), Some(locktype)) => Ok(LockEntry {
                lockscope,
                locktype,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<LockScope> for LockScope {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "lockscope").await?;

        let lockscope = loop {
            if xml.maybe_open(DAV_URN, "exclusive").await?.is_some() {
                xml.close().await?;
                break LockScope::Exclusive;
            }

            if xml.maybe_open(DAV_URN, "shared").await?.is_some() {
                xml.close().await?;
                break LockScope::Shared;
            }

            xml.skip().await?;
        };

        xml.close().await?;
        Ok(lockscope)
    }
}

impl QRead<LockType> for LockType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "locktype").await?;

        let locktype = loop {
            if xml.maybe_open(DAV_URN, "write").await?.is_some() {
                xml.close().await?;
                break LockType::Write;
            }

            xml.skip().await?;
        };

        xml.close().await?;
        Ok(locktype)
    }
}

impl QRead<Href> for Href {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "href").await?;
        let url = xml.tag_string().await?;
        xml.close().await?;
        Ok(Href(url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::Core;
    use chrono::{FixedOffset, TimeZone};
    use quick_xml::reader::NsReader;

    #[tokio::test]
    async fn basic_propfind_propname() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
<rando/>
<garbage><old/></garbage>
<D:propfind xmlns:D="DAV:">
    <D:propname/>
</D:propfind>
"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<PropFind<Core>>().await.unwrap();

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

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<PropFind<Core>>().await.unwrap();

        assert_eq!(
            got,
            PropFind::Prop(PropName(vec![
                PropertyRequest::DisplayName,
                PropertyRequest::GetContentLength,
                PropertyRequest::GetContentType,
                PropertyRequest::GetEtag,
                PropertyRequest::GetLastModified,
                PropertyRequest::ResourceType,
                PropertyRequest::SupportedLock,
            ]))
        );
    }

    #[tokio::test]
    async fn rfc_lock_error() {
        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
     <D:error xmlns:D="DAV:">
       <D:lock-token-submitted>
         <D:href>/locked/</D:href>
       </D:lock-token-submitted>
     </D:error>"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<Error<Core>>().await.unwrap();

        assert_eq!(
            got,
            Error(vec![Violation::LockTokenSubmitted(vec![Href(
                "/locked/".into()
            )])])
        );
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

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<PropertyUpdate<Core>>().await.unwrap();

        assert_eq!(
            got,
            PropertyUpdate(vec![
                PropertyUpdateItem::Set(Set(PropValue(vec![]))),
                PropertyUpdateItem::Remove(Remove(PropName(vec![]))),
            ])
        );
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

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<LockInfo>().await.unwrap();

        assert_eq!(
            got,
            LockInfo {
                lockscope: LockScope::Exclusive,
                locktype: LockType::Write,
                owner: Some(Owner::Href(Href(
                    "http://example.org/~ejw/contact.html".into()
                ))),
            }
        );
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

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<Multistatus<Core>>().await.unwrap();

        assert_eq!(
            got,
            Multistatus {
                responses: vec![
                    Response {
                        status_or_propstat: StatusOrPropstat::PropStat(
                            Href("http://www.example.com/container/".into()),
                            vec![PropStat {
                                prop: AnyProp(vec![
                                    AnyProperty::Request(PropertyRequest::CreationDate),
                                    AnyProperty::Request(PropertyRequest::DisplayName),
                                    AnyProperty::Request(PropertyRequest::ResourceType),
                                    AnyProperty::Request(PropertyRequest::SupportedLock),
                                ]),
                                status: Status(http::status::StatusCode::OK),
                                error: None,
                                responsedescription: None,
                            }],
                        ),
                        error: None,
                        responsedescription: None,
                        location: None,
                    },
                    Response {
                        status_or_propstat: StatusOrPropstat::PropStat(
                            Href("http://www.example.com/container/front.html".into()),
                            vec![PropStat {
                                prop: AnyProp(vec![
                                    AnyProperty::Request(PropertyRequest::CreationDate),
                                    AnyProperty::Request(PropertyRequest::DisplayName),
                                    AnyProperty::Request(PropertyRequest::GetContentLength),
                                    AnyProperty::Request(PropertyRequest::GetContentType),
                                    AnyProperty::Request(PropertyRequest::GetEtag),
                                    AnyProperty::Request(PropertyRequest::GetLastModified),
                                    AnyProperty::Request(PropertyRequest::ResourceType),
                                    AnyProperty::Request(PropertyRequest::SupportedLock),
                                ]),
                                status: Status(http::status::StatusCode::OK),
                                error: None,
                                responsedescription: None,
                            }],
                        ),
                        error: None,
                        responsedescription: None,
                        location: None,
                    },
                ],
                responsedescription: None,
                extension: None,
            }
        );
    }

    #[tokio::test]
    async fn rfc_multistatus_value() {
        let src = r#"
     <?xml version="1.0" encoding="utf-8" ?>
     <D:multistatus xmlns:D="DAV:">
       <D:response>
         <D:href>/container/</D:href>
         <D:propstat>
           <D:prop xmlns:R="http://ns.example.com/boxschema/">
             <R:bigbox><R:BoxType>Box type A</R:BoxType></R:bigbox>
             <R:author><R:Name>Hadrian</R:Name></R:author>
             <D:creationdate>1997-12-01T17:42:21-08:00</D:creationdate>
             <D:displayname>Example collection</D:displayname>
             <D:resourcetype><D:collection/></D:resourcetype>
             <D:supportedlock>
               <D:lockentry>
                 <D:lockscope><D:exclusive/></D:lockscope>
                 <D:locktype><D:write/></D:locktype>
               </D:lockentry>
               <D:lockentry>
                 <D:lockscope><D:shared/></D:lockscope>
                 <D:locktype><D:write/></D:locktype>
               </D:lockentry>
             </D:supportedlock>
           </D:prop>
           <D:status>HTTP/1.1 200 OK</D:status>
         </D:propstat>
       </D:response>
       <D:response>
         <D:href>/container/front.html</D:href>
         <D:propstat>
           <D:prop xmlns:R="http://ns.example.com/boxschema/">
             <R:bigbox><R:BoxType>Box type B</R:BoxType>
             </R:bigbox>
             <D:creationdate>1997-12-01T18:27:21-08:00</D:creationdate>
             <D:displayname>Example HTML resource</D:displayname>
             <D:getcontentlength>4525</D:getcontentlength>
             <D:getcontenttype>text/html</D:getcontenttype>
             <D:getetag>"zzyzx"</D:getetag>
             <D:getlastmodified
               >Mon, 12 Jan 1998 09:25:56 GMT</D:getlastmodified>
             <D:resourcetype/>
             <D:supportedlock>
               <D:lockentry>
                 <D:lockscope><D:exclusive/></D:lockscope>
                 <D:locktype><D:write/></D:locktype>
               </D:lockentry>
               <D:lockentry>
                 <D:lockscope><D:shared/></D:lockscope>
                 <D:locktype><D:write/></D:locktype>
               </D:lockentry>
             </D:supportedlock>
           </D:prop>
           <D:status>HTTP/1.1 200 OK</D:status>
         </D:propstat>
       </D:response>
     </D:multistatus>"#;

        let mut rdr = Reader::new(NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        let got = rdr.find::<Multistatus<Core>>().await.unwrap();

        assert_eq!(
            got,
            Multistatus {
                extension: None,
                responses: vec![
                    Response {
                        status_or_propstat: StatusOrPropstat::PropStat(
                            Href("/container/".into()),
                            vec![PropStat {
                                prop: AnyProp(vec![
                                    AnyProperty::Value(Property::CreationDate(
                                        FixedOffset::west_opt(8 * 3600)
                                            .unwrap()
                                            .with_ymd_and_hms(1997, 12, 01, 17, 42, 21)
                                            .unwrap()
                                    )),
                                    AnyProperty::Value(Property::DisplayName(
                                        "Example collection".into()
                                    )),
                                    AnyProperty::Value(Property::ResourceType(vec![
                                        ResourceType::Collection
                                    ])),
                                    AnyProperty::Value(Property::SupportedLock(vec![
                                        LockEntry {
                                            lockscope: LockScope::Exclusive,
                                            locktype: LockType::Write,
                                        },
                                        LockEntry {
                                            lockscope: LockScope::Shared,
                                            locktype: LockType::Write,
                                        },
                                    ])),
                                ]),
                                status: Status(http::status::StatusCode::OK),
                                error: None,
                                responsedescription: None,
                            }],
                        ),
                        error: None,
                        responsedescription: None,
                        location: None,
                    },
                    Response {
                        status_or_propstat: StatusOrPropstat::PropStat(
                            Href("/container/front.html".into()),
                            vec![PropStat {
                                prop: AnyProp(vec![
                                    AnyProperty::Value(Property::CreationDate(
                                        FixedOffset::west_opt(8 * 3600)
                                            .unwrap()
                                            .with_ymd_and_hms(1997, 12, 01, 18, 27, 21)
                                            .unwrap()
                                    )),
                                    AnyProperty::Value(Property::DisplayName(
                                        "Example HTML resource".into()
                                    )),
                                    AnyProperty::Value(Property::GetContentLength(4525)),
                                    AnyProperty::Value(Property::GetContentType(
                                        "text/html".into()
                                    )),
                                    AnyProperty::Value(Property::GetEtag(r#""zzyzx""#.into())),
                                    AnyProperty::Value(Property::GetLastModified(
                                        FixedOffset::west_opt(0)
                                            .unwrap()
                                            .with_ymd_and_hms(1998, 01, 12, 09, 25, 56)
                                            .unwrap()
                                    )),
                                    //@FIXME know bug, can't disambiguate between an empty resource
                                    //type value and a request resource type
                                    AnyProperty::Request(PropertyRequest::ResourceType),
                                    AnyProperty::Value(Property::SupportedLock(vec![
                                        LockEntry {
                                            lockscope: LockScope::Exclusive,
                                            locktype: LockType::Write,
                                        },
                                        LockEntry {
                                            lockscope: LockScope::Shared,
                                            locktype: LockType::Write,
                                        },
                                    ])),
                                ]),
                                status: Status(http::status::StatusCode::OK),
                                error: None,
                                responsedescription: None,
                            }],
                        ),
                        error: None,
                        responsedescription: None,
                        location: None,
                    },
                ],
                responsedescription: None,
            }
        );
    }
}
