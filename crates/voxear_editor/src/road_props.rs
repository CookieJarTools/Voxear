//! The Road and Area Fill components' fields in the Inspector. A road's
//! points are dragged in the Scene view (see `editor::road_view`); its first
//! mixed block is its Block here, and more of a mix, a pattern and widths
//! point by point come from scripts, `vox editor call` and the scene file.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    roads::{AreaFill, AreaLevel, MixBlock, Road, RoadGround, RoadSteps},
};

use crate::{
    fields::{self, Pick},
    props::{Part, Prop, Value},
};

/// A value the Road component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoadField {
    Ground,
    Width,
    Block,
    Edge,
    Kerb,
    Grade,
    SlopeOut,
    Steps,
    Bridge,
    Deck,
    Tunnel,
    Roof,
    Water,
    Depth,
    ClearPlants,
    Priority,
}

/// A value the Area Fill component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AreaField {
    Level,
    Size,
    Block,
    Fill,
    SlopeOut,
    ClearPlants,
    Priority,
}

use AreaField as A;
use RoadField as R;

pub fn road_props(world: &World, entity: Entity) -> Vec<Prop> {
    let Some(road) = world.get::<Road>(entity) else { return Vec::new() };
    let mut fields = vec![R::Ground, R::Width, R::Block, R::Edge, R::Kerb];
    if road.ground == RoadGround::Grade {
        fields.extend([R::Grade, R::SlopeOut, R::Steps]);
    }
    fields.push(R::Bridge);
    if road.bridge {
        fields.push(R::Deck);
    }
    fields.push(R::Tunnel);
    if road.tunnel {
        fields.push(R::Roof);
    }
    fields.push(R::Water);
    if road.water.is_some() {
        fields.push(R::Depth);
    }
    fields.extend([R::ClearPlants, R::Priority]);
    fields.into_iter().map(Prop::Road).collect()
}

pub fn area_props() -> Vec<Prop> {
    [A::Level, A::Size, A::Block, A::Fill, A::SlopeOut, A::ClearPlants, A::Priority]
        .into_iter()
        .map(Prop::AreaFill)
        .collect()
}

pub fn road_label(field: RoadField) -> String {
    match field {
        R::Ground => "Ground",
        R::Width => "Width",
        R::Block => "Block",
        R::Edge => "Edge",
        R::Kerb => "Kerb",
        R::Grade => "Grade",
        R::SlopeOut => "Slope Out",
        R::Steps => "Steps",
        R::Bridge => "Bridge",
        R::Deck => "Deck",
        R::Tunnel => "Tunnel",
        R::Roof => "Roof",
        R::Water => "Water",
        R::Depth => "Depth",
        R::ClearPlants => "Clear Plants",
        R::Priority => "Priority",
    }
    .into()
}

pub fn area_label(field: AreaField) -> String {
    match field {
        A::Level => "Level",
        A::Size => "Size",
        A::Block => "Block",
        A::Fill => "Fill",
        A::SlopeOut => "Slope Out",
        A::ClearPlants => "Clear Plants",
        A::Priority => "Priority",
    }
    .into()
}

pub fn road_tooltip(field: RoadField) -> &'static str {
    match field {
        R::Ground => {
            "Follow lays it on the ground as it is; Grade cuts and fills the ground to a line between its points. Default: Grade."
        }
        R::Width => {
            "How wide it is at every point, in blocks. Points can each have their own from scripts. Default: 5."
        }
        R::Block => "The block it is laid with; None for the ground's own top. A mix and a pattern come from scripts.",
        R::Edge => "The block along each edge; None for none.",
        R::Kerb => "How many blocks the edge stands over the road. Default: 0.",
        R::Grade => "The most it climbs a block along, in blocks. Default: 0.25.",
        R::SlopeOut => "Over how many blocks each side the ground eases back to the generator's. Default: 3.",
        R::Steps => "Blocks climbs a whole block a step; Slabs lays a slab before each step up. Default: Blocks.",
        R::Bridge => "Over ground or water well under it, a deck on piers rather than a bank of earth. Default: off.",
        R::Deck => "The block of a bridge's deck and piers.",
        R::Tunnel => "Through ground well over it, a tunnel rather than a cutting. Default: off.",
        R::Roof => "The block of a tunnel's roof, where the ground does not hold one.",
        R::Water => "Water in it, as a canal; None for a road.",
        R::Depth => "How many blocks deep a canal is. Default: 2.",
        R::ClearPlants => "Whether generated trees, plants and structures are left out of its reach. Default: on.",
        R::Priority => "Where two roads meet, the higher priority's is laid. Default: 0.",
    }
}

pub fn area_tooltip(field: AreaField) -> &'static str {
    match field {
        A::Level => "Level at its Obj's height, or on the ground as it is. Default: Follow.",
        A::Size => "Its size along x and z, a rectangle round its Obj, in metres. Any outline comes from scripts.",
        A::Block => "The block it is paved with; None for the ground's own top. A mix and a pattern come from scripts.",
        A::Fill => "What made ground is built of; None for its biome's own.",
        A::SlopeOut => "Over how many blocks the ground eases back to the generator's. Default: 3.",
        A::ClearPlants => "Whether generated trees, plants and structures are left out of it. Default: on.",
        A::Priority => "Where two meet, the higher priority's ground wins. Default: 0.",
    }
}

pub fn road_key(field: RoadField) -> &'static str {
    match field {
        R::Ground => "ground",
        R::Width => "points",
        R::Block => "mix",
        R::Edge => "edge",
        R::Kerb => "kerb",
        R::Grade => "grade",
        R::SlopeOut => "slope_out",
        R::Steps => "steps",
        R::Bridge => "bridge",
        R::Deck => "deck",
        R::Tunnel => "tunnel",
        R::Roof => "roof",
        R::Water => "water",
        R::Depth => "depth",
        R::ClearPlants => "clear_plants",
        R::Priority => "priority",
    }
}

pub fn area_key(field: AreaField) -> &'static str {
    match field {
        A::Level => "level",
        A::Size => "outline",
        A::Block => "mix",
        A::Fill => "fill",
        A::SlopeOut => "slope_out",
        A::ClearPlants => "clear_plants",
        A::Priority => "priority",
    }
}

pub fn road_unit(field: RoadField) -> Option<&'static str> {
    match field {
        R::Width | R::Kerb | R::SlopeOut | R::Depth => Some("blocks"),
        _ => None,
    }
}

pub fn area_unit(field: AreaField) -> Option<&'static str> {
    match field {
        A::Size => Some("m"),
        A::SlopeOut => Some("blocks"),
        _ => None,
    }
}

pub fn road_range(field: RoadField) -> Option<[f32; 2]> {
    match field {
        R::Width => Some([1.0, 64.0]),
        R::Kerb => Some([0.0, 4.0]),
        R::Grade => Some([0.05, 2.0]),
        R::SlopeOut => Some([0.0, 16.0]),
        R::Depth => Some([1.0, 16.0]),
        R::Priority => Some([-100.0, 100.0]),
        _ => None,
    }
}

pub fn road_picked(field: RoadField) -> bool {
    matches!(field, R::Block | R::Edge | R::Deck | R::Roof | R::Water)
}

pub fn area_picked(field: AreaField) -> bool {
    matches!(field, A::Block | A::Fill)
}

/// The blocks of the project to pick from, and None.
pub fn block_choices(game: &Game) -> Vec<(String, Pick)> {
    let mut found = vec![("None".to_string(), Pick::Asset(None))];
    found.extend(
        fields::assets_of(game, Some(AssetKind::Block)).into_iter().filter(|(_, pick)| *pick != Pick::Asset(None)),
    );
    found
}

/// The GUID of the block picked, or none; false where it is no block.
fn picked_guid(game: &Game, choice: &Pick) -> Option<Option<AssetGuid>> {
    let Pick::Asset(path) = choice else { return None };
    match path {
        Some(path) => game.content.parse(path).ok().and_then(|reference| reference.guid()).map(Some),
        None => Some(None),
    }
}

/// A block's path as the Inspector shows it, or None.
fn block_value(game: &Game, guid: Option<AssetGuid>) -> Value {
    let path = guid.and_then(|guid| game.content.db().get(guid).map(|entry| entry.path.clone()));
    Value::Asset(path.unwrap_or_else(|| "None".into()))
}

fn first_mixed(mix: &[MixBlock]) -> Option<AssetGuid> {
    mix.first().and_then(|entry| entry.block)
}

/// Sets the first block of a mix, or with none, empties it.
fn set_first(mix: &mut Vec<MixBlock>, guid: Option<AssetGuid>) {
    match (guid, mix.first_mut()) {
        (None, _) => mix.clear(),
        (Some(guid), Some(first)) => first.block = Some(guid),
        (Some(guid), None) => mix.push(MixBlock { block: Some(guid), weight: 1 }),
    }
}

pub fn road_pick(game: &mut Game, entity: Entity, field: RoadField, choice: &Pick) -> bool {
    let Some(guid) = picked_guid(game, choice) else { return false };
    let Some(road) = game.engine.world.get_mut::<Road>(entity) else { return false };
    match field {
        R::Block => set_first(&mut road.mix, guid),
        R::Edge => road.edge = guid,
        R::Deck => road.deck = guid,
        R::Roof => road.roof = guid,
        R::Water => road.water = guid,
        _ => return false,
    }
    true
}

pub fn area_pick(game: &mut Game, entity: Entity, field: AreaField, choice: &Pick) -> bool {
    let Some(guid) = picked_guid(game, choice) else { return false };
    let Some(area) = game.engine.world.get_mut::<AreaFill>(entity) else { return false };
    match field {
        A::Block => set_first(&mut area.mix, guid),
        A::Fill => area.fill = guid,
        _ => return false,
    }
    true
}

fn choice<T: PartialEq + Copy>(all: &[T], now: T, name: fn(T) -> &'static str) -> Value {
    Value::Choice(
        all.iter().position(|kept| *kept == now).unwrap_or(0),
        all.iter().map(|v| name(*v).to_string()).collect(),
    )
}

pub fn road_get(game: &Game, entity: Entity, field: RoadField) -> Option<Value> {
    let road = game.engine.world.get::<Road>(entity)?;
    Some(match field {
        R::Ground => choice(&RoadGround::ALL, road.ground, RoadGround::name),
        R::Width => Value::Number(road.points.first().map_or(5.0, |point| point.width)),
        R::Block => block_value(game, first_mixed(&road.mix)),
        R::Edge => block_value(game, road.edge),
        R::Kerb => Value::Whole(i64::from(road.kerb)),
        R::Grade => Value::Number(road.grade),
        R::SlopeOut => Value::Whole(i64::from(road.slope_out)),
        R::Steps => choice(&[RoadSteps::Blocks, RoadSteps::Slabs], road.steps, |steps| match steps {
            RoadSteps::Blocks => "Blocks",
            RoadSteps::Slabs => "Slabs",
        }),
        R::Bridge => Value::Toggle(road.bridge),
        R::Deck => block_value(game, road.deck),
        R::Tunnel => Value::Toggle(road.tunnel),
        R::Roof => block_value(game, road.roof),
        R::Water => block_value(game, road.water),
        R::Depth => Value::Whole(i64::from(road.depth)),
        R::ClearPlants => Value::Toggle(road.clear_plants),
        R::Priority => Value::Whole(i64::from(road.priority)),
    })
}

pub fn road_set(game: &mut Game, entity: Entity, field: RoadField, value: Value) -> bool {
    let Some(road) = game.engine.world.get_mut::<Road>(entity) else { return false };
    let whole = |value: i64, [low, high]: [f32; 2]| value.clamp(low as i64, high as i64);
    match (field, value) {
        (R::Ground, Value::Choice(at, _)) => road.ground = RoadGround::ALL.get(at).copied().unwrap_or_default(),
        (R::Width, Value::Number(width)) => {
            road.points.iter_mut().for_each(|point| point.width = width.clamp(1.0, 64.0))
        }
        (R::Kerb, Value::Whole(kerb)) => road.kerb = whole(kerb, [0.0, 4.0]) as u32,
        (R::Grade, Value::Number(grade)) => road.grade = grade.clamp(0.05, 2.0),
        (R::SlopeOut, Value::Whole(steps)) => road.slope_out = whole(steps, [0.0, 16.0]) as u32,
        (R::Steps, Value::Choice(at, _)) => road.steps = if at == 1 { RoadSteps::Slabs } else { RoadSteps::Blocks },
        (R::Bridge, Value::Toggle(on)) => road.bridge = on,
        (R::Tunnel, Value::Toggle(on)) => road.tunnel = on,
        (R::Depth, Value::Whole(depth)) => road.depth = whole(depth, [1.0, 16.0]) as u32,
        (R::ClearPlants, Value::Toggle(on)) => road.clear_plants = on,
        (R::Priority, Value::Whole(priority)) => road.priority = whole(priority, [-1000.0, 1000.0]) as i32,
        _ => return false,
    }
    true
}

pub fn area_get(game: &Game, entity: Entity, field: AreaField) -> Option<Value> {
    let area = game.engine.world.get::<AreaFill>(entity)?;
    Some(match field {
        A::Level => choice(&AreaLevel::ALL, area.level, AreaLevel::name),
        A::Size => {
            let span = |axis: usize| {
                let values = area.outline.iter().map(|corner| corner[axis]);
                values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
            };
            Value::Vector(vec![span(0).max(0.0), span(1).max(0.0)])
        }
        A::Block => block_value(game, first_mixed(&area.mix)),
        A::Fill => block_value(game, area.fill),
        A::SlopeOut => Value::Whole(i64::from(area.slope_out)),
        A::ClearPlants => Value::Toggle(area.clear_plants),
        A::Priority => Value::Whole(i64::from(area.priority)),
    })
}

pub fn area_set(game: &mut Game, entity: Entity, field: AreaField, value: Value) -> bool {
    let Some(area) = game.engine.world.get_mut::<AreaFill>(entity) else { return false };
    match (field, value) {
        (A::Level, Value::Choice(at, _)) => area.level = AreaLevel::ALL.get(at).copied().unwrap_or_default(),
        (A::Size, Value::Vector(size)) if size.len() == 2 => {
            let [x, z] = [size[0].clamp(0.5, 4096.0) * 0.5, size[1].clamp(0.5, 4096.0) * 0.5];
            area.outline = vec![[-x, -z], [x, -z], [x, z], [-x, z]];
        }
        (A::SlopeOut, Value::Whole(steps)) => area.slope_out = steps.clamp(0, 16) as u32,
        (A::ClearPlants, Value::Toggle(on)) => area.clear_plants = on,
        (A::Priority, Value::Whole(priority)) => area.priority = priority.clamp(-1000, 1000) as i32,
        _ => return false,
    }
    true
}

/// Adds the Road and Area Fill parts where `entity` has the components.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<Road>(entity) {
        parts.push(Part::Road);
    }
    if world.has::<AreaFill>(entity) {
        parts.push(Part::AreaFill);
    }
}
