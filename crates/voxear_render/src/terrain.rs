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
    heights::{HOLE_WORDS, TILE_LEN},
    lod::{LodRanges, LodTree, PATCH_QUADS, Patch},
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
}

impl Component for TerrainLook {}

impl TerrainLook {
    pub const DEFAULT_TEXTURE_SIZE: f32 = 4.0;

    /// A terrain drawn with `material` alone.
    pub fn new(material: MaterialHandle, texture_size: f32) -> Self {
        Self { material, texture_size, surfaces: Arc::from([]) }
    }

    /// What the GI's rays see it as: its first surface, or its material.
    fn seen(&self) -> (MaterialHandle, f32) {
        self.surfaces.first().map_or((self.material, self.texture_size), |surface| (surface.material, surface.size))
    }
}

/// How one of a terrain's surfaces looks: its material, its pictures
/// repeated every `size` metres, how sharply it meets the others (0 to 1,
/// by their height pictures), and whether it is drawn from the side too on
/// steep ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceLook {
    pub material: MaterialHandle,
    pub size: f32,
    pub sharpness: f32,
    pub triplanar: bool,
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
}

/// A terrain's flag for having holes, so the shaders look for them.
const TERRAIN_HOLES: u32 = 1;

/// A surface as `shaders/terrain.slang` reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuTerrainSurface {
    pub material: u32,
    /// One over its pictures' size in metres.
    pub uv_scale: f32,
    pub sharpness: f32,
    /// `SURFACE_TRIPLANAR` to be drawn from the side too on steep ground.
    pub flags: u32,
}

const SURFACE_TRIPLANAR: u32 = 1;

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
/// shares.
struct Resident {
    tiles: [u32; 2],
    spacing: f32,
    ground: Buffer,
    /// Whether the buffer has room for shares.
    weighted: bool,
    /// The tile each slot holds now, by pointer, heights and holes.
    uploaded: Vec<Option<Arc<HeightTile>>>,
    /// The shares each slot holds now, by pointer.
    uploaded_weights: Vec<Option<Arc<WeightTile>>>,
    tree: Arc<LodTree>,
    /// Each tile's mesh for rays, and the tile it was made from.
    proxies: Vec<Option<(Arc<HeightTile>, MeshHandle)>>,
    /// The frame each tile last changed.
    changed: Vec<u64>,
    seen: u64,
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
    pub has_holes: bool,
    /// Its heights, holes and shares in this frame's graph, for every view
    /// to wait on.
    pub heights_id: Option<BufferId>,
    pub tree: Arc<LodTree>,
    pub ranges: LodRanges,
    pub look: TerrainLook,
    pub cast_shadows: bool,
    /// Its tiles' meshes for rays, those made so far.
    pub proxies: Vec<MeshHandle>,
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
}

/// Where in a terrain's buffer of `count` tiles its heights, holes and
/// shares start, and how large it is with or without the shares.
fn regions(count: usize) -> [u64; 4] {
    let holes = (count * TILE_LEN * 4) as u64;
    let weights = holes + (count * HOLE_WORDS * 4) as u64;
    [0, holes, weights, weights + (count * TILE_LEN * 8) as u64]
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
            let fits = self.resident.get(&key).is_some_and(|resident| {
                resident.tiles == [map.tiles_x(), map.tiles_z()]
                    && resident.spacing == map.spacing()
                    && resident.weighted == weights.is_some()
            });
            if !fits {
                if let Some(old) = self.resident.remove(&key) {
                    retire(old, assets, frames, frame_index);
                }
                self.resident.insert(key, Resident::new(gpu, map, weights.is_some())?);
            }
            let resident = self.resident.get_mut(&key).expect("resident");
            resident.seen = frame_index;
            let mut changed = false;
            let [_, holes_at, weights_at, _] = regions(map.tiles().len());
            let mut stage = |bytes: &[u8], to: u64| {
                let from = staged.len() as u64;
                staged.extend_from_slice(bytes);
                self.copies.push(Copy { from, key, to, size: bytes.len() as u64 });
            };
            let no_holes = [0u32; HOLE_WORDS];
            for (index, tile) in map.tiles().iter().enumerate() {
                if resident.uploaded[index].as_ref().is_some_and(|held| Arc::ptr_eq(held, tile)) {
                    continue;
                }
                stage(bytemuck::cast_slice(tile.samples()), (index * TILE_LEN * 4) as u64);
                let holes = tile.holes().unwrap_or(&no_holes);
                stage(bytemuck::cast_slice(holes), holes_at + (index * HOLE_WORDS * 4) as u64);
                resident.uploaded[index] = Some(tile.clone());
                resident.changed[index] = frame_index;
                changed = true;
            }
            for (index, tile) in weights.map_or(&[][..], |weights| weights.tiles()).iter().enumerate() {
                if resident.uploaded_weights[index].as_ref().is_some_and(|held| Arc::ptr_eq(held, tile)) {
                    continue;
                }
                stage(bytemuck::cast_slice(tile.samples()), weights_at + (index * TILE_LEN * 8) as u64);
                resident.uploaded_weights[index] = Some(tile.clone());
            }
            if changed {
                resident.tree = Arc::new(LodTree::new(map));
            }
            for index in 0..map.tiles().len() {
                let tile = &map.tiles()[index];
                let current = resident.proxies[index].as_ref().is_some_and(|(made, _)| Arc::ptr_eq(made, tile));
                // A tile that has a mesh waits until it settles; one that has none gets one at once.
                let settling =
                    resident.proxies[index].is_some() && frame_index < resident.changed[index] + PROXY_SETTLE;
                if current || settling || proxies_left == 0 {
                    continue;
                }
                proxies_left -= 1;
                let (tx, tz) = (index as u32 % map.tiles_x(), index as u32 / map.tiles_x());
                let mesh = proxy(map, tx, tz, 1.0 / look.seen().1.max(0.01));
                let handle = assets.add_mesh_with_lods(gpu, &mesh, &crate::lod::full_lod(&mesh))?;
                if let Some((_, old)) = resident.proxies[index].replace((tile.clone(), handle)) {
                    assets.free_mesh(old, frame_index);
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
                has_holes: map.has_holes(),
                heights_id: None,
                tree,
                ranges,
                look,
                cast_shadows: terrain.cast_shadows,
                proxies: resident.proxies.iter().flatten().map(|(_, mesh)| *mesh).collect(),
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
        for drawn in &mut self.drawn {
            let resident = &self.resident[&drawn.key];
            let id = *ids.entry(drawn.key).or_insert_with(|| fg.import_buffer(&resident.ground));
            drawn.heights_id = Some(id);
        }
        if let Some(staging) = self.staging.take() {
            let copies: Vec<(vk::Buffer, vk::BufferCopy)> = self
                .copies
                .iter()
                .map(|copy| {
                    let region = vk::BufferCopy { src_offset: copy.from, dst_offset: copy.to, size: copy.size };
                    (self.resident[&copy.key].ground.handle, region)
                })
                .collect();
            let written: Vec<(BufferId, BufferAccess)> =
                ids.values().map(|&id| (id, BufferAccess::TransferWrite)).collect();
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
    fn new(gpu: &Gpu, map: &Heightmap, weighted: bool) -> Result<Self> {
        let count = map.tiles().len();
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let [_, _, without, with] = regions(count);
        let size = if weighted { with } else { without };
        let ground = gpu.create_buffer("terrain ground", size, usage, MemoryLocation::GpuOnly)?;
        Ok(Self {
            tiles: [map.tiles_x(), map.tiles_z()],
            spacing: map.spacing(),
            ground,
            weighted,
            uploaded: vec![None; count],
            uploaded_weights: vec![None; count],
            tree: Arc::new(LodTree::new(map)),
            proxies: vec![None; count],
            changed: vec![0; count],
            seen: 0,
        })
    }
}

fn retire(resident: Resident, assets: &mut Assets, frames: &mut Frames, frame_index: u64) {
    frames.retire(Retired::Buffer(resident.ground));
    for (_, mesh) in resident.proxies.into_iter().flatten() {
        assets.free_mesh(mesh, frame_index);
    }
}

/// Tile `(tx, tz)`'s mesh for rays: a vertex every `PROXY_STEP` samples,
/// in the terrain's space, its pictures repeated as the ground's are.
fn proxy(map: &Heightmap, tx: u32, tz: u32, uv_scale: f32) -> MeshData {
    let across = TILE_QUADS / PROXY_STEP + 1;
    let spacing = map.spacing();
    let mut mesh = MeshData::default();
    for z in 0..across {
        for x in 0..across {
            let (sx, sz) = (tx * TILE_QUADS + x * PROXY_STEP, tz * TILE_QUADS + z * PROXY_STEP);
            let (lx, lz) = (sx as f32 * spacing, sz as f32 * spacing);
            let normal = map.normal_at(lx, lz);
            let tangent = Vec3::new(normal.y, -normal.x, 0.0).normalize_or(Vec3::X);
            mesh.vertices.push(Vertex {
                position: [lx, map.sample(i64::from(sx), i64::from(sz)), lz],
                normal: normal.into(),
                uv: [lx * uv_scale, -lz * uv_scale],
                tangent: [tangent.x, tangent.y, tangent.z, 1.0],
                wind: NO_WIND_MASKS,
            });
        }
    }
    for z in 0..across - 1 {
        for x in 0..across - 1 {
            let a = z * across + x;
            let (b, c, d) = (a + 1, a + across, a + across + 1);
            // Split from the low corner to the high, facing up, as the ground is.
            mesh.indices.extend([a, c, d, a, d, b]);
        }
    }
    mesh
}

/// What a view draws of the terrains: where their records and patches
/// are, how many patches each list has, and their heights to wait on.
#[derive(Clone)]
pub(crate) struct ViewTerrains {
    pub lists: [(u32, u32); LIST_COUNT as usize],
    pub heights: Vec<BufferId>,
    /// The patch grid's indices.
    pub grid: vk::Buffer,
    /// Whether any terrain casts the sun's shadows.
    pub shadows: bool,
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
        let model = Mat4::from_translation(terrain.corner);
        let normal = Mat3::from_mat4(model).inverse().transpose();
        for (tile, mesh) in terrain.proxies.iter().enumerate() {
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
                flags: if look.triplanar { SURFACE_TRIPLANAR } else { 0 },
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
            flags: if terrain.has_holes { TERRAIN_HOLES } else { 0 },
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
        if has_sun && terrain.cast_shadows {
            for cascade in 0..CASCADES {
                let planes = voxear_terrain::lod::planes_from(&frame.cascade_planes[cascade], terrain.corner);
                patches.clear();
                voxear_terrain::lod::select(&terrain.tree, local, &terrain.ranges, &planes, &mut patches);
                lists[LIST_SHADOW as usize + cascade].extend(patches.iter().map(pack));
            }
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
    buffer.write(TERRAINS_AT as u64, &records);
    buffer.write(SURFACES_AT as u64, &surfaces);
    buffer.write(PATCHES_AT as u64, &all);
    let shadows = (0..CASCADES).any(|cascade| placed[LIST_SHADOW as usize + cascade].1 > 0);
    let heights = drawn.iter().filter_map(|terrain| terrain.heights_id).collect();
    let grid = drawn.first().map_or(vk::Buffer::null(), |terrain| terrain.grid);
    Some(ViewTerrains { lists: placed, heights, shadows, grid })
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
    ctx.bindless.push(ctx.gpu, ctx.cmd, &Push { list, ..*push });
    unsafe {
        ctx.gpu.device.cmd_bind_index_buffer(ctx.cmd, terrains.grid, 0, vk::IndexType::UINT32);
        ctx.gpu.device.cmd_draw_indexed(ctx.cmd, PATCH_INDICES, count, 0, 0, 0);
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
    if terrains.count(LIST_EARLY) == 0 {
        return;
    }
    let reads: Vec<_> = terrains.heights.iter().map(|&id| (id, BufferAccess::ShaderRead)).collect();
    let images = [(depth, ImageAccess::DepthAttachment), (surface, ImageAccess::ColorAttachment)];
    let terrains = terrains.clone();
    fg.add_pass("Depth (terrain)", &images, &reads, move |ctx| {
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        let rough = [attachment(ctx.view(surface), vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, None)];
        begin_rendering(ctx, ctx.extent(depth), &rough, Some(&target));
        bind(ctx, pipelines.terrain_depth, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_list(ctx, &terrains, &push, LIST_EARLY);
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
    if terrains.count(LIST_EARLY) == 0 {
        return;
    }
    let mut reads: Vec<_> = buffers.to_vec();
    reads.extend(terrains.heights.iter().map(|&id| (id, BufferAccess::ShaderRead)));
    let terrains = terrains.clone();
    fg.add_pass("Opaque (terrain)", &images, &reads, move |ctx| {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let color = [attachment(ctx.view(hdr), layout, None), attachment(ctx.view(velocity), layout, None)];
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, ctx.extent(hdr), &color, Some(&target));
        bind(ctx, pipeline, vk::PipelineBindPoint::GRAPHICS, &push);
        draw_list(ctx, &terrains, &push, LIST_EARLY);
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tiles_mesh_for_rays_lies_on_its_ground_and_faces_up() {
        let map = Heightmap::from_fn(2, 1, 0.5, |x, z| (x * 0.1).sin() * 3.0 + z * 0.01);
        let mesh = proxy(&map, 1, 0, 0.25);
        assert_eq!(mesh.vertices.len(), 33 * 33);
        for vertex in &mesh.vertices {
            let [x, y, z] = vertex.position;
            assert!((map.height_at(x, z) - y).abs() < 1e-4);
        }
        assert!(mesh.vertices[0].position[0] >= 128.0, "the second tile starts 128 m along");
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(mesh.vertices[triangle[i] as usize].position));
            assert!((b - a).cross(c - a).y > 0.0, "every triangle faces up");
        }
        assert_eq!(voxear_terrain::TILE_SAMPLES, 257);
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
        const { assert!(size_of::<GpuTerrain>() == 176) };
        const { assert!(SURFACES_AT.is_multiple_of(16) && PATCHES_AT.is_multiple_of(16)) };
        const { assert!(FRAME_SIZE < 1 << 20) };
    }
}
