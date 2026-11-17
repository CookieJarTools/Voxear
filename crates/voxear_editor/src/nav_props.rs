//! An Obj's Nav Agent in the Inspector: how fast it walks, turns and
//! stops, its size, how it goes round others and how it moves (see
//! `voxear_engine::nav_agent`), each row with a tooltip.

use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    nav_agent::{NavAgent, NavAgentState, NavAvoidance, NavMoves},
};

use crate::props::{Part, Prop, Value};

/// A value the Nav Agent component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavField {
    Speed,
    Acceleration,
    TurnSpeed,
    StoppingDistance,
    AutoBrake,
    Radius,
    Height,
    Avoidance,
    Priority,
    Moves,
    Swims,
}

use NavField as F;

const ALL: [NavField; 11] = [
    F::Speed,
    F::Acceleration,
    F::TurnSpeed,
    F::StoppingDistance,
    F::AutoBrake,
    F::Radius,
    F::Height,
    F::Avoidance,
    F::Priority,
    F::Moves,
    F::Swims,
];

const MOVES: [NavMoves; 3] = [NavMoves::CharacterController, NavMoves::AlongTheNav, NavMoves::NotAtAll];
const AVOIDANCE: [NavAvoidance; 3] = [NavAvoidance::None, NavAvoidance::Low, NavAvoidance::High];

/// The values the component shows, in order.
pub fn props(_: &World, _: Entity) -> Vec<Prop> {
    ALL.into_iter().map(Prop::Nav).collect()
}

/// What a value's row says.
pub fn label(field: NavField) -> String {
    let label = match field {
        F::Speed => "Speed",
        F::Acceleration => "Acceleration",
        F::TurnSpeed => "Turn Speed",
        F::StoppingDistance => "Stopping Distance",
        F::AutoBrake => "Auto Brake",
        F::Radius => "Radius",
        F::Height => "Height",
        F::Avoidance => "Avoidance",
        F::Priority => "Priority",
        F::Moves => "Moves",
        F::Swims => "Swims",
    };
    label.into()
}

/// What a value's row tells on hover: what it does, and what more or less
/// of it changes.
pub fn tooltip(field: NavField) -> &'static str {
    match field {
        F::Speed => "How fast it walks at most, in metres a second. Default: 3.5.",
        F::Acceleration => "How quickly it speeds up and slows down, in metres a second each second. Default: 8.",
        F::TurnSpeed => "How fast it turns to face the way it goes, in degrees a second. Default: 360.",
        F::StoppingDistance => "How near its destination it stops, in metres. Default: 0.",
        F::AutoBrake => {
            "Whether it slows down to stop at its destination, or arrives at speed and stops there. Default: on."
        }
        F::Radius => "How wide it is round its middle, in metres: how far other agents keep from it. Default: 0.35.",
        F::Height => {
            "How tall it is, in metres: what it fits under. With a Character Controller, the controller's height is \
             used. Default: 1.8."
        }
        F::Avoidance => {
            "How carefully it goes round other agents and characters. High heeds the ten nearest, Low the four \
             nearest, None none: others still go round it. Default: High."
        }
        F::Priority => {
            "From 0 to 1: the higher, the more others go round it and the less it goes round them. Default: 0.5."
        }
        F::Moves => {
            "How it moves along its way. Character Controller collides with everything; Along the Nav costs far \
             less, for crowds; Not at All leaves its Desired Velocity for a script to move it by. Default: Character \
             Controller."
        }
        F::Swims => "Whether it swims up in water too deep to stand in, or sinks and walks the bottom. Default: on.",
    }
}

/// The value as a prefab's locks name it.
pub fn key(field: NavField) -> &'static str {
    match field {
        F::Speed => "speed",
        F::Acceleration => "acceleration",
        F::TurnSpeed => "turn_speed",
        F::StoppingDistance => "stopping_distance",
        F::AutoBrake => "auto_brake",
        F::Radius => "radius",
        F::Height => "height",
        F::Avoidance => "avoidance",
        F::Priority => "priority",
        F::Moves => "moves",
        F::Swims => "swims",
    }
}

pub fn unit(field: NavField) -> Option<&'static str> {
    match field {
        F::Speed => Some("m/s"),
        F::Acceleration => Some("m/s²"),
        F::TurnSpeed => Some("°/s"),
        F::StoppingDistance | F::Radius | F::Height => Some("m"),
        _ => None,
    }
}

/// The range a number is dragged within.
pub fn range(field: NavField) -> Option<[f32; 2]> {
    match field {
        F::Speed => Some([0.0, 20.0]),
        F::Acceleration => Some([0.0, 50.0]),
        F::TurnSpeed => Some([0.0, 1440.0]),
        F::StoppingDistance => Some([0.0, 10.0]),
        F::Radius => Some([0.05, 4.0]),
        F::Height => Some([0.1, 8.0]),
        F::Priority => Some([0.0, 1.0]),
        _ => None,
    }
}

fn moves_name(moves: NavMoves) -> &'static str {
    match moves {
        NavMoves::CharacterController => "Character Controller",
        NavMoves::AlongTheNav => "Along the Nav",
        NavMoves::NotAtAll => "Not at All",
    }
}

fn avoidance_name(avoidance: NavAvoidance) -> &'static str {
    match avoidance {
        NavAvoidance::None => "None",
        NavAvoidance::Low => "Low",
        NavAvoidance::High => "High",
    }
}

pub fn get(game: &Game, entity: Entity, field: NavField) -> Option<Value> {
    let agent = game.engine.world.get::<NavAgent>(entity)?;
    Some(match field {
        F::Speed => Value::Number(agent.speed),
        F::Acceleration => Value::Number(agent.acceleration),
        F::TurnSpeed => Value::Number(agent.turn_speed),
        F::StoppingDistance => Value::Number(agent.stopping_distance),
        F::AutoBrake => Value::Toggle(agent.auto_brake),
        F::Radius => Value::Number(agent.radius),
        F::Height => Value::Number(agent.height),
        F::Avoidance => Value::Choice(
            AVOIDANCE.iter().position(|kept| *kept == agent.avoidance).unwrap_or(2),
            AVOIDANCE.iter().map(|avoidance| avoidance_name(*avoidance).to_string()).collect(),
        ),
        F::Priority => Value::Number(agent.priority),
        F::Moves => Value::Choice(
            MOVES.iter().position(|kept| *kept == agent.moves).unwrap_or(0),
            MOVES.iter().map(|moves| moves_name(*moves).to_string()).collect(),
        ),
        F::Swims => Value::Toggle(agent.swims),
    })
}

pub fn set(game: &mut Game, entity: Entity, field: NavField, value: Value) -> bool {
    let Some(agent) = game.engine.world.get_mut::<NavAgent>(entity) else { return false };
    let number = match value {
        Value::Number(n) => Some(n),
        Value::Whole(n) => Some(n as f32),
        _ => None,
    };
    match (field, &value) {
        (F::AutoBrake, Value::Toggle(on)) => agent.auto_brake = *on,
        (F::Swims, Value::Toggle(on)) => agent.swims = *on,
        (F::Moves, Value::Choice(at, _)) => agent.moves = MOVES.get(*at).copied().unwrap_or_default(),
        (F::Avoidance, Value::Choice(at, _)) => agent.avoidance = AVOIDANCE.get(*at).copied().unwrap_or_default(),
        (F::Speed, _) => agent.speed = number.unwrap_or(agent.speed).clamp(0.0, 100.0),
        (F::Acceleration, _) => agent.acceleration = number.unwrap_or(agent.acceleration).clamp(0.0, 1000.0),
        (F::TurnSpeed, _) => agent.turn_speed = number.unwrap_or(agent.turn_speed).clamp(0.0, 3600.0),
        (F::StoppingDistance, _) => {
            agent.stopping_distance = number.unwrap_or(agent.stopping_distance).clamp(0.0, 1000.0)
        }
        (F::Radius, _) => agent.radius = number.unwrap_or(agent.radius).clamp(0.01, 50.0),
        (F::Height, _) => agent.height = number.unwrap_or(agent.height).clamp(0.05, 100.0),
        (F::Priority, _) => agent.priority = number.unwrap_or(agent.priority).clamp(0.0, 1.0),
        _ => return false,
    }
    true
}

pub fn add(world: &mut World, entity: Entity) {
    world.insert(entity, NavAgent::default());
}

pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<NavAgent>(entity);
    world.remove::<NavAgentState>(entity);
}

/// Adds the Nav Agent part where `entity` has the component.
pub fn parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<NavAgent>(entity) {
        parts.push(Part::NavAgent);
    }
}
