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
            }

            // Option 2: an extension property
            if maybe_res.is_none() {
                maybe_res = E::PropertyRequest::qread(xml).await?.map(PropertyRequest::Extension);
            }

            // Close the current tag
            xml.skip().await?; 

            return Ok(maybe_res)
        }
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
}
