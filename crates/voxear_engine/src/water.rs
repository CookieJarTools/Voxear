//! Water as the game asks about it: how high the sea is at a point, which
//! way it faces, and how it flows, from the CPU's copy of its long waves
//! (`voxear_render::OceanSampler`), the same waves the GPU draws and the same
//! on every machine; and the same of Pools, Streams and worlds of blocks'
//! water (`voxear_terrain::blocks::BlockWorld::water_over`). Moved on each tick,
//! before scripts run, from the level's first Ocean, and each Stream's points
//! along its Path made again; scripts read it through `Water` in C#.
//!
//! A Buoyancy floats its Obj's Rigidbody on all of it: before each physics
//! step, upright columns across its Collider's footprint are each pushed up
//! as much as they are under water, so waves rock it and it rights itself,
//! and slowed against the way the water there flows, so a Stream carries it.
//! Up is against the pull where it is, and the push as strong as that pull.
//!
//! What goes into the water fast throws up a splash, and so does what moves
//! through it fast, floating or not: drops of water, or of molten rock from
//! lava. Scripts make one with `Water.Splash`.

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_physics::{
    Push,
    scene::{Collider, ColliderMesh, ColliderShape, PhysicsScene, Rigidbody},
};
use voxear_render::{DryArea, DryShape, Liquid, Ocean, SeaEasing, SeaNow, WindZone, OceanSampler, ParticlePlayback, ParticleSystem, Pool, SeaState, Stream, StreamPoint, ocean, stream_over};
use voxear_scene::{Destroying, Path, Transform, WorldTransform};

pub use voxear_physics::scene::Buoyancy;

/// How many columns a side the water pushes a floating Obj up by.
const COLUMNS: usize = 3;

/// How far over a world of blocks' water a point is still over it, in
/// metres, through air: as a Pool's surface is over all its footprint.
const OVER_BLOCK_WATER: f32 = 16.0;

/// The sea's long waves on the CPU, and the Streams' points along their
/// Paths, kept as a world resource.
#[derive(Default)]
pub struct WaterQuery {
    sampler: Option<OceanSampler>,
    /// The sea and the level the sampler was made for.
    made_for: Option<SeaState>,
    /// Each Stream's points, in the world, how deep it is and what runs in it.
    streams: Vec<(Vec<StreamPoint>, f32, Liquid)>,
    /// The engine's time as the water was last moved on, for splashes scripts make.
    now: f64,
    /// Whether the sea's waves calm in shallow water (the Ocean's Shores).
    shores: bool,
    /// How the sea is following its wind as it changes.
    easing: Option<SeaEasing>,
}

/// The water at a point: how high its surface is above it, the way that
/// surface faces, how fast it flows, in metres a second, and whether it is
/// water or lava.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterPoint {
    pub height: f32,
    pub normal: Vec3,
    pub flow: Vec3,
    pub liquid: Liquid,
}

/// Moves the sea's waves on to `time`, in the game's elapsed seconds, as
/// the renderer draws them; eases them towards the sea the level's first
/// Ocean's wind makes, putting the sea now on its Obj ([`SeaNow`]) for the
/// renderer to draw the same; and lets them go when it has none. Lays out
/// each Stream's points where its Path is now.
pub fn update(world: &mut World, time: f64) {
    voxear_scripting::set_water_sampler(for_scripts);
    voxear_scripting::set_water_splasher(splash_for_scripts);
    // The first Global Wind Zone that is on, as the renderer blows the sea with.
    let switched = voxear_scene::Switched::of(world);
    let zone = world
        .query_ref::<(Entity, &WindZone, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .find(|(entity, zone, ..)| {
            zone.shape == voxear_render::WindShape::Global && switched.is_on(world, *entity, voxear_scene::Switch::WindZone)
        })
        .map(|(_, zone, transform, at)| (placed(transform, at).1, zone.strength));
    // As hard as the weather has it blowing, as the renderer blows everything else.
    let weather = voxear_render::weather::Weather::in_world(world);
    let zone = zone.map(|(rotation, strength)| {
        (rotation, weather.wind(WindZone { strength, ..WindZone::default() }).strength)
    });
    let first = world.query_ref::<(Entity, &Ocean, &Transform, Option<&WorldTransform>)>().into_iter().next().map(
        |(entity, ocean, transform, at)| {
            let (position, rotation) = placed(transform, at);
            (entity, ocean.sea_blown(rotation, zone), ocean.transition, position.y, ocean.shores)
        },
    );
    let streams: Vec<(Vec<StreamPoint>, f32, Liquid)> = world
        .query_ref::<(&Stream, &Path, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .map(|(stream, path, transform, at)| {
            (stream.points(path, at.map_or_else(|| transform.matrix(), |w| w.0)), stream.depth, stream.liquid)
        })
        .collect();
    let origin = voxear_scene::WorldOrigin::of(world).0;
    if world.resource::<WaterQuery>().is_none() {
        world.insert_resource(WaterQuery::default());
    }
    let query = world.resource_mut::<WaterQuery>().expect("inserted above");
    query.streams = streams;
    query.now = time;
    let Some((entity, target, transition, level, shores)) = first else {
        (query.sampler, query.made_for, query.easing) = (None, None, None);
        return;
    };
    query.shores = shores;
    let sea = query.easing.get_or_insert_with(|| SeaEasing::new(target)).at(target, time, transition);
    // A change of only the waves' height or choppiness keeps the spectrum it has.
    if query.made_for != Some(sea) {
        match query.sampler.as_mut() {
            Some(sampler) => sampler.set_sea(sea),
            None => query.sampler = Some(OceanSampler::new(sea, level)),
        }
        query.made_for = Some(sea);
    }
    let sampler = query.sampler.as_mut().expect("made above");
    sampler.set_level(level);
    sampler.set_origin([origin.x as f64, origin.z as f64]);
    sampler.set_time(time);
    world.insert(entity, SeaNow(sea));
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
    let dry = world.query_ref::<(&DryArea, &Transform, Option<&WorldTransform>)>().into_iter().find_map(|(area, t, w)| {
        let (position, rotation) = placed(t, w);
        let (inside, model) = match area.shape {
            DryShape::Mesh => {
                let model = w.map_or_else(|| t.matrix(), |w| w.0);
                (area.mesh_contains(model, at), model)
            }
            _ => (area.contains(position, rotation, at), Mat4::from_rotation_translation(rotation, position)),
        };
        // Inside, only its bilge's water.
        inside.then(|| area.bilge_over(model, at))
    });
    if let Some(bilge) = dry {
        return bilge.map(|height| WaterPoint { height, normal: Vec3::Y, flow: Vec3::ZERO, liquid: Liquid::Water });
    }
    sample_wet(world, world.resource::<PhysicsScene>(), at)
}

/// How shallow the sea is round `at` for how much its waves die away there:
/// its depth averaged over a few metres, so the waves calm gradually where
/// the ground drops away and a rock or post in deep water barely calms them
/// (as the shaders' blurred sea floor depth, `floorDepthMain`); and, where it
/// is shallow, how much of the waves reach it from deeper water round it.
/// Neither looks past a wall standing out of the sea, as waves do not.
fn sea_shallows(world: &World, physics: Option<&PhysicsScene>, level: f32, at: Vec3) -> ocean::Shallows {
    let open = |point: Vec3| open_water(physics, level, at, point);
    let ring = |reach: f32| {
        (0..8).map(move |i| {
            let angle = i as f32 * std::f32::consts::FRAC_PI_4;
            at + Vec3::new(angle.cos(), 0.0, angle.sin()) * reach
        })
    };
    let depth_at = |point: Vec3| depth_under(world, physics, level, point).clamp(0.0, ocean::SHALLOWS_DEPTH);
    let depths: Vec<f32> = std::iter::once(at).chain(ring(4.0).filter(|&p| open(p))).map(depth_at).collect();
    let depth = depths.iter().sum::<f32>() / depths.len() as f32;
    if depth >= ocean::SHALLOWS_REACHED {
        return depth.into();
    }
    let deepest = ring(ocean::SHALLOWS_LOOK).filter(|&p| open(p)).map(depth_at).fold(depth, f32::max);
    ocean::Shallows { depth, reach: ocean::waves_reach(depth, deepest) }
}

/// Whether the sea runs from `at` to `point` at its level, with no collider
/// standing out of it between them.
fn open_water(physics: Option<&PhysicsScene>, level: f32, at: Vec3, point: Vec3) -> bool {
    let Some(physics) = physics else { return true };
    let (from, to) = (Vec3::new(at.x, level, at.z), Vec3::new(point.x, level, point.z));
    let along = to - from;
    physics.raycast(from, along.normalize_or_zero(), along.length(), u32::MAX, false).is_none()
}

/// How deep the sea is under `at`: from `level` down to the first collider
/// under it that is not floating, by a ray from the ceiling over the sea
/// the drawn floor is kept under, so a roof does not count; deep where there
/// is none, or physics has not started.
fn depth_under(world: &World, physics: Option<&PhysicsScene>, level: f32, at: Vec3) -> f32 {
    let Some(physics) = physics else { return f32::INFINITY };
    let mut from = Vec3::new(at.x, level + ocean::FLOOR_CEILING, at.z);
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
        Some((pool.surface_over(position, rotation, at)?, pool.liquid))
    });
    if let Some((height, liquid)) = pool {
        return Some(WaterPoint { height, normal: Vec3::Y, flow: Vec3::ZERO, liquid });
    }
    // In a world of blocks' water, or over it: the top of its column, still.
    let blocks = world.query_ref::<(&voxear_terrain::blocks::BlockWorld, &Transform, Option<&WorldTransform>)>();
    let block_water = blocks.into_iter().find_map(|(blocks, local, placed)| {
        let corner = voxear_terrain::corner(local, placed);
        Some(corner.y + blocks.water_over((at - corner).to_array(), OVER_BLOCK_WATER)?)
    });
    if let Some(height) = block_water {
        return Some(WaterPoint { height, normal: Vec3::Y, flow: Vec3::ZERO, liquid: Liquid::Water });
    }
    let query = world.resource::<WaterQuery>()?;
    let stream =
        query.streams.iter().find_map(|(points, depth, liquid)| Some((stream_over(points, *depth, at)?, *liquid)));
    if let Some(((height, flow), liquid)) = stream {
        return Some(WaterPoint { height, normal: Vec3::Y, flow, liquid });
    }
    let sampler = query.sampler.as_ref()?;
    let shallows = if query.shores { sea_shallows(world, physics, sampler.level(), at) } else { f32::INFINITY.into() };
    let height = sampler.height_over(at.x, at.z, shallows);
    // On the sea, the water flows as its waves carry it round, so what floats is slowed against that, not held back.
    let (normal, flow) = (sampler.normal_over(at.x, at.z, shallows), sampler.velocity_over(at.x, at.z, shallows));
    Some(WaterPoint { height, normal, flow, liquid: Liquid::Water })
}

/// How deep under water `at` is, in metres (less than nothing over it),
/// how the water there flows, and what it is (see `sample_wet`).
fn depth_at(world: &World, physics: Option<&PhysicsScene>, at: Vec3) -> Option<(f32, Vec3, Liquid)> {
    sample_wet(world, physics, at).map(|water| (water.height - at.y, water.flow, water.liquid))
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

/// A body in or near water as `float` found it: how much of it is under
/// water, the water's surface over its middle, how fast it is falling, how
/// fast it moves across the water against the way that flows, how wide it
/// is and what the water is, for `splash` to tell when it hits the water or
/// ploughs through it.
pub struct Floating {
    entity: Entity,
    under: f32,
    surface: Vec3,
    falling: f32,
    across: f32,
    width: f32,
    liquid: Liquid,
}

/// Pushes each floating Obj's body up where it is under water, and slows it
/// against the water, for the physics step about to run. A body physics has
/// not made yet floats from the next step. Gives back how each lies in the
/// water, and how each other body that moves lies in it too, which the
/// water does not push.
pub fn float(world: &World, physics: &mut PhysicsScene) -> Vec<Floating> {
    let mut found = sinking(world, physics);
    let floating: Vec<(Entity, Buoyancy, Collider, f32)> = world
        .query_ref::<(Entity, &Buoyancy, &Collider, &Rigidbody)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(.., body)| !body.kinematic)
        .map(|(entity, buoyancy, collider, body)| (entity, *buoyancy, *collider, body.mass.max(1e-3)))
        .collect();
    for (entity, buoyancy, collider, mass) in floating {
        let Some(body) = physics.body(entity) else { continue };
        let to_world: Mat4 = voxear_scene::world_matrix(world, entity);
        let (middle, reach) = collider_box(world, entity, &collider);
        // Each column as tall as the Collider stands, in the world.
        let tall = to_world.transform_vector3(Vec3::new(0.0, reach.y * 2.0, 0.0)).length().max(1e-3);
        let centre = to_world.transform_point3(middle);
        let share = 1.0 / (COLUMNS * COLUMNS) as f32;
        // Up against the pull here, and the water's push as strong as it.
        let pull = physics.gravity_at(world, centre);
        let strength = pull.length();
        let up = if strength > 1e-6 { -pull / strength } else { Vec3::Y };
        // As strong a push, wholly under, as holds it up `submerged` under.
        let submerged = buoyancy.submerged.max(0.01);
        let lift = mass * strength / submerged * share;
        // Drag measured against how it bobs: a spring as stiff as the water's push rising up it, and as much
        // drag, at 1, as stops it with no overshoot; turning, as far out as its sides are.
        let stiff = mass * strength / (submerged * tall);
        let damping = 2.0 * (stiff * mass).sqrt();
        let across = to_world.transform_vector3(Vec3::new(reach.x, 0.0, reach.z)).length_squared() / 3.0;
        let (moving, turning) = physics.world().velocity(body);
        let mut under_all = 0.0;
        for i in 0..COLUMNS {
            for j in 0..COLUMNS {
                let spot = |k: usize| (k as f32 + 0.5) / COLUMNS as f32 * 2.0 - 1.0;
                let at = to_world.transform_point3(middle + Vec3::new(reach.x * spot(i), 0.0, reach.z * spot(j)));
                let Some((depth, flow, _)) = depth_at(world, Some(&*physics), at) else { continue };
                let under = ((depth + tall * 0.5) / tall).clamp(0.0, 1.0);
                if under <= 0.0 {
                    continue;
                }
                under_all += under * share;
                let along = moving + turning.cross(at - centre) - flow;
                let force = up * lift * under - along * buoyancy.drag * damping * share * under / submerged;
                physics.world_mut().push(body, Push::Force(force, Some(at)));
            }
        }
        if under_all > 0.0 {
            let slowing = buoyancy.angular_drag * damping * across * under_all / submerged;
            physics.world_mut().push(body, Push::Torque(-turning * slowing));
        }
        let water = depth_at(world, Some(&*physics), centre);
        let surface = water.map_or(centre, |(depth, ..)| centre + up * depth);
        let width = (across * 3.0).sqrt() * 2.0;
        let (flow, liquid) = water.map_or((Vec3::ZERO, Liquid::Water), |(_, flow, liquid)| (flow, liquid));
        let against = moving - flow;
        let ploughing = (against - up * against.dot(up)).length();
        found.push(Floating { entity, under: under_all, surface, falling: -moving.dot(up), across: ploughing, width, liquid });
    }
    found
}

/// How fast a body that does not float moves before the water under it is
/// looked for, in metres a second: one at rest makes no splash.
const SINKING_SPEED: f32 = 0.5;

/// How each body that does not float lies in the water, of those that move:
/// a rock thrown in, or a barrel rolled through the shallows. The water does
/// not push them; they are found so that they splash.
fn sinking(world: &World, physics: &PhysicsScene) -> Vec<Floating> {
    let bodies = world.query_ref::<(Entity, &Collider, &Rigidbody)>().without::<Destroying>().without::<Buoyancy>();
    let mut found = Vec::new();
    for (entity, collider, _) in bodies.into_iter().filter(|(.., body)| !body.kinematic) {
        let Some(body) = physics.body(entity) else { continue };
        let (moving, _) = physics.world().velocity(body);
        if moving.length_squared() < SINKING_SPEED * SINKING_SPEED {
            continue;
        }
        let to_world: Mat4 = voxear_scene::world_matrix(world, entity);
        let (middle, reach) = collider_box(world, entity, collider);
        let centre = to_world.transform_point3(middle);
        let pull = physics.gravity_at(world, centre);
        let up = if pull.length_squared() > 1e-12 { -pull.normalize() } else { Vec3::Y };
        let Some((depth, flow, liquid)) = depth_at(world, Some(physics), centre) else { continue };
        let tall = to_world.transform_vector3(Vec3::new(0.0, reach.y * 2.0, 0.0)).length().max(1e-3);
        let width = to_world.transform_vector3(Vec3::new(reach.x, 0.0, reach.z)).length() * 2.0;
        let against = moving - flow;
        found.push(Floating {
            entity,
            under: ((depth + tall * 0.5) / tall).clamp(0.0, 1.0),
            surface: centre + up * depth,
            falling: -moving.dot(up),
            across: (against - up * against.dot(up)).length(),
            width,
            liquid,
        });
    }
    found
}

/// How fast a floating body falls into the water, in metres a second, before
/// it splashes.
const SPLASH_SPEED: f32 = 2.0;
/// How much of a body is under water before it counts as in it.
const IN_WATER: f32 = 0.02;

/// How fast a body moves across the water, half in it, before it throws up
/// a splash as it goes, in metres a second, and how long it is before its
/// next, in seconds. Lava, as thick as it is, is thrown up sooner.
const PLOUGH_SPEED: f32 = 1.5;
const PLOUGH_SPEED_LAVA: f32 = 0.6;
const PLOUGH_EVERY: f64 = 0.35;

/// How much of a body was under water as last seen, to tell when it goes
/// in, and when it may next splash as it ploughs through.
#[derive(Clone, Copy, Debug)]
struct Wetted(f32, f64);

impl Component for Wetted {}

/// A splash's Obj, taken away once its drops have fallen.
#[derive(Clone, Copy, Debug)]
struct Splash {
    until: f64,
}

impl Component for Splash {}

/// Throws up a splash where each body has just gone into the water fast
/// enough, as big as it is fast and wide, and a smaller one every so often
/// while it ploughs through the water half in it; and takes away the
/// splashes whose drops have fallen. `now` is the engine's time.
pub fn splash(world: &mut World, floating: Vec<Floating>, now: f64) {
    for body in floating {
        let Wetted(was, next) = world.get::<Wetted>(body.entity).copied().unwrap_or(Wetted(1.0, 0.0));
        let entering = was < IN_WATER && body.under >= IN_WATER && body.falling >= SPLASH_SPEED;
        let fast = if body.liquid == Liquid::Lava { PLOUGH_SPEED_LAVA } else { PLOUGH_SPEED };
        let ploughing = body.under >= IN_WATER && body.under < 0.95 && body.across >= fast && now >= next;
        let again = if entering || ploughing { now + PLOUGH_EVERY } else { next };
        world.insert(body.entity, Wetted(body.under, again));
        if entering {
            throw_up(world, body.surface, body.falling, body.width, body.liquid, now);
        } else if ploughing {
            throw_up(world, body.surface, body.across * SPLASH_SPEED / fast, body.width * 0.5, body.liquid, now);
        }
    }
    let splashes = world.query_ref::<(Entity, &Splash)>();
    let gone: Vec<Entity> = splashes.into_iter().filter(|(_, splash)| splash.until < now).map(|(e, _)| e).collect();
    for entity in gone {
        world.despawn(entity);
    }
}

/// A splash at `surface`, as of something `width` wide going in at `falling`
/// metres a second: drops of water, or blobs of molten rock from lava.
fn throw_up(world: &mut World, surface: Vec3, falling: f32, width: f32, liquid: Liquid, now: f64) {
    let drops = match liquid {
        Liquid::Water => splash_drops(falling, width),
        Liquid::Lava => lava_drops(falling, width),
    };
    let lasts = f64::from(drops.lifetime[1]) + 0.2;
    let playback = ParticlePlayback { started: crate::particles::now(world) as f32, stopped: None };
    world.spawn((drops, playback, Splash { until: now + lasts }, Transform::from_position(surface)));
}

/// A splash where the water's surface is over or under `at`, `size` metres
/// wide, as scripts ask for one (`Water.Splash`): whether there was water
/// there to throw up.
pub fn splash_at(world: &mut World, at: Vec3, size: f32) -> bool {
    let Some(water) = sample(world, at) else { return false };
    let now = world.resource::<WaterQuery>().map_or(0.0, |query| query.now);
    let size = size.clamp(0.05, 20.0);
    throw_up(world, Vec3::new(at.x, water.height, at.z), SPLASH_SPEED * (1.0 + size.min(3.0)), size, water.liquid, now);
    true
}

/// A splash as scripts ask for one.
fn splash_for_scripts(world: &mut World, at: [f32; 3], size: f32) -> bool {
    splash_at(world, Vec3::from(at), size)
}

/// A splash's drops, for a body falling `falling` metres a second, `width`
/// wide: thrown up round it, more and higher the harder it hits.
fn splash_drops(falling: f32, width: f32) -> ParticleSystem {
    let hard = (falling / SPLASH_SPEED).min(4.0);
    ParticleSystem {
        looping: false,
        duration: 0.1,
        rate: 0.0,
        burst: (25.0 * hard * width.clamp(0.3, 4.0)).clamp(10.0, 400.0) as u32,
        lifetime: [0.5, 0.9 + 0.2 * hard],
        speed: [1.0 + hard * 0.5, 2.0 + hard * 1.2],
        size: [0.06, 0.16],
        size_end: 2.0,
        color: [1.0, 1.0, 1.0, 0.7],
        color_end: [1.0, 1.0, 1.0, 0.0],
        radius: width * 0.5,
        angle: 35.0,
        gravity: 1.0,
        drag: 0.4,
        softness: 0.1,
        ..ParticleSystem::default()
    }
}

/// A splash of lava, for a body falling `falling` metres a second, `width`
/// wide: fewer and heavier than water's drops and thrown less far, as thick
/// as lava is; blobs of molten rock that glow yellow and cool to red as they
/// fall back.
fn lava_drops(falling: f32, width: f32) -> ParticleSystem {
    let hard = (falling / SPLASH_SPEED).min(4.0);
    ParticleSystem {
        looping: false,
        duration: 0.1,
        rate: 0.0,
        burst: (10.0 * hard * width.clamp(0.3, 4.0)).clamp(6.0, 160.0) as u32,
        lifetime: [0.45, 0.75 + 0.15 * hard],
        speed: [0.8 + hard * 0.4, 1.6 + hard * 0.9],
        size: [0.05, 0.15],
        size_end: 0.7,
        color: [1.0, 0.5, 0.1, 1.0],
        color_end: [0.55, 0.05, 0.0, 0.85],
        intensity: 6.0,
        radius: width * 0.5,
        angle: 40.0,
        gravity: 1.0,
        drag: 0.1,
        lit: false,
        softness: 0.02,
        ..ParticleSystem::default()
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
    fn a_crate_dropped_in_splashes_once_and_the_splash_clears_away() {
        let mut world = World::new();
        world.spawn((Pool { size: [20.0, 20.0], depth: 5.0, ..Pool::default() }, Transform::default()));
        crate_over(&mut world, Vec3::new(0.0, 4.0, 0.0));
        let mut physics = PhysicsScene::new();
        let dt = 1.0 / 60.0;
        let mut splashes: Vec<(Entity, f64, u32, Vec3)> = Vec::new();
        for step in 0..(5.0 / dt) as usize {
            let now = f64::from(step as f32 * dt);
            update(&mut world, now);
            voxear_scene::propagate_transforms(&mut world);
            let floating = float(&world, &mut physics);
            physics.step(&mut world, dt);
            splash(&mut world, floating, now);
            let made: Vec<(Entity, u32, Vec3)> = world
                .query_ref::<(Entity, &ParticleSystem, &Transform, &Splash)>()
                .into_iter()
                .map(|(entity, drops, at, _)| (entity, drops.burst, at.position))
                .collect();
            for (entity, burst, at) in made {
                if !splashes.iter().any(|(seen, ..)| *seen == entity) {
                    splashes.push((entity, now, burst, at));
                }
            }
        }
        assert_eq!(splashes.len(), 1, "one splash as it goes in, none as it bobs: {splashes:?}");
        let (_, when, burst, at) = splashes[0];
        assert!(when > 0.5 && when < 1.2, "as it reaches the water, falling three metres: {when}");
        assert!(burst > 40 && at.y.abs() < 0.3, "a big splash at the surface: {burst} at {at}");
        assert!(world.query_ref::<&Splash>().into_iter().next().is_none(), "gone once its drops have fallen");
    }

    #[test]
    fn a_rock_that_sinks_into_lava_throws_up_molten_rock() {
        let mut world = World::new();
        let lake = Pool { size: [20.0, 20.0], depth: 5.0, liquid: Liquid::Lava, ..Pool::default() };
        world.spawn((lake, Transform::default()));
        // No Buoyancy: it goes straight down.
        let body = Rigidbody { mass: 40.0, ..Rigidbody::default() };
        world.spawn((Collider::default(), body, Transform::from_position(Vec3::new(0.0, 4.0, 0.0))));
        let mut physics = PhysicsScene::new();
        let dt = 1.0 / 60.0;
        let mut thrown: Vec<(Entity, ParticleSystem, Vec3)> = Vec::new();
        for step in 0..(1.5 / dt) as usize {
            let now = f64::from(step as f32 * dt);
            update(&mut world, now);
            voxear_scene::propagate_transforms(&mut world);
            let floating = float(&world, &mut physics);
            physics.step(&mut world, dt);
            splash(&mut world, floating, now);
            for (entity, drops, at, _) in world.query_ref::<(Entity, &ParticleSystem, &Transform, &Splash)>() {
                if !thrown.iter().any(|(seen, ..)| *seen == entity) {
                    thrown.push((entity, *drops, at.position));
                }
            }
        }
        assert_eq!(thrown.len(), 1, "one splash as it goes in");
        let (_, drops, at) = thrown[0];
        assert!(at.y.abs() < 0.3, "at the lava's surface: {at}");
        assert!(drops.color[0] > drops.color[2] * 4.0 && !drops.lit && drops.intensity > 2.0, "glowing orange: {drops:?}");
        assert!(drops.burst < splash_drops(8.0, 1.7).burst, "fewer than water throws up, as thick as it is");
    }

    #[test]
    fn what_ploughs_through_water_half_in_it_splashes_every_so_often() {
        let mut world = World::new();
        let boat = world.spawn(Transform::default());
        let at = |across: f32| Floating {
            entity: boat,
            under: 0.5,
            surface: Vec3::ZERO,
            falling: 0.0,
            across,
            width: 1.0,
            liquid: Liquid::Water,
        };
        let splashes = |world: &World| world.query_ref::<&Splash>().into_iter().count();
        world.insert(boat, Wetted(0.5, 0.0));
        splash(&mut world, vec![at(0.5)], 1.0);
        assert_eq!(splashes(&world), 0, "drifting, none");
        splash(&mut world, vec![at(3.0)], 1.1);
        assert_eq!(splashes(&world), 1, "ploughing through at three metres a second");
        splash(&mut world, vec![at(3.0)], 1.2);
        assert_eq!(splashes(&world), 1, "not again at once");
        splash(&mut world, vec![at(3.0)], 1.6);
        assert_eq!(splashes(&world), 2, "but again a little later");
    }

    #[test]
    fn a_splash_asked_for_is_thrown_up_at_the_waters_surface_where_there_is_water() {
        let mut world = World::new();
        world.spawn((Pool { size: [6.0, 6.0], ..Pool::default() }, Transform::from_position(Vec3::new(0.0, 2.0, 0.0))));
        update(&mut world, 3.0);
        assert!(splash_at(&mut world, Vec3::new(1.0, 0.5, 1.0), 0.5), "under the pond's surface");
        let made: Vec<Vec3> = world.query_ref::<(&Transform, &Splash)>().into_iter().map(|(at, _)| at.position).collect();
        assert_eq!(made, vec![Vec3::new(1.0, 2.0, 1.0)], "at its surface over the point");
        assert!(!splash_at(&mut world, Vec3::new(50.0, 0.0, 0.0), 0.5), "no water there");
    }

    #[test]
    fn a_buoyant_ball_floats_on_block_water_and_one_without_sinks_through_it_to_the_ground() {
        use voxear_terrain::blocks::{
            BlockMap, BlockWorld,
            file::{BlockFile, BlockKind},
        };
        let mut world = World::new();
        let mut file = BlockFile::default();
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let (_, faces) = file.faces();
        // A floor of stone 16 blocks a side under water 4 deep: its top at 0 in the world, the floor's at -4.
        let mut map = BlockMap::default();
        for z in 0..16 {
            for x in 0..16 {
                map.set([x, 0, z], 3);
                for y in 1..5 {
                    map.set([x, y, z], 4);
                }
            }
        }
        world.spawn((BlockWorld::new(map, &faces, 1.0), Transform::from_position(Vec3::new(-8.0, -5.0, -8.0))));
        let sphere = Collider { shape: ColliderShape::Sphere, radius: 0.5, ..Collider::default() };
        let body = Rigidbody { mass: 10.0, ..Rigidbody::default() };
        let floating =
            world.spawn((Buoyancy::default(), sphere, body, Transform::from_position(Vec3::new(-3.0, 3.0, 0.0))));
        let sinking = world.spawn((sphere, body, Transform::from_position(Vec3::new(3.0, 3.0, 0.0))));
        play(&mut world, 8.0);
        let height = |entity: Entity| world.get::<Transform>(entity).unwrap().position.y;
        assert!(height(floating).abs() < 0.2, "half under, at the water's top: {}", height(floating));
        assert!((height(sinking) + 3.5).abs() < 0.1, "through the water onto the floor: {}", height(sinking));
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
    fn a_crate_rides_a_stormy_sea_up_and_down_its_waves() {
        let mut world = World::new();
        world.spawn((Ocean { weather: voxear_render::Weather::Stormy, ..Ocean::default() }, Transform::default()));
        let floating = crate_over(&mut world, Vec3::new(0.0, 1.0, 0.0));
        let mut physics = PhysicsScene::new();
        let dt = 1.0 / 60.0;
        let (mut off, mut rise, mut count) = (0.0f32, 0.0f32, 0);
        for step in 0..(20.0 / dt) as usize {
            update(&mut world, f64::from(step as f32 * dt));
            voxear_scene::propagate_transforms(&mut world);
            float(&world, &mut physics);
            physics.step(&mut world, dt);
            // Once settled: how far its middle is from the surface it floats on, against how far that surface moves.
            if step as f32 * dt > 5.0 {
                let at = world.get::<Transform>(floating).unwrap().position;
                let water = sample(&world, at).unwrap().height;
                off += (at.y - water).powi(2);
                rise += water.powi(2);
                count += 1;
            }
        }
        let (off, rise) = ((off / count as f32).sqrt(), (rise / count as f32).sqrt());
        assert!(off < rise * 0.25, "its middle {off} m from the surface, which moves {rise} m");
    }

    #[test]
    fn an_ocean_blown_by_the_wind_zone_turns_as_it_turns() {
        use voxear_render::WindSource;
        let mut world = World::new();
        let ocean = Ocean { wind_source: WindSource::WindZone, transition: 4.0, ..Ocean::default() };
        let sea = world.spawn((ocean, Transform::default()));
        let east = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let zone = world.spawn((WindZone { strength: 2.0, ..WindZone::default() }, Transform::new(Vec3::ZERO, east, Vec3::ONE)));
        update(&mut world, 1.0);
        let now = |world: &World| world.get::<SeaNow>(sea).unwrap().0;
        assert!(now(&world).direction[0] > 0.99, "along the Wind Zone: {:?}", now(&world).direction);
        assert!((now(&world).wind.speed - 14.0).abs() < 1e-3, "as hard as it blows: {}", now(&world).wind.speed);
        // Turned north, the sea follows over its transition, not at once.
        world.get_mut::<Transform>(zone).unwrap().rotation = Quat::IDENTITY;
        update(&mut world, 2.0);
        assert!(now(&world).direction[0] > 0.99, "not turned yet: {:?}", now(&world).direction);
        update(&mut world, 4.0);
        let [x, z] = now(&world).direction;
        assert!(x > 0.1 && z > 0.1, "turning: {x} {z}");
        update(&mut world, 6.5);
        assert!(now(&world).direction[1] > 0.99, "turned: {:?}", now(&world).direction);
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
    fn a_flooded_room_lies_calm_under_its_roof_though_the_sea_outside_is_stormy() {
        let mut world = World::new();
        world.spawn((Ocean { weather: voxear_render::Weather::Stormy, ..Ocean::default() }, Transform::default()));
        // A floor 0.4 m under the surface, walled round and roofed six metres up, the open sea outside.
        let block = |at: Vec3, size: [f32; 3]| (Collider { size, ..Collider::default() }, Transform::from_position(at));
        world.spawn(block(Vec3::new(0.0, -0.9, 0.0), [20.0, 1.0, 20.0]));
        world.spawn(block(Vec3::new(0.0, 6.25, 0.0), [20.0, 0.5, 20.0]));
        for (at, size) in [
            (Vec3::new(0.0, 2.5, 10.0), [20.0, 6.0, 0.5]),
            (Vec3::new(0.0, 2.5, -10.0), [20.0, 6.0, 0.5]),
            (Vec3::new(10.0, 2.5, 0.0), [0.5, 6.0, 20.0]),
            (Vec3::new(-10.0, 2.5, 0.0), [0.5, 6.0, 20.0]),
        ] {
            world.spawn(block(at, size));
        }
        let mut physics = PhysicsScene::new();
        update(&mut world, 12.0);
        voxear_scene::propagate_transforms(&mut world);
        physics.step(&mut world, 1.0 / 60.0);
        let shallows = sea_shallows(&world, Some(&physics), 0.0, Vec3::new(0.0, 0.0, 8.0));
        assert!((shallows.depth - 0.4).abs() < 0.05, "as deep as the floor, not the roof: {}", shallows.depth);
        assert!(shallows.reach < 0.1, "near the wall, the sea past it does not reach in: {}", shallows.reach);
        let swing = |x: f32| {
            let at = |i: i32| Vec3::new(x, 0.0, i as f32 * 0.4 - 8.0);
            let height = |i: i32| sample_wet(&world, Some(&physics), at(i)).unwrap().height;
            (0..40).map(|i| height(i).abs()).fold(0.0f32, f32::max)
        };
        let (inside, open) = (swing(0.0), swing(40.0));
        assert!(inside < open * 0.1, "waves in the room {inside}, out at sea {open}");
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
