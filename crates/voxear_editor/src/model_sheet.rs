//! The Model window's flat view of a box model's sheet: its pixels, each a
//! square large enough to see, over a chequered ground where the sheet has
//! holes, with the part each box's faces take outlined. A press on it
//! paints the pixel under it, and a drag every pixel it passes over, as a
//! stroke on the model itself does (see `model_view`).

use voxear_render::{BoxModel, Face, Renderer, Sheet, TextureId, TextureSource};
use voxear_ui::{Color, ImageRef, NodeId, Shape, Style, Ui};

use crate::model_view::{Picked, ViewEvent};

/// How much room the sheet leaves round itself, in logical pixels.
const MARGIN: f32 = 10.0;
/// The longest side the picture of the sheet is given, in its own pixels.
const LARGEST: u32 = 512;

const FACES: Color = Color::rgba(255, 255, 255, 46);
const PICKED: Color = Color::rgb(255, 196, 64);

pub struct SheetView {
    frame: NodeId,
    image: NodeId,
    overlay: NodeId,
    texture: Option<TextureId>,
    /// Where the sheet is drawn in the frame, from its top left, and how
    /// many logical pixels one of its pixels is across.
    placed: [f32; 3],
    /// The sheet's pixel a stroke last painted, while one goes on.
    stroke: Option<[u32; 2]>,
}

impl SheetView {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let frame = ui.panel(parent, Style::default());
        ui.add_class(frame, "model-sheet");
        let image = ui.image(frame);
        ui.add_class(image, "model-sheet-image");
        let overlay = ui.panel(frame, Style { pickable: false, ..Style::default() });
        ui.add_class(overlay, "model-sheet-image");
        Self { frame, image, overlay, texture: None, placed: [0.0, 0.0, 1.0], stroke: None }
    }

    /// The element the sheet is drawn in, which takes its presses.
    pub fn frame_node(&self) -> NodeId {
        self.frame
    }

    pub fn is_painting(&self) -> bool {
        self.stroke.is_some()
    }

    /// Lets go of what it holds on the GPU.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(texture) = self.texture.take() {
            renderer.free_texture(texture);
        }
        self.stroke = None;
    }

    /// The sheet changed: its picture is made again, each pixel a square of
    /// one colour, over a chequered ground where it is not solid.
    pub fn sheet_changed(&mut self, renderer: &mut Renderer, sheet: &Sheet) {
        if let Some(texture) = self.texture.take() {
            renderer.free_texture(texture);
        }
        let times = (LARGEST / sheet.width.max(sheet.height).max(1)).clamp(1, 16);
        let (width, height) = (sheet.width * times, sheet.height * times);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b, a] = sheet.get(x / times, y / times);
                let ground = if (x / times + y / times) % 2 == 0 { 46 } else { 54 };
                let solid = u32::from(a);
                let mix = |channel: u8| ((u32::from(channel) * solid + ground * (255 - solid)) / 255) as u8;
                rgba.extend([mix(r), mix(g), mix(b), 255]);
            }
        }
        let source = TextureSource { name: "box model sheet view", width, height, rgba: &rgba, srgb: true };
        self.texture = renderer.add_texture(&source).ok();
    }

    /// The view's frame: the sheet drawn to fit, the faces of `model`'s
    /// boxes outlined on it, those of what is picked brighter, and what a
    /// press or a drag paints.
    pub fn update(&mut self, ui: &mut Ui, model: &BoxModel, picked: Option<Picked>) -> Vec<ViewEvent> {
        let [left, top, width, height] = ui.rect(self.frame);
        let (sheet_width, sheet_height) = (model.sheet.width as f32, model.sheet.height as f32);
        let each = ((width - 2.0 * MARGIN) / sheet_width).min((height - 2.0 * MARGIN) / sheet_height).max(0.5);
        let placed =
            [((width - sheet_width * each) * 0.5).round(), ((height - sheet_height * each) * 0.5).round(), each];
        if placed != self.placed {
            self.placed = placed;
            let style = format!(
                "left: {}px; top: {}px; width: {}px; height: {}px",
                placed[0],
                placed[1],
                sheet_width * each,
                sheet_height * each
            );
            ui.set_inline_style(self.image, &style);
            ui.set_inline_style(self.overlay, &style);
        }
        let image = self.texture.map(|texture| ImageRef { texture: texture.0, uv: [0.0, 0.0, 1.0, 1.0] });
        ui.set_image(self.image, image);

        let under = ui.pointer_position().and_then(|(x, y)| {
            let (x, y) = ((x - left - placed[0]) / each, (y - top - placed[1]) / each);
            let inside = (0.0..sheet_width).contains(&x) && (0.0..sheet_height).contains(&y);
            inside.then_some([x as u32, y as u32])
        });
        let mut events = Vec::new();
        let within = |pixel: [u32; 2]| {
            let holds = |[x, y, w, h]: &[u32; 4]| (*x..x + w).contains(&pixel[0]) && (*y..y + h).contains(&pixel[1]);
            let whole = [0, 0, model.sheet.width, model.sheet.height];
            faces(model).into_iter().map(|(_, place)| place).find(holds).unwrap_or(whole)
        };
        let pressed = ui.is_pressed(self.frame);
        match (pressed, under, self.stroke) {
            // A stroke goes on: every pixel between where it was and where it is.
            (true, Some(now), Some(last)) if now != last => {
                for pixel in line(last, now).into_iter().skip(1) {
                    events.push(ViewEvent::Paint { pixel, within: within(pixel) });
                }
                self.stroke = Some(now);
            }
            (true, Some(now), None) => {
                events.push(ViewEvent::Paint { pixel: now, within: within(now) });
                self.stroke = Some(now);
            }
            (true, ..) => {}
            (false, under, stroke) => {
                // A press made and let go between two frames paints its pixel all the same.
                if let (None, true, Some(pixel)) = (stroke, ui.clicked(self.frame), under) {
                    events.push(ViewEvent::Paint { pixel, within: within(pixel) });
                    events.push(ViewEvent::PaintEnded);
                }
                if self.stroke.take().is_some() {
                    events.push(ViewEvent::PaintEnded);
                }
            }
        }

        let mut shapes = Vec::new();
        let mut outline = |[x, y, w, h]: [u32; 4], width: f32, color: Color| {
            let (x0, y0, x1, y1) = (x as f32 * each, y as f32 * each, (x + w) as f32 * each, (y + h) as f32 * each);
            for (from, to) in [([x0, y0], [x1, y0]), ([x1, y0], [x1, y1]), ([x1, y1], [x0, y1]), ([x0, y1], [x0, y0])] {
                shapes.push(Shape::Line { from, to, width, color });
            }
        };
        let lit = |part: (usize, usize)| match picked {
            Some(Picked::Box(bone, index)) => part == (bone, index),
            Some(Picked::Bone(bone)) => part.0 == bone,
            None => false,
        };
        let all = faces(model);
        all.iter().filter(|(part, _)| !lit(*part)).for_each(|(_, place)| outline(*place, 1.0, FACES));
        all.iter().filter(|(part, _)| lit(*part)).for_each(|(_, place)| outline(*place, 1.5, PICKED));
        if let Some([x, y]) = under {
            outline([x, y, 1, 1], 1.0, Color::WHITE);
        }
        ui.set_shapes(self.overlay, shapes);
        events
    }

    /// Where the middle of one of the sheet's pixels is drawn in the
    /// window, for a test to press on.
    pub fn pixel_point(&self, ui: &Ui, [x, y]: [u32; 2]) -> (f32, f32) {
        let [left, top, _, _] = ui.rect(self.frame);
        let [from_left, from_top, each] = self.placed;
        (left + from_left + (x as f32 + 0.5) * each, top + from_top + (y as f32 + 0.5) * each)
    }
}

/// The part of the sheet each drawn face of each box takes, with the box.
fn faces(model: &BoxModel) -> Vec<((usize, usize), [u32; 4])> {
    let mut found = Vec::new();
    for (bone, index, model_box) in model.boxes() {
        found.extend(Face::ALL.iter().filter_map(|face| Some(((bone, index), model_box.face_place(*face)?))));
        found.extend(Face::ALL.iter().filter_map(|face| Some(((bone, index), model_box.layer_place(*face)?))));
    }
    found.retain(|(_, place)| place[2] > 0 && place[3] > 0);
    found
}

/// Every pixel on the straight line from one to another, both among them.
fn line(from: [u32; 2], to: [u32; 2]) -> Vec<[u32; 2]> {
    let (mut x, mut y) = (from[0] as i64, from[1] as i64);
    let (end_x, end_y) = (to[0] as i64, to[1] as i64);
    let (across, down) = ((end_x - x).abs(), -(end_y - y).abs());
    let (step_x, step_y) = (if x < end_x { 1 } else { -1 }, if y < end_y { 1 } else { -1 });
    let mut error = across + down;
    let mut pixels = vec![[x as u32, y as u32]];
    while (x, y) != (end_x, end_y) {
        let doubled = 2 * error;
        if doubled >= down {
            error += down;
            x += step_x;
        }
        if doubled <= across {
            error += across;
            y += step_y;
        }
        pixels.push([x as u32, y as u32]);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroke_passes_over_every_pixel_between_two() {
        assert_eq!(line([1, 1], [4, 1]), [[1, 1], [2, 1], [3, 1], [4, 1]]);
        assert_eq!(line([3, 3], [1, 1]), [[3, 3], [2, 2], [1, 1]]);
        let steep = line([0, 0], [2, 6]);
        assert_eq!((steep.len(), steep[0], steep[6]), (7, [0, 0], [2, 6]));
        assert_eq!(line([5, 5], [5, 5]), [[5, 5]]);
    }

    #[test]
    fn every_drawn_face_has_its_part_of_the_sheet() {
        let model = BoxModel::starter();
        let found = faces(&model);
        assert_eq!(found.len(), 6);
        assert!(found.iter().all(|(part, place)| *part == (0, 0) && place[2] == 16 && place[3] == 16));
    }
}
