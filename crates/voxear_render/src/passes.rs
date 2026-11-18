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
        LAMP_PAGE_VIEW, LIST_LOCAL_SHADOW, LIST_SHADOW, LIST_TWO_SIDED, LOCAL_SHADOW_ATLAS, LOCAL_SHADOW_COLUMNS,
        LOCAL_SHADOW_TILE, Push, VSM_CULL, VSM_CULL_MOVING, VSM_PAGE_VIEW, cleared_draw_counts, draw_count_offset,
        meshlet_count_offset, meshlet_list, stat, two_sided_shadow,
    },
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    pipelines::Pipelines,
    shadow_cache::LOCAL_CULL_STILL,
    shadows::CASCADES,
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
    /// How many texels a side each of the sun's cascades has in it.
    pub cascade_size: u32,
    /// The local lights' shadow atlases, when any local light casts a
    /// shadow this frame.
    pub local_shadows: Option<LocalShadows>,
    /// The sun's virtual shadow maps, in place of `shadows`.
    pub virtual_shadows: Option<VirtualShadows>,
    /// The glass shadow atlas and the panes drawn into it, when glass is in
    /// view and the sun or a local light casts shadows.
    pub glass_shadows: Option<(glass::Shadows, Vec<glass::Pane>)>,
    /// The colour the sun takes through leaves, from its cascades, and the
    /// blocks drawn into it, for a shader pack that lets the sun through.
    pub leaf_light: Option<(ImageId, crate::blocks::ViewBlocks)>,
}

/// What a view's virtual shadow maps draw into and read (see `vsm`): the
/// pool; each level with pages drawn this frame and those pages, each by
/// its place among the frame's pages and its slot; and where the view asks
/// for the pages it needs.
#[derive(Clone)]
pub struct VirtualShadows {
    pub pool: ImageId,
    pub levels: Vec<(usize, Vec<(u32, u16)>)>,
    /// The moving pool and, by level, the pages what moves stands in this
    /// frame: each its place among the frame's pages, its slot in the pool
    /// and in the moving pool.
    pub moving: Option<(ImageId, Vec<(usize, Vec<(u32, u16, u16)>)>)>,
    pub requests: (BufferId, vk::Buffer),
    /// Where the requests are copied for the CPU to read.
    pub readback: (BufferId, vk::Buffer),
    pub table: TableCopy,
}

/// What a view's local lights' shadows draw into: the local shadow atlas;
/// the view's kept atlas of what stands still, where any light keeps it
/// (see `shadow_cache`); each slot's first view and how many its light
/// takes; and which slots keep what stands still and draw it again.
#[derive(Clone)]
pub struct LocalShadows {
    pub atlas: ImageId,
    pub kept: Option<ImageId>,
    pub views: Vec<(u32, u32)>,
    pub lamps: crate::shadow_cache::Lamps,
    /// The lamps' virtual shadow pages, where lamps have them.
    pub pages: Option<LampPages>,
}

/// The lamps' virtual shadow pages a frame draws (see `lamp_vsm`): the
/// pool; each page drawn, in the frame's order, by its lamp's slot and its
/// slot of the pool; and the frame's requests and where they are copied
/// for the CPU.
#[derive(Clone)]
pub struct LampPages {
    pub pool: ImageId,
    pub drawn: Vec<(u32, u16)>,
    pub requests: (BufferId, vk::Buffer),
    pub readback: (BufferId, vk::Buffer),
    pub table: TableCopy,
}

/// A page table as the CPU wrote it this frame, and the buffer in GPU
/// memory shading reads it from, and its size.
#[derive(Clone, Copy)]
pub struct TableCopy {
    pub from: (BufferId, vk::Buffer),
    pub to: (BufferId, vk::Buffer),
    pub size: u64,
}

/// Copies a page table into the buffer shading reads, before anything
/// reads it.
fn copy_table(fg: &mut FrameGraph<'_>, name: &str, table: TableCopy) {
    let ((from, from_handle), (to, to_handle)) = (table.from, table.to);
    let copying = [(from, BufferAccess::TransferRead), (to, BufferAccess::TransferWrite)];
    fg.add_pass(name, &[], &copying, move |ctx| unsafe {
        let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: table.size };
        ctx.gpu.device.cmd_copy_buffer(ctx.cmd, from_handle, to_handle, &[region]);
    });
    fg.sync_buffer(to, BufferAccess::ShaderRead);
}

pub struct Draws {
    pub draws: BufferId,
    pub count: BufferId,
    pub instances: BufferId,
    pub visibility: BufferId,
    pub lod_state: BufferId,
    /// The grid of pushes through foliage, which plants read where they stand.
    pub foliage_pushes: BufferId,
    /// The map of tracks round the camera, which the terrain reads, and
    /// whether it changes this frame: something presses, or it scrolls.
    pub tracks: BufferId,
    pub tracks_change: bool,
    pub stats: BufferId,
    /// Where the stats are copied for the CPU to read.
    pub stats_readback: BufferId,
    pub clusters: BufferId,
    pub meshlet_draws: BufferId,
    pub meshlet_instances: BufferId,
    pub meshlet_work: BufferId,
    /// Dicing's lists, counts and draws (see `dice`).
    pub diced_instances: BufferId,
    pub dice_counts: BufferId,
    pub dice_splits: BufferId,
    pub dice_patches: BufferId,
    pub dice_draws: BufferId,
    pub dice_counts_handle: vk::Buffer,
    pub dice_draws_handle: vk::Buffer,
    pub draws_handle: vk::Buffer,
    pub count_handle: vk::Buffer,
    pub stats_handle: vk::Buffer,
    pub stats_readback_handle: vk::Buffer,
    pub index_buffer: vk::Buffer,
    pub meshlet_draws_handle: vk::Buffer,
    /// How many meshlets the instance lists' draws can turn into, which
    /// lays out the meshlet lists; `None` draws the instance lists whole.
    pub meshlet_capacity: Option<crate::gpu::MeshletCapacity>,
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
    /// Each pixel's normal and roughness, which the depth passes write when
    /// they run (whenever ambient occlusion, contact shadows or reflections
    /// are on).
    pub surface: Option<ImageId>,
    /// What each pixel's surface is, written beside its normal by the depth
    /// passes, for a worn shader pack's passes that read it.
    pub surface_ids: Option<ImageId>,
    /// Screen-space reflections' image, and last frame's finished picture
    /// they read, when they are on; they need the depth drawn first too.
    pub reflections: Option<(ImageId, ImageId)>,
    /// The nearest-depth pyramid screen-space reflections walk through,
    /// when they are not traced (see `ambient_occlusion::pyramid_desc`).
    pub reflection_pyramid: Option<ImageId>,
    /// The atmosphere's tables, when the sky is the atmosphere: drawn first,
    /// and sampled by the opaque passes and the sky.
    pub atmosphere: Option<atmosphere::Tables>,
    /// The sky's clouds, where it has any: drawn after the tables, and
    /// sampled by the sky and by everything the sun lights.
    pub clouds: Option<crate::clouds::CloudImages>,
    /// The volumetric fog's volumes, filled once the opaque passes are done.
    pub volume: Option<volumetric::Volume>,
    /// The sea, drawn after the sky and before glass.
    pub water: Option<crate::water::Water>,
    /// The glass in view, drawn after the sky.
    pub glass: Option<glass::Glass>,
    /// How each Particle System is drawn, in the order they are listed,
    /// after glass; and the pool the simulated ones' particles are in.
    pub particles: Vec<crate::particles::Drawn>,
    pub particle_pool: Option<BufferId>,
    /// The simulated particles' step, in the view that records it.
    pub particle_step: Option<crate::particles::Step>,
    /// How many drops the sea's spray has, listed after the Particle Systems.
    pub spray: Option<u32>,
    /// The spray over Streams' rapids (`particles::gather`).
    pub rapids: Vec<crate::particles::RapidsSpray>,
    /// How many smokes and mists there are, drawn after the fog too.
    pub smokes: u32,
    pub mists: u32,
    /// The smoke's and mist's baked noise.
    pub smoke_noise: u32,
    /// The GI's probes, whose rays are traced once the opaque passes are
    /// done; shading reads last frame's.
    pub probes: Option<ddgi::Probes>,
    /// The terrains' patches in view and in the sun's cascades, drawn with
    /// what was visible last frame, before the depth pyramid.
    pub terrain: Option<crate::terrain::ViewTerrains>,
    /// The worlds of blocks' faces in view and in the sun's cascades (see
    /// `blocks`).
    pub blocks: Option<crate::blocks::ViewBlocks>,
    /// The grass this view scatters round its camera and draws after the
    /// terrain (see `grass`).
    pub grass: Option<crate::grass::ViewGrass>,
    /// Reflections trace the scene's triangles by the GPU's ray queries,
    /// with or without the GI, rather than the screen.
    pub traced_reflections: bool,
    /// How many Portals' openings the view draws into its depth (see
    /// `portals`): the first of its buffer's.
    pub portal_openings: u32,
    /// The view is one through a Portal: it draws over what the view it is
    /// seen in has drawn, and reads that view's sky tables.
    pub through: bool,
    /// It works out its own sky tables: a camera's own view, or one through
    /// a Portal into another dimension than the view it is seen in.
    pub own_sky: bool,
    /// Something casting a shadow has a two-sided material, so the shadows'
    /// two-sided lists are drawn.
    pub two_sided_casters: bool,
    /// A reflection probe to capture or load this frame, once the GI's rays
    /// are traced, and the pipelines it takes (see `reflection_probes`).
    pub probe_job: Option<(crate::reflection_probes::ProbeFrame, crate::reflection_probes::Job, crate::reflection_probes::ProbePipelines)>,
    /// The frame's sound rays, which the first camera's own view traces.
    pub sound_job: Option<(crate::sound_rays::SoundPass, crate::sound_rays::SoundPipelines)>,
    /// This frame's share of a lighting bake, with its pipelines.
    pub bake: Option<(crate::lightmapper::BakeFrame, [vk::Pipeline; 3])>,
}

/// Records the frame's passes up to post-processing, and gives back the view
/// under the water, which is drawn after the air's fog (`water::record_under`),
/// and the particles, drawn after it too. `far_sides` records the views
/// through the Portals it sees, once its own surfaces are drawn and before
/// its sky and water.
pub fn record(
    fg: &mut FrameGraph<'_>,
    frame: &Frame,
    targets: &Targets,
    draws: &Draws,
    (lit, far_sides): (&mut dyn FnMut(&mut FrameGraph<'_>), &mut dyn FnMut(&mut FrameGraph<'_>)),
) -> (Option<crate::water::UnderWater>, Option<crate::particles::Draw>, Option<crate::smoke::Draw>) {
    let culling = [
        (draws.count, BufferAccess::ComputeWrite),
        (draws.draws, BufferAccess::ComputeWrite),
        (draws.instances, BufferAccess::ComputeWrite),
        (draws.meshlet_work, BufferAccess::ComputeWrite),
        (draws.visibility, BufferAccess::ComputeWrite),
        (draws.lod_state, BufferAccess::ComputeWrite),
        (draws.stats, BufferAccess::ComputeWrite),
        (draws.dice_counts, BufferAccess::ComputeWrite),
        (draws.diced_instances, BufferAccess::ComputeWrite),
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
    // Models blending into the ground read the terrains' heights and shares too.
    let grounds = frame.terrain.iter().flat_map(|terrains| &terrains.heights);
    let grounds = grounds.map(|&id| (id, BufferAccess::ShaderRead));
    let shading_buffers: Vec<_> = edge_buffers.iter().copied().chain(counted).chain(grounds).collect();
    drawing.extend(counted);

    let (count_handle, stats_handle) = (draws.count_handle, draws.stats_handle);
    let dice_counts_handle = draws.dice_counts_handle;
    let reset = [
        (draws.count, BufferAccess::TransferWrite),
        (draws.stats, BufferAccess::TransferWrite),
        (draws.dice_counts, BufferAccess::TransferWrite),
    ];
    fg.add_pass("Reset", &[], &reset, move |ctx| unsafe {
        let counts = cleared_draw_counts();
        ctx.gpu.device.cmd_update_buffer(ctx.cmd, count_handle, 0, bytemuck::cast_slice(&counts));
        ctx.gpu.device.cmd_fill_buffer(ctx.cmd, stats_handle, 0, stat::COUNT as u64 * 4, 0);
        let dice = crate::dice::CLEARED_COUNTS;
        ctx.gpu.device.cmd_update_buffer(ctx.cmd, dice_counts_handle, 0, bytemuck::cast_slice(&dice));
    });

    let push = frame.push;
    let instances = frame.instance_count;
    let pipelines = frame.pipelines;

    crate::foliage::record(fg, (pipelines, push), draws.foliage_pushes);
    if draws.tracks_change {
        crate::tracks::record(fg, (pipelines, push), draws.tracks);
    }

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
        two_sided_shadows: frame.two_sided_casters,
    };
    // Draw passes read the meshlet lists when there are any.
    let draw_push = Push { source: if meshlets { DRAWS_MESHLETS } else { DRAWS_INSTANCES }, ..push };
    let meshlet_pass = |fg: &mut FrameGraph<'_>, name: &str, images: &[(ImageId, ImageAccess)], lists: [u32; 2]| {
        if meshlets && mesh.is_none() {
            let two_sided = indirect.two_sided_shadows && lists[0] >= LIST_SHADOW;
            fg.add_pass(name, images, &meshlet_culling, move |ctx| {
                cull_shadow_meshlets(ctx, pipelines.meshlet_cull, &push, count_handle, lists[0]..lists[1], two_sided);
            });
        }
    };

    if let Some(atlas) = targets.shadows {
        fg.add_pass("Shadow cull", &[], &culling, move |ctx| {
            cull(ctx, pipelines.cull, &push, LIST_SHADOW, instances, CASCADES as u32);
        });
        meshlet_pass(fg, "Shadow meshlets", &[], [LIST_SHADOW, LIST_SHADOW + CASCADES as u32]);
        let cascade_size = targets.cascade_size;
        fg.add_pass("Shadows", &[(atlas, ImageAccess::DepthAttachment)], &drawing, move |ctx| {
            let size = cascade_size * 2;
            let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, Some(clear_depth(1.0)));
            begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
            let shadow = mesh.map_or(pipelines.shadow, |mesh| mesh.shadow);
            let cascade_viewport = |cascade: u32| {
                let offset =
                    vk::Offset2D { x: (cascade % 2 * cascade_size) as i32, y: (cascade / 2 * cascade_size) as i32 };
                set_viewport(ctx, offset, vk::Extent2D { width: cascade_size, height: cascade_size });
            };
            bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for cascade in 0..CASCADES as u32 {
                cascade_viewport(cascade);
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: LIST_SHADOW + cascade, ..draw_push });
                indirect.draw(ctx, LIST_SHADOW + cascade);
            }
            if indirect.two_sided_shadows {
                let two_sided = mesh.map_or(pipelines.shadow_two_sided, |mesh| mesh.shadow_two_sided);
                bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                for cascade in 0..CASCADES as u32 {
                    cascade_viewport(cascade);
                    indirect.draw_two_sided_shadow(ctx, LIST_SHADOW + cascade, &draw_push);
                }
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    if let Some(vsm) = targets.virtual_shadows.clone() {
        record_virtual_shadows(fg, frame, &vsm, (&culling, &meshlet_culling, &drawing), draws, indirect);
    }
    if let Some(local) = targets.local_shadows.clone() {
        let count = local.views.len() as u32;
        let (blocks, terrains) = (frame.blocks.clone(), frame.terrain.clone());
        let mut lamp_drawing = drawing.clone();
        let pools = blocks.iter().flat_map(|blocks| blocks.pools.iter());
        let grounds = terrains.iter().flat_map(|terrains| terrains.heights.iter());
        lamp_drawing.extend(pools.chain(grounds).map(|&id| (id, BufferAccess::ShaderRead)));
        let tile_at = |view: u32| vk::Offset2D {
            x: (view % LOCAL_SHADOW_COLUMNS * LOCAL_SHADOW_TILE) as i32,
            y: (view / LOCAL_SHADOW_COLUMNS * LOCAL_SHADOW_TILE) as i32,
        };
        let tile = vk::Extent2D { width: LOCAL_SHADOW_TILE, height: LOCAL_SHADOW_TILE };
        let shadow = mesh.map_or(pipelines.local_shadow, |mesh| mesh.local_shadow);
        if let Some(pages) = local.pages.clone() {
            let lamps = (local.lamps.redraw, count);
            let lists = (&culling[..], &meshlet_culling[..], &lamp_drawing[..]);
            record_lamp_pages(fg, frame, &pages, lamps, lists, draws, indirect);
        }
        // What stands still round the lights that draw it again, into their kept views, cleared first.
        if let (Some(kept), true) = (local.kept, local.lamps.redraw != 0) {
            let redraw = local.lamps.redraw;
            fg.add_pass("Local shadow cull (still)", &[], &culling, move |ctx| {
                cull(
                    ctx,
                    pipelines.cull,
                    &Push { level: LOCAL_CULL_STILL, output: redraw, ..push },
                    LIST_LOCAL_SHADOW,
                    instances,
                    count,
                );
            });
            meshlet_pass(fg, "Local shadow meshlets (still)", &[], [LIST_LOCAL_SHADOW, LIST_LOCAL_SHADOW + count]);
            let (views, blocks, terrains) = (local.views.clone(), blocks.clone(), terrains.clone());
            fg.add_pass("Kept local shadows", &[(kept, ImageAccess::DepthAttachment)], &lamp_drawing, move |ctx| {
                let [width, height] = LOCAL_SHADOW_ATLAS;
                let depth = attachment(ctx.view(kept), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
                begin_rendering(ctx, vk::Extent2D { width, height }, &[], Some(&depth));
                let drawn: Vec<(u32, u32)> = views
                    .iter()
                    .enumerate()
                    .filter(|(slot, _)| redraw >> slot & 1 != 0)
                    .flat_map(|(slot, &(first, faces))| (first..first + faces).map(move |view| (slot as u32, view)))
                    .collect();
                let rects: Vec<vk::ClearRect> = drawn
                    .iter()
                    .map(|&(_, view)| vk::ClearRect {
                        rect: vk::Rect2D { offset: tile_at(view), extent: tile },
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .collect();
                let clear = vk::ClearAttachment::default()
                    .aspect_mask(vk::ImageAspectFlags::DEPTH)
                    .clear_value(clear_depth(1.0));
                unsafe { ctx.gpu.device.cmd_clear_attachments(ctx.cmd, &[clear], &rects) };
                bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                for &(slot, view) in &drawn {
                    set_viewport(ctx, tile_at(view), tile);
                    let list = LIST_LOCAL_SHADOW + slot;
                    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: view, ..draw_push });
                    indirect.draw(ctx, list);
                }
                if indirect.two_sided_shadows {
                    let two_sided = mesh.map_or(pipelines.local_shadow_two_sided, |mesh| mesh.local_shadow_two_sided);
                    bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                    for &(slot, view) in &drawn {
                        set_viewport(ctx, tile_at(view), tile);
                        let view_push = Push { level: view, ..draw_push };
                        indirect.draw_two_sided_shadow(ctx, LIST_LOCAL_SHADOW + slot, &view_push);
                    }
                }
                if let Some(blocks) = &blocks {
                    bind(ctx, pipelines.block_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                    for &(slot, view) in drawn.iter().filter(|(slot, _)| blocks.draws_for_lamp(*slot, true)) {
                        set_viewport(ctx, tile_at(view), tile);
                        crate::blocks::draw_lamp_runs(ctx, blocks, &push, slot, true, view);
                    }
                }
                // The ground, drawn for a lamp only when its kept views are.
                if let Some(terrains) = &terrains {
                    bind(ctx, pipelines.terrain_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                    for &(slot, view) in
                        drawn.iter().filter(|(slot, _)| crate::terrain::draws_for_lamp(terrains, *slot))
                    {
                        set_viewport(ctx, tile_at(view), tile);
                        crate::terrain::draw_lamp(ctx, terrains, &push, slot, view);
                    }
                }
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            });
            // The lights' lists empty again, for what moves.
            fg.add_pass("Local shadow lists reset", &[], &[(draws.count, BufferAccess::TransferWrite)], move |ctx| {
                reset_lists(ctx, count_handle, LIST_LOCAL_SHADOW, count);
            });
        }
        fg.add_pass("Local shadow cull", &[], &culling, move |ctx| {
            cull(ctx, pipelines.cull, &Push { level: 0, output: 0, ..push }, LIST_LOCAL_SHADOW, instances, count);
        });
        meshlet_pass(fg, "Local shadow meshlets", &[], [LIST_LOCAL_SHADOW, LIST_LOCAL_SHADOW + count]);
        // Each light's list drawn into each of its views: a spot light's one, a cube's six. Where a light keeps what
        // stands still, its kept views are copied in first where anything moving is drawn over them; the copy reads
        // the lists' counts to see.
        let mut images = vec![(local.atlas, ImageAccess::DepthAttachment)];
        images.extend(local.kept.map(|kept| (kept, ImageAccess::Sampled)));
        lamp_drawing.push((draws.count, BufferAccess::ShaderRead));
        let (atlas, views, lamps) = (local.atlas, local.views.clone(), local.lamps.clone());
        let copies = local.kept.is_some() && lamps.cached != 0;
        fg.add_pass("Local shadows", &images, &lamp_drawing, move |ctx| {
            let [width, height] = LOCAL_SHADOW_ATLAS;
            let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, Some(clear_depth(1.0)));
            begin_rendering(ctx, vk::Extent2D { width, height }, &[], Some(&depth));
            let all = || {
                views
                    .iter()
                    .enumerate()
                    .flat_map(|(slot, &(first, faces))| (first..first + faces).map(move |view| (slot as u32, view)))
            };
            if copies {
                bind(ctx, pipelines.kept_copy, vk::PipelineBindPoint::GRAPHICS, &push);
                for (_, view) in all().filter(|&(slot, _)| lamps.is_cached(slot as usize)) {
                    set_viewport(ctx, tile_at(view), tile);
                    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { level: view, ..push });
                    unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0) };
                }
            }
            bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for (slot, view) in all() {
                set_viewport(ctx, tile_at(view), tile);
                let list = LIST_LOCAL_SHADOW + slot;
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: view, ..draw_push });
                indirect.draw(ctx, list);
            }
            if indirect.two_sided_shadows {
                let two_sided = mesh.map_or(pipelines.local_shadow_two_sided, |mesh| mesh.local_shadow_two_sided);
                bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                for (slot, view) in all() {
                    set_viewport(ctx, tile_at(view), tile);
                    let view_push = Push { level: view, ..draw_push };
                    indirect.draw_two_sided_shadow(ctx, LIST_LOCAL_SHADOW + slot, &view_push);
                }
            }
            if let Some(blocks) = &blocks {
                bind(ctx, pipelines.block_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                for (slot, view) in all().filter(|&(slot, _)| blocks.draws_for_lamp(slot, false)) {
                    set_viewport(ctx, tile_at(view), tile);
                    crate::blocks::draw_lamp_runs(ctx, blocks, &push, slot, false, view);
                }
            }
            // The ground, each frame for a lamp that keeps nothing.
            if let Some(terrains) = &terrains {
                bind(ctx, pipelines.terrain_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                let each_frame =
                    |slot: u32| !lamps.is_cached(slot as usize) && crate::terrain::draws_for_lamp(terrains, slot);
                for (slot, view) in all().filter(|&(slot, _)| each_frame(slot)) {
                    set_viewport(ctx, tile_at(view), tile);
                    crate::terrain::draw_lamp(ctx, terrains, &push, slot, view);
                }
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    if let Some((maps, panes)) = targets.glass_shadows.clone() {
        let lights = targets.local_shadows.as_ref().map(|local| local.views.clone()).unwrap_or_default();
        let sources = (targets.shadows.is_some() || targets.virtual_shadows.is_some(), lights);
        glass::record_shadows(fg, (pipelines, push), maps, panes, sources, draws.index_buffer);
    }
    let local_atlas = targets.local_shadows.as_ref().map(|local| local.atlas);
    let pool = targets.virtual_shadows.as_ref().map(|vsm| vsm.pool);
    let moving_pool = targets.virtual_shadows.as_ref().and_then(|vsm| vsm.moving.as_ref().map(|(image, _)| *image));
    let kept_atlas = targets.local_shadows.as_ref().and_then(|local| local.kept);
    let lamp_pool = targets.local_shadows.as_ref().and_then(|local| local.pages.as_ref().map(|pages| pages.pool));
    let glass_maps = targets.glass_shadows.as_ref().map(|(maps, _)| maps.images());
    if let Some((image, view)) = targets.leaf_light.clone() {
        crate::shader_packs::record_leaf_light(fg, (pipelines, push), image, view);
    }
    let leaf_light = targets.leaf_light.as_ref().map(|(image, _)| *image);
    // What lights pass lamps' shadows and glass through, beside the sun's shadows.
    let shadow_maps: Vec<ImageId> = local_atlas
        .into_iter()
        .chain(kept_atlas)
        .chain(lamp_pool)
        .chain(pool)
        .chain(moving_pool)
        .chain(glass_maps.into_iter().flatten())
        .chain(leaf_light)
        .collect();

    let ambient_occlusion = frame.ambient_occlusion;
    let contact_shadows = frame.contact_shadows;
    let reflections = frame.reflections;
    // Drawn as depth first, then shaded over it, for what reads the depth before shading, and whenever each pixel's
    // normal is wanted, as a shader pack's passes may want it.
    let through = frame.through;
    let depth_first = ambient_occlusion.is_some()
        || contact_shadows.is_some()
        || reflections.is_some()
        || through
        || frame.surface.is_some();
    if let Some(atmosphere) = frame.atmosphere.filter(|_| frame.own_sky) {
        atmosphere::record(fg, (pipelines, push), atmosphere);
    }
    // Where the Dry Areas shaped as meshes are, which the opaque passes read to light what is in them as dry; and the
    // sea's waves and the sunlight they focus, which light what is under the water.
    let dry_mask = frame.water.as_ref().and_then(crate::water::Water::dry_mask);
    let caustics = frame.water.as_ref().map(crate::water::Water::caustics);
    if let Some(water) = &frame.water {
        crate::water::record_dry_mask(fg, (pipelines, push), water, draws.index_buffer);
        // A view through a Portal draws with the waves the camera's own view worked out.
        if !water.shares_waves() {
            crate::water::record_waves(fg, (pipelines, push), water);
        }
        // What has moved across the water lately: its trails' ripples run on.
        if let Some(trails) = water.trails.filter(|trails| trails.gpu.dispatches() > 0) {
            crate::water_trails::record(fg, (pipelines, push), (trails.images, trails.parity), &trails.gpu);
        }
    }
    let mut tables: Vec<ImageId> = frame.atmosphere.map(|atmosphere| atmosphere.all().to_vec()).unwrap_or_default();
    if let Some(clouds) = frame.clouds {
        crate::clouds::record(fg, (pipelines, push), clouds, &tables);
        // Read with the tables from here on: wherever the sun's light is read, and by the sky.
        tables.extend(clouds.sampled());
    }
    let shading = |color: ImageAccess| {
        let mut images =
            vec![(targets.hdr, color), (targets.velocity, color), (targets.depth, ImageAccess::DepthAttachment)];
        images.extend(targets.shadows.map(|atlas| (atlas, ImageAccess::Sampled)));
        images.extend(shadow_maps.iter().map(|&map| (map, ImageAccess::Sampled)));
        images.extend(ambient_occlusion.map(|ao| (ao.current, ImageAccess::Sampled)));
        images.extend(contact_shadows.map(|contact| (contact, ImageAccess::Sampled)));
        images.extend(reflections.map(|(target, _)| (target, ImageAccess::Sampled)));
        // The GI's probes, which shading and reflections read.
        images.extend(frame.probes.into_iter().flat_map(|probes| {
            [probes.irradiance, probes.depth, probes.states].map(|image| (image, ImageAccess::Sampled))
        }));
        images.extend(tables.iter().map(|&table| (table, ImageAccess::Sampled)));
        images.extend(dry_mask.map(|mask| (mask, ImageAccess::Sampled)));
        images.extend(caustics.map(|image| (image, ImageAccess::Sampled)));
        images
    };
    let shading_push = Push { size: [targets.cascade_size; 2], ..draw_push };
    let forward = match (mesh, frame.wireframe) {
        (Some(mesh), true) => mesh.wireframe,
        (Some(mesh), false) => mesh.forward,
        (None, true) => pipelines.wireframe,
        (None, false) => pipelines.forward,
    };
    // The wireframe culls no backs already.
    let forward = Drawing {
        pipeline: forward,
        two_sided: if frame.wireframe { pipelines.wireframe } else { pipelines.forward_two_sided },
    };
    let (hdr, velocity, depth) = (targets.hdr, targets.velocity, targets.depth);
    let scene = SceneTargets { hdr, velocity, depth };
    // The Portals' openings, drawn into the depth as any surface: nothing of the view's own world behind one is
    // shaded after it, and the view through it finds its pixels by it (portal.slang).
    let openings = frame.portal_openings;
    let portal_surfaces = |fg: &mut FrameGraph<'_>| {
        if openings == 0 {
            return;
        }
        fg.add_pass("Portal surfaces", &[(depth, ImageAccess::DepthAttachment)], &[], move |ctx| {
            let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
            begin_rendering(ctx, ctx.extent(depth), &[], Some(&target));
            bind(ctx, pipelines.portal_surface, vk::PipelineBindPoint::GRAPHICS, &push);
            for opening in 0..openings {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { level: opening, ..push });
                unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 6, 1, 0, 0) };
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    };
    // Only read where the depth passes run, which have a surface image whenever they do.
    let depth_targets = DepthTargets {
        depth,
        surface: frame.surface.unwrap_or(depth),
        ids: frame.surface_ids.filter(|_| pipelines.surface_ids),
    };
    let depth_pass = |fg: &mut FrameGraph<'_>, name: &str, list: u32| {
        let pipeline =
            Drawing { pipeline: mesh.map_or(pipelines.depth, |mesh| mesh.depth), two_sided: pipelines.depth_two_sided };
        let push = Push { list, ..draw_push };
        fg.add_pass(name, &depth_targets.images(), &shading_buffers, move |ctx| {
            // A view through a Portal draws into the depth opened for it.
            depth_only(ctx, pipeline, &push, indirect, depth_targets, list == LIST_EARLY && !through);
        });
    };

    fg.add_pass("Cull (early)", &[], &culling, move |ctx| {
        cull(ctx, pipelines.cull, &push, LIST_EARLY, instances, 1);
    });
    meshlet_pass(fg, "Meshlets (early)", &[], [LIST_EARLY, LIST_LATE]);
    // The displaced surfaces the cull listed, split into pieces, each piece in view sent out as a draw.
    let dice = Dice { counts: draws.dice_counts_handle, draws: draws.dice_draws_handle };
    let split = [
        (draws.dice_counts, BufferAccess::IndirectRead),
        (draws.dice_counts, BufferAccess::ComputeWrite),
        (draws.diced_instances, BufferAccess::ComputeRead),
        (draws.dice_splits, BufferAccess::ComputeWrite),
    ];
    fg.add_pass("Dice (split)", &[], &split, move |ctx| {
        bind(ctx, pipelines.dice_split, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch_indirect(ctx.cmd, dice.counts, crate::dice::SPLIT_DISPATCH) };
    });
    let pieces = [
        (draws.dice_counts, BufferAccess::IndirectRead),
        (draws.dice_counts, BufferAccess::ComputeWrite),
        (draws.dice_splits, BufferAccess::ComputeRead),
        (draws.dice_patches, BufferAccess::ComputeWrite),
        (draws.dice_draws, BufferAccess::ComputeWrite),
        (draws.stats, BufferAccess::ComputeWrite),
    ];
    fg.add_pass("Dice (pieces)", &[], &pieces, move |ctx| {
        bind(ctx, pipelines.dice_patch, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch_indirect(ctx.cmd, dice.counts, crate::dice::PATCH_DISPATCH) };
    });
    // The pieces cast the sun's shadows of the surfaces they were diced from, into each cascade.
    if let Some(atlas) = targets.shadows {
        let reads = [
            (draws.dice_counts, BufferAccess::IndirectRead),
            (draws.dice_draws, BufferAccess::IndirectRead),
            (draws.dice_patches, BufferAccess::ShaderRead),
            (draws.dice_splits, BufferAccess::ShaderRead),
        ];
        let cascade_size = targets.cascade_size;
        fg.add_pass("Shadows (diced)", &[(atlas, ImageAccess::DepthAttachment)], &reads, move |ctx| {
            let size = cascade_size * 2;
            let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
            begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
            bind(ctx, pipelines.shadow_diced, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for cascade in 0..CASCADES as u32 {
                let offset = vk::Offset2D {
                    x: (cascade % 2 * cascade_size) as i32,
                    y: (cascade / 2 * cascade_size) as i32,
                };
                set_viewport(ctx, offset, vk::Extent2D { width: cascade_size, height: cascade_size });
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: LIST_SHADOW + cascade, ..draw_push });
                dice.draw(ctx);
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    if let (Some(atlas), Some(terrains)) = (targets.shadows, &frame.terrain) {
        crate::terrain::record_shadows(fg, (pipelines, push), terrains, atlas);
    }
    if let (Some(atlas), Some(blocks)) = (targets.shadows, &frame.blocks) {
        crate::blocks::record_shadows(fg, (pipelines, push), blocks, (atlas, targets.cascade_size));
    }
    // The grass round the camera, scattered afresh before anything draws it.
    if let Some(grass) = &frame.grass {
        crate::grass::record_scatter(fg, (pipelines, push), grass);
    }
    // What the grass's vertices read beside their own: how plants are pushed aside.
    let pushes = [(draws.foliage_pushes, BufferAccess::ShaderRead)];
    let diced_buffers: Vec<_> = shading_buffers
        .iter()
        .copied()
        .chain([
            (draws.dice_counts, BufferAccess::IndirectRead),
            (draws.dice_draws, BufferAccess::IndirectRead),
            (draws.dice_patches, BufferAccess::ShaderRead),
            (draws.dice_splits, BufferAccess::ShaderRead),
        ])
        .collect();
    match depth_first {
        true => {
            depth_pass(fg, "Depth (early)", LIST_EARLY);
            fg.add_pass("Depth (diced)", &depth_targets.images(), &diced_buffers, move |ctx| {
                diced_depth(ctx, pipelines.depth_diced, &draw_push, dice, depth_targets);
            });
            if let Some(terrains) = &frame.terrain {
                crate::terrain::record_depth(fg, (pipelines, push), terrains, depth_targets);
            }
            if let Some(blocks) = &frame.blocks {
                crate::blocks::record_depth(fg, (pipelines, push), blocks, depth_targets);
            }
            if let Some(grass) = &frame.grass {
                crate::grass::record_depth(fg, (pipelines, push), grass, depth_targets, &pushes);
            }
        }
        false => {
            fg.add_pass("Opaque (early)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
                let clear = Clear { color: !through, depth: !through };
                opaque(ctx, forward, &Push { list: LIST_EARLY, ..shading_push }, indirect, scene, clear);
            });
            fg.add_pass("Opaque (diced)", &shading(ImageAccess::ColorAttachment), &diced_buffers, move |ctx| {
                diced_opaque(ctx, pipelines.forward_diced, &shading_push, dice, scene);
            });
            if let Some(terrains) = &frame.terrain {
                let pipeline = if frame.wireframe { pipelines.terrain_wireframe } else { pipelines.terrain_forward };
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                crate::terrain::record_opaque(fg, shading_push, pipeline, terrains, images, targets, &shading_buffers);
            }
            if let Some(blocks) = &frame.blocks {
                let pipeline = if frame.wireframe { pipelines.block_wireframe } else { pipelines.block_forward };
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                crate::blocks::record_opaque(fg, shading_push, pipeline, blocks, images, targets, &shading_buffers);
            }
            if let Some(grass) = &frame.grass {
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                let pipeline = pipelines.grass_forward;
                crate::grass::record_opaque(fg, shading_push, pipeline, grass, images, targets, &shading_buffers);
            }
        }
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

    // The pages of the sun's virtual shadow maps the view asks for, from the pyramid's third level, as far as the
    // early pass drew, read when this frame in flight comes round. The pyramid, not the depth, so the depth's own
    // layout is left as it is.
    if let Some(vsm) = &targets.virtual_shadows {
        let ((requests, requests_handle), (readback, readback_handle)) = (vsm.requests, vsm.readback);
        const MIP: u32 = 2;
        let size = [(pyramid_size[0] >> MIP).max(1), (pyramid_size[1] >> MIP).max(1)];
        let mark = Push { texture: pyramid_texture.0, size, level: MIP, ..push };
        let images = [(pyramid, ImageAccess::Sampled)];
        fg.add_pass("Virtual shadow requests", &images, &[(requests, BufferAccess::ComputeWrite)], move |ctx| {
            bind(ctx, pipelines.vsm_mark, vk::PipelineBindPoint::COMPUTE, &mark);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
        });
        let words = (crate::vsm::REQUEST_WORDS * 4) as u64;
        let copy = [(requests, BufferAccess::TransferRead), (readback, BufferAccess::TransferWrite)];
        fg.add_pass("Virtual shadow requests readback", &[], &copy, move |ctx| unsafe {
            let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: words };
            ctx.gpu.device.cmd_copy_buffer(ctx.cmd, requests_handle, readback_handle, &[region]);
        });
        fg.sync_buffer(readback, BufferAccess::HostRead);
    }
    // The same for the lamps' pages.
    if let Some(pages) = targets.local_shadows.as_ref().and_then(|local| local.pages.as_ref()) {
        let ((requests, requests_handle), (readback, readback_handle)) = (pages.requests, pages.readback);
        const MIP: u32 = 2;
        let size = [(pyramid_size[0] >> MIP).max(1), (pyramid_size[1] >> MIP).max(1)];
        let mark = Push { texture: pyramid_texture.0, size, level: MIP, ..push };
        let images = [(pyramid, ImageAccess::Sampled)];
        fg.add_pass("Lamp shadow requests", &images, &[(requests, BufferAccess::ComputeWrite)], move |ctx| {
            bind(ctx, pipelines.lamp_mark, vk::PipelineBindPoint::COMPUTE, &mark);
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
        });
        let words = (crate::lamp_vsm::REQUEST_WORDS * 4) as u64;
        let copy = [(requests, BufferAccess::TransferRead), (readback, BufferAccess::TransferWrite)];
        fg.add_pass("Lamp shadow requests readback", &[], &copy, move |ctx| unsafe {
            let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: words };
            ctx.gpu.device.cmd_copy_buffer(ctx.cmd, requests_handle, readback_handle, &[region]);
        });
        fg.sync_buffer(readback, BufferAccess::HostRead);
    }

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
                let surface = frame.surface.expect("the depth passes' surface");
                ambient_occlusion::record(fg, (pipelines, push), (depth, surface), ao);
            }
            if let Some(contact) = contact_shadows {
                ambient_occlusion::record_contact_shadows(fg, (pipelines, push), depth, contact);
            }
            if let Some((target, history)) = reflections {
                let surface = frame.surface.expect("the depth passes' surface");
                let images = (depth, surface);
                let tracing = (frame.traced_reflections, frame.reflection_pyramid);
                let sampled = (shadow_maps.as_slice(), tables.as_slice());
                ambient_occlusion::record_reflections(
                    fg,
                    (pipelines, push),
                    images,
                    history,
                    target,
                    tracing,
                    frame.probes,
                    sampled,
                );
            }
            // After what reads the depth for the view's own world, and before any of it is shaded.
            portal_surfaces(fg);
            // Both lists again, over their own depth. Task shaders cull them again, without counting them again.
            let forward = Drawing {
                pipeline: mesh.map_or(pipelines.forward_on_depth, |mesh| mesh.forward_on_depth),
                two_sided: pipelines.forward_on_depth_two_sided,
            };
            let again = if mesh.is_some() { Push { source: DRAWS_AGAIN, ..shading_push } } else { shading_push };
            for (name, list) in [("Opaque (early)", LIST_EARLY), ("Opaque (late)", LIST_LATE)] {
                // A view through a Portal draws over what the view it is seen in drew.
                let clear = Clear { color: list == LIST_EARLY && !through, depth: false };
                fg.add_pass(name, &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
                    opaque(ctx, forward, &Push { list, ..again }, indirect, scene, clear);
                });
            }
            fg.add_pass("Opaque (diced)", &shading(ImageAccess::ColorAttachment), &diced_buffers, move |ctx| {
                diced_opaque(ctx, pipelines.forward_on_depth_diced, &shading_push, dice, scene);
            });
            if let Some(terrains) = &frame.terrain {
                let pipeline = pipelines.terrain_forward_on_depth;
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                crate::terrain::record_opaque(fg, shading_push, pipeline, terrains, images, targets, &shading_buffers);
            }
            if let Some(blocks) = &frame.blocks {
                let pipeline = pipelines.block_forward_on_depth;
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                crate::blocks::record_opaque(fg, shading_push, pipeline, blocks, images, targets, &shading_buffers);
            }
            if let Some(grass) = &frame.grass {
                let pipeline = pipelines.grass_forward_on_depth;
                let images = shading(ImageAccess::ColorAttachment);
                let targets = (hdr, velocity, depth);
                crate::grass::record_opaque(fg, shading_push, pipeline, grass, images, targets, &shading_buffers);
            }
        }
        false => fg.add_pass("Opaque (late)", &shading(ImageAccess::ColorAttachment), &shading_buffers, move |ctx| {
            opaque(ctx, forward, &Push { list: LIST_LATE, ..shading_push }, indirect, scene, Clear::default());
        }),
    }
    if !depth_first {
        portal_surfaces(fg);
    }
    if let Some(probes) = frame.probes {
        ddgi::record(fg, (pipelines, push), probes, &shadow_maps);
    }
    if let Some((probe_frame, job, probe_pipelines)) = &frame.probe_job {
        // What the probe's rays meet is lit as the GI's rays' hits are: through the lights' shadows, and by the GI.
        let gi = frame.probes.iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]);
        let sampled: Vec<ImageId> = shadow_maps.iter().copied().chain(gi).collect();
        crate::reflection_probes::record(fg, *probe_pipelines, push, probe_frame, *job, &sampled);
    }
    if let Some((pass, sound_pipelines)) = frame.sound_job {
        crate::sound_rays::record(fg, sound_pipelines, push, pass);
    }
    if let Some((bake, bake_pipelines)) = &frame.bake {
        let tables: Vec<ImageId> = frame.atmosphere.map(|tables| tables.all().to_vec()).unwrap_or_default();
        crate::lightmapper::record(fg, *bake_pipelines, push, bake, &tables);
    }
    if let Some(volume) = frame.volume {
        let shadows: Vec<ImageId> = targets.shadows.into_iter().chain(shadow_maps.iter().copied()).collect();
        volumetric::record(fg, (pipelines, push), volume, &shadows, &[draws.clusters]);
    }
    // Counted in GPU memory, where the culling passes' atomics are fast, then copied out at once.
    let (readback, stats_bytes) = (draws.stats_readback_handle, stat::COUNT as u64 * 4);
    let copy = [(draws.stats, BufferAccess::TransferRead), (draws.stats_readback, BufferAccess::TransferWrite)];
    fg.add_pass("Read back stats", &[], &copy, move |ctx| unsafe {
        let region = vk::BufferCopy { src_offset: 0, dst_offset: 0, size: stats_bytes };
        ctx.gpu.device.cmd_copy_buffer(ctx.cmd, stats_handle, readback, &[region]);
    });
    fg.sync_buffer(draws.stats_readback, BufferAccess::HostRead);
    far_sides(fg);

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
    // What a shader pack draws once the opaque surfaces and the sky are lit, before water and glass.
    lit(fg);

    let mut under = None;
    // The images the sea's shading reads, which its spray reads too.
    let mut sea_images = Vec::new();
    if let Some(water) = frame.water.clone() {
        let mut sampled: Vec<ImageId> =
            targets.shadows.into_iter().chain(shadow_maps.iter().copied()).chain(tables.iter().copied()).collect();
        sampled.extend(frame.probes.into_iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]));
        sampled.extend(dry_mask);
        let index_buffer = draws.index_buffer;
        let water_targets = crate::water::WaterTargets { hdr, velocity, depth, index_buffer, sampled };
        (under, sea_images) = crate::water::record(fg, (pipelines, push), water, water_targets);
    }

    if let Some(glass) = frame.glass.clone() {
        let mut sampled: Vec<ImageId> =
            targets.shadows.into_iter().chain(shadow_maps.iter().copied()).chain(tables.iter().copied()).collect();
        sampled.extend(frame.probes.into_iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]));
        let index_buffer = draws.index_buffer;
        let glass_targets =
            glass::GlassTargets { hdr, depth, sampled, clusters: draws.clusters, pushes: draws.foliage_pushes, index_buffer };
        glass::record(fg, (pipelines, push), glass, glass_targets);
    }

    // The smokes and the particles are drawn after the fog, each fogged as far off as it is (post_passes).
    let smoke = (frame.smokes > 0 || frame.mists > 0).then(|| {
        let mut sampled: Vec<ImageId> =
            targets.shadows.into_iter().chain(shadow_maps.iter().copied()).chain(tables.iter().copied()).collect();
        sampled.extend(frame.probes.into_iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]));
        let (count, mists, noise) = (frame.smokes, frame.mists, frame.smoke_noise);
        crate::smoke::Draw { count, mists, noise, depth, sampled, clusters: draws.clusters }
    });
    let mut particles = None;
    if !frame.particles.is_empty() || frame.spray.is_some() || !frame.rapids.is_empty() || frame.particle_step.is_some()
    {
        let mut sampled: Vec<ImageId> =
            targets.shadows.into_iter().chain(shadow_maps.iter().copied()).chain(tables.iter().copied()).collect();
        sampled.extend(frame.probes.into_iter().flat_map(|probes| [probes.irradiance, probes.depth, probes.states]));
        // The spray only with the sea drawn this frame, whose waves and foam it reads.
        let spray = frame.spray.filter(|_| !sea_images.is_empty());
        sampled.extend(sea_images);
        let clusters = draws.clusters;
        let pool = frame.particle_pool;
        let index_buffer = draws.index_buffer;
        let particle_targets =
            crate::particles::Targets { hdr, velocity, depth, sampled, clusters, pool, index_buffer, extent: frame.extent };
        let (counts, rapids) = (frame.particles.clone(), frame.rapids.clone());
        let step = frame.particle_step;
        particles = Some(crate::particles::Draw { step, counts, spray, rapids, targets: particle_targets });
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
            for list in [LIST_EARLY, LIST_LATE] {
                indirect.draw_two_sided(ctx, list, pipelines.edges, &edges_push);
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    (under, particles, smoke)
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
    meshlets: Option<(vk::Buffer, crate::gpu::MeshletCapacity)>,
    /// Draw the list's queued meshlets with task and mesh shaders, a task
    /// workgroup for each round of them, the count the cull grew.
    tasks: bool,
    /// Draw the shadows' two-sided lists, which are empty without it.
    two_sided_shadows: bool,
}

/// A pass's pipeline, and the one that draws its list's two-sided draws.
#[derive(Clone, Copy)]
struct Drawing {
    pipeline: vk::Pipeline,
    two_sided: vk::Pipeline,
}

impl Indirect {
    /// The two-sided draws of the early or late list, with `pipeline`, which
    /// culls no backs: always whole LODs, from the instance lists.
    fn draw_two_sided(self, ctx: &PassContext, list: u32, pipeline: vk::Pipeline, push: &Push) {
        let list = LIST_TWO_SIDED + list - LIST_EARLY;
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &Push { list, source: DRAWS_INSTANCES, ..*push });
        Self { meshlets: None, tasks: false, ..self }.draw(ctx, list);
    }

    /// The two-sided draws beside sun or lamp shadow list `list`, into the
    /// view `push` says, with a pipeline bound that culls no backs.
    fn draw_two_sided_shadow(self, ctx: &PassContext, list: u32, push: &Push) {
        let list = two_sided_shadow(list);
        ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, ..*push });
        self.draw(ctx, list);
    }

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

/// **Virtual shadow pages** (see `vsm`): the pages drawn this frame, four
/// levels at a time, the sun's lists culled for each level's pages and
/// drawn into each page's slot of the pool, cleared first; then the
/// terrains' patches and the worlds of blocks' faces each level takes. The
/// lists are emptied again between rounds. The view's requests are cleared
/// first, for the frame to ask afresh.
fn record_virtual_shadows(
    fg: &mut FrameGraph<'_>,
    frame: &Frame,
    vsm: &VirtualShadows,
    (culling, meshlet_culling, drawing): (
        &[(BufferId, BufferAccess)],
        &[(BufferId, BufferAccess)],
        &[(BufferId, BufferAccess)],
    ),
    draws: &Draws,
    indirect: Indirect,
) {
    let (push, pipelines, instances) = (frame.push, frame.pipelines, frame.instance_count);
    let two_sided = indirect.two_sided_shadows;
    let draw_push = Push { source: if indirect.meshlets.is_some() { DRAWS_MESHLETS } else { DRAWS_INSTANCES }, ..push };
    copy_table(fg, "Virtual shadow table", vsm.table);
    let (requests, requests_handle) = vsm.requests;
    let words = (crate::vsm::REQUEST_WORDS * 4) as u64;
    fg.add_pass("Virtual shadow requests clear", &[], &[(requests, BufferAccess::TransferWrite)], move |ctx| unsafe {
        ctx.gpu.device.cmd_fill_buffer(ctx.cmd, requests_handle, 0, words, 0);
    });
    let (blocks, terrains) = (frame.blocks.clone(), frame.terrain.clone());
    let mut page_drawing = drawing.to_vec();
    let pools = blocks.iter().flat_map(|blocks| blocks.pools.iter());
    let grounds = terrains.iter().flat_map(|terrains| terrains.heights.iter());
    page_drawing.extend(pools.chain(grounds).map(|&id| (id, BufferAccess::ShaderRead)));
    let count_handle = draws.count_handle;
    let mesh = if indirect.tasks { pipelines.mesh } else { None };
    let shadow = mesh.map_or(pipelines.sun_pages, |mesh| mesh.sun_pages);
    let pool = vsm.pool;
    for (round, batch) in vsm.levels.chunks(CASCADES).enumerate() {
        let batch = batch.to_vec();
        let count = batch.len() as u32;
        if round > 0 {
            fg.add_pass("Virtual shadow lists reset", &[], &[(draws.count, BufferAccess::TransferWrite)], move |ctx| {
                reset_lists(ctx, count_handle, LIST_SHADOW, CASCADES as u32);
            });
        }
        let levels = batch.iter().enumerate().fold(0u32, |bits, (at, (level, _))| bits | (*level as u32) << (8 * at));
        let cull_push = Push { level: VSM_CULL, output: levels, ..push };
        fg.add_pass("Virtual shadow cull", &[], culling, move |ctx| {
            cull(ctx, pipelines.cull, &cull_push, LIST_SHADOW, instances, count);
        });
        if indirect.meshlets.is_some() && !indirect.tasks {
            fg.add_pass("Virtual shadow meshlets", &[], meshlet_culling, move |ctx| {
                let lists = LIST_SHADOW..LIST_SHADOW + count;
                cull_shadow_meshlets(ctx, pipelines.meshlet_cull, &cull_push, count_handle, lists, two_sided);
            });
        }
        let (blocks, terrains) = (blocks.clone(), terrains.clone());
        fg.add_pass("Virtual shadow pages", &[(pool, ImageAccess::DepthAttachment)], &page_drawing, move |ctx| {
            let side = crate::vsm::POOL;
            let depth = attachment(ctx.view(pool), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
            begin_rendering(ctx, vk::Extent2D { width: side, height: side }, &[], Some(&depth));
            let slot_at = |slot: u16| {
                let (column, row) =
                    (u32::from(slot) % crate::vsm::POOL_SLOTS, u32::from(slot) / crate::vsm::POOL_SLOTS);
                vk::Offset2D { x: (column * crate::vsm::SLOT) as i32, y: (row * crate::vsm::SLOT) as i32 }
            };
            let extent = vk::Extent2D { width: crate::vsm::SLOT, height: crate::vsm::SLOT };
            let pages: Vec<(u32, usize, u32, u16)> = batch
                .iter()
                .enumerate()
                .flat_map(|(at, (level, pages))| pages.iter().map(move |&(page, slot)| (at as u32, *level, page, slot)))
                .collect();
            let rects: Vec<vk::ClearRect> = pages
                .iter()
                .map(|&(.., slot)| vk::ClearRect {
                    rect: vk::Rect2D { offset: slot_at(slot), extent },
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .collect();
            let clear =
                vk::ClearAttachment::default().aspect_mask(vk::ImageAspectFlags::DEPTH).clear_value(clear_depth(1.0));
            unsafe { ctx.gpu.device.cmd_clear_attachments(ctx.cmd, &[clear], &rects) };
            bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for &(at, _, page, slot) in &pages {
                set_viewport(ctx, slot_at(slot), extent);
                let list = LIST_SHADOW + at;
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: VSM_PAGE_VIEW + page, ..draw_push });
                indirect.draw(ctx, list);
            }
            if indirect.two_sided_shadows {
                let two_sided = mesh.map_or(pipelines.shadow_two_sided, |mesh| mesh.shadow_two_sided);
                bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                for &(at, _, page, slot) in &pages {
                    set_viewport(ctx, slot_at(slot), extent);
                    let page_push = Push { level: VSM_PAGE_VIEW + page, ..draw_push };
                    indirect.draw_two_sided_shadow(ctx, LIST_SHADOW + at, &page_push);
                }
            }
            if let Some(terrains) = &terrains {
                bind(ctx, pipelines.terrain_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                for &(_, level, page, slot) in &pages {
                    set_viewport(ctx, slot_at(slot), extent);
                    crate::terrain::draw_sun_page(ctx, terrains, &push, level, VSM_PAGE_VIEW + page);
                }
            }
            if let Some(blocks) = &blocks {
                bind(ctx, pipelines.block_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
                for &(_, level, page, slot) in &pages {
                    set_viewport(ctx, slot_at(slot), extent);
                    crate::blocks::draw_sun_page(ctx, blocks, &push, level, VSM_PAGE_VIEW + page);
                }
            }
            unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
        });
    }
    let Some((moving_pool, moving)) = vsm.moving.clone() else { return };
    // Each page what moves stands in copied, still part and border, into its slot of the moving pool.
    let slot_corner = |slot: u16, slots: u32| {
        let (column, row) = (u32::from(slot) % slots, u32::from(slot) / slots);
        vk::Offset3D { x: (column * crate::vsm::SLOT) as i32, y: (row * crate::vsm::SLOT) as i32, z: 0 }
    };
    let regions: Vec<vk::ImageCopy> = moving
        .iter()
        .flat_map(|(_, pages)| pages.iter())
        .map(|&(_, slot, moving_slot)| {
            let layers = vk::ImageSubresourceLayers::default().aspect_mask(vk::ImageAspectFlags::DEPTH).layer_count(1);
            vk::ImageCopy::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offset(slot_corner(slot, crate::vsm::POOL_SLOTS))
                .dst_offset(slot_corner(moving_slot, crate::vsm::MOVING_POOL_SLOTS))
                .extent(vk::Extent3D { width: crate::vsm::SLOT, height: crate::vsm::SLOT, depth: 1 })
        })
        .collect();
    let copying = [(pool, ImageAccess::TransferSrc), (moving_pool, ImageAccess::TransferDst)];
    fg.add_pass("Virtual shadow moving copies", &copying, &[], move |ctx| unsafe {
        let (from, to) = (ctx.image(pool), ctx.image(moving_pool));
        let (src, dst) = (vk::ImageLayout::TRANSFER_SRC_OPTIMAL, vk::ImageLayout::TRANSFER_DST_OPTIMAL);
        ctx.gpu.device.cmd_copy_image(ctx.cmd, from, src, to, dst, &regions);
    });
    // What moves over each copy, four levels at a time, as the pages were drawn.
    for (round, batch) in moving.chunks(CASCADES).enumerate() {
        let batch = batch.to_vec();
        let count = batch.len() as u32;
        if round > 0 || !vsm.levels.is_empty() {
            fg.add_pass("Virtual shadow lists reset", &[], &[(draws.count, BufferAccess::TransferWrite)], move |ctx| {
                reset_lists(ctx, count_handle, LIST_SHADOW, CASCADES as u32);
            });
        }
        let levels = batch.iter().enumerate().fold(0u32, |bits, (at, (level, _))| bits | (*level as u32) << (8 * at));
        let cull_push = Push { level: VSM_CULL_MOVING, output: levels, ..push };
        fg.add_pass("Virtual shadow moving cull", &[], culling, move |ctx| {
            cull(ctx, pipelines.cull, &cull_push, LIST_SHADOW, instances, count);
        });
        if indirect.meshlets.is_some() && !indirect.tasks {
            fg.add_pass("Virtual shadow moving meshlets", &[], meshlet_culling, move |ctx| {
                let lists = LIST_SHADOW..LIST_SHADOW + count;
                cull_shadow_meshlets(ctx, pipelines.meshlet_cull, &cull_push, count_handle, lists, two_sided);
            });
        }
        fg.add_pass(
            "Virtual shadow moving pages",
            &[(moving_pool, ImageAccess::DepthAttachment)],
            drawing,
            move |ctx| {
                let side = crate::vsm::MOVING_POOL;
                let depth = attachment(ctx.view(moving_pool), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
                begin_rendering(ctx, vk::Extent2D { width: side, height: side }, &[], Some(&depth));
                let extent = vk::Extent2D { width: crate::vsm::SLOT, height: crate::vsm::SLOT };
                let pages = || {
                    batch.iter().enumerate().flat_map(|(at, (_, pages))| {
                        pages.iter().map(move |&(page, _, moving_slot)| {
                            let corner = slot_corner(moving_slot, crate::vsm::MOVING_POOL_SLOTS);
                            (LIST_SHADOW + at as u32, page, vk::Offset2D { x: corner.x, y: corner.y })
                        })
                    })
                };
                bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                for (list, page, corner) in pages() {
                    set_viewport(ctx, corner, extent);
                    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: VSM_PAGE_VIEW + page, ..draw_push });
                    indirect.draw(ctx, list);
                }
                if indirect.two_sided_shadows {
                    let two_sided = mesh.map_or(pipelines.shadow_two_sided, |mesh| mesh.shadow_two_sided);
                    bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
                    for (list, page, corner) in pages() {
                        set_viewport(ctx, corner, extent);
                        indirect.draw_two_sided_shadow(ctx, list, &Push { level: VSM_PAGE_VIEW + page, ..draw_push });
                    }
                }
                unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
            },
        );
    }
}

/// **Lamp shadow pages** (see `lamp_vsm`): the frame's requests cleared,
/// for it to ask afresh; then, where pages are drawn, what stands still
/// round their lamps (`redraw`) culled into the lamps' lists and drawn into
/// each page's slot of the pool, cleared first, with the worlds of blocks'
/// faces and the terrains' patches each lamp keeps; and the lists emptied
/// again for what moves.
fn record_lamp_pages(
    fg: &mut FrameGraph<'_>,
    frame: &Frame,
    pages: &LampPages,
    (redraw, count): (u32, u32),
    (culling, meshlet_culling, drawing): (
        &[(BufferId, BufferAccess)],
        &[(BufferId, BufferAccess)],
        &[(BufferId, BufferAccess)],
    ),
    draws: &Draws,
    indirect: Indirect,
) {
    let (push, pipelines, instances) = (frame.push, frame.pipelines, frame.instance_count);
    let two_sided = indirect.two_sided_shadows;
    let draw_push = Push { source: if indirect.meshlets.is_some() { DRAWS_MESHLETS } else { DRAWS_INSTANCES }, ..push };
    copy_table(fg, "Lamp shadow table", pages.table);
    let (requests, requests_handle) = pages.requests;
    let words = (crate::lamp_vsm::REQUEST_WORDS * 4) as u64;
    fg.add_pass("Lamp shadow requests clear", &[], &[(requests, BufferAccess::TransferWrite)], move |ctx| unsafe {
        ctx.gpu.device.cmd_fill_buffer(ctx.cmd, requests_handle, 0, words, 0);
    });
    if pages.drawn.is_empty() {
        return;
    }
    let count_handle = draws.count_handle;
    let mesh = if indirect.tasks { pipelines.mesh } else { None };
    let shadow = mesh.map_or(pipelines.local_shadow, |mesh| mesh.local_shadow);
    let cull_push = Push { level: LOCAL_CULL_STILL, output: redraw, ..push };
    fg.add_pass("Lamp shadow cull", &[], culling, move |ctx| {
        cull(ctx, pipelines.cull, &cull_push, LIST_LOCAL_SHADOW, instances, count);
    });
    if indirect.meshlets.is_some() && !indirect.tasks {
        fg.add_pass("Lamp shadow meshlets", &[], meshlet_culling, move |ctx| {
            let lists = LIST_LOCAL_SHADOW..LIST_LOCAL_SHADOW + count;
            cull_shadow_meshlets(ctx, pipelines.meshlet_cull, &cull_push, count_handle, lists, two_sided);
        });
    }
    let (blocks, terrains) = (frame.blocks.clone(), frame.terrain.clone());
    let (pool, drawn) = (pages.pool, pages.drawn.clone());
    fg.add_pass("Lamp shadow pages", &[(pool, ImageAccess::DepthAttachment)], drawing, move |ctx| {
        let side = crate::lamp_vsm::POOL;
        let depth = attachment(ctx.view(pool), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, vk::Extent2D { width: side, height: side }, &[], Some(&depth));
        let slot_at = |slot: u16| {
            let (column, row) =
                (u32::from(slot) % crate::lamp_vsm::POOL_SLOTS, u32::from(slot) / crate::lamp_vsm::POOL_SLOTS);
            vk::Offset2D { x: (column * crate::vsm::SLOT) as i32, y: (row * crate::vsm::SLOT) as i32 }
        };
        let extent = vk::Extent2D { width: crate::vsm::SLOT, height: crate::vsm::SLOT };
        let rects: Vec<vk::ClearRect> = drawn
            .iter()
            .map(|&(_, slot)| vk::ClearRect {
                rect: vk::Rect2D { offset: slot_at(slot), extent },
                base_array_layer: 0,
                layer_count: 1,
            })
            .collect();
        let clear =
            vk::ClearAttachment::default().aspect_mask(vk::ImageAspectFlags::DEPTH).clear_value(clear_depth(1.0));
        unsafe { ctx.gpu.device.cmd_clear_attachments(ctx.cmd, &[clear], &rects) };
        bind(ctx, shadow, vk::PipelineBindPoint::GRAPHICS, &draw_push);
        for (page, &(lamp, slot)) in drawn.iter().enumerate() {
            set_viewport(ctx, slot_at(slot), extent);
            let list = LIST_LOCAL_SHADOW + lamp;
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: LAMP_PAGE_VIEW + page as u32, ..draw_push });
            indirect.draw(ctx, list);
        }
        if indirect.two_sided_shadows {
            let two_sided = mesh.map_or(pipelines.local_shadow_two_sided, |mesh| mesh.local_shadow_two_sided);
            bind(ctx, two_sided, vk::PipelineBindPoint::GRAPHICS, &draw_push);
            for (page, &(lamp, slot)) in drawn.iter().enumerate() {
                set_viewport(ctx, slot_at(slot), extent);
                let page_push = Push { level: LAMP_PAGE_VIEW + page as u32, ..draw_push };
                indirect.draw_two_sided_shadow(ctx, LIST_LOCAL_SHADOW + lamp, &page_push);
            }
        }
        if let Some(blocks) = &blocks {
            bind(ctx, pipelines.block_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
            for (page, &(lamp, slot)) in
                drawn.iter().enumerate().filter(|(_, (lamp, _))| blocks.draws_for_lamp(*lamp, true))
            {
                set_viewport(ctx, slot_at(slot), extent);
                crate::blocks::draw_lamp_runs(ctx, blocks, &push, lamp, true, LAMP_PAGE_VIEW + page as u32);
            }
        }
        if let Some(terrains) = &terrains {
            bind(ctx, pipelines.terrain_local_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
            for (page, &(lamp, slot)) in
                drawn.iter().enumerate().filter(|(_, (lamp, _))| crate::terrain::draws_for_lamp(terrains, *lamp))
            {
                set_viewport(ctx, slot_at(slot), extent);
                crate::terrain::draw_lamp(ctx, terrains, &push, lamp, LAMP_PAGE_VIEW + page as u32);
            }
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
    // The lamps' lists empty again, for what moves.
    fg.add_pass("Lamp shadow lists reset", &[], &[(draws.count, BufferAccess::TransferWrite)], move |ctx| {
        reset_lists(ctx, count_handle, LIST_LOCAL_SHADOW, count);
    });
}

/// Empties `count` shadow lists from `first` on, and the two-sided lists
/// beside them, as the frame starts them.
fn reset_lists(ctx: &PassContext, count_buffer: vk::Buffer, first: u32, count: u32) {
    let counts = cleared_draw_counts();
    for first in [first, two_sided_shadow(first)] {
        let (start, words) = (first as usize * 4, count as usize * 4);
        let lists = bytemuck::cast_slice(&counts[start..start + words]);
        unsafe {
            ctx.gpu.device.cmd_update_buffer(ctx.cmd, count_buffer, draw_count_offset(first), lists);
            ctx.gpu.device.cmd_fill_buffer(ctx.cmd, count_buffer, meshlet_count_offset(first), u64::from(count) * 4, 0);
        }
    }
}

fn cull(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, list: u32, instances: u32, lists: u32) {
    bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &Push { list, ..*push });
    unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, instances.div_ceil(64).max(1), lists, 1) };
}

/// A workgroup for each round of meshlets the cull queued in each of
/// `lists`, sized by the count the cull grew, so the CPU never reads it.
/// As `cull_meshlets`, for sun or lamp shadow lists, and with `two_sided`
/// for the two-sided lists beside them too.
fn cull_shadow_meshlets(
    ctx: &PassContext,
    pipeline: vk::Pipeline,
    push: &Push,
    count: vk::Buffer,
    lists: std::ops::Range<u32>,
    two_sided: bool,
) {
    if two_sided {
        let beside = two_sided_shadow(lists.start)..two_sided_shadow(lists.end);
        cull_meshlets(ctx, pipeline, push, count, lists.start..lists.end);
        cull_meshlets(ctx, pipeline, push, count, beside);
        return;
    }
    cull_meshlets(ctx, pipeline, push, count, lists);
}

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
    pipeline: Drawing,
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
    bind(ctx, pipeline.pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    indirect.draw(ctx, push.list);
    indirect.draw_two_sided(ctx, push.list, pipeline.two_sided, push);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// The pieces of the displaced surfaces dicing sent out: the counts, whose
/// pieces' count says how many draws there are, and the draws.
#[derive(Clone, Copy)]
struct Dice {
    counts: vk::Buffer,
    draws: vk::Buffer,
}

impl Dice {
    fn draw(self, ctx: &PassContext) {
        let (max, stride) = (crate::dice::MAX_PATCHES as u32, crate::dice::PATCH_SIZE as u32);
        let (draws, counts, at) = (self.draws, self.counts, crate::dice::PATCH_COUNT);
        unsafe { ctx.gpu.device.cmd_draw_indirect_count(ctx.cmd, draws, 0, counts, at, max, stride) };
    }
}

/// The diced pieces shaded, over what the opaque passes drew.
fn diced_opaque(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, dice: Dice, targets: SceneTargets) {
    let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
    let color = [attachment(ctx.view(targets.hdr), layout, None), attachment(ctx.view(targets.velocity), layout, None)];
    let depth = attachment(ctx.view(targets.depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
    begin_rendering(ctx, ctx.extent(targets.hdr), &color, Some(&depth));
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    dice.draw(ctx);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// The diced pieces as depth alone, over what the depth pass drew.
fn diced_depth(ctx: &PassContext, pipeline: vk::Pipeline, push: &Push, dice: Dice, targets: DepthTargets) {
    targets.begin(ctx);
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    dice.draw(ctx);
    unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
}

/// What the depth passes draw: the depth, each pixel's normal and roughness
/// beside it, and with a worn shader pack that reads them, what each pixel's
/// surface is (`SURFACE_ID_FORMAT`), which the depth pipelines then write.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DepthTargets {
    pub depth: ImageId,
    pub surface: ImageId,
    pub ids: Option<ImageId>,
}

impl DepthTargets {
    /// How a depth pass uses them, for the frame graph.
    pub fn images(&self) -> Vec<(ImageId, ImageAccess)> {
        let mut images = vec![(self.depth, ImageAccess::DepthAttachment), (self.surface, ImageAccess::ColorAttachment)];
        images.extend(self.ids.map(|ids| (ids, ImageAccess::ColorAttachment)));
        images
    }

    /// The depth attachment, cleared with `clear`, or drawn over.
    pub fn depth_attachment(&self, ctx: &PassContext, clear: bool) -> vk::RenderingAttachmentInfo<'static> {
        attachment(ctx.view(self.depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, clear.then(|| clear_depth(0.0)))
    }

    /// The colour attachments, cleared with `clear`: where nothing is drawn, the sky, as rough as can be, so
    /// reflections pass it over, and of no surface.
    pub fn colors(&self, ctx: &PassContext, clear: bool) -> Vec<vk::RenderingAttachmentInfo<'static>> {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let mut colors = vec![attachment(ctx.view(self.surface), layout, clear.then(|| clear_color([1.0; 4])))];
        colors.extend(self.ids.map(|ids| attachment(ctx.view(ids), layout, clear.then(|| clear_color([0.0; 4])))));
        colors
    }

    /// Begins drawing into them, over what is there.
    pub fn begin(&self, ctx: &PassContext) {
        let depth = self.depth_attachment(ctx, false);
        begin_rendering(ctx, ctx.extent(self.depth), &self.colors(ctx, false), Some(&depth));
    }
}

/// One opaque list as depth alone, the first clearing it.
fn depth_only(
    ctx: &PassContext,
    pipeline: Drawing,
    push: &Push,
    indirect: Indirect,
    targets: DepthTargets,
    clear: bool,
) {
    let target = targets.depth_attachment(ctx, clear);
    begin_rendering(ctx, ctx.extent(targets.depth), &targets.colors(ctx, clear), Some(&target));
    bind(ctx, pipeline.pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    indirect.draw(ctx, push.list);
    indirect.draw_two_sided(ctx, push.list, pipeline.two_sided, push);
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

pub(crate) fn clear_depth(depth: f32) -> vk::ClearValue {
    vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth, stencil: 0 } }
}

/// A rendering attachment that clears when given a value and loads otherwise.
pub(crate) fn attachment(
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

pub(crate) fn set_viewport(ctx: &PassContext, offset: vk::Offset2D, extent: vk::Extent2D) {
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
