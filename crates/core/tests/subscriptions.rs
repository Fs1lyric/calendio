use calendio_core::db::{Db, EventInput};

fn feed(events: &[(&str, &str, &str)]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n");
    for (uid, date, title) in events {
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20261001T000000Z\r\nDTSTART;VALUE=DATE:{date}\r\nSUMMARY:{title}\r\nTRANSP:TRANSPARENT\r\nEND:VEVENT\r\n"
        ));
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}
fn titles(db: &Db, calendar: i64) -> Vec<String> {
    let mut titles: Vec<_> = db
        .all_events()
        .unwrap()
        .into_iter()
        .filter(|e| db.event_calendar(e.id).unwrap() == calendar)
        .map(|e| e.input.title)
        .collect();
    titles.sort();
    titles
}
fn mine() -> EventInput {
    EventInput {
        title: "Mine".into(),
        notes: String::new(),
        start: "2026-12-25T09:00:00Z".into(),
        end: "2026-12-25T10:00:00Z".into(),
        all_day: false,
        tz: "UTC".into(),
        rrule: None,
        color: None,
        reminder_minutes: None,
    }
}

#[test]
fn subscriptions_add_with_webcal_urls_and_reject_other_schemes() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_subscription("Holidays", "#ff9500", "webcal://example.com/holidays.ics").unwrap();
    let subs = db.subscriptions().unwrap();
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].calendar_id, id);
    assert_eq!(subs[0].url, "https://example.com/holidays.ics");
    assert_eq!(subs[0].last_fetched, None);
    assert!(db.is_subscribed(id).unwrap());
    assert!(!db.is_subscribed(1).unwrap());
    assert!(db.calendars().unwrap().iter().any(|c| c.id == id && c.name == "Holidays"));
    for bad in ["file:///etc/passwd", "ftp://x/y.ics", "not a url", ""] {
        assert!(db.create_subscription("Bad", "#ff9500", bad).is_err(), "{bad}");
    }
}

#[test]
fn replacing_a_feed_updates_and_removes_events_without_duplicates() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_subscription("Holidays", "#ff9500", "https://example.com/h.ics").unwrap();
    db.create_event(&mine()).unwrap();
    let now = "2026-11-08T09:00:00Z".parse().unwrap();
    db.replace_subscription(
        id,
        &feed(&[("a", "20261225", "Christmas"), ("b", "20261231", "NYE")]),
        Some("\"v1\""),
        now,
        "UTC",
    )
    .unwrap();
    assert_eq!(titles(&db, id), ["Christmas", "NYE"]);
    db.replace_subscription(
        id,
        &feed(&[("a", "20261225", "Christmas Day"), ("c", "20270101", "New Year")]),
        None,
        now,
        "UTC",
    )
    .unwrap();
    assert_eq!(titles(&db, id), ["Christmas Day", "New Year"]);
    db.replace_subscription(
        id,
        &feed(&[("a", "20261225", "Christmas Day"), ("c", "20270101", "New Year")]),
        None,
        now,
        "UTC",
    )
    .unwrap();
    assert_eq!(titles(&db, id), ["Christmas Day", "New Year"], "same feed twice never duplicates");
    assert_eq!(titles(&db, 1), ["Mine"], "other calendars are untouched");
    let sub = &db.subscriptions().unwrap()[0];
    assert_eq!(sub.last_fetched.as_deref(), Some("2026-11-08T09:00:00Z"));
    assert_eq!(sub.etag, None);
}

#[test]
fn a_broken_feed_keeps_the_previous_events() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_subscription("Holidays", "#ff9500", "https://example.com/h.ics").unwrap();
    let now = "2026-11-08T09:00:00Z".parse().unwrap();
    db.replace_subscription(id, &feed(&[("a", "20261225", "Christmas")]), Some("\"v1\""), now, "UTC").unwrap();
    assert!(db.replace_subscription(id, "<html>not a calendar</html>", None, now, "UTC").is_err());
    assert_eq!(titles(&db, id), ["Christmas"]);
    assert_eq!(db.subscriptions().unwrap()[0].etag.as_deref(), Some("\"v1\""));
}

#[test]
fn subscribed_events_are_read_only_and_unsubscribing_removes_them() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_subscription("Holidays", "#ff9500", "https://example.com/h.ics").unwrap();
    let now = "2026-11-08T09:00:00Z".parse().unwrap();
    db.replace_subscription(id, &feed(&[("a", "20261225", "Christmas")]), None, now, "UTC").unwrap();
    let event = db.all_events().unwrap()[0].id;
    let renamed = EventInput { title: "Mine now".into(), ..db.event(event).unwrap().unwrap().input };
    assert!(db
        .record_edit("Update", Some(event), |d| {
            d.update_event(event, &renamed)?;
            Ok(event)
        })
        .is_err());
    assert!(db.record_edit("Create", None, |d| d.create_event_with_details(&mine(), id, &Default::default())).is_err());
    assert_eq!(titles(&db, id), ["Christmas"]);
    db.delete_calendar(id).unwrap();
    assert!(db.all_events().unwrap().is_empty(), "feed events are not moved into Personal");
    assert!(db.subscriptions().unwrap().is_empty());
}

#[test]
fn version_before_subscriptions_migrates_with_no_subscriptions() {
    let db = Db::open_in_memory().unwrap();
    assert!(db.subscriptions().unwrap().is_empty());
    assert_eq!(db.schema_version().unwrap(), Db::SCHEMA_VERSION);
}
