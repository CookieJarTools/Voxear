//! Trails on water: what moves on a water surface leaves a wake behind it.
//! A swimmer leaves ripples and a short trail of foam; a boat, or anything
//! moving across the water, a V of waves that spreads out and fades; and
//! rings show where something goes into the water or comes out of it.
//!
//! The ripples are a wave simulation over a square of water round each
//! view's camera, [`WaterTrails::area`] metres across and
//! [`TrailQuality`] texels a side. They are only drawn, so they live on the
//! GPU, and they are not the same on every machine. Each texel holds how
//! far the water there is raised, how fast it is rising, how much foam lies
//! on it and how much of that is fresh. The square's texels hold the
//! world's cells modulo its size, as the track map's do, so it scrolls with
//! the camera without moving anything. Each frame (`waterTrails*` in
//! `shaders/water_trails.slang`): the cells that came into the square are
//! cleared and everything is carried along the way the water flows
//! ([`TrailFlow`]); then, in a step or a few, the waves run on by the wave
//! equation and die away, and each body that cuts the surface ([`Stir`])
//! pushes the water out of where it now is and lets it back where it was,
//! as much as it displaces, which is the waves it makes moving across the
//! water, going in and coming out; and leaves foam. A body moving faster
//! than the waves run leaves them behind it in a V, the narrower the faster
//! it goes. The water's shaders bend their normals by the slopes and lay
//! the foam over themselves (`trailAt`).
//!
//! The engine finds what stirs the water (`voxear_engine::water_trails`):
//! every Character Controller, every body with a Collider that cuts the
//! surface, and every Obj with a [`WaterWake`], which also tunes or turns
//! off what each makes.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity};
use voxear_rhi::{Bindless, Gpu, ImageDesc, StorageImageId, vk};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    passes::bind,
    pipelines::Pipelines,
};

/// The most bodies that stir the water in a frame, the nearest to the
/// camera first.
pub const MAX_STIRS: usize = 32;
/// How many cells a side the grid of the water's flow has, over the square.
pub const FLOW_CELLS: usize = 16;
/// How fast the ripples run across the water, in metres a second
/// (`TRAIL_WAVE_SPEED` in the shaders): a body faster than this leaves a V.
pub const WAVE_SPEED: f32 = 1.1;
/// The most steps the waves take in a frame; a longer frame steps them more
/// slowly than the clock.
const MAX_STEPS: u32 = 4;
/// How far a body can move in a frame and still stir the water all the way;
/// one moved further, as it is put somewhere new, starts afresh there.
const LONGEST_MOVE: f32 = 4.0;
/// How long, in seconds, the ripples go on after the last body left the
/// water, before the square is let go.
const QUIET_AFTER: f32 = 20.0;
/// How far, in metres, above or below the surface a body's edge may be and
/// still count as cutting it, the share fading out to there.
const CUTTING: f32 = 0.08;

/// How an Obj stirs the water it moves through: how wide its wake is, how
/// hard it stirs and how much foam it leaves. Characters and bodies with a
/// Collider stir the water with none, sized by their bodies; one with a
/// Water Wake goes by it, and one whose Water Wake is switched off stirs
/// nothing. An Obj with no body stirs only with one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WaterWake {
    /// How wide it is where it cuts the water, in metres; 0 for as wide as
    /// its body (half a metre with none).
    pub size: f32,
    /// How hard it stirs the water: 1 as its body would, 0 for no waves.
    pub strength: f32,
    /// How much foam it leaves: 1 as its body would, 0 for none.
    pub foam: f32,
}

impl Component for WaterWake {}

impl Default for WaterWake {
    fn default() -> Self {
        Self { size: 0.0, strength: 1.0, foam: 1.0 }
    }
}

/// How finely the trails are worked out: how many texels a side the square
/// round the camera has, or none at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailQuality {
    Off,
    Low,
    #[default]
    Medium,
    High,
}

impl TrailQuality {
    pub const ALL: [Self; 4] = [Self::Off, Self::Low, Self::Medium, Self::High];

    /// How many texels a side its square has.
    pub fn texels(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Low => 256,
            Self::Medium => 512,
            Self::High => 1024,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

/// How the trails look. Blocky, the water stands in small squares a
/// quarter of a block across, each flat, raised or lowered a step at a time
/// by the ripples under it, and its foam lies square by square.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrailLook {
    /// Blocky on worlds of blocks, smooth on the sea, Pools and Streams.
    #[default]
    MatchWater,
    Smooth,
    Blocky,
}

impl TrailLook {
    pub const ALL: [Self; 3] = [Self::MatchWater, Self::Smooth, Self::Blocky];

    pub fn label(self) -> &'static str {
        match self {
            Self::MatchWater => "Match the Water",
            Self::Smooth => "Smooth",
            Self::Blocky => "Blocky",
        }
    }

    /// As the shaders number it (`TRAIL_MATCH`, `TRAIL_SMOOTH`, `TRAIL_BLOCKY`).
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|&look| look == self).unwrap_or(0) as u32
    }
}

/// The project's settings for trails on water (`[water_trails]` in
/// `voxear.toml`), a world resource: how finely they are worked out, how
/// far round the camera, and how they look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterTrails {
    pub quality: TrailQuality,
    /// How far across the square round the camera is, in metres.
    pub area: f32,
    pub look: TrailLook,
}

impl Default for WaterTrails {
    fn default() -> Self {
        Self { quality: TrailQuality::default(), area: 64.0, look: TrailLook::default() }
    }
}

impl WaterTrails {
    /// The smallest and largest square, in metres.
    pub const AREA: (f32, f32) = (16.0, 512.0);

    /// How wide each texel is, in metres.
    pub fn texel(&self) -> f32 {
        self.area.clamp(Self::AREA.0, Self::AREA.1) / self.quality.texels().max(1) as f32
    }
}

/// A body cutting the water's surface, as the engine finds it each frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stir {
    pub entity: Entity,
    /// Its two ends across the water from its Obj's place: a long body
    /// stirs it as a capsule from one to the other, a round one as a disc.
    pub ends: [Vec3; 2],
    /// How far round its ends it reaches, in metres.
    pub radius: f32,
    /// How high the water's surface is where it is, in the world.
    pub surface: f32,
    /// How far below and above its Obj's place it reaches, in metres.
    pub reach: [f32; 2],
    /// How the water flows where it is, in metres a second: only what it
    /// moves against the water stirs it.
    pub flow: Vec3,
    /// How hard it stirs, and how much foam it leaves (see [`WaterWake`]).
    pub strength: f32,
    pub foam: f32,
    /// How much it treads water as it floats, from 0 to 1: a swimmer's 1,
    /// so rings spread from it as it stays still.
    pub bob: f32,
}

/// What stirs the water this frame, which the engine notes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stirs(pub Vec<Stir>);

/// How the water flows over a grid round the viewer, which the engine
/// works out: the trails are carried along with it, down a river.
#[derive(Clone, Debug, PartialEq)]
pub struct TrailFlow {
    /// Its first cell's corner, in the world, along x and z.
    pub corner: [f32; 2],
    /// How wide each cell is, in metres.
    pub cell: f32,
    /// Each cell's flow along x and z, in metres a second, row by row along
    /// z, `FLOW_CELLS` a row.
    pub cells: Vec<[f32; 2]>,
}

/// A body as the shaders stir the water with it (`TrailStir` in
/// `shaders/common.slang`): the water is pushed out of where it is now
/// and let back where it was, each step, as much as it displaces there.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuStir {
    /// Its two ends' x and z last frame, where the water has carried them
    /// since, then this frame.
    pub from: [f32; 4],
    pub to: [f32; 4],
    /// Its radius, how hard it stirs, how much foam it leaves and how much
    /// it treads water.
    pub size: [f32; 4],
    /// How much of it cut the surface last frame and this frame, how fast it
    /// rises, in metres a second, and how fast it moves across the water.
    pub motion: [f32; 4],
}

/// The trails as the shaders read them (`Trails` in `shaders/common.slang`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTrails {
    pub stirs: [GpuStir; MAX_STIRS],
    /// The water's flow, two cells a row, x and z of each.
    pub flow: [[f32; 4]; FLOW_CELLS * FLOW_CELLS / 2],
    pub flow_corner: [f32; 2],
    pub flow_cell: f32,
    pub count: u32,
    /// The square's corner now and last frame, in texels from the world's
    /// origin.
    pub origin: [i32; 2],
    pub last_origin: [i32; 2],
    pub texel: f32,
    pub texels: u32,
    /// How many steps the waves take this frame, and how long each is.
    pub steps: u32,
    pub step: f32,
    /// The ripples' texture for the water to read, `NO_TEXTURE` for none.
    pub texture: u32,
    /// Whether the square holds nothing to keep yet.
    pub fresh: u32,
    /// How they look, by `TrailLook::index`.
    pub look: u32,
    /// The trails' own clock, in seconds, for what treads water.
    pub clock: f32,
    /// Whether any of the water in the flow's grid flows.
    pub flowing: u32,
    pub _pad: [u32; 3],
}

impl Default for GpuTrails {
    fn default() -> Self {
        Self { texture: NO_TEXTURE, ..bytemuck::Zeroable::zeroed() }
    }
}

impl GpuTrails {
    /// How many passes over the square this frame takes: one to clear and
    /// carry it where it scrolled or flows, then a step each.
    pub fn dispatches(&self) -> u32 {
        u32::from(self.carries()) + self.steps
    }

    /// Whether this frame clears what came into the square or carries it
    /// along the water's flow.
    pub fn carries(&self) -> bool {
        self.fresh != 0 || self.origin != self.last_origin || (self.flowing != 0 && self.steps > 0)
    }
}

/// A smooth step from 0 at `x` of 0 to 1 at 1.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How much of a body reaching from `bottom` to `top` cuts the water's
/// surface at `surface`, from 0 to 1.
pub fn cutting(bottom: f32, top: f32, surface: f32) -> f32 {
    smooth((surface - bottom) / CUTTING + 1.0) * smooth((top - surface) / CUTTING + 1.0)
}

/// What each view remembers of its trails between frames.
#[derive(Debug, Default)]
pub struct TrailMemory {
    /// Where each body was last frame and how much of it cut the surface.
    last: HashMap<Entity, (Vec3, f32)>,
    /// The square's corner, in texels, and the settings it was made with.
    origin: Option<[i32; 2]>,
    made: Option<(u32, u32)>,
    /// Which of the two images holds the ripples as they are now.
    parity: bool,
    clock: f32,
    /// Seconds since anything last stirred the water.
    quiet: f32,
}

impl TrailMemory {
    /// Moves where each body was by `by`, as the world's origin moves
    /// everything (see `voxear_scene::WorldOrigin`), and the square with
    /// them when that is a whole number of squares' widths: each texel keeps
    /// its place. Moved by any other amount, the trails start again.
    pub fn shift(&mut self, by: Vec3) {
        for (at, _) in self.last.values_mut() {
            *at += by;
        }
        let Some((texels, texel)) = self.made else { return };
        let texel = f32::from_bits(texel);
        let moved = [by.x, by.z].map(|metres| metres / texel);
        let whole = moved.iter().all(|t| t.fract() == 0.0 && (*t as i64).rem_euclid(i64::from(texels)) == 0);
        match self.origin.as_mut() {
            Some(origin) if whole => *origin = [origin[0] + moved[0] as i32, origin[1] + moved[1] as i32],
            _ => self.origin = None,
        }
    }

    /// Which of the two images holds the ripples at the start of this
    /// frame: the other one when it is true.
    pub fn parity(&self) -> bool {
        self.parity
    }

    /// This frame's trails for a camera at `camera`, from what stirs the
    /// water and where each is now, `delta` seconds after the last frame:
    /// `None` when there are none to work out, as with Quality Off or long
    /// after anything last stirred the water. The texture is left for the
    /// renderer to fill in.
    pub fn frame(
        &mut self,
        camera: Vec3,
        stirs: &[(Stir, Vec3)],
        flow: Option<&TrailFlow>,
        settings: WaterTrails,
        delta: f32,
    ) -> Option<GpuTrails> {
        let texels = settings.quality.texels();
        if texels == 0 {
            (self.origin, self.made) = (None, None);
            self.last.clear();
            return None;
        }
        let texel = settings.texel();
        let made = (texels, texel.to_bits());
        if self.made != Some(made) {
            (self.origin, self.made) = (None, Some(made));
        }
        let delta = delta.clamp(0.0, 0.25);
        self.quiet = if stirs.is_empty() { self.quiet + delta } else { 0.0 };
        if self.quiet > QUIET_AFTER {
            self.origin = None;
            self.last.clear();
            return None;
        }
        let half = (texels / 2) as i32;
        let origin = [(camera.x / texel).floor() as i32 - half, (camera.z / texel).floor() as i32 - half];
        let mut gpu = GpuTrails {
            origin,
            last_origin: self.origin.unwrap_or(origin),
            texel,
            texels,
            fresh: u32::from(self.origin.is_none()),
            look: settings.look.index(),
            ..GpuTrails::default()
        };
        self.origin = Some(origin);
        // Steps short enough for the waves to stay steady, as few as a frame needs, and no more than a few: a
        // long frame steps them more slowly than the clock.
        if delta > 0.0 {
            let longest = 0.5 * texel / WAVE_SPEED;
            gpu.steps = ((delta / longest).ceil() as u32).clamp(1, MAX_STEPS);
            gpu.step = (delta / gpu.steps as f32).min(longest);
        }
        let stepped = gpu.steps as f32 * gpu.step;
        self.clock = (self.clock + stepped) % 3600.0;
        gpu.clock = self.clock;
        if let Some(flow) = flow.filter(|flow| flow.cells.len() == FLOW_CELLS * FLOW_CELLS) {
            for (row, pair) in gpu.flow.iter_mut().zip(flow.cells.chunks_exact(2)) {
                *row = [pair[0][0], pair[0][1], pair[1][0], pair[1][1]];
            }
            (gpu.flow_corner, gpu.flow_cell) = (flow.corner, flow.cell);
            gpu.flowing = u32::from(flow.cells.iter().any(|cell| cell[0] != 0.0 || cell[1] != 0.0));
        }
        // The nearest first, only those over the square.
        let mut near: Vec<&(Stir, Vec3)> = stirs.iter().collect();
        near.sort_by(|a, b| a.1.distance_squared(camera).total_cmp(&b.1.distance_squared(camera)));
        let span = texels as f32 * texel;
        let corner = [origin[0] as f32 * texel, origin[1] as f32 * texel];
        let over = |at: Vec3, reach: f32| {
            (corner[0] - reach..corner[0] + span + reach).contains(&at.x)
                && (corner[1] - reach..corner[1] + span + reach).contains(&at.z)
        };
        let mut seen = HashMap::with_capacity(near.len());
        for (stir, at) in near {
            let cut = cutting(at.y + stir.reach[0], at.y + stir.reach[1], stir.surface);
            let was = self.last.get(&stir.entity).copied().filter(|(was, _)| was.distance(*at) <= LONGEST_MOVE);
            seen.insert(stir.entity, (*at, cut));
            let reach = stir.radius + stir.ends[0].distance(stir.ends[1]);
            if gpu.count as usize >= MAX_STIRS || !over(*at, reach) || stir.radius <= 0.0 {
                continue;
            }
            let Some((from, cut_before)) = was else { continue };
            if cut <= 0.0 && cut_before <= 0.0 {
                continue;
            }
            let moved = if delta > 0.0 { (*at - from) / delta } else { Vec3::ZERO };
            let across = (moved - stir.flow).with_y(0.0).length();
            // From where the water that was under it has since flowed to: only what it moves against the water stirs it.
            let carried = from + stir.flow.with_y(0.0) * delta;
            let ends = |place: Vec3| stir.ends.map(|end| place + end);
            let ([a0, b0], [a1, b1]) = (ends(carried), ends(*at));
            gpu.stirs[gpu.count as usize] = GpuStir {
                from: [a0.x, a0.z, b0.x, b0.z],
                to: [a1.x, a1.z, b1.x, b1.z],
                size: [
                    stir.radius.max(texel * 1.5),
                    stir.strength.max(0.0),
                    stir.foam.max(0.0),
                    stir.bob.clamp(0.0, 1.0),
                ],
                motion: [cut_before, cut, moved.y, across],
            };
            gpu.count += 1;
        }
        self.last = seen;
        Some(gpu)
    }

    /// Notes that this frame's passes ran: the ripples are now in the other
    /// image when they took an odd number of passes.
    pub fn ran(&mut self, gpu: &GpuTrails) {
        self.parity ^= gpu.dispatches() % 2 == 1;
    }
}

/// The two images the ripples are kept in from frame to frame, made for
/// `texels` a side.
pub(crate) fn images(
    fg: &mut FrameGraph<'_>,
    (gpu, bindless): (&Gpu, &mut Bindless),
    texels: u32,
) -> Result<[ImageId; 2]> {
    let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
    let desc = ImageDesc::new(texels, texels, vk::Format::R16G16B16A16_SFLOAT, usage);
    Ok([
        fg.history_image(gpu, bindless, "water trails a", desc)?,
        fg.history_image(gpu, bindless, "water trails b", desc)?,
    ])
}

/// Records the passes that move the ripples on, from the image `parity`
/// picks to whichever holds them after (see [`GpuTrails::dispatches`]):
/// clearing and carrying them, then each step, a dispatch each, one image
/// to the other.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    (images, parity): ([ImageId; 2], bool),
    trails: &GpuTrails,
) {
    let storage: [StorageImageId; 2] = [fg.storage(images[0], 0), fg.storage(images[1], 0)];
    let (carries, steps, texels) = (trails.carries(), trails.steps, trails.texels);
    let access = [(images[0], ImageAccess::StorageWrite), (images[1], ImageAccess::StorageWrite)];
    fg.add_pass("Water trails", &access, &[], move |ctx| {
        let mut from = usize::from(parity);
        let groups = texels.div_ceil(8);
        let passes = (0..u32::from(carries))
            .map(|_| (pipelines.water_trails_carry, 0))
            .chain((0..steps).map(|step| (pipelines.water_trails, step)));
        for (index, (pipeline, step)) in passes.enumerate() {
            if index > 0 {
                crate::water::compute_barrier(ctx);
            }
            let push = Push { level: step, output: storage[1 - from].0, textures: [storage[from].0, 0, 0, 0], ..push };
            bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, groups, 1) };
            from = 1 - from;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swimmer(entity: u64, surface: f32) -> Stir {
        Stir {
            entity: Entity::from_bits(entity),
            ends: [Vec3::ZERO; 2],
            radius: 0.35,
            surface,
            reach: [0.0, 1.8],
            flow: Vec3::ZERO,
            strength: 1.0,
            foam: 1.0,
            bob: 1.0,
        }
    }

    #[test]
    fn a_body_cuts_the_surface_only_where_it_reaches_across_it() {
        assert_eq!(cutting(-1.0, 1.0, 0.0), 1.0);
        assert_eq!(cutting(0.5, 2.0, 0.0), 0.0, "all over the water");
        assert_eq!(cutting(-3.0, -1.0, 0.0), 0.0, "all under it");
        let skimming = cutting(0.04, 2.0, 0.0);
        assert!(skimming > 0.0 && skimming < 1.0, "a little over it, a little: {skimming}");
    }

    #[test]
    fn the_square_centres_on_the_camera_and_settles_its_steps_to_the_frame() {
        let mut memory = TrailMemory::default();
        let settings = WaterTrails::default();
        let first = memory.frame(Vec3::ZERO, &[], None, settings, 1.0 / 60.0).expect("trails");
        let texel = settings.texel();
        assert_eq!((first.texels, first.fresh, first.origin), (512, 1, [-256, -256]));
        assert_eq!(first.steps, 1, "a short frame, one step");
        assert!((first.step - 1.0 / 60.0).abs() < 1e-6);
        let next = memory.frame(Vec3::new(texel * 3.5, 0.0, 0.0), &[], None, settings, 0.2).expect("trails");
        assert_eq!((next.fresh, next.origin, next.last_origin), (0, [-253, -256], [-256, -256]));
        assert!(next.steps > 1 && next.steps <= MAX_STEPS, "a long frame, steps short enough: {}", next.steps);
        assert!(next.step * WAVE_SPEED <= 0.5 * texel + 1e-6);
        let paused = memory.frame(Vec3::ZERO, &[], None, settings, 0.0).expect("trails");
        assert_eq!(paused.steps, 0, "paused, the waves stand still");
        let off = WaterTrails { quality: TrailQuality::Off, ..settings };
        assert!(memory.frame(Vec3::ZERO, &[], None, off, 0.1).is_none());
    }

    #[test]
    fn a_body_stirs_from_where_it_was_against_the_water_and_only_while_it_cuts_the_surface() {
        let mut memory = TrailMemory::default();
        let settings = WaterTrails::default();
        let body = swimmer(7, 0.0);
        let at = |x: f32, y: f32| Vec3::new(x, y, 1.0);
        let first = memory.frame(Vec3::ZERO, &[(body, at(0.0, -1.2))], None, settings, 0.1).expect("trails");
        assert_eq!(first.count, 0, "not yet seen to move");
        let swum = memory.frame(Vec3::ZERO, &[(body, at(0.3, -1.2))], None, settings, 0.1).expect("trails");
        assert_eq!(swum.count, 1);
        let stir = swum.stirs[0];
        assert_eq!((stir.from[0], stir.to[0]), (0.0, 0.3));
        assert_eq!(stir.motion[..2], [1.0, 1.0]);
        assert!((stir.motion[3] - 3.0).abs() < 1e-4, "3 m/s across the water: {}", stir.motion[3]);
        // Carried along at the water's own speed, it moves nothing against it.
        let carried = Stir { flow: Vec3::new(3.0, 0.0, 0.0), ..body };
        let drifted = memory.frame(Vec3::ZERO, &[(carried, at(0.6, -1.2))], None, settings, 0.1).expect("trails");
        assert!(drifted.stirs[0].motion[3] < 1e-3);
        assert_eq!(drifted.stirs[0].from[0], drifted.stirs[0].to[0], "from where the water carried it");
        // Diving under, it stirs as it goes, then nothing.
        let dived = memory.frame(Vec3::ZERO, &[(body, at(0.6, -2.5))], None, settings, 0.1).expect("trails");
        assert_eq!(dived.count, 1);
        assert!(dived.stirs[0].motion[2] < -10.0, "going down fast");
        let under = memory.frame(Vec3::ZERO, &[(body, at(0.6, -2.6))], None, settings, 0.1).expect("trails");
        assert_eq!(under.count, 0);
        // Put somewhere else, it starts again there.
        let jumped = memory.frame(Vec3::ZERO, &[(body, at(9.0, -1.2))], None, settings, 0.1).expect("trails");
        assert_eq!(jumped.count, 0);
    }

    #[test]
    fn the_nearest_bodies_are_kept_and_the_square_is_let_go_once_the_water_is_quiet() {
        let mut memory = TrailMemory::default();
        let settings = WaterTrails::default();
        let many: Vec<(Stir, Vec3)> =
            (0..MAX_STIRS as u64 + 5).map(|i| (swimmer(i, 0.0), Vec3::new(i as f32, -1.0, 0.0))).collect();
        memory.frame(Vec3::ZERO, &many, None, settings, 0.1);
        let moved: Vec<(Stir, Vec3)> = many.iter().map(|(stir, at)| (*stir, *at + Vec3::Z * 0.1)).collect();
        let gpu = memory.frame(Vec3::ZERO, &moved, None, settings, 0.1).expect("trails");
        assert_eq!(gpu.count as usize, MAX_STIRS);
        assert_eq!(gpu.stirs[0].to[0], 0.0, "the nearest first");
        for _ in 0..(QUIET_AFTER / 0.25) as usize {
            assert!(memory.frame(Vec3::ZERO, &[], None, settings, 0.25).is_some(), "the ripples die away first");
        }
        assert!(memory.frame(Vec3::ZERO, &[], None, settings, 0.25).is_none());
        assert_eq!(memory.frame(Vec3::ZERO, &moved, None, settings, 0.1).expect("again").fresh, 1);
    }

    #[test]
    fn the_flow_is_laid_out_two_cells_a_row_and_the_passes_flip_the_images() {
        let mut memory = TrailMemory::default();
        let mut cells = vec![[0.0; 2]; FLOW_CELLS * FLOW_CELLS];
        cells[3] = [1.5, -0.5];
        let flow = TrailFlow { corner: [-32.0, -32.0], cell: 4.0, cells };
        let gpu = memory.frame(Vec3::ZERO, &[], Some(&flow), WaterTrails::default(), 1.0 / 60.0).expect("trails");
        assert_eq!(gpu.flow[1], [0.0, 0.0, 1.5, -0.5]);
        assert_eq!(gpu.flowing, 1);
        assert_eq!(gpu.dispatches(), 2, "fresh, so cleared, then a step");
        memory.ran(&gpu);
        assert!(!memory.parity());
        let next = memory.frame(Vec3::ZERO, &[], Some(&flow), WaterTrails::default(), 1.0 / 60.0).expect("trails");
        memory.ran(&next);
        assert_eq!(next.dispatches(), 2, "carried along the river, then a step");
        let still = TrailFlow { cells: vec![[0.0; 2]; FLOW_CELLS * FLOW_CELLS], ..flow };
        let calm = memory.frame(Vec3::ZERO, &[], Some(&still), WaterTrails::default(), 1.0 / 60.0).expect("trails");
        assert_eq!(calm.dispatches(), 1, "nothing to carry, a step");
        memory.ran(&calm);
        assert!(memory.parity());
    }

    #[test]
    fn the_origin_moving_by_whole_squares_keeps_the_trails() {
        let mut memory = TrailMemory::default();
        let settings = WaterTrails::default();
        let first = memory.frame(Vec3::ZERO, &[], None, settings, 0.1).expect("trails");
        let span = settings.area;
        memory.shift(Vec3::new(-span * 4.0, 0.0, 0.0));
        let kept = memory.frame(Vec3::new(-span * 4.0, 0.0, 0.0), &[], None, settings, 0.1).expect("trails");
        assert_eq!(kept.fresh, 0);
        assert_eq!(kept.last_origin, [first.origin[0] - 4 * 512, first.origin[1]]);
        memory.shift(Vec3::new(1.3, 0.0, 0.0));
        assert_eq!(memory.frame(Vec3::ZERO, &[], None, settings, 0.1).expect("trails").fresh, 1);
    }
}
