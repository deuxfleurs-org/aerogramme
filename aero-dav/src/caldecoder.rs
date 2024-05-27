use chrono::NaiveDateTime;
use quick_xml::events::Event;

use super::caltypes::*;
use super::error::ParsingError;
use super::types as dav;
use super::xml::{IRead, QRead, Reader, CAL_URN, DAV_URN};

// ---- ROOT ELEMENTS ---
impl<E: dav::Extension> QRead<MkCalendar<E>> for MkCalendar<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "mkcalendar").await?;
        let set = xml.find().await?;
        xml.close().await?;
        Ok(MkCalendar(set))
    }
}

impl<E: dav::Extension> QRead<MkCalendarResponse<E>> for MkCalendarResponse<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "mkcalendar-response").await?;
        let propstats = xml.collect().await?;
        xml.close().await?;
        Ok(MkCalendarResponse(propstats))
    }
}

impl<E: dav::Extension> QRead<ReportType<E>> for ReportType<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match CalendarQuery::<E>::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Self::Query),
        }

        match CalendarMultiget::<E>::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(Self::Multiget),
        }

        FreeBusyQuery::qread(xml).await.map(Self::FreeBusy)
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
            Some(filter) => Ok(CalendarQuery {
                selector,
                filter,
                timezone,
            }),
            _ => Err(ParsingError::MissingChild),
        }
    }
}

impl<E: dav::Extension> QRead<CalendarMultiget<E>> for CalendarMultiget<E> {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-multiget").await?;
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
        xml.open(CAL_URN, "free-busy-query").await?;
        let range = xml.find().await?;
        xml.close().await?;
        Ok(FreeBusyQuery(range))
    }
}

// ---- EXTENSIONS ---
impl QRead<Violation> for Violation {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(DAV_URN, "resource-must-be-null")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::ResourceMustBeNull)
        } else if xml.maybe_open(DAV_URN, "need-privileges").await?.is_some() {
            xml.close().await?;
            Ok(Self::NeedPrivileges)
        } else if xml
            .maybe_open(CAL_URN, "calendar-collection-location-ok")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::CalendarCollectionLocationOk)
        } else if xml
            .maybe_open(CAL_URN, "valid-calendar-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::ValidCalendarData)
        } else if xml
            .maybe_open(CAL_URN, "initialize-calendar-collection")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::InitializeCalendarCollection)
        } else if xml
            .maybe_open(CAL_URN, "supported-calendar-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::SupportedCalendarData)
        } else if xml
            .maybe_open(CAL_URN, "valid-calendar-object-resource")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::ValidCalendarObjectResource)
        } else if xml
            .maybe_open(CAL_URN, "supported-calendar-component")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::SupportedCalendarComponent)
        } else if xml.maybe_open(CAL_URN, "no-uid-conflict").await?.is_some() {
            let href = xml.find().await?;
            xml.close().await?;
            Ok(Self::NoUidConflict(href))
        } else if xml
            .maybe_open(CAL_URN, "max-resource-size")
            .await?
            .is_some()
        {
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
        } else if xml
            .maybe_open(CAL_URN, "max-attendees-per-instance")
            .await?
            .is_some()
        {
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
        } else if xml
            .maybe_open(CAL_URN, "number-of-matches-within-limits")
            .await?
            .is_some()
        {
            xml.close().await?;
            Ok(Self::NumberOfMatchesWithinLimits)
        } else {
            Err(ParsingError::Recoverable)
        }
    }
}

impl QRead<Property> for Property {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open_start(CAL_URN, "calendar-home-set")
            .await?
            .is_some()
        {
            let href = xml.find().await?;
            xml.close().await?;
            return Ok(Property::CalendarHomeSet(href));
        }
        if xml
            .maybe_open_start(CAL_URN, "calendar-description")
            .await?
            .is_some()
        {
            let lang = xml.prev_attr("xml:lang");
            let text = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::CalendarDescription { lang, text });
        }

        if xml
            .maybe_open_start(CAL_URN, "calendar-timezone")
            .await?
            .is_some()
        {
            let tz = xml.tag_string().await?;
            xml.close().await?;
            return Ok(Property::CalendarTimezone(tz));
        }

        if xml
            .maybe_open_start(CAL_URN, "supported-calendar-component-set")
            .await?
            .is_some()
        {
            let comp = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCalendarComponentSet(comp));
        }

        if xml
            .maybe_open_start(CAL_URN, "supported-calendar-data")
            .await?
            .is_some()
        {
            let mime = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCalendarData(mime));
        }

        if xml
            .maybe_open_start(CAL_URN, "max-resource-size")
            .await?
            .is_some()
        {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxResourceSize(sz));
        }

        if xml
            .maybe_open_start(CAL_URN, "max-date-time")
            .await?
            .is_some()
        {
            let dtstr = xml.tag_string().await?;
            let dt = NaiveDateTime::parse_from_str(dtstr.as_str(), UTC_DATETIME_FMT)?.and_utc();
            xml.close().await?;
            return Ok(Property::MaxDateTime(dt));
        }

        if xml
            .maybe_open_start(CAL_URN, "max-instances")
            .await?
            .is_some()
        {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxInstances(sz));
        }

        if xml
            .maybe_open_start(CAL_URN, "max-attendees-per-instance")
            .await?
            .is_some()
        {
            let sz = xml.tag_string().await?.parse::<u64>()?;
            xml.close().await?;
            return Ok(Property::MaxAttendeesPerInstance(sz));
        }

        if xml
            .maybe_open_start(CAL_URN, "supported-collation-set")
            .await?
            .is_some()
        {
            let cols = xml.collect().await?;
            xml.close().await?;
            return Ok(Property::SupportedCollationSet(cols));
        }

        let mut dirty = false;
        let mut caldata: Option<CalendarDataPayload> = None;
        xml.maybe_read(&mut caldata, &mut dirty).await?;
        if let Some(cal) = caldata {
            return Ok(Property::CalendarData(cal));
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<PropertyRequest> for PropertyRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml
            .maybe_open(CAL_URN, "calendar-home-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::CalendarHomeSet);
        }
        if xml
            .maybe_open(CAL_URN, "calendar-description")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::CalendarDescription);
        }
        if xml
            .maybe_open(CAL_URN, "calendar-timezone")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::CalendarTimezone);
        }
        if xml
            .maybe_open(CAL_URN, "supported-calendar-component-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedCalendarComponentSet);
        }
        if xml
            .maybe_open(CAL_URN, "supported-calendar-data")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedCalendarData);
        }
        if xml
            .maybe_open(CAL_URN, "max-resource-size")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::MaxResourceSize);
        }
        if xml.maybe_open(CAL_URN, "min-date-time").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MinDateTime);
        }
        if xml.maybe_open(CAL_URN, "max-date-time").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxDateTime);
        }
        if xml.maybe_open(CAL_URN, "max-instances").await?.is_some() {
            xml.close().await?;
            return Ok(Self::MaxInstances);
        }
        if xml
            .maybe_open(CAL_URN, "max-attendees-per-instance")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::MaxAttendeesPerInstance);
        }
        if xml
            .maybe_open(CAL_URN, "supported-collation-set")
            .await?
            .is_some()
        {
            xml.close().await?;
            return Ok(Self::SupportedCollationSet);
        }
        let mut dirty = false;
        let mut m_cdr = None;
        xml.maybe_read(&mut m_cdr, &mut dirty).await?;
        m_cdr
            .ok_or(ParsingError::Recoverable)
            .map(Self::CalendarData)
    }
}

impl QRead<ResourceType> for ResourceType {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "calendar").await?.is_some() {
            xml.close().await?;
            return Ok(Self::Calendar);
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
            (Some(content_type), Some(version)) => Ok(Self {
                content_type,
                version,
            }),
            _ => Err(ParsingError::Recoverable),
        }
    }
}

impl QRead<CalendarDataRequest> for CalendarDataRequest {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "calendar-data").await?;
        let mime = CalendarDataSupport::qread(xml).await.ok();
        let (mut comp, mut recurrence, mut limit_freebusy_set) = (None, None, None);

        if !xml.parent_has_child() {
            return Ok(Self {
                mime,
                comp,
                recurrence,
                limit_freebusy_set,
            });
        }

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
        Ok(Self {
            mime,
            comp,
            recurrence,
            limit_freebusy_set,
        })
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
        let (mut prop_kind, mut comp_kind) = (None, None);

        let bs = xml.open(CAL_URN, "comp").await?;
        let name = Component::new(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );

        // Return early if it's an empty tag
        if matches!(bs, Event::Empty(_)) {
            xml.close().await?;
            return Ok(Self {
                name,
                prop_kind,
                comp_kind,
            });
        }

        loop {
            let mut dirty = false;
            let (mut tmp_prop_kind, mut tmp_comp_kind): (Option<PropKind>, Option<CompKind>) =
                (None, None);

            xml.maybe_read(&mut tmp_prop_kind, &mut dirty).await?;
            Box::pin(xml.maybe_read(&mut tmp_comp_kind, &mut dirty)).await?;

            //@FIXME hack
            // Merge
            match (tmp_prop_kind, &mut prop_kind) {
                (Some(PropKind::Prop(mut a)), Some(PropKind::Prop(ref mut b))) => b.append(&mut a),
                (Some(PropKind::AllProp), v) => *v = Some(PropKind::AllProp),
                (Some(x), b) => *b = Some(x),
                (None, _) => (),
            };
            match (tmp_comp_kind, &mut comp_kind) {
                (Some(CompKind::Comp(mut a)), Some(CompKind::Comp(ref mut b))) => b.append(&mut a),
                (Some(CompKind::AllComp), v) => *v = Some(CompKind::AllComp),
                (Some(a), b) => *b = Some(a),
                (None, _) => (),
            };

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        xml.close().await?;
        Ok(Self {
            name,
            prop_kind,
            comp_kind,
        })
    }
}

impl QRead<CompSupport> for CompSupport {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "comp").await?;
        let inner = Component::new(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
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
                return Ok(CompKind::AllComp);
            }

            xml.maybe_push(&mut comp, &mut dirty).await?;

            if !dirty {
                break;
            }
        }
        match &comp[..] {
            [] => Err(ParsingError::Recoverable),
            _ => Ok(CompKind::Comp(comp)),
        }
    }
}

impl QRead<PropKind> for PropKind {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut prop = Vec::new();
        loop {
            let mut dirty = false;

            if xml.maybe_open(CAL_URN, "allprop").await?.is_some() {
                xml.close().await?;
                return Ok(PropKind::AllProp);
            }

            xml.maybe_push(&mut prop, &mut dirty).await?;

            if !dirty {
                break;
            }
        }

        match &prop[..] {
            [] => Err(ParsingError::Recoverable),
            _ => Ok(PropKind::Prop(prop)),
        }
    }
}

impl QRead<RecurrenceModifier> for RecurrenceModifier {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        match Expand::qread(xml).await {
            Err(ParsingError::Recoverable) => (),
            otherwise => return otherwise.map(RecurrenceModifier::Expand),
        }
        LimitRecurrenceSet::qread(xml)
            .await
            .map(RecurrenceModifier::LimitRecurrenceSet)
    }
}

impl QRead<Expand> for Expand {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "expand").await?;
        let (rstart, rend) = match (xml.prev_attr("start"), xml.prev_attr("end")) {
            (Some(start), Some(end)) => (start, end),
            _ => return Err(ParsingError::MissingAttribute),
        };

        let start = NaiveDateTime::parse_from_str(rstart.as_str(), UTC_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), UTC_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue);
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

        let start = NaiveDateTime::parse_from_str(rstart.as_str(), UTC_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), UTC_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue);
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

        let start = NaiveDateTime::parse_from_str(rstart.as_str(), UTC_DATETIME_FMT)?.and_utc();
        let end = NaiveDateTime::parse_from_str(rend.as_str(), UTC_DATETIME_FMT)?.and_utc();
        if start > end {
            return Err(ParsingError::InvalidValue);
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
            return Ok(Self::AllProp);
        }

        // propname
        if let Some(_) = xml.maybe_open(DAV_URN, "propname").await? {
            xml.close().await?;
            return Ok(Self::PropName);
        }

        // prop
        let (mut maybe_prop, mut dirty) = (None, false);
        xml.maybe_read::<dav::PropName<E>>(&mut maybe_prop, &mut dirty)
            .await?;
        if let Some(prop) = maybe_prop {
            return Ok(Self::Prop(prop));
        }

        Err(ParsingError::Recoverable)
    }
}

impl QRead<CompFilter> for CompFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "comp-filter").await?;
        let name = Component::new(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let additional_rules = Box::pin(xml.maybe_find()).await?;
        xml.close().await?;
        Ok(Self {
            name,
            additional_rules,
        })
    }
}

impl QRead<CompFilterRules> for CompFilterRules {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut time_range = None;
        let mut prop_filter = Vec::new();
        let mut comp_filter = Vec::new();

        loop {
            let mut dirty = false;

            if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
                xml.close().await?;
                return Ok(Self::IsNotDefined);
            }

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
            _ => Ok(Self::Matches(CompFilterMatch {
                time_range,
                prop_filter,
                comp_filter,
            })),
        }
    }
}

impl QRead<CompFilterMatch> for CompFilterMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<PropFilter> for PropFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "prop-filter").await?;
        let name = ComponentProperty(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let additional_rules = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self {
            name,
            additional_rules,
        })
    }
}

impl QRead<PropFilterRules> for PropFilterRules {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        let mut time_or_text = None;
        let mut param_filter = Vec::new();

        loop {
            let mut dirty = false;

            if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
                xml.close().await?;
                return Ok(Self::IsNotDefined);
            }

            xml.maybe_read(&mut time_or_text, &mut dirty).await?;
            xml.maybe_push(&mut param_filter, &mut dirty).await?;

            if !dirty {
                match xml.peek() {
                    Event::End(_) => break,
                    _ => xml.skip().await?,
                };
            }
        }

        match (&time_or_text, &param_filter[..]) {
            (None, []) => Err(ParsingError::Recoverable),
            _ => Ok(PropFilterRules::Match(PropFilterMatch {
                time_or_text,
                param_filter,
            })),
        }
    }
}

impl QRead<PropFilterMatch> for PropFilterMatch {
    async fn qread(_xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        unreachable!();
    }
}

impl QRead<ParamFilter> for ParamFilter {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "param-filter").await?;
        let name = PropertyParameter(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let additional_rules = xml.maybe_find().await?;
        xml.close().await?;
        Ok(Self {
            name,
            additional_rules,
        })
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
        Ok(Self {
            collation,
            negate_condition,
            text,
        })
    }
}

impl QRead<ParamFilterMatch> for ParamFilterMatch {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        if xml.maybe_open(CAL_URN, "is-not-defined").await?.is_some() {
            xml.close().await?;
            return Ok(Self::IsNotDefined);
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
        xml.open(CAL_URN, "filter").await?;
        let comp_filter = xml.find().await?;
        xml.close().await?;
        Ok(Self(comp_filter))
    }
}

impl QRead<TimeRange> for TimeRange {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "time-range").await?;

        let start = match xml.prev_attr("start") {
            Some(r) => Some(NaiveDateTime::parse_from_str(r.as_str(), UTC_DATETIME_FMT)?.and_utc()),
            _ => None,
        };
        let end = match xml.prev_attr("end") {
            Some(r) => Some(NaiveDateTime::parse_from_str(r.as_str(), UTC_DATETIME_FMT)?.and_utc()),
            _ => None,
        };

        xml.close().await?;

        match (start, end) {
            (Some(start), Some(end)) => {
                if start > end {
                    return Err(ParsingError::InvalidValue);
                }
                Ok(TimeRange::FullRange(start, end))
            }
            (Some(start), None) => Ok(TimeRange::OnlyStart(start)),
            (None, Some(end)) => Ok(TimeRange::OnlyEnd(end)),
            (None, None) => Err(ParsingError::MissingAttribute),
        }
    }
}

impl QRead<CalProp> for CalProp {
    async fn qread(xml: &mut Reader<impl IRead>) -> Result<Self, ParsingError> {
        xml.open(CAL_URN, "prop").await?;
        let name = ComponentProperty(
            xml.prev_attr("name")
                .ok_or(ParsingError::MissingAttribute)?,
        );
        let novalue = xml.prev_attr("novalue").map(|v| v == "yes");
        xml.close().await?;
        Ok(Self { name, novalue })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realization::Calendar;
    use crate::xml::Node;
    use chrono::{TimeZone, Utc};
    //use quick_reader::NsReader;

    async fn deserialize<T: Node<T>>(src: &str) -> T {
        let mut rdr = Reader::new(quick_xml::NsReader::from_reader(src.as_bytes()))
            .await
            .unwrap();
        rdr.find().await.unwrap()
    }

    #[tokio::test]
    async fn simple_comp_filter() {
        let expected = CompFilter {
            name: Component::VEvent,
            additional_rules: None,
        };
        let src = r#"<C:comp-filter name="VEVENT" xmlns:C="urn:ietf:params:xml:ns:caldav" />"#;
        let got = deserialize::<CompFilter>(src).await;
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn basic_mkcalendar() {
        let expected = MkCalendar(dav::Set(dav::PropValue(vec![dav::Property::DisplayName(
            "Lisa's Events".into(),
        )])));

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

    #[tokio::test]
    async fn rfc_mkcalendar() {
        let expected = MkCalendar(dav::Set(dav::PropValue(vec![
            dav::Property::DisplayName("Lisa's Events".into()),
            dav::Property::Extension(Property::CalendarDescription {
                lang: Some("en".into()),
                text: "Calendar restricted to events.".into(),
            }),
            dav::Property::Extension(Property::SupportedCalendarComponentSet(vec![
                CompSupport(Component::VEvent)
            ])),
            dav::Property::Extension(Property::CalendarTimezone("BEGIN:VCALENDAR\nPRODID:-//Example Corp.//CalDAV Client//EN\nVERSION:2.0\nEND:VCALENDAR".into())),
        ])));

        let src = r#"
   <?xml version="1.0" encoding="utf-8" ?>
   <C:mkcalendar xmlns:D="DAV:"
                 xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:set>
       <D:prop>
         <D:displayname>Lisa's Events</D:displayname>
         <C:calendar-description xml:lang="en"
   >Calendar restricted to events.</C:calendar-description>
         <C:supported-calendar-component-set>
           <C:comp name="VEVENT"/>
         </C:supported-calendar-component-set>
         <C:calendar-timezone><![CDATA[BEGIN:VCALENDAR
PRODID:-//Example Corp.//CalDAV Client//EN
VERSION:2.0
END:VCALENDAR]]></C:calendar-timezone>
       </D:prop>
     </D:set>
   </C:mkcalendar>"#;

        let got = deserialize::<MkCalendar<Calendar>>(src).await;
        assert_eq!(got, expected)
    }

    #[tokio::test]
    async fn rfc_calendar_query() {
        let expected = CalendarQuery {
            selector: Some(CalendarSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::CalendarData(
                    CalendarDataRequest {
                        mime: None,
                        comp: Some(Comp {
                            name: Component::VCalendar,
                            prop_kind: Some(PropKind::Prop(vec![CalProp {
                                name: ComponentProperty("VERSION".into()),
                                novalue: None,
                            }])),
                            comp_kind: Some(CompKind::Comp(vec![
                                Comp {
                                    name: Component::VEvent,
                                    prop_kind: Some(PropKind::Prop(vec![
                                        CalProp {
                                            name: ComponentProperty("SUMMARY".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("UID".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("DTSTART".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("DTEND".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("DURATION".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("RRULE".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("RDATE".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("EXRULE".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("EXDATE".into()),
                                            novalue: None,
                                        },
                                        CalProp {
                                            name: ComponentProperty("RECURRENCE-ID".into()),
                                            novalue: None,
                                        },
                                    ])),
                                    comp_kind: None,
                                },
                                Comp {
                                    name: Component::VTimeZone,
                                    prop_kind: None,
                                    comp_kind: None,
                                },
                            ])),
                        }),
                        recurrence: None,
                        limit_freebusy_set: None,
                    },
                )),
            ]))),
            filter: Filter(CompFilter {
                name: Component::VCalendar,
                additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                    prop_filter: vec![],
                    comp_filter: vec![CompFilter {
                        name: Component::VEvent,
                        additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                            prop_filter: vec![],
                            comp_filter: vec![],
                            time_range: Some(TimeRange::FullRange(
                                Utc.with_ymd_and_hms(2006, 1, 4, 0, 0, 0).unwrap(),
                                Utc.with_ymd_and_hms(2006, 1, 5, 0, 0, 0).unwrap(),
                            )),
                        })),
                    }],
                    time_range: None,
                })),
            }),
            timezone: None,
        };

        let src = r#"
<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:"
             xmlns:C="urn:ietf:params:xml:ns:caldav">
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
       <C:time-range start="20060104T000000Z"
                     end="20060105T000000Z"/>
     </C:comp-filter>
   </C:comp-filter>
 </C:filter>
</C:calendar-query>
"#;

        let got = deserialize::<CalendarQuery<Calendar>>(src).await;
        assert_eq!(got, expected)
    }

    #[tokio::test]
    async fn rfc_calendar_query_res() {
        let expected = dav::Multistatus::<Calendar> {
            responses: vec![
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("http://cal.example.com/bernard/work/abcd2.ics".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"fffff-abcd2\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::CalendarData(CalendarDataPayload {
                                        mime: None,
                                        payload: "BEGIN:VCALENDAR".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
                dav::Response {
                    status_or_propstat: dav::StatusOrPropstat::PropStat(
                        dav::Href("http://cal.example.com/bernard/work/abcd3.ics".into()),
                        vec![dav::PropStat {
                            prop: dav::AnyProp(vec![
                                dav::AnyProperty::Value(dav::Property::GetEtag(
                                    "\"fffff-abcd3\"".into(),
                                )),
                                dav::AnyProperty::Value(dav::Property::Extension(
                                    Property::CalendarData(CalendarDataPayload {
                                        mime: None,
                                        payload: "BEGIN:VCALENDAR".into(),
                                    }),
                                )),
                            ]),
                            status: dav::Status(http::status::StatusCode::OK),
                            error: None,
                            responsedescription: None,
                        }],
                    ),
                    error: None,
                    location: None,
                    responsedescription: None,
                },
            ],
            responsedescription: None,
        };

        let src = r#"<D:multistatus xmlns:D="DAV:"
              xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <D:href>http://cal.example.com/bernard/work/abcd2.ics</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"fffff-abcd2"</D:getetag>
           <C:calendar-data>BEGIN:VCALENDAR</C:calendar-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
     <D:response>
       <D:href>http://cal.example.com/bernard/work/abcd3.ics</D:href>
       <D:propstat>
         <D:prop>
           <D:getetag>"fffff-abcd3"</D:getetag>
           <C:calendar-data>BEGIN:VCALENDAR</C:calendar-data>
         </D:prop>
         <D:status>HTTP/1.1 200 OK</D:status>
       </D:propstat>
     </D:response>
   </D:multistatus>
"#;

        let got = deserialize::<dav::Multistatus<Calendar>>(src).await;
        assert_eq!(got, expected)
    }

    #[tokio::test]
    async fn rfc_recurring_evt() {
        let expected = CalendarQuery::<Calendar> {
            selector: Some(CalendarSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::Extension(PropertyRequest::CalendarData(
                    CalendarDataRequest {
                        mime: None,
                        comp: None,
                        recurrence: Some(RecurrenceModifier::LimitRecurrenceSet(
                            LimitRecurrenceSet(
                                Utc.with_ymd_and_hms(2006, 1, 3, 0, 0, 0).unwrap(),
                                Utc.with_ymd_and_hms(2006, 1, 5, 0, 0, 0).unwrap(),
                            ),
                        )),
                        limit_freebusy_set: None,
                    },
                )),
            ]))),
            filter: Filter(CompFilter {
                name: Component::VCalendar,
                additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                    prop_filter: vec![],
                    comp_filter: vec![CompFilter {
                        name: Component::VEvent,
                        additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                            prop_filter: vec![],
                            comp_filter: vec![],
                            time_range: Some(TimeRange::FullRange(
                                Utc.with_ymd_and_hms(2006, 1, 3, 0, 0, 0).unwrap(),
                                Utc.with_ymd_and_hms(2006, 1, 5, 0, 0, 0).unwrap(),
                            )),
                        })),
                    }],
                    time_range: None,
                })),
            }),
            timezone: None,
        };

        let src = r#"
  <?xml version="1.0" encoding="utf-8" ?>
   <C:calendar-query xmlns:D="DAV:"
                     xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:prop>
       <C:calendar-data>
         <C:limit-recurrence-set start="20060103T000000Z"
                                 end="20060105T000000Z"/>
       </C:calendar-data>
     </D:prop>
     <C:filter>
       <C:comp-filter name="VCALENDAR">
         <C:comp-filter name="VEVENT">
           <C:time-range start="20060103T000000Z"
                         end="20060105T000000Z"/>
         </C:comp-filter>
       </C:comp-filter>
     </C:filter>
   </C:calendar-query>"#;

        let got = deserialize::<CalendarQuery<Calendar>>(src).await;
        assert_eq!(got, expected)
    }

    #[tokio::test]
    async fn rfc_pending_todos() {
        let expected = CalendarQuery::<Calendar> {
            selector: Some(CalendarSelector::Prop(dav::PropName(vec![
                dav::PropertyRequest::GetEtag,
                dav::PropertyRequest::Extension(PropertyRequest::CalendarData(
                    CalendarDataRequest {
                        mime: None,
                        comp: None,
                        recurrence: None,
                        limit_freebusy_set: None,
                    },
                )),
            ]))),
            filter: Filter(CompFilter {
                name: Component::VCalendar,
                additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                    time_range: None,
                    prop_filter: vec![],
                    comp_filter: vec![CompFilter {
                        name: Component::VTodo,
                        additional_rules: Some(CompFilterRules::Matches(CompFilterMatch {
                            time_range: None,
                            comp_filter: vec![],
                            prop_filter: vec![
                                PropFilter {
                                    name: ComponentProperty("COMPLETED".into()),
                                    additional_rules: Some(PropFilterRules::IsNotDefined),
                                },
                                PropFilter {
                                    name: ComponentProperty("STATUS".into()),
                                    additional_rules: Some(PropFilterRules::Match(
                                        PropFilterMatch {
                                            param_filter: vec![],
                                            time_or_text: Some(TimeOrText::Text(TextMatch {
                                                collation: None,
                                                negate_condition: Some(true),
                                                text: "CANCELLED".into(),
                                            })),
                                        },
                                    )),
                                },
                            ],
                        })),
                    }],
                })),
            }),
            timezone: None,
        };

        let src = r#"<?xml version="1.0" encoding="utf-8" ?>
   <C:calendar-query xmlns:C="urn:ietf:params:xml:ns:caldav">
     <D:prop xmlns:D="DAV:">
       <D:getetag/>
       <C:calendar-data/>
     </D:prop>
     <C:filter>
       <C:comp-filter name="VCALENDAR">
         <C:comp-filter name="VTODO">
           <C:prop-filter name="COMPLETED">
             <C:is-not-defined/>
           </C:prop-filter>
           <C:prop-filter name="STATUS">
             <C:text-match
                negate-condition="yes">CANCELLED</C:text-match>
           </C:prop-filter>
         </C:comp-filter>
       </C:comp-filter>
     </C:filter>
   </C:calendar-query>"#;

        let got = deserialize::<CalendarQuery<Calendar>>(src).await;
        assert_eq!(got, expected)
    }
}
