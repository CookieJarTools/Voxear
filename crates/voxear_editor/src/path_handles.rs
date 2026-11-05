//! Handles that edit the selected Obj's Path in the Scene view: a dot on
//! each point to drag it, a smaller one at its side to drag how wide the
//! Path is there, a faint one halfway along each stretch that adds a point
//! there as it is dragged, and one past an open Path's end that adds a point
//! after its last. Shift-clicking a point takes it away, down to two. Each
//! drag or click is one step to undo; holding Ctrl snaps a point to the snap
//! distance and a width to tenths of a metre.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_scene::{Path, PathPoint};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{ViewCamera, along_axis, on_plane, snapped};

/// How near the pointer must be to a dot to take it, in logical pixels.
const REACH: f32 = 9.0;
/// The dots' radii on screen: a point's, a width's and an added point's.
const DOT: f32 = 5.0;
const SMALL: f32 = 3.5;
const COLOR: u32 = 0x5ad1e6;
const LIT: u32 = 0xf2c94c;
/// How far past an open Path's last point its dot to add another is, in metres.
const ONWARD: f32 = 4.0;

/// What a dot does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// Moves point `i`.
    Point(usize),
    /// Sets how wide the Path is at point `i`.
    Width(usize),
    /// Adds a point halfway along stretch `i`, after point `i`.
    Insert(usize),
    /// Adds a point after the last.
    Extend,
}

/// A drag under way: the Obj, the point or width it moves, where the pointer
/// took hold from the dot, and the view's forward then.
#[derive(Clone, Copy, Debug)]
struct Drag {
    entity: Entity,
    grip: Grip,
    offset: Vec3,
    facing: Vec3,
}

/// The selected Obj's Path's handles, and any drag of one.
#[derive(Default)]
pub struct PathHandles {
    drag: Option<Drag>,
    hovered: Option<Grip>,
    /// An Obj whose point a click took away, for the step to undo.
    removed: Option<Entity>,
}

/// Which way is across a Path where it runs along `tangent`: level, to its
/// right.
fn across(tangent: Vec3) -> Vec3 {
    Vec3::new(tangent.z, 0.0, -tangent.x).normalize_or(Vec3::X)
}

/// Every dot the Path has, with where each is in the world.
fn grips(path: &Path, to_world: Mat4) -> Vec<(Grip, Vec3)> {
    let place = |p: Vec3| to_world.transform_point3(p);
    let mut grips = Vec::new();
    for i in 0..path.points.len() {
        let at = place(Vec3::from(path.points[i].position));
        grips.push((Grip::Point(i), at));
        // At its side, across the curve there.
        let (before, after) = (path.points[i.saturating_sub(1)].position, path.points[(i + 1).min(path.points.len() - 1)].position);
        let tangent = to_world.transform_vector3(Vec3::from(after) - Vec3::from(before));
        grips.push((Grip::Width(i), at + across(tangent) * path.points[i].width * 0.5));
    }
    for segment in 0..path.segments() {
        grips.push((Grip::Insert(segment), place(path.point_at(segment, 0.5).0)));
    }
    if !path.closed && path.points.len() >= 2 {
        let n = path.points.len();
        let (last, before) = (Vec3::from(path.points[n - 1].position), Vec3::from(path.points[n - 2].position));
        let onward = (last - before).normalize_or(Vec3::Z) * ONWARD;
        grips.push((Grip::Extend, place(last + onward)));
    }
    grips
}

fn placed(world: &World, entity: Entity) -> Option<(Path, Mat4)> {
    Some((world.get::<Path>(entity)?.clone(), voxear_scene::world_matrix(world, entity)))
}

impl PathHandles {
    /// Whether a dot is held, or a click took a point away and the button
    /// is not yet up.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some() || self.removed.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.hovered.is_some()
    }

    /// Notes which dot is under the pointer, if any.
    pub fn hover(&mut self, world: &World, camera: &ViewCamera, entity: Option<Entity>, point: Option<[f32; 2]>) {
        if self.drag.is_some() {
            return;
        }
        self.hovered = entity.zip(point).and_then(|(entity, point)| {
            let (path, to_world) = placed(world, entity)?;
            let near = |at: Vec3| camera.project(at).map(|p| (p[0] - point[0]).hypot(p[1] - point[1]));
            // A point before its width, and both before the dots that add points, when they are as near.
            let distances = grips(&path, to_world).into_iter().filter_map(|(grip, at)| Some((grip, near(at)?)));
            distances.filter(|&(_, d)| d <= REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(grip, _)| grip)
        });
    }

    /// Takes hold of the dot under `point`: adding the point it stands for
    /// first, or, with `remove` (Shift), taking a point away. Returns
    /// whether it took one.
    pub fn begin(&mut self, world: &mut World, camera: &ViewCamera, entity: Entity, point: [f32; 2], remove: bool) -> bool {
        self.hover(world, camera, Some(entity), Some(point));
        let Some(grip) = self.hovered else { return false };
        let Some((mut path, to_world)) = placed(world, entity) else { return false };
        let grip = match grip {
            Grip::Point(i) if remove => {
                if path.points.len() > 2 {
                    path.points.remove(i);
                    world.insert(entity, path);
                    self.removed = Some(entity);
                }
                self.hovered = None;
                return self.removed.is_some();
            }
            _ if remove => return false,
            Grip::Insert(segment) => {
                let (at, width) = path.point_at(segment, 0.5);
                let index = (segment + 1).min(path.points.len());
                path.points.insert(index, PathPoint { position: at.to_array(), width });
                world.insert(entity, path);
                Grip::Point(index)
            }
            Grip::Extend => {
                let at = grips(&path, to_world).into_iter().find(|(g, _)| *g == Grip::Extend).map(|(_, at)| at);
                let last = *path.points.last().expect("an open Path with points");
                let local = to_world.inverse().transform_point3(at.unwrap_or(Vec3::ZERO));
                path.points.push(PathPoint { position: local.to_array(), ..last });
                let index = path.points.len() - 1;
                world.insert(entity, path);
                Grip::Point(index)
            }
            grip => grip,
        };
        let Some((path, to_world)) = placed(world, entity) else { return false };
        let at = grips(&path, to_world).into_iter().find(|(g, _)| *g == grip).map_or(Vec3::ZERO, |(_, at)| at);
        let hit = on_plane(camera.ray(point), at, camera.forward).unwrap_or(at);
        self.drag = Some(Drag { entity, grip, offset: at - hit, facing: camera.forward });
        self.hovered = Some(grip);
        true
    }

    /// Moves the held dot to a point. `snap` is whether Ctrl is held, and `step` the snap distance.
    pub fn drag_to(&mut self, world: &mut World, camera: &ViewCamera, point: [f32; 2], (snap, step): (bool, f32)) {
        let Some(drag) = self.drag else { return };
        let Some((mut path, to_world)) = placed(world, drag.entity) else { return };
        let ray = camera.ray(point);
        let Some(&(_, at)) = grips(&path, to_world).iter().find(|(g, _)| *g == drag.grip) else { return };
        match drag.grip {
            Grip::Point(i) => {
                let Some(hit) = on_plane(ray, at, drag.facing) else { return };
                let local = to_world.inverse().transform_point3(hit + drag.offset);
                let step = if snap { step } else { 0.0 };
                path.points[i].position = local.to_array().map(|v| snapped(v, step));
            }
            Grip::Width(i) => {
                let middle = to_world.transform_point3(Vec3::from(path.points[i].position));
                let Some(along) = along_axis(ray, middle, (at - middle).normalize_or(Vec3::X)) else { return };
                let width = (along * 2.0).max(0.1);
                path.points[i].width = if snap { snapped(width, 0.1).max(0.1) } else { width };
            }
            Grip::Insert(_) | Grip::Extend => return,
        }
        world.insert(drag.entity, path);
    }

    /// Lets go of the dot, or ends a click that took a point away, returning
    /// the Obj it was on.
    pub fn end(&mut self) -> Option<Entity> {
        self.drag.take().map(|drag| drag.entity).or_else(|| self.removed.take())
    }

    /// The selected Obj's Path's dots, as shapes in the view's own
    /// coordinates (from its top left). Its curve is a gizmo's.
    pub fn shapes(&self, world: &World, camera: &ViewCamera, entity: Option<Entity>) -> Vec<Shape> {
        let Some((path, to_world)) = entity.and_then(|entity| placed(world, entity)) else { return Vec::new() };
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let held = self.drag.map(|drag| drag.grip);
        let mut shapes = Vec::new();
        let all = grips(&path, to_world);
        // Each width's dot joined to its point.
        for &(grip, at) in &all {
            let Grip::Width(i) = grip else { continue };
            let middle = to_world.transform_point3(Vec3::from(path.points[i].position));
            if let (Some(from), Some(to)) = (camera.project(middle), camera.project(at)) {
                shapes.push(Shape::Line { from: local(from), to: local(to), width: 1.0, color: Color::hex(COLOR).with_alpha(140) });
            }
        }
        for (grip, at) in all {
            let Some(center) = camera.project(at).map(local) else { continue };
            let hot = self.hovered == Some(grip) || held == Some(grip);
            let (radius, alpha) = match grip {
                Grip::Point(_) => (DOT, 255),
                Grip::Width(_) => (SMALL, 255),
                Grip::Insert(_) | Grip::Extend => (SMALL, 110),
            };
            let fill = if hot { Color::hex(LIT) } else { Color::hex(COLOR).with_alpha(alpha) };
            shapes.push(Shape::Circle { center, radius, fill, border: Some((1.0, Color::rgba(12, 14, 16, 200))) });
        }
        shapes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::Quat;
    use voxear_render::Camera;
    use voxear_scene::Transform;

    #[test]
    fn dragging_the_middle_of_a_stretch_adds_a_point_there_and_shift_click_takes_it_away() {
        let mut world = World::new();
        let down = Quat::from_rotation_arc(Vec3::Z, Vec3::NEG_Y);
        let lens = world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 30.0, 10.0), down, Vec3::ONE)));
        let camera = ViewCamera::new(&world, lens, [0.0, 0.0, 800.0, 600.0]).unwrap();
        let entity = world.spawn((Path::default(), Transform::default()));
        let mut handles = PathHandles::default();
        let middle = Path::default().point_at(0, 0.5).0;
        let at = camera.project(middle).expect("in view");
        assert!(handles.begin(&mut world, &camera, entity, at, false), "the dot halfway along the first stretch");
        assert_eq!(world.get::<Path>(entity).unwrap().points.len(), 4);
        let aside = camera.project(middle + Vec3::new(3.0, 0.0, 0.0)).unwrap();
        handles.drag_to(&mut world, &camera, aside, (false, 1.0));
        assert_eq!(handles.end(), Some(entity));
        let added = world.get::<Path>(entity).unwrap().points[1];
        assert!(Vec3::from(added.position).distance(middle + Vec3::new(3.0, 0.0, 0.0)) < 0.01, "dragged aside: {:?}", added.position);
        let now = camera.project(Vec3::from(added.position)).unwrap();
        assert!(handles.begin(&mut world, &camera, entity, now, true));
        assert_eq!(handles.end(), Some(entity), "one step to undo");
        assert_eq!(world.get::<Path>(entity).unwrap(), &Path::default(), "back as it was");
    }
}
