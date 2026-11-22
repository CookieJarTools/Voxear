//! Handles that edit the selected Obj's Road or Route in the Scene view: its
//! middle line and edges drawn over the ground, a dot on each point to drag
//! it (it lands on the ground as it is without the road), a smaller one at
//! its side to drag how wide it is there, a faint one halfway along each
//! stretch that adds a point there, and one past its end that adds a point
//! after its last. Shift-clicking a point takes it away, down to two. An end
//! of a road let go of on another road snaps onto that road's middle, so the
//! two join. A World Area's or an Area Fill's outline is edited the same way,
//! as a loop, with no widths and no end. Each drag or click is one step to
//! undo.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    roads::{AreaFill, Road, RoadPoint},
    world_areas::WorldArea,
    world_routes::Route,
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

/// What the handles edit: a Road's points, a Route's, or the outline of a
/// World Area or an Area Fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Road,
    Route,
    Area,
    Fill,
}

/// The points edited, in the Obj's own space, each with its width; whether
/// they make a loop.
#[derive(Clone, Debug)]
struct Lined {
    source: Source,
    points: Vec<RoadPoint>,
}

impl Lined {
    fn closed(&self) -> bool {
        matches!(self.source, Source::Area | Source::Fill)
    }

    /// The fewest points it keeps.
    fn fewest(&self) -> usize {
        if self.closed() { 3 } else { 2 }
    }

    /// How many metres its width is a unit of: a road's blocks, a route's metres.
    fn unit(&self, world: &World, entity: Entity) -> f32 {
        match self.source {
            Source::Road => block_size(world, entity),
            _ => 1.0,
        }
    }

    fn of(world: &World, entity: Entity) -> Option<Lined> {
        let outline = |outline: &[[f32; 2]]| {
            outline.iter().map(|&[x, z]| RoadPoint { at: [x, 0.0, z], width: 0.0 }).collect::<Vec<_>>()
        };
        if let Some(road) = world.get::<Road>(entity) {
            return Some(Lined { source: Source::Road, points: road.points.clone() });
        }
        if let Some(route) = world.get::<Route>(entity) {
            let points = route.points.iter().map(|&at| RoadPoint { at, width: route.width }).collect();
            return Some(Lined { source: Source::Route, points });
        }
        if let Some(area) =
            world.get::<WorldArea>(entity).filter(|area| area.shape == voxear_engine::world_areas::AreaShape::Outline)
        {
            return Some(Lined { source: Source::Area, points: outline(&area.outline) });
        }
        let fill = world.get::<AreaFill>(entity)?;
        Some(Lined { source: Source::Fill, points: outline(&fill.outline) })
    }

    /// Puts its points back in the component they came from.
    fn store(self, world: &mut World, entity: Entity) {
        let outline: Vec<[f32; 2]> = self.points.iter().map(|point| [point.at[0], point.at[2]]).collect();
        match self.source {
            Source::Road => {
                if let Some(road) = world.get_mut::<Road>(entity) {
                    road.points = self.points;
                }
            }
            Source::Route => {
                if let Some(route) = world.get_mut::<Route>(entity) {
                    route.width = self.points.first().map_or(route.width, |point| point.width);
                    route.points = self.points.iter().map(|point| point.at).collect();
                }
            }
            Source::Area => {
                if let Some(area) = world.get_mut::<WorldArea>(entity) {
                    area.outline = outline;
                }
            }
            Source::Fill => {
                if let Some(fill) = world.get_mut::<AreaFill>(entity) {
                    fill.outline = outline;
                }
            }
        }
    }
}

fn grips(lined: &Lined, to_world: Mat4, size: f32) -> Vec<(Grip, Vec3)> {
    let place = |p: [f32; 3]| to_world.transform_point3(Vec3::from(p));
    let points = &lined.points;
    let n = points.len();
    let mut grips = Vec::new();
    for i in 0..n {
        let at = place(points[i].at);
        grips.push((Grip::Point(i), at));
        if !lined.closed() {
            let tangent = place(points[(i + 1).min(n - 1)].at) - place(points[i.saturating_sub(1)].at);
            grips.push((Grip::Width(i), at + across(tangent) * points[i].width * size * 0.5));
        }
    }
    let stretches = if lined.closed() { n } else { n.saturating_sub(1) };
    for i in 0..stretches {
        grips.push((Grip::Insert(i), (place(points[i].at) + place(points[(i + 1) % n].at)) * 0.5));
    }
    if n >= 2 && !lined.closed() {
        let (last, before) = (place(points[n - 1].at), place(points[n - 2].at));
        grips.push((Grip::Extend, last + (last - before).normalize_or(Vec3::Z) * ONWARD));
    }
    grips
}

fn placed(world: &World, entity: Entity) -> Option<(Lined, Mat4)> {
    Some((Lined::of(world, entity)?, voxear_scene::world_matrix(world, entity)))
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
            let all = grips(&road, to_world, road.unit(world, entity));
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
        let size = road.unit(world, entity);
        let grip = match grip {
            Grip::Point(i) if remove => {
                if road.points.len() > road.fewest() {
                    road.points.remove(i);
                    road.store(world, entity);
                    self.removed = Some(entity);
                }
                self.hovered = None;
                return self.removed.is_some();
            }
            _ if remove => return false,
            Grip::Insert(i) => {
                let (a, b) = (road.points[i], road.points[(i + 1) % road.points.len()]);
                let at = [0, 1, 2].map(|axis| (a.at[axis] + b.at[axis]) * 0.5);
                road.points.insert(i + 1, RoadPoint { at, width: (a.width + b.width) * 0.5 });
                road.store(world, entity);
                Grip::Point(i + 1)
            }
            Grip::Extend => {
                let at = grips(&road, to_world, size).into_iter().find(|(g, _)| *g == Grip::Extend).map(|(_, at)| at);
                let last = *road.points.last().expect("a road has two points at least");
                let local = to_world.inverse().transform_point3(at.unwrap_or(Vec3::ZERO));
                road.points.push(RoadPoint { at: local.to_array(), ..last });
                let index = road.points.len() - 1;
                road.store(world, entity);
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
        let size = road.unit(world, drag.entity);
        let ray = camera.ray(point);
        let Some(&(_, at)) = grips(&road, to_world, size).iter().find(|(g, _)| *g == drag.grip) else { return };
        match drag.grip {
            Grip::Point(i) => {
                let Some(hit) = on_plane(ray, at, drag.facing) else { return };
                let mut moved = hit + drag.offset;
                // Only a road's ends join another road; an outline keeps to its Obj's level.
                let end = road.source == Source::Road && (i == 0 || i + 1 == road.points.len());
                if road.closed() {
                    let local = to_world.inverse().transform_point3(moved);
                    road.points[i].at = [local.x, 0.0, local.z];
                    road.store(&mut game.engine.world, drag.entity);
                    return;
                }
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
                let width = match road.source {
                    Source::Road => (along * 2.0 / size).round().clamp(1.0, 64.0),
                    _ => (along * 2.0).clamp(0.5, 500.0),
                };
                // A route is as wide all along.
                match road.source {
                    Source::Road => road.points[i].width = width,
                    _ => road.points.iter_mut().for_each(|point| point.width = width),
                }
            }
            Grip::Insert(_) | Grip::Extend => return,
        }
        road.store(&mut game.engine.world, drag.entity);
    }

    pub fn end(&mut self) -> Option<Entity> {
        self.drag.take().map(|drag| drag.entity).or_else(|| self.removed.take())
    }

    /// The selected road's middle line, edges and dots, in the view's own
    /// coordinates.
    pub fn shapes(&self, world: &World, camera: &ViewCamera, entity: Option<Entity>) -> Vec<Shape> {
        let Some(entity) = entity else { return Vec::new() };
        let Some((road, to_world)) = placed(world, entity) else { return Vec::new() };
        let size = road.unit(world, entity);
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
        if road.closed() {
            for (i, &at) in points.iter().enumerate() {
                line(&mut shapes, at, points[(i + 1) % points.len()], 2.0, 230);
            }
        }
        for (i, pair) in points.windows(2).enumerate().filter(|_| !road.closed()) {
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
