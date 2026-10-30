//! Levels. A level is a list of scenes:
//!
//! ```toml
//! format = 1
//!
//! [[scene]]
//! scene = "Scenes/Environment.scene"
//!
//! [[scene]]
//! scene = "Scenes/Night.scene"
//! reload = true      # reload even when the previous level had it too
//! ```
//!
//! Loading a level keeps the scenes it shares with the current one, unloads
//! the rest and loads what is new. Objs marked `persistent` survive every
//! change (Unity's `DontDestroyOnLoad`), along with their children. Unlike
//! Unity, reloading the scene a persistent Obj came from does not create a
//! second copy of it.
//!
//! Objs created at runtime, such as instantiated prefabs, belong to the
//! level's last scene (Unity's active scene) and unload with it.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::Entity;
use voxear_render::Renderer;
use voxear_scene::{Destroying, Name, Parent, ScenePersistence};

use crate::{content::Content, engine::Engine, prefab::ObjPath, scene_file};

pub const LEVEL_FORMAT: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelFile {
    pub format: u32,
    #[serde(default, rename = "scene")]
    pub scenes: Vec<LevelScene>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelScene {
    /// A scene GUID or project path.
    pub scene: String,
    #[serde(default)]
    pub reload: bool,
}

/// Which scenes a level change kept, loaded and unloaded, by path.
#[derive(Debug, Default)]
pub struct LevelChange {
    pub kept: Vec<String>,
    pub loaded: Vec<String>,
    pub unloaded: Vec<String>,
}

struct LoadedScene {
    guid: AssetGuid,
    path: String,
    /// Each Obj's path in the scene file, and its entity.
    objs: Vec<(ObjPath, Entity)>,
}

struct Target {
    guid: AssetGuid,
    path: String,
    reload: bool,
}

#[derive(Default)]
pub struct Levels {
    current: Option<String>,
    scenes: Vec<LoadedScene>,
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

    pub fn load(
        &mut self,
        engine: &mut Engine,
        content: &mut Content,
        mut renderer: Option<&mut Renderer>,
        level: &str,
    ) -> Result<LevelChange> {
        let (level_path, targets) = read_level(content, level)?;
        let mut change = LevelChange::default();
        self.adopt_runtime_objs(engine);

        let mut kept = Vec::new();
        for scene in std::mem::take(&mut self.scenes) {
            let keep = targets.iter().any(|t| t.guid == scene.guid && !t.reload);
            if keep {
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
            let (text, path) = content.asset_text(target.guid, AssetKind::Scene)?;
            let scene = scene_file::parse(&text, &path)?;
            self.persistent.retain(|_, entity| engine.world.is_alive(*entity));
            let survivors: HashSet<ObjPath> =
                self.persistent.keys().filter(|(guid, _)| *guid == target.guid).map(|(_, path)| path.clone()).collect();
            let objs = scene_file::spawn(engine, content, renderer.as_deref_mut(), &scene, &survivors)
                .with_context(|| format!("loading {}", target.path))?;
            kept.push(LoadedScene { guid: target.guid, path: target.path.clone(), objs });
            change.loaded.push(target.path.clone());
        }

        kept.sort_by_key(|scene| targets.iter().position(|t| t.guid == scene.guid));
        self.scenes = kept;
        self.current = Some(level_path);
        Ok(change)
    }
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
        let Some(active) = self.scenes.last_mut() else { return };
        let runtime = engine
            .world
            .query_ref::<(Entity, &Name)>()
            .without::<Parent>()
            .without::<Destroying>()
            .into_iter()
            .map(|(entity, _)| entity)
            .filter(|entity| !tracked.contains(entity));
        active.objs.extend(runtime.map(|entity| (ObjPath::default(), entity)));
    }
}

/// A level's file, parsed. The content build reads levels too.
pub(crate) fn parse_level(text: &str, path: &str) -> Result<LevelFile> {
    let file: LevelFile = toml::from_str(text).with_context(|| format!("reading {path}"))?;
    ensure!(file.format == LEVEL_FORMAT, "{path} uses level format {}, not {LEVEL_FORMAT}", file.format);
    Ok(file)
}

fn read_level(content: &mut Content, level: &str) -> Result<(String, Vec<Target>)> {
    let reference = content.parse(level)?;
    let guid = reference.guid().with_context(|| format!("`{level}` is not a level asset"))?;
    let entry = content.db().get(guid).with_context(|| format!("no asset with GUID {guid}"))?.clone();
    if entry.kind != AssetKind::Level {
        bail!("{} is not a .level file", entry.path);
    }
    let (text, _) = content.asset_text(guid, AssetKind::Level)?;
    let file = parse_level(&text, &entry.path)?;

    let mut targets = Vec::with_capacity(file.scenes.len());
    for scene in &file.scenes {
        let guid = content.parse(&scene.scene)?.guid().with_context(|| format!("`{}` is not a scene", scene.scene))?;
        let scene_entry = content.db().get(guid).with_context(|| format!("no asset with GUID {guid}"))?;
        if scene_entry.kind != AssetKind::Scene {
            bail!("{} lists {}, which is not a .scene file", entry.path, scene_entry.path);
        }
        targets.push(Target { guid, path: scene_entry.path.clone(), reload: scene.reload });
    }
    Ok((entry.path.clone(), targets))
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
