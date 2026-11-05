//! Compact date navigation and the independent today agenda.
use crate::{chrome, grid, grid::View, theme};
use calendio_core::{details::EventDetails, recur::Occurrence};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use chrono_tz::Tz;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense};

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
    /// The mini month: today in a red circle, the week on screen tinted (the
    /// day, in Day view). Returns a clicked day.
    pub fn show(&mut self, ui: &mut egui::Ui, view: View, anchor: NaiveDate, today: NaiveDate) -> Option<NaiveDate> {
        let width = ui.available_width();
        ui.horizontal(|ui| {
            let title = RichText::new(self.month.format("%B %Y").to_string()).font(theme::semibold(13.0));
            ui.label(title.color(theme::ink()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if chrome::glyph_button(ui, "›", 16.0, "Next month").clicked() {
                    self.month = grid::step(View::Month, self.month, 1);
                }
                if chrome::glyph_button(ui, "‹", 16.0, "Previous month").clicked() {
                    self.month = grid::step(View::Month, self.month, -1);
                }
            });
        });
        ui.add_space(4.0);

        let cell = (width / 7.0).floor();
        let (rect, _) = ui.allocate_exact_size(vec2(cell * 7.0, HEAD + ROW * 6.0), Sense::hover());
        let p = ui.painter();
        for (i, label) in ["M", "T", "W", "T", "F", "S", "S"].into_iter().enumerate() {
            let at = pos2(rect.left() + cell * (i as f32 + 0.5), rect.top() + HEAD / 2.0);
            p.text(at, Align2::CENTER_CENTER, label, FontId::proportional(10.5), theme::ink_3());
        }
        let days = month_days(self.month);
        let top = rect.top() + HEAD;
        if let Some(row) = tinted_row(&days, view, anchor) {
            let band =
                Rect::from_min_size(pos2(rect.left(), top + ROW * row as f32 + 1.0), vec2(cell * 7.0, ROW - 2.0));
            p.rect_filled(band, CornerRadius::same(6), theme::tint());
        }
        let mut chosen = None;
        for (i, &day) in days.iter().enumerate() {
            let centre = pos2(rect.left() + cell * ((i % 7) as f32 + 0.5), top + ROW * ((i / 7) as f32 + 0.5));
            let response = ui.interact(
                Rect::from_center_size(centre, vec2(cell, ROW)),
                ui.id().with(("mini-day", day)),
                Sense::click(),
            );
            let mut colour = if day.month() == self.month.month() { theme::ink() } else { theme::ink_3() };
            let mut font = FontId::proportional(12.0);
            if day == today {
                p.circle_filled(centre, 10.5, theme::today());
                (colour, font) = (Color32::WHITE, theme::semibold(12.0));
            } else if view == View::Day && day == anchor {
                p.circle_filled(centre, 10.5, theme::tint());
            } else if response.hovered() {
                p.circle_filled(centre, 10.5, theme::hover());
            }
            p.text(centre, Align2::CENTER_CENTER, day.day().to_string(), font, colour);
            if response.on_hover_text(day.format("%A %-d %B %Y").to_string()).clicked() {
                chosen = Some(day);
            }
        }
        chosen
    }
}

/// Weekday initials, then six weeks.
const HEAD: f32 = 18.0;
const ROW: f32 = 26.0;
/// The mini month's height, so the sidebar can pin it to the bottom.
pub const MINI_MONTH_H: f32 = 46.0 + HEAD + ROW * 6.0;

/// Which of the mini month's six rows to tint: the week on screen in Week view.
fn tinted_row(days: &[NaiveDate], view: View, anchor: NaiveDate) -> Option<usize> {
    (view == View::Week).then(|| days.iter().position(|&d| d == anchor)).flatten().map(|i| i / 7)
}

pub struct TodayItem {
    pub occurrence: Occurrence,
    pub details: EventDetails,
}

pub fn today_agenda(ui: &mut egui::Ui, items: &[TodayItem], now: NaiveDateTime, tz: Tz) -> Option<Occurrence> {
    let mut selected = None;
    ui.add_space(12.0);
    ui.heading("Today");
    ui.label(RichText::new(now.format("%A, %-d %B").to_string()).color(theme::ink_2()));
    ui.label(RichText::new(format!("{} events · {tz}", items.len())).small().color(theme::ink_3()));
    ui.add_space(12.0);
    if items.is_empty() {
        ui.label("Nothing scheduled today.");
        ui.label(RichText::new("Pick a time in your calendar to make a plan.").small().color(theme::ink_3()));
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
                .fill(theme::surface())
                .stroke(egui::Stroke::new(1.0, theme::line()))
                .corner_radius(8)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(times).small().color(theme::ink_3()));
                    if ui
                        .selectable_label(false, RichText::new(&event.title).strong())
                        .on_hover_text(&event.notes)
                        .clicked()
                    {
                        selected = Some(event.clone());
                    }
                    if !item.details.location.is_empty() {
                        ui.label(RichText::new(&item.details.location).color(theme::ink_2()));
                    }
                    ui.label(
                        RichText::new(if item.details.busy { "Busy" } else { "Free" }).small().color(theme::ink_3()),
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
    fn today_panel_uses_twelve_hour_times() {
        let context = egui::Context::default();
        theme::apply(&context);
        let now = NaiveDate::from_ymd_opt(2026, 1, 19).unwrap().and_hms_opt(8, 0, 0).unwrap();
        let items = [TodayItem {
            occurrence: Occurrence {
                event_id: 1,
                title: "Planning".into(),
                notes: String::new(),
                start: "2026-01-19T09:00:00Z".into(),
                end: "2026-01-19T10:00:00Z".into(),
                all_day: false,
                color: None,
                reminder_minutes: None,
                booking_id: None,
                original_start: None,
            },
            details: EventDetails::default(),
        }];
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(260.0, 780.0));
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut output = context.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                today_agenda(ui, &items, now, chrono_tz::UTC);
            });
            texts = crate::frames::text_rects(&output.shapes, screen);
            output.textures_delta.clear();
        }
        crate::frames::one(&texts, "9 – 10 AM");
    }

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
    fn only_the_week_on_screen_is_tinted() {
        let anchor = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let days = month_days(anchor);
        assert_eq!(tinted_row(&days, View::Week, anchor), Some(1), "26 Oct–1 Nov is row 0, so 5 Nov is row 1");
        assert_eq!(tinted_row(&days, View::Week, days[41]), Some(5));
        assert_eq!(tinted_row(&days, View::Month, anchor), None);
        assert_eq!(tinted_row(&days, View::Day, anchor), None);
        let elsewhere = chrono::NaiveDate::from_ymd_opt(2027, 3, 1).unwrap();
        assert_eq!(tinted_row(&days, View::Week, elsewhere), None);
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
                nav.show(ui, View::Week, day, day);
                used = ui.min_rect().width();
            },
        );
        output.textures_delta.clear();
        assert!(used <= 220.0, "Sidebar needs {used} pixels");
    }
}
