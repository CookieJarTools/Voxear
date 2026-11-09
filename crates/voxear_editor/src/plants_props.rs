//! A Foliage's plants in the Inspector (the Vegetation module).
//!
//! Its section has two tools along its top (`plants_panel`): **Settings**,
//! how its plants are drawn, their trunks and what one cut down becomes;
//! and **Paint**, the brush the Scene view paints its plants with
//! (`plants_tools`) and how they grow where it paints. Which tool shows is
//! the editor's, kept in the world as [`FoliageUi`], not saved; the brush's
//! size and strength are the terrain brush's, shared.
//!
//! The trunk, the Cut prefab and how plants are painted are the scene's,
//! written with the Foliage (`FoliageSource`), and the trunk and the Cut
//! prefab are the Obj's `Plants` too, for physics and scripts.

use anyhow::Result;
use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    scene_file::{FoliageSource, PaintData},
};
use voxear_scene::{Plants, Trunk};

use crate::{
    fields::{self, Pick},
    props::{Prop, Value},
};

/// Which of a Foliage's tools its Inspector shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FoliageTool {
    #[default]
    Settings,
    Paint,
}

impl FoliageTool {
    pub const ALL: [FoliageTool; 2] = [FoliageTool::Settings, FoliageTool::Paint];

    pub fn name(self) -> &'static str {
        match self {
            FoliageTool::Settings => "Settings",
            FoliageTool::Paint => "Paint",
        }
    }
}

/// What the editor is doing with Foliage: the tool shown. Kept in the edit
/// world, not saved.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FoliageUi {
    pub tool: FoliageTool,
}

pub fn ui(world: &World) -> FoliageUi {
    world.resource::<FoliageUi>().copied().unwrap_or_default()
}

pub fn show_tool(world: &mut World, tool: FoliageTool) {
    match world.resource_mut::<FoliageUi>() {
        Some(ui) => ui.tool = tool,
        None => world.insert_resource(FoliageUi { tool }),
    }
}

/// A value a Foliage's plants show: their trunks, what one cut down
/// becomes, the brush, and how painted plants grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantsField {
    Trunk,
    TrunkRadius,
    TrunkHeight,
    Cut,
    BrushSize,
    Strength,
    Density,
    Scale,
    Lean,
    Steepest,
    Spacing,
}

use PlantsField as F;

/// The values a Foliage shows: under Settings, `settings` (how it is
/// drawn), then its trunks and Cut prefab; under Paint, the brush and how
/// painted plants grow.
pub fn props(world: &World, entity: Entity, settings: Vec<Prop>) -> Vec<Prop> {
    if ui(world).tool == FoliageTool::Paint {
        let paint = [F::BrushSize, F::Strength, F::Density, F::Scale, F::Lean, F::Steepest, F::Spacing];
        return paint.into_iter().map(Prop::Plants).collect();
    }
    let mut props = settings;
    props.push(Prop::Plants(F::Trunk));
    if source(world, entity).is_some_and(|data| data.trunk.is_some()) {
        props.extend([Prop::Plants(F::TrunkRadius), Prop::Plants(F::TrunkHeight)]);
    }
    props.push(Prop::Plants(F::Cut));
    props
}

fn source(world: &World, entity: Entity) -> Option<&voxear_engine::scene_file::FoliageData> {
    world.get::<FoliageSource>(entity).map(|source| &source.0)
}

pub fn label(field: PlantsField) -> &'static str {
    match field {
        F::Trunk => "Trunk collider",
        F::TrunkRadius => "Trunk radius",
        F::TrunkHeight => "Trunk height",
        F::Cut => "Cut prefab",
        F::BrushSize => "Size",
        F::Strength => "Strength",
        F::Density => "Density",
        F::Scale => "Sizes",
        F::Lean => "Lean with ground",
        F::Steepest => "Steepest ground",
        F::Spacing => "Spacing",
    }
}

/// The name a prefab's locks give the value.
pub fn key(field: PlantsField) -> &'static str {
    match field {
        F::Trunk | F::TrunkRadius | F::TrunkHeight => "trunk",
        F::Cut => "cut",
        F::Density | F::Scale | F::Lean | F::Steepest | F::Spacing => "paint",
        // The brush is the editor's.
        F::BrushSize | F::Strength => "",
    }
}

pub fn unit(field: PlantsField) -> Option<&'static str> {
    match field {
        F::TrunkRadius | F::TrunkHeight | F::BrushSize | F::Spacing => Some("m"),
        F::Steepest => Some("°"),
        F::Density => Some("per 100 m²"),
        _ => None,
    }
}

/// The range a value's slider spans, for those shown as sliders.
pub fn range(field: PlantsField) -> Option<[f32; 2]> {
    match field {
        F::Strength | F::Lean => Some([0.0, 1.0]),
        F::Steepest => Some([0.0, 90.0]),
        _ => None,
    }
}

pub fn get(game: &Game, entity: Entity, field: PlantsField) -> Option<Value> {
    let world = &game.engine.world;
    let data = source(world, entity)?;
    let paint = data.paint;
    let brush = crate::terrain_props::ui(world).brush;
    Some(match field {
        F::Trunk => Value::Toggle(data.trunk.is_some()),
        F::TrunkRadius => Value::Number(data.trunk?.radius),
        F::TrunkHeight => Value::Number(data.trunk?.height),
        F::Cut => Value::Asset(path_of(game, data.cut.as_deref())),
        F::BrushSize => Value::Number(brush.radius),
        F::Strength => Value::Number(brush.strength),
        F::Density => Value::Number(paint.density),
        F::Scale => Value::Vector(paint.scale.to_vec()),
        F::Lean => Value::Number(paint.lean),
        F::Steepest => Value::Number(paint.steepest),
        F::Spacing => Value::Number(paint.spacing),
    })
}

/// An asset's path, by the reference the Foliage holds, or "None".
fn path_of(game: &Game, reference: Option<&str>) -> String {
    let guid = reference.and_then(|r| game.content.parse(r).ok()).and_then(|reference| reference.guid());
    guid.and_then(|guid| game.content.db().get(guid)).map_or("None".into(), |entry| entry.path.clone())
}

fn number(value: &Value) -> Option<f32> {
    match value {
        Value::Number(n) => Some(*n),
        Value::Whole(n) => Some(*n as f32),
        _ => None,
    }
}

/// Sets a value: the brush's are the editor's, the rest the Foliage's as
/// written, with its trunk and Cut prefab given to its plants too.
pub fn set(game: &mut Game, entity: Entity, field: PlantsField, value: Value) -> bool {
    let world = &mut game.engine.world;
    if matches!(field, F::BrushSize | F::Strength) {
        let brush = &mut crate::terrain_props::ui_mut(world).brush;
        let before = *brush;
        match field {
            F::BrushSize => brush.radius = number(&value).unwrap_or(brush.radius).clamp(0.1, 1000.0),
            _ => brush.strength = number(&value).unwrap_or(brush.strength).clamp(0.0, 1.0),
        }
        return *brush != before;
    }
    change(world, entity, |data| {
        let paint = &mut data.paint;
        match (field, &value) {
            (F::Trunk, Value::Toggle(on)) => data.trunk = on.then(|| data.trunk.unwrap_or_default()),
            (F::TrunkRadius, v) => {
                let trunk = data.trunk.get_or_insert_with(Trunk::default);
                trunk.radius = number(v).unwrap_or(trunk.radius).clamp(0.01, 100.0);
            }
            (F::TrunkHeight, v) => {
                let trunk = data.trunk.get_or_insert_with(Trunk::default);
                trunk.height = number(v).unwrap_or(trunk.height).clamp(0.05, 1000.0);
            }
            (F::Density, v) => paint.density = number(v).unwrap_or(paint.density).clamp(0.01, 100_000.0),
            (F::Scale, Value::Vector(v)) if v.len() >= 2 => {
                let [a, b] = [v[0].max(0.01), v[1].max(0.01)];
                paint.scale = [a.min(b), a.max(b)];
            }
            (F::Lean, v) => paint.lean = number(v).unwrap_or(paint.lean).clamp(0.0, 1.0),
            (F::Steepest, v) => paint.steepest = number(v).unwrap_or(paint.steepest).clamp(0.0, 90.0),
            (F::Spacing, v) => paint.spacing = number(v).unwrap_or(paint.spacing).max(0.0),
            _ => {}
        }
    })
}

/// Changes the Foliage as written, and gives its plants its trunk and Cut
/// prefab; whether anything changed.
fn change(world: &mut World, entity: Entity, how: impl FnOnce(&mut voxear_engine::scene_file::FoliageData)) -> bool {
    let Some(source) = world.get_mut::<FoliageSource>(entity) else { return false };
    let before = source.0.clone();
    how(&mut source.0);
    if source.0 == before {
        return false;
    }
    let (trunk, cut) = (source.0.trunk, source.0.cut.clone());
    if let Some(plants) = world.get_mut::<Plants>(entity) {
        (plants.trunk, plants.cut) = (trunk, cut);
    }
    true
}

/// What a picker shows: the project's prefabs, for what a plant cut down
/// becomes.
pub fn choices(game: &Game, field: PlantsField) -> Vec<(String, Pick)> {
    match field {
        F::Cut => fields::assets_of(game, Some(AssetKind::Prefab)),
        _ => Vec::new(),
    }
}

pub fn is_picked(field: PlantsField) -> bool {
    field == F::Cut
}

/// Takes the prefab picked for what a plant cut down becomes.
pub fn pick(game: &mut Game, entity: Entity, field: PlantsField, choice: &Pick) -> bool {
    let (F::Cut, Pick::Asset(path)) = (field, choice) else { return false };
    let reference = match path {
        Some(path) => match game.content.parse(path) {
            Ok(reference) => Some(reference.to_string()),
            Err(_) => return false,
        },
        None => None,
    };
    change(&mut game.engine.world, entity, |data| data.cut = reference)
}

/// How painted plants grow, as the Foliage says.
pub fn paint(world: &World, entity: Entity) -> PaintData {
    source(world, entity).map(|data| data.paint).unwrap_or_default()
}

/// Puts `plants` on the Foliage, placed one by one, in place of those it
/// had: written as rows, its scatter turned into rows too, and grown as its
/// `Plants`, trunks and Cut prefab kept.
pub fn replace_plants(world: &mut World, entity: Entity, plants: Vec<voxear_scene::Plant>) -> Result<()> {
    let source = world.get_mut::<FoliageSource>(entity).ok_or_else(|| anyhow::anyhow!("no Foliage"))?;
    source.0.plants = plants.iter().map(voxear_engine::scene_file::PlantRow::of).collect();
    source.0.scatter = None;
    let grown = source.0.grown();
    world.insert(entity, grown);
    Ok(())
}
