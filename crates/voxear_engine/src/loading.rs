//! Loading in the background, so the game keeps running while a level or a
//! prefab loads.
//!
//! A worker thread, with its own reader, reads and parses every level,
//! scene, prefab and material file the target needs, and prepares its models:
//! parsing, decoding images and building LODs. It reads in waves, one batch
//! per step down the dependency graph (the level, then its scenes, then what
//! they use), so the IO system works on each wave's reads together. The main thread then uploads
//! the models a few at a time within a per-frame budget, and finishes. A
//! level waits as **ready** until activation is allowed, then changes over
//! at the end of a frame, like `Level.Load`.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use voxear_assets::{AssetDb, AssetGuid, AssetKind, AssetReader};
use voxear_render::{CookedModel, PreparedModel, Renderer};

use crate::{content::Content, content_build::direct_needs, prefab::PrefabId};

/// How long uploads may take per frame, across all loads. At least one model
/// uploads each frame, however long it takes.
const UPLOAD_BUDGET: Duration = Duration::from_millis(4);

/// A background load, by the id scripts hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoadId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum LoadTarget {
    Level(String),
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
}

/// Counted by the worker, read by the main thread.
#[derive(Default)]
struct Progress {
    found: AtomicU32,
    done: AtomicU32,
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
        let reference = match &target {
            LoadTarget::Level(path) | LoadTarget::Prefab(path) => path,
        };
        let root = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not an asset"))?;
        let db = content.shared_db();
        let mut reader = content.worker_reader();
        let loaded = content.loaded();
        let cooked = content.cooked_models();
        let progress = Arc::new(Progress::default());
        let counter = progress.clone();
        let job = std::thread::Builder::new().name(format!("load {reference}")).spawn(move || {
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
            allow_activation: true,
            status: LoadStatus { state: LoadState::Loading, progress: 0.0, prefab: None },
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

    /// Models that loads still in progress have uploaded.
    pub fn uploaded_models(&self) -> HashSet<AssetGuid> {
        self.active.iter().flat_map(|l| l.uploaded.iter().copied()).collect()
    }

    /// Moves every load along: collects finished workers, uploads within the
    /// frame budget, and returns the levels that may now change over. Call
    /// once per frame, after the engine ticks.
    pub fn advance(&mut self, content: &mut Content, mut renderer: Option<&mut Renderer>) -> Vec<(LoadId, String)> {
        let started = Instant::now();
        let mut activate = Vec::new();
        for load in &mut self.active {
            let step = advance_one(load, content, renderer.as_deref_mut(), started);
            match step {
                Ok(Some(level)) => activate.push((load.id, level)),
                Ok(None) => {}
                Err(error) => load.status.state = LoadState::Failed(format!("{error:#}")),
            }
        }
        self.retire();
        activate
    }

    /// Records the outcome of a level change [`advance`](Self::advance) asked for.
    pub fn activated(&mut self, id: LoadId, result: Result<()>) {
        if let Some(load) = self.active.iter_mut().find(|l| l.id == id) {
            load.status.state = match result {
                Ok(()) => LoadState::Done,
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

/// One frame's work on one load. Returns a level to change to once it may.
fn advance_one(
    load: &mut Load,
    content: &mut Content,
    mut renderer: Option<&mut Renderer>,
    frame_started: Instant,
) -> Result<Option<String>> {
    if load.job.as_ref().is_some_and(|job| job.is_finished()) {
        let job = load.job.take().expect("checked above");
        let prepared = job.join().map_err(|_| anyhow::anyhow!("the loading thread panicked"))??;
        load.prepared = Some(prepared);
    }
    let Some(prepared) = &mut load.prepared else {
        let found = load.progress.found.load(Ordering::Relaxed).max(1);
        let reading = load.progress.done.load(Ordering::Relaxed) as f32 / found as f32;
        load.status.progress = load.status.progress.max(reading * 0.8);
        return Ok(None);
    };

    if let Some(renderer) = renderer.as_deref_mut() {
        let total = load.uploads + prepared.models.len();
        while let Some((guid, model)) = prepared.models.pop() {
            content.adopt_model(renderer, guid, &model)?;
            load.uploads += 1;
            load.uploaded.push(guid);
            load.status.progress = 0.8 + 0.1 * load.uploads as f32 / total as f32;
            if frame_started.elapsed() > UPLOAD_BUDGET {
                return Ok(None);
            }
        }
    }
    // Texts go to content only when used, so a level held at ready keeps its
    // own through any sweep another level change makes.
    match &load.target {
        LoadTarget::Prefab(path) => {
            content.adopt_texts(std::mem::take(&mut prepared.texts));
            load.status.prefab = Some(content.prefab(renderer, path)?);
            load.status.state = LoadState::Done;
            load.status.progress = 1.0;
            Ok(None)
        }
        LoadTarget::Level(path) => {
            load.status.state = LoadState::Ready;
            load.status.progress = 0.9;
            if !load.allow_activation {
                return Ok(None);
            }
            content.adopt_texts(std::mem::take(&mut prepared.texts));
            Ok(Some(path.clone()))
        }
    }
}

/// How the worker treats models.
struct Models<'a> {
    work: ModelWork,
    /// Those the content build cooked, which only need reading.
    cooked: &'a HashSet<AssetGuid>,
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
    let mut wave = vec![root];
    while !wave.is_empty() {
        let mut reads = Vec::new();
        for guid in wave.drain(..) {
            if !seen.insert(guid) || loaded.contains(&guid) {
                continue;
            }
            let entry = db.get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
            let read = match entry.kind {
                AssetKind::Level | AssetKind::Scene | AssetKind::Prefab | AssetKind::Material => true,
                // A source model reads its own buffers and images as it is prepared.
                AssetKind::Model => models.work != ModelWork::Skip && models.cooked.contains(&guid),
                AssetKind::Texture => false,
            };
            progress.found.fetch_add(1, Ordering::Relaxed);
            reads.push((guid, entry, read));
        }
        let paths: Vec<&str> = reads.iter().filter(|(.., read)| *read).map(|(_, e, _)| e.path.as_str()).collect();
        let mut bytes = reader.read_many(&paths)?.into_iter();
        for (guid, entry, read) in reads {
            let data = if read { bytes.next() } else { None };
            match (entry.kind, data) {
                (AssetKind::Model, Some(data)) => {
                    let mut model = CookedModel::from_bytes(&entry.path, &data)?;
                    if models.work == (ModelWork::Prepare { decompress: true }) {
                        model.decompress()?;
                    }
                    prepared.models.push((guid, ReadyModel::Cooked(model)));
                }
                (AssetKind::Model, None) if models.work != ModelWork::Skip => {
                    let model = voxear_render::prepare_model(&entry.path, &mut |file| reader.read(file))?;
                    prepared.models.push((guid, ReadyModel::Source(model)));
                }
                (_, Some(data)) => {
                    let text = String::from_utf8(data).with_context(|| format!("{} is not UTF-8 text", entry.path))?;
                    let table: toml::Table =
                        toml::from_str(&text).with_context(|| format!("reading {}", entry.path))?;
                    wave.extend(direct_needs(db, &table));
                    prepared.texts.insert(guid, text.into());
                }
                (_, None) => {}
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
        }
    }
    Ok(prepared)
}
