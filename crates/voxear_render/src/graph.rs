//! A render graph: passes declare how they use images and buffers, and the
//! graph derives every layout transition and barrier, labels each pass for
//! GPU tools, and times it.
//!
//! Version one runs passes in declaration order on one queue. Transient
//! images persist between frames (keyed by name) and keep their last state,
//! so the first barrier of a frame also orders against the previous frame.
//!
//! Every graph-owned image can be copied from, so the frame debugger can
//! copy any of them after the pass it is looking at (see [`recorded`] and
//! [`insert_pass`]).
//!
//! [`recorded`]: FrameGraph::recorded
//! [`insert_pass`]: FrameGraph::insert_pass

use std::collections::HashMap;

use anyhow::Result;
use voxear_rhi::{Bindless, Buffer, Gpu, GpuTimer, Image, ImageDesc, StorageImageId, TextureId, vk};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(usize);

/// How a pass uses an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAccess {
    /// Contents are discarded (the start of a transient's life each frame).
    Undefined,
    ColorAttachment,
    DepthAttachment,
    DepthRead,
    /// Sampled from fragment or compute shaders.
    Sampled,
    /// Sampled from fragment, compute or task shaders; only where the GPU
    /// has task shaders.
    SampledWithTasks,
    StorageWrite,
    /// Written as a storage image from fragment shaders.
    FragmentStorageWrite,
    TransferSrc,
    TransferDst,
    Present,
}

/// How a pass uses a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferAccess {
    TransferRead,
    TransferWrite,
    ComputeWrite,
    ComputeRead,
    IndirectRead,
    ShaderRead,
    /// Read by task and mesh shaders; only where the GPU has them.
    MeshShaderRead,
    /// Read and written by task shaders, as culling's counters are; only
    /// where the GPU has them.
    TaskWrite,
    /// Read on the CPU after the frame's fence.
    HostRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    layout: vk::ImageLayout,
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
}

const UNTOUCHED: State =
    State { layout: vk::ImageLayout::UNDEFINED, stage: vk::PipelineStageFlags2::NONE, access: vk::AccessFlags2::NONE };

impl ImageAccess {
    fn state(self) -> State {
        use vk::{AccessFlags2 as A, ImageLayout as L, PipelineStageFlags2 as S};
        let (layout, stage, access) = match self {
            Self::Undefined => (L::UNDEFINED, S::NONE, A::NONE),
            Self::ColorAttachment => (
                L::COLOR_ATTACHMENT_OPTIMAL,
                S::COLOR_ATTACHMENT_OUTPUT,
                A::COLOR_ATTACHMENT_READ | A::COLOR_ATTACHMENT_WRITE,
            ),
            Self::DepthAttachment => (
                L::DEPTH_ATTACHMENT_OPTIMAL,
                S::EARLY_FRAGMENT_TESTS | S::LATE_FRAGMENT_TESTS,
                A::DEPTH_STENCIL_ATTACHMENT_READ | A::DEPTH_STENCIL_ATTACHMENT_WRITE,
            ),
            Self::DepthRead => (
                L::DEPTH_READ_ONLY_OPTIMAL,
                S::EARLY_FRAGMENT_TESTS | S::LATE_FRAGMENT_TESTS,
                A::DEPTH_STENCIL_ATTACHMENT_READ,
            ),
            Self::Sampled => {
                (L::SHADER_READ_ONLY_OPTIMAL, S::FRAGMENT_SHADER | S::COMPUTE_SHADER, A::SHADER_SAMPLED_READ)
            }
            Self::SampledWithTasks => (
                L::SHADER_READ_ONLY_OPTIMAL,
                S::FRAGMENT_SHADER | S::COMPUTE_SHADER | S::TASK_SHADER_EXT,
                A::SHADER_SAMPLED_READ,
            ),
            Self::StorageWrite => (L::GENERAL, S::COMPUTE_SHADER, A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE),
            Self::FragmentStorageWrite => {
                (L::GENERAL, S::FRAGMENT_SHADER, A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE)
            }
            Self::TransferSrc => (L::TRANSFER_SRC_OPTIMAL, S::ALL_TRANSFER, A::TRANSFER_READ),
            Self::TransferDst => (L::TRANSFER_DST_OPTIMAL, S::ALL_TRANSFER, A::TRANSFER_WRITE),
            Self::Present => (L::PRESENT_SRC_KHR, S::NONE, A::NONE),
        };
        State { layout, stage, access }
    }
}

impl BufferAccess {
    fn state(self) -> (vk::PipelineStageFlags2, vk::AccessFlags2) {
        use vk::{AccessFlags2 as A, PipelineStageFlags2 as S};
        match self {
            Self::TransferRead => (S::ALL_TRANSFER, A::TRANSFER_READ),
            Self::TransferWrite => (S::ALL_TRANSFER, A::TRANSFER_WRITE),
            Self::ComputeWrite => (S::COMPUTE_SHADER, A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE),
            Self::ComputeRead => (S::COMPUTE_SHADER, A::SHADER_STORAGE_READ),
            Self::IndirectRead => (S::DRAW_INDIRECT, A::INDIRECT_COMMAND_READ),
            Self::ShaderRead => (S::VERTEX_SHADER | S::FRAGMENT_SHADER | S::COMPUTE_SHADER, A::SHADER_STORAGE_READ),
            Self::MeshShaderRead => (S::TASK_SHADER_EXT | S::MESH_SHADER_EXT, A::SHADER_STORAGE_READ),
            Self::TaskWrite => (S::TASK_SHADER_EXT, A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE),
            Self::HostRead => (S::HOST, A::HOST_READ),
        }
    }
}

fn writes(access: vk::AccessFlags2) -> bool {
    access.intersects(
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE
            | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
            | vk::AccessFlags2::SHADER_STORAGE_WRITE
            | vk::AccessFlags2::TRANSFER_WRITE,
    )
}

/// A graph-owned image that lives across frames.
struct Transient {
    image: Image,
    state: State,
    /// Keep contents between frames (history buffers, last frame's depth pyramid).
    persistent: bool,
    texture: TextureId,
    storage: Vec<StorageImageId>,
}

/// The parts of the graph that persist between frames.
pub struct RenderGraph {
    transients: HashMap<String, Transient>,
    timer: GpuTimer,
}

impl RenderGraph {
    pub fn new(gpu: &Gpu, slots: usize) -> Result<Self> {
        Ok(Self { transients: HashMap::new(), timer: GpuTimer::new(gpu, slots)? })
    }

    /// Starts recording a frame's passes. Call after the slot's fence wait.
    pub fn begin<'a>(&'a mut self, gpu: &Gpu, slot: usize) -> FrameGraph<'a> {
        self.timer.begin_frame(gpu, slot);
        FrameGraph {
            graph: self,
            slot,
            images: Vec::new(),
            buffers: Vec::new(),
            passes: Vec::new(),
            prefix: String::new(),
        }
    }

    /// Milliseconds per pass from the latest completed frame.
    pub fn timings(&self) -> &[(String, f32)] {
        self.timer.results()
    }

    /// The last measured frame's GPU time, first pass to last, which its
    /// passes' times added up are not, as passes overlap and wait.
    pub fn frame_ms(&self) -> f32 {
        self.timer.frame_ms()
    }

    /// Lets go of the kept image `name`, as of a UI panel that has gone.
    /// The caller makes sure the GPU is done with it.
    pub fn release(&mut self, gpu: &Gpu, bindless: &mut Bindless, name: &str) {
        if let Some(transient) = self.transients.remove(name) {
            release(gpu, bindless, transient);
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu, bindless: &mut Bindless) {
        for (_, transient) in self.transients.drain() {
            release(gpu, bindless, transient);
        }
        self.timer.destroy(gpu);
    }
}

fn release(gpu: &Gpu, bindless: &mut Bindless, transient: Transient) {
    bindless.remove_texture(transient.texture);
    for id in transient.storage {
        bindless.remove_storage_image(id);
    }
    gpu.destroy_image(transient.image);
}

enum GraphImage {
    Imported { name: String, image: vk::Image, view: vk::ImageView, desc: ImageDesc, state: State },
    Transient { name: String, first_use: bool },
}

/// A pass recorded so far, as the frame debugger lists it: where it is among
/// the graph's passes, for [`FrameGraph::insert_pass`], its name, and the
/// images it uses.
pub struct RecordedPass {
    pub at: usize,
    pub name: String,
    pub images: Vec<(ImageId, ImageAccess)>,
}

/// Buffers are reached through device addresses, so their barriers are
/// global memory barriers and only the last access matters.
struct GraphBuffer {
    stage: vk::PipelineStageFlags2,
    access: vk::AccessFlags2,
}

type Record<'a> = Box<dyn FnOnce(&PassContext) + 'a>;

struct Pass<'a> {
    name: String,
    images: Vec<(ImageId, ImageAccess)>,
    buffers: Vec<(BufferId, BufferAccess)>,
    record: Option<Record<'a>>,
}

/// One frame's passes, recorded into a command buffer by [`execute`](Self::execute).
pub struct FrameGraph<'a> {
    graph: &'a mut RenderGraph,
    slot: usize,
    images: Vec<GraphImage>,
    buffers: Vec<GraphBuffer>,
    passes: Vec<Pass<'a>>,
    /// Put before the name of each image the graph keeps from now on, so a
    /// second 3D view's images are its own rather than the first's.
    prefix: String,
}

/// What a pass sees while recording.
pub struct PassContext<'c> {
    pub gpu: &'c Gpu,
    pub cmd: vk::CommandBuffer,
    pub bindless: &'c Bindless,
    images: &'c [(vk::Image, vk::ImageView, ImageDesc, vk::ImageView)],
}

impl PassContext<'_> {
    pub fn image(&self, id: ImageId) -> vk::Image {
        self.images[id.0].0
    }

    pub fn view(&self, id: ImageId) -> vk::ImageView {
        self.images[id.0].1
    }

    /// A view of an image's first mip alone, to render into one with mips.
    pub fn first_mip(&self, id: ImageId) -> vk::ImageView {
        self.images[id.0].3
    }

    pub fn extent(&self, id: ImageId) -> vk::Extent2D {
        self.images[id.0].2.extent()
    }

    /// Sets a full-target viewport and scissor.
    pub fn set_viewport(&self, extent: vk::Extent2D) {
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D { offset: vk::Offset2D::default(), extent };
        unsafe {
            self.gpu.device.cmd_set_viewport(self.cmd, 0, &[viewport]);
            self.gpu.device.cmd_set_scissor(self.cmd, 0, &[scissor]);
        }
    }
}

impl<'a> FrameGraph<'a> {
    /// Brings an externally owned image into the graph. `current` is how it
    /// was last used (`Undefined` to discard its contents).
    pub fn import_image(
        &mut self,
        name: &str,
        (image, view): (vk::Image, vk::ImageView),
        desc: ImageDesc,
        current: ImageAccess,
    ) -> ImageId {
        let name = name.to_string();
        self.images.push(GraphImage::Imported { name, image, view, desc, state: current.state() });
        ImageId(self.images.len() - 1)
    }

    pub fn image_name(&self, id: ImageId) -> &str {
        match &self.images[id.0] {
            GraphImage::Imported { name, .. } | GraphImage::Transient { name, .. } => name,
        }
    }

    pub fn image_desc(&self, id: ImageId) -> ImageDesc {
        match &self.images[id.0] {
            GraphImage::Imported { desc, .. } => *desc,
            GraphImage::Transient { name, .. } => self.graph.transients[name].image.desc,
        }
    }

    /// The passes recorded so far that do work, in order.
    pub fn recorded(&self) -> Vec<RecordedPass> {
        let passes = self.passes.iter().enumerate().filter(|(_, pass)| pass.record.is_some());
        passes.map(|(at, pass)| RecordedPass { at, name: pass.name.clone(), images: pass.images.clone() }).collect()
    }

    /// A graph-owned image whose contents are discarded every frame.
    pub fn create_image(&mut self, gpu: &Gpu, bindless: &mut Bindless, name: &str, desc: ImageDesc) -> Result<ImageId> {
        self.transient(gpu, bindless, name, desc, false)
    }

    /// A graph-owned image every 3D view uses in turn, as the shadow atlas:
    /// one image whatever the views' prefix, so views do not each keep one.
    /// Its size must not depend on the view's.
    pub fn shared_image(&mut self, gpu: &Gpu, bindless: &mut Bindless, name: &str, desc: ImageDesc) -> Result<ImageId> {
        let prefix = std::mem::take(&mut self.prefix);
        let image = self.transient(gpu, bindless, name, desc, false);
        self.prefix = prefix;
        image
    }

    /// A graph-owned image whose contents carry over to the next frame.
    pub fn history_image(
        &mut self,
        gpu: &Gpu,
        bindless: &mut Bindless,
        name: &str,
        desc: ImageDesc,
    ) -> Result<ImageId> {
        self.transient(gpu, bindless, name, desc, true)
    }

    /// Names the images kept from here on with `prefix` before their own
    /// names, as each 3D view after the first does, until set again.
    pub fn set_prefix(&mut self, prefix: &str) {
        self.prefix = prefix.to_string();
    }

    fn transient(
        &mut self,
        gpu: &Gpu,
        bindless: &mut Bindless,
        name: &str,
        desc: ImageDesc,
        persistent: bool,
    ) -> Result<ImageId> {
        let prefixed = format!("{}{name}", self.prefix);
        let name = prefixed.as_str();
        let desc = ImageDesc { usage: desc.usage | vk::ImageUsageFlags::TRANSFER_SRC, ..desc };
        let stale = self.graph.transients.get(name).is_some_and(|t| t.image.desc != desc);
        if stale {
            // A resize: rare enough that waiting for the GPU is simpler than deferring.
            gpu.wait_idle();
            let old = self.graph.transients.remove(name).expect("checked above");
            release(gpu, bindless, old);
        }
        if !self.graph.transients.contains_key(name) {
            let image = gpu.create_image(name, desc)?;
            let texture = if desc.usage.contains(vk::ImageUsageFlags::STORAGE) {
                bindless.add_texture_in(gpu, image.view, vk::ImageLayout::GENERAL)
            } else {
                bindless.add_texture(gpu, image.view)
            };
            let storage = image.mip_views.iter().map(|&view| bindless.add_storage_image(gpu, view)).collect();
            let storage = if image.mip_views.is_empty() && desc.usage.contains(vk::ImageUsageFlags::STORAGE) {
                vec![bindless.add_storage_image(gpu, image.view)]
            } else {
                storage
            };
            let transient = Transient { image, state: UNTOUCHED, persistent, texture, storage };
            self.graph.transients.insert(name.to_string(), transient);
        }
        self.images.push(GraphImage::Transient { name: name.to_string(), first_use: true });
        Ok(ImageId(self.images.len() - 1))
    }

    /// Whether a graph-owned image has not been used yet, as a kept one has
    /// not in the frame it is made: for what is drawn into it only once.
    pub fn is_new(&self, id: ImageId) -> bool {
        match &self.images[id.0] {
            GraphImage::Transient { name, .. } => self.graph.transients[name].state == UNTOUCHED,
            GraphImage::Imported { .. } => false,
        }
    }

    /// The bindless index for sampling a graph-owned image.
    pub fn texture(&self, id: ImageId) -> TextureId {
        match &self.images[id.0] {
            GraphImage::Transient { name, .. } => self.graph.transients[name].texture,
            GraphImage::Imported { .. } => panic!("imported images are not registered for sampling"),
        }
    }

    /// The bindless index for writing one mip of a graph-owned storage image.
    pub fn storage(&self, id: ImageId, mip: usize) -> StorageImageId {
        match &self.images[id.0] {
            GraphImage::Transient { name, .. } => self.graph.transients[name].storage[mip],
            GraphImage::Imported { .. } => panic!("imported images are not registered as storage"),
        }
    }

    pub fn import_buffer(&mut self, _buffer: &Buffer) -> BufferId {
        self.buffers
            .push(GraphBuffer { stage: vk::PipelineStageFlags2::ALL_COMMANDS, access: vk::AccessFlags2::MEMORY_WRITE });
        BufferId(self.buffers.len() - 1)
    }

    pub fn add_pass(
        &mut self,
        name: &str,
        images: &[(ImageId, ImageAccess)],
        buffers: &[(BufferId, BufferAccess)],
        record: impl FnOnce(&PassContext) + 'a,
    ) {
        self.passes.push(Pass {
            name: name.to_string(),
            images: images.to_vec(),
            buffers: buffers.to_vec(),
            record: Some(Box::new(record)),
        });
    }

    /// Adds a pass at `at` among the passes recorded so far, before the one
    /// there, as the frame debugger adds its copy after the pass it looks at.
    pub fn insert_pass(
        &mut self,
        at: usize,
        name: &str,
        images: &[(ImageId, ImageAccess)],
        record: impl FnOnce(&PassContext) + 'a,
    ) {
        let pass = Pass {
            name: name.to_string(),
            images: images.to_vec(),
            buffers: Vec::new(),
            record: Some(Box::new(record)),
        };
        self.passes.insert(at.min(self.passes.len()), pass);
    }

    /// Moves an image into a final state (e.g. `Present`) with no pass.
    pub fn transition(&mut self, id: ImageId, access: ImageAccess) {
        self.passes.push(Pass { name: String::new(), images: vec![(id, access)], buffers: Vec::new(), record: None });
    }

    /// Orders earlier writes to a buffer before a later use with no pass,
    /// such as making GPU-written counters visible to the CPU.
    pub fn sync_buffer(&mut self, id: BufferId, access: BufferAccess) {
        self.passes.push(Pass { name: String::new(), images: Vec::new(), buffers: vec![(id, access)], record: None });
    }

    /// Records every pass with its barriers, labels and timers.
    pub fn execute(mut self, gpu: &Gpu, cmd: vk::CommandBuffer, bindless: &Bindless) {
        let resolved = self.resolve();
        let passes = std::mem::take(&mut self.passes);
        for pass in passes {
            self.barriers(gpu, cmd, &pass, &resolved);
            let Some(record) = pass.record else { continue };
            gpu.begin_label(cmd, &pass.name, [0.55, 0.36, 0.96, 1.0]);
            let scope = self.graph.timer.begin(gpu, cmd, self.slot, &pass.name);
            record(&PassContext { gpu, cmd, bindless, images: &resolved });
            self.graph.timer.end(gpu, cmd, self.slot, scope);
            gpu.end_label(cmd);
        }
    }

    fn resolve(&self) -> Vec<(vk::Image, vk::ImageView, ImageDesc, vk::ImageView)> {
        self.images
            .iter()
            .map(|image| match image {
                GraphImage::Imported { image, view, desc, .. } => (*image, *view, *desc, *view),
                GraphImage::Transient { name, .. } => {
                    let t = &self.graph.transients[name];
                    let first = t.image.mip_views.first().copied().unwrap_or(t.image.view);
                    (t.image.handle, t.image.view, t.image.desc, first)
                }
            })
            .collect()
    }

    fn barriers(
        &mut self,
        gpu: &Gpu,
        cmd: vk::CommandBuffer,
        pass: &Pass,
        resolved: &[(vk::Image, vk::ImageView, ImageDesc, vk::ImageView)],
    ) {
        let mut image_barriers = Vec::new();
        for &(id, access) in &pass.images {
            let (old, discard) = self.image_state(id);
            let mut new = access.state();
            // Images compute writes stay in GENERAL, so sampling them needs no transition.
            let sampled = matches!(access, ImageAccess::Sampled | ImageAccess::SampledWithTasks);
            if resolved[id.0].2.usage.contains(vk::ImageUsageFlags::STORAGE) && sampled {
                new.layout = vk::ImageLayout::GENERAL;
            }
            let hazard = old.layout != new.layout || writes(old.access) || writes(new.access);
            if hazard {
                let (image, _, desc, _) = resolved[id.0];
                let range = vk::ImageSubresourceRange::default()
                    .aspect_mask(desc.aspect())
                    .level_count(vk::REMAINING_MIP_LEVELS)
                    .layer_count(1);
                image_barriers.push(
                    vk::ImageMemoryBarrier2::default()
                        .image(image)
                        .old_layout(if discard { vk::ImageLayout::UNDEFINED } else { old.layout })
                        .new_layout(new.layout)
                        .src_stage_mask(old.stage)
                        .src_access_mask(old.access)
                        .dst_stage_mask(new.stage)
                        .dst_access_mask(new.access)
                        .subresource_range(range),
                );
            }
            let merged = if hazard {
                new
            } else {
                // Successive reads in one layout: later writes must wait for all of them.
                State { layout: new.layout, stage: old.stage | new.stage, access: old.access | new.access }
            };
            self.set_image_state(id, merged);
        }

        let mut memory = None::<(vk::PipelineStageFlags2, vk::AccessFlags2, vk::PipelineStageFlags2, vk::AccessFlags2)>;
        for &(id, access) in &pass.buffers {
            let buffer = &mut self.buffers[id.0];
            let (stage, flags) = access.state();
            if writes(buffer.access) || writes(flags) {
                let m = memory.get_or_insert((vk::PipelineStageFlags2::NONE, vk::AccessFlags2::NONE, stage, flags));
                m.0 |= buffer.stage;
                m.1 |= buffer.access;
                m.2 |= stage;
                m.3 |= flags;
                (buffer.stage, buffer.access) = (stage, flags);
            } else {
                buffer.stage |= stage;
                buffer.access |= flags;
            }
        }

        let memory_barriers: Vec<_> = memory
            .map(|(src_stage, src_access, dst_stage, dst_access)| {
                vk::MemoryBarrier2::default()
                    .src_stage_mask(src_stage)
                    .src_access_mask(src_access)
                    .dst_stage_mask(dst_stage)
                    .dst_access_mask(dst_access)
            })
            .into_iter()
            .collect();
        if image_barriers.is_empty() && memory_barriers.is_empty() {
            return;
        }
        let dependency =
            vk::DependencyInfo::default().image_memory_barriers(&image_barriers).memory_barriers(&memory_barriers);
        unsafe { gpu.device.cmd_pipeline_barrier2(cmd, &dependency) };
    }

    /// The image's current state, and whether its contents may be discarded.
    fn image_state(&mut self, id: ImageId) -> (State, bool) {
        match &mut self.images[id.0] {
            GraphImage::Imported { state, .. } => (*state, state.layout == vk::ImageLayout::UNDEFINED),
            GraphImage::Transient { name, first_use } => {
                let transient = &self.graph.transients[name.as_str()];
                let discard = *first_use && !transient.persistent;
                *first_use = false;
                (transient.state, discard || transient.state.layout == vk::ImageLayout::UNDEFINED)
            }
        }
    }

    fn set_image_state(&mut self, id: ImageId, new: State) {
        match &mut self.images[id.0] {
            GraphImage::Imported { state, .. } => *state = new,
            GraphImage::Transient { name, .. } => {
                self.graph.transients.get_mut(name.as_str()).expect("transient exists").state = new;
            }
        }
    }
}
