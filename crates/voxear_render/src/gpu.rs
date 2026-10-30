//! Structs shared with shaders. Each mirrors a struct in
//! `shaders/common.slang` under scalar layout; change both together.

use voxear_core::math::{Mat4, Vec3};

use crate::{lod::MAX_LODS, shadows::CASCADES};

pub type Mat4Cols = [[f32; 4]; 4];

pub fn cols(m: Mat4) -> Mat4Cols {
    m.to_cols_array_2d()
}

pub fn vec3(v: Vec3) -> [f32; 3] {
    v.to_array()
}

/// Marks a texture slot that is not in use.
pub const NO_TEXTURE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMeshLod {
    pub index_offset: u32,
    pub index_count: u32,
    /// Mesh-space distance from the original surface.
    pub error: f32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMesh {
    pub vertex_offset: u32,
    pub lod_count: u32,
    pub _pad: [u32; 2],
    pub center: [f32; 3],
    pub radius: f32,
    pub lods: [GpuMeshLod; MAX_LODS],
}

pub const MATERIAL_NORMAL_MAP: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMaterial {
    pub base_color: [f32; 4],
    pub emissive: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub base_color_texture: u32,
    pub metallic_roughness_texture: u32,
    pub normal_texture: u32,
    pub emissive_texture: u32,
    pub flags: u32,
    pub normal_scale: f32,
    /// Zero for opaque; otherwise pixels below this alpha are discarded.
    pub alpha_cutoff: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInstance {
    pub model: Mat4Cols,
    /// Inverse-transpose of the model's 3×3, as three padded columns.
    pub normal: [[f32; 4]; 3],
    pub mesh: u32,
    pub material: u32,
    pub _pad: [u32; 2],
}

pub const LIGHT_POINT: u32 = 0;
pub const LIGHT_SPOT: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position: [f32; 3],
    pub range: f32,
    /// Colour times intensity.
    pub color: [f32; 3],
    pub kind: u32,
    pub direction: [f32; 3],
    pub cos_outer: f32,
    pub cos_inner: f32,
    pub _pad: [f32; 3],
}

pub const DEBUG_VIEW_NONE: u32 = 0;
pub const DEBUG_VIEW_LOD: u32 = 1;
pub const DEBUG_VIEW_CASCADES: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuFrame {
    pub view_proj: Mat4Cols,
    pub inv_view_proj: Mat4Cols,
    /// Left, right, bottom, top, near; inside when `dot(n, p) + d >= 0`.
    pub planes: [[f32; 4]; 6],
    pub camera_position: [f32; 3],
    pub time: f32,
    pub camera_forward: [f32; 3],
    pub near: f32,
    /// The direction sunlight travels.
    pub sun_direction: [f32; 3],
    pub light_count: u32,
    pub sun_color: [f32; 3],
    pub exposure: f32,
    pub sky_zenith: [f32; 3],
    /// Screen pixels per world unit at distance one, for LOD selection.
    pub lod_scale: f32,
    pub sky_horizon: [f32; 3],
    /// Largest LOD error allowed on screen, in pixels.
    pub lod_threshold: f32,
    pub ground_color: [f32; 3],
    pub debug_view: u32,
    pub viewport: [u32; 2],
    pub instance_count: u32,
    pub hiz_texture: u32,
    pub hiz_size: [u32; 2],
    pub hiz_mips: u32,
    pub shadow_texture: u32,
    pub cascade_view_proj: [Mat4Cols; CASCADES],
    pub cascade_planes: [[[f32; 4]; 6]; CASCADES],
    pub cascade_far: [f32; CASCADES],
    pub cascade_texel: [f32; CASCADES],
}

/// `VkDrawIndexedIndirectCommand`.
pub const DRAW_COMMAND_SIZE: u64 = 20;

/// Draw lists the culling pass fills, each `MAX_INSTANCES` long.
pub const LIST_EARLY: u32 = 0;
pub const LIST_LATE: u32 = 1;
pub const LIST_SHADOW: u32 = 2;
pub const LIST_COUNT: u32 = LIST_SHADOW + CASCADES as u32;

/// Counters the culling pass writes, read back a few frames later.
pub mod stat {
    pub const TESTED: usize = 0;
    pub const FRUSTUM_CULLED: usize = 1;
    pub const OCCLUSION_CULLED: usize = 2;
    pub const DRAWN_EARLY: usize = 3;
    pub const DRAWN_LATE: usize = 4;
    /// One counter per LOD, for the main view.
    pub const LOD: usize = 5;
    pub const SHADOW_DRAWS: usize = LOD + crate::lod::MAX_LODS;
    pub const COUNT: usize = SHADOW_DRAWS + 1;
}

/// Device addresses every shader reads, plus per-pass parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Push {
    pub frame: u64,
    pub instances: u64,
    pub meshes: u64,
    pub vertices: u64,
    pub materials: u64,
    pub lights: u64,
    pub draws: u64,
    pub draw_count: u64,
    pub draw_instances: u64,
    pub visibility: u64,
    pub stats: u64,
    pub texture: u32,
    pub max_draws: u32,
    /// Which draw list to fill or draw.
    pub list: u32,
    /// A storage image to write.
    pub output: u32,
    pub size: [u32; 2],
    /// A mip level to read.
    pub level: u32,
    pub _pad: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(size_of::<GpuMesh>(), 32 + 16 * MAX_LODS);
        assert_eq!(size_of::<GpuMaterial>(), 64);
        assert_eq!(size_of::<GpuInstance>(), 128);
        assert_eq!(size_of::<GpuLight>(), 64);
        // Two matrices, six planes, nine 16-byte rows, then the cascades.
        let cascades = CASCADES * (64 + 96 + 8);
        assert_eq!(size_of::<GpuFrame>(), 64 * 2 + 96 + 16 * 9 + cascades);
        assert!(size_of::<Push>() as u32 <= voxear_rhi::PUSH_CONSTANT_SIZE);
    }
}
