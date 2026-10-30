//! The renderer: a render graph over GPU-driven passes.
//!
//! Each frame the world is extracted into instance, light and frame data.
//! Compute passes cull instances against the frustum, the sun's shadow
//! cascades and a depth pyramid (two-phase occlusion culling), and pick a
//! LOD for each, writing indirect draws, so the CPU never decides
//! visibility. See [`passes`] for the frame's pass order.

mod assets;
mod gltf_import;
mod gpu;
mod graph;
mod lod;
mod mesh;
mod passes;
mod pipelines;
mod scene;
mod shadows;

use std::{
    mem::ManuallyDrop,
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use voxear_ecs::World;
use voxear_rhi::{
    Bindless, Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, GpuInfo, Image, ImageDesc, MemoryLocation, RenderDoc, Retired,
    Swapchain, WindowHandles, full_mip_count, vk,
};

pub use assets::{MaterialDesc, MaterialHandle, MeshHandle};
pub use gltf_import::ImportedNode;
pub use gpu::{DEBUG_VIEW_CASCADES, DEBUG_VIEW_LOD, DEBUG_VIEW_NONE};
pub use graph::{BufferAccess, FrameGraph, ImageAccess, ImageId, PassContext, RenderGraph};
pub use lod::MAX_LODS;
pub use mesh::{MeshData, Primitive, Vertex};
pub use scene::{Camera, Light, LightKind, MeshRenderer, ViewOptions};

use crate::{
    assets::Assets,
    gpu::{DRAW_COMMAND_SIZE, GpuFrame, GpuInstance, GpuLight, LIST_COUNT, NO_TEXTURE, Push, stat},
    pipelines::{DEPTH_FORMAT, HDR_FORMAT, PYRAMID_FORMAT, Pipelines},
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
}

impl RenderStats {
    fn read(counters: &[u32]) -> Self {
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
    buffers: Vec<FrameBuffers>,
    /// Which instances were visible last frame; drives the early pass.
    visibility: Option<Buffer>,
    options: ViewOptions,
    stats: RenderStats,
    pending_size: Option<(u32, u32)>,
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
        Self::build(gpu, Output::Window(swapchain), renderdoc)
    }

    /// Renders into an image with no window, for screenshots, tests and tools.
    pub fn new_offscreen(width: u32, height: u32, settings: &RendererSettings) -> Result<Self> {
        let renderdoc = attach_renderdoc(settings);
        let gpu = Gpu::new_headless()?;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
        let image = gpu.create_image("output", ImageDesc::new(width, height, OFFSCREEN_FORMAT, usage))?;
        Self::build(gpu, Output::Offscreen { image: ManuallyDrop::new(image), last: ImageAccess::Undefined }, renderdoc)
    }

    fn build(gpu: Gpu, output: Output, renderdoc: Option<RenderDoc>) -> Result<Self> {
        let frames = Frames::new(&gpu)?;
        let mut bindless = Bindless::new(&gpu)?;
        let graph = RenderGraph::new(&gpu, FRAMES_IN_FLIGHT)?;
        let assets = Assets::new(&gpu, &mut bindless)?;
        let output_format = match &output {
            Output::Window(swapchain) => swapchain.format.format,
            Output::Offscreen { image, .. } => image.desc.format,
        };
        let pipelines = Pipelines::new(&gpu, &bindless, output_format)?;
        let buffers = (0..FRAMES_IN_FLIGHT).map(|slot| create_frame_buffers(&gpu, slot)).collect::<Result<_>>()?;
        let visibility = gpu.create_buffer(
            "visibility",
            MAX_INSTANCES * 4,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuOnly,
        )?;
        // Nothing was visible before the first frame.
        gpu.immediate(|cmd| unsafe { gpu.device.cmd_fill_buffer(cmd, visibility.handle, 0, vk::WHOLE_SIZE, 0) })?;
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
            buffers,
            visibility: Some(visibility),
            options: ViewOptions::default(),
            stats: RenderStats::default(),
            pending_size: None,
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

    /// Loads a `.gltf` or `.glb` file and returns its drawable pieces.
    pub fn import_gltf(&mut self, path: &Path) -> Result<Vec<ImportedNode>> {
        let assets = self.assets.as_mut().expect("assets live until drop");
        gltf_import::import(&self.gpu, &mut self.bindless, assets, path)
    }

    fn extent(&self) -> vk::Extent2D {
        match &self.output {
            Output::Window(swapchain) => swapchain.extent,
            Output::Offscreen { image, .. } => image.desc.extent(),
        }
    }

    /// Draws the world's cameras, lights and mesh renderers.
    pub fn render(&mut self, world: &World, time: f32) -> Result<()> {
        if let (Some((width, height)), Output::Window(swapchain)) = (self.pending_size.take(), &mut self.output) {
            swapchain.recreate(&self.gpu, width, height)?;
        }
        let extent = self.extent();
        if extent.width == 0 || extent.height == 0 {
            return Ok(());
        }

        let frame = self.frames.begin(&self.gpu)?;
        // This slot's previous frame has finished, so its counters are final.
        self.stats = RenderStats::read(bytemuck::cast_slice(&self.buffers[frame.slot].stats.read()[..stat::COUNT * 4]));
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

        let extracted = scene::extract(world, extent.width, extent.height, time, &self.options);
        let instance_count = extracted.instances.len().min(MAX_INSTANCES as usize);
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

        let Self { gpu, graph, bindless, pipelines, buffers, assets, visibility, .. } = self;
        let assets = assets.as_ref().expect("assets live until drop");
        let visibility = visibility.as_ref().expect("visibility lives until drop");
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
        buffers.frame.write(0, &[frame_data]);
        buffers.instances.write(0, &extracted.instances[..instance_count]);
        buffers.lights.write(0, &extracted.lights[..light_count]);

        let push = Push {
            frame: buffers.frame.address,
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
            max_draws: MAX_INSTANCES as u32,
            ..Default::default()
        };
        let targets = passes::Targets {
            hdr,
            depth,
            depth_texture: fg.texture(depth),
            pyramid,
            pyramid_texture: fg.texture(pyramid),
            pyramid_mips: (0..pyramid_desc.mip_levels as usize).map(|mip| fg.storage(pyramid, mip)).collect(),
            pyramid_size: [pyramid_desc.width, pyramid_desc.height],
            shadows,
            output,
        };
        let draws = passes::Draws {
            draws: fg.import_buffer(&buffers.draws),
            count: fg.import_buffer(&buffers.draw_count),
            instances: fg.import_buffer(&buffers.draw_instances),
            visibility: fg.import_buffer(visibility),
            stats: fg.import_buffer(&buffers.stats),
            draws_handle: buffers.draws.handle,
            count_handle: buffers.draw_count.handle,
            stats_handle: buffers.stats.handle,
            index_buffer: assets.indices.handle,
        };
        let frame_setup = passes::Frame {
            pipelines: *pipelines,
            push,
            instance_count: instance_count as u32,
            extent,
            hdr_texture: fg.texture(hdr),
        };
        passes::record(&mut fg, &frame_setup, &targets, &draws);
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
            let FrameBuffers { frame, instances, lights, draws, draw_count, draw_instances, stats } = buffers;
            for buffer in [frame, instances, lights, draws, draw_count, draw_instances, stats] {
                self.frames.retire(Retired::Buffer(buffer));
            }
        }
        if let Some(visibility) = self.visibility.take() {
            self.frames.retire(Retired::Buffer(visibility));
        }
        self.graph.destroy(gpu, &mut self.bindless);
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
    })
}
