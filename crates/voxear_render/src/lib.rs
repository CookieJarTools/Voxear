//! The renderer: a render graph over GPU-driven passes.
//!
//! Each frame the world is extracted into instance, light and frame data. A
//! compute pass culls instances against the frustum and writes indirect
//! draws, so the CPU never decides visibility. The opaque pass draws
//! everything in one indirect call, then the sky fills the background and
//! tonemapping writes the final image.

mod assets;
mod gltf_import;
mod gpu;
mod graph;
mod mesh;
mod scene;

use std::{
    mem::ManuallyDrop,
    path::{Path, PathBuf},
};

use anyhow::{Result, ensure};
use voxear_ecs::World;
use voxear_rhi::{
    Bindless, Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, GpuInfo, GraphicsPipelineDesc, Image, ImageDesc,
    MemoryLocation, RenderDoc, Retired, ShaderModule, Swapchain, WindowHandles, create_compute_pipeline,
    create_graphics_pipeline, vk,
};

pub use assets::{MaterialDesc, MaterialHandle, MeshHandle};
pub use gltf_import::ImportedNode;
pub use graph::{BufferAccess, FrameGraph, ImageAccess, ImageId, PassContext, RenderGraph};
pub use mesh::{MeshData, Primitive, Vertex};
pub use scene::{Camera, Light, LightKind, MeshRenderer};

use crate::{
    assets::Assets,
    gpu::{DRAW_COMMAND_SIZE, GpuFrame, GpuInstance, GpuLight, Push},
};

const MAX_INSTANCES: u64 = 64 * 1024;
const MAX_LIGHTS: u64 = 1024;
const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
const OFFSCREEN_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;

const CULL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cull.spv"));
const FORWARD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/forward.spv"));
const SKY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sky.spv"));
const TONEMAP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tonemap.spv"));

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
}

struct Pipelines {
    cull: vk::Pipeline,
    forward: vk::Pipeline,
    sky: vk::Pipeline,
    tonemap: vk::Pipeline,
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
        let pipelines = create_pipelines(&gpu, &bindless, output_format)?;
        let buffers = (0..FRAMES_IN_FLIGHT).map(|slot| create_frame_buffers(&gpu, slot)).collect::<Result<_>>()?;
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
            pending_size: None,
        })
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

    pub fn gpu(&self) -> &GpuInfo {
        &self.gpu.info
    }

    /// GPU milliseconds per pass from the latest completed frame.
    pub fn timings(&self) -> &[(String, f32)] {
        self.graph.timings()
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

        let extracted = scene::extract(world, extent.width, extent.height, time);
        let buffers = &self.buffers[frame.slot];
        let instance_count = extracted.instances.len().min(MAX_INSTANCES as usize);
        let light_count = extracted.lights.len().min(MAX_LIGHTS as usize);
        let mut frame_data = extracted.frame;
        frame_data.instance_count = instance_count as u32;
        frame_data.light_count = light_count as u32;
        buffers.frame.write(0, &[frame_data]);
        buffers.instances.write(0, &extracted.instances[..instance_count]);
        buffers.lights.write(0, &extracted.lights[..light_count]);

        let assets = self.assets.as_ref().expect("assets live until drop");
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
            texture: 0,
            max_draws: MAX_INSTANCES as u32,
        };

        let (output_image, output_view, output_desc, output_last) = match &self.output {
            Output::Window(swapchain) => {
                let acquired = acquired.as_ref().expect("acquired above");
                let desc = ImageDesc::new(extent.width, extent.height, swapchain.format.format, vk::ImageUsageFlags::empty());
                (acquired.image, acquired.view, desc, ImageAccess::Undefined)
            }
            Output::Offscreen { image, last } => (image.handle, image.view, image.desc, *last),
        };

        let Self { gpu, graph, bindless, pipelines, .. } = self;
        let mut fg = graph.begin(gpu, frame.slot);
        let hdr_desc = ImageDesc::new(
            extent.width,
            extent.height,
            HDR_FORMAT,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        );
        let hdr = fg.create_image(gpu, bindless, "hdr", hdr_desc)?;
        let depth_desc = ImageDesc::new(
            extent.width,
            extent.height,
            DEPTH_FORMAT,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        );
        let depth = fg.create_image(gpu, bindless, "depth", depth_desc)?;
        let output = fg.import_image(output_image, output_view, output_desc, output_last);
        let hdr_texture = fg.texture(hdr);

        let buffers = &self.buffers[frame.slot];
        let draws = fg.import_buffer(&buffers.draws);
        let draw_count = fg.import_buffer(&buffers.draw_count);
        let draw_instances = fg.import_buffer(&buffers.draw_instances);
        let (draws_handle, count_handle, index_buffer) =
            (buffers.draws.handle, buffers.draw_count.handle, assets.indices.handle);

        fg.add_pass("Reset draws", &[], &[(draw_count, BufferAccess::TransferWrite)], move |ctx| unsafe {
            ctx.gpu.device.cmd_fill_buffer(ctx.cmd, count_handle, 0, 4, 0);
        });

        let cull_pipeline = pipelines.cull;
        fg.add_pass(
            "Cull",
            &[],
            &[
                (draw_count, BufferAccess::ComputeWrite),
                (draws, BufferAccess::ComputeWrite),
                (draw_instances, BufferAccess::ComputeWrite),
            ],
            move |ctx| unsafe {
                ctx.gpu.device.cmd_bind_pipeline(ctx.cmd, vk::PipelineBindPoint::COMPUTE, cull_pipeline);
                ctx.bindless.bind(ctx.gpu, ctx.cmd, vk::PipelineBindPoint::COMPUTE);
                ctx.bindless.push(ctx.gpu, ctx.cmd, &push);
                ctx.gpu.device.cmd_dispatch(ctx.cmd, (instance_count as u32).div_ceil(64).max(1), 1, 1);
            },
        );

        let forward_pipeline = pipelines.forward;
        fg.add_pass(
            "Opaque",
            &[(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthAttachment)],
            &[
                (draws, BufferAccess::IndirectRead),
                (draw_count, BufferAccess::IndirectRead),
                (draw_instances, BufferAccess::ShaderRead),
            ],
            move |ctx| {
                let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, Some(clear_color([0.0; 4])))];
                let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, Some(clear_depth(0.0)));
                begin_rendering(ctx, extent, &color, Some(&depth));
                unsafe {
                    let device = &ctx.gpu.device;
                    device.cmd_bind_pipeline(ctx.cmd, vk::PipelineBindPoint::GRAPHICS, forward_pipeline);
                    ctx.bindless.bind(ctx.gpu, ctx.cmd, vk::PipelineBindPoint::GRAPHICS);
                    ctx.bindless.push(ctx.gpu, ctx.cmd, &push);
                    device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32);
                    device.cmd_draw_indexed_indirect_count(
                        ctx.cmd,
                        draws_handle,
                        0,
                        count_handle,
                        0,
                        MAX_INSTANCES as u32,
                        DRAW_COMMAND_SIZE as u32,
                    );
                    device.cmd_end_rendering(ctx.cmd);
                }
            },
        );

        let sky_pipeline = pipelines.sky;
        fg.add_pass(
            "Sky",
            &[(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthRead)],
            &[],
            move |ctx| {
                let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
                let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL, None);
                begin_rendering(ctx, extent, &color, Some(&depth));
                fullscreen(ctx, sky_pipeline, &push);
            },
        );

        let tonemap_pipeline = pipelines.tonemap;
        let tonemap_push = Push { texture: hdr_texture.0, ..push };
        fg.add_pass(
            "Tonemap",
            &[(hdr, ImageAccess::Sampled), (output, ImageAccess::ColorAttachment)],
            &[],
            move |ctx| {
                let color = [attachment(ctx.view(output), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
                begin_rendering(ctx, extent, &color, None);
                fullscreen(ctx, tonemap_pipeline, &tonemap_push);
            },
        );

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
        if self.show_capture && let Some(renderdoc) = self.renderdoc.as_ref().filter(|r| !r.is_capturing()) {
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
        unsafe {
            for pipeline in [self.pipelines.cull, self.pipelines.forward, self.pipelines.sky, self.pipelines.tonemap] {
                gpu.device.destroy_pipeline(pipeline, None);
            }
        }
        for buffers in self.buffers.drain(..) {
            for buffer in
                [buffers.frame, buffers.instances, buffers.lights, buffers.draws, buffers.draw_count, buffers.draw_instances]
            {
                self.frames.retire(Retired::Buffer(buffer));
            }
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

fn create_frame_buffers(gpu: &Gpu, slot: usize) -> Result<FrameBuffers> {
    let host = MemoryLocation::CpuToGpu;
    let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
    let indirect = storage | vk::BufferUsageFlags::INDIRECT_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
    Ok(FrameBuffers {
        frame: gpu.create_buffer(&format!("frame {slot}"), size_of::<GpuFrame>() as u64, storage, host)?,
        instances: gpu.create_buffer(
            &format!("instances {slot}"),
            MAX_INSTANCES * size_of::<GpuInstance>() as u64,
            storage,
            host,
        )?,
        lights: gpu.create_buffer(&format!("lights {slot}"), MAX_LIGHTS * size_of::<GpuLight>() as u64, storage, host)?,
        draws: gpu.create_buffer(
            &format!("draws {slot}"),
            MAX_INSTANCES * DRAW_COMMAND_SIZE,
            indirect,
            MemoryLocation::GpuOnly,
        )?,
        draw_count: gpu.create_buffer(&format!("draw count {slot}"), 4, indirect, MemoryLocation::GpuOnly)?,
        draw_instances: gpu.create_buffer(
            &format!("draw instances {slot}"),
            MAX_INSTANCES * 4,
            storage,
            MemoryLocation::GpuOnly,
        )?,
    })
}

fn create_pipelines(gpu: &Gpu, bindless: &Bindless, output_format: vk::Format) -> Result<Pipelines> {
    let layout = bindless.pipeline_layout;
    let cull_module = ShaderModule::new(gpu, "cull.slang", CULL)?;
    let forward_module = ShaderModule::new(gpu, "forward.slang", FORWARD)?;
    let sky_module = ShaderModule::new(gpu, "sky.slang", SKY)?;
    let tonemap_module = ShaderModule::new(gpu, "tonemap.slang", TONEMAP)?;

    let cull = create_compute_pipeline(gpu, layout, "Cull", &cull_module, c"cullMain")?;
    let forward = create_graphics_pipeline(
        gpu,
        layout,
        &GraphicsPipelineDesc {
            color_formats: &[HDR_FORMAT],
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::new("Opaque", &forward_module, c"vertexMain", c"fragmentMain")
        },
    )?;
    let sky = create_graphics_pipeline(
        gpu,
        layout,
        &GraphicsPipelineDesc {
            color_formats: &[HDR_FORMAT],
            depth_format: Some(DEPTH_FORMAT),
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            cull: vk::CullModeFlags::NONE,
            ..GraphicsPipelineDesc::new("Sky", &sky_module, c"skyVertex", c"skyFragment")
        },
    )?;
    let tonemap = create_graphics_pipeline(
        gpu,
        layout,
        &GraphicsPipelineDesc {
            color_formats: &[output_format],
            cull: vk::CullModeFlags::NONE,
            ..GraphicsPipelineDesc::new("Tonemap", &tonemap_module, c"tonemapVertex", c"tonemapFragment")
        },
    )?;

    for module in [cull_module, forward_module, sky_module, tonemap_module] {
        module.destroy(gpu);
    }
    Ok(Pipelines { cull, forward, sky, tonemap })
}

fn clear_color(color: [f32; 4]) -> vk::ClearValue {
    vk::ClearValue { color: vk::ClearColorValue { float32: color } }
}

fn clear_depth(depth: f32) -> vk::ClearValue {
    vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 } }
}

/// A rendering attachment that clears when given a value and loads otherwise.
fn attachment(view: vk::ImageView, layout: vk::ImageLayout, clear: Option<vk::ClearValue>) -> vk::RenderingAttachmentInfo<'static> {
    let info = vk::RenderingAttachmentInfo::default().image_view(view).image_layout(layout).store_op(vk::AttachmentStoreOp::STORE);
    match clear {
        Some(value) => info.load_op(vk::AttachmentLoadOp::CLEAR).clear_value(value),
        None => info.load_op(vk::AttachmentLoadOp::LOAD),
    }
}

fn begin_rendering(
    ctx: &PassContext,
    extent: vk::Extent2D,
    color: &[vk::RenderingAttachmentInfo],
    depth: Option<&vk::RenderingAttachmentInfo>,
) {
    let area = vk::Rect2D { offset: vk::Offset2D::default(), extent };
    let mut info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(color);
    if let Some(depth) = depth {
        info = info.depth_attachment(depth);
    }
    unsafe { ctx.gpu.device.cmd_begin_rendering(ctx.cmd, &info) };
    ctx.set_viewport(extent);
}

/// Draws one screen-covering triangle and ends the rendering scope.
fn fullscreen(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push) {
    unsafe {
        let device = &ctx.gpu.device;
        device.cmd_bind_pipeline(ctx.cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
        ctx.bindless.bind(ctx.gpu, ctx.cmd, vk::PipelineBindPoint::GRAPHICS);
        ctx.bindless.push(ctx.gpu, ctx.cmd, push);
        device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
        device.cmd_end_rendering(ctx.cmd);
    }
}
