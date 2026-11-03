//! Turns asset references from scene files into GPU meshes and materials,
//! and sounds, importing each source asset once.
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
    Access, AssetDb, AssetGuid, AssetKind, AssetReader, AssetRef, FolderReader, MaterialRemap, Mounts, SoundImport,
    TextureImport,
    bundle::{BundleInfo, BundleReader, Catalog},
};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    CookedModel, ImportedModel, ImportedNode, MaterialDesc, MaterialHandle, MeshData, MeshHandle, MeshRenderer,
    IesProfile, MeshTriangles, Primitive, Renderer, TextureId, TextureSource,
};
use voxear_scene::Destroying;

use crate::{
    audio::{AudioSource, Sound, SoundCache, SoundId, load_sound},
    loading::ReadyModel,
    physics::ColliderMesh,
    prefab::{self, PREFAB_FORMAT, PrefabCache, PrefabFile, PrefabId, PrefabInstance, Template},
};

/// A PBR material as written in `.material` files and inline in scenes.
/// Colours are sRGB, as picked in a colour picker. Each map is a picture
/// the material reads, by its GUID, which multiplies the value beside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_map: Option<AssetGuid>,
    /// Metallic in its blue, roughness in its green, as glTF has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metallic_map: Option<AssetGuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_map: Option<AssetGuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive_map: Option<AssetGuid>,
}

impl Default for MaterialData {
    fn default() -> Self {
        Self {
            color: default_color(),
            metallic: 0.0,
            roughness: default_roughness(),
            emissive: [0.0; 3],
            color_map: None,
            metallic_map: None,
            normal_map: None,
            emissive_map: None,
        }
    }
}

/// One of a material's maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialMap {
    Color,
    Metallic,
    Normal,
    Emissive,
}

impl MaterialMap {
    pub const ALL: [Self; 4] = [Self::Color, Self::Metallic, Self::Normal, Self::Emissive];

    /// Whether its picture holds colours, read as sRGB, rather than values.
    pub fn srgb(self) -> bool {
        matches!(self, Self::Color | Self::Emissive)
    }
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
    /// The picture a map reads, if it has one.
    pub fn map(&self, map: MaterialMap) -> Option<AssetGuid> {
        match map {
            MaterialMap::Color => self.color_map,
            MaterialMap::Metallic => self.metallic_map,
            MaterialMap::Normal => self.normal_map,
            MaterialMap::Emissive => self.emissive_map,
        }
    }

    pub fn set_map(&mut self, map: MaterialMap, picture: Option<AssetGuid>) {
        *match map {
            MaterialMap::Color => &mut self.color_map,
            MaterialMap::Metallic => &mut self.metallic_map,
            MaterialMap::Normal => &mut self.normal_map,
            MaterialMap::Emissive => &mut self.emissive_map,
        } = picture;
    }

    /// The GPU's material, its maps' textures in [`MaterialMap::ALL`]'s order.
    fn desc(&self, [color, metallic, normal, emissive]: [Option<TextureId>; 4]) -> MaterialDesc {
        let [r, g, b] = linear(self.color);
        MaterialDesc {
            base_color: [r, g, b, 1.0],
            metallic: self.metallic,
            roughness: self.roughness,
            emissive: self.emissive,
            base_color_texture: color,
            metallic_roughness_texture: metallic,
            normal_texture: normal,
            emissive_texture: emissive,
            ..MaterialDesc::default()
        }
    }

    /// Identical inline materials share one GPU material.
    fn key(&self) -> MaterialKey {
        let [r, g, b] = self.color.map(f32::to_bits);
        let [er, eg, eb] = self.emissive.map(f32::to_bits);
        let values = [r, g, b, self.metallic.to_bits(), self.roughness.to_bits(), er, eg, eb];
        (values, MaterialMap::ALL.map(|map| self.map(map)))
    }
}

type MaterialKey = ([u32; 8], [Option<AssetGuid>; 4]);

/// What [`Content::release_unused`] freed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Released {
    pub prefabs: usize,
    pub models: usize,
    pub meshes: usize,
    pub materials: usize,
    pub textures: usize,
    pub sounds: usize,
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

/// What importing a model again changed (see [`Content::reimport_model`]).
#[derive(Clone, Debug, Default)]
pub struct Reimported {
    /// Its parts that sit elsewhere in it now, as after a change of size.
    pub moved: Vec<MovedPart>,
    /// When its parts changed and it was uploaded anew, the mesh and the
    /// material each old one became, for parts it still has.
    pub meshes: HashMap<MeshHandle, MeshHandle>,
    pub materials: HashMap<MaterialHandle, MaterialHandle>,
}

/// The parts of a model imported again whose place in it changed, from
/// each old part and the new one it became.
fn moved_parts<'a>(pairs: impl Iterator<Item = (&'a ImportedNode, &'a ImportedNode)>) -> Vec<MovedPart> {
    let place = |node: &ImportedNode| (node.position, node.rotation, node.scale);
    pairs
        .filter(|(was, now)| place(was) != place(now))
        .map(|(was, now)| MovedPart { mesh: now.mesh, before: place(was), after: place(now) })
        .collect()
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
    /// Models replaced by an import with other parts, kept for the undo
    /// steps that may still draw them.
    stale_models: Vec<ImportedModel>,
    meshes: HashMap<AssetRef, MeshHandle>,
    mesh_refs: HashMap<MeshHandle, AssetRef>,
    /// Meshes' triangles as Mesh colliders take them, read from their
    /// models when first asked for; `None` for one that could not be.
    triangles: HashMap<AssetRef, Option<ColliderMesh>>,
    /// The key the next mesh's triangles get, each read anew a new one.
    next_triangles: u64,
    asset_materials: HashMap<AssetRef, MaterialHandle>,
    inline_materials: HashMap<MaterialKey, MaterialHandle>,
    material_sources: HashMap<MaterialHandle, MaterialSource>,
    /// Each loaded model's materials drawn with `.material` assets.
    remaps: HashMap<AssetGuid, MaterialRemap>,
    /// Pictures materials read, by GUID and whether they are read as sRGB.
    pictures: HashMap<(AssetGuid, bool), TextureId>,
    /// IES profiles uploaded for lights, by their assets.
    light_profiles: HashMap<AssetGuid, TextureId>,
    /// Colour grading lookup tables uploaded, by their pictures.
    luts: HashMap<AssetGuid, TextureId>,
    prefabs: PrefabCache,
    sounds: SoundCache,
    /// What a world set aside still draws and plays, as the editor's edited
    /// level while the game plays, which `release_unused` keeps.
    aside: InUse,
}

/// The meshes, materials, prefab instances and sounds a world uses.
#[derive(Default)]
struct InUse {
    meshes: HashSet<MeshHandle>,
    materials: HashSet<MaterialHandle>,
    prefabs: HashSet<AssetGuid>,
    sounds: HashSet<SoundId>,
}

impl InUse {
    fn of(world: &World) -> Self {
        let mut used = Self::default();
        for renderer in world.query_ref::<&MeshRenderer>().without::<Destroying>() {
            used.meshes.insert(renderer.mesh);
            used.materials.insert(renderer.material);
        }
        used.prefabs = world.query_ref::<&PrefabInstance>().without::<Destroying>().into_iter().map(|p| p.0).collect();
        let heard = world.query_ref::<&AudioSource>().without::<Destroying>().into_iter().filter_map(|s| s.sound);
        used.sounds = heard.collect();
        used
    }
}

/// Where asset files are read from.
enum Source {
    /// The project's folder, and its packages' beside it.
    Folder(FolderReader, Mounts),
    /// The reader, and every catalog layer with its folder, base first.
    Bundles(BundleReader, Vec<(PathBuf, Arc<Catalog>)>),
}

impl Content {
    /// Scans the project's assets and its packages', creating `.meta` files
    /// for new ones of its own, and reads them from their folders.
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_mounted(crate::package_assets::mounts_or_project(root))
    }

    /// [`open`](Self::open), with the packages' assets mounted as `mounts`
    /// says rather than as the project's `packages.toml` resolves.
    pub fn open_mounted(mounts: Mounts) -> Result<Self> {
        let (db, report) = AssetDb::scan_mounted(mounts.clone())?;
        for path in &report.created {
            tracing::info!("new asset: {path}");
        }
        for path in &report.reassigned {
            tracing::warn!("{path} had the same GUID as another asset (copied with its .meta?); it has a new one");
        }
        Ok(Self::with_source(db, Source::Folder(FolderReader::mounted(mounts.clone()), mounts)))
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
            stale_models: Vec::new(),
            meshes: HashMap::new(),
            mesh_refs: HashMap::new(),
            triangles: HashMap::new(),
            next_triangles: 1,
            asset_materials: HashMap::new(),
            inline_materials: HashMap::new(),
            material_sources: HashMap::new(),
            remaps: HashMap::new(),
            pictures: HashMap::new(),
            light_profiles: HashMap::new(),
            luts: HashMap::new(),
            prefabs: PrefabCache::default(),
            sounds: SoundCache::default(),
            aside: InUse::default(),
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
            Source::Folder(_, mounts) => Box::new(FolderReader::mounted(mounts.clone())),
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
        let files = self.texts.keys().chain(self.models.keys()).chain(self.prefabs.files.keys()).copied();
        files.chain(self.sounds.loaded_guids()).collect()
    }

    /// Keeps texts a background load read, so later loads need no IO.
    pub(crate) fn adopt_texts(&mut self, texts: HashMap<AssetGuid, Arc<str>>) {
        self.texts.extend(texts);
    }

    /// Keeps sounds a background load read and decoded, so the scenes that
    /// play them come in with no IO or decoding.
    pub(crate) fn adopt_sounds(&mut self, sounds: Vec<(AssetGuid, Sound)>) {
        for (guid, sound) in sounds {
            self.sounds.adopt(guid, sound);
        }
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
        self.register_model(renderer, guid, model);
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

    /// Forgets what was read of every asset of a kind, as after files of it
    /// changed, so each is read again when next used.
    pub fn forget_kind(&mut self, kind: AssetKind) {
        let db = &self.db;
        self.texts.retain(|guid, _| db.get(*guid).is_none_or(|entry| entry.kind != kind));
    }

    /// Looks at the project's folder again after files came or went, as the
    /// editor's Project panel makes and removes them, giving new ones their
    /// GUIDs. Content read from bundles does not change.
    pub fn rescan(&mut self) -> Result<()> {
        let Source::Folder(_, mounts) = &self.source else { return Ok(()) };
        let (db, report) = AssetDb::scan_mounted(mounts.clone())?;
        for path in &report.created {
            tracing::info!("new asset: {path}");
        }
        self.db = Arc::new(db);
        Ok(())
    }

    /// Mounts the project's packages again, as they are after its
    /// `packages.toml` changed, and looks at every asset again. What is
    /// loaded already stays as it was.
    pub fn remount_packages(&mut self) -> Result<()> {
        let Source::Folder(reader, mounts) = &mut self.source else { return Ok(()) };
        *mounts = crate::package_assets::mounts(mounts.root())?;
        *reader = FolderReader::mounted(mounts.clone());
        self.rescan()
    }

    /// Saves a `.material` asset, and shows the change at once wherever it is
    /// drawn.
    pub fn save_material(&mut self, renderer: &mut Renderer, path: &str, data: &MaterialData) -> Result<()> {
        let Source::Folder(_, mounts) = &self.source else { bail!("materials in built content cannot change") };
        let (mount, _) = mounts.locate(path);
        if mounts.access(&mount) == Access::ReadOnly {
            bail!("{path} is in a package from the package cache, which is not changed");
        }
        let file = mounts.file(path);
        std::fs::write(&file, toml::to_string(data)?).with_context(|| format!("writing {}", file.display()))?;
        let Some(guid) = self.db.guid_of(path) else { return Ok(()) };
        self.texts.remove(&guid);
        let own = self.asset_materials.get(&AssetRef::asset(guid, None)).copied();
        let remapped = self.remapped_to(guid);
        if own.is_none() && remapped.is_empty() {
            return Ok(());
        }
        let look = self.look(renderer, data)?;
        if let Some(handle) = own {
            renderer.update_material(handle, &look)?;
        }
        for (model, part) in remapped {
            self.show_remapped(renderer, model, &part, &look)?;
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
            self.register_model(renderer, guid, model);
        }
        Ok(&self.models[&guid].nodes)
    }

    /// Imports a loaded model again, as after its import settings or its
    /// file changed. With the same parts as before, it goes behind the
    /// handles its meshes and materials have, so whatever draws them shows
    /// the change from the next frame. With other parts, as after its file
    /// changed, it is uploaded anew, and [`Reimported`] says which handles
    /// became which; the old model stays loaded, since undo steps may still
    /// draw it. A model not loaded is left to import when next used.
    pub fn reimport_model(&mut self, renderer: &mut Renderer, guid: AssetGuid) -> Result<Reimported> {
        if !self.models.contains_key(&guid) {
            return Ok(Reimported::default());
        }
        ensure!(!self.cooked.contains(&guid), "a model in built content cannot be imported again");
        let path = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?.path.clone();
        // Its colliders take the new triangles, under new keys, so their bodies are made again.
        self.triangles.retain(|reference, _| !matches!(reference, AssetRef::Asset { guid: of, .. } if *of == guid));
        let reader = self.reader();
        let prepared = voxear_render::prepare_model(&path, &mut |file| reader.read(file))?;
        let old = &self.models[&guid];
        let parts = old.nodes.iter().map(|node| (node.mesh_part.as_str(), node.material_part.as_deref()));
        if prepared.parts().eq(parts) {
            let model = renderer.reupload_model(old, &prepared)?;
            let moved = moved_parts(old.nodes.iter().zip(&model.nodes));
            self.models.insert(guid, model);
            // The file's materials went back behind the handles; the remapped ones go on them again.
            self.remap_materials(renderer, guid);
            return Ok(Reimported { moved, ..Reimported::default() });
        }

        let model = renderer.upload_model(&prepared)?;
        let old = self.models.remove(&guid).expect("loaded, as checked above");
        let mut reimported = Reimported::default();
        let mut pairs = Vec::new();
        for was in &old.nodes {
            if let Some(now) = model.nodes.iter().find(|now| now.mesh_part == was.mesh_part) {
                reimported.meshes.insert(was.mesh, now.mesh);
                pairs.push((was, now));
            }
            let material = model.nodes.iter().find(|now| now.material_part == was.material_part);
            if let Some(now) = material.filter(|_| was.material_part.is_some()) {
                reimported.materials.insert(was.material, now.material);
            }
        }
        reimported.moved = moved_parts(pairs.into_iter());
        // Prefabs drawing it are read again, with its new parts.
        self.prefabs.templates.iter_mut().for_each(|template| *template = None);
        self.stale_models.push(old);
        self.register_model(renderer, guid, model);
        Ok(reimported)
    }

    /// The project paths of the models imported from their source files.
    /// A model's parts when it is loaded, without loading it, as the
    /// editor lists meshes to pick from.
    pub fn loaded_model(&self, guid: AssetGuid) -> Option<&[ImportedNode]> {
        self.models.get(&guid).map(|model| model.nodes.as_slice())
    }

    /// How a loaded model's materials are remapped to `.material` assets.
    pub fn material_remap(&self, model: AssetGuid) -> Option<&MaterialRemap> {
        self.remaps.get(&model)
    }

    /// A loaded model's materials, each part with its name in the file, in
    /// the order its parts first draw, without loading it.
    pub fn loaded_model_materials(&self, guid: AssetGuid) -> Option<&[(String, String)]> {
        self.models.get(&guid).map(|model| model.materials.as_slice())
    }

    pub fn loaded_models(&self) -> Vec<String> {
        let source = self.models.keys().filter(|guid| !self.cooked.contains(guid));
        source.filter_map(|&guid| Some(self.db.get(guid)?.path.clone())).collect()
    }

    fn register_model(&mut self, renderer: &mut Renderer, guid: AssetGuid, model: ImportedModel) {
        for node in &model.nodes {
            let mesh = AssetRef::asset(guid, Some(&node.mesh_part));
            self.mesh_refs.insert(node.mesh, mesh.clone());
            self.meshes.insert(mesh, node.mesh);
            let material = AssetRef::asset(guid, node.material_part.as_deref());
            self.material_sources.insert(node.material, MaterialSource::Asset(material.clone()));
            self.asset_materials.insert(material, node.material);
        }
        self.models.insert(guid, model);
        self.remap_materials(renderer, guid);
    }

    /// Puts the look of each `.material` a loaded model's materials are
    /// remapped to (see [`MaterialRemap`]) on the model's own materials, so
    /// whatever draws a part shows it, and remembers the remapping, for
    /// [`Content::save_material`]. A material that cannot be read leaves the
    /// file's in place, saying so.
    fn remap_materials(&mut self, renderer: &mut Renderer, guid: AssetGuid) {
        let Some(path) = self.db.get(guid).map(|entry| entry.path.clone()) else { return };
        let meta = format!("{path}.{}", voxear_assets::META_EXTENSION);
        let remap = self.read_text(&meta).map(|text| MaterialRemap::from_meta(&text)).unwrap_or_default();
        for (part, &material) in &remap.0 {
            let look = self.material_data(material).and_then(|data| self.look(renderer, &data));
            let result = look.and_then(|look| self.show_remapped(renderer, guid, part, &look));
            if let Err(error) = result {
                tracing::warn!("{path}: {part} keeps the file's material: {error:#}");
            }
        }
        match remap.0.is_empty() {
            true => self.remaps.remove(&guid),
            false => self.remaps.insert(guid, remap),
        };
    }

    /// Shows `look` on a loaded model's material `part`.
    fn show_remapped(&self, renderer: &mut Renderer, model: AssetGuid, part: &str, look: &MaterialDesc) -> Result<()> {
        let nodes = self.models.get(&model).map_or(&[][..], |model| &model.nodes[..]);
        let handles: HashSet<MaterialHandle> =
            nodes.iter().filter(|node| node.material_part.as_deref() == Some(part)).map(|node| node.material).collect();
        ensure!(!handles.is_empty(), "the model has no material {part}");
        handles.into_iter().try_for_each(|handle| renderer.update_material(handle, look))
    }

    /// A `.material` asset's values.
    pub fn material_data(&mut self, guid: AssetGuid) -> Result<MaterialData> {
        let (text, path) = self.asset_text(guid, AssetKind::Material)?;
        toml::from_str(&text).with_context(|| format!("reading {path}"))
    }

    /// What the GPU draws a material with, its maps' pictures loaded.
    fn look(&mut self, renderer: &mut Renderer, data: &MaterialData) -> Result<MaterialDesc> {
        let mut textures = [None; 4];
        for (texture, map) in textures.iter_mut().zip(MaterialMap::ALL) {
            if let Some(picture) = data.map(map) {
                *texture = Some(self.picture(renderer, picture, map.srgb())?);
            }
        }
        Ok(data.desc(textures))
    }

    /// A picture asset as a light's cookie, in its colours as they look.
    pub fn cookie(&mut self, renderer: &mut Renderer, reference: &AssetRef) -> Result<TextureId> {
        let guid = reference.guid().with_context(|| format!("`{reference}` is not a picture"))?;
        self.picture(renderer, guid, true)
    }

    /// The picture asset a texture was uploaded from.
    pub fn picture_guid(&self, texture: TextureId) -> Option<AssetGuid> {
        self.pictures.iter().find(|(_, known)| **known == texture).map(|((guid, _), _)| *guid)
    }

    /// An IES profile asset as a light's profile, read and uploaded on first
    /// use.
    pub fn light_profile(&mut self, renderer: &mut Renderer, reference: &AssetRef) -> Result<TextureId> {
        let guid = reference.guid().with_context(|| format!("`{reference}` is not a light profile"))?;
        if let Some(&texture) = self.light_profiles.get(&guid) {
            return Ok(texture);
        }
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        ensure!(entry.kind == AssetKind::LightProfile, "{} is not an IES light profile", entry.path);
        let path = entry.path.clone();
        let profile = IesProfile::parse(&self.read_text(&path)?).with_context(|| format!("reading {path}"))?;
        let texture = renderer.add_light_profile(&profile)?;
        self.light_profiles.insert(guid, texture);
        Ok(texture)
    }

    /// A picture as a colour grading lookup table, read and uploaded as it
    /// is, on first use: its import settings' size and compression would
    /// change its colours.
    pub fn lut(&mut self, renderer: &mut Renderer, reference: &AssetRef) -> Result<TextureId> {
        let guid = reference.guid().with_context(|| format!("`{reference}` is not a picture"))?;
        if let Some(&texture) = self.luts.get(&guid) {
            return Ok(texture);
        }
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        ensure!(entry.kind == AssetKind::Texture, "{} is not a picture", entry.path);
        let path = entry.path.clone();
        let bytes = self.read(&path)?;
        let picture = image::load_from_memory(&bytes).with_context(|| format!("reading {path}"))?.to_rgba8();
        let (width, height) = picture.dimensions();
        ensure!(
            width == height * height,
            "{path} is {width}×{height}: a lookup table is as many squares wide as it is high, as 256×16 or 1024×32"
        );
        let texture = renderer.add_lut(picture.as_raw(), height)?;
        self.luts.insert(guid, texture);
        Ok(texture)
    }

    /// The IES profile asset a light's profile was uploaded from.
    pub fn light_profile_guid(&self, texture: TextureId) -> Option<AssetGuid> {
        self.light_profiles.iter().find(|(_, known)| **known == texture).map(|(guid, _)| *guid)
    }

    /// A picture asset as a texture, read and uploaded on first use at the
    /// size its import settings allow.
    fn picture(&mut self, renderer: &mut Renderer, guid: AssetGuid, srgb: bool) -> Result<TextureId> {
        if let Some(&texture) = self.pictures.get(&(guid, srgb)) {
            return Ok(texture);
        }
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        ensure!(entry.kind == AssetKind::Texture, "{} is not a picture", entry.path);
        let path = entry.path.clone();
        let bytes = self.read(&path)?;
        let meta = format!("{path}.{}", voxear_assets::META_EXTENSION);
        let settings = self.read_text(&meta).map(|text| TextureImport::from_meta(&text)).unwrap_or_default();
        let picture = image::load_from_memory(&bytes).with_context(|| format!("reading {path}"))?.to_rgba8();
        let (width, height) = settings.size(picture.width(), picture.height());
        let picture = match (width, height) == picture.dimensions() {
            true => picture,
            false => image::imageops::resize(&picture, width, height, image::imageops::FilterType::Triangle),
        };
        let source = TextureSource { name: &path, width, height, rgba: picture.as_raw(), srgb };
        let texture = renderer.add_texture(&source)?;
        self.pictures.insert((guid, srgb), texture);
        Ok(texture)
    }

    /// The models whose material is remapped to the `.material` asset `guid`,
    /// each with the part.
    fn remapped_to(&self, guid: AssetGuid) -> Vec<(AssetGuid, String)> {
        let parts = self.remaps.iter().flat_map(|(model, remap)| remap.0.iter().map(move |(part, m)| (model, part, m)));
        parts.filter(|(_, _, material)| **material == guid).map(|(model, part, _)| (*model, part.clone())).collect()
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
                let look = self.look(renderer, data)?;
                let handle = renderer.add_material(&look)?;
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
                let data = self.material_data(guid)?;
                let look = self.look(renderer, &data)?;
                renderer.add_material(&look)?
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
        // What the world uses, and what a world set aside does.
        let InUse { mut meshes, mut materials, prefabs: mut instances, sounds: mut heard } = InUse::of(world);
        meshes.extend(&self.aside.meshes);
        materials.extend(&self.aside.materials);
        instances.extend(&self.aside.prefabs);
        heard.extend(&self.aside.sounds);

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
        released.sounds = self.sounds.release(&heard);
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
            self.remaps.remove(&guid);
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

    /// Keeps what a world set aside uses from being released until
    /// [`bring_back`](Self::bring_back): the editor's edited level, while
    /// the game plays in a world of its own and changes levels.
    pub fn set_aside(&mut self, world: &World) {
        self.aside = InUse::of(world);
    }

    /// Lets go of the world [`set_aside`](Self::set_aside), once it is the
    /// world in use again.
    pub fn bring_back(&mut self) {
        self.aside = InUse::default();
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

    /// A mesh's triangles, as its Obj's Mesh collider takes its shape from
    /// them: read from its model the first time one asks, and kept until
    /// the model is imported again. `None` for a mesh not loaded from
    /// content, or a model that cannot be read, which is logged.
    pub fn triangles(&mut self, mesh: MeshHandle) -> Option<ColliderMesh> {
        let reference = self.mesh_refs.get(&mesh)?.clone();
        if let Some(found) = self.triangles.get(&reference) {
            return found.clone();
        }
        let read = self.read_triangles(&reference);
        let read = read.inspect_err(|error| tracing::warn!("{reference}'s triangles for a collider: {error:#}"));
        let found = read.ok().flatten().map(|triangles| {
            self.next_triangles += 1;
            ColliderMesh {
                key: self.next_triangles,
                points: triangles.points.into(),
                indices: triangles.indices.into(),
            }
        });
        self.triangles.insert(reference, found.clone());
        found
    }

    /// A mesh's triangles from its model's file, or the built-in mesh's.
    fn read_triangles(&mut self, reference: &AssetRef) -> Result<Option<MeshTriangles>> {
        let (guid, part) = match reference {
            AssetRef::Builtin(name) => return Ok(Some(MeshData::primitive(primitive(name)?).triangles())),
            AssetRef::Asset { guid, part } => (*guid, part.as_deref()),
        };
        let path = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?.path.clone();
        if self.cooked.contains(&guid) {
            return CookedModel::from_bytes(&path, &self.read(&path)?)?.triangles(part);
        }
        let reader = self.reader();
        Ok(voxear_render::prepare_model(&path, &mut |file| reader.read(file))?.triangles(part))
    }

    /// The reference a mesh was loaded from, for saving scenes.
    pub fn mesh_ref(&self, handle: MeshHandle) -> Option<&AssetRef> {
        self.mesh_refs.get(&handle)
    }

    pub fn material_source(&self, handle: MaterialHandle) -> Option<&MaterialSource> {
        self.material_sources.get(&handle)
    }

    /// A sound asset's id, loading it now if it is not loaded, as its
    /// import settings say: decoded whole, or kept to stream as it plays.
    pub fn sound(&mut self, reference: &AssetRef) -> Result<SoundId> {
        let guid = reference.guid().with_context(|| format!("`{reference}` is not a sound"))?;
        let entry = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        ensure!(entry.kind == AssetKind::Sound, "{} is not a sound", entry.path);
        let id = self.sounds.id(guid);
        self.loaded_sound(id)?;
        Ok(id)
    }

    /// A sound by id, loaded again if it was freed since.
    pub fn loaded_sound(&mut self, id: SoundId) -> Result<Arc<Sound>> {
        if let Some(sound) = self.sounds.get(id) {
            return Ok(sound);
        }
        let guid = self.sounds.guid(id).with_context(|| format!("no sound has id {}", id.0))?;
        let path = self.db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?.path.clone();
        let bytes = self.read(&path)?;
        let sound = Arc::new(load_sound(bytes, &path, sound_import(self.reader(), &path))?);
        self.sounds.put(id, sound.clone());
        Ok(sound)
    }

    pub fn sound_guid(&self, id: SoundId) -> Option<AssetGuid> {
        self.sounds.guid(id)
    }

    /// Forgets a loaded sound, so it loads again with new import settings
    /// the next time it plays. Voices playing it now play on as they were.
    pub fn forget_sound(&mut self, path: &str) {
        if let Some(guid) = self.db.guid_of(path) {
            let id = self.sounds.id(guid);
            self.sounds.release_one(id);
        }
    }

    /// How many sounds are loaded now.
    pub fn loaded_sounds(&self) -> usize {
        self.sounds.loaded()
    }
}

/// A sound's import settings, from its `.meta`, which built content ships
/// beside it for these.
pub(crate) fn sound_import(reader: &mut dyn AssetReader, path: &str) -> SoundImport {
    let settings = reader.read_text(&format!("{path}.{}", voxear_assets::META_EXTENSION)).ok();
    settings.map_or_else(SoundImport::default, |text| SoundImport::from_meta(&text))
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
