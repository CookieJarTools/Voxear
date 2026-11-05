use anyhow::Result;
use voxear_rhi::{
    Bindless, Blend, Gpu, GraphicsPipelineDesc, ShaderModule, create_compute_pipeline, create_graphics_pipeline, vk,
};

use crate::{atmosphere::TABLE_FORMAT, hot_reload::ShaderSet, volumetric::VOLUME_FORMAT};

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
/// Ambient occlusion within a frame: a visibility from 0 to 1.
pub const AO_FORMAT: vk::Format = vk::Format::R8_UNORM;
/// Each pixel's normal, folded flat (octahedral) in red and green, and its
/// roughness in blue, which the depth passes write beside the depth, so
/// reflections skip what is too rough to show them and trace from the
/// surface's own smooth normal. Ten bits each, as eight would facet a
/// mirror's reflection.
pub const SURFACE_FORMAT: vk::Format = vk::Format::A2B10G10R10_UNORM_PACK32;
/// Its history, which blends a tenth of each frame in, so eight bits would
/// stop short of the value it is heading for.
pub const AO_HISTORY_FORMAT: vk::Format = vk::Format::R16_SFLOAT;

/// Slope-scaled bias for shadow casters. The forward pass also offsets
/// lookups along the normal, so this only has to remove what that misses.
const SHADOW_BIAS: (f32, f32) = (0.0, 1.5);
/// Lifts a mesh's edges off its own faces, towards the camera (depth is
/// reversed), so they are not lost in them.
const EDGE_BIAS: (f32, f32) = (1.0, 1.0);

/// The scene's pipelines with task and mesh shaders in place of vertex
/// shaders, for GPUs that have them (see `shaders/meshlet_task.slang`).
#[derive(Clone, Copy)]
pub struct MeshPipelines {
    pub shadow: vk::Pipeline,
    pub local_shadow: vk::Pipeline,
    pub forward: vk::Pipeline,
    pub depth: vk::Pipeline,
    pub forward_on_depth: vk::Pipeline,
    pub wireframe: vk::Pipeline,
    pub edges: vk::Pipeline,
}

#[derive(Clone, Copy)]
pub struct Pipelines {
    pub clusters: vk::Pipeline,
    pub cull: vk::Pipeline,
    /// Turns each instance draw into draws of its meshlets that survive culling.
    pub meshlet_cull: vk::Pipeline,
    pub pyramid: vk::Pipeline,
    pub shadow: vk::Pipeline,
    /// As `shadow`, for local lights' views, without clamping depth: what
    /// lies nearer a light than its near plane, as the flame or bulb round
    /// it, is clipped away rather than flattened onto it, where it would
    /// shade everything.
    pub local_shadow: vk::Pipeline,
    pub forward: vk::Pipeline,
    /// The opaque lists as depth alone, drawn first when ambient occlusion
    /// is on, so it can work from the frame's whole depth.
    pub depth: vk::Pipeline,
    /// The forward pass over the depth already drawn: it neither writes
    /// depth nor shades anything that depth hides.
    pub forward_on_depth: vk::Pipeline,
    /// The forward pass's meshes as their edges, for the wireframe view.
    pub wireframe: vk::Pipeline,
    /// The meshes' edges drawn faintly over the lit scene, for the lit
    /// wireframe view.
    pub edges: vk::Pipeline,
    pub sky: vk::Pipeline,
    /// The atmosphere's tables (see `atmosphere`).
    pub transmittance: vk::Pipeline,
    pub multi_scattering: vk::Pipeline,
    pub sky_view: vk::Pipeline,
    pub aerial_perspective: vk::Pipeline,
    /// Volumetric fog's passes: the volume, its integration, and laying it
    /// over the scene.
    pub volume_scatter: vk::Pipeline,
    pub volume_integrate: vk::Pipeline,
    pub volumetric_fog: vk::Pipeline,
    /// Ambient occlusion's passes: GTAO, its denoise and its temporal blend.
    pub gtao: vk::Pipeline,
    pub ao_denoise: vk::Pipeline,
    pub ao_temporal: vk::Pipeline,
    /// Contact shadows' march towards the sun through the depth buffer.
    pub contact_shadows: vk::Pipeline,
    /// Screen-space reflections' march through the depth buffer.
    pub reflections: vk::Pipeline,
    pub fog: vk::Pipeline,
    pub taa: vk::Pipeline,
    /// TAA that builds the output from a scene drawn smaller.
    pub taa_upscale: vk::Pipeline,
    pub dof: vk::Pipeline,
    pub motion_blur: vk::Pipeline,
    pub bloom_down: vk::Pipeline,
    pub bloom_up: vk::Pipeline,
    pub exposure: vk::Pipeline,
    /// The sun's light and the sky's, once a frame under the atmosphere.
    pub sky_light: vk::Pipeline,
    /// Pushes through foliage: springing back, and this frame's stamped.
    pub foliage_push: vk::Pipeline,
    /// Glass: the copy of the picture it is drawn over, and its panes.
    pub glass_copy: vk::Pipeline,
    pub glass: vk::Pipeline,
    /// The sea's waves: the FFT's rows and columns, their mips, the depth
    /// it is drawn over, and its surface (see `water`).
    pub ocean_rows: vk::Pipeline,
    pub ocean_columns: vk::Pipeline,
    pub ocean_mip: vk::Pipeline,
    pub ocean_foam: vk::Pipeline,
    pub water_depth_copy: vk::Pipeline,
    pub water: vk::Pipeline,
    pub water_pool: vk::Pipeline,
    pub water_wall: vk::Pipeline,
    /// Under water: what the view shows dimmed, then the water's own light.
    pub underwater_dim: vk::Pipeline,
    pub underwater_glow: vk::Pipeline,
    /// Panes into the glass shadow atlas (`glass::record_shadows`).
    pub glass_shadow: vk::Pipeline,
    pub glass_shadow_mip: vk::Pipeline,
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
    /// A plant drawn into each picture of its impostor's atlas: its colour
    /// and its normals (see `impostors`).
    pub impostor_bake: vk::Pipeline,
    /// Where the GPU has mesh shaders.
    pub mesh: Option<MeshPipelines>,
    /// Ray-traced GI's rays and update passes, where the GPU has ray tracing.
    pub ddgi: Option<[vk::Pipeline; 2]>,
    /// The same, tracing rays through hierarchies in a compute shader, for
    /// GPUs without ray tracing.
    pub ddgi_software: [vk::Pipeline; 2],
    /// Reflections by the GPU's rays, through the GI's scene, where the GPU
    /// has ray tracing (`rayReflectionsFragment` in `shaders/ddgi.slang`).
    pub ray_reflections: Option<vk::Pipeline>,
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
            module("ambient_occlusion")?,
            module("volumetric")?,
            module("ddgi_software")?,
            module("glass")?,
            module("foliage_push")?,
            module("water")?,
        ];
        let [
            cull,
            pyramid,
            shadow,
            forward,
            sky,
            clusters,
            post,
            exposure,
            ui,
            preview,
            debug,
            occlusion,
            volumetric,
            ddgi_software,
            glass,
            foliage_push,
            water,
        ] = &modules;
        // Only a GPU with mesh shaders can load their modules.
        let meshlets = match gpu.mesh_shader {
            Some(_) => Some([module("meshlet_task")?, module("meshlets")?, module("meshlet_shadow")?]),
            None => None,
        };
        // Only a GPU with ray tracing can load ray queries.
        let ddgi = match gpu.acceleration_structure {
            Some(_) => Some(module("ddgi")?),
            None => None,
        };

        let graphics = |desc: &GraphicsPipelineDesc| create_graphics_pipeline(gpu, layout, desc);
        let fullscreen = |name, entry, formats, blend| {
            graphics(&GraphicsPipelineDesc {
                color_formats: formats,
                cull: vk::CullModeFlags::NONE,
                blend,
                ..GraphicsPipelineDesc::new(name, post, c"postVertex", entry)
            })
        };
        let occlusion_pass = |name, entry, format: &'static [vk::Format]| {
            graphics(&GraphicsPipelineDesc {
                color_formats: format,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, occlusion, c"aoVertex", entry)
            })
        };
        let table = |name, entry| {
            graphics(&GraphicsPipelineDesc {
                color_formats: &[TABLE_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, sky, c"lutVertex", entry)
            })
        };
        let volume = |name, entry| {
            graphics(&GraphicsPipelineDesc {
                color_formats: &[VOLUME_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new(name, volumetric, c"volumeVertex", entry)
            })
        };
        let scene_formats = &[HDR_FORMAT, VELOCITY_FORMAT];
        let hdr = &[HDR_FORMAT];
        let bloom = &[BLOOM_FORMAT];
        let output = &[output_format];

        let pipelines = Self {
            clusters: create_compute_pipeline(gpu, layout, "Light clusters", clusters, c"clusterMain")?,
            cull: create_compute_pipeline(gpu, layout, "Cull", cull, c"cullMain")?,
            meshlet_cull: create_compute_pipeline(gpu, layout, "Meshlets", cull, c"meshletMain")?,
            pyramid: create_compute_pipeline(gpu, layout, "Depth pyramid", pyramid, c"reduceMain")?,
            shadow: graphics(&GraphicsPipelineDesc {
                // Single-sided geometry such as planes must still cast.
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                depth_clamp: true,
                ..GraphicsPipelineDesc::depth_only("Shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            local_shadow: graphics(&GraphicsPipelineDesc {
                cull: vk::CullModeFlags::NONE,
                depth_bias: Some(SHADOW_BIAS),
                ..GraphicsPipelineDesc::depth_only("Local shadows", shadow, c"shadowVertex", DEPTH_FORMAT)
            })?,
            forward: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Opaque", forward, c"vertexMain", c"fragmentMain")
            })?,
            depth: graphics(&GraphicsPipelineDesc {
                color_formats: &[SURFACE_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                ..GraphicsPipelineDesc::new("Depth", forward, c"vertexMain", c"depthFragment")
            })?,
            forward_on_depth: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
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
            edges: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                polygon_mode: vk::PolygonMode::LINE,
                blend: Blend::Alpha,
                depth_bias: Some(EDGE_BIAS),
                ..GraphicsPipelineDesc::new("Edges", forward, c"vertexMain", c"edgeFragment")
            })?,
            sky: graphics(&GraphicsPipelineDesc {
                color_formats: scene_formats,
                depth_format: Some(DEPTH_FORMAT),
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Sky", sky, c"skyVertex", c"skyFragment")
            })?,
            transmittance: table("Transmittance", c"transmittanceFragment")?,
            multi_scattering: table("Multiple scattering", c"multiScatteringFragment")?,
            sky_view: table("Sky view", c"skyViewFragment")?,
            aerial_perspective: table("Aerial perspective", c"aerialPerspectiveFragment")?,
            gtao: occlusion_pass("GTAO", c"gtaoFragment", &[AO_FORMAT])?,
            ao_denoise: occlusion_pass("AO denoise", c"aoDenoiseFragment", &[AO_FORMAT])?,
            ao_temporal: occlusion_pass("AO temporal", c"aoTemporalFragment", &[AO_HISTORY_FORMAT])?,
            contact_shadows: occlusion_pass("Contact shadows", c"contactShadowFragment", &[AO_FORMAT])?,
            reflections: occlusion_pass("Reflections", c"reflectionsFragment", &[HDR_FORMAT])?,
            fog: fullscreen("Fog", c"fogFragment", hdr, Blend::Alpha)?,
            volume_scatter: volume("Fog volume", c"volumeScatterFragment")?,
            volume_integrate: create_compute_pipeline(gpu, layout, "Fog integration", volumetric, c"volumeIntegrateMain")?,
            volumetric_fog: fullscreen("Volumetric fog", c"volumetricFogFragment", hdr, Blend::Premultiplied)?,
            taa: fullscreen("TAA", c"taaFragment", hdr, Blend::Opaque)?,
            taa_upscale: fullscreen("TAA upscale", c"taaUpscaleFragment", hdr, Blend::Opaque)?,
            dof: fullscreen("Depth of field", c"dofFragment", hdr, Blend::Opaque)?,
            motion_blur: fullscreen("Motion blur", c"motionBlurFragment", hdr, Blend::Opaque)?,
            bloom_down: fullscreen("Bloom down", c"bloomDownFragment", bloom, Blend::Opaque)?,
            bloom_up: fullscreen("Bloom up", c"bloomUpFragment", bloom, Blend::Additive)?,
            exposure: create_compute_pipeline(gpu, layout, "Exposure", exposure, c"exposureMain")?,
            glass_copy: create_compute_pipeline(gpu, layout, "Glass copy", glass, c"glassCopyMain")?,
            glass_shadow_mip: create_compute_pipeline(gpu, layout, "Glass shadow mips", glass, c"glassShadowMipMain")?,
            foliage_push: create_compute_pipeline(gpu, layout, "Foliage pushes", foliage_push, c"pushMain")?,
            sky_light: create_compute_pipeline(gpu, layout, "Sky light", sky, c"skyLightMain")?,
            ocean_rows: create_compute_pipeline(gpu, layout, "Ocean rows", water, c"oceanRowsMain")?,
            ocean_columns: create_compute_pipeline(gpu, layout, "Ocean columns", water, c"oceanColumnsMain")?,
            ocean_mip: create_compute_pipeline(gpu, layout, "Ocean mips", water, c"oceanMipMain")?,
            ocean_foam: create_compute_pipeline(gpu, layout, "Ocean foam", water, c"oceanFoamMain")?,
            water_depth_copy: create_compute_pipeline(gpu, layout, "Water depth copy", water, c"waterDepthCopyMain")?,
            water: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Water", water, c"waterVertex", c"waterFragment")
            })?,
            water_pool: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Pools", water, c"poolVertex", c"waterFragment")
            })?,
            water_wall: graphics(&GraphicsPipelineDesc {
                color_formats: &[HDR_FORMAT, VELOCITY_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Walls of water", water, c"wallVertex", c"wallFragment")
            })?,
            underwater_dim: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Filter,
                ..GraphicsPipelineDesc::new("Under water", water, c"underwaterVertex", c"underwaterDimFragment")
            })?,
            underwater_glow: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Additive,
                ..GraphicsPipelineDesc::new("Water's light", water, c"underwaterVertex", c"underwaterGlowFragment")
            })?,
            // Tested against the depth in its fragment shader, which reads it.
            glass: graphics(&GraphicsPipelineDesc {
                color_formats: hdr,
                ..GraphicsPipelineDesc::new("Glass", glass, c"glassVertex", c"glassFragment")
            })?,
            glass_shadow: graphics(&GraphicsPipelineDesc {
                color_formats: &crate::glass::SHADOW_FORMATS,
                cull: vk::CullModeFlags::NONE,
                blend: Blend::Filter,
                // A pane nearer the sun than a cascade's near plane still filters its light.
                depth_clamp: true,
                ..GraphicsPipelineDesc::new("Glass shadows", glass, c"glassShadowVertex", c"glassShadowFragment")
            })?,
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
            impostor_bake: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::impostors::IMPOSTOR_COLOR_FORMAT, crate::impostors::IMPOSTOR_NORMAL_FORMAT],
                depth_format: Some(DEPTH_FORMAT),
                depth_write: true,
                depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
                // Leaves show from either side.
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Impostor bake", preview, c"previewVertex", c"impostorBakeFragment")
            })?,
            preview_resolve: graphics(&GraphicsPipelineDesc {
                color_formats: &[crate::previews::PREVIEW_FORMAT],
                cull: vk::CullModeFlags::NONE,
                ..GraphicsPipelineDesc::new("Preview resolve", preview, c"resolveVertex", c"resolveFragment")
            })?,
            mesh: meshlets.as_ref().map(|modules| mesh_pipelines(gpu, layout, modules, scene_formats)).transpose()?,
            ddgi_software: [
                create_compute_pipeline(gpu, layout, "DDGI rays", ddgi_software, c"ddgiRaysMain")?,
                create_compute_pipeline(gpu, layout, "DDGI update", ddgi_software, c"ddgiUpdateMain")?,
            ],
            ray_reflections: match &ddgi {
                Some(module) => Some(graphics(&GraphicsPipelineDesc {
                    color_formats: &[HDR_FORMAT],
                    cull: vk::CullModeFlags::NONE,
                    ..GraphicsPipelineDesc::new("Reflections", module, c"aoVertex", c"rayReflectionsFragment")
                })?),
                None => None,
            },
            ddgi: match &ddgi {
                Some(module) => Some([
                    create_compute_pipeline(gpu, layout, "DDGI rays", module, c"ddgiRaysMain")?,
                    create_compute_pipeline(gpu, layout, "DDGI update", module, c"ddgiUpdateMain")?,
                ]),
                None => None,
            },
        };
        for module in modules.into_iter().chain(meshlets.into_iter().flatten()).chain(ddgi) {
            module.destroy(gpu);
        }
        Ok(pipelines)
    }

    pub fn destroy(&self, gpu: &Gpu) {
        let all = [
            self.clusters,
            self.cull,
            self.meshlet_cull,
            self.pyramid,
            self.shadow,
            self.local_shadow,
            self.forward,
            self.depth,
            self.forward_on_depth,
            self.wireframe,
            self.edges,
            self.sky,
            self.transmittance,
            self.multi_scattering,
            self.sky_view,
            self.aerial_perspective,
            self.gtao,
            self.ao_denoise,
            self.ao_temporal,
            self.contact_shadows,
            self.reflections,
            self.fog,
            self.volume_scatter,
            self.volume_integrate,
            self.volumetric_fog,
            self.taa,
            self.taa_upscale,
            self.dof,
            self.motion_blur,
            self.bloom_down,
            self.bloom_up,
            self.exposure,
            self.glass_copy,
            self.foliage_push,
            self.sky_light,
            self.glass,
            self.glass_shadow,
            self.glass_shadow_mip,
            self.composite_ldr,
            self.composite,
            self.finish,
            self.frame_debug,
            self.ui,
            self.debug,
            self.preview,
            self.preview_resolve,
            self.impostor_bake,
        ];
        let mesh =
            self.mesh.iter().flat_map(|m| [m.shadow, m.local_shadow, m.forward, m.depth, m.forward_on_depth, m.wireframe, m.edges]);
        let traced = self.ddgi.into_iter().flatten().chain(self.ddgi_software).chain(self.ray_reflections);
        for pipeline in all.into_iter().chain(mesh).chain(traced) {
            unsafe { gpu.device.destroy_pipeline(pipeline, None) };
        }
    }
}

/// The shadow, forward, depth, wireframe and edges pipelines as `Pipelines::new`
/// makes them, with the task shader culling meshlets and mesh shaders
/// making them in place of the vertex shaders.
fn mesh_pipelines(
    gpu: &Gpu,
    layout: vk::PipelineLayout,
    [task, forward, shadow]: &[ShaderModule; 3],
    scene_formats: &[vk::Format],
) -> Result<MeshPipelines> {
    let graphics = |desc: &GraphicsPipelineDesc| create_graphics_pipeline(gpu, layout, desc);
    let task = (task, c"meshletTask");
    let forward = (forward, c"forwardMesh");
    Ok(MeshPipelines {
        shadow: graphics(&GraphicsPipelineDesc {
            cull: vk::CullModeFlags::NONE,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            depth_clamp: true,
            ..GraphicsPipelineDesc::mesh("Shadows", task, (shadow, c"shadowMesh"), None)
        })?,
        local_shadow: graphics(&GraphicsPipelineDesc {
            cull: vk::CullModeFlags::NONE,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: Some(SHADOW_BIAS),
            ..GraphicsPipelineDesc::mesh("Local shadows", task, (shadow, c"shadowMesh"), None)
        })?,
        forward: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Opaque", task, forward, Some(c"fragmentMain"))
        })?,
        depth: graphics(&GraphicsPipelineDesc {
            color_formats: &[SURFACE_FORMAT],
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Depth", task, forward, Some(c"depthFragment"))
        })?,
        forward_on_depth: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            ..GraphicsPipelineDesc::mesh("Opaque", task, forward, Some(c"fragmentMain"))
        })?,
        wireframe: graphics(&GraphicsPipelineDesc {
            color_formats: scene_formats,
            depth_format: Some(DEPTH_FORMAT),
            depth_write: true,
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            cull: vk::CullModeFlags::NONE,
            polygon_mode: vk::PolygonMode::LINE,
            ..GraphicsPipelineDesc::mesh("Wireframe", task, forward, Some(c"fragmentMain"))
        })?,
        edges: graphics(&GraphicsPipelineDesc {
            color_formats: &[HDR_FORMAT],
            depth_format: Some(DEPTH_FORMAT),
            depth_compare: vk::CompareOp::GREATER_OR_EQUAL,
            cull: vk::CullModeFlags::NONE,
            polygon_mode: vk::PolygonMode::LINE,
            blend: Blend::Alpha,
            depth_bias: Some(EDGE_BIAS),
            ..GraphicsPipelineDesc::mesh("Edges", task, forward, Some(c"edgeFragment"))
        })?,
    })
}
