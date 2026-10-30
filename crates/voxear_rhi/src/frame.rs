use anyhow::Result;
use ash::vk;

use crate::{device::Gpu, resources::Retired, swapchain::Swapchain};

pub const FRAMES_IN_FLIGHT: usize = 2;

struct Slot {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    image_available: vk::Semaphore,
    done: vk::Fence,
    retired: Vec<Retired>,
}

/// A frame being recorded. `slot` indexes per-frame resources.
pub struct Frame {
    pub cmd: vk::CommandBuffer,
    pub slot: usize,
    pub number: u64,
}

/// An acquired swapchain image.
pub struct Acquired {
    pub index: u32,
    pub image: vk::Image,
    pub view: vk::ImageView,
    suboptimal: bool,
}

/// Paces the CPU at most [`FRAMES_IN_FLIGHT`] frames ahead of the GPU, and
/// destroys retired resources once no frame can still use them.
pub struct Frames {
    slots: Vec<Slot>,
    current: usize,
    number: u64,
}

impl Frames {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let mut slots = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for index in 0..FRAMES_IN_FLIGHT {
            unsafe {
                let pool_info = vk::CommandPoolCreateInfo::default()
                    .queue_family_index(gpu.queue_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT);
                let pool = gpu.device.create_command_pool(&pool_info, None)?;
                let alloc = vk::CommandBufferAllocateInfo::default().command_pool(pool).command_buffer_count(1);
                let cmd = gpu.device.allocate_command_buffers(&alloc)?[0];
                gpu.set_name(cmd, &format!("frame {index}"));
                let image_available = gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
                let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
                let done = gpu.device.create_fence(&fence_info, None)?;
                slots.push(Slot { pool, cmd, image_available, done, retired: Vec::new() });
            }
        }
        Ok(Self { slots, current: 0, number: 0 })
    }

    /// Waits until this slot's previous frame finished, frees what it retired,
    /// and starts recording.
    pub fn begin(&mut self, gpu: &Gpu) -> Result<Frame> {
        let slot = &mut self.slots[self.current];
        unsafe {
            gpu.device.wait_for_fences(&[slot.done], true, u64::MAX)?;
            for resource in slot.retired.drain(..) {
                gpu.destroy(resource);
            }
            gpu.device.reset_command_pool(slot.pool, vk::CommandPoolResetFlags::empty())?;
            let begin = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            gpu.device.begin_command_buffer(slot.cmd, &begin)?;
        }
        self.number += 1;
        Ok(Frame { cmd: slot.cmd, slot: self.current, number: self.number })
    }

    /// Acquires the next swapchain image. `None` means the swapchain is out
    /// of date and must be recreated; the recorded frame is then dropped.
    pub fn acquire(&mut self, gpu: &Gpu, swapchain: &Swapchain) -> Result<Option<Acquired>> {
        let slot = &self.slots[self.current];
        let acquired = unsafe {
            gpu.swapchain_loader.acquire_next_image(swapchain.handle, u64::MAX, slot.image_available, vk::Fence::null())
        };
        match acquired {
            Ok((index, suboptimal)) => Ok(Some(Acquired {
                index,
                image: swapchain.images[index as usize],
                view: swapchain.views[index as usize],
                suboptimal,
            })),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Defers destroying a resource until this slot comes round again.
    pub fn retire(&mut self, resource: Retired) {
        self.slots[self.current].retired.push(resource);
    }

    /// Submits the frame, and presents when it rendered to a swapchain image.
    /// Returns `true` when the swapchain should be recreated.
    pub fn submit(&mut self, gpu: &Gpu, frame: Frame, present: Option<(&Swapchain, Acquired)>) -> Result<bool> {
        let slot = &self.slots[frame.slot];
        self.current = (self.current + 1) % self.slots.len();
        unsafe {
            gpu.device.end_command_buffer(frame.cmd)?;
            // Reset only now that a submit is certain, or the next wait would never finish.
            gpu.device.reset_fences(&[slot.done])?;
            let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
            let Some((swapchain, acquired)) = present else {
                let submit = vk::SubmitInfo2::default().command_buffer_infos(&commands);
                gpu.device.queue_submit2(gpu.queue, &[submit], slot.done)?;
                return Ok(false);
            };

            let present_ready = swapchain.present_ready[acquired.index as usize];
            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(slot.image_available)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(present_ready)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let submit = vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&commands)
                .signal_semaphore_infos(&signals);
            gpu.device.queue_submit2(gpu.queue, &[submit], slot.done)?;

            let wait = [present_ready];
            let swapchains = [swapchain.handle];
            let indices = [acquired.index];
            let info = vk::PresentInfoKHR::default().wait_semaphores(&wait).swapchains(&swapchains).image_indices(&indices);
            match gpu.swapchain_loader.queue_present(gpu.queue, &info) {
                Ok(suboptimal) => Ok(suboptimal || acquired.suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true),
                Err(error) => Err(error.into()),
            }
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        gpu.wait_idle();
        unsafe {
            for mut slot in self.slots.drain(..) {
                for resource in slot.retired.drain(..) {
                    gpu.destroy(resource);
                }
                gpu.device.destroy_fence(slot.done, None);
                gpu.device.destroy_semaphore(slot.image_available, None);
                gpu.device.destroy_command_pool(slot.pool, None);
            }
        }
    }
}
