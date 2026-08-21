//! Colour themes.
//!
//! A theme is three colours — a background, a foreground, and an accent — plus
//! whether it is a light or a dark design. Everything else libadwaita needs
//! (card backgrounds, header bars, popovers, the colour of text on an accent
//! button) is derived from those three by mixing, so adding a theme means adding
//! one line to `THEMES` rather than hand-picking a dozen shades.
//!
//! The output is a stylesheet of libadwaita `@define-color` overrides. That is
//! deliberately the same mechanism libadwaita uses for its own palette, so every
//! adaptive widget in the app — rows, header bars, dialogs, switches — follows
//! the theme without knowing it exists.
//!
//! This module is pure: it produces CSS text and never touches GTK, which is what
//! makes the colour maths testable. Loading the result into a display is
//! `ui::theming`'s job.

/// A 24-bit colour.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    /// Build a colour from a hex literal, written the way CSS writes it:
    /// `Rgb::hex(0x1e1e2e)`.
    pub const fn hex(value: u32) -> Self {
        Self {
            red: ((value >> 16) & 0xff) as u8,
            green: ((value >> 8) & 0xff) as u8,
            blue: (value & 0xff) as u8,
        }
    }

    /// CSS representation, for example `#1e1e2e`.
    pub fn to_css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
    }

    /// Blend towards another colour. `amount` is 0.0 (unchanged) to 1.0 (fully
    /// the other colour).
    pub fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let blend = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8
        };

        Self {
            red: blend(self.red, other.red),
            green: blend(self.green, other.green),
            blue: blend(self.blue, other.blue),
        }
    }

    /// Perceived brightness, 0.0 (black) to 1.0 (white).
    ///
    /// The weights come from the sRGB luma formula: human eyes are far more
    /// sensitive to green than to blue, so a plain average of the channels would
    /// call yellow and blue equally bright.
    pub fn brightness(self) -> f32 {
        (0.2126 * f32::from(self.red)
            + 0.7152 * f32::from(self.green)
            + 0.0722 * f32::from(self.blue))
            / 255.0
    }

    /// Black or white, whichever stays readable on top of this colour.
    pub fn readable_text(self) -> Self {
        if self.brightness() > 0.55 {
            Self::hex(0x000000)
        } else {
            Self::hex(0xffffff)
        }
    }
}

/// Whether a theme is a light or dark design, or defers to the desktop.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Variant {
    /// Follow the desktop's light/dark preference.
    System,
    Light,
    Dark,
}

/// The three colours a theme is built from.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Palette {
    /// Window background.
    pub background: Rgb,
    /// Primary text.
    pub foreground: Rgb,
    /// Accent used for buttons, switches and selections.
    pub accent: Rgb,
}

/// A selectable theme.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Stable identifier written to the config file. Never change these.
    pub id: &'static str,
    /// Name shown in Settings.
    pub name: &'static str,
    pub variant: Variant,
    /// `None` means "use libadwaita's own colours", so the three built-in
    /// entries do not need a palette.
    pub palette: Option<Palette>,
}

impl Theme {
    const fn plain(id: &'static str, name: &'static str, variant: Variant) -> Self {
        Self {
            id,
            name,
            variant,
            palette: None,
        }
    }

    const fn dark(id: &'static str, name: &'static str, bg: u32, fg: u32, accent: u32) -> Self {
        Self::palette(id, name, Variant::Dark, bg, fg, accent)
    }

    const fn light(id: &'static str, name: &'static str, bg: u32, fg: u32, accent: u32) -> Self {
        Self::palette(id, name, Variant::Light, bg, fg, accent)
    }

    const fn palette(
        id: &'static str,
        name: &'static str,
        variant: Variant,
        bg: u32,
        fg: u32,
        accent: u32,
    ) -> Self {
        Self {
            id,
            name,
            variant,
            palette: Some(Palette {
                background: Rgb::hex(bg),
                foreground: Rgb::hex(fg),
                accent: Rgb::hex(accent),
            }),
        }
    }

    /// Stylesheet overriding libadwaita's named colours, or `None` when this
    /// theme uses libadwaita's own palette.
    pub fn css(&self) -> Option<String> {
        let palette = self.palette?;
        Some(build_css(&palette))
    }
}

/// How far "raised" surfaces sit from the window background.
///
/// Cards, header bars and popovers all need to separate from the window without
/// introducing new hand-picked colours, so each is the background nudged a fixed
/// amount towards the foreground.
const HEADERBAR_LIFT: f32 = 0.04;
const CARD_LIFT: f32 = 0.07;
const VIEW_LIFT: f32 = 0.02;
const POPOVER_LIFT: f32 = 0.09;

fn build_css(palette: &Palette) -> String {
    let Palette {
        background,
        foreground,
        accent,
    } = *palette;

    let headerbar = background.mix(foreground, HEADERBAR_LIFT);
    let view = background.mix(foreground, VIEW_LIFT);
    let card = background.mix(foreground, CARD_LIFT);
    let popover = background.mix(foreground, POPOVER_LIFT);
    // Text sitting on an accent-filled button has to contrast with the accent,
    // not with the window, so it is chosen from the accent's own brightness.
    let on_accent = accent.readable_text();

    // `accent_color` is the standalone version used for text and icons.
    // libadwaita expects it to be legible against the *window*, so on a dark
    // theme a saturated accent is lightened and on a light theme darkened.
    let accent_text = match foreground.brightness() > 0.5 {
        true => accent.mix(Rgb::hex(0xffffff), 0.2),
        false => accent.mix(Rgb::hex(0x000000), 0.15),
    };

    let definitions = [
        ("window_bg_color", background),
        ("window_fg_color", foreground),
        ("view_bg_color", view),
        ("view_fg_color", foreground),
        ("headerbar_bg_color", headerbar),
        ("headerbar_fg_color", foreground),
        ("headerbar_border_color", foreground),
        ("headerbar_backdrop_color", background),
        ("sidebar_bg_color", headerbar),
        ("sidebar_fg_color", foreground),
        ("sidebar_backdrop_color", background),
        ("secondary_sidebar_bg_color", headerbar),
        ("secondary_sidebar_fg_color", foreground),
        ("card_bg_color", card),
        ("card_fg_color", foreground),
        ("dialog_bg_color", popover),
        ("dialog_fg_color", foreground),
        ("popover_bg_color", popover),
        ("popover_fg_color", foreground),
        ("accent_bg_color", accent),
        ("accent_fg_color", on_accent),
        ("accent_color", accent_text),
    ];

    let mut css = String::with_capacity(definitions.len() * 48);
    for (name, color) in definitions {
        css.push_str("@define-color ");
        css.push_str(name);
        css.push(' ');
        css.push_str(&color.to_css());
        css.push_str(";\n");
    }
    css
}

/// Identifier of the theme that follows the desktop.
///
/// This is `"default"` rather than `"system"` because that is the value already
/// written to existing config files.
pub const DEFAULT_THEME_ID: &str = "default";

/// Every selectable theme, in the order they appear in Settings.
///
/// The first three defer to libadwaita's own palette; the rest are ports of
/// well-known editor and desktop colour schemes.
pub const THEMES: &[Theme] = &[
    Theme::plain(DEFAULT_THEME_ID, "System Default", Variant::System),
    Theme::plain("adwaita-light", "Adwaita Light", Variant::Light),
    Theme::plain("adwaita-dark", "Adwaita Dark", Variant::Dark),
    // AirGradient's own brand colours, taken from the design tokens published in
    // airgradient.com's stylesheet: the primary blue `--primaryColor500`, the
    // body text colour `--main-text-color` on the site's off-white page tint,
    // and for the dark entry the `--dark-mode-page-bg` and `--dark-mode-accent`
    // the site switches to. The blue is lighter in dark mode for the same reason
    // the site lightens it: the brand blue is too dim to read against navy.
    Theme::light("airgradient", "AirGradient", 0xf5f8fb, 0x212121, 0x1c75bc),
    Theme::dark(
        "airgradient-dark",
        "AirGradient Dark",
        0x0f172a,
        0xf5f8fb,
        0x60a5fa,
    ),
    // Catppuccin
    Theme::light(
        "catppuccin-latte",
        "Catppuccin Latte",
        0xeff1f5,
        0x4c4f69,
        0x1e66f5,
    ),
    Theme::dark(
        "catppuccin-frappe",
        "Catppuccin Frappé",
        0x303446,
        0xc6d0f5,
        0x8caaee,
    ),
    Theme::dark(
        "catppuccin-macchiato",
        "Catppuccin Macchiato",
        0x24273a,
        0xcad3f5,
        0x8aadf4,
    ),
    Theme::dark(
        "catppuccin-mocha",
        "Catppuccin Mocha",
        0x1e1e2e,
        0xcdd6f4,
        0x89b4fa,
    ),
    // Nord and friends
    Theme::dark("nord", "Nord", 0x2e3440, 0xd8dee9, 0x88c0d0),
    Theme::light("nord-light", "Nord Light", 0xeceff4, 0x2e3440, 0x5e81ac),
    Theme::dark("polar-night", "Polar Night", 0x242933, 0xe5e9f0, 0x81a1c1),
    // Dracula family
    Theme::dark("dracula", "Dracula", 0x282a36, 0xf8f8f2, 0xbd93f9),
    Theme::dark("dracula-pro", "Dracula Pro", 0x22212c, 0xf8f8f2, 0xff80bf),
    // Gruvbox
    Theme::dark("gruvbox-dark", "Gruvbox Dark", 0x282828, 0xebdbb2, 0xd79921),
    Theme::dark(
        "gruvbox-dark-hard",
        "Gruvbox Dark Hard",
        0x1d2021,
        0xebdbb2,
        0xfabd2f,
    ),
    Theme::light(
        "gruvbox-light",
        "Gruvbox Light",
        0xfbf1c7,
        0x3c3836,
        0xaf3a03,
    ),
    // Solarized
    Theme::dark(
        "solarized-dark",
        "Solarized Dark",
        0x002b36,
        0x93a1a1,
        0x268bd2,
    ),
    Theme::light(
        "solarized-light",
        "Solarized Light",
        0xfdf6e3,
        0x586e75,
        0x268bd2,
    ),
    // Tokyo Night
    Theme::dark("tokyo-night", "Tokyo Night", 0x1a1b26, 0xc0caf5, 0x7aa2f7),
    Theme::dark(
        "tokyo-night-storm",
        "Tokyo Night Storm",
        0x24283b,
        0xc0caf5,
        0x7aa2f7,
    ),
    Theme::light(
        "tokyo-night-day",
        "Tokyo Night Day",
        0xe1e2e7,
        0x3760bf,
        0x2e7de9,
    ),
    // Everforest
    Theme::dark(
        "everforest-dark",
        "Everforest Dark",
        0x2d353b,
        0xd3c6aa,
        0xa7c080,
    ),
    Theme::light(
        "everforest-light",
        "Everforest Light",
        0xfdf6e3,
        0x5c6a72,
        0x8da101,
    ),
    // Rosé Pine
    Theme::dark("rose-pine", "Rosé Pine", 0x191724, 0xe0def4, 0xebbcba),
    Theme::dark(
        "rose-pine-moon",
        "Rosé Pine Moon",
        0x232136,
        0xe0def4,
        0xea9a97,
    ),
    Theme::light(
        "rose-pine-dawn",
        "Rosé Pine Dawn",
        0xfaf4ed,
        0x575279,
        0xd7827e,
    ),
    // Editor classics
    Theme::dark("one-dark", "One Dark", 0x282c34, 0xabb2bf, 0x61afef),
    Theme::light("one-light", "One Light", 0xfafafa, 0x383a42, 0x4078f2),
    Theme::dark("monokai", "Monokai", 0x272822, 0xf8f8f2, 0xa6e22e),
    Theme::dark("monokai-pro", "Monokai Pro", 0x2d2a2e, 0xfcfcfa, 0xffd866),
    Theme::dark("zenburn", "Zenburn", 0x3f3f3f, 0xdcdccc, 0x8cd0d3),
    Theme::dark("cobalt", "Cobalt", 0x193549, 0xe1efff, 0xffc600),
    Theme::dark("oceanic-next", "Oceanic Next", 0x1b2b34, 0xcdd3de, 0x6699cc),
    Theme::dark("night-owl", "Night Owl", 0x011627, 0xd6deeb, 0x82aaff),
    Theme::light("light-owl", "Light Owl", 0xfbfbfb, 0x403f53, 0x2aa298),
    Theme::dark("horizon", "Horizon", 0x1c1e26, 0xd5d8da, 0xe95678),
    Theme::dark("synthwave", "Synthwave", 0x241b2f, 0xf8f8f2, 0xff7edb),
    // Material
    Theme::dark(
        "material-ocean",
        "Material Ocean",
        0x0f111a,
        0xa6accd,
        0x84ffff,
    ),
    Theme::dark(
        "material-palenight",
        "Material Palenight",
        0x292d3e,
        0xa6accd,
        0xc792ea,
    ),
    Theme::dark(
        "material-darker",
        "Material Darker",
        0x212121,
        0xeeffff,
        0x89ddff,
    ),
    Theme::light(
        "material-lighter",
        "Material Lighter",
        0xfafafa,
        0x546e7a,
        0x39adb5,
    ),
    // Ayu
    Theme::dark("ayu-dark", "Ayu Dark", 0x0f1419, 0xe6e1cf, 0xff8f40),
    Theme::dark("ayu-mirage", "Ayu Mirage", 0x1f2430, 0xcbccc6, 0xffcc66),
    Theme::light("ayu-light", "Ayu Light", 0xfafafa, 0x5c6166, 0xff9940),
    // Others
    Theme::dark("kanagawa", "Kanagawa", 0x1f1f28, 0xdcd7ba, 0x7e9cd8),
    Theme::dark("melange-dark", "Melange Dark", 0x292522, 0xece1d7, 0xa3a9ce),
    Theme::light(
        "melange-light",
        "Melange Light",
        0xf1f1f1,
        0x54433a,
        0x6e9b72,
    ),
    Theme::dark("iceberg-dark", "Iceberg Dark", 0x161821, 0xc6c8d1, 0x84a0c6),
    Theme::light(
        "iceberg-light",
        "Iceberg Light",
        0xe8e9ec,
        0x33374c,
        0x2d539e,
    ),
    Theme::dark("github-dark", "GitHub Dark", 0x0d1117, 0xc9d1d9, 0x58a6ff),
    Theme::light("github-light", "GitHub Light", 0xffffff, 0x24292f, 0x0969da),
    Theme::dark("carbon", "Carbon", 0x161616, 0xf4f4f4, 0x4589ff),
    Theme::dark("forest-night", "Forest Night", 0x1b2229, 0xd8caac, 0x87a987),
    Theme::light("sepia", "Sepia", 0xf4ecd8, 0x5b4636, 0x8c6d3f),
    Theme::light("mint", "Mint", 0xf2fbf6, 0x1f3d2f, 0x0f9d58),
    Theme::light("lavender", "Lavender", 0xf6f4fb, 0x342e4a, 0x6c5ce7),
    Theme::dark("sunset", "Sunset", 0x241a24, 0xf6e0d2, 0xf4845f),
    Theme::dark("slate", "Slate", 0x1e232b, 0xd7dbe0, 0x5eaaa8),
];

/// Look a theme up by the identifier stored in the config file.
///
/// Unknown identifiers fall back to the default rather than failing, so a config
/// written by a newer version of the app still opens.
pub fn find(id: &str) -> &'static Theme {
    THEMES
        .iter()
        .find(|theme| theme.id == id)
        .unwrap_or(&THEMES[0])
}

/// Names in dropdown order.
pub fn names() -> Vec<&'static str> {
    THEMES.iter().map(|theme| theme.name).collect()
}

/// Position of a theme in the dropdown.
pub fn index_of(id: &str) -> u32 {
    THEMES
        .iter()
        .position(|theme| theme.id == id)
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0)
}

/// Theme at a dropdown position.
pub fn at_index(index: u32) -> &'static Theme {
    THEMES.get(index as usize).unwrap_or(&THEMES[0])
}

#[cfg(test)]
mod tests {
    use super::{at_index, find, index_of, names, Rgb, Variant, DEFAULT_THEME_ID, THEMES};

    #[test]
    fn ships_at_least_forty_themes() {
        assert!(
            THEMES.len() >= 40,
            "expected 40+ themes, found {}",
            THEMES.len()
        );
    }

    #[test]
    fn theme_ids_are_unique() {
        let mut ids: Vec<&str> = THEMES.iter().map(|theme| theme.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();

        assert_eq!(ids.len(), count, "duplicate theme id");
    }

    #[test]
    fn theme_names_are_unique() {
        let mut names = names();
        names.sort_unstable();
        let count = names.len();
        names.dedup();

        assert_eq!(names.len(), count, "duplicate theme name");
    }

    #[test]
    fn default_theme_follows_the_system_and_has_no_palette() {
        let theme = find(DEFAULT_THEME_ID);

        assert_eq!(theme.variant, Variant::System);
        assert!(theme.palette.is_none());
        assert!(theme.css().is_none());
    }

    #[test]
    fn unknown_id_falls_back_to_default() {
        assert_eq!(find("no-such-theme").id, DEFAULT_THEME_ID);
    }

    #[test]
    fn dropdown_index_round_trips() {
        for theme in THEMES {
            assert_eq!(at_index(index_of(theme.id)).id, theme.id);
        }
    }

    #[test]
    fn every_palette_theme_defines_the_libadwaita_colors() {
        for theme in THEMES.iter().filter(|theme| theme.palette.is_some()) {
            let css = theme.css().expect("palette themes produce css");

            for name in [
                "window_bg_color",
                "window_fg_color",
                "headerbar_bg_color",
                "card_bg_color",
                "popover_bg_color",
                "accent_bg_color",
                "accent_fg_color",
                "accent_color",
            ] {
                assert!(
                    css.contains(&format!("@define-color {name} #")),
                    "{} is missing {name}",
                    theme.id
                );
            }
        }
    }

    #[test]
    fn palette_backgrounds_match_the_declared_variant() {
        for theme in THEMES {
            let Some(palette) = theme.palette else {
                continue;
            };

            match theme.variant {
                Variant::Dark => assert!(
                    palette.background.brightness() < 0.5,
                    "{} is declared dark but has a light background",
                    theme.id
                ),
                Variant::Light => assert!(
                    palette.background.brightness() > 0.5,
                    "{} is declared light but has a dark background",
                    theme.id
                ),
                Variant::System => panic!("{} should not carry a palette", theme.id),
            }
        }
    }

    #[test]
    fn foreground_contrasts_with_background() {
        // A theme where text and background have similar brightness would be
        // unreadable, and is the easiest mistake to make when adding one.
        for theme in THEMES {
            let Some(palette) = theme.palette else {
                continue;
            };
            let contrast =
                (palette.background.brightness() - palette.foreground.brightness()).abs();

            assert!(
                contrast > 0.3,
                "{} has too little contrast ({contrast:.2})",
                theme.id
            );
        }
    }

    #[test]
    fn text_on_accent_is_readable() {
        for theme in THEMES {
            let Some(palette) = theme.palette else {
                continue;
            };
            let on_accent = palette.accent.readable_text();
            let contrast = (palette.accent.brightness() - on_accent.brightness()).abs();

            assert!(
                contrast > 0.4,
                "{} accent text contrast is only {contrast:.2}",
                theme.id
            );
        }
    }

    #[test]
    fn the_airgradient_themes_use_the_brand_blue() {
        // The identifiers are written to config files, and the accent is what
        // makes the theme recognisably AirGradient's, so both are pinned here.
        let light = find("airgradient").palette.expect("light palette");
        assert_eq!(light.accent.to_css(), "#1c75bc");

        let dark = find("airgradient-dark").palette.expect("dark palette");
        assert_eq!(dark.background.to_css(), "#0f172a");
    }

    #[test]
    fn hex_parsing_and_formatting_round_trip() {
        assert_eq!(Rgb::hex(0x1e1e2e).to_css(), "#1e1e2e");
        assert_eq!(Rgb::hex(0xffffff).to_css(), "#ffffff");
        assert_eq!(Rgb::hex(0x000000).to_css(), "#000000");
    }

    #[test]
    fn mixing_moves_towards_the_other_color() {
        let black = Rgb::hex(0x000000);
        let white = Rgb::hex(0xffffff);

        assert_eq!(black.mix(white, 0.0), black);
        assert_eq!(black.mix(white, 1.0), white);
        assert_eq!(black.mix(white, 0.5).to_css(), "#808080");
    }

    #[test]
    fn readable_text_picks_the_contrasting_extreme() {
        assert_eq!(Rgb::hex(0xffffff).readable_text(), Rgb::hex(0x000000));
        assert_eq!(Rgb::hex(0x000000).readable_text(), Rgb::hex(0xffffff));
    }
}
