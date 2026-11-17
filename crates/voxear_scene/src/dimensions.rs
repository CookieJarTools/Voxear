//! Which dimension each Obj is in. A dimension is a world of its own in the
//! one game: its own Block Worlds, sky and weather, at coordinates of its own
//! that may be the same as another's. Nothing in one dimension is drawn,
//! heard, collided with or streamed round in another.
//!
//! An Obj is in the dimension its root is in: an [`InDimension`] on the root,
//! or none for the level's own dimension, [`DimensionId::LEVEL`]. So every
//! Obj made before dimensions is in the level's, and nothing is written for
//! it.

use std::collections::HashMap;

use voxear_ecs::{Component, Entity, World};

use crate::hierarchy::Parent;

/// A dimension the game has made, by its number. The level's own is 0; the
/// others are numbered as they are made and never saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DimensionId(pub u16);

impl DimensionId {
    /// The level's own dimension: its scenes' Objs, and every Obj with no
    /// [`InDimension`] over it.
    pub const LEVEL: Self = Self(0);
    /// How many dimensions there can be at once, the level's counted. Physics
    /// keeps each dimension's layers apart in its object layers, which hold
    /// this many sets of 32.
    pub const MOST: u16 = 1024;

    pub fn is_level(self) -> bool {
        self == Self::LEVEL
    }
}

/// The dimension a world is seen from: a resource the engine keeps as it
/// follows the game's camera, for what is worked out once a frame for every
/// view, as reflection probes are. Absent, the level's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CurrentDimension(pub DimensionId);

impl CurrentDimension {
    pub fn of(world: &World) -> DimensionId {
        world.resource::<Self>().map_or(DimensionId::LEVEL, |current| current.0)
    }
}

/// On an Obj, most often a root: the dimension it, and everything under it,
/// is in. An Obj without one takes its parent's; a root without one is in
/// the level's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InDimension(pub DimensionId);
impl Component for InDimension {}

/// As deep as any hierarchy goes, and no further should one loop.
const MAX_DEPTH: usize = 64;

/// The dimension an Obj is in: its root's.
pub fn dimension_of(world: &World, entity: Entity) -> DimensionId {
    let mut at = entity;
    for _ in 0..MAX_DEPTH {
        if let Some(own) = world.get::<InDimension>(at) {
            return own.0;
        }
        match world.get::<Parent>(at) {
            Some(parent) => at = parent.0,
            None => return DimensionId::LEVEL,
        }
    }
    DimensionId::LEVEL
}

/// Puts a root Obj, and so everything under it, in a dimension. The level's
/// leaves it with no [`InDimension`], as an Obj made before dimensions has.
pub fn set_dimension(world: &mut World, root: Entity, dimension: DimensionId) {
    match dimension.is_level() {
        true => drop(world.remove::<InDimension>(root)),
        false => drop(world.insert(root, InDimension(dimension))),
    }
}

/// Puts an Obj, and everything under it, in a dimension, as going through a
/// Portal into it does: a root as [`set_dimension`] puts it; an Obj under
/// another with an [`InDimension`] of its own, where that is not already its
/// parent's.
pub fn put_in_dimension(world: &mut World, obj: Entity, dimension: DimensionId) {
    match world.get::<Parent>(obj).map(|parent| parent.0) {
        None => set_dimension(world, obj, dimension),
        Some(parent) if dimension_of(world, parent) == dimension => drop(world.remove::<InDimension>(obj)),
        Some(_) => drop(world.insert(obj, InDimension(dimension))),
    }
}

/// Every Obj's dimension as a world is now, for a system that asks of many:
/// only the Objs not in the level's are kept, so a game with no other
/// dimensions keeps none and asks nothing more.
#[derive(Clone, Debug, Default)]
pub struct Dimensioned {
    elsewhere: HashMap<Entity, DimensionId>,
}

impl Dimensioned {
    pub fn of(world: &World) -> Self {
        let roots: Vec<(Entity, DimensionId)> =
            world.query_ref::<(Entity, &InDimension)>().into_iter().map(|(entity, own)| (entity, own.0)).collect();
        let mut elsewhere = HashMap::new();
        if roots.is_empty() {
            return Self { elsewhere };
        }
        let mut children: HashMap<Entity, Vec<Entity>> = HashMap::new();
        for (child, parent) in world.query_ref::<(Entity, &Parent)>() {
            children.entry(parent.0).or_default().push(child);
        }
        // From each root down, an Obj under it taking its dimension unless it has its own.
        let mut open: Vec<(Entity, DimensionId)> = roots;
        while let Some((obj, dimension)) = open.pop() {
            let dimension = world.get::<InDimension>(obj).map_or(dimension, |own| own.0);
            if !dimension.is_level() {
                elsewhere.insert(obj, dimension);
            }
            for &child in children.get(&obj).into_iter().flatten() {
                open.push((child, dimension));
            }
        }
        Self { elsewhere }
    }

    /// The dimension an Obj is in.
    pub fn of_obj(&self, entity: Entity) -> DimensionId {
        self.elsewhere.get(&entity).copied().unwrap_or(DimensionId::LEVEL)
    }

    /// Whether every Obj is in the level's dimension.
    pub fn all_in_level(&self) -> bool {
        self.elsewhere.is_empty()
    }

    /// The Objs not in the level's dimension, and the dimension each is in.
    pub fn elsewhere(&self) -> impl Iterator<Item = (Entity, DimensionId)> + '_ {
        self.elsewhere.iter().map(|(entity, dimension)| (*entity, *dimension))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Transform;

    #[test]
    fn an_obj_is_in_its_roots_dimension_and_with_none_in_the_levels() {
        let mut world = World::new();
        let level = world.spawn((Transform::default(),));
        let root = world.spawn((Transform::default(), InDimension(DimensionId(3))));
        let child = world.spawn((Transform::default(), Parent(root)));
        let grandchild = world.spawn((Transform::default(), Parent(child)));
        let own = world.spawn((Transform::default(), Parent(root), InDimension(DimensionId(5))));
        assert_eq!(dimension_of(&world, level), DimensionId::LEVEL);
        assert_eq!(dimension_of(&world, grandchild), DimensionId(3));
        assert_eq!(dimension_of(&world, own), DimensionId(5));
        let found = Dimensioned::of(&world);
        assert!(!found.all_in_level());
        for obj in [level, root, child, grandchild, own] {
            assert_eq!(found.of_obj(obj), dimension_of(&world, obj), "{obj:?}");
        }
        // A child put in another dimension takes its own with it, and back in its parent's, its parent's again.
        put_in_dimension(&mut world, child, DimensionId(4));
        assert_eq!(dimension_of(&world, grandchild), DimensionId(4));
        assert_eq!(Dimensioned::of(&world).of_obj(grandchild), DimensionId(4));
        put_in_dimension(&mut world, child, DimensionId(3));
        assert!(!world.has::<InDimension>(child));
        set_dimension(&mut world, root, DimensionId::LEVEL);
        assert_eq!(dimension_of(&world, grandchild), DimensionId::LEVEL);
        assert!(Dimensioned::of(&world).elsewhere().all(|(obj, _)| obj == own));
    }
}
