use anyhow::Result;
use ash::vk;

use crate::{device::Gpu, swapchain::Swapchain};

pub const FRAMES_IN_FLIGHT: usize = 2;

struct Slot {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    image_available: vk::Semaphore,
    done: vk::Fence,
}

/// A frame being recorded into a swapchain image.
pub struct Frame {
    pub cmd: vk::CommandBuffer,
    pub image_index: u32,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub extent: vk::Extent2D,
    pub format: vk::Format,
    suboptimal: bool,
}

/// Paces the CPU at most [`FRAMES_IN_FLIGHT`] frames ahead of the GPU.
pub struct Frames {
    slots: Vec<Slot>,
    current: usize,
}

impl Frames {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let mut slots = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for _ in 0..FRAMES_IN_FLIGHT {
            unsafe {
                let pool_info = vk::CommandPoolCreateInfo::default()
                    .queue_family_index(gpu.queue_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT);
                let pool = gpu.device.create_command_pool(&pool_info, None)?;
                let alloc = vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1);
                let cmd = gpu.device.allocate_command_buffers(&alloc)?[0];
                let image_available = gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
                let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
                let done = gpu.device.create_fence(&fence_info, None)?;
                slots.push(Slot { pool, cmd, image_available, done });
            }
        }
        Ok(Self { slots, current: 0 })
    }

    /// Waits for this slot's previous frame, acquires an image and starts
    /// recording. `None` means the swapchain is out of date and must be
    /// recreated first.
    pub fn begin(&mut self, gpu: &Gpu, swapchain: &Swapchain) -> Result<Option<Frame>> {
        let slot = &self.slots[self.current];
        unsafe {
            gpu.device.wait_for_fences(&[slot.done], true, u64::MAX)?;
            let acquired = gpu.swapchain_loader.acquire_next_image(
                swapchain.handle,
                u64::MAX,
                slot.image_available,
                vk::Fence::null(),
            );
            let (image_index, suboptimal) = match acquired {
                Ok(result) => result,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            // Only reset once an image is guaranteed, or the next wait would never finish.
            gpu.device.reset_fences(&[slot.done])?;
            gpu.device.reset_command_pool(slot.pool, vk::CommandPoolResetFlags::empty())?;
            let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            gpu.device.begin_command_buffer(slot.cmd, &begin)?;
            Ok(Some(Frame {
                cmd: slot.cmd,
                image_index,
                image: swapchain.images[image_index as usize],
                view: swapchain.views[image_index as usize],
                extent: swapchain.extent,
                format: swapchain.format.format,
                suboptimal,
            }))
        }
    }

    /// Submits and presents. Returns `true` when the swapchain should be
    /// recreated before the next frame.
    pub fn end(&mut self, gpu: &Gpu, swapchain: &Swapchain, frame: Frame) -> Result<bool> {
        let slot = &self.slots[self.current];
        self.current = (self.current + 1) % self.slots.len();
        let present_ready = swapchain.present_ready[frame.image_index as usize];
        unsafe {
            gpu.device.end_command_buffer(frame.cmd)?;
            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(slot.image_available)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(present_ready)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
            let submit = vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&commands)
                .signal_semaphore_infos(&signals);
            gpu.device.queue_submit2(gpu.queue, &[submit], slot.done)?;

            let wait = [present_ready];
            let swapchains = [swapchain.handle];
            let indices = [frame.image_index];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait)
                .swapchains(&swapchains)
                .image_indices(&indices);
            match gpu.swapchain_loader.queue_present(gpu.queue, &present) {
                Ok(suboptimal) => Ok(suboptimal || frame.suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true),
                Err(error) => Err(error.into()),
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        unsafe {
            for slot in self.slots.drain(..) {
                gpu.device.destroy_fence(slot.done, None);
                gpu.device.destroy_semaphore(slot.image_available, None);
                gpu.device.destroy_command_pool(slot.pool, None);
            }
        }
    }
}

/// Records a full-image layout transition with synchronization2.
#[allow(clippy::too_many_arguments)]
pub fn transition_image(
    gpu: &Gpu,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    src: (vk::PipelineStageFlags2, vk::AccessFlags2),
    dst: (vk::PipelineStageFlags2, vk::AccessFlags2),
) {
    let barrier = [vk::ImageMemoryBarrier2::default()
        .image(image)
        .old_layout(from)
        .new_layout(to)
        .src_stage_mask(src.0)
        .src_access_mask(src.1)
        .dst_stage_mask(dst.0)
        .dst_access_mask(dst.1)
        .subresource_range(crate::swapchain::color_range())];
    let dependency = vk::DependencyInfo::default().image_memory_barriers(&barrier);
    unsafe { gpu.device.cmd_pipeline_barrier2(cmd, &dependency) };
}
