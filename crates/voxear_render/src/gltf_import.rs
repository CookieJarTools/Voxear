//! glTF 2.0 import. glTF is right-handed and Voxear is left-handed like
//! Unity, so everything is mirrored on X at import and nothing is flipped at
//! runtime. Files come through a callback, so a model loads the same from a
//! project folder or from a content bundle. [`prepare`] does the slow work
//! (parsing, decoding images, building LODs) on any thread, and [`upload`]
//! then needs only the GPU. FBX files are read by `fbx_import` into the same
//! [`PreparedModel`], and box models by `box_model`; [`prepare`] and
//! [`companion_files`] take any of them.
//!
//! A model's import settings come from its `.meta` file, read through the
//! same callback (see `voxear_assets::ModelImport` and `mesh_tools`), as do
//! those of each picture file it reads (`voxear_assets::TextureImport`), so
//! the editor, a game run from the project and a content build all import
//! it alike. [`reupload`] puts a model imported again behind the handles it
//! had, for settings changed while it is drawn.

use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use voxear_assets::{ModelImport, TextureImport, meta_path};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_rhi::{Bindless, Gpu, TextureId, vk};

use crate::{
    assets::{Assets, MaterialDesc, MaterialHandle, MeshHandle, TextureSource},
    cooked::CookedModel,
    fbx_import,
    lod::Lod,
    mesh::{MeshData, Vertex},
    mesh_tools::{self, FromFile},
    skeleton::{Bone, BonePose, CLIP_RATE, Clip, ModelAnimation, Skeleton, Skin, SkinVertex},
    texture_codec,
};

/// A model on the GPU: its drawable pieces, and the textures it created,
/// which are freed with it.
pub struct ImportedModel {
    pub nodes: Vec<ImportedNode>,
    pub textures: Vec<TextureId>,
    /// Its skeleton, skins and clips, for a model made to animate.
    pub animation: Option<std::sync::Arc<ModelAnimation>>,
    /// Each material part, `material<index>`, with its name in the file, in
    /// the order the parts first draw.
    pub materials: Vec<(String, String)>,
    /// What part of a body each of its bones moves, one for each bone of
    /// its skeleton, for a model that says: a box model. Empty for others.
    pub roles: Vec<crate::BoneRole>,
}

/// One drawable piece of the imported scene, with its transform relative to
/// the model's root.
#[derive(Clone, Debug)]
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
    /// The skin that moves its mesh, among the model's animation's skins;
    /// its place is then the model's own, as the skin's bones place it.
    pub skin: Option<usize>,
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
    pub(crate) animation: Option<ModelAnimation>,
    /// What part of a body each bone moves (see [`ImportedModel::roles`]).
    pub(crate) roles: Vec<crate::BoneRole>,
}

pub(crate) struct PreparedTexture {
    /// Its name in the file, or empty when it has none.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub format: TextureFormat,
    /// Level 0 first. RGBA8 textures have one level and the GPU makes the
    /// rest; compressed ones carry their whole chain.
    pub mips: Vec<Vec<u8>>,
    /// Whether [`PreparedModel::compress_textures`] compresses it, as its
    /// picture's import settings say.
    pub compress: bool,
    /// The project path of the picture file it was read from, or `None`
    /// for one inside the model's file.
    pub source: Option<String>,
}

impl PreparedTexture {
    /// What the GPU's tools call it.
    fn label(&self) -> &str {
        if self.name.is_empty() { "model texture" } else { &self.name }
    }
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
    /// Its name in the file. A cooked model's have none.
    pub name: String,
    pub desc: MaterialDesc,
    pub base_color: Option<usize>,
    pub metallic_roughness: Option<usize>,
    pub normal: Option<usize>,
    pub emissive: Option<usize>,
}

impl PreparedMaterial {
    /// The default material's look, for a model imported without its
    /// materials: each keeps a material of its own, so its references
    /// still work and taking the materials back needs no new ones.
    pub(crate) fn plain(name: &str) -> Self {
        let desc = MaterialDesc::default();
        Self { name: name.to_string(), desc, base_color: None, metallic_roughness: None, normal: None, emissive: None }
    }
}

/// One of a model's materials as its file has it, for the editor to list
/// and to extract into a `.material` asset.
#[derive(Clone, Debug)]
pub struct FileMaterial {
    /// `material<index>`, as asset references name it.
    pub part: String,
    pub name: String,
    pub desc: MaterialDesc,
    /// Its colour, metallic and roughness, normal and emissive maps' pictures.
    pub maps: [Option<FilePicture>; 4],
}

/// A picture a model's material reads.
#[derive(Clone, Debug, PartialEq)]
pub enum FilePicture {
    /// A picture file in the project, by its path.
    File(String),
    /// A picture inside the model's file: its name, empty when it has none,
    /// and its pixels, RGBA8.
    Inside { name: String, width: u32, height: u32, rgba: Vec<u8> },
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
    /// The skin that moves its mesh (see [`ImportedNode::skin`]).
    pub skin: Option<usize>,
}

impl PreparedModel {
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Its skeleton, skins and clips, for a model made to animate.
    pub fn animation(&self) -> Option<&ModelAnimation> {
        self.animation.as_ref()
    }

    /// What part of a body each of its bones moves, for a model that says.
    pub fn roles(&self) -> &[crate::BoneRole] {
        &self.roles
    }

    /// The triangles of a part's mesh, by its name (`mesh0.0`), or of the
    /// first part's for none, for a Mesh collider.
    pub fn triangles(&self, part: Option<&str>) -> Option<crate::MeshTriangles> {
        let node = match part {
            Some(part) => self.nodes.iter().find(|node| node.mesh_part == part),
            None => self.nodes.first(),
        }?;
        Some(self.meshes.get(node.mesh)?.0.triangles())
    }

    /// Each drawable part's mesh and material, by name, in order: what
    /// [`reupload`] needs to match an earlier import.
    pub fn parts(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.nodes.iter().map(|node| (node.mesh_part.as_str(), node.material_part.as_deref()))
    }

    /// Each of its materials that a part draws with, as its file has it, in
    /// the order the parts first draw. Read before
    /// [`compress_textures`](Self::compress_textures), whose pictures it
    /// cannot give back.
    pub fn file_materials(&self) -> Vec<FileMaterial> {
        let picture = |index: Option<usize>| {
            let texture = &self.textures[index?];
            Some(match &texture.source {
                Some(path) => FilePicture::File(path.clone()),
                None if texture.format != TextureFormat::Rgba8 => return None,
                None => FilePicture::Inside {
                    name: texture.name.clone(),
                    width: texture.width,
                    height: texture.height,
                    rgba: texture.mips[0].clone(),
                },
            })
        };
        let mut found: Vec<FileMaterial> = Vec::new();
        for node in &self.nodes {
            let (Some(part), Some(index)) = (&node.material_part, node.material) else { continue };
            if found.iter().any(|material| material.part == *part) {
                continue;
            }
            let material = &self.materials[index];
            let maps = [material.base_color, material.metallic_roughness, material.normal, material.emissive];
            found.push(FileMaterial {
                part: part.clone(),
                name: material.name.clone(),
                desc: material.desc,
                maps: maps.map(picture),
            });
        }
        found
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
    /// RGBA8, as do those whose import settings say not to compress them.
    /// Leaves out every picture, for a build that draws nothing, as a
    /// dedicated server's: its meshes, for colliders, and its skeleton and
    /// clips stay.
    pub fn drop_pictures(&mut self) {
        self.textures.clear();
        for material in &mut self.materials {
            (material.base_color, material.metallic_roughness, material.normal, material.emissive) = (None, None, None, None);
        }
    }

    pub fn compress_textures(&mut self) {
        let normals: Vec<usize> = self.materials.iter().filter_map(|m| m.normal).collect();
        for (index, texture) in self.textures.iter_mut().enumerate() {
            let fits = texture.width % 4 == 0 && texture.height % 4 == 0;
            if texture.format != TextureFormat::Rgba8 || !fits || !texture.compress {
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

/// Reads and processes a model without touching the GPU, as the import
/// settings in its `.meta` say, or with the defaults when it has none.
pub fn prepare(path: &str, read: &mut ReadFile) -> Result<PreparedModel> {
    let meta = meta_path(std::path::Path::new(path)).to_string_lossy().into_owned();
    let settings = read(&meta).map(|bytes| ModelImport::from_meta(&String::from_utf8_lossy(&bytes)));
    prepare_with(path, read, &settings.unwrap_or_default())
}

/// [`prepare`], with the import settings given. A model's own LODs, as
/// parts named `Rock_LOD0` and on, become its meshes' LODs (see `model_lods`).
pub(crate) fn prepare_with(path: &str, read: &mut ReadFile, settings: &ModelImport) -> Result<PreparedModel> {
    let mut model = read_model(path, read, settings)?;
    bound_skins(&mut model);
    crate::model_lods::fold(&mut model, settings);
    Ok(model)
}

/// Bounds each skin's joints by the vertices each moves most, at full
/// detail (see [`Skin::bounds`](crate::Skin::bounds)).
fn bound_skins(model: &mut PreparedModel) {
    let PreparedModel { animation: Some(animation), nodes, meshes, .. } = model else { return };
    for skin in &mut animation.skins {
        skin.bounds = vec![(Vec3::INFINITY, Vec3::NEG_INFINITY); skin.joints.len()];
    }
    for node in nodes.iter() {
        let (Some(skin), Some((data, _))) = (node.skin, meshes.get(node.mesh)) else { continue };
        let Some(skin) = animation.skins.get_mut(skin) else { continue };
        for (vertex, weights) in data.vertices.iter().zip(&data.skin) {
            let heaviest = (0..4).max_by_key(|&i| weights.weights[i]).unwrap_or(0);
            let joint = usize::from(weights.joints[heaviest]);
            let (Some(bind), Some(bounds)) = (skin.inverse_binds.get(joint), skin.bounds.get_mut(joint)) else {
                continue;
            };
            let at = bind.transform_point3(Vec3::from(vertex.position));
            *bounds = (bounds.0.min(at), bounds.1.max(at));
        }
    }
}

fn read_model(path: &str, read: &mut ReadFile, settings: &ModelImport) -> Result<PreparedModel> {
    if crate::box_model::is_box_model(path) {
        return crate::box_model::prepare(path, read).with_context(|| format!("reading {path}"));
    }
    if fbx_import::is_fbx(path) {
        return fbx_import::prepare(path, read, settings).with_context(|| format!("reading {path}"));
    }
    let (document, buffers, images, compress, sources) =
        load(path, read).with_context(|| format!("reading {path}"))?;
    let mut preparer = Preparer {
        settings: *settings,
        size: mesh_tools::size(settings),
        buffers,
        images,
        compress,
        sources,
        model: PreparedModel {
            path: path.to_string(),
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            nodes: Vec::new(),
            animation: None,
            roles: Vec::new(),
        },
        meshes: HashMap::new(),
        textures: HashMap::new(),
        materials: HashMap::new(),
    };
    let scene = document.default_scene().or_else(|| document.scenes().next()).context("the file has no scenes")?;
    for node in scene.nodes() {
        preparer.visit(&node, Mat4::IDENTITY).with_context(|| format!("reading {path}"))?;
    }
    let size = preparer.size;
    preparer.model.animation =
        read_animation(&document, &scene, &preparer.buffers, size).with_context(|| format!("reading {path}"))?;
    Ok(preparer.model)
}

/// Uploads a prepared model and returns its drawable pieces.
pub fn upload(gpu: &Gpu, bindless: &mut Bindless, assets: &mut Assets, model: &PreparedModel) -> Result<ImportedModel> {
    let textures = upload_textures(gpu, bindless, assets, model)?;
    let materials = add_materials(gpu, assets, &model.materials, &textures)?;
    let mut meshes = Vec::with_capacity(model.meshes.len());
    for (data, lods) in &model.meshes {
        meshes.push(assets.add_mesh_with_lods(gpu, data, lods)?);
    }
    let animation = model.animation.clone().map(std::sync::Arc::new);
    Ok(imported(assets, &model.materials, &model.nodes, &meshes, &materials, (textures, animation, &model.roles)))
}

/// Puts a model prepared again, as with other import settings, behind the
/// handles `old` was given, so whatever draws it shows the change from the
/// next frame: each mesh's data replaced and each material's values written
/// over, matched by their parts. Its old textures are freed once frames in
/// flight are done with them. Fails, changing nothing, when its parts are
/// not the ones it had, as when its file changed: it is uploaded anew then.
pub fn reupload(
    gpu: &Gpu,
    bindless: &mut Bindless,
    assets: &mut Assets,
    old: &ImportedModel,
    model: &PreparedModel,
    frame: u64,
) -> Result<ImportedModel> {
    let same = |(node, was): (&PreparedNode, &ImportedNode)| {
        node.mesh_part == was.mesh_part && node.material_part == was.material_part
    };
    ensure!(
        old.nodes.len() == model.nodes.len() && model.nodes.iter().zip(&old.nodes).all(same),
        "{}'s parts are not the ones it had",
        model.path
    );
    let mut meshes = vec![None; model.meshes.len()];
    let mut materials = vec![None; model.materials.len()];
    for (node, was) in model.nodes.iter().zip(&old.nodes) {
        meshes[node.mesh] = Some(was.mesh);
        if let Some(material) = node.material {
            materials[material] = Some(was.material);
        }
    }
    let meshes: Vec<MeshHandle> = meshes.into_iter().collect::<Option<_>>().context("a mesh no part draws")?;
    let materials: Vec<MaterialHandle> =
        materials.into_iter().collect::<Option<_>>().context("a material no part uses")?;

    let textures = upload_textures(gpu, bindless, assets, model)?;
    for (&handle, (data, lods)) in meshes.iter().zip(&model.meshes) {
        assets.replace_mesh(gpu, handle, data, lods, frame)?;
    }
    for (&handle, material) in materials.iter().zip(&model.materials) {
        assets.set_material(gpu, handle, &material_desc(material, &textures))?;
    }
    old.textures.iter().for_each(|&texture| assets.free_texture(texture, frame));
    let animation = model.animation.clone().map(std::sync::Arc::new);
    Ok(imported(assets, &model.materials, &model.nodes, &meshes, &materials, (textures, animation, &model.roles)))
}

fn upload_textures(
    gpu: &Gpu,
    bindless: &mut Bindless,
    assets: &mut Assets,
    model: &PreparedModel,
) -> Result<Vec<TextureId>> {
    let mut textures = Vec::with_capacity(model.textures.len());
    for texture in &model.textures {
        let id = match texture.format {
            TextureFormat::Rgba8 => {
                let source = TextureSource {
                    name: texture.label(),
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
                texture.label(),
                (texture.width, texture.height),
                format.vulkan(texture.srgb),
                &texture.mips,
            )?,
        };
        textures.push(id);
    }
    Ok(textures)
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
    let animation = model.animation.clone().map(std::sync::Arc::new);
    Ok(imported(assets, &model.materials, &model.nodes, &meshes, &materials, (textures, animation, &model.roles)))
}

fn add_materials(
    gpu: &Gpu,
    assets: &mut Assets,
    materials: &[PreparedMaterial],
    textures: &[TextureId],
) -> Result<Vec<MaterialHandle>> {
    materials.iter().map(|material| assets.add_material(gpu, &material_desc(material, textures))).collect()
}

/// A material's values, with its textures as uploaded.
fn material_desc(material: &PreparedMaterial, textures: &[TextureId]) -> MaterialDesc {
    let texture = |index: Option<usize>| index.map(|i| textures[i]);
    MaterialDesc {
        base_color_texture: texture(material.base_color),
        metallic_roughness_texture: texture(material.metallic_roughness),
        normal_texture: texture(material.normal),
        emissive_texture: texture(material.emissive),
        ..material.desc
    }
}

fn imported(
    assets: &Assets,
    prepared: &[PreparedMaterial],
    nodes: &[PreparedNode],
    meshes: &[MeshHandle],
    materials: &[MaterialHandle],
    (textures, animation, roles): (Vec<TextureId>, Option<std::sync::Arc<ModelAnimation>>, &[crate::BoneRole]),
) -> ImportedModel {
    let mut names: Vec<(String, String)> = Vec::new();
    for node in nodes {
        let (Some(part), Some(index)) = (&node.material_part, node.material) else { continue };
        if !names.iter().any(|(known, _)| known == part) {
            names.push((part.clone(), prepared.get(index).map_or_else(String::new, |m| m.name.clone())));
        }
    }
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
            skin: node.skin,
        })
        .collect();
    ImportedModel { nodes, textures, materials: names, animation, roles: roles.to_vec() }
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
    settings: ModelImport,
    /// The metres one unit of the file becomes.
    size: f32,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
    /// Whether each image is compressed for a build.
    compress: Vec<bool>,
    /// The project path of each image's file, for those not inside the model's.
    sources: Vec<Option<String>>,
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
            // A skinned mesh is where its skin's bones put it, in the model's own space, whatever its node says.
            let skin = node.skin().map(|skin| skin.index());
            let placed = if skin.is_some() { Mat4::IDENTITY } else { world };
            let (scale, rotation, position) = placed.to_scale_rotation_translation();
            let position = position * self.size;
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
                    skin,
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
        let colors: Vec<[u8; 4]> = reader.read_colors(0).map(|c| c.into_rgba_u8().collect()).unwrap_or_default();
        let indices: Vec<u32> = match reader.read_indices() {
            Some(indices) => indices.into_u32().collect(),
            None => (0..positions.len() as u32).collect(),
        };
        let joints: Vec<[u16; 4]> = reader.read_joints(0).map(|j| j.into_u16().collect()).unwrap_or_default();
        let weights: Vec<[f32; 4]> = reader.read_weights(0).map(|w| w.into_f32().collect()).unwrap_or_default();
        let skin = match joints.len() == positions.len() && weights.len() == positions.len() {
            true => joints.iter().zip(&weights).map(|(&j, &w)| SkinVertex::new(j, w)).collect(),
            false => Vec::new(),
        };

        let vertices = positions
            .iter()
            .enumerate()
            .map(|(i, &position)| Vertex {
                position,
                normal: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                uv: uvs.get(i).copied().unwrap_or_default(),
                tangent: tangents.get(i).copied().unwrap_or([1.0, 0.0, 0.0, 1.0]),
                wind: colors.get(i).copied().unwrap_or(crate::mesh::NO_WIND_MASKS),
            })
            .collect();
        let mut data = MeshData { vertices, indices, skin };
        data.mirror_x();
        let file = FromFile { normals: !normals.is_empty(), tangents: !tangents.is_empty(), colors: !colors.is_empty() };
        self.model.meshes.push(mesh_tools::finish(data, &self.settings, self.size, file));
        let index = self.model.meshes.len() - 1;
        self.meshes.insert(key, index);
        Ok(index)
    }

    fn material(&mut self, material: gltf::Material) -> Result<Option<usize>> {
        let Some(key) = material.index() else { return Ok(None) };
        if let Some(&index) = self.materials.get(&key) {
            return Ok(Some(index));
        }
        let name = material.name().unwrap_or_default();
        if !self.settings.materials {
            self.model.materials.push(PreparedMaterial::plain(name));
            self.materials.insert(key, self.model.materials.len() - 1);
            return Ok(Some(self.model.materials.len() - 1));
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
        let name = name.to_string();
        self.model.materials.push(PreparedMaterial { name, desc, base_color, metallic_roughness, normal, emissive });
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
            name: texture.name().or(texture.source().name()).unwrap_or_default().to_string(),
            width: image.width,
            height: image.height,
            srgb,
            format: TextureFormat::Rgba8,
            mips: vec![rgba],
            compress: self.compress[image_index],
            source: self.sources[image_index].clone(),
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
    // A box model's pixels are in its own file.
    if crate::box_model::is_box_model(path) {
        return Ok(Vec::new());
    }
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

/// The file, its buffers, its images at the size their settings say,
/// whether each is compressed for a build, and each one's file.
type Loaded = (gltf::Document, Vec<gltf::buffer::Data>, Vec<gltf::image::Data>, Vec<bool>, Vec<Option<String>>);

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
    let (mut images, mut compress, mut sources) = (Vec::new(), Vec::new(), Vec::new());
    for image in document.images() {
        sources.push(match image.source() {
            gltf::image::Source::Uri { uri, .. } if !uri.starts_with("data:") => Some(resolve(path, uri)),
            _ => None,
        });
        let (encoded, settings) = match image.source() {
            gltf::image::Source::View { view, .. } => {
                let buffer = &buffers[view.buffer().index()].0;
                (buffer[view.offset()..view.offset() + view.length()].to_vec(), TextureImport::default())
            }
            gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => {
                (read_uri(path, uri, read)?, TextureImport::default())
            }
            gltf::image::Source::Uri { uri, .. } => {
                (read_uri(path, uri, read)?, texture_settings(&resolve(path, uri), read))
            }
        };
        let decoded =
            image::load_from_memory(&encoded).with_context(|| format!("decoding image {}", image.index()))?.to_rgba8();
        let decoded = sized(decoded, &settings);
        let (width, height) = decoded.dimensions();
        images.push(gltf::image::Data {
            pixels: decoded.into_raw(),
            format: gltf::image::Format::R8G8B8A8,
            width,
            height,
        });
        compress.push(settings.compress);
    }
    Ok((document, buffers, images, compress, sources))
}

/// The import settings of the picture file at `path`, from its `.meta`, or
/// the defaults.
pub(crate) fn texture_settings(path: &str, read: &mut ReadFile) -> TextureImport {
    let meta = meta_path(std::path::Path::new(path)).to_string_lossy().into_owned();
    read(&meta).map(|bytes| TextureImport::from_meta(&String::from_utf8_lossy(&bytes))).unwrap_or_default()
}

/// A picture shrunk to the largest size its settings allow, keeping its
/// shape, its green turned over if they say so.
pub(crate) fn sized(mut picture: image::RgbaImage, settings: &TextureImport) -> image::RgbaImage {
    settings.apply_flip(&mut picture);
    let (width, height) = settings.size(picture.width(), picture.height());
    if (width, height) == picture.dimensions() {
        return picture;
    }
    image::imageops::resize(&picture, width, height, image::imageops::FilterType::Triangle)
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

/// A model's skeleton, skins and clips, in Voxear's space (mirrored on X,
/// and `size` metres a unit): `None` for a model with neither skins nor
/// animations. The skeleton holds every bone of the skins and every node
/// the animations move, with the nodes above them, so each keeps its
/// parent; in the order they are met down the scene, parents first.
fn read_animation(
    document: &gltf::Document,
    scene: &gltf::Scene,
    buffers: &[gltf::buffer::Data],
    size: f32,
) -> Result<Option<ModelAnimation>> {
    if document.skins().len() == 0 && document.animations().len() == 0 {
        return Ok(None);
    }
    // Each node's parent, down from the scene's.
    let mut parents: HashMap<usize, usize> = HashMap::new();
    let mut order = Vec::new();
    let mut stack: Vec<gltf::Node> = scene.nodes().collect();
    stack.reverse();
    while let Some(node) = stack.pop() {
        order.push(node.index());
        let children: Vec<gltf::Node> = node.children().collect();
        for child in children.into_iter().rev() {
            parents.insert(child.index(), node.index());
            stack.push(child);
        }
    }
    let mut wanted: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let skin_joints = document.skins().flat_map(|skin| skin.joints().map(|joint| joint.index()).collect::<Vec<_>>());
    let moved = document.animations().flat_map(|a| a.channels().map(|c| c.target().node().index()).collect::<Vec<_>>());
    for node in skin_joints.chain(moved) {
        let mut at = Some(node);
        while let Some(index) = at.filter(|index| wanted.insert(*index)) {
            at = parents.get(&index).copied();
        }
    }
    let nodes: Vec<gltf::Node> = document.nodes().collect();
    let mut bone_of: HashMap<usize, usize> = HashMap::new();
    let mut bones = Vec::new();
    for index in order.into_iter().filter(|index| wanted.contains(index)) {
        let node = &nodes[index];
        let (translation, rotation, scale) = node.transform().decomposed();
        bone_of.insert(index, bones.len());
        bones.push(Bone {
            name: node.name().map_or_else(|| format!("bone{index}"), str::to_string),
            parent: parents.get(&index).and_then(|parent| bone_of.get(parent).copied()),
            rest: BonePose {
                translation: mirror(translation) * size,
                rotation: mirror_rotation(rotation),
                scale: Vec3::from(scale),
            },
        });
    }
    // Into Voxear's space and back, for the bind matrices: mirrored on X and sized.
    let into = Mat4::from_scale(Vec3::new(-size, size, size));
    let back = into.inverse();
    let mut skins = Vec::new();
    for skin in document.skins() {
        let reader = skin.reader(|buffer| Some(&buffers[buffer.index()]));
        let joints: Vec<usize> = skin.joints().map(|joint| bone_of[&joint.index()]).collect();
        let binds: Vec<Mat4> = match reader.read_inverse_bind_matrices() {
            Some(matrices) => matrices.map(|m| into * Mat4::from_cols_array_2d(&m) * back).collect(),
            None => vec![Mat4::IDENTITY; joints.len()],
        };
        ensure!(binds.len() == joints.len(), "a skin has {} joints but {} bind matrices", joints.len(), binds.len());
        skins.push(Skin { joints, inverse_binds: binds, bounds: Vec::new() });
    }
    let skeleton = Skeleton { bones };
    let mut clips = Vec::new();
    for animation in document.animations() {
        clips.push(read_clip(&animation, buffers, &skeleton, &bone_of, size)?);
    }
    Ok(Some(ModelAnimation { skeleton, skins, clips }))
}

/// One animation, every bone's pose sampled at `CLIP_RATE` from its start to
/// its last key; a bone it does not move stays at rest.
fn read_clip(
    animation: &gltf::Animation,
    buffers: &[gltf::buffer::Data],
    skeleton: &Skeleton,
    bone_of: &HashMap<usize, usize>,
    size: f32,
) -> Result<Clip> {
    use gltf::animation::{Interpolation, util::ReadOutputs};
    let name = animation.name().map_or_else(|| format!("clip{}", animation.index()), str::to_string);
    let mut end = 0.0f32;
    let mut tracks = Vec::new();
    for channel in animation.channels() {
        let Some(&bone) = bone_of.get(&channel.target().node().index()) else { continue };
        let reader = channel.reader(|buffer| Some(&buffers[buffer.index()]));
        let times: Vec<f32> = reader.read_inputs().context("a channel has no times")?.collect();
        let values: Vec<[f32; 4]> = match reader.read_outputs().context("a channel has no values")? {
            ReadOutputs::Translations(t) => t.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
            ReadOutputs::Rotations(r) => r.into_f32().collect(),
            ReadOutputs::Scales(s) => s.map(|[x, y, z]| [x, y, z, 0.0]).collect(),
            ReadOutputs::MorphTargetWeights(_) => continue,
        };
        // A cubic spline's keys come with a tangent either side; the value is the middle of each three.
        let values: Vec<[f32; 4]> = match channel.sampler().interpolation() {
            Interpolation::CubicSpline => values.chunks(3).filter_map(|key| key.get(1).copied()).collect(),
            _ => values,
        };
        let step = channel.sampler().interpolation() == Interpolation::Step;
        end = end.max(times.last().copied().unwrap_or(0.0));
        tracks.push((bone, channel.target().property(), times, values, step));
    }
    let frames = (end * CLIP_RATE).round() as usize + 1;
    let rest = skeleton.rest();
    let mut poses = Vec::with_capacity(frames * rest.len());
    for _ in 0..frames {
        poses.extend_from_slice(&rest);
    }
    for (bone, property, times, values, step) in tracks {
        for frame in 0..frames {
            let value = key_at(&times, &values, frame as f32 / CLIP_RATE, step, property);
            let pose = &mut poses[frame * rest.len() + bone];
            match property {
                gltf::animation::Property::Translation => {
                    pose.translation = mirror([value[0], value[1], value[2]]) * size;
                }
                gltf::animation::Property::Rotation => pose.rotation = mirror_rotation(value),
                gltf::animation::Property::Scale => pose.scale = Vec3::new(value[0], value[1], value[2]),
                gltf::animation::Property::MorphTargetWeights => {}
            }
        }
    }
    Ok(Clip { name, frames, poses })
}

/// A track's value at `time`: between the keys either side, or the earlier
/// for a stepped one; rotations along the shorter way round.
fn key_at(times: &[f32], values: &[[f32; 4]], time: f32, step: bool, property: gltf::animation::Property) -> [f32; 4] {
    let Some(&last) = values.last() else { return [0.0, 0.0, 0.0, 1.0] };
    let after = times.partition_point(|&t| t <= time);
    if after == 0 {
        return values[0];
    }
    if after >= times.len().min(values.len()) {
        return last;
    }
    let (a, b) = (values[after - 1], values[after]);
    if step {
        return a;
    }
    let t = ((time - times[after - 1]) / (times[after] - times[after - 1]).max(1e-6)).clamp(0.0, 1.0);
    if property == gltf::animation::Property::Rotation {
        let q = Quat::from_array(a).normalize().slerp(Quat::from_array(b).normalize(), t);
        return q.to_array();
    }
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cooked::tests::quad;

    #[test]
    fn a_pictures_meta_shrinks_it_and_can_keep_it_uncompressed() {
        // The quad, its picture in a file of its own beside it.
        let gltf = String::from_utf8(quad()).unwrap();
        let start = gltf.find("\"images\": [{ \"uri\": \"").unwrap() + "\"images\": [{ \"uri\": \"".len();
        let end = start + gltf[start..].find('"').unwrap();
        let gltf = format!("{}Bricks.png{}", &gltf[..start], &gltf[end..]);
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(64, 32, image::Rgba([200, 90, 40, 255]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        let files = |settings: &'static str| {
            let (gltf, png) = (gltf.clone(), png.clone());
            move |file: &str| -> Result<Vec<u8>> {
                Ok(match file {
                    "Models/Bricks.png" => png.clone(),
                    "Models/Bricks.png.meta" => settings.as_bytes().to_vec(),
                    "Models/Quad.gltf" => gltf.clone().into_bytes(),
                    _ => anyhow::bail!("no {file}"),
                })
            }
        };

        let mut model =
            prepare("Models/Quad.gltf", &mut files("[texture]\nmax_size = 16\ncompress = false\n")).unwrap();
        let texture = &model.textures[0];
        assert_eq!((texture.width, texture.height), (16, 8), "shrunk to 16 pixels across, keeping its shape");
        model.compress_textures();
        assert_eq!(model.textures[0].format, TextureFormat::Rgba8, "shipped as it is");

        let mut model = prepare("Models/Quad.gltf", &mut files("")).unwrap();
        assert_eq!((model.textures[0].width, model.textures[0].height), (64, 32), "without settings, its own size");
        model.compress_textures();
        assert_eq!(model.textures[0].format, TextureFormat::Bc7, "and compressed");
    }

    #[test]
    fn a_models_own_lods_are_read_as_one_parts_lods() {
        // The quad twice, as its own LOD 0 and LOD 1, as an artist's file names them.
        let gltf = String::from_utf8(quad()).unwrap();
        let nodes = r#""nodes": [{ "name": "Quad_LOD0", "mesh": 0 }, { "name": "Quad_LOD1", "mesh": 0 }]"#;
        let scenes = r#""scenes": [{ "nodes": [0, 1] }]"#;
        let gltf = gltf
            .replace(r#""nodes": [{ "name": "Quad", "mesh": 0, "translation": [1, 2, 3] }]"#, nodes)
            .replace(r#""scenes": [{ "nodes": [0] }]"#, scenes);
        let model = prepare("Models/Quad.gltf", &mut |_| Ok(gltf.clone().into_bytes())).unwrap();
        let names: Vec<&str> = model.nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Quad"], "one part, by the name before the suffix");
        assert_eq!(model.meshes[0].1.len(), 2, "with the file's two LODs");
    }

    #[test]
    fn a_models_meta_sizes_it_and_can_leave_out_its_materials_and_lods() {
        let gltf = quad();
        let meta = "guid = \"x\"\n[model]\nunits = \"centimetres\"\nscale = 2.0\nmaterials = false\nlods = false\n";
        let mut read = |file: &str| Ok(if file.ends_with(".meta") { meta.as_bytes().to_vec() } else { gltf.clone() });
        let model = prepare("Models/Quad.gltf", &mut read).unwrap();

        let (mesh, lods) = &model.meshes[0];
        let reach = mesh.vertices.iter().map(|v| v.position[1]).fold(0.0, f32::max);
        assert!((reach - 0.02).abs() < 1e-6, "a centimetre, twice over: {reach}");
        assert!(model.nodes[0].position.abs_diff_eq(Vec3::new(-0.02, 0.04, 0.06), 1e-6), "where it sits, sized too");
        assert_eq!(lods.len(), 1, "no LODs");
        assert!(mesh.vertices.iter().all(|v| v.normal[2].abs() == 1.0), "normals the file lacks, made from the face");

        assert!(model.textures.is_empty(), "no textures read for materials left out");
        assert_eq!(model.materials[0].desc.roughness, MaterialDesc::default().roughness);
        assert_eq!(model.nodes[0].material_part.as_deref(), Some("material0"), "the part keeps its material");

        let mut without =
            |file: &str| if file.ends_with(".meta") { anyhow::bail!("no .meta") } else { Ok(gltf.clone()) };
        let plain = prepare("Models/Quad.gltf", &mut without).unwrap();
        assert_eq!(plain.materials[0].desc.roughness, 0.25, "without a .meta, the defaults");
    }
}
