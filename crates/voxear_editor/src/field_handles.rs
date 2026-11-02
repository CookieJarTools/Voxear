//! Handles that edit a behaviour's fields in the Scene view, as the field's
//! attribute asks (`[PositionHandle]`, `[RadiusHandle]`, `[DirectionHandle]`,
//! `[BoundsHandle]`, `[ArcHandle]` and `[SliderHandle]` in Voxear.Core), so
//! no editor code is needed to drag a value where it belongs. They show for
//! the selected Obj while editing, under the move, rotate and scale handles,
//! as dots to drag: a point, or each point of a list joined in order as a
//! path; a circle's rim; an arrow's tip; a box's faces; an arc's end; a
//! point along an axis. A drag writes the field as it goes and is one step
//! to undo. Holding Ctrl snaps a point to the snap distance, and the rest to
//! tenths of a metre, or 5 degrees.

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::script_types::{FieldHandle, FieldKind, HandleKind, ScriptTypes};
use voxear_scene::{Transform, WorldTransform};
use voxear_ui::{Color, Shape};

use crate::{
    fields::{self, Step},
    props,
    scene_tools::{ViewCamera, along_axis, on_plane, ring, snapped},
};

/// How near the pointer must be to a dot to take it, in logical pixels.
const REACH: f32 = 9.0;
/// A dot's radius on screen, and the colour handles take unless told.
const DOT: f32 = 4.5;
const COLOR: u32 = 0x5ad1e6;
/// How far an arc is drawn from the Obj, in metres, when its attribute says nothing.
const ARC_RADIUS: f32 = 2.0;
const ARC_SEGMENTS: usize = 32;

/// One dot to drag: the field it edits, what kind of handle it is, and
/// where it sits in the world now.
#[derive(Clone, Debug, PartialEq)]
struct Grip {
    behaviour: String,
    path: Vec<Step>,
    handle: FieldHandle,
    at: Vec3,
    /// For a box's face, which axis and side: ±1, 2 or 3 for X, Y or Z.
    face: i8,
}

/// A drag under way.
#[derive(Clone, Debug)]
struct Drag {
    entity: Entity,
    grip: Grip,
    /// From where the pointer took hold to the dot, kept as the dot follows it.
    offset: Vec3,
    /// The view's forward then, for dragging in its plane.
    facing: Vec3,
}

/// The fields' handles for the selected Obj, and any drag of one.
#[derive(Default)]
pub struct FieldHandles {
    drag: Option<Drag>,
    hovered: Option<usize>,
}

/// Where the Obj is in the world: its position, rotation and matrix.
fn frame(world: &World, entity: Entity) -> Option<(Vec3, Quat, Mat4)> {
    let transform = world.get::<Transform>(entity)?;
    let matrix = world.get::<WorldTransform>(entity).map_or_else(|| transform.matrix(), |w| w.0);
    let (_, rotation, position) = matrix.to_scale_rotation_translation();
    Some((position, rotation, matrix))
}

fn vector(value: Option<&toml::Value>) -> Option<Vec3> {
    let items = value?.as_array()?;
    let number = |i: usize| items.get(i).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)));
    Some(Vec3::new(number(0)? as f32, number(1)? as f32, number(2)? as f32))
}

fn number(value: Option<&toml::Value>) -> Option<f32> {
    let value = value?;
    value.as_float().or_else(|| value.as_integer().map(|n| n as f64)).map(|n| n as f32)
}

fn written(vector: Vec3) -> toml::Value {
    toml::Value::Array(vector.to_array().iter().map(|&v| toml::Value::Float(f64::from(v))).collect())
}

/// The direction an arc opens from, and the axis it turns around: the Obj's
/// forward and up.
fn arc_axes(rotation: Quat) -> (Vec3, Vec3) {
    (rotation * Vec3::Z, rotation * Vec3::Y)
}

/// The world point of a box's face, given its centre, rotation and size.
fn face_point(position: Vec3, rotation: Quat, size: Vec3, face: i8) -> Vec3 {
    let axis = face.unsigned_abs() as usize - 1;
    let mut offset = Vec3::ZERO;
    offset[axis] = size[axis] * 0.5 * f32::from(face.signum());
    position + rotation * offset
}

/// Every dot the selected Obj's fields have, with where each is now.
fn grips(world: &World, types: &ScriptTypes, camera: &ViewCamera, entity: Entity) -> Vec<Grip> {
    let Some((position, rotation, matrix)) = frame(world, entity) else { return Vec::new() };
    let mut grips = Vec::new();
    for spec in props::specs(world, entity) {
        let Some(declared) = types.behaviour(&spec.type_name) else { continue };
        for field in &declared.fields {
            let Some(handle) = field.inspector.handle.clone() else { continue };
            let top = vec![Step::Key(field.name.clone())];
            let value = fields::value_at(world, types, entity, &spec.type_name, &top);
            let grip = |path: Vec<Step>, at: Vec3, face: i8| Grip {
                behaviour: spec.type_name.clone(),
                path,
                handle: handle.clone(),
                at,
                face,
            };
            let place = |point: Vec3| if handle.world { point } else { matrix.transform_point3(point) };
            match handle.kind {
                HandleKind::Position if matches!(field.kind, FieldKind::List { .. }) => {
                    let items = value.as_ref().and_then(|v| v.as_array()).cloned().unwrap_or_default();
                    for (index, item) in items.iter().enumerate() {
                        let Some(point) = vector(Some(item)) else { continue };
                        let path = vec![Step::Key(field.name.clone()), Step::Item(index)];
                        grips.push(grip(path, place(point), 0));
                    }
                }
                HandleKind::Position => {
                    if let Some(point) = vector(value.as_ref()) {
                        grips.push(grip(top, place(point), 0));
                    }
                }
                HandleKind::Radius => {
                    let radius = number(value.as_ref()).unwrap_or(0.0);
                    grips.push(grip(top, position + camera.right * radius, 0));
                }
                HandleKind::Direction => {
                    let direction = vector(value.as_ref()).unwrap_or(Vec3::ZERO);
                    let turned = if handle.world { direction } else { rotation * direction };
                    grips.push(grip(top, position + turned, 0));
                }
                HandleKind::Bounds => {
                    let size = vector(value.as_ref()).unwrap_or(Vec3::ONE);
                    for face in [1, -1, 2, -2, 3, -3] {
                        grips.push(grip(top.clone(), face_point(position, rotation, size, face), face));
                    }
                }
                HandleKind::Arc => {
                    let degrees = number(value.as_ref()).unwrap_or(0.0);
                    let (forward, up) = arc_axes(rotation);
                    let reach = handle.radius.unwrap_or(ARC_RADIUS);
                    let end = Quat::from_axis_angle(up, (degrees * 0.5).to_radians()) * forward * reach;
                    grips.push(grip(top, position + end, 0));
                }
                HandleKind::Slider => {
                    let distance = number(value.as_ref()).unwrap_or(0.0);
                    let axis = rotation * Vec3::AXES[handle.axis.min(2)];
                    grips.push(grip(top, position + axis * distance, 0));
                }
            }
        }
    }
    grips
}

impl FieldHandles {
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether the pointer is over a dot, so a press takes it before the UI.
    pub fn is_hovered(&self) -> bool {
        self.hovered.is_some()
    }

    /// Notes which dot is under the pointer, if any, to light it and take a press.
    pub fn hover(
        &mut self,
        world: &World,
        types: &ScriptTypes,
        camera: &ViewCamera,
        entity: Option<Entity>,
        point: Option<[f32; 2]>,
    ) {
        if self.drag.is_some() {
            return;
        }
        self.hovered = entity.zip(point).and_then(|(entity, point)| {
            let grips = grips(world, types, camera, entity);
            let near = |grip: &Grip| camera.project(grip.at).map(|at| (at[0] - point[0]).hypot(at[1] - point[1]));
            let distances = grips.iter().enumerate().filter_map(|(index, grip)| Some((index, near(grip)?)));
            distances.filter(|&(_, d)| d <= REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(index, _)| index)
        });
    }

    /// Takes hold of the dot under `point`, if there is one. Returns whether it did.
    pub fn begin(
        &mut self,
        world: &World,
        types: &ScriptTypes,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
    ) -> bool {
        self.hover(world, types, camera, Some(entity), Some(point));
        let Some(grip) = self.hovered.and_then(|index| grips(world, types, camera, entity).into_iter().nth(index))
        else {
            return false;
        };
        let hit = on_plane(camera.ray(point), grip.at, camera.forward).unwrap_or(grip.at);
        self.drag = Some(Drag { entity, offset: grip.at - hit, grip, facing: camera.forward });
        true
    }

    /// Moves the held dot to a point, writing its field. `snap` is whether Ctrl is held, and `step` the snap distance.
    pub fn drag_to(
        &mut self,
        world: &mut World,
        types: &ScriptTypes,
        camera: &ViewCamera,
        point: [f32; 2],
        (snap, step): (bool, f32),
    ) {
        let Some(drag) = &self.drag else { return };
        let Some((position, rotation, matrix)) = frame(world, drag.entity) else { return };
        let grip = &drag.grip;
        let ray = camera.ray(point);
        let in_view = || on_plane(ray, grip.at, drag.facing).map(|hit| hit + drag.offset);
        let tenth = |value: f32| if snap { snapped(value, 0.1) } else { value };
        let value = match grip.handle.kind {
            HandleKind::Position => {
                let Some(world_point) = in_view() else { return };
                let point =
                    if grip.handle.world { world_point } else { matrix.inverse().transform_point3(world_point) };
                let step = if snap { step } else { 0.0 };
                written(Vec3::from_array(point.to_array().map(|v| snapped(v, step))))
            }
            HandleKind::Radius => {
                let Some(hit) = on_plane(ray, position, drag.facing) else { return };
                toml::Value::Float(f64::from(tenth(hit.distance(position))))
            }
            HandleKind::Direction => {
                let Some(tip) = in_view() else { return };
                let direction = tip - position;
                let direction = if grip.handle.world { direction } else { rotation.inverse() * direction };
                written(Vec3::from_array(direction.to_array().map(tenth)))
            }
            HandleKind::Bounds => {
                let axis_index = grip.face.unsigned_abs() as usize - 1;
                let axis = rotation * Vec3::AXES[axis_index];
                let Some(along) = along_axis(ray, position, axis) else { return };
                let mut size =
                    vector(fields::value_at(world, types, drag.entity, &grip.behaviour, &grip.path).as_ref())
                        .unwrap_or(Vec3::ONE);
                size[axis_index] = tenth((along * f32::from(grip.face.signum()) * 2.0).max(0.01)).max(0.01);
                written(size)
            }
            HandleKind::Arc => {
                let (forward, up) = arc_axes(rotation);
                let Some(hit) = on_plane(ray, position, up) else { return };
                let toward = hit - position;
                let angle = up.dot(forward.cross(toward)).atan2(forward.dot(toward)).abs().to_degrees() * 2.0;
                let angle = if snap { snapped(angle, 5.0) } else { angle };
                toml::Value::Float(f64::from(angle.clamp(0.0, 360.0)))
            }
            HandleKind::Slider => {
                let axis = rotation * Vec3::AXES[grip.handle.axis.min(2)];
                let Some(along) = along_axis(ray, position, axis) else { return };
                toml::Value::Float(f64::from(tenth(along)))
            }
        };
        fields::set_raw_at(world, types, drag.entity, &grip.behaviour, &grip.path, value);
    }

    /// Lets go of the dot, returning the Obj it was on.
    pub fn end(&mut self) -> Option<Entity> {
        self.drag.take().map(|drag| drag.entity)
    }

    /// The handles of the selected Obj's fields, as shapes in the view's own
    /// coordinates (from its top left).
    pub fn shapes(
        &self,
        world: &World,
        types: &ScriptTypes,
        camera: &ViewCamera,
        entity: Option<Entity>,
    ) -> Vec<Shape> {
        let Some(entity) = entity else { return Vec::new() };
        let Some((position, rotation, _)) = frame(world, entity) else { return Vec::new() };
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let on_screen = |p: Vec3| camera.project(p).map(local);
        let held = self.drag.as_ref().map(|drag| &drag.grip);
        let grips = grips(world, types, camera, entity);
        let lit = Color::hex(0xf2c94c);
        let color_of = |grip: &Grip| grip.handle.color.as_deref().and_then(hex).unwrap_or(Color::hex(COLOR));
        let mut shapes = Vec::new();
        let line = |shapes: &mut Vec<Shape>, from: Vec3, to: Vec3, color: Color| {
            if let (Some(from), Some(to)) = (on_screen(from), on_screen(to)) {
                shapes.push(Shape::Line { from, to, width: 1.5, color });
            }
        };
        // The outlines first, then the dots over them.
        for (index, grip) in grips.iter().enumerate() {
            let color = color_of(grip).with_alpha(200);
            match grip.handle.kind {
                HandleKind::Position => {
                    let next =
                        grips.get(index + 1).filter(|n| n.behaviour == grip.behaviour && n.path[0] == grip.path[0]);
                    if let Some(next) = next.filter(|_| grip.path.len() > 1) {
                        line(&mut shapes, grip.at, next.at, color);
                    }
                }
                HandleKind::Radius => {
                    let radius = grip.at.distance(position);
                    let points = ring(camera, position, camera.forward, radius);
                    for pair in points.windows(2) {
                        shapes.push(Shape::Line { from: local(pair[0]), to: local(pair[1]), width: 1.5, color });
                    }
                }
                HandleKind::Direction | HandleKind::Slider => line(&mut shapes, position, grip.at, color),
                HandleKind::Bounds if grip.face == 1 => {
                    let size = Vec3::new(
                        grip.at.distance(position) * 2.0,
                        grips[index + 2].at.distance(position) * 2.0,
                        grips[index + 4].at.distance(position) * 2.0,
                    );
                    let corner = |i: usize| {
                        let sign = Vec3::new(
                            if i & 1 == 0 { -0.5 } else { 0.5 },
                            if i & 2 == 0 { -0.5 } else { 0.5 },
                            if i & 4 == 0 { -0.5 } else { 0.5 },
                        );
                        position + rotation * (size * sign)
                    };
                    for a in 0..8 {
                        for bit in [1, 2, 4] {
                            if a & bit == 0 {
                                line(&mut shapes, corner(a), corner(a | bit), color);
                            }
                        }
                    }
                }
                HandleKind::Bounds => {}
                HandleKind::Arc => {
                    let (forward, up) = arc_axes(rotation);
                    let reach = grip.handle.radius.unwrap_or(ARC_RADIUS);
                    let half = up.dot(forward.cross(grip.at - position)).atan2(forward.dot(grip.at - position));
                    let point = |t: f32| position + Quat::from_axis_angle(up, half * t) * forward * reach;
                    line(&mut shapes, position, point(-1.0), color);
                    line(&mut shapes, position, point(1.0), color);
                    for step in 0..ARC_SEGMENTS {
                        let t = step as f32 / ARC_SEGMENTS as f32 * 2.0 - 1.0;
                        line(&mut shapes, point(t), point(t + 2.0 / ARC_SEGMENTS as f32), color);
                    }
                }
            }
        }
        for (index, grip) in grips.iter().enumerate() {
            let Some(center) = on_screen(grip.at) else { continue };
            let hot = self.hovered == Some(index) || held.is_some_and(|h| h.path == grip.path && h.face == grip.face);
            let fill = if hot { lit } else { color_of(grip) };
            shapes.push(Shape::Circle { center, radius: DOT, fill, border: Some((1.0, Color::rgba(12, 14, 16, 200))) });
        }
        shapes
    }
}

/// A colour from hex such as "#ff8800".
fn hex(text: &str) -> Option<Color> {
    u32::from_str_radix(text.trim_start_matches('#'), 16)
        .ok()
        .filter(|_| text.trim_start_matches('#').len() == 6)
        .map(Color::hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_from_hex() {
        assert_eq!(hex("#ff8800"), Some(Color::hex(0xff8800)));
        assert_eq!(hex("ff8800"), Some(Color::hex(0xff8800)));
        assert_eq!(hex("#f80"), None, "only the long form");
        assert_eq!(hex("nonsense"), None);
    }

    #[test]
    fn a_box_face_sits_half_its_size_out_along_its_axis() {
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let size = Vec3::new(4.0, 2.0, 6.0);
        assert!((face_point(Vec3::ZERO, Quat::IDENTITY, size, 1) - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-5);
        assert!((face_point(Vec3::ZERO, Quat::IDENTITY, size, -3) - Vec3::new(0.0, 0.0, -3.0)).length() < 1e-5);
        assert!(
            (face_point(Vec3::ZERO, turned, size, 1) - Vec3::new(0.0, 0.0, -2.0)).length() < 1e-5,
            "turned with the Obj"
        );
    }
}
