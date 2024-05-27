use quick_xml::events::{BytesText, Event};
use quick_xml::Error as QError;

use super::types::Extension;
use super::versioningtypes::*;
use super::xml::{IWrite, QWrite, Writer};

impl<E: Extension> QWrite for Report<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Report::VersionTree => unimplemented!(),
            Report::ExpandProperty => unimplemented!(),
            Report::Extension(inner) => inner.qwrite(xml).await,
        }
    }
}

impl QWrite for Limit {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("limit");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for NResults {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("nresults");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(&format!("{}", self.0))))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::Node;
    use crate::xml::Reader;
    use tokio::io::AsyncWriteExt;

    async fn serialize_deserialize<T: Node<T>>(src: &T) -> T {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let q = quick_xml::writer::Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
        let ns_to_apply = vec![
            ("xmlns:D".into(), "DAV:".into()),
            ("xmlns:C".into(), "urn:ietf:params:xml:ns:caldav".into()),
        ];
        let mut writer = Writer { q, ns_to_apply };

        src.qwrite(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        // deserialize
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(got.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn nresults() {
        let orig = NResults(100);
        assert_eq!(orig, serialize_deserialize(&orig).await);
    }

    #[tokio::test]
    async fn limit() {
        let orig = Limit(NResults(1024));
        assert_eq!(orig, serialize_deserialize(&orig).await);
    }
}
