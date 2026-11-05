//! The month grid and the week/day time grid. They only paint and report what
//! the person did; the app decides what that means for the store.

use crate::grid::{self, View};
use crate::theme;
use calendio_core::recur::Occurrence;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use chrono_tz::Tz;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

pub const HOUR_PX: f32 = 48.0;
const GUTTER: f32 = 56.0;
const SNAP: i64 = 15;

pub enum Action {
    /// Open the occurrence at this index for editing.
    Open(usize),
    New {
        start: NaiveDateTime,
        end: NaiveDateTime,
        all_day: bool,
    },
    /// Drop the occurrence at this index so it starts at `start`.
    Move {
        index: usize,
        start: NaiveDateTime,
    },
    Goto(NaiveDate),
}

pub struct Scene<'a> {
    pub occ: &'a [Occurrence],
    pub tz: Tz,
    pub now: NaiveDateTime,
}

impl Scene<'_> {
    fn local_start(&self, o: &Occurrence) -> NaiveDateTime {
        if o.all_day {
            o.start.parse::<NaiveDate>().unwrap_or_default().and_time(NaiveTime::MIN)
        } else {
            grid::local(&o.start, self.tz)
        }
    }
}

fn at(day: NaiveDate, hour: u32) -> NaiveDateTime {
    day.and_hms_opt(hour, 0, 0).expect("valid hour")
}

fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

/// A rounded event chip with a coloured edge and clipped text. `notes` go under
/// the lines, wrapped to the chip's width.
fn chip(ui: &Ui, rect: Rect, color: Color32, lines: &[(String, f32, Color32)], notes: Option<&str>) {
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(rect, CornerRadius::same(4), theme::soft(color));
    p.rect_filled(Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), CornerRadius::same(2), color);
    let mut y = rect.top() + 2.0;
    for (text, size, c) in lines {
        p.text(Pos2::new(rect.left() + 7.0, y), Align2::LEFT_TOP, text, font(*size), *c);
        y += size + 3.0;
    }
    if let Some(notes) = notes {
        let galley = p.layout(notes.to_owned(), font(11.0), theme::ink_2(), (rect.width() - 10.0).max(1.0));
        p.galley(Pos2::new(rect.left() + 7.0, y), galley, theme::ink_2());
    }
}

/// The smallest time-grid block with room for a line of notes under its title
/// and time.
const NOTES_MIN_H: f32 = 44.0;

/// The notes to show on hover, if there are any.
fn hover_notes(notes: &str) -> Option<&str> {
    Some(notes.trim()).filter(|n| !n.is_empty())
}

/// The notes to print inside a time-grid block `height` pixels tall. Shorter
/// blocks leave them to the hover tooltip.
fn block_notes(notes: &str, height: f32) -> Option<&str> {
    hover_notes(notes).filter(|_| height >= NOTES_MIN_H)
}

/// Adds the event's notes to a chip's response as its hover tooltip.
fn with_notes(resp: egui::Response, notes: &str) -> egui::Response {
    match hover_notes(notes) {
        Some(n) => resp.on_hover_text(n),
        None => resp,
    }
}

// ---------- month ----------

pub fn month(ui: &mut Ui, s: &Scene, anchor: NaiveDate) -> Option<Action> {
    let mut action = None;
    let days = grid::days(View::Month, anchor);
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    let head = 26.0;
    let (w, h) = (full.width() / 7.0, (full.height() - head) / 6.0);
    let painter = ui.painter().clone();

    for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
        let x = full.left() + w * (i as f32 + 0.5);
        painter.text(Pos2::new(x, full.top() + head / 2.0), Align2::CENTER_CENTER, name, font(12.0), theme::ink_3());
    }

    for (i, &day) in days.iter().enumerate() {
        let (row, col) = (i / 7, i % 7);
        let cell = Rect::from_min_size(
            Pos2::new(full.left() + w * col as f32, full.top() + head + h * row as f32),
            egui::vec2(w, h),
        );
        let r = ui.interact(cell, Id::new(("cell", day)), Sense::click());
        let fill = if r.hovered() { theme::surface() } else { theme::bg() };
        painter.rect_filled(cell, 0.0, fill);
        painter.rect_stroke(cell, 0.0, Stroke::new(0.5, theme::line()), StrokeKind::Inside);
        if r.clicked() {
            action = Some(Action::New { start: at(day, 9), end: at(day, 10), all_day: false });
        }

        let today = day == s.now.date();
        let in_month = day.month() == anchor.month();
        let num_center = Pos2::new(cell.left() + 16.0, cell.top() + 14.0);
        let num_rect = Rect::from_center_size(num_center, egui::vec2(24.0, 22.0));
        if today {
            painter.circle_filled(num_center, 11.0, theme::today());
        }
        let num_color = if today {
            theme::bg()
        } else if in_month {
            theme::ink()
        } else {
            theme::ink_3()
        };
        painter.text(num_center, Align2::CENTER_CENTER, day.day(), font(13.0), num_color);
        if ui.interact(num_rect, Id::new(("num", day)), Sense::click()).on_hover_text("Open this day").clicked() {
            action = Some(Action::Goto(day));
        }

        let mut items: Vec<usize> = (0..s.occ.len())
            .filter(|&k| {
                let o = &s.occ[k];
                if o.all_day {
                    grid::covers(o, day)
                } else {
                    grid::span_on(o, day, s.tz).is_some()
                }
            })
            .collect();
        items.sort_by_key(|&k| (!s.occ[k].all_day, s.local_start(&s.occ[k])));

        let line = 19.0;
        let room = ((cell.height() - 30.0) / line).floor().max(1.0) as usize;
        let hidden = items.len().saturating_sub(room);
        for (n, &k) in items.iter().take(room).enumerate() {
            let o = &s.occ[k];
            let mut r = Rect::from_min_size(
                Pos2::new(cell.left() + 4.0, cell.top() + 28.0 + line * n as f32),
                egui::vec2(cell.width() - 8.0, line - 2.0),
            );
            // The last line makes room for "+n" when some don't fit.
            if hidden > 0 && n + 1 == room {
                let more = Rect::from_min_max(Pos2::new(r.right() - 30.0, r.top()), r.max);
                r.max.x -= 32.0;
                painter.text(more.center(), Align2::CENTER_CENTER, format!("+{hidden}"), font(12.0), theme::ink_2());
                let resp = ui.interact(more, Id::new(("more", day)), Sense::click());
                if resp.on_hover_text(format!("{hidden} more")).clicked() {
                    action = Some(Action::Goto(day));
                }
            }
            let color = theme::hex(o.color.as_deref());
            let text = if o.all_day {
                o.title.clone()
            } else {
                format!("{} {}", grid::local(&o.start, s.tz).format("%H:%M"), o.title)
            };
            chip(ui, r, color, &[(text, 12.0, theme::ink())], None);
            if with_notes(ui.interact(r, Id::new(("mchip", k, day)), Sense::click()), &o.notes).clicked() {
                action = Some(Action::Open(k));
            }
        }
    }
    action
}

// ---------- week and day ----------

/// A drag in progress, kept in egui's memory between frames.
#[derive(Clone, Copy)]
enum Drag {
    Create { day: usize, from: i64 },
    Move { index: usize, grab: i64 },
}

pub fn time_grid(ui: &mut Ui, days: &[NaiveDate], s: &Scene, scroll_to: Option<f32>) -> Option<Action> {
    let mut action = None;
    let full = ui.available_rect_before_wrap();
    let col_w = (full.width() - GUTTER) / days.len() as f32;
    let col_x = |i: usize| full.left() + GUTTER + col_w * i as f32;

    // Day headers.
    let head = 46.0;
    let (head_rect, _) = ui.allocate_exact_size(egui::vec2(full.width(), head), Sense::hover());
    for (i, &day) in days.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(col_x(i), head_rect.top()), egui::vec2(col_w, head));
        let today = day == s.now.date();
        let p = ui.painter();
        p.text(
            Pos2::new(r.center().x, r.top() + 10.0),
            Align2::CENTER_CENTER,
            day.format("%a").to_string().to_uppercase(),
            font(11.0),
            if today { theme::today() } else { theme::ink_3() },
        );
        let c = Pos2::new(r.center().x, r.top() + 30.0);
        if today {
            p.circle_filled(c, 13.0, theme::today());
        }
        p.text(c, Align2::CENTER_CENTER, day.day(), font(17.0), if today { theme::bg() } else { theme::ink() });
        if days.len() > 1 && ui.interact(r, Id::new(("head", day)), Sense::click()).clicked() {
            action = Some(Action::Goto(day));
        }
    }

    // All-day lane: each all-day occurrence spans its visible days.
    let first = days[0];
    let all_day: Vec<(usize, i64, i64)> = s
        .occ
        .iter()
        .enumerate()
        .filter(|(_, o)| o.all_day)
        .filter_map(|(k, o)| {
            let a = (o.start.parse::<NaiveDate>().ok()? - first).num_days().max(0);
            let b = (o.end.parse::<NaiveDate>().ok()? - first).num_days().min(days.len() as i64);
            (a < b).then_some((k, a, b))
        })
        .collect();
    let lanes = grid::pack(&all_day.iter().map(|&(_, a, b)| (a, b)).collect::<Vec<_>>());
    let lane_count = lanes.iter().map(|p| p.column + 1).max().unwrap_or(0).max(1);
    let lane_h = 22.0;
    let (lane_rect, _) =
        ui.allocate_exact_size(egui::vec2(full.width(), lane_count as f32 * lane_h + 8.0), Sense::hover());
    ui.painter().text(
        Pos2::new(lane_rect.left() + GUTTER - 8.0, lane_rect.top() + 14.0),
        Align2::RIGHT_CENTER,
        "all day",
        font(11.0),
        theme::ink_3(),
    );
    for (i, &day) in days.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(col_x(i), lane_rect.top()), egui::vec2(col_w, lane_rect.height()));
        ui.painter().rect_stroke(r, 0.0, Stroke::new(0.5, theme::line()), StrokeKind::Inside);
        if ui.interact(r, Id::new(("lane", day)), Sense::click()).clicked() {
            action = Some(Action::New { start: at(day, 0), end: at(day, 0), all_day: true });
        }
    }
    for (&(k, a, b), lane) in all_day.iter().zip(&lanes) {
        let r = Rect::from_min_max(
            Pos2::new(col_x(a as usize) + 2.0, lane_rect.top() + 4.0 + lane.column as f32 * lane_h),
            Pos2::new(col_x(b as usize) - 2.0, lane_rect.top() + 2.0 + (lane.column + 1) as f32 * lane_h),
        );
        let o = &s.occ[k];
        chip(ui, r, theme::hex(o.color.as_deref()), &[(o.title.clone(), 12.0, theme::ink())], None);
        if with_notes(ui.interact(r, Id::new(("achip", k)), Sense::click()), &o.notes).clicked() {
            action = Some(Action::Open(k));
        }
    }

    // The hours.
    let mut area = egui::ScrollArea::vertical().id_salt("timegrid").auto_shrink([false, false]);
    if let Some(y) = scroll_to {
        area = area.vertical_scroll_offset(y);
    }
    area.show(ui, |ui| {
        let (body, _) = ui.allocate_exact_size(egui::vec2(full.width(), 24.0 * HOUR_PX), Sense::hover());
        let col = |i: usize| Rect::from_min_size(Pos2::new(col_x(i), body.top()), egui::vec2(col_w, body.height()));
        let p = ui.painter().clone();
        for h in 0..24 {
            let y = body.top() + h as f32 * HOUR_PX;
            p.line_segment(
                [Pos2::new(body.left() + GUTTER, y), Pos2::new(body.right(), y)],
                Stroke::new(0.5, theme::line()),
            );
            if h > 0 {
                p.text(
                    Pos2::new(body.left() + GUTTER - 8.0, y),
                    Align2::RIGHT_CENTER,
                    format!("{h:02}:00"),
                    font(11.0),
                    theme::ink_3(),
                );
            }
        }
        let pointer = ui.ctx().pointer_interact_pos();
        let day_at = |pos: Pos2| ((pos.x - col_x(0)) / col_w).floor().clamp(0.0, days.len() as f32 - 1.0) as usize;
        let minute_at = |pos: Pos2| grid::minute_at(pos.y - body.top(), HOUR_PX, SNAP);
        let drag_id = Id::new("grid-drag");
        let drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id));

        // Empty space: click for an hour, drag to sweep out a span.
        for (i, &day) in days.iter().enumerate() {
            let rect = col(i);
            p.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(0.5, theme::line()));
            let r = ui.interact(rect, Id::new(("col", day)), Sense::click_and_drag());
            if r.clicked() {
                if let Some(pos) = r.interact_pointer_pos() {
                    let m = grid::minute_at(pos.y - body.top(), HOUR_PX, 30);
                    let start = day.and_time(NaiveTime::MIN) + Duration::minutes(m);
                    action = Some(Action::New { start, end: start + Duration::hours(1), all_day: false });
                }
            }
            if r.drag_started() {
                if let Some(pos) = r.interact_pointer_pos() {
                    ui.data_mut(|d| d.insert_temp(drag_id, Drag::Create { day: i, from: minute_at(pos) }));
                }
            }
        }
        if let (Some(Drag::Create { day, from }), Some(pos)) = (drag, pointer) {
            let to = minute_at(pos);
            let (a, b) = (from.min(to), from.max(to) + SNAP);
            let ghost = Rect::from_min_max(
                Pos2::new(col(day).left() + 2.0, body.top() + a as f32 / 60.0 * HOUR_PX),
                Pos2::new(col(day).right() - 4.0, body.top() + b as f32 / 60.0 * HOUR_PX),
            );
            p.rect_filled(ghost, CornerRadius::same(4), theme::soft(theme::accent()));
            p.rect_stroke(ghost, CornerRadius::same(4), Stroke::new(1.0, theme::accent()), StrokeKind::Inside);
            let label = format!("{} – {}", hm(a), hm(b));
            p.text(ghost.left_top() + egui::vec2(6.0, 4.0), Align2::LEFT_TOP, label, font(11.0), theme::ink());
            if ui.input(|i| i.pointer.any_released()) {
                let midnight = days[day].and_time(NaiveTime::MIN);
                action = Some(Action::New {
                    start: midnight + Duration::minutes(a),
                    end: midnight + Duration::minutes(b),
                    all_day: false,
                });
                ui.data_mut(|d| d.remove::<Drag>(drag_id));
            }
        }

        // Events, packed side by side where they overlap.
        for (i, &day) in days.iter().enumerate() {
            let todays: Vec<(usize, (i64, i64))> =
                (0..s.occ.len()).filter_map(|k| grid::span_on(&s.occ[k], day, s.tz).map(|sp| (k, sp))).collect();
            let placed = grid::pack(&todays.iter().map(|t| t.1).collect::<Vec<_>>());
            for (&(k, (a, b)), pl) in todays.iter().zip(&placed) {
                let o = &s.occ[k];
                let w = col_w / pl.columns as f32;
                let r = Rect::from_min_size(
                    Pos2::new(col(i).left() + w * pl.column as f32 + 1.0, body.top() + a as f32 / 60.0 * HOUR_PX + 1.0),
                    egui::vec2(w - 4.0, ((b - a) as f32 / 60.0 * HOUR_PX - 2.0).max(16.0)),
                );
                let moving = matches!(drag, Some(Drag::Move { index, .. }) if index == k);
                let color = theme::hex(o.color.as_deref());
                let ink = if moving { theme::ink_3() } else { theme::ink() };
                let start = grid::local(&o.start, s.tz);
                let end = grid::local(&o.end, s.tz);
                let mut lines = vec![(o.title.clone(), 13.0, ink)];
                if r.height() > 34.0 {
                    lines.push((format!("{} – {}", start.format("%H:%M"), end.format("%H:%M")), 11.0, theme::ink_2()));
                }
                chip(ui, r, color, &lines, block_notes(&o.notes, r.height()));
                let resp = with_notes(ui.interact(r, Id::new(("ev", k, day)), Sense::click_and_drag()), &o.notes);
                if resp.clicked() {
                    action = Some(Action::Open(k));
                }
                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let under = day.and_time(NaiveTime::MIN) + Duration::minutes(minute_at(pos));
                        let grab = (under - start).num_minutes();
                        ui.data_mut(|d| d.insert_temp(drag_id, Drag::Move { index: k, grab }));
                    }
                }
            }
        }
        if let (Some(Drag::Move { index, grab }), Some(pos)) = (drag, pointer) {
            let o = &s.occ[index];
            let length = grid::local(&o.end, s.tz) - grid::local(&o.start, s.tz);
            let d = day_at(pos);
            let raw = days[d].and_time(NaiveTime::MIN) + Duration::minutes(minute_at(pos) - grab);
            let start = snap_time(raw);
            let a = (start - days[d].and_time(NaiveTime::MIN)).num_minutes();
            let top = body.top() + a as f32 / 60.0 * HOUR_PX;
            let ghost = Rect::from_min_size(
                Pos2::new(col(d).left() + 1.0, top),
                egui::vec2(col_w - 4.0, (length.num_minutes() as f32 / 60.0 * HOUR_PX).max(16.0)),
            );
            p.rect_stroke(
                ghost,
                CornerRadius::same(4),
                Stroke::new(1.5, theme::hex(o.color.as_deref())),
                StrokeKind::Inside,
            );
            p.text(
                ghost.left_top() + egui::vec2(7.0, 3.0),
                Align2::LEFT_TOP,
                start.format("%H:%M"),
                font(11.0),
                theme::ink(),
            );
            if ui.input(|i| i.pointer.any_released()) {
                if start != grid::local(&o.start, s.tz) {
                    action = Some(Action::Move { index, start });
                }
                ui.data_mut(|d| d.remove::<Drag>(drag_id));
            }
        }

        // Now.
        if let Some(i) = days.iter().position(|&d| d == s.now.date()) {
            let y = body.top() + (s.now.hour() * 60 + s.now.minute()) as f32 / 60.0 * HOUR_PX;
            let rect = col(i);
            p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.5, theme::today()));
            p.circle_filled(Pos2::new(rect.left(), y), 4.0, theme::today());
        }
    });
    action
}

fn hm(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn snap_time(t: NaiveDateTime) -> NaiveDateTime {
    let m = (t.hour() * 60 + t.minute()) as i64;
    t.date().and_time(NaiveTime::MIN) + Duration::minutes(m.div_euclid(SNAP) * SNAP)
}

/// Chronological, keyboard-focusable agenda for the next thirty days.
pub fn agenda(ui: &mut Ui, s: &Scene) -> Option<Action> {
    let mut action = None;
    if s.occ.is_empty() {
        ui.add_space(40.0);
        ui.heading("A little room in your calendar.");
        ui.label("No events in this period. Create an event or import a calendar.");
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut previous = String::new();
        for (index, o) in s.occ.iter().enumerate() {
            let start = s.local_start(o);
            let day = start.format("%A, %-d %B").to_string();
            if day != previous {
                ui.add_space(18.0);
                ui.label(egui::RichText::new(&day).strong().size(18.0));
                ui.separator();
                previous = day;
            }
            let time = if o.all_day {
                "All day".into()
            } else {
                format!("{} – {}", start.format("%H:%M"), grid::local(&o.end, s.tz).format("%H:%M"))
            };
            if ui
                .add_sized(
                    [ui.available_width(), 54.0],
                    egui::Button::new(format!("{time}    {}", o.title))
                        .fill(theme::soft(theme::hex(o.color.as_deref()))),
                )
                .on_hover_text(&o.notes)
                .clicked()
            {
                action = Some(Action::Open(index));
            }
        }
    });
    action
}

fn year_busy_days(scene: &Scene, anchor: NaiveDate) -> std::collections::HashSet<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(anchor.year(), 1, 1).expect("valid year");
    let last = NaiveDate::from_ymd_opt(anchor.year(), 12, 31).expect("valid year");
    let mut busy = std::collections::HashSet::new();
    for occurrence in scene.occ {
        let start = scene.local_start(occurrence).date().max(first);
        let end = if occurrence.all_day {
            occurrence.end.parse::<NaiveDate>().ok().and_then(|d| d.pred_opt())
        } else {
            grid::local(&occurrence.end, scene.tz).checked_sub_signed(Duration::nanoseconds(1)).map(|t| t.date())
        };
        if let Some(end) = end {
            for day in start.iter_days().take_while(|day| *day <= end.min(last)) {
                busy.insert(day);
            }
        }
    }
    busy
}

/// Year overview. Each day is a real focusable button; a dot marks busy days.
pub fn year(ui: &mut Ui, s: &Scene, anchor: NaiveDate) -> Option<Action> {
    let columns = (((ui.available_width() - 14.0) / 240.0) as usize).clamp(1, 4);
    let card_width = ((ui.available_width() - 14.0 - 18.0 * (columns - 1) as f32) / columns as f32 - 24.0).max(198.0);
    let cell_width = (card_width - 12.0) / 7.0;
    let mut action = None;
    let busy = year_busy_days(s, anchor);
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("year-grid").num_columns(columns).spacing([18.0, 20.0]).show(ui, |ui| {
            for month in 1..=12 {
                let first = NaiveDate::from_ymd_opt(anchor.year(), month, 1).expect("valid month");
                egui::Frame::new().fill(theme::surface()).inner_margin(12.0).corner_radius(8).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(card_width);
                        ui.spacing_mut().button_padding = egui::vec2(2.0, 2.0);
                        ui.spacing_mut().interact_size = egui::vec2(24.0, 24.0);
                        ui.label(egui::RichText::new(first.format("%B").to_string()).strong().size(17.0));
                        egui::Grid::new(("mini-month", month)).num_columns(7).spacing([2.0, 2.0]).show(ui, |ui| {
                            for name in ["M", "T", "W", "T", "F", "S", "S"] {
                                ui.label(egui::RichText::new(name).small().color(theme::ink_3()));
                            }
                            ui.end_row();
                            let start = grid::week_start(first);
                            for i in 0..42 {
                                let day = start + Duration::days(i);
                                if day.month() == month {
                                    let has_events = busy.contains(&day);
                                    let text =
                                        if has_events { format!("{}·", day.day()) } else { day.day().to_string() };
                                    let button =
                                        egui::Button::new(egui::RichText::new(text).size(if cell_width > 36.0 {
                                            14.0
                                        } else {
                                            12.0
                                        }))
                                        .selected(day == s.now.date());
                                    if ui
                                        .add_sized([cell_width, 25.0], button)
                                        .on_hover_text(format!(
                                            "{}{}",
                                            day,
                                            if has_events { " · Has events" } else { "" }
                                        ))
                                        .clicked()
                                    {
                                        action = Some(Action::Goto(day));
                                    }
                                } else {
                                    ui.allocate_space(egui::vec2(cell_width, 25.0));
                                }
                                if i % 7 == 6 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                });
                if (month as usize).is_multiple_of(columns) {
                    ui.end_row();
                }
            }
        });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn year_marks_every_busy_day_and_respects_exclusive_midnight_ends() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let make = |start: &str, end: &str, all_day| Occurrence {
            event_id: 1,
            title: "Away".into(),
            notes: String::new(),
            start: start.into(),
            end: end.into(),
            all_day,
            color: None,
            reminder_minutes: None,
            booking_id: None,
            original_start: None,
        };
        let occurrences =
            [make("2026-10-19T23:00:00Z", "2026-10-21T00:00:00Z", false), make("2026-10-23", "2026-10-25", true)];
        let scene = Scene { occ: &occurrences, tz: chrono_tz::UTC, now: day.and_hms_opt(9, 0, 0).unwrap() };
        let busy = year_busy_days(&scene, day);
        for n in [19, 20, 23, 24] {
            assert!(busy.contains(&NaiveDate::from_ymd_opt(2026, 10, n).unwrap()));
        }
        for n in [21, 22, 25] {
            assert!(!busy.contains(&NaiveDate::from_ymd_opt(2026, 10, n).unwrap()));
        }
    }

    #[test]
    fn year_cards_fit_inside_the_calendar_width() {
        for width in [360.0, 800.0, 1180.0] {
            let context = egui::Context::default();
            theme::apply(&context);
            let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
            let scene = Scene { occ: &[], tz: chrono_tz::UTC, now: day.and_hms_opt(9, 0, 0).unwrap() };
            let mut used = 0.0;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 780.0))),
                    ..Default::default()
                },
                |ui| {
                    year(ui, &scene, day);
                    used = ui.min_rect().width();
                },
            );
            output.textures_delta.clear();
            assert!(used <= width, "Year view uses {used} pixels inside a {width}-pixel window");
        }
    }

    #[test]
    fn every_calendar_view_renders_at_desktop_and_narrow_sizes() {
        for width in [360.0, 800.0, 1180.0] {
            for view in [grid::View::Year, grid::View::Agenda, grid::View::Month, grid::View::Week, grid::View::Day] {
                let context = egui::Context::default();
                theme::apply(&context);
                let day = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
                let scene = Scene { occ: &[], tz: chrono_tz::UTC, now: day.and_hms_opt(9, 0, 0).unwrap() };
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 780.0))),
                        ..Default::default()
                    },
                    |ui| match view {
                        grid::View::Year => {
                            year(ui, &scene, day);
                        }
                        grid::View::Agenda => {
                            agenda(ui, &scene);
                        }
                        grid::View::Month => {
                            month(ui, &scene, day);
                        }
                        v => {
                            time_grid(ui, &grid::days(v, day), &scene, None);
                        }
                    },
                );
                assert!(!output.shapes.is_empty(), "View must paint content");
                output.textures_delta.clear();
            }
        }
    }

    #[test]
    fn an_hour_block_shows_its_notes() {
        assert_eq!(block_notes("Ice wash face", HOUR_PX - 2.0), Some("Ice wash face"));
    }

    #[test]
    fn notes_are_trimmed_and_blank_notes_show_nothing() {
        assert_eq!(block_notes("get ready\n", HOUR_PX), Some("get ready"));
        assert_eq!(block_notes(" \n", HOUR_PX), None);
    }

    #[test]
    fn a_half_hour_block_leaves_notes_to_the_tooltip() {
        assert_eq!(block_notes("watch reels", HOUR_PX / 2.0 - 2.0), None);
        assert_eq!(hover_notes("watch reels\n"), Some("watch reels"));
    }
}
