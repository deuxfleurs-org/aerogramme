use super::types::*;
use super::xml::{IWrite, Node, QWrite, Writer};
use quick_xml::events::{BytesText, Event};
use quick_xml::Error as QError;

// --- XML ROOTS

/// PROPFIND REQUEST
impl<E: Extension> QWrite for PropFind<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("propfind");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::PropName => {
                let empty_propname = xml.create_dav_element("propname");
                xml.q
                    .write_event_async(Event::Empty(empty_propname))
                    .await?
            }
            Self::AllProp(maybe_include) => {
                let empty_allprop = xml.create_dav_element("allprop");
                xml.q.write_event_async(Event::Empty(empty_allprop)).await?;
                if let Some(include) = maybe_include {
                    include.qwrite(xml).await?;
                }
            }
            Self::Prop(propname) => propname.qwrite(xml).await?,
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

/// PROPPATCH REQUEST
impl<E: Extension> QWrite for PropertyUpdate<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("propertyupdate");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for update in self.0.iter() {
            update.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

/// PROPFIND RESPONSE, PROPPATCH RESPONSE, COPY RESPONSE, MOVE RESPONSE
/// DELETE RESPONSE,
impl<E: Extension> QWrite for Multistatus<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("multistatus");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for response in self.responses.iter() {
            response.qwrite(xml).await?;
        }
        if let Some(description) = &self.responsedescription {
            description.qwrite(xml).await?;
        }
        if let Some(extension) = &self.extension {
            extension.qwrite(xml).await?;
        }

        xml.q.write_event_async(Event::End(end)).await?;
        Ok(())
    }
}

/// LOCK REQUEST
impl QWrite for LockInfo {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("lockinfo");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.lockscope.qwrite(xml).await?;
        self.locktype.qwrite(xml).await?;
        if let Some(owner) = &self.owner {
            owner.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

/// SOME LOCK RESPONSES
impl<E: Extension> QWrite for PropValue<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("prop");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for propval in &self.0 {
            propval.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

/// Error response
impl<E: Extension> QWrite for Error<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("error");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for violation in &self.0 {
            violation.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

// --- XML inner elements
impl<E: Extension> QWrite for PropertyUpdateItem<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Set(set) => set.qwrite(xml).await,
            Self::Remove(rm) => rm.qwrite(xml).await,
        }
    }
}

impl<E: Extension> QWrite for Set<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("set");
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for Remove<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("remove");
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for PropName<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("prop");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for propname in &self.0 {
            propname.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for AnyProp<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("prop");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for propname in &self.0 {
            propname.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for AnyProperty<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Request(v) => v.qwrite(xml).await,
            Self::Value(v) => v.qwrite(xml).await,
        }
    }
}

impl QWrite for Href {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("href");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(&self.0)))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for Response<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("response");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.status_or_propstat.qwrite(xml).await?;
        if let Some(error) = &self.error {
            error.qwrite(xml).await?;
        }
        if let Some(responsedescription) = &self.responsedescription {
            responsedescription.qwrite(xml).await?;
        }
        if let Some(location) = &self.location {
            location.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for StatusOrPropstat<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Status(many_href, status) => {
                for href in many_href.iter() {
                    href.qwrite(xml).await?;
                }
                status.qwrite(xml).await
            }
            Self::PropStat(href, propstat_list) => {
                href.qwrite(xml).await?;
                for propstat in propstat_list.iter() {
                    propstat.qwrite(xml).await?;
                }
                Ok(())
            }
        }
    }
}

impl QWrite for Status {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("status");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;

        let txt = format!(
            "HTTP/1.1 {} {}",
            self.0.as_str(),
            self.0.canonical_reason().unwrap_or("No reason")
        );
        xml.q
            .write_event_async(Event::Text(BytesText::new(&txt)))
            .await?;

        xml.q.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl QWrite for ResponseDescription {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("responsedescription");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(&self.0)))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Location {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("location");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for PropStat<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("propstat");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.prop.qwrite(xml).await?;
        self.status.qwrite(xml).await?;
        if let Some(error) = &self.error {
            error.qwrite(xml).await?;
        }
        if let Some(description) = &self.responsedescription {
            description.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<E: Extension> QWrite for Property<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        use Property::*;
        match self {
            CreationDate(date) => {
                // <D:creationdate>1997-12-01T17:42:21-08:00</D:creationdate>
                let start = xml.create_dav_element("creationdate");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&date.to_rfc3339())))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            DisplayName(name) => {
                // <D:displayname>Example collection</D:displayname>
                let start = xml.create_dav_element("displayname");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(name)))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            GetContentLanguage(lang) => {
                let start = xml.create_dav_element("getcontentlanguage");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(lang)))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            GetContentLength(len) => {
                // <D:getcontentlength>4525</D:getcontentlength>
                let start = xml.create_dav_element("getcontentlength");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&len.to_string())))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            GetContentType(ct) => {
                // <D:getcontenttype>text/html</D:getcontenttype>
                let start = xml.create_dav_element("getcontenttype");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&ct)))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            GetEtag(et) => {
                // <D:getetag>"zzyzx"</D:getetag>
                let start = xml.create_dav_element("getetag");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(et)))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            GetLastModified(date) => {
                // <D:getlastmodified>Mon, 12 Jan 1998 09:25:56 GMT</D:getlastmodified>
                let start = xml.create_dav_element("getlastmodified");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&date.to_rfc2822())))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await?;
            }
            LockDiscovery(many_locks) => {
                // <D:lockdiscovery><D:activelock> ... </D:activelock></D:lockdiscovery>
                let start = xml.create_dav_element("lockdiscovery");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for lock in many_locks.iter() {
                    lock.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await?;
            }
            ResourceType(many_types) => {
                // <D:resourcetype><D:collection/></D:resourcetype>

                // <D:resourcetype/>

                // <x:resourcetype xmlns:x="DAV:">
                //   <x:collection/>
                //   <f:search-results xmlns:f="http://www.example.com/ns"/>
                // </x:resourcetype>

                let start = xml.create_dav_element("resourcetype");
                if many_types.is_empty() {
                    xml.q.write_event_async(Event::Empty(start)).await?;
                } else {
                    let end = start.to_end();
                    xml.q.write_event_async(Event::Start(start.clone())).await?;
                    for restype in many_types.iter() {
                        restype.qwrite(xml).await?;
                    }
                    xml.q.write_event_async(Event::End(end)).await?;
                }
            }
            SupportedLock(many_entries) => {
                // <D:supportedlock/>

                //  <D:supportedlock> <D:lockentry> ... </D:lockentry> </D:supportedlock>

                let start = xml.create_dav_element("supportedlock");
                if many_entries.is_empty() {
                    xml.q.write_event_async(Event::Empty(start)).await?;
                } else {
                    let end = start.to_end();
                    xml.q.write_event_async(Event::Start(start.clone())).await?;
                    for entry in many_entries.iter() {
                        entry.qwrite(xml).await?;
                    }
                    xml.q.write_event_async(Event::End(end)).await?;
                }
            }
            Extension(inner) => inner.qwrite(xml).await?,
        };
        Ok(())
    }
}

impl<E: Extension> QWrite for ResourceType<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Collection => {
                let empty_collection = xml.create_dav_element("collection");
                xml.q
                    .write_event_async(Event::Empty(empty_collection))
                    .await
            }
            Self::Extension(inner) => inner.qwrite(xml).await,
        }
    }
}

impl<E: Extension> QWrite for Include<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("include");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for prop in self.0.iter() {
            prop.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for PropertyRequest<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        use PropertyRequest::*;
        let mut atom = async |c| {
            let empty_tag = xml.create_dav_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

        match self {
            CreationDate => atom("creationdate").await,
            DisplayName => atom("displayname").await,
            GetContentLanguage => atom("getcontentlanguage").await,
            GetContentLength => atom("getcontentlength").await,
            GetContentType => atom("getcontenttype").await,
            GetEtag => atom("getetag").await,
            GetLastModified => atom("getlastmodified").await,
            LockDiscovery => atom("lockdiscovery").await,
            ResourceType => atom("resourcetype").await,
            SupportedLock => atom("supportedlock").await,
            Extension(inner) => inner.qwrite(xml).await,
        }
    }
}

impl QWrite for ActiveLock {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
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
        let start = xml.create_dav_element("activelock");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.locktype.qwrite(xml).await?;
        self.lockscope.qwrite(xml).await?;
        self.depth.qwrite(xml).await?;
        if let Some(owner) = &self.owner {
            owner.qwrite(xml).await?;
        }
        if let Some(timeout) = &self.timeout {
            timeout.qwrite(xml).await?;
        }
        if let Some(locktoken) = &self.locktoken {
            locktoken.qwrite(xml).await?;
        }
        self.lockroot.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for LockType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("locktype");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Write => {
                let empty_write = xml.create_dav_element("write");
                xml.q.write_event_async(Event::Empty(empty_write)).await?
            }
        };
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for LockScope {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("lockscope");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Exclusive => {
                let empty_tag = xml.create_dav_element("exclusive");
                xml.q.write_event_async(Event::Empty(empty_tag)).await?
            }
            Self::Shared => {
                let empty_tag = xml.create_dav_element("shared");
                xml.q.write_event_async(Event::Empty(empty_tag)).await?
            }
        };
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Owner {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("owner");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Txt(txt) => {
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&txt)))
                    .await?
            }
            Self::Href(href) => href.qwrite(xml).await?,
            Self::Unknown => (),
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Depth {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("depth");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Zero => {
                xml.q
                    .write_event_async(Event::Text(BytesText::new("0")))
                    .await?
            }
            Self::One => {
                xml.q
                    .write_event_async(Event::Text(BytesText::new("1")))
                    .await?
            }
            Self::Infinity => {
                xml.q
                    .write_event_async(Event::Text(BytesText::new("infinity")))
                    .await?
            }
        };
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Timeout {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("timeout");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        match self {
            Self::Seconds(count) => {
                let txt = format!("Second-{}", count);
                xml.q
                    .write_event_async(Event::Text(BytesText::new(&txt)))
                    .await?
            }
            Self::Infinite => {
                xml.q
                    .write_event_async(Event::Text(BytesText::new("Infinite")))
                    .await?
            }
        };
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for LockToken {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("locktoken");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for LockRoot {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("lockroot");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for LockEntry {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("lockentry");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.lockscope.qwrite(xml).await?;
        self.locktype.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for Violation<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_dav_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

        match self {
            Violation::LockTokenMatchesRequestUri => atom("lock-token-matches-request-uri").await,
            Violation::LockTokenSubmitted(hrefs) if hrefs.is_empty() => {
                atom("lock-token-submitted").await
            }
            Violation::LockTokenSubmitted(hrefs) => {
                let start = xml.create_dav_element("lock-token-submitted");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for href in hrefs {
                    href.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            }
            Violation::NoConflictingLock(hrefs) if hrefs.is_empty() => {
                atom("no-conflicting-lock").await
            }
            Violation::NoConflictingLock(hrefs) => {
                let start = xml.create_dav_element("no-conflicting-lock");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for href in hrefs {
                    href.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            }
            Violation::NoExternalEntities => atom("no-external-entities").await,
            Violation::PreservedLiveProperties => atom("preserved-live-properties").await,
            Violation::PropfindFiniteDepth => atom("propfind-finite-depth").await,
            Violation::CannotModifyProtectedProperty => {
                atom("cannot-modify-protected-property").await
            }
            Violation::Extension(inner) => inner.qwrite(xml).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::xml;
    use super::*;
    use crate::realization::Core;
    use tokio::io::AsyncWriteExt;

    /// To run only the unit tests and avoid the behavior ones:
    /// cargo test --bin aerogramme

    async fn serialize(elem: &impl QWrite) -> String {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let q = quick_xml::writer::Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
        let ns_to_apply = vec![("xmlns:D".into(), "DAV:".into())];
        let mut writer = Writer { q, ns_to_apply };

        elem.qwrite(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        return got.into();
    }

    async fn deserialize<T: xml::Node<T>>(src: &str) -> T {
        let mut rdr = xml::Reader::new(quick_xml::reader::NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn basic_href() {
        let orig = Href("/SOGo/dav/so/".into());

        let got = serialize(&orig).await;
        let expected = r#"<D:href xmlns:D="DAV:">/SOGo/dav/so/</D:href>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Href>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn basic_multistatus() {
        let orig = Multistatus::<Core> {
            extension: None,
            responses: vec![],
            responsedescription: Some(ResponseDescription("Hello world".into())),
        };
        let got = serialize(&orig).await;

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:responsedescription>Hello world</D:responsedescription>
</D:multistatus>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Multistatus::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_error_delete_locked() {
        let orig = Error::<Core>(vec![Violation::LockTokenSubmitted(vec![Href(
            "/locked/".into(),
        )])]);
        let got = serialize(&orig).await;

        let expected = r#"<D:error xmlns:D="DAV:">
    <D:lock-token-submitted>
        <D:href>/locked/</D:href>
    </D:lock-token-submitted>
</D:error>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Error<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_propname_req() {
        let orig = PropFind::<Core>::PropName;

        let got = serialize(&orig).await;

        let expected = r#"<D:propfind xmlns:D="DAV:">
    <D:propname/>
</D:propfind>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<PropFind::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_propname_res() {
        let orig = Multistatus::<Core> {
            extension: None,
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
        };

        let got = serialize(&orig).await;

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>http://www.example.com/container/</D:href>
        <D:propstat>
            <D:prop>
                <D:creationdate/>
                <D:displayname/>
                <D:resourcetype/>
                <D:supportedlock/>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>http://www.example.com/container/front.html</D:href>
        <D:propstat>
            <D:prop>
                <D:creationdate/>
                <D:displayname/>
                <D:getcontentlength/>
                <D:getcontenttype/>
                <D:getetag/>
                <D:getlastmodified/>
                <D:resourcetype/>
                <D:supportedlock/>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Multistatus::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_allprop_req() {
        let orig = PropFind::<Core>::AllProp(None);
        let got = serialize(&orig).await;

        let expected = r#"<D:propfind xmlns:D="DAV:">
    <D:allprop/>
</D:propfind>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<PropFind::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_allprop_res() {
        use chrono::{FixedOffset, TimeZone};

        let orig = Multistatus::<Core> {
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
                                        .with_ymd_and_hms(1997, 12, 1, 17, 42, 21)
                                        .unwrap(),
                                )),
                                AnyProperty::Value(Property::DisplayName(
                                    "Example collection".into(),
                                )),
                                AnyProperty::Value(Property::ResourceType(vec![
                                    ResourceType::Collection,
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
                                        .with_ymd_and_hms(1997, 12, 1, 18, 27, 21)
                                        .unwrap(),
                                )),
                                AnyProperty::Value(Property::DisplayName(
                                    "Example HTML resource".into(),
                                )),
                                AnyProperty::Value(Property::GetContentLength(4525)),
                                AnyProperty::Value(Property::GetContentType("text/html".into())),
                                AnyProperty::Value(Property::GetEtag(r#""zzyzx""#.into())),
                                AnyProperty::Value(Property::GetLastModified(
                                    FixedOffset::east_opt(0)
                                        .unwrap()
                                        .with_ymd_and_hms(1998, 1, 12, 9, 25, 56)
                                        .unwrap(),
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
        };

        let got = serialize(&orig).await;

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>/container/</D:href>
        <D:propstat>
            <D:prop>
                <D:creationdate>1997-12-01T17:42:21-08:00</D:creationdate>
                <D:displayname>Example collection</D:displayname>
                <D:resourcetype>
                    <D:collection/>
                </D:resourcetype>
                <D:supportedlock>
                    <D:lockentry>
                        <D:lockscope>
                            <D:exclusive/>
                        </D:lockscope>
                        <D:locktype>
                            <D:write/>
                        </D:locktype>
                    </D:lockentry>
                    <D:lockentry>
                        <D:lockscope>
                            <D:shared/>
                        </D:lockscope>
                        <D:locktype>
                            <D:write/>
                        </D:locktype>
                    </D:lockentry>
                </D:supportedlock>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>/container/front.html</D:href>
        <D:propstat>
            <D:prop>
                <D:creationdate>1997-12-01T18:27:21-08:00</D:creationdate>
                <D:displayname>Example HTML resource</D:displayname>
                <D:getcontentlength>4525</D:getcontentlength>
                <D:getcontenttype>text/html</D:getcontenttype>
                <D:getetag>&quot;zzyzx&quot;</D:getetag>
                <D:getlastmodified>Mon, 12 Jan 1998 09:25:56 +0000</D:getlastmodified>
                <D:resourcetype/>
                <D:supportedlock>
                    <D:lockentry>
                        <D:lockscope>
                            <D:exclusive/>
                        </D:lockscope>
                        <D:locktype>
                            <D:write/>
                        </D:locktype>
                    </D:lockentry>
                    <D:lockentry>
                        <D:lockscope>
                            <D:shared/>
                        </D:lockscope>
                        <D:locktype>
                            <D:write/>
                        </D:locktype>
                    </D:lockentry>
                </D:supportedlock>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Multistatus::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_allprop_include() {
        let orig = PropFind::<Core>::AllProp(Some(Include(vec![
            PropertyRequest::DisplayName,
            PropertyRequest::ResourceType,
        ])));

        let got = serialize(&orig).await;

        let expected = r#"<D:propfind xmlns:D="DAV:">
    <D:allprop/>
    <D:include>
        <D:displayname/>
        <D:resourcetype/>
    </D:include>
</D:propfind>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<PropFind::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_propertyupdate() {
        let orig = PropertyUpdate::<Core>(vec![
            PropertyUpdateItem::Set(Set(PropValue(vec![Property::GetContentLanguage(
                "fr-FR".into(),
            )]))),
            PropertyUpdateItem::Remove(Remove(PropName(vec![PropertyRequest::DisplayName]))),
        ]);
        let got = serialize(&orig).await;

        let expected = r#"<D:propertyupdate xmlns:D="DAV:">
    <D:set>
        <D:prop>
            <D:getcontentlanguage>fr-FR</D:getcontentlanguage>
        </D:prop>
    </D:set>
    <D:remove>
        <D:prop>
            <D:displayname/>
        </D:prop>
    </D:remove>
</D:propertyupdate>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(
            deserialize::<PropertyUpdate::<Core>>(got.as_str()).await,
            orig
        )
    }

    #[tokio::test]
    async fn rfc_delete_locked2() {
        let orig = Multistatus::<Core> {
            extension: None,
            responses: vec![Response {
                status_or_propstat: StatusOrPropstat::Status(
                    vec![Href("http://www.example.com/container/resource3".into())],
                    Status(http::status::StatusCode::from_u16(423).unwrap()),
                ),
                error: Some(Error(vec![Violation::LockTokenSubmitted(vec![])])),
                responsedescription: None,
                location: None,
            }],
            responsedescription: None,
        };

        let got = serialize(&orig).await;

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:response>
        <D:href>http://www.example.com/container/resource3</D:href>
        <D:status>HTTP/1.1 423 Locked</D:status>
        <D:error>
            <D:lock-token-submitted/>
        </D:error>
    </D:response>
</D:multistatus>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<Multistatus::<Core>>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_simple_lock_request() {
        let orig = LockInfo {
            lockscope: LockScope::Exclusive,
            locktype: LockType::Write,
            owner: Some(Owner::Href(Href(
                "http://example.org/~ejw/contact.html".into(),
            ))),
        };

        let got = serialize(&orig).await;

        let expected = r#"<D:lockinfo xmlns:D="DAV:">
    <D:lockscope>
        <D:exclusive/>
    </D:lockscope>
    <D:locktype>
        <D:write/>
    </D:locktype>
    <D:owner>
        <D:href>http://example.org/~ejw/contact.html</D:href>
    </D:owner>
</D:lockinfo>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<LockInfo>(got.as_str()).await, orig)
    }

    #[tokio::test]
    async fn rfc_simple_lock_response() {
        let orig = PropValue::<Core>(vec![Property::LockDiscovery(vec![ActiveLock {
            lockscope: LockScope::Exclusive,
            locktype: LockType::Write,
            depth: Depth::Infinity,
            owner: Some(Owner::Href(Href(
                "http://example.org/~ejw/contact.html".into(),
            ))),
            timeout: Some(Timeout::Seconds(604800)),
            locktoken: Some(LockToken(Href(
                "urn:uuid:e71d4fae-5dec-22d6-fea5-00a0c91e6be4".into(),
            ))),
            lockroot: LockRoot(Href(
                "http://example.com/workspace/webdav/proposal.doc".into(),
            )),
        }])]);

        let got = serialize(&orig).await;

        let expected = r#"<D:prop xmlns:D="DAV:">
    <D:lockdiscovery>
        <D:activelock>
            <D:locktype>
                <D:write/>
            </D:locktype>
            <D:lockscope>
                <D:exclusive/>
            </D:lockscope>
            <D:depth>infinity</D:depth>
            <D:owner>
                <D:href>http://example.org/~ejw/contact.html</D:href>
            </D:owner>
            <D:timeout>Second-604800</D:timeout>
            <D:locktoken>
                <D:href>urn:uuid:e71d4fae-5dec-22d6-fea5-00a0c91e6be4</D:href>
            </D:locktoken>
            <D:lockroot>
                <D:href>http://example.com/workspace/webdav/proposal.doc</D:href>
            </D:lockroot>
        </D:activelock>
    </D:lockdiscovery>
</D:prop>"#;

        assert_eq!(
            &got, expected,
            "\n---GOT---\n{got}\n---EXP---\n{expected}\n"
        );
        assert_eq!(deserialize::<PropValue::<Core>>(got.as_str()).await, orig)
    }
}
