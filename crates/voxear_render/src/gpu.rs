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
/// Lit as leaves are: wrapped, through the leaf, and with a rim.
pub const MATERIAL_FOLIAGE: u32 = 2;
/// Glass: drawn after the opaque scene, over what lies behind it, which it
/// bends, tints and frosts, reflecting its surroundings.
pub const MATERIAL_GLASS: u32 = 4;
/// Leaves that stay as made through the seasons (see `seasons::Leaves`).
pub const MATERIAL_LEAVES_STAY: u32 = 16;
/// How its glow moves: these bits hold a `Glow` (see `glow`).
pub const MATERIAL_GLOW_SHIFT: u32 = 5;
/// A plant's impostor, its emission the sphere its atlas was pictured in
/// and its normal scale that sphere's radius (see `impostors`).
pub const MATERIAL_IMPOSTOR: u32 = 128;

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
    /// Glass's index of refraction: how strongly it bends and reflects.
    pub ior: f32,
    /// How much plants drawn with it bend in the wind from the ground up,
    /// and how much their leaves flutter; 0 for neither.
    pub wind_bend: f32,
    pub wind_flutter: f32,
    /// How many times a second its glow goes round, when it moves (see
    /// `MATERIAL_GLOW_*`).
    pub glow_speed: f32,
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
    /// From how far from the camera it starts to fade out, by dithering,
    /// and how far it is gone; both 0 for never (see `foliage`). z holds a
    /// plant's tint, RGBA8 in its bits, 0 for none.
    pub fade: [f32; 4],
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

/// An instance that casts no shadow, as grass far from the camera.
pub const INSTANCE_NO_SHADOW: u32 = 4;

/// A plant's impostor: its quad turns to the camera and shows the picture
/// of the plant from nearest that side (impostor.slang). Rays pass through
/// it, as they would through a flat card.
pub const INSTANCE_IMPOSTOR: u32 = 16;

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
    /// Its first view in the local shadow atlas, or `NO_SHADOW`: a spot
    /// light's one, or the six faces of a cube round any other kind, in
    /// `CUBE_FACES`' order.
    pub shadow: u32,
    pub _pad: [u32; 3],
}

/// A light that casts no shadow, or none this frame.
pub const NO_SHADOW: u32 = u32::MAX;

/// Local lights' shadows: how many lights may cast them in a frame, how many
/// views of the atlas they share (a spot light takes one, any other six, a
/// face of a cube each), and how big each view is, laid out in rows of
/// `LOCAL_SHADOW_COLUMNS`.
pub const LOCAL_SHADOW_LIGHTS: usize = 8;
pub const LOCAL_SHADOW_VIEWS: usize = 48;
pub const LOCAL_SHADOW_TILE: u32 = 512;
pub const LOCAL_SHADOW_COLUMNS: u32 = 8;
pub const LOCAL_SHADOW_ATLAS: [u32; 2] = [
    LOCAL_SHADOW_TILE * LOCAL_SHADOW_COLUMNS,
    LOCAL_SHADOW_TILE * (LOCAL_SHADOW_VIEWS as u32).div_ceil(LOCAL_SHADOW_COLUMNS),
];

/// The local lights casting shadows this frame, as shaders read them
/// (`LocalShadows` in `shaders/common.slang`; see `shadows::local_views`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLocalShadows {
    /// Each view's projection from world space, its depth 0 at the light.
    pub view_proj: [Mat4Cols; LOCAL_SHADOW_VIEWS],
    /// Each shadow list's light: where it is and how far its shadow reaches,
    /// which its casters are culled by.
    pub casters: [[f32; 4]; LOCAL_SHADOW_LIGHTS],
    /// The atlas, or `NO_TEXTURE` while no local light casts a shadow.
    pub texture: u32,
    pub count: u32,
    pub _pad: [u32; 2],
}

impl Default for GpuLocalShadows {
    fn default() -> Self {
        Self { texture: NO_TEXTURE, ..bytemuck::Zeroable::zeroed() }
    }
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
/// The light falling on each surface from all round, as the GI's probes
/// find it: bounced light and the sky's, before the surface's colour.
pub const DEBUG_VIEW_GLOBAL_ILLUMINATION: u32 = 10;

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
    /// How bright the stars are.
    pub stars: f32,
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
    pub ddgi: GpuDdgi,
    pub wind: GpuWind,
    pub pushes: GpuPushes,
    pub seasons: GpuSeasons,
    pub local_shadows: GpuLocalShadows,
}

/// Most pushers through foliage a frame takes, the nearest the camera.
pub const MAX_PUSHERS: usize = 16;
/// The grid of pushes through foliage round the camera: cells each way, and
/// how far apart, in metres.
pub const PUSH_CELLS: u32 = 128;
pub const PUSH_CELL: f32 = 0.5;

/// What pushes through foliage this frame, and where the grid of pushes
/// lies (`Pushes` in `shaders/common.slang`; see `foliage`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPushes {
    /// Each pusher's position and radius.
    pub pushers: [[f32; 4]; MAX_PUSHERS],
    /// Each pusher's strength, four to a row.
    pub strengths: [[f32; 4]; MAX_PUSHERS / 4],
    /// The grid's lowest cell, in cells from the world's origin, x and z.
    pub origin: [i32; 2],
    pub cell: f32,
    pub count: u32,
}

/// The level's seasons, as shaders read them (`Seasons` in
/// `shaders/common.slang`; see `seasons`). Not enabled without a Seasons.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuSeasons {
    /// Each season's leaf colour, weighted by how far leaves turn to it, and
    /// that amount; spring, summer, autumn and winter.
    pub colors: [[f32; 4]; 4],
    /// Each season's snow, in y.
    pub looks: [[f32; 4]; 4],
    /// The colour dry leaves turn, and how much of a field dries.
    pub dry: [f32; 4],
    pub time: f32,
    pub variation: f32,
    pub patch_size: f32,
    pub wetness: f32,
    pub enabled: u32,
    /// How much of the snow and wetness reach what is not a plant.
    pub ground: f32,
    pub _pad: [u32; 2],
}

/// The level's wind, as shaders read it (`Wind` in `shaders/common.slang`;
/// see `wind`). Strength 0 without a Wind Zone.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuWind {
    /// Which way it blows, level: x and z.
    pub direction: [f32; 2],
    pub strength: f32,
    pub gusts: f32,
    pub gust_size: f32,
    pub gust_speed: f32,
    pub turbulence: f32,
    pub _pad: f32,
}

/// Ray-traced GI's probes, as shaders find them (`Ddgi` in
/// `shaders/common.slang`): a grid of probes round the camera in each of
/// `DDGI_CASCADES` cascades, each probe tracing rays against the scene and
/// keeping, in small octahedral maps, the light from each direction and how
/// far its rays went, which keeps light from passing walls.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuDdgi {
    /// Each cascade's lowest probe, in its probes from the world's origin,
    /// this frame and last; w unused.
    pub origin: [[i32; 4]; DDGI_CASCADES],
    pub previous_origin: [[i32; 4]; DDGI_CASCADES],
    /// The finest cascade's probe spacing, in metres.
    pub spacing: f32,
    /// The probes' irradiance and depth maps and their states, as textures.
    pub irradiance: u32,
    pub depth: u32,
    pub probes: u32,
    /// 1 while the probes light the scene, in place of the plain sky light.
    pub enabled: u32,
    /// The scene's slot in `scenes`, for ray queries.
    pub scene: u32,
    /// Frames left in which every probe traces, as the GI starts.
    pub warm_up: u32,
    /// How bright the bounced light is, 1 as measured.
    pub strength: f32,
    /// This frame's turn of the probes' rays, a rotation's rows, so the
    /// directions they sample change from frame to frame.
    pub rotation: [[f32; 4]; 3],
}

/// The ray-traced GI's cascades, each `DDGI_CASCADE_SCALE` times as far apart
/// as the one before, and the probes in each, across, up and deep. Three,
/// each twice the last, rather than two four times apart: with probes four
/// metres apart past the nearest sixteen, the light of a lamp smeared down a
/// corridor and changed before the camera's eyes as the fine probes took
/// over; the three reach as far, to sixty-four metres.
pub const DDGI_CASCADES: usize = 3;
pub const DDGI_CASCADE_SCALE: f32 = 2.0;
pub const DDGI_PROBES: [u32; 3] = [32, 16, 32];
/// Rays each probe traces when it is due. 256 rather than 128: with half the
/// noise in each update, lamp-lit walls no longer shimmer as the probes'
/// light settles (see `DDGI_KEEP` in ddgi.slang), for 0.44 ms at 1080p.
pub const DDGI_RAYS: u32 = 256;
/// Each probe's maps, texels across.
pub const DDGI_TEXELS: u32 = 8;

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
    /// The colour the sun's shadows take, as a share of its light; black
    /// for none.
    pub sun_shadow_tint: [f32; 3],
    /// 1 when the sky's light is the moon: its disc, and a night sky.
    pub moon: u32,
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
    pub _pad_gamma: f32,
    pub gain: [f32; 3],
    pub _pad_gain: f32,
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
    /// How much shiny surfaces whose normals turn fast across a pixel are
    /// roughened, against shimmer: `AntiAliasing::specular_smoothing`.
    pub specular_smoothing: f32,
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
    /// Each pixel's normal and roughness as the depth passes wrote them, or
    /// `NO_TEXTURE`, for TAA to tell mirrors by.
    pub surface_texture: u32,
    /// Screen-space reflections' image, or `NO_TEXTURE` without them; how
    /// far rays go and how thick things count as, in metres; the steps rays
    /// take; and the roughness past which surfaces show none.
    pub reflection_texture: u32,
    pub reflection_distance: f32,
    pub reflection_thickness: f32,
    pub reflection_steps: u32,
    pub reflection_max_roughness: f32,
    /// The part of auto exposure's histogram it averages, between these
    /// percentiles.
    pub exposure_low_percent: f32,
    pub exposure_high_percent: f32,
    /// How far the camera moved since last frame, from 0, still, to 1, a
    /// few centimetres or a fraction of a degree or more.
    pub camera_motion: f32,
    /// Bloom's colour, linear, multiplying its glow.
    pub bloom_tint: [f32; 3],
    /// How dark auto exposure leaves dark scenes, from 0 (as bright as day)
    /// to 1 (as the eye sees them).
    pub exposure_night: f32,
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
    /// Every LOD's indices, three a triangle, counting from its mesh's first
    /// vertex, for ray-traced GI to find what a ray hit.
    pub indices: u64,
    /// Without ray tracing: every mesh's hierarchy's nodes and triangle
    /// order, the table of where each mesh's start, and this frame's
    /// hierarchy over the scene's instances and those instances.
    pub bvh_nodes: u64,
    pub bvh_order: u64,
    pub bvh_table: u64,
    pub scene_nodes: u64,
    pub scene_instances: u64,
    /// The grid of pushes through foliage, kept from frame to frame.
    pub foliage_pushes: u64,
    /// Under the atmosphere, the sun's light and the sky's, worked out once
    /// a frame; 0 otherwise.
    pub sky_light: u64,
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
/// One list for each local light casting a shadow, drawn into each of its views.
pub const LIST_LOCAL_SHADOW: u32 = LIST_SHADOW + CASCADES as u32;
pub const LIST_COUNT: u32 = LIST_LOCAL_SHADOW + LOCAL_SHADOW_LIGHTS as u32;

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
/// cascade's and local light's holds it once. Mirrors `meshletListBase` in
/// the shaders.
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
    (4 + u64::from(LIST_COUNT - LIST_SHADOW)) * u64::from(capacity)
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
        assert_eq!(size_of::<GpuMaterial>(), 80);
        // Three matrices, ids and flags, then where its LODs end.
        assert_eq!(size_of::<GpuInstance>(), 192 + 32 + 16);
        assert_eq!(size_of::<GpuLight>(), 96);
        // Two matrices, six planes, nine 16-byte rows, the cascades, then the
        // view matrix and three rows of clustering and timing.
        let cascades = CASCADES * (64 + 96 + 8);
        let motion = 64 * 2 + 16;
        let post = 16 * 20;
        let meshlets = 16;
        let lighting = 16 * 4 + 16 * 6 + 16;
        let ddgi = size_of::<GpuDdgi>();
        // Four seasons' colours and looks, then the dry colour and two rows of settings.
        let seasons = 16 * 4 * 2 + 16 + 32;
        // Each view's matrix, each list's light, then the atlas and count.
        let local_shadows = 64 * LOCAL_SHADOW_VIEWS + 16 * LOCAL_SHADOW_LIGHTS + 16;
        assert_eq!(size_of::<GpuLocalShadows>(), local_shadows);
        assert_eq!(LOCAL_SHADOW_ATLAS, [4096, 3072], "as LOCAL_SHADOW_ATLAS in the shaders");
        assert_eq!(
            size_of::<GpuFrame>(),
            64 * 2 + 96 + 16 * 9 + cascades + 64 + 16 * 3 + motion + post + meshlets + lighting + ddgi + 32 + 16 * MAX_PUSHERS + 4 * MAX_PUSHERS + 16 + seasons + local_shadows
        );
        assert_eq!(size_of::<GpuDdgi>(), 32 * DDGI_CASCADES + 32 + 48);
        assert_eq!(size_of::<GpuLighting>(), lighting);
        assert_eq!(size_of::<GpuPost>(), post);
        assert_eq!(size_of::<GpuBuffers>(), 29 * 8);
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
