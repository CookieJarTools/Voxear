//! Where an Obj is in the hierarchy, for scripts (`Obj.Parent`,
//! `Obj.SetParent` and `Obj.Children` in Voxear.Core): the Obj it is
//! inside, and the Objs directly inside it, in the order the engine's own
//! systems go through them, by their entities' numbers.
//!
//! [`set_parent`] puts an Obj inside another, or at the top, while the game
//! runs, as a boat takes its passengers aboard: kept where it is in the
//! world, or where it is from its parent, with its physics, smoothing
//! between ticks and dimension brought along, and refused where it would
//! go inside itself, into another dimension or into another scene.

use std::collections::HashMap;
use std::fmt;

use voxear_core::math::{Mat4, Quat};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Name, Parent, Transform};
use voxear_scripting::ComponentKind;

use crate::cameras::obj_value;
use crate::levels::Levels;
use crate::physics::PhysicsScene;

// What `values` reads, in the low byte of its number, as `ObjHierarchy` in
// Voxear.Core numbers them; which child is the number's higher bytes.
const PARENT: i32 = 0;
const CHILD_COUNT: i32 = 1;
const CHILD: i32 = 2;
// What `texts` is asked: to set the parent, from the parent's entity and
// whether to keep the Obj where it is in the world, as words.
const SET_PARENT: i32 = 3;

/// As deep as any hierarchy goes; deeper is a loop.
const MAX_DEPTH: usize = 256;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Hierarchy, values);
    voxear_scripting::set_kind_texts(ComponentKind::Hierarchy, texts);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if written.is_some() || !world.is_alive(entity) {
        return None;
    }
    match what & 0xff {
        PARENT => Some(obj_value(world.get::<Parent>(entity).map(|parent| parent.0))),
        CHILD_COUNT => Some([children(world, entity).len() as f32, 0.0, 0.0, 0.0]),
        CHILD => children(world, entity).get((what >> 8) as usize).map(|&child| obj_value(Some(child))),
        _ => None,
    }
}

/// `SET_PARENT` with "<parent's entity> <1 to keep where it is in the
/// world, or 0>", 0 for none: nothing back when it went, or why not.
fn texts(world: &mut World, entity: Entity, what: i32, text: &str) -> Option<String> {
    if what != SET_PARENT {
        return None;
    }
    let mut words = text.split_whitespace();
    let parent = words.next()?.parse::<u64>().ok()?;
    let parent = (parent != 0).then(|| Entity::from_bits(parent));
    let keep_world = words.next() != Some("0");
    let done = match voxear_scripting::set_parent(world, entity, parent, keep_world) {
        Some(done) => done,
        None => set_parent(world, None, entity, parent, keep_world).map_err(|error| error.to_string()),
    };
    Some(done.err().unwrap_or_default())
}

/// The Objs whose parent is `entity`, by their entities' numbers.
pub(crate) fn children(world: &World, entity: Entity) -> Vec<Entity> {
    let mut children: Vec<Entity> = world
        .query_ref::<(Entity, &Parent)>()
        .into_iter()
        .filter(|(_, parent)| parent.0 == entity)
        .map(|(child, _)| child)
        .collect();
    children.sort_by_key(|child| child.index());
    children
}

/// An Obj and every Obj under it, the Obj first.
pub fn family(world: &World, entity: Entity) -> Vec<Entity> {
    let mut inside: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (child, parent) in world.query_ref::<(Entity, &Parent)>() {
        inside.entry(parent.0).or_default().push(child);
    }
    let mut family = vec![entity];
    let mut at = 0;
    while at < family.len() {
        if let Some(children) = inside.get(&family[at]) {
            let new: Vec<Entity> = children.iter().copied().filter(|child| !family.contains(child)).collect();
            family.extend(new);
        }
        at += 1;
    }
    family
}

/// An Obj put inside another, or at the top, while the game ran, rather
/// than where its scene or prefab put it: a save of the scene keeps where
/// it went.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reparented;
impl Component for Reparented {}

/// Why an Obj could not go inside another, by their names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParentError {
    /// It, or the Obj it was to go inside, is gone or being destroyed.
    Gone,
    /// The Obj it was to go inside is itself, or inside it.
    InsideItself { obj: String, parent: String },
    /// The two are in different dimensions.
    OtherDimension { obj: String, parent: String },
    /// It, or an Obj under it, was loaded by one scene and would go into
    /// another: by their paths.
    OtherScene { obj: String, scene: String, into: String },
}

impl fmt::Display for ParentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gone => write!(f, "the Obj, or the one it was to go inside, is gone"),
            Self::InsideItself { obj, parent } if obj == parent => write!(f, "{obj} cannot go inside itself"),
            Self::InsideItself { obj, parent } => {
                write!(f, "{obj} cannot go inside {parent}, which is inside it")
            }
            Self::OtherDimension { obj, parent } => {
                write!(f, "{obj} cannot go inside {parent}, which is in another dimension; move it there first")
            }
            Self::OtherScene { obj, scene, into } => write!(
                f,
                "{obj} cannot go into {into}: an Obj a scene loaded stays in its scene, and this one is in {scene}"
            ),
        }
    }
}

impl std::error::Error for ParentError {}

/// Puts an Obj, and everything under it, inside `parent`, or at the top of
/// its scene with none, while the game runs. With `keep_world` it stays
/// where it is in the world, its Transform worked out again from its new
/// parent, as a passenger taken aboard does, and physics and the smoothing
/// between ticks see no move; without, it keeps its Transform, so moves
/// with its new parent, set down there at once as through a Portal, its
/// bodies with no speed from the jump. It stays in its dimension. True when
/// its parent changed; refused when `parent` is it or inside it, or in
/// another dimension, or, given `levels`, when it or an Obj under it was
/// loaded by another scene than `parent`'s.
pub fn set_parent(
    world: &mut World,
    levels: Option<&Levels>,
    entity: Entity,
    parent: Option<Entity>,
    keep_world: bool,
) -> Result<bool, ParentError> {
    let gone = |obj: Entity| !world.is_alive(obj) || world.has::<Destroying>(obj);
    if gone(entity) || parent.is_some_and(gone) {
        return Err(ParentError::Gone);
    }
    let name =
        |obj: Entity| world.get::<Name>(obj).map_or_else(|| "an Obj".to_string(), |name| format!("\"{}\"", name.0));
    if let Some(parent) = parent {
        let mut at = Some(parent);
        for _ in 0..MAX_DEPTH {
            let Some(obj) = at else { break };
            if obj == entity {
                return Err(ParentError::InsideItself { obj: name(entity), parent: name(parent) });
            }
            at = world.get::<Parent>(obj).map(|parent| parent.0);
        }
        if voxear_scene::dimension_of(world, entity) != voxear_scene::dimension_of(world, parent) {
            return Err(ParentError::OtherDimension { obj: name(entity), parent: name(parent) });
        }
    }
    let before = world.get::<Parent>(entity).map(|parent| parent.0);
    if before == parent {
        return Ok(false);
    }
    let family = family(world, entity);
    if let Some((scene, into)) = levels.and_then(|levels| levels.parent_refused(world, &family, parent)) {
        return Err(ParentError::OtherScene { obj: name(entity), scene, into });
    }

    let dimension = voxear_scene::dimension_of(world, entity);
    let placed = voxear_scene::world_matrix(world, entity);
    let inside =
        |world: &World, parent: Option<Entity>| parent.map_or(Mat4::IDENTITY, |p| voxear_scene::world_matrix(world, p));
    let (was_inside, now_inside) = (inside(world, before), inside(world, parent));
    match parent {
        Some(parent) => drop(world.insert(entity, Parent(parent))),
        None => drop(world.remove::<Parent>(entity)),
    }
    voxear_scene::put_in_dimension(world, entity, dimension);
    world.insert(entity, Reparented);
    if keep_world {
        let (scale, rotation, position) = (now_inside.inverse() * placed).to_scale_rotation_translation();
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            // A scale worked out again is a hair off; one that has not changed is kept as it was.
            transform.scale = if scale.abs_diff_eq(transform.scale, 1e-5) { transform.scale } else { scale };
            (transform.position, transform.rotation) = (position, rotation.normalize());
        }
        crate::tick_smoothing::reparented(world, entity, now_inside.inverse() * was_inside);
    } else if let Some(mut physics) = world.remove_resource::<PhysicsScene>() {
        for &obj in &family {
            physics.teleported(world, obj, Quat::IDENTITY);
        }
        world.insert_resource(physics);
    }
    crate::net::reparented(world, entity, parent);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cameras::value_obj;

    use voxear_core::math::{Quat, Vec3};
    use voxear_scene::{DimensionId, InDimension};

    fn placed(world: &World, obj: Entity) -> Mat4 {
        voxear_scene::world_matrix(world, obj)
    }

    #[test]
    fn an_obj_put_inside_another_stays_where_it_is_in_the_world_or_keeps_its_own_place() {
        let mut world = World::new();
        let boat = world.spawn((Transform::new(Vec3::new(10.0, 2.0, -4.0), Quat::from_rotation_y(0.7), Vec3::ONE),));
        let deck = world.spawn((
            Transform::new(Vec3::new(0.0, 1.0, 2.0), Quat::from_rotation_x(0.2), Vec3::splat(2.0)),
            Parent(boat),
        ));
        let rider = world.spawn((Transform::new(Vec3::new(3.0, 0.5, 1.0), Quat::from_rotation_y(-1.1), Vec3::ONE),));
        let hat = world.spawn((Transform::from_position(Vec3::Y), Parent(rider)));
        let (was, hat_was) = (placed(&world, rider), placed(&world, hat));

        assert_eq!(set_parent(&mut world, None, rider, Some(deck), true), Ok(true));
        assert_eq!(world.get::<Parent>(rider), Some(&Parent(deck)));
        assert!(placed(&world, rider).abs_diff_eq(was, 1e-4), "where it was in the world");
        assert!(placed(&world, hat).abs_diff_eq(hat_was, 1e-4), "and what is inside it with it");
        let scale = world.get::<Transform>(rider).unwrap().scale;
        assert!(scale.abs_diff_eq(Vec3::splat(0.5), 1e-5), "its scale from the deck's: {scale}");
        assert_eq!(set_parent(&mut world, None, rider, Some(deck), true), Ok(false), "already there");

        // Let off at the top, it is still where it was; put in again keeping its own place, it moves with the deck.
        assert_eq!(set_parent(&mut world, None, rider, None, true), Ok(true));
        assert!(!world.has::<Parent>(rider) && placed(&world, rider).abs_diff_eq(was, 1e-4));
        let own = *world.get::<Transform>(rider).unwrap();
        assert_eq!(set_parent(&mut world, None, rider, Some(deck), false), Ok(true));
        assert_eq!(*world.get::<Transform>(rider).unwrap(), own, "its Transform as it was");
        assert!(placed(&world, rider).abs_diff_eq(placed(&world, deck) * own.matrix(), 1e-4));
        assert!(world.has::<Reparented>(rider), "for a save to keep");
    }

    #[test]
    fn an_obj_cannot_go_inside_itself_another_dimension_or_something_gone() {
        let mut world = World::new();
        let top = world.spawn((Transform::default(), Name("Top".into())));
        let middle = world.spawn((Transform::default(), Parent(top), Name("Middle".into())));
        let bottom = world.spawn((Transform::default(), Parent(middle)));
        let refused = set_parent(&mut world, None, top, Some(bottom), true);
        assert_eq!(refused, Err(ParentError::InsideItself { obj: "\"Top\"".into(), parent: "an Obj".into() }));
        assert_eq!(refused.unwrap_err().to_string(), "\"Top\" cannot go inside an Obj, which is inside it");
        assert!(matches!(
            set_parent(&mut world, None, middle, Some(middle), true),
            Err(ParentError::InsideItself { .. })
        ));
        assert_eq!(world.get::<Parent>(top), None, "nothing changed");

        let elsewhere = world.spawn((Transform::default(), InDimension(DimensionId(3))));
        let error = set_parent(&mut world, None, bottom, Some(elsewhere), true).unwrap_err();
        assert!(matches!(error, ParentError::OtherDimension { .. }), "{error}");

        let gone = world.spawn((Transform::default(),));
        world.despawn(gone);
        assert_eq!(set_parent(&mut world, None, bottom, Some(gone), true), Err(ParentError::Gone));
        world.insert(middle, Destroying);
        assert_eq!(set_parent(&mut world, None, middle, None, true), Err(ParentError::Gone), "one being destroyed");
    }

    #[test]
    fn an_obj_taken_out_of_another_dimensions_obj_stays_in_that_dimension() {
        let mut world = World::new();
        let dungeon = DimensionId(2);
        let cart = world.spawn((Transform::default(), InDimension(dungeon)));
        let miner = world.spawn((Transform::from_position(Vec3::X), Parent(cart)));
        assert_eq!(set_parent(&mut world, None, miner, None, true), Ok(true));
        assert_eq!(voxear_scene::dimension_of(&world, miner), dungeon);
        assert_eq!(set_parent(&mut world, None, miner, Some(cart), true), Ok(true));
        assert_eq!(world.get::<InDimension>(miner), None, "its cart's dimension is its own again");
    }

    #[test]
    fn an_obj_shown_between_ticks_moves_on_evenly_into_its_new_parent() {
        let mut world = World::new();
        let boat = world.spawn((Transform::from_position(Vec3::new(5.0, 0.0, 0.0)),));
        let rider = world.spawn((Transform::from_position(Vec3::ZERO),));
        crate::tick_smoothing::begin_ticks(&mut world);
        crate::tick_smoothing::before_tick(&mut world);
        world.get_mut::<Transform>(rider).unwrap().position = Vec3::new(1.0, 0.0, 0.0);
        crate::tick_smoothing::end_ticks(&mut world, true, 0.5);
        crate::tick_smoothing::begin_ticks(&mut world);
        crate::tick_smoothing::before_tick(&mut world);
        world.get_mut::<Transform>(rider).unwrap().position = Vec3::new(2.0, 0.0, 0.0);
        // Taken aboard in the tick, kept where it is.
        set_parent(&mut world, None, rider, Some(boat), true).unwrap();
        crate::tick_smoothing::end_ticks(&mut world, true, 0.5);
        let shown = placed(&world, rider).w_axis.truncate();
        assert!(shown.abs_diff_eq(Vec3::new(1.5, 0.0, 0.0), 1e-4), "half-way between its ticks in the world: {shown}");
    }

    #[test]
    fn an_objs_parent_and_children_are_read_in_the_engines_order() {
        let mut world = World::new();
        let top = world.spawn((Transform::default(),));
        let first = world.spawn((Transform::default(), Parent(top)));
        let second = world.spawn((Transform::default(), Parent(top)));
        let inner = world.spawn((Transform::default(), Parent(second)));
        let parent = |world: &mut World, of| value_obj(values(world, of, PARENT, None).unwrap());
        assert_eq!(parent(&mut world, top), None, "the top is inside nothing");
        assert_eq!(parent(&mut world, inner), Some(second));
        assert_eq!(values(&mut world, top, CHILD_COUNT, None), Some([2.0, 0.0, 0.0, 0.0]), "not the one further in");
        let child = |world: &mut World, at: i32| values(world, top, at << 8 | CHILD, None).and_then(value_obj);
        assert_eq!((child(&mut world, 0), child(&mut world, 1)), (Some(first), Some(second)));
        assert_eq!(child(&mut world, 2), None, "past the last");
        assert_eq!(values(&mut world, top, PARENT, Some([0.0; 4])), None, "nothing is set through it");
    }
}
