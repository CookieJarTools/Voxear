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

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::{Destroying, EditorOnly, Name, Parent, ScenePersistence};

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
}

impl Default for LevelOptions {
    fn default() -> Self {
        Self { hold: false, least_time: 0.0, unload_rest: true, set_active: true, tidy_up: true }
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
    /// Its sky and post-processing, for when it is the active scene.
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
        let (level_path, targets, active) = read_level(content, level)?;
        self.change(engine, content, renderer, (level_path, targets, active), options)
    }

    /// Opens one scene on its own, as the editor does for a scene that is in
    /// no level: everything else unloads and it is the active scene.
    pub fn open_scene(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        renderer: Option<&mut Renderer>,
        scene: &str,
    ) -> Result<LevelChange> {
        let (guid, path) = find_scene(content, scene)?;
        let target = Target { guid, path: path.clone(), reload: false, required: true };
        self.change(engine, content, renderer, (path, vec![target], Some(guid)), &LevelOptions::default())
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
            let wanted = targets.iter().any(|t| t.guid == scene.guid && !t.reload);
            let unwanted = !targets.iter().any(|t| t.guid == scene.guid);
            if wanted || (unwanted && !options.unload_rest) {
                change.kept.push(scene.path.clone());
                kept.push(scene);
            } else {
                for (id, entity) in unload(engine, &scene) {
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
        let (text, path) = content.asset_text(target.guid, AssetKind::Scene)?;
        let scene = scene_file::parse(&text, &path)?;
        self.persistent.retain(|_, entity| engine.world.is_alive(*entity));
        let survivors: HashSet<ObjPath> =
            self.persistent.keys().filter(|(guid, _)| *guid == target.guid).map(|(_, path)| path.clone()).collect();
        let objs = scene_file::spawn(engine, content, renderer, &scene, &survivors)
            .with_context(|| format!("loading {}", target.path))?;
        Ok(LoadedScene { guid: target.guid, path: target.path.clone(), objs, sky: scene.sky, post: scene.post })
    }

    /// The sky and post-processing come from the active scene, or failing
    /// that, the last one with any.
    fn apply_environment(&self, engine: &mut Engine) -> Result<()> {
        let active = self.scenes.iter().find(|s| Some(s.guid) == self.active);
        let sky = active.and_then(|s| s.sky).or_else(|| self.scenes.iter().rev().find_map(|s| s.sky));
        let post =
            active.and_then(|s| s.post.as_ref()).or_else(|| self.scenes.iter().rev().find_map(|s| s.post.as_ref()));
        scene_file::apply_environment(engine, sky, post)
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
            // The world's sky is the level's; each scene keeps its own.
            (captured.sky, captured.post) = (scene.sky, scene.post.clone());
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
        let mut copy = Levels { current: self.current.clone(), active: self.active, ..Levels::default() };
        for scene in &self.scenes {
            let captured = scene_file::capture_scene(&edit.world, content, &scene.objs);
            let objs = scene_file::spawn(play, content, renderer.as_deref_mut(), &captured, &HashSet::new())
                .with_context(|| format!("starting {}", scene.path))?;
            let (sky, post) = (scene.sky, scene.post.clone());
            copy.scenes.push(LoadedScene { guid: scene.guid, path: scene.path.clone(), objs, sky, post });
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

    /// The scene an Obj is kept in.
    pub fn scene_of(&self, entity: Entity) -> Option<AssetGuid> {
        self.scenes.iter().find(|scene| scene.objs.iter().any(|(_, e)| *e == entity)).map(|scene| scene.guid)
    }

    /// Where Objs are kept, in the order their scenes keep them.
    pub fn tracked(&self, entities: &[Entity]) -> Vec<Tracked> {
        let kept = self.scenes.iter().flat_map(|scene| scene.objs.iter().map(move |(path, e)| (scene.guid, path, *e)));
        kept.filter(|(_, _, entity)| entities.contains(entity))
            .map(|(scene, path, entity)| Tracked { scene, path: path.clone(), entity })
            .collect()
    }

    /// Stops keeping Objs in their scenes, as when the editor deletes them
    /// or moves them to another scene, and returns where each was, to put
    /// back with [`track`](Self::track).
    pub fn untrack(&mut self, entities: &[Entity]) -> Vec<Tracked> {
        let mut found = Vec::new();
        for scene in &mut self.scenes {
            let guid = scene.guid;
            scene.objs.retain(|(path, entity)| {
                let leaving = entities.contains(entity);
                if leaving {
                    found.push(Tracked { scene: guid, path: path.clone(), entity: *entity });
                }
                !leaving
            });
        }
        found
    }

    /// Keeps Objs in scenes again, where [`untrack`](Self::untrack) found
    /// them; any whose scene is no longer open go to the active scene.
    pub fn track(&mut self, objs: &[Tracked]) {
        for obj in objs {
            let index = self.scenes.iter().position(|s| s.guid == obj.scene).or_else(|| self.active_index());
            if let Some(scene) = index.map(|i| &mut self.scenes[i]) {
                scene.objs.push((obj.path.clone(), obj.entity));
            }
        }
    }

    /// Keeps new Objs in `scene`, or the active scene, as made from a scene
    /// file: each with its path from the file, whose first id is replaced by
    /// one after the highest in that scene, so prefab members stay under
    /// their instance. Returns where each is now kept.
    pub fn track_new(&mut self, scene: Option<AssetGuid>, objs: &[(ObjPath, Entity)]) -> Vec<Tracked> {
        let index =
            scene.and_then(|guid| self.scenes.iter().position(|s| s.guid == guid)).or_else(|| self.active_index());
        let Some(scene) = index.map(|i| &mut self.scenes[i]) else { return Vec::new() };
        let mut next = scene.objs.iter().filter_map(|(path, _)| path.ids().first().copied()).max().unwrap_or_default();
        let mut renumbered: HashMap<u32, u32> = HashMap::new();
        let mut tracked = Vec::with_capacity(objs.len());
        for (path, entity) in objs {
            let first = path.ids().first().copied();
            let id = match first.map(|first| *renumbered.entry(first).or_insert_with(|| next + 1)) {
                Some(id) => id,
                None => next + 1,
            };
            next = next.max(id);
            let rest = first.and_then(|first| path.strip(&ObjPath::new(first))).unwrap_or_default();
            let path = ObjPath::new(id).join(&rest);
            scene.objs.push((path.clone(), *entity));
            tracked.push(Tracked { scene: scene.guid, path, entity: *entity });
        }
        tracked
    }

    fn active_index(&self) -> Option<usize> {
        self.scenes.iter().position(|s| Some(s.guid) == self.active).or(self.scenes.len().checked_sub(1))
    }
}

/// Where an Obj is kept in the open level: its scene, and its path there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracked {
    pub scene: AssetGuid,
    pub path: ObjPath,
    pub entity: Entity,
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
    if entry.kind != AssetKind::Scene {
        bail!("{} is not a .scene file", entry.path);
    }
    Ok((guid, entry.path.clone()))
}

/// Destroys a scene's Objs, except persistent ones and their children, which
/// are detached from any doomed parent and kept. Returns the kept Objs.
fn unload(engine: &mut Engine, scene: &LoadedScene) -> Vec<(ObjPath, Entity)> {
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
    let kept: HashSet<Entity> = alive.iter().copied().filter(|e| persistent(*e)).collect();
    let detach: Vec<Entity> =
        kept.iter().copied().filter(|e| world.get::<Parent>(*e).is_some_and(|p| !kept.contains(&p.0))).collect();
    for entity in detach {
        engine.world.remove::<Parent>(entity);
    }
    let doomed: Vec<Entity> = alive.into_iter().filter(|e| !kept.contains(e)).collect();
    engine.destroy_objs(&doomed);
    scene.objs.iter().filter(|(_, e)| kept.contains(e)).cloned().collect()
}
