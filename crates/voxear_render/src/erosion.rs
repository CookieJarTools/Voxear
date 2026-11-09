//! Erosion on the GPU, for a terrain's Erosion layer (see
//! `voxear_terrain::erosion`): rain running off the ground, cutting it where
//! it runs fast and dropping what it carries where it slows, and ground
//! steeper than loose ground stands slumping (`shaders/erosion.slang`).
//!
//! It runs once, when the editor asks, outside the frames: the ground is
//! uploaded, worn down step after step in batches the GPU works through
//! without the editor waiting on each, and read back, what the water still
//! carried settled where it was.

use anyhow::Result;
use voxear_rhi::{Bindless, Buffer, Gpu, MemoryLocation, vk};
use voxear_terrain::erosion::{Grid, Weather};

use crate::pipelines::Pipelines;

/// What the erosion passes read, as `ErosionJob` in `shaders/erosion.slang`
/// has it: where each of the grid's buffers is, its size and spacing, and
/// how the weather wears it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuErosionJob {
    ground: u64,
    carved: u64,
    water: u64,
    held: u64,
    flux: u64,
    velocity: u64,
    sediment: u64,
    carried: u64,
    slump: u64,
    cells: [u32; 2],
    spacing: f32,
    dt: f32,
    rain: f32,
    capacity: f32,
    wear: f32,
    settle: f32,
    evaporation: f32,
    talus: f32,
    slump_rate: f32,
    pad: u32,
}

/// How many steps go to the GPU at once.
const BATCH: u32 = 50;

/// The most steps erosion runs.
pub const MOST_STEPS: u32 = 5000;

/// `grid`'s heights worn down by `weather`.
pub(crate) fn erode(
    gpu: &Gpu,
    bindless: &Bindless,
    pipelines: &Pipelines,
    grid: &Grid,
    weather: &Weather,
) -> Result<Vec<f32>> {
    let count = u64::from(grid.cells[0]) * u64::from(grid.cells[1]);
    let usage =
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC;
    // Each buffer's name and how many floats a cell it holds.
    let wanted: [(&str, u64); 9] = [
        ("erosion ground", 1),
        ("erosion carved", 1),
        ("erosion water", 1),
        ("erosion held", 1),
        ("erosion flux", 4),
        ("erosion velocity", 2),
        ("erosion sediment", 1),
        ("erosion carried", 1),
        ("erosion slump", 8),
    ];
    let mut buffers: Vec<Buffer> = Vec::with_capacity(wanted.len());
    for (name, floats) in wanted {
        match gpu.create_buffer(name, count * floats * 4, usage, MemoryLocation::GpuOnly) {
            Ok(buffer) => buffers.push(buffer),
            Err(error) => {
                buffers.into_iter().for_each(|buffer| gpu.destroy_buffer(buffer));
                return Err(error);
            }
        }
    }
    let result = run(gpu, bindless, pipelines, grid, weather, &buffers, count);
    for buffer in buffers {
        gpu.destroy_buffer(buffer);
    }
    result
}

fn run(
    gpu: &Gpu,
    bindless: &Bindless,
    pipelines: &Pipelines,
    grid: &Grid,
    weather: &Weather,
    buffers: &[Buffer],
    count: u64,
) -> Result<Vec<f32>> {
    let address = |at: usize| buffers[at].address;
    let job = GpuErosionJob {
        ground: address(0),
        carved: address(1),
        water: address(2),
        held: address(3),
        flux: address(4),
        velocity: address(5),
        sediment: address(6),
        carried: address(7),
        slump: address(8),
        cells: grid.cells,
        spacing: grid.spacing,
        dt: 0.02,
        rain: weather.rain.max(0.0) / 1000.0,
        capacity: 0.01,
        wear: weather.wear.clamp(0.0, 1.0),
        settle: 0.3,
        evaporation: 0.02,
        talus: weather.slump.clamp(1.0, 89.0).to_radians().tan(),
        slump_rate: 0.15,
        pad: 0,
    };
    let jobs = gpu.create_buffer(
        "erosion job",
        size_of::<GpuErosionJob>() as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        MemoryLocation::GpuOnly,
    )?;
    let ran = (|| {
        gpu.upload_buffer(&jobs, 0, bytemuck::bytes_of(&job))?;
        gpu.upload_buffer(&buffers[0], 0, bytemuck::cast_slice(&grid.heights))?;
        gpu.immediate(|cmd| unsafe {
            for buffer in &buffers[2..] {
                gpu.device.cmd_fill_buffer(cmd, buffer.handle, 0, vk::WHOLE_SIZE, 0);
            }
        })?;
        let passes = [
            pipelines.erosion_flux,
            pipelines.erosion_water,
            pipelines.erosion_carve,
            pipelines.erosion_carry,
            pipelines.erosion_slump,
            pipelines.erosion_settle,
        ];
        let groups = [grid.cells[0].div_ceil(8), grid.cells[1].div_ceil(8)];
        let push = jobs.address;
        let steps = weather.steps.min(MOST_STEPS);
        let mut done = 0;
        while done < steps {
            let batch = BATCH.min(steps - done);
            gpu.immediate(|cmd| unsafe {
                let barrier = vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
                let wait = |cmd| {
                    gpu.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::DependencyFlags::empty(),
                        &[barrier],
                        &[],
                        &[],
                    )
                };
                wait(cmd);
                for _ in 0..batch {
                    for &pipeline in &passes {
                        gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
                        bindless.push(gpu, cmd, &push);
                        gpu.device.cmd_dispatch(cmd, groups[0], groups[1], 1);
                        wait(cmd);
                    }
                }
            })?;
            done += batch;
        }
        let ground: Vec<f32> = bytemuck::pod_collect_to_vec(&gpu.read_buffer(&buffers[0], 0, count * 4)?);
        let sediment: Vec<f32> = bytemuck::pod_collect_to_vec(&gpu.read_buffer(&buffers[6], 0, count * 4)?);
        // What the water still carried settles where it is.
        Ok(ground.iter().zip(&sediment).map(|(ground, carried)| ground + carried).collect())
    })();
    gpu.destroy_buffer(jobs);
    ran
}
