use quick_xml::events::Event;
use chrono::NaiveDateTime;

use super::types as dav;
use super::caltypes::*;
use super::xml::{QRead, IRead, Reader, Node, CAL_URN};
use super::error::ParsingError;

// ---- ROOT ELEMENTS ---
impl<E: dav::Extension> QRead<MkCalendar<E>> for MkCalendar<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "mkcalendar").await?;
        let set = xml.find().await?;
        xml.close().await?;
        Ok(MkCalendar(set))
    }
}

impl<E: dav::Extension, N: Node<N>> QRead<MkCalendarResponse<E,N>> for MkCalendarResponse<E,N> {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension> QRead<CalendarQuery<E>> for CalendarQuery<E> {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension> QRead<CalendarMultiget<E>> for CalendarMultiget<E> {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<FreeBusyQuery> for FreeBusyQuery {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}


// ---- EXTENSIONS ---
impl QRead<Violation> for Violation {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<Property> for Property {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "calendar-description").await?.is_some() {
            let lang = xml.prev_attr("xml:lang");
            let text = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::CalendarDescription { lang, text })
        }

        if xml.maybe_open(CAL_URN, "calendar-timezone").await?.is_some() {
            let tz = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::CalendarTimezone(tz))
        }

        if xml.maybe_open(CAL_URN, "supported-calendar-component-set").await?.is_some() {
            let comp = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCalendarComponentSet(comp))
        }

        if xml.maybe_open(CAL_URN, "supported-calendar-data").await?.is_some() {
            let mime = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCalendarData(mime))
        }

        if xml.maybe_open(CAL_URN, "max-resource-size").await?.is_some() {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxResourceSize(sz))
        }

        if xml.maybe_open(CAL_URN, "max-date-time").await?.is_some() {
            let dtstr = xml.tag_string().await?;
            let dt = NaiveDateTime::parse_from_str(dtstr.as_str(), ICAL_DATETIME_FMT)?.and_utc();
            xml.close().await?;
            return Ok(Property::MaxDateTime(dt))
        }

        if xml.maybe_open(CAL_URN, "max-instances").await?.is_some() {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxInstances(sz))
        }

        if xml.maybe_open(CAL_URN, "max-attendees-per-instance").await?.is_some() {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxAttendeesPerInstance(sz))
        }

        if xml.maybe_open(CAL_URN, "supported-collation-set").await?.is_some() {
            let cols = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCollationSet(cols))
        }

        let mut dirty = false;
        let mut caldata: Option<CalendarDataPayload> = None;
        xml.maybe_read(&mut caldata, &mut dirty).await?;
        if let Some(cal) = caldata {
            return Ok(Property::CalendarData(cal))
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<ResourceType> for ResourceType {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

// ---- INNER XML ----
impl QRead<SupportedCollation> for SupportedCollation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "supported-collation").await?;
        let col = Collation::new(xml.tag_string().await?);
        xml.close().await?;
        Ok(SupportedCollation(col))
    }
}

impl QRead<CalendarDataPayload> for CalendarDataPayload {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-data").await?;
        let mime = CalendarDataSupport::qread(xml).await.ok();
        let payload = xml.tag_string().await?;
        xml.close().await?;
        Ok(CalendarDataPayload { mime, payload })
    }
}

impl QRead<CalendarDataSupport> for CalendarDataSupport {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let ct = xml.prev_attr("content-type");
        let vs = xml.prev_attr("version");
        match (ct, vs) {
            (Some(content_type), Some(version)) => Ok(Self { content_type, version }),
            _ => Err(ParsingError::Recoverable),
        }
    }
}

impl QRead<CalendarDataRequest> for CalendarDataRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-data").await?;
        let mime = CalendarDataSupport::qread(xml).await.ok();

        let (mut comp, mut recurrence, mut limit_freebusy_set) = (None, None, None);

        loop {
            let mut dirty = false;
            xml.maybe_read(&mut comp, &mut dirty).await?;
            xml.maybe_read(&mut recurrence, &mut dirty).await?;
            xml.maybe_read(&mut limit_freebusy_set, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }

        }

        xml.close().await?;
        Ok(Self { mime, comp, recurrence, limit_freebusy_set })
    }
}

impl QRead<CalendarDataEmpty> for CalendarDataEmpty {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-data").await?;
        let mime = CalendarDataSupport::qread(xml).await.ok();
        xml.close().await?;
        Ok(Self(mime))
    }
}

impl QRead<Comp> for Comp {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "comp").await?;
        let name = Component::new(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        let additional_rules = Box::pin(xml.maybe_find()).await?;
        xml.close().await?;
        Ok(Self { name, additional_rules })
    }
}

impl QRead<CompInner> for CompInner {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let (mut prop_kind, mut comp_kind) = (None, None);

        loop {
            let mut dirty = false; 
          
            xml.maybe_read(&mut prop_kind, &mut dirty).await?;
            xml.maybe_read(&mut comp_kind, &mut dirty).await?;

           if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        };

        match (prop_kind, comp_kind) {
            (Some(prop_kind), Some(comp_kind)) => Ok(Self { prop_kind, comp_kind }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl QRead<CompSupport> for CompSupport {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "comp").await?;
        let inner = Component::new(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        xml.close().await?;
        Ok(Self(inner))
    }
}

impl QRead<CompKind> for CompKind {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut comp = Vec::new();
        loop {
            let mut dirty = false;

            if xml.maybe_open(CAL_URN, "allcomp").await?.is_some() {
                xml.close().await?;
                return Ok(CompKind::AllComp)
            }

            xml.maybe_push(&mut comp, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        Ok(CompKind::Comp(comp))
    }
}

impl QRead<PropKind> for PropKind {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut prop = Vec::new();
        loop {
            let mut dirty = false;

            if xml.maybe_open(CAL_URN, "allprop").await?.is_some() {
                xml.close().await?;
                return Ok(PropKind::AllProp)
            }

            xml.maybe_push(&mut prop, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        Ok(PropKind::Prop(prop))
    }
}

impl QRead<RecurrenceModifier> for RecurrenceModifier {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<Expand> for Expand {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<LimitRecurrenceSet> for LimitRecurrenceSet {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<LimitFreebusySet> for LimitFreebusySet {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl<E: dav::Extension> QRead<CalendarSelector<E>> for CalendarSelector<E> {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<CompFilter> for CompFilter {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<CompFilterRules> for CompFilterRules {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<CompFilterMatch> for CompFilterMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<PropFilter> for PropFilter {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<PropFilterRules> for PropFilterRules {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<PropFilterMatch> for PropFilterMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<TimeOrText> for TimeOrText {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<TextMatch> for TextMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<ParamFilterMatch> for ParamFilterMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<TimeZone> for TimeZone {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<Filter> for Filter {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<TimeRange> for TimeRange {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<CalProp> for CalProp {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    //use chrono::{FixedOffset, TimeZone};
    use crate::realization::Calendar;
    //use quick_reader::NsReader;

    async fn deserialize<T: Node<T>>(src: &str) -> T {
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(src.as_bytes())).await.unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn basic_mkcalendar() {
        let expected = MkCalendar(dav::Set(dav::PropValue(vec![
            dav::Property::DisplayName("Lisa's Events".into()),
        ])));

        let src = r#"
<?xml version="1.0" encoding="utf-8" ?>
<C:mkcalendar xmlns:D="DAV:"
             xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:set>
    <D:prop>
      <D:displayname>Lisa's Events</D:displayname>
    </D:prop>
   </D:set>
 </C:mkcalendar>
"#;
        let got = deserialize::<MkCalendar<Calendar>>(src).await;
        assert_eq!(got, expected)
    }
}
