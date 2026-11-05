use std::{ffi::CStr, io::Cursor};

use anyhow::{Context, Result};
use ash::vk;

use crate::device::Gpu;

/// A compiled shader file. Slang emits every entry point of a file into one
/// SPIR-V module, so pipelines pick stages by entry point name.
pub struct ShaderModule {
    pub handle: vk::ShaderModule,
}

impl ShaderModule {
    pub fn new(gpu: &Gpu, name: &str, spirv: &[u8]) -> Result<Self> {
        let words = ash::util::read_spv(&mut Cursor::new(spirv)).with_context(|| format!("{name} is not SPIR-V"))?;
        let info = vk::ShaderModuleCreateInfo::default().code(&words);
        let handle = unsafe { gpu.device.create_shader_module(&info, None)? };
        gpu.set_name(handle, name);
        Ok(Self { handle })
    }

    pub fn destroy(self, gpu: &Gpu) {
        unsafe { gpu.device.destroy_shader_module(self.handle, None) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Opaque,
    Alpha,
    Additive,
    /// For colours already multiplied by their alpha, such as the UI's.
    Premultiplied,
    /// Light through filters, in the first attachment: the colour multiplies
    /// what is there, and the alpha keeps the least of the two, as the
    /// nearest of what was drawn. Any attachments after it are written over.
    Filter,
    /// The greater of what is drawn and what is there, in each channel.
    Max,
}

pub struct GraphicsPipelineDesc<'a> {
    pub name: &'a str,
    pub module: &'a ShaderModule,
    /// The vertex shader, unless `mesh_entry` is set.
    pub vertex_entry: &'a CStr,
    /// A mesh shader, and the task shader before it if any, in place of the
    /// vertex shader: geometry made on the GPU, with no vertex input. The
    /// task shader may come from a module of its own.
    pub task: Option<(&'a ShaderModule, &'a CStr)>,
    pub mesh_entry: Option<&'a CStr>,
    pub fragment_entry: Option<&'a CStr>,
    pub color_formats: &'a [vk::Format],
    pub depth_format: Option<vk::Format>,
    pub depth_write: bool,
    pub depth_compare: vk::CompareOp,
    pub cull: vk::CullModeFlags,
    pub front_face: vk::FrontFace,
    pub polygon_mode: vk::PolygonMode,
    pub blend: Blend,
    /// Constant and slope-scaled depth bias, for shadow maps.
    pub depth_bias: Option<(f32, f32)>,
    /// Clamp depth instead of clipping, so shadow casters behind the light's
    /// near plane still cast.
    pub depth_clamp: bool,
}

impl<'a> GraphicsPipelineDesc<'a> {
    /// Depth only, with no fragment stage: for shadow maps and depth prepasses.
    pub fn depth_only(name: &'a str, module: &'a ShaderModule, vertex: &'a CStr, depth_format: vk::Format) -> Self {
        Self {
            fragment_entry: None,
            depth_format: Some(depth_format),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            ..Self::new(name, module, vertex, vertex)
        }
    }

    /// As [`new`](Self::new), but with a task and a mesh shader in place of
    /// the vertex shader, and the fragment shader, from `module` with the
    /// mesh shader, only when given.
    pub fn mesh(
        name: &'a str,
        task: (&'a ShaderModule, &'a CStr),
        (module, mesh): (&'a ShaderModule, &'a CStr),
        fragment: Option<&'a CStr>,
    ) -> Self {
        Self {
            task: Some(task),
            mesh_entry: Some(mesh),
            fragment_entry: fragment,
            ..Self::new(name, module, mesh, mesh)
        }
    }

    /// Opaque, back-face culled, no depth: change the fields that differ.
    pub fn new(name: &'a str, module: &'a ShaderModule, vertex: &'a CStr, fragment: &'a CStr) -> Self {
        Self {
            name,
            module,
            vertex_entry: vertex,
            task: None,
            mesh_entry: None,
            fragment_entry: Some(fragment),
            color_formats: &[],
            depth_format: None,
            depth_write: false,
            depth_compare: vk::CompareOp::ALWAYS,
            cull: vk::CullModeFlags::BACK,
            // Unity's convention: front faces are clockwise on screen. Verified by
            // rendering the primitives (see `vox render`).
            front_face: vk::FrontFace::CLOCKWISE,
            polygon_mode: vk::PolygonMode::FILL,
            blend: Blend::Opaque,
            depth_bias: None,
            depth_clamp: false,
        }
    }
}

pub fn create_graphics_pipeline(
    gpu: &Gpu,
    layout: vk::PipelineLayout,
    desc: &GraphicsPipelineDesc,
) -> Result<vk::Pipeline> {
    let stage =
        |stage, name| vk::PipelineShaderStageCreateInfo::default().stage(stage).module(desc.module.handle).name(name);
    let mut stages = match desc.mesh_entry {
        Some(mesh) => {
            let task =
                desc.task.map(|(module, task)| stage(vk::ShaderStageFlags::TASK_EXT, task).module(module.handle));
            task.into_iter().chain([stage(vk::ShaderStageFlags::MESH_EXT, mesh)]).collect()
        }
        None => vec![stage(vk::ShaderStageFlags::VERTEX, desc.vertex_entry)],
    };
    if let Some(fragment) = desc.fragment_entry {
        stages.push(
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(desc.module.handle)
                .name(fragment),
        );
    }
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
    let (bias_constant, bias_slope) = desc.depth_bias.unwrap_or_default();
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(desc.polygon_mode)
        .cull_mode(desc.cull)
        .front_face(desc.front_face)
        .depth_clamp_enable(desc.depth_clamp)
        .depth_bias_enable(desc.depth_bias.is_some())
        .depth_bias_constant_factor(bias_constant)
        .depth_bias_slope_factor(bias_slope)
        .line_width(1.0);
    let multisample =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(desc.depth_format.is_some())
        .depth_write_enable(desc.depth_write)
        .depth_compare_op(desc.depth_compare);
    let attachment = blend_state(desc.blend);
    let mut attachments = vec![attachment; desc.color_formats.len()];
    if desc.blend == Blend::Filter {
        attachments.iter_mut().skip(1).for_each(|rest| *rest = blend_state(Blend::Opaque));
    }
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(desc.color_formats)
        .depth_attachment_format(desc.depth_format.unwrap_or(vk::Format::UNDEFINED));

    let mut info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // Mesh shaders make their own geometry, with no vertex input to describe.
    if desc.mesh_entry.is_none() {
        info = info.vertex_input_state(&vertex_input).input_assembly_state(&assembly);
    }
    let pipeline = unsafe { gpu.device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        .map_err(|(_, error)| error)
        .with_context(|| format!("creating pipeline {}", desc.name))?[0];
    gpu.set_name(pipeline, desc.name);
    Ok(pipeline)
}

pub fn create_compute_pipeline(
    gpu: &Gpu,
    layout: vk::PipelineLayout,
    name: &str,
    module: &ShaderModule,
    entry: &CStr,
) -> Result<vk::Pipeline> {
    let stage = vk::PipelineShaderStageCreateInfo::default()
        .stage(vk::ShaderStageFlags::COMPUTE)
        .module(module.handle)
        .name(entry);
    let info = vk::ComputePipelineCreateInfo::default().stage(stage).layout(layout);
    let pipeline = unsafe { gpu.device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) }
        .map_err(|(_, error)| error)
        .with_context(|| format!("creating compute pipeline {name}"))?[0];
    gpu.set_name(pipeline, name);
    Ok(pipeline)
}

fn blend_state(blend: Blend) -> vk::PipelineColorBlendAttachmentState {
    let state = vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA);
    match blend {
        Blend::Opaque => state,
        Blend::Alpha => state
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD),
        Blend::Premultiplied => state
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD),
        Blend::Filter => state
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ZERO)
            .dst_color_blend_factor(vk::BlendFactor::SRC_COLOR)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::MIN),
        Blend::Additive => state
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD),
        Blend::Max => state
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::MAX)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::MAX),
    }
}
