//! Calendio: paper surfaces, ink typography and a blue action accent.

use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    White,
    Dark,
    Catppuccin,
    TokyoNight,
    Gruvbox,
    Everforest,
    RosePine,
    Nord,
}

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    surface: Color32,
    surface_2: Color32,
    line: Color32,
    ink: Color32,
    ink_2: Color32,
    ink_3: Color32,
    accent: Color32,
    danger: Color32,
    dark: bool,
}

impl Theme {
    pub const ALL: [Self; 9] = [
        Self::System,
        Self::White,
        Self::Dark,
        Self::Catppuccin,
        Self::TokyoNight,
        Self::Gruvbox,
        Self::Everforest,
        Self::RosePine,
        Self::Nord,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::White => "white",
            Self::Dark => "dark",
            Self::Catppuccin => "catppuccin",
            Self::TokyoNight => "tokyo-night",
            Self::Gruvbox => "gruvbox",
            Self::Everforest => "everforest",
            Self::RosePine => "rose-pine",
            Self::Nord => "nord",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::White => "White",
            Self::Dark => "Dark",
            Self::Catppuccin => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::Gruvbox => "Gruvbox",
            Self::Everforest => "Everforest",
            Self::RosePine => "Rosé Pine",
            Self::Nord => "Nord",
        }
    }
    pub fn parse(id: &str) -> Self {
        Self::ALL.into_iter().find(|t| t.id() == id).unwrap_or_default()
    }
    fn palette(self, system_dark: bool) -> Palette {
        let colors = match self {
            Self::System => return if system_dark { Self::Dark } else { Self::White }.palette(false),
            Self::White => [0xfafbf9, 0xffffff, 0xeef1f0, 0xd6ddda, 0x202c32, 0x42535d, 0x566871, 0x2457b2, 0xa72e49],
            Self::Dark => [0x16191f, 0x1e222a, 0x272d37, 0x424b5a, 0xe8edf4, 0xc7d0dd, 0xa5b2c4, 0x8ab4f8, 0xff9bad],
            Self::Catppuccin => {
                [0x1e1e2e, 0x181825, 0x313244, 0x585b70, 0xcdd6f4, 0xbac2de, 0xa6adc8, 0xcba6f7, 0xf38ba8]
            }
            Self::TokyoNight => {
                [0x1a1b26, 0x16161e, 0x24283b, 0x414868, 0xc0caf5, 0xa9b1d6, 0x9aa5ce, 0x7aa2f7, 0xf7768e]
            }
            Self::Gruvbox => [0x282828, 0x1d2021, 0x3c3836, 0x665c54, 0xebdbb2, 0xd5c4a1, 0xbdae93, 0xfabd2f, 0xfb4934],
            Self::Everforest => {
                [0x2d353b, 0x232a2e, 0x343f44, 0x56635f, 0xd3c6aa, 0xc5c2a4, 0xb2c2ad, 0xa7c080, 0xe67e80]
            }
            Self::RosePine => {
                [0x191724, 0x1f1d2e, 0x26233a, 0x524f67, 0xe0def4, 0xc4bfd6, 0xa9a3bd, 0xc4a7e7, 0xeb6f92]
            }
            Self::Nord => [0x2e3440, 0x272d38, 0x3b4252, 0x596579, 0xeceff4, 0xd8dee9, 0xb8c5d6, 0x88c0d0, 0xf0a1ad],
        };
        let c = |i: usize| {
            let n = colors[i];
            Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
        };
        Palette {
            bg: c(0),
            surface: c(1),
            surface_2: c(2),
            line: c(3),
            ink: c(4),
            ink_2: c(5),
            ink_3: c(6),
            accent: c(7),
            danger: c(8),
            dark: self != Self::White,
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        let palette = self.palette(ctx.system_theme() == Some(egui::Theme::Dark));
        PALETTE.with(|p| p.set(palette));
        apply(ctx);
    }
}
thread_local! { static PALETTE: std::cell::Cell<Palette> = std::cell::Cell::new(Theme::White.palette(false)); }
macro_rules! color {
    ($name:ident,$field:ident) => {
        pub fn $name() -> Color32 {
            PALETTE.with(|p| p.get().$field)
        }
    };
}
color!(bg, bg);
color!(surface, surface);
color!(surface_2, surface_2);
color!(line, line);
color!(ink, ink);
color!(ink_2, ink_2);
color!(ink_3, ink_3);
color!(accent, accent);
color!(danger, danger);
pub fn pink() -> Color32 {
    danger()
}

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
    Color32::from_rgb(mix(c.r(), bg().r()), mix(c.g(), bg().g()), mix(c.b(), bg().b()))
}

pub fn apply(ctx: &egui::Context) {
    let mut v = if PALETTE.with(|p| p.get().dark) { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = bg();
    v.window_fill = surface();
    v.window_stroke = Stroke::new(1.0, line());
    v.window_corner_radius = CornerRadius::same(10);
    v.extreme_bg_color = surface_2();
    v.faint_bg_color = surface();
    v.override_text_color = Some(ink());
    v.hyperlink_color = accent();
    v.selection.bg_fill = soft(accent());
    v.selection.stroke = Stroke::new(1.0, accent());
    let r = CornerRadius::same(6);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, surface()),
        (&mut v.widgets.inactive, surface_2()),
        (&mut v.widgets.hovered, line()),
        (&mut v.widgets.active, soft(accent())),
        (&mut v.widgets.open, surface_2()),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, line());
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ink_2());
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent());
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
    fn themes_resolve_and_follow_system() {
        assert_eq!(Theme::ALL.len(), 9);
        for theme in Theme::ALL {
            assert_eq!(Theme::parse(theme.id()), theme);
            let p = theme.palette(false);
            for text in [p.ink, p.ink_2, p.ink_3] {
                for bg in [p.bg, p.surface, p.surface_2] {
                    let a = luminance(text);
                    let b = luminance(bg);
                    assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5, "{} text contrast", theme.label());
                }
            }
        }
        assert_eq!(Theme::System.palette(true).bg, Theme::Dark.palette(false).bg);
        assert_eq!(Theme::System.palette(false).bg, Theme::White.palette(false).bg);
        assert_eq!(Theme::parse("unknown"), Theme::System);
    }

    #[test]
    fn calendar_uses_a_light_canvas_with_readable_text() {
        Theme::White.apply(&egui::Context::default());
        assert!(luminance(bg()) > 0.85, "The default calendar canvas must be light");
        for text in [ink(), ink_2(), ink_3(), accent(), danger()] {
            for background in [bg(), surface(), surface_2()] {
                let a = luminance(text);
                let b = luminance(background);
                let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(contrast >= 4.5, "Small UI text contrast is {contrast}");
            }
        }
    }
}
