//! Particles: a Particle System on an Obj sends out soft dots of light or
//! colour, sparks, smoke, spray or dust, drawn after glass by
//! `shaders/particles.slang`.
//!
//! This is the stateless path: every particle is worked out where it is
//! drawn from its system's settings, its own number and how long the system
//! has been going, with nothing kept between frames. A particle's slot comes
//! round again once its last particle has died, so a system costs the same
//! however long it runs, and every machine draws the same particles. They
//! move with their Obj, as particles simulated in its own space do.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

use voxear_rhi::vk;

use crate::{
    gpu::{GpuParticleSystem, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering, bind},
    pipelines::Pipelines,
};

/// The most particle systems a frame draws, the nearest the camera.
pub const MAX_PARTICLE_SYSTEMS: usize = 256;
/// The most particles one system has alive at once.
pub const MAX_PARTICLES: u32 = 16384;

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

/// This frame's Particle Systems, the farthest from `camera` first, so the
/// nearer draw over them, and how many particles each draws; then, listed
/// after them, the sea's spray and how many drops it has, when the first
/// Ocean has any.
pub(crate) fn gather(world: &World, camera: Vec3, time: f32) -> (Vec<GpuParticleSystem>, Vec<u32>, Option<u32>) {
    let query = world.query_ref::<(&ParticleSystem, &Transform, Option<&WorldTransform>, Option<&ParticlePlayback>)>();
    let mut systems: Vec<(f32, GpuParticleSystem, u32)> = query
        .without::<Destroying>()
        .into_iter()
        .filter(|(system, ..)| system.slots() > 0)
        .map(|(system, transform, at, playback)| {
            let model = at.map_or_else(|| transform.matrix(), |w| w.0);
            let distance = model.w_axis.truncate().distance_squared(camera);
            (distance, gpu(system, model, time, playback.copied()), system.slots())
        })
        .collect();
    systems.sort_by(|a, b| b.0.total_cmp(&a.0));
    systems.truncate(MAX_PARTICLE_SYSTEMS - 1);
    let (mut gpus, counts): (Vec<GpuParticleSystem>, Vec<u32>) =
        systems.into_iter().map(|(_, gpu, slots)| (gpu, slots)).unzip();
    let seas = world.query_ref::<(&crate::Ocean, &Transform, Option<&WorldTransform>)>();
    let sea = seas.without::<Destroying>().into_iter().next().map(|(ocean, transform, at)| {
        (ocean.spray, at.map_or(transform.position.y, |w| w.0.w_axis.y))
    });
    let spray = sea.filter(|(amount, _)| *amount > 0.0).map(|(amount, level)| {
        let drops = sea_spray(amount);
        gpus.push(gpu(&drops, Mat4::from_translation(Vec3::new(0.0, level, 0.0)), time, None));
        drops.slots()
    });
    (gpus, counts, spray)
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
/// each Particle System draws, how many drops the sea's spray has, and what
/// they draw into and read.
#[derive(Clone)]
pub struct Draw {
    pub counts: Vec<u32>,
    pub spray: Option<u32>,
    pub targets: Targets,
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
    pub extent: vk::Extent2D,
}

/// Records the Particle Systems, each drawn as `counts` says of its
/// particles, a quad each, in the order they are listed; then the sea's
/// spray, listed after them, with as many drops as `spray` says. Each is
/// fogged as far off as it is, through the volumetric fog's integrated
/// volume when there is one.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    draw: Draw,
    fog_volume: Option<ImageId>,
) {
    let Draw { counts, spray, targets } = draw;
    let Targets { hdr, depth, sampled, clusters, extent } = targets;
    let depth_texture = fg.texture(depth);
    let mut images = vec![(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    images.extend(sampled.iter().chain(&fog_volume).map(|&image| (image, ImageAccess::Sampled)));
    let volume = fog_volume.map_or(crate::gpu::NO_TEXTURE, |volume| fg.texture(volume).0);
    let push = Push { textures: [depth_texture.0, volume, 0, 0], ..push };
    fg.add_pass("Particles", &images, &[(clusters, BufferAccess::ShaderRead)], move |ctx| {
        let color = [vk::RenderingAttachmentInfo::default()
            .image_view(ctx.view(hdr))
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)];
        begin_rendering(ctx, extent, &color, None);
        bind(ctx, pipelines.particles, vk::PipelineBindPoint::GRAPHICS, &push);
        for (system, &count) in counts.iter().enumerate() {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: system as u32, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, count * 6, 1, 0, 0) };
        }
        if let Some(drops) = spray {
            bind(ctx, pipelines.sea_spray, vk::PipelineBindPoint::GRAPHICS, &push);
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { source: counts.len() as u32, ..push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, drops * 6, 1, 0, 0) };
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// The flags a system's GPU form carries, as `PARTICLES_*` in
/// `shaders/particles.slang`.
const LOOPING: u32 = 1;
const ADDITIVE: u32 = 2;
const LIT: u32 = 4;

fn gpu(system: &ParticleSystem, model: Mat4, time: f32, playback: Option<ParticlePlayback>) -> GpuParticleSystem {
    let playback = playback.unwrap_or(ParticlePlayback { started: 0.0, stopped: None });
    let range = |r: [f32; 2]| [r[0].min(r[1]), r[0].max(r[1])];
    let flags = [(system.looping, LOOPING), (system.blend == ParticleBlend::Additive, ADDITIVE), (system.lit, LIT)]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |all, (_, flag)| all | flag);
    let lifetime = range(system.lifetime).map(|l| l.max(0.01));
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
        time: time - playback.started,
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
        stopped: playback.stopped.map_or(f32::INFINITY, |stopped| stopped - playback.started),
        intensity: system.intensity.max(0.0),
        _pad: [0.0; 2],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let (systems, slots, spray) = gather(&world, Vec3::ZERO, 10.0);
        assert_eq!(spray, None, "no sea, no spray");
        assert_eq!(slots.len(), 2);
        assert_eq!(systems[0].model[3][2], 20.0, "the far one first");
        assert_eq!((systems[0].time, systems[0].stopped), (7.0, 2.0), "seven seconds in, stopped at two");
        assert_eq!(systems[1].stopped, f32::INFINITY);
    }
}
