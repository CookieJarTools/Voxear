use std::{
    ffi::{CStr, CString, c_char, c_void},
    mem::ManuallyDrop,
    sync::Mutex,
};

use anyhow::{Context, Result, bail};
use ash::{ext, khr, nv, vk};
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
    /// GDeflate decompression on the GPU (`VK_NV_memory_decompression`).
    pub memory_decompression: bool,
    /// Nanoseconds per timestamp tick.
    pub timestamp_period: f32,
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
    /// Decompresses GDeflate on the GPU, where the driver can. Set
    /// `VOXEAR_GPU_DECOMPRESSION=0` to decompress on the CPU instead.
    pub decompression: Option<nv::memory_decompression::Device>,
    /// Names and labels for RenderDoc, Nsight and validation messages.
    debug_utils: Option<ext::debug_utils::Device>,
    messenger: Option<(ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
}

/// A window to present to.
#[derive(Clone, Copy)]
pub struct WindowHandles {
    pub display: RawDisplayHandle,
    pub window: RawWindowHandle,
}

impl Gpu {
    /// Creates the device and a surface for the window, picking the best GPU
    /// that can present to it.
    ///
    /// # Safety
    /// The window handles must stay valid until the surface is destroyed.
    pub unsafe fn new(window: WindowHandles) -> Result<(Self, vk::SurfaceKHR)> {
        let (gpu, surface) = unsafe { Self::create(Some(window))? };
        Ok((gpu, surface.expect("a window was given")))
    }

    /// Creates a device with no window, for offscreen rendering and tools.
    pub fn new_headless() -> Result<Self> {
        Ok(unsafe { Self::create(None)? }.0)
    }

    unsafe fn create(window: Option<WindowHandles>) -> Result<(Self, Option<vk::SurfaceKHR>)> {
        let entry = unsafe { ash::Entry::load() }.context("the Vulkan loader is not installed")?;
        let validation = wants_validation(&entry);
        let debug_utils = has_instance_extension(&entry, ext::debug_utils::NAME);
        let mut extensions = match window {
            Some(w) => ash_window::enumerate_required_extensions(w.display)?.to_vec(),
            None => vec![khr::surface::NAME.as_ptr()],
        };
        if debug_utils {
            extensions.push(ext::debug_utils::NAME.as_ptr());
        }
        let instance = create_instance(&entry, &extensions, validation)?;
        let messenger = (validation && debug_utils).then(|| create_messenger(&entry, &instance)).transpose()?;

        let surface = match window {
            Some(w) => Some(unsafe { ash_window::create_surface(&entry, &instance, w.display, w.window, None)? }),
            None => None,
        };
        let surface_loader = khr::surface::Instance::new(&entry, &instance);

        let candidates = unsafe { instance.enumerate_physical_devices()? };
        let mut best: Option<(u64, vk::PhysicalDevice, u32, GpuInfo)> = None;
        for physical in candidates {
            let info = describe(&instance, physical);
            if let Some(reason) = &info.unsupported {
                tracing::info!("skipping {}: {reason}", info.name);
                continue;
            }
            let Some(family) = queue_family(&instance, &surface_loader, physical, surface) else {
                tracing::info!("skipping {}: no suitable queue", info.name);
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

        let decompress =
            info.memory_decompression && std::env::var("VOXEAR_GPU_DECOMPRESSION").map(|v| v != "0").unwrap_or(true);
        let device = create_device(&instance, physical, queue_family, decompress)?;
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
        if decompress {
            tracing::info!("GPU decompression: GDeflate");
        }
        let gpu = Self {
            debug_utils: debug_utils.then(|| ext::debug_utils::Device::new(&instance, &device)),
            decompression: decompress.then(|| nv::memory_decompression::Device::new(&instance, &device)),
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
            messenger,
        };
        Ok((gpu, surface))
    }

    /// A surface for another window on the display the device was made
    /// for, to present to as well, as a panel the editor floats on the
    /// desktop is.
    ///
    /// # Safety
    /// The window handles must stay valid until the surface is destroyed.
    pub unsafe fn create_surface(&self, window: WindowHandles) -> Result<vk::SurfaceKHR> {
        let surface =
            unsafe { ash_window::create_surface(&self.entry, &self.instance, window.display, window.window, None)? };
        let presents = unsafe {
            self.surface_loader.get_physical_device_surface_support(self.physical, self.queue_family, surface)
        };
        if presents != Ok(true) {
            unsafe { self.surface_loader.destroy_surface(surface, None) };
            bail!("{} cannot show images in this window", self.info.name);
        }
        Ok(surface)
    }

    pub fn wait_idle(&self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
    }

    /// Names a Vulkan object so captures and validation messages read like
    /// the engine's own structure.
    pub fn set_name<H: vk::Handle>(&self, handle: H, name: &str) {
        let Some(debug) = &self.debug_utils else { return };
        let Ok(name) = CString::new(name) else { return };
        let info = vk::DebugUtilsObjectNameInfoEXT::default().object_handle(handle).object_name(&name);
        unsafe {
            let _ = debug.set_debug_utils_object_name(&info);
        }
    }

    /// Opens a named region in a command buffer, shown as a group in GPU tools.
    pub fn begin_label(&self, cmd: vk::CommandBuffer, name: &str, color: [f32; 4]) {
        let Some(debug) = &self.debug_utils else { return };
        let Ok(name) = CString::new(name) else { return };
        let label = vk::DebugUtilsLabelEXT::default().label_name(&name).color(color);
        unsafe { debug.cmd_begin_debug_utils_label(cmd, &label) };
    }

    pub fn end_label(&self, cmd: vk::CommandBuffer) {
        if let Some(debug) = &self.debug_utils {
            unsafe { debug.cmd_end_debug_utils_label(cmd) };
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            self.wait_idle();
            ManuallyDrop::drop(&mut self.allocator);
            self.device.destroy_device(None);
            if let Some((loader, messenger)) = self.messenger.take() {
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

fn has_instance_extension(entry: &ash::Entry, name: &CStr) -> bool {
    let extensions = unsafe { entry.enumerate_instance_extension_properties(None) }.unwrap_or_default();
    extensions.iter().any(|e| e.extension_name_as_c_str() == Ok(name))
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
    let missing = missing_instance_extensions(entry, extensions);
    if !missing.is_empty() {
        bail!("the Vulkan driver or an active layer does not offer {}", missing.join(", "));
    }
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

/// Requested instance extensions the loader does not list, by name. Layers
/// such as RenderDoc's can hide extensions they do not support.
fn missing_instance_extensions(entry: &ash::Entry, requested: &[*const c_char]) -> Vec<String> {
    let available = unsafe { entry.enumerate_instance_extension_properties(None) }.unwrap_or_default();
    requested
        .iter()
        .map(|&name| unsafe { CStr::from_ptr(name) })
        .filter(|name| !available.iter().any(|e| e.extension_name_as_c_str() == Ok(*name)))
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}

fn create_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<(ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)> {
    let loader = ext::debug_utils::Instance::new(entry, instance);
    let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR)
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
    let mut v11 = vk::PhysicalDeviceVulkan11Properties::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut driver).push_next(&mut v11);
    unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
    let props = properties.properties;
    // Culling compacts its draws a wave at a time.
    let waves = vk::SubgroupFeatureFlags::BASIC | vk::SubgroupFeatureFlags::BALLOT;
    let compute_waves = v11.subgroup_supported_stages.contains(vk::ShaderStageFlags::COMPUTE)
        && v11.subgroup_supported_operations.contains(waves);
    let memory = unsafe { instance.get_physical_device_memory_properties(physical) };
    let device_local: u64 = memory
        .memory_heaps_as_slice()
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
    } else if props.limits.timestamp_compute_and_graphics == vk::FALSE {
        Some("no GPU timestamps".to_string())
    } else if !compute_waves {
        Some("no subgroup ballots in compute shaders".to_string())
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
        memory_decompression: has(nv::memory_decompression::NAME) && supports_decompression(instance, physical),
        timestamp_period: props.limits.timestamp_period,
        unsupported,
    }
}

/// The feature set Voxear requires. Every entry is supported by current
/// NVIDIA, AMD and Intel drivers on Linux and Windows.
struct Features<'a> {
    core: vk::PhysicalDeviceFeatures2<'a>,
    v11: vk::PhysicalDeviceVulkan11Features<'a>,
    v12: vk::PhysicalDeviceVulkan12Features<'a>,
    v13: vk::PhysicalDeviceVulkan13Features<'a>,
}

impl Features<'_> {
    fn required() -> Self {
        let core = vk::PhysicalDeviceFeatures::default()
            .multi_draw_indirect(true)
            .draw_indirect_first_instance(true)
            .sampler_anisotropy(true)
            .shader_int64(true)
            .fill_mode_non_solid(true)
            .depth_clamp(true)
            .independent_blend(true)
            .texture_compression_bc(true);
        Self {
            core: vk::PhysicalDeviceFeatures2::default().features(core),
            v11: vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true),
            v12: vk::PhysicalDeviceVulkan12Features::default()
                .timeline_semaphore(true)
                .buffer_device_address(true)
                .scalar_block_layout(true)
                .draw_indirect_count(true)
                .sampler_filter_minmax(true)
                .host_query_reset(true)
                .descriptor_indexing(true)
                .runtime_descriptor_array(true)
                .descriptor_binding_partially_bound(true)
                .descriptor_binding_update_unused_while_pending(true)
                .descriptor_binding_sampled_image_update_after_bind(true)
                .descriptor_binding_storage_image_update_after_bind(true)
                .shader_sampled_image_array_non_uniform_indexing(true)
                .shader_storage_image_array_non_uniform_indexing(true),
            v13: vk::PhysicalDeviceVulkan13Features::default()
                .dynamic_rendering(true)
                .synchronization2(true)
                .shader_demote_to_helper_invocation(true),
        }
    }

    fn query(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Self {
        let mut features = Self {
            core: vk::PhysicalDeviceFeatures2::default(),
            v11: vk::PhysicalDeviceVulkan11Features::default(),
            v12: vk::PhysicalDeviceVulkan12Features::default(),
            v13: vk::PhysicalDeviceVulkan13Features::default(),
        };
        let mut chain = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut features.v11)
            .push_next(&mut features.v12)
            .push_next(&mut features.v13);
        unsafe { instance.get_physical_device_features2(physical, &mut chain) };
        features.core.features = chain.features;
        features
    }

    /// Pairs of (name, required, supported).
    fn compare(&self, supported: &Self) -> Vec<(&'static str, bool, bool)> {
        let (r, s) = (self, supported);
        let on = |v: vk::Bool32| v == vk::TRUE;
        vec![
            ("multi-draw indirect", on(r.core.features.multi_draw_indirect), on(s.core.features.multi_draw_indirect)),
            (
                "indirect first instance",
                on(r.core.features.draw_indirect_first_instance),
                on(s.core.features.draw_indirect_first_instance),
            ),
            ("anisotropic filtering", on(r.core.features.sampler_anisotropy), on(s.core.features.sampler_anisotropy)),
            ("64-bit shader integers", on(r.core.features.shader_int64), on(s.core.features.shader_int64)),
            ("wireframe", on(r.core.features.fill_mode_non_solid), on(s.core.features.fill_mode_non_solid)),
            ("depth clamp", on(r.core.features.depth_clamp), on(s.core.features.depth_clamp)),
            ("independent blend", on(r.core.features.independent_blend), on(s.core.features.independent_blend)),
            (
                "BC texture compression",
                on(r.core.features.texture_compression_bc),
                on(s.core.features.texture_compression_bc),
            ),
            ("shader draw parameters", on(r.v11.shader_draw_parameters), on(s.v11.shader_draw_parameters)),
            ("timeline semaphores", on(r.v12.timeline_semaphore), on(s.v12.timeline_semaphore)),
            ("buffer device address", on(r.v12.buffer_device_address), on(s.v12.buffer_device_address)),
            ("scalar block layout", on(r.v12.scalar_block_layout), on(s.v12.scalar_block_layout)),
            ("indirect draw count", on(r.v12.draw_indirect_count), on(s.v12.draw_indirect_count)),
            ("min/max samplers", on(r.v12.sampler_filter_minmax), on(s.v12.sampler_filter_minmax)),
            ("host query reset", on(r.v12.host_query_reset), on(s.v12.host_query_reset)),
            ("descriptor indexing", on(r.v12.descriptor_indexing), on(s.v12.descriptor_indexing)),
            ("runtime descriptor arrays", on(r.v12.runtime_descriptor_array), on(s.v12.runtime_descriptor_array)),
            (
                "partially bound descriptors",
                on(r.v12.descriptor_binding_partially_bound),
                on(s.v12.descriptor_binding_partially_bound),
            ),
            (
                "update-unused-while-pending descriptors",
                on(r.v12.descriptor_binding_update_unused_while_pending),
                on(s.v12.descriptor_binding_update_unused_while_pending),
            ),
            (
                "update-after-bind sampled images",
                on(r.v12.descriptor_binding_sampled_image_update_after_bind),
                on(s.v12.descriptor_binding_sampled_image_update_after_bind),
            ),
            (
                "update-after-bind storage images",
                on(r.v12.descriptor_binding_storage_image_update_after_bind),
                on(s.v12.descriptor_binding_storage_image_update_after_bind),
            ),
            (
                "non-uniform image indexing",
                on(r.v12.shader_sampled_image_array_non_uniform_indexing),
                on(s.v12.shader_sampled_image_array_non_uniform_indexing),
            ),
            (
                "non-uniform storage image indexing",
                on(r.v12.shader_storage_image_array_non_uniform_indexing),
                on(s.v12.shader_storage_image_array_non_uniform_indexing),
            ),
            ("dynamic rendering", on(r.v13.dynamic_rendering), on(s.v13.dynamic_rendering)),
            ("synchronization2", on(r.v13.synchronization2), on(s.v13.synchronization2)),
            (
                "demote to helper invocation",
                on(r.v13.shader_demote_to_helper_invocation),
                on(s.v13.shader_demote_to_helper_invocation),
            ),
        ]
    }
}

fn missing_features(instance: &ash::Instance, physical: vk::PhysicalDevice) -> Vec<&'static str> {
    let supported = Features::query(instance, physical);
    Features::required()
        .compare(&supported)
        .into_iter()
        .filter(|(_, required, supported)| *required && !*supported)
        .map(|(name, _, _)| name)
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

/// A graphics + compute queue family, which must also present when there is
/// a surface.
fn queue_family(
    instance: &ash::Instance,
    surface_loader: &khr::surface::Instance,
    physical: vk::PhysicalDevice,
    surface: Option<vk::SurfaceKHR>,
) -> Option<u32> {
    let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
    families.iter().enumerate().find_map(|(index, family)| {
        let index = index as u32;
        let graphics = family.queue_flags.contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE);
        let timestamps = family.timestamp_valid_bits > 0;
        let present = surface.is_none_or(|surface| unsafe {
            surface_loader.get_physical_device_surface_support(physical, index, surface) == Ok(true)
        });
        (graphics && timestamps && present).then_some(index)
    })
}

/// Whether the GPU can decompress GDeflate, the format cooked models ship in.
fn supports_decompression(instance: &ash::Instance, physical: vk::PhysicalDevice) -> bool {
    let mut decompression = vk::PhysicalDeviceMemoryDecompressionFeaturesNV::default();
    let mut features = vk::PhysicalDeviceFeatures2::default().push_next(&mut decompression);
    unsafe { instance.get_physical_device_features2(physical, &mut features) };
    let mut methods = vk::PhysicalDeviceMemoryDecompressionPropertiesNV::default();
    let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut methods);
    unsafe { instance.get_physical_device_properties2(physical, &mut properties) };
    decompression.memory_decompression == vk::TRUE
        && methods.decompression_methods.contains(vk::MemoryDecompressionMethodFlagsNV::GDEFLATE_1_0)
}

fn create_device(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    queue_family: u32,
    decompression: bool,
) -> Result<ash::Device> {
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family).queue_priorities(&priorities)];
    let mut extensions = vec![khr::swapchain::NAME.as_ptr()];
    let mut features = Features::required();
    let mut chain =
        features.core.push_next(&mut features.v11).push_next(&mut features.v12).push_next(&mut features.v13);
    let mut decompress = vk::PhysicalDeviceMemoryDecompressionFeaturesNV::default().memory_decompression(true);
    if decompression {
        extensions.push(nv::memory_decompression::NAME.as_ptr());
        chain = chain.push_next(&mut decompress);
    }
    let info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queues)
        .enabled_extension_names(&extensions)
        .push_next(&mut chain);
    unsafe { instance.create_device(physical, &info, None) }.context("creating the Vulkan device")
}
