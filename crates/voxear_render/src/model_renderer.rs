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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelRenderer {
    /// Shared, as a model's parts are the same wherever it is placed.
    pub parts: Arc<[ModelPart]>,
    /// As a Mesh Renderer's: shifts which LOD each part draws.
    pub lod_bias: f32,
}

impl Component for ModelRenderer {}

impl ModelRenderer {
    pub fn new(parts: Vec<ModelPart>) -> Self {
        Self { parts: parts.into(), lod_bias: 0.0 }
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
