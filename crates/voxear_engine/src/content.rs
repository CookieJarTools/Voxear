//! Turns asset references from scene files into GPU meshes and materials,
//! importing each source asset once.
//!
//! In a project this reads source files directly, so pressing play never
//! needs a build. Shipped games will read the bundles the content build
//! makes instead, through the same calls.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetDb, AssetGuid, AssetKind, AssetRef};
use voxear_render::{ImportedNode, MaterialDesc, MaterialHandle, MeshHandle, Primitive, Renderer};

use crate::prefab::{self, PREFAB_FORMAT, PrefabCache, PrefabFile, PrefabId, Template};

/// A PBR material as written in `.material` files and inline in scenes.
/// Colours are sRGB, as picked in a colour picker.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialData {
    #[serde(default = "default_color")]
    pub color: [f32; 3],
    #[serde(default)]
    pub metallic: f32,
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    #[serde(default, skip_serializing_if = "is_black")]
    pub emissive: [f32; 3],
}

fn default_color() -> [f32; 3] {
    [0.8; 3]
}

fn default_roughness() -> f32 {
    0.5
}

fn is_black(color: &[f32; 3]) -> bool {
    *color == [0.0; 3]
}

impl MaterialData {
    fn desc(&self) -> MaterialDesc {
        let [r, g, b] = linear(self.color);
        MaterialDesc {
            base_color: [r, g, b, 1.0],
            metallic: self.metallic,
            roughness: self.roughness,
            emissive: self.emissive,
            ..MaterialDesc::default()
        }
    }

    /// Identical inline materials share one GPU material.
    fn key(&self) -> [u32; 8] {
        let [r, g, b] = self.color.map(f32::to_bits);
        let [er, eg, eb] = self.emissive.map(f32::to_bits);
        [r, g, b, self.metallic.to_bits(), self.roughness.to_bits(), er, eg, eb]
    }
}

/// Where a material came from, so scenes save it back the way it was written.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialSource {
    Asset(AssetRef),
    Inline(MaterialData),
}

pub struct Content {
    db: AssetDb,
    models: HashMap<AssetGuid, Vec<ImportedNode>>,
    meshes: HashMap<AssetRef, MeshHandle>,
    mesh_refs: HashMap<MeshHandle, AssetRef>,
    asset_materials: HashMap<AssetRef, MaterialHandle>,
    inline_materials: HashMap<[u32; 8], MaterialHandle>,
    material_sources: HashMap<MaterialHandle, MaterialSource>,
    prefabs: PrefabCache,
}

impl Content {
    /// Scans the project's assets, creating `.meta` files for new ones.
    pub fn open(root: &Path) -> Result<Self> {
        let (db, report) = AssetDb::scan(root)?;
        for path in &report.created {
            tracing::info!("new asset: {path}");
        }
        for path in &report.reassigned {
            tracing::warn!("{path} had the same GUID as another asset (copied with its .meta?); it has a new one");
        }
        Ok(Self {
            db,
            models: HashMap::new(),
            meshes: HashMap::new(),
            mesh_refs: HashMap::new(),
            asset_materials: HashMap::new(),
            inline_materials: HashMap::new(),
            material_sources: HashMap::new(),
            prefabs: PrefabCache::default(),
        })
    }

    pub fn db(&self) -> &AssetDb {
        &self.db
    }

    pub fn parse(&self, reference: &str) -> Result<AssetRef> {
        AssetRef::parse(reference, &self.db)
    }

    /// The drawable parts of a model, imported on first use.
    pub fn model(&mut self, renderer: &mut Renderer, guid: AssetGuid) -> Result<&[ImportedNode]> {
        if !self.models.contains_key(&guid) {
            let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
            if entry.kind != AssetKind::Model {
                bail!("{} is not a model", entry.path);
            }
            let path = self.db.root().join(&entry.path);
            let nodes = renderer.import_gltf(&path)?;
            for node in &nodes {
                let mesh = AssetRef::asset(guid, Some(&node.mesh_part));
                self.mesh_refs.insert(node.mesh, mesh.clone());
                self.meshes.insert(mesh, node.mesh);
                let material = AssetRef::asset(guid, node.material_part.as_deref());
                self.material_sources.insert(node.material, MaterialSource::Asset(material.clone()));
                self.asset_materials.insert(material, node.material);
            }
            self.models.insert(guid, nodes);
        }
        Ok(&self.models[&guid])
    }

    pub fn mesh(&mut self, renderer: &mut Renderer, reference: &AssetRef) -> Result<MeshHandle> {
        if let Some(&handle) = self.meshes.get(reference) {
            return Ok(handle);
        }
        let handle = match reference {
            AssetRef::Builtin(name) => renderer.primitive(primitive(name)?)?,
            AssetRef::Asset { guid, part } => {
                let nodes = self.model(renderer, *guid)?;
                let node = match part {
                    Some(part) => nodes.iter().find(|n| &n.mesh_part == part),
                    None => nodes.first(),
                };
                node.with_context(|| format!("no mesh `{reference}`"))?.mesh
            }
        };
        self.meshes.insert(reference.clone(), handle);
        self.mesh_refs.insert(handle, reference.clone());
        Ok(handle)
    }

    pub fn material(&mut self, renderer: &mut Renderer, source: &MaterialSource) -> Result<MaterialHandle> {
        let handle = match source {
            MaterialSource::Inline(data) => {
                if let Some(&handle) = self.inline_materials.get(&data.key()) {
                    return Ok(handle);
                }
                let handle = renderer.add_material(&data.desc())?;
                self.inline_materials.insert(data.key(), handle);
                handle
            }
            MaterialSource::Asset(reference) => self.asset_material(renderer, reference)?,
        };
        self.material_sources.entry(handle).or_insert_with(|| source.clone());
        Ok(handle)
    }

    fn asset_material(&mut self, renderer: &mut Renderer, reference: &AssetRef) -> Result<MaterialHandle> {
        if let Some(&handle) = self.asset_materials.get(reference) {
            return Ok(handle);
        }
        let guid = reference.guid().with_context(|| format!("`{reference}` is not a material"))?;
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        let handle = match entry.kind {
            AssetKind::Material => {
                let path = self.db.root().join(&entry.path);
                let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
                let data: MaterialData = toml::from_str(&text).with_context(|| format!("reading {}", entry.path))?;
                renderer.add_material(&data.desc())?
            }
            AssetKind::Model => {
                self.model(renderer, guid)?;
                *self.asset_materials.get(reference).with_context(|| format!("no material `{reference}`"))?
            }
            _ => bail!("{} is not a material", entry.path),
        };
        self.asset_materials.insert(reference.clone(), handle);
        Ok(handle)
    }

    /// A `.prefab` file as written, read on first use.
    pub fn prefab_file(&mut self, guid: AssetGuid) -> Result<Arc<PrefabFile>> {
        if let Some(file) = self.prefabs.files.get(&guid) {
            return Ok(file.clone());
        }
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        if entry.kind != AssetKind::Prefab {
            bail!("{} is not a .prefab file", entry.path);
        }
        let path = self.db.root().join(&entry.path);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let file: PrefabFile = toml::from_str(&text).with_context(|| format!("reading {}", entry.path))?;
        ensure!(file.format == PREFAB_FORMAT, "{} uses prefab format {}, not {PREFAB_FORMAT}", entry.path, file.format);
        let file = Arc::new(file);
        self.prefabs.files.insert(guid, file.clone());
        Ok(file)
    }

    /// Loads a prefab by GUID or project path, with its meshes and materials,
    /// so it can be instantiated any number of times. Loading it again
    /// returns the same id.
    pub fn prefab(&mut self, renderer: Option<&mut Renderer>, reference: &str) -> Result<PrefabId> {
        let guid = self.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a prefab"))?;
        if let Some(&id) = self.prefabs.ids.get(&guid) {
            return Ok(id);
        }
        let objs = prefab::expand_prefab(self, guid).with_context(|| format!("loading {reference}"))?;
        let template = Template::resolve(self, renderer, &objs).with_context(|| format!("loading {reference}"))?;
        let id = PrefabId(self.prefabs.templates.len() as u32);
        self.prefabs.templates.push(Arc::new(template));
        self.prefabs.ids.insert(guid, id);
        Ok(id)
    }

    pub fn template(&self, prefab: PrefabId) -> Option<&Arc<Template>> {
        self.prefabs.templates.get(prefab.0 as usize)
    }

    /// The reference a mesh was loaded from, for saving scenes.
    pub fn mesh_ref(&self, handle: MeshHandle) -> Option<&AssetRef> {
        self.mesh_refs.get(&handle)
    }

    pub fn material_source(&self, handle: MaterialHandle) -> Option<&MaterialSource> {
        self.material_sources.get(&handle)
    }
}

fn primitive(name: &str) -> Result<Primitive> {
    Ok(match name {
        "cube" => Primitive::Cube,
        "sphere" => Primitive::Sphere,
        "plane" => Primitive::Plane,
        _ => bail!("no built-in mesh named `{name}` (cube, sphere and plane exist)"),
    })
}

/// Colours in project files are sRGB, as picked in any colour picker, like
/// Unity's inspector. Shading happens in linear space.
pub fn linear(srgb: [f32; 3]) -> [f32; 3] {
    srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) })
}

pub fn srgb(linear: [f32; 3]) -> [f32; 3] {
    linear.map(|c| if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 })
}
