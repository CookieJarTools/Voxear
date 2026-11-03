//! Loading in the background, so the game keeps running while a level or a
//! prefab loads.
//!
//! A level, a scene to load alongside those loaded, or a prefab. A worker
//! thread, with its own reader, reads and parses every level, scene, prefab
//! and material file the target needs, decodes its sounds (only reading
//! those kept to stream), and prepares its models: parsing, decoding images
//! and building LODs. It reads in waves, one batch
//! per step down the dependency graph (the level, then its scenes, then what
//! they use), so the IO system works on each wave's reads together. The main thread then uploads
//! the models a few at a time within a per-frame budget, and finishes. A
//! level waits as **ready** until activation is allowed (and, with
//! [`LevelOptions::least_time`], until enough time has passed), then changes
//! over at the end of a frame, like `Level.Load`. A level's progress counts
//! each of its scenes for its `weight`, as LayerCake's does, and scenes
//! already loaded count as done.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use voxear_assets::{AssetDb, AssetGuid, AssetKind, AssetReader, AssetRef};
use voxear_render::{CookedModel, PreparedModel, Renderer};

use crate::{
    audio::{Sound, load_sound},
    content::{Content, sound_import},
    content_build::direct_needs,
    levels::{LevelChange, LevelOptions},
    prefab::PrefabId,
};

/// How long uploads may take per frame, across all loads. At least one model
/// uploads each frame, however long it takes.
const UPLOAD_BUDGET: Duration = Duration::from_millis(4);

/// A background load, by the id scripts hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoadId(pub u32);

/// What a background load brings in, by GUID or project path.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadTarget {
    Level(String),
    /// One scene, alongside those loaded, as [`crate::Levels::add_scene`]
    /// loads it. It waits at ready as a level does.
    Scene(String),
    Prefab(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadState {
    /// Reading files and preparing models, then uploading them.
    Loading,
    /// A level that has loaded and waits for activation to be allowed.
    Ready,
    Done,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadStatus {
    pub state: LoadState,
    /// From 0 to 1. A level waiting for activation stops at 0.9, as in Unity.
    pub progress: f32,
    /// The loaded prefab, once a prefab load is done.
    pub prefab: Option<PrefabId>,
    /// The scenes a level change brought in, once it is done.
    pub scenes: Vec<String>,
}

/// What a background load does with models.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModelWork {
    /// Nothing draws them, as in headless runs.
    #[default]
    Skip,
    /// Ready them to upload. Cooked models stay compressed for the GPU to
    /// decompress, or with `decompress` are decompressed on the loading
    /// thread, for GPUs that cannot.
    Prepare { decompress: bool },
}

impl ModelWork {
    pub fn for_renderer(renderer: Option<&Renderer>) -> Self {
        renderer.map_or(Self::Skip, |r| Self::Prepare { decompress: !r.decompresses_on_gpu() })
    }
}

/// A model ready to upload: prepared from its source in a project, or read
/// cooked from built content.
pub(crate) enum ReadyModel {
    Source(PreparedModel),
    Cooked(CookedModel),
}

/// What the worker read and prepared.
#[derive(Default)]
struct Prepared {
    texts: HashMap<AssetGuid, Arc<str>>,
    models: Vec<(AssetGuid, ReadyModel)>,
    sounds: Vec<(AssetGuid, Sound)>,
}

/// Counted by the worker, read by the main thread.
#[derive(Default)]
struct Progress {
    found: AtomicU32,
    done: AtomicU32,
    /// For a level: each scene's weight, and the files found and done for it.
    scenes: Mutex<HashMap<AssetGuid, (f32, u32, u32)>>,
}

impl Progress {
    /// How much has been read, from 0 to 1: each scene for its weight, or
    /// for a prefab, every file alike.
    fn reading(&self) -> f32 {
        let scenes = self.scenes.lock().expect("never poisoned");
        let total: f32 = scenes.values().map(|(weight, ..)| weight).sum();
        if total > 0.0 {
            let part = |found: u32, done: u32| if found == 0 { 1.0 } else { done as f32 / found as f32 };
            return scenes.values().map(|&(weight, found, done)| weight * part(found, done)).sum::<f32>() / total;
        }
        self.done.load(Ordering::Relaxed) as f32 / self.found.load(Ordering::Relaxed).max(1) as f32
    }

    fn count(&self, owner: Option<AssetGuid>, done: bool) {
        let counter = if done { &self.done } else { &self.found };
        counter.fetch_add(1, Ordering::Relaxed);
        let Some(owner) = owner else { return };
        if let Some(scene) = self.scenes.lock().expect("never poisoned").get_mut(&owner) {
            if done { scene.2 += 1 } else { scene.1 += 1 }
        }
    }
}

struct Load {
    id: LoadId,
    target: LoadTarget,
    progress: Arc<Progress>,
    job: Option<JoinHandle<Result<Prepared>>>,
    prepared: Option<Prepared>,
    uploads: usize,
    /// Models this load uploaded, which sweeps leave alone until it is done.
    uploaded: Vec<AssetGuid>,
    allow_activation: bool,
    /// How a level changes over, and when it started, for its least time.
    options: LevelOptions,
    started: Instant,
    status: LoadStatus,
}

/// Every background load, polled once per frame by [`crate::Game`].
#[derive(Default)]
pub struct Loads {
    next: u32,
    active: Vec<Load>,
    finished: HashMap<LoadId, LoadStatus>,
}

impl Loads {
    /// Starts loading a level or prefab on a worker thread.
    pub fn start(&mut self, content: &Content, target: LoadTarget, models: ModelWork) -> Result<LoadId> {
        self.start_with(content, target, LevelOptions::default(), models)
    }

    /// Starts loading a level that changes over as `options` say.
    pub fn start_with(
        &mut self,
        content: &Content,
        target: LoadTarget,
        options: LevelOptions,
        models: ModelWork,
    ) -> Result<LoadId> {
        let reference = match &target {
            LoadTarget::Level(path) | LoadTarget::Scene(path) | LoadTarget::Prefab(path) => path,
        };
        let root = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not an asset"))?;
        let db = content.shared_db();
        let mut reader = content.worker_reader();
        let loaded = content.loaded();
        let cooked = content.cooked_models();
        let progress = Arc::new(Progress::default());
        let counter = progress.clone();
        let job = std::thread::Builder::new().name(format!("load {reference}")).spawn(move || {
            // The Profiler shows this thread's samples beside the main thread's.
            voxear_core::profiling::register_thread("Loading");
            voxear_core::profile_scope!("Load");
            let models = Models { work: models, cooked: &cooked };
            gather(&db, reader.as_mut(), root, &loaded, models, &counter)
        })?;

        let id = LoadId(self.next);
        self.next += 1;
        self.active.push(Load {
            id,
            target,
            progress,
            job: Some(job),
            prepared: None,
            uploads: 0,
            uploaded: Vec::new(),
            allow_activation: !options.hold,
            options,
            started: Instant::now(),
            status: LoadStatus { state: LoadState::Loading, progress: 0.0, prefab: None, scenes: Vec::new() },
        });
        Ok(id)
    }

    pub fn status(&self, id: LoadId) -> Option<LoadStatus> {
        let active = self.active.iter().find(|l| l.id == id).map(|l| l.status.clone());
        active.or_else(|| self.finished.get(&id).cloned())
    }

    /// A level load that is not allowed to activate stops at ready, so a
    /// loading screen can choose its moment.
    pub fn set_allow_activation(&mut self, id: LoadId, allow: bool) {
        if let Some(load) = self.active.iter_mut().find(|l| l.id == id) {
            load.allow_activation = allow;
        }
    }

    pub fn is_idle(&self) -> bool {
        self.active.is_empty()
    }

    /// The level change under way, if any, and how far along it is: what a
    /// progress bar follows, whoever started it.
    pub fn level_in_progress(&self) -> Option<(&str, &LoadStatus)> {
        self.active.iter().find_map(|load| match &load.target {
            LoadTarget::Level(level) => Some((level.as_str(), &load.status)),
            LoadTarget::Scene(_) | LoadTarget::Prefab(_) => None,
        })
    }

    /// Models that loads still in progress have uploaded.
    pub fn uploaded_models(&self) -> HashSet<AssetGuid> {
        self.active.iter().flat_map(|l| l.uploaded.iter().copied()).collect()
    }

    /// Moves every load along: collects finished workers, uploads within the
    /// frame budget, and returns the levels and scenes that may now come in,
    /// with how. Call once per frame, after the engine ticks.
    pub fn advance(
        &mut self,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
    ) -> Vec<(LoadId, LoadTarget, LevelOptions)> {
        let started = Instant::now();
        let mut activate = Vec::new();
        for load in &mut self.active {
            let step = advance_one(load, content, renderer.as_deref_mut(), started);
            match step {
                Ok(true) => activate.push((load.id, load.target.clone(), load.options)),
                Ok(false) => {}
                Err(error) => load.status.state = LoadState::Failed(format!("{error:#}")),
            }
        }
        self.retire();
        activate
    }

    /// Records the outcome of a level change, or a scene loaded alongside,
    /// that [`advance`](Self::advance) asked for.
    pub fn activated(&mut self, id: LoadId, result: Result<LevelChange>) {
        if let Some(load) = self.active.iter_mut().find(|l| l.id == id) {
            load.status.state = match result {
                Ok(change) => {
                    load.status.scenes = change.loaded;
                    LoadState::Done
                }
                Err(error) => LoadState::Failed(format!("{error:#}")),
            };
            load.status.progress = 1.0;
        }
        self.retire();
    }

    fn retire(&mut self) {
        let (finished, active): (Vec<Load>, Vec<Load>) = std::mem::take(&mut self.active)
            .into_iter()
            .partition(|l| matches!(l.status.state, LoadState::Done | LoadState::Failed(_)));
        self.active = active;
        for load in finished {
            if let LoadState::Failed(error) = &load.status.state {
                tracing::error!("could not load {:?}: {error}", load.target);
            }
            self.finished.insert(load.id, load.status);
        }
    }
}

/// One frame's work on one load. Returns whether its level or scene may
/// now come in.
fn advance_one(
    load: &mut Load,
    content: &mut Content,
    mut renderer: Option<&mut Renderer>,
    frame_started: Instant,
) -> Result<bool> {
    if load.job.as_ref().is_some_and(|job| job.is_finished()) {
        let job = load.job.take().expect("checked above");
        let prepared = job.join().map_err(|_| anyhow::anyhow!("the loading thread panicked"))??;
        load.prepared = Some(prepared);
    }
    let Some(prepared) = &mut load.prepared else {
        load.status.progress = load.status.progress.max(load.progress.reading() * 0.8);
        return Ok(false);
    };

    if let Some(renderer) = renderer.as_deref_mut() {
        let total = load.uploads + prepared.models.len();
        while let Some((guid, model)) = prepared.models.pop() {
            content.adopt_model(renderer, guid, &model)?;
            load.uploads += 1;
            load.uploaded.push(guid);
            load.status.progress = 0.8 + 0.1 * load.uploads as f32 / total as f32;
            if frame_started.elapsed() > UPLOAD_BUDGET {
                return Ok(false);
            }
        }
    }
    // Texts and sounds go to content only when used, so a level held at
    // ready keeps its own through any sweep another level change makes.
    match &load.target {
        LoadTarget::Prefab(path) => {
            content.adopt_texts(std::mem::take(&mut prepared.texts));
            content.adopt_sounds(std::mem::take(&mut prepared.sounds));
            load.status.prefab = Some(content.prefab(renderer, path)?);
            load.status.state = LoadState::Done;
            load.status.progress = 1.0;
            Ok(false)
        }
        LoadTarget::Level(_) | LoadTarget::Scene(_) => {
            load.status.state = LoadState::Ready;
            load.status.progress = 0.9;
            let early = load.started.elapsed().as_secs_f32() < load.options.least_time;
            if !load.allow_activation || early {
                return Ok(false);
            }
            content.adopt_texts(std::mem::take(&mut prepared.texts));
            content.adopt_sounds(std::mem::take(&mut prepared.sounds));
            Ok(true)
        }
    }
}

/// How the worker treats models.
struct Models<'a> {
    work: ModelWork,
    /// Those the content build cooked, which only need reading.
    cooked: &'a HashSet<AssetGuid>,
}

/// The weight a level file gives each of its scenes.
fn note_weights(db: &AssetDb, level: &toml::Table, progress: &Progress) {
    let Some(scenes) = level.get("scene").and_then(toml::Value::as_array) else { return };
    let mut weights = progress.scenes.lock().expect("never poisoned");
    for scene in scenes {
        let Some(reference) = scene.get("scene").and_then(toml::Value::as_str) else { continue };
        let Some(guid) = AssetRef::parse(reference, db).ok().and_then(|r| r.guid()) else { continue };
        let weight = scene.get("weight").and_then(|w| w.as_float().or(w.as_integer().map(|i| i as f64))).unwrap_or(1.0);
        weights.entry(guid).or_insert((0.0, 0, 0)).0 = weight.max(0.0) as f32;
    }
}

/// Runs on the worker: everything `root` needs that is not loaded already.
fn gather(
    db: &AssetDb,
    reader: &mut dyn AssetReader,
    root: AssetGuid,
    loaded: &HashSet<AssetGuid>,
    models: Models,
    progress: &Progress,
) -> Result<Prepared> {
    let mut prepared = Prepared::default();
    let mut seen = HashSet::new();
    // Each file with the scene it is read for, so a level's progress can
    // count each scene for its weight.
    let mut wave: Vec<(AssetGuid, Option<AssetGuid>)> = vec![(root, None)];
    while !wave.is_empty() {
        let mut reads = Vec::new();
        for (guid, owner) in std::mem::take(&mut wave) {
            if !seen.insert(guid) {
                continue;
            }
            if loaded.contains(&guid) {
                // A scene already loaded counts as done: nothing found, nothing to do.
                if owner == Some(guid) {
                    progress.scenes.lock().expect("never poisoned").entry(guid).or_insert((1.0, 0, 0));
                }
                continue;
            }
            let entry = db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
            let read = match entry.kind {
                AssetKind::Level | AssetKind::Scene | AssetKind::Prefab | AssetKind::Material | AssetKind::Sound => {
                    true
                }
                // A source model reads its own buffers and images as it is prepared.
                AssetKind::Model => models.work != ModelWork::Skip && models.cooked.contains(&guid),
                // Textures load with their models, scripts load UI documents themselves, and the
                // game reads its input actions and string tables when it starts, and its sound
                // the paths round the level's walls once it plays.
                AssetKind::Texture
                | AssetKind::Ui
                | AssetKind::InputActions
                | AssetKind::Strings
                | AssetKind::Paths => false,
            };
            if owner == Some(guid) {
                progress.scenes.lock().expect("never poisoned").entry(guid).or_insert((1.0, 0, 0));
            }
            progress.count(owner, false);
            reads.push((guid, owner, entry, read));
        }
        let paths: Vec<&str> = reads.iter().filter(|(.., read)| *read).map(|(_, _, e, _)| e.path.as_str()).collect();
        let read_files = voxear_core::profile_marker!("Read files").scope();
        let mut bytes = reader.read_many(&paths)?.into_iter();
        drop(read_files);
        for (guid, owner, entry, read) in reads {
            let data = if read { bytes.next() } else { None };
            match (entry.kind, data) {
                (AssetKind::Model, Some(data)) => {
                    voxear_core::profile_scope!("Unpack model");
                    let mut model = CookedModel::from_bytes(&entry.path, &data)?;
                    if models.work == (ModelWork::Prepare { decompress: true }) {
                        model.decompress()?;
                    }
                    prepared.models.push((guid, ReadyModel::Cooked(model)));
                }
                (AssetKind::Sound, Some(data)) => {
                    voxear_core::profile_scope!("Decode sound");
                    let settings = sound_import(reader, &entry.path);
                    prepared.sounds.push((guid, load_sound(data, &entry.path, settings)?));
                }
                (AssetKind::Model, None) if models.work != ModelWork::Skip => {
                    voxear_core::profile_scope!("Prepare model");
                    let model = voxear_render::prepare_model(&entry.path, &mut |file| reader.read(file))?;
                    prepared.models.push((guid, ReadyModel::Source(model)));
                }
                (_, Some(data)) => {
                    let text = String::from_utf8(data).with_context(|| format!("{} is not UTF-8 text", entry.path))?;
                    let table: toml::Table =
                        toml::from_str(&text).with_context(|| format!("reading {}", entry.path))?;
                    if entry.kind == AssetKind::Level {
                        note_weights(db, &table, progress);
                    }
                    for need in direct_needs(db, &table) {
                        let scene = db.get(need).is_some_and(|e| e.kind == AssetKind::Scene);
                        wave.push((need, owner.or(scene.then_some(need))));
                    }
                    prepared.texts.insert(guid, text.into());
                }
                (_, None) => {}
            }
            progress.count(owner, true);
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_levels_progress_counts_each_scene_for_its_weight() {
        let progress = Progress::default();
        let (big, small, kept) = (AssetGuid::new_random(), AssetGuid::new_random(), AssetGuid::new_random());
        progress.scenes.lock().unwrap().extend([(big, (3.0, 4, 2)), (small, (1.0, 2, 2)), (kept, (1.0, 0, 0))]);
        // Half of a scene worth 3, and two whole scenes worth 1: (1.5 + 1 + 1) / 5.
        assert!((progress.reading() - 0.7).abs() < 1e-6, "{}", progress.reading());

        let prefab = Progress::default();
        prefab.found.store(4, Ordering::Relaxed);
        prefab.done.store(1, Ordering::Relaxed);
        assert_eq!(prefab.reading(), 0.25, "without scenes, every file counts alike");
    }
}
