use calendio_core::db::{Db, EventInput};

fn event(title: &str) -> EventInput {
    EventInput {
        title: title.into(),
        notes: "Budget review".into(),
        start: "2026-10-19T09:00:00Z".into(),
        end: "2026-10-19T10:00:00Z".into(),
        all_day: false,
        tz: "UTC".into(),
        rrule: None,
        color: None,
        reminder_minutes: None,
    }
}

#[test]
fn existing_events_have_a_personal_calendar_and_new_assignments_persist() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_event(&event("Planning")).unwrap();
    let calendars = db.calendars().unwrap();
    assert_eq!(calendars[0].name, "Personal");
    assert_eq!(db.event_calendar(id).unwrap(), calendars[0].id);
    let work = db.create_calendar("Work", "#2457b2").unwrap();
    db.assign_calendar(id, work).unwrap();
    assert_eq!(db.event_calendar(id).unwrap(), work);
    assert!(db.assign_calendar(id, 9999).is_err());
    assert!(db.assign_calendar(9999, work).is_err());
}

#[test]
fn hiding_a_calendar_does_not_make_busy_time_bookable() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_event(&event("Planning")).unwrap();
    let work = db.create_calendar("Work", "#2457b2").unwrap();
    db.assign_calendar(id, work).unwrap();
    let (from, to) = ("2026-10-19T00:00:00Z", "2026-10-20T00:00:00Z");
    assert_eq!(db.visible_occurrences(from, to).unwrap().len(), 1);
    db.update_calendar(work, "Work", "#2457b2", false).unwrap();
    assert!(db.visible_occurrences(from, to).unwrap().is_empty());
    assert_eq!(db.busy_between(from.parse().unwrap(), to.parse().unwrap()).unwrap().len(), 1);
}

#[test]
fn deleting_calendar_moves_events_to_personal_without_losing_them() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_event(&event("Planning")).unwrap();
    let work = db.create_calendar("Work", "#2457b2").unwrap();
    db.assign_calendar(id, work).unwrap();
    db.delete_calendar(work).unwrap();
    assert_eq!(db.event_calendar(id).unwrap(), 1);
    assert!(db.event(id).unwrap().is_some());
    assert!(db.delete_calendar(1).is_err());
}

#[test]
fn search_is_unicode_case_insensitive_literal_and_checks_notes() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_event(&event("ÉQUIPE 100%_project")).unwrap();
    db.create_event(&event("Other")).unwrap();
    assert_eq!(db.search_events("équipe").unwrap()[0].id, id);
    assert_eq!(db.search_events("100%_").unwrap().len(), 1);
    assert_eq!(db.search_events("BUDGET").unwrap().len(), 2);
    assert!(db.search_events("' OR 1=1 --").unwrap().is_empty());
    assert!(db.search_events("  ").unwrap().is_empty());
}

#[test]
fn calendar_metadata_is_validated() {
    let db = Db::open_in_memory().unwrap();
    assert!(db.create_calendar("  ", "#2457b2").is_err());
    assert!(db.create_calendar("Work", "not-a-color").is_err());
    assert!(db.update_calendar(123, "Work", "#2457b2", true).is_err());
}

#[test]
fn invalid_calendar_change_does_not_partially_save_event_changes() {
    let db = Db::open_in_memory().unwrap();
    let input = event("Original");
    let id = db.create_event(&input).unwrap();
    let mut changed = input.clone();
    changed.title = "Updated".into();
    assert!(db.update_event_in_calendar(id, &changed, Some(999)).is_err());
    assert_eq!(db.event(id).unwrap().unwrap().input, input);
    assert!(db.create_event_in_calendar(&changed, 999).is_err());
    assert_eq!(db.all_events().unwrap().len(), 1);
}

#[test]
fn removing_a_calendar_preserves_import_identity() {
    let db = Db::open_in_memory().unwrap();
    let work = db.create_calendar("Work", "#2457b2").unwrap();
    let id = db.create_event_in_calendar(&event("Planning"), work).unwrap();
    let export = db.export_ics(work).unwrap();
    db.delete_calendar(work).unwrap();
    assert_eq!(db.event_calendar(id).unwrap(), 1);
    assert_eq!(db.import_ics(1, &export, "UTC").unwrap().skipped, 1);
    assert_eq!(db.all_events().unwrap().len(), 1);
}

#[test]
fn version_one_database_is_migrated_without_changing_existing_events() {
    let dir = std::env::temp_dir().join(format!("calendio-v1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("migration.db");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(include_str!("fixtures/schema_v1.sql")).unwrap();
    old.pragma_update(None, "user_version", 1).unwrap();
    old.execute("INSERT INTO events (id,title,start,end,tz,created_at,updated_at) VALUES (42,'Existing event','2026-10-19T09:00:00Z','2026-10-19T10:00:00Z','UTC','2026-10-01T00:00:00Z','2026-10-01T00:00:00Z')",[]).unwrap();
    drop(old);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), Db::SCHEMA_VERSION);
    assert_eq!(db.event(42).unwrap().unwrap().title, "Existing event");
    assert_eq!(db.event_calendar(42).unwrap(), 1);
    let data = db.export_ics(1).unwrap();
    assert_eq!(db.import_ics(1, &data, "UTC").unwrap().skipped, 1);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
