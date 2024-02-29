use std::io::Cursor;

use quick_xml::Error as QError;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;
use super::types::*;


//-------------- TRAITS ----------------------
/*pub trait DavWriter<E: Extension> {
    fn create_dav_element(&mut self, name: &str) -> ElementWriter<impl AsyncWrite + Unpin>;
    fn child(w: &mut QWriter<impl AsyncWrite + Unpin>) -> impl DavWriter<E>;
    async fn error(&mut self, err: &E::Error) -> Result<(), QError>;
}*/

/// Basic encode trait to make a type encodable
pub trait QuickWritable<E: Extension, C: Context<E>> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError>; 
}

pub trait Context<E: Extension> {
    fn child(&self) -> Self;
    fn create_dav_element(&self, name: &str) -> BytesStart;
    async fn hook_error(&self, err: &E::Error, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError>;
}

pub struct NoExtCtx {
    root: bool
}
impl Context<NoExtension> for NoExtCtx {
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
impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Multistatus<E> {
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
impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Href {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, _ctx: C) -> Result<(), QError> {
        xml.create_element("href")
            .write_text_content_async(BytesText::new(&self.0))
            .await?;
        Ok(())
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Response<E> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        xml.create_element("response")
            .write_inner_content_async::<_, _, QError>(|inner_xml| async move {
                self.href.write(inner_xml, ctx.child()).await?; 
                self.status_or_propstat.write(inner_xml, ctx.child()).await?;
                if let Some(error) = &self.error {
                    error.write(inner_xml, ctx.child()).await?;
                }
                if let Some(responsedescription) = &self.responsedescription {
                    responsedescription.write(inner_xml, ctx.child()).await?;
                }
                if let Some(location) = &self.location {
                    location.write(inner_xml, ctx.child()).await?;
                }

                Ok(inner_xml)
            })
        .await?;

        Ok(())
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for StatusOrPropstat<E> {
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

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Status {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        xml.create_element("status")
            .write_text_content_async(
                BytesText::new(&format!("HTTP/1.1 {} {}", self.0.as_str(), self.0.canonical_reason().unwrap_or("No reason")))
            )
            .await?;
        Ok(())
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for ResponseDescription {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_dav_element("responsedescription");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(&self.0))).await?;
        xml.write_event_async(Event::End(end)).await?;

        Ok(())
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Location {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for PropStat<E> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Error<E> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        xml.create_element("error")
            .write_inner_content_async::<_, _, QError>(|inner_xml| async move {
                for violation in &self.0 {
                    violation.write(inner_xml, ctx.child()).await?;
                }

                Ok(inner_xml)
            })
        .await?;

        Ok(())
    }
}

impl<E: Extension, C: Context<E>> QuickWritable<E,C> for Violation<E> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Violation::LockTokenMatchesRequestUri => xml.create_element("lock-token-matches-request-uri").write_empty_async().await?, 
            Violation::LockTokenSubmitted(hrefs) => xml
                .create_element("lock-token-submitted")
                .write_inner_content_async::<_, _, QError>(|inner_xml| async move {
                    for href in hrefs {
                        href.write(inner_xml, ctx.child()).await?;
                    }
                    Ok(inner_xml)
                }
            ).await?,
            Violation::NoConflictingLock(hrefs) =>  xml
                .create_element("no-conflicting-lock")
                .write_inner_content_async::<_, _, QError>(|inner_xml| async move {
                    for href in hrefs {
                        href.write(inner_xml, ctx.child()).await?;
                    }
                    Ok(inner_xml)
                }
            ).await?,
            Violation::NoExternalEntities => xml.create_element("no-external-entities").write_empty_async().await?,
            Violation::PreservedLiveProperties => xml.create_element("preserved-live-properties").write_empty_async().await?,
            Violation::PropfindFiniteDepth => xml.create_element("propfind-finite-depth").write_empty_async().await?,
            Violation::CannotModifyProtectedProperty => xml.create_element("cannot-modify-protected-property").write_empty_async().await?,
            Violation::Extension(inner) => {
                ctx.hook_error(inner, xml).await?;
                xml
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

        let ctx = NoExtCtx{ root: true };
        Href("/SOGo/dav/so/".into()).write(&mut writer, ctx).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        assert_eq!(buffer.as_slice(), &b"<D:href>/SOGo/dav/so/</D:href>"[..]);
    }


    #[tokio::test]
    async fn test_multistatus() {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);

        let ctx = NoExtCtx{ root: true };
        let xml: Multistatus<NoExtension> = Multistatus { responses: vec![], responsedescription: Some(ResponseDescription("Hello world".into())) };
        xml.write(&mut writer, ctx).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:responsedescription>Hello world</D:responsedescription>
</D:multistatus>"#;
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        assert_eq!(got, expected);
    }
}
