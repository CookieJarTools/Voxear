//! The floating origin: where the world's middle is, measured from the
//! level's own.
//!
//! An f32 position is exact to a step that grows with its distance from
//! the middle: a twentieth of a millimetre a kilometre out, a millimetre at
//! 10 km, 6 cm at 1000 km. So in a world without end the engine now and
//! then moves everything back towards the middle by whole metres, keeping
//! the camera near it ([`move_objs`] for the Objs; the engine moves physics,
//! sound and drawing with them). [`WorldOrigin`] keeps where the middle has
//! got to, so a position from the level's real middle is always its local
//! one plus the origin, in f64.

use std::collections::HashMap;

use voxear_core::math::{DVec3, I64Vec3, IVec3, Vec3};
use voxear_ecs::{Entity, World};

use crate::{Parent, Transform, WorldTransform};

/// Where the world's middle is from the level's real middle, in whole
/// metres: a position in the world plus this is the same position from the
/// level's middle. Zero until the world first moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldOrigin(pub I64Vec3);

impl WorldOrigin {
    /// The world's origin, zero in a world that has never moved.
    pub fn of(world: &World) -> Self {
        world.resource::<WorldOrigin>().copied().unwrap_or_default()
    }

    /// A position in the world, measured from the level's real middle.
    pub fn to_level(self, at: Vec3) -> DVec3 {
        at.as_dvec3() + self.0.as_dvec3()
    }

    /// A position from the level's real middle, in the world as it is now.
    pub fn from_level(self, at: DVec3) -> Vec3 {
        (at - self.0.as_dvec3()).as_vec3()
    }

    /// Whether the world has moved from the level's middle at all.
    pub fn is_moved(self) -> bool {
        self.0 != I64Vec3::ZERO
    }
}

/// Each root Obj's place from the level's middle as it was put at the last
/// move, and where that put it. One still there when the world moves again
/// is put from its place, exactly, not from its f32 position, which loses
/// its smallest steps each time it is moved far from the middle.
#[derive(Default)]
struct Places(HashMap<Entity, (DVec3, Vec3)>);

/// Moves every Obj by `by` whole metres, so the world's middle moves the
/// other way, and gives the new [`WorldOrigin`], kept in `world` too. Only
/// root Objs' transforms change, children being relative to them; each
/// `WorldTransform` moves with them, for what reads them before they are
/// worked out again.
pub fn move_objs(world: &mut World, by: IVec3) -> WorldOrigin {
    let was = WorldOrigin::of(world);
    let now = WorldOrigin(was.0 - by.as_i64vec3());
    let mut places = world.remove_resource::<Places>().unwrap_or_default();
    places.0.retain(|entity, _| world.is_alive(*entity));
    let roots: Vec<Entity> = world
        .query_ref::<(Entity, &Transform, Option<&Parent>)>()
        .into_iter()
        .filter(|(_, _, parent)| parent.is_none_or(|parent| !world.has::<Transform>(parent.0)))
        .map(|(entity, ..)| entity)
        .collect();
    for entity in roots {
        let Some(transform) = world.get_mut::<Transform>(entity) else { continue };
        let level = match places.0.get(&entity) {
            Some(&(level, put)) if put == transform.position => level,
            _ => was.to_level(transform.position),
        };
        transform.position = now.from_level(level);
        places.0.insert(entity, (level, transform.position));
    }
    let moved = by.as_vec3();
    for placed in world.query::<&mut WorldTransform>() {
        placed.0.w_axis += moved.extend(0.0);
    }
    world.insert_resource(places);
    world.insert_resource(now);
    now
}

/// Puts a root Obj at `level`, a position from the level's real middle,
/// wherever the world's middle has got to, as a scene loaded after the world
/// moved places its Objs. Its place is kept exactly for later moves.
pub fn place_at_level(world: &mut World, entity: Entity, level: DVec3) {
    let at = WorldOrigin::of(world).from_level(level);
    let Some(transform) = world.get_mut::<Transform>(entity) else { return };
    transform.position = at;
    match world.resource_mut::<Places>() {
        Some(places) => {
            places.0.insert(entity, (level, at));
        }
        None => world.insert_resource(Places(HashMap::from([(entity, (level, at))]))),
    }
}

#[cfg(test)]
mod tests {
    use voxear_core::math::{Quat, Vec3};

    use super::*;

    #[test]
    fn roots_move_and_children_follow() {
        let mut world = World::new();
        let root = world.spawn(Transform::new(Vec3::new(2100.0, 5.0, 0.0), Quat::IDENTITY, Vec3::ONE));
        let child = world.spawn((Transform::from_position(Vec3::X), Parent(root)));
        crate::propagate_transforms(&mut world);

        let origin = move_objs(&mut world, IVec3::new(-2048, 0, 0));
        assert_eq!(origin, WorldOrigin(I64Vec3::new(2048, 0, 0)));
        assert_eq!(world.get::<Transform>(root).unwrap().position, Vec3::new(52.0, 5.0, 0.0));
        assert_eq!(world.get::<Transform>(child).unwrap().position, Vec3::X);
        // Moved with the root, before the transforms are worked out again.
        assert_eq!(world.get::<WorldTransform>(child).unwrap().0.w_axis.truncate(), Vec3::new(53.0, 5.0, 0.0));
        assert_eq!(origin.to_level(Vec3::new(52.0, 5.0, 0.0)), DVec3::new(2100.0, 5.0, 0.0));
    }

    #[test]
    fn an_obj_left_far_behind_comes_back_exactly() {
        let mut world = World::new();
        let start = Vec3::new(0.3, 1.7, -0.9);
        let prop = world.spawn(Transform::from_position(start));
        // Out 100 km a step at a time, where its f32 position loses its smallest steps, and back.
        for _ in 0..100 {
            move_objs(&mut world, IVec3::new(-1024, 0, 1024));
        }
        assert!(world.get::<Transform>(prop).unwrap().position.x < -100_000.0);
        for _ in 0..100 {
            move_objs(&mut world, IVec3::new(1024, 0, -1024));
        }
        assert_eq!(world.get::<Transform>(prop).unwrap().position, start);
        assert_eq!(WorldOrigin::of(&world), WorldOrigin::default());
    }

    #[test]
    fn an_obj_moved_between_moves_keeps_where_it_was_moved_to() {
        let mut world = World::new();
        let walker = world.spawn(Transform::IDENTITY);
        move_objs(&mut world, IVec3::new(-1024, 0, 0));
        world.get_mut::<Transform>(walker).unwrap().position.x = 10.0;
        move_objs(&mut world, IVec3::new(1024, 0, 0));
        assert_eq!(world.get::<Transform>(walker).unwrap().position.x, 1034.0);
    }

    #[test]
    fn an_obj_placed_after_a_move_is_placed_from_the_level_middle() {
        let mut world = World::new();
        move_objs(&mut world, IVec3::new(-100 * 1024, 0, 0));
        let late = world.spawn(Transform::IDENTITY);
        place_at_level(&mut world, late, DVec3::new(102_400.25, 0.0, 0.0));
        assert_eq!(world.get::<Transform>(late).unwrap().position.x, 0.25);
        move_objs(&mut world, IVec3::new(100 * 1024, 0, 0));
        assert_eq!(world.get::<Transform>(late).unwrap().position.x, 102_400.25);
    }
}
