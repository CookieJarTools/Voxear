//! Cooked models: prepared models written out by the content build, so a
//! game loads one with a single read and an upload. No glTF parsing, image
//! decoding or LOD building happens at runtime.
//!
//! The layout is little-endian: `VXMODEL`, the format version, then meshes
//! (vertices and LODs), textures (RGBA8), materials and nodes, each a count
//! followed by its items. Changing the layout bumps [`COOKED_VERSION`]; the
//! build cache keys on it, so an old cook is never read as a new one.

use anyhow::{Context, Result, bail, ensure};
use voxear_core::math::{Quat, Vec3};

use crate::{
    assets::MaterialDesc,
    gltf_import::{PreparedMaterial, PreparedModel, PreparedNode, PreparedTexture, TextureFormat},
    lod::Lod,
    mesh::{MeshData, Vertex},
};

/// 2: textures carry a format and their mip chain.
pub const COOKED_VERSION: u32 = 2;
const MAGIC: &[u8; 8] = b"VXMODEL\0";
const NONE: u32 = u32::MAX;

impl PreparedModel {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer(MAGIC.to_vec());
        w.u32(COOKED_VERSION);
        w.u32(self.meshes.len() as u32);
        for (mesh, lods) in &self.meshes {
            w.u32(mesh.vertices.len() as u32);
            w.bytes(bytemuck::cast_slice(&mesh.vertices));
            w.u32(lods.len() as u32);
            for lod in lods {
                w.f32(lod.error);
                w.u32(lod.indices.len() as u32);
                w.bytes(bytemuck::cast_slice(&lod.indices));
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
            for level in &texture.mips {
                w.u32(level.len() as u32);
                w.bytes(level);
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
        w.0
    }

    /// Reads a cooked model; `path` is the model's project path.
    pub fn from_bytes(path: &str, bytes: &[u8]) -> Result<Self> {
        read(path, bytes).with_context(|| format!("reading the cooked {path}"))
    }
}

fn read(path: &str, bytes: &[u8]) -> Result<PreparedModel> {
    let mut r = Reader { bytes, at: 0 };
    ensure!(r.take(8)? == MAGIC, "not a cooked model");
    let version = r.u32()?;
    ensure!(version == COOKED_VERSION, "cooked with format {version}, not {COOKED_VERSION}; build the content again");

    let mut meshes = Vec::new();
    for _ in 0..r.u32()? {
        let vertices = r.pod::<Vertex>()?;
        let mut lods = Vec::new();
        for _ in 0..r.u32()? {
            let error = r.f32()?;
            lods.push(Lod { indices: r.pod::<u32>()?, error });
        }
        meshes.push((MeshData { vertices, indices: Vec::new() }, lods));
    }
    let mut textures = Vec::new();
    for _ in 0..r.u32()? {
        let name = r.str()?;
        let (width, height, srgb) = (r.u32()?, r.u32()?, r.u32()? != 0);
        let format = r.u32()?;
        let format = TextureFormat::from_u32(format).with_context(|| format!("unknown texture format {format}"))?;
        let mut mips = Vec::new();
        for _ in 0..r.u32()? {
            let len = r.u32()? as usize;
            mips.push(r.take(len)?.to_vec());
        }
        ensure!(!mips.is_empty(), "texture {name} has no pixels");
        textures.push(PreparedTexture { name, width, height, srgb, format, mips });
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
    ensure!(r.at == bytes.len(), "{} bytes follow the model", bytes.len() - r.at);
    Ok(PreparedModel { path: path.to_string(), meshes, textures, materials, nodes })
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, value: u32) {
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

    /// A count, then that many plain-data items. Copied, since the bytes in a
    /// bundle are not aligned for them.
    fn pod<T: bytemuck::Pod + Default>(&mut self) -> Result<Vec<T>> {
        let count = self.u32()? as usize;
        let bytes = self.take(count.checked_mul(size_of::<T>()).context("too many items")?)?;
        let mut items = vec![T::default(); count];
        bytemuck::cast_slice_mut::<T, u8>(&mut items).copy_from_slice(bytes);
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-triangle quad with a 2×1 texture, a material and named parts,
    /// all embedded so no files are needed.
    fn quad() -> Vec<u8> {
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
        let bytes = model.to_bytes();
        let back = PreparedModel::from_bytes("Models/Quad.gltf", &bytes).unwrap();

        assert_eq!(back.to_bytes(), bytes, "writing it again gives the same bytes");
        assert_eq!(back.meshes[0].0.vertices, model.meshes[0].0.vertices);
        assert_eq!(back.meshes[0].1[0].indices.len(), 6);
        assert_eq!(back.textures[0].mips, [vec![255, 0, 0, 255, 0, 0, 255, 255]]);
        assert_eq!(back.materials[0].desc.roughness, 0.25);
        assert_eq!(back.materials[0].base_color, Some(0));
        let node = &back.nodes[0];
        assert_eq!((node.name.as_str(), node.mesh_part.as_str()), ("Quad", "mesh0.0"));
        assert_eq!(node.material_part.as_deref(), Some("material0"));
        assert_eq!(node.position, Vec3::new(-1.0, 2.0, 3.0), "mirrored to Unity's coordinates at import");

        assert!(PreparedModel::from_bytes("x", &bytes[..bytes.len() - 1]).is_err(), "truncated files are caught");
        let mut newer = bytes.clone();
        newer[8] = 99;
        let error = format!("{:#}", PreparedModel::from_bytes("x", &newer).err().expect("a newer version fails"));
        assert!(error.contains("build the content again"), "{error}");
    }
}
