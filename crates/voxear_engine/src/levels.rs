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
        ensure!(entry.kind == AssetKind::Scene, "{} is not a .scene file", entry.path);
        let target = Target { guid: scene, path: entry.path.clone(), reload: false, required: true };
        let loaded = self.load_scene(engine, content, renderer, &target)?;
        self.scenes.push(loaded);
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
        let prefab = content.db().get(target.guid).is_some_and(|entry| entry.kind == AssetKind::Prefab);
        let kind = if prefab { AssetKind::Prefab } else { AssetKind::Scene };
        let (text, path) = content.asset_text(target.guid, kind)?;
        let scene = scene_file::parse(&text, &path)?;
        self.persistent.retain(|_, entity| engine.world.is_alive(*entity));
        let survivors: HashSet<ObjPath> =
            self.persistent.keys().filter(|(guid, _)| *guid == target.guid).map(|(_, path)| path.clone()).collect();
        let objs = scene_file::spawn(engine, content, renderer, &scene, &survivors)
            .with_context(|| format!("loading {}", target.path))?;
        let lighting = match scene.lighting {
            Some(reference) => {
                let lit = content.lighting(&content.parse(&reference)?).with_context(|| format!("loading {reference}"))?;
                Some((reference, lit))
            }
            None => None,
        };
        let (sky, post) = (scene.sky, scene.post);
        Ok(LoadedScene { guid: target.guid, path: target.path.clone(), objs, lighting, sky, post })
    }

    /// The sky and post-processing come from the active scene, or failing
    /// that, the last one with any; lighting a script applied replaces them.
    fn apply_environment(&self, engine: &mut Engine) -> Result<()> {
        if let Some((reference, lit)) = &self.applied_lighting {
            let lighting = Some((reference.as_str(), lit));
            return scene_file::apply_environment(engine, scene_file::Environment { lighting, sky: None, post: None });
        }
        let active = self.scenes.iter().find(|s| Some(s.guid) == self.active);
        let sky = active.and_then(|s| s.sky).or_else(|| self.scenes.iter().rev().find_map(|s| s.sky));
        let post =
            active.and_then(|s| s.post.as_ref()).or_else(|| self.scenes.iter().rev().find_map(|s| s.post.as_ref()));
        let lighting = active
            .and_then(|s| s.lighting.as_ref())
            .or_else(|| self.scenes.iter().rev().find_map(|s| s.lighting.as_ref()))
            .map(|(reference, lit)| (reference.as_str(), lit));
        scene_file::apply_environment(engine, scene_file::Environment { lighting, sky, post })
    }

    /// The lighting settings the active scene names, as it names them.
    pub fn scene_lighting(&self) -> Option<&str> {
        let active = self.scenes.iter().find(|s| Some(s.guid) == self.active)?;
        active.lighting.as_ref().map(|(reference, _)| reference.as_str())
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
            // The world's sky and lighting are the level's; each scene keeps its own.
            (captured.sky, captured.post) = (scene.sky, scene.post.clone());
            captured.lighting = scene.lighting.as_ref().map(|(reference, _)| reference.clone());
            let file = project.join(&scene.path);
            let on_disk = scene_file::read(&file).ok().and_then(|s| toml::to_string(&s).ok());
            if on_disk.is_some() && on_disk == toml::to_string(&captured).ok() {
                continue;
            }
            scene_file::write(&file, &captured).with_context(|| format!("saving {}", scene.path))?;
            // Objs that had no id have one now, which the next save keeps.
            for (path, entity) in &mut scene.objs {
                if path.ids().is_empty()
                    && let Some(&id) = ids.get(entity)
                {
                    *path = ObjPath::new(id);
                }
            }
            written.push(scene.path.clone());
        }
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
            captured.lighting = scene.lighting.as_ref().map(|(reference, _)| reference.clone());
            let objs = scene_file::spawn(play, content, renderer.as_deref_mut(), &captured, &HashSet::new())
                .with_context(|| format!("starting {}", scene.path))?;
            let (lighting, sky, post) = (scene.lighting.clone(), scene.sky, scene.post.clone());
            copy.scenes.push(LoadedScene { guid: scene.guid, path: scene.path.clone(), objs, lighting, sky, post });
        }
        copy.apply_environment(play)?;
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
        let index = self.scenes.iter().position(|s| Some(s.guid) == active).or(self.scenes.len().checked_sub(1));
        let Some(active) = index.map(|i| &mut self.scenes[i]) else { return };
        let runtime = engine
            .world
            .query_ref::<(Entity, &Name)>()
            .without::<Parent>()
            .without::<Destroying>()
            .without::<EditorOnly>()
            .into_iter()
            .map(|(entity, _)| entity)
            .filter(|entity| !tracked.contains(entity));
        active.objs.extend(runtime.map(|entity| (ObjPath::default(), entity)));
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
    if !matches!(entry.kind, AssetKind::Scene | AssetKind::Prefab) {
        bail!("{} is not a .scene or .prefab file", entry.path);
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
    let doomed: Vec<Entity> = alive.into_iter().filter(|e| !kept.contains(e)).collect();
    engine.destroy_objs(&doomed);
    scene.objs.iter().filter(|(_, e)| kept.contains(e)).cloned().collect()
}
