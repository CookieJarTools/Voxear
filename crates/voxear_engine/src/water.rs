//! Water as the game asks about it: how high the sea is at a point, which
//! way it faces, and how it flows, from the CPU's copy of its long waves
//! (`voxear_render::OceanSampler`), the same waves the GPU draws and the same
//! on every machine; and the same of Pools and Streams. Moved on each tick,
//! before scripts run, from the level's first Ocean, and each Stream's points
//! along its Path made again; scripts read it through `Water` in C#.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{DryArea, Ocean, OceanSampler, Pool, SeaState, Stream, StreamPoint, stream_over};
use voxear_scene::{Path, Transform, WorldTransform};

/// The sea's long waves on the CPU, and the Streams' points along their
/// Paths, kept as a world resource.
#[derive(Default)]
pub struct WaterQuery {
    sampler: Option<OceanSampler>,
    /// The sea and the level the sampler was made for.
    made_for: Option<SeaState>,
    /// Each Stream's points, in the world, and how deep it is.
    streams: Vec<(Vec<StreamPoint>, f32)>,
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
/// changed, and lets them go when it has none. Lays out each Stream's points
/// where its Path is now.
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
    let streams: Vec<(Vec<StreamPoint>, f32)> = world
        .query_ref::<(&Stream, &Path, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .map(|(stream, path, transform, at)| (stream.points(path, at.map_or_else(|| transform.matrix(), |w| w.0)), stream.depth))
        .collect();
    if world.resource::<WaterQuery>().is_none() {
        world.insert_resource(WaterQuery::default());
    }
    let query = world.resource_mut::<WaterQuery>().expect("inserted above");
    query.streams = streams;
    let Some((sea, level)) = first else {
        (query.sampler, query.made_for) = (None, None);
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

/// Where an Obj is in the world and how it is turned.
fn placed(transform: &Transform, at: Option<&WorldTransform>) -> (Vec3, Quat) {
    at.map_or((transform.position, transform.rotation), |w| {
        let (_, rotation, position) = w.0.to_scale_rotation_translation();
        (position, rotation)
    })
}

/// The water at `at`, or `None` where there is no water: inside a Dry Area,
/// or away from the sea, every Pool and every Stream. Over a Pool's
/// footprint its surface, not the sea's; over a Stream its, flowing.
pub fn sample(world: &World, at: Vec3) -> Option<WaterPoint> {
    let dry = world.query_ref::<(&DryArea, &Transform, Option<&WorldTransform>)>().into_iter().any(|(area, t, w)| {
        let (position, rotation) = placed(t, w);
        area.contains(position, rotation, at)
    });
    if dry {
        return None;
    }
    let pools = world.query_ref::<(&Pool, &Transform, Option<&WorldTransform>)>();
    let pool = pools.into_iter().find_map(|(pool, t, w)| {
        let (position, rotation) = placed(t, w);
        pool.surface_over(position, rotation, at)
    });
    if let Some(height) = pool {
        return Some(WaterPoint { height, normal: Vec3::Y, flow: Vec3::ZERO });
    }
    let query = world.resource::<WaterQuery>()?;
    let stream = query.streams.iter().find_map(|(points, depth)| stream_over(points, *depth, at));
    if let Some((height, flow)) = stream {
        return Some(WaterPoint { height, normal: Vec3::Y, flow });
    }
    let sampler = query.sampler.as_ref()?;
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

    #[test]
    fn a_dry_area_has_no_water_and_a_pool_has_its_own() {
        let mut world = World::new();
        world.spawn((Ocean::default(), Transform::default()));
        world.spawn((DryArea::default(), Transform::from_position(Vec3::new(20.0, 0.0, 0.0))));
        world.spawn((Pool { size: [4.0, 4.0], ..Pool::default() }, Transform::from_position(Vec3::new(-20.0, 6.0, 0.0))));
        update(&mut world, 1.0);
        assert!(sample(&world, Vec3::new(20.0, 0.0, 0.5)).is_none(), "inside the hull it is dry");
        assert_eq!(sample(&world, Vec3::new(-19.0, 4.0, 1.0)).map(|w| w.height), Some(6.0), "the pool's surface");
        assert!(sample(&world, Vec3::new(-19.0, 0.0, 1.0)).is_some_and(|w| w.height != 6.0), "under its bottom, the sea's");
        assert!(sample(&world, Vec3::new(0.0, 0.0, 0.0)).is_some(), "the sea elsewhere");
    }

    #[test]
    fn a_stream_has_water_flowing_along_its_path_with_no_sea() {
        let mut world = World::new();
        world.spawn((Stream { speed: 2.0, ..Stream::default() }, Path::default(), Transform::from_position(Vec3::new(0.0, 5.0, 0.0))));
        update(&mut world, 1.0);
        let water = sample(&world, Vec3::new(0.0, 4.8, 3.0)).expect("in the stream");
        assert!((water.height - 5.0).abs() < 0.3, "its surface: {}", water.height);
        assert!(water.flow.z > 1.5, "flowing down its Path: {}", water.flow);
        assert!(sample(&world, Vec3::new(10.0, 4.8, 3.0)).is_none(), "beside it, dry");
    }
}
