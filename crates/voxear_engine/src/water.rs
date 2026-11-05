//! Water as the game asks about it: how high the sea is at a point, which
//! way it faces, and how it flows, from the CPU's copy of its long waves
//! (`voxear_render::OceanSampler`), the same waves the GPU draws and the same
//! on every machine. Moved on each tick, before scripts run, from the
//! level's first Ocean; scripts read it through `Water` in C#.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{Ocean, OceanSampler, SeaState};
use voxear_scene::{Transform, WorldTransform};

/// The sea's long waves on the CPU, kept as a world resource.
#[derive(Default)]
pub struct WaterQuery {
    sampler: Option<OceanSampler>,
    /// The sea and the level the sampler was made for.
    made_for: Option<SeaState>,
}

/// The water at a point: how high its surface is above it, the way that
/// surface faces, and how fast it flows, in metres a second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterPoint {
    pub height: f32,
    pub normal: Vec3,
    pub flow: Vec3,
}

/// Moves the sea's waves on to `time`, in the game's elapsed seconds, as
/// the renderer draws them; makes them again when the level's first Ocean
/// changed, and lets them go when it has none.
pub fn update(world: &mut World, time: f64) {
    voxear_scripting::set_water_sampler(for_scripts);
    let first = world.query_ref::<(&Ocean, &Transform, Option<&WorldTransform>)>().into_iter().next().map(
        |(ocean, transform, at)| {
            let (rotation, position) = at.map_or((transform.rotation, transform.position), |w| {
                let (_, rotation, position) = w.0.to_scale_rotation_translation();
                (rotation, position)
            });
            (ocean.sea_state(rotation), position.y)
        },
    );
    if world.resource::<WaterQuery>().is_none() {
        world.insert_resource(WaterQuery::default());
    }
    let query = world.resource_mut::<WaterQuery>().expect("inserted above");
    let Some((sea, level)) = first else {
        *query = WaterQuery::default();
        return;
    };
    if query.made_for != Some(sea) {
        query.sampler = Some(OceanSampler::new(sea, level));
        query.made_for = Some(sea);
    }
    let sampler = query.sampler.as_mut().expect("made above");
    sampler.set_level(level);
    sampler.set_time(time);
}

/// The water at `at`, or `None` where there is no water.
pub fn sample(world: &World, at: Vec3) -> Option<WaterPoint> {
    let sampler = world.resource::<WaterQuery>()?.sampler.as_ref()?;
    let height = sampler.height_at(at.x, at.z);
    Some(WaterPoint { height, normal: sampler.normal_at(at.x, at.z), flow: Vec3::ZERO })
}

/// The water at a point as scripts take it: height, normal and flow.
fn for_scripts(world: &World, at: [f32; 3]) -> Option<[f32; 7]> {
    let water = sample(world, Vec3::from(at))?;
    let (n, f) = (water.normal, water.flow);
    Some([water.height, n.x, n.y, n.z, f.x, f.y, f.z])
}

/// The sea's wind, along its Obj's +Z, for what wants to know which way the
/// waves run.
pub fn wind_of(rotation: Quat) -> Vec3 {
    let along = rotation * Vec3::Z;
    Vec3::new(along.x, 0.0, along.z).normalize_or(Vec3::Z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_render::Weather;

    #[test]
    fn the_sea_is_asked_about_where_its_ocean_is_and_moves_with_time() {
        let mut world = World::new();
        assert!(sample(&world, Vec3::ZERO).is_none(), "no sea, no water");
        let sea = Ocean { weather: Weather::Stormy, ..Ocean::default() };
        world.spawn((sea, Transform::from_position(Vec3::new(0.0, 3.0, 0.0))));
        update(&mut world, 10.0);
        let now = sample(&world, Vec3::new(5.0, 0.0, 5.0)).expect("a sea");
        assert!((now.height - 3.0).abs() < 8.0 && now.height != 3.0, "waves round its level: {}", now.height);
        assert!(now.normal.y > 0.3);
        update(&mut world, 14.0);
        let later = sample(&world, Vec3::new(5.0, 0.0, 5.0)).unwrap();
        assert_ne!(now.height, later.height, "the waves move on");
    }
}
