//! A small bounded iCalendar (RFC 5545) reader for invitation mail: the
//! calendar METHOD and the first VEVENT's identity and times. Malformed input
//! yields None; slicing happens only at ASCII delimiters, so nothing panics.
use chrono::{
    DateTime, Days, FixedOffset, NaiveDate, NaiveDateTime, SecondsFormat, TimeDelta, TimeZone, Utc,
};
use fm_calendar::Tz;
use std::collections::BTreeMap;

/// Calendar parts larger than this are not read.
pub(crate) const MAX_PART_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 8;
const MAX_UID_BYTES: usize = 1024;
const MAX_SUMMARY_CHARS: usize = 2048;
/// Ten years; longer DURATION values are treated as malformed.
const MAX_SPAN_SECONDS: i64 = 10 * 366 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    Date(NaiveDate),
    Instant(DateTime<Utc>),
}
impl When {
    /// The Calendar projection's occurrence identity (`EventTime::identity`).
    pub(crate) fn identity(&self) -> String {
        match self {
            Self::Date(date) => date.to_string(),
            Self::Instant(instant) => instant.to_rfc3339(),
        }
    }
    /// Public form: an RFC 3339 UTC instant or a YYYY-MM-DD date.
    pub(crate) fn public(&self) -> String {
        match self {
            Self::Date(date) => date.to_string(),
            Self::Instant(instant) => instant.to_rfc3339_opts(SecondsFormat::Secs, true),
        }
    }
    pub(crate) fn all_day(&self) -> bool {
        matches!(self, Self::Date(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Times {
    pub start: When,
    /// Exclusive, as in Google: the day after an all-day event's last day.
    pub end: When,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecurrenceId {
    Absent,
    Known(When),
    /// Present, but floating or in an unresolvable zone.
    Unknown,
}

#[derive(Debug, Clone)]
pub(crate) struct Invite {
    pub uid: String,
    /// Uppercase iTIP method, or empty when the calendar names none.
    pub method: String,
    pub sequence: i64,
    pub summary: String,
    pub recurrence: RecurrenceId,
    /// None when the times are floating, malformed or in an unresolvable zone.
    pub times: Option<Times>,
}

struct Property {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}
impl Property {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Default)]
struct VTimezone {
    location: Option<String>,
    standard_offset: Option<FixedOffset>,
    daylight: bool,
}

enum Zone {
    Named(Tz),
    Fixed(FixedOffset),
}
impl Zone {
    fn to_utc(&self, local: NaiveDateTime) -> Option<DateTime<Utc>> {
        match self {
            // A time skipped by a forward transition uses the offset before
            // the gap (RFC 5545 3.3.5), which equals the next hour's instant.
            Self::Named(zone) => zone
                .from_local_datetime(&local)
                .earliest()
                .or_else(|| {
                    local
                        .checked_add_signed(TimeDelta::hours(1))
                        .and_then(|later| zone.from_local_datetime(&later).earliest())
                })
                .map(|time| time.with_timezone(&Utc)),
            Self::Fixed(offset) => offset
                .from_local_datetime(&local)
                .single()
                .map(|time| time.with_timezone(&Utc)),
        }
    }
}

/// Parse one calendar object. `method_hint` is the MIME part's `method`
/// parameter, used only when the calendar itself has no METHOD.
pub(crate) fn parse(text: &str, method_hint: Option<&str>) -> Option<Invite> {
    if text.len() > MAX_PART_BYTES {
        return None;
    }
    let mut stack: Vec<String> = Vec::new();
    let mut method = None;
    let mut event: Option<Vec<Property>> = None;
    let mut current_event: Option<Vec<Property>> = None;
    let mut zones = BTreeMap::<String, VTimezone>::new();
    let mut current_zone: Option<(Option<String>, VTimezone)> = None;
    for line in unfold(text) {
        let Some(property) = property(&line) else {
            continue;
        };
        let depth = stack.len();
        match property.name.as_str() {
            "BEGIN" => {
                let component = property.value.trim().to_ascii_uppercase();
                if depth == 0 {
                    // Text before the calendar object is not part of it.
                    if component == "VCALENDAR" {
                        stack.push(component);
                    }
                    continue;
                }
                if depth >= MAX_DEPTH {
                    return None;
                }
                match (depth, component.as_str()) {
                    (1, "VEVENT") if event.is_none() => current_event = Some(Vec::new()),
                    (1, "VTIMEZONE") => current_zone = Some((None, VTimezone::default())),
                    (2, "DAYLIGHT") if stack[1] == "VTIMEZONE" => {
                        if let Some((_, zone)) = &mut current_zone {
                            zone.daylight = true;
                        }
                    }
                    _ => {}
                }
                stack.push(component);
            }
            "END" => {
                if depth == 0 {
                    continue;
                }
                let component = property.value.trim().to_ascii_uppercase();
                if stack.last() != Some(&component) {
                    return None;
                }
                stack.pop();
                match (depth, component.as_str()) {
                    (2, "VEVENT") => {
                        if let Some(properties) = current_event.take() {
                            event = Some(properties);
                        }
                    }
                    (2, "VTIMEZONE") => {
                        if let Some((Some(id), zone)) = current_zone.take() {
                            zones.entry(id).or_insert(zone);
                        }
                    }
                    // Only the first calendar object counts.
                    (1, _) => break,
                    _ => {}
                }
            }
            name => match depth {
                1 if name == "METHOD" && method.is_none() => {
                    method = Some(property.value.trim().to_ascii_uppercase())
                }
                // Nested VALARM properties never reach the event.
                2 if stack[1] == "VEVENT" => {
                    if let Some(properties) = &mut current_event {
                        properties.push(property);
                    }
                }
                2 if stack[1] == "VTIMEZONE" => {
                    if let Some((id, zone)) = &mut current_zone {
                        match name {
                            "TZID" if id.is_none() => *id = Some(property.value.trim().into()),
                            "X-LIC-LOCATION" => zone.location = Some(property.value.trim().into()),
                            _ => {}
                        }
                    }
                }
                3 if stack[1] == "VTIMEZONE" && stack[2] == "STANDARD" && name == "TZOFFSETTO" => {
                    if let Some((_, zone)) = &mut current_zone {
                        if zone.standard_offset.is_none() {
                            zone.standard_offset = offset(&property.value);
                        }
                    }
                }
                _ => {}
            },
        }
    }
    let properties = event?;
    let first = |name: &str| properties.iter().find(|p| p.name == name);
    let uid = unescape(first("UID")?.value.trim());
    if uid.is_empty() || uid.len() > MAX_UID_BYTES || uid.chars().any(char::is_control) {
        return None;
    }
    let method = method
        .or_else(|| method_hint.map(|hint| hint.trim().to_ascii_uppercase()))
        .filter(|m| m.len() <= 64 && m.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .unwrap_or_default();
    let sequence = first("SEQUENCE")
        .and_then(|p| p.value.trim().parse::<i64>().ok())
        .filter(|n| *n >= 0)
        .unwrap_or(0);
    let summary = first("SUMMARY")
        .map(|p| {
            unescape(&p.value)
                .trim()
                .chars()
                .take(MAX_SUMMARY_CHARS)
                .collect()
        })
        .unwrap_or_default();
    let recurrence = match first("RECURRENCE-ID") {
        None => RecurrenceId::Absent,
        Some(p) => when(p, &zones).map_or(RecurrenceId::Unknown, RecurrenceId::Known),
    };
    let times = first("DTSTART").and_then(|start| {
        times(
            start,
            first("DTEND"),
            first("DURATION").map(|p| p.value.as_str()),
            &zones,
        )
    });
    Some(Invite {
        uid,
        method,
        sequence,
        summary,
        recurrence,
        times,
    })
}

/// Join folded lines (CRLF or bare LF followed by a space or tab).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = raw.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
            }
        } else if !raw.is_empty() {
            lines.push(raw.into());
        }
    }
    lines
}

/// `NAME *(;PARAM=VALUE) : VALUE`, with quoted parameter values.
fn property(line: &str) -> Option<Property> {
    let mut quoted = false;
    let mut name_end = None;
    let mut segment = 0;
    let mut raw_params = Vec::new();
    let mut value_start = None;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b';' | b':' if !quoted => {
                if name_end.is_none() {
                    name_end = Some(index);
                } else {
                    raw_params.push(&line[segment..index]);
                }
                segment = index + 1;
                if byte == b':' {
                    value_start = Some(index + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let name = line[..name_end?].trim();
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return None;
    }
    let params = raw_params
        .into_iter()
        .filter_map(|param| {
            let (key, value) = param.split_once('=')?;
            Some((
                key.trim().to_ascii_uppercase(),
                value.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect();
    Some(Property {
        name: name.to_ascii_uppercase(),
        params,
        value: line[value_start?..].into(),
    })
}

/// RFC 5545 TEXT escapes: \n, \N, \, \; and \\.
fn unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            output.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => output.push('\n'),
            Some(other) => output.push(other),
            None => output.push('\\'),
        }
    }
    output
}

fn digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

/// A DATE or DATE-TIME value. Floating and unresolvable local times are None.
fn when(property: &Property, zones: &BTreeMap<String, VTimezone>) -> Option<When> {
    let value = property.value.trim();
    let bytes = value.as_bytes();
    if property
        .param("VALUE")
        .is_some_and(|kind| kind.eq_ignore_ascii_case("DATE"))
        || bytes.len() == 8
    {
        return (bytes.len() == 8 && digits(bytes))
            .then(|| NaiveDate::parse_from_str(value, "%Y%m%d").ok())
            .flatten()
            .map(When::Date);
    }
    let (local, utc) = match bytes.last() {
        Some(b'Z' | b'z') => (&bytes[..bytes.len() - 1], true),
        _ => (bytes, false),
    };
    // Byte checks first: a passing value is pure ASCII, so slicing is safe.
    if local.len() != 15 || local[8] != b'T' || !digits(&local[..8]) || !digits(&local[9..]) {
        return None;
    }
    let local = NaiveDateTime::parse_from_str(&value[..15], "%Y%m%dT%H%M%S").ok()?;
    if utc {
        return Some(When::Instant(Utc.from_utc_datetime(&local)));
    }
    resolve_zone(property.param("TZID")?, zones)?
        .to_utc(local)
        .map(When::Instant)
}

fn times(
    start: &Property,
    end: Option<&Property>,
    duration: Option<&str>,
    zones: &BTreeMap<String, VTimezone>,
) -> Option<Times> {
    let start = when(start, zones)?;
    let end = match (end, duration) {
        (Some(end), _) => when(end, zones)?,
        (None, Some(duration)) => {
            let span = span(duration)?;
            match start {
                When::Date(date) if span.num_seconds() % 86_400 == 0 => {
                    When::Date(date.checked_add_days(Days::new(span.num_days().try_into().ok()?))?)
                }
                When::Date(_) => return None,
                When::Instant(at) => When::Instant(at.checked_add_signed(span)?),
            }
        }
        // RFC 5545: a date lasts one day and a date-time is a point in time.
        (None, None) => match start {
            When::Date(date) => When::Date(date.succ_opt()?),
            When::Instant(at) => When::Instant(at),
        },
    };
    match (start, end) {
        (When::Date(from), When::Date(to)) if to <= from => Some(Times {
            start,
            end: When::Date(from.succ_opt()?),
        }),
        (When::Date(_), When::Date(_)) => Some(Times { start, end }),
        (When::Instant(from), When::Instant(to)) if to >= from => Some(Times { start, end }),
        _ => None,
    }
}

/// A non-negative DURATION such as P1D, PT1H30M or P2W.
fn span(value: &str) -> Option<TimeDelta> {
    let value = value.trim();
    let value = value.strip_prefix('+').unwrap_or(value);
    let rest = value.strip_prefix(['P', 'p'])?;
    let mut total: i64 = 0;
    let mut number: Option<i64> = None;
    let mut time = false;
    let mut any = false;
    for c in rest.chars() {
        if let Some(digit) = c.to_digit(10) {
            number = Some(
                number
                    .unwrap_or(0)
                    .checked_mul(10)?
                    .checked_add(i64::from(digit))?,
            );
            continue;
        }
        let unit = c.to_ascii_uppercase();
        if unit == 'T' && !time && number.is_none() {
            time = true;
            continue;
        }
        let scale = match (time, unit) {
            (false, 'W') => 604_800,
            (false, 'D') => 86_400,
            (true, 'H') => 3_600,
            (true, 'M') => 60,
            (true, 'S') => 1,
            _ => return None,
        };
        total = total.checked_add(number.take()?.checked_mul(scale)?)?;
        any = true;
    }
    if number.is_some() || !any || total > MAX_SPAN_SECONDS {
        return None;
    }
    TimeDelta::try_seconds(total)
}

/// TZOFFSETTO: +HHMM or +HHMMSS.
fn offset(value: &str) -> Option<FixedOffset> {
    let bytes = value.trim().as_bytes();
    let sign = match bytes.first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let body = &bytes[1..];
    if !(body.len() == 4 || body.len() == 6) || !digits(body) {
        return None;
    }
    let number = |at: usize| i32::from(body[at] - b'0') * 10 + i32::from(body[at + 1] - b'0');
    let seconds = number(0) * 3600 + number(2) * 60 + if body.len() == 6 { number(4) } else { 0 };
    FixedOffset::east_opt(sign * seconds)
}

/// Common Windows zone names (CLDR's primary mapping) that Outlook uses as TZID.
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("Canada Central Standard Time", "America/Regina"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("UTC", "Etc/UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("India Standard Time", "Asia/Kolkata"),
    ("China Standard Time", "Asia/Shanghai"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
];

fn resolve_zone(tzid: &str, zones: &BTreeMap<String, VTimezone>) -> Option<Zone> {
    let tzid = tzid.trim();
    if tzid.is_empty() || tzid.len() > 256 {
        return None;
    }
    if let Ok(zone) = tzid.parse::<Tz>() {
        return Some(Zone::Named(zone));
    }
    if let Some((_, name)) = WINDOWS_ZONES
        .iter()
        .find(|(windows, _)| windows.eq_ignore_ascii_case(tzid))
    {
        return name.parse::<Tz>().ok().map(Zone::Named);
    }
    // Vendor-prefixed names: /mozilla.org/20050126_1/America/New_York.
    let mut rest = tzid;
    while let Some((_, tail)) = rest.split_once('/') {
        if let Ok(zone) = tail.parse::<Tz>() {
            return Some(Zone::Named(zone));
        }
        rest = tail;
    }
    let definition = zones.get(tzid)?;
    if let Some(zone) = definition
        .location
        .as_deref()
        .and_then(|name| name.parse::<Tz>().ok())
    {
        return Some(Zone::Named(zone));
    }
    // Without daylight rules the standard offset is the whole definition.
    if !definition.daylight {
        return definition.standard_offset.map(Zone::Fixed);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calendar(event: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\n{event}\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n")
    }

    #[test]
    fn folded_escaped_text_and_first_event_only() {
        let text = "BEGIN:VCALENDAR\r\nMETHOD:request\r\nBEGIN:VEVENT\r\nUID:abc\r\n 123@google.com\r\nSUMMARY:Plan\\, review\\; and \\\\ \r\n\tnext\\nsteps\r\nDTSTART:20260929T190000Z\r\nDURATION:PT1H30M\r\nSEQUENCE:3\r\nBEGIN:VALARM\r\nSUMMARY:Alarm text\r\nDTSTART:20000101T000000Z\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:second\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let invite = parse(text, None).unwrap();
        assert_eq!(invite.uid, "abc123@google.com");
        assert_eq!(invite.method, "REQUEST");
        assert_eq!(invite.sequence, 3);
        assert_eq!(invite.summary, "Plan, review; and \\ next\nsteps");
        let times = invite.times.unwrap();
        assert_eq!(times.start.public(), "2026-09-29T19:00:00Z");
        assert_eq!(times.end.public(), "2026-09-29T20:30:00Z");
        assert_eq!(invite.recurrence, RecurrenceId::Absent);
    }

    #[test]
    fn dates_zones_and_durations() {
        let all_day = parse(
            &calendar("UID:a\r\nDTSTART;VALUE=DATE:20261010\r\nDURATION:P2D"),
            None,
        )
        .unwrap();
        let times = all_day.times.unwrap();
        assert!(times.start.all_day());
        assert_eq!(
            (times.start.public(), times.end.public()),
            ("2026-10-10".into(), "2026-10-12".into())
        );
        let one_day = parse(&calendar("UID:a\r\nDTSTART;VALUE=DATE:20261010"), None).unwrap();
        assert_eq!(one_day.times.unwrap().end.public(), "2026-10-11");
        let quoted = parse(
            &calendar("UID:a\r\nDTSTART;TZID=\"Pacific Standard Time\":20260929T120000\r\nDTEND;TZID=America/Los_Angeles:20260929T130000"),
            None,
        )
        .unwrap();
        let times = quoted.times.unwrap();
        assert_eq!(times.start.public(), "2026-09-29T19:00:00Z");
        assert_eq!(times.end.public(), "2026-09-29T20:00:00Z");
        let prefixed = parse(
            &calendar(
                "UID:a\r\nDTSTART;TZID=/mozilla.org/20050126_1/Europe/London:20260115T090000",
            ),
            None,
        )
        .unwrap();
        assert_eq!(
            prefixed.times.unwrap().start.public(),
            "2026-01-15T09:00:00Z"
        );
        // A spring-forward gap uses the offset before the gap.
        let gap = parse(
            &calendar("UID:a\r\nDTSTART;TZID=America/New_York:20260308T023000"),
            None,
        )
        .unwrap();
        assert_eq!(gap.times.unwrap().start.public(), "2026-03-08T07:30:00Z");
        for unusable in [
            "UID:a\r\nDTSTART:20260929T120000",
            "UID:a\r\nDTSTART;TZID=Nowhere/Special:20260929T120000",
            "UID:a\r\nDTSTART:20260929T190000Z\r\nDTEND:20260929T180000Z",
            "UID:a\r\nDTSTART;VALUE=DATE:20260929\r\nDTEND:20260929T180000Z",
            "UID:a\r\nDTSTART:20260929T190000Z\r\nDURATION:-PT1H",
            "UID:a\r\nDTSTART;VALUE=DATE:20260929\r\nDURATION:PT1H",
            "UID:a\r\nDTSTART:2026-09-29",
            "UID:a\r\nDTSTART:20260929T19000é",
        ] {
            let invite = parse(&calendar(unusable), None).unwrap();
            assert!(invite.times.is_none(), "{unusable}");
        }
    }

    #[test]
    fn vtimezone_definitions_resolve_only_when_exact() {
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VTIMEZONE\r\nTZID:(UTC+05:30) Chennai\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0530\r\nTZOFFSETTO:+0530\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VTIMEZONE\r\nTZID:Custom Eastern\r\nBEGIN:STANDARD\r\nTZOFFSETTO:-0500\r\nEND:STANDARD\r\nBEGIN:DAYLIGHT\r\nTZOFFSETTO:-0400\r\nEND:DAYLIGHT\r\nEND:VTIMEZONE\r\nBEGIN:VTIMEZONE\r\nTZID:Located\r\nX-LIC-LOCATION:Europe/Paris\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:z\r\nDTSTART;TZID=\"(UTC+05:30) Chennai\":20260929T103000\r\nRECURRENCE-ID;TZID=Custom Eastern:20260929T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let invite = parse(text, Some("cancel")).unwrap();
        assert_eq!(invite.method, "CANCEL");
        assert_eq!(invite.times.unwrap().start.public(), "2026-09-29T05:00:00Z");
        assert_eq!(invite.recurrence, RecurrenceId::Unknown);
        let located = parse(&text.replace("TZID=Custom Eastern", "TZID=Located"), None).unwrap();
        assert_eq!(
            located.recurrence,
            RecurrenceId::Known(When::Instant("2026-09-29T08:00:00Z".parse().unwrap()))
        );
    }

    #[test]
    fn every_windows_zone_maps_to_a_known_zone() {
        for (windows, zone) in WINDOWS_ZONES {
            assert!(zone.parse::<Tz>().is_ok(), "{windows} -> {zone}");
        }
    }

    #[test]
    fn malformed_input_is_rejected_without_panicking() {
        for text in [
            "",
            "garbage",
            "BEGIN:VEVENT\r\nUID:a\r\nEND:VEVENT",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20260929T190000Z\r\nEND:VEVENT\r\nEND:VCALENDAR",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nEND:VTODO\r\nEND:VCALENDAR",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\n",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID;X=\"unterminated:a\r\nEND:VEVENT\r\nEND:VCALENDAR",
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:\r\nEND:VEVENT\r\nEND:VCALENDAR",
        ] {
            assert!(parse(text, None).is_none(), "{text:?}");
        }
        let deep = format!(
            "BEGIN:VCALENDAR\r\n{}UID:a\r\n",
            "BEGIN:X\r\n".repeat(MAX_DEPTH + 1)
        );
        assert!(parse(&deep, None).is_none());
        assert!(parse(&"X".repeat(MAX_PART_BYTES + 1), None).is_none());
        // Multibyte characters at every delimiter position never split a char.
        let base = calendar(
            "UID:é\r\nDTSTART;TZID=\"é\":2026é929T190000Z\r\nDURATION:Pé1D\r\nSUMMARY:\\é",
        );
        for cut in (0..base.len()).filter(|i| base.is_char_boundary(*i)) {
            let _ = parse(&base[..cut], None);
            let _ = parse(&format!("{}é{}", &base[..cut], &base[cut..]), Some("é"));
        }
    }
}
