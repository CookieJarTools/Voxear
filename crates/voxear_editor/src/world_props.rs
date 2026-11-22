//! The World Area, Area Defaults, Route and Route Stop components' fields in
//! the Inspector. An area's outline and a route's points are dragged in the
//! Scene view (see `road_handles`); a painted area is painted with the Block
//! World's Area brush. Tags are a count, then a tag and its data for each.

use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    script_types::ScriptTypes,
    world_areas::{AreaDefaults, AreaEntry, AreaShape, WorldArea},
    world_routes::{Route, RouteKind, RouteStop},
};

use crate::{
    fields::{self, Pick},
    props::{Part, Prop, Value},
};

/// The Definition of tags' data assets, and of route kinds'.
const AREA_TAG: &str = "Voxear.AreaTag";
const ROUTE_KIND: &str = "Voxear.CustomRouteKind";
/// The most tags the Inspector lists on one area.
const MOST_TAGS: usize = 32;

/// A value one of the four shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorldField {
    AreaShape,
    AreaNumber,
    AreaHeights,
    AreaLow,
    AreaHigh,
    AreaPriority,
    AreaNoStructures,
    AreaTags,
    AreaTag(u8),
    AreaData(u8),
    DefaultTags,
    DefaultTag(u8),
    DefaultData(u8),
    DefaultBiome(u8),
    RouteKind,
    RouteCustom,
    RouteWidth,
    RouteOneWay,
    RouteDraught,
    RouteShore,
    RouteOnMap,
    RoutePoints,
    StopData,
}

use WorldField as W;

impl WorldField {
    pub fn part(self) -> Part {
        match self {
            W::AreaShape
            | W::AreaNumber
            | W::AreaHeights
            | W::AreaLow
            | W::AreaHigh
            | W::AreaPriority
            | W::AreaNoStructures
            | W::AreaTags
            | W::AreaTag(_)
            | W::AreaData(_) => Part::WorldArea,
            W::DefaultTags | W::DefaultTag(_) | W::DefaultData(_) | W::DefaultBiome(_) => Part::AreaDefaults,
            W::RouteKind
            | W::RouteCustom
            | W::RouteWidth
            | W::RouteOneWay
            | W::RouteDraught
            | W::RouteShore
            | W::RouteOnMap
            | W::RoutePoints => Part::Route,
            W::StopData => Part::RouteStop,
        }
    }
}

/// Adds the parts of the four `entity` has.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<WorldArea>(entity) {
        parts.push(Part::WorldArea);
    }
    if world.has::<AreaDefaults>(entity) {
        parts.push(Part::AreaDefaults);
    }
    if world.has::<Route>(entity) {
        parts.push(Part::Route);
    }
    if world.has::<RouteStop>(entity) {
        parts.push(Part::RouteStop);
    }
}

/// The fields `part` shows for `entity`, as its settings now are.
pub fn props(world: &World, entity: Entity, part: &Part) -> Vec<Prop> {
    let mut fields = Vec::new();
    match part {
        Part::WorldArea => {
            let Some(area) = world.get::<WorldArea>(entity) else { return Vec::new() };
            fields.push(W::AreaShape);
            if area.shape == AreaShape::Painted {
                fields.push(W::AreaNumber);
            }
            fields.push(W::AreaHeights);
            if area.low.is_some() || area.high.is_some() {
                fields.extend([W::AreaLow, W::AreaHigh]);
            }
            fields.extend([W::AreaPriority, W::AreaNoStructures, W::AreaTags]);
            for at in 0..area.tags.len().min(MOST_TAGS) as u8 {
                fields.extend([W::AreaTag(at), W::AreaData(at)]);
            }
        }
        Part::AreaDefaults => {
            let Some(defaults) = world.get::<AreaDefaults>(entity) else { return Vec::new() };
            fields.push(W::DefaultTags);
            for at in 0..defaults.tags.len().min(MOST_TAGS) as u8 {
                fields.extend([W::DefaultTag(at), W::DefaultData(at), W::DefaultBiome(at)]);
            }
        }
        Part::Route => {
            let Some(route) = world.get::<Route>(entity) else { return Vec::new() };
            fields.push(W::RouteKind);
            if route.kind == RouteKind::Custom {
                fields.push(W::RouteCustom);
            }
            fields.extend([W::RouteWidth, W::RouteOneWay]);
            if route.kind == RouteKind::Sea {
                fields.extend([W::RouteDraught, W::RouteShore]);
            }
            fields.extend([W::RouteOnMap, W::RoutePoints]);
        }
        Part::RouteStop => fields.push(W::StopData),
        _ => {}
    }
    fields.into_iter().map(Prop::World).collect()
}

pub fn label(field: WorldField) -> String {
    match field {
        W::AreaShape => "Shape".into(),
        W::AreaNumber => "Paint Number".into(),
        W::AreaHeights => "Heights".into(),
        W::AreaLow => "Low".into(),
        W::AreaHigh => "High".into(),
        W::AreaPriority => "Priority".into(),
        W::AreaNoStructures => "No Structures".into(),
        W::AreaTags | W::DefaultTags => "Tags".into(),
        W::AreaTag(at) | W::DefaultTag(at) => format!("Tag {}", at + 1),
        W::AreaData(at) | W::DefaultData(at) => format!("Data {}", at + 1),
        W::DefaultBiome(at) => format!("Biome {}", at + 1),
        W::RouteKind => "Kind".into(),
        W::RouteCustom => "Route Kind".into(),
        W::RouteWidth => "Width".into(),
        W::RouteOneWay => "One Way".into(),
        W::RouteDraught => "Draught".into(),
        W::RouteShore => "Shore".into(),
        W::RouteOnMap => "On Map".into(),
        W::RoutePoints => "Points".into(),
        W::StopData => "Data".into(),
    }
}

pub fn tooltip(field: WorldField) -> &'static str {
    match field {
        W::AreaShape => {
            "Outline: the outline round its Obj. Painted: what is painted with its number by the Area brush."
        }
        W::AreaNumber => "Its number on the world's painted Area map, given it the first time it is painted.",
        W::AreaHeights => "Any holds every height; Between holds only from Low to High. Default: Any.",
        W::AreaLow => "The lowest it holds, in metres up in the world.",
        W::AreaHigh => "The highest it holds, in metres up in the world.",
        W::AreaPriority => "Where two hold a place, the higher priority's tags are asked first. Default: 0.",
        W::AreaNoStructures => "Whether the generator's structures are kept out of it. Default: off.",
        W::AreaTags => "How many tags it has.",
        W::AreaTag(_) => "An Area Tag data asset. Structures keep within areas of some tags and out of others.",
        W::AreaData(_) => "The data asset the tag carries here, of the game's own; None for none.",
        W::DefaultTags => "How many tags the world has defaults for.",
        W::DefaultTag(_) => "An Area Tag data asset, for places no World Area with it holds.",
        W::DefaultData(_) => "The data asset the tag carries there.",
        W::DefaultBiome(_) => "The biome it holds for; None for anywhere. A biome's comes before anywhere's.",
        W::RouteKind => "What travels it. Every Road is a route of kind Road. Default: Road.",
        W::RouteCustom => "A Route Kind data asset of the game's own.",
        W::RouteWidth => "How wide it is, in metres. Default: 6.",
        W::RouteOneWay => "Whether it is travelled only from its first point towards its last. Default: off.",
        W::RouteDraught => "How deep the water must be under a sea route, in metres. Default: 3.",
        W::RouteShore => "How far a sea route keeps from land, in metres. Default: 24.",
        W::RouteOnMap => "Whether the game's maps draw it. Default: on.",
        W::RoutePoints => "How many points it has. Drag them in the Scene view; Lay Over Sea lays a sea route itself.",
        W::StopData => "The data asset of the game's own it holds: a harbour's, an inn's.",
    }
}

/// Its name as calls and locks give it; none for a tag's row, named by its
/// label, as `tag_1`.
pub fn key(field: WorldField) -> &'static str {
    match field {
        W::AreaShape => "shape",
        W::AreaNumber => "number",
        W::AreaHeights => "heights",
        W::AreaLow => "low",
        W::AreaHigh => "high",
        W::AreaPriority => "priority",
        W::AreaNoStructures => "no_structures",
        W::AreaTags | W::DefaultTags => "tags",
        W::AreaTag(_) | W::AreaData(_) | W::DefaultTag(_) | W::DefaultData(_) | W::DefaultBiome(_) => "",
        W::RouteKind => "kind",
        W::RouteCustom => "custom",
        W::RouteWidth => "width",
        W::RouteOneWay => "one_way",
        W::RouteDraught => "draught",
        W::RouteShore => "shore",
        W::RouteOnMap => "on_map",
        W::RoutePoints => "points",
        W::StopData => "data",
    }
}

pub fn unit(field: WorldField) -> Option<&'static str> {
    match field {
        W::AreaLow | W::AreaHigh | W::RouteWidth | W::RouteDraught | W::RouteShore => Some("m"),
        _ => None,
    }
}

/// Whether the field picks an asset.
pub fn is_picked(field: WorldField) -> bool {
    matches!(
        field,
        W::AreaTag(_)
            | W::AreaData(_)
            | W::DefaultTag(_)
            | W::DefaultData(_)
            | W::DefaultBiome(_)
            | W::RouteCustom
            | W::StopData
    )
}

/// What the picker of a picked field calls what it takes.
pub fn takes_name(field: WorldField) -> &'static str {
    match field {
        W::AreaTag(_) | W::DefaultTag(_) => "Area Tag",
        W::DefaultBiome(_) => "Biome",
        W::RouteCustom => "Route Kind",
        _ => "Data Asset",
    }
}

/// What a picked field can take, None first.
pub fn choices(game: &Game, types: &ScriptTypes, field: WorldField) -> Vec<(String, Pick)> {
    match field {
        W::AreaTag(_) | W::DefaultTag(_) => fields::data_assets(game, types, AREA_TAG),
        W::RouteCustom => fields::data_assets(game, types, ROUTE_KIND),
        W::DefaultBiome(_) => fields::assets_of(game, Some(AssetKind::Biome)),
        _ => fields::assets_of(game, Some(AssetKind::Data)),
    }
}

fn picked_guid(game: &Game, choice: &Pick) -> Option<Option<AssetGuid>> {
    let Pick::Asset(path) = choice else { return None };
    match path {
        Some(path) => game.content.parse(path).ok().and_then(|reference| reference.guid()).map(Some),
        None => Some(None),
    }
}

fn asset_value(game: &Game, guid: Option<AssetGuid>) -> Value {
    let path = guid.and_then(|guid| game.content.db().get(guid).map(|entry| entry.path.clone()));
    Value::Asset(path.unwrap_or_else(|| "None".into()))
}

/// Puts the asset picked in a picked field.
pub fn pick(game: &mut Game, entity: Entity, field: WorldField, choice: &Pick) -> bool {
    let Some(guid) = picked_guid(game, choice) else { return false };
    let world = &mut game.engine.world;
    match field {
        W::AreaTag(at) | W::AreaData(at) => {
            let Some(area) = world.get_mut::<WorldArea>(entity) else { return false };
            let Some(entry) = area.tags.get_mut(usize::from(at)) else { return false };
            match field {
                W::AreaTag(_) => entry.tag = guid,
                _ => entry.data = guid,
            }
        }
        W::DefaultTag(at) | W::DefaultData(at) | W::DefaultBiome(at) => {
            let Some(defaults) = world.get_mut::<AreaDefaults>(entity) else { return false };
            let Some(entry) = defaults.tags.get_mut(usize::from(at)) else { return false };
            match field {
                W::DefaultTag(_) => entry.tag = guid,
                W::DefaultData(_) => entry.data = guid,
                _ => entry.biome = guid,
            }
        }
        W::RouteCustom => {
            let Some(route) = world.get_mut::<Route>(entity) else { return false };
            route.custom = guid;
        }
        W::StopData => {
            let Some(stop) = world.get_mut::<RouteStop>(entity) else { return false };
            stop.data = guid;
        }
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

pub fn get(game: &Game, entity: Entity, field: WorldField) -> Option<Value> {
    let world = &game.engine.world;
    Some(match field.part() {
        Part::WorldArea => {
            let area = world.get::<WorldArea>(entity)?;
            let entry = |at: u8| area.tags.get(usize::from(at)).copied();
            match field {
                W::AreaShape => choice(&AreaShape::ALL, area.shape, AreaShape::name),
                W::AreaNumber => Value::Fixed(area.number.to_string()),
                W::AreaHeights => {
                    let between = area.low.is_some() || area.high.is_some();
                    Value::Choice(usize::from(between), vec!["Any".into(), "Between".into()])
                }
                W::AreaLow => Value::Number(area.low.unwrap_or(f32::MIN).max(-100_000.0)),
                W::AreaHigh => Value::Number(area.high.unwrap_or(f32::MAX).min(100_000.0)),
                W::AreaPriority => Value::Whole(i64::from(area.priority)),
                W::AreaNoStructures => Value::Toggle(area.no_structures),
                W::AreaTags => Value::Whole(area.tags.len() as i64),
                W::AreaTag(at) => asset_value(game, entry(at)?.tag),
                W::AreaData(at) => asset_value(game, entry(at)?.data),
                _ => return None,
            }
        }
        Part::AreaDefaults => {
            let defaults = world.get::<AreaDefaults>(entity)?;
            let entry = |at: u8| defaults.tags.get(usize::from(at)).copied();
            match field {
                W::DefaultTags => Value::Whole(defaults.tags.len() as i64),
                W::DefaultTag(at) => asset_value(game, entry(at)?.tag),
                W::DefaultData(at) => asset_value(game, entry(at)?.data),
                W::DefaultBiome(at) => asset_value(game, entry(at)?.biome),
                _ => return None,
            }
        }
        Part::Route => {
            let route = world.get::<Route>(entity)?;
            match field {
                W::RouteKind => choice(&RouteKind::ALL, route.kind, RouteKind::name),
                W::RouteCustom => asset_value(game, route.custom),
                W::RouteWidth => Value::Number(route.width),
                W::RouteOneWay => Value::Toggle(route.one_way),
                W::RouteDraught => Value::Number(route.draught),
                W::RouteShore => Value::Number(route.shore),
                W::RouteOnMap => Value::Toggle(route.on_map),
                W::RoutePoints => Value::Fixed(route.points.len().to_string()),
                _ => return None,
            }
        }
        Part::RouteStop => asset_value(game, world.get::<RouteStop>(entity)?.data),
        _ => return None,
    })
}

pub fn set(game: &mut Game, entity: Entity, field: WorldField, value: Value) -> bool {
    let world = &mut game.engine.world;
    let far = 100_000.0;
    let count = |value: i64| value.clamp(0, MOST_TAGS as i64) as usize;
    match field.part() {
        Part::WorldArea => {
            let Some(area) = world.get_mut::<WorldArea>(entity) else { return false };
            match (field, value) {
                (W::AreaShape, Value::Choice(at, _)) => {
                    area.shape = AreaShape::ALL.get(at).copied().unwrap_or_default()
                }
                (W::AreaHeights, Value::Choice(at, _)) => match at {
                    0 => (area.low, area.high) = (None, None),
                    _ if area.low.is_none() && area.high.is_none() => (area.low, area.high) = (Some(0.0), Some(64.0)),
                    _ => return false,
                },
                (W::AreaLow, Value::Number(low)) => area.low = Some(low.clamp(-far, far)),
                (W::AreaHigh, Value::Number(high)) => area.high = Some(high.clamp(-far, far)),
                (W::AreaPriority, Value::Whole(priority)) => area.priority = priority.clamp(-1000, 1000) as i32,
                (W::AreaNoStructures, Value::Toggle(on)) => area.no_structures = on,
                (W::AreaTags, Value::Whole(many)) => area.tags.resize(count(many), AreaEntry::default()),
                _ => return false,
            }
        }
        Part::AreaDefaults => {
            let Some(defaults) = world.get_mut::<AreaDefaults>(entity) else { return false };
            match (field, value) {
                (W::DefaultTags, Value::Whole(many)) => defaults.tags.resize(count(many), AreaEntry::default()),
                _ => return false,
            }
        }
        Part::Route => {
            let Some(route) = world.get_mut::<Route>(entity) else { return false };
            match (field, value) {
                (W::RouteKind, Value::Choice(at, _)) => {
                    route.kind = RouteKind::ALL.get(at).copied().unwrap_or_default()
                }
                (W::RouteWidth, Value::Number(width)) => route.width = width.clamp(0.5, 500.0),
                (W::RouteOneWay, Value::Toggle(on)) => route.one_way = on,
                (W::RouteDraught, Value::Number(draught)) => route.draught = draught.clamp(0.0, 200.0),
                (W::RouteShore, Value::Number(shore)) => route.shore = shore.clamp(0.0, 2000.0),
                (W::RouteOnMap, Value::Toggle(on)) => route.on_map = on,
                _ => return false,
            }
        }
        _ => return false,
    }
    true
}
