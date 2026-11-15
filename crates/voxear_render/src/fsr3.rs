//! AMD FSR 3.1's temporal upscaler, as a choice beside Voxear's own TAA
//! (`Upscaler::Fsr`), from AMD's MIT-licensed HLSL in `fsr3/amd` (see its
//! README). AMD ships FSR's host code and compiled shaders for Windows only,
//! so Voxear compiles the passes to SPIR-V with slangc itself (`build.rs`) and
//! runs them here, in place of AMD's host library.
//!
//! The seven passes of one frame are recorded in one pass of the frame graph,
//! which sees only what comes in (the scene as drawn, its depth, its motion and
//! the exposure) and the picture that goes out, at the output's size. FSR's own
//! images, its history among them, are the view's (`Fsr3State`), kept in the
//! general layout, with a barrier between dispatches. Each pass binds its
//! resources by name, as slangc's reflection of AMD's shaders names them
//! (`FSR3_PASSES`, made by `build.rs`), through descriptor sets of its own.

use anyhow::{Context, Result};
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Gpu, Image, ImageDesc, MemoryLocation, ShaderModule, vk};

/// What a binding of a pass is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fsr3Binding {
    Uniform,
    Sampler,
    Texture,
    Storage,
}

/// A pass of FSR 3.1 as compiled: its SPIR-V, its thread group, and each of
/// its resources' name, kind and binding in set 0.
pub(crate) struct Fsr3PassInfo {
    pub name: &'static str,
    pub spirv: &'static [u8],
    /// Its thread group's size, which the dispatches above assume: checked by the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub group: [u32; 3],
    pub bindings: &'static [(&'static str, Fsr3Binding, u32)],
}

include!(concat!(env!("OUT_DIR"), "/fsr3_passes.rs"));

const PREPARE_INPUTS: usize = 0;
const LUMA_PYRAMID: usize = 1;
const SHADING_CHANGE_PYRAMID: usize = 2;
const SHADING_CHANGE: usize = 3;
const PREPARE_REACTIVITY: usize = 4;
const LUMA_INSTABILITY: usize = 5;
const ACCUMULATE: usize = 6;
const AUTOGEN_REACTIVE: usize = 7;

/// Where each constant buffer is in a frame's part of the constants buffer.
const FSR3_CONSTANTS: u64 = 0;
const SPD_CONSTANTS: u64 = 256;
const REACTIVE_CONSTANTS: u64 = 384;
const CONSTANTS_PER_FRAME: u64 = 512;

/// Every pass's pipeline, set layout and layout, and the two samplers AMD's
/// shaders name, which the set layouts hold.
#[derive(Clone, Copy)]
pub(crate) struct Fsr3Pipelines {
    passes: [(vk::DescriptorSetLayout, vk::PipelineLayout, vk::Pipeline); 8],
    samplers: [vk::Sampler; 2],
    /// Each frame in flight's descriptor sets, for every view, reset as the
    /// frame begins (`reset_sets`).
    pools: [vk::DescriptorPool; FRAMES_IN_FLIGHT],
}

impl Fsr3Pipelines {
    pub fn new(gpu: &Gpu) -> Result<Self> {
        let sampler = |filter: vk::Filter| {
            let info = vk::SamplerCreateInfo::default()
                .mag_filter(filter)
                .min_filter(filter)
                .mipmap_mode(if filter == vk::Filter::LINEAR {
                    vk::SamplerMipmapMode::LINEAR
                } else {
                    vk::SamplerMipmapMode::NEAREST
                })
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .max_lod(vk::LOD_CLAMP_NONE);
            unsafe { gpu.device.create_sampler(&info, None) }.context("creating FSR's samplers")
        };
        let samplers = [sampler(vk::Filter::NEAREST)?, sampler(vk::Filter::LINEAR)?];
        let mut passes = [(vk::DescriptorSetLayout::null(), vk::PipelineLayout::null(), vk::Pipeline::null()); 8];
        for (made, info) in passes.iter_mut().zip(&FSR3_PASSES) {
            let immutable: Vec<[vk::Sampler; 1]> = info
                .bindings
                .iter()
                .map(|(name, ..)| [if *name == "s_LinearClamp" { samplers[1] } else { samplers[0] }])
                .collect();
            let bindings: Vec<vk::DescriptorSetLayoutBinding> = info
                .bindings
                .iter()
                .zip(&immutable)
                .map(|(&(_, kind, binding), sampler)| {
                    let layout = vk::DescriptorSetLayoutBinding::default()
                        .binding(binding)
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                        .descriptor_type(descriptor_type(kind));
                    match kind {
                        Fsr3Binding::Sampler => layout.immutable_samplers(sampler),
                        _ => layout,
                    }
                })
                .collect();
            let set = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
            let set = unsafe { gpu.device.create_descriptor_set_layout(&set, None)? };
            let sets = [set];
            let layout = unsafe {
                gpu.device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&sets), None)?
            };
            let name = format!("FSR 3 {}", info.name.replace('_', " "));
            let module = ShaderModule::new(gpu, &name, info.spirv)?;
            let pipeline = voxear_rhi::create_compute_pipeline(gpu, layout, &name, &module, c"main");
            module.destroy(gpu);
            *made = (set, layout, pipeline?);
        }
        let sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: 128 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLER, descriptor_count: 128 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: 512 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::STORAGE_IMAGE, descriptor_count: 512 },
        ];
        let mut pools = [vk::DescriptorPool::null(); FRAMES_IN_FLIGHT];
        for pool in &mut pools {
            let info = vk::DescriptorPoolCreateInfo::default().max_sets(64).pool_sizes(&sizes);
            *pool = unsafe { gpu.device.create_descriptor_pool(&info, None)? };
        }
        Ok(Self { passes, samplers, pools })
    }

    /// Frees a frame in flight's descriptor sets, as its slot comes round
    /// again and the GPU is done with them.
    pub fn reset_sets(&self, gpu: &Gpu, slot: usize) {
        unsafe { gpu.device.reset_descriptor_pool(self.pools[slot], vk::DescriptorPoolResetFlags::empty()).ok() };
    }

    pub fn destroy(&self, gpu: &Gpu) {
        unsafe {
            for (set, layout, pipeline) in self.passes {
                gpu.device.destroy_pipeline(pipeline, None);
                gpu.device.destroy_pipeline_layout(layout, None);
                gpu.device.destroy_descriptor_set_layout(set, None);
            }
            for sampler in self.samplers {
                gpu.device.destroy_sampler(sampler, None);
            }
            for pool in self.pools {
                gpu.device.destroy_descriptor_pool(pool, None);
            }
        }
    }
}

fn descriptor_type(kind: Fsr3Binding) -> vk::DescriptorType {
    match kind {
        Fsr3Binding::Uniform => vk::DescriptorType::UNIFORM_BUFFER,
        Fsr3Binding::Sampler => vk::DescriptorType::SAMPLER,
        Fsr3Binding::Texture => vk::DescriptorType::SAMPLED_IMAGE,
        Fsr3Binding::Storage => vk::DescriptorType::STORAGE_IMAGE,
    }
}

/// What one frame of FSR is given: the views of what comes in and goes out,
/// and how the frame was drawn.
pub(crate) struct Fsr3Frame {
    pub color: vk::ImageView,
    /// The scene before smoke, particles and the like were drawn over it,
    /// from which the reactive mask is made: what changed tells FSR to keep
    /// less of its history there. None where nothing was drawn over it.
    pub opaque: Option<vk::ImageView>,
    pub depth: vk::ImageView,
    pub motion: vk::ImageView,
    pub exposure: vk::ImageView,
    pub output: vk::ImageView,
    /// The scene's size as drawn, and the output's.
    pub render: [u32; 2],
    pub upscale: [u32; 2],
    /// The camera's sub-pixel offset this frame, in drawn pixels.
    pub jitter: [f32; 2],
    pub fov_y: f32,
    pub near: f32,
    pub delta_time: f32,
    /// History is thrown away, as on a camera cut.
    pub reset: bool,
    pub slot: usize,
}

/// A view's FSR: its images, which hold its history from frame to frame,
/// and the descriptor pools and constants of each frame in flight.
pub(crate) struct Fsr3State {
    render: [u32; 2],
    upscale: [u32; 2],
    images: Vec<Image>,
    constants: Buffer,
    /// The images are new, in no layout yet, their history unknown.
    fresh: bool,
    resource_frame: u64,
    frame_index: f32,
    jitter: [f32; 2],
    previous_render: [u32; 2],
    previous_upscale: [u32; 2],
    jitter_phases: f32,
}

// The images, in the order `Fsr3State::new` makes them.
const LUMA: usize = 0; // and 1
const ACCUMULATION: usize = 2; // and 3
const UPSCALED: usize = 4; // and 5
const LUMA_HISTORY: usize = 6; // and 7
const FRAME_INFO: usize = 8;
const SPD_ATOMIC: usize = 9;
const SPD_MIPS: usize = 10;
const RECONSTRUCTED_DEPTH: usize = 11;
const DILATED_DEPTH: usize = 12;
const DILATED_MOTION: usize = 13;
const INTERMEDIATE: usize = 14;
const SHADING_CHANGE_IMAGE: usize = 15;
const NEW_LOCKS: usize = 16;
const FARTHEST_MIP1: usize = 17;
const DILATED_REACTIVE: usize = 18;
const DEFAULT_REACTIVE: usize = 19;
const AUTOREACTIVE: usize = 20;
const LANCZOS: usize = 21;

impl Fsr3State {
    pub fn new(gpu: &Gpu, render: [u32; 2], upscale: [u32; 2]) -> Result<Self> {
        let storage = vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
        let half = render.map(|side| (side / 2).max(1));
        use vk::Format as F;
        let plain: [(&str, [u32; 2], F); 21] = [
            ("FSR luma 1", render, F::R16_SFLOAT),
            ("FSR luma 2", render, F::R16_SFLOAT),
            ("FSR accumulation 1", render, F::R8_UNORM),
            ("FSR accumulation 2", render, F::R8_UNORM),
            ("FSR upscaled 1", upscale, F::R16G16B16A16_SFLOAT),
            ("FSR upscaled 2", upscale, F::R16G16B16A16_SFLOAT),
            ("FSR luma history 1", render, F::R16G16B16A16_SFLOAT),
            ("FSR luma history 2", render, F::R16G16B16A16_SFLOAT),
            ("FSR frame info", [1, 1], F::R32G32B32A32_SFLOAT),
            ("FSR SPD atomic", [1, 1], F::R32_UINT),
            ("FSR SPD mips", half, F::R16G16_SFLOAT),
            ("FSR reconstructed depth", render, F::R32_UINT),
            ("FSR dilated depth", render, F::R32_SFLOAT),
            ("FSR dilated motion", render, F::R16G16_SFLOAT),
            ("FSR intermediate", render, F::R16_SFLOAT),
            ("FSR shading change", half, F::R8_UNORM),
            ("FSR new locks", upscale, F::R8_UNORM),
            ("FSR farthest depth mip 1", half, F::R16_SFLOAT),
            ("FSR dilated reactive masks", render, F::R8G8B8A8_UNORM),
            ("FSR default reactivity", [1, 1], F::R8_UNORM),
            ("FSR reactive mask", render, F::R8_UNORM),
        ];
        let mut images = Vec::with_capacity(22);
        for (index, (name, [width, height], format)) in plain.into_iter().enumerate() {
            let mut desc = ImageDesc::new(width, height, format, storage);
            if index == SPD_MIPS {
                desc = desc.with_mips();
            }
            images.push(gpu.create_image(name, desc)?);
        }
        // Lanczos 2 weights across 0 to 2, as AMD's host code fills them.
        let lanczos: Vec<u8> = (0..128)
            .flat_map(|i| {
                let x = 2.0 * i as f32 / 127.0;
                ((lanczos2(x) * 32767.0).round() as i16).to_le_bytes()
            })
            .collect();
        let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
        let lut = gpu.create_image("FSR Lanczos", ImageDesc::new(128, 1, F::R16_SNORM, usage))?;
        gpu.upload_image(&lut, &lanczos)?;
        images.push(lut);
        let constants = gpu.create_buffer(
            "FSR constants",
            CONSTANTS_PER_FRAME * FRAMES_IN_FLIGHT as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            MemoryLocation::CpuToGpu,
        )?;
        Ok(Self {
            render,
            upscale,
            images,
            constants,
            fresh: true,
            resource_frame: 0,
            frame_index: 0.0,
            jitter: [0.0; 2],
            previous_render: render,
            previous_upscale: upscale,
            jitter_phases: 0.0,
        })
    }

    /// Whether it was made for these sizes.
    pub fn fits(&self, render: [u32; 2], upscale: [u32; 2]) -> bool {
        self.render == render && self.upscale == upscale
    }

    /// Hands its images and constants over to be freed once no frame in
    /// flight uses them: as sizes change, or as the view goes.
    pub fn retire(self, frames: &mut voxear_rhi::Frames) {
        for image in self.images {
            frames.retire(voxear_rhi::Retired::Image(image));
        }
        frames.retire(voxear_rhi::Retired::Buffer(self.constants));
    }

    /// Records one frame of FSR into `cmd`: the inputs in the layouts the
    /// frame graph left them in (sampled), the output in the general layout.
    pub fn record(&mut self, gpu: &Gpu, cmd: vk::CommandBuffer, pipelines: &Fsr3Pipelines, frame: &Fsr3Frame) {
        let device = &gpu.device;
        let pool = pipelines.pools[frame.slot];
        let reset = frame.reset || self.fresh;
        let everything = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(vk::REMAINING_MIP_LEVELS)
            .layer_count(1);
        // New images start in the general layout, where FSR keeps them.
        if self.fresh {
            let barriers: Vec<vk::ImageMemoryBarrier2> = self.images[..LANCZOS]
                .iter()
                .map(|image| {
                    vk::ImageMemoryBarrier2::default()
                        .image(image.handle)
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::GENERAL)
                        .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                        .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                        .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
                        .subresource_range(everything)
                })
                .collect();
            unsafe {
                device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barriers))
            };
        }
        let odd = self.resource_frame & 1 == 1;
        let (accumulation_read, accumulation_write) =
            if odd { (ACCUMULATION + 1, ACCUMULATION) } else { (ACCUMULATION, ACCUMULATION + 1) };
        let (upscaled_read, upscaled_write) = if odd { (UPSCALED + 1, UPSCALED) } else { (UPSCALED, UPSCALED + 1) };
        let (history_read, history_write) =
            if odd { (LUMA_HISTORY + 1, LUMA_HISTORY) } else { (LUMA_HISTORY, LUMA_HISTORY + 1) };
        let (luma_now, luma_before) = if odd { (LUMA + 1, LUMA) } else { (LUMA, LUMA + 1) };

        // What FSR clears, as its host code does: all its history when it is new or thrown away, and what is
        // built up again each frame.
        let clear = |image: usize, value: [f32; 4]| unsafe {
            let colour = vk::ClearColorValue { float32: value };
            device.cmd_clear_color_image(
                cmd,
                self.images[image].handle,
                vk::ImageLayout::GENERAL,
                &colour,
                &[everything],
            );
        };
        if self.fresh {
            for image in [ACCUMULATION, ACCUMULATION + 1, LUMA, LUMA + 1, DEFAULT_REACTIVE] {
                clear(image, [0.0; 4]);
            }
        }
        if reset {
            clear(accumulation_read, [0.0; 4]);
            clear(FRAME_INFO, [-1.0, 1.0, 0.0, 0.0]);
        }
        // Reverse-Z: the nearest depth is the largest, and nothing reconstructed is 0.
        for image in [RECONSTRUCTED_DEPTH, SPD_ATOMIC, SPD_MIPS, NEW_LOCKS] {
            clear(image, [0.0; 4]);
        }
        let after_clears = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER | vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE);
        unsafe { device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[after_clears])) };

        let constants = self.constants(frame, reset);
        let offset = CONSTANTS_PER_FRAME * frame.slot as u64;
        self.constants.write(offset + FSR3_CONSTANTS, &constants[..]);
        let (spd, groups) = spd_setup(frame.render);
        self.constants.write(offset + SPD_CONSTANTS, &spd[..]);
        // As AMD's sample makes its mask: tonemapped, the largest channel's change, and past a fifth, nine tenths.
        let reactive = [1.0f32.to_bits(), 0.2f32.to_bits(), 0.9f32.to_bits(), 1 | 4 | 8];
        self.constants.write(offset + REACTIVE_CONSTANTS, &reactive[..]);
        let reactive_mask = if frame.opaque.is_some() { AUTOREACTIVE } else { DEFAULT_REACTIVE };

        let spd = &self.images[SPD_MIPS];
        let view = |name: &str| -> Option<(vk::ImageView, vk::ImageLayout)> {
            let general = vk::ImageLayout::GENERAL;
            let read = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
            let own = |image: usize| Some((self.images[image].view, general));
            // An image of one mip has no views of its mips: its own view is its only mip's.
            if let Some(mip) = name.strip_prefix("rw_spd_mip").and_then(|m| m.parse::<usize>().ok()) {
                let mip_view = spd.mip_views.get(mip.min(spd.mip_views.len().saturating_sub(1))).copied();
                return Some((mip_view.unwrap_or(spd.view), general));
            }
            match name {
                "r_input_color_jittered" => Some((frame.color, read)),
                "r_input_depth" => Some((frame.depth, read)),
                "r_input_motion_vectors" => Some((frame.motion, read)),
                "r_input_exposure" => Some((frame.exposure, read)),
                "rw_upscaled_output" => Some((frame.output, general)),
                "r_reactive_mask" => own(reactive_mask),
                "r_transparency_and_composition_mask" => own(DEFAULT_REACTIVE),
                "r_input_opaque_only" => frame.opaque.map(|opaque| (opaque, read)),
                "rw_output_autoreactive" => own(AUTOREACTIVE),
                // FSR's mask of what is composed over the scene: left empty, as the default is.
                "rw_output_autocomposition" => own(DILATED_REACTIVE),
                "r_dilated_motion_vectors" | "rw_dilated_motion_vectors" => own(DILATED_MOTION),
                "r_dilated_depth" | "rw_dilated_depth" => own(DILATED_DEPTH),
                "r_reconstructed_previous_nearest_depth" | "rw_reconstructed_previous_nearest_depth" => {
                    own(RECONSTRUCTED_DEPTH)
                }
                // FSR keeps the farthest depth and the luma instability in one image, one after the other.
                "r_farthest_depth" | "rw_farthest_depth" | "r_luma_instability" | "rw_luma_instability" => {
                    own(INTERMEDIATE)
                }
                "r_current_luma" | "rw_current_luma" => own(luma_now),
                "r_previous_luma" => own(luma_before),
                "r_farthest_depth_mip1" | "rw_farthest_depth_mip1" => own(FARTHEST_MIP1),
                "r_frame_info" | "rw_frame_info" => own(FRAME_INFO),
                "r_spd_mips" => own(SPD_MIPS),
                "rw_spd_global_atomic" => own(SPD_ATOMIC),
                "r_shading_change" | "rw_shading_change" => own(SHADING_CHANGE_IMAGE),
                "r_accumulation" => own(accumulation_read),
                "rw_accumulation" => own(accumulation_write),
                "rw_new_locks" => own(NEW_LOCKS),
                "r_dilated_reactive_masks" | "rw_dilated_reactive_masks" => own(DILATED_REACTIVE),
                "r_luma_history" => own(history_read),
                "rw_luma_history" => own(history_write),
                "r_internal_upscaled_color" => own(upscaled_read),
                "rw_internal_upscaled_color" => own(upscaled_write),
                "r_lanczos_lut" => Some((self.images[LANCZOS].view, read)),
                _ => None,
            }
        };

        let groups_of = |size: [u32; 2], tile: u32| [size[0].div_ceil(tile), size[1].div_ceil(tile)];
        let shading = [frame.render[0] / 2, frame.render[1] / 2];
        let reactive = frame.opaque.map(|_| (AUTOGEN_REACTIVE, groups_of(frame.render, 8)));
        let dispatches: [(usize, [u32; 2]); 7] = [
            (PREPARE_INPUTS, groups_of(frame.render, 8)),
            (LUMA_PYRAMID, groups),
            (SHADING_CHANGE_PYRAMID, groups),
            (SHADING_CHANGE, groups_of(shading, 8)),
            (PREPARE_REACTIVITY, groups_of(frame.render, 8)),
            (LUMA_INSTABILITY, groups_of(frame.render, 8)),
            (ACCUMULATE, groups_of(frame.upscale, 8)),
        ];
        let between = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(
                vk::AccessFlags2::SHADER_SAMPLED_READ
                    | vk::AccessFlags2::SHADER_STORAGE_READ
                    | vk::AccessFlags2::SHADER_STORAGE_WRITE,
            );
        for (pass, [x, y]) in reactive.into_iter().chain(dispatches) {
            let info = &FSR3_PASSES[pass];
            let (set_layout, layout, pipeline) = pipelines.passes[pass];
            let layouts = [set_layout];
            let allocate = vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts);
            let Ok(sets) = (unsafe { device.allocate_descriptor_sets(&allocate) }) else {
                tracing::error!("FSR 3: out of descriptor sets");
                return;
            };
            let set = sets[0];
            let images: Vec<(u32, vk::DescriptorType, [vk::DescriptorImageInfo; 1])> = info
                .bindings
                .iter()
                .filter(|(_, kind, _)| matches!(kind, Fsr3Binding::Texture | Fsr3Binding::Storage))
                .filter_map(|&(name, kind, binding)| {
                    let Some((view, layout)) = view(name) else {
                        tracing::error!("FSR 3 {}: nothing to bind as {name}", info.name);
                        return None;
                    };
                    Some((
                        binding,
                        descriptor_type(kind),
                        [vk::DescriptorImageInfo::default().image_view(view).image_layout(layout)],
                    ))
                })
                .collect();
            let buffers: Vec<(u32, [vk::DescriptorBufferInfo; 1])> = info
                .bindings
                .iter()
                .filter(|(_, kind, _)| *kind == Fsr3Binding::Uniform)
                .map(|&(name, _, binding)| {
                    let at = offset
                        + match name {
                            "cbSPD" => SPD_CONSTANTS,
                            "cbGenerateReactive" => REACTIVE_CONSTANTS,
                            _ => FSR3_CONSTANTS,
                        };
                    (binding, [vk::DescriptorBufferInfo::default().buffer(self.constants.handle).offset(at).range(256)])
                })
                .collect();
            let mut writes: Vec<vk::WriteDescriptorSet> = images
                .iter()
                .map(|(binding, ty, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(*binding)
                        .descriptor_type(*ty)
                        .image_info(info)
                })
                .collect();
            writes.extend(buffers.iter().map(|(binding, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(*binding)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(info)
            }));
            unsafe {
                device.update_descriptor_sets(&writes, &[]);
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
                device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::COMPUTE, layout, 0, &[set], &[]);
                device.cmd_dispatch(cmd, x.max(1), y.max(1), 1);
                device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[between]));
            }
        }
        self.fresh = false;
        self.resource_frame += 1;
    }

    /// The constants of `cbFSR3Upscaler` for a frame, as AMD's host code works
    /// them out, for reverse-Z depth with no far plane.
    fn constants(&mut self, frame: &Fsr3Frame, reset: bool) -> [u32; 40] {
        let previous_jitter = self.jitter;
        self.jitter = frame.jitter;
        let (render, upscale) = (frame.render, frame.upscale);
        let aspect = render[0] as f32 / render[1].max(1) as f32;
        let tan_half_y = (frame.fov_y * 0.5).tan();
        let phases = (8.0 * (upscale[0] as f32 / render[0].max(1) as f32).powi(2)) as i32 as f32;
        self.jitter_phases = match reset || self.jitter_phases == 0.0 {
            true => phases,
            false => self.jitter_phases + (phases - self.jitter_phases).signum(),
        };
        self.frame_index = if reset { 0.0 } else { self.frame_index + 1.0 };
        let mut words = [0u32; 40];
        let ints = [
            render,
            self.previous_render,
            upscale,
            self.previous_upscale,
            [self.render[0], self.render[1]],
            [self.upscale[0], self.upscale[1]],
        ];
        for (i, pair) in ints.iter().enumerate() {
            words[i * 2] = pair[0];
            words[i * 2 + 1] = pair[1];
        }
        let floats = [
            // Reverse-Z with no far plane: view depth is the near plane over the device depth.
            -f32::EPSILON,
            frame.near,
            aspect * tan_half_y,
            tan_half_y,
            frame.jitter[0],
            frame.jitter[1],
            previous_jitter[0],
            previous_jitter[1],
            // Voxear's motion is now less then, in UV; FSR's is then less now.
            -1.0,
            -1.0,
            render[0] as f32 / upscale[0] as f32,
            render[1] as f32 / upscale[1] as f32,
            0.0,
            0.0,
            aspect * tan_half_y,
            self.jitter_phases,
            frame.delta_time.clamp(0.0, 1.0),
            1.0,
            1.0,
            self.frame_index,
            1.0,
            1.0,
            1.0,
            1.0 / 3.0,
            -1.0 / 3.0,
        ];
        for (i, value) in floats.iter().enumerate() {
            words[12 + i] = value.to_bits();
        }
        self.previous_render = render;
        self.previous_upscale = upscale;
        words
    }
}

fn lanczos2(x: f32) -> f32 {
    use std::f32::consts::PI;
    if x.abs() < f32::EPSILON {
        return 1.0;
    }
    (PI * x).sin() / (PI * x) * ((0.5 * PI * x).sin() / (0.5 * PI * x))
}

/// SPD's constants for downsampling the drawn picture, and its thread
/// groups, as `ffxSpdSetup` works them out.
fn spd_setup(render: [u32; 2]) -> ([u32; 8], [u32; 2]) {
    let groups = [(render[0] - 1) / 64 + 1, (render[1] - 1) / 64 + 1];
    let mips = (render[0].max(render[1]) as f32).log2().floor().min(12.0) as u32;
    (([mips, groups[0] * groups[1], 0, 0, render[0], render[1], 0, 0]), groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pass_is_compiled_and_names_what_it_binds() {
        assert_eq!(FSR3_PASSES.len(), 8);
        assert_eq!(FSR3_PASSES[AUTOGEN_REACTIVE].name, "autogen_reactive");
        for info in &FSR3_PASSES {
            assert!(info.spirv.len() > 1000 && info.spirv[..4] == [0x03, 0x02, 0x23, 0x07], "{} is SPIR-V", info.name);
            assert!(info.bindings.iter().any(|(name, ..)| *name == "cbFSR3Upscaler"), "{}", info.name);
            for (name, kind, binding) in info.bindings {
                let range = match kind {
                    Fsr3Binding::Uniform => 0..16,
                    Fsr3Binding::Texture => 16..48,
                    Fsr3Binding::Storage => 48..80,
                    Fsr3Binding::Sampler => 80..96,
                };
                assert!(range.contains(binding), "{}: {name} at {binding}", info.name);
            }
        }
        assert_eq!(FSR3_PASSES[ACCUMULATE].name, "accumulate");
        assert_eq!(FSR3_PASSES[LUMA_PYRAMID].group, [256, 1, 1]);
    }

    #[test]
    fn spd_and_lanczos_match_amds_host_code() {
        assert_eq!(spd_setup([2560, 1440]), ([11, 40 * 23, 0, 0, 2560, 1440, 0, 0], [40, 23]));
        assert_eq!(lanczos2(0.0), 1.0);
        assert!(lanczos2(1.0).abs() < 1e-6 && lanczos2(2.0).abs() < 1e-6, "zero at whole numbers");
        assert!(lanczos2(1.5) < 0.0, "a negative lobe between 1 and 2");
    }
}
