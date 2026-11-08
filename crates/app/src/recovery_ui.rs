//! Local recovery and integration status. Credentials never enter text widgets.
use crate::{clock::Clock, theme};
use calendio_core::{db::Db, recovery};
use chrono::{DateTime, FixedOffset, Local, Utc};
use eframe::egui::{self, RichText};
use std::path::{Path, PathBuf};
#[derive(Default)]
pub struct RecoveryUi {
    pub open: bool,
    /// The backup picked in the list, waiting for the confirm box.
    chosen: Option<PathBuf>,
    confirm: bool,
    mirror: Option<String>,
    message: String,
}
fn directory() -> PathBuf {
    std::env::var_os("CALENDIO_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Db::default_path().parent().unwrap_or(Path::new(".")).join("backups"))
}
/// The restore list, newest first: ("Today", path), ("Yesterday", path), …
pub fn restore_rows(folder: &Path, now: DateTime<FixedOffset>, clock: Clock) -> Vec<(String, PathBuf)> {
    recovery::list_backups(folder)
        .into_iter()
        .map(|b| (recovery::backup_label(&b, now, |t| clock.short(t)), b.path))
        .collect()
}
fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).small().color(theme::ink_3()));
}
fn stamp() -> String {
    Utc::now().format("%Y-%m-%d-%H%M%S").to_string()
}
impl RecoveryUi {
    pub fn show(&mut self, ctx: &egui::Context, db: &mut Db, clock: Clock) -> bool {
        if !self.open {
            return false;
        }
        let mut open = self.open;
        let mut changed = false;
        egui::Window::new("Recovery & connections").open(&mut open).default_width(540.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(620.0).show(ui, |ui| {
                changed |= self.undo(ui, db);
                self.backups(ui, db);
                changed |= self.restore(ui, db, clock);
                ui.separator();
                self.connections(ui, db, clock);
                ui.separator();
                ui.label(&self.message);
            });
        });
        self.open = open;
        changed
    }
    fn undo(&mut self, ui: &mut egui::Ui, db: &Db) -> bool {
        ui.heading("Undo & backups");
        ui.label("Undo restores the last saved event edit when it has not changed again.");
        let undo = db.undo_label().ok().flatten();
        let text = undo.as_ref().map(|s| format!("Undo {s}")).unwrap_or_else(|| "Nothing to undo".into());
        if !ui.add_enabled(undo.is_some(), egui::Button::new(text)).clicked() {
            return false;
        }
        match db.undo() {
            Ok(label) => {
                self.message = format!("Undid {label}");
                true
            }
            Err(error) => {
                self.message = error.to_string();
                false
            }
        }
    }
    fn backups(&mut self, ui: &mut egui::Ui, db: &Db) {
        if ui.button("Save backup now").clicked() {
            let folder = directory();
            let path = folder.join(format!("calendio-{}.sqlite3", stamp()));
            let result = std::fs::create_dir_all(&folder)
                .map_err(|e| e.to_string())
                .and_then(|_| db.backup_to(&path).map_err(|e| e.to_string()));
            self.message = match result {
                Ok(()) => format!("Saved {}", path.display()),
                Err(error) => error,
            };
        }
        note(ui, format!("Backups: {} (the newest 30 daily copies are kept)", directory().display()));
        if let Some(status) = db.setting("backup_status").ok().flatten() {
            note(ui, status);
        }
        ui.add_space(6.0);
        ui.label("Also copy daily backups to");
        let mirror = self.mirror.get_or_insert_with(|| db.setting("backup_mirror").ok().flatten().unwrap_or_default());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(mirror).hint_text("A USB stick or a synced folder").desired_width(380.0));
            if ui.button("Save").clicked() {
                self.message = match db.set_setting("backup_mirror", mirror.trim()) {
                    Ok(()) if mirror.trim().is_empty() => "Backups stay on this disk only".into(),
                    Ok(()) => format!("Daily backups will also be copied to {}", mirror.trim()),
                    Err(error) => error.to_string(),
                };
            }
        });
        note(ui, "The booking service copies them once a day; leave it empty to keep backups on this disk only.");
    }
    fn restore(&mut self, ui: &mut egui::Ui, db: &mut Db, clock: Clock) -> bool {
        ui.separator();
        ui.label("Restore a backup");
        let rows = restore_rows(&directory(), Local::now().fixed_offset(), clock);
        if rows.is_empty() {
            note(ui, "No backups yet. The booking service saves one every day.");
        }
        egui::ScrollArea::vertical().id_salt("backup-list").max_height(220.0).show(ui, |ui| {
            for (label, path) in &rows {
                ui.horizontal(|ui| {
                    let chosen = self.chosen.as_ref() == Some(path);
                    ui.label(if chosen { RichText::new(label).strong() } else { RichText::new(label) });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!chosen, egui::Button::new("Restore")).clicked() {
                            self.chosen = Some(path.clone());
                            self.confirm = false;
                        }
                    });
                });
            }
        });
        let Some(chosen) = self.chosen.clone() else { return false };
        let label = rows.iter().find(|(_, p)| *p == chosen).map_or("this backup", |(l, _)| l.as_str());
        ui.checkbox(&mut self.confirm, format!("Replace current data with {label}; save a safety backup first"));
        note(
            ui,
            "Stop the booking service and wait 30 seconds before restoring. \
             Restored notification jobs are paused to prevent duplicate email.",
        );
        let mut changed = false;
        ui.horizontal(|ui| {
            if ui.add_enabled(self.confirm, egui::Button::new("Restore backup")).clicked() {
                let safety = directory().join(format!("before-restore-{}.sqlite3", stamp()));
                match db.restore_backup(&chosen, &safety) {
                    Ok(()) => {
                        changed = true;
                        self.chosen = None;
                        self.confirm = false;
                        self.message = format!("Restored. Safety backup: {}", safety.display());
                    }
                    Err(error) => self.message = error.to_string(),
                }
            }
            if ui.button("Cancel").clicked() {
                self.chosen = None;
                self.confirm = false;
            }
        });
        changed
    }
    fn connections(&mut self, ui: &mut egui::Ui, db: &Db, clock: Clock) {
        ui.heading("Connections");
        ui.label("Google Calendar");
        let google = GoogleStatus {
            configured: google_configured(),
            status: db.setting("google_status").ok().flatten(),
            last_sync: db.setting("google_last_sync").ok().flatten(),
        };
        let (line, last, error) = google.summary(Local::now().fixed_offset(), clock);
        ui.label(if error { RichText::new(line).color(theme::danger()) } else { RichText::new(line) });
        if let Some(last) = last {
            note(ui, last);
        }
        note(
            ui,
            "The booking service synchronizes every minute while running. Credentials stay in \
             ~/.config/calendio/google.env and the desktop keyring.",
        );
        if ui.button("Connect Google Calendar").clicked() {
            let command = std::env::current_exe().map(|p| p.with_file_name("calendio-booker"));
            let spawned = command.and_then(|p| {
                std::process::Command::new(p)
                    .arg("--google-connect")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
            });
            self.message = match spawned {
                Ok(_) => {
                    "Google sign-in started. Check the connection status after completing it in your browser.".into()
                }
                Err(error) => format!("Build/install calendio-booker first: {error}"),
            };
        }
        ui.add_space(12.0);
        ui.label("Booking email");
        ui.label(db.setting("email_status").ok().flatten().unwrap_or_else(|| "Not configured".into()));
        if let Ok((pending, failed)) = db.email_counts() {
            ui.label(format!("{pending} queued · {failed} failed"));
            if failed > 0 && ui.button("Retry failed email").clicked() {
                self.message = match db.retry_failed_email(Utc::now()) {
                    Ok(()) => "Failed messages queued for retry".into(),
                    Err(error) => error.to_string(),
                };
            }
        }
        note(
            ui,
            "Delivery requires a verified sender, Resend API key, and the public booking URL. See docs/operations.md.",
        );
    }
}
/// Whether a Google Desktop client ID is set, in the environment or google.env.
fn google_configured() -> bool {
    std::env::var("CALENDIO_GOOGLE_CLIENT_ID").is_ok_and(|v| !v.trim().is_empty())
        || calendio_core::config::read_env(&calendio_core::config::google_env_path())
            .iter()
            .any(|(k, v)| k == "CALENDIO_GOOGLE_CLIENT_ID" && !v.trim().is_empty())
}
/// What the booking service last wrote about Google, plus whether a client is set up.
pub struct GoogleStatus {
    pub configured: bool,
    pub status: Option<String>,
    pub last_sync: Option<String>,
}
impl GoogleStatus {
    /// (headline, "Last synced …" line, headline is an error)
    pub fn summary(&self, now: DateTime<FixedOffset>, clock: Clock) -> (String, Option<String>, bool) {
        let last = self.last_sync.as_deref().and_then(|t| t.parse::<DateTime<Utc>>().ok()).map(|t| {
            let local = t.with_timezone(now.offset());
            let day = match (now.date_naive() - local.date_naive()).num_days() {
                0 => "today".to_owned(),
                1 => "yesterday".to_owned(),
                _ => local.format("%a %-d %b").to_string(),
            };
            format!("Last synced {day} at {}", clock.short(local.time()))
        });
        let status = self.status.clone().unwrap_or_default();
        let fine = status.is_empty()
            || ["Synced", "Not connected", "Google Calendar connected"].iter().any(|ok| status.starts_with(ok));
        if !self.configured && (status.is_empty() || status.starts_with("Not connected")) {
            return ("Not set up: add a Desktop OAuth client to ~/.config/calendio/google.env".into(), last, false);
        }
        let headline = if status.is_empty() { "Not connected".to_owned() } else { status };
        (headline, last, !fine)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn google_summary_shows_setup_last_sync_and_errors() {
        let now = "2026-11-08T22:00:00+06:00".parse().unwrap();
        let status = |configured: bool, status: Option<&str>, last: Option<&str>| GoogleStatus {
            configured,
            status: status.map(str::to_owned),
            last_sync: last.map(str::to_owned),
        };
        let (line, last, error) = status(false, None, None).summary(now, Clock::H12);
        assert!(line.starts_with("Not set up") && last.is_none() && !error);
        let (line, _, error) = status(true, Some("Not connected"), None).summary(now, Clock::H12);
        assert_eq!((line.as_str(), error), ("Not connected", false));
        let (line, last, error) =
            status(true, Some("Synced: 2 pulled, 1 pushed, 0 conflict copies"), Some("2026-11-08T15:41:00+00:00"))
                .summary(now, Clock::H12);
        assert!(line.starts_with("Synced") && !error);
        assert_eq!(last.as_deref(), Some("Last synced today at 9:41 PM"));
        let (line, last, error) = status(true, Some("Google token refresh failed"), Some("2026-11-07T03:00:00+00:00"))
            .summary(now, Clock::H24);
        assert_eq!((line.as_str(), error), ("Google token refresh failed", true));
        assert_eq!(last.as_deref(), Some("Last synced yesterday at 09:00"));
    }

    use super::*;
    #[test]
    fn restore_rows_follow_the_clock_and_list_newest_first() {
        let folder = std::env::temp_dir().join(format!("calendio-app-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["calendio-2026-11-07.sqlite3", "calendio-2026-11-08-143000.sqlite3", "pre-claude.db"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let now = "2026-11-08T22:00:00+06:00".parse().unwrap();
        let labels: Vec<_> = restore_rows(&folder, now, Clock::H12).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels, ["Today, saved at 8:30 PM", "Yesterday"]);
        let labels: Vec<_> = restore_rows(&folder, now, Clock::H24).into_iter().map(|(l, _)| l).collect();
        assert_eq!(labels[0], "Today, saved at 20:30");
        std::fs::remove_dir_all(folder).unwrap();
    }
}
