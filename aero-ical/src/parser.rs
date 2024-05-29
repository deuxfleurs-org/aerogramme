use chrono::TimeDelta;

use nom::branch::alt;
use nom::bytes::complete::{tag, tag_no_case};
use nom::character::complete as nomchar;
use nom::combinator::{map, map_opt, opt, value};
use nom::sequence::{pair, tuple};
use nom::IResult;

use aero_dav::caltypes as cal;

//@FIXME too simple, we have 4 cases in practices:
// - floating datetime
// - floating datetime with a tzid as param so convertible to tz datetime
// - utc datetime
// - floating(?) date (without time)
pub fn date_time(dt: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    tracing::trace!(raw_time = dt, "VEVENT raw time");
    let tmpl = match dt.chars().last() {
        Some('Z') => cal::UTC_DATETIME_FMT,
        Some(_) => {
            tracing::warn!(
                raw_time = dt,
                "floating datetime is not properly supported yet"
            );
            cal::FLOATING_DATETIME_FMT
        }
        None => return None,
    };

    chrono::NaiveDateTime::parse_from_str(dt, tmpl)
        .ok()
        .map(|v| v.and_utc())
}

/// RFC3389 Duration Value
///
/// ```abnf
/// dur-value  = (["+"] / "-") "P" (dur-date / dur-time / dur-week)
/// dur-date   = dur-day [dur-time]
/// dur-time   = "T" (dur-hour / dur-minute / dur-second)
/// dur-week   = 1*DIGIT "W"
/// dur-hour   = 1*DIGIT "H" [dur-minute]
/// dur-minute = 1*DIGIT "M" [dur-second]
/// dur-second = 1*DIGIT "S"
/// dur-day    = 1*DIGIT "D"
/// ```
pub fn dur_value(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(
        tuple((
            dur_sign,
            tag_no_case("P"),
            alt((dur_date, dur_time, dur_week)),
        )),
        |(sign, _, delta)| delta.checked_mul(sign),
    )(text)
}

fn dur_sign(text: &str) -> IResult<&str, i32> {
    map(opt(alt((value(1, tag("+")), value(-1, tag("-"))))), |x| {
        x.unwrap_or(1)
    })(text)
}
fn dur_date(text: &str) -> IResult<&str, TimeDelta> {
    map(pair(dur_day, opt(dur_time)), |(day, time)| {
        day + time.unwrap_or(TimeDelta::zero())
    })(text)
}
fn dur_time(text: &str) -> IResult<&str, TimeDelta> {
    map(
        pair(tag_no_case("T"), alt((dur_hour, dur_minute, dur_second))),
        |(_, x)| x,
    )(text)
}
fn dur_week(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(pair(nomchar::i64, tag_no_case("W")), |(i, _)| {
        TimeDelta::try_weeks(i)
    })(text)
}
fn dur_day(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(pair(nomchar::i64, tag_no_case("D")), |(i, _)| {
        TimeDelta::try_days(i)
    })(text)
}
fn dur_hour(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(
        tuple((nomchar::i64, tag_no_case("H"), opt(dur_minute))),
        |(i, _, mm)| TimeDelta::try_hours(i).map(|hours| hours + mm.unwrap_or(TimeDelta::zero())),
    )(text)
}
fn dur_minute(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(
        tuple((nomchar::i64, tag_no_case("M"), opt(dur_second))),
        |(i, _, ms)| TimeDelta::try_minutes(i).map(|min| min + ms.unwrap_or(TimeDelta::zero())),
    )(text)
}
fn dur_second(text: &str) -> IResult<&str, TimeDelta> {
    map_opt(pair(nomchar::i64, tag_no_case("S")), |(i, _)| {
        TimeDelta::try_seconds(i)
    })(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5545_example1() {
        // A duration of 15 days, 5 hours, and 20 seconds would be:
        let to_parse = "P15DT5H0M20S";
        let (_, time_delta) = dur_value(to_parse).unwrap();
        assert_eq!(
            time_delta,
            TimeDelta::try_days(15).unwrap()
                + TimeDelta::try_hours(5).unwrap()
                + TimeDelta::try_seconds(20).unwrap()
        );
    }

    #[test]
    fn rfc5545_example2() {
        // A duration of 7 weeks would be:
        let to_parse = "P7W";
        let (_, time_delta) = dur_value(to_parse).unwrap();
        assert_eq!(time_delta, TimeDelta::try_weeks(7).unwrap());
    }

    #[test]
    fn rfc4791_example1() {
        // 10 minutes before
        let to_parse = "-PT10M";

        let (_, time_delta) = dur_value(to_parse).unwrap();
        assert_eq!(time_delta, TimeDelta::try_minutes(-10).unwrap());
    }

    #[test]
    fn ical_org_example1() {
        // The following example is for a "VALARM" calendar component that specifies an email alarm
        // that will trigger 2 days before the scheduled due DATE-TIME of a to-do with which it is associated.
        let to_parse = "-P2D";

        let (_, time_delta) = dur_value(to_parse).unwrap();
        assert_eq!(time_delta, TimeDelta::try_days(-2).unwrap());
    }
}
