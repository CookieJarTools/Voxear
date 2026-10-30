use anyhow::{Context, Result};
use voxear_ecs::{Entity, World};
use voxear_render::Renderer;
use voxear_scene::Placement;
use voxear_scripting::{LoadProgress, ScriptServices, ServicesScope, SpawnedPrefab};

use crate::{
    content::Content,
    engine::Engine,
    levels::{LevelChange, Levels},
    loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads},
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
    /// Whether frames render, so background loads know to prepare models.
    rendering: bool,
}

impl Game {
    /// A game reading assets from the project folder.
    pub fn new(project: Project, engine: Engine) -> Result<Self> {
        let content = Content::open(&project.root)?;
        Ok(Self::with_content(project, engine, content))
    }

    /// A game reading assets from `content`, such as built bundles.
    pub fn with_content(project: Project, engine: Engine, content: Content) -> Self {
        Self { engine, content, levels: Levels::default(), loads: Loads::default(), project, rendering: false }
    }

    /// Loads the project's `start_level`.
    pub fn start(&mut self, renderer: Option<&mut Renderer>) -> Result<()> {
        self.rendering = renderer.is_some();
        let level = self
            .project
            .file
            .project
            .start_level
            .clone()
            .with_context(|| format!("{} has no start_level in voxear.toml", self.project.name()))?;
        self.load_level(&level, renderer)
    }

    pub fn load_level(&mut self, level: &str, renderer: Option<&mut Renderer>) -> Result<()> {
        let change = self.levels.load(&mut self.engine, &mut self.content, renderer, level)?;
        log_change(level, &change);
        Ok(())
    }

    /// Starts loading a level in the background. It changes over by itself
    /// once loaded, at the end of a frame, unless activation is held back
    /// with [`Loads::set_allow_activation`].
    pub fn load_level_async(&mut self, level: &str) -> Result<LoadId> {
        self.loads.start(&self.content, LoadTarget::Level(level.to_string()), self.rendering)
    }

    /// Starts loading a prefab in the background; its status gives the
    /// prefab once done.
    pub fn load_prefab_async(&mut self, prefab: &str) -> Result<LoadId> {
        self.loads.start(&self.content, LoadTarget::Prefab(prefab.to_string()), self.rendering)
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
        let template = self.content.template(id).expect("just loaded").clone();
        let rendering = self.rendering;
        let mut services = GameServices { content: &mut self.content, loads: &mut self.loads, renderer, rendering };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.instantiate(&template, parent, placement)
    }

    /// Runs one frame, then any level change scripts asked for. A level that
    /// fails to load is logged and the game carries on, as in Unity.
    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        self.rendering = renderer.is_some();
        {
            let renderer = renderer.as_deref_mut();
            let rendering = self.rendering;
            let mut services = GameServices { content: &mut self.content, loads: &mut self.loads, renderer, rendering };
            let _scope = ServicesScope::enter(&mut services);
            self.engine.tick(delta);
        }
        for (id, level) in self.loads.advance(&mut self.content, renderer.as_deref_mut()) {
            let changed = self.load_level(&level, renderer.as_deref_mut());
            self.loads.activated(id, changed);
        }
        let requested = self.engine.scripts_mut().and_then(|scripts| scripts.take_level_requests().pop());
        if let Some(level) = requested
            && let Err(error) = self.load_level(&level, renderer)
        {
            tracing::error!("could not load level {level}: {error:#}");
        }
    }
}

/// What scripts reach through native calls while the game runs: content,
/// and the renderer to upload what they load.
struct GameServices<'a> {
    content: &'a mut Content,
    loads: &'a mut Loads,
    renderer: Option<&'a mut Renderer>,
    rendering: bool,
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
        let template = self.content.template(PrefabId(prefab)).context("no prefab with that id is loaded")?;
        let root = template.root().context("the prefab has no Objs")?;
        let entities = template.spawn_objs(world, parent, placement);
        let objs =
            entities.iter().enumerate().map(|(i, &entity)| (entity, template.behaviours(i, &entities))).collect();
        Ok(SpawnedPrefab { root: entities[root], objs })
    }

    fn load_async(&mut self, level: bool, reference: &str) -> Result<u32> {
        let target = if level { LoadTarget::Level(reference.into()) } else { LoadTarget::Prefab(reference.into()) };
        Ok(self.loads.start(self.content, target, self.rendering)?.0)
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
