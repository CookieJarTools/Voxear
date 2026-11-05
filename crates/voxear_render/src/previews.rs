//! Previews: an object drawn on its own into a small image, for the editor's
//! Project panel, reference fields and Inspector, and for games (inventory
//! icons, menus). A preview is the meshes and materials of one object, with
//! where each sits in it, and [`PreviewOptions`]: its size, the background
//! (any colour, or clear), how the object is turned, moved and zoomed, the
//! field of view, and the light.
//!
//! Previews are drawn into cells of one atlas texture, a few each frame, in
//! the frame's own graph before the UI, so one asked for shows a frame
//! later. Each is drawn at twice its size with its own depth, then
//! [`resolve`](crate) averages each pixel's block, tonemaps it, and lays it
//! over the background. The shader is `shaders/preview.slang`. A drawn
//! preview stays in the atlas until freed; when the atlas is full, the
//! preview looked at longest ago gives up its cell and is drawn again when
//! next looked at.

use std::{
    collections::{HashMap, VecDeque},
    path::Path,
};

use anyhow::Result;
use voxear_core::math::{
    Mat3, Mat4, Quat, Vec2, Vec3, Vec4,
    camera::lh::{proj::vulkan, view::look_at_mat4},
};
use voxear_rhi::{Bindless, Buffer, Gpu, ImageDesc, MemoryLocation, TextureId, vk};

use crate::{
    MaterialHandle, MeshHandle,
    assets::Assets,
    gpu::{GpuBuffers, GpuFrame, GpuInstance, Push, cols},
    graph::{BufferAccess, FrameGraph, ImageAccess, ImageId},
    passes::{begin_rendering_in, begin_rendering_rect, bind},
    pipelines::{DEPTH_FORMAT, HDR_FORMAT, Pipelines},
};

/// The atlas previews are drawn into, at its largest, and its format: sRGB,
/// so a preview samples as linear colour, premultiplied by its alpha. It
/// starts at a quarter of that, and grows once when it fills.
pub const PREVIEW_ATLAS_SIZE: u32 = 2048;
const FIRST_ATLAS_SIZE: u32 = 1024;
pub const PREVIEW_FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;
/// The largest preview, in pixels; one asked bigger is drawn at this.
pub const MAX_PREVIEW_SIZE: u32 = 512;
/// The smallest cell; smaller previews take one this size.
const SMALLEST_CELL: u32 = 32;
/// How much bigger a preview is drawn than it shows, for smooth edges, at
/// most: the drawn image is never larger than [`MAX_DRAWN`], so the
/// largest previews get twice and small ones four times. It is as large as
/// the largest preview drawn so far needs, from [`FIRST_DRAWN`].
const SUPERSAMPLE: u32 = 4;
const MAX_DRAWN: u32 = MAX_PREVIEW_SIZE * 2;
const FIRST_DRAWN: u32 = 256;
/// Previews drawn in one frame at most; the rest wait for the next.
const PER_FRAME: usize = 8;
/// Parts across one frame's previews; a preview with more is left out.
const MAX_PARTS: usize = 2048;
/// How much room a framed object leaves around itself.
const MARGIN: f32 = 1.08;

/// A preview, as [`Renderer::add_preview`](crate::Renderer::add_preview)
/// returns it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PreviewId(pub u32);

/// One mesh of what a preview shows, with its material and where it sits
/// in the object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewPart {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub transform: Mat4,
}

/// How a preview looks. The defaults suit an icon: a three-quarter view
/// from a little above, framed to fit, on a clear background.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewOptions {
    /// Width and height in pixels, up to [`MAX_PREVIEW_SIZE`].
    pub size: u32,
    /// Behind the object: sRGB colour and alpha, where an alpha of 0 leaves
    /// it clear.
    pub background: [f32; 4],
    /// Turns the object before it is framed.
    pub rotation: Quat,
    /// Moves the framed object across the image, as a share of its width
    /// and height: `(0.25, 0.0)` is a quarter to the right.
    pub offset: Vec2,
    /// How large the object is against the frame: 1 fits it, 2 is twice as
    /// close.
    pub zoom: f32,
    /// The camera's field of view, in degrees. Small looks flat, like an
    /// orthographic icon; large looks close.
    pub fov_degrees: f32,
    /// Where the key light comes from, towards the object, and its colour
    /// and strength. A fill light from the camera's side follows it.
    pub light_from: Vec3,
    pub light_color: Vec3,
    pub light_intensity: f32,
    /// The sky's light from every side, as a share of a scene's.
    pub ambient: f32,
    /// Brightens or darkens the whole image, as a camera's exposure.
    pub exposure: f32,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        Self {
            size: 128,
            background: [0.0; 4],
            rotation: Quat::from_rotation_y(-35f32.to_radians()),
            offset: Vec2::ZERO,
            zoom: 1.0,
            fov_degrees: 30.0,
            light_from: Vec3::new(-0.5, 0.8, -0.6),
            light_color: Vec3::ONE,
            light_intensity: 3.0,
            ambient: 1.0,
            exposure: 1.0,
        }
    }
}

/// A preview's pixels, read back from the GPU to save or use elsewhere:
/// `size` by `size` RGBA, in sRGB with straight alpha, from the top row
/// down.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewPixels {
    pub size: u32,
    pub rgba: Vec<u8>,
}

impl PreviewPixels {
    /// Writes the pixels to a PNG file, its alpha kept.
    pub fn save_png(&self, path: &Path) -> Result<()> {
        image::save_buffer(path, &self.rgba, self.size, self.size, image::ExtendedColorType::Rgba8)?;
        Ok(())
    }
}

/// Where a drawn preview is, to show it: the atlas's bindless texture and
/// the preview's rectangle in it, as fractions (left, top, right, bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewImage {
    pub texture: TextureId,
    pub uv: [f32; 4],
    pub size: u32,
}

struct Preview {
    parts: Vec<PreviewPart>,
    options: PreviewOptions,
    /// Its cell in the atlas, once it has one, and whether it is drawn there.
    cell: Option<Cell>,
    drawn: bool,
    /// The frame it was last looked at, for giving up cells when the atlas
    /// is full.
    seen: u64,
}

/// A square of the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    x: u32,
    y: u32,
    size: u32,
}

/// What one frame draws for one preview.
#[derive(Clone, Copy)]
struct Job {
    cell: Cell,
    /// The preview's own size, which may be less than its cell's: it is
    /// drawn and framed in that much, from the cell's corner.
    shown: u32,
    /// The parts' instances, by their place in the frame's list.
    parts: (usize, usize),
    frame: usize,
    background: [f32; 4],
    exposure: f32,
}

/// Each frame slot's buffers: a frame per preview, the instances, and the
/// pointers shaders read.
struct SlotBuffers {
    frames: Buffer,
    instances: Buffer,
    pointers: Buffer,
}

#[derive(Default)]
pub(crate) struct Previews {
    next: u32,
    previews: HashMap<PreviewId, Preview>,
    /// Previews waiting to be drawn, oldest first.
    queue: VecDeque<PreviewId>,
    cells: Cells,
    texture: Option<TextureId>,
    slots: Vec<Option<SlotBuffers>>,
    frame: u64,
    /// Previews whose pixels were asked for, those being copied back in a
    /// frame slot, and those read.
    reads: Vec<PreviewId>,
    in_flight: Vec<Read>,
    read: HashMap<PreviewId, PreviewPixels>,
    /// The atlas filled, so it grows at the start of the next frame.
    grow: bool,
    /// How big the image previews are drawn in is, which only grows.
    drawn: u32,
}

/// A preview's pixels being copied into a buffer the CPU can read, in the
/// frame slot that copies them: read once that slot comes round again.
struct Read {
    id: PreviewId,
    slot: usize,
    buffer: Buffer,
    size: u32,
}

impl Previews {
    pub fn add(&mut self, parts: Vec<PreviewPart>, options: PreviewOptions) -> PreviewId {
        let id = PreviewId(self.next);
        self.next += 1;
        self.previews.insert(id, Preview { parts, options, cell: None, drawn: false, seen: self.frame });
        self.queue.push_back(id);
        id
    }

    /// Changes what a preview shows or how, to draw it again. What it showed
    /// stays in its cell until then: hidden while it waited, a preview being
    /// turned by a drag, drawn again each frame, showed black until let go.
    pub fn set(&mut self, id: PreviewId, parts: Option<Vec<PreviewPart>>, options: PreviewOptions) {
        let Some(preview) = self.previews.get_mut(&id) else { return };
        if let Some(parts) = parts {
            preview.parts = parts;
        }
        if preview.cell.is_some_and(|cell| cell.size != cell_size(options.size)) {
            self.cells.free(preview.cell.take().expect("checked"));
            preview.drawn = false;
        }
        preview.options = options;
        if !self.queue.contains(&id) {
            self.queue.push_back(id);
        }
    }

    pub fn remove(&mut self, id: PreviewId) {
        if let Some(cell) = self.previews.remove(&id).and_then(|p| p.cell) {
            self.cells.free(cell);
        }
        self.queue.retain(|queued| *queued != id);
    }

    /// Where a drawn preview is, or `None` while it waits. Looking at one
    /// keeps it in the atlas; one that gave its cell up is drawn again.
    pub fn image(&mut self, id: PreviewId) -> Option<PreviewImage> {
        let frame = self.frame;
        let preview = self.previews.get_mut(&id)?;
        preview.seen = frame;
        if !preview.drawn {
            if !self.queue.contains(&id) {
                self.queue.push_back(id);
            }
            return None;
        }
        let (cell, texture) = (preview.cell?, self.texture?);
        let atlas = self.cells.size as f32;
        let shown = preview.options.size.min(MAX_PREVIEW_SIZE) as f32;
        let [x, y] = [cell.x as f32, cell.y as f32];
        Some(PreviewImage {
            texture,
            uv: [x / atlas, y / atlas, (x + shown) / atlas, (y + shown) / atlas],
            size: shown as u32,
        })
    }

    /// Asks for a preview's pixels, to read back once it is drawn; see
    /// [`pixels`](Self::pixels).
    pub fn read(&mut self, id: PreviewId) {
        if self.previews.contains_key(&id) && !self.reads.contains(&id) {
            self.reads.push(id);
        }
    }

    /// A preview's pixels, once read back, a few frames after they were
    /// asked for. Taken, so they are handed over once.
    pub fn pixels(&mut self, id: PreviewId) -> Option<PreviewPixels> {
        self.read.remove(&id)
    }

    /// Adds this frame's preview passes to `fg`, drawing those waiting, and
    /// returns the atlas, for the UI to sample, when there is one.
    pub fn record(
        &mut self,
        fg: &mut FrameGraph<'_>,
        (gpu, bindless): (&Gpu, &mut Bindless),
        (pipelines, assets): (&Pipelines, &Assets),
        slot: usize,
    ) -> Result<Option<ImageId>> {
        // Previews looked at this frame, before it went up, keep their cells.
        self.frame += 1;
        self.finish_reads(gpu, slot);
        if self.previews.is_empty() {
            return Ok(None);
        }
        let usage =
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC;
        if std::mem::take(&mut self.grow) {
            self.grow_atlas();
        }
        let desc = ImageDesc::new(self.cells.size, self.cells.size, PREVIEW_FORMAT, usage);
        let atlas = fg.history_image(gpu, bindless, "preview atlas", desc)?;
        self.texture = Some(fg.texture(atlas));
        let jobs = self.plan(assets);
        if jobs.0.is_empty() {
            self.record_reads(fg, gpu, atlas, slot)?;
            return Ok(Some(atlas));
        }
        let buffers = self.slot_buffers(gpu, slot)?;
        buffers.frames.write(0, &jobs.1);
        buffers.instances.write(0, &jobs.2);
        let pointers = GpuBuffers {
            instances: buffers.instances.address,
            meshes: assets.meshes.address,
            vertices: assets.vertices.address,
            materials: assets.materials.address,
            material_properties: assets.material_properties.address,
            ..Default::default()
        };
        buffers.pointers.write(0, &[pointers]);
        let (frames, pointers) = (buffers.frames.address, buffers.pointers.address);

        let needed = jobs.0.iter().map(|job| job.shown * scale_of(job.cell)).max().unwrap_or(0);
        self.drawn = self.drawn.max(FIRST_DRAWN).max(needed.next_power_of_two());
        let drawn_size = self.drawn;
        let drawn_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let color_desc = ImageDesc::new(drawn_size, drawn_size, HDR_FORMAT, drawn_usage);
        let color = fg.create_image(gpu, bindless, "preview color", color_desc)?;
        let depth_usage = vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
        let depth_desc = ImageDesc::new(drawn_size, drawn_size, DEPTH_FORMAT, depth_usage);
        let depth = fg.create_image(gpu, bindless, "preview depth", depth_desc)?;
        let color_texture = fg.texture(color);
        let (draw_pipeline, resolve_pipeline, index_buffer) =
            (pipelines.preview, pipelines.preview_resolve, assets.indices.handle);
        let draws = jobs.3;
        for job in jobs.0 {
            let frame = frames + (job.frame * size_of::<GpuFrame>()) as u64;
            let push = Push { frame, buffers: pointers, ..Default::default() };
            let parts: Vec<(usize, (u32, u32))> = draws[job.parts.0..job.parts.1].to_vec();
            let scale = scale_of(job.cell);
            let drawn = job.shown * scale;
            let images = [(color, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthAttachment)];
            fg.add_pass("Preview", &images, &[], move |ctx| {
                let area = vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: vk::Extent2D { width: drawn, height: drawn },
                };
                let clear = vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } };
                let color = [vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.view(color))
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .clear_value(clear)
                    .store_op(vk::AttachmentStoreOp::STORE)];
                let far = vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 0.0, stencil: 0 } };
                let depth = vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.view(depth))
                    .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .clear_value(far)
                    .store_op(vk::AttachmentStoreOp::DONT_CARE);
                begin_rendering_rect(ctx, area, &color, Some(&depth));
                unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, index_buffer, 0, vk::IndexType::UINT32) };
                for (instance, (first, count)) in &parts {
                    bind(ctx, draw_pipeline, vk::PipelineBindPoint::GRAPHICS, &Push { list: *instance as u32, ..push });
                    unsafe { ctx.gpu.device.cmd_draw_indexed(ctx.cmd, *count, 1, *first, 0, 0) };
                }
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            });
            let resolve = Push {
                texture: color_texture.0,
                size: [job.cell.x, job.cell.y],
                level: scale,
                max_draws: drawn_size,
                output: job.exposure.to_bits(),
                textures: job.background.map(f32::to_bits),
                ..Default::default()
            };
            let (cell, shown) = (job.cell, job.shown);
            let images = [(color, ImageAccess::Sampled), (atlas, ImageAccess::ColorAttachment)];
            fg.add_pass("Preview resolve", &images, &[], move |ctx| {
                let rect = vk::Rect2D {
                    offset: vk::Offset2D { x: cell.x as i32, y: cell.y as i32 },
                    extent: vk::Extent2D { width: shown, height: shown },
                };
                let target = [vk::RenderingAttachmentInfo::default()
                    .image_view(ctx.view(atlas))
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::LOAD)
                    .store_op(vk::AttachmentStoreOp::STORE)];
                begin_rendering_in(ctx, rect, &target);
                bind(ctx, resolve_pipeline, vk::PipelineBindPoint::GRAPHICS, &resolve);
                unsafe {
                    ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
                    ctx.gpu.device.cmd_end_rendering(ctx.cmd);
                }
            });
        }
        self.record_reads(fg, gpu, atlas, slot)?;
        Ok(Some(atlas))
    }

    /// Takes up to [`PER_FRAME`] previews from the queue that have a cell or
    /// can get one, and works out what each draws: the jobs, a frame for
    /// each, the instances, and each instance's first LOD.
    #[allow(clippy::type_complexity)]
    fn plan(&mut self, assets: &Assets) -> (Vec<Job>, Vec<GpuFrame>, Vec<GpuInstance>, Vec<(usize, (u32, u32))>) {
        let (mut jobs, mut frames, mut instances, mut draws) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut waiting = VecDeque::new();
        while jobs.len() < PER_FRAME {
            let Some(id) = self.queue.pop_front() else { break };
            let Some(cell) = self.cell_for(id) else {
                waiting.push_back(id);
                break;
            };
            let preview = self.previews.get_mut(&id).expect("queued previews exist");
            let parts: Vec<(&PreviewPart, (u32, u32))> =
                preview.parts.iter().filter_map(|part| Some((part, assets.first_lod(part.mesh)?))).collect();
            if instances.len() + parts.len() > MAX_PARTS {
                waiting.push_back(id);
                break;
            }
            let options = preview.options;
            let start = instances.len();
            for (part, lod) in &parts {
                let model = Mat4::from_quat(options.rotation) * part.transform;
                let normal = Mat3::from_mat4(model).inverse().transpose();
                instances.push(GpuInstance {
                    model: cols(model),
                    prev_model: cols(model),
                    normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                    mesh: part.mesh.0,
                    material: part.material.0,
                    flags: 0,
                    lod_bias: 0.0,
                    lod_screen: [[0.0; 4]; 2],
                    fade: [0.0; 4],
                });
                draws.push((draws.len(), *lod));
            }
            let bounds = framing(assets, &preview.parts, options.rotation);
            frames.push(camera_frame(bounds, &options));
            jobs.push(Job {
                cell,
                shown: options.size.clamp(1, MAX_PREVIEW_SIZE),
                parts: (start, instances.len()),
                frame: frames.len() - 1,
                background: linear(options.background),
                exposure: options.exposure,
            });
            preview.drawn = true;
        }
        waiting.extend(self.queue.drain(..));
        self.queue = waiting;
        (jobs, frames, instances, draws)
    }

    /// A preview's cell, finding it one if it has none: a free one, or else
    /// the cell of the preview looked at longest ago.
    fn cell_for(&mut self, id: PreviewId) -> Option<Cell> {
        let size = cell_size(self.previews.get(&id)?.options.size);
        if let Some(cell) = self.previews[&id].cell {
            return Some(cell);
        }
        loop {
            if let Some(cell) = self.cells.alloc(size) {
                // Drawn this frame, so nothing else takes the cell back before it shows.
                let frame = self.frame;
                let preview = self.previews.get_mut(&id).expect("checked");
                (preview.cell, preview.seen) = (Some(cell), frame);
                return Some(cell);
            }
            // A full atlas grows once, next frame, before cells are taken back.
            if self.cells.size < PREVIEW_ATLAS_SIZE {
                self.grow = true;
                return None;
            }
            let oldest = self
                .previews
                .iter()
                .filter(|(other, p)| **other != id && p.cell.is_some() && p.seen + 1 < self.frame)
                .min_by_key(|(_, p)| p.seen)
                .map(|(other, _)| *other)?;
            let preview = self.previews.get_mut(&oldest).expect("found above");
            preview.drawn = false;
            let cell = preview.cell.take().expect("filtered on it");
            self.cells.free(cell);
        }
    }

    /// Doubles the atlas: every preview takes a new cell in it and is drawn
    /// again over the next frames.
    fn grow_atlas(&mut self) {
        self.cells = Cells::new((self.cells.size * 2).min(PREVIEW_ATLAS_SIZE));
        for (id, preview) in &mut self.previews {
            (preview.cell, preview.drawn) = (None, false);
            if !self.queue.contains(id) {
                self.queue.push_back(*id);
            }
        }
    }

    fn slot_buffers(&mut self, gpu: &Gpu, slot: usize) -> Result<&SlotBuffers> {
        if self.slots.len() <= slot {
            self.slots.resize_with(slot + 1, || None);
        }
        if self.slots[slot].is_none() {
            let host = MemoryLocation::CpuToGpu;
            let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
            let frames = (PER_FRAME * size_of::<GpuFrame>()) as u64;
            let instances = (MAX_PARTS * size_of::<GpuInstance>()) as u64;
            self.slots[slot] = Some(SlotBuffers {
                frames: gpu.create_buffer(&format!("preview frames {slot}"), frames, storage, host)?,
                instances: gpu.create_buffer(&format!("preview instances {slot}"), instances, storage, host)?,
                pointers: gpu.create_buffer(
                    &format!("preview pointers {slot}"),
                    size_of::<GpuBuffers>() as u64,
                    storage,
                    host,
                )?,
            });
        }
        Ok(self.slots[slot].as_ref().expect("made above"))
    }

    /// Copies the previews whose pixels were asked for, once drawn, into
    /// buffers the CPU reads when this slot comes round again.
    fn record_reads(&mut self, fg: &mut FrameGraph<'_>, gpu: &Gpu, atlas: ImageId, slot: usize) -> Result<()> {
        for id in std::mem::take(&mut self.reads) {
            let Some(preview) = self.previews.get(&id) else { continue };
            let (Some(cell), true) = (preview.cell, preview.drawn) else {
                self.reads.push(id);
                continue;
            };
            let size = preview.options.size.min(MAX_PREVIEW_SIZE);
            let usage = vk::BufferUsageFlags::TRANSFER_DST;
            let bytes = u64::from(size * size * 4);
            let buffer =
                gpu.create_buffer(&format!("preview read {}", id.0), bytes, usage, MemoryLocation::GpuToCpu)?;
            let target = fg.import_buffer(&buffer);
            let handle = buffer.handle;
            let images = [(atlas, ImageAccess::TransferSrc)];
            fg.add_pass("Preview read", &images, &[(target, BufferAccess::TransferWrite)], move |ctx| {
                let layers =
                    vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).layer_count(1);
                let region = vk::BufferImageCopy::default()
                    .image_subresource(layers)
                    .image_offset(vk::Offset3D { x: cell.x as i32, y: cell.y as i32, z: 0 })
                    .image_extent(vk::Extent3D { width: size, height: size, depth: 1 });
                let layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
                unsafe {
                    ctx.gpu.device.cmd_copy_image_to_buffer(ctx.cmd, ctx.image(atlas), layout, handle, &[region])
                };
            });
            fg.sync_buffer(target, BufferAccess::HostRead);
            self.in_flight.push(Read { id, slot, buffer, size });
        }
        Ok(())
    }

    /// Reads the pixels copied when this slot last ran, whose frame the GPU
    /// has finished by now, turning them to straight alpha.
    fn finish_reads(&mut self, gpu: &Gpu, slot: usize) {
        let (done, waiting): (Vec<Read>, Vec<Read>) = self.in_flight.drain(..).partition(|read| read.slot == slot);
        self.in_flight = waiting;
        for read in done {
            let rgba = read.buffer.read()[..(read.size * read.size * 4) as usize]
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(straight)
                .collect();
            self.read.insert(read.id, PreviewPixels { size: read.size, rgba });
            gpu.destroy_buffer(read.buffer);
        }
    }

    /// The buffers, to let go of with the renderer.
    pub fn take_buffers(&mut self) -> Vec<Buffer> {
        let reads = self.in_flight.drain(..).map(|read| read.buffer);
        let slots = self.slots.drain(..).flatten().flat_map(|b| [b.frames, b.instances, b.pointers]);
        reads.chain(slots).collect()
    }
}

/// How many times bigger a cell's preview is drawn than it shows.
fn scale_of(cell: Cell) -> u32 {
    (MAX_DRAWN / cell.size).clamp(1, SUPERSAMPLE)
}

/// The cell a preview of `size` pixels takes: the next power of two.
fn cell_size(size: u32) -> u32 {
    size.clamp(SMALLEST_CELL, MAX_PREVIEW_SIZE).next_power_of_two()
}

/// The sphere around the turned parts' own bounding spheres, as a centre
/// and radius. Spheres frame tighter than boxes: a ball's box's corners lie
/// well outside it.
fn framing(assets: &Assets, parts: &[PreviewPart], rotation: Quat) -> (Vec3, f32) {
    let spheres: Vec<(Vec3, f32)> = parts
        .iter()
        .filter_map(|part| {
            let (center, radius) = assets.mesh_sphere(part.mesh)?;
            let model = Mat4::from_quat(rotation) * part.transform;
            let axes = [model.x_axis, model.y_axis, model.z_axis];
            let scale = axes.iter().map(|axis| axis.truncate().length()).fold(0.0, f32::max);
            Some((model.transform_point3(center), radius * scale))
        })
        .collect();
    if spheres.is_empty() {
        return (Vec3::ZERO, 1.0);
    }
    let (min, max) = spheres.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(min, max), (c, r)| {
        (min.min(c - Vec3::splat(*r)), max.max(c + Vec3::splat(*r)))
    });
    let center = (min + max) * 0.5;
    let radius = spheres.iter().map(|(c, r)| c.distance(center) + r).fold(0.0, f32::max);
    (center, radius.max(1e-3))
}

/// The camera and light for a preview: looking at the object from a little
/// above and in front, far enough back that its bounding sphere fits, then
/// zoomed and moved as the options say.
fn camera_frame((center, radius): (Vec3, f32), options: &PreviewOptions) -> GpuFrame {
    let fov = options.fov_degrees.clamp(1.0, 120.0).to_radians();
    // A little room around it, so it does not touch the edges.
    let distance = radius * MARGIN / (fov * 0.5).sin() / options.zoom.max(0.01);
    let toward = Vec3::new(0.0, 0.35, -1.0).normalize();
    let forward = -toward;
    let right = Vec3::Y.cross(forward).normalize();
    let up = forward.cross(right);
    // Moving the camera the other way moves the object across the image.
    let across = (fov * 0.5).tan() * distance * 2.0;
    let shift = right * (-options.offset.x * across) + up * (options.offset.y * across);
    let position = center + toward * distance + shift;
    let view = look_at_mat4(position, center + shift, Vec3::Y);
    let near = (distance - radius * 1.5).max(distance * 0.01).max(0.001);
    let projection = vulkan::perspective_infinite_reverse(fov, 1.0, near);
    let sky = crate::Sky::default();
    let light = options.light_from.try_normalize().unwrap_or(Vec3::Y);
    GpuFrame {
        view_proj: cols(projection * view),
        camera_position: position.to_array(),
        camera_forward: forward.to_array(),
        near,
        sun_direction: (-light).to_array(),
        sun_color: (options.light_color * options.light_intensity).to_array(),
        sky_zenith: sky.zenith.to_array(),
        sky_horizon: sky.horizon.to_array(),
        ground_color: sky.ground.to_array(),
        ambient: options.ambient,
        exposure: options.exposure,
        ..Default::default()
    }
}

/// A pixel of the atlas, premultiplied and sRGB-encoded, with its colour
/// divided back out of its alpha, as image files keep it.
fn straight(pixel: &[u8; 4]) -> [u8; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    let channel = |c: u8| {
        let c = f32::from(c) / 255.0;
        let linear = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        let unmixed = (linear / alpha).min(1.0);
        let encoded = if unmixed <= 0.0031308 { unmixed * 12.92 } else { 1.055 * unmixed.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0).round() as u8
    };
    [channel(pixel[0]), channel(pixel[1]), channel(pixel[2]), pixel[3]]
}

/// An sRGB colour with alpha, as linear, for the GPU.
fn linear([r, g, b, a]: [f32; 4]) -> [f32; 4] {
    let channel = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    [channel(r), channel(g), channel(b), a]
}

/// The atlas's squares, handed out as a quadtree: a square too big for what
/// is asked splits into four, and four free quarters join again.
struct Cells {
    /// The atlas's width and height.
    size: u32,
    /// Free squares by size, the whole atlas to begin with.
    free: HashMap<u32, Vec<(u32, u32)>>,
}

impl Default for Cells {
    fn default() -> Self {
        Self::new(FIRST_ATLAS_SIZE)
    }
}

impl Cells {
    fn new(size: u32) -> Self {
        Self { size, free: HashMap::from([(size, vec![(0, 0)])]) }
    }

    fn alloc(&mut self, size: u32) -> Option<Cell> {
        if let Some((x, y)) = self.free.get_mut(&size).and_then(Vec::pop) {
            return Some(Cell { x, y, size });
        }
        if size >= self.size {
            return None;
        }
        let bigger = self.alloc(size * 2)?;
        let quarters = [(size, 0), (0, size), (size, size)];
        let free = self.free.entry(size).or_default();
        free.extend(quarters.map(|(dx, dy)| (bigger.x + dx, bigger.y + dy)));
        Some(Cell { x: bigger.x, y: bigger.y, size })
    }

    fn free(&mut self, cell: Cell) {
        let parent = cell.size * 2;
        let (px, py) = (cell.x / parent * parent, cell.y / parent * parent);
        let siblings: Vec<(u32, u32)> = [(0, 0), (cell.size, 0), (0, cell.size), (cell.size, cell.size)]
            .map(|(dx, dy)| (px + dx, py + dy))
            .into_iter()
            .filter(|&at| at != (cell.x, cell.y))
            .collect();
        let free = self.free.entry(cell.size).or_default();
        let joined = cell.size < self.size && siblings.iter().all(|s| free.contains(s));
        if !joined {
            free.push((cell.x, cell.y));
            return;
        }
        free.retain(|s| !siblings.contains(s));
        self.free(Cell { x: px, y: py, size: parent });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_split_and_join_again() {
        let mut cells = Cells::new(PREVIEW_ATLAS_SIZE);
        let a = cells.alloc(128).unwrap();
        let b = cells.alloc(128).unwrap();
        assert_ne!((a.x, a.y), (b.x, b.y));
        let big = cells.alloc(512).unwrap();
        let overlap =
            |p: Cell, q: Cell| p.x < q.x + q.size && q.x < p.x + p.size && p.y < q.y + q.size && q.y < p.y + p.size;
        assert!(!overlap(a, big) && !overlap(b, big));
        cells.free(a);
        cells.free(b);
        cells.free(big);
        assert_eq!(cells.free.get(&PREVIEW_ATLAS_SIZE), Some(&vec![(0, 0)]), "everything joins back into the atlas");
        let all = (0..(PREVIEW_ATLAS_SIZE / 512).pow(2)).filter_map(|_| cells.alloc(512)).count();
        assert_eq!(all, 16);
        assert!(cells.alloc(32).is_none(), "full");
    }
}
