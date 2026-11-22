//! Grass on terrain (see `voxear_terrain::grass`), scattered on the GPU
//! round each view's camera every frame (`shaders/grass.slang`).
//!
//! Each kind of each terrain's grass is a job: a grid of cells fixed to
//! the world, one blade at most a cell, over the square its draw distance
//! reaches round the camera. The CPU only says which jobs there are and
//! how much room each of their levels of detail has in the view's blades:
//! as many as cells a ring of its distance can hold, so it never runs out.
//! Then, on the GPU, **Grass reset** fills each draw's indirect command
//! from its mesh's LOD and counts nothing, **Grass scatter** writes the
//! blades that grow, counting them in their draws, and **Grass clamp**
//! keeps each count within its room. **Depth (grass)** and **Opaque
//! (grass)** draw them after the terrain, with no instance of the scene's
//! each, so millions are no more than the GPU can draw.
//!
//! **Ground cover** on worlds of blocks goes the same way (see
//! [`GroundCoverLook`]): each kind of plant a world's blocks grow on their
//! tops is a job of its own, and **Ground cover scatter** walks the faces
//! looking up of the chunks near enough, a workgroup to a share of a
//! chunk's, growing plants on each block of those whose surface carries it.
//! Plants grow wherever a face shows, under trees too, and go the frame a
//! block is laid over them: nothing of them is kept.

use std::sync::Arc;

use voxear_core::math::Vec3;
use voxear_rhi::{Buffer, vk};

use crate::{
    MaterialHandle, MeshHandle,
    gpu::Push,
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    passes::{attachment, begin_rendering, bind},
    pipelines::Pipelines,
    terrain::Drawn,
};

/// How one kind of a terrain's grass looks: its mesh and material, how many
/// to a square metre where it grows fully, its smallest and largest size,
/// and how far from the camera it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrassLook {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub density: f32,
    pub scale: [f32; 2],
    pub draw_distance: f32,
    /// How long it takes to stand up again where a Track Maker trod it
    /// flat, in seconds; 0 for it to stay flat.
    pub springs_back: f32,
}

/// One kind of plant a world of blocks grows on its blocks' tops (see
/// `voxear_terrain::blocks::block::GroundCover`): its mesh and material,
/// how many to a block where it grows, its smallest and largest size, how
/// far from the camera it is drawn, and the surfaces it grows on, a bit
/// each of a world's [`MAX_BLOCK_SURFACES`](crate::blocks::MAX_BLOCK_SURFACES):
/// the tops of the blocks that grow it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundCoverLook {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub density: f32,
    pub scale: [f32; 2],
    pub draw_distance: f32,
    pub surfaces: [u32; 8],
}

/// A job's `terrain` with this bit is a world of blocks' ground cover, the
/// world's place among the view's under it (see `ground_cover_jobs`).
pub const BLOCK_COVER: u32 = 1 << 31;

/// How long ground cover trodden flat takes to stand up again, in seconds.
const COVER_SPRINGS_BACK: f32 = 3.0;

/// How many workgroups share each chunk's faces looking up.
pub const COVER_SLICES: u32 = 8;

/// How many words a view keeps for its ground cover jobs' surfaces and
/// chunks: each job its eight words of surfaces, then two a chunk.
const COVER_WORDS: usize = 32 * 1024;

/// How many kinds of grass, over every terrain, a view scatters.
pub const MAX_GRASS_JOBS: usize = 64;

/// How many levels of detail a kind of grass is drawn with at most.
const MAX_LODS: usize = 4;

/// How many draws a view makes of its grass: a kind's LOD each.
const MAX_GRASS_DRAWS: usize = MAX_GRASS_JOBS * MAX_LODS;

/// How many blades a view keeps room for, every kind together.
pub const MAX_GRASS_BLADES: u64 = 1 << 20;

/// The most cells a job's grid may have each way, so a dense kind drawn
/// far does not dispatch more threads than it could ever fill.
const MAX_CELLS: u32 = 2048;

/// One kind of grass on one terrain, as `shaders/grass.slang` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGrassJob {
    pub terrain: u32,
    pub kind: u32,
    pub first_cell: [i32; 2],
    pub cells: [u32; 2],
    pub cell_size: f32,
    pub draw_distance: f32,
    pub scale: [f32; 2],
    pub mesh: u32,
    pub material: u32,
    pub lod_count: u32,
    /// Its first draw, the rest after it, one a LOD.
    pub draws: u32,
    pub springs_back: f32,
    /// How far each LOD reaches from the camera, in metres.
    pub lod_reach: [f32; 4],
    /// Where each LOD's blades start in the view's blades, and how many
    /// there is room for.
    pub offsets: [u32; 4],
    pub capacities: [u32; 4],
    /// Where its first cell's corner is in the world. Cells are counted from
    /// the level's middle, so each blade grows in the same place however the
    /// world's middle moves (see `voxear_scene::WorldOrigin`), and stands
    /// exactly where it should however far out.
    pub corner: [f32; 2],
}

/// One draw, as its vertex shader reads it: where its blades start, its job.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGrassDraw {
    pub offset: u32,
    pub job: u32,
}

const JOBS_SIZE: usize = MAX_GRASS_JOBS * size_of::<GpuGrassJob>();

/// Where a view's ground cover's words start in its grass buffer, after its
/// draws (`grassCoverWords` in `shaders/grass.slang`).
const COVER_AT: usize = JOBS_SIZE + MAX_GRASS_DRAWS * size_of::<GpuGrassDraw>();

/// How large a view's buffer of jobs, draws and ground cover words is.
pub const FRAME_SIZE: u64 = (COVER_AT + COVER_WORDS * 4) as u64;

/// How large a view's indirect commands for its grass are.
pub const ARGS_SIZE: u64 = (MAX_GRASS_DRAWS * DRAW_COMMAND) as u64;

/// How large one indirect indexed draw is: five words.
const DRAW_COMMAND: usize = 20;

/// Where a view's jobs and draws are in its grass buffer.
pub(crate) fn addresses(buffer: &Buffer) -> [u64; 2] {
    [buffer.address, buffer.address + JOBS_SIZE as u64]
}

/// What a view scatters and draws of the grass: each job's dispatch, how
/// many draws there are, and the buffers its passes wait on.
#[derive(Clone)]
pub(crate) struct ViewGrass {
    /// Each job's dispatch, and whether it is a world of blocks' ground
    /// cover, which the ground cover scatter makes.
    pub jobs: Vec<([u32; 2], bool)>,
    pub draws: u32,
    pub args: vk::Buffer,
    pub args_id: BufferId,
    pub blades_id: BufferId,
    /// The terrains' buffers and the worlds of blocks' pools, which the
    /// scatters read.
    pub grounds: Vec<BufferId>,
    /// The map of tracks round the camera, which lays blades down where
    /// something trod.
    pub tracks: BufferId,
    pub index_buffer: vk::Buffer,
}

/// How many cells of `cell` metres a ring from `near` to `far` metres
/// round a point can reach into: room enough for a blade each.
fn ring_cells(near: f32, far: f32, cell: f32) -> u64 {
    let slack = cell * std::f32::consts::SQRT_2;
    let outer = far + slack;
    let inner = (near - slack).max(0.0);
    (std::f32::consts::PI * (outer * outer - inner * inner) / (cell * cell)).ceil() as u64 + 16
}

/// Where each of a kind's rings ends, as a share of its draw distance: each
/// twice as far as the one before, so the nearest, drawn finest, is small.
/// The GPU picks each ring's LOD (`grassReset`).
const RINGS: [f32; MAX_LODS] = [0.125, 0.25, 0.5, 1.0];

/// Picks each terrain's grass to scatter round `frame`'s camera this
/// frame, and writes the jobs and draws into `buffer`; `None` when there
/// is none.
pub(crate) fn gather(
    drawn: &[Drawn],
    (blocks, offset): (&[crate::blocks::DrawnBlocks], Vec3),
    frame: &crate::gpu::GpuFrame,
    origin: voxear_core::math::I64Vec3,
    buffer: &Buffer,
) -> Option<(Vec<GpuGrassJob>, Vec<([u32; 2], bool)>)> {
    // Round the camera the view culls from: its own, or the one it borrows (`GpuCull`).
    let camera = Vec3::from(frame.cull.position);
    let mut wanted: Vec<(Source, GrassLook)> = Vec::new();
    for (number, terrain) in drawn.iter().enumerate().take(crate::terrain::MAX_TERRAINS) {
        if terrain.grass.is_none() {
            continue;
        }
        for (kind, look) in terrain.look.grass.iter().enumerate().take(voxear_terrain::grass::MAX_GRASS) {
            if let Some(look) = look.filter(|look| look.density > 0.0) {
                wanted.push((Source::Terrain(number, kind), look));
            }
        }
    }
    // Each world of blocks' ground cover, as a terrain's kind of grass is drawn: a blade a cell of a block's
    // share of a plant, numbered past the terrains.
    let covers = ground_cover_jobs(blocks, offset, frame);
    for (number, (world, _, look, _)) in covers.iter().enumerate() {
        let cell = blocks[*world].block_size / look.density.max(1e-3).sqrt();
        let grass = GrassLook {
            mesh: look.mesh,
            material: look.material,
            density: 1.0 / (cell * cell),
            scale: look.scale,
            draw_distance: look.draw_distance,
            springs_back: COVER_SPRINGS_BACK,
        };
        wanted.push((Source::Cover(number), grass));
    }
    wanted.truncate(MAX_GRASS_JOBS);
    if wanted.is_empty() {
        return None;
    }
    // Each kind's cells and rings at its own distance; all drawn nearer if together they want more room than there is.
    let cell_of = |look: &GrassLook| (1.0 / look.density.max(1e-3).sqrt()).max(0.02);
    let reach_of = |look: &GrassLook, near: f32| look.draw_distance.min(cell_of(look) * MAX_CELLS as f32 * 0.5) * near;
    let room = |near: f32| -> u64 {
        wanted.iter().map(|(_, look)| ring_cells(0.0, reach_of(look, near), cell_of(look))).sum()
    };
    let mut near = 1.0f32;
    while room(near) > MAX_GRASS_BLADES && near > 0.05 {
        near *= 0.9;
    }
    let mut jobs = Vec::with_capacity(wanted.len());
    let mut dispatches = Vec::with_capacity(wanted.len());
    let mut draws: Vec<GpuGrassDraw> = Vec::new();
    let mut next = 0u32;
    let mut words: Vec<u32> = Vec::new();
    for (index, (source, look)) in wanted.into_iter().enumerate() {
        let (cell, reach) = (cell_of(&look), reach_of(&look, near));
        let (number, kind) = match source {
            Source::Terrain(number, kind) => (number, kind),
            Source::Cover(cover) => {
                // A world of blocks' ground cover: its surfaces and its chunks' faces looking up, within its reach.
                let (world, kind, cover, chunks) = &covers[cover];
                let runs: Vec<[u32; 2]> =
                    chunks.iter().filter(|(away, _)| *away <= reach).map(|(_, run)| *run).collect();
                if runs.is_empty() || words.len() + 8 + runs.len() * 2 > COVER_WORDS {
                    continue;
                }
                let first = words.len();
                words.extend_from_slice(&cover.surfaces);
                words.extend(runs.iter().flatten());
                let mut job = GpuGrassJob {
                    terrain: BLOCK_COVER | *world as u32,
                    kind: *kind as u32,
                    first_cell: [first as i32, runs.len() as i32],
                    cell_size: blocks[*world].block_size,
                    draw_distance: reach,
                    scale: look.scale,
                    mesh: look.mesh.0,
                    material: look.material.0,
                    lod_count: MAX_LODS as u32,
                    draws: draws.len() as u32,
                    springs_back: look.springs_back,
                    ..GpuGrassJob::default()
                };
                job.corner = [cover.density, 0.0];
                for lod in 0..MAX_LODS {
                    let inner = if lod == 0 { 0.0 } else { RINGS[lod - 1] * reach };
                    let outer = RINGS[lod] * reach;
                    let capacity = ring_cells(inner, outer, cell).min(MAX_GRASS_BLADES - u64::from(next)) as u32;
                    job.lod_reach[lod] = outer;
                    job.offsets[lod] = next;
                    job.capacities[lod] = capacity;
                    draws.push(GpuGrassDraw { offset: next, job: index as u32 });
                    next += capacity;
                }
                dispatches.push(([runs.len() as u32, COVER_SLICES], true));
                jobs.push(job);
                continue;
            }
        };
        let terrain = &drawn[number];
        let lod_count = MAX_LODS;
        let mut job = GpuGrassJob {
            terrain: number as u32,
            kind: kind as u32,
            cell_size: cell,
            draw_distance: reach,
            scale: look.scale,
            mesh: look.mesh.0,
            material: look.material.0,
            lod_count: lod_count as u32,
            draws: draws.len() as u32,
            springs_back: look.springs_back.max(0.0),
            ..GpuGrassJob::default()
        };
        // The square the distance reaches, within the terrain.
        let size = terrain.tiles.map(|tiles| (tiles * voxear_terrain::TILE_QUADS) as f32 * terrain.spacing);
        let low = [(camera.x - reach).max(terrain.corner.x), (camera.z - reach).max(terrain.corner.z)];
        let high =
            [(camera.x + reach).min(terrain.corner.x + size[0]), (camera.z + reach).min(terrain.corner.z + size[1])];
        if low[0] >= high[0] || low[1] >= high[1] {
            continue;
        }
        // Counted from the level's middle, in f64, as it may be far from the world's.
        let (from, size) = ([origin.x as f64, origin.z as f64], f64::from(cell));
        let first = [0, 1].map(|axis| ((f64::from(low[axis]) + from[axis]) / size).floor());
        job.first_cell = first.map(|cells| cells as i32);
        job.corner = [0, 1].map(|axis| (first[axis] * size - from[axis]) as f32);
        job.cells = [0, 1].map(|axis| {
            let last = ((f64::from(high[axis]) + from[axis]) / size).ceil();
            ((last - first[axis]).max(0.0) as u32 + 1).min(MAX_CELLS)
        });
        for lod in 0..lod_count {
            let inner = if lod == 0 { 0.0 } else { RINGS[lod - 1] * reach };
            let outer = RINGS[lod] * reach;
            let capacity = ring_cells(inner, outer, cell).min(MAX_GRASS_BLADES - u64::from(next)) as u32;
            job.lod_reach[lod] = outer;
            job.offsets[lod] = next;
            job.capacities[lod] = capacity;
            draws.push(GpuGrassDraw { offset: next, job: index as u32 });
            next += capacity;
        }
        dispatches.push(([job.cells[0].div_ceil(8), job.cells[1].div_ceil(8)], false));
        jobs.push(job);
    }
    if jobs.is_empty() {
        return None;
    }
    // Draws name their job by its place among those kept.
    let mut renumbered = Vec::with_capacity(draws.len());
    for (place, job) in jobs.iter_mut().enumerate() {
        let first = job.draws as usize;
        job.draws = renumbered.len() as u32;
        for draw in &draws[first..first + job.lod_count as usize] {
            renumbered.push(GpuGrassDraw { job: place as u32, ..*draw });
        }
    }
    buffer.write(0, &jobs);
    buffer.write(JOBS_SIZE as u64, &renumbered);
    if !words.is_empty() {
        buffer.write(COVER_AT as u64, &words);
    }
    Some((jobs, dispatches))
}

/// Each kind of ground cover of each world of blocks a view draws, as
/// `blocks::gather` numbers the worlds: the world, the kind, its look, and
/// the runs of faces looking up, and of blocks of parts, of its own chunks
/// (no coarser ring's) whose boxes the frustum meets, each with how far its
/// box is from the camera, nearest first. A world drawn smooth or turned grows none.
fn ground_cover_jobs(
    blocks: &[crate::blocks::DrawnBlocks],
    offset: Vec3,
    frame: &crate::gpu::GpuFrame,
) -> Vec<(usize, usize, GroundCoverLook, Vec<(f32, [u32; 2])>)> {
    let camera = Vec3::from(frame.cull.position);
    let mut jobs = Vec::new();
    for (number, world) in blocks.iter().take(crate::blocks::MAX_BLOCK_WORLDS).enumerate() {
        if world.look.ground_cover.is_empty() || world.smooth || world.turn.is_some() {
            continue;
        }
        let corner = world.corner - offset;
        let local = camera - corner;
        let planes = crate::blocks::turned_planes(&frame.cull.planes, corner, voxear_core::math::Quat::IDENTITY);
        let farthest = world.look.ground_cover.iter().map(|cover| cover.draw_distance).fold(0.0, f32::max);
        // A plant stands over its block: a chunk is looked at as reaching a few metres higher than its faces.
        let mut chunks: Vec<(f32, [u32; 2])> = Vec::new();
        for chunk in world.chunks.iter() {
            let ([first, count], [parts, words]) = (chunk.facings[UP], chunk.parts);
            if chunk.key[0] != 0 || chunk.cone.is_some() || count == 0 && words == 0 {
                continue;
            }
            let [low, high] = chunk.bounds;
            let away = (local.clamp(low, high) - local).length();
            let grown = [low, high + Vec3::new(0.0, PLANT_HEADROOM, 0.0)];
            if away <= farthest && crate::blocks::within(&planes, grown) {
                if count > 0 {
                    chunks.push((away, [first, count]));
                }
                // Its blocks of parts, among them slabs, whose tops grow plants as a block's do.
                if words > 0 {
                    chunks.push((away, [parts, words | COVER_PARTS]));
                }
            }
        }
        chunks.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (kind, cover) in world.look.ground_cover.iter().enumerate() {
            if cover.density > 0.0 && cover.draw_distance > 0.0 && !chunks.is_empty() {
                jobs.push((number, kind, *cover, chunks.clone()));
            }
        }
    }
    jobs
}

/// What a job grows: a terrain's kind of grass, by the terrain's place and
/// the kind's, or a world of blocks' kind of ground cover, by its place
/// among [`ground_cover_jobs`].
#[derive(Clone, Copy)]
enum Source {
    Terrain(usize, usize),
    Cover(usize),
}

/// A run of a chunk's blocks of parts' words, two quads' a face, in place of
/// its faces looking up, has this bit over its length (`grassCoverParts`).
const COVER_PARTS: u32 = 1 << 31;

/// Which of a chunk's facings looks up (see `voxear_terrain::blocks::mesh`).
const UP: usize = 2;

/// How far over its faces a chunk's plants may reach, in metres.
const PLANT_HEADROOM: f32 = 3.0;

/// **Grass reset**, **Grass scatter** and **Grass clamp**: each draw's
/// command made afresh, the blades that grow this frame written and
/// counted, and each count kept within its room.
pub(crate) fn record_scatter(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), grass: &ViewGrass) {
    let draws = grass.draws;
    let groups = draws.div_ceil(64);
    fg.add_pass("Grass reset", &[], &[(grass.args_id, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.grass_reset, vk::PipelineBindPoint::COMPUTE, &Push { list: draws, ..push });
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });
    let mut buffers: Vec<_> = grass.grounds.iter().map(|&id| (id, BufferAccess::ComputeRead)).collect();
    buffers.extend([(grass.args_id, BufferAccess::ComputeWrite), (grass.blades_id, BufferAccess::ComputeWrite)]);
    let jobs = grass.jobs.clone();
    fg.add_pass("Grass scatter", &[], &buffers, move |ctx| {
        for (blocks, pipeline) in [(false, pipelines.grass_scatter), (true, pipelines.ground_cover_scatter)] {
            if !jobs.iter().any(|(_, cover)| *cover == blocks) {
                continue;
            }
            bind(ctx, pipeline, vk::PipelineBindPoint::COMPUTE, &push);
            for (job, ([x, z], _)) in jobs.iter().enumerate().filter(|(_, (_, cover))| *cover == blocks) {
                ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: job as u32, ..push });
                unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, *x, *z, 1) };
            }
        }
    });
    fg.add_pass("Grass clamp", &[], &[(grass.args_id, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.grass_clamp, vk::PipelineBindPoint::COMPUTE, &Push { list: draws, ..push });
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });
}

/// Draws every grass draw, each by its indirect command: a terrain's grass
/// with the first of `pipelines`, seen from both sides, and ground cover
/// with the second, which culls the backs of its closed cubes.
fn draw_all(ctx: &crate::graph::PassContext, grass: &ViewGrass, push: &Push, pipelines: [vk::Pipeline; 2]) {
    unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, grass.index_buffer, 0, vk::IndexType::UINT32) };
    let mut bound = None;
    for draw in 0..grass.draws {
        // Every job has a draw for each of its LODs.
        let cover = grass.jobs.get(draw as usize / MAX_LODS).is_some_and(|(_, cover)| *cover);
        if bound != Some(cover) {
            bind(ctx, pipelines[usize::from(cover)], vk::PipelineBindPoint::GRAPHICS, push);
            bound = Some(cover);
        }
        ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: draw, ..*push });
        let offset = u64::from(draw) * DRAW_COMMAND as u64;
        unsafe { ctx.gpu.device.cmd_draw_indexed_indirect(ctx.cmd, grass.args, offset, 1, DRAW_COMMAND as u32) };
    }
}

/// What a grass draw reads: its commands, its blades, and `more`.
fn reads(grass: &ViewGrass, more: &[(BufferId, BufferAccess)]) -> Vec<(BufferId, BufferAccess)> {
    let mut reads = vec![(grass.args_id, BufferAccess::IndirectRead), (grass.blades_id, BufferAccess::ShaderRead)];
    reads.extend(grass.grounds.iter().map(|&id| (id, BufferAccess::ShaderRead)));
    reads.push((grass.tracks, BufferAccess::ShaderRead));
    reads.extend_from_slice(more);
    reads
}

/// **Depth (grass)**: the blades as depth alone, with each pixel's normal
/// and roughness, over what the depth passes drew.
pub(crate) fn record_depth(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    grass: &ViewGrass,
    targets: crate::passes::DepthTargets,
    more: &[(BufferId, BufferAccess)],
) {
    let grass = grass.clone();
    fg.add_pass("Depth (grass)", &targets.images(), &reads(&grass, more), move |ctx| {
        targets.begin(ctx);
        draw_all(ctx, &grass, &push, [pipelines.grass_depth, pipelines.cover_depth]);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Opaque (grass)**: the blades shaded, over what the opaque passes drew;
/// `pipelines`, a terrain's grass's and ground cover's, write depth or test
/// against the depth passes'.
pub(crate) fn record_opaque(
    fg: &mut FrameGraph<'_>,
    push: Push,
    pipelines: [vk::Pipeline; 2],
    grass: &ViewGrass,
    images: Vec<(ImageId, ImageAccess)>,
    (hdr, velocity, depth): (ImageId, ImageId, ImageId),
    more: &[(BufferId, BufferAccess)],
) {
    let grass = grass.clone();
    fg.add_pass("Opaque (grass)", &images, &reads(&grass, more), move |ctx| {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let color = [attachment(ctx.view(hdr), layout, None), attachment(ctx.view(velocity), layout, None)];
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, ctx.extent(hdr), &color, Some(&target));
        draw_all(ctx, &grass, &push, pipelines);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// Keeps a look's grass list shared, as `TerrainLook` holds it.
pub(crate) type Looks = Arc<[Option<GrassLook>]>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_has_room_for_a_blade_in_every_cell_it_reaches() {
        const { assert!(size_of::<GpuGrassJob>() == 116 && size_of::<GpuGrassDraw>() == 8) };
        // The shaders find the ground cover's words past room for 256 draws (`grassCoverWords`).
        const { assert!(MAX_GRASS_DRAWS == 256 && COVER_AT == JOBS_SIZE + 256 * 8 && COVER_AT % 4 == 0) };
        // A disc 10 m round of half-metre cells: about 1,257 cells, and its edge's.
        let room = ring_cells(0.0, 10.0, 0.5);
        assert!((1257..1600).contains(&room), "{room}");
        assert!(ring_cells(5.0, 10.0, 0.5) < room, "a ring has less than its disc");
    }
}
