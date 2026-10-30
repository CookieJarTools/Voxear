use anyhow::Result;
use ash::vk;

use crate::device::Gpu;

pub struct Swapchain {
    pub surface: vk::SurfaceKHR,
    pub handle: vk::SwapchainKHR,
    pub format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
    pub images: Vec<vk::Image>,
    pub views: Vec<vk::ImageView>,
    /// Signalled when an image may be presented. There is one per image
    /// because presentation holds the semaphore until that image comes back.
    pub present_ready: Vec<vk::Semaphore>,
    pub vsync: bool,
}

impl Swapchain {
    pub fn new(gpu: &Gpu, surface: vk::SurfaceKHR, width: u32, height: u32, vsync: bool) -> Result<Self> {
        let mut swapchain = Self {
            surface,
            handle: vk::SwapchainKHR::null(),
            format: vk::SurfaceFormatKHR::default(),
            extent: vk::Extent2D::default(),
            images: Vec::new(),
            views: Vec::new(),
            present_ready: Vec::new(),
            vsync,
        };
        swapchain.build(gpu, width, height)?;
        Ok(swapchain)
    }

    /// Rebuilds for a new window size. A zero size (minimised) is ignored
    /// until the window comes back.
    pub fn recreate(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<()> {
        gpu.wait_idle();
        self.build(gpu, width, height)
    }

    pub fn is_renderable(&self) -> bool {
        self.extent.width > 0 && self.extent.height > 0 && self.handle != vk::SwapchainKHR::null()
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        self.release_images(gpu);
        unsafe {
            gpu.swapchain_loader.destroy_swapchain(self.handle, None);
            gpu.surface_loader.destroy_surface(self.surface, None);
        }
        self.handle = vk::SwapchainKHR::null();
    }

    fn build(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<()> {
        let caps = unsafe { gpu.surface_loader.get_physical_device_surface_capabilities(gpu.physical, self.surface)? };
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            self.extent = extent;
            return Ok(());
        }

        let formats = unsafe { gpu.surface_loader.get_physical_device_surface_formats(gpu.physical, self.surface)? };
        let format = formats
            .iter()
            .copied()
            .find(|f| {
                matches!(f.format, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB)
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .unwrap_or(formats[0]);
        let modes =
            unsafe { gpu.surface_loader.get_physical_device_surface_present_modes(gpu.physical, self.surface)? };
        let present_mode = choose_present_mode(&modes, self.vsync);

        let mut image_count = caps.min_image_count + 1;
        if caps.max_image_count != 0 {
            image_count = image_count.min(caps.max_image_count);
        }

        let old = self.handle;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .old_swapchain(old);
        self.handle = unsafe { gpu.swapchain_loader.create_swapchain(&info, None)? };
        self.release_images(gpu);
        if old != vk::SwapchainKHR::null() {
            unsafe { gpu.swapchain_loader.destroy_swapchain(old, None) };
        }

        self.format = format;
        self.extent = extent;
        self.images = unsafe { gpu.swapchain_loader.get_swapchain_images(self.handle)? };
        for &image in &self.images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format.format)
                .subresource_range(color_range());
            self.views.push(unsafe { gpu.device.create_image_view(&view_info, None)? });
            self.present_ready.push(unsafe { gpu.device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? });
        }
        tracing::debug!("swapchain {}x{} {:?} {:?}", extent.width, extent.height, format.format, present_mode);
        Ok(())
    }

    fn release_images(&mut self, gpu: &Gpu) {
        unsafe {
            for view in self.views.drain(..) {
                gpu.device.destroy_image_view(view, None);
            }
            for semaphore in self.present_ready.drain(..) {
                gpu.device.destroy_semaphore(semaphore, None);
            }
        }
        self.images.clear();
    }
}

/// FIFO is vsync and always available. Without vsync, mailbox gives the
/// lowest latency without tearing.
fn choose_present_mode(modes: &[vk::PresentModeKHR], vsync: bool) -> vk::PresentModeKHR {
    if vsync {
        return vk::PresentModeKHR::FIFO;
    }
    [vk::PresentModeKHR::MAILBOX, vk::PresentModeKHR::IMMEDIATE]
        .into_iter()
        .find(|m| modes.contains(m))
        .unwrap_or(vk::PresentModeKHR::FIFO)
}

pub(crate) fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default().aspect_mask(vk::ImageAspectFlags::COLOR).level_count(1).layer_count(1)
}
