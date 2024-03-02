use super::encoder::{QuickWritable, Context};
use super::caltypes::*;
use super::types::Extension;

use quick_xml::Error as QError;
use quick_xml::events::{Event, BytesEnd, BytesStart, BytesText};
use quick_xml::writer::{ElementWriter, Writer};
use quick_xml::name::PrefixDeclaration;
use tokio::io::AsyncWrite;

const ICAL_DATETIME_FMT: &str = "%Y%m%dT%H%M%SZ";

// =============== Calendar Trait ===========================
pub trait CalContext: Context {
    fn create_cal_element(&self, name: &str) -> BytesStart;
}

// =============== CalDAV Extension Setup ===================
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

    async fn hook_property(&self, prop: &Self::Property, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        prop.write(xml, self.child()).await 
    }

    async fn hook_resourcetype(&self, restype: &Self::ResourceType, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        restype.write(xml, self.child()).await
    }

    async fn hook_propertyrequest(&self, propreq: &Self::PropertyRequest, xml: &mut Writer<impl AsyncWrite+Unpin>) -> Result<(), QError> {
        propreq.write(xml, self.child()).await 
    }
}

impl CalContext for CalExtension {
    fn create_cal_element(&self, name: &str) -> BytesStart {
        self.create_ns_element("C", name)
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
}

// ==================== Calendar Types Serialization =========================

// -------------------- MKCALENDAR METHOD ------------------------------------
impl<C: CalContext> QuickWritable<C> for MkCalendar<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("mkcalendar");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for MkCalendarResponse<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("mkcalendar-response");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        for propstat in self.0.iter() {
            propstat.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await
    }
}

// ----------------------- REPORT METHOD -------------------------------------

impl<C: CalContext> QuickWritable<C> for CalendarQuery<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("calendar-query");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.write(xml, ctx.child()).await?;
        }
        self.filter.write(xml, ctx.child()).await?;
        if let Some(tz) = &self.timezone  {
            tz.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarMultiget<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("calendar-multiget");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.write(xml, ctx.child()).await?;
        }
        for href in self.href.iter() {
            href.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for FreeBusyQuery {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("free-busy-query");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

// -------------------------- DAV::prop --------------------------------------
impl<C: CalContext> QuickWritable<C> for PropertyRequest {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut atom = async |c| xml.write_event_async(Event::Empty(ctx.create_cal_element(c))).await;

        match self {
            Self::CalendarDescription => atom("calendar-description").await,
            Self::CalendarTimezone => atom("calendar-timezone").await,
            Self::SupportedCalendarComponentSet => atom("supported-calendar-component-set").await,
            Self::SupportedCalendarData => atom("supported-calendar-data").await,
            Self::MaxResourceSize => atom("max-resource-size").await,
            Self::MinDateTime => atom("min-date-time").await,
            Self::MaxDateTime => atom("max-date-time").await,
            Self::MaxInstances => atom("max-instances").await,
            Self::MaxAttendeesPerInstance =>  atom("max-attendees-per-instance").await,
            Self::SupportedCollationSet =>  atom("supported-collation-set").await,    
            Self::CalendarData(req) => req.write(xml, ctx).await,
        }
    }
}
impl<C: CalContext> QuickWritable<C> for Property {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::CalendarDescription { lang, text } => {
                let mut start = ctx.create_cal_element("calendar-description");
                if let Some(the_lang) = lang {
                    start.push_attribute(("xml:lang", the_lang.as_str()));
                }
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(text))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::CalendarTimezone(payload) => {
                let start = ctx.create_cal_element("calendar-timezone");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(payload))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::SupportedCalendarComponentSet(many_comp) => {
                let start = ctx.create_cal_element("supported-calendar-component-set");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for comp in many_comp.iter() {
                    comp.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await
            },
            Self::SupportedCalendarData(many_mime) => {
                let start = ctx.create_cal_element("supported-calendar-data");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for mime in many_mime.iter() {
                    mime.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await
            },
            Self::MaxResourceSize(bytes) => {
                let start = ctx.create_cal_element("max-resource-size");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(bytes.to_string().as_str()))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::MinDateTime(dt) => {
                let start = ctx.create_cal_element("min-date-time");
                let end = start.to_end();

                let dtstr = format!("{}", dt.format(ICAL_DATETIME_FMT));
                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(dtstr.as_str()))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::MaxDateTime(dt) => {
                let start = ctx.create_cal_element("max-date-time");
                let end = start.to_end();

                let dtstr = format!("{}", dt.format(ICAL_DATETIME_FMT));
                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(dtstr.as_str()))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::MaxInstances(count) => {
                let start = ctx.create_cal_element("max-instances");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(count.to_string().as_str()))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::MaxAttendeesPerInstance(count) => {
                let start = ctx.create_cal_element("max-attendees-per-instance");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                xml.write_event_async(Event::Text(BytesText::new(count.to_string().as_str()))).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::SupportedCollationSet(many_collations) => {
                let start = ctx.create_cal_element("supported-collation-set");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for collation in many_collations.iter() {
                    collation.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await               
            },
            Self::CalendarData(inner) => inner.write(xml, ctx).await,
        }
    }
}

// ---------------------- DAV::resourcetype ----------------------------------
impl<C: CalContext> QuickWritable<C> for ResourceType {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::Calendar => xml.write_event_async(Event::Empty(ctx.create_dav_element("calendar"))).await,
        }
    }
}

// --------------------------- DAV::error ------------------------------------
impl<C: CalContext> QuickWritable<C> for Violation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::ResourceMustBeNull => {
                let start = ctx.create_cal_element("resource-must-be-null");
                xml.write_event_async(Event::Empty(start)).await?;
           },
            _ => unimplemented!(),
        };
        Ok(())
    }
}


// ---------------------------- Inner XML ------------------------------------
impl<C: CalContext> QuickWritable<C> for SupportedCollation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C>  for Collation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataPayload {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataRequest {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataEmpty {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataSupport {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for Comp {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CompSupport {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CompKind {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for PropKind {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CalProp {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for RecurrenceModifier {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for Expand {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for LimitRecurrenceSet {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for LimitFreebusySet {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C>  for CalendarSelector<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilterInner {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilterInner {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for TimeOrText {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for TextMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for ParamFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for ParamFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for TimeZone {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for Filter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for Component {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for ComponentProperty {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for PropertyParameter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}

impl<C: CalContext> QuickWritable<C> for TimeRange {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        unimplemented!();
    }
}


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
            DavViolation::Extension(Violation::ResourceMustBeNull),
        ]);

        res.write(&mut writer, CalExtension { root: true }).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");

        let expected = r#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <C:resource-must-be-null/>
</D:error>"#;
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        assert_eq!(got, expected);
    }
}
