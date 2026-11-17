//! The renderer: a render graph over GPU-driven passes.
//!
//! Each frame the world is extracted into instance, light and frame data.
//! Compute passes cull instances against the frustum, the sun's shadow
//! cascades and a depth pyramid (two-phase occlusion culling), and pick a
//! LOD for each, writing indirect draws; then each draw's meshlets are
//! culled the same way, and against their normal cones, so the CPU never
//! decides visibility. See [`passes`] for the frame's pass order, and
//! [`post_passes`] for the post-processing after it.

mod alloc;
mod ambient_occlusion;
mod assets;
mod atmosphere;
pub use atmosphere::FarHaze;
mod box_model;
mod bvh;
mod canopy;
mod fsr3;
mod capture;
mod clouds;
mod cooked;
mod cooked_picture;
mod debug_pass;
mod fbx_import;
mod foliage;
mod frame_debugger;
mod glass;
mod gltf_import;
mod gpu;
mod ddgi;
pub mod display;
mod graph;
mod hot_reload;
mod ies;
pub mod lightmap_uvs;
pub mod lightmapper;
mod lightmaps;
mod lod;
mod looks;
mod map;
pub mod ocean;
mod lod_group;
mod ltc;
#[cfg(test)]
mod ltc_fit;
mod mesh;
mod mesh_tools;
mod meshlets;
mod model_lods;
mod model_renderer;
mod impostors;
mod effects;
mod forces;
mod particle_meshes;
mod particle_sim;
mod particles;
mod passes;
mod dice;
mod mist;
mod skeleton;
mod skinning;
mod smoke;
mod grass;
mod blocks;
mod terrain;
mod dug;
mod erosion;
/// The most steps a terrain's Erosion layer runs.
pub use erosion::MOST_STEPS as MOST_EROSION_STEPS;
mod tracks;
mod payload;
mod pipelines;
pub mod portals;
mod post;
mod post_passes;
mod post_volume;
mod reflection_probes;
mod previews;
mod scene;
pub mod sea;
mod seasons;
mod lamp_vsm;
mod shadow_cache;
mod shadows;
mod sound_rays;
mod texture_codec;
mod ui_pass;
mod ui_panels;
mod volumetric;
mod vsm;
mod water;
pub mod water_bodies;
pub mod weather;
pub mod weather_maps;
mod wind;

use std::{collections::HashMap, mem::ManuallyDrop, path::PathBuf};

use anyhow::{Result, ensure};
use voxear_core::{profile_marker, profile_scope};
use voxear_ecs::World;
use voxear_rhi::{
    Acquire, Acquired, Bindless, Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, GpuInfo, Image, ImageDesc, MemoryLocation,
    RenderDoc, Retired, Swapchain, TopLevel, WindowHandles, full_mip_count, vk,
};

pub use assets::AssetUsage;
pub use assets::{MaterialDesc, MaterialHandle, MeshHandle, TextureSource};
pub use box_model::{
    BoneRole, BoxBone, BoxClip, BoxHit, BoxKey, BoxLayer, BoxModel, BoxSkin, BoxTrack, Ease, Face, FaceRect, KEY_STEP,
    LARGEST_GRID, LONGEST_CLIP, MOST_COLORS, MOST_WIND_BEND, ModelBox, Sheet, StarterPlant, Voxels, is_box_model,
    on_step,
};
pub use capture::{Capture, CapturedImage};
pub use cooked::{COOKED_VERSION, CookedModel};
pub use cooked_picture::{COOKED_PICTURE_VERSION, CookedPicture, PictureUse, read_hdr, sized as sized_hdr};
pub use ui_panels::{MAX_UI_PANELS, PanelRect, UiPanel, UiPanelFrame, panels as ui_panel_rects};
pub use frame_debugger::{Channels, FrameImage, FrameLook, FramePass};
pub use gltf_import::{
    FilePicture, FileMaterial, ImportedModel, ImportedNode, PartPlace, PreparedModel, ReadFile, companion_files,
    prepare as prepare_model,
};
pub use gpu::{
    DEBUG_VIEW_AMBIENT_OCCLUSION, DEBUG_VIEW_CASCADES, DEBUG_VIEW_GLOBAL_ILLUMINATION, DEBUG_VIEW_LIGHT_COMPLEXITY,
    DEBUG_VIEW_LIGHTMAP_DENSITY, DEBUG_VIEW_LIT_WIREFRAME, DEBUG_VIEW_LOD, DEBUG_VIEW_MESHLETS, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, DEBUG_VIEW_UNLIT,
    DEBUG_VIEW_WIREFRAME, MATERIAL_PROPERTIES, NO_TEXTURE, STANDARD_SHADER,
};
pub use graph::{BufferAccess, FrameGraph, ImageAccess, ImageId, PassContext, RenderGraph};
pub use lod::{LodInfo, MAX_LODS, lod_at, lod_at_share, screen_share};
pub use lod_group::{LodCamera, LodChoices, LodGroup, LodLevel, LodMeasure, LodView, default_until};
pub use model_lods::lod_of;
pub use mesh::{MeshData, MeshTriangles, NO_WIND_MASKS, Primitive, Vertex};
pub use skinning::{HiddenBones, Pose};
pub use skeleton::{Bone, BonePose, CLIP_RATE, Clip, ModelAnimation, Skeleton, Skin, SkinVertex};
pub use post::{
    AmbientOcclusion, AmbientOcclusionQuality, AntiAliasing, AntiAliasingMode, Bloom, ContactShadows, DepthOfField, Exposure, Fog,
    GiSource, GlobalIllumination, Reflections,
    Grading, MotionBlur, PostProcess, ScriptPost, Tonemap, Tonemapper, UpscaleQuality, Upscaler, Vignette,
};
pub use previews::{
    MAX_PREVIEW_SIZE, MAX_VIEW_SIZE, PREVIEW_ATLAS_SIZE, PreviewCamera, PreviewId, PreviewImage, PreviewOptions,
    PreviewPart, PreviewPixels,
};
pub use ies::IesProfile;
pub use map::{GpuMapColumn, MAP_LEAST_TILT, MAX_MAP_SIZE, MapCamera, MapDraw, MapId, MapImage, MapLook};
pub use looks::{LOOK_VALUES, Look, LookName, LookShader, SunLook, WaterLook, WornLook};
pub use shadows::{LampShadows, SHADOW_DISTANCE, SHADOW_SPLIT, SunShadows};
pub use post_volume::{PostVolume, VolumeShape};
pub use reflection_probes::{PROBE_SIZE, ProbePixels, ProbeResolution, ProbeUpdate, ReflectionProbe, ReflectionProbeBake};
pub use sound_rays::{
    MAX_SOUND_RAYS, MAX_SOUND_SOURCES, MAX_SOUND_SPOTS, MAX_SOURCE_RAYS, SOUND_FIELD, SOUND_FIXED, SOUND_MATERIALS,
    SoundHeard, SoundMaterial, SoundRays, SoundSource,
};
pub use lightmapper::{BakeProgress, BakeStage, LightBake};
pub use lightmaps::{
    BAND_SHARES, BakedAtlas, BakedLighting, BakedProbes, LightBakeState, LightMode, LightProbeGroup, Lightmap, LightmapBake,
    LightmapPlace, MOST_FILLED_PROBES, ProbeFill, ProbeLight, harmonics,
};
pub use foliage::{Foliage, FoliageInteraction, Impostors, Plant, PlantSet, Plants, Walkers, scatter};
pub use tracks::TrackMaker;
pub use assets::Glow;
pub use impostors::Impostor;
pub use model_renderer::{ModelPart, ModelRenderer};
pub use seasons::{Leaves, Season, SeasonLook, Seasons};
pub use effects::{
    BuiltinTexture, EffectRef, Emitter, MAX_EMITTERS, ends_after as particles_end_after, longest_life as particles_longest_life, ParticleEffect, ParticleEffects, ParticlePictures, ParticleTexture, Starter,
};
pub use forces::{BLAST_TIME, Blast, ForceField, ForceKind, MAX_FORCES, ParticleBlasts};
pub use particle_meshes::{BuiltinMesh, ParticleMesh, ParticleMeshes};
pub use particle_sim::{DEFAULT_PARTICLE_BUDGET, MAX_HITS as MAX_PARTICLE_HITS, MAX_PARTICLE_BUDGET, ParticleBudget, ParticleCounts, ParticleHit};
pub use particles::{
    MAX_PARTICLE_LIGHTS, EmitShape, ParticleBlend, ParticleCollision, ParticleGraphInfo, ParticleGraphs, ParticlePlayback, ParticleSpace, ParticleSystem, ParticleTime,
    ParticleHalfSize, ParticleTrails,
};
pub use mist::{MAX_MIST_POINTS, MIST_RISE, Mist, MistTrail};
pub use smoke::{SMOKE_GRID, Smoke, SmokeBlast, SmokeCue, SmokeFill, SmokeHoles, SmokeShot, smoke_origin};
pub use grass::GrassLook;
pub use blocks::{BlockCounts, BlockLook, BlockTints, BlockVoxels, GpuBlockMotion, GpuMotionKey, GpuMovingBlock, MAX_MOTIONS, MAX_MOVING};
pub use terrain::{SurfaceLook, TerrainLook};
pub use wind::{WindShape, WindZone};
pub use ocean::{Ocean, OceanSampler, SeaEasing, SeaNow, SeaState, Weather, WindSource};
pub use water_bodies::{DryArea, DryShape, Liquid, Pool, Stream, StreamPoint, stream_over};
pub use scene::{
    Camera, CloudKind, Clouds, Light, LightKind, MAX_TRANSITIONS, MeshRenderer, RenderFeatures, Sky, SkyBody, SkyMode,
    ViewOptions,
    picture_sky_light, temperature_color,
};
pub use portals::{
    CameraFollows, Carry, Portal, PortalFar, PortalPose, PortalSide, PortalTraveller, Straddles, ViewCarry,
};
pub use voxear_rhi::TextureId;
pub use hot_reload::{CustomCode, compile_custom_shaders, engine_shader_dir};

use crate::{
    assets::Assets,
    gpu::{
        CLUSTER_COUNT, DRAW_COMMAND_SIZE, DRAW_COUNT_SIZE, GpuBuffers, GpuFrame, GpuInstance, GpuLight, LIST_COUNT,
        MAX_CLUSTER_LIGHTS, MESHLET_CONES, MESHLET_CULLING, MESHLET_RUNS, Mat4Cols, MeshletCapacity, Push,
        SKY_ATMOSPHERE, meshlet_lists_len, stat,
    },
    hot_reload::{CustomCompile, ShaderSet, ShaderWatcher},
    pipelines::{DEPTH_FORMAT, HDR_FORMAT, PYRAMID_FORMAT, Pipelines, VELOCITY_FORMAT},
    shadows::ATLAS_RESOLUTION,
};

const MAX_INSTANCES: u64 = 64 * 1024;
const MAX_LIGHTS: u64 = 1024;
/// The meshlet lists start with room for this many meshlets of each
/// instance list's draws, and grow with the scene (see `gpu::meshlet_list`).
const FIRST_MESHLET_CAPACITY: u32 = 4096;
/// How large a view's buffer of the blocks' indirect draws is made at
/// first, in bytes: room for three thousand runs.
const BLOCK_DRAWS_FIRST: u64 = 64 * 1024;
/// A view whose instances' draws could turn into more meshlets than this
/// draws whole LODs instead. The main view's lists hold twice as many, and
/// the meshlet pass runs a workgroup for every 32 of a list's, which stays
/// within the 65,535 workgroups every GPU can dispatch. Its lists for one
/// frame in flight then take 256 MB.
const MAX_MESHLET_CAPACITY: u32 = 65_535 * 16;
const OFFSCREEN_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;

/// What the GPU culled and drew, read back from a recent frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub instances: u32,
    pub frustum_culled: u32,
    pub occlusion_culled: u32,
    pub drawn_early: u32,
    pub drawn_late: u32,
    /// Main-view draws per LOD level.
    pub lods: [u32; MAX_LODS],
    pub shadow_draws: u32,
    /// Extra draws for the outgoing LOD of instances that are fading.
    pub fade_draws: u32,
    /// The most lights any one cluster had to shade.
    pub cluster_max_lights: u32,
    /// Light-to-cluster assignments across the whole grid.
    pub cluster_light_links: u32,
    /// Meshlets of the main view's draws that were tested, and of those,
    /// how many were outside the view, facing away, hidden behind what was
    /// drawn, and drawn. All zero with meshlet culling off.
    pub meshlets: u32,
    pub meshlets_frustum_culled: u32,
    pub meshlets_cone_culled: u32,
    pub meshlets_occlusion_culled: u32,
    pub meshlets_drawn: u32,
    /// Meshlets drawn into the sun's shadow cascades.
    pub shadow_meshlets: u32,
    /// Pieces of displaced surfaces diced and drawn, and those that did not
    /// fit in a frame's budget.
    pub diced_patches: u32,
    pub dice_overflow: u32,
    /// Auto exposure's adapted EV, and the scene's metered average
    /// luminance. Both stay zero while auto exposure is off.
    pub exposure_ev: f32,
    pub average_luminance: f32,
    /// What the main view drew of its worlds of blocks this frame: its
    /// draws and triangles, and those of the sun's cascades.
    pub blocks: BlockCounts,
    /// The simulated particles: how many are alive, and what their systems
    /// hold of the project's budget.
    pub particles: ParticleCounts,
    /// The main view's virtual shadow pages for the sun: how many it holds
    /// in its pool, how many it drew this frame, and how many it copied for
    /// what moves to be drawn over.
    pub virtual_shadow_pages: u32,
    pub virtual_shadow_drawn: u32,
    pub virtual_shadow_moving: u32,
    /// The main view's virtual shadow pages for lamps: how many it holds in
    /// its pool, and how many it drew this frame.
    pub lamp_shadow_pages: u32,
    pub lamp_shadow_drawn: u32,
}

impl RenderStats {
    fn read(counters: &[u32], exposure: &[f32]) -> Self {
        let mut lods = [0; MAX_LODS];
        lods.copy_from_slice(&counters[stat::LOD..stat::LOD + MAX_LODS]);
        Self {
            instances: counters[stat::TESTED],
            frustum_culled: counters[stat::FRUSTUM_CULLED],
            occlusion_culled: counters[stat::OCCLUSION_CULLED],
            drawn_early: counters[stat::DRAWN_EARLY],
            drawn_late: counters[stat::DRAWN_LATE],
            lods,
            shadow_draws: counters[stat::SHADOW_DRAWS],
            fade_draws: counters[stat::FADE_DRAWS],
            cluster_max_lights: counters[stat::CLUSTER_MAX_LIGHTS],
            cluster_light_links: counters[stat::CLUSTER_LIGHT_LINKS],
            meshlets: counters[stat::MESHLETS_TESTED],
            meshlets_frustum_culled: counters[stat::MESHLETS_FRUSTUM_CULLED],
            meshlets_cone_culled: counters[stat::MESHLETS_CONE_CULLED],
            meshlets_occlusion_culled: counters[stat::MESHLETS_OCCLUSION_CULLED],
            meshlets_drawn: counters[stat::MESHLETS_DRAWN],
            shadow_meshlets: counters[stat::MESHLETS_SHADOW_DRAWN],
            diced_patches: counters[stat::DICED_PATCHES],
            dice_overflow: counters[stat::DICE_OVERFLOW],
            exposure_ev: exposure[0],
            average_luminance: exposure[2],
            blocks: BlockCounts::default(),
            particles: ParticleCounts::default(),
            virtual_shadow_pages: 0,
            virtual_shadow_drawn: 0,
            virtual_shadow_moving: 0,
            lamp_shadow_pages: 0,
            lamp_shadow_drawn: 0,
        }
    }
}

/// Options fixed when the renderer is created.
#[derive(Clone, Debug, Default)]
pub struct RendererSettings {
    /// Load RenderDoc (when installed) so frames can be captured.
    pub renderdoc: bool,
    /// Where RenderDoc captures go; RenderDoc's own default when `None`.
    pub capture_folder: Option<PathBuf>,
    /// Recompile shaders when their source changes (source checkouts only).
    pub hot_reload: bool,
    /// Trace the GI's rays in a compute shader even where the GPU has ray
    /// tracing, as GPUs without it do: for testing that path.
    pub software_rays: bool,
}

fn attach_renderdoc(settings: &RendererSettings) -> Option<RenderDoc> {
    let renderdoc = RenderDoc::attach(settings.renderdoc)?;
    if let Some(folder) = &settings.capture_folder {
        renderdoc.set_capture_folder(folder, "frame");
    }
    Some(renderdoc)
}

/// Something the renderer draws into: the window or image it was made with
/// ([`OutputId::MAIN`]), or one added since, as the window of a panel the
/// editor floats on the desktop. Views and UIs name the one they draw into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OutputId(u32);

impl OutputId {
    /// The window or image the renderer was made with.
    pub const MAIN: Self = Self(0);
}

enum Target {
    Window(Swapchain),
    // Destroyed explicitly, which needs the GPU. `encoding` is what its pixels mean, as a window's would.
    Offscreen { image: ManuallyDrop<Image>, last: ImageAccess, encoding: display::Encoding },
}

/// A window or image drawn into each frame, with the UI drawn over it.
struct Output {
    id: OutputId,
    target: Target,
    /// A window's new size, its swapchain made again at it before the next frame.
    pending_size: Option<(u32, u32)>,
    ui: ui_pass::UiGpu,
    /// For a window after the first, signalled when its image may be drawn
    /// into, one for each frame in flight. The first uses the frame's own.
    ready: Vec<vk::Semaphore>,
}

impl Output {
    /// What its pixels mean: SDR, or an HDR encoding.
    fn encoding(&self) -> display::Encoding {
        match &self.target {
            Target::Window(swapchain) => swapchain.encoding,
            Target::Offscreen { encoding, .. } => *encoding,
        }
    }

    fn extent(&self) -> vk::Extent2D {
        match &self.target {
            Target::Window(swapchain) => swapchain.extent,
            Target::Offscreen { image, .. } => image.desc.extent(),
        }
    }

    /// The caller makes sure the GPU is idle.
    fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        self.ui.destroy(gpu, bindless);
        for semaphore in self.ready.drain(..) {
            unsafe { gpu.device.destroy_semaphore(semaphore, None) };
        }
        match &mut self.target {
            Target::Window(swapchain) => swapchain.destroy(gpu),
            Target::Offscreen { image, .. } => gpu.destroy_image(unsafe { ManuallyDrop::take(image) }),
        }
    }
}

/// The UI's quads for one frame in flight, every output's together, and
/// pointers that name them for the UI's shader, so the UI draws whether or
/// not any view did.
struct UiBuffers {
    quads: Buffer,
    /// What the UI's shaded backgrounds read.
    values: Buffer,
    pointers: Buffer,
}

struct FrameBuffers {
    frame: Buffer,
    instances: Buffer,
    lights: Buffer,
    draws: Buffer,
    draw_count: Buffer,
    draw_instances: Buffer,
    /// What the passes count, in GPU memory, copied to `stats_readback` at
    /// the end of the frame for the CPU to read.
    stats: Buffer,
    stats_readback: Buffer,
    clusters: Buffer,
    pointers: Buffer,
    debug_lines: Buffer,
    debug_triangles: Buffer,
    particle_systems: Buffer,
    smokes: Buffer,
    smoke_cells: Buffer,
    smoke_holes: Buffer,
    mists: Buffer,
    mist_points: Buffer,
    /// This view's UI panels (see `ui_panels`).
    ui_panels: Buffer,
    /// This view's terrains and the patches it draws of them (see `terrain`).
    terrain: Buffer,
    /// This view's worlds of blocks and their surfaces (see `blocks`).
    blocks: Buffer,
    /// Their lists' runs as indirect draws, made the first time a view
    /// draws blocks (see `blocks::ViewBlocks::write_draws`).
    block_draws: Option<Buffer>,
    /// The openings of the Portals this view sees (see `portals`).
    portal_quads: Buffer,
    /// The weather's grids round the viewer (see `weather_maps`).
    weather_maps: Buffer,
    /// This view's grass: its jobs and draws, its draws' indirect commands,
    /// and its blades (see `grass`).
    grass: Buffer,
    grass_args: Buffer,
    grass_blades: Buffer,
    /// Dicing's lists, counts and draws (see `dice`).
    diced_instances: Buffer,
    dice_counts: Buffer,
    dice_splits: Buffer,
    dice_patches: Buffer,
    dice_draws: Buffer,
    /// The sun's light and the sky's, worked out once a frame under the
    /// atmosphere (see `atmosphere::record`).
    sky_light: Buffer,
    /// The meshlet lists, made for `meshlet_capacity` (see `gpu::meshlet_list`).
    meshlet_draws: Buffer,
    meshlet_draw_instances: Buffer,
    /// The meshlets the cull queues for the meshlet pass, laid out as the lists.
    meshlet_work: Buffer,
    meshlet_capacity: MeshletCapacity,
    /// The sun's virtual shadow pages' table and what the frame asks for,
    /// made when first needed (see `vsm`).
    vsm: Option<VsmBuffers>,
    /// The same for the lamps' virtual shadow pages (see `lamp_vsm`).
    lamp_vsm: Option<VsmBuffers>,
    /// What its buffers' names start with.
    prefix: String,
}

/// A frame in flight's virtual shadow buffers: the page table it reads,
/// and the pages its view asks for, in GPU memory, where the requests'
/// atomics are fast, and copied out for the CPU to read when the frame comes
/// round again, if it asked.
struct VsmBuffers {
    /// The page table as the CPU writes it, and the copy in GPU memory
    /// shading reads, as each pixel reads it once a light.
    table: Buffer,
    gpu_table: Buffer,
    requests: Buffer,
    readback: Buffer,
    asked: bool,
}

impl VsmBuffers {
    /// Buffers named from `name`, for a table and requests of so many words.
    fn new(gpu: &Gpu, name: &str, slot: usize, (table_words, request_words): (usize, usize)) -> Result<Self> {
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let (table_size, source) = ((table_words * 4) as u64, vk::BufferUsageFlags::TRANSFER_SRC);
        let table = gpu.create_buffer(&format!("{name} table {slot}"), table_size, source, MemoryLocation::CpuToGpu)?;
        let usage = storage | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC;
        let gpu_table =
            gpu.create_buffer(&format!("{name} gpu table {slot}"), table_size, usage, MemoryLocation::GpuOnly)?;
        let size = (request_words * 4) as u64;
        let requests = gpu.create_buffer(&format!("{name} requests {slot}"), size, usage, MemoryLocation::GpuOnly)?;
        let readback = gpu.create_buffer(
            &format!("{name} requests readback {slot}"),
            size,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )?;
        Ok(Self { table, gpu_table, requests, readback, asked: false })
    }
}

impl FrameBuffers {
    /// Makes the virtual shadow buffers, the first time they are needed.
    fn fit_vsm(&mut self, gpu: &Gpu, slot: usize) -> Result<&mut VsmBuffers> {
        if self.vsm.is_none() {
            let words = (vsm::LEVELS * (vsm::WINDOW * vsm::WINDOW) as usize, vsm::REQUEST_WORDS);
            self.vsm = Some(VsmBuffers::new(gpu, &format!("{}vsm", self.prefix), slot, words)?);
        }
        Ok(self.vsm.as_mut().expect("made above"))
    }

    /// Makes the lamps' virtual shadow buffers, the first time they are needed.
    fn fit_lamp_vsm(&mut self, gpu: &Gpu, slot: usize) -> Result<&mut VsmBuffers> {
        if self.lamp_vsm.is_none() {
            let words = (lamp_vsm::TABLE_WORDS, lamp_vsm::REQUEST_WORDS);
            self.lamp_vsm = Some(VsmBuffers::new(gpu, &format!("{}lamp vsm", self.prefix), slot, words)?);
        }
        Ok(self.lamp_vsm.as_mut().expect("made above"))
    }

    /// Makes sure the blocks' indirect draws have room for `needed`, making
    /// the buffer again half as large again when they do not. The caller has
    /// waited for this slot's last frame, so the old one can go at once.
    fn fit_block_draws(&mut self, gpu: &Gpu, slot: usize, needed: usize) -> Result<&Buffer> {
        let size = (needed.max(1) * size_of::<blocks::BlockDraw>()) as u64;
        let have = self.block_draws.as_ref().map_or(0, |buffer| buffer.size);
        if size > have {
            let size = size.max(have + have / 2).max(BLOCK_DRAWS_FIRST);
            let name = format!("{}block draws {slot}", self.prefix);
            let made =
                gpu.create_buffer(&name, size, vk::BufferUsageFlags::INDIRECT_BUFFER, MemoryLocation::CpuToGpu)?;
            if let Some(old) = self.block_draws.replace(made) {
                gpu.destroy_buffer(old);
            }
        }
        Ok(self.block_draws.as_ref().expect("made above"))
    }

    /// Makes sure the meshlet lists have room for `needed`, making them
    /// again half as large again when they do not, so a growing scene does
    /// not make them every frame. The caller has waited for this slot's last
    /// frame, so the old ones can go at once.
    fn fit_meshlets(&mut self, gpu: &Gpu, slot: usize, needed: MeshletCapacity) -> Result<()> {
        let have = self.meshlet_capacity;
        if needed.all <= have.all && needed.two_sided <= have.two_sided {
            return Ok(());
        }
        let grow = |needed: u32, have: u32| match needed <= have {
            true => have,
            false => needed.max(have + have / 2).min(MAX_MESHLET_CAPACITY),
        };
        let capacity =
            MeshletCapacity { all: grow(needed.all, have.all), two_sided: grow(needed.two_sided, have.two_sided) };
        let [draws, instances, work] = create_meshlet_lists(gpu, &self.prefix, slot, capacity)?;
        gpu.destroy_buffer(std::mem::replace(&mut self.meshlet_draws, draws));
        gpu.destroy_buffer(std::mem::replace(&mut self.meshlet_draw_instances, instances));
        gpu.destroy_buffer(std::mem::replace(&mut self.meshlet_work, work));
        self.meshlet_capacity = capacity;
        Ok(())
    }
}

pub struct Renderer {
    renderdoc: Option<RenderDoc>,
    /// The GI's rays are traced in a compute shader rather than by the GPU's
    /// ray tracing, which it lacks or the settings turn off.
    software_rays: bool,
    show_capture: bool,
    gpu: Gpu,
    /// What it draws into, the one it was made with first.
    outputs: Vec<Output>,
    next_output: u32,
    frames: Frames,
    bindless: Bindless,
    graph: RenderGraph,
    assets: Option<Assets>,
    pipelines: Pipelines,
    /// The format the picture and the UI are drawn in: the first window's,
    /// or with HDR, a display image's, which the encode pass fits to each
    /// window (see `display`).
    output_format: vk::Format,
    /// The first window's SDR format, which other windows are made in.
    window_format: vk::Format,
    /// What the game asks of the display: HDR or not, paper white and the peak.
    display: display::DisplaySettings,
    watcher: Option<ShaderWatcher>,
    /// The shaders the pipelines are made from, as built in or as hot
    /// reload compiled them, before a project's custom shaders.
    shaders: ShaderSet,
    /// A project's custom shaders, compiled into the modules that draw
    /// materials (see [`set_custom_shaders`](Self::set_custom_shaders)).
    custom: CustomShaders,
    /// The 3D views drawn each frame: the first, which the stats are of, and
    /// any after it (see [`View`]).
    views: Vec<View>,
    frame_index: u64,
    last_time: Option<f32>,
    /// How long the last frame waited, in milliseconds: for its slot's
    /// previous frame on the GPU, and for the display to take an image.
    waited_ms: f32,
    stats: RenderStats,
    ui_buffers: Vec<UiBuffers>,
    /// UI panels in the world: their UIs, textures, and how large each showed.
    ui_panels: ui_panels::UiPanels,
    previews: previews::Previews,
    /// Maps of worlds of blocks, each drawn into an image of its own.
    maps: map::Maps,
    /// Plants' impostors waiting to be baked, and what bakes drew from.
    impostor_bakes: impostors::Bakes,
    /// This frame's skinned parts, posed once for every view.
    skinning: skinning::Skinning,
    /// Every simulated Particle System's particles, stepped once a frame.
    particle_sim: particle_sim::ParticleSim,
    /// Every terrain's heights on the GPU (see `terrain`).
    terrains: terrain::Terrains,
    blocks: blocks::BlockWorlds,
    gpu_decompression: bool,
    /// Meshlet culling draws with task and mesh shaders, where the GPU has them.
    mesh_shaders: bool,
    /// What the output is cleared to outside the view, linear.
    clear: [f32; 4],
    frame_debugger: frame_debugger::FrameDebugger,
    captures: capture::Captures,
    reflection_probes: reflection_probes::ReflectionProbes,
    sound_rays: sound_rays::SoundRayWork,
    /// A lighting bake under way, or done and not yet taken (see `lightmapper`).
    baking: Option<lightmapper::Baking>,
    /// How many of a bake's dispatches a frame records, whether that was set
    /// rather than worked out from how long they take, and how many the last
    /// frame recorded.
    bake_pace: usize,
    bake_pace_set: bool,
    bake_recorded: usize,
    /// The level's bake's lightmaps, on the GPU.
    baked_textures: lightmaps::BakedTextures,
}

impl Renderer {
    /// Renders into a window.
    ///
    /// # Safety
    /// The window must outlive the renderer.
    pub unsafe fn new(window: WindowHandles, width: u32, height: u32, settings: &RendererSettings) -> Result<Self> {
        let renderdoc = attach_renderdoc(settings);
        let (gpu, surface) = unsafe { Gpu::new(window)? };
        let vsync = std::env::var("VOXEAR_VSYNC").map(|v| v != "0").unwrap_or(true);
        let swapchain = Swapchain::new(&gpu, surface, width, height, vsync)?;
        Self::build(gpu, Target::Window(swapchain), renderdoc, settings)
    }

    /// Renders into an image with no window, for screenshots, tests and tools.
    pub fn new_offscreen(width: u32, height: u32, settings: &RendererSettings) -> Result<Self> {
        let renderdoc = attach_renderdoc(settings);
        let gpu = Gpu::new_headless()?;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let image = gpu.create_image("output", ImageDesc::new(width, height, OFFSCREEN_FORMAT, usage))?;
        let encoding = display::Encoding::Sdr;
        let output = Target::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined, encoding };
        Self::build(gpu, output, renderdoc, settings)
    }

    /// Renders into an image shown as `encoding`, as an HDR window's would
    /// be: HDR10's 10 bits or scRGB's half floats, read back as they are.
    /// For testing HDR output on a machine whose display has none.
    pub fn new_offscreen_hdr(
        width: u32,
        height: u32,
        encoding: display::Encoding,
        display: display::DisplaySettings,
        settings: &RendererSettings,
    ) -> Result<Self> {
        let renderdoc = attach_renderdoc(settings);
        let gpu = Gpu::new_headless()?;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let format = match encoding {
            display::Encoding::Sdr => OFFSCREEN_FORMAT,
            display::Encoding::Hdr10 => vk::Format::A2B10G10R10_UNORM_PACK32,
            display::Encoding::ScRgb => vk::Format::R16G16B16A16_SFLOAT,
        };
        let image = gpu.create_image("output", ImageDesc::new(width, height, format, usage))?;
        let output = Target::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined, encoding };
        let mut renderer = Self::build(gpu, output, renderdoc, settings)?;
        renderer.display = display.held();
        Ok(renderer)
    }

    fn build(gpu: Gpu, target: Target, renderdoc: Option<RenderDoc>, settings: &RendererSettings) -> Result<Self> {
        let hot_reload = settings.hot_reload;
        let frames = Frames::new(&gpu)?;
        let mut bindless = Bindless::new(&gpu)?;
        let graph = RenderGraph::new(&gpu, FRAMES_IN_FLIGHT)?;
        // Without ray tracing, the GI's rays walk hierarchies of the meshes' triangles in a compute shader.
        let software_rays = settings.software_rays || gpu.acceleration_structure.is_none();
        let assets = Assets::new(&gpu, &mut bindless, software_rays)?;
        // Windows after the first, and images added, are SDR, in the first's SDR format.
        let (window_format, encoding) = match &target {
            Target::Window(swapchain) => (swapchain.format.format, swapchain.encoding),
            Target::Offscreen { encoding, .. } if encoding.is_hdr() => (OFFSCREEN_FORMAT, *encoding),
            Target::Offscreen { image, encoding, .. } => (image.desc.format, *encoding),
        };
        let output_format = if encoding.is_hdr() { display::DISPLAY_FORMAT } else { window_format };
        let shaders = ShaderSet::embedded();
        let pipelines = Pipelines::new(&gpu, &bindless, (output_format, window_format), &shaders)?;
        let first = View::new(&gpu, 0)?;
        let ui_buffers = (0..FRAMES_IN_FLIGHT).map(|slot| create_ui_buffers(&gpu, slot)).collect::<Result<_>>()?;
        let main =
            Output { id: OutputId::MAIN, target, pending_size: None, ui: ui_pass::UiGpu::default(), ready: Vec::new() };
        Ok(Self {
            software_rays,
            renderdoc,
            show_capture: false,
            gpu,
            outputs: vec![main],
            next_output: 1,
            frames,
            bindless,
            graph,
            assets: Some(assets),
            pipelines,
            output_format,
            window_format,
            display: display::DisplaySettings::default(),
            watcher: if hot_reload { ShaderWatcher::new() } else { None },
            shaders,
            custom: CustomShaders::default(),
            views: vec![first],
            frame_index: 0,
            last_time: None,
            waited_ms: 0.0,
            stats: RenderStats::default(),
            ui_buffers,
            ui_panels: ui_panels::UiPanels::default(),
            previews: previews::Previews::default(),
            maps: map::Maps::default(),
            impostor_bakes: impostors::Bakes::default(),
            skinning: skinning::Skinning::default(),
            particle_sim: particle_sim::ParticleSim::new(FRAMES_IN_FLIGHT),
            terrains: terrain::Terrains::default(),
            blocks: blocks::BlockWorlds::default(),
            gpu_decompression: true,
            mesh_shaders: true,
            clear: [0.0, 0.0, 0.0, 1.0],
            frame_debugger: Default::default(),
            captures: Default::default(),
            reflection_probes: Default::default(),
            sound_rays: Default::default(),
            baking: None,
            bake_pace: 2,
            bake_pace_set: false,
            bake_recorded: 0,
            baked_textures: Default::default(),
        })
    }

    pub fn gpu(&self) -> &GpuInfo {
        &self.gpu.info
    }

    /// What a reflection probe's Obj shows, as captured, read back from the
    /// GPU for a bake to save: none until it has been captured. Waits for
    /// the GPU.
    pub fn reflection_probe_pixels(&self, entity: voxear_ecs::Entity) -> Result<Option<ProbePixels>> {
        let Some((resolution, slot)) = self.reflection_probes.slot_of(entity) else { return Ok(None) };
        let Some(image) = self.graph.kept_image(&reflection_probes::array_name(resolution)) else { return Ok(None) };
        reflection_probes::read_back(&self.gpu, image, slot).map(Some)
    }

    /// The `captures` a reflection probe's Obj had when its cube was last
    /// filled, by a capture or a load: none until it has been. A bake that
    /// asks for a capture knows it has been taken once this has caught up.
    pub fn reflection_probe_captured(&self, entity: voxear_ecs::Entity) -> Option<u32> {
        self.reflection_probes.captured(entity)
    }

    /// Rays for the game's sound to trace in the next frame, against the
    /// scene the GI traces, in place of any asked for before and not yet
    /// drawn; `None` for none. What they find comes back from
    /// [`sound_heard`](Self::sound_heard) once the GPU has finished that
    /// frame. Only the first view traces them, and only a frame with a
    /// view draws them.
    pub fn trace_sound(&mut self, rays: Option<SoundRays>) {
        self.sound_rays.ask(rays);
    }

    /// What the sound rays of a finished frame found, the latest, once.
    pub fn sound_heard(&mut self) -> Option<SoundHeard> {
        self.sound_rays.take_heard()
    }
    /// Starts baking the world's lighting (see `lightmapper`): every Obj
    /// with a Lightmap that receives, and every Light Probe Group's probes,
    /// at the settings in its `[post.global_illumination]`. The bake traces
    /// a little each frame from then on; [`light_bake_progress`](Self::light_bake_progress)
    /// says how far it has gone and [`take_light_bake`](Self::take_light_bake)
    /// gives what it made once done. A bake under way is given up for it.
    /// Returns how many parts, texels and probes it bakes.
    pub fn start_light_bake(&mut self, world: &World) -> Result<(usize, usize, usize)> {
        self.cancel_light_bake();
        let assets = self.assets.as_ref().expect("assets live until drop");
        let (parts, probes, settings) = lightmapper::gather(world, (&self.gpu, assets))?;
        let layout = lightmapper::prepare(&parts, probes, settings);
        let counts = (layout.places.len(), layout.texels.len(), layout.probes.len());
        self.baking = Some(lightmapper::Baking::start(&self.gpu, layout, FRAMES_IN_FLIGHT)?);
        Ok(counts)
    }

    /// How far the lighting bake under way has gone; none when there is
    /// none, or once it is done.
    pub fn light_bake_progress(&self) -> Option<BakeProgress> {
        self.baking.as_ref().filter(|baking| baking.done.is_none()).map(|baking| baking.progress())
    }

    /// What the lighting bake made, once it is done, taken once.
    pub fn take_light_bake(&mut self) -> Option<Result<LightBake>> {
        if self.baking.as_ref().is_some_and(|baking| baking.done.is_some()) {
            return self.baking.take().and_then(|mut baking| baking.done.take());
        }
        None
    }

    /// Gives up the lighting bake under way, if any.
    pub fn cancel_light_bake(&mut self) {
        if let Some(baking) = self.baking.take() {
            baking.cancel(&self.gpu);
        }
    }

    /// How many of a lighting bake's dispatches each frame records, each up
    /// to `lightmapper::PATHS_PER_DISPATCH` paths: more bakes sooner and
    /// slows the frames it runs in more. Unset, as many as take about
    /// `lightmapper::BAKE_FRAME_MS` of the GPU's time a frame. What a bake
    /// makes is the same either way.
    pub fn set_light_bake_pace(&mut self, dispatches: usize) {
        self.bake_pace = dispatches.clamp(1, lightmapper::MOST_DISPATCHES);
        self.bake_pace_set = true;
    }

    /// GPU milliseconds per pass from the latest completed frame.
    /// How long the last frame waited, in milliseconds: for the GPU to
    /// finish an earlier frame and for the display to take an image, as
    /// vsync paces the frames. A frame's time less this is what the CPU
    /// spent on it.
    pub fn waited_ms(&self) -> f32 {
        self.waited_ms
    }

    /// Each pass's GPU time from a recent frame, in milliseconds. Passes
    /// overlap and wait for the ones before them, so a light one can read as
    /// heavy, and added up they are not the frame's time (see `gpu_frame_ms`).
    pub fn timings(&self) -> &[(String, f32)] {
        self.graph.timings()
    }

    /// A recent frame's GPU time, in milliseconds, from its first pass's
    /// start to its last's end.
    pub fn gpu_frame_ms(&self) -> f32 {
        self.graph.frame_ms()
    }

    /// Culling and LOD counts from a recent frame.
    /// The strikes particles of systems that report them made since this was
    /// last asked, the oldest first: read back from the GPU a few frames
    /// after they happened.
    pub fn take_particle_hits(&mut self) -> Vec<ParticleHit> {
        self.particle_sim.take_hits()
    }

    pub fn stats(&self) -> RenderStats {
        let [virtual_shadow_pages, virtual_shadow_drawn, virtual_shadow_moving] = self.views[0].vsm_counts;
        let [lamp_shadow_pages, lamp_shadow_drawn] = self.views[0].lamp_counts;
        RenderStats {
            blocks: self.views[0].block_counts,
            virtual_shadow_pages,
            virtual_shadow_drawn,
            virtual_shadow_moving,
            lamp_shadow_pages,
            lamp_shadow_drawn,
            ..self.stats
        }
    }

    /// The first 3D view's camera and look.
    pub fn set_view_options(&mut self, options: ViewOptions) {
        self.views[0].options = options;
    }

    /// The 3D views to draw after the first, each frame, until set again:
    /// each into its rectangle of the output, through its camera, as the
    /// editor's Game view beside its Scene view. Each keeps its own culling,
    /// exposure and history from frame to frame, by its place in the list.
    pub fn set_more_views(&mut self, views: &[([u32; 4], ViewOptions)]) -> Result<()> {
        let views: Vec<_> = views.iter().map(|&(rect, options)| (OutputId::MAIN, rect, options)).collect();
        self.set_more_views_in(&views)
    }

    /// The views after the first as [`set_more_views`](Self::set_more_views)
    /// sets them, each drawn into an output of its own choosing: a Scene
    /// view in a window the editor floats on the desktop goes there.
    pub fn set_more_views_in(&mut self, views: &[(OutputId, [u32; 4], ViewOptions)]) -> Result<()> {
        while self.views.len() > views.len() + 1 {
            let view = self.views.pop().expect("more than one");
            view.retire(&mut self.frames);
        }
        while self.views.len() < views.len() + 1 {
            self.views.push(View::new(&self.gpu, self.views.len())?);
        }
        for (view, (output, rect, options)) in self.views[1..].iter_mut().zip(views) {
            (view.output, view.rect, view.options) = (*output, Some(*rect), *options);
        }
        Ok(())
    }

    /// The output the first 3D view draws into, the main one until set.
    pub fn set_view_output(&mut self, output: OutputId) {
        self.views[0].output = output;
    }

    /// Each upscaler, and why it cannot run here, or none where it can. One
    /// a view's settings choose that cannot is Voxear's own in its place.
    pub fn upscalers(&self) -> Vec<(Upscaler, Option<String>)> {
        let nvidia = self.gpu.info.name.contains("NVIDIA") || self.gpu.info.name.contains("GeForce");
        let fsr = self.pipelines.fsr3.is_none().then(|| {
            "this GPU cannot read and write storage images without a format, which AMD FSR's passes need".to_string()
        });
        let dlss = match nvidia {
            true => "NVIDIA DLSS is not built in: its SDK is NVIDIA's, for whoever makes the game to download under \
                     NVIDIA's own licence"
                .to_string(),
            false => "NVIDIA DLSS needs an NVIDIA RTX GPU".to_string(),
        };
        let xess = match cfg!(windows) {
            true => "Intel XeSS is not built in yet".to_string(),
            false => "Intel XeSS has no build for this system: Intel makes it for Windows alone".to_string(),
        };
        vec![(Upscaler::Voxear, None), (Upscaler::Fsr, fsr), (Upscaler::Dlss, Some(dlss)), (Upscaler::Xess, Some(xess))]
    }

    /// Asks the main window for HDR output, or SDR, with how bright paper
    /// white and the display's peak are, from the next frame. Where the
    /// display offers none of what is asked, it shows SDR, which
    /// [`display`](Self::display) tells.
    pub fn set_display(&mut self, settings: display::DisplaySettings) -> Result<()> {
        self.display = settings.held();
        let (hdr, brightness) = (self.display.hdr, (self.display.paper_white, self.display.peak));
        if let Some(Target::Window(swapchain)) = self.outputs.first_mut().map(|output| &mut output.target) {
            swapchain.set_hdr(&self.gpu, hdr, brightness)?;
        }
        self.match_output_format()
    }

    /// What is asked of the display, and what the main window shows and its
    /// display offers.
    pub fn display(&self) -> (display::DisplaySettings, display::DisplayState) {
        let state = match self.outputs.first().map(|output| &output.target) {
            Some(Target::Window(swapchain)) => {
                display::DisplayState { encoding: swapchain.encoding, offered: swapchain.offered }
            }
            Some(Target::Offscreen { encoding, .. }) => {
                let offered = display::Offered {
                    hdr10: *encoding == display::Encoding::Hdr10,
                    scrgb: *encoding == display::Encoding::ScRgb,
                };
                display::DisplayState { encoding: *encoding, offered }
            }
            None => display::DisplayState::default(),
        };
        (self.display, state)
    }

    /// Draws the picture into display images while any output is HDR, and
    /// straight into the windows otherwise, making the pipelines again when
    /// that changes.
    fn match_output_format(&mut self) -> Result<()> {
        let hdr = self.outputs.iter().any(|output| output.encoding().is_hdr());
        let wanted = if hdr { display::DISPLAY_FORMAT } else { self.window_format };
        if wanted == self.output_format {
            return Ok(());
        }
        self.gpu.wait_idle();
        let pipelines = Pipelines::new(&self.gpu, &self.bindless, (wanted, self.window_format), &self.shaders)?;
        self.pipelines.destroy(&self.gpu);
        self.pipelines = pipelines;
        self.output_format = wanted;
        tracing::info!("drawing for {}", if hdr { "HDR output" } else { "SDR output" });
        Ok(())
    }

    /// Adds a window to draw into as well, as a panel the editor floats on
    /// the desktop has. Its UI is set with [`set_output_ui`](Self::set_output_ui),
    /// and views go there with [`set_more_views_in`](Self::set_more_views_in).
    /// It never waits for the display: a frame its window is not ready for
    /// leaves it as it was, so one the desktop is not showing never holds up
    /// the others.
    ///
    /// # Safety
    /// The window must outlive its output: remove it with
    /// [`remove_output`](Self::remove_output) before the window closes.
    pub unsafe fn add_window_output(&mut self, window: WindowHandles, width: u32, height: u32) -> Result<OutputId> {
        let surface = unsafe { self.gpu.create_surface(window)? };
        let swapchain = match Swapchain::new_matching(&self.gpu, surface, (width, height), false, self.window_format) {
            Ok(swapchain) => swapchain,
            Err(error) => {
                unsafe { self.gpu.surface_loader.destroy_surface(surface, None) };
                return Err(error);
            }
        };
        let mut ready = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            ready.push(unsafe { self.gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? });
        }
        Ok(self.push_output(Target::Window(swapchain), ready))
    }

    /// Adds an image to draw into as well, as [`add_window_output`](Self::add_window_output)
    /// adds a window: for tests and tools, read back with
    /// [`read_output_pixels`](Self::read_output_pixels).
    pub fn add_offscreen_output(&mut self, width: u32, height: u32) -> Result<OutputId> {
        let image = self.create_output_image(width, height)?;
        let encoding = display::Encoding::Sdr;
        let target = Target::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined, encoding };
        Ok(self.push_output(target, Vec::new()))
    }

    fn create_output_image(&self, width: u32, height: u32) -> Result<Image> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        self.gpu.create_image("output", ImageDesc::new(width.max(1), height.max(1), self.window_format, usage))
    }

    fn push_output(&mut self, target: Target, ready: Vec<vk::Semaphore>) -> OutputId {
        let id = OutputId(self.next_output);
        self.next_output += 1;
        self.outputs.push(Output { id, target, pending_size: None, ui: ui_pass::UiGpu::default(), ready });
        id
    }

    /// Stops drawing into an output added since the renderer was made, and
    /// lets its window go. Views that drew there draw nowhere until moved.
    pub fn remove_output(&mut self, id: OutputId) {
        let Some(index) = self.outputs.iter().position(|o| o.id == id).filter(|_| id != OutputId::MAIN) else { return };
        self.gpu.wait_idle();
        let mut output = self.outputs.remove(index);
        output.destroy(&self.gpu, &mut self.bindless);
    }

    /// The outputs drawn into, the main one first.
    pub fn outputs(&self) -> Vec<OutputId> {
        self.outputs.iter().map(|o| o.id).collect()
    }

    /// An output's size in pixels.
    pub fn output_size(&self, id: OutputId) -> Option<(u32, u32)> {
        let extent = self.outputs.iter().find(|o| o.id == id)?.extent();
        Some((extent.width, extent.height))
    }

    /// Gives an output a new size, as its window was resized: a window's
    /// from the next frame, an image's at once.
    pub fn resize_output(&mut self, id: OutputId, width: u32, height: u32) -> Result<()> {
        let Some(index) = self.outputs.iter().position(|o| o.id == id) else { return Ok(()) };
        if let Target::Window(_) = self.outputs[index].target {
            self.outputs[index].pending_size = Some((width, height));
            return Ok(());
        }
        if self.outputs[index].extent() == (vk::Extent2D { width, height }) {
            return Ok(());
        }
        let image = self.create_output_image(width, height)?;
        self.gpu.wait_idle();
        if let Target::Offscreen { image: old, last, .. } = &mut self.outputs[index].target {
            self.gpu.destroy_image(unsafe { ManuallyDrop::take(old) });
            (*old, *last) = (ManuallyDrop::new(image), ImageAccess::Undefined);
        }
        Ok(())
    }

    /// Moves what every view keeps from last frame by `by`, as the world's
    /// origin moves everything (see `voxear_scene::WorldOrigin`): last
    /// frame's camera and model matrices, so motion vectors, TAA and every
    /// reprojection see nothing move; the GI's probes, the tracks, the foam
    /// and the sea floor, so none starts again; and every Foliage's plants,
    /// so none is built again. Call between frames, after the world moved.
    pub fn shift_origin(&mut self, by: voxear_core::math::Vec3) {
        for view in &mut self.views {
            view.shift_origin(by);
        }
        self.particle_sim.shift_origin(by);
    }


    /// Tells the views through `camera` that it went through a Portal:
    /// it now looks from where `by` takes where it looked from, and the
    /// picture goes on unbroken, as what it saw through the opening is what
    /// it sees now.
    pub fn carry_view(&mut self, camera: voxear_ecs::Entity, by: voxear_core::math::Mat4) {
        for view in &mut self.views {
            if view.options.camera.is_none_or(|own| own == camera) {
                view.carried(by);
            }
        }
    }

    /// Tells every view that `objs` went through Portals, each by its own
    /// move: they count as having stood still, not as having crossed the
    /// world in a frame.
    pub fn carry_objs(&mut self, objs: &[(voxear_ecs::Entity, voxear_core::math::Mat4)]) {
        for view in &mut self.views {
            view.carried_objs(objs);
        }
    }

    /// Makes each image the renderer keeps from frame to frame start out
    /// holding NaN wherever it holds floats, as memory a GPU has used for a
    /// while may: for tests, so that a pass reading what nothing wrote shows
    /// in the picture rather than reading the zeros fresh memory often holds.
    pub fn fill_new_images_with_nan(&mut self, fill: bool) {
        self.graph.fill_new_with_nan = fill;
    }

    /// How many pictures the renderer keeps from frame to frame, and how many
    /// views through Portals: for tests, to see what a far side holds.
    pub fn kept(&self) -> (usize, usize) {
        fn count(children: &[PortalChild]) -> usize {
            children.iter().map(|child| 1 + count(&child.view.portals)).sum()
        }
        (self.graph.kept_images(), self.views.iter().map(|view| count(&view.portals)).sum())
    }

    /// Forgets last frame for TAA and motion vectors. Call on camera cuts,
    /// where blending with the old view would smear.
    pub fn reset_history(&mut self) {
        for view in &mut self.views {
            view.reset_history();
        }
    }

    pub fn has_renderdoc(&self) -> bool {
        self.renderdoc.is_some()
    }

    /// Captures the next presented frame in RenderDoc and opens it.
    pub fn capture_next_frame(&mut self) {
        match &self.renderdoc {
            Some(renderdoc) => {
                renderdoc.trigger_capture();
                self.show_capture = true;
            }
            None => tracing::warn!("RenderDoc is not loaded; run with --renderdoc to enable captures"),
        }
    }

    /// Captures the next presented frame in RenderDoc without opening it, for
    /// a tool that opens the file itself; false when RenderDoc is not loaded.
    pub fn capture_next_frame_unopened(&mut self) -> bool {
        self.renderdoc.as_ref().inspect(|renderdoc| renderdoc.trigger_capture()).is_some()
    }

    /// Starts capturing everything submitted until [`end_capture`](Self::end_capture).
    pub fn start_capture(&self) {
        if let Some(renderdoc) = &self.renderdoc {
            renderdoc.start_capture(&self.gpu.instance);
        }
    }

    /// Ends a capture and returns the file RenderDoc wrote.
    pub fn end_capture(&self) -> Option<PathBuf> {
        let renderdoc = self.renderdoc.as_ref()?;
        renderdoc.end_capture(&self.gpu.instance).then(|| renderdoc.latest_capture()).flatten()
    }

    /// The file of the latest capture RenderDoc wrote, as one asked for with
    /// [`capture_next_frame`](Self::capture_next_frame) lands a frame later.
    pub fn latest_capture(&self) -> Option<PathBuf> {
        self.renderdoc.as_ref()?.latest_capture()
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.outputs[0].pending_size = Some((width, height));
    }

    pub fn primitive(&mut self, primitive: Primitive) -> Result<MeshHandle> {
        self.assets.as_mut().expect("assets live until drop").primitive(&self.gpu, primitive)
    }

    pub fn add_mesh(&mut self, mesh: &MeshData) -> Result<MeshHandle> {
        self.assets.as_mut().expect("assets live until drop").add_mesh(&self.gpu, mesh)
    }

    pub fn add_material(&mut self, desc: &MaterialDesc) -> Result<MaterialHandle> {
        self.assets.as_mut().expect("assets live until drop").add_material(&self.gpu, desc)
    }

    /// Changes a material in place, as editing a material asset does, so
    /// everything drawn with it changes from the next frame.
    pub fn update_material(&mut self, handle: MaterialHandle, desc: &MaterialDesc) -> Result<()> {
        self.assets.as_mut().expect("assets live until drop").set_material(&self.gpu, handle, desc)
    }

    /// Loads a `.gltf`, `.glb`, `.fbx` or `.boxmodel` file and returns its drawable pieces.
    /// `path` is the model's project path, and `read` reads it and every file
    /// it names, from wherever the content lives.
    pub fn import_model(&mut self, path: &str, read: &mut ReadFile) -> Result<ImportedModel> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::import(&self.gpu, &mut self.bindless, assets, path, read)
    }

    /// Uploads a model made with [`prepare_model`], for example on a
    /// loading thread.
    pub fn upload_model(&mut self, model: &PreparedModel) -> Result<ImportedModel> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::upload(&self.gpu, &mut self.bindless, assets, model)
    }

    /// Puts a model prepared again, as after its import settings changed,
    /// behind the mesh and material handles `old` was given, so whatever
    /// draws it shows the change from the next frame. Fails, changing
    /// nothing, when the model's parts are not the ones it had.
    pub fn reupload_model(&mut self, old: &ImportedModel, model: &PreparedModel) -> Result<ImportedModel> {
        let frame = self.frame_index;
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::reupload(&self.gpu, &mut self.bindless, assets, old, model, frame)
    }

    /// Uploads a cooked model, as the content build wrote it. All of its
    /// meshes and textures go up in one submission, decompressed by the GPU
    /// where it can.
    pub fn upload_cooked(&mut self, model: &CookedModel) -> Result<ImportedModel> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::upload_cooked(&self.gpu, &mut self.bindless, assets, model, self.gpu_decompression)
    }

    /// Whether cooked models decompress on the GPU. When false, a loading
    /// thread should call [`CookedModel::decompress`] first, so the frame
    /// does not have to.
    pub fn decompresses_on_gpu(&self) -> bool {
        self.gpu_decompression && self.gpu.decompression.is_some()
    }

    /// Turns GPU decompression off, or back on where the GPU has it, to
    /// compare the two paths.
    pub fn set_gpu_decompression(&mut self, enabled: bool) {
        self.gpu_decompression = enabled;
    }

    /// Whether meshlet culling draws with task and mesh shaders, rather than
    /// a compute pass and indirect draws: where the GPU has them, unless
    /// turned off.
    pub fn uses_mesh_shaders(&self) -> bool {
        self.mesh_shaders && self.pipelines.mesh.is_some()
    }

    /// Turns the mesh shader path off, or back on where the GPU has it, to
    /// compare the two. Both draw the same image.
    pub fn set_mesh_shaders(&mut self, enabled: bool) {
        self.mesh_shaders = enabled;
    }

    /// Frees a mesh once no frame in flight still draws with it. Built-in
    /// meshes are never freed.
    pub fn free_mesh(&mut self, mesh: MeshHandle) {
        let frame = self.frame_index;
        self.assets.as_mut().expect("assets live until drop").free_mesh(mesh, frame);
    }

    /// Frees a material once no frame in flight still uses it. The default
    /// material is never freed.
    pub fn free_material(&mut self, material: MaterialHandle) {
        let frame = self.frame_index;
        self.assets.as_mut().expect("assets live until drop").free_material(material, frame);
    }

    /// Uploads a picture as a texture with its mips, for the UI to show (see
    /// `voxear_ui::ImageRef`), which expects its colour premultiplied by its
    /// alpha. Freed with [`free_texture`](Self::free_texture).
    pub fn add_texture(&mut self, source: &TextureSource) -> Result<TextureId> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        assets.add_texture(&self.gpu, &mut self.bindless, source)
    }

    /// Uploads an HDR picture, light as it is, linear RGBA floats a pixel, as
    /// half floats with mips. Freed with [`free_texture`](Self::free_texture).
    pub fn add_hdr_texture(&mut self, name: &str, (width, height): (u32, u32), pixels: &[[f32; 4]]) -> Result<TextureId> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        let halves = texture_codec::to_f16_bytes(pixels);
        assets.add_hdr_texture(&self.gpu, &mut self.bindless, name, (width, height), &halves)
    }

    /// Uploads a picture a build cooked (see [`CookedPicture`]), its colours
    /// read as sRGB or as the values they are. Freed with
    /// [`free_texture`](Self::free_texture).
    pub fn add_cooked_picture(&mut self, name: &str, picture: &CookedPicture, srgb: bool) -> Result<TextureId> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        let (width, height) = picture.size();
        if let Some(halves) = picture.half_floats() {
            return assets.add_hdr_texture(&self.gpu, &mut self.bindless, name, (width, height), halves);
        }
        match picture.rgba() {
            Some(rgba) => {
                let source = TextureSource { name, width, height, rgba, srgb };
                assets.add_texture(&self.gpu, &mut self.bindless, &source)
            }
            None => {
                let format = picture.format().vulkan(srgb);
                assets.add_texture_mips(&self.gpu, &mut self.bindless, name, (width, height), format, picture.mips())
            }
        }
    }

    /// Uploads a colour grading lookup table (see `Grading::lut`) as it is:
    /// a picture `size` × `size` squares wide and `size` high, sRGB
    /// values, without compression or mipmaps, which would blur its colours
    /// together.
    pub fn add_lut(&mut self, rgba: &[u8], size: u32) -> Result<TextureId> {
        anyhow::ensure!(
            size >= 2 && rgba.len() == (size * size * size * 4) as usize,
            "a lookup table {size} high should be {} wide",
            size * size
        );
        let assets = self.assets.as_mut().expect("assets live until drop");
        let format = vk::Format::R8G8B8A8_UNORM;
        assets.add_texture_mips(&self.gpu, &mut self.bindless, "colour lookup table", (size * size, size), format, &[rgba.to_vec()])
    }

    /// Uploads an IES profile, baked over the sphere of directions, for a
    /// point or spot light's `profile`.
    pub fn add_light_profile(&mut self, profile: &IesProfile) -> Result<TextureId> {
        let texels: Vec<u16> = profile.bake().into_iter().map(ltc::half).collect();
        let bytes = bytemuck::cast_slice(&texels).to_vec();
        let assets = self.assets.as_mut().expect("assets live until drop");
        let size = (ies::IES_WIDTH, ies::IES_HEIGHT);
        assets.add_texture_mips(&self.gpu, &mut self.bindless, "light profile", size, vk::Format::R16_SFLOAT, &[bytes])
    }

    pub fn free_texture(&mut self, texture: TextureId) {
        let frame = self.frame_index;
        self.assets.as_mut().expect("assets live until drop").free_texture(texture, frame);
    }

    /// A mesh's box in its own space, min and max corners, for picking. The
    /// box of its bounding sphere for a mesh uploaded cooked.
    pub fn mesh_bounds(&self, mesh: MeshHandle) -> Option<(voxear_core::math::Vec3, voxear_core::math::Vec3)> {
        self.assets.as_ref()?.mesh_bounds(mesh)
    }

    /// Every mesh's box in its own space, as [`mesh_bounds`](Self::mesh_bounds)
    /// has them, with how many times they have changed, for a copy kept
    /// elsewhere to know when it is out of date.
    pub fn mesh_boxes(
        &self,
    ) -> Option<(u64, &std::collections::HashMap<MeshHandle, (voxear_core::math::Vec3, voxear_core::math::Vec3)>)> {
        Some(self.assets.as_ref()?.mesh_boxes())
    }

    /// A mesh's finest triangles in its own space, for picking: read back
    /// from the GPU the first time, which waits for it, and kept.
    pub fn mesh_triangles(&self, mesh: MeshHandle) -> Option<std::sync::Arc<MeshTriangles>> {
        match self.assets.as_ref()?.mesh_triangles(&self.gpu, mesh) {
            Ok(triangles) => triangles,
            Err(error) => {
                tracing::error!("reading back mesh {}: {error:#}", mesh.0);
                None
            }
        }
    }

    /// A mesh's bounding sphere in its own space, centre and radius.
    pub fn mesh_sphere(&self, mesh: MeshHandle) -> Option<(voxear_core::math::Vec3, f32)> {
        self.assets.as_ref()?.mesh_sphere(mesh)
    }

    /// A mesh's LODs, the most detailed first.
    pub fn mesh_lods(&self, mesh: MeshHandle) -> Option<Vec<LodInfo>> {
        self.assets.as_ref()?.mesh_lods(mesh)
    }

    /// What every LOD Group shows to `camera`, as a view through it chooses.
    pub fn lod_choices(&self, world: &voxear_ecs::World, camera: LodCamera) -> LodChoices {
        LodChoices::new(world, camera, &|mesh| self.mesh_sphere(mesh))
    }

    /// Asks for a preview of an object, its parts drawn as `options` say.
    /// It is drawn over the next frames, a few a frame; [`preview`](Self::preview)
    /// says where it is once drawn. The meshes and materials need only live
    /// until then.
    pub fn add_preview(&mut self, parts: Vec<PreviewPart>, options: PreviewOptions) -> PreviewId {
        self.previews.add(parts, options)
    }

    /// Draws a preview again, as `options` now say, with new parts if given:
    /// turned by a drag, or after its object changed.
    pub fn set_preview(&mut self, id: PreviewId, parts: Option<Vec<PreviewPart>>, options: PreviewOptions) {
        self.previews.set(id, parts, options);
    }

    /// Where a preview is drawn, to show it, or `None` while it waits.
    /// Asking keeps it in the atlas; one let go of to make room is drawn
    /// again.
    pub fn preview(&mut self, id: PreviewId) -> Option<PreviewImage> {
        self.previews.image(id)
    }

    /// Lets a preview go, freeing its place in the atlas.
    pub fn free_preview(&mut self, id: PreviewId) {
        self.previews.remove(id);
    }

    /// A map of a world of blocks, `side` cells a side (rounded up to a
    /// power of two), with nothing in it: its tiles are written with
    /// [`write_map_tile`](Self::write_map_tile), and it is drawn the frames
    /// [`draw_map`](Self::draw_map) asks.
    pub fn add_map(&mut self, side: u32) -> Result<MapId> {
        self.maps.add(&self.gpu, side)
    }

    /// The cells a map's window has along a side.
    pub fn map_side(&self, id: MapId) -> Option<u32> {
        self.maps.side(id)
    }

    /// Each block's colour on a map, by the block's number: sRGB.
    pub fn set_map_palette(&mut self, id: MapId, colors: &[[u8; 3]]) -> Result<()> {
        self.maps.set_palette(&self.gpu, id, colors)
    }

    /// Writes one tile of a map, the tile whose lowest cell is `at` along x
    /// and z: 32 by 32 columns, a row along x after another.
    pub fn write_map_tile(&mut self, id: MapId, at: [i32; 2], columns: &[GpuMapColumn]) {
        self.maps.write_tile(id, at, columns);
    }

    /// Asks for a map to be drawn with the next frame, as `draw` says. Asked
    /// every frame the map shows; it is drawn again only when what it shows
    /// or how it is seen changed.
    pub fn draw_map(&mut self, id: MapId, draw: MapDraw) {
        self.maps.draw(id, draw);
    }

    /// Where a map was drawn, to show it in the UI, once it has been.
    pub fn map_image(&self, id: MapId) -> Option<MapImage> {
        self.maps.image(id)
    }

    /// Lets a map go.
    pub fn free_map(&mut self, id: MapId) {
        self.maps.remove(id);
    }

    /// Asks for a preview's pixels, to save or use elsewhere: they are
    /// copied back once it is drawn, and [`preview_pixels`](Self::preview_pixels)
    /// hands them over a few frames later.
    pub fn read_preview(&mut self, id: PreviewId) {
        self.previews.read(id);
    }

    /// A preview's pixels asked for with [`read_preview`](Self::read_preview),
    /// once they are back, or `None` until then. Handed over once.
    pub fn preview_pixels(&mut self, id: PreviewId) -> Option<PreviewPixels> {
        self.previews.pixels(id)
    }

    /// GPU memory: each live allocation by name, and the blocks they are
    /// sub-allocated from.
    pub fn gpu_memory(&self) -> voxear_rhi::GpuMemory {
        self.gpu.memory()
    }

    /// How much of the GPU asset storage is in use.
    pub fn asset_usage(&self) -> AssetUsage {
        self.assets.as_ref().expect("assets live until drop").usage()
    }

    /// Draws the 3D view into `rect` (left, top, width, height in pixels of
    /// the output) instead of all of it, as the editor's Scene panel does.
    /// What lies outside is cleared, for the UI to draw over.
    pub fn set_view_rect(&mut self, rect: Option<[u32; 4]>) {
        self.views[0].rect = rect;
    }

    /// The colour around a view drawn into part of the output, as linear
    /// RGBA: the UI's background, so its gaps match what it paints.
    pub fn set_clear_color(&mut self, color: [f32; 4]) {
        self.clear = color;
    }

    /// Swaps in recompiled shaders when the watcher saw a change.
    fn reload_shaders(&mut self) {
        let started = std::time::Instant::now();
        let Some(result) = self.watcher.as_mut().and_then(ShaderWatcher::poll) else { return };
        let reloaded = result.and_then(|shaders| {
            self.gpu.wait_idle();
            let pipelines = Pipelines::new(&self.gpu, &self.bindless, (self.output_format, self.window_format), &shaders)?;
            Ok((pipelines, shaders))
        });
        match reloaded {
            Ok((pipelines, shaders)) => {
                self.pipelines.destroy(&self.gpu);
                self.pipelines = pipelines;
                // Compiled with the project's custom shaders, which are part of them now.
                self.shaders = shaders;
                tracing::info!("shaders reloaded in {:.0} ms", started.elapsed().as_secs_f32() * 1000.0);
            }
            Err(error) => tracing::error!("shader reload failed, keeping the previous shaders: {error:#}"),
        }
    }

    /// Compiles a project's custom shaders, `code` as its generated
    /// material and UI shader code (see `voxear_engine::shaders`), into the
    /// shaders that draw materials and the UI, on a thread of its own, and
    /// draws with them once they compile; until then, and if they do not,
    /// with those it had. The same code again does nothing; whether the code
    /// changed.
    pub fn set_custom_shaders(&mut self, code: impl Into<CustomCode>) -> bool {
        let code = code.into();
        if self.custom.code == code {
            return false;
        }
        self.custom.code = code.clone();
        if self.custom.compiling.is_some() {
            self.custom.waiting = Some(code);
            return true;
        }
        let dir = self.custom.dir();
        if let Some(watcher) = &mut self.watcher {
            watcher.custom = Some(dir.clone());
        }
        self.custom.compiling = Some(CustomCompile::start(code, dir));
        self.custom.state = CustomShaderState::Compiling;
        true
    }

    /// Draws with modules compiled before with a project's custom shaders,
    /// as a built game does, without compiling anything.
    pub fn use_custom_modules(&mut self, modules: &[(String, Vec<u8>)]) -> Result<()> {
        let shaders = self.shaders.replaced(modules);
        self.gpu.wait_idle();
        let pipelines = Pipelines::new(&self.gpu, &self.bindless, (self.output_format, self.window_format), &shaders)?;
        self.pipelines.destroy(&self.gpu);
        self.pipelines = pipelines;
        self.custom.state = CustomShaderState::Ready;
        Ok(())
    }

    /// Where a project's custom shaders stand: compiling, drawn with, or
    /// failed, with Slang's error naming the shader and the line.
    pub fn custom_shaders(&self) -> &CustomShaderState {
        &self.custom.state
    }

    /// Swaps in a project's custom shaders once they compiled.
    fn finish_custom_shaders(&mut self) {
        if !self.custom.compiling.as_mut().is_some_and(CustomCompile::finished) {
            return;
        }
        let compiled = self.custom.compiling.take().expect("checked").take();
        let made = compiled.and_then(|modules| {
            let shaders = self.shaders.replaced(&modules);
            self.gpu.wait_idle();
            Ok((Pipelines::new(&self.gpu, &self.bindless, (self.output_format, self.window_format), &shaders)?, shaders))
        });
        match made {
            Ok((pipelines, shaders)) => {
                self.pipelines.destroy(&self.gpu);
                self.pipelines = pipelines;
                self.shaders = shaders;
                self.custom.state = CustomShaderState::Ready;
                tracing::info!("custom shaders compiled");
            }
            Err(error) => {
                tracing::error!("custom shaders did not compile: {error:#}");
                self.custom.state = CustomShaderState::Failed(format!("{error:#}"));
            }
        }
        if let Some(code) = self.custom.waiting.take() {
            self.custom.code = CustomCode::default();
            self.set_custom_shaders(code);
        }
    }

    /// Draws the world's cameras, lights and mesh renderers, into each output.
    pub fn render(&mut self, world: &World, time: f32) -> Result<()> {
        self.reload_shaders();
        self.finish_custom_shaders();
        for output in &mut self.outputs {
            if let (Some((width, height)), Target::Window(swapchain)) = (output.pending_size.take(), &mut output.target)
            {
                swapchain.recreate(&self.gpu, width, height)?;
            }
        }
        // A window made again may show otherwise, as on a display without HDR.
        self.match_output_format()?;
        // A window shrunk to nothing, as when minimised, is left out, the first too.
        let mut drawn: Vec<bool> = self.outputs.iter().map(|o| o.extent().width > 0 && o.extent().height > 0).collect();
        if !drawn.contains(&true) {
            return Ok(());
        }

        let waiting = std::time::Instant::now();
        let frame = {
            profile_scope!("Wait for GPU");
            self.frames.begin(&self.gpu)?
        };
        let mut waited = waiting.elapsed();
        // This slot's previous frame has finished, and with it every frame that could use what was freed before it.
        let assets = self.assets.as_mut().expect("assets live until drop");
        assets.collect(&self.gpu, &mut self.bindless, self.frame_index);
        // A lighting bake reads its sums back once traced, and finishes on a thread of its own; while it traces, as
        // many dispatches a frame as take about a frame's share of the GPU, by how long the last ones took.
        if let Some(baking) = &mut self.baking {
            baking.update(&self.gpu, self.frame_index);
            let took = self.graph.timings().iter().find(|(name, _)| name == "Lightmap bake").map(|(_, ms)| *ms);
            if let (false, Some(ms), 1..) = (self.bake_pace_set, took, self.bake_recorded) {
                self.bake_pace = lightmapper::pace(self.bake_pace, ms / self.bake_recorded as f32);
            }
        }
        // The level's bake's lightmaps, uploaded when they change.
        self.baked_textures.update(world, (&self.gpu, &mut self.bindless), assets, self.frame_index)?;
        // Impostors asked for last frame are made, to bake before this frame's views draw them.
        self.impostor_bakes.prepare(assets, &self.gpu, &mut self.bindless)?;
        // Every posed model's skinned parts, for all the views.
        self.skinning.prepare(&self.gpu, assets, world, self.frame_index, frame.slot)?;
        self.captures.finish(&self.gpu, frame.slot);
        self.sound_rays.finish(frame.slot);
        // This slot's previous frame has finished, so its counters are final.
        let first = &self.views[0];
        let counters = bytemuck::cast_slice(&first.buffers[frame.slot].stats_readback.read()[..stat::COUNT * 4]);
        let exposure = bytemuck::cast_slice(first.exposure.read());
        self.stats = RenderStats::read(counters, exposure);
        let acquired = {
            // Acquiring the next image waits for the display, as vsync paces the frames.
            profile_scope!("Wait for Display");
            let waiting = std::time::Instant::now();
            let acquired = self.acquire_images(&mut drawn)?;
            waited += waiting.elapsed();
            self.waited_ms = waited.as_secs_f32() * 1000.0;
            if !drawn.contains(&true) {
                return Ok(());
            }
            acquired
        };

        let delta_time = self.last_time.map_or(0.0, |last| (time - last).clamp(0.0, 0.25));
        self.last_time = Some(time);
        let frame_index = self.frame_index;
        self.frame_index += 1;
        // The views through Portals no longer seen for a while let what they hold go.
        for (index, view) in self.views.iter_mut().enumerate() {
            let root = if index == 0 { String::new() } else { format!("view {index} ") };
            let services = (&self.gpu, &mut self.graph, &mut self.bindless, &mut self.frames);
            free_portal_views(&mut view.portals, services, (&root, &root), frame_index);
        }
        // Every terrain's tiles that changed, staged to copy in before any view draws them.
        let assets = self.assets.as_mut().expect("assets live until drop");
        self.terrains.prepare(&self.gpu, assets, &mut self.frames, world, frame_index)?;
        // Block worlds make their chunks' meshes for rays nearest the first view's camera first, as it was last frame.
        let camera = self.views.first().and_then(|view| view.prev_camera).map(|(at, _)| at);
        self.blocks.prepare(&self.gpu, assets, &mut self.frames, world, (frame_index, camera))?;
        // Every simulated particle's step, for all the views.
        let stepping = (particles::time_of(world, time), (frame_index, frame.slot));
        self.particle_sim.prepare(&self.gpu, &mut self.frames, world, stepping.0, stepping.1)?;
        self.stats.particles = self.particle_sim.counts();

        let recording = profile_marker!("Record Passes").scope();
        let (output_format, display_settings) = (self.output_format, self.display);
        let Self {
            gpu,
            graph,
            bindless,
            pipelines,
            views,
            assets,
            outputs,
            ui_buffers,
            ui_panels,
            previews,
            maps,
            impostor_bakes,
            skinning,
            particle_sim,
            terrains,
            blocks,
            frames,
            frame_debugger,
            captures,
            reflection_probes,
            sound_rays,
            clear,
            mesh_shaders,
            software_rays,
            baking,
            bake_pace,
            baked_textures,
            ..
        } = self;
        let assets = assets.as_ref().expect("assets live until drop");
        let timings = if frame_debugger.on { graph.timings().to_vec() } else { Vec::new() };
        // This slot's previous frame is done: its FSR descriptor sets may be made again.
        if let Some(fsr3) = &pipelines.fsr3 {
            fsr3.reset_sets(gpu, frame.slot);
        }
        let mut fg = graph.begin(gpu, frame.slot);
        impostor_bakes.record(&mut fg, pipelines, assets, gpu, bindless)?;
        skinning.record(&mut fg, *pipelines, frame.slot);
        let particle_pool = particle_sim.import(&mut fg);
        let particle_step = particle_sim.step(&mut fg, frame.slot);
        let terrain_draws = terrains.record(&mut fg, frames);
        let block_draws = blocks.record(&mut fg, frames);
        let ui = &ui_buffers[frame.slot];
        // The UI's shaded backgrounds read the time from `output`.
        let ui_push = Push { buffers: ui.pointers.address, output: time.to_bits(), ..Push::default() };
        // UI panels in the world first, into their textures, for every view to draw.
        // The reflection probes every view shades with, and the one the first view captures or loads.
        let probes = reflection_probes.prepare(&mut fg, (gpu, bindless), world, (time, frame_index, frame.slot))?;
        let mut probe_job_done = false;
        let sound = sound_rays.prepare(gpu, frame.slot)?;
        let mut sound_done = false;
        // A lighting bake's dispatches for this frame, which the first camera's own view records.
        let mut bake = baking.as_mut().and_then(|baking| baking.frame(frame.slot, *bake_pace));
        let baked_frame = baked_textures.frame(baking.as_ref().is_some_and(|baking| baking.tracing()));
        let mut bake_dispatches = 0;
        let panel_pipelines = (pipelines.ui_panel, pipelines.ui_panel_mips, ui_push);
        let (panel_textures, (panel_quads, panel_values)) =
            ui_panels.record(&mut fg, (gpu, bindless), panel_pipelines, (&ui.quads, &ui.values), (0, 0))?;
        // Each output drawn this frame, as the graph knows it.
        let images: Vec<Option<(ImageId, ImageDesc)>> = outputs
            .iter()
            .zip(&acquired)
            .zip(&drawn)
            .map(|((output, acquired), &drawn)| {
                let full = output.extent();
                let (image, view, desc, last) = match (&output.target, acquired) {
                    _ if !drawn => return None,
                    (Target::Window(swapchain), Some(acquired)) => {
                        let desc = ImageDesc::new(full.width, full.height, swapchain.format.format, swapchain.usage);
                        (acquired.image, acquired.view, desc, ImageAccess::Undefined)
                    }
                    (Target::Offscreen { image, last, .. }, _) => (image.handle, image.view, image.desc, *last),
                    (Target::Window(_), None) => return None,
                };
                let name =
                    if output.id == OutputId::MAIN { "output".to_string() } else { format!("output {}", output.id.0) };
                Some((fg.import_image(&name, (image, view), desc, last), desc))
            })
            .collect();
        // With HDR, the views and the UI draw into a display image of each output, which is fitted to it last.
        let windows = images.clone();
        let mut images = images;
        let hdr = output_format == display::DISPLAY_FORMAT && outputs.iter().any(|output| output.encoding().is_hdr());
        for (index, image) in images.iter_mut().enumerate() {
            let Some((_, desc)) = image.filter(|_| hdr) else { continue };
            let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
            let made = ImageDesc::new(desc.width, desc.height, output_format, usage);
            let name = if index == 0 { "display".to_string() } else { format!("display {index}") };
            *image = Some((fg.create_image(gpu, bindless, &name, made)?, made));
        }
        // What each output's views fit their picture to: how many times paper white its display shows at most,
        // and whether they draw into a display image.
        let fitted: Vec<(f32, bool)> = outputs
            .iter()
            .zip(&windows)
            .zip(&images)
            .map(|((output, window), drawn)| {
                let display_image = window.zip(*drawn).is_some_and(|((a, _), (b, _))| a != b);
                (display_settings.peak_ratio(output.encoding()), display_image)
            })
            .collect();
        // The first view drawn into each output clears around itself, and keeps the view for its UI's backdrop.
        let mut backdrops = vec![None; outputs.len()];
        let mut cleared = vec![false; outputs.len()];
        let mut push = None;
        for (index, view) in views.iter_mut().enumerate() {
            let Some(at) = outputs.iter().position(|o| o.id == view.output) else { continue };
            let Some((output, desc)) = images[at] else { continue };
            // A view after the first keeps images of its own, by name.
            fg.set_prefix(&if index == 0 { String::new() } else { format!("view {index} ") });
            let first = !std::mem::replace(&mut cleared[at], true);
            let keep_ldr = first && outputs[at].ui.wants_backdrop();
            let full = desc.extent();
            let ui_quads = ui.quads.address;
            let mesh_shaders = *mesh_shaders;
            let software_rays = *software_rays;
            let setup = ViewSetup {
                fitted: fitted[at],
                time,
                delta_time,
                frame_index,
                full,
                ui_quads,
                clear: *clear,
                first,
                keep_ldr,
                mesh_shaders,
                software_rays,
                posed: skinning.posed.clone(),
                simulated: (particle_sim.placed.clone(), particle_sim.pool_address(), particle_pool),
                particle_step,
                panels: panel_textures.clone(),
                terrains: terrain_draws.clone(),
                blocks: block_draws.clone(),
                probes: probes.map(|probes| reflection_probes::ProbeFrame { job: probes.job.filter(|_| !probe_job_done), ..probes }),
                sound: sound.filter(|_| !sound_done),
                baked: baked_frame.clone(),
                bake: bake.clone(),
            };
            let recorded =
                view.record(&mut fg, (gpu, bindless, pipelines, assets), world, &setup, output, frame.slot, None)?;
            if first {
                backdrops[at] = recorded.backdrop;
            }
            probe_job_done |= recorded.probe_job_done;
            sound_done |= recorded.sound_done;
            if recorded.bake_dispatches > 0 {
                bake_dispatches = recorded.bake_dispatches;
                bake = None;
            }
            for (entity, size) in recorded.panel_sizes {
                ui_panels.saw(entity, size);
            }
            if index == 0 {
                push = Some(recorded.push);
            }
        }
        fg.set_prefix("");
        for view in views.iter() {
            view.retire_fsr3(frames);
        }
        if probe_job_done {
            reflection_probes.take_job(world, time);
        }
        if sound_done {
            sound_rays.recorded(frame.slot);
        }
        if let Some(baking) = baking.as_mut().filter(|_| bake_dispatches > 0) {
            baking.recorded(bake_dispatches, frame_index);
        }
        self.bake_recorded = bake_dispatches;
        // An output no view drew into is cleared for its UI.
        for (&image, cleared) in images.iter().zip(&cleared) {
            if let (Some((output, _)), false) = (image, cleared) {
                let color = *clear;
                fg.add_pass("Clear output", &[(output, ImageAccess::ColorAttachment)], &[], move |ctx| {
                    post_passes::clear(ctx, output, color)
                });
            }
        }
        let preview_atlas = previews.record(&mut fg, (gpu, bindless), (pipelines, assets), frame.slot)?;
        let debugger_view = frame_debugger.view_image(&mut fg, gpu, bindless)?;
        let map_images = maps.record(&mut fg, (gpu, bindless), pipelines, frame.slot)?;
        for buffer in maps.take_retired() {
            frames.retire(Retired::Buffer(buffer));
        }
        let sampled: Vec<_> = preview_atlas.into_iter().chain(debugger_view).chain(map_images).collect();
        // Captures read RGBA8 as people see it, which a display image is not.
        let main = images[0].filter(|_| !fitted[0].1);
        if let Some((output, desc)) = main {
            captures.record(&mut fg, gpu, (output, desc), true, frame.slot)?;
        }
        ui_panels.end_frame();
        let mut first = (panel_quads, panel_values);
        for ((output, image), backdrop) in outputs.iter().zip(&images).zip(backdrops) {
            let Some((image, desc)) = *image else { continue };
            let target = (image, desc.extent());
            let buffers = (&ui.quads, &ui.values);
            let (quads, values) =
                output.ui.record(&mut fg, (pipelines.ui, ui_push), buffers, first, target, &sampled, backdrop);
            first = (first.0 + quads, first.1 + values);
        }
        if let Some((output, desc)) = main {
            captures.record(&mut fg, gpu, (output, desc), false, frame.slot)?;
        }
        // Each display image fitted to its window: HDR10's curve, scRGB's scale, or SDR's.
        for ((output, window), drawn) in outputs.iter().zip(&windows).zip(&images) {
            let (Some((target, desc)), Some((display, _))) = (*window, *drawn) else { continue };
            if target == display {
                continue;
            }
            let Some(encode) = pipelines.encoder(desc.format) else {
                anyhow::bail!("no encode pass for a window in {:?}", desc.format);
            };
            let kind = match output.encoding() {
                display::Encoding::Sdr => 0,
                display::Encoding::Hdr10 => 1,
                display::Encoding::ScRgb => 2,
            };
            let brightness = [display_settings.paper_white.to_bits(), display_settings.peak.to_bits()];
            let push = Push {
                textures: [fg.texture(display).0, 0, 0, 0],
                output: kind,
                size: brightness,
                level: frame_index as u32,
                ..ui_push
            };
            let passes = [(display, ImageAccess::Sampled), (target, ImageAccess::ColorAttachment)];
            fg.add_pass("Encode", &passes, &[], move |ctx| post_passes::draw(ctx, encode, &push, target, false));
        }
        let images = windows;
        for (image, acquired) in images.iter().zip(&acquired) {
            match (image, acquired) {
                (Some((output, _)), Some(_)) => fg.transition(*output, ImageAccess::Present),
                // An offscreen output ends each frame ready to draw into, as the next frame expects, even after a
                // capture copied it once the UI was drawn.
                (Some((output, _)), None) => fg.transition(*output, ImageAccess::ColorAttachment),
                (None, _) => {}
            }
        }
        let push = push.unwrap_or(ui_push);
        frame_debugger.record(&mut fg, (gpu, bindless), (pipelines.frame_debug, push), debugger_view, &timings)?;
        let executing = profile_marker!("Execute").scope();
        fg.execute(gpu, frame.cmd, bindless);
        drop(executing);
        drop(recording);

        profile_scope!("Submit");
        let mut presents = Vec::new();
        let mut presented = Vec::new();
        for ((index, output), acquired) in self.outputs.iter().enumerate().zip(acquired) {
            if let (Target::Window(swapchain), Some(acquired)) = (&output.target, acquired) {
                presents.push((swapchain, acquired));
                presented.push(index);
            }
        }
        let recreate = self.frames.submit_presenting(&self.gpu, frame, presents)?;
        for (index, recreate) in presented.into_iter().zip(recreate) {
            let output = &mut self.outputs[index];
            if recreate {
                let full = output.extent();
                output.pending_size = Some((full.width, full.height));
            }
        }
        for (output, drawn) in self.outputs.iter_mut().zip(drawn) {
            if let (Target::Offscreen { last, .. }, true) = (&mut output.target, drawn) {
                *last = ImageAccess::ColorAttachment;
            }
        }
        if self.show_capture
            && let Some(renderdoc) = self.renderdoc.as_ref().filter(|r| !r.is_capturing())
        {
            renderdoc.show_ui();
            self.show_capture = false;
        }
        Ok(())
    }

    /// Acquires an image of each window's swapchain to draw into, leaving
    /// out a window that has none ready (`drawn` becomes false for it). The
    /// first window waits for its image; the others do not.
    fn acquire_images(&mut self, drawn: &mut [bool]) -> Result<Vec<Option<Acquired>>> {
        let mut acquired = Vec::with_capacity(self.outputs.len());
        for (output, drawn) in self.outputs.iter_mut().zip(drawn.iter_mut()) {
            let Target::Window(swapchain) = &output.target else {
                acquired.push(None);
                continue;
            };
            let result = match output.id {
                _ if !*drawn => Acquire::NotReady,
                OutputId::MAIN => match self.frames.acquire(&self.gpu, swapchain)? {
                    Some(image) => Acquire::Ready(image),
                    None => Acquire::OutOfDate,
                },
                _ => self.frames.try_acquire(&self.gpu, swapchain, output.ready[self.frames.slot()])?,
            };
            acquired.push(match result {
                Acquire::Ready(image) => Some(image),
                Acquire::OutOfDate => {
                    let full = output.extent();
                    output.pending_size = Some((full.width, full.height));
                    *drawn = false;
                    None
                }
                Acquire::NotReady => {
                    *drawn = false;
                    None
                }
            });
        }
        Ok(acquired)
    }

    /// Asks for a picture of a coming frame: see [`Capture`]. It is ready
    /// from [`take_capture`](Self::take_capture) a frame or two later, and
    /// several asked for come in the order they were asked, one from each
    /// frame that reaches their moment.
    pub fn request_capture(&mut self, capture: Capture) {
        self.captures.ask(capture);
    }

    /// The oldest capture that is ready, if one is.
    pub fn take_capture(&mut self) -> Option<CapturedImage> {
        self.captures.take()
    }

    /// Turns the frame debugger on or off, and says what it looks at. While
    /// it is on, each frame notes its passes for [`frame_passes`]; given a
    /// look, it copies that image after its pass for [`frame_debugger_view`].
    ///
    /// [`frame_passes`]: Self::frame_passes
    /// [`frame_debugger_view`]: Self::frame_debugger_view
    pub fn set_frame_debugger(&mut self, on: bool, look: Option<FrameLook>) {
        self.frame_debugger.on = on;
        self.frame_debugger.look = look;
    }

    /// The passes of the last frame drawn with the frame debugger on, in
    /// order, with the images each used.
    pub fn frame_passes(&self) -> &[FramePass] {
        &self.frame_debugger.passes
    }

    /// The image the frame debugger looks at, for a UI `Image` to show, and
    /// its size in pixels.
    pub fn frame_debugger_view(&self) -> Option<(voxear_ui::ImageRef, [u32; 2])> {
        let (texture, size) = self.frame_debugger.view?;
        Some((voxear_ui::ImageRef { texture: texture.0, uv: [0.0, 0.0, 1.0, 1.0] }, size))
    }

    /// What the UI draws over every frame from now on, until replaced. An
    /// empty frame draws nothing.
    pub fn set_ui(&mut self, frame: &voxear_ui::UiFrame) -> Result<()> {
        self.set_output_ui(OutputId::MAIN, frame)
    }

    /// The game's own UI, laid out for an area that starts at `origin` in
    /// the output, drawn under the UI [`set_ui`](Self::set_ui) gives: a
    /// player's whole window, or the editor's Game view. An empty frame draws
    /// nothing.
    pub fn set_game_ui(&mut self, frame: &voxear_ui::UiFrame, origin: [u32; 2]) -> Result<()> {
        self.set_output_game_ui(OutputId::MAIN, frame, origin)
    }

    /// The UI panels in the world, each laid out for its texture: those not
    /// among them are let go. Kept until set again.
    pub fn set_ui_panels(&mut self, frames: &[UiPanelFrame<'_>]) -> Result<()> {
        self.ui_panels.set(&self.gpu, &mut self.bindless, &mut self.graph, frames)
    }

    /// How large each UI panel showed in the last frame's views, the
    /// largest of them, in pixels: what its next texture is sized for.
    pub fn ui_panel_sizes(&self) -> &HashMap<voxear_ecs::Entity, [f32; 2]> {
        self.ui_panels.sizes()
    }

    /// What the UI draws over an output, as [`set_ui`](Self::set_ui) sets
    /// the main one's.
    pub fn set_output_ui(&mut self, id: OutputId, frame: &voxear_ui::UiFrame) -> Result<()> {
        let Some(output) = self.outputs.iter_mut().find(|o| o.id == id) else { return Ok(()) };
        output.ui.set(&self.gpu, &mut self.bindless, frame)
    }

    /// The game's own UI over an output, as [`set_game_ui`](Self::set_game_ui)
    /// sets the main one's: over the Game view, wherever it is.
    pub fn set_output_game_ui(&mut self, id: OutputId, frame: &voxear_ui::UiFrame, origin: [u32; 2]) -> Result<()> {
        let Some(output) = self.outputs.iter_mut().find(|o| o.id == id) else { return Ok(()) };
        output.ui.set_game(&self.gpu, &mut self.bindless, frame, origin)
    }

    /// Reads back the last rendered offscreen frame as RGBA8 (sRGB).
    /// `grid`'s ground worn down by `weather` on the GPU, as a terrain's
    /// Erosion layer wears it (see `voxear_terrain::erosion`): its heights
    /// after, in the same order. Waits until it is done.
    pub fn erode(
        &self,
        grid: &voxear_terrain::erosion::Grid,
        weather: &voxear_terrain::erosion::Weather,
    ) -> Result<Vec<f32>> {
        erosion::erode(&self.gpu, &self.bindless, &self.pipelines, grid, weather)
    }

    pub fn read_pixels(&mut self) -> Result<(u32, u32, Vec<u8>)> {
        self.read_output_pixels(OutputId::MAIN)
    }

    /// Reads back what was last drawn into an offscreen output as RGBA8 (sRGB).
    pub fn read_output_pixels(&mut self, id: OutputId) -> Result<(u32, u32, Vec<u8>)> {
        let output = self.outputs.iter_mut().find(|o| o.id == id).ok_or_else(|| anyhow::anyhow!("no such output"))?;
        let Target::Offscreen { image, last, .. } = &mut output.target else {
            anyhow::bail!("only offscreen renderers can read pixels back");
        };
        ensure!(*last != ImageAccess::Undefined, "nothing has been rendered yet");
        self.gpu.wait_idle();
        let layout = match *last {
            ImageAccess::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            _ => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        };
        let pixels = self.gpu.read_image(image.handle, image.desc, layout)?;
        *last = ImageAccess::TransferSrc;
        Ok((image.desc.width, image.desc.height, pixels))
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.gpu.wait_idle();
        let gpu = &self.gpu;
        if let Some(baking) = self.baking.take() {
            baking.cancel(gpu);
        }
        self.baked_textures.destroy(gpu);
        self.pipelines.destroy(gpu);
        self.reflection_probes.destroy(gpu);
        self.skinning.destroy(gpu);
        self.particle_sim.destroy(gpu);
        self.terrains.destroy(gpu);
        self.blocks.destroy(gpu);
        for view in std::mem::take(&mut self.views) {
            view.retire(&mut self.frames);
        }
        let baked = self.impostor_bakes.take_buffers().into_iter().chain(self.maps.take_buffers());
        let baked = baked.chain(self.sound_rays.take_buffers());
        for buffer in self.previews.take_buffers().into_iter().chain(self.captures.take_buffers()).chain(baked) {
            self.frames.retire(Retired::Buffer(buffer));
        }
        for buffers in self.ui_buffers.drain(..) {
            self.frames.retire(Retired::Buffer(buffers.quads));
            self.frames.retire(Retired::Buffer(buffers.values));
            self.frames.retire(Retired::Buffer(buffers.pointers));
        }
        self.ui_panels.destroy(gpu, &mut self.bindless);
        self.graph.destroy(gpu, &mut self.bindless);
        for output in &mut self.outputs {
            output.destroy(gpu, &mut self.bindless);
        }
        if let Some(assets) = self.assets.take() {
            assets.destroy(gpu, &mut self.bindless);
        }
        self.frames.destroy(gpu);
        self.bindless.destroy(gpu);
    }
}

/// One 3D view drawn each frame: where in the output, through which camera,
/// and what it keeps from frame to frame, so two views never share it: its
/// buffers for each frame in flight, which instances it saw and the LOD each
/// had, its exposure, and last frame's motion and TAA history.
struct View {
    /// Which view it is, the first 0, which picks its scenes' slots.
    number: usize,
    /// The scene as ray queries see it, one top level a frame in flight,
    /// made the first time a frame needs it, where ray tracing is on.
    top_levels: Vec<Option<TopLevel>>,
    /// What each slot's top level was last built from, so a frame whose
    /// instances are the same, as most are in a still level, reuses it.
    top_level_built: Vec<(u64, Vec<voxear_rhi::Instance>)>,
    /// What it drew of its worlds of blocks in its last frame.
    block_counts: BlockCounts,
    /// The virtual shadow pages it held, drew and copied for what moves in
    /// its last frame.
    vsm_counts: [u32; 3],
    /// The lamps' virtual shadow pages it held and drew in its last frame.
    lamp_counts: [u32; 2],
    /// Without ray tracing, the scene's hierarchy over its instances and the
    /// instances it names, one pair a frame in flight, made when first needed.
    scene_hierarchies: Vec<Option<(Buffer, Buffer)>>,
    /// The output it draws into, and where in it; all of it when `None`.
    output: OutputId,
    rect: Option<[u32; 4]>,
    options: ViewOptions,
    buffers: Vec<FrameBuffers>,
    /// Which instances were visible last frame; drives the early pass.
    visibility: Buffer,
    /// Each instance's current and previous LOD and fade progress.
    lod_state: Buffer,
    /// The grid of pushes through foliage round the camera (see `foliage`).
    foliage_pushes: Buffer,
    /// The map of tracks in the ground round the camera, and where its
    /// Track Makers were last frame (see `tracks`).
    tracks: Buffer,
    track_memory: tracks::TrackMemory,
    /// Auto exposure's adapted EV and whether it has started.
    exposure: Buffer,
    /// Last frame's model matrices and camera, for motion vectors.
    prev_models: HashMap<scene::Owner, Mat4Cols>,
    /// Last frame's Objs in their instances' order.
    prev_entities: Vec<scene::Owner>,
    /// Every Foliage's plants, built once (see `foliage`).
    foliage: foliage::FoliageCache,
    /// Its instances' baked light (see `lightmaps`).
    baked: lightmaps::ViewBaked,
    /// It has said its scene has more instances than it can draw.
    warned_instances: bool,
    prev_view_proj: Option<Mat4Cols>,
    /// Last frame's camera position and forward, for how far it moved.
    prev_camera: Option<(voxear_core::math::Vec3, voxear_core::math::Vec3)>,
    /// The size TAA's history was rendered at; `None` when it is stale.
    taa_history: Option<vk::Extent2D>,
    /// The same for ambient occlusion's.
    ao_history: Option<vk::Extent2D>,
    /// The atmosphere the kept atmosphere tables were drawn for; `None` when
    /// they are stale.
    atmosphere_tables: Option<gpu::GpuAtmosphere>,
    /// Whether last frame drew a fog volume this one can carry on from.
    fog_history: bool,
    /// The size of the picture last frame drew clouds for, where it drew
    /// any, for this one's to build on, and how far the clouds have
    /// drifted, in metres along x and z.
    cloud_history: Option<vk::Extent2D>,
    cloud_drift: [f64; 2],
    /// The sea's spectrum and FFT rows (see `water`).
    ocean: water::OceanBuffers,
    /// The ray-traced GI's origins last frame, its spacing, and how many
    /// frames it has run, when it had probes this one can carry on from.
    ddgi_history: Option<([[i32; 4]; gpu::DDGI_CASCADES], f32, u32)>,
    /// The light the GI's probes gathered last frame, to tell when it jumps.
    ddgi_lit_by: Option<ddgi::LightKey>,
    /// It has said its scene has too many meshlets to cull one by one.
    warned_meshlets: bool,
    /// The views through the Portals it sees, kept from frame to frame.
    portals: Vec<PortalChild>,
    /// What stands still in its local lights' reach, whose shadows they keep.
    shadow_cache: shadow_cache::ShadowCache,
    /// AMD FSR 3.1's images, where FSR takes TAA's place, and those it no
    /// longer needs, freed once no frame uses them (`retire_fsr3`).
    fsr3: std::sync::Arc<std::sync::Mutex<Option<fsr3::Fsr3State>>>,
    fsr3_retired: std::sync::Arc<std::sync::Mutex<Vec<fsr3::Fsr3State>>>,
    /// The sun's virtual shadow pages it keeps (see `vsm`).
    vsm: vsm::VirtualShadows,
    /// The lamps' virtual shadow pages it keeps (see `lamp_vsm`).
    lamp_pages: lamp_vsm::LampPages,
}

/// A view through a Portal, kept by the view it is seen in for as long as
/// that view sees the Portal: which Portal, and the frame it last drew in.
struct PortalChild {
    portal: voxear_ecs::Entity,
    view: Box<View>,
    used: u64,
}

/// What a view through a Portal draws into, and what it takes from the view
/// it is seen in (see `portals`).
#[derive(Clone, Copy)]
struct PortalPass {
    through: portals::Through,
    /// The colour and motion of the view it is seen in, which it draws
    /// over, and a depth of its own, opened where its Portal shows.
    hdr: ImageId,
    velocity: ImageId,
    depth: ImageId,
    /// The size of the view it is seen in, and its options: its own.
    size: (u32, u32),
    options: ViewOptions,
    /// That view's sky tables and sky light, which it reads, and how far
    /// its clouds have drifted.
    atmosphere: Option<atmosphere::Tables>,
    sky_light: u64,
    cloud_drift: [f64; 2],
    /// The sun casts shadows on its side, and its worlds of blocks' water is drawn.
    shadows: bool,
    water: bool,
    /// Its opening's rectangle in the view: its lowest and highest x and y, from -1 to 1.
    rect: [f32; 4],
    /// How many Portals deep it is: 1 for one seen straight from a camera.
    level: u32,
    /// The waves and caustics of the view it is seen in, when that drew water.
    waves: Option<water::SharedWaves>,
}

/// The Portals a view sees, and what the views through them need of it.
#[derive(Clone)]
struct Sees {
    /// Those whose far sides are drawn: the first of its openings.
    seen: Vec<portals::PortalSeen>,
    /// Each opening's share of mist, in its buffer's order.
    mists: Vec<f32>,
    /// Its camera, in its own space, whose middle is `offset` from the
    /// world's, and its projection without jitter.
    eye: voxear_core::math::Vec3,
    offset: voxear_core::math::Vec3,
    rotation: voxear_core::math::Quat,
    projection: voxear_core::math::Mat4,
    /// Its push constants, which name its openings, and its depth.
    push: Push,
    depth: ImageId,
    /// The size it is drawn at, and the size of the view it is drawn for.
    extent: vk::Extent2D,
    size: (u32, u32),
    options: ViewOptions,
    atmosphere: Option<atmosphere::Tables>,
    sky_light: u64,
    cloud_drift: [f64; 2],
    /// How many Portals deep it is itself: 0 for a camera's own view.
    level: u32,
    /// Its water's waves and caustics, for the views through its Portals to share.
    waves: Option<water::SharedWaves>,
}

/// What every view's frame shares.
struct ViewSetup {
    /// How many times paper white the output's display shows at most, and
    /// whether the view draws into a display image (see `display`).
    fitted: (f32, bool),
    time: f32,
    delta_time: f32,
    frame_index: u64,
    full: vk::Extent2D,
    /// The UI's quads, which every view's pointers name.
    ui_quads: u64,
    clear: [f32; 4],
    /// The first view drawn, which clears around itself.
    first: bool,
    /// Keep the finished view for the UI's backdrop blur.
    keep_ldr: bool,
    /// Meshlet culling draws with task and mesh shaders, where the pipelines have them.
    mesh_shaders: bool,
    /// The GI's rays are traced in a compute shader, not by ray tracing.
    software_rays: bool,
    /// This frame's posed parts, whose instances draw their posed vertices.
    posed: std::sync::Arc<HashMap<scene::Owner, skinning::Posed>>,
    /// This frame's simulated Particle Systems with particles out, the pool
    /// their particles are in, and the pool as the graph knows it.
    simulated: (std::sync::Arc<particles::PlacedSystems>, u64, Option<graph::BufferId>),
    /// This frame's step of the simulated particles, which the first view
    /// records among its passes.
    particle_step: Option<particles::Step>,
    /// This frame's UI panels' textures, which each view draws its panels with.
    panels: Vec<ui_panels::PanelTexture>,
    /// This frame's terrains, as every view draws them.
    terrains: std::sync::Arc<[terrain::Drawn]>,
    /// This frame's worlds of blocks, as every view draws them.
    blocks: std::sync::Arc<[blocks::DrawnBlocks]>,
    /// This frame's reflection probes, and the capture or load the first
    /// camera's own view records, until one has.
    probes: Option<reflection_probes::ProbeFrame>,
    /// This frame's sound rays, which the first camera's own view records.
    sound: Option<sound_rays::SoundPass>,
    /// The level's bake as every view lights with it, and a bake's dispatches
    /// the first camera's own view records, until one has.
    baked: lightmaps::BakedFrame,
    bake: Option<lightmapper::BakeFrame>,
}

/// What a view gives the frame after it: its push constants, which the UI
/// and frame debugger use for the first view, and its finished image
/// blurred for the UI's backdrop, when kept, with its size and place.
struct ViewRecorded {
    push: Push,
    backdrop: Option<(ImageId, [u32; 2], vk::Rect2D)>,
    /// How large each UI panel showed in the view, in pixels.
    panel_sizes: Vec<(voxear_ecs::Entity, [f32; 2])>,
    /// It recorded this frame's reflection probe capture or load.
    probe_job_done: bool,
    /// It recorded this frame's sound rays.
    sound_done: bool,
    /// How many of a lighting bake's dispatches it recorded.
    bake_dispatches: usize,
    /// The Portals a view through a Portal sees in turn, for the view it
    /// is seen in to draw the far sides of.
    sees: Option<Sees>,
}

impl View {
    /// The `number`th view's buffers, nothing seen or metered yet.
    fn new(gpu: &Gpu, number: usize) -> Result<Self> {
        let named = |name: &str| if number == 0 { name.to_string() } else { format!("view {number} {name}") };
        let buffers =
            (0..FRAMES_IN_FLIGHT).map(|slot| create_frame_buffers(gpu, &named(""), slot)).collect::<Result<_>>()?;
        let persistent = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let visibility =
            gpu.create_buffer(&named("visibility"), MAX_INSTANCES * 4, persistent, MemoryLocation::GpuOnly)?;
        let lod_state =
            gpu.create_buffer(&named("lod state"), MAX_INSTANCES * 4, persistent, MemoryLocation::GpuOnly)?;
        let cells = u64::from(gpu::PUSH_CELLS * gpu::PUSH_CELLS) * 8;
        let foliage_pushes = gpu.create_buffer(&named("foliage pushes"), cells, persistent, MemoryLocation::GpuOnly)?;
        let texels = u64::from(tracks::TRACK_TEXELS * tracks::TRACK_TEXELS) * 8;
        let tracks = gpu.create_buffer(&named("tracks"), texels, persistent, MemoryLocation::GpuOnly)?;
        // Host visible so the adapted exposure shows in the stats.
        let exposure = gpu.create_buffer(&named("exposure"), 16, persistent, MemoryLocation::GpuToCpu)?;
        // Nothing was visible, no LOD chosen and no exposure metered before the first frame.
        gpu.immediate(|cmd| unsafe {
            for buffer in [&visibility, &lod_state, &exposure, &foliage_pushes, &tracks] {
                gpu.device.cmd_fill_buffer(cmd, buffer.handle, 0, vk::WHOLE_SIZE, 0);
            }
        })?;
        Ok(Self {
            number,
            top_levels: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            top_level_built: (0..FRAMES_IN_FLIGHT).map(|_| (0, Vec::new())).collect(),
            block_counts: BlockCounts::default(),
            vsm_counts: [0; 3],
            lamp_counts: [0; 2],
            scene_hierarchies: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            output: OutputId::MAIN,
            rect: None,
            options: ViewOptions::default(),
            buffers,
            visibility,
            lod_state,
            foliage_pushes,
            tracks,
            track_memory: tracks::TrackMemory::default(),
            exposure,
            prev_models: HashMap::new(),
            prev_entities: Vec::new(),
            foliage: foliage::FoliageCache::default(),
            baked: Default::default(),
            warned_instances: false,
            prev_view_proj: None,
            prev_camera: None,
            taa_history: None,
            ao_history: None,
            atmosphere_tables: None,
            fog_history: false,
            cloud_history: None,
            cloud_drift: [0.0; 2],
            ocean: water::OceanBuffers::new(gpu, &named, FRAMES_IN_FLIGHT)?,
            ddgi_history: None,
            ddgi_lit_by: None,
            warned_meshlets: false,
            portals: Vec::new(),
            shadow_cache: shadow_cache::ShadowCache::default(),
            fsr3: Default::default(),
            fsr3_retired: Default::default(),
            vsm: vsm::VirtualShadows::default(),
            lamp_pages: lamp_vsm::LampPages::default(),
        })
    }

    /// Frees the FSR images it no longer needs, once no frame uses them.
    fn retire_fsr3(&self, frames: &mut Frames) {
        let retired = std::mem::take(&mut *self.fsr3_retired.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
        for state in retired {
            state.retire(frames);
        }
    }

    /// Lets its buffers go once no frame in flight uses them.
    fn retire(mut self, frames: &mut Frames) {
        self.retire_fsr3(frames);
        let state = self.fsr3.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(state) = state {
            state.retire(frames);
        }
        for buffer in self.baked.take_buffers() {
            frames.retire(Retired::Buffer(buffer));
        }
        for child in self.portals {
            child.view.retire(frames);
        }
        for buffers in self.buffers {
            let FrameBuffers {
                frame,
                instances,
                lights,
                draws,
                draw_count,
                draw_instances,
                stats,
                clusters,
                pointers,
                debug_lines,
                debug_triangles,
                particle_systems,
                smokes,
                smoke_cells,
                smoke_holes,
                mists,
                mist_points,
                ui_panels,
                terrain,
                blocks,
                portal_quads,
                weather_maps,
                grass,
                grass_args,
                grass_blades,
                diced_instances,
                dice_counts,
                dice_splits,
                dice_patches,
                dice_draws,
                sky_light,
                meshlet_draws,
                meshlet_draw_instances,
                meshlet_work,
                stats_readback,
                vsm,
                lamp_vsm,
                block_draws,
                ..
            } = buffers;
            for buffer in block_draws {
                frames.retire(Retired::Buffer(buffer));
            }
            for buffer in vsm.into_iter().chain(lamp_vsm).flat_map(|vsm| [vsm.table, vsm.gpu_table, vsm.requests, vsm.readback]) {
                frames.retire(Retired::Buffer(buffer));
            }
            let all = [frame, instances, lights, draws, draw_count, draw_instances, stats, clusters, pointers];
            let meshlets = [meshlet_draws, meshlet_draw_instances, meshlet_work];
            let diced = [diced_instances, dice_counts, dice_splits, dice_patches, dice_draws];
            let debug = [
                debug_lines,
                debug_triangles,
                particle_systems,
                smokes,
                smoke_cells,
                smoke_holes,
                mists,
                mist_points,
                ui_panels,
                terrain,
                blocks,
                portal_quads,
                weather_maps,
                grass,
                grass_args,
                grass_blades,
            ];
            for buffer in all.into_iter().chain(debug).chain([sky_light, stats_readback]).chain(meshlets).chain(diced) {
                frames.retire(Retired::Buffer(buffer));
            }
        }
        for buffer in [self.visibility, self.lod_state, self.exposure, self.foliage_pushes, self.tracks] {
            frames.retire(Retired::Buffer(buffer));
        }
        for buffer in self.ocean.into_buffers() {
            frames.retire(Retired::Buffer(buffer));
        }
        for level in self.top_levels.into_iter().flatten() {
            frames.retire(Retired::TopLevel(level));
        }
        for (nodes, instances) in self.scene_hierarchies.into_iter().flatten() {
            frames.retire(Retired::Buffer(nodes));
            frames.retire(Retired::Buffer(instances));
        }
    }

    /// Without ray tracing: this frame's hierarchy over the instances drawn,
    /// each by its mesh's box in the world, written for slot `slot`. Returns
    /// the addresses of its nodes and of the instances it names.
    fn trace_scene(
        hierarchy: &mut Option<(Buffer, Buffer)>,
        (gpu, assets): (&Gpu, &Assets),
        (instances, ids): (&[gpu::GpuInstance], &[u32]),
        slot: usize,
    ) -> Result<(u64, u64)> {
        use voxear_core::math::{Mat4, Vec3};
        if hierarchy.is_none() {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let nodes_size = 2 * MAX_INSTANCES * size_of::<bvh::BvhNode>() as u64;
            let instances_size = MAX_INSTANCES * size_of::<bvh::TraceInstance>() as u64;
            *hierarchy = Some((
                gpu.create_buffer(&format!("scene bvh {slot}"), nodes_size, usage, MemoryLocation::CpuToGpu)?,
                gpu.create_buffer(&format!("scene instances {slot}"), instances_size, usage, MemoryLocation::CpuToGpu)?,
            ));
        }
        let traced: Vec<_> = ids
            .iter()
            .map(|&id| (id as usize, &instances[id as usize]))
            .filter(|(_, instance)| instance.flags & gpu::INSTANCE_IMPOSTOR == 0)
            .filter_map(|(id, instance)| {
                let (low, high) = assets.mesh_bounds(MeshHandle(instance.mesh))?;
                let model = Mat4::from_cols_array_2d(&instance.model);
                let corners = (0..8).map(|c| {
                    let pick = |bit: u32, axis: usize| if c & bit == 0 { low[axis] } else { high[axis] };
                    model.transform_point3(Vec3::new(pick(1, 0), pick(2, 1), pick(4, 2)))
                });
                let (min, max) = corners.fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
                    (lo.min(p), hi.max(p))
                });
                let inverse = model.inverse();
                let row = |r: usize| [inverse.x_axis[r], inverse.y_axis[r], inverse.z_axis[r], inverse.w_axis[r]];
                let entry = bvh::TraceInstance {
                    inverse: [row(0), row(1), row(2)],
                    instance: id as u32,
                    mesh: instance.mesh,
                    _pad: [0; 2],
                };
                Some((entry, (min, max)))
            })
            .collect();
        let (nodes, ordered) = bvh::scene(&traced);
        let (node_buffer, instance_buffer) = hierarchy.as_ref().expect("made above");
        node_buffer.write(0, &nodes);
        if !ordered.is_empty() {
            instance_buffer.write(0, &ordered);
        }
        Ok((node_buffer.address, instance_buffer.address))
    }

    /// The scene's slot in the bindless set's `scenes` for frame slot `slot`,
    /// its top level built from this frame's instances by a pass recorded
    /// into `fg`: each of `ids`' instances with a bottom level, its number
    /// its place in the instance list. `None` without ray tracing.
    fn build_scene(
        (top_level, built, number): (&mut Option<TopLevel>, &mut (u64, Vec<voxear_rhi::Instance>), usize),
        fg: &mut FrameGraph<'_>,
        (gpu, bindless, assets): (&Gpu, &Bindless, &Assets),
        (instances, ids): (&[gpu::GpuInstance], &[u32]),
        slot: usize,
    ) -> Result<Option<u32>> {
        if gpu.acceleration_structure.is_none() {
            return Ok(None);
        }
        let index = (number * FRAMES_IN_FLIGHT + slot) as u32;
        if index >= voxear_rhi::MAX_SCENES {
            return Ok(None);
        }
        if top_level.is_none() {
            let level = TopLevel::new(gpu, &format!("view {number} scene {slot}"), MAX_INSTANCES as u32)?;
            bindless.set_scene(gpu, index, level.structure.handle);
            *top_level = Some(level);
        }
        let level = top_level.as_mut().expect("made above");
        let placed: Vec<voxear_rhi::Instance> = ids
            .iter()
            .map(|&id| (id as usize, &instances[id as usize]))
            .filter(|(_, instance)| instance.flags & gpu::INSTANCE_IMPOSTOR == 0)
            .filter_map(|(id, instance)| {
                let mesh = assets.bottom_level(MeshHandle(instance.mesh))?;
                let m = instance.model;
                let transform = [
                    m[0][0], m[1][0], m[2][0], m[3][0], m[0][1], m[1][1], m[2][1], m[3][1], m[0][2], m[1][2], m[2][2],
                    m[3][2],
                ];
                // Cut out, or left out of a lighting bake that traces this frame: its triangles come back as
                // candidates, each kept or passed through.
                let skipped = instance.flags & gpu::INSTANCE_NO_BAKE_CAST != 0;
                let opaque = !assets.is_cut_out(MaterialHandle(instance.material)) && !skipped;
                Some(voxear_rhi::Instance { mesh, transform, id: id as u32, opaque })
            })
            .collect();
        // The same instances, of the same meshes' bottom levels, as this slot's top level was last built from:
        // it stands as it is. Its pass is listed all the same, doing nothing, so the frame's passes stay the same
        // from frame to frame: the frame debugger keeps the pass looked at by its place in the list, and with
        // reflections tracing a still scene, the pass came only in the first frames and the look slid onto the next.
        let changes = assets.bottom_level_changes;
        if built.0 == changes && built.1 == placed && !placed.is_empty() {
            fg.add_pass("Scene BVH", &[], &[], |_| {});
            return Ok(Some(index));
        }
        level.write(&placed);
        *built = (changes, placed);
        let build = level.build();
        fg.add_pass("Scene BVH", &[], &[], move |ctx| {
            build.record(ctx.gpu, ctx.cmd);
            // What ray queries read after it waits for the build.
            let barrier = [vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
                .src_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_WRITE_KHR)
                .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::FRAGMENT_SHADER)
                .dst_access_mask(vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR)];
            let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
            unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
        });
        Ok(Some(index))
    }

    fn shift_origin(&mut self, by: voxear_core::math::Vec3) {
        use voxear_core::math::{Mat4, Vec4};
        let moved = by.extend(0.0);
        // A point now at p was at p - by last frame.
        if let Some(before) = self.prev_view_proj.as_mut() {
            *before = (Mat4::from_cols_array_2d(before) * Mat4::from_translation(-by)).to_cols_array_2d();
        }
        for model in self.prev_models.values_mut() {
            model[3] = (Vec4::from(model[3]) + moved).to_array();
        }
        if let Some((camera, _)) = self.prev_camera.as_mut() {
            *camera += by;
        }
        // Probes keep their slots, by their places mod the grid, only when the move is a whole number of grids.
        if let Some((origins, spacing, _)) = self.ddgi_history.as_mut() {
            let mut whole = true;
            let mut moved_by = [[0i32; 3]; gpu::DDGI_CASCADES];
            for (cascade, cells) in moved_by.iter_mut().enumerate() {
                let size = *spacing * gpu::DDGI_CASCADE_SCALE.powi(cascade as i32);
                for axis in 0..3 {
                    let probes = by[axis] / size;
                    let grids = (probes as i64).rem_euclid(i64::from(gpu::DDGI_PROBES[axis]));
                    whole &= probes.fract() == 0.0 && grids == 0;
                    cells[axis] = probes as i32;
                }
            }
            for (origin, cells) in origins.iter_mut().zip(moved_by).filter(|_| whole) {
                for axis in 0..3 {
                    origin[axis] += cells[axis];
                }
            }
            if !whole {
                self.ddgi_history = None;
            }
        }
        self.track_memory.shift(by);
        self.ocean.shift(by);
        self.foliage.shift(by);
        // A view through a Portal has a space of its own, which moves with its Portal: only the plants it keeps
        // where the world has them move.
        for child in &mut self.portals {
            child.view.shift_plants(by);
        }
    }

    fn shift_plants(&mut self, by: voxear_core::math::Vec3) {
        self.foliage.shift(by);
        for child in &mut self.portals {
            child.view.shift_plants(by);
        }
    }

    /// The camera went through a Portal (see `Renderer::carry_view`): last
    /// frame's camera is where `by` takes it, so what was seen through the
    /// opening keeps its motion and its TAA history. What is kept round the
    /// camera starts afresh on the far side.
    fn carried(&mut self, by: voxear_core::math::Mat4) {
        use voxear_core::math::Mat4;
        let back = by.inverse();
        if let Some(before) = self.prev_view_proj.as_mut() {
            *before = (Mat4::from_cols_array_2d(before) * back).to_cols_array_2d();
        }
        if let Some((camera, forward)) = self.prev_camera.as_mut() {
            (*camera, *forward) = (by.transform_point3(*camera), by.transform_vector3(*forward));
        }
        self.cloud_history = None;
        self.ddgi_history = None;
        self.fog_history = false;
        for child in &mut self.portals {
            child.view.reset_history();
        }
    }

    /// Objs that went through a Portal, each by its own move, were last
    /// frame where that takes where they were.
    fn carried_objs(&mut self, objs: &[(voxear_ecs::Entity, voxear_core::math::Mat4)]) {
        use voxear_core::math::Mat4;
        if objs.is_empty() {
            return;
        }
        // What was drawn of each on the far side is now what is drawn of it on the near side, and the other way
        // round: an Obj part-way through was drawn on both.
        let went: Vec<scene::Owner> = self.prev_models.keys().filter(|owner| !owner.far_side).copied().collect();
        for owner in went {
            let Some((_, by)) = objs.iter().find(|(entity, _)| *entity == owner.entity) else { continue };
            let far = scene::Owner { far_side: true, ..owner };
            let (here, there) = (self.prev_models.remove(&owner), self.prev_models.remove(&far));
            match (here, there) {
                (Some(here), Some(there)) => {
                    self.prev_models.insert(owner, there);
                    self.prev_models.insert(far, here);
                }
                (Some(here), None) => {
                    let carried = (*by * Mat4::from_cols_array_2d(&here)).to_cols_array_2d();
                    self.prev_models.insert(owner, carried);
                }
                _ => {}
            }
        }
    }

    fn reset_history(&mut self) {
        self.taa_history = None;
        self.ao_history = None;
        self.atmosphere_tables = None;
        self.fog_history = false;
        self.cloud_history = None;
        self.ddgi_history = None;
        self.prev_models.clear();
        self.prev_view_proj = None;
        self.prev_camera = None;
        for child in &mut self.portals {
            child.view.reset_history();
        }
    }

    /// Its rectangle within an output of `full`, at least a pixel each way.
    fn rect_in(&self, full: vk::Extent2D) -> vk::Rect2D {
        let Some([x, y, w, h]) = self.rect else {
            return vk::Rect2D { offset: vk::Offset2D::default(), extent: full };
        };
        let x = x.min(full.width - 1);
        let y = y.min(full.height - 1);
        let extent = vk::Extent2D { width: w.clamp(1, full.width - x), height: h.clamp(1, full.height - y) };
        vk::Rect2D { offset: vk::Offset2D { x: x as i32, y: y as i32 }, extent }
    }

    /// Gives each instance last frame's model matrix and the frame last
    /// frame's camera, so shaders can work out how far each pixel moved.
    /// Objs seen for the first time count as still, as plants of a Foliage
    /// always do. An instance in a place in the list another held last frame
    /// is marked new there.
    fn track_motion(&mut self, extracted: &mut scene::ExtractedFrame) {
        for (index, (instance, owner)) in extracted.instances.iter_mut().zip(&extracted.entities).enumerate() {
            if let Some(previous) = owner.plant.is_none().then(|| self.prev_models.get(owner)).flatten() {
                instance.prev_model = *previous;
            }
            if self.prev_entities.get(index) != Some(owner) {
                instance.flags |= gpu::INSTANCE_NEW;
            }
        }
        self.prev_entities.clone_from(&extracted.entities);
        self.prev_models.clear();
        let objs = extracted.entities.iter().zip(&extracted.instances).filter(|(owner, _)| owner.plant.is_none());
        self.prev_models.extend(objs.map(|(owner, instance)| (*owner, instance.model)));
        let current = extracted.frame.unjittered_view_proj;
        extracted.frame.prev_view_proj = self.prev_view_proj.replace(current).unwrap_or(current);
        // How far the camera moved, for TAA: 2 cm, or about a quarter of a degree, counts as moving.
        let vector = voxear_core::math::Vec3::from;
        let camera = (vector(extracted.frame.camera_position), vector(extracted.frame.camera_forward));
        let (before, looked) = self.prev_camera.replace(camera).unwrap_or(camera);
        let turned = looked.dot(camera.1).clamp(-1.0, 1.0).acos();
        extracted.frame.post.camera_motion = (before.distance(camera.0) / 0.02 + turned / 0.005).min(1.0);
    }

    /// Records the view's passes into its rectangle of `output`: culling,
    /// the scene, post-processing and debug lines.
    fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless, pipelines, assets): (&Gpu, &mut Bindless, &Pipelines, &Assets),
        world: &World,
        setup: &ViewSetup,
        output: ImageId,
        slot: usize,
        portal: Option<&PortalPass>,
    ) -> Result<ViewRecorded> {
        // A view through a Portal is the size of the view it is seen in, with its options.
        let view = match portal {
            Some(pass) => {
                self.options = pass.options;
                let extent = vk::Extent2D { width: pass.size.0, height: pass.size.1 };
                vk::Rect2D { offset: vk::Offset2D::default(), extent }
            }
            None => self.rect_in(setup.full),
        };
        let extent = view.extent;
        let through = portal.map(|pass| pass.through);
        // Where the view's own space has its middle: the world's, but for a view through a Portal.
        let offset = through.map_or(voxear_core::math::Vec3::ZERO, |through| through.offset);
        let extracting = profile_marker!("Extract").scope();
        let mut extracted = scene::extract(
            world,
            (extent.width, extent.height),
            (setup.time, setup.frame_index),
            &self.options,
            &|mesh| assets.mesh_sphere(mesh),
            through.as_ref(),
        );
        // What a view through a Portal goes without: what reads the screen, bounced light, and the sea.
        if portal.is_some() {
            let post = &mut extracted.post;
            post.ambient_occlusion.enabled = false;
            post.contact_shadows.enabled = false;
            post.reflections.enabled = false;
            post.global_illumination.enabled = false;
            post.fog.volumetric = false;
            extracted.ocean = None;
        }
        // What the project's modules leave out is not drawn.
        let features = world.resource::<RenderFeatures>().copied().unwrap_or_default();
        // What presses into the ground, from where each was last frame.
        let makers = if features.terrain { std::mem::take(&mut extracted.track_makers) } else { Vec::new() };
        let camera = voxear_core::math::Vec3::from(extracted.frame.camera_position);
        extracted.frame.tracks = self.track_memory.frame(camera, makers);
        if !features.water {
            extracted.ocean = None;
            extracted.water_bodies = Default::default();
        }
        // The scene is drawn at its render size, which TAA upscales to the view's.
        let output_extent = extent;
        let panel_projection = voxear_core::math::Mat4::from_cols_array_2d(&extracted.frame.unjittered_view_proj);
        let extent = vk::Extent2D { width: extracted.render_size.0, height: extracted.render_size.1 };
        // Every Foliage's plants within their draw distance, as instances of their own.
        let camera = voxear_core::math::Vec3::from(extracted.frame.camera_position);
        // Under the sea the air's fog is left out: the water's own takes its place (see `water`).
        if extracted.ocean.is_some_and(|(_, level, _)| camera.y < level) {
            extracted.post.fog.enabled = false;
        }
        // Each terrain's meshes for rays, among the Objs the GI traces and the sea floor is drawn from.
        if portal.is_none() {
            terrain::add_proxies(&setup.terrains, &mut extracted);
            blocks::add_proxies(&setup.blocks, &mut extracted);
        }
        if features.vegetation {
            // The plants round the camera where the world has it, moved into the view's own space.
            let first = extracted.instances.len();
            let seen_from = camera + offset;
            // The view's frustum moved out of its own space into the world's, where the plants are.
            let moved = |[x, y, z, d]: [f32; 4]| [x, y, z, d - voxear_core::math::Vec3::new(x, y, z).dot(offset)];
            let planes = extracted.frame.planes.map(moved);
            let sphere = |mesh| assets.mesh_sphere(mesh);
            let gathering = profile_marker!("Foliage").scope();
            self.foliage.gather(world, seen_from, &planes, &mut extracted, &sphere, &assets.impostors);
            drop(gathering);
            if portal.is_some() {
                for plant in &mut extracted.instances[first..] {
                    scene::rebase(&mut plant.model, offset);
                    scene::rebase(&mut plant.prev_model, offset);
                }
            }
        }
        if extracted.instances.len() > MAX_INSTANCES as usize && !std::mem::replace(&mut self.warned_instances, true) {
            let count = extracted.instances.len();
            tracing::warn!("{count} instances are more than the {MAX_INSTANCES} a view draws; the rest are left out");
        }
        let instancing = profile_marker!("Instances").scope();
        // Glass after the rest, past what culling, shadows and the GI's rays see.
        let opaque = glass::sort_last(&mut extracted.instances, &mut extracted.entities, camera, assets);
        // Each instance's baked light, from the level's bake, and while a bake traces, what is to be in it.
        let gi = extracted.post.global_illumination;
        let wanted = (gi.enabled && gi.source.uses_bake()) || self.options.debug_view == gpu::DEBUG_VIEW_LIGHTMAP_DENSITY;
        let baked_instances = self.baked.assign(
            (gpu, world, assets),
            &setup.baked,
            (&mut extracted.instances, &extracted.entities),
            (wanted, gi.texels_per_metre, offset),
            slot,
        )?;
        if !(gi.enabled && gi.source.uses_bake()) {
            extracted.frame.bake = gpu::GpuBake { sun_mode: LightMode::Realtime.order(), ..extracted.frame.bake };
            extracted.frame.bake.source = gpu::GI_REALTIME;
            (extracted.frame.bake.baked_lights, extracted.frame.bake.bake_lights) = (0, 0);
        } else if let Some((address, size, cell, origin)) = setup.baked.volume {
            let bake = &mut extracted.frame.bake;
            (bake.volume, bake.volume_size, bake.volume_cell) = (address, size, cell);
            // Into the view's own space, as the instances are.
            bake.volume_origin = (voxear_core::math::Vec3::from(origin) - offset).to_array();
        }
        let instance_count = extracted.instances.len().min(MAX_INSTANCES as usize);
        let opaque_count = opaque.min(instance_count);
        self.track_motion(&mut extracted);
        // Drawn again on a Portal's far side, it is posed as it is on the near one. A Foliage's plants never are.
        let objs = extracted.instances.iter_mut().zip(&extracted.entities).filter(|(_, owner)| owner.plant.is_none());
        for (instance, owner) in objs.filter(|_| !setup.posed.is_empty()) {
            if let Some(posed) = setup.posed.get(&scene::Owner { far_side: false, ..*owner }) {
                posed.apply(instance);
            }
        }
        drop(instancing);
        let shadow_setup = profile_marker!("Shadow Setup").scope();
        // The local lights' slots of their atlases, and what of the shadows of what stands still they keep.
        let depth_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let shadowing = !extracted.shadow_lights.is_empty();
        // Lamps' virtual shadow pages keep what stands still in place of the kept atlas.
        let virtual_lamps = shadowing
            && self.options.local_shadow_cache
            && portal.is_none()
            && extracted.lamp_shadows == LampShadows::Virtual;
        let caching = shadowing && self.options.local_shadow_cache && !virtual_lamps;
        let [atlas_width, atlas_height] = gpu::LOCAL_SHADOW_ATLAS;
        let atlas_desc = ImageDesc::new(atlas_width, atlas_height, DEPTH_FORMAT, depth_usage);
        let local_shadows = match shadowing {
            true => Some(fg.shared_image(gpu, bindless, "local shadow atlas", atlas_desc)?),
            false => None,
        };
        let kept_shadows = match caching {
            true => Some(fg.history_image(gpu, bindless, "kept local shadows", atlas_desc)?),
            false => None,
        };
        // The sun's shadows as virtual shadow maps, which, as the lamps' kept shadows, follow what stands still.
        let virtual_sun = extracted.has_sun && portal.is_none() && extracted.sun_shadows == SunShadows::Virtual;
        if caching || virtual_sun || virtual_lamps {
            self.shadow_cache.mark_still(&mut extracted, opaque_count, assets);
            self.shadow_cache.note_blocks(&setup.blocks);
            self.shadow_cache.note_terrains(&setup.terrains);
        } else {
            self.shadow_cache.forget();
        }
        let mut lamps = if shadowing {
            let fresh = kept_shadows.is_some_and(|kept| fg.is_new(kept));
            let lights = std::mem::take(&mut extracted.shadow_lights);
            let parts = (&mut extracted.frame.local_shadows, extracted.lights.as_mut_slice());
            let keeping = (caching || virtual_lamps, fresh, setup.frame_index);
            let (lamps, views) = self.shadow_cache.give_slots(&lights, parts, keeping);
            extracted.local_shadows = views;
            lamps
        } else {
            shadow_cache::Lamps::default()
        };
        // The lamps' pages: a lamp is read from them, and keeps what stands still out of the atlas, once it stands
        // still and its coarsest pages are drawn; those it draws this frame take what stands still into them.
        let lamp_pages = match virtual_lamps {
            true => {
                let desc = ImageDesc::new(lamp_vsm::POOL, lamp_vsm::POOL, DEPTH_FORMAT, depth_usage);
                let pool = fg.history_image(gpu, bindless, "lamp virtual shadow pool", desc)?;
                if fg.is_new(pool) {
                    self.lamp_pages.forget();
                }
                let buffers = self.buffers[slot].fit_lamp_vsm(gpu, slot)?;
                let read = buffers.readback.read();
                let asked: Option<&[u32]> =
                    buffers.asked.then(|| bytemuck::cast_slice(&read[..lamp_vsm::REQUEST_WORDS * 4]));
                let shadows = &extracted.frame.local_shadows;
                let views: Vec<Vec<gpu::Mat4Cols>> = extracted
                    .local_shadows
                    .iter()
                    .map(|&(first, count)| shadows.view_proj[first as usize..(first + count) as usize].to_vec())
                    .collect();
                // The tangent of half the angle each slot's views take in, border and all, from their first row.
                let tan_halves: [f32; gpu::LOCAL_SHADOW_LIGHTS] = std::array::from_fn(|at| {
                    views.get(at).and_then(|views| views.first()).map_or(1.0, |view| {
                        let across = voxear_core::math::Vec3::new(view[0][0], view[1][0], view[2][0]).length();
                        1.0 / across.max(1e-6)
                    })
                });
                let (changed, everything) = self.shadow_cache.for_lamps();
                let input =
                    lamp_vsm::LampInput { views, frame_index: setup.frame_index, slot, asked, changed, everything };
                let drawn = self.lamp_pages.update(input);
                self.lamp_counts = [drawn.held as u32, drawn.drawn.len() as u32];
                buffers.table.write(0, &drawn.table);
                buffers.asked = true;
                lamps.cached &= drawn.ready;
                lamps.redraw = drawn.drawing();
                extracted.frame.local_shadows.cached = lamps.cached;
                let addresses = (buffers.gpu_table.address, buffers.requests.address);
                let textures = fg.texture(pool).0;
                extracted.frame.lamp_vsm =
                    lamp_vsm::gpu(&drawn, &extracted.local_shadows, &tan_halves, addresses, textures);
                Some((pool, drawn))
            }
            false => {
                self.lamp_pages.forget();
                self.lamp_counts = [0; 2];
                None
            }
        };
        let (moving, changed, everything) = self.shadow_cache.take_for_sun();
        let virtual_shadows = match virtual_sun {
            true => {
                let desc = ImageDesc::new(vsm::POOL, vsm::POOL, DEPTH_FORMAT, depth_usage);
                let pool = fg.history_image(gpu, bindless, "virtual shadow pool", desc)?;
                if fg.is_new(pool) {
                    self.vsm.forget();
                }
                let buffers = self.buffers[slot].fit_vsm(gpu, slot)?;
                let read = buffers.readback.read();
                let asked: Option<&[u32]> =
                    buffers.asked.then(|| bytemuck::cast_slice(&read[..vsm::REQUEST_WORDS * 4]));
                let input = vsm::VsmInput {
                    sun: voxear_core::math::Vec3::from(extracted.frame.sun_direction),
                    camera: voxear_core::math::Vec3::from(extracted.frame.camera_position),
                    frame_index: setup.frame_index,
                    slot,
                    asked,
                    moving,
                    changed: &changed,
                    everything,
                };
                let drawn = self.vsm.update(input);
                self.vsm_counts = [drawn.held as u32, drawn.drawn.len() as u32, drawn.moving.len() as u32];
                buffers.table.write(0, &drawn.table);
                buffers.asked = true;
                // This frame's copies of the pages what moves stands in, with it drawn over them.
                let moving_pool = match drawn.moving.is_empty() {
                    false => {
                        let usage = depth_usage | vk::ImageUsageFlags::TRANSFER_DST;
                        let desc = ImageDesc::new(vsm::MOVING_POOL, vsm::MOVING_POOL, DEPTH_FORMAT, usage);
                        Some(fg.create_image(gpu, bindless, "virtual shadow moving pool", desc)?)
                    }
                    true => None,
                };
                let textures = (fg.texture(pool).0, moving_pool.map_or(NO_TEXTURE, |image| fg.texture(image).0));
                extracted.frame.vsm = vsm::gpu(&drawn, (buffers.gpu_table.address, buffers.requests.address), textures);
                Some((pool, drawn, moving_pool))
            }
            false => {
                self.vsm.forget();
                self.vsm_counts = [0; 3];
                None
            }
        };
        let taa = extracted.post.anti_aliasing.mode == AntiAliasingMode::Taa;
        let taa_reset = self.taa_history != Some(extent);
        self.taa_history = taa.then_some(extent);
        let wireframe = self.options.debug_view == gpu::DEBUG_VIEW_WIREFRAME;
        // The wireframe draws edges where the depth passes would draw faces, so it goes without.
        let occluded = extracted.post.ambient_occlusion.enabled && !wireframe;
        let ao_reset = self.ao_history != Some(extent);
        self.ao_history = occluded.then_some(extent);
        let light_count = extracted.lights.len().min(MAX_LIGHTS as usize);
        drop(shadow_setup);
        drop(extracting);

        // As many meshlets as every instance's draw could turn into, so the meshlet lists never drop any.
        // The two-sided casters' shadows go in lists of their own, as many as they could turn into.
        let (mut needed, mut two_sided_needed, mut two_sided_casters) = (0u64, 0u64, false);
        for instance in &extracted.instances[..opaque_count] {
            let most = u64::from(assets.most_meshlets(instance.mesh));
            needed += most;
            let casts = instance.flags & gpu::INSTANCE_NO_SHADOW == 0;
            if casts && assets.is_two_sided(MaterialHandle(instance.material)) {
                two_sided_needed += most;
                two_sided_casters = true;
            }
        }
        let meshlet_capacity = match self.options.meshlet_culling {
            true if needed <= u64::from(MAX_MESHLET_CAPACITY) => {
                let needed = MeshletCapacity { all: needed as u32, two_sided: two_sided_needed as u32 };
                self.buffers[slot].fit_meshlets(gpu, slot, needed)?;
                Some(self.buffers[slot].meshlet_capacity)
            }
            true => {
                if !std::mem::replace(&mut self.warned_meshlets, true) {
                    tracing::warn!("{needed} meshlets are too many to cull one by one; drawing whole LODs");
                }
                None
            }
            false => None,
        };
        let gathers = profile_marker!("Gathers").scope();
        let buffers = &self.buffers[slot];
        // The terrains' patches this view draws, for its camera and the sun's cascades, and the tracks in them.
        let tracks_id = fg.import_buffer(&self.tracks);
        let no_terrains: &[terrain::Drawn] = &[];
        let terrains = if portal.is_some() { no_terrains } else { &setup.terrains };
        // Each level of the sun's virtual shadow maps with pages drawn this frame, and the planes round them.
        let sun_pages: Vec<(usize, [[f32; 4]; 6])> = virtual_shadows
            .as_ref()
            .map(|(_, drawn, _)| {
                drawn.levels().into_iter().map(|level| (level, extracted.frame.vsm.planes[level])).collect()
            })
            .unwrap_or_default();
        // The sun's cascades are drawn only into its atlas: with virtual shadow maps, its pages pick their own.
        let cascades = virtual_shadows.is_none();
        let view_terrains = terrain::gather(
            terrains,
            &extracted.frame,
            &extracted.entities,
            extracted.has_sun && cascades,
            &buffers.terrain,
            &lamps,
            &sun_pages,
        )
        .map(|drawn| terrain::ViewTerrains { tracks: Some(tracks_id), ..drawn });
        let sun_shadows = portal.is_none_or(|pass| pass.shadows);
        let mut view_blocks = blocks::gather(
            &setup.blocks,
            &extracted,
            &buffers.blocks,
            (offset, Some(through.map_or(camera, |t| t.window)), sun_shadows && cascades, portal.is_none()),
            &lamps,
            &sun_pages,
        );
        // Each list a world draws, one indirect call however many runs it has.
        if let Some(view_blocks) = &mut view_blocks {
            view_blocks.write_draws(|count| self.buffers[slot].fit_block_draws(gpu, slot, count))?;
        }
        let buffers = &self.buffers[slot];
        self.block_counts = view_blocks.as_ref().map(blocks::ViewBlocks::counts).unwrap_or_default();
        // The grass this view scatters round its camera on the terrains.
        let gathered = grass::gather(terrains, &extracted.frame, extracted.origin, &buffers.grass);
        let view_grass = gathered.map(|(jobs, dispatches)| {
            grass::ViewGrass {
                jobs: dispatches,
                draws: jobs.iter().map(|job| job.lod_count).sum(),
                args: buffers.grass_args.handle,
                args_id: fg.import_buffer(&buffers.grass_args),
                blades_id: fg.import_buffer(&buffers.grass_blades),
                grounds: setup.terrains.iter().filter_map(|terrain| terrain.heights_id).collect(),
                tracks: tracks_id,
                index_buffer: assets.indices.handle,
            }
        });
        let color_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let (hdr, velocity, depth) = match portal {
            Some(pass) => (pass.hdr, pass.velocity, pass.depth),
            None => {
                let sized = |format, usage| ImageDesc::new(extent.width, extent.height, format, usage);
                (
                    fg.create_image(gpu, bindless, "hdr", sized(HDR_FORMAT, color_usage))?,
                    fg.create_image(gpu, bindless, "velocity", sized(VELOCITY_FORMAT, color_usage))?,
                    fg.create_image(gpu, bindless, "depth", sized(DEPTH_FORMAT, depth_usage))?,
                )
            }
        };
        let pyramid_desc = pyramid_desc(extent);
        let pyramid = fg.create_image(gpu, bindless, "depth pyramid", pyramid_desc)?;
        let shadows = if extracted.has_sun && sun_shadows && virtual_shadows.is_none() {
            // A quarter the size through a Portal, whose far side shows in part of the picture.
            let side = if portal.is_some() { ATLAS_RESOLUTION / 2 } else { ATLAS_RESOLUTION };
            let desc = ImageDesc::new(side, side, DEPTH_FORMAT, depth_usage);
            // The views through Portals share an atlas of their own: the view they are seen in still reads its
            // own after they are drawn.
            let name = if portal.is_some() { "portal shadow atlas" } else { "shadow atlas" };
            Some(fg.shared_image(gpu, bindless, name, desc)?)
        } else {
            None
        };

        let history = (setup.frame_index % 2) as usize;
        let ambient_occlusion = match occluded {
            true => {
                let full = extracted.post.ambient_occlusion.full_resolution;
                Some(ambient_occlusion::Targets::new(fg, gpu, bindless, extent, history, full)?)
            }
            false => None,
        };

        let mut frame_data = extracted.frame;
        (frame_data.post.display_peak, frame_data.post.display_image) = (setup.fitted.0, u32::from(setup.fitted.1));
        if extracted.canopy_cookie {
            frame_data.lighting.sun_cookie = assets.canopy.0;
        }
        frame_data.post.ao_texture = ambient_occlusion.map_or(NO_TEXTURE, |ao| fg.texture(ao.current).0);
        frame_data.post.ao_reset = u32::from(ao_reset);
        drop(gathers);
        let _passes = profile_marker!("Passes").scope();
        // Contact shadows are the sun's, so they go without one, and the wireframe goes without its depth.
        let contact = extracted.post.contact_shadows.enabled && extracted.has_sun && !wireframe;
        let contact_shadows = match contact {
            true => {
                let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
                let desc = ImageDesc::new(extent.width, extent.height, pipelines::AO_FORMAT, usage);
                Some(fg.create_image(gpu, bindless, "contact shadows", desc)?)
            }
            false => None,
        };
        if let Some(image) = contact_shadows {
            frame_data.post.contact_texture = fg.texture(image).0;
        }
        // Reflections read last frame's finished picture, so they need TAA, and a frame of it to read.
        let reflecting = extracted.post.reflections.enabled && taa && !taa_reset && !wireframe;
        let reflections = match reflecting {
            true => {
                let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
                // At full resolution: see `ambient_occlusion::record_reflections`.
                let desc = ImageDesc::new(extent.width, extent.height, HDR_FORMAT, usage);
                let target = fg.create_image(gpu, bindless, "reflections", desc)?;
                let previous = post_passes::taa_history(fg, gpu, bindless, output_extent, 1 - history)?;
                Some((target, previous))
            }
            false => None,
        };
        if let Some((target, _)) = reflections {
            frame_data.post.reflection_texture = fg.texture(target).0;
        }
        // The depth passes run for these, and write each pixel's normal and roughness beside the depth.
        // A view through a Portal draws its depth first too, so each pixel of its opening is shaded once.
        let depth_first = ambient_occlusion.is_some() || contact_shadows.is_some() || reflections.is_some();
        let surface = match depth_first || portal.is_some() {
            true => {
                let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
                let desc = ImageDesc::new(extent.width, extent.height, pipelines::SURFACE_FORMAT, usage);
                let surface = fg.create_image(gpu, bindless, "surface", desc)?;
                frame_data.post.surface_texture = fg.texture(surface).0;
                Some(surface)
            }
            false => None,
        };
        let lit = extracted.post.global_illumination;
        // Probes whose rays are traced against the scene's triangles, so no light passes walls.
        let mut probes = None;
        // When the light jumps, as a look or lighting settings are swapped, the probes start again: they would keep
        // the old light for a minute and more, in patches.
        let lit_by = extracted.light_key;
        let jumped = self.ddgi_lit_by.replace(lit_by).is_some_and(|before| lit_by.jumped_from(&before));
        let previous_ddgi = self.ddgi_history.take().filter(|_| !jumped);
        let mut scene_hierarchy = (0, 0);
        // Plants of a Foliage come after the Objs; rays leave them out, as thousands of small moving meshes cost far
        // more to trace than they change the bounced light or a reflection. A frame that captures a reflection
        // probe takes those round it as well, so a probe in a garden shows its hedges; the frame's other rays see
        // them too, for that one frame, which the GI's slow blend hides.
        let objs = extracted.entities[..opaque_count].iter().position(|owner| owner.plant.is_some());
        let objs = objs.unwrap_or(opaque_count);
        let capture_point = setup
            .probes
            .filter(|_| self.number == 0 && portal.is_none() && !wireframe)
            .and_then(|probes| probes.job)
            .and_then(|job| match job.kind {
                reflection_probes::JobKind::Capture { point } => Some(voxear_core::math::Vec3::from(point)),
                reflection_probes::JobKind::Load { .. } => None,
            });
        let mut traced_ids: Vec<u32> = (0..objs as u32).collect();
        if let Some(point) = capture_point {
            traced_ids.extend(reflection_probes::plants_near(&extracted.instances[objs..opaque_count], point, objs as u32));
        }
        let traced = (&extracted.instances[..], &traced_ids[..]);
        // Whether reflections trace the scene's triangles, by the GPU's ray queries, rather than the screen.
        let mut traced_reflections = false;
        // How the scene is traced this frame, if it is: in software (true) or by the GPU's rays.
        let mut traced_by = None;
        // A level lit by its bake alone traces no probes round the camera.
        if lit.enabled && lit.source.uses_probes() && !wireframe {
            // Ray queries where the GPU has them; otherwise the scene's hierarchy, walked in a compute shader.
            let scene = if setup.software_rays {
                let hierarchy = &mut self.scene_hierarchies[slot];
                scene_hierarchy = Self::trace_scene(hierarchy, (gpu, assets), traced, slot)?;
                Some(0)
            } else if pipelines.ddgi.is_some() {
                let top_level = (&mut self.top_levels[slot], &mut self.top_level_built[slot], self.number);
                Self::build_scene(top_level, fg, (gpu, bindless, assets), traced, slot)?
            } else {
                None
            };
            if let Some(scene) = scene {
                let made = ddgi::Probes::new(fg, gpu, bindless, setup.software_rays)?;
                let spacing = lit.probe_spacing.clamp(0.1, 16.0);
                let origin = ddgi::origins(voxear_core::math::Vec3::from(frame_data.camera_position), spacing);
                let kept = previous_ddgi.filter(|(_, known, _)| *known == spacing);
                let frames = kept.map_or(0, |(.., frames)| frames.saturating_add(1));
                frame_data.ddgi = gpu::GpuDdgi {
                    origin,
                    previous_origin: kept.map_or(origin, |(before, ..)| before),
                    spacing,
                    irradiance: fg.texture(made.irradiance).0,
                    depth: fg.texture(made.depth).0,
                    probes: fg.texture(made.states).0,
                    enabled: 1,
                    scene,
                    warm_up: ddgi::WARM_UP_FRAMES.saturating_sub(frames),
                    strength: lit.strength.max(0.0),
                    rotation: ddgi::rotation(setup.frame_index),
                };
                self.ddgi_history = Some((origin, spacing, frames));
                traced_reflections = !made.software;
                traced_by = Some(made.software);
                probes = Some(made);
            }
        } else if reflections.is_some() && !setup.software_rays && pipelines.ray_reflections.is_some() {
            // Without GI, reflections still trace the scene where the GPU has ray queries, lit by the sun, the
            // lamps and the sky: the screen holds only what the camera sees, and a chrome ball mirrored the ground
            // behind the cubes beside it where it should have shown their sides.
            let top_level = (&mut self.top_levels[slot], &mut self.top_level_built[slot], self.number);
            if let Some(scene) = Self::build_scene(top_level, fg, (gpu, bindless, assets), traced, slot)? {
                frame_data.ddgi.scene = scene;
                traced_reflections = true;
                traced_by = Some(false);
            }
        }
        if let Some(probes) = &setup.probes {
            frame_data.reflection_probes = probes.gpu;
        }
        // Screen-space reflections walk through a pyramid of the nearest depth.
        let reflection_pyramid = match reflections.is_some() && !traced_reflections {
            true => Some(fg.create_image(gpu, bindless, "reflection depth pyramid", ambient_occlusion::pyramid_desc(extent))?),
            false => None,
        };
        // The first camera's own view captures this frame's reflection probe, through the scene its GI or
        // reflections trace, or one built for it.
        let probe_job = setup.probes.filter(|_| self.number == 0 && portal.is_none() && !wireframe);
        let probe_job = probe_job.and_then(|probes| probes.job.map(|job| (probes, job)));
        let probe_job = match probe_job {
            Some((probes, job)) => {
                if traced_by.is_none() && matches!(job.kind, reflection_probes::JobKind::Capture { .. }) {
                    if setup.software_rays || pipelines.probe_capture.is_none() {
                        let hierarchy = &mut self.scene_hierarchies[slot];
                        scene_hierarchy = Self::trace_scene(hierarchy, (gpu, assets), traced, slot)?;
                        traced_by = Some(true);
                    } else {
                        let top_level = (&mut self.top_levels[slot], &mut self.top_level_built[slot], self.number);
                        if let Some(scene) = Self::build_scene(top_level, fg, (gpu, bindless, assets), traced, slot)? {
                            frame_data.ddgi.scene = scene;
                            traced_by = Some(false);
                        }
                    }
                }
                let capture = match traced_by {
                    Some(false) => pipelines.probe_capture,
                    Some(true) => Some(pipelines.probe_capture_software),
                    // A load traces nothing.
                    None => Some(pipelines.probe_capture_software)
                        .filter(|_| matches!(job.kind, reflection_probes::JobKind::Load { .. })),
                };
                capture.map(|capture| {
                    let filters = reflection_probes::ProbePipelines {
                        capture,
                        downsample: pipelines.probe_downsample,
                        filter: pipelines.probe_filter,
                    };
                    (probes, job, filters)
                })
            }
            None => None,
        };
        let probe_job_done = probe_job.is_some();
        // The first camera's own view traces the frame's sound rays, through the scene its GI or reflections
        // trace, or one built for them.
        let sound_job = setup.sound.filter(|_| self.number == 0 && portal.is_none() && !wireframe);
        let sound_job = match sound_job {
            Some(pass) => {
                if traced_by.is_none() {
                    if setup.software_rays || pipelines.sound_occlusion.is_none() {
                        let hierarchy = &mut self.scene_hierarchies[slot];
                        scene_hierarchy = Self::trace_scene(hierarchy, (gpu, assets), traced, slot)?;
                        traced_by = Some(true);
                    } else {
                        let top_level = (&mut self.top_levels[slot], &mut self.top_level_built[slot], self.number);
                        if let Some(scene) = Self::build_scene(top_level, fg, (gpu, bindless, assets), traced, slot)? {
                            frame_data.ddgi.scene = scene;
                            traced_by = Some(false);
                        }
                    }
                }
                let traced = match (traced_by, pipelines.sound_occlusion, pipelines.sound_reverb) {
                    (Some(false), Some(occlusion), Some(reverb)) => Some(sound_rays::SoundPipelines { occlusion, reverb }),
                    (Some(true), ..) => Some(sound_rays::SoundPipelines {
                        occlusion: pipelines.sound_occlusion_software,
                        reverb: pipelines.sound_reverb_software,
                    }),
                    _ => None,
                };
                traced.map(|traced| (pass, traced))
            }
            None => None,
        };
        let sound_done = sound_job.is_some();
        // The first camera's own view traces this frame's share of a lighting bake, through the scene its GI or
        // reflections trace, or one built for it.
        let bake = setup.bake.as_ref().filter(|_| self.number == 0 && portal.is_none() && !wireframe);
        if bake.is_some() && traced_by.is_none() {
            if setup.software_rays || pipelines.light_bake.is_none() {
                let hierarchy = &mut self.scene_hierarchies[slot];
                scene_hierarchy = Self::trace_scene(hierarchy, (gpu, assets), traced, slot)?;
                traced_by = Some(true);
            } else {
                let top_level = (&mut self.top_levels[slot], &mut self.top_level_built[slot], self.number);
                if let Some(scene) = Self::build_scene(top_level, fg, (gpu, bindless, assets), traced, slot)? {
                    frame_data.ddgi.scene = scene;
                    traced_by = Some(false);
                }
            }
        }
        let bake = bake.zip(match traced_by {
            Some(false) => pipelines.light_bake,
            Some(true) => Some(pipelines.light_bake_software),
            None => None,
        });
        let bake_dispatches = bake.map_or(0, |(bake, _)| bake.dispatches.len());
        let panes = match wireframe || portal.is_some() {
            true => Vec::new(),
            false => glass::panes(&extracted.instances[..instance_count], opaque_count, assets),
        };
        // Glass filters the light of whatever casts shadows through it.
        let sunlit_glass = shadows.is_some() || virtual_shadows.is_some();
        let glass_shadows = match !panes.is_empty() && (sunlit_glass || local_shadows.is_some()) {
            true => Some((glass::Shadows::new(fg, (gpu, bindless))?, panes.clone())),
            false => None,
        };
        if let Some((maps, _)) = &glass_shadows {
            frame_data.glass_shadow = maps.frame_field(fg);
        }
        // The first Ocean's sea, its waves moved on to now, the Pools and Dry Areas, and the worlds of blocks' water.
        let block_water = view_blocks.as_ref().filter(|blocks| blocks.has_water()).cloned();
        let wet = extracted.ocean.is_some()
            || extracted.water_bodies.pools > 0
            || !extracted.water_bodies.streams.is_empty()
            || block_water.is_some();
        // Through a Portal, only the water of worlds of blocks, and only while some of it shows there.
        let shown = portal.is_none_or(|pass| pass.water && block_water.is_some());
        let water = match wet && !wireframe && shown {
            true => Some(water::Water::new(
                fg,
                (gpu, bindless),
                (&mut self.ocean, slot),
                (extracted.ocean, &extracted.water_bodies, block_water, extracted.origin),
                (extent, setup.time, camera),
                (
                    &{
                        // The sea floor is drawn from the Objs, not the plants after them.
                        let objs = extracted.entities[..opaque_count].iter().position(|owner| owner.plant.is_some());
                        water::ground(&extracted.instances[..objs.unwrap_or(opaque_count)], assets)
                    },
                    water::dry_meshes(&extracted.water_bodies, assets),
                ),
                portal.and_then(|pass| pass.waves),
            )?),
            false => None,
        };
        if let Some(water) = &water {
            frame_data.ocean = water.frame;
        }
        // A look pack's colour for the water of worlds of blocks, in place of each block's own.
        frame_data.ocean.block_water = extracted.block_water;
        let glass = match panes.is_empty() {
            true => None,
            false => Some(glass::Glass::new(fg, (gpu, bindless), extent, panes)?),
        };
        let fog = extracted.post.fog;
        let foggy = fog.enabled && fog.volumetric && fog.density > 0.0 && !wireframe;
        let volume = match foggy {
            true => Some(volumetric::Volume::new(fg, gpu, bindless, history)?),
            false => None,
        };
        if let Some(volume) = volume {
            frame_data.post.fog_volume = fg.texture(volume.integrated).0;
            frame_data.post.fog_volume_reset = u32::from(!self.fog_history);
        }
        self.fog_history = foggy;
        frame_data.lighting.ltc_matrix = assets.ltc[0].0;
        frame_data.lighting.ltc_amplitude = assets.ltc[1].0;
        let current = (frame_data.lighting.sky == SKY_ATMOSPHERE).then_some(frame_data.lighting.atmosphere);
        let kept = self.atmosphere_tables.as_ref().map(bytemuck::bytes_of);
        let fresh = kept != current.as_ref().map(bytemuck::bytes_of);
        // A view through a Portal reads the sky tables of the view it is seen in.
        let atmosphere = match (current, portal) {
            (Some(_), Some(pass)) => pass.atmosphere,
            (Some(_), None) => {
                self.atmosphere_tables = current;
                Some(atmosphere::Tables::new(fg, gpu, bindless, fresh)?)
            }
            (None, _) => {
                self.atmosphere_tables = None;
                None
            }
        };
        if let Some(tables) = atmosphere {
            frame_data.lighting.transmittance = fg.texture(tables.transmittance).0;
            frame_data.lighting.multi_scattering = fg.texture(tables.multi_scattering).0;
            frame_data.lighting.sky_view = fg.texture(tables.sky_view).0;
            frame_data.lighting.aerial_perspective = fg.texture(tables.aerial_perspective).0;
        }
        // The clouds drift the way the wind blows, or along x where there is none, and their pictures are this
        // frame's.
        let cloudy = frame_data.lighting.clouds.cover > 0.0 && !wireframe;
        let cloud_images = match cloudy {
            true => {
                // Through a Portal at half the size again: its far side shows in part of the picture.
                let shrink = if portal.is_some() { 4 } else { 2 };
                let mut images = clouds::CloudImages::new(fg, (gpu, bindless), (extent, shrink), history)?;
                // Through a Portal, only the part of the sky its opening shows, and the clouds' panorama is left out.
                images.within = portal.map(|pass| pass.rect);
                if portal.is_some() {
                    images.panorama = None;
                }
                Some(images)
            }
            false => None,
        };
        if let Some(images) = cloud_images {
            let wind = frame_data.wind.direction;
            let blows = if wind[0].hypot(wind[1]) > 1e-3 { wind } else { [1.0, 0.0] };
            let moved = frame_data.lighting.clouds.speed * setup.delta_time;
            let before = self.cloud_drift;
            self.cloud_drift = match portal {
                // The same clouds as in the view it is seen in, as far along.
                Some(pass) => pass.cloud_drift,
                None => [0, 1].map(|axis| self.cloud_drift[axis] + f64::from(blows[axis] * moved)),
            };
            let camera = frame_data.camera_position;
            // From where the pattern is in the level, so it keeps its place as the world's middle moves.
            let middle = [extracted.origin.x, extracted.origin.z];
            let seen = &mut frame_data.lighting.clouds;
            seen.drift = [0, 1].map(|axis| (self.cloud_drift[axis] - middle[axis] as f64) as f32);
            seen.step = [0, 1].map(|axis| (self.cloud_drift[axis] - before[axis]) as f32);
            seen.view = fg.texture(images.current).0;
            seen.distances = fg.texture(images.distances).0;
            seen.shadow_map = fg.texture(images.shadows).0;
            seen.panorama = images.panorama.map_or(NO_TEXTURE, |[_, current]| fg.texture(current).0);
            seen.noise = assets.smoke_noise.0;
            seen.shadow_reach = clouds::SHADOW_REACH;
            seen.shadow_middle = clouds::shadow_middle([camera[0], camera[2]], seen.drift);
            // A picture of another size has nothing of last frame's in it.
            seen.reset = u32::from(self.cloud_history != Some(extent));
        }
        self.cloud_history = cloudy.then_some(extent);
        frame_data.instance_count = opaque_count as u32;
        frame_data.light_count = light_count as u32;
        frame_data.hiz_texture = fg.texture(pyramid).0;
        frame_data.hiz_size = [pyramid_desc.width, pyramid_desc.height];
        frame_data.hiz_mips = pyramid_desc.mip_levels;
        frame_data.shadow_texture = shadows.map_or(NO_TEXTURE, |atlas| fg.texture(atlas).0);
        frame_data.local_shadows.texture = local_shadows.map_or(NO_TEXTURE, |atlas| fg.texture(atlas).0);
        frame_data.local_shadows.cache = kept_shadows.map_or(NO_TEXTURE, |kept| fg.texture(kept).0);
        // A lamp with moving blocks in its reach reads what is drawn over its kept views.
        let moving_blocks = view_blocks.as_ref().map_or(0, |blocks| blocks.moving_for_lamps(&lamps));
        frame_data.local_shadows.cached |= moving_blocks << gpu::KEPT_MOVING_SHIFT;
        frame_data.delta_time = setup.delta_time;
        frame_data.frame_index = setup.frame_index as u32;
        frame_data.taa_reset = u32::from(taa_reset);
        frame_data.meshlet_capacity = meshlet_capacity.map_or(0, |capacity| capacity.all);
        frame_data.meshlet_two_sided_capacity = meshlet_capacity.map_or(0, |capacity| capacity.two_sided);
        // The wireframe view draws back faces too, and the meshlets view colours each meshlet by its own draw.
        let cones = if self.options.debug_view == gpu::DEBUG_VIEW_WIREFRAME { 0 } else { MESHLET_CONES };
        let runs = if self.options.debug_view == gpu::DEBUG_VIEW_MESHLETS { 0 } else { MESHLET_RUNS };
        let culling = if meshlet_capacity.is_some() { MESHLET_CULLING } else { 0 };
        frame_data.meshlet_flags = cones | runs | culling;
        buffers.frame.write(0, &[frame_data]);
        // The first view steps the simulated particles, for every view.
        let particle_step = setup.particle_step.filter(|_| self.number == 0);
        buffers.pointers.write(
            0,
            &[GpuBuffers {
                instances: buffers.instances.address,
                meshes: assets.meshes.address,
                vertices: assets.vertices.address,
                materials: assets.materials.address,
                lights: buffers.lights.address,
                draws: buffers.draws.address,
                draw_count: buffers.draw_count.address,
                draw_instances: buffers.draw_instances.address,
                visibility: self.visibility.address,
                stats: buffers.stats.address,
                clusters: buffers.clusters.address,
                lod_state: self.lod_state.address,
                exposure: self.exposure.address,
                ui_quads: setup.ui_quads,
                debug_lines: buffers.debug_lines.address,
                meshlets: assets.meshlets.address,
                meshlet_vertices: assets.meshlet_vertices.address,
                meshlet_triangles: assets.meshlet_triangles.address,
                meshlet_draws: buffers.meshlet_draws.address,
                meshlet_draw_instances: buffers.meshlet_draw_instances.address,
                meshlet_work: buffers.meshlet_work.address,
                indices: assets.indices.address,
                bvh_nodes: assets.hierarchies.as_ref().map_or(0, |h| h.nodes.address),
                bvh_order: assets.hierarchies.as_ref().map_or(0, |h| h.order.address),
                bvh_table: assets.hierarchies.as_ref().map_or(0, |h| h.table.address),
                scene_nodes: scene_hierarchy.0,
                scene_instances: scene_hierarchy.1,
                foliage_pushes: self.foliage_pushes.address,
                tracks: self.tracks.address,
                // Only under the atmosphere, which works it out.
                sky_light: match (atmosphere, portal) {
                    (Some(_), Some(pass)) => pass.sky_light,
                    (Some(_), None) => buffers.sky_light.address,
                    (None, _) => 0,
                },
                material_properties: assets.material_properties.address,
                ocean_spectrum: self.ocean.spectra[slot].address,
                ocean_fft: self.ocean.fft.address,
                water_bodies: self.ocean.bodies[slot].address,
                stream_points: self.ocean.stream_points[slot].address,
                debug_triangles: buffers.debug_triangles.address,
                particle_systems: buffers.particle_systems.address,
                smokes: buffers.smokes.address,
                smoke_cells: buffers.smoke_cells.address,
                smoke_holes: buffers.smoke_holes.address,
                mists: buffers.mists.address,
                mist_points: buffers.mist_points.address,
                diced_instances: buffers.diced_instances.address,
                dice_counts: buffers.dice_counts.address,
                dice_splits: buffers.dice_splits.address,
                dice_patches: buffers.dice_patches.address,
                dice_draws: buffers.dice_draws.address,
                skins: assets.skins.address,
                skin_matrices: 0,
                ui_panels: buffers.ui_panels.address,
                terrain_lists: terrain::addresses(&buffers.terrain)[0],
                terrains: terrain::addresses(&buffers.terrain)[1],
                terrain_patches: terrain::addresses(&buffers.terrain)[2],
                ui_values: 0,
                grass_jobs: grass::addresses(&buffers.grass)[0],
                grass_args: buffers.grass_args.address,
                grass_blades: buffers.grass_blades.address,
                grass_draws: grass::addresses(&buffers.grass)[1],
                block_worlds: buffers.blocks.address,
                particles: setup.simulated.1,
                particle_chunks: particle_step.map_or(0, |step| step.chunks),
                particle_sim: particle_step.map_or(0, |step| step.frame),
                particle_counters: particle_step.map_or(0, |step| step.counters_address),
                particle_steps: particle_step.map_or(0, |step| step.systems),
                particle_trails: setup.simulated.1,
                particle_hits: particle_step.map_or(0, |step| step.hits_address),
                reflection_probes: setup.probes.map_or(0, |probes| probes.list),
                portal_quads: buffers.portal_quads.address,
                weather_maps: buffers.weather_maps.address,
                lightmap_uvs: assets.lightmap_uvs.address,
                baked_instances,
                bake_job: bake.map_or(0, |(bake, _)| bake.jobs),
            }],
        );
        buffers.instances.write(0, &extracted.instances[..instance_count]);
        buffers.lights.write(0, &extracted.lights[..light_count]);
        let camera = voxear_core::math::Vec3::from(extracted.frame.camera_position);
        // The texture each system draws with: a built-in one of the renderer's, or a picture the engine has loaded.
        let pictures = world.resource::<ParticlePictures>();
        let particle_texture = |texture: ParticleTexture| match texture {
            ParticleTexture::Builtin(builtin) => {
                let slot = BuiltinTexture::PICTURES.iter().position(|picture| *picture == builtin)?;
                Some(assets.particle_textures[slot].0)
            }
            ParticleTexture::Asset(guid) => pictures.and_then(|pictures| pictures.0.get(&guid)).map(|id| id.0),
        };
        let puff_lit = Some(assets.puff_light_maps.map(|map| map.0));
        // The mesh a system's particles are drawn as: a built-in one, or a model's the engine has loaded.
        let models = world.resource::<ParticleMeshes>();
        let particle_mesh = |mesh: ParticleMesh| {
            let (handle, material) = match mesh {
                ParticleMesh::Builtin(builtin) => {
                    let slot = BuiltinMesh::ALL.iter().position(|other| *other == builtin)?;
                    (assets.particle_meshes[slot], assets.default_material)
                }
                ParticleMesh::Asset(guid) => *models?.0.get(&guid)?,
            };
            let (first_index, index_count) = assets.first_lod(handle)?;
            Some(particle_meshes::MeshDraw { mesh: handle.0, material: material.0, first_index, index_count })
        };
        let drawing =
            (&*setup.simulated.0, particles::Looks { find: &particle_texture, puff_lit, mesh: &particle_mesh });
        // Particles, smoke and mist are not drawn through a Portal.
        let (particle_systems, particles, spray, rapids) = match portal {
            Some(_) => Default::default(),
            None => {
                let now = particles::time_of(world, setup.time);
                particles::gather(world, camera, now, &extracted.water_bodies, drawing)
            }
        };
        buffers.particle_systems.write(0, &particle_systems);
        let (smokes, smoke_cells, smoke_holes) = match portal {
            Some(_) => Default::default(),
            None => smoke::gather(world, camera, setup.time),
        };
        buffers.smokes.write(0, &smokes);
        buffers.smoke_cells.write(0, &smoke_cells);
        buffers.smoke_holes.write(0, &smoke_holes);
        let (mists, mist_points) = match portal {
            Some(_) => Default::default(),
            None => mist::gather(world, camera, setup.time),
        };
        buffers.mists.write(0, &mists);
        buffers.mist_points.write(0, &mist_points);
        // The weather's grids, none through a Portal: what lies on the far side is the level's weather alone.
        let none = || vec![0u32; weather_maps::HEADER];
        let maps = match portal {
            Some(_) => none(),
            None => world.resource::<weather_maps::WeatherMaps>().map_or_else(none, |maps| maps.words()),
        };
        buffers.weather_maps.write(0, &maps);

        let push = Push {
            frame: buffers.frame.address,
            buffers: buffers.pointers.address,
            max_draws: MAX_INSTANCES as u32,
            ..Default::default()
        };
        // The Portals in view: their openings, and for those whose far sides are drawn, the views through them.
        let quality = world.resource::<portals::PortalQuality>().copied().unwrap_or_default();
        let level = portal.map_or(0, |pass| pass.level);
        let most = match level {
            _ if wireframe || level >= quality.depth.min(portals::MAX_PORTAL_DEPTH) => 0,
            0 => quality.views as usize,
            // A view through a Portal draws fewer in turn.
            _ => (quality.views as usize).min(2),
        };
        let unjittered = voxear_core::math::Mat4::from_cols_array_2d(&extracted.frame.unjittered_view_proj);
        let jittered = voxear_core::math::Mat4::from_cols_array_2d(&extracted.frame.view_proj);
        let sight = portals::in_sight(world, (camera, offset), unjittered, most, through.map(|through| through.out_of));
        let openings: Vec<portals::GpuPortalQuad> = sight
            .openings
            .iter()
            .map(|(pose, size, colour, mist)| portals::GpuPortalQuad::new(pose, *size, (offset, jittered), *colour, *mist))
            .collect();
        if !openings.is_empty() {
            buffers.portal_quads.write(0, &openings);
        }
        let sees = (!openings.is_empty()).then(|| {
            let view_matrix = voxear_core::math::Mat4::from_cols_array_2d(&extracted.frame.view);
            let (_, rotation, _) = view_matrix.inverse().to_scale_rotation_translation();
            Sees {
                seen: sight.seen.clone(),
                mists: openings.iter().map(|opening| opening.mist[3]).collect(),
                eye: camera,
                offset,
                rotation,
                projection: unjittered * view_matrix.inverse(),
                push,
                depth,
                extent,
                size: (view.extent.width, view.extent.height),
                options: self.options,
                atmosphere,
                sky_light: match (atmosphere, portal) {
                    (Some(_), Some(pass)) => pass.sky_light,
                    (Some(_), None) => buffers.sky_light.address,
                    (None, _) => 0,
                },
                cloud_drift: self.cloud_drift,
                level,
                waves: water.as_ref().map(water::Water::shared_waves),
            }
        });
        // What the passes after the scene read as its depth: with far sides drawn, theirs where they show; and the
        // view's own depth as it was when they were drawn, before its sky, water and glass.
        let (seen_depth, kept_depth) = match (&sees, portal) {
            (Some(sees), None) if !sees.seen.is_empty() => {
                let desc = ImageDesc::new(extent.width, extent.height, pipelines::PYRAMID_FORMAT, color_usage);
                let seen = fg.create_image(gpu, bindless, "seen depth", desc)?;
                (Some(seen), Some(fg.create_image(gpu, bindless, "own depth before water", desc)?))
            }
            _ => (None, None),
        };
        let requests_ids = self.buffers[slot]
            .vsm
            .as_ref()
            .filter(|_| virtual_shadows.is_some())
            .map(|vsm| (fg.import_buffer(&vsm.requests), fg.import_buffer(&vsm.readback), table_copy(fg, vsm)));
        let lamp_pages = lamp_pages.map(|(pool, drawn)| {
            let buffers = self.buffers[slot].lamp_vsm.as_ref().expect("made with the pool");
            passes::LampPages {
                pool,
                drawn: drawn.drawn.iter().map(|(page, slot)| (u32::from(page.slot), *slot)).collect(),
                requests: (fg.import_buffer(&buffers.requests), buffers.requests.handle),
                readback: (fg.import_buffer(&buffers.readback), buffers.readback.handle),
                table: table_copy(fg, buffers),
            }
        });
        let targets = passes::Targets {
            hdr,
            velocity,
            depth,
            depth_texture: fg.texture(depth),
            pyramid,
            pyramid_texture: fg.texture(pyramid),
            pyramid_mips: (0..pyramid_desc.mip_levels as usize).map(|mip| fg.storage(pyramid, mip)).collect(),
            pyramid_size: [pyramid_desc.width, pyramid_desc.height],
            shadows,
            cascade_size: if portal.is_some() { shadows::CASCADE_RESOLUTION / 2 } else { shadows::CASCADE_RESOLUTION },
            virtual_shadows: virtual_shadows.as_ref().map(|(pool, drawn, moving_pool)| {
                let buffers = self.buffers[slot].vsm.as_ref().expect("made with the pool");
                let (requests, readback) = (buffers.requests.handle, buffers.readback.handle);
                let mut levels: Vec<(usize, Vec<(u32, u16)>)> = Vec::new();
                for (page, (key, slot)) in drawn.drawn.iter().enumerate() {
                    match levels.last_mut() {
                        Some((level, pages)) if *level == key.level as usize => pages.push((page as u32, *slot)),
                        _ => levels.push((key.level as usize, vec![(page as u32, *slot)])),
                    }
                }
                let (requests_id, readback_id, table) = requests_ids.expect("imported");
                // After the pages drawn, those what moves stands in, by level: each its place among the frame's pages,
                // its slot, and its slot in the moving pool.
                let mut moving: Vec<(usize, Vec<(u32, u16, u16)>)> = Vec::new();
                for (at, (key, slot, moving_slot)) in drawn.moving.iter().enumerate() {
                    let page = (drawn.drawn.len() + at) as u32;
                    match moving.last_mut() {
                        Some((level, pages)) if *level == key.level as usize => pages.push((page, *slot, *moving_slot)),
                        _ => moving.push((key.level as usize, vec![(page, *slot, *moving_slot)])),
                    }
                }
                passes::VirtualShadows {
                    pool: *pool,
                    levels,
                    moving: moving_pool.map(|image| (image, moving)),
                    requests: (requests_id, requests),
                    readback: (readback_id, readback),
                    table,
                }
            }),
            local_shadows: local_shadows.map(|atlas| passes::LocalShadows {
                atlas,
                kept: kept_shadows,
                views: extracted.local_shadows.clone(),
                lamps: lamps.clone(),
                pages: lamp_pages,
            }),
            glass_shadows,
        };
        let draws = passes::Draws {
            draws: fg.import_buffer(&buffers.draws),
            count: fg.import_buffer(&buffers.draw_count),
            instances: fg.import_buffer(&buffers.draw_instances),
            visibility: fg.import_buffer(&self.visibility),
            lod_state: fg.import_buffer(&self.lod_state),
            foliage_pushes: fg.import_buffer(&self.foliage_pushes),
            tracks: tracks_id,
            tracks_change: extracted.frame.tracks.changes(),
            stats: fg.import_buffer(&buffers.stats),
            stats_readback: fg.import_buffer(&buffers.stats_readback),
            clusters: fg.import_buffer(&buffers.clusters),
            meshlet_draws: fg.import_buffer(&buffers.meshlet_draws),
            meshlet_instances: fg.import_buffer(&buffers.meshlet_draw_instances),
            meshlet_work: fg.import_buffer(&buffers.meshlet_work),
            diced_instances: fg.import_buffer(&buffers.diced_instances),
            dice_counts: fg.import_buffer(&buffers.dice_counts),
            dice_splits: fg.import_buffer(&buffers.dice_splits),
            dice_patches: fg.import_buffer(&buffers.dice_patches),
            dice_draws: fg.import_buffer(&buffers.dice_draws),
            dice_counts_handle: buffers.dice_counts.handle,
            dice_draws_handle: buffers.dice_draws.handle,
            draws_handle: buffers.draws.handle,
            count_handle: buffers.draw_count.handle,
            stats_handle: buffers.stats.handle,
            stats_readback_handle: buffers.stats_readback.handle,
            index_buffer: assets.indices.handle,
            meshlet_draws_handle: buffers.meshlet_draws.handle,
            meshlet_capacity,
            mesh_shaders: setup.mesh_shaders && pipelines.mesh.is_some(),
        };
        let edges = self.options.debug_view == gpu::DEBUG_VIEW_LIT_WIREFRAME;
        let frame_setup = passes::Frame {
            pipelines: *pipelines,
            push,
            instance_count: opaque_count as u32,
            extent,
            wireframe,
            edges,
            ambient_occlusion,
            contact_shadows,
            surface,
            reflections,
            reflection_pyramid,
            atmosphere,
            clouds: cloud_images,
            volume,
            water,
            glass,
            particles,
            particle_pool: setup.simulated.2,
            particle_step,
            spray,
            rapids,
            smokes: smokes.len() as u32,
            smoke_noise: assets.smoke_noise.0,
            mists: mists.len() as u32,
            probes,
            terrain: view_terrains,
            blocks: view_blocks,
            grass: view_grass,
            traced_reflections,
            portal_openings: openings.len() as u32,
            through: portal.is_some(),
            two_sided_casters,
            probe_job,
            sound_job,
            bake: bake.map(|(bake, pipelines)| (bake.clone(), pipelines)),
        };
        // A camera's own view draws the far sides of the Portals it sees once its own surfaces are in, and those
        // seen through them after each; a view through a Portal leaves its own to the view it is seen in.
        let mut children = std::mem::take(&mut self.portals);
        let mut failed = None;
        let mut far_sides = |fg: &mut FrameGraph<'_>| {
            let Some(sees) = sees.as_ref() else { return };
            if let Some(kept) = kept_depth {
                keep_own_depth(fg, (pipelines, sees.push), depth, kept);
            }
            let targets = (hdr, velocity, seen_depth);
            let services = (gpu, &mut *bindless, pipelines, assets);
            let root = fg.prefix().to_string();
            failed = draw_portals(&mut children, fg, services, (world, setup, slot), (sees, &root), targets).err();
        };
        let (under_water, particles, smoke) = passes::record(fg, &frame_setup, &targets, &draws, &mut far_sides);
        // The water and glass drawn since, where they stand in front of what the far sides' depth holds.
        if let (Some(seen), Some(kept)) = (seen_depth, kept_depth) {
            lay_nearer_depth(fg, (pipelines, push), depth, kept, seen);
        }
        // A view not seen for a while lets its buffers go with the view it was seen in.
        self.portals = children;
        if let Some(error) = failed {
            return Err(error);
        }
        if portal.is_some() {
            return Ok(ViewRecorded {
                push,
                backdrop: None,
                panel_sizes: Vec::new(),
                sees,
                probe_job_done,
                sound_done,
                bake_dispatches,
            });
        }
        let post = post_passes::PostInput {
            pipelines: *pipelines,
            push,
            extent: output_extent,
            render_extent: extent,
            settings: extracted.post,
            hdr,
            velocity,
            depth: seen_depth.unwrap_or(depth),
            output,
            output_rect: (view.extent != setup.full).then_some(view),
            clear: setup.clear,
            clear_outside: setup.first,
            exposure: fg.import_buffer(&self.exposure),
            history,
            keep_ldr: setup.keep_ldr,
            fog_volume: volume.map(|volume| volume.integrated),
            under_water,
            particles,
            smoke,
            surface,
            fsr3: pipelines.fsr3.map(|fsr3| post_passes::Fsr3Input {
                pipelines: fsr3,
                state: self.fsr3.clone(),
                retired: self.fsr3_retired.clone(),
                jitter: frame_data.jitter,
                fov_y: extracted.fov_y,
                near: frame_data.near,
                delta_time: setup.delta_time,
                reset: taa_reset,
                slot,
            }),
        };
        let finished = post_passes::record(fg, gpu, bindless, &post)?;
        let backdrop = match finished.filter(|_| post.keep_ldr) {
            Some(view_image) => {
                let (blurred, size) =
                    post_passes::blur_for_backdrop(fg, gpu, bindless, (*pipelines, push), view_image, output_extent)?;
                Some((blurred, size, view))
            }
            None => None,
        };
        // UI panels in the world, over the finished picture so their colours are as designed, and under debug draw.
        let panel_view = ui_panels::PanelView {
            textures: &setup.panels,
            camera,
            view_projection: panel_projection,
            view,
            render: extent,
        };
        let panel_pipelines = (pipelines.ui_panels, pipelines.ui_panel_text, push);
        let panel_sizes = ui_panels::draw(fg, panel_pipelines, &buffers.ui_panels, world, &panel_view, (output, depth));
        let gizmos = self.options.gizmos;
        let debug = (pipelines.debug, pipelines.debug_solid, push);
        debug_pass::record(fg, debug, (&buffers.debug_lines, &buffers.debug_triangles), (output, view), depth, gizmos);
        Ok(ViewRecorded { push, backdrop, panel_sizes, sees: None, probe_job_done, sound_done, bake_dispatches })
    }
}

/// Draws the far sides of the Portals a view `sees`, each by a view of its
/// own kept in `children`, over the colour and motion of the camera's view,
/// and lays each one's depth over `seen_depth`; then the far sides of the
/// Portals each of those sees in turn; and last the mist of the view's own
/// openings.
///
/// Its images are named after the camera's own view, whose prefix is
/// `root`: two cameras' views, as the editor's Game and Scene views, are of
/// different sizes, and an image of one name made at another size is made
/// again in its place.
fn draw_portals(
    children: &mut Vec<PortalChild>,
    fg: &mut FrameGraph<'_>,
    (gpu, bindless, pipelines, assets): (&Gpu, &mut Bindless, &Pipelines, &Assets),
    (world, setup, slot): (&World, &ViewSetup, usize),
    (sees, root): (&Sees, &str),
    (hdr, velocity, seen_depth): (ImageId, ImageId, Option<ImageId>),
) -> Result<()> {
    let quality = world.resource::<portals::PortalQuality>().copied().unwrap_or_default();
    let pipelines = *pipelines;
    let outer = fg.prefix().to_string();
    let extent = sees.extent;
    let level = sees.level + 1;
    let (parent_depth, parent_push) = (sees.depth, sees.push);
    let parent_texture = fg.texture(parent_depth).0;
    if let Some(seen_depth) = seen_depth.filter(|_| sees.level == 0) {
        // The camera's own depth first, for the far sides to be laid over.
        let push = Push { textures: [parent_texture, 0, 0, 0], ..parent_push };
        let images = [(parent_depth, ImageAccess::Sampled), (seen_depth, ImageAccess::ColorAttachment)];
        fg.add_pass("Portal depth", &images, &[], move |ctx| {
            portal_fullscreen(ctx, pipelines.portal_depth, &push, seen_depth);
        });
    }
    for (index, seen) in sees.seen.iter().enumerate() {
        // Kept by the camera's own view whenever it draws any far side.
        let Some(seen_depth) = seen_depth else { break };
        let at = match children.iter().position(|child| child.portal == seen.entity) {
            Some(at) => at,
            None => {
                // The view of a Portal no longer seen, or a new one.
                let idle = children.iter().position(|child| child.used + PORTAL_VIEW_KEPT < setup.frame_index);
                match idle {
                    Some(at) => {
                        children[at].view.reset_history();
                        children[at].portal = seen.entity;
                        at
                    }
                    None => {
                        // The first number no view at this level has, as one let go leaves a gap.
                        let base = PORTAL_VIEW_NUMBERS * level as usize;
                        let taken = |number: usize| children.iter().any(|child| child.view.number == number);
                        let number = (base..).find(|&number| !taken(number)).expect("numbers without end");
                        let view = Box::new(View::new(gpu, number)?);
                        children.push(PortalChild { portal: seen.entity, view, used: 0 });
                        children.len() - 1
                    }
                }
            }
        };
        let child = &mut children[at];
        child.used = setup.frame_index;
        // A depth for each level, which its views use in turn: the furthest where the opening shows, the nearest
        // everywhere else.
        fg.set_prefix(&format!("{root}portal level {level} "));
        let usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let depth = fg.create_image(gpu, bindless, "depth", ImageDesc::new(extent.width, extent.height, DEPTH_FORMAT, usage))?;
        let mask = Push { texture: parent_texture, level: index as u32, ..parent_push };
        let images = [(parent_depth, ImageAccess::Sampled), (depth, ImageAccess::DepthAttachment)];
        fg.add_pass("Portal mask", &images, &[], move |ctx| {
            let layout = vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
            let target = passes::attachment(ctx.view(depth), layout, Some(passes::clear_depth(1.0)));
            passes::begin_rendering(ctx, ctx.extent(depth), &[], Some(&target));
            passes::bind(ctx, pipelines.portal_mask, vk::PipelineBindPoint::GRAPHICS, &mask);
            unsafe {
                ctx.gpu.device.cmd_draw(ctx.cmd, 6, 1, 0, 0);
                ctx.gpu.device.cmd_end_rendering(ctx.cmd);
            }
        });
        // From the near Portal's place, in the seeing view's own small numbers.
        let from_near = sees.eye - (seen.near.position - sees.offset);
        let pass = PortalPass {
            through: portals::Through::new(
                seen,
                from_near,
                sees.rotation,
                sees.projection,
                (quality.draw_distance > 0.0).then_some(quality.draw_distance),
            ),
            hdr,
            velocity,
            depth,
            size: sees.size,
            options: sees.options,
            atmosphere: sees.atmosphere,
            sky_light: sees.sky_light,
            cloud_drift: sees.cloud_drift,
            shadows: seen.distance < quality.shadow_distance,
            water: quality.water,
            rect: seen.rect,
            level,
            waves: sees.waves,
        };
        // After the view it is seen in, as the views through each of that one's Portals are numbered alike.
        fg.set_prefix(&format!("{outer}portal view {} ", child.view.number));
        let recorded = child.view.record(fg, (gpu, &mut *bindless, &pipelines, assets), world, setup, hdr, slot, Some(&pass))?;
        let over = Push { textures: [fg.texture(depth).0, 0, 0, 0], ..parent_push };
        let images = [(depth, ImageAccess::Sampled), (seen_depth, ImageAccess::ColorAttachment)];
        fg.add_pass("Portal depth over", &images, &[], move |ctx| {
            portal_fullscreen(ctx, pipelines.portal_depth_over, &over, seen_depth);
        });
        // The Portals seen through it, while its depth still stands.
        if let Some(nested) = recorded.sees {
            let mut deeper = std::mem::take(&mut child.view.portals);
            let services = (gpu, &mut *bindless, &pipelines, assets);
            let targets = (hdr, velocity, Some(seen_depth));
            let drawn = draw_portals(&mut deeper, fg, services, (world, setup, slot), (&nested, root), targets);
            children[at].view.portals = deeper;
            drawn?;
        }
    }
    fg.set_prefix(&outer);
    // The mist of each opening that shows any, over what shows in it.
    let misted: Vec<u32> = (0..sees.mists.len() as u32).filter(|&opening| sees.mists[opening as usize] > 0.0).collect();
    if !misted.is_empty() {
        let mist = Push { texture: parent_texture, ..parent_push };
        let images = [(parent_depth, ImageAccess::Sampled), (hdr, ImageAccess::ColorAttachment)];
        fg.add_pass("Portal mist", &images, &[], move |ctx| {
            let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
            let color = [passes::attachment(ctx.view(hdr), layout, None)];
            passes::begin_rendering(ctx, ctx.extent(hdr), &color, None);
            passes::bind(ctx, pipelines.portal_mist, vk::PipelineBindPoint::GRAPHICS, &mist);
            for opening in misted {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { level: opening, ..mist });
                unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 6, 1, 0, 0) };
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    Ok(())
}

/// How many frames a view through a Portal is kept for a Portal no longer
/// seen, before another Portal takes it.
const PORTAL_VIEW_KEPT: u64 = 120;

/// How many frames a view through a Portal is kept for a Portal no longer
/// seen at all, before it lets its buffers and pictures go: a few seconds,
/// so a camera that looks away and back draws on at once, and one that has
/// walked off holds nothing for it.
const PORTAL_VIEW_FREED: u64 = 600;

/// Lets go of each view through a Portal in `children` not drawn for
/// [`PORTAL_VIEW_FREED`] frames, with its pictures, named after `outer`,
/// and the views through Portals it saw in turn. Once a camera's own view,
/// whose pictures are named after `root`, keeps none, the depths its views
/// through Portals shared go too.
fn free_portal_views(
    children: &mut Vec<PortalChild>,
    (gpu, graph, bindless, frames): (&Gpu, &mut RenderGraph, &mut Bindless, &mut Frames),
    (root, outer): (&str, &str),
    frame: u64,
) {
    if children.is_empty() {
        return;
    }
    let mut kept = Vec::with_capacity(children.len());
    for mut child in children.drain(..) {
        let named = format!("{outer}portal view {} ", child.view.number);
        if child.used + PORTAL_VIEW_FREED < frame {
            // Not drawn for hundreds of frames, so no frame in flight uses its pictures.
            graph.release_named(gpu, bindless, |name| name.starts_with(&named));
            child.view.retire(frames);
        } else {
            free_portal_views(&mut child.view.portals, (gpu, graph, bindless, frames), (root, &named), frame);
            kept.push(child);
        }
    }
    if kept.is_empty() && root == outer {
        let levels = format!("{root}portal level ");
        graph.release_named(gpu, bindless, |name| name.starts_with(&levels));
    }
    *children = kept;
}

/// Where the numbers of each level's views through Portals start, as
/// `View::number`: past any camera's own.
const PORTAL_VIEW_NUMBERS: usize = 64;

/// A triangle over all of `target`, drawn with `pipeline`.
/// The copy of a page table the CPU wrote into the one shading reads.
fn table_copy(fg: &mut FrameGraph<'_>, buffers: &VsmBuffers) -> passes::TableCopy {
    passes::TableCopy {
        from: (fg.import_buffer(&buffers.table), buffers.table.handle),
        to: (fg.import_buffer(&buffers.gpu_table), buffers.gpu_table.handle),
        size: buffers.table.size,
    }
}

/// Copies the view's own depth into `kept`, as it is once its surfaces are
/// drawn and before its sky, water and glass, for [`lay_nearer_depth`].
fn keep_own_depth(fg: &mut FrameGraph<'_>, (pipelines, push): (&Pipelines, Push), depth: ImageId, kept: ImageId) {
    let pipeline = pipelines.portal_depth;
    let push = Push { textures: [fg.texture(depth).0, 0, 0, 0], ..push };
    let images = [(depth, ImageAccess::Sampled), (kept, ImageAccess::ColorAttachment)];
    fg.add_pass("Own depth before water", &images, &[], move |ctx| portal_fullscreen(ctx, pipeline, &push, kept));
}

/// Lays over `seen`, the depth the passes after the scene read, the view's
/// `depth` wherever it is nearer now than it was in `kept`: the water and
/// glass drawn after the far sides. Without it the fog, and all else that
/// reads that depth, went by what lies under the water, as far off as the
/// sea floor, whenever a Portal was in view.
fn lay_nearer_depth(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (&Pipelines, Push),
    depth: ImageId,
    kept: ImageId,
    seen: ImageId,
) {
    let pipeline = pipelines.portal_depth_nearer;
    let push = Push { textures: [fg.texture(depth).0, fg.texture(kept).0, 0, 0], ..push };
    let images = [(depth, ImageAccess::Sampled), (kept, ImageAccess::Sampled), (seen, ImageAccess::ColorAttachment)];
    fg.add_pass("Depth before the water's", &images, &[], move |ctx| portal_fullscreen(ctx, pipeline, &push, seen));
}

fn portal_fullscreen(ctx: &graph::PassContext, pipeline: vk::Pipeline, push: &Push, target: ImageId) {
    let color = [passes::attachment(ctx.view(target), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
    passes::begin_rendering(ctx, ctx.extent(target), &color, None);
    passes::bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    unsafe {
        ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
        ctx.gpu.device.cmd_end_rendering(ctx.cmd);
    }
}

/// The depth pyramid starts at the largest power of two that fits inside the
/// depth buffer, so each level halves exactly.
fn pyramid_desc(extent: vk::Extent2D) -> ImageDesc {
    let floor_pow2 = |v: u32| 1 << (31 - v.max(1).leading_zeros());
    let (width, height) = (floor_pow2(extent.width), floor_pow2(extent.height));
    ImageDesc {
        width,
        height,
        format: PYRAMID_FORMAT,
        usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
        mip_levels: full_mip_count(width, height),
        cubes: 0,
    }
}

/// The UI's buffers for one frame in flight: its quads, and pointers naming
/// them, which never change.
fn create_ui_buffers(gpu: &Gpu, slot: usize) -> Result<UiBuffers> {
    let (storage, host) = (vk::BufferUsageFlags::STORAGE_BUFFER, MemoryLocation::CpuToGpu);
    let size = ui_pass::MAX_UI_QUADS * size_of::<voxear_ui::Quad>() as u64;
    let quads = gpu.create_buffer(&format!("ui quads {slot}"), size, storage, host)?;
    let size = ui_pass::MAX_UI_VALUES * size_of::<[f32; 4]>() as u64;
    let values = gpu.create_buffer(&format!("ui values {slot}"), size, storage, host)?;
    let pointers = gpu.create_buffer(&format!("ui pointers {slot}"), size_of::<GpuBuffers>() as u64, storage, host)?;
    let table = GpuBuffers { ui_quads: quads.address, ui_values: values.address, ..bytemuck::Zeroable::zeroed() };
    pointers.write(0, &[table]);
    Ok(UiBuffers { quads, values, pointers })
}

/// A view's buffers for one frame in flight, their names after `prefix`.
fn create_frame_buffers(gpu: &Gpu, prefix: &str, slot: usize) -> Result<FrameBuffers> {
    let host = MemoryLocation::CpuToGpu;
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let indirect = storage | vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
    let lists = u64::from(LIST_COUNT);
    let [meshlet_draws, meshlet_draw_instances, meshlet_work] =
        create_meshlet_lists(gpu, prefix, slot, MeshletCapacity { all: FIRST_MESHLET_CAPACITY, two_sided: 0 })?;
    Ok(FrameBuffers {
        frame: gpu.create_buffer(&format!("{prefix}frame {slot}"), size_of::<GpuFrame>() as u64, storage, host)?,
        instances: gpu.create_buffer(
            &format!("{prefix}instances {slot}"),
            MAX_INSTANCES * size_of::<GpuInstance>() as u64,
            storage,
            host,
        )?,
        lights: gpu.create_buffer(
            &format!("{prefix}lights {slot}"),
            MAX_LIGHTS * size_of::<GpuLight>() as u64,
            storage,
            host,
        )?,
        draws: gpu.create_buffer(
            &format!("{prefix}draws {slot}"),
            lists * MAX_INSTANCES * DRAW_COMMAND_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        draw_count: gpu.create_buffer(
            &format!("{prefix}draw counts {slot}"),
            DRAW_COUNT_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        draw_instances: gpu.create_buffer(
            &format!("{prefix}draw instances {slot}"),
            lists * MAX_INSTANCES * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        stats: gpu.create_buffer(
            &format!("{prefix}stats {slot}"),
            stat::COUNT as u64 * 4,
            storage | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::GpuOnly,
        )?,
        stats_readback: gpu.create_buffer(
            &format!("{prefix}stats readback {slot}"),
            stat::COUNT as u64 * 4,
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )?,
        clusters: gpu.create_buffer(
            &format!("{prefix}light clusters {slot}"),
            u64::from(CLUSTER_COUNT) * u64::from(MAX_CLUSTER_LIGHTS + 1) * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        pointers: gpu.create_buffer(
            &format!("{prefix}pointers {slot}"),
            size_of::<GpuBuffers>() as u64,
            storage,
            host,
        )?,
        debug_lines: gpu.create_buffer(
            &format!("{prefix}debug lines {slot}"),
            debug_pass::MAX_DEBUG_LINES * size_of::<gpu::GpuDebugLine>() as u64,
            storage,
            host,
        )?,
        debug_triangles: gpu.create_buffer(
            &format!("{prefix}debug triangles {slot}"),
            debug_pass::MAX_DEBUG_TRIANGLES * size_of::<gpu::GpuDebugTriangle>() as u64,
            storage,
            host,
        )?,
        particle_systems: gpu.create_buffer(
            &format!("{prefix}particle systems {slot}"),
            particles::MAX_PARTICLE_SYSTEMS as u64 * size_of::<gpu::GpuParticleSystem>() as u64,
            storage,
            host,
        )?,
        smokes: gpu.create_buffer(
            &format!("{prefix}smokes {slot}"),
            smoke::MAX_SMOKES as u64 * size_of::<gpu::GpuSmoke>() as u64,
            storage,
            host,
        )?,
        smoke_cells: gpu.create_buffer(
            &format!("{prefix}smoke cells {slot}"),
            (smoke::MAX_SMOKES * smoke::SMOKE_STRIDE * 4) as u64,
            storage,
            host,
        )?,
        smoke_holes: gpu.create_buffer(
            &format!("{prefix}smoke holes {slot}"),
            (smoke::MAX_SMOKES * (smoke::MAX_SHOTS + smoke::MAX_BLASTS) * 2 * 16) as u64,
            storage,
            host,
        )?,
        mists: gpu.create_buffer(
            &format!("{prefix}mists {slot}"),
            mist::MAX_MISTS as u64 * size_of::<gpu::GpuMist>() as u64,
            storage,
            host,
        )?,
        mist_points: gpu.create_buffer(
            &format!("{prefix}mist points {slot}"),
            (mist::MAX_MISTS * mist::MAX_MIST_POINTS * 16) as u64,
            storage,
            host,
        )?,
        ui_panels: gpu.create_buffer(
            &format!("{prefix}ui panels {slot}"),
            (ui_panels::MAX_UI_PANELS * size_of::<gpu::GpuUiPanel>()) as u64,
            storage,
            host,
        )?,
        terrain: gpu.create_buffer(&format!("{prefix}terrains {slot}"), terrain::FRAME_SIZE, storage, host)?,
        blocks: gpu.create_buffer(&format!("{prefix}blocks {slot}"), blocks::FRAME_SIZE, storage, host)?,
        portal_quads: gpu.create_buffer(
            &format!("{prefix}portal openings {slot}"),
            (portals::MAX_PORTAL_QUADS * size_of::<portals::GpuPortalQuad>()) as u64,
            storage,
            host,
        )?,
        weather_maps: gpu.create_buffer(
            &format!("{prefix}weather maps {slot}"),
            (weather_maps::WORDS * 4) as u64,
            storage,
            host,
        )?,
        grass: gpu.create_buffer(&format!("{prefix}grass {slot}"), grass::FRAME_SIZE, storage, host)?,
        grass_args: gpu.create_buffer(
            &format!("{prefix}grass draws {slot}"),
            grass::ARGS_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        grass_blades: gpu.create_buffer(
            &format!("{prefix}grass blades {slot}"),
            grass::MAX_GRASS_BLADES * 16,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        diced_instances: gpu.create_buffer(
            &format!("{prefix}diced instances {slot}"),
            dice::MAX_DICED_INSTANCES * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        dice_counts: gpu.create_buffer(
            &format!("{prefix}dice counts {slot}"),
            dice::COUNTS_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        dice_splits: gpu.create_buffer(
            &format!("{prefix}dice splits {slot}"),
            dice::MAX_SPLITS * 16,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        dice_patches: gpu.create_buffer(
            &format!("{prefix}dice pieces {slot}"),
            dice::MAX_PATCHES * dice::PATCH_SIZE,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        dice_draws: gpu.create_buffer(
            &format!("{prefix}dice draws {slot}"),
            dice::MAX_PATCHES * dice::PATCH_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        sky_light: gpu.create_buffer(&format!("{prefix}sky light {slot}"), 32, storage, MemoryLocation::GpuOnly)?,
        meshlet_draws,
        meshlet_draw_instances,
        meshlet_work,
        meshlet_capacity: MeshletCapacity { all: FIRST_MESHLET_CAPACITY, two_sided: 0 },
        vsm: None,
        lamp_vsm: None,
        block_draws: None,
        prefix: prefix.to_string(),
    })
}

/// A view's meshlet lists for one frame in flight, for `capacity` (see
/// `gpu::meshlet_list`): their draws, each draw's instance, and the
/// meshlets queued for the meshlet pass.
fn create_meshlet_lists(gpu: &Gpu, prefix: &str, slot: usize, capacity: MeshletCapacity) -> Result<[Buffer; 3]> {
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let entries = meshlet_lists_len(capacity).max(1);
    let draws = gpu.create_buffer(
        &format!("{prefix}meshlet draws {slot}"),
        entries * DRAW_COMMAND_SIZE,
        storage | vk::BufferUsageFlags::INDIRECT_BUFFER,
        MemoryLocation::GpuOnly,
    )?;
    let instances = gpu.create_buffer(
        &format!("{prefix}meshlet draw instances {slot}"),
        entries * 4,
        storage,
        MemoryLocation::GpuOnly,
    )?;
    let work =
        gpu.create_buffer(&format!("{prefix}meshlet work {slot}"), entries * 8, storage, MemoryLocation::GpuOnly)?;
    Ok([draws, instances, work])
}

/// Where a project's custom shaders stand.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CustomShaderState {
    /// None asked for.
    #[default]
    None,
    Compiling,
    /// Drawn with.
    Ready,
    /// Slang's error, naming the shader and the line; the shaders drawn with
    /// before stay.
    Failed(String),
}

/// A project's custom shaders: the code last asked for, what compiles it,
/// code asked for while it did, and where they stand.
#[derive(Default)]
struct CustomShaders {
    code: CustomCode,
    compiling: Option<CustomCompile>,
    waiting: Option<CustomCode>,
    state: CustomShaderState,
    /// Which of the process's renderers this is, for a folder of its own.
    number: std::sync::OnceLock<u32>,
}

impl CustomShaders {
    /// Where the project's `project_shaders.slang` and its modules are
    /// written, for this run: a folder of this renderer's own, so two
    /// renderers of one process compiling at once do not write over each
    /// other's.
    fn dir(&self) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let number = self.number.get_or_init(|| NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        std::env::temp_dir().join(format!("voxear-custom-shaders-{}-{number}", std::process::id()))
    }
}
