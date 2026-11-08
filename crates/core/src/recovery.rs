//! Conflict-checked undo and consistent SQLite recovery.
use crate::{
    db::{Db, Error, Event, Exception, Result},
    delivery::{random_secret, time},
    details::EventDetails,
};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    ops::Deref,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static SAVEPOINT: AtomicU64 = AtomicU64::new(1);
pub(crate) struct SqlSavepoint<'a> {
    conn: &'a rusqlite::Connection,
    name: String,
    done: bool,
}
impl Deref for SqlSavepoint<'_> {
    type Target = rusqlite::Connection;
    fn deref(&self) -> &Self::Target {
        self.conn
    }
}
impl SqlSavepoint<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.conn.execute_batch(&format!("RELEASE {}", self.name))?;
        self.done = true;
        Ok(())
    }
}
impl Drop for SqlSavepoint<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.conn.execute_batch(&format!("ROLLBACK TO {0}; RELEASE {0}", self.name));
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSnapshot {
    pub event: Event,
    pub calendar_id: i64,
    pub details: EventDetails,
    pub exceptions: Vec<Exception>,
    pub uid: String,
}
fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}
fn private_file(path: &Path) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| invalid("Choose a new backup filename in an existing writable directory"))?;
    Ok(())
}
fn copy_database(conn: &rusqlite::Connection, destination: &Path) -> Result<()> {
    let parent = destination.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let temporary = Temp(parent.join(format!(".calendio-{}.tmp", random_secret()?)));
    private_file(&temporary.0)?;
    conn.backup(rusqlite::DatabaseName::Main, &temporary.0, None)?;
    let check = rusqlite::Connection::open_with_flags(&temporary.0, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if check.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(invalid("Backup integrity check failed"));
    }
    drop(check);
    std::fs::File::open(&temporary.0).and_then(|f| f.sync_all()).map_err(|_| invalid("Could not flush the backup"))?;
    std::fs::hard_link(&temporary.0, destination)
        .map_err(|_| invalid("Backup destination exists or cannot be created"))?;
    Ok(())
}
impl Db {
    pub(crate) fn savepoint(&self) -> Result<SqlSavepoint<'_>> {
        let name = format!("calendio_{}", SAVEPOINT.fetch_add(1, Ordering::Relaxed));
        self.conn().execute_batch(&format!("SAVEPOINT {name}"))?;
        Ok(SqlSavepoint { conn: self.conn(), name, done: false })
    }
    pub fn snapshot(&self, id: i64) -> Result<EventSnapshot> {
        let event = self.event(id)?.ok_or_else(|| invalid("Event no longer exists"))?;
        let uid = self.conn().query_row("SELECT uid FROM event_uids WHERE event_id=?1", [id], |r| r.get(0))?;
        Ok(EventSnapshot {
            event,
            calendar_id: self.event_calendar(id)?,
            details: self.event_details(id)?,
            exceptions: self.exceptions_for(&[id])?,
            uid,
        })
    }
    pub fn record_edit(&self, label: &str, id: Option<i64>, action: impl FnOnce(&Self) -> Result<i64>) -> Result<i64> {
        let transaction = self.savepoint()?;
        let before = id.map(|id| self.snapshot(id)).transpose()?;
        if before.as_ref().is_some_and(|s| s.event.booking_id.is_some()) {
            return Err(invalid(
                "Manage booked events from Booking, so guest details and availability stay consistent",
            ));
        }
        let changed = action(self)?;
        if id.is_some_and(|id| id != changed) {
            return Err(invalid("An edit changed the wrong event"));
        }
        let after = if self.event(changed)?.is_some() { Some(self.snapshot(changed)?) } else { None };
        if after.as_ref().is_some_and(|s| s.event.booking_id.is_some()) {
            return Err(invalid("Bookings cannot be edited through event undo"));
        }
        if before != after {
            let encode = |v: &Option<EventSnapshot>| {
                v.as_ref().map(serde_json::to_string).transpose().map_err(|_| invalid("Cannot save undo history"))
            };
            self.conn().execute(
                "INSERT INTO edit_history(label,event_id,before_json,after_json) VALUES (?1,?2,?3,?4)",
                params![label, changed, encode(&before)?, encode(&after)?],
            )?;
            self.conn().execute(
                "DELETE FROM edit_history WHERE id NOT IN (SELECT id FROM edit_history ORDER BY id DESC LIMIT 100)",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(changed)
    }
    pub fn undo_label(&self) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT label FROM edit_history WHERE undone=0 ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .optional()?)
    }
    pub fn undo(&self) -> Result<String> {
        let transaction = self.savepoint()?;
        let row=self.conn().query_row("SELECT id,label,event_id,before_json,after_json FROM edit_history WHERE undone=0 ORDER BY id DESC LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?))).optional()?.ok_or_else(||invalid("Nothing to undo"))?;
        let decode = |v: Option<String>| {
            v.map(|v| serde_json::from_str::<EventSnapshot>(&v))
                .transpose()
                .map_err(|_| invalid("Invalid undo history"))
        };
        let before = decode(row.3)?;
        let after = decode(row.4)?;
        let current = if self.event(row.2)?.is_some() { Some(self.snapshot(row.2)?) } else { None };
        if current != after {
            return Err(invalid("This event changed after the edit. Undo would overwrite newer work"));
        }
        if let Some(snapshot) = before {
            self.restore_event(&snapshot)?;
        } else {
            self.delete_event(row.2)?;
        }
        self.conn().execute("UPDATE edit_history SET undone=1 WHERE id=?1", [row.0])?;
        transaction.commit()?;
        Ok(row.1)
    }
    pub fn restore_event(&self, snapshot: &EventSnapshot) -> Result<()> {
        let e = &snapshot.event;
        let transaction = self.savepoint()?;
        if e.booking_id.is_some() {
            return Err(invalid("Booking snapshots require booking management"));
        }
        self.conn().execute("INSERT INTO events(id,title,notes,start,end,all_day,tz,rrule,color,reminder_minutes,created_at,updated_at,calendar_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(id) DO UPDATE SET title=excluded.title,notes=excluded.notes,start=excluded.start,end=excluded.end,all_day=excluded.all_day,tz=excluded.tz,rrule=excluded.rrule,color=excluded.color,reminder_minutes=excluded.reminder_minutes,created_at=excluded.created_at,updated_at=excluded.updated_at,calendar_id=excluded.calendar_id",params![e.id,e.title,e.notes,e.start,e.end,e.all_day,e.tz,e.rrule,e.color,e.reminder_minutes,e.created_at,e.updated_at,snapshot.calendar_id])?;
        self.set_event_details(e.id, &snapshot.details)?;
        self.conn().execute("DELETE FROM event_exceptions WHERE event_id=?1", [e.id])?;
        for exception in &snapshot.exceptions {
            self.set_exception(exception)?;
        }
        self.conn().execute("DELETE FROM event_uids WHERE event_id=?1", [e.id])?;
        self.conn().execute(
            "INSERT INTO event_uids(calendar_id,uid,event_id) VALUES (?1,?2,?3)",
            params![snapshot.calendar_id, snapshot.uid, e.id],
        )?;
        self.conn()
            .execute("UPDATE google_events SET deleted=0 WHERE json_extract(local_uid,'$[1]')=?1", [&snapshot.uid])?;
        transaction.commit()?;
        Ok(())
    }
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        copy_database(self.conn(), path)
    }
    /// Today's backup, made once per UTC day; returns its path either way.
    pub fn daily_backup(&self, directory: &Path, now: DateTime<Utc>) -> Result<PathBuf> {
        std::fs::create_dir_all(directory).map_err(|_| invalid("Cannot create backup directory"))?;
        let path = directory.join(format!("calendio-{}.sqlite3", now.format("%Y-%m-%d")));
        if !path.exists() {
            self.backup_to(&path)?;
        }
        Ok(path)
    }
    /// Deletes all but the newest `keep` daily backups. Manual saves, safety copies and
    /// unrelated files are never touched. Returns how many files were removed.
    pub fn prune_backups(directory: &Path, keep: usize) -> Result<usize> {
        let mut daily = daily_files(directory);
        daily.sort();
        let excess = daily.len().saturating_sub(keep);
        for (_, path) in &daily[..excess] {
            std::fs::remove_file(path).map_err(|_| invalid("Cannot remove an old backup"))?;
        }
        Ok(excess)
    }
    /// Copies every daily backup the mirror folder lacks (a USB stick, a synced cloud
    /// folder), then prunes the mirror to `keep`. Returns how many files were copied.
    pub fn mirror_backups(source: &Path, mirror: &Path, keep: usize) -> Result<usize> {
        std::fs::create_dir_all(mirror).map_err(|_| invalid("Cannot reach the backup mirror folder"))?;
        let mut copied = 0;
        for (_, path) in daily_files(source) {
            let Some(name) = path.file_name() else { continue };
            let target = mirror.join(name);
            if target.exists() {
                continue;
            }
            let temporary = Temp(mirror.join(format!(".calendio-{}.tmp", random_secret()?)));
            std::fs::copy(&path, &temporary.0)
                .and_then(|_| std::fs::File::open(&temporary.0)?.sync_all())
                .and_then(|_| std::fs::rename(&temporary.0, &target))
                .map_err(|_| invalid("Cannot copy a backup to the mirror folder"))?;
            copied += 1;
        }
        Self::prune_backups(mirror, keep)?;
        Ok(copied)
    }
    pub fn restore_backup(&mut self, source: &Path, safety: &Path) -> Result<()> {
        if self
            .setting("booker_heartbeat")?
            .and_then(|v| v.parse::<DateTime<Utc>>().ok())
            .is_some_and(|t| t > Utc::now() - chrono::Duration::seconds(30))
        {
            return Err(invalid("Stop the booking service and wait 30 seconds before restoring"));
        }
        let candidate = rusqlite::Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let version = candidate.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?;
        if !(1..=Self::SCHEMA_VERSION).contains(&version)
            || candidate.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? != "ok"
        {
            return Err(invalid("Not a supported Calendio backup"));
        }
        let stage =
            Temp(safety.parent().unwrap_or(Path::new(".")).join(format!(".calendio-restore-{}.tmp", random_secret()?)));
        copy_database(&candidate, &stage.0)?;
        let validated = Self::open(&stage.0)?;
        if validated.conn().prepare("PRAGMA foreign_key_check")?.query([])?.next()?.is_some() {
            return Err(invalid("Backup contains invalid references"));
        }
        self.backup_to(safety)?;
        let backup = rusqlite::backup::Backup::new(validated.conn(), self.connection_mut())?;
        backup.run_to_completion(128, std::time::Duration::from_millis(5), None)?;
        drop(backup);
        self.conn().execute("UPDATE email_jobs SET status='superseded',lease=NULL,lease_until=NULL WHERE status IN ('pending','sending','failed')",[])?;
        self.set_setting("google_sync_token", "")?;
        self.set_setting("reminders_checked", &time(Utc::now()))?;
        Ok(())
    }
}

fn daily_files(directory: &Path) -> Vec<(NaiveDate, PathBuf)> {
    list_backups(directory).into_iter().filter(|b| b.kind == BackupKind::Daily).map(|b| (b.date, b.path)).collect()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BackupKind {
    /// `calendio-YYYY-MM-DD.sqlite3`, written by the booking service once a day.
    Daily,
    /// `calendio-YYYY-MM-DD-HHMMSS.sqlite3`, from "Save backup now".
    Manual,
    /// `before-restore-YYYY-MM-DD-HHMMSS.sqlite3`, the safety copy a restore makes.
    BeforeRestore,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BackupFile {
    pub path: PathBuf,
    pub kind: BackupKind,
    /// The UTC date in the file name.
    pub date: NaiveDate,
    /// The UTC time in the file name; daily backups have none.
    pub time: Option<NaiveTime>,
}
fn parse_backup(path: PathBuf) -> Option<BackupFile> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".sqlite3")?;
    let (kind, rest) = if let Some(rest) = stem.strip_prefix("before-restore-") {
        (BackupKind::BeforeRestore, rest)
    } else {
        let rest = stem.strip_prefix("calendio-")?;
        (if rest.len() == 10 { BackupKind::Daily } else { BackupKind::Manual }, rest)
    };
    if kind == BackupKind::Daily {
        let date = NaiveDate::parse_from_str(rest, "%Y-%m-%d").ok()?;
        return Some(BackupFile { path, kind, date, time: None });
    }
    let stamp = NaiveDateTime::parse_from_str(rest, "%Y-%m-%d-%H%M%S").ok()?;
    Some(BackupFile { path, kind, date: stamp.date(), time: Some(stamp.time()) })
}
/// Every Calendio backup in `directory`, newest first.
pub fn list_backups(directory: &Path) -> Vec<BackupFile> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    let mut list: Vec<_> = entries.flatten().filter_map(|e| parse_backup(e.path())).collect();
    // A timed copy from today sorts above today's daily file: it was taken later.
    list.sort_by(|a, b| (b.date, b.time.is_some(), b.time, b.kind).cmp(&(a.date, a.time.is_some(), a.time, a.kind)));
    list
}
/// "Today", "Yesterday", "Tue 3 Nov", plus the local time for timed copies.
pub fn backup_label(backup: &BackupFile, now: DateTime<FixedOffset>, time: impl Fn(NaiveTime) -> String) -> String {
    let local = backup.time.map(|t| now.offset().from_utc_datetime(&backup.date.and_time(t)).naive_local());
    let date = local.map_or(backup.date, |l| l.date());
    let today = now.date_naive();
    let mut day = if date == today {
        "Today".to_owned()
    } else if Some(date) == today.pred_opt() {
        "Yesterday".to_owned()
    } else {
        date.format("%a %-d %b").to_string()
    };
    if date.year() != today.year() && date != today && Some(date) != today.pred_opt() {
        day.push_str(&date.format(" %Y").to_string());
    }
    match (backup.kind, local) {
        (BackupKind::Manual, Some(l)) => format!("{day}, saved at {}", time(l.time())),
        (BackupKind::BeforeRestore, Some(l)) => format!("{day}, before a restore at {}", time(l.time())),
        _ => day,
    }
}
