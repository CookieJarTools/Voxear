use anyhow::{Context, Result};
use voxear_core::{
    math::{Quat, Vec2, Vec3},
    profile_scope,
};
use voxear_ecs::{Entity, World};
use voxear_render::{MAX_PREVIEW_SIZE, PreviewId, PreviewOptions, Renderer};
use voxear_scene::Placement;
use voxear_scripting::{
    ContentStats, LevelInfo, LevelLoadOptions, LoadProgress, PreviewOptions as ScriptPreviewOptions, PreviewSource,
    PreviewState, ScriptServices, ServicesScope, SpawnedPrefab, UiAnswer, UiAsk,
};

use crate::{
    content::{Content, Released},
    engine::Engine,
    game_ui::{ClassChange, GameUi},
    levels::{LevelChange, LevelOptions, Levels},
    loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads, ModelWork},
    prefab::PrefabId,
    previews::ScriptPreviews,
    project::Project,
    scripts::ScriptWatcher,
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
    /// Rebuilds the scripts as they change, when the game reloads them itself.
    watcher: Option<ScriptWatcher>,
    /// Previews scripts asked for.
    previews: ScriptPreviews,
    /// The game's own UI, which its scripts load documents into.
    pub ui: GameUi,
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
            watcher: None,
            previews: ScriptPreviews::default(),
            ui: GameUi::default(),
        }
    }

    /// Rebuilds the scripts when they change and reloads them into the
    /// running game, keeping it going (see [`Engine::reload_scripts`]), as
    /// `vox run` does. The editor reloads them its own way.
    pub fn watch_scripts(&mut self) {
        if self.project.scripts_project().is_some() {
            self.watcher = Some(ScriptWatcher::new(&self.project));
        }
    }

    /// Reloads the scripts once a build that the watcher started has finished.
    fn reload_changed_scripts(&mut self) {
        let Some(finished) = self.watcher.as_mut().and_then(|watcher| watcher.poll(&self.project)) else { return };
        if let Err(error) = finished {
            return tracing::error!("the changed scripts did not build, so the old ones keep running: {error:#}");
        }
        if !self.engine.scripts().is_some_and(|scripts| scripts.is_out_of_date()) {
            return;
        }
        match self.engine.reload_scripts() {
            Ok(()) => tracing::info!("reloaded the changed scripts into the running game"),
            Err(error) => tracing::error!("{error:#}"),
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
    pub fn load_level(&mut self, level: &str, renderer: Option<&mut Renderer>) -> Result<()> {
        self.load_level_with(level, &LevelOptions::default(), renderer).map(|_| ())
    }

    /// Changes level now, as `options` say, and returns what changed.
    pub fn load_level_with(
        &mut self,
        level: &str,
        options: &LevelOptions,
        mut renderer: Option<&mut Renderer>,
    ) -> Result<LevelChange> {
        let change =
            self.levels.load_with(&mut self.engine, &mut self.content, renderer.as_deref_mut(), level, options)?;
        log_change(level, &change);
        if options.tidy_up {
            self.release_unused(renderer);
        }
        Ok(change)
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
        self.load_level_async_with(level, LevelOptions::default())
    }

    /// Starts loading a level in the background that changes over as
    /// `options` say: held for a loading screen, or taking a least time.
    pub fn load_level_async_with(&mut self, level: &str, options: LevelOptions) -> Result<LoadId> {
        self.loads.start_with(&self.content, LoadTarget::Level(level.to_string()), options, self.models)
    }

    /// Changes level in the background as a running game would, as the
    /// editor's Levels panel does: through the scripts' `Level.LoadAsync`,
    /// so every event fires, or straight to the loader without scripts.
    pub fn switch_level(&mut self, level: &str, renderer: Option<&mut Renderer>) -> Result<()> {
        if self.engine.scripts().is_none() {
            return self.load_level_async(level).map(|_| ());
        }
        let models = self.models;
        let unload_requested = &mut self.unload_requested;
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer,
            models,
            unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.switch_level_through_scripts(level);
        Ok(())
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
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer,
            models,
            unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.instantiate(&template, parent, placement)
    }

    /// Runs one frame, then any level change scripts asked for. A level that
    /// fails to load is logged and the game carries on, as in Unity.
    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        {
            profile_scope!("Script Reload Check");
            self.reload_changed_scripts();
        }
        self.models = ModelWork::for_renderer(renderer.as_deref());
        {
            let renderer = renderer.as_deref_mut();
            let models = self.models;
            let unload_requested = &mut self.unload_requested;
            let mut services = GameServices {
                content: &mut self.content,
                loads: &mut self.loads,
                levels: &self.levels,
                renderer,
                models,
                unload_requested,
                previews: &mut self.previews,
                ui: &mut self.ui,
            };
            let _scope = ServicesScope::enter(&mut services);
            self.engine.tick(delta);
        }
        profile_scope!("Loading");
        for (id, level, options) in self.loads.advance(&mut self.content, renderer.as_deref_mut()) {
            let changed = self.load_level_with(&level, &options, renderer.as_deref_mut());
            self.loads.activated(id, changed);
        }
        let requested = self.engine.scripts_mut().and_then(|scripts| scripts.take_level_requests().pop());
        if let Some(level) = requested
            && let Err(error) = self.load_level(&level, renderer.as_deref_mut())
        {
            tracing::error!("could not load level {level}: {error:#}");
        }
        if let Some(renderer) = renderer.as_deref_mut() {
            self.previews.finish_saves(renderer);
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
    levels: &'a Levels,
    renderer: Option<&'a mut Renderer>,
    models: ModelWork,
    unload_requested: &'a mut bool,
    previews: &'a mut ScriptPreviews,
    ui: &'a mut GameUi,
}

impl ScriptServices for GameServices<'_> {
    fn ui_load(&mut self, path: &str) -> Result<u32> {
        self.ui.load(self.content, path)
    }

    fn ui_find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.ui.find(document, name)
    }

    fn ui_close(&mut self, document: u32) {
        self.ui.close(document);
    }

    fn ui_ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        let ui = &mut *self.ui;
        match ask {
            UiAsk::Text => ui.text(element).map_or(UiAnswer::None, UiAnswer::Text),
            UiAsk::SetText(text) => {
                ui.set_text(element, text);
                UiAnswer::None
            }
            UiAsk::Class(class, change) => {
                let change = match change {
                    0 => ClassChange::Remove,
                    1 => ClassChange::Add,
                    2 => ClassChange::Toggle,
                    _ => ClassChange::Has,
                };
                UiAnswer::Yes(ui.class(element, class, change))
            }
            UiAsk::Visible => UiAnswer::Yes(ui.visible(element)),
            UiAsk::SetVisible(visible) => {
                ui.set_visible(element, visible);
                UiAnswer::None
            }
            UiAsk::Clicked => UiAnswer::Yes(ui.clicked(element)),
            UiAsk::Value => UiAnswer::Number(ui.value(element)),
            UiAsk::SetValue(value) => {
                ui.set_value(element, value);
                UiAnswer::None
            }
            UiAsk::SetStyle(style) => {
                ui.set_style(element, style);
                UiAnswer::None
            }
        }
    }

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

    fn load_level(&mut self, reference: &str, options: LevelLoadOptions) -> Result<u32> {
        let options = LevelOptions {
            hold: options.hold != 0,
            least_time: options.least_time.max(0.0),
            unload_rest: options.unload_rest != 0,
            set_active: options.set_active != 0,
            tidy_up: options.tidy_up != 0,
        };
        let target = LoadTarget::Level(reference.to_string());
        Ok(self.loads.start_with(self.content, target, options, self.models)?.0)
    }

    fn level_info(&self, what: LevelInfo, id: u32) -> Option<String> {
        match what {
            LevelInfo::Current => self.levels.current().map(str::to_string),
            LevelInfo::ActiveScene => self.levels.active_scene().map(str::to_string),
            LevelInfo::Scenes => self.loads.status(LoadId(id)).map(|status| status.scenes.join("\n")),
            LevelInfo::Error => match self.loads.status(LoadId(id))?.state {
                LoadState::Failed(error) => Some(error),
                _ => None,
            },
        }
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

    fn create_preview(&mut self, source: PreviewSource<'_>, options: &ScriptPreviewOptions) -> Result<u32> {
        let renderer = self.renderer.as_deref_mut().context("previews need the renderer")?;
        let reference = match source {
            PreviewSource::Asset(reference) => reference.to_string(),
            PreviewSource::Prefab(prefab) => {
                self.content.prefab_guid(PrefabId(prefab)).context("no such prefab is loaded")?.to_string()
            }
        };
        let options = PreviewOptions {
            size: options.size.clamp(1, MAX_PREVIEW_SIZE),
            background: options.background,
            rotation: Quat::from_array(options.rotation).normalize(),
            offset: Vec2::from(options.offset),
            zoom: options.zoom,
            fov_degrees: options.fov_degrees,
            light_from: Vec3::from(options.light_from),
            light_color: Vec3::from(options.light_color),
            light_intensity: options.light_intensity,
            ambient: options.ambient,
            exposure: options.exposure,
        };
        Ok(self.previews.create(self.content, renderer, &reference, options)?.0)
    }

    fn preview_state(&mut self, preview: u32) -> Option<PreviewState> {
        let renderer = self.renderer.as_deref_mut()?;
        let image = self.previews.image(renderer, PreviewId(preview))?;
        Some(image.map_or_else(PreviewState::default, |image| PreviewState {
            ready: 1,
            texture: image.texture.0,
            region: image.uv,
        }))
    }

    fn save_preview(&mut self, preview: u32, path: &str) -> Result<()> {
        let renderer = self.renderer.as_deref_mut().context("previews need the renderer")?;
        self.previews.save(renderer, PreviewId(preview), std::path::Path::new(path))
    }

    fn free_preview(&mut self, preview: u32) {
        if let Some(renderer) = self.renderer.as_deref_mut() {
            self.previews.free(renderer, PreviewId(preview));
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
