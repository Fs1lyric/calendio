//! Compact date navigation and the independent today agenda.
use crate::{grid, theme};
use calendio_core::{details::EventDetails, recur::Occurrence};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use chrono_tz::Tz;
use eframe::egui::{self, RichText};

fn month_days(anchor: NaiveDate) -> Vec<NaiveDate> {
    grid::days(grid::View::Month, anchor)
}

pub struct Navigation {
    month: NaiveDate,
}
impl Navigation {
    pub fn new(day: NaiveDate) -> Self {
        Self { month: day.with_day(1).expect("month has a first day") }
    }
    pub fn sync(&mut self, day: NaiveDate) {
        self.month = day.with_day(1).expect("month has a first day");
    }
    pub fn show(&mut self, ui: &mut egui::Ui, selected: NaiveDate, today: NaiveDate) -> Option<NaiveDate> {
        let mut chosen = None;
        ui.scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(2.0, 3.0);
            ui.spacing_mut().interact_size = egui::vec2(24.0, 24.0);
            ui.horizontal(|ui| {
                if ui.button("‹").on_hover_text("Previous month").clicked() {
                    self.month = grid::step(grid::View::Month, self.month, -1);
                }
                ui.label(RichText::new(self.month.format("%B %Y").to_string()).strong());
                if ui.button("›").on_hover_text("Next month").clicked() {
                    self.month = grid::step(grid::View::Month, self.month, 1);
                }
            });
            ui.add_space(8.0);
            let cell = ((ui.available_width() - 14.0) / 7.0).floor();
            egui::Grid::new("sidebar-month").num_columns(7).spacing([2.0, 2.0]).show(ui, |ui| {
                for label in ["M", "T", "W", "T", "F", "S", "S"] {
                    ui.add_sized([cell, 18.0], egui::Label::new(RichText::new(label).size(10.0).color(theme::INK_3)));
                }
                ui.end_row();
                for (index, day) in month_days(self.month).into_iter().enumerate() {
                    let mut label = RichText::new(day.day().to_string()).size(12.0);
                    if day.month() != self.month.month() {
                        label = label.color(theme::INK_3);
                    }
                    if day == today {
                        label = label.strong().color(theme::ACCENT);
                    }
                    if ui
                        .add_sized([cell, 25.0], egui::Button::new(label).selected(day == selected))
                        .on_hover_text(day.format("%A %-d %B %Y").to_string())
                        .clicked()
                    {
                        chosen = Some(day);
                    }
                    if index % 7 == 6 {
                        ui.end_row();
                    }
                }
            });
        });
        chosen
    }
}

pub struct TodayItem {
    pub occurrence: Occurrence,
    pub details: EventDetails,
}

pub fn today_agenda(ui: &mut egui::Ui, items: &[TodayItem], now: NaiveDateTime, tz: Tz) -> Option<Occurrence> {
    let mut selected = None;
    ui.add_space(12.0);
    ui.heading("Today");
    ui.label(RichText::new(now.format("%A, %-d %B").to_string()).color(theme::INK_2));
    ui.label(RichText::new(format!("{} events · {tz}", items.len())).small().color(theme::INK_3));
    ui.add_space(12.0);
    if items.is_empty() {
        ui.label("Nothing scheduled today.");
        ui.label(RichText::new("Pick a time in your calendar to make a plan.").small().color(theme::INK_3));
    }
    egui::ScrollArea::vertical().id_salt("today-agenda").show(ui, |ui| {
        for item in items {
            let event = &item.occurrence;
            let times = if event.all_day {
                "All day".into()
            } else {
                let end = grid::local(&event.end, tz);
                let end_format = if end.date() == now.date() { "%H:%M" } else { "%a %H:%M" };
                format!("{} – {}", grid::local(&event.start, tz).format("%H:%M"), end.format(end_format))
            };
            egui::Frame::new()
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .corner_radius(8)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(times).small().color(theme::INK_3));
                    if ui
                        .selectable_label(false, RichText::new(&event.title).strong())
                        .on_hover_text(&event.notes)
                        .clicked()
                    {
                        selected = Some(event.clone());
                    }
                    if !item.details.location.is_empty() {
                        ui.label(RichText::new(&item.details.location).color(theme::INK_2));
                    }
                    ui.label(
                        RichText::new(if item.details.busy { "Busy" } else { "Free" }).small().color(theme::INK_3),
                    );
                    if !item.details.meeting_url.is_empty() {
                        ui.hyperlink_to("Join meeting", &item.details.meeting_url);
                    }
                });
            ui.add_space(8.0);
        }
    });
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    #[test]
    fn mini_calendar_has_six_complete_monday_first_weeks() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let days = month_days(day);
        assert_eq!(days.len(), 42);
        assert_eq!(days[0].weekday(), chrono::Weekday::Mon);
        assert_eq!(days[0].to_string(), "2026-09-28");
        assert_eq!(days[41].to_string(), "2026-11-08");
    }
    #[test]
    fn compact_navigation_fits_the_sidebar() {
        let context = eframe::egui::Context::default();
        crate::theme::apply(&context);
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let mut nav = Navigation::new(day);
        let mut used = 0.0;
        let mut output = context.run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(220.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                nav.show(ui, day, day);
                used = ui.min_rect().width();
            },
        );
        output.textures_delta.clear();
        assert!(used <= 220.0, "Sidebar needs {used} pixels");
    }
}
