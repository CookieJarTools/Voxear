//! GPU storage for meshes, materials and textures.
//!
//! Every mesh lives in one shared vertex buffer and one shared index buffer,
//! described by a mesh table, so a single indirect draw call can draw any
//! mix of meshes. This is a stand-in until Content (M2) owns asset lifetimes.

use std::collections::HashMap;

use anyhow::{Result, ensure};
use voxear_rhi::{Bindless, Buffer, Gpu, Image, ImageDesc, MemoryLocation, TextureId, vk};

use crate::{
    gpu::{GpuMaterial, GpuMesh, MATERIAL_NORMAL_MAP},
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

pub struct Assets {
    pub vertices: Buffer,
    pub indices: Buffer,
    pub meshes: Buffer,
    pub materials: Buffer,
    vertex_count: u64,
    index_count: u64,
    mesh_count: u64,
    material_count: u64,
    textures: Vec<(Image, TextureId)>,
    white: TextureId,
    primitives: HashMap<Primitive, MeshHandle>,
    pub default_material: MaterialHandle,
}

impl Assets {
    pub fn new(gpu: &Gpu, bindless: &mut Bindless) -> Result<Self> {
        let device_local = MemoryLocation::GpuOnly;
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;
        let vertices = gpu.create_buffer("vertices", MAX_VERTICES * size_of::<Vertex>() as u64, storage, device_local)?;
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
            vertex_count: 0,
            index_count: 0,
            mesh_count: 0,
            material_count: 0,
            textures: Vec::new(),
            white: TextureId(0),
            primitives: HashMap::new(),
            default_material: MaterialHandle(0),
        };
        let white = TextureSource { name: "white", width: 1, height: 1, rgba: &[255; 4], srgb: false };
        assets.white = assets.add_texture(gpu, bindless, &white)?;
        assets.default_material = assets.add_material(gpu, &MaterialDesc::default())?;
        Ok(assets)
    }

    pub fn add_mesh(&mut self, gpu: &Gpu, mesh: &MeshData) -> Result<MeshHandle> {
        let vertex_count = mesh.vertices.len() as u64;
        let index_count = mesh.indices.len() as u64;
        ensure!(self.vertex_count + vertex_count <= MAX_VERTICES, "out of vertex space");
        ensure!(self.index_count + index_count <= MAX_INDICES, "out of index space");
        ensure!(self.mesh_count < MAX_MESHES, "too many meshes");

        let vertex_offset = self.vertex_count;
        let index_offset = self.index_count;
        gpu.upload_buffer(&self.vertices, vertex_offset * size_of::<Vertex>() as u64, bytemuck::cast_slice(&mesh.vertices))?;
        gpu.upload_buffer(&self.indices, index_offset * 4, bytemuck::cast_slice(&mesh.indices))?;

        let (center, radius) = mesh.bounds();
        let entry = GpuMesh {
            vertex_offset: vertex_offset as u32,
            index_offset: index_offset as u32,
            index_count: index_count as u32,
            _pad: 0,
            center: center.into(),
            radius,
        };
        let handle = MeshHandle(self.mesh_count as u32);
        gpu.upload_buffer(&self.meshes, self.mesh_count * size_of::<GpuMesh>() as u64, bytemuck::bytes_of(&entry))?;
        self.vertex_count += vertex_count;
        self.index_count += index_count;
        self.mesh_count += 1;
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
        self.textures.push((image, id));
        Ok(id)
    }

    pub fn add_material(&mut self, gpu: &Gpu, desc: &MaterialDesc) -> Result<MaterialHandle> {
        ensure!(self.material_count < MAX_MATERIALS, "too many materials");
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
        let handle = MaterialHandle(self.material_count as u32);
        let offset = self.material_count * size_of::<GpuMaterial>() as u64;
        gpu.upload_buffer(&self.materials, offset, bytemuck::bytes_of(&material))?;
        self.material_count += 1;
        Ok(handle)
    }

    pub fn destroy(self, gpu: &Gpu, bindless: &mut Bindless) {
        for (image, id) in self.textures {
            bindless.remove_texture(id);
            gpu.destroy_image(image);
        }
        for buffer in [self.vertices, self.indices, self.meshes, self.materials] {
            gpu.destroy_buffer(buffer);
        }
    }
}
