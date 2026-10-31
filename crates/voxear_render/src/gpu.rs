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
    /// Last frame's model matrix, for motion vectors.
    pub prev_model: Mat4Cols,
    /// Inverse-transpose of the model's 3×3, as three padded columns.
    pub normal: [[f32; 4]; 3],
    pub mesh: u32,
    pub material: u32,
    /// `INSTANCE_NEW` when another Obj had this place in the list last frame.
    pub flags: u32,
    pub _pad: u32,
}

/// An instance whose place in the list another Obj held last frame, so the
/// LOD history kept for that place is not its own.
pub const INSTANCE_NEW: u32 = 1;

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
pub const DEBUG_VIEW_LIGHT_COMPLEXITY: u32 = 3;
pub const DEBUG_VIEW_NORMALS: u32 = 4;
/// Each surface's own colour, without light or shadow.
pub const DEBUG_VIEW_UNLIT: u32 = 5;
/// The meshes' edges, drawn as lines.
pub const DEBUG_VIEW_WIREFRAME: u32 = 6;

/// Froxel grid for clustered lighting: screen tiles by depth slices.
pub const CLUSTER_GRID: [u32; 3] = [16, 9, 24];
pub const CLUSTER_COUNT: u32 = CLUSTER_GRID[0] * CLUSTER_GRID[1] * CLUSTER_GRID[2];
/// Lights one cluster can hold; each cluster stores a count then the indices.
pub const MAX_CLUSTER_LIGHTS: u32 = 128;
/// Depth where the last cluster slice ends; lights beyond use that slice.
pub const CLUSTER_FAR: f32 = 500.0;

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
    pub view: Mat4Cols,
    pub cluster_grid: [u32; 3],
    pub cluster_max_lights: u32,
    /// `slice = log(depth) * cluster_scale - cluster_bias`.
    pub cluster_scale: f32,
    pub cluster_bias: f32,
    /// Pixels per cluster tile.
    pub tile_size: [f32; 2],
    pub delta_time: f32,
    /// LOD fades progress by this much per second.
    pub lod_fade_speed: f32,
    /// Scales the sky's ambient light.
    pub ambient: f32,
    pub _pad4: f32,
    /// The projection without TAA jitter, for motion vectors.
    pub unjittered_view_proj: Mat4Cols,
    pub prev_view_proj: Mat4Cols,
    /// TAA's sub-pixel offset this frame, in pixels.
    pub jitter: [f32; 2],
    /// History is invalid (first frame, resize, TAA just enabled).
    pub taa_reset: u32,
    pub frame_index: u32,
    pub post: GpuPost,
}

/// Post-processing settings as shaders read them. Mirrors `Post` in
/// `shaders/common.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPost {
    pub bloom_intensity: f32,
    pub sharpness: f32,
    pub taa_history: f32,
    pub chromatic_aberration: f32,
    pub exposure_compensation: f32,
    pub exposure_min: f32,
    pub exposure_max: f32,
    pub auto_exposure: u32,
    pub exposure_speed_up: f32,
    pub exposure_speed_down: f32,
    pub tonemapper: u32,
    pub fxaa: u32,
    pub tonemap_contrast: f32,
    pub tonemap_saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub grading_contrast: f32,
    pub grading_saturation: f32,
    pub vignette: f32,
    pub vignette_smoothness: f32,
    pub lift: [f32; 3],
    pub vignette_roundness: f32,
    pub gamma: [f32; 3],
    pub grain: f32,
    pub gain: [f32; 3],
    pub grain_size: f32,
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub fog_height_falloff: f32,
    pub fog_base_height: f32,
    pub fog_sun_glow: f32,
    pub bloom_scatter: f32,
    pub dof_focus: f32,
    pub dof_aperture: f32,
    pub dof_max_blur: f32,
    pub motion_blur: f32,
}

/// Device addresses of every buffer shaders use, written once per frame and
/// reached through one pointer in the push constants.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuBuffers {
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
    pub clusters: u64,
    pub lod_state: u64,
    /// Auto exposure's adapted EV, carried between frames.
    pub exposure: u64,
    pub ui_quads: u64,
    pub debug_lines: u64,
}

/// A line of debug draw (see `voxear_core::debug_draw`), its colour sRGB.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuDebugLine {
    pub from: [f32; 3],
    /// `DEBUG_ON_TOP`.
    pub flags: u32,
    pub to: [f32; 3],
    pub _pad: f32,
    pub color: [f32; 4],
}

/// Drawn over the scene rather than hidden behind it.
pub const DEBUG_ON_TOP: u32 = 1;

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
    /// Draws made for the outgoing LOD of an instance that is fading.
    pub const FADE_DRAWS: usize = SHADOW_DRAWS + 1;
    pub const CLUSTER_MAX_LIGHTS: usize = FADE_DRAWS + 1;
    pub const CLUSTER_LIGHT_LINKS: usize = CLUSTER_MAX_LIGHTS + 1;
    pub const COUNT: usize = CLUSTER_LIGHT_LINKS + 1;
}

/// The frame and buffer pointers every shader reads, plus per-pass parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Push {
    pub frame: u64,
    pub buffers: u64,
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
    /// Textures a post pass reads.
    pub textures: [u32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(size_of::<GpuMesh>(), 32 + 16 * MAX_LODS);
        assert_eq!(size_of::<GpuMaterial>(), 64);
        assert_eq!(size_of::<GpuInstance>(), 192);
        assert_eq!(size_of::<GpuLight>(), 64);
        // Two matrices, six planes, nine 16-byte rows, the cascades, then the
        // view matrix and three rows of clustering and timing.
        let cascades = CASCADES * (64 + 96 + 8);
        let motion = 64 * 2 + 16;
        let post = 16 * 11;
        assert_eq!(size_of::<GpuFrame>(), 64 * 2 + 96 + 16 * 9 + cascades + 64 + 16 * 3 + motion + post);
        assert_eq!(size_of::<GpuPost>(), post);
        assert_eq!(size_of::<GpuBuffers>(), 15 * 8);
        assert_eq!(size_of::<GpuDebugLine>(), 48);
        assert!(size_of::<Push>() as u32 <= voxear_rhi::PUSH_CONSTANT_SIZE);
    }
}
