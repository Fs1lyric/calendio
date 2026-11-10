//! Google Calendar projection: a bounded window, expanded recurring instances,
//! incremental pulls and ETag-guarded writes. Booking times remain authoritative.
use crate::State;
use calendio_core::{
    db::{Db, Error, EventInput, Exception},
    delivery::random_secret,
    details::EventDetails,
};
use chrono::{DateTime, Duration, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[derive(Clone)]
pub struct Request {
    pub method: &'static str,
    pub event: Option<String>,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub etag: Option<String>,
}
pub struct Reply {
    pub status: u16,
    pub body: Value,
}
pub trait Api {
    async fn call(&self, request: Request) -> Result<Reply, String>;
}
#[derive(Default)]
pub struct Stats {
    pub pulled: usize,
    pub pushed: usize,
    pub conflicts: usize,
}
#[derive(Clone)]
struct Link {
    key: String,
    remote: String,
    etag: String,
    fingerprint: String,
    deleted: bool,
}
#[derive(Clone)]
struct Wire {
    key: String,
    body: Value,
}
fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}
fn failure(e: Error) -> String {
    e.to_string()
}
fn key(db: &Db, id: i64, original: Option<String>) -> Result<String, String> {
    let snapshot = db.snapshot(id).map_err(failure)?;
    serde_json::to_string(&(snapshot.calendar_id, snapshot.uid, original))
        .map_err(|_| "Cannot encode sync identity".into())
}
fn local_id(db: &Db, key: &str) -> Result<Option<(i64, Option<String>)>, String> {
    let (calendar, uid, original): (i64, String, Option<String>) =
        serde_json::from_str(key).map_err(|_| "Invalid Google mapping")?;
    use rusqlite_compat::Optional;
    let id = Optional::event_id(db, calendar, &uid)?;
    Ok(id.map(|id| (id, original)))
}
// Keep SQL error handling at the storage boundary, without exposing DB internals.
mod rusqlite_compat {
    use super::*;
    pub struct Optional;
    impl Optional {
        pub fn event_id(db: &Db, calendar: i64, uid: &str) -> Result<Option<i64>, String> {
            let mut stmt = db
                .conn()
                .prepare("SELECT event_id FROM event_uids WHERE calendar_id=?1 AND uid=?2")
                .map_err(|_| "Cannot read sync identity")?;
            let mut rows =
                stmt.query([calendar.to_string(), uid.to_string()]).map_err(|_| "Cannot read sync identity")?;
            rows.next()
                .map_err(|_| "Cannot read sync identity")?
                .map(|r| r.get(0).map_err(|_| "Invalid sync identity".into()))
                .transpose()
        }
    }
}
fn links(db: &Db) -> Result<Vec<Link>, String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT local_uid,remote_id,etag,fingerprint,deleted FROM google_events")
        .map_err(|_| "Cannot read Google state")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Link { key: r.get(0)?, remote: r.get(1)?, etag: r.get(2)?, fingerprint: r.get(3)?, deleted: r.get(4)? })
        })
        .map_err(|_| "Cannot read Google state")?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|_| "Invalid Google state".into())
}
fn save_link(db: &Db, link: &Link) -> Result<(), String> {
    db.conn().execute("INSERT INTO google_events(local_uid,remote_id,etag,fingerprint,deleted) VALUES (?1,?2,?3,?4,0) ON CONFLICT(local_uid) DO UPDATE SET remote_id=excluded.remote_id,etag=excluded.etag,fingerprint=excluded.fingerprint,deleted=0",[&link.key,&link.remote,&link.etag,&link.fingerprint]).map_err(|_|"Cannot persist Google mapping")?;
    Ok(())
}
fn remove_link(db: &Db, key: &str) -> Result<(), String> {
    db.conn()
        .execute("DELETE FROM google_events WHERE local_uid=?1", [key])
        .map_err(|_| "Cannot remove Google mapping")?;
    Ok(())
}
fn projection(db: &Db, now: DateTime<Utc>) -> Result<Vec<Wire>, String> {
    let from = stamp(now - Duration::days(90));
    let to = stamp(now + Duration::days(365));
    let mut result = Vec::new();
    for o in db.occurrences_between(&from, &to).map_err(failure)? {
        // Subscribed feeds are someone else's calendar; never copy them into Google.
        if db.is_subscribed(db.event_calendar(o.event_id).map_err(failure)?).map_err(failure)? {
            continue;
        }
        let event = db.event(o.event_id).map_err(failure)?.ok_or("Missing sync event")?;
        let details = db.event_details(o.event_id).map_err(failure)?;
        let identity = key(db, o.event_id, o.original_start.clone())?;
        let date =
            |value: &str| if o.all_day { json!({"date":value}) } else { json!({"dateTime":value,"timeZone":event.tz}) };
        let mut body = json!({"summary":o.title,"description":o.notes,"location":details.location,"start":date(&o.start),"end":date(&o.end),"transparency":if details.busy{"opaque"}else{"transparent"},"status":"confirmed","extendedProperties":{"private":{"calendioKey":identity}}});
        if !details.meeting_url.is_empty() {
            body["description"] = json!(format!("{}\n\nMeeting: {}", o.notes, details.meeting_url));
            body["extendedProperties"]["private"]["calendioMeeting"] = json!(details.meeting_url);
        }
        result.push(Wire { key: identity, body });
    }
    Ok(result)
}
fn google_date(value: &Value, fallback: Tz) -> Result<(String, bool, Tz), String> {
    if let Some(date) = value["date"].as_str() {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "Invalid Google all-day date")?;
        return Ok((date.into(), true, fallback));
    }
    let raw = value["dateTime"].as_str().ok_or("Google event has no start or end")?;
    let zone = value["timeZone"]
        .as_str()
        .map(str::parse::<Tz>)
        .transpose()
        .map_err(|_| "Unknown Google timezone")?
        .unwrap_or(fallback);
    let instant = if let Ok(at) = raw.parse::<DateTime<Utc>>() {
        at
    } else {
        let local =
            NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f").map_err(|_| "Invalid Google date-time")?;
        zone.from_local_datetime(&local).earliest().ok_or("Google event falls in a DST gap")?.with_timezone(&Utc)
    };
    Ok((stamp(instant), false, zone))
}
fn decode(value: &Value) -> Result<(EventInput, EventDetails), String> {
    let (start, all_day, zone) = google_date(&value["start"], chrono_tz::UTC)?;
    let (end, end_day, _) = google_date(&value["end"], zone)?;
    if all_day != end_day {
        return Err("Google start/end types differ".into());
    }
    let meeting = value["extendedProperties"]["private"]["calendioMeeting"]
        .as_str()
        .or(value["hangoutLink"].as_str())
        .unwrap_or("");
    let mut notes = value["description"].as_str().unwrap_or("").to_string();
    if !meeting.is_empty() {
        if let Some(original) = notes.strip_suffix(&format!("\n\nMeeting: {meeting}")) {
            notes = original.into();
        }
    }
    let details = EventDetails {
        location: value["location"].as_str().unwrap_or("").into(),
        meeting_url: meeting.into(),
        busy: value["transparency"] != "transparent",
    }
    .clean()
    .map_err(failure)?;
    Ok((
        EventInput {
            title: value["summary"].as_str().filter(|s| !s.trim().is_empty()).unwrap_or("Untitled event").into(),
            notes,
            start,
            end,
            all_day,
            tz: zone.to_string(),
            rrule: None,
            color: None,
            reminder_minutes: None,
        },
        details,
    ))
}
fn import_calendar(db: &Db) -> Result<i64, String> {
    if let Some(id) = db.setting("google_import_calendar").map_err(failure)?.and_then(|v| v.parse::<i64>().ok()) {
        if db.calendars().map_err(failure)?.iter().any(|c| c.id == id) {
            return Ok(id);
        }
    }
    let id = db.create_calendar("Google Calendar", "#1badf8").map_err(failure)?;
    db.set_setting("google_import_calendar", &id.to_string()).map_err(failure)?;
    Ok(id)
}
fn preserve(db: &Db, wire: &Wire) -> Result<(), String> {
    let (mut event, details) = decode(&wire.body)?;
    event.title.push_str(" (local conflict copy)");
    let calendar = if let Some(c) = db.calendars().map_err(failure)?.into_iter().find(|c| c.name == "Sync conflicts") {
        c.id
    } else {
        db.create_calendar("Sync conflicts", "#a72e49").map_err(failure)?
    };
    db.create_event_with_details(&event, calendar, &details).map_err(failure)?;
    Ok(())
}
fn apply_remote(db: &Db, value: &Value, now: DateTime<Utc>, stats: &mut Stats) -> Result<(), String> {
    let remote = value["id"].as_str().ok_or("Google event is missing its ID")?;
    let etag = value["etag"].as_str().unwrap_or("");
    let existing = links(db)?.into_iter().find(|l| l.remote == remote);
    let identity = existing.as_ref().map(|l| l.key.clone());
    let target = identity.as_deref().map(|key| local_id(db, key)).transpose()?.flatten();
    let current = projection(db, now)?.into_iter().find(|w| identity.as_ref() == Some(&w.key));
    if existing.as_ref().is_some_and(|l| l.etag == etag) {
        return Ok(());
    }
    if let Some((id, _)) = &target {
        if db.event(*id).map_err(failure)?.is_some_and(|e| e.booking_id.is_some()) {
            let identity = identity.ok_or("Missing booking sync identity")?;
            let new_remote = if value["status"] == "cancelled" {
                format!("c{}", random_secret().map_err(failure)?)
            } else {
                remote.into()
            };
            save_link(
                db,
                &Link {
                    key: identity,
                    remote: new_remote,
                    etag: if value["status"] == "cancelled" { String::new() } else { etag.into() },
                    fingerprint: String::new(),
                    deleted: false,
                },
            )?;
            return Ok(());
        }
    }
    if let (Some(link), Some(wire)) = (&existing, &current) {
        let fingerprint = wire.body.to_string();
        if !link.fingerprint.is_empty() && link.fingerprint != fingerprint {
            preserve(db, wire)?;
            stats.conflicts += 1;
        }
    }
    if value["status"] == "cancelled" {
        if let Some((id, original)) = target {
            if let Some(original) = original {
                db.set_exception(&Exception {
                    event_id: id,
                    original_start: original,
                    cancelled: true,
                    title: None,
                    notes: None,
                    start: None,
                    end: None,
                })
                .map_err(failure)?;
            } else {
                db.delete_event(id).map_err(failure)?;
            }
        }
        if let Some(identity) = identity {
            remove_link(db, &identity)?;
        }
        return Ok(());
    }
    if existing.as_ref().is_some_and(|l| l.deleted)
        && target.is_none()
        && existing.as_ref().is_some_and(|l| l.etag == etag)
    {
        return Ok(());
    }
    let (input, details) = decode(value)?;
    let id = if let Some((id, original)) = target {
        if let Some(original) = original {
            db.set_exception(&Exception {
                event_id: id,
                original_start: original,
                cancelled: true,
                title: None,
                notes: None,
                start: None,
                end: None,
            })
            .map_err(failure)?;
            if let Some(identity) = &identity {
                remove_link(db, identity)?;
            }
            db.create_event_with_details(&input, db.event_calendar(id).map_err(failure)?, &details).map_err(failure)?
        } else {
            db.update_event_with_details(id, &input, None, Some(&details)).map_err(failure)?;
            id
        }
    } else {
        db.create_event_with_details(&input, import_calendar(db)?, &details).map_err(failure)?
    };
    let identity = key(db, id, None)?;
    let fingerprint =
        projection(db, now)?.into_iter().find(|w| w.key == identity).map(|w| w.body.to_string()).unwrap_or_default();
    save_link(db, &Link { key: identity, remote: remote.into(), etag: etag.into(), fingerprint, deleted: false })?;
    stats.pulled += 1;
    Ok(())
}
pub async fn sync<A: Api>(state: &Arc<State>, api: &A, now: DateTime<Utc>) -> Result<Stats, String> {
    let mut stats = Stats::default();
    let token = state
        .db
        .lock()
        .map_err(|_| "Calendar lock failed")?
        .setting("google_sync_token")
        .map_err(failure)?
        .filter(|s| !s.is_empty());
    let mut original = token;
    let mut page: Option<String> = None;
    let mut reset = false;
    let mut seen = HashSet::new();
    let mut pages = 0;
    let next_cursor = loop {
        pages += 1;
        if pages > 200 {
            return Err("Google sync exceeded its page limit".into());
        }
        let mut query = vec![
            ("singleEvents".into(), "true".into()),
            ("showDeleted".into(), "true".into()),
            ("maxResults".into(), "250".into()),
        ];
        if let Some(token) = &original {
            query.push(("syncToken".into(), token.clone()));
        } else {
            query.push(("timeMin".into(), stamp(now - Duration::days(90))));
            query.push(("timeMax".into(), stamp(now + Duration::days(365))));
        }
        if let Some(page) = &page {
            query.push(("pageToken".into(), page.clone()));
        }
        let response = api.call(Request { method: "GET", event: None, query, body: None, etag: None }).await?;
        if response.status == 410 && !reset {
            original = None;
            page = None;
            reset = true;
            continue;
        }
        if response.status != 200 {
            return Err(format!("Google list failed ({})", response.status));
        }
        let items = response.body["items"].as_array().ok_or("Google list has no event array")?;
        for value in items {
            if let Some(id) = value["id"].as_str() {
                seen.insert(id.to_string());
            }
            apply_remote(&*state.db.lock().map_err(|_| "Calendar lock failed")?, value, now, &mut stats)?;
        }
        if let Some(next) = response.body["nextPageToken"].as_str() {
            page = Some(next.to_string());
            continue;
        }
        let next = response.body["nextSyncToken"].as_str().ok_or("Google list has no final sync token")?;
        break next.to_string();
    };
    // A full refresh verifies missing cached resources instead of deleting local data blindly.
    if original.is_none() {
        let cached = links(&*state.db.lock().map_err(|_| "Calendar lock failed")?)?;
        for link in cached {
            if seen.contains(&link.remote) || link.etag.is_empty() {
                continue;
            }
            let reply = api
                .call(Request {
                    method: "GET",
                    event: Some(link.remote.clone()),
                    query: vec![],
                    body: None,
                    etag: None,
                })
                .await?;
            if matches!(reply.status, 404 | 410) {
                apply_remote(
                    &*state.db.lock().map_err(|_| "Calendar lock failed")?,
                    &json!({"id":link.remote,"status":"cancelled","etag":"deleted"}),
                    now,
                    &mut stats,
                )?;
            } else if reply.status == 200 {
                apply_remote(&*state.db.lock().map_err(|_| "Calendar lock failed")?, &reply.body, now, &mut stats)?;
            } else {
                return Err(format!("Google reconciliation failed ({})", reply.status));
            }
        }
    }
    let wires = projection(&*state.db.lock().map_err(|_| "Calendar lock failed")?, now)?;
    let mut cached: HashMap<_, _> = links(&*state.db.lock().map_err(|_| "Calendar lock failed")?)?
        .into_iter()
        .map(|l| (l.key.clone(), l))
        .collect();
    let active: HashSet<_> = wires.iter().map(|w| w.key.clone()).collect();
    for wire in wires {
        let link = if let Some(link) = cached.remove(&wire.key) {
            link
        } else {
            let link = Link {
                key: wire.key.clone(),
                remote: format!("c{}", random_secret().map_err(failure)?),
                etag: String::new(),
                fingerprint: String::new(),
                deleted: false,
            };
            save_link(&*state.db.lock().map_err(|_| "Calendar lock failed")?, &link)?;
            link
        };
        let fingerprint = wire.body.to_string();
        if link.fingerprint == fingerprint && !link.deleted {
            continue;
        }
        let inserting = link.etag.is_empty();
        let mut body = wire.body.clone();
        if inserting {
            body["id"] = json!(link.remote);
        }
        let mut reply = api
            .call(Request {
                method: if inserting { "POST" } else { "PATCH" },
                event: if inserting { None } else { Some(link.remote.clone()) },
                query: vec![("sendUpdates".into(), "none".into())],
                body: Some(body),
                etag: if inserting { None } else { Some(link.etag.clone()) },
            })
            .await?;
        if inserting && reply.status == 409 {
            let existing = api
                .call(Request {
                    method: "GET",
                    event: Some(link.remote.clone()),
                    query: vec![],
                    body: None,
                    etag: None,
                })
                .await?;
            if existing.status == 200 && existing.body["extendedProperties"]["private"]["calendioKey"] == wire.key {
                reply = existing;
            } else {
                return Err("Google create identity conflict".into());
            }
        }
        if !matches!(reply.status, 200 | 201) {
            return Err(format!("Google write failed ({}); local changes retained", reply.status));
        }
        let etag = reply.body["etag"].as_str().ok_or("Google write has no ETag")?;
        save_link(
            &*state.db.lock().map_err(|_| "Calendar lock failed")?,
            &Link { etag: etag.into(), fingerprint: wire.body.to_string(), deleted: false, ..link },
        )?;
        stats.pushed += 1;
    }
    for link in cached.into_values() {
        if active.contains(&link.key) {
            continue;
        }
        let target = {
            let db = state.db.lock().map_err(|_| "Calendar lock failed")?;
            local_id(&db, &link.key)?
        };
        if let Some((_, original)) = target {
            if !link.deleted {
                if original.is_none() {
                    continue;
                }
                if let Ok(previous) = serde_json::from_str::<Value>(&link.fingerprint) {
                    let (start, _, _) = google_date(&previous["start"], chrono_tz::UTC)?;
                    let start = if let Ok(t) = start.parse::<DateTime<Utc>>() {
                        t
                    } else {
                        chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d")
                            .map_err(|_| "Invalid saved sync date")?
                            .and_hms_opt(0, 0, 0)
                            .unwrap()
                            .and_utc()
                    };
                    if start < now - Duration::days(90) || start > now + Duration::days(365) {
                        continue;
                    }
                }
            }
        }
        if !link.etag.is_empty() {
            let response = api
                .call(Request {
                    method: "DELETE",
                    event: Some(link.remote.clone()),
                    query: vec![("sendUpdates".into(), "none".into())],
                    body: None,
                    etag: Some(link.etag.clone()),
                })
                .await?;
            if !matches!(response.status, 200 | 204 | 404 | 410) {
                return Err(format!("Google delete failed ({}); deletion retained for retry", response.status));
            }
        }
        remove_link(&*state.db.lock().map_err(|_| "Calendar lock failed")?, &link.key)?;
    }
    state
        .db
        .lock()
        .map_err(|_| "Calendar lock failed")?
        .set_setting("google_sync_token", &next_cursor)
        .map_err(failure)?;
    Ok(stats)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    struct Fake {
        replies: Mutex<std::collections::VecDeque<Reply>>,
        calls: Mutex<Vec<Request>>,
    }
    impl Api for Fake {
        async fn call(&self, r: Request) -> Result<Reply, String> {
            self.calls.lock().unwrap().push(r);
            self.replies.lock().unwrap().pop_front().ok_or("Unexpected provider request".into())
        }
    }
    fn fake(values: Vec<(u16, Value)>) -> Fake {
        Fake {
            replies: Mutex::new(values.into_iter().map(|(status, body)| Reply { status, body }).collect()),
            calls: Mutex::new(vec![]),
        }
    }
    fn now() -> DateTime<Utc> {
        "2026-10-15T00:00:00Z".parse().unwrap()
    }
    fn state() -> Arc<State> {
        Arc::new(State { db: std::sync::Mutex::new(Db::open_in_memory().unwrap()), tz: chrono_tz::UTC, now })
    }
    fn remote() -> Value {
        json!({"id":"remote1","etag":"v1","summary":"Google meeting","start":{"dateTime":"2026-10-19T09:00:00Z","timeZone":"UTC"},"end":{"dateTime":"2026-10-19T10:00:00Z","timeZone":"UTC"}})
    }
    #[tokio::test]
    async fn paginated_pull_imports_busy_events_and_saves_only_the_final_cursor() {
        let s = state();
        let api = fake(vec![
            (200, json!({"items":[remote()],"nextPageToken":"page2"})),
            (200, json!({"items":[],"nextSyncToken":"cursor"})),
        ]);
        let stats = sync(&s, &api, now()).await.unwrap();
        assert_eq!(stats.pulled, 1);
        let db = s.db.lock().unwrap();
        assert_eq!(db.setting("google_sync_token").unwrap().as_deref(), Some("cursor"));
        assert_eq!(
            db.busy_between("2026-10-19T00:00:00Z".parse().unwrap(), "2026-10-20T00:00:00Z".parse().unwrap())
                .unwrap()
                .len(),
            1
        );
        assert!(api.calls.lock().unwrap()[1].query.iter().any(|(k, v)| k == "pageToken" && v == "page2"));
    }
    #[tokio::test]
    async fn expired_cursor_restarts_and_local_updates_use_if_match() {
        let s = state();
        s.db.lock().unwrap().set_setting("google_sync_token", "old").unwrap();
        let api = fake(vec![(410, json!({})), (200, json!({"items":[remote()],"nextSyncToken":"fresh"}))]);
        sync(&s, &api, now()).await.unwrap();
        {
            let db = s.db.lock().unwrap();
            let mut event = db.all_events().unwrap().pop().unwrap();
            event.input.title = "Edited locally".into();
            db.update_event(event.id, &event.input).unwrap();
        }
        let api =
            fake(vec![(200, json!({"items":[],"nextSyncToken":"next"})), (200, json!({"id":"remote1","etag":"v2"}))]);
        sync(&s, &api, now()).await.unwrap();
        let calls = api.calls.lock().unwrap();
        assert_eq!(calls[1].method, "PATCH");
        assert_eq!(calls[1].etag.as_deref(), Some("v1"));
    }
    #[tokio::test]
    async fn failed_page_does_not_advance_the_cursor() {
        let s = state();
        s.db.lock().unwrap().set_setting("google_sync_token", "old").unwrap();
        let api = fake(vec![(200, json!({"items":[],"nextPageToken":"next"})), (503, json!({}))]);
        assert!(sync(&s, &api, now()).await.is_err());
        assert_eq!(s.db.lock().unwrap().setting("google_sync_token").unwrap().as_deref(), Some("old"));
    }
}
