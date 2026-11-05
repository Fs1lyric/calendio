//! Calendio's look: Apple Calendar's white canvas, hairline grid and system red
//! by default, set in Inter, with the older palettes kept as alternatives.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, Visuals};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    /// Apple light or dark, whichever the system asks for.
    #[default]
    System,
    AppleLight,
    AppleDark,
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
    /// Today's date circle and the now line.
    today: Color32,
    dark: bool,
}

impl Theme {
    pub const ALL: [Self; 11] = [
        Self::System,
        Self::AppleLight,
        Self::AppleDark,
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
            Self::AppleLight => "apple-light",
            Self::AppleDark => "apple-dark",
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
            Self::AppleLight => "Apple Light",
            Self::AppleDark => "Apple Dark",
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
            Self::System => return if system_dark { Self::AppleDark } else { Self::AppleLight }.palette(false),
            Self::AppleLight => {
                [0xffffff, 0xf5f5f7, 0xebebef, 0xe5e5e5, 0x1d1d1f, 0x48484a, 0x636366, 0x0062cc, 0xd70015]
            }
            Self::AppleDark => {
                [0x1e1e1e, 0x262626, 0x323232, 0x3a3a3a, 0xf5f5f7, 0xd1d1d6, 0xa1a1a6, 0x4ea6ff, 0xff6961]
            }
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
            today: match self {
                Self::AppleLight => Color32::from_rgb(0xff, 0x3b, 0x30),
                Self::AppleDark => Color32::from_rgb(0xff, 0x45, 0x3a),
                _ => c(7),
            },
            dark: !matches!(self, Self::White | Self::AppleLight),
        }
    }
    pub fn apply(self, ctx: &egui::Context) {
        let palette = self.palette(ctx.system_theme() == Some(egui::Theme::Dark));
        PALETTE.with(|p| p.set(palette));
        apply(ctx);
    }
}
thread_local! { static PALETTE: std::cell::Cell<Palette> = std::cell::Cell::new(Theme::AppleLight.palette(false)); }
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
color!(today, today);

fn is_dark() -> bool {
    PALETTE.with(|p| p.get().dark)
}

/// The chosen segment of a segmented control: white on light themes, a lifted grey on dark.
pub fn raised() -> Color32 {
    if is_dark() {
        surface_2().lerp_to_gamma(ink(), 0.22)
    } else {
        Color32::WHITE
    }
}

/// A faint tint under a hovered borderless control.
pub fn hover() -> Color32 {
    ink().gamma_multiply(0.07)
}

/// The soft drop shadow under a raised control.
pub fn shadow() -> Color32 {
    Color32::from_black_alpha(if is_dark() { 90 } else { 28 })
}

/// Inter (OFL, `assets/Inter-LICENSE.txt`) at three weights. egui has no font
/// weights, so SemiBold and Bold are named families; the default fonts stay
/// behind each one for symbols and emoji Inter lacks.
fn font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    for (name, data, family) in [
        ("Inter", &include_bytes!("../assets/Inter-Regular.ttf")[..], FontFamily::Proportional),
        ("Inter SemiBold", &include_bytes!("../assets/Inter-SemiBold.ttf")[..], FontFamily::Name("semibold".into())),
        ("Inter Bold", &include_bytes!("../assets/Inter-Bold.ttf")[..], FontFamily::Name("bold".into())),
    ] {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(data)));
        let mut list = vec![name.to_owned()];
        list.extend(fallbacks.iter().cloned());
        fonts.families.insert(family, list);
    }
    fonts
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

/// Loads the fonts once per context; they take effect from the next frame.
fn install_fonts(ctx: &egui::Context) {
    let id = egui::Id::new("calendio-fonts");
    if ctx.data(|d| d.get_temp::<bool>(id).is_none()) {
        ctx.set_fonts(font_definitions());
        ctx.data_mut(|d| d.insert_temp(id, true));
    }
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
    install_fonts(ctx);
    let mut v = if is_dark() { Visuals::dark() } else { Visuals::light() };
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
        assert_eq!(Theme::ALL.len(), 11);
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
        assert_eq!(Theme::System.palette(true).bg, Theme::AppleDark.palette(false).bg);
        assert_eq!(Theme::System.palette(false).bg, Theme::AppleLight.palette(false).bg);
        assert_eq!(Theme::parse("unknown"), Theme::System);
    }

    fn rgb(n: u32) -> Color32 {
        Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
    }

    #[test]
    fn apple_themes_use_a_white_or_charcoal_canvas_hairlines_and_system_red() {
        let light = Theme::AppleLight.palette(false);
        assert_eq!((light.bg, light.line, light.today), (rgb(0xffffff), rgb(0xe5e5e5), rgb(0xff3b30)));
        assert!(!light.dark);
        let dark = Theme::AppleDark.palette(false);
        assert_eq!((dark.bg, dark.line, dark.today), (rgb(0x1e1e1e), rgb(0x3a3a3a), rgb(0xff453a)));
        assert!(dark.dark);
        assert_eq!(Theme::parse("apple-light"), Theme::AppleLight);
        assert_eq!(Theme::parse("apple-dark"), Theme::AppleDark);
    }

    #[test]
    fn the_default_theme_is_apple_and_follows_the_system() {
        assert_eq!(Theme::default().palette(false).bg, Theme::AppleLight.palette(false).bg);
        assert_eq!(Theme::default().palette(true).bg, Theme::AppleDark.palette(false).bg);
        assert_eq!(Theme::parse(""), Theme::default());
    }

    #[test]
    fn apple_accent_and_danger_text_stay_readable() {
        for theme in [Theme::AppleLight, Theme::AppleDark] {
            let p = theme.palette(false);
            for text in [p.accent, p.danger] {
                for background in [p.bg, p.surface, p.surface_2] {
                    let a = luminance(text);
                    let b = luminance(background);
                    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                    assert!(contrast >= 4.5, "{} accent/danger contrast is {contrast}", theme.label());
                }
            }
        }
    }

    #[test]
    fn inter_is_the_ui_font_with_semibold_and_bold_weights() {
        let ctx = egui::Context::default();
        Theme::AppleLight.apply(&ctx);
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label(egui::RichText::new("November").font(bold(28.0)));
            ui.label(egui::RichText::new("Standup").font(semibold(12.0)));
        });
        let families = ctx.fonts(|f| f.definitions().families.clone());
        assert_eq!(families[&egui::FontFamily::Proportional][0], "Inter");
        assert_eq!(families[&egui::FontFamily::Name("semibold".into())][0], "Inter SemiBold");
        assert_eq!(families[&egui::FontFamily::Name("bold".into())][0], "Inter Bold");
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
