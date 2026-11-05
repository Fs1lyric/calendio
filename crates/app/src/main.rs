//! Calendio, the desktop calendar.

mod booking;
mod editor;
mod form;
mod grid;
mod navigation;
mod quick_add;
mod recovery_ui;
mod setup;
mod theme;
mod views;
mod workspace;

use calendio_core::db::Db;
use calendio_core::recur::Occurrence;
use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike, Utc};
use chrono_tz::Tz;
use editor::{accent_button, Editor, Outcome};
use eframe::egui::{self, Key, RichText};
use form::{Form, Save, Scope};
use grid::View;
use views::{Action, Scene};

struct App {
    db: Db,
    workspace: workspace::Workspace,
    quick_add: quick_add::QuickAdd,
    navigation: navigation::Navigation,
    today: Vec<navigation::TodayItem>,
    recovery: recovery_ui::RecoveryUi,
    theme: theme::Theme,
    tz: Tz,
    view: View,
    anchor: NaiveDate,
    occ: Vec<Occurrence>,
    stale: bool,
    loaded_at: std::time::Instant,
    scroll_to: Option<f32>,
    editor: Option<Editor>,
    /// The booking screen replaces the calendar while it is open.
    booking: Option<booking::Screen>,
    error: Option<String>,
}

impl App {
    fn new(db: Db, tz: Tz) -> Self {
        let workspace = workspace::Workspace::new(&db);
        let theme = theme::Theme::parse(&db.setting("theme").ok().flatten().unwrap_or_default());
        App {
            theme,
            db,
            workspace,
            quick_add: quick_add::QuickAdd::default(),
            navigation: navigation::Navigation::new(Utc::now().with_timezone(&tz).date_naive()),
            today: Vec::new(),
            recovery: recovery_ui::RecoveryUi::default(),
            tz,
            view: View::Week,
            anchor: Utc::now().with_timezone(&tz).date_naive(),
            occ: Vec::new(),
            stale: true,
            loaded_at: std::time::Instant::now(),
            scroll_to: Some(8.0 * views::HOUR_PX),
            editor: None,
            booking: None,
            error: None,
        }
    }

    fn now(&self) -> NaiveDateTime {
        Utc::now().with_timezone(&self.tz).naive_local()
    }

    /// Reloads after a change, and every 30 s so bookings made by the booker show up.
    fn reload(&mut self) {
        if !self.stale && self.loaded_at.elapsed().as_secs() < 30 {
            return;
        }
        let (from, to) = grid::window(&grid::days(self.view, self.anchor), self.tz);
        if let Err(e) = self.workspace.refresh(&self.db) {
            self.error = Some(e);
        }
        match self.db.visible_occurrences(&from.to_rfc3339(), &to.to_rfc3339()) {
            Ok(occ) => self.occ = occ,
            Err(e) => self.error = Some(e.to_string()),
        }
        let (from, to) = grid::window(&[self.now().date()], self.tz);
        match self.db.visible_occurrences(&from.to_rfc3339(), &to.to_rfc3339()).and_then(|items| {
            items
                .into_iter()
                .map(|occurrence| {
                    self.db
                        .event_details(occurrence.event_id)
                        .map(|details| navigation::TodayItem { occurrence, details })
                })
                .collect()
        }) {
            Ok(items) => self.today = items,
            Err(error) => self.error = Some(error.to_string()),
        }
        self.stale = false;
        self.loaded_at = std::time::Instant::now();
    }

    fn go(&mut self, view: View, anchor: NaiveDate) {
        self.booking = None;
        self.workspace.search_open = false;
        self.navigation.sync(anchor);
        if view != self.view && view != View::Month {
            self.scroll_to = Some(8.0 * views::HOUR_PX);
        }
        (self.view, self.anchor, self.stale) = (view, anchor, true);
    }

    fn open(&mut self, index: usize) {
        let occurrence = self.occ[index].clone();
        self.open_occurrence(&occurrence);
    }

    fn open_occurrence(&mut self, o: &Occurrence) {
        match self.db.event(o.event_id) {
            Ok(Some(e)) => self.editor = Some(self.make_editor(Form::open(o, &e, self.tz), Some(e))),
            Ok(None) => self.stale = true,
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn make_editor(&self, form: Form, series: Option<calendio_core::db::Event>) -> Editor {
        let calendar_id =
            series.as_ref().and_then(|e| self.db.event_calendar(e.id).ok()).unwrap_or(self.workspace.selected);
        let details = series.as_ref().map(|e| self.db.event_details(e.id));
        let mut editor = Editor::new(form, series);
        if let Some(details) = details {
            match details {
                Ok(details) => editor.details = details,
                Err(error) => editor.error = Some(error.to_string()),
            }
        }

        editor.calendar_id = calendar_id;
        editor.calendars = self.workspace.calendars.clone();
        editor
    }

    fn apply(&self, save: &Save, calendar_id: Option<i64>) -> calendio_core::db::Result<()> {
        match save {
            Save::Create(e) => self.db.create_event_in_calendar(e, calendar_id.unwrap_or(1)).map(drop),
            Save::Update(id, e) => self.db.update_event_in_calendar(*id, e, calendar_id),
            Save::Exception(x) => {
                if let Some(id) = calendar_id {
                    if id != self.db.event_calendar(x.event_id)? {
                        return Err(calendio_core::db::Error::Invalid(
                            "To move calendars, choose the entire series when saving".into(),
                        ));
                    }
                }
                self.db.set_exception(x)
            }
        }
    }

    fn save_editor(
        &self,
        save: &Save,
        calendar_id: i64,
        details: &calendio_core::details::EventDetails,
    ) -> calendio_core::db::Result<()> {
        let target = match save {
            Save::Create(_) => None,
            Save::Update(id, _) => Some(*id),
            Save::Exception(x) => Some(x.event_id),
        };
        self.db
            .record_edit("event change", target, |db| match save {
                Save::Create(event) => db.create_event_with_details(event, calendar_id, details),
                Save::Update(id, event) => {
                    db.update_event_with_details(*id, event, Some(calendar_id), Some(details))?;
                    Ok(*id)
                }
                Save::Exception(exception) => {
                    if details.clean()? != self.db.event_details(exception.event_id)? {
                        return Err(calendio_core::db::Error::Invalid(
                            "To change location, meeting link, or availability, choose the entire series".into(),
                        ));
                    }
                    self.apply(save, Some(calendar_id))?;
                    Ok(exception.event_id)
                }
            })
            .map(drop)
    }

    /// A drag drop: a single event moves, a series moves just this occurrence.
    fn drop_at(&mut self, index: usize, start: NaiveDateTime) {
        let o = &self.occ[index];
        let Ok(Some(e)) = self.db.event(o.event_id) else { return };
        let mut f = Form::open(o, &e, self.tz);
        let length = f.end - f.start;
        (f.start, f.end) = (start, start + length);
        let result = f.save(Scope::One, Some(&e)).map_err(|m| m.to_string());
        match result.and_then(|save| {
            self.db
                .record_edit("move event", Some(e.id), |_| {
                    self.apply(&save, None)?;
                    Ok(e.id)
                })
                .map(drop)
                .map_err(|e| e.to_string())
        }) {
            Ok(()) => self.stale = true,
            Err(e) => self.error = Some(e),
        }
    }

    /// Navigation and the period on the left, view switch and "New" on the
    /// right. Narrow windows put the two halves on separate rows.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let narrow = ui.available_width() < 760.0;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            if !narrow {
                ui.label(RichText::new("Calendio").font(theme::semibold(16.0)).color(theme::accent()));
                ui.add_space(12.0);
            }
            if self.booking.is_some() {
                ui.label(RichText::new("Booking").size(if narrow { 15.0 } else { 18.0 }));
            } else if ui.button("Today").clicked() {
                self.go(self.view, self.now().date());
            }
            if self.booking.is_none() {
                if ui.button("‹").clicked() {
                    self.go(self.view, grid::step(self.view, self.anchor, -1));
                }
                if ui.button("›").clicked() {
                    self.go(self.view, grid::step(self.view, self.anchor, 1));
                }
                let size = if narrow { 15.0 } else { 18.0 };
                let title = RichText::new(grid::title(self.view, self.anchor)).font(theme::bold(size));
                ui.add(egui::Label::new(title).truncate());
            }
            if !narrow {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.switcher(ui));
            }
        });
        if narrow {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.switcher(ui));
            });
        }
    }

    /// Right to left: "New", Booking, then Month / Week / Day.
    fn switcher(&mut self, ui: &mut egui::Ui) {
        if accent_button(ui, "+ New").clicked() {
            self.new_event();
        }
        ui.add_space(8.0);
        if ui.selectable_label(self.booking.is_some(), "Booking").clicked() {
            self.open_booking();
        }
        ui.add_space(8.0);
        if ui.available_width() < 600.0 {
            let label = match self.view {
                View::Month => "Month",
                View::Week => "Week",
                View::Day => "Day",
                View::Agenda => "Agenda",
                View::Year => "Year",
            };
            egui::ComboBox::from_id_salt("calendar-view").selected_text(label).show_ui(ui, |ui| {
                for (view, name) in [
                    (View::Month, "Month"),
                    (View::Week, "Week"),
                    (View::Day, "Day"),
                    (View::Agenda, "Agenda"),
                    (View::Year, "Year"),
                ] {
                    if ui.selectable_label(self.view == view && self.booking.is_none(), name).clicked() {
                        self.go(view, self.anchor);
                    }
                }
            });
            return;
        }
        for (v, name) in [
            (View::Year, "Year"),
            (View::Agenda, "Agenda"),
            (View::Day, "Day"),
            (View::Week, "Week"),
            (View::Month, "Month"),
        ] {
            if ui.selectable_label(self.view == v && self.booking.is_none(), name).clicked() {
                self.go(v, self.anchor);
            }
        }
    }

    fn open_booking(&mut self) {
        self.workspace.search_open = false;
        self.booking.get_or_insert_with(booking::Screen::new).reload();
    }

    fn new_event(&mut self) {
        // Today: the next full hour. Another day: nine o'clock.
        let now = self.now();
        let start = if self.anchor == now.date() {
            now.date().and_hms_opt(now.hour(), 0, 0).unwrap() + Duration::hours(1)
        } else {
            self.anchor.and_hms_opt(9, 0, 0).unwrap()
        };
        self.editor = Some(self.make_editor(Form::new(start, start + Duration::hours(1), false, self.tz), None));
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if self.editor.is_none()
            && !self.quick_add.is_open()
            && ctx.input(|i| i.modifiers.command && i.key_pressed(Key::K))
        {
            self.workspace.open_search();
            self.booking = None;
            return;
        }
        if self.quick_add.is_open() {
            return;
        }
        if self.editor.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let pressed = |k| ctx.input(|i| i.key_pressed(k));
        if pressed(Key::T) {
            self.go(self.view, self.now().date());
        }
        if pressed(Key::M) {
            self.go(View::Month, self.anchor);
        }
        if pressed(Key::W) {
            self.go(View::Week, self.anchor);
        }
        if pressed(Key::D) {
            self.go(View::Day, self.anchor);
        }
        if pressed(Key::N) {
            self.new_event();
        }
        if pressed(Key::Q) {
            self.quick_add.open();
        }
        if pressed(Key::B) {
            self.open_booking();
        }
        if pressed(Key::ArrowLeft) || pressed(Key::J) {
            self.go(self.view, grid::step(self.view, self.anchor, -1));
        }
        if pressed(Key::ArrowRight) || pressed(Key::K) {
            self.go(self.view, grid::step(self.view, self.anchor, 1));
        }
    }
}

impl App {
    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.theme.apply(&ctx);
        self.keys(&ctx);
        self.reload();
        ctx.request_repaint_after(std::time::Duration::from_secs(30));

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(8.0);
            self.top_bar(ui);
            if let Some(e) = self.error.clone() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(e).color(theme::danger()));
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            }
            ui.add_space(6.0);
            if self.workspace.toolbar(ui) {
                self.quick_add.open();
            }
            ui.horizontal_wrapped(|ui| {
                let label = self.db.undo_label().ok().flatten();
                if ui
                    .add_enabled(label.is_some(), egui::Button::new("Undo"))
                    .on_hover_text(label.unwrap_or_else(|| "Nothing to undo".into()))
                    .clicked()
                {
                    match self.db.undo() {
                        Ok(_) => self.stale = true,
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                let previous = self.theme;
                egui::ComboBox::from_id_salt("theme-picker")
                    .selected_text(format!("Theme: {}", self.theme.label()))
                    .show_ui(ui, |ui| {
                        for theme in theme::Theme::ALL {
                            ui.selectable_value(&mut self.theme, theme, theme.label());
                        }
                    });
                if self.theme != previous {
                    if let Err(error) = self.db.set_setting("theme", self.theme.id()) {
                        self.error = Some(error.to_string());
                    }
                    self.theme.apply(&ctx);
                    ctx.request_repaint();
                }
                if ui.button("Recovery & connections").clicked() {
                    self.recovery.open = true;
                }
            });
            ui.add_space(6.0);
        });
        if self.workspace.search_open {
            self.booking = None;
        }
        let show_today = ui.available_width() >= 1440.0;
        if ui.available_width() >= 940.0 {
            egui::Panel::left("navigation").resizable(false).default_size(232.0).show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("sidebar-scroll").show(ui, |ui| {
                    ui.add_space(20.0);
                    let today = self.now().date();
                    if let Some(day) = self.navigation.show(ui, self.anchor, today) {
                        self.go(if self.view == View::Year { View::Day } else { self.view }, day);
                    }
                    ui.add_space(20.0);
                    if ui.add_sized([180.0, 38.0], egui::Button::new("Create event")).clicked() {
                        self.new_event();
                    }
                    ui.add_space(20.0);
                    ui.label(RichText::new("CALENDAR").size(11.0).color(theme::ink_3()));
                    for (view, name) in [
                        (View::Year, "Year"),
                        (View::Agenda, "Agenda"),
                        (View::Day, "Day"),
                        (View::Week, "Week"),
                        (View::Month, "Month"),
                    ] {
                        if ui.selectable_label(self.booking.is_none() && self.view == view, name).clicked() {
                            self.go(view, self.anchor);
                        }
                    }
                    ui.add_space(20.0);
                    ui.label(RichText::new("SCHEDULING").size(11.0).color(theme::ink_3()));
                    if ui.selectable_label(self.booking.is_some(), "Booking links & hours").clicked() {
                        self.open_booking();
                    }
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.label(RichText::new("MY CALENDARS").size(11.0).color(theme::ink_3()));
                    match self.workspace.sidebar(ui, &self.db) {
                        Ok(changed) => self.stale |= changed,
                        Err(e) => self.error = Some(e),
                    }
                    ui.add_space(20.0);
                    ui.label(RichText::new("TIMEZONE").size(11.0).color(theme::ink_3()));
                    ui.label(self.tz.to_string());
                    ui.add_space(16.0);
                    ui.label(RichText::new("Stored on this device").small().color(theme::ink_3()));
                });
            });
        }
        if show_today && self.booking.is_none() && !self.workspace.search_open {
            let now = self.now();
            let selected = egui::Panel::right("today")
                .resizable(false)
                .default_size(260.0)
                .show(ui, |ui| navigation::today_agenda(ui, &self.today, now, self.tz))
                .inner;
            if let Some(occurrence) = selected {
                self.open_occurrence(&occurrence);
            }
        }
        if let Some(screen) = self.booking.as_mut() {
            let result = egui::CentralPanel::default().show(ui, |ui| screen.show(ui, &self.db, self.tz)).inner;
            match result {
                Ok(booking::Changed::Calendar) => self.stale = true,
                Ok(booking::Changed::Nothing) => {}
                Err(e) => self.error = Some(e),
            }
        } else if self.workspace.search_open {
            let found = egui::CentralPanel::default().show(ui, |ui| self.workspace.search(ui, &self.db, self.tz)).inner;
            if let Some((form, event)) = found {
                self.editor = Some(self.make_editor(form, Some(event)));
            }
        } else {
            self.calendar(ui);
        }
        if self.workspace.dialogs(&ctx, &self.db, self.tz) {
            self.stale = true;
        }
        if let Some(form) = self.quick_add.show(&ctx, self.now(), self.tz) {
            self.editor = Some(self.make_editor(form, None));
        }
        self.editor_frame(&ctx);
        if self.recovery.show(&ctx, &mut self.db) {
            self.stale = true;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

impl App {
    fn calendar(&mut self, ui: &mut egui::Ui) {
        let action = egui::CentralPanel::default()
            .show(ui, |ui| {
                let scene = Scene { occ: &self.occ, tz: self.tz, now: self.now() };
                match self.view {
                    View::Month => views::month(ui, &scene, self.anchor),
                    View::Agenda => views::agenda(ui, &scene),
                    View::Year => views::year(ui, &scene, self.anchor),
                    v => views::time_grid(ui, &grid::days(v, self.anchor), &scene, self.scroll_to.take()),
                }
            })
            .inner;

        match action {
            Some(Action::Open(k)) => self.open(k),
            Some(Action::New { start, end, all_day }) => {
                self.editor = Some(self.make_editor(Form::new(start, end, all_day, self.tz), None));
            }
            Some(Action::Move { index, start }) => self.drop_at(index, start),
            Some(Action::Goto(day)) => self.go(View::Day, day),
            None => {}
        }
    }

    fn editor_frame(&mut self, ctx: &egui::Context) {
        let Some(ed) = self.editor.as_mut() else { return };
        ed.calendars = self.workspace.calendars.clone();
        let outcome = ed.show(ctx);
        let calendar_id = ed.calendar_id;
        let details = ed.details.clone();
        let result = match &outcome {
            None => return,
            Some(Outcome::Close) => Ok(()),
            Some(Outcome::Save(s)) => self.save_editor(s, calendar_id, &details),
            Some(Outcome::DeleteEvent(id)) => self
                .db
                .record_edit("delete event", Some(*id), |db| {
                    db.delete_event(*id)?;
                    Ok(*id)
                })
                .map(drop),
            Some(Outcome::DeleteOne(x)) => self
                .db
                .record_edit("cancel occurrence", Some(x.event_id), |db| {
                    db.set_exception(x)?;
                    Ok(x.event_id)
                })
                .map(drop),
        };
        match result {
            Ok(()) => {
                self.editor = None;
                self.stale = true;
            }
            Err(e) => {
                if let Some(ed) = self.editor.as_mut() {
                    ed.error = Some(e.to_string());
                }
            }
        }
    }
}

const USAGE: &str = "usage: calendio [--view month|week|day|agenda|year] [--booking] [--new] [--quick-add] [--theme system|white|dark|catppuccin|tokyo-night|gruvbox|everforest|rose-pine|nord] [--recovery]";

/// Vsync stays off: on Wayland a vsynced swap waits for a frame callback, and Hyprland
/// sends none to a window on a hidden workspace, so the 30s repaint would block the
/// main thread there until the compositor reports Calendio as not responding.
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Calendio")
            .with_app_id("calendio")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([360.0, 320.0]),
        glow_options: eframe::egui_glow::GlowConfiguration { vsync: false, ..Default::default() },
        ..Default::default()
    }
}

fn main() -> eframe::Result {
    let mut view = View::Week;
    let mut quick_add = false;
    let mut quick_entry = false;
    let mut booking = false;
    let mut selected_theme = None;
    let mut recovery = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--view" => {
                view = match args.next().as_deref() {
                    Some("month") => View::Month,
                    Some("week") => View::Week,
                    Some("day") => View::Day,
                    Some("agenda") => View::Agenda,
                    Some("year") => View::Year,
                    _ => {
                        eprintln!("{USAGE}");
                        std::process::exit(2);
                    }
                }
            }
            "--new" => quick_add = true,
            "--quick-add" => quick_entry = true,
            "--booking" => booking = true,
            "--recovery" => recovery = true,
            "--theme" => {
                let value = args.next().unwrap_or_default();
                if !theme::Theme::ALL.iter().any(|t| t.id() == value) {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
                selected_theme = Some(theme::Theme::parse(&value));
            }
            _ => {
                eprintln!("{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let tz: Tz = iana_time_zone::get_timezone().ok().and_then(|z| z.parse().ok()).unwrap_or(Tz::UTC);
    let db = match Db::open(&Db::default_path()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("calendio: can't open the calendar: {e}");
            std::process::exit(1);
        }
    };
    eframe::run_native(
        "Calendio",
        native_options(),
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let mut app = App::new(db, tz);
            app.view = view;
            app.recovery.open = recovery;
            if let Some(theme) = selected_theme {
                app.theme = theme;
            }
            if booking {
                app.open_booking();
            }
            if quick_add {
                app.new_event();
            }
            if quick_entry {
                app.quick_add.open();
            }
            Ok(Box::new(app))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_theme_is_loaded_on_startup() {
        for theme in theme::Theme::ALL {
            let db = Db::open_in_memory().unwrap();
            db.set_setting("theme", theme.id()).unwrap();
            assert_eq!(App::new(db, chrono_tz::UTC).theme, theme);
        }
    }

    #[test]
    fn swaps_never_wait_on_the_compositor() {
        assert!(!super::native_options().glow_options.vsync);
    }
}

#[cfg(test)]
mod calendar_edit_tests {
    use super::*;
    #[test]
    fn moving_one_occurrence_does_not_silently_change_its_calendar() {
        let db = Db::open_in_memory().unwrap();
        let input = calendio_core::db::EventInput {
            title: "Daily planning".into(),
            notes: String::new(),
            start: "2026-10-19T09:00:00Z".into(),
            end: "2026-10-19T10:00:00Z".into(),
            all_day: false,
            tz: "UTC".into(),
            rrule: Some("FREQ=DAILY;COUNT=3".into()),
            color: None,
            reminder_minutes: None,
        };
        let id = db.create_event(&input).unwrap();
        let work = db.create_calendar("Work", "#2457b2").unwrap();
        let app = App::new(db, chrono_tz::UTC);
        let exception = calendio_core::db::Exception {
            event_id: id,
            original_start: "2026-10-20T09:00:00Z".into(),
            cancelled: false,
            title: Some("Changed".into()),
            notes: None,
            start: None,
            end: None,
        };
        assert!(app.apply(&Save::Exception(exception), Some(work)).is_err());
        assert!(app.db.exceptions_for(&[id]).unwrap().is_empty());
        assert_eq!(app.db.event_calendar(id).unwrap(), 1);
    }
}

#[cfg(test)]
mod improvement_tests {
    use super::*;
    use calendio_core::details::EventDetails;

    #[test]
    fn editor_saves_details_and_availability_with_the_event() {
        let app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let form = quick_add::parse("Planning tomorrow at 2pm", app.now(), chrono_tz::UTC).unwrap();
        let save = form.save(Scope::All, None).unwrap();
        let details = EventDetails {
            location: "North studio".into(),
            meeting_url: "https://meet.example.com/planning".into(),
            busy: false,
        };
        app.save_editor(&save, 1, &details).unwrap();
        let event = app.db.all_events().unwrap().pop().unwrap();
        assert_eq!(app.db.event_details(event.id).unwrap(), details);
        assert_eq!(app.db.search_events("north studio").unwrap()[0].id, event.id);
    }
    #[test]
    fn today_agenda_is_independent_of_the_browsed_month() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let now = app.now();
        let start = now.date().and_hms_opt(9, 0, 0).unwrap();
        let mut form = Form::new(start, start + Duration::hours(1), false, chrono_tz::UTC);
        form.title = "Today planning".into();
        app.apply(&form.save(Scope::All, None).unwrap(), Some(1)).unwrap();
        app.go(View::Month, now.date() + Duration::days(90));
        app.reload();
        assert!(app.occ.is_empty());
        assert_eq!(app.today.len(), 1);
        assert_eq!(app.today[0].occurrence.title, "Today planning");
    }
    #[test]
    fn changing_series_details_for_only_one_occurrence_is_rejected() {
        let app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let mut form = quick_add::parse("Planning today at 9am", app.now(), chrono_tz::UTC).unwrap();
        form.repeat = form::Repeat::Daily;
        app.apply(&form.save(Scope::All, None).unwrap(), Some(1)).unwrap();
        let event = app.db.all_events().unwrap().pop().unwrap();
        let exception = calendio_core::db::Exception {
            event_id: event.id,
            original_start: event.start.clone(),
            cancelled: false,
            title: None,
            notes: None,
            start: None,
            end: None,
        };
        assert!(app
            .save_editor(
                &Save::Exception(exception),
                1,
                &EventDetails { location: "Changed".into(), ..Default::default() }
            )
            .is_err());
        assert!(app.db.exceptions_for(&[event.id]).unwrap().is_empty());
    }
    #[test]
    fn complete_app_renders_calendar_editor_and_quick_add_at_supported_sizes() {
        for (width, height) in [(360.0, 320.0), (800.0, 780.0), (1904.0, 949.0)] {
            for state in 0..4 {
                let ctx = egui::Context::default();
                theme::apply(&ctx);
                let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
                if state == 1 {
                    app.new_event();
                }
                if state == 2 {
                    app.quick_add.open();
                }
                if state == 3 {
                    app.recovery.open = true;
                }
                for _ in 0..2 {
                    let mut used = 0.0;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
                            ..Default::default()
                        },
                        |ui| {
                            app.show(ui);
                            used = ui.min_rect().width();
                        },
                    );
                    assert!(!output.shapes.is_empty());
                    output.textures_delta.clear();
                    assert!(used <= width, "App uses {used} pixels in a {width}-pixel window");
                }
            }
        }
    }

    /// The text of every label whose centre is inside both its clip rect and the window.
    fn visible_texts(shapes: &[egui::epaint::ClippedShape], screen: egui::Rect) -> Vec<String> {
        fn walk(shape: &egui::Shape, view: egui::Rect, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, view, out)),
                egui::Shape::Text(text) if view.contains(shape.visual_bounding_rect().center()) => {
                    out.push(text.galley.text().to_owned())
                }
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, clipped.clip_rect.intersect(screen), &mut out);
        }
        out
    }

    #[test]
    fn editor_save_and_cancel_stay_on_screen_in_the_smallest_window() {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 320.0));
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        app.new_event();
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut output =
                ctx.run_ui(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| app.show(ui));
            output.textures_delta.clear();
            texts = visible_texts(&output.shapes, screen);
        }
        for label in ["Save", "Cancel"] {
            assert!(texts.iter().any(|t| t == label), "{label} is off screen in a 360x320 window; visible: {texts:?}");
        }
    }
}
