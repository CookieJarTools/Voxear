//! Structs shared with shaders. Each mirrors a struct in
//! `shaders/common.slang` under scalar layout; change both together.

use voxear_core::math::{Mat4, Vec3};

pub type Mat4Cols = [[f32; 4]; 4];

pub fn cols(m: Mat4) -> Mat4Cols {
    m.to_cols_array_2d()
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMesh {
    pub vertex_offset: u32,
    pub index_offset: u32,
    pub index_count: u32,
    pub _pad: u32,
    pub center: [f32; 3],
    pub radius: f32,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuFrame {
    pub view_proj: Mat4Cols,
    /// Left, right, bottom, top, near; inside when `dot(n, p) + d >= 0`.
    pub planes: [[f32; 4]; 6],
    pub camera_position: [f32; 3],
    pub time: f32,
    /// The direction sunlight travels.
    pub sun_direction: [f32; 3],
    pub light_count: u32,
    pub sun_color: [f32; 3],
    pub exposure: f32,
    pub sky_zenith: [f32; 3],
    pub _pad0: f32,
    pub sky_horizon: [f32; 3],
    pub _pad1: f32,
    pub ground_color: [f32; 3],
    pub _pad2: f32,
    pub viewport: [u32; 2],
    pub instance_count: u32,
    pub _pad3: u32,
    pub inv_view_proj: Mat4Cols,
}

/// `VkDrawIndexedIndirectCommand`.
pub const DRAW_COMMAND_SIZE: u64 = 20;

/// Device addresses every shader reads, plus a per-pass texture index.
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
    pub texture: u32,
    pub max_draws: u32,
}

pub fn vec3(v: Vec3) -> [f32; 3] {
    v.to_array()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(size_of::<GpuMesh>(), 32);
        assert_eq!(size_of::<GpuMaterial>(), 64);
        assert_eq!(size_of::<GpuInstance>(), 128);
        assert_eq!(size_of::<GpuLight>(), 64);
        // view_proj, planes, six float3+scalar rows, viewport row, inv_view_proj.
        assert_eq!(size_of::<GpuFrame>(), 64 + 96 + 16 * 6 + 16 + 64);
        assert!(size_of::<Push>() as u32 <= voxear_rhi::PUSH_CONSTANT_SIZE);
    }
}
