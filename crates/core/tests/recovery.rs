use calendio_core::{
    db::{Db, EventInput},
    details::EventDetails,
};
fn event() -> EventInput {
    EventInput {
        title: "Planning".into(),
        notes: String::new(),
        start: "2026-10-19T09:00:00Z".into(),
        end: "2026-10-19T10:00:00Z".into(),
        all_day: false,
        tz: "UTC".into(),
        rrule: Some("FREQ=DAILY;COUNT=3".into()),
        color: None,
        reminder_minutes: None,
    }
}
#[test]
fn create_update_and_delete_undo_restore_complete_event_state() {
    let db = Db::open_in_memory().unwrap();
    let details = EventDetails { location: "Studio".into(), ..Default::default() };
    let id = db.record_edit("Create", None, |d| d.create_event_with_details(&event(), 1, &details)).unwrap();
    let original = db.snapshot(id).unwrap();
    let mut changed = event();
    changed.title = "Changed".into();
    db.record_edit("Update", Some(id), |d| {
        d.update_event(id, &changed)?;
        Ok(id)
    })
    .unwrap();
    db.undo().unwrap();
    assert_eq!(db.snapshot(id).unwrap(), original);
    db.record_edit("Delete", Some(id), |d| {
        d.delete_event(id)?;
        Ok(id)
    })
    .unwrap();
    db.undo().unwrap();
    assert_eq!(db.snapshot(id).unwrap(), original);
    db.undo().unwrap();
    assert!(db.event(id).unwrap().is_none());
}
#[test]
fn undo_does_not_overwrite_a_later_external_change() {
    let db = Db::open_in_memory().unwrap();
    let id = db.record_edit("Create", None, |d| d.create_event(&event())).unwrap();
    let mut external = event();
    external.title = "Remote change".into();
    db.update_event(id, &external).unwrap();
    assert!(db.undo().is_err());
    assert_eq!(db.event(id).unwrap().unwrap().title, "Remote change");
}
#[test]
fn backup_is_consistent_refuses_overwrite_and_restore_makes_safety_copy() {
    let folder = std::env::temp_dir().join(format!("calendio-recovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let mut db = Db::open(&folder.join("live.db")).unwrap();
    let id = db.create_event(&event()).unwrap();
    let backup = folder.join("backup.db");
    db.backup_to(&backup).unwrap();
    assert!(db.backup_to(&backup).is_err());
    db.delete_event(id).unwrap();
    let safety = folder.join("safety.db");
    db.restore_backup(&backup, &safety).unwrap();
    assert!(db.event(id).unwrap().is_some());
    let old = Db::open(&safety).unwrap();
    assert!(old.event(id).unwrap().is_none());
    drop(old);
    drop(db);
    std::fs::remove_dir_all(folder).unwrap();
}
