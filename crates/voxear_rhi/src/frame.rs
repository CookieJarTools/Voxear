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
    /// Signalled once the image may be drawn into.
    ready: vk::Semaphore,
}

/// What asking another window's swapchain for an image gave.
pub enum Acquire {
    Ready(Acquired),
    /// The swapchain must be recreated first.
    OutOfDate,
    /// No image is free yet, as for a window the desktop is not showing;
    /// that window waits for a later frame.
    NotReady,
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
        let ready = self.slots[self.current].image_available;
        match acquire(gpu, swapchain, ready, u64::MAX)? {
            Acquire::Ready(acquired) => Ok(Some(acquired)),
            Acquire::OutOfDate | Acquire::NotReady => Ok(None),
        }
    }

    /// Acquires the next image of another window's swapchain, signalling
    /// `ready`, which the window keeps one of for each frame in flight
    /// ([`slot`](Frame::slot)). It does not wait for an image, so a window
    /// the desktop is not showing never holds up the others.
    pub fn try_acquire(&mut self, gpu: &Gpu, swapchain: &Swapchain, ready: vk::Semaphore) -> Result<Acquire> {
        acquire(gpu, swapchain, ready, 0)
    }

    /// The frame being recorded's slot, for semaphores kept per frame in flight.
    pub fn slot(&self) -> usize {
        self.current
    }

    /// Defers destroying a resource until this slot comes round again.
    pub fn retire(&mut self, resource: Retired) {
        self.slots[self.current].retired.push(resource);
    }

    /// Submits the frame, and presents when it rendered to a swapchain image.
    /// Returns `true` when the swapchain should be recreated.
    pub fn submit(&mut self, gpu: &Gpu, frame: Frame, present: Option<(&Swapchain, Acquired)>) -> Result<bool> {
        let recreate = self.submit_presenting(gpu, frame, present.into_iter().collect())?;
        Ok(recreate.first().copied().unwrap_or(false))
    }

    /// Submits the frame and presents each swapchain image it rendered to,
    /// one for each window. Returns, for each, whether its swapchain should
    /// be recreated.
    pub fn submit_presenting(
        &mut self,
        gpu: &Gpu,
        frame: Frame,
        presents: Vec<(&Swapchain, Acquired)>,
    ) -> Result<Vec<bool>> {
        let slot = &self.slots[frame.slot];
        self.current = (self.current + 1) % self.slots.len();
        let stage = vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT;
        let waits: Vec<_> = presents
            .iter()
            .map(|(_, acquired)| vk::SemaphoreSubmitInfo::default().semaphore(acquired.ready).stage_mask(stage))
            .collect();
        let present_ready: Vec<_> =
            presents.iter().map(|(swapchain, acquired)| swapchain.present_ready[acquired.index as usize]).collect();
        let signals: Vec<_> = present_ready
            .iter()
            .map(|&ready| vk::SemaphoreSubmitInfo::default().semaphore(ready).stage_mask(stage))
            .collect();
        unsafe {
            gpu.device.end_command_buffer(frame.cmd)?;
            // Reset only now that a submit is certain, or the next wait would never finish.
            gpu.device.reset_fences(&[slot.done])?;
            let commands = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
            let submit = vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&commands)
                .signal_semaphore_infos(&signals);
            gpu.device.queue_submit2(gpu.queue, &[submit], slot.done)?;
        }
        // Each on its own, so one window's swapchain going out of date leaves the others presented.
        let mut recreate = Vec::with_capacity(presents.len());
        for ((swapchain, acquired), ready) in presents.iter().zip(present_ready) {
            let (wait, swapchains, indices) = ([ready], [swapchain.handle], [acquired.index]);
            let info =
                vk::PresentInfoKHR::default().wait_semaphores(&wait).swapchains(&swapchains).image_indices(&indices);
            recreate.push(match unsafe { gpu.swapchain_loader.queue_present(gpu.queue, &info) } {
                Ok(suboptimal) => suboptimal || acquired.suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(error) => return Err(error.into()),
            });
        }
        Ok(recreate)
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

fn acquire(gpu: &Gpu, swapchain: &Swapchain, ready: vk::Semaphore, timeout: u64) -> Result<Acquire> {
    let acquired =
        unsafe { gpu.swapchain_loader.acquire_next_image(swapchain.handle, timeout, ready, vk::Fence::null()) };
    match acquired {
        Ok((index, suboptimal)) => Ok(Acquire::Ready(Acquired {
            index,
            image: swapchain.images[index as usize],
            view: swapchain.views[index as usize],
            suboptimal,
            ready,
        })),
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(Acquire::OutOfDate),
        Err(vk::Result::NOT_READY | vk::Result::TIMEOUT) => Ok(Acquire::NotReady),
        Err(error) => Err(error.into()),
    }
}
