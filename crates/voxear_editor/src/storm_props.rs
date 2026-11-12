//! The level's Storms in the Inspector: whether storms come by themselves
//! and how often, and how its sea is drawn in rough weather (see
//! `voxear_engine::storms`).

use voxear_ecs::{Entity, World};
use voxear_engine::{Game, storms::StormRules};
use voxear_render::sea::SeaLook;

use crate::props::{Part, Prop, Value};

/// A value the Storms component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StormField {
    Natural,
    Frequency,
    Largest,
    Standing,
    Seed,
    SeaLook,
    Swell,
    Whirlpools,
    Maelstroms,
    Glow,
}

use StormField as F;

const ALL: [StormField; 10] = [
    F::Natural,
    F::Frequency,
    F::Largest,
    F::Standing,
    F::Seed,
    F::SeaLook,
    F::Swell,
    F::Whirlpools,
    F::Maelstroms,
    F::Glow,
];

/// The values the component shows, in order: how often and how large only
/// with natural storms on.
pub fn props(world: &World, entity: Entity) -> Vec<Prop> {
    let natural = world.get::<StormRules>(entity).is_some_and(|rules| rules.natural);
    let shown = |field: &StormField| natural || !matches!(field, F::Frequency | F::Largest | F::Standing | F::Seed);
    ALL.into_iter().filter(shown).map(Prop::Storm).collect()
}

/// What a value's row says.
pub fn label(field: StormField) -> String {
    let label = match field {
        F::Natural => "Natural Storms",
        F::Frequency => "Frequency",
        F::Largest => "Largest",
        F::Standing => "Standing Storms",
        F::Seed => "Seed",
        F::SeaLook => "Sea Look",
        F::Swell => "Swell Height",
        F::Whirlpools => "Storm Whirlpools",
        F::Maelstroms => "Maelstroms",
        F::Glow => "Night Glow",
    };
    label.into()
}

/// The value as a prefab's locks name it.
pub fn key(field: StormField) -> &'static str {
    match field {
        F::Natural => "natural",
        F::Frequency => "frequency",
        F::Largest => "largest",
        F::Standing => "standing",
        F::Seed => "seed",
        F::SeaLook => "sea_look",
        F::Swell => "swell",
        F::Whirlpools => "whirlpools",
        F::Maelstroms => "maelstroms",
        F::Glow => "glow",
    }
}

pub fn unit(field: StormField) -> Option<&'static str> {
    matches!(field, F::Largest | F::Swell).then_some("m")
}

/// The range a number is dragged within.
pub fn range(field: StormField) -> Option<[f32; 2]> {
    match field {
        F::Frequency => Some([0.0, 1.0]),
        F::Largest => Some([250.0, 4000.0]),
        F::Swell => Some([0.0, 8.0]),
        F::Glow => Some([0.0, 2.0]),
        _ => None,
    }
}

pub fn get(game: &Game, entity: Entity, field: StormField) -> Option<Value> {
    let rules = game.engine.world.get::<StormRules>(entity)?;
    Some(match field {
        F::Natural => Value::Toggle(rules.natural),
        F::Frequency => Value::Number(rules.frequency),
        F::Largest => Value::Number(rules.largest),
        F::Standing => Value::Toggle(rules.standing),
        F::Seed => Value::Whole(i64::from(rules.seed)),
        F::SeaLook => {
            Value::Choice(rules.sea_look.index(), SeaLook::ALL.iter().map(|look| look.name().to_string()).collect())
        }
        F::Swell => Value::Number(rules.swell),
        F::Whirlpools => Value::Toggle(rules.whirlpools),
        F::Maelstroms => Value::Toggle(rules.maelstroms),
        F::Glow => Value::Number(rules.glow),
    })
}

pub fn set(game: &mut Game, entity: Entity, field: StormField, value: Value) -> bool {
    let Some(rules) = game.engine.world.get_mut::<StormRules>(entity) else { return false };
    let number = match value {
        Value::Number(n) => Some(n),
        Value::Whole(n) => Some(n as f32),
        _ => None,
    };
    match (field, &value) {
        (F::Natural, Value::Toggle(on)) => rules.natural = *on,
        (F::Standing, Value::Toggle(on)) => rules.standing = *on,
        (F::Whirlpools, Value::Toggle(on)) => rules.whirlpools = *on,
        (F::Maelstroms, Value::Toggle(on)) => rules.maelstroms = *on,
        (F::SeaLook, Value::Choice(at, _)) => rules.sea_look = SeaLook::ALL.get(*at).copied().unwrap_or_default(),
        (F::Seed, Value::Whole(seed)) => rules.seed = (*seed).clamp(0, i64::from(u32::MAX)) as u32,
        (F::Frequency, _) => rules.frequency = number.unwrap_or(rules.frequency).clamp(0.0, 1.0),
        (F::Largest, _) => rules.largest = number.unwrap_or(rules.largest).clamp(250.0, 20_000.0),
        (F::Swell, _) => rules.swell = number.unwrap_or(rules.swell).clamp(0.0, 16.0),
        (F::Glow, _) => rules.glow = number.unwrap_or(rules.glow).clamp(0.0, 4.0),
        _ => return false,
    }
    true
}

pub fn add(world: &mut World, entity: Entity) {
    world.insert(entity, StormRules::default());
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<StormRules>(entity);
}

/// Adds the Storms part where `entity` has the component.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<StormRules>(entity) {
        parts.push(Part::Storms);
    }
}
