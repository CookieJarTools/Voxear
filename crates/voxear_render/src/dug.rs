//! A terrain's dug ground on the GPU: each dug chunk's triangles (see
//! `voxear_terrain::dig`), made on the terrain's background threads when
//! the chunk changes, the chunk as it was drawn until they are done, and
//! kept in a pool of vertices of the terrain's own, a range of it a chunk,
//! which the terrain's shaders draw from as they draw its patches
//! (`terrainVertex` in `shaders/terrain.slang`).

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use voxear_core::math::Vec3;
use voxear_rhi::{Buffer, FRAMES_IN_FLIGHT, Frames, Gpu, MemoryLocation, Retired, vk};
use voxear_terrain::{
    Heightmap,
    dig::{ChunkKey, ChunkMesh, DugChunk, Excavation, MESH_LEVELS},
};

use crate::alloc::RangeAllocator;

/// A vertex of dug ground as the shaders read it (`DugVertex` in
/// `shaders/common.slang`): where it is, in metres from the terrain's
/// corner, then the way the ground faces there in two 12-bit numbers
/// (folded onto an octahedron) and the surface digging laid there or
/// beside it, with its top bit for whether it lies on it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DugVertex {
    pub local: [f32; 3],
    pub packed: u32,
}

/// How many vertices a pool starts with room for; it doubles when full.
const FIRST_CAPACITY: u64 = 1 << 18;

/// A way the ground faces and a surface, as one word.
fn pack(normal: [f32; 3], surface: u8) -> u32 {
    let n = Vec3::from(normal);
    let n = n / (n.x.abs() + n.y.abs() + n.z.abs()).max(1e-6);
    let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
    let (x, z) = if n.y < 0.0 { ((1.0 - n.z.abs()) * sign(n.x), (1.0 - n.x.abs()) * sign(n.z)) } else { (n.x, n.z) };
    let quantise = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 4095.0).round() as u32;
    quantise(x) | quantise(z) << 12 | u32::from(surface) << 24
}

/// A chunk's triangles as vertices, three a triangle.
fn vertices(mesh: &ChunkMesh) -> Vec<DugVertex> {
    mesh.indices
        .iter()
        .map(|&i| {
            let i = i as usize;
            let surface = (mesh.surfaces[i] & 0x7F) | u8::from(mesh.laid[i]) << 7;
            DugVertex { local: mesh.positions[i], packed: pack(mesh.normals[i], surface) }
        })
        .collect()
}

/// One chunk in the pool: the chunk it was made from, the box its ground
/// lies in (in metres from the terrain's corner), and each level of
/// detail's vertices, where they start in the pool and how many.
struct Held {
    chunk: Arc<DugChunk>,
    bounds: [Vec3; 2],
    levels: Vec<(u64, Arc<[DugVertex]>)>,
}

/// One chunk as a view picks what to draw of it: the box its ground lies
/// in, and each level of detail's first vertex and count, finest first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DugDraw {
    pub bounds: [Vec3; 2],
    pub levels: [[u32; 2]; MESH_LEVELS],
}

/// What a view draws of a terrain's dug chunks seen from `camera` (in its
/// space): those in `planes` (inward, in its space), each at the finest
/// level of detail within `ends` of the camera (how far each level but
/// the last reaches), the coarsest past them; as first vertices and
/// counts, those that follow on in the pool drawn as one.
pub(crate) fn pick(draws: &[DugDraw], camera: Vec3, planes: &[[f32; 4]], ends: &[f32]) -> Vec<[u32; 2]> {
    let mut picked: Vec<[u32; 2]> = Vec::new();
    for draw in draws {
        let [low, high] = draw.bounds;
        let seen = planes.iter().all(|&[x, y, z, d]| {
            let corner = Vec3::new(
                if x >= 0.0 { high.x } else { low.x },
                if y >= 0.0 { high.y } else { low.y },
                if z >= 0.0 { high.z } else { low.z },
            );
            x * corner.x + y * corner.y + z * corner.z + d >= 0.0
        });
        if !seen {
            continue;
        }
        let away = camera.clamp(low, high).distance(camera);
        let level = ends.iter().take(MESH_LEVELS - 1).take_while(|&&end| away > end).count();
        let [first, count] = draw.levels[level];
        if count > 0 {
            picked.push([first, count]);
        }
    }
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

/// One terrain's dug chunks' vertices, and which of its columns are dug, a
/// bit each, row after row, in a range of its own, for its grass to grow
/// over the holes its voxels stand in.
pub(crate) struct DugPool {
    pub buffer: Buffer,
    space: RangeAllocator,
    capacity: u64,
    held: HashMap<ChunkKey, Held>,
    /// Ranges given up, freed once the frames that may draw them are done.
    freeing: Vec<(u64, u64, u64)>,
    columns: Vec<u32>,
    columns_x: u32,
    /// Where the columns' bits are, in vertices, and how many.
    columns_at: Option<(u64, u64)>,
    /// Each chunk to draw, kept until the pool changes.
    draws: Option<Arc<[DugDraw]>>,
    changed: bool,
}

impl DugPool {
    fn create(gpu: &Gpu, capacity: u64) -> Result<Buffer> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let size = capacity * size_of::<DugVertex>() as u64;
        gpu.create_buffer("terrain dug ground", size, usage, MemoryLocation::GpuOnly)
    }

    pub fn new(gpu: &Gpu) -> Result<Self> {
        Ok(Self {
            buffer: Self::create(gpu, FIRST_CAPACITY)?,
            space: RangeAllocator::new(FIRST_CAPACITY),
            capacity: FIRST_CAPACITY,
            held: HashMap::new(),
            freeing: Vec::new(),
            columns: Vec::new(),
            columns_x: 0,
            columns_at: None,
            draws: None,
            changed: true,
        })
    }

    /// Where the dug columns' bits are, and how many columns a row has.
    pub fn columns(&self) -> Option<(u64, u32)> {
        self.columns_at.map(|(first, _)| (self.buffer.address + first * size_of::<DugVertex>() as u64, self.columns_x))
    }

    /// Brings the pool up to `dug`: stages the triangles of each chunk that
    /// changed, by pointer, with `stage(bytes, to)` into the pool's buffer,
    /// growing it when they do not fit. A chunk whose triangles are still
    /// being made is drawn as it was until they are done.
    pub fn update(
        &mut self,
        gpu: &Gpu,
        frames: &mut Frames,
        heights: &Arc<Heightmap>,
        dug: &Arc<Excavation>,
        frame_index: u64,
        stage: &mut dyn FnMut(&[u8], u64),
    ) -> Result<()> {
        // What the frames in flight no longer draw is free again.
        let free_by = frame_index.saturating_sub(FRAMES_IN_FLIGHT as u64);
        let (done, waiting): (Vec<_>, Vec<_>) = self.freeing.drain(..).partition(|(when, ..)| *when <= free_by);
        self.freeing = waiting;
        for (_, first, count) in done {
            self.space.free(first, count);
        }
        let before = self.held.len();
        self.held.retain(|key, held| {
            let kept = dug.chunk(*key).is_some();
            if !kept {
                self.freeing.extend(held.levels.iter().map(|(first, made)| (frame_index, *first, made.len() as u64)));
            }
            kept
        });
        self.changed |= self.held.len() != before;
        for (key, chunk) in dug.chunks() {
            if self.held.get(key).is_some_and(|held| Arc::ptr_eq(&held.chunk, chunk)) {
                continue;
            }
            let Some(mesh) = Excavation::meshed(dug, heights, *key) else { continue };
            if let Some(old) = self.held.remove(key) {
                self.freeing.extend(old.levels.iter().map(|(first, made)| (frame_index, *first, made.len() as u64)));
            }
            // The finest level, then each coarser; a chunk with no ground in it has none.
            let mut levels = Vec::with_capacity(MESH_LEVELS);
            for level in std::iter::once(&*mesh).chain(&mesh.coarser) {
                let made: Arc<[DugVertex]> = vertices(level).into();
                let count = made.len() as u64;
                let first = match self.space.alloc(count) {
                    Some(first) => first,
                    None => {
                        self.grow(gpu, frames, count, stage)?;
                        self.space.alloc(count).expect("room after growing")
                    }
                };
                stage(bytemuck::cast_slice(&made), first * size_of::<DugVertex>() as u64);
                levels.push((first, made));
            }
            let points = mesh.positions.iter().map(|&p| Vec3::from(p));
            let bounds = points.fold([Vec3::MAX, Vec3::MIN], |[low, high], p| [low.min(p), high.max(p)]);
            self.held.insert(*key, Held { chunk: chunk.clone(), bounds, levels });
            self.changed = true;
        }
        // Which columns are dug, staged again when they change.
        let columns_x = heights.quads()[0] / voxear_terrain::dig::CHUNK_SAMPLES;
        let columns_z = heights.quads()[1] / voxear_terrain::dig::CHUNK_SAMPLES;
        let mut columns = vec![0u32; (columns_x * columns_z).div_ceil(32) as usize];
        for (x, z) in dug.columns() {
            let index = z as u32 * columns_x + x as u32;
            columns[(index / 32) as usize] |= 1 << (index % 32);
        }
        if columns != self.columns || self.columns_at.is_none() {
            if let Some((first, count)) = self.columns_at.take() {
                self.freeing.push((frame_index, first, count));
            }
            let count = (columns.len() as u64).div_ceil(4);
            let first = match self.space.alloc(count) {
                Some(first) => first,
                None => {
                    self.grow(gpu, frames, count, stage)?;
                    self.space.alloc(count).expect("room after growing")
                }
            };
            let mut bytes: Vec<u32> = columns.clone();
            bytes.resize(count as usize * 4, 0);
            stage(bytemuck::cast_slice(&bytes), first * size_of::<DugVertex>() as u64);
            self.columns_at = Some((first, count));
            (self.columns, self.columns_x) = (columns, columns_x);
        }
        Ok(())
    }

    /// A buffer twice as large, or more where `wanted` more vertices need
    /// it, with every chunk held staged into it again.
    fn grow(&mut self, gpu: &Gpu, frames: &mut Frames, wanted: u64, stage: &mut dyn FnMut(&[u8], u64)) -> Result<()> {
        let held: u64 = self.held.values().flat_map(|held| &held.levels).map(|(_, made)| made.len() as u64).sum();
        let mut capacity = self.capacity * 2;
        while capacity < held + wanted {
            capacity *= 2;
        }
        let old = std::mem::replace(&mut self.buffer, Self::create(gpu, capacity)?);
        frames.retire(Retired::Buffer(old));
        self.capacity = capacity;
        self.space = RangeAllocator::new(capacity);
        self.freeing.clear();
        for (first, made) in self.held.values_mut().flat_map(|held| &mut held.levels) {
            *first = self.space.alloc(made.len() as u64).expect("room in a larger pool");
            stage(bytemuck::cast_slice(made), *first * size_of::<DugVertex>() as u64);
        }
        self.changed = true;
        // The columns' bits are staged again too.
        self.columns_at = None;
        Ok(())
    }

    /// Each chunk with ground in it, for a view to pick what it draws of
    /// them (see [`pick`]): made again only when the pool changed.
    pub fn draws(&mut self) -> Arc<[DugDraw]> {
        if self.changed || self.draws.is_none() {
            let draws = self.held.values().filter(|held| !held.levels.is_empty()).map(|held| {
                let mut levels = [[0u32; 2]; MESH_LEVELS];
                for (level, (first, made)) in held.levels.iter().enumerate().take(MESH_LEVELS) {
                    levels[level] = [*first as u32, made.len() as u32];
                }
                // A level missing draws the last there is.
                for level in held.levels.len().max(1)..MESH_LEVELS {
                    levels[level] = levels[level - 1];
                }
                DugDraw { bounds: held.bounds, levels }
            });
            self.draws = Some(draws.collect());
            self.changed = false;
        }
        self.draws.clone().expect("made above")
    }

    pub fn retire(self, frames: &mut Frames) {
        frames.retire(Retired::Buffer(self.buffer));
    }

    pub fn destroy(self, gpu: &Gpu) {
        gpu.destroy_buffer(self.buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_draws_the_chunks_it_sees_coarser_further_off_and_those_side_by_side_as_one() {
        let chunk = |x: f32, first: u32| DugDraw {
            bounds: [Vec3::new(x, 0.0, 0.0), Vec3::new(x + 8.0, 8.0, 8.0)],
            levels: [[first, 30], [first + 30, 10], [first + 40, 5]],
        };
        // Three chunks along x, the second's vertices following on from the first's in the pool.
        let draws = [chunk(0.0, 0), chunk(8.0, 40), chunk(100.0, 90)];
        let ends = [20.0, 60.0];
        // From the first: both near ones at their finest, the far one at its coarsest.
        let picked = pick(&draws, Vec3::new(4.0, 4.0, 4.0), &[], &ends);
        assert_eq!(picked, vec![[0, 30], [40, 30], [130, 5]]);
        // From further along: the first coarser, its vertices ending where the second's finest begin, as one draw.
        let picked = pick(&draws, Vec3::new(30.0, 4.0, 4.0), &[], &ends);
        assert_eq!(picked, vec![[30, 40], [130, 5]]);
        // Looking along +x from x = 50: only the far one, the near ones behind.
        let picked = pick(&draws, Vec3::new(50.0, 4.0, 4.0), &[[1.0, 0.0, 0.0, -50.0]], &ends);
        assert_eq!(picked, vec![[120, 10]]);
        // Past every end, each at its coarsest: here one after the other in the pool, so drawn as one.
        let side = [chunk(0.0, 0), DugDraw { levels: [[100, 1], [101, 1], [45, 7]], ..chunk(8.0, 0) }];
        assert_eq!(pick(&side, Vec3::new(500.0, 0.0, 0.0), &[], &ends), vec![[40, 12]]);
    }

    #[test]
    fn a_normal_folds_into_a_word_and_back_close_to_itself() {
        // As the shaders unfold it.
        let unpack = |word: u32| {
            let e = [(word & 0xFFF) as f32 / 4095.0 * 2.0 - 1.0, ((word >> 12) & 0xFFF) as f32 / 4095.0 * 2.0 - 1.0];
            let mut n = Vec3::new(e[0], 1.0 - e[0].abs() - e[1].abs(), e[1]);
            if n.y < 0.0 {
                let sign = |v: f32| if v < 0.0 { -1.0 } else { 1.0 };
                (n.x, n.z) = ((1.0 - e[1].abs()) * sign(e[0]), (1.0 - e[0].abs()) * sign(e[1]));
            }
            (n.normalize(), (word >> 24) as u8)
        };
        for normal in [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.3, -0.5, -0.81], [-0.7, 0.1, 0.7]] {
            let normal = Vec3::from(normal).normalize();
            let (back, surface) = unpack(pack(normal.to_array(), 5));
            assert!(back.dot(normal) > 0.999, "{normal} came back {back}");
            assert_eq!(surface, 5);
        }
    }
}
