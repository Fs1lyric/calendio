//! Calendio, the desktop calendar.

mod booking;
mod chrome;
mod clock;
mod editor;
mod form;
mod grid;
mod navigation;
mod quick_add;
mod recovery_ui;
mod setup;
mod swatches;
mod theme;
mod views;
mod workspace;

use calendio_core::db::Db;
use calendio_core::recur::Occurrence;
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;
use editor::{Editor, Outcome};
use eframe::egui::{self, Key, RichText};
use form::{Form, Save, Scope};
use grid::View;
use views::{Action, Scene};

/// The toolbar's view switch, in Apple Calendar's order. Agenda lives in the `…` menu.
const VIEWS: [(View, &str); 4] =
    [(View::Day, "Day"), (View::Week, "Week"), (View::Month, "Month"), (View::Year, "Year")];

/// Below this width the toolbar's search field becomes a button.
const NARROW: f32 = 700.0;

struct App {
    db: Db,
    workspace: workspace::Workspace,
    quick_add: quick_add::QuickAdd,
    navigation: navigation::Navigation,
    /// The large month in the Day view's right-hand pane.
    day_month: navigation::Navigation,
    today: Vec<navigation::TodayItem>,
    recovery: recovery_ui::RecoveryUi,
    theme: theme::Theme,
    /// 12- or 24-hour times; `main` reads it from the system.
    clock: clock::Clock,
    tz: Tz,
    view: View,
    anchor: NaiveDate,
    occ: Vec<Occurrence>,
    stale: bool,
    loaded_at: std::time::Instant,
    scroll_to: Option<f32>,
    /// The sidebar toggle's choice; until it is used the sidebar shows in wide windows.
    sidebar: Option<bool>,
    /// The occurrence the editor opened, as (event id, start), drawn solid.
    selected: Option<(i64, String)>,
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
            clock: clock::Clock::H12,
            db,
            workspace,
            quick_add: quick_add::QuickAdd::default(),
            navigation: navigation::Navigation::new(Utc::now().with_timezone(&tz).date_naive()),
            day_month: navigation::Navigation::new(Utc::now().with_timezone(&tz).date_naive()),
            today: Vec::new(),
            recovery: recovery_ui::RecoveryUi::default(),
            tz,
            view: View::Week,
            anchor: Utc::now().with_timezone(&tz).date_naive(),
            occ: Vec::new(),
            stale: true,
            loaded_at: std::time::Instant::now(),
            scroll_to: Some(6.5 * views::HOUR_PX),
            sidebar: None,
            selected: None,
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
        self.day_month.sync(anchor);
        if view != self.view && view != View::Month {
            self.scroll_to = Some(6.5 * views::HOUR_PX);
        }
        (self.view, self.anchor, self.stale) = (view, anchor, true);
    }

    /// Opens the occurrence a view reported. An index from before a reload
    /// is just stale, so reload rather than open the wrong event.
    fn open(&mut self, index: usize) {
        match self.occ.get(index).cloned() {
            Some(occurrence) => self.open_occurrence(&occurrence),
            None => self.stale = true,
        }
    }

    fn open_occurrence(&mut self, o: &Occurrence) {
        self.selected = Some((o.event_id, o.start.clone()));
        match self.db.event(o.event_id) {
            Ok(Some(e)) => self.editor = Some(self.make_editor(Form::open(o, &e, self.tz), Some(e))),
            Ok(None) => self.stale = true,
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Events from a subscribed feed cannot be edited, dragged or recoloured.
    fn read_only(&self, event_id: i64) -> bool {
        self.db.event_calendar(event_id).is_ok_and(|c| self.workspace.subscribed.contains(&c))
    }

    fn make_editor(&self, form: Form, series: Option<calendio_core::db::Event>) -> Editor {
        let read_only = series.as_ref().is_some_and(|e| self.read_only(e.id));
        let writable = |id: &i64| !self.workspace.subscribed.contains(id);
        let calendar_id = series
            .as_ref()
            .and_then(|e| self.db.event_calendar(e.id).ok())
            .unwrap_or(Some(self.workspace.selected).filter(writable).unwrap_or(1));
        let details = series.as_ref().map(|e| self.db.event_details(e.id));
        let mut editor = Editor::new(form, series, self.clock);
        if let Some(details) = details {
            match details {
                Ok(details) => editor.details = details,
                Err(error) => editor.error = Some(error.to_string()),
            }
        }

        editor.calendar_id = calendar_id;
        editor.read_only = read_only;
        editor.calendars = self.workspace.calendars.iter().filter(|c| read_only || writable(&c.id)).cloned().collect();
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
        let Some(o) = self.occ.get(index) else {
            self.stale = true;
            return;
        };
        if self.read_only(o.event_id) {
            return;
        }
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

    /// Gives the event behind an occurrence a new colour. Colour belongs to the
    /// whole series, so a repeating event changes everywhere, as in Apple Calendar.
    fn recolour(&mut self, index: usize, colour: Option<String>) {
        let Some(o) = self.occ.get(index) else {
            self.stale = true;
            return;
        };
        if self.read_only(o.event_id) {
            return;
        }
        let Ok(Some(e)) = self.db.event(o.event_id) else { return };
        // Straight from the stored event, so an edited occurrence's title or
        // times never spill onto the rest of the series.
        let mut input = e.input.clone();
        input.color = colour;
        let result = self.db.record_edit("change colour", Some(e.id), |db| {
            db.update_event(e.id, &input)?;
            Ok(e.id)
        });
        match result.map(drop).map_err(|e| e.to_string()) {
            Ok(()) => self.stale = true,
            Err(e) => self.error = Some(e),
        }
    }

    /// Row one: sidebar toggle and `+` on the left, the view switch centred,
    /// search and the `…` menu on the right. Row two: the big heading on the
    /// left and `‹ Today ›` on the right.
    fn toolbar(&mut self, ui: &mut egui::Ui, sidebar_shown: bool) {
        let width = ui.available_width();
        let narrow = width < NARROW;
        let (row, _) = ui.allocate_exact_size(egui::vec2(width, chrome::CONTROL_H), egui::Sense::hover());
        let child = |ui: &mut egui::Ui, rect: egui::Rect, layout: egui::Layout| {
            ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout))
        };

        let mut left = child(ui, row, egui::Layout::left_to_right(egui::Align::Center));
        left.spacing_mut().item_spacing.x = 2.0;
        if chrome::icon_button(&mut left, 30.0, "Show or hide the sidebar", chrome::sidebar_glyph).clicked() {
            self.sidebar = Some(!sidebar_shown);
        }
        if chrome::glyph_button(&mut left, "+", 20.0, "New event (N)").clicked() {
            self.new_event();
        }

        let labels = VIEWS.map(|(_, name)| name);
        // Roomy segments, squeezed between the side buttons in a phone-width window.
        let room = (width - 150.0) / 4.0;
        let segment = chrome::segment_width(ui, &labels, 16.0).min(room.max(chrome::segment_width(ui, &labels, 3.0)));
        let selected = if self.booking.is_some() { None } else { VIEWS.iter().position(|(v, _)| *v == self.view) };
        let switch = egui::Rect::from_center_size(row.center(), egui::vec2(segment * 4.0 + 4.0, chrome::CONTROL_H));
        let mut centre = child(ui, switch, egui::Layout::left_to_right(egui::Align::Center));
        if let Some(i) = chrome::segmented(&mut centre, &labels, selected, segment) {
            self.go(VIEWS[i].0, self.anchor);
        }

        let mut right = child(ui, row, egui::Layout::right_to_left(egui::Align::Center));
        right.spacing_mut().item_spacing.x = 6.0;
        let more = chrome::glyph_button(&mut right, "…", 18.0, "More");
        self.more_menu(&more);
        if narrow {
            if chrome::icon_button(&mut right, 28.0, "Search (Ctrl+K)", chrome::magnifier_glyph).clicked() {
                self.workspace.open_search();
                self.booking = None;
            }
        } else {
            let field = if width < 1000.0 { 150.0 } else { 200.0 };
            self.workspace.search_field(&mut right, &self.db, field);
            if self.workspace.search_open {
                self.booking = None;
            }
        }

        ui.add_space(12.0);
        let size = if narrow { 22.0 } else { 26.0 };
        let (row, _) = ui.allocate_exact_size(egui::vec2(width, size * 1.25), egui::Sense::hover());
        let mut nav = child(ui, row, egui::Layout::right_to_left(egui::Align::Center));
        nav.spacing_mut().item_spacing.x = 2.0;
        let (bold, regular) = if self.booking.is_some() {
            ("Booking".to_owned(), "links & hours".to_owned())
        } else {
            if chrome::glyph_button(&mut nav, "›", 20.0, "Next (→)").clicked() {
                self.go(self.view, grid::step(self.view, self.anchor, 1));
            }
            if chrome::pill_button(&mut nav, "Today", "Today (T)").clicked() {
                self.go(self.view, self.now().date());
            }
            if chrome::glyph_button(&mut nav, "‹", 20.0, "Previous (←)").clicked() {
                self.go(self.view, grid::step(self.view, self.anchor, -1));
            }
            grid::heading(self.view, self.anchor)
        };
        let used = nav.min_rect().width();
        let title = egui::Rect::from_min_max(row.min, egui::pos2(row.right() - used - 12.0, row.bottom()));
        let mut title = child(ui, title, egui::Layout::left_to_right(egui::Align::Center));
        title.spacing_mut().item_spacing.x = 7.0;
        title.add(egui::Label::new(RichText::new(bold).font(theme::bold(size)).color(theme::ink())).truncate());
        if !regular.is_empty() {
            let regular = RichText::new(regular).font(egui::FontId::proportional(size)).color(theme::ink_2());
            title.add(egui::Label::new(regular).truncate());
        }
    }

    /// Everything that left the toolbar: quick add, agenda, booking, calendar
    /// files, undo, theme and recovery.
    fn more_menu(&mut self, button: &egui::Response) {
        let ctx = button.ctx.clone();
        egui::Popup::menu(button).show(|ui| {
            ui.set_min_width(230.0);
            ui.spacing_mut().interact_size.y = 24.0;
            ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
            let item = |ui: &mut egui::Ui, label: &str, shortcut: &str| {
                ui.add(egui::Button::new(label).shortcut_text(shortcut).frame_when_inactive(false)).clicked()
            };
            if item(ui, "New event", "N") {
                self.new_event();
            }
            if item(ui, "Quick add…", "Q") {
                self.quick_add.open();
            }
            ui.separator();
            if item(ui, "Agenda", "A") {
                self.go(View::Agenda, self.anchor);
            }
            if item(ui, "Booking links & hours", "B") {
                self.open_booking();
            }
            ui.separator();
            if item(ui, "Manage calendars…", "") {
                self.workspace.open_manage();
            }
            if item(ui, "New subscription…", "") {
                self.workspace.open_subscribe();
            }
            if item(ui, "Import / export…", "") {
                self.workspace.open_transfer();
            }
            ui.separator();
            let label = self.db.undo_label().ok().flatten();
            let undo = egui::Button::new(label.as_ref().map_or("Undo".into(), |l| format!("Undo {l}")))
                .shortcut_text("Ctrl+Z")
                .frame_when_inactive(false);
            if ui.add_enabled(label.is_some(), undo).clicked() {
                self.undo();
            }
            ui.menu_button("Theme", |ui| {
                for theme in theme::Theme::ALL {
                    if ui.selectable_label(self.theme == theme, theme.label()).clicked() {
                        self.set_theme(theme, &ctx);
                        ui.close();
                    }
                }
            });
            if item(ui, "Recovery & connections…", "") {
                self.recovery.open = true;
            }
        });
    }

    fn undo(&mut self) {
        match self.db.undo() {
            Ok(_) => self.stale = true,
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_theme(&mut self, theme: theme::Theme, ctx: &egui::Context) {
        self.theme = theme;
        if let Err(error) = self.db.set_setting("theme", theme.id()) {
            self.error = Some(error.to_string());
        }
        theme.apply(ctx);
        ctx.request_repaint();
    }

    fn open_booking(&mut self) {
        self.workspace.search_open = false;
        self.booking.get_or_insert_with(booking::Screen::new).reload();
    }

    fn new_event(&mut self) {
        // Today: the next full hour. Another day: nine o'clock.
        let now = self.now();
        let start = if self.anchor == now.date() {
            now.date().and_time(NaiveTime::MIN) + Duration::hours(i64::from(now.hour()) + 1)
        } else {
            self.anchor.and_time(NaiveTime::MIN) + Duration::hours(9)
        };
        self.editor = Some(self.make_editor(Form::new(start, start + Duration::hours(1), false, self.tz), None));
    }

    fn keys(&mut self, ctx: &egui::Context) {
        if self.editor.is_none()
            && !self.quick_add.is_open()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::K))
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
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Z)) {
            self.undo();
            return;
        }
        // Ctrl+1–4 (⌘ on a Mac): Day, Week, Month, Year, like Apple Calendar.
        let views = [Key::Num1, Key::Num2, Key::Num3, Key::Num4];
        if let Some(i) = views.iter().position(|&k| ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, k))) {
            self.go(VIEWS[i].0, self.anchor);
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
        if pressed(Key::A) {
            self.go(View::Agenda, self.anchor);
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

        let window = ui.available_width();
        let sidebar_shown = self.sidebar.unwrap_or(window >= 940.0);
        let bar =
            egui::Frame::new().fill(theme::bg()).inner_margin(egui::Margin { left: 10, right: 10, top: 8, bottom: 8 });
        egui::Panel::top("top").frame(bar).show(ui, |ui| {
            self.toolbar(ui, sidebar_shown);
            if let Some(e) = self.error.clone() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(e).color(theme::danger()));
                    if ui.small_button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            }
        });
        if self.workspace.search_open {
            self.booking = None;
        }
        // Day view brings its own right-hand pane, so the Today panel stays out of it.
        let show_today = ui.available_width() >= 1440.0 && self.view != View::Day;
        if sidebar_shown {
            let frame = egui::Frame::new().fill(theme::surface()).inner_margin(egui::Margin::symmetric(12, 14));
            egui::Panel::left("navigation").resizable(false).default_size(232.0).frame(frame).show(ui, |ui| {
                let full = ui.available_rect_before_wrap();
                let split = (full.bottom() - navigation::MINI_MONTH_H).max(full.top() + 60.0);
                let list = egui::Rect::from_min_max(full.min, egui::pos2(full.right(), split - 8.0));
                let mut top = ui.new_child(egui::UiBuilder::new().max_rect(list));
                egui::ScrollArea::vertical().id_salt("sidebar-scroll").max_height(list.height()).show(&mut top, |ui| {
                    match self.workspace.sidebar(ui, &self.db) {
                        Ok(changed) => self.stale |= changed,
                        Err(e) => self.error = Some(e),
                    }
                });
                let mini = egui::Rect::from_min_max(egui::pos2(full.left(), split), full.max);
                let mut bottom = ui.new_child(egui::UiBuilder::new().max_rect(mini));
                let today = self.now().date();
                if let Some(day) = self.navigation.show(&mut bottom, self.view, self.anchor, today) {
                    self.go(if self.view == View::Year { View::Day } else { self.view }, day);
                }
                ui.allocate_rect(full, egui::Sense::hover());
            });
        }
        if show_today && self.booking.is_none() && !self.workspace.search_open {
            let now = self.now();
            let selected = egui::Panel::right("today")
                .resizable(false)
                .default_size(260.0)
                .show(ui, |ui| navigation::today_agenda(ui, &self.today, now, self.tz, self.clock))
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
            let narrow = window < NARROW;
            let found =
                egui::CentralPanel::default().show(ui, |ui| self.workspace.search(ui, &self.db, self.tz, narrow)).inner;
            if let Some((form, event)) = found {
                self.editor = Some(self.make_editor(form, Some(event)));
            }
        } else {
            self.calendar(ui);
        }
        if self.workspace.dialogs(&ctx, &self.db, self.tz) {
            self.stale = true;
        }
        if let Some(form) = self.quick_add.show(&ctx, self.now(), self.tz, self.clock) {
            self.editor = Some(self.make_editor(form, None));
        }
        self.editor_frame(&ctx);
        if self.recovery.show(&ctx, &mut self.db, self.clock) {
            self.stale = true;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// The narrowest calendar area (in points; the default zoom is 1.25) that still
/// gets the Day view's right-hand pane.
const DAY_PANE_MIN: f32 = 640.0;

impl App {
    fn calendar(&mut self, ui: &mut egui::Ui) {
        let mut pane_action = None;
        if self.view == View::Day && ui.available_width() >= DAY_PANE_MIN {
            let scene = Scene {
                occ: &self.occ,
                tz: self.tz,
                now: self.now(),
                clock: self.clock,
                selected: self.editor.as_ref().and(self.selected.clone()),
            };
            let frame = egui::Frame::new().fill(theme::bg()).inner_margin(egui::Margin::symmetric(14, 0));
            pane_action = egui::Panel::right("day-pane")
                .resizable(false)
                .default_size(250.0)
                .frame(frame)
                .show(ui, |ui| views::day_pane(ui, &scene, self.anchor, &mut self.day_month))
                .inner;
        }
        let action = egui::CentralPanel::default()
            .show(ui, |ui| {
                let scene = Scene {
                    occ: &self.occ,
                    tz: self.tz,
                    now: self.now(),
                    clock: self.clock,
                    selected: self.editor.as_ref().and(self.selected.clone()),
                };
                match self.view {
                    View::Month => views::month(ui, &scene, self.anchor),
                    View::Agenda => views::agenda(ui, &scene),
                    View::Year => views::year(ui, &scene, self.anchor),
                    v => views::time_grid(ui, &grid::days(v, self.anchor), &scene, &mut self.scroll_to),
                }
            })
            .inner
            .or(pane_action);

        match action {
            Some(Action::Open(k)) => self.open(k),
            Some(Action::New { start, end, all_day }) => {
                self.editor = Some(self.make_editor(Form::new(start, end, all_day, self.tz), None));
            }
            Some(Action::Move { index, start }) => self.drop_at(index, start),
            Some(Action::Goto(day)) => self.go(View::Day, day),
            Some(Action::Recolour { index, colour }) => self.recolour(index, colour),
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
                self.selected = None;
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

const USAGE: &str = "usage: calendio [--view month|week|day|agenda|year] [--booking] [--new] [--quick-add] [--theme system|apple-light|apple-dark|white|dark|catppuccin|tokyo-night|gruvbox|everforest|rose-pine|nord] [--recovery] [--size WIDTHxHEIGHT]";

/// `--size 390x780`: the window's starting size, for screenshots of narrow layouts.
fn parse_size(value: &str) -> Option<[f32; 2]> {
    let (w, h) = value.split_once('x')?;
    let (w, h) = (w.parse::<u16>().ok()?, h.parse::<u16>().ok()?);
    (w >= 360 && h >= 320).then_some([f32::from(w), f32::from(h)])
}

/// Vsync stays off: on Wayland a vsynced swap waits for a frame callback, and Hyprland
/// sends none to a window on a hidden workspace, so the 30s repaint would block the
/// main thread there until the compositor reports Calendio as not responding.
fn native_options(size: [f32; 2]) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Calendio")
            .with_app_id("calendio")
            .with_inner_size(size)
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
    let mut size = [1180.0, 780.0];
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
            "--size" => match args.next().as_deref().and_then(parse_size) {
                Some(value) => size = value,
                None => {
                    eprintln!("{USAGE}");
                    std::process::exit(2);
                }
            },
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
    let clock = clock::Clock::system();
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
        native_options(size),
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            // Bigger text and icons on a 1080p screen; UI_ZOOM=1 gives the old size.
            let zoom = std::env::var("UI_ZOOM").ok().and_then(|z| z.parse::<f32>().ok()).unwrap_or(1.25);
            cc.egui_ctx.set_zoom_factor(zoom.clamp(0.5, 3.0));
            let mut app = App::new(db, tz);
            app.view = view;
            app.clock = clock;
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
    fn a_stale_occurrence_index_reloads_instead_of_crashing() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        app.reload();
        app.open(3);
        let start = app.now();
        app.drop_at(3, start);
        assert!(app.editor.is_none());
        assert!(app.stale, "the calendar reloads");
    }

    #[test]
    fn swaps_never_wait_on_the_compositor() {
        assert!(!super::native_options([1180.0, 780.0]).glow_options.vsync);
    }

    #[test]
    fn window_size_flag_accepts_supported_sizes_only() {
        assert_eq!(parse_size("390x780"), Some([390.0, 780.0]));
        assert_eq!(parse_size("1180x780"), Some([1180.0, 780.0]));
        for bad in ["300x780", "390x200", "390", "x780", "wide"] {
            assert_eq!(parse_size(bad), None, "{bad}");
        }
    }
}

#[cfg(test)]
mod calendar_edit_tests {
    use super::*;
    fn subscribed_app() -> (App, i64) {
        let db = Db::open_in_memory().unwrap();
        let id = db.create_subscription("Holidays", "#ff9500", "https://example.com/h.ics").unwrap();
        let feed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//T//EN\r\nBEGIN:VEVENT\r\nUID:x\r\n\
                    DTSTAMP:20261001T000000Z\r\nDTSTART:20261019T090000Z\r\nDTEND:20261019T100000Z\r\n\
                    SUMMARY:Holiday\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        db.replace_subscription(id, feed, None, "2026-10-01T00:00:00Z".parse().unwrap(), "UTC").unwrap();
        let mut app = App::new(db, chrono_tz::UTC);
        app.view = View::Week;
        app.anchor = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        app.stale = true;
        app.reload();
        (app, id)
    }
    #[test]
    fn dragging_or_recolouring_a_subscribed_event_is_ignored() {
        let (mut app, _) = subscribed_app();
        let index = app.occ.iter().position(|o| o.title == "Holiday").expect("the feed event is shown");
        let before = app.db.event(app.occ[index].event_id).unwrap().unwrap();
        app.drop_at(index, NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_hms_opt(9, 0, 0).unwrap());
        app.recolour(index, Some("#34c759".into()));
        assert_eq!(app.db.event(before.id).unwrap().unwrap(), before);
        assert_eq!(app.error, None, "ignored quietly, no error banner");
    }
    #[test]
    fn subscribed_events_open_read_only_and_new_events_never_target_a_feed() {
        let (mut app, feed) = subscribed_app();
        let index = app.occ.iter().position(|o| o.title == "Holiday").unwrap();
        app.open(index);
        assert!(app.editor.as_ref().unwrap().read_only);
        app.editor = None;
        app.workspace.selected = feed;
        app.new_event();
        let editor = app.editor.as_ref().unwrap();
        assert!(!editor.read_only);
        assert_ne!(editor.calendar_id, feed);
        assert!(editor.calendars.iter().all(|c| c.id != feed), "the calendar menu leaves feeds out");
    }
    #[test]
    fn recolouring_a_block_changes_the_whole_series_and_nothing_else() {
        let db = Db::open_in_memory().unwrap();
        let input = calendio_core::db::EventInput {
            title: "Gym".into(),
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
        // The second day has its own title, which must stay on that day only.
        db.set_exception(&calendio_core::db::Exception {
            event_id: id,
            original_start: "2026-10-20T09:00:00Z".into(),
            cancelled: false,
            title: Some("Gym (legs)".into()),
            notes: None,
            start: None,
            end: None,
        })
        .unwrap();
        let mut app = App::new(db, chrono_tz::UTC);
        app.view = View::Week;
        app.anchor = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        app.stale = true;
        app.reload();
        let legs = app.occ.iter().position(|o| o.title == "Gym (legs)").expect("the edited day is shown");
        app.recolour(legs, Some("#ff9500".into()));
        let e = app.db.event(id).unwrap().unwrap();
        assert_eq!(e.color.as_deref(), Some("#ff9500"));
        assert_eq!((e.title.as_str(), e.start.as_str()), ("Gym", "2026-10-19T09:00:00Z"));
        assert!(app.stale, "the calendar reloads");
        app.reload();
        assert!(app.occ.iter().all(|o| o.color.as_deref() == Some("#ff9500")), "every day of the series");

        app.recolour(0, None);
        assert_eq!(app.db.event(id).unwrap().unwrap().color, None, "back to the calendar's colour");
    }

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

/// Shared by the layout tests: runs a few frames and reads back where text landed.
#[cfg(test)]
mod frames {
    use super::*;

    /// Every visible label and where it is: centre inside both its clip rect and the window.
    pub fn text_rects(shapes: &[egui::epaint::ClippedShape], screen: egui::Rect) -> Vec<(String, egui::Rect)> {
        fn walk(shape: &egui::Shape, view: egui::Rect, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, view, out)),
                egui::Shape::Text(text) => {
                    let rect = shape.visual_bounding_rect();
                    if view.contains(rect.center()) {
                        out.push((text.galley.text().to_owned(), rect));
                    }
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

    pub fn render(app: &mut App, width: f32, height: f32) -> Vec<(String, egui::Rect)> {
        let harness = Harness::new(width, height);
        text_rects(&harness.settle(app), harness.screen)
    }

    /// One egui context driving an App frame by frame, with optional input.
    pub struct Harness {
        pub ctx: egui::Context,
        pub screen: egui::Rect,
    }

    impl Harness {
        pub fn new(width: f32, height: f32) -> Self {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            Self { ctx, screen: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height)) }
        }

        pub fn run(&self, app: &mut App, events: Vec<egui::Event>) -> Vec<egui::epaint::ClippedShape> {
            let input = egui::RawInput { screen_rect: Some(self.screen), events, ..Default::default() };
            let mut output = self.ctx.run_ui(input, |ui| app.show(ui));
            output.textures_delta.clear();
            output.shapes
        }

        /// A few frames so layout settles; the last frame's shapes.
        pub fn settle(&self, app: &mut App) -> Vec<egui::epaint::ClippedShape> {
            (0..3).map(|_| self.run(app, Vec::new())).last().unwrap_or_default()
        }

        pub fn click(&self, app: &mut App, pos: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.run(app, vec![egui::Event::PointerMoved(pos)]);
            self.run(app, vec![button(true)]);
            self.run(app, vec![button(false)]);
        }
    }

    /// Every painted shape, flattened, with the clip rect it is drawn under.
    pub fn flat(shapes: &[egui::epaint::ClippedShape]) -> Vec<(egui::Rect, egui::Shape)> {
        fn walk(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<(egui::Rect, egui::Shape)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, clip, out)),
                other => out.push((clip, other.clone())),
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, clipped.clip_rect, &mut out);
        }
        out
    }

    pub fn circles(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::CircleShape> {
        flat(shapes)
            .into_iter()
            .filter_map(|(clip, s)| match s {
                egui::Shape::Circle(c) if clip.contains(c.center) => Some(c),
                _ => None,
            })
            .collect()
    }

    pub fn rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::epaint::RectShape> {
        flat(shapes)
            .into_iter()
            .filter_map(|(clip, s)| match s {
                egui::Shape::Rect(r) if clip.intersects(r.rect) => Some(r),
                _ => None,
            })
            .collect()
    }

    /// The font a label was laid out in.
    pub fn text_font(shapes: &[egui::epaint::ClippedShape], label: &str, near: egui::Pos2) -> Option<egui::FontId> {
        flat(shapes).into_iter().find_map(|(_, s)| match s {
            egui::Shape::Text(t)
                if t.galley.text() == label && egui::Shape::Text(t.clone()).visual_bounding_rect().contains(near) =>
            {
                t.galley.job.sections.first().map(|s| s.format.font_id.clone())
            }
            _ => None,
        })
    }

    /// The colour a label was laid out in.
    pub fn text_color(shapes: &[egui::epaint::ClippedShape], label: &str, near: egui::Pos2) -> Option<egui::Color32> {
        flat(shapes).into_iter().find_map(|(_, s)| match s {
            egui::Shape::Text(t)
                if t.galley.text() == label && egui::Shape::Text(t.clone()).visual_bounding_rect().contains(near) =>
            {
                t.override_text_color.or_else(|| t.galley.job.sections.first().map(|s| s.format.color))
            }
            _ => None,
        })
    }

    pub fn find(texts: &[(String, egui::Rect)], label: &str) -> Vec<egui::Rect> {
        texts.iter().filter(|(t, _)| t == label).map(|(_, r)| *r).collect()
    }

    pub fn one(texts: &[(String, egui::Rect)], label: &str) -> egui::Rect {
        let found = find(texts, label);
        assert_eq!(found.len(), 1, "expected one {label:?}, found {found:?}; visible: {texts:?}");
        found[0]
    }
}

#[cfg(test)]
mod toolbar_tests {
    use super::frames::{find, one, render};
    use super::*;

    fn november_app() -> App {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        app.go(View::Month, NaiveDate::from_ymd_opt(2026, 11, 5).unwrap());
        app
    }

    #[test]
    fn day_view_has_apples_month_and_events_pane_on_the_right() {
        let mut app = november_app();
        app.go(View::Day, NaiveDate::from_ymd_opt(2026, 11, 5).unwrap());
        // 1180x780 pixels at the default 1.25 zoom.
        let texts = render(&mut app, 944.0, 624.0);
        // One mini month in the sidebar, one in the day pane on the right.
        let months = find(&texts, "November 2026");
        assert_eq!(months.len(), 2, "{months:?}");
        assert!(months.iter().any(|r| r.left() > 640.0), "the pane is on the right");
        let empty = one(&texts, "No Events");
        assert!(empty.left() > 640.0);
        // Week view keeps the whole width for the grid.
        app.go(View::Week, NaiveDate::from_ymd_opt(2026, 11, 5).unwrap());
        let texts = render(&mut app, 944.0, 624.0);
        assert_eq!(find(&texts, "November 2026").len(), 1);
        assert!(find(&texts, "No Events").is_empty());
    }

    #[test]
    fn toolbar_is_one_row_with_the_views_centred_and_the_month_title_below() {
        let mut app = november_app();
        let texts = render(&mut app, 1180.0, 780.0);
        let week = one(&texts, "Week");
        for label in ["Day", "Month", "Year"] {
            let rect = one(&texts, label);
            assert!((rect.center().y - week.center().y).abs() < 2.0, "{label} is off the toolbar row");
        }
        let centre = (one(&texts, "Day").left() + one(&texts, "Year").right()) / 2.0;
        assert!((centre - 590.0).abs() < 80.0, "the view switch is centred, not at {centre}");
        let plus = one(&texts, "+");
        assert!(plus.left() < 120.0 && (plus.center().y - week.center().y).abs() < 4.0, "+ sits on the left");
        let search = one(&texts, "Search");
        assert!(search.left() > 860.0 && (search.center().y - week.center().y).abs() < 4.0, "search on the right");
        let more = one(&texts, "…");
        assert!(more.left() > 860.0 && (more.center().y - week.center().y).abs() < 4.0, "… menu on the right");

        let month = one(&texts, "November");
        let year = one(&texts, "2026");
        assert!(month.top() > week.bottom() && month.left() < 300.0, "the title sits under the toolbar on the left");
        assert!(month.height() > 2.0 * week.height() - 6.0, "the title is large");
        assert!(year.left() > month.right() && (year.center().y - month.center().y).abs() < 4.0);
        let today = one(&texts, "Today");
        assert!(today.left() > 900.0 && (today.center().y - month.center().y).abs() < 14.0, "Today on the title row");
        for arrow in ["‹", "›"] {
            let beside = find(&texts, arrow).into_iter().any(|r| {
                (r.center().y - today.center().y).abs() < 4.0 && (r.center().x - today.center().x).abs() < 60.0
            });
            assert!(beside, "{arrow} beside Today");
        }

        for gone in [
            "Import / export",
            "Quick add",
            "Undo",
            "Recovery & connections",
            "Search events",
            "Calendars",
            "Booking",
            "+ New",
            "Create event",
            "Agenda",
            "Calendio",
        ] {
            assert!(find(&texts, gone).is_empty(), "{gone:?} should be off the toolbar");
        }
        assert!(texts.iter().all(|(t, _)| !t.starts_with("Theme")), "the theme picker moves to the … menu");
        for rect in find(&texts, "Personal") {
            assert!(rect.left() < 232.0, "only the sidebar names the Personal calendar");
        }
    }

    #[test]
    fn week_opens_on_the_working_day_not_at_midnight() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        // A week in the past: in this week the red now line would hide the hour label
        // beside it, so around 7 AM UTC "7 AM" was missing and the test failed.
        app.go(View::Week, NaiveDate::from_ymd_opt(2025, 11, 5).unwrap());
        let texts = render(&mut app, 1180.0, 780.0);
        assert!(find(&texts, "1 AM").is_empty(), "the small hours are scrolled away");
        assert_eq!(find(&texts, "7 AM").len(), 1, "seven in the morning is in view");
    }

    #[test]
    fn toolbar_still_fits_one_row_in_a_phone_width_window() {
        for width in [360.0, 390.0] {
            let mut app = november_app();
            let texts = render(&mut app, width, 780.0);
            let week = one(&texts, "Week");
            for label in ["Day", "Month", "Year", "+", "…"] {
                let rect = one(&texts, label);
                assert!((rect.center().y - week.center().y).abs() < 4.0, "{label} wraps at {width} px");
                assert!(rect.right() <= width, "{label} is cut off at {width} px");
            }
            for label in ["November", "Today", "‹", "›"] {
                assert!(one(&texts, label).right() <= width, "{label} is cut off at {width} px");
            }
        }
    }
}

#[cfg(test)]
mod sidebar_tests {
    use super::frames::{circles, find, one, rects, text_color, text_rects, Harness};
    use super::*;
    use chrono::Datelike;

    const SIDEBAR: f32 = 240.0;

    fn hex(s: &str) -> egui::Color32 {
        egui::Color32::from_hex(s).unwrap()
    }

    fn app_with_calendars() -> (App, i64, i64) {
        let db = Db::open_in_memory().unwrap();
        let work = db.create_calendar("Work", "#ff9500").unwrap();
        let hidden = db.create_calendar("Hobbies", "#63da38").unwrap();
        db.update_calendar(hidden, "Hobbies", "#34c759", false).unwrap();
        let mut app = App::new(db, chrono_tz::UTC);
        let today = app.now().date();
        app.go(View::Week, today);
        (app, work, hidden)
    }

    /// The round checkbox left of a calendar's name.
    fn dot_beside(shapes: &[egui::epaint::ClippedShape], name: egui::Rect) -> egui::epaint::CircleShape {
        circles(shapes)
            .into_iter()
            .filter(|c| c.center.x < name.left() && (c.center.y - name.center().y).abs() < 4.0)
            .min_by(|a, b| (name.left() - a.center.x).total_cmp(&(name.left() - b.center.x)))
            .unwrap_or_else(|| panic!("no round checkbox beside the calendar at {name:?}"))
    }

    #[test]
    fn sidebar_groups_calendars_under_my_calendars_with_round_colour_checkboxes() {
        let (mut app, _, _) = app_with_calendars();
        let harness = Harness::new(1180.0, 780.0);
        let shapes = harness.settle(&mut app);
        let texts = text_rects(&shapes, harness.screen);
        let header = one(&texts, "My Calendars");
        assert!(header.left() < SIDEBAR);
        let mut above = header.bottom();
        for (name, colour, visible) in
            [("Personal", "#0088ff", true), ("Work", "#ff8d28", true), ("Hobbies", "#34c759", false)]
        {
            let rect = one(&texts, name);
            assert!(rect.left() < SIDEBAR && rect.top() >= above, "{name} is listed under My Calendars, in order");
            above = rect.bottom();
            let dot = dot_beside(&shapes, rect);
            if visible {
                assert_eq!(dot.fill, hex(colour), "a shown calendar's circle is filled with its colour");
            } else {
                assert_eq!(dot.fill, egui::Color32::TRANSPARENT, "a hidden calendar's circle is empty");
                assert_eq!(dot.stroke.color, hex(colour), "a hidden calendar's ring keeps its colour");
            }
        }
        for gone in ["SCHEDULING", "TIMEZONE", "MY CALENDARS", "Stored on this device", "Booking links & hours"] {
            assert!(find(&texts, gone).is_empty(), "{gone:?} is gone from the sidebar");
        }
    }

    #[test]
    fn clicking_a_calendar_circle_toggles_its_visibility() {
        let (mut app, work, _) = app_with_calendars();
        let harness = Harness::new(1180.0, 780.0);
        let shapes = harness.settle(&mut app);
        let dot = dot_beside(&shapes, one(&text_rects(&shapes, harness.screen), "Work"));
        harness.click(&mut app, dot.center);
        let visible = |app: &App| app.db.calendars().unwrap().into_iter().find(|c| c.id == work).unwrap().visible;
        assert!(!visible(&app), "one click hides Work");
        harness.click(&mut app, dot.center);
        assert!(visible(&app), "a second click shows it again");
    }

    #[test]
    fn mini_month_sits_at_the_bottom_with_today_in_red_and_the_week_tinted() {
        let (mut app, _, _) = app_with_calendars();
        let today = app.now().date();
        let harness = Harness::new(1180.0, 780.0);
        let shapes = harness.settle(&mut app);
        let texts = text_rects(&shapes, harness.screen);
        let header = one(&texts, &today.format("%B %Y").to_string());
        assert!(header.left() < SIDEBAR && header.top() > 780.0 / 2.0, "the mini month is at the bottom");

        let number = today.day().to_string();
        let red = circles(&shapes)
            .into_iter()
            .filter(|c| c.fill == theme::today() && c.center.x < SIDEBAR && c.center.y > header.bottom())
            .collect::<Vec<_>>();
        assert_eq!(red.len(), 1, "one red circle marks today in the mini month");
        let cell = red[0].center;
        let label = find(&texts, &number).into_iter().find(|r| r.center().distance(cell) < 3.0);
        assert!(label.is_some(), "today's number sits in the red circle");
        assert_eq!(text_color(&shapes, &number, cell), Some(egui::Color32::WHITE), "today's number is white");

        let band = rects(&shapes)
            .into_iter()
            .find(|r| r.fill == theme::tint() && r.rect.contains(cell) && r.rect.right() < SIDEBAR + 1.0);
        let band = band.expect("the selected week is tinted");
        assert!(band.rect.width() > 150.0, "the tint spans the whole week, not one day");
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::frames::{one, text_rects, Harness};
    use super::*;

    fn press(key: Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        vec![event(true), event(false)]
    }

    #[test]
    fn ctrl_1_to_4_switch_day_week_month_year() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let harness = Harness::new(1180.0, 780.0);
        harness.settle(&mut app);
        for (key, view) in
            [(Key::Num1, View::Day), (Key::Num2, View::Week), (Key::Num3, View::Month), (Key::Num4, View::Year)]
        {
            harness.run(&mut app, press(key, egui::Modifiers::COMMAND));
            assert_eq!(app.view, view, "Ctrl+{key:?}");
        }
        harness.run(&mut app, press(Key::Num1, egui::Modifiers::NONE));
        assert_eq!(app.view, View::Year, "a bare digit is not a shortcut");
    }

    #[test]
    fn t_jumps_to_today_and_the_arrows_page() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let today = app.now().date();
        app.go(View::Week, today + Duration::days(70));
        let harness = Harness::new(1180.0, 780.0);
        harness.settle(&mut app);
        harness.run(&mut app, press(Key::ArrowRight, egui::Modifiers::NONE));
        assert_eq!(app.anchor, today + Duration::days(77));
        harness.run(&mut app, press(Key::ArrowLeft, egui::Modifiers::NONE));
        harness.run(&mut app, press(Key::ArrowLeft, egui::Modifiers::NONE));
        assert_eq!(app.anchor, today + Duration::days(63));
        harness.run(&mut app, press(Key::T, egui::Modifiers::NONE));
        assert_eq!(app.anchor, today);
        assert_eq!(app.view, View::Week, "T keeps the view");
    }

    #[test]
    fn switching_views_keeps_the_toolbar_heading_row_and_sidebar_still() {
        let mut app = App::new(Db::open_in_memory().unwrap(), chrono_tz::UTC);
        let harness = Harness::new(1180.0, 780.0);
        let mut first: Option<Vec<egui::Rect>> = None;
        for view in [View::Week, View::Day, View::Month, View::Year, View::Agenda, View::Week] {
            app.go(view, app.anchor);
            let texts = text_rects(&harness.settle(&mut app), harness.screen);
            let still: Vec<egui::Rect> =
                ["+", "Day", "Week", "Year", "Search", "…", "Today", "My Calendars"].map(|l| one(&texts, l)).to_vec();
            match &first {
                None => first = Some(still),
                Some(first) => {
                    for (a, b) in first.iter().zip(&still) {
                        assert!((a.min - b.min).length() < 0.5, "{view:?} moves the chrome: {a:?} → {b:?}");
                    }
                }
            }
        }
    }
}
