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
//!
//! Its ground is its own (rolling hills with grass and flowers), or a world
//! generator's ([`VoxelGenerator`]): the generator's graph compiled to a
//! program the GPU runs a voxel column at a time (`shaders/generator.slang`).
//! A graph with a node the GPU cannot run makes the volume's own ground,
//! with a warning naming the node.

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Bindless, Buffer, Gpu, MemoryLocation, vk};
use voxear_terrain::generate::{GpuProgram, GpuVolume, WorldGen};

use super::{
    GROUND_COLORS, GpuVoxelVolume, MOST_VOXELS_A_METRE, VoxelGenerator, VoxelGround, VoxelTrees, VoxelVolume, trees,
};
use crate::pipelines::Pipelines;

/// How many columns are made a frame at most, at most with a Volume, whose
/// voxels each run its graph, and at most with trees.
pub(crate) const MAX_JOBS: usize = 16;
const MAX_VOLUME_JOBS: usize = 4;
const MAX_TREE_JOBS: usize = 8;
/// How many bricks a column's slot has room for, and material bytes a brick
/// on average. A column of grass and flowers on hills keeps 150 to 200
/// bricks, of 120 to 160 bytes. One with overhangs and caves keeps up to 800
/// of the deep column's bricks, and those far fewer bytes, 90 at most, as
/// what a brick keeps of a cave's wall is a skin.
const BRICK_ROOM: u32 = 384;
const MATERIALS_A_BRICK: u32 = 160;
const VOLUME_BRICK_ROOM: u32 = 1024;
const VOLUME_MATERIALS_A_BRICK: u32 = 64;
/// How many more bricks a column keeps for each tree to a hectare, and
/// material bytes each: a wood of 100 oaks and pines a hectare keeps 430
/// bricks a column at the median and up to 1,700, of 90 to 110 bytes, as a
/// crown's thin shell of leaves crosses many bricks.
const BRICKS_A_TREE_A_HECTARE: f32 = 14.0;
const TREE_MATERIALS_A_BRICK: u32 = 112;
/// The most numbers a Volume carries up a column, and the most spans.
const MOST_CARRIED: usize = 8;
const MOST_SPANS: usize = 4;
const ENTRY_WORDS: u64 = 28;
const BRICK_BYTES: u64 = 80;
/// How many voxel columns a column's heights and its tops are kept for a
/// side: a border of four, and of two.
const SIDE: u64 = 72;
const TOPS: u64 = 68;
const COUNTERS: u64 = 4;
/// The most trees placed round a column, and the numbers each takes.
const MAX_TREES: u64 = 32;
const TREE_WORDS: u64 = 8;
/// The most voxel columns a window is across: 256 sector columns.
const LARGEST_WINDOW: u32 = 256;

/// The colours of the ground a generator makes, as sRGB, in the palette
/// after the volume's own ground's: rock in three shades, sand, snow and
/// gravel in two.
pub(crate) const GENERATED_COLORS: [[u8; 4]; 9] = [
    [118, 117, 112, 255], // 13 rock
    [132, 130, 125, 255],
    [100, 99, 96, 255],
    [214, 196, 148, 255], // 16 sand
    [198, 178, 128, 255],
    [236, 240, 246, 255], // 18 snow
    [218, 226, 236, 255],
    [128, 120, 108, 255], // 20 gravel
    [104, 98, 90, 255],
];

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
    program: u64,
    grounds: u64,
    volume: u64,
    carried: u64,
    forest: u64,
    trees: u64,
    tree_carried: u64,
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
    program_words: u32,
    outs: [u32; 4],
    floor: f32,
    at: [f32; 2],
    sea_level: f32,
    volume_words: u32,
    density_slot: u32,
    carried_count: u32,
    span_count: u32,
    carried_slots: [u32; MOST_CARRIED],
    spans: [u32; MOST_SPANS],
    reach: f32,
    depth: f32,
    tree_kinds: u32,
    tree_reach: f32,
    pad: u32,
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
    /// What each column's climate is, as the columns pass found it.
    grounds: Buffer,
    /// Its generator's program, where it has one the GPU can run, and its
    /// Volume's cells' program and what each column carries up, where it
    /// has one.
    program: Option<(Buffer, GpuProgram)>,
    volume: Option<(Buffer, Buffer)>,
    /// Its trees' kinds and plants, where it has trees, with how many kinds
    /// and how far the furthest reaches across from its trunk, in metres;
    /// the trees placed round each column being made, and room for what the
    /// column under each one's foot carries up.
    forest: Option<(Buffer, u32, f32)>,
    trees: Buffer,
    tree_carried: Buffer,
    /// How many bricks and material bytes a column's slot has room for.
    room: Room,
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
/// it is drawn from and how its voxels shine.
pub(crate) fn same_ground(a: &VoxelVolume, b: &VoxelVolume) -> bool {
    a.voxels_per_metre == b.voxels_per_metre
        && a.reach == b.reach
        && a.size.y == b.size.y
        && a.ground == b.ground
        && a.generator == b.generator
        && a.trees == b.trees
}

/// The program `generator` runs on the GPU, if its graph can run there:
/// else a warning naming the nodes that cannot.
fn program_of(generator: &VoxelGenerator) -> Option<GpuProgram> {
    match generator.world.gpu_program() {
        Ok(program) => {
            if !program.left_out.is_empty() {
                tracing::info!(
                    "a Voxel Volume's generator runs on the GPU without its {}",
                    program.left_out.join(" or ")
                );
            }
            Some(program)
        }
        Err(names) => {
            tracing::warn!(
                "a Voxel Volume's generator cannot run on the GPU ({}): it makes its own ground",
                names.join(", ")
            );
            None
        }
    }
}

/// Fills a record's Volume fields from `cells`, at `voxel` metres a voxel.
fn volume_fields(make: &mut GpuWorldMake, cells: &GpuVolume, voxel: f32) {
    make.volume_words = cells.words.len() as u32;
    make.density_slot = cells.density;
    make.carried_count = cells.carried.len() as u32;
    make.span_count = cells.spans.len().min(MOST_SPANS) as u32;
    for (slot, &carried) in make.carried_slots.iter_mut().zip(&cells.carried) {
        *slot = carried;
    }
    for (place, [foot, top]) in make.spans.iter_mut().zip(&cells.spans) {
        *place = foot | (top << 8);
    }
    // In voxels, held well within what a whole number holds: without end is as far as there is.
    let voxels = |metres: f32| if metres.is_finite() { (metres / voxel).clamp(0.0, 1.0e6) } else { 1.0e6 };
    make.reach = voxels(cells.reach);
    make.depth = voxels(cells.depth);
}

/// A program's words in a buffer the GPU reads.
fn program_buffer(gpu: &Gpu, program: &GpuProgram) -> Result<Buffer> {
    let buffer = gpu.create_buffer(
        "voxel world program",
        (program.words.len() as u64 * 4).max(16),
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::GpuOnly,
    )?;
    gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&program.words))?;
    Ok(buffer)
}

/// How many sector columns an endless volume's window is a side, and how
/// many sectors tall each column is.
fn window_of(volume: &VoxelVolume) -> (u32, u32) {
    let metres = sector_metres(volume);
    let window = (((volume.reach.max(metres) * 2.0) / metres).ceil() as u32 + 1).clamp(2, LARGEST_WINDOW);
    let tall = ((volume.size.y.max(metres) / metres).ceil() as u32).clamp(1, 16);
    (window, tall)
}

/// The program a volume's generator runs on the GPU, if it has one the GPU
/// can run, its Volume left out where it carries up more than there is
/// room for.
fn gpu_program_of(volume: &VoxelVolume) -> Option<GpuProgram> {
    let mut program = volume.generator.as_ref().and_then(program_of);
    if let Some(made) = program.as_mut().filter(|made| {
        made.volume.as_ref().is_some_and(|v| v.carried.len() > MOST_CARRIED || v.spans.len() > MOST_SPANS)
    }) {
        tracing::warn!("a Voxel Volume's generator carries up too much for the GPU: its Volume is left out");
        made.volume = None;
    }
    program
}

/// How many bricks and material bytes a column's slot has room for.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Room {
    bricks: u32,
    materials: u32,
}

/// The room a column's slot has, with a Volume or without, and with as
/// many trees to a hectare as `trees`.
fn room_of(program: Option<&GpuProgram>, trees: &[VoxelTrees]) -> Room {
    let (bricks, a_brick) = match program.is_some_and(|program| program.volume.is_some()) {
        true => (VOLUME_BRICK_ROOM, VOLUME_MATERIALS_A_BRICK),
        false => (BRICK_ROOM, MATERIALS_A_BRICK),
    };
    let per_hectare: f32 = trees.iter().map(|kind| kind.per_hectare.max(0.0)).sum();
    // In steps of 512, so dragging how many trees there are makes the buffers again only now and then.
    let more = ((per_hectare * BRICKS_A_TREE_A_HECTARE / 512.0).ceil() as u32 * 512).min(4096);
    Room { bricks: bricks + more, materials: bricks * a_brick + more * TREE_MATERIALS_A_BRICK }
}

/// An endless volume's palette, its own ground's colours, a generator's
/// and its trees', and its trees as the GPU reads them, where it has any.
#[allow(clippy::type_complexity)]
fn forest_buffers(gpu: &Gpu, volume: &VoxelVolume) -> Result<(Buffer, Option<(Buffer, u32, f32)>)> {
    let first = (GROUND_COLORS.len() + GENERATED_COLORS.len()) as u32;
    let forest = trees::forest(&volume.trees, first);
    let tree_colors = forest.as_ref().map(|forest| forest.colors.clone()).unwrap_or_default();
    let all = GROUND_COLORS.iter().chain(&GENERATED_COLORS).chain(&tree_colors);
    let colours: Vec<u32> = all.map(|c| u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16).collect();
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER
        | vk::BufferUsageFlags::TRANSFER_DST
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let palette = gpu.create_buffer("voxel world palette", colours.len() as u64 * 4, usage, MemoryLocation::GpuOnly)?;
    gpu.upload_buffer(&palette, 0, bytemuck::cast_slice(&colours))?;
    let forest = match forest {
        Some(forest) => {
            let size = forest.words.len() as u64 * 4;
            let buffer = gpu.create_buffer("voxel world forest", size, usage, MemoryLocation::GpuOnly)?;
            gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&forest.words))?;
            Some((buffer, forest.words[0], forest.reach))
        }
        None => None,
    };
    Ok((palette, forest))
}

/// A program's buffer, and its Volume's cells' program and what each column
/// carries up to them, where it has a Volume.
#[allow(clippy::type_complexity)]
fn program_buffers(
    gpu: &Gpu,
    program: Option<GpuProgram>,
) -> Result<(Option<(Buffer, GpuProgram)>, Option<(Buffer, Buffer)>)> {
    let caves = match program.as_ref().and_then(|made| made.volume.as_ref()) {
        Some(cells) => {
            let words = program_buffer(gpu, &GpuProgram { words: cells.words.clone(), ..GpuProgram::default() })?;
            let count = (cells.carried.len() as u64).max(1);
            let carried = gpu.create_buffer(
                "voxel world carried",
                MAX_JOBS as u64 * SIDE * SIDE * count * 4,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
                MemoryLocation::GpuOnly,
            )?;
            Some((words, carried))
        }
        None => None,
    };
    let program = match program {
        Some(program) => Some((program_buffer(gpu, &program)?, program)),
        None => None,
    };
    Ok((program, caves))
}

impl Endless {
    /// The buffers of an endless volume, every slot holding no column yet.
    pub fn new(gpu: &Gpu, volume: &VoxelVolume) -> Result<Self> {
        let (window, tall) = window_of(volume);
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
        let (palette, forest) = forest_buffers(gpu, volume)?;
        let trees = make("trees", jobs * MAX_TREES * TREE_WORDS * 4, none)?;
        let tree_carried = make("tree carried", jobs * 64 * MOST_CARRIED as u64 * 4, none)?;
        let empty = make("empty sector", 25 * 4, none)?;
        let mut nothing = [0u32; 25];
        nothing[24] = u32::MAX;
        gpu.upload_buffer(&empty, 0, bytemuck::cast_slice(&nothing))?;
        let heights = make("heights", jobs * SIDE * SIDE * 4, none)?;
        let columns = make("columns", jobs * TOPS * TOPS * 4, none)?;
        let grounds = make("grounds", jobs * SIDE * SIDE * 4, none)?;
        let program = gpu_program_of(volume);
        let (program, caves) = program_buffers(gpu, program)?;
        let room = room_of(program.as_ref().map(|(_, program)| program), &volume.trees);
        let bricks = make("bricks", slots_in * u64::from(room.bricks) * BRICK_BYTES, none)?;
        let materials = make("materials", slots_in * u64::from(room.materials), none)?;
        let slots = make("slots", jobs * 64 * rows * 4, none)?;
        let loose = make("loose bricks", jobs * u64::from(room.bricks) * BRICK_BYTES, none)?;
        let counters = make("counters", (jobs * COUNTERS + 8 + jobs) * 4, vk::BufferUsageFlags::INDIRECT_BUFFER)?;
        // The candidate bricks, and after them those the coarse pass leaves.
        let candidates = make("candidates", 2 * jobs * 64 * rows * 4, none)?;
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
            grounds,
            program,
            volume: caves,
            forest,
            trees,
            tree_carried,
            room,
            makes,
            made: vec![None; (window * window) as usize],
            jobs: Vec::new(),
            window_sector: [0; 3],
            corner: Vec3::ZERO,
        })
    }

    /// Makes `volume`'s ground in the buffers it has, where they fit it: its
    /// generator's program's buffers made again and every column made over,
    /// each slot showing what it held until then. Gives back the buffers let
    /// go, or `None` where `volume` needs buffers of other sizes, as a
    /// further reach does. Dragging a field of the volume in the Inspector
    /// makes its ground again each frame, without its hundreds of megabytes
    /// made again too.
    pub fn refit(&mut self, gpu: &Gpu, volume: &VoxelVolume) -> Result<Option<Vec<Buffer>>> {
        if window_of(volume) != (self.window, self.tall) {
            return Ok(None);
        }
        let program = gpu_program_of(volume);
        if room_of(program.as_ref(), &volume.trees) != self.room {
            return Ok(None);
        }
        let (program, caves) = program_buffers(gpu, program)?;
        let (palette, forest) = forest_buffers(gpu, volume)?;
        let mut gone: Vec<Buffer> =
            std::mem::replace(&mut self.program, program).map(|(buffer, _)| buffer).into_iter().collect();
        gone.extend(
            std::mem::replace(&mut self.volume, caves).into_iter().flat_map(|(words, carried)| [words, carried]),
        );
        gone.push(std::mem::replace(&mut self.palette, palette));
        gone.extend(std::mem::replace(&mut self.forest, forest).map(|(buffer, _, _)| buffer));
        self.from = volume.clone();
        self.made.iter_mut().for_each(|made| *made = None);
        Ok(Some(gone))
    }

    /// Its size on the GPU, in bytes.
    pub fn bytes(&self) -> u64 {
        [
            &self.table,
            &self.bricks,
            &self.materials,
            &self.heights,
            &self.columns,
            &self.slots,
            &self.loose,
            &self.grounds,
        ]
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
        let most = match (self.volume.is_some(), self.forest.is_some()) {
            (true, _) => MAX_VOLUME_JOBS,
            (false, true) => MAX_TREE_JOBS,
            (false, false) => MAX_JOBS,
        };
        for (_, column) in wanted.into_iter().take(most) {
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
        let generator = self.from.generator.as_ref();
        let no_program = GpuProgram::default();
        let (program, words) = match &self.program {
            Some((buffer, program)) => (buffer.address, program),
            None => (0, &no_program),
        };
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
            program,
            grounds: self.grounds.address,
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
            brick_room: self.room.bricks,
            material_room: self.room.materials,
            job_count: self.jobs.len() as u32,
            candidate_room: (MAX_JOBS as u64 * 64 * rows) as u32,
            program_words: if program == 0 { 0 } else { words.words.len() as u32 },
            outs: [words.height, words.warmth, words.wetness, words.rock],
            floor: generator.map_or(0.0, |generator| generator.floor),
            at: generator.map_or([0.0; 2], |generator| generator.at),
            sea_level: generator.map_or(0.0, |generator| generator.world.sea_level()),
            volume: self.volume.as_ref().map_or(0, |(words, _)| words.address),
            carried: self.volume.as_ref().map_or(0, |(_, carried)| carried.address),
            volume_words: 0,
            density_slot: 0,
            carried_count: 0,
            span_count: 0,
            carried_slots: [0; MOST_CARRIED],
            spans: [0; MOST_SPANS],
            reach: 0.0,
            depth: 0.0,
            forest: self.forest.as_ref().map_or(0, |(buffer, _, _)| buffer.address),
            trees: self.trees.address,
            tree_carried: self.tree_carried.address,
            tree_kinds: self.forest.as_ref().map_or(0, |(_, kinds, _)| *kinds),
            tree_reach: self.forest.as_ref().map_or(0.0, |(_, _, reach)| *reach),
            pad: 0,
            jobs,
        };
        let mut make = make;
        let cells = self.program.as_ref().and_then(|(_, program)| program.volume.as_ref());
        if let (Some(cells), Some(_)) = (cells, &self.volume) {
            let voxel = make.voxel;
            volume_fields(&mut make, cells, voxel);
        }
        let buffer = &self.makes[slot.min(self.makes.len() - 1)];
        buffer.write(0, std::slice::from_ref(&make));
        Some(buffer.address)
    }

    /// What recording this frame's passes needs, apart from the volume.
    pub fn recording(&self) -> Recording {
        Recording {
            material_room: self.room.materials,
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
    material_room: u32,
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
                let room = u64::from(self.material_room);
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
        run(pipelines.voxel_world_tops, [((TOPS * TOPS) as u32).div_ceil(64), jobs, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_trees, [1, jobs, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_candidates, [1, jobs, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_arguments, [1, 1, 1]);
        barrier(
            vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::DRAW_INDIRECT,
            read_write | vk::AccessFlags2::INDIRECT_COMMAND_READ,
        );
        let indirect = |pipeline: vk::Pipeline, at: u64| unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            bindless.push(gpu, cmd, &push);
            device.cmd_dispatch_indirect(cmd, self.counters, (MAX_JOBS as u64 * COUNTERS + at) * 4);
        };
        indirect(pipelines.voxel_world_coarse, 1);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_brick_arguments, [1, 1, 1]);
        barrier(
            vk::PipelineStageFlags2::COMPUTE_SHADER | vk::PipelineStageFlags2::DRAW_INDIRECT,
            read_write | vk::AccessFlags2::INDIRECT_COMMAND_READ,
        );
        indirect(pipelines.voxel_world_bricks, 5);
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
            self.grounds,
            self.trees,
            self.tree_carried,
        ];
        all.extend(self.forest.map(|(buffer, _, _)| buffer));
        all.extend(self.program.map(|(buffer, _)| buffer));
        all.extend(self.volume.into_iter().flat_map(|(words, carried)| [words, carried]));
        all.extend(self.makes);
        all
    }
}

/// What `world` makes at each of `places`, `[x, y, z]` in metres in its
/// world, run on the GPU as an endless volume's columns and voxels run it:
/// the ground's height in metres, how warm it is, how wet, its rock mask,
/// and how far into its Volume's ground a cell `size` metres across at the
/// place is (nothing where it has no Volume on the GPU). For checking the
/// GPU against the CPU.
pub(crate) fn generated_at(
    gpu: &Gpu,
    (pipelines, bindless): (&Pipelines, &Bindless),
    world: &WorldGen,
    places: &[[f32; 3]],
    size: f32,
) -> Result<Vec<[f32; 5]>> {
    let program = world.gpu_program().map_err(|names| anyhow::anyhow!("not on the GPU: {}", names.join(", ")))?;
    let code = program_buffer(gpu, &program)?;
    let cells = program.volume.as_ref().map(|cells| GpuProgram { words: cells.words.clone(), ..GpuProgram::default() });
    let cells = cells.map(|cells| program_buffer(gpu, &cells)).transpose()?;
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let count = places.len().max(1) as u64;
    let asked = gpu.create_buffer("generator places", count * 12, usage, MemoryLocation::CpuToGpu)?;
    asked.write(0, places);
    let made = gpu.create_buffer("generator made", count * 64, usage, MemoryLocation::GpuToCpu)?;
    let record =
        gpu.create_buffer("generator record", size_of::<GpuWorldMake>() as u64, usage, MemoryLocation::CpuToGpu)?;
    let mut make: GpuWorldMake = bytemuck::Zeroable::zeroed();
    make.program = code.address;
    make.program_words = program.words.len() as u32;
    make.outs = [program.height, program.warmth, program.wetness, program.rock];
    make.candidates = asked.address;
    make.loose = made.address;
    make.candidate_room = places.len() as u32;
    make.voxel = size;
    if let (Some(volume), Some(words)) = (&program.volume, &cells) {
        make.volume = words.address;
        volume_fields(&mut make, volume, size);
    }
    record.write(0, std::slice::from_ref(&make));
    let push = WorldPush { make: record.address };
    gpu.immediate(|cmd| unsafe {
        gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipelines.voxel_world_places);
        bindless.push(gpu, cmd, &push);
        gpu.device.cmd_dispatch(cmd, (places.len() as u32).div_ceil(64), 1, 1);
    })?;
    let words: &[f32] = bytemuck::cast_slice(&made.read()[..places.len() * 64]);
    let out = words.chunks(16).map(|place| [place[0], place[1], place[2], place[3], place[4]]).collect();
    for buffer in [code, asked, made, record].into_iter().chain(cells) {
        gpu.destroy_buffer(buffer);
    }
    Ok(out)
}
