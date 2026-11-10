use std::ptr::NonNull;

use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::{
    MemoryLocation,
    vulkan::{Allocation, AllocationCreateDesc, AllocationScheme},
};

use crate::device::Gpu;

/// A buffer. Every buffer has a device address, so shaders reach buffers
/// through pointers in push constants rather than descriptor sets.
pub struct Buffer {
    pub handle: vk::Buffer,
    pub size: u64,
    pub address: vk::DeviceAddress,
    allocation: Option<Allocation>,
}

// Mapped pointers are only written from the thread that owns the renderer.
unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

/// An open batch of uploads and builds (see [`Gpu::batch`]).
#[must_use = "a batch is done when it is finished or dropped"]
pub struct GpuBatch<'a> {
    gpu: &'a Gpu,
    open: bool,
}

impl GpuBatch<'_> {
    /// Closes the batch, doing what it gathered when it is the outermost.
    pub fn finish(mut self) -> Result<()> {
        self.open = false;
        self.gpu.end_batch()
    }
}

impl Drop for GpuBatch<'_> {
    fn drop(&mut self) {
        if self.open
            && let Err(error) = self.gpu.end_batch()
        {
            tracing::error!("a batch of uploads failed: {error:#}");
        }
    }
}

/// Uploads and builds gathered while a batch is open (see [`Gpu::batch`]): how many batches are open, the bytes to upload and
/// where each run of them goes, and the bottom levels to build after.
#[derive(Default)]
pub(crate) struct Batch {
    pub open: u32,
    pub bytes: Vec<u8>,
    pub copies: Vec<(vk::Buffer, vk::BufferCopy)>,
    pub builds: Vec<crate::accel::BottomLevelBuild>,
}

impl Buffer {
    /// The CPU-visible mapping, for buffers created in `CpuToGpu` or
    /// `GpuToCpu` memory.
    pub fn mapped(&self) -> Option<NonNull<u8>> {
        self.allocation.as_ref()?.mapped_ptr().map(|p| p.cast())
    }

    /// Copies `data` into a mapped buffer at `offset`.
    pub fn write<T: bytemuck::Pod>(&self, offset: u64, data: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        assert!(offset + bytes.len() as u64 <= self.size, "write past the end of a buffer");
        let mapped = self.mapped().expect("buffer is not host visible");
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapped.as_ptr().add(offset as usize), bytes.len()) };
    }

    /// Reads a mapped buffer.
    pub fn read(&self) -> &[u8] {
        let mapped = self.mapped().expect("buffer is not host visible");
        unsafe { std::slice::from_raw_parts(mapped.as_ptr(), self.size as usize) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageDesc {
    pub width: u32,
    pub height: u32,
    pub format: vk::Format,
    pub usage: vk::ImageUsageFlags,
    pub mip_levels: u32,
}

impl ImageDesc {
    pub fn new(width: u32, height: u32, format: vk::Format, usage: vk::ImageUsageFlags) -> Self {
        Self { width, height, format, usage, mip_levels: 1 }
    }

    pub fn with_mips(mut self) -> Self {
        self.mip_levels = full_mip_count(self.width, self.height);
        self
    }

    pub fn extent(&self) -> vk::Extent2D {
        vk::Extent2D { width: self.width, height: self.height }
    }

    pub fn aspect(&self) -> vk::ImageAspectFlags {
        if is_depth(self.format) { vk::ImageAspectFlags::DEPTH } else { vk::ImageAspectFlags::COLOR }
    }
}

pub fn full_mip_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

pub fn is_depth(format: vk::Format) -> bool {
    matches!(format, vk::Format::D32_SFLOAT | vk::Format::D24_UNORM_S8_UINT | vk::Format::D16_UNORM)
}

pub struct Image {
    pub handle: vk::Image,
    /// A view of every mip.
    pub view: vk::ImageView,
    /// One view per mip, for compute passes that write mips one at a time.
    pub mip_views: Vec<vk::ImageView>,
    pub desc: ImageDesc,
    allocation: Option<Allocation>,
}

/// A GPU object waiting until the GPU can no longer be using it.
pub enum Retired {
    Buffer(Buffer),
    Image(Image),
    TopLevel(crate::accel::TopLevel),
}

/// GPU memory as the allocator holds it: each live allocation's name and
/// size, and the bytes of the blocks it sub-allocates them from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuMemory {
    pub allocations: Vec<(String, u64)>,
    pub capacity: u64,
}

impl Gpu {
    /// What the allocator holds now.
    pub fn memory(&self) -> GpuMemory {
        let allocator = self.allocator.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let report = allocator.generate_report();
        let allocations = report.allocations.into_iter().map(|a| (a.name, a.size)).collect();
        GpuMemory { allocations, capacity: report.total_capacity_bytes }
    }

    pub fn create_buffer(
        &self,
        name: &str,
        size: u64,
        usage: vk::BufferUsageFlags,
        location: MemoryLocation,
    ) -> Result<Buffer> {
        let size = size.max(16);
        let usage = usage | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let info = vk::BufferCreateInfo::default().size(size).usage(usage);
        let handle = unsafe { self.device.create_buffer(&info, None)? };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(handle) };
        let allocation = self
            .allocator
            .lock()
            .expect("allocator poisoned")
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location,
                linear: true,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocating {size} bytes for {name}"))?;
        unsafe { self.device.bind_buffer_memory(handle, allocation.memory(), allocation.offset())? };
        let address =
            unsafe { self.device.get_buffer_device_address(&vk::BufferDeviceAddressInfo::default().buffer(handle)) };
        self.set_name(handle, name);
        Ok(Buffer { handle, size, address, allocation: Some(allocation) })
    }

    pub fn create_image(&self, name: &str, desc: ImageDesc) -> Result<Image> {
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(desc.format)
            .extent(vk::Extent3D { width: desc.width, height: desc.height, depth: 1 })
            .mip_levels(desc.mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(desc.usage)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        let handle = unsafe { self.device.create_image(&info, None)? };
        let requirements = unsafe { self.device.get_image_memory_requirements(handle) };
        let allocation = self
            .allocator
            .lock()
            .expect("allocator poisoned")
            .allocate(&AllocationCreateDesc {
                name,
                requirements,
                location: MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocating image {name}"))?;
        unsafe { self.device.bind_image_memory(handle, allocation.memory(), allocation.offset())? };
        self.set_name(handle, name);

        let view = self.create_view(handle, desc, 0, desc.mip_levels)?;
        let mut mip_views = Vec::new();
        if desc.mip_levels > 1 && desc.usage.contains(vk::ImageUsageFlags::STORAGE) {
            for mip in 0..desc.mip_levels {
                mip_views.push(self.create_view(handle, desc, mip, 1)?);
            }
        }
        Ok(Image { handle, view, mip_views, desc, allocation: Some(allocation) })
    }

    fn create_view(&self, image: vk::Image, desc: ImageDesc, base_mip: u32, mips: u32) -> Result<vk::ImageView> {
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(desc.aspect())
            .base_mip_level(base_mip)
            .level_count(mips)
            .layer_count(1);
        let info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(desc.format)
            .subresource_range(range);
        Ok(unsafe { self.device.create_image_view(&info, None)? })
    }

    pub fn destroy(&self, resource: Retired) {
        match resource {
            Retired::Buffer(buffer) => self.destroy_buffer(buffer),
            Retired::Image(image) => self.destroy_image(image),
            Retired::TopLevel(level) => level.destroy(self),
        }
    }

    pub fn destroy_buffer(&self, mut buffer: Buffer) {
        unsafe { self.device.destroy_buffer(buffer.handle, None) };
        if let Some(allocation) = buffer.allocation.take() {
            let _ = self.allocator.lock().expect("allocator poisoned").free(allocation);
        }
    }

    pub fn destroy_image(&self, mut image: Image) {
        unsafe {
            for view in image.mip_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
            self.device.destroy_image_view(image.view, None);
            self.device.destroy_image(image.handle, None);
        }
        if let Some(allocation) = image.allocation.take() {
            let _ = self.allocator.lock().expect("allocator poisoned").free(allocation);
        }
    }

    pub fn create_sampler(&self, info: &vk::SamplerCreateInfo<'_>) -> Result<vk::Sampler> {
        Ok(unsafe { self.device.create_sampler(info, None)? })
    }

    /// Opens a batch: until it closes, uploads ([`Gpu::upload_buffer`]) and
    /// bottom levels' builds ([`Gpu::build_bottom_level`]) are gathered rather
    /// than each submitted and waited for, and then done in one submission,
    /// the uploads first. What they write must not be read before then.
    /// [`GpuBatch::finish`] closes it, or dropping it; batches nest, and the
    /// outermost one's close does the work.
    pub fn batch(&self) -> GpuBatch<'_> {
        self.batch.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).open += 1;
        GpuBatch { gpu: self, open: true }
    }

    /// Closes a batch, and when it is the outermost, submits what it
    /// gathered and waits for it.
    fn end_batch(&self) -> Result<()> {
        let batch = {
            let mut batch = self.batch.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            batch.open = batch.open.saturating_sub(1);
            if batch.open > 0 {
                return Ok(());
            }
            std::mem::take(&mut *batch)
        };
        if batch.copies.is_empty() && batch.builds.is_empty() {
            return Ok(());
        }
        let staging = match batch.bytes.is_empty() {
            true => None,
            false => {
                let usage = vk::BufferUsageFlags::TRANSFER_SRC;
                let staging =
                    self.create_buffer("staging", batch.bytes.len() as u64, usage, MemoryLocation::CpuToGpu)?;
                staging.write(0, &batch.bytes);
                Some(staging)
            }
        };
        let done = self.immediate(|cmd| unsafe {
            if let Some(staging) = &staging {
                for (dst, region) in &batch.copies {
                    self.device.cmd_copy_buffer(cmd, staging.handle, *dst, std::slice::from_ref(region));
                }
            }
            if !batch.builds.is_empty() {
                let written = vk::MemoryBarrier2::default()
                    .src_stage_mask(vk::PipelineStageFlags2::COPY)
                    .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                    .dst_stage_mask(vk::PipelineStageFlags2::ACCELERATION_STRUCTURE_BUILD_KHR)
                    .dst_access_mask(vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::ACCELERATION_STRUCTURE_READ_KHR);
                self.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[written]));
                self.record_bottom_levels(cmd, &batch.builds);
            }
        });
        if let Some(staging) = staging {
            self.destroy_buffer(staging);
        }
        for build in batch.builds {
            self.destroy_buffer(build.scratch);
        }
        done
    }

    /// Records commands, submits them and waits. For load-time uploads; the
    /// streaming path will use the transfer queue instead.
    pub fn immediate(&self, record: impl FnOnce(vk::CommandBuffer)) -> Result<()> {
        unsafe {
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(self.queue_family)
                .flags(vk::CommandPoolCreateFlags::TRANSIENT);
            let pool = self.device.create_command_pool(&pool_info, None)?;
            let alloc = vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1);
            let cmd = self.device.allocate_command_buffers(&alloc)?[0];
            let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device.begin_command_buffer(cmd, &begin)?;
            record(cmd);
            self.device.end_command_buffer(cmd)?;
            let fence = self.device.create_fence(&vk::FenceCreateInfo::default(), None)?;
            let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
            let submit = vk::SubmitInfo2::default().command_buffer_infos(&commands);
            self.device.queue_submit2(self.queue, &[submit], fence)?;
            self.device.wait_for_fences(&[fence], true, u64::MAX)?;
            self.device.destroy_fence(fence, None);
            self.device.destroy_command_pool(pool, None);
        }
        Ok(())
    }

    /// Uploads into a device-local buffer through a temporary staging buffer;
    /// while a batch is open, when it closes (see [`Gpu::batch`]).
    pub fn upload_buffer(&self, dst: &Buffer, offset: u64, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        {
            let mut batch = self.batch.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if batch.open > 0 {
                let src_offset = batch.bytes.len() as u64;
                batch.bytes.extend_from_slice(data);
                batch
                    .copies
                    .push((dst.handle, vk::BufferCopy { src_offset, dst_offset: offset, size: data.len() as u64 }));
                return Ok(());
            }
        }
        let staging = self.create_buffer(
            "staging",
            data.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        staging.write(0, data);
        let region = vk::BufferCopy { src_offset: 0, dst_offset: offset, size: data.len() as u64 };
        self.immediate(|cmd| unsafe { self.device.cmd_copy_buffer(cmd, staging.handle, dst.handle, &[region]) })?;
        self.destroy_buffer(staging);
        Ok(())
    }

    /// Uploads mip 0 of a colour image, generates the rest of the mip chain
    /// with blits, and leaves the image ready for sampling.
    pub fn upload_image(&self, image: &Image, pixels: &[u8]) -> Result<()> {
        let staging = self.create_buffer(
            "staging",
            pixels.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        staging.write(0, pixels);
        let desc = image.desc;
        self.immediate(|cmd| {
            mip_barrier(self, cmd, image.handle, 0, desc.mip_levels, Transition::UndefinedToTransferDst);
            let region = vk::BufferImageCopy::default().image_subresource(color_layers(0)).image_extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            });
            unsafe {
                self.device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.handle,
                    image.handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
            }
            generate_mips(self, cmd, image);
        })?;
        self.destroy_buffer(staging);
        Ok(())
    }

    /// Uploads every mip of an image whose mips were made ahead of time, such
    /// as a block-compressed texture, and leaves it ready for sampling.
    /// `mips` holds level 0 first, in the image's own format.
    pub fn upload_mips(&self, image: &Image, mips: &[Vec<u8>]) -> Result<()> {
        let total: usize = mips.iter().map(Vec::len).sum();
        let staging =
            self.create_buffer("staging", total as u64, vk::BufferUsageFlags::TRANSFER_SRC, MemoryLocation::CpuToGpu)?;
        let mut regions = Vec::with_capacity(mips.len());
        let mut offset = 0;
        for (level, data) in mips.iter().enumerate() {
            staging.write(offset, data);
            let (width, height) = ((image.desc.width >> level).max(1), (image.desc.height >> level).max(1));
            regions.push(
                vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(color_layers(level as u32))
                    .image_extent(vk::Extent3D { width, height, depth: 1 }),
            );
            offset += data.len() as u64;
        }
        let count = mips.len() as u32;
        self.immediate(|cmd| {
            mip_barrier(self, cmd, image.handle, 0, count, Transition::UndefinedToTransferDst);
            unsafe {
                self.device.cmd_copy_buffer_to_image(
                    cmd,
                    staging.handle,
                    image.handle,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &regions,
                );
            }
            mip_barrier(self, cmd, image.handle, 0, count, Transition::DstToShaderRead);
        })?;
        self.destroy_buffer(staging);
        Ok(())
    }

    /// Copies an image's first mip into a host-visible buffer and returns its
    /// bytes. For screenshots and tests.
    /// Reads `size` bytes of a buffer made with `TRANSFER_SRC`, from
    /// `offset`, once what was submitted before has written them; waits for
    /// the GPU.
    pub fn read_buffer(&self, buffer: &Buffer, offset: u64, size: u64) -> Result<Vec<u8>> {
        let usage = vk::BufferUsageFlags::TRANSFER_DST;
        let readback = self.create_buffer("readback", size, usage, MemoryLocation::GpuToCpu)?;
        self.immediate(|cmd| unsafe {
            let written = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ);
            self.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[written]));
            let region = vk::BufferCopy { src_offset: offset, dst_offset: 0, size };
            self.device.cmd_copy_buffer(cmd, buffer.handle, readback.handle, &[region]);
        })?;
        let bytes = readback.read()[..size as usize].to_vec();
        self.destroy_buffer(readback);
        Ok(bytes)
    }

    pub fn read_image(&self, image: vk::Image, desc: ImageDesc, layout: vk::ImageLayout) -> Result<Vec<u8>> {
        let bytes = u64::from(desc.width) * u64::from(desc.height) * 4;
        let readback =
            self.create_buffer("readback", bytes, vk::BufferUsageFlags::TRANSFER_DST, MemoryLocation::GpuToCpu)?;
        self.immediate(|cmd| unsafe {
            let to_src = vk::ImageMemoryBarrier2::default()
                .image(image)
                .old_layout(layout)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::COPY)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
                .subresource_range(color_range(0, 1));
            self.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&[to_src]));
            let region = vk::BufferImageCopy::default().image_subresource(color_layers(0)).image_extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            });
            self.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback.handle,
                &[region],
            );
        })?;
        let pixels = readback.read()[..bytes as usize].to_vec();
        self.destroy_buffer(readback);
        Ok(pixels)
    }
}

/// Layout changes for uploading into an image, for [`mip_barrier`].
pub enum Transition {
    UndefinedToTransferDst,
    TransferDstToSrc,
    SrcToShaderRead,
    DstToShaderRead,
}

/// Moves mips `mip..mip + count` of an image between the layouts an upload uses.
pub fn mip_barrier(gpu: &Gpu, cmd: vk::CommandBuffer, image: vk::Image, mip: u32, count: u32, transition: Transition) {
    use vk::{AccessFlags2 as A, ImageLayout as L, PipelineStageFlags2 as S};
    let (old, new, src, src_access, dst, dst_access) = match transition {
        Transition::UndefinedToTransferDst => {
            (L::UNDEFINED, L::TRANSFER_DST_OPTIMAL, S::NONE, A::NONE, S::ALL_TRANSFER, A::TRANSFER_WRITE)
        }
        Transition::TransferDstToSrc => (
            L::TRANSFER_DST_OPTIMAL,
            L::TRANSFER_SRC_OPTIMAL,
            S::ALL_TRANSFER,
            A::TRANSFER_WRITE,
            S::ALL_TRANSFER,
            A::TRANSFER_READ,
        ),
        Transition::SrcToShaderRead => (
            L::TRANSFER_SRC_OPTIMAL,
            L::SHADER_READ_ONLY_OPTIMAL,
            S::ALL_TRANSFER,
            A::TRANSFER_READ,
            S::ALL_COMMANDS,
            A::SHADER_READ,
        ),
        Transition::DstToShaderRead => (
            L::TRANSFER_DST_OPTIMAL,
            L::SHADER_READ_ONLY_OPTIMAL,
            S::ALL_TRANSFER,
            A::TRANSFER_WRITE,
            S::ALL_COMMANDS,
            A::SHADER_READ,
        ),
    };
    let barrier = vk::ImageMemoryBarrier2::default()
        .image(image)
        .old_layout(old)
        .new_layout(new)
        .src_stage_mask(src)
        .src_access_mask(src_access)
        .dst_stage_mask(dst)
        .dst_access_mask(dst_access)
        .subresource_range(color_range(mip, count));
    unsafe { gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&[barrier])) };
}

/// Expects every mip in `TRANSFER_DST_OPTIMAL` with mip 0 filled; leaves every
/// mip in `SHADER_READ_ONLY_OPTIMAL`.
fn generate_mips(gpu: &Gpu, cmd: vk::CommandBuffer, image: &Image) {
    let desc = image.desc;
    let (mut width, mut height) = (desc.width as i32, desc.height as i32);
    for mip in 1..desc.mip_levels {
        mip_barrier(gpu, cmd, image.handle, mip - 1, 1, Transition::TransferDstToSrc);
        let (next_width, next_height) = ((width / 2).max(1), (height / 2).max(1));
        let blit = vk::ImageBlit::default()
            .src_subresource(color_layers(mip - 1))
            .src_offsets([vk::Offset3D::default(), vk::Offset3D { x: width, y: height, z: 1 }])
            .dst_subresource(color_layers(mip))
            .dst_offsets([vk::Offset3D::default(), vk::Offset3D { x: next_width, y: next_height, z: 1 }]);
        unsafe {
            gpu.device.cmd_blit_image(
                cmd,
                image.handle,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image.handle,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit],
                vk::Filter::LINEAR,
            );
        }
        mip_barrier(gpu, cmd, image.handle, mip - 1, 1, Transition::SrcToShaderRead);
        (width, height) = (next_width, next_height);
    }
    mip_barrier(gpu, cmd, image.handle, desc.mip_levels - 1, 1, Transition::DstToShaderRead);
}

fn color_layers(mip: u32) -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::COLOR).mip_level(mip).layer_count(1)
}

fn color_range(mip: u32, count: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(mip)
        .level_count(count)
        .layer_count(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_in_a_batch_land_together_when_its_outermost_closes_as_one_at_a_time_would() {
        let Ok(gpu) = Gpu::new_headless() else { return };
        let usage = vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC;
        let buffer = gpu.create_buffer("batched", 64, usage, MemoryLocation::GpuOnly).unwrap();
        gpu.upload_buffer(&buffer, 0, &[0; 64]).unwrap();
        let outer = gpu.batch();
        gpu.upload_buffer(&buffer, 0, &[1; 16]).unwrap();
        let inner = gpu.batch();
        gpu.upload_buffer(&buffer, 32, &[2; 16]).unwrap();
        inner.finish().unwrap();
        assert_eq!(gpu.batch.lock().unwrap().copies.len(), 2, "an inner batch's close leaves its work to the outer's");
        outer.finish().unwrap();
        let read = gpu.read_buffer(&buffer, 0, 64).unwrap();
        assert_eq!((&read[..16], &read[16..32], &read[32..48]), (&[1; 16][..], &[0; 16][..], &[2; 16][..]));
        // A batch left on the way out of an error still does its work.
        {
            let _left = gpu.batch();
            gpu.upload_buffer(&buffer, 48, &[3; 16]).unwrap();
        }
        assert_eq!(gpu.read_buffer(&buffer, 48, 16).unwrap(), [3; 16]);
        assert_eq!(gpu.batch.lock().unwrap().open, 0);
        gpu.destroy_buffer(buffer);
    }
}
