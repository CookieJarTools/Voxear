use anyhow::{Context, Result};
use voxear_ecs::{Entity, World};
use voxear_render::Renderer;
use voxear_scene::Placement;
use voxear_scripting::{ScriptServices, ServicesScope, SpawnedPrefab};

use crate::{
    content::Content,
    engine::Engine,
    levels::{LevelChange, Levels},
    prefab::PrefabId,
    project::Project,
};

/// A running project: the engine with its content and levels. The window,
/// `vox run` and `vox render` all drive one of these.
pub struct Game {
    pub engine: Engine,
    pub content: Content,
    pub levels: Levels,
    pub project: Project,
}

impl Game {
    pub fn new(project: Project, engine: Engine) -> Result<Self> {
        let content = Content::open(&project.root)?;
        Ok(Self { engine, content, levels: Levels::default(), project })
    }

    /// Loads the project's `start_level`.
    pub fn start(&mut self, renderer: Option<&mut Renderer>) -> Result<()> {
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
        let mut services = GameServices { content: &mut self.content, renderer };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.instantiate(&template, parent, placement)
    }

    /// Runs one frame, then any level change scripts asked for. A level that
    /// fails to load is logged and the game carries on, as in Unity.
    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        {
            let mut services = GameServices { content: &mut self.content, renderer: renderer.as_deref_mut() };
            let _scope = ServicesScope::enter(&mut services);
            self.engine.tick(delta);
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
    renderer: Option<&'a mut Renderer>,
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
