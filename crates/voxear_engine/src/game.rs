use std::collections::HashMap;

mod net_dimensions;

use anyhow::{Context, Result};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_core::{
    math::{IVec3, Quat, Vec2, Vec3},
    profile_scope,
};
use voxear_ecs::{Entity, World};
use voxear_localization::{Localization, StringTable};
use voxear_render::{MAX_PREVIEW_SIZE, MeshHandle, MeshRenderer, PostProcess, PreviewId, PreviewOptions, Renderer};
use voxear_scene::{Destroying, Parent, Placement, Transform};
use voxear_scripting::{
    AssetName, AudioAsk, ContentStats, CursorLock, HeatEventData, LevelInfo, LevelLoadOptions, LoadProgress, PlaySound,
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
    /// The maps of the world its UI shows.
    pub maps: crate::map::Maps,
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
    /// The sky picture last refused, so it is reported once.
    sky_failed: Option<voxear_assets::AssetGuid>,
    /// The pictures prompts show for controls, as the input actions name
    /// them.
    pub prompt_icons: crate::prompt_icons::PromptIcons,
}

/// The project's input actions' names, `Map/Name`, by their ids, for the
/// Inspector to show an `InputActionRef` field by.
#[derive(Clone, Debug, Default)]
pub struct ActionNames(pub HashMap<String, String>);

impl ActionNames {
    pub fn of(input: &voxear_input::Input) -> Self {
        let file = input.authored_file();
        let actions = file.maps.iter().flat_map(|map| map.actions.iter().map(move |action| (map, action)));
        let named = actions.filter(|(_, action)| !action.id.is_empty());
        Self(named.map(|(map, action)| (action.id.clone(), format!("{}/{}", map.name, action.name))).collect())
    }

    /// An action's name, `Map/Name`, by its id, or what it is named by
    /// when that is not an id.
    pub fn label(&self, reference: &str) -> String {
        self.0.get(reference).cloned().unwrap_or_else(|| reference.to_string())
    }
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
    /// The HDR output scripts ask for, with paper white and the peak, and
    /// whether that changed since it was last made so.
    pub hdr: voxear_render::display::DisplaySettings,
    pub hdr_changed: bool,
    /// What the window shows, and the HDR its display offers, as whatever
    /// shows the game has it.
    pub hdr_shown: voxear_render::display::DisplayState,
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
            hdr: voxear_render::display::DisplaySettings::default(),
            hdr_changed: false,
            hdr_shown: voxear_render::display::DisplayState::default(),
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
            maps: crate::map::Maps::default(),
            display: Display::default(),
            audio: GameAudio::default(),
            input: voxear_input::Input::new(),
            lut_failed: None,
            sky_failed: None,
            prompt_icons: crate::prompt_icons::PromptIcons::default(),
        };
        game.audio.headphones = game.project.file.audio.headphones;
        game.audio.reverb = game.project.file.audio.reverb;
        game.audio.reverb_mode = game.project.file.audio.reverb_mode;
        game.audio.tracer.project = game.project.file.audio.sound_tracing();
        game.load_input_actions();
        game.load_fonts();
        game.load_strings();
        game.engine.world.insert_resource(game.project.layers());
        crate::modules::put(&mut game.engine.world, game.project.modules());
        game.project.file.quality.put(&mut game.engine.world);
        voxear_terrain::MachineBudget::open(&mut game.engine.world, game.project.file.world, None);
        game.project.file.portals.put(&mut game.engine.world);
        game.project.file.water_trails.put(&mut game.engine.world);
        game.project.file.navigation.put(&mut game.engine.world);
        game.project.file.crowds.put(&mut game.engine.world);
        game.engine.world.insert_resource(game.project.file.foliage.bounded());
        game.project.file.physics.put(&mut game.engine.world);
        game.project.file.time.put(&mut game.engine);
        let in_effect = crate::project_scripts::ProjectInEffect::of(&game.project.file);
        game.engine.world.insert_resource(in_effect);
        game
    }

    /// Makes worlds of blocks spend what this machine keeps for the game in
    /// `Graphics.toml` beside its saves, in place of the project's, where it
    /// keeps anything; and lets a game's options keep what they set there
    /// (see `voxear_terrain::MachineBudget`). A built game, `vox run` and the
    /// editor do this; tests and pictures keep to the project's.
    /// Puts an Obj inside another, or at the top of its scene with none,
    /// while the game runs, as a script's `Obj.SetParent` does (see
    /// [`crate::set_parent`]): refused into another scene, another
    /// dimension, or inside itself.
    pub fn set_parent(
        &mut self,
        entity: Entity,
        parent: Option<Entity>,
        keep_world: bool,
    ) -> Result<bool, crate::ParentError> {
        crate::obj_hierarchy::set_parent(&mut self.engine.world, Some(&self.levels), entity, parent, keep_world)
    }

    pub fn use_machine_budget(&mut self, company: Option<&str>) {
        let file = crate::player::machine_file(company, self.project.name());
        let project = self.project.file.world;
        if let Some(problem) = voxear_terrain::MachineBudget::open(&mut self.engine.world, project, file) {
            tracing::warn!("{problem}; the project's world quality is used");
        }
    }

    /// Reads the project's input actions: the `.inputactions` file its
    /// `[input]` picks, or else the first by path when it has several, or
    /// else Voxear's own. A package's are its own, and never the game's.
    pub fn load_input_actions(&mut self) {
        let db = self.content.db();
        let mut files: Vec<(String, AssetGuid)> = (db.entries())
            .filter(|entry| entry.kind == AssetKind::InputActions && entry.mount == voxear_assets::Mount::Project)
            .map(|entry| (entry.path.clone(), entry.guid))
            .collect();
        files.sort();
        if let Some(picked) = self.project.file.input.actions.as_deref() {
            let guid = picked.parse::<AssetGuid>().ok().or_else(|| db.guid_of(picked));
            match files.iter().position(|(_, file)| Some(*file) == guid) {
                Some(at) => files.swap(0, at),
                None => tracing::warn!("[input] actions names {picked}, which is not the project's; the first is used"),
            }
        }
        if let Some((path, guid)) = files.into_iter().next() {
            let read = self.content.asset_text(guid, AssetKind::InputActions);
            if let Err(error) = read.and_then(|(text, _)| self.input.load_actions(&text)) {
                tracing::error!("{path}: {error:#}; the game uses Voxear's own input actions");
            }
        }
        self.engine.world.insert_resource(ActionNames::of(&self.input));
    }

    /// Reads every string table and asset table in the project, in path
    /// order, into the world's [`Localization`]. The game starts in the language Steam
    /// says it is in, when the developer ships Steamworks (see [`crate::steam_language`]),
    /// or else the one the player's system is in, or the nearest the project has, or its own
    /// (and as it starts, in the one the player chose last, see [`start`](Self::start));
    /// read again, as when a table changed, it stays in the one it is in.
    /// Reads the project's custom shaders and has the renderer compile them
    /// into its own, on a thread of their own, with the built-in ones its
    /// materials pick, and draws the materials that use them with them.
    /// Nothing to compile for a project without any, nor for a built game,
    /// whose build compiled them, nor when nothing changed.
    pub fn apply_custom_shaders(&mut self, renderer: &mut Renderer) {
        self.content.read_shaders();
        let registry = self.content.shaders();
        for shader in registry.shaders.iter().filter(|shader| shader.used) {
            if let Err(error) = &shader.info {
                tracing::error!("the shader {}: {error}", shader.path);
            }
        }
        // Particle Systems call their particle graphs by these numbers.
        let numbers = registry.particle_graph_numbers();
        if self.engine.world.resource::<voxear_render::ParticleGraphs>() != Some(&numbers) {
            self.engine.world.insert_resource(numbers);
        }
        // The UI's backgrounds can be drawn with the built-in effects and the project's UI shaders.
        self.ui.set_shaders(std::rc::Rc::new(crate::shaders::ui_shaders(registry)));
        // A built game draws with the shaders its build compiled, built-in ones and all.
        if let Some(modules) = self.content.built_shaders() {
            if let Err(error) = modules.and_then(|modules| renderer.use_custom_modules(&modules)) {
                tracing::error!("drawing with the built custom shaders: {error:#}");
            }
        } else {
            // A project that never had shaders keeps the renderer's own, uncompiled.
            let had = *renderer.custom_shaders() != voxear_render::CustomShaderState::None;
            if !registry.any_used() && !had {
                return;
            }
            let code = voxear_render::CustomCode {
                materials: crate::shaders::dispatch_code(registry),
                ui: crate::shaders::ui_dispatch_code(registry),
                look: crate::shaders::look_dispatch_code(registry),
                particles: crate::shaders::particle_dispatch_code(registry),
            };
            if !renderer.set_custom_shaders(code) {
                return;
            }
        }
        if let Err(error) = self.content.refresh_shaded_materials(renderer) {
            tracing::error!("{error:#}");
        }
    }

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
        localization.set_fonts(&self.project.file.localization.fonts);
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

    /// Reads every font of the project's and its packages', `.ttf` and
    /// `.otf`, for UI text to draw in, named by its path or its GUID (see
    /// `voxear_ui::fonts`); a font read already and unchanged is kept.
    pub fn load_fonts(&mut self) {
        let fonts: Vec<(String, AssetGuid)> = (self.content.db().entries())
            .filter(|entry| entry.kind == AssetKind::Font)
            .map(|entry| (entry.path.clone(), entry.guid))
            .collect();
        for (path, guid) in fonts {
            match self.content.read(&path) {
                Ok(data) => drop(voxear_ui::fonts::register(&[&path, &guid.to_string()], std::sync::Arc::new(data))),
                Err(error) => tracing::error!("{path}: {error:#}"),
            }
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
    /// Starts networked play as `start` says, through a network as bad as
    /// `conditions`, and gives back where its server is.
    pub fn start_session(
        &mut self,
        start: crate::net::NetStart,
        conditions: voxear_net::Conditions,
    ) -> Result<std::net::SocketAddr> {
        use crate::net::{self, NetDefaults, NetSettings, NetStart};
        anyhow::ensure!(
            crate::modules::has(&self.engine.world, crate::modules::Module::Networking),
            "{} does not have the Networking module; switch it on in Project Settings",
            self.project.name()
        );
        let mut settings = NetSettings::for_project(self.project.name(), crate::engine_version(), &self.project.file.network);
        settings.conditions = conditions;
        // Scripts that start another session later start it the same way.
        self.engine.world.insert_resource(NetDefaults(settings.clone()));
        self.engine.set_fixed_delta(1.0 / settings.tick_rate as f32);
        let port = |port: u16| if port == 0 { settings.port } else { port };
        match start {
            NetStart::Server(at) => net::serve(&mut self.engine.world, settings.clone(), port(at), false),
            NetStart::Host(at) => net::serve(&mut self.engine.world, settings.clone(), port(at), true),
            NetStart::Connect(address) => net::connect(&mut self.engine.world, settings, address).map(|()| address),
        }
    }

    /// Wears the look pack the player chose last time, kept beside the
    /// saves, as a game starts. One that is no longer there is forgotten.
    pub fn wear_remembered_look(&mut self) {
        let Some(reference) = crate::look_packs::remembered() else { return };
        match crate::look_packs::wear(&mut self.engine.world, &mut self.content, &reference) {
            Ok(()) => tracing::info!("wearing the look pack chosen last time, {reference}"),
            Err(error) => {
                tracing::warn!("the look pack chosen last time is left off: {error:#}");
                crate::look_packs::remember("");
            }
        }
    }

    /// Wears the texture pack the player chose last time, kept beside the
    /// saves, as a game starts. One that is no longer there is forgotten.
    pub fn wear_remembered_textures(&mut self) {
        let Some(reference) = crate::texture_packs::remembered() else { return };
        match crate::texture_packs::wear(&mut self.content, &reference) {
            Ok(()) => tracing::info!("wearing the texture pack chosen last time, {reference}"),
            Err(error) => {
                tracing::warn!("the texture pack chosen last time is left off: {error:#}");
                crate::texture_packs::remember("");
            }
        }
    }

    /// Wears the shader pack the player chose last time, kept beside the
    /// saves, with the options they set on it, as a game starts: compiled on
    /// a thread, or read from the compiled packs kept. One that is no longer
    /// there is forgotten.
    pub fn wear_remembered_shader_pack(&mut self) {
        let Some(saves) = voxear_scripting::saves_folder() else { return };
        let Some((reference, options)) = crate::shader_packs::remembered(&saves) else { return };
        match crate::shader_packs::wear_soon(&mut self.engine.world, &mut self.content, &reference, false) {
            Ok(()) => {
                tracing::info!("wearing the shader pack chosen last time, {reference}");
                for (key, value) in options {
                    // An option the pack no longer has is left behind.
                    let _ = crate::shader_packs::set_option(&mut self.engine.world, &key, value);
                }
            }
            Err(error) => {
                tracing::warn!("the shader pack chosen last time is left off: {error:#}");
                crate::shader_packs::take_off(&mut self.engine.world);
                crate::shader_packs::remember(&self.engine.world, &saves);
            }
        }
    }

    /// Does what scripts asked of shader packs this frame: wearing one,
    /// compiled on a thread, and setting its options; and keeps what the
    /// player chose beside the saves where they asked to.
    fn shader_pack_requests(&mut self) {
        let requests = self.engine.scripts_mut().map(|scripts| scripts.take_shader_pack_requests()).unwrap_or_default();
        let mut keep = false;
        for request in requests {
            match request {
                voxear_scripting::ShaderPackRequest::Wear { reference, remember } => {
                    let world = &mut self.engine.world;
                    match crate::shader_packs::wear_soon(world, &mut self.content, &reference, false) {
                        Ok(()) => keep |= remember,
                        Err(error) => tracing::error!(target: "script", "{error:#}"),
                    }
                }
                voxear_scripting::ShaderPackRequest::SetOption { name, value, remember } => {
                    match crate::shader_packs::set_option(&mut self.engine.world, &name, value) {
                        Ok(_) => keep |= remember,
                        Err(error) => tracing::error!(target: "script", "{error:#}"),
                    }
                }
            }
        }
        if keep && let Some(saves) = voxear_scripting::saves_folder() {
            crate::shader_packs::remember(&self.engine.world, &saves);
        }
    }

    pub fn start(&mut self, mut renderer: Option<&mut Renderer>) -> Result<()> {
        self.audio.start_game();
        // For scripts that start networked play.
        let defaults = crate::net::NetSettings::for_project(self.project.name(), crate::engine_version(), &self.project.file.network);
        self.engine.world.insert_resource(crate::net::NetDefaults(defaults));
        // The project's shaders first, so its materials are uploaded with them.
        if let Some(renderer) = renderer.as_deref_mut() {
            self.apply_custom_shaders(renderer);
        }
        // The language the player chose last time, kept beside the saves, over the system's.
        if let Some(saved) = voxear_scripting::saved_language()
            && let Some(localization) = self.engine.world.resource_mut::<Localization>()
        {
            localization.set_language(&saved);
        }
        self.wear_remembered_look();
        self.wear_remembered_textures();
        // Shader packs compiled in one run are kept beside the saves for the next; the game's own were compiled
        // when it was built.
        let kept = voxear_scripting::saves_folder().and_then(|saves| crate::shader_packs::compiled_folder(&saves));
        voxear_render::shader_packs::keep_compiles_in(kept);
        voxear_render::shader_packs::read_compiles_from(self.content.built_shader_packs());
        self.wear_remembered_shader_pack();
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

    /// Makes the ground round where the world is seen from before the game
    /// goes on, as a loading screen waits for it: terrains and worlds of
    /// blocks without end, and the plants grown on them. No time passes and
    /// no script runs meanwhile. True once all of it is made; false if
    /// `limit` went by first, with what was made by then kept and the rest
    /// still made as the game ticks.
    pub fn make_world(&mut self, limit: std::time::Duration) -> bool {
        let started = std::time::Instant::now();
        loop {
            let world = &mut self.engine.world;
            crate::terrain::stream(world, &mut self.content);
            crate::portals::find_exits(world);
            crate::blocks::stream(world, &mut self.content);
            crate::grown_plants::update(world, &mut self.content);
            let made = crate::terrain::streamed(world)
                && crate::blocks::streamed(world)
                && crate::grown_plants::settled(world);
            if made {
                return true;
            }
            if started.elapsed() >= limit {
                return false;
            }
            // The terrain's threads make it; this one only takes in what they finished.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// The dimension `asset` makes as `key`, made now if it is not yet: its
    /// file's Objs spawned in it, its sky, sun, post-processing and weather
    /// kept for when the camera comes into it. `key` tells apart the
    /// dimensions one asset makes for each place that opens it; 0 for one
    /// alone.
    pub fn make_dimension(
        &mut self,
        asset: voxear_assets::AssetGuid,
        key: u64,
        renderer: Option<&mut Renderer>,
    ) -> Result<crate::dimensions::DimensionId> {
        use crate::dimensions::{self, DimensionFile, Dimensions};
        if let Some(made) = self.engine.world.resource::<Dimensions>().and_then(|made| made.find(asset, key)) {
            return Ok(made);
        }
        let (text, path) = self.content.asset_text(asset, voxear_assets::AssetKind::Dimension)?;
        let file = DimensionFile::parse(&text, &path)?;
        let surroundings = dimensions::file_surroundings(&mut self.engine, &mut self.content, &file)
            .with_context(|| format!("loading {path}"))?;
        let world = &mut self.engine.world;
        let id = dimensions::make_empty(world, file.settings.clone(), Some(surroundings), self.levels.current())
            .with_context(|| format!("making {path}: there are as many dimensions as there can be"))?;
        let objs = crate::scene_file::spawn_objs(
            &mut self.engine,
            &mut self.content,
            renderer,
            &file.scene,
            &std::collections::HashSet::new(),
        )
        .with_context(|| format!("loading {path}"));
        let objs = match objs {
            Ok(objs) => objs,
            Err(error) => {
                dimensions::let_go(&mut self.engine.world, id);
                return Err(error);
            }
        };
        let world = &mut self.engine.world;
        let roots: Vec<Entity> = objs
            .iter()
            .map(|(_, entity)| *entity)
            .filter(|&entity| !world.has::<voxear_scene::Parent>(entity))
            .collect();
        // Its Objs are where it puts them from the level's real middle, as a scene's are, wherever the world's is.
        let moved = voxear_scene::WorldOrigin::of(world).is_moved();
        for &root in &roots {
            dimensions::set_dimension(world, root, id);
            if moved && let Some(file) = world.get::<voxear_scene::Transform>(root).map(|t| t.position) {
                voxear_scene::place_at_level(world, root, file.as_dvec3());
            }
        }
        dimensions::record_made(world, id, (Some(asset), key), objs);
        // The changes made in it before it was last let go, or a save put back.
        crate::dimension_saves::put_back(&mut self.engine.world, &mut self.content, id, self.levels.current());
        Ok(id)
    }

    /// Links each Portal near enough to show its far side that leads into a
    /// dimension's file, making that dimension first; and lets go each
    /// dimension made so once nothing has needed it for its `keep_loaded`
    /// seconds: no camera in it, and no Portal near enough to it to show
    /// it. While the game plays.
    fn link_dimensions(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        use crate::dimensions::{self, Dimensions};
        if self.engine.is_edit_mode() {
            return;
        }
        // One dimension loaded for a Portal at a time: the open Portal's, or on a server the one open for each
        // player. While one is open, any other loaded so goes at once, before the open one's is made; the one a
        // camera is in stays, as do those networked play needs: those players are in, or the server has kept.
        let world = &self.engine.world;
        let open = crate::portals::open_into(world);
        if !open.is_empty() {
            let mut keep: Vec<dimensions::DimensionId> = open
                .iter()
                .filter_map(|portal| world.get::<voxear_render::Portal>(*portal)?.to.filter(|to| world.is_alive(*to)))
                .map(|to| dimensions::dimension_of(world, to))
                .collect();
            keep.extend(crate::net::dimensions_needed(world));
            let current = dimensions::current(world);
            let going: Vec<dimensions::DimensionId> = world
                .resource::<Dimensions>()
                .map(|made| made.on_demand().filter(|id| !keep.contains(id) && *id != current).collect())
                .unwrap_or_default();
            for id in going {
                self.let_go_dimension(id);
            }
        }
        for (portal, link) in crate::portals::link_wanted(&self.engine.world) {
            let asset = self.content.parse(&link.dimension).ok().and_then(|parsed| parsed.guid());
            let made = asset.map(|asset| {
                let fresh = self.engine.world.resource::<Dimensions>().and_then(|made| made.find(asset, 0)).is_none();
                let made = self.make_dimension(asset, 0, renderer.as_deref_mut());
                if let (true, Ok(id)) = (fresh, &made) {
                    dimensions::made_on_demand(&mut self.engine.world, *id);
                }
                made
            });
            let id = match made {
                Some(Ok(id)) => id,
                Some(Err(error)) => {
                    tracing::warn!("a Portal leads into {} and it could not be made: {error:#}", link.dimension);
                    self.engine.world.remove::<crate::portals::PortalInto>(portal);
                    continue;
                }
                None => {
                    tracing::warn!("a Portal leads into {}, which is no dimension of the project", link.dimension);
                    self.engine.world.remove::<crate::portals::PortalInto>(portal);
                    continue;
                }
            };
            let world = &mut self.engine.world;
            let path = link.portal.path().ok();
            let far = path.and_then(|path| world.resource::<Dimensions>()?.obj_at(id, &path));
            let Some(far) = far.filter(|far| world.has::<voxear_render::Portal>(*far)) else {
                tracing::warn!("a Portal leads into Obj {:?} of {}, which is no Portal", link.portal, link.dimension);
                world.remove::<crate::portals::PortalInto>(portal);
                continue;
            };
            if let Some(near) = world.get_mut::<voxear_render::Portal>(portal) {
                near.to = Some(far);
            }
            // The far one, leading nowhere, leads back.
            if let Some(back) = world.get_mut::<voxear_render::Portal>(far).filter(|back| back.to.is_none()) {
                back.to = Some(portal);
            }
        }
        for (portal, gate) in crate::portals::gates_wanted(&self.engine.world) {
            match self.make_dungeon_dimension(gate, renderer.as_deref_mut()) {
                Ok((_, back)) => {
                    let world = &mut self.engine.world;
                    if let Some(near) = world.get_mut::<voxear_render::Portal>(portal) {
                        near.to = Some(back);
                    }
                    if let Some(far) = world.get_mut::<voxear_render::Portal>(back) {
                        far.to = Some(portal);
                    }
                }
                Err(error) => {
                    tracing::warn!("a dungeon's dimension could not be made: {error:#}");
                    self.engine.world.remove::<crate::dungeons::DungeonGate>(portal);
                }
            }
        }
        let mut needed: Vec<dimensions::DimensionId> =
            crate::portals::exits(&self.engine.world).iter().map(|exit| exit.dimension).collect();
        needed.extend(crate::net::dimensions_needed(&self.engine.world));
        for id in dimensions::idle_ones(&mut self.engine.world, delta, &needed) {
            self.let_go_dimension(id);
        }
    }

    /// The dimension of the dungeon whose gate `gate` stands in, made now if
    /// it is not yet, and the Portal in it that leads back out: the dungeon
    /// built in a world of rock all round, its own (see
    /// `voxear_terrain::blocks::dungeon::in_rock`), dark unless the dungeon
    /// names a `.dimension` to start from, which brings its lighting and its
    /// Objs.
    pub fn make_dungeon_dimension(
        &mut self,
        gate: crate::dungeons::DungeonGate,
        renderer: Option<&mut Renderer>,
    ) -> Result<(crate::dimensions::DimensionId, Entity)> {
        use crate::dimensions::{self, DimensionKind, DimensionSettings, Dimensions};
        use voxear_terrain::blocks::structure::StructureBlocks;
        let world = &self.engine.world;
        let asset =
            world.get::<crate::blocks::BlockWorldAsset>(gate.world).cloned().context("its world of blocks is gone")?;
        let key = gate.key(asset.guid);
        let made = world.resource::<Dimensions>().and_then(|made| made.find_by_key(key));
        if let Some(id) = made {
            let back = world
                .query_ref::<(Entity, &voxear_render::Portal, &crate::dungeons::DungeonWayOut)>()
                .into_iter()
                .find(|(entity, ..)| dimensions::dimension_of(world, *entity) == id)
                .map(|(entity, ..)| entity)
                .context("the dungeon's dimension has no way out")?;
            return Ok((id, back));
        }
        let file = asset.file.clone();
        let size = file.block_size;
        let generate = file.generate.as_ref().context("its world of blocks is not generated")?;
        let built = generate.built.clone().context("its world of blocks builds no structures")?;
        let StructureBlocks::Dungeon(dungeon) = &built.list.get(gate.structure).context("no such structure")?.blocks
        else {
            anyhow::bail!("its structure is no dungeon");
        };
        let landed = crate::blocks::generator(&mut self.content, &file).context("its generator could not be read")?;
        let rock = match dungeon.blocks.stone {
            voxear_terrain::blocks::chunk::AIR => file.ground.bottom,
            stone => stone,
        };
        let turn = |placed, turns| built.turned_block(placed, turns, false);
        // As many floors as its site gives it and its stage has grown it by.
        let stage = crate::dungeons::DungeonStages::of(world, gate.site()).stage;
        let floors = crate::dungeons::floors_at_stage(dungeon, gate.seed, stage);
        let started = std::time::Instant::now();
        let inside = voxear_terrain::blocks::dungeon::in_rock(
            dungeon,
            &landed,
            file.block_size,
            (gate.seed, floors),
            rock,
            &turn,
        )
        .context("it could not be planned in rock")?;
        tracing::debug!(floors, took = ?started.elapsed(), "a dungeon's dimension built");
        // From the dimension its file names, or one of the engine's own, dark and still.
        let template = dungeon.dimension.as_deref().and_then(|named| self.content.parse(named).ok()?.guid());
        let id = match template {
            Some(template) => self.make_dimension(template, key, renderer)?,
            None => {
                let settings = DimensionSettings::of(DimensionKind::Dungeon);
                let level = self.levels.current();
                dimensions::make_empty(&mut self.engine.world, settings, None, level)
                    .context("there are as many dimensions as there can be")?
            }
        };
        dimensions::made_on_demand(&mut self.engine.world, id);
        dimensions::note_key(&mut self.engine.world, id, key);
        dimensions::note_dungeon(&mut self.engine.world, id, gate);
        let world = &mut self.engine.world;
        // Its world: the overworld's blocks, numbered as there, laid as the dungeon was built.
        let ground_file = voxear_terrain::blocks::file::BlockFile { generate: None, ..file };
        let layout = crate::dungeons::DungeonInside(inside.layout.clone());
        let base = std::sync::Arc::new(inside.blocks);
        let blocks = crate::blocks::make(&asset.drawn, &base, None);
        let owned = crate::blocks::BlockWorldAsset { file: ground_file, base, pending: Default::default(), ..asset };
        let look = world.get::<voxear_render::BlockLook>(gate.world).cloned();
        let rock_world =
            world.spawn((voxear_scene::Name("Dungeon".into()), voxear_scene::Transform::default(), blocks, owned));
        if let Some(look) = look {
            world.insert(rock_world, look);
        }
        world.insert(rock_world, layout);
        world.insert(rock_world, crate::dungeons::DungeonFrom(gate));
        dimensions::set_dimension(world, rock_world, id);
        // And the Portal back out, by the head of the stair.
        let [x, y, z] = inside.arrival;
        let at = voxear_core::math::Vec3::new(x as f32 + 0.5, y as f32, z as f32 + 0.5) * size;
        let turn = voxear_core::math::Quat::from_rotation_y(inside.facing[0].atan2(inside.facing[1]));
        let portal = voxear_render::Portal { size: [3.0, 4.0].into(), ..voxear_render::Portal::default() };
        let name = voxear_scene::Name("Way Out".into());
        let placed = voxear_scene::Transform::new(at, turn, voxear_core::math::Vec3::ONE);
        let back = world.spawn((name, placed, portal, crate::dungeons::DungeonWayOut));
        dimensions::set_dimension(world, back, id);
        // The changes made in it before it was last let go, or a save put back.
        crate::dimension_saves::put_back(&mut self.engine.world, &mut self.content, id, self.levels.current());
        Ok((id, back))
    }

    /// Lets a dimension go, destroying its Objs at the end of the frame:
    /// persistent ones are moved to the level's. False where it was not made.
    /// What was changed in it is kept, and put back when it is made again.
    pub fn let_go_dimension(&mut self, id: crate::dimensions::DimensionId) -> bool {
        crate::dimension_saves::keep(&mut self.engine.world, &mut self.content, id, self.levels.current());
        match crate::dimensions::let_go(&mut self.engine.world, id) {
            Some(doomed) => {
                self.engine.destroy_objs(&doomed);
                true
            }
            None => false,
        }
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
        // A level starts from its own middle: Objs that stay, as persistent ones do, move back with the world.
        let origin = voxear_scene::WorldOrigin::of(&self.engine.world);
        if origin.is_moved() {
            self.move_origin(origin.0.as_ivec3(), renderer.as_deref_mut());
        }
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
    pub fn reimport_model(
        &mut self,
        renderer: &mut Renderer,
        path: &str,
        mut aside: Option<&mut World>,
    ) -> Result<usize> {
        let guid = self.content.db().guid_of(path).with_context(|| format!("{path} is not in the project"))?;
        let Reimported { moved, meshes, materials } = self.content.reimport_model(renderer, guid)?;
        if !meshes.is_empty() || !materials.is_empty() {
            for world in std::iter::once(&mut self.engine.world).chain(aside.as_deref_mut()) {
                for drawn in world.query::<&mut MeshRenderer>() {
                    drawn.mesh = meshes.get(&drawn.mesh).copied().unwrap_or(drawn.mesh);
                    drawn.material = materials.get(&drawn.material).copied().unwrap_or(drawn.material);
                }
            }
        }
        // Objs drawing the whole model draw its parts as they are now, where they now sit.
        if let Some(nodes) = self.content.loaded_model(guid).map(<[_]>::to_vec) {
            let animation = self.content.model_animation(guid);
            for world in std::iter::once(&mut self.engine.world).chain(aside.as_deref_mut()) {
                let query =
                    world.query::<(Entity, &crate::scene_file::ModelSource, &mut voxear_render::ModelRenderer)>();
                // Those whose bones are not the ones they were posed with, as a box model's with a bone added.
                let mut other_bones = Vec::new();
                for (entity, source, drawn) in query {
                    if self.content.parse(&source.0.model).ok().and_then(|r| r.guid()) != Some(guid) {
                        continue;
                    }
                    match crate::prefab::model_parts(&mut self.content, renderer, &nodes, &source.0) {
                        Ok(parts) => drawn.parts = parts.into(),
                        Err(error) => tracing::error!("{error:#}"),
                    }
                    let bones = |animation: &Option<std::sync::Arc<voxear_render::ModelAnimation>>| {
                        animation.as_ref().map(|animation| animation.skeleton.bones.len())
                    };
                    if bones(&drawn.animation) != bones(&animation) {
                        other_bones.push(entity);
                    }
                    drawn.animation = animation.clone();
                }
                // A pose of the old bones would move the new ones' skin wrongly; what animates it poses it afresh.
                for entity in other_bones {
                    world.remove::<voxear_render::Pose>(entity);
                    world.remove::<crate::animation::AnimatorState>(entity);
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

    /// Starts the game's UI afresh, with nothing on it, and lets go of what
    /// showed on the one before: its documents, and its maps with the
    /// input and the pointer an open map held. What the editor does as
    /// play stops, so the next play's UI is made as the first was.
    pub fn reset_ui(&mut self, renderer: Option<&mut Renderer>) {
        let shaders = self.ui.shaders();
        self.ui = GameUi::default();
        self.ui.set_shaders(shaders);
        self.maps.leave_ui(&mut self.input, &mut self.display, renderer);
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

    /// Runs `run` on the game's world with the game's services lent to
    /// scripts, as while ticking: content, blocks, prefabs, UI and sound.
    /// For scripts run apart from the game's own runtime, as the editor runs
    /// a `[Command]` on the edited level.
    pub fn with_services<R>(&mut self, renderer: Option<&mut Renderer>, run: impl FnOnce(&mut World) -> R) -> R {
        let models = self.models;
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer,
            models,
            unload_requested: &mut self.unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
        };
        let _scope = ServicesScope::enter(&mut services);
        run(&mut self.engine.world)
    }

    /// Changes the running scripts' loaded instance of the data asset
    /// `guid`, as the Inspector changed its file while the game plays (see
    /// [`Engine::refill_data_asset`]), with the content at hand for a data
    /// asset a changed field picks that they have not loaded yet.
    pub fn refill_data_asset(&mut self, guid: AssetGuid, changed: &toml::Table, reset: &[String]) -> Result<bool> {
        let models = self.models;
        let unload_requested = &mut self.unload_requested;
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer: None,
            models,
            unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.refill_data_asset(&guid.to_string(), changed, reset)
    }

    /// Starts loading a prefab in the background; its status gives the
    /// prefab once done.
    pub fn load_prefab_async(&mut self, prefab: &str) -> Result<LoadId> {
        self.loads.start(&self.content, LoadTarget::Prefab(prefab.to_string()), self.models)
    }

    pub fn load_status(&self, id: LoadId) -> Option<LoadStatus> {
        self.loads.status(id)
    }

    /// Puts each item drop shown as its item's Dropped prefab's prefab under
    /// it, once.
    fn show_dropped_prefabs(&mut self, mut renderer: Option<&mut Renderer>) {
        use crate::item_drops::DroppedAs;
        let waiting: Vec<(Entity, String)> = self
            .engine
            .world
            .query_ref::<(Entity, &DroppedAs)>()
            .without::<voxear_scene::Destroying>()
            .into_iter()
            .filter(|(_, dropped)| !dropped.shown)
            .map(|(entity, dropped)| (entity, dropped.prefab.clone()))
            .collect();
        for (entity, prefab) in waiting {
            self.engine.world.insert(entity, DroppedAs { prefab: prefab.clone(), shown: true });
            if let Err(error) = self.instantiate(&prefab, Some(entity), None, renderer.as_deref_mut()) {
                tracing::warn!("an item's Dropped prefab {prefab}: {error:#}");
            }
        }
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

    /// Takes away the Objs whose ground streamed out, kept for later or
    /// destroyed as their Streams With Ground says, and makes again those
    /// kept whose ground came back (see `ground_streaming`).
    fn stream_with_ground(&mut self, mut renderer: Option<&mut Renderer>) {
        use crate::ground_streaming::{GroundMoves, GroundRecord, GroundRecords};
        let Some(moves) = self.engine.world.resource::<GroundMoves>().cloned() else { return };
        let going = crate::ground_streaming::going(&mut self.engine.world, &moves);
        let told: Vec<(Entity, bool)> =
            going.keep.iter().map(|(entity, _)| *entity).chain(going.destroy.iter().copied()).map(|e| (e, true)).collect();
        self.engine.tell_ground_events(&told);
        let mut gone: Vec<Entity> = going.destroy.clone();
        for (entity, which) in going.keep {
            if !self.engine.world.is_alive(entity) {
                continue;
            }
            let moved = &moves.worlds[which];
            let at = self.engine.world.get::<voxear_scene::WorldTransform>(entity).map(|p| p.0.w_axis.truncate());
            let scene = crate::scene_file::capture(&self.engine.world, &mut self.content, &[entity]);
            let fits = crate::ground_streaming::fits_under(&self.engine.world, entity);
            let record = GroundRecord {
                fits,
                world: moved.world,
                column: moved.column_of(at.unwrap_or(moved.corner)),
                dimension: moved.dimension,
                scene,
            };
            let world = &mut self.engine.world;
            if world.resource::<GroundRecords>().is_none() {
                world.insert_resource(GroundRecords::default());
            }
            world.resource_mut::<GroundRecords>().expect("put in above").records.push(record);
            gone.push(entity);
        }
        if !gone.is_empty() {
            self.engine.destroy_objs(&gone);
        }
        let mut back: Vec<(Entity, bool)> = Vec::new();
        for record in crate::ground_streaming::coming(&mut self.engine.world, &moves) {
            let none = std::collections::HashSet::new();
            let made = crate::scene_file::spawn_objs(&mut self.engine, &mut self.content, renderer.as_deref_mut(), &record.scene, &none);
            let made = match made {
                Ok(made) => made,
                Err(error) => {
                    tracing::warn!("an Obj kept while its ground was away: {error:#}");
                    continue;
                }
            };
            let world = &mut self.engine.world;
            let roots: Vec<Entity> =
                made.iter().map(|(_, entity)| *entity).filter(|&entity| !world.has::<voxear_scene::Parent>(entity)).collect();
            if record.dimension != voxear_scene::DimensionId::LEVEL {
                for &root in &roots {
                    voxear_scene::put_in_dimension(world, root, record.dimension);
                }
            }
            crate::ground_streaming::landed(world, &roots);
            back.extend(roots.into_iter().map(|root| (root, false)));
        }
        self.engine.tell_ground_events(&back);
    }

    /// Makes records of Crowd Members gone far off and Objs again of records
    /// come near (see `crowd_records`), each made from its prefab, sent on
    /// where it was going.
    fn keep_crowd_records(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        let level = self.levels.current().map(str::to_string);
        crate::crowd_records::keep_figures_to(&mut self.engine.world, level.as_deref());
        // A player's machine keeps the server's records, walked on, and only draws them.
        let makes = !crate::net::is_client(&self.engine.world);
        let (changes, gone) = crate::crowd_records::update(&mut self.engine.world, delta, level.as_deref(), makes);
        self.draw_crowd_records(delta, renderer.as_deref_mut());
        if !gone.is_empty() {
            self.engine.destroy_objs(&gone);
        }
        for record in changes.make {
            let prefab = record.prefab.to_string();
            let (position, rotation) = crate::crowd_records::placement(&self.engine.world, &record);
            let placed = Some(voxear_scene::Placement { position, rotation });
            let root = match self.instantiate(&prefab, None, placed, renderer.as_deref_mut()) {
                Ok(root) => root,
                Err(error) => {
                    tracing::warn!("a crowd record's prefab {prefab}: {error:#}");
                    continue;
                }
            };
            // Kept as a record again when it goes far off, and saved as a copy a script made.
            let index = self
                .content
                .prefab(renderer.as_deref_mut(), &prefab)
                .and_then(|id| self.content.loaded_template(renderer.as_deref_mut(), id))
                .ok()
                .and_then(|template| template.root())
                .unwrap_or(0) as u32;
            let world = &mut self.engine.world;
            world.insert(root, Spawned { prefab: record.prefab, root, index });
            let member = world.get::<crate::crowds::CrowdMember>(root).copied().unwrap_or_default();
            world.insert(root, crate::crowds::CrowdMember { keep_as_record: true, ..member });
            crate::crowd_records::send_on(world, root, &record);
        }
    }

    /// Draws the records within Project Settings ▸ Crowds ▸ Draw records
    /// within of the camera, each as its prefab's models with nothing else
    /// of it there, playing their clips on the GPU (see `crowd_records`).
    fn draw_crowd_records(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        use crate::crowd_records::{Ghost, Ghosts, RecordGhost};
        let world = &mut self.engine.world;
        let within = world.resource::<crate::crowds::CrowdSettings>().map_or(600.0, |s| s.draw_records_within);
        let drawn = match within > 0.0 {
            true => crate::crowd_records::to_draw(world, within, crate::crowd_records::MOST_DRAWN),
            false => Vec::new(),
        };
        let mut ghosts = world.remove_resource::<Ghosts>().unwrap_or_default();
        let wanted: std::collections::HashSet<u64> = drawn.iter().map(|drawn| drawn.key).collect();
        ghosts.0.retain(|id, ghost| {
            let keep = wanted.contains(id) && world.is_alive(ghost.root);
            if !keep {
                ghost.parts.iter().for_each(|(part, ..)| drop(world.despawn(*part)));
                world.despawn(ghost.root);
            }
            keep
        });
        for drawn in &drawn {
            if !ghosts.0.contains_key(&drawn.key) {
                let template = match drawn.from {
                    crate::crowd_records::DrawnFrom::Record(guid) => {
                        self.content.prefab(renderer.as_deref_mut(), &guid.to_string())
                    }
                    crate::crowd_records::DrawnFrom::Figure(id) => Ok(id),
                };
                let template = template.and_then(|id| self.content.loaded_template(renderer.as_deref_mut(), id));
                let Ok(template) = template else { continue };
                let world = &mut self.engine.world;
                let root = world.spawn((Transform::from_position(drawn.at), RecordGhost));
                let mut parts = Vec::new();
                for obj in &template.objs {
                    let Some(model) = obj.model.as_ref().and_then(|model| model.0.clone()) else { continue };
                    // Where it is under the prefab's root, every parent between taken in.
                    let mut placed = if obj.parent.is_some() { obj.transform.matrix() } else { voxear_core::math::Mat4::IDENTITY };
                    let mut up = obj.parent;
                    while let Some(parent) = up.filter(|&parent| template.objs[parent].parent.is_some()) {
                        placed = template.objs[parent].transform.matrix() * placed;
                        up = template.objs[parent].parent;
                    }
                    let (scale, rotation, position) = placed.to_scale_rotation_translation();
                    let clips = model.animation.clone().map(|animation| {
                        let mut named = |words: &[&str]| {
                            let found = animation.clips.iter().find(|clip| {
                                let name = clip.name.to_lowercase();
                                words.iter().any(|word| name.contains(word))
                            })?;
                            crate::animation::fitted_clip(world, &found.name.clone(), &animation)
                        };
                        match obj.animator.as_ref().filter(|animator| !animator.clip.is_empty()) {
                            Some(animator) => {
                                let clip = crate::animation::fitted_clip(world, &animator.clip, &animation);
                                (clip.clone(), clip)
                            }
                            None => (named(&["walk", "run"]), named(&["idle", "stand"])),
                        }
                    });
                    let (walk, stand) = clips.unwrap_or((None, None));
                    let part = world.spawn((
                        model,
                        Transform::new(position, rotation, scale),
                        voxear_scene::Parent(root),
                        RecordGhost,
                    ));
                    parts.push((part, walk, stand));
                }
                ghosts.0.insert(drawn.key, Ghost { root, parts, time: (drawn.key % 97) as f32 * 0.37 });
            }
            if let Some(ghost) = ghosts.0.get_mut(&drawn.key) {
                crate::crowd_records::move_ghost(&mut self.engine.world, ghost, drawn, delta);
            }
        }
        self.engine.world.insert_resource(ghosts);
    }

    /// Makes what Heat Bodies' changes became this frame, each in its Obj's
    /// place under its parent, starting as warm as it was, the Obj taken
    /// away; then tells scripts of every change and every character that
    /// feels otherwise.
    fn make_heat_changes(&mut self, mut renderer: Option<&mut Renderer>) {
        let due = crate::heat::take_due(&mut self.engine.world);
        if due.changes.is_empty() && due.feels.is_empty() {
            return;
        }
        let mut events = Vec::with_capacity(due.changes.len() + due.feels.len());
        let mut names = Vec::with_capacity(events.capacity());
        for change in due.changes {
            let entity = change.entity;
            let mut became = None;
            if let Some(prefab) = change.becomes.as_deref().filter(|_| self.engine.world.is_alive(entity)) {
                let world = &self.engine.world;
                let parent = world.get::<Parent>(entity).map(|parent| parent.0);
                let placement =
                    world.get::<Transform>(entity).map(|t| Placement { position: t.position, rotation: t.rotation });
                let warmth = world.get::<crate::heat::HeatState>(entity).map(|state| state.temperature);
                match self.instantiate(prefab, parent, placement, renderer.as_deref_mut()) {
                    Ok(made) => {
                        let world = &mut self.engine.world;
                        if let (Some(body), Some(warmth)) = (world.get::<crate::heat::HeatBody>(made).cloned(), warmth) {
                            world.insert(made, crate::heat::HeatState::new(&body, warmth));
                        }
                        self.engine.destroy_objs(&[entity]);
                        became = Some(made);
                    }
                    Err(error) => tracing::error!("a Heat Body's change {} could not make {prefab}: {error:#}", change.name),
                }
            }
            let became = became.map_or(0, Entity::to_bits);
            events.push(HeatEventData { obj: entity.to_bits(), became, kind: 0, value: change.index as i32 });
            names.push(change.name);
        }
        for (entity, feels) in due.feels {
            let value = crate::heat::Feels::ALL.iter().position(|&f| f == feels).unwrap_or_default() as i32;
            events.push(HeatEventData { obj: entity.to_bits(), became: 0, kind: 1, value });
            names.push(String::new());
        }
        let names = names.join("\n");
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer,
            models: self.models,
            unload_requested: &mut self.unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.tell_heat_events(&events, &names);
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
        // A script's first, as what scripts set is laid over the level's.
        let script = world.resource::<voxear_render::ScriptPost>().and_then(|script| lut_in(script.overrides()));
        let named = script
            .or_else(|| world.resource::<PostTable>().and_then(|table| lut_in(&table.0)))
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
        self.prepare_sky_picture(renderer);
    }

    /// Uploads the picture a gradient sky names, on first use, and gives it
    /// and its light to the sky, the level's and a look pack's worn over it.
    fn prepare_sky_picture(&mut self, renderer: &mut Renderer) {
        let world = &self.engine.world;
        let named = world.resource::<voxear_render::Sky>().and_then(|sky| sky.picture);
        let worn = world.resource::<voxear_render::WornLook>().and_then(|worn| worn.sky.picture);
        for guid in [named, worn].into_iter().flatten() {
            let made = self.content.sky_picture(renderer, guid);
            let (texture, light) = match made {
                Ok(made) => made,
                Err(error) => {
                    if self.sky_failed != Some(guid) {
                        tracing::error!("the sky's picture: {error:#}");
                        self.sky_failed = Some(guid);
                    }
                    continue;
                }
            };
            let world = &mut self.engine.world;
            for sky in world.resource_mut::<voxear_render::Sky>().into_iter().filter(|sky| sky.picture == Some(guid)) {
                (sky.picture_texture, sky.picture_light) = (Some(texture), Some(light));
            }
            let worn = world.resource_mut::<voxear_render::WornLook>().map(|worn| &mut worn.sky);
            for sky in worn.into_iter().filter(|sky| sky.picture == Some(guid)) {
                (sky.picture_texture, sky.picture_light) = (Some(texture), Some(light));
            }
        }
    }

    pub fn tick(&mut self, delta: f32, mut renderer: Option<&mut Renderer>) {
        {
            profile_scope!("Script Reload Check");
            self.reload_changed_scripts();
        }
        // Between frames, before anything this frame reads where things are.
        if !self.engine.is_edit_mode()
            && let Some(by) = crate::origin::due(&self.engine.world, &self.project.file.floating_origin)
        {
            profile_scope!("Floating Origin");
            self.move_origin(by, renderer.as_deref_mut());
        }
        // Read once, before the scripts that ask about it.
        {
            profile_scope!("Input");
            self.input.update(delta);
        }
        let mouse = self.input.devices().mouse().delta;
        self.engine.world.insert_resource(crate::first_person::MouseLook { delta: mouse.into() });
        // The world's sky and weather are those of the dimension the camera is in, before anything reads them.
        let doomed = crate::dimensions::update(&mut self.engine.world, self.levels.current());
        if !doomed.is_empty() {
            self.engine.destroy_objs(&doomed);
        }
        // Rays and overlaps ask in the dimension the game is seen from.
        let current = crate::dimensions::current(&self.engine.world);
        if let Some(physics) = self.engine.world.resource_mut::<crate::physics::PhysicsScene>() {
            physics.world_mut().ask_in(current.0);
        }
        self.net_begin(delta, renderer.as_deref_mut());
        // A player's machine makes the dimensions its server names, and takes in their edits.
        self.join_dimensions(renderer.as_deref_mut());
        self.models = ModelWork::for_renderer(renderer.as_deref());
        {
            profile_scope!("Collider Meshes");
            give_collider_meshes(&mut self.engine.world, &mut self.content);
        }
        {
            profile_scope!("Particles");
            crate::particles::resolve(&mut self.engine.world, &mut self.content, renderer.as_deref_mut());
        }
        // Before physics steps, so ground made round the camera stands things on it this frame.
        {
            profile_scope!("Terrain Streaming");
            crate::terrain::stream(&mut self.engine.world, &mut self.content);
        }
        // The texture pack worn, where it changed: its materials first, then the worlds of blocks drawn with them.
        if let Some(renderer) = renderer.as_deref_mut() {
            crate::texture_packs::sync(&mut self.content, renderer);
        }
        {
            profile_scope!("Block Refresh");
            crate::blocks::refresh(&mut self.engine.world, &mut self.content, renderer.as_deref_mut());
        }
        // The far sides of Portals near by are made with the ground round the camera, and each Portal opens
        // once its own is.
        {
            profile_scope!("Portals");
            // A Portal in the gate of each dungeon in a dimension of its own near where the game is seen from, in
            // the editor too, where it is opened from to look round.
            crate::dungeons::stand_gates(&mut self.engine.world, &mut self.content);
            crate::portals::find_exits(&mut self.engine.world);
            // A Portal into a dimension not made yet makes it once near enough, and its far side is found next tick.
            self.link_dimensions(delta, renderer.as_deref_mut());
        }
        crate::blocks::stream(&mut self.engine.world, &mut self.content);
        crate::block_tints::update(&mut self.engine.world, &mut self.content);
        {
            profile_scope!("Portals");
            crate::portals::update_far(&mut self.engine.world, delta);
        }
        {
            profile_scope!("Block Flow");
            crate::blocks::flow(&mut self.engine.world, delta);
        }
        {
            profile_scope!("Block Tick");
            crate::blocks::tick(&mut self.engine.world, &mut self.content, delta);
            // Doors agents are about to go through, opened as a player would.
            crate::nav_agent::open_doors(&mut self.engine.world, &mut self.content);
        }
        crate::grown_plants::update(&mut self.engine.world, &mut self.content);
        // Trees coming down, and what they broke into, while the game plays.
        if !self.engine.is_edit_mode() {
            crate::trees::update(&mut self.engine.world, &mut self.content, delta);
            crate::saplings::update(&mut self.engine.world, &mut self.content, delta);
            crate::item_drops::update(&mut self.engine.world, &mut self.content, renderer.as_deref_mut(), delta);
            self.show_dropped_prefabs(renderer.as_deref_mut());
        }
        {
            profile_scope!("Heat and Storms");
            crate::heat::prepare(&mut self.engine.world, &mut self.content);
            crate::storms::prepare(&mut self.engine.world, &mut self.content);
        }
        // The meshes' boxes, as the renderer has them, for the scripts that ask.
        if let Some(renderer) = renderer.as_deref() {
            profile_scope!("Mesh Boxes");
            crate::mesh_boxes::refresh(&mut self.engine.world, renderer);
        }
        if crate::modules::has(&self.engine.world, crate::modules::Module::Guns) {
            crate::guns::contact::give_shapes(&mut self.engine.world, renderer.as_deref());
            crate::guns::handling::give_handlings(&mut self.engine.world, &mut self.content);
        }
        // After the handlings, whose actions may be based on clips.
        {
            profile_scope!("Clips");
            crate::animation::give_clips(&mut self.engine.world, &mut self.content);
            crate::animation::give_mob_bones(&mut self.engine.world, &mut self.content);
        }
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
        // What scripts asked of dimensions, made or let go before the frame is drawn.
        self.serve_dimension_asks(renderer.as_deref_mut());
        // Before the frame is drawn: what went through a Portal did not cross the world in a frame.
        {
            profile_scope!("Portals");
            crate::portals::tell_renderer(&mut self.engine.world, renderer.as_deref_mut());
        }
        {
            profile_scope!("Heat and Storms");
            self.make_heat_changes(renderer.as_deref_mut());
        }
        if !self.engine.is_edit_mode() {
            profile_scope!("Crowd Records");
            self.keep_crowd_records(delta, renderer.as_deref_mut());
        }
        if !self.engine.is_edit_mode() {
            profile_scope!("Streams With Ground");
            self.stream_with_ground(renderer.as_deref_mut());
        }
        // The columns made and let go, told to scripts while the game plays, and forgotten in the editor.
        let columns = crate::ground_streaming::take_untold(&mut self.engine.world);
        if !self.engine.is_edit_mode() {
            self.engine.tell_column_events(&columns);
        }
        // A block a script put down in a world that had not held it is drawn as it says from this frame.
        {
            profile_scope!("Block Refresh");
            crate::blocks::refresh(&mut self.engine.world, &mut self.content, renderer.as_deref_mut());
        }
        // Screen UIs show while the game plays: those made this frame, without their Objs gone.
        if !self.engine.is_edit_mode() {
            self.ui.sync_screens(&mut self.engine.world, &mut self.content);
        }
        if let Some(renderer) = renderer.as_deref_mut() {
            self.prepare_post(renderer);
        }
        self.ui.follow_language(self.engine.world.resource::<Localization>());
        // Localized Texts show their keys' text in their documents, once Screen UIs have loaded theirs.
        self.ui.show_localized_texts(&self.engine.world, self.engine.world.resource::<Localization>());
        if let Some(renderer) = renderer.as_deref_mut() {
            self.prompt_icons.follow(&self.input, &mut self.content, renderer);
        }
        self.ui.follow_input(&self.input, self.prompt_icons.sprites());
        // Maps show while the game plays, as the UI they are in does.
        if !self.engine.is_edit_mode() {
            self.maps.sync_huds(&mut self.engine.world, &mut self.ui, &mut self.input, &mut self.display);
            let (scale, numbers) = (self.ui.scale(), self.ui.map_view_numbers());
            let world = &mut self.engine.world;
            self.maps.update(world, &mut self.content, renderer.as_deref_mut(), (self.ui.ui(), scale), &numbers);
        }
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
        let look_pack = self.engine.scripts_mut().and_then(|scripts| scripts.take_look_pack_requests().pop());
        if let Some((reference, remember)) = look_pack {
            match crate::look_packs::wear(&mut self.engine.world, &mut self.content, &reference) {
                // The choice goes with the player to the game's next run.
                Ok(()) if remember => {
                    crate::look_packs::remember(crate::look_packs::worn(&self.engine.world).unwrap_or_default());
                }
                Ok(()) => {}
                Err(error) => tracing::error!(target: "script", "{error:#}"),
            }
        }
        self.shader_pack_requests();
        let texture_pack = self.engine.scripts_mut().and_then(|scripts| scripts.take_texture_pack_requests().pop());
        if let Some((reference, remember)) = texture_pack {
            match crate::texture_packs::wear(&mut self.content, &reference) {
                Ok(()) if remember => {
                    crate::texture_packs::remember(crate::texture_packs::worn(&self.content).unwrap_or_default());
                }
                Ok(()) => {}
                Err(error) => tracing::error!(target: "script", "{error:#}"),
            }
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
            self.audio.trace_sound(renderer.as_deref_mut());
        }
        // A server tells its clients the level it plays and shares new networked Objs, then whatever this
        // frame left to send goes.
        crate::net::set_level(&mut self.engine.world, self.levels.current());
        self.share_net_objects();
        self.serve_dimensions();
        crate::net::frame_end(&mut self.engine.world);
        if std::mem::take(&mut self.unload_requested) {
            self.release_unused(renderer);
        }
    }

    /// Moves the world's middle by `by` whole metres (see `crate::origin`):
    /// every Obj, physics, effects, sound and what the renderer keeps from
    /// last frame move together, so nothing seen jumps, then scripts hear of
    /// it. The game moves it itself as the viewer goes far out; this is for
    /// a game that wants to move it somewhere else.
    pub fn move_origin(&mut self, by: IVec3, mut renderer: Option<&mut Renderer>) {
        self.engine.move_origin(by);
        self.audio.shift(by.as_vec3());
        if let Some(renderer) = renderer.as_deref_mut() {
            renderer.shift_origin(by.as_vec3());
        }
        let mut services = GameServices {
            content: &mut self.content,
            loads: &mut self.loads,
            levels: &self.levels,
            renderer,
            models: self.models,
            unload_requested: &mut self.unload_requested,
            previews: &mut self.previews,
            ui: &mut self.ui,
            display: &mut self.display,
            audio: &mut self.audio,
            input: &mut self.input,
        };
        let _scope = ServicesScope::enter(&mut services);
        self.engine.tell_origin_moved(by);
    }

    /// Networked play's start of a frame: what arrived, this frame's input
    /// for the local player, the session's tick rate for the fixed steps,
    /// and on a client the level its server plays.
    fn net_begin(&mut self, delta: f32, renderer: Option<&mut Renderer>) {
        let Some(rate) = self.engine.world.resource::<crate::net::NetSession>().map(|s| s.tick_rate()) else { return };
        let actions = self.input.actions().len();
        let local: Vec<voxear_input::ActionState> =
            (0..actions).map(|action| self.input.state(voxear_input::ActionId(action))).collect();
        crate::net::frame_begin(&mut self.engine.world, delta, &local);
        self.engine.set_fixed_delta(1.0 / rate as f32);
        let mut renderer = renderer;
        if let Some(level) = crate::net::take_level_request(&mut self.engine.world)
            && self.levels.current() != Some(level.as_str())
            && let Err(error) = self.load_level(&level, renderer.as_deref_mut())
        {
            tracing::error!("could not load the server's level {level}: {error:#}");
        }
        self.make_net_objects(renderer);
        self.engine.reconcile();
    }

    /// On a server, shares each networked Obj not yet shared: one its
    /// scene placed, found on players' machines where their scene placed
    /// it, or a prefab's copy, made there from the prefab. Any other is
    /// left unshared, with a warning, as players could not make it.
    fn share_net_objects(&mut self) {
        use crate::net::{NetId, NetObject, NetRole, NetSession, Networked, Origin};
        let world = &mut self.engine.world;
        let serving = world.resource::<NetSession>().is_some_and(|s| matches!(s.role(), NetRole::Server | NetRole::Host));
        if !serving {
            return;
        }
        let new: Vec<Entity> = world
            .query_ref::<(Entity, &NetObject)>()
            .without::<Networked>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, _)| entity)
            .collect();
        for entity in new {
            // Placed by a dimension's file, it is found by that file's GUID and its path there.
            let in_dimension = || {
                let dimensions = world.resource::<crate::dimensions::Dimensions>()?;
                dimensions.made().find_map(|made| {
                    let (path, _) = made.objs.iter().find(|(_, placed)| *placed == entity)?;
                    Some(Origin::Placed { scene: made.asset?.to_string(), path: path.to_string() })
                })
            };
            let origin = match self.levels.place_of(entity) {
                Some((scene, path)) => Some(Origin::Placed { scene: scene.to_string(), path: path.to_string() }),
                None => in_dimension().or_else(|| {
                    world
                        .get::<Spawned>(entity)
                        .filter(|s| s.root == entity)
                        .map(|s| Origin::Prefab(s.prefab.to_string()))
                }),
            };
            match origin {
                Some(origin) => drop(crate::net::track(world, entity, origin)),
                None => {
                    let name = world.get::<voxear_scene::Name>(entity).map_or("An Obj".into(), |n| n.0.clone());
                    tracing::warn!("{name} has a Net Object but is neither placed in a scene nor a prefab's root, so players cannot make it");
                    world.insert(entity, Networked { id: NetId(0), owner: None });
                }
            }
        }
    }

    /// On a client, finds or makes the networked Objs the server shared,
    /// and destroys those it took away.
    fn make_net_objects(&mut self, mut renderer: Option<&mut Renderer>) {
        use crate::net::Origin;
        for request in crate::net::take_spawns(&mut self.engine.world) {
            let entity = match &request.origin {
                Origin::Placed { scene, path } => {
                    let place = scene.parse().ok().zip(ObjPath::parse(path).ok());
                    place.and_then(|(scene, path)| {
                        // In the level's scenes, or in the file of the dimension the server has it in.
                        self.levels.entity_at(scene, &path).or_else(|| {
                            let dimensions = self.engine.world.resource::<crate::dimensions::Dimensions>()?;
                            let made = dimensions.get(request.dimension).filter(|made| made.asset == Some(scene))?;
                            dimensions.obj_at(made.id, &path)
                        })
                    })
                }
                Origin::Prefab(prefab) => {
                    let placement = Placement { position: request.state.position, rotation: request.state.rotation };
                    let made = self.instantiate(prefab, None, Some(placement), renderer.as_deref_mut());
                    made.inspect_err(|error| tracing::error!("could not make the server's {prefab}: {error:#}")).ok()
                }
            };
            match entity {
                Some(entity) => {
                    // In the dimension the server has it in.
                    let world = &mut self.engine.world;
                    if crate::dimensions::dimension_of(world, entity) != request.dimension {
                        voxear_scene::put_in_dimension(world, entity, request.dimension);
                    }
                    crate::net::bind(world, &request, entity);
                }
                None => tracing::warn!("the server shared {:?}, which is not here", request.origin),
            }
        }
        let gone = crate::net::take_despawns(&mut self.engine.world);
        if !gone.is_empty() {
            self.engine.destroy_objs(&gone);
        }
    }
}

/// How a key for an Obj a script made from a prefab starts (see
/// [`ScriptServices::obj_key`]).
const SPAWNED_KEY: &str = "spawned:";

/// What scripts reach through native calls while the game runs: content,
/// and the renderer to upload what they load.
/// An Obj and every Obj under it, parents first.
fn family(world: &World, root: Entity) -> Vec<Entity> {
    let mut children: HashMap<Entity, Vec<Entity>> = HashMap::new();
    for (child, parent) in world.query_ref::<(Entity, &Parent)>().without::<Destroying>() {
        children.entry(parent.0).or_default().push(child);
    }
    let mut family = vec![root];
    let mut at = 0;
    while let Some(&entity) = family.get(at) {
        family.extend(children.get(&entity).into_iter().flatten().copied());
        at += 1;
    }
    family
}

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

    fn asset_fields(&mut self, reference: &str) -> Result<String> {
        self.content.data_asset_json(reference)
    }

    fn block_json(&mut self, reference: &str) -> Result<String> {
        crate::blocks::block_json(self.content, reference)
    }

    fn blocks_version(&mut self) -> u64 {
        self.content.blocks_version()
    }

    fn block_guids(&mut self) -> Vec<String> {
        crate::blocks::every_block(self.content)
    }

    fn biome_json(&mut self, reference: &str) -> Result<String> {
        crate::blocks::biome_json(self.content, reference)
    }

    fn look_packs_json(&mut self, world: &World) -> Result<String> {
        let folder = voxear_scripting::saves_folder().and_then(|saves| crate::look_packs::player_folder(&saves));
        Ok(crate::look_packs::json(self.content, world, folder.as_deref()))
    }

    fn shader_packs_json(&mut self, world: &World) -> Result<String> {
        let folder = voxear_scripting::saves_folder().and_then(|saves| crate::shader_packs::player_folder(&saves));
        Ok(crate::shader_packs::json(self.content, world, folder.as_deref()))
    }

    fn texture_packs_json(&mut self) -> Result<String> {
        let folder = voxear_scripting::saves_folder().and_then(|saves| crate::texture_packs::player_folder(&saves));
        Ok(crate::texture_packs::json(self.content, folder.as_deref()))
    }

    fn texture_pack_starter(&mut self, name: &str) -> Result<String> {
        let folder = voxear_scripting::saves_folder().and_then(|saves| crate::texture_packs::player_folder(&saves));
        let folder = folder.context("this game has nowhere to keep a player's texture packs")?;
        let file = crate::texture_packs::write_starter(self.content, &folder, name)?;
        Ok(format!("{}{}", crate::texture_packs::FILE_PREFIX, file.display()))
    }

    fn biome_at(&mut self, world: &World, at: [f32; 3]) -> Option<String> {
        crate::blocks::biome_at(world, self.content, at.into()).map(|guid| guid.to_string())
    }

    fn caves_json(&mut self, world: &World, obj: u64, what: i32, at: [f32; 4]) -> Result<String> {
        crate::caves::caves_json(world, self.content, obj, what, at)
    }

    fn dungeons_json(&mut self, world: &World, obj: u64, at: [f32; 4]) -> Result<String> {
        crate::dungeons::dungeons_json(world, self.content, obj, at)
    }

    fn endless_changes(&mut self, world: &World, entity: Entity) -> Option<voxear_terrain::blocks::edits::BlockEdits> {
        crate::blocks::endless_changes(world, entity)
    }

    fn put_endless_changes(
        &mut self,
        world: &mut World,
        entity: Entity,
        edits: &voxear_terrain::blocks::edits::BlockEdits,
    ) -> bool {
        crate::blocks::put_endless_changes(world, entity, edits)
    }

    fn dimension_edits(&mut self, world: &mut World) -> Option<Vec<u8>> {
        Some(crate::dimension_saves::capture_all(world, self.content, self.levels.current()))
    }

    fn apply_dimension_edits(&mut self, world: &mut World, bytes: &[u8]) -> Result<i32> {
        crate::dimension_saves::apply_all(world, self.content, self.levels.current(), bytes)
    }

    fn block_guid(&mut self, world: &World, entity: Entity, block: u16) -> Option<String> {
        crate::blocks::block_guid(world, entity, block)
    }

    fn block_number(&mut self, world: &mut World, entity: Entity, reference: &str, grow: bool) -> Option<u16> {
        crate::blocks::block_number(world, self.content, entity, reference, grow)
    }

    fn block_number_state(&mut self, world: &mut World, entity: Entity, reference: &str, state: u8, grow: bool) -> Option<u16> {
        crate::blocks::block_number_state(world, self.content, entity, reference, state, grow)
    }

    fn block_use(&mut self, world: &mut World, entity: Entity, at: [i32; 3]) -> Option<(u8, bool)> {
        use voxear_terrain::blocks::shape::UseKind;
        let used = crate::blocks::use_block(world, self.content, entity, at)?;
        let kind = [UseKind::None, UseKind::Opens, UseKind::Toggles, UseKind::Presses, UseKind::Says];
        Some((kind.iter().position(|kind| *kind == used.kind).unwrap_or(0) as u8, used.now))
    }

    fn block_change_state(&mut self, world: &mut World, entity: Entity, at: [i32; 3], state: u8, animate: bool) -> usize {
        crate::blocks::change_state(world, self.content, entity, at, state, animate)
    }

    fn door_halves(&mut self, world: &mut World, entity: Entity, changes: Vec<([i32; 3], u16)>) -> Vec<([i32; 3], u16)> {
        crate::blocks::with_door_halves(world, self.content, entity, changes)
    }

    fn tree_call(&mut self, world: &mut World, what: i32, at: [f32; 3], way: [f32; 3]) -> i32 {
        use crate::trees::{Chopped, block_at, chop, cut_through};
        let Some((entity, place)) = block_at(world, Vec3::from(at)) else { return 0 };
        if what == 2 {
            let grew = crate::saplings::grow_now(world, self.content, entity, place);
            return if matches!(grew, crate::saplings::Grew::Grew | crate::saplings::Grew::Done) { 4 } else { 0 };
        }
        let done = match what {
            0 => chop(world, self.content, entity, place, Vec3::from(way)),
            1 => cut_through(world, self.content, entity, place, Vec3::from(way)),
            _ => Chopped::No,
        };
        match done {
            Chopped::No => 0,
            Chopped::Thinner => 1,
            Chopped::Cut => 2,
            Chopped::Felled(_) => 3,
        }
    }

    fn sapling_plant(&mut self, world: &mut World, reference: &str, at: [f32; 3]) -> bool {
        crate::saplings::plant_at(world, self.content, Vec3::from(at), reference)
    }

    fn saplings(&mut self, world: &World) -> Vec<([f32; 3], f32)> {
        crate::saplings::all(world, self.content).into_iter().map(|(at, grown)| (at.to_array(), grown)).collect()
    }

    fn trees_felled(&mut self, world: &mut World) -> Vec<voxear_scripting::FelledInfo> {
        crate::trees::take_felled(world)
            .into_iter()
            .map(|tree| voxear_scripting::FelledInfo {
                world: tree.world.to_bits(),
                cut: tree.cut.to_array(),
                away: tree.away.to_array(),
                landed: tree.landed.to_array(),
                drops: tree.drops.iter().map(|drop| drop.to_bits()).collect(),
            })
            .collect()
    }

    fn item_drop(&mut self, world: &mut World, reference: &str, count: u32, at: [f32; 3], velocity: [f32; 3]) -> u64 {
        crate::item_drops::drop_item(world, reference, count, Vec3::from(at), Vec3::from(velocity)).to_bits()
    }

    fn item_drops_near(&mut self, world: &World, at: [f32; 3], radius: f32) -> Vec<u64> {
        crate::item_drops::near(world, Vec3::from(at), radius).into_iter().map(|drop| drop.to_bits()).collect()
    }

    fn item_drop_read(&mut self, world: &mut World, drop: u64, take: bool) -> Option<(String, u32, [f32; 3])> {
        let entity = Entity::from_bits(drop);
        let at = world.get::<voxear_scene::Transform>(entity)?.position.to_array();
        let (item, count) = match take {
            true => crate::item_drops::take(world, entity)?,
            false => {
                let drop = world.get::<crate::item_drops::ItemDrop>(entity).filter(|_| !world.has::<voxear_scene::Destroying>(entity))?;
                (drop.item.clone(), drop.count)
            }
        };
        Some((item, count, at))
    }

    fn blocks_grown(&mut self, world: &World, entity: Entity) -> Vec<(u16, String)> {
        crate::blocks::blocks_numbered_since(world, entity)
    }

    fn block_listed(&mut self, world: &World, entity: Entity, block: u16) -> bool {
        crate::blocks::block_listed(world, entity, block)
    }

    fn data_assets(&mut self) -> Result<String> {
        Ok(self.content.data_assets_json())
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

    fn hdr(&mut self, set: Option<voxear_scripting::HdrInfo>) -> voxear_scripting::HdrInfo {
        use voxear_render::display::{DisplaySettings, Encoding, HdrChoice};
        if let Some(asked) = set {
            let hdr = match asked.mode {
                1 => HdrChoice::On,
                2 => HdrChoice::Hdr10,
                3 => HdrChoice::ScRgb,
                _ => HdrChoice::Off,
            };
            let asked = DisplaySettings { hdr, paper_white: asked.paper_white, peak: asked.peak }.held();
            self.display.hdr_changed |= asked != self.display.hdr;
            self.display.hdr = asked;
        }
        let (settings, shown) = (self.display.hdr, self.display.hdr_shown);
        voxear_scripting::HdrInfo {
            mode: match settings.hdr {
                HdrChoice::Off => 0,
                HdrChoice::On => 1,
                HdrChoice::Hdr10 => 2,
                HdrChoice::ScRgb => 3,
            },
            output: match shown.encoding {
                Encoding::Sdr => 0,
                Encoding::Hdr10 => 1,
                Encoding::ScRgb => 2,
            },
            offered: i32::from(shown.offered.hdr10) | i32::from(shown.offered.scrgb) << 1,
            paper_white: settings.paper_white,
            peak: settings.peak,
        }
    }

    fn cursor(&self) -> (bool, CursorLock) {
        (self.display.cursor_visible, self.display.cursor_lock)
    }

    fn set_cursor(&mut self, visible: bool, lock: CursorLock) {
        (self.display.cursor_visible, self.display.cursor_lock) = (visible, lock);
        self.display.cursor_changed = true;
    }

    fn ui_load(&mut self, reference: &str) -> Result<u32> {
        // By project path, or by GUID, as a UiDocumentAsset field names it.
        let path = match reference.parse::<AssetGuid>() {
            Ok(guid) => self.content.db().get(guid).with_context(|| format!("no asset has the GUID {guid}"))?.path.clone(),
            Err(_) => reference.to_string(),
        };
        self.ui.load(self.content, &path)
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
        // A picture is read here, where the project's pictures and the renderer are; with no renderer, an image
        // shows nothing.
        if let UiAsk::SetPicture(reference) = ask {
            let guid = self.content.parse(reference).ok().and_then(|parsed| parsed.guid());
            let shown = guid.zip(self.renderer.as_deref_mut()).and_then(|(guid, renderer)| {
                let read = self.content.ui_picture(renderer, guid);
                read.map_err(|error| tracing::warn!(target: "script", "{reference}: {error:#}")).ok()
            });
            let image = shown.map(|(texture, _)| (texture.0, [0.0, 0.0, 1.0, 1.0]));
            return self.ui.ask(element, UiAsk::SetImage(image));
        }
        self.ui.ask(element, ask)
    }

    fn ui_panel_document(&mut self, world: &mut World, entity: Entity) -> Option<u32> {
        self.ui.load_panel(world, self.content, entity)
    }

    fn ui_screen_document(&mut self, world: &mut World, entity: Entity) -> Option<u32> {
        self.ui.load_screen(world, self.content, entity)
    }

    fn load_prefab(&mut self, reference: &str) -> Result<u32> {
        Ok(self.content.prefab(self.renderer.as_deref_mut(), reference)?.0)
    }

    fn prefab_placement(&mut self, reference: &str) -> Option<String> {
        let id = self.content.prefab(self.renderer.as_deref_mut(), reference).ok()?;
        let template = self.content.loaded_template(self.renderer.as_deref_mut(), id).ok()?;
        let placement = template.objs.get(template.root()?)?.placement?;
        serde_json::to_string(&placement).ok()
    }

    fn set_model(&mut self, world: &mut World, entity: Entity, reference: &str) -> Result<()> {
        crate::model_renderers::set_model(world, self.content, self.renderer.as_deref_mut(), entity, reference)
    }

    fn change_model(
        &mut self,
        world: &mut World,
        entity: Entity,
        change: voxear_scripting::ModelChange<'_>,
    ) -> Result<()> {
        crate::model_renderers::change_model(world, self.content, self.renderer.as_deref_mut(), entity, change)
    }

    fn obj_asset(
        &mut self,
        world: &mut World,
        entity: Entity,
        which: voxear_scripting::ObjAsset,
        set: Option<Option<&str>>,
    ) -> Result<Option<String>> {
        crate::script_components::obj_asset(world, self.content, self.renderer.as_deref_mut(), entity, which, set)
    }

    fn model_part_name(&mut self, world: &World, entity: Entity, part: usize) -> Option<String> {
        crate::model_renderers::part_name(world, self.content, entity, part)
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

    fn copy_obj(
        &mut self,
        world: &mut World,
        original: Entity,
        parent: Option<Entity>,
        placement: Option<Placement>,
    ) -> Result<SpawnedPrefab> {
        anyhow::ensure!(world.is_alive(original), "the Obj to copy is gone");
        // Captured as a scene saves it, then made again as a prefab's Objs are, references between them kept.
        let objs: Vec<(ObjPath, Entity)> =
            family(world, original).into_iter().map(|entity| (ObjPath::default(), entity)).collect();
        let (file, ids) = crate::scene_file::capture_copy(world, self.content, &objs);
        let flat = crate::prefab::expand(self.content, &file.objs)?;
        let template = crate::prefab::Template::resolve(self.content, self.renderer.as_deref_mut(), &flat)?;
        let root = ids.get(&original).and_then(|id| flat.iter().position(|obj| obj.path == ObjPath::new(*id)));
        let root = root.or(template.root()).context("the Obj could not be copied")?;
        let entities = template.spawn_objs(world, parent, placement);
        let objs = entities.iter().enumerate().map(|(i, &entity)| (entity, template.behaviours(i, &entities))).collect();
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

    fn set_parent(
        &mut self,
        world: &mut World,
        entity: Entity,
        parent: Option<Entity>,
        keep_world: bool,
    ) -> Option<Result<bool, String>> {
        let done = crate::obj_hierarchy::set_parent(world, Some(self.levels), entity, parent, keep_world);
        Some(done.map_err(|error| error.to_string()))
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
        // The plants scripts took away, by their Obj's key and their numbers.
        let mut plants: Vec<(String, Vec<u32>)> = world
            .query_ref::<(Entity, &voxear_scene::Plants)>()
            .into_iter()
            .filter(|(entity, plants)| !plants.removed.is_empty() && !is_doomed(world, doomed, *entity))
            .filter_map(|(entity, plants)| Some((self.obj_key(world, entity)?, plants.removed.iter().collect())))
            .collect();
        plants.sort();
        let plants: Vec<serde_json::Value> =
            plants.into_iter().map(|(key, gone)| serde_json::json!({ "obj": key, "gone": gone })).collect();
        // The Objs scripts put inside others, or at the top, but the copies' roots, which `spawned` places.
        let mut moved: Vec<(Entity, String)> = world
            .query_ref::<(Entity, &crate::obj_hierarchy::Reparented, Option<&Spawned>)>()
            .into_iter()
            .filter(|(entity, _, spawned)| spawned.is_none_or(|spawned| spawned.root != *entity))
            .filter(|(entity, ..)| !is_doomed(world, doomed, *entity))
            .filter_map(|(entity, ..)| Some((entity, self.obj_key(world, entity)?)))
            .collect();
        moved.sort_by_key(|(entity, _)| entity.index());
        // One inside an Obj a save cannot find again is left where its scene or prefab puts it.
        let moved: Vec<serde_json::Value> = moved
            .into_iter()
            .filter_map(|(entity, key)| {
                let parent = match world.get::<Parent>(entity) {
                    Some(parent) => Some(self.obj_key(world, parent.0)?),
                    None => None,
                };
                Some(serde_json::json!({ "obj": entity.to_bits(), "key": key, "parent": parent }))
            })
            .collect();
        let state =
            serde_json::json!({ "level": level, "gone": gone, "spawned": spawned, "plants": plants, "moved": moved });
        Some(state.to_string())
    }

    fn asset_guid(&self, asset: AssetName<'_>) -> Option<String> {
        let guid = match asset {
            AssetName::Reference(reference) => self.content.parse(reference).ok()?.guid()?,
            AssetName::Prefab(prefab) => self.content.prefab_guid(PrefabId(prefab))?,
            AssetName::Sound(sound) => self.content.sound_guid(SoundId(sound))?,
        };
        Some(guid.to_string())
    }

    fn terrain_surface(&self, world: &World, terrain: Entity, material: &str) -> Option<u8> {
        let guid = |reference: &str| self.content.parse(reference).ok()?.guid();
        let wanted = guid(material)?;
        let asset = world.get::<crate::terrain::TerrainAsset>(terrain)?;
        let surfaces = &asset.file.surfaces;
        let found = surfaces.iter().position(|surface| surface.material.as_deref().and_then(guid) == Some(wanted))?;
        u8::try_from(found).ok()
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
            camera: None,
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
    let drawn = meshes.filter(|(_, collider, _)| collider.shape == ColliderShape::Mesh).map(|(e, _, r)| (e, Ok(r.mesh)));
    // Where nothing draws the mesh, as on a server, by the mesh it names.
    let named = world.query_ref::<(Entity, &Collider, &crate::prefab::ColliderMeshSource)>().without::<Destroying>();
    let named = named.into_iter().filter(|(_, collider, _)| collider.shape == ColliderShape::Mesh);
    let wanted: Vec<(Entity, Result<MeshHandle, voxear_assets::AssetRef>)> =
        drawn.chain(named.map(|(e, _, source)| (e, Err(source.0.clone())))).collect();
    for (entity, mesh) in &wanted {
        let entity = *entity;
        let given = world.get::<ColliderMesh>(entity).map(|given| given.key);
        let triangles = match mesh {
            Ok(mesh) => content.triangles(*mesh),
            Err(reference) => content.triangles_of(reference),
        };
        match triangles {
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
