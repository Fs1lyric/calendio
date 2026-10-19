//! Calendio: paper surfaces, ink typography and a blue action accent.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

pub const BG: Color32 = Color32::from_rgb(0xfa, 0xfb, 0xf9);
pub const SURFACE: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
pub const SURFACE_2: Color32 = Color32::from_rgb(0xee, 0xf1, 0xf0);
pub const LINE: Color32 = Color32::from_rgb(0xd6, 0xdd, 0xda);
pub const INK: Color32 = Color32::from_rgb(0x20, 0x2c, 0x32);
pub const INK_2: Color32 = Color32::from_rgb(0x42, 0x53, 0x5d);
pub const INK_3: Color32 = Color32::from_rgb(0x56, 0x68, 0x71);
pub const ACCENT: Color32 = Color32::from_rgb(0x24, 0x57, 0xb2);
pub const PINK: Color32 = Color32::from_rgb(0xa7, 0x2e, 0x49);
pub const DANGER: Color32 = PINK;

/// Event colours, stored on the event as `#rrggbb`.
pub const SWATCHES: [(&str, &str); 7] = [
    ("Lilac", "#f4c3e8"),
    ("Pink", "#f48aab"),
    ("Mint", "#a5e0ca"),
    ("Cream", "#f5dfb4"),
    ("Blue", "#89b3f9"),
    ("Teal", "#96dfcd"),
    ("Slate", "#b2bad1"),
];

pub fn hex(s: Option<&str>) -> Color32 {
    s.and_then(|s| Color32::from_hex(s).ok()).unwrap_or(Color32::from_rgb(0x89, 0xb3, 0xf9))
}

/// A colour faded into the background, for event fills.
pub fn soft(c: Color32) -> Color32 {
    let mix = |a: u8, b: u8| ((a as u16 * 12 + b as u16 * 88) / 100) as u8;
    Color32::from_rgb(mix(c.r(), BG.r()), mix(c.g(), BG.g()), mix(c.b(), BG.b()))
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::light();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.window_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = SURFACE_2;
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(INK);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = soft(ACCENT);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    let r = CornerRadius::same(6);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, SURFACE),
        (&mut v.widgets.inactive, SURFACE_2),
        (&mut v.widgets.hovered, LINE),
        (&mut v.widgets.active, soft(ACCENT)),
        (&mut v.widgets.open, SURFACE_2),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK_2);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.interact_size.y = 34.0;
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 8.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        let linear = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }

    #[test]
    fn calendar_uses_a_light_canvas_with_readable_text() {
        assert!(luminance(BG) > 0.85, "The default calendar canvas must be light");
        for text in [INK, INK_2, INK_3, ACCENT, DANGER] {
            for background in [BG, SURFACE, SURFACE_2] {
                let a = luminance(text);
                let b = luminance(background);
                let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(contrast >= 4.5, "Small UI text contrast is {contrast}");
            }
        }
    }
}
