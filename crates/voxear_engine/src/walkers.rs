//! Characters push through foliage with nothing set up: every Obj with a
//! Character Controller bends the grass and plants it walks through, as one
//! with a Foliage Interaction does, about as far round as it is wide. A
//! character with a Foliage Interaction of its own pushes as that says,
//! and one whose Strength is 0 pushes nothing.

use voxear_ecs::{Entity, World};
use voxear_render::{FoliageInteraction, Walkers};
use voxear_scene::{Destroying, Switched, Transform, WorldTransform};

use crate::physics::CharacterController;

/// How far round a character plants are pushed, as many times its radius.
const REACH: f32 = 2.0;

/// How a character `radius` metres round pushes through foliage.
fn push(radius: f32) -> FoliageInteraction {
    FoliageInteraction { radius: (radius.max(0.05) * REACH).max(0.4), strength: 1.0 }
}

/// Notes where the level's characters are, for the renderer to push plants
/// away from them this frame.
pub fn update(world: &mut World) {
    let switched = Switched::of(world);
    let walkers = world
        .query_ref::<(Entity, &CharacterController, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(entity, ..)| switched.is_active(*entity) && world.get::<FoliageInteraction>(*entity).is_none())
        .map(|(_, body, transform, at)| (at.map_or(transform.position, |w| w.0.w_axis.truncate()), push(body.radius)))
        .collect();
    world.insert_resource(Walkers(walkers));
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Vec3;

    use super::*;

    #[test]
    fn characters_push_plants_but_for_those_with_an_interaction_of_their_own() {
        let mut world = World::new();
        let at = |x: f32| Transform::from_position(Vec3::new(x, 0.0, 0.0));
        world.spawn((CharacterController::default(), at(1.0)));
        world.spawn((CharacterController { radius: 0.5, ..CharacterController::default() }, at(2.0)));
        world.spawn((CharacterController::default(), FoliageInteraction::default(), at(3.0)));
        update(&mut world);
        let mut walkers = world.resource::<Walkers>().expect("noted").0.clone();
        walkers.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
        let seen: Vec<(f32, f32)> = walkers.iter().map(|(at, push)| (at.x, push.radius)).collect();
        assert_eq!(seen, [(1.0, 0.6), (2.0, 1.0)]);
    }
}
