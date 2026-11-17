//! Worlds of blocks on the GPU (see `voxear_terrain::blocks`): each world's
//! chunks' quads in a pool of its own, a range of it a chunk, staged in
//! when a chunk's faces change, by pointer, and drawn from it six vertices
//! a quad with no vertex buffer (`blockVertex` in `shaders/blocks.slang`).
//!
//! Each view picks the chunks it sees and, of each, only the faces turned
//! towards its camera; quads following on in the pool are drawn as one.
//! The sun's cascades take the faces turned towards the sun of the chunks
//! they see: ground is closed all round, so sunlight meets those first. Each chunk
//! also gives the GI's rays a mesh of its own, its faces merged by surface
//! alone, as the terrain's tiles do.
//!
//! A chunk's water's quads follow its ground's in the pool. They are drawn
//! apart, with the water after the opaque scene (`water::record`): every
//! one of a chunk the view sees, seen from either side, casting no shadow.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use voxear_core::math::{Mat3, Mat4, Quat, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, KeptBatch, MemoryLocation, Retired, vk};
use voxear_terrain::coord_hash::CoordSet;
use voxear_terrain::blocks::{
    fell::LooseBlocks,
    light::{self, LightMap},
    BlockWorld, ChunkMeshes, RingFades,
    chunk::CHUNK,
    mesh::{ChunkMesh, FACINGS},
    smooth,
};

use crate::{
    MaterialHandle, MeshHandle,
    alloc::{RangeAllocator, SlotAllocator},
    assets::Assets,
    gpu::{
        GpuInstance, INSTANCE_RAYS_ONLY, LIST_COUNT, LIST_EARLY, LIST_LOCAL_SHADOW, LIST_SHADOW, LOCAL_SHADOW_LIGHTS,
        Push,
    },
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    mesh::{MeshData, NO_WIND_MASKS, PROXY_MATERIAL, Vertex},
    passes::{attachment, begin_rendering, bind, set_viewport},
    pipelines::Pipelines,
    scene::{ExtractedFrame, Owner},
    shadows::CASCADES,
    terrain::{GpuTerrainSurface, SurfaceLook, surface_flags},
};

/// How a world of blocks looks: the material a face with no surface of its
/// own is drawn with, and its surfaces, the ways its faces are drawn (see
/// `voxear_terrain::blocks::file::BlockFile::faces`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockLook {
    pub material: MaterialHandle,
    pub surfaces: Arc<[SurfaceLook]>,
    /// The small cubes its cubes' faces are cut into; none by default.
    pub voxels: BlockVoxels,
    /// Its blocks of parts' motions, as `BlockFaces::motion` counts them
    /// from 1 (see `voxear_terrain::blocks::file::BlockFile::motions`).
    pub motions: Arc<[GpuBlockMotion]>,
    /// How its faces take the light from all round them.
    pub light: BlockLight,
}

/// How a world of cubes' faces take the light from all round them (see
/// `voxear_terrain::blocks::file::BlockLighting`): with `kept`, by the
/// light kept for each block (`voxear_terrain::blocks::light`), the sky's
/// light from all round as much as a face's sky light lets in and `glow`,
/// linear and as bright as it is, as much as its block light; the sun's as
/// its shadows say, and past them as its sky light does. Else as the rest
/// of the scene is lit: by the GI's probes, and the sky's light darkened
/// by ambient occlusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockLight {
    pub kept: bool,
    pub glow: Vec3,
}

impl BlockLight {
    /// How bright block light is at most, times as bright as usual.
    pub const BRIGHTEST: f32 = 8.0;
}

impl Default for BlockLight {
    /// As the scene is lit, which is all a world drawn smooth has.
    fn default() -> Self {
        Self { kept: false, glow: Vec3::new(1.0, 0.48, 0.15) }
    }
}

/// A key of a block's motion as the shaders read it (`BlockMotionKey` in
/// `shaders/common.slang`): when, how far its parts are turned, in
/// radians, and moved, in sixteenths, and how the way on goes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMotionKey {
    pub time: f32,
    pub turn: f32,
    pub shift: f32,
    pub ease: u32,
}

/// The most keys a motion has on the GPU.
pub const MOTION_KEYS: usize = 8;

/// The most motions a world of blocks draws, and the most of its blocks
/// moving at once that it draws moving.
pub const MAX_MOTIONS: usize = 63;
pub const MAX_MOVING: usize = 64;

/// A block's motion as the shaders read it (`BlockMotion`): how long it
/// lasts, how many keys it has, and how far a block that hangs swings at
/// most, in radians.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuBlockMotion {
    pub length: f32,
    pub count: u32,
    pub swing: f32,
    pub pad: u32,
    pub keys: [GpuMotionKey; MOTION_KEYS],
}

impl GpuBlockMotion {
    /// `motion` as the shaders read it, and a block that hangs swinging
    /// `swing` degrees at most.
    pub fn of(motion: Option<&voxear_terrain::blocks::shape::BlockMotion>, swing: f32) -> Self {
        let mut gpu = Self { swing: swing.to_radians(), ..Self::default() };
        if let Some(motion) = motion {
            gpu.length = motion.length();
            for (place, key) in gpu.keys.iter_mut().zip(&motion.keys) {
                *place = GpuMotionKey { time: key.time, turn: key.turn.to_radians(), shift: key.shift, ease: key.ease.code() };
            }
            gpu.count = motion.keys.len().min(MOTION_KEYS) as u32;
        }
        gpu
    }
}

/// A block moving as the shaders read it (`MovingBlock`): where it is in
/// its world's blocks, whether it is opening, and how long since it
/// started, in seconds.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMovingBlock {
    pub at: [i32; 4],
    pub timing: [f32; 4],
}

/// The small cubes a world of cubes' faces are drawn as made of: each face
/// cut into `count` of them along a block's side, each one colour, its
/// block's picture's there, and sunk into the face up to `relief` of them
/// deep where the picture is dark, or here and there on a face with no
/// picture. Worked out as the face is shaded (`blockCubes` in
/// `shaders/blocks.slang`), so the world's mesh is the same; they give way
/// to the plain face where they are too small on screen to tell apart. A
/// world drawn smooth has none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockVoxels {
    /// How many small cubes lie along a block's side; 0 for none.
    pub count: u32,
    /// How many small cubes deep the deepest of them are sunk, from 0 for
    /// a flat face of squares to 4.
    pub relief: f32,
}

impl Default for BlockVoxels {
    fn default() -> Self {
        Self { count: 0, relief: 2.0 }
    }
}

impl BlockVoxels {
    /// The most small cubes along a block's side, and the deepest they sink.
    pub const MOST: u32 = 64;
    pub const DEEPEST: f32 = 4.0;

    /// `count` small cubes to a block's side, sunk as deep as by default.
    pub fn of(count: u32) -> Self {
        Self { count, ..Self::default() }
    }
}

impl Component for BlockLook {}

/// The colours a world of blocks' tinted faces take from the biomes they
/// stand in, round where it is seen from (see
/// `voxear_terrain::blocks::generate::tints`): a square of its tint
/// lattice's points, a point every `TINT_STEP` blocks, `size` a side from
/// point `corner`, row by row along x, each point its grass tint and then
/// its foliage tint, packed as the lattice packs them. A face beyond it
/// takes the tint of the nearest point at its edge; a world without one
/// is drawn untinted.
/// And its far tints, as many points a side as packed alike, each
/// `TINT_FAR` times further apart, for the far view past its near
/// ones: their first point and their points. And how far from the camera
/// the plants that colour its ground far off (see `tints::cover_of`) stop
/// being drawn, in metres: where they begin to fade, and over how far, the
/// ground taking their colour as they go; and the surfaces they colour, a
/// bit each by number.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockTints {
    pub corner: [i32; 2],
    pub size: u32,
    pub points: Arc<[u32]>,
    pub far_corner: [i32; 2],
    pub far_points: Arc<[u32]>,
    pub cover: [f32; 2],
    pub covered: [u32; 8],
}

/// A world of blocks' surface the plants on it colour far off (see
/// [`BlockTints::covered`]): `TERRAIN_SURFACE_COVERED` in `common.slang`.
const SURFACE_COVERED: u32 = 512;

impl Component for BlockTints {}

/// The most worlds of blocks a view draws.
pub const MAX_BLOCK_WORLDS: usize = 16;

/// The most surfaces a world of blocks has.
pub const MAX_BLOCK_SURFACES: usize = 256;

/// The most chunks with faces a world draws.
const SLOTS: u32 = 65_536;

/// Where a pool's tints start, after each slot's chunk's place, and the most
/// room they take: as many points as a world's tints have at most, two
/// tints each.
const TINTS_AT: u64 = SLOTS as u64 * 16;
const TINT_BYTES: u64 = (MOST_TINT_POINTS * 2 * 4) as u64 * 2;

// The shaders' BLOCK_TINT_STEP (common.slang) is the lattice's.
const _: () = assert!(voxear_terrain::blocks::generate::tints::TINT_STEP == 4);
// And its BLOCK_TINT_FAR the far view's, and BLOCK_TINT_MOST how many points the near ones take.
const _: () = assert!(voxear_terrain::blocks::generate::tints::TINT_FAR == 16);
const _: () = assert!(MOST_TINT_POINTS == 65536);
// And its BLOCK_MOST_COVER the most cover a point keeps.
const _: () = assert!(voxear_terrain::blocks::generate::tints::MOST_COVER == 0.5);

/// The most points a side a world's tints have.
pub const MOST_TINT_POINTS_SIDE: usize = 256;
const MOST_TINT_POINTS: usize = MOST_TINT_POINTS_SIDE * MOST_TINT_POINTS_SIDE;

/// Where a pool's fades start, after its tints: two times each slot's
/// chunk fades by (see [`fade_times`]).
const FADES_AT: u64 = TINTS_AT + TINT_BYTES;

/// Where a pool's quads start: after its fades.
const QUADS_AT: u64 = FADES_AT + SLOTS as u64 * 8;

/// When chunk `key`'s face fade in and out as its world's rings fade into
/// each other (`fades`), on `voxear_terrain::blocks::fade_clock`: in as the
/// coarse column over it is covered, and never while it is not; and out as
/// its own column is, never where it is not.
fn fade_times(fades: &RingFades, key: [i32; 4]) -> [f32; 2] {
    let [level, x, _, z] = key;
    // The world's own blocks drawn past where they are made, at level -1, under the first coarser ring as its own.
    let came = fades.covered.get(&[level.max(0) + 1, x >> 1, z >> 1]).copied().unwrap_or(0.0);
    let went = fades.covered.get(&[level, x, z]).copied().unwrap_or(f32::MAX);
    [came, went]
}

/// How many quads a pool starts with room for; it doubles when full.
const FIRST_CAPACITY: u64 = 1 << 16;

/// How many chunks get their meshes for rays in a frame, at most.
const PROXIES_A_FRAME: usize = 8;

/// How much nearer the camera, in chunks, a chunk without a mesh for rays
/// has to be than the farthest with one to take its room in a full mesh
/// pool: chunks about as far off as each other do not swap back and forth
/// as the camera moves.
const PROXY_SWAP_MARGIN: f32 = 2.0;

/// The level of a smooth chunk's surface its mesh for rays is made from:
/// its first coarser, half a block from its own at most (see
/// `smooth::COARSER`). Kept alike where chunks meet at any level, as each
/// level is, it lets no light through between them.
const RAYS_LEVEL: usize = 1;

/// A world of blocks as `shaders/blocks.slang` reads it (`BlockWorld` in
/// `shaders/common.slang`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuBlockWorld {
    pub corner: [f32; 3],
    pub block_size: f32,
    pub quads: u64,
    pub origins: u64,
    pub surfaces: u64,
    pub surface_count: u32,
    pub material: u32,
    pub anchor: u32,
    /// `BLOCKS_SMOOTH` where it is drawn smooth.
    pub flags: u32,
    /// How many small cubes its faces are cut into along a block's side, 0
    /// for none, and how many deep they sink at most (see [`BlockVoxels`]).
    pub voxels: f32,
    pub relief: f32,
    /// How high its sea's top is over its corner, in metres: only water
    /// faces there are drawn rough (see `sea`). Far below for a world whose
    /// water is all its sea.
    pub sea_top: f32,
    /// How many points its tints are a side, 0 for none (see `BlockTints`).
    pub tint_size: u32,
    /// Its motions and its blocks moving now, and how many of each.
    pub motions: u64,
    pub moving: u64,
    pub motion_count: u32,
    pub moving_count: u32,
    /// Its tints' points, its near ones and then its far ones, and the
    /// points they start from.
    pub tints: u64,
    pub tint_corner: [i32; 2],
    /// How it is turned about its corner, as a quaternion, where it is
    /// drawn turned (`BLOCKS_TURNED`).
    pub turn: [f32; 4],
    pub far_tint_corner: [i32; 2],
    /// Where a world without end's rings fade into each other (see
    /// `voxear_terrain::blocks::RingFades`): the clock they fade by, its
    /// coarsest ring's level, each chunk slot's times (see [`fade_times`]),
    /// and where its rings are round, from its corner, how far its own
    /// blocks reach and over how wide a band they fade; a reach of 0 for
    /// none.
    pub ring_clock: f32,
    pub ring_coarsest: u32,
    pub fades: u64,
    pub rings: [f32; 4],
    /// How far from the camera its grass takes the colour of the plants on
    /// it, in metres: from where they begin to fade, over how far (see
    /// [`BlockTints::cover`]).
    pub cover: [f32; 2],
    /// Lit by the light kept for its blocks (`BLOCKS_LIT`): the colour of
    /// the light its blocks give, linear and as bright as it is, and over
    /// it how much of the sky's light the darkest face takes; and each
    /// quad's corners' light, a word a quad (see
    /// `voxear_terrain::blocks::mesh::ChunkMesh::light`).
    pub glow: [f32; 4],
    pub light: u64,
    pub light_pad: [u32; 2],
}

// The shaders' RING_FADE_SECONDS (common.slang) is the streamer's.
const _: () = assert!(voxear_terrain::blocks::RING_FADE_SECONDS == 0.6);

/// A world of blocks' flag for being drawn smooth: its pool holds triangles'
/// corners, two pairs of words each, in place of quads.
pub const BLOCKS_SMOOTH: u32 = 1;

/// A world of blocks' flag for being drawn turned about its corner: blocks
/// that came loose (`voxear_terrain::blocks::fell::LooseBlocks`).
pub const BLOCKS_TURNED: u32 = 2;

/// A world of blocks' flag for its faces being lit by the light kept for
/// its blocks (see [`BlockLight`]).
pub const BLOCKS_LIT: u32 = 4;

/// How much of the sky's light from all round a face with no sky light at
/// all takes, lit by the light kept for its blocks: a cave is dark, but
/// not black.
pub const DARKEST_SKY: f32 = 0.012;

/// The light word of a quad of a chunk made with none: open sky at every
/// corner, and no block light.
const OPEN_SKY_WORD: u32 = 0xF0F0_F0F0;

/// Where a view's block buffer keeps its surfaces, after its worlds.
const SURFACES_AT: usize = MAX_BLOCK_WORLDS * size_of::<GpuBlockWorld>();

/// Where a view's block buffer keeps its worlds' motions and moving
/// blocks, after their surfaces, and how much each world takes there.
const PARTS_AT: usize = SURFACES_AT + MAX_BLOCK_WORLDS * MAX_BLOCK_SURFACES * size_of::<GpuTerrainSurface>();
const PARTS_EACH: usize = MAX_MOTIONS * size_of::<GpuBlockMotion>() + MAX_MOVING * size_of::<GpuMovingBlock>();

/// How many blocks along x, y and z the light round the camera a view's
/// shaders read is (see [`GpuLightVolume`]), and how many it moves by at a
/// time as the camera goes.
pub const LIGHT_VOLUME: [i32; 3] = [96, 48, 96];
const LIGHT_VOLUME_STEP: i32 = 8;
const LIGHT_VOLUME_BLOCKS: usize = (LIGHT_VOLUME[0] * LIGHT_VOLUME[1] * LIGHT_VOLUME[2]) as usize;

/// Where a view's block buffer keeps the light round its camera: its
/// header, then its blocks' light, a byte each.
const LIGHT_VOLUME_AT: usize = PARTS_AT + MAX_BLOCK_WORLDS * PARTS_EACH;
pub(crate) const LIGHT_BYTES_AT: usize = LIGHT_VOLUME_AT + size_of::<GpuLightVolume>();

/// How large a view's block buffer is.
pub const FRAME_SIZE: u64 = (LIGHT_BYTES_AT + LIGHT_VOLUME_BLOCKS) as u64;

// The shaders read worlds, and the light round the camera, laid out on whole sixteen bytes.
const _: () = assert!(size_of::<GpuBlockWorld>() % 16 == 0 && LIGHT_VOLUME_AT % 16 == 0);

/// Where a view's light round its camera starts, and its blocks' light, in
/// its block buffer at `address`.
pub(crate) fn light_volume_addresses(address: u64) -> [u64; 2] {
    [address + LIGHT_VOLUME_AT as u64, address + LIGHT_BYTES_AT as u64]
}

/// The light kept for the blocks round the camera as the shaders read it
/// (`BlockLightVolume` in `shaders/common.slang`): of the world of cubes
/// lit by it that the camera is in, from its block `origin`, whose lowest
/// corner is at `corner` in the view's space, blocks `block_size` a side,
/// [`LIGHT_VOLUME`] blocks along x, y and z, a byte each after it (see
/// `voxear_terrain::blocks::light::volume`); `found` 0 for none. Things
/// that are not blocks, the mist among them, are lit by it: dark in caves,
/// and warm by blocks that give light, `glow` (linear, and over it how much
/// of the sky's light the darkest takes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLightVolume {
    pub origin: [i32; 3],
    pub found: u32,
    pub corner: [f32; 3],
    pub block_size: f32,
    pub glow: [f32; 4],
}

/// One chunk in a pool: the faces it was made from, its slot, where its
/// quads start, the box its faces lie in, and its whole cube, in metres
/// from the world's corner.
struct Held {
    mesh: Arc<ChunkMesh>,
    slot: u32,
    first: u64,
    bounds: [Vec3; 2],
    cube: [Vec3; 2],
    /// How many of each facing's quads are cut out, last in it.
    cut: [u32; FACINGS],
}

/// A chunk's quads as its pool holds them, each with its slot: in each
/// facing, those of a solid surface first and those of a surface `cut`
/// out from its pictures last, as leaves are, which the sun's shadows draw
/// apart (see [`record_shadows`]); and how many are cut out in each. Drawn
/// smooth, its corners' words as they are, `cut` empty.
/// And each quad's corners' light, in the same order, open sky for a chunk
/// made with none.
fn arranged(mesh: &ChunkMesh, slot: u32, cut: &[bool]) -> (Vec<[u32; 2]>, Vec<u32>, [u32; FACINGS]) {
    let mut quads: Vec<[u32; 2]> = mesh.quads.iter().map(|&[a, b]| [a, b | slot << 16]).collect();
    let mut light = match mesh.light.len() == mesh.quads.len() {
        true => mesh.light.clone(),
        false => vec![OPEN_SKY_WORD; mesh.quads.len()],
    };
    let mut counts = [0; FACINGS];
    if !cut.iter().any(|&cut| cut) {
        return (quads, light, counts);
    }
    // As the shaders read a quad's surface: the last where it has more than the world.
    let is_cut = |quad: &[u32; 2]| cut[usize::from(quad[1] as u8).min(cut.len() - 1)];
    for (facing, count) in counts.iter_mut().enumerate() {
        let range = mesh.buckets[facing] as usize..mesh.buckets[facing + 1] as usize;
        let mut run: Vec<([u32; 2], u32)> =
            quads[range.clone()].iter().copied().zip(light[range.clone()].iter().copied()).collect();
        run.sort_by_key(|(quad, _)| is_cut(quad));
        *count = run.iter().filter(|(quad, _)| is_cut(quad)).count() as u32;
        for ((quad, lit), at) in run.into_iter().zip(range) {
            (quads[at], light[at]) = (quad, lit);
        }
    }
    (quads, light, counts)
}

/// The box chunk `mesh`'s faces lie in, in its blocks from its corner: its
/// ground's faces, and the blocks its sheets of leaves and vines and its
/// blocks of parts stand in. `None` where it has none of those, as a chunk
/// of water alone, whose water lies anywhere in its cube.
fn faces_box(mesh: &ChunkMesh) -> Option<[Vec3; 2]> {
    let block = |word: u32| Vec3::new((word & 31) as f32, (word >> 5 & 31) as f32, (word >> 10 & 31) as f32);
    let (first, count) = mesh.loose();
    let loose = mesh.quads[first as usize..(first + count) as usize].iter().map(|&[word, _]| block(word));
    let (first, words) = mesh.parts();
    let parts = mesh.quads[first as usize..(first + words) as usize]
        .chunks_exact(2)
        .map(|pair| Vec3::from(voxear_terrain::blocks::mesh::unpack_part([pair[0], pair[1]]).at.map(|v| v as f32)));
    let blocks = loose.chain(parts).map(|at| [at, at + Vec3::ONE]);
    let ground = mesh.positions.iter().map(|&point| [Vec3::from(point); 2]);
    ground.chain(blocks).reduce(|[low, high], [from, to]| [low.min(from), high.max(to)])
}

/// One chunk as a view picks what to draw of it: its level and place, its
/// box, where each facing's quads start in the pool and how many, +x
/// first, and its water's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChunkDraw {
    pub key: [i32; 4],
    /// The box its faces lie in, and its whole cube, which its water may
    /// fill anywhere.
    pub bounds: [Vec3; 2],
    pub cube: [Vec3; 2],
    pub facings: [[u32; 2]; FACINGS],
    /// How many of each facing's quads are of a surface cut out from its
    /// pictures, last among them (see [`arranged`]).
    pub cut: [u32; FACINGS],
    /// Its loose quads, sheets of bushy leaves and of vines, drawn
    /// whichever way the view looks while it is near enough.
    pub loose: [u32; 2],
    /// Its blocks of parts' quads, two quads' words each, drawn whichever
    /// way the view looks.
    pub parts: [u32; 2],
    pub water: [u32; 2],
    /// Drawn smooth, the cone the ways its triangles face lie within (see
    /// `voxear_terrain::blocks::mesh::ChunkMesh::cone`), [`EVERY_WAY`] where
    /// they face every way; `None` for cubes, kept by the way they face.
    pub cone: Option<[f32; 4]>,
    /// Which of its meshes it is drawn from, the same while its blocks are,
    /// wherever in the pool they lie (see `shadow_cache`).
    pub mesh: usize,
}

/// How far from the camera a chunk's loose quads are drawn, in blocks
/// (`BLOCK_LOOSE_REACH` in `shaders/block_quads.slang`, which shrinks a
/// sheet of leaves away before it).
pub const LOOSE_REACH: f32 = 160.0;

/// The cone of a smooth chunk whose triangles face every way: wider than
/// any turn, so never turned away.
const EVERY_WAY: [f32; 4] = [0.0, 1.0, 0.0, 2.0];

/// How many columns of chunks a side a group has (see [`ChunkGroup`]), as a
/// power of two.
const GROUP_SHIFT: i32 = 2;

/// Neighbouring chunks of a world, of one level and in a square of
/// columns [`GROUP_SHIFT`] across, as a view looks at them first: where
/// they are in the world's list of chunks, the box all their faces lie in
/// and the one their cubes fill. A view passes over a group out of sight
/// whole, and takes a group wholly in sight without looking at each chunk.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChunkGroup {
    pub chunks: std::ops::Range<u32>,
    pub bounds: [Vec3; 2],
    pub cube: [Vec3; 2],
}

/// The group a chunk is in, by its level and its column's square, as one
/// number to sort by.
fn group_of([level, x, _, z]: [i32; 4]) -> u64 {
    let square = |v: i32| u64::from((v >> GROUP_SHIFT) as u32 & 0xFF_FFFF);
    (u64::from(level as u32 & 0xFFFF) << 48) | (square(x) << 24) | square(z)
}

/// The groups of `chunks`, which are in order by their groups.
fn groups(chunks: &[ChunkDraw]) -> Vec<ChunkGroup> {
    let mut groups: Vec<ChunkGroup> = Vec::new();
    let mut last = None;
    for (index, chunk) in chunks.iter().enumerate() {
        let key = group_of(chunk.key);
        match groups.last_mut() {
            Some(group) if last == Some(key) => {
                group.chunks.end = index as u32 + 1;
                group.bounds = [group.bounds[0].min(chunk.bounds[0]), group.bounds[1].max(chunk.bounds[1])];
                group.cube = [group.cube[0].min(chunk.cube[0]), group.cube[1].max(chunk.cube[1])];
            }
            _ => groups.push(ChunkGroup {
                chunks: index as u32..index as u32 + 1,
                bounds: chunk.bounds,
                cube: chunk.cube,
            }),
        }
        last = Some(key);
    }
    groups
}

/// `chunks` as one group, for picking from a list of them made in passing.
fn one_group(chunks: &[ChunkDraw]) -> [ChunkGroup; 1] {
    let union = |of: fn(&ChunkDraw) -> [Vec3; 2]| {
        chunks.iter().map(of).reduce(|[low, high], [from, to]| [low.min(from), high.max(to)]).unwrap_or([Vec3::ZERO; 2])
    };
    [ChunkGroup { chunks: 0..chunks.len() as u32, bounds: union(|c| c.bounds), cube: union(|c| c.cube) }]
}

/// How much of a box is within `planes`, inward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InSight {
    Outside,
    Partly,
    Wholly,
}

/// How much of the box from `low` to `high` is within `planes`: none of it
/// where its corner furthest along a plane's way in is behind it, all of it
/// where its nearest is in front of every one.
fn in_sight(planes: &[[f32; 4]], [low, high]: [Vec3; 2]) -> InSight {
    let mut wholly = true;
    for &[x, y, z, d] in planes {
        let pick = |way: f32, low: f32, high: f32| if way >= 0.0 { (high, low) } else { (low, high) };
        let ((fx, nx), (fy, ny), (fz, nz)) = (pick(x, low.x, high.x), pick(y, low.y, high.y), pick(z, low.z, high.z));
        if x * fx + y * fy + z * fz + d < 0.0 {
            return InSight::Outside;
        }
        wholly &= x * nx + y * ny + z * nz + d >= 0.0;
    }
    if wholly { InSight::Wholly } else { InSight::Partly }
}

/// Whether the box from `low` to `high` is within `planes`, inward.
fn within(planes: &[[f32; 4]], [low, high]: [Vec3; 2]) -> bool {
    planes.iter().all(|&[x, y, z, d]| {
        let corner = Vec3::new(
            if x >= 0.0 { high.x } else { low.x },
            if y >= 0.0 { high.y } else { low.y },
            if z >= 0.0 { high.z } else { low.z },
        );
        x * corner.x + y * corner.y + z * corner.z + d >= 0.0
    })
}

/// `planes`, inward, from the world's space into that of a world of blocks
/// whose corner is at `corner` and which is turned back into its own space
/// by `back`.
fn turned_planes(planes: &[[f32; 4]], corner: Vec3, back: Quat) -> Vec<[f32; 4]> {
    planes
        .iter()
        .map(|&[x, y, z, d]| {
            let normal = Vec3::new(x, y, z);
            let turned = back * normal;
            [turned.x, turned.y, turned.z, d + normal.dot(corner)]
        })
        .collect()
}

/// Which of a chunk's faces a view draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Facing {
    /// Those turned towards a camera at this point, in the world's space.
    Camera(Vec3),
    /// Those turned towards light going this way: ground is closed all
    /// round, so a ray of it meets one of those first, and they alone cast
    /// its shadows.
    Light(Vec3),
}

/// How finely a view draws smooth chunks: from where, in the world's space,
/// how far their surface may move for each metre they are away, and how
/// large the world's blocks are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Detail {
    pub camera: Vec3,
    pub allowed: f32,
    pub block_size: f32,
    /// Where a world without end's rings fade into each other: where they
    /// are round, along x and z in its space, how far its own blocks reach,
    /// and its coarsest ring's level. A chunk wholly past where its ring has
    /// faded out is left out (see [`Detail::past_its_ring`]).
    pub rings: Option<([f32; 2], f32, i32)>,
}

impl Detail {
    /// Whether every face of `chunk` lies past where its ring has faded out
    /// to nothing, the ring outside it drawn there in its place.
    fn past_its_ring(&self, chunk: &ChunkDraw) -> bool {
        let Some((at, reach, coarsest)) = self.rings else { return false };
        let level = chunk.key[0];
        let [low, high] = chunk.bounds;
        let nearest = |axis: usize, at: f32| (low[axis] - at).max(at - high[axis]).max(0.0);
        level < coarsest && nearest(0, at[0]).max(nearest(2, at[1])) > reach * (1u32 << level.clamp(0, 16)) as f32
    }

    /// The coarsest of a smooth chunk's levels (see
    /// `voxear_terrain::blocks::smooth::LEVELS`) whose surface moves no more
    /// than allowed for how far away it is, of those it has.
    fn level(&self, chunk: &ChunkDraw) -> usize {
        let [low, high] = chunk.bounds;
        let distance = (self.camera.clamp(low, high) - self.camera).length();
        let block = self.block_size * (1u32 << chunk.key[0].clamp(0, 16)) as f32;
        (1..smooth::LEVELS)
            .rev()
            .find(|&level| chunk.facings[level][1] > 0 && smooth::COARSER[level - 1] * block <= self.allowed * distance)
            .unwrap_or(0)
    }
}

/// [`pick_grouped`] from a list of chunks as one group, all its quads.
#[cfg(test)]
pub(crate) fn pick(
    chunks: &[ChunkDraw],
    planes: &[[f32; 4]],
    (facing, detail): (Facing, Option<Detail>),
    seen: Option<&CoordSet<[i32; 3]>>,
    shown: Option<&CoordSet<[i32; 3]>>,
) -> Vec<[u32; 2]> {
    pick_parts(chunks, planes, (facing, detail), (seen, shown), Parts::With, None)
}

/// A world's chunks, in order by their groups, and the groups.
type Grouped<'a> = (&'a [ChunkDraw], &'a [ChunkGroup]);

/// Which of a chunk's quads a list draws: all, all but its blocks of
/// parts, or those alone, as a lamp's kept shadow draws the rest once and
/// them, which may move, each frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Parts {
    With,
    Without,
    Only,
}

/// [`pick_grouped`] from a list of chunks as one group.
#[cfg(test)]
fn pick_parts(
    chunks: &[ChunkDraw],
    planes: &[[f32; 4]],
    facing: (Facing, Option<Detail>),
    sights: (Option<&CoordSet<[i32; 3]>>, Option<&CoordSet<[i32; 3]>>),
    parts: Parts,
    near_first: Option<Vec3>,
) -> Vec<[u32; 2]> {
    pick_grouped((chunks, &one_group(chunks)), planes, facing, sights, parts, near_first)
}

/// What a view draws of a world's chunks: those within `planes` (inward,
/// in the world's space) and, where `seen` says which of its own chunks the
/// camera can see into, those; and of each, the faces `facing` picks, and of
/// a smooth one the level `detail` picks, with only the quads `parts` says;
/// as first quads and counts, those that follow on in the pool drawn as one.
/// A group out of sight is passed over whole, and the chunks of one wholly
/// in sight are not each looked at again.
///
/// Where `near_first`, the camera, the chunks nearest it come first, so the
/// depth pass draws what is in front before what it hides, and the GPU
/// throws away hidden pixels before working out their surface. A chunk's
/// own runs stay in the pool's order, so a quad laid over another of its
/// chunk is still drawn after it.
fn pick_grouped(
    (chunks, groups): Grouped,
    planes: &[[f32; 4]],
    (facing, detail): (Facing, Option<Detail>),
    (seen, shown): (Option<&CoordSet<[i32; 3]>>, Option<&CoordSet<[i32; 3]>>),
    parts: Parts,
    near_first: Option<Vec3>,
) -> Vec<[u32; 2]> {
    // Turned towards the light, the faces whose way out goes against it.
    let lit = match facing {
        Facing::Light(way) => [way.x <= 0.0, way.x >= 0.0, way.y <= 0.0, way.y >= 0.0, way.z <= 0.0, way.z >= 0.0],
        _ => [true; FACINGS],
    };
    let take = |chunk: &ChunkDraw, whole: bool, out: &mut Vec<[u32; 2]>| {
        let [level, x, y, z] = chunk.key;
        // Only the columns shown from the place the view looks from, where the world says which.
        if shown.is_some_and(|shown| !shown.contains(&[level, x, z])) {
            return;
        }
        if level == 0 && seen.is_some_and(|seen| !seen.contains(&[x, y, z])) {
            return;
        }
        if !(whole || within(planes, chunk.bounds)) || detail.is_some_and(|detail| detail.past_its_ring(chunk)) {
            return;
        }
        let [low, high] = chunk.bounds;
        // Its blocks of parts, their boxes' faces turned every way, and some moving.
        if level == 0 && chunk.cone.is_none() && chunk.parts[1] > 0 && parts != Parts::Without {
            out.push(chunk.parts);
        }
        if parts == Parts::Only {
            return;
        }
        // A smooth chunk is drawn whole at one of its levels, unless every triangle in it is turned away.
        if let Some(cone) = chunk.cone {
            let [first, count] = chunk.facings[detail.map_or(0, |detail| detail.level(chunk))];
            if count > 0 && !turned_away(cone, chunk.bounds, facing) {
                out.push([first, count]);
            }
            return;
        }
        // A face is turned towards the camera when the camera is past its plane, along the way it faces.
        let toward = match facing {
            Facing::Camera(camera) => [
                camera.x > low.x,
                camera.x < high.x,
                camera.y > low.y,
                camera.y < high.y,
                camera.z > low.z,
                camera.z < high.z,
            ],
            _ => lit,
        };
        for (facing, &[first, count]) in chunk.facings.iter().enumerate() {
            if count > 0 && toward[facing] {
                out.push([first, count]);
            }
        }
        // Its sheets of leaves and vines, seen from every side, while the camera is near.
        let near = |detail: Detail| {
            let reach = LOOSE_REACH * detail.block_size;
            (detail.camera.clamp(low, high) - detail.camera).length_squared() <= reach * reach
        };
        if level == 0 && chunk.loose[1] > 0 && detail.is_none_or(near) {
            out.push(chunk.loose);
        }
    };
    // Nearest first, each chunk drawn is put in order by how far it is, its own runs together: only those drawn.
    let away = |chunk: &ChunkDraw, camera: Vec3| ((chunk.bounds[0] + chunk.bounds[1]) * 0.5).distance_squared(camera);
    let mut picked: Vec<[u32; 2]> = Vec::new();
    let mut order: Vec<(f32, u32, u32)> = Vec::new();
    for group in groups {
        let sight = in_sight(planes, group.bounds);
        if sight == InSight::Outside {
            continue;
        }
        let whole = sight == InSight::Wholly;
        for chunk in &chunks[group.chunks.start as usize..group.chunks.end as usize] {
            let start = picked.len();
            take(chunk, whole, &mut picked);
            if let Some(camera) = near_first.filter(|_| picked.len() > start) {
                picked[start..].sort_unstable();
                order.push((away(chunk, camera), start as u32, picked.len() as u32));
            }
        }
    }
    if near_first.is_none() {
        return merged(picked);
    }
    order.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut nearest_first = Vec::with_capacity(picked.len());
    for (_, start, end) in order {
        nearest_first.extend_from_slice(&picked[start as usize..end as usize]);
    }
    joined(nearest_first)
}

/// Whether every triangle facing within `cone` (its axis and the sine of
/// its angle, in a world's space) somewhere in the box `bounds` is turned
/// away as `facing` sees it: from a camera, or from light.
fn turned_away([x, y, z, spread]: [f32; 4], [low, high]: [Vec3; 2], facing: Facing) -> bool {
    let axis = Vec3::new(x, y, z);
    match facing {
        // Going the way the light goes, by more than the cone is wide.
        Facing::Light(way) => axis.dot(way.normalize_or_zero()) > spread,
        // The camera behind every triangle wherever in the box it lies: the box seen from the camera within a
        // quarter turn of the axis, less the cone's width and the box's.
        Facing::Camera(camera) => {
            let (middle, radius) = ((low + high) * 0.5, (high - low).length() * 0.5);
            let away = middle - camera;
            let distance = away.length();
            distance > radius && axis.dot(away / distance) > spread + radius / distance
        }
    }
}

/// [`pick_water_grouped`] from a list of chunks as one group, all kept.
#[cfg(test)]
pub(crate) fn pick_water(
    chunks: &[ChunkDraw],
    planes: &[[f32; 4]],
    seen: Option<&CoordSet<[i32; 3]>>,
    shown: Option<&CoordSet<[i32; 3]>>,
) -> Vec<[u32; 2]> {
    pick_water_grouped((chunks, &one_group(chunks)), planes, (seen, shown), |_| true)
}

/// The water a view draws of a world's chunks in their groups: every face
/// of the water of the chunks it sees, as [`pick_grouped`] picks them, of
/// those `kept` keeps.
fn pick_water_grouped(
    (chunks, groups): Grouped,
    planes: &[[f32; 4]],
    (seen, shown): (Option<&CoordSet<[i32; 3]>>, Option<&CoordSet<[i32; 3]>>),
    kept: impl Fn(&ChunkDraw) -> bool,
) -> Vec<[u32; 2]> {
    let mut picked = Vec::new();
    for group in groups {
        let sight = in_sight(planes, group.cube);
        if sight == InSight::Outside {
            continue;
        }
        for chunk in &chunks[group.chunks.start as usize..group.chunks.end as usize] {
            let [level, x, y, z] = chunk.key;
            if chunk.water[1] > 0
                && shown.is_none_or(|shown| shown.contains(&[level, x, z]))
                && (level != 0 || seen.is_none_or(|seen| seen.contains(&[x, y, z])))
                && (sight == InSight::Wholly || within(planes, chunk.cube))
                && kept(chunk)
            {
                picked.push(chunk.water);
            }
        }
    }
    merged(picked)
}

/// The runs of `chunks`' quads cut out from their pictures, in the order
/// their pool holds them: the last of each facing's (see [`arranged`]), and
/// every sheet of leaves and block of parts.
fn cut_runs(chunks: &[ChunkDraw]) -> Vec<[u32; 2]> {
    let mut cut: Vec<[u32; 2]> = Vec::new();
    for chunk in chunks.iter().filter(|chunk| chunk.cone.is_none()) {
        for (&[first, count], &cut_count) in chunk.facings.iter().zip(&chunk.cut) {
            if cut_count > 0 {
                cut.push([first + count - cut_count, cut_count]);
            }
        }
        for run in [chunk.loose, chunk.parts] {
            if run[1] > 0 {
                cut.push(run);
            }
        }
    }
    cut.sort_unstable();
    cut
}

/// Runs of quads `picked` (see [`pick`]) as two lists of runs: those whose
/// quads may all be drawn as depth alone, and those within `cut` (see
/// [`cut_runs`]).
fn cut_apart(cut: &[[u32; 2]], picked: Vec<[u32; 2]>) -> (Vec<[u32; 2]>, Vec<[u32; 2]>) {
    let (mut solid, mut cut_out) = (Vec::new(), Vec::new());
    for [mut first, count] in picked {
        let end = first + count;
        // The cut runs that start before this run ends, from the last that starts at or before its first.
        let from = cut.partition_point(|&[start, _]| start <= first).saturating_sub(1);
        for &[start, length] in cut[from..].iter().take_while(|&&[start, _]| start < end) {
            let (start, stop) = (start.max(first), (start + length).min(end));
            if start >= stop {
                continue;
            }
            if start > first {
                solid.push([first, start - first]);
            }
            cut_out.push([start, stop - start]);
            first = stop;
        }
        if first < end {
            solid.push([first, end - first]);
        }
    }
    (solid, cut_out)
}

/// Runs of quads in the order given, those that follow on drawn as one.
fn joined(picked: Vec<[u32; 2]>) -> Vec<[u32; 2]> {
    let mut joined: Vec<[u32; 2]> = Vec::with_capacity(picked.len());
    for [first, count] in picked {
        match joined.last_mut() {
            Some(last) if last[0] + last[1] == first => last[1] += count,
            _ => joined.push([first, count]),
        }
    }
    joined
}

/// Runs of quads in order, those that follow on drawn as one.
fn merged(mut picked: Vec<[u32; 2]>) -> Vec<[u32; 2]> {
    picked.sort_unstable();
    let mut merged: Vec<[u32; 2]> = Vec::with_capacity(picked.len());
    for [first, count] in picked {
        match merged.last_mut() {
            Some(last) if last[0] + last[1] == first => last[1] += count,
            _ => merged.push([first, count]),
        }
    }
    merged
}

/// A chunk's mesh for rays: the chunk it was made from, how many vertices
/// and indices it takes, which of its world's surfaces' changes it was
/// painted after (see `Pool::painted`), and its most common surface, worked
/// out once for how it sounds (see `ACOUSTIC_ROCK`).
type Proxy = (Arc<ChunkMesh>, MeshHandle, [usize; 2], u32, Option<usize>);

/// One world's chunks on the GPU: one buffer, each slot's chunk's place in
/// blocks, then the quads (or, drawn smooth, its triangles' corners); and
/// each chunk's mesh for rays.
struct Pool {
    buffer: Buffer,
    space: RangeAllocator,
    slots: SlotAllocator,
    capacity: u64,
    /// By level (0 for the world's own chunks, more for a world without
    /// end's coarser ones) and place in that level's chunks.
    held: HashMap<[i32; 4], Held>,
    /// Ranges and slots given up, freed once the frames that may draw them
    /// are done.
    freeing: Vec<(u64, u64, u64, u32)>,
    proxies: HashMap<[i32; 3], Proxy>,
    /// Its chunks to make meshes for rays for, the nearest the camera first,
    /// each with how far off it is, in chunks; sorted again as the world's
    /// chunks or places, or the chunk the camera is in, change from those
    /// `ordered` holds.
    order: Vec<([i32; 3], f32)>,
    /// How many of the first in `order` have their meshes for rays made from
    /// the world's chunks as they are: those farther off give theirs up
    /// first, so a frame goes on from there.
    made_to: usize,
    ordered: Option<(Arc<ChunkMeshes>, Arc<Vec<voxear_terrain::blocks::BlockPlace>>, Option<[i32; 3]>)>,
    /// The mesh pool had no room for its nearest chunk without a mesh for
    /// rays, and none with one was far enough beyond it to give theirs up:
    /// tried again once the camera is in another chunk, or its faces change.
    full: bool,
    /// The frame from which the room its farthest chunks' meshes for rays
    /// gave up is free again, once the frames tracing them are done.
    room_at: u64,
    block_size: f32,
    smooth: bool,
    /// Its chunks to draw, in their groups, made again when it changes.
    draws: Option<(Arc<[ChunkDraw]>, Arc<[ChunkGroup]>)>,
    /// Which of each of its own chunks' sides see each other, made again
    /// with its draws.
    sights: Option<Arc<HashMap<[i32; 3], u16>>>,
    seen: u64,
    /// The world's faces, its own and coarser, as the pool was last brought
    /// up to them, and as every one of its own had its mesh for rays made.
    made_from: Option<(Arc<ChunkMeshes>, Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>)>,
    proxied: Option<(Arc<ChunkMeshes>, Arc<Vec<voxear_terrain::blocks::BlockPlace>>)>,
    /// Its chunks' meshes for rays, each with where its chunk is from the
    /// world's corner, made again as they change.
    placed: Option<Arc<[(MeshHandle, Vec3, u32)]>>,
    /// The world as it last drew it, drawn in place of the pool that follows
    /// it until that one has every chunk.
    shown: Option<DrawnBlocks>,
    /// Its tints as last brought up, where it has any: their points, from
    /// which point and how many a side.
    tinted: Option<(Arc<[u32]>, [i32; 2], u32, Option<[i32; 2]>)>,
    /// Which of its surfaces are cut out from their pictures, as leaves
    /// are, as its chunks' quads were last put in order by (see
    /// [`arranged`]).
    cut: Arc<[bool]>,
    /// The surfaces its chunks' meshes for rays are painted with, and how
    /// many times they have changed: a mesh for rays painted before the
    /// last change is made again.
    painted: (Arc<[SurfaceLook]>, u32),
    /// How its world's rings fade into each other, as its chunks' fades
    /// were last brought up by.
    faded: Arc<RingFades>,
    /// The light round the camera as last taken: from the world's blocks
    /// and light, from which block, and each block's.
    volume: Option<(Arc<voxear_terrain::blocks::BlockMap>, Arc<LightMap>, [i32; 3], Arc<[u8]>)>,
}

impl Pool {
    /// Its quads, then their light, a word each, as many as it has room for.
    fn create(gpu: &Gpu, capacity: u64) -> Result<Buffer> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        gpu.create_buffer("block quads", QUADS_AT + capacity * 12, usage, MemoryLocation::GpuOnly)
    }

    /// Where its quads' light starts.
    fn light_at(&self) -> u64 {
        QUADS_AT + self.capacity * 8
    }

    fn new(gpu: &Gpu, block_size: f32, smooth: bool) -> Result<Self> {
        Ok(Self {
            buffer: Self::create(gpu, FIRST_CAPACITY)?,
            space: RangeAllocator::new(FIRST_CAPACITY),
            slots: SlotAllocator::new(SLOTS),
            capacity: FIRST_CAPACITY,
            held: HashMap::new(),
            freeing: Vec::new(),
            proxies: HashMap::new(),
            order: Vec::new(),
            made_to: 0,
            ordered: None,
            full: false,
            room_at: 0,
            block_size,
            smooth,
            draws: None,
            sights: None,
            cut: Arc::from([]),
            painted: (Arc::from([]), 0),
            faded: Arc::default(),
            seen: 0,
            made_from: None,
            tinted: None,
            proxied: None,
            placed: None,
            shown: None,
            volume: None,
        })
    }

    /// Brings the pool up to `world`'s chunks: stages the quads of each
    /// that changed, by pointer, with `stage(bytes, to)`, growing the pool
    /// once for all of them. It stages no more than the frame's bytes `left`
    /// (see `WorldBudget::upload`), past its first chunk; each of the rest
    /// shows as it was until its frame comes, and those the world no longer
    /// has go once none is left. Whether the pool is up to the world.
    fn update(
        &mut self,
        gpu: &Gpu,
        frames: &mut Frames,
        world: &BlockWorld,
        (frame_index, left): (u64, &mut u64),
        stage: &mut dyn FnMut(&[u8], u64),
    ) -> Result<bool> {
        let free_by = frame_index.saturating_sub(FRAMES_IN_FLIGHT as u64);
        let (done, waiting): (Vec<_>, Vec<_>) = self.freeing.drain(..).partition(|(when, ..)| *when <= free_by);
        self.freeing = waiting;
        for (_, first, count, slot) in done {
            self.space.free(first, count);
            self.slots.free(slot);
        }
        // Nothing to bring up where its faces are those it was last brought up to.
        let same = |(meshes, far): &(Arc<ChunkMeshes>, Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>)| {
            Arc::ptr_eq(meshes, &world.meshes) && Arc::ptr_eq(far, &world.far)
        };
        if self.made_from.as_ref().is_some_and(same) {
            return Ok(true);
        }
        let side = CHUNK as f32 * world.block_size;
        let near = world.meshes.iter().map(|(k, mesh)| ([0, k[0], k[1], k[2]], mesh));
        let held = &self.held;
        let coming: Vec<([i32; 4], &Arc<ChunkMesh>)> = near
            .chain(world.far.iter().map(|(k, mesh)| (*k, mesh)))
            .filter(|(key, mesh)| !held.get(key).is_some_and(|held| Arc::ptr_eq(&held.mesh, mesh)))
            .collect();
        // Grown at once to hold every chunk coming, so those held are staged again once, not at each doubling.
        let quads = |count: u64, held: &Held| count + held.mesh.quads.len() as u64;
        let used = self.held.values().fold(0, quads) + self.freeing.iter().map(|freed| freed.2).sum::<u64>();
        let wanted: u64 = coming.iter().map(|(_, mesh)| mesh.quads.len() as u64).sum();
        if used + wanted > self.capacity {
            self.grow(gpu, frames, wanted, stage)?;
        }
        // As many as the frame stages, at least one; the rest come over the next frames, each shown as it was till then.
        let mut changed = false;
        let mut whole = true;
        for (key, mesh) in coming {
            let count = mesh.quads.len() as u64;
            if changed && count * 8 > *left {
                whole = false;
                break;
            }
            *left = left.saturating_sub(count * 8);
            changed = true;
            if let Some(old) = self.held.remove(&key) {
                self.freeing.push((frame_index, old.first, old.mesh.quads.len() as u64, old.slot));
            }
            let Some(slot) = self.slots.alloc() else {
                tracing::warn!("a world of blocks has more chunks than {SLOTS}; the rest are left out");
                continue;
            };
            let first = match self.space.alloc(count) {
                Some(first) => first,
                None => {
                    self.grow(gpu, frames, count, stage)?;
                    self.space.alloc(count).expect("room after growing")
                }
            };
            let (quads, light, cut) = arranged(mesh, slot, &self.cut);
            stage(bytemuck::cast_slice(&quads), QUADS_AT + first * 8);
            stage(bytemuck::cast_slice(&light), self.light_at() + first * 4);
            // The world's own blocks drawn past where they are made, at level -1, drawn as its own are.
            stage(bytemuck::cast_slice(&[key[1], key[2], key[3], key[0].max(0)]), u64::from(slot) * 16);
            stage(bytemuck::cast_slice(&fade_times(&world.fades, key)), FADES_AT + u64::from(slot) * 8);
            // A coarser level's chunk is as many times larger as its blocks are; drawn smooth, its surface
            // reaches up to two blocks before it and past it.
            let scale = (1u32 << key[0].clamp(0, 16)) as f32;
            let side = side * scale;
            let low = Vec3::new(key[1] as f32, key[2] as f32, key[3] as f32) * side;
            let margin = if self.smooth { Vec3::splat(2.0 * self.block_size * scale) } else { Vec3::ZERO };
            let cube = [low - margin, low + Vec3::splat(side) + margin];
            // Where its faces are, a block more each way for leaves that sway and doors that swing: often a thin
            // layer of a coarse chunk's cube, hundreds of metres high, which the sun's cascades would take whole.
            let block = world.block_size * scale;
            let bounds = faces_box(mesh).map_or(cube, |[from, to]| {
                let grown = margin + Vec3::splat(block);
                [(low + from * block - grown).max(cube[0]), (low + to * block + grown).min(cube[1])]
            });
            self.held.insert(key, Held { mesh: mesh.clone(), slot, first, bounds, cube, cut });
        }
        // Those the world no longer has go once every one coming has come, so none leaves a gap till then.
        if whole {
            let before = self.held.len();
            let freeing = &mut self.freeing;
            let has = |key: &[i32; 4]| match key[0] {
                0 => world.meshes.contains_key(&[key[1], key[2], key[3]]),
                _ => world.far.contains_key(key),
            };
            self.held.retain(|key, held| {
                let kept = has(key);
                if !kept {
                    freeing.push((frame_index, held.first, held.mesh.quads.len() as u64, held.slot));
                }
                kept
            });
            changed |= self.held.len() != before;
            self.made_from = Some((world.meshes.clone(), world.far.clone()));
        }
        if changed {
            self.draws = None;
            self.sights = None;
        }
        Ok(whole)
    }

    /// A buffer twice as large, or more where `wanted` more quads need it,
    /// with every chunk held staged into it again.
    fn grow(&mut self, gpu: &Gpu, frames: &mut Frames, wanted: u64, stage: &mut dyn FnMut(&[u8], u64)) -> Result<()> {
        let held: u64 = self.held.values().map(|held| held.mesh.quads.len() as u64).sum();
        let mut capacity = self.capacity * 2;
        while capacity < held + wanted {
            capacity *= 2;
        }
        let old = std::mem::replace(&mut self.buffer, Self::create(gpu, capacity)?);
        frames.retire(Retired::Buffer(old));
        self.capacity = capacity;
        self.space = RangeAllocator::new(capacity);
        self.made_from = None;
        // Its tints are brought up again into the new buffer.
        self.tinted = None;
        // Slots given up wait no longer: nothing draws from the old buffer's quads now.
        for (_, _, _, slot) in self.freeing.drain(..) {
            self.slots.free(slot);
        }
        for (key, held) in &mut self.held {
            held.first = self.space.alloc(held.mesh.quads.len() as u64).expect("room in a larger pool");
            let (quads, light);
            (quads, light, held.cut) = arranged(&held.mesh, held.slot, &self.cut);
            stage(bytemuck::cast_slice(&quads), QUADS_AT + held.first * 8);
            stage(bytemuck::cast_slice(&light), QUADS_AT + capacity * 8 + held.first * 4);
            stage(bytemuck::cast_slice(&[key[1], key[2], key[3], key[0].max(0)]), u64::from(held.slot) * 16);
            stage(bytemuck::cast_slice(&fade_times(&self.faded, *key)), FADES_AT + u64::from(held.slot) * 8);
        }
        self.draws = None;
        self.sights = None;
        Ok(())
    }

    /// Every chunk held put in order again where it is, by which of the
    /// world's surfaces are cut out now, where that changed.
    fn arrange(&mut self, cut: Arc<[bool]>, stage: &mut dyn FnMut(&[u8], u64)) {
        if self.cut == cut {
            return;
        }
        self.cut = cut;
        let light_at = self.light_at();
        for held in self.held.values_mut() {
            let (quads, light);
            (quads, light, held.cut) = arranged(&held.mesh, held.slot, &self.cut);
            stage(bytemuck::cast_slice(&quads), QUADS_AT + held.first * 8);
            stage(bytemuck::cast_slice(&light), light_at + held.first * 4);
        }
        self.draws = None;
    }

    /// Every chunk held given its fades again, where its world's rings fade
    /// otherwise now.
    fn fade(&mut self, fades: &Arc<RingFades>, stage: &mut dyn FnMut(&[u8], u64)) {
        if Arc::ptr_eq(&self.faded, fades) || self.faded == *fades {
            self.faded = fades.clone();
            return;
        }
        self.faded = fades.clone();
        for (key, held) in &self.held {
            stage(bytemuck::cast_slice(&fade_times(fades, *key)), FADES_AT + u64::from(held.slot) * 8);
        }
    }

    /// Which of each of its own chunks' sides see each other, made again
    /// only when the pool changed.
    fn sights(&mut self) -> Arc<HashMap<[i32; 3], u16>> {
        if self.sights.is_none() {
            let own = self.held.iter().filter(|(key, _)| key[0] == 0);
            self.sights = Some(Arc::new(own.map(|(key, held)| ([key[1], key[2], key[3]], held.mesh.sights)).collect()));
        }
        self.sights.clone().expect("made above")
    }

    /// Each chunk to draw, in order by its group, and the groups, made again
    /// only when the pool changed.
    fn draws(&mut self) -> (Arc<[ChunkDraw]>, Arc<[ChunkGroup]>) {
        if self.draws.is_none() {
            let draws = self.held.iter().map(|(key, held)| {
                let buckets = held.mesh.buckets;
                let facings = std::array::from_fn(|f| [held.first as u32 + buckets[f], buckets[f + 1] - buckets[f]]);
                let (water, count) = held.mesh.water();
                let water = [held.first as u32 + water, count];
                let (loose, count) = held.mesh.loose();
                let loose = [held.first as u32 + loose, count];
                let (parts, count) = held.mesh.parts();
                let parts = [held.first as u32 + parts, count];
                let cone = self.smooth.then(|| held.mesh.cone.unwrap_or(EVERY_WAY));
                let mesh = Arc::as_ptr(&held.mesh) as usize;
                ChunkDraw {
                    key: *key,
                    bounds: held.bounds,
                    cube: held.cube,
                    facings,
                    cut: held.cut,
                    loose,
                    parts,
                    water,
                    cone,
                    mesh,
                }
            });
            let mut draws: Vec<ChunkDraw> = draws.collect();
            draws.sort_by_cached_key(|chunk| group_of(chunk.key));
            let groups = groups(&draws);
            self.draws = Some((draws.into(), groups.into()));
        }
        self.draws.clone().expect("made above")
    }
}

/// A world of blocks to draw this frame, as every view draws it.
#[derive(Clone)]
pub(crate) struct DrawnBlocks {
    pub entity: Entity,
    pub corner: Vec3,
    /// How it is turned about its corner: none but for blocks that came
    /// loose, which the GI's rays do not see.
    pub turn: Option<Quat>,
    /// The world whose bounced light it is lit by, for blocks that came
    /// loose from it: its own otherwise.
    pub lit_by: Option<Entity>,
    pub block_size: f32,
    pub smooth: bool,
    /// Where its pool's chunks' places start, and its quads; and where its
    /// quads' light starts.
    pub address: u64,
    pub light: u64,
    pub chunks: Arc<[ChunkDraw]>,
    /// Its chunks in groups of neighbours, as `chunks` holds them in order.
    pub groups: Arc<[ChunkGroup]>,
    /// For a world of cubes, its blocks and which of each chunk's sides see
    /// each other, for each view to pass over the chunks it cannot see into.
    pub sights: Option<(Arc<voxear_terrain::blocks::BlockMap>, Arc<HashMap<[i32; 3], u16>>)>,
    /// What it shows from each place it is seen from, where it is seen from
    /// more than one (see `voxear_terrain::blocks::BlockPlace`).
    pub places: Arc<Vec<voxear_terrain::blocks::BlockPlace>>,
    /// How its rings fade into each other, for a world without end.
    pub fades: Arc<RingFades>,
    pub look: BlockLook,
    /// Its blocks moving now, as the shaders read them.
    pub moving: Arc<[GpuMovingBlock]>,
    /// How high its sea's top is over its corner, where it has a sea level.
    pub sea_top: Option<f32>,
    pub cast_shadows: bool,
    /// Its chunks' meshes for rays, each with where its chunk is from the
    /// world's corner.
    /// Each chunk's mesh for rays, where it is from the world's corner, and
    /// the acoustic preset sound rays hear it as, plus one: its most common
    /// surface's material's, or rock's, as most of a world of blocks is.
    pub proxies: Arc<[(MeshHandle, Vec3, u32)]>,
    /// Its tints in its pool, where it has any: the point they start from
    /// and how many a side.
    pub tints: Option<([i32; 2], u32, Option<[i32; 2]>)>,
    /// How far off its ground takes the colour of the plants on it, and
    /// which surfaces (see [`BlockTints::cover`]).
    pub cover: [f32; 2],
    pub covered: [u32; 8],
    /// Its pool in this frame's graph, for every view to wait on.
    pub id: Option<BufferId>,
    /// Lit by the light kept for its blocks, that light round the camera:
    /// from which block, and each block's (see [`GpuLightVolume`]).
    pub volume: Option<([i32; 3], Arc<[u8]>)>,
    /// The index buffer a world of cubes is drawn with (see [`quad_indices`]).
    pub indices: vk::Buffer,
}

/// A copy into a world's pool: from where in the staging buffer, into
/// whose, where and how much.
#[derive(Clone)]
struct Copy {
    from: u64,
    key: Entity,
    to: u64,
    size: u64,
}

/// Every world of blocks on the GPU, and what to copy in this frame.
#[derive(Default)]
pub(crate) struct BlockWorlds {
    resident: HashMap<Entity, Pool>,
    /// Each world's pool of blocks of another size, drawn until its new
    /// pool has every chunk.
    leaving: HashMap<Entity, Pool>,
    copies: Vec<Copy>,
    staging: Option<Buffer>,
    /// The meshes for rays made this frame: their uploads and bottom levels'
    /// builds, kept for the frame's own commands to do (see `record`).
    rays: Vec<KeptBatch>,
    drawn: Vec<DrawnBlocks>,
    /// Whether it has said that the GI's rays are left without some chunks,
    /// once.
    warned_full: bool,
    /// Every world of cubes' quads as their triangles take their corners
    /// (see [`quad_indices`]), made the first time one is drawn.
    indices: Option<Buffer>,
}

/// The most quads one indexed draw of a world of cubes takes, as many as
/// [`quad_indices`] has: a longer run is drawn in several.
const QUADS_A_DRAW: u32 = 1 << 16;

/// The index buffer every world of cubes is drawn with: four vertices a
/// quad, its corners (`cubeCorner` in `shaders/block_quads.slang`), taken
/// 0, 1, 2 and 0, 2, 3 as its two triangles, so each corner is worked out
/// once and not for both triangles it is in.
fn quad_indices() -> Vec<u32> {
    (0..QUADS_A_DRAW).flat_map(|quad| [0, 1, 2, 0, 2, 3].map(|corner| quad * 4 + corner)).collect()
}

/// The axis `normal` lies most along: 0 for x, 1 for y, 2 for z.
fn axis_of(normal: Vec3) -> usize {
    let n = normal.abs();
    if n.x >= n.y && n.x >= n.z {
        0
    } else if n.y >= n.z {
        1
    } else {
        2
    }
}

/// How far up a smooth surface's point of a mesh for rays faces at least to
/// take its top's material, or down to take its bottom's: halfway through
/// the turn over which `smoothLayer` in `shaders/blocks.slang` blends them.
const SMOOTH_TOP: f32 = 0.775;

/// A chunk's mesh for rays, in metres from its corner, its pictures
/// repeated as the world's are from `from`, where the chunk is from the
/// world's corner: a chunk of cubes' faces merged by surface, a smooth
/// chunk's surface at its [`RAYS_LEVEL`]. Its triangles share their
/// corners, each facing the way the faces round it face on average: a
/// block's face has its corners to itself, so faces its own way, and a
/// smooth surface's run on smoothly. Each corner carries the material of
/// the surface of `looks` it is drawn with, and takes that surface's
/// picture size, which the GI's rays take in place of the instance's
/// (`PROXY_MATERIAL`): a smooth surface's point its top's where it faces
/// up, its bottom's where it faces down and its sides' between. With no
/// looks, every corner takes the instance's material and `uv_scale`.
fn proxy(
    mesh: &ChunkMesh,
    smooth: bool,
    block_size: f32,
    from: Vec3,
    (looks, uv_scale): (&[SurfaceLook], f32),
) -> MeshData {
    let (positions, kinds, indices) = match smooth {
        true => voxear_terrain::blocks::smooth::level_triangles(mesh, RAYS_LEVEL),
        false => (mesh.positions.clone(), Vec::new(), mesh.indices.clone()),
    };
    let points: Vec<Vec3> = positions.iter().map(|&p| Vec3::from(p) * block_size).collect();
    let mut ways = vec![Vec3::ZERO; points.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [0, 1, 2].map(|i| points[triangle[i] as usize]);
        // As large as the triangle, so larger faces count for more.
        let way = (b - a).cross(c - a);
        for &i in triangle {
            ways[i as usize] += way;
        }
    }
    let vertices = points
        .iter()
        .zip(ways)
        .enumerate()
        .map(|(at, (&point, way))| {
            let normal = way.normalize_or(Vec3::Y);
            let tangent = Vec3::new(normal.y, -normal.x, 0.0).normalize_or(Vec3::X);
            let world = point + from;
            // Each face's pictures from the side it faces, as the shaders draw them.
            let uv = match axis_of(normal) {
                0 => [world.z, -world.y],
                1 => [world.x, -world.z],
                _ => [world.x, -world.y],
            };
            let surface = match smooth {
                true => kinds.get(at).map(|&[top, sides, bottom]| match normal.y {
                    up if up >= SMOOTH_TOP => top,
                    up if up <= -SMOOTH_TOP => bottom,
                    _ => sides,
                }),
                false => mesh.surfaces.get(at).copied(),
            };
            // As the shaders read a surface: the last where it has more than the world.
            let look = surface.filter(|_| !looks.is_empty()).map(|s| &looks[usize::from(s).min(looks.len() - 1)]);
            let (wind, uv_scale) = match look {
                Some(look) => ((PROXY_MATERIAL | look.material.0).to_le_bytes(), 1.0 / look.size.max(0.01)),
                None => (NO_WIND_MASKS, uv_scale),
            };
            Vertex {
                position: point.to_array(),
                normal: normal.to_array(),
                uv: uv.map(|v| v * uv_scale),
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind,
            }
        })
        .collect();
    MeshData { vertices, indices, ..MeshData::default() }
}

/// The chunks whose meshes for rays give up their room in a full mesh pool,
/// the farthest from the camera first, as much as each of `wanting` needs
/// (its vertices and indices, and how far off it is). They are of those in
/// `order` (the nearest first, each with how far off it is, in chunks) that
/// have meshes for rays, with the room `made` says they take, each farther
/// off than the one it makes room for by more than [`PROXY_SWAP_MARGIN`].
fn giving_up(
    order: &[([i32; 3], f32)],
    made: impl Fn(&[i32; 3]) -> Option<[usize; 2]>,
    wanting: impl IntoIterator<Item = ([usize; 2], f32)>,
) -> Vec<[i32; 3]> {
    let (mut given, mut needed, mut freed) = (Vec::new(), [0; 2], [0; 2]);
    // Those from here on have given theirs up, or have none.
    let mut farthest = order.len();
    for (wants, away) in wanting {
        needed = [needed[0] + wants[0], needed[1] + wants[1]];
        while freed[0] < needed[0] || freed[1] < needed[1] {
            let found = order[..farthest]
                .iter()
                .enumerate()
                .rev()
                .find_map(|(at, (key, far))| made(key).map(|room| (at, *key, *far, room)));
            let Some((at, key, _, room)) = found.filter(|(_, _, far, _)| *far > away + PROXY_SWAP_MARGIN) else {
                return given;
            };
            farthest = at;
            given.push(key);
            freed = [freed[0] + room[0], freed[1] + room[1]];
        }
    }
    given
}

impl BlockWorlds {
    /// Brings every world of blocks in `world` up to date on the GPU,
    /// staging the chunks that changed, and lists what to draw. Chunks'
    /// meshes for rays are made the nearest `camera` first.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        assets: &mut Assets,
        frames: &mut Frames,
        world: &World,
        (frame_index, camera): (u64, Option<Vec3>),
    ) -> Result<()> {
        voxear_core::profile_scope!("Block Uploads");
        self.drawn.clear();
        self.copies.clear();
        // Staged and never recorded: every chunk staged again.
        if let Some(staging) = self.staging.take() {
            frames.retire(Retired::Buffer(staging));
            for (_, pool) in self.resident.drain().chain(self.leaving.drain()) {
                retire(pool, assets, frames, frame_index);
            }
        }
        // Meshes for rays no frame's commands made are made now, before anything draws them.
        let left_over = voxear_core::profile_marker!("Ray Meshes Left Over").scope();
        for kept in self.rays.drain(..) {
            gpu.do_kept(kept)?;
        }
        drop(left_over);
        let features = world.resource::<crate::RenderFeatures>().copied().unwrap_or_default();
        // The frame's chunks to bring up, between every world.
        let mut left = voxear_terrain::WorldBudget::of(world).upload_bytes();
        let any_hidden = world.query_ref::<&voxear_scene::Hidden>().into_iter().next().is_some();
        // Each world, its corner, and for blocks come loose, how they are turned and the world they came from.
        let found: Vec<(Entity, &BlockWorld, Vec3, Option<(Quat, Option<Entity>)>)> = if features.terrain {
            let mut found: Vec<_> = world
                .query_ref::<(Entity, &BlockWorld, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
                .without::<voxear_scene::Destroying>()
                .into_iter()
                .map(|(entity, blocks, local, placed)| {
                    (entity, blocks, placed.map_or(local.position, |placed| placed.0.w_axis.truncate()), None)
                })
                .collect();
            // Blocks come loose, turned about their Obj's place, their corner `offset` from it.
            let loose = world
                .query_ref::<(Entity, &LooseBlocks, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
                .without::<voxear_scene::Destroying>();
            for (entity, loose, local, placed) in loose {
                let (turn, at) = match placed {
                    Some(placed) => {
                        let (_, turn, at) = placed.0.to_scale_rotation_translation();
                        (turn, at)
                    }
                    None => (local.rotation, local.position),
                };
                found.push((entity, &loose.world, at + turn * Vec3::from(loose.offset), Some((turn, loose.from))));
            }
            found
        } else {
            Vec::new()
        };
        let indices = match &self.indices {
            Some(indices) => indices.handle,
            None => {
                let indices = quad_indices();
                let size = (indices.len() * size_of::<u32>()) as u64;
                let usage = vk::BufferUsageFlags::INDEX_BUFFER;
                let buffer = gpu.create_buffer("block quad indices", size, usage, MemoryLocation::CpuToGpu)?;
                buffer.write(0, &indices);
                self.indices.insert(buffer).handle
            }
        };
        let mut staged: Vec<u8> = Vec::new();
        let mut proxies_left = PROXIES_A_FRAME;
        for (entity, blocks, corner, loose) in found {
            let (turn, lit_by) = (loose.map(|(turn, _)| turn), loose.and_then(|(_, from)| from));
            if any_hidden && crate::scene::hidden(world, entity) {
                continue;
            }
            let look = world.get::<BlockLook>(entity).cloned().unwrap_or_else(|| BlockLook {
                material: assets.default_material,
                surfaces: Arc::from([]),
                voxels: BlockVoxels::default(),
                motions: Arc::from([]),
                light: BlockLight::default(),
            });
            let changed = |pool: &Pool| pool.block_size != blocks.block_size || pool.smooth != blocks.smooth;
            if self.resident.get(&entity).is_some_and(changed) {
                let pool = self.resident.remove(&entity).expect("held");
                // Drawn as it was until the new pool has every chunk, unless an older one already is.
                match pool.shown.is_some() && !self.leaving.contains_key(&entity) {
                    true => drop(self.leaving.insert(entity, pool)),
                    false => retire(pool, assets, frames, frame_index),
                }
            }
            if !self.resident.contains_key(&entity) {
                self.resident.insert(entity, Pool::new(gpu, blocks.block_size, blocks.smooth)?);
            }
            let pool = self.resident.get_mut(&entity).expect("made above");
            pool.seen = frame_index;
            let copies = &mut self.copies;
            let mut stage = |bytes: &[u8], to: u64| {
                let from = staged.len() as u64;
                staged.extend_from_slice(bytes);
                copies.push(Copy { from, key: entity, to, size: bytes.len() as u64 });
            };
            // Its chunks' quads in order by which of its surfaces are cut out from their pictures (see `arranged`).
            let cut: Arc<[bool]> = match blocks.smooth {
                true => Arc::from([]),
                false => look.surfaces.iter().map(|surface| assets.is_cut_out(surface.material)).collect(),
            };
            pool.arrange(cut, &mut stage);
            let fading = voxear_core::profile_marker!("Fades").scope();
            pool.fade(&blocks.fades, &mut stage);
            drop(fading);
            let chunks = voxear_core::profile_marker!("Chunks").scope();
            let whole = pool.update(gpu, frames, blocks, (frame_index, &mut left), &mut stage)?;
            drop(chunks);
            // Its tints, brought up where they changed; one larger than its room is left untinted.
            match world.get::<BlockTints>(entity) {
                Some(tints)
                    if tints.size as usize <= MOST_TINT_POINTS_SIDE
                        && tints.points.len() == (tints.size * tints.size * 2) as usize =>
                {
                    if !pool.tinted.as_ref().is_some_and(|(points, ..)| Arc::ptr_eq(points, &tints.points)) {
                        stage(bytemuck::cast_slice(&tints.points), TINTS_AT);
                        if tints.far_points.len() == tints.points.len() {
                            stage(bytemuck::cast_slice(&tints.far_points), TINTS_AT + TINT_BYTES / 2);
                        }
                    }
                    let far = (tints.far_points.len() == tints.points.len()).then_some(tints.far_corner);
                    pool.tinted = Some((tints.points.clone(), tints.corner, tints.size, far));
                }
                _ => pool.tinted = None,
            }
            // Each chunk's mesh for rays, a few a frame, the nearest the camera first, the old one shown until the
            // new one is made; none for blocks that came loose, a moment from going.
            let side = CHUNK as f32 * blocks.block_size;
            // Where the camera is from the world's corner, in chunks.
            let near = camera.map(|camera| (camera - corner) / side);
            let camera_chunk = near.map(|near| near.floor().as_ivec3().to_array());
            let same = |(meshes, places): &(Arc<ChunkMeshes>, Arc<Vec<voxear_terrain::blocks::BlockPlace>>)| {
                Arc::ptr_eq(meshes, &blocks.meshes) && Arc::ptr_eq(places, &blocks.places)
            };
            // Painted with its surfaces' materials: when those change, every chunk's is made again.
            if !Arc::ptr_eq(&pool.painted.0, &look.surfaces) {
                if pool.painted.0 != look.surfaces {
                    pool.painted.1 += 1;
                    (pool.proxied, pool.made_to) = (None, 0);
                }
                pool.painted.0 = look.surfaces.clone();
            }
            // With the mesh pool full, a camera gone into another chunk may have a nearer chunk take a farther's room.
            let moved = pool.full && !pool.ordered.as_ref().is_some_and(|(.., at)| *at == camera_chunk);
            let settled = pool.proxied.as_ref().is_some_and(same) && !moved;
            if turn.is_none() && !settled && frame_index >= pool.room_at {
                // Seen from more places than one, as through Portals, only what is round the first is traced:
                // the GI's rays reach no further than round the camera.
                let own = blocks.places.first().map(|place| &place.columns);
                let traced = |key: &[i32; 3]| own.is_none_or(|own| own.contains(&[0, key[0], key[2]]));
                let before = pool.proxies.len();
                pool.proxies.retain(|key, (_, mesh, ..)| {
                    let kept = blocks.meshes.contains_key(key) && traced(key);
                    if !kept {
                        assets.free_mesh(*mesh, frame_index);
                    }
                    kept
                });
                let sorted = pool.ordered.as_ref().is_some_and(|(meshes, places, at)| {
                    Arc::ptr_eq(meshes, &blocks.meshes) && Arc::ptr_eq(places, &blocks.places) && *at == camera_chunk
                });
                if !sorted {
                    let _ordering = voxear_core::profile_marker!("Ray Order").scope();
                    let middle = |key: &[i32; 3]| Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) + 0.5;
                    let away = |key: &[i32; 3]| near.map_or(0.0, |near| middle(key).distance(near));
                    pool.order.clear();
                    pool.order.extend(blocks.meshes.keys().filter(|key| traced(key)).map(|key| (*key, away(key))));
                    pool.order.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
                    pool.ordered = Some((blocks.meshes.clone(), blocks.places.clone(), camera_chunk));
                    pool.made_to = 0;
                }
                let mut changed = pool.proxies.len() != before;
                let mut all = true;
                pool.full = false;
                let uv_scale = 1.0 / blocks.block_size.max(0.01);
                let order = std::mem::take(&mut pool.order);
                let paint = pool.painted.1;
                let unmade = |proxies: &HashMap<[i32; 3], Proxy>, key: &[i32; 3], mesh: &Arc<ChunkMesh>| {
                    !proxies
                        .get(key)
                        .is_some_and(|(made, _, _, painted, _)| Arc::ptr_eq(made, mesh) && *painted == paint)
                };
                // Uploaded and built together, waited for once.
                let batch = gpu.batch();
                let mut made_to = order.len();
                for (index, &(key, away)) in order.iter().enumerate().skip(pool.made_to) {
                    let Some(mesh) = blocks.meshes.get(&key) else { continue };
                    if !unmade(&pool.proxies, &key, mesh) {
                        continue;
                    }
                    made_to = index;
                    if proxies_left == 0 {
                        all = false;
                        break;
                    }
                    proxies_left -= 1;
                    let from = Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                    let making = voxear_core::profile_marker!("Ray Mesh Make").scope();
                    let data = proxy(mesh, blocks.smooth, blocks.block_size, from, (&look.surfaces, uv_scale));
                    let lods = crate::lod::rays_only(&data);
                    drop(making);
                    let _uploading = voxear_core::profile_marker!("Ray Mesh Upload").scope();
                    let room = [data.vertices.len(), data.indices.len()];
                    // Out of room, the rest wait: the GI's rays miss them, but the frame goes on.
                    let handle = match assets.add_mesh_with_lods(gpu, &data, &lods) {
                        Ok(handle) => handle,
                        Err(error) => {
                            if !std::mem::replace(&mut self.warned_full, true) {
                                tracing::warn!(
                                    "a world of blocks' chunks are left out of the GI's rays, the farthest first: {error:#}"
                                );
                            }
                            // Room for this one and the next few, from the farthest with meshes for rays; a chunk
                            // of cubes needs as many as its own faces have, a smooth one less.
                            let next = order[index + 1..]
                                .iter()
                                .filter_map(|(key, away)| blocks.meshes.get(key).map(|mesh| (key, mesh, *away)))
                                .filter(|(key, mesh, _)| unmade(&pool.proxies, key, mesh))
                                .take(PROXIES_A_FRAME - 1)
                                .map(|(_, mesh, away)| ([mesh.positions.len(), mesh.indices.len()], away));
                            let wanting = std::iter::once((room, away)).chain(next);
                            let given =
                                giving_up(&order, |key| pool.proxies.get(key).map(|(_, _, room, ..)| *room), wanting);
                            for key in &given {
                                if let Some((_, mesh, ..)) = pool.proxies.remove(key) {
                                    assets.free_mesh(mesh, frame_index);
                                }
                            }
                            match given.is_empty() {
                                // Tried again once its faces change or the camera moves on, as room may be made by then.
                                true => pool.full = true,
                                // Tried again once the room given up is free.
                                false => {
                                    (changed, all) = (true, false);
                                    pool.room_at = frame_index + FRAMES_IN_FLIGHT as u64;
                                }
                            }
                            proxies_left = 0;
                            break;
                        }
                    };
                    // Its most common surface worked out once, for how it sounds (see `ACOUSTIC_ROCK`).
                    let surface = most_common_surface(mesh);
                    if let Some((_, old, ..)) = pool.proxies.insert(key, (mesh.clone(), handle, room, paint, surface)) {
                        assets.free_mesh(old, frame_index);
                    }
                    changed = true;
                    made_to = order.len();
                }
                (pool.order, pool.made_to) = (order, made_to);
                // Done by the frame's own commands: submitted alone and waited for, it would wait for every frame
                // the GPU is still drawing too.
                if let Some(kept) = batch.keep()? {
                    self.rays.push(kept);
                }
                pool.proxied = all.then(|| (blocks.meshes.clone(), blocks.places.clone()));
                if changed {
                    pool.placed = None;
                }
            }
            // Lit by its blocks' light, that light round the camera, taken again as the camera goes a few blocks on
            // or the blocks change.
            let lit = look.light.kept && !blocks.smooth && turn.is_none();
            let volume = camera.filter(|_| lit).map(|camera| {
                let at = ((camera - corner) / blocks.block_size).floor().as_ivec3().to_array();
                let origin =
                    [0, 1, 2].map(|i| (at[i] - LIGHT_VOLUME[i] / 2).div_euclid(LIGHT_VOLUME_STEP) * LIGHT_VOLUME_STEP);
                let kept = pool.volume.as_ref().filter(|(map, light, from, _)| {
                    Arc::ptr_eq(map, &blocks.blocks) && Arc::ptr_eq(light, &blocks.light) && *from == origin
                });
                match kept {
                    Some((.., bytes)) => (origin, bytes.clone()),
                    None => {
                        let bytes: Arc<[u8]> =
                            light::volume(&blocks.blocks, &blocks.faces, &blocks.light, origin, LIGHT_VOLUME).into();
                        pool.volume = Some((blocks.blocks.clone(), blocks.light.clone(), origin, bytes.clone()));
                        (origin, bytes)
                    }
                }
            });
            let placing = voxear_core::profile_marker!("Ray Places").scope();
            let placed = pool
                .placed
                .get_or_insert_with(|| {
                    let at = |key: &[i32; 3]| Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                    let acoustic = |surface: Option<usize>| {
                        let material = surface.and_then(|surface| look.surfaces.get(surface));
                        match material.map_or(0, |look| assets.acoustic(look.material)) {
                            0 => ACOUSTIC_ROCK + 1,
                            given => given,
                        }
                    };
                    let proxies = pool.proxies.iter();
                    proxies.map(|(key, (_, mesh, _, _, surface))| (*mesh, at(key), acoustic(*surface))).collect()
                })
                .clone();
            drop(placing);
            let (chunks, groups) = pool.draws();
            let drawn = DrawnBlocks {
                entity,
                corner,
                turn,
                lit_by,
                block_size: blocks.block_size,
                smooth: blocks.smooth,
                address: pool.buffer.address,
                light: pool.buffer.address + pool.light_at(),
                chunks,
                groups,
                sights: (!blocks.smooth).then(|| (blocks.blocks.clone(), pool.sights())),
                places: blocks.places.clone(),
                fades: blocks.fades.clone(),
                look,
                moving: blocks
                    .moving
                    .iter()
                    .take(MAX_MOVING)
                    .map(|moving| GpuMovingBlock {
                        at: [moving.at[0], moving.at[1], moving.at[2], i32::from(moving.opening)],
                        timing: [(blocks.clock - moving.started).max(0.0) as f32, moving.length, 0.0, 0.0],
                    })
                    .collect(),
                sea_top: world.get::<crate::sea::SeaLevel>(entity).map(|level| level.0),
                cast_shadows: blocks.cast_shadows,
                proxies: placed,
                tints: pool.tinted.as_ref().map(|&(_, corner, size, far)| (corner, size, far)),
                cover: world.get::<BlockTints>(entity).map_or([0.0; 2], |tints| tints.cover),
                covered: world.get::<BlockTints>(entity).map_or([0; 8], |tints| tints.covered),
                id: None,
                volume,
                indices,
            };
            if whole && let Some(old) = self.leaving.remove(&entity) {
                retire(old, assets, frames, frame_index);
            }
            // Until then, the pool of blocks of the old size is drawn where the world is now, as it last drew.
            match self.leaving.get(&entity).and_then(|old| old.shown.clone()) {
                Some(old) => {
                    self.drawn.push(DrawnBlocks {
                        corner,
                        turn,
                        lit_by,
                        look: drawn.look,
                        moving: drawn.moving,
                        sea_top: drawn.sea_top,
                        cast_shadows: drawn.cast_shadows,
                        // Its tints are in the new pool alone.
                        tints: None,
                        ..old
                    })
                }
                None => {
                    pool.shown = Some(drawn.clone());
                    self.drawn.push(drawn);
                }
            }
        }
        let gone: Vec<Entity> =
            self.resident.iter().filter(|(_, pool)| pool.seen != frame_index).map(|(key, _)| *key).collect();
        for key in gone {
            let pool = self.resident.remove(&key).expect("held");
            retire(pool, assets, frames, frame_index);
            if let Some(old) = self.leaving.remove(&key) {
                retire(old, assets, frames, frame_index);
            }
        }
        let _staging = voxear_core::profile_marker!("Staging").scope();
        if !staged.is_empty() {
            let usage = vk::BufferUsageFlags::TRANSFER_SRC;
            let staging = gpu.create_buffer("block uploads", staged.len() as u64, usage, MemoryLocation::CpuToGpu)?;
            staging.write(0, &staged);
            self.staging = Some(staging);
        }
        Ok(())
    }

    /// Copies this frame's staged chunks in, and gives what every view
    /// draws, each world's pool as the graph knows it.
    pub fn record(&mut self, fg: &mut FrameGraph<'_>, frames: &mut Frames) -> Arc<[DrawnBlocks]> {
        // Each pool once, by where its buffer is: a world drawn from the pool it is leaving is copied into the next.
        let mut ids = HashMap::new();
        let pools: Vec<&Pool> = self.resident.values().chain(self.leaving.values()).collect();
        for drawn in &mut self.drawn {
            let Some(pool) = pools.iter().find(|pool| pool.buffer.address == drawn.address) else { continue };
            drawn.id = Some(*ids.entry(drawn.address).or_insert_with(|| fg.import_buffer(&pool.buffer)));
        }
        // The meshes for rays made this frame, before the top level of what rays meet is built with them.
        if !self.rays.is_empty() {
            let mut rays = std::mem::take(&mut self.rays);
            for kept in &mut rays {
                for buffer in kept.take_buffers() {
                    frames.retire(Retired::Buffer(buffer));
                }
            }
            fg.add_pass("Ray mesh uploads (blocks)", &[], &[], move |ctx| {
                for kept in &rays {
                    ctx.gpu.record_kept(ctx.cmd, kept);
                }
            });
        }
        if let Some(staging) = self.staging.take() {
            let mut copies: Vec<(vk::Buffer, vk::BufferCopy)> = Vec::with_capacity(self.copies.len());
            for copy in &self.copies {
                let Some(pool) = self.resident.get(&copy.key) else { continue };
                ids.entry(pool.buffer.address).or_insert_with(|| fg.import_buffer(&pool.buffer));
                let region = vk::BufferCopy { src_offset: copy.from, dst_offset: copy.to, size: copy.size };
                copies.push((pool.buffer.handle, region));
            }
            let written: Vec<(BufferId, BufferAccess)> =
                ids.values().map(|&id| (id, BufferAccess::TransferWrite)).collect();
            let source = staging.handle;
            fg.add_pass("Block uploads", &[], &written, move |ctx| {
                for (target, region) in &copies {
                    unsafe { ctx.gpu.device.cmd_copy_buffer(ctx.cmd, source, *target, std::slice::from_ref(region)) };
                }
            });
            frames.retire(Retired::Buffer(staging));
        }
        self.drawn.clone().into()
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        if let Some(staging) = self.staging.take() {
            gpu.destroy_buffer(staging);
        }
        if let Some(indices) = self.indices.take() {
            gpu.destroy_buffer(indices);
        }
        for mut kept in self.rays.drain(..) {
            for buffer in kept.take_buffers() {
                gpu.destroy_buffer(buffer);
            }
        }
        for (_, pool) in self.resident.drain().chain(self.leaving.drain()) {
            gpu.destroy_buffer(pool.buffer);
        }
    }
}

fn retire(pool: Pool, assets: &mut Assets, frames: &mut Frames, frame_index: u64) {
    frames.retire(Retired::Buffer(pool.buffer));
    for (_, (_, mesh, ..)) in pool.proxies {
        assets.free_mesh(mesh, frame_index);
    }
}

/// Rock's place among `voxear_physics::scene::Acoustic`'s presets.
const ACOUSTIC_ROCK: u32 = 10;

/// The surface most of a chunk's faces are drawn with, its water's aside.
fn most_common_surface(chunk: &ChunkMesh) -> Option<usize> {
    let mut counts = [0u32; 256];
    for quad in &chunk.quads[..chunk.water().0 as usize] {
        counts[(quad[1] & 0xff) as usize] += 1;
    }
    let (surface, &count) = counts.iter().enumerate().max_by_key(|(_, count)| **count)?;
    (count > 0).then_some(surface)
}

/// Adds each world's chunks' meshes for rays to the frame's instances, so
/// the GI's rays see its blocks.
pub(crate) fn add_proxies(drawn: &[DrawnBlocks], extracted: &mut ExtractedFrame) {
    for blocks in drawn {
        for (part, &(mesh, at, acoustic)) in blocks.proxies.iter().enumerate() {
            let model = Mat4::from_translation(blocks.corner + at);
            let normal = Mat3::from_mat4(model).inverse().transpose();
            extracted.instances.push(GpuInstance {
                model: crate::gpu::cols(model),
                prev_model: crate::gpu::cols(model),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                mesh: mesh.0,
                material: blocks.look.material.0,
                flags: INSTANCE_RAYS_ONLY,
                acoustic,
                ..GpuInstance::default()
            });
            extracted.entities.push(Owner { entity: blocks.entity, plant: None, part: Some(part as u32), far_side: false });
        }
    }
}

/// What a view draws of its worlds of blocks: for each world, its number in
/// the view, whether it is drawn smooth, and the quads (or corners' words)
/// each list draws; for each world with water in view, its number, whether
/// it is drawn smooth and its water's quads (or corners' words); their pools
/// to wait on; and whether any casts the sun's shadows. Every pass that
/// draws them holds a copy, its lists shared.
#[derive(Clone)]
pub(crate) struct ViewBlocks {
    pub runs: Arc<[(u32, bool, Vec<Vec<[u32; 2]>>)]>,
    pub water: Arc<[(u32, bool, Vec<[u32; 2]>)]>,
    pub pools: Vec<BufferId>,
    pub shadows: bool,
    /// The index buffer worlds of cubes are drawn with (see [`quad_indices`]).
    indices: vk::Buffer,
    /// The lists' runs as indirect draws, once written (see
    /// [`write_draws`](Self::write_draws)): the buffer, and for each world's
    /// lists, a world's [`LISTS`] after another's, its first draw in it and
    /// how many.
    draws: Option<(vk::Buffer, Arc<[[u32; 2]]>)>,
}

/// An indirect draw's words: as an indexed draw of a world of cubes, its
/// indices, one instance, from the first index and vertex; drawn smooth,
/// its vertices, one instance and from the first vertex; and the run's
/// first quad as its first instance, which the shaders read as where the
/// run starts (`blockRunStart`).
pub(crate) type BlockDraw = [u32; 5];

/// The most indirect draws one call draws: as many as every GPU draws at
/// least.
const DRAWS_A_CALL: u32 = 65_535;

/// Run `[first, count]` of a world as indirect draws: one drawn smooth, as
/// many as its quads take of [`QUADS_A_DRAW`] for a world of cubes.
fn run_draws(smooth: bool, [first, count]: [u32; 2], draws: &mut Vec<BlockDraw>) {
    if smooth {
        draws.push([count / 2, 1, 0, first, 0]);
        return;
    }
    for from in (0..count).step_by(QUADS_A_DRAW as usize) {
        draws.push([(count - from).min(QUADS_A_DRAW) * 6, 1, 0, 0, first + from]);
    }
}

impl ViewBlocks {
    /// Writes every list's runs as indirect draws into the buffer `fit`
    /// gives with room for as many as they take, so a pass draws a world's
    /// list with one call however many runs it has.
    pub fn write_draws<'a>(&mut self, fit: impl FnOnce(usize) -> Result<&'a Buffer>) -> Result<()> {
        let mut draws: Vec<BlockDraw> = Vec::new();
        let mut spans = Vec::with_capacity(self.runs.len() * LISTS);
        for (_, smooth, lists) in self.runs.iter() {
            for list in lists {
                let first = draws.len() as u32;
                for &run in list {
                    run_draws(*smooth, run, &mut draws);
                }
                spans.push([first, draws.len() as u32 - first]);
            }
        }
        let buffer = fit(draws.len())?;
        if !draws.is_empty() {
            buffer.write(0, &draws);
        }
        self.draws = Some((buffer.handle, spans.into()));
        Ok(())
    }

    /// Draws list `stored` of the view's world `world` (its place in
    /// [`runs`](Self::runs)), with the push constants already given, as
    /// indirect draws, once written.
    fn draw_list(&self, ctx: &PassContext, push: &Push, world: usize, stored: u32) {
        let Some((buffer, spans)) = &self.draws else { return };
        let [first, count] = spans[world * LISTS + stored as usize];
        if count == 0 {
            return;
        }
        let smooth = self.runs[world].1;
        ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { output: 0, ..*push });
        let stride = size_of::<BlockDraw>() as u32;
        if !smooth {
            unsafe { ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, self.indices, 0, vk::IndexType::UINT32) };
        }
        for from in (0..count).step_by(DRAWS_A_CALL as usize) {
            let (offset, count) = (u64::from(first + from) * u64::from(stride), (count - from).min(DRAWS_A_CALL));
            match smooth {
                true => unsafe { ctx.gpu.device.cmd_draw_indirect(ctx.cmd, *buffer, offset, count, stride) },
                false => unsafe { ctx.gpu.device.cmd_draw_indexed_indirect(ctx.cmd, *buffer, offset, count, stride) },
            }
        }
    }

    /// Whether any world's water is in view.
    pub fn has_water(&self) -> bool {
        !self.water.is_empty()
    }

    /// How many draws and triangles its worlds take, in view and in the
    /// sun's cascades.
    pub fn counts(&self) -> BlockCounts {
        let mut counts = BlockCounts::default();
        for (_, smooth, lists) in self.runs.iter() {
            // A quad is two triangles; drawn smooth, a triangle is three corners of two pairs of words.
            let triangles = |runs: &[[u32; 2]]| -> u64 {
                runs.iter().map(|&[_, count]| if *smooth { u64::from(count) / 6 } else { u64::from(count) * 2 }).sum()
            };
            counts.draws += lists[LIST_EARLY as usize].len() as u32;
            counts.triangles += triangles(&lists[LIST_EARLY as usize]);
            for list in (0..CASCADES).flat_map(|c| [LIST_SHADOW as usize + c, SHADOW_CUT as usize + c]) {
                counts.shadow_draws += lists[list].len() as u32;
                counts.shadow_triangles += triangles(&lists[list]);
            }
        }
        for (_, smooth, runs) in self.water.iter() {
            counts.draws += runs.len() as u32;
            let triangles = |count: u32| if *smooth { u64::from(count) / 6 } else { u64::from(count) * 2 };
            counts.water_triangles += runs.iter().map(|&[_, count]| triangles(count)).sum::<u64>();
        }
        counts
    }
}

/// How many draws and triangles a view's worlds of blocks take: their
/// ground in view, their water, and their ground in the sun's cascades.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockCounts {
    pub draws: u32,
    pub triangles: u64,
    pub water_triangles: u64,
    pub shadow_draws: u32,
    pub shadow_triangles: u64,
}

/// Picks each world's chunks and faces for this view and writes the
/// worlds' records into `buffer`; `None` when there is nothing to draw.
/// `entities` gives each instance's owner, from which each world finds its
/// anchor among its meshes for rays. A view through a Portal has a space of
/// its own, its middle `offset` from the world's (see `portals::Through`).
/// The sun's cascades take their chunks only where `shadows` says: not
/// with its virtual shadow maps, whose levels drawn this frame take theirs
/// from `sun_pages`, nor through a Portal without the sun's shadows. Only
/// the chunks seen into through their sides from `sights` are drawn: the
/// camera, or for a view through a Portal, where the Portal it looks out of
/// stands, as all it sees, it sees through there, and its camera may stand
/// inside the ground behind it.
///
/// Each lamp casting a shadow takes the chunks in its reach, their faces
/// turned towards it: for one that keeps what stands still (see
/// `shadow_cache`), all but the blocks of parts, which may move, only when
/// it draws its kept views, and those each frame; for any other, all of
/// them each frame.
///
/// Where `rings`, as in a view not through a Portal, a world without end's
/// rings fade into each other (see `voxear_terrain::blocks::RingFades`).
pub(crate) fn gather(
    drawn: &[DrawnBlocks],
    extracted: &crate::scene::ExtractedFrame,
    buffer: &Buffer,
    (offset, sights, shadows, rings): (Vec3, Option<Vec3>, bool, bool),
    lamps: &crate::shadow_cache::Lamps,
    sun_pages: &[(usize, [[f32; 4]; 6])],
) -> Option<ViewBlocks> {
    // The light round the camera of the world lit by its blocks' light it is in, or the first: or none.
    let camera_at = Vec3::from(extracted.frame.camera_position) + offset;
    let inside = |blocks: &&DrawnBlocks| {
        blocks.volume.as_ref().is_some_and(|(origin, _)| {
            let at = ((camera_at - blocks.corner) / blocks.block_size).floor().as_ivec3().to_array();
            (0..3).all(|i| (origin[i]..origin[i] + LIGHT_VOLUME[i]).contains(&at[i]))
        })
    };
    let around = drawn.iter().take(MAX_BLOCK_WORLDS).find(inside);
    let volume = around.and_then(|blocks| Some((blocks, blocks.volume.as_ref()?)));
    let header = volume.map_or(GpuLightVolume::default(), |(blocks, (origin, _))| {
        let glow = blocks.look.light.glow;
        let origin_at = blocks.corner - offset
            + Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32) * blocks.block_size;
        GpuLightVolume {
            origin: *origin,
            found: 1,
            corner: origin_at.into(),
            block_size: blocks.block_size,
            glow: [glow.x, glow.y, glow.z, DARKEST_SKY],
        }
    });
    buffer.write(LIGHT_VOLUME_AT as u64, std::slice::from_ref(&header));
    if let Some((_, (_, bytes))) = volume {
        buffer.write(LIGHT_BYTES_AT as u64, &bytes[..]);
    }
    if drawn.is_empty() {
        return None;
    }
    let (frame, entities, has_sun) = (&extracted.frame, extracted.entities.as_slice(), extracted.has_sun && shadows);
    voxear_core::profile_scope!("Block Picking");
    let mut anchors: HashMap<Entity, u32> = HashMap::new();
    for (index, owner) in entities.iter().enumerate().take(crate::MAX_INSTANCES as usize) {
        if owner.plant.is_none() && owner.part.is_some() {
            anchors.entry(owner.entity).or_insert(index as u32);
        }
    }
    let camera = Vec3::from(frame.camera_position);
    let mut records = Vec::with_capacity(drawn.len().min(MAX_BLOCK_WORLDS));
    let mut surfaces = vec![GpuTerrainSurface::default(); drawn.len().min(MAX_BLOCK_WORLDS) * MAX_BLOCK_SURFACES];
    let mut runs = Vec::new();
    let mut water = Vec::new();
    for (number, blocks) in drawn.iter().take(MAX_BLOCK_WORLDS).enumerate() {
        let looks = &blocks.look.surfaces[..blocks.look.surfaces.len().min(MAX_BLOCK_SURFACES)];
        for (at, (place, look)) in surfaces[number * MAX_BLOCK_SURFACES..].iter_mut().zip(looks).enumerate() {
            *place = GpuTerrainSurface {
                material: look.material.0,
                uv_scale: 1.0 / look.size.max(0.01),
                sharpness: 0.0,
                flags: surface_flags(look)
                    | if blocks.covered[at >> 5] >> (at & 31) & 1 != 0 { SURFACE_COVERED } else { 0 },
                tracks: 0.0,
                fills_in: 0.0,
            };
        }
        let surfaces_at = (SURFACES_AT + number * MAX_BLOCK_SURFACES * size_of::<GpuTerrainSurface>()) as u64;
        // Its motions, then its blocks moving now, each as many as it has.
        let parts_at = (PARTS_AT + number * PARTS_EACH) as u64;
        let moving_at = parts_at + (MAX_MOTIONS * size_of::<GpuBlockMotion>()) as u64;
        let motions = &blocks.look.motions[..blocks.look.motions.len().min(MAX_MOTIONS)];
        if !motions.is_empty() {
            buffer.write(parts_at, motions);
        }
        let moving = &blocks.moving[..blocks.moving.len().min(MAX_MOVING)];
        if !moving.is_empty() {
            buffer.write(moving_at, moving);
        }
        // A look pack the player wears says how every world's faces are cut into small cubes.
        let voxels = extracted.block_voxels.unwrap_or(blocks.look.voxels);
        // Lit by the light kept for its blocks, a world of cubes that says so.
        let lit = blocks.look.light.kept && !blocks.smooth;
        let glow = blocks.look.light.glow;
        // Its rings fading into each other, a world of cubes without end seen from where it is looked at from.
        let fades = &blocks.fades;
        let fading = rings && !blocks.smooth && blocks.turn.is_none() && fades.reach > 0.0;
        // Whole metres off the world's own corner, so its far chunks keep their place to the millimetre.
        let corner = blocks.corner - offset;
        records.push(GpuBlockWorld {
            corner: corner.into(),
            block_size: blocks.block_size,
            quads: blocks.address + QUADS_AT,
            origins: blocks.address,
            surfaces: buffer.address + surfaces_at,
            surface_count: looks.len() as u32,
            material: blocks.look.material.0,
            anchor: anchors.get(&blocks.lit_by.unwrap_or(blocks.entity)).copied().unwrap_or(0),
            flags: if blocks.smooth { BLOCKS_SMOOTH } else { 0 }
                | if blocks.turn.is_some() { BLOCKS_TURNED } else { 0 }
                | if lit { BLOCKS_LIT } else { 0 },
            voxels: if blocks.smooth { 0.0 } else { voxels.count.min(BlockVoxels::MOST) as f32 },
            relief: voxels.relief.clamp(0.0, BlockVoxels::DEEPEST),
            sea_top: blocks.sea_top.unwrap_or(crate::sea::ANY_WATER),
            tint_size: blocks.tints.map_or(0, |(_, size, _)| size),
            motions: buffer.address + parts_at,
            moving: buffer.address + moving_at,
            motion_count: motions.len() as u32,
            moving_count: moving.len() as u32,
            tints: blocks.address + TINTS_AT,
            tint_corner: blocks.tints.map_or([0; 2], |(corner, ..)| corner),
            turn: blocks.turn.unwrap_or(Quat::IDENTITY).to_array(),
            // A far corner of i32::MIN for none.
            far_tint_corner: blocks.tints.and_then(|(_, _, far)| far).unwrap_or([i32::MIN; 2]),
            ring_clock: voxear_terrain::blocks::fade_clock(),
            ring_coarsest: fades.coarsest,
            fades: blocks.address + FADES_AT,
            rings: match fading {
                true => [fades.at[0], fades.at[1], fades.reach, fades.band],
                false => [0.0; 4],
            },
            cover: blocks.cover,
            glow: [glow.x, glow.y, glow.z, DARKEST_SKY],
            light: blocks.light,
            light_pad: [0; 2],
        });
        // Its chunks are picked in its own space: turned back where it is turned.
        let back = blocks.turn.unwrap_or(Quat::IDENTITY).inverse();
        let local = back * (camera - corner);
        let mut lists = vec![Vec::new(); LISTS];
        let planes = turned_planes(&frame.planes, corner, back);
        // What the world shows from the place nearest the camera, where it is seen from more than one.
        let nearest = |place: &&voxear_terrain::blocks::BlockPlace| {
            (place.at[0] - local.x).powi(2) + (place.at[1] - local.z).powi(2)
        };
        let shown = blocks.places.iter().min_by(|a, b| nearest(a).total_cmp(&nearest(b))).map(|place| &place.columns);
        // The chunks the camera can see into, walking from its own through their sides that see each other: within
        // the columns shown from its place, where the world is seen from more than one, as the walk would otherwise
        // cross every column of air between the places.
        let seen = blocks.sights.as_ref().zip(sights).and_then(|((map, sights), from)| {
            let side = CHUNK as f32 * blocks.block_size;
            let from = ((back * (from - corner)) / side).floor().as_ivec3().to_array();
            let bounds = |key: [i32; 3]| {
                let low = Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                [low, low + Vec3::splat(side)]
            };
            let ours = |key: [i32; 3]| shown.is_none_or(|shown| shown.contains(&[0, key[0], key[2]]));
            let _walk = voxear_core::profile_marker!("Sight Walk").scope();
            voxear_terrain::blocks::sight::seen(map, sights, from, |key| ours(key) && within(&planes, bounds(key)))
        });
        // A smooth chunk drawn as coarse as its distance allows, as models' levels are picked.
        let allowed = frame.lod_threshold / frame.lod_scale.max(1e-6);
        let ring = fading.then_some((fades.at, fades.reach, fades.coarsest as i32));
        let detail = Some(Detail { camera: local, allowed, block_size: blocks.block_size, rings: ring });
        let grouped = (&blocks.chunks[..], &blocks.groups[..]);
        let camera_chunks = voxear_core::profile_marker!("Camera Chunks").scope();
        lists[LIST_EARLY as usize] = pick_grouped(
            grouped,
            &planes,
            (Facing::Camera(local), detail),
            (seen.as_ref(), shown),
            Parts::With,
            Some(local),
        );
        // A coarse column's water is left out where the finer columns over it are made, theirs drawn whole in its
        // place, as water shows no ring's fade.
        let covered = |chunk: &ChunkDraw| {
            let [level, x, _, z] = chunk.key;
            fading && level > 0 && fades.covered.get(&[level, x, z]).is_some_and(|&since| since != f32::MAX)
        };
        let wet = pick_water_grouped(grouped, &planes, (seen.as_ref(), shown), |chunk| !covered(chunk));
        drop(camera_chunks);
        if !wet.is_empty() {
            water.push((number as u32, blocks.smooth, wet));
        }
        if has_sun && blocks.cast_shadows {
            let sunlit = Facing::Light(back * Vec3::from(frame.sun_direction));
            let cascades: Vec<Vec<[f32; 4]>> =
                frame.cascade_planes.iter().map(|planes| turned_planes(planes, corner, back)).collect();
            // Every cascade's planes face the same ways, along and across the sun: the loosest of each way holds them
            // all. The chunks within those are looked at for each cascade, of the thousands a far view has.
            let around: Vec<[f32; 4]> = (0..cascades[0].len())
                .map(|way| {
                    let loosest = cascades.iter().map(|planes| planes[way][3]).fold(f32::MIN, f32::max);
                    [cascades[0][way][0], cascades[0][way][1], cascades[0][way][2], loosest]
                })
                .collect();
            let ours = |chunk: &&ChunkDraw| {
                shown.is_none_or(|shown| shown.contains(&[chunk.key[0], chunk.key[1], chunk.key[3]]))
            };
            let casting: Vec<ChunkDraw> =
                blocks.chunks.iter().filter(ours).filter(|chunk| within(&around, chunk.bounds)).copied().collect();
            let cut = cut_runs(&casting);
            let casting_group = one_group(&casting);
            for (cascade, planes) in cascades.iter().enumerate() {
                let picked =
                    pick_grouped((&casting, &casting_group), planes, (sunlit, detail), (None, None), Parts::With, None);
                // In the last cascade a picture's pixel is about one of the cascade's: leaves cast as solid there.
                (lists[LIST_SHADOW as usize + cascade], lists[SHADOW_CUT as usize + cascade]) = match cascade {
                    SOLID_CASCADE.. => (picked, Vec::new()),
                    _ => cut_apart(&cut, picked),
                };
            }
        }
        // The faces turned towards the sun within each level of its virtual shadow maps drawn this frame.
        let sun_chunks = voxear_core::profile_marker!("Sun Page Chunks").scope();
        for (level, planes) in sun_pages.iter().filter(|_| blocks.cast_shadows) {
            let sunlit = Facing::Light(back * Vec3::from(frame.sun_direction));
            let planes = turned_planes(planes, corner, back);
            lists[VSM_LIST as usize + level] =
                pick_grouped(grouped, &planes, (sunlit, detail), (None, shown), Parts::With, None);
        }
        drop(sun_chunks);
        let lit = lamps.slots.iter().enumerate().filter_map(|(slot, lamp)| Some((slot, (*lamp)?)));
        for (slot, (at, reach)) in lit.filter(|_| blocks.cast_shadows) {
            let planes = turned_planes(&crate::shadow_cache::reach_planes(at, reach), corner, back);
            let from = back * (at - corner);
            // What the world shows from the place nearest the lamp, so its shadow is the same wherever the camera is.
            let nearest = |place: &&voxear_terrain::blocks::BlockPlace| {
                (place.at[0] - from.x).powi(2) + (place.at[1] - from.z).powi(2)
            };
            let shown =
                blocks.places.iter().min_by(|a, b| nearest(a).total_cmp(&nearest(b))).map(|place| &place.columns);
            let picked =
                |parts| pick_grouped(grouped, &planes, (Facing::Camera(from), None), (None, shown), parts, None);
            let list = LIST_LOCAL_SHADOW as usize + slot;
            // A lamp drawing its virtual shadow pages takes what stands still into them, kept or not yet.
            if blocks.turn.is_none() && (lamps.is_cached(slot) || lamps.redraws(slot)) {
                if lamps.redraws(slot) {
                    lists[LAMP_KEPT as usize + slot] = picked(Parts::Without);
                }
                lists[list] = picked(if lamps.is_cached(slot) { Parts::Only } else { Parts::With });
            } else {
                lists[list] = picked(Parts::With);
            }
        }
        runs.push((number as u32, blocks.smooth, lists));
    }
    buffer.write(0, &records);
    buffer.write(SURFACES_AT as u64, &surfaces);
    let cascades = (0..CASCADES).flat_map(|c| [LIST_SHADOW as usize + c, SHADOW_CUT as usize + c]).collect::<Vec<_>>();
    let shadows = runs.iter().any(|(_, _, lists)| cascades.iter().any(|&list| !lists[list].is_empty()));
    let pools = drawn.iter().take(MAX_BLOCK_WORLDS).filter_map(|blocks| blocks.id).collect();
    let indices = drawn[0].indices;
    Some(ViewBlocks { runs: runs.into(), water: water.into(), pools, shadows, indices, draws: None })
}

/// Where a world's lists of what a lamp keeps of it start, beyond the lists
/// the GPU fills: one a lamp, by its slot.
const LAMP_KEPT: u32 = LIST_COUNT;

/// Where a world's lists of what each sun cascade draws cut out from its
/// pictures start, after the lamps' (see [`record_shadows`]): one a
/// cascade. Its list at `LIST_SHADOW` draws the rest.
const SHADOW_CUT: u32 = LAMP_KEPT + LOCAL_SHADOW_LIGHTS as u32;

/// Where its lists for the levels of the sun's virtual shadow maps start,
/// after those: one a level.
const VSM_LIST: u32 = SHADOW_CUT + CASCADES as u32;

/// How many lists each world has.
const LISTS: usize = VSM_LIST as usize + crate::vsm::LEVELS;

/// The first sun cascade whose texels are as large as a block's pictures'
/// pixels, into which faces cut out from their pictures cast as solid.
const SOLID_CASCADE: usize = CASCADES - 1;

impl ViewBlocks {
    /// The lamps' slots, a bit each, for which a world draws something
    /// each frame over what they keep.
    pub fn moving_for_lamps(&self, lamps: &crate::shadow_cache::Lamps) -> u32 {
        let moving =
            |slot: usize| self.runs.iter().any(|(_, _, lists)| !lists[LIST_LOCAL_SHADOW as usize + slot].is_empty());
        (0..lamps.slots.len())
            .filter(|&slot| lamps.is_cached(slot) && moving(slot))
            .fold(0, |bits, slot| bits | 1 << slot)
    }

    /// Whether any world draws anything into a lamp's views: what it keeps
    /// of them (`kept`), or what it draws each frame.
    pub fn draws_for_lamp(&self, slot: u32, kept: bool) -> bool {
        let stored = if kept { LAMP_KEPT + slot } else { LIST_LOCAL_SHADOW + slot };
        draws(self, stored)
    }
}

/// Each world's quads a level of the sun's virtual shadow maps takes, drawn
/// into page `view` (`VSM_PAGE_VIEW` on), as `draw_runs` draws a list.
pub(crate) fn draw_sun_page(ctx: &PassContext, view: &ViewBlocks, push: &Push, level: usize, page: u32) {
    let list = VSM_LIST + level as u32;
    for (world, (number, _, _)) in view.runs.iter().enumerate() {
        view.draw_list(ctx, &Push { list, level: number + 1, source: page, ..*push }, world, list);
    }
}

/// Each world's quads a lamp's slot draws into its view `view`: what it
/// keeps of them (`kept`), or what it draws each frame, as `draw_runs`
/// draws a list.
pub(crate) fn draw_lamp_runs(ctx: &PassContext, view: &ViewBlocks, push: &Push, slot: u32, kept: bool, at: u32) {
    let stored = if kept { LAMP_KEPT + slot } else { LIST_LOCAL_SHADOW + slot };
    let list = LIST_LOCAL_SHADOW + slot;
    for (world, (number, _, _)) in view.runs.iter().enumerate() {
        view.draw_list(ctx, &Push { list, level: number + 1, source: at, ..*push }, world, stored);
    }
}

/// Each world's quads list `list` draws: six vertices a quad (drawn
/// smooth, a vertex every two pairs of words), the world's number in the
/// view and one in `level`, and the run's first quad in `output`, as the
/// shaders' vertex numbers leave a draw's first out.
fn draw_runs(ctx: &PassContext, view: &ViewBlocks, push: &Push, list: u32) {
    draw_runs_from(ctx, view, push, (list, list));
}

/// As [`draw_runs`], each world's runs of its list `from`, drawn as list
/// `list` is.
fn draw_runs_from(ctx: &PassContext, view: &ViewBlocks, push: &Push, (from, list): (u32, u32)) {
    for (world, (number, _, _)) in view.runs.iter().enumerate() {
        view.draw_list(ctx, &Push { list, level: number + 1, ..*push }, world, from);
    }
}

/// Each world's water in view, six vertices a quad (drawn smooth, a vertex
/// every two pairs of words), as `draw_runs` draws its ground: drawn with
/// the water (`water::record`), with `pipeline`.
pub(crate) fn draw_water(ctx: &PassContext, view: &ViewBlocks, push: &Push, pipeline: vk::Pipeline) {
    if !view.has_water() {
        return;
    }
    bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, push);
    for (number, smooth, runs) in view.water.iter() {
        for &[first, count] in runs {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { level: number + 1, output: first, ..*push });
            let vertices = if *smooth { count / 2 } else { count * 6 };
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0) };
        }
    }
}

/// Whether any world draws anything in list `list`.
fn draws(view: &ViewBlocks, list: u32) -> bool {
    view.runs.iter().any(|(_, _, lists)| !lists[list as usize].is_empty())
}

/// **Shadows (blocks)**: the faces each cascade sees, into its corner of
/// the sun's atlas, over what the other shadow passes drew: those of solid
/// surfaces as depth alone, which the GPU tests before shading anything,
/// then those cut out from their pictures, as leaves are, whose fragments
/// may be thrown away where the picture is clear.
pub(crate) fn record_shadows(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    view: &ViewBlocks,
    (atlas, cascade_size): (ImageId, u32),
) {
    if !view.shadows {
        return;
    }
    let reads: Vec<_> = view.pools.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    let view = view.clone();
    fg.add_pass("Shadows (blocks)", &[(atlas, ImageAccess::DepthAttachment)], &reads, move |ctx| {
        let size = cascade_size * 2;
        let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
        for (pipeline, lists) in [(pipelines.block_shadow_solid, LIST_SHADOW), (pipelines.block_shadow, SHADOW_CUT)] {
            if !(0..CASCADES as u32).any(|cascade| draws(&view, lists + cascade)) {
                continue;
            }
            bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
            for cascade in 0..CASCADES as u32 {
                let offset =
                    vk::Offset2D { x: (cascade % 2 * cascade_size) as i32, y: (cascade / 2 * cascade_size) as i32 };
                set_viewport(ctx, offset, vk::Extent2D { width: cascade_size, height: cascade_size });
                draw_runs_from(ctx, &view, &push, (lists + cascade, LIST_SHADOW + cascade));
            }
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Depth (blocks)**: the faces in view as depth alone, with each pixel's
/// normal and roughness, over what the depth pass drew.
pub(crate) fn record_depth(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    view: &ViewBlocks,
    (depth, surface): (ImageId, ImageId),
) {
    if !draws(view, LIST_EARLY) {
        return;
    }
    let reads: Vec<_> = view.pools.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    let images = [(depth, ImageAccess::DepthAttachment), (surface, ImageAccess::ColorAttachment)];
    let view = view.clone();
    fg.add_pass("Depth (blocks)", &images, &reads, move |ctx| {
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        let rough = [attachment(ctx.view(surface), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, ctx.extent(depth), &rough, Some(&target));
        bind(ctx, pipelines.block_depth, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_runs(ctx, &view, &push, LIST_EARLY);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Opaque (blocks)**: the faces in view shaded, over what the opaque
/// passes drew; `pipeline` writes depth or tests against the depth passes'.
pub(crate) fn record_opaque(
    fg: &mut FrameGraph<'_>,
    push: Push,
    pipeline: vk::Pipeline,
    view: &ViewBlocks,
    images: Vec<(ImageId, ImageAccess)>,
    (hdr, velocity, depth): (ImageId, ImageId, ImageId),
    buffers: &[(BufferId, BufferAccess)],
) {
    if !draws(view, LIST_EARLY) {
        return;
    }
    let mut reads: Vec<_> = buffers.to_vec();
    reads.extend(view.pools.iter().map(|&id| (id, BufferAccess::ShaderRead)));
    let view = view.clone();
    fg.add_pass("Opaque (blocks)", &images, &reads, move |ctx| {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let color = [attachment(ctx.view(hdr), layout, None), attachment(ctx.view(velocity), layout, None)];
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, ctx.extent(hdr), &color, Some(&target));
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_runs(ctx, &view, &push, LIST_EARLY);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// The foot of a fall of a world of cubes' water, where it lands (see
/// `ChunkMesh::falls`): the middle of the foot, in the world; how far it
/// reaches along x and z, falls landing side by side being one; and how
/// high the fall stands over it, all in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FallFoot {
    pub at: Vec3,
    pub size: [f32; 2],
    pub high: f32,
}

/// The most falls that throw up a splash in a frame, the nearest the camera,
/// and how far from it one still does, in metres.
const SPLASHING_FALLS: usize = 8;
const SPLASH_REACH: f32 = 96.0;

/// The feet of the falls nearest `camera`, the nearest first, among the
/// worlds of cubes: the splash at each is thrown up by `particles::gather`.
pub(crate) fn fall_feet(world: &World, camera: Vec3) -> Vec<FallFoot> {
    use voxear_scene::{Destroying, Transform, WorldTransform};
    let side = CHUNK as i32;
    let mut feet: Vec<(f32, FallFoot)> = Vec::new();
    let worlds = world.query_ref::<(&BlockWorld, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
    for (blocks, local, placed) in worlds {
        if blocks.smooth {
            continue;
        }
        let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
        let size = blocks.block_size;
        // Falls landing side by side at one height are one: each by its lowest and highest block, and its height.
        let mut joined: Vec<([i32; 3], [i32; 3], u8)> = Vec::new();
        for (key, mesh) in blocks.meshes.iter().filter(|(_, mesh)| !mesh.falls.is_empty()) {
            let middle = corner + (Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) + 0.5) * (side as f32 * size);
            if middle.distance(camera) > SPLASH_REACH + side as f32 * size {
                continue;
            }
            for &[x, y, z, high] in &mesh.falls {
                let at = [key[0] * side + i32::from(x), key[1] * side + i32::from(y), key[2] * side + i32::from(z)];
                let beside = joined.iter_mut().find(|(low, top, _)| {
                    low[1] == at[1] && (0..3).step_by(2).all(|i| at[i] >= low[i] - 1 && at[i] <= top[i] + 1)
                });
                match beside {
                    Some((low, top, tallest)) => {
                        (low[0], low[2]) = (low[0].min(at[0]), low[2].min(at[2]));
                        (top[0], top[2]) = (top[0].max(at[0]), top[2].max(at[2]));
                        *tallest = (*tallest).max(high);
                    }
                    None => joined.push((at, at, high)),
                }
            }
        }
        for (low, top, high) in joined {
            let across = |i: usize| (top[i] - low[i] + 1) as f32 * size;
            let middle = |i: usize| (low[i] + top[i] + 1) as f32 * 0.5 * size;
            let at = corner + Vec3::new(middle(0), low[1] as f32 * size, middle(2));
            let away = at.distance(camera);
            if away <= SPLASH_REACH {
                feet.push((away, FallFoot { at, size: [across(0), across(2)], high: f32::from(high) * size }));
            }
        }
    }
    feet.sort_by(|a, b| a.0.total_cmp(&b.0));
    feet.truncate(SPLASHING_FALLS);
    feet.into_iter().map(|(_, foot)| foot).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nearer_chunk_takes_the_room_of_the_farthest_and_chunks_about_as_far_off_do_not_swap() {
        // Chunks along x as far off as their place, each made but the one 1 chunk off, with room for 10 vertices
        // and 20 indices.
        let order: Vec<([i32; 3], f32)> = [0, 1, 2, 3, 10, 11].map(|x| ([x, 0, 0], x as f32)).to_vec();
        let made = |key: &[i32; 3]| (key[0] != 1).then_some([10, 20]);
        assert_eq!(giving_up(&order, made, [([10, 20], 1.0)]), vec![[11, 0, 0]], "the farthest gives its room up");
        // Needing more than one has, the next farthest gives theirs up too, but none within the margin.
        let given = giving_up(&order, made, [([15, 20], 1.0)]);
        assert_eq!(given, vec![[11, 0, 0], [10, 0, 0]]);
        assert_eq!(giving_up(&order, made, [([35, 20], 1.0)]), given, "3 chunks off is within 2 of 1");
        // Each wanting room is given it by those far enough off from it.
        let given = giving_up(&order, made, [([10, 20], 1.0), ([10, 20], 7.5)]);
        assert_eq!(given, vec![[11, 0, 0], [10, 0, 0]]);
        assert_eq!(giving_up(&order, made, [([10, 20], 1.0), ([10, 20], 8.5)]), vec![[11, 0, 0]]);
        assert!(giving_up(&order, made, [([10, 20], 9.5)]).is_empty(), "11 chunks off is within 2 of 9.5");
    }

    #[test]
    fn a_chunks_mesh_for_rays_takes_each_faces_surfaces_material_and_picture_size() {
        use voxear_terrain::blocks::{
            BlockFaces, BlockMap,
            smooth::{self, Ball},
        };
        let look = |material: u32, size: f32| SurfaceLook {
            material: MaterialHandle(material),
            size,
            sharpness: 0.0,
            triplanar: true,
            tracks: 0.0,
            fills_in: 0.0,
            liquid: Default::default(),
            glitter: false,
            leafy: false,
            mixed: false,
            tint: 0,
        };
        // Its top drawn with surface 0, its sides with 1 and its bottom with 2.
        let looks = [look(7, 2.0), look(8, 1.0), look(9, 4.0)];
        let faces = [BlockFaces::default(), BlockFaces::new(0, 1, 2)];
        let painted = |vertex: &Vertex| {
            let material = u32::from_le_bytes(vertex.wind);
            match vertex.normal[1] {
                up if up >= SMOOTH_TOP => material == PROXY_MATERIAL | 7,
                up if up <= -SMOOTH_TOP => material == PROXY_MATERIAL | 9,
                _ => material == PROXY_MATERIAL | 8,
            }
        };
        // A floor of cubes: each face its own surface's material, its pictures that surface's size.
        let mut map = BlockMap::default();
        for z in 0..4 {
            for x in 0..4 {
                map.set([x, 0, z], 1);
            }
        }
        let cubes = voxear_terrain::blocks::mesh::mesh_chunk(&map, [0, 0, 0], &faces);
        let rays = proxy(&cubes, false, 1.0, Vec3::ZERO, (&looks, 1.0));
        assert!(rays.vertices.iter().all(painted));
        let top = rays.vertices.iter().find(|vertex| vertex.normal[1] > 0.5 && vertex.position[0] > 0.0).unwrap();
        assert_eq!(top.uv[0], top.position[0] / 2.0, "the top's picture two blocks across");
        let bottom = rays.vertices.iter().find(|vertex| vertex.normal[1] < -0.5 && vertex.position[0] > 0.0).unwrap();
        assert_eq!(bottom.uv[0], bottom.position[0] / 4.0);
        // With no looks, the instance's material, the world's picture size.
        let plain = proxy(&cubes, false, 1.0, Vec3::ZERO, (&[], 0.5));
        assert!(plain.vertices.iter().all(|vertex| vertex.wind == NO_WIND_MASKS));
        assert_eq!(plain.vertices[0].uv, rays.vertices[0].uv.map(|v| v / 2.0));
        // A smooth ball: its top's material where it faces up, its bottom's where it faces down, its sides' between.
        let mut map = BlockMap::default();
        for (at, block, fill) in smooth::ball(&map, [16.0; 3], 8.0, Ball::Add(1), &faces) {
            map.set_cell(at, block, fill);
        }
        let ball = smooth::mesh_chunk(&map, [0, 0, 0], &faces);
        let rays = proxy(&ball, true, 1.0, Vec3::ZERO, (&looks, 1.0));
        assert!(rays.vertices.iter().all(painted));
        for material in [7, 8, 9] {
            assert!(rays.vertices.iter().any(|vertex| u32::from_le_bytes(vertex.wind) == PROXY_MATERIAL | material));
        }
    }

    #[test]
    fn a_view_draws_the_chunks_it_sees_and_only_their_faces_turned_towards_it() {
        // Two chunks along x, 32 m each, the second's quads following on from the first's.
        let chunk = |x: f32, first: u32| ChunkDraw {
            key: [0, (x / 32.0) as i32, 0, 0],
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cube: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cut: [0; FACINGS],
            facings: std::array::from_fn(|f| [first + 10 * f as u32, 10]),
            loose: [0, 0],
            parts: [0, 0],
            water: [first + 60, 0],
            cone: None,
            mesh: 0,
        };
        let chunks = [chunk(0.0, 0), chunk(32.0, 60)];
        // From above the first, beyond its +x side: no -y faces of either, and none of the second's +x.
        let picked = pick(&chunks, &[], (Facing::Camera(Vec3::new(16.0, 40.0, 16.0)), None), None, None);
        let has = |first: u32| picked.iter().any(|&[f, c]| f <= first && first < f + c);
        assert!(has(0) && has(20), "the first's +x and +y faces");
        assert!(!has(30) && !has(90), "no faces turned down");
        assert!(!has(60), "the second's +x faces are beyond the camera");
        assert!(has(70), "its -x faces face back towards it");
        // Smooth chunks, three levels of 40, 15 and 5 quads' words each, the second's following on from the first's.
        let smooth = |x: f32, first: u32, cone: [f32; 4]| {
            let levels = [[first, 40], [first + 40, 15], [first + 55, 5]];
            let facings = std::array::from_fn(|f| levels.get(f).copied().unwrap_or([first + 60, 0]));
            ChunkDraw { facings, cone: Some(cone), ..chunk(x, first) }
        };
        // Facing every way, each drawn whole at its first level; the second's first following on from the first's.
        let mut second = smooth(32.0, 60, EVERY_WAY);
        second.facings[0] = [40, 40];
        let whole = [smooth(0.0, 0, EVERY_WAY), second];
        let from = Facing::Camera(Vec3::new(16.0, 40.0, 16.0));
        assert_eq!(pick(&whole, &[], (from, None), None, None), vec![[0, 80]]);
        // Looking along +x from x = 40: only the second.
        let ahead = pick(&whole, &[[1.0, 0.0, 0.0, -40.0]], (Facing::Camera(Vec3::new(40.0, 16.0, 16.0)), None), None, None);
        assert_eq!(ahead, vec![[40, 40]]);
        // Only the columns shown from where the view looks, where the world says which: the second's.
        let shown = CoordSet::from_iter([[0, 1, 0]]);
        assert_eq!(pick(&whole, &[], (from, None), None, Some(&shown)), vec![[40, 40]]);
        // Only the chunks the camera can see into, where it is known which.
        let seen = CoordSet::from_iter([[1, 0, 0]]);
        assert_eq!(pick(&whole, &[], (from, None), Some(&seen), None), vec![[40, 40]]);
        // Further away, coarser: 2 m blocks, half a block allowed at 1000 m and two blocks at 4000.
        let at = |x: f32| Detail { camera: Vec3::new(x, 16.0, 16.0), allowed: 0.001, block_size: 2.0, rings: None };
        let level = |x: f32| pick(&whole[..1], &[], (Facing::Camera(at(x).camera), Some(at(x))), None, None);
        assert_eq!([level(-500.0), level(-2000.0), level(-9000.0)], [vec![[0, 40]], vec![[40, 15]], vec![[55, 5]]]);
        // A smooth chunk, its triangles all facing within a tenth of up: drawn from above or beside it, not at all
        // from below, nor into the shadows of light coming up from under it.
        let up = [smooth(0.0, 0, [0.0, 1.0, 0.0, 0.1])];
        let drawn = |facing: Facing| !pick(&up, &[], (facing, None), None, None).is_empty();
        assert!(drawn(Facing::Camera(Vec3::new(16.0, 60.0, 16.0))));
        assert!(!drawn(Facing::Camera(Vec3::new(16.0, -60.0, 16.0))));
        assert!(drawn(Facing::Camera(Vec3::new(60.0, 10.0, 16.0))), "beside it");
        assert!(drawn(Facing::Light(Vec3::new(0.2, -1.0, 0.0))));
        assert!(!drawn(Facing::Light(Vec3::new(0.2, 1.0, 0.0))));
        // One facing every way, drawn from anywhere.
        for from in [Vec3::new(16.0, -60.0, 16.0), Vec3::new(-60.0, 10.0, 16.0)] {
            assert_eq!(pick(&whole[..1], &[], (Facing::Camera(from), None), None, None), vec![[0, 40]]);
        }
        assert_eq!(pick(&whole[..1], &[], (Facing::Light(Vec3::new(0.2, 1.0, 0.0)), None), None, None), vec![[0, 40]]);
        // The sun's cascades: of each chunk, the faces turned towards sunlight going down, to -x and to +z.
        let lit = pick(&chunks, &[], (Facing::Light(Vec3::new(-0.3, -0.8, 0.5)), None), None, None);
        let has = |first: u32| lit.iter().any(|&[f, c]| f <= first && first < f + c);
        assert!([0, 20, 50, 60, 80, 110].map(has).iter().all(|&h| h), "+x, +y and -z faces: {lit:?}");
        assert!(![10, 30, 40, 70, 90, 100].map(has).iter().any(|&h| h), "none turned away: {lit:?}");
    }

    #[test]
    fn the_depth_pass_draws_the_chunks_nearest_the_camera_first_each_chunks_quads_in_order() {
        // Three chunks along x, 32 m each, held in the pool far one first: the camera at the near end.
        let chunk = |x: f32, first: u32| ChunkDraw {
            key: [0, (x / 32.0) as i32, 0, 0],
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cube: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cut: [0; FACINGS],
            facings: std::array::from_fn(|f| [first + 10 * f as u32, 10]),
            loose: [0, 0],
            parts: [0, 0],
            water: [first + 60, 0],
            cone: None,
            mesh: 0,
        };
        let chunks = [chunk(64.0, 0), chunk(32.0, 60), chunk(0.0, 120)];
        let camera = Vec3::new(-8.0, 40.0, 16.0);
        let facing = (Facing::Camera(camera), None);
        let near_first = pick_parts(&chunks, &[], facing, (None, None), Parts::With, Some(camera));
        // Seen from -x, above and within z: each chunk's -x, +y, +z and -z faces, the nearest chunk's first, each
        // chunk's in pool order and those that follow on as one.
        assert_eq!(near_first, vec![[130, 20], [160, 20], [70, 20], [100, 20], [10, 20], [40, 20]]);
        // The same quads as drawn in the pool's order, those that follow on as one.
        let mut quads: Vec<u32> = near_first.iter().flat_map(|&[first, count]| first..first + count).collect();
        quads.sort_unstable();
        let pooled: Vec<u32> = pick(&chunks, &[], facing, None, None).iter().flat_map(|&[f, c]| f..f + c).collect();
        assert_eq!(quads, pooled);
        // Runs that follow on in the order given are joined, and none out of it.
        assert_eq!(joined(vec![[0, 4], [4, 2], [10, 1], [6, 1]]), vec![[0, 6], [10, 1], [6, 1]]);
    }

    #[test]
    fn a_view_draws_the_water_of_every_chunk_it_sees_whichever_way_it_faces() {
        // Three chunks along x, the first and last with water after their ground's quads.
        let chunk = |x: f32, first: u32, water: u32| ChunkDraw {
            key: [0, (x / 32.0) as i32, 0, 0],
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cube: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            cut: [0; FACINGS],
            facings: std::array::from_fn(|f| [first + 10 * f as u32, 10]),
            loose: [0, 0],
            parts: [0, 0],
            water: [first + 60, water],
            cone: None,
            mesh: 0,
        };
        let chunks = [chunk(0.0, 0, 5), chunk(32.0, 65, 0), chunk(64.0, 125, 7)];
        assert_eq!(pick_water(&chunks, &[], None, None), vec![[60, 5], [185, 7]]);
        // Only those in view, and those the camera can see into.
        assert_eq!(pick_water(&chunks, &[[1.0, 0.0, 0.0, -40.0]], None, None), vec![[185, 7]]);
        let seen = CoordSet::from_iter([[0, 0, 0]]);
        assert_eq!(pick_water(&chunks, &[], Some(&seen), None), vec![[60, 5]]);
    }

    #[test]
    fn cut_out_faces_go_last_in_each_facing_and_cast_apart_from_the_solid() {
        use voxear_terrain::blocks::mesh::pack;
        // Surface 1 is leaves, cut out; 0 and 2 are solid. Two facings, three quads and one.
        let quad = |x: u32, surface: u8| pack([x, 0, 0], [1, 1], 0, surface, [3; 4]);
        let mut mesh = ChunkMesh { quads: vec![quad(0, 1), quad(1, 0), quad(2, 2), quad(3, 1)], ..Default::default() };
        mesh.buckets = [0, 3, 4, 4, 4, 4, 4, 4, 4];
        // Each quad's light, by the place it came in: it goes where its quad goes.
        mesh.light = vec![10, 11, 12, 13];
        let cut: Arc<[bool]> = Arc::from([false, true, false]);
        let (quads, light, counts) = arranged(&mesh, 5, &cut);
        let surfaces: Vec<u8> = quads.iter().map(|quad| quad[1] as u8).collect();
        assert_eq!(surfaces, [0, 2, 1, 1], "the solid first in the first facing, in the order they came");
        assert_eq!(light, [11, 12, 10, 13], "each quad's light with it");
        assert_eq!(counts, [1, 1, 0, 0, 0, 0]);
        assert!(quads.iter().all(|quad| quad[1] >> 16 == 5), "each with its slot");
        // A world with nothing cut out keeps its quads as they came; one made with no light is open sky.
        assert_eq!(arranged(&mesh, 0, &Arc::from([false; 3])).2, [0; FACINGS]);
        let unlit = ChunkMesh { light: Vec::new(), ..mesh.clone() };
        assert_eq!(arranged(&unlit, 0, &cut).1, [OPEN_SKY_WORD; 4]);
        // Drawn from the pool's 100th quad: the cut runs are the last of each facing.
        let draw = ChunkDraw {
            key: [0; 4],
            bounds: [Vec3::ZERO, Vec3::splat(32.0)],
            cube: [Vec3::ZERO, Vec3::splat(32.0)],
            facings: [[100, 3], [103, 1], [104, 0], [104, 0], [104, 0], [104, 0]],
            cut: counts,
            loose: [104, 2],
            parts: [0, 0],
            water: [106, 0],
            cone: None,
            mesh: 0,
        };
        let runs = cut_runs(&[draw]);
        assert_eq!(runs, vec![[102, 1], [103, 1], [104, 2]], "the leaves of each facing and the sheets");
        // One run over all of them comes apart into its solid quads and its cut ones.
        assert_eq!(cut_apart(&runs, vec![[100, 6]]), (vec![[100, 2]], vec![[102, 1], [103, 1], [104, 2]]));
        // Runs that only touch a cut run, or start inside one, are cut where they cross.
        assert_eq!(cut_apart(&runs, vec![[100, 1], [103, 2]]), (vec![[100, 1]], vec![[103, 1], [104, 1]]));
    }

    #[test]
    fn a_chunks_box_is_where_its_faces_are_not_its_whole_cube() {
        // Ground faces between heights 3 and 5, and a sheet of leaves in the block at height 9.
        let mut mesh = ChunkMesh {
            positions: vec![[0.0, 3.0, 0.0], [32.0, 5.0, 32.0]],
            quads: vec![voxear_terrain::blocks::mesh::pack_loose([4, 9, 4], 6, 0, 1, 0, 0)],
            ..Default::default()
        };
        mesh.buckets = [0, 0, 0, 0, 0, 0, 0, 1, 1];
        assert_eq!(faces_box(&mesh), Some([Vec3::new(0.0, 3.0, 0.0), Vec3::new(32.0, 10.0, 32.0)]));
        assert_eq!(faces_box(&ChunkMesh::default()), None, "water alone may lie anywhere in its cube");
    }
}
