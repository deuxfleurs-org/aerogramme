use quick_xml::events::{BytesText, Event};
use quick_xml::Error as QError;

use super::synctypes::*;
use super::types::Extension;
use super::xml::{IWrite, QWrite, Writer};

impl<E: Extension> QWrite for SyncCollection<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("sync-collection");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.sync_token.qwrite(xml).await?;
        self.sync_level.qwrite(xml).await?;
        if let Some(limit) = &self.limit {
            limit.qwrite(xml).await?;
        }
        self.prop.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for SyncTokenRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("sync-token");

        match self {
            Self::InitialSync => xml.q.write_event_async(Event::Empty(start)).await,
            Self::IncrementalSync(uri) => {
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q
                    .write_event_async(Event::Text(BytesText::new(uri.as_str())))
                    .await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for SyncToken {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("sync-token");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(self.0.as_str())))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for SyncLevel {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_dav_element("sync-level");
        let end = start.to_end();
        let text = match self {
            Self::One => "1",
            Self::Infinite => "infinite",
        };

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q
            .write_event_async(Event::Text(BytesText::new(text)))
            .await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::All;
    use crate::types as dav;
    use crate::versioningtypes as vers;
    use crate::xml::Node;
    use crate::xml::Reader;
    use tokio::io::AsyncWriteExt;

    async fn serialize_deserialize<T: Node<T>>(src: &T) {
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
        let res = rdr.find().await.unwrap();

        // check
        assert_eq!(src, &res);
    }

    #[tokio::test]
    async fn sync_level() {
        serialize_deserialize(&SyncLevel::One).await;
        serialize_deserialize(&SyncLevel::Infinite).await;
    }

    #[tokio::test]
    async fn sync_token_request() {
        serialize_deserialize(&SyncTokenRequest::InitialSync).await;
        serialize_deserialize(&SyncTokenRequest::IncrementalSync(
            "http://example.com/ns/sync/1232".into(),
        ))
        .await;
    }

    #[tokio::test]
    async fn sync_token() {
        serialize_deserialize(&SyncToken("http://example.com/ns/sync/1232".into())).await;
    }

    #[tokio::test]
    async fn sync_collection() {
        serialize_deserialize(&SyncCollection::<All> {
            sync_token: SyncTokenRequest::IncrementalSync("http://example.com/ns/sync/1232".into()),
            sync_level: SyncLevel::One,
            limit: Some(vers::Limit(vers::NResults(100))),
            prop: dav::PropName(vec![dav::PropertyRequest::GetEtag]),
        })
        .await;

        serialize_deserialize(&SyncCollection::<All> {
            sync_token: SyncTokenRequest::InitialSync,
            sync_level: SyncLevel::Infinite,
            limit: None,
            prop: dav::PropName(vec![dav::PropertyRequest::GetEtag]),
        })
        .await;
    }
}
