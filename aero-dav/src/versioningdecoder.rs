use super::error::ParsingError;
use super::types as dav;
use super::versioningtypes::*;
use super::xml::{IRead, QRead, Reader, DAV_URN};

impl<E: dav::Extension> QRead<Report<E>> for Report<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        //@FIXME VersionTree not implemented
        //@FIXME ExpandTree not implemented

        E::ReportType::qread(xml).await.map(Report::Extension)
    }
}

impl QRead<Limit> for Limit {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "limit").await?;
        let nres = xml.find().await?;
        xml.close().await?;
        Ok(Limit(nres))
    }
}

impl QRead<NResults> for NResults {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "nresults").await?;
        let sz = xml.tag_string().await?.parse::<u64>()?;
        xml.close().await?;
        Ok(NResults(sz))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::Node;

    async fn deserialize<T: Node<T>>(src: &str) -> T {
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn nresults() {
        let expected = NResults(100);
        let src = r#"<D:nresults xmlns:D="DAV:">100</D:nresults>"#;
        let got = deserialize::<NResults>(src).await;
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn limit() {
        let expected = Limit(NResults(1024));
        let src = r#"<D:limit xmlns:D="DAV:">
            <D:nresults>1024</D:nresults>
        </D:limit>"#;
        let got = deserialize::<Limit>(src).await;
        assert_eq!(got, expected);
    }
}
