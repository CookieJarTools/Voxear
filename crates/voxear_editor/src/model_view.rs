//! The Model window's view of a box model: the model standing on a floor
//! of blocks, seen from a camera that goes round it, with what is picked
//! outlined and handles to move and size it by.
//!
//! The model is drawn by the renderer as a preview with a camera of its own
//! (see `voxear_render::PreviewCamera`), square, and shown cut to the
//! view's shape, so what is under the pointer is found with the same
//! camera: a ray from it against the model's boxes. The outline and the
//! handles are shapes drawn over the picture.
//!
//! A drag on the view turns the camera, the middle button moves it, and
//! the wheel brings it nearer. A press let go where it was made picks the
//! box under it. A drag from a handle moves what is picked along the
//! handle's axis, or one face of a box, a whole pixel at a time.

use voxear_core::math::{Mat4, Vec2, Vec3};
use voxear_render::{
    BoxModel, Face, MAX_VIEW_SIZE, MaterialDesc, MaterialHandle, MeshData, MeshHandle, PreviewCamera, PreviewId,
    PreviewOptions, PreviewPart, Renderer, TextureId, TextureSource, Vertex,
};
use voxear_ui::{Color, ImageRef, NodeId, Shape, Style, Ui};

/// The view's field of view across its longer side, in degrees.
const FOV: f32 = 35.0;
/// How far a press may move, in logical pixels, and still be a click.
const CLICK: f32 = 4.0;
/// How far a drag turns the camera, in radians a pixel.
const TURN_PER_PIXEL: f32 = 0.008;
/// How far from what is picked a move handle's end is drawn, and how near
/// the pointer must be to a handle to take it, in logical pixels.
const HANDLE_LENGTH: f32 = 64.0;
const HANDLE_REACH: f32 = 9.0;
/// How many blocks across the floor is.
const FLOOR: f32 = 8.0;

const AXES: [(Vec3, Color); 3] =
    [(Vec3::X, Color::rgb(232, 86, 86)), (Vec3::Y, Color::rgb(110, 196, 92)), (Vec3::Z, Color::rgb(86, 138, 236))];
const OUTLINE: Color = Color::rgb(255, 196, 64);
const FAINT: Color = Color::rgba(255, 196, 64, 110);

/// A bone, or one of a bone's boxes by its place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picked {
    Bone(usize),
    Box(usize, usize),
}

impl Picked {
    /// The bone, or the box's bone.
    pub fn bone(self) -> usize {
        match self {
            Picked::Bone(bone) | Picked::Box(bone, _) => bone,
        }
    }
}

/// A handle on what is picked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    /// Moves it along X, Y or Z, by the axis's number.
    Move(usize),
    /// Moves one face of a box, sizing it.
    Face(Face),
}

/// What the view asks of the panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewEvent {
    /// A click picked this box, or with none, nothing.
    Pick(Option<Picked>),
    /// A drag from a handle began, went on to this many pixels from where
    /// it began, or ended.
    DragBegan(Handle),
    Dragged(Handle, f32),
    DragEnded,
    /// A stroke paints this pixel of the sheet; `within` is the part of the
    /// sheet its face takes, by its left, top, width and height.
    Paint {
        pixel: [u32; 2],
        within: [u32; 4],
    },
    /// The stroke was let go.
    PaintEnded,
}

/// What a press on the model does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// A click picks a box, and handles move and size what is picked.
    #[default]
    Shape,
    /// A press on the model paints the pixel under it, and with `mirror`
    /// the pixel across the body from it too.
    Paint { mirror: bool },
    /// The model is only looked at, as while it is shown moving.
    Look,
}

/// Where the camera is: going round `target`, `distance` metres from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub target: Vec3,
    /// Round the model, in radians: at 0 it is seen from in front.
    pub yaw: f32,
    /// Up over the model, in radians.
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self { target: Vec3::new(0.0, 0.5, 0.0), yaw: -0.6, pitch: 0.35, distance: 5.0 }
    }
}

impl Orbit {
    pub fn camera(&self) -> PreviewCamera {
        let level = self.pitch.cos();
        let from = Vec3::new(self.yaw.sin() * level, self.pitch.sin(), self.yaw.cos() * level);
        let position = self.target + from * self.distance;
        PreviewCamera { position, target: self.target, near: (self.distance * 0.02).clamp(0.005, 1.0) }
    }

    /// Looks at a box from `least` to `most` whole, from the side it is
    /// seen from now, in a view whose shorter side is `share` of its
    /// longer.
    pub fn frame(&mut self, least: Vec3, most: Vec3, share: f32) {
        self.target = (least + most) * 0.5;
        let radius = ((most - least).length() * 0.5).max(0.05);
        let half = ((FOV.to_radians() * 0.5).tan() * share.clamp(0.1, 1.0)).atan();
        self.distance = radius * 1.25 / half.sin();
    }
}

/// A drag from a handle: the handle, the line it moves along in the model's
/// space, how far along that line the press was, and how many metres a
/// pixel of the model is.
#[derive(Clone, Copy)]
struct Dragging {
    handle: Handle,
    from: Vec3,
    along: Vec3,
    pressed_at: f32,
    pixel: f32,
    last: f32,
}

/// A press on the view not yet let go: where it was made, where it was last
/// frame, and whether it has moved too far to be a click, so turns the
/// camera.
#[derive(Clone, Copy)]
struct Press {
    at: (f32, f32),
    last: (f32, f32),
    moved: bool,
}

pub struct ModelView {
    frame: NodeId,
    image: NodeId,
    overlay: NodeId,
    pub orbit: Orbit,
    preview: Option<PreviewId>,
    /// The model's mesh, sheet and material as last uploaded, and the
    /// floor's.
    drawn: Option<(MeshHandle, TextureId, MaterialHandle)>,
    floor: Option<(MeshHandle, TextureId, MaterialHandle)>,
    /// The side of the square the view is drawn in, in pixels, and the
    /// view's own width and height in logical pixels.
    size: u32,
    shape: Vec2,
    /// What it was asked to look at whole, least and most, until it knows its shape to do so.
    to_frame: Option<(Vec3, Vec3)>,
    press: Option<Press>,
    panning: Option<(f32, f32)>,
    dragging: Option<Dragging>,
    /// Where a stroke of paint last was in the view, while one goes on.
    stroke: Option<Vec2>,
    dark: bool,
    /// Asked to draw again, as when the model or the camera changed.
    stale: bool,
}

impl ModelView {
    pub fn new(ui: &mut Ui, parent: NodeId) -> Self {
        let frame = ui.panel(parent, Style::default());
        ui.add_class(frame, "model-view");
        ui.take_wheel(frame, true);
        // The frame takes the presses: its picture and what is drawn over it do not.
        let image = ui.image(frame);
        ui.add_class(image, "model-view-image");
        ui.set_visible(image, false);
        let overlay = ui.panel(frame, Style { pickable: false, ..Style::default() });
        ui.add_class(overlay, "model-view-overlay");
        Self {
            frame,
            image,
            overlay,
            orbit: Orbit::default(),
            preview: None,
            drawn: None,
            floor: None,
            size: 0,
            shape: Vec2::ONE,
            to_frame: None,
            press: None,
            panning: None,
            dragging: None,
            stroke: None,
            dark: true,
            stale: true,
        }
    }

    /// The element the view is drawn in, which takes its presses.
    pub fn frame_node(&self) -> NodeId {
        self.frame
    }

    pub fn set_dark(&mut self, dark: bool) {
        (self.dark, self.stale) = (dark, true);
    }

    /// Whether a handle or the camera is being dragged.
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
            || self.stroke.is_some()
            || self.press.is_some_and(|press| press.moved)
            || self.panning.is_some()
    }

    /// Lets go of everything it holds on the GPU.
    pub fn clear(&mut self, renderer: &mut Renderer) {
        if let Some(preview) = self.preview.take() {
            renderer.free_preview(preview);
        }
        for (mesh, texture, material) in self.drawn.take().into_iter().chain(self.floor.take()) {
            renderer.free_mesh(mesh);
            renderer.free_material(material);
            renderer.free_texture(texture);
        }
        (self.press, self.panning, self.dragging, self.stroke, self.stale) = (None, None, None, None, true);
    }

    /// Looks at the whole of `model`, or at the floor for one with nothing
    /// in it.
    pub fn frame_model(&mut self, model: &BoxModel) {
        self.to_frame = Some(model.bounds().unwrap_or((Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 1.0, 0.5))));
    }

    /// The model changed: its mesh and sheet are uploaded again.
    pub fn model_changed(&mut self, renderer: &mut Renderer, model: &BoxModel) {
        if let Some((mesh, texture, material)) = self.drawn.take() {
            renderer.free_mesh(mesh);
            renderer.free_material(material);
            renderer.free_texture(texture);
        }
        self.stale = true;
        // Drawn as it stands, with nothing posing it: no skin is needed.
        let data = MeshData { skin: Vec::new(), ..model.mesh() };
        if data.indices.is_empty() {
            return;
        }
        let sheet = &model.sheet;
        let source = TextureSource {
            name: "box model sheet",
            width: sheet.width,
            height: sheet.height,
            rgba: &sheet.rgba,
            srgb: true,
        };
        let made = renderer.add_texture(&source).and_then(|texture| {
            let desc = MaterialDesc {
                roughness: 1.0,
                alpha_cutoff: Some(0.5),
                pixels: true,
                base_color_texture: Some(texture),
                ..MaterialDesc::default()
            };
            Ok((renderer.add_mesh(&data)?, texture, renderer.add_material(&desc)?))
        });
        match made {
            Ok(drawn) => self.drawn = Some(drawn),
            Err(error) => tracing::error!("the model could not be drawn: {error:#}"),
        }
    }

    /// The view's frame: the camera turned, moved and brought nearer by
    /// the pointer, the picture drawn again when anything changed, what is
    /// picked outlined with its handles, and what the pointer did to the
    /// model. `scale` is the window's, for how fine the picture is.
    pub fn update(
        &mut self,
        ui: &mut Ui,
        renderer: &mut Renderer,
        model: &BoxModel,
        picked: Option<Picked>,
        mode: ViewMode,
        scale: f32,
    ) -> Vec<ViewEvent> {
        // Painting, nothing has handles.
        let handled = picked.filter(|_| mode == ViewMode::Shape);
        let [_, _, width, height] = ui.rect(self.frame);
        if width < 1.0 || height < 1.0 {
            return Vec::new();
        }
        self.shape = Vec2::new(width, height);
        if let Some((least, most)) = self.to_frame.take() {
            self.orbit.frame(least, most, width.min(height) / width.max(height));
            self.stale = true;
        }
        let across = width.max(height);
        let size = ((across * scale).ceil() as u32).div_ceil(64).max(1) * 64;
        let size = size.min(MAX_VIEW_SIZE);
        if size != self.size {
            (self.size, self.stale) = (size, true);
        }
        let seen = Seen { camera: self.orbit.camera(), width, height };
        let handles = handles(model, handled, &seen);
        let events = self.pointer(ui, model, &seen, &handles, mode);
        // Seen again from where the pointer just put the camera.
        let seen = Seen { camera: self.orbit.camera(), width, height };
        let handles = self::handles(model, handled, &seen);
        let hot = self.dragging.map(|drag| drag.handle).or_else(|| {
            let pointer = self.pointer_in(ui)?;
            (!self.is_dragging()).then(|| nearest_handle(&handles, pointer)).flatten()
        });
        let mut drawn = shapes(model, picked, &seen, &handles, hot);
        // Painting, the pixel under the pointer is marked.
        if let (ViewMode::Paint { .. }, Some(pointer), false) = (mode, self.pointer_in(ui), self.is_dragging()) {
            let bones = model.rest_matrices();
            let (origin, direction) = seen.ray(pointer);
            let corners = model.pick(&bones, origin, direction).and_then(|hit| model.pixel_corners(&bones, &hit));
            if let Some(corners) = corners.map(|corners| corners.map(|corner| seen.project(corner))) {
                for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
                    if let (Some(from), Some(to)) = (corners[a], corners[b]) {
                        drawn.push(Shape::Line {
                            from: from.to_array(),
                            to: to.to_array(),
                            width: 1.5,
                            color: Color::WHITE,
                        });
                    }
                }
            }
        }
        ui.set_shapes(self.overlay, drawn);
        self.draw(ui, renderer, &seen);
        events
    }

    /// Where the pointer is in the view, from its top left, while it is
    /// over it.
    fn pointer_in(&self, ui: &Ui) -> Option<Vec2> {
        let (x, y) = ui.pointer_position()?;
        let [left, top, width, height] = ui.rect(self.frame);
        let inside = (left..left + width).contains(&x) && (top..top + height).contains(&y);
        inside.then(|| Vec2::new(x - left, y - top))
    }

    fn pointer(
        &mut self,
        ui: &mut Ui,
        model: &BoxModel,
        seen: &Seen,
        handles: &[(Handle, HandleLine)],
        mode: ViewMode,
    ) -> Vec<ViewEvent> {
        let mut events = Vec::new();
        let [left, top, _, _] = ui.rect(self.frame);
        let pointer = ui.pointer_position();
        let local = pointer.map(|(x, y)| Vec2::new(x - left, y - top));

        // The wheel: nearer or further, by a tenth a notch.
        let wheel = ui.wheeled(self.frame)[1];
        if wheel != 0.0 {
            self.orbit.distance = (self.orbit.distance * (1.0 + wheel * 0.0025)).clamp(0.05, 500.0);
            self.stale = true;
        }

        // The middle button moves the camera across what it sees.
        match (ui.is_middle_pressed(self.frame), pointer, self.panning) {
            (true, Some(now), Some(last)) => {
                let (_, right, up) = seen.camera.axes();
                let per_pixel = seen.metres_per_pixel(self.orbit.distance);
                self.orbit.target += (right * -(now.0 - last.0) + up * (now.1 - last.1)) * per_pixel;
                (self.panning, self.stale) = (Some(now), self.stale || now != last);
            }
            (true, Some(now), None) => self.panning = Some(now),
            _ => self.panning = None,
        }

        let pressed = ui.is_pressed(self.frame);
        match (pressed, pointer, local) {
            // A drag from a handle goes on.
            (true, _, Some(local)) if self.dragging.is_some() => {
                let drag = self.dragging.as_mut().expect("checked");
                let (origin, direction) = seen.ray(local);
                let now = along_line(drag.from, drag.along, origin, direction);
                let pixels = ((now - drag.pressed_at) / drag.pixel).round();
                if pixels != drag.last {
                    drag.last = pixels;
                    events.push(ViewEvent::Dragged(drag.handle, pixels));
                }
            }
            // A press goes on: far enough from where it was made, it turns the camera.
            (true, Some(now), _) if self.press.is_some() => {
                let press = self.press.as_mut().expect("checked");
                let from_start = Vec2::new(now.0 - press.at.0, now.1 - press.at.1);
                press.moved |= from_start.length() > CLICK;
                if press.moved {
                    self.orbit.yaw -= (now.0 - press.last.0) * TURN_PER_PIXEL;
                    self.orbit.pitch = (self.orbit.pitch + (now.1 - press.last.1) * TURN_PER_PIXEL).clamp(-1.5, 1.5);
                    self.stale = true;
                }
                press.last = now;
            }
            // A stroke goes on: every pixel the pointer passed over since the last frame.
            (true, _, Some(local)) if self.stroke.is_some() => {
                let last = self.stroke.replace(local).expect("checked");
                let steps = (local.distance(last) / 1.5).ceil().max(1.0) as u32;
                for step in 1..=steps {
                    self.dab(model, seen, last.lerp(local, step as f32 / steps as f32), mode, &mut events);
                }
            }
            // A press on the model, painting, begins a stroke.
            (true, Some(_), Some(local))
                if matches!(mode, ViewMode::Paint { .. }) && self.dab(model, seen, local, mode, &mut events) =>
            {
                self.stroke = Some(local);
            }
            // A press begins: on a handle, or on the view.
            (true, Some(now), Some(local)) => match nearest_handle(handles, local) {
                Some(handle) => {
                    let line = handles.iter().find(|(other, _)| *other == handle).map(|(_, line)| *line);
                    let line = line.expect("found among them");
                    let (origin, direction) = seen.ray(local);
                    let pressed_at = along_line(line.from, line.along, origin, direction);
                    let pixel = model.pixel_size();
                    self.dragging =
                        Some(Dragging { handle, from: line.from, along: line.along, pressed_at, pixel, last: 0.0 });
                    events.push(ViewEvent::DragBegan(handle));
                }
                None => self.press = Some(Press { at: now, last: now, moved: false }),
            },
            // Let go: a drag ends, and a click that turned nothing picks what is under it.
            _ => {
                let dragged = self.dragging.take().is_some();
                if dragged {
                    events.push(ViewEvent::DragEnded);
                }
                let turned = self.press.take().is_some_and(|press| press.moved);
                let stroked = self.stroke.take().is_some();
                if stroked {
                    events.push(ViewEvent::PaintEnded);
                }
                if let (true, false, false, false, Some(local)) =
                    (ui.clicked(self.frame), dragged, turned, stroked, local)
                {
                    match mode {
                        ViewMode::Look => {}
                        ViewMode::Shape => {
                            let (origin, direction) = seen.ray(local);
                            let hit = model.pick(&model.rest_matrices(), origin, direction);
                            events.push(ViewEvent::Pick(hit.map(|hit| Picked::Box(hit.bone, hit.index))));
                        }
                        // A press made and let go between two frames paints its pixel all the same.
                        ViewMode::Paint { .. } => {
                            if self.dab(model, seen, local, mode, &mut events) {
                                events.push(ViewEvent::PaintEnded);
                            }
                        }
                    }
                }
            }
        }
        events
    }

    /// Paints the pixel of the model under a point of the view, and with
    /// the mode's mirror the one across the body from it. False where the
    /// point is off the model.
    fn dab(&self, model: &BoxModel, seen: &Seen, at: Vec2, mode: ViewMode, events: &mut Vec<ViewEvent>) -> bool {
        let bones = model.rest_matrices();
        let (origin, direction) = seen.ray(at);
        let Some(hit) = model.pick(&bones, origin, direction) else { return false };
        let across = Vec3::new(-1.0, 1.0, 1.0);
        let mirrored = matches!(mode, ViewMode::Paint { mirror: true })
            .then(|| model.pick(&bones, origin * across, direction * across))
            .flatten();
        for hit in [Some(hit), mirrored].into_iter().flatten() {
            let within = model.bones[hit.bone].boxes[hit.index].face_place(hit.face);
            if let (Some(pixel), Some(within)) = (hit.pixel, within) {
                events.push(ViewEvent::Paint { pixel, within });
            }
        }
        true
    }

    /// Draws the model with its bones where `bones` puts them rather than
    /// at rest, one for each bone, for showing it move.
    pub fn show_posed(&mut self, renderer: &mut Renderer, model: &BoxModel, bones: &[Mat4]) {
        let Some((mesh, texture, material)) = self.drawn else { return self.model_changed(renderer, model) };
        let data = MeshData { skin: Vec::new(), ..model.mesh_posed(bones) };
        if data.indices.is_empty() {
            return;
        }
        if let Ok(posed) = renderer.add_mesh(&data) {
            renderer.free_mesh(mesh);
            (self.drawn, self.stale) = (Some((posed, texture, material)), true);
        }
    }

    /// The model's sheet changed and nothing else: its pixels are uploaded
    /// again, behind the material the model is drawn with.
    pub fn sheet_changed(&mut self, renderer: &mut Renderer, model: &BoxModel) {
        let Some((mesh, texture, material)) = self.drawn else { return self.model_changed(renderer, model) };
        let sheet = &model.sheet;
        let source = TextureSource {
            name: "box model sheet",
            width: sheet.width,
            height: sheet.height,
            rgba: &sheet.rgba,
            srgb: true,
        };
        let Ok(painted) = renderer.add_texture(&source) else { return };
        let desc = MaterialDesc {
            roughness: 1.0,
            alpha_cutoff: Some(0.5),
            pixels: true,
            base_color_texture: Some(painted),
            ..MaterialDesc::default()
        };
        if renderer.update_material(material, &desc).is_ok() {
            renderer.free_texture(texture);
            (self.drawn, self.stale) = (Some((mesh, painted, material)), true);
        }
    }

    /// Draws the picture again where anything changed, and shows it, cut to
    /// the view's shape.
    fn draw(&mut self, ui: &mut Ui, renderer: &mut Renderer, seen: &Seen) {
        if self.stale {
            self.stale = false;
            let background = if self.dark { [0.075, 0.08, 0.09, 1.0] } else { [0.78, 0.8, 0.83, 1.0] };
            let options = PreviewOptions {
                size: self.size,
                camera: Some(seen.camera),
                rotation: voxear_core::math::Quat::IDENTITY,
                fov_degrees: FOV,
                background,
                ..PreviewOptions::default()
            };
            let mut parts = Vec::new();
            if let Some((mesh, _, material)) = self.floor(renderer) {
                parts.push(PreviewPart { mesh, material, transform: Mat4::IDENTITY });
            }
            if let Some((mesh, _, material)) = self.drawn {
                parts.push(PreviewPart { mesh, material, transform: Mat4::IDENTITY });
            }
            match self.preview {
                Some(preview) => renderer.set_preview(preview, Some(parts), options),
                None => self.preview = Some(renderer.add_preview(parts, options)),
            }
        }
        let image = self.preview.and_then(|preview| renderer.preview(preview)).map(|image| {
            // The square's middle, as wide and as high as the view is.
            let [u0, v0, u1, v1] = image.uv;
            let across = seen.width.max(seen.height);
            let (cut_u, cut_v) =
                ((1.0 - seen.width / across) * 0.5 * (u1 - u0), (1.0 - seen.height / across) * 0.5 * (v1 - v0));
            ImageRef { texture: image.texture.0, uv: [u0 + cut_u, v0 + cut_v, u1 - cut_u, v1 - cut_v] }
        });
        ui.set_image(self.image, image);
        ui.set_visible(self.image, image.is_some());
    }

    /// The floor: a square of blocks, each marked off, and each pixel of a
    /// block in it faintly.
    fn floor(&mut self, renderer: &mut Renderer) -> Option<(MeshHandle, TextureId, MaterialHandle)> {
        if self.floor.is_none() {
            const ACROSS: u32 = 16;
            let mut rgba = Vec::with_capacity((ACROSS * ACROSS * 4) as usize);
            for at in 0..ACROSS * ACROSS {
                let (x, y) = (at % ACROSS, at / ACROSS);
                let shade: u8 = match (x == 0 || y == 0, (x + y) % 2 == 0) {
                    (true, _) => 86,
                    (false, true) => 58,
                    (false, false) => 54,
                };
                rgba.extend([shade, shade + 2, shade + 6, 255]);
            }
            let source = TextureSource { name: "model floor", width: ACROSS, height: ACROSS, rgba: &rgba, srgb: true };
            let half = FLOOR * 0.5;
            let corners = [(-half, half), (half, half), (half, -half), (-half, -half)];
            let vertices = corners.map(|(x, z)| Vertex {
                position: [x, 0.0, z],
                normal: [0.0, 1.0, 0.0],
                uv: [x, -z],
                tangent: [1.0, 0.0, 0.0, 1.0],
                ..Vertex::default()
            });
            let data = MeshData { vertices: vertices.to_vec(), indices: vec![0, 1, 2, 0, 2, 3], skin: Vec::new() };
            let made = renderer.add_texture(&source).and_then(|texture| {
                let desc = MaterialDesc {
                    roughness: 1.0,
                    pixels: true,
                    base_color_texture: Some(texture),
                    ..MaterialDesc::default()
                };
                Ok((renderer.add_mesh(&data)?, texture, renderer.add_material(&desc)?))
            });
            self.floor = made.ok();
        }
        self.floor
    }

    /// Where a point of the model's space is drawn in the window, for a
    /// test to press on.
    pub fn point(&self, ui: &Ui, at: Vec3) -> Option<(f32, f32)> {
        let [left, top, width, height] = ui.rect(self.frame);
        let seen = Seen { camera: self.orbit.camera(), width, height };
        seen.project(at).map(|point| (left + point.x, top + point.y))
    }

    /// Where a handle on what is picked is drawn in the window, for a test
    /// to drag.
    pub fn handle_point(&self, ui: &Ui, model: &BoxModel, picked: Picked, handle: Handle) -> Option<(f32, f32)> {
        let [left, top, width, height] = ui.rect(self.frame);
        let seen = Seen { camera: self.orbit.camera(), width, height };
        let handles = handles(model, Some(picked), &seen);
        let line = handles.iter().find(|(other, _)| *other == handle)?.1;
        Some((left + line.end.x, top + line.end.y))
    }
}

/// The view as the camera sees it: the camera, and the view's size in
/// logical pixels.
struct Seen {
    camera: PreviewCamera,
    width: f32,
    height: f32,
}

impl Seen {
    /// How much of the square picture the view shows, across and down.
    fn share(&self) -> Vec2 {
        let across = self.width.max(self.height);
        Vec2::new(self.width / across, self.height / across)
    }

    /// Where a point of the model's space is in the view, from its top
    /// left; `None` for one behind the camera.
    fn project(&self, at: Vec3) -> Option<Vec2> {
        let clip = self.camera.view_projection(FOV) * at.extend(1.0);
        if clip.w <= 1e-5 {
            return None;
        }
        let share = self.share();
        let (x, y) = (clip.x / clip.w / share.x, clip.y / clip.w / share.y);
        Some(Vec2::new((x * 0.5 + 0.5) * self.width, (y * 0.5 + 0.5) * self.height))
    }

    /// The ray from the camera through a point of the view: where it
    /// starts and which way it goes, in the model's space.
    fn ray(&self, at: Vec2) -> (Vec3, Vec3) {
        let share = self.share();
        let x = (at.x / self.width * 2.0 - 1.0) * share.x;
        let y = (at.y / self.height * 2.0 - 1.0) * share.y;
        let (forward, right, up) = self.camera.axes();
        let reach = (FOV.to_radians() * 0.5).tan();
        (self.camera.position, (forward + right * (x * reach) - up * (y * reach)).normalize())
    }

    /// How many metres one logical pixel of the view covers, `distance`
    /// from the camera.
    fn metres_per_pixel(&self, distance: f32) -> f32 {
        distance * (FOV.to_radians() * 0.5).tan() * 2.0 / self.width.max(self.height)
    }
}

/// A handle as drawn: the line it moves what is picked along, in the
/// model's space, and where its two ends are in the view.
#[derive(Clone, Copy)]
struct HandleLine {
    from: Vec3,
    along: Vec3,
    start: Vec2,
    end: Vec2,
    color: Color,
}

/// The handles on what is picked: an arrow each for X, Y and Z, along the
/// axes of what it is measured from, and for a box a knob on each face.
fn handles(model: &BoxModel, picked: Option<Picked>, seen: &Seen) -> Vec<(Handle, HandleLine)> {
    let Some(picked) = picked else { return Vec::new() };
    let bones = model.rest_matrices();
    let Some(bone) = model.bones.get(picked.bone()) else { return Vec::new() };
    let mut found = Vec::new();
    // A bone moves in its parent's space, a box in its bone's.
    let (space, middle) = match picked {
        Picked::Bone(at) => {
            let parent = bone.parent.and_then(|parent| bones.get(parent)).copied().unwrap_or(Mat4::IDENTITY);
            (parent, bones[at].w_axis.truncate())
        }
        Picked::Box(at, index) => {
            let Some(model_box) = bone.boxes.get(index) else { return Vec::new() };
            let (place, reach) = model.box_place(&bones[at], model_box);
            // A knob a little way out from each face that has any size to it.
            for face in Face::ALL {
                let out = place.transform_vector3(face.normal()).normalize_or_zero();
                let on_face = place.transform_point3(face.normal() * reach);
                let length = seen.metres_per_pixel((on_face - seen.camera.position).length()) * 14.0;
                let (Some(start), Some(end)) = (seen.project(on_face), seen.project(on_face + out * length)) else {
                    continue;
                };
                found.push((Handle::Face(face), HandleLine { from: on_face, along: out, start, end, color: OUTLINE }));
            }
            (bones[at], place.w_axis.truncate())
        }
    };
    let length = seen.metres_per_pixel((middle - seen.camera.position).length()) * HANDLE_LENGTH;
    for (axis, (direction, color)) in AXES.iter().enumerate() {
        let along = space.transform_vector3(*direction).normalize_or_zero();
        let (Some(start), Some(end)) = (seen.project(middle), seen.project(middle + along * length)) else { continue };
        found.push((Handle::Move(axis), HandleLine { from: middle, along, start, end, color: *color }));
    }
    found
}

/// The handle whose end the pointer is nearest, within reach of it.
fn nearest_handle(handles: &[(Handle, HandleLine)], pointer: Vec2) -> Option<Handle> {
    let near = handles.iter().map(|(handle, line)| (*handle, line.end.distance(pointer)));
    near.filter(|(_, distance)| *distance <= HANDLE_REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(handle, _)| handle)
}

/// How far along the line from `from` along `along` the point nearest a ray
/// is, in metres.
fn along_line(from: Vec3, along: Vec3, origin: Vec3, direction: Vec3) -> f32 {
    let between = from - origin;
    let (a, b, c) = (along.dot(along), along.dot(direction), direction.dot(direction));
    let (d, e) = (along.dot(between), direction.dot(between));
    let facing = a * c - b * b;
    // Seen end on, the pointer says nothing about how far along.
    if facing.abs() < 1e-6 { 0.0 } else { (b * e - c * d) / facing }
}

/// What is drawn over the picture: what is picked outlined, a bone's pivot
/// marked, and the handles.
fn shapes(
    model: &BoxModel,
    picked: Option<Picked>,
    seen: &Seen,
    handles: &[(Handle, HandleLine)],
    hot: Option<Handle>,
) -> Vec<Shape> {
    let Some(picked) = picked else { return Vec::new() };
    let bones = model.rest_matrices();
    let Some(bone) = model.bones.get(picked.bone()) else { return Vec::new() };
    let mut shapes = Vec::new();
    let mut outline = |index: usize, color: Color| {
        let Some(model_box) = bone.boxes.get(index) else { return };
        let corners = model.box_corners(&bones[picked.bone()], model_box).map(|corner| seen.project(corner));
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)] {
            if let (Some(from), Some(to)) = (corners[a], corners[b]) {
                shapes.push(Shape::Line { from: from.to_array(), to: to.to_array(), width: 1.5, color });
            }
        }
    };
    match picked {
        Picked::Box(_, index) => outline(index, OUTLINE),
        Picked::Bone(_) => (0..bone.boxes.len()).for_each(|index| outline(index, FAINT)),
    }
    // The pivot of the bone picked, or of the box's bone.
    if let Some(pivot) = seen.project(bones[picked.bone()].w_axis.truncate()) {
        let ring = Some((1.5, Color::rgb(20, 22, 26)));
        shapes.push(Shape::Circle { center: pivot.to_array(), radius: 4.0, fill: OUTLINE, border: ring });
    }
    for (handle, line) in handles {
        let lit = hot == Some(*handle);
        let color = if lit { Color::WHITE } else { line.color };
        if matches!(handle, Handle::Move(_)) {
            shapes.push(Shape::Line { from: line.start.to_array(), to: line.end.to_array(), width: 2.0, color });
        }
        let radius = if matches!(handle, Handle::Move(_)) { 5.0 } else { 3.5 };
        let ring = Some((1.0, Color::rgb(20, 22, 26)));
        shapes.push(Shape::Circle { center: line.end.to_array(), radius, fill: color, border: ring });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_drawn_where_the_ray_through_it_goes_whatever_the_views_shape() {
        for (width, height) in [(900.0, 500.0), (400.0, 700.0), (512.0, 512.0)] {
            let orbit = Orbit { target: Vec3::new(0.2, 0.8, -0.1), yaw: 0.7, pitch: 0.4, distance: 4.0 };
            let seen = Seen { camera: orbit.camera(), width, height };
            for at in [Vec3::new(0.5, 1.0, 0.25), Vec3::new(-0.4, 0.1, 0.6), orbit.target] {
                let drawn = seen.project(at).unwrap();
                let (origin, direction) = seen.ray(drawn);
                let nearest = origin + direction * (at - origin).dot(direction);
                assert!((nearest - at).length() < 1e-3, "{at} at {drawn} in {width} × {height}");
            }
            // What it looks at is in the middle.
            let middle = seen.project(orbit.target).unwrap();
            assert!((middle - Vec2::new(width, height) * 0.5).length() < 0.01);
        }
    }

    #[test]
    fn the_camera_goes_round_and_frames_what_it_is_shown() {
        let mut orbit = Orbit { yaw: 0.0, pitch: 0.0, ..Orbit::default() };
        orbit.frame(Vec3::new(-0.5, 0.0, -0.5), Vec3::new(0.5, 2.0, 0.5), 1.0);
        assert_eq!(orbit.target, Vec3::new(0.0, 1.0, 0.0));
        let camera = orbit.camera();
        assert!(camera.position.z > 3.0 && camera.position.x.abs() < 1e-5, "from in front: {}", camera.position);
        // Its top and foot are both in a view as wide as it is high.
        let seen = Seen { camera, width: 600.0, height: 600.0 };
        for at in [Vec3::new(0.0, 2.0, 0.0), Vec3::ZERO] {
            let drawn = seen.project(at).unwrap();
            assert!((0.0..600.0).contains(&drawn.y), "{drawn}");
        }
        // A model facing +Z seen from in front has its right, +X, on the left.
        assert!(seen.project(Vec3::new(0.4, 1.0, 0.0)).unwrap().x < 300.0);
    }

    #[test]
    fn a_ray_says_how_far_along_a_handle_it_points() {
        // Looking down Z at a line along X through the origin, from a little to its right.
        let along = along_line(Vec3::ZERO, Vec3::X, Vec3::new(0.75, 0.3, 5.0), Vec3::NEG_Z);
        assert!((along - 0.75).abs() < 1e-5);
        assert_eq!(along_line(Vec3::ZERO, Vec3::Z, Vec3::new(0.0, 0.0, 5.0), Vec3::NEG_Z), 0.0, "seen end on");
    }

    #[test]
    fn a_picked_box_has_an_arrow_for_each_axis_and_a_knob_on_each_face() {
        let model = BoxModel::starter();
        let orbit = Orbit::default();
        let seen = Seen { camera: orbit.camera(), width: 800.0, height: 600.0 };
        let found = handles(&model, Some(Picked::Box(0, 0)), &seen);
        assert_eq!(found.len(), 9);
        let arrow = found.iter().find(|(handle, _)| *handle == Handle::Move(1)).unwrap().1;
        // About as long on screen wherever it is: a little shorter for being seen from above.
        let drawn = arrow.start.distance(arrow.end);
        assert!((HANDLE_LENGTH * 0.75..=HANDLE_LENGTH + 2.0).contains(&drawn), "{drawn}");
        assert!(arrow.end.y < arrow.start.y, "up is up");
        assert_eq!(nearest_handle(&found, arrow.end + Vec2::new(3.0, 2.0)), Some(Handle::Move(1)));
        assert_eq!(nearest_handle(&found, Vec2::new(5.0, 5.0)), None);
        // A bone has the arrows alone, from its pivot.
        let found = handles(&model, Some(Picked::Bone(0)), &seen);
        assert_eq!(found.len(), 3);
        assert!(handles(&model, None, &seen).is_empty());
    }
}
