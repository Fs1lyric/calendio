//! Apple Calendar's colour choices for a time block: rows of round swatches
//! with a check on the chosen one, and the same colours as a right-click menu.

use crate::theme;
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui};

const SIZE: f32 = 22.0;
const RADIUS: f32 = 8.5;

/// A white check, or a dark one on a colour too pale for white.
fn check(ui: &Ui, center: Pos2, on: Color32, scale: f32) {
    let ink = if theme::contrast(Color32::WHITE, on) >= 2.0 { Color32::WHITE } else { Color32::from_gray(40) };
    let s = scale;
    let points =
        [center + egui::vec2(-3.6, 0.2) * s, center + egui::vec2(-1.0, 2.8) * s, center + egui::vec2(3.8, -2.6) * s];
    ui.painter().line(points.to_vec(), Stroke::new(1.8 * s, ink));
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// One round swatch. Returns whether it was clicked.
fn swatch(ui: &mut Ui, fill: Color32, chosen: bool, name: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(SIZE, SIZE), Sense::click());
    let r = if resp.hovered() { RADIUS + 1.0 } else { RADIUS };
    ui.painter().circle_filled(rect.center(), r, fill);
    if chosen {
        check(ui, rect.center(), fill, 1.0);
    }
    resp.on_hover_text(name).clicked()
}

/// The colour picker. `colour` is `None` for "the calendar's colour", which is
/// offered first as `calendar` when given. Apple's colours sit on the first row,
/// the pastels on the second, then a custom colour. Returns whether it changed.
pub fn picker(ui: &mut Ui, colour: &mut Option<String>, calendar: Option<Color32>) -> bool {
    let before = colour.clone();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(fill) = calendar {
                if swatch(ui, fill, colour.is_none(), "Calendar colour") {
                    *colour = None;
                }
            }
            for (name, hex) in &theme::SWATCHES[..theme::APPLE] {
                if swatch(ui, theme::hex(Some(hex)), colour.as_deref().is_some_and(|c| same(c, hex)), name) {
                    *colour = Some((*hex).to_string());
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (name, hex) in &theme::SWATCHES[theme::APPLE..] {
                if swatch(ui, theme::hex(Some(hex)), colour.as_deref().is_some_and(|c| same(c, hex)), name) {
                    *colour = Some((*hex).to_string());
                }
            }
            let custom = colour.as_deref().is_some_and(|c| !theme::SWATCHES.iter().any(|(_, h)| same(c, h)));
            let current = colour.as_deref().map(|c| theme::hex(Some(c))).or(calendar).unwrap_or(theme::accent());
            let mut rgb = [current.r(), current.g(), current.b()];
            ui.add_space(4.0);
            let resp = ui.color_edit_button_srgb(&mut rgb).on_hover_text("Custom colour");
            if custom {
                check(ui, resp.rect.center(), current, 0.8);
            }
            if resp.changed() {
                *colour = Some(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]));
            }
        });
    });
    *colour != before
}

/// The right-click menu on a time block: every colour as a dot and its name,
/// with a check beside the current one. Returns the colour picked, if any
/// (`Some(None)` puts the block back on its calendar's colour).
pub fn menu(ui: &mut Ui, current: Option<&str>) -> Option<Option<String>> {
    let mut picked = None;
    ui.set_min_width(170.0);
    ui.label(egui::RichText::new("Colour").font(theme::semibold(12.0)).color(theme::ink_3()));
    let row = |ui: &mut Ui, fill: Option<Color32>, name: &str, chosen: bool| {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(160.0), 22.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 4.0, theme::accent());
        }
        let text = if resp.hovered() { Color32::WHITE } else { theme::ink() };
        if chosen {
            let c = Pos2::new(rect.left() + 9.0, rect.center().y);
            let points = [c + egui::vec2(-3.6, 0.2), c + egui::vec2(-1.0, 2.8), c + egui::vec2(3.8, -2.6)];
            ui.painter().line(points.to_vec(), Stroke::new(1.6, text));
        }
        if let Some(fill) = fill {
            ui.painter().circle_filled(Pos2::new(rect.left() + 24.0, rect.center().y), 5.0, fill);
        }
        ui.painter().text(
            Pos2::new(rect.left() + 36.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(13.0),
            text,
        );
        resp.clicked()
    };
    for (name, hex) in theme::SWATCHES {
        if row(ui, Some(theme::hex(Some(hex))), name, current.is_some_and(|c| same(c, hex))) {
            picked = Some(Some(hex.to_string()));
        }
    }
    ui.separator();
    if row(ui, None, "Calendar colour", false) {
        picked = Some(None);
    }
    if picked.is_some() {
        ui.close();
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the picker for a few frames, clicking `at` on the last ones.
    fn run(
        colour: &mut Option<String>,
        calendar: Option<Color32>,
        at: Option<Pos2>,
    ) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        theme::Theme::AppleLight.apply(&ctx);
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 300.0));
        let mut events: Vec<Vec<egui::Event>> = vec![vec![], vec![], vec![]];
        if let Some(pos) = at {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            events.extend([vec![egui::Event::PointerMoved(pos)], vec![button(true)], vec![button(false)], vec![]]);
        }
        let mut shapes = Vec::new();
        for events in events {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                picker(ui, colour, calendar);
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        shapes
    }

    fn circle_of(shapes: &[egui::epaint::ClippedShape], fill: Color32) -> Pos2 {
        crate::frames::circles(shapes)
            .into_iter()
            .find(|c| c.fill == fill)
            .unwrap_or_else(|| panic!("no swatch filled {fill:?}"))
            .center
    }

    #[test]
    fn the_picker_offers_apples_seven_colours_then_the_seven_pastels() {
        // iPadOS 26 Calendar's calendar colours: iOS 26's system colours, sampled
        // from Apple's own iPad Calendar screenshots (spec/reference/ipad).
        assert_eq!(
            theme::SWATCHES[..theme::APPLE],
            [
                ("Red", "#ff383c"),
                ("Orange", "#ff8d28"),
                ("Yellow", "#ffcc00"),
                ("Green", "#34c759"),
                ("Blue", "#0088ff"),
                ("Purple", "#cb30e0"),
                ("Brown", "#ac7f5e"),
            ]
        );
        // The person's pastels, kept on a second row.
        assert_eq!(
            theme::SWATCHES[theme::APPLE..],
            [
                ("Lilac", "#f4c3e8"),
                ("Blush", "#f48aab"),
                ("Seafoam", "#a5e0ca"),
                ("Cream", "#f5dfb4"),
                ("Sky", "#89b3f9"),
                ("Aqua", "#96dfcd"),
                ("Slate", "#b2bad1"),
            ]
        );
    }

    #[test]
    fn pastels_sit_on_the_second_row_under_apples_colours() {
        let shapes = run(&mut None, None, None);
        let red = circle_of(&shapes, theme::hex(Some("#ff383c")));
        let lilac = circle_of(&shapes, theme::hex(Some("#f4c3e8")));
        assert!(lilac.y > red.y + 10.0, "pastels are a row below");
    }

    #[test]
    fn every_colour_is_painted_as_a_round_swatch_plus_the_calendar_colour() {
        let calendar = Color32::from_rgb(0x24, 0x57, 0xb2);
        let shapes = run(&mut None, Some(calendar), None);
        for (_, hex) in theme::SWATCHES {
            circle_of(&shapes, theme::hex(Some(hex)));
        }
        circle_of(&shapes, calendar);
    }

    #[test]
    fn clicking_a_swatch_picks_it_and_checks_it() {
        let mut colour = None;
        let blue = theme::hex(Some("#0088ff"));
        let at = circle_of(&run(&mut colour.clone(), None, None), blue);
        let shapes = run(&mut colour, None, Some(at));
        assert_eq!(colour.as_deref(), Some("#0088ff"));
        let near_blue = crate::frames::flat(&shapes).into_iter().any(|(_, s)| match s {
            egui::Shape::Path(p) => {
                p.points.iter().all(|q| q.distance(at) < RADIUS)
                    && p.stroke.color == egui::epaint::ColorMode::Solid(Color32::WHITE)
            }
            _ => false,
        });
        assert!(near_blue, "a white check sits inside the chosen swatch");
    }

    #[test]
    fn the_calendar_swatch_goes_back_to_the_calendar_colour() {
        let calendar = Color32::from_rgb(0x24, 0x57, 0xb2);
        let mut colour = Some("#ff3b30".to_string());
        let at = circle_of(&run(&mut colour.clone(), Some(calendar), None), calendar);
        run(&mut colour, Some(calendar), Some(at));
        assert_eq!(colour, None);
    }
}
