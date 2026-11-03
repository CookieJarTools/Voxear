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
    /// Its meshlets' place in the meshlet table. They hold its indices in
    /// turn from `index_offset`.
    pub meshlet_offset: u32,
    pub meshlet_count: u32,
    pub _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMesh {
    pub vertex_offset: u32,
    pub lod_count: u32,
    /// Where its meshlets' vertices and triangles start, which theirs count from.
    pub meshlet_vertex_offset: u32,
    pub meshlet_triangle_offset: u32,
    pub center: [f32; 3],
    pub radius: f32,
    pub lods: [GpuMeshLod; MAX_LODS],
}

/// A cluster of up to 124 of a LOD's triangles (see `meshlets`), with what
/// culling needs to know of it, in mesh space.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMeshlet {
    pub center: [f32; 3],
    pub radius: f32,
    /// Its triangles' normals lie within a cone around this axis, whose
    /// half-angle has the cosine `cone_cutoff`; that is 1 when they spread
    /// too wide for any viewpoint to see only their backs.
    pub cone_axis: [f32; 3],
    pub cone_cutoff: f32,
    /// Its first index, counted from its LOD's first.
    pub first_index: u32,
    /// Where its vertices and triangles start, counted from its mesh's.
    pub vertex_offset: u32,
    pub triangle_offset: u32,
    /// Its vertex count in the low byte, and its triangle count above.
    pub counts: u32,
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
    /// `INSTANCE_NEW` when another Obj had this place in the list last frame,
    /// and above `INSTANCE_LOD_SHIFT`, the LOD it always draws, plus one.
    pub flags: u32,
    /// Scales the LOD error it allows by two to this power: above 0 it
    /// switches to simpler LODs sooner.
    pub lod_bias: f32,
    /// With `INSTANCE_LOD_SCREEN`, the share of the screen's height each LOD
    /// shows down to, the first five of eight; the next LOD shows below it.
    pub lod_screen: [[f32; 4]; 2],
}

/// An instance whose place in the list another Obj held last frame, so the
/// LOD history kept for that place is not its own.
pub const INSTANCE_NEW: u32 = 1;

/// Where an instance's forced LOD, plus one, sits in its flags; 0 there
/// leaves the LOD to the distance.
pub const INSTANCE_LOD_SHIFT: u32 = 8;

/// An instance whose LODs switch where its `lod_screen` says, by how much
/// of the screen it covers, rather than by their error.
pub const INSTANCE_LOD_SCREEN: u32 = 2;

pub const LIGHT_POINT: u32 = 0;
pub const LIGHT_SPOT: u32 = 1;
/// Area lights, shining from one face along their direction.
pub const LIGHT_RECTANGLE: u32 = 2;
pub const LIGHT_DISC: u32 = 3;
/// A capsule along its `right` axis.
pub const LIGHT_TUBE: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    pub position: [f32; 3],
    /// How far it reaches, from its shape's surface.
    pub range: f32,
    /// Colour times intensity.
    pub color: [f32; 3],
    pub kind: u32,
    /// Its Obj's +Z, which spot and area lights shine along.
    pub direction: [f32; 3],
    pub cos_outer: f32,
    /// Its Obj's +X: a rectangle's width and a tube's length lie along it,
    /// and a cookie's or profile's horizontal axis.
    pub right: [f32; 3],
    pub cos_inner: f32,
    /// A rectangle's half width and half height; a disc's radius twice; a
    /// tube's half length and radius.
    pub size: [f32; 2],
    /// A texture projected along a spot light's cone, or `NO_TEXTURE`.
    pub cookie: u32,
    /// An IES profile baked over the sphere (see `ies`), or `NO_TEXTURE`.
    pub profile: u32,
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
/// The scene lit as usual, with the meshes' edges drawn faintly over it.
pub const DEBUG_VIEW_LIT_WIREFRAME: u32 = 7;
/// Each meshlet in a colour of its own, lit by the sun.
pub const DEBUG_VIEW_MESHLETS: u32 = 8;
/// What ambient occlusion leaves of the sky's light, as the opaque passes
/// multiply it in, tint and all.
pub const DEBUG_VIEW_AMBIENT_OCCLUSION: u32 = 9;

/// `GpuFrame::meshlet_flags`: back-facing meshlets are culled; surviving
/// meshlets side by side are drawn together, as one range of indices; draws
/// are culled meshlet by meshlet.
pub const MESHLET_CONES: u32 = 1;
pub const MESHLET_RUNS: u32 = 2;
pub const MESHLET_CULLING: u32 = 4;

/// `Push::source`: which lists a draw pass reads. With mesh shaders,
/// `DRAWS_AGAIN` draws the meshlet lists a second time, as the lit
/// wireframe's edges, without counting them in the stats again.
pub const DRAWS_INSTANCES: u32 = 0;
pub const DRAWS_MESHLETS: u32 = 1;
pub const DRAWS_AGAIN: u32 = 2;

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
    /// How many meshlets every instance's draw can turn into together, at
    /// most, which lays out the meshlet lists (see [`meshlet_list`]).
    pub meshlet_capacity: u32,
    /// `MESHLET_CONES`, `MESHLET_RUNS` and `MESHLET_CULLING`.
    pub meshlet_flags: u32,
    pub _pad5: [u32; 2],
    pub lighting: GpuLighting,
    pub gi: GpuGi,
}

/// How many nested grids the voxel GI's clipmap has, each twice the last's
/// voxel size, and how many voxels each is across.
pub const GI_LEVELS: usize = 3;
pub const GI_SIZE: u32 = 64;

/// Voxel global illumination as the shaders read it (see `gi`). Mirrors
/// `Gi` in `shaders/common.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGi {
    /// Each level's lowest voxel, in that level's voxels from the world's
    /// origin, this frame and last frame; w unused.
    pub origin: [[i32; 4]; GI_LEVELS],
    pub previous_origin: [[i32; 4]; GI_LEVELS],
    /// The sun's shadow cascades back into the world, to turn what they saw
    /// into voxels.
    pub cascade_inverse: [Mat4Cols; crate::shadows::CASCADES],
    /// The first level's voxel size, in metres.
    pub voxel_size: f32,
    /// The voxels as a texture, or `NO_TEXTURE` without GI, and as a
    /// storage image for the passes that fill them.
    pub texture: u32,
    pub storage: u32,
    /// 1 when the voxels are stale, as on their first frame.
    pub reset: u32,
    /// How bright the bounced light is, 1 as measured.
    pub strength: f32,
    /// Cones traced for each pixel's light.
    pub cones: u32,
    pub _pad: [u32; 2],
}

/// `GpuLighting::sky`: the colours the scene gives, or the atmosphere.
pub const SKY_GRADIENT: u32 = 0;
pub const SKY_ATMOSPHERE: u32 = 1;

/// What area lights, cookies and the sky shade with beyond the frame's
/// basics. Mirrors `Lighting` in `shaders/common.slang`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLighting {
    /// The LTC tables (see `ltc`).
    pub ltc_matrix: u32,
    pub ltc_amplitude: u32,
    /// A texture the sun's light passes through, tiled over the world every
    /// `sun_cookie_size` metres, or `NO_TEXTURE`.
    pub sun_cookie: u32,
    pub sun_cookie_size: f32,
    /// The sun's Obj's +X and +Y, which lay its cookie out.
    pub sun_right: [f32; 3],
    /// `SKY_GRADIENT` or `SKY_ATMOSPHERE`.
    pub sky: u32,
    pub sun_up: [f32; 3],
    /// The atmosphere's lookup tables, drawn each frame (see `atmosphere`).
    pub transmittance: u32,
    pub multi_scattering: u32,
    pub sky_view: u32,
    pub aerial_perspective: u32,
    /// The tangent of half the sun's angle across, which softens its
    /// shadows (see `Light::softness`); 0 for hard ones.
    pub sun_softness: f32,
    pub atmosphere: GpuAtmosphere,
}

/// A planet's atmosphere, in kilometres and per kilometre, as Hillaire's
/// "A Scalable and Production Ready Sky and Atmosphere Rendering
/// Technique" (2020) models it. Mirrors `Atmosphere` in the shaders.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAtmosphere {
    pub rayleigh_scattering: [f32; 3],
    pub rayleigh_height: f32,
    pub mie_scattering: [f32; 3],
    pub mie_height: f32,
    pub mie_extinction: [f32; 3],
    /// Henyey-Greenstein's g for the aerosols: how much they scatter forward.
    pub mie_g: f32,
    pub ozone_absorption: [f32; 3],
    pub bottom_radius: f32,
    pub ground_albedo: [f32; 3],
    pub top_radius: f32,
    /// The cosine of the sun disc's angular radius.
    pub sun_cos_radius: f32,
    /// Distances in the scene count as this many times as far for the haze
    /// over them, as aerial perspective is too faint at a game's scale.
    pub aerial_scale: f32,
    pub _pad: [f32; 2],
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
    /// Ambient occlusion's tint (linear) and strength, its radius in metres
    /// and the slices and steps GTAO searches.
    pub ao_tint: [f32; 3],
    pub ao_strength: f32,
    pub ao_radius: f32,
    pub ao_slices: u32,
    pub ao_steps: u32,
    /// The occlusion the opaque passes read, or `NO_TEXTURE` while it is off.
    pub ao_texture: u32,
    /// The reciprocals of the projection's x and y scales, which turn a
    /// pixel's NDC times its view depth back into view space.
    pub ao_unproject: [f32; 2],
    /// Last frame's occlusion is stale (first frame, resize, just turned on).
    pub ao_reset: u32,
    pub _ao_pad: u32,
    /// Volumetric fog: Henyey-Greenstein's g for how much light the fog
    /// scatters forward, and how far its volume reaches, in metres.
    pub fog_anisotropy: f32,
    pub fog_volume_far: f32,
    /// The integrated volume, or `NO_TEXTURE` without volumetric fog.
    pub fog_volume: u32,
    /// 1 when the volume's history is stale, as on its first frame.
    pub fog_volume_reset: u32,
    /// Contact shadows' image, or `NO_TEXTURE` without them, and how they
    /// march: how far, how thick things count as, and in how many steps.
    pub contact_texture: u32,
    pub contact_length: f32,
    pub contact_thickness: f32,
    pub contact_steps: u32,
    /// The colour grading lookup table, or `NO_TEXTURE`, and how much of it
    /// shows; the shader reads its size from the table.
    pub lut_texture: u32,
    pub _lut_pad0: u32,
    pub lut_contribution: f32,
    pub _lut_pad: u32,
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
    pub meshlets: u64,
    pub meshlet_vertices: u64,
    pub meshlet_triangles: u64,
    /// The meshlet lists the meshlet pass fills.
    pub meshlet_draws: u64,
    pub meshlet_draw_instances: u64,
    /// The meshlets the cull queues for the meshlet pass, laid out as the
    /// meshlet lists: each as its draw's packed instance, and its place in
    /// the draw's LOD.
    pub meshlet_work: u64,
}

/// A line of debug draw (see `voxear_core::debug_draw`), its colour sRGB.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuDebugLine {
    pub from: [f32; 3],
    /// `DEBUG_ON_TOP`.
    pub flags: u32,
    pub to: [f32; 3],
    /// Pixels wide, or 0 for the pass's usual width.
    pub width: f32,
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

/// The draw count buffer holds each instance list's count at the head of a
/// dispatch's arguments, (count, 1, 1), and a fourth word. With meshlet
/// culling, a list's draws are not written but their meshlets queued: the
/// first word is then how many workgroups the meshlet pass runs for them,
/// which the CPU never reads, and the fourth how many were queued. Each
/// meshlet list's count follows them all.
pub const DRAW_COUNT_STRIDE: u64 = 16;
pub const DRAW_COUNT_SIZE: u64 = LIST_COUNT as u64 * (DRAW_COUNT_STRIDE + 4);

/// Where an instance list's draw count sits in the draw count buffer, in bytes.
pub fn draw_count_offset(list: u32) -> u64 {
    u64::from(list) * DRAW_COUNT_STRIDE
}

/// Where a meshlet list's draw count sits in the draw count buffer, in bytes.
pub fn meshlet_count_offset(list: u32) -> u64 {
    u64::from(LIST_COUNT) * DRAW_COUNT_STRIDE + u64::from(list) * 4
}

/// The draw count buffer as the frame starts: no draws, and the dispatches'
/// other sizes one.
pub fn cleared_draw_counts() -> Vec<u32> {
    let mut words = vec![0; (DRAW_COUNT_SIZE / 4) as usize];
    for list in 0..LIST_COUNT as usize {
        words[list * 4 + 1..list * 4 + 3].fill(1);
    }
    words
}

/// A meshlet list's first entry and how many it holds, when every instance
/// list's draws can turn into `capacity` meshlets: the main view's lists
/// hold twice that, as a fading instance draws two LODs, and each shadow
/// cascade's holds it once. Mirrors `meshletListBase` in the shaders.
pub fn meshlet_list(list: u32, capacity: u32) -> (u64, u32) {
    let capacity = u64::from(capacity);
    let (base, held) = if list < LIST_SHADOW {
        (u64::from(list) * 2 * capacity, 2 * capacity)
    } else {
        ((4 + u64::from(list - LIST_SHADOW)) * capacity, capacity)
    };
    (base, held as u32)
}

/// Entries in all the meshlet lists together.
pub fn meshlet_lists_len(capacity: u32) -> u64 {
    (4 + CASCADES as u64) * u64::from(capacity)
}

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
    /// Meshlets of the main view's draws, as the meshlet pass culled them,
    /// and those drawn into the shadows.
    pub const MESHLETS_TESTED: usize = CLUSTER_LIGHT_LINKS + 1;
    pub const MESHLETS_FRUSTUM_CULLED: usize = MESHLETS_TESTED + 1;
    pub const MESHLETS_CONE_CULLED: usize = MESHLETS_FRUSTUM_CULLED + 1;
    pub const MESHLETS_OCCLUSION_CULLED: usize = MESHLETS_CONE_CULLED + 1;
    pub const MESHLETS_DRAWN: usize = MESHLETS_OCCLUSION_CULLED + 1;
    pub const MESHLETS_SHADOW_DRAWN: usize = MESHLETS_DRAWN + 1;
    pub const COUNT: usize = MESHLETS_SHADOW_DRAWN + 1;
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
    /// `DRAWS_INSTANCES` or `DRAWS_MESHLETS`, for a draw pass.
    pub source: u32,
    /// Textures a post pass reads.
    pub textures: [u32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_shaders() {
        assert_eq!(size_of::<GpuMesh>(), 32 + 24 * MAX_LODS);
        assert_eq!(size_of::<GpuMeshlet>(), 48);
        assert_eq!(size_of::<GpuMaterial>(), 64);
        // Three matrices, ids and flags, then where its LODs end.
        assert_eq!(size_of::<GpuInstance>(), 192 + 32);
        assert_eq!(size_of::<GpuLight>(), 80);
        // Two matrices, six planes, nine 16-byte rows, the cascades, then the
        // view matrix and three rows of clustering and timing.
        let cascades = CASCADES * (64 + 96 + 8);
        let motion = 64 * 2 + 16;
        let post = 16 * 17;
        let meshlets = 16;
        let lighting = 16 * 4 + 16 * 6;
        let gi = 16 * GI_LEVELS * 2 + 64 * CASCADES + 32;
        assert_eq!(
            size_of::<GpuFrame>(),
            64 * 2 + 96 + 16 * 9 + cascades + 64 + 16 * 3 + motion + post + meshlets + lighting + gi
        );
        assert_eq!(size_of::<GpuGi>(), gi);
        assert_eq!(size_of::<GpuLighting>(), lighting);
        assert_eq!(size_of::<GpuPost>(), post);
        assert_eq!(size_of::<GpuBuffers>(), 21 * 8);
        assert_eq!(size_of::<GpuDebugLine>(), 48);
        assert!(size_of::<Push>() as u32 <= voxear_rhi::PUSH_CONSTANT_SIZE);
    }

    #[test]
    fn meshlet_lists_lie_end_to_end() {
        let capacity = 100;
        let mut next = 0;
        for list in 0..LIST_COUNT {
            let (base, held) = meshlet_list(list, capacity);
            assert_eq!(base, next, "list {list} starts where the one before ends");
            assert_eq!(held, if list < LIST_SHADOW { 200 } else { 100 });
            next += u64::from(held);
        }
        assert_eq!(next, meshlet_lists_len(capacity));
        let counts = cleared_draw_counts();
        assert_eq!(counts.len() as u64 * 4, DRAW_COUNT_SIZE);
        assert_eq!(&counts[4..8], [0, 1, 1, 0], "each list's dispatch is (count, 1, 1)");
        assert_eq!(meshlet_count_offset(0), LIST_COUNT as u64 * 16);
        assert!(counts[(meshlet_count_offset(0) / 4) as usize..].iter().all(|&count| count == 0));
    }
}
