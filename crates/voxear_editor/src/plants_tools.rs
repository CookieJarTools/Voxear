//! A Foliage's brush in the Scene view (its **Paint** tool).
//!
//! A ring on the ground under the pointer shows the brush; holding the
//! left button paints the Foliage's plants there, a dab each frame along
//! where the pointer went, until as many stand in the brush as its
//! density says. Each plant is dropped onto whatever is below it, a
//! terrain or any mesh but other plants, leans with the ground there as
//! far as it is told, and is left out where the ground is too steep or
//! another plant stands nearer than its spacing. Shift takes plants away
//! instead. Each stroke is one step to undo.
//!
//! A Foliage's plants painted are placed one by one: painting turns its
//! scattered plants into placed ones first, to paint among them.

use std::time::Instant;

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{Game, scene_file::PaintData};
use voxear_render::{Foliage, Renderer};
use voxear_scene::{Plant, Plants};
use voxear_ui::{Color, Shape};

use crate::{
    plants_props::{self, FoliageTool},
    scene_tools::{self, ViewCamera},
};

const COLOR: u32 = 0x8fd16a;
/// How far apart a stroke's dabs are, as a share of the brush's radius.
const SPACING: f32 = 0.25;
/// How long one frame's painting may count for, so a stall does not plant a forest.
const LONGEST_FRAME: f32 = 0.1;
/// How many of the plants a full brush wants it plants in a second.
const RATE: f32 = 4.0;

/// A stroke under way: the Foliage, whether it takes plants away, where the
/// last dab was and when, and the numbers it draws from.
struct Stroke {
    entity: Entity,
    erase: bool,
    last: [f32; 2],
    at: Instant,
    seed: u32,
}

/// The Scene view's Foliage brush.
#[derive(Default)]
pub struct PlantsTools {
    /// The ground under the pointer while the brush shows, in the world.
    under: Option<(Entity, Vec3)>,
    stroke: Option<Stroke>,
    strokes: u32,
}

/// Whether the Foliage brush is out: the Obj has a Foliage and Paint shows.
fn painting(game: &Game, entity: Entity) -> bool {
    let world = &game.engine.world;
    world.has::<Foliage>(entity) && world.has::<Plants>(entity) && plants_props::ui(world).tool == FoliageTool::Paint
}

/// Where the Obj is in the world.
fn model_of(game: &Game, entity: Entity) -> Mat4 {
    voxear_scene::world_matrix(&game.engine.world, entity)
}

impl PlantsTools {
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.under.is_some()
    }

    /// Notes the ground under the pointer, for the selected Obj `entity`
    /// when its Foliage's Paint tool shows.
    pub fn hover(
        &mut self,
        game: &Game,
        renderer: &Renderer,
        camera: &ViewCamera,
        entity: Option<Entity>,
        point: Option<[f32; 2]>,
    ) {
        if self.is_dragging() {
            return;
        }
        self.under = None;
        let (Some(entity), Some(point)) = (entity, point) else { return };
        if !painting(game, entity) {
            return;
        }
        // Plants are left out of what the brush finds, so its own Obj's ground, a terrain's, counts.
        let at = scene_tools::surface_at(&game.engine.world, renderer, camera, point);
        self.under = at.map(|at| (entity, at));
    }

    /// Starts a stroke under the pointer; whether it took the press.
    pub fn begin(&mut self, game: &mut Game, renderer: &Renderer, entity: Entity, shift: bool) -> bool {
        let Some((_, at)) = self.under.filter(|(e, _)| *e == entity) else { return false };
        self.strokes = self.strokes.wrapping_add(1);
        let seed = self.strokes.wrapping_mul(0x9E37_79B9);
        self.stroke = Some(Stroke { entity, erase: shift, last: [at.x, at.z], at: Instant::now(), seed });
        self.paint(game, renderer, at, 1.0 / 60.0);
        true
    }

    /// Carries the stroke on to the ground under `point`.
    pub fn drag_to(&mut self, game: &mut Game, renderer: &Renderer, camera: &ViewCamera, point: [f32; 2]) {
        let Some(stroke) = &mut self.stroke else { return };
        let now = Instant::now();
        let held = now.duration_since(stroke.at).as_secs_f32().min(LONGEST_FRAME);
        stroke.at = now;
        let entity = stroke.entity;
        let Some(at) = scene_tools::surface_at(&game.engine.world, renderer, camera, point) else {
            return;
        };
        self.under = Some((entity, at));
        self.paint(game, renderer, at, held);
    }

    /// Dabs from the stroke's last dab to `to`, sharing `held` seconds
    /// among them.
    fn paint(&mut self, game: &mut Game, renderer: &Renderer, to: Vec3, held: f32) {
        let Some(stroke) = &mut self.stroke else { return };
        let world = &game.engine.world;
        let brush = crate::terrain_props::ui(world).brush;
        let grow = plants_props::paint(world, stroke.entity);
        let Some(plants) = world.get::<Plants>(stroke.entity) else { return };
        let mut list: Vec<Plant> = plants.standing().map(|(_, plant)| *plant).collect();
        let model = model_of(game, stroke.entity);
        let gap = (brush.radius * SPACING).max(0.25);
        let distance = ((to.x - stroke.last[0]).powi(2) + (to.z - stroke.last[1]).powi(2)).sqrt();
        let steps = ((distance / gap).ceil() as usize).clamp(1, 32);
        let mut changed = false;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let at = [stroke.last[0] + (to.x - stroke.last[0]) * t, stroke.last[1] + (to.z - stroke.last[1]) * t];
            let amount = held / steps as f32;
            stroke.seed = stroke.seed.wrapping_add(0x6D2B_79F5);
            let dab = Dab { at, top: to.y + brush.radius + 30.0, radius: brush.radius, strength: brush.strength };
            changed |= match stroke.erase {
                true => erase(&mut list, model, &dab),
                false => grow_in(game, renderer, &mut list, (model, grow), &dab, (amount, stroke.seed)),
            };
        }
        stroke.last = [to.x, to.z];
        if changed && let Err(error) = plants_props::replace_plants(&mut game.engine.world, stroke.entity, list) {
            tracing::error!("{error:#}");
        }
    }

    /// Ends the stroke, returning the Foliage's Obj for the step to undo.
    pub fn end(&mut self) -> Option<Entity> {
        self.stroke.take().map(|stroke| stroke.entity)
    }

    /// The brush's ring on the ground, in the view's own coordinates.
    pub fn shapes(&self, game: &Game, renderer: &Renderer, camera: &ViewCamera, entity: Option<Entity>) -> Vec<Shape> {
        let Some((_, at)) = self.under.filter(|(e, _)| Some(*e) == entity) else { return Vec::new() };
        let radius = crate::terrain_props::ui(&game.engine.world).brush.radius;
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let ground = |x: f32, z: f32| {
            let top = at.y + radius + 30.0;
            let found = scene_tools::ground_below(&game.engine.world, renderer, (x, z, top), &[]);
            found.map_or(Vec3::new(x, at.y, z), |(ground, _)| ground)
        };
        let ring: Vec<Vec3> = (0..=32)
            .map(|i| {
                let angle = i as f32 / 32.0 * std::f32::consts::TAU;
                ground(at.x + angle.cos() * radius, at.z + angle.sin() * radius)
            })
            .collect();
        let mut shapes = Vec::new();
        for pair in ring.windows(2) {
            if let (Some(from), Some(to)) = (camera.project(pair[0]), camera.project(pair[1])) {
                shapes.push(Shape::Line { from: local(from), to: local(to), width: 1.5, color: Color::hex(COLOR) });
            }
        }
        shapes
    }
}

/// One dab: where, in the world's x and z, how high to look down from for
/// the ground, how wide, and how strong.
struct Dab {
    at: [f32; 2],
    top: f32,
    radius: f32,
    strength: f32,
}

/// A plant's place in the world's x and z.
fn spot(plant: &Plant, model: Mat4) -> [f32; 2] {
    let at = model.transform_point3(Vec3::from(plant.position));
    [at.x, at.z]
}

fn within(a: [f32; 2], b: [f32; 2], distance: f32) -> bool {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) <= distance * distance
}

/// Takes away the plants in the dab; whether any went.
fn erase(list: &mut Vec<Plant>, model: Mat4, dab: &Dab) -> bool {
    let before = list.len();
    list.retain(|plant| !within(spot(plant, model), dab.at, dab.radius));
    list.len() != before
}

/// Plants more in the dab, towards as many as the density says, each on
/// the ground below it; whether any were planted.
fn grow_in(
    game: &Game,
    renderer: &Renderer,
    list: &mut Vec<Plant>,
    (model, grow): (Mat4, PaintData),
    dab: &Dab,
    (amount, seed): (f32, u32),
) -> bool {
    let area = std::f32::consts::PI * dab.radius * dab.radius;
    let wanted = grow.density / 100.0 * area;
    let near: Vec<[f32; 2]> = list
        .iter()
        .map(|plant| spot(plant, model))
        .filter(|at| within(*at, dab.at, dab.radius + grow.spacing))
        .collect();
    let standing = near.iter().filter(|at| within(**at, dab.at, dab.radius)).count() as f32;
    // A share of what is missing each dab, so painting slowly fills the brush; one try at the least.
    let tries = ((wanted - standing).max(0.0) * dab.strength.clamp(0.0, 1.0) * amount * RATE).ceil() as usize;
    if tries == 0 {
        return false;
    }
    let mut next = voxear_scene::plants::random(seed);
    let inverse = model.inverse();
    let obj_scale = model.to_scale_rotation_translation().0.max_element().max(1e-4);
    let mut planted: Vec<[f32; 2]> = Vec::new();
    for _ in 0..tries.min(256) {
        // A point in the disc, evenly over it.
        let (r, angle) = (dab.radius * next().sqrt(), next() * std::f32::consts::TAU);
        let at = [dab.at[0] + angle.cos() * r, dab.at[1] + angle.sin() * r];
        if near.iter().chain(&planted).any(|other| within(*other, at, grow.spacing)) {
            continue;
        }
        let found = scene_tools::ground_below(&game.engine.world, renderer, (at[0], at[1], dab.top), &[]);
        let Some((ground, normal)) = found else { continue };
        if normal.angle_between(Vec3::Y).to_degrees() > grow.steepest {
            continue;
        }
        // Its up between straight up and square to the ground, as far as it is told to lean, in the Obj's space.
        let up = inverse.transform_vector3(Vec3::Y.lerp(normal, grow.lean.clamp(0.0, 1.0))).normalize_or(Vec3::Y);
        let position = inverse.transform_point3(ground).to_array();
        let size = (grow.scale[0] + (grow.scale[1] - grow.scale[0]) * next()) / obj_scale;
        let mut plant = Plant::new(position, next() * std::f32::consts::TAU, size);
        plant.lean = [up.x, up.z];
        list.push(plant);
        planted.push(at);
    }
    !planted.is_empty()
}
