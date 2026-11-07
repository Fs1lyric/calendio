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
            new_color: "#007aff".into(),
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
    pub fn open_manage(&mut self) {
        self.drafts = self.calendars.clone();
        self.manage_open = true;
        self.message.clear();
    }
    pub fn open_transfer(&mut self) {
        self.transfer_open = true;
        self.message.clear();
    }
    /// The toolbar's search field. Typing shows results in place of the
    /// calendar; Escape clears it and goes back.
    pub fn search_field(&mut self, ui: &mut egui::Ui, db: &Db, width: f32) {
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Search")
                .margin(egui::Margin { left: 26, right: 8, top: 4, bottom: 4 })
                .min_size(egui::vec2(width, crate::chrome::CONTROL_H))
                .desired_width(width - 34.0),
        );
        let lens = egui::Rect::from_min_size(response.rect.min, egui::vec2(24.0, response.rect.height()));
        crate::chrome::magnifier_glyph(ui.painter(), lens, theme::ink_3());
        if self.focus_search {
            response.request_focus();
            self.focus_search = false;
        }
        if response.changed() {
            self.search_open = !self.query.trim().is_empty();
            self.find(db);
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.query.clear();
            self.search_open = false;
            response.surrender_focus();
        }
    }
    fn find(&mut self, db: &Db) {
        match db.search_events(&self.query) {
            Ok(results) => self.results = results,
            Err(e) => self.message = e.to_string(),
        }
    }
    pub fn calendar_picker(&mut self, ui: &mut egui::Ui, id: &str) {
        let name = self.calendars.iter().find(|c| c.id == self.selected).map(|c| c.name.as_str()).unwrap_or("Personal");
        egui::ComboBox::from_id_salt(id).selected_text(name).show_ui(ui, |ui| {
            for calendar in &self.calendars {
                ui.selectable_value(&mut self.selected, calendar.id, &calendar.name);
            }
        });
    }
    /// "My Calendars": the round checkbox in each calendar's colour shows or
    /// hides it, and clicking the name makes it the calendar new events go to.
    pub fn sidebar(&mut self, ui: &mut egui::Ui, db: &Db) -> Result<bool, String> {
        use egui::{pos2, vec2, Align2, FontId, Sense, Stroke};
        ui.label(RichText::new("My Calendars").font(theme::semibold(12.0)).color(theme::ink_3()));
        ui.add_space(2.0);
        let mut changed = false;
        for c in &mut self.calendars {
            let colour = theme::hex(Some(&c.color));
            let (row, name) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
            let centre = pos2(row.left() + 13.0, row.center().y);
            let dot = ui.interact(
                egui::Rect::from_center_size(centre, vec2(22.0, 22.0)),
                ui.id().with(("calendar-visible", c.id)),
                Sense::click(),
            );
            let p = ui.painter();
            if self.selected == c.id {
                p.rect_filled(row, egui::CornerRadius::same(6), theme::hover());
            }
            if c.visible {
                p.circle_filled(centre, 7.0, colour);
                let tick = [centre + vec2(-3.4, 0.2), centre + vec2(-1.0, 2.6), centre + vec2(3.4, -2.4)];
                p.add(egui::Shape::line(tick.to_vec(), Stroke::new(1.6, egui::Color32::WHITE)));
            } else {
                p.circle_stroke(centre, 6.3, Stroke::new(1.5, colour));
            }
            let text = pos2(row.left() + 30.0, row.center().y);
            p.text(text, Align2::LEFT_CENTER, &c.name, FontId::proportional(13.0), theme::ink());
            if dot.on_hover_text(if c.visible { "Hide" } else { "Show" }).clicked() {
                c.visible = !c.visible;
                db.update_calendar(c.id, &c.name, &c.color, c.visible).map_err(|e| e.to_string())?;
                changed = true;
            } else if name.on_hover_text("New events go here").clicked() {
                self.selected = c.id;
            }
        }
        Ok(changed)
    }
    /// Search results in place of the calendar. Narrow windows have no
    /// toolbar field, so `with_field` puts one above the results.
    pub fn search(&mut self, ui: &mut egui::Ui, db: &Db, tz: Tz, with_field: bool) -> Option<(Form, Event)> {
        if with_field {
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
                self.find(db);
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
                    });
                    colour_row(ui, &mut calendar.color);
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
            });
            colour_row(ui, &mut self.new_color);
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
                        workspace.search_field(ui, &db, 180.0);
                        workspace.search(ui, &db, chrono_tz::UTC, width < 700.0);
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

/// A calendar always has a colour, so the picker has no "calendar colour" swatch.
fn colour_row(ui: &mut egui::Ui, colour: &mut String) {
    let mut picked = Some(colour.clone());
    if crate::swatches::picker(ui, &mut picked, None) {
        if let Some(c) = picked {
            *colour = c;
        }
    }
}
