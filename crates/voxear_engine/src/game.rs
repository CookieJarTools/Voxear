use anyhow::{Context, Result};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_core::{
    math::{Quat, Vec2, Vec3},
    profile_scope,
};
use voxear_ecs::{Entity, World};
use voxear_localization::{Localization, StringTable};
use voxear_render::{MAX_PREVIEW_SIZE, MeshHandle, MeshRenderer, PostProcess, PreviewId, PreviewOptions, Renderer};
use voxear_scene::{Destroying, Parent, Placement, Transform};
use voxear_scripting::{
    AssetName, AudioAsk, ContentStats, CursorLock, LevelInfo, LevelLoadOptions, LoadProgress, PlaySound,
    PreviewOptions as ScriptPreviewOptions, PreviewSource, PreviewState, SceneLoadOptions, SceneRequest,
    ScriptServices, ServicesScope, SoundFacts, SpawnedPrefab, UiAnswer, UiAsk,
};
use voxear_ui::ElementKind;

use crate::{
    audio::{GameAudio, SoundId, sound_facts},
    content::{Content, Reimported, Released},
    engine::Engine,
    game_ui::GameUi,
    levels::{LevelChange, LevelOptions, Levels, is_doomed},
    loading::{LoadId, LoadState, LoadStatus, LoadTarget, Loads, ModelWork},
    physics::{Collider, ColliderMesh, ColliderShape},
    prefab::{ObjPath, PrefabId, Spawned},
    previews::ScriptPreviews,
    project::Project,
    scene_file::{LightingPost, PostTable},
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
    /// The window as scripts see it and ask for it.
    pub display: Display,
    /// The game's sound, and what its Audio Sources play.
    pub audio: GameAudio,
    /// The keyboard, the mouse and gamepads, read as the project's input
    /// actions; whatever shows the game hands it the window's events.
    pub input: voxear_input::Input,
    /// The lookup table the post-processing names that could not be loaded,
    /// said once rather than every frame.
    lut_failed: Option<String>,
}

/// The game's window as scripts see it and ask for it, through `Screen` and
/// `Cursor` in C#: whatever shows the game sets its size each frame, and
/// afterwards makes what scripts asked for.
#[derive(Clone, Debug)]
pub struct Display {
    /// The window's size in pixels, and whether it fills the screen.
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    /// A size and full screen asked for and not yet made.
    pub asked: Option<(u32, u32, bool)>,
    /// Whether the mouse shows, and how it is held, as scripts asked.
    pub cursor_visible: bool,
    pub cursor_lock: CursorLock,
    /// The mouse was asked to change since it was last made so.
    pub cursor_changed: bool,
}

impl Default for Display {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            fullscreen: false,
            asked: None,
            cursor_visible: true,
            cursor_lock: CursorLock::None,
            cursor_changed: false,
        }
    }
}

impl Game {
    /// A game reading assets from the project folder.
    pub fn new(project: Project, engine: Engine) -> Result<Self> {
        let content = Content::open(&project.root)?;
        Ok(Self::with_content(project, engine, content))
    }

    /// A game reading assets from `content`, such as built bundles.
    pub fn with_content(project: Project, engine: Engine, content: Content) -> Self {
        let mut game = Self {
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
            display: Display::default(),
            audio: GameAudio::default(),
            input: voxear_input::Input::new(),
            lut_failed: None,
        };
        game.audio.headphones = game.project.file.audio.headphones;
        game.audio.reverb = game.project.file.audio.reverb;
        game.audio.reverb_mode = game.project.file.audio.reverb_mode;
        game.load_input_actions();
        game.load_strings();
        game.engine.world.insert_resource(game.project.layers());
        game
    }

    /// Reads the project's input actions: its `.inputactions` file, the
    /// first by path when it has several, or else Voxear's own. A package's
    /// are its own, and never the game's.
    pub fn load_input_actions(&mut self) {
        let db = self.content.db();
        let mut files: Vec<(String, AssetGuid)> = (db.entries())
            .filter(|entry| entry.kind == AssetKind::InputActions && entry.mount == voxear_assets::Mount::Project)
            .map(|entry| (entry.path.clone(), entry.guid))
            .collect();
        files.sort();
        let Some((path, guid)) = files.into_iter().next() else { return };
        let read = self.content.asset_text(guid, AssetKind::InputActions);
        if let Err(error) = read.and_then(|(text, _)| self.input.load_actions(&text)) {
            tracing::error!("{path}: {error:#}; the game uses Voxear's own input actions");
        }
    }

    /// Reads every string table and asset table in the project, in path
    /// order, into the world's [`Localization`]. The game starts in the language Steam
    /// says it is in, when the developer ships Steamworks (see [`crate::steam_language`]),
    /// or else the one the player's system is in, or the nearest the project has, or its own
    /// (and as it starts, in the one the player chose last, see [`start`](Self::start));
    /// read again, as when a table changed, it stays in the one it is in.
    pub fn load_strings(&mut self) {
        let languages = &self.project.file.localization.languages;
        let mut localization = match self.engine.world.remove_resource::<Localization>() {
            Some(mut kept) => {
                kept.set_languages(languages);
                kept
            }
            None => {
                let mut fresh = Localization::new(languages);
                // The store's language first, as the player picked it for this game, then the system's.
                let store = crate::steam_language::steam_language();
                if let Some(wanted) = store.or_else(voxear_localization::system_language) {
                    fresh.set_language(&wanted);
                }
                fresh
            }
        };
        localization.clear();
        let db = self.content.db();
        let tables =
            |kind: AssetKind| db.entries().filter(move |e| e.kind == kind).map(move |e| (e.path.clone(), e.guid, kind));
        let mut files: Vec<(String, AssetGuid, AssetKind)> =
            tables(AssetKind::Strings).chain(tables(AssetKind::AssetTable)).collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, guid, kind) in files {
            let read = self.content.asset_text(guid, kind);
            match (read.and_then(|(text, _)| StringTable::parse(&text)), kind) {
                (Ok(table), AssetKind::AssetTable) => localization.add_asset_table(&table),
                (Ok(table), _) => localization.add_table(&table),
                (Err(error), _) => tracing::error!("{path}: {error:#}"),
            }
        }
        self.engine.world.insert_resource(localization);
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
        self.audio.start_game();
        // The language the player chose last time, kept beside the saves, over the system's.
        if let Some(saved) = voxear_scripting::saved_language()
            && let Some(localization) = self.engine.world.resource_mut::<Localization>()
        {
            localization.set_language(&saved);
        }
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
        log_change("level", level, &change);
        if options.tidy_up {
            self.release_unused(renderer);
        }
        Ok(change)
    }

    /// Imports the model at `path` again, as after its import settings or
    /// its file changed (see [`Content::reimport_model`]). Objs drawing a
    /// part of it that was uploaded anew draw the new one. A part placed in
    /// the world that still sits where the model put it moves to where the
    /// model puts it now, as after a change of size; one moved since stays.
    /// A world set aside, as the editor's edited level while the game
    /// plays, draws the new parts too. Returns how many moved.
    pub fn reimport_model(&mut self, renderer: &mut Renderer, path: &str, aside: Option<&mut World>) -> Result<usize> {
        let guid = self.content.db().guid_of(path).with_context(|| format!("{path} is not in the project"))?;
        let Reimported { moved, meshes, materials } = self.content.reimport_model(renderer, guid)?;
        if !meshes.is_empty() || !materials.is_empty() {
            for world in std::iter::once(&mut self.engine.world).chain(aside) {
                for drawn in world.query::<&mut MeshRenderer>() {
                    drawn.mesh = meshes.get(&drawn.mesh).copied().unwrap_or(drawn.mesh);
                    drawn.material = materials.get(&drawn.material).copied().unwrap_or(drawn.material);
                }
            }
        }
        let world = &mut self.engine.world;
        let mut placed = Vec::new();
        for (entity, renderer, transform) in world.query_ref::<(Entity, &MeshRenderer, &Transform)>() {
            let at = (transform.position, transform.rotation, transform.scale);
            let part = moved.iter().find(|part| part.mesh == renderer.mesh && same_place(part.before, at));
            if let Some(part) = part {
                placed.push((entity, part.after));
            }
        }
        for &(entity, (position, rotation, scale)) in &placed {
            world.insert(entity, Transform::new(position, rotation, scale));
        }
        Ok(placed.len())
    }

    /// Frees what nothing uses any more (see [`Content::release_unused`]).
    /// Runs after every level change, and when scripts call
    /// `Content.UnloadUnused`.
    pub fn release_unused(&mut self, renderer: Option<&mut Renderer>) -> Released {
        let keep = self.loads.uploaded_models();
        let released = self.content.release_unused(&self.engine.world, renderer, &keep);
        if !released.is_empty() {
            let Released { prefabs, models, meshes, materials, textures, sounds } = released;
            tracing::info!(
                "released {prefabs} prefabs and {models} models: {meshes} meshes, {materials} materials, {textures} textures; and {sounds} sounds"
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

    /// Loads one scene alongside those loaded now, without a level; see
    /// [`Levels::add_scene`] for what `options` mean for it.
    pub fn add_scene(
        &mut self,
        scene: &str,
        options: &LevelOptions,
        renderer: Option<&mut Renderer>,
    ) -> Result<LevelChange> {
        let change = self.levels.add_scene(&mut self.engine, &mut self.content, renderer, scene, options)?;
        log_change("scene", scene, &change);
        Ok(change)
    }

    /// Starts loading one scene in the background, to come in alongside
    /// those loaded once it is read, as `options` say.
    pub fn load_scene_async(&mut self, scene: &str, options: LevelOptions) -> Result<LoadId> {
        self.loads.start_with(&self.content, LoadTarget::Scene(scene.to_string()), options, self.models)
    }

    /// Unloads one loaded scene, its Objs with it, persistent ones aside.
    /// Returns whether it was loaded.
    pub fn unload_scene(&mut self, scene: &str) -> Result<bool> {
        let guid = self.content.parse(scene)?.guid().with_context(|| format!("`{scene}` is not a scene"))?;
        self.levels.close_scene(&mut self.engine, guid)
    }

    /// Makes a loaded scene the active one. Returns whether it is loaded.
    pub fn set_active_scene(&mut self, scene: &str) -> Result<bool> {
        let guid = self.content.parse(scene)?.guid().with_context(|| format!("`{scene}` is not a scene"))?;
        self.levels.set_active_scene(&mut self.engine, guid)
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
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
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
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.instantiate(&template, parent, placement)
    }

    /// Runs one frame, then any level change scripts asked for. A level that
    /// fails to load is logged and the game carries on, as in Unity.
    /// Loads the colour grading lookup table the level's post-processing
    /// names, `[post.grading] lut`, and gives it to the settings, as they
    /// cannot hold an asset of their own. Runs before each frame is drawn,
    /// in the editor as well as the game, so a level's look shows as it is
    /// edited.
    pub fn prepare_post(&mut self, renderer: &mut Renderer) {
        // The scene's own, or failing that, its lighting settings'.
        let lut_in = |table: &toml::Table| Some(table.get("grading")?.get("lut")?.as_str()?.to_string());
        let world = &self.engine.world;
        let named = world
            .resource::<PostTable>()
            .and_then(|table| lut_in(&table.0))
            .or_else(|| world.resource::<LightingPost>().and_then(|table| lut_in(&table.0)));
        let lut = named.and_then(|path| {
            let loaded = self.content.parse(&path).and_then(|reference| self.content.lut(renderer, &reference));
            match loaded {
                Ok(texture) => Some(texture),
                Err(error) => {
                    if self.lut_failed.as_deref() != Some(path.as_str()) {
                        tracing::error!("the colour lookup table {path}: {error:#}");
                        self.lut_failed = Some(path);
                    }
                    None
                }
            }
        });
        if let Some(post) = self.engine.world.resource_mut::<PostProcess>() {
            post.grading.lut = lut;
        }
    }

    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        {
            profile_scope!("Script Reload Check");
            self.reload_changed_scripts();
        }
        // Read once, before the scripts that ask about it.
        self.input.update(delta);
        self.models = ModelWork::for_renderer(renderer.as_deref());
        give_collider_meshes(&mut self.engine.world, &mut self.content);
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
                display: &mut self.display,
                audio: &mut self.audio,
                input: &mut self.input,
            };
            let _scope = ServicesScope::enter(&mut services);
            self.engine.tick(delta);
        }
        if let Some(renderer) = renderer.as_deref_mut() {
            self.prepare_post(renderer);
        }
        self.ui.follow_language(self.engine.world.resource::<Localization>());
        self.ui.follow_input(&self.input);
        self.ui.show_tooltip(delta);
        // The UI's list and tree views have the scripts fill the rows now on screen, before the frame draws them.
        let rows = self.ui.rows_to_fill();
        if !rows.is_empty() {
            profile_scope!("UI Rows");
            let mut services = GameServices {
                content: &mut self.content,
                loads: &mut self.loads,
                levels: &self.levels,
                renderer: renderer.as_deref_mut(),
                models: self.models,
                unload_requested: &mut self.unload_requested,
                previews: &mut self.previews,
                ui: &mut self.ui,
                display: &mut self.display,
                audio: &mut self.audio,
                input: &mut self.input,
            };
            if let Err(error) = self.engine.fill_ui_rows(&mut services, &rows) {
                tracing::error!(target: "script", "{error:#}");
            }
        }
        profile_scope!("Loading");
        for (id, target, options) in self.loads.advance(&mut self.content, renderer.as_deref_mut()) {
            let changed = match target {
                LoadTarget::Level(level) => self.load_level_with(&level, &options, renderer.as_deref_mut()),
                LoadTarget::Scene(scene) => self.add_scene(&scene, &options, renderer.as_deref_mut()),
                LoadTarget::Prefab(_) => continue,
            };
            self.loads.activated(id, changed);
        }
        let scene_requests = self.engine.scripts_mut().map(|scripts| scripts.take_scene_requests()).unwrap_or_default();
        for request in scene_requests {
            let (done, scene) = match &request {
                SceneRequest::Unload(scene) => (self.unload_scene(scene), scene),
                SceneRequest::SetActive(scene) => (self.set_active_scene(scene), scene),
            };
            match done {
                Ok(true) => {}
                Ok(false) => tracing::warn!(target: "script", "{scene} is not loaded, so {request:?} did nothing"),
                Err(error) => tracing::error!(target: "script", "{request:?}: {error:#}"),
            }
        }
        let requested = self.engine.scripts_mut().and_then(|scripts| scripts.take_level_requests().pop());
        if let Some(level) = requested
            && let Err(error) = self.load_level(&level, renderer.as_deref_mut())
        {
            tracing::error!("could not load level {level}: {error:#}");
        }
        let lighting = self.engine.scripts_mut().and_then(|scripts| scripts.take_lighting_requests().pop());
        if let Some(reference) = lighting
            && let Err(error) = self.levels.apply_lighting(&mut self.engine, &mut self.content, &reference)
        {
            tracing::error!(target: "script", "could not apply lighting {reference}: {error:#}");
        }
        if let Some(renderer) = renderer.as_deref_mut() {
            self.previews.finish_saves(renderer);
        }
        {
            profile_scope!("Audio");
            let running = !self.engine.is_edit_mode();
            if running {
                self.audio.hear_level(self.levels.current(), &mut self.content);
            }
            self.audio.update(&self.engine.world, &mut self.content, running, delta);
        }
        if std::mem::take(&mut self.unload_requested) {
            self.release_unused(renderer);
        }
    }
}

/// How a key for an Obj a script made from a prefab starts (see
/// [`ScriptServices::obj_key`]).
const SPAWNED_KEY: &str = "spawned:";

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
    display: &'a mut Display,
    audio: &'a mut GameAudio,
    input: &'a mut voxear_input::Input,
}

impl ScriptServices for GameServices<'_> {
    fn input(&mut self) -> Option<&mut voxear_input::Input> {
        Some(self.input)
    }

    fn load_sound(&mut self, reference: &str) -> Result<u32> {
        let reference = self.content.parse(reference)?;
        Ok(self.content.sound(&reference)?.0)
    }

    fn sound_info(&mut self, sound: u32) -> Option<SoundFacts> {
        self.content.loaded_sound(SoundId(sound)).ok().map(|loaded| sound_facts(&loaded))
    }

    fn play_sound(&mut self, sound: u32, play: &PlaySound) -> u64 {
        self.audio.play_for_script(self.content, sound, play)
    }

    fn audio(&mut self, world: Option<&mut World>, ask: AudioAsk, set: Option<f32>) -> Option<f32> {
        self.audio.answer(self.content, world, ask, set)
    }

    fn screen(&self) -> (u32, u32, bool) {
        (self.display.width, self.display.height, self.display.fullscreen)
    }

    fn set_resolution(&mut self, width: u32, height: u32, fullscreen: bool) {
        self.display.asked = Some((width.max(1), height.max(1), fullscreen));
    }

    fn cursor(&self) -> (bool, CursorLock) {
        (self.display.cursor_visible, self.display.cursor_lock)
    }

    fn set_cursor(&mut self, visible: bool, lock: CursorLock) {
        (self.display.cursor_visible, self.display.cursor_lock) = (visible, lock);
        self.display.cursor_changed = true;
    }

    fn ui_load(&mut self, path: &str) -> Result<u32> {
        self.ui.load(self.content, path)
    }

    fn ui_find(&mut self, document: u32, name: &str) -> Option<u32> {
        self.ui.find(document, name)
    }

    fn ui_close(&mut self, document: u32) {
        self.ui.close(document);
    }

    fn ui_new(&mut self) -> Result<u32> {
        Ok(self.ui.new_document())
    }

    fn ui_create(&mut self, parent: u32, kind: i32, text: &str) -> Option<u32> {
        self.ui.create(parent, ElementKind::from_number(kind)?, text)
    }

    fn ui_remove(&mut self, element: u32) {
        self.ui.remove(element);
    }

    fn ui_ask(&mut self, element: u32, ask: UiAsk<'_>) -> UiAnswer {
        self.ui.ask(element, ask)
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
        let guid = self.content.prefab_guid(PrefabId(prefab)).context("no such prefab is loaded")?;
        let template = self.content.loaded_template(self.renderer.as_deref_mut(), PrefabId(prefab))?;
        let root = template.root().context("the prefab has no Objs")?;
        let entities = template.spawn_objs(world, parent, placement);
        for (index, &entity) in entities.iter().enumerate() {
            world.insert(entity, Spawned { prefab: guid, root: entities[root], index: index as u32 });
        }
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
            reload_shared: options.reload_shared != 0,
        };
        let target = LoadTarget::Level(reference.to_string());
        Ok(self.loads.start_with(self.content, target, options, self.models)?.0)
    }

    fn load_scene(&mut self, reference: &str, options: SceneLoadOptions) -> Result<u32> {
        let target = LoadTarget::Scene(reference.to_string());
        Ok(self.loads.start_with(self.content, target, scene_options(options), self.models)?.0)
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
            LevelInfo::Loaded => Some(self.levels.loaded_scenes().collect::<Vec<_>>().join("\n")),
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

    fn obj_key(&self, world: &World, entity: Entity) -> Option<String> {
        if let Some((scene, path)) = self.levels.place_of(entity) {
            return Some(format!("{scene}:{path}"));
        }
        let spawned = world.get::<Spawned>(entity)?;
        world.is_alive(spawned.root).then(|| format!("{SPAWNED_KEY}{}/{}", spawned.root.to_bits(), spawned.index))
    }

    fn obj_by_key(&self, world: &World, key: &str) -> Option<Entity> {
        if let Some(copy) = key.strip_prefix(SPAWNED_KEY) {
            let (root, index) = copy.split_once('/')?;
            let (root, index) = (Entity::from_bits(root.parse().ok()?), index.parse::<u32>().ok()?);
            let mut found = world.query_ref::<(Entity, &Spawned)>().into_iter();
            return found.find(|(_, spawned)| spawned.root == root && spawned.index == index).map(|(entity, _)| entity);
        }
        let (scene, path) = key.split_once(':')?;
        self.levels.obj_at(scene.parse().ok()?, &ObjPath::parse(path).ok()?)
    }

    fn scene_state(&self, world: &World, doomed: &[Entity]) -> Option<String> {
        let level = self.levels.current().and_then(|path| self.content.db().guid_of(path)).map(|guid| guid.to_string());
        let gone: Vec<String> =
            self.levels.gone(world, doomed).into_iter().map(|(scene, path)| format!("{scene}:{path}")).collect();
        let mut roots: Vec<(Entity, AssetGuid)> = world
            .query_ref::<(Entity, &Spawned)>()
            .into_iter()
            .filter(|(entity, spawned)| spawned.root == *entity && !is_doomed(world, doomed, *entity))
            .map(|(entity, spawned)| (entity, spawned.prefab))
            .collect();
        roots.sort_by_key(|(entity, _)| entity.index());
        let spawned: Vec<serde_json::Value> = roots
            .into_iter()
            .map(|(root, prefab)| {
                let parent = world.get::<Parent>(root).and_then(|parent| self.obj_key(world, parent.0));
                serde_json::json!({ "root": root.to_bits(), "prefab": prefab.to_string(), "parent": parent })
            })
            .collect();
        Some(serde_json::json!({ "level": level, "gone": gone, "spawned": spawned }).to_string())
    }

    fn asset_guid(&self, asset: AssetName<'_>) -> Option<String> {
        let guid = match asset {
            AssetName::Reference(reference) => self.content.parse(reference).ok()?.guid()?,
            AssetName::Prefab(prefab) => self.content.prefab_guid(PrefabId(prefab))?,
            AssetName::Sound(sound) => self.content.sound_guid(SoundId(sound))?,
        };
        Some(guid.to_string())
    }

    fn prefab_path(&self, prefab: u32) -> Option<String> {
        let guid = self.content.prefab_guid(PrefabId(prefab))?;
        Some(self.content.db().get(guid)?.path.clone())
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

/// What a change did, for the log: `what` is "level" or "scene", and
/// `name` the level or scene it was for.
/// Gives each Obj with a Mesh collider its mesh's triangles, which physics
/// makes its shape from, and takes them from one that has none now.
fn give_collider_meshes(world: &mut World, content: &mut Content) {
    let meshes = world.query_ref::<(Entity, &Collider, &MeshRenderer)>().without::<Destroying>().into_iter();
    let wanted: Vec<(Entity, MeshHandle)> =
        meshes.filter(|(_, collider, _)| collider.shape == ColliderShape::Mesh).map(|(e, _, r)| (e, r.mesh)).collect();
    for &(entity, mesh) in &wanted {
        let given = world.get::<ColliderMesh>(entity).map(|given| given.key);
        match content.triangles(mesh) {
            Some(triangles) if given != Some(triangles.key) => drop(world.insert(entity, triangles)),
            None if given.is_some() => drop(world.remove::<ColliderMesh>(entity)),
            _ => {}
        }
    }
    let stale = world.query_ref::<(Entity, &ColliderMesh)>().into_iter().map(|(entity, _)| entity);
    let stale: Vec<Entity> = stale.filter(|entity| !wanted.iter().any(|(e, _)| e == entity)).collect();
    for entity in stale {
        world.remove::<ColliderMesh>(entity);
    }
}

fn log_change(what: &str, name: &str, change: &LevelChange) {
    let list = |scenes: &[String]| if scenes.is_empty() { "none".to_string() } else { scenes.join(", ") };
    tracing::info!(
        "{what} {name}: loaded {} · kept {} · unloaded {}",
        list(&change.loaded),
        list(&change.kept),
        list(&change.unloaded)
    );
}

/// Whether an Obj's place is a model part's, to within what a scene file
/// keeps of it: its rotation is written as angles, and read back a hair off.
fn same_place(part: (Vec3, Quat, Vec3), obj: (Vec3, Quat, Vec3)) -> bool {
    const CLOSE: f32 = 1e-4;
    part.0.abs_diff_eq(obj.0, CLOSE) && part.1.dot(obj.1).abs() > 1.0 - CLOSE && part.2.abs_diff_eq(obj.2, CLOSE)
}

/// How a scene loaded alongside comes in, from what scripts ask for.
fn scene_options(options: SceneLoadOptions) -> LevelOptions {
    LevelOptions {
        hold: options.hold != 0,
        set_active: options.set_active != 0,
        reload_shared: options.reload != 0,
        ..LevelOptions::default()
    }
}
