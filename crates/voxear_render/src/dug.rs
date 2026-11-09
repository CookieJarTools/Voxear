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
    dig::{ChunkKey, ChunkMesh, DugChunk, Excavation},
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

/// One chunk in the pool: the chunk it was made from, where its vertices
/// start and how many.
struct Held {
    chunk: Arc<DugChunk>,
    first: u64,
    count: u64,
    vertices: Arc<[DugVertex]>,
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
        self.held.retain(|key, held| {
            let kept = dug.chunk(*key).is_some();
            if !kept {
                self.freeing.push((frame_index, held.first, held.count));
            }
            kept
        });
        for (key, chunk) in dug.chunks() {
            if self.held.get(key).is_some_and(|held| Arc::ptr_eq(&held.chunk, chunk)) {
                continue;
            }
            let Some(mesh) = Excavation::meshed(dug, heights, *key) else { continue };
            let made: Arc<[DugVertex]> = vertices(&mesh).into();
            let count = made.len() as u64;
            if let Some(old) = self.held.remove(key) {
                self.freeing.push((frame_index, old.first, old.count));
            }
            let first = match self.space.alloc(count) {
                Some(first) => first,
                None => {
                    self.grow(gpu, frames, count, stage)?;
                    self.space.alloc(count).expect("room after growing")
                }
            };
            stage(bytemuck::cast_slice(&made), first * size_of::<DugVertex>() as u64);
            self.held.insert(*key, Held { chunk: chunk.clone(), first, count, vertices: made });
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
        let held: u64 = self.held.values().map(|held| held.count).sum();
        let mut capacity = self.capacity * 2;
        while capacity < held + wanted {
            capacity *= 2;
        }
        let old = std::mem::replace(&mut self.buffer, Self::create(gpu, capacity)?);
        frames.retire(Retired::Buffer(old));
        self.capacity = capacity;
        self.space = RangeAllocator::new(capacity);
        self.freeing.clear();
        for held in self.held.values_mut() {
            held.first = self.space.alloc(held.count).expect("room in a larger pool");
            stage(bytemuck::cast_slice(&held.vertices), held.first * size_of::<DugVertex>() as u64);
        }
        // The columns' bits are staged again too.
        self.columns_at = None;
        Ok(())
    }

    /// Each chunk's first vertex and count, to draw.
    pub fn draws(&self) -> Vec<[u32; 2]> {
        let mut draws: Vec<[u32; 2]> = self
            .held
            .values()
            .filter(|held| held.count > 0)
            .map(|held| [held.first as u32, held.count as u32])
            .collect();
        draws.sort_unstable();
        draws
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
