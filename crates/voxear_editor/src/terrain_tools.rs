//! A Terrain's tools in the Scene view.
//!
//! Under **Sculpt**, a ring on the ground under the pointer shows the brush,
//! and holding the left button paints with it, a dab each frame along where
//! the pointer went; Shift lowers rather than raises, and Ctrl+click with
//! Set Height takes the height under the pointer. The brush paints into the
//! picked layer when it is a sculpt layer, else the topmost, else a new one.
//! The Holes brush cuts holes instead, and with Shift held as the stroke
//! begins fills them.
//!
//! Under **Paint**, the same ring paints the picked surface over the others,
//! and Shift takes it away.
//!
//! Under **Layers**, a picked Stamp shows its outline on the ground, with a
//! dot in its middle to move it, one past its far side to turn it and one
//! at a corner to size it; a picked Spline shows a dot on each point, to
//! move it over the ground. Each stroke or drag is one step to undo.
//!
//! While a layer is painted or dragged, the ground below it is kept, so
//! each frame makes only that layer and the ones above it again, over the
//! tiles it reaches.

use std::{sync::Arc, time::Instant};

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Game, terrain::TerrainAsset};
use voxear_terrain::{
    Terrain,
    brush::{self, BrushKind},
    composite::{Compositor, Dirty},
    layers::{Area, LayerKind},
};
use voxear_ui::{Color, Shape};

use crate::{
    scene_tools::ViewCamera,
    terrain_props::{self, TerrainTool},
};

/// How near the pointer must be to a dot to take it, in logical pixels.
const REACH: f32 = 9.0;
const DOT: f32 = 5.0;
const COLOR: u32 = 0x5ad1e6;
const LIT: u32 = 0xf2c94c;
/// How far apart a stroke's dabs are, as a share of the brush's radius.
const SPACING: f32 = 0.25;
/// How long one frame's painting may count for, so a stall does not dig a pit.
const LONGEST_FRAME: f32 = 0.1;

/// What a dot on a picked layer does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// Moves a stamp.
    Middle,
    /// Turns a stamp.
    Turn,
    /// Sizes a stamp, both ways at once.
    Corner,
    /// Moves a spline's point `i`.
    Point(usize),
}

/// A stroke under way: the terrain, what it paints, the height Flatten
/// brings the ground to, where the last dab was, and when.
struct Stroke {
    entity: Entity,
    paints: Paints,
    flatten_to: f32,
    last: [f32; 2],
    at: Instant,
}

/// What a stroke paints.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Paints {
    /// A sculpt layer's raster.
    Sculpt(u32),
    /// The terrain's holes: cut, or filled with Shift held as the stroke
    /// began, so letting Shift go first does not cut again.
    Holes { fill: bool },
    /// Surface `n` over the others.
    Surface(usize),
}

/// A layer's dot held: the terrain, the layer, the dot, how high it was
/// taken, and where the pointer took hold from it, across the level it is
/// dragged over.
struct Hold {
    entity: Entity,
    layer: usize,
    grip: Grip,
    level: f32,
    offset: [f32; 2],
}

/// The Scene view's terrain tools.
#[derive(Default)]
pub struct TerrainTools {
    compositor: Compositor,
    /// The ground under the pointer, in the terrain's space, while the brush
    /// shows: where it is and which way it faces.
    under: Option<(Entity, Vec3)>,
    hovered: Option<Grip>,
    stroke: Option<Stroke>,
    hold: Option<Hold>,
}

/// Where a ray through `point` meets an Obj's terrain, in its own space,
/// its holes' ground too, so a brush can fill them.
fn ground_under(game: &Game, camera: &ViewCamera, entity: Entity, point: [f32; 2]) -> Option<Vec3> {
    let world = &game.engine.world;
    let terrain = world.get::<Terrain>(entity)?;
    let corner = voxear_terrain::corner(world.get(entity)?, world.get(entity));
    let (origin, direction) = camera.ray(point);
    voxear_terrain::ray::raycast_whole(&terrain.heights, origin - corner, direction, 20_000.0).map(|hit| hit.point)
}

/// Where a ray through `point` crosses the level `level` metres up in an
/// Obj's terrain's space: the plane a held dot is dragged over, so it
/// follows the pointer past the ground's edge and over the sky.
fn on_level(game: &Game, camera: &ViewCamera, entity: Entity, point: [f32; 2], level: f32) -> Option<[f32; 2]> {
    let corner = corner_of(game, entity)?;
    let (origin, direction) = camera.ray(point);
    let origin = origin - corner;
    if direction.y.abs() < 1e-5 {
        return None;
    }
    let along = (level - origin.y) / direction.y;
    (along > 0.0).then_some([origin.x + direction.x * along, origin.z + direction.z * along])
}

/// The corner of an Obj's terrain in the world.
fn corner_of(game: &Game, entity: Entity) -> Option<Vec3> {
    let world = &game.engine.world;
    Some(voxear_terrain::corner(world.get(entity)?, world.get(entity)))
}

/// The dots of the picked layer, a stamp's or a spline's, where they are in
/// the terrain's space.
fn grips(game: &Game, entity: Entity) -> Vec<(Grip, Vec3)> {
    let world = &game.engine.world;
    let ui = terrain_props::ui(world);
    if ui.tool != TerrainTool::Layers {
        return Vec::new();
    }
    let (Some(asset), Some(terrain)) = (world.get::<TerrainAsset>(entity), world.get::<Terrain>(entity)) else {
        return Vec::new();
    };
    let ground = |x: f32, z: f32| Vec3::new(x, terrain.heights.height_at(x, z), z);
    match asset.file.layers.get(ui.layer).map(|layer| &layer.kind) {
        Some(LayerKind::Stamp { position: [x, z], rotation, size: [w, l], .. }) => {
            let (sin, cos) = rotation.to_radians().sin_cos();
            // The stamp's own +x and +z, turned as it is.
            let along = |u: f32, v: f32| (x + u * cos + v * sin, z - u * sin + v * cos);
            let (tx, tz) = along(0.0, l * 0.5 + 2.0);
            let (cx, cz) = along(w * 0.5, l * 0.5);
            vec![(Grip::Middle, ground(*x, *z)), (Grip::Turn, ground(tx, tz)), (Grip::Corner, ground(cx, cz))]
        }
        Some(LayerKind::Spline { points, .. }) => {
            points.iter().enumerate().map(|(i, p)| (Grip::Point(i), Vec3::new(p[0], p[1], p[2]))).collect()
        }
        _ => Vec::new(),
    }
}

impl TerrainTools {
    /// Whether a stroke or a drag is under way.
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some() || self.hold.is_some()
    }

    /// Whether a press in the Scene view is for the terrain: the brush over
    /// its ground, or a dot under the pointer.
    pub fn is_hovered(&self) -> bool {
        self.under.is_some() || self.hovered.is_some()
    }

    /// Notes the ground or the dot under the pointer, for the selected Obj
    /// `entity` when it has a terrain.
    pub fn hover(&mut self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, point: Option<[f32; 2]>) {
        if self.is_dragging() {
            return;
        }
        (self.under, self.hovered) = (None, None);
        let (Some(entity), Some(point)) = (entity, point) else { return };
        if !game.engine.world.has::<Terrain>(entity) {
            return;
        }
        match terrain_props::ui(&game.engine.world).tool {
            TerrainTool::Sculpt | TerrainTool::Paint => {
                self.under = ground_under(game, camera, entity, point).map(|at| (entity, at));
            }
            TerrainTool::Layers => {
                let Some(corner) = corner_of(game, entity) else { return };
                let near = |at: Vec3| camera.project(at + corner).map(|p| (p[0] - point[0]).hypot(p[1] - point[1]));
                let distances = grips(game, entity).into_iter().filter_map(|(grip, at)| Some((grip, near(at)?)));
                self.hovered =
                    distances.filter(|&(_, d)| d <= REACH).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(g, _)| g);
            }
            TerrainTool::Settings => {}
        }
    }

    /// Starts a stroke, or takes hold of a dot, under `point`. With
    /// `control` and Set Height, takes the height there instead. Returns
    /// whether it took the press.
    pub fn begin(
        &mut self,
        game: &mut Game,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
        (shift, control): (bool, bool),
    ) -> bool {
        self.hover(game, camera, Some(entity), Some(point));
        if let Some(grip) = self.hovered {
            let layer = terrain_props::ui(&game.engine.world).layer;
            let at = grips(game, entity).into_iter().find(|(g, _)| *g == grip).map_or(Vec3::ZERO, |(_, at)| at);
            let hit = on_level(game, camera, entity, point, at.y).unwrap_or([at.x, at.z]);
            if let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) {
                self.compositor.edit(&asset.file, Some(layer));
            }
            self.hold = Some(Hold { entity, layer, grip, level: at.y, offset: [at.x - hit[0], at.z - hit[1]] });
            return true;
        }
        let Some((_, at)) = self.under else { return false };
        let ui = terrain_props::ui(&game.engine.world);
        let brush = ui.brush;
        if control && brush.kind == BrushKind::SetHeight && ui.tool == TerrainTool::Sculpt {
            terrain_props::ui_mut(&mut game.engine.world).brush.height = at.y;
            return true;
        }
        let paints = match (ui.tool, brush.kind) {
            (TerrainTool::Paint, _) => {
                let Some(surface) = terrain_props::paint_surface(game, entity) else { return false };
                Paints::Surface(surface)
            }
            (_, BrushKind::Holes) => {
                if !terrain_props::ready_for_holes(game, entity) {
                    return false;
                }
                Paints::Holes { fill: shift }
            }
            _ => {
                let Some((layer, raster)) = terrain_props::sculpt_layer(game, entity) else { return false };
                let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return false };
                self.compositor.edit(&asset.file, Some(layer));
                Paints::Sculpt(raster)
            }
        };
        if matches!(paints, Paints::Holes { .. }) {
            // Holes are cut as each tile is made, from the base up.
            if let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) {
                self.compositor.edit(&asset.file, None);
            }
        }
        self.stroke = Some(Stroke { entity, paints, flatten_to: at.y, last: [at.x, at.z], at: Instant::now() });
        self.paint(game, [at.x, at.z], 1.0 / 60.0, shift);
        true
    }

    /// Carries the stroke or the drag on to `point`.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2], shift: bool) {
        if let Some(hold) = &self.hold {
            let (entity, layer, grip, offset) = (hold.entity, hold.layer, hold.grip, hold.offset);
            let Some(hit) = on_level(game, camera, entity, point, hold.level) else { return };
            self.drag_grip(game, (entity, layer, grip), [hit[0] + offset[0], hit[1] + offset[1]]);
            return;
        }
        let Some(stroke) = &mut self.stroke else { return };
        let now = Instant::now();
        let held = now.duration_since(stroke.at).as_secs_f32().min(LONGEST_FRAME);
        stroke.at = now;
        let entity = stroke.entity;
        let Some(at) = ground_under(game, camera, entity, point) else { return };
        self.under = Some((entity, at));
        self.paint(game, [at.x, at.z], held, shift);
    }

    /// Dabs from the stroke's last dab to `to`, `SPACING` of the brush's
    /// radius apart, sharing `held` seconds among them, and makes the ground
    /// they reach again.
    fn paint(&mut self, game: &mut Game, to: [f32; 2], held: f32, lower: bool) {
        let Some(stroke) = &mut self.stroke else { return };
        let brush = terrain_props::ui(&game.engine.world).brush;
        let Some(asset) = game.engine.world.get::<TerrainAsset>(stroke.entity) else { return };
        let file = asset.file.clone();
        let mut rasters = (*asset.rasters).clone();
        let gap = (brush.radius * SPACING).max(file.spacing());
        let distance = ((to[0] - stroke.last[0]).powi(2) + (to[1] - stroke.last[1]).powi(2)).sqrt();
        let steps = ((distance / gap).ceil() as usize).clamp(1, 64);
        let pictures = game.content.terrain_pictures(&file);
        let find = |reference: &str| pictures.get(reference).cloned();
        let mut areas: Vec<Area> = Vec::with_capacity(steps);
        let heights = game.engine.world.get::<Terrain>(stroke.entity).map(|terrain| terrain.heights.clone());
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let at = [stroke.last[0] + (to[0] - stroke.last[0]) * t, stroke.last[1] + (to[1] - stroke.last[1]) * t];
            let amount = held / steps as f32;
            let area = match stroke.paints {
                Paints::Sculpt(raster) => brush::dab(
                    &brush,
                    at,
                    amount,
                    lower,
                    stroke.flatten_to,
                    (&file, &self.compositor, &find),
                    (&mut rasters, raster),
                ),
                Paints::Holes { fill } => brush::cut(&file, &mut rasters, at, brush.radius, fill),
                Paints::Surface(surface) => {
                    let Some(heights) = &heights else { return };
                    voxear_terrain::surfaces::paint(&file, heights, &mut rasters, surface, &brush, at, amount, lower)
                }
            };
            areas.push(area);
        }
        stroke.last = to;
        let entity = stroke.entity;
        let dirty = Dirty::Areas(areas);
        match stroke.paints {
            // Painting changes only where the surfaces cover, not the ground.
            Paints::Surface(_) => terrain_props::repaint(game, entity, Arc::new(rasters), &dirty),
            Paints::Sculpt(_) | Paints::Holes { .. } => {
                terrain_props::remake_with(game, entity, file, Arc::new(rasters), &dirty, &self.compositor)
            }
        };
    }

    /// Moves the held dot of layer `layer` to `to` on the ground, in the
    /// terrain's space.
    fn drag_grip(&mut self, game: &mut Game, (entity, layer, grip): (Entity, usize, Grip), to: [f32; 2]) {
        let Some(asset) = game.engine.world.get::<TerrainAsset>(entity) else { return };
        let mut file = asset.file.clone();
        let rasters = asset.rasters.clone();
        let Some(before) = file.layers.get(layer).cloned() else { return };
        match (&mut file.layers[layer].kind, grip) {
            (LayerKind::Stamp { position, .. }, Grip::Middle) => *position = to,
            (LayerKind::Stamp { position, rotation, .. }, Grip::Turn) => {
                let (dx, dz) = (to[0] - position[0], to[1] - position[1]);
                if dx.abs() + dz.abs() > 1e-3 {
                    // The dot sits along the stamp's own +z, which a turn of `rotation` points along (sin, cos).
                    *rotation = dx.atan2(dz).to_degrees();
                }
            }
            (LayerKind::Stamp { position, rotation, size, .. }, Grip::Corner) => {
                let (sin, cos) = rotation.to_radians().sin_cos();
                let (dx, dz) = (to[0] - position[0], to[1] - position[1]);
                let (u, v) = (dx * cos - dz * sin, dx * sin + dz * cos);
                *size = [(u.abs() * 2.0).max(0.5), (v.abs() * 2.0).max(0.5)];
            }
            (LayerKind::Spline { points, .. }, Grip::Point(i)) if i < points.len() => {
                // A point stands on the ground below the spline, which it then lays level along it.
                let pictures = game.content.terrain_pictures(&asset.file);
                let find = |reference: &str| pictures.get(reference).cloned();
                let sources = voxear_terrain::composite::Sources { pictures: &find, rasters: &rasters };
                let spacing = asset.file.spacing();
                let below = self.compositor.below_at(
                    &asset.file,
                    &sources,
                    (to[0] / spacing).round() as i64,
                    (to[1] / spacing).round() as i64,
                );
                points[i] = [to[0], below, to[1]];
            }
            _ => return,
        }
        let dirty = terrain_props::dirty_between(Some(&before), file.layers.get(layer));
        terrain_props::remake_with(game, entity, file, rasters, &dirty, &self.compositor);
    }

    /// Ends the stroke or lets go of the dot, returning the terrain's Obj
    /// for the step to undo.
    pub fn end(&mut self) -> Option<Entity> {
        self.stroke.take().map(|stroke| stroke.entity).or_else(|| self.hold.take().map(|hold| hold.entity))
    }

    /// The brush's ring on the ground, or the picked layer's outline and
    /// dots, as shapes in the view's own coordinates.
    pub fn shapes(&self, game: &Game, camera: &ViewCamera, entity: Option<Entity>) -> Vec<Shape> {
        let Some(entity) = entity else { return Vec::new() };
        let Some(corner) = corner_of(game, entity) else { return Vec::new() };
        let Some(terrain) = game.engine.world.get::<Terrain>(entity) else { return Vec::new() };
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let screen = |at: Vec3| camera.project(at + corner).map(local);
        let on_ground = |x: f32, z: f32| Vec3::new(x, terrain.heights.height_at(x, z), z);
        let mut shapes = Vec::new();
        let outline = |points: Vec<Vec3>, color: Color, shapes: &mut Vec<Shape>| {
            for pair in points.windows(2) {
                if let (Some(from), Some(to)) = (screen(pair[0]), screen(pair[1])) {
                    shapes.push(Shape::Line { from, to, width: 1.5, color });
                }
            }
        };
        let ui = terrain_props::ui(&game.engine.world);
        if let Some((_, at)) = self.under.filter(|(e, _)| *e == entity) {
            let radius = ui.brush.radius;
            let ring: Vec<Vec3> = (0..=48)
                .map(|i| {
                    let angle = i as f32 / 48.0 * std::f32::consts::TAU;
                    on_ground(at.x + angle.cos() * radius, at.z + angle.sin() * radius)
                })
                .collect();
            outline(ring, Color::hex(COLOR), &mut shapes);
            // Where the brush is strongest: half its radius.
            let inner: Vec<Vec3> = (0..=32)
                .map(|i| {
                    let angle = i as f32 / 32.0 * std::f32::consts::TAU;
                    on_ground(at.x + angle.cos() * radius * 0.5, at.z + angle.sin() * radius * 0.5)
                })
                .collect();
            outline(inner, Color::hex(COLOR).with_alpha(110), &mut shapes);
        }
        let Some(asset) = game.engine.world.get::<TerrainAsset>(entity).filter(|_| ui.tool == TerrainTool::Layers)
        else {
            return shapes;
        };
        if let Some(LayerKind::Stamp { position: [x, z], rotation, size: [w, l], .. }) =
            asset.file.layers.get(ui.layer).map(|layer| &layer.kind)
        {
            let (sin, cos) = rotation.to_radians().sin_cos();
            let corner_at = |u: f32, v: f32| on_ground(x + u * cos + v * sin, z - u * sin + v * cos);
            let mut edge = Vec::new();
            for (a, b) in
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)].windows(2).map(|w| (w[0], w[1]))
            {
                for k in 0..=8 {
                    let t = k as f32 / 8.0;
                    let (u, v) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    edge.push(corner_at(u * w * 0.5, v * l * 0.5));
                }
            }
            outline(edge, Color::hex(COLOR).with_alpha(200), &mut shapes);
        }
        if let Some(LayerKind::Spline { points, .. }) = asset.file.layers.get(ui.layer).map(|layer| &layer.kind) {
            let line: Vec<Vec3> = points.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect();
            outline(line, Color::hex(COLOR).with_alpha(200), &mut shapes);
        }
        let held = self.hold.as_ref().map(|hold| hold.grip);
        for (grip, at) in grips(game, entity) {
            let Some(center) = screen(at) else { continue };
            let hot = self.hovered == Some(grip) || held == Some(grip);
            let fill = if hot { Color::hex(LIT) } else { Color::hex(COLOR) };
            shapes.push(Shape::Circle { center, radius: DOT, fill, border: Some((1.0, Color::rgba(12, 14, 16, 200))) });
        }
        shapes
    }
}
