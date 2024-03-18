use quick_xml::Error as QError;
use quick_xml::events::{Event, BytesText};

use super::caltypes::*;
use super::xml::{Node, QWrite, IWrite, Writer};
use super::types::Extension;


// ==================== Calendar Types Serialization =========================

// -------------------- MKCALENDAR METHOD ------------------------------------
impl<E: Extension> QWrite for MkCalendar<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("mkcalendar");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for MkCalendarResponse<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("mkcalendar-response");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        for propstat in self.0.iter() {
            propstat.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

// ----------------------- REPORT METHOD -------------------------------------

impl<E: Extension> QWrite for CalendarQuery<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("calendar-query");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.qwrite(xml).await?;
        }
        self.filter.qwrite(xml).await?;
        if let Some(tz) = &self.timezone  {
            tz.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl<E: Extension> QWrite for CalendarMultiget<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("calendar-multiget");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(selector) = &self.selector {
            selector.qwrite(xml).await?;
        }
        for href in self.href.iter() {
            href.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for FreeBusyQuery {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("free-busy-query");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

// -------------------------- DAV::prop --------------------------------------
impl QWrite for PropertyRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_cal_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

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
            Self::CalendarData(req) => req.qwrite(xml).await,
        }
    }
}
impl QWrite for Property {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::CalendarDescription { lang, text } => {
                let mut start = xml.create_cal_element("calendar-description");
                if let Some(the_lang) = lang {
                    start.push_attribute(("xml:lang", the_lang.as_str()));
                }
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(text))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::CalendarTimezone(payload) => {
                let start = xml.create_cal_element("calendar-timezone");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(payload))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::SupportedCalendarComponentSet(many_comp) => {
                let start = xml.create_cal_element("supported-calendar-component-set");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for comp in many_comp.iter() {
                    comp.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::SupportedCalendarData(many_mime) => {
                let start = xml.create_cal_element("supported-calendar-data");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for mime in many_mime.iter() {
                    mime.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MaxResourceSize(bytes) => {
                let start = xml.create_cal_element("max-resource-size");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(bytes.to_string().as_str()))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MinDateTime(dt) => {
                let start = xml.create_cal_element("min-date-time");
                let end = start.to_end();

                let dtstr = format!("{}", dt.format(ICAL_DATETIME_FMT));
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(dtstr.as_str()))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MaxDateTime(dt) => {
                let start = xml.create_cal_element("max-date-time");
                let end = start.to_end();

                let dtstr = format!("{}", dt.format(ICAL_DATETIME_FMT));
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(dtstr.as_str()))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MaxInstances(count) => {
                let start = xml.create_cal_element("max-instances");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(count.to_string().as_str()))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MaxAttendeesPerInstance(count) => {
                let start = xml.create_cal_element("max-attendees-per-instance");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                xml.q.write_event_async(Event::Text(BytesText::new(count.to_string().as_str()))).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::SupportedCollationSet(many_collations) => {
                let start = xml.create_cal_element("supported-collation-set");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for collation in many_collations.iter() {
                    collation.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await               
            },
            Self::CalendarData(inner) => inner.qwrite(xml).await,
        }
    }
}

// ---------------------- DAV::resourcetype ----------------------------------
impl QWrite for ResourceType {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Calendar => {
                let empty_tag = xml.create_cal_element("calendar");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
        }
    }
}

// --------------------------- DAV::error ------------------------------------
impl QWrite for Violation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut atom = async |c| {
            let empty_tag = xml.create_cal_element(c);
            xml.q.write_event_async(Event::Empty(empty_tag)).await
        };

        match self {
            //@FIXME
            // DAV elements, should not be here but in RFC3744 on ACLs
            // (we do not use atom as this error is in the DAV namespace, not the caldav one)
            Self::NeedPrivileges => {
                let empty_tag = xml.create_dav_element("need-privileges");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },

            // Regular CalDAV errors
            Self::ResourceMustBeNull => atom("resource-must-be-null").await,
            Self::CalendarCollectionLocationOk => atom("calendar-collection-location-ok").await,
            Self::ValidCalendarData => atom("valid-calendar-data").await,
            Self::InitializeCalendarCollection => atom("initialize-calendar-collection").await,
            Self::SupportedCalendarData => atom("supported-calendar-data").await,
            Self::ValidCalendarObjectResource => atom("valid-calendar-object-resource").await,
            Self::SupportedCalendarComponent => atom("supported-calendar-component").await,
            Self::NoUidConflict(href) => {
                let start = xml.create_cal_element("no-uid-conflict");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                href.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::MaxResourceSize => atom("max-resource-size").await,
            Self::MinDateTime => atom("min-date-time").await,
            Self::MaxDateTime => atom("max-date-time").await,
            Self::MaxInstances => atom("max-instances").await,
            Self::MaxAttendeesPerInstance => atom("max-attendees-per-instance").await,
            Self::ValidFilter => atom("valid-filter").await,
            Self::SupportedFilter { comp, prop, param } => {
                let start = xml.create_cal_element("supported-filter");
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                for comp_item in comp.iter() {
                    comp_item.qwrite(xml).await?;
                }
                for prop_item in prop.iter() {
                    prop_item.qwrite(xml).await?;
                }
                for param_item in param.iter() {
                    param_item.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            },
            Self::NumberOfMatchesWithinLimits => atom("number-of-matches-within-limits").await,
        }
    }
}


// ---------------------------- Inner XML ------------------------------------
impl QWrite for SupportedCollation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("supported-collation");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await

    }
}

impl QWrite for Collation {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let col = match self {
           Self::AsciiCaseMap => "i;ascii-casemap",
           Self::Octet => "i;octet",
           Self::Unknown(v) => v.as_str(),
        };

        xml.q.write_event_async(Event::Text(BytesText::new(col))).await
    }
}

impl QWrite for CalendarDataSupport {
    async fn qwrite(&self, _xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        unreachable!();
    }
}

impl QWrite for CalendarDataPayload {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("calendar-data");
        if let Some(mime) = &self.mime {
            start.push_attribute(("content-type", mime.content_type.as_str()));
            start.push_attribute(("version", mime.version.as_str()));
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q.write_event_async(Event::Text(BytesText::new(self.payload.as_str()))).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for CalendarDataRequest {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("calendar-data");
        if let Some(mime) = &self.mime {
            start.push_attribute(("content-type", mime.content_type.as_str()));
            start.push_attribute(("version", mime.version.as_str()));
        }
        let end = start.to_end();
        xml.q.write_event_async(Event::Start(start.clone())).await?;
        if let Some(comp) = &self.comp {
            comp.qwrite(xml).await?;
        }
        if let Some(recurrence) = &self.recurrence {
            recurrence.qwrite(xml).await?;
        }
        if let Some(freebusy) = &self.limit_freebusy_set {
            freebusy.qwrite(xml).await?;
        }
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for CalendarDataEmpty {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("calendar-data");
        if let Some(mime) = &self.0 {
            empty.push_attribute(("content-type", mime.content_type.as_str()));
            empty.push_attribute(("version", mime.version.as_str()));
        }
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl QWrite for Comp {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("comp");
        start.push_attribute(("name", self.name.as_str()));
        match (&self.prop_kind, &self.comp_kind) {
            (None, None) => xml.q.write_event_async(Event::Empty(start)).await,
            _ => {
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                if let Some(prop_kind) = &self.prop_kind {
                    prop_kind.qwrite(xml).await?;
                }
                if let Some(comp_kind) = &self.comp_kind {
                    comp_kind.qwrite(xml).await?;
                }
                xml.q.write_event_async(Event::End(end)).await
            },
        }
    }
}

impl QWrite for CompSupport {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("comp");
        empty.push_attribute(("name", self.0.as_str()));
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl QWrite for CompKind {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AllComp => {
                let empty_tag = xml.create_cal_element("allcomp");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Comp(many_comp) => {
                for comp in many_comp.iter() {
                    // Required: recursion in an async fn requires boxing
                    // rustc --explain E0733
                    // Cycle detected when computing type of ...
                    // For more information about this error, try `rustc --explain E0391`.
                    // https://github.com/rust-lang/rust/issues/78649
                    #[inline(always)]
                    fn recurse<'a>(comp: &'a Comp, xml: &'a mut Writer<impl IWrite>) -> futures::future::BoxFuture<'a, Result<(), QError>> {
                        Box::pin(comp.qwrite(xml))
                    }
                    recurse(comp, xml).await?;
                }
                Ok(())
            }
        }
    }
}

impl QWrite for PropKind {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AllProp => {
                let empty_tag = xml.create_cal_element("allprop");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Prop(many_prop) => {
                for prop in many_prop.iter() {
                    prop.qwrite(xml).await?;
                }
                Ok(())
            }
        }
    }
}

impl QWrite for CalProp {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("prop");
        empty.push_attribute(("name", self.name.0.as_str()));
        match self.novalue {
            None => (),
            Some(true) => empty.push_attribute(("novalue", "yes")),
            Some(false) => empty.push_attribute(("novalue", "no")),
        }
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl QWrite for RecurrenceModifier {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Expand(exp) => exp.qwrite(xml).await,
            Self::LimitRecurrenceSet(lrs) => lrs.qwrite(xml).await,
        }
    }
}

impl QWrite for Expand {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("expand");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl QWrite for LimitRecurrenceSet {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("limit-recurrence-set");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl QWrite for LimitFreebusySet {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("limit-freebusy-set");
        empty.push_attribute(("start", format!("{}", self.0.format(ICAL_DATETIME_FMT)).as_str()));
        empty.push_attribute(("end", format!("{}", self.1.format(ICAL_DATETIME_FMT)).as_str()));
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

impl<E: Extension> QWrite for CalendarSelector<E> {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::AllProp => {
                let empty_tag = xml.create_dav_element("allprop");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::PropName => {
                let empty_tag = xml.create_dav_element("propname");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Prop(prop) => prop.qwrite(xml).await,
        }
    }
}

impl QWrite for CompFilter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("comp-filter");
        start.push_attribute(("name", self.name.as_str()));

        match &self.additional_rules {
            None => xml.q.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();

                xml.q.write_event_async(Event::Start(start.clone())).await?;
                rules.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for CompFilterRules {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::IsNotDefined =>  {
                let empty_tag = xml.create_dav_element("is-not-defined");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Matches(cfm) => cfm.qwrite(xml).await,
        }
    }
}

impl QWrite for CompFilterMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        if let Some(time_range) = &self.time_range {
            time_range.qwrite(xml).await?;
        }

        for prop_item in self.prop_filter.iter() {
            prop_item.qwrite(xml).await?;
        }
        for comp_item in self.comp_filter.iter() {
            // Required: recursion in an async fn requires boxing
            // rustc --explain E0733
            // Cycle detected when computing type of ...
            // For more information about this error, try `rustc --explain E0391`.
            // https://github.com/rust-lang/rust/issues/78649
            #[inline(always)]
            fn recurse<'a>(comp: &'a CompFilter, xml: &'a mut Writer<impl IWrite>) -> futures::future::BoxFuture<'a, Result<(), QError>> {
                Box::pin(comp.qwrite(xml))
            }
            recurse(comp_item, xml).await?;
        }
        Ok(())
    }
}

impl QWrite for PropFilter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("prop-filter");
        start.push_attribute(("name", self.name.0.as_str()));

        match &self.additional_rules {
            None => xml.q.write_event_async(Event::Empty(start.clone())).await,
            Some(rules) => {
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                rules.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for PropFilterRules {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::IsNotDefined => {
                let empty_tag = xml.create_dav_element("is-not-defined");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Match(prop_match) => prop_match.qwrite(xml).await,
        }
    }
}

impl QWrite for PropFilterMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        if let Some(time_range) = &self.time_range {
            time_range.qwrite(xml).await?;
        }
        if let Some(time_or_text) = &self.time_or_text {
            time_or_text.qwrite(xml).await?;
        }
        for param_item in self.param_filter.iter() {
            param_item.qwrite(xml).await?;
        }
        Ok(())
    }
}

impl QWrite for TimeOrText {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::Time(time) => time.qwrite(xml).await,
            Self::Text(txt) => txt.qwrite(xml).await,
        }
    }
}

impl QWrite for TextMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("text-match");
        if let Some(collation) = &self.collation {
            start.push_attribute(("collation", collation.as_str()));
        }
        match self.negate_condition {
            None => (),
            Some(true) => start.push_attribute(("negate-condition", "yes")),
            Some(false) => start.push_attribute(("negate-condition", "no")),
        }
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q.write_event_async(Event::Text(BytesText::new(self.text.as_str()))).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for ParamFilter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut start = xml.create_cal_element("param-filter");
        start.push_attribute(("name", self.name.as_str()));

        match &self.additional_rules {
            None => xml.q.write_event_async(Event::Empty(start)).await,
            Some(rules) => {
                let end = start.to_end();
                xml.q.write_event_async(Event::Start(start.clone())).await?;
                rules.qwrite(xml).await?;
                xml.q.write_event_async(Event::End(end)).await
            }
        }
    }
}

impl QWrite for ParamFilterMatch {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        match self {
            Self::IsNotDefined => {
                let empty_tag = xml.create_dav_element("is-not-defined");
                xml.q.write_event_async(Event::Empty(empty_tag)).await
            },
            Self::Match(tm) => tm.qwrite(xml).await,
        }
    }
}

impl QWrite for TimeZone {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("timezone");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        xml.q.write_event_async(Event::Text(BytesText::new(self.0.as_str()))).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for Filter {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let start = xml.create_cal_element("filter");
        let end = start.to_end();

        xml.q.write_event_async(Event::Start(start.clone())).await?;
        self.0.qwrite(xml).await?;
        xml.q.write_event_async(Event::End(end)).await
    }
}

impl QWrite for TimeRange {
    async fn qwrite(&self, xml: &mut Writer<impl IWrite>) -> Result<(), QError> {
        let mut empty = xml.create_cal_element("time-range");
        match self {
            Self::OnlyStart(start) => empty.push_attribute(("start", format!("{}", start.format(ICAL_DATETIME_FMT)).as_str())),
            Self::OnlyEnd(end) => empty.push_attribute(("end", format!("{}", end.format(ICAL_DATETIME_FMT)).as_str())),
            Self::FullRange(start, end) => {
                empty.push_attribute(("start", format!("{}", start.format(ICAL_DATETIME_FMT)).as_str()));
                empty.push_attribute(("end", format!("{}", end.format(ICAL_DATETIME_FMT)).as_str()));
            }
        }
        xml.q.write_event_async(Event::Empty(empty)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types as dav;
    use crate::realization::Calendar;
    use tokio::io::AsyncWriteExt;
    use chrono::{Utc,TimeZone};

    async fn serialize(elem: &impl QWrite) -> String {
        let mut buffer = Vec::new();
        let mut tokio_buffer = tokio::io::BufWriter::new(&mut buffer);
        let q = quick_xml::writer::Writer::new_with_indent(&mut tokio_buffer, b' ', 4);
        let ns_to_apply = vec![ 
            ("xmlns:D".into(), "DAV:".into()),
            ("xmlns:C".into(), "urn:ietf:params:xml:ns:caldav".into()),
        ];
        let mut writer = Writer { q, ns_to_apply };

        elem.qwrite(&mut writer).await.expect("xml serialization");
        tokio_buffer.flush().await.expect("tokio buffer flush");
        let got = std::str::from_utf8(buffer.as_slice()).unwrap();

        return got.into()
    }

    #[tokio::test]
    async fn basic_violation() {
        let got = serialize(
            &dav::Error::<Calendar>(vec![
                dav::Violation::Extension(Violation::ResourceMustBeNull),
            ])
        ).await;

        let expected = r#"<D:error xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <C:resource-must-be-null/>
</D:error>"#;

        assert_eq!(&got, expected, "\n---GOT---\n{got}\n---EXP---\n{expected}\n");
    }

    #[tokio::test]
    async fn rfc_calendar_query1_req() {
        let got = serialize(
            &CalendarQuery::<Calendar> {
                selector: Some(CalendarSelector::Prop(dav::PropName(vec![
                    dav::PropertyRequest::GetEtag,
                    dav::PropertyRequest::Extension(PropertyRequest::CalendarData(CalendarDataRequest {
                        mime: None,
                        comp: Some(Comp {
                            name: Component::VCalendar,
                            prop_kind: Some(PropKind::Prop(vec![
                                    CalProp {
                                        name: ComponentProperty("VERSION".into()),
                                        novalue: None,
                                    }
                                ])),
                            comp_kind: Some(CompKind::Comp(vec![
                                    Comp {
                                        name: Component::VEvent,
                                        prop_kind: Some(PropKind::Prop(vec![
                                            CalProp { name: ComponentProperty("SUMMARY".into()), novalue: None },
                                            CalProp { name: ComponentProperty("UID".into()), novalue: None },
                                            CalProp { name: ComponentProperty("DTSTART".into()), novalue: None },
                                            CalProp { name: ComponentProperty("DTEND".into()), novalue: None },
                                            CalProp { name: ComponentProperty("DURATION".into()), novalue: None },
                                            CalProp { name: ComponentProperty("RRULE".into()), novalue: None },
                                            CalProp { name: ComponentProperty("RDATE".into()), novalue: None },
                                            CalProp { name: ComponentProperty("EXRULE".into()), novalue: None },
                                            CalProp { name: ComponentProperty("EXDATE".into()), novalue: None },
                                            CalProp { name: ComponentProperty("RECURRENCE-ID".into()), novalue: None },
                                        ])),
                                        comp_kind: None,
                                    },
                                    Comp {
                                        name: Component::VTimeZone,
                                        prop_kind: None,
                                        comp_kind: None,
                                    }
                                ])),
                            }),
                        recurrence: None,
                        limit_freebusy_set: None,
                    })),
                ]))),
                filter: Filter(CompFilter {
                    name: Component::VCalendar,
                    additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                        time_range: None,
                        prop_filter: vec![],
                        comp_filter: vec![
                            CompFilter {
                                name: Component::VEvent,
                                additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                                    time_range: Some(TimeRange::FullRange(
                                       Utc.with_ymd_and_hms(2006,1,4,0,0,0).unwrap(),
                                       Utc.with_ymd_and_hms(2006,1,5,0,0,0).unwrap(),
                                    )),
                                    prop_filter: vec![],
                                    comp_filter: vec![],
                                })),
                            },
                        ],
                    })),
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

    #[tokio::test]
    async fn rfc_calendar_query1_res() {
        let got = serialize(
            &dav::Multistatus::<Calendar> {
                responses: vec![
                    dav::Response {
                        status_or_propstat: dav::StatusOrPropstat::PropStat(
                            dav::Href("http://cal.example.com/bernard/work/abcd2.ics".into()),
                            vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag("\"fffff-abcd2\"".into())),
                                dav::AnyProperty::Value(dav::Property::Extension(Property::CalendarData(CalendarDataPayload {
                                    mime: None,
                                    payload: "PLACEHOLDER".into()
                                }))),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                            }]
                        ),
                        location: None,
                        error: None,
                        responsedescription: None,
                    },
                    dav::Response {
                        status_or_propstat: dav::StatusOrPropstat::PropStat(
                            dav::Href("http://cal.example.com/bernard/work/abcd3.ics".into()),
                            vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag("\"fffff-abcd3\"".into())),
                                dav::AnyProperty::Value(dav::Property::Extension(Property::CalendarData(CalendarDataPayload{
                                    mime: None,
                                    payload: "PLACEHOLDER".into(),
                                }))),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                            }]
                        ),
                        location: None,
                        error: None,
                        responsedescription: None,
                    },
                ],
                responsedescription: None,
            }, 
        ).await;

        let expected = r#"<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <D:response>
        <D:href>http://cal.example.com/bernard/work/abcd2.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;fffff-abcd2&quot;</D:getetag>
                <C:calendar-data>PLACEHOLDER</C:calendar-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
    <D:response>
        <D:href>http://cal.example.com/bernard/work/abcd3.ics</D:href>
        <D:propstat>
            <D:prop>
                <D:getetag>&quot;fffff-abcd3&quot;</D:getetag>
                <C:calendar-data>PLACEHOLDER</C:calendar-data>
            </D:prop>
            <D:status>HTTP/1.1 200 OK</D:status>
        </D:propstat>
    </D:response>
</D:multistatus>"#;


        assert_eq!(&got, expected, "\n---GOT---\n{got}\n---EXP---\n{expected}\n");
    }
}
