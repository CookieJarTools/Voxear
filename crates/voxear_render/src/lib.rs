//! The renderer: a render graph over GPU-driven passes.
//!
//! Each frame the world is extracted into instance, light and frame data.
//! Compute passes cull instances against the frustum, the sun's shadow
//! cascades and a depth pyramid (two-phase occlusion culling), and pick a
//! LOD for each, writing indirect draws, so the CPU never decides
//! visibility. See [`passes`] for the frame's pass order, and
//! [`post_passes`] for the post-processing after it.

mod alloc;
mod assets;
mod cooked;
mod gltf_import;
mod gpu;
mod graph;
mod hot_reload;
mod lod;
mod mesh;
mod passes;
mod payload;
mod pipelines;
mod post;
mod post_passes;
mod scene;
mod shadows;
mod texture_codec;
mod ui_pass;

use std::{collections::HashMap, mem::ManuallyDrop, path::PathBuf};

use anyhow::{Result, ensure};
use voxear_ecs::{Entity, World};
use voxear_rhi::{
    Bindless, Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, GpuInfo, Image, ImageDesc, MemoryLocation, RenderDoc, Retired,
    Swapchain, WindowHandles, full_mip_count, vk,
};

pub use assets::AssetUsage;
pub use assets::{MaterialDesc, MaterialHandle, MeshHandle};
pub use cooked::{COOKED_VERSION, CookedModel};
pub use gltf_import::{
    ImportedModel, ImportedNode, PreparedModel, ReadFile, companion_files, prepare as prepare_model,
};
pub use gpu::{DEBUG_VIEW_CASCADES, DEBUG_VIEW_LIGHT_COMPLEXITY, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE, DEBUG_VIEW_NORMALS};
pub use graph::{BufferAccess, FrameGraph, ImageAccess, ImageId, PassContext, RenderGraph};
pub use lod::MAX_LODS;
pub use mesh::{MeshData, Primitive, Vertex};
pub use post::{
    AntiAliasing, AntiAliasingMode, Bloom, DepthOfField, Exposure, Fog, Grading, Grain, MotionBlur, PostProcess,
    Tonemap, Tonemapper, Vignette,
};
pub use scene::{Camera, Light, LightKind, MeshRenderer, Sky, ViewOptions};
pub use voxear_rhi::TextureId;

use crate::{
    assets::Assets,
    gpu::{
        CLUSTER_COUNT, DRAW_COMMAND_SIZE, GpuBuffers, GpuFrame, GpuInstance, GpuLight, LIST_COUNT, MAX_CLUSTER_LIGHTS,
        Mat4Cols, NO_TEXTURE, Push, stat,
    },
    hot_reload::{ShaderSet, ShaderWatcher},
    pipelines::{DEPTH_FORMAT, HDR_FORMAT, PYRAMID_FORMAT, Pipelines, VELOCITY_FORMAT},
    shadows::ATLAS_RESOLUTION,
};

const MAX_INSTANCES: u64 = 64 * 1024;
const MAX_LIGHTS: u64 = 1024;
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
}

fn attach_renderdoc(settings: &RendererSettings) -> Option<RenderDoc> {
    let renderdoc = RenderDoc::attach(settings.renderdoc)?;
    if let Some(folder) = &settings.capture_folder {
        renderdoc.set_capture_folder(folder, "frame");
    }
    Some(renderdoc)
}

enum Output {
    Window(Swapchain),
    // Destroyed explicitly in `Drop`, which needs the GPU.
    Offscreen { image: ManuallyDrop<Image>, last: ImageAccess },
}

struct FrameBuffers {
    frame: Buffer,
    instances: Buffer,
    lights: Buffer,
    draws: Buffer,
    draw_count: Buffer,
    draw_instances: Buffer,
    stats: Buffer,
    clusters: Buffer,
    pointers: Buffer,
    ui: Buffer,
}

pub struct Renderer {
    renderdoc: Option<RenderDoc>,
    show_capture: bool,
    gpu: Gpu,
    output: Output,
    frames: Frames,
    bindless: Bindless,
    graph: RenderGraph,
    assets: Option<Assets>,
    pipelines: Pipelines,
    output_format: vk::Format,
    watcher: Option<ShaderWatcher>,
    buffers: Vec<FrameBuffers>,
    /// Which instances were visible last frame; drives the early pass.
    visibility: Option<Buffer>,
    /// Each instance's current and previous LOD and fade progress.
    lod_state: Option<Buffer>,
    /// Auto exposure's adapted EV and whether it has started.
    exposure: Option<Buffer>,
    /// Last frame's model matrices and camera, for motion vectors.
    prev_models: HashMap<Entity, Mat4Cols>,
    prev_view_proj: Option<Mat4Cols>,
    /// The size TAA's history was rendered at; `None` when it is stale.
    taa_history: Option<vk::Extent2D>,
    frame_index: u64,
    last_time: Option<f32>,
    options: ViewOptions,
    stats: RenderStats,
    pending_size: Option<(u32, u32)>,
    ui: ui_pass::UiGpu,
    gpu_decompression: bool,
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
        Self::build(gpu, Output::Window(swapchain), renderdoc, settings.hot_reload)
    }

    /// Renders into an image with no window, for screenshots, tests and tools.
    pub fn new_offscreen(width: u32, height: u32, settings: &RendererSettings) -> Result<Self> {
        let renderdoc = attach_renderdoc(settings);
        let gpu = Gpu::new_headless()?;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let image = gpu.create_image("output", ImageDesc::new(width, height, OFFSCREEN_FORMAT, usage))?;
        let output = Output::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined };
        Self::build(gpu, output, renderdoc, settings.hot_reload)
    }

    fn build(gpu: Gpu, output: Output, renderdoc: Option<RenderDoc>, hot_reload: bool) -> Result<Self> {
        let frames = Frames::new(&gpu)?;
        let mut bindless = Bindless::new(&gpu)?;
        let graph = RenderGraph::new(&gpu, FRAMES_IN_FLIGHT)?;
        let assets = Assets::new(&gpu, &mut bindless)?;
        let output_format = match &output {
            Output::Window(swapchain) => swapchain.format.format,
            Output::Offscreen { image, .. } => image.desc.format,
        };
        let pipelines = Pipelines::new(&gpu, &bindless, output_format, &ShaderSet::embedded())?;
        let buffers = (0..FRAMES_IN_FLIGHT).map(|slot| create_frame_buffers(&gpu, slot)).collect::<Result<_>>()?;
        let persistent = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let visibility = gpu.create_buffer("visibility", MAX_INSTANCES * 4, persistent, MemoryLocation::GpuOnly)?;
        let lod_state = gpu.create_buffer("lod state", MAX_INSTANCES * 4, persistent, MemoryLocation::GpuOnly)?;
        // Host visible so the adapted exposure shows in the stats.
        let exposure = gpu.create_buffer("exposure", 16, persistent, MemoryLocation::GpuToCpu)?;
        // Nothing was visible, no LOD chosen and no exposure metered before the first frame.
        gpu.immediate(|cmd| unsafe {
            for buffer in [&visibility, &lod_state, &exposure] {
                gpu.device.cmd_fill_buffer(cmd, buffer.handle, 0, vk::WHOLE_SIZE, 0);
            }
        })?;
        Ok(Self {
            renderdoc,
            show_capture: false,
            gpu,
            output,
            frames,
            bindless,
            graph,
            assets: Some(assets),
            pipelines,
            output_format,
            watcher: if hot_reload { ShaderWatcher::new() } else { None },
            buffers,
            visibility: Some(visibility),
            lod_state: Some(lod_state),
            exposure: Some(exposure),
            prev_models: HashMap::new(),
            prev_view_proj: None,
            taa_history: None,
            frame_index: 0,
            last_time: None,
            options: ViewOptions::default(),
            stats: RenderStats::default(),
            pending_size: None,
            ui: ui_pass::UiGpu::default(),
            gpu_decompression: true,
        })
    }

    pub fn gpu(&self) -> &GpuInfo {
        &self.gpu.info
    }

    /// GPU milliseconds per pass from the latest completed frame.
    pub fn timings(&self) -> &[(String, f32)] {
        self.graph.timings()
    }

    /// Culling and LOD counts from a recent frame.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    pub fn set_view_options(&mut self, options: ViewOptions) {
        self.options = options;
    }

    /// Forgets last frame for TAA and motion vectors. Call on camera cuts,
    /// where blending with the old view would smear.
    pub fn reset_history(&mut self) {
        self.taa_history = None;
        self.prev_models.clear();
        self.prev_view_proj = None;
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
        self.pending_size = Some((width, height));
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

    /// Loads a `.gltf` or `.glb` file and returns its drawable pieces. `path`
    /// is the model's project path, and `read` reads it and every file it
    /// names, from wherever the content lives.
    pub fn import_gltf(&mut self, path: &str, read: &mut ReadFile) -> Result<ImportedModel> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::import(&self.gpu, &mut self.bindless, assets, path, read)
    }

    /// Uploads a model made with [`prepare_model`], for example on a
    /// loading thread.
    pub fn upload_model(&mut self, model: &PreparedModel) -> Result<ImportedModel> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::upload(&self.gpu, &mut self.bindless, assets, model)
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

    pub fn free_texture(&mut self, texture: TextureId) {
        let frame = self.frame_index;
        self.assets.as_mut().expect("assets live until drop").free_texture(texture, frame);
    }

    /// How much of the GPU asset storage is in use.
    pub fn asset_usage(&self) -> AssetUsage {
        self.assets.as_ref().expect("assets live until drop").usage()
    }

    fn extent(&self) -> vk::Extent2D {
        match &self.output {
            Output::Window(swapchain) => swapchain.extent,
            Output::Offscreen { image, .. } => image.desc.extent(),
        }
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

    /// Draws the world's cameras, lights and mesh renderers.
    pub fn render(&mut self, world: &World, time: f32) -> Result<()> {
        self.reload_shaders();
        if let (Some((width, height)), Output::Window(swapchain)) = (self.pending_size.take(), &mut self.output) {
            swapchain.recreate(&self.gpu, width, height)?;
        }
        let extent = self.extent();
        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }

        let frame = self.frames.begin(&self.gpu)?;
        // This slot's previous frame has finished, and with it every frame that could use what was freed before it.
        let assets = self.assets.as_mut().expect("assets live until drop");
        assets.collect(&self.gpu, &mut self.bindless, self.frame_index);
        // This slot's previous frame has finished, so its counters are final.
        let counters = bytemuck::cast_slice(&self.buffers[frame.slot].stats.read()[..stat::COUNT * 4]);
        let exposure = bytemuck::cast_slice(self.exposure.as_ref().expect("exposure lives until drop").read());
        self.stats = RenderStats::read(counters, exposure);
        let acquired = match &self.output {
            Output::Window(swapchain) => match self.frames.acquire(&self.gpu, swapchain)? {
                Some(acquired) => Some(acquired),
                None => {
                    self.pending_size = Some((extent.width, extent.height));
                    return Ok(());
                }
            },
            Output::Offscreen { .. } => None,
        };

        let mut extracted = scene::extract(world, extent.width, extent.height, time, self.frame_index, &self.options);
        let delta_time = self.last_time.map_or(0.0, |last| (time - last).clamp(0.0, 0.25));
        self.last_time = Some(time);
        let instance_count = extracted.instances.len().min(MAX_INSTANCES as usize);
        self.track_motion(&mut extracted);
        let taa = extracted.post.anti_aliasing.mode == AntiAliasingMode::Taa;
        let taa_reset = self.taa_history != Some(extent);
        self.taa_history = taa.then_some(extent);
        let frame_index = self.frame_index;
        self.frame_index += 1;
        let light_count = extracted.lights.len().min(MAX_LIGHTS as usize);

        let (output_image, output_view, output_desc, output_last) = match &self.output {
            Output::Window(swapchain) => {
                let acquired = acquired.as_ref().expect("acquired above");
                let desc =
                    ImageDesc::new(extent.width, extent.height, swapchain.format.format, vk::ImageUsageFlags::empty());
                (acquired.image, acquired.view, desc, ImageAccess::Undefined)
            }
            Output::Offscreen { image, last } => (image.handle, image.view, image.desc, *last),
        };

        let Self { gpu, graph, bindless, pipelines, buffers, assets, visibility, lod_state, exposure, ui, .. } = self;
        let assets = assets.as_ref().expect("assets live until drop");
        let visibility = visibility.as_ref().expect("visibility lives until drop");
        let lod_state = lod_state.as_ref().expect("LOD state lives until drop");
        let exposure = exposure.as_ref().expect("exposure lives until drop");
        let buffers = &buffers[frame.slot];
        let mut fg = graph.begin(gpu, frame.slot);

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
            Some(fg.create_image(gpu, bindless, "shadow atlas", desc)?)
        } else {
            None
        };
        let output = fg.import_image(output_image, output_view, output_desc, output_last);

        let mut frame_data = extracted.frame;
        frame_data.instance_count = instance_count as u32;
        frame_data.light_count = light_count as u32;
        frame_data.hiz_texture = fg.texture(pyramid).0;
        frame_data.hiz_size = [pyramid_desc.width, pyramid_desc.height];
        frame_data.hiz_mips = pyramid_desc.mip_levels;
        frame_data.shadow_texture = shadows.map_or(NO_TEXTURE, |atlas| fg.texture(atlas).0);
        frame_data.delta_time = delta_time;
        frame_data.frame_index = frame_index as u32;
        frame_data.taa_reset = u32::from(taa_reset);
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
                visibility: visibility.address,
                stats: buffers.stats.address,
                clusters: buffers.clusters.address,
                lod_state: lod_state.address,
                exposure: exposure.address,
                ui_quads: buffers.ui.address,
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
        };
        let draws = passes::Draws {
            draws: fg.import_buffer(&buffers.draws),
            count: fg.import_buffer(&buffers.draw_count),
            instances: fg.import_buffer(&buffers.draw_instances),
            visibility: fg.import_buffer(visibility),
            lod_state: fg.import_buffer(lod_state),
            stats: fg.import_buffer(&buffers.stats),
            clusters: fg.import_buffer(&buffers.clusters),
            draws_handle: buffers.draws.handle,
            count_handle: buffers.draw_count.handle,
            stats_handle: buffers.stats.handle,
            index_buffer: assets.indices.handle,
        };
        let frame_setup = passes::Frame { pipelines: *pipelines, push, instance_count: instance_count as u32, extent };
        passes::record(&mut fg, &frame_setup, &targets, &draws);
        let post = post_passes::PostInput {
            pipelines: *pipelines,
            push,
            extent,
            settings: extracted.post,
            hdr,
            velocity,
            depth,
            output,
            exposure: fg.import_buffer(exposure),
            history: (frame_index % 2) as usize,
        };
        post_passes::record(&mut fg, gpu, bindless, &post)?;
        ui.record(&mut fg, pipelines.ui, push, &buffers.ui, (output, extent));
        if acquired.is_some() {
            fg.transition(output, ImageAccess::Present);
        }
        fg.execute(gpu, frame.cmd, bindless);

        let needs_resize = match (&mut self.output, acquired) {
            (Output::Window(swapchain), Some(acquired)) => {
                self.frames.submit(&self.gpu, frame, Some((swapchain, acquired)))?
            }
            (Output::Offscreen { last, .. }, _) => {
                *last = ImageAccess::ColorAttachment;
                self.frames.submit(&self.gpu, frame, None)?
            }
            (Output::Window(_), None) => unreachable!("windows always acquire"),
        };
        if needs_resize {
            self.pending_size = Some((extent.width, extent.height));
        }
        if self.show_capture
            && let Some(renderdoc) = self.renderdoc.as_ref().filter(|r| !r.is_capturing())
        {
            renderdoc.show_ui();
            self.show_capture = false;
        }
        Ok(())
    }

    /// Gives each instance last frame's model matrix and the frame last
    /// frame's camera, so shaders can work out how far each pixel moved.
    /// Objs seen for the first time count as still.
    fn track_motion(&mut self, extracted: &mut scene::ExtractedFrame) {
        for (instance, entity) in extracted.instances.iter_mut().zip(&extracted.entities) {
            if let Some(previous) = self.prev_models.get(entity) {
                instance.prev_model = *previous;
            }
        }
        self.prev_models.clear();
        self.prev_models.extend(extracted.entities.iter().zip(&extracted.instances).map(|(e, i)| (*e, i.model)));
        let current = extracted.frame.unjittered_view_proj;
        extracted.frame.prev_view_proj = self.prev_view_proj.replace(current).unwrap_or(current);
    }

    /// What the UI draws over every frame from now on, until replaced. An
    /// empty frame draws nothing.
    pub fn set_ui(&mut self, frame: &voxear_ui::UiFrame) -> Result<()> {
        self.ui.set(&self.gpu, &mut self.bindless, frame)
    }

    /// Reads back the last rendered offscreen frame as RGBA8 (sRGB).
    pub fn read_pixels(&mut self) -> Result<(u32, u32, Vec<u8>)> {
        let Output::Offscreen { image, last } = &mut self.output else {
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
        for buffers in self.buffers.drain(..) {
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
                ui,
            } = buffers;
            for buffer in [frame, instances, lights, draws, draw_count, draw_instances, stats, clusters, pointers, ui] {
                self.frames.retire(Retired::Buffer(buffer));
            }
        }
        for buffer in [self.visibility.take(), self.lod_state.take(), self.exposure.take()].into_iter().flatten() {
            self.frames.retire(Retired::Buffer(buffer));
        }
        self.graph.destroy(gpu, &mut self.bindless);
        self.ui.destroy(gpu, &mut self.bindless);
        if let Some(assets) = self.assets.take() {
            assets.destroy(gpu, &mut self.bindless);
        }
        self.frames.destroy(gpu);
        self.bindless.destroy(gpu);
        match &mut self.output {
            Output::Window(swapchain) => swapchain.destroy(gpu),
            Output::Offscreen { image, .. } => gpu.destroy_image(unsafe { ManuallyDrop::take(image) }),
        }
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

fn create_frame_buffers(gpu: &Gpu, slot: usize) -> Result<FrameBuffers> {
    let host = MemoryLocation::CpuToGpu;
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let indirect = storage | vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
    let lists = u64::from(LIST_COUNT);
    Ok(FrameBuffers {
        frame: gpu.create_buffer(&format!("frame {slot}"), size_of::<GpuFrame>() as u64, storage, host)?,
        instances: gpu.create_buffer(
            &format!("instances {slot}"),
            MAX_INSTANCES * size_of::<GpuInstance>() as u64,
            storage,
            host,
        )?,
        lights: gpu.create_buffer(
            &format!("lights {slot}"),
            MAX_LIGHTS * size_of::<GpuLight>() as u64,
            storage,
            host,
        )?,
        draws: gpu.create_buffer(
            &format!("draws {slot}"),
            lists * MAX_INSTANCES * DRAW_COMMAND_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        draw_count: gpu.create_buffer(&format!("draw counts {slot}"), lists * 4, indirect, MemoryLocation::GpuOnly)?,
        draw_instances: gpu.create_buffer(
            &format!("draw instances {slot}"),
            lists * MAX_INSTANCES * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        stats: gpu.create_buffer(
            &format!("stats {slot}"),
            stat::COUNT as u64 * 4,
            storage | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )?,
        clusters: gpu.create_buffer(
            &format!("light clusters {slot}"),
            u64::from(CLUSTER_COUNT) * u64::from(MAX_CLUSTER_LIGHTS + 1) * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
        pointers: gpu.create_buffer(&format!("pointers {slot}"), size_of::<GpuBuffers>() as u64, storage, host)?,
        ui: gpu.create_buffer(
            &format!("ui quads {slot}"),
            ui_pass::MAX_UI_QUADS * size_of::<voxear_ui::Quad>() as u64,
            storage,
            host,
        )?,
    })
}
