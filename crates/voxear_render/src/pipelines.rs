use anyhow::Result;
use voxear_rhi::{
    Bindless, Gpu, GraphicsPipelineDesc, ShaderModule, create_compute_pipeline, create_graphics_pipeline, vk,
};

use crate::hot_reload::ShaderSet;

pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const PYRAMID_FORMAT: vk::Format = vk::Format::R32_SFLOAT;

/// Slope-scaled bias for shadow casters. The forward pass also offsets
/// lookups along the normal, so this only has to remove what that misses.
const SHADOW_BIAS: (f32, f32) = (0.0, 1.5);

#[derive(Clone, Copy)]
pub struct Pipelines {
    pub clusters: vk::Pipeline,
    pub cull: vk::Pipeline,
    pub pyramid: vk::Pipeline,
    pub shadow: vk::Pipeline,
    pub forward: vk::Pipeline,
    pub sky: vk::Pipeline,
    pub tonemap: vk::Pipeline,
}

impl Pipelines {
    pub fn new(gpu: &Gpu, bindless: &Bindless, output_format: vk::Format, shaders: &ShaderSet) -> Result<Self> {
        let layout = bindless.pipeline_layout;
        let module = |name: &str| ShaderModule::new(gpu, &format!("{name}.slang"), shaders.get(name)?);
        let modules = [
            module("cull")?,
            module("pyramid")?,
            module("shadow")?,
            module("forward")?,
            module("sky")?,
            module("tonemap")?,
            module("clusters")?,
        ];
        let [cull, pyramid, shadow, forward, sky, tonemap, clusters] = &modules;

        let pipelines = Self {
            clusters: create_compute_pipeline(gpu, layout, "Light clusters", clusters, c"clusterMain")?,
            cull: create_compute_pipeline(gpu, layout, "Cull", cull, c"cullMain")?,
            pyramid: create_compute_pipeline(gpu, layout, "Depth pyramid", pyramid, c"reduceMain")?,
            shadow: create_graphics_pipeline(
                gpu,
                layout,
                &GraphicsPipelineDesc {
                    // Single-sided geometry such as planes must still cast.
                    cull: vk::CullModeFlags::NONE,
                    depth_bias: Some(SHADOW_BIAS),
                    depth_clamp: true,
                    ..GraphicsPipelineDesc::depth_only("Shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
                },
            )?,
            forward: create_graphics_pipeline(
                gpu,
                layout,
                &GraphicsPipelineDesc {
                    color_formats: &[HDR_FORMAT],
                    depth_format: Some(DEPTH_FORMAT),
                    depth_write: true,
                    depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                    ..GraphicsPipelineDesc::new("Opaque", forward, c"vertexMain", c"fragmentMain")
                },
            )?,
            sky: create_graphics_pipeline(
                gpu,
                layout,
                &GraphicsPipelineDesc {
                    color_formats: &[HDR_FORMAT],
                    depth_format: Some(DEPTH_FORMAT),
                    depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                    cull: vk::CullModeFlags::NONE,
                    ..GraphicsPipelineDesc::new("Sky", sky, c"skyVertex", c"skyFragment")
                },
            )?,
            tonemap: create_graphics_pipeline(
                gpu,
                layout,
                &GraphicsPipelineDesc {
                    color_formats: &[output_format],
                    cull: vk::CullModeFlags::NONE,
                    ..GraphicsPipelineDesc::new("Tonemap", tonemap, c"tonemapVertex", c"tonemapFragment")
                },
            )?,
        };
        for module in modules {
            module.destroy(gpu);
        }
        Ok(pipelines)
    }

    pub fn destroy(&self, gpu: &Gpu) {
        for pipeline in [self.clusters, self.cull, self.pyramid, self.shadow, self.forward, self.sky, self.tonemap] {
            unsafe { gpu.device.destroy_pipeline(pipeline, None) };
        }
    }
}
