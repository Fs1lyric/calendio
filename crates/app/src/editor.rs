//! The event editor: a modal over the calendar that edits a [`Form`].

use crate::chrome;
use crate::clock::Clock;
use crate::form::{parse_time, Ends, Form, Repeat, Save, Scope};
use crate::grid::{self, View};
use crate::theme;
use calendio_core::db::{Event, Exception};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use eframe::egui::{self, CornerRadius, Id, RichText};

pub enum Outcome {
    Save(Save),
    DeleteEvent(i64),
    DeleteOne(Exception),
    Close,
}

#[derive(Clone, Copy, PartialEq)]
enum Asking {
    Save,
    Delete,
}

pub struct Editor {
    pub form: Form,
    pub details: calendio_core::details::EventDetails,
    pub calendar_id: i64,
    pub calendars: Vec<calendio_core::calendars::Calendar>,
    /// The stored event when editing one.
    series: Option<Event>,
    clock: Clock,
    start_time: String,
    end_time: String,
    count: String,
    asking: Option<Asking>,
    pub error: Option<String>,
}

const REMINDERS: [(Option<i64>, &str); 7] = [
    (None, "No reminder"),
    (Some(0), "At the start"),
    (Some(5), "5 minutes before"),
    (Some(10), "10 minutes before"),
    (Some(30), "30 minutes before"),
    (Some(60), "1 hour before"),
    (Some(24 * 60), "1 day before"),
];

impl Editor {
    pub fn new(form: Form, series: Option<Event>, clock: Clock) -> Self {
        let count = match form.ends {
            Ends::After(n) => n.to_string(),
            _ => "10".into(),
        };
        Editor {
            details: calendio_core::details::EventDetails { busy: !form.all_day, ..Default::default() },
            calendar_id: 1,
            calendars: Vec::new(),
            clock,
            start_time: clock.field(form.start.time()),
            end_time: clock.field(form.end.time()),
            count,
            form,
            series,
            asking: None,
            error: None,
        }
    }

    fn is_new(&self) -> bool {
        self.form.event_id.is_none()
    }

    /// Copies the time boxes into the form.
    fn read_times(&mut self) -> Result<(), String> {
        if self.form.all_day {
            return Ok(());
        }
        let start = parse_time(&self.start_time).ok_or_else(|| {
            format!("the start time should look like {}", self.clock.field(NaiveTime::from_hms_opt(9, 30, 0).unwrap()))
        })?;
        let end = parse_time(&self.end_time).ok_or_else(|| {
            format!("the end time should look like {}", self.clock.field(NaiveTime::from_hms_opt(10, 30, 0).unwrap()))
        })?;
        self.form.start = self.form.start.date().and_time(start);
        // An end at or before the start on the same date means the next day.
        let mut end_at = self.form.end.date().and_time(end);
        if self.form.end.date() == self.form.start.date() && end_at <= self.form.start && end < start {
            end_at += Duration::days(1);
        }
        self.form.end = end_at;
        Ok(())
    }

    fn save(&mut self, scope: Scope) -> Option<Outcome> {
        if let Err(e) = self.read_times() {
            self.error = Some(e);
            return None;
        }
        match self.form.save(scope, self.series.as_ref()) {
            Ok(save) => Some(Outcome::Save(save)),
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        let mut out = None;
        let modal = egui::Modal::new(Id::new("editor"))
            .frame(egui::Frame::window(&ctx.global_style()).inner_margin(20.0))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(240.0, 420.0));
                let name = self
                    .calendars
                    .iter()
                    .find(|c| c.id == self.calendar_id)
                    .map(|c| c.name.as_str())
                    .unwrap_or("Personal");
                egui::ComboBox::from_id_salt("event-calendar").selected_text(name).icon(chrome::updown).show_ui(
                    ui,
                    |ui| {
                        for calendar in &self.calendars {
                            ui.selectable_value(&mut self.calendar_id, calendar.id, &calendar.name);
                        }
                    },
                );
                ui.add_space(10.0);
                if self.asking.is_some() {
                    if let Some(error) = &self.error {
                        ui.label(RichText::new(error).color(theme::danger()));
                    }
                }
                // The fields scroll; Save and Cancel stay pinned below them so
                // they are reachable however short the window is.
                let mut entered = false;
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() - 200.0).max(120.0)).show(
                    ui,
                    |ui| match self.asking {
                        Some(asking) => out = self.ask_scope(ui, asking),
                        None => entered = self.fields(ui),
                    },
                );
                if out.is_none() && self.asking.is_none() {
                    out = self.footer(ui, entered);
                }
            });
        if out.is_none() && modal.should_close() && self.asking.is_none() {
            out = Some(Outcome::Close);
        }
        out
    }

    /// The event's fields. Returns whether Enter was pressed in the title.
    fn fields(&mut self, ui: &mut egui::Ui) -> bool {
        let calendar_colour =
            theme::hex(self.calendars.iter().find(|c| c.id == self.calendar_id).map(|c| c.color.as_str()));
        let f = &mut self.form;
        let title = egui::TextEdit::singleline(&mut f.title)
            .hint_text(RichText::new("New Event").font(theme::semibold(22.0)).color(theme::ink_3()))
            .font(theme::semibold(22.0))
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY);
        let title = ui.add(title);
        if self.is_new() && !ui.ctx().memory(|m| m.focused().is_some()) {
            title.request_focus();
        }
        let entered = title.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(10.0);

        let f = &mut self.form;
        egui::Grid::new("editor-grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
            ui.label(RichText::new("Starts").color(theme::ink_2()));
            ui.horizontal(|ui| {
                let mut day = f.start.date();
                if date_button(ui, "start-day", &mut day) {
                    let length = f.end - f.start;
                    f.start = day.and_time(f.start.time());
                    f.end = f.start + length;
                }
                if !f.all_day {
                    ui.add(egui::TextEdit::singleline(&mut self.start_time).desired_width(74.0));
                }
            });
            ui.end_row();

            ui.label(RichText::new(if f.all_day { "Last day" } else { "Ends" }).color(theme::ink_2()));
            ui.horizontal(|ui| {
                let mut day = f.end.date();
                if date_button(ui, "end-day", &mut day) {
                    f.end = day.and_time(f.end.time());
                }
                if !f.all_day {
                    ui.add(egui::TextEdit::singleline(&mut self.end_time).desired_width(74.0));
                }
            });
            ui.end_row();

            ui.label("");
            let was = f.all_day;
            // Apple's All-day box is a small rounded square, not a pill.
            ui.scope(|ui| {
                let w = &mut ui.visuals_mut().widgets;
                for v in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                    v.corner_radius = CornerRadius::same(3);
                }
                ui.checkbox(&mut f.all_day, "All-day");
            });
            if was != f.all_day && !f.all_day {
                // Back to timed: give it an hour at nine on the first day.
                f.start = f.start.date().and_time(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
                f.end = f.start + Duration::hours(1);
                self.start_time = self.clock.field(f.start.time());
                self.end_time = self.clock.field(f.end.time());
            } else if was != f.all_day {
                f.end = f.end.date().and_time(NaiveTime::MIN);
                f.start = f.start.date().and_time(NaiveTime::MIN);
            }
            if was != f.all_day {
                self.details.busy = !f.all_day;
            }
            ui.end_row();

            ui.label(RichText::new("Location").color(theme::ink_2()));
            ui.add(
                egui::TextEdit::singleline(&mut self.details.location)
                    .hint_text("Place or address")
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
            ui.label(RichText::new("Meeting link").color(theme::ink_2()));
            ui.vertical(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.details.meeting_url)
                        .hint_text("https://…")
                        .desired_width(f32::INFINITY),
                );
                if !self.details.meeting_url.is_empty() && self.details.clean().is_ok() {
                    ui.hyperlink_to("Open meeting", &self.details.meeting_url);
                }
            });
            ui.end_row();
            ui.label(RichText::new("Show as").color(theme::ink_2()));
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.details.busy, true, "Busy");
                ui.selectable_value(&mut self.details.busy, false, "Free");
            });
            ui.end_row();

            ui.label(RichText::new("Repeat").color(theme::ink_2()));
            ui.vertical(|ui| {
                egui::ComboBox::from_id_salt("repeat")
                    .icon(chrome::updown)
                    .selected_text(f.repeat.label().to_string())
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for p in Repeat::PRESETS {
                            let label = p.label().to_string();
                            ui.selectable_value(&mut f.repeat, p, label);
                        }
                    });
                if f.repeat != Repeat::Never {
                    ui.horizontal(|ui| {
                        let mut kind = match f.ends {
                            Ends::Never => 0,
                            Ends::On(_) => 1,
                            Ends::After(_) => 2,
                        };
                        ui.radio_value(&mut kind, 0, "Forever");
                        ui.radio_value(&mut kind, 1, "Until");
                        ui.radio_value(&mut kind, 2, "Times");
                        f.ends = match (kind, f.ends) {
                            (0, _) => Ends::Never,
                            (1, Ends::On(d)) => Ends::On(d),
                            (1, _) => Ends::On(f.start.date() + Duration::days(30)),
                            (_, _) => Ends::After(self.count.trim().parse().unwrap_or(10).max(1)),
                        };
                    });
                    match &mut f.ends {
                        Ends::On(day) => {
                            date_button(ui, "until-day", day);
                        }
                        Ends::After(n) => {
                            if ui.add(egui::TextEdit::singleline(&mut self.count).desired_width(48.0)).changed() {
                                *n = self.count.trim().parse().unwrap_or(1).max(1);
                            }
                        }
                        Ends::Never => {}
                    }
                }
            });
            ui.end_row();

            ui.label(RichText::new("Reminder").color(theme::ink_2()));
            let current = REMINDERS.iter().find(|r| r.0 == f.reminder_minutes).map_or("Custom", |r| r.1);
            egui::ComboBox::from_id_salt("reminder").icon(chrome::updown).selected_text(current).width(200.0).show_ui(
                ui,
                |ui| {
                    for (minutes, label) in REMINDERS {
                        ui.selectable_value(&mut f.reminder_minutes, minutes, label);
                    }
                },
            );
            ui.end_row();

            ui.vertical(|ui| {
                ui.add_space(3.0);
                ui.label(RichText::new("Colour").color(theme::ink_2()));
            });
            crate::swatches::picker(ui, &mut f.color, Some(calendar_colour));
            ui.end_row();

            ui.label(RichText::new("Notes").color(theme::ink_2()));
            ui.add(egui::TextEdit::multiline(&mut f.notes).desired_rows(3).desired_width(f32::INFINITY));
            ui.end_row();
        });
        entered
    }

    /// The error line and the Delete, Cancel and Save buttons.
    fn footer(&mut self, ui: &mut egui::Ui, entered: bool) -> Option<Outcome> {
        if let Some(e) = &self.error {
            ui.add_space(6.0);
            ui.label(RichText::new(e).color(theme::danger()));
        }
        ui.add_space(14.0);
        let mut out = None;
        ui.horizontal(|ui| {
            if !self.is_new() && ui.button(RichText::new("Delete").color(theme::danger())).clicked() {
                if self.form.is_series() {
                    self.asking = Some(Asking::Delete);
                } else {
                    out = self.form.event_id.map(Outcome::DeleteEvent);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if accent_button(ui, "Save").clicked() || entered {
                    if self.form.is_series() {
                        self.asking = Some(Asking::Save);
                    } else {
                        out = self.save(Scope::All);
                    }
                }
                if ui.button("Cancel").clicked() {
                    out = Some(Outcome::Close);
                }
            });
        });
        out
    }

    /// For a series: does the change apply to this occurrence or all of them?
    fn ask_scope(&mut self, ui: &mut egui::Ui, asking: Asking) -> Option<Outcome> {
        let verb = if asking == Asking::Save { "Change" } else { "Delete" };
        ui.label(RichText::new(format!("{verb} a repeating event")).size(18.0));
        ui.add_space(8.0);
        let mut out = None;
        ui.vertical_centered_justified(|ui| {
            if ui.button(format!("{verb} only this one")).clicked() {
                out = match asking {
                    Asking::Save => self.save(Scope::One),
                    Asking::Delete => self.form.cancel_one().map(Outcome::DeleteOne),
                };
            }
            if ui.button(format!("{verb} every occurrence")).clicked() {
                out = match asking {
                    Asking::Save => self.save(Scope::All),
                    Asking::Delete => self.form.event_id.map(Outcome::DeleteEvent),
                };
            }
            if ui.button("Back").clicked() {
                self.asking = None;
            }
        });
        if out.is_none() && self.error.is_some() {
            self.asking = None;
        }
        out
    }
}

pub fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(theme::bg()).strong())
            .fill(theme::accent())
            .corner_radius(CornerRadius::same(6)),
    )
}

/// A button showing a date that opens a small month to pick from. Returns
/// whether the date changed.
pub fn date_button(ui: &mut egui::Ui, salt: &str, day: &mut NaiveDate) -> bool {
    let button = ui.button(day.format("%a %-d %b %Y").to_string());
    let shown_key = Id::new(("date-shown", salt));
    let mut changed = false;
    egui::Popup::from_toggle_button_response(&button)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut shown: NaiveDate = ui.data(|d| d.get_temp(shown_key)).unwrap_or(*day);
            ui.horizontal(|ui| {
                if ui.small_button("‹").clicked() {
                    shown = grid::step(View::Month, shown, -1);
                }
                ui.label(shown.format("%B %Y").to_string());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("›").clicked() {
                        shown = grid::step(View::Month, shown, 1);
                    }
                });
            });
            egui::Grid::new(("date-grid", salt)).spacing([2.0, 2.0]).show(ui, |ui| {
                for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
                    ui.label(RichText::new(name).small().color(theme::ink_3()));
                }
                ui.end_row();
                for (i, d) in grid::days(View::Month, shown).into_iter().enumerate() {
                    let color = if d == *day {
                        theme::bg()
                    } else if d.month() == shown.month() {
                        theme::ink()
                    } else {
                        theme::ink_3()
                    };
                    let mut b = egui::Button::new(RichText::new(d.day().to_string()).color(color))
                        .min_size(egui::vec2(28.0, 24.0))
                        .frame(d == *day);
                    if d == *day {
                        b = b.fill(theme::accent());
                    }
                    if ui.add(b).clicked() {
                        *day = d;
                        changed = true;
                    }
                    if i % 7 == 6 {
                        ui.end_row();
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(shown_key, shown));
        });
    if changed {
        egui::Popup::close_all(ui.ctx());
        ui.data_mut(|d| d.remove::<NaiveDate>(shown_key));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;

    #[test]
    fn time_boxes_follow_the_clock() {
        let start = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).unwrap().and_hms_opt(18, 0, 0).unwrap();
        let form = Form::new(start, start + Duration::hours(1), false, chrono_tz::UTC);
        let editor = Editor::new(form.clone(), None, Clock::H12);
        assert_eq!((editor.start_time.as_str(), editor.end_time.as_str()), ("6:00 PM", "7:00 PM"));
        let mut editor = Editor::new(form, None, Clock::H24);
        assert_eq!(editor.start_time, "18:00");
        editor.start_time = "6:30 PM".into();
        editor.read_times().unwrap();
        assert_eq!(editor.form.start.time(), chrono::NaiveTime::from_hms_opt(18, 30, 0).unwrap());
    }
}
