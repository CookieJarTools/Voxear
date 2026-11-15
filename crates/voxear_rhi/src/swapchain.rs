use anyhow::{Result, bail};
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
    /// What its images can be used for: drawn into, and copied from where
    /// the surface allows it, for the frame debugger.
    pub usage: vk::ImageUsageFlags,
    /// The format its images must have, for a window drawn with pipelines
    /// made for another's.
    matching: Option<vk::Format>,
    /// What output is asked for, what its images are shown as, and the HDR
    /// outputs its surface offers.
    pub hdr: HdrChoice,
    pub encoding: Encoding,
    pub offered: Offered,
    /// How bright paper white and the display's peak are, in nits, told to
    /// the display where the driver can.
    pub brightness: (f32, f32),
}

/// What output a window asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HdrChoice {
    /// SDR.
    #[default]
    Off,
    /// HDR where the display offers it, HDR10 first, then scRGB; SDR where
    /// it offers neither.
    On,
    /// HDR10 alone, or SDR.
    Hdr10,
    /// scRGB alone, or SDR.
    ScRgb,
}

/// What a swapchain's pixels mean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    /// sRGB, written linear into an sRGB format, from black to paper white.
    #[default]
    Sdr,
    /// HDR10: BT.2020 colours, through the PQ curve (ST 2084), in 10 bits a
    /// channel, 1 being 10,000 nits.
    Hdr10,
    /// scRGB: linear BT.709 colours in half floats, 1 being 80 nits, past 1
    /// for brighter and below 0 for colours outside sRGB's.
    ScRgb,
}

impl Encoding {
    pub fn is_hdr(self) -> bool {
        self != Self::Sdr
    }
}

/// The HDR outputs a surface offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offered {
    pub hdr10: bool,
    pub scrgb: bool,
}

/// The surface format for `choice` from those a surface offers, and what
/// its pixels mean: an HDR one where it is asked for and offered, the
/// sRGB one otherwise. `matching` holds a window to one format, in sRGB,
/// as a window drawn with another's pipelines must be.
pub fn choose_format(
    formats: &[vk::SurfaceFormatKHR],
    choice: HdrChoice,
    matching: Option<vk::Format>,
) -> Option<(vk::SurfaceFormatKHR, Encoding)> {
    let srgb = |f: &&vk::SurfaceFormatKHR| f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR;
    if let Some(wanted) = matching {
        return formats.iter().filter(srgb).find(|f| f.format == wanted).map(|f| (*f, Encoding::Sdr));
    }
    let hdr10 = || formats.iter().find(|f| is_hdr10(f)).map(|f| (*f, Encoding::Hdr10));
    let scrgb = || formats.iter().find(|f| is_scrgb(f)).map(|f| (*f, Encoding::ScRgb));
    let hdr = match choice {
        HdrChoice::Off => None,
        HdrChoice::On => hdr10().or_else(scrgb),
        HdrChoice::Hdr10 => hdr10(),
        HdrChoice::ScRgb => scrgb(),
    };
    let sdr = || {
        let found = formats
            .iter()
            .filter(srgb)
            .find(|f| matches!(f.format, vk::Format::B8G8R8A8_SRGB | vk::Format::R8G8B8A8_SRGB))
            .or_else(|| formats.iter().find(srgb))
            .or(formats.first());
        found.map(|f| (*f, Encoding::Sdr))
    };
    hdr.or_else(sdr)
}

/// The HDR outputs among a surface's formats.
pub fn offered(formats: &[vk::SurfaceFormatKHR]) -> Offered {
    Offered { hdr10: formats.iter().any(is_hdr10), scrgb: formats.iter().any(is_scrgb) }
}

fn is_hdr10(f: &vk::SurfaceFormatKHR) -> bool {
    f.color_space == vk::ColorSpaceKHR::HDR10_ST2084_EXT
        && matches!(f.format, vk::Format::A2B10G10R10_UNORM_PACK32 | vk::Format::A2R10G10B10_UNORM_PACK32)
}

fn is_scrgb(f: &vk::SurfaceFormatKHR) -> bool {
    f.color_space == vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT && f.format == vk::Format::R16G16B16A16_SFLOAT
}

impl Swapchain {
    pub fn new(gpu: &Gpu, surface: vk::SurfaceKHR, width: u32, height: u32, vsync: bool) -> Result<Self> {
        Self::create(gpu, surface, (width, height), vsync, None)
    }

    /// A swapchain for another window, its images in `format`, as the first
    /// window's are, so the same pipelines draw into both. Fails when the
    /// window's surface does not offer it.
    pub fn new_matching(
        gpu: &Gpu,
        surface: vk::SurfaceKHR,
        (width, height): (u32, u32),
        vsync: bool,
        format: vk::Format,
    ) -> Result<Self> {
        Self::create(gpu, surface, (width, height), vsync, Some(format))
    }

    fn create(
        gpu: &Gpu,
        surface: vk::SurfaceKHR,
        (width, height): (u32, u32),
        vsync: bool,
        matching: Option<vk::Format>,
    ) -> Result<Self> {
        let mut swapchain = Self {
            surface,
            handle: vk::SwapchainKHR::null(),
            format: vk::SurfaceFormatKHR::default(),
            extent: vk::Extent2D::default(),
            images: Vec::new(),
            views: Vec::new(),
            present_ready: Vec::new(),
            vsync,
            usage: vk::ImageUsageFlags::empty(),
            matching,
            hdr: HdrChoice::Off,
            encoding: Encoding::Sdr,
            offered: Offered::default(),
            brightness: (200.0, 1000.0),
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
        // Without the colour spaces extension a driver may still list HDR formats, which may not be used.
        let usable: Vec<vk::SurfaceFormatKHR> = formats
            .iter()
            .copied()
            .filter(|f| gpu.color_spaces || f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .collect();
        self.offered = offered(&usable);
        let (format, encoding) = match choose_format(&usable, self.hdr, self.matching) {
            Some(chosen) => chosen,
            None => match self.matching {
                Some(wanted) => bail!("the window cannot show images in {wanted:?}, as the editor's first window does"),
                None => bail!("the window offers no format to show images in"),
            },
        };
        if self.hdr != HdrChoice::Off && !encoding.is_hdr() {
            tracing::info!("HDR output asked for, but the display offers none; showing SDR");
        }
        self.encoding = encoding;
        let modes =
            unsafe { gpu.surface_loader.get_physical_device_surface_present_modes(gpu.physical, self.surface)? };
        let present_mode = choose_present_mode(&modes, self.vsync);

        let mut image_count = caps.min_image_count + 1;
        if caps.max_image_count != 0 {
            image_count = image_count.min(caps.max_image_count);
        }

        let copyable = caps.supported_usage_flags & vk::ImageUsageFlags::TRANSFER_SRC;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_DST | copyable;
        let old = self.handle;
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(usage)
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
        self.usage = usage;
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
        self.tell_brightness(gpu);
        Ok(())
    }

    /// Asks for HDR output, or SDR, and makes the swapchain again for it
    /// when that changes what it shows.
    pub fn set_hdr(&mut self, gpu: &Gpu, hdr: HdrChoice, brightness: (f32, f32)) -> Result<()> {
        let rebuild = hdr != self.hdr;
        self.hdr = hdr;
        self.brightness = brightness;
        if rebuild && self.handle != vk::SwapchainKHR::null() {
            gpu.wait_idle();
            return self.build(gpu, self.extent.width, self.extent.height);
        }
        self.tell_brightness(gpu);
        Ok(())
    }

    /// Tells an HDR display how bright the pictures get, as the content's
    /// mastering display, so its own tonemapping leaves them as they are.
    fn tell_brightness(&self, gpu: &Gpu) {
        let Some(metadata) = gpu.hdr_metadata.as_ref().filter(|_| self.encoding.is_hdr()) else { return };
        let (paper_white, peak) = self.brightness;
        let point = |x: f32, y: f32| vk::XYColorEXT { x, y };
        // BT.2020's primaries and D65.
        let info = vk::HdrMetadataEXT::default()
            .display_primary_red(point(0.708, 0.292))
            .display_primary_green(point(0.170, 0.797))
            .display_primary_blue(point(0.131, 0.046))
            .white_point(point(0.3127, 0.3290))
            .max_luminance(peak)
            .min_luminance(0.0)
            .max_content_light_level(peak)
            .max_frame_average_light_level(paper_white.min(peak));
        unsafe { metadata.set_hdr_metadata(&[self.handle], &[info]) };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn format(format: vk::Format, color_space: vk::ColorSpaceKHR) -> vk::SurfaceFormatKHR {
        vk::SurfaceFormatKHR { format, color_space }
    }

    #[test]
    fn hdr_is_chosen_where_offered_and_sdr_otherwise() {
        let sdr = format(vk::Format::B8G8R8A8_SRGB, vk::ColorSpaceKHR::SRGB_NONLINEAR);
        let unorm = format(vk::Format::B8G8R8A8_UNORM, vk::ColorSpaceKHR::SRGB_NONLINEAR);
        let hdr10 = format(vk::Format::A2B10G10R10_UNORM_PACK32, vk::ColorSpaceKHR::HDR10_ST2084_EXT);
        let scrgb = format(vk::Format::R16G16B16A16_SFLOAT, vk::ColorSpaceKHR::EXTENDED_SRGB_LINEAR_EXT);
        // HDR10's colour space in a format it cannot be drawn into is not HDR10.
        let odd = format(vk::Format::R8G8B8A8_UNORM, vk::ColorSpaceKHR::HDR10_ST2084_EXT);
        let all = [unorm, sdr, odd, scrgb, hdr10];
        let pick = |formats: &[vk::SurfaceFormatKHR], choice| choose_format(formats, choice, None).unwrap();
        assert_eq!(pick(&all, HdrChoice::Off), (sdr, Encoding::Sdr), "the sRGB format, never an HDR one");
        assert_eq!(pick(&all, HdrChoice::On), (hdr10, Encoding::Hdr10), "HDR10 first");
        assert_eq!(pick(&all, HdrChoice::ScRgb), (scrgb, Encoding::ScRgb));
        assert_eq!(pick(&[sdr, scrgb], HdrChoice::On), (scrgb, Encoding::ScRgb), "scRGB where HDR10 is not offered");
        assert_eq!(pick(&[sdr, scrgb], HdrChoice::Hdr10), (sdr, Encoding::Sdr), "SDR where what is asked is not");
        assert_eq!(pick(&[unorm, odd], HdrChoice::On), (unorm, Encoding::Sdr), "SDR in whatever there is");
        assert_eq!(offered(&all), Offered { hdr10: true, scrgb: true });
        assert_eq!(offered(&[sdr, odd]), Offered::default());
        // A window drawn with another's pipelines stays in its format, in SDR.
        assert_eq!(choose_format(&all, HdrChoice::On, Some(vk::Format::B8G8R8A8_SRGB)), Some((sdr, Encoding::Sdr)));
        assert_eq!(choose_format(&[hdr10], HdrChoice::Off, Some(vk::Format::B8G8R8A8_SRGB)), None);
        assert_eq!(choose_format(&[], HdrChoice::On, None), None);
    }
}
