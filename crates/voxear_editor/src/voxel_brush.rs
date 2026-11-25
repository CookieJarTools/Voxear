//! An endless Voxel Volume's brush in the Scene view.
//!
//! With the volume selected and its Inspector's **Brush** set to Carve or
//! Fill, a ball under the pointer shows where the ray from it meets the
//! volume's ground (`voxear_engine::voxel_volumes::raycast`), as big as
//! **Brush Size**. Holding the left button carves balls out of the ground,
//! or fills them with **Brush Ground**, a ball each time the pointer has
//! gone a third of the size along; Shift does the other. A stroke is one
//! step to undo, and the scene saves what it changed, as scripts' and
//! `vox editor call voxels.carve`'s changes are kept.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_engine::Game;
use voxear_render::{VoxelMaterial, VoxelVolume};
use voxear_ui::{Color, Shape};

use crate::{props::Value, scene_tools::ViewCamera};

const CARVE_COLOR: u32 = 0xe8784c;
const FILL_COLOR: u32 = 0x5ad1e6;
/// How far a ray from the pointer looks for the ground, in metres.
const REACH: f32 = 2000.0;
/// How far apart a stroke's balls are, as a share of the brush's size.
const SPACING: f32 = 0.33;

/// What the brush does, as its Inspector row picks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoxelBrushMode {
    #[default]
    Off,
    Carve,
    Fill,
}

impl VoxelBrushMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Carve, Self::Fill];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Carve => "Carve",
            Self::Fill => "Fill",
        }
    }
}

/// The brush's settings: the editor's own, not the scene's, so they are
/// kept for every volume and never saved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelBrushUi {
    pub mode: VoxelBrushMode,
    /// The balls' radius, in metres.
    pub size: f32,
    pub ground: VoxelMaterial,
}

impl Default for VoxelBrushUi {
    fn default() -> Self {
        Self { mode: VoxelBrushMode::Off, size: 1.0, ground: VoxelMaterial::Dirt }
    }
}

/// The brush's settings.
pub fn ui(world: &World) -> VoxelBrushUi {
    world.resource::<VoxelBrushUi>().copied().unwrap_or_default()
}

fn ui_mut(world: &mut World) -> &mut VoxelBrushUi {
    if world.resource::<VoxelBrushUi>().is_none() {
        world.insert_resource(VoxelBrushUi::default());
    }
    world.resource_mut::<VoxelBrushUi>().expect("inserted above")
}

/// A row of the brush in a Voxel Volume's Inspector section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoxelBrushField {
    Mode,
    Size,
    Ground,
}

/// The rows an endless volume's section shows for its brush: the size and
/// ground with the brush on, the ground for filling.
pub fn fields(world: &World) -> Vec<VoxelBrushField> {
    match ui(world).mode {
        VoxelBrushMode::Off => vec![VoxelBrushField::Mode],
        _ => vec![VoxelBrushField::Mode, VoxelBrushField::Size, VoxelBrushField::Ground],
    }
}

pub fn label(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "Brush",
        VoxelBrushField::Size => "Brush Size",
        VoxelBrushField::Ground => "Brush Ground",
    }
}

/// As `vox editor call field.set` names it.
pub fn key(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "brush",
        VoxelBrushField::Size => "brush_size",
        VoxelBrushField::Ground => "brush_ground",
    }
}

pub fn tooltip(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => {
            "Carve balls out of the ground or fill them, dragging in the Scene view; Shift does the other"
        }
        VoxelBrushField::Size => "The balls' radius, in metres",
        VoxelBrushField::Ground => "What a ball filled is",
    }
}

fn choice<T: PartialEq + Copy>(all: &[T], one: T, name: fn(T) -> &'static str) -> Value {
    let at = all.iter().position(|other| *other == one).unwrap_or_default();
    Value::Choice(at, all.iter().map(|each| name(*each).to_string()).collect())
}

pub fn get(world: &World, field: VoxelBrushField) -> Value {
    let ui = ui(world);
    match field {
        VoxelBrushField::Mode => choice(&VoxelBrushMode::ALL, ui.mode, VoxelBrushMode::name),
        VoxelBrushField::Size => Value::Number(ui.size),
        VoxelBrushField::Ground => choice(&VoxelMaterial::ALL, ui.ground, VoxelMaterial::name),
    }
}

pub fn set(world: &mut World, field: VoxelBrushField, value: Value) -> bool {
    let ui = ui_mut(world);
    match (field, value) {
        (VoxelBrushField::Mode, Value::Choice(at, _)) => {
            ui.mode = VoxelBrushMode::ALL.get(at).copied().unwrap_or_default()
        }
        (VoxelBrushField::Size, Value::Number(size)) => ui.size = size.clamp(0.1, 20.0),
        (VoxelBrushField::Ground, Value::Choice(at, _)) => {
            ui.ground = VoxelMaterial::ALL.get(at).copied().unwrap_or_default()
        }
        _ => return false,
    }
    true
}

/// A stroke under way: the volume, whether it fills, and where its last
/// ball was.
struct Stroke {
    entity: Entity,
    fill: bool,
    last: Vec3,
}

/// The Scene view's voxel brush.
#[derive(Default)]
pub struct VoxelTools {
    /// The volume's ground under the pointer, while the brush shows.
    under: Option<(Entity, Vec3)>,
    stroke: Option<Stroke>,
}

/// Where a ray through `point` meets `entity`'s endless volume's ground.
fn ground_under(game: &Game, camera: &ViewCamera, entity: Entity, point: [f32; 2]) -> Option<Vec3> {
    let world = &game.engine.world;
    if !world.get::<VoxelVolume>(entity).is_some_and(|volume| volume.endless) {
        return None;
    }
    let (origin, direction) = camera.ray(point);
    voxear_engine::voxel_volumes::raycast(world, origin, direction, REACH, Some(entity)).map(|(_, hit)| hit.point)
}

impl VoxelTools {
    pub fn is_dragging(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn is_hovered(&self) -> bool {
        self.under.is_some()
    }

    /// Notes the ground under the pointer, for the selected Obj `entity`
    /// when it has an endless volume and the brush is on.
    pub fn hover(&mut self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, point: Option<[f32; 2]>) {
        if self.is_dragging() {
            return;
        }
        self.under = None;
        let (Some(entity), Some(point)) = (entity, point) else { return };
        if ui(&game.engine.world).mode == VoxelBrushMode::Off {
            return;
        }
        self.under = ground_under(game, camera, entity, point).map(|at| (entity, at));
    }

    /// Starts a stroke under `point`: a ball carved or filled there, Shift
    /// doing the other. Returns whether it took the press.
    pub fn begin(
        &mut self,
        game: &mut Game,
        camera: &ViewCamera,
        entity: Entity,
        point: [f32; 2],
        shift: bool,
    ) -> bool {
        self.hover(game, camera, Some(entity), Some(point));
        let Some((_, at)) = self.under else { return false };
        let fill = (ui(&game.engine.world).mode == VoxelBrushMode::Fill) != shift;
        self.stroke = Some(Stroke { entity, fill, last: at });
        self.dab(game, at);
        true
    }

    /// Carries the stroke on to the ground under `point`: a ball each time
    /// it has gone a third of the size.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2]) {
        let Some(stroke) = &self.stroke else { return };
        let (entity, last) = (stroke.entity, stroke.last);
        let Some(at) = ground_under(game, camera, entity, point) else { return };
        self.under = Some((entity, at));
        let spacing = (ui(&game.engine.world).size * SPACING).max(0.05);
        let gone = at.distance(last);
        let steps = (gone / spacing).floor() as usize;
        for step in 1..=steps.min(64) {
            self.dab(game, last.lerp(at, step as f32 * spacing / gone));
        }
    }

    /// A ball at `at` for the stroke.
    fn dab(&mut self, game: &mut Game, at: Vec3) {
        let Some(stroke) = &mut self.stroke else { return };
        let ui = ui(&game.engine.world);
        let fill = stroke.fill.then_some(ui.ground);
        voxear_engine::voxel_volumes::edit(&mut game.engine.world, at, ui.size, fill, Some(stroke.entity));
        stroke.last = at;
    }

    /// Ends the stroke, giving the volume it changed, for one step to undo.
    pub fn end(&mut self) -> Option<Entity> {
        self.stroke.take().map(|stroke| stroke.entity)
    }

    /// The ball under the pointer: three rings round it, as shapes in the
    /// view's own coordinates.
    pub fn shapes(&self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, shift: bool) -> Vec<Shape> {
        let Some((under, at)) = self.under else { return Vec::new() };
        if entity != Some(under) {
            return Vec::new();
        }
        let ui = ui(&game.engine.world);
        let fill = self.stroke.as_ref().map_or((ui.mode == VoxelBrushMode::Fill) != shift, |stroke| stroke.fill);
        let color = Color::hex(if fill { FILL_COLOR } else { CARVE_COLOR });
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let mut shapes = Vec::new();
        let rings: [fn(f32) -> Vec3; 3] = [
            |a| Vec3::new(a.cos(), 0.0, a.sin()),
            |a| Vec3::new(a.cos(), a.sin(), 0.0),
            |a| Vec3::new(0.0, a.cos(), a.sin()),
        ];
        for ring in rings {
            let points: Vec<Option<[f32; 2]>> = (0..=40)
                .map(|i| camera.project(at + ring(i as f32 / 40.0 * std::f32::consts::TAU) * ui.size).map(local))
                .collect();
            for pair in points.windows(2) {
                if let (Some(from), Some(to)) = (pair[0], pair[1]) {
                    shapes.push(Shape::Line { from, to, width: 1.5, color });
                }
            }
        }
        if let Some(middle) = camera.project(at).map(local) {
            shapes.push(Shape::Circle { center: middle, radius: 3.0, fill: color, border: None });
        }
        shapes
    }
}
