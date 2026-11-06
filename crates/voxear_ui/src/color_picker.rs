//! A colour picker, as design tools and Unity have one: a square of
//! saturation (across) and brightness (down) for the hue picked on the bar
//! under it, the colour as it was and as it is now side by side, and its hex
//! code and red, green and blue to type. An HDR colour adds an Intensity in
//! stops, which brightens it past white in linear light, as emissive
//! materials need; Unity's HDR picker works the same way.
//!
//! It is a group of ordinary elements, built in code and asked each frame:
//! [`ColorPicker::update`] follows drags on the square and the bar and what
//! is typed, and returns the colour when it changed. The default theme
//! styles it (`color-picker` and the classes in it).

use crate::{Color, Gradient, NodeId, Style, Ui};

/// How far an HDR colour's intensity goes either way, in stops.
const STOPS: f32 = 10.0;
const HUES: [f32; 7] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0];

/// A colour as the picker edits it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickedColor {
    /// As people pick it: sRGB, each channel from 0 to 1.
    pub srgb: [f32; 3],
    /// For an HDR colour, how many stops brighter it is than `srgb`.
    pub intensity: f32,
}

impl PickedColor {
    pub fn srgb(srgb: [f32; 3]) -> Self {
        Self { srgb: srgb.map(|c| c.clamp(0.0, 1.0)), intensity: 0.0 }
    }

    /// A linear HDR colour, as emission is kept: brighter than white, its
    /// brightest channel is brought down to 1 and the rest counted as
    /// intensity.
    pub fn from_linear(rgb: [f32; 3]) -> Self {
        let brightest = rgb.into_iter().fold(0.0, f32::max);
        let (scaled, intensity) =
            if brightest > 1.0 { (rgb.map(|c| c / brightest), brightest.log2()) } else { (rgb, 0.0) };
        Self { srgb: scaled.map(|c| to_srgb(c.max(0.0))), intensity }
    }

    /// An HDR colour as scripts keep one: sRGB times two to its intensity,
    /// so past white its brightest channel is over 1.
    pub fn brightened(srgb: [f32; 3]) -> Self {
        let brightest = srgb.into_iter().fold(0.0, f32::max);
        if brightest <= 1.0 {
            return Self::srgb(srgb);
        }
        Self { srgb: srgb.map(|c| (c / brightest).max(0.0)), intensity: brightest.log2() }
    }

    /// As scripts keep it, its intensity applied in sRGB.
    pub fn to_brightened(self) -> [f32; 3] {
        let gain = self.intensity.exp2();
        self.srgb.map(|c| c * gain)
    }

    /// As linear light, its intensity applied.
    pub fn to_linear(self) -> [f32; 3] {
        let gain = self.intensity.exp2();
        self.srgb.map(|c| to_linear(c) * gain)
    }

    /// `#RRGGBB`, as design tools write it.
    pub fn hex(self) -> String {
        let [r, g, b] = self.srgb.map(byte);
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    fn color(self) -> Color {
        let [r, g, b] = self.srgb.map(byte);
        Color::rgb(r, g, b)
    }
}

pub struct ColorPicker {
    root: NodeId,
    square: NodeId,
    square_mark: NodeId,
    hue: NodeId,
    hue_mark: NodeId,
    before: NodeId,
    now: NodeId,
    hex: NodeId,
    channels: [NodeId; 3],
    intensity_row: NodeId,
    intensity: NodeId,
    intensity_field: NodeId,
    color: PickedColor,
    /// Hue in degrees, saturation and value, kept apart from the colour so
    /// the hue holds while the colour is grey or black.
    hsv: [f32; 3],
    hdr: bool,
}

impl ColorPicker {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let root = ui.panel(parent, Style::column());
        ui.add_class(root, "color-picker");
        let square = ui.panel(root, Style::default());
        ui.add_class(square, "color-square");
        let square_mark = ui.panel(square, Style::default());
        ui.add_class(square_mark, "color-mark");

        let hue = ui.panel(root, Style::row());
        ui.add_class(hue, "color-hue");
        for pair in HUES.windows(2) {
            let segment = ui.panel(hue, Style::default());
            ui.add_class(segment, "color-hue-segment");
            let [left, right] = [pair[0], pair[1]].map(|h| PickedColor::srgb(hsv_to_srgb([h, 1.0, 1.0])).color());
            ui.set_gradient(segment, Some(Gradient { left, right, bottom: None }));
        }
        let hue_mark = ui.panel(hue, Style::default());
        ui.add_class(hue_mark, "color-hue-mark");

        let compare = ui.panel(root, Style::row());
        ui.add_class(compare, "color-row");
        let before = ui.panel(compare, Style::default());
        ui.add_class(before, "color-before");
        let now = ui.panel(compare, Style::default());
        ui.add_class(now, "color-now");
        let hex = ui.text_field(compare, "");
        ui.add_class(hex, "color-hex");

        let values = ui.panel(root, Style::row());
        ui.add_class(values, "color-row");
        let channels = ["R", "G", "B"].map(|letter| {
            let cell = ui.panel(values, Style::row());
            ui.add_class(cell, "color-channel");
            let label = ui.add_label(cell, letter);
            ui.add_class(label, "color-channel-letter");
            let field = ui.text_field(cell, "");
            ui.add_class(field, "color-channel-input");
            field
        });

        let intensity_row = ui.panel(root, Style::row());
        ui.add_class(intensity_row, "color-row");
        let label = ui.add_label(intensity_row, "Intensity");
        ui.add_class(label, "color-intensity-label");
        let intensity = ui.slider(intensity_row, -STOPS, STOPS, 0.0);
        ui.add_class(intensity, "color-intensity-slider");
        let intensity_field = ui.text_field(intensity_row, "");
        ui.add_class(intensity_field, "color-intensity-input");

        let mut picker = Self {
            root,
            square,
            square_mark,
            hue,
            hue_mark,
            before,
            now,
            hex,
            channels,
            intensity_row,
            intensity,
            intensity_field,
            color: PickedColor::srgb([1.0; 3]),
            hsv: [0.0, 0.0, 1.0],
            hdr: false,
        };
        picker.set(ui, picker.color, false);
        picker
    }

    /// The group of elements, to place or show.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Shows `color`, as the colour before too, and with `hdr`, its
    /// intensity.
    pub fn set(&mut self, ui: &mut Ui, color: PickedColor, hdr: bool) {
        self.hdr = hdr;
        self.color = if hdr { color } else { PickedColor { intensity: 0.0, ..color } };
        self.hsv = srgb_to_hsv(self.color.srgb);
        ui.set_visible(self.intensity_row, hdr);
        ui.set_inline_style(self.before, &format!("background-color: {}", self.color.hex()));
        self.show(ui);
    }

    pub fn color(&self) -> PickedColor {
        self.color
    }

    /// Follows drags on the square and the hue bar and what is typed.
    /// Returns the colour when it changed this frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<PickedColor> {
        let before = self.color;
        if let Some([across, down]) = drag(ui, self.square) {
            self.hsv[1] = across;
            self.hsv[2] = 1.0 - down;
            self.color.srgb = hsv_to_srgb(self.hsv);
        }
        if let Some([across, _]) = drag(ui, self.hue) {
            self.hsv[0] = across * 360.0;
            self.color.srgb = hsv_to_srgb(self.hsv);
        }
        if ui.changed(self.hex)
            && let Some(srgb) = parse_hex(&ui.text(self.hex))
        {
            self.typed(srgb);
        }
        for (channel, field) in self.channels.into_iter().enumerate() {
            if ui.changed(field)
                && let Ok(value) = ui.text(field).trim().parse::<f32>()
            {
                let mut srgb = self.color.srgb;
                srgb[channel] = value.clamp(0.0, 1.0);
                self.typed(srgb);
            }
        }
        if self.hdr && ui.changed(self.intensity) {
            self.color.intensity = ui.slider_value(self.intensity);
        }
        if self.hdr
            && ui.changed(self.intensity_field)
            && let Ok(stops) = ui.text(self.intensity_field).trim().parse::<f32>()
        {
            self.color.intensity = stops.clamp(-STOPS, STOPS);
        }
        if self.color == before {
            return None;
        }
        self.show(ui);
        Some(self.color)
    }

    /// A colour typed in, which the square and bar follow; the hue stays
    /// where it was for a grey.
    fn typed(&mut self, srgb: [f32; 3]) {
        self.color.srgb = srgb;
        let [hue, saturation, value] = srgb_to_hsv(srgb);
        self.hsv = [if saturation > 0.0 && value > 0.0 { hue } else { self.hsv[0] }, saturation, value];
    }

    /// The square's hue, the marks, the swatch and the values, leaving a
    /// field being typed in as it is.
    fn show(&self, ui: &mut Ui) {
        let pure = PickedColor::srgb(hsv_to_srgb([self.hsv[0], 1.0, 1.0])).color();
        ui.set_gradient(self.square, Some(Gradient { left: Color::WHITE, right: pure, bottom: Some(Color::BLACK) }));
        let [_, _, width, height] = ui.rect(self.square);
        let (left, top) = (self.hsv[1] * width, (1.0 - self.hsv[2]) * height);
        ui.set_inline_style(self.square_mark, &format!("left: {left}px; top: {top}px"));
        let [_, _, bar, _] = ui.rect(self.hue);
        ui.set_inline_style(self.hue_mark, &format!("left: {}px", self.hsv[0] / 360.0 * bar));
        ui.set_inline_style(self.now, &format!("background-color: {}", self.color.hex()));
        let focused = ui.focused();
        let write = |ui: &mut Ui, field: NodeId, text: String| {
            if focused != Some(field) {
                ui.set_text(field, &text);
            }
        };
        write(ui, self.hex, self.color.hex());
        for (&field, value) in self.channels.iter().zip(self.color.srgb) {
            write(ui, field, format!("{value:.3}"));
        }
        if self.hdr {
            ui.set_slider_value(self.intensity, self.color.intensity);
            write(ui, self.intensity_field, format!("{:.2}", self.color.intensity));
        }
    }
}

/// Where the pointer is across and down `node` while it is pressed there,
/// as fractions from its top left, held to its edges.
fn drag(ui: &Ui, node: NodeId) -> Option<[f32; 2]> {
    if !ui.is_pressed(node) {
        return None;
    }
    let (x, y) = ui.pointer_position()?;
    let [left, top, width, height] = ui.rect(node);
    Some([((x - left) / width.max(1.0)).clamp(0.0, 1.0), ((y - top) / height.max(1.0)).clamp(0.0, 1.0)])
}

fn byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn parse_hex(text: &str) -> Option<[f32; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some([value >> 16, value >> 8, value].map(|c| (c & 0xff) as f32 / 255.0))
}

/// Hue in degrees, then saturation and value from 0 to 1, of an sRGB colour.
pub fn srgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let range = max - min;
    let hue = if range <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / range).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / range + 2.0)
    } else {
        60.0 * ((r - g) / range + 4.0)
    };
    [hue, if max > 0.0 { range / max } else { 0.0 }, max]
}

pub fn hsv_to_srgb([hue, saturation, value]: [f32; 3]) -> [f32; 3] {
    let chroma = value * saturation;
    let h = (hue.rem_euclid(360.0)) / 60.0;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;
    [r + m, g + m, b + m]
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_go_to_hsv_and_back() {
        for srgb in [[1.0, 0.0, 0.0], [0.2, 0.6, 0.9], [0.5, 0.5, 0.5], [0.0, 0.0, 0.0], [0.9, 0.9, 0.1]] {
            let back = hsv_to_srgb(srgb_to_hsv(srgb));
            assert!(srgb.iter().zip(back).all(|(a, b)| (a - b).abs() < 1e-5), "{srgb:?} came back as {back:?}");
        }
        assert_eq!(srgb_to_hsv([0.0, 1.0, 0.0]), [120.0, 1.0, 1.0]);
    }

    #[test]
    fn hdr_colours_split_into_a_colour_and_stops() {
        let emission = [6.0, 1.8, 3.0];
        let picked = PickedColor::from_linear(emission);
        assert!((picked.intensity - 6f32.log2()).abs() < 1e-5, "the brightest channel sets the intensity");
        assert!((picked.srgb[0] - 1.0).abs() < 1e-5);
        let back = picked.to_linear();
        assert!(emission.iter().zip(back).all(|(a, b)| (a - b).abs() < 1e-3), "{back:?}");
        assert_eq!(
            PickedColor::from_linear([0.5, 0.25, 0.0]).intensity,
            0.0,
            "a colour no brighter than white has none"
        );
        assert_eq!(PickedColor::srgb([1.0, 0.5, 0.0]).hex(), "#FF8000");
        assert_eq!(parse_hex("#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_hex("ff80"), None);
    }

    #[test]
    fn script_hdr_colours_keep_their_intensity_in_srgb() {
        let glow = [4.0, 2.0, 0.0];
        let picked = PickedColor::brightened(glow);
        assert_eq!((picked.srgb, picked.intensity), ([1.0, 0.5, 0.0], 2.0));
        assert_eq!(picked.to_brightened(), glow);
        assert_eq!(PickedColor::brightened([0.2, 0.4, 0.6]).intensity, 0.0);
    }
}
