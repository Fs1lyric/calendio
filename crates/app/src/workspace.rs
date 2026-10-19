//! Calendar management, search and file interchange. All writes go through core.
use crate::{form::Form, grid, theme};
use calendio_core::{
    calendars::Calendar,
    db::{Db, Event},
};
use chrono_tz::Tz;
use eframe::egui::{self, RichText};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::PathBuf,
};

pub struct Workspace {
    pub calendars: Vec<Calendar>,
    pub selected: i64,
    pub search_open: bool,
    query: String,
    focus_search: bool,
    results: Vec<Event>,
    manage_open: bool,
    drafts: Vec<Calendar>,
    new_name: String,
    new_color: String,
    transfer_open: bool,
    path: String,
    message: String,
    delete_pending: Option<i64>,
}

impl Workspace {
    pub fn new(db: &Db) -> Self {
        Self {
            calendars: db.calendars().unwrap_or_default(),
            selected: 1,
            search_open: false,
            query: String::new(),
            focus_search: false,
            results: Vec::new(),
            manage_open: false,
            drafts: Vec::new(),
            new_name: String::new(),
            new_color: "#346538".into(),
            transfer_open: false,
            path: String::new(),
            message: String::new(),
            delete_pending: None,
        }
    }
    pub fn refresh(&mut self, db: &Db) -> Result<(), String> {
        self.calendars = db.calendars().map_err(|e| e.to_string())?;
        if !self.calendars.iter().any(|c| c.id == self.selected) {
            self.selected = 1;
        }
        if self.search_open {
            self.results = db.search_events(&self.query).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn open_search(&mut self) {
        self.search_open = true;
        self.focus_search = true;
    }
    pub fn toolbar(&mut self, ui: &mut egui::Ui) -> bool {
        let mut quick = false;
        ui.horizontal_wrapped(|ui| {
            quick = ui.button("Quick add").on_hover_text("Q · Write an event in one line").clicked();
            if ui.selectable_label(self.search_open, "Search events").on_hover_text("Ctrl+K").clicked() {
                self.search_open = !self.search_open;
                self.focus_search = self.search_open;
            }
            if ui.button("Calendars").clicked() {
                self.drafts = self.calendars.clone();
                self.manage_open = true;
                self.message.clear();
            }
            if ui.button("Import / export").clicked() {
                self.transfer_open = true;
                self.message.clear();
            }
            ui.separator();
            self.calendar_picker(ui, "new-event-calendar");
        });
        quick
    }
    pub fn calendar_picker(&mut self, ui: &mut egui::Ui, id: &str) {
        let name = self.calendars.iter().find(|c| c.id == self.selected).map(|c| c.name.as_str()).unwrap_or("Personal");
        egui::ComboBox::from_id_salt(id).selected_text(name).show_ui(ui, |ui| {
            for calendar in &self.calendars {
                ui.selectable_value(&mut self.selected, calendar.id, &calendar.name);
            }
        });
    }
    pub fn sidebar(&mut self, ui: &mut egui::Ui, db: &Db) -> Result<bool, String> {
        let mut changed = false;
        for c in &mut self.calendars {
            let was_visible = c.visible;
            ui.horizontal(|ui| {
                if ui.checkbox(&mut c.visible, "").changed() {
                    changed = true;
                }
                if ui.selectable_label(self.selected == c.id, RichText::new(&c.name).color(theme::ink())).clicked() {
                    self.selected = c.id;
                }
            });
            if was_visible != c.visible {
                db.update_calendar(c.id, &c.name, &c.color, c.visible).map_err(|e| e.to_string())?;
            }
        }
        Ok(changed)
    }
    pub fn search(&mut self, ui: &mut egui::Ui, db: &Db, tz: Tz) -> Option<(Form, Event)> {
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search titles, notes, and locations across every calendar")
                .desired_width(f32::INFINITY),
        );
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        if response.changed() {
            match db.search_events(&self.query) {
                Ok(results) => self.results = results,
                Err(e) => self.message = e.to_string(),
            }
        }
        ui.add_space(12.0);
        ui.label(RichText::new(format!("{} results", self.results.len())).color(theme::ink_3()));
        let mut open = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in &self.results {
                let date = if event.all_day {
                    event.start.clone()
                } else {
                    grid::local(&event.start, tz).format("%a %-d %b %Y · %H:%M").to_string()
                };
                let cal = db
                    .event_calendar(event.id)
                    .ok()
                    .and_then(|id| self.calendars.iter().find(|c| c.id == id))
                    .map(|c| c.name.as_str())
                    .unwrap_or("Calendar");
                if ui
                    .add_sized(
                        [ui.available_width(), 48.0],
                        egui::Button::new(format!("{}\n{date} · {cal}", event.title)),
                    )
                    .clicked()
                {
                    let o = calendio_core::recur::Occurrence {
                        event_id: event.id,
                        title: event.title.clone(),
                        notes: event.notes.clone(),
                        start: event.start.clone(),
                        end: event.end.clone(),
                        all_day: event.all_day,
                        color: event.color.clone(),
                        reminder_minutes: event.reminder_minutes,
                        booking_id: event.booking_id,
                        original_start: None,
                    };
                    open = Some((Form::open(&o, event, tz), event.clone()));
                }
            }
        });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        open
    }
    pub fn dialogs(&mut self, ctx: &egui::Context, db: &Db, tz: Tz) -> bool {
        let mut changed = false;
        let mut manage_open = self.manage_open;
        egui::Window::new("Your calendars").open(&mut manage_open).default_width(520.0).show(ctx, |ui| {
            ui.label("Visibility only affects your view. All events still block bookings.");
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for calendar in &mut self.drafts {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut calendar.visible, "Visible");
                        ui.add(egui::TextEdit::singleline(&mut calendar.name).desired_width(150.0));
                        ui.add(egui::TextEdit::singleline(&mut calendar.color).desired_width(85.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Save calendar").clicked() {
                            match db.update_calendar(calendar.id, &calendar.name, &calendar.color, calendar.visible) {
                                Ok(()) => {
                                    changed = true;
                                    self.message = "Calendar saved".into();
                                }
                                Err(e) => self.message = e.to_string(),
                            }
                        }
                        if calendar.id != 1
                            && ui
                                .button(if self.delete_pending == Some(calendar.id) {
                                    "Confirm: move events to Personal"
                                } else {
                                    "Remove calendar"
                                })
                                .clicked()
                        {
                            if self.delete_pending == Some(calendar.id) {
                                match db.delete_calendar(calendar.id) {
                                    Ok(()) => {
                                        changed = true;
                                        self.message = "Calendar removed; events moved to Personal".into();
                                    }
                                    Err(e) => self.message = e.to_string(),
                                }
                                self.delete_pending = None;
                            } else {
                                self.delete_pending = Some(calendar.id);
                            }
                        }
                    });
                }
            });
            ui.separator();
            ui.label("New calendar");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_name)
                        .hint_text("Work, Family, Projects…")
                        .desired_width(240.0),
                );
                ui.add(egui::TextEdit::singleline(&mut self.new_color).desired_width(85.0));
            });
            if ui.button("Create calendar").clicked() {
                match db.create_calendar(&self.new_name, &self.new_color) {
                    Ok(id) => {
                        self.selected = id;
                        changed = true;
                        self.new_name.clear();
                        self.message = "Calendar created".into();
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            ui.label(&self.message);
        });
        self.manage_open = manage_open;
        let mut transfer_open = self.transfer_open;
        egui::Window::new("Calendar files").open(&mut transfer_open).default_width(530.0).show(ctx, |ui| {
            ui.label("Import or export an iCalendar (.ics) file.");
            ui.label("Imports add new event IDs and skip previously imported IDs.");
            ui.label("Some external metadata cannot be imported; warnings appear below.");
            self.calendar_picker(ui, "transfer-calendar");
            ui.add(
                egui::TextEdit::singleline(&mut self.path)
                    .hint_text("/home/lyric/Downloads/calendar.ics")
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                if ui.button("Import file").clicked() {
                    let result = self.import(db, tz);
                    match result {
                        Ok(message) => {
                            self.message = message;
                            changed = true;
                        }
                        Err(e) => self.message = e,
                    }
                }
                if ui.button("Export to new file").clicked() {
                    self.message = match self.export(db) {
                        Ok(()) => "Calendar exported".into(),
                        Err(e) => e,
                    };
                }
            });
            ui.label("Export creates a new file. Choose a new filename to keep an existing export.");
            egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                ui.label(&self.message);
            });
        });
        self.transfer_open = transfer_open;
        if changed {
            if let Err(e) = self.refresh(db) {
                self.message = e;
            }
            self.drafts = self.calendars.clone();
        }
        changed
    }
    fn file_path(&self) -> Result<PathBuf, String> {
        let path = self.path.trim();
        if path.is_empty() {
            return Err("Enter an .ics file path first".into());
        }
        if let Some(rest) = path.strip_prefix("~/") {
            Ok(PathBuf::from(std::env::var_os("HOME").ok_or("Home directory is unavailable")?).join(rest))
        } else {
            Ok(PathBuf::from(path))
        }
    }
    fn import(&self, db: &Db, tz: Tz) -> Result<String, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(self.file_path()?)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let input = std::str::from_utf8(&bytes).map_err(|_| "The ICS file must use UTF-8")?;
        let report = db.import_ics(self.selected, input, &tz.to_string()).map_err(|e| e.to_string())?;
        Ok(format!(
            "Imported {} events; skipped {} existing events.\n{}",
            report.imported,
            report.skipped,
            report.warnings.join("\n")
        ))
    }
    fn export(&self, db: &Db) -> Result<(), String> {
        let data = db.export_ics(self.selected).map_err(|e| e.to_string())?;
        let mut file =
            OpenOptions::new().write(true).create_new(true).open(self.file_path()?).map_err(|e| e.to_string())?;
        file.write_all(data.as_bytes()).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_and_management_screens_render_without_mutating_events() {
        let db = Db::open_in_memory().unwrap();
        db.create_event(&calendio_core::db::EventInput {
            title: "Planning".into(),
            notes: "Review".into(),
            start: "2026-10-19T09:00:00Z".into(),
            end: "2026-10-19T10:00:00Z".into(),
            all_day: false,
            tz: "UTC".into(),
            rrule: None,
            color: None,
            reminder_minutes: None,
        })
        .unwrap();
        for width in [360.0, 1180.0] {
            for state in 0..3 {
                let context = egui::Context::default();
                theme::apply(&context);
                let mut workspace = Workspace::new(&db);
                workspace.query = "Planning".into();
                workspace.search_open = true;
                workspace.refresh(&db).unwrap();
                workspace.manage_open = state == 1;
                workspace.transfer_open = state == 2;
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 780.0))),
                        ..Default::default()
                    },
                    |ui| {
                        workspace.toolbar(ui);
                        workspace.search(ui, &db, chrono_tz::UTC);
                        workspace.dialogs(ui.ctx(), &db, chrono_tz::UTC);
                    },
                );
                assert!(!output.shapes.is_empty());
                output.textures_delta.clear();
            }
        }
        assert_eq!(db.all_events().unwrap().len(), 1);
    }
}
