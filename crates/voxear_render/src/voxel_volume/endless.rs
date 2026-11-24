//! An endless Voxel Volume: ground of small voxels without end, made on the
//! GPU round the camera (`shaders/voxel_world.slang`).
//!
//! Its world is a window of sector columns round the camera, each a sector
//! (64 voxels) a side and as tall as the volume, kept in a slot of the
//! window by where it is in the world, wrapping. Each slot has room for so
//! many bricks and material bytes of its own, so a column made over another
//! needs nothing freed. As the camera moves, the columns that come into the
//! window are made, the nearest first, [`MAX_JOBS`] a frame; until then the
//! walk finds its slot's entry names another column and takes it as empty.

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Bindless, Buffer, Gpu, MemoryLocation, vk};

use super::{GROUND_COLORS, GpuVoxelVolume, MOST_VOXELS_A_METRE, VoxelGround, VoxelVolume};
use crate::pipelines::Pipelines;

/// How many columns are made a frame at most.
pub(crate) const MAX_JOBS: usize = 16;
/// How many bricks, and material bytes, a column's slot has room for: a
/// column of grass and flowers on hills takes 150 to 250 bricks.
const BRICK_ROOM: u32 = 384;
const MATERIAL_ROOM: u32 = BRICK_ROOM * 160;
const ENTRY_WORDS: u64 = 28;
const BRICK_BYTES: u64 = 80;
const SIDE: u64 = 66;
const COUNTERS: u64 = 4;
/// The most voxel columns a window is across: 256 sector columns.
const LARGEST_WINDOW: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuWorldJob {
    column: [i32; 2],
    slot: u32,
    pad: u32,
}

/// What the passes making columns read (`WorldMake` in voxel_world.slang).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuWorldMake {
    table: u64,
    bricks: u64,
    materials: u64,
    palette: u64,
    heights: u64,
    columns: u64,
    slots: u64,
    loose: u64,
    counters: u64,
    candidates: u64,
    height: f32,
    hills: f32,
    hill_spacing: f32,
    grass: f32,
    grass_height: f32,
    flowers: f32,
    seed: u32,
    voxel: f32,
    window: u32,
    tall: u32,
    brick_room: u32,
    material_room: u32,
    job_count: u32,
    candidate_room: u32,
    pad: [u32; 2],
    jobs: [GpuWorldJob; MAX_JOBS],
}

/// How the passes making columns are pushed.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldPush {
    make: u64,
}

/// An endless volume's buffers and which column each slot holds.
pub(crate) struct Endless {
    /// What it was made for: its voxels, height, reach and ground.
    pub from: VoxelVolume,
    window: u32,
    tall: u32,
    table: Buffer,
    bricks: Buffer,
    materials: Buffer,
    palette: Buffer,
    empty: Buffer,
    heights: Buffer,
    columns: Buffer,
    slots: Buffer,
    loose: Buffer,
    counters: Buffer,
    candidates: Buffer,
    /// The passes' records: one a frame in flight, and one for making
    /// columns before the frames start.
    makes: Vec<Buffer>,
    /// Which column each slot holds, made or asked for.
    made: Vec<Option<[i32; 2]>>,
    /// The columns made this frame, and their slots.
    jobs: Vec<([i32; 2], u32)>,
    /// The window's least sector, this frame.
    window_sector: [i32; 3],
    /// Its corner in the world, which its sectors count from.
    corner: Vec3,
}

/// How many voxel metres a sector column is a side.
fn sector_metres(volume: &VoxelVolume) -> f32 {
    64.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32
}

/// Whether two volumes make the same endless ground: everything but where
/// it is drawn from.
pub(crate) fn same_ground(a: &VoxelVolume, b: &VoxelVolume) -> bool {
    a.voxels_per_metre == b.voxels_per_metre
        && a.reach == b.reach
        && a.size.y == b.size.y
        && a.ground == b.ground
        && a.roughness == b.roughness
}

impl Endless {
    /// The buffers of an endless volume, every slot holding no column yet.
    pub fn new(gpu: &Gpu, volume: &VoxelVolume) -> Result<Self> {
        let metres = sector_metres(volume);
        let window = (((volume.reach.max(metres) * 2.0) / metres).ceil() as u32 + 1).clamp(2, LARGEST_WINDOW);
        let tall = ((volume.size.y.max(metres) / metres).ceil() as u32).clamp(1, 16);
        let slots_in = u64::from(window * window);
        let rows = u64::from(tall * 8);
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let make = |name: &str, size: u64, extra: vk::BufferUsageFlags| {
            gpu.create_buffer(&format!("voxel world {name}"), size.max(16), storage | extra, MemoryLocation::GpuOnly)
        };
        let none = vk::BufferUsageFlags::empty();
        let jobs = MAX_JOBS as u64;
        let table = make("table", slots_in * u64::from(tall) * ENTRY_WORDS * 4, none)?;
        let bricks = make("bricks", slots_in * u64::from(BRICK_ROOM) * BRICK_BYTES, none)?;
        let materials = make("materials", slots_in * u64::from(MATERIAL_ROOM), none)?;
        let palette = make("palette", GROUND_COLORS.len() as u64 * 4, none)?;
        let colours: Vec<u32> =
            GROUND_COLORS.iter().map(|c| u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16).collect();
        gpu.upload_buffer(&palette, 0, bytemuck::cast_slice(&colours))?;
        let empty = make("empty sector", 25 * 4, none)?;
        let mut nothing = [0u32; 25];
        nothing[24] = u32::MAX;
        gpu.upload_buffer(&empty, 0, bytemuck::cast_slice(&nothing))?;
        let heights = make("heights", jobs * SIDE * SIDE * 4, none)?;
        let columns = make("columns", jobs * 4096 * 4, none)?;
        let slots = make("slots", jobs * 64 * rows * 4, none)?;
        let loose = make("loose bricks", jobs * u64::from(BRICK_ROOM) * BRICK_BYTES, none)?;
        let counters = make("counters", (jobs * COUNTERS + 4) * 4, vk::BufferUsageFlags::INDIRECT_BUFFER)?;
        let candidates = make("candidates", jobs * 64 * rows * 4, none)?;
        let mut makes = Vec::new();
        for slot in 0..=voxear_rhi::FRAMES_IN_FLIGHT {
            makes.push(gpu.create_buffer(
                &format!("voxel world make {slot}"),
                size_of::<GpuWorldMake>() as u64,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                MemoryLocation::CpuToGpu,
            )?);
        }
        // No slot holds a column yet: every entry names a sector nowhere.
        gpu.immediate(|cmd| unsafe { gpu.device.cmd_fill_buffer(cmd, table.handle, 0, vk::WHOLE_SIZE, 0x8000_0000) })?;
        Ok(Self {
            from: volume.clone(),
            window,
            tall,
            table,
            bricks,
            materials,
            palette,
            empty,
            heights,
            columns,
            slots,
            loose,
            counters,
            candidates,
            makes,
            made: vec![None; (window * window) as usize],
            jobs: Vec::new(),
            window_sector: [0; 3],
            corner: Vec3::ZERO,
        })
    }

    /// Its size on the GPU, in bytes.
    pub fn bytes(&self) -> u64 {
        [&self.table, &self.bricks, &self.materials, &self.heights, &self.columns, &self.slots, &self.loose]
            .iter()
            .map(|buffer| buffer.size)
            .sum()
    }

    /// Moves the window round `camera`, the volume's corner at `corner`, and
    /// picks the columns to make this frame: those in the window that their
    /// slots do not hold, the nearest first.
    pub fn follow(&mut self, corner: Vec3, camera: Vec3) {
        self.corner = corner;
        let metres = sector_metres(&self.from);
        let at = ((camera - corner) / metres).floor();
        let half = (self.window / 2) as i32;
        self.window_sector = [at.x as i32 - half, 0, at.z as i32 - half];
        let w = self.window as i32;
        let mut wanted: Vec<(i32, [i32; 2])> = Vec::new();
        for dz in 0..w {
            for dx in 0..w {
                let column = [self.window_sector[0] + dx, self.window_sector[2] + dz];
                if self.made[self.slot_of(column) as usize] != Some(column) {
                    let distance = (column[0] - at.x as i32).pow(2) + (column[1] - at.z as i32).pow(2);
                    wanted.push((distance, column));
                }
            }
        }
        wanted.sort_unstable_by_key(|(distance, _)| *distance);
        self.jobs.clear();
        for (_, column) in wanted.into_iter().take(MAX_JOBS) {
            let slot = self.slot_of(column);
            self.made[slot as usize] = Some(column);
            self.jobs.push((column, slot));
        }
    }

    fn slot_of(&self, column: [i32; 2]) -> u32 {
        let w = self.window as i32;
        (column[1].rem_euclid(w) * w + column[0].rem_euclid(w)) as u32
    }

    /// Whether any column of the window is still to be made.
    pub fn unfinished(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// The volume as the walk reads it this frame, and its corner.
    pub fn drawn(&self) -> (GpuVoxelVolume, Vec3) {
        let metres = sector_metres(&self.from);
        let voxel = 1.0 / self.from.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
        let w = self.window as i32;
        let record = GpuVoxelVolume {
            sectors: self.table.address,
            bricks: self.bricks.address,
            materials: self.materials.address,
            palette: self.palette.address,
            sector_count: [w, self.tall as i32, w],
            voxel,
            corner: [0.0; 3],
            roughness: self.from.roughness.clamp(0.0, 1.0),
            window_sector: self.window_sector,
            windowed: 1,
            empty_sector: self.empty.address,
        };
        let at = Vec3::new(self.window_sector[0] as f32, 0.0, self.window_sector[2] as f32) * metres;
        (record, self.corner + at)
    }

    /// The buffers the walk reads, which making columns writes.
    pub fn pools(&self) -> [&Buffer; 3] {
        [&self.table, &self.bricks, &self.materials]
    }

    /// Writes this frame's record for the passes into its buffer for frame
    /// slot `slot` (or the one for loading, past the last), and gives its
    /// address: `None` with no columns to make.
    pub fn write(&self, slot: usize) -> Option<u64> {
        if self.jobs.is_empty() {
            return None;
        }
        let ground = self.from.ground.unwrap_or(VoxelGround { grass: 0.0, flowers: 0.0, ..VoxelGround::default() });
        let mut jobs = [GpuWorldJob::default(); MAX_JOBS];
        for (job, (column, place)) in jobs.iter_mut().zip(&self.jobs) {
            *job = GpuWorldJob { column: *column, slot: *place, pad: 0 };
        }
        let rows = u64::from(self.tall * 8);
        let make = GpuWorldMake {
            table: self.table.address,
            bricks: self.bricks.address,
            materials: self.materials.address,
            palette: self.palette.address,
            heights: self.heights.address,
            columns: self.columns.address,
            slots: self.slots.address,
            loose: self.loose.address,
            counters: self.counters.address,
            candidates: self.candidates.address,
            height: ground.height,
            hills: ground.hills,
            hill_spacing: ground.hill_spacing,
            grass: ground.grass,
            grass_height: ground.grass_height,
            flowers: ground.flowers,
            seed: ground.seed,
            voxel: 1.0 / self.from.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32,
            window: self.window,
            tall: self.tall,
            brick_room: BRICK_ROOM,
            material_room: MATERIAL_ROOM,
            job_count: self.jobs.len() as u32,
            candidate_room: (MAX_JOBS as u64 * 64 * rows) as u32,
            pad: [0; 2],
            jobs,
        };
        let buffer = &self.makes[slot.min(self.makes.len() - 1)];
        buffer.write(0, std::slice::from_ref(&make));
        Some(buffer.address)
    }

    /// What recording this frame's passes needs, apart from the volume.
    pub fn recording(&self) -> Recording {
        Recording {
            materials: self.materials.handle,
            slots: self.slots.handle,
            counters: self.counters.handle,
            jobs: self.jobs.iter().map(|(_, slot)| *slot).collect(),
            tall: self.tall,
        }
    }
}

/// What recording the passes that make a frame's columns needs.
pub(crate) struct Recording {
    materials: vk::Buffer,
    slots: vk::Buffer,
    counters: vk::Buffer,
    jobs: Vec<u32>,
    tall: u32,
}

impl Recording {
    /// Records the passes that make this frame's columns, from the record at
    /// `make`, into `cmd`: each slot's materials cleared, then the four
    /// passes with what each reads of the last finished between them.
    pub fn record(&self, gpu: &Gpu, cmd: vk::CommandBuffer, (pipelines, bindless): (&Pipelines, &Bindless), make: u64) {
        let device = &gpu.device;
        let barrier = |dst: vk::PipelineStageFlags2, access: vk::AccessFlags2| {
            let barrier = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::ALL_TRANSFER)
                .src_access_mask(vk::AccessFlags2::SHADER_WRITE | vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(dst)
                .dst_access_mask(access);
            unsafe { device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[barrier])) };
        };
        let read_write = vk::AccessFlags2::SHADER_READ | vk::AccessFlags2::SHADER_WRITE;
        let jobs = self.jobs.len() as u32;
        unsafe {
            for &slot in &self.jobs {
                let room = u64::from(MATERIAL_ROOM);
                device.cmd_fill_buffer(cmd, self.materials, u64::from(slot) * room, room, 0);
            }
            device.cmd_fill_buffer(cmd, self.slots, 0, vk::WHOLE_SIZE, 0);
            device.cmd_fill_buffer(cmd, self.counters, 0, vk::WHOLE_SIZE, 0);
        }
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        let push = WorldPush { make };
        let run = |pipeline: vk::Pipeline, groups: [u32; 3]| unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            bindless.push(gpu, cmd, &push);
            device.cmd_dispatch(cmd, groups[0], groups[1], groups[2]);
        };
        run(pipelines.voxel_world_columns, [((SIDE * SIDE) as u32).div_ceil(64), jobs, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_candidates, [1, jobs, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_arguments, [1, 1, 1]);
        barrier(
            vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::DRAW_INDIRECT,
            read_write | vk::AccessFlags2::INDIRECT_COMMAND_READ,
        );
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipelines.voxel_world_bricks);
            bindless.push(gpu, cmd, &push);
            device.cmd_dispatch_indirect(cmd, self.counters, (MAX_JOBS as u64 * COUNTERS + 1) * 4);
        }
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_sectors, [self.tall, jobs, 1]);
    }
}

impl Endless {
    /// Lets go of its buffers.
    pub fn destroy(self, gpu: &Gpu) {
        for buffer in self.into_buffers() {
            gpu.destroy_buffer(buffer);
        }
    }

    /// Its buffers, for letting go once no frame in flight reads them.
    pub fn into_buffers(self) -> Vec<Buffer> {
        let mut all = vec![
            self.table,
            self.bricks,
            self.materials,
            self.palette,
            self.empty,
            self.heights,
            self.columns,
            self.slots,
            self.loose,
            self.counters,
            self.candidates,
        ];
        all.extend(self.makes);
        all
    }
}
