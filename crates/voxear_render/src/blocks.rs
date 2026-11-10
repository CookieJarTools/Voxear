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

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use voxear_core::math::{Mat3, Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, MemoryLocation, Retired, vk};
use voxear_terrain::blocks::{
    BlockWorld, ChunkMeshes,
    chunk::CHUNK,
    mesh::{ChunkMesh, FACINGS},
    smooth,
};

use crate::{
    MaterialHandle, MeshHandle,
    alloc::{RangeAllocator, SlotAllocator},
    assets::Assets,
    gpu::{GpuInstance, INSTANCE_RAYS_ONLY, LIST_COUNT, LIST_EARLY, LIST_SHADOW, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    mesh::{MeshData, NO_WIND_MASKS, Vertex},
    passes::{attachment, begin_rendering, bind, set_viewport},
    pipelines::Pipelines,
    scene::{ExtractedFrame, Owner},
    shadows::{CASCADE_RESOLUTION, CASCADES},
    terrain::{GpuTerrainSurface, SurfaceLook, surface_flags},
};

/// How a world of blocks looks: the material a face with no surface of its
/// own is drawn with, and its surfaces, the ways its faces are drawn (see
/// `voxear_terrain::blocks::file::BlockFile::faces`).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockLook {
    pub material: MaterialHandle,
    pub surfaces: Arc<[SurfaceLook]>,
}

impl Component for BlockLook {}

/// The most worlds of blocks a view draws.
pub const MAX_BLOCK_WORLDS: usize = 16;

/// The most surfaces a world of blocks has.
pub const MAX_BLOCK_SURFACES: usize = 256;

/// The most chunks with faces a world draws.
const SLOTS: u32 = 65_536;

/// Where a pool's quads start: after each slot's chunk's place.
const QUADS_AT: u64 = SLOTS as u64 * 16;

/// How many quads a pool starts with room for; it doubles when full.
const FIRST_CAPACITY: u64 = 1 << 16;

/// How many chunks get their meshes for rays in a frame, at most.
const PROXIES_A_FRAME: usize = 8;

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
}

/// A world of blocks' flag for being drawn smooth: its pool holds triangles'
/// corners, two pairs of words each, in place of quads.
pub const BLOCKS_SMOOTH: u32 = 1;

/// Where a view's block buffer keeps its surfaces, after its worlds.
const SURFACES_AT: usize = MAX_BLOCK_WORLDS * size_of::<GpuBlockWorld>();

/// How large a view's block buffer is.
pub const FRAME_SIZE: u64 =
    (SURFACES_AT + MAX_BLOCK_WORLDS * MAX_BLOCK_SURFACES * size_of::<GpuTerrainSurface>()) as u64;

/// One chunk in a pool: the faces it was made from, its slot, where its
/// quads start, and the box it lies in, in metres from the world's corner.
struct Held {
    mesh: Arc<ChunkMesh>,
    slot: u32,
    first: u64,
    bounds: [Vec3; 2],
}

/// One chunk as a view picks what to draw of it: its level and place, its
/// box, where each facing's quads start in the pool and how many, +x
/// first, and its water's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChunkDraw {
    pub key: [i32; 4],
    pub bounds: [Vec3; 2],
    pub facings: [[u32; 2]; FACINGS],
    pub water: [u32; 2],
    /// Drawn smooth, the cone the ways its triangles face lie within (see
    /// `voxear_terrain::blocks::mesh::ChunkMesh::cone`), [`EVERY_WAY`] where
    /// they face every way; `None` for cubes, kept by the way they face.
    pub cone: Option<[f32; 4]>,
}

/// The cone of a smooth chunk whose triangles face every way: wider than
/// any turn, so never turned away.
const EVERY_WAY: [f32; 4] = [0.0, 1.0, 0.0, 2.0];

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
}

impl Detail {
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

/// What a view draws of a world's chunks: those within `planes` (inward,
/// in the world's space) and, where `seen` says which of its own chunks the
/// camera can see into, those; and of each, the faces `facing` picks, and of
/// a smooth one the level `detail` picks; as first quads and counts, those
/// that follow on in the pool drawn as one.
pub(crate) fn pick(
    chunks: &[ChunkDraw],
    planes: &[[f32; 4]],
    (facing, detail): (Facing, Option<Detail>),
    seen: Option<&HashSet<[i32; 3]>>,
) -> Vec<[u32; 2]> {
    // Turned towards the light, the faces whose way out goes against it.
    let lit = match facing {
        Facing::Light(way) => [way.x <= 0.0, way.x >= 0.0, way.y <= 0.0, way.y >= 0.0, way.z <= 0.0, way.z >= 0.0],
        _ => [true; FACINGS],
    };
    let mut picked: Vec<[u32; 2]> = Vec::new();
    for chunk in chunks {
        let [level, x, y, z] = chunk.key;
        if level == 0 && seen.is_some_and(|seen| !seen.contains(&[x, y, z])) {
            continue;
        }
        if !within(planes, chunk.bounds) {
            continue;
        }
        let [low, high] = chunk.bounds;
        // A smooth chunk is drawn whole at one of its levels, unless every triangle in it is turned away.
        if let Some(cone) = chunk.cone {
            let [first, count] = chunk.facings[detail.map_or(0, |detail| detail.level(chunk))];
            if count > 0 && !turned_away(cone, chunk.bounds, facing) {
                picked.push([first, count]);
            }
            continue;
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
                picked.push([first, count]);
            }
        }
    }
    merged(picked)
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

/// The water a view draws of a world's chunks: every face of the water of
/// the chunks it sees, as [`pick`] picks them.
pub(crate) fn pick_water(chunks: &[ChunkDraw], planes: &[[f32; 4]], seen: Option<&HashSet<[i32; 3]>>) -> Vec<[u32; 2]> {
    let picked = chunks.iter().filter(|chunk| chunk.water[1] > 0).filter(|chunk| {
        let [level, x, y, z] = chunk.key;
        (level != 0 || seen.is_none_or(|seen| seen.contains(&[x, y, z]))) && within(planes, chunk.bounds)
    });
    merged(picked.map(|chunk| chunk.water).collect())
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
    proxies: HashMap<[i32; 3], (Arc<ChunkMesh>, MeshHandle)>,
    block_size: f32,
    smooth: bool,
    draws: Option<Arc<[ChunkDraw]>>,
    /// Which of each of its own chunks' sides see each other, made again
    /// with its draws.
    sights: Option<Arc<HashMap<[i32; 3], u16>>>,
    seen: u64,
    /// The world's faces, its own and coarser, as the pool was last brought
    /// up to them, and as every one of its own had its mesh for rays made.
    made_from: Option<(Arc<ChunkMeshes>, Arc<HashMap<[i32; 4], Arc<ChunkMesh>>>)>,
    proxied: Option<Arc<ChunkMeshes>>,
    /// Its chunks' meshes for rays, each with where its chunk is from the
    /// world's corner, made again as they change.
    placed: Option<Arc<[(MeshHandle, Vec3)]>>,
}

impl Pool {
    fn create(gpu: &Gpu, capacity: u64) -> Result<Buffer> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        gpu.create_buffer("block quads", QUADS_AT + capacity * 8, usage, MemoryLocation::GpuOnly)
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
            block_size,
            smooth,
            draws: None,
            sights: None,
            seen: 0,
            made_from: None,
            proxied: None,
            placed: None,
        })
    }

    /// Brings the pool up to `world`'s chunks: stages the quads of each
    /// that changed, by pointer, with `stage(bytes, to)`, growing the pool
    /// when they do not fit.
    fn update(
        &mut self,
        gpu: &Gpu,
        frames: &mut Frames,
        world: &BlockWorld,
        frame_index: u64,
        stage: &mut dyn FnMut(&[u8], u64),
    ) -> Result<()> {
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
            return Ok(());
        }
        self.made_from = Some((world.meshes.clone(), world.far.clone()));
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
        let mut changed = self.held.len() != before;
        let side = CHUNK as f32 * world.block_size;
        let near = world.meshes.iter().map(|(k, mesh)| ([0, k[0], k[1], k[2]], mesh));
        for (key, mesh) in near.chain(world.far.iter().map(|(k, mesh)| (*k, mesh))) {
            let key = &key;
            if self.held.get(key).is_some_and(|held| Arc::ptr_eq(&held.mesh, mesh)) {
                continue;
            }
            if let Some(old) = self.held.remove(key) {
                self.freeing.push((frame_index, old.first, old.mesh.quads.len() as u64, old.slot));
            }
            let Some(slot) = self.slots.alloc() else {
                tracing::warn!("a world of blocks has more chunks than {SLOTS}; the rest are left out");
                continue;
            };
            let count = mesh.quads.len() as u64;
            let first = match self.space.alloc(count) {
                Some(first) => first,
                None => {
                    self.grow(gpu, frames, count, stage)?;
                    self.space.alloc(count).expect("room after growing")
                }
            };
            let quads: Vec<[u32; 2]> = mesh.quads.iter().map(|&[a, b]| [a, b | slot << 16]).collect();
            stage(bytemuck::cast_slice(&quads), QUADS_AT + first * 8);
            stage(bytemuck::cast_slice(&[key[1], key[2], key[3], key[0]]), u64::from(slot) * 16);
            // A coarser level's chunk is as many times larger as its blocks are; drawn smooth, its surface
            // reaches up to two blocks before it and past it.
            let scale = (1u32 << key[0].clamp(0, 16)) as f32;
            let side = side * scale;
            let low = Vec3::new(key[1] as f32, key[2] as f32, key[3] as f32) * side;
            let margin = if self.smooth { Vec3::splat(2.0 * self.block_size * scale) } else { Vec3::ZERO };
            let bounds = [low - margin, low + Vec3::splat(side) + margin];
            self.held.insert(*key, Held { mesh: mesh.clone(), slot, first, bounds });
            changed = true;
        }
        if changed {
            self.draws = None;
            self.sights = None;
        }
        Ok(())
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
        // Slots given up wait no longer: nothing draws from the old buffer's quads now.
        for (_, _, _, slot) in self.freeing.drain(..) {
            self.slots.free(slot);
        }
        for (key, held) in &mut self.held {
            held.first = self.space.alloc(held.mesh.quads.len() as u64).expect("room in a larger pool");
            let quads: Vec<[u32; 2]> = held.mesh.quads.iter().map(|&[a, b]| [a, b | held.slot << 16]).collect();
            stage(bytemuck::cast_slice(&quads), QUADS_AT + held.first * 8);
            stage(bytemuck::cast_slice(&[key[1], key[2], key[3], key[0]]), u64::from(held.slot) * 16);
        }
        self.draws = None;
        self.sights = None;
        Ok(())
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

    /// Each chunk to draw, made again only when the pool changed.
    fn draws(&mut self) -> Arc<[ChunkDraw]> {
        if self.draws.is_none() {
            let draws = self.held.iter().map(|(key, held)| {
                let buckets = held.mesh.buckets;
                let facings = std::array::from_fn(|f| [held.first as u32 + buckets[f], buckets[f + 1] - buckets[f]]);
                let (water, count) = held.mesh.water();
                let water = [held.first as u32 + water, count];
                let cone = self.smooth.then(|| held.mesh.cone.unwrap_or(EVERY_WAY));
                ChunkDraw { key: *key, bounds: held.bounds, facings, water, cone }
            });
            self.draws = Some(draws.collect());
        }
        self.draws.clone().expect("made above")
    }
}

/// A world of blocks to draw this frame, as every view draws it.
#[derive(Clone)]
pub(crate) struct DrawnBlocks {
    pub entity: Entity,
    pub corner: Vec3,
    pub block_size: f32,
    pub smooth: bool,
    /// Where its pool's chunks' places start, and its quads.
    pub address: u64,
    pub chunks: Arc<[ChunkDraw]>,
    /// For a world of cubes, its blocks and which of each chunk's sides see
    /// each other, for each view to pass over the chunks it cannot see into.
    pub sights: Option<(Arc<voxear_terrain::blocks::BlockMap>, Arc<HashMap<[i32; 3], u16>>)>,
    pub look: BlockLook,
    pub cast_shadows: bool,
    /// Its chunks' meshes for rays, each with where its chunk is from the
    /// world's corner.
    pub proxies: Arc<[(MeshHandle, Vec3)]>,
    /// Its pool in this frame's graph, for every view to wait on.
    pub id: Option<BufferId>,
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
    copies: Vec<Copy>,
    staging: Option<Buffer>,
    drawn: Vec<DrawnBlocks>,
    /// Whether it has said that the GI's rays are left without some chunks,
    /// once.
    warned_full: bool,
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

/// A chunk's mesh for rays: its faces merged by surface alone, in metres
/// from its corner, its pictures repeated as the world's are from `from`,
/// where the chunk is from the world's corner. Its triangles share their
/// corners, each facing the way the faces round it face on average: a
/// block's face has its corners to itself, so faces its own way, and a
/// smooth surface's run on smoothly.
fn proxy(mesh: &ChunkMesh, block_size: f32, from: Vec3, uv_scale: f32) -> MeshData {
    let points: Vec<Vec3> = mesh.positions.iter().map(|&p| Vec3::from(p) * block_size).collect();
    let mut ways = vec![Vec3::ZERO; points.len()];
    for triangle in mesh.indices.chunks_exact(3) {
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
        .map(|(&point, way)| {
            let normal = way.normalize_or(Vec3::Y);
            let tangent = Vec3::new(normal.y, -normal.x, 0.0).normalize_or(Vec3::X);
            let world = point + from;
            // Each face's pictures from the side it faces, as the shaders draw them.
            let uv = match axis_of(normal) {
                0 => [world.z, -world.y],
                1 => [world.x, -world.z],
                _ => [world.x, -world.y],
            };
            Vertex {
                position: point.to_array(),
                normal: normal.to_array(),
                uv: uv.map(|v| v * uv_scale),
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind: NO_WIND_MASKS,
            }
        })
        .collect();
    MeshData { vertices, indices: mesh.indices.clone(), ..MeshData::default() }
}

impl BlockWorlds {
    /// Brings every world of blocks in `world` up to date on the GPU,
    /// staging the chunks that changed, and lists what to draw.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        assets: &mut Assets,
        frames: &mut Frames,
        world: &World,
        frame_index: u64,
    ) -> Result<()> {
        voxear_core::profile_scope!("Block Uploads");
        self.drawn.clear();
        self.copies.clear();
        // Staged and never recorded: every chunk staged again.
        if let Some(staging) = self.staging.take() {
            frames.retire(Retired::Buffer(staging));
            for (_, pool) in self.resident.drain() {
                retire(pool, assets, frames, frame_index);
            }
        }
        let features = world.resource::<crate::RenderFeatures>().copied().unwrap_or_default();
        let any_hidden = world.query_ref::<&voxear_scene::Hidden>().into_iter().next().is_some();
        let found: Vec<(Entity, &BlockWorld, Vec3)> = if features.terrain {
            world
                .query_ref::<(Entity, &BlockWorld, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
                .without::<voxear_scene::Destroying>()
                .into_iter()
                .map(|(entity, blocks, local, placed)| {
                    (entity, blocks, placed.map_or(local.position, |placed| placed.0.w_axis.truncate()))
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut staged: Vec<u8> = Vec::new();
        let mut proxies_left = PROXIES_A_FRAME;
        for (entity, blocks, corner) in found {
            if any_hidden && crate::scene::hidden(world, entity) {
                continue;
            }
            let look = world
                .get::<BlockLook>(entity)
                .cloned()
                .unwrap_or_else(|| BlockLook { material: assets.default_material, surfaces: Arc::from([]) });
            let changed = |pool: &Pool| pool.block_size != blocks.block_size || pool.smooth != blocks.smooth;
            if self.resident.get(&entity).is_some_and(changed) {
                let pool = self.resident.remove(&entity).expect("held");
                retire(pool, assets, frames, frame_index);
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
            pool.update(gpu, frames, blocks, frame_index, &mut stage)?;
            // Each chunk's mesh for rays, a few a frame, the old one shown until the new one is made.
            if !pool.proxied.as_ref().is_some_and(|made| Arc::ptr_eq(made, &blocks.meshes)) {
                let before = pool.proxies.len();
                pool.proxies.retain(|key, (_, mesh)| {
                    let kept = blocks.meshes.contains_key(key);
                    if !kept {
                        assets.free_mesh(*mesh, frame_index);
                    }
                    kept
                });
                let mut changed = pool.proxies.len() != before;
                let mut all = true;
                let side = CHUNK as f32 * blocks.block_size;
                let uv_scale = 1.0 / blocks.block_size.max(0.01);
                // Uploaded and built together, waited for once.
                let batch = gpu.batch();
                for (key, mesh) in blocks.meshes.iter() {
                    if pool.proxies.get(key).is_some_and(|(made, _)| Arc::ptr_eq(made, mesh)) {
                        continue;
                    }
                    if proxies_left == 0 {
                        all = false;
                        break;
                    }
                    proxies_left -= 1;
                    let from = Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                    let making = voxear_core::profile_marker!("Ray Mesh Make").scope();
                    let data = proxy(mesh, blocks.block_size, from, uv_scale);
                    let lods = crate::lod::rays_only(&data);
                    drop(making);
                    let _uploading = voxear_core::profile_marker!("Ray Mesh Upload").scope();
                    // Out of room, the rest wait: the GI's rays miss them, but the frame goes on.
                    let handle = match assets.add_mesh_with_lods(gpu, &data, &lods) {
                        Ok(handle) => handle,
                        Err(error) => {
                            if !std::mem::replace(&mut self.warned_full, true) {
                                tracing::warn!("a world of blocks' chunks are left out of the GI's rays: {error:#}");
                            }
                            // Tried again once its faces change, as room may be made by then.
                            proxies_left = 0;
                            break;
                        }
                    };
                    if let Some((_, old)) = pool.proxies.insert(*key, (mesh.clone(), handle)) {
                        assets.free_mesh(old, frame_index);
                    }
                    changed = true;
                }
                {
                    let _uploading = voxear_core::profile_marker!("Ray Mesh Upload").scope();
                    batch.finish()?;
                }
                if all {
                    pool.proxied = Some(blocks.meshes.clone());
                }
                if changed {
                    pool.placed = None;
                }
            }
            let side = CHUNK as f32 * blocks.block_size;
            let placed = pool
                .placed
                .get_or_insert_with(|| {
                    let at = |key: &[i32; 3]| Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                    pool.proxies.iter().map(|(key, (_, mesh))| (*mesh, at(key))).collect()
                })
                .clone();
            self.drawn.push(DrawnBlocks {
                entity,
                corner,
                block_size: blocks.block_size,
                smooth: blocks.smooth,
                address: pool.buffer.address,
                chunks: pool.draws(),
                sights: (!blocks.smooth).then(|| (blocks.blocks.clone(), pool.sights())),
                look,
                cast_shadows: blocks.cast_shadows,
                proxies: placed,
                id: None,
            });
        }
        let gone: Vec<Entity> =
            self.resident.iter().filter(|(_, pool)| pool.seen != frame_index).map(|(key, _)| *key).collect();
        for key in gone {
            let pool = self.resident.remove(&key).expect("held");
            retire(pool, assets, frames, frame_index);
        }
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
        let mut ids = HashMap::new();
        for drawn in &mut self.drawn {
            let pool = &self.resident[&drawn.entity];
            drawn.id = Some(*ids.entry(drawn.entity).or_insert_with(|| fg.import_buffer(&pool.buffer)));
        }
        if let Some(staging) = self.staging.take() {
            let copies: Vec<(vk::Buffer, vk::BufferCopy)> = self
                .copies
                .iter()
                .filter_map(|copy| {
                    let region = vk::BufferCopy { src_offset: copy.from, dst_offset: copy.to, size: copy.size };
                    Some((self.resident.get(&copy.key)?.buffer.handle, region))
                })
                .collect();
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
        for (_, pool) in self.resident.drain() {
            gpu.destroy_buffer(pool.buffer);
        }
    }
}

fn retire(pool: Pool, assets: &mut Assets, frames: &mut Frames, frame_index: u64) {
    frames.retire(Retired::Buffer(pool.buffer));
    for (_, (_, mesh)) in pool.proxies {
        assets.free_mesh(mesh, frame_index);
    }
}

/// Adds each world's chunks' meshes for rays to the frame's instances, so
/// the GI's rays see its blocks.
pub(crate) fn add_proxies(drawn: &[DrawnBlocks], extracted: &mut ExtractedFrame) {
    for blocks in drawn {
        for (part, (mesh, at)) in blocks.proxies.iter().enumerate() {
            let model = Mat4::from_translation(blocks.corner + *at);
            let normal = Mat3::from_mat4(model).inverse().transpose();
            extracted.instances.push(GpuInstance {
                model: crate::gpu::cols(model),
                prev_model: crate::gpu::cols(model),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                mesh: mesh.0,
                material: blocks.look.material.0,
                flags: INSTANCE_RAYS_ONLY,
                ..GpuInstance::default()
            });
            extracted.entities.push(Owner { entity: blocks.entity, plant: None, part: Some(part as u32) });
        }
    }
}

/// What a view draws of its worlds of blocks: for each world, its number in
/// the view, whether it is drawn smooth, and the quads (or corners' words)
/// each list draws; for each world with water in view, its number, whether
/// it is drawn smooth and its water's quads (or corners' words); their pools
/// to wait on; and whether any casts the sun's shadows.
#[derive(Clone)]
pub(crate) struct ViewBlocks {
    pub runs: Vec<(u32, bool, Vec<Vec<[u32; 2]>>)>,
    pub water: Vec<(u32, bool, Vec<[u32; 2]>)>,
    pub pools: Vec<BufferId>,
    pub shadows: bool,
}

impl ViewBlocks {
    /// Whether any world's water is in view.
    pub fn has_water(&self) -> bool {
        !self.water.is_empty()
    }

    /// How many draws and triangles its worlds take, in view and in the
    /// sun's cascades.
    pub fn counts(&self) -> BlockCounts {
        let mut counts = BlockCounts::default();
        for (_, smooth, lists) in &self.runs {
            // A quad is two triangles; drawn smooth, a triangle is three corners of two pairs of words.
            let triangles = |runs: &[[u32; 2]]| -> u64 {
                runs.iter().map(|&[_, count]| if *smooth { u64::from(count) / 6 } else { u64::from(count) * 2 }).sum()
            };
            counts.draws += lists[LIST_EARLY as usize].len() as u32;
            counts.triangles += triangles(&lists[LIST_EARLY as usize]);
            for cascade in 0..CASCADES {
                let list = &lists[LIST_SHADOW as usize + cascade];
                counts.shadow_draws += list.len() as u32;
                counts.shadow_triangles += triangles(list);
            }
        }
        for (_, smooth, runs) in &self.water {
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
/// anchor among its meshes for rays.
pub(crate) fn gather(
    drawn: &[DrawnBlocks],
    frame: &crate::gpu::GpuFrame,
    entities: &[Owner],
    has_sun: bool,
    buffer: &Buffer,
) -> Option<ViewBlocks> {
    if drawn.is_empty() {
        return None;
    }
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
        for (place, look) in surfaces[number * MAX_BLOCK_SURFACES..].iter_mut().zip(looks) {
            *place = GpuTerrainSurface {
                material: look.material.0,
                uv_scale: 1.0 / look.size.max(0.01),
                sharpness: 0.0,
                flags: surface_flags(look),
                tracks: 0.0,
                fills_in: 0.0,
            };
        }
        let surfaces_at = (SURFACES_AT + number * MAX_BLOCK_SURFACES * size_of::<GpuTerrainSurface>()) as u64;
        records.push(GpuBlockWorld {
            corner: blocks.corner.into(),
            block_size: blocks.block_size,
            quads: blocks.address + QUADS_AT,
            origins: blocks.address,
            surfaces: buffer.address + surfaces_at,
            surface_count: looks.len() as u32,
            material: blocks.look.material.0,
            anchor: anchors.get(&blocks.entity).copied().unwrap_or(0),
            flags: if blocks.smooth { BLOCKS_SMOOTH } else { 0 },
        });
        let local = camera - blocks.corner;
        let mut lists = vec![Vec::new(); LIST_COUNT as usize];
        let planes = voxear_terrain::lod::planes_from(&frame.planes, blocks.corner);
        // The chunks the camera can see into, walking from its own through their sides that see each other.
        let seen = blocks.sights.as_ref().and_then(|(map, sights)| {
            let side = CHUNK as f32 * blocks.block_size;
            let from = (local / side).floor().as_ivec3().to_array();
            let bounds = |key: [i32; 3]| {
                let low = Vec3::new(key[0] as f32, key[1] as f32, key[2] as f32) * side;
                [low, low + Vec3::splat(side)]
            };
            voxear_terrain::blocks::sight::seen(map, sights, from, |key| within(&planes, bounds(key)))
        });
        // A smooth chunk drawn as coarse as its distance allows, as models' levels are picked.
        let allowed = frame.lod_threshold / frame.lod_scale.max(1e-6);
        let detail = Some(Detail { camera: local, allowed, block_size: blocks.block_size });
        lists[LIST_EARLY as usize] = pick(&blocks.chunks, &planes, (Facing::Camera(local), detail), seen.as_ref());
        let wet = pick_water(&blocks.chunks, &planes, seen.as_ref());
        if !wet.is_empty() {
            water.push((number as u32, blocks.smooth, wet));
        }
        if has_sun && blocks.cast_shadows {
            let sunlit = Facing::Light(Vec3::from(frame.sun_direction));
            for cascade in 0..CASCADES {
                let planes = voxear_terrain::lod::planes_from(&frame.cascade_planes[cascade], blocks.corner);
                lists[LIST_SHADOW as usize + cascade] = pick(&blocks.chunks, &planes, (sunlit, detail), None);
            }
        }
        runs.push((number as u32, blocks.smooth, lists));
    }
    buffer.write(0, &records);
    buffer.write(SURFACES_AT as u64, &surfaces);
    let shadows = runs.iter().any(|(_, _, lists)| (0..CASCADES).any(|c| !lists[LIST_SHADOW as usize + c].is_empty()));
    let pools = drawn.iter().take(MAX_BLOCK_WORLDS).filter_map(|blocks| blocks.id).collect();
    Some(ViewBlocks { runs, water, pools, shadows })
}

/// Each world's quads list `list` draws: six vertices a quad (drawn
/// smooth, a vertex every two pairs of words), the world's number in the
/// view and one in `level`, and the run's first quad in `output`, as the
/// shaders' vertex numbers leave a draw's first out.
fn draw_runs(ctx: &PassContext, view: &ViewBlocks, push: &Push, list: u32) {
    for (number, smooth, lists) in &view.runs {
        for &[first, count] in &lists[list as usize] {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: number + 1, output: first, ..*push });
            let vertices = if *smooth { count / 2 } else { count * 6 };
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, vertices, 1, 0, 0) };
        }
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
    for (number, smooth, runs) in &view.water {
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
/// the sun's atlas, over what the other shadow passes drew.
pub(crate) fn record_shadows(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    view: &ViewBlocks,
    atlas: ImageId,
) {
    if !view.shadows {
        return;
    }
    let reads: Vec<_> = view.pools.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    let view = view.clone();
    fg.add_pass("Shadows (blocks)", &[(atlas, ImageAccess::DepthAttachment)], &reads, move |ctx| {
        let size = CASCADE_RESOLUTION * 2;
        let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
        bind(ctx, pipelines.block_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
        for cascade in 0..CASCADES as u32 {
            let offset = vk::Offset2D {
                x: (cascade % 2 * CASCADE_RESOLUTION) as i32,
                y: (cascade / 2 * CASCADE_RESOLUTION) as i32,
            };
            set_viewport(ctx, offset, vk::Extent2D { width: CASCADE_RESOLUTION, height: CASCADE_RESOLUTION });
            draw_runs(ctx, &view, &push, LIST_SHADOW + cascade);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_draws_the_chunks_it_sees_and_only_their_faces_turned_towards_it() {
        // Two chunks along x, 32 m each, the second's quads following on from the first's.
        let chunk = |x: f32, first: u32| ChunkDraw {
            key: [0, (x / 32.0) as i32, 0, 0],
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            facings: std::array::from_fn(|f| [first + 10 * f as u32, 10]),
            water: [first + 60, 0],
            cone: None,
        };
        let chunks = [chunk(0.0, 0), chunk(32.0, 60)];
        // From above the first, beyond its +x side: no -y faces of either, and none of the second's +x.
        let picked = pick(&chunks, &[], (Facing::Camera(Vec3::new(16.0, 40.0, 16.0)), None), None);
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
        assert_eq!(pick(&whole, &[], (from, None), None), vec![[0, 80]]);
        // Looking along +x from x = 40: only the second.
        let ahead = pick(&whole, &[[1.0, 0.0, 0.0, -40.0]], (Facing::Camera(Vec3::new(40.0, 16.0, 16.0)), None), None);
        assert_eq!(ahead, vec![[40, 40]]);
        // Only the chunks the camera can see into, where it is known which.
        let seen = HashSet::from([[1, 0, 0]]);
        assert_eq!(pick(&whole, &[], (from, None), Some(&seen)), vec![[40, 40]]);
        // Further away, coarser: 2 m blocks, half a block allowed at 1000 m and two blocks at 4000.
        let at = |x: f32| Detail { camera: Vec3::new(x, 16.0, 16.0), allowed: 0.001, block_size: 2.0 };
        let level = |x: f32| pick(&whole[..1], &[], (Facing::Camera(at(x).camera), Some(at(x))), None);
        assert_eq!([level(-500.0), level(-2000.0), level(-9000.0)], [vec![[0, 40]], vec![[40, 15]], vec![[55, 5]]]);
        // A smooth chunk, its triangles all facing within a tenth of up: drawn from above or beside it, not at all
        // from below, nor into the shadows of light coming up from under it.
        let up = [smooth(0.0, 0, [0.0, 1.0, 0.0, 0.1])];
        let drawn = |facing: Facing| !pick(&up, &[], (facing, None), None).is_empty();
        assert!(drawn(Facing::Camera(Vec3::new(16.0, 60.0, 16.0))));
        assert!(!drawn(Facing::Camera(Vec3::new(16.0, -60.0, 16.0))));
        assert!(drawn(Facing::Camera(Vec3::new(60.0, 10.0, 16.0))), "beside it");
        assert!(drawn(Facing::Light(Vec3::new(0.2, -1.0, 0.0))));
        assert!(!drawn(Facing::Light(Vec3::new(0.2, 1.0, 0.0))));
        // One facing every way, drawn from anywhere.
        for from in [Vec3::new(16.0, -60.0, 16.0), Vec3::new(-60.0, 10.0, 16.0)] {
            assert_eq!(pick(&whole[..1], &[], (Facing::Camera(from), None), None), vec![[0, 40]]);
        }
        assert_eq!(pick(&whole[..1], &[], (Facing::Light(Vec3::new(0.2, 1.0, 0.0)), None), None), vec![[0, 40]]);
        // The sun's cascades: of each chunk, the faces turned towards sunlight going down, to -x and to +z.
        let lit = pick(&chunks, &[], (Facing::Light(Vec3::new(-0.3, -0.8, 0.5)), None), None);
        let has = |first: u32| lit.iter().any(|&[f, c]| f <= first && first < f + c);
        assert!([0, 20, 50, 60, 80, 110].map(has).iter().all(|&h| h), "+x, +y and -z faces: {lit:?}");
        assert!(![10, 30, 40, 70, 90, 100].map(has).iter().any(|&h| h), "none turned away: {lit:?}");
    }

    #[test]
    fn a_view_draws_the_water_of_every_chunk_it_sees_whichever_way_it_faces() {
        // Three chunks along x, the first and last with water after their ground's quads.
        let chunk = |x: f32, first: u32, water: u32| ChunkDraw {
            key: [0, (x / 32.0) as i32, 0, 0],
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 32.0, 32.0, 32.0)],
            facings: std::array::from_fn(|f| [first + 10 * f as u32, 10]),
            water: [first + 60, water],
            cone: None,
        };
        let chunks = [chunk(0.0, 0, 5), chunk(32.0, 65, 0), chunk(64.0, 125, 7)];
        assert_eq!(pick_water(&chunks, &[], None), vec![[60, 5], [185, 7]]);
        // Only those in view, and those the camera can see into.
        assert_eq!(pick_water(&chunks, &[[1.0, 0.0, 0.0, -40.0]], None), vec![[185, 7]]);
        let seen = HashSet::from([[0, 0, 0]]);
        assert_eq!(pick_water(&chunks, &[], Some(&seen)), vec![[60, 5]]);
    }
}
