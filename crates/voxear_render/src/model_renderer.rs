//! A whole model drawn by one Obj: a Model Renderer draws each of a model's
//! parts, its mesh and material where the model places it, as a Mesh
//! Renderer draws one, so a model of a hundred parts is one Obj rather than
//! a hundred. Each part is an instance of its own, culled, given its LOD,
//! shadowed and traced for bounced light and reflections as any mesh is.

use std::sync::Arc;

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Component;

use crate::{MaterialHandle, MeshHandle, Renderer};

/// One part of a model: its mesh, its material, where it sits in the
/// model, which is the Obj's space, and which of the model's nodes it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPart {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub transform: Mat4,
    pub node: u32,
    /// The skin among its model's animation's that moves it, for a skinned
    /// part; it then sits where its bones put it, at the model's own place.
    pub skin: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelRenderer {
    /// Shared, as a model's parts are the same wherever it is placed.
    pub parts: Arc<[ModelPart]>,
    /// As a Mesh Renderer's: shifts which LOD each part draws.
    pub lod_bias: f32,
    /// The model's skeleton, skins and clips, for a model made to animate,
    /// whose skinned parts a `Pose` moves.
    pub animation: Option<Arc<crate::ModelAnimation>>,
    /// Colours that multiply what it draws, in sRGB, white for none: the
    /// first over the whole model, or where a part's material has a tint
    /// mask, each where the mask says. This Obj's own, so two Objs drawing
    /// one model can be tinted apart. A box model's pixels and cubes keep
    /// their sharp edges and holes, near and far.
    pub tints: [[f32; 3]; TINTS],
    /// Whether any part's material has a tint mask, so its second and
    /// third tints show where the mask says.
    pub masked: bool,
}

/// Which of a Model Renderer's tints: without a tint mask the first tints
/// the whole model; with one, each where the mask says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TintSlot {
    #[default]
    First,
    Second,
    Third,
}

impl TintSlot {
    pub const ALL: [Self; TINTS] = [Self::First, Self::Second, Self::Third];

    /// Its place among a Model Renderer's tints.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The colour a tint mask has where this tint falls: red, green or blue.
    pub fn mask_color(slot: Option<Self>) -> [u8; 4] {
        match slot {
            Some(Self::First) => [255, 0, 0, 255],
            Some(Self::Second) => [0, 255, 0, 255],
            Some(Self::Third) => [0, 0, 255, 255],
            None => [0, 0, 0, 255],
        }
    }
}

/// How many tints a Model Renderer has.
pub const TINTS: usize = 3;

/// A tint that changes nothing.
pub const NO_TINT: [f32; 3] = [1.0; 3];

impl Component for ModelRenderer {}

impl ModelRenderer {
    pub fn new(parts: Vec<ModelPart>) -> Self {
        Self { parts: parts.into(), lod_bias: 0.0, animation: None, tints: [NO_TINT; TINTS], masked: false }
    }

    /// Its tints as an instance carries them: each sRGB RGBA8 in its bits,
    /// 0 for one that changes nothing.
    pub fn gpu_tints(&self) -> [u32; TINTS] {
        self.tints.map(pack_tint)
    }

    /// Each part's box in the Obj's space, as the part's mesh's corners
    /// where the part sits: for picking a part, and the model's bounds.
    pub fn part_boxes<'a>(&'a self, renderer: &'a Renderer) -> impl Iterator<Item = (Mat4, Vec3, Vec3)> + 'a {
        self.parts.iter().filter_map(|part| {
            let (min, max) = renderer.mesh_bounds(part.mesh)?;
            Some((part.transform, min, max))
        })
    }

    /// The box round every part, in the Obj's space.
    pub fn bounds(&self, renderer: &Renderer) -> Option<(Vec3, Vec3)> {
        let mut found = None;
        for (transform, min, max) in self.part_boxes(renderer) {
            for corner in 0..8 {
                let pick = |bit: usize, a: f32, b: f32| if corner & bit == 0 { a } else { b };
                let point = transform.transform_point3(Vec3::new(
                    pick(1, min.x, max.x),
                    pick(2, min.y, max.y),
                    pick(4, min.z, max.z),
                ));
                let (low, high) = found.unwrap_or((point, point));
                found = Some((low.min(point), high.max(point)));
            }
        }
        found
    }
}

/// A tint as the GPU reads it: sRGB RGBA8, 0 for white, which changes
/// nothing, so black is still a tint.
pub fn pack_tint(tint: [f32; 3]) -> u32 {
    let [r, g, b] = tint.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u32);
    if [r, g, b] == [255; 3] { 0 } else { r | g << 8 | b << 16 | 255 << 24 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_packs_to_no_tint_and_black_is_still_one() {
        assert_eq!(pack_tint(NO_TINT), 0);
        assert_eq!(pack_tint([0.0; 3]), 255 << 24);
        assert_eq!(pack_tint([1.0, 0.5, 0.0]), 255 | 128 << 8 | 255 << 24);
        let tinted = ModelRenderer { tints: [[0.2, 0.4, 0.6], NO_TINT, [0.0; 3]], ..ModelRenderer::new(Vec::new()) };
        assert_eq!(tinted.gpu_tints(), [51 | 102 << 8 | 153 << 16 | 255 << 24, 0, 255 << 24]);
    }
}
