use std::io::Cursor;

use anyhow::Result;
use voxear_rhi::{Gpu, vk};

const VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/triangle.vert.spv"));
const FRAGMENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/triangle.frag.spv"));

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push {
    time: f32,
    aspect: f32,
}

pub(crate) struct TrianglePass {
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}

impl TrianglePass {
    pub fn new(gpu: &Gpu, color_format: vk::Format) -> Result<Self> {
        let device = &gpu.device;
        let ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .size(size_of::<Push>() as u32)];
        let layout = unsafe {
            device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&ranges), None)?
        };

        let vertex = shader_module(gpu, VERTEX)?;
        let fragment = shader_module(gpu, FRAGMENT)?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vertex).name(c"main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment)
                .name(c"main"),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let assembly =
            vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .line_width(1.0);
        let multisample =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let blend_attachments =
            [vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let formats = [color_format];
        let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats);

        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&assembly)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(layout)
            .push_next(&mut rendering);
        let pipeline = unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
            .map_err(|(_, error)| error);
        unsafe {
            device.destroy_shader_module(vertex, None);
            device.destroy_shader_module(fragment, None);
        }
        Ok(Self { layout, pipeline: pipeline?[0] })
    }

    pub fn record(&self, gpu: &Gpu, cmd: vk::CommandBuffer, extent: vk::Extent2D, time: f32) {
        let device = &gpu.device;
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: extent.width as f32,
            height: extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D { offset: vk::Offset2D::default(), extent };
        let push = Push { time, aspect: extent.width as f32 / extent.height.max(1) as f32 };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_set_viewport(cmd, 0, &[viewport]);
            device.cmd_set_scissor(cmd, 0, &[scissor]);
            device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::VERTEX, 0, bytemuck::bytes_of(&push));
            device.cmd_draw(cmd, 3, 1, 0, 0);
        }
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        unsafe {
            gpu.device.destroy_pipeline(self.pipeline, None);
            gpu.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn shader_module(gpu: &Gpu, spirv: &[u8]) -> Result<vk::ShaderModule> {
    let words = ash::util::read_spv(&mut Cursor::new(spirv))?;
    let info = vk::ShaderModuleCreateInfo::default().code(&words);
    Ok(unsafe { gpu.device.create_shader_module(&info, None)? })
}
