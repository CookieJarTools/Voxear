//! The passes of one frame, in order:
//!
//! 1. **Reset** draw counters and stats, then build **Light clusters**.
//! 2. **Shadow cull** and **Shadows**: casters per sun cascade into the atlas.
//! 3. **Cull (early)** and **Opaque (early)**: what was visible last frame.
//! 4. **Depth pyramid** from that depth.
//! 5. **Cull (late)** and **Opaque (late)**: whatever the pyramid does not
//!    hide that early missed, which also records visibility for next frame.
//! 6. **Sky** behind everything.
//!
//! With ambient occlusion on, steps 3 and 5 draw depth alone (**Depth
//! (early)** and **Depth (late)**); ambient occlusion's passes then work from
//! that depth, and **Opaque (early)** and **Opaque (late)** shade the same
//! lists over it, before the sky. See
//! [`ambient_occlusion`](crate::ambient_occlusion).
//!
//! With meshlet culling, each cull queues its draws' meshlets rather than
//! the draws, and a meshlet pass after it (**Shadow meshlets**, **Meshlets
//! (early)**, **Meshlets (late)**) draws those that survive the frustum,
//! their normal cones and, late, the depth pyramid, which the passes after
//! it draw. Where the GPU has mesh shaders, the draw passes' task shaders
//! cull the queued meshlets instead, and there is no meshlet pass.
//!
//! Opaque and sky passes also write motion vectors. Post-processing follows,
//! in [`post_passes`](crate::post_passes).

use voxear_rhi::{StorageImageId, TextureId, vk};

use crate::{
    ambient_occlusion, atmosphere, ddgi, glass,
    gpu::{
        CLUSTER_COUNT, DRAW_COMMAND_SIZE, DRAWS_AGAIN, DRAWS_INSTANCES, DRAWS_MESHLETS, LIST_EARLY, LIST_LATE,
        LIST_SHADOW, Push, cleared_draw_counts, draw_count_offset, meshlet_count_offset, meshlet_list, stat,
    },
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    pipelines::Pipelines,
    shadows::{CASCADE_RESOLUTION, CASCADES},
    volumetric,
};

pub struct Targets {
    pub hdr: ImageId,
    pub velocity: ImageId,
    pub depth: ImageId,
    pub depth_texture: TextureId,
    pub pyramid: ImageId,
    pub pyramid_texture: TextureId,
    pub pyramid_mips: Vec<StorageImageId>,
    pub pyramid_size: [u32; 2],
    pub shadows: Option<ImageId>,
}

pub struct Draws {
    pub draws: BufferId,
    pub count: BufferId,
    pub instances: BufferId,
    pub visibility: BufferId,
    pub lod_state: BufferId,
    /// The grid of pushes through foliage, which plants read where they stand.
    pub foliage_pushes: BufferId,
    pub stats: BufferId,
    /// Where the stats are copied for the CPU to read.
    pub stats_readback: BufferId,
    pub clusters: BufferId,
    pub meshlet_draws: BufferId,
    pub meshlet_instances: BufferId,
    pub meshlet_work: BufferId,
    pub draws_handle: vk::Buffer,
    pub count_handle: vk::Buffer,
    pub stats_handle: vk::Buffer,
    pub stats_readback_handle: vk::Buffer,
    pub index_buffer: vk::Buffer,
    pub meshlet_draws_handle: vk::Buffer,
    /// How many meshlets the instance lists' draws can turn into, which
    /// lays out the meshlet lists; `None` draws the instance lists whole.
    pub meshlet_capacity: Option<u32>,
    /// With meshlet culling, draw with task and mesh shaders rather than a
    /// meshlet pass. Only where the pipelines have them.
    pub mesh_shaders: bool,
}

pub struct Frame {
    pub pipelines: Pipelines,
    pub push: Push,
    pub instance_count: u32,
    pub extent: vk::Extent2D,
    /// Opaque meshes are drawn as their edges.
    pub wireframe: bool,
    /// Opaque meshes' edges are drawn faintly over them, lit as usual.
    pub edges: bool,
    /// Ambient occlusion's images, when it is on: the opaque lists are then
    /// drawn as depth first, and shaded once it has run.
    pub ambient_occlusion: Option<ambient_occlusion::Targets>,
    /// Contact shadows' image, when they are on: like ambient occlusion,
    /// they need the depth drawn first.
    pub contact_shadows: Option<ImageId>,
    /// Screen-space reflections' image, and last frame's finished picture
    /// they read, when they are on; they need the depth drawn first too.
    pub reflections: Option<(ImageId, ImageId)>,
    /// The atmosphere's tables, when the sky is the atmosphere: drawn first,
    /// and sampled by the opaque passes and the sky.
    pub atmosphere: Option<atmosphere::Tables>,
    /// The volumetric fog's volumes, filled once the opaque passes are done.
    pub volume: Option<volumetric::Volume>,
    /// The glass in view, drawn after the sky.
    pub glass: Option<glass::Glass>,
    /// The GI's probes, whose rays are traced once the opaque passes are
    /// done; shading reads last frame's.
    pub probes: Option<ddgi::Probes>,
}

pub fn record(fg: &mut FrameGraph<'_>, frame: &Frame, targets: &Targets, draws: &Draws) {
    let culling = [
        (draws.count, BufferAccess::ComputeWrite),
        (draws.draws, BufferAccess::ComputeWrite),
        (draws.instances, BufferAccess::ComputeWrite),
        (draws.meshlet_work, BufferAccess::ComputeWrite),
        (draws.visibility, BufferAccess::ComputeWrite),
        (draws.lod_state, BufferAccess::ComputeWrite),
        (draws.stats, BufferAccess::ComputeWrite),
    ];
    // The count is read as the dispatch's size before the pass writes the meshlet lists' counts beside it.
    let meshlet_culling = [
        (draws.count, BufferAccess::IndirectRead),
        (draws.count, BufferAccess::ComputeWrite),
        (draws.meshlet_work, BufferAccess::ComputeRead),
        (draws.meshlet_draws, BufferAccess::ComputeWrite),
        (draws.meshlet_instances, BufferAccess::ComputeWrite),
        (draws.stats, BufferAccess::ComputeWrite),
    ];
    let meshlets = draws.meshlet_capacity.is_some();
    let mesh = if meshlets && draws.mesh_shaders { frame.pipelines.mesh } else { None };
    // The lists the draw passes read, as commands and as data (the meshlets debug view reads their first indices);
    // or with mesh shaders, the queued meshlets their task shaders cull, counting what they cull.
    let (list_draws, list_instances) =
        if meshlets { (draws.meshlet_draws, draws.meshlet_instances) } else { (draws.draws, draws.instances) };
    // Every pass that places a plant's vertices reads the pushes through foliage.
    let pushes_read = match mesh {
        Some(_) => vec![(draws.foliage_pushes, BufferAccess::ShaderRead), (draws.foliage_pushes, BufferAccess::MeshShaderRead)],
        None => vec![(draws.foliage_pushes, BufferAccess::ShaderRead)],
    };
    let mut drawing = vec![(draws.count, BufferAccess::IndirectRead)];
    drawing.extend(pushes_read.iter().copied());
    match mesh {
        Some(_) => drawing.extend([(draws.meshlet_work, BufferAccess::MeshShaderRead)]),
        None => drawing.extend([
            (list_draws, BufferAccess::IndirectRead),
            (list_draws, BufferAccess::ShaderRead),
            (list_instances, BufferAccess::ShaderRead),
        ]),
    }
    let edge_buffers: Vec<_> = drawing
        .iter()
        .copied()
        .chain([(draws.clusters, BufferAccess::ShaderRead), (draws.lod_state, BufferAccess::ShaderRead)])
        .collect();
    let counted = mesh.map(|_| (draws.stats, BufferAccess::TaskWrite));
    let shading_buffers: Vec<_> = edge_buffers.iter().copied().chain(counted).collect();
    drawing.extend(counted);

    let (count_handle, stats_handle) = (draws.count_handle, draws.stats_handle);
    fg.add_pass(
        "Reset",
        &[],
        &[(draws.count, BufferAccess::TransferWrite), (draws.stats, BufferAccess::TransferWrite)],
        move |ctx| unsafe {
            let counts = cleared_draw_counts();
            ctx.gpu.device.cmd_update_buffer(ctx.cmd, count_handle, 0, bytemuck::cast_slice(&counts));
            ctx.gpu.device.cmd_fill_buffer(ctx.cmd, stats_handle, 0, stat::COUNT as u64 * 4, 0);
        },
    );

    let push = frame.push;
    let instances = frame.instance_count;
    let pipelines = frame.pipelines;

    crate::foliage::record(fg, (pipelines, push), draws.foliage_pushes);

    fg.add_pass(
        "Light clusters",
        &[],
        &[(draws.clusters, BufferAccess::ComputeWrite), (draws.stats, BufferAccess::ComputeWrite)],
        move |ctx| {
            bind(ctx, pipelines.clusters, vk::PipelineBindPoint::COMPUTE, &push);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, CLUSTER_COUNT.div_ceil(64), 1, 1) };
        },
    );
    let indirect = Indirect {
        draws: draws.draws_handle,
        count: draws.count_handle,
        index_buffer: draws.index_buffer,
        meshlets: draws.meshlet_capacity.map(|capacity| (draws.meshlet_draws_handle, capacity)),
        tasks: mesh.is_some(),
    };
    // Draw passes read the meshlet lists when there are any.
    let draw_push = Push { source: if meshlets { DRAWS_MESHLETS } else { DRAWS_INSTANCES }, ..push };
    let meshlet_pass = |fg: &mut FrameGraph<'_>, name: &str, images: &[(ImageId, ImageAccess)], lists: [u32; 2]| {
        if meshlets && mesh.is_none() {
            fg.add_pass(name, images, &meshlet_culling, move |ctx| {
                cull_meshlets(ctx, pipelines.meshlet_cull, &push, count_handle, lists[0]..lists[1]);
            });
        }
    };

    if let Some(atlas) = targets.shadows {
        fg.add_pass("Shadow cull", &[], &culling, move |ctx| {
            cull(ctx, pipelines.cull, &push, LIST_SHADOW, instances, CASCADES as u32);
        });
        meshlet_pass(fg, "Shadow meshlets", &[], [LIST_SHADOW, LIST_SHADOW + CASCADES as u32]);
        fg.add_pass("Shadows", &[(atlas, ImageAccess::DepthAttachment)], &drawing, move |ctx| {
            let size = CASCADE_RESOLUTION * 2;
            let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, Some(clear_depth(1.0)));
            begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
            let shadow = mesh.map_or(pipelines.shadow, |mesh| mesh.shadow);
            bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for cascade in 0..CASCADES as u32 {
                let offset = vk::Offset2D {
                    x: (cascade % 2 * CASCADE_RESOLUTION) as i32,
                    y: (cascade / 2 * CASCADE_RESOLUTION) as i32,
                };
                set_viewport(ctx, offset, vk::Extent2D { width: CASCADE_RESOLUTION, height: CASCADE_RESOLUTION });
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: LIST_SHADOW + cascade, ..draw_push });
                indirect.draw(ctx, LIST_SHADOW + cascade);
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }

    let ambient_occlusion = frame.ambient_occlusion;
    let contact_shadows = frame.contact_shadows;
    let reflections = frame.reflections;
    // Drawn as depth first, then shaded over it, for what reads the depth before shading.
    let depth_first = ambient_occlusion.is_some() || contact_shadows.is_some() || reflections.is_some();
    if let Some(atmosphere) = frame.atmosphere {
        atmosphere::record(fg, (pipelines, push), atmosphere);
    }
    let tables: Vec<ImageId> = frame.atmosphere.map(|atmosphere| atmosphere.all().to_vec()).unwrap_or_default();
    let shading = |color: ImageAccess| {
        let mut images =
            vec![(targets.hdr, color), (targets.velocity, color), (targets.depth, ImageAccess::DepthAttachment)];
        images.extend(targets.shadows.map(|atlas| (atlas, ImageAccess::Sampled)));
        images.extend(ambient_occlusion.map(|ao| (ao.current, ImageAccess::Sampled)));
        images.extend(contact_shadows.map(|contact| (contact, ImageAccess::Sampled)));
        images.extend(reflections.map(|(target, _)| (target, ImageAccess::Sampled)));
        // The GI's probes, which shading and reflections read.
        images.extend(frame.probes.into_iter().flat_map(|probes| {
            [probes.irradiance, probes.depth, probes.states].map(|image| (image, ImageAccess::Sampled))
        }));
        images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
        images
    };
    let shading_push = Push { size: [CASCADE_RESOLUTION; 2], ..draw_push };
    let forward = match (mesh, frame.wireframe) {
        (Some(mesh), true) => mesh.wireframe,
        (Some(mesh), false) => mesh.forward,
        (None, true) => pipelines.wireframe,
        (None, false) => pipelines.forward,
    };
    let (hdr, velocity, depth) = (targets.hdr, targets.velocity, targets.depth);
    let scene = SceneTargets { hdr, velocity, depth };
    let depth_pass = |fg: &mut FrameGraph<'_>, name: &str, list: u32| {
        let pipeline = mesh.map_or(pipelines.depth, |mesh| mesh.depth);
        let push = Push { list, ..draw_push };
        fg.add_pass(name, &[(depth, ImageAccess::DepthAttachment)], &shading_buffers, move |ctx| {
            depth_only(ctx, pipeline, &push, indirect, depth, list == LIST_EARLY);
        });
    };

    fg.add_pass("Cull (early)", &[], &culling, move |ctx| {
        cull(ctx, pipelines.cull, &push, LIST_EARLY, instances, 1);
    });
    meshlet_pass(fg, "Meshlets (early)", &[], [LIST_EARLY, LIST_LATE]);
    match depth_first {
        true => depth_pass(fg, "Depth (early)", LIST_EARLY),
        false => fg.add_pass("Opaque (early)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
            let clear = Clear { color: true, depth: true };
            opaque(ctx, forward, &Push { list: LIST_EARLY, ..shading_push }, indirect, scene, clear);
        }),
    }

    let pyramid = targets.pyramid;
    let pyramid_mips = targets.pyramid_mips.clone();
    let (depth_texture, pyramid_texture, pyramid_size) =
        (targets.depth_texture, targets.pyramid_texture, targets.pyramid_size);
    fg.add_pass(
        "Depth pyramid",
        &[(depth, ImageAccess::Sampled), (pyramid, ImageAccess::StorageWrite)],
        &[],
        move |ctx| {
            build_pyramid(ctx, pipelines.pyramid, &push, depth_texture, pyramid_texture, &pyramid_mips, pyramid_size)
        },
    );

    // With mesh shaders, the late pass's task shaders sample the pyramid too, after this pass.
    let late_pyramid = if mesh.is_some() { ImageAccess::SampledWithTasks } else { ImageAccess::Sampled };
    fg.add_pass("Cull (late)", &[(pyramid, late_pyramid)], &culling, move |ctx| {
        cull(ctx, pipelines.cull, &push, LIST_LATE, instances, 1);
    });
    meshlet_pass(fg, "Meshlets (late)", &[(pyramid, ImageAccess::Sampled)], [LIST_LATE, LIST_SHADOW]);
    match depth_first {
        true => {
            depth_pass(fg, "Depth (late)", LIST_LATE);
            if let Some(ao) = ambient_occlusion {
                ambient_occlusion::record(fg, (pipelines, push), depth, ao);
            }
            if let Some(contact) = contact_shadows {
                ambient_occlusion::record_contact_shadows(fg, (pipelines, push), depth, contact);
            }
            if let Some((target, history)) = reflections {
                ambient_occlusion::record_reflections(fg, (pipelines, push), depth, history, target);
            }
            // Both lists again, over their own depth. Task shaders cull them again, without counting them again.
            let forward = mesh.map_or(pipelines.forward_on_depth, |mesh| mesh.forward_on_depth);
            let again = if mesh.is_some() { Push { source: DRAWS_AGAIN, ..shading_push } } else { shading_push };
            for (name, list) in [("Opaque (early)", LIST_EARLY), ("Opaque (late)", LIST_LATE)] {
                let clear = Clear { color: list == LIST_EARLY, depth: false };
                fg.add_pass(name, &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
                    opaque(ctx, forward, &Push { list, ..again }, indirect, scene, clear);
                });
            }
        }
        false => fg.add_pass("Opaque (late)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
            opaque(ctx, forward, &Push { list: LIST_LATE, ..shading_push }, indirect, scene, Clear::default());
        }),
    }
    if let Some(probes) = frame.probes {
        ddgi::record(fg, (pipelines, push), probes);
    }
    if let Some(volume) = frame.volume {
        volumetric::record(fg, (pipelines, push), volume, targets.shadows, &[draws.clusters]);
    }
    // Counted in GPU memory, where the culling passes' atomics are fast, then copied out at once.
    let (readback, stats_bytes) = (draws.stats_readback_handle, stat::COUNT as u64 * 4);
    let copy = [(draws.stats, BufferAccess::TransferRead), (draws.stats_readback, BufferAccess::TransferWrite)];
    fg.add_pass("Read back stats", &[], &copy, move |ctx| unsafe {
        let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: stats_bytes };
        ctx.gpu.device.cmd_copy_buffer(ctx.cmd, stats_handle, readback, &[region]);
    });
    fg.sync_buffer(draws.stats_readback, BufferAccess::HostRead);

    let extent = frame.extent;
    let mut sky_images = vec![
        (hdr, ImageAccess::ColorAttachment),
        (velocity, ImageAccess::ColorAttachment),
        (depth, ImageAccess::DepthRead),
    ];
    sky_images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
    fg.add_pass("Sky", &sky_images, &[], move |ctx| {
        let color = [
            attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
            attachment(ctx.view(velocity), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None),
        ];
        let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL, None);
        begin_rendering(ctx, extent, &color, Some(&depth));
        fullscreen(ctx, pipelines.sky, &push);
    });

    if let Some(glass) = frame.glass.clone() {
        let mut sampled: Vec<ImageId> = targets.shadows.into_iter().chain(tables.iter().copied()).collect();
        sampled.extend(frame.probes.into_iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]));
        let index_buffer = draws.index_buffer;
        let glass_targets =
            glass::GlassTargets { hdr, depth, sampled, clusters: draws.clusters, pushes: draws.foliage_pushes, index_buffer };
        glass::record(fg, (pipelines, push), glass, glass_targets);
    }

    if frame.edges {
        let images = [(hdr, ImageAccess::ColorAttachment), (depth, ImageAccess::DepthRead)];
        // Task shaders cull the lists again for the edges, without counting them again.
        let edges_push = if mesh.is_some() { Push { source: DRAWS_AGAIN, ..shading_push } } else { shading_push };
        fg.add_pass("Edges", &images, &edge_buffers, move |ctx| {
            let color = [attachment(ctx.view(hdr), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
            let depth = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_READ_ONLY_OPTIMAL, None);
            begin_rendering(ctx, extent, &color, Some(&depth));
            let edges = mesh.map_or(pipelines.edges, |mesh| mesh.edges);
            bind(ctx, edges, vk::PipelineBindPoint::GRAPHICS, &edges_push);
            // What both opaque passes drew.
            for list in [LIST_EARLY, LIST_LATE] {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, ..edges_push });
                indirect.draw(ctx, list);
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
}

/// Which of the opaque passes' targets they clear first: the first pass
/// clears colour, and depth too unless the depth passes drew it.
#[derive(Clone, Copy, Default)]
struct Clear {
    color: bool,
    depth: bool,
}

/// What the opaque passes draw into.
#[derive(Clone, Copy)]
struct SceneTargets {
    hdr: ImageId,
    velocity: ImageId,
    depth: ImageId,
}

/// The indirect draw buffers; each draw list occupies its own range.
#[derive(Clone, Copy)]
struct Indirect {
    draws: vk::Buffer,
    count: vk::Buffer,
    index_buffer: vk::Buffer,
    /// The meshlet lists, and the capacity they are laid out by, when the
    /// passes draw those instead of the instance lists.
    meshlets: Option<(vk::Buffer, u32)>,
    /// Draw the list's queued meshlets with task and mesh shaders, a task
    /// workgroup for each round of them, the count the cull grew.
    tasks: bool,
}

impl Indirect {
    fn draw(self, ctx: &PassContext, list: u32) {
        if let Some(mesh) = ctx.gpu.mesh_shader.as_ref().filter(|_| self.tasks) {
            let stride = 3 * size_of::<u32>() as u32;
            unsafe { mesh.cmd_draw_mesh_tasks_indirect(ctx.cmd, self.count, draw_count_offset(list), 1, stride) };
            return;
        }
        let max = crate::MAX_INSTANCES as u32;
        let (draws, first, count_offset, max) = match self.meshlets {
            Some((draws, capacity)) => {
                let (base, held) = meshlet_list(list, capacity);
                (draws, base, meshlet_count_offset(list), held)
            }
            None => (self.draws, u64::from(list) * u64::from(max), draw_count_offset(list), max),
        };
        unsafe {
            ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, self.index_buffer, 0, vk::IndexType::UINT32);
            ctx.gpu.device.cmd_draw_indexed_indirect_count(
                ctx.cmd,
                draws,
                first * DRAW_COMMAND_SIZE,
                self.count,
                count_offset,
                max,
                DRAW_COMMAND_SIZE as u32,
            );
        }
    }
}

fn cull(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, list: u32, instances: u32, lists: u32) {
    bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &Push { list, ..*push });
    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, instances.div_ceil(64).max(1), lists, 1) };
}

/// A workgroup for each round of meshlets the cull queued in each of
/// `lists`, sized by the count the cull grew, so the CPU never reads it.
fn cull_meshlets(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    count: vk::Buffer,
    lists: std::ops::Range<u32>,
) {
    bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, push);
    for list in lists {
        ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, ..*push });
        unsafe { ctx.gpu.device.cmd_dispatch_indirect(ctx.cmd, count, draw_count_offset(list)) };
    }
}

fn opaque(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    indirect: Indirect,
    targets: SceneTargets,
    clear: Clear,
) {
    let extent = ctx.extent(targets.hdr);
    let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
    let color = [
        attachment(ctx.view(targets.hdr), layout, clear.color.then(|| clear_color([0.0; 4]))),
        attachment(ctx.view(targets.velocity), layout, clear.color.then(|| clear_color([0.0; 4]))),
    ];
    let depth_layout = vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL;
    let depth = attachment(ctx.view(targets.depth), depth_layout, clear.depth.then(|| clear_depth(0.0)));
    begin_rendering(ctx, extent, &color, Some(&depth));
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    indirect.draw(ctx, push.list);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// One opaque list as depth alone, the first clearing it.
fn depth_only(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, indirect: Indirect, depth: ImageId, clear: bool) {
    let target =
        attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, clear.then(|| clear_depth(0.0)));
    begin_rendering(ctx, ctx.extent(depth), &[], Some(&target));
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    indirect.draw(ctx, push.list);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// Mip 0 reads the depth buffer; every later mip reads the one before it.
fn build_pyramid(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    depth: TextureId,
    pyramid: TextureId,
    mips: &[StorageImageId],
    size: [u32; 2],
) {
    for (mip, output) in mips.iter().enumerate() {
        let mip = mip as u32;
        let width = (size[0] >> mip).max(1);
        let height = (size[1] >> mip).max(1);
        let push = Push {
            texture: if mip == 0 { depth.0 } else { pyramid.0 },
            level: mip.saturating_sub(1),
            output: output.0,
            size: [width, height],
            ..*push
        };
        bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, width.div_ceil(8), height.div_ceil(8), 1) };
        let barrier = [vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)];
        let dependency = vk::DependencyInfo::default().memory_barriers(&barrier);
        unsafe { ctx.gpu.device.cmd_pipeline_barrier2(ctx.cmd, &dependency) };
    }
}

pub(crate) fn bind(ctx: &PassContext, pipeline: vk::Pipeline, bind_point: vk::PipelineBindPoint, push: &Push) {
    unsafe { ctx.gpu.device.cmd_bind_pipeline(ctx.cmd, bind_point, pipeline) };
    ctx.bindless.bind(ctx.gpu, ctx.cmd, bind_point);
    ctx.bindless.push(ctx.gpu, ctx.cmd, push);
}

fn clear_color(color: [f32; 4]) -> vk::ClearValue {
    vk::ClearValue { color: vk::ClearColorValue { float32: color } }
}

fn clear_depth(depth: f32) -> vk::ClearValue {
    vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 } }
}

/// A rendering attachment that clears when given a value and loads otherwise.
fn attachment(
    view: vk::ImageView,
    layout: vk::ImageLayout,
    clear: Option<vk::ClearValue>,
) -> vk::RenderingAttachmentInfo<'static> {
    let info = vk::RenderingAttachmentInfo::default()
        .image_view(view)
        .image_layout(layout)
        .store_op(vk::AttachmentStoreOp::STORE);
    match clear {
        Some(value) => info.load_op(vk::AttachmentLoadOp::CLEAR).clear_value(value),
        None => info.load_op(vk::AttachmentLoadOp::LOAD),
    }
}

pub(crate) fn begin_rendering(
    ctx: &PassContext,
    extent: vk::Extent2D,
    color: &[vk::RenderingAttachmentInfo],
    depth: Option<&vk::RenderingAttachmentInfo>,
) {
    let area = vk::Rect2D { offset: vk::Offset2D::default(), extent };
    let mut info = vk::RenderingInfo::default().render_area(area).layer_count(1).color_attachments(color);
    if let Some(depth) = depth {
        info = info.depth_attachment(depth);
    }
    unsafe { ctx.gpu.device.cmd_begin_rendering(ctx.cmd, &info) };
    set_viewport(ctx, vk::Offset2D::default(), extent);
}

/// Begins rendering into `rect` of the colour attachments, with the viewport
/// and scissor on it, so a fullscreen pass fills just that rectangle.
pub(crate) fn begin_rendering_in(ctx: &PassContext, rect: vk::Rect2D, color: &[vk::RenderingAttachmentInfo]) {
    begin_rendering_rect(ctx, rect, color, None);
}

/// Begins rendering into `rect` of the attachments, depth too, with the
/// viewport and scissor on it; clears touch only the rectangle.
pub(crate) fn begin_rendering_rect(
    ctx: &PassContext,
    rect: vk::Rect2D,
    color: &[vk::RenderingAttachmentInfo],
    depth: Option<&vk::RenderingAttachmentInfo>,
) {
    let mut info = vk::RenderingInfo::default().render_area(rect).layer_count(1).color_attachments(color);
    if let Some(depth) = depth {
        info = info.depth_attachment(depth);
    }
    unsafe { ctx.gpu.device.cmd_begin_rendering(ctx.cmd, &info) };
    set_viewport(ctx, rect.offset, rect.extent);
}

fn set_viewport(ctx: &PassContext, offset: vk::Offset2D, extent: vk::Extent2D) {
    let viewport = vk::Viewport {
        x: offset.x as f32,
        y: offset.y as f32,
        width: extent.width as f32,
        height: extent.height as f32,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    unsafe {
        ctx.gpu.device.cmd_set_viewport(ctx.cmd, 0, &[viewport]);
        ctx.gpu.device.cmd_set_scissor(ctx.cmd, 0, &[vk::Rect2D { offset, extent }]);
    }
}

/// Draws one screen-covering triangle and ends the rendering scope.
pub(crate) fn fullscreen(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push) {
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    unsafe {
        ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0);
        ctx.gpu.device.cmd_end_rendering(ctx.cmd);
    }
}
