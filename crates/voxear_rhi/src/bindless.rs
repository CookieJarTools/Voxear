//! One global descriptor set holding every texture, storage image and
//! sampler, indexed from shaders, and where the GPU has ray tracing, the
//! scenes' acceleration structures. Buffers need no descriptors at all: shaders
//! reach them through device addresses passed in push constants.

use anyhow::Result;
use ash::vk;

use crate::device::Gpu;

pub const MAX_TEXTURES: u32 = 16 * 1024;
pub const MAX_STORAGE_IMAGES: u32 = 1024;
pub const MAX_SAMPLERS: u32 = 16;
pub const MAX_COMPARISON_SAMPLERS: u32 = 4;
/// Acceleration structures: one a frame in flight for each renderer's view.
pub const MAX_SCENES: u32 = 16;
/// The guaranteed minimum on every Vulkan device.
pub const PUSH_CONSTANT_SIZE: u32 = 128;

/// Indices of the built-in samplers, matching `shaders/bindless.slang`.
pub mod samplers {
    pub const LINEAR_REPEAT: u32 = 0;
    pub const LINEAR_CLAMP: u32 = 1;
    pub const NEAREST_CLAMP: u32 = 2;
    /// Returns the minimum of the footprint; builds depth pyramids.
    pub const MIN_REDUCTION: u32 = 3;

    /// In `comparisonSamplers`: bilinear PCF, lit when the reference depth is
    /// less than or equal to the stored one.
    pub const SHADOW: u32 = 0;
}

const BINDING_TEXTURES: u32 = 0;
const BINDING_SAMPLERS: u32 = 1;
const BINDING_STORAGE_IMAGES: u32 = 2;
const BINDING_COMPARISON_SAMPLERS: u32 = 3;
const BINDING_SCENES: u32 = 4;

/// Index of a sampled texture in the global array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct TextureId(pub u32);

/// Index of a storage image in the global array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct StorageImageId(pub u32);

struct Slots {
    next: u32,
    free: Vec<u32>,
    capacity: u32,
}

impl Slots {
    fn new(capacity: u32) -> Self {
        Self { next: 0, free: Vec::new(), capacity }
    }

    fn take(&mut self) -> u32 {
        if let Some(index) = self.free.pop() {
            return index;
        }
        assert!(self.next < self.capacity, "bindless array is full ({} slots)", self.capacity);
        self.next += 1;
        self.next - 1
    }
}

pub struct Bindless {
    pub layout: vk::DescriptorSetLayout,
    /// The one pipeline layout every pipeline uses: this set plus push constants.
    pub pipeline_layout: vk::PipelineLayout,
    pub set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    textures: Slots,
    storage_images: Slots,
    samplers: Vec<vk::Sampler>,
}

impl Bindless {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let device = &gpu.device;
        let ray_tracing = gpu.acceleration_structure.is_some();
        let mut bindings = vec![
            vk::DescriptorSetLayoutBinding::default()
                .binding(BINDING_TEXTURES)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(MAX_TEXTURES)
                .stage_flags(vk::ShaderStageFlags::ALL),
            vk::DescriptorSetLayoutBinding::default()
                .binding(BINDING_SAMPLERS)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(MAX_SAMPLERS)
                .stage_flags(vk::ShaderStageFlags::ALL),
            vk::DescriptorSetLayoutBinding::default()
                .binding(BINDING_STORAGE_IMAGES)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(MAX_STORAGE_IMAGES)
                .stage_flags(vk::ShaderStageFlags::ALL),
            vk::DescriptorSetLayoutBinding::default()
                .binding(BINDING_COMPARISON_SAMPLERS)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(MAX_COMPARISON_SAMPLERS)
                .stage_flags(vk::ShaderStageFlags::ALL),
        ];
        if ray_tracing {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(BINDING_SCENES)
                    .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                    .descriptor_count(MAX_SCENES)
                    .stage_flags(vk::ShaderStageFlags::ALL),
            );
        }
        let flags = vk::DescriptorBindingFlags::PARTIALLY_BOUND
            | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
            | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING;
        let binding_flags = vec![flags; bindings.len()];
        let mut flags_info = vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);
        let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
            .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
            .bindings(&bindings)
            .push_next(&mut flags_info);

        unsafe {
            let layout = device.create_descriptor_set_layout(&layout_info, None)?;
            let sizes = [
                vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: MAX_TEXTURES },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::SAMPLER,
                    descriptor_count: MAX_SAMPLERS + MAX_COMPARISON_SAMPLERS,
                },
                vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_IMAGE, descriptor_count: MAX_STORAGE_IMAGES },
                vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::ACCELERATION_STRUCTURE_KHR,
                    descriptor_count: MAX_SCENES,
                },
            ];
            let sizes = &sizes[..if ray_tracing { 4 } else { 3 }];
            let pool_info = vk::DescriptorPoolCreateInfo::default()
                .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
                .max_sets(1)
                .pool_sizes(sizes);
            let pool = device.create_descriptor_pool(&pool_info, None)?;
            let layouts = [layout];
            let alloc = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts);
            let set = device.allocate_descriptor_sets(&alloc)?[0];
            gpu.set_name(set, "bindless");

            let ranges =
                [vk::PushConstantRange::default().stage_flags(vk::ShaderStageFlags::ALL).size(PUSH_CONSTANT_SIZE)];
            let pipeline_layout = device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().set_layouts(&layouts).push_constant_ranges(&ranges),
                None,
            )?;

            let mut bindless = Self {
                layout,
                pipeline_layout,
                set,
                pool,
                textures: Slots::new(MAX_TEXTURES),
                storage_images: Slots::new(MAX_STORAGE_IMAGES),
                samplers: Vec::new(),
            };
            bindless.create_default_samplers(gpu)?;
            Ok(bindless)
        }
    }

    fn create_default_samplers(&mut self, gpu: &Gpu) -> Result<()> {
        let linear = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .max_lod(vk::LOD_CLAMP_NONE);
        let repeat = linear.anisotropy_enable(true).max_anisotropy(16.0);
        let clamp = linear
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        let nearest = clamp
            .mag_filter(vk::Filter::NEAREST)
            .min_filter(vk::Filter::NEAREST)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST);
        let mut reduction = vk::SamplerReductionModeCreateInfo::default().reduction_mode(vk::SamplerReductionMode::MIN);
        let min_reduction = clamp.mipmap_mode(vk::SamplerMipmapMode::NEAREST).push_next(&mut reduction);

        for (expected, info) in [
            (samplers::LINEAR_REPEAT, repeat),
            (samplers::LINEAR_CLAMP, clamp),
            (samplers::NEAREST_CLAMP, nearest),
            (samplers::MIN_REDUCTION, min_reduction),
        ] {
            let index = self.add_sampler(gpu, &info)?;
            debug_assert_eq!(index, expected);
        }

        let shadow = clamp
            .compare_enable(true)
            .compare_op(vk::CompareOp::LESS_OR_EQUAL)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST);
        let sampler = gpu.create_sampler(&shadow)?;
        let info = [vk::DescriptorImageInfo::default().sampler(sampler)];
        self.write(gpu, BINDING_COMPARISON_SAMPLERS, samplers::SHADOW, vk::DescriptorType::SAMPLER, &info);
        self.samplers.push(sampler);
        Ok(())
    }

    pub fn add_sampler(&mut self, gpu: &Gpu, info: &vk::SamplerCreateInfo<'_>) -> Result<u32> {
        let index = self.samplers.len() as u32;
        assert!(index < MAX_SAMPLERS, "too many samplers");
        let sampler = gpu.create_sampler(info)?;
        let image_info = [vk::DescriptorImageInfo::default().sampler(sampler)];
        self.write(gpu, BINDING_SAMPLERS, index, vk::DescriptorType::SAMPLER, &image_info);
        self.samplers.push(sampler);
        Ok(index)
    }

    /// Makes an image view sampleable as `textures[id]`.
    pub fn add_texture(&mut self, gpu: &Gpu, view: vk::ImageView) -> TextureId {
        self.add_texture_in(gpu, view, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
    }

    /// Like [`add_texture`](Self::add_texture) for images sampled in another
    /// layout, such as `GENERAL` for images compute also writes.
    pub fn add_texture_in(&mut self, gpu: &Gpu, view: vk::ImageView, layout: vk::ImageLayout) -> TextureId {
        let index = self.textures.take();
        let info = [vk::DescriptorImageInfo::default().image_view(view).image_layout(layout)];
        self.write(gpu, BINDING_TEXTURES, index, vk::DescriptorType::SAMPLED_IMAGE, &info);
        TextureId(index)
    }

    /// Points an existing slot at a new view, e.g. after a resize.
    pub fn set_texture(&mut self, gpu: &Gpu, id: TextureId, view: vk::ImageView) {
        let info = [vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        self.write(gpu, BINDING_TEXTURES, id.0, vk::DescriptorType::SAMPLED_IMAGE, &info);
    }

    /// Makes an image view writable from compute as `storageImages[id]`.
    pub fn add_storage_image(&mut self, gpu: &Gpu, view: vk::ImageView) -> StorageImageId {
        let index = self.storage_images.take();
        let info = [vk::DescriptorImageInfo::default().image_view(view).image_layout(vk::ImageLayout::GENERAL)];
        self.write(gpu, BINDING_STORAGE_IMAGES, index, vk::DescriptorType::STORAGE_IMAGE, &info);
        StorageImageId(index)
    }

    /// Frees a slot. The caller must make sure no frame in flight still uses it.
    pub fn remove_texture(&mut self, id: TextureId) {
        self.textures.free.push(id.0);
    }

    pub fn remove_storage_image(&mut self, id: StorageImageId) {
        self.storage_images.free.push(id.0);
    }

    /// Puts a scene's acceleration structure in slot `index` of `scenes`,
    /// for ray queries. Only where the GPU has ray tracing.
    pub fn set_scene(&self, gpu: &Gpu, index: u32, scene: vk::AccelerationStructureKHR) {
        assert!(index < MAX_SCENES, "only {MAX_SCENES} scenes");
        let structures = [scene];
        let mut info = vk::WriteDescriptorSetAccelerationStructureKHR::default().acceleration_structures(&structures);
        let mut write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(BINDING_SCENES)
            .dst_array_element(index)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .push_next(&mut info);
        write.descriptor_count = 1;
        unsafe { gpu.device.update_descriptor_sets(&[write], &[]) };
    }

    pub fn bind(&self, gpu: &Gpu, cmd: vk::CommandBuffer, bind_point: vk::PipelineBindPoint) {
        unsafe { gpu.device.cmd_bind_descriptor_sets(cmd, bind_point, self.pipeline_layout, 0, &[self.set], &[]) };
    }

    /// Pushes up to [`PUSH_CONSTANT_SIZE`] bytes, visible to every stage.
    pub fn push<T: bytemuck::Pod>(&self, gpu: &Gpu, cmd: vk::CommandBuffer, data: &T) {
        let bytes = bytemuck::bytes_of(data);
        assert!(bytes.len() as u32 <= PUSH_CONSTANT_SIZE, "push constants over {PUSH_CONSTANT_SIZE} bytes");
        unsafe { gpu.device.cmd_push_constants(cmd, self.pipeline_layout, vk::ShaderStageFlags::ALL, 0, bytes) };
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        unsafe {
            for sampler in self.samplers.drain(..) {
                gpu.device.destroy_sampler(sampler, None);
            }
            gpu.device.destroy_pipeline_layout(self.pipeline_layout, None);
            gpu.device.destroy_descriptor_pool(self.pool, None);
            gpu.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }

    fn write(&self, gpu: &Gpu, binding: u32, index: u32, kind: vk::DescriptorType, info: &[vk::DescriptorImageInfo]) {
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(binding)
            .dst_array_element(index)
            .descriptor_type(kind)
            .image_info(info);
        unsafe { gpu.device.update_descriptor_sets(&[write], &[]) };
    }
}
