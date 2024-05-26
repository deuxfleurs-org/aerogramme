use crate::parser;
use aero_dav::caltypes as cal;

pub fn is_component_match(
    parent: &icalendar::parser::Component,
    components: &[icalendar::parser::Component],
    filter: &cal::CompFilter,
) -> bool {
    // Find the component among the list
    //@FIXME do not handle correctly multiple entities (eg. 3 VEVENT)
    let maybe_comp = components
        .iter()
        .find(|candidate| candidate.name.as_str() == filter.name.as_str());

    // Filter according to rules
    match (maybe_comp, &filter.additional_rules) {
        (Some(_), None) => true,
        (None, Some(cal::CompFilterRules::IsNotDefined)) => true,
        (None, None) => false,
        (Some(_), Some(cal::CompFilterRules::IsNotDefined)) => false,
        (None, Some(cal::CompFilterRules::Matches(_))) => false,
        (Some(component), Some(cal::CompFilterRules::Matches(matcher))) => {
            // check time range
            if let Some(time_range) = &matcher.time_range {
                if !is_in_time_range(
                    &filter.name,
                    parent,
                    component.properties.as_ref(),
                    time_range,
                ) {
                    return false;
                }
            }

            // check properties
            if !is_properties_match(component.properties.as_ref(), matcher.prop_filter.as_ref()) {
                return false;
            }

            // check inner components
            matcher.comp_filter.iter().all(|inner_filter| {
                is_component_match(component, component.components.as_ref(), &inner_filter)
            })
        }
    }
}

fn prop_date(
    properties: &[icalendar::parser::Property],
    name: &str,
) -> Option<chrono::DateTime<chrono::Utc>> {
    properties
        .iter()
        .find(|candidate| candidate.name.as_str() == name)
        .map(|p| p.val.as_str())
        .map(parser::date_time)
        .flatten()
}

fn prop_parse<T: std::str::FromStr>(
    properties: &[icalendar::parser::Property],
    name: &str,
) -> Option<T> {
    properties
        .iter()
        .find(|candidate| candidate.name.as_str() == name)
        .map(|p| p.val.as_str().parse::<T>().ok())
        .flatten()
}

fn is_properties_match(props: &[icalendar::parser::Property], filters: &[cal::PropFilter]) -> bool {
    filters.iter().all(|single_filter| {
        // Find the property
        let single_prop = props
            .iter()
            .find(|candidate| candidate.name.as_str() == single_filter.name.0.as_str());
        match (&single_filter.additional_rules, single_prop) {
            (None, Some(_)) | (Some(cal::PropFilterRules::IsNotDefined), None) => true,
            (None, None)
            | (Some(cal::PropFilterRules::IsNotDefined), Some(_))
            | (Some(cal::PropFilterRules::Match(_)), None) => false,
            (Some(cal::PropFilterRules::Match(pattern)), Some(prop)) => {
                // check value
                match &pattern.time_or_text {
                    Some(cal::TimeOrText::Time(time_range)) => {
                        let maybe_parsed_date = parser::date_time(prop.val.as_str());

                        let parsed_date = match maybe_parsed_date {
                            None => return false,
                            Some(v) => v,
                        };

                        // see if entry is in range
                        let is_in_range = match time_range {
                            cal::TimeRange::OnlyStart(after) => &parsed_date >= after,
                            cal::TimeRange::OnlyEnd(before) => &parsed_date <= before,
                            cal::TimeRange::FullRange(after, before) => {
                                &parsed_date >= after && &parsed_date <= before
                            }
                        };
                        if !is_in_range {
                            return false;
                        }

                        // if you are here, this subcondition is valid
                    }
                    Some(cal::TimeOrText::Text(txt_match)) => {
                        //@FIXME ignoring collation
                        let is_match = match txt_match.negate_condition {
                            None | Some(false) => {
                                prop.val.as_str().contains(txt_match.text.as_str())
                            }
                            Some(true) => !prop.val.as_str().contains(txt_match.text.as_str()),
                        };
                        if !is_match {
                            return false;
                        }
                    }
                    None => (), // if not filter on value is set, continue
                };

                // check parameters
                pattern.param_filter.iter().all(|single_param_filter| {
                    let maybe_param = prop.params.iter().find(|candidate| {
                        candidate.key.as_str() == single_param_filter.name.as_str()
                    });

                    match (maybe_param, &single_param_filter.additional_rules) {
                        (Some(_), None) => true,
                        (None, None) => false,
                        (Some(_), Some(cal::ParamFilterMatch::IsNotDefined)) => false,
                        (None, Some(cal::ParamFilterMatch::IsNotDefined)) => true,
                        (None, Some(cal::ParamFilterMatch::Match(_))) => false,
                        (Some(param), Some(cal::ParamFilterMatch::Match(txt_match))) => {
                            let param_val = match &param.val {
                                Some(v) => v,
                                None => return false,
                            };

                            match txt_match.negate_condition {
                                None | Some(false) => {
                                    param_val.as_str().contains(txt_match.text.as_str())
                                }
                                Some(true) => !param_val.as_str().contains(txt_match.text.as_str()),
                            }
                        }
                    }
                })
            }
        }
    })
}

fn resolve_trigger(
    parent: &icalendar::parser::Component,
    properties: &[icalendar::parser::Property],
) -> Option<chrono::DateTime<chrono::Utc>> {
    // A. Do we have a TRIGGER property? If not, returns early
    let maybe_trigger_prop = properties
        .iter()
        .find(|candidate| candidate.name.as_str() == "TRIGGER");

    let trigger_prop = match maybe_trigger_prop {
        None => return None,
        Some(v) => v,
    };

    // B.1 Is it an absolute datetime? If so, returns early
    let maybe_absolute = trigger_prop
        .params
        .iter()
        .find(|param| param.key.as_str() == "VALUE")
        .map(|param| param.val.as_ref())
        .flatten()
        .map(|v| v.as_str() == "DATE-TIME");

    if maybe_absolute.is_some() {
        let final_date = prop_date(properties, "TRIGGER");
        tracing::trace!(trigger=?final_date, "resolved absolute trigger");
        return final_date;
    }

    // B.2 Otherwise it's a timedelta relative to a parent field.
    // C.1 Parse the timedelta value, returns early if invalid
    let (_, time_delta) = parser::dur_value(trigger_prop.val.as_str()).ok()?;

    // C.2 Get the parent reference absolute datetime, returns early if invalid
    let maybe_bound = trigger_prop
        .params
        .iter()
        .find(|param| param.key.as_str() == "RELATED")
        .map(|param| param.val.as_ref())
        .flatten();

    // If the trigger is set relative to START, then the "DTSTART" property MUST be present in the associated
    // "VEVENT" or "VTODO" calendar component.
    //
    // If an alarm is specified for an event with the trigger set relative to the END,
    // then the "DTEND" property or the "DTSTART" and "DURATION " properties MUST be present
    // in the associated "VEVENT" calendar component.
    //
    // If the alarm is specified for a to-do with a trigger set relative to the END,
    // then either the "DUE" property or the "DTSTART" and "DURATION " properties
    // MUST be present in the associated "VTODO" calendar component.
    let related_field = match maybe_bound.as_ref().map(|v| v.as_str()) {
        Some("START") => "DTSTART",
        Some("END") => "DTEND", //@FIXME must add support for DUE, DTSTART, and DURATION
        _ => "DTSTART",         // by default use DTSTART
    };
    let parent_date = match prop_date(parent.properties.as_ref(), related_field) {
        Some(v) => v,
        _ => return None,
    };

    // C.3 Compute the final date from the base date + timedelta
    let final_date = parent_date + time_delta;
    tracing::trace!(trigger=?final_date, "resolved relative trigger");
    Some(final_date)
}

fn is_in_time_range(
    component: &cal::Component,
    parent: &icalendar::parser::Component,
    properties: &[icalendar::parser::Property],
    time_range: &cal::TimeRange,
) -> bool {
    //@FIXME timezones are not properly handled currently (everything is UTC)
    //@FIXME does not support repeat
    //ref: https://datatracker.ietf.org/doc/html/rfc4791#section-9.9
    let (start, end) = match time_range {
        cal::TimeRange::OnlyStart(start) => (start, &chrono::DateTime::<chrono::Utc>::MAX_UTC),
        cal::TimeRange::OnlyEnd(end) => (&chrono::DateTime::<chrono::Utc>::MIN_UTC, end),
        cal::TimeRange::FullRange(start, end) => (start, end),
    };

    match component {
        cal::Component::VEvent => {
            let dtstart = match prop_date(properties, "DTSTART") {
                Some(v) => v,
                _ => return false,
            };
            let maybe_dtend = prop_date(properties, "DTEND");
            let maybe_duration = prop_parse::<i64>(properties, "DURATION")
                .map(|d| chrono::TimeDelta::new(std::cmp::max(d, 0), 0))
                .flatten();

            //@FIXME missing "date" management (only support "datetime")
            match (&maybe_dtend, &maybe_duration) {
                //       | Y | N | N | * | (start <  DTEND AND end > DTSTART)            |
                (Some(dtend), _) => start < dtend && end > &dtstart,
                //       | N | Y | Y | * | (start <  DTSTART+DURATION AND end > DTSTART) |
                (_, Some(duration)) => *start <= dtstart + *duration && end > &dtstart,
                //       | N | N | N | Y | (start <= DTSTART AND end > DTSTART)          |
                _ => start <= &dtstart && end > &dtstart,
            }
        }
        cal::Component::VTodo => {
            let maybe_dtstart = prop_date(properties, "DTSTART");
            let maybe_due = prop_date(properties, "DUE");
            let maybe_completed = prop_date(properties, "COMPLETED");
            let maybe_created = prop_date(properties, "CREATED");
            let maybe_duration = prop_parse::<i64>(properties, "DURATION")
                .map(|d| chrono::TimeDelta::new(d, 0))
                .flatten();

            match (
                maybe_dtstart,
                maybe_duration,
                maybe_due,
                maybe_completed,
                maybe_created,
            ) {
                //    | Y | Y | N | * | * | (start  <= DTSTART+DURATION)  AND             |
                //    |   |   |   |   |   | ((end   >  DTSTART)  OR                       |
                //    |   |   |   |   |   |  (end   >= DTSTART+DURATION))                 |
                (Some(dtstart), Some(duration), None, _, _) => {
                    *start <= dtstart + duration && (*end > dtstart || *end >= dtstart + duration)
                }
                //    | Y | N | Y | * | * | ((start <  DUE)      OR  (start <= DTSTART))  |
                //    |   |   |   |   |   | AND                                           |
                //    |   |   |   |   |   | ((end   >  DTSTART)  OR  (end   >= DUE))      |
                (Some(dtstart), None, Some(due), _, _) => {
                    (*start < due || *start <= dtstart) && (*end > dtstart || *end >= due)
                }
                //    | Y | N | N | * | * | (start  <= DTSTART)  AND (end >  DTSTART)     |
                (Some(dtstart), None, None, _, _) => *start <= dtstart && *end > dtstart,
                //    | N | N | Y | * | * | (start  <  DUE)      AND (end >= DUE)         |
                (None, None, Some(due), _, _) => *start < due && *end >= due,
                //    | N | N | N | Y | Y | ((start <= CREATED)  OR  (start <= COMPLETED))|
                //    |   |   |   |   |   | AND                                           |
                //    |   |   |   |   |   | ((end   >= CREATED)  OR  (end   >= COMPLETED))|
                (None, None, None, Some(completed), Some(created)) => {
                    (*start <= created || *start <= completed)
                        && (*end >= created || *end >= completed)
                }
                //    | N | N | N | Y | N | (start  <= COMPLETED) AND (end  >= COMPLETED) |
                (None, None, None, Some(completed), None) => {
                    *start <= completed && *end >= completed
                }
                //    | N | N | N | N | Y | (end    >  CREATED)                           |
                (None, None, None, None, Some(created)) => *end > created,
                //    | N | N | N | N | N | TRUE                                          |
                _ => true,
            }
        }
        cal::Component::VJournal => {
            let maybe_dtstart = prop_date(properties, "DTSTART");
            match maybe_dtstart {
                //    | Y | Y | (start <= DTSTART)     AND (end > DTSTART) |
                Some(dtstart) => *start <= dtstart && *end > dtstart,
                //    | N | * | FALSE                                      |
                None => false,
            }
        }
        cal::Component::VFreeBusy => {
            //@FIXME freebusy is not supported yet
            false
        }
        cal::Component::VAlarm => {
            //@FIXME does not support REPEAT
            let maybe_trigger = resolve_trigger(parent, properties);
            match maybe_trigger {
                //  (start <= trigger-time) AND (end > trigger-time)
                Some(trigger_time) => *start <= trigger_time && *end > trigger_time,
                _ => false,
            }
        }
        _ => false,
    }
}
