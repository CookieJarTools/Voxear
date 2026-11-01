use anyhow::Result;
use voxear_rhi::{
    Bindless, Blend, Gpu, GraphicsPipelineDesc, ShaderModule, create_compute_pipeline, create_graphics_pipeline, vk,
};

use crate::hot_reload::ShaderSet;

pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;
pub const PYRAMID_FORMAT: vk::Format = vk::Format::R32_SFLOAT;
/// Screen motion since last frame, in UV units.
pub const VELOCITY_FORMAT: vk::Format = vk::Format::R16G16_SFLOAT;
/// Bloom needs range, not precision or alpha.
pub const BLOOM_FORMAT: vk::Format = vk::Format::B10G11R11_UFLOAT_PACK32;
/// The tonemapped image when a finishing pass still follows. Kept at 16 bits
/// so the finish's dither can still break up banding.
pub const LDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;

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
    /// The forward pass's meshes as their edges, for the wireframe view.
    pub wireframe: vk::Pipeline,
    pub sky: vk::Pipeline,
    pub fog: vk::Pipeline,
    pub taa: vk::Pipeline,
    pub dof: vk::Pipeline,
    pub motion_blur: vk::Pipeline,
    pub bloom_down: vk::Pipeline,
    pub bloom_up: vk::Pipeline,
    pub exposure: vk::Pipeline,
    /// Writes the LDR intermediate, for a finishing pass to read.
    pub composite_ldr: vk::Pipeline,
    /// Writes the output directly, when nothing follows.
    pub composite: vk::Pipeline,
    pub finish: vk::Pipeline,
    /// The frame debugger's view of an image.
    pub frame_debug: vk::Pipeline,
    pub ui: vk::Pipeline,
    /// Debug draw's lines, over the finished frame.
    pub debug: vk::Pipeline,
    /// An object on its own, for its preview, and its resolve into the
    /// preview atlas.
    pub preview: vk::Pipeline,
    pub preview_resolve: vk::Pipeline,
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
            module("clusters")?,
            module("post")?,
            module("exposure")?,
            module("ui")?,
            module("preview")?,
            module("debug")?,
        ];
        let [cull, pyramid, shadow, forward, sky, clusters, post, exposure, ui, preview, debug] = &modules;

        let graphics = |desc: &GraphicsPipelineDesc| create_graphics_pipeline(gpu, layout, desc);
        let fullscreen = |name, entry, formats, blend| {
            graphics(&GraphicsPipelineDesc {
                color_formats: formats,
                cull: vk::CullModeFlags::NONE,
                blend,
                ..GraphicsPipelineDesc::new(name, post, c"postVertex", entry)
            })
        };
        let scene_formats = &[HDR_FORMAT, VELOCITY_FORMAT];
        let hdr = &[HDR_FORMAT];
        let bloom = &[BLOOM_FORMAT];
        let output = &[output_format];

        let pipelines = Self {
            clusters: create_compute_pipeline(gpu, layout, "Light clusters", clusters, c"clusterMain")?,
            cull: create_compute_pipeline(gpu, layout, "Cull", cull, c"cullMain")?,
            pyramid: create_compute_pipeline(gpu, layout, "Depth pyramid", pyramid, c"reduceMain")?,
            shadow: graphics(&GraphicsPipelineDesc {
                // Single-sided geometry such as planes must still cast.
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque", forward, c"vertexMain", c"fragmentMain")
            })?,
            wireframe: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                ..GraphicsPipelineDesc::new("Wireframe", forward, c"vertexMain", c"fragmentMain")
            })?,
            sky: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Sky", sky, c"skyVertex", c"skyFragment")
            })?,
            fog: fullscreen("Fog", c"fogFragment", hdr, Blend::Alpha)?,
            taa: fullscreen("TAA", c"taaFragment", hdr, Blend::Opaque)?,
            dof: fullscreen("Depth of field", c"dofFragment", hdr, Blend::Opaque)?,
            motion_blur: fullscreen("Motion blur", c"motionBlurFragment", hdr, Blend::Opaque)?,
            bloom_down: fullscreen("Bloom down", c"bloomDownFragment", bloom, Blend::Opaque)?,
            bloom_up: fullscreen("Bloom up", c"bloomUpFragment", bloom, Blend::Additive)?,
            exposure: create_compute_pipeline(gpu, layout, "Exposure", exposure, c"exposureMain")?,
            composite_ldr: fullscreen("Composite", c"compositeFragment", &[LDR_FORMAT], Blend::Opaque)?,
            composite: fullscreen("Composite", c"compositeFragment", output, Blend::Opaque)?,
            finish: fullscreen("Finish", c"finishFragment", output, Blend::Opaque)?,
            frame_debug: fullscreen("Frame debugger view", c"frameDebugFragment", hdr, Blend::Opaque)?,
            ui: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("UI", ui, c"uiVertex", c"uiFragment")
            })?,
            debug: graphics(&GraphicsPipelineDesc {
                color_formats: output,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Premultiplied,
                ..GraphicsPipelineDesc::new("Debug draw", debug, c"debugVertex", c"debugFragment")
            })?,
            preview: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                // Planes and open meshes show from either side.
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Preview", preview, c"previewVertex", c"previewFragment")
            })?,
            preview_resolve: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::previews::PREVIEW_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Preview resolve", preview, c"resolveVertex", c"resolveFragment")
            })?,
        };
        for module in modules {
            module.destroy(gpu);
        }
        Ok(pipelines)
    }

    pub fn destroy(&self, gpu: &Gpu) {
        let all = [
            self.clusters,
            self.cull,
            self.pyramid,
            self.shadow,
            self.forward,
            self.wireframe,
            self.sky,
            self.fog,
            self.taa,
            self.dof,
            self.motion_blur,
            self.bloom_down,
            self.bloom_up,
            self.exposure,
            self.composite_ldr,
            self.composite,
            self.finish,
            self.frame_debug,
            self.ui,
            self.debug,
            self.preview,
            self.preview_resolve,
        ];
        for pipeline in all {
            unsafe { gpu.device.destroy_pipeline(pipeline, None) };
        }
    }
}
