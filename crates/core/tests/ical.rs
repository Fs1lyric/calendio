use calendio_core::db::{Db, EventInput, Exception};

fn event() -> EventInput {
    EventInput {
        title: "Design, planning; café \\ team".into(),
        notes: "First line\nSecond line".into(),
        start: "2026-10-19T09:00:00Z".into(),
        end: "2026-10-19T10:00:00Z".into(),
        all_day: false,
        tz: "Asia/Dhaka".into(),
        rrule: None,
        color: Some("#2457b2".into()),
        reminder_minutes: Some(10),
    }
}
fn wrap(events: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n{events}END:VCALENDAR\r\n")
}
fn vevent(uid: &str, start: &str, end: &str) -> String {
    format!("BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:20261001T000000Z\r\nDTSTART{start}\r\nDTEND{end}\r\nSUMMARY:Planning\r\nEND:VEVENT\r\n")
}

#[test]
fn export_import_preserves_text_timezone_reminder_and_stable_identity() {
    let source = Db::open_in_memory().unwrap();
    let original = event();
    source.create_event(&original).unwrap();
    let ics = source.export_ics(1).unwrap();
    assert!(ics.contains("DTSTART;TZID=Asia/Dhaka:20261019T150000"));
    assert!(ics.contains("Design\\, planning\\; café \\\\ team"));
    let target = Db::open_in_memory().unwrap();
    let report = target.import_ics(1, &ics, "UTC").unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(target.all_events().unwrap()[0].input, original);
    let again = target.import_ics(1, &ics, "UTC").unwrap();
    assert_eq!(again.skipped, 1);
    assert_eq!(target.all_events().unwrap().len(), 1);
}

#[test]
fn all_day_exclusive_end_and_recurring_exceptions_roundtrip() {
    let source = Db::open_in_memory().unwrap();
    let mut input = event();
    input.all_day = true;
    input.start = "2026-10-19".into();
    input.end = "2026-10-20".into();
    input.rrule = Some("FREQ=DAILY;COUNT=3".into());
    let id = source.create_event(&input).unwrap();
    source
        .set_exception(&Exception {
            event_id: id,
            original_start: "2026-10-20".into(),
            cancelled: true,
            title: None,
            notes: None,
            start: None,
            end: None,
        })
        .unwrap();
    source
        .set_exception(&Exception {
            event_id: id,
            original_start: "2026-10-21".into(),
            cancelled: false,
            title: Some("Moved".into()),
            notes: None,
            start: Some("2026-10-22".into()),
            end: Some("2026-10-23".into()),
        })
        .unwrap();
    let data = source.export_ics(1).unwrap();
    assert!(data.contains("DTEND;VALUE=DATE:20261020"));
    let target = Db::open_in_memory().unwrap();
    target.import_ics(1, &data, "UTC").unwrap();
    let expected = source.occurrences_between("2026-10-18T00:00:00Z", "2026-10-24T00:00:00Z").unwrap();
    let actual = target.occurrences_between("2026-10-18T00:00:00Z", "2026-10-24T00:00:00Z").unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn import_unfolds_lines_and_uses_iana_zones_across_dst() {
    let db = Db::open_in_memory().unwrap();
    let data=wrap("BEGIN:VEVENT\r\nUID:dst\r\nDTSTART;TZID=America/New_York:20261031T090000\r\nDTEND;TZID=America/New_York:20261031T100000\r\nRRULE:FREQ=DAILY;COUNT=3\r\nSUMMARY:Very long \r\n folded title\r\nEND:VEVENT\r\n");
    db.import_ics(1, &data, "UTC").unwrap();
    let occ = db.occurrences_between("2026-10-31T00:00:00Z", "2026-11-03T00:00:00Z").unwrap();
    assert_eq!(occ[0].title, "Very long folded title");
    assert_eq!(occ[0].start, "2026-10-31T13:00:00Z");
    assert_eq!(occ[1].start, "2026-11-01T14:00:00Z");
}

#[test]
fn malformed_import_is_atomic_and_rejects_unsupported_recurrence() {
    let db = Db::open_in_memory().unwrap();
    let valid = vevent("good", ":20261019T090000Z", ":20261019T100000Z");
    let invalid = vevent("bad", ":20261019T100000Z", ":20261019T090000Z");
    assert!(db.import_ics(1, &wrap(&(valid.clone() + &invalid)), "UTC").is_err());
    assert!(db.all_events().unwrap().is_empty());
    let unsupported = valid.replace("END:VEVENT", "RDATE:20261020T090000Z\r\nEND:VEVENT");
    assert!(db.import_ics(1, &wrap(&unsupported), "UTC").is_err());
    assert!(db.all_events().unwrap().is_empty());
    assert!(db.import_ics(1, "BEGIN:VCALENDAR\nBEGIN:VEVENT\n", "UTC").is_err());
}

#[test]
fn exdates_remove_occurrences_and_missing_all_day_end_means_one_day() {
    let db = Db::open_in_memory().unwrap();
    let data=wrap("BEGIN:VEVENT\r\nUID:holiday\r\nDTSTART;VALUE=DATE:20261019\r\nRRULE:FREQ=DAILY;COUNT=3\r\nEXDATE;VALUE=DATE:20261020\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\n");
    db.import_ics(1, &data, "UTC").unwrap();
    let events = db.all_events().unwrap();
    assert_eq!(events[0].end, "2026-10-20");
    assert_eq!(db.occurrences_between("2026-10-19T00:00:00Z", "2026-10-23T00:00:00Z").unwrap().len(), 2);
}

#[test]
fn export_folds_utf8_at_75_octets_without_splitting_characters() {
    let db = Db::open_in_memory().unwrap();
    let mut input = event();
    input.title = "é".repeat(100);
    db.create_event(&input).unwrap();
    let data = db.export_ics(1).unwrap();
    assert!(data.split("\r\n").all(|line| line.len() <= 75));
    let target = Db::open_in_memory().unwrap();
    target.import_ics(1, &data, "UTC").unwrap();
    assert_eq!(target.all_events().unwrap()[0].title, input.title);
}

#[test]
fn export_only_selected_calendar_and_import_preserves_location() {
    let db = Db::open_in_memory().unwrap();
    db.create_event(&event()).unwrap();
    let other = db.create_calendar("Work", "#2457b2").unwrap();
    assert!(!db.export_ics(other).unwrap().contains("BEGIN:VEVENT"));
    let data = wrap(
        &vevent("loc", ":20261019T090000Z", ":20261019T100000Z").replace("END:VEVENT", "LOCATION:Studio\r\nEND:VEVENT"),
    );
    let report = db.import_ics(other, &data, "UTC").unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("LOCATION")));
    let imported = db.all_events().unwrap().into_iter().find(|e| db.event_calendar(e.id).unwrap() == other).unwrap();
    assert_eq!(db.event_details(imported.id).unwrap().location, "Studio");
}

#[test]
fn utc_import_does_not_acquire_the_floating_time_zone() {
    let db = Db::open_in_memory().unwrap();
    db.import_ics(1, &wrap(&vevent("utc", ":20261019T090000Z", ":20261019T100000Z")), "America/New_York").unwrap();
    assert_eq!(db.all_events().unwrap()[0].tz, "UTC");
}

#[test]
fn renamed_occurrence_stays_on_its_original_day_and_reimport_is_idempotent() {
    let db = Db::open_in_memory().unwrap();
    let mut input = event();
    input.rrule = Some("FREQ=DAILY;COUNT=3".into());
    let id = db.create_event(&input).unwrap();
    db.set_exception(&Exception {
        event_id: id,
        original_start: "2026-10-20T09:00:00Z".into(),
        cancelled: false,
        title: Some("Renamed".into()),
        notes: None,
        start: None,
        end: None,
    })
    .unwrap();
    let data = db.export_ics(1).unwrap();
    assert_eq!(db.import_ics(1, &data, "UTC").unwrap().skipped, 1);
    let target = Db::open_in_memory().unwrap();
    target.import_ics(1, &data, "UTC").unwrap();
    assert_eq!(
        db.occurrences_between("2026-10-19T00:00:00Z", "2026-10-23T00:00:00Z").unwrap(),
        target.occurrences_between("2026-10-19T00:00:00Z", "2026-10-23T00:00:00Z").unwrap()
    );
}

#[test]
fn external_display_alarm_and_timed_until_are_interoperable() {
    let db = Db::open_in_memory().unwrap();
    let data = wrap(&vevent("alarm", ":20261019T090000Z", ":20261019T100000Z").replace(
        "END:VEVENT",
        "BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nDESCRIPTION:Reminder\r\nEND:VALARM\r\nEND:VEVENT",
    ));
    db.import_ics(1, &data, "UTC").unwrap();
    assert_eq!(db.all_events().unwrap()[0].reminder_minutes, Some(15));
    let mut e = event();
    e.rrule = Some("FREQ=DAILY;UNTIL=20261022".into());
    let id = db.create_event(&e).unwrap();
    let export = db.export_ics(1).unwrap();
    assert!(export.contains("RRULE:FREQ=DAILY;UNTIL=20261022T175959Z"));
    let target = Db::open_in_memory().unwrap();
    target.import_ics(1, &export, "UTC").unwrap();
    let from = "2026-10-19T00:00:00Z";
    let to = "2026-10-24T00:00:00Z";
    assert_eq!(
        db.occurrences_between(from, to).unwrap().iter().filter(|o| o.event_id == id).count(),
        target.occurrences_between(from, to).unwrap().iter().filter(|o| o.title == e.title).count()
    );
}

#[test]
fn event_locations_links_and_availability_survive_interchange() {
    use calendio_core::details::EventDetails;
    let db = Db::open_in_memory().unwrap();
    let details = EventDetails {
        location: "Studio, north wing".into(),
        meeting_url: "https://meet.example.com/planning".into(),
        busy: false,
    };
    db.create_event_with_details(&event(), 1, &details).unwrap();
    let data = db.export_ics(1).unwrap();
    assert!(data.contains("LOCATION:Studio\\, north wing"));
    assert!(data.contains("TRANSP:TRANSPARENT"));
    let imported = Db::open_in_memory().unwrap();
    imported.import_ics(1, &data, "UTC").unwrap();
    assert_eq!(imported.event_details(imported.all_events().unwrap()[0].id).unwrap(), details);
}
