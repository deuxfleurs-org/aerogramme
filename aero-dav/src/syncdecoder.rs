use quick_xml::events::Event;

use super::error::ParsingError;
use super::synctypes::*;
use super::types as dav;
use super::xml::{IRead, QRead, Reader, DAV_URN};

impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "sync-token").await?.is_some() {
            xml.close().await?;
            return Ok(Self::SyncToken);
        }
        return Err(ParsingError::Recoverable);
    }
}

impl QRead<Property> for Property {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut dirty = false;
        let mut m_cdr = None;
        xml.maybe_read(&mut m_cdr, &mut dirty).await?;
        m_cdr.ok_or(ParsingError::Recoverable).map(Self::SyncToken)
    }
}

impl QRead<ReportTypeName> for ReportTypeName {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "sync-collection").await?.is_some() {
            xml.close().await?;
            return Ok(Self::SyncCollection);
        }
        Err(ParsingError::Recoverable)
    }
}

impl QRead<Multistatus> for Multistatus {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        SyncToken::qread(xml)
            .await
            .map(|sync_token| Multistatus { sync_token })
    }
}

impl<E: dav::Extension> QRead<SyncCollection<E>> for SyncCollection<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "sync-collection").await?;
        let (mut sync_token, mut sync_level, mut limit, mut prop) = (None, None, None, None);
        loop {
            let mut dirty = false;
            xml.maybe_read(&mut sync_token, &mut dirty).await?;
            xml.maybe_read(&mut sync_level, &mut dirty).await?;
            xml.maybe_read(&mut limit, &mut dirty).await?;
            xml.maybe_read(&mut prop, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        match (sync_token, sync_level, prop) {
            (Some(sync_token), Some(sync_level), Some(prop)) => Ok(SyncCollection {
                sync_token,
                sync_level,
                limit,
                prop,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<SyncTokenRequest> for SyncTokenRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "sync-token").await?;
        let token = match xml.tag_string().await {
            Ok(v) => SyncTokenRequest::IncrementalSync(v),
            Err(ParsingError::Recoverable) => SyncTokenRequest::InitialSync,
            Err(e) => return Err(e),
        };
        xml.close().await?;
        Ok(token)
    }
}

impl QRead<SyncToken> for SyncToken {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "sync-token").await?;
        let token = xml.tag_string().await?;
        xml.close().await?;
        Ok(SyncToken(token))
    }
}

impl QRead<SyncLevel> for SyncLevel {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "sync-level").await?;
        let lvl = match xml.tag_string().await?.to_lowercase().as_str() {
            "1" => SyncLevel::One,
            "infinite" => SyncLevel::Infinite,
            _ => return Err(ParsingError::InvalidValue),
        };
        xml.close().await?;
        Ok(lvl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::{self, All};
    use crate::types as dav;
    use crate::versioningtypes as vers;
    use crate::xml::Node;

    async fn deserialize<T: Node<T>>(src: &str) -> T {
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn sync_level() {
        {
            let expected = SyncLevel::One;
            let src = r#"<D:sync-level xmlns:D="DAV:">1</D:sync-level>"#;
            let got = deserialize::<SyncLevel>(src).await;
            assert_eq!(got, expected);
        }
        {
            let expected = SyncLevel::Infinite;
            let src = r#"<D:sync-level xmlns:D="DAV:">infinite</D:sync-level>"#;
            let got = deserialize::<SyncLevel>(src).await;
            assert_eq!(got, expected);
        }
    }

    #[tokio::test]
    async fn sync_token_request() {
        {
            let expected = SyncTokenRequest::InitialSync;
            let src = r#"<D:sync-token xmlns:D="DAV:"/>"#;
            let got = deserialize::<SyncTokenRequest>(src).await;
            assert_eq!(got, expected);
        }
        {
            let expected =
                SyncTokenRequest::IncrementalSync("http://example.com/ns/sync/1232".into());
            let src =
                r#"<D:sync-token xmlns:D="DAV:">http://example.com/ns/sync/1232</D:sync-token>"#;
            let got = deserialize::<SyncTokenRequest>(src).await;
            assert_eq!(got, expected);
        }
    }

    #[tokio::test]
    async fn sync_token() {
        let expected = SyncToken("http://example.com/ns/sync/1232".into());
        let src = r#"<D:sync-token xmlns:D="DAV:">http://example.com/ns/sync/1232</D:sync-token>"#;
        let got = deserialize::<SyncToken>(src).await;
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn sync_collection() {
        {
            let expected = SyncCollection::<All> {
                sync_token: SyncTokenRequest::IncrementalSync(
                    "http://example.com/ns/sync/1232".into(),
                ),
                sync_level: SyncLevel::One,
                limit: Some(vers::Limit(vers::NResults(100))),
                prop: dav::PropName(vec![dav::PropertyRequest::GetEtag]),
            };
            let src = r#"<D:sync-collection xmlns:D="DAV:">
                <D:sync-token>http://example.com/ns/sync/1232</D:sync-token>
                <D:sync-level>1</D:sync-level>
                <D:limit>
                    <D:nresults>100</D:nresults>
                </D:limit>
                <D:prop>
                    <D:getetag/>
                </D:prop>
            </D:sync-collection>"#;
            let got = deserialize::<SyncCollection<All>>(src).await;
            assert_eq!(got, expected);
        }

        {
            let expected = SyncCollection::<All> {
                sync_token: SyncTokenRequest::InitialSync,
                sync_level: SyncLevel::Infinite,
                limit: None,
                prop: dav::PropName(vec![dav::PropertyRequest::GetEtag]),
            };
            let src = r#"<D:sync-collection xmlns:D="DAV:">
                <D:sync-token/>
                <D:sync-level>infinite</D:sync-level>
                <D:prop>
                    <D:getetag/>
                </D:prop>
            </D:sync-collection>"#;
            let got = deserialize::<SyncCollection<All>>(src).await;
            assert_eq!(got, expected);
        }
    }

    #[tokio::test]
    async fn prop_req() {
        let expected = dav::PropName::<All>(vec![dav::PropertyRequest::Extension(
            realization::PropertyRequest::Sync(PropertyRequest::SyncToken),
        )]);
        let src = r#"<prop xmlns="DAV:"><sync-token/></prop>"#;
        let got = deserialize::<dav::PropName<All>>(src).await;
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn prop_val() {
        let expected = dav::PropValue::<All>(vec![
            dav::Property::Extension(realization::Property::Sync(Property::SyncToken(SyncToken(
                "http://example.com/ns/sync/1232".into(),
            )))),
            dav::Property::Extension(realization::Property::Vers(
                vers::Property::SupportedReportSet(vec![vers::SupportedReport(
                    vers::ReportName::Extension(realization::ReportTypeName::Sync(
                        ReportTypeName::SyncCollection,
                    )),
                )]),
            )),
        ]);
        let src = r#"<prop xmlns="DAV:">
            <sync-token>http://example.com/ns/sync/1232</sync-token>
            <supported-report-set>
                <supported-report>
                    <report><sync-collection/></report>
                </supported-report>
            </supported-report-set>
        </prop>"#;
        let got = deserialize::<dav::PropValue<All>>(src).await;
        assert_eq!(got, expected);
    }
}
