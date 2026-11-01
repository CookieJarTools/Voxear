//! Turns asset references from scene files into GPU meshes and materials,
//! importing each source asset once.
//!
//! In a project this reads source files directly, so pressing play never
//! needs a build. Built games read the bundles `vox content build` makes,
//! through the same calls (see [`Content::open_bundles`]).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{
    AssetDb, AssetGuid, AssetKind, AssetReader, AssetRef, FolderReader,
    bundle::{BundleInfo, BundleReader, Catalog},
};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    CookedModel, ImportedModel, ImportedNode, MaterialDesc, MaterialHandle, MeshHandle, MeshRenderer, Primitive,
    Renderer,
};
use voxear_scene::Destroying;

use crate::{
    loading::ReadyModel,
    prefab::{self, PREFAB_FORMAT, PrefabCache, PrefabFile, PrefabId, PrefabInstance, Template},
};

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

/// What [`Content::release_unused`] freed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Released {
    pub prefabs: usize,
    pub models: usize,
    pub meshes: usize,
    pub materials: usize,
    pub textures: usize,
}

impl Released {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// What content has loaded, beside what the GPU holds (see
/// `Renderer::asset_usage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentUsage {
    pub models: usize,
    pub prefabs: usize,
    pub kept_prefabs: usize,
    pub materials: usize,
}

/// A part of a model imported again that sits elsewhere in it now: its
/// mesh, and its position, rotation and scale in the model before and after.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovedPart {
    pub mesh: MeshHandle,
    pub before: (Vec3, Quat, Vec3),
    pub after: (Vec3, Quat, Vec3),
}

/// Where a material came from, so scenes save it back the way it was written.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialSource {
    Asset(AssetRef),
    Inline(MaterialData),
}

pub struct Content {
    db: Arc<AssetDb>,
    source: Source,
    /// Asset texts a background load read ahead, by GUID.
    texts: HashMap<AssetGuid, Arc<str>>,
    /// Models the content build cooked, which load without import work.
    cooked: Arc<HashSet<AssetGuid>>,
    models: HashMap<AssetGuid, ImportedModel>,
    meshes: HashMap<AssetRef, MeshHandle>,
    mesh_refs: HashMap<MeshHandle, AssetRef>,
    asset_materials: HashMap<AssetRef, MaterialHandle>,
    inline_materials: HashMap<[u32; 8], MaterialHandle>,
    material_sources: HashMap<MaterialHandle, MaterialSource>,
    prefabs: PrefabCache,
}

/// Where asset files are read from.
enum Source {
    Folder(FolderReader, PathBuf),
    /// The reader, and every catalog layer with its folder, base first.
    Bundles(BundleReader, Vec<(PathBuf, Arc<Catalog>)>),
}

impl Content {
    /// Scans the project's assets, creating `.meta` files for new ones, and
    /// reads them from the project folder.
    pub fn open(root: &Path) -> Result<Self> {
        let (db, report) = AssetDb::scan(root)?;
        for path in &report.created {
            tracing::info!("new asset: {path}");
        }
        for path in &report.reassigned {
            tracing::warn!("{path} had the same GUID as another asset (copied with its .meta?); it has a new one");
        }
        Ok(Self::with_source(db, Source::Folder(FolderReader::new(root), root.to_path_buf())))
    }

    /// Reads assets from built bundles in `content_dir` (see
    /// `vox content build`), as a player does. Only assets the build
    /// included can load.
    pub fn open_bundles(root: &Path, content_dir: &Path) -> Result<Self> {
        let catalog = Catalog::read(content_dir)?;
        let db = AssetDb::from_entries(root, catalog.entries()?);
        let reader = BundleReader::new(content_dir, &catalog);
        let cooked = catalog.assets.iter().filter(|a| a.cooked).map(|a| a.guid.parse()).collect::<Result<_>>()?;
        let layers = vec![(content_dir.to_path_buf(), Arc::new(catalog))];
        let mut content = Self::with_source(db, Source::Bundles(reader, layers));
        content.cooked = Arc::new(cooked);
        Ok(content)
    }

    /// Layers more built content on top, such as a mod, a patch or DLC. An
    /// asset with the GUID of one already here replaces it (give a
    /// replacement the original's `.meta`); one with a new GUID is added.
    /// Path lookups find the topmost layer's asset. Mount before loading:
    /// what is loaded already stays as it was.
    pub fn mount(&mut self, content_dir: &Path) -> Result<()> {
        let catalog = Catalog::read(content_dir)?;
        let Source::Bundles(reader, layers) = &mut self.source else {
            bail!("only built content can have layers; open it with Content::open_bundles");
        };
        reader.mount(content_dir, &catalog);
        let mut db = (*self.db).clone();
        let mut cooked = (*self.cooked).clone();
        for (asset, entry) in catalog.assets.iter().zip(catalog.entries()?) {
            if asset.cooked {
                cooked.insert(entry.guid);
            } else {
                cooked.remove(&entry.guid);
            }
            db.insert(entry);
        }
        self.db = Arc::new(db);
        self.cooked = Arc::new(cooked);
        layers.push((content_dir.to_path_buf(), Arc::new(catalog)));
        tracing::info!("mounted {} over the content", content_dir.display());
        Ok(())
    }

    fn with_source(db: AssetDb, source: Source) -> Self {
        Self {
            db: Arc::new(db),
            source,
            texts: HashMap::new(),
            cooked: Arc::default(),
            models: HashMap::new(),
            meshes: HashMap::new(),
            mesh_refs: HashMap::new(),
            asset_materials: HashMap::new(),
            inline_materials: HashMap::new(),
            material_sources: HashMap::new(),
            prefabs: PrefabCache::default(),
        }
    }

    /// Reads one of an asset's files by project path.
    pub fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        self.reader().read(path)
    }

    pub fn read_text(&mut self, path: &str) -> Result<String> {
        self.reader().read_text(path)
    }

    /// The bundles opened so far, when reading built content.
    pub fn opened_bundles(&self) -> Vec<&BundleInfo> {
        match &self.source {
            Source::Bundles(reader, _) => reader.opened().collect(),
            Source::Folder(..) => Vec::new(),
        }
    }

    fn reader(&mut self) -> &mut dyn AssetReader {
        match &mut self.source {
            Source::Folder(reader, _) => reader,
            Source::Bundles(reader, _) => reader,
        }
    }

    /// A second reader over the same content, for a loading thread.
    pub(crate) fn worker_reader(&self) -> Box<dyn AssetReader> {
        match &self.source {
            Source::Folder(_, root) => Box::new(FolderReader::new(root)),
            Source::Bundles(_, layers) => {
                let (base_dir, base) = &layers[0];
                let mut reader = BundleReader::new(base_dir, base);
                layers[1..].iter().for_each(|(dir, catalog)| reader.mount(dir, catalog));
                Box::new(reader)
            }
        }
    }

    pub(crate) fn shared_db(&self) -> Arc<AssetDb> {
        self.db.clone()
    }

    pub(crate) fn cooked_models(&self) -> Arc<HashSet<AssetGuid>> {
        self.cooked.clone()
    }

    /// The text of an asset of the expected kind, with its project path. A
    /// background load may have read it already.
    pub fn asset_text(&mut self, guid: AssetGuid, kind: AssetKind) -> Result<(String, String)> {
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        ensure!(entry.kind == kind, "{} is not a {kind:?} asset", entry.path);
        let path = entry.path.clone();
        if let Some(text) = self.texts.get(&guid) {
            return Ok((text.to_string(), path));
        }
        Ok((self.read_text(&path)?, path))
    }

    /// Assets that are loaded already, which a background load can skip.
    pub(crate) fn loaded(&self) -> HashSet<AssetGuid> {
        self.texts.keys().chain(self.models.keys()).chain(self.prefabs.files.keys()).copied().collect()
    }

    /// Keeps texts a background load read, so later loads need no IO.
    pub(crate) fn adopt_texts(&mut self, texts: HashMap<AssetGuid, Arc<str>>) {
        self.texts.extend(texts);
    }

    /// Uploads a model a background load prepared, unless it is loaded already.
    pub(crate) fn adopt_model(&mut self, renderer: &mut Renderer, guid: AssetGuid, model: &ReadyModel) -> Result<()> {
        if self.models.contains_key(&guid) {
            return Ok(());
        }
        let model = match model {
            ReadyModel::Source(model) => renderer.upload_model(model)?,
            ReadyModel::Cooked(model) => renderer.upload_cooked(model)?,
        };
        self.register_model(guid, model);
        Ok(())
    }

    pub fn db(&self) -> &AssetDb {
        &self.db
    }

    /// Forgets what was read of an asset, so it is read from its file again
    /// when next used, as after a prefab opened on its own is saved: new
    /// instances, and the level reopened, show the change.
    pub fn forget(&mut self, path: &str) {
        let Some(guid) = self.db.guid_of(path) else { return };
        self.texts.remove(&guid);
        self.prefabs.files.remove(&guid);
        if let Some(&id) = self.prefabs.ids.get(&guid)
            && let Some(template) = self.prefabs.templates.get_mut(id.0 as usize)
        {
            *template = None;
        }
    }

    /// Looks at the project's folder again after files came or went, as the
    /// editor's Project panel makes and removes them, giving new ones their
    /// GUIDs. Content read from bundles does not change.
    pub fn rescan(&mut self) -> Result<()> {
        let Source::Folder(_, root) = &self.source else { return Ok(()) };
        let (db, report) = AssetDb::scan(root)?;
        for path in &report.created {
            tracing::info!("new asset: {path}");
        }
        self.db = Arc::new(db);
        Ok(())
    }

    /// Saves a `.material` asset, and shows the change at once wherever it is
    /// drawn.
    pub fn save_material(&mut self, renderer: &mut Renderer, path: &str, data: &MaterialData) -> Result<()> {
        let Source::Folder(_, root) = &self.source else { bail!("materials in built content cannot change") };
        let file = root.join(path);
        std::fs::write(&file, toml::to_string(data)?).with_context(|| format!("writing {}", file.display()))?;
        let Some(guid) = self.db.guid_of(path) else { return Ok(()) };
        self.texts.remove(&guid);
        if let Some(&handle) = self.asset_materials.get(&AssetRef::asset(guid, None)) {
            renderer.update_material(handle, &data.desc())?;
        }
        Ok(())
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
            let path = entry.path.clone();
            let model = if self.cooked.contains(&guid) {
                renderer.upload_cooked(&CookedModel::from_bytes(&path, &self.read(&path)?)?)?
            } else {
                let reader = self.reader();
                renderer.import_model(&path, &mut |file| reader.read(file))?
            };
            self.register_model(guid, model);
        }
        Ok(&self.models[&guid].nodes)
    }

    /// Imports a loaded model again, as after its import settings changed,
    /// behind the handles its meshes and materials have, so whatever draws
    /// them shows the change from the next frame. Returns each part whose
    /// place in the model changed, as after a change of size. A model not
    /// loaded is left to import with its settings when next used.
    pub fn reimport_model(&mut self, renderer: &mut Renderer, guid: AssetGuid) -> Result<Vec<MovedPart>> {
        if !self.models.contains_key(&guid) {
            return Ok(Vec::new());
        }
        ensure!(!self.cooked.contains(&guid), "a model in built content cannot be imported again");
        let path = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?.path.clone();
        let reader = self.reader();
        let prepared = voxear_render::prepare_model(&path, &mut |file| reader.read(file))?;
        let old = &self.models[&guid];
        let model = renderer.reupload_model(old, &prepared)?;
        let place = |node: &ImportedNode| (node.position, node.rotation, node.scale);
        let moved = (old.nodes.iter().zip(&model.nodes))
            .filter(|(was, now)| place(was) != place(now))
            .map(|(was, now)| MovedPart { mesh: now.mesh, before: place(was), after: place(now) })
            .collect();
        self.models.insert(guid, model);
        Ok(moved)
    }

    fn register_model(&mut self, guid: AssetGuid, model: ImportedModel) {
        for node in &model.nodes {
            let mesh = AssetRef::asset(guid, Some(&node.mesh_part));
            self.mesh_refs.insert(node.mesh, mesh.clone());
            self.meshes.insert(mesh, node.mesh);
            let material = AssetRef::asset(guid, node.material_part.as_deref());
            self.material_sources.insert(node.material, MaterialSource::Asset(material.clone()));
            self.asset_materials.insert(material, node.material);
        }
        self.models.insert(guid, model);
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
                let (text, path) = self.asset_text(guid, AssetKind::Material)?;
                let data: MaterialData = toml::from_str(&text).with_context(|| format!("reading {path}"))?;
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
        let (text, path) = self.asset_text(guid, AssetKind::Prefab)?;
        let file: PrefabFile = toml::from_str(&text).with_context(|| format!("reading {path}"))?;
        ensure!(file.format == PREFAB_FORMAT, "{path} uses prefab format {}, not {PREFAB_FORMAT}", file.format);
        let file = Arc::new(file);
        self.prefabs.files.insert(guid, file.clone());
        Ok(file)
    }

    /// Loads a prefab by GUID or project path, with its meshes and materials,
    /// so it can be instantiated any number of times. Loading it again
    /// returns the same id.
    pub fn prefab(&mut self, renderer: Option<&mut Renderer>, reference: &str) -> Result<PrefabId> {
        let guid = self.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a prefab"))?;
        let id = match self.prefabs.ids.get(&guid) {
            Some(&id) => id,
            None => {
                let id = PrefabId(self.prefabs.templates.len() as u32);
                self.prefabs.templates.push(None);
                self.prefabs.guids.push(guid);
                self.prefabs.ids.insert(guid, id);
                id
            }
        };
        self.loaded_template(renderer, id).with_context(|| format!("loading {reference}"))?;
        Ok(id)
    }

    /// The asset a loaded prefab came from, by its id.
    pub fn prefab_guid(&self, prefab: PrefabId) -> Option<AssetGuid> {
        self.prefabs.guids.get(prefab.0 as usize).copied()
    }

    /// A loaded prefab's template, or `None` once it has been released.
    pub fn template(&self, prefab: PrefabId) -> Option<&Arc<Template>> {
        self.prefabs.templates.get(prefab.0 as usize)?.as_ref()
    }

    /// A prefab's template, loading it again if a sweep released it, so a
    /// prefab id never goes stale.
    pub fn loaded_template(&mut self, renderer: Option<&mut Renderer>, prefab: PrefabId) -> Result<Arc<Template>> {
        let index = prefab.0 as usize;
        let guid = *self.prefabs.guids.get(index).context("no prefab with that id was ever loaded")?;
        self.prefabs.used.insert(prefab);
        if let Some(template) = &self.prefabs.templates[index] {
            return Ok(template.clone());
        }
        let objs = prefab::expand_prefab(self, guid)?;
        let template = Arc::new(Template::resolve(self, renderer, &objs)?);
        self.prefabs.templates[index] = Some(template.clone());
        Ok(template)
    }

    /// Keeps a prefab loaded across sweeps until it is let go, as for a
    /// prefab a persistent manager spawns now and then.
    pub fn keep_prefab(&mut self, prefab: PrefabId, keep: bool) {
        if keep {
            self.prefabs.kept.insert(prefab);
        } else {
            self.prefabs.kept.remove(&prefab);
        }
    }

    /// Frees what nothing uses any more, as Unity's `UnloadUnusedAssets`
    /// does; the game calls it after every level change.
    ///
    /// In use are the meshes and materials live Objs draw with, and the
    /// prefabs that are kept, loaded or instantiated since the last sweep, or
    /// have a live instance, with everything they draw with. A model goes as
    /// a whole once none of its pieces is in use. Texts read ahead and parsed
    /// prefab files are dropped too, and bundles close: whatever is needed
    /// later is read again. A released prefab loads again the next time it
    /// is used.
    pub fn release_unused(
        &mut self,
        world: &World,
        renderer: Option<&mut Renderer>,
        keep_models: &HashSet<AssetGuid>,
    ) -> Released {
        let mut released = Released::default();
        let mut meshes = HashSet::new();
        let mut materials = HashSet::new();
        for renderer in world.query_ref::<&MeshRenderer>().without::<Destroying>() {
            meshes.insert(renderer.mesh);
            materials.insert(renderer.material);
        }

        let instances: HashSet<AssetGuid> =
            world.query_ref::<&PrefabInstance>().without::<Destroying>().into_iter().map(|p| p.0).collect();
        let cache = &mut self.prefabs;
        for (index, slot) in cache.templates.iter_mut().enumerate() {
            let id = PrefabId(index as u32);
            let in_use =
                cache.kept.contains(&id) || cache.used.contains(&id) || instances.contains(&cache.guids[index]);
            match slot {
                Some(template) if in_use => {
                    let drawn = template.objs.iter().filter_map(|o| o.renderer);
                    drawn.for_each(|r| {
                        meshes.insert(r.mesh);
                        materials.insert(r.material);
                    });
                }
                Some(_) => {
                    *slot = None;
                    released.prefabs += 1;
                }
                None => {}
            }
        }
        cache.used.clear();
        cache.files.clear();
        self.texts.clear();
        if let Source::Bundles(reader, _) = &mut self.source {
            reader.close_all();
        }

        let Some(renderer) = renderer else { return released };
        let unused: Vec<AssetGuid> = self
            .models
            .iter()
            .filter(|(guid, m)| {
                let drawn = m.nodes.iter().any(|n| meshes.contains(&n.mesh) || materials.contains(&n.material));
                !drawn && !keep_models.contains(guid)
            })
            .map(|(guid, _)| *guid)
            .collect();
        for guid in unused {
            let model = self.models.remove(&guid).expect("listed above");
            let model_meshes: HashSet<MeshHandle> = model.nodes.iter().map(|n| n.mesh).collect();
            let model_materials: HashSet<MaterialHandle> = model.nodes.iter().map(|n| n.material).collect();
            model_meshes.iter().for_each(|&m| renderer.free_mesh(m));
            model_materials.iter().for_each(|&m| renderer.free_material(m));
            model.textures.iter().for_each(|&t| renderer.free_texture(t));
            self.meshes.retain(|_, m| !model_meshes.contains(m));
            self.mesh_refs.retain(|m, _| !model_meshes.contains(m));
            self.asset_materials.retain(|_, m| !model_materials.contains(m));
            released.models += 1;
            released.meshes += model_meshes.len();
            released.materials += model_materials.len();
            released.textures += model.textures.len();
        }

        // Materials of their own: `.material` files and inline ones.
        let mut free_material = |handle: MaterialHandle| {
            let unused = !materials.contains(&handle);
            if unused {
                renderer.free_material(handle);
                released.materials += 1;
            }
            !unused
        };
        self.asset_materials.retain(|_, handle| free_material(*handle));
        self.inline_materials.retain(|_, handle| free_material(*handle));
        let known: HashSet<MaterialHandle> =
            self.asset_materials.values().chain(self.inline_materials.values()).copied().collect();
        self.material_sources.retain(|handle, _| known.contains(handle));
        released
    }

    /// What is loaded now, for the stats overlay and scripts.
    pub fn usage(&self) -> ContentUsage {
        ContentUsage {
            models: self.models.len(),
            prefabs: self.prefabs.templates.iter().flatten().count(),
            kept_prefabs: self.prefabs.kept.len(),
            materials: self.asset_materials.len() + self.inline_materials.len(),
        }
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
