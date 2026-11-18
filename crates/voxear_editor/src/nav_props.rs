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
    OpensDoors,
    Jumps,
    Climbs,
}

use NavField as F;

const ALL: [NavField; 14] = [
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
    F::OpensDoors,
    F::Jumps,
    F::Climbs,
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
        F::OpensDoors => "Opens Doors",
        F::Jumps => "Jumps",
        F::Climbs => "Climbs",
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
        F::Swims => {
            "Whether it swims across water too deep to stand in, and swims up in it, or goes round it and sinks if it \
             falls in. Default: on."
        }
        F::OpensDoors => "Whether it opens shut doors on its way and walks through, or goes round them. Default: on.",
        F::Jumps => "Whether it jumps gaps one or two blocks across on its way, or goes round them. Default: on.",
        F::Climbs => {
            "Whether it climbs vines, ropes, chains and ladders on its way, up and down, or goes round them. Default: on."
        }
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
        F::OpensDoors => "opens_doors",
        F::Jumps => "jumps",
        F::Climbs => "climbs",
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
        F::OpensDoors => Value::Toggle(agent.opens_doors),
        F::Jumps => Value::Toggle(agent.jumps),
        F::Climbs => Value::Toggle(agent.climbs),
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
        (F::OpensDoors, Value::Toggle(on)) => agent.opens_doors = *on,
        (F::Jumps, Value::Toggle(on)) => agent.jumps = *on,
        (F::Climbs, Value::Toggle(on)) => agent.climbs = *on,
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

/// A value the Nav Area component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavAreaField {
    Size,
    Area,
}

use voxear_engine::navigation::NavArea;
use voxear_terrain::blocks::block::BlockNavArea;

/// The areas a Nav Area may give, as its dropdown shows them: every one
/// but As It Is, which a box has no use for.
fn areas() -> &'static [BlockNavArea] {
    &BlockNavArea::ALL[1..]
}

pub fn area_props() -> Vec<Prop> {
    vec![Prop::NavArea(NavAreaField::Size), Prop::NavArea(NavAreaField::Area)]
}

pub fn area_label(field: NavAreaField) -> String {
    match field {
        NavAreaField::Size => "Size",
        NavAreaField::Area => "Area",
    }
    .into()
}

pub fn area_tooltip(field: NavAreaField) -> &'static str {
    match field {
        NavAreaField::Size => {
            "How wide, high and long the box is, in metres, centred on the Obj and square to the world. Every place \
             whose feet are in it is of its area. Default: 4, 2, 4."
        }
        NavAreaField::Area => {
            "The ground it makes, whatever is under it: a Road is sought out, Not Walkable is never stood on, and \
             the project's own areas cost what Project Settings ▸ Navigation says. Default: Road."
        }
    }
}

pub fn area_key(field: NavAreaField) -> &'static str {
    match field {
        NavAreaField::Size => "size",
        NavAreaField::Area => "area",
    }
}

pub fn area_get(game: &Game, entity: Entity, field: NavAreaField) -> Option<Value> {
    let area = game.engine.world.get::<NavArea>(entity)?;
    Some(match field {
        NavAreaField::Size => Value::Vector(area.size.to_vec()),
        NavAreaField::Area => Value::Choice(
            areas().iter().position(|kept| *kept == area.area).unwrap_or(0),
            areas().iter().map(|area| area.name().to_string()).collect(),
        ),
    })
}

pub fn area_set(game: &mut Game, entity: Entity, field: NavAreaField, value: Value) -> bool {
    let Some(area) = game.engine.world.get_mut::<NavArea>(entity) else { return false };
    match (field, value) {
        (NavAreaField::Size, Value::Vector(size)) if size.len() == 3 => {
            area.size = [0, 1, 2].map(|axis| size[axis].clamp(0.0, 10_000.0));
        }
        (NavAreaField::Area, Value::Choice(at, _)) => area.area = areas().get(at).copied().unwrap_or_default(),
        _ => return false,
    }
    true
}

pub fn area_add(world: &mut World, entity: Entity) {
    world.insert(entity, NavArea::default());
}

pub fn area_remove(world: &mut World, entity: Entity) {
    world.remove::<NavArea>(entity);
}

/// Adds the Nav Area part where `entity` has the component.
pub fn area_parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<NavArea>(entity) {
        parts.push(Part::NavArea);
    }
}

/// A value the Nav Link component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavLinkField {
    End,
    Kind,
    BothWays,
    Cost,
}

use voxear_engine::navigation::{NavLink, NavLinkKind};

const LINK_FIELDS: [NavLinkField; 4] =
    [NavLinkField::End, NavLinkField::Kind, NavLinkField::BothWays, NavLinkField::Cost];

pub fn link_props() -> Vec<Prop> {
    LINK_FIELDS.into_iter().map(Prop::NavLink).collect()
}

pub fn link_label(field: NavLinkField) -> String {
    match field {
        NavLinkField::End => "End",
        NavLinkField::Kind => "Kind",
        NavLinkField::BothWays => "Both Ways",
        NavLinkField::Cost => "Cost",
    }
    .into()
}

pub fn link_tooltip(field: NavLinkField) -> &'static str {
    match field {
        NavLinkField::End => {
            "Where the link leads, from its Obj, in the Obj's own space, so it moves and turns with it: the top of a \
             ladder, the far side of a gap. Things finding their way go from the place nearest the Obj to the place \
             nearest this. Default: 0, 0, 2."
        }
        NavLinkField::Kind => {
            "How what walks it crosses it: Walk straight along it, Jump over it in a leap, Climb it slowly as a \
             ladder, or Teleport from one end to the other at once. Default: Climb."
        }
        NavLinkField::BothWays => {
            "Whether it is taken back the other way too, or only from its Obj to its end. Default: on."
        }
        NavLinkField::Cost => {
            "What taking it costs more than walking as far, in metres of plain ground: more keeps things off it \
             where there is another way. Default: 1."
        }
    }
}

pub fn link_key(field: NavLinkField) -> &'static str {
    match field {
        NavLinkField::End => "end",
        NavLinkField::Kind => "kind",
        NavLinkField::BothWays => "both_ways",
        NavLinkField::Cost => "cost",
    }
}

pub fn link_get(game: &Game, entity: Entity, field: NavLinkField) -> Option<Value> {
    let link = game.engine.world.get::<NavLink>(entity)?;
    Some(match field {
        NavLinkField::End => Value::Vector(link.end.to_vec()),
        NavLinkField::Kind => Value::Choice(
            NavLinkKind::ALL.iter().position(|kept| *kept == link.kind).unwrap_or(0),
            NavLinkKind::ALL.iter().map(|kind| kind.name().to_string()).collect(),
        ),
        NavLinkField::BothWays => Value::Toggle(link.both_ways),
        NavLinkField::Cost => Value::Number(link.cost),
    })
}

pub fn link_set(game: &mut Game, entity: Entity, field: NavLinkField, value: Value) -> bool {
    let Some(link) = game.engine.world.get_mut::<NavLink>(entity) else { return false };
    match (field, value) {
        (NavLinkField::End, Value::Vector(end)) if end.len() == 3 => {
            link.end = [0, 1, 2].map(|axis| end[axis].clamp(-10_000.0, 10_000.0));
        }
        (NavLinkField::Kind, Value::Choice(at, _)) => link.kind = NavLinkKind::ALL.get(at).copied().unwrap_or_default(),
        (NavLinkField::BothWays, Value::Toggle(on)) => link.both_ways = on,
        (NavLinkField::Cost, Value::Number(cost)) => link.cost = cost.clamp(0.0, 10_000.0),
        _ => return false,
    }
    true
}

pub fn link_add(world: &mut World, entity: Entity) {
    world.insert(entity, NavLink::default());
}

pub fn link_remove(world: &mut World, entity: Entity) {
    world.remove::<NavLink>(entity);
}

/// Adds the Nav Link part where `entity` has the component.
pub fn link_parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<NavLink>(entity) {
        parts.push(Part::NavLink);
    }
}

/// A value the Nav Obstacle component shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavObstacleField {
    Size,
    Carve,
    MoveThreshold,
    StillTime,
}

use voxear_engine::nav_obstacle::NavObstacle;

const OBSTACLE_FIELDS: [NavObstacleField; 4] =
    [NavObstacleField::Size, NavObstacleField::Carve, NavObstacleField::MoveThreshold, NavObstacleField::StillTime];

pub fn obstacle_props() -> Vec<Prop> {
    OBSTACLE_FIELDS.into_iter().map(Prop::NavObstacle).collect()
}

pub fn obstacle_label(field: NavObstacleField) -> String {
    match field {
        NavObstacleField::Size => "Size",
        NavObstacleField::Carve => "Carve",
        NavObstacleField::MoveThreshold => "Move Threshold",
        NavObstacleField::StillTime => "Still Time",
    }
    .into()
}

pub fn obstacle_tooltip(field: NavObstacleField) -> &'static str {
    match field {
        NavObstacleField::Size => {
            "How wide, high and long the box agents keep off is, in metres, centred on the Obj and turned and \
             scaled with it. Default: 1, 1, 1."
        }
        NavObstacleField::Carve => {
            "Whether it is cut out of the nav where it stands still, so ways are found round it, or stop short where \
             it shuts the only one. Off, it is only gone round as agents go round each other. A carving obstacle's \
             own colliders are left out of a nav mesh bake. Default: on."
        }
        NavObstacleField::MoveThreshold => {
            "How far it moves, in metres, before it counts as moving: its cut is let go, and agents go round it as \
             it comes. Default: 0.1."
        }
        NavObstacleField::StillTime => {
            "How long it stands still, in seconds, before it carves where it stopped. Default: 0.5."
        }
    }
}

pub fn obstacle_key(field: NavObstacleField) -> &'static str {
    match field {
        NavObstacleField::Size => "size",
        NavObstacleField::Carve => "carve",
        NavObstacleField::MoveThreshold => "move_threshold",
        NavObstacleField::StillTime => "still_time",
    }
}

pub fn obstacle_get(game: &Game, entity: Entity, field: NavObstacleField) -> Option<Value> {
    let obstacle = game.engine.world.get::<NavObstacle>(entity)?;
    Some(match field {
        NavObstacleField::Size => Value::Vector(obstacle.size.to_vec()),
        NavObstacleField::Carve => Value::Toggle(obstacle.carve),
        NavObstacleField::MoveThreshold => Value::Number(obstacle.move_threshold),
        NavObstacleField::StillTime => Value::Number(obstacle.still_time),
    })
}

pub fn obstacle_set(game: &mut Game, entity: Entity, field: NavObstacleField, value: Value) -> bool {
    let Some(obstacle) = game.engine.world.get_mut::<NavObstacle>(entity) else { return false };
    match (field, value) {
        (NavObstacleField::Size, Value::Vector(size)) if size.len() == 3 => {
            obstacle.size = [0, 1, 2].map(|axis| size[axis].clamp(0.0, 10_000.0));
        }
        (NavObstacleField::Carve, Value::Toggle(on)) => obstacle.carve = on,
        (NavObstacleField::MoveThreshold, Value::Number(far)) => obstacle.move_threshold = far.clamp(0.0, 100.0),
        (NavObstacleField::StillTime, Value::Number(time)) => obstacle.still_time = time.clamp(0.0, 60.0),
        _ => return false,
    }
    true
}

pub fn obstacle_add(world: &mut World, entity: Entity) {
    world.insert(entity, NavObstacle::default());
}

pub fn obstacle_remove(world: &mut World, entity: Entity) {
    world.remove::<NavObstacle>(entity);
}

/// Adds the Nav Obstacle part where `entity` has the component.
pub fn obstacle_parts(world: &World, entity: Entity, parts: &mut Vec<Part>) {
    if world.has::<NavObstacle>(entity) {
        parts.push(Part::NavObstacle);
    }
}
