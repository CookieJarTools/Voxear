//! FBX import, through ufbx, into the same [`PreparedModel`] a glTF file
//! makes, so everything after reading is shared: LODs, upload, cooking and
//! previews. ufbx scales the file's units into metres in its vertices, and
//! the file's up is then stood up along +Y and mirrored on X, as glTF is, in
//! the vertices too, each node's transform following: a model made in
//! centimetres in a Z-up program comes in at its size, standing up, with no
//! turn left on its Objs to undo. Only the file's up is taken, not its right
//! and front, as files are written turned about their up for where they are
//! going. Import settings that name the file's units read it unconverted and
//! size it by them instead. Normals the file lacks are made, and UVs are
//! turned to count down from the top, as glTF's do. A mesh is split
//! into a part for each material its faces use (`mesh<mesh>.<part>`), each
//! drawn with that material (`material<index>`), meshes and materials
//! counted in the file's order.
//!
//! Materials are read through ufbx's PBR view of them, which it fills from
//! older Phong materials too: colour, metal, roughness and emission, and the
//! colour, normal and emission maps. A map replaces its colour, as it does in
//! the programs that write FBX. Textures come embedded in the file, or from
//! a file the FBX names: at its path relative to the FBX, or by its name in
//! the FBX's folder, as the absolute paths modelling programs write seldom
//! hold on another machine. A texture found nowhere, or in a format Voxear
//! cannot read, is left out with a warning, and the model loads without it.
//!
//! A skinned model's bones, and the nodes above them, make its skeleton, in
//! the same space as its meshes; each skin's mesh sits where its bones put
//! it, its vertices moved by their four heaviest bones. Each animation stack
//! is a clip, sampled at `CLIP_RATE` from its start to its end.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use voxear_assets::{ModelImport, TextureImport, Units};
use voxear_core::math::{Mat3, Mat4, Quat, Vec3};

use crate::{
    assets::MaterialDesc,
    gltf_import::{
        PreparedMaterial, PreparedModel, PreparedNode, PreparedTexture, ReadFile, TextureFormat, relative_to, sized,
        texture_settings,
    },
    mesh::{MeshData, Vertex},
    mesh_tools::{self, FromFile},
    skeleton::{Bone, BonePose, CLIP_RATE, Clip, ModelAnimation, Skeleton, Skin, SkinVertex},
};

/// Whether a project path names an FBX file.
pub(crate) fn is_fbx(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("fbx"))
}

/// Reads and processes an FBX file without touching the GPU, as `settings`
/// say.
pub(crate) fn prepare(path: &str, read: &mut ReadFile, settings: &ModelImport) -> Result<PreparedModel> {
    let scene = load(&read(path)?, true, settings.units == Units::File)?;
    let (size, space) = (mesh_tools::size(settings), space(&scene.settings.axes));
    let (animation, skins) = read_animation(&scene, space, size);
    let mut preparer = Preparer {
        settings: *settings,
        size,
        space,
        skins,
        model: PreparedModel {
            animation: None,
            path: path.to_string(),
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: Vec::new(),
            nodes: Vec::new(),
        },
        meshes: HashMap::new(),
        materials: HashMap::new(),
        textures: HashMap::new(),
    };
    for node in scene.nodes.iter() {
        if let Some(mesh) = &node.mesh {
            preparer.node(node, mesh, read);
        }
    }
    preparer.model.animation = animation;
    Ok(preparer.model)
}

/// The project paths of the texture files an FBX file may read, in every
/// place [`prepare`] looks for them, leaving out those it holds itself.
pub(crate) fn companion_files(path: &str, bytes: &[u8]) -> Result<Vec<String>> {
    let scene = load(bytes, false, true)?;
    let mut files = Vec::new();
    for texture in scene.textures.iter().filter(|t| t.type_ == ufbx::TextureType::File && t.content.is_empty()) {
        for file in texture_files(path, texture) {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    Ok(files)
}

/// The file as ufbx reads it, with its meshes and animation or without: in
/// metres, or in its own unit when not `in_metres`.
fn load(bytes: &[u8], geometry: bool, in_metres: bool) -> Result<ufbx::SceneRoot> {
    let opts = ufbx::LoadOpts {
        target_unit_meters: if in_metres { 1.0 } else { 0.0 },
        space_conversion: ufbx::SpaceConversion::ModifyGeometry,
        geometry_transform_handling: ufbx::GeometryTransformHandling::ModifyGeometry,
        generate_missing_normals: true,
        ignore_animation: !geometry,
        ignore_geometry: !geometry,
        ..Default::default()
    };
    ufbx::load_memory(bytes, opts).map_err(|error| anyhow!("{error:?}"))
}

/// What turns the file's space into Voxear's: its up stood up along +Y by
/// the shortest turn, then the mirror on X that glTF gets. The file's right
/// and front are not taken: files are written turned about their up for the
/// program they are for, and stand facing the right way once up is up. A
/// file that does not say which way is up is taken to be Y up.
fn space(axes: &ufbx::CoordinateAxes) -> Mat4 {
    // Where the file's X, Y and Z go: a quarter turn about X for Z up, about Z for X up, a half turn upside down.
    let (x, y, z) = (Vec3::X, Vec3::Y, Vec3::Z);
    let stand = match axes.up {
        ufbx::CoordinateAxis::PositiveZ => Mat3::from_cols(x, -z, y),
        ufbx::CoordinateAxis::NegativeZ => Mat3::from_cols(x, z, -y),
        ufbx::CoordinateAxis::PositiveX => Mat3::from_cols(y, -x, z),
        ufbx::CoordinateAxis::NegativeX => Mat3::from_cols(-y, x, z),
        ufbx::CoordinateAxis::NegativeY => Mat3::from_cols(x, -y, -z),
        ufbx::CoordinateAxis::PositiveY | ufbx::CoordinateAxis::Unknown => Mat3::IDENTITY,
    };
    Mat4::from_mat3(Mat3::from_diagonal(Vec3::new(-1.0, 1.0, 1.0)) * stand)
}

/// Turns a mesh from the file's space into Voxear's. A turn that mirrors
/// turns its triangles around too, as `MeshData::mirror_x` does.
fn into_space(data: &mut MeshData, space: Mat4) {
    let turn = Mat3::from_mat4(space);
    let mirrored = turn.determinant() < 0.0;
    for v in &mut data.vertices {
        v.position = (turn * Vec3::from(v.position)).into();
        v.normal = (turn * Vec3::from(v.normal)).into();
        let tangent = turn * Vec3::new(v.tangent[0], v.tangent[1], v.tangent[2]);
        // Mirroring flips the bitangent's handedness.
        let sign = if mirrored { -v.tangent[3] } else { v.tangent[3] };
        v.tangent = [tangent.x, tangent.y, tangent.z, sign];
    }
    if mirrored {
        for tri in data.indices.as_chunks_mut::<3>().0 {
            tri.swap(1, 2);
        }
    }
}

/// Where a texture the FBX names may be, as project paths: at its path
/// relative to the FBX, then by its name in the FBX's folder.
fn texture_files(path: &str, texture: &ufbx::Texture) -> Vec<String> {
    let mut files = Vec::new();
    let relative = texture.relative_filename.replace('\\', "/");
    if !relative.is_empty() {
        files.push(relative_to(path, &relative));
    }
    let names = [&texture.relative_filename, &texture.filename, &texture.absolute_filename];
    let name = names.into_iter().find_map(|file| file.rsplit(['/', '\\']).next().filter(|name| !name.is_empty()));
    if let Some(beside) = name.map(|name| relative_to(path, name)).filter(|beside| !files.contains(beside)) {
        files.push(beside);
    }
    files
}

struct Preparer {
    settings: ModelImport,
    /// The metres one unit, as the file is read, becomes.
    size: f32,
    /// From the file's space into Voxear's.
    space: Mat4,
    /// The model's skin for each of the file's skin deformers.
    skins: HashMap<u32, usize>,
    model: PreparedModel,
    /// What is already made, by the file's mesh and part, material, and
    /// texture and whether it is read as sRGB; a texture not found is `None`.
    meshes: HashMap<(u32, u32), usize>,
    materials: HashMap<u32, usize>,
    textures: HashMap<(u32, bool), Option<usize>>,
}

impl Preparer {
    /// A node's mesh, a piece for each material its faces use.
    fn node(&mut self, node: &ufbx::Node, mesh: &ufbx::Mesh, read: &mut ReadFile) {
        // In Voxear's space, as its mesh is: the same turn on both sides of the node's matrix.
        let world = self.space * matrix(&node.geometry_to_world) * self.space.inverse();
        // A skinned mesh is where its bones put it, in the model's own space.
        let skin = mesh.skin_deformers.first().and_then(|deformer| self.skins.get(&deformer.element.typed_id).copied());
        let world = if skin.is_some() { Mat4::IDENTITY } else { world };
        let (scale, rotation, position) = world.to_scale_rotation_translation();
        let position = position * self.size;
        let name = [&node.element.name, &mesh.element.name].into_iter().find(|name| !name.is_empty());
        let name = name.map_or("mesh", |name| name);
        for part in mesh.material_parts.iter().filter(|part| part.num_triangles > 0) {
            let material = mesh.materials.get(part.index as usize);
            let piece = PreparedNode {
                skin,
                name: name.to_string(),
                position,
                rotation,
                scale,
                mesh: self.mesh(mesh, part),
                material: material.map(|material| self.material(material, read)),
                mesh_part: format!("mesh{}.{}", mesh.element.typed_id, part.index),
                material_part: material.map(|material| format!("material{}", material.element.typed_id)),
            };
            self.model.nodes.push(piece);
        }
    }

    /// The faces of a mesh that use one material, as triangles, each corner
    /// a vertex of its own unless another is the same in every way.
    fn mesh(&mut self, mesh: &ufbx::Mesh, part: &ufbx::MeshPart) -> usize {
        let key = (mesh.element.typed_id, part.index);
        if let Some(&index) = self.meshes.get(&key) {
            return index;
        }
        let mut corners = vec![0; mesh.max_face_triangles * 3];
        let (mut vertices, mut indices, mut seen) = (Vec::new(), Vec::new(), HashMap::new());
        let deformer =
            mesh.skin_deformers.first().filter(|deformer| self.skins.contains_key(&deformer.element.typed_id));
        let mut skin = Vec::new();
        for &face in part.face_indices.iter() {
            let triangles = ufbx::triangulate_face(&mut corners, mesh, mesh.faces[face as usize]) as usize;
            for &corner in &corners[..triangles * 3] {
                let vertex = vertex(mesh, corner as usize);
                // Corners alike but for their bones are vertices of their own.
                let point = deformer.map_or(u32::MAX, |_| mesh.vertex_indices[corner as usize]);
                let key = (bytemuck::cast::<Vertex, [u32; size_of::<Vertex>() / 4]>(vertex), point);
                let index = *seen.entry(key).or_insert_with(|| {
                    vertices.push(vertex);
                    if let Some(deformer) = deformer {
                        skin.push(skin_vertex(deformer, point as usize));
                    }
                    vertices.len() as u32 - 1
                });
                indices.push(index);
            }
        }
        let mut data = MeshData { vertices, indices, skin };
        into_space(&mut data, self.space);
        let file = FromFile {
            normals: mesh.vertex_normal.exists,
            tangents: mesh.vertex_tangent.exists && mesh.vertex_bitangent.exists,
            colors: mesh.vertex_color.exists,
        };
        self.model.meshes.push(mesh_tools::finish(data, &self.settings, self.size, file));
        let index = self.model.meshes.len() - 1;
        self.meshes.insert(key, index);
        index
    }

    fn material(&mut self, material: &ufbx::Material, read: &mut ReadFile) -> usize {
        let key = material.element.typed_id;
        if let Some(&index) = self.materials.get(&key) {
            return index;
        }
        let name = material.element.name.to_string();
        if !self.settings.materials {
            self.model.materials.push(PreparedMaterial::plain(&name));
            self.materials.insert(key, self.model.materials.len() - 1);
            return self.model.materials.len() - 1;
        }
        let pbr = &material.pbr;
        let base_color = self.texture(&pbr.base_color, true, read);
        let normal = self.texture(&pbr.normal_map, false, read);
        let emissive = self.texture(&pbr.emission_color, true, read);
        let base = colour(&pbr.base_color, base_color.is_some(), 1.0).map(|c| c * value(&pbr.base_factor, 1.0));
        let emission =
            colour(&pbr.emission_color, emissive.is_some(), 0.0).map(|c| c * value(&pbr.emission_factor, 1.0));
        let desc = MaterialDesc {
            base_color: [base[0], base[1], base[2], 1.0],
            metallic: value(&pbr.metalness, 0.0),
            roughness: value(&pbr.roughness, 0.5),
            emissive: emission,
            ..MaterialDesc::default()
        };
        let metallic_roughness = None;
        self.model.materials.push(PreparedMaterial { name, desc, base_color, metallic_roughness, normal, emissive });
        let index = self.model.materials.len() - 1;
        self.materials.insert(key, index);
        index
    }

    /// The texture a map shows, when it has one that can be read. A layered
    /// texture shows its first layer.
    fn texture(&mut self, map: &ufbx::MaterialMap, srgb: bool, read: &mut ReadFile) -> Option<usize> {
        let texture = map.texture.as_ref().filter(|_| map.texture_enabled)?;
        let texture: &ufbx::Texture = match texture.type_ {
            ufbx::TextureType::File => texture,
            _ => texture.file_textures.first()?,
        };
        let key = (texture.element.typed_id, srgb);
        if let Some(&found) = self.textures.get(&key) {
            return found;
        }
        let found = self.read_texture(texture, srgb, read);
        self.textures.insert(key, found);
        found
    }

    fn read_texture(&mut self, texture: &ufbx::Texture, srgb: bool, read: &mut ReadFile) -> Option<usize> {
        let path = &self.model.path;
        let (encoded, settings, source) = match texture.content.is_empty() {
            false => (texture.content.to_vec(), TextureImport::default(), None),
            true => {
                let files = texture_files(path, texture);
                let Some((file, bytes)) = files.iter().find_map(|file| Some((file, read(file).ok()?))) else {
                    tracing::warn!("{path}: no texture {} (looked for {})", &*texture.filename, files.join(", "));
                    return None;
                };
                (bytes, texture_settings(file, read), Some(file.clone()))
            }
        };
        let image = match image::load_from_memory(&encoded) {
            Ok(image) => sized(image.to_rgba8(), &settings),
            Err(error) => {
                tracing::warn!("{path}: could not read the texture {}: {error}", &*texture.filename);
                return None;
            }
        };
        let (width, height) = image.dimensions();
        let name = [&texture.element.name, &texture.filename].into_iter().find(|name| !name.is_empty());
        self.model.textures.push(PreparedTexture {
            name: name.map_or("", |name| name).to_string(),
            width,
            height,
            srgb,
            format: TextureFormat::Rgba8,
            mips: vec![image.into_raw()],
            compress: settings.compress,
            source,
        });
        Some(self.model.textures.len() - 1)
    }
}

/// A model's skeleton, skins and clips, in Voxear's space as its meshes are
/// (`space`, and `size` metres a unit), and the skin made of each of the
/// file's skin deformers: `None` for a file without bones. The skeleton
/// holds every bone the skins use and every node the file calls a bone,
/// with the nodes above them, parents first.
fn read_animation(scene: &ufbx::Scene, space: Mat4, size: f32) -> (Option<ModelAnimation>, HashMap<u32, usize>) {
    let mut wanted = std::collections::HashSet::new();
    let used = scene.skin_deformers.iter().flat_map(|skin| skin.clusters.iter().filter_map(|c| c.bone_node.as_deref()));
    let named = scene.nodes.iter().filter(|node| node.bone.is_some()).map(|node| &**node);
    for node in used.chain(named) {
        let mut at = Some(node);
        while let Some(node) = at.filter(|node| !node.is_root && wanted.insert(node.element.typed_id)) {
            at = node.parent.as_deref();
        }
    }
    if wanted.is_empty() {
        return (None, HashMap::new());
    }
    // Down from the scene's root, so each parent comes before its children.
    let mut nodes: Vec<&ufbx::Node> = Vec::new();
    let mut stack: Vec<&ufbx::Node> = scene.root_node.children.iter().rev().map(|node| &**node).collect();
    while let Some(node) = stack.pop() {
        if wanted.contains(&node.element.typed_id) {
            nodes.push(node);
        }
        stack.extend(node.children.iter().rev().map(|node| &**node));
    }
    let bone_of: HashMap<u32, usize> =
        nodes.iter().enumerate().map(|(index, node)| (node.element.typed_id, index)).collect();
    let pose = |transform: &ufbx::Transform| pose_of(transform, space, size);
    let bones = nodes.iter().enumerate().map(|(index, node)| Bone {
        name: if node.element.name.is_empty() { format!("bone{index}") } else { node.element.name.to_string() },
        parent: node.parent.as_ref().and_then(|parent| bone_of.get(&parent.element.typed_id).copied()),
        rest: pose(&node.local_transform),
    });
    let skeleton = Skeleton { bones: bones.collect() };
    // Into Voxear's space and back, for the bind matrices.
    let into = Mat4::from_scale(Vec3::splat(size)) * space;
    let back = into.inverse();
    let (mut skins, mut skin_of) = (Vec::new(), HashMap::new());
    for deformer in scene.skin_deformers.iter() {
        let bone =
            |cluster: &ufbx::SkinCluster| cluster.bone_node.as_ref().and_then(|n| bone_of.get(&n.element.typed_id));
        let joints = deformer.clusters.iter().map(|cluster| bone(cluster).copied().unwrap_or(0)).collect();
        let inverse_binds = deformer.clusters.iter().map(|cluster| into * matrix(&cluster.geometry_to_bone) * back);
        skin_of.insert(deformer.element.typed_id, skins.len());
        skins.push(Skin { joints, inverse_binds: inverse_binds.collect(), ..Skin::default() });
    }
    let rest = skeleton.rest();
    let clips = scene.anim_stacks.iter().map(|stack| {
        let frames = ((stack.time_end - stack.time_begin).max(0.0) * f64::from(CLIP_RATE)).round() as usize + 1;
        let mut poses = Vec::with_capacity(frames * rest.len());
        for frame in 0..frames {
            let time = stack.time_begin + frame as f64 / f64::from(CLIP_RATE);
            poses.extend(nodes.iter().map(|node| pose(&ufbx::evaluate_transform(&stack.anim, node, time))));
        }
        Clip { name: clip_name(&stack.element.name), frames, poses }
    });
    let clips = clips.collect();
    (Some(ModelAnimation { skeleton, skins, clips }), skin_of)
}

/// A node's place under its parent in Voxear's space: the same turn on both
/// sides, and sized.
fn pose_of(transform: &ufbx::Transform, space: Mat4, size: f32) -> BonePose {
    let (t, r, s) = (transform.translation, transform.rotation, transform.scale);
    let rotation = Quat::from_xyzw(r.x as f32, r.y as f32, r.z as f32, r.w as f32).normalize();
    let scale = Vec3::new(s.x as f32, s.y as f32, s.z as f32);
    let local = Mat4::from_scale_rotation_translation(scale, rotation, Vec3::new(t.x as f32, t.y as f32, t.z as f32));
    let (scale, rotation, translation) = (space * local * space.inverse()).to_scale_rotation_translation();
    BonePose { translation: translation * size, rotation, scale }
}

/// A stack's name as a clip's: without the take or armature a program puts
/// before it, as `Armature|Walk_Loop`.
fn clip_name(stack: &str) -> String {
    stack.rsplit('|').next().filter(|name| !name.is_empty()).unwrap_or(stack).to_string()
}

/// A vertex's four heaviest bones, of its skin's joints.
fn skin_vertex(deformer: &ufbx::SkinDeformer, point: usize) -> SkinVertex {
    let (mut joints, mut weights) = ([0u16; 4], [0.0f32; 4]);
    let Some(found) = deformer.vertices.get(point) else { return SkinVertex::new(joints, weights) };
    let begin = found.weight_begin as usize;
    let mut each: Vec<ufbx::SkinWeight> =
        deformer.weights.iter().skip(begin).take(found.num_weights as usize).copied().collect();
    each.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    for (at, weight) in each.into_iter().take(4).enumerate() {
        (joints[at], weights[at]) = (weight.cluster_index as u16, weight.weight as f32);
    }
    SkinVertex::new(joints, weights)
}

/// One corner of a mesh's faces, in glTF's terms: its UV counted down from
/// the top, and its tangent's sign for a bitangent along that UV.
fn vertex(mesh: &ufbx::Mesh, corner: usize) -> Vertex {
    let vec3 = |v: ufbx::Vec3| [v.x as f32, v.y as f32, v.z as f32];
    let position = vec3(mesh.vertex_position[corner]);
    let normal = if mesh.vertex_normal.exists { vec3(mesh.vertex_normal[corner]) } else { [0.0, 1.0, 0.0] };
    let uv = match mesh.vertex_uv.exists {
        true => [mesh.vertex_uv[corner].x as f32, 1.0 - mesh.vertex_uv[corner].y as f32],
        false => [0.0; 2],
    };
    let tangent = match mesh.vertex_tangent.exists && mesh.vertex_bitangent.exists {
        true => {
            let (t, b) =
                (Vec3::from(vec3(mesh.vertex_tangent[corner])), Vec3::from(vec3(mesh.vertex_bitangent[corner])));
            // The file's bitangent runs up its V, which now runs down.
            let sign = if Vec3::from(normal).cross(t).dot(b) < 0.0 { 1.0 } else { -1.0 };
            [t.x, t.y, t.z, sign]
        }
        false => [1.0, 0.0, 0.0, 1.0],
    };
    // Its colour, as wind masks if the settings read them so.
    let wind = match mesh.vertex_color.exists {
        true => {
            let c = mesh.vertex_color[corner];
            [c.x, c.y, c.z, c.w].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        }
        false => crate::mesh::NO_WIND_MASKS,
    };
    Vertex { position, normal, uv, tangent, wind }
}

/// A map's colour, or white when a texture replaces it, or `none` when it
/// has no value.
fn colour(map: &ufbx::MaterialMap, textured: bool, none: f32) -> [f32; 3] {
    let v = map.value_vec4;
    match (textured, map.has_value) {
        (true, _) => [1.0; 3],
        (false, true) => [v.x as f32, v.y as f32, v.z as f32],
        (false, false) => [none; 3],
    }
}

fn value(map: &ufbx::MaterialMap, none: f32) -> f32 {
    if map.has_value { map.value_vec4.x as f32 } else { none }
}

fn matrix(m: &ufbx::Matrix) -> Mat4 {
    let columns = [
        [m.m00, m.m10, m.m20, 0.0],
        [m.m01, m.m11, m.m21, 0.0],
        [m.m02, m.m12, m.m22, 0.0],
        [m.m03, m.m13, m.m23, 1.0],
    ];
    Mat4::from_cols_array_2d(&columns.map(|column| column.map(|n| n as f32)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor of two squares side by side, one red and one wood, as 3ds Max
    /// or Blender writes it: Z up, in centimetres, one metre along X, and the
    /// wood's picture named by the absolute path it had on the artist's
    /// machine.
    const FLOOR: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",1
	}
}
Objects:  {
	Geometry: 100, "Geometry::Floor", "Mesh" {
		Vertices: *18 {
			a: -50,-50,0,0,-50,0,0,50,0,-50,50,0,50,-50,0,50,50,0
		}
		PolygonVertexIndex: *8 {
			a: 0,1,2,-4,1,4,5,-3
		}
		GeometryVersion: 124
		LayerElementNormal: 0 {
			Version: 101
			Name: ""
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "Direct"
			Normals: *24 {
				a: 0,0,1,0,0,1,0,0,1,0,0,1,0,0,1,0,0,1,0,0,1,0,0,1
			}
		}
		LayerElementUV: 0 {
			Version: 101
			Name: "UVMap"
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *8 {
				a: 0,0,1,0,1,1,0,1
			}
			UVIndex: *8 {
				a: 0,1,2,3,0,1,2,3
			}
		}
		LayerElementMaterial: 0 {
			Version: 101
			Name: ""
			MappingInformationType: "ByPolygon"
			ReferenceInformationType: "IndexToDirect"
			Materials: *2 {
				a: 0,1
			}
		}
		Layer: 0 {
			Version: 100
			LayerElement:  {
				Type: "LayerElementNormal"
				TypedIndex: 0
			}
			LayerElement:  {
				Type: "LayerElementUV"
				TypedIndex: 0
			}
			LayerElement:  {
				Type: "LayerElementMaterial"
				TypedIndex: 0
			}
		}
	}
	Model: 200, "Model::Floor", "Mesh" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",100,0,0
		}
	}
	Material: 300, "Material::Red", "" {
		Version: 102
		ShadingModel: "phong"
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",1,0,0
		}
	}
	Material: 301, "Material::Wood", "" {
		Version: 102
		ShadingModel: "phong"
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",0.8,0.8,0.8
		}
	}
	Texture: 500, "Texture::Wood", "" {
		Type: "TextureVideoClip"
		FileName: "C:\Art\Textures\wood.png"
		RelativeFilename: "..\..\Art\Textures\wood.png"
	}
}
Connections:  {
	C: "OO",200,0
	C: "OO",100,200
	C: "OO",300,200
	C: "OO",301,200
	C: "OP",500,301, "DiffuseColor"
}
"#;

    fn png() -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 120, 40, 255]));
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    /// The floor read from `Models/Floor.fbx`, with `files` beside it.
    fn floor(text: &str, files: &[(&str, Vec<u8>)]) -> PreparedModel {
        let mut read = |file: &str| -> Result<Vec<u8>> {
            if file == "Models/Floor.fbx" {
                return Ok(text.as_bytes().to_vec());
            }
            files.iter().find(|(name, _)| *name == file).map(|(_, bytes)| bytes.clone()).ok_or(anyhow!("no {file}"))
        };
        prepare("Models/Floor.fbx", &mut read, &ModelImport::default()).unwrap()
    }

    #[test]
    fn a_z_up_model_in_centimetres_comes_in_standing_up_in_metres() {
        let model = floor(FLOOR, &[]);
        for node in &model.nodes {
            assert_eq!(node.position, Vec3::new(-1.0, 0.0, 0.0), "a metre along X, mirrored as glTF is");
            assert_eq!((node.rotation, node.scale), (Default::default(), Vec3::ONE), "and no turn left to undo");
        }
        let vertices: Vec<&Vertex> = model.meshes.iter().flat_map(|(mesh, _)| &mesh.vertices).collect();
        for vertex in &vertices {
            assert_eq!(vertex.position[1], 0.0, "lying flat: {vertex:?}");
            assert_eq!(vertex.normal, [0.0, 1.0, 0.0], "facing up");
        }
        let reach = |axis: usize| vertices.iter().map(|v| v.position[axis].abs()).fold(0.0, f32::max);
        assert_eq!((reach(0), reach(2)), (0.5, 0.5), "a metre across");
        // The file's front, its -Y, faces +Z, as a glTF model's front does.
        let front = vertices.iter().find(|v| v.uv == [0.0, 1.0]).unwrap();
        assert_eq!(front.position[2], 0.5, "{front:?}");
        // A triangle's corners turn about its normal, as a glTF model's do once mirrored and turned around.
        let (mesh, _) = &model.meshes[0];
        let corner = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
        let [a, b, c] = [mesh.indices[0], mesh.indices[1], mesh.indices[2]].map(corner);
        assert!((b - a).cross(c - a).y > 0.0, "wound as mirror_x winds them");
    }

    #[test]
    fn units_named_in_the_settings_replace_the_files() {
        let read = |file: &str| -> Result<Vec<u8>> {
            if file == "Models/Floor.fbx" { Ok(FLOOR.as_bytes().to_vec()) } else { Err(anyhow!("no {file}")) }
        };
        // The file is in centimetres; read as metres, it is a hundred times the size.
        let settings = ModelImport { units: Units::Metres, ..ModelImport::default() };
        let model = prepare("Models/Floor.fbx", &mut { read }, &settings).unwrap();
        assert_eq!(model.nodes[0].position, Vec3::new(-100.0, 0.0, 0.0));
        let reach = model.meshes[0].0.vertices.iter().map(|v| v.position[0].abs()).fold(0.0, f32::max);
        assert_eq!(reach, 50.0);
        let settings = ModelImport { scale: 0.5, ..ModelImport::default() };
        let model = prepare("Models/Floor.fbx", &mut { read }, &settings).unwrap();
        assert_eq!(model.nodes[0].position, Vec3::new(-0.5, 0.0, 0.0), "the file's unit, then the scale");
    }

    #[test]
    fn faces_split_by_material_and_textures_are_found_beside_the_file() {
        let model = floor(FLOOR, &[("Models/wood.png", png())]);
        let parts: Vec<(&str, Option<&str>)> =
            model.nodes.iter().map(|n| (n.mesh_part.as_str(), n.material_part.as_deref())).collect();
        assert_eq!(parts, [("mesh0.0", Some("material0")), ("mesh0.1", Some("material1"))]);
        assert!(model.nodes.iter().all(|node| node.name == "Floor"));
        let [red, wood] = [0, 1].map(|at| &model.materials[model.nodes[at].material.unwrap()]);
        assert_eq!((red.desc.base_color, red.base_color), ([1.0, 0.0, 0.0, 1.0], None), "a Phong colour");
        assert_eq!(wood.base_color, Some(0), "found by its name, as the path it gives was the artist's");
        assert_eq!(wood.desc.base_color, [1.0; 4], "the picture in place of the colour");
        assert_eq!((model.textures[0].width, model.textures[0].height), (2, 2));
        assert!(model.textures[0].srgb);
        let companions = companion_files("Models/Floor.fbx", FLOOR.as_bytes()).unwrap();
        assert_eq!(companions, ["Art/Textures/wood.png", "Models/wood.png"], "every place it is looked for");
    }

    #[test]
    fn a_texture_found_nowhere_is_left_out_and_one_inside_the_file_is_read() {
        let model = floor(FLOOR, &[]);
        let wood = &model.materials[model.nodes[1].material.unwrap()];
        assert_eq!((wood.base_color, wood.desc.base_color), (None, [0.8, 0.8, 0.8, 1.0]), "its own colour then");

        let video = format!(
            "\tVideo: 600, \"Video::Wood\", \"Clip\" {{\n\t\tType: \"Clip\"\n\t\tFileName: \"C:\\Art\\wood.png\"\n\
             \t\tContent: , \"{}\"\n\t}}\n}}\nConnections:  {{\n\tC: \"OO\",600,500\n",
            base64::encode(png())
        );
        let embedded = FLOOR.replacen("}\nConnections:  {\n", &video, 1);
        let model = floor(&embedded, &[]);
        let wood = &model.materials[model.nodes[1].material.unwrap()];
        assert_eq!(wood.base_color, Some(0), "read from inside the file");
        assert_eq!(companion_files("Models/Floor.fbx", embedded.as_bytes()).unwrap(), Vec::<String>::new());
    }

    /// Two bones, a root and hips 90 centimetres above it, in a file that
    /// is Z up and in centimetres, with one clip a second long moving the
    /// root a metre and a half to the front.
    const WALK: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXHeaderVersion: 1003
	FBXVersion: 7400
}
GlobalSettings:  {
	Version: 1000
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",1
	}
}
Objects:  {
	NodeAttribute: 11, "NodeAttribute::root", "LimbNode" {
		TypeFlags: "Skeleton"
	}
	NodeAttribute: 12, "NodeAttribute::hips", "LimbNode" {
		TypeFlags: "Skeleton"
	}
	Model: 1, "Model::root", "LimbNode" {
		Version: 232
	}
	Model: 2, "Model::hips", "LimbNode" {
		Version: 232
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",0,0,90
		}
	}
	AnimationStack: 20, "AnimStack::Armature|Walk", "" {
		Properties70:  {
			P: "LocalStart", "KTime", "Time", "",0
			P: "LocalStop", "KTime", "Time", "",46186158000
		}
	}
	AnimationLayer: 21, "AnimLayer::BaseLayer", "" {
	}
	AnimationCurveNode: 22, "AnimCurveNode::T", "" {
		Properties70:  {
			P: "d|X", "Number", "", "A",0
			P: "d|Y", "Number", "", "A",0
			P: "d|Z", "Number", "", "A",0
		}
	}
	AnimationCurve: 23, "AnimCurve::", "" {
		Default: 0
		KeyVer: 4009
		KeyTime: *2 {
			a: 0,46186158000
		}
		KeyValueFloat: *2 {
			a: 0,-150
		}
		KeyAttrFlags: *1 {
			a: 4
		}
		KeyAttrDataFloat: *4 {
			a: 0,0,0,0
		}
		KeyAttrRefCount: *1 {
			a: 2
		}
	}
}
Connections:  {
	C: "OO",1,0
	C: "OO",2,1
	C: "OO",11,1
	C: "OO",12,2
	C: "OO",21,20
	C: "OO",22,21
	C: "OP",22,1, "Lcl Translation"
	C: "OP",23,22, "d|Y"
}
"#;

    #[test]
    fn bones_and_a_clip_come_in_standing_up_in_metres() {
        let model = floor(WALK, &[]);
        let animation = model.animation.expect("its bones");
        let names: Vec<&str> = animation.skeleton.bones.iter().map(|bone| bone.name.as_str()).collect();
        assert_eq!(names, ["root", "hips"]);
        assert_eq!(animation.skeleton.bones[1].parent, Some(0));
        let hips = animation.skeleton.bones[1].rest.translation;
        assert!(hips.distance(Vec3::new(0.0, 0.9, 0.0)) < 1e-4, "90 cm up: {hips}");
        let clip = &animation.clips[0];
        assert_eq!((clip.name.as_str(), clip.frames), ("Walk", 61), "a second, named without its armature");
        let mut pose = animation.skeleton.rest();
        clip.sample(1.0, false, &mut pose);
        let root = pose[0].translation;
        assert!(root.distance(Vec3::new(0.0, 0.0, 1.5)) < 1e-3, "to the front, a metre and a half: {root}");
    }

    /// Reads the FBX at `VOXEAR_TEST_MODEL` and the same character as glTF
    /// at `VOXEAR_TEST_GLB`, when set, and checks they come in alike: the
    /// same bones and clips, standing in the same place, the skinned mesh
    /// as big, and the feet within two centimetres of the glTF's once each
    /// clip is two frames in: files hold a clip's first keys differently,
    /// and an FBX's keys are often thinned out where it was written.
    #[test]
    #[ignore]
    fn an_fbx_character_comes_in_as_its_gltf_does() {
        let (Ok(fbx), Ok(gltf)) = (std::env::var("VOXEAR_TEST_MODEL"), std::env::var("VOXEAR_TEST_GLB")) else {
            return;
        };
        let read = |path: &str| {
            let path = std::path::Path::new(path);
            let dir = path.parent().unwrap().to_path_buf();
            let mut read = move |file: &str| -> Result<Vec<u8>> { Ok(std::fs::read(dir.join(file))?) };
            let name = path.file_name().unwrap().to_str().unwrap();
            crate::gltf_import::prepare_with(name, &mut read, &ModelImport::default()).unwrap()
        };
        let (fbx, gltf) = (read(&fbx), read(&gltf));
        let (a, b) = (fbx.animation.as_ref().unwrap(), gltf.animation.as_ref().unwrap());
        let names = |animation: &ModelAnimation| {
            animation.skeleton.bones.iter().map(|bone| bone.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names(a), names(b));
        let clips =
            |animation: &ModelAnimation| animation.clips.iter().map(|c| (c.name.clone(), c.frames)).collect::<Vec<_>>();
        assert_eq!(clips(a), clips(b));
        let at = |animation: &ModelAnimation, pose: &[BonePose]| {
            animation.skeleton.model_matrices(pose).iter().map(|m| m.w_axis.truncate()).collect::<Vec<_>>()
        };
        let rest = at(a, &a.skeleton.rest()).into_iter().zip(at(b, &b.skeleton.rest()));
        assert!(rest.map(|(x, y)| x.distance(y)).fold(0.0, f32::max) < 1e-3, "the same rest pose");
        let (low, high) = (skinned_bounds(&fbx), skinned_bounds(&gltf));
        assert!(low.distance(high) < 0.01, "the skinned mesh as big: {low} and {high}");
        let feet: Vec<usize> = (0..a.skeleton.bones.len())
            .filter(|&i| {
                let name = a.skeleton.bones[i].name.to_lowercase();
                name.contains("foot") || name.contains("toe")
            })
            .collect();
        for (x, y) in a.clips.iter().zip(&b.clips) {
            for frame in 2..x.frames {
                let (mut p, mut q) = (a.skeleton.rest(), b.skeleton.rest());
                x.sample(frame as f32 / CLIP_RATE, false, &mut p);
                y.sample(frame as f32 / CLIP_RATE, false, &mut q);
                let (p, q) = (at(a, &p), at(b, &q));
                for &foot in &feet {
                    let off = p[foot].distance(q[foot]);
                    assert!(off < 0.02, "{} at frame {frame}: {} is {off} m off", x.name, a.skeleton.bones[foot].name);
                }
            }
        }
    }

    /// How far the skinned meshes reach at rest, corner to corner.
    fn skinned_bounds(model: &PreparedModel) -> Vec3 {
        let animation = model.animation.as_ref().unwrap();
        let bones = animation.skeleton.model_matrices(&animation.skeleton.rest());
        let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for node in model.nodes.iter().filter(|node| node.skin.is_some()) {
            let skin = &animation.skins[node.skin.unwrap()];
            let mesh = &model.meshes[node.mesh].0;
            for (vertex, weights) in mesh.vertices.iter().zip(&mesh.skin) {
                let mut moved = Vec3::ZERO;
                for (joint, weight) in weights.joints.iter().zip(weights.weights) {
                    let joint = *joint as usize;
                    let placed = bones[skin.joints[joint]] * skin.inverse_binds[joint];
                    moved += placed.transform_point3(Vec3::from(vertex.position)) * (f32::from(weight) / 65535.0);
                }
                (low, high) = (low.min(moved), high.max(moved));
            }
        }
        high - low
    }
}
