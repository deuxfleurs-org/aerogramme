use std::borrow::Cow;
use std::future::Future;

use quick_xml::events::{Event, BytesStart, BytesDecl, BytesText};
use quick_xml::events::attributes::AttrError;
use quick_xml::name::{Namespace, QName, PrefixDeclaration, ResolveResult, ResolveResult::*};
use quick_xml::reader::NsReader;
use tokio::io::AsyncBufRead;

use super::types::*;
use super::error::ParsingError;
use super::xml::{QRead, Reader, IRead, DAV_URN, CAL_URN};

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
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        // Find propfind
        xml.tag_start(DAV_URN, "propfind").await?;
        // Find any tag
        let propfind: PropFind<E> = loop {
            match xml.peek() {
                Event::Start(_) if xml.is_tag(DAV_URN, "allprop") => {
                    xml.tag_start(DAV_URN, "allprop").await?;
                    let r = PropFind::AllProp(Include::qread(xml).await?);
                    xml.tag_stop(DAV_URN, "allprop").await?;
                    break r
                },
                Event::Start(_) if xml.is_tag(DAV_URN, "prop") => {
                    let propname = PropName::qread(xml).await?.ok_or(ParsingError::MissingChild)?;
                    break PropFind::Prop(propname);
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

        Ok(Some(propfind))
    }
}

/// PROPPATCH request
impl<E: Extension> QRead<PropertyUpdate<E>> for PropertyUpdate<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "propertyupdate").await?;
        let mut collected_items = Vec::new();
        loop {
            // Try to collect a property item
            if let Some(item) = PropertyUpdateItem::qread(xml).await? {
                collected_items.push(item);
                continue
            }

            // Skip or stop otherwise
            match xml.peek() {
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        }

        xml.tag_stop(DAV_URN, "propertyupdate").await?;
        Ok(Some(PropertyUpdate(collected_items)))
    }
}

/// Generic response
//@TODO Multistatus

// LOCK REQUEST
impl QRead<LockInfo> for LockInfo {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "lockinfo").await?;
        let (mut m_scope, mut m_type, mut owner) = (None, None, None);
        loop {
            if let Some(v) = LockScope::qread(xml).await? {
                m_scope = Some(v);
            } else if let Some(v) = LockType::qread(xml).await? {
                m_type = Some(v);
            } else if let Some(v) = Owner::qread(xml).await? {
                owner = Some(v);
            } else {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        xml.tag_stop(DAV_URN, "lockinfo").await?;
        match (m_scope, m_type) {
            (Some(lockscope), Some(locktype)) => Ok(Some(LockInfo { lockscope, locktype, owner })),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

// LOCK RESPONSE
impl<E: Extension> QRead<PropValue<E>> for PropValue<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "prop").await?;
        let mut acc = Vec::new();
        loop {
            // Found a property
            if let Some(prop) = Property::qread(xml).await? {
                acc.push(prop);
                continue;
            }

            // Otherwise skip or escape
            match xml.peek() {
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(Some(PropValue(acc)))
    }
}


/// Error response
impl<E: Extension> QRead<Error<E>> for Error<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "error").await?;
        let mut violations = Vec::new();
        loop {
            match xml.peek() {
                Event::Start(_) | Event::Empty(_) => { 
                    Violation::qread(xml).await?.map(|v| violations.push(v)); 
                },
                Event::End(_) if xml.is_tag(DAV_URN, "error") => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "error").await?;
        Ok(Some(Error(violations)))
    }
}



// ---- INNER XML
impl<E: Extension> QRead<PropertyUpdateItem<E>> for PropertyUpdateItem<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        if let Some(rm) = Remove::qread(xml).await? {
            return Ok(Some(PropertyUpdateItem::Remove(rm)))
        }
        Ok(Set::qread(xml).await?.map(PropertyUpdateItem::Set))
    }
}

impl<E: Extension> QRead<Remove<E>> for Remove<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "remove") => xml.next().await?,
            _ => return Ok(None),
        };

        let propname = loop {
            match xml.peek() {
                Event::Start(b) | Event::Empty(b) if xml.is_tag(DAV_URN, "prop") => break PropName::qread(xml).await?,
                _ => xml.skip().await?,
            };
        };
        
        xml.tag_stop(DAV_URN, "remove").await?;
        Ok(propname.map(Remove))
    }
}

impl<E: Extension> QRead<Set<E>> for Set<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "set") => xml.next().await?,
            _ => return Ok(None),
        };
        let propvalue = loop {
            match xml.peek() {
                Event::Start(b) | Event::Empty(b) if xml.is_tag(DAV_URN, "prop") => break PropValue::qread(xml).await?,
                _ => xml.skip().await?,
            };
        };


        xml.tag_stop(DAV_URN, "set").await?;
        Ok(propvalue.map(Set))
    }
}

impl<E: Extension> QRead<Violation<E>> for Violation<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        loop {
            let bs = match xml.peek() {
                Event::Start(b) | Event::Empty(b) => b,
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
                    b"lock-token-matches-request-uri" => {
                        xml.next().await?;
                        Some(Violation::LockTokenMatchesRequestUri)
                    },
                    b"lock-token-submitted" => {
                        // start tag
                        xml.next().await?;

                        let mut links = Vec::new();
                        loop {
                            // If we find a Href
                            if let Some(href) = Href::qread(xml).await? {
                                links.push(href);
                                continue
                            }

                            // Otherwise
                            match xml.peek() {
                                Event::End(_) => break,
                                _ => { xml.skip().await?; },
                            }
                        }
                        xml.tag_stop(DAV_URN, "lock-token-submitted").await?;
                        Some(Violation::LockTokenSubmitted(links))
                    },
                    b"no-conflicting-lock" => {
                        // start tag
                        xml.next().await?;

                        let mut links = Vec::new();
                        loop {
                            // If we find a Href
                            if let Some(href) = Href::qread(xml).await? {
                                links.push(href);
                                continue
                            }

                            // Otherwise
                            match xml.peek() {
                                Event::End(_) => break,
                                _ => { xml.skip().await?; },
                            }
                        }
                        xml.tag_stop(DAV_URN, "no-conflicting-lock").await?;
                        Some(Violation::NoConflictingLock(links))
                    },
                    b"no-external-entities" => {
                        xml.next().await?;
                        Some(Violation::NoExternalEntities)
                    },
                    b"preserved-live-properties" => {
                        xml.next().await?;
                        Some(Violation::PreservedLiveProperties)
                    },
                    b"propfind-finite-depth" => {
                        xml.next().await?;
                        Some(Violation::PropfindFiniteDepth)
                    },
                    b"cannot-modify-protected-property" => {
                        xml.next().await?;
                        Some(Violation::CannotModifyProtectedProperty)
                    },
                    _ => None,
                };
            }

            // Option 2: an extension property, delegating
            if maybe_res.is_none() {
                maybe_res = E::Error::qread(xml).await?.map(Violation::Extension);
            }

            return Ok(maybe_res)
        }
    }
}

impl<E: Extension> QRead<Include<E>> for Include<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "include").await?;
        let mut acc = Vec::new();
        loop {
            // Found a property
            if let Some(prop) = PropertyRequest::qread(xml).await? {
                acc.push(prop);
                continue;
            }

            // Otherwise skip or escape
            match xml.peek() {
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "include").await?;
        Ok(Some(Include(acc)))
    }
}

impl<E: Extension> QRead<PropName<E>> for PropName<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "prop").await?;
        let mut acc = Vec::new();
        loop {
            // Found a property
            if let Some(prop) = PropertyRequest::qread(xml).await? {
                acc.push(prop);
                continue;
            }

            // Otherwise skip or escape
            match xml.peek() {
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(Some(PropName(acc)))
    }
}

impl<E: Extension> QRead<PropertyRequest<E>> for PropertyRequest<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        let bs = match xml.peek() {
            Event::Start(b) | Event::Empty(b) => b,
            _ => return Ok(None),
        };

        let mut maybe_res = None;

        // Option 1: a pure core DAV property
        let (ns, loc) = xml.rdr.resolve_element(bs.name());
        if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
            maybe_res = match loc.into_inner() {
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
            if maybe_res.is_some() {
                xml.skip().await?; 
            }
        }

        // Option 2: an extension property, delegating
        if maybe_res.is_none() {
            maybe_res = E::PropertyRequest::qread(xml).await?.map(PropertyRequest::Extension);
        }

        Ok(maybe_res)
    }
}

impl<E: Extension> QRead<Property<E>> for Property<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        use chrono::{DateTime, FixedOffset, TimeZone};

        let bs = match xml.peek() {
            Event::Start(b) | Event::Empty(b) => b,
            _ => return Ok(None),
        };

        let mut maybe_res = None;

        // Option 1: a pure core DAV property
        let (ns, loc) = xml.rdr.resolve_element(bs.name());
        if matches!(ns, Bound(Namespace(ns)) if ns == DAV_URN) {
            maybe_res = match loc.into_inner() {
                b"creationdate" => {
                    xml.next().await?;
                    let datestr = xml.tag_string().await?;
                    Some(Property::CreationDate(DateTime::parse_from_rfc3339(datestr.as_str())?))
                },
                b"displayname" => {
                    xml.next().await?;
                    Some(Property::DisplayName(xml.tag_string().await?))
                },
                b"getcontentlanguage" => {
                    xml.next().await?;
                    Some(Property::GetContentLanguage(xml.tag_string().await?))
                },
                b"getcontentlength" => {
                    xml.next().await?;
                    let cl = xml.tag_string().await?.parse::<u64>()?;
                    Some(Property::GetContentLength(cl))
                },
                b"getcontenttype" => {
                    xml.next().await?;
                    Some(Property::GetContentType(xml.tag_string().await?))
                },
                b"getetag" => {
                    xml.next().await?;
                    Some(Property::GetEtag(xml.tag_string().await?))
                },
                b"getlastmodified" => {
                    xml.next().await?;
                    xml.next().await?;
                    let datestr = xml.tag_string().await?;
                    Some(Property::CreationDate(DateTime::parse_from_rfc2822(datestr.as_str())?))
                },
                b"lockdiscovery" => {
                    // start tag
                    xml.next().await?;

                    let mut acc = Vec::new();
                    loop {
                        // If we find a lock
                        if let Some(lock) = ActiveLock::qread(xml).await? {
                            acc.push(lock);
                            continue
                        }

                        // Otherwise
                        match xml.peek() {
                            Event::End(_) => break,
                            _ => { xml.skip().await?; },
                        }
                    }
                    xml.tag_stop(DAV_URN, "lockdiscovery").await?;
                    Some(Property::LockDiscovery(acc))
                },
                b"resourcetype" => {
                    xml.next().await?;

                    let mut acc = Vec::new();
                    loop {
                        // If we find a resource type...
                        if let Some(restype) = ResourceType::qread(xml).await? {
                            acc.push(restype);
                            continue
                        }

                        // Otherwise
                        match xml.peek() {
                            Event::End(_) => break,
                            _ => { xml.skip().await?; },
                        }
                    }
                    xml.tag_stop(DAV_URN, "resourcetype").await?;
                    Some(Property::ResourceType(acc))
                },
                b"supportedlock" => {
                    xml.next().await?;

                    let mut acc = Vec::new();
                    loop {
                        // If we find a resource type...
                        if let Some(restype) = LockEntry::qread(xml).await? {
                            acc.push(restype);
                            continue
                        }

                        // Otherwise
                        match xml.peek() {
                            Event::End(_) => break,
                            _ => { xml.skip().await?; },
                        }
                    }
                    xml.tag_stop(DAV_URN, "supportedlock").await?;
                    Some(Property::SupportedLock(acc))
                },
                _ => None,
            };
        }

        // Option 2: an extension property, delegating
        if maybe_res.is_none() {
            maybe_res = E::Property::qread(xml).await?.map(Property::Extension);
        }

        Ok(maybe_res)
    }
}

impl QRead<ActiveLock> for ActiveLock {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "activelock").await?;
        let (mut m_scope, mut m_type, mut m_depth, mut owner, mut timeout, mut locktoken, mut m_root) = 
            (None, None, None, None, None, None, None);

        loop {
            if let Some(v) = LockScope::qread(xml).await? {
                m_scope = Some(v);
            } else if let Some(v) = LockType::qread(xml).await? {
                m_type = Some(v);
            } else if let Some(v) = Depth::qread(xml).await? {
                m_depth = Some(v);
            } else if let Some(v) = Owner::qread(xml).await? {
                owner = Some(v);
            } else if let Some(v) = Timeout::qread(xml).await? {
                timeout = Some(v);
            } else if let Some(v) = LockToken::qread(xml).await? {
                locktoken = Some(v);
            } else if let Some(v) = LockRoot::qread(xml).await? {
                m_root = Some(v);
            } else {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => { xml.skip().await?; },
                }
            }
        }

        xml.tag_stop(DAV_URN, "activelock").await?;
        match (m_scope, m_type, m_depth, m_root) {
            (Some(lockscope), Some(locktype), Some(depth), Some(lockroot)) =>
                Ok(Some(ActiveLock { lockscope, locktype, depth, owner, timeout, locktoken, lockroot })),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<Depth> for Depth {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "depth").await?;
        let depth_str = xml.tag_string().await?;
        xml.tag_stop(DAV_URN, "depth").await?;
        match depth_str.as_str() {
            "0" => Ok(Some(Depth::Zero)),
            "1" => Ok(Some(Depth::One)),
            "infinity" => Ok(Some(Depth::Infinity)),
            _ => Err(ParsingError::WrongToken),
        }
    }
}

impl QRead<Owner> for Owner {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        if xml.maybe_tag_start(DAV_URN, "owner").await?.is_none() {
            return Ok(None)
        }
        
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
                    if let Some(href) = Href::qread(xml).await? {
                        owner = Owner::Href(href)
                    }
                    xml.skip().await?;
                }
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        };
        xml.tag_stop(DAV_URN, "owner").await?;
        Ok(Some(owner))
    }
}

impl QRead<Timeout> for Timeout {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        const SEC_PFX: &str = "SEC_PFX";

        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "timeout") => xml.next().await?,
            _ => return Ok(None),
        };
        
        let timeout = match xml.tag_string().await?.as_str() {
            "Infinite" => Timeout::Infinite,
            seconds => match seconds.strip_prefix(SEC_PFX) {
                Some(secs) => Timeout::Seconds(secs.parse::<u32>()?),
                None => return Err(ParsingError::InvalidValue),
            },
        };

        xml.tag_stop(DAV_URN, "timeout").await?;
        Ok(Some(timeout))
    }
}

impl QRead<LockToken> for LockToken {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "locktoken") => xml.next().await?,
            _ => return Ok(None),
        };
        let href = Href::qread(xml).await?.ok_or(ParsingError::MissingChild)?;
        xml.tag_stop(DAV_URN, "locktoken").await?;
        Ok(Some(LockToken(href)))
    }
}

impl QRead<LockRoot> for LockRoot {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "lockroot").await?;
        let href = Href::qread(xml).await?.ok_or(ParsingError::MissingChild)?;
        xml.tag_stop(DAV_URN, "lockroot").await?;
        Ok(Some(LockRoot(href)))
    }
}

impl<E: Extension> QRead<ResourceType<E>> for ResourceType<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Empty(b) if xml.is_tag(DAV_URN, "collection") => {
                xml.next().await?;
                Ok(Some(ResourceType::Collection))
            },
            _ => Ok(E::ResourceType::qread(xml).await?.map(ResourceType::Extension)),
        }
    }
}

impl QRead<LockEntry> for LockEntry {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        xml.tag_start(DAV_URN, "lockentry").await?;
        let (mut maybe_scope, mut maybe_type) = (None, None);

        loop {
            match xml.peek() {
                Event::Start(_) if xml.is_tag(DAV_URN, "lockscope") => {
                    maybe_scope = LockScope::qread(xml).await?;
                },
                Event::Start(_) if xml.is_tag(DAV_URN, "lockentry") => {
                    maybe_type = LockType::qread(xml).await?;
                }
                Event::End(_) => break,
                _ => { xml.skip().await?; },
            }
        }

        let lockentry = match (maybe_scope, maybe_type) {
            (Some(lockscope), Some(locktype)) => LockEntry { lockscope, locktype },
            _ => return Err(ParsingError::MissingChild),
        };

        xml.tag_stop(DAV_URN, "lockentry").await?;
        Ok(Some(lockentry))
    }
}

impl QRead<LockScope> for LockScope {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        if xml.maybe_tag_start(DAV_URN, "lockscope").await?.is_none() {
            return Ok(None)
        }

        let lockscope = loop {
            println!("lockscope tag: {:?}", xml.peek());
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
        Ok(Some(lockscope))
    }
}

impl QRead<LockType> for LockType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        if xml.maybe_tag_start(DAV_URN, "locktype").await?.is_none() {
            return Ok(None)
        }

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
        Ok(Some(locktype))
    }
}

impl QRead<Href> for Href {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "href") => xml.next().await?,
            _ => return Ok(None),
        };

        let mut url = xml.tag_string().await?;
        xml.tag_stop(DAV_URN, "href").await?;
        Ok(Some(Href(url)))
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
        let got = PropFind::<Core>::qread(&mut rdr).await.unwrap().unwrap();

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
        let got = PropFind::<Core>::qread(&mut rdr).await.unwrap().unwrap();

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
        let got = Error::<Core>::qread(&mut rdr).await.unwrap().unwrap();

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
        let got = PropertyUpdate::<Core>::qread(&mut rdr).await.unwrap().unwrap();

        assert_eq!(got, PropertyUpdate(vec![
            PropertyUpdateItem::Set(Set(PropValue(vec![]))),
            PropertyUpdateItem::Remove(Remove(PropName(vec![]))),
        ]));
    }

    #[tokio::test]
    async fn rfc_lockinfo1() {
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
        let got = LockInfo::qread(&mut rdr).await.unwrap().unwrap();
        assert_eq!(got, LockInfo {
            lockscope: LockScope::Exclusive,
            locktype: LockType::Write,
            owner: Some(Owner::Href(Href("http://example.org/~ejw/contact.html".into()))),
        });
    }

}
