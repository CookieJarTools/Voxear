//! The Inspector's fields for a Curve and a Gradient, drawn in their rows
//! and edited there by dragging: a small graph of the curve with a dot for
//! each key, and a strip of the gradient's colours with a marker for each
//! key and its colour's numbers under it. A press where there is no key
//! adds one; a key dragged off the field and let go is taken out.

use std::cell::Cell;

use voxear_core::curve::{Curve, Gradient};
use voxear_ui::{Color, NodeId, Shape, Style, Ui};

/// How near a key, in pixels, a press picks it up.
const PICK: f32 = 8.0;
/// How far off the field, in pixels, a key let go is taken out.
const OFF: f32 = 20.0;
/// How many straight pieces draw a curve, and slices a gradient.
const STEPS: usize = 40;

const GUIDE: u32 = 0x3a3c44;
const LINE: u32 = 0x8fb4ff;
const KEY: u32 = 0xe8eaf0;
const HELD: u32 = 0xffb44a;

/// A curve's field: its graph, the key held, and the values it shows
/// between, kept while a key is dragged so the graph does not shift under
/// the pointer.
pub(crate) struct CurveField {
    pub graph: NodeId,
    held: Cell<Option<usize>>,
    range: Cell<[f32; 2]>,
}

impl CurveField {
    pub fn new(ui: &mut Ui, row: NodeId) -> Self {
        let graph = ui.panel(row, Style::default());
        ui.add_class(graph, "curve-field");
        Self { graph, held: Cell::new(None), range: Cell::new([0.0, 1.0]) }
    }

    /// Draws `curve`, between 0 and 1 or as far as its keys go.
    pub fn draw(&self, ui: &mut Ui, curve: &Curve) {
        if self.held.get().is_none() {
            let (low, high) = curve.keys().iter().fold((0.0f32, 1.0f32), |(lo, hi), k| (lo.min(k[1]), hi.max(k[1])));
            self.range.set([low, high + (high - low) * 0.1]);
        }
        let [_, _, width, height] = ui.rect(self.graph);
        let [low, high] = self.range.get();
        let y = |v: f32| height - (v - low) / (high - low).max(1e-4) * height;
        let mut shapes = Vec::new();
        for guide in [0.0, 1.0].into_iter().filter(|v| (low..=high).contains(v)) {
            shapes.push(Shape::Line { from: [0.0, y(guide)], to: [width, y(guide)], width: 1.0, color: Color::hex(GUIDE) });
        }
        // With no keys it changes nothing: shown flat at 1, faintly.
        let faint = curve.is_empty();
        let value = |t: f32| curve.at(t).unwrap_or(1.0);
        let points: Vec<[f32; 2]> =
            (0..=STEPS).map(|i| i as f32 / STEPS as f32).map(|t| [t * width, y(value(t))]).collect();
        let color = if faint { Color::hex(LINE).with_alpha(70) } else { Color::hex(LINE) };
        for pair in points.windows(2) {
            shapes.push(Shape::Line { from: pair[0], to: pair[1], width: 1.5, color });
        }
        for (i, key) in curve.keys().iter().enumerate() {
            let fill = if self.held.get() == Some(i) { Color::hex(HELD) } else { Color::hex(KEY) };
            shapes.push(Shape::Circle { center: [key[0] * width, y(key[1])], radius: 3.5, fill, border: None });
        }
        ui.set_shapes(self.graph, shapes);
    }

    /// What dragging on the field did to `curve`, if anything.
    pub fn drag(&self, ui: &Ui, curve: &Curve) -> Option<Curve> {
        let [left, top, width, height] = ui.rect(self.graph);
        let [low, high] = self.range.get();
        let (px, py) = ui.pointer_position()?;
        let outside = px < left - OFF || px > left + width + OFF || py < top - OFF || py > top + height + OFF;
        if !ui.is_pressed(self.graph) {
            let let_go = self.held.take()?;
            return outside.then(|| curve.without(let_go));
        }
        let key = [
            ((px - left) / width.max(1.0)).clamp(0.0, 1.0),
            (low + (1.0 - (py - top) / height.max(1.0)) * (high - low)).clamp(low, high),
        ];
        match self.held.get() {
            Some(held) => {
                let (moved, at) = curve.with_key(held, key);
                self.held.set(Some(at));
                (moved != *curve).then_some(moved)
            }
            None => {
                let near = |k: &[f32; 2]| {
                    let (kx, ky) = (left + k[0] * width, top + (1.0 - (k[1] - low) / (high - low).max(1e-4)) * height);
                    (kx - px).hypot(ky - py)
                };
                let nearest = curve.keys().iter().enumerate().map(|(i, k)| (i, near(k))).min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((i, _)) = nearest.filter(|(_, d)| *d <= PICK) {
                    self.held.set(Some(i));
                    return None;
                }
                let (added, at) = curve.with_added(key)?;
                self.held.set(Some(at));
                Some(added)
            }
        }
    }
}

/// A gradient's field: its strip, the key held and the key picked, whose
/// colour its fields show, in sRGB, as colours are typed elsewhere.
pub(crate) struct GradientField {
    pub strip: NodeId,
    pub fields: Vec<NodeId>,
    held: Cell<Option<usize>>,
    picked: Cell<usize>,
}

impl GradientField {
    pub fn new(ui: &mut Ui, row: NodeId, fields: impl FnOnce(&mut Ui, NodeId) -> Vec<NodeId>) -> Self {
        let column = ui.panel(row, Style::default());
        ui.add_class(column, "gradient-field");
        let strip = ui.panel(column, Style::default());
        ui.add_class(strip, "gradient-strip");
        let numbers = ui.panel(column, Style::default());
        ui.add_class(numbers, "vector");
        ui.add_class(numbers, "color");
        let fields = fields(ui, numbers);
        Self { strip, fields, held: Cell::new(None), picked: Cell::new(0) }
    }

    /// Draws `gradient`, and shows its picked key's colour in the fields
    /// not being typed in.
    pub fn draw(&self, ui: &mut Ui, gradient: &Gradient) {
        let [_, _, width, height] = ui.rect(self.strip);
        let mut shapes = Vec::new();
        let slice = width / STEPS as f32;
        for i in 0..STEPS {
            let t = (i as f32 + 0.5) / STEPS as f32;
            let fill = gradient.at(t).map_or(Color::hex(GUIDE), shown);
            shapes.push(Shape::Rect { rect: [i as f32 * slice, 0.0, slice + 0.5, height - 6.0], radius: 0.0, fill });
        }
        let picked = self.picked.get().min(gradient.keys().len().saturating_sub(1));
        for (i, key) in gradient.keys().iter().enumerate() {
            let ring = if i == picked { Color::hex(HELD) } else { Color::hex(KEY) };
            let fill = shown([key[1], key[2], key[3]]);
            shapes.push(Shape::Circle { center: [key[0] * width, height - 5.0], radius: 4.5, fill, border: Some((1.5, ring)) });
        }
        ui.set_shapes(self.strip, shapes);
        let rgb = gradient.keys().get(picked).map(|k| voxear_engine::srgb([k[1], k[2], k[3]]));
        for (axis, &field) in self.fields.iter().enumerate() {
            let text = rgb.map_or(String::new(), |rgb| crate::inspector::number(rgb[axis]));
            if ui.focused() != Some(field) && ui.text(field) != text {
                ui.set_text(field, &text);
            }
        }
    }

    /// What dragging on the strip, or typing a colour, did to `gradient`.
    pub fn edit(&self, ui: &Ui, gradient: &Gradient) -> Option<Gradient> {
        if let Some(typed) = self.typed(ui, gradient) {
            return Some(typed);
        }
        let [left, top, width, height] = ui.rect(self.strip);
        let (px, py) = ui.pointer_position()?;
        let outside = py < top - OFF || py > top + height + OFF || px < left - OFF || px > left + width + OFF;
        if !ui.is_pressed(self.strip) {
            let let_go = self.held.take()?;
            return outside.then(|| gradient.without(let_go));
        }
        let t = ((px - left) / width.max(1.0)).clamp(0.0, 1.0);
        match self.held.get() {
            Some(held) => {
                let key = gradient.keys()[held];
                let (moved, at) = gradient.with_key(held, [t, key[1], key[2], key[3]]);
                self.held.set(Some(at));
                self.picked.set(at);
                (moved != *gradient).then_some(moved)
            }
            None => {
                let nearest = gradient.keys().iter().enumerate().map(|(i, k)| (i, (left + k[0] * width - px).abs()));
                if let Some((i, _)) = nearest.min_by(|a, b| a.1.total_cmp(&b.1)).filter(|(_, d)| *d <= PICK) {
                    self.held.set(Some(i));
                    self.picked.set(i);
                    return None;
                }
                let [r, g, b] = gradient.at(t).unwrap_or([1.0; 3]);
                let (added, at) = gradient.with_added([t, r, g, b])?;
                self.held.set(Some(at));
                self.picked.set(at);
                Some(added)
            }
        }
    }

    /// The picked key with the colour typed into its fields, in sRGB.
    fn typed(&self, ui: &Ui, gradient: &Gradient) -> Option<Gradient> {
        if !self.fields.iter().any(|&field| ui.changed(field)) {
            return None;
        }
        let picked = self.picked.get();
        let key = *gradient.keys().get(picked)?;
        let mut rgb = voxear_engine::srgb([key[1], key[2], key[3]]);
        for (axis, &field) in self.fields.iter().enumerate() {
            if let Ok(n) = ui.text(field).trim().parse::<f32>() {
                rgb[axis] = n.clamp(0.0, 1.0);
            }
        }
        let [r, g, b] = voxear_engine::linear(rgb);
        Some(gradient.with_key(picked, [key[0], r, g, b]).0)
    }
}

/// A linear colour as the UI draws it.
fn shown(linear: [f32; 3]) -> Color {
    let [r, g, b] = voxear_engine::srgb(linear).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    Color::rgb(r, g, b)
}
