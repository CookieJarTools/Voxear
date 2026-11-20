//! Handles that edit the selected Obj's Road in the Scene view: its middle
//! line and edges drawn over the ground, a dot on each point to drag it (it
//! lands on the ground as it is without the road), a smaller one at its side
//! to drag how wide the road is there, a faint one halfway along each stretch
//! that adds a point there, and one past its end that adds a point after its
//! last. Shift-clicking a point takes it away, down to two. An end let go of
//! on another road snaps onto that road's middle, so the two join. Each drag
//! or click is one step to undo.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    roads::{Road, RoadPoint},
};
use voxear_ui::{Color, Shape};

use crate::scene_tools::{ViewCamera, along_axis, on_plane};

/// How near the pointer must be to a dot to take it, in logical pixels.
const REACH: f32 = 9.0;
const DOT: f32 = 5.0;
const SMALL: f32 = 3.5;
const COLOR: u32 = 0xe6a85a;
const LIT: u32 = 0xf2c94c;
/// How far past its last point its dot to add another is, in metres.
const ONWARD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    Point(usize),
    Width(usize),
    Insert(usize),
    Extend,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    entity: Entity,
    grip: Grip,
    offset: Vec3,
    facing: Vec3,
}

/// The selected Obj's Road's handles, and any drag of one.
#[derive(Default)]
pub struct RoadHandles {
    drag: Option<Drag>,
    hovered: Option<Grip>,
    removed: Option<Entity>,
}

fn across(tangent: Vec3) -> Vec3 {
    Vec3::new(tangent.z, 0.0, -tangent.x).normalize_or(Vec3::X)
}

/// How many metres a block of the world of blocks under `entity` is.
fn block_size(world: &World, entity: Entity) -> f32 {
    voxear_engine::roads::world_of(world, entity)
        .and_then(|blocks| {
            world.get::<voxear_engine::blocks::BlockWorldAsset>(blocks).map(|asset| asset.file.block_size)
        })
        .unwrap_or(1.0)
        .max(0.01)
}

fn grips(road: &Road, to_world: Mat4, size: f32) -> Vec<(Grip, Vec3)> {
    let place = |p: [f32; 3]| to_world.transform_point3(Vec3::from(p));
    let n = road.points.len();
    let mut grips = Vec::new();
    for i in 0..n {
        let at = place(road.points[i].at);
        grips.push((Grip::Point(i), at));
        let tangent = place(road.points[(i + 1).min(n - 1)].at) - place(road.points[i.saturating_sub(1)].at);
        grips.push((Grip::Width(i), at + across(tangent) * road.points[i].width * size * 0.5));
    }
    for i in 0..n.saturating_sub(1) {
        grips.push((Grip::Insert(i), (place(road.points[i].at) + place(road.points[i + 1].at)) * 0.5));
    }
    if n >= 2 {
        let (last, before) = (place(road.points[n - 1].at), place(road.points[n - 2].at));
        grips.push((Grip::Extend, last + (last - before).normalize_or(Vec3::Z) * ONWARD));
    }
    grips
}

fn placed(world: &World, entity: Entity) -> Option<(Road, Mat4)> {
    Some((world.get::<Road>(entity)?.clone(), voxear_scene::world_matrix(world, entity)))
}

impl RoadHandles {
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
            let (road, to_world) = placed(world, entity)?;
            let near = |at: Vec3| camera.project(at).map(|p| (p[0] - point[0]).hypot(p[1] - point[1]));
            let all = grips(&road, to_world, block_size(world, entity));
            let distances = all.into_iter().filter_map(|(grip, at)| Some((grip, near(at)?)));
            distances.filter(|&(_, d)| d <= REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(grip, _)| grip)
        });
    }

    /// Takes hold of the dot under `point`, adding the point it stands for
    /// first, or with `remove` (Shift) taking a point away.
    pub fn begin(
        &mut self,
        world: &mut World,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
        remove: bool,
    ) -> bool {
        self.hover(world, camera, Some(entity), Some(point));
        let Some(grip) = self.hovered else { return false };
        let Some((mut road, to_world)) = placed(world, entity) else { return false };
        let size = block_size(world, entity);
        let grip = match grip {
            Grip::Point(i) if remove => {
                if road.points.len() > 2 {
                    road.points.remove(i);
                    world.insert(entity, road);
                    self.removed = Some(entity);
                }
                self.hovered = None;
                return self.removed.is_some();
            }
            _ if remove => return false,
            Grip::Insert(i) => {
                let (a, b) = (road.points[i], road.points[i + 1]);
                let at = [0, 1, 2].map(|axis| (a.at[axis] + b.at[axis]) * 0.5);
                road.points.insert(i + 1, RoadPoint { at, width: (a.width + b.width) * 0.5 });
                world.insert(entity, road);
                Grip::Point(i + 1)
            }
            Grip::Extend => {
                let at = grips(&road, to_world, size).into_iter().find(|(g, _)| *g == Grip::Extend).map(|(_, at)| at);
                let last = *road.points.last().expect("a road has two points at least");
                let local = to_world.inverse().transform_point3(at.unwrap_or(Vec3::ZERO));
                road.points.push(RoadPoint { at: local.to_array(), ..last });
                let index = road.points.len() - 1;
                world.insert(entity, road);
                Grip::Point(index)
            }
            grip => grip,
        };
        let Some((road, to_world)) = placed(world, entity) else { return false };
        let at = grips(&road, to_world, size).into_iter().find(|(g, _)| *g == grip).map_or(Vec3::ZERO, |(_, at)| at);
        // Points slide over the ground, on a level plane through where they were.
        let facing = match grip {
            Grip::Point(_) => Vec3::NEG_Y,
            _ => camera.forward,
        };
        let hit = on_plane(camera.ray(point), at, facing).unwrap_or(at);
        self.drag = Some(Drag { entity, grip, offset: at - hit, facing });
        self.hovered = Some(grip);
        true
    }

    /// Moves the held dot to a point: a road's point onto the ground under
    /// it, or onto the middle of another road it is let go of over, at an end.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2]) {
        let Some(drag) = self.drag else { return };
        let world = &game.engine.world;
        let Some((mut road, to_world)) = placed(world, drag.entity) else { return };
        let size = block_size(world, drag.entity);
        let ray = camera.ray(point);
        let Some(&(_, at)) = grips(&road, to_world, size).iter().find(|(g, _)| *g == drag.grip) else { return };
        match drag.grip {
            Grip::Point(i) => {
                let Some(hit) = on_plane(ray, at, drag.facing) else { return };
                let mut moved = hit + drag.offset;
                let end = i == 0 || i + 1 == road.points.len();
                let blocks = voxear_engine::roads::world_of(world, drag.entity);
                let reach = road.points[i].width * size;
                let joined = blocks
                    .filter(|_| end)
                    .and_then(|blocks| voxear_engine::roads::nearest(world, blocks, moved, reach, Some(drag.entity)));
                moved = match joined {
                    Some((_, middle, _)) => middle,
                    None => {
                        let (engine, content) = (&mut game.engine, &mut game.content);
                        let ground = voxear_engine::ground_fit::ground_under(
                            &mut engine.world,
                            content,
                            moved,
                            Some(drag.entity),
                        );
                        Vec3::new(moved.x, ground.unwrap_or(moved.y), moved.z)
                    }
                };
                road.points[i].at = to_world.inverse().transform_point3(moved).to_array();
            }
            Grip::Width(i) => {
                let middle = to_world.transform_point3(Vec3::from(road.points[i].at));
                let Some(along) = along_axis(ray, middle, (at - middle).normalize_or(Vec3::X)) else { return };
                road.points[i].width = (along * 2.0 / size).round().clamp(1.0, 64.0);
            }
            Grip::Insert(_) | Grip::Extend => return,
        }
        game.engine.world.insert(drag.entity, road);
    }

    pub fn end(&mut self) -> Option<Entity> {
        self.drag.take().map(|drag| drag.entity).or_else(|| self.removed.take())
    }

    /// The selected road's middle line, edges and dots, in the view's own
    /// coordinates.
    pub fn shapes(&self, world: &World, camera: &ViewCamera, entity: Option<Entity>) -> Vec<Shape> {
        let Some(entity) = entity else { return Vec::new() };
        let Some((road, to_world)) = placed(world, entity) else { return Vec::new() };
        let size = block_size(world, entity);
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let line = |shapes: &mut Vec<Shape>, from: Vec3, to: Vec3, width: f32, alpha: u8| {
            if let (Some(a), Some(b)) = (camera.project(from), camera.project(to)) {
                shapes.push(Shape::Line {
                    from: local(a),
                    to: local(b),
                    width,
                    color: Color::hex(COLOR).with_alpha(alpha),
                });
            }
        };
        let mut shapes = Vec::new();
        let points: Vec<Vec3> = road.points.iter().map(|p| to_world.transform_point3(Vec3::from(p.at))).collect();
        for (i, pair) in points.windows(2).enumerate() {
            line(&mut shapes, pair[0], pair[1], 2.0, 230);
            let side = across(pair[1] - pair[0]);
            let (a, b) = (road.points[i].width * size * 0.5, road.points[i + 1].width * size * 0.5);
            line(&mut shapes, pair[0] + side * a, pair[1] + side * b, 1.0, 140);
            line(&mut shapes, pair[0] - side * a, pair[1] - side * b, 1.0, 140);
        }
        let held = self.drag.map(|drag| drag.grip);
        for (grip, at) in grips(&road, to_world, size) {
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
