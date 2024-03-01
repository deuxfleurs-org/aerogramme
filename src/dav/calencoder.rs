use super::encoder::{QuickWritable, Context};
use super::caltypes::*;
use super::types::Extension;

use quick_xml::Error as QError;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;

impl Context for CalExtension {
    fn child(&self) -> Self {
        Self { root: false }
    }
    fn create_dav_element(&self, name: &str) -> BytesStart {
        self.create_ns_element("D", name)
    }

    async fn hook_error(&self, err: &Violation, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        err.write(xml, self.child()).await
    }
}

impl CalExtension {
    fn create_ns_element(&self, ns: &str, name: &str) -> BytesStart {
        let mut start = BytesStart::new(format!("{}:{}", ns, name));
        if self.root {
            start.push_attribute(("xmlns:D", "DAV:"));
            start.push_attribute(("xmlns:C", "urn:ietf:params:xml:ns:caldav"));
        }
        start
    }
    fn create_cal_element(&self, name: &str) -> BytesStart {
        self.create_ns_element("C", name)
    }
}

impl QuickWritable<CalExtension> for Violation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: CalExtension) -> Result<(), QError> {
        match self {
            Self::SupportedFilter => {
                let start = ctx.create_cal_element("supported-filter");
                xml.write_event_async(Event::Empty(start)).await?;
           },
        };
        Ok(())
    }
}

/*
 <?xml version="1.0" encoding="utf-8" ?>
   <D:error>
     <C:supported-filter>
       <C:prop-filter name="X-ABC-GUID"/>
     </C:supported-filter>
   </D:error>
*/

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dav::types::{Error, Violation as DavViolation};
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_violation() {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);

        let res = Error(vec![
            DavViolation::Extension(Violation::SupportedFilter),
        ]);

        res.write(&mut writer, CalExtension { root: true }).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        let expected = r#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <C:supported-filter/>
</D:error>"#;
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        assert_eq!(got, expected);
    }
}
