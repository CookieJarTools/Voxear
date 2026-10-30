use std::{
    ffi::{CStr, c_char, c_void},
    mem::ManuallyDrop,
    sync::Mutex,
};

use anyhow::{Context, Result, bail};
use ash::{ext, khr, vk};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

const VALIDATION_LAYER: &CStr = c"VK_LAYER_KHRONOS_validation";

/// What a GPU is and which optional tiers it supports.
#[derive(Clone, Debug)]
pub struct GpuInfo {
    pub name: String,
    pub kind: vk::PhysicalDeviceType,
    pub api_version: (u32, u32, u32),
    pub driver: String,
    pub device_local_mb: u64,
    pub mesh_shaders: bool,
    pub ray_tracing: bool,
    /// Why the GPU cannot run Voxear, if it cannot.
    pub unsupported: Option<String>,
}

/// The device everything renders with.
pub struct Gpu {
    pub entry: ash::Entry,
    pub instance: ash::Instance,
    pub physical: vk::PhysicalDevice,
    pub device: ash::Device,
    pub queue_family: u32,
    pub queue: vk::Queue,
    pub info: GpuInfo,
    pub surface_loader: khr::surface::Instance,
    pub swapchain_loader: khr::swapchain::Device,
    pub allocator: ManuallyDrop<Mutex<Allocator>>,
    debug: Option<(ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
}

impl Gpu {
    /// Creates the device and a surface for the window, picking the best GPU
    /// that can present to it.
    ///
    /// # Safety
    /// The window handles must stay valid until the surface is destroyed.
    pub unsafe fn new(display: RawDisplayHandle, window: RawWindowHandle) -> Result<(Self, vk::SurfaceKHR)> {
        let entry = unsafe { ash::Entry::load() }.context("the Vulkan loader is not installed")?;
        let validation = wants_validation(&entry);
        let mut extensions = ash_window::enumerate_required_extensions(display)?.to_vec();
        if validation {
            extensions.push(ext::debug_utils::NAME.as_ptr());
        }
        let instance = create_instance(&entry, &extensions, validation)?;
        let debug = validation.then(|| create_messenger(&entry, &instance)).transpose()?;

        let surface = unsafe { ash_window::create_surface(&entry, &instance, display, window, None)? };
        let surface_loader = khr::surface::Instance::new(&entry, &instance);

        let candidates = unsafe { instance.enumerate_physical_devices()? };
        let mut best: Option<(u64, vk::PhysicalDevice, u32, GpuInfo)> = None;
        for physical in candidates {
            let info = describe(&instance, physical);
            if let Some(reason) = &info.unsupported {
                tracing::info!("skipping {}: {reason}", info.name);
                continue;
            }
            let Some(family) = present_queue_family(&instance, &surface_loader, physical, surface) else {
                tracing::info!("skipping {}: cannot present to this window", info.name);
                continue;
            };
            let score = score(&info);
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, physical, family, info));
            }
        }
        let Some((_, physical, queue_family, info)) = best else {
            bail!("no GPU supports Vulkan 1.3 with the features Voxear needs; run `vox doctor` for details");
        };

        let device = create_device(&instance, physical, queue_family)?;
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let swapchain_loader = khr::swapchain::Device::new(&instance, &device);
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: instance.clone(),
            device: device.clone(),
            physical_device: physical,
            debug_settings: Default::default(),
            buffer_device_address: true,
            allocation_sizes: Default::default(),
        })?;

        let (major, minor, patch) = info.api_version;
        tracing::info!("GPU: {} (Vulkan {major}.{minor}.{patch}, {})", info.name, info.driver);
        let gpu = Self {
            entry,
            instance,
            physical,
            device,
            queue_family,
            queue,
            info,
            surface_loader,
            swapchain_loader,
            allocator: ManuallyDrop::new(Mutex::new(allocator)),
            debug,
        };
        Ok((gpu, surface))
    }

    pub fn wait_idle(&self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            self.wait_idle();
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            if let Some((loader, messenger)) = self.debug.take() {
                loader.destroy_debug_utils_messenger(messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

/// Lists every GPU and whether it can run Voxear, without needing a window.
pub fn enumerate_gpus() -> Result<Vec<GpuInfo>> {
    let entry = unsafe { ash::Entry::load() }.context("the Vulkan loader is not installed")?;
    let instance = create_instance(&entry, &[], false)?;
    let devices = unsafe { instance.enumerate_physical_devices() };
    let infos = devices.map(|devices| devices.into_iter().map(|d| describe(&instance, d)).collect());
    unsafe { instance.destroy_instance(None) };
    Ok(infos?)
}

fn wants_validation(entry: &ash::Entry) -> bool {
    let requested = match std::env::var("VOXEAR_VALIDATION") {
        Ok(value) => value != "0",
        Err(_) => cfg!(debug_assertions),
    };
    if !requested {
        return false;
    }
    let layers = unsafe { entry.enumerate_instance_layer_properties() }.unwrap_or_default();
    let available = layers.iter().any(|l| l.layer_name_as_c_str() == Ok(VALIDATION_LAYER));
    if !available {
        tracing::debug!("Vulkan validation layer not installed; running without it");
    }
    available
}

fn create_instance(entry: &ash::Entry, extensions: &[*const c_char], validation: bool) -> Result<ash::Instance> {
    let app = vk::ApplicationInfo::default()
        .application_name(c"Voxear")
        .engine_name(c"Voxear")
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(vk::API_VERSION_1_3);
    let layers = if validation { vec![VALIDATION_LAYER.as_ptr()] } else { Vec::new() };
    let info = vk::InstanceCreateInfo::default()
        .application_info(&app)
        .enabled_extension_names(extensions)
        .enabled_layer_names(&layers);
    unsafe { entry.create_instance(&info, None) }.context("creating the Vulkan instance")
}

fn create_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<(ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    let loader = ext::debug_utils::Instance::new(entry, instance);
    let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(log_validation));
    let messenger = unsafe { loader.create_debug_utils_messenger(&info, None)? };
    tracing::info!("Vulkan validation enabled");
    Ok((loader, messenger))
}

unsafe extern "system" fn log_validation(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _user: *mut c_void,
) -> vk::Bool32 {
    let message = unsafe { (*data).message_as_c_str() }.map(|m| m.to_string_lossy()).unwrap_or_default();
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        tracing::error!(target: "vulkan", "{message}");
    } else {
        tracing::warn!(target: "vulkan", "{message}");
    }
    vk::FALSE
}

fn describe(instance: &ash::Instance, physical: vk::PhysicalDevice) -> GpuInfo {
    let mut driver = vk::PhysicalDeviceDriverProperties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
    unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
    let props = properties.properties;
    let memory = unsafe { instance.get_physical_device_memory_properties(physical) };
    let device_local: u64 = memory.memory_heaps_as_slice()
        .iter()
        .filter(|h| h.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL))
        .map(|h| h.size)
        .sum();

    let extensions = unsafe { instance.enumerate_device_extension_properties(physical) }.unwrap_or_default();
    let has = |name: &CStr| extensions.iter().any(|e| e.extension_name_as_c_str() == Ok(name));

    let name = props.device_name_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let driver_name = driver.driver_name_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let driver_info = driver.driver_info_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let api = props.api_version;

    let unsupported = if vk::api_version_major(api) == 1 && vk::api_version_minor(api) < 3 {
        Some("needs Vulkan 1.3".to_string())
    } else if !has(khr::swapchain::NAME) {
        Some("no swapchain support".to_string())
    } else {
        let missing = missing_features(instance, physical);
        (!missing.is_empty()).then(|| format!("missing {}", missing.join(", ")))
    };

    GpuInfo {
        name,
        kind: props.device_type,
        api_version: (vk::api_version_major(api), vk::api_version_minor(api), vk::api_version_patch(api)),
        driver: format!("{driver_name} {driver_info}").trim().to_string(),
        device_local_mb: device_local / (1024 * 1024),
        mesh_shaders: has(ext::mesh_shader::NAME),
        ray_tracing: has(khr::acceleration_structure::NAME) && has(khr::ray_query::NAME),
        unsupported,
    }
}

fn missing_features(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Vec<&'static str> {
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut v12).push_next(&mut v13);
    unsafe { instance.get_physical_device_features2(physical, &mut features) };
    [
        ("dynamic rendering", v13.dynamic_rendering),
        ("synchronization2", v13.synchronization2),
        ("timeline semaphores", v12.timeline_semaphore),
        ("buffer device address", v12.buffer_device_address),
        ("descriptor indexing", v12.descriptor_indexing),
        ("runtime descriptor arrays", v12.runtime_descriptor_array),
        ("partially bound descriptors", v12.descriptor_binding_partially_bound),
        ("non-uniform image indexing", v12.shader_sampled_image_array_non_uniform_indexing),
    ]
    .into_iter()
    .filter(|(_, supported)| *supported == vk::FALSE)
    .map(|(name, _)| name)
    .collect()
}

fn score(info: &GpuInfo) -> u64 {
    let kind = match info.kind {
        vk::PhysicalDeviceType::DISCRETE_GPU => 1_000_000,
        vk::PhysicalDeviceType::INTEGRATED_GPU => 100_000,
        vk::PhysicalDeviceType::VIRTUAL_GPU => 10_000,
        _ => 0,
    };
    kind + info.device_local_mb
}

fn present_queue_family(
    instance: &ash::Instance,
    surface_loader: &khr::surface::Instance,
    physical: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
) -> Option<u32> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
    families.iter().enumerate().find_map(|(index, family)| {
        let index = index as u32;
        let graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE);
        let present = unsafe { surface_loader.get_physical_device_surface_support(physical, index, surface) };
        (graphics && present == Ok(true)).then_some(index)
    })
}

fn create_device(instance: &ash::Instance, physical: vk::PhysicalDevice, queue_family: u32) -> Result<ash::Device> {
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
    let extensions = [khr::swapchain::NAME.as_ptr()];
    let mut v12 = vk::PhysicalDeviceVulkan12Features::default()
        .timeline_semaphore(true)
        .buffer_device_address(true)
        .descriptor_indexing(true)
        .runtime_descriptor_array(true)
        .descriptor_binding_partially_bound(true)
        .shader_sampled_image_array_non_uniform_indexing(true);
    let mut v13 = vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true).synchronization2(true);
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut v12).push_next(&mut v13);
    let info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut features);
    unsafe { instance.create_device(physical, &info, None) }.context("creating the Vulkan device")
}
