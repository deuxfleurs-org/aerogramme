use std::io::Cursor;

use anyhow::Result;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;
use super::types::*;

//@FIXME a cleaner way to manager namespace would be great
//but at the same time, the quick-xml library is not cooperating.
//So instead of writing many cursed workarounds - I tried, I am just hardcoding the namespaces...

pub trait Encode {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<()>; 
}

impl Encode for Href {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite + Unpin>) -> Result<()> {
        xml.create_element("D:href")
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
}
