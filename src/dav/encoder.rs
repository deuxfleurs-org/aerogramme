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
}


//--------------------- ENCODING --------------------

// --- XML ROOTS
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


impl<C: Context> QuickWritable<C> for Property<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        use Property::*;
        match self {
            CreationDate(date) => unimplemented!(),
            DisplayName(name) => unimplemented!(),
            //@FIXME not finished
            _ => unimplemented!(),
        };
        Ok(())
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
