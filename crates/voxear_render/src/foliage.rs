//! Pushing through foliage: characters, vehicles and anything else with a
//! Foliage Interaction bend grass and plants out of their way, and the
//! plants spring back after them, as The Vegetation Engine's interaction
//! does in Unity.
//!
//! Pushes are kept in a grid of cells round the camera, `PUSH_CELLS` each
//! way `PUSH_CELL` metres apart, in a buffer kept from frame to frame. Each
//! frame, **Foliage pushes** (`shaders/foliage_push.slang`) lets every
//! cell's push spring back a little and stamps where each pusher is now, so
//! a trail stays behind something running through a field. Plants read the
//! grid where they stand (`shaders/wind.slang`) and lean away. A world cell
//! keeps its place in the grid as the camera moves, as the GI's probes do;
//! pushes left from far away have sprung back long before the camera comes
//! round to their cells again.

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::Component;
use voxear_rhi::vk;

use crate::{
    gpu::{GpuPushes, MAX_PUSHERS, PUSH_CELL, PUSH_CELLS, Push},
    graph::{BufferAccess, BufferId, FrameGraph},
    passes::bind,
    pipelines::Pipelines,
};

/// Something that bends foliage out of its way as it passes through:
/// grass and plants within `radius` of the Obj lean away from it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FoliageInteraction {
    /// How far round the Obj plants are pushed, in metres.
    pub radius: f32,
    /// How hard: 1 bends grass well over, 0.3 just stirs it.
    pub strength: f32,
}

impl Component for FoliageInteraction {}

impl Default for FoliageInteraction {
    fn default() -> Self {
        Self { radius: 0.6, strength: 1.0 }
    }
}

/// The pushers as shaders read them, the nearest to the camera first, at
/// most `MAX_PUSHERS`, with the grid placed round the camera.
pub(crate) fn pushes(camera: Vec3, mut pushers: Vec<(Vec3, FoliageInteraction)>) -> GpuPushes {
    pushers.sort_by(|a, b| a.0.distance_squared(camera).total_cmp(&b.0.distance_squared(camera)));
    let mut gpu = GpuPushes::default();
    for (slot, (at, push)) in pushers.iter().take(MAX_PUSHERS).enumerate() {
        gpu.pushers[slot] = [at.x, at.y, at.z, push.radius.max(0.01)];
        gpu.strengths[slot / 4][slot % 4] = push.strength.max(0.0);
    }
    gpu.count = pushers.len().min(MAX_PUSHERS) as u32;
    let half = (PUSH_CELLS / 2) as i32;
    gpu.origin = [(camera.x / PUSH_CELL).floor() as i32 - half, (camera.z / PUSH_CELL).floor() as i32 - half];
    gpu.cell = PUSH_CELL;
    gpu
}

/// Records the pass that springs the pushes back and stamps this frame's.
pub(crate) fn record(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), cells: BufferId) {
    fg.add_pass("Foliage pushes", &[], &[(cells, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.foliage_push, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, (PUSH_CELLS * PUSH_CELLS).div_ceil(64), 1, 1) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_pushers_are_kept_and_the_grid_centres_on_the_camera() {
        let push = FoliageInteraction::default();
        let far: Vec<_> = (0..MAX_PUSHERS + 4).map(|i| (Vec3::new(100.0 + i as f32, 0.0, 0.0), push)).collect();
        let near = (Vec3::new(1.0, 0.0, 1.0), FoliageInteraction { radius: 2.0, strength: 0.5 });
        let gpu = pushes(Vec3::new(10.2, 3.0, -4.9), far.into_iter().chain([near]).collect());
        assert_eq!(gpu.count as usize, MAX_PUSHERS);
        assert_eq!(gpu.pushers[0], [1.0, 0.0, 1.0, 2.0], "the nearest first");
        assert_eq!(gpu.strengths[0][0], 0.5);
        let half = (PUSH_CELLS / 2) as i32;
        assert_eq!(gpu.origin, [20 - half, -10 - half]);
    }
}
