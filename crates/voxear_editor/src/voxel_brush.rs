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
//!
//! Set to Print, the box of the **Brush Print** picked shows where the
//! ground under the pointer is, turned **Brush Turn** degrees, and a click
//! prints it there, its model's origin on the ground.

use voxear_assets::AssetGuid;
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::Game;
use voxear_render::{VoxelMaterial, VoxelVolume};
use voxear_ui::{Color, Shape};

use crate::{props::Value, scene_tools::ViewCamera};

const CARVE_COLOR: u32 = 0xe8784c;
const FILL_COLOR: u32 = 0x5ad1e6;
const PRINT_COLOR: u32 = 0xf2c94c;
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
    Print,
}

impl VoxelBrushMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::Carve, Self::Fill, Self::Print];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Carve => "Carve",
            Self::Fill => "Fill",
            Self::Print => "Print",
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
    /// The Voxel Print a click prints, and how far it is turned about the
    /// up axis, in degrees.
    pub print: Option<AssetGuid>,
    pub turn: f32,
}

impl Default for VoxelBrushUi {
    fn default() -> Self {
        Self { mode: VoxelBrushMode::Off, size: 1.0, ground: VoxelMaterial::Dirt, print: None, turn: 0.0 }
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
    Print,
    Turn,
}

/// The rows an endless volume's section shows for its brush: the size and
/// ground with the brush carving or filling, the print and its turn with
/// it printing.
pub fn fields(world: &World) -> Vec<VoxelBrushField> {
    match ui(world).mode {
        VoxelBrushMode::Off => vec![VoxelBrushField::Mode],
        VoxelBrushMode::Print => vec![VoxelBrushField::Mode, VoxelBrushField::Print, VoxelBrushField::Turn],
        _ => vec![VoxelBrushField::Mode, VoxelBrushField::Size, VoxelBrushField::Ground],
    }
}

pub fn label(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "Brush",
        VoxelBrushField::Size => "Brush Size",
        VoxelBrushField::Ground => "Brush Ground",
        VoxelBrushField::Print => "Brush Print",
        VoxelBrushField::Turn => "Brush Turn",
    }
}

/// As `vox editor call field.set` names it.
pub fn key(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "brush",
        VoxelBrushField::Size => "brush_size",
        VoxelBrushField::Ground => "brush_ground",
        VoxelBrushField::Print => "brush_print",
        VoxelBrushField::Turn => "brush_turn",
    }
}

pub fn tooltip(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => {
            "Carve balls out of the ground or fill them, dragging in the Scene view, Shift doing the other; or print a Voxel Print where you click"
        }
        VoxelBrushField::Size => "The balls' radius, in metres",
        VoxelBrushField::Ground => "What a ball filled is",
        VoxelBrushField::Print => "The Voxel Print a click prints",
        VoxelBrushField::Turn => "How far the print is turned about the up axis, in degrees",
    }
}

/// Whether a row picks an asset: the print.
pub fn is_picked(field: VoxelBrushField) -> bool {
    field == VoxelBrushField::Print
}

/// The Voxel Prints a row can pick, with none first.
pub fn choices(game: &Game) -> Vec<(String, crate::fields::Pick)> {
    crate::fields::assets_of(game, Some(voxear_assets::AssetKind::VoxelPrint))
}

/// Picks the brush's print, read now so its box shows under the pointer.
pub fn pick(game: &mut Game, choice: &crate::fields::Pick) -> bool {
    let crate::fields::Pick::Asset(path) = choice else { return false };
    let print = match path {
        Some(path) => match game.content.voxel_print(path) {
            Some((guid, _)) => Some(guid),
            None => return false,
        },
        None => None,
    };
    ui_mut(&mut game.engine.world).print = print;
    true
}

fn choice<T: PartialEq + Copy>(all: &[T], one: T, name: fn(T) -> &'static str) -> Value {
    let at = all.iter().position(|other| *other == one).unwrap_or_default();
    Value::Choice(at, all.iter().map(|each| name(*each).to_string()).collect())
}

pub fn get(game: &Game, field: VoxelBrushField) -> Value {
    let ui = ui(&game.engine.world);
    match field {
        VoxelBrushField::Mode => choice(&VoxelBrushMode::ALL, ui.mode, VoxelBrushMode::name),
        VoxelBrushField::Size => Value::Number(ui.size),
        VoxelBrushField::Ground => choice(&VoxelMaterial::ALL, ui.ground, VoxelMaterial::name),
        VoxelBrushField::Print => {
            let path = ui.print.and_then(|guid| game.content.db().get(guid)).map(|entry| entry.path.clone());
            Value::Asset(path.unwrap_or_else(|| "None".into()))
        }
        VoxelBrushField::Turn => Value::Number(ui.turn),
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
        (VoxelBrushField::Turn, Value::Number(turn)) => ui.turn = turn.rem_euclid(360.0),
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
    /// Whether it printed a print, which dragging does not repeat.
    printed: bool,
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
    /// doing the other; or the brush's print printed there. Returns whether
    /// it took the press.
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
        let ui = ui(&game.engine.world);
        if ui.mode == VoxelBrushMode::Print {
            let Some(print) = ui.print else { return false };
            let (content, world) = (&mut game.content, &mut game.engine.world);
            let reference = print.to_string();
            voxear_engine::voxel_volumes::print(content, world, &reference, at, [0.0, ui.turn, 0.0], Some(entity));
            // A stroke of its own, for one step to undo, which dragging carries no further.
            self.stroke = Some(Stroke { entity, fill: false, last: at, printed: true });
            return true;
        }
        let fill = (ui.mode == VoxelBrushMode::Fill) != shift;
        self.stroke = Some(Stroke { entity, fill, last: at, printed: false });
        self.dab(game, at);
        true
    }

    /// Carries the stroke on to the ground under `point`: a ball each time
    /// it has gone a third of the size.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2]) {
        let Some(stroke) = self.stroke.as_ref().filter(|stroke| !stroke.printed) else { return };
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
        if ui.mode == VoxelBrushMode::Print {
            return print_box(game, camera, &ui, at);
        }
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

/// The box of the brush's print placed at `at`, turned as the brush turns
/// it: its twelve edges, and a dot at its origin, as shapes in the view's
/// own coordinates.
fn print_box(game: &Game, camera: &ViewCamera, ui: &VoxelBrushUi, at: Vec3) -> Vec<Shape> {
    let color = Color::hex(PRINT_COLOR);
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    let mut shapes = Vec::new();
    if let Some(print) = ui.print.and_then(|guid| game.content.loaded_voxel_print(guid)) {
        let (low, high) = print.bounds();
        let turn = Quat::from_rotation_y(ui.turn.to_radians());
        let corner = |i: usize| {
            let c = Vec3::new([low.x, high.x][i & 1], [low.y, high.y][(i >> 1) & 1], [low.z, high.z][i >> 2]);
            camera.project(at + turn * c).map(local)
        };
        for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            if let (Some(from), Some(to)) = (corner(a), corner(b)) {
                shapes.push(Shape::Line { from, to, width: 1.5, color });
            }
        }
    }
    if let Some(middle) = camera.project(at).map(local) {
        shapes.push(Shape::Circle { center: middle, radius: 3.0, fill: color, border: None });
    }
    shapes
}
