//! Apple Calendar's toolbar pieces, painted directly: the segmented view
//! switch, borderless glyph buttons and the sidebar and search glyphs.

use crate::theme;
use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Rect, Response, Sense, Stroke};

/// Height of every toolbar control.
pub const CONTROL_H: f32 = 26.0;

/// Equal-width segments on a grey track; the chosen one is a raised pill.
/// Returns the segment clicked this frame.
pub fn segmented(ui: &mut egui::Ui, labels: &[&str], selected: Option<usize>, segment: f32) -> Option<usize> {
    let size = vec2(segment * labels.len() as f32 + 4.0, CONTROL_H);
    let (track, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(track, CornerRadius::same(7), theme::surface_2());
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate() {
        let rect = Rect::from_min_size(track.min + vec2(2.0 + segment * i as f32, 2.0), vec2(segment, CONTROL_H - 4.0));
        let response = ui.interact(rect, ui.id().with(("segment", i)), Sense::click());
        let p = ui.painter();
        if selected == Some(i) {
            p.rect_filled(rect.translate(vec2(0.0, 0.5)), CornerRadius::same(5), theme::shadow());
            p.rect_filled(rect, CornerRadius::same(5), theme::raised());
        } else if response.hovered() {
            p.rect_filled(rect, CornerRadius::same(5), theme::hover());
        }
        if i > 0 && selected != Some(i) && selected != Some(i - 1) {
            let x = rect.left();
            p.line_segment([pos2(x, rect.top() + 5.0), pos2(x, rect.bottom() - 5.0)], Stroke::new(1.0, theme::line()));
        }
        p.text(rect.center(), Align2::CENTER_CENTER, *label, FontId::proportional(13.0), theme::ink());
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// The width a segment needs for the widest label, plus side padding.
pub fn segment_width(ui: &egui::Ui, labels: &[&str], padding: f32) -> f32 {
    let widest = labels
        .iter()
        .map(|l| ui.ctx().fonts_mut(|f| f.layout_no_wrap((*l).to_owned(), FontId::proportional(13.0), Color32::WHITE)))
        .map(|g| g.size().x)
        .fold(0.0, f32::max);
    widest + 2.0 * padding
}

/// A borderless button that shows a hover tint, then lets `paint` draw its face.
pub fn icon_button(
    ui: &mut egui::Ui,
    width: f32,
    hover: &str,
    paint: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL_H), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), theme::hover());
    }
    paint(ui.painter(), rect, theme::ink_2());
    response.on_hover_text(hover)
}

/// A borderless button with a text glyph such as `+`, `…`, `‹` or `›`.
pub fn glyph_button(ui: &mut egui::Ui, glyph: &str, size: f32, hover: &str) -> Response {
    icon_button(ui, 28.0, hover, |p, rect, color| {
        p.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(size), color);
    })
}

/// A rounded window with a divider near its left edge.
pub fn sidebar_glyph(p: &Painter, rect: Rect, color: Color32) {
    let r = Rect::from_center_size(rect.center(), vec2(17.0, 13.0));
    let stroke = Stroke::new(1.3, color);
    p.rect_stroke(r, CornerRadius::same(3), stroke, egui::StrokeKind::Inside);
    let x = r.left() + 6.0;
    p.line_segment([pos2(x, r.top() + 1.0), pos2(x, r.bottom() - 1.0)], stroke);
}

/// A lens and handle.
pub fn magnifier_glyph(p: &Painter, rect: Rect, color: Color32) {
    let c = rect.center() - vec2(1.5, 1.5);
    let stroke = Stroke::new(1.4, color);
    p.circle_stroke(c, 4.5, stroke);
    p.line_segment([c + vec2(3.3, 3.3), c + vec2(7.0, 7.0)], stroke);
}

/// A small filled pill with a label, like Apple's Today button.
pub fn pill_button(ui: &mut egui::Ui, label: &str, hover: &str) -> Response {
    let galley = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label.to_owned(), FontId::proportional(13.0), theme::ink()));
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 22.0, CONTROL_H - 2.0), Sense::click());
    let fill = if response.hovered() { theme::line() } else { theme::surface_2() };
    ui.painter().rect_filled(rect, CornerRadius::same(6), fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, theme::ink());
    response.on_hover_text(hover)
}
