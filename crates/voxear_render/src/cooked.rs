//! Cooked models: prepared models written out by the content build, so a
//! game loads one with a single read and an upload. No glTF parsing, image
//! decoding or LOD building happens at runtime.
//!
//! The layout is little-endian: `VXMODEL`, the format version, then meshes,
//! textures, materials and nodes, each a count followed by its items, and
//! last the [`Payload`]: every vertex, index, meshlet and texture mip the GPU needs,
//! in one block that ships GDeflate-compressed and is decompressed by the
//! GPU where it can. Meshes and textures say where their data sits in it.
//! Changing the layout bumps [`COOKED_VERSION`]; the build cache keys on it,
//! so an old cook is never read as a new one.

use anyhow::{Context, Result, bail, ensure};
use voxear_core::math::{Mat4, Quat, Vec3};

use crate::{
    assets::MaterialDesc,
    gltf_import::{PreparedMaterial, PreparedModel, PreparedNode, TextureFormat},
    gpu::GpuMeshlet,
    mesh::{MeshTriangles, Vertex},
    meshlets::MeshMeshlets,
    payload::Payload,
    skeleton::{Bone, BonePose, Clip, ModelAnimation, Skeleton, Skin},
    texture_codec,
};

/// 3: the GPU data moved into one payload, GDeflate-compressed in tiles, and
/// every texture carries its whole mip chain. 4: each LOD's indices are in
/// meshlet order, and its meshlets ship in the payload. 5: coarse LODs past
/// a mesh's seams, by sloppy simplification (see `lod`). 8: skinned meshes'
/// skin data in the payload, nodes' skins, and the model's skeleton, skins
/// and clips.
pub const COOKED_VERSION: u32 = 10;
const MAGIC: &[u8; 8] = b"VXMODEL\0";
const NONE: u32 = u32::MAX;
/// Where each part of the payload starts, a multiple of a compressed
/// texture's 16-byte blocks, as copies into images require.
const ALIGN: usize = 16;

/// A model as it ships: what it holds, and its GPU data in a payload.
pub struct CookedModel {
    pub(crate) path: String,
    pub(crate) meshes: Vec<CookedMesh>,
    pub(crate) textures: Vec<CookedTexture>,
    pub(crate) materials: Vec<PreparedMaterial>,
    pub(crate) nodes: Vec<PreparedNode>,
    pub(crate) animation: Option<ModelAnimation>,
    pub(crate) payload: Payload,
}

pub(crate) struct CookedMesh {
    /// Where the vertices start in the payload, and how many there are.
    pub vertices: (u64, u32),
    /// Where the indices start; every LOD's follow one another.
    pub indices: u64,
    /// Each LOD's error, index count and meshlet count, finest first.
    pub lods: Vec<(f32, u32, u32)>,
    /// Where every LOD's meshlets, their vertices and their triangles start
    /// in the payload, one LOD's after another, and how many of each.
    pub meshlets: (u64, u32),
    pub meshlet_vertices: (u64, u32),
    pub meshlet_triangles: (u64, u32),
    /// Bounding sphere, worked out at cook time since the CPU may never see
    /// the vertices.
    pub center: Vec3,
    pub radius: f32,
    /// Where a skinned mesh's skin data starts in the payload, one for each
    /// vertex; none for a rigid one.
    pub skin: (u64, u32),
}

pub(crate) struct CookedTexture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub format: TextureFormat,
    /// Each mip's place in the payload, level 0 first.
    pub mips: Vec<(u64, u32)>,
}

impl PreparedModel {
    /// Packs the model to ship: textures get their whole mip chain, and all
    /// GPU data goes into one payload, compressed where that helps. Call
    /// [`compress_textures`](Self::compress_textures) first to ship BC7 and BC5.
    pub fn cook(&self) -> Result<CookedModel> {
        let mut data = Vec::new();
        let mut place = |bytes: &[u8]| {
            data.resize(data.len().next_multiple_of(ALIGN), 0);
            let offset = data.len() as u64;
            data.extend_from_slice(bytes);
            offset
        };
        let mut meshes = Vec::with_capacity(self.meshes.len());
        for (mesh, lods) in &self.meshes {
            let vertices = (place(bytemuck::cast_slice(&mesh.vertices)), mesh.vertices.len() as u32);
            let indices: Vec<u32> = lods.iter().flat_map(|lod| lod.indices.iter().copied()).collect();
            let indices = place(bytemuck::cast_slice(&indices));
            let gathered = MeshMeshlets::gather(lods.iter().map(|lod| &lod.meshlets));
            let meshlets = (place(bytemuck::cast_slice(&gathered.meshlets)), gathered.meshlets.len() as u32);
            let meshlet_vertices = (place(bytemuck::cast_slice(&gathered.vertices)), gathered.vertices.len() as u32);
            let meshlet_triangles = (place(bytemuck::cast_slice(&gathered.triangles)), gathered.triangles.len() as u32);
            let counts = lods.iter().zip(&gathered.counts);
            let lods = counts.map(|(lod, &meshlets)| (lod.error, lod.indices.len() as u32, meshlets)).collect();
            let (center, radius) = mesh.bounds();
            let skin = (place(bytemuck::cast_slice(&mesh.skin)), mesh.skin.len() as u32);
            meshes.push(CookedMesh {
                skin,
                vertices,
                indices,
                lods,
                meshlets,
                meshlet_vertices,
                meshlet_triangles,
                center,
                radius,
            });
        }
        let mut textures = Vec::with_capacity(self.textures.len());
        for texture in &self.textures {
            // RGBA8 textures leave their mips to the GPU when uploaded from
            // source; a cooked one carries them, so uploads only copy.
            let levels = if texture.format == TextureFormat::Rgba8 && texture.mips.len() == 1 {
                let chain = texture_codec::mip_chain(&texture.mips[0], texture.width, texture.height, texture.srgb);
                chain.into_iter().map(|(_, _, pixels)| pixels).collect()
            } else {
                texture.mips.clone()
            };
            let mips = levels.iter().map(|level| (place(level), level.len() as u32)).collect();
            textures.push(CookedTexture {
                name: texture.name.clone(),
                width: texture.width,
                height: texture.height,
                srgb: texture.srgb,
                format: texture.format,
                mips,
            });
        }
        Ok(CookedModel {
            path: self.path.clone(),
            meshes,
            textures,
            materials: self.materials.clone(),
            nodes: self.nodes.clone(),
            animation: self.animation.clone(),
            payload: Payload::compress(data)?,
        })
    }
}

impl CookedModel {
    /// Its skeleton, skins and clips, for a model made to animate.
    pub fn animation(&self) -> Option<&ModelAnimation> {
        self.animation.as_ref()
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// The triangles of a part's mesh, by its name, or of the first part's
    /// for none, for a Mesh collider: its finest LOD, read out of the GPU
    /// data, which is decompressed here to do it.
    pub fn triangles(&self, part: Option<&str>) -> Result<Option<MeshTriangles>> {
        let node = match part {
            Some(part) => self.nodes.iter().find(|node| node.mesh_part == part),
            None => self.nodes.first(),
        };
        let Some(mesh) = node.and_then(|node| self.meshes.get(node.mesh)) else { return Ok(None) };
        let bytes = self.payload.unpack()?;
        let (points, indices) = finest_triangles(&bytes, mesh)?;
        Ok(Some(MeshTriangles { points, indices }))
    }

    /// Whether the GPU data is still compressed.
    pub fn is_compressed(&self) -> bool {
        matches!(self.payload, Payload::Tiled { .. })
    }

    /// Decompresses the GPU data on this thread, for GPUs that cannot. A
    /// loading thread does this so the frame does not have to.
    pub fn decompress(&mut self) -> Result<()> {
        self.payload.make_raw().with_context(|| format!("decompressing {}", self.path))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer(MAGIC.to_vec());
        w.u32(COOKED_VERSION);
        w.u32(self.meshes.len() as u32);
        for mesh in &self.meshes {
            w.u64(mesh.vertices.0);
            w.u32(mesh.vertices.1);
            w.u64(mesh.indices);
            w.f32s(&mesh.center.to_array());
            w.f32(mesh.radius);
            w.u32(mesh.lods.len() as u32);
            for &(error, count, meshlets) in &mesh.lods {
                w.f32(error);
                w.u32(count);
                w.u32(meshlets);
            }
            for (offset, count) in [mesh.meshlets, mesh.meshlet_vertices, mesh.meshlet_triangles, mesh.skin] {
                w.u64(offset);
                w.u32(count);
            }
        }
        w.u32(self.textures.len() as u32);
        for texture in &self.textures {
            w.str(&texture.name);
            w.u32(texture.width);
            w.u32(texture.height);
            w.u32(u32::from(texture.srgb));
            w.u32(texture.format as u32);
            w.u32(texture.mips.len() as u32);
            for &(offset, len) in &texture.mips {
                w.u64(offset);
                w.u32(len);
            }
        }
        w.u32(self.materials.len() as u32);
        for material in &self.materials {
            let desc = &material.desc;
            w.f32s(&desc.base_color);
            w.f32s(&[desc.metallic, desc.roughness]);
            w.f32s(&desc.emissive);
            w.f32(desc.normal_scale);
            w.option(desc.alpha_cutoff.map(f32::to_bits));
            for texture in [material.base_color, material.metallic_roughness, material.normal, material.emissive] {
                w.option(texture.map(|t| t as u32));
            }
            w.str(&material.name);
        }
        w.u32(self.nodes.len() as u32);
        for node in &self.nodes {
            w.str(&node.name);
            w.f32s(&node.position.to_array());
            w.f32s(&node.rotation.to_array());
            w.f32s(&node.scale.to_array());
            w.u32(node.mesh as u32);
            w.option(node.material.map(|m| m as u32));
            w.str(&node.mesh_part);
            match &node.material_part {
                Some(part) => {
                    w.u32(1);
                    w.str(part);
                }
                None => w.u32(0),
            }
            w.option(node.skin.map(|skin| skin as u32));
        }
        w.animation(self.animation.as_ref());
        match &self.payload {
            Payload::Raw(bytes) => {
                w.u32(0);
                w.u64(bytes.len() as u64);
                w.bytes(bytes);
            }
            Payload::Tiled { size, tiles, data } => {
                w.u32(1);
                w.u64(*size);
                w.u32(tiles.len() as u32);
                for &(offset, len) in tiles {
                    w.u32(offset);
                    w.u32(len);
                }
                w.u64(data.len() as u64);
                w.bytes(data);
            }
        }
        w.0
    }

    /// Reads a cooked model; `path` is the model's project path.
    pub fn from_bytes(path: &str, bytes: &[u8]) -> Result<Self> {
        read(path, bytes).with_context(|| format!("reading the cooked {path}"))
    }
}

fn read(path: &str, bytes: &[u8]) -> Result<CookedModel> {
    let mut r = Reader { bytes, at: 0 };
    ensure!(r.take(8)? == MAGIC, "not a cooked model");
    let version = r.u32()?;
    ensure!(version == COOKED_VERSION, "cooked with format {version}, not {COOKED_VERSION}; build the content again");

    let mut meshes = Vec::new();
    for _ in 0..r.u32()? {
        let vertices = (r.u64()?, r.u32()?);
        let indices = r.u64()?;
        let center = Vec3::from_array(r.f32s()?);
        let radius = r.f32()?;
        let mut lods = Vec::new();
        for _ in 0..r.u32()? {
            lods.push((r.f32()?, r.u32()?, r.u32()?));
        }
        let (meshlets, meshlet_vertices, meshlet_triangles) =
            ((r.u64()?, r.u32()?), (r.u64()?, r.u32()?), (r.u64()?, r.u32()?));
        let skin = (r.u64()?, r.u32()?);
        meshes.push(CookedMesh {
            skin,
            vertices,
            indices,
            lods,
            meshlets,
            meshlet_vertices,
            meshlet_triangles,
            center,
            radius,
        });
    }
    let mut textures = Vec::new();
    for _ in 0..r.u32()? {
        let name = r.str()?;
        let (width, height, srgb) = (r.u32()?, r.u32()?, r.u32()? != 0);
        let format = r.u32()?;
        let format = TextureFormat::from_u32(format).with_context(|| format!("unknown texture format {format}"))?;
        let mut mips = Vec::new();
        for _ in 0..r.u32()? {
            mips.push((r.u64()?, r.u32()?));
        }
        ensure!(!mips.is_empty(), "texture {name} has no pixels");
        textures.push(CookedTexture { name, width, height, srgb, format, mips });
    }
    let mut materials = Vec::new();
    for _ in 0..r.u32()? {
        let [r0, g, b, a] = r.f32s()?;
        let [metallic, roughness] = r.f32s()?;
        let emissive = r.f32s()?;
        let normal_scale = r.f32()?;
        let alpha_cutoff = r.option()?.map(f32::from_bits);
        let desc = MaterialDesc {
            base_color: [r0, g, b, a],
            metallic,
            roughness,
            emissive,
            normal_scale,
            alpha_cutoff,
            ..MaterialDesc::default()
        };
        let mut texture = || r.option().map(|t| t.map(|t| t as usize));
        let (base_color, metallic_roughness, normal, emissive) = (texture()?, texture()?, texture()?, texture()?);
        let name = r.str()?;
        materials.push(PreparedMaterial { name, desc, base_color, metallic_roughness, normal, emissive });
    }
    let mut nodes = Vec::new();
    for _ in 0..r.u32()? {
        let name = r.str()?;
        let position = Vec3::from_array(r.f32s()?);
        let rotation = Quat::from_array(r.f32s()?);
        let scale = Vec3::from_array(r.f32s()?);
        let mesh = r.u32()? as usize;
        let material = r.option()?.map(|m| m as usize);
        let mesh_part = r.str()?;
        let material_part = if r.u32()? != 0 { Some(r.str()?) } else { None };
        let skin = r.option()?.map(|skin| skin as usize);
        ensure!(mesh < meshes.len(), "a node uses mesh {mesh}, which is not there");
        nodes.push(PreparedNode { name, position, rotation, scale, mesh, material, mesh_part, material_part, skin });
    }
    let animation = r.animation()?;
    if let Some(skin) = nodes.iter().filter_map(|node| node.skin).max() {
        let found = animation.as_ref().is_some_and(|a| skin < a.skins.len());
        ensure!(found, "a node uses skin {skin}, which is not there");
    }
    let payload = match r.u32()? {
        0 => {
            let len = r.u64()? as usize;
            Payload::Raw(r.take(len)?.to_vec())
        }
        1 => {
            let size = r.u64()?;
            let mut tiles = Vec::new();
            for _ in 0..r.u32()? {
                tiles.push((r.u32()?, r.u32()?));
            }
            let len = r.u64()? as usize;
            Payload::Tiled { size, tiles, data: r.take(len)?.to_vec() }
        }
        kind => bail!("unknown payload kind {kind}"),
    };
    ensure!(r.at == bytes.len(), "{} bytes follow the model", bytes.len() - r.at);
    let size = payload.size();
    let fits = |(offset, len): (u64, u64)| offset.checked_add(len).is_some_and(|end| end <= size);
    for mesh in &meshes {
        let index_count: u64 = mesh.lods.iter().map(|&(_, count, _)| u64::from(count)).sum();
        let meshlet_count: u64 = mesh.lods.iter().map(|&(_, _, count)| u64::from(count)).sum();
        ensure!(
            fits((mesh.vertices.0, u64::from(mesh.vertices.1) * size_of::<Vertex>() as u64)),
            "a mesh lies outside the payload"
        );
        ensure!(fits((mesh.indices, index_count * 4)), "a mesh's indices lie outside the payload");
        ensure!(meshlet_count == u64::from(mesh.meshlets.1), "a mesh's LODs do not add up to its meshlets");
        ensure!(
            fits((mesh.meshlets.0, u64::from(mesh.meshlets.1) * size_of::<GpuMeshlet>() as u64))
                && fits((mesh.meshlet_vertices.0, u64::from(mesh.meshlet_vertices.1) * 4))
                && fits((mesh.meshlet_triangles.0, u64::from(mesh.meshlet_triangles.1) * 4)),
            "a mesh's meshlets lie outside the payload"
        );
    }
    for mesh in &meshes {
        ensure!(
            mesh.skin.1 == 0 || mesh.skin.1 == mesh.vertices.1,
            "a mesh's skin data is not one for each vertex"
        );
        ensure!(fits((mesh.skin.0, u64::from(mesh.skin.1) * 16)), "a mesh's skin data lies outside the payload");
    }
    for texture in &textures {
        ensure!(
            texture.mips.iter().all(|&(o, l)| fits((o, u64::from(l)))),
            "texture {} lies outside the payload",
            texture.name
        );
    }
    Ok(CookedModel { path: path.to_string(), meshes, textures, materials, nodes, animation, payload })
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn f32s(&mut self, values: &[f32]) {
        values.iter().for_each(|&v| self.f32(v));
    }

    /// Raw bytes. Their count is written before them by the caller, as an
    /// item count or implied by the texture size.
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    fn str(&mut self, text: &str) {
        self.u32(text.len() as u32);
        self.bytes(text.as_bytes());
    }

    fn option(&mut self, value: Option<u32>) {
        self.u32(value.unwrap_or(NONE));
    }

    fn pose(&mut self, pose: &BonePose) {
        self.f32s(&pose.translation.to_array());
        self.f32s(&pose.rotation.to_array());
        self.f32s(&pose.scale.to_array());
    }

    /// The model's skeleton, skins and clips, after a flag saying it has them.
    fn animation(&mut self, animation: Option<&ModelAnimation>) {
        let Some(animation) = animation else { return self.u32(0) };
        self.u32(1);
        self.u32(animation.skeleton.bones.len() as u32);
        for bone in &animation.skeleton.bones {
            self.str(&bone.name);
            self.option(bone.parent.map(|p| p as u32));
            self.pose(&bone.rest);
        }
        self.u32(animation.skins.len() as u32);
        for skin in &animation.skins {
            self.u32(skin.joints.len() as u32);
            for (index, (&joint, bind)) in skin.joints.iter().zip(&skin.inverse_binds).enumerate() {
                self.u32(joint as u32);
                self.f32s(&bind.to_cols_array());
                let (least, most) = skin.bounds.get(index).copied().unwrap_or((Vec3::INFINITY, Vec3::NEG_INFINITY));
                self.f32s(&least.to_array());
                self.f32s(&most.to_array());
            }
        }
        self.u32(animation.clips.len() as u32);
        for clip in &animation.clips {
            self.str(&clip.name);
            self.u32(clip.frames as u32);
            clip.poses.iter().for_each(|pose| self.pose(pose));
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(count).filter(|&end| end <= self.bytes.len());
        let Some(end) = end else { bail!("the file ends early") };
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight bytes")))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().expect("four bytes")))
    }

    fn f32s<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut values = [0.0; N];
        for value in &mut values {
            *value = self.f32()?;
        }
        Ok(values)
    }

    fn str(&mut self) -> Result<String> {
        let len = self.u32()? as usize;
        Ok(String::from_utf8(self.take(len)?.to_vec())?)
    }

    fn option(&mut self) -> Result<Option<u32>> {
        Ok(Some(self.u32()?).filter(|&v| v != NONE))
    }

    fn pose(&mut self) -> Result<BonePose> {
        let translation = Vec3::from_array(self.f32s()?);
        let rotation = Quat::from_array(self.f32s()?);
        let scale = Vec3::from_array(self.f32s()?);
        Ok(BonePose { translation, rotation, scale })
    }

    fn animation(&mut self) -> Result<Option<ModelAnimation>> {
        if self.u32()? == 0 {
            return Ok(None);
        }
        let mut bones = Vec::new();
        for index in 0..self.u32()? as usize {
            let name = self.str()?;
            let parent = self.option()?.map(|p| p as usize);
            ensure!(parent.is_none_or(|p| p < index), "bone {name} comes before its parent");
            bones.push(Bone { name, parent, rest: self.pose()? });
        }
        let mut skins = Vec::new();
        for _ in 0..self.u32()? {
            let mut skin = Skin::default();
            for _ in 0..self.u32()? {
                let joint = self.u32()? as usize;
                ensure!(joint < bones.len(), "a skin's joint is bone {joint}, which is not there");
                skin.joints.push(joint);
                skin.inverse_binds.push(Mat4::from_cols_array(&self.f32s()?));
                skin.bounds.push((Vec3::from_array(self.f32s()?), Vec3::from_array(self.f32s()?)));
            }
            skins.push(skin);
        }
        let mut clips = Vec::new();
        for _ in 0..self.u32()? {
            let name = self.str()?;
            let frames = self.u32()? as usize;
            let count = frames.checked_mul(bones.len()).context("a clip too long to read")?;
            ensure!(count * 40 <= self.bytes.len() - self.at, "clip {name} runs past the file's end");
            let poses = (0..count).map(|_| self.pose()).collect::<Result<_>>()?;
            clips.push(Clip { name, frames, poses });
        }
        Ok(Some(ModelAnimation { skeleton: Skeleton { bones }, skins, clips }))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A two-triangle quad with a 2×1 texture, a material and named parts,
    /// all embedded so no files are needed.
    pub(crate) fn quad() -> Vec<u8> {
        let mut buffer = Vec::new();
        for value in [0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0] {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0u16, 1, 2, 0, 2, 3] {
            buffer.extend_from_slice(&index.to_le_bytes());
        }
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
        }
        let data = |bytes: &[u8], mime: &str| format!("data:{mime};base64,{}", base64::encode(bytes));
        format!(
            r#"{{
              "asset": {{ "version": "2.0" }},
              "buffers": [{{ "uri": "{}", "byteLength": {} }}],
              "bufferViews": [{{ "buffer": 0, "byteLength": 48 }}, {{ "buffer": 0, "byteOffset": 48, "byteLength": 12 }}],
              "accessors": [
                {{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0] }},
                {{ "bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR" }}
              ],
              "images": [{{ "uri": "{}" }}],
              "textures": [{{ "source": 0 }}],
              "materials": [{{ "name": "Painted",
                "pbrMetallicRoughness": {{ "baseColorTexture": {{ "index": 0 }}, "roughnessFactor": 0.25 }} }}],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0 }}, "indices": 1, "material": 0 }}] }}],
              "nodes": [{{ "name": "Quad", "mesh": 0, "translation": [1, 2, 3] }}],
              "scenes": [{{ "nodes": [0] }}]
            }}"#,
            data(&buffer, "application/octet-stream"),
            buffer.len(),
            data(&png, "image/png")
        )
        .into_bytes()
    }

    /// A bar two metres tall bound to two bones, its bottom to "Root" and
    /// its top to "Tip" a metre up, and a clip, "Bend", turning the tip a
    /// quarter turn about Z over a second. Its mesh's node sits five metres
    /// along X, which a skinned mesh's place ignores.
    pub(crate) fn bent_bar() -> Vec<u8> {
        let mut buffer = Vec::new();
        let mut put = |values: &[f32]| values.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
        put(&[-0.1, 0.0, 0.0, 0.1, 0.0, 0.0, 0.1, 2.0, 0.0, -0.1, 2.0, 0.0]);
        let mut rest = Vec::new();
        for index in [0u16, 1, 2, 0, 2, 3] {
            rest.extend_from_slice(&index.to_le_bytes());
        }
        for joints in [[0u16, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 0, 0]] {
            joints.iter().for_each(|j| rest.extend_from_slice(&j.to_le_bytes()));
        }
        buffer.extend(rest);
        let mut put = |values: &[f32]| values.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
        put(&[1.0, 0.0, 0.0, 0.0].repeat(4));
        put(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        put(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0]);
        put(&[0.0, 1.0]);
        let half = std::f32::consts::FRAC_PI_4;
        put(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, half.sin(), half.cos()]);
        let data = format!("data:application/octet-stream;base64,{}", base64::encode(&buffer));
        format!(
            r#"{{
              "asset": {{ "version": "2.0" }},
              "buffers": [{{ "uri": "{data}", "byteLength": {} }}],
              "bufferViews": [
                {{ "buffer": 0, "byteOffset": 0, "byteLength": 48 }},
                {{ "buffer": 0, "byteOffset": 48, "byteLength": 12 }},
                {{ "buffer": 0, "byteOffset": 60, "byteLength": 32 }},
                {{ "buffer": 0, "byteOffset": 92, "byteLength": 64 }},
                {{ "buffer": 0, "byteOffset": 156, "byteLength": 128 }},
                {{ "buffer": 0, "byteOffset": 284, "byteLength": 8 }},
                {{ "buffer": 0, "byteOffset": 292, "byteLength": 32 }}
              ],
              "accessors": [
                {{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-0.1, 0, 0], "max": [0.1, 2, 0] }},
                {{ "bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR" }},
                {{ "bufferView": 2, "componentType": 5123, "count": 4, "type": "VEC4" }},
                {{ "bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC4" }},
                {{ "bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4" }},
                {{ "bufferView": 5, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] }},
                {{ "bufferView": 6, "componentType": 5126, "count": 2, "type": "VEC4" }}
              ],
              "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0, "JOINTS_0": 2, "WEIGHTS_0": 3 }}, "indices": 1 }}] }}],
              "skins": [{{ "joints": [0, 1], "inverseBindMatrices": 4 }}],
              "animations": [{{ "name": "Bend",
                "samplers": [{{ "input": 5, "output": 6 }}],
                "channels": [{{ "sampler": 0, "target": {{ "node": 1, "path": "rotation" }} }}] }}],
              "nodes": [
                {{ "name": "Root", "children": [1] }},
                {{ "name": "Tip", "translation": [0, 1, 0] }},
                {{ "name": "Bar", "mesh": 0, "skin": 0, "translation": [5, 0, 0] }}
              ],
              "scenes": [{{ "nodes": [0, 2] }}]
            }}"#,
            buffer.len()
        )
        .into_bytes()
    }

    #[test]
    fn a_skinned_models_skeleton_skin_and_clip_are_read_and_cooked() {
        let gltf = bent_bar();
        let model = crate::gltf_import::prepare("Models/Bar.gltf", &mut |_| Ok(gltf.clone())).unwrap();
        let animation = model.animation.as_ref().expect("a skeleton and a clip");
        let names: Vec<_> = animation.skeleton.bones.iter().map(|b| (b.name.as_str(), b.parent)).collect();
        assert_eq!(names, [("Root", None), ("Tip", Some(0))]);
        assert_eq!(animation.skeleton.bones[1].rest.translation, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(animation.skins[0].joints, [0, 1]);
        assert_eq!(animation.skins[0].inverse_binds[1].w_axis.truncate(), Vec3::new(0.0, -1.0, 0.0));
        // Each joint's box round the corners it moves, the tip's in its own space, a metre up.
        let near = |a: Vec3, b: Vec3| (a - b).length() < 1e-5;
        let [(root_least, root_most), (tip_least, tip_most)] = animation.skins[0].bounds[..] else { panic!() };
        assert!(near(root_least, Vec3::new(-0.1, 0.0, 0.0)) && near(root_most, Vec3::new(0.1, 0.0, 0.0)));
        assert!(near(tip_least, Vec3::new(-0.1, 1.0, 0.0)) && near(tip_most, Vec3::new(0.1, 1.0, 0.0)));
        let node = &model.nodes[0];
        assert_eq!((node.skin, node.position), (Some(0), Vec3::ZERO), "placed by its bones, not its node");
        let (mesh, _) = &model.meshes[node.mesh];
        assert_eq!(mesh.skin.len(), mesh.vertices.len());
        for (vertex, skin) in mesh.vertices.iter().zip(&mesh.skin) {
            let joint = if vertex.position[1] > 1.0 { 1 } else { 0 };
            assert_eq!((skin.joints[0], skin.weights[0]), (joint, 65535));
        }
        let clip = &animation.clips[0];
        assert_eq!((clip.name.as_str(), clip.frames), ("Bend", 61));
        // Half way, an eighth of a turn; mirrored on X, the other way about Z.
        let half = clip.frame(30)[1].rotation;
        assert!((half.z + (std::f32::consts::PI / 8.0).sin()).abs() < 1e-3, "{half}");

        let bytes = model.cook().unwrap().to_bytes();
        let back = CookedModel::from_bytes("Models/Bar.gltf", &bytes).unwrap();
        assert_eq!(back.animation.as_ref(), Some(animation));
        assert_eq!(back.nodes[0].skin, Some(0));
        let payload = back.payload.unpack().unwrap();
        let (offset, count) = (back.meshes[0].skin.0 as usize, back.meshes[0].skin.1 as usize);
        assert_eq!(&payload[offset..offset + count * 16], bytemuck::cast_slice::<_, u8>(&mesh.skin));
        assert_eq!(back.to_bytes(), bytes);
    }

    /// Reads the model at `VOXEAR_TEST_MODEL`, when set, and reports its
    /// skeleton and clips: for checking a real file by hand.
    #[test]
    fn a_model_named_by_the_environment_reads_its_animation() {
        let Ok(path) = std::env::var("VOXEAR_TEST_MODEL") else { return };
        let model = crate::gltf_import::prepare(&path, &mut |file| Ok(std::fs::read(file)?)).unwrap();
        let animation = model.animation.as_ref().expect("an animated model");
        let skinned = model.meshes.iter().filter(|(mesh, _)| !mesh.skin.is_empty()).count();
        let (bones, skins) = (animation.skeleton.bones.len(), animation.skins.len());
        eprintln!("{bones} bones, {skins} skins, {skinned} skinned meshes");
        for clip in &animation.clips {
            eprintln!("  {} {:.2}s", clip.name, clip.duration());
        }
        let bytes = model.cook().unwrap().to_bytes();
        assert_eq!(CookedModel::from_bytes(&path, &bytes).unwrap().animation.as_ref(), Some(animation));
        eprintln!("cooked to {} KB", bytes.len() / 1024);
    }

    #[test]
    fn cooked_models_read_back_exactly() {
        let gltf = quad();
        let model = crate::gltf_import::prepare("Models/Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
        let bytes = model.cook().unwrap().to_bytes();
        let back = CookedModel::from_bytes("Models/Quad.gltf", &bytes).unwrap();

        assert_eq!(back.to_bytes(), bytes, "writing it again gives the same bytes");
        let payload = back.payload.unpack().unwrap();
        let mesh = &back.meshes[0];
        let (offset, count) = (mesh.vertices.0 as usize, mesh.vertices.1 as usize);
        let vertices: &[u8] = bytemuck::cast_slice(&model.meshes[0].0.vertices);
        assert_eq!(&payload[offset..offset + count * size_of::<Vertex>()], vertices);
        assert_eq!(mesh.lods[0].1, 6);
        let lod = &model.meshes[0].1[0];
        assert_eq!((mesh.lods[0].2, mesh.meshlets.1), (1, 1), "two triangles make one meshlet");
        let meshlets: &[u8] = bytemuck::cast_slice(&lod.meshlets.meshlets);
        assert_eq!(&payload[mesh.meshlets.0 as usize..][..meshlets.len()], meshlets);
        let indices: &[u8] = bytemuck::cast_slice(&lod.indices);
        assert_eq!(&payload[mesh.indices as usize..][..indices.len()], indices, "indices in meshlet order");
        let texture = &back.textures[0];
        assert_eq!(texture.mips.len(), 2, "the 1×1 mip was made at cook time");
        let (offset, len) = texture.mips[0];
        assert_eq!(&payload[offset as usize..][..len as usize], [255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(texture.mips.iter().all(|(offset, _)| offset % 16 == 0), "mips start on a block");
        assert_eq!(back.materials[0].desc.roughness, 0.25);
        assert_eq!(back.materials[0].base_color, Some(0));
        assert_eq!(back.materials[0].name, "Painted", "its name kept");
        let node = &back.nodes[0];
        assert_eq!((node.name.as_str(), node.mesh_part.as_str()), ("Quad", "mesh0.0"));
        assert_eq!(node.material_part.as_deref(), Some("material0"));
        assert_eq!(node.position, Vec3::new(-1.0, 2.0, 3.0), "mirrored to Unity's coordinates at import");

        assert!(CookedModel::from_bytes("x", &bytes[..bytes.len() - 1]).is_err(), "truncated files are caught");
        let mut newer = bytes.clone();
        newer[8] = 99;
        let error = format!("{:#}", CookedModel::from_bytes("x", &newer).err().expect("a newer version fails"));
        assert!(error.contains("build the content again"), "{error}");
    }

    #[test]
    fn large_models_ship_compressed_and_decompress_to_the_same_data() {
        let gltf = quad();
        let mut model = crate::gltf_import::prepare("Models/Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
        // A 256×256 gradient: big enough for several tiles, and smooth.
        let pixels: Vec<u8> = (0..256 * 256).flat_map(|i| [(i % 256) as u8, (i / 256) as u8, 128, 255]).collect();
        let texture = &mut model.textures[0];
        (texture.width, texture.height, texture.mips) = (256, 256, vec![pixels]);
        let cooked = model.cook().unwrap();
        assert!(cooked.is_compressed());

        let mut back = CookedModel::from_bytes("Models/Quad.gltf", &cooked.to_bytes()).unwrap();
        let triangles = back.triangles(Some("mesh0.0")).unwrap().unwrap();
        assert_eq!(triangles, model.triangles(None).unwrap(), "a collider's triangles, as imported");
        assert_eq!((triangles.points.len(), triangles.indices.len()), (4, 6));
        assert_eq!(back.triangles(Some("mesh9.0")).unwrap(), None);
        let expected = back.payload.unpack().unwrap().into_owned();
        back.decompress().unwrap();
        assert!(!back.is_compressed());
        assert_eq!(back.payload, crate::payload::Payload::Raw(expected));
    }
}

/// A cooked mesh's finest LOD, its points and three indices a triangle, read
/// out of its model's unpacked GPU data.
pub(crate) fn finest_triangles(bytes: &[u8], mesh: &CookedMesh) -> Result<(Vec<Vec3>, Vec<u32>)> {
    let ((start, count), stride) = (mesh.vertices, size_of::<Vertex>());
    let vertices = bytes.get(start as usize..start as usize + count as usize * stride).context("a mesh's vertices")?;
    let float = |bytes: &[u8]| f32::from_le_bytes(bytes.try_into().expect("four bytes"));
    let points = vertices.chunks_exact(stride).map(|v| Vec3::new(float(&v[0..4]), float(&v[4..8]), float(&v[8..12])));
    let finest = mesh.lods.first().map_or(0, |&(_, count, _)| count as usize);
    let indices = bytes.get(mesh.indices as usize..mesh.indices as usize + finest * 4).context("a mesh's indices")?;
    let indices = indices.as_chunks::<4>().0.iter().map(|bytes| u32::from_le_bytes(*bytes)).collect();
    Ok((points.collect(), indices))
}
