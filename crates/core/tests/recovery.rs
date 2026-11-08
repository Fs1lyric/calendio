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
fn scratch(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("calendio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}
#[test]
fn prune_keeps_the_newest_daily_backups_and_leaves_other_files_alone() {
    let folder = scratch("prune");
    let start = chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    for day in 0..35 {
        let date = start + chrono::Duration::days(day);
        std::fs::write(folder.join(format!("calendio-{}.sqlite3", date.format("%Y-%m-%d"))), b"x").unwrap();
    }
    for other in ["pre-claude-1.db", "calendio-2026-01-01-101500.sqlite3", "before-restore-2026-01-01-101500.sqlite3"] {
        std::fs::write(folder.join(other), b"x").unwrap();
    }
    assert_eq!(Db::prune_backups(&folder, 30).unwrap(), 5);
    let daily: Vec<_> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.len() == "calendio-2026-09-01.sqlite3".len() && n.starts_with("calendio-"))
        .collect();
    assert_eq!(daily.len(), 30);
    assert!(!folder.join("calendio-2026-09-01.sqlite3").exists());
    assert!(!folder.join("calendio-2026-09-05.sqlite3").exists());
    assert!(folder.join("calendio-2026-09-06.sqlite3").exists());
    assert!(folder.join("calendio-2026-10-05.sqlite3").exists());
    for other in ["pre-claude-1.db", "calendio-2026-01-01-101500.sqlite3", "before-restore-2026-01-01-101500.sqlite3"] {
        assert!(folder.join(other).exists(), "{other} was pruned");
    }
    std::fs::remove_dir_all(folder).unwrap();
}
#[test]
fn mirror_copies_missing_daily_backups_and_prunes_the_mirror() {
    let source = scratch("mirror-src");
    let mirror = scratch("mirror-dst").join("usb");
    let db = Db::open(&source.join("live.db")).unwrap();
    let now = "2026-11-08T09:00:00Z".parse().unwrap();
    let path = db.daily_backup(&source.join("backups"), now).unwrap();
    assert_eq!(path, source.join("backups/calendio-2026-11-08.sqlite3"));
    std::fs::write(source.join("backups/calendio-2026-11-07.sqlite3"), b"older").unwrap();
    std::fs::write(source.join("backups/before-restore-2026-11-07-101500.sqlite3"), b"skip").unwrap();
    assert_eq!(Db::mirror_backups(&source.join("backups"), &mirror, 30).unwrap(), 2);
    assert_eq!(std::fs::read(mirror.join("calendio-2026-11-07.sqlite3")).unwrap(), b"older");
    assert_eq!(std::fs::read(mirror.join("calendio-2026-11-08.sqlite3")).unwrap(), std::fs::read(&path).unwrap());
    assert!(!mirror.join("before-restore-2026-11-07-101500.sqlite3").exists());
    assert_eq!(Db::mirror_backups(&source.join("backups"), &mirror, 30).unwrap(), 0, "already mirrored");
    assert_eq!(Db::mirror_backups(&source.join("backups"), &mirror, 1).unwrap(), 0);
    assert!(!mirror.join("calendio-2026-11-07.sqlite3").exists(), "mirror keeps only the newest");
    drop(db);
    std::fs::remove_dir_all(source).unwrap();
    std::fs::remove_dir_all(mirror.parent().unwrap()).unwrap();
}
#[test]
fn backups_list_newest_first_with_friendly_labels() {
    use calendio_core::recovery::{backup_label, list_backups, BackupKind};
    let folder = scratch("list");
    for name in [
        "calendio-2026-11-03.sqlite3",
        "calendio-2026-11-07.sqlite3",
        "calendio-2026-11-08.sqlite3",
        "calendio-2026-11-08-143000.sqlite3",
        "before-restore-2026-11-08-160500.sqlite3",
        "calendio-2025-12-31.sqlite3",
        "notes.txt",
        ".calendio-abc.tmp",
    ] {
        std::fs::write(folder.join(name), b"x").unwrap();
    }
    let list = list_backups(&folder);
    let names: Vec<_> = list.iter().map(|b| b.path.file_name().unwrap().to_str().unwrap().to_owned()).collect();
    assert_eq!(
        names,
        [
            "before-restore-2026-11-08-160500.sqlite3",
            "calendio-2026-11-08-143000.sqlite3",
            "calendio-2026-11-08.sqlite3",
            "calendio-2026-11-07.sqlite3",
            "calendio-2026-11-03.sqlite3",
            "calendio-2025-12-31.sqlite3",
        ]
    );
    assert_eq!(list[0].kind, BackupKind::BeforeRestore);
    assert_eq!(list[1].kind, BackupKind::Manual);
    assert_eq!(list[2].kind, BackupKind::Daily);
    // Dhaka is UTC+6: a 14:30 UTC save reads as 20:30 local.
    let now: chrono::DateTime<chrono::FixedOffset> = "2026-11-08T22:00:00+06:00".parse().unwrap();
    let hhmm = |t: chrono::NaiveTime| t.format("%H:%M").to_string();
    let labels: Vec<_> = list.iter().map(|b| backup_label(b, now, hhmm)).collect();
    assert_eq!(
        labels,
        [
            "Today, before a restore at 22:05",
            "Today, saved at 20:30",
            "Today",
            "Yesterday",
            "Tue 3 Nov",
            "Wed 31 Dec 2025",
        ]
    );
    std::fs::remove_dir_all(folder).unwrap();
}
