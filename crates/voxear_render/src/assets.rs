//! GPU storage for meshes, materials and textures.
//!
//! Every mesh lives in one shared vertex buffer and one shared index buffer,
//! described by a mesh table, so a single indirect draw call can draw any
//! mix of meshes. Its meshlets live in a shared meshlet table the same way,
//! with their vertices and triangles beside it. Content frees what nothing uses; the space and slots go
//! back to their allocators once no frame in flight can still read them.

use std::collections::HashMap;
use voxear_core::math::Vec3;

use anyhow::{Context, Result, ensure};
use ash::nv;
use voxear_rhi::{
    AccelerationStructure, BUILD_INPUT, Bindless, Buffer, FRAMES_IN_FLIGHT, Gpu, Image, ImageDesc, MemoryLocation,
    TextureId, Transition, Triangles, mip_barrier, vk,
};

use crate::{
    alloc::{RangeAllocator, SlotAllocator},
    bvh::MeshHierarchies,
    cooked::{CookedModel, CookedTexture},
    gpu::{GpuMaterial, GpuMesh, GpuMeshLod, GpuMeshlet, MATERIAL_FOLIAGE, MATERIAL_NORMAL_MAP},
    lod::{Lod, LodInfo, MAX_LODS, build_lods},
    mesh::{MeshData, Primitive, Vertex},
    meshlets::MeshMeshlets,
    payload::{Payload, tile_len},
};

const MAX_VERTICES: u64 = 2 * 1024 * 1024;
const MAX_INDICES: u64 = 8 * 1024 * 1024;
const MAX_MESHES: u64 = 16 * 1024;
/// Enough for every index in meshlets of 64 triangles, half the most they hold.
const MAX_MESHLETS: u64 = MAX_INDICES / (3 * 64);
/// A meshlet has about half as many vertices as triangles; this allows three
/// for every two.
const MAX_MESHLET_VERTICES: u64 = MAX_INDICES / 2;
const MAX_MESHLET_TRIANGLES: u64 = MAX_INDICES / 3;
const MAX_MATERIALS: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);

/// A PBR metal/rough material, as in glTF.
#[derive(Clone, Copy, Debug)]
pub struct MaterialDesc {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub base_color_texture: Option<TextureId>,
    /// Green is roughness, blue is metallic.
    pub metallic_roughness_texture: Option<TextureId>,
    pub normal_texture: Option<TextureId>,
    pub normal_scale: f32,
    pub emissive_texture: Option<TextureId>,
    pub alpha_cutoff: Option<f32>,
    /// Lit as leaves are: light wrapped round, through the leaf against the
    /// sun, and a rim of the sun's colour.
    pub foliage: bool,
}

impl Default for MaterialDesc {
    fn default() -> Self {
        Self {
            base_color: [1.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0; 3],
            base_color_texture: None,
            metallic_roughness_texture: None,
            normal_texture: None,
            normal_scale: 1.0,
            emissive_texture: None,
            alpha_cutoff: None,
            foliage: false,
        }
    }
}

/// RGBA8 pixels for [`Assets::add_texture`]. Colour textures are `srgb`;
/// data textures (normals, roughness) must not be.
pub struct TextureSource<'a> {
    pub name: &'a str,
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
    pub srgb: bool,
}

/// How much of the GPU asset storage is in use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AssetUsage {
    pub meshes: u32,
    pub materials: u32,
    pub textures: u32,
    pub vertices: u64,
    pub indices: u64,
    /// Freed, but waiting for frames in flight to finish with them.
    pub retiring: u32,
    /// Cooked model bytes the GPU has decompressed since startup.
    pub decompressed_on_gpu: u64,
}

/// Where a mesh's data sits in the shared buffers, each as (offset, count).
#[derive(Clone, Copy, Default)]
struct Space {
    vertices: (u64, u64),
    indices: (u64, u64),
    meshlets: (u64, u64),
    meshlet_vertices: (u64, u64),
    meshlet_triangles: (u64, u64),
}

impl Space {
    fn ranges(&self) -> [(u64, u64); 5] {
        [self.vertices, self.indices, self.meshlets, self.meshlet_vertices, self.meshlet_triangles]
    }
}

/// Where a mesh's data sits in the shared buffers, how many indices its
/// most detailed LOD, which comes first, has, and its bounding sphere.
#[derive(Clone, Copy)]
struct MeshRanges {
    space: Space,
    first_lod: u32,
    sphere: (Vec3, f32),
    /// Each LOD's triangles and error, for the editor to show.
    lods: [LodInfo; MAX_LODS],
    lod_count: u8,
}

impl MeshRanges {
    fn new(space: Space) -> Self {
        let lods = [LodInfo::default(); MAX_LODS];
        Self { space, first_lod: 0, sphere: (Vec3::ZERO, 0.0), lods, lod_count: 0 }
    }

    /// Keeps what the editor shows of a mesh's LODs.
    fn keep_lods(&mut self, lods: impl Iterator<Item = LodInfo>) {
        self.lod_count = 0;
        for (slot, lod) in self.lods.iter_mut().zip(lods) {
            *slot = lod;
            self.lod_count += 1;
        }
    }
}

enum Retiring {
    Mesh(MeshHandle),
    /// A replaced or freed mesh's bottom level, for ray queries.
    BottomLevel(AccelerationStructure),
    /// A replaced mesh's old space.
    Space(Space),
    Material(MaterialHandle),
    Texture(TextureId),
}

pub struct Assets {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub meshes: Buffer,
    pub meshlets: Buffer,
    /// Each meshlet's vertices, as indices into its mesh's.
    pub meshlet_vertices: Buffer,
    /// Each meshlet's triangles, three bytes into its vertices a word.
    pub meshlet_triangles: Buffer,
    pub materials: Buffer,
    /// Vertex, index, meshlet, meshlet vertex and meshlet triangle space.
    space: [RangeAllocator; 5],
    mesh_slots: SlotAllocator,
    material_slots: SlotAllocator,
    mesh_ranges: HashMap<MeshHandle, MeshRanges>,
    /// The most meshlets any LOD of each mesh has, by its slot: the most one
    /// draw of it can turn into.
    most_meshlets: Vec<u32>,
    /// Each mesh's box in its own space, for picking: exact for meshes
    /// uploaded from their vertices, the bounding sphere's box for cooked ones.
    mesh_bounds: HashMap<MeshHandle, (Vec3, Vec3)>,
    textures: HashMap<TextureId, Image>,
    /// Freed resources and the frame from which nothing in flight uses them.
    retiring: Vec<(u64, Retiring)>,
    /// Each mesh's bottom level, where ray tracing is on, and those replaced
    /// since the last frame, waiting to be retired.
    bottom_levels: HashMap<MeshHandle, AccelerationStructure>,
    /// Each mesh's hierarchy of triangles, where the GI's rays are traced in a
    /// compute shader.
    pub hierarchies: Option<MeshHierarchies>,
    retired_levels: Vec<AccelerationStructure>,
    white: TextureId,
    primitives: HashMap<Primitive, MeshHandle>,
    pub default_material: MaterialHandle,
    /// Payload bytes the GPU has decompressed, so far.
    pub decompressed_on_gpu: u64,
    /// The LTC tables area lights shade with, matrices then amplitudes (see
    /// `ltc`).
    pub ltc: [TextureId; 2],
}

impl Assets {
    /// `software_rays` keeps a hierarchy of each mesh's triangles, for the
    /// GI's rays to walk in a compute shader.
    pub fn new(gpu: &Gpu, bindless: &mut Bindless, software_rays: bool) -> Result<Self> {
        let device_local = MemoryLocation::GpuOnly;
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        // Ray tracing builds each mesh's bottom level straight from the shared buffers.
        let traced = if gpu.acceleration_structure.is_some() { BUILD_INPUT } else { vk::BufferUsageFlags::empty() };
        let vertices =
            gpu.create_buffer("vertices", MAX_VERTICES * size_of::<Vertex>() as u64, storage | traced, device_local)?;
        let indices = gpu.create_buffer(
            "indices",
            MAX_INDICES * 4,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST | traced,
            device_local,
        )?;
        let meshes = gpu.create_buffer("meshes", MAX_MESHES * size_of::<GpuMesh>() as u64, storage, device_local)?;
        let meshlets =
            gpu.create_buffer("meshlets", MAX_MESHLETS * size_of::<GpuMeshlet>() as u64, storage, device_local)?;
        let meshlet_vertices =
            gpu.create_buffer("meshlet vertices", MAX_MESHLET_VERTICES * 4, storage, device_local)?;
        let meshlet_triangles =
            gpu.create_buffer("meshlet triangles", MAX_MESHLET_TRIANGLES * 4, storage, device_local)?;
        let materials =
            gpu.create_buffer("materials", MAX_MATERIALS * size_of::<GpuMaterial>() as u64, storage, device_local)?;

        let mut assets = Self {
            vertices,
            indices,
            meshes,
            meshlets,
            meshlet_vertices,
            meshlet_triangles,
            materials,
            space: [MAX_VERTICES, MAX_INDICES, MAX_MESHLETS, MAX_MESHLET_VERTICES, MAX_MESHLET_TRIANGLES]
                .map(RangeAllocator::new),
            mesh_slots: SlotAllocator::new(MAX_MESHES as u32),
            mesh_bounds: HashMap::new(),
            material_slots: SlotAllocator::new(MAX_MATERIALS as u32),
            mesh_ranges: HashMap::new(),
            most_meshlets: vec![0; MAX_MESHES as usize],
            textures: HashMap::new(),
            retiring: Vec::new(),
            bottom_levels: HashMap::new(),
            hierarchies: software_rays.then(|| MeshHierarchies::new(gpu, MAX_MESHES)).transpose()?,
            retired_levels: Vec::new(),
            white: TextureId(0),
            primitives: HashMap::new(),
            default_material: MaterialHandle(0),
            decompressed_on_gpu: 0,
            ltc: [TextureId(0); 2],
        };
        let white = TextureSource { name: "white", width: 1, height: 1, rgba: &[255; 4], srgb: false };
        assets.white = assets.add_texture(gpu, bindless, &white)?;
        assets.default_material = assets.add_material(gpu, &MaterialDesc::default())?;
        let (matrix, amplitude) = crate::ltc::encode();
        let size = (crate::ltc::LTC_SIZE as u32, crate::ltc::LTC_SIZE as u32);
        let format = vk::Format::R16G16B16A16_SFLOAT;
        for (slot, (name, texels)) in [("LTC matrices", matrix), ("LTC amplitudes", amplitude)].into_iter().enumerate() {
            let bytes = bytemuck::cast_slice(&texels).to_vec();
            assets.ltc[slot] = assets.add_texture_mips(gpu, bindless, name, size, format, &[bytes])?;
        }
        Ok(assets)
    }

    /// Uploads a mesh with an automatically generated LOD chain.
    pub fn add_mesh(&mut self, gpu: &Gpu, mesh: &MeshData) -> Result<MeshHandle> {
        self.add_mesh_with_lods(gpu, mesh, &build_lods(mesh))
    }

    /// Uploads a mesh whose LOD chain was built already, such as on a
    /// loading thread.
    pub fn add_mesh_with_lods(&mut self, gpu: &Gpu, mesh: &MeshData, lods: &[Lod]) -> Result<MeshHandle> {
        let meshlets = MeshMeshlets::gather(lods.iter().map(|lod| &lod.meshlets));
        let (handle, ranges) = self.reserve_mesh(sizes(mesh, lods, &meshlets))?;
        self.write_mesh(gpu, handle, ranges, mesh, lods, &meshlets)?;
        Ok(handle)
    }

    /// Puts new data behind a mesh's handle, as when its model is imported
    /// again with other settings: whatever draws the mesh draws the new data
    /// from the next frame. The old data's space is given back once frames
    /// in flight are done with it.
    pub fn replace_mesh(
        &mut self,
        gpu: &Gpu,
        mesh: MeshHandle,
        data: &MeshData,
        lods: &[Lod],
        frame: u64,
    ) -> Result<()> {
        let old = *self.mesh_ranges.get(&mesh).context("the mesh to replace is not loaded")?;
        let meshlets = MeshMeshlets::gather(lods.iter().map(|lod| &lod.meshlets));
        let ranges = MeshRanges::new(self.reserve_space(sizes(data, lods, &meshlets))?);
        self.mesh_ranges.insert(mesh, ranges);
        self.mesh_bounds.remove(&mesh);
        self.write_mesh(gpu, mesh, ranges, data, lods, &meshlets)?;
        self.retire(frame, Retiring::Space(old.space));
        Ok(())
    }

    /// Writes a mesh's vertices, its LODs' indices and meshlets and its
    /// entry, into the space and slot reserved for it.
    fn write_mesh(
        &mut self,
        gpu: &Gpu,
        handle: MeshHandle,
        ranges: MeshRanges,
        mesh: &MeshData,
        lods: &[Lod],
        meshlets: &MeshMeshlets,
    ) -> Result<()> {
        let index_count: u64 = lods.iter().map(|lod| lod.indices.len() as u64).sum();
        let space = ranges.space;
        let (vertex_offset, index_start, slot) = (space.vertices.0, space.indices.0, handle.0);
        self.most_meshlets[slot as usize] = meshlets.most();
        if let (Some(ranges), Some(lod)) = (self.mesh_ranges.get_mut(&handle), lods.first()) {
            ranges.first_lod = lod.indices.len() as u32;
            ranges.sphere = mesh.bounds();
            ranges.keep_lods(lods.iter().map(|lod| LodInfo { triangles: lod.indices.len() as u32 / 3, error: lod.error }));
        }

        gpu.upload_buffer(
            &self.vertices,
            vertex_offset * size_of::<Vertex>() as u64,
            bytemuck::cast_slice(&mesh.vertices),
        )?;

        let (center, radius) = mesh.bounds();
        let lods_drawn =
            lods.iter().zip(&meshlets.counts).map(|(lod, &count)| (lod.error, lod.indices.len() as u32, count));
        let entry = mesh_entry(space, lods_drawn, (center, radius));
        let indices: Vec<u32> = lods.iter().flat_map(|lod| lod.indices.iter().copied()).collect();
        debug_assert_eq!(indices.len() as u64, index_count);
        gpu.upload_buffer(&self.indices, index_start * 4, bytemuck::cast_slice(&indices))?;
        let meshlet_data: [(&Buffer, u64, &[u8]); 3] = [
            (
                &self.meshlets,
                space.meshlets.0 * size_of::<GpuMeshlet>() as u64,
                bytemuck::cast_slice(&meshlets.meshlets),
            ),
            (&self.meshlet_vertices, space.meshlet_vertices.0 * 4, bytemuck::cast_slice(&meshlets.vertices)),
            (&self.meshlet_triangles, space.meshlet_triangles.0 * 4, bytemuck::cast_slice(&meshlets.triangles)),
        ];
        for (buffer, offset, bytes) in meshlet_data.into_iter().filter(|(_, _, bytes)| !bytes.is_empty()) {
            gpu.upload_buffer(buffer, offset, bytes)?;
        }

        gpu.upload_buffer(&self.meshes, u64::from(slot) * size_of::<GpuMesh>() as u64, bytemuck::bytes_of(&entry))?;
        let (min, max) = mesh.vertices.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(min, max), v| {
            let p = Vec3::from(v.position);
            (min.min(p), max.max(p))
        });
        if !mesh.vertices.is_empty() {
            self.mesh_bounds.insert(handle, (min, max));
        }
        if let (Some(hierarchies), Some(lod)) = (&mut self.hierarchies, lods.first()) {
            let points: Vec<Vec3> = mesh.vertices.iter().map(|v| Vec3::from(v.position)).collect();
            hierarchies.add(gpu, handle.0, &points, &lod.indices)?;
        }
        self.build_bottom_level(gpu, handle)
    }

    /// Builds a mesh's bottom level from its most detailed LOD, where ray
    /// tracing is on, replacing any it had.
    fn build_bottom_level(&mut self, gpu: &Gpu, handle: MeshHandle) -> Result<()> {
        let Some(&ranges) = self.mesh_ranges.get(&handle) else {
            return Ok(());
        };
        if gpu.acceleration_structure.is_none() || ranges.first_lod < 3 {
            return Ok(());
        }
        let stride = size_of::<Vertex>() as u64;
        let triangles = Triangles {
            vertices: self.vertices.address + ranges.space.vertices.0 * stride,
            stride,
            vertex_count: ranges.space.vertices.1 as u32,
            indices: self.indices.address + ranges.space.indices.0 * 4,
            triangle_count: ranges.first_lod / 3,
        };
        let built = gpu.build_bottom_level(&format!("mesh {}", handle.0), triangles)?;
        if let Some(old) = self.bottom_levels.insert(handle, built) {
            self.retired_levels.push(old);
        }
        Ok(())
    }

    /// Where a mesh's bottom level is, for an instance of it in a top level;
    /// none without ray tracing.
    pub fn bottom_level(&self, mesh: MeshHandle) -> Option<vk::DeviceAddress> {
        self.bottom_levels.get(&mesh).map(|level| level.address)
    }

    /// A mesh's box in its own space, min and max corners.
    pub fn mesh_bounds(&self, mesh: MeshHandle) -> Option<(Vec3, Vec3)> {
        self.mesh_bounds.get(&mesh).copied()
    }

    /// The first index and index count of a mesh's most detailed LOD, for
    /// drawing it directly rather than through culling.
    pub fn first_lod(&self, mesh: MeshHandle) -> Option<(u32, u32)> {
        let ranges = self.mesh_ranges.get(&mesh)?;
        Some((ranges.space.indices.0 as u32, ranges.first_lod))
    }

    /// The most meshlets one draw of the mesh in table slot `mesh` can turn
    /// into: those of its LOD with the most.
    pub fn most_meshlets(&self, mesh: u32) -> u32 {
        self.most_meshlets.get(mesh as usize).copied().unwrap_or(0)
    }

    /// A mesh's bounding sphere in its own space, centre and radius, for
    /// framing it in a preview.
    pub fn mesh_sphere(&self, mesh: MeshHandle) -> Option<(Vec3, f32)> {
        Some(self.mesh_ranges.get(&mesh)?.sphere)
    }

    /// A mesh's LODs, the most detailed first.
    pub fn mesh_lods(&self, mesh: MeshHandle) -> Option<Vec<LodInfo>> {
        let ranges = self.mesh_ranges.get(&mesh)?;
        Some(ranges.lods[..usize::from(ranges.lod_count)].to_vec())
    }

    /// Takes space in every shared buffer and a mesh table slot, all or
    /// nothing. `sizes` counts vertices, indices, meshlets, meshlet vertices
    /// and meshlet triangles.
    fn reserve_mesh(&mut self, sizes: [u64; 5]) -> Result<(MeshHandle, MeshRanges)> {
        let space = self.reserve_space(sizes)?;
        let Some(slot) = self.mesh_slots.alloc() else {
            self.free_space(space);
            anyhow::bail!("too many meshes");
        };
        let ranges = MeshRanges::new(space);
        self.mesh_ranges.insert(MeshHandle(slot), ranges);
        Ok((MeshHandle(slot), ranges))
    }

    /// Room in each shared buffer for as many as `sizes` says, all or nothing.
    fn reserve_space(&mut self, sizes: [u64; 5]) -> Result<Space> {
        const WHAT: [&str; 5] = ["vertex", "index", "meshlet", "meshlet vertex", "meshlet triangle"];
        let mut ranges = [(0, 0); 5];
        for (index, &count) in sizes.iter().enumerate() {
            let Some(offset) = self.space[index].alloc(count) else {
                for (allocator, &(offset, count)) in self.space.iter_mut().zip(&ranges[..index]) {
                    allocator.free(offset, count);
                }
                anyhow::bail!("out of {} space", WHAT[index]);
            };
            ranges[index] = (offset, count);
        }
        let [vertices, indices, meshlets, meshlet_vertices, meshlet_triangles] = ranges;
        Ok(Space { vertices, indices, meshlets, meshlet_vertices, meshlet_triangles })
    }

    fn free_space(&mut self, space: Space) {
        for (allocator, (offset, count)) in self.space.iter_mut().zip(space.ranges()) {
            allocator.free(offset, count);
        }
    }

    /// Uploads a cooked model's meshes and textures with one submission. A
    /// compressed payload is decompressed by the GPU when `decompress` is
    /// given, and otherwise on this thread.
    pub fn add_cooked(
        &mut self,
        gpu: &Gpu,
        bindless: &mut Bindless,
        model: &CookedModel,
        decompress: Option<&nv::memory_decompression::Device>,
    ) -> Result<(Vec<MeshHandle>, Vec<TextureId>)> {
        let mut meshes = Vec::with_capacity(model.meshes.len());
        for mesh in &model.meshes {
            let index_count = mesh.lods.iter().map(|&(_, count, _)| u64::from(count)).sum();
            let sizes =
                [mesh.vertices.1, mesh.meshlets.1, mesh.meshlet_vertices.1, mesh.meshlet_triangles.1].map(u64::from);
            match self.reserve_mesh([sizes[0], index_count, sizes[1], sizes[2], sizes[3]]) {
                Ok(reserved) => meshes.push(reserved),
                Err(error) => {
                    meshes.into_iter().for_each(|(handle, _)| self.release_mesh(handle));
                    return Err(error);
                }
            }
        }
        let mut images = Vec::with_capacity(model.textures.len());
        for texture in &model.textures {
            let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
            let format = texture.format.vulkan(texture.srgb);
            let desc = ImageDesc {
                mip_levels: texture.mips.len() as u32,
                ..ImageDesc::new(texture.width, texture.height, format, usage)
            };
            images.push(gpu.create_image(&texture.name, desc)?);
        }

        let tiled = match (&model.payload, decompress) {
            (Payload::Tiled { size, tiles, data }, Some(device)) => Some((*size, tiles.as_slice(), data, device)),
            _ => None,
        };
        let unpacked = if tiled.is_some() { None } else { Some(model.payload.unpack()?) };
        let source: &[u8] = match (&tiled, &unpacked) {
            (Some((_, _, data, _)), _) => data,
            (None, Some(bytes)) => bytes,
            (None, None) => unreachable!("one or the other is set"),
        };
        let entries: Vec<GpuMesh> = model
            .meshes
            .iter()
            .zip(&meshes)
            .map(|(mesh, (_, ranges))| mesh_entry(ranges.space, mesh.lods.iter().copied(), (mesh.center, mesh.radius)))
            .collect();
        for (mesh, (handle, _)) in model.meshes.iter().zip(&meshes) {
            let reach = Vec3::splat(mesh.radius);
            self.most_meshlets[handle.0 as usize] = mesh.lods.iter().map(|&(_, _, count)| count).max().unwrap_or(0);
            self.mesh_bounds.insert(*handle, (mesh.center - reach, mesh.center + reach));
            if let (Some(ranges), Some(&(_, count, _))) = (self.mesh_ranges.get_mut(handle), mesh.lods.first()) {
                (ranges.first_lod, ranges.sphere) = (count, (mesh.center, mesh.radius));
                ranges.keep_lods(mesh.lods.iter().map(|&(error, count, _)| LodInfo { triangles: count / 3, error }));
            }
        }
        let table = source.len().next_multiple_of(16) as u64;
        let staging_size = table + (entries.len() * size_of::<GpuMesh>()) as u64;
        let staging = gpu.create_buffer(
            "model staging",
            staging_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
        )?;
        staging.write(0, source);
        staging.write(table, &entries);
        let scratch = match &tiled {
            Some((size, ..)) => {
                let usage = vk::BufferUsageFlags::TRANSFER_SRC;
                Some(gpu.create_buffer("decompressed model", *size, usage, MemoryLocation::GpuOnly)?)
            }
            None => None,
        };

        let recorded = gpu.immediate(|cmd| {
            let from = match (&tiled, &scratch) {
                (Some((size, tiles, _, device)), Some(scratch)) => {
                    decompress_tiles(gpu, cmd, device, (&staging, scratch), *size, tiles);
                    scratch.handle
                }
                _ => staging.handle,
            };
            for ((mesh, (handle, ranges)), entry) in model.meshes.iter().zip(&meshes).zip(0u64..) {
                let (space, vertex, meshlet) =
                    (ranges.space, size_of::<Vertex>() as u64, size_of::<GpuMeshlet>() as u64);
                let copies = [
                    (from, self.vertices.handle, mesh.vertices.0, space.vertices.0 * vertex, space.vertices.1 * vertex),
                    (from, self.indices.handle, mesh.indices, space.indices.0 * 4, space.indices.1 * 4),
                    (
                        from,
                        self.meshlets.handle,
                        mesh.meshlets.0,
                        space.meshlets.0 * meshlet,
                        space.meshlets.1 * meshlet,
                    ),
                    (
                        from,
                        self.meshlet_vertices.handle,
                        mesh.meshlet_vertices.0,
                        space.meshlet_vertices.0 * 4,
                        space.meshlet_vertices.1 * 4,
                    ),
                    (
                        from,
                        self.meshlet_triangles.handle,
                        mesh.meshlet_triangles.0,
                        space.meshlet_triangles.0 * 4,
                        space.meshlet_triangles.1 * 4,
                    ),
                ];
                let table_entry = table + entry * size_of::<GpuMesh>() as u64;
                let slot = u64::from(handle.0) * size_of::<GpuMesh>() as u64;
                let table_copy = (staging.handle, self.meshes.handle, table_entry, slot, size_of::<GpuMesh>() as u64);
                let copies = copies.into_iter().chain([table_copy]).filter(|copy| copy.4 > 0);
                for (src, dst, src_offset, dst_offset, size) in copies {
                    let region = vk::BufferCopy { src_offset, dst_offset, size };
                    unsafe { gpu.device.cmd_copy_buffer(cmd, src, dst, &[region]) };
                }
            }
            for (texture, image) in model.textures.iter().zip(&images) {
                copy_mips(gpu, cmd, from, texture, image);
            }
            // Everything later frames read was written by transfers here.
            let barrier = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ);
            unsafe {
                gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[barrier]))
            };
        });
        gpu.destroy_buffer(staging);
        if let Some(scratch) = scratch {
            gpu.destroy_buffer(scratch);
        }
        recorded?;
        for (handle, _) in &meshes {
            self.build_bottom_level(gpu, *handle)?;
        }
        if self.hierarchies.is_some() {
            // Read back out of the model's data, which the GPU may have decompressed itself.
            let bytes = match &unpacked {
                Some(bytes) => std::borrow::Cow::Borrowed(bytes.as_ref()),
                None => std::borrow::Cow::Owned(model.payload.unpack()?.into_owned()),
            };
            for (mesh, (handle, _)) in model.meshes.iter().zip(&meshes) {
                let (points, indices) = crate::cooked::finest_triangles(&bytes, mesh)?;
                self.hierarchies.as_mut().expect("checked above").add(gpu, handle.0, &points, &indices)?;
            }
        }
        if tiled.is_some() {
            self.decompressed_on_gpu += model.payload.size();
        }
        let how = if tiled.is_some() { "decompressed on the GPU" } else { "uncompressed" };
        tracing::debug!("uploaded {} in one submission: {} bytes, {how}", model.path(), model.payload.size());

        let textures = images
            .into_iter()
            .map(|image| {
                let id = bindless.add_texture(gpu, image.view);
                self.textures.insert(id, image);
                id
            })
            .collect();
        Ok((meshes.into_iter().map(|(handle, _)| handle).collect(), textures))
    }

    /// Gives back what [`reserve_mesh`](Self::reserve_mesh) took, for a mesh
    /// no frame has drawn yet.
    fn release_mesh(&mut self, handle: MeshHandle) {
        self.mesh_bounds.remove(&handle);
        if let Some(ranges) = self.mesh_ranges.remove(&handle) {
            self.free_space(ranges.space);
            self.mesh_slots.free(handle.0);
        }
    }

    /// Built-in meshes, created once on first use.
    pub fn primitive(&mut self, gpu: &Gpu, primitive: Primitive) -> Result<MeshHandle> {
        if let Some(&handle) = self.primitives.get(&primitive) {
            return Ok(handle);
        }
        let handle = self.add_mesh(gpu, &MeshData::primitive(primitive))?;
        self.primitives.insert(primitive, handle);
        Ok(handle)
    }

    /// Uploads a texture with a full mip chain.
    pub fn add_texture(&mut self, gpu: &Gpu, bindless: &mut Bindless, source: &TextureSource) -> Result<TextureId> {
        let TextureSource { name, width, height, rgba, srgb } = *source;
        ensure!(rgba.len() == (width * height * 4) as usize, "{name}: pixel data does not match {width}×{height}");
        let format = if srgb { vk::Format::R8G8B8A8_SRGB } else { vk::Format::R8G8B8A8_UNORM };
        let usage =
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC;
        let image = gpu.create_image(name, ImageDesc::new(width, height, format, usage).with_mips())?;
        gpu.upload_image(&image, rgba)?;
        let id = bindless.add_texture(gpu, image.view);
        self.textures.insert(id, image);
        Ok(id)
    }

    /// Uploads a texture whose mips were made ahead of time, such as a
    /// block-compressed one from a cooked model. `mips` holds level 0 first.
    pub fn add_texture_mips(
        &mut self,
        gpu: &Gpu,
        bindless: &mut Bindless,
        name: &str,
        (width, height): (u32, u32),
        format: vk::Format,
        mips: &[Vec<u8>],
    ) -> Result<TextureId> {
        let usage = vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST;
        let desc = ImageDesc { mip_levels: mips.len() as u32, ..ImageDesc::new(width, height, format, usage) };
        let image = gpu.create_image(name, desc)?;
        gpu.upload_mips(&image, mips)?;
        let id = bindless.add_texture(gpu, image.view);
        self.textures.insert(id, image);
        Ok(id)
    }

    pub fn add_material(&mut self, gpu: &Gpu, desc: &MaterialDesc) -> Result<MaterialHandle> {
        let slot = self.material_slots.alloc().context("too many materials")?;
        let handle = MaterialHandle(slot);
        self.set_material(gpu, handle, desc)?;
        Ok(handle)
    }

    /// Writes a material's values over what its slot held, so what it is
    /// drawn on changes from the next frame.
    pub fn set_material(&mut self, gpu: &Gpu, handle: MaterialHandle, desc: &MaterialDesc) -> Result<()> {
        let slot = handle.0;
        let texture = |t: Option<TextureId>| t.unwrap_or(self.white).0;
        let material = GpuMaterial {
            base_color: desc.base_color,
            emissive: desc.emissive,
            metallic: desc.metallic,
            roughness: desc.roughness,
            base_color_texture: texture(desc.base_color_texture),
            metallic_roughness_texture: texture(desc.metallic_roughness_texture),
            normal_texture: texture(desc.normal_texture),
            emissive_texture: texture(desc.emissive_texture),
            flags: if desc.normal_texture.is_some() { MATERIAL_NORMAL_MAP } else { 0 }
                | if desc.foliage { MATERIAL_FOLIAGE } else { 0 },
            normal_scale: desc.normal_scale,
            alpha_cutoff: desc.alpha_cutoff.unwrap_or(0.0),
        };
        let offset = u64::from(slot) * size_of::<GpuMaterial>() as u64;
        gpu.upload_buffer(&self.materials, offset, bytemuck::bytes_of(&material))?;
        Ok(())
    }

    /// Frees a mesh once frames in flight are done with it. Built-in meshes
    /// stay.
    pub fn free_mesh(&mut self, mesh: MeshHandle, frame: u64) {
        let builtin = self.primitives.values().any(|&p| p == mesh);
        if !builtin && self.mesh_ranges.contains_key(&mesh) {
            self.retire(frame, Retiring::Mesh(mesh));
        }
    }

    /// Frees a material once frames in flight are done with it. The default
    /// material stays.
    pub fn free_material(&mut self, material: MaterialHandle, frame: u64) {
        if material != self.default_material {
            self.retire(frame, Retiring::Material(material));
        }
    }

    pub fn free_texture(&mut self, texture: TextureId, frame: u64) {
        if texture != self.white && self.textures.contains_key(&texture) {
            self.retire(frame, Retiring::Texture(texture));
        }
    }

    fn retire(&mut self, frame: u64, what: Retiring) {
        self.retiring.push((frame + FRAMES_IN_FLIGHT as u64, what));
    }

    /// Retires the bottom levels replaced since the last frame, as of `frame`.
    fn retire_levels(&mut self, frame: u64) {
        for level in std::mem::take(&mut self.retired_levels) {
            self.retire(frame, Retiring::BottomLevel(level));
        }
    }

    /// Gives back everything whose frames in flight have finished. Call at
    /// the start of a frame, once its slot's fence has been waited on.
    pub fn collect(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: u64) {
        self.retire_levels(frame);
        let (ready, waiting): (Vec<_>, Vec<_>) = self.retiring.drain(..).partition(|(at, _)| *at <= frame);
        self.retiring = waiting;
        for (_, what) in ready {
            self.reclaim(gpu, bindless, what);
        }
    }

    fn reclaim(&mut self, gpu: &Gpu, bindless: &mut Bindless, what: Retiring) {
        match what {
            Retiring::BottomLevel(level) => gpu.destroy_acceleration_structure(level),
            Retiring::Mesh(mesh) => {
                self.mesh_bounds.remove(&mesh);
                if let Some(hierarchies) = &mut self.hierarchies {
                    hierarchies.remove(mesh.0);
                }
                if let Some(level) = self.bottom_levels.remove(&mesh) {
                    gpu.destroy_acceleration_structure(level);
                }
                if let Some(ranges) = self.mesh_ranges.remove(&mesh) {
                    self.free_space(ranges.space);
                    self.mesh_slots.free(mesh.0);
                }
            }
            Retiring::Space(space) => self.free_space(space),
            Retiring::Material(material) => self.material_slots.free(material.0),
            Retiring::Texture(texture) => {
                if let Some(image) = self.textures.remove(&texture) {
                    bindless.remove_texture(texture);
                    gpu.destroy_image(image);
                }
            }
        }
    }

    pub fn usage(&self) -> AssetUsage {
        AssetUsage {
            meshes: self.mesh_slots.used(),
            materials: self.material_slots.used(),
            textures: self.textures.len() as u32,
            vertices: self.space[0].used(),
            indices: self.space[1].used(),
            retiring: self.retiring.len() as u32,
            decompressed_on_gpu: self.decompressed_on_gpu,
        }
    }

    pub fn destroy(self, gpu: &Gpu, bindless: &mut Bindless) {
        let retired = self.retiring.into_iter().filter_map(|(_, what)| match what {
            Retiring::BottomLevel(level) => Some(level),
            _ => None,
        });
        for level in self.bottom_levels.into_values().chain(self.retired_levels).chain(retired) {
            gpu.destroy_acceleration_structure(level);
        }
        if let Some(hierarchies) = self.hierarchies {
            hierarchies.destroy(gpu);
        }
        for (id, image) in self.textures {
            bindless.remove_texture(id);
            gpu.destroy_image(image);
        }
        let meshlets = [self.meshlets, self.meshlet_vertices, self.meshlet_triangles];
        for buffer in [self.vertices, self.indices, self.meshes, self.materials].into_iter().chain(meshlets) {
            gpu.destroy_buffer(buffer);
        }
    }
}

/// How much of each shared buffer a mesh takes, as [`Assets::reserve_space`] counts.
fn sizes(mesh: &MeshData, lods: &[Lod], meshlets: &MeshMeshlets) -> [u64; 5] {
    let index_count = lods.iter().map(|lod| lod.indices.len()).sum::<usize>();
    [mesh.vertices.len(), index_count, meshlets.meshlets.len(), meshlets.vertices.len(), meshlets.triangles.len()]
        .map(|count| count as u64)
}

/// A mesh's table entry, once its space is known, from its LODs' errors,
/// index counts and meshlet counts, finest first, and its bounding sphere.
fn mesh_entry(space: Space, lods: impl Iterator<Item = (f32, u32, u32)>, (center, radius): (Vec3, f32)) -> GpuMesh {
    let mut entry = GpuMesh {
        vertex_offset: space.vertices.0 as u32,
        meshlet_vertex_offset: space.meshlet_vertices.0 as u32,
        meshlet_triangle_offset: space.meshlet_triangles.0 as u32,
        center: center.into(),
        radius,
        ..Default::default()
    };
    let (mut index_offset, mut meshlet_offset) = (space.indices.0 as u32, space.meshlets.0 as u32);
    for (slot, (error, index_count, meshlet_count)) in entry.lods.iter_mut().zip(lods) {
        *slot = GpuMeshLod { index_offset, index_count, error, meshlet_offset, meshlet_count, _pad: 0 };
        index_offset += index_count;
        meshlet_offset += meshlet_count;
        entry.lod_count += 1;
    }
    entry
}

/// Unpacks every tile of a payload from `staging` into `scratch`, one region
/// per tile, all at once, then makes the result readable by copies.
fn decompress_tiles(
    gpu: &Gpu,
    cmd: vk::CommandBuffer,
    device: &nv::memory_decompression::Device,
    (staging, scratch): (&Buffer, &Buffer),
    size: u64,
    tiles: &[(u32, u32)],
) {
    let regions: Vec<vk::DecompressMemoryRegionNV> = tiles
        .iter()
        .enumerate()
        .map(|(index, &(offset, len))| vk::DecompressMemoryRegionNV {
            src_address: staging.address + u64::from(offset),
            dst_address: scratch.address + (index * crate::payload::TILE) as u64,
            compressed_size: u64::from(len),
            decompressed_size: tile_len(size, index),
            decompression_method: vk::MemoryDecompressionMethodFlagsNV::GDEFLATE_1_0,
        })
        .collect();
    // The NV extension names no stage of its own, so the barrier covers all.
    let barrier = vk::MemoryBarrier2::default()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_READ);
    unsafe {
        device.cmd_decompress_memory(cmd, &regions);
        gpu.device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[barrier]));
    }
}

/// Copies a texture's mips out of the payload into its image, leaving it
/// ready to sample.
fn copy_mips(gpu: &Gpu, cmd: vk::CommandBuffer, from: vk::Buffer, texture: &CookedTexture, image: &Image) {
    let regions: Vec<vk::BufferImageCopy> = texture
        .mips
        .iter()
        .enumerate()
        .map(|(level, &(offset, _))| {
            let (width, height) = ((texture.width >> level).max(1), (texture.height >> level).max(1));
            let layers = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(level as u32)
                .layer_count(1);
            vk::BufferImageCopy::default().buffer_offset(offset).image_subresource(layers).image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
        })
        .collect();
    let count = regions.len() as u32;
    mip_barrier(gpu, cmd, image.handle, 0, count, Transition::UndefinedToTransferDst);
    unsafe {
        gpu.device.cmd_copy_buffer_to_image(cmd, from, image.handle, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &regions);
    }
    mip_barrier(gpu, cmd, image.handle, 0, count, Transition::DstToShaderRead);
}
