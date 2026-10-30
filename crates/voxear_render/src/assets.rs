//! GPU storage for meshes, materials and textures.
//!
//! Every mesh lives in one shared vertex buffer and one shared index buffer,
//! described by a mesh table, so a single indirect draw call can draw any
//! mix of meshes. Content frees what nothing uses; the space and slots go
//! back to their allocators once no frame in flight can still read them.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use voxear_rhi::{Bindless, Buffer, FRAMES_IN_FLIGHT, Gpu, Image, ImageDesc, MemoryLocation, TextureId, vk};

use crate::{
    alloc::{RangeAllocator, SlotAllocator},
    gpu::{GpuMaterial, GpuMesh, GpuMeshLod, MATERIAL_NORMAL_MAP},
    lod::{Lod, build_lods},
    mesh::{MeshData, Primitive, Vertex},
};

const MAX_VERTICES: u64 = 2 * 1024 * 1024;
const MAX_INDICES: u64 = 8 * 1024 * 1024;
const MAX_MESHES: u64 = 16 * 1024;
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
}

/// Where a mesh's data sits in the shared buffers.
struct MeshRanges {
    vertices: (u64, u64),
    indices: (u64, u64),
}

#[derive(Clone, Copy)]
enum Retiring {
    Mesh(MeshHandle),
    Material(MaterialHandle),
    Texture(TextureId),
}

pub struct Assets {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub meshes: Buffer,
    pub materials: Buffer,
    vertex_space: RangeAllocator,
    index_space: RangeAllocator,
    mesh_slots: SlotAllocator,
    material_slots: SlotAllocator,
    mesh_ranges: HashMap<MeshHandle, MeshRanges>,
    textures: HashMap<TextureId, Image>,
    /// Freed resources and the frame from which nothing in flight uses them.
    retiring: Vec<(u64, Retiring)>,
    white: TextureId,
    primitives: HashMap<Primitive, MeshHandle>,
    pub default_material: MaterialHandle,
}

impl Assets {
    pub fn new(gpu: &Gpu, bindless: &mut Bindless) -> Result<Self> {
        let device_local = MemoryLocation::GpuOnly;
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let vertices =
            gpu.create_buffer("vertices", MAX_VERTICES * size_of::<Vertex>() as u64, storage, device_local)?;
        let indices = gpu.create_buffer(
            "indices",
            MAX_INDICES * 4,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            device_local,
        )?;
        let meshes = gpu.create_buffer("meshes", MAX_MESHES * size_of::<GpuMesh>() as u64, storage, device_local)?;
        let materials =
            gpu.create_buffer("materials", MAX_MATERIALS * size_of::<GpuMaterial>() as u64, storage, device_local)?;

        let mut assets = Self {
            vertices,
            indices,
            meshes,
            materials,
            vertex_space: RangeAllocator::new(MAX_VERTICES),
            index_space: RangeAllocator::new(MAX_INDICES),
            mesh_slots: SlotAllocator::new(MAX_MESHES as u32),
            material_slots: SlotAllocator::new(MAX_MATERIALS as u32),
            mesh_ranges: HashMap::new(),
            textures: HashMap::new(),
            retiring: Vec::new(),
            white: TextureId(0),
            primitives: HashMap::new(),
            default_material: MaterialHandle(0),
        };
        let white = TextureSource { name: "white", width: 1, height: 1, rgba: &[255; 4], srgb: false };
        assets.white = assets.add_texture(gpu, bindless, &white)?;
        assets.default_material = assets.add_material(gpu, &MaterialDesc::default())?;
        Ok(assets)
    }

    /// Uploads a mesh with an automatically generated LOD chain.
    pub fn add_mesh(&mut self, gpu: &Gpu, mesh: &MeshData) -> Result<MeshHandle> {
        self.add_mesh_with_lods(gpu, mesh, &build_lods(mesh))
    }

    /// Uploads a mesh whose LOD chain was built already, such as on a
    /// loading thread.
    pub fn add_mesh_with_lods(&mut self, gpu: &Gpu, mesh: &MeshData, lods: &[Lod]) -> Result<MeshHandle> {
        let vertex_count = mesh.vertices.len() as u64;
        let index_count: u64 = lods.iter().map(|lod| lod.indices.len() as u64).sum();
        let vertex_offset = self.vertex_space.alloc(vertex_count).context("out of vertex space")?;
        let Some(index_start) = self.index_space.alloc(index_count) else {
            self.vertex_space.free(vertex_offset, vertex_count);
            anyhow::bail!("out of index space");
        };
        let Some(slot) = self.mesh_slots.alloc() else {
            self.vertex_space.free(vertex_offset, vertex_count);
            self.index_space.free(index_start, index_count);
            anyhow::bail!("too many meshes");
        };

        gpu.upload_buffer(
            &self.vertices,
            vertex_offset * size_of::<Vertex>() as u64,
            bytemuck::cast_slice(&mesh.vertices),
        )?;

        let (center, radius) = mesh.bounds();
        let mut entry = GpuMesh {
            vertex_offset: vertex_offset as u32,
            lod_count: lods.len() as u32,
            center: center.into(),
            radius,
            ..Default::default()
        };
        let mut index_offset = index_start;
        let mut indices = Vec::with_capacity(index_count as usize);
        for (slot, lod) in entry.lods.iter_mut().zip(lods) {
            *slot = GpuMeshLod {
                index_offset: index_offset as u32,
                index_count: lod.indices.len() as u32,
                error: lod.error,
                _pad: 0,
            };
            index_offset += lod.indices.len() as u64;
            indices.extend_from_slice(&lod.indices);
        }
        gpu.upload_buffer(&self.indices, index_start * 4, bytemuck::cast_slice(&indices))?;

        let handle = MeshHandle(slot);
        gpu.upload_buffer(&self.meshes, u64::from(slot) * size_of::<GpuMesh>() as u64, bytemuck::bytes_of(&entry))?;
        let ranges = MeshRanges { vertices: (vertex_offset, vertex_count), indices: (index_start, index_count) };
        self.mesh_ranges.insert(handle, ranges);
        Ok(handle)
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

    pub fn add_material(&mut self, gpu: &Gpu, desc: &MaterialDesc) -> Result<MaterialHandle> {
        let slot = self.material_slots.alloc().context("too many materials")?;
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
            flags: if desc.normal_texture.is_some() { MATERIAL_NORMAL_MAP } else { 0 },
            normal_scale: desc.normal_scale,
            alpha_cutoff: desc.alpha_cutoff.unwrap_or(0.0),
        };
        let handle = MaterialHandle(slot);
        let offset = u64::from(slot) * size_of::<GpuMaterial>() as u64;
        gpu.upload_buffer(&self.materials, offset, bytemuck::bytes_of(&material))?;
        Ok(handle)
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

    /// Gives back everything whose frames in flight have finished. Call at
    /// the start of a frame, once its slot's fence has been waited on.
    pub fn collect(&mut self, gpu: &Gpu, bindless: &mut Bindless, frame: u64) {
        let (ready, waiting): (Vec<_>, Vec<_>) = self.retiring.drain(..).partition(|(at, _)| *at <= frame);
        self.retiring = waiting;
        for (_, what) in ready {
            self.reclaim(gpu, bindless, what);
        }
    }

    fn reclaim(&mut self, gpu: &Gpu, bindless: &mut Bindless, what: Retiring) {
        match what {
            Retiring::Mesh(mesh) => {
                if let Some(ranges) = self.mesh_ranges.remove(&mesh) {
                    self.vertex_space.free(ranges.vertices.0, ranges.vertices.1);
                    self.index_space.free(ranges.indices.0, ranges.indices.1);
                    self.mesh_slots.free(mesh.0);
                }
            }
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
            vertices: self.vertex_space.used(),
            indices: self.index_space.used(),
            retiring: self.retiring.len() as u32,
        }
    }

    pub fn destroy(self, gpu: &Gpu, bindless: &mut Bindless) {
        for (id, image) in self.textures {
            bindless.remove_texture(id);
            gpu.destroy_image(image);
        }
        for buffer in [self.vertices, self.indices, self.meshes, self.materials] {
            gpu.destroy_buffer(buffer);
        }
    }
}
