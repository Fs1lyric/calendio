//! The month grid and the week/day time grid. They only paint and report what
//! the person did; the app decides what that means for the store.

use crate::clock::Clock;
use crate::grid::{self, View};
use crate::theme;
use calendio_core::recur::Occurrence;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use chrono_tz::Tz;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};

pub const HOUR_PX: f32 = 48.0;
const GUTTER: f32 = 56.0;
/// Space above midnight and below 24:00 inside the scrolling hours.
const PAD: f32 = 12.0;
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
    /// Give the event behind this occurrence a new colour; `None` is its calendar's.
    Recolour {
        index: usize,
        colour: Option<String>,
    },
}

pub struct Scene<'a> {
    pub occ: &'a [Occurrence],
    pub tz: Tz,
    pub now: NaiveDateTime,
    pub clock: Clock,
    /// The occurrence open in the editor, as (event id, start stamp).
    pub selected: Option<(i64, String)>,
}

impl Scene<'_> {
    fn is_selected(&self, o: &Occurrence) -> bool {
        self.selected.as_ref().is_some_and(|(id, start)| *id == o.event_id && *start == o.start)
    }

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

/// An Apple Calendar event block: the tinted fill, a 3 px bar in the calendar
/// colour, a semibold title in a darker shade of it with the time underneath,
/// and `notes` wrapped below. The selected block is solid with white text; a
/// block being dragged is faded.
struct Block<'a> {
    title: &'a str,
    time: Option<String>,
    notes: Option<&'a str>,
    selected: bool,
    faded: bool,
}

fn event_block(ui: &Ui, rect: Rect, color: Color32, b: Block) {
    let c = theme::event_colors(color, b.selected);
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(rect, CornerRadius::same(5), c.fill);
    let bar = CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 };
    p.rect_filled(Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), bar, c.bar);
    let ink = if b.faded { c.title.gamma_multiply(0.55) } else { c.title };
    let x = rect.left() + 8.0;
    let title = p.layout_no_wrap(b.title.to_owned(), theme::semibold(12.0), ink);
    let mut y = rect.top() + 3.0 + title.size().y;
    p.galley(Pos2::new(x, rect.top() + 3.0), title, ink);
    if let Some(time) = b.time {
        let time = p.layout_no_wrap(time, font(11.0), ink);
        let height = time.size().y;
        p.galley(Pos2::new(x, y), time, ink);
        y += height;
    }
    if let Some(notes) = b.notes {
        let galley = p.layout(notes.to_owned(), font(11.0), ink, (rect.width() - 11.0).max(1.0));
        p.galley(Pos2::new(x, y + 2.0), galley, ink);
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
/// Right-clicking a block opens Apple's colour menu; picking one recolours it.
fn colour_menu(resp: &egui::Response, o: &Occurrence, index: usize, action: &mut Option<Action>) {
    resp.context_menu(|ui| {
        if let Some(colour) = crate::swatches::menu(ui, o.color.as_deref()) {
            *action = Some(Action::Recolour { index, colour });
        }
    });
}

fn with_notes(resp: egui::Response, notes: &str) -> egui::Response {
    match hover_notes(notes) {
        Some(n) => resp.on_hover_text(n),
        None => resp,
    }
}

// ---------- month ----------

/// Apple's month: the date top-right (today white in a red circle), timed
/// events as "● 9 AM Title" lines, all-day events as solid pills, and
/// "+2 more" when a day runs out of room.
pub fn month(ui: &mut Ui, s: &Scene, anchor: NaiveDate) -> Option<Action> {
    let mut action = None;
    let days = grid::days(View::Month, anchor);
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    let head = 26.0;
    let (w, h) = (full.width() / 7.0, (full.height() - head) / 6.0);
    let painter = ui.painter().clone();

    for (i, name) in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].iter().enumerate() {
        let x = full.left() + w * (i as f32 + 1.0) - 10.0;
        painter.text(Pos2::new(x, full.top() + head / 2.0), Align2::RIGHT_CENTER, name, font(12.0), theme::ink_2());
    }

    for (i, &day) in days.iter().enumerate() {
        let (row, col) = (i / 7, i % 7);
        let cell = Rect::from_min_size(
            Pos2::new(full.left() + w * col as f32, full.top() + head + h * row as f32),
            egui::vec2(w, h),
        );
        let r = ui.interact(cell, Id::new(("cell", day)), Sense::click());
        if r.hovered() {
            painter.rect_filled(cell, 0.0, theme::hover());
        }
        // One hairline above each week and between days, never doubled.
        let ppp = ui.ctx().pixels_per_point();
        let crisp = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
        let hairline = Stroke::new(1.0 / ppp, theme::line());
        let top = crisp(cell.top());
        painter.line_segment([Pos2::new(cell.left(), top), Pos2::new(cell.right(), top)], hairline);
        if col > 0 {
            let x = crisp(cell.left());
            painter.line_segment([Pos2::new(x, cell.top()), Pos2::new(x, cell.bottom())], hairline);
        }
        if r.clicked() {
            action = Some(Action::New { start: at(day, 9), end: at(day, 10), all_day: false });
        }

        let today = day == s.now.date();
        let in_month = day.month() == anchor.month();
        let num_center = Pos2::new(cell.right() - 17.0, cell.top() + 15.0);
        let (num_color, num_font) = if today {
            painter.circle_filled(num_center, 11.0, theme::today());
            (Color32::WHITE, theme::semibold(13.0))
        } else if in_month {
            (theme::ink(), font(13.0))
        } else {
            (theme::ink_3(), font(13.0))
        };
        painter.text(num_center, Align2::CENTER_CENTER, day.day(), num_font, num_color);
        let num_rect = Rect::from_center_size(num_center, egui::vec2(26.0, 24.0));
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

        let line = 18.0;
        let top = cell.top() + 28.0;
        let room = (((cell.bottom() - top - 2.0) / line).floor() as usize).max(1);
        let (shown, more) = if items.len() > room { (room - 1, items.len() - (room - 1)) } else { (items.len(), 0) };
        let slot = |n: usize| {
            Rect::from_min_size(
                Pos2::new(cell.left() + 4.0, top + line * n as f32),
                egui::vec2(cell.width() - 8.0, line - 2.0),
            )
        };
        for (n, &k) in items.iter().take(shown).enumerate() {
            let o = &s.occ[k];
            let r = slot(n);
            let color = theme::hex(o.color.as_deref());
            let clip = painter.with_clip_rect(r.intersect(ui.clip_rect()));
            if o.all_day || s.is_selected(o) {
                let c = theme::event_colors(color, true);
                clip.rect_filled(r, CornerRadius::same(4), c.fill);
                let text = if o.all_day { o.title.clone() } else { timed_label(s, o) };
                clip.text(Pos2::new(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, text, font(12.0), c.title);
            } else {
                clip.circle_filled(Pos2::new(r.left() + 5.0, r.center().y), 3.5, color);
                let start = s.clock.short(grid::local(&o.start, s.tz).time());
                let mut job = egui::text::LayoutJob::default();
                let section = |color| egui::TextFormat { font_id: font(12.0), color, ..Default::default() };
                // A phone-width cell has no room for both; the title matters more.
                if cell.width() >= 90.0 {
                    job.append(&format!("{start} "), 0.0, section(theme::ink_2()));
                }
                job.append(&o.title, 0.0, section(theme::ink()));
                let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
                let at = Pos2::new(r.left() + 13.0, r.center().y - galley.size().y / 2.0);
                clip.galley(at, galley, theme::ink());
            }
            let resp = with_notes(ui.interact(r, Id::new(("mchip", k, day)), Sense::click()), &o.notes);
            if resp.clicked() {
                action = Some(Action::Open(k));
            }
            colour_menu(&resp, o, k, &mut action);
        }
        if more > 0 {
            let r = slot(shown);
            let label = format!("+{more} more");
            painter.text(
                Pos2::new(r.left() + 6.0, r.center().y),
                Align2::LEFT_CENTER,
                label,
                font(12.0),
                theme::ink_2(),
            );
            if ui.interact(r, Id::new(("more", day)), Sense::click()).on_hover_text("Open this day").clicked() {
                action = Some(Action::Goto(day));
            }
        }
    }
    action
}

/// "9 AM Title" for a timed event in the month grid.
fn timed_label(s: &Scene, o: &Occurrence) -> String {
    format!("{} {}", s.clock.short(grid::local(&o.start, s.tz).time()), o.title)
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

    // Day headers: "Mon 19", today's number white in a red circle. Columns
    // too narrow for one line (a phone-width week) stack the weekday on top.
    let head = 36.0;
    let stacked = col_w < 72.0;
    let (head_rect, _) = ui.allocate_exact_size(egui::vec2(full.width(), head), Sense::hover());
    for (i, &day) in days.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(col_x(i), head_rect.top()), egui::vec2(col_w, head));
        let today = day == s.now.date();
        let (name_color, number_color) =
            if today { (theme::today(), Color32::WHITE) } else { (theme::ink_2(), theme::ink()) };
        let name_font = font(if stacked { 11.0 } else { 13.0 });
        let name = ui.ctx().fonts_mut(|f| f.layout_no_wrap(day.format("%a").to_string(), name_font, name_color));
        let number_font = if today { theme::semibold(13.0) } else { font(13.0) };
        let number = ui.ctx().fonts_mut(|f| f.layout_no_wrap(day.day().to_string(), number_font, number_color));
        let p = ui.painter();
        let circle = if stacked { 20.0 } else { 22.0 };
        let c = if stacked {
            p.galley(Pos2::new(r.center().x - name.size().x / 2.0, r.top() + 2.0), name, name_color);
            Pos2::new(r.center().x, r.top() + 25.0)
        } else {
            // Only today's number needs room for its circle.
            let slot = if today { circle } else { number.size().x };
            let gap = if today { 3.0 } else { 5.0 };
            let width = name.size().x + gap + slot;
            let left = r.center().x - width / 2.0;
            p.galley(Pos2::new(left, r.center().y - name.size().y / 2.0), name, name_color);
            Pos2::new(left + width - slot / 2.0, r.center().y)
        };
        if today {
            p.circle_filled(c, circle / 2.0, theme::today());
        }
        p.galley(c - number.size() / 2.0, number, number_color);
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
        "all-day",
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
        let block = Block { title: &o.title, time: None, notes: None, selected: s.is_selected(o), faded: false };
        event_block(ui, r, theme::hex(o.color.as_deref()), block);
        let resp = with_notes(ui.interact(r, Id::new(("achip", k)), Sense::click()), &o.notes);
        if resp.clicked() {
            action = Some(Action::Open(k));
        }
        colour_menu(&resp, o, k, &mut action);
    }

    // The hours.
    let mut area = egui::ScrollArea::vertical().id_salt("timegrid").auto_shrink([false, false]);
    if let Some(y) = scroll_to {
        area = area.vertical_scroll_offset(y);
    }
    area.show(ui, |ui| {
        // Padding above midnight and below the last hour keeps their labels whole.
        let (outer, _) = ui.allocate_exact_size(egui::vec2(full.width(), 24.0 * HOUR_PX + 2.0 * PAD), Sense::hover());
        let body = Rect::from_min_size(outer.min + egui::vec2(0.0, PAD), egui::vec2(full.width(), 24.0 * HOUR_PX));
        let col = |i: usize| Rect::from_min_size(Pos2::new(col_x(i), body.top()), egui::vec2(col_w, body.height()));
        let p = ui.painter().clone();
        let ppp = ui.ctx().pixels_per_point();
        let hairline = |y: f32| ((y * ppp).floor() + 0.5) / ppp;
        let now_y = days
            .contains(&s.now.date())
            .then(|| body.top() + (s.now.hour() * 60 + s.now.minute()) as f32 / 60.0 * HOUR_PX);
        let (left, right) = (body.left() + GUTTER, body.right());
        for h in 0..24 {
            let y = body.top() + h as f32 * HOUR_PX;
            let (line, half) = (hairline(y), hairline(y + HOUR_PX / 2.0));
            p.line_segment([Pos2::new(left, line), Pos2::new(right, line)], Stroke::new(1.0 / ppp, theme::line()));
            let faint = theme::line().gamma_multiply(0.45);
            p.line_segment([Pos2::new(left, half), Pos2::new(right, half)], Stroke::new(1.0 / ppp, faint));
            let near_now = now_y.is_some_and(|n| (n - y).abs() < 18.0);
            if h > 0 && !near_now {
                p.text(
                    Pos2::new(left - 8.0, y),
                    Align2::RIGHT_CENTER,
                    s.clock.hour(h as u32),
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
            let x = ((rect.left() * ppp).floor() + 0.5) / ppp;
            p.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0 / ppp, theme::line()),
            );
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
            let label = s.clock.range(clock_time(a), clock_time(b));
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
                let start = grid::local(&o.start, s.tz);
                let end = grid::local(&o.end, s.tz);
                let block = Block {
                    title: &o.title,
                    time: (r.height() > 30.0).then(|| s.clock.range(start.time(), end.time())),
                    notes: block_notes(&o.notes, r.height()),
                    selected: s.is_selected(o),
                    faded: moving,
                };
                event_block(ui, r, color, block);
                let resp = with_notes(ui.interact(r, Id::new(("ev", k, day)), Sense::click_and_drag()), &o.notes);
                if resp.clicked() {
                    action = Some(Action::Open(k));
                }
                colour_menu(&resp, o, k, &mut action);
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
                s.clock.short(start.time()),
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

        // Now: a red line across today with the time in a red pill in the gutter.
        if let (Some(i), Some(y)) = (days.iter().position(|&d| d == s.now.date()), now_y) {
            let rect = col(i);
            p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.5, theme::today()));
            p.circle_filled(Pos2::new(rect.left(), y), 4.0, theme::today());
            let label = s.clock.bare(s.now.time());
            let text = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label, theme::semibold(11.0), Color32::WHITE));
            let size = text.size() + egui::vec2(10.0, 4.0);
            let pill = Rect::from_center_size(Pos2::new(left - 5.0 - size.x / 2.0, y), size);
            p.rect_filled(pill, CornerRadius::same((size.y / 2.0) as u8), theme::today());
            p.galley(pill.center() - text.size() / 2.0, text, Color32::WHITE);
        }
    });
    action
}

/// Minutes after midnight as a time of day; 24:00 reads as midnight.
fn clock_time(minutes: i64) -> NaiveTime {
    NaiveTime::MIN + Duration::minutes(minutes)
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
                s.clock.range(start.time(), grid::local(&o.end, s.tz).time())
            };
            let resp = ui
                .add_sized(
                    [ui.available_width(), 54.0],
                    egui::Button::new(format!("{time}    {}", o.title))
                        .fill(theme::soft(theme::hex(o.color.as_deref()))),
                )
                .on_hover_text(&o.notes);
            if resp.clicked() {
                action = Some(Action::Open(index));
            }
            colour_menu(&resp, o, index, &mut action);
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
    use crate::clock::Clock;

    fn overview_frame(occ: &[Occurrence], year_view: bool) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        let context = egui::Context::default();
        theme::Theme::AppleLight.apply(&context);
        let now = NaiveDate::from_ymd_opt(2026, 1, 19).unwrap().and_hms_opt(9, 0, 0).unwrap();
        let scene = Scene { occ, tz: chrono_tz::UTC, now, clock: Clock::H12, selected: None };
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 780.0));
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let mut output = context.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
                if year_view {
                    year(ui, &scene, now.date());
                } else {
                    agenda(ui, &scene);
                }
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        (shapes, screen)
    }

    #[test]
    fn year_uses_plain_day_numbers_and_a_red_today_circle() {
        let (shapes, screen) = overview_frame(&[], true);
        let texts = crate::frames::text_rects(&shapes, screen);
        let today = crate::frames::find(&texts, "19").into_iter().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        assert!(crate::frames::circles(&shapes)
            .iter()
            .any(|c| c.fill == theme::today() && c.center.distance(today.center()) < 3.0));
        assert_eq!(crate::frames::text_color(&shapes, "19", today.center()), Some(Color32::WHITE));
        let ordinary =
            crate::frames::find(&texts, "20").into_iter().min_by(|a, b| a.top().total_cmp(&b.top())).unwrap();
        assert!(
            !crate::frames::rects(&shapes)
                .iter()
                .any(|r| r.fill == theme::surface_2() && r.rect.contains(ordinary.center())),
            "day numbers have no grey button tiles"
        );
    }

    #[test]
    fn agenda_aligns_titles_left_times_right_and_leads_with_calendar_colour() {
        let blue = Color32::from_rgb(0x24, 0x57, 0xb2);
        let occ = [event(1, "Planning", "2026-01-19T09:00:00Z", "2026-01-19T10:00:00Z", "#2457b2")];
        let (shapes, screen) = overview_frame(&occ, false);
        let texts = crate::frames::text_rects(&shapes, screen);
        let title = crate::frames::one(&texts, "Planning");
        let time = crate::frames::one(&texts, "9 – 10 AM");
        assert!(title.left() < 50.0 && time.right() > 1100.0);
        assert!((title.center().y - time.center().y).abs() < 3.0);
        assert!(crate::frames::circles(&shapes)
            .iter()
            .any(|c| c.fill == blue && c.center.x < title.left() && (c.center.y - title.center().y).abs() < 3.0));
    }

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
        let scene = Scene {
            occ: &occurrences,
            tz: chrono_tz::UTC,
            now: day.and_hms_opt(9, 0, 0).unwrap(),
            clock: Clock::H12,
            selected: None,
        };
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
            let scene = Scene {
                occ: &[],
                tz: chrono_tz::UTC,
                now: day.and_hms_opt(9, 0, 0).unwrap(),
                clock: Clock::H12,
                selected: None,
            };
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
                let scene = Scene {
                    occ: &[],
                    tz: chrono_tz::UTC,
                    now: day.and_hms_opt(9, 0, 0).unwrap(),
                    clock: Clock::H12,
                    selected: None,
                };
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

    /// A week (19–25 Oct 2026) with "now" on Wednesday at 10:30, scrolled to 8 AM.
    fn week_frame(clock: Clock) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        week_with(&[], None, clock)
    }

    fn week_with(
        occ: &[Occurrence],
        selected: Option<(i64, String)>,
        clock: Clock,
    ) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        week_at(occ, selected, clock, 1180.0)
    }

    #[test]
    fn right_clicking_a_block_offers_every_colour_and_picking_one_recolours_it() {
        let occ = [event(7, "Standup", "2026-10-21T09:00:00Z", "2026-10-21T10:00:00Z", "#007aff")];
        let context = egui::Context::default();
        theme::Theme::AppleLight.apply(&context);
        let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(10, 30, 0).unwrap();
        let scene = Scene { occ: &occ, tz: chrono_tz::UTC, now, clock: Clock::H24, selected: None };
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1180.0, 780.0));
        let mut frame = 0;
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut action = None;
            let scroll = (frame == 0).then_some(8.0 * HOUR_PX);
            frame += 1;
            let mut output = context.run_ui(input, |ui| {
                action = time_grid(ui, &grid::days(View::Week, monday), &scene, scroll);
            });
            output.textures_delta.clear();
            (output.shapes, action)
        };
        let mut shapes = Vec::new();
        for _ in 0..3 {
            shapes = run(Vec::new()).0;
        }
        let block = crate::frames::text_rects(&shapes, screen)
            .into_iter()
            .find(|(t, _)| t == "Standup")
            .expect("the block is on screen")
            .1;
        let click = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let at = block.center();
        run(vec![egui::Event::PointerMoved(at)]);
        run(vec![click(at, egui::PointerButton::Secondary, true)]);
        run(vec![click(at, egui::PointerButton::Secondary, false)]);
        let shapes = run(Vec::new()).0;
        let texts = crate::frames::text_rects(&shapes, screen);
        for (name, _) in theme::SWATCHES {
            assert!(texts.iter().any(|(t, _)| t == name), "the menu lists {name}");
        }
        let blue = crate::frames::one(&texts, "Blue");
        let checked = crate::frames::flat(&shapes).into_iter().any(|(_, s)| match s {
            egui::Shape::Path(p) => p.points.iter().all(|q| (q.y - blue.center().y).abs() < 8.0 && q.x < blue.left()),
            _ => false,
        });
        assert!(checked, "a check sits left of the current colour, Blue");
        let orange = crate::frames::one(&texts, "Orange").center();
        run(vec![egui::Event::PointerMoved(orange)]);
        run(vec![click(orange, egui::PointerButton::Primary, true)]);
        let (_, action) = run(vec![click(orange, egui::PointerButton::Primary, false)]);
        match action {
            Some(Action::Recolour { index: 0, colour }) => assert_eq!(colour.as_deref(), Some("#ff9500")),
            _ => panic!("picking Orange recolours the block"),
        }
    }

    fn week_at(
        occ: &[Occurrence],
        selected: Option<(i64, String)>,
        clock: Clock,
        width: f32,
    ) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        let context = egui::Context::default();
        theme::Theme::AppleLight.apply(&context);
        let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(10, 30, 0).unwrap();
        let scene = Scene { occ, tz: chrono_tz::UTC, now, clock, selected };
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 780.0));
        let mut shapes = Vec::new();
        for frame in 0..3 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut output = context.run_ui(input, |ui| {
                let scroll = (frame == 0).then_some(8.0 * HOUR_PX);
                time_grid(ui, &grid::days(View::Week, monday), &scene, scroll);
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        (shapes, screen)
    }

    fn is_hour_label(text: &str) -> bool {
        text.ends_with(" AM") || text.ends_with(" PM") || text == "Noon" || text.ends_with(":00")
    }

    #[test]
    fn week_header_reads_mon_19_with_today_in_a_red_circle() {
        let (shapes, screen) = week_frame(Clock::H12);
        let texts = crate::frames::text_rects(&shapes, screen);
        let head = |label: &str| -> Vec<Rect> {
            texts.iter().filter(|(t, r)| t == label && r.top() < 50.0).map(|(_, r)| *r).collect()
        };
        for (name, number) in [("Mon", "19"), ("Wed", "21"), ("Sun", "25")] {
            let (n, d) = (head(name), head(number));
            assert_eq!((n.len(), d.len()), (1, 1), "one {name} {number} header in {texts:?}");
            assert!((n[0].center().y - d[0].center().y).abs() < 3.0, "{name} {number} is one line");
            assert!(d[0].left() > n[0].right(), "the number follows the weekday");
        }
        assert!(head("MON").is_empty(), "weekdays are not shouted");
        let today = head("21")[0];
        let red = crate::frames::circles(&shapes).into_iter().filter(|c| c.fill == theme::today()).collect::<Vec<_>>();
        assert!(red.iter().any(|c| c.center.distance(today.center()) < 3.0), "today's number sits in a red circle");
        assert_eq!(crate::frames::text_color(&shapes, "21", today.center()), Some(Color32::WHITE));
    }

    #[test]
    fn a_phone_width_week_header_stacks_the_weekday_over_the_date() {
        for width in [360.0, 390.0] {
            let (shapes, screen) = week_at(&[], None, Clock::H12, width);
            let head: Vec<(String, Rect)> =
                crate::frames::text_rects(&shapes, screen).into_iter().filter(|(_, r)| r.top() < 36.0).collect();
            assert_eq!(head.len(), 14, "seven weekdays and seven dates at {width} px: {head:?}");
            for (i, (a, ra)) in head.iter().enumerate() {
                for (b, rb) in &head[i + 1..] {
                    assert!(!ra.expand(1.0).intersects(*rb), "{a} and {b} collide at {width} px");
                }
            }
            let column = (width - GUTTER) / 7.0;
            for (_, rect) in &head {
                let i = ((rect.center().x - GUTTER) / column).floor();
                assert!(rect.left() >= GUTTER + i * column && rect.right() <= GUTTER + (i + 1.0) * column);
            }
        }
    }

    #[test]
    fn hour_labels_follow_the_clock_and_the_first_one_is_whole() {
        let (shapes, screen) = week_frame(Clock::H12);
        let texts = crate::frames::text_rects(&shapes, screen);
        for label in ["8 AM", "9 AM", "Noon", "1 PM"] {
            assert!(texts.iter().any(|(t, _)| t == label), "{label} is in the gutter: {texts:?}");
        }
        assert!(!texts.iter().any(|(t, _)| t == "09:00"));
        for (clip, shape) in crate::frames::flat(&shapes) {
            if let egui::Shape::Text(text) = &shape {
                let rect = shape.visual_bounding_rect();
                let straddles_top = rect.top() < clip.top() && rect.bottom() > clip.top();
                assert!(
                    !(is_hour_label(text.galley.text()) && straddles_top),
                    "{} is cut in half at the top of the hours",
                    text.galley.text()
                );
            }
        }
        let (shapes, screen) = week_frame(Clock::H24);
        let texts = crate::frames::text_rects(&shapes, screen);
        for label in ["09:00", "13:00"] {
            assert!(texts.iter().any(|(t, _)| t == label), "{label} in a 24-hour gutter");
        }
        assert!(!texts.iter().any(|(t, _)| t == "9 AM"));
    }

    #[test]
    fn hour_lines_are_hairlines_and_half_hours_are_fainter() {
        let (shapes, screen) = week_frame(Clock::H12);
        let texts = crate::frames::text_rects(&shapes, screen);
        let y = |label: &str| texts.iter().find(|(t, _)| t == label).map(|(_, r)| r.center().y).unwrap();
        let (nine, ten) = (y("9 AM"), y("10 AM"));
        let lines: Vec<(f32, Stroke)> = crate::frames::flat(&shapes)
            .into_iter()
            .filter_map(|(_, s)| match s {
                egui::Shape::LineSegment { points: [a, b], stroke }
                    if (a.y - b.y).abs() < 0.01 && (a.x - b.x).abs() > 500.0 =>
                {
                    Some((a.y, stroke))
                }
                _ => None,
            })
            .collect();
        // Label rects are glyph ink, a few points off the line they are centred on;
        // lines are 24 points apart, so 6 is unambiguous.
        let near = |target: f32| lines.iter().find(|(y, _)| (y - target).abs() < 6.0).map(|(_, s)| *s);
        let hour = near(nine).expect("an hour line at 9 AM");
        let half = near((nine + ten) / 2.0).expect("a half-hour line at 9:30");
        assert!(hour.width <= 1.0, "hour lines are hairlines");
        assert_eq!(hour.color, theme::line());
        assert!(half.color.a() < hour.color.a(), "half-hour lines are fainter than hour lines");
    }

    #[test]
    fn the_now_line_crosses_only_today_with_a_red_time_pill_in_the_gutter() {
        let (shapes, screen) = week_frame(Clock::H12);
        let texts = crate::frames::text_rects(&shapes, screen);
        let pill_text = texts.iter().find(|(t, _)| t == "10:30").map(|(_, r)| *r).expect("the time of now");
        assert!(pill_text.right() <= GUTTER, "the time sits in the gutter");
        assert_eq!(crate::frames::text_color(&shapes, "10:30", pill_text.center()), Some(Color32::WHITE));
        let pill = crate::frames::rects(&shapes)
            .into_iter()
            .find(|r| r.fill == theme::today() && r.rect.contains(pill_text.center()));
        assert!(pill.is_some(), "a red pill behind the time");
        let col_w = (1180.0 - GUTTER) / 7.0;
        let red: Vec<[Pos2; 2]> = crate::frames::flat(&shapes)
            .into_iter()
            .filter_map(|(_, s)| match s {
                egui::Shape::LineSegment { points, stroke } if stroke.color == theme::today() => Some(points),
                _ => None,
            })
            .collect();
        assert_eq!(red.len(), 1, "one now line");
        let [a, b] = red[0];
        assert!((a.x - (GUTTER + 2.0 * col_w)).abs() < 2.0 && (b.x - (GUTTER + 3.0 * col_w)).abs() < 2.0);
        assert!((a.y - pill_text.center().y).abs() < 2.0, "the pill sits on the line");
    }

    fn planning(colour: &str) -> Occurrence {
        Occurrence {
            event_id: 7,
            title: "Planning".into(),
            notes: String::new(),
            start: "2026-10-22T09:00:00Z".into(),
            end: "2026-10-22T10:00:00Z".into(),
            all_day: false,
            color: Some(colour.into()),
            reminder_minutes: None,
            booking_id: None,
            original_start: None,
        }
    }

    /// The painted rectangle with `fill` that holds the label's centre.
    fn block_under(shapes: &[egui::epaint::ClippedShape], fill: Color32, at: Pos2) -> egui::epaint::RectShape {
        crate::frames::rects(shapes)
            .into_iter()
            .find(|r| r.fill == fill && r.rect.contains(at))
            .unwrap_or_else(|| panic!("no block filled {fill:?} under {at:?}"))
    }

    #[test]
    fn event_blocks_have_a_tinted_fill_a_left_bar_and_a_semibold_title_over_the_time() {
        let blue = Color32::from_rgb(0x24, 0x57, 0xb2);
        let (shapes, screen) = week_with(&[planning("#2457b2")], None, Clock::H12);
        let colours = theme::event_colors(blue, false);
        let texts = crate::frames::text_rects(&shapes, screen);
        let title = crate::frames::one(&texts, "Planning");
        let time = crate::frames::one(&texts, "9 – 10 AM");
        assert!(time.top() >= title.bottom() - 1.0 && (time.left() - title.left()).abs() < 1.0, "time under title");
        let block = block_under(&shapes, colours.fill, title.center());
        assert_eq!(block.corner_radius, CornerRadius::same(5));
        let bar = crate::frames::rects(&shapes).into_iter().find(|r| {
            r.fill == blue && (r.rect.width() - 3.0).abs() < 0.01 && (r.rect.left() - block.rect.left()).abs() < 0.5
        });
        let bar = bar.expect("a 3 px bar in the calendar colour on the block's left edge");
        assert!((bar.rect.height() - block.rect.height()).abs() < 0.5, "the bar runs the block's height");
        assert_eq!(crate::frames::text_color(&shapes, "Planning", title.center()), Some(colours.title));
        let font = crate::frames::text_font(&shapes, "Planning", title.center()).unwrap();
        assert_eq!(font.family, egui::FontFamily::Name("semibold".into()), "the title is semibold");
    }

    #[test]
    fn the_selected_event_is_solid_with_white_text() {
        let blue = Color32::from_rgb(0x24, 0x57, 0xb2);
        let selected = Some((7, "2026-10-22T09:00:00Z".to_owned()));
        let (shapes, screen) = week_with(&[planning("#2457b2")], selected, Clock::H12);
        let title = crate::frames::one(&crate::frames::text_rects(&shapes, screen), "Planning");
        block_under(&shapes, blue, title.center());
        assert_eq!(crate::frames::text_color(&shapes, "Planning", title.center()), Some(Color32::WHITE));
    }

    fn event(id: i64, title: &str, start: &str, end: &str, colour: &str) -> Occurrence {
        Occurrence {
            event_id: id,
            title: title.into(),
            notes: String::new(),
            start: start.into(),
            end: end.into(),
            all_day: !start.contains('T'),
            color: Some(colour.into()),
            reminder_minutes: None,
            booking_id: None,
            original_start: None,
        }
    }

    /// October 2026 at 1180x780, "now" on Wednesday 21 at 10:30.
    fn month_frame(occ: &[Occurrence]) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        month_at(occ, 1180.0)
    }

    fn month_at(occ: &[Occurrence], width: f32) -> (Vec<egui::epaint::ClippedShape>, Rect) {
        let context = egui::Context::default();
        theme::Theme::AppleLight.apply(&context);
        let now = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap().and_hms_opt(10, 30, 0).unwrap();
        let scene = Scene { occ, tz: chrono_tz::UTC, now, clock: Clock::H12, selected: None };
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 780.0));
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut output = context.run_ui(input, |ui| {
                month(ui, &scene, now.date());
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        (shapes, screen)
    }

    #[test]
    fn month_cells_put_the_date_top_right_and_list_timed_events_as_dot_time_title() {
        let blue = Color32::from_rgb(0x24, 0x57, 0xb2);
        let occ = [event(1, "Planning", "2026-10-22T09:00:00Z", "2026-10-22T10:00:00Z", "#2457b2")];
        let (shapes, screen) = month_frame(&occ);
        let texts = crate::frames::text_rects(&shapes, screen);
        let column = 1180.0 / 7.0;
        // 22 Oct is a Thursday: the fourth column.
        let (left, right) = (3.0 * column, 4.0 * column);
        let line = crate::frames::one(&texts, "9 AM Planning");
        assert!(line.left() > left && line.left() < left + 30.0, "the event line starts at the cell's left");
        let dot = crate::frames::circles(&shapes)
            .into_iter()
            .find(|c| c.fill == blue && c.center.x < line.left() && (c.center.y - line.center().y).abs() < 3.0);
        assert!(dot.is_some(), "a dot in the calendar colour leads the line");
        let date = texts
            .iter()
            .find(|(t, r)| t == "22" && r.center().x > left && r.center().x < right)
            .map(|(_, r)| *r)
            .expect("the date");
        assert!(date.right() > right - 18.0 && date.right() < right, "the date sits at the cell's right");
        assert!(date.bottom() < line.top(), "the date is above the events");

        let today = texts
            .iter()
            .find(|(t, r)| t == "21" && r.center().x > 2.0 * column && r.center().x < left)
            .map(|(_, r)| *r)
            .expect("today's date");
        assert!(today.right() > left - 24.0, "today's date is top-right too");
        let red = crate::frames::circles(&shapes).into_iter().find(|c| c.fill == theme::today());
        assert!(red.is_some_and(|c| c.center.distance(today.center()) < 3.0), "today is in a red circle");
        assert_eq!(crate::frames::text_color(&shapes, "21", today.center()), Some(Color32::WHITE));
        assert!(crate::frames::find(&texts, "Mon").len() == 1, "weekday names head the columns");
    }

    #[test]
    fn phone_width_month_cells_drop_the_time_so_the_title_shows() {
        let occ = [event(1, "Planning", "2026-10-22T09:00:00Z", "2026-10-22T10:00:00Z", "#2457b2")];
        let (shapes, screen) = month_at(&occ, 390.0);
        let texts = crate::frames::text_rects(&shapes, screen);
        crate::frames::one(&texts, "Planning");
        assert!(crate::frames::find(&texts, "9 AM Planning").is_empty());
    }

    #[test]
    fn month_all_day_events_are_solid_pills() {
        let purple = Color32::from_rgb(0xaf, 0x52, 0xde);
        let (shapes, screen) = month_frame(&[event(2, "Trip", "2026-10-23", "2026-10-25", "#af52de")]);
        let texts = crate::frames::text_rects(&shapes, screen);
        let trips = crate::frames::find(&texts, "Trip");
        assert_eq!(trips.len(), 1, "one joined pill labels the trip once");
        for trip in trips {
            let pill =
                crate::frames::rects(&shapes).into_iter().find(|r| r.fill == purple && r.rect.contains(trip.center()));
            assert!(
                pill.is_some_and(|p| p.rect.width() > screen.width() / 7.0 * 1.9),
                "a joined solid pill spans both days"
            );
            let ink = crate::frames::text_color(&shapes, "Trip", trip.center());
            assert_eq!(ink, Some(theme::event_colors(purple, true).title));
        }
    }

    #[test]
    fn a_full_month_cell_ends_with_n_more() {
        let occ: Vec<Occurrence> = (0..6)
            .map(|i| {
                let start = format!("2026-10-27T{:02}:00:00Z", 8 + i);
                let end = format!("2026-10-27T{:02}:30:00Z", 8 + i);
                event(10 + i, &format!("Busy {i}"), &start, &end, "#ff9500")
            })
            .collect();
        let (shapes, screen) = month_frame(&occ);
        let texts = crate::frames::text_rects(&shapes, screen);
        let shown = texts.iter().filter(|(t, _)| t.contains(" Busy ")).count();
        assert!((1..6).contains(&shown), "{shown} of 6 events fit");
        let more = format!("+{} more", 6 - shown);
        crate::frames::one(&texts, &more);
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
