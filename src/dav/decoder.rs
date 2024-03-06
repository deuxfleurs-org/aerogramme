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

// ---- ROOT ----
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
        let mut acc: Vec<PropertyRequest<E>> = Vec::new();
        loop {
            match xml.peek() {
                Event::Start(_) | Event::Empty(_) => { 
                    PropertyRequest::qread(xml).await?.map(|v| acc.push(v)); 
                },
                Event::End(_) if xml.is_tag(DAV_URN, "include") => break,
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
        let mut acc: Vec<PropertyRequest<E>> = Vec::new();
        loop {
            match xml.peek() {
                Event::Start(_) | Event::Empty(_) => {
                    PropertyRequest::qread(xml).await?.map(|v| acc.push(v));
                },
                Event::End(_) if xml.is_tag(DAV_URN, "prop") => break,
                _ => { xml.skip().await?; },
            }
        }
        xml.tag_stop(DAV_URN, "prop").await?;
        Ok(Some(PropName(acc)))
    }
}

impl<E: Extension> QRead<PropertyRequest<E>> for PropertyRequest<E> {
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

            return Ok(maybe_res)
        }
    }
}

impl QRead<Href> for Href {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Option<Self>, ParsingError> {
        match xml.peek() {
            Event::Start(b) if xml.is_tag(DAV_URN, "href") => xml.next().await?,
            _ => return Ok(None),
        };

        let mut url = String::new();
        loop {
            match xml.peek() {
                Event::End(_) => break,
                Event::Start(_) | Event::Empty(_) => return Err(ParsingError::WrongToken),
                Event::CData(unescaped) => {
                    url.push_str(std::str::from_utf8(unescaped.as_ref())?);
                    xml.next().await?
                },
                Event::Text(escaped) => {
                    url.push_str(escaped.unescape()?.as_ref());
                    xml.next().await?
                }
                _ => xml.skip().await?,
            };
        }
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
}
