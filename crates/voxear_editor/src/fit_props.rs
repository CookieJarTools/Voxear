//! The Ground Fit component's fields in the Inspector: how it meets the
//! ground, its footprint's first box, the height it levels to, and the blocks
//! made ground is built of. More boxes and an outline are set from scripts,
//! `vox editor call` and the scene file.

use voxear_assets::AssetKind;
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    ground_fit::{FitBox, FitLevel, FitMode, GroundFit, MOST_SLOPE_OUT},
};

use crate::{
    fields::{self, Pick},
    props::{Part, Prop, Value},
};

/// A value the Ground Fit component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FitField {
    Mode,
    Size,
    Centre,
    Level,
    Point,
    Sink,
    Margin,
    SlopeOut,
    Fill,
    Top,
    ClearPlants,
    Priority,
}

use FitField as F;

const FIELDS: [FitField; 12] = [
    F::Mode,
    F::Size,
    F::Centre,
    F::Level,
    F::Point,
    F::Sink,
    F::Margin,
    F::SlopeOut,
    F::Fill,
    F::Top,
    F::ClearPlants,
    F::Priority,
];

pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let at_point = world.get::<GroundFit>(entity).is_some_and(|fit| fit.level == FitLevel::Point);
    FIELDS.into_iter().filter(|field| *field != F::Point || at_point).map(Prop::GroundFit).collect()
}

pub fn label(field: FitField) -> String {
    match field {
        F::Mode => "Mode",
        F::Size => "Size",
        F::Centre => "Centre",
        F::Level => "Level",
        F::Point => "Point",
        F::Sink => "Sink",
        F::Margin => "Margin",
        F::SlopeOut => "Slope Out",
        F::Fill => "Fill",
        F::Top => "Top",
        F::ClearPlants => "Clear Plants",
        F::Priority => "Priority",
    }
    .into()
}

pub fn tooltip(field: FitField) -> &'static str {
    match field {
        F::Mode => {
            "Foundation builds the ground up under it; Flatten builds it up and clears the hillside from over it; Cut \
             only clears. Default: Flatten."
        }
        F::Size => "Its footprint's size along x and z, in metres in the Obj's own space. Default: 4, 4.",
        F::Centre => "Where its footprint's middle is, from the Obj, in metres in its own space. Default: 0, 0.",
        F::Level => "The height it levels to: the Obj's own, or its Point's, such as its door. Default: Obj.",
        F::Point => "The point it levels to, in the Obj's own space: its door, so it meets the street there.",
        F::Sink => "How many blocks into the ground it stands. Default: 0.",
        F::Margin => "How far past its footprint the ground is levelled, in blocks. Default: 1.",
        F::SlopeOut => {
            "Over how many blocks the ground eases back to the generator's, a block a step, so it can be walked; 0 \
             for a sheer edge, as a retaining wall. Default: 4."
        }
        F::Fill => "What made ground is built of; None for its biome's own. Default: None.",
        F::Top => "What the levelled ground is topped with; None for its biome's own. Default: None.",
        F::ClearPlants => "Whether generated trees, plants and structures are left out of its reach. Default: on.",
        F::Priority => "Where two fits meet, the higher priority's ground wins. Default: 0.",
    }
}

pub fn key(field: FitField) -> &'static str {
    match field {
        F::Mode => "mode",
        F::Size | F::Centre => "boxes",
        F::Level => "level",
        F::Point => "point",
        F::Sink => "sink",
        F::Margin => "margin",
        F::SlopeOut => "slope_out",
        F::Fill => "fill",
        F::Top => "top",
        F::ClearPlants => "clear_plants",
        F::Priority => "priority",
    }
}

pub fn unit(field: FitField) -> Option<&'static str> {
    match field {
        F::Size | F::Centre | F::Point => Some("m"),
        F::Sink | F::Margin | F::SlopeOut => Some("blocks"),
        _ => None,
    }
}

pub fn range(field: FitField) -> Option<[f32; 2]> {
    match field {
        F::Sink => Some([-16.0, 16.0]),
        F::Margin => Some([0.0, 16.0]),
        F::SlopeOut => Some([0.0, MOST_SLOPE_OUT as f32]),
        F::Priority => Some([-100.0, 100.0]),
        _ => None,
    }
}

pub fn is_picked(field: FitField) -> bool {
    matches!(field, F::Fill | F::Top)
}

pub fn takes_name(_field: FitField) -> &'static str {
    "Block"
}

pub fn choices(game: &Game, field: FitField) -> Vec<(String, Pick)> {
    if !is_picked(field) {
        return Vec::new();
    }
    let mut found = vec![("None".to_string(), Pick::Asset(None))];
    found.extend(
        fields::assets_of(game, Some(AssetKind::Block)).into_iter().filter(|(_, pick)| *pick != Pick::Asset(None)),
    );
    found
}

/// Takes the block picked for its fill or top, or none.
pub fn pick(game: &mut Game, entity: Entity, field: FitField, choice: &Pick) -> bool {
    let Pick::Asset(path) = choice else { return false };
    let guid = match path {
        Some(path) => match game.content.parse(path).ok().and_then(|reference| reference.guid()) {
            Some(guid) => Some(guid),
            None => return false,
        },
        None => None,
    };
    let Some(fit) = game.engine.world.get_mut::<GroundFit>(entity) else { return false };
    match field {
        F::Fill => fit.fill = guid,
        F::Top => fit.top = guid,
        _ => return false,
    }
    true
}

fn first_box(fit: &GroundFit) -> FitBox {
    fit.boxes.first().copied().unwrap_or_default()
}

pub fn get(game: &Game, entity: Entity, field: FitField) -> Option<Value> {
    let fit = game.engine.world.get::<GroundFit>(entity)?;
    let block = |guid: Option<voxear_assets::AssetGuid>| {
        let path = guid.and_then(|guid| game.content.db().get(guid).map(|entry| entry.path.clone()));
        Value::Asset(path.unwrap_or_else(|| "None".into()))
    };
    Some(match field {
        F::Mode => Value::Choice(
            FitMode::ALL.iter().position(|kept| *kept == fit.mode).unwrap_or(0),
            FitMode::ALL.iter().map(|mode| mode.name().to_string()).collect(),
        ),
        F::Size => Value::Vector(first_box(fit).size.to_vec()),
        F::Centre => Value::Vector(first_box(fit).centre.to_vec()),
        F::Level => Value::Choice(
            FitLevel::ALL.iter().position(|kept| *kept == fit.level).unwrap_or(0),
            FitLevel::ALL.iter().map(|level| level.name().to_string()).collect(),
        ),
        F::Point => Value::Vector(fit.point.to_vec()),
        F::Sink => Value::Whole(i64::from(fit.sink)),
        F::Margin => Value::Number(fit.margin),
        F::SlopeOut => Value::Whole(i64::from(fit.slope_out)),
        F::Fill => block(fit.fill),
        F::Top => block(fit.top),
        F::ClearPlants => Value::Toggle(fit.clear_plants),
        F::Priority => Value::Whole(i64::from(fit.priority)),
    })
}

pub fn set(game: &mut Game, entity: Entity, field: FitField, value: Value) -> bool {
    let Some(fit) = game.engine.world.get_mut::<GroundFit>(entity) else { return false };
    let far = 10_000.0;
    let whole = |value: i64, [low, high]: [f32; 2]| value.clamp(low as i64, high as i64) as i32;
    match (field, value) {
        (F::Mode, Value::Choice(at, _)) => fit.mode = FitMode::ALL.get(at).copied().unwrap_or_default(),
        (F::Size | F::Centre, Value::Vector(two)) if two.len() == 2 => {
            if fit.boxes.is_empty() {
                fit.boxes.push(FitBox::default());
            }
            let first = &mut fit.boxes[0];
            match field {
                F::Size => first.size = [two[0].clamp(0.0, far), two[1].clamp(0.0, far)],
                _ => first.centre = [two[0].clamp(-far, far), two[1].clamp(-far, far)],
            }
        }
        (F::Level, Value::Choice(at, _)) => fit.level = FitLevel::ALL.get(at).copied().unwrap_or_default(),
        (F::Point, Value::Vector(point)) if point.len() == 3 => {
            fit.point = [0, 1, 2].map(|a| point[a].clamp(-far, far))
        }
        (F::Sink, Value::Whole(sink)) => fit.sink = whole(sink, [-16.0, 16.0]),
        (F::Margin, Value::Number(margin)) => fit.margin = margin.clamp(0.0, 64.0),
        (F::SlopeOut, Value::Whole(steps)) => fit.slope_out = whole(steps, [0.0, MOST_SLOPE_OUT as f32]) as u32,
        (F::ClearPlants, Value::Toggle(on)) => fit.clear_plants = on,
        (F::Priority, Value::Whole(priority)) => fit.priority = whole(priority, [-1000.0, 1000.0]),
        _ => return false,
    }
    true
}

/// Gives `entity` a Ground Fit whose box covers `bounds`, the model's along x
/// and z in its own space where it has one, else one 4 m square.
pub fn add(world: &mut World, entity: Entity, bounds: Option<([f32; 2], [f32; 2])>) {
    let boxes = match bounds {
        Some((low, high)) if high[0] > low[0] && high[1] > low[1] => {
            let centre = [(low[0] + high[0]) * 0.5, (low[1] + high[1]) * 0.5];
            vec![FitBox { centre, size: [high[0] - low[0], high[1] - low[1]] }]
        }
        _ => vec![FitBox::default()],
    };
    world.insert(entity, GroundFit { boxes, ..GroundFit::default() });
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<GroundFit>(entity);
}

/// Adds the Ground Fit part where `entity` has the component.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<GroundFit>(entity) {
        parts.push(Part::GroundFit);
    }
}

use voxear_engine::ground_streaming::{StreamsWithGround, WhenUnloaded};

pub fn streams_get(game: &Game, entity: Entity) -> Option<Value> {
    let streams = game.engine.world.get::<StreamsWithGround>(entity)?;
    Some(Value::Choice(
        WhenUnloaded::ALL.iter().position(|kept| *kept == streams.when_unloaded).unwrap_or(0),
        WhenUnloaded::ALL.iter().map(|when| when.name().to_string()).collect(),
    ))
}

pub fn streams_set(game: &mut Game, entity: Entity, value: Value) -> bool {
    let (Some(streams), Value::Choice(at, _)) = (game.engine.world.get_mut::<StreamsWithGround>(entity), value) else {
        return false;
    };
    streams.when_unloaded = WhenUnloaded::ALL.get(at).copied().unwrap_or_default();
    true
}

pub const STREAMS_TOOLTIP: &str = "What happens to the Obj when the ground under it streams out of the world of \
     blocks while the game plays: Keep For Later takes it away and makes it again as it was when its ground comes \
     back; Destroy takes it away for good, as a spawned mob; Stay leaves it. Default: Keep For Later.";

/// Adds the Streams With Ground part where `entity` has the component.
pub fn streams_parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<StreamsWithGround>(entity) {
        parts.push(Part::StreamsWithGround);
    }
}
