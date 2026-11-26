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
//! Set to Box, a drag across the ground marks out a box's foot, turned
//! **Brush Turn** degrees, its corners on a grid of **Brush Snap** metres:
//! let go and it is filled **Brush Height** tall on the ground where the
//! drag began, clearing **Brush Clear** over it; with Shift it is dug as
//! deep. A box half a metre tall is a slab.
//!
//! Set to Area or Cover, each click sets a point of an outline across the
//! ground, and a click on its first point closes it: an area is laid flat
//! at its first point's height, **Brush Height** thick, clearing **Brush
//! Clear** over it, or dug out with Shift; a cover lays the ground's own top
//! within it, wherever it is, **Brush Height** deep.
//!
//! Set to Print, the box of the **Brush Print** picked shows where the
//! ground under the pointer is, turned **Brush Turn** degrees, and a click
//! prints it there, its model's origin on the ground, fitted to the ground
//! on a footing of **Brush Footing** with **Brush Fit**.
//!
//! Every fill but a print's is in **Brush Tint**, white for its own colours.

use voxear_assets::AssetGuid;
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    voxel_volumes::{VoxelEditData, VoxelEditShape},
};
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
/// How near an outline's first point a click closes it, in pixels.
const CLOSING: f32 = 10.0;

/// What the brush does, as its Inspector row picks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoxelBrushMode {
    #[default]
    Off,
    Carve,
    Fill,
    Box,
    Area,
    Cover,
    Print,
    Dig,
    Build,
}

impl VoxelBrushMode {
    pub const ALL: [Self; 9] =
        [Self::Off, Self::Carve, Self::Fill, Self::Box, Self::Area, Self::Cover, Self::Print, Self::Dig, Self::Build];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Carve => "Carve",
            Self::Fill => "Fill",
            Self::Box => "Box",
            Self::Area => "Area",
            Self::Cover => "Cover",
            Self::Print => "Print",
            Self::Dig => "Dig",
            Self::Build => "Build",
        }
    }

    /// Whether it sets an outline's points, click by click.
    fn outlines(self) -> bool {
        matches!(self, Self::Area | Self::Cover)
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
    /// How tall a box is, how thick an area, how deep a cover, in metres;
    /// what each clears over its top; and the grid a box's and an area's
    /// points keep to, none for 0.
    pub height: f32,
    pub clear: f32,
    pub snap: f32,
    /// The colour fills are shaded from, as sRGB from 0 to 1: white for
    /// their own.
    pub tint: [f32; 3],
    /// The Voxel Print a click prints, how far it and a box are turned
    /// about the up axis in degrees, and whether a print is fitted to the
    /// ground and on what footing.
    pub print: Option<AssetGuid>,
    pub turn: f32,
    pub fit: bool,
    pub footing: VoxelMaterial,
}

impl Default for VoxelBrushUi {
    fn default() -> Self {
        Self {
            mode: VoxelBrushMode::Off,
            size: 1.0,
            ground: VoxelMaterial::Dirt,
            height: 0.5,
            clear: 0.0,
            snap: 0.0,
            tint: [1.0; 3],
            print: None,
            turn: 0.0,
            fit: false,
            footing: VoxelMaterial::Ashlar,
        }
    }
}

impl VoxelBrushUi {
    /// The tint fills take: none for white.
    fn tint(&self) -> Option<[u8; 3]> {
        let tint = self.tint.map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8);
        (tint != [255; 3]).then_some(tint)
    }

    /// `at` on the brush's grid, across the ground.
    fn snapped(&self, at: Vec3) -> Vec3 {
        if self.snap <= 0.0 {
            return at;
        }
        let snap = |v: f32| (v / self.snap).round() * self.snap;
        Vec3::new(snap(at.x), at.y, snap(at.z))
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
    Height,
    Clear,
    Snap,
    Tint,
    Print,
    Turn,
    Fit,
    Footing,
}

/// The rows an endless volume's section shows for its brush, those its mode
/// uses.
pub fn fields(world: &World) -> Vec<VoxelBrushField> {
    use VoxelBrushField as F;
    let ui = ui(world);
    match ui.mode {
        VoxelBrushMode::Off => vec![F::Mode],
        VoxelBrushMode::Print if ui.fit => vec![F::Mode, F::Print, F::Turn, F::Fit, F::Footing],
        VoxelBrushMode::Print => vec![F::Mode, F::Print, F::Turn, F::Fit],
        VoxelBrushMode::Box => vec![F::Mode, F::Ground, F::Height, F::Clear, F::Turn, F::Snap, F::Tint],
        VoxelBrushMode::Area => vec![F::Mode, F::Ground, F::Height, F::Clear, F::Snap, F::Tint],
        VoxelBrushMode::Cover => vec![F::Mode, F::Ground, F::Height, F::Clear, F::Tint],
        VoxelBrushMode::Carve | VoxelBrushMode::Fill => vec![F::Mode, F::Size, F::Ground, F::Tint],
        VoxelBrushMode::Dig => vec![F::Mode],
        VoxelBrushMode::Build => vec![F::Mode, F::Ground, F::Tint],
    }
}

pub fn label(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "Brush",
        VoxelBrushField::Size => "Brush Size",
        VoxelBrushField::Ground => "Brush Ground",
        VoxelBrushField::Height => "Brush Height",
        VoxelBrushField::Clear => "Brush Clear",
        VoxelBrushField::Snap => "Brush Snap",
        VoxelBrushField::Tint => "Brush Tint",
        VoxelBrushField::Print => "Brush Print",
        VoxelBrushField::Turn => "Brush Turn",
        VoxelBrushField::Fit => "Brush Fit",
        VoxelBrushField::Footing => "Brush Footing",
    }
}

/// As `vox editor call field.set` names it.
pub fn key(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => "brush",
        VoxelBrushField::Size => "brush_size",
        VoxelBrushField::Ground => "brush_ground",
        VoxelBrushField::Height => "brush_height",
        VoxelBrushField::Clear => "brush_clear",
        VoxelBrushField::Snap => "brush_snap",
        VoxelBrushField::Tint => "brush_tint",
        VoxelBrushField::Print => "brush_print",
        VoxelBrushField::Turn => "brush_turn",
        VoxelBrushField::Fit => "brush_fit",
        VoxelBrushField::Footing => "brush_footing",
    }
}

pub fn tooltip(field: VoxelBrushField) -> &'static str {
    match field {
        VoxelBrushField::Mode => {
            "Carve or fill balls, dragging; mark out a box, dragging; click an area's or a cover's outline, closing it on its first point; click to print a Voxel Print; or dig and build a voxel at a time. Shift digs in place of filling"
        }
        VoxelBrushField::Size => "The balls' radius, in metres",
        VoxelBrushField::Ground => "The ground or paving it fills with",
        VoxelBrushField::Height => "How tall a box is, how thick an area, how deep a cover, in metres",
        VoxelBrushField::Clear => "How far over its top it clears the ground, in metres",
        VoxelBrushField::Snap => "The grid a box's corners and an area's points keep to, in metres: 0 for none",
        VoxelBrushField::Tint => "The colour its ground or paving is shaded from: white for its own",
        VoxelBrushField::Print => "The Voxel Print a click prints",
        VoxelBrushField::Turn => "How far a print or a box is turned about the up axis, in degrees",
        VoxelBrushField::Fit => "Fit the print to the ground: clear the ground inside it, and stand it on a footing",
        VoxelBrushField::Footing => "What a fitted print's footing is made of",
    }
}

/// A unit a row's number is in.
pub fn unit(field: VoxelBrushField) -> Option<&'static str> {
    match field {
        VoxelBrushField::Size | VoxelBrushField::Height | VoxelBrushField::Clear | VoxelBrushField::Snap => Some("m"),
        VoxelBrushField::Turn => Some("°"),
        _ => None,
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
        VoxelBrushField::Height => Value::Number(ui.height),
        VoxelBrushField::Clear => Value::Number(ui.clear),
        VoxelBrushField::Snap => Value::Number(ui.snap),
        VoxelBrushField::Tint => Value::Color(ui.tint),
        VoxelBrushField::Print => {
            let path = ui.print.and_then(|guid| game.content.db().get(guid)).map(|entry| entry.path.clone());
            Value::Asset(path.unwrap_or_else(|| "None".into()))
        }
        VoxelBrushField::Turn => Value::Number(ui.turn),
        VoxelBrushField::Fit => Value::Toggle(ui.fit),
        VoxelBrushField::Footing => choice(&VoxelMaterial::ALL, ui.footing, VoxelMaterial::name),
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
        (VoxelBrushField::Height, Value::Number(height)) => ui.height = height.clamp(1.0 / 64.0, 64.0),
        (VoxelBrushField::Clear, Value::Number(clear)) => ui.clear = clear.clamp(0.0, 64.0),
        (VoxelBrushField::Snap, Value::Number(snap)) => ui.snap = snap.clamp(0.0, 16.0),
        (VoxelBrushField::Tint, Value::Color(tint)) => ui.tint = tint.map(|part| part.clamp(0.0, 1.0)),
        (VoxelBrushField::Turn, Value::Number(turn)) => ui.turn = turn.rem_euclid(360.0),
        (VoxelBrushField::Fit, Value::Toggle(fit)) => ui.fit = fit,
        (VoxelBrushField::Footing, Value::Choice(at, _)) => {
            ui.footing = VoxelMaterial::ALL.get(at).copied().unwrap_or(VoxelMaterial::Ashlar)
        }
        _ => return false,
    }
    true
}

/// A stroke under way: the volume, whether it fills, where its last ball
/// was or where its box began.
struct Stroke {
    entity: Entity,
    fill: bool,
    last: Vec3,
    /// Whether it made its change as it began (a print, an outline closed),
    /// which dragging does not repeat.
    made: bool,
}

/// The Scene view's voxel brush.
#[derive(Default)]
pub struct VoxelTools {
    /// The volume's ground under the pointer, while the brush shows.
    under: Option<(Entity, Vec3)>,
    stroke: Option<Stroke>,
    /// An outline's points so far, on the volume they are set on.
    outline: Option<(Entity, Vec<Vec3>)>,
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

/// The box a drag from `from` to `to` marks out, turned `turn` degrees about
/// the up axis at `from`: the middle of its foot on the ground where the
/// drag began, and its size across, at least a voxel each way.
fn box_between(from: Vec3, to: Vec3, turn: f32) -> (Vec3, [f32; 2]) {
    let turned = Quat::from_rotation_y(turn.to_radians());
    let local = turned.inverse() * (to - from);
    let size = [local.x.abs().max(1.0 / 16.0), local.z.abs().max(1.0 / 16.0)];
    let middle = from + turned * Vec3::new(local.x * 0.5, 0.0, local.z * 0.5);
    (Vec3::new(middle.x, from.y, middle.z), size)
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
        let mode = ui(&game.engine.world).mode;
        // An outline set with another mode, or on another volume, is let go.
        if !mode.outlines() || self.outline.as_ref().is_some_and(|(on, _)| Some(*on) != entity) {
            self.outline = None;
        }
        let (Some(entity), Some(point)) = (entity, point) else { return };
        if mode == VoxelBrushMode::Off {
            return;
        }
        self.under = ground_under(game, camera, entity, point).map(|at| (entity, at));
    }

    /// Starts a stroke under `point`: a ball carved or filled there, Shift
    /// doing the other; a box's first corner; an outline's next point, or
    /// the outline closed on its first; or the brush's print printed there.
    /// Returns whether it took the press.
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
        let made = |fill| Some(Stroke { entity, fill, last: at, made: true });
        match ui.mode {
            VoxelBrushMode::Print => {
                let Some(print) = ui.print else { return false };
                let data = VoxelEditData {
                    fit: ui.fit,
                    fill: ui.fit.then_some(ui.footing),
                    ..VoxelEditData::printed(print.to_string(), at.to_array(), [0.0, ui.turn, 0.0])
                };
                voxear_engine::voxel_volumes::print_as(&mut game.content, &mut game.engine.world, &data, Some(entity));
                // A stroke of its own, for one step to undo, which dragging carries no further.
                self.stroke = made(false);
            }
            VoxelBrushMode::Area | VoxelBrushMode::Cover => {
                let at = ui.snapped(at);
                let points = &mut self.outline.get_or_insert_with(|| (entity, Vec::new())).1;
                let closes = points.len() >= 3
                    && camera.project(points[0]).is_some_and(|first| {
                        (first[0] - point[0]).hypot(first[1] - point[1]) <= CLOSING
                    });
                if !closes {
                    points.push(at);
                    return true;
                }
                let (_, points) = self.outline.take().expect("set above");
                let outline: Vec<[f32; 2]> = points.iter().map(|p| [p.x, p.z]).collect();
                let fill = (!shift).then_some(ui.ground);
                let mut data = match ui.mode {
                    VoxelBrushMode::Area => VoxelEditData::area(outline, points[0].y - ui.height, points[0].y, fill),
                    _ => VoxelEditData::cover(outline, ui.height, fill),
                };
                data.clear = if shift { 0.0 } else { ui.clear };
                data.tint = ui.tint();
                voxear_engine::voxel_volumes::shape_edit(&mut game.engine.world, &data, Some(entity));
                self.stroke = made(!shift);
            }
            VoxelBrushMode::Box => {
                self.stroke = Some(Stroke { entity, fill: !shift, last: ui.snapped(at), made: false });
            }
            VoxelBrushMode::Dig | VoxelBrushMode::Build => {
                let build = (ui.mode == VoxelBrushMode::Build) != shift;
                self.stroke = Some(Stroke { entity, fill: build, last: at, made: false });
                self.set_voxel(game, camera, point);
            }
            _ => {
                let fill = (ui.mode == VoxelBrushMode::Fill) != shift;
                self.stroke = Some(Stroke { entity, fill, last: at, made: false });
                self.dab(game, at);
            }
        }
        true
    }

    /// Carries the stroke on to the ground under `point`: a ball each time
    /// it has gone a third of the size, or a box's far corner followed.
    pub fn drag_to(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2]) {
        let Some(stroke) = self.stroke.as_ref().filter(|stroke| !stroke.made) else { return };
        let (entity, last) = (stroke.entity, stroke.last);
        let Some(at) = ground_under(game, camera, entity, point) else { return };
        let ui = ui(&game.engine.world);
        if ui.mode == VoxelBrushMode::Box {
            self.under = Some((entity, ui.snapped(at)));
            return;
        }
        if matches!(ui.mode, VoxelBrushMode::Dig | VoxelBrushMode::Build) {
            self.under = Some((entity, at));
            if at.distance(last) >= 1.0 / 16.0 {
                self.set_voxel(game, camera, point);
                if let Some(stroke) = &mut self.stroke {
                    stroke.last = at;
                }
            }
            return;
        }
        self.under = Some((entity, at));
        let spacing = (ui.size * SPACING).max(0.05);
        let gone = at.distance(last);
        let steps = (gone / spacing).floor() as usize;
        for step in 1..=steps.min(64) {
            self.dab(game, last.lerp(at, step as f32 * spacing / gone));
        }
    }

    /// Digs the voxel the pointer's ray meets, or builds one on the face it
    /// meets, for the stroke.
    fn set_voxel(&mut self, game: &mut Game, camera: &ViewCamera, point: [f32; 2]) {
        let Some(stroke) = &self.stroke else { return };
        let (origin, direction) = camera.ray(point);
        let world = &mut game.engine.world;
        let Some((_, hit)) = voxear_engine::voxel_volumes::raycast(world, origin, direction, REACH, Some(stroke.entity))
        else {
            return;
        };
        let ui = ui(world);
        // Half a voxel into the one met, or out onto the face beside it.
        let inward = if stroke.fill { 1.0 } else { -1.0 } * 0.5 / 16.0;
        let at = hit.point + hit.normal * inward;
        let kind = stroke.fill.then_some(voxear_render::VoxelKind { material: ui.ground, tint: ui.tint() });
        voxear_engine::voxel_volumes::set_voxels(world, &[at], kind, Some(stroke.entity));
    }

    /// A ball at `at` for the stroke.
    fn dab(&mut self, game: &mut Game, at: Vec3) {
        let Some(stroke) = &mut self.stroke else { return };
        let ui = ui(&game.engine.world);
        let mut data = VoxelEditData::ball(at.to_array(), ui.size, stroke.fill.then_some(ui.ground));
        data.tint = ui.tint().filter(|_| stroke.fill);
        voxear_engine::voxel_volumes::shape_edit(&mut game.engine.world, &data, Some(stroke.entity));
        stroke.last = at;
    }

    /// Ends the stroke, a box made as it ends, giving the volume it changed,
    /// for one step to undo: none where it changed nothing yet, as an
    /// outline's points do not.
    pub fn end(&mut self, game: &mut Game) -> Option<Entity> {
        let stroke = self.stroke.take()?;
        let ui = ui(&game.engine.world);
        if ui.mode == VoxelBrushMode::Box && !stroke.made {
            let to = self.under.map_or(stroke.last, |(_, at)| at);
            let (foot, [width, depth]) = box_between(stroke.last, to, ui.turn);
            let foot = if stroke.fill { foot } else { foot - Vec3::Y * ui.height };
            let fill = stroke.fill.then_some(ui.ground);
            let mut data =
                VoxelEditData::boxed(foot.to_array(), [width, ui.height, depth], [0.0, ui.turn, 0.0], fill);
            data.clear = if stroke.fill { ui.clear } else { 0.0 };
            data.tint = ui.tint().filter(|_| stroke.fill);
            voxear_engine::voxel_volumes::shape_edit(&mut game.engine.world, &data, Some(stroke.entity));
        }
        Some(stroke.entity)
    }

    /// The brush under the pointer, as shapes in the view's own coordinates:
    /// a ball's three rings, a box's or a print's edges, an outline so far.
    pub fn shapes(&self, game: &Game, camera: &ViewCamera, entity: Option<Entity>, shift: bool) -> Vec<Shape> {
        let Some((under, at)) = self.under else { return Vec::new() };
        if entity != Some(under) {
            return Vec::new();
        }
        let ui = ui(&game.engine.world);
        let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
        let fill = self.stroke.as_ref().map_or((ui.mode != VoxelBrushMode::Carve) != shift, |stroke| stroke.fill);
        let color = Color::hex(if fill { FILL_COLOR } else { CARVE_COLOR });
        let mut shapes = Vec::new();
        match ui.mode {
            VoxelBrushMode::Print => return print_box(game, camera, &ui, at),
            VoxelBrushMode::Box => {
                let from = self.stroke.as_ref().map_or(ui.snapped(at), |stroke| stroke.last);
                let (foot, [width, depth]) = box_between(from, at, ui.turn);
                let foot = if fill { foot } else { foot - Vec3::Y * ui.height };
                let half = Vec3::new(width * 0.5, 0.0, depth * 0.5);
                let low = -half;
                let high = half + Vec3::Y * ui.height;
                shapes.extend(box_edges(camera, foot, Quat::from_rotation_y(ui.turn.to_radians()), low, high, color));
            }
            VoxelBrushMode::Area | VoxelBrushMode::Cover => {
                let mut points: Vec<Vec3> = self.outline.as_ref().map(|(_, points)| points.clone()).unwrap_or_default();
                points.push(ui.snapped(at));
                let projected: Vec<Option<[f32; 2]>> = points.iter().map(|p| camera.project(*p).map(local)).collect();
                for pair in projected.windows(2) {
                    if let (Some(from), Some(to)) = (pair[0], pair[1]) {
                        shapes.push(Shape::Line { from, to, width: 1.5, color });
                    }
                }
                for dot in projected.iter().flatten() {
                    shapes.push(Shape::Circle { center: *dot, radius: 3.0, fill: color, border: None });
                }
            }
            _ => {
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
            }
        }
        shapes
    }
}

/// The twelve edges of the box from `low` to `high`, turned `turn` and
/// placed at `at`, as shapes in the view's own coordinates.
fn box_edges(camera: &ViewCamera, at: Vec3, turn: Quat, low: Vec3, high: Vec3, color: Color) -> Vec<Shape> {
    let local = |p: [f32; 2]| [p[0] - camera.rect[0], p[1] - camera.rect[1]];
    let corner = |i: usize| {
        let c = Vec3::new([low.x, high.x][i & 1], [low.y, high.y][(i >> 1) & 1], [low.z, high.z][i >> 2]);
        camera.project(at + turn * c).map(local)
    };
    let mut shapes = Vec::new();
    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
        if let (Some(from), Some(to)) = (corner(a), corner(b)) {
            shapes.push(Shape::Line { from, to, width: 1.5, color });
        }
    }
    shapes
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
        shapes = box_edges(camera, at, Quat::from_rotation_y(ui.turn.to_radians()), low, high, color);
    }
    if let Some(middle) = camera.project(at).map(local) {
        shapes.push(Shape::Circle { center: middle, radius: 3.0, fill: color, border: None });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_marks_out_a_box_turned_about_where_it_began() {
        let (foot, size) = box_between(Vec3::new(1.0, 5.0, 1.0), Vec3::new(5.0, 7.0, 3.0), 0.0);
        assert_eq!((foot, size), (Vec3::new(3.0, 5.0, 2.0), [4.0, 2.0]));
        // Turned a quarter, the drag along X is the box's depth along its own Z.
        let (foot, size) = box_between(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), 90.0);
        assert!((foot - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-4 && (size[1] - 4.0).abs() < 1e-4, "{foot} {size:?}");
        let ui = VoxelBrushUi { snap: 0.5, ..VoxelBrushUi::default() };
        assert_eq!(ui.snapped(Vec3::new(1.2, 3.3, -0.8)), Vec3::new(1.0, 3.3, -1.0));
        assert_eq!(VoxelBrushUi::default().tint(), None, "white is its own colours");
    }
}
