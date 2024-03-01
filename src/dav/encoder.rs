use std::io::Cursor;

use quick_xml::Error as QError;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;
use super::types::*;


//-------------- TRAITS ----------------------

/// Basic encode trait to make a type encodable
pub trait QuickWritable<C: Context> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError>; 
}

/// Encoding context
pub trait Context: Extension {
    fn child(&self) -> Self;
    fn create_dav_element(&self, name: &str) -> BytesStart;
    async fn hook_error(&self, err: &Self::Error, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError>;
    async fn hook_property(&self, prop: &Self::Property, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError>;
    async fn hook_resourcetype(&self, prop: &Self::ResourceType, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError>;
}

/// -------------- NoExtension Encoding Context
impl Context for NoExtension {
    fn child(&self) -> Self {
        Self { root: false }
    }
    fn create_dav_element(&self, name: &str) -> BytesStart {
        let mut start = BytesStart::new(format!("D:{}", name));
        if self.root {
            start.push_attribute(("xmlns:D", "DAV:"));
        }
        start
    }
    async fn hook_error(&self, err: &Disabled, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        unreachable!();
    }
    async fn hook_property(&self, prop: &Disabled, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        unreachable!();
    }
    async fn hook_resourcetype(&self, restype: &Disabled, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        unreachable!();
    }
}


//--------------------- ENCODING --------------------

// --- XML ROOTS

/// PROPFIND REQUEST
impl<C: Context> QuickWritable<C> for PropFind<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

/// PROPFIND RESPONSE, PROPPATCH RESPONSE, COPY RESPONSE, MOVE RESPONSE
/// DELETE RESPONSE, 
impl<C: Context> QuickWritable<C> for Multistatus<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("multistatus");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        for response in self.responses.iter() {
            response.write(xml, ctx.child()).await?;
        }
        if let Some(description) = &self.responsedescription {
            description.write(xml, ctx.child()).await?;
        }

        xml.write_event_async(Event::End(end)).await?;
        Ok(())
    }
}

/// LOCK REQUEST
impl<C: Context> QuickWritable<C> for LockInfo {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

/// SOME LOCK RESPONSES
impl<C: Context> QuickWritable<C> for Prop<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("prop");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        for property in &self.0 {
            property.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

// --- XML inner elements
impl<C: Context> QuickWritable<C> for Href {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("href");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(&self.0))).await?;
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for Response<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("href");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.href.write(xml, ctx.child()).await?; 
        self.status_or_propstat.write(xml, ctx.child()).await?;
        if let Some(error) = &self.error {
            error.write(xml, ctx.child()).await?;
        }
        if let Some(responsedescription) = &self.responsedescription {
            responsedescription.write(xml, ctx.child()).await?;
        }
        if let Some(location) = &self.location {
            location.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for StatusOrPropstat<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::Status(status) => status.write(xml, ctx.child()).await,
            Self::PropStat(propstat_list) => {
                for propstat in propstat_list.iter() {
                    propstat.write(xml, ctx.child()).await?;
                }

                Ok(())
            }
        }
    }
}

impl<C: Context> QuickWritable<C> for Status {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("status");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;

        let txt = format!("HTTP/1.1 {} {}", self.0.as_str(), self.0.canonical_reason().unwrap_or("No reason"));
        xml.write_event_async(Event::Text(BytesText::new(&txt))).await?;

        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for ResponseDescription {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("responsedescription");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(&self.0))).await?;
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for Location {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("location");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for PropStat<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("propstat");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.prop.write(xml, ctx.child()).await?;
        self.status.write(xml, ctx.child()).await?;
        if let Some(error) = &self.error {
            error.write(xml, ctx.child()).await?;
        }
        if let Some(description) = &self.responsedescription {
            description.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for Property<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        use Property::*;
        match self {
            CreationDate(date) => {
                // <D:creationdate>1997-12-01T17:42:21-08:00</D:creationdate>
                let start = ctx.create_dav_element("creationdate");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(&date.to_rfc3339()))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            DisplayName(name) => {
                // <D:displayname>Example collection</D:displayname>
                let start = ctx.create_dav_element("displayname");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(name))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            GetContentLanguage(lang) => {
                let start = ctx.create_dav_element("getcontentlanguage");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(lang))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            GetContentLength(len) => {
                // <D:getcontentlength>4525</D:getcontentlength>
                let start = ctx.create_dav_element("getcontentlength");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(&len.to_string()))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            GetContentType(ct) => {
                // <D:getcontenttype>text/html</D:getcontenttype>
                let start = ctx.create_dav_element("getcontenttype");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(&ct))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            GetEtag(et) => {
                // <D:getetag>"zzyzx"</D:getetag>
                let start = ctx.create_dav_element("getetag");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(et))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            GetLastModified(date) => {
                // <D:getlastmodified>Mon, 12 Jan 1998 09:25:56 GMT</D:getlastmodified>
                let start = ctx.create_dav_element("getlastmodified");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(&date.to_rfc3339()))).await?;
                xml.write_event_async(Event::End(end)).await?;
            },
            LockDiscovery(many_locks) => {
                // <D:lockdiscovery><D:activelock> ... </D:activelock></D:lockdiscovery>
                let start = ctx.create_dav_element("lockdiscovery");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for lock in many_locks.iter() {
                    lock.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await?;
            },
            ResourceType(many_types) => {
                // <D:resourcetype><D:collection/></D:resourcetype>
                
                // <D:resourcetype/>
                
                // <x:resourcetype xmlns:x="DAV:">
                //   <x:collection/>
                //   <f:search-results xmlns:f="http://www.example.com/ns"/>
                // </x:resourcetype>
    
                let start = ctx.create_dav_element("resourcetype");
                if many_types.is_empty() {
                    xml.write_event_async(Event::Empty(start)).await?;
                } else {
                    let end = start.to_end();
                    xml.write_event_async(Event::Start(start.clone())).await?;
                    for restype in many_types.iter() {
                        restype.write(xml, ctx.child()).await?;
                    }
                    xml.write_event_async(Event::End(end)).await?;
                }
            },
            SupportedLock(many_entries) => {
                // <D:supportedlock/>

                //  <D:supportedlock> <D:lockentry> ... </D:lockentry> </D:supportedlock>

                let start = ctx.create_dav_element("supportedlock");
                if many_entries.is_empty() {
                    xml.write_event_async(Event::Empty(start)).await?;
                } else {
                    let end = start.to_end();
                    xml.write_event_async(Event::Start(start.clone())).await?;
                    for entry in many_entries.iter() {
                        entry.write(xml, ctx.child()).await?;
                    }
                    xml.write_event_async(Event::End(end)).await?;
                }
            },
            Extension(inner) => {
                ctx.hook_property(inner, xml).await?;
            },
        };
        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for ResourceType<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::Collection => xml.write_event_async(Event::Empty(ctx.create_dav_element("collection"))).await,
            Self::Extension(inner) => ctx.hook_resourcetype(inner, xml).await,
        }
    }
}

impl<C: Context> QuickWritable<C> for ActiveLock {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        // <D:activelock>
        //   <D:locktype><D:write/></D:locktype>
        //   <D:lockscope><D:exclusive/></D:lockscope>
        //   <D:depth>infinity</D:depth>
        //   <D:owner>
        //     <D:href>http://example.org/~ejw/contact.html</D:href>
        //   </D:owner>
        //   <D:timeout>Second-604800</D:timeout>
        //   <D:locktoken>
        //     <D:href>urn:uuid:e71d4fae-5dec-22d6-fea5-00a0c91e6be4</D:href>
        //   </D:locktoken>
        //   <D:lockroot>
        //     <D:href>http://example.com/workspace/webdav/proposal.doc</D:href>
        //   </D:lockroot>
        // </D:activelock>
        let start = ctx.create_dav_element("activelock");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.locktype.write(xml, ctx.child()).await?;
        self.lockscope.write(xml, ctx.child()).await?;
        self.depth.write(xml, ctx.child()).await?;
        if let Some(owner) = &self.owner {
            owner.write(xml, ctx.child()).await?;
        }
        if let Some(timeout) = &self.timeout {
            timeout.write(xml, ctx.child()).await?;
        }
        if let Some(locktoken) = &self.locktoken {
            locktoken.write(xml, ctx.child()).await?;
        }
        self.lockroot.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for LockType {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("locktype");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Write => xml.write_event_async(Event::Empty(ctx.create_dav_element("write"))).await?,
        }; 
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for LockScope {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("lockscope");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Exclusive => xml.write_event_async(Event::Empty(ctx.create_dav_element("exclusive"))).await?,
            Self::Shared => xml.write_event_async(Event::Empty(ctx.create_dav_element("shared"))).await?,
        }; 
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for Owner {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("owner");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        if let Some(txt) = &self.txt {
            xml.write_event_async(Event::Text(BytesText::new(&txt))).await?;
        }
        if let Some(href) = &self.url {
            href.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for Depth {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("depth");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Zero => xml.write_event_async(Event::Text(BytesText::new("0"))).await?,
            Self::One => xml.write_event_async(Event::Text(BytesText::new("1"))).await?,
            Self::Infinity => xml.write_event_async(Event::Text(BytesText::new("infinity"))).await?,
        };
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for Timeout {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("timeout");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Seconds(count) => {
                let txt = format!("Second-{}", count);
                xml.write_event_async(Event::Text(BytesText::new(&txt))).await?
            },
            Self::Infinite => xml.write_event_async(Event::Text(BytesText::new("Infinite"))).await?
        };
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for LockToken {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("locktoken");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for LockRoot {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("lockroot");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for LockEntry {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("lockentry");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.lockscope.write(xml, ctx.child()).await?;
        self.locktype.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: Context> QuickWritable<C> for Error<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("error");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        for violation in &self.0 {
            violation.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<C: Context> QuickWritable<C> for Violation<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Violation::LockTokenMatchesRequestUri => xml.write_event_async(Event::Empty(ctx.create_dav_element("lock-token-matches-request-uri"))).await?, 
            Violation::LockTokenSubmitted(hrefs) => {
                let start = ctx.create_dav_element("lock-token-submitted");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for href in hrefs {
                    href.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await?;
            },
            Violation::NoConflictingLock(hrefs) => {
                let start = ctx.create_dav_element("no-conflicting-lock");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for href in hrefs {
                    href.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await?;
            },
            Violation::NoExternalEntities => xml.write_event_async(Event::Empty(ctx.create_dav_element("no-external-entities"))).await?,
            Violation::PreservedLiveProperties => xml.write_event_async(Event::Empty(ctx.create_dav_element("preserved-live-properties"))).await?,
            Violation::PropfindFiniteDepth => xml.write_event_async(Event::Empty(ctx.create_dav_element("propfind-finite-depth"))).await?,
            Violation::CannotModifyProtectedProperty => xml.write_event_async(Event::Empty(ctx.create_dav_element("cannot-modify-protected-property"))).await?,
            Violation::Extension(inner) => {
                ctx.hook_error(inner, xml).await?;
            },
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    /// To run only the unit tests and avoid the behavior ones:
    /// cargo test --bin aerogramme

    #[tokio::test]
    async fn test_href() {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);

        let ctx = NoExtension { root: false };
        Href("/SOGo/dav/so/".into()).write(&mut writer, ctx).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        assert_eq!(buffer.as_slice(), &b"<D:href>/SOGo/dav/so/</D:href>"[..]);
    }


    #[tokio::test]
    async fn test_multistatus() {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);

        let ctx = NoExtension { root: true };
        let xml = Multistatus { responses: vec![], responsedescription: Some(ResponseDescription("Hello world".into())) };
        xml.write(&mut writer, ctx).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:responsedescription>Hello world</D:responsedescription>
</D:multistatus>"#;
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        assert_eq!(got, expected);
    }
}
