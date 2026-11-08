//! The daily backup step: save, prune to 30, and copy to the optional mirror folder.
use calendio_core::db::Db;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
/// Daily backups kept on disk and in the mirror.
pub const KEEP: usize = 30;
pub fn directory() -> PathBuf {
    std::env::var_os("CALENDIO_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Db::default_path().parent().unwrap_or(Path::new(".")).join("backups"))
}
/// Runs the whole step and returns the status line for Settings.
pub fn daily(db: &Db, directory: &Path, now: DateTime<Utc>) -> Result<String, String> {
    db.daily_backup(directory, now).map_err(|e| e.to_string())?;
    Db::prune_backups(directory, KEEP).map_err(|e| e.to_string())?;
    let mirror = db.setting("backup_mirror").ok().flatten().unwrap_or_default();
    if mirror.trim().is_empty() {
        return Ok("Daily backup saved".into());
    }
    match Db::mirror_backups(directory, Path::new(mirror.trim()), KEEP) {
        Ok(_) => Ok(format!("Daily backup saved and copied to {}", mirror.trim())),
        Err(error) => Err(format!("Daily backup saved, but not copied to {}: {error}", mirror.trim())),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("calendio-booker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }
    #[test]
    fn daily_step_prunes_and_mirrors_when_a_mirror_is_set() {
        let home = scratch("step");
        let backups = home.join("backups");
        let mirror = home.join("mirror");
        let db = Db::open(&home.join("live.db")).unwrap();
        std::fs::create_dir_all(&backups).unwrap();
        let start = chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        for day in 0..32 {
            let name = format!("calendio-{}.sqlite3", (start + chrono::Duration::days(day)).format("%Y-%m-%d"));
            std::fs::write(backups.join(name), b"x").unwrap();
        }
        let now = "2026-11-08T09:00:00Z".parse().unwrap();
        assert_eq!(daily(&db, &backups, now).unwrap(), "Daily backup saved");
        assert_eq!(calendio_core::recovery::list_backups(&backups).len(), KEEP);
        assert!(backups.join("calendio-2026-11-08.sqlite3").exists());
        assert!(!mirror.exists());
        db.set_setting("backup_mirror", mirror.to_str().unwrap()).unwrap();
        assert!(daily(&db, &backups, now).unwrap().starts_with("Daily backup saved and copied"));
        assert!(mirror.join("calendio-2026-11-08.sqlite3").exists());
        assert_eq!(calendio_core::recovery::list_backups(&mirror).len(), KEEP);
        drop(db);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn an_unreachable_mirror_reports_but_keeps_the_local_backup() {
        let home = scratch("bad-mirror");
        let db = Db::open(&home.join("live.db")).unwrap();
        std::fs::write(home.join("file"), b"not a folder").unwrap();
        db.set_setting("backup_mirror", home.join("file/inside").to_str().unwrap()).unwrap();
        let now = "2026-11-08T09:00:00Z".parse().unwrap();
        let error = daily(&db, &home.join("backups"), now).unwrap_err();
        assert!(error.contains("not copied"), "{error}");
        assert!(home.join("backups/calendio-2026-11-08.sqlite3").exists());
        drop(db);
        std::fs::remove_dir_all(home).unwrap();
    }
}
