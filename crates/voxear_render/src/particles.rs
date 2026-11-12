//! Particles: a Particle System on an Obj sends out soft dots of light or
//! colour, sparks, smoke, spray or dust, drawn after glass by
//! `shaders/particles.slang`.
//!
//! A system runs one of two ways, whichever its settings need, and is
//! drawn the same either way. While where a particle is depends on nothing
//! but its age, every particle is worked out where it is drawn from its
//! system's settings, its own number and how long the system has been
//! going, with nothing kept between frames. A particle's slot comes round
//! again once its last particle has died, so a system costs the same however
//! long it runs, every machine draws the same particles, and they move with
//! their Obj. Once a setting needs more (the wind, turbulence, particles
//! that stay behind in the world as their Obj moves on, that meet what
//! stands in their way or are thrown about by forces), the system is
//! simulated: its particles are kept on the GPU and stepped each frame (see
//! `particle_sim`). [`ParticleSystem::simulated_for`] says which, and why.
//!
//! Particles may leave trails, a tail each along the way it has come or one
//! ribbon through them all ([`ParticleTrails`]), drawn as strips with the
//! particles' own pixels; and a system may draw each particle as a mesh
//! (`particle_meshes`), in a pass of its own before the flat ones that draws
//! into the depth as well, so meshes hide one another and what is behind
//! them.

use serde::{Deserialize, Serialize};
use voxear_core::curve::{Curve, Gradient};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

use voxear_rhi::vk;

use crate::{
    effects::{EffectRef, ParticleEffects, ParticleTexture},
    gpu::{GpuParticleSystem, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    particle_meshes::{MeshDraw, ParticleMesh},
    passes::{attachment, begin_rendering, bind},
    pipelines::Pipelines,
};

/// The most particle systems a frame draws, the nearest the camera.
pub const MAX_PARTICLE_SYSTEMS: usize = 256;
/// The most particles one system has alive at once, while it is not
/// simulated.
pub const MAX_PARTICLES: u32 = 16384;
/// The most particles one simulated system has alive at once.
pub const MAX_SIMULATED_PARTICLES: u32 = 1 << 20;

/// Where a Particle System's particles start from, and which way they go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmitShape {
    /// From its Obj's middle, every way.
    Point,
    /// From inside a ball, outwards.
    Sphere,
    /// From a disc, up its Obj's Y, spreading out to its angle.
    #[default]
    Cone,
    /// From inside a box, up its Obj's Y.
    Box,
}

impl EmitShape {
    pub const ALL: [Self; 4] = [Self::Point, Self::Sphere, Self::Cone, Self::Box];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Point => "Point",
            Self::Sphere => "Sphere",
            Self::Cone => "Cone",
            Self::Box => "Box",
        }
    }
}

/// What a Particle System's particles move in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleSpace {
    /// Its Obj's own space: they move and turn with it, as a torch's flame
    /// stays on the torch.
    #[default]
    Local,
    /// The world: once out they stay where they are as the Obj moves on, as
    /// the smoke a torch leaves behind it.
    World,
}

impl ParticleSpace {
    pub const ALL: [Self; 2] = [Self::Local, Self::World];

    fn is_local(&self) -> bool {
        *self == Self::Local
    }

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::World => "World",
        }
    }
}

/// What a Particle System's particles meet on their way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleCollision {
    /// Nothing: they pass through everything.
    #[default]
    Off,
    /// What the main view sees, by its depth: the cheapest, and right for
    /// sparks, rain and debris in front of the player. What is out of view
    /// or hidden behind something stops nothing.
    Depth,
}

impl ParticleCollision {
    pub const ALL: [Self; 2] = [Self::Off, Self::Depth];

    fn is_off(&self) -> bool {
        *self == Self::Off
    }

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Depth => "Depth",
        }
    }
}

/// What a Particle System's particles leave behind them as they go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleTrails {
    #[default]
    Off,
    /// Each particle draws a tail along the way it has come, narrowing and
    /// fading to its end: sparks, embers, shooting stars.
    Particles,
    /// The particles are joined into one ribbon in the order they were sent
    /// out, and not drawn themselves: the streak behind a spell, a blade or
    /// a rocket, from a system in the World space on an Obj that moves.
    Ribbon,
}

impl ParticleTrails {
    pub const ALL: [Self; 3] = [Self::Off, Self::Particles, Self::Ribbon];

    fn is_off(&self) -> bool {
        *self == Self::Off
    }

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Particles => "Particles",
            Self::Ribbon => "Ribbon",
        }
    }
}

/// How many points of its way a particle with a trail keeps
/// (`PARTICLE_TRAIL_POINTS` in the shaders), and how many particles' room
/// in the pool they take, three points to a particle's.
pub const TRAIL_POINTS: u32 = 15;
pub const TRAIL_ROOM: u32 = 5;

/// How a Particle System's particles show over what is behind them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleBlend {
    /// Covering it as much as they are opaque: smoke, dust, spray.
    #[default]
    Alpha,
    /// Adding their light to it: sparks, fire, magic.
    Additive,
}

impl ParticleBlend {
    pub const ALL: [Self; 2] = [Self::Alpha, Self::Additive];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Alpha => "Alpha",
            Self::Additive => "Additive",
        }
    }
}

/// A source of particles, sent out from its Obj's shape at a steady rate
/// and in a burst each time it starts over, each living a while, flying
/// out, falling and slowing, growing or shrinking and changing colour as it
/// ages. Ranges are `[least, most]`, each particle somewhere between.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParticleSystem {
    /// How long one round lasts, in seconds; it starts over after it when it
    /// loops, and stops sending out particles when it does not.
    pub duration: f32,
    pub looping: bool,
    /// How many particles it sends out each second.
    pub rate: f32,
    /// How many it sends out at once as each round starts.
    pub burst: u32,
    /// How long each lives, in seconds.
    pub lifetime: [f32; 2],
    /// How fast each sets out, in metres a second.
    pub speed: [f32; 2],
    /// How wide each is as it starts, in metres.
    pub size: [f32; 2],
    /// How wide each is as it dies, against how wide it started.
    pub size_end: f32,
    /// Colour and opacity as each starts, linear; and as it dies.
    pub color: [f32; 4],
    pub color_end: [f32; 4],
    /// How much brighter than its colour each glows, for sparks and fire.
    pub intensity: f32,
    pub shape: EmitShape,
    /// The Sphere's or Cone's radius, in metres.
    pub radius: f32,
    /// How far out from straight up a Cone sends them, in degrees.
    pub angle: f32,
    /// The Box's size, in metres.
    pub box_size: [f32; 3],
    /// How much gravity pulls them down: 1 as hard as it pulls anything, 0
    /// not at all, less than 0 up, as smoke rises.
    pub gravity: f32,
    /// How quickly the air slows them, each second.
    pub drag: f32,
    /// How fast each turns, either way, in degrees a second.
    pub spin: f32,
    pub blend: ParticleBlend,
    /// Lit by the sun, the sky and lamps, as smoke and dust are; or showing
    /// their colour as it is, as sparks and fire do.
    pub lit: bool,
    /// How far, in metres, they fade out before what they meet, rather than
    /// cutting along it.
    pub softness: f32,
    /// Which particles: two systems alike but for this send out different
    /// ones.
    pub seed: u32,
    /// How wide each is over its life, times how wide it started: with keys,
    /// in place of growing or shrinking to its end size.
    #[serde(skip_serializing_if = "Curve::is_empty")]
    pub size_curve: Curve,
    /// How opaque each is over its life, times how opaque it started: with
    /// keys, in place of fading to its end opacity.
    #[serde(skip_serializing_if = "Curve::is_empty")]
    pub opacity_curve: Curve,
    /// How fast each flies over its life, times how fast it set out.
    #[serde(skip_serializing_if = "Curve::is_empty")]
    pub speed_curve: Curve,
    /// Each one's colour over its life, linear: with keys, in place of
    /// fading from its colour to its end colour.
    #[serde(skip_serializing_if = "Gradient::is_empty")]
    pub color_gradient: Gradient,
    /// What they move in: with their Obj, or left behind in the world.
    #[serde(skip_serializing_if = "ParticleSpace::is_local")]
    pub space: ParticleSpace,
    /// How much of the wind they pick up: 0 none of it, 1 carried along at
    /// its speed, as smoke is.
    #[serde(skip_serializing_if = "is_zero")]
    pub wind: f32,
    /// How fast swirls in the air stir them about, in metres a second.
    #[serde(skip_serializing_if = "is_zero")]
    pub turbulence: f32,
    /// How wide those swirls are, in metres.
    #[serde(skip_serializing_if = "is_one")]
    pub turbulence_size: f32,
    /// What they meet on their way, and bounce off or stop at.
    #[serde(skip_serializing_if = "ParticleCollision::is_off")]
    pub collision: ParticleCollision,
    /// How much of its speed into what it meets a particle keeps, away
    /// from it: 0 stops dead, 1 bounces as fast as it came.
    #[serde(skip_serializing_if = "is_bounce")]
    pub bounce: f32,
    /// How much of its speed along what it meets it loses each time: 0
    /// slides on, 1 stops.
    #[serde(skip_serializing_if = "is_friction")]
    pub friction: f32,
    /// How much of the life it has left it loses each time it strikes
    /// something: 1 and it dies there.
    #[serde(skip_serializing_if = "is_zero")]
    pub life_lost: f32,
    /// How much Force Fields, blasts and what passes by move them: 0 not
    /// at all.
    #[serde(skip_serializing_if = "is_zero")]
    pub forces: f32,
    /// What each is drawn with: the soft round Dot, another built-in
    /// texture, or one of the project's pictures.
    #[serde(skip_serializing_if = "ParticleTexture::is_dot")]
    pub texture: ParticleTexture,
    /// How many pictures a project's picture holds, across and down, as a
    /// flipbook does; a built-in texture knows its own.
    #[serde(skip_serializing_if = "is_one_tile")]
    pub tiles: [u32; 2],
    /// Each plays through its texture's pictures over its life, rather
    /// than keeping one of them.
    #[serde(skip_serializing_if = "is_false")]
    pub animate: bool,
    /// How much longer each is drawn along the way it is going, in metres
    /// for each metre a second: sparks and rain as streaks.
    #[serde(skip_serializing_if = "is_zero")]
    pub stretch: f32,
    /// How long after it starts before it sends anything out, in seconds.
    #[serde(skip_serializing_if = "is_zero")]
    pub delay: f32,
    /// For a picture of the project's lit from six sides, the second of its
    /// two maps: how bright each pixel is lit from the left, from below and
    /// from in front, as the texture says for the right, the top and
    /// behind. The built-in Puff has its own.
    #[serde(skip_serializing_if = "ParticleTexture::is_dot")]
    pub light_map: ParticleTexture,
    /// How bright a light it casts on what is round it, as a Point Light's
    /// intensity: 0 none. A stream's flickers; a burst's flashes and fades.
    #[serde(skip_serializing_if = "is_zero")]
    pub light: f32,
    /// How far that light reaches, in metres.
    #[serde(skip_serializing_if = "is_light_range")]
    pub light_range: f32,
    /// What its particles leave behind them: a tail each, or one ribbon
    /// through them all.
    #[serde(skip_serializing_if = "ParticleTrails::is_off")]
    pub trails: ParticleTrails,
    /// How long a particle's tail is, in seconds of the way it has come.
    #[serde(skip_serializing_if = "is_trail_time")]
    pub trail_time: f32,
    /// How wide a tail or the ribbon is, against the particles.
    #[serde(skip_serializing_if = "is_trail_width")]
    pub trail_width: f32,
    /// A mesh each particle is drawn as, tumbling as it spins, as wide as
    /// its size across, in place of a flat picture: debris, shell cases,
    /// stones. A built-in one, or one of the project's models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<ParticleMesh>,
    /// An effect played in place of all of the above: a starter, or one of
    /// the project's `.particles` assets, each of its emitters a system of
    /// its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectRef>,
}

/// How long a tail is and how wide a trail is, to begin with.
const TRAIL_TIME: f32 = 0.5;
const TRAIL_WIDTH: f32 = 0.5;

fn is_trail_time(value: &f32) -> bool {
    *value == TRAIL_TIME
}

fn is_trail_width(value: &f32) -> bool {
    *value == TRAIL_WIDTH
}

/// How far a system's light reaches, to begin with.
const LIGHT_RANGE: f32 = 8.0;

fn is_light_range(value: &f32) -> bool {
    *value == LIGHT_RANGE
}

fn is_one_tile(tiles: &[u32; 2]) -> bool {
    *tiles == [1, 1]
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The bounce and friction a system starts with.
const BOUNCE: f32 = 0.4;
const FRICTION: f32 = 0.2;

fn is_bounce(value: &f32) -> bool {
    *value == BOUNCE
}

fn is_friction(value: &f32) -> bool {
    *value == FRICTION
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

impl Component for ParticleSystem {}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self {
            duration: 5.0,
            looping: true,
            rate: 10.0,
            burst: 0,
            lifetime: [2.0, 3.0],
            speed: [1.0, 2.0],
            size: [0.2, 0.3],
            size_end: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
            color_end: [1.0, 1.0, 1.0, 0.0],
            intensity: 1.0,
            shape: EmitShape::Cone,
            radius: 0.1,
            angle: 20.0,
            box_size: [1.0, 1.0, 1.0],
            gravity: 0.0,
            drag: 0.0,
            spin: 0.0,
            blend: ParticleBlend::Alpha,
            lit: true,
            softness: 0.3,
            seed: 0,
            size_curve: Curve::default(),
            opacity_curve: Curve::default(),
            speed_curve: Curve::default(),
            color_gradient: Gradient::default(),
            space: ParticleSpace::Local,
            wind: 0.0,
            turbulence: 0.0,
            turbulence_size: 1.0,
            collision: ParticleCollision::Off,
            bounce: BOUNCE,
            friction: FRICTION,
            life_lost: 0.0,
            forces: 0.0,
            texture: ParticleTexture::default(),
            tiles: [1, 1],
            animate: false,
            stretch: 0.0,
            delay: 0.0,
            light_map: ParticleTexture::default(),
            light: 0.0,
            light_range: LIGHT_RANGE,
            trails: ParticleTrails::Off,
            trail_time: TRAIL_TIME,
            trail_width: TRAIL_WIDTH,
            mesh: None,
            effect: None,
        }
    }
}

impl ParticleSystem {
    /// How many particles it has alive at most: its burst, and enough slots
    /// for its steady stream to come round once the longest lived has died.
    pub fn slots(&self) -> u32 {
        let stream = (self.rate.max(0.0) * self.lifetime[0].max(self.lifetime[1]).max(0.0)).ceil() as u32;
        (self.burst + stream).min(MAX_PARTICLES)
    }

    /// What makes it a simulated system, its particles kept on the GPU and
    /// stepped each frame, rather than worked out from their age alone:
    /// each setting that needs it, by the name the editor shows; none for a
    /// system that is not.
    pub fn simulated_for(&self) -> Vec<&'static str> {
        let needs = [
            (self.space == ParticleSpace::World, "Space"),
            (self.wind > 0.0, "Wind"),
            (self.turbulence > 0.0, "Turbulence"),
            (self.collision != ParticleCollision::Off, "Collision"),
            (self.forces > 0.0, "Forces"),
            (self.trails != ParticleTrails::Off, "Trails"),
        ];
        needs
            .into_iter()
            .filter(|(needs, _)| *needs)
            .map(|(_, name)| name)
            .collect()
    }

    pub fn is_simulated(&self) -> bool {
        !self.simulated_for().is_empty()
    }

    /// How many particles a simulated system has room for: its stream as
    /// long as the longest lived lives, and each burst still out, one for
    /// every round that starts in that time.
    pub fn capacity(&self) -> u32 {
        let longest = self.lifetime[0].max(self.lifetime[1]).max(0.01);
        let stream = (self.rate.max(0.0) * longest).ceil() as u64;
        let rounds = if self.looping { (longest / self.duration.max(0.01)).ceil().max(1.0) as u64 } else { 1 };
        (u64::from(self.burst) * rounds + stream).min(u64::from(MAX_SIMULATED_PARTICLES)) as u32
    }

    /// How many particles' room in the pool a simulated system takes: its
    /// particles', and their tails' points after them.
    pub fn room(&self) -> u32 {
        let tails = if self.trails == ParticleTrails::Particles { TRAIL_ROOM } else { 0 };
        self.capacity().saturating_mul(1 + tails)
    }

    /// How long between the points a tail keeps, in seconds.
    pub(crate) fn trail_interval(&self) -> f32 {
        self.trail_time.max(0.01) / (TRAIL_POINTS - 1) as f32
    }
}

/// Where a simulated system's particles are in the pool this frame (see
/// `particle_sim`): its first, how many it has room for, and the place of
/// its oldest, which is drawn first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placed {
    pub first: u32,
    pub capacity: u32,
    pub oldest: u32,
}

/// This frame's simulated systems that have particles out, by their Objs
/// and which of their effect's emitters they are.
pub(crate) type PlacedSystems = std::collections::HashMap<(Entity, u32), Placed>;

/// The texture a system draws with as the shaders find it; `None` for the
/// Dot, or for a picture not loaded yet, which draws as the Dot.
pub(crate) type FindTexture<'a> = &'a dyn Fn(ParticleTexture) -> Option<u32>;

/// What finds what a system is drawn with: any texture, the built-in Puff's
/// two maps for being lit from six sides, and a mesh, none for a model not
/// loaded yet.
#[derive(Clone, Copy)]
pub(crate) struct Looks<'a> {
    pub find: FindTexture<'a>,
    pub puff_lit: Option<[u32; 2]>,
    pub mesh: &'a dyn Fn(ParticleMesh) -> Option<MeshDraw>,
}

/// How one of a frame's systems is drawn: how many particles it has room
/// for, the trails they leave, and for one whose particles are meshes, the
/// mesh's first index and how many it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawn {
    pub count: u32,
    pub trails: ParticleTrails,
    pub mesh: Option<(u32, u32)>,
}

/// When a Particle System started, and stopped, in the renderer's time: set
/// by the engine as it plays and stops it. Without one, it has been going
/// since time began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticlePlayback {
    pub started: f32,
    pub stopped: Option<f32>,
}

impl Component for ParticlePlayback {}

/// The time particles run by, in seconds, when it is not the renderer's
/// own: a resource of the world, kept by whoever plays them, so particles
/// can be paused, slowed or stepped while everything else carries on. A
/// Particle System's playback is in this time. Without it, the time the
/// frame is rendered at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleTime(pub f32);

/// The time particles run by this frame, rendered at `time`.
pub(crate) fn time_of(world: &World, time: f32) -> f32 {
    world.resource::<ParticleTime>().map_or(time, |particles| particles.0)
}

/// The spray over a Stream's rapids, as `gather` lists it: its system, the
/// Stream's body, how many points the Stream has and how many drops.
pub type RapidsSpray = (u32, u32, u32, u32);

/// The most Streams with rapids that throw up spray in a frame, the nearest
/// the camera.
const SPRAYING_STREAMS: usize = 4;

/// This frame's Particle Systems, the farthest from `camera` first, so the
/// nearer draw over them, and how many particles each draws, a simulated
/// one from where `placed` has its particles, or not at all with none out;
/// then, listed
/// after them, the sea's spray and how many drops it has, when the first
/// Ocean has any; then the spray over the rapids of the Streams of water
/// nearest the camera among `bodies`.
pub(crate) fn gather(
    world: &World,
    camera: Vec3,
    time: f32,
    bodies: &crate::water::WaterBodies,
    (placed, textures): (&PlacedSystems, Looks<'_>),
) -> (Vec<GpuParticleSystem>, Vec<Drawn>, Option<u32>, Vec<RapidsSpray>) {
    let query =
        world.query_ref::<(Entity, &ParticleSystem, &Transform, Option<&WorldTransform>, Option<&ParticlePlayback>)>();
    let effects = world.resource::<ParticleEffects>();
    let mut systems: Vec<(f32, GpuParticleSystem, Drawn)> = Vec::new();
    for (entity, system, transform, at, playback) in query.without::<Destroying>() {
        let model = at.map_or_else(|| transform.matrix(), |w| w.0);
        let distance = model.w_axis.truncate().distance_squared(camera);
        // Its effect's emitters, each a system of its own in the order the effect lists them, or itself.
        for (emitter, system) in (0..).zip(crate::effects::emitters(effects, system)) {
            let drawn = textured(gpu(system, model, time, playback.copied()), system, textures);
            let mut drawn = GpuParticleSystem { seed: emitter_seed(drawn.seed, emitter), ..drawn };
            // Drawn as meshes: nothing until its model is loaded.
            let mesh = system.mesh.map(|mesh| (textures.mesh)(mesh));
            if let Some(mesh) = mesh {
                let Some(mesh) = mesh else { continue };
                (drawn.mesh, drawn.mesh_material, drawn.flags) = (mesh.mesh, mesh.material, drawn.flags | MESH);
            }
            let mesh = mesh.flatten().map(|mesh| (mesh.first_index, mesh.index_count));
            if !system.is_simulated() {
                if system.slots() > 0 {
                    systems.push((distance, drawn, Drawn { count: system.slots(), trails: ParticleTrails::Off, mesh }));
                }
                continue;
            }
            let Some(at) = placed.get(&(entity, emitter)) else { continue };
            let drawn = GpuParticleSystem { pool_first: at.first, capacity: at.capacity, head: at.oldest, ..drawn };
            systems.push((distance, drawn, Drawn { count: at.capacity, trails: system.trails, mesh }));
        }
    }
    // The farthest first; one Obj's emitters stay in their effect's order.
    systems.sort_by(|a, b| b.0.total_cmp(&a.0));
    let over = systems.len().saturating_sub(MAX_PARTICLE_SYSTEMS - 1 - SPRAYING_STREAMS);
    systems.drain(..over);
    let (mut gpus, counts): (Vec<GpuParticleSystem>, Vec<Drawn>) =
        systems.into_iter().map(|(_, gpu, drawn)| (gpu, drawn)).unzip();
    let seas = world.query_ref::<(&crate::Ocean, &Transform, Option<&WorldTransform>)>();
    let sea = seas.without::<Destroying>().into_iter().next().map(|(ocean, transform, at)| {
        (ocean.spray, at.map_or(transform.position.y, |w| w.0.w_axis.y))
    });
    let spray = sea.filter(|(amount, _)| *amount > 0.0).map(|(amount, level)| {
        let drops = sea_spray(amount);
        gpus.push(gpu(&drops, Mat4::from_translation(Vec3::new(0.0, level, 0.0)), time, None));
        drops.slots()
    });
    // The Streams are listed after the Pools, the nearest first.
    let mut rapids = Vec::new();
    let streams = (bodies.pools..).zip(&bodies.bodies[bodies.pools as usize..]).take(bodies.streams.len());
    for (body, stream) in streams.filter(|(_, stream)| stream.liquid == crate::water_bodies::Liquid::Water as u32) {
        let (first, count) = (stream.shape as usize, stream.size[0] as usize);
        let points = &bodies.points[first..first + count];
        // How many square metres of white water it has, about.
        let white: f32 = points.windows(2).map(|pair| {
            let run = Vec3::from(pair[1].position).distance(Vec3::from(pair[0].position));
            pair[0].foam * pair[0].foam * pair[0].width * run
        }).sum();
        if white < 1.0 || rapids.len() == SPRAYING_STREAMS {
            continue;
        }
        let drops = rapids_spray(white);
        rapids.push((gpus.len() as u32, body, count as u32, drops.slots()));
        gpus.push(gpu(&drops, Mat4::IDENTITY, time, None));
    }
    (gpus, counts, spray, rapids)
}

/// The spray over `white` square metres of a Stream's white water: many
/// fine drops thrown up a metre or so and falling back, each too small to
/// show as a puff.
fn rapids_spray(white: f32) -> ParticleSystem {
    ParticleSystem {
        rate: (white * 60.0).min(9000.0),
        lifetime: [0.35, 0.8],
        speed: [1.2, 3.2],
        size: [0.02, 0.06],
        size_end: 1.5,
        color: [1.0, 1.0, 1.0, 0.6],
        color_end: [1.0, 1.0, 1.0, 0.0],
        gravity: 1.0,
        drag: 0.7,
        softness: 0.05,
        seed: 0x5712EA,
        ..ParticleSystem::default()
    }
}

/// The sea's spray as a system, as much as an Ocean's Spray says: puffs of
/// fine drops thrown up a few metres, spreading into mist as they fall back,
/// lit, fading.
fn sea_spray(amount: f32) -> ParticleSystem {
    ParticleSystem {
        rate: 600.0 * amount,
        lifetime: [0.8, 1.6],
        speed: [1.5, 4.0],
        size: [0.15, 0.4],
        size_end: 3.0,
        color: [1.0, 1.0, 1.0, 0.4],
        color_end: [1.0, 1.0, 1.0, 0.0],
        gravity: 1.0,
        drag: 0.8,
        softness: 0.1,
        seed: 0x5EA,
        ..ParticleSystem::default()
    }
}

/// The particles a frame draws, after the fog (see `post_passes`): how many
/// each Particle System draws, how many drops the sea's spray has, the
/// spray over Streams' rapids, and what they draw into and read.
#[derive(Clone)]
pub struct Draw {
    /// The simulated particles' step, recorded before they are drawn: only
    /// in the main view.
    pub step: Option<Step>,
    pub counts: Vec<Drawn>,
    pub spray: Option<u32>,
    pub rapids: Vec<RapidsSpray>,
    pub targets: Targets,
}

/// A frame's step of the simulated particles, as the main view records it
/// (see `particle_sim`): how many workgroups step them, the counters they
/// add to and where those are copied out, and where the pass finds its
/// systems, the chunks it steps, what it reads of the frame and its counters.
#[derive(Clone, Copy)]
pub struct Step {
    pub groups: u32,
    pub counters: vk::Buffer,
    pub counted: vk::Buffer,
    pub counted_id: BufferId,
    pub systems: u64,
    pub chunks: u64,
    pub frame: u64,
    pub counters_address: u64,
}

/// What the particles' pass draws into and reads.
#[derive(Clone)]
pub struct Targets {
    pub hdr: ImageId,
    /// The scene's depth, which the particles test themselves against and
    /// fade before.
    pub depth: ImageId,
    /// What lighting reads besides: the shadow atlas, the GI's probes and
    /// the atmosphere's tables.
    pub sampled: Vec<ImageId>,
    pub clusters: BufferId,
    /// The pool of simulated particles, when any are drawn.
    pub pool: Option<BufferId>,
    /// Every mesh's indices, for the systems whose particles are meshes.
    pub index_buffer: vk::Buffer,
    pub extent: vk::Extent2D,
}

/// Records the Particle Systems, each drawn as `counts` says of its
/// particles, in the order they are listed: first those whose particles are
/// meshes, in a pass of their own that draws them into the depth as well, so
/// they hide one another and what is drawn after fades before them; then the
/// rest, a quad a particle with its tail or their ribbon under it; then the
/// sea's spray, listed after them, with as many drops as `spray` says. Each
/// is fogged as far off as it is, through the volumetric fog's integrated
/// volume when there is one.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    draw: Draw,
    fog_volume: Option<ImageId>,
) {
    let Draw { step, counts, spray, rapids, targets } = draw;
    let Targets { hdr, depth, sampled, clusters, pool, index_buffer, extent } = targets;
    let depth_texture = fg.texture(depth);
    if let (Some(step), Some(pool)) = (step, pool) {
        let stepping = (pipelines.particle_sim, Push { textures: [depth_texture.0, 0, 0, 0], ..push });
        record_step(fg, stepping, step, (pool, depth));
    }
    let volume = fog_volume.map_or(crate::gpu::NO_TEXTURE, |volume| fg.texture(volume).0);
    let read: Vec<_> = sampled.iter().chain(&fog_volume).map(|&image| (image, ImageAccess::Sampled)).collect();
    let buffers: Vec<_> = [Some(clusters), pool].into_iter().flatten().map(|b| (b, BufferAccess::ShaderRead)).collect();
    let meshes: Vec<(u32, Drawn)> = (0..).zip(counts.iter().copied()).filter(|(_, drawn)| drawn.mesh.is_some()).collect();
    if !meshes.is_empty() {
        let mut images = vec![(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthAttachment)];
        images.extend(&read);
        let push = Push { textures: [crate::gpu::NO_TEXTURE, volume, 0, 0], ..push };
        fg.add_pass("Particle meshes", &images, &buffers, move |ctx| {
            let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
            let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
            begin_rendering(ctx, extent, &color, Some(&target));
            bind(ctx, pipelines.particle_meshes, vk::PipelineBindPoint::GRAPHICS, &push);
            unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32) };
            for &(system, drawn) in &meshes {
                let Some((first_index, index_count)) = drawn.mesh else { continue };
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: system, ..push });
                unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, index_count, drawn.count, first_index, 0, 0) };
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    let mut images = vec![(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    images.extend(&read);
    let push = Push { textures: [depth_texture.0, volume, 0, 0], ..push };
    fg.add_pass("Particles", &images, &buffers, move |ctx| {
        let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, extent, &color, None);
        // Each system with the pipeline for what it draws, bound again only when it changes.
        let mut bound = vk::Pipeline::null();
        let mut draw = |pipeline: vk::Pipeline, system: u32, vertices: u32| {
            if std::mem::replace(&mut bound, pipeline) != pipeline {
                bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
            }
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: system, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0) };
        };
        for (system, drawn) in (0..).zip(&counts) {
            match drawn.trails {
                ParticleTrails::Off => {}
                ParticleTrails::Particles => draw(pipelines.particle_trails, system, drawn.count * TRAIL_POINTS * 6),
                // The ribbon in place of the particles it joins.
                ParticleTrails::Ribbon => {
                    draw(pipelines.particle_ribbon, system, drawn.count * 6);
                    continue;
                }
            }
            if drawn.mesh.is_none() {
                draw(pipelines.particles, system, drawn.count * 6);
            }
        }
        if let Some(drops) = spray {
            bind(ctx, pipelines.sea_spray, vk::PipelineBindPoint::GRAPHICS, &push);
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: counts.len() as u32, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, drops * 6, 1, 0, 0) };
        }
        if !rapids.is_empty() {
            bind(ctx, pipelines.stream_spray, vk::PipelineBindPoint::GRAPHICS, &push);
        }
        for &(system, body, points, drops) in &rapids {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: system, list: body, level: points, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, drops * 6, 1, 0, 0) };
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// Records the simulated particles' step: every chunk of the pool with
/// particles out, stepped by `shaders/particle_sim.slang` against the view's
/// `depth`, and how many are alive counted and copied out for the CPU.
fn record_step(
    fg: &mut FrameGraph<'_>,
    (pipeline, push): (vk::Pipeline, Push),
    step: Step,
    (pool, depth): (BufferId, ImageId),
) {
    let written = [(pool, BufferAccess::ComputeWrite), (step.counted_id, BufferAccess::TransferWrite)];
    fg.add_pass("Particle simulation", &[(depth, ImageAccess::Sampled)], &written, move |ctx| {
        use vk::{AccessFlags2 as A, PipelineStageFlags2 as S};
        // The counters start from nothing, are added to as the particles are stepped, and are copied out: each
        // after the one before.
        let after = |from: (S, A), to: (S, A)| {
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(from.0)
                .src_access_mask(from.1)
                .dst_stage_mask(to.0)
                .dst_access_mask(to.1)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        };
        let stepping = (S::COMPUTE_SHADER, A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE);
        let size = crate::particle_sim::COUNTERS;
        unsafe { ctx.gpu.device.cmd_fill_buffer(ctx.cmd, step.counters, 0, size, 0) };
        after((S::ALL_TRANSFER, A::TRANSFER_WRITE), stepping);
        bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, step.groups, 1, 1) };
        after(stepping, (S::ALL_TRANSFER, A::TRANSFER_READ));
        let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size };
        unsafe { ctx.gpu.device.cmd_copy_buffer(ctx.cmd, step.counters, step.counted, &[region]) };
    });
    fg.sync_buffer(step.counted_id, BufferAccess::HostRead);
}

/// The flags a system's GPU form carries, as `PARTICLES_*` in
/// `shaders/particles.slang`.
const LOOPING: u32 = 1;
const ADDITIVE: u32 = 2;
const LIT: u32 = 4;
const SIMULATED: u32 = 8;
const WORLD: u32 = 16;
const COLLIDE: u32 = 64;
/// Each particle plays through its texture's pictures over its life.
const ANIMATE: u32 = 128;
/// Lit from six sides by its texture and its light map.
const SIX_WAY: u32 = 256;
/// Each particle draws a tail along its way; or they are joined in one
/// ribbon, which reaches to its Obj while it is still sending them out.
const TRAILS: u32 = 512;
const RIBBON: u32 = 1024;
const RIBBON_FED: u32 = 2048;
/// Each particle is drawn as a mesh.
const MESH: u32 = 4096;
/// Its room in the pool holds none of its own particles yet: only for the
/// pass that steps them.
pub(crate) const FRESH: u32 = 32;

pub(crate) fn gpu(
    system: &ParticleSystem,
    model: Mat4,
    time: f32,
    playback: Option<ParticlePlayback>,
) -> GpuParticleSystem {
    let playback = playback.unwrap_or(ParticlePlayback { started: 0.0, stopped: None });
    // Its own start is its Delay after its playback's.
    let started = playback.started + system.delay.max(0.0);
    let range = |r: [f32; 2]| [r[0].min(r[1]), r[0].max(r[1])];
    let flags = [
        (system.looping, LOOPING),
        (system.blend == ParticleBlend::Additive, ADDITIVE),
        (system.lit, LIT),
        (system.is_simulated(), SIMULATED),
        (system.space == ParticleSpace::World, WORLD),
        (system.collision == ParticleCollision::Depth, COLLIDE),
        (system.animate, ANIMATE),
        (system.trails == ParticleTrails::Particles, TRAILS),
        (system.trails == ParticleTrails::Ribbon, RIBBON),
        (system.trails == ParticleTrails::Ribbon && sending(system, time - started, playback.stopped.map(|s| s - started)), RIBBON_FED),
    ];
    let flags = flags
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |all, (_, flag)| all | flag);
    let lifetime = range(system.lifetime).map(|l| l.max(0.01));
    let interval = system.trail_interval();
    GpuParticleSystem {
        model: crate::gpu::cols(model),
        color: system.color,
        color_end: system.color_end,
        lifetime,
        speed: range(system.speed),
        size: range(system.size).map(|s| s.max(0.0)),
        size_end: system.size_end.max(0.0),
        gravity: system.gravity,
        drag: system.drag.max(0.0),
        rate: system.rate.max(0.0),
        duration: system.duration.max(0.01),
        time: time - started,
        shape: system.shape as u32,
        radius: system.radius.max(0.0),
        angle: system.angle.clamp(0.0, 180.0).to_radians(),
        spin: system.spin.to_radians(),
        box_size: system.box_size.map(|s| s.max(0.0)),
        softness: system.softness.max(0.0),
        burst: system.burst.min(MAX_PARTICLES),
        slots: system.slots(),
        seed: system.seed,
        flags,
        stopped: playback.stopped.map_or(f32::INFINITY, |stopped| stopped - started),
        intensity: system.intensity.max(0.0),
        wind: system.wind.max(0.0),
        turbulence: system.turbulence.max(0.0),
        size_curve: curve_keys(&system.size_curve),
        opacity_curve: curve_keys(&system.opacity_curve),
        speed_curve: curve_keys(&system.speed_curve),
        gradient: gradient_keys(&system.color_gradient),
        curve_counts: [
            system.size_curve.keys().len() as u32,
            system.opacity_curve.keys().len() as u32,
            system.speed_curve.keys().len() as u32,
            system.color_gradient.keys().len() as u32,
        ],
        to_local: crate::gpu::cols(model.inverse()),
        turbulence_size: system.turbulence_size.max(0.01),
        bounce: system.bounce.clamp(0.0, 1.0),
        friction: system.friction.clamp(0.0, 1.0),
        life_lost: system.life_lost.clamp(0.0, 1.0),
        forces: system.forces.max(0.0),
        texture: crate::gpu::NO_TEXTURE,
        light_map: crate::gpu::NO_TEXTURE,
        tiles: [1, 1],
        stretch: system.stretch.max(0.0),
        trail_tick: (time.max(0.0) / interval) as u32,
        trail_part: (time.max(0.0) / interval).fract(),
        trail_width: system.trail_width.max(0.0),
        trail_interval: interval,
        ..GpuParticleSystem::default()
    }
}

/// Whether `system` is still sending particles out `since` seconds after it
/// started, having `stopped` then if ever.
fn sending(system: &ParticleSystem, since: f32, stopped: Option<f32>) -> bool {
    let until = stopped.unwrap_or(f32::INFINITY).min(if system.looping { f32::INFINITY } else { system.duration });
    system.rate > 0.0 && since >= 0.0 && since < until
}

/// The seed emitter `emitter` of an effect draws its particles from, given
/// its own: each emitter's are different, though their seeds be the same.
pub(crate) fn emitter_seed(seed: u32, emitter: u32) -> u32 {
    seed.wrapping_add(emitter.wrapping_mul(0x9E37_79B9))
}

/// `drawn` with the texture `system` draws with, as `textures` finds it.
pub(crate) fn textured(drawn: GpuParticleSystem, system: &ParticleSystem, textures: Looks<'_>) -> GpuParticleSystem {
    // A built-in texture knows how many pictures it holds; a project's picture is told.
    let tiles = match system.texture {
        ParticleTexture::Builtin(texture) => texture.tiles(),
        ParticleTexture::Asset(_) => system.tiles.map(|n| n.clamp(1, 64)),
    };
    // Lit from six sides: the built-in Puff by its own maps, a project's picture by the light map it names.
    let puff = system.texture == ParticleTexture::Builtin(crate::effects::BuiltinTexture::Puff);
    let six = match (system.lit, puff, system.light_map) {
        (true, true, _) => textures.puff_lit,
        (true, false, map @ ParticleTexture::Asset(_)) => (textures.find)(system.texture).zip((textures.find)(map)).map(<[u32; 2]>::from),
        _ => None,
    };
    if let Some([texture, light_map]) = six {
        return GpuParticleSystem { texture, light_map, tiles, flags: drawn.flags | SIX_WAY, ..drawn };
    }
    match (textures.find)(system.texture) {
        Some(texture) => GpuParticleSystem { texture, tiles, ..drawn },
        None => drawn,
    }
}

/// A curve's keys as the shaders read them, two to a row.
fn curve_keys(curve: &Curve) -> [[f32; 4]; 4] {
    let mut rows = [[0.0; 4]; 4];
    for (i, key) in curve.keys().iter().enumerate() {
        rows[i / 2][i % 2 * 2] = key[0];
        rows[i / 2][i % 2 * 2 + 1] = key[1];
    }
    rows
}

/// A gradient's keys as the shaders read them, a row each.
fn gradient_keys(gradient: &Gradient) -> [[f32; 4]; 8] {
    let mut rows = [[0.0; 4]; 8];
    for (row, key) in rows.iter_mut().zip(gradient.keys()) {
        *row = *key;
    }
    rows
}

/// The most lights Particle Systems cast in a frame, the nearest the camera.
pub const MAX_PARTICLE_LIGHTS: usize = 16;

/// How bright a system's light is `since` seconds after its emitter
/// started, from 0 to 1, having `stopped` then if ever: a stream's steady
/// while it sends particles out, with a flicker of its own (by `seed`), and
/// fading as its last die; a burst's full as it goes off and fading as fast
/// again, as a flash does.
fn light_now(system: &ParticleSystem, since: f32, stopped: f32, seed: u32) -> f32 {
    if since < 0.0 {
        return 0.0;
    }
    let duration = system.duration.max(0.01);
    let longest = system.lifetime[0].max(system.lifetime[1]).max(0.01);
    let until = stopped.min(if system.looping { f32::INFINITY } else { duration });
    let mut bright: f32 = 0.0;
    if system.rate > 0.0 {
        // Two slow wobbles and a quick one, never in step with another system's.
        let phase = seed as f32 * 0.37;
        let flicker = 0.8 + 0.1 * (since * 9.0 + phase).sin() + 0.07 * (since * 23.0 + phase * 1.7).sin() + 0.03 * (since * 51.0).sin();
        bright = flicker * (1.0 - ((since - until) / longest).clamp(0.0, 1.0));
    }
    if system.burst > 0 {
        let round = if system.looping { (since.min(until) / duration).floor() * duration } else { 0.0 };
        let gone = ((since - round) / longest).clamp(0.0, 1.0);
        bright = bright.max((1.0 - gone) * (1.0 - gone));
    }
    bright
}

/// The lights this frame's Particle Systems cast, the nearest `camera`
/// first and no more than [`MAX_PARTICLE_LIGHTS`]: a point light at each
/// emitter that has a Light, in its colour as it starts.
pub(crate) fn lights(world: &World, camera: Vec3, time: f32) -> Vec<crate::gpu::GpuLight> {
    use crate::gpu::{GpuLight, LIGHT_POINT, NO_SHADOW, NO_TEXTURE};
    let query =
        world.query_ref::<(Entity, &ParticleSystem, &Transform, Option<&WorldTransform>, Option<&ParticlePlayback>)>();
    let effects = world.resource::<ParticleEffects>();
    let mut found: Vec<(f32, GpuLight)> = Vec::new();
    for (entity, own, transform, at, playback) in query.without::<Destroying>() {
        let position = at.map_or(transform.position, |w| w.0.w_axis.truncate());
        let playback = playback.copied().unwrap_or(ParticlePlayback { started: 0.0, stopped: None });
        for (emitter, system) in (0u32..).zip(crate::effects::emitters(effects, own)) {
            if system.light <= 0.0 || system.light_range <= 0.0 {
                continue;
            }
            let started = playback.started + system.delay.max(0.0);
            let stopped = playback.stopped.map_or(f32::INFINITY, |stopped| stopped - started);
            let seed = emitter_seed(system.seed, emitter) ^ entity.to_bits() as u32;
            let bright = light_now(system, time - started, stopped, seed);
            if bright <= 0.001 {
                continue;
            }
            let [r, g, b] = system.color_gradient.at(0.0).unwrap_or([system.color[0], system.color[1], system.color[2]]);
            let color = Vec3::new(r, g, b) * (system.light * bright);
            let light = GpuLight {
                position: position.to_array(),
                range: system.light_range,
                color: color.to_array(),
                kind: LIGHT_POINT,
                direction: [0.0, 0.0, 1.0],
                cos_outer: 0.0,
                right: [1.0, 0.0, 0.0],
                cos_inner: 0.0,
                size: [0.0; 2],
                cookie: NO_TEXTURE,
                profile: NO_TEXTURE,
                shadow: NO_SHADOW,
                _pad: [0; 3],
            };
            found.push((position.distance_squared(camera), light));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().take(MAX_PARTICLE_LIGHTS).map(|(_, light)| light).collect()
}

/// Textures for tests that draw none.
#[cfg(test)]
const NO_TEXTURES: Looks<'static> = Looks { find: &|_| None, puff_lit: None, mesh: &|_| None };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fire_flickers_and_a_flash_fades_as_fast_as_its_particles_die() {
        let fire = ParticleSystem { rate: 30.0, lifetime: [1.0, 1.0], light: 5.0, ..ParticleSystem::default() };
        let seen: Vec<f32> = (0..40).map(|i| light_now(&fire, i as f32 * 0.05, f32::INFINITY, 3)).collect();
        assert!(seen.iter().all(|bright| (0.55..=1.05).contains(bright)), "always alight: {seen:?}");
        assert!(seen.iter().cloned().fold(0.0, f32::max) - seen.iter().cloned().fold(1.0, f32::min) > 0.1, "and flickering");
        assert!(light_now(&fire, 10.4, 10.0, 3) > 0.3 && light_now(&fire, 11.5, 10.0, 3) == 0.0, "out as its last die");
        let flash = ParticleSystem { rate: 0.0, burst: 3, looping: false, lifetime: [0.1, 0.2], light: 20.0, ..fire };
        assert_eq!((light_now(&flash, -0.1, f32::INFINITY, 0), light_now(&flash, 0.0, f32::INFINITY, 0)), (0.0, 1.0));
        assert!((light_now(&flash, 0.1, f32::INFINITY, 0) - 0.25).abs() < 1e-4, "half way, a quarter as bright");
        assert_eq!(light_now(&flash, 0.3, f32::INFINITY, 0), 0.0);
        // Only emitters with a Light cast one, at their Obj, in their colour, the nearest first.
        let mut world = World::new();
        world.spawn((fire, Transform::from_position(Vec3::new(0.0, 1.0, 9.0))));
        world.spawn((ParticleSystem { color: [1.0, 0.5, 0.0, 1.0], ..fire }, Transform::from_position(Vec3::new(0.0, 1.0, 2.0))));
        world.spawn((ParticleSystem::default(), Transform::default()));
        let cast = lights(&world, Vec3::ZERO, 5.0);
        assert_eq!(cast.len(), 2);
        assert_eq!((cast[0].position, cast[0].range), ([0.0, 1.0, 2.0], LIGHT_RANGE));
        assert!(cast[0].color[1] > 0.0 && cast[0].color[2] == 0.0 && cast[0].color[0] > cast[0].color[1], "{:?}", cast[0].color);
    }

    #[test]
    fn a_system_keeps_slots_for_its_burst_and_its_stream_as_long_as_the_longest_lives() {
        let system = ParticleSystem { rate: 10.0, lifetime: [1.0, 2.5], burst: 5, ..ParticleSystem::default() };
        assert_eq!(system.slots(), 5 + 25);
        let flood = ParticleSystem { rate: 1e6, ..ParticleSystem::default() };
        assert_eq!(flood.slots(), MAX_PARTICLES, "never more than a system can draw");
    }

    #[test]
    fn the_farthest_systems_come_first_and_a_stopped_one_keeps_when_it_stopped() {
        let mut world = World::new();
        world.spawn((ParticleSystem::default(), Transform::from_position(Vec3::new(0.0, 0.0, 2.0))));
        let far = world.spawn((ParticleSystem::default(), Transform::from_position(Vec3::new(0.0, 0.0, 20.0))));
        world.insert(far, ParticlePlayback { started: 3.0, stopped: Some(5.0) });
        let (systems, slots, spray, rapids) = gather(&world, Vec3::ZERO, 10.0, &Default::default(), (&PlacedSystems::new(), NO_TEXTURES));
        assert_eq!(spray, None, "no sea, no spray");
        assert!(rapids.is_empty(), "no Streams, none over rapids");
        assert_eq!(slots.len(), 2);
        assert_eq!(systems[0].model[3][2], 20.0, "the far one first");
        assert_eq!((systems[0].time, systems[0].stopped), (7.0, 2.0), "seven seconds in, stopped at two");
        assert_eq!(systems[1].stopped, f32::INFINITY);
    }

    #[test]
    fn a_stream_with_rapids_throws_up_spray_and_one_that_runs_level_none() {
        use crate::water_bodies::Stream;
        use voxear_scene::{Path, PathPoint};
        let point = |y: f32, z: f32| PathPoint { position: [0.0, y, z], width: 6.0 };
        let level = Path { points: vec![point(0.0, 0.0), point(0.0, 30.0)], closed: false };
        let steep = Path { points: vec![point(0.0, 0.0), point(0.0, 10.0), point(-4.0, 22.0), point(-4.0, 40.0)], closed: false };
        let bodies = |path: &Path| {
            let stream = Stream::default();
            let points = stream.points(path, Mat4::IDENTITY);
            let body = stream.gpu(0, points.len() as u32, None);
            let points = points.iter().map(|p| p.gpu()).collect::<Vec<_>>();
            crate::water::WaterBodies { bodies: vec![body], pools: 0, streams: vec![points.len() as u32], points, dry_meshes: Vec::new() }
        };
        let world = World::new();
        let (systems, _, _, rapids) = gather(&world, Vec3::ZERO, 10.0, &bodies(&level), (&PlacedSystems::new(), NO_TEXTURES));
        assert!(systems.is_empty() && rapids.is_empty(), "a level stream throws up none");
        let steep = bodies(&steep);
        let (systems, _, _, rapids) = gather(&world, Vec3::ZERO, 10.0, &steep, (&PlacedSystems::new(), NO_TEXTURES));
        assert_eq!((systems.len(), rapids.len()), (1, 1));
        let (system, body, points, drops) = rapids[0];
        assert_eq!((system, body, points), (0, 0, steep.streams[0]), "its own system, for the Stream's body and points");
        assert!(drops > 50, "enough drops to see: {drops}");
    }
}
