//! Cooked models: prepared models written out by the content build, so a
//! game loads one with a single read and an upload. No glTF parsing, image
//! decoding or LOD building happens at runtime.
//!
//! The layout is little-endian: `VXMODEL`, the format version, then meshes,
//! textures, materials and nodes, each a count followed by its items, and
//! last the [`Payload`]: every vertex, index and texture mip the GPU needs,
//! in one block that ships GDeflate-compressed and is decompressed by the
//! GPU where it can. Meshes and textures say where their data sits in it.
//! Changing the layout bumps [`COOKED_VERSION`]; the build cache keys on it,
//! so an old cook is never read as a new one.

use anyhow::{Context, Result, bail, ensure};
use voxear_core::math::{Quat, Vec3};

use crate::{
    assets::MaterialDesc,
    gltf_import::{PreparedMaterial, PreparedModel, PreparedNode, TextureFormat},
    mesh::Vertex,
    payload::Payload,
    texture_codec,
};

/// 3: the GPU data moved into one payload, GDeflate-compressed in tiles, and
/// every texture carries its whole mip chain.
pub const COOKED_VERSION: u32 = 3;
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
    pub(crate) payload: Payload,
}

pub(crate) struct CookedMesh {
    /// Where the vertices start in the payload, and how many there are.
    pub vertices: (u64, u32),
    /// Where the indices start; every LOD's follow one another.
    pub indices: u64,
    /// Each LOD's error and index count, finest first.
    pub lods: Vec<(f32, u32)>,
    /// Bounding sphere, worked out at cook time since the CPU may never see
    /// the vertices.
    pub center: Vec3,
    pub radius: f32,
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
            let lods = lods.iter().map(|lod| (lod.error, lod.indices.len() as u32)).collect();
            let (center, radius) = mesh.bounds();
            meshes.push(CookedMesh { vertices, indices, lods, center, radius });
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
            payload: Payload::compress(data)?,
        })
    }
}

impl CookedModel {
    pub fn path(&self) -> &str {
        &self.path
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
            for &(error, count) in &mesh.lods {
                w.f32(error);
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
        }
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
            lods.push((r.f32()?, r.u32()?));
        }
        meshes.push(CookedMesh { vertices, indices, lods, center, radius });
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
        materials.push(PreparedMaterial { desc, base_color, metallic_roughness, normal, emissive });
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
        ensure!(mesh < meshes.len(), "a node uses mesh {mesh}, which is not there");
        nodes.push(PreparedNode { name, position, rotation, scale, mesh, material, mesh_part, material_part });
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
        let index_count: u64 = mesh.lods.iter().map(|&(_, count)| u64::from(count)).sum();
        ensure!(
            fits((mesh.vertices.0, u64::from(mesh.vertices.1) * size_of::<Vertex>() as u64)),
            "a mesh lies outside the payload"
        );
        ensure!(fits((mesh.indices, index_count * 4)), "a mesh's indices lie outside the payload");
    }
    for texture in &textures {
        ensure!(
            texture.mips.iter().all(|&(o, l)| fits((o, u64::from(l)))),
            "texture {} lies outside the payload",
            texture.name
        );
    }
    Ok(CookedModel { path: path.to_string(), meshes, textures, materials, nodes, payload })
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
              "materials": [{{ "pbrMetallicRoughness": {{ "baseColorTexture": {{ "index": 0 }}, "roughnessFactor": 0.25 }} }}],
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
        let texture = &back.textures[0];
        assert_eq!(texture.mips.len(), 2, "the 1×1 mip was made at cook time");
        let (offset, len) = texture.mips[0];
        assert_eq!(&payload[offset as usize..][..len as usize], [255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(texture.mips.iter().all(|(offset, _)| offset % 16 == 0), "mips start on a block");
        assert_eq!(back.materials[0].desc.roughness, 0.25);
        assert_eq!(back.materials[0].base_color, Some(0));
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
        let expected = back.payload.unpack().unwrap().into_owned();
        back.decompress().unwrap();
        assert!(!back.is_compressed());
        assert_eq!(back.payload, crate::payload::Payload::Raw(expected));
    }
}
