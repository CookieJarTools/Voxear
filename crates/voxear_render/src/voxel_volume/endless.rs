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

use std::sync::Arc;

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Bindless, Buffer, Gpu, MemoryLocation, vk};
use voxear_terrain::generate::{GpuProgram, GpuVolume, WorldGen};

use super::{
    GROUND_COLORS, GpuVoxelVolume, MOST_VOXELS_A_METRE, PAVING_COLORS, VoxelBiomes, VoxelChanges, VoxelEdit,
    VoxelGenerator, VoxelGround, VoxelMaterial, VoxelPrint, VoxelShape, VoxelTrees, VoxelVolume, edit,
    growth::{self, Growth},
    VoxelWaterLook, print,
    rivers::{self, ColumnWater, GpuVoxelWater, LaidRivers},
    trees,
};
use crate::pipelines::Pipelines;

/// How many columns are made a frame at most, at most with a Volume, whose
/// voxels each run its graph, and at most with trees.
pub(crate) const MAX_JOBS: usize = 16;
const MAX_VOLUME_JOBS: usize = 4;
const MAX_TREE_JOBS: usize = 8;
/// How many bricks a job keeps while its column is made, and how many
/// material bytes: a column of a wood keeps up to 1,700 bricks of about 110
/// bytes each, one with caves up to 800 of 40.
const JOB_BRICKS: u64 = 4096;
const JOB_MATERIALS: u64 = 512 * 1024;
/// The shared room every column's bricks and materials are kept in: pages
/// of 64 bricks, at most 64 a slot, and pages of 4,096 material bytes, at
/// most 128 a slot (`shaders/voxel_world.slang`).
const PAGE_BRICKS: u64 = 64;
const SLOT_PAGES: u64 = 64;
const MATERIAL_PAGE: u64 = 4096;
const SLOT_MATERIAL_PAGES: u64 = 128;
/// What a column keeps on average, which its shared room is sized by, with
/// a quarter more: measured over a thousand columns of each, grass and
/// flowers on hills keep 170 bricks and 20 KB of materials; overhangs and
/// caves 100 and 4.5 KB. A tree's bricks are its plant's, a word each in
/// the column, but where trees cross or stand in the grass the column keeps
/// bricks of its own: a wood keeps 0.6 more bricks and 44 more bytes for
/// each tree to a hectare. Where the room runs short it is made again half
/// as large again.
const AVERAGE_BRICKS: f64 = 200.0;
const AVERAGE_VOLUME_BRICKS: f64 = 150.0;
const AVERAGE_BRICKS_A_TREE: f64 = 0.7;
const AVERAGE_MATERIALS: f64 = 24_000.0;
const AVERAGE_VOLUME_MATERIALS: f64 = 8_000.0;
const AVERAGE_MATERIALS_A_TREE: f64 = 50.0;
const ROOM_SPARE: f64 = 1.25;
/// The most numbers a Volume carries up a column, and the most spans.
const MOST_CARRIED: usize = 8;
const MOST_SPANS: usize = 4;
const ENTRY_WORDS: u64 = 53;
const BRICK_BYTES: u64 = 80;
/// How many voxel columns a column's heights and its tops are kept for a
/// side: a border of four, and of two.
const SIDE: u64 = 72;
const TOPS: u64 = 68;
const COUNTERS: u64 = 4;
/// The most trees placed round a column, and the numbers each takes.
const MAX_TREES: u64 = 32;
const TREE_WORDS: u64 = 8;
/// The most edits that reach a column being made and the trees round it
/// (`MOST_COLUMN_EDITS` in `shaders/voxel_world.slang`).
const MOST_COLUMN_EDITS: u64 = 4096;
/// The most voxel columns a window is across: 256 sector columns.
const LARGEST_WINDOW: u32 = 256;
/// The words an edit takes as the GPU reads it (`EDIT_WORDS` in
/// `shaders/voxel_world.slang`), its kinds, and its marks.
const EDIT_WORDS: usize = 32;
const EDIT_BALL: u32 = 0;
const EDIT_BOX: u32 = 1;
const EDIT_RAMP: u32 = 2;
const EDIT_AREA: u32 = 3;
const EDIT_COVER: u32 = 4;
const EDIT_PRINT: u32 = 5;
const EDIT_VOXELS: u32 = 6;
const EDIT_FIT: u32 = 1 << 16;
const EDIT_WIDENED: u32 = 1 << 17;
/// Where the prints' records start in their words, and the words each takes.
const PRINTS_AT: usize = 4;
const PRINT_WORDS: usize = 13;
/// The most colours a volume's palette holds. A material is a byte: below
/// [`SHARED_COLOURS`] a place every brick shares (the ground's, the pavings'
/// and the trees'), above it a place in the brick's own bank of 128, the
/// first bank the palette's places 128 to 255, the next 256 to 383, and so on
/// (`voxelColour` in `shaders/voxels.slang`).
pub(crate) const MOST_COLOURS: usize = 256 + 31 * BANK;
const SHARED_COLOURS: usize = 128;
const BANK: usize = 128;

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

/// The colours of its rivers' water as voxels, as sRGB, in the palette after
/// the pavings': from as shallow as the sand under it shows through to deep,
/// in eight shades, then churned white where a waterfall lands, in two.
pub(crate) const WATER_COLORS: [[u8; 4]; 10] = [
    [150, 160, 118, 255], // shallow
    [118, 146, 112, 255],
    [88, 130, 106, 255],
    [64, 114, 102, 255],
    [46, 98, 96, 255],
    [34, 84, 88, 255],
    [26, 70, 78, 255],
    [20, 58, 68, 255], // deep
    [214, 228, 228, 255], // churned
    [168, 200, 200, 255],
];

/// Where the water's shades are in the palette, after the pavings', and how
/// many with the churned ones (`WATER_VOXEL`, and `WATER_SHADES` and two,
/// in `shaders/voxel_world.slang`, as materials, one more).
const WATER_COLOUR: u32 = (GROUND_COLORS.len() + GENERATED_COLORS.len() + PAVING_COLORS.len()) as u32;
const WATER_SHADES: u32 = WATER_COLORS.len() as u32;

/// How many colours the ground, a generator's ground, the pavings and the
/// water of voxels take at the start of the palette, which tinted fills are
/// tinted from.
const BASE_COLOURS: usize = GROUND_COLORS.len() + GENERATED_COLORS.len() + PAVING_COLORS.len() + WATER_COLORS.len();

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
    pages: u64,
    material_pages: u64,
    slot_counts: u64,
    free_pages: u64,
    pool: u64,
    staging: u64,
    placed: u64,
    models: u64,
    edits: u64,
    job_edits: u64,
    prints: u64,
    biomes: u64,
    column_grounds: u64,
    rivers: u64,
    growth: u64,
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
    brick_pages: u32,
    edit_count: u32,
    edit_pad: u32,
    river_window: u32,
    water_voxels: u32,
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
    /// What each column of a job is made of where it is a biome's ground,
    /// beside its word in `columns` (`columnGrounds`).
    column_grounds: Buffer,
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
    /// Its generator's biomes as the GPU reads them, where it has any.
    biomes: Option<Buffer>,
    /// What grows where: the areas its trees and cover keep to, and its
    /// cover's kinds.
    growth: Buffer,
    trees: Buffer,
    tree_carried: Buffer,
    /// Its trees' plants as voxels in each of their turns, where it has
    /// trees: their records and sectors, their bricks and materials being
    /// kept at the start of the volume's own, in pages no column is given.
    models: Option<Buffer>,
    /// The edits its columns are made with, as the GPU reads them, with room
    /// for more; and the edits that reach each column being made.
    edits: Arc<Vec<VoxelEdit>>,
    changes: Arc<VoxelChanges>,
    edit_total: u32,
    edit_buffer: Option<Buffer>,
    job_edits: Buffer,
    /// Every print its edits have printed, in the order first printed;
    /// their voxels as the GPU reads them; the places in the palette of
    /// each print's colours in each tint it is printed in, and of each
    /// paving's or ground's tinted; and the palette's colours before
    /// theirs. Prints and colours are only ever added, so the columns made
    /// keep theirs.
    prints: Vec<Arc<VoxelPrint>>,
    tables: std::collections::HashMap<Tinted, Vec<u32>>,
    print_buffer: Option<Buffer>,
    colours: Vec<[u8; 4]>,
    base_colours: usize,
    /// The shared room: how many pages of bricks and of materials it has,
    /// each slot's pages of each and how many, the pages free, how many are
    /// free and whether any ran short (read on the CPU), and each job's
    /// materials and where they are placed while its column is made.
    room: Room,
    grow: f64,
    pages: Buffer,
    material_pages: Buffer,
    slot_counts: Buffer,
    free_pages: Buffer,
    pool: Buffer,
    staging: Buffer,
    placed: Buffer,
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
    /// Its generator's rivers, where it runs any; whether columns are
    /// waiting on their points; and the water over each slot's column, with
    /// a count of its changes.
    rivers: Option<LaidRivers>,
    waiting: bool,
    water: Vec<ColumnWater>,
    water_changes: u64,
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
        && a.water == b.water
        && a.cover == b.cover
        && (Arc::ptr_eq(&a.areas, &b.areas) || a.areas == b.areas)
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

/// A program's words in a buffer the GPU reads, what its steps read beside
/// their own numbers (painted maps) after them.
fn program_buffer(gpu: &Gpu, program: &GpuProgram) -> Result<Buffer> {
    let words = [program.words.as_slice(), program.data.as_slice()].concat();
    let buffer = gpu.create_buffer(
        "voxel world program",
        (words.len() as u64 * 4).max(16),
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::GpuOnly,
    )?;
    for (i, piece) in bytemuck::cast_slice::<u32, u8>(&words).chunks(64 << 20).enumerate() {
        gpu.upload_buffer(&buffer, (i * (64 << 20)) as u64, piece)?;
    }
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

/// How many pages of bricks and of materials the shared room has.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Room {
    brick_pages: u32,
    material_pages: u32,
}

/// The shared room of a volume of `slots` columns, with a Volume or
/// without and with `trees`, `grow` times what columns keep on average.
fn room_of(program: Option<&GpuProgram>, trees: &[VoxelTrees], slots: u32, grow: f64) -> Room {
    let volume = program.is_some_and(|program| program.volume.is_some());
    let per_hectare: f64 = trees.iter().map(|kind| f64::from(kind.per_hectare.max(0.0))).sum();
    let (bricks, materials) = match volume {
        true => (AVERAGE_VOLUME_BRICKS, AVERAGE_VOLUME_MATERIALS),
        false => (AVERAGE_BRICKS, AVERAGE_MATERIALS),
    };
    let bricks = bricks + per_hectare * AVERAGE_BRICKS_A_TREE;
    // Half a page of each a column on top, the part of its last page it leaves.
    let materials = materials + per_hectare * AVERAGE_MATERIALS_A_TREE + MATERIAL_PAGE as f64 / 2.0;
    let columns = f64::from(slots) * ROOM_SPARE * grow;
    let jobs = MAX_JOBS as f64;
    let brick_pages = columns * (bricks / PAGE_BRICKS as f64 + 0.5) + jobs * SLOT_PAGES as f64;
    let material_pages = columns * materials / MATERIAL_PAGE as f64 + jobs * SLOT_MATERIAL_PAGES as f64;
    Room { brick_pages: brick_pages.ceil() as u32, material_pages: material_pages.ceil() as u32 }
}

/// A palette's colours in a buffer the GPU reads: each colour's sRGB, and
/// over it, for a colour of a bank, the shared place nearest it, which a
/// voxel of it takes in a brick of another bank.
fn palette_buffer(gpu: &Gpu, colours: &[[u8; 4]]) -> Result<Buffer> {
    let mut words = palette_words(colours);
    super::add_kinds(&mut words, colours);
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER
        | vk::BufferUsageFlags::TRANSFER_DST
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let palette = gpu.create_buffer("voxel world palette", (words.len() as u64 * 4).max(16), usage, MemoryLocation::GpuOnly)?;
    gpu.upload_buffer(&palette, 0, bytemuck::cast_slice(&words))?;
    Ok(palette)
}

fn palette_words(colours: &[[u8; 4]]) -> Vec<u32> {
    let shared = &colours[..colours.len().min(SHARED_COLOURS)];
    colours
        .iter()
        .enumerate()
        .map(|(at, c)| {
            let rgb = u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16;
            let nearest = if at < SHARED_COLOURS { 0 } else { nearest_of(shared, *c) as u32 };
            rgb | nearest << 24
        })
        .collect()
}

/// The place in `colours` of the one nearest `colour`.
fn nearest_of(colours: &[[u8; 4]], colour: [u8; 4]) -> usize {
    let distance = |c: &[u8; 4]| (0..3).map(|i| (i32::from(c[i]) - i32::from(colour[i])).pow(2)).sum::<i32>();
    colours.iter().enumerate().min_by_key(|(_, c)| distance(c)).map_or(0, |(i, _)| i)
}

/// The place of `colour` among the palette's first colours, which every
/// brick shares unless there are more than a bank's worth (a biome's
/// blocks'), added where it is new, else the nearest there is: for the
/// colours the ground is made of, before any print's.
fn place_in(palette: &mut Vec<[u8; 4]>, colour: [u8; 4]) -> u8 {
    if let Some(at) = palette.iter().take(256).position(|c| c[..3] == colour[..3]) {
        return at as u8;
    }
    if palette.len() < 256 {
        palette.push(colour);
        return (palette.len() - 1) as u8;
    }
    nearest_of(&palette[..256], colour) as u8
}

/// The bank a palette place is in: none for a shared one.
fn bank_of(place: usize) -> Option<usize> {
    match place {
        p if p < SHARED_COLOURS => None,
        p if p < SHARED_COLOURS + BANK => Some(0),
        p => Some((p - SHARED_COLOURS - BANK) / BANK + 1),
    }
}

/// Where bank `bank`'s places start in the palette.
fn bank_start(bank: usize) -> usize {
    if bank == 0 { SHARED_COLOURS } else { SHARED_COLOURS + BANK * bank }
}

/// The places in `palette` of a table's colours `wanted` (a print's in a
/// tint, or a fill's tinted): each shared place a colour is, else all in one
/// bank where they fit, the palette's last or a new one, so a brick of one
/// building has every colour of it. With every bank full, the nearest.
fn place_table(palette: &mut Vec<[u8; 4]>, wanted: &[[u8; 4]]) -> Vec<u32> {
    // The same colour and kind: glass or a light keeps a place of its own beside a solid colour like it.
    let same = |a: &[u8; 4], b: &[u8; 4]| a == b;
    let shared = palette.len().min(SHARED_COLOURS);
    let mut places: Vec<Option<usize>> =
        wanted.iter().map(|colour| palette[..shared].iter().position(|c| same(c, colour))).collect();
    // The colours of it not shared, once each, in as few banks as they take.
    let mut left: Vec<[u8; 4]> = Vec::new();
    for (colour, place) in wanted.iter().zip(&places) {
        if place.is_none() && !left.iter().any(|c| same(c, colour)) {
            left.push(*colour);
        }
    }
    for chunk in left.chunks(BANK) {
        // The palette's last bank, if it has them all already or room for those it hasn't; else the next.
        let last = bank_of(palette.len().saturating_sub(1).max(SHARED_COLOURS)).unwrap_or(0);
        let within = |palette: &Vec<[u8; 4]>, bank: usize| {
            let start = bank_start(bank).min(palette.len());
            let end = (bank_start(bank) + BANK).min(palette.len());
            (start, end)
        };
        let (start, end) = within(palette, last);
        let missing = chunk.iter().filter(|colour| !palette[start..end].iter().any(|c| same(c, colour))).count();
        let room = (bank_start(last) + BANK).saturating_sub(palette.len().max(bank_start(last)));
        let bank = if missing <= room {
            Some(last)
        } else if last + 1 < (MOST_COLOURS - SHARED_COLOURS) / BANK {
            Some(last + 1)
        } else {
            None
        };
        let Some(bank) = bank else {
            // Every bank full: the nearest the palette has.
            for (colour, place) in wanted.iter().zip(places.iter_mut()) {
                if place.is_none() && chunk.iter().any(|c| same(c, colour)) {
                    *place = Some(nearest_of(palette, *colour));
                }
            }
            continue;
        };
        // The palette filled out to the bank's start, with the colour before.
        while palette.len() < bank_start(bank) {
            let filler = palette.last().copied().unwrap_or([0, 0, 0, 255]);
            palette.push(filler);
        }
        for colour in chunk {
            let (start, end) = within(palette, bank);
            let at = match palette[start..end].iter().position(|c| same(c, colour)) {
                Some(at) => start + at,
                None => {
                    palette.push(*colour);
                    palette.len() - 1
                }
            };
            for (wanted, place) in wanted.iter().zip(places.iter_mut()) {
                if place.is_none() && same(wanted, colour) {
                    *place = Some(at);
                }
            }
        }
    }
    places.into_iter().map(|place| place.unwrap_or(0) as u32).collect()
}

/// An endless volume's palette's colours, its own ground's, a generator's,
/// its trees' and its biomes' blocks', and its trees as the GPU reads them,
/// where it has any; with their plants' voxels too, with `models`; and its
/// generator's biomes as the GPU reads them, where it has any.
#[allow(clippy::type_complexity)]
fn forest_buffers(
    gpu: &Gpu,
    volume: &VoxelVolume,
    models: bool,
) -> Result<(Vec<[u8; 4]>, Option<(Buffer, u32, f32)>, Option<trees::Models>, Option<Buffer>)> {
    // The pavings' colours after the ground's, where the shaders look for them, then the crops', then the trees'.
    let cover_colors = growth::cover_colors(&volume.cover);
    let first = (BASE_COLOURS + cover_colors.len()) as u32;
    let forest = trees::forest(&volume.trees, first);
    let tree_colors = forest.as_ref().map(|forest| forest.colors.clone()).unwrap_or_default();
    let mut all: Vec<[u8; 4]> = GROUND_COLORS
        .iter()
        .chain(&GENERATED_COLORS)
        .chain(&PAVING_COLORS)
        .chain(&WATER_COLORS)
        .chain(&cover_colors)
        .chain(&tree_colors)
        .copied()
        .collect();
    let biomes = volume.generator.as_ref().and_then(|generator| generator.biomes.as_deref());
    let biomes = match biomes.map(|biomes| biome_words(biomes, &mut all)) {
        Some(words) => {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
            let buffer =
                gpu.create_buffer("voxel world biomes", words.len() as u64 * 4, usage, MemoryLocation::GpuOnly)?;
            for (i, piece) in bytemuck::cast_slice::<u32, u8>(&words).chunks(64 << 20).enumerate() {
                gpu.upload_buffer(&buffer, (i * (64 << 20)) as u64, piece)?;
            }
            Some(buffer)
        }
        None => None,
    };
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER
        | vk::BufferUsageFlags::TRANSFER_DST
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let made = forest.as_ref().filter(|_| models).map(|forest| trees::models(forest, block, &all));
    let forest = match forest {
        Some(forest) => {
            let size = forest.words.len() as u64 * 4;
            let buffer = gpu.create_buffer("voxel world forest", size, usage, MemoryLocation::GpuOnly)?;
            gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&forest.words))?;
            Some((buffer, forest.words[0], forest.reach))
        }
        None => None,
    };
    Ok((all, forest, made, biomes))
}

/// A generator's biomes as the GPU reads them (`biomeOf` in
/// `shaders/voxel_world.slang`), each of their blocks' colours added to
/// `palette` in two shades, its own and a darker: their words, and after
/// them a word for each block, its colour's place in the palette, with a
/// bit over it where grass grows on it, as on a green block. Where the
/// palette is full, its nearest colours.
fn biome_words(biomes: &VoxelBiomes, palette: &mut Vec<[u8; 4]>) -> Vec<u32> {
    let mut words = biomes.words.clone();
    // Where the blocks' places start, after the words.
    if words.len() > 10 {
        words[10] = words.len() as u32;
    }
    for &[r, g, b] in &biomes.colours {
        let place = place_in(palette, [r, g, b, 255]);
        let dark = |c: u8| (f32::from(c) * 0.86).round() as u8;
        let shade = place_in(palette, [dark(r), dark(g), dark(b), 255]);
        let green = g > r.saturating_add(8) && g > b.saturating_add(8);
        words.push(u32::from(place) | u32::from(shade) << 8 | u32::from(green) << 16);
    }
    words
}

/// The words of what grows where on `volume` (see [`growth`]), as the GPU
/// reads them.
fn growth_buffer(gpu: &Gpu, volume: &VoxelVolume) -> Result<Buffer> {
    let voxel = 1.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
    let first = BASE_COLOURS as u32;
    let words = Growth::new(&volume.areas, &volume.trees, &volume.cover, voxel).words(&volume.cover, voxel, first);
    let usage = vk::BufferUsageFlags::STORAGE_BUFFER
        | vk::BufferUsageFlags::TRANSFER_DST
        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
    let buffer = gpu.create_buffer("voxel world growth", words.len() as u64 * 4, usage, MemoryLocation::GpuOnly)?;
    gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&words))?;
    Ok(buffer)
}

/// What a volume's trees' plants' voxels are made from: its kinds that grow,
/// whatever how many, and how many voxels a metre.
fn plants_of(volume: &VoxelVolume) -> (Vec<VoxelTrees>, u32) {
    let kinds = volume.trees.iter().filter(|kind| kind.per_hectare > 0.0).take(trees::MOST_KINDS);
    (kinds.map(|kind| VoxelTrees { per_hectare: 1.0, ..kind.clone() }).collect(), volume.voxels_per_metre)
}

/// How far the trees' feet a column's trees may stand on reach round it,
/// in columns: a tree's crown reaches as far as its kinds' furthest and a
/// block and a half, from a foot anywhere in its cell.
fn tree_ring(volume: &VoxelVolume, forest: Option<&(Buffer, u32, f32)>) -> u32 {
    forest.map_or(0, |(_, _, reach)| ((reach + 1.5 + trees::CELL) / sector_metres(volume)).ceil() as u32)
}

/// The rivers `volume`'s generator lays into its columns, where its program
/// runs on the GPU (`runs`) and its graph runs any, for a window `window`
/// columns a side: its table reaching a column further round, for the
/// heights' border, and as far again as its trees may stand.
fn laid_rivers(
    gpu: &Gpu,
    volume: &VoxelVolume,
    runs: bool,
    window: u32,
    forest: Option<&(Buffer, u32, f32)>,
) -> Result<Option<LaidRivers>> {
    let layer = volume.generator.as_ref().filter(|_| runs).and_then(|g| rivers::layer_of(g, volume.voxels_per_metre));
    let Some(layer) = layer else { return Ok(None) };
    Ok(Some(LaidRivers::new(gpu, layer, window, 1 + tree_ring(volume, forest))?))
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
        Self::grown(gpu, volume, 1.0)
    }

    /// The buffers of an endless volume whose shared room is `grow` times
    /// what its columns keep on average.
    pub fn grown(gpu: &Gpu, volume: &VoxelVolume, grow: f64) -> Result<Self> {
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
        let (colours, forest, made, biomes) = forest_buffers(gpu, volume, true)?;
        let palette = palette_buffer(gpu, &colours)?;
        let growth = growth_buffer(gpu, volume)?;
        let trees = make("trees", jobs * MAX_TREES * TREE_WORDS * 4, none)?;
        let tree_carried = make("tree carried", jobs * 64 * MOST_CARRIED as u64 * 4, none)?;
        let empty = make("empty sector", 25 * 4, none)?;
        let mut nothing = [0u32; 25];
        nothing[24] = u32::MAX;
        gpu.upload_buffer(&empty, 0, bytemuck::cast_slice(&nothing))?;
        let heights = make("heights", jobs * SIDE * SIDE * 4, none)?;
        let columns = make("columns", jobs * TOPS * TOPS * 4, none)?;
        let column_grounds = make("column grounds", jobs * TOPS * TOPS * 4, none)?;
        let grounds = make("grounds", jobs * SIDE * SIDE * 4, none)?;
        let program = gpu_program_of(volume);
        let (program, caves) = program_buffers(gpu, program)?;
        let room = room_of(program.as_ref().map(|(_, program)| program), &volume.trees, window * window, grow);
        // The trees' plants' bricks and materials first, in pages no column is given.
        let (model_bricks, model_materials) =
            made.as_ref().map_or((&[][..], &[][..]), |made| (&made.bricks[..], &made.materials[..]));
        let reserved = [
            (model_bricks.len() as u64 * 4 / BRICK_BYTES).div_ceil(PAGE_BRICKS) as u32,
            (model_materials.len() as u64).div_ceil(MATERIAL_PAGE) as u32,
        ];
        let bricks = make("bricks", u64::from(reserved[0] + room.brick_pages) * PAGE_BRICKS * BRICK_BYTES, none)?;
        let materials = make("materials", u64::from(reserved[1] + room.material_pages) * MATERIAL_PAGE, none)?;
        for (buffer, bytes) in [(&bricks, bytemuck::cast_slice(model_bricks)), (&materials, model_materials)] {
            for (i, piece) in bytes.chunks(64 << 20).enumerate() {
                gpu.upload_buffer(buffer, (i * (64 << 20)) as u64, piece)?;
            }
        }
        let models = match &made {
            Some(made) => {
                let usage = storage;
                let buffer = gpu.create_buffer(
                    "voxel world tree models",
                    made.words.len() as u64 * 4,
                    usage,
                    MemoryLocation::GpuOnly,
                )?;
                gpu.upload_buffer(&buffer, 0, bytemuck::cast_slice(&made.words))?;
                Some(buffer)
            }
            None => None,
        };
        let pages = make("pages", slots_in * SLOT_PAGES * 4, none)?;
        let material_pages = make("material pages", slots_in * SLOT_MATERIAL_PAGES * 4, none)?;
        let slot_counts = make("slot counts", slots_in * 4, none)?;
        // Every page free at first: the bricks' pages, then the materials'.
        let free: Vec<u32> = (reserved[0]..reserved[0] + room.brick_pages)
            .chain(reserved[1]..reserved[1] + room.material_pages)
            .collect();
        let free_pages = make("free pages", free.len() as u64 * 4, none)?;
        gpu.upload_buffer(&free_pages, 0, bytemuck::cast_slice(&free))?;
        let pool = gpu.create_buffer("voxel world pool", 16, storage, MemoryLocation::GpuToCpu)?;
        pool.write(0, &[room.brick_pages, room.material_pages, 0, 0]);
        let staging = make("staging", jobs * JOB_MATERIALS, none)?;
        let placed = make("placed", jobs * JOB_BRICKS * 4, none)?;
        let slots = make("slots", jobs * 64 * rows * 4, none)?;
        let loose = make("loose bricks", jobs * JOB_BRICKS * BRICK_BYTES, none)?;
        let counters =
            make("counters", (jobs * COUNTERS + 8 + jobs + jobs * 33) * 4, vk::BufferUsageFlags::INDIRECT_BUFFER)?;
        let job_edits = make("job edits", jobs * MOST_COLUMN_EDITS * 4, none)?;
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
        // No slot holds a column yet: every entry names a sector nowhere, and every slot holds no pages.
        gpu.immediate(|cmd| unsafe {
            gpu.device.cmd_fill_buffer(cmd, table.handle, 0, vk::WHOLE_SIZE, 0x8000_0000);
            gpu.device.cmd_fill_buffer(cmd, slot_counts.handle, 0, vk::WHOLE_SIZE, 0);
        })?;
        let mut endless = Self {
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
            column_grounds,
            slots,
            loose,
            counters,
            candidates,
            grounds,
            program,
            volume: caves,
            forest,
            biomes,
            growth,
            trees,
            tree_carried,
            models,
            edits: Arc::default(),
            changes: Arc::default(),
            edit_total: 0,
            edit_buffer: None,
            job_edits,
            prints: Vec::new(),
            tables: std::collections::HashMap::new(),
            print_buffer: None,
            base_colours: colours.len(),
            colours,
            room,
            grow,
            pages,
            material_pages,
            slot_counts,
            free_pages,
            pool,
            staging,
            placed,
            makes,
            made: vec![None; (window * window) as usize],
            jobs: Vec::new(),
            window_sector: [0; 3],
            corner: Vec3::ZERO,
            rivers: None,
            waiting: false,
            water: vec![ColumnWater::default(); (window * window) as usize],
            water_changes: 0,
        };
        endless.rivers = laid_rivers(gpu, volume, endless.program.is_some(), window, endless.forest.as_ref())?;
        // No column is made yet, and no frame reads what the edits' buffers took the place of.
        for buffer in endless.set_edits(gpu, &volume.edits, &volume.changes)? {
            gpu.destroy_buffer(buffer);
        }
        Ok(endless)
    }

    /// Takes `edits` as what its columns are made with: those that differ
    /// from the edits it had, from the first that does, have the columns
    /// they reach made again. Gives back the buffers let go, where the
    /// edits outgrew theirs or printed prints it had none of.
    pub fn set_edits(
        &mut self,
        gpu: &Gpu,
        edits: &Arc<Vec<VoxelEdit>>,
        changes: &Arc<VoxelChanges>,
    ) -> Result<Vec<Buffer>> {
        let same_edits = Arc::ptr_eq(&self.edits, edits) || self.edits == *edits;
        let same_changes = Arc::ptr_eq(&self.changes, changes) || self.changes == *changes;
        if same_edits && same_changes {
            (self.edits, self.changes) = (edits.clone(), changes.clone());
            return Ok(Vec::new());
        }
        let same = self.edits.iter().zip(edits.iter()).take_while(|(a, b)| a == b).count();
        let voxel = self.voxel();
        let mut reached: Vec<(Vec3, Vec3)> =
            self.edits[same..].iter().chain(&edits[same..]).map(VoxelEdit::reach).collect();
        // The bricks whose voxels were set or let go.
        reached.extend(changes.changed_from(&self.changes).into_iter().map(|(low, high)| {
            (Vec3::from(low.map(|a| a as f32)) * voxel, Vec3::from(high.map(|a| (a + 1) as f32)) * voxel)
        }));
        for (low, high) in reached {
            // The columns the edit reaches, with a voxel's border, are made again; and those the trees standing in it
            // reach, as they may grow no more, or grow again.
            let trees = self.forest.as_ref().map_or(0.0, |(_, _, reach)| reach + 1.5 + trees::CELL);
            let low = ((low - trees) / voxel - 1.0) / 64.0;
            let high = ((high + trees) / voxel + 1.0) / 64.0;
            for z in low.z.floor() as i32..=high.z.floor() as i32 {
                for x in low.x.floor() as i32..=high.x.floor() as i32 {
                    let slot = self.slot_of([x, z]) as usize;
                    if self.made[slot] == Some([x, z]) {
                        self.made[slot] = None;
                    }
                }
            }
        }
        self.changes = changes.clone();
        let gone = self.write_edits(gpu, edits)?;
        self.edits = edits.clone();
        Ok(gone)
    }

    fn voxel(&self) -> f32 {
        1.0 / self.from.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32
    }

    /// Writes `edits` for the GPU: the prints they print that it has none of
    /// yet taken in, the colours they are printed or tinted in added to the
    /// palette, and their words, outlines and colour tables. Gives back the
    /// buffers let go.
    fn write_edits(&mut self, gpu: &Gpu, edits: &[VoxelEdit]) -> Result<Vec<Buffer>> {
        let mut gone = self.add_prints(gpu, edits)?;
        let voxel = self.voxel();
        let colours_before = self.colours.len();
        let changes = self.changes.clone();
        let bricks = changes.bricks().count();
        let mut words = vec![0u32; (edits.len() + bricks) * EDIT_WORDS];
        // Each colour table once, however many edits share it.
        let mut placed: std::collections::HashMap<Tinted, u32> = std::collections::HashMap::new();
        for (i, edit) in edits.iter().enumerate() {
            let (mut edit_words, points) = self.edit_words(edit, voxel);
            if matches!(edit.shape, VoxelShape::Area { .. } | VoxelShape::Cover { .. }) {
                edit_words[29] = words.len() as u32;
                edit_words[30] = (points.len() / 2) as u32;
                words.extend(points);
            }
            let tinted = match edit.print() {
                Some(printed) => Some(Tinted::Print(printed.print.id, edit.tint)),
                None => edit.fill.zip(edit.tint).map(|(material, tint)| Tinted::Fill(material, tint)),
            };
            if let Some(tinted) = tinted {
                let at = match placed.get(&tinted) {
                    Some(&at) => at,
                    None => {
                        let table = self.table(&tinted);
                        let at = words.len() as u32;
                        words.extend_from_slice(&table);
                        placed.insert(tinted, at);
                        at
                    }
                };
                edit_words[21] = at;
            }
            words[i * EDIT_WORDS..][..EDIT_WORDS].copy_from_slice(&edit_words);
        }
        // The voxels set one at a time over them all, an edit a brick: their kinds first, each its fill and its colour
        // table where it is tinted.
        let kinds_at = words.len() as u32;
        let kinds: Vec<(u32, Option<Tinted>)> = changes
            .kinds()
            .iter()
            .map(|kind| (kind.material as u32 + 1, kind.tint.map(|tint| Tinted::Fill(kind.material, tint))))
            .collect();
        words.extend(std::iter::repeat_n(0, kinds.len() * 2));
        for (k, (fill, tinted)) in kinds.into_iter().enumerate() {
            words[kinds_at as usize + k * 2] = fill;
            if let Some(tinted) = tinted {
                let at = match placed.get(&tinted) {
                    Some(&at) => at,
                    None => {
                        let table = self.table(&tinted);
                        let at = words.len() as u32;
                        words.extend_from_slice(&table);
                        placed.insert(tinted, at);
                        at
                    }
                };
                words[kinds_at as usize + k * 2 + 1] = at;
            }
        }
        for (i, (brick, changed)) in changes.bricks().enumerate() {
            let low = Vec3::from(brick.map(|a| (a * 8) as f32));
            let mut edit_words = [0u32; EDIT_WORDS];
            let middle = low + 4.0;
            edit_words[..4].copy_from_slice(&[middle.x.to_bits(), middle.y.to_bits(), middle.z.to_bits(), 8f32.to_bits()]);
            edit_words[4] = EDIT_VOXELS;
            for (k, v) in [low.x, low.y, low.z, low.x + 8.0, low.y + 8.0, low.z + 8.0].into_iter().enumerate() {
                edit_words[6 + k] = v.to_bits();
            }
            edit_words[21] = kinds_at;
            for (k, a) in brick.iter().enumerate() {
                edit_words[22 + k] = (a * 8) as u32;
            }
            edit_words[29] = words.len() as u32;
            words.extend(changed.set.iter().flat_map(|&word| [word as u32, (word >> 32) as u32]));
            words.extend(changed.codes.chunks(4).map(|four| u32::from_le_bytes([four[0], four[1], four[2], four[3]])));
            words[(edits.len() + i) * EDIT_WORDS..][..EDIT_WORDS].copy_from_slice(&edit_words);
        }
        self.edit_total = (edits.len() + bricks) as u32;
        if self.colours.len() != colours_before {
            gone.push(std::mem::replace(&mut self.palette, palette_buffer(gpu, &self.colours)?));
        }
        let bytes = (words.len() as u64 * 4).max(16);
        if self.edit_buffer.as_ref().is_none_or(|buffer| buffer.size < bytes) {
            // Room for half as many again, so a script carving a tunnel grows it seldom.
            let room = bytes + bytes / 2;
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
            let buffer = gpu.create_buffer("voxel world edits", room, usage, MemoryLocation::GpuOnly)?;
            gone.extend(self.edit_buffer.replace(buffer));
        }
        if let Some(buffer) = &self.edit_buffer {
            for (i, piece) in bytemuck::cast_slice::<u32, u8>(&words).chunks(64 << 20).enumerate() {
                gpu.upload_buffer(buffer, (i * (64 << 20)) as u64, piece)?;
            }
        }
        Ok(gone)
    }

    /// The places in the palette a print's colours or a tinted fill's take
    /// (`editColour` and `editTinted` in `shaders/voxel_world.slang`): for a
    /// print, its palette's colours in their tint; for a fill, every colour
    /// of the ground and the pavings, those of its own ground or paving in
    /// its tint. Added to the palette where they are new, the nearest it has
    /// where it is full.
    fn table(&mut self, tinted: &Tinted) -> Vec<u32> {
        if let Some(table) = self.tables.get(tinted) {
            return table.clone();
        }
        let table: Vec<u32> = match *tinted {
            Tinted::Print(id, tint) => {
                let print = self.prints.iter().find(|print| print.id == id).cloned();
                // Its colours with their kinds, glass and lights as its settings say.
                let palette: Vec<[u8; 4]> = print
                    .map(|print| (0..print.palette.len()).map(|i| print.volume_colour(i)).collect())
                    .unwrap_or_default();
                let shades: Vec<[u8; 4]> = match tint {
                    Some(tint) => tinted_colours(&palette, tint),
                    None => palette,
                };
                place_table(&mut self.colours, &shades)
            }
            Tinted::Fill(material, tint) => {
                let own = places_of(material);
                let base: Vec<[u8; 4]> = self.colours[..BASE_COLOURS].to_vec();
                let shades = tinted_colours(&own.iter().map(|&at| base[at]).collect::<Vec<_>>(), tint);
                let placed = place_table(&mut self.colours, &shades);
                (0..BASE_COLOURS)
                    .map(|at| match own.iter().position(|&o| o == at) {
                        Some(k) => placed[k],
                        None => at as u32,
                    })
                    .collect()
            }
        };
        self.tables.insert(tinted.clone(), table.clone());
        table
    }

    /// An edit as the GPU reads it (`EDIT_WORDS` in `shaders/voxel_world.slang`),
    /// but for where its outline and colour table are: where it is and how
    /// far a ball round there reaches, its kind, fill and marks, and the box
    /// it reaches, in the world's voxels; then its shape in whole numbers,
    /// as the CPU works it out ([`edit::Fixed`]).
    fn edit_words(&self, edit: &VoxelEdit, voxel: f32) -> ([u32; EDIT_WORDS], Vec<u32>) {
        let mut words = [0u32; EDIT_WORDS];
        let (low, high) = edit.reach();
        let (low, high) = (low / voxel, high / voxel);
        let at = edit.at() / voxel;
        let fill = edit.fill.map_or(0, |material| material as u32 + 1);
        let corner = |i: usize| Vec3::new([low.x, high.x][i & 1], [low.y, high.y][i >> 1 & 1], [low.z, high.z][i >> 2]);
        let mut radius = (0..8).map(|i| corner(i).distance(at)).fold(0.0, f32::max) + 1.0;
        let fixed = edit::Fixed::of(edit, voxel);
        let whole = |v: i64| v as i32 as u32;
        let mut put = |at: usize, values: &[i64]| {
            for (k, &v) in values.iter().enumerate() {
                words[at + k] = whole(v);
            }
        };
        for (i, row) in fixed.back.iter().enumerate() {
            put(12 + i * 3, row);
        }
        put(22, &fixed.at);
        let kind = match &edit.shape {
            VoxelShape::Ball { radius: r, .. } => {
                radius = r / voxel + 1.0;
                put(25, &[fixed.size[0]]);
                EDIT_BALL
            }
            VoxelShape::Box { .. } | VoxelShape::Ramp { .. } => {
                put(25, &fixed.size);
                put(28, &[fixed.clear]);
                if matches!(edit.shape, VoxelShape::Ramp { .. }) { EDIT_RAMP } else { EDIT_BOX }
            }
            VoxelShape::Area { .. } => {
                put(25, &[fixed.low, fixed.high]);
                put(28, &[fixed.clear]);
                EDIT_AREA
            }
            VoxelShape::Cover { .. } => {
                words[18] = fixed.cover[0] as u32;
                words[19] = fixed.cover[1] as u32;
                // Reaching all the way up and down: the box alone says where.
                radius = 1.0e30;
                EDIT_COVER
            }
            VoxelShape::Print(printed) => {
                let index = self.prints.iter().position(|print| print.id == printed.print.id).unwrap_or(0);
                put(25, &[index as i64, fixed.per_metre[0], fixed.per_metre[1]]);
                put(29, &fixed.origin);
                EDIT_PRINT
            }
        };
        let marks = match edit.print() {
            Some(printed) => {
                (if printed.fit { EDIT_FIT } else { 0 }) | (if fixed.widened { EDIT_WIDENED } else { 0 })
            }
            None => 0,
        };
        words[..4].copy_from_slice(&[at.x.to_bits(), at.y.to_bits(), at.z.to_bits(), radius.to_bits()]);
        words[4] = kind | fill << 8 | marks;
        for (i, v) in [low.x, low.y, low.z, high.x, high.y, high.z].into_iter().enumerate() {
            words[6 + i] = v.to_bits();
        }
        let points = fixed.points.iter().flat_map(|p| [whole(p[0]), whole(p[1])]).collect();
        (words, points)
    }

    /// Takes in the prints `edits` print that it has none of yet, their
    /// voxels to the prints' buffer. Gives back the buffer let go.
    fn add_prints(&mut self, gpu: &Gpu, edits: &[VoxelEdit]) -> Result<Vec<Buffer>> {
        let mut added = false;
        for printed in edits.iter().filter_map(VoxelEdit::print) {
            if self.prints.iter().any(|print| print.id == printed.print.id) {
                continue;
            }
            self.prints.push(printed.print.clone());
            added = true;
        }
        if !added {
            return Ok(Vec::new());
        }
        let words = prints_words(&self.prints);
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let buffer = gpu.create_buffer("voxel world prints", words.len() as u64 * 4, usage, MemoryLocation::GpuOnly)?;
        for (i, piece) in bytemuck::cast_slice::<u32, u8>(&words).chunks(64 << 20).enumerate() {
            gpu.upload_buffer(&buffer, (i * (64 << 20)) as u64, piece)?;
        }
        Ok(self.print_buffer.replace(buffer).into_iter().collect())
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
        if room_of(program.as_ref(), &volume.trees, self.window * self.window, self.grow) != self.room {
            return Ok(None);
        }
        // Other plants make other voxels at the start of the room.
        if plants_of(volume) != plants_of(&self.from) {
            return Ok(None);
        }
        let (program, caves) = program_buffers(gpu, program)?;
        let (colours, forest, _, biomes) = forest_buffers(gpu, volume, false)?;
        // The prints' and tints' colours placed again after the ground's and the trees', with every column made over.
        self.base_colours = colours.len();
        self.colours = colours;
        self.tables.clear();
        let edits = self.edits.clone();
        let mut gone_edits = self.write_edits(gpu, &edits)?;
        let palette = palette_buffer(gpu, &self.colours)?;
        let mut gone: Vec<Buffer> =
            std::mem::replace(&mut self.program, program).map(|(buffer, _)| buffer).into_iter().collect();
        gone.append(&mut gone_edits);
        gone.extend(
            std::mem::replace(&mut self.volume, caves).into_iter().flat_map(|(words, carried)| [words, carried]),
        );
        gone.push(std::mem::replace(&mut self.palette, palette));
        gone.push(std::mem::replace(&mut self.growth, growth_buffer(gpu, volume)?));
        gone.extend(std::mem::replace(&mut self.forest, forest).map(|(buffer, _, _)| buffer));
        gone.extend(std::mem::replace(&mut self.biomes, biomes));
        let laid = laid_rivers(gpu, volume, self.program.is_some(), self.window, self.forest.as_ref())?;
        gone.extend(std::mem::replace(&mut self.rivers, laid).map(|rivers| rivers.table));
        self.from = volume.clone();
        self.made.iter_mut().for_each(|made| *made = None);
        self.water.iter_mut().for_each(|water| *water = ColumnWater::default());
        self.water_changes += 1;
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
            &self.column_grounds,
            &self.slots,
            &self.loose,
            &self.grounds,
            &self.pages,
            &self.material_pages,
            &self.staging,
            &self.placed,
        ]
        .iter()
        .map(|buffer| buffer.size)
        .sum::<u64>()
            + self.models.as_ref().map_or(0, |models| models.size)
            + self.print_buffer.as_ref().map_or(0, |prints| prints.size)
    }

    /// Moves the window round `camera`, the volume's corner at `corner`, and
    /// picks the columns to make this frame: those in the window that their
    /// slots do not hold, the nearest first.
    pub fn follow(&mut self, corner: Vec3, camera: Vec3) {
        let moved = self.corner != corner;
        self.corner = corner;
        let metres = sector_metres(&self.from);
        let at = ((camera - corner) / metres).floor();
        let half = (self.window / 2) as i32;
        self.window_sector = [at.x as i32 - half, 0, at.z as i32 - half];
        if let Some(rivers) = &mut self.rivers {
            let least = [self.window_sector[0], self.window_sector[2]];
            rivers.follow(least, self.window, [at.x as i32, at.z as i32]);
        }
        if moved {
            self.lay_water_again();
        }
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
        // A column is made once the rivers' points round it are laid in.
        let rivers = self.rivers.as_ref();
        let wanted_any = !wanted.is_empty();
        let ready: Vec<[i32; 2]> = wanted
            .into_iter()
            .filter(|(_, column)| rivers.is_none_or(|rivers| rivers.ready_for(*column)))
            .take(most)
            .map(|(_, column)| column)
            .collect();
        for column in ready {
            let slot = self.slot_of(column);
            self.made[slot as usize] = Some(column);
            self.jobs.push((column, slot));
            self.lay_water(column, slot);
        }
        self.waiting = wanted_any && self.jobs.is_empty();
    }

    /// Lays the water of the rivers over column `column` in slot `slot`, as
    /// it is made.
    fn lay_water(&mut self, column: [i32; 2], slot: u32) {
        let quads = match &self.rivers {
            Some(rivers) => {
                let grid = rivers.layer.grid(column);
                let voxel = 1.0 / self.from.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
                let floor = self.from.generator.as_ref().map_or(0.0, |generator| generator.floor);
                let made = rivers::water_quads(&grid, column, (self.corner, floor, voxel));
                // Water of voxels is the columns' own, its waterfalls voxels thrown over their lips; smooth water is
                // its squares, its waterfalls sheets.
                match self.from.water {
                    VoxelWaterLook::Voxels => ColumnWater { lips: made.lips, ..ColumnWater::default() },
                    VoxelWaterLook::Smooth => ColumnWater { lips: Vec::new(), ..made },
                }
            }
            None => ColumnWater::default(),
        };
        let kept = &mut self.water[slot as usize];
        if !(kept.is_empty() && quads.is_empty()) {
            *kept = quads;
            self.water_changes += 1;
        }
    }

    /// Lays the water over every column made again, as the volume moved.
    fn lay_water_again(&mut self) {
        for slot in 0..self.made.len() {
            if let Some(column) = self.made[slot].filter(|_| !self.water[slot].is_empty()) {
                self.lay_water(column, slot as u32);
            }
        }
    }

    /// The water over its rivers and lakes, each square in the world; the
    /// squares its waterfalls pour over; its waterfalls' lips; and how many
    /// times they have changed.
    pub fn water_squares(&self) -> impl Iterator<Item = &GpuVoxelWater> {
        self.water.iter().flat_map(|water| &water.squares)
    }

    pub fn water_falls(&self) -> impl Iterator<Item = &GpuVoxelWater> {
        self.water.iter().flat_map(|water| &water.falls)
    }

    pub fn water_lips(&self) -> impl Iterator<Item = &rivers::GpuVoxelLip> {
        self.water.iter().flat_map(|water| &water.lips)
    }

    pub fn water_changes(&self) -> u64 {
        self.water_changes
    }

    fn slot_of(&self, column: [i32; 2]) -> u32 {
        let w = self.window as i32;
        (column[1].rem_euclid(w) * w + column[0].rem_euclid(w)) as u32
    }

    /// Whether its shared room ran short of pages for a column, as the GPU
    /// noted it a frame or two ago, and how many times what its columns keep
    /// on average it has room for.
    pub fn short(&self) -> Option<f64> {
        let words: &[u32] = bytemuck::cast_slice(&self.pool.read()[..16]);
        (words[2] != 0).then_some(self.grow)
    }

    /// How many columns are made this frame.
    pub fn making_now(&self) -> u32 {
        self.jobs.len() as u32
    }

    /// Whether any column of the window is still to be made.
    pub fn unfinished(&self) -> bool {
        !self.jobs.is_empty() || self.waiting
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
            pages: self.pages.address,
            water: if self.from.water == VoxelWaterLook::Voxels { WATER_COLOUR + 1 } else { 0 },
            shades: WATER_SHADES,
        };
        let at = Vec3::new(self.window_sector[0] as f32, 0.0, self.window_sector[2] as f32) * metres;
        (record, self.corner + at)
    }

    /// What the GPU holds at each of `cells`, in voxels from the volume's
    /// corner: a palette place and one, or 0 for nothing; and the palette's
    /// colours. For checking the GPU's voxels against the CPU's: the cells'
    /// columns must be made.
    pub fn probe(
        &self,
        gpu: &Gpu,
        (pipelines, bindless): (&Pipelines, &Bindless),
        cells: &[[i32; 3]],
    ) -> Result<(Vec<u32>, Vec<[u8; 4]>)> {
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct Probe {
            volume: GpuVoxelVolume,
            cells: u64,
            found: u64,
            count: u32,
            pad: u32,
        }
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let count = cells.len().max(1) as u64;
        let at = self.window_sector;
        let local: Vec<[i32; 3]> = cells.iter().map(|c| [c[0] - at[0] * 64, c[1], c[2] - at[2] * 64]).collect();
        let asked = gpu.create_buffer("voxel probe cells", count * 12, usage, MemoryLocation::CpuToGpu)?;
        asked.write(0, &local);
        let found = gpu.create_buffer("voxel probe found", count * 4, usage, MemoryLocation::GpuToCpu)?;
        let record = gpu.create_buffer("voxel probe", size_of::<Probe>() as u64, usage, MemoryLocation::CpuToGpu)?;
        let probe =
            Probe { volume: self.drawn().0, cells: asked.address, found: found.address, count: cells.len() as u32, pad: 0 };
        record.write(0, std::slice::from_ref(&probe));
        let push = crate::gpu::Push { buffers: record.address, ..bytemuck::Zeroable::zeroed() };
        gpu.immediate(|cmd| unsafe {
            gpu.device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipelines.voxel_probe);
            bindless.push(gpu, cmd, &push);
            gpu.device.cmd_dispatch(cmd, (cells.len() as u32).div_ceil(64), 1, 1);
        })?;
        let out = bytemuck::cast_slice::<u8, u32>(&found.read()[..cells.len() * 4]).to_vec();
        for buffer in [asked, found, record] {
            gpu.destroy_buffer(buffer);
        }
        Ok((out, self.colours.clone()))
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
            brick_room: JOB_BRICKS as u32,
            material_room: JOB_MATERIALS as u32,
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
            biomes: self.biomes.as_ref().map_or(0, |buffer| buffer.address),
            column_grounds: self.column_grounds.address,
            trees: self.trees.address,
            tree_carried: self.tree_carried.address,
            tree_kinds: self.forest.as_ref().map_or(0, |(_, kinds, _)| *kinds),
            tree_reach: self.forest.as_ref().map_or(0.0, |(_, _, reach)| *reach),
            pages: self.pages.address,
            material_pages: self.material_pages.address,
            slot_counts: self.slot_counts.address,
            free_pages: self.free_pages.address,
            pool: self.pool.address,
            staging: self.staging.address,
            placed: self.placed.address,
            models: self.models.as_ref().map_or(0, |models| models.address),
            edits: self.edit_buffer.as_ref().map_or(0, |buffer| buffer.address),
            job_edits: self.job_edits.address,
            prints: self.print_buffer.as_ref().map_or(0, |buffer| buffer.address),
            rivers: self.rivers.as_ref().map_or(0, |rivers| rivers.table.address),
            growth: self.growth.address,
            river_window: self.rivers.as_ref().map_or(0, |rivers| rivers.window),
            water_voxels: u32::from(self.from.water == VoxelWaterLook::Voxels),
            edit_count: self.edit_total,
            edit_pad: 0,
            brick_pages: self.room.brick_pages,
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
            staging: self.staging.handle,
            slots: self.slots.handle,
            counters: self.counters.handle,
            jobs: self.jobs.iter().map(|(_, slot)| *slot).collect(),
            tall: self.tall,
        }
    }
}

/// What recording the passes that make a frame's columns needs.
pub(crate) struct Recording {
    staging: vk::Buffer,
    slots: vk::Buffer,
    counters: vk::Buffer,
    jobs: Vec<u32>,
    tall: u32,
}

impl Recording {
    /// Records the passes that make this frame's columns, from the record at
    /// `make`, into `cmd`: each job's materials cleared, then the passes,
    /// with what each reads of the last finished between them.
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
            device.cmd_fill_buffer(cmd, self.staging, 0, self.jobs.len() as u64 * JOB_MATERIALS, 0);
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
        // The edits that reach each column before its trees, whose feet they may take away.
        run(pipelines.voxel_world_edits, [1, jobs, 1]);
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
        run(pipelines.voxel_world_free, [1, 1, 1]);
        barrier(vk::PipelineStageFlags2::COMPUTE_SHADER, read_write);
        run(pipelines.voxel_world_pack, [1, 1, 1]);
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
            self.column_grounds,
            self.slots,
            self.loose,
            self.counters,
            self.candidates,
            self.grounds,
            self.growth,
            self.trees,
            self.tree_carried,
            self.pages,
            self.material_pages,
            self.slot_counts,
            self.free_pages,
            self.pool,
            self.staging,
            self.placed,
        ];
        all.extend(self.forest.map(|(buffer, _, _)| buffer));
        all.extend(self.biomes);
        all.extend(self.models);
        all.extend(self.edit_buffer);
        all.extend(self.print_buffer);
        all.push(self.job_edits);
        all.extend(self.program.map(|(buffer, _)| buffer));
        all.extend(self.volume.into_iter().flat_map(|(words, carried)| [words, carried]));
        all.extend(self.makes);
        all.extend(self.rivers.map(|rivers| rivers.table));
        all
    }
}


/// Prints as the GPU reads them (`PRINTS_AT` in `shaders/voxel_world.slang`):
/// how many, then for each its bricks along X, Y and Z, where its grid, its
/// bricks and its materials start among the words, its size in voxels and
/// where its origin is, and where its footprint starts; then each one's
/// grid, bricks and materials, these four to a word, as places in its own
/// palette (which each edit's colour table makes places in the volume's),
/// and its footprint.
fn prints_words(prints: &[Arc<VoxelPrint>]) -> Vec<u32> {
    let mut words = vec![0u32; PRINTS_AT + prints.len() * PRINT_WORDS];
    words[0] = prints.len() as u32;
    for (i, print) in prints.iter().enumerate() {
        let across = print.bricks_across();
        let grid = words.len() as u32;
        words.extend_from_slice(&print.grid);
        let bricks = words.len() as u32;
        words.extend_from_slice(&print.bricks);
        let materials = words.len() as u32;
        words.extend(print.materials.chunks(4).map(|four| {
            let mut word = [0u8; 4];
            word[..four.len()].copy_from_slice(four);
            u32::from_le_bytes(word)
        }));
        let footprint = words.len() as u32;
        words.extend(print.footprint());
        let at = PRINTS_AT + i * PRINT_WORDS;
        let o = print.origin;
        words[at..at + PRINT_WORDS].copy_from_slice(&[
            across[0],
            across[1],
            across[2],
            grid,
            bricks,
            materials,
            print.size[0],
            print.size[1],
            print.size[2],
            o.x.to_bits(),
            o.y.to_bits(),
            o.z.to_bits(),
            footprint,
        ]);
    }
    debug_assert_eq!(print::BRICK_WORDS, 17);
    words
}

/// What a colour table is made for: a print's palette in a tint (or its
/// own colours), or a ground's or a paving's colours in a tint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Tinted {
    Print(u64, Option<[u8; 3]>),
    Fill(VoxelMaterial, [u8; 3]),
}

/// The places among the palette's first colours a fill of `material` takes
/// (`fillVoxel` in `shaders/voxel_world.slang`).
fn places_of(material: VoxelMaterial) -> Vec<usize> {
    let generated = GROUND_COLORS.len();
    let paving = generated + GENERATED_COLORS.len();
    match material {
        VoxelMaterial::Dirt => vec![1],
        VoxelMaterial::Grass => vec![0, 1],
        VoxelMaterial::Rock => vec![generated, generated + 1, generated + 2],
        VoxelMaterial::Sand => vec![generated + 3, generated + 4],
        VoxelMaterial::Snow => vec![generated + 5, generated + 6, generated + 1],
        VoxelMaterial::Gravel => vec![generated + 7, generated + 8],
        _ => {
            let first = paving + (material as usize - VoxelMaterial::Cobbles as usize) * 4;
            (first..first + 4).collect()
        }
    }
}

/// `colours` in `tint`: their average made the tint, each shaded from it
/// as it was from their average.
fn tinted_colours(colours: &[[u8; 4]], tint: [u8; 3]) -> Vec<[u8; 4]> {
    if colours.is_empty() {
        return Vec::new();
    }
    let mean: [f32; 3] =
        [0, 1, 2].map(|i| colours.iter().map(|c| f32::from(c[i])).sum::<f32>() / colours.len() as f32);
    colours
        .iter()
        .map(|c| {
            let shade = |i: usize| (f32::from(c[i]) / mean[i].max(1.0) * f32::from(tint[i])).round().clamp(0.0, 255.0) as u8;
            [shade(0), shade(1), shade(2), c[3]]
        })
        .collect()
}

/// What `world` makes at each of `places`, `[x, y, z]` in metres in its
/// world, run on the GPU as an endless volume's columns and voxels run it:
/// the ground's height in metres, how warm it is, how wet, its rock mask,
/// and how far into its Volume's ground a cell `size` metres across at the
/// place is (nothing where it has no Volume on the GPU); and with `biomes`,
/// the biome the place is, by its place in their list (-1 for none). For
/// checking the GPU against the CPU.
pub(crate) fn generated_at(
    gpu: &Gpu,
    (pipelines, bindless): (&Pipelines, &Bindless),
    world: &WorldGen,
    biomes: Option<&VoxelBiomes>,
    places: &[[f32; 3]],
    size: f32,
) -> Result<Vec<[f32; 6]>> {
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
    make.sea_level = world.sea_level();
    let biome_buffer = match biomes {
        Some(biomes) => {
            let words = biome_words(biomes, &mut Vec::new());
            let buffer =
                gpu.create_buffer("generator biomes", words.len() as u64 * 4, usage, MemoryLocation::CpuToGpu)?;
            buffer.write(0, &words);
            make.biomes = buffer.address;
            Some(buffer)
        }
        None => None,
    };
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
    let out = words.chunks(16).map(|place| [place[0], place[1], place[2], place[3], place[4], place[15]]).collect();
    for buffer in [code, asked, made, record].into_iter().chain(cells).chain(biome_buffer) {
        gpu.destroy_buffer(buffer);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_kinds_of_building_each_keep_their_colours_in_a_bank_of_their_own_or_a_shared_one() {
        let mut palette: Vec<[u8; 4]> =
            GROUND_COLORS.iter().chain(&GENERATED_COLORS).chain(&PAVING_COLORS).chain(&WATER_COLORS).copied().collect();
        let mut tables = Vec::new();
        for kind in 0..60u8 {
            // Thirty colours a kind, all its own but its first, which is the ground's dirt.
            let mut colours: Vec<[u8; 4]> = (0..29u8).map(|k| [kind * 4, k * 8, 255 - kind, 255]).collect();
            colours.insert(0, GROUND_COLORS[1]);
            tables.push((colours.clone(), place_table(&mut palette, &colours)));
        }
        assert!(palette.len() <= MOST_COLOURS, "{} colours", palette.len());
        for (colours, places) in &tables {
            assert_eq!(places[0], 1, "the dirt is the shared dirt");
            // Each colour where the table says, and all a kind's own colours in one bank.
            for (colour, &place) in colours.iter().zip(places) {
                assert_eq!(palette[place as usize][..3], colour[..3]);
            }
            let banks: std::collections::HashSet<_> = places[1..].iter().map(|&p| bank_of(p as usize)).collect();
            assert_eq!(banks.len(), 1, "one bank: {banks:?}");
        }
        // The same kind printed again takes the places it had.
        let again = place_table(&mut palette, &tables[59].0);
        assert_eq!(again, tables[59].1);
        // Over a shared colour's place, a bank's colour says the shared one nearest it.
        let words = palette_words(&palette);
        let at = tables[10].1[5] as usize;
        assert_eq!((words[at] >> 24) as usize, nearest_of(&palette[..SHARED_COLOURS], palette[at]));
    }
}
