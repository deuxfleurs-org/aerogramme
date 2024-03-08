use quick_xml::events::Event;
use chrono::NaiveDateTime;

use super::types as dav;
use super::caltypes::*;
use super::xml::{QRead, IRead, Reader, Node, DAV_URN, CAL_URN};
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
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "mkcalendar-response").await?;
        let propstats = xml.collect().await?;
        xml.close().await?;
        Ok(MkCalendarResponse(propstats))
    }
}

impl<E: dav::Extension> QRead<CalendarQuery<E>> for CalendarQuery<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-query").await?;
        let (mut selector, mut filter, mut timezone) = (None, None, None);
        loop {
            let mut dirty = false;
            xml.maybe_read(&mut selector, &mut dirty).await?;
            xml.maybe_read(&mut filter, &mut dirty).await?;
            xml.maybe_read(&mut timezone, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }
        xml.close().await?;

        match filter {
            Some(filter) => Ok(CalendarQuery { selector, filter, timezone }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl<E: dav::Extension> QRead<CalendarMultiget<E>> for CalendarMultiget<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "free-busy-query").await?;
        let mut selector = None;
        let mut href = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read(&mut selector, &mut dirty).await?;
            xml.maybe_push(&mut href, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        Ok(CalendarMultiget { selector, href })
    }
}

impl QRead<FreeBusyQuery> for FreeBusyQuery {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-multiple-get").await?;
        let range = xml.find().await?;
        xml.close().await?;
        Ok(FreeBusyQuery(range))
    }
}


// ---- EXTENSIONS ---
impl QRead<Violation> for Violation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(DAV_URN, "resource-must-be-null").await?.is_some() {
            xml.close().await?;
            Ok(Self::ResourceMustBeNull)
        } else if xml.maybe_open(DAV_URN, "need-privileges").await?.is_some() {
            xml.close().await?;
            Ok(Self::NeedPrivileges)
        } else if xml.maybe_open(CAL_URN, "calendar-collection-location-ok").await?.is_some() {
            xml.close().await?;
            Ok(Self::CalendarCollectionLocationOk)
        } else if xml.maybe_open(CAL_URN, "valid-calendar-data").await?.is_some() {
            xml.close().await?;
            Ok(Self::ValidCalendarData)
        } else if xml.maybe_open(CAL_URN, "initialize-calendar-collection").await?.is_some() {
            xml.close().await?;
            Ok(Self::InitializeCalendarCollection)
        } else if xml.maybe_open(CAL_URN, "supported-calendar-data").await?.is_some() {
            xml.close().await?;
            Ok(Self::SupportedCalendarData)
        } else if xml.maybe_open(CAL_URN, "valid-calendar-object-resource").await?.is_some() {
            xml.close().await?;
            Ok(Self::ValidCalendarObjectResource)
        } else if xml.maybe_open(CAL_URN, "supported-calendar-component").await?.is_some() {
            xml.close().await?;
            Ok(Self::SupportedCalendarComponent)
        } else if xml.maybe_open(CAL_URN, "no-uid-conflict").await?.is_some() {
            let href = xml.find().await?;
            xml.close().await?;
            Ok(Self::NoUidConflict(href))
        } else if xml.maybe_open(CAL_URN, "max-resource-size").await?.is_some() {
            xml.close().await?;
            Ok(Self::MaxResourceSize)
        } else if xml.maybe_open(CAL_URN, "min-date-time").await?.is_some() {
            xml.close().await?;
            Ok(Self::MinDateTime)
        } else if xml.maybe_open(CAL_URN, "max-date-time").await?.is_some() {
            xml.close().await?;
            Ok(Self::MaxDateTime)
        } else if xml.maybe_open(CAL_URN, "max-instances").await?.is_some() {
            xml.close().await?;
            Ok(Self::MaxInstances)
        } else if xml.maybe_open(CAL_URN, "max-attendees-per-instance").await?.is_some() {
            xml.close().await?;
            Ok(Self::MaxAttendeesPerInstance)
        } else if xml.maybe_open(CAL_URN, "valid-filter").await?.is_some() {
            xml.close().await?;
            Ok(Self::ValidFilter)
        } else if xml.maybe_open(CAL_URN, "supported-filter").await?.is_some() {
            let (mut comp, mut prop, mut param) = (Vec::new(), Vec::new(), Vec::new());
            loop {
                let mut dirty = false;
                xml.maybe_push(&mut comp, &mut dirty).await?;
                xml.maybe_push(&mut prop, &mut dirty).await?;
                xml.maybe_push(&mut param, &mut dirty).await?;

                if !dirty {
                    match xml.peek() {
                        Event::End(_) => break,
                        _ => xml.skip().await?,
                    };
                }
            }
            xml.close().await?;
            Ok(Self::SupportedFilter { comp, prop, param })
        } else if xml.maybe_open(CAL_URN, "number-of-matches-within-limits").await?.is_some() {
            xml.close().await?;
            Ok(Self::NumberOfMatchesWithinLimits)
        } else {
            Err(ParsingError::Recoverable)
        }
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
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "calendar-description").await?.is_some() {
            xml.close().await?;
            return Ok(Self::CalendarDescription)
        } 
        if xml.maybe_open(CAL_URN, "calendar-timezone").await?.is_some() {
            xml.close().await?;
            return Ok(Self::CalendarTimezone)
        }
        if xml.maybe_open(CAL_URN, "supported-calendar-component-set").await?.is_some() {
            xml.close().await?;
            return Ok(Self::SupportedCalendarComponentSet)
        }
        if xml.maybe_open(CAL_URN, "supported-calendar-data").await?.is_some() {
            xml.close().await?;
            return Ok(Self::SupportedCalendarData)
        }
        if xml.maybe_open(CAL_URN, "max-resource-size").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxResourceSize)
        }
        if xml.maybe_open(CAL_URN, "min-date-time").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MinDateTime)
        }
        if xml.maybe_open(CAL_URN, "max-date-time").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxDateTime)
        }
        if xml.maybe_open(CAL_URN, "max-instances").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxInstances)
        }
        if xml.maybe_open(CAL_URN, "max-attendees-per-instance").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxAttendeesPerInstance)
        }
        if xml.maybe_open(CAL_URN, "supported-collation-set").await?.is_some() {
            xml.close().await?;
            return Ok(Self::SupportedCollationSet)
        }
        let mut dirty = false;
        let mut m_cdr = None;
        xml.maybe_read(&mut m_cdr, &mut dirty).await?;
        m_cdr.ok_or(ParsingError::Recoverable).map(Self::CalendarData)
    }
}

impl QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "calendar").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Calendar)
        }
        Err(ParsingError::Recoverable)
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
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match Expand::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(RecurrenceModifier::Expand),
        }
        LimitRecurrenceSet::qread(xml).await.map(RecurrenceModifier::LimitRecurrenceSet)
    }
}

impl QRead<Expand> for Expand {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "expand").await?;
        let (rstart, rend) = match (xml.prev_attr("start"), xml.prev_attr("end")) {
            (Some(start), Some(end)) => (start, end),
            _ => return Err(ParsingError::MissingAttribute),
        };
        
        let start = NaiveDateTime::parse_from_str(rstart.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue)
        }

        xml.close().await?;
        Ok(Expand(start, end))
    }
}

impl QRead<LimitRecurrenceSet> for LimitRecurrenceSet {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "limit-recurrence-set").await?;
        let (rstart, rend) = match (xml.prev_attr("start"), xml.prev_attr("end")) {
            (Some(start), Some(end)) => (start, end),
            _ => return Err(ParsingError::MissingAttribute),
        };
        
        let start = NaiveDateTime::parse_from_str(rstart.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue)
        }

        xml.close().await?;
        Ok(LimitRecurrenceSet(start, end))
    }
}

impl QRead<LimitFreebusySet> for LimitFreebusySet {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "limit-freebusy-set").await?;
        let (rstart, rend) = match (xml.prev_attr("start"), xml.prev_attr("end")) {
            (Some(start), Some(end)) => (start, end),
            _ => return Err(ParsingError::MissingAttribute),
        };
        
        let start = NaiveDateTime::parse_from_str(rstart.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), ICAL_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue)
        }

        xml.close().await?;
        Ok(LimitFreebusySet(start, end))
    }
}

impl<E: dav::Extension> QRead<CalendarSelector<E>> for CalendarSelector<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        // allprop
        if let Some(_) = xml.maybe_open(DAV_URN, "allprop").await? {
            xml.close().await?;
            return Ok(Self::AllProp)
        }

        // propname
        if let Some(_) = xml.maybe_open(DAV_URN, "propname").await? {
            xml.close().await?;
            return Ok(Self::PropName)
        }

        // prop
        let (mut maybe_prop, mut dirty) = (None, false);
        xml.maybe_read::<dav::PropName<E>>(&mut maybe_prop, &mut dirty).await?;
        if let Some(prop) = maybe_prop {
            return Ok(Self::Prop(prop))
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<CompFilter> for CompFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "comp-filter").await?;
        let name = Component::new(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        let additional_rules = Box::pin(xml.maybe_find()).await?;
        xml.close().await?;
        Ok(Self { name, additional_rules })
    }
}

impl QRead<CompFilterRules> for CompFilterRules {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
            xml.close().await?;
            return Ok(Self::IsNotDefined)
        }
        CompFilterMatch::qread(xml).await.map(CompFilterRules::Matches)
    }
}

impl QRead<CompFilterMatch> for CompFilterMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut time_range = None;
        let mut prop_filter = Vec::new();
        let mut comp_filter = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read(&mut time_range, &mut dirty).await?;
            xml.maybe_push(&mut prop_filter, &mut dirty).await?;
            xml.maybe_push(&mut comp_filter, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        match (&time_range, &prop_filter[..], &comp_filter[..]) {
            (None, [], []) => Err(ParsingError::Recoverable),
            _ => Ok(CompFilterMatch { time_range, prop_filter, comp_filter }),
        }
    }
}

impl QRead<PropFilter> for PropFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "prop-filter").await?;
        let name = ComponentProperty(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        let additional_rules = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self { name, additional_rules })
    }
}

impl QRead<PropFilterRules> for PropFilterRules {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
            xml.close().await?;
            return Ok(Self::IsNotDefined)
        }
        PropFilterMatch::qread(xml).await.map(PropFilterRules::Match)
    }
}

impl QRead<PropFilterMatch> for PropFilterMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut time_range = None;
        let mut time_or_text = None;
        let mut param_filter = Vec::new();

        loop {
            let mut dirty = false;
            xml.maybe_read(&mut time_range, &mut dirty).await?;
            xml.maybe_read(&mut time_or_text, &mut dirty).await?;
            xml.maybe_push(&mut param_filter, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        match (&time_range, &time_or_text, &param_filter[..]) {
            (None, None, []) => Err(ParsingError::Recoverable),
            _ => Ok(PropFilterMatch { time_range, time_or_text, param_filter }),
        }
    }
}

impl QRead<ParamFilter> for ParamFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "param-filter").await?;
        let name = PropertyParameter(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        let additional_rules = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self { name, additional_rules })
    }
}

impl QRead<TimeOrText> for TimeOrText {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match TimeRange::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Self::Time),
        }
        TextMatch::qread(xml).await.map(Self::Text)
    }
}

impl QRead<TextMatch> for TextMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "text-match").await?;
        let collation = xml.prev_attr("collation").map(Collation::new);
        let negate_condition = xml.prev_attr("negate-condition").map(|v| v == "yes");
        let text = xml.tag_string().await?;
        xml.close().await?;
        Ok(Self { collation, negate_condition, text })
    }
}

impl QRead<ParamFilterMatch> for ParamFilterMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
            xml.close().await?;
            return Ok(Self::IsNotDefined)
        }
        TextMatch::qread(xml).await.map(Self::Match)
    }
}

impl QRead<TimeZone> for TimeZone {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "timezone").await?;
        let inner = xml.tag_string().await?;
        xml.close().await?;
        Ok(Self(inner))
    }
}

impl QRead<Filter> for Filter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "timezone").await?;
        let comp_filter = xml.find().await?;
        xml.close().await?;
        Ok(Self(comp_filter))
    }
}

impl QRead<TimeRange> for TimeRange {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "time-range").await?;

        let start = match xml.prev_attr("start") {
            Some(r) => Some(NaiveDateTime::parse_from_str(r.as_str(), ICAL_DATETIME_FMT)?.and_utc()),
            _ => None,
        };
        let end = match xml.prev_attr("end") {
            Some(r) => Some(NaiveDateTime::parse_from_str(r.as_str(), ICAL_DATETIME_FMT)?.and_utc()),
            _ => None,
        };

        xml.close().await?;

        match (start, end) {
            (Some(start), Some(end)) => {
                if start > end {
                    return Err(ParsingError::InvalidValue)
                }
                Ok(TimeRange::FullRange(start, end))
            },
            (Some(start), None) => Ok(TimeRange::OnlyStart(start)),
            (None, Some(end)) => Ok(TimeRange::OnlyEnd(end)),
            (None, None) => Err(ParsingError::MissingAttribute),
        }
    }
}

impl QRead<CalProp> for CalProp {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "prop").await?;        
        let name = ComponentProperty(xml.prev_attr("name").ok_or(ParsingError::MissingAttribute)?);
        let novalue = xml.prev_attr("novalue").map(|v| v == "yes");
        xml.close().await?;
        Ok(Self { name, novalue })
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
