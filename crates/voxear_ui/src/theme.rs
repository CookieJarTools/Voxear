//! Themes made from three choices, as Linear makes its own: the colour of
//! the ground under everything, an accent, and how far text and lines stand
//! out. Every `--vx-*` colour of the default theme is made from them in
//! OKLCH, where equal steps of lightness look equal whatever the hue, so a
//! dark theme, a light one and one of your own all hold together.
//!
//! [`Theme::DARK`] gives the default theme's own colours, near enough that
//! no one could tell. [`Ui::set_theme`] puts a theme's colours on a UI, over
//! the default theme's and under any sheet of your own.

use std::fmt::Write;

use crate::{Color, StyleSheet, Ui};

/// What a theme's sheet is called, in warnings and among a root's sheets.
const SOURCE: &str = "Theme";

/// The surfaces and lines: where each one's lightness lies from the panel
/// (0) towards the text (1) at the default contrast, and how much it is
/// tinted by the base's colour and the accent's, as parts of their chroma.
const SURFACES: [(&str, f32, f32, f32); 9] = [
    ("panel", 0.0, 0.4, 0.015),
    ("overlay", 0.019, 0.4, 0.045),
    ("field", 0.049, 0.4, 0.055),
    ("hover", 0.06, 0.4, 0.045),
    ("line", 0.075, 0.4, 0.03),
    ("line-2", 0.127, 0.4, 0.055),
    ("field-line", 0.127, 0.4, 0.055),
    ("selected", 0.127, 0.4, 0.08),
    ("seg-on", 0.191, 0.4, 0.08),
];

/// The three kinds of text, in the same way.
const TEXTS: [(&str, f32, f32, f32); 3] =
    [("muted", 0.686, 1.6, 0.045), ("text-2", 0.86, 1.2, 0.01), ("text", 1.0, 0.4, 0.0)];

/// The colours that keep their hue in every theme.
const FIXED: [(&str, u32); 7] = [
    ("success", 0x4cb782),
    ("warning", 0xe2b340),
    ("prefab", 0x7cb1ff),
    ("prefab-member", 0xa3c6f5),
    ("x", 0xeb5757),
    ("y", 0x4cb782),
    ("z", 0x5e8bf0),
];

/// A theme, from three choices; see the module's notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    /// The ground under every panel. A dark one makes a dark theme, with
    /// panels and text lighter than it, and a light one a light theme.
    pub base: Color,
    /// Selection, focus, links and the main buttons.
    pub accent: Color,
    /// How far text and lines stand out from the panels, from 0 to 1; 0.5
    /// is the default.
    pub contrast: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

impl Theme {
    /// The default theme: near-black panels and a periwinkle accent.
    pub const DARK: Theme = Theme { base: Color::hex(0x0c0e10), accent: Color::hex(0x7988ff), contrast: 0.5 };
    /// White panels on a pale grey ground, with a deeper accent that reads
    /// on white.
    pub const LIGHT: Theme = Theme { base: Color::hex(0xf4f5f7), accent: Color::hex(0x5e6ad2), contrast: 0.5 };

    /// Whether the base is dark, and so the theme.
    pub fn is_dark(&self) -> bool {
        self.base.to_oklch()[0] < 0.6
    }

    /// Every colour the default theme names, without its `--vx-` prefix.
    pub fn tokens(&self) -> Vec<(&'static str, Color)> {
        let [ground, base_chroma, base_hue] = self.base.to_oklch();
        let [accent_lightness, accent_chroma, accent_hue] = self.accent.to_oklch();
        let contrast = self.contrast.clamp(0.0, 1.0);
        let dark = self.is_dark();
        // Panels sit a little lighter than the ground in both, and text runs
        // from them towards white or black. Near white, steps of lightness
        // show less on screen, so a light theme takes bigger ones.
        let (panel, far, reach, steps) = if dark {
            (ground + 0.056, 1.0, 0.86 + 0.14 * contrast, 0.5 + contrast)
        } else {
            ((ground + 0.03).min(1.0), 0.0, 0.62 + 0.2 * contrast, (0.5 + contrast) * 1.5)
        };
        let text = panel + (far - panel) * reach;
        let tint = |chroma: f32, hue: f32| {
            let (sin, cos) = hue.to_radians().sin_cos();
            [chroma * cos, chroma * sin]
        };
        let (base, accent) = (tint(base_chroma, base_hue), tint(accent_chroma, accent_hue));
        // The base's tint is kept faint, so a strongly coloured ground does
        // not turn every panel and word that colour.
        let base = base.map(|v| v.clamp(-0.02, 0.02));
        let mix = |along: f32, of_base: f32, of_accent: f32| {
            let lightness = panel + (text - panel) * along.min(1.0);
            let a = base[0] * of_base + accent[0] * of_accent;
            let b = base[1] * of_base + accent[1] * of_accent;
            Color::oklab(lightness, a, b, 1.0)
        };

        let mut tokens = vec![("bg", self.base.with_alpha(0xff))];
        for (name, along, of_base, of_accent) in SURFACES {
            let color = mix(along * steps, of_base, of_accent);
            tokens.push((name, if name == "overlay" { color.with_alpha(235) } else { color }));
            if name == "overlay" {
                // Frosted glass: the overlay's colour, thinner, over the blurred view.
                tokens.push(("glass", color.with_alpha(153)));
            }
        }
        for (name, along, of_base, of_accent) in TEXTS {
            let along = if name == "text" { along } else { along + (contrast - 0.5) * 0.15 };
            tokens.push((name, mix(along, of_base, of_accent)));
        }
        let accent = self.accent.with_alpha(0xff);
        let hover = if dark {
            Color::oklch(accent_lightness + 0.04, accent_chroma * 0.87, accent_hue, 1.0)
        } else {
            Color::oklch(accent_lightness - 0.04, accent_chroma, accent_hue, 1.0)
        };
        let on_accent = if accent_lightness > 0.75 { Color::oklch(0.2, 0.0, 0.0, 1.0) } else { Color::WHITE };
        tokens.extend([("accent", accent), ("accent-hover", hover), ("accent-soft", accent.with_alpha(51))]);
        tokens.push(("on-accent", on_accent));
        for (name, hex) in FIXED {
            // Darker on a light theme, so they read on white, and closer
            // together, keeping their order.
            let [lightness, chroma, hue] = Color::hex(hex).to_oklch();
            let light = 0.56 + (lightness - 0.7) * 0.5;
            let color = if dark { Color::hex(hex) } else { Color::oklch(light, chroma, hue, 1.0) };
            tokens.push((name, color));
        }
        tokens
    }

    /// The tokens as a sheet for a UI's root.
    pub fn sheet(&self) -> StyleSheet {
        let mut text = String::from(":root {\n");
        for (name, color) in self.tokens() {
            let _ = writeln!(text, "    --vx-{name}: {};", color.css());
        }
        text.push('}');
        StyleSheet::parse(SOURCE, &text)
    }
}

impl Ui {
    /// Colours everything with `theme`. Its sheet goes on the root after the
    /// default theme and before any sheet of your own, so your sheets still
    /// win; a theme set again replaces the last.
    pub fn set_theme(&mut self, theme: &Theme) {
        let sheet = std::rc::Rc::new(theme.sheet());
        let root = self.root();
        let Some(element) = self.element_mut(root) else { return };
        let sheets = &mut element.sheets;
        match sheets.iter().position(|s| s.source == SOURCE) {
            Some(at) => sheets[at] = sheet,
            None => {
                let at = sheets.iter().position(|s| s.source == "Default.vss").map_or(0, |at| at + 1);
                sheets.insert(at, sheet);
            }
        }
        self.restyle = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How far apart two colours look, in OKLab, where about 0.02 is the
    /// least most people notice.
    fn apart(one: Color, other: Color) -> f32 {
        let lab = |c: Color| {
            let [l, chroma, hue] = c.to_oklch();
            let (sin, cos) = hue.to_radians().sin_cos();
            [l, chroma * cos, chroma * sin]
        };
        let (a, b) = (lab(one), lab(other));
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    }

    /// The default theme's own value for a token, as `Default.vss` writes it.
    fn default_token(name: &str) -> Color {
        let line = crate::DEFAULT_THEME
            .lines()
            .find_map(|line| line.trim().strip_prefix(&format!("--vx-{name}: ")))
            .unwrap_or_else(|| panic!("Default.vss has no --vx-{name}"));
        let mut input = cssparser::ParserInput::new(line.trim_end_matches(';'));
        crate::properties::color(&mut cssparser::Parser::new(&mut input)).unwrap()
    }

    /// WCAG's contrast ratio, from 1 to 21.
    fn ratio(one: Color, other: Color) -> f32 {
        let luminance = |c: Color| {
            let [r, g, b, _] = c.linear_premultiplied();
            0.2126 * r + 0.7152 * g + 0.0722 * b
        };
        let (a, b) = (luminance(one), luminance(other));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn token(theme: &Theme, name: &str) -> Color {
        theme.tokens().into_iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn the_dark_theme_is_the_default_themes_own_colours() {
        for (name, made) in Theme::DARK.tokens() {
            let own = default_token(name);
            assert!(apart(made, own) < 0.006, "--vx-{name}: made {}, the default is {}", made.css(), own.css());
            assert!(made.a.abs_diff(own.a) <= 1, "--vx-{name}'s alpha");
        }
    }

    #[test]
    fn colours_convert_to_oklch_and_back() {
        for hex in [0x0c0e10, 0x7988ff, 0xf4f5f7, 0xeb5757, 0x4cb782, 0xffffff, 0x000000] {
            let color = Color::hex(hex);
            let [l, c, h] = color.to_oklch();
            assert_eq!(Color::oklch(l, c, h, 1.0), color, "{hex:06x}");
        }
        let [l, c, _] = Color::WHITE.to_oklch();
        assert!((l - 1.0).abs() < 1e-3 && c < 1e-3);
    }

    #[test]
    fn text_reads_on_panels_in_every_theme() {
        let warm = Theme { base: Color::hex(0x1a1612), accent: Color::hex(0xe2a04a), contrast: 0.5 };
        for theme in [Theme::DARK, Theme::LIGHT, warm] {
            let panel = token(&theme, "panel");
            assert!(ratio(token(&theme, "text"), panel) > 12.0, "{theme:?}");
            assert!(ratio(token(&theme, "muted"), panel) > 4.5, "muted text reads, {theme:?}");
            assert!(ratio(token(&theme, "line"), panel) > 1.1, "lines show, {theme:?}");
        }
        assert!(!Theme::LIGHT.is_dark() && Theme::DARK.is_dark());
        let light_text = token(&Theme::LIGHT, "text").to_oklch()[0];
        assert!(light_text < 0.4, "dark text on a light theme");
        let on_amber = token(&Theme { accent: Color::hex(0xffd166), ..Theme::DARK }, "on-accent");
        assert!(on_amber.to_oklch()[0] < 0.3, "dark text on a light accent");
    }

    #[test]
    fn more_contrast_spreads_text_and_lines_from_the_panels() {
        for base in [Theme::DARK, Theme::LIGHT] {
            let at = |contrast: f32| Theme { contrast, ..base };
            let panel = token(&base, "panel");
            for name in ["text", "muted", "line"] {
                let [low, mid, high] = [0.0, 0.5, 1.0].map(|c| ratio(token(&at(c), name), panel));
                assert!(low < mid && mid < high, "{name}: {low} {mid} {high}");
            }
        }
    }

    #[test]
    fn a_theme_restyles_a_ui_and_your_own_sheets_still_win() {
        let mut ui = Ui::new();
        let root = ui.root();
        let button = ui.button(root, "Save");
        let background = |ui: &mut Ui| {
            ui.frame(400, 300, 1.0);
            ui.style(button).unwrap().background.unwrap()
        };
        let dark = background(&mut ui);
        assert_eq!(dark, default_token("field"));

        ui.set_theme(&Theme::LIGHT);
        let light = background(&mut ui);
        assert_eq!(light, token(&Theme::LIGHT, "field"));
        ui.set_theme(&Theme::DARK);
        assert_eq!(background(&mut ui), token(&Theme::DARK, "field"), "set again, it replaces the last");

        ui.add_sheet(root, StyleSheet::parse("Mine.vss", ":root { --vx-field: #ff0000; }"));
        ui.set_theme(&Theme::LIGHT);
        assert_eq!(background(&mut ui), Color::hex(0xff0000), "a sheet of your own wins over the theme");
    }
}
