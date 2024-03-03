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
        let mut atom = async |c| xml.write_event_async(Event::Empty(ctx.create_cal_element(c))).await;

        match self {
            //@FIXME
            // DAV elements, should not be here but in RFC3744 on ACLs
            // (we do not use atom as this error is in the DAV namespace, not the caldav one)
            Self::NeedPrivileges => xml.write_event_async(Event::Empty(ctx.create_dav_element("need-privileges"))).await,

            // Regular CalDAV errors
            Self::ResourceMustBeNull => atom("resource-must-be-null").await,
            Self::CalendarCollectionLocationOk => atom("calendar-collection-location-ok").await,
            Self::ValidCalendarData => atom("valid-calendar-data").await,
            Self::InitializeCalendarCollection => atom("initialize-calendar-collection").await,
            Self::SupportedCalendarData => atom("supported-calendar-data").await,
            Self::ValidCalendarObjectResource => atom("valid-calendar-object-resource").await,
            Self::SupportedCalendarComponent => atom("supported-calendar-component").await,
            Self::NoUidConflict(href) => {
                let start = ctx.create_cal_element("no-uid-conflict");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                href.write(xml, ctx.child()).await?;
                xml.write_event_async(Event::End(end)).await
            },
            Self::MaxResourceSize => atom("max-resource-size").await,
            Self::MinDateTime => atom("min-date-time").await,
            Self::MaxDateTime => atom("max-date-time").await,
            Self::MaxInstances => atom("max-instances").await,
            Self::MaxAttendeesPerInstance => atom("max-attendees-per-instance").await,
            Self::ValidFilter => atom("valid-filter").await,
            Self::SupportedFilter { comp, prop, param } => {
                let start = ctx.create_cal_element("supported-filter");
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                for comp_item in comp.iter() {
                    comp_item.write(xml, ctx.child()).await?;
                }
                for prop_item in prop.iter() {
                    prop_item.write(xml, ctx.child()).await?;
                }
                for param_item in param.iter() {
                    param_item.write(xml, ctx.child()).await?;
                }
                xml.write_event_async(Event::End(end)).await
            },
            Self::NumberOfMatchesWithinLimits => atom("number-of-matches-within-limits").await,
        }
    }
}


// ---------------------------- Inner XML ------------------------------------
impl<C: CalContext> QuickWritable<C> for SupportedCollation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let start = ctx.create_cal_element("supported-collation");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await

    }
}

impl<C: CalContext> QuickWritable<C>  for Collation {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let col = match self {
           Self::AsciiCaseMap => "i;ascii-casemap",
           Self::Octet => "i;octet",
           Self::Unknown(v) => v.as_str(),
        };

        xml.write_event_async(Event::Text(BytesText::new(col))).await
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataPayload {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("calendar-data");
        if let Some(mime) = &self.mime {
            start.push_attribute(("content-type", mime.content_type.as_str()));
            start.push_attribute(("version", mime.version.as_str()));
        }
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(self.payload.as_str()))).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataRequest {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("calendar-data");
        if let Some(mime) = &self.mime {
            start.push_attribute(("content-type", mime.content_type.as_str()));
            start.push_attribute(("version", mime.version.as_str()));
        }
        let end = start.to_end();
        xml.write_event_async(Event::Start(start.clone())).await?;
        if let Some(comp) = &self.comp {
            comp.write(xml, ctx.child()).await?;
        }
        if let Some(recurrence) = &self.recurrence {
            recurrence.write(xml, ctx.child()).await?;
        }
        if let Some(freebusy) = &self.limit_freebusy_set {
            freebusy.write(xml, ctx.child()).await?;
        }
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for CalendarDataEmpty {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("calendar-data");
        if let Some(mime) = &self.0 {
            empty.push_attribute(("content-type", mime.content_type.as_str()));
            empty.push_attribute(("version", mime.version.as_str()));
        }
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C> for Comp {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("comp");
        start.push_attribute(("name", self.name.as_str()));
        let end = start.to_end();
        xml.write_event_async(Event::Start(start.clone())).await?;
        self.prop_kind.write(xml, ctx.child()).await?;
        self.comp_kind.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for CompSupport {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("comp");
        empty.push_attribute(("name", self.0.as_str()));
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C> for CompKind {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::AllComp => xml.write_event_async(Event::Empty(ctx.create_cal_element("allcomp"))).await,
            Self::Comp(many_comp) => {
                for comp in many_comp.iter() {
                    // Required: recursion in an async fn requires boxing
                    // rustc --explain E0733
                    Box::pin(comp.write(xml, ctx.child())).await?;
                }
                Ok(())
            }
        }
    }
}

impl<C: CalContext> QuickWritable<C> for PropKind {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::AllProp => xml.write_event_async(Event::Empty(ctx.create_cal_element("allprop"))).await,
            Self::Prop(many_prop) => {
                for prop in many_prop.iter() {
                    prop.write(xml, ctx.child()).await?;
                }
                Ok(())
            }
        }
    }
}

impl<C: CalContext> QuickWritable<C> for CalProp {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("prop");
        empty.push_attribute(("name", self.name.0.as_str()));
        match self.novalue {
            None => (),
            Some(true) => empty.push_attribute(("novalue", "yes")),
            Some(false) => empty.push_attribute(("novalue", "no")),
        }
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C> for RecurrenceModifier {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::Expand(exp) => exp.write(xml, ctx).await,
            Self::LimitRecurrenceSet(lrs) => lrs.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for Expand {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("expand");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C> for LimitRecurrenceSet {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("limit-recurrence-set");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C> for LimitFreebusySet {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("limit-freebusy-set");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.write_event_async(Event::Empty(empty)).await
    }
}

impl<C: CalContext> QuickWritable<C>  for CalendarSelector<C> {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::AllProp => xml.write_event_async(Event::Empty(ctx.create_dav_element("allprop"))).await,
            Self::PropName => xml.write_event_async(Event::Empty(ctx.create_dav_element("propname"))).await,
            Self::Prop(prop) => prop.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("comp-filter");
        start.push_attribute(("name", self.name.as_str()));

        match &self.additional_rules {
            None => xml.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();

                xml.write_event_async(Event::Start(start.clone())).await?;
                rules.write(xml, ctx.child()).await?;
                xml.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilterRules {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::IsNotDefined =>  xml.write_event_async(Event::Empty(ctx.create_dav_element("is-not-defined"))).await,
            Self::Matches(cfm) => cfm.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for CompFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        if let Some(time_range) = &self.time_range {
            time_range.write(xml, ctx.child()).await?;
        }

        for prop_item in self.prop_filter.iter() {
            prop_item.write(xml, ctx.child()).await?;
        }
        for comp_item in self.comp_filter.iter() {
            // Required: recursion in an async fn requires boxing
            // rustc --explain E0733
            Box::pin(comp_item.write(xml, ctx.child())).await?;
        }
        Ok(())
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("prop-filter");
        start.push_attribute(("name", self.name.as_str()));

        match &self.additional_rules {
            None => xml.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();
                xml.write_event_async(Event::Start(start.clone())).await?;
                rules.write(xml, ctx.child()).await?;
                xml.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilterRules {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::IsNotDefined => xml.write_event_async(Event::Empty(ctx.create_dav_element("is-not-defined"))).await,
            Self::Match(prop_match) => prop_match.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for PropFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        if let Some(time_range) = &self.time_range {
            time_range.write(xml, ctx.child()).await?;
        }
        if let Some(time_or_text) = &self.time_or_text {
            time_or_text.write(xml, ctx.child()).await?;
        }
        for param_item in self.param_filter.iter() {
            param_item.write(xml, ctx.child()).await?;
        }
        Ok(())
    }
}

impl<C: CalContext> QuickWritable<C> for TimeOrText {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::Time(time) => time.write(xml, ctx).await,
            Self::Text(txt) => txt.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for TextMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("text-match");
        if let Some(collation) = &self.collation {
            start.push_attribute(("collation", collation.as_str()));
        }
        match self.negate_condition {
            None => (),
            Some(true) => start.push_attribute(("negate-condition", "yes")),
            Some(false) => start.push_attribute(("negate-condition", "no")),
        }
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(self.text.as_str()))).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for ParamFilter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("param-filter");
        start.push_attribute(("name", self.name.as_str()));

        match &self.additional_rules {
            None => xml.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();
                xml.write_event_async(Event::Start(start.clone())).await?;
                rules.write(xml, ctx.child()).await?;
                xml.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl<C: CalContext> QuickWritable<C> for ParamFilterMatch {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        match self {
            Self::IsNotDefined =>  xml.write_event_async(Event::Empty(ctx.create_dav_element("is-not-defined"))).await,
            Self::Match(tm) => tm.write(xml, ctx).await,
        }
    }
}

impl<C: CalContext> QuickWritable<C> for TimeZone {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("timezone");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        xml.write_event_async(Event::Text(BytesText::new(self.0.as_str()))).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for Filter {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut start = ctx.create_cal_element("filter");
        let end = start.to_end();

        xml.write_event_async(Event::Start(start.clone())).await?;
        self.0.write(xml, ctx.child()).await?;
        xml.write_event_async(Event::End(end)).await
    }
}

impl<C: CalContext> QuickWritable<C> for TimeRange {
    async fn write(&self, xml: &mut Writer<impl AsyncWrite+Unpin>, ctx: C) -> Result<(), QError> {
        let mut empty = ctx.create_cal_element("time-range");
        match self {
            Self::OnlyStart(start) => empty.push_attribute(("start", format!("{}", start.format(ICAL_DATETIME_FMT)).as_str())),
            Self::OnlyEnd(end) => empty.push_attribute(("end", format!("{}", end.format(ICAL_DATETIME_FMT)).as_str())),
            Self::FullRange(start, end) => {
                empty.push_attribute(("start", format!("{}", start.format(ICAL_DATETIME_FMT)).as_str()));
                empty.push_attribute(("end", format!("{}", end.format(ICAL_DATETIME_FMT)).as_str()));
            }
        }
        xml.write_event_async(Event::Empty(empty)).await
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::dav::types as dav;
    use tokio::io::AsyncWriteExt;

    async fn serialize<C: Context, Q: QuickWritable<C>>(ctx: C, elem: &Q) -> String {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let mut writer = Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
        elem.write(&mut writer, ctx).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        return got.into()
    }

    #[tokio::test]
    async fn basic_violation() {
        let got = serialize(
            CalExtension { root: true },
            &dav::Error(vec![
                dav::Violation::Extension(Violation::ResourceMustBeNull),
            ])
        ).await;

        let expected = r#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <C:resource-must-be-null/>
</D:error>"#;

        assert_eq!(&got, expected, "\n---GOT---\n{got}\n---EXP---\n{expected}\n");
    }

    #[tokio::test]
    async fn rfc_calendar_query1() {
        let got = serialize(
            CalExtension { root: true },
            &CalendarQuery {
                selector: Some(CalendarSelector::Prop(dav::PropName(vec![
                    dav::PropertyRequest::GetEtag,
                    dav::PropertyRequest::Extension(PropertyRequest::CalendarData(CalendarDataRequest {
                        mime: None,
                        comp: Some(Comp {
                            name: Component::VCalendar,
                            prop_kind: PropKind::Prop(vec![
                                CalProp {
                                    name: ComponentProperty("VERSION".into()),
                                    novalue: None,
                                }
                            ]),
                            comp_kind: CompKind::Comp(vec![
                                Comp {
                                    name: Component::VEvent,
                                    comp_kind: CompKind::Comp(vec![]),
                                    prop_kind: PropKind::Prop(vec![
                                        CalProp { name: ComponentProperty("SUMMARY".into()), novalue: None },
                                    ]),
                                },
                                Comp {
                                    name: Component::VTimeZone,
                                    prop_kind: PropKind::Prop(vec![]),
                                    comp_kind: CompKind::Comp(vec![]),
                                }
                            ]),        
                        }),
                        recurrence: None,
                        limit_freebusy_set: None,
                    })),
                ]))),
                filter: Filter(CompFilter {
                    name: Component::VCalendar,
                    additional_rules: None,
                }),
                timezone: None,
            }
        ).await;

        let expected = r#"<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <D:prop>
        <D:getetag/>
        <C:calendar-data>
            <C:comp name="VCALENDAR">
                <C:prop name="VERSION"/>
                <C:comp name="VEVENT">
                    <C:prop name="SUMMARY"/>
                    <C:prop name="UID"/>
                    <C:prop name="DTSTART"/>
                    <C:prop name="DTEND"/>
                    <C:prop name="DURATION"/>
                    <C:prop name="RRULE"/>
                    <C:prop name="RDATE"/>
                    <C:prop name="EXRULE"/>
                    <C:prop name="EXDATE"/>
                    <C:prop name="RECURRENCE-ID"/>
                </C:comp>
                <C:comp name="VTIMEZONE"/>
            </C:comp>
        </C:calendar-data>
    </D:prop>
    <C:filter>
        <C:comp-filter name="VCALENDAR">
            <C:comp-filter name="VEVENT">
                <C:time-range start="20060104T000000Z" end="20060105T000000Z"/>
            </C:comp-filter>
        </C:comp-filter>
    </C:filter>
</C:calendar-query>"#;
        
        assert_eq!(&got, expected, "\n---GOT---\n{got}\n---EXP---\n{expected}\n");
    }
}
