//! Levels, as CookieJarTools' LayerCake loads them. A level is a list of
//! scenes, loaded together:
//!
//! ```toml
//! format = 1
//! stands_on = "Levels/Core.level"   # a level whose scenes are always there
//!
//! [[scene]]
//! scene = "Scenes/Environment.scene"
//! weight = 3.0       # counts three times as much as a scene of weight 1 while loading
//!
//! [[scene]]
//! scene = "Scenes/Night.scene"
//! active = true      # where new Objs go, and whose sky and post-processing the level uses
//!
//! [[scene]]
//! scene = "Scenes/Ambience.scene"
//! required = false   # passed over if it fails to load, rather than stopping the change
//! reload = true      # reload even when the previous level had it too
//! ```
//!
//! **Standing on** another level puts its scenes first: managers, audio,
//! the persistent UI. Levels standing on the same one keep those scenes
//! loaded as one gives way to the next, so nothing in them is torn down and
//! built again. The **active** scene is the one a level marks, or its first;
//! only a level with no scenes of its own takes its base's.
//!
//! Loading a level keeps the scenes it shares with the current one, unloads
//! the rest (unless [`LevelOptions::unload_rest`] is off) and loads what is
//! new. Objs marked `persistent` survive every
//! change (Unity's `DontDestroyOnLoad`), along with their children. Unlike
//! Unity, reloading the scene a persistent Obj came from does not create a
//! second copy of it.
//!
//! Objs created at runtime, such as instantiated prefabs, belong to the
//! level's active scene and unload with it.
//!
//! The editor can also open a scene **alongside** the level, additively, as
//! Unity's Open Scene Additive does ([`Levels::open_scene_alongside`]), and
//! close one ([`Levels::close_scene`]). Opening the same level again, as the
//! editor does after changing it, keeps the scenes opened alongside it;
//! opening another level, or a scene on its own, closes them.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::{Entity, World};
use voxear_render::Renderer;
use voxear_scene::{Destroying, EditorOnly, Name, Parent, ScenePersistence};

use crate::lighting::Lighting;
use crate::{content::Content, engine::Engine, prefab::ObjPath, scene_file};

pub const LEVEL_FORMAT: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelFile {
    pub format: u32,
    /// A level whose scenes this one loads first and keeps between levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stands_on: Option<String>,
    #[serde(default, rename = "scene")]
    pub scenes: Vec<LevelScene>,
    /// The level's own ray-traced audio, over the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<crate::sound_tracing::LevelAudio>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelScene {
    /// A scene GUID or project path.
    pub scene: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub reload: bool,
    /// Where new Objs go, and whose sky and post-processing the level uses.
    #[serde(default, skip_serializing_if = "is_false")]
    pub active: bool,
    /// Off, a scene that fails to load is passed over.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub required: bool,
    /// What it counts for while the level loads, against the others.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: f32,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_true(value: &bool) -> bool {
    *value
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

fn is_one(value: &f32) -> bool {
    *value == 1.0
}

/// How a level change is made, as LayerCake's `SceneLoadOptions`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelOptions {
    /// Wait, once everything is read in, to be let in: time for a loading
    /// screen to finish. Background loads only.
    pub hold: bool,
    /// Seconds a background change takes at the least, however quickly it
    /// loads, so a loading screen is seen.
    pub least_time: f32,
    /// Unload the scenes loaded before that the new level does not want.
    pub unload_rest: bool,
    /// Make the new level's active scene the active one; off, the scene
    /// active before stays so while it is loaded.
    pub set_active: bool,
    /// Free what nothing uses once the change is over.
    pub tidy_up: bool,
    /// Load again the scenes the new level shares with those loaded now,
    /// as if each were marked `reload`. For a single scene loaded alongside
    /// ([`Levels::add_scene`]), load it again if it is loaded already.
    pub reload_shared: bool,
}

impl Default for LevelOptions {
    fn default() -> Self {
        Self { hold: false, least_time: 0.0, unload_rest: true, set_active: true, tidy_up: true, reload_shared: false }
    }
}

/// Which scenes a level change kept, loaded and unloaded, by path, and
/// those passed over because they failed to load and were not required.
#[derive(Debug, Default)]
pub struct LevelChange {
    pub kept: Vec<String>,
    pub loaded: Vec<String>,
    pub unloaded: Vec<String>,
    pub skipped: Vec<String>,
}

struct LoadedScene {
    guid: AssetGuid,
    path: String,
    /// Each Obj's path in the scene file, and its entity.
    objs: Vec<(ObjPath, Entity)>,
    /// Its lighting settings, as it names them and worked out, and its sky
    /// and post-processing, for when it is the active scene.
    lighting: Option<(String, Lighting)>,
    sky: Option<scene_file::SkyData>,
    post: Option<toml::Table>,
    /// Its own sun, lamps and weather.
    own: scene_file::OwnSun,
    /// For a `.dimension` opened as a scene, as the editor opens one: the
    /// dimension its Objs are in and its file's settings.
    dimension: Option<OpenDimension>,
}

/// A `.dimension` open as a scene: the dimension made for it and its file's
/// `[dimension]` settings, written back with its Objs.
#[derive(Clone, Debug)]
pub struct OpenDimension {
    pub id: crate::dimensions::DimensionId,
    pub settings: crate::dimensions::DimensionSettings,
}

/// A scene a level wants, from its file or the level it stands on.
pub(crate) struct Target {
    pub guid: AssetGuid,
    pub path: String,
    pub reload: bool,
    pub required: bool,
}

#[derive(Default)]
pub struct Levels {
    current: Option<String>,
    scenes: Vec<LoadedScene>,
    /// The scene runtime Objs go into and whose environment is used.
    active: Option<AssetGuid>,
    /// Persistent Objs that outlived their scene, by scene and path.
    persistent: HashMap<(AssetGuid, ObjPath), Entity>,
    /// The scenes the open level, or the scene opened on its own, asks for;
    /// any other open scene was opened alongside them.
    wanted: Vec<AssetGuid>,
    /// Lighting settings a script applied, over the scenes' own, until the
    /// level changes.
    applied_lighting: Option<(String, Lighting)>,
}

impl Levels {
    /// The path of the level loaded last.
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn loaded_scenes(&self) -> impl Iterator<Item = &str> {
        self.scenes.iter().map(|s| s.path.as_str())
    }

    /// The path of the scene runtime Objs go into and whose sky and
    /// post-processing the level uses.
    pub fn active_scene(&self) -> Option<&str> {
        self.scenes.iter().find(|s| Some(s.guid) == self.active).map(|s| s.path.as_str())
    }

    /// Changes level now, with the default [`LevelOptions`].
    pub fn load(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        level: &str,
    ) -> Result<LevelChange> {
        self.load_with(engine, content, renderer, level, &LevelOptions::default())
    }

    /// Changes level now, as `options` say.
    pub fn load_with(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        level: &str,
        options: &LevelOptions,
    ) -> Result<LevelChange> {
        let (level_path, mut targets, active) = read_level(content, level)?;
        let wanted: Vec<AssetGuid> = targets.iter().map(|t| t.guid).collect();
        // The open level opened again keeps the scenes opened alongside it.
        if self.current.as_deref() == Some(level_path.as_str()) {
            let alongside = self.scenes.iter().filter(|s| !self.wanted.contains(&s.guid) && !wanted.contains(&s.guid));
            targets.extend(alongside.map(|scene| Target {
                guid: scene.guid,
                path: scene.path.clone(),
                reload: false,
                required: false,
            }));
        }
        let change = self.change(engine, content, renderer, (level_path, targets, active), options)?;
        self.wanted = wanted;
        Ok(change)
    }

    /// Loads `scenes` again from their files, as after a tool wrote them,
    /// leaving the other open scenes as they are and keeping the active one.
    /// Their Objs come back with the paths their files give them.
    pub fn reload_scenes(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        scenes: &[AssetGuid],
    ) -> Result<LevelChange> {
        let Some(level) = self.current.clone() else { anyhow::bail!("no level or scene is open") };
        let targets = self
            .scenes
            .iter()
            .map(|s| Target { guid: s.guid, path: s.path.clone(), reload: scenes.contains(&s.guid), required: true })
            .collect();
        let options = LevelOptions { set_active: false, tidy_up: false, ..LevelOptions::default() };
        let active = self.active;
        self.change(engine, content, renderer, (level, targets, active), &options)
    }

    /// Whether an open scene's file holds what the scene has now, as saving
    /// it would write it: so a file changed on disk is told from one the
    /// editor saved itself. A particle effect open as a scene always is.
    pub fn scene_matches_disk(&self, engine: &Engine, content: &mut Content, project: &Path, scene: AssetGuid) -> bool {
        let Some(open) = self.scenes.iter().find(|s| s.guid == scene) else { return true };
        if open.path.ends_with(".particles") {
            return true;
        }
        let (mut captured, _) = scene_file::capture_scene_with_ids(&engine.world, content, &open.objs);
        (captured.sky, captured.post) = (open.sky, open.post.clone());
        captured.set_own_sun(open.own.clone());
        captured.lighting = open.lighting.as_ref().map(|(reference, _)| reference.clone());
        if let Some(dimension) = &open.dimension {
            let (now, on_disk) = dimension_texts(&project.join(&open.path), &dimension.settings, captured);
            return on_disk.is_some() && on_disk == now.ok();
        }
        let on_disk = scene_file::read(&project.join(&open.path)).ok().and_then(|s| toml::to_string(&s).ok());
        on_disk.is_some() && on_disk == toml::to_string(&captured).ok()
    }

    /// The open scenes, by GUID, with their paths.
    pub fn open_scenes(&self) -> Vec<(AssetGuid, &str)> {
        self.scenes.iter().map(|s| (s.guid, s.path.as_str())).collect()
    }

    /// Opens one scene on its own, as the editor does for a scene that is in
    /// no level: everything else unloads and it is the active scene. A prefab
    /// opens the same way, to edit, its Objs as a scene's, and saves back as
    /// a prefab.
    pub fn open_scene(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        scene: &str,
    ) -> Result<LevelChange> {
        let (guid, path) = find_scene(content, scene)?;
        let target = Target { guid, path: path.clone(), reload: false, required: true };
        let change =
            self.change(engine, content, renderer, (path, vec![target], Some(guid)), &LevelOptions::default())?;
        self.wanted = vec![guid];
        Ok(change)
    }

    /// Opens a scene alongside those open, additively, as the editor does:
    /// nothing else unloads, and the active scene stays as it was, its sky
    /// and post-processing with it. Returns whether it opened, as a scene
    /// open already is left as it is.
    pub fn open_scene_alongside(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        scene: AssetGuid,
    ) -> Result<bool> {
        if self.scenes.iter().any(|s| s.guid == scene) {
            return Ok(false);
        }
        let entry = content.db().get(scene).with_context(|| format!("no asset with GUID {scene}"))?;
        ensure!(
            matches!(entry.kind, AssetKind::Scene | AssetKind::Dimension),
            "{} is not a .scene or .dimension file",
            entry.path
        );
        let target = Target { guid: scene, path: entry.path.clone(), reload: false, required: true };
        let loaded = self.load_scene(engine, content, renderer, &target)?;
        self.scenes.push(loaded);
        crate::reflection_bakes::put_on_level(self, &mut engine.world, content);
        crate::light_bakes::put_on_level(self, &mut engine.world, content);
        crate::navigation::put_level_nav(self, &mut engine.world, content);
        // Loading a scene puts its own sky up; the active scene's goes back.
        self.apply_environment(engine)?;
        Ok(true)
    }

    /// Loads one scene alongside those loaded, as scripts' `Scene.LoadAsync`
    /// does, without a level: nothing else unloads. A scene loaded already
    /// stays as it is, unless [`LevelOptions::reload_shared`] asks for it to
    /// load again; with [`LevelOptions::set_active`], it becomes the active
    /// scene. The other options are a level change's.
    pub fn add_scene(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        scene: &str,
        options: &LevelOptions,
    ) -> Result<LevelChange> {
        let (guid, path) = find_scene(content, scene)?;
        let mut change = LevelChange::default();
        if options.reload_shared && self.close_scene(engine, guid)? {
            change.unloaded.push(path.clone());
        }
        match self.open_scene_alongside(engine, content, renderer, guid)? {
            true => change.loaded.push(path),
            false => change.kept.push(path),
        }
        if options.set_active {
            self.set_active_scene(engine, guid)?;
        }
        Ok(change)
    }

    /// Makes a loaded scene the active one: where Objs made at runtime go,
    /// and whose sky and post-processing are used. Objs made at runtime
    /// before stay with the scene that was active then. Returns whether the
    /// scene is loaded.
    pub fn set_active_scene(&mut self, engine: &mut Engine, scene: AssetGuid) -> Result<bool> {
        if !self.scenes.iter().any(|s| s.guid == scene) {
            return Ok(false);
        }
        self.adopt_runtime_objs(engine);
        self.active = Some(scene);
        self.apply_environment(engine)?;
        Ok(true)
    }

    /// Closes one open scene, unloading its Objs; persistent ones stay only
    /// while the game runs. If it was the active scene, the last one open is
    /// now. Returns whether it was open.
    pub fn close_scene(&mut self, engine: &mut Engine, scene: AssetGuid) -> Result<bool> {
        let Some(at) = self.scenes.iter().position(|s| s.guid == scene) else { return Ok(false) };
        self.adopt_runtime_objs(engine);
        let closing = self.scenes.remove(at);
        let keep_persistent = !engine.is_edit_mode();
        for (id, entity) in unload(engine, &closing, keep_persistent) {
            self.persistent.insert((closing.guid, id), entity);
        }
        if self.active == Some(scene) {
            self.active = self.scenes.last().map(|s| s.guid);
        }
        self.apply_environment(engine)?;
        Ok(true)
    }

    /// Whether an open scene was opened alongside the level, or the scene
    /// opened on its own, rather than being one it asks for.
    pub fn opened_alongside(&self, scene: AssetGuid) -> bool {
        self.scenes.iter().any(|s| s.guid == scene) && !self.wanted.contains(&scene)
    }

    /// The `.dimension` files open as scenes, each with the dimension made
    /// for it.
    pub fn open_dimensions(&self) -> Vec<(AssetGuid, &str, &OpenDimension)> {
        self.scenes.iter().filter_map(|s| Some((s.guid, s.path.as_str(), s.dimension.as_ref()?))).collect()
    }

    /// Each Obj an open scene keeps, with its path in the scene's file.
    pub fn tracked_in(&self, scene: AssetGuid) -> Vec<(Entity, ObjPath)> {
        let open = self.scenes.iter().find(|s| s.guid == scene);
        open.map(|s| s.objs.iter().map(|(path, entity)| (*entity, path.clone())).collect()).unwrap_or_default()
    }

    /// The open scene that is a dimension's file, by its dimension.
    pub fn dimension_scene(&self, id: crate::dimensions::DimensionId) -> Option<AssetGuid> {
        self.scenes.iter().find(|s| s.dimension.as_ref().is_some_and(|d| d.id == id)).map(|s| s.guid)
    }

    /// The dimension an open scene's Objs are in: the level's for a scene,
    /// its own for a dimension's file.
    pub fn dimension_of_scene(&self, scene: AssetGuid) -> Option<crate::dimensions::DimensionId> {
        let open = self.scenes.iter().find(|s| s.guid == scene)?;
        Some(open.dimension.as_ref().map_or(crate::dimensions::DimensionId::LEVEL, |d| d.id))
    }

    /// Puts the root Objs of each open scene in its scene's dimension where
    /// they are not, as after the editor moves Objs between a level's scene
    /// and a dimension's file, or makes one in a dimension's. Returns how
    /// many moved.
    pub fn keep_scene_dimensions(&self, world: &mut World) -> usize {
        let mut moving = Vec::new();
        for scene in &self.scenes {
            let id = scene.dimension.as_ref().map_or(crate::dimensions::DimensionId::LEVEL, |d| d.id);
            for &(_, entity) in &scene.objs {
                let root = world.is_alive(entity) && !world.has::<Parent>(entity) && !world.has::<Destroying>(entity);
                if root && crate::dimensions::dimension_of(world, entity) != id {
                    moving.push((entity, id));
                }
            }
        }
        for &(entity, id) in &moving {
            crate::dimensions::set_dimension(world, entity, id);
        }
        moving.len()
    }

    /// Gives a dimension's file open as a scene new settings, which its next
    /// save writes, its dimension changed to match. False where it is not
    /// open.
    pub fn set_dimension_settings(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        scene: AssetGuid,
        settings: crate::dimensions::DimensionSettings,
    ) -> Result<bool> {
        let Some(open) = self.scenes.iter_mut().find(|s| s.guid == scene) else { return Ok(false) };
        let Some(dimension) = open.dimension.as_mut() else { return Ok(false) };
        dimension.settings = settings.clone();
        let scene = scene_file::SceneFile {
            lighting: open.lighting.as_ref().map(|(reference, _)| reference.clone()),
            sky: open.sky,
            post: open.post.clone(),
            sun: open.own.sun,
            lamps: open.own.lamps,
            weather: open.own.weather.clone(),
            ..scene_file::SceneFile::default()
        };
        let file = crate::dimensions::DimensionFile { settings: settings.clone(), scene };
        let surroundings = crate::dimensions::file_surroundings(engine, content, &file)?;
        crate::dimensions::change(&mut engine.world, dimension.id, settings, surroundings);
        Ok(true)
    }

    /// Moves from the scenes loaded now to those a level (its path, its
    /// scenes, and its active one) wants.
    fn change(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
        (level_path, targets, active): (String, Vec<Target>, Option<AssetGuid>),
        options: &LevelOptions,
    ) -> Result<LevelChange> {
        let mut change = LevelChange::default();
        self.adopt_runtime_objs(engine);

        let mut kept = Vec::new();
        for scene in std::mem::take(&mut self.scenes) {
            let wanted = targets.iter().any(|t| t.guid == scene.guid && !t.reload && !options.reload_shared);
            let unwanted = !targets.iter().any(|t| t.guid == scene.guid);
            if wanted || (unwanted && !options.unload_rest) {
                change.kept.push(scene.path.clone());
                kept.push(scene);
            } else {
                // In edit mode persistent Objs unload like any other: only a running game keeps them.
                let keep_persistent = !engine.is_edit_mode();
                for (id, entity) in unload(engine, &scene, keep_persistent) {
                    self.persistent.insert((scene.guid, id), entity);
                }
                change.unloaded.push(scene.path);
            }
        }

        for target in &targets {
            if kept.iter().any(|s| s.guid == target.guid) {
                continue;
            }
            match self.load_scene(engine, content, renderer.as_deref_mut(), target) {
                Ok(scene) => {
                    kept.push(scene);
                    change.loaded.push(target.path.clone());
                }
                Err(error) if !target.required => {
                    tracing::warn!("passed over {}, which is not required: {error:#}", target.path);
                    change.skipped.push(target.path.clone());
                }
                Err(error) => return Err(error),
            }
        }

        // Scenes the level does not want, kept when `unload_rest` is off, stay first.
        kept.sort_by_key(|scene| targets.iter().position(|t| t.guid == scene.guid).map_or(-1, |i| i as i64));
        self.scenes = kept;
        let loaded = |guid: &AssetGuid| self.scenes.iter().any(|s| s.guid == *guid);
        let still_active = self.active.filter(loaded);
        self.active = match still_active {
            Some(guid) if !options.set_active => Some(guid),
            _ => active.filter(loaded).or(self.scenes.last().map(|s| s.guid)),
        };
        // A new level is lit by its own scenes again.
        if self.current.as_deref() != Some(level_path.as_str()) {
            self.applied_lighting = None;
        }
        self.apply_environment(engine)?;
        self.current = Some(level_path);
        crate::reflection_bakes::put_on_level(self, &mut engine.world, content);
        crate::light_bakes::put_on_level(self, &mut engine.world, content);
        crate::navigation::put_level_nav(self, &mut engine.world, content);
        Ok(change)
    }

    fn load_scene(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        target: &Target,
    ) -> Result<LoadedScene> {
        // A prefab opened on its own reads as a scene: both are a list of Objs.
        // And a particle effect opened on its own as a scene of its emitters, an Obj each.
        // And a dimension's file as a scene of its Objs, spawned in a dimension of its own.
        let kind = content.db().get(target.guid).map_or(AssetKind::Scene, |entry| entry.kind);
        let kind = match kind {
            AssetKind::Prefab | AssetKind::Particles | AssetKind::Dimension => kind,
            _ => AssetKind::Scene,
        };
        let (text, path) = content.asset_text(target.guid, kind)?;
        let mut dimension = None;
        let scene = match kind {
            AssetKind::Particles => {
                let effect = voxear_render::ParticleEffect::from_toml(&text);
                crate::particles::effect_scene(&effect.map_err(|error| anyhow::anyhow!("reading {path}: {error}"))?)
            }
            AssetKind::Dimension => {
                let file = crate::dimensions::DimensionFile::parse(&text, &path)?;
                let surroundings = crate::dimensions::file_surroundings(engine, content, &file)
                    .with_context(|| format!("loading {path}"))?;
                let settings = file.settings.clone();
                let made = crate::dimensions::make_empty(
                    &mut engine.world,
                    settings.clone(),
                    Some(surroundings),
                    self.current.as_deref(),
                );
                let id = made.with_context(|| format!("making {path}: there are as many dimensions as there can be"))?;
                dimension = Some(OpenDimension { id, settings });
                file.scene
            }
            _ => scene_file::parse(&text, &path)?,
        };
        self.persistent.retain(|_, entity| engine.world.is_alive(*entity));
        let survivors: HashSet<ObjPath> =
            self.persistent.keys().filter(|(guid, _)| *guid == target.guid).map(|(_, path)| path.clone()).collect();
        let objs = match &dimension {
            // A dimension's file leaves the world's sky alone: its own is kept for when it is seen.
            Some(open) => {
                let objs = scene_file::spawn_objs(engine, content, renderer, &scene, &survivors);
                let objs = objs.with_context(|| format!("loading {}", target.path));
                let objs = match objs {
                    Ok(objs) => objs,
                    Err(error) => {
                        crate::dimensions::let_go(&mut engine.world, open.id);
                        return Err(error);
                    }
                };
                let world = &mut engine.world;
                for &(_, entity) in &objs {
                    if !world.has::<Parent>(entity) {
                        crate::dimensions::set_dimension(world, entity, open.id);
                    }
                }
                crate::dimensions::record_made(world, open.id, (Some(target.guid), 0), objs.clone());
                objs
            }
            None => scene_file::spawn(engine, content, renderer, &scene, &survivors)
                .with_context(|| format!("loading {}", target.path))?,
        };
        // A scene's Objs are where it puts them from the level's real middle, wherever the world's has got to.
        if voxear_scene::WorldOrigin::of(&engine.world).is_moved() {
            let world = &mut engine.world;
            for &(_, entity) in &objs {
                let Some(file) = world.get::<voxear_scene::Transform>(entity).map(|t| t.position) else { continue };
                if !world.has::<Parent>(entity) {
                    voxear_scene::place_at_level(world, entity, file.as_dvec3());
                }
            }
        }
        let own = scene.own_sun();
        let lighting = match scene.lighting {
            Some(reference) => {
                let lit = content.lighting(&content.parse(&reference)?).with_context(|| format!("loading {reference}"))?;
                Some((reference, lit))
            }
            None => None,
        };
        let (sky, post) = (scene.sky, scene.post);
        Ok(LoadedScene { guid: target.guid, path: target.path.clone(), objs, lighting, sky, post, own, dimension })
    }

    /// The sky and post-processing come from the active scene, or failing
    /// that, the last one with any; lighting a script applied replaces them.
    /// A dimension's file lights its own dimension, not the level. While
    /// another dimension is current, the level's are set aside for it.
    fn apply_environment(&self, engine: &mut Engine) -> Result<()> {
        let away = crate::dimensions::level_aside(&mut engine.world);
        let applied = self.apply_level_environment(engine);
        crate::dimensions::level_back(&mut engine.world, away);
        applied
    }

    fn apply_level_environment(&self, engine: &mut Engine) -> Result<()> {
        if let Some((reference, lit)) = &self.applied_lighting {
            let lighting = Some((reference.as_str(), lit));
            return scene_file::apply_environment(engine, scene_file::Environment { lighting, ..Default::default() });
        }
        let level = || self.scenes.iter().rev().filter(|s| s.dimension.is_none());
        let active = level().find(|s| Some(s.guid) == self.active);
        let sky = active.and_then(|s| s.sky).or_else(|| level().find_map(|s| s.sky));
        let post = active.and_then(|s| s.post.as_ref()).or_else(|| level().find_map(|s| s.post.as_ref()));
        let lighting = active
            .and_then(|s| s.lighting.as_ref())
            .or_else(|| level().find_map(|s| s.lighting.as_ref()))
            .map(|(reference, lit)| (reference.as_str(), lit));
        let own = active
            .map(|s| &s.own)
            .filter(|own| !own.is_empty())
            .or_else(|| level().map(|s| &s.own).find(|own| !own.is_empty()));
        scene_file::apply_environment(engine, scene_file::Environment { lighting, sky, post, own })
    }

    /// The lighting settings the active scene names, as it names them.
    pub fn scene_lighting(&self) -> Option<&str> {
        let active = self.scenes.iter().find(|s| Some(s.guid) == self.active)?;
        active.lighting.as_ref().map(|(reference, _)| reference.as_str())
    }

    /// The own sky, post-processing, sun, lamps and weather of the scene that
    /// lights the level with them (see [`own_scene`](Self::own_scene)), as
    /// lighting settings write them. `None` without a scene open.
    pub fn own_lighting(&self) -> Option<Result<crate::lighting::LightingFile>> {
        let scene = &self.scenes[self.own_scene()?];
        Some(scene_file::own_lighting(scene.sky, scene.post.as_ref(), &scene.own))
    }

    /// Gives the scene that lights the level with its own settings other
    /// ones, as lighting settings write them, and lights the level with
    /// them. The scene keeps them when it is saved.
    pub fn set_own_lighting(&mut self, engine: &mut Engine, own: &crate::lighting::LightingFile) -> Result<()> {
        let index = self.own_scene().context("no scene is open")?;
        // A [sky] that writes only some settings is laid over the sky the level shows, where the scene has none.
        let world = &engine.world;
        let shown =
            world.resource::<scene_file::SkyFromLook>().is_none().then(|| world.resource::<voxear_render::Sky>());
        let shown = shown.flatten().map(scene_file::sky_data);
        let scene = &mut self.scenes[index];
        let (sky, post, sun) = scene_file::split_own_lighting(own, scene.sky.or(shown))?;
        (scene.sky, scene.post, scene.own) = (sky, post, sun);
        if scene.sky.is_none() {
            engine.world.insert_resource(scene_file::SkyFromLook);
        }
        self.apply_environment(engine)
    }

    /// The project path of the scene whose own settings light the level (see
    /// [`own_scene`](Self::own_scene)), which the Lighting page changes.
    pub fn own_scene_path(&self) -> Option<&str> {
        Some(self.scenes[self.own_scene()?].path.as_str())
    }

    /// The loaded scene whose own settings light the level while none names
    /// lighting settings: the active scene where it sets any, or failing
    /// that, the last that does, or else the active scene.
    fn own_scene(&self) -> Option<usize> {
        let sets =
            |s: &LoadedScene| s.dimension.is_none() && (s.sky.is_some() || s.post.is_some() || !s.own.is_empty());
        let active = self.scenes.iter().position(|s| Some(s.guid) == self.active);
        active.filter(|&index| sets(&self.scenes[index])).or_else(|| self.scenes.iter().rposition(sets)).or(active)
    }

    /// The lighting settings a loaded scene names, by its project path:
    /// `None` when it is not loaded, `Some(None)` when it names none.
    pub fn lighting_of(&self, path: &str) -> Option<Option<&str>> {
        let scene = self.scenes.iter().find(|s| s.path == path)?;
        Some(scene.lighting.as_ref().map(|(reference, _)| reference.as_str()))
    }

    /// Names other lighting settings in the active scene, or none, and
    /// lights the level with them. The scene keeps them when it is saved.
    pub fn set_scene_lighting(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        reference: Option<&str>,
    ) -> Result<()> {
        let path = self.active_scene().context("no scene is open")?.to_string();
        self.set_lighting_of(engine, content, &path, reference)
    }

    /// Names other lighting settings in the loaded scene at `path`, or
    /// none, and lights the level again. The scene keeps them when it is
    /// saved.
    pub fn set_lighting_of(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        path: &str,
        reference: Option<&str>,
    ) -> Result<()> {
        let lighting = match reference {
            Some(reference) => {
                let lit = content.lighting(&content.parse(reference)?).with_context(|| format!("loading {reference}"))?;
                Some((reference.to_string(), lit))
            }
            None => None,
        };
        let scene = self.scenes.iter_mut().find(|s| s.path == path).with_context(|| format!("{path} is not open"))?;
        scene.lighting = lighting;
        if scene.lighting.is_none() {
            engine.world.remove_resource::<scene_file::SceneLighting>();
            engine.world.remove_resource::<scene_file::LightingPost>();
        }
        self.apply_environment(engine)
    }

    /// Reads the loaded scenes' lighting settings again, and any a script
    /// applied, as after they were edited, and lights the level with them.
    pub fn refresh_lighting(&mut self, engine: &mut Engine, content: &mut Content) -> Result<()> {
        let named = self.scenes.iter_mut().filter_map(|s| s.lighting.as_mut()).chain(self.applied_lighting.as_mut());
        for (reference, lighting) in named {
            *lighting = content.lighting(&content.parse(reference)?).with_context(|| format!("loading {reference}"))?;
        }
        self.apply_environment(engine)
    }

    /// Lights the level with other lighting settings, as `Lighting.Apply`
    /// does, in place of its scenes' own until the level changes.
    pub fn apply_lighting(&mut self, engine: &mut Engine, content: &mut Content, reference: &str) -> Result<()> {
        let lighting = content.lighting(&content.parse(reference)?).with_context(|| format!("loading {reference}"))?;
        self.applied_lighting = Some((reference.to_string(), lighting));
        self.apply_environment(engine)
    }
}

impl Levels {
    /// Writes every loaded scene whose Objs changed back to its file in
    /// `project`, as the editor's Save does. Objs made since the level
    /// loaded go into the active scene. Scenes that are as they are on disk
    /// are left alone, comments and all. Returns the scenes written.
    pub fn save(&mut self, engine: &Engine, content: &mut Content, project: &Path) -> Result<Vec<String>> {
        self.adopt_runtime_objs(engine);
        let mut written = Vec::new();
        for scene in &mut self.scenes {
            let (mut captured, ids) = scene_file::capture_scene_with_ids(&engine.world, content, &scene.objs);
            // Voxels dug and placed are kept in region files beside the scene, which it names.
            let voxels = crate::voxel_volumes::save_changes(&engine.world, (project, &scene.path), &ids, &mut captured.objs)
                .with_context(|| format!("saving {}'s voxels", scene.path))?;
            written.extend(voxels);
            // The world's sky and lighting are the level's; each scene keeps its own.
            (captured.sky, captured.post) = (scene.sky, scene.post.clone());
            captured.set_own_sun(scene.own.clone());
            captured.lighting = scene.lighting.as_ref().map(|(reference, _)| reference.clone());
            let file = project.join(&scene.path);
            // A particle effect open as a scene goes back to its file as an effect: its Objs' Particle Systems.
            if scene.path.ends_with(".particles") {
                let effect = crate::particles::scene_effect(&captured);
                let text = effect.to_toml().map_err(|error| anyhow::anyhow!("saving {}: {error}", scene.path))?;
                if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
                    std::fs::write(&file, text).with_context(|| format!("saving {}", scene.path))?;
                    content.forget_particle_effect(scene.guid);
                    written.push(scene.path.clone());
                }
                continue;
            }
            // A dimension's file keeps its settings over its Objs.
            if let Some(dimension) = &scene.dimension {
                let (text, on_disk) = dimension_texts(&file, &dimension.settings, captured);
                let text = text.with_context(|| format!("saving {}", scene.path))?;
                if on_disk.as_deref() != Some(text.as_str()) {
                    std::fs::write(&file, text).with_context(|| format!("saving {}", scene.path))?;
                    written.push(scene.path.clone());
                }
            } else {
                let on_disk = scene_file::read(&file).ok().and_then(|s| toml::to_string(&s).ok());
                if on_disk.is_some() && on_disk == toml::to_string(&captured).ok() {
                    continue;
                }
                scene_file::write(&file, &captured).with_context(|| format!("saving {}", scene.path))?;
                written.push(scene.path.clone());
            }
            // Objs that had no id have one now, which the next save keeps.
            for (path, entity) in &mut scene.objs {
                if path.ids().is_empty()
                    && let Some(&id) = ids.get(entity)
                {
                    *path = ObjPath::new(id);
                }
            }
        }
        // Terrains the level changed, which are assets of their own.
        written.extend(crate::terrain::save(content, project)?);
        written.extend(crate::blocks::save(content, project)?);
        Ok(written)
    }

    /// The loaded level copied into `play` from `edit`'s Objs as they are
    /// now, edits included, rather than from its files, as entering play
    /// mode does. Each scene's Objs are captured and spawned again with the
    /// same paths, so persistent Objs are still recognised when their scene
    /// reloads, and behaviours kept as data in edit mode start for real when
    /// `play` has scripts.
    pub fn play_copy(
        &self,
        edit: &Engine,
        play: &mut Engine,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
    ) -> Result<Levels> {
        let (current, active, wanted) = (self.current.clone(), self.active, self.wanted.clone());
        let mut copy = Levels { current, active, wanted, ..Levels::default() };
        for scene in &self.scenes {
            let mut captured = scene_file::capture_scene(&edit.world, content, &scene.objs);
            (captured.sky, captured.post) = (scene.sky, scene.post.clone());
            captured.set_own_sun(scene.own.clone());
            captured.lighting = scene.lighting.as_ref().map(|(reference, _)| reference.clone());
            let (lighting, sky, post, own) = (scene.lighting.clone(), scene.sky, scene.post.clone(), scene.own.clone());
            // A dimension open in the editor goes into play as edited, made as a Portal into it would make it, so
            // it is let go as one would be once nothing needs it.
            if let Some(open) = &scene.dimension {
                let world = &edit.world;
                let surroundings = world
                    .resource::<crate::dimensions::Dimensions>()
                    .map(|dimensions| dimensions.surroundings(world, open.id));
                let current = copy.current.as_deref();
                let made = crate::dimensions::make_empty(&mut play.world, open.settings.clone(), surroundings, current);
                let id = made.context("there are as many dimensions as there can be")?;
                let objs = scene_file::spawn_objs(play, content, renderer.as_deref_mut(), &captured, &HashSet::new())
                    .with_context(|| format!("starting {}", scene.path))?;
                for &(_, entity) in &objs {
                    if !play.world.has::<Parent>(entity) {
                        crate::dimensions::set_dimension(&mut play.world, entity, id);
                    }
                }
                crate::dimensions::record_made(&mut play.world, id, (Some(scene.guid), 0), objs.clone());
                crate::dimensions::made_on_demand(&mut play.world, id);
                let dimension = Some(OpenDimension { id, settings: open.settings.clone() });
                let (guid, path) = (scene.guid, scene.path.clone());
                copy.scenes.push(LoadedScene { guid, path, objs, lighting, sky, post, own, dimension });
                continue;
            }
            let objs = scene_file::spawn(play, content, renderer.as_deref_mut(), &captured, &HashSet::new())
                .with_context(|| format!("starting {}", scene.path))?;
            let (guid, path) = (scene.guid, scene.path.clone());
            copy.scenes.push(LoadedScene { guid, path, objs, lighting, sky, post, own, dimension: None });
        }
        copy.apply_environment(play)?;
        // The level's lighting bake goes with it, as baked, even before the level is saved.
        crate::light_bakes::copy_to(&edit.world, &mut play.world, |entity| {
            let (scene, path) = self.place_of(entity)?;
            copy.entity_at(scene, &path)
        });
        // The level's nav mesh goes with it, as baked, even before the level is saved.
        if let Some(nav) = edit.world.resource::<crate::navigation::LevelNav>() {
            play.world.insert_resource(nav.clone());
        }
        if let Some(navs) = edit.world.resource::<crate::navigation::TypeNavs>() {
            play.world.insert_resource(navs.clone());
        }
        // The reflection probes' bakes go with them, as baked, even before the level is saved.
        for scene in &self.scenes {
            for (path, entity) in &scene.objs {
                let Some(bake) = edit.world.get::<voxear_render::ReflectionProbeBake>(*entity).cloned() else { continue };
                if let Some(played) = copy.entity_at(scene.guid, path) {
                    play.world.insert(played, bake);
                }
            }
        }
        Ok(copy)
    }

    /// The Obj in `other` loaded from the same place in the same scene as
    /// `entity` is here: an Obj's counterpart between edit and play mode.
    pub fn counterpart(&self, entity: Entity, other: &Levels) -> Option<Entity> {
        let (guid, path) = self
            .scenes
            .iter()
            .find_map(|scene| scene.objs.iter().find(|(_, e)| *e == entity).map(|(path, _)| (scene.guid, path)))?;
        let scene = other.scenes.iter().find(|scene| scene.guid == guid)?;
        scene.objs.iter().find(|(p, _)| p == path).map(|(_, e)| *e)
    }

    /// The Obj a loaded scene placed at `path`, as [`place_of`](Self::place_of)
    /// names it, if it is still there.
    pub fn entity_at(&self, scene: AssetGuid, path: &ObjPath) -> Option<Entity> {
        let loaded = self.scenes.iter().find(|loaded| loaded.guid == scene)?;
        loaded.objs.iter().find(|(p, _)| p == path).map(|(_, entity)| *entity)
    }

    /// Where an Obj was loaded from: its scene and its path in that scene's
    /// file, the same in every run, as a save names it. None for an Obj made
    /// while the game runs.
    pub fn place_of(&self, entity: Entity) -> Option<(AssetGuid, ObjPath)> {
        let loaded = self.scenes.iter().find_map(|scene| {
            scene
                .objs
                .iter()
                .find(|(path, e)| *e == entity && !path.is_root())
                .map(|(path, _)| (scene.guid, path.clone()))
        });
        loaded.or_else(|| self.persistent.iter().find(|(_, e)| **e == entity).map(|(place, _)| place.clone()))
    }

    /// The Objs that open scenes loaded and that have been destroyed since,
    /// or are being destroyed, by scene and path, as a save of the scene
    /// keeps them so they stay gone. `doomed` are Objs scripts asked to
    /// destroy that are still waiting, with everything under them.
    pub fn gone(&self, world: &World, doomed: &[Entity]) -> Vec<(AssetGuid, ObjPath)> {
        let gone = |entity: Entity| is_doomed(world, doomed, entity);
        self.scenes
            .iter()
            .flat_map(|scene| scene.objs.iter().map(move |(path, entity)| (scene.guid, path, *entity)))
            .filter(|(_, path, entity)| !path.is_root() && gone(*entity))
            .map(|(scene, path, _)| (scene, path.clone()))
            .collect()
    }

    /// The Obj loaded from `path` in `scene`, while that scene is open or the
    /// Obj outlived it as a persistent one.
    pub fn obj_at(&self, scene: AssetGuid, path: &ObjPath) -> Option<Entity> {
        let open = self.scenes.iter().find(|s| s.guid == scene);
        let loaded = open.and_then(|s| s.objs.iter().find(|(p, _)| p == path).map(|(_, entity)| *entity));
        loaded.or_else(|| self.persistent.get(&(scene, path.clone())).copied())
    }

    /// The scene an Obj is kept in.
    pub fn scene_of(&self, entity: Entity) -> Option<AssetGuid> {
        self.scenes.iter().find(|scene| scene.objs.iter().any(|(_, e)| *e == entity)).map(|scene| scene.guid)
    }

    /// The scene an Obj belongs to while the game runs: the scene that
    /// loaded it, or the one that loaded the nearest Obj it is inside; an
    /// Obj scripts made at the top belongs to the active scene, as it
    /// unloads with that.
    pub fn home_scene(&self, world: &World, entity: Entity) -> Option<AssetGuid> {
        let mut at = Some(entity);
        for _ in 0..256 {
            let Some(obj) = at else { break };
            if let Some(scene) = self.scene_of(obj) {
                return Some(scene);
            }
            at = world.get::<Parent>(obj).map(|parent| parent.0);
        }
        self.active_scene_guid()
    }

    /// Why `family`, an Obj and everything under it, may not go inside
    /// `parent`, or to the top with none, while the game runs: an Obj a
    /// scene loaded stays in that scene, so goes only inside an Obj of the
    /// same one. The scenes it and `parent` are in, by path, or `None` when
    /// it may.
    pub fn parent_refused(&self, world: &World, family: &[Entity], parent: Option<Entity>) -> Option<(String, String)> {
        let tracked = self.tracked(family);
        let own = family.first().and_then(|&obj| self.scene_of(obj));
        let home = match parent {
            Some(parent) => self.home_scene(world, parent),
            // At the top, an Obj a scene loaded stays in it; one scripts made goes into the active scene.
            None => own.or_else(|| self.active_scene_guid()),
        };
        let away = tracked.iter().find(|obj| Some(obj.scene) != home)?;
        let path = |guid: Option<AssetGuid>| {
            let scene = self.scenes.iter().find(|scene| Some(scene.guid) == guid);
            scene.map_or_else(|| "no scene".to_string(), |scene| scene.path.clone())
        };
        Some((path(Some(away.scene)), path(home)))
    }

    /// Where Objs are kept, in the order their scenes keep them.
    pub fn tracked(&self, entities: &[Entity]) -> Vec<Tracked> {
        let mut found = Vec::new();
        for scene in &self.scenes {
            for (index, (path, entity)) in scene.objs.iter().enumerate() {
                if entities.contains(entity) {
                    found.push(Tracked { scene: scene.guid, path: path.clone(), entity: *entity, index });
                }
            }
        }
        found
    }

    /// Each open scene, in the order they loaded, by GUID and path, with its
    /// Objs in the order it keeps them, which is the order its file writes
    /// them and the order the Hierarchy shows them in.
    pub fn scene_order(&self) -> Vec<(AssetGuid, &str, Vec<Entity>)> {
        self.scenes.iter().map(|s| (s.guid, s.path.as_str(), s.objs.iter().map(|(_, e)| *e).collect())).collect()
    }

    /// The scene Objs no scene loaded go into: the active one.
    pub fn active_scene_guid(&self) -> Option<AssetGuid> {
        self.active_index().map(|i| self.scenes[i].guid)
    }

    /// Stops keeping Objs in their scenes, as when the editor deletes them
    /// or moves them, and returns where each was, to put back with
    /// [`track`](Self::track).
    pub fn untrack(&mut self, entities: &[Entity]) -> Vec<Tracked> {
        let found = self.tracked(entities);
        for scene in &mut self.scenes {
            scene.objs.retain(|(_, entity)| !entities.contains(entity));
        }
        found
    }

    /// Keeps Objs in scenes again, each at the place [`untrack`] found it,
    /// so a scene's order is as it was; any whose scene is no longer open
    /// go to the end of the active scene.
    ///
    /// [`untrack`]: Self::untrack
    pub fn track(&mut self, objs: &[Tracked]) {
        let mut objs = objs.to_vec();
        objs.sort_by_key(|obj| obj.index);
        for obj in objs {
            let index = self.scenes.iter().position(|s| s.guid == obj.scene).or_else(|| self.active_index());
            if let Some(scene) = index.map(|i| &mut self.scenes[i]) {
                let at = obj.index.min(scene.objs.len());
                scene.objs.insert(at, (obj.path, obj.entity));
            }
        }
    }

    /// Keeps new Objs at the end of `scene`, or the active scene, as made
    /// from a scene file: each with its path from the file, whose first id
    /// is replaced by one after the highest in that scene, so prefab members
    /// stay under their instance. Returns where each is now kept.
    pub fn track_new(&mut self, scene: Option<AssetGuid>, objs: &[(ObjPath, Entity)]) -> Vec<Tracked> {
        let index =
            scene.and_then(|guid| self.scenes.iter().position(|s| s.guid == guid)).or_else(|| self.active_index());
        let Some(scene) = index.map(|i| &mut self.scenes[i]) else { return Vec::new() };
        let renumbered = renumber(&scene.objs, objs);
        let mut tracked = Vec::with_capacity(objs.len());
        for (path, entity) in renumbered {
            tracked.push(Tracked { scene: scene.guid, path: path.clone(), entity, index: scene.objs.len() });
            scene.objs.push((path, entity));
        }
        tracked
    }

    /// Moves Objs, kept together in the order given, to just before
    /// `before` in `scene`, or to its end. Those from another scene get new
    /// ids in this one. Returns where each is kept now.
    pub fn move_objs(&mut self, entities: &[Entity], scene: AssetGuid, before: Option<Entity>) -> Vec<Tracked> {
        let mut leaving = self.untrack(entities);
        leaving.sort_by_key(|t| entities.iter().position(|e| *e == t.entity));
        let Some(at) = self.scenes.iter().position(|s| s.guid == scene) else {
            self.track(&leaving);
            return leaving;
        };
        let (staying, moving): (Vec<Tracked>, Vec<Tracked>) = leaving.into_iter().partition(|t| t.scene == scene);
        let moving: Vec<(ObjPath, Entity)> = moving.into_iter().map(|t| (t.path, t.entity)).collect();
        let target = &mut self.scenes[at];
        let mut arriving: Vec<(ObjPath, Entity)> = staying.into_iter().map(|t| (t.path, t.entity)).collect();
        arriving.extend(renumber(&target.objs, &moving));
        arriving.sort_by_key(|(_, e)| entities.iter().position(|x| x == e));
        let first = before.and_then(|b| target.objs.iter().position(|(_, e)| *e == b)).unwrap_or(target.objs.len());
        let mut placed = Vec::with_capacity(arriving.len());
        for (place, (path, entity)) in (first..).zip(arriving) {
            placed.push(Tracked { scene, path: path.clone(), entity, index: place });
            target.objs.insert(place, (path, entity));
        }
        placed
    }

    fn active_index(&self) -> Option<usize> {
        self.scenes.iter().position(|s| Some(s.guid) == self.active).or(self.scenes.len().checked_sub(1))
    }
}

/// Whether an Obj is destroyed, being destroyed, or waiting to be as one of
/// `doomed` or under one.
pub fn is_doomed(world: &World, doomed: &[Entity], entity: Entity) -> bool {
    let mut at = Some(entity);
    while let Some(obj) = at {
        if !world.is_alive(obj) || world.get::<Destroying>(obj).is_some() || doomed.contains(&obj) {
            return true;
        }
        at = world.get::<Parent>(obj).map(|parent| parent.0);
    }
    false
}

/// Where an Obj is kept in the open level: its scene, its path there, and
/// its place in the scene's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracked {
    pub scene: AssetGuid,
    pub path: ObjPath,
    pub entity: Entity,
    pub index: usize,
}

/// New Objs' paths for a scene that has `existing`: each first id replaced
/// by one after the highest there, the same for every Obj that shared it, so
/// prefab members stay under their instance.
fn renumber(existing: &[(ObjPath, Entity)], objs: &[(ObjPath, Entity)]) -> Vec<(ObjPath, Entity)> {
    let mut next = existing.iter().filter_map(|(path, _)| path.ids().first().copied()).max().unwrap_or_default();
    let mut renumbered: HashMap<u32, u32> = HashMap::new();
    objs.iter()
        .map(|(path, entity)| {
            let first = path.ids().first().copied();
            let id = match first.map(|first| *renumbered.entry(first).or_insert_with(|| next + 1)) {
                Some(id) => id,
                None => next + 1,
            };
            next = next.max(id);
            let rest = first.and_then(|first| path.strip(&ObjPath::new(first))).unwrap_or_default();
            (ObjPath::new(id).join(&rest), *entity)
        })
        .collect()
}

impl Levels {
    /// Hands root Objs that no scene loaded, which scripts or the engine
    /// created at runtime, to the active scene.
    fn adopt_runtime_objs(&mut self, engine: &Engine) {
        let tracked: HashSet<Entity> = self
            .scenes
            .iter()
            .flat_map(|scene| scene.objs.iter().map(|(_, entity)| *entity))
            .chain(self.persistent.values().copied())
            .collect();
        let active = self.active;
        let level = |s: &LoadedScene| s.dimension.is_none();
        let index = self.scenes.iter().position(|s| Some(s.guid) == active && level(s));
        let Some(index) = index.or_else(|| self.scenes.iter().rposition(level)).or(self.scenes.len().checked_sub(1))
        else {
            return;
        };
        let world = &engine.world;
        let runtime: Vec<Entity> = world
            .query_ref::<(Entity, &Name)>()
            .without::<Parent>()
            .without::<Destroying>()
            .without::<EditorOnly>()
            .into_iter()
            .map(|(entity, _)| entity)
            .filter(|entity| !tracked.contains(entity))
            .collect();
        // Each goes into the scene of its dimension: the active one for the level's, a dimension's own file for
        // one opened as a scene, and none for a dimension made by code, as a dungeon's, which is not saved.
        for entity in runtime {
            let dimension = crate::dimensions::dimension_of(world, entity);
            let at = match dimension.is_level() {
                true => Some(index),
                false => self.scenes.iter().position(|s| s.dimension.as_ref().is_some_and(|d| d.id == dimension)),
            };
            if let Some(at) = at {
                self.scenes[at].objs.push((ObjPath::default(), entity));
            }
        }
    }
}

/// What a level loads, for listing it: its scenes, those of the levels it
/// stands on first, its active scene and the level it stands on, by path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LevelSummary {
    pub scenes: Vec<String>,
    pub active: Option<String>,
    pub stands_on: Option<String>,
}

/// Reads what a level loads without loading it.
pub fn summarise(content: &mut Content, level: &str) -> Result<LevelSummary> {
    let (path, targets, active) = read_level(content, level)?;
    let guid = content.db().guid_of(&path).context("the level is in the asset database")?;
    let (text, _) = content.asset_text(guid, AssetKind::Level)?;
    let file = parse_level(&text, &path)?;
    let stands_on = file
        .stands_on
        .as_deref()
        .and_then(|base| content.parse(base).ok()?.guid())
        .and_then(|base| content.db().get(base).map(|e| e.path.clone()));
    let active = active.and_then(|guid| targets.iter().find(|t| t.guid == guid)).map(|t| t.path.clone());
    Ok(LevelSummary { scenes: targets.into_iter().map(|t| t.path).collect(), active, stands_on })
}

/// A level's file, parsed. The content build reads levels too.
pub(crate) fn parse_level(text: &str, path: &str) -> Result<LevelFile> {
    let file: LevelFile = toml::from_str(text).with_context(|| format!("reading {path}"))?;
    ensure!(file.format == LEVEL_FORMAT, "{path} uses level format {}, not {LEVEL_FORMAT}", file.format);
    Ok(file)
}

/// A level's own ray-traced audio settings, from its `[audio]`: none set
/// where it has none, or will not read.
pub(crate) fn level_audio(content: &mut Content, level: &str) -> crate::sound_tracing::LevelAudio {
    let mut read = || -> Result<Option<crate::sound_tracing::LevelAudio>> {
        let guid = content.parse(level)?.guid().with_context(|| format!("`{level}` is not a level asset"))?;
        let (text, _) = content.asset_text(guid, AssetKind::Level)?;
        Ok(parse_level(&text, level)?.audio)
    };
    read().ok().flatten().unwrap_or_default()
}

/// A level's path, every scene it wants (those of the levels it stands on
/// first, each once), and its active scene.
pub(crate) fn read_level(content: &mut Content, level: &str) -> Result<(String, Vec<Target>, Option<AssetGuid>)> {
    let mut chain = Vec::new();
    let mut targets = Vec::new();
    let (path, active) = read_chain(content, level, &mut chain, &mut targets)?;
    Ok((path, targets, active))
}

/// Reads a level and, first, the levels it stands on, adding their scenes
/// to `targets`. Returns the level's path and its active scene.
fn read_chain(
    content: &mut Content,
    level: &str,
    chain: &mut Vec<AssetGuid>,
    targets: &mut Vec<Target>,
) -> Result<(String, Option<AssetGuid>)> {
    let reference = content.parse(level)?;
    let guid = reference.guid().with_context(|| format!("`{level}` is not a level asset"))?;
    let entry = content.db().get(guid).with_context(|| format!("no asset with GUID {guid}"))?.clone();
    if entry.kind != AssetKind::Level {
        bail!("{} is not a .level file", entry.path);
    }
    if chain.contains(&guid) {
        bail!("{} stands on itself, through the levels it stands on", entry.path);
    }
    chain.push(guid);
    let (text, _) = content.asset_text(guid, AssetKind::Level)?;
    let file = parse_level(&text, &entry.path)?;
    let base_active = match &file.stands_on {
        Some(base) => {
            let (_, active) = read_chain(content, base, chain, targets)
                .with_context(|| format!("{} stands on {base}", entry.path))?;
            active
        }
        None => None,
    };

    let mut own = Vec::new();
    let mut marked = None;
    for scene in &file.scenes {
        let found = find_scene(content, &scene.scene).with_context(|| format!("{} lists {}", entry.path, scene.scene));
        let (scene_guid, scene_path) = match found {
            Ok(found) => found,
            Err(error) if !scene.required => {
                tracing::warn!("passed over a scene that is not required: {error:#}");
                continue;
            }
            Err(error) => return Err(error),
        };
        if scene.active {
            marked = Some(scene_guid);
        }
        own.push(scene_guid);
        if targets.iter().any(|t| t.guid == scene_guid) {
            continue;
        }
        targets.push(Target { guid: scene_guid, path: scene_path, reload: scene.reload, required: scene.required });
    }
    // A level's own scene is the one to be in, not a base scene it sits on.
    Ok((entry.path.clone(), marked.or(own.first().copied()).or(base_active)))
}

/// A scene by GUID or path, with its path.
fn find_scene(content: &Content, reference: &str) -> Result<(AssetGuid, String)> {
    let guid = content.parse(reference)?.guid().with_context(|| format!("`{reference}` is not a scene"))?;
    let entry = content.db().get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
    if !matches!(entry.kind, AssetKind::Scene | AssetKind::Prefab | AssetKind::Particles) {
        bail!("{} is not a .scene, .prefab or .particles file", entry.path);
    }
    Ok((guid, entry.path.clone()))
}

/// Destroys a scene's Objs, except persistent ones and their children, which
/// are detached from any doomed parent and kept. Returns the kept Objs.
/// Destroys a scene's Objs, but for persistent ones while `keep_persistent`,
/// which are returned.
fn unload(engine: &mut Engine, scene: &LoadedScene, keep_persistent: bool) -> Vec<(ObjPath, Entity)> {
    let world = &engine.world;
    let alive: Vec<Entity> = scene.objs.iter().map(|(_, e)| *e).filter(|e| world.is_alive(*e)).collect();
    let persistent = |entity: Entity| {
        let mut current = Some(entity);
        let mut depth = 0;
        while let Some(e) = current {
            if world.has::<ScenePersistence>(e) {
                return true;
            }
            current = world.get::<Parent>(e).map(|p| p.0);
            depth += 1;
            if depth > 256 {
                break;
            }
        }
        false
    };
    let kept: HashSet<Entity> = alive.iter().copied().filter(|e| keep_persistent && persistent(*e)).collect();
    let detach: Vec<Entity> =
        kept.iter().copied().filter(|e| world.get::<Parent>(*e).is_some_and(|p| !kept.contains(&p.0))).collect();
    for entity in detach {
        engine.world.remove::<Parent>(entity);
    }
    let mut doomed: Vec<Entity> = alive.into_iter().filter(|e| !kept.contains(e)).collect();
    // A dimension's file closing lets its dimension go, and whatever else is in it.
    if let Some(open) = &scene.dimension {
        for &entity in &kept {
            if !engine.world.has::<Parent>(entity) {
                crate::dimensions::set_dimension(&mut engine.world, entity, crate::dimensions::DimensionId::LEVEL);
            }
        }
        let rest = crate::dimensions::let_go(&mut engine.world, open.id).unwrap_or_default();
        let rest: Vec<Entity> = rest.into_iter().filter(|entity| !doomed.contains(entity)).collect();
        doomed.extend(rest);
    }
    engine.destroy_objs(&doomed);
    scene.objs.iter().filter(|(_, e)| kept.contains(e)).cloned().collect()
}

/// A dimension's file as it would be saved with `captured` as its scene,
/// and as it is on disk, each written the same way so the two compare.
fn dimension_texts(
    file: &Path,
    settings: &crate::dimensions::DimensionSettings,
    captured: scene_file::SceneFile,
) -> (Result<String>, Option<String>) {
    let now = crate::dimensions::DimensionFile { settings: settings.clone(), scene: captured }.to_toml();
    let on_disk = std::fs::read_to_string(file).ok();
    let on_disk = on_disk.and_then(|text| crate::dimensions::DimensionFile::parse(&text, "").ok()?.to_toml().ok());
    (now, on_disk)
}
