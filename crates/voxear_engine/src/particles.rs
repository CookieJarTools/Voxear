//! Playing Particle Systems: each starts as it appears, its time counted
//! from then (`voxear_render::ParticlePlayback`), and scripts play and stop
//! it.

use voxear_ecs::{Entity, World};
use voxear_render::{ParticlePlayback, ParticleSystem};

/// Starts every Particle System that has not started yet, at `now`.
pub fn start(world: &mut World, now: f64) {
    let waiting: Vec<Entity> = world
        .query_ref::<(Entity, &ParticleSystem)>()
        .without::<ParticlePlayback>()
        .into_iter()
        .map(|(entity, _)| entity)
        .collect();
    for entity in waiting {
        world.insert(entity, ParticlePlayback { started: now as f32, stopped: None });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_particle_system_starts_as_it_appears_and_keeps_its_start() {
        let mut world = World::new();
        let first = world.spawn((ParticleSystem::default(),));
        start(&mut world, 2.0);
        let second = world.spawn((ParticleSystem::default(),));
        start(&mut world, 5.0);
        let started = |entity| world.get::<ParticlePlayback>(entity).map(|p| p.started);
        assert_eq!((started(first), started(second)), (Some(2.0), Some(5.0)));
    }
}
