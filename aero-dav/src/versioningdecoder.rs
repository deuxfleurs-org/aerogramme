use super::error::ParsingError;
use super::types as dav;
use super::versioningtypes::*;
use super::xml::{IRead, QRead, Reader, DAV_URN};

// -- extensions ---
impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(DAV_URN, "supported-report-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedReportSet);
        }
        return Err(ParsingError::Recoverable);
    }
}

impl<E: dav::Extension> QRead<Property<E>> for Property<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(DAV_URN, "supported-report-set")
            .await?
            .is_some()
        {
            let supported_reports = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedReportSet(supported_reports));
        }
        Err(ParsingError::Recoverable)
    }
}

impl<E: dav::Extension> QRead<SupportedReport<E>> for SupportedReport<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "supported-report").await?;
        let r = xml.find().await?;
        xml.close().await?;
        Ok(SupportedReport(r))
    }
}

impl<E: dav::Extension> QRead<ReportName<E>> for ReportName<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(DAV_URN, "report").await?;

        let final_result = if xml.maybe_open(DAV_URN, "version-tree").await?.is_some() {
            xml.close().await?;
            Ok(ReportName::VersionTree)
        } else if xml.maybe_open(DAV_URN, "expand-property").await?.is_some() {
            xml.close().await?;
            Ok(ReportName::ExpandProperty)
        } else {
            let x = match xml.maybe_find().await? {
                Some(v) => v,
                None => return Err(ParsingError::MissingChild),
            };
            Ok(ReportName::Extension(x))
            //E::ReportTypeName::qread(xml).await.map(ReportName::Extension)
        };

        xml.close().await?;
        final_result
    }
}

impl<E: dav::Extension> QRead<Report<E>> for Report<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "version-tree").await?.is_some() {
            xml.close().await?;
            tracing::warn!("version-tree is not implemented, skipping");
            Ok(Report::VersionTree)
        } else if xml.maybe_open(DAV_URN, "expand-property").await?.is_some() {
            xml.close().await?;
            tracing::warn!("expand-property is not implemented, skipping");
            Ok(Report::ExpandProperty)
        } else {
            E::ReportType::qread(xml).await.map(Report::Extension)
        }
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
