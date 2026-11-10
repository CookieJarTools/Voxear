//! Drawing terrain (see `voxear_terrain`).
//!
//! The renderer keeps each terrain's heights on the GPU, every tile in a
//! slot of one buffer, and copies a tile in again only when its pointer
//! changes, once a frame for every view (**Terrain uploads**). Each view
//! picks the patches to draw on the CPU, a quadtree walk of the terrain's
//! levels of detail (`voxear_terrain::lod`), for its camera and for each of
//! the sun's cascades, and draws them with a grid made in the vertex shader
//! (`shaders/terrain.slang`): no vertex buffer, each vertex reading the
//! heights where it falls and sliding onto the next level's grid as it
//! nears that level's distance. The ground is shaded as any mesh is, by the
//! forward passes' fragment shaders, with the terrain's material repeated
//! every `texture_size` metres, or with its surfaces: each sample's four
//! largest shares, kept beside its heights (with which samples are holes',
//! a bit each), blended between the samples round each pixel and by the
//! surfaces' height pictures.
//!
//! Terrain is drawn before the depth pyramid, with what was visible last
//! frame, so it hides what is behind hills from the late cull. For the
//! GI's rays and the sea floor, each tile also has a coarse mesh of its
//! own, an instance only rays see (`INSTANCE_RAYS_ONLY`), which the raster
//! passes leave out.

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use voxear_core::math::{Mat3, Mat4, Vec3, Vec4};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Buffer, Frames, Gpu, MemoryLocation, Retired, vk};
use voxear_terrain::{
    HeightTile, Heightmap, TILE_QUADS,
    dig::{CHUNK_SAMPLES, DugChunk, Excavation},
    heights::{HOLE_WORDS, TILE_LEN},
    lod::{LodRanges, LodTree, PATCH_QUADS, Patch},
    grass::GrassTile,
    surfaces::{MAX_SURFACES, WeightTile},
};

use crate::{
    MaterialHandle, MeshHandle,
    assets::Assets,
    gpu::{GpuInstance, INSTANCE_RAYS_ONLY, LIST_COUNT, LIST_EARLY, LIST_SHADOW, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId, PassContext},
    mesh::{MeshData, NO_WIND_MASKS, Vertex},
    passes::{attachment, begin_rendering, bind, set_viewport},
    pipelines::Pipelines,
    scene::{ExtractedFrame, Owner},
    shadows::{CASCADE_RESOLUTION, CASCADES},
};

/// How many terrains a view draws at once.
pub const MAX_TERRAINS: usize = 64;

/// How many patches a view draws, every list together.
pub const MAX_TERRAIN_PATCHES: usize = 32_768;

/// How many levels of detail a terrain's GPU record holds the distances of.
pub const MAX_TERRAIN_LEVELS: usize = 12;

/// How many samples apart the vertices of a tile's mesh for rays are.
const PROXY_STEP: u32 = 8;

/// How many tiles' meshes for rays are built in one frame, so loading or
/// shaping a large terrain spreads over frames.
const PROXIES_A_FRAME: usize = 8;

/// How many frames a tile must stay as it is before its mesh for rays is
/// made again: a brush changes its tiles every frame, and the GI need not
/// follow each dab.
const PROXY_SETTLE: u64 = 20;

/// How a terrain looks: its material, its pictures repeated every
/// `texture_size` metres; or its surfaces, in the order its shares number
/// them, where it has any.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainLook {
    pub material: MaterialHandle,
    pub texture_size: f32,
    pub surfaces: Arc<[SurfaceLook]>,
    /// Each kind of its grass, in the order its densities number them; none
    /// for a kind with no mesh to draw.
    pub grass: crate::grass::Looks,
}

impl Component for TerrainLook {}

impl TerrainLook {
    pub const DEFAULT_TEXTURE_SIZE: f32 = 4.0;

    /// A terrain drawn with `material` alone.
    pub fn new(material: MaterialHandle, texture_size: f32) -> Self {
        Self { material, texture_size, surfaces: Arc::from([]), grass: Arc::from([]) }
    }

    /// What the GI's rays see it as: its first surface, or its material.
    fn seen(&self) -> (MaterialHandle, f32) {
        self.surfaces.first().map_or((self.material, self.texture_size), |surface| (surface.material, surface.size))
    }
}

/// How one of a terrain's surfaces looks: its material, its pictures
/// repeated every `size` metres, how sharply it meets the others (0 to 1,
/// by their height pictures), whether it is drawn from the side too on
/// steep ground, how far Track Makers press into it (a share of their
/// depth, see `tracks`), and how many seconds their tracks take to fill in
/// again (0 for never); what liquid lies on it, and whether it sparkles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceLook {
    pub material: MaterialHandle,
    pub size: f32,
    pub sharpness: f32,
    pub triplanar: bool,
    pub tracks: f32,
    pub fills_in: f32,
    /// What lies on it or runs over it, and whether it sparkles in the sun.
    pub liquid: voxear_terrain::surfaces::Liquid,
    pub glitter: bool,
}

/// A terrain as `shaders/terrain.slang` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTerrain {
    pub corner: [f32; 3],
    pub spacing: f32,
    /// Its tiles' heights, `TILE_LEN` floats each, row after row of tiles.
    pub heights: u64,
    pub tiles: [u32; 2],
    pub material: u32,
    /// An instance of its own the fragment shaders read where an
    /// instance's are asked for: one of its meshes for rays.
    pub anchor: u32,
    /// One over its pictures' size in metres.
    pub uv_scale: f32,
    pub levels: u32,
    /// Each level's distances its vertices slide onto the next level's
    /// grid over, `[start, end]`.
    pub morph: [[f32; 2]; MAX_TERRAIN_LEVELS],
    /// Its tiles' shares, two words a sample, as its heights are laid out.
    pub weights: u64,
    /// Its tiles' holes, `HOLE_WORDS` words a tile, a bit a sample.
    pub holes: u64,
    /// Its surfaces, `surface_count` of them; none for its material alone.
    pub surfaces: u64,
    pub surface_count: u32,
    /// `TERRAIN_HOLES` when any of its tiles has holes.
    pub flags: u32,
    /// How thickly each kind of its grass grows, `grass_words` words a
    /// sample, as its heights are laid out; 0 for none.
    pub grass: u64,
    pub grass_words: u32,
    /// For a terrain without end, how far its window's tiles are turned in
    /// their slots along x, and `ring_z` along z: each tile keeps the slot
    /// it came into as its window moves. 0 for a terrain of a set size.
    pub ring_x: u32,
    /// Its dug chunks' vertices (see `dug`), drawn with the patches, and
    /// which of its columns of chunks are dug, a bit each, with how many a
    /// row has; 0 for a terrain not dug.
    pub dug: u64,
    pub dug_columns: u64,
    pub dug_columns_x: u32,
    pub ring_z: u32,
}

/// A terrain's flag for having holes, so the shaders look for them.
const TERRAIN_HOLES: u32 = 1;
/// A terrain's flag for a surface tracks press into, so the shaders read
/// the tracks.
const TERRAIN_TRACKS: u32 = 2;

/// A surface as `shaders/terrain.slang` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTerrainSurface {
    pub material: u32,
    /// One over its pictures' size in metres.
    pub uv_scale: f32,
    pub sharpness: f32,
    /// `SURFACE_TRIPLANAR` to be drawn from the side too on steep ground,
    /// its liquid's flag, and `SURFACE_GLITTER`.
    pub flags: u32,
    pub tracks: f32,
    pub fills_in: f32,
}

const SURFACE_TRIPLANAR: u32 = 1;
const SURFACE_WET: u32 = 2;
const SURFACE_PUDDLES: u32 = 4;
const SURFACE_LAVA: u32 = 8;
const SURFACE_GLITTER: u32 = 16;

/// A surface's flags for the shaders: drawn from the side, its liquid, and
/// whether it glitters.
pub(crate) fn surface_flags(look: &SurfaceLook) -> u32 {
    use voxear_terrain::surfaces::Liquid;
    let liquid = match look.liquid {
        Liquid::None => 0,
        Liquid::Wet => SURFACE_WET,
        Liquid::Puddles => SURFACE_PUDDLES,
        Liquid::Lava => SURFACE_LAVA,
    };
    u32::from(look.triplanar) * SURFACE_TRIPLANAR | liquid | u32::from(look.glitter) * SURFACE_GLITTER
}

/// A patch as the vertex shader reads it: its corner in the terrain's
/// space, how wide, and its level and terrain, `level | terrain << 8`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTerrainPatch {
    pub origin: [f32; 2],
    pub size: f32,
    pub packed: u32,
}

/// Where each draw list's patches start, then how many it has.
const LISTS_SIZE: usize = 2 * LIST_COUNT as usize * 4;
const TERRAINS_AT: usize = LISTS_SIZE;
/// Each terrain's surfaces, `MAX_SURFACES` places each.
const SURFACES_AT: usize = TERRAINS_AT + MAX_TERRAINS * size_of::<GpuTerrain>();
const PATCHES_AT: usize = SURFACES_AT + MAX_TERRAINS * MAX_SURFACES * size_of::<GpuTerrainSurface>();

/// How large a view's buffer of terrains and patches is.
pub const FRAME_SIZE: u64 = (PATCHES_AT + MAX_TERRAIN_PATCHES * size_of::<GpuTerrainPatch>()) as u64;

/// One terrain's ground on the GPU: one buffer holding every tile's heights,
/// then every tile's holes, then, where it has surfaces, every tile's
/// shares, then, where it has grass, every tile's densities. Each is kept by
/// its tile's slot (see `slot_of`).
struct Resident {
    tiles: [u32; 2],
    spacing: f32,
    ground: Buffer,
    /// Whether the buffer has room for shares, and for how many words of
    /// grass a sample.
    weighted: bool,
    grass_words: usize,
    /// The densities each slot holds now, by pointer.
    uploaded_grass: Vec<Option<Arc<GrassTile>>>,
    /// The tile each slot holds now, by pointer, heights and holes.
    uploaded: Vec<Option<Arc<HeightTile>>>,
    /// The shares each slot holds now, by pointer.
    uploaded_weights: Vec<Option<Arc<WeightTile>>>,
    tree: Arc<LodTree>,
    /// Each slot's mesh for rays, in its tile's own space, and the tile and
    /// dug chunks it was made from.
    proxies: Vec<Option<(Arc<HeightTile>, Vec<Arc<DugChunk>>, MeshHandle)>>,
    /// Each tile's dug chunks as last seen, to know when they change.
    dug_seen: Vec<Vec<Arc<DugChunk>>>,
    /// The frame each tile last changed.
    changed: Vec<u64>,
    seen: u64,
    /// Its dug chunks' triangles, once it is dug.
    dug: Option<crate::dug::DugPool>,
    /// Where a terrain without end's window started when its tree was made.
    origin: Option<[i32; 2]>,
}

/// A terrain to draw this frame, as every view draws it.
#[derive(Clone)]
pub(crate) struct Drawn {
    pub entity: Entity,
    pub corner: Vec3,
    pub tiles: [u32; 2],
    pub spacing: f32,
    pub heights: u64,
    pub holes: u64,
    /// Its shares, where it has surfaces and shares for them.
    pub weights: Option<u64>,
    /// Its grass's densities and how many words a sample, where it has grass.
    pub grass: Option<(u64, u32)>,
    pub has_holes: bool,
    /// Its heights, holes and shares in this frame's graph, for every view
    /// to wait on.
    pub heights_id: Option<BufferId>,
    pub tree: Arc<LodTree>,
    pub ranges: LodRanges,
    pub look: TerrainLook,
    pub cast_shadows: bool,
    /// For a terrain without end, how far its tiles are turned in their
    /// slots, along x and z (see `GpuTerrain::ring_x`).
    pub ring: [u32; 2],
    /// Its tiles' meshes for rays, those made so far, each in its tile's own
    /// space, with where its tile is from the corner.
    pub proxies: Vec<(MeshHandle, Vec3)>,
    /// Where its dug chunks' vertices are, each chunk to draw (see
    /// `dug::pick`), and where its dug columns' bits are and how many a row
    /// has, where it is dug; the pool in this frame's graph.
    pub dug: Option<(u64, Arc<[crate::dug::DugDraw]>, (u64, u32))>,
    pub dug_id: Option<BufferId>,
    /// The patch grid's indices, which every terrain draws with.
    pub grid: vk::Buffer,
    key: u64,
}

/// A copy into a terrain's buffer: from where in the staging buffer, into
/// which terrain's, where and how much.
#[derive(Clone)]
struct Copy {
    from: u64,
    key: u64,
    to: u64,
    size: u64,
    /// Into its dug chunks' pool, not its ground.
    dug: bool,
}

/// Where in a terrain's buffer of `count` tiles its heights, holes, shares
/// (when `weighted`) and grass (`grass_words` words a sample) start, and
/// how large it is.
fn regions(count: usize, weighted: bool, grass_words: usize) -> [u64; 5] {
    let holes = (count * TILE_LEN * 4) as u64;
    let weights = holes + (count * HOLE_WORDS * 4) as u64;
    let grass = weights + if weighted { (count * TILE_LEN * 8) as u64 } else { 0 };
    [0, holes, weights, grass, grass + (count * TILE_LEN * 4 * grass_words) as u64]
}

/// How far a terrain without end's window starting at tile `origin` turns
/// its tiles in their slots, along x and z.
fn ring_of(map: &Heightmap, origin: Option<[i32; 2]>) -> [u32; 2] {
    origin
        .map_or([0, 0], |[x, z]| [x.rem_euclid(map.tiles_x() as i32) as u32, z.rem_euclid(map.tiles_z() as i32) as u32])
}

/// Every terrain's heights on the GPU, and what to copy in this frame.
#[derive(Default)]
pub(crate) struct Terrains {
    resident: HashMap<u64, Resident>,
    /// What this frame copies.
    copies: Vec<Copy>,
    staging: Option<Buffer>,
    drawn: Vec<Drawn>,
    /// The patch grid's indices, made with the first terrain.
    grid: Option<Buffer>,
}

impl Terrains {
    /// Brings every terrain in `world` up to date on the GPU, staging the
    /// tiles that changed, and lists what to draw.
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        assets: &mut Assets,
        frames: &mut Frames,
        world: &World,
        frame_index: u64,
    ) -> Result<()> {
        self.drawn.clear();
        self.copies.clear();
        // Staged and never recorded, as when a frame stopped short: sent again, from what each slot holds.
        if let Some(staging) = self.staging.take() {
            frames.retire(Retired::Buffer(staging));
            for resident in self.resident.values_mut() {
                resident.uploaded.iter_mut().for_each(|held| *held = None);
                resident.uploaded_weights.iter_mut().for_each(|held| *held = None);
                resident.uploaded_grass.iter_mut().for_each(|held| *held = None);
            }
        }
        let features = world.resource::<crate::RenderFeatures>().copied().unwrap_or_default();
        let found = if features.terrain { voxear_terrain::terrains(world) } else { Vec::new() };
        let any_hidden = world.query_ref::<&voxear_scene::Hidden>().into_iter().next().is_some();
        let mut staged: Vec<u8> = Vec::new();
        let mut proxies_left = PROXIES_A_FRAME;
        if !found.is_empty() && self.grid.is_none() {
            let indices = grid_indices();
            let bytes: &[u8] = bytemuck::cast_slice(&indices);
            let usage = vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
            let grid = gpu.create_buffer("terrain grid", bytes.len() as u64, usage, MemoryLocation::GpuOnly)?;
            gpu.upload_buffer(&grid, 0, bytes)?;
            self.grid = Some(grid);
        }
        let grid = self.grid.as_ref().map_or(vk::Buffer::null(), |grid| grid.handle);
        for (entity, terrain, corner) in found {
            if any_hidden && crate::scene::hidden(world, entity) {
                continue;
            }
            let map = &terrain.heights;
            let look = world
                .get::<TerrainLook>(entity)
                .cloned()
                .unwrap_or_else(|| TerrainLook::new(assets.default_material, TerrainLook::DEFAULT_TEXTURE_SIZE));
            let key = map.id();
            let weights = terrain.weights.as_ref().filter(|weights| weights.fits(map));
            let grass = terrain.grass.as_ref().filter(|grass| grass.fits(map));
            let grass_words = grass.map_or(0, |grass| grass.words());
            let fits = self.resident.get(&key).is_some_and(|resident| {
                resident.tiles == [map.tiles_x(), map.tiles_z()]
                    && resident.spacing == map.spacing()
                    && resident.weighted == weights.is_some()
                    && resident.grass_words == grass_words
            });
            if !fits {
                if let Some(old) = self.resident.remove(&key) {
                    retire(old, assets, frames, frame_index);
                }
                self.resident.insert(key, Resident::new(gpu, map, weights.is_some(), grass_words)?);
            }
            let resident = self.resident.get_mut(&key).expect("resident");
            resident.seen = frame_index;
            let mut changed = false;
            let [_, holes_at, weights_at, grass_at, _] = regions(map.tiles().len(), weights.is_some(), grass_words);
            let mut stage = |bytes: &[u8], to: u64| {
                let from = staged.len() as u64;
                staged.extend_from_slice(bytes);
                self.copies.push(Copy { from, key, to, size: bytes.len() as u64, dug: false });
            };
            let no_holes = [0u32; HOLE_WORDS];
            let dug = terrain.dug.as_ref().filter(|dug| !dug.is_empty());
            // Seen for the first time, all its dug ground is made at once, so it is drawn whole from the first frame.
            if let Some(dug) = dug
                && resident.uploaded.iter().any(Option::is_none)
            {
                dug.mesh_all(map);
            }
            let origin = terrain.origin;
            for (index, tile) in map.tiles().iter().enumerate() {
                let slot = voxear_terrain::endless::slot_of(map, origin, index);
                if resident.uploaded[slot].as_ref().is_some_and(|held| Arc::ptr_eq(held, tile)) {
                    continue;
                }
                // A tile holed for ground dug in it is drawn as it was until that ground's triangles are made.
                let (tx, tz) = (index as u32 % map.tiles_x(), index as u32 / map.tiles_x());
                if resident.uploaded[slot].is_some() && dug.is_some_and(|dug| !dug.tile_meshed(tx, tz)) {
                    continue;
                }
                stage(bytemuck::cast_slice(tile.samples()), (slot * TILE_LEN * 4) as u64);
                let holes = tile.holes().unwrap_or(&no_holes);
                stage(bytemuck::cast_slice(holes), holes_at + (slot * HOLE_WORDS * 4) as u64);
                resident.uploaded[slot] = Some(tile.clone());
                resident.changed[slot] = frame_index;
                changed = true;
            }
            for (index, tile) in weights.map_or(&[][..], |weights| weights.tiles()).iter().enumerate() {
                let slot = voxear_terrain::endless::slot_of(map, origin, index);
                if resident.uploaded_weights[slot].as_ref().is_some_and(|held| Arc::ptr_eq(held, tile)) {
                    continue;
                }
                stage(bytemuck::cast_slice(tile.samples()), weights_at + (slot * TILE_LEN * 8) as u64);
                resident.uploaded_weights[slot] = Some(tile.clone());
            }
            for (index, tile) in grass.map_or(&[][..], |grass| grass.tiles()).iter().enumerate() {
                let slot = voxear_terrain::endless::slot_of(map, origin, index);
                if resident.uploaded_grass[slot].as_ref().is_some_and(|held| Arc::ptr_eq(held, tile)) {
                    continue;
                }
                stage(bytemuck::cast_slice(tile.samples()), grass_at + (slot * TILE_LEN * 4 * grass_words) as u64);
                resident.uploaded_grass[slot] = Some(tile.clone());
            }
            // A window moved with no tile changed is drawn from tiles in other places: its tree is made again too.
            if changed || resident.origin != origin {
                resident.tree = Arc::new(LodTree::new(map));
                resident.origin = origin;
            }
            // Its dug chunks' triangles, those that changed made again.
            match dug {
                Some(dug) => {
                    if resident.dug.is_none() {
                        resident.dug = Some(crate::dug::DugPool::new(gpu)?);
                    }
                    let pool = resident.dug.as_mut().expect("made above");
                    let copies = &mut self.copies;
                    let mut stage = |bytes: &[u8], to: u64| {
                        let from = staged.len() as u64;
                        staged.extend_from_slice(bytes);
                        copies.push(Copy { from, key, to, size: bytes.len() as u64, dug: true });
                    };
                    pool.update(gpu, frames, map, dug, frame_index, &mut stage)?;
                }
                None => {
                    if let Some(pool) = resident.dug.take() {
                        pool.retire(frames);
                    }
                }
            }
            let same = |a: &[Arc<DugChunk>], b: &[Arc<DugChunk>]| {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b))
            };
            let width = TILE_QUADS as f32 * map.spacing();
            let mut placed_proxies = Vec::new();
            for index in 0..map.tiles().len() {
                let tile = &map.tiles()[index];
                let slot = voxear_terrain::endless::slot_of(map, origin, index);
                let (tx, tz) = (index as u32 % map.tiles_x(), index as u32 / map.tiles_x());
                let at = Vec3::new(tx as f32 * width, 0.0, tz as f32 * width);
                // A terrain without end's tile not made yet has nothing for rays to meet.
                if voxear_terrain::endless::is_unmade(tile) {
                    continue;
                }
                // Its dug chunks, which rays see in place of the ground they were dug out of; a change to them is a
                // change to the tile.
                let chunks: Vec<Arc<DugChunk>> = dug
                    .map(|dug| dug.tile_chunks(tx, tz).map(|(_, chunk)| chunk.clone()).collect())
                    .unwrap_or_default();
                if !same(&resident.dug_seen[slot], &chunks) {
                    resident.dug_seen[slot] = chunks.clone();
                    resident.changed[slot] = frame_index;
                }
                let current = resident.proxies[slot]
                    .as_ref()
                    .is_some_and(|(made, was, _)| Arc::ptr_eq(made, tile) && same(was, &chunks));
                // A tile that has a mesh waits until it settles; one that has none gets one at once. Either waits for
                // its dug ground's triangles.
                let settling = resident.proxies[slot].is_some() && frame_index < resident.changed[slot] + PROXY_SETTLE;
                let unmade = chunks.iter().any(|chunk| chunk.mesh().is_none());
                if !(current || settling || unmade || proxies_left == 0) {
                    proxies_left -= 1;
                    // Its pictures repeated from where the tile is from the terrain's Obj, as the ground's are.
                    let from = at + terrain.offset();
                    let mesh = proxy(map, dug.map(|dug| &**dug), (tx, tz), from, 1.0 / look.seen().1.max(0.01));
                    let handle = assets.add_mesh_with_lods(gpu, &mesh, &crate::lod::full_lod(&mesh))?;
                    if let Some((_, _, old)) = resident.proxies[slot].replace((tile.clone(), chunks, handle)) {
                        assets.free_mesh(old, frame_index);
                    }
                }
                // Shown only once made from this tile, so a slot that came to hold another tile shows nothing till
                // then.
                if let Some((made, _, mesh)) = &resident.proxies[slot]
                    && Arc::ptr_eq(made, tile)
                {
                    placed_proxies.push((*mesh, at));
                }
            }
            let tree = resident.tree.clone();
            let ranges = LodRanges::new(terrain.detail_distance, tree.leaf_size(), tree.levels());
            self.drawn.push(Drawn {
                entity,
                corner,
                tiles: resident.tiles,
                spacing: resident.spacing,
                heights: resident.ground.address,
                holes: resident.ground.address + holes_at,
                weights: weights.map(|_| resident.ground.address + weights_at),
                grass: grass.map(|_| (resident.ground.address + grass_at, grass_words as u32)),
                has_holes: map.has_holes(),
                heights_id: None,
                tree,
                ranges,
                look,
                cast_shadows: terrain.cast_shadows,
                ring: ring_of(map, origin),
                proxies: placed_proxies,
                dug: resident
                    .dug
                    .as_mut()
                    .map(|pool| (pool.buffer.address, pool.draws(), pool.columns().unwrap_or((0, 0)))),
                dug_id: None,
                grid,
                key,
            });
        }
        // What no longer is, or is no longer drawn, goes once the frames that drew it are done.
        let gone: Vec<u64> =
            self.resident.iter().filter(|(_, resident)| resident.seen != frame_index).map(|(key, _)| *key).collect();
        for key in gone {
            let resident = self.resident.remove(&key).expect("resident");
            retire(resident, assets, frames, frame_index);
        }
        if !staged.is_empty() {
            let bytes: &[u8] = bytemuck::cast_slice(&staged);
            let usage = vk::BufferUsageFlags::TRANSFER_SRC;
            let staging = gpu.create_buffer("terrain uploads", bytes.len() as u64, usage, MemoryLocation::CpuToGpu)?;
            staging.write(0, bytes);
            self.staging = Some(staging);
        }
        Ok(())
    }

    /// Copies this frame's staged tiles in, and gives what every view draws,
    /// each terrain's heights as the graph knows them.
    pub fn record(&mut self, fg: &mut FrameGraph<'_>, frames: &mut Frames) -> Arc<[Drawn]> {
        let mut ids = HashMap::new();
        let mut dug_ids = HashMap::new();
        for drawn in &mut self.drawn {
            let resident = &self.resident[&drawn.key];
            let id = *ids.entry(drawn.key).or_insert_with(|| fg.import_buffer(&resident.ground));
            drawn.heights_id = Some(id);
            if let Some(pool) = &resident.dug {
                drawn.dug_id = Some(*dug_ids.entry(drawn.key).or_insert_with(|| fg.import_buffer(&pool.buffer)));
            }
        }
        if let Some(staging) = self.staging.take() {
            let copies: Vec<(vk::Buffer, vk::BufferCopy)> = self
                .copies
                .iter()
                .map(|copy| {
                    let region = vk::BufferCopy { src_offset: copy.from, dst_offset: copy.to, size: copy.size };
                    let resident = &self.resident[&copy.key];
                    let target = match (&resident.dug, copy.dug) {
                        (Some(pool), true) => pool.buffer.handle,
                        _ => resident.ground.handle,
                    };
                    (target, region)
                })
                .collect();
            let written: Vec<(BufferId, BufferAccess)> =
                ids.values().chain(dug_ids.values()).map(|&id| (id, BufferAccess::TransferWrite)).collect();
            let source = staging.handle;
            fg.add_pass("Terrain uploads", &[], &written, move |ctx| {
                for (target, region) in &copies {
                    unsafe { ctx.gpu.device.cmd_copy_buffer(ctx.cmd, source, *target, std::slice::from_ref(region)) };
                }
            });
            frames.retire(Retired::Buffer(staging));
        }
        self.drawn.clone().into()
    }

    pub fn destroy(&mut self, gpu: &Gpu) {
        for (_, resident) in self.resident.drain() {
            gpu.destroy_buffer(resident.ground);
            if let Some(pool) = resident.dug {
                pool.destroy(gpu);
            }
        }
        if let Some(grid) = self.grid.take() {
            gpu.destroy_buffer(grid);
        }
        if let Some(staging) = self.staging.take() {
            gpu.destroy_buffer(staging);
        }
    }
}

impl Resident {
    fn new(gpu: &Gpu, map: &Heightmap, weighted: bool, grass_words: usize) -> Result<Self> {
        let count = map.tiles().len();
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let [.., size] = regions(count, weighted, grass_words);
        let ground = gpu.create_buffer("terrain ground", size, usage, MemoryLocation::GpuOnly)?;
        Ok(Self {
            tiles: [map.tiles_x(), map.tiles_z()],
            spacing: map.spacing(),
            ground,
            weighted,
            grass_words,
            uploaded_grass: vec![None; count],
            uploaded: vec![None; count],
            uploaded_weights: vec![None; count],
            tree: Arc::new(LodTree::new(map)),
            proxies: vec![None; count],
            dug_seen: vec![Vec::new(); count],
            changed: vec![0; count],
            seen: 0,
            dug: None,
            origin: None,
        })
    }
}

fn retire(resident: Resident, assets: &mut Assets, frames: &mut Frames, frame_index: u64) {
    frames.retire(Retired::Buffer(resident.ground));
    if let Some(pool) = resident.dug {
        pool.retire(frames);
    }
    for (_, _, mesh) in resident.proxies.into_iter().flatten() {
        assets.free_mesh(mesh, frame_index);
    }
}

/// Tile `(tx, tz)`'s mesh for rays: a vertex every `PROXY_STEP` samples,
/// in the tile's own space, its pictures repeated as the ground's are from
/// `from`, where the tile is from the terrain's Obj; where it is dug, its dug
/// chunks at their coarsest instead.
fn proxy(map: &Heightmap, dug: Option<&Excavation>, (tx, tz): (u32, u32), from: Vec3, uv_scale: f32) -> MeshData {
    let across = TILE_QUADS / PROXY_STEP + 1;
    let spacing = map.spacing();
    let corner = Vec3::new((tx * TILE_QUADS) as f32 * spacing, 0.0, (tz * TILE_QUADS) as f32 * spacing);
    let mut mesh = MeshData::default();
    for z in 0..across {
        for x in 0..across {
            let (sx, sz) = (tx * TILE_QUADS + x * PROXY_STEP, tz * TILE_QUADS + z * PROXY_STEP);
            let (lx, lz) = (sx as f32 * spacing, sz as f32 * spacing);
            let normal = map.normal_at(lx, lz);
            let tangent = Vec3::new(normal.y, -normal.x, 0.0).normalize_or(Vec3::X);
            let (px, pz) = (lx - corner.x, lz - corner.z);
            mesh.vertices.push(Vertex {
                position: [px, map.sample(i64::from(sx), i64::from(sz)), pz],
                normal: normal.into(),
                uv: [(px + from.x) * uv_scale, -(pz + from.z) * uv_scale],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind: NO_WIND_MASKS,
            });
        }
    }
    for z in 0..across - 1 {
        for x in 0..across - 1 {
            // None over a dug column: its chunks are the ground there.
            let (sx, sz) = (tx * TILE_QUADS + x * PROXY_STEP, tz * TILE_QUADS + z * PROXY_STEP);
            if dug.is_some_and(|dug| dug.is_dug((sx / CHUNK_SAMPLES) as i32, (sz / CHUNK_SAMPLES) as i32)) {
                continue;
            }
            let a = z * across + x;
            let (b, c, d) = (a + 1, a + across, a + across + 1);
            // Split from the low corner to the high, facing up, as the ground is.
            mesh.indices.extend([a, c, d, a, d, b]);
        }
    }
    for (_, chunk) in dug.into_iter().flat_map(|dug| dug.tile_chunks(tx, tz)) {
        let Some(made) = chunk.mesh() else { continue };
        let coarsest = made.coarser.last().unwrap_or(made);
        let first = mesh.vertices.len() as u32;
        for (&position, &normal) in coarsest.positions.iter().zip(&coarsest.normals) {
            let tangent = Vec3::new(normal[1], -normal[0], 0.0).normalize_or(Vec3::X);
            let position = [position[0] - corner.x, position[1], position[2] - corner.z];
            mesh.vertices.push(Vertex {
                position,
                normal,
                uv: [(position[0] + from.x) * uv_scale, -(position[2] + from.z) * uv_scale],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind: NO_WIND_MASKS,
            });
        }
        mesh.indices.extend(coarsest.indices.iter().map(|&i| first + i));
    }
    mesh
}

/// What a view draws of the terrains: where their records and patches
/// are, how many patches each list has, and their heights to wait on.
#[derive(Clone)]
pub(crate) struct ViewTerrains {
    pub lists: [(u32, u32); LIST_COUNT as usize],
    pub heights: Vec<BufferId>,
    /// The view's tracks, which the ground's pixels read.
    pub tracks: Option<BufferId>,
    /// The patch grid's indices.
    pub grid: vk::Buffer,
    /// Whether any terrain casts the sun's shadows.
    pub shadows: bool,
    /// Each dug terrain's number in the view, and for each list, the first
    /// vertex and count of each run of its chunks' vertices it draws.
    pub dug: Vec<(u32, Vec<Arc<[[u32; 2]]>>)>,
}

impl ViewTerrains {
    pub fn count(&self, list: u32) -> u32 {
        self.lists[list as usize].1
    }
}

/// Adds each terrain's meshes for rays to the frame's instances, ahead of
/// the plants, so the GI's rays and the sea floor see them.
pub(crate) fn add_proxies(drawn: &[Drawn], extracted: &mut ExtractedFrame) {
    for terrain in drawn {
        for (tile, (mesh, at)) in terrain.proxies.iter().enumerate() {
            let model = Mat4::from_translation(terrain.corner + *at);
            let normal = Mat3::from_mat4(model).inverse().transpose();
            extracted.instances.push(GpuInstance {
                model: crate::gpu::cols(model),
                prev_model: crate::gpu::cols(model),
                normal: [normal.x_axis, normal.y_axis, normal.z_axis].map(|c| Vec4::from((c, 0.0)).to_array()),
                mesh: mesh.0,
                material: terrain.look.seen().0.0,
                flags: INSTANCE_RAYS_ONLY,
                ..GpuInstance::default()
            });
            extracted.entities.push(Owner { entity: terrain.entity, plant: None, part: Some(tile as u32) });
        }
    }
}

/// Picks each terrain's patches for this view and writes them, with the
/// terrains' records, into `buffer`; `None` when there is nothing to draw.
/// `entities` gives each instance's owner, from which each terrain finds
/// its anchor among its meshes for rays.
pub(crate) fn gather(
    drawn: &[Drawn],
    frame: &crate::gpu::GpuFrame,
    entities: &[Owner],
    has_sun: bool,
    buffer: &Buffer,
) -> Option<ViewTerrains> {
    if drawn.is_empty() {
        // An empty record first, so models blending into the ground find none.
        buffer.write(TERRAINS_AT as u64, &[GpuTerrain::default()]);
        return None;
    }
    let mut anchors: HashMap<Entity, u32> = HashMap::new();
    for (index, owner) in entities.iter().enumerate().take(crate::MAX_INSTANCES as usize) {
        if owner.plant.is_none() && owner.part.is_some() {
            anchors.entry(owner.entity).or_insert(index as u32);
        }
    }
    let camera = Vec3::from(frame.camera_position);
    let mut records = Vec::with_capacity(drawn.len().min(MAX_TERRAINS));
    let mut surfaces = vec![GpuTerrainSurface::default(); drawn.len().min(MAX_TERRAINS) * MAX_SURFACES];
    let mut lists: Vec<Vec<GpuTerrainPatch>> = vec![Vec::new(); LIST_COUNT as usize];
    let mut patches = Vec::new();
    let mut dug = Vec::new();
    for (number, terrain) in drawn.iter().take(MAX_TERRAINS).enumerate() {
        let mut morph = [[f32::MAX; 2]; MAX_TERRAIN_LEVELS];
        for (level, range) in terrain.ranges.morph.iter().take(MAX_TERRAIN_LEVELS).enumerate() {
            morph[level] = *range;
        }
        // Surfaces only with their shares: until those are made, the material alone.
        let looks = if terrain.weights.is_some() { &terrain.look.surfaces[..] } else { &[] };
        let looks = &looks[..looks.len().min(MAX_SURFACES)];
        for (place, look) in surfaces[number * MAX_SURFACES..].iter_mut().zip(looks) {
            *place = GpuTerrainSurface {
                material: look.material.0,
                uv_scale: 1.0 / look.size.max(0.01),
                sharpness: look.sharpness.clamp(0.0, 1.0),
                flags: surface_flags(look),
                tracks: look.tracks.clamp(0.0, 1.0),
                fills_in: look.fills_in.max(0.0),
            };
        }
        let surfaces_at = (SURFACES_AT + number * MAX_SURFACES * size_of::<GpuTerrainSurface>()) as u64;
        records.push(GpuTerrain {
            corner: terrain.corner.into(),
            spacing: terrain.spacing,
            heights: terrain.heights,
            tiles: terrain.tiles,
            material: terrain.look.material.0,
            anchor: anchors.get(&terrain.entity).copied().unwrap_or(0),
            uv_scale: 1.0 / terrain.look.texture_size.max(0.01),
            levels: terrain.tree.levels() as u32,
            morph,
            weights: terrain.weights.unwrap_or(0),
            holes: terrain.holes,
            surfaces: buffer.address + surfaces_at,
            surface_count: looks.len() as u32,
            flags: if terrain.has_holes { TERRAIN_HOLES } else { 0 }
                | if looks.iter().any(|look| look.tracks > 0.0) { TERRAIN_TRACKS } else { 0 },
            grass: terrain.grass.map_or(0, |(address, _)| address),
            grass_words: terrain.grass.map_or(0, |(_, words)| words),
            ring_x: terrain.ring[0],
            dug: terrain.dug.as_ref().map_or(0, |(address, ..)| *address),
            dug_columns: terrain.dug.as_ref().map_or(0, |(.., (columns, _))| *columns),
            dug_columns_x: terrain.dug.as_ref().map_or(0, |(.., (_, across))| *across),
            ring_z: terrain.ring[1],
        });
        let local = camera - terrain.corner;
        let pack = |patch: &Patch| GpuTerrainPatch {
            origin: patch.origin,
            size: patch.size,
            packed: patch.level | (number as u32) << 8,
        };
        let planes = voxear_terrain::lod::planes_from(&frame.planes, terrain.corner);
        patches.clear();
        voxear_terrain::lod::select(&terrain.tree, local, &terrain.ranges, &planes, &mut patches);
        lists[LIST_EARLY as usize].extend(patches.iter().map(pack));
        // Its dug chunks each list sees, coarser where the ground round them is drawn coarser.
        let ends: Vec<f32> = (0..voxear_terrain::dig::MESH_LEVELS - 1).map(|level| terrain.ranges.end(level)).collect();
        let mut dug_lists: Vec<Arc<[[u32; 2]]>> = vec![Arc::from([]); LIST_COUNT as usize];
        if let Some((_, draws, _)) = &terrain.dug {
            dug_lists[LIST_EARLY as usize] = crate::dug::pick(draws, local, &planes, &ends).into();
        }
        if has_sun && terrain.cast_shadows {
            for cascade in 0..CASCADES {
                let planes = voxear_terrain::lod::planes_from(&frame.cascade_planes[cascade], terrain.corner);
                patches.clear();
                voxear_terrain::lod::select(&terrain.tree, local, &terrain.ranges, &planes, &mut patches);
                lists[LIST_SHADOW as usize + cascade].extend(patches.iter().map(pack));
                if let Some((_, draws, _)) = &terrain.dug {
                    dug_lists[LIST_SHADOW as usize + cascade] = crate::dug::pick(draws, local, &planes, &ends).into();
                }
            }
        }
        if terrain.dug.is_some() {
            dug.push((number as u32, dug_lists));
        }
    }
    let mut placed = [(0u32, 0u32); LIST_COUNT as usize];
    let mut all: Vec<GpuTerrainPatch> = Vec::new();
    for (list, patches) in lists.iter().enumerate() {
        let room = MAX_TERRAIN_PATCHES - all.len();
        let taken = patches.len().min(room);
        placed[list] = (all.len() as u32, taken as u32);
        all.extend_from_slice(&patches[..taken]);
    }
    let mut header = [0u32; 2 * LIST_COUNT as usize];
    for (list, (first, count)) in placed.iter().enumerate() {
        header[list] = *first;
        header[LIST_COUNT as usize + list] = *count;
    }
    buffer.write(0, &header);
    // An empty record after the last ends the list for models blending into the ground.
    if records.len() < MAX_TERRAINS {
        records.push(GpuTerrain::default());
    }
    buffer.write(TERRAINS_AT as u64, &records);
    buffer.write(SURFACES_AT as u64, &surfaces);
    buffer.write(PATCHES_AT as u64, &all);
    let shadows = (0..CASCADES).any(|cascade| placed[LIST_SHADOW as usize + cascade].1 > 0);
    // The dug pools are waited on with the heights.
    let heights = drawn.iter().flat_map(|terrain| terrain.heights_id.into_iter().chain(terrain.dug_id)).collect();
    let grid = drawn.first().map_or(vk::Buffer::null(), |terrain| terrain.grid);
    Some(ViewTerrains { lists: placed, heights, tracks: None, shadows, grid, dug })
}

/// Where a view's terrain buffer holds its lists, records and patches.
pub(crate) fn addresses(buffer: &Buffer) -> [u64; 3] {
    [buffer.address, buffer.address + TERRAINS_AT as u64, buffer.address + PATCHES_AT as u64]
}

/// How many indices a patch's grid is drawn with: two triangles a quad.
const PATCH_INDICES: u32 = PATCH_QUADS * PATCH_QUADS * 6;

/// The indices of a patch's grid, the same for every patch: its vertices
/// row after row, a vertex more than quads each way, each quad split from
/// its low corner to its high one, as the ground is, facing up.
fn grid_indices() -> Vec<u32> {
    let across = PATCH_QUADS + 1;
    let mut indices = Vec::with_capacity(PATCH_INDICES as usize);
    for z in 0..PATCH_QUADS {
        for x in 0..PATCH_QUADS {
            let a = z * across + x;
            let (b, c, d) = (a + 1, a + across, a + across + 1);
            indices.extend([a, c, d, a, d, b]);
        }
    }
    indices
}

fn draw_list(ctx: &PassContext, terrains: &ViewTerrains, push: &Push, list: u32) {
    let count = terrains.count(list);
    if count == 0 {
        return;
    }
    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: 0, ..*push });
    unsafe {
        ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, terrains.grid, 0, vk::IndexType::UINT32);
        ctx.gpu.device.cmd_draw_indexed(ctx.cmd, PATCH_INDICES, count, 0, 0, 0);
    }
}

/// Each dug terrain's chunks, with the same pipeline as its patches: the
/// terrain's number in the view and one in `level`, which tells its vertex
/// shader to read the terrain's dug vertices, and the chunk's first vertex
/// in `output`, as the shaders' vertex numbers leave a draw's first out.
fn draw_dug(ctx: &PassContext, terrains: &ViewTerrains, push: &Push, list: u32) {
    for (number, lists) in &terrains.dug {
        for &[first, count] in lists[list as usize].iter() {
            ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, level: number + 1, output: first, ..*push });
            unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, count, 1, 0, 0) };
        }
    }
}

/// **Shadows (terrain)**: the patches each cascade sees, into its corner of
/// the sun's atlas, over what the other shadow passes drew.
pub(crate) fn record_shadows(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    terrains: &ViewTerrains,
    atlas: ImageId,
) {
    if !terrains.shadows {
        return;
    }
    let reads: Vec<_> = terrains.heights.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    let terrains = terrains.clone();
    fg.add_pass("Shadows (terrain)", &[(atlas, ImageAccess::DepthAttachment)], &reads, move |ctx| {
        let size = CASCADE_RESOLUTION * 2;
        let depth = attachment(ctx.view(atlas), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, vk::Extent2D { width: size, height: size }, &[], Some(&depth));
        bind(ctx, pipelines.terrain_shadow, vk::PipelineBindPoint::GRAPHICS, &push);
        for cascade in 0..CASCADES as u32 {
            let offset = vk::Offset2D {
                x: (cascade % 2 * CASCADE_RESOLUTION) as i32,
                y: (cascade / 2 * CASCADE_RESOLUTION) as i32,
            };
            set_viewport(ctx, offset, vk::Extent2D { width: CASCADE_RESOLUTION, height: CASCADE_RESOLUTION });
            draw_list(ctx, &terrains, &push, LIST_SHADOW + cascade);
            draw_dug(ctx, &terrains, &push, LIST_SHADOW + cascade);
        }
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Depth (terrain)**: the patches in view as depth alone, with each
/// pixel's normal and roughness, over what the depth pass drew.
pub(crate) fn record_depth(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    terrains: &ViewTerrains,
    (depth, surface): (ImageId, ImageId),
) {
    if terrains.count(LIST_EARLY) == 0 && terrains.dug.is_empty() {
        return;
    }
    let mut reads: Vec<_> = terrains.heights.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    reads.extend(terrains.tracks.map(|id| (id, BufferAccess::ShaderRead)));
    let images = [(depth, ImageAccess::DepthAttachment), (surface, ImageAccess::ColorAttachment)];
    let terrains = terrains.clone();
    fg.add_pass("Depth (terrain)", &images, &reads, move |ctx| {
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        let rough = [attachment(ctx.view(surface), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, ctx.extent(depth), &rough, Some(&target));
        bind(ctx, pipelines.terrain_depth, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_list(ctx, &terrains, &push, LIST_EARLY);
        draw_dug(ctx, &terrains, &push, LIST_EARLY);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Opaque (terrain)**: the patches in view shaded, over what the opaque
/// passes drew; `pipeline` writes depth or tests against the depth passes'.
pub(crate) fn record_opaque(
    fg: &mut FrameGraph<'_>,
    push: Push,
    pipeline: vk::Pipeline,
    terrains: &ViewTerrains,
    images: Vec<(ImageId, ImageAccess)>,
    (hdr, velocity, depth): (ImageId, ImageId, ImageId),
    buffers: &[(BufferId, BufferAccess)],
) {
    if terrains.count(LIST_EARLY) == 0 && terrains.dug.is_empty() {
        return;
    }
    let mut reads: Vec<_> = buffers.to_vec();
    reads.extend(terrains.heights.iter().map(|&id| (id, BufferAccess::ShaderRead)));
    reads.extend(terrains.tracks.map(|id| (id, BufferAccess::ShaderRead)));
    let terrains = terrains.clone();
    fg.add_pass("Opaque (terrain)", &images, &reads, move |ctx| {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let color = [attachment(ctx.view(hdr), layout, None), attachment(ctx.view(velocity), layout, None)];
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, ctx.extent(hdr), &color, Some(&target));
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_list(ctx, &terrains, &push, LIST_EARLY);
        draw_dug(ctx, &terrains, &push, LIST_EARLY);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tiles_mesh_for_rays_lies_on_its_ground_and_faces_up() {
        let map = Heightmap::from_fn(2, 1, 0.5, |x, z| (x * 0.1).sin() * 3.0 + z * 0.01);
        // The second tile, 128 m along, in its own space.
        let mesh = proxy(&map, None, (1, 0), Vec3::new(128.0, 0.0, 0.0), 0.25);
        assert_eq!(mesh.vertices.len(), 33 * 33);
        for vertex in &mesh.vertices {
            let [x, y, z] = vertex.position;
            assert!((map.height_at(x + 128.0, z) - y).abs() < 1e-4);
        }
        assert_eq!(mesh.vertices[0].position[0], 0.0);
        assert_eq!(mesh.vertices[0].uv[0], 32.0, "its pictures repeated as the ground's, from where it is");
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[triangle[i] as usize].position));
            assert!((b - a).cross(c - a).y > 0.0, "every triangle faces up");
        }
        assert_eq!(voxear_terrain::TILE_SAMPLES, 257);
    }

    #[test]
    fn a_dug_tiles_mesh_for_rays_has_its_dug_ground_in_place_of_the_heights_there() {
        use voxear_terrain::dig::{Carve, Shape};
        let map = Heightmap::flat(1, 1, 0.5, 0.0);
        let mut dug = Excavation::default();
        // A pit 3 m round in the column from 56 to 64 m each way.
        dug.carve(&map, &Shape::Ball { at: [60.0, 0.0, 60.0], radius: 3.0 }, Carve::Dig, None);
        dug.mesh_all(&map);
        let mesh = proxy(&map, Some(&dug), (0, 0), Vec3::ZERO, 0.25);
        let plain = proxy(&map, None, (0, 0), Vec3::ZERO, 0.25);
        let points = |mesh: &MeshData| -> Vec<Vec3> {
            mesh.indices.iter().map(|&i| Vec3::from(mesh.vertices[i as usize].position)).collect()
        };
        // The heights' quads over the dug column left out: four of them, 4 m a side, two triangles each.
        let over = |p: &Vec3| (56.0..=64.0).contains(&p.x) && (56.0..=64.0).contains(&p.z);
        let heights_over = |mesh: &MeshData| {
            let big = |t: &[Vec3]| (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5 > 7.9;
            points(mesh).chunks(3).filter(|t| t.iter().all(over) && big(t)).count()
        };
        assert_eq!(heights_over(&plain), 8);
        assert_eq!(heights_over(&mesh), 0, "the ground over the pit left out");
        // And the pit's floor there, 3 m down.
        let deepest = points(&mesh).iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!((deepest + 3.0).abs() < 0.3, "the pit's floor: {deepest}");
        for t in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[t[i] as usize].position));
            let n = (b - a).cross(c - a);
            let middle = (a + b + c) / 3.0;
            // Every triangle faces the air: out of the pit's middle below the ground, up beside it.
            if middle.y < -0.5 {
                assert!(n.dot(Vec3::new(60.0, 0.0, 60.0) - middle) > -1e-4, "{middle} faces into the ground");
            }
        }
    }

    #[test]
    fn every_quad_of_the_patch_grid_is_two_triangles_split_from_its_low_corner() {
        let indices = grid_indices();
        assert_eq!(indices.len(), PATCH_INDICES as usize);
        let across = PATCH_QUADS + 1;
        let at = |i: u32| Vec3::new((i % across) as f32, 0.0, (i / across) as f32);
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = [at(triangle[0]), at(triangle[1]), at(triangle[2])];
            assert!((b - a).cross(c - a).y > 0.0, "facing up");
        }
        // The first quad's split runs from (0, 0) to (1, 1).
        assert_eq!(&indices[..6], &[0, across, across + 1, 0, across + 1, 1]);
    }

    #[test]
    fn a_views_terrain_buffer_holds_its_lists_records_and_patches() {
        const { assert!(TERRAINS_AT.is_multiple_of(8), "records hold addresses, so start on eight bytes") };
        const { assert!(size_of::<GpuTerrain>() == 216) };
        const { assert!(SURFACES_AT.is_multiple_of(16) && PATCHES_AT.is_multiple_of(16)) };
        const { assert!(FRAME_SIZE < 1 << 20) };
    }
}
