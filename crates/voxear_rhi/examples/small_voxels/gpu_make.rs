//! The world made on the GPU (`make.slang`), timed pass by pass and checked
//! against the one made on the CPU.

use std::time::Instant;

use anyhow::Result;
use voxear_rhi::{Bindless, Buffer, Gpu, ShaderModule, create_compute_pipeline, vk};

use crate::{
    BRICK_WORDS, HEIGHT_METRES, Made, PALETTE, SECTOR, SECTOR_WORDS, Scene, buffer, compute_barrier, place, plant_ints,
    upload,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuMakeJob {
    heights: u64,
    columns: u64,
    plants: u64,
    plant_lists: u64,
    plant_indices: u64,
    candidates: u64,
    counters: u64,
    slots: u64,
    loose: u64,
    bricks: u64,
    sectors: u64,
    materials: u64,
    palette: u64,
    sector_count: [i32; 3],
    voxel: f32,
    camera: [f32; 2],
    across: i32,
    candidate_room: u32,
}

/// What the GPU made: the walk's three buffers, which the caller destroys.
pub struct GpuMade {
    pub sectors: Buffer,
    pub bricks: Buffer,
    pub materials: Buffer,
}

const PASSES: [&str; 5] = ["columns", "candidates", "arguments", "bricks", "sectors"];

pub fn make_on_gpu(
    gpu: &Gpu,
    bindless: &Bindless,
    module: &ShaderModule,
    scene: &Scene,
    cpu: &Made,
) -> Result<GpuMade> {
    let v = scene.voxel;
    let across = (scene.across / v) as i32;
    let columns = across / SECTOR;
    let tall = (HEIGHT_METRES / v) as i32 / SECTOR;
    let rows = tall * 8;

    let placing = Instant::now();
    let (plants, _, by_column) = place(scene);
    let mut lists = Vec::with_capacity(by_column.len() * 2);
    let mut indices: Vec<u32> = Vec::new();
    for list in &by_column {
        lists.push(indices.len() as u32);
        lists.push(list.len() as u32);
        indices.extend(list.iter().map(|&i| i as u32));
    }
    let ints: Vec<i32> = plants.iter().flat_map(plant_ints).collect();
    let placed_ms = placing.elapsed().as_secs_f64() * 1000.0;

    let none = vk::BufferUsageFlags::empty();
    let side = u64::from((across + 2) as u32);
    let bricks_across = u64::from((across / 8) as u32);
    let cpu_bricks = (cpu.bricks.len() / BRICK_WORDS) as u64;
    let brick_room = cpu_bricks + cpu_bricks / 10 + 1024;
    let candidate_room = (cpu_bricks * 6 + (1 << 20)) as u32;
    let heights = buffer(gpu, "make heights", side * side * 4, none)?;
    let column_info = buffer(gpu, "make columns", (across as u64).pow(2) * 4, none)?;
    let plant_buffer = upload(gpu, "make plants", bytemuck::cast_slice(&ints))?;
    let list_buffer = upload(gpu, "make plant lists", bytemuck::cast_slice(&lists))?;
    let index_buffer = upload(gpu, "make plant indices", bytemuck::cast_slice(&indices))?;
    let candidates = buffer(gpu, "make candidates", u64::from(candidate_room) * 4, none)?;
    let counters = buffer(gpu, "make counters", 64, vk::BufferUsageFlags::INDIRECT_BUFFER)?;
    let slots = buffer(gpu, "make slots", bricks_across * bricks_across * rows as u64 * 4, none)?;
    let loose = buffer(gpu, "make loose bricks", brick_room * BRICK_WORDS as u64 * 4, none)?;
    let bricks = buffer(gpu, "made bricks", brick_room * BRICK_WORDS as u64 * 4, none)?;
    let sector_count = (columns * columns * tall) as u64;
    let sectors = buffer(gpu, "made sectors", sector_count * SECTOR_WORDS as u64 * 4, none)?;
    let material_bytes = (cpu.materials.len() as u64 + cpu.materials.len() as u64 / 10 + 4096) & !3;
    let materials = buffer(gpu, "made materials", material_bytes, none)?;
    let palette: Vec<u32> = PALETTE
        .iter()
        .map(|c| (c[0] * 255.0) as u32 | ((c[1] * 255.0) as u32) << 8 | ((c[2] * 255.0) as u32) << 16)
        .collect();
    let palette = upload(gpu, "make palette", bytemuck::cast_slice(&palette))?;
    let job = GpuMakeJob {
        heights: heights.address,
        columns: column_info.address,
        plants: plant_buffer.address,
        plant_lists: list_buffer.address,
        plant_indices: index_buffer.address,
        candidates: candidates.address,
        counters: counters.address,
        slots: slots.address,
        loose: loose.address,
        bricks: bricks.address,
        sectors: sectors.address,
        materials: materials.address,
        palette: palette.address,
        sector_count: [columns, tall, columns],
        voxel: v,
        camera: scene.camera,
        across,
        candidate_room,
    };
    let job_buffer = upload(gpu, "make job", bytemuck::bytes_of(&job))?;
    let push = job_buffer.address;
    let layout = bindless.pipeline_layout;
    let pipelines = [
        create_compute_pipeline(gpu, layout, "make columns", module, c"makeColumns")?,
        create_compute_pipeline(gpu, layout, "make candidates", module, c"makeCandidates")?,
        create_compute_pipeline(gpu, layout, "make arguments", module, c"makeArguments")?,
        create_compute_pipeline(gpu, layout, "make bricks", module, c"makeBricks")?,
        create_compute_pipeline(gpu, layout, "make sectors", module, c"makeSectors")?,
    ];
    let queries = unsafe {
        gpu.device.create_query_pool(
            &vk::QueryPoolCreateInfo::default().query_type(vk::QueryType::TIMESTAMP).query_count(8),
            None,
        )?
    };
    let rw = vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE;
    let mut runs: Vec<[f64; 7]> = Vec::new();
    for _ in 0..6 {
        gpu.immediate(|cmd| unsafe {
            let device = &gpu.device;
            device.cmd_reset_query_pool(cmd, queries, 0, 8);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::TOP_OF_PIPE, queries, 0);
            for cleared in [&counters, &slots, &materials] {
                device.cmd_fill_buffer(cmd, cleared.handle, 0, vk::WHOLE_SIZE, 0);
            }
            compute_barrier(gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, queries, 1);
            let bind = |k: usize| {
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipelines[k]);
                bindless.push(gpu, cmd, &push);
            };
            let side = (across + 2) as u32;
            bind(0);
            device.cmd_dispatch(cmd, side.div_ceil(8), side.div_ceil(8), 1);
            compute_barrier(gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, queries, 2);
            bind(1);
            device.cmd_dispatch(cmd, ((across / 8) as u32).pow(2).div_ceil(64), 1, 1);
            compute_barrier(gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, queries, 3);
            bind(2);
            device.cmd_dispatch(cmd, 1, 1, 1);
            compute_barrier(
                gpu,
                cmd,
                vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::DRAW_INDIRECT,
                rw | vk::AccessFlags2::INDIRECT_COMMAND_READ,
            );
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, queries, 4);
            bind(3);
            device.cmd_dispatch_indirect(cmd, counters.handle, 32);
            compute_barrier(gpu, cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, rw);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::COMPUTE_SHADER, queries, 5);
            bind(4);
            device.cmd_dispatch(cmd, columns as u32, tall as u32, columns as u32);
            device.cmd_write_timestamp2(cmd, vk::PipelineStageFlags2::BOTTOM_OF_PIPE, queries, 6);
        })?;
        let mut stamps = [0u64; 7];
        unsafe {
            gpu.device.get_query_pool_results(
                queries,
                0,
                &mut stamps,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )?
        };
        let ms = |a: usize, b: usize| (stamps[b] - stamps[a]) as f64 * f64::from(gpu.info.timestamp_period) / 1e6;
        runs.push([ms(0, 6), ms(0, 1), ms(1, 2), ms(2, 3), ms(3, 4), ms(4, 5), ms(5, 6)]);
    }
    let best = runs.iter().skip(1).min_by(|a, b| a[0].total_cmp(&b[0])).unwrap();
    let counts: Vec<u32> =
        gpu.read_buffer(&counters, 0, 32)?.chunks(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    println!(
        "made on the GPU in {:.1} ms, least of 5 (clearing {:.1}, {} {:.1}, {} {:.1}, {} {:.2}, {} {:.1}, {} {:.1}); plants placed on the CPU in {placed_ms:.1} ms",
        best[0],
        best[1],
        PASSES[0],
        best[2],
        PASSES[1],
        best[3],
        PASSES[2],
        best[4],
        PASSES[3],
        best[5],
        PASSES[4],
        best[6],
    );
    println!(
        "  {} candidates, {} bricks (CPU {}), {} solid voxels (CPU {}), {} material bytes{}",
        counts[0],
        counts[1],
        cpu_bricks,
        counts[4],
        cpu.solid,
        counts[2],
        if u64::from(counts[0]) > u64::from(candidate_room) || u64::from(counts[1]) > brick_room {
            " — OUT OF ROOM, the world is cut short"
        } else {
            ""
        }
    );
    gpu.wait_idle();
    unsafe {
        gpu.device.destroy_query_pool(queries, None);
        for pipeline in pipelines {
            gpu.device.destroy_pipeline(pipeline, None);
        }
    }
    for done in [
        heights,
        column_info,
        plant_buffer,
        list_buffer,
        index_buffer,
        candidates,
        counters,
        slots,
        loose,
        palette,
        job_buffer,
    ] {
        gpu.destroy_buffer(done);
    }
    Ok(GpuMade { sectors, bricks, materials })
}
