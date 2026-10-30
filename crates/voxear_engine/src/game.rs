use anyhow::{Context, Result};
use voxear_ecs::{Entity, World};
use voxear_render::Renderer;
use voxear_scene::Placement;
use voxear_scripting::{ContentStats, LoadProgress, ScriptServices, ServicesScope, SpawnedPrefab};

use crate::{
    content::{Content, Released},
    engine::Engine,
    levels::{LevelChange, Levels},
    loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads, ModelWork},
    prefab::PrefabId,
    project::Project,
};

/// A running project: the engine with its content and levels. The window,
/// `vox run` and `vox render` all drive one of these.
pub struct Game {
    pub engine: Engine,
    pub content: Content,
    pub levels: Levels,
    pub loads: Loads,
    pub project: Project,
    /// What background loads do with models, which depends on the renderer.
    models: ModelWork,
    /// Scripts asked for a sweep, which runs at the end of the frame.
    unload_requested: bool,
}

impl Game {
    /// A game reading assets from the project folder.
    pub fn new(project: Project, engine: Engine) -> Result<Self> {
        let content = Content::open(&project.root)?;
        Ok(Self::with_content(project, engine, content))
    }

    /// A game reading assets from `content`, such as built bundles.
    pub fn with_content(project: Project, engine: Engine, content: Content) -> Self {
        Self {
            engine,
            content,
            levels: Levels::default(),
            loads: Loads::default(),
            project,
            models: ModelWork::Skip,
            unload_requested: false,
        }
    }

    /// Loads the project's `start_level`.
    pub fn start(&mut self, renderer: Option<&mut Renderer>) -> Result<()> {
        self.models = ModelWork::for_renderer(renderer.as_deref());
        let level = self
            .project
            .file
            .project
            .start_level
            .clone()
            .with_context(|| format!("{} has no start_level in voxear.toml", self.project.name()))?;
        self.load_level(&level, renderer)
    }

    /// Changes level now, then frees what the old one used and the new one
    /// does not.
    pub fn load_level(&mut self, level: &str, mut renderer: Option<&mut Renderer>) -> Result<()> {
        let change = self.levels.load(&mut self.engine, &mut self.content, renderer.as_deref_mut(), level)?;
        log_change(level, &change);
        self.release_unused(renderer);
        Ok(())
    }

    /// Frees what nothing uses any more (see [`Content::release_unused`]).
    /// Runs after every level change, and when scripts call
    /// `Content.UnloadUnused`.
    pub fn release_unused(&mut self, renderer: Option<&mut Renderer>) -> Released {
        let keep = self.loads.uploaded_models();
        let released = self.content.release_unused(&self.engine.world, renderer, &keep);
        if !released.is_empty() {
            let Released { prefabs, models, meshes, materials, textures } = released;
            tracing::info!(
                "released {prefabs} prefabs and {models} models: {meshes} meshes, {materials} materials, {textures} textures"
            );
        }
        released
    }

    /// Starts loading a level in the background. It changes over by itself
    /// once loaded, at the end of a frame, unless activation is held back
    /// with [`Loads::set_allow_activation`].
    pub fn load_level_async(&mut self, level: &str) -> Result<LoadId> {
        self.loads.start(&self.content, LoadTarget::Level(level.to_string()), self.models)
    }

    /// Starts loading a prefab in the background; its status gives the
    /// prefab once done.
    pub fn load_prefab_async(&mut self, prefab: &str) -> Result<LoadId> {
        self.loads.start(&self.content, LoadTarget::Prefab(prefab.to_string()), self.models)
    }

    pub fn load_status(&self, id: LoadId) -> Option<LoadStatus> {
        self.loads.status(id)
    }

    /// Instantiates a prefab by GUID or project path, loading it first if
    /// needed. Awake and OnEnable run before this returns; see
    /// [`Engine::instantiate`].
    pub fn instantiate(
        &mut self,
        prefab: &str,
        parent: Option<Entity>,
        placement: Option<Placement>,
        mut renderer: Option<&mut Renderer>,
    ) -> Result<Entity> {
        let id = self.content.prefab(renderer.as_deref_mut(), prefab)?;
        let template = self.content.loaded_template(renderer.as_deref_mut(), id)?;
        let models = self.models;
        let unload_requested = &mut self.unload_requested;
        let mut services =
            GameServices { content: &mut self.content, loads: &mut self.loads, renderer, models, unload_requested };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.instantiate(&template, parent, placement)
    }

    /// Runs one frame, then any level change scripts asked for. A level that
    /// fails to load is logged and the game carries on, as in Unity.
    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        self.models = ModelWork::for_renderer(renderer.as_deref());
        {
            let renderer = renderer.as_deref_mut();
            let models = self.models;
            let unload_requested = &mut self.unload_requested;
            let mut services =
                GameServices { content: &mut self.content, loads: &mut self.loads, renderer, models, unload_requested };
            let _scope = ServicesScope::enter(&mut services);
            self.engine.tick(delta);
        }
        for (id, level) in self.loads.advance(&mut self.content, renderer.as_deref_mut()) {
            let changed = self.load_level(&level, renderer.as_deref_mut());
            self.loads.activated(id, changed);
        }
        let requested = self.engine.scripts_mut().and_then(|scripts| scripts.take_level_requests().pop());
        if let Some(level) = requested
            && let Err(error) = self.load_level(&level, renderer.as_deref_mut())
        {
            tracing::error!("could not load level {level}: {error:#}");
        }
        if std::mem::take(&mut self.unload_requested) {
            self.release_unused(renderer);
        }
    }
}

/// What scripts reach through native calls while the game runs: content,
/// and the renderer to upload what they load.
struct GameServices<'a> {
    content: &'a mut Content,
    loads: &'a mut Loads,
    renderer: Option<&'a mut Renderer>,
    models: ModelWork,
    unload_requested: &'a mut bool,
}

impl ScriptServices for GameServices<'_> {
    fn load_prefab(&mut self, reference: &str) -> Result<u32> {
        Ok(self.content.prefab(self.renderer.as_deref_mut(), reference)?.0)
    }

    fn spawn_prefab(
        &mut self,
        world: &mut World,
        prefab: u32,
        parent: Option<Entity>,
        placement: Option<Placement>,
    ) -> Result<SpawnedPrefab> {
        let template = self.content.loaded_template(self.renderer.as_deref_mut(), PrefabId(prefab))?;
        let root = template.root().context("the prefab has no Objs")?;
        let entities = template.spawn_objs(world, parent, placement);
        let objs =
            entities.iter().enumerate().map(|(i, &entity)| (entity, template.behaviours(i, &entities))).collect();
        Ok(SpawnedPrefab { root: entities[root], objs })
    }

    fn load_async(&mut self, level: bool, reference: &str) -> Result<u32> {
        let target = if level { LoadTarget::Level(reference.into()) } else { LoadTarget::Prefab(reference.into()) };
        Ok(self.loads.start(self.content, target, self.models)?.0)
    }

    fn load_status(&self, id: u32) -> Option<LoadProgress> {
        let status = self.loads.status(LoadId(id))?;
        let state = match status.state {
            LoadState::Loading => 0,
            LoadState::Ready => 1,
            LoadState::Done => 2,
            LoadState::Failed(_) => 3,
        };
        let prefab = status.prefab.map_or(-1, |p| p.0 as i32);
        Some(LoadProgress { state, progress: status.progress, prefab })
    }

    fn allow_activation(&mut self, id: u32, allow: bool) {
        self.loads.set_allow_activation(LoadId(id), allow);
    }

    fn keep_prefab(&mut self, prefab: u32, keep: bool) {
        self.content.keep_prefab(PrefabId(prefab), keep);
    }

    fn unload_unused(&mut self) {
        *self.unload_requested = true;
    }

    fn usage(&self) -> ContentStats {
        let content = self.content.usage();
        let gpu = self.renderer.as_ref().map(|r| r.asset_usage()).unwrap_or_default();
        ContentStats {
            models: content.models as u32,
            prefabs: content.prefabs as u32,
            kept_prefabs: content.kept_prefabs as u32,
            materials: content.materials as u32,
            gpu_meshes: gpu.meshes,
            gpu_materials: gpu.materials,
            gpu_textures: gpu.textures,
            gpu_retiring: gpu.retiring,
        }
    }
}

fn log_change(level: &str, change: &LevelChange) {
    let list = |scenes: &[String]| if scenes.is_empty() { "none".to_string() } else { scenes.join(", ") };
    tracing::info!(
        "level {level}: loaded {} · kept {} · unloaded {}",
        list(&change.loaded),
        list(&change.kept),
        list(&change.unloaded)
    );
}
