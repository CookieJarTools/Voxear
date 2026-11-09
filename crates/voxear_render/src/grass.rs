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
}

/// One draw, as its vertex shader reads it: where its blades start, its job.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuGrassDraw {
    pub offset: u32,
    pub job: u32,
}

const JOBS_SIZE: usize = MAX_GRASS_JOBS * size_of::<GpuGrassJob>();

/// How large a view's buffer of jobs and draws is.
pub const FRAME_SIZE: u64 = (JOBS_SIZE + MAX_GRASS_DRAWS * size_of::<GpuGrassDraw>()) as u64;

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
    pub jobs: Vec<[u32; 2]>,
    pub draws: u32,
    pub args: vk::Buffer,
    pub args_id: BufferId,
    pub blades_id: BufferId,
    /// The terrains' buffers, which the scatter reads.
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
    frame: &crate::gpu::GpuFrame,
    buffer: &Buffer,
) -> Option<(Vec<GpuGrassJob>, Vec<[u32; 2]>)> {
    let camera = Vec3::from(frame.camera_position);
    let mut wanted: Vec<(usize, usize, GrassLook)> = Vec::new();
    for (number, terrain) in drawn.iter().enumerate().take(crate::terrain::MAX_TERRAINS) {
        if terrain.grass.is_none() {
            continue;
        }
        for (kind, look) in terrain.look.grass.iter().enumerate().take(voxear_terrain::grass::MAX_GRASS) {
            if let Some(look) = look.filter(|look| look.density > 0.0) {
                wanted.push((number, kind, look));
            }
        }
    }
    wanted.truncate(MAX_GRASS_JOBS);
    if wanted.is_empty() {
        return None;
    }
    // Each kind's cells and rings at its own distance; all drawn nearer if together they want more room than there is.
    let cell_of = |look: &GrassLook| (1.0 / look.density.max(1e-3).sqrt()).max(0.02);
    let reach_of = |look: &GrassLook, near: f32| look.draw_distance.min(cell_of(look) * MAX_CELLS as f32 * 0.5) * near;
    let room = |near: f32| -> u64 {
        wanted.iter().map(|(_, _, look)| ring_cells(0.0, reach_of(look, near), cell_of(look))).sum()
    };
    let mut near = 1.0f32;
    while room(near) > MAX_GRASS_BLADES && near > 0.05 {
        near *= 0.9;
    }
    let mut jobs = Vec::with_capacity(wanted.len());
    let mut dispatches = Vec::with_capacity(wanted.len());
    let mut draws: Vec<GpuGrassDraw> = Vec::new();
    let mut next = 0u32;
    for (index, (number, kind, look)) in wanted.into_iter().enumerate() {
        let terrain = &drawn[number];
        let (cell, reach) = (cell_of(&look), reach_of(&look, near));
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
        job.first_cell = low.map(|v| (v / cell).floor() as i32);
        job.cells = [0, 1]
            .map(|axis| (((high[axis] / cell).ceil() as i32 - job.first_cell[axis]).max(0) as u32 + 1).min(MAX_CELLS));
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
        dispatches.push([job.cells[0].div_ceil(8), job.cells[1].div_ceil(8)]);
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
    Some((jobs, dispatches))
}

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
        bind(ctx, pipelines.grass_scatter, vk::PipelineBindPoint::COMPUTE, &push);
        for (job, [x, z]) in jobs.iter().enumerate() {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list: job as u32, ..push });
            unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, *x, *z, 1) };
        }
    });
    fg.add_pass("Grass clamp", &[], &[(grass.args_id, BufferAccess::ComputeWrite)], move |ctx| {
        bind(ctx, pipelines.grass_clamp, vk::PipelineBindPoint::COMPUTE, &Push { list: draws, ..push });
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, groups, 1, 1) };
    });
}

/// Draws every grass draw with `pipeline`, each by its indirect command.
fn draw_all(ctx: &crate::graph::PassContext, grass: &ViewGrass, push: &Push) {
    unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, grass.index_buffer, 0, vk::IndexType::UINT32) };
    for draw in 0..grass.draws {
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
    (depth, surface): (ImageId, ImageId),
    more: &[(BufferId, BufferAccess)],
) {
    let images = [(depth, ImageAccess::DepthAttachment), (surface, ImageAccess::ColorAttachment)];
    let grass = grass.clone();
    fg.add_pass("Depth (grass)", &images, &reads(&grass, more), move |ctx| {
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        let rough = [attachment(ctx.view(surface), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, ctx.extent(depth), &rough, Some(&target));
        bind(ctx, pipelines.grass_depth, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_all(ctx, &grass, &push);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Opaque (grass)**: the blades shaded, over what the opaque passes drew;
/// `pipeline` writes depth or tests against the depth passes'.
pub(crate) fn record_opaque(
    fg: &mut FrameGraph<'_>,
    push: Push,
    pipeline: vk::Pipeline,
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
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_all(ctx, &grass, &push);
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
        const { assert!(size_of::<GpuGrassJob>() == 108 && size_of::<GpuGrassDraw>() == 8) };
        // A disc 10 m round of half-metre cells: about 1,257 cells, and its edge's.
        let room = ring_cells(0.0, 10.0, 0.5);
        assert!((1257..1600).contains(&room), "{room}");
        assert!(ring_cells(5.0, 10.0, 0.5) < room, "a ring has less than its disc");
    }
}
