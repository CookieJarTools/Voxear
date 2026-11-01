//! glTF 2.0 import. glTF is right-handed and Voxear is left-handed like
//! Unity, so everything is mirrored on X at import and nothing is flipped at
//! runtime. Files come through a callback, so a model loads the same from a
//! project folder or from a content bundle. [`prepare`] does the slow work
//! (parsing, decoding images, building LODs) on any thread, and [`upload`]
//! then needs only the GPU. FBX files are read by `fbx_import` into the same
//! [`PreparedModel`]; [`prepare`] and [`companion_files`] take either.

use std::collections::HashMap;

use anyhow::{Context, Result};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_rhi::{Bindless, Gpu, TextureId, vk};

use crate::{
    assets::{Assets, MaterialDesc, MaterialHandle, MeshHandle, TextureSource},
    cooked::CookedModel,
    fbx_import,
    lod::{Lod, build_lods},
    mesh::{MeshData, Vertex},
    texture_codec,
};

/// A model on the GPU: its drawable pieces, and the textures it created,
/// which are freed with it.
pub struct ImportedModel {
    pub nodes: Vec<ImportedNode>,
    pub textures: Vec<TextureId>,
}

/// One drawable piece of the imported scene, with its transform relative to
/// the model's root.
pub struct ImportedNode {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    /// Stable name of the mesh inside the model, `mesh<mesh>.<primitive>`, for
    /// asset references like `<model guid>#mesh0.0`.
    pub mesh_part: String,
    /// `material<index>`, or `None` for glTF's default material.
    pub material_part: Option<String>,
}

/// Reads one of a model's files by project path: the `.gltf`, `.glb` or
/// `.fbx` itself, or a buffer or image it names.
pub type ReadFile<'a> = dyn FnMut(&str) -> Result<Vec<u8>> + 'a;

/// A model read and processed, ready to upload: meshes with their LOD chains,
/// decoded textures, materials and the node layout. Making one is the slow
/// part of an import, and it needs no GPU, so it can happen on any thread.
pub struct PreparedModel {
    pub(crate) path: String,
    pub(crate) meshes: Vec<(MeshData, Vec<Lod>)>,
    pub(crate) textures: Vec<PreparedTexture>,
    pub(crate) materials: Vec<PreparedMaterial>,
    pub(crate) nodes: Vec<PreparedNode>,
}

pub(crate) struct PreparedTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub format: TextureFormat,
    /// Level 0 first. RGBA8 textures have one level and the GPU makes the
    /// rest; compressed ones carry their whole chain.
    pub mips: Vec<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextureFormat {
    Rgba8 = 0,
    Bc7 = 1,
    Bc5 = 2,
}

impl TextureFormat {
    pub fn from_u32(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Rgba8,
            1 => Self::Bc7,
            2 => Self::Bc5,
            _ => return None,
        })
    }

    pub(crate) fn vulkan(self, srgb: bool) -> vk::Format {
        match (self, srgb) {
            (Self::Rgba8, true) => vk::Format::R8G8B8A8_SRGB,
            (Self::Rgba8, false) => vk::Format::R8G8B8A8_UNORM,
            (Self::Bc7, true) => vk::Format::BC7_SRGB_BLOCK,
            (Self::Bc7, false) => vk::Format::BC7_UNORM_BLOCK,
            (Self::Bc5, _) => vk::Format::BC5_UNORM_BLOCK,
        }
    }
}

/// A material whose textures are indices into the model's textures until upload.
#[derive(Clone)]
pub(crate) struct PreparedMaterial {
    pub desc: MaterialDesc,
    pub base_color: Option<usize>,
    pub metallic_roughness: Option<usize>,
    pub normal: Option<usize>,
    pub emissive: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct PreparedNode {
    pub name: String,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub mesh: usize,
    /// `None` for glTF's default material.
    pub material: Option<usize>,
    pub mesh_part: String,
    pub material_part: Option<String>,
}

impl PreparedModel {
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Replaces a texture's pixels with `rgba`, for tests and tools that
    /// need a particular image.
    pub fn replace_texture(&mut self, index: usize, width: u32, height: u32, rgba: Vec<u8>) {
        let texture = &mut self.textures[index];
        (texture.width, texture.height, texture.format, texture.mips) =
            (width, height, TextureFormat::Rgba8, vec![rgba]);
    }

    /// Compresses the textures for shipping: BC5 for normal maps, BC7 for
    /// the rest, each with its whole mip chain. A quarter of RGBA8's memory
    /// and bundle size. Textures whose sides are not multiples of 4 stay
    /// RGBA8.
    pub fn compress_textures(&mut self) {
        let normals: Vec<usize> = self.materials.iter().filter_map(|m| m.normal).collect();
        for (index, texture) in self.textures.iter_mut().enumerate() {
            let fits = texture.width % 4 == 0 && texture.height % 4 == 0;
            if texture.format != TextureFormat::Rgba8 || !fits {
                continue;
            }
            let normal = normals.contains(&index);
            let levels = texture_codec::mip_chain(&texture.mips[0], texture.width, texture.height, texture.srgb);
            let encode = if normal { texture_codec::encode_bc5 } else { texture_codec::encode_bc7 };
            texture.mips = levels.iter().map(|(w, h, pixels)| encode(pixels, *w, *h)).collect();
            texture.format = if normal { TextureFormat::Bc5 } else { TextureFormat::Bc7 };
        }
    }
}

/// Reads and processes a model without touching the GPU.
pub fn prepare(path: &str, read: &mut ReadFile) -> Result<PreparedModel> {
    if fbx_import::is_fbx(path) {
        return fbx_import::prepare(path, read).with_context(|| format!("reading {path}"));
    }
    let (document, buffers, images) = load(path, read).with_context(|| format!("reading {path}"))?;
    let mut preparer = Preparer {
        buffers,
        images,
        model: PreparedModel {
            path: path.to_string(),
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            nodes: Vec::new(),
        },
        meshes: HashMap::new(),
        textures: HashMap::new(),
        materials: HashMap::new(),
    };
    let scene = document.default_scene().or_else(|| document.scenes().next()).context("the file has no scenes")?;
    for node in scene.nodes() {
        preparer.visit(&node, Mat4::IDENTITY).with_context(|| format!("reading {path}"))?;
    }
    Ok(preparer.model)
}

/// Uploads a prepared model and returns its drawable pieces.
pub fn upload(gpu: &Gpu, bindless: &mut Bindless, assets: &mut Assets, model: &PreparedModel) -> Result<ImportedModel> {
    let mut textures = Vec::with_capacity(model.textures.len());
    for texture in &model.textures {
        let id = match texture.format {
            TextureFormat::Rgba8 => {
                let source = TextureSource {
                    name: &texture.name,
                    width: texture.width,
                    height: texture.height,
                    rgba: &texture.mips[0],
                    srgb: texture.srgb,
                };
                assets.add_texture(gpu, bindless, &source)?
            }
            format => assets.add_texture_mips(
                gpu,
                bindless,
                &texture.name,
                (texture.width, texture.height),
                format.vulkan(texture.srgb),
                &texture.mips,
            )?,
        };
        textures.push(id);
    }
    let materials = add_materials(gpu, assets, &model.materials, &textures)?;
    let mut meshes = Vec::with_capacity(model.meshes.len());
    for (data, lods) in &model.meshes {
        meshes.push(assets.add_mesh_with_lods(gpu, data, lods)?);
    }
    Ok(imported(assets, &model.nodes, &meshes, &materials, textures))
}

/// Uploads a cooked model: its meshes and textures with one submission, then
/// its materials.
pub fn upload_cooked(
    gpu: &Gpu,
    bindless: &mut Bindless,
    assets: &mut Assets,
    model: &CookedModel,
    decompress: bool,
) -> Result<ImportedModel> {
    let device = gpu.decompression.as_ref().filter(|_| decompress);
    let (meshes, textures) = assets.add_cooked(gpu, bindless, model, device)?;
    let materials = add_materials(gpu, assets, &model.materials, &textures)?;
    Ok(imported(assets, &model.nodes, &meshes, &materials, textures))
}

fn add_materials(
    gpu: &Gpu,
    assets: &mut Assets,
    materials: &[PreparedMaterial],
    textures: &[TextureId],
) -> Result<Vec<MaterialHandle>> {
    let texture = |index: Option<usize>| index.map(|i| textures[i]);
    materials
        .iter()
        .map(|material| {
            let desc = MaterialDesc {
                base_color_texture: texture(material.base_color),
                metallic_roughness_texture: texture(material.metallic_roughness),
                normal_texture: texture(material.normal),
                emissive_texture: texture(material.emissive),
                ..material.desc
            };
            assets.add_material(gpu, &desc)
        })
        .collect()
}

fn imported(
    assets: &Assets,
    nodes: &[PreparedNode],
    meshes: &[MeshHandle],
    materials: &[MaterialHandle],
    textures: Vec<TextureId>,
) -> ImportedModel {
    let nodes = nodes
        .iter()
        .map(|node| ImportedNode {
            name: node.name.clone(),
            position: node.position,
            rotation: node.rotation,
            scale: node.scale,
            mesh: meshes[node.mesh],
            material: node.material.map_or(assets.default_material, |m| materials[m]),
            mesh_part: node.mesh_part.clone(),
            material_part: node.material_part.clone(),
        })
        .collect();
    ImportedModel { nodes, textures }
}

/// Reads, processes and uploads a model in one go.
pub fn import(
    gpu: &Gpu,
    bindless: &mut Bindless,
    assets: &mut Assets,
    path: &str,
    read: &mut ReadFile,
) -> Result<ImportedModel> {
    upload(gpu, bindless, assets, &prepare(path, read)?)
}

struct Preparer {
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
    model: PreparedModel,
    meshes: HashMap<(usize, usize), usize>,
    textures: HashMap<(usize, bool), usize>,
    materials: HashMap<usize, usize>,
}

impl Preparer {
    fn visit(&mut self, node: &gltf::Node, parent: Mat4) -> Result<()> {
        let (translation, rotation, scale) = node.transform().decomposed();
        let local =
            Mat4::from_scale_rotation_translation(Vec3::from(scale), mirror_rotation(rotation), mirror(translation));
        let world = parent * local;

        if let Some(mesh) = node.mesh() {
            let (scale, rotation, position) = world.to_scale_rotation_translation();
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let mesh_index = self.mesh(&mesh, &primitive)?;
                let material = self.material(primitive.material())?;
                self.model.nodes.push(PreparedNode {
                    name: node.name().or(mesh.name()).unwrap_or("mesh").to_string(),
                    position,
                    rotation,
                    scale,
                    mesh: mesh_index,
                    material,
                    mesh_part: format!("mesh{}.{}", mesh.index(), primitive.index()),
                    material_part: primitive.material().index().map(|i| format!("material{i}")),
                });
            }
        }
        for child in node.children() {
            self.visit(&child, world)?;
        }
        Ok(())
    }

    fn mesh(&mut self, mesh: &gltf::Mesh, primitive: &gltf::Primitive) -> Result<usize> {
        let key = (mesh.index(), primitive.index());
        if let Some(&index) = self.meshes.get(&key) {
            return Ok(index);
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
        let lods = build_lods(&data);
        self.model.meshes.push((data, lods));
        let index = self.model.meshes.len() - 1;
        self.meshes.insert(key, index);
        Ok(index)
    }

    fn material(&mut self, material: gltf::Material) -> Result<Option<usize>> {
        let Some(key) = material.index() else { return Ok(None) };
        if let Some(&index) = self.materials.get(&key) {
            return Ok(Some(index));
        }
        let pbr = material.pbr_metallic_roughness();
        let base_color = pbr.base_color_texture().map(|t| self.texture(t.texture(), true)).transpose()?;
        let metallic_roughness =
            pbr.metallic_roughness_texture().map(|t| self.texture(t.texture(), false)).transpose()?;
        let normal_info = material.normal_texture();
        let normal_scale = normal_info.as_ref().map_or(1.0, |n| n.scale());
        let normal = normal_info.map(|t| self.texture(t.texture(), false)).transpose()?;
        let emissive = material.emissive_texture().map(|t| self.texture(t.texture(), true)).transpose()?;
        let alpha_cutoff = match material.alpha_mode() {
            gltf::material::AlphaMode::Mask => Some(material.alpha_cutoff().unwrap_or(0.5)),
            _ => None,
        };
        let desc = MaterialDesc {
            base_color: pbr.base_color_factor(),
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive: material.emissive_factor().map(|c| c * material.emissive_strength().unwrap_or(1.0)),
            normal_scale,
            alpha_cutoff,
            ..MaterialDesc::default()
        };
        self.model.materials.push(PreparedMaterial { desc, base_color, metallic_roughness, normal, emissive });
        let index = self.model.materials.len() - 1;
        self.materials.insert(key, index);
        Ok(Some(index))
    }

    fn texture(&mut self, texture: gltf::Texture, srgb: bool) -> Result<usize> {
        let image_index = texture.source().index();
        if let Some(&index) = self.textures.get(&(image_index, srgb)) {
            return Ok(index);
        }
        let image = &self.images[image_index];
        let rgba =
            to_rgba8(image).with_context(|| format!("texture {image_index} uses an unsupported pixel format"))?;
        self.model.textures.push(PreparedTexture {
            name: texture.name().unwrap_or("gltf texture").to_string(),
            width: image.width,
            height: image.height,
            srgb,
            format: TextureFormat::Rgba8,
            mips: vec![rgba],
        });
        let index = self.model.textures.len() - 1;
        self.textures.insert((image_index, srgb), index);
        Ok(index)
    }
}

/// The project paths of the files a glTF file names, relative to its own
/// folder: its buffers and images, but not those embedded in it. For an FBX
/// file, the textures it may read.
pub fn companion_files(path: &str, bytes: &[u8]) -> Result<Vec<String>> {
    if fbx_import::is_fbx(path) {
        return fbx_import::companion_files(path, bytes);
    }
    let document = gltf::Gltf::from_slice(bytes)?.document;
    let buffers = document.buffers().filter_map(|b| match b.source() {
        gltf::buffer::Source::Uri(uri) => Some(uri),
        gltf::buffer::Source::Bin => None,
    });
    let images = document.images().filter_map(|i| match i.source() {
        gltf::image::Source::Uri { uri, .. } => Some(uri),
        gltf::image::Source::View { .. } => None,
    });
    Ok(buffers.chain(images).filter(|uri| !uri.starts_with("data:")).map(|uri| resolve(path, uri)).collect())
}

type Loaded = (gltf::Document, Vec<gltf::buffer::Data>, Vec<gltf::image::Data>);

/// What `gltf::import` does, but reading every file through `read`.
fn load(path: &str, read: &mut ReadFile) -> Result<Loaded> {
    let gltf::Gltf { document, mut blob } = gltf::Gltf::from_slice(&read(path)?)?;
    let mut buffers = Vec::new();
    for buffer in document.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => blob.take().context("the file names a binary chunk it does not have")?,
            gltf::buffer::Source::Uri(uri) => read_uri(path, uri, read)?,
        };
        buffers.push(gltf::buffer::Data(data));
    }
    let mut images = Vec::new();
    for image in document.images() {
        let encoded = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = &buffers[view.buffer().index()].0;
                buffer[view.offset()..view.offset() + view.length()].to_vec()
            }
            gltf::image::Source::Uri { uri, .. } => read_uri(path, uri, read)?,
        };
        let decoded =
            image::load_from_memory(&encoded).with_context(|| format!("decoding image {}", image.index()))?.to_rgba8();
        let (width, height) = decoded.dimensions();
        images.push(gltf::image::Data {
            pixels: decoded.into_raw(),
            format: gltf::image::Format::R8G8B8A8,
            width,
            height,
        });
    }
    Ok((document, buffers, images))
}

fn read_uri(path: &str, uri: &str, read: &mut ReadFile) -> Result<Vec<u8>> {
    match uri.strip_prefix("data:") {
        Some(data) => {
            let (_, encoded) = data.split_once(";base64,").context("only base64 data URIs are supported")?;
            Ok(base64::decode(encoded)?)
        }
        None => read(&resolve(path, uri)),
    }
}

/// A URI relative to the glTF file, as a project path: percent escapes
/// decoded and `..` folded away.
fn resolve(path: &str, uri: &str) -> String {
    relative_to(path, &percent_decode(uri))
}

/// A path relative to the file at `path`, as a project path, with `..`
/// folded away.
pub(crate) fn relative_to(path: &str, relative: &str) -> String {
    let mut parts: Vec<String> = path.split('/').map(str::to_string).collect();
    parts.pop();
    for part in relative.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part.to_string()),
        }
    }
    parts.join("/")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%').then(|| text.get(i + 1..i + 3)).flatten();
        match escaped.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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
