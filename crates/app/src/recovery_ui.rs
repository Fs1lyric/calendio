//! Local recovery and integration status. Credentials never enter text widgets.
use crate::theme;
use calendio_core::db::Db;
use chrono::Utc;
use eframe::egui::{self, RichText};
use std::path::PathBuf;
#[derive(Default)]
pub struct RecoveryUi {
    pub open: bool,
    path: String,
    confirm: bool,
    message: String,
}
fn directory() -> PathBuf {
    std::env::var_os("CALENDIO_BACKUP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Db::default_path().parent().unwrap_or(std::path::Path::new(".")).join("backups"))
}
impl RecoveryUi {
    pub fn show(&mut self, ctx: &egui::Context, db: &mut Db) -> bool {
        if !self.open {
            return false;
        }
        let mut open = self.open;
        let mut changed = false;
        egui::Window::new("Recovery & connections").open(&mut open).default_width(540.0).show(ctx,|ui|{egui::ScrollArea::vertical().max_height(620.0).show(ui,|ui|{
  ui.heading("Undo & backups");ui.label("Undo restores the last saved event edit when it has not changed again.");let undo=db.undo_label().ok().flatten();if ui.add_enabled(undo.is_some(),egui::Button::new(undo.map(|s|format!("Undo {s}")).unwrap_or_else(||"Nothing to undo".into()))).clicked(){match db.undo(){Ok(label)=>{self.message=format!("Undid {label}");changed=true;},Err(error)=>self.message=error.to_string()}}
  if ui.button("Save backup now").clicked(){let folder=directory();let result=std::fs::create_dir_all(&folder).map_err(|e|e.to_string()).and_then(|_|{let path=folder.join(format!("calendio-{}.sqlite3",Utc::now().format("%Y-%m-%d-%H%M%S")));db.backup_to(&path).map(|_|path).map_err(|e|e.to_string())});self.message=match result{Ok(path)=>format!("Saved {}",path.display()),Err(error)=>error};}ui.label(RichText::new(format!("Backups: {}",directory().display())).small().color(theme::ink_3()));
  ui.separator();ui.label("Restore from a Calendio backup");ui.add(egui::TextEdit::singleline(&mut self.path).hint_text("/path/to/backup.sqlite3").desired_width(f32::INFINITY));ui.checkbox(&mut self.confirm,"Replace current data; save a safety backup first");ui.label(RichText::new("Stop the booking service and wait 30 seconds before restoring. Restored notification jobs are paused to prevent duplicate email.").small().color(theme::ink_3()));if ui.add_enabled(self.confirm&&!self.path.trim().is_empty(),egui::Button::new("Restore backup")).clicked(){let safety=directory().join(format!("before-restore-{}.sqlite3",Utc::now().format("%Y-%m-%d-%H%M%S")));let result=std::fs::create_dir_all(directory()).map_err(|e|e.to_string()).and_then(|_|db.restore_backup(std::path::Path::new(self.path.trim()),&safety).map_err(|e|e.to_string()));match result{Ok(())=>{changed=true;self.confirm=false;self.message=format!("Restored. Safety backup: {}",safety.display());},Err(error)=>self.message=error}}
  ui.separator();ui.heading("Connections");ui.label("Google Calendar");ui.label(db.setting("google_status").ok().flatten().unwrap_or_else(||"Not connected".into()));ui.label(RichText::new("Set your Desktop OAuth client credentials, then connect. The booking service synchronizes every minute while running.").small().color(theme::ink_3()));if ui.button("Connect Google Calendar").clicked(){let command=std::env::current_exe().map(|p|p.with_file_name("calendio-booker"));match command.and_then(|p|std::process::Command::new(p).arg("--google-connect").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()){Ok(_)=>self.message="Google sign-in started. Check the connection status after completing it in your browser.".into(),Err(error)=>self.message=format!("Build/install calendio-booker first: {error}")}}
  ui.add_space(12.0);ui.label("Booking email");ui.label(db.setting("email_status").ok().flatten().unwrap_or_else(||"Not configured".into()));if let Ok((pending,failed))=db.email_counts(){ui.label(format!("{pending} queued · {failed} failed"));if failed>0&&ui.button("Retry failed email").clicked(){self.message=match db.retry_failed_email(Utc::now()){Ok(())=>"Failed messages queued for retry".into(),Err(error)=>error.to_string()};}}ui.label(RichText::new("Delivery requires a verified sender, Resend API key, and the public booking URL. See docs/operations.md.").small().color(theme::ink_3()));ui.separator();ui.label(&self.message);
 });});
        self.open = open;
        changed
    }
}
