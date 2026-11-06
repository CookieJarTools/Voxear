//! Smoke: a cloud that fills the space round its Obj as smoke from a
//! grenade does, flowing through doors and round corners but not through
//! walls, drawn by `shaders/smoke.slang`.
//!
//! The engine fills each Smoke's grid of `SMOKE_GRID`³ cells outwards from
//! its Obj against the level's colliders (`SmokeFill`), noting how many
//! steps out each cell is, so the cloud grows out from where it started.
//! The renderer marches through the grids at half the view's size, lit by
//! the sun through the cloud and by the light round it, and lays the result
//! over the scene, fogged as far off as the cloud is.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, World};
use voxear_rhi::vk;
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    gpu::{GpuSmoke, NO_TEXTURE, Push},
    graph::{FrameGraph, ImageAccess, ImageId},
    pipelines::Pipelines,
};

/// How many cells a smoke's grid has along each side.
pub const SMOKE_GRID: usize = 32;
/// The most smokes a frame draws, the nearest the camera.
pub const MAX_SMOKES: usize = 16;
/// How many 32-bit words of cells a smoke takes, four cells to a word.
pub const SMOKE_WORDS: usize = SMOKE_GRID * SMOKE_GRID * SMOKE_GRID / 4;

/// A cloud of smoke filling the space round its Obj, as a smoke grenade's:
/// as much as a ball `size` across would hold, spreading round walls.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Smoke {
    /// How much it fills: as much as a ball this many metres across holds.
    pub size: f32,
    /// How fine its cells are, in metres: finer follows walls more closely
    /// and reaches less far.
    pub cell: f32,
    /// How thick it is: how much of the light a metre of it takes.
    pub density: f32,
    /// Its colour, linear: how much of the light it scatters, by colour.
    pub color: [f32; 3],
    /// How long it takes to spread out, in seconds.
    pub spread: f32,
    /// How long it lasts before it starts to thin, in seconds; 0 for ever.
    pub lasts: f32,
    /// How long it takes to thin away, in seconds.
    pub fade: f32,
    /// Which swirls: two smokes alike but for this look different.
    pub seed: u32,
}

impl Component for Smoke {}

impl Default for Smoke {
    fn default() -> Self {
        Self { size: 8.0, cell: 0.4, density: 3.0, color: [0.8, 0.8, 0.8], spread: 2.0, lasts: 0.0, fade: 3.0, seed: 0 }
    }
}

impl Smoke {
    /// How many cells it fills at most.
    pub fn budget(&self) -> usize {
        let radius = self.size.max(0.0) * 0.5;
        let cell = self.cell.max(0.05);
        let cells = (4.0 / 3.0 * std::f32::consts::PI * radius.powi(3) / cell.powi(3)).ceil() as usize;
        cells.clamp(1, SMOKE_GRID * SMOKE_GRID * SMOKE_GRID)
    }

    /// How thick it is `age` seconds after it started, from 0 to 1, as it
    /// thins at the end of its life.
    pub fn thickness(&self, age: f32) -> f32 {
        if self.lasts <= 0.0 {
            return 1.0;
        }
        1.0 - ((age - self.lasts) / self.fade.max(1e-3)).clamp(0.0, 1.0)
    }
}

/// A Smoke's cells as filled round where it was: for each cell, 0 where the
/// smoke does not reach, or 1 more than how many steps out from the middle
/// it is; and how many steps out the farthest is. Set by the engine, with
/// the Smoke and the place it was filled for and when, in the renderer's
/// time, so it is filled again when either changes.
#[derive(Clone, Debug, PartialEq)]
pub struct SmokeFill {
    pub cells: Arc<[u8]>,
    pub steps: u8,
    /// The grid's first corner, in the world.
    pub origin: Vec3,
    pub filled_for: (Smoke, Vec3),
    pub started: f32,
}

impl Component for SmokeFill {}

impl SmokeFill {
    /// The cells, four to a word, as the shaders read them.
    fn words(&self) -> impl Iterator<Item = u32> + '_ {
        self.cells
            .chunks(4)
            .map(|four| four.iter().enumerate().fold(0u32, |word, (i, &c)| word | u32::from(c) << (i * 8)))
    }
}

/// The grid's first corner for a Smoke at `middle`, `cell` metres a cell,
/// the middle cell's centre on it.
pub fn smoke_origin(middle: Vec3, cell: f32) -> Vec3 {
    middle - Vec3::splat((SMOKE_GRID / 2) as f32 * cell + cell * 0.5)
}

/// This frame's smokes, the nearest `camera` first, and their cells, at
/// `time`.
pub(crate) fn gather(world: &World, camera: Vec3, time: f32) -> (Vec<GpuSmoke>, Vec<u32>) {
    let query = world.query_ref::<(&Smoke, &SmokeFill, &Transform, Option<&WorldTransform>)>();
    let mut smokes: Vec<(f32, &Smoke, &SmokeFill)> = query
        .without::<Destroying>()
        .into_iter()
        .map(|(smoke, fill, transform, at)| {
            let middle = at.map_or(transform.position, |w| w.0.w_axis.truncate());
            (middle.distance_squared(camera), smoke, fill)
        })
        .filter(|(_, smoke, fill)| smoke.thickness(time - fill.started) > 0.0 && fill.steps > 0)
        .collect();
    smokes.sort_by(|a, b| a.0.total_cmp(&b.0));
    smokes.truncate(MAX_SMOKES);
    let mut cells = Vec::with_capacity(smokes.len() * SMOKE_WORDS);
    let gpu = smokes
        .into_iter()
        .map(|(_, smoke, fill)| {
            let first = cells.len() as u32;
            cells.extend(fill.words());
            let age = time - fill.started;
            GpuSmoke {
                origin: fill.origin.to_array(),
                cell: fill.filled_for.0.cell.max(0.05),
                color: smoke.color.map(|c| c.clamp(0.0, 1.0)),
                density: smoke.density.max(0.0) * smoke.thickness(age),
                grown: (age / smoke.spread.max(1e-3)).clamp(0.0, 1.0),
                steps: f32::from(fill.steps),
                seed: (smoke.seed % 1024) as f32 * 17.31,
                age,
                cells: first,
                _pad: [0; 3],
            }
        })
        .collect();
    (gpu, cells)
}

/// The smokes a frame draws, after the fog (see `post_passes`): how many,
/// and what their lighting reads.
#[derive(Clone)]
pub struct Draw {
    pub count: u32,
    /// The scene's depth, and the shadow atlas, the GI's probes and the
    /// atmosphere's tables.
    pub depth: ImageId,
    pub sampled: Vec<ImageId>,
}

/// Records the smokes: marched at half the view's size into an image of
/// their own, then laid over `hdr`, each pixel from the half-size ones
/// nearest it in depth, so the smoke's edge stays sharp along what stands
/// in front of it.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (gpu, bindless): (&voxear_rhi::Gpu, &mut voxear_rhi::Bindless),
    (pipelines, push): (Pipelines, Push),
    (draw, hdr, extent): (Draw, ImageId, vk::Extent2D),
    fog_volume: Option<ImageId>,
) -> anyhow::Result<()> {
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let half = vk::Extent2D { width: extent.width.div_ceil(2), height: extent.height.div_ceil(2) };
    let desc = voxear_rhi::ImageDesc::new(half.width, half.height, vk::Format::R16G16B16A16_SFLOAT, usage);
    let marched = fg.create_image(gpu, bindless, "smoke", desc)?;
    let Draw { count, depth, sampled } = draw;
    let depth_texture = fg.texture(depth).0;
    let volume = fog_volume.map_or(NO_TEXTURE, |volume| fg.texture(volume).0);
    let mut images = vec![(marched, ImageAccess::ColorAttachment), (depth, ImageAccess::Sampled)];
    images.extend(sampled.iter().chain(&fog_volume).map(|&image| (image, ImageAccess::Sampled)));
    let march = Push { textures: [depth_texture, volume, count, 0], size: [half.width, half.height], ..push };
    fg.add_pass("Smoke", &images, &[], move |ctx| {
        crate::post_passes::draw(ctx, pipelines.smoke_march, &march, marched, false);
    });
    let marched_texture = fg.texture(marched).0;
    let images = [(hdr, ImageAccess::ColorAttachment), (marched, ImageAccess::Sampled), (depth, ImageAccess::Sampled)];
    let lay =
        Push { texture: marched_texture, textures: [depth_texture, 0, 0, 0], size: [half.width, half.height], ..push };
    fg.add_pass("Smoke over the scene", &images, &[], move |ctx| {
        crate::post_passes::draw(ctx, pipelines.smoke_composite, &lay, hdr, true);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_smoke_fills_as_much_as_a_ball_its_size_and_thins_at_the_end() {
        let smoke = Smoke { size: 4.0, cell: 0.5, lasts: 10.0, fade: 2.0, ..Smoke::default() };
        // A ball four metres across holds about 33.5 cubic metres: 268 cells of half a metre.
        assert_eq!(smoke.budget(), 269);
        assert_eq!((smoke.thickness(5.0), smoke.thickness(11.0), smoke.thickness(13.0)), (1.0, 0.5, 0.0));
        assert_eq!(Smoke::default().thickness(1e6), 1.0, "one with no end lasts for ever");
    }

    #[test]
    fn cells_go_to_the_shaders_four_to_a_word() {
        let fill = SmokeFill {
            cells: Arc::from(vec![1, 2, 3, 4, 0, 0, 0, 9]),
            steps: 9,
            origin: Vec3::ZERO,
            filled_for: (Smoke::default(), Vec3::ZERO),
            started: 0.0,
        };
        assert_eq!(fill.words().collect::<Vec<_>>(), [0x04030201, 0x09000000]);
    }
}
