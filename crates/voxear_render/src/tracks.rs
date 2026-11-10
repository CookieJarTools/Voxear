//! Tracks in the ground: feet, wheels and anything with a **Track Maker**
//! press into the terrain as they pass over it, deepest in surfaces that
//! give (snow, mud), and the tracks stay behind them until the surface
//! fills them in again.
//!
//! Tracks are only drawn: nothing collides with them and they are not the
//! same on every machine, so they live on the GPU. Each view keeps a map of
//! the ground round its camera (`TRACK_TEXELS` a side, `TRACK_TEXEL` metres
//! each), each texel how deep the ground there was pressed and when. Its
//! texels hold the world's cells modulo its size, so as the camera moves
//! the map scrolls without moving anything: a pass each frame clears the
//! texels that came into view, then presses each Track Maker's path since
//! last frame into it (`shaders/tracks.slang`). The terrain's shaders read
//! it to lower the ground and bend its light (`shaders/terrain.slang`).

use std::collections::HashMap;

use ash::vk;
use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, Entity};

use crate::{
    gpu::Push,
    graph::{BufferAccess, BufferId, FrameGraph},
    passes::bind,
    pipelines::Pipelines,
};

/// How many texels the track map has a side.
pub const TRACK_TEXELS: u32 = 1024;
/// How wide each texel is, in metres: the map spans 64 m round the camera.
pub const TRACK_TEXEL: f32 = 1.0 / 16.0;
/// The most Track Makers that press into the ground in a frame, the nearest
/// to the camera first.
pub const MAX_TRACK_MAKERS: usize = 64;
/// How far a Track Maker can move in a frame and still leave a track all
/// the way; one moved further (as it is put somewhere new) starts afresh.
const LONGEST_STEP: f32 = 2.0;

/// Something that leaves tracks in the ground where it touches it: a foot,
/// a wheel, a sled's runner. Put it where the thing meets the ground, as on
/// the sole of a foot or under a wheel's hub at the tyre.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrackMaker {
    /// How wide its track is, in metres.
    pub width: f32,
    /// How deep it presses into ground that gives all the way, as fresh
    /// snow does, in metres; less into firmer ground.
    pub depth: f32,
}

impl Component for TrackMaker {}

impl Default for TrackMaker {
    fn default() -> Self {
        Self { width: 0.25, depth: 0.12 }
    }
}

/// The track map's corner, in texels from the world's origin along x and
/// z, for a camera at `camera`: the camera in its middle.
pub fn window(camera: Vec3) -> [i32; 2] {
    let half = (TRACK_TEXELS / 2) as i32;
    [(camera.x / TRACK_TEXEL).floor() as i32 - half, (camera.z / TRACK_TEXEL).floor() as i32 - half]
}

/// What presses into the ground this frame, as `shaders/tracks.slang` reads
/// it (`Tracks` in `shaders/common.slang`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTracks {
    /// Each Track Maker's path since last frame: from x and z, to x and z.
    pub paths: [[f32; 4]; MAX_TRACK_MAKERS],
    /// Each one's half width and depth, then how high it was above the
    /// ground at each end: it presses where it touches.
    pub sizes: [[f32; 4]; MAX_TRACK_MAKERS],
    /// The map's corner now and last frame, in texels.
    pub origin: [i32; 2],
    pub last_origin: [i32; 2],
    pub count: u32,
    pub texel: f32,
    /// Whether the map holds nothing to keep yet, as for a view's first
    /// frame, when every texel is cleared.
    pub fresh: u32,
    pub _pad: u32,
}

impl GpuTracks {
    /// Whether the map changes this frame: something presses into it, it
    /// scrolls, or it has nothing yet.
    pub fn changes(&self) -> bool {
        self.count > 0 || self.origin != self.last_origin || self.fresh != 0
    }
}

impl Default for GpuTracks {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

/// A Track Maker this frame: its Obj, where it is, how high above the
/// ground under it, and its settings.
pub type Maker = (Entity, Vec3, f32, TrackMaker);

/// What each view remembers of its tracks between frames: where each Track
/// Maker was, and where the map was.
#[derive(Debug, Default)]
pub struct TrackMemory {
    last: HashMap<Entity, (Vec3, f32)>,
    origin: Option<[i32; 2]>,
}

impl TrackMemory {
    /// Moves where each Track Maker was by `by`, as the world's origin moves
    /// everything (see `voxear_scene::WorldOrigin`), and the map with them
    /// when that is a whole number of maps' widths, as the default step is:
    /// each texel keeps its place in the map. Moved by any other amount, the
    /// map starts again.
    pub fn shift(&mut self, by: Vec3) {
        for (at, _) in self.last.values_mut() {
            *at += by;
        }
        let texels = [by.x, by.z].map(|metres| metres / TRACK_TEXEL);
        let whole = texels.iter().all(|t| t.fract() == 0.0 && (*t as i64).rem_euclid(TRACK_TEXELS as i64) == 0);
        if let Some(origin) = self.origin.as_mut().filter(|_| whole) {
            *origin = [origin[0] + texels[0] as i32, origin[1] + texels[1] as i32];
        }
    }

    /// This frame's presses for a camera at `camera`, from `makers`, the
    /// nearest first, each from where it was last frame to where it is now:
    /// only those over the map.
    pub fn frame(&mut self, camera: Vec3, mut makers: Vec<Maker>) -> GpuTracks {
        let origin = window(camera);
        let mut gpu = GpuTracks {
            origin,
            last_origin: self.origin.unwrap_or(origin),
            texel: TRACK_TEXEL,
            fresh: u32::from(self.origin.is_none()),
            ..GpuTracks::default()
        };
        self.origin = Some(origin);
        makers.sort_by(|a, b| a.1.distance_squared(camera).total_cmp(&b.1.distance_squared(camera)));
        let span = TRACK_TEXELS as f32 * TRACK_TEXEL;
        let corner = [origin[0] as f32 * TRACK_TEXEL, origin[1] as f32 * TRACK_TEXEL];
        let over = |at: Vec3, reach: f32| {
            (corner[0] - reach..corner[0] + span + reach).contains(&at.x)
                && (corner[1] - reach..corner[1] + span + reach).contains(&at.z)
        };
        let mut seen = HashMap::with_capacity(makers.len());
        for (entity, at, above, maker) in makers {
            let was = self.last.get(&entity).copied().filter(|(was, _)| was.distance(at) <= LONGEST_STEP);
            let (from, above_from) = was.unwrap_or((at, above));
            seen.insert(entity, (at, above));
            let slot = gpu.count as usize;
            if slot >= MAX_TRACK_MAKERS || !over(at, maker.width) || maker.depth <= 0.0 || maker.width <= 0.0 {
                continue;
            }
            gpu.paths[slot] = [from.x, from.z, at.x, at.z];
            gpu.sizes[slot] = [maker.width * 0.5, maker.depth, above_from, above];
            gpu.count += 1;
        }
        self.last = seen;
        gpu
    }
}

/// Records the pass that clears what came into the map and presses this
/// frame's tracks into it.
pub(crate) fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), texels: BufferId) {
    fg.add_pass("Tracks", &[], &[(texels, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.tracks, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, (TRACK_TEXELS * TRACK_TEXELS).div_ceil(64), 1, 1) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_is_centred_on_the_camera_and_scrolls_a_texel_at_a_time() {
        assert_eq!(window(Vec3::ZERO), [-512, -512]);
        assert_eq!(window(Vec3::new(TRACK_TEXEL * 3.5, 0.0, -TRACK_TEXEL * 0.5)), [-509, -513]);
        let mut memory = TrackMemory::default();
        let first = memory.frame(Vec3::ZERO, Vec::new());
        assert_eq!((first.fresh, first.origin, first.last_origin), (1, [-512, -512], [-512, -512]));
        let next = memory.frame(Vec3::new(1.0, 0.0, 0.0), Vec::new());
        assert_eq!((next.fresh, next.origin, next.last_origin), (0, [-496, -512], [-512, -512]));
    }

    #[test]
    fn makers_press_from_where_they_were_unless_they_jumped() {
        let mut memory = TrackMemory::default();
        let foot = Entity::from_bits(7);
        let maker = TrackMaker::default();
        memory.frame(Vec3::ZERO, vec![(foot, Vec3::new(1.0, 0.0, 1.0), 0.0, maker)]);
        let walked = memory.frame(Vec3::ZERO, vec![(foot, Vec3::new(1.5, 0.1, 1.0), 0.1, maker)]);
        assert_eq!(walked.count, 1);
        assert_eq!(walked.paths[0], [1.0, 1.0, 1.5, 1.0]);
        assert_eq!(walked.sizes[0], [0.125, 0.12, 0.0, 0.1]);
        let jumped = memory.frame(Vec3::ZERO, vec![(foot, Vec3::new(9.0, 0.0, 1.0), 0.0, maker)]);
        assert_eq!(jumped.paths[0], [9.0, 1.0, 9.0, 1.0], "a dot where it landed");
        // One off the map presses nothing, nor does one with no depth.
        let far = (Entity::from_bits(8), Vec3::new(500.0, 0.0, 0.0), 0.0, maker);
        let flat = (Entity::from_bits(9), Vec3::ZERO, 0.0, TrackMaker { depth: 0.0, ..maker });
        assert_eq!(memory.frame(Vec3::ZERO, vec![far, flat]).count, 0);
    }
}
