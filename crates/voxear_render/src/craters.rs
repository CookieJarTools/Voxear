//! Craters: blasts digging into the ground. A Crater Field covers a square
//! of the level round its Obj; each crater a script blasts there is kept in
//! its `CraterMarks`, and stamped into a texture of how far down the ground
//! has been dug at each point. Displaced materials that take craters
//! (`MATERIAL_CRATERS`) sink by it where they face up (`shaders/dice.slang`).

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    gpu::{NO_TEXTURE, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    pipelines::Pipelines,
};

/// The most craters a field keeps; a new one past it takes the oldest's
/// place.
pub const MAX_CRATERS: usize = 4096;
/// The deepest a crater digs, in metres.
pub const MAX_CRATER_DEPTH: f32 = 3.0;

/// The square of ground blasts dig craters into, `size` metres a side round
/// its Obj, kept as a picture `resolution` texels a side.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CraterField {
    pub size: f32,
    pub resolution: u32,
}

impl Component for CraterField {}

impl Default for CraterField {
    fn default() -> Self {
        Self { size: 128.0, resolution: 1024 }
    }
}

/// One crater: where it was blasted, how wide and how deep.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crater {
    pub centre: Vec3,
    pub radius: f32,
    pub depth: f32,
}

/// The craters blasted into a field, oldest first, kept by the engine; and
/// how many times the list has lost craters, so what was stamped is redone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraterMarks {
    pub craters: Vec<Crater>,
    pub generation: u32,
}

impl Component for CraterMarks {}

impl CraterMarks {
    /// A crater, taking the oldest's place when full.
    pub fn blast(&mut self, crater: Crater) {
        if self.craters.len() >= MAX_CRATERS {
            self.craters.remove(0);
            self.generation += 1;
        }
        let crater =
            Crater { radius: crater.radius.max(0.05), depth: crater.depth.clamp(0.0, MAX_CRATER_DEPTH), ..crater };
        self.craters.push(crater);
    }

    pub fn clear(&mut self) {
        self.craters.clear();
        self.generation += 1;
    }
}

/// How far down a crater of `radius` and `depth` digs `off` metres from its
/// middle: a bowl, with a lip thrown up round it. As `craterDepth` in
/// `shaders/dice.slang`.
pub fn crater_depth(off: f32, radius: f32, depth: f32) -> f32 {
    let r = off / radius.max(1e-3);
    let bowl = if r < 1.0 { -depth * (1.0 - r * r).powi(2) } else { 0.0 };
    let lip = 0.2 * depth * (-((r - 1.0) / 0.3).powi(2)).exp();
    bowl + lip
}

/// This frame's field: where its middle is, in X and Z, its size and
/// resolution, its generation, and its craters as the shaders read them
/// (where in X and Z, how wide and how deep).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Field {
    pub middle: [f32; 2],
    pub size: f32,
    pub resolution: u32,
    pub generation: u32,
    pub craters: Vec<[f32; 4]>,
}

impl Field {
    /// What a view stamped the field for: when it changes, the view stamps
    /// it all again.
    pub fn key(&self) -> [u32; 5] {
        [self.middle[0].to_bits(), self.middle[1].to_bits(), self.size.to_bits(), self.resolution, self.generation]
    }
}

/// The level's first Crater Field, if it has one.
pub(crate) fn gather(world: &World) -> Option<Field> {
    let query = world.query_ref::<(&CraterField, Option<&CraterMarks>, &Transform, Option<&WorldTransform>)>();
    let (field, marks, transform, at) = query.without::<Destroying>().into_iter().next()?;
    let middle = at.map_or(transform.position, |w| w.0.w_axis.truncate());
    let craters = marks.map_or(&[][..], |marks| &marks.craters[..]);
    Some(Field {
        middle: [middle.x, middle.z],
        size: field.size.max(1.0),
        resolution: field.resolution.clamp(64, 4096),
        generation: marks.map_or(0, |marks| marks.generation),
        craters: craters.iter().map(|c| [c.centre.x, c.centre.z, c.radius, c.depth]).collect(),
    })
}

/// What a view last stamped into its craters picture: the field's key and
/// how many craters.
pub(crate) type Stamped = Option<([u32; 5], u32)>;

/// The field's picture for this view, with the craters it has not yet
/// stamped stamped into it: all of them, cleared first, when the picture is
/// new or the field changed. Gives back the picture and what it now holds.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (gpu, bindless): (&voxear_rhi::Gpu, &mut voxear_rhi::Bindless),
    (pipelines, push): (Pipelines, Push),
    (field, craters): (&Field, BufferId),
    stamped: Stamped,
) -> anyhow::Result<(ImageId, Stamped)> {
    use voxear_rhi::vk;
    let usage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED;
    let desc = voxear_rhi::ImageDesc::new(field.resolution, field.resolution, vk::Format::R32_SFLOAT, usage);
    let image = fg.history_image(gpu, bindless, "craters", desc)?;
    let count = field.craters.len() as u32;
    let fresh = fg.is_new(image) || stamped.is_none_or(|(key, done)| key != field.key() || done > count);
    let first = if fresh { 0 } else { stamped.map_or(0, |(_, done)| done) };
    if fresh || first < count {
        let output = fg.storage(image, 0).0;
        let size = [field.resolution, field.resolution];
        let stamp = Push { output, size, textures: [first, count, u32::from(fresh), 0], ..push };
        fg.add_pass(
            "Craters",
            &[(image, ImageAccess::StorageWrite)],
            &[(craters, BufferAccess::ComputeRead)],
            move |ctx| {
                crate::passes::bind(ctx, pipelines.crater_stamp, vk::PipelineBindPoint::COMPUTE, &stamp);
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
            },
        );
    }
    Ok((image, Some((field.key(), count))))
}

/// The frame's crater field as the shaders read it: where its first corner
/// is in X and Z, its size, and its picture.
pub(crate) fn gpu_field(field: Option<&Field>, texture: Option<u32>) -> crate::gpu::GpuCraterField {
    match (field, texture) {
        (Some(field), Some(texture)) => crate::gpu::GpuCraterField {
            corner: [field.middle[0] - field.size * 0.5, field.middle[1] - field.size * 0.5],
            size: field.size,
            texture,
        },
        _ => crate::gpu::GpuCraterField { corner: [0.0; 2], size: 1.0, texture: NO_TEXTURE },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crater_is_a_bowl_with_a_lip_and_nothing_far_off() {
        assert_eq!(crater_depth(0.0, 2.0, 1.0), -1.0 + 0.2 * (-(1.0f32 / 0.3).powi(2)).exp());
        assert!(crater_depth(2.0, 2.0, 1.0) > 0.15, "the lip at its edge");
        assert!(crater_depth(5.0, 2.0, 1.0).abs() < 1e-4, "nothing far off");
    }

    #[test]
    fn a_full_field_lets_its_oldest_crater_go_and_says_so() {
        let mut marks = CraterMarks::default();
        let crater = |x: f32| Crater { centre: Vec3::new(x, 0.0, 0.0), radius: 1.0, depth: 9.0 };
        for i in 0..MAX_CRATERS + 2 {
            marks.blast(crater(i as f32));
        }
        assert_eq!(marks.craters.len(), MAX_CRATERS);
        assert_eq!((marks.craters[0].centre.x, marks.generation), (2.0, 2));
        assert_eq!(marks.craters[0].depth, MAX_CRATER_DEPTH, "no deeper than the deepest");
    }
}
