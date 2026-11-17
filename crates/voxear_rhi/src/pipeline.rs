use std::{
    ffi::{CStr, CString},
    io::Cursor,
};

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
    let pipeline = unsafe { gpu.device.create_graphics_pipelines(gpu.pipeline_cache, &[info], None) }
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
    let pipeline = unsafe { gpu.device.create_compute_pipelines(gpu.pipeline_cache, &[info], None) }
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

/// The size of a pipeline cache's header: its size, its version, the
/// vendor's and device's numbers and the driver's cache UUID.
const CACHE_HEADER: usize = 32;

impl Gpu {
    /// Fills the pipeline cache with what [`pipeline_cache_data`](Self::pipeline_cache_data)
    /// gave on an earlier run, so the pipelines in it are not compiled
    /// again. Call it before any pipeline is made. Data another GPU or
    /// driver wrote is left out. Returns whether the data was taken.
    pub fn load_pipeline_cache(&mut self, data: &[u8]) -> bool {
        if data.len() <= CACHE_HEADER || data[..CACHE_HEADER] != self.pipeline_cache_header()[..] {
            return false;
        }
        let info = vk::PipelineCacheCreateInfo::default().initial_data(data);
        let Ok(cache) = (unsafe { self.device.create_pipeline_cache(&info, None) }) else { return false };
        unsafe { self.device.destroy_pipeline_cache(self.pipeline_cache, None) };
        self.pipeline_cache = cache;
        true
    }

    /// What the pipeline cache holds, to save for the next run.
    pub fn pipeline_cache_data(&self) -> Result<Vec<u8>> {
        Ok(unsafe { self.device.get_pipeline_cache_data(self.pipeline_cache)? })
    }

    /// The header this GPU's driver writes at the start of its pipeline
    /// cache's data: data with any other is for another GPU or driver.
    pub fn pipeline_cache_header(&self) -> [u8; CACHE_HEADER] {
        let properties = unsafe { self.instance.get_physical_device_properties(self.physical) };
        let mut header = [0; CACHE_HEADER];
        let words = [CACHE_HEADER as u32, vk::PipelineCacheHeaderVersion::ONE.as_raw() as u32];
        let words = words.into_iter().chain([properties.vendor_id, properties.device_id]);
        for (at, word) in words.enumerate() {
            header[at * 4..at * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        header[16..].copy_from_slice(&properties.pipeline_cache_uuid);
        header
    }
}

/// A pipeline to make later, perhaps on another thread: what
/// [`create_graphics_pipeline`] or [`create_compute_pipeline`] is given,
/// kept so that many can be made at once (see [`make_pipelines`]).
pub enum PipelineJob {
    Graphics { layout: vk::PipelineLayout, desc: Box<KeptGraphicsDesc> },
    Compute { layout: vk::PipelineLayout, name: String, module: ShaderModule, entry: CString },
}

/// A [`GraphicsPipelineDesc`] that keeps its own names and formats.
pub struct KeptGraphicsDesc {
    name: String,
    module: ShaderModule,
    vertex_entry: CString,
    task: Option<(ShaderModule, CString)>,
    mesh_entry: Option<CString>,
    fragment_entry: Option<CString>,
    color_formats: Vec<vk::Format>,
    depth_format: Option<vk::Format>,
    depth_write: bool,
    depth_compare: vk::CompareOp,
    cull: vk::CullModeFlags,
    front_face: vk::FrontFace,
    polygon_mode: vk::PolygonMode,
    blend: Blend,
    depth_bias: Option<(f32, f32)>,
    depth_clamp: bool,
}

impl PipelineJob {
    pub fn graphics(layout: vk::PipelineLayout, desc: &GraphicsPipelineDesc) -> Self {
        let module = |module: &ShaderModule| ShaderModule { handle: module.handle };
        let desc = KeptGraphicsDesc {
            name: desc.name.to_string(),
            module: module(desc.module),
            vertex_entry: desc.vertex_entry.to_owned(),
            task: desc.task.map(|(task, entry)| (module(task), entry.to_owned())),
            mesh_entry: desc.mesh_entry.map(CStr::to_owned),
            fragment_entry: desc.fragment_entry.map(CStr::to_owned),
            color_formats: desc.color_formats.to_vec(),
            depth_format: desc.depth_format,
            depth_write: desc.depth_write,
            depth_compare: desc.depth_compare,
            cull: desc.cull,
            front_face: desc.front_face,
            polygon_mode: desc.polygon_mode,
            blend: desc.blend,
            depth_bias: desc.depth_bias,
            depth_clamp: desc.depth_clamp,
        };
        Self::Graphics { layout, desc: Box::new(desc) }
    }

    pub fn compute(layout: vk::PipelineLayout, name: &str, module: &ShaderModule, entry: &CStr) -> Self {
        let module = ShaderModule { handle: module.handle };
        Self::Compute { layout, name: name.to_string(), module, entry: entry.to_owned() }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Graphics { desc, .. } => &desc.name,
            Self::Compute { name, .. } => name,
        }
    }

    /// Makes it, now, on this thread.
    pub fn make(&self, gpu: &Gpu) -> Result<vk::Pipeline> {
        match self {
            Self::Graphics { layout, desc: kept } => {
                let desc = GraphicsPipelineDesc {
                    name: &kept.name,
                    module: &kept.module,
                    vertex_entry: &kept.vertex_entry,
                    task: kept.task.as_ref().map(|(module, entry)| (module, entry.as_c_str())),
                    mesh_entry: kept.mesh_entry.as_deref(),
                    fragment_entry: kept.fragment_entry.as_deref(),
                    color_formats: &kept.color_formats,
                    depth_format: kept.depth_format,
                    depth_write: kept.depth_write,
                    depth_compare: kept.depth_compare,
                    cull: kept.cull,
                    front_face: kept.front_face,
                    polygon_mode: kept.polygon_mode,
                    blend: kept.blend,
                    depth_bias: kept.depth_bias,
                    depth_clamp: kept.depth_clamp,
                };
                create_graphics_pipeline(gpu, *layout, &desc)
            }
            Self::Compute { layout, name, module, entry } => create_compute_pipeline(gpu, *layout, name, module, entry),
        }
    }
}

/// Makes `jobs` on up to `threads` threads at once, as compiling each takes
/// the driver a while, and gives them back in the order asked for.
/// `progress` hears how many are made, of how many, now and then while
/// they are made, on the calling thread. On an error the pipelines made are
/// destroyed and the first error is returned.
pub fn make_pipelines(
    gpu: &Gpu,
    jobs: &[PipelineJob],
    threads: usize,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<vk::Pipeline>> {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    let (next, made) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let results: Vec<Mutex<Option<Result<(vk::Pipeline, std::time::Duration)>>>> =
        jobs.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, jobs.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(at) else { break };
                    let started = std::time::Instant::now();
                    let result = job.make(gpu).map(|pipeline| (pipeline, started.elapsed()));
                    *results[at].lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result);
                    made.fetch_add(1, Ordering::Release);
                }
            });
        }
        loop {
            let done = made.load(Ordering::Acquire);
            progress(done, jobs.len());
            if done >= jobs.len() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    });
    let results: Vec<Result<(vk::Pipeline, std::time::Duration)>> = results
        .into_iter()
        .map(|slot| slot.into_inner().unwrap_or_else(|poisoned| poisoned.into_inner()).expect("every job was made"))
        .collect();
    if results.iter().any(Result::is_err) {
        for (pipeline, _) in results.iter().filter_map(|result| result.as_ref().ok()) {
            unsafe { gpu.device.destroy_pipeline(*pipeline, None) };
        }
        return Err(results.into_iter().find_map(Result::err).expect("one failed"));
    }
    let made: Vec<_> = results.into_iter().map(|result| result.expect("all made")).collect();
    // The slowest are what a first start waits for, however many threads there are.
    let mut slowest: Vec<(&str, f32)> =
        jobs.iter().zip(&made).map(|(job, (_, took))| (job.name(), took.as_secs_f32())).collect();
    slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
    let slowest: Vec<String> = slowest.iter().take(4).map(|(name, took)| format!("{name} {took:.1} s")).collect();
    tracing::debug!("slowest pipelines to make: {}", slowest.join(", "));
    Ok(made.into_iter().map(|(pipeline, _)| pipeline).collect())
}
