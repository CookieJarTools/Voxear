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
mod bvh;
mod capture;
mod cooked;
mod debug_pass;
mod fbx_import;
mod foliage;
mod frame_debugger;
mod glass;
mod gltf_import;
mod gpu;
mod ddgi;
mod graph;
mod hot_reload;
mod ies;
mod lod;
mod looks;
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
mod passes;
mod payload;
mod pipelines;
mod post;
mod post_passes;
mod post_volume;
mod previews;
mod scene;
mod seasons;
mod shadows;
mod texture_codec;
mod ui_pass;
mod volumetric;
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
pub use capture::{Capture, CapturedImage};
pub use cooked::{COOKED_VERSION, CookedModel};
pub use frame_debugger::{Channels, FrameImage, FrameLook, FramePass};
pub use gltf_import::{
    FilePicture, FileMaterial, ImportedModel, ImportedNode, PreparedModel, ReadFile, companion_files,
    prepare as prepare_model,
};
pub use gpu::{
    DEBUG_VIEW_AMBIENT_OCCLUSION, DEBUG_VIEW_CASCADES, DEBUG_VIEW_GLOBAL_ILLUMINATION, DEBUG_VIEW_LIGHT_COMPLEXITY,
    DEBUG_VIEW_LIT_WIREFRAME, DEBUG_VIEW_LOD, DEBUG_VIEW_MESHLETS, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS, DEBUG_VIEW_UNLIT,
    DEBUG_VIEW_WIREFRAME,
};
pub use graph::{BufferAccess, FrameGraph, ImageAccess, ImageId, PassContext, RenderGraph};
pub use lod::{LodInfo, MAX_LODS, lod_at, lod_at_share, screen_share};
pub use lod_group::{LodCamera, LodChoices, LodGroup, LodLevel, LodMeasure, LodView, default_until};
pub use model_lods::lod_of;
pub use mesh::{MeshData, MeshTriangles, NO_WIND_MASKS, Primitive, Vertex};
pub use post::{
    AmbientOcclusion, AmbientOcclusionQuality, AntiAliasing, AntiAliasingMode, Bloom, ContactShadows, DepthOfField, Exposure, Fog,
    GlobalIllumination, Reflections,
    Grading, MotionBlur, PostProcess, Tonemap, Tonemapper, Vignette,
};
pub use previews::{
    MAX_PREVIEW_SIZE, PREVIEW_ATLAS_SIZE, PreviewId, PreviewImage, PreviewOptions, PreviewPart, PreviewPixels,
};
pub use ies::IesProfile;
pub use looks::{Look, LookName, SunLook};
pub use shadows::{SHADOW_DISTANCE, SHADOW_SPLIT};
pub use post_volume::{PostVolume, VolumeShape};
pub use foliage::{Foliage, FoliageInteraction, Impostors, Plant, scatter};
pub use assets::Glow;
pub use impostors::Impostor;
pub use model_renderer::{ModelPart, ModelRenderer};
pub use seasons::{Leaves, Season, SeasonLook, Seasons};
pub use wind::WindZone;
pub use scene::{
    Camera, Light, LightKind, MAX_TRANSITIONS, MeshRenderer, Sky, SkyBody, SkyMode, ViewOptions, temperature_color,
};
pub use voxear_rhi::TextureId;

use crate::{
    assets::Assets,
    gpu::{
        CLUSTER_COUNT, DRAW_COMMAND_SIZE, DRAW_COUNT_SIZE, GpuBuffers, GpuFrame, GpuInstance, GpuLight, LIST_COUNT,
        MAX_CLUSTER_LIGHTS, MESHLET_CONES, MESHLET_CULLING, MESHLET_RUNS, Mat4Cols, NO_TEXTURE, Push,
        meshlet_lists_len, stat,
    SKY_ATMOSPHERE, },
    hot_reload::{ShaderSet, ShaderWatcher},
    pipelines::{DEPTH_FORMAT, HDR_FORMAT, PYRAMID_FORMAT, Pipelines, VELOCITY_FORMAT},
    shadows::ATLAS_RESOLUTION,
};

const MAX_INSTANCES: u64 = 64 * 1024;
const MAX_LIGHTS: u64 = 1024;
/// The meshlet lists start with room for this many meshlets of each
/// instance list's draws, and grow with the scene (see `gpu::meshlet_list`).
const FIRST_MESHLET_CAPACITY: u32 = 4096;
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
    /// Auto exposure's adapted EV, and the scene's metered average
    /// luminance. Both stay zero while auto exposure is off.
    pub exposure_ev: f32,
    pub average_luminance: f32,
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
            exposure_ev: exposure[0],
            average_luminance: exposure[2],
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
    // Destroyed explicitly, which needs the GPU.
    Offscreen { image: ManuallyDrop<Image>, last: ImageAccess },
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
    /// The sun's light and the sky's, worked out once a frame under the
    /// atmosphere (see `atmosphere::record`).
    sky_light: Buffer,
    /// The meshlet lists, made for `meshlet_capacity` (see `gpu::meshlet_list`).
    meshlet_draws: Buffer,
    meshlet_draw_instances: Buffer,
    /// The meshlets the cull queues for the meshlet pass, laid out as the lists.
    meshlet_work: Buffer,
    meshlet_capacity: u32,
    /// What its buffers' names start with.
    prefix: String,
}

impl FrameBuffers {
    /// Makes sure the meshlet lists have room for `needed`, making them
    /// again half as large again when they do not, so a growing scene does
    /// not make them every frame. The caller has waited for this slot's last
    /// frame, so the old ones can go at once.
    fn fit_meshlets(&mut self, gpu: &Gpu, slot: usize, needed: u32) -> Result<()> {
        if needed <= self.meshlet_capacity {
            return Ok(());
        }
        let capacity = needed.max(self.meshlet_capacity + self.meshlet_capacity / 2).min(MAX_MESHLET_CAPACITY);
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
    output_format: vk::Format,
    watcher: Option<ShaderWatcher>,
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
    previews: previews::Previews,
    /// Plants' impostors waiting to be baked, and what bakes drew from.
    impostor_bakes: impostors::Bakes,
    gpu_decompression: bool,
    /// Meshlet culling draws with task and mesh shaders, where the GPU has them.
    mesh_shaders: bool,
    /// What the output is cleared to outside the view, linear.
    clear: [f32; 4],
    frame_debugger: frame_debugger::FrameDebugger,
    captures: capture::Captures,
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
        let output = Target::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined };
        Self::build(gpu, output, renderdoc, settings)
    }

    fn build(gpu: Gpu, target: Target, renderdoc: Option<RenderDoc>, settings: &RendererSettings) -> Result<Self> {
        let hot_reload = settings.hot_reload;
        let frames = Frames::new(&gpu)?;
        let mut bindless = Bindless::new(&gpu)?;
        let graph = RenderGraph::new(&gpu, FRAMES_IN_FLIGHT)?;
        // Without ray tracing, the GI's rays walk hierarchies of the meshes' triangles in a compute shader.
        let software_rays = settings.software_rays || gpu.acceleration_structure.is_none();
        let assets = Assets::new(&gpu, &mut bindless, software_rays)?;
        let output_format = match &target {
            Target::Window(swapchain) => swapchain.format.format,
            Target::Offscreen { image, .. } => image.desc.format,
        };
        let pipelines = Pipelines::new(&gpu, &bindless, output_format, &ShaderSet::embedded())?;
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
            watcher: if hot_reload { ShaderWatcher::new() } else { None },
            views: vec![first],
            frame_index: 0,
            last_time: None,
            waited_ms: 0.0,
            stats: RenderStats::default(),
            ui_buffers,
            previews: previews::Previews::default(),
            impostor_bakes: impostors::Bakes::default(),
            gpu_decompression: true,
            mesh_shaders: true,
            clear: [0.0, 0.0, 0.0, 1.0],
            frame_debugger: Default::default(),
            captures: Default::default(),
        })
    }

    pub fn gpu(&self) -> &GpuInfo {
        &self.gpu.info
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
    pub fn stats(&self) -> RenderStats {
        self.stats
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
        let swapchain = match Swapchain::new_matching(&self.gpu, surface, (width, height), false, self.output_format) {
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
        let target = Target::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined };
        Ok(self.push_output(target, Vec::new()))
    }

    fn create_output_image(&self, width: u32, height: u32) -> Result<Image> {
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        self.gpu.create_image("output", ImageDesc::new(width.max(1), height.max(1), self.output_format, usage))
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
        if let Target::Offscreen { image: old, last } = &mut self.outputs[index].target {
            self.gpu.destroy_image(unsafe { ManuallyDrop::take(old) });
            (*old, *last) = (ManuallyDrop::new(image), ImageAccess::Undefined);
        }
        Ok(())
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

    /// Loads a `.gltf`, `.glb` or `.fbx` file and returns its drawable pieces.
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
            Pipelines::new(&self.gpu, &self.bindless, self.output_format, &shaders)
        });
        match reloaded {
            Ok(pipelines) => {
                self.pipelines.destroy(&self.gpu);
                self.pipelines = pipelines;
                tracing::info!("shaders reloaded in {:.0} ms", started.elapsed().as_secs_f32() * 1000.0);
            }
            Err(error) => tracing::error!("shader reload failed, keeping the previous shaders: {error:#}"),
        }
    }

    /// Draws the world's cameras, lights and mesh renderers, into each output.
    pub fn render(&mut self, world: &World, time: f32) -> Result<()> {
        self.reload_shaders();
        for output in &mut self.outputs {
            if let (Some((width, height)), Target::Window(swapchain)) = (output.pending_size.take(), &mut output.target)
            {
                swapchain.recreate(&self.gpu, width, height)?;
            }
        }
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
        // Impostors asked for last frame are made, to bake before this frame's views draw them.
        self.impostor_bakes.prepare(assets, &self.gpu, &mut self.bindless)?;
        self.captures.finish(&self.gpu, frame.slot);
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

        let recording = profile_marker!("Record Passes").scope();
        let Self {
            gpu,
            graph,
            bindless,
            pipelines,
            views,
            assets,
            outputs,
            ui_buffers,
            previews,
            impostor_bakes,
            frame_debugger,
            captures,
            clear,
            mesh_shaders,
            software_rays,
            ..
        } = self;
        let assets = assets.as_ref().expect("assets live until drop");
        let timings = if frame_debugger.on { graph.timings().to_vec() } else { Vec::new() };
        let mut fg = graph.begin(gpu, frame.slot);
        impostor_bakes.record(&mut fg, pipelines, assets, gpu, bindless)?;
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
                    (Target::Offscreen { image, last }, _) => (image.handle, image.view, image.desc, *last),
                    (Target::Window(_), None) => return None,
                };
                let name =
                    if output.id == OutputId::MAIN { "output".to_string() } else { format!("output {}", output.id.0) };
                Some((fg.import_image(&name, (image, view), desc, last), desc))
            })
            .collect();
        let ui = &ui_buffers[frame.slot];
        let ui_push = Push { buffers: ui.pointers.address, ..Push::default() };
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
            };
            let recorded =
                view.record(&mut fg, (gpu, bindless, pipelines, assets), world, &setup, output, frame.slot)?;
            if first {
                backdrops[at] = recorded.backdrop;
            }
            if index == 0 {
                push = Some(recorded.push);
            }
        }
        fg.set_prefix("");
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
        let sampled: Vec<_> = preview_atlas.into_iter().chain(debugger_view).collect();
        let main = images[0];
        if let Some((output, desc)) = main {
            captures.record(&mut fg, gpu, (output, desc), true, frame.slot)?;
        }
        let mut first_quad = 0;
        for ((output, image), backdrop) in outputs.iter().zip(&images).zip(backdrops) {
            let Some((image, desc)) = *image else { continue };
            let target = (image, desc.extent());
            first_quad +=
                output.ui.record(&mut fg, (pipelines.ui, ui_push), &ui.quads, first_quad, target, &sampled, backdrop);
        }
        if let Some((output, desc)) = main {
            captures.record(&mut fg, gpu, (output, desc), false, frame.slot)?;
        }
        for (image, acquired) in images.iter().zip(&acquired) {
            if let (Some((output, _)), Some(_)) = (image, acquired) {
                fg.transition(*output, ImageAccess::Present);
            }
        }
        let push = push.unwrap_or(ui_push);
        frame_debugger.record(&mut fg, (gpu, bindless), (pipelines.frame_debug, push), debugger_view, &timings)?;
        fg.execute(gpu, frame.cmd, bindless);
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
    pub fn read_pixels(&mut self) -> Result<(u32, u32, Vec<u8>)> {
        self.read_output_pixels(OutputId::MAIN)
    }

    /// Reads back what was last drawn into an offscreen output as RGBA8 (sRGB).
    pub fn read_output_pixels(&mut self, id: OutputId) -> Result<(u32, u32, Vec<u8>)> {
        let output = self.outputs.iter_mut().find(|o| o.id == id).ok_or_else(|| anyhow::anyhow!("no such output"))?;
        let Target::Offscreen { image, last } = &mut output.target else {
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
        self.pipelines.destroy(gpu);
        for view in std::mem::take(&mut self.views) {
            view.retire(&mut self.frames);
        }
        let baked = self.impostor_bakes.take_buffers();
        for buffer in self.previews.take_buffers().into_iter().chain(self.captures.take_buffers()).chain(baked) {
            self.frames.retire(Retired::Buffer(buffer));
        }
        for buffers in self.ui_buffers.drain(..) {
            self.frames.retire(Retired::Buffer(buffers.quads));
            self.frames.retire(Retired::Buffer(buffers.pointers));
        }
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
    /// Auto exposure's adapted EV and whether it has started.
    exposure: Buffer,
    /// Last frame's model matrices and camera, for motion vectors.
    prev_models: HashMap<scene::Owner, Mat4Cols>,
    /// Last frame's Objs in their instances' order.
    prev_entities: Vec<scene::Owner>,
    /// Every Foliage's plants, built once (see `foliage`).
    foliage: foliage::FoliageCache,
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
    /// The ray-traced GI's origins last frame, its spacing, and how many
    /// frames it has run, when it had probes this one can carry on from.
    ddgi_history: Option<([[i32; 4]; gpu::DDGI_CASCADES], f32, u32)>,
    /// It has said its scene has too many meshlets to cull one by one.
    warned_meshlets: bool,
}

/// What every view's frame shares.
struct ViewSetup {
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
}

/// What a view gives the frame after it: its push constants, which the UI
/// and frame debugger use for the first view, and its finished image
/// blurred for the UI's backdrop, when kept, with its size and place.
struct ViewRecorded {
    push: Push,
    backdrop: Option<(ImageId, [u32; 2], vk::Rect2D)>,
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
        // Host visible so the adapted exposure shows in the stats.
        let exposure = gpu.create_buffer(&named("exposure"), 16, persistent, MemoryLocation::GpuToCpu)?;
        // Nothing was visible, no LOD chosen and no exposure metered before the first frame.
        gpu.immediate(|cmd| unsafe {
            for buffer in [&visibility, &lod_state, &exposure, &foliage_pushes] {
                gpu.device.cmd_fill_buffer(cmd, buffer.handle, 0, vk::WHOLE_SIZE, 0);
            }
        })?;
        Ok(Self {
            number,
            top_levels: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            top_level_built: (0..FRAMES_IN_FLIGHT).map(|_| (0, Vec::new())).collect(),
            scene_hierarchies: (0..FRAMES_IN_FLIGHT).map(|_| None).collect(),
            output: OutputId::MAIN,
            rect: None,
            options: ViewOptions::default(),
            buffers,
            visibility,
            lod_state,
            foliage_pushes,
            exposure,
            prev_models: HashMap::new(),
            prev_entities: Vec::new(),
            foliage: foliage::FoliageCache::default(),
            warned_instances: false,
            prev_view_proj: None,
            prev_camera: None,
            taa_history: None,
            ao_history: None,
            atmosphere_tables: None,
            fog_history: false,
            ddgi_history: None,
            warned_meshlets: false,
        })
    }

    /// Lets its buffers go once no frame in flight uses them.
    fn retire(self, frames: &mut Frames) {
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
                sky_light,
                meshlet_draws,
                meshlet_draw_instances,
                meshlet_work,
                stats_readback,
                ..
            } = buffers;
            let all = [frame, instances, lights, draws, draw_count, draw_instances, stats, clusters, pointers];
            let meshlets = [meshlet_draws, meshlet_draw_instances, meshlet_work];
            for buffer in all.into_iter().chain([debug_lines, sky_light, stats_readback]).chain(meshlets) {
                frames.retire(Retired::Buffer(buffer));
            }
        }
        for buffer in [self.visibility, self.lod_state, self.exposure, self.foliage_pushes] {
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
        instances: &[gpu::GpuInstance],
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
        let traced: Vec<_> = instances
            .iter()
            .enumerate()
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
    /// into `fg`: each instance with a bottom level, its number its place in
    /// the instance list. `None` without ray tracing.
    fn build_scene(
        (top_level, built, number): (&mut Option<TopLevel>, &mut (u64, Vec<voxear_rhi::Instance>), usize),
        fg: &mut FrameGraph<'_>,
        (gpu, bindless, assets): (&Gpu, &Bindless, &Assets),
        instances: &[gpu::GpuInstance],
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
        let placed: Vec<voxear_rhi::Instance> = instances
            .iter()
            .enumerate()
            .filter_map(|(id, instance)| {
                let mesh = assets.bottom_level(MeshHandle(instance.mesh))?;
                let m = instance.model;
                let transform = [
                    m[0][0], m[1][0], m[2][0], m[3][0], m[0][1], m[1][1], m[2][1], m[3][1], m[0][2], m[1][2], m[2][2],
                    m[3][2],
                ];
                let opaque = !assets.is_cut_out(MaterialHandle(instance.material));
                Some(voxear_rhi::Instance { mesh, transform, id: id as u32, opaque })
            })
            .collect();
        // The same instances, of the same meshes' bottom levels, as this slot's top level was last built from:
        // it stands as it is.
        let changes = assets.bottom_level_changes;
        if built.0 == changes && built.1 == placed && !placed.is_empty() {
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

    fn reset_history(&mut self) {
        self.taa_history = None;
        self.ao_history = None;
        self.atmosphere_tables = None;
        self.fog_history = false;
        self.ddgi_history = None;
        self.prev_models.clear();
        self.prev_view_proj = None;
        self.prev_camera = None;
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
            if let Some(previous) = self.prev_models.get(owner).filter(|_| owner.plant.is_none()) {
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
    ) -> Result<ViewRecorded> {
        let view = self.rect_in(setup.full);
        let extent = view.extent;
        let extracting = profile_marker!("Extract").scope();
        let mut extracted =
            scene::extract(world, (extent.width, extent.height), (setup.time, setup.frame_index), &self.options, &|mesh| {
                assets.mesh_sphere(mesh)
            });
        // The scene is drawn at its render size, which TAA upscales to the view's.
        let output_extent = extent;
        let extent = vk::Extent2D { width: extracted.render_size.0, height: extracted.render_size.1 };
        // Every Foliage's plants within their draw distance, as instances of their own.
        let camera = voxear_core::math::Vec3::from(extracted.frame.camera_position);
        self.foliage.gather(world, camera, &mut extracted, &|mesh| assets.mesh_sphere(mesh), &assets.impostors);
        if extracted.instances.len() > MAX_INSTANCES as usize && !std::mem::replace(&mut self.warned_instances, true) {
            let count = extracted.instances.len();
            tracing::warn!("{count} instances are more than the {MAX_INSTANCES} a view draws; the rest are left out");
        }
        // Glass after the rest, past what culling, shadows and the GI's rays see.
        let opaque = glass::sort_last(&mut extracted.instances, &mut extracted.entities, camera, assets);
        let instance_count = extracted.instances.len().min(MAX_INSTANCES as usize);
        let opaque_count = opaque.min(instance_count);
        self.track_motion(&mut extracted);
        let taa = extracted.post.anti_aliasing.mode == AntiAliasingMode::Taa;
        let taa_reset = self.taa_history != Some(extent);
        self.taa_history = taa.then_some(extent);
        let wireframe = self.options.debug_view == gpu::DEBUG_VIEW_WIREFRAME;
        // The wireframe draws edges where the depth passes would draw faces, so it goes without.
        let occluded = extracted.post.ambient_occlusion.enabled && !wireframe;
        let ao_reset = self.ao_history != Some(extent);
        self.ao_history = occluded.then_some(extent);
        let light_count = extracted.lights.len().min(MAX_LIGHTS as usize);
        drop(extracting);

        // As many meshlets as every instance's draw could turn into, so the meshlet lists never drop any.
        let needed: u64 =
            extracted.instances[..opaque_count].iter().map(|i| u64::from(assets.most_meshlets(i.mesh))).sum();
        let meshlet_capacity = match self.options.meshlet_culling {
            true if needed <= u64::from(MAX_MESHLET_CAPACITY) => {
                self.buffers[slot].fit_meshlets(gpu, slot, needed as u32)?;
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
        let buffers = &self.buffers[slot];
        let color_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let depth_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let hdr = fg.create_image(
            gpu,
            bindless,
            "hdr",
            ImageDesc::new(extent.width, extent.height, HDR_FORMAT, color_usage),
        )?;
        let velocity = fg.create_image(
            gpu,
            bindless,
            "velocity",
            ImageDesc::new(extent.width, extent.height, VELOCITY_FORMAT, color_usage),
        )?;
        let depth = fg.create_image(
            gpu,
            bindless,
            "depth",
            ImageDesc::new(extent.width, extent.height, DEPTH_FORMAT, depth_usage),
        )?;
        let pyramid_desc = pyramid_desc(extent);
        let pyramid = fg.create_image(gpu, bindless, "depth pyramid", pyramid_desc)?;
        let shadows = if extracted.has_sun {
            let desc = ImageDesc::new(ATLAS_RESOLUTION, ATLAS_RESOLUTION, DEPTH_FORMAT, depth_usage);
            Some(fg.shared_image(gpu, bindless, "shadow atlas", desc)?)
        } else {
            None
        };
        let local_shadows = match extracted.local_shadows.is_empty() {
            true => None,
            false => {
                let [width, height] = gpu::LOCAL_SHADOW_ATLAS;
                let desc = ImageDesc::new(width, height, DEPTH_FORMAT, depth_usage);
                Some(fg.shared_image(gpu, bindless, "local shadow atlas", desc)?)
            }
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
        frame_data.post.ao_texture = ambient_occlusion.map_or(NO_TEXTURE, |ao| fg.texture(ao.current).0);
        frame_data.post.ao_reset = u32::from(ao_reset);
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
        let surface = match ambient_occlusion.is_some() || contact_shadows.is_some() || reflections.is_some() {
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
        let previous_ddgi = self.ddgi_history.take();
        let mut scene_hierarchy = (0, 0);
        if lit.enabled && !wireframe {
            // Plants of a Foliage come after the Objs; the rays leave them out, as thousands of small moving
            // meshes cost far more to trace than they change the bounced light.
            let objs = extracted.entities[..opaque_count].iter().position(|owner| owner.plant.is_some());
            let traced = &extracted.instances[..objs.unwrap_or(opaque_count)];
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
                probes = Some(made);
            }
        }
        let panes = match wireframe {
            true => Vec::new(),
            false => glass::panes(&extracted.instances[..instance_count], opaque_count, assets),
        };
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
        self.atmosphere_tables = current;
        let atmosphere = match current {
            Some(_) => Some(atmosphere::Tables::new(fg, gpu, bindless, fresh)?),
            None => None,
        };
        if let Some(tables) = atmosphere {
            frame_data.lighting.transmittance = fg.texture(tables.transmittance).0;
            frame_data.lighting.multi_scattering = fg.texture(tables.multi_scattering).0;
            frame_data.lighting.sky_view = fg.texture(tables.sky_view).0;
            frame_data.lighting.aerial_perspective = fg.texture(tables.aerial_perspective).0;
        }
        frame_data.instance_count = opaque_count as u32;
        frame_data.light_count = light_count as u32;
        frame_data.hiz_texture = fg.texture(pyramid).0;
        frame_data.hiz_size = [pyramid_desc.width, pyramid_desc.height];
        frame_data.hiz_mips = pyramid_desc.mip_levels;
        frame_data.shadow_texture = shadows.map_or(NO_TEXTURE, |atlas| fg.texture(atlas).0);
        frame_data.local_shadows.texture = local_shadows.map_or(NO_TEXTURE, |atlas| fg.texture(atlas).0);
        frame_data.delta_time = setup.delta_time;
        frame_data.frame_index = setup.frame_index as u32;
        frame_data.taa_reset = u32::from(taa_reset);
        frame_data.meshlet_capacity = meshlet_capacity.unwrap_or(0);
        // The wireframe view draws back faces too, and the meshlets view colours each meshlet by its own draw.
        let cones = if self.options.debug_view == gpu::DEBUG_VIEW_WIREFRAME { 0 } else { MESHLET_CONES };
        let runs = if self.options.debug_view == gpu::DEBUG_VIEW_MESHLETS { 0 } else { MESHLET_RUNS };
        let culling = if meshlet_capacity.is_some() { MESHLET_CULLING } else { 0 };
        frame_data.meshlet_flags = cones | runs | culling;
        buffers.frame.write(0, &[frame_data]);
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
                // Only under the atmosphere, which works it out.
                sky_light: if atmosphere.is_some() { buffers.sky_light.address } else { 0 },
            }],
        );
        buffers.instances.write(0, &extracted.instances[..instance_count]);
        buffers.lights.write(0, &extracted.lights[..light_count]);

        let push = Push {
            frame: buffers.frame.address,
            buffers: buffers.pointers.address,
            max_draws: MAX_INSTANCES as u32,
            ..Default::default()
        };
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
            local_shadows: local_shadows.map(|atlas| (atlas, extracted.local_shadows.clone())),
        };
        let draws = passes::Draws {
            draws: fg.import_buffer(&buffers.draws),
            count: fg.import_buffer(&buffers.draw_count),
            instances: fg.import_buffer(&buffers.draw_instances),
            visibility: fg.import_buffer(&self.visibility),
            lod_state: fg.import_buffer(&self.lod_state),
            foliage_pushes: fg.import_buffer(&self.foliage_pushes),
            stats: fg.import_buffer(&buffers.stats),
            stats_readback: fg.import_buffer(&buffers.stats_readback),
            clusters: fg.import_buffer(&buffers.clusters),
            meshlet_draws: fg.import_buffer(&buffers.meshlet_draws),
            meshlet_instances: fg.import_buffer(&buffers.meshlet_draw_instances),
            meshlet_work: fg.import_buffer(&buffers.meshlet_work),
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
            atmosphere,
            volume,
            glass,
            probes,
        };
        passes::record(fg, &frame_setup, &targets, &draws);
        let post = post_passes::PostInput {
            pipelines: *pipelines,
            push,
            extent: output_extent,
            render_extent: extent,
            settings: extracted.post,
            hdr,
            velocity,
            depth,
            output,
            output_rect: (view.extent != setup.full).then_some(view),
            clear: setup.clear,
            clear_outside: setup.first,
            exposure: fg.import_buffer(&self.exposure),
            history,
            keep_ldr: setup.keep_ldr,
            fog_volume: volume.map(|volume| volume.integrated),
            surface,
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
        let gizmos = self.options.gizmos;
        debug_pass::record(fg, (pipelines.debug, push), &buffers.debug_lines, (output, view), depth, gizmos);
        Ok(ViewRecorded { push, backdrop })
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
    }
}

/// The UI's buffers for one frame in flight: its quads, and pointers naming
/// them, which never change.
fn create_ui_buffers(gpu: &Gpu, slot: usize) -> Result<UiBuffers> {
    let (storage, host) = (vk::BufferUsageFlags::STORAGE_BUFFER, MemoryLocation::CpuToGpu);
    let size = ui_pass::MAX_UI_QUADS * size_of::<voxear_ui::Quad>() as u64;
    let quads = gpu.create_buffer(&format!("ui quads {slot}"), size, storage, host)?;
    let pointers = gpu.create_buffer(&format!("ui pointers {slot}"), size_of::<GpuBuffers>() as u64, storage, host)?;
    pointers.write(0, &[GpuBuffers { ui_quads: quads.address, ..bytemuck::Zeroable::zeroed() }]);
    Ok(UiBuffers { quads, pointers })
}

/// A view's buffers for one frame in flight, their names after `prefix`.
fn create_frame_buffers(gpu: &Gpu, prefix: &str, slot: usize) -> Result<FrameBuffers> {
    let host = MemoryLocation::CpuToGpu;
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let indirect = storage | vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
    let lists = u64::from(LIST_COUNT);
    let [meshlet_draws, meshlet_draw_instances, meshlet_work] =
        create_meshlet_lists(gpu, prefix, slot, FIRST_MESHLET_CAPACITY)?;
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
        sky_light: gpu.create_buffer(&format!("{prefix}sky light {slot}"), 32, storage, MemoryLocation::GpuOnly)?,
        meshlet_draws,
        meshlet_draw_instances,
        meshlet_work,
        meshlet_capacity: FIRST_MESHLET_CAPACITY,
        prefix: prefix.to_string(),
    })
}

/// A view's meshlet lists for one frame in flight, for `capacity` (see
/// `gpu::meshlet_list`): their draws, each draw's instance, and the
/// meshlets queued for the meshlet pass.
fn create_meshlet_lists(gpu: &Gpu, prefix: &str, slot: usize, capacity: u32) -> Result<[Buffer; 3]> {
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
