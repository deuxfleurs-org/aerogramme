use std::io::Cursor;

use futures::stream::{StreamExt, TryStreamExt};
use quick_xml::Error;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;
use super::types::*;

//@FIXME a cleaner way to manager namespace would be great
//but at the same time, the quick-xml library is not cooperating.
//So instead of writing many cursed workarounds - I tried, I am just hardcoding the namespaces...

pub trait Encode {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<(), Error>; 
}

impl Encode for Href {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<(), Error> {
        xml.create_element("D:href")
            .write_text_content_async(BytesText::new(&self.0))
            .await?;
        Ok(())
    }
}

impl<T> Encode for Multistatus<T> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<(), Error> {
        xml.create_element("D:multistatus")
            .with_attribute(("xmlns:D", "DAV:"))
            .write_inner_content_async::<_, _, quick_xml::Error>(|inner_xml| async move {
                for response in self.responses.iter() {
                    response.write(inner_xml).await?;
                }
                
                if let Some(description) = &self.responsedescription {
                    description.write(inner_xml).await?;
                }
                
                Ok(inner_xml)
            })
            .await?;
        Ok(())
    }
}

impl<T> Encode for Response<T> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<(), Error> {
        unimplemented!();
    }
}

impl Encode for ResponseDescription {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<(), Error> {
        xml.create_element("D:responsedescription")
            .write_text_content_async(BytesText::new(&self.0))
            .await?;
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

        Href("/SOGo/dav/so/".into()).write(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        assert_eq!(buffer.as_slice(), &b"<D:href>/SOGo/dav/so/</D:href>"[..]);
    }


    #[tokio::test]
    async fn test_multistatus() {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);

        let xml: Multistatus<u64> = Multistatus { responses: vec![], responsedescription: Some(ResponseDescription("Hello world".into())) };
        xml.write(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        let expected = r#"<D:multistatus xmlns:D="DAV:">
    <D:responsedescription>Hello world</D:responsedescription>
</D:multistatus>"#;
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        assert_eq!(got, expected);
    }
}
