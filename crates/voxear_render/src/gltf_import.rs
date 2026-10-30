//! glTF 2.0 import. glTF is right-handed and Voxear is left-handed like
//! Unity, so everything is mirrored on X at import and nothing is flipped at
//! runtime. This loads straight to the GPU; Content (M2) will cache imports.

use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_rhi::{Bindless, Gpu, TextureId};

use crate::{
    assets::{Assets, MaterialDesc, MaterialHandle, MeshHandle, TextureSource},
    mesh::{MeshData, Vertex},
};

/// One drawable piece of the imported scene, with its world transform.
pub struct ImportedNode {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
}

struct Importer<'a> {
    gpu: &'a Gpu,
    bindless: &'a mut Bindless,
    assets: &'a mut Assets,
    images: Vec<gltf::image::Data>,
    buffers: Vec<gltf::buffer::Data>,
    textures: HashMap<(usize, bool), TextureId>,
    materials: HashMap<Option<usize>, MaterialHandle>,
    meshes: HashMap<(usize, usize), MeshHandle>,
    nodes: Vec<ImportedNode>,
}

pub fn import(gpu: &Gpu, bindless: &mut Bindless, assets: &mut Assets, path: &Path) -> Result<Vec<ImportedNode>> {
    let (document, buffers, images) = gltf::import(path).with_context(|| format!("reading {}", path.display()))?;
    let mut importer = Importer {
        gpu,
        bindless,
        assets,
        images,
        buffers,
        textures: HashMap::new(),
        materials: HashMap::new(),
        meshes: HashMap::new(),
        nodes: Vec::new(),
    };
    let scene = document.default_scene().or_else(|| document.scenes().next()).context("the file has no scenes")?;
    for node in scene.nodes() {
        importer.visit(&node, Mat4::IDENTITY)?;
    }
    Ok(importer.nodes)
}

impl Importer<'_> {
    fn visit(&mut self, node: &gltf::Node, parent: Mat4) -> Result<()> {
        let (translation, rotation, scale) = node.transform().decomposed();
        let local = Mat4::from_scale_rotation_translation(Vec3::from(scale), mirror_rotation(rotation), mirror(translation));
        let world = parent * local;

        if let Some(mesh) = node.mesh() {
            let (scale, rotation, position) = world.to_scale_rotation_translation();
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let mesh_handle = self.mesh(&mesh, &primitive)?;
                let material = self.material(primitive.material())?;
                let name = node.name().or(mesh.name()).unwrap_or("mesh").to_string();
                self.nodes.push(ImportedNode { name, position, rotation, scale, mesh: mesh_handle, material });
            }
        }
        for child in node.children() {
            self.visit(&child, world)?;
        }
        Ok(())
    }

    fn mesh(&mut self, mesh: &gltf::Mesh, primitive: &gltf::Primitive) -> Result<MeshHandle> {
        let key = (mesh.index(), primitive.index());
        if let Some(&handle) = self.meshes.get(&key) {
            return Ok(handle);
        }
        let reader = primitive.reader(|buffer| Some(&self.buffers[buffer.index()]));
        let positions: Vec<[f32; 3]> = reader.read_positions().context("primitive has no positions")?.collect();
        let normals: Vec<[f32; 3]> = reader.read_normals().map(|n| n.collect()).unwrap_or_default();
        let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).map(|t| t.into_f32().collect()).unwrap_or_default();
        let tangents: Vec<[f32; 4]> = reader.read_tangents().map(|t| t.collect()).unwrap_or_default();
        let indices: Vec<u32> = match reader.read_indices() {
            Some(indices) => indices.into_u32().collect(),
            None => (0..positions.len() as u32).collect(),
        };

        let vertices = positions
            .iter()
            .enumerate()
            .map(|(i, &position)| Vertex {
                position,
                normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                uv: uvs.get(i).copied().unwrap_or_default(),
                tangent: tangents.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]),
            })
            .collect();
        let mut data = MeshData { vertices, indices };
        data.mirror_x();
        let handle = self.assets.add_mesh(self.gpu, &data)?;
        self.meshes.insert(key, handle);
        Ok(handle)
    }

    fn material(&mut self, material: gltf::Material) -> Result<MaterialHandle> {
        let key = material.index();
        if let Some(&handle) = self.materials.get(&key) {
            return Ok(handle);
        }
        if key.is_none() {
            return Ok(self.assets.default_material);
        }
        let pbr = material.pbr_metallic_roughness();
        let base_color_texture = pbr.base_color_texture().map(|t| self.texture(t.texture(), true)).transpose()?;
        let metallic_roughness_texture =
            pbr.metallic_roughness_texture().map(|t| self.texture(t.texture(), false)).transpose()?;
        let normal = material.normal_texture();
        let normal_scale = normal.as_ref().map_or(1.0, |n| n.scale());
        let normal_texture = normal.map(|t| self.texture(t.texture(), false)).transpose()?;
        let emissive_texture = material.emissive_texture().map(|t| self.texture(t.texture(), true)).transpose()?;
        let alpha_cutoff = match material.alpha_mode() {
            gltf::material::AlphaMode::Mask => Some(material.alpha_cutoff().unwrap_or(0.5)),
            _ => None,
        };
        let desc = MaterialDesc {
            base_color: pbr.base_color_factor(),
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive: material.emissive_factor().map(|c| c * material.emissive_strength().unwrap_or(1.0)),
            base_color_texture,
            metallic_roughness_texture,
            normal_texture,
            normal_scale,
            emissive_texture,
            alpha_cutoff,
        };
        let handle = self.assets.add_material(self.gpu, &desc)?;
        self.materials.insert(key, handle);
        Ok(handle)
    }

    fn texture(&mut self, texture: gltf::Texture, srgb: bool) -> Result<TextureId> {
        let index = texture.source().index();
        if let Some(&id) = self.textures.get(&(index, srgb)) {
            return Ok(id);
        }
        let image = &self.images[index];
        let rgba = to_rgba8(image).with_context(|| format!("texture {index} uses an unsupported pixel format"))?;
        let source = TextureSource {
            name: texture.name().unwrap_or("gltf texture"),
            width: image.width,
            height: image.height,
            rgba: &rgba,
            srgb,
        };
        let id = self.assets.add_texture(self.gpu, self.bindless, &source)?;
        self.textures.insert((index, srgb), id);
        Ok(id)
    }
}

fn mirror(v: [f32; 3]) -> Vec3 {
    Vec3::new(-v[0], v[1], v[2])
}

/// Mirroring on X keeps the rotation angle about X and reverses it about Y and Z.
fn mirror_rotation(q: [f32; 4]) -> Quat {
    Quat::from_xyzw(q[0], -q[1], -q[2], q[3])
}

fn to_rgba8(image: &gltf::image::Data) -> Option<Vec<u8>> {
    use gltf::image::Format;
    let pixels = &image.pixels;
    Some(match image.format {
        Format::R8G8B8A8 => pixels.clone(),
        Format::R8G8B8 => pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        Format::R8G8 => pixels.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[1], 0, 255]).collect(),
        Format::R8 => pixels.iter().flat_map(|&p| [p, p, p, 255]).collect(),
        // 16-bit channels are little-endian; keep the high byte.
        Format::R16G16B16A16 => pixels.as_chunks::<2>().0.iter().map(|p| p[1]).collect(),
        Format::R16G16B16 => pixels.as_chunks::<6>().0.iter().flat_map(|p| [p[1], p[3], p[5], 255]).collect(),
        _ => return None,
    })
}
