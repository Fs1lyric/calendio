//! Deliberately bounded RFC 5545 event interchange. Unsupported recurrence
//! semantics are errors; unsupported descriptive metadata produces warnings.
use crate::db::{Db, Error, EventInput, Exception, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub skipped: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
struct Property {
    name: String,
    params: HashMap<String, String>,
    value: String,
}
struct Item {
    uid: String,
    event: EventInput,
    details: crate::details::EventDetails,
    details_present: bool,
    original: Option<String>,
    cancelled: bool,
    excluded: Vec<String>,
}
fn bad(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

fn split_unquoted(s: &str, delimiter: char) -> Vec<&str> {
    let mut quoted = false;
    let mut start = 0;
    let mut result = Vec::new();
    for (i, c) in s.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        if c == delimiter && !quoted {
            result.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    result.push(&s[start..]);
    result
}
fn property(line: &str) -> Result<Property> {
    let mut quoted = false;
    let colon = line
        .char_indices()
        .find_map(|(i, c)| {
            if c == '"' {
                quoted = !quoted;
            }
            (c == ':' && !quoted).then_some(i)
        })
        .ok_or_else(|| bad("ICS property has no colon"))?;
    let parts = split_unquoted(&line[..colon], ';');
    let mut params = HashMap::new();
    for part in &parts[1..] {
        let (k, v) = part.split_once('=').ok_or_else(|| bad("Invalid ICS property parameter"))?;
        params.insert(k.to_uppercase(), v.trim_matches('"').to_owned());
    }
    Ok(Property { name: parts[0].to_uppercase(), params, value: line[colon + 1..].to_string() })
}
fn text(s: &str) -> Result<String> {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        result.push(match chars.next() {
            Some('n' | 'N') => '\n',
            Some(c @ ('\\' | ';' | ',')) => c,
            _ => return Err(bad("Invalid ICS text escape")),
        });
    }
    Ok(result)
}
fn escaped(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\\n")
        .replace(';', "\\;")
        .replace(',', "\\,")
}
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}
fn date(p: &Property, fallback: Tz) -> Result<(String, bool, Tz)> {
    let date_only = p.params.get("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    if date_only {
        let day = NaiveDate::parse_from_str(&p.value, "%Y%m%d").map_err(|_| bad("Invalid ICS all-day date"))?;
        return Ok((day.format("%Y-%m-%d").to_string(), true, fallback));
    }
    if let Some(value) = p.value.strip_suffix('Z') {
        let dt = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").map_err(|_| bad("Invalid ICS UTC time"))?;
        return Ok((stamp(Utc.from_utc_datetime(&dt)), false, Tz::UTC));
    }
    let zone = p
        .params
        .get("TZID")
        .map(|s| s.parse::<Tz>())
        .transpose()
        .map_err(|_| bad("Unsupported ICS timezone; use an IANA timezone"))?
        .unwrap_or(fallback);
    let dt = NaiveDateTime::parse_from_str(&p.value, "%Y%m%dT%H%M%S").map_err(|_| bad("Invalid ICS local time"))?;
    let at = zone.from_local_datetime(&dt).earliest().ok_or_else(|| bad("ICS time falls in a daylight-saving gap"))?;
    Ok((stamp(at.with_timezone(&Utc)), false, zone))
}
fn duration(s: &str) -> Result<Duration> {
    let mut seconds = 0i64;
    let mut digits = String::new();
    let mut in_time = false;
    let mut any = false;
    let value = s.strip_prefix('P').ok_or_else(|| bad("Invalid ICS duration"))?;
    for c in value.chars() {
        if c == 'T' {
            in_time = true;
            continue;
        }
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let number = digits.parse::<i64>().map_err(|_| bad("Invalid ICS duration"))?;
        digits.clear();
        let multiplier = match (c, in_time) {
            ('W', false) => 604800,
            ('D', false) => 86400,
            ('H', true) => 3600,
            ('M', true) => 60,
            ('S', true) => 1,
            _ => return Err(bad("Unsupported ICS duration")),
        };
        seconds = number
            .checked_mul(multiplier)
            .and_then(|n| seconds.checked_add(n))
            .ok_or_else(|| bad("ICS duration is too large"))?;
        any = true;
    }
    if !digits.is_empty() || !any {
        return Err(bad("Invalid ICS duration"));
    }
    Duration::try_seconds(seconds).ok_or_else(|| bad("ICS duration is too large"))
}
fn parse_item(properties: &[Property], fallback: Tz, warnings: &mut HashSet<String>) -> Result<Item> {
    let one = |name: &str| -> Result<Option<&Property>> {
        let mut found = properties.iter().filter(|p| p.name == name);
        let first = found.next();
        if found.next().is_some() {
            return Err(bad(format!("Repeated ICS {name}")));
        }
        Ok(first)
    };
    let required =
        |name: &str| -> Result<&Property> { one(name)?.ok_or_else(|| bad(format!("ICS event needs {name}"))) };
    let uid = required("UID")?.value.clone();
    if uid.is_empty() || uid.len() > 1024 {
        return Err(bad("Invalid ICS UID"));
    }
    let fallback = one("X-CALENDIO-TIMEZONE")?
        .map(|p| p.value.parse::<Tz>())
        .transpose()
        .map_err(|_| bad("Unknown Calendio timezone"))?
        .unwrap_or(fallback);
    let (start, all_day, zone) = date(required("DTSTART")?, fallback)?;
    let end = if let Some(p) = one("DTEND")? {
        let (end, end_day, _) = date(p, zone)?;
        if end_day != all_day {
            return Err(bad("ICS start and end must have the same date type"));
        }
        end
    } else {
        let length = one("DURATION")?.map(|p| duration(&p.value)).transpose()?.unwrap_or(if all_day {
            Duration::days(1)
        } else {
            Duration::zero()
        });
        if all_day {
            if length.num_seconds() % 86400 != 0 {
                return Err(bad("All-day duration must be whole days"));
            }
            NaiveDate::parse_from_str(&start, "%Y-%m-%d")
                .unwrap()
                .checked_add_signed(length)
                .ok_or_else(|| bad("ICS end is out of range"))?
                .format("%Y-%m-%d")
                .to_string()
        } else {
            stamp(
                start
                    .parse::<DateTime<Utc>>()
                    .unwrap()
                    .checked_add_signed(length)
                    .ok_or_else(|| bad("ICS end is out of range"))?,
            )
        }
    };
    if one("DTEND")?.is_some() && one("DURATION")?.is_some() {
        return Err(bad("ICS event cannot have both DTEND and DURATION"));
    }
    let original = one("RECURRENCE-ID")?
        .map(|p| {
            if p.params.contains_key("RANGE") {
                return Err(bad("THISANDFUTURE recurrence changes are not supported"));
            }
            let (value, day, _) = date(p, zone)?;
            if day != all_day {
                return Err(bad("Recurrence ID date type differs from event"));
            }
            Ok(value)
        })
        .transpose()?;
    let mut excluded = Vec::new();
    for p in properties {
        match p.name.as_str() {
            "RDATE" | "EXRULE" => return Err(bad(format!("{} recurrence is not supported", p.name))),
            "EXDATE" => {
                for value in p.value.split(',') {
                    let mut p = p.clone();
                    p.value = value.to_string();
                    let (v, day, _) = date(&p, zone)?;
                    if day != all_day {
                        return Err(bad("EXDATE date type differs from event"));
                    }
                    excluded.push(v);
                }
            }
            "UID"
            | "DTSTAMP"
            | "DTSTART"
            | "DTEND"
            | "DURATION"
            | "SUMMARY"
            | "DESCRIPTION"
            | "RRULE"
            | "RECURRENCE-ID"
            | "STATUS"
            | "CREATED"
            | "LAST-MODIFIED"
            | "SEQUENCE"
            | "X-CALENDIO-TIMEZONE"
            | "COLOR"
            | "LOCATION"
            | "URL"
            | "TRANSP"
            | "X-CALENDIO-REMINDER"
            | "X-CALENDIO-ALARM" => {}
            name => {
                warnings.insert(format!("{name} metadata was not imported"));
            }
        }
    }
    let cancelled = one("STATUS")?.is_some_and(|p| p.value.eq_ignore_ascii_case("CANCELLED"));
    if cancelled && original.is_none() {
        return Err(bad("Cancelled series masters are not imported; no existing events were deleted"));
    }
    let event = EventInput {
        title: one("SUMMARY")?.map(|p| text(&p.value)).transpose()?.unwrap_or_else(|| "Untitled event".into()),
        notes: one("DESCRIPTION")?.map(|p| text(&p.value)).transpose()?.unwrap_or_default(),
        start,
        end,
        all_day,
        tz: zone.to_string(),
        rrule: one("RRULE")?.map(|p| p.value.clone()),
        color: one("COLOR")?.map(|p| p.value.clone()),
        reminder_minutes: one("X-CALENDIO-REMINDER")?
            .or(one("X-CALENDIO-ALARM")?)
            .map(|p| p.value.parse::<i64>().map_err(|_| bad("Invalid reminder minutes")))
            .transpose()?,
    };
    let details = crate::details::EventDetails {
        location: one("LOCATION")?.map(|p| text(&p.value)).transpose()?.unwrap_or_default(),
        meeting_url: one("URL")?.map(|p| p.value.clone()).unwrap_or_default(),
        busy: match one("TRANSP")?.map(|p| p.value.to_ascii_uppercase()) {
            Some(v) if v == "TRANSPARENT" => false,
            Some(v) if v == "OPAQUE" => true,
            Some(_) => return Err(bad("Unknown ICS transparency value")),
            None => !event.all_day,
        },
    }
    .clean()?;
    let details_present = properties.iter().any(|p| matches!(p.name.as_str(), "LOCATION" | "URL" | "TRANSP"));
    Ok(Item { uid, event, details, details_present, original, cancelled, excluded })
}
fn parse(input: &str, fallback: Tz) -> Result<(Vec<Item>, Vec<String>)> {
    if input.len() > 8 * 1024 * 1024 {
        return Err(bad("ICS files must be at most 8 MiB"));
    }
    let mut lines: Vec<String> = Vec::new();
    for line in input.trim_start_matches('\u{feff}').lines() {
        if line.starts_with([' ', '\t']) {
            lines.last_mut().ok_or_else(|| bad("Invalid folded ICS line"))?.push_str(&line[1..]);
        } else if !line.is_empty() {
            lines.push(line.to_string());
        }
    }
    let mut stack: Vec<String> = Vec::new();
    let mut properties = Vec::new();
    let mut alarm_properties: Vec<Property> = Vec::new();
    let mut items = Vec::new();
    let mut warnings = HashSet::new();
    let mut complete = false;
    let mut version = false;
    for line in lines {
        let p = property(&line)?;
        if p.name == "BEGIN" {
            if complete {
                return Err(bad("Multiple calendar objects are not supported"));
            }
            let component = p.value.to_uppercase();
            match (stack.last().map(String::as_str), component.as_str()) {
                (None, "VCALENDAR")
                | (Some("VCALENDAR"), "VEVENT")
                | (Some("VEVENT"), "VALARM")
                | (Some("VCALENDAR"), "VTIMEZONE")
                | (Some("VTIMEZONE"), "STANDARD" | "DAYLIGHT") => {}
                _ => return Err(bad(format!("Unsupported ICS component {component}"))),
            }
            if component == "VEVENT" {
                properties.clear();
            }
            if component == "VALARM" {
                alarm_properties.clear();
            }
            if component == "VTIMEZONE" {
                warnings.insert("VTIMEZONE definitions use installed IANA timezone rules".to_string());
            }
            stack.push(component);
            continue;
        }
        if p.name == "END" {
            let component = stack.pop().ok_or_else(|| bad("Unexpected ICS END"))?;
            if !component.eq_ignore_ascii_case(&p.value) {
                return Err(bad("Mismatched ICS component ending"));
            }
            if component == "VALARM" {
                let action = alarm_properties.iter().find(|p| p.name == "ACTION");
                let trigger = alarm_properties.iter().find(|p| p.name == "TRIGGER");
                let minutes = trigger.and_then(|p| {
                    if p.params.get("RELATED").is_some_and(|v| !v.eq_ignore_ascii_case("START"))
                        || p.params.get("VALUE").is_some_and(|v| !v.eq_ignore_ascii_case("DURATION"))
                    {
                        return None;
                    }
                    if p.value.starts_with("PT") {
                        return duration(&p.value).ok().filter(|d| d.is_zero()).map(|_| 0);
                    }
                    p.value
                        .strip_prefix('-')
                        .and_then(|v| duration(v).ok())
                        .filter(|d| d.num_seconds() % 60 == 0)
                        .map(|d| d.num_minutes())
                });
                if let Some(minutes) = minutes.filter(|_| {
                    action.is_some_and(|p| p.value.eq_ignore_ascii_case("DISPLAY"))
                        && !properties.iter().any(|p: &Property| p.name == "X-CALENDIO-ALARM")
                }) {
                    properties.push(Property {
                        name: "X-CALENDIO-ALARM".into(),
                        params: HashMap::new(),
                        value: minutes.to_string(),
                    });
                } else {
                    warnings.insert("An additional or unsupported alarm was not imported; Calendio supports one display reminder before start".into());
                }
            }
            if component == "VEVENT" {
                items.push(parse_item(&properties, fallback, &mut warnings)?);
                if items.len() > 10000 {
                    return Err(bad("Import at most 10,000 events at once"));
                }
            }
            if component == "VCALENDAR" {
                complete = true;
            }
            continue;
        }
        match stack.last().map(String::as_str) {
            Some("VCALENDAR") if p.name == "VERSION" => version = p.value == "2.0",
            Some("VCALENDAR") if p.name == "METHOD" && !p.value.eq_ignore_ascii_case("PUBLISH") => {
                return Err(bad("Meeting requests must be handled as invitations, not imported as calendar files"))
            }
            Some("VEVENT") => properties.push(p),
            Some("VALARM") => alarm_properties.push(p),
            None => return Err(bad("ICS property outside VCALENDAR")),
            _ => {}
        }
    }
    if !complete || !stack.is_empty() || !version {
        return Err(bad("Incomplete ICS calendar or unsupported version"));
    }
    let mut warnings: Vec<_> = warnings.into_iter().collect();
    warnings.sort();
    Ok((items, warnings))
}

fn emit(out: &mut String, line: &str) {
    let mut column = 0;
    for c in line.chars() {
        if column + c.len_utf8() > 75 {
            out.push_str("\r\n ");
            column = 1;
        }
        out.push(c);
        column += c.len_utf8();
    }
    out.push_str("\r\n");
}
fn time_property(name: &str, value: &str, event: &EventInput) -> Result<String> {
    if event.all_day {
        Ok(format!("{name};VALUE=DATE:{}", value.replace('-', "")))
    } else {
        let at = value.parse::<DateTime<Utc>>().map_err(|_| bad("Invalid stored event time"))?;
        let tz = event.tz.parse::<Tz>().map_err(|_| bad("Invalid stored event timezone"))?;
        if tz == Tz::UTC {
            Ok(format!("{name}:{}", at.format("%Y%m%dT%H%M%SZ")))
        } else {
            Ok(format!("{name};TZID={tz}:{}", at.with_timezone(&tz).format("%Y%m%dT%H%M%S")))
        }
    }
}
fn emit_event(
    out: &mut String,
    uid: &str,
    event: &EventInput,
    details: &crate::details::EventDetails,
    original: Option<&str>,
    cancelled: bool,
) -> Result<()> {
    emit(out, "BEGIN:VEVENT");
    emit(out, &format!("UID:{uid}"));
    emit(out, &format!("DTSTAMP:{}", Utc::now().format("%Y%m%dT%H%M%SZ")));
    emit(out, &time_property("DTSTART", &event.start, event)?);
    emit(out, &time_property("DTEND", &event.end, event)?);
    emit(out, &format!("SUMMARY:{}", escaped(&event.title)));
    emit(out, &format!("DESCRIPTION:{}", escaped(&event.notes)));
    if !details.location.is_empty() {
        emit(out, &format!("LOCATION:{}", escaped(&details.location)));
    }
    if !details.meeting_url.is_empty() {
        emit(out, &format!("URL:{}", details.meeting_url));
    }
    emit(out, if details.busy { "TRANSP:OPAQUE" } else { "TRANSP:TRANSPARENT" });
    emit(out, &format!("X-CALENDIO-TIMEZONE:{}", event.tz));
    if let Some(original) = original {
        emit(out, &time_property("RECURRENCE-ID", original, event)?);
    } else if let Some(rule) = &event.rrule {
        let rule = if event.all_day { rule.clone() } else { crate::recur::until_in_utc(event, rule)? };
        emit(out, &format!("RRULE:{rule}"));
    }
    if cancelled {
        emit(out, "STATUS:CANCELLED");
    }
    if let Some(color) = &event.color {
        emit(out, &format!("COLOR:{color}"));
    }
    if let Some(minutes) = event.reminder_minutes {
        emit(out, &format!("X-CALENDIO-REMINDER:{minutes}"));
        emit(out, "BEGIN:VALARM");
        emit(out, "ACTION:DISPLAY");
        emit(out, &format!("TRIGGER:-PT{minutes}M"));
        emit(out, &format!("DESCRIPTION:{}", escaped(&event.title)));
        emit(out, "END:VALARM");
    }
    emit(out, "END:VEVENT");
    Ok(())
}
impl Db {
    pub fn export_ics(&self, calendar_id: i64) -> Result<String> {
        if !self.calendars()?.iter().any(|c| c.id == calendar_id) {
            return Err(bad("Calendar does not exist"));
        }
        let mut out = String::new();
        for line in ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Calendio//Calendar//EN", "CALSCALE:GREGORIAN"] {
            emit(&mut out, line);
        }
        for event in self.all_events()? {
            if self.event_calendar(event.id)? != calendar_id {
                continue;
            }
            let uid = self
                .conn()
                .query_row("SELECT uid FROM event_uids WHERE event_id=?1", [event.id], |r| r.get::<_, String>(0))
                .optional()?
                .unwrap_or_else(|| format!("{}@calendio.local", event.id));
            let details = self.event_details(event.id)?;
            emit_event(&mut out, &uid, &event.input, &details, None, false)?;
            for x in self.exceptions_for(&[event.id])? {
                let mut input = event.input.clone();
                input.rrule = None;
                if let Some(title) = x.title {
                    input.title = title;
                }
                if let Some(notes) = x.notes {
                    input.notes = notes;
                }
                if let (Some(start), Some(end)) = (x.start, x.end) {
                    input.start = start;
                    input.end = end;
                } else if input.all_day {
                    let start =
                        NaiveDate::parse_from_str(&event.start, "%Y-%m-%d").map_err(|_| bad("Invalid stored date"))?;
                    let end =
                        NaiveDate::parse_from_str(&event.end, "%Y-%m-%d").map_err(|_| bad("Invalid stored date"))?;
                    let original = NaiveDate::parse_from_str(&x.original_start, "%Y-%m-%d")
                        .map_err(|_| bad("Invalid stored recurrence date"))?;
                    input.start = x.original_start.clone();
                    input.end = original
                        .checked_add_signed(end - start)
                        .ok_or_else(|| bad("Exception date is out of range"))?
                        .format("%Y-%m-%d")
                        .to_string();
                } else {
                    let start = event.start.parse::<DateTime<Utc>>().map_err(|_| bad("Invalid stored time"))?;
                    let end = event.end.parse::<DateTime<Utc>>().map_err(|_| bad("Invalid stored time"))?;
                    let original =
                        x.original_start.parse::<DateTime<Utc>>().map_err(|_| bad("Invalid recurrence time"))?;
                    input.start = x.original_start.clone();
                    input.end = stamp(
                        original
                            .checked_add_signed(end - start)
                            .ok_or_else(|| bad("Exception time is out of range"))?,
                    );
                }
                emit_event(&mut out, &uid, &input, &details, Some(&x.original_start), x.cancelled)?;
            }
        }
        emit(&mut out, "END:VCALENDAR");
        Ok(out)
    }

    /// Import new UIDs only. Re-import is idempotent and does not overwrite
    /// locally edited events. Entire imports roll back on any validation error.
    pub fn import_ics(&self, calendar_id: i64, input: &str, floating_zone: &str) -> Result<ImportReport> {
        let fallback = floating_zone.parse::<Tz>().map_err(|_| bad("Unknown import timezone"))?;
        let (items, warnings) = parse(input, fallback)?;
        // A savepoint, so a subscription refresh can wrap the import in its own.
        let tx = self.savepoint()?;
        if !self.calendars()?.iter().any(|c| c.id == calendar_id) {
            return Err(bad("Calendar does not exist"));
        }
        let mut report = ImportReport { warnings, ..Default::default() };
        let mut masters = HashMap::new();
        let mut seen = HashSet::new();
        for item in items.iter().filter(|i| i.original.is_none()) {
            if !seen.insert(item.uid.clone()) {
                return Err(bad("Duplicate series master UID in ICS file"));
            }
            let exists = tx
                .query_row(
                    "SELECT event_id FROM event_uids WHERE calendar_id=?1 AND uid=?2",
                    params![calendar_id, item.uid],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?;
            if exists.is_some() {
                report.skipped += 1;
                continue;
            }
            let id = self.create_event(&item.event)?;
            self.set_event_details(id, &item.details)?;
            tx.execute("UPDATE events SET calendar_id=?2 WHERE id=?1", params![id, calendar_id])?;
            tx.execute("DELETE FROM event_uids WHERE event_id=?1", [id])?;
            tx.execute(
                "INSERT INTO event_uids (calendar_id,uid,event_id) VALUES (?1,?2,?3)",
                params![calendar_id, item.uid, id],
            )?;
            masters.insert(item.uid.clone(), id);
            report.imported += 1;
            for original in &item.excluded {
                self.set_exception(&Exception {
                    event_id: id,
                    original_start: original.clone(),
                    cancelled: true,
                    title: None,
                    notes: None,
                    start: None,
                    end: None,
                })?;
            }
        }
        let mut occurrences = HashSet::new();
        for item in items.iter().filter(|i| i.original.is_some()) {
            let original = item.original.as_ref().unwrap();
            if !occurrences.insert((&item.uid, original)) {
                return Err(bad("Duplicate recurrence exception in ICS file"));
            }
            if let Some(&id) = masters.get(&item.uid) {
                let master = self.event(id)?.ok_or_else(|| bad("Missing series master"))?;
                if item.details_present && item.details != self.event_details(id)? {
                    return Err(bad("Per-occurrence location, meeting link, or availability changes are not supported; edit the series instead"));
                }
                if master.all_day != item.event.all_day {
                    return Err(bad("Exception and series date types differ"));
                }
                self.set_exception(&Exception {
                    event_id: id,
                    original_start: original.clone(),
                    cancelled: item.cancelled,
                    title: Some(item.event.title.clone()),
                    notes: Some(item.event.notes.clone()),
                    start: Some(item.event.start.clone()),
                    end: Some(item.event.end.clone()),
                })?;
            } else if !seen.contains(&item.uid) {
                return Err(bad("An imported recurrence exception needs its series master"));
            }
        }
        tx.commit()?;
        Ok(report)
    }
}
