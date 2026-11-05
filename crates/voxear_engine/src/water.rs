//! Water as the game asks about it: how high the sea is at a point, which
//! way it faces, and how it flows, from the CPU's copy of its long waves
//! (`voxear_render::OceanSampler`), the same waves the GPU draws and the same
//! on every machine; and the same of Pools and Streams. Moved on each tick,
//! before scripts run, from the level's first Ocean, and each Stream's points
//! along its Path made again; scripts read it through `Water` in C#.
//!
//! A Buoyancy floats its Obj's Rigidbody on all of it: before each physics
//! step, upright columns across its Collider's footprint are each pushed up
//! as much as they are under water, so waves rock it and it rights itself,
//! and slowed against the way the water there flows, so a Stream carries it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_physics::{
    Push,
    scene::{Collider, ColliderMesh, ColliderShape, PhysicsScene, Rigidbody},
};
use voxear_render::{DryArea, Ocean, OceanSampler, Pool, SeaState, Stream, StreamPoint, stream_over};
use voxear_scene::{Destroying, Path, Transform, WorldTransform};

/// Floats its Obj on the sea, Pools and Streams. It needs a Rigidbody and a
/// Collider beside it; how much it weighs does not change how it floats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Buoyancy {
    /// How much of it sits under still water, from 0 to 1: a boat's half,
    /// a log's more. Over 1, it sinks.
    pub submerged: f32,
    /// How quickly the water slows it, moving and turning, against the way
    /// the water flows, so a Stream carries it along: 1 stops it bobbing
    /// and rocking without its overshooting, less lets it bob longer.
    pub drag: f32,
    pub angular_drag: f32,
}

impl Component for Buoyancy {}

impl Default for Buoyancy {
    fn default() -> Self {
        Self { submerged: 0.5, drag: 0.4, angular_drag: 0.4 }
    }
}

/// How many columns a side the water pushes a floating Obj up by.
const COLUMNS: usize = 3;

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
    sample_wet(world, world.resource::<PhysicsScene>(), at)
}

/// How deep the sea is round `at` for how much its waves die away there:
/// the deepest within a couple of metres, so a rock or post standing in
/// deep water does not calm the waves round it as a beach would (as
/// `seaDepth` in the water shader).
fn sea_depth(world: &World, physics: Option<&PhysicsScene>, level: f32, at: Vec3) -> f32 {
    [Vec3::ZERO, Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z]
        .into_iter()
        .map(|side| depth_under(world, physics, level, at + side * 2.0))
        .fold(f32::NEG_INFINITY, f32::max)
}

/// How deep the sea is under `at`: from `level` down to the first collider
/// under it that is not floating, by a ray from above the waves; deep where
/// there is none, or physics has not started.
fn depth_under(world: &World, physics: Option<&PhysicsScene>, level: f32, at: Vec3) -> f32 {
    let Some(physics) = physics else { return f32::INFINITY };
    let mut from = Vec3::new(at.x, level + 50.0, at.z);
    for _ in 0..4 {
        let Some(hit) = physics.raycast(from, Vec3::NEG_Y, 400.0, u32::MAX, false) else { return f32::INFINITY };
        if !world.get::<Rigidbody>(hit.obj).is_some_and(|body| !body.kinematic) {
            return level - hit.point.y;
        }
        // Under a floating thing: past it to what is below.
        from = hit.point - Vec3::Y * 0.05;
    }
    f32::INFINITY
}

/// The water at `at` as if there were no Dry Areas: what a hull, which is
/// one, floats on. The sea's waves die away where it is shallow.
fn sample_wet(world: &World, physics: Option<&PhysicsScene>, at: Vec3) -> Option<WaterPoint> {
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
    let depth = sea_depth(world, physics, sampler.level(), at);
    let height = sampler.height_over(at.x, at.z, depth);
    Some(WaterPoint { height, normal: sampler.normal_over(at.x, at.z, depth), flow: Vec3::ZERO })
}

/// The water at a point as scripts take it: height, normal and flow.
fn for_scripts(world: &World, at: [f32; 3]) -> Option<[f32; 7]> {
    let water = sample(world, Vec3::from(at))?;
    let (n, f) = (water.normal, water.flow);
    Some([water.height, n.x, n.y, n.z, f.x, f.y, f.z])
}

/// Where a Collider is in its Obj's space: its middle and how far it
/// reaches each way, as a box round it.
fn collider_box(world: &World, entity: Entity, collider: &Collider) -> (Vec3, Vec3) {
    let middle = Vec3::from(collider.center);
    let reach = match collider.shape {
        ColliderShape::Box => Vec3::from(collider.size) * 0.5,
        ColliderShape::Sphere => Vec3::splat(collider.radius),
        ColliderShape::Capsule => Vec3::new(collider.radius, (collider.height * 0.5).max(collider.radius), collider.radius),
        ColliderShape::Mesh => {
            let points = world.get::<ColliderMesh>(entity).map(|mesh| mesh.points.clone());
            let (low, high) = points.iter().flat_map(|p| p.iter()).fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(low, high), &p| (low.min(p), high.max(p)));
            return match low.x.is_finite() {
                true => ((low + high) * 0.5, (high - low) * 0.5),
                false => (middle, Vec3::splat(0.5)),
            };
        }
    };
    (middle, reach)
}

/// Pushes each floating Obj's body up where it is under water, and slows it
/// against the water, for the physics step about to run. A body physics has
/// not made yet floats from the next step.
pub fn float(world: &World, physics: &mut PhysicsScene) {
    let floating: Vec<(Entity, Buoyancy, Collider, f32)> = world
        .query_ref::<(Entity, &Buoyancy, &Collider, &Rigidbody)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(.., body)| !body.kinematic)
        .map(|(entity, buoyancy, collider, body)| (entity, *buoyancy, *collider, body.mass.max(1e-3)))
        .collect();
    let gravity = physics.gravity().length();
    for (entity, buoyancy, collider, mass) in floating {
        let Some(body) = physics.body(entity) else { continue };
        let to_world: Mat4 = voxear_scene::world_matrix(world, entity);
        let (middle, reach) = collider_box(world, entity, &collider);
        // Each column as tall as the Collider stands, in the world.
        let tall = to_world.transform_vector3(Vec3::new(0.0, reach.y * 2.0, 0.0)).length().max(1e-3);
        let centre = to_world.transform_point3(middle);
        let share = 1.0 / (COLUMNS * COLUMNS) as f32;
        // As strong a push, wholly under, as holds it up `submerged` under.
        let submerged = buoyancy.submerged.max(0.01);
        let lift = mass * gravity / submerged * share;
        // Drag measured against how it bobs: a spring as stiff as the water's push rising up it, and as much
        // drag, at 1, as stops it with no overshoot; turning, as far out as its sides are.
        let stiff = mass * gravity / (submerged * tall);
        let damping = 2.0 * (stiff * mass).sqrt();
        let across = to_world.transform_vector3(Vec3::new(reach.x, 0.0, reach.z)).length_squared() / 3.0;
        let (moving, turning) = physics.world().velocity(body);
        let mut under_all = 0.0;
        for i in 0..COLUMNS {
            for j in 0..COLUMNS {
                let spot = |k: usize| (k as f32 + 0.5) / COLUMNS as f32 * 2.0 - 1.0;
                let at = to_world.transform_point3(middle + Vec3::new(reach.x * spot(i), 0.0, reach.z * spot(j)));
                let Some(water) = sample_wet(world, Some(&*physics), at) else { continue };
                let under = ((water.height - (at.y - tall * 0.5)) / tall).clamp(0.0, 1.0);
                if under <= 0.0 {
                    continue;
                }
                under_all += under * share;
                let along = moving + turning.cross(at - centre) - water.flow;
                let force = Vec3::Y * lift * under - along * buoyancy.drag * damping * share * under / submerged;
                physics.world_mut().push(body, Push::Force(force, Some(at)));
            }
        }
        if under_all > 0.0 {
            let slowing = buoyancy.angular_drag * damping * across * under_all / submerged;
            physics.world_mut().push(body, Push::Torque(-turning * slowing));
        }
    }
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

    /// A crate a metre across, floating, dropped over water from two metres up.
    fn crate_over(world: &mut World, at: Vec3) -> Entity {
        let body = Rigidbody { mass: 40.0, ..Rigidbody::default() };
        world.spawn((Buoyancy::default(), Collider::default(), body, Transform::from_position(at)))
    }

    fn play(world: &mut World, seconds: f32) {
        let mut physics = PhysicsScene::new();
        let dt = 1.0 / 60.0;
        for step in 0..(seconds / dt) as usize {
            update(world, f64::from(step as f32 * dt));
            voxear_scene::propagate_transforms(world);
            float(world, &mut physics);
            physics.step(world, dt);
        }
    }

    #[test]
    fn a_buoyant_crate_floats_half_under_a_pool_and_a_stream_carries_another() {
        let mut world = World::new();
        world.spawn((Pool { size: [20.0, 20.0], depth: 5.0, ..Pool::default() }, Transform::default()));
        let floating = crate_over(&mut world, Vec3::new(0.0, 2.0, 0.0));
        let point = |z: f32| voxear_scene::PathPoint { position: [0.0, 0.0, z], width: 8.0 };
        let river = Path { points: vec![point(0.0), point(40.0), point(80.0)], closed: false };
        world.spawn((Stream { speed: 2.0, ..Stream::default() }, river, Transform::from_position(Vec3::new(40.0, 0.0, 0.0))));
        let drifting = crate_over(&mut world, Vec3::new(40.0, 1.0, 10.0));
        play(&mut world, 8.0);
        let at = |entity| world.get::<Transform>(entity).unwrap().position;
        assert!(at(floating).y.abs() < 0.08, "half under, its middle at the surface: {}", at(floating));
        assert!(at(floating).x.abs() < 0.1 && at(floating).z.abs() < 0.1, "still water, still crate: {}", at(floating));
        assert!(at(drifting).z > 20.0 && at(drifting).y.abs() < 0.2, "carried down the stream: {}", at(drifting));
    }

    #[test]
    fn the_sea_calms_over_a_shallow_bank() {
        let mut world = World::new();
        world.spawn((Ocean { weather: voxear_render::Weather::Stormy, ..Ocean::default() }, Transform::default()));
        // A bank a metre under the surface, on the left.
        let bank = Collider { size: [40.0, 2.0, 40.0], ..Collider::default() };
        world.spawn((bank, Transform::from_position(Vec3::new(-30.0, -2.0, 0.0))));
        let mut physics = PhysicsScene::new();
        update(&mut world, 12.0);
        voxear_scene::propagate_transforms(&mut world);
        physics.step(&mut world, 1.0 / 60.0);
        let swing = |x: f32| {
            (0..40).map(|i| sample_wet(&world, Some(&physics), Vec3::new(x, 0.0, i as f32 * 0.4 - 8.0)).unwrap().height.abs()).fold(0.0f32, f32::max)
        };
        let (shallow, open) = (swing(-30.0), swing(40.0));
        assert!(shallow < open * 0.4, "waves over the bank {shallow}, in open water {open}");
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
