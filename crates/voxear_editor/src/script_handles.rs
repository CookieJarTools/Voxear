//! Handles a behaviour's gizmos show for tools of their own (`Handles` in
//! Voxear.Core), which the editor draws and drags in the Scene view: a
//! point to drag in the plane of the view; a radius's dot on its rim, to
//! the right on screen; a slider's dot at its distance along its line; and
//! a button; and a disc's dot on its ring, dragged round it. Each frame the gizmos say which handles they show, and are
//! told which one the pointer holds and where it has dragged it to, or
//! which button it clicked, and give back the value as the drag left it.
//! Holding Ctrl snaps a point to the snap distance, a radius or distance to
//! tenths of a metre, and a disc to 15° turns.

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_scripting::{HandleId, HandleInput, ScriptHandle, ScriptHandleKind};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{ViewCamera, along_axis, on_plane, snapped};

/// How near the pointer must be to a dot to take it, in logical pixels.
const REACH: f32 = 9.0;
/// How far a disc turns in a step while Ctrl is held, in degrees.
const DISC_STEP: f32 = 15.0;
/// A dot's radius on screen, a button's half width, and the colour they take unless told.
const DOT: f32 = 4.5;
const BUTTON: f32 = 6.0;
const COLOR: u32 = 0x5ad1e6;

/// A drag under way, of the handle as it was when the pointer took it.
#[derive(Clone, Debug)]
struct Drag {
    handle: ScriptHandle,
    /// From where the pointer took hold to the dot, kept as the dot follows it.
    offset: Vec3,
    /// The view's forward then, for dragging in its plane.
    facing: Vec3,
    /// Where the pointer has dragged it to, once it has moved: a point, and a radius or distance.
    to: Option<(Vec3, f32)>,
}

/// The handles the gizmos showed last, and any drag of one.
#[derive(Default)]
pub struct ScriptHandles {
    shown: Vec<ScriptHandle>,
    hovered: Option<usize>,
    drag: Option<Drag>,
    clicked: Option<HandleId>,
}

/// Where a handle's dot sits in the world.
fn grip(handle: &ScriptHandle, camera: &ViewCamera) -> Vec3 {
    match handle.kind {
        ScriptHandleKind::Position | ScriptHandleKind::Button => handle.at,
        ScriptHandleKind::Radius => handle.at + camera.right * handle.number,
        ScriptHandleKind::Slider => handle.at + handle.direction * handle.number,
        ScriptHandleKind::Disc => handle.at + handle.toward * handle.number,
    }
}

/// Where a disc's dot goes for a pointer over `hit` in its plane: round its
/// ring to there, in 15° steps from where it was taken when `snap`.
fn round_disc(handle: &ScriptHandle, hit: Vec3, snap: bool) -> Option<Vec3> {
    let axis = handle.direction.normalize_or_zero();
    let flat = hit - handle.at;
    let way = (flat - axis * flat.dot(axis)).try_normalize()?;
    if !snap {
        return Some(handle.at + way * handle.number);
    }
    let start = handle.toward;
    let angle = start.cross(way).dot(axis).atan2(start.dot(way));
    let step = DISC_STEP.to_radians();
    let turned = voxear_core::math::Quat::from_axis_angle(axis, (angle / step).round() * step) * start;
    Some(handle.at + turned * handle.number)
}

impl ScriptHandles {
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether the pointer is over a handle, so a press takes it before the UI.
    pub fn is_hovered(&self) -> bool {
        self.hovered.is_some()
    }

    /// The handles the gizmos showed this frame. With no gizmos drawn, there are none.
    pub fn show(&mut self, handles: Vec<ScriptHandle>) {
        if self.hovered.is_some_and(|index| self.shown.get(index).map(|h| &h.id) != handles.get(index).map(|h| &h.id)) {
            self.hovered = None;
        }
        self.shown = handles;
    }

    /// Notes which handle is under the pointer, if any, to light it and take a press.
    pub fn hover(&mut self, camera: &ViewCamera, point: Option<[f32; 2]>) {
        if self.drag.is_some() {
            return;
        }
        self.hovered = point.and_then(|point| {
            let near = |handle: &ScriptHandle| {
                camera.project(grip(handle, camera)).map(|at| (at[0] - point[0]).hypot(at[1] - point[1]))
            };
            let distances = self.shown.iter().enumerate().filter_map(|(index, h)| Some((index, near(h)?)));
            distances.filter(|&(_, d)| d <= REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(index, _)| index)
        });
    }

    /// Takes hold of the handle under `point`, if there is one, returning
    /// the Obj whose behaviour showed it. A button is clicked as it is
    /// pressed, and held until let go, so what the click changes is one
    /// step to undo like a drag.
    pub fn begin(&mut self, camera: &ViewCamera, point: [f32; 2]) -> Option<Entity> {
        self.hover(camera, Some(point));
        let handle = self.shown.get(self.hovered?)?.clone();
        if handle.kind == ScriptHandleKind::Button {
            self.clicked = Some(handle.id.clone());
        }
        let at = grip(&handle, camera);
        let hit = on_plane(camera.ray(point), at, camera.forward).unwrap_or(at);
        let entity = handle.id.entity;
        self.drag = Some(Drag { handle, offset: at - hit, facing: camera.forward, to: None });
        Some(entity)
    }

    /// Moves the held handle to follow the pointer. `snap` is whether Ctrl is held, and `step` the snap distance.
    pub fn drag_to(&mut self, camera: &ViewCamera, point: [f32; 2], (snap, step): (bool, f32)) {
        let Some(drag) = &mut self.drag else { return };
        let handle = &drag.handle;
        let ray = camera.ray(point);
        let tenth = |value: f32| if snap { snapped(value, 0.1) } else { value };
        drag.to = match handle.kind {
            ScriptHandleKind::Position => on_plane(ray, handle.at, drag.facing).map(|hit| {
                let step = if snap { step } else { 0.0 };
                (Vec3::from_array((hit + drag.offset).to_array().map(|v| snapped(v, step))), 0.0)
            }),
            ScriptHandleKind::Radius => {
                on_plane(ray, handle.at, drag.facing).map(|hit| (hit, tenth(hit.distance(handle.at))))
            }
            ScriptHandleKind::Slider => along_axis(ray, handle.at, handle.direction)
                .map(|along| (handle.at + handle.direction * tenth(along), tenth(along))),
            ScriptHandleKind::Disc => on_plane(ray, handle.at, handle.direction)
                .and_then(|hit| round_disc(handle, hit, snap))
                .map(|dot| (dot, handle.number)),
            ScriptHandleKind::Button => None,
        }
        .or(drag.to);
    }

    /// What the pointer does to the handles, for this frame's gizmos. A click is told once.
    pub fn input(&mut self) -> HandleInput {
        let drag = self.drag.as_ref().and_then(|drag| Some((drag.handle.id.clone(), drag.to?.0, drag.to?.1)));
        HandleInput { drag, clicked: self.clicked.take() }
    }

    /// Lets go of the handle, returning the Obj whose behaviour showed it.
    pub fn end(&mut self) -> Option<Entity> {
        self.drag.take().map(|drag| drag.handle.id.entity)
    }

    /// The handles' dots and buttons, as shapes in the view's own
    /// coordinates (from its top left). Their lines and rings the gizmos
    /// draw themselves.
    pub fn shapes(&self, camera: &ViewCamera) -> Vec<Shape> {
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let held = self.drag.as_ref().map(|drag| &drag.handle.id);
        let border = Color::rgba(12, 14, 16, 200);
        let mut shapes = Vec::new();
        for (index, handle) in self.shown.iter().enumerate() {
            let Some(center) = camera.project(grip(handle, camera)).map(local) else { continue };
            let hot = self.hovered == Some(index) || held == Some(&handle.id);
            let fill = if hot { Color::hex(0xf2c94c) } else { handle.color.map_or(Color::hex(COLOR), color) };
            if handle.kind == ScriptHandleKind::Button {
                let edge = BUTTON + 1.0;
                shapes.push(Shape::Rect {
                    rect: [center[0] - edge, center[1] - edge, edge * 2.0, edge * 2.0],
                    radius: 3.0,
                    fill: border,
                });
                let rect = [center[0] - BUTTON, center[1] - BUTTON, BUTTON * 2.0, BUTTON * 2.0];
                shapes.push(Shape::Rect { rect, radius: 2.0, fill });
            } else {
                shapes.push(Shape::Circle { center, radius: DOT, fill, border: Some((1.0, border)) });
            }
        }
        shapes
    }
}

/// A colour as the gizmos give it, red, green, blue and alpha from 0 to 1.
fn color(parts: [f32; 4]) -> Color {
    let [r, g, b, a] = parts.map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8);
    Color::rgba(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;
    use voxear_ecs::World;
    use voxear_render::Camera;
    use voxear_scene::Transform;

    use super::*;

    fn camera() -> ViewCamera {
        let mut world = World::new();
        let transform = Transform::new(Vec3::new(0.0, 0.0, -10.0), Quat::IDENTITY, Vec3::ONE);
        let entity = world.spawn((Camera::default(), transform));
        ViewCamera::new(&world, entity, [0.0, 0.0, 800.0, 600.0]).unwrap()
    }

    fn handle(kind: ScriptHandleKind, at: Vec3, number: f32) -> ScriptHandle {
        let id = HandleId { entity: Entity::from_bits(7), behaviour: "Hello.Guard".into(), id: 0 };
        ScriptHandle { id, kind, at, direction: Vec3::Y, number, toward: Vec3::X, color: None }
    }

    #[test]
    fn a_disc_turns_round_its_ring_in_steps_with_ctrl() {
        let disc = handle(ScriptHandleKind::Disc, Vec3::ZERO, 2.0);
        assert_eq!(grip(&disc, &camera()), Vec3::new(2.0, 0.0, 0.0), "its dot out along its way");
        // A pointer a little over 40° round towards +Z, and off the ring.
        let hit = Vec3::new(3.0, 0.0, 2.6);
        let free = round_disc(&disc, hit, false).unwrap();
        assert!((free.length() - 2.0).abs() < 1e-5 && free.z > 1.0, "on the ring: {free}");
        let snapped = round_disc(&disc, hit, true).unwrap();
        let expected = Vec3::new(45f32.to_radians().cos(), 0.0, 45f32.to_radians().sin()) * 2.0;
        assert!(snapped.distance(expected) < 1e-4, "45° round: {snapped}");
    }

    #[test]
    fn a_radius_sits_on_its_rim_and_a_slider_at_its_distance() {
        let camera = camera();
        let radius = grip(&handle(ScriptHandleKind::Radius, Vec3::ZERO, 2.0), &camera);
        assert!(radius.distance(camera.right * 2.0) < 1e-5, "{radius}");
        let slider = grip(&handle(ScriptHandleKind::Slider, Vec3::X, 3.0), &camera);
        assert_eq!(slider, Vec3::new(1.0, 3.0, 0.0));
    }

    #[test]
    fn a_click_is_told_once_and_a_drag_until_let_go() {
        let camera = camera();
        let mut handles = ScriptHandles::default();
        handles.show(vec![handle(ScriptHandleKind::Button, Vec3::ZERO, 0.0)]);
        let middle = camera.project(Vec3::ZERO).unwrap();
        assert_eq!(handles.begin(&camera, middle), Some(Entity::from_bits(7)));
        assert!(handles.input().clicked.is_some());
        assert_eq!(handles.input(), HandleInput::default(), "a click is told the frame it happens");
        assert_eq!(handles.end(), Some(Entity::from_bits(7)));

        handles.show(vec![handle(ScriptHandleKind::Position, Vec3::ZERO, 0.0)]);
        handles.begin(&camera, middle);
        assert_eq!(handles.input().drag, None, "nothing to tell until the pointer moves");
        handles.drag_to(&camera, [middle[0] + 40.0, middle[1]], (false, 0.0));
        let (_, point, _) = handles.input().drag.expect("dragged");
        assert!(point.dot(camera.right) > 0.1 && point.z.abs() < 1e-4, "right, in the plane of the view: {point}");
        assert!(handles.input().drag.is_some(), "told each frame while held");
        handles.end();
        assert_eq!(handles.input(), HandleInput::default());
    }
}
