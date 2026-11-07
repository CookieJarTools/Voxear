//! Making, duplicating, deleting and reparenting Objs, as the Hierarchy
//! does, and placing prefabs, models and sounds dragged from the Project.
//! Objs are copied and brought back through scene files of them
//! (`scene_file::capture_scene` and `spawn`), so everything a scene holds
//! comes back, prefab instances included. They keep their place in the
//! level's scenes (`Levels::untrack` and `track`), so a save writes them
//! where they were, with the ids they had. A scene's order is its Objs'
//! order in the Hierarchy, so Objs moved or brought back keep their places
//! in it too.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, ensure};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game, ObjPath, Tracked,
    modules::Module,
    prefab::{PREFAB_FORMAT, PrefabFile},
    scene_file::{self, ModelSource, PartData, SceneFile},
};
use voxear_render::{LodGroup, Renderer};
use voxear_scene::{Destroying, Name, Parent, Transform};

use crate::project_view::AssetPart;

/// What the Hierarchy's + makes, as Unity's GameObject menu: what the menu
/// says, the new Obj's name, the rest of its scene file entry, and the
/// module it needs, without which the menus leave it out.
pub const NEW_OBJS: [(&str, &str, &str, Option<Module>); 18] = [
    ("Empty", "Obj", "", None),
    ("Cube", "Cube", "[obj.renderer]\nmesh = \"builtin:cube\"", None),
    ("Sphere", "Sphere", "[obj.renderer]\nmesh = \"builtin:sphere\"", None),
    ("Plane", "Plane", "[obj.renderer]\nmesh = \"builtin:plane\"", None),
    (
        "Directional Light",
        "Directional Light",
        "rotation = [50.0, -30.0, 0.0]\n[obj.light]\ntype = \"directional\"",
        None,
    ),
    ("Point Light", "Point Light", "[obj.light]\ntype = \"point\"", None),
    ("Spot Light", "Spot Light", "rotation = [90.0, 0.0, 0.0]\n[obj.light]\ntype = \"spot\"", None),
    ("Rectangle Light", "Rectangle Light", "rotation = [90.0, 0.0, 0.0]\n[obj.light]\ntype = \"rectangle\"", None),
    ("Disc Light", "Disc Light", "rotation = [90.0, 0.0, 0.0]\n[obj.light]\ntype = \"disc\"", None),
    ("Tube Light", "Tube Light", "[obj.light]\ntype = \"tube\"", None),
    ("Camera", "Camera", "[obj.camera]", None),
    ("Post Volume", "Post Volume", "scale = [10.0, 4.0, 10.0]\n[obj.post_volume]", None),
    ("Audio Source", "Audio Source", "[obj.audio]", None),
    ("Particle System", "Particle System", "[obj.particles]", None),
    ("Smoke", "Smoke", "[obj.smoke]", None),
    ("Mist", "Mist", "[obj.mist]", None),
    ("Car", "Car", CAR, None),
    ("Rifle", "Rifle", RIFLE, Some(Module::Guns)),
];

/// A car ready to drive: a body that physics moves, with a Vehicle, and
/// four wheels under it, the front two steering and the back two driven,
/// each with a flattened sphere for a tyre.
const CAR: &str = r#"[obj.collider]
size = [1.8, 0.6, 4.0]
[obj.rigidbody]
mass = 1200.0
[obj.vehicle]

[[obj]]
id = 2
name = "Body"
parent = 1
scale = [1.8, 0.6, 4.0]
[obj.renderer]
mesh = "builtin:cube"

[[obj]]
id = 3
name = "Front Left Wheel"
parent = 1
position = [-0.9, -0.4, 1.4]
[obj.wheel]
steer_angle = 35.0

[[obj]]
id = 4
name = "Front Right Wheel"
parent = 1
position = [0.9, -0.4, 1.4]
[obj.wheel]
steer_angle = 35.0

[[obj]]
id = 5
name = "Back Left Wheel"
parent = 1
position = [-0.9, -0.4, -1.4]
[obj.wheel]
powered = true
handbrake = 4000.0

[[obj]]
id = 6
name = "Back Right Wheel"
parent = 1
position = [0.9, -0.4, -1.4]
[obj.wheel]
powered = true
handbrake = 4000.0

[[obj]]
id = 7
name = "Tyre"
parent = 3
scale = [0.25, 0.7, 0.7]
[obj.renderer]
mesh = "builtin:sphere"

[[obj]]
id = 8
name = "Tyre"
parent = 4
scale = [0.25, 0.7, 0.7]
[obj.renderer]
mesh = "builtin:sphere"

[[obj]]
id = 9
name = "Tyre"
parent = 5
scale = [0.25, 0.7, 0.7]
[obj.renderer]
mesh = "builtin:sphere"

[[obj]]
id = 10
name = "Tyre"
parent = 6
scale = [0.25, 0.7, 0.7]
[obj.renderer]
mesh = "builtin:sphere"
"#;

/// A bolt-action rifle to make other guns from: a Gun with the preset's
/// sockets, and blocks where its parts go, each of which can have its look
/// swapped for a model's. The bolt turns up and slides back, a stripper
/// clip shows while a hand has it, and spent cases are copied from the
/// case.
const RIFLE: &str = r#"[obj.gun]
kind = "bolt_action_rifle"

[[obj]]
id = 2
name = "Stock"
parent = 1
position = [0.0, -0.03, -0.19]
scale = [0.045, 0.12, 0.34]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.36, 0.22, 0.12], roughness = 0.6 }

[[obj]]
id = 3
name = "Fore-end"
parent = 1
position = [0.0, -0.005, 0.32]
scale = [0.045, 0.06, 0.72]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.36, 0.22, 0.12], roughness = 0.6 }

[[obj]]
id = 4
name = "Receiver"
parent = 1
position = [0.0, 0.04, 0.08]
scale = [0.034, 0.034, 0.22]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.12, 0.12, 0.13], metallic = 1.0, roughness = 0.45 }

[[obj]]
id = 5
name = "Barrel"
parent = 1
position = [0.0, 0.045, 0.585]
scale = [0.018, 0.018, 0.81]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.12, 0.12, 0.13], metallic = 1.0, roughness = 0.45 }

[[obj]]
id = 6
name = "Bolt"
parent = 1
position = [0.045, 0.05, 0.1]
scale = [0.06, 0.015, 0.015]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.12, 0.12, 0.13], metallic = 1.0, roughness = 0.45 }
[obj.gun_part]
role = "bolt"
handle = [0.075, 0.05, 0.1]
motion = { type = "turn", axis = [0.0, 0.0, 1.0], pivot = [0.0, 0.05, 0.0], angle = 80.0 }
then = { type = "slide", axis = [0.0, 0.0, -1.0], travel = 0.1 }

[[obj]]
id = 7
name = "Rear Sight"
parent = 1
position = [0.0, 0.065, 0.2]
scale = [0.03, 0.015, 0.01]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.12, 0.12, 0.13], roughness = 0.6 }

[[obj]]
id = 8
name = "Front Sight"
parent = 1
position = [0.0, 0.07, 0.96]
scale = [0.006, 0.03, 0.006]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.12, 0.12, 0.13], roughness = 0.6 }

[[obj]]
id = 9
name = "Clip"
parent = 1
position = [0.0, 0.1, 0.12]
scale = [0.012, 0.05, 0.06]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.7, 0.6, 0.3], metallic = 1.0, roughness = 0.4 }
[obj.gun_part]
role = "clip"
handle = [0.0, 0.13, 0.12]
motion = { type = "slide", axis = [0.0, -1.0, 0.0], travel = 0.25 }

[[obj]]
id = 10
name = "Case"
parent = 1
position = [0.03, 0.05, 0.12]
scale = [0.011, 0.011, 0.05]
[obj.renderer]
mesh = "builtin:cube"
material = { color = [0.85, 0.65, 0.3], metallic = 1.0, roughness = 0.35 }
[obj.gun_part]
role = "case"
handle = [0.03, 0.05, 0.12]
"#;

/// Objs taken out of the world, ready to bring back exactly: the top one,
/// each one's parent, and a scene file of them for each scene they were in.
#[derive(Debug)]
pub struct Taken {
    root: Entity,
    parents: Vec<(Entity, Entity)>,
    parts: Vec<Part>,
}

/// The taken Objs one scene kept (`None` for any no scene kept): each with
/// its path and its place there, and all of them as a scene file.
#[derive(Debug)]
struct Part {
    scene: Option<AssetGuid>,
    paths: Vec<(ObjPath, Entity, usize)>,
    file: SceneFile,
}

impl Taken {
    /// Follows Objs that undo or redo brought back as new entities.
    pub fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        let follow = |entity: &mut Entity| *entity = renamed.get(entity).copied().unwrap_or(*entity);
        follow(&mut self.root);
        for (child, parent) in &mut self.parents {
            follow(child);
            follow(parent);
        }
        for part in &mut self.parts {
            part.paths.iter_mut().for_each(|(_, entity, _)| follow(entity));
        }
    }
}

/// Where an Obj sits: its parent, its Transform there, and where it and
/// everything under it are kept in the level's scenes.
#[derive(Clone, Debug, PartialEq)]
pub struct Placing {
    parent: Option<Entity>,
    transform: Transform,
    tracked: Vec<Tracked>,
}

impl Placing {
    fn of(game: &Game, entity: Entity) -> Option<Self> {
        let world = &game.engine.world;
        let transform = *world.get::<Transform>(entity)?;
        let tracked = game.levels.tracked(&family(world, entity));
        Some(Self { parent: world.get::<Parent>(entity).map(|p| p.0), transform, tracked })
    }

    /// Puts the Obj back here.
    pub fn apply(&self, game: &mut Game, entity: Entity) {
        let world = &mut game.engine.world;
        match self.parent {
            Some(parent) => {
                world.insert(entity, Parent(parent));
            }
            None => {
                world.remove::<Parent>(entity);
            }
        }
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            *transform = self.transform;
        }
        // Only when it changed scene, so a scene's Objs keep their order.
        let family = family(world, entity);
        if game.levels.tracked(&family) != self.tracked {
            game.levels.untrack(&family);
            game.levels.track(&self.tracked);
        }
    }

    pub fn remap(&mut self, renamed: &HashMap<Entity, Entity>) {
        let follow = |entity: &mut Entity| *entity = renamed.get(entity).copied().unwrap_or(*entity);
        if let Some(parent) = &mut self.parent {
            follow(parent);
        }
        self.tracked.iter_mut().for_each(|t| follow(&mut t.entity));
    }
}

/// The Obj and every Obj under it, parents first.
pub fn family(world: &World, root: Entity) -> Vec<Entity> {
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

/// Makes one of [`NEW_OBJS`]: under `parent` at its middle, or at the top
/// at `position`, last in the parent's scene, or `scene`, or the active one.
/// Returns it.
pub fn create(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    kind: usize,
    parent: Option<Entity>,
    scene: Option<AssetGuid>,
    position: Vec3,
) -> Result<Entity> {
    let (_, name, rest, _) = NEW_OBJS[kind.min(NEW_OBJS.len() - 1)];
    let mut file = scene_file::parse(&format!("format = 1\n\n[[obj]]\nid = 1\nname = \"{name}\"\n{rest}\n"), name)?;
    if parent.is_none() {
        file.objs[0].position = Some(position.to_array());
    }
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, renderer, &file, &HashSet::new())?;
    game.levels.track_new(parent.and_then(|p| game.levels.scene_of(p)).or(scene), &spawned);
    let made = spawned.first().map(|(_, entity)| *entity).context("the new Obj was made")?;
    if let Some(parent) = parent {
        game.engine.world.insert(made, Parent(parent));
    }
    Ok(made)
}

/// Places an instance of the prefab at `path` (from the project's folder):
/// under `parent` at its middle, or at the top at `position`, last in the
/// parent's scene, or `scene`, or the active one. At the top, the prefab's
/// root keeps its own offset from `position`, so a lamp made to stand on the
/// ground at the origin stands on it here. It is named for the prefab, and
/// numbered as a copy is when an Obj has that name already. Returns it.
pub fn place_prefab(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    path: &str,
    parent: Option<Entity>,
    scene: Option<AssetGuid>,
    position: Vec3,
) -> Result<Entity> {
    let reference = toml::Value::String(path.to_string());
    let file = scene_file::parse(&format!("format = 1\n\n[[obj]]\nid = 1\nprefab = {reference}\n"), path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, renderer, &file, &HashSet::new())?;
    game.levels.track_new(parent.and_then(|p| game.levels.scene_of(p)).or(scene), &spawned);
    let made = spawned.iter().find(|(at, _)| *at == ObjPath::new(1)).map(|(_, e)| *e);
    let made = made.context("the instance was made")?;
    settle(&mut game.engine.world, made, parent, position);
    Ok(made)
}

/// Places the model at `path` (from the project's folder) as one Obj named
/// for it, where [`place_prefab`] places an instance and offset as it is:
/// with a Mesh Renderer when the model is one part, or else a Model
/// Renderer drawing every part where the model puts it, rather than an Obj
/// for each ([`unpack_model`] makes those). Each part is drawn with the
/// model's own material for it. Returns the Obj.
pub fn place_model(
    game: &mut Game,
    renderer: &mut Renderer,
    path: &str,
    parent: Option<Entity>,
    scene: Option<AssetGuid>,
    position: Vec3,
) -> Result<Entity> {
    let guid = game.content.db().guid_of(path).with_context(|| format!("{path} is not in the project"))?;
    let parts: Vec<ModelPart> = (game.content.model(renderer, guid)?.iter())
        .map(|node| ModelPart {
            name: node.name.clone(),
            transform: Transform::new(node.position, node.rotation, node.scale),
            mesh: node.mesh_part.clone(),
            material: node.material_part.as_ref().map(|material| format!("{path}#{material}")),
        })
        .collect();
    ensure!(!parts.is_empty(), "{path} has nothing to draw");
    let name = stem(path);
    let mut text = String::from("format = 1\n");
    // Each part's Obj, by its id in the file.
    let mut placed: Vec<(u32, Transform)> = Vec::new();
    // A model made to animate keeps its skeleton with a Model Renderer, however few its parts.
    let animated = game.content.model_animation(guid).is_some();
    if let ([part], false) = (parts.as_slice(), animated) {
        text += &model_obj(path, 1, name, None, Some(part));
        placed.push((1, part.transform));
    } else {
        text += &model_obj(path, 1, name, None, None);
        let model = toml::Value::String(guid.to_string());
        text += &format!("[obj.model]\nmodel = {model}  # {path}\n");
    }
    let file = scene_file::parse(&text, path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, Some(renderer), &file, &HashSet::new())?;
    game.levels.track_new(parent.and_then(|p| game.levels.scene_of(p)).or(scene), &spawned);
    let world = &mut game.engine.world;
    // Set once made, as a scene file keeps rotations as angles.
    for (id, transform) in placed {
        if let Some((_, entity)) = spawned.iter().find(|(at, _)| *at == ObjPath::new(id)) {
            world.insert(*entity, transform);
        }
    }
    let made = spawned.iter().find(|(at, _)| *at == ObjPath::new(1)).map(|(_, e)| *e);
    let made = made.context("the model's Obj was made")?;
    settle(world, made, parent, position);
    Ok(made)
}

/// Turns the whole model an Obj draws into an Obj inside it for each part,
/// where the model puts it, each with a Mesh Renderer drawing the part with
/// the model's material for it, as placing a model once did: for moving,
/// hiding or changing a part on its own. The Obj keeps everything else.
/// Returns the parts' Objs.
pub fn unpack_model(game: &mut Game, renderer: &mut Renderer, entity: Entity) -> Result<Vec<Entity>> {
    let source = game.engine.world.get::<ModelSource>(entity).cloned().context("it draws no model")?;
    let (path, nodes) = model_nodes(game, renderer, &source)?;
    // As this Obj changed them; those drawn by Objs of their own already stay so.
    let parts: Vec<ModelPart> = (0u32..)
        .zip(&nodes)
        .filter(|(node, _)| !source.0.part(*node).is_some_and(|part| part.apart))
        .map(|(node, part)| model_part(&path, part, source.0.part(node)))
        .collect();
    let name = stem(&path);
    let mut text = String::from("format = 1\n");
    for (id, part) in (1..).zip(&parts) {
        text += &model_obj(&path, id, if part.name.is_empty() { name } else { &part.name }, None, Some(part));
    }
    let file = scene_file::parse(&text, &path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, Some(renderer), &file, &HashSet::new())?;
    game.levels.track_new(game.levels.scene_of(entity), &spawned);
    let world = &mut game.engine.world;
    let mut made = Vec::new();
    for (id, part) in (1..).zip(&parts) {
        if let Some((_, part_obj)) = spawned.iter().find(|(at, _)| *at == ObjPath::new(id)) {
            world.insert(*part_obj, part.transform);
            world.insert(*part_obj, Parent(entity));
            made.push(*part_obj);
        }
    }
    world.remove::<voxear_render::ModelRenderer>(entity);
    world.remove::<ModelSource>(entity);
    Ok(made)
}

/// The path of the model an Obj draws whole, and its nodes, loading it.
fn model_nodes(
    game: &mut Game,
    renderer: &mut Renderer,
    source: &ModelSource,
) -> Result<(String, Vec<voxear_render::ImportedNode>)> {
    let guid = game.content.parse(&source.0.model)?.guid().with_context(|| format!("{} is no model", source.0.model))?;
    let path = game.content.db().get(guid).map(|entry| entry.path.clone()).context("the model is not in the project")?;
    Ok((path, game.content.model(renderer, guid)?.to_vec()))
}

/// A node of the model at `path` as placing it needs, as `changed` changes
/// it.
fn model_part(path: &str, node: &voxear_render::ImportedNode, changed: Option<&PartData>) -> ModelPart {
    let placed = (node.position, node.rotation, node.scale);
    let (position, rotation, scale) = changed.map_or(placed, |changed| changed.transform(placed));
    let own = node.material_part.as_ref().map(|material| format!("{path}#{material}"));
    ModelPart {
        name: node.name.clone(),
        transform: Transform::new(position, rotation, scale),
        mesh: node.mesh_part.clone(),
        material: changed.and_then(|changed| changed.material.clone()).or(own),
    }
}

/// Changes one part of the model an Obj draws whole, as `change` says, and
/// draws the model again.
pub fn change_part(
    game: &mut Game,
    renderer: &mut Renderer,
    entity: Entity,
    node: u32,
    change: impl FnOnce(&mut PartData),
) -> Result<()> {
    let mut source = game.engine.world.get::<ModelSource>(entity).cloned().context("it draws no model")?;
    change(source.0.part_mut(node));
    source.0.tidy_parts();
    let drawn = voxear_engine::prefab::model_renderer(&mut game.content, renderer, &source.0)?;
    let world = &mut game.engine.world;
    world.insert(entity, drawn);
    world.insert(entity, source);
    Ok(())
}

/// Takes one part out of the model an Obj draws whole into an Obj of its
/// own inside it, drawing what it drew where it was, to change and add
/// components to on its own. Returns the new Obj.
pub fn split_part(game: &mut Game, renderer: &mut Renderer, entity: Entity, node: u32) -> Result<Entity> {
    let source = game.engine.world.get::<ModelSource>(entity).cloned().context("it draws no model")?;
    let (path, nodes) = model_nodes(game, renderer, &source)?;
    let at = nodes.get(node as usize).context("the model has no such part")?;
    let part = model_part(&path, at, source.0.part(node));
    let name = if part.name.is_empty() { stem(&path).to_string() } else { part.name.clone() };
    let text = format!("format = 1\n{}", model_obj(&path, 1, &name, None, Some(&part)));
    let file = scene_file::parse(&text, &path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, Some(renderer), &file, &HashSet::new())?;
    game.levels.track_new(game.levels.scene_of(entity), &spawned);
    let made = spawned.first().map(|(_, made)| *made).context("the part's Obj was made")?;
    game.engine.world.insert(made, part.transform);
    game.engine.world.insert(made, Parent(entity));
    change_part(game, renderer, entity, node, |changed| changed.apart = true)?;
    Ok(made)
}

/// Places the sound at `path` (from the project's folder) as an Obj named
/// for it, with an Audio Source playing it, where [`place_prefab`] places
/// an instance, as Unity does with a clip dragged into the scene. Returns
/// it.
pub fn place_sound(
    game: &mut Game,
    path: &str,
    parent: Option<Entity>,
    scene: Option<AssetGuid>,
    position: Vec3,
) -> Result<Entity> {
    let guid = game.content.db().guid_of(path).with_context(|| format!("{path} is not in the project"))?;
    let string = |text: &str| toml::Value::String(text.to_string());
    let text = format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = {}\n[obj.audio]\nsound = {}\n",
        string(stem(path)),
        string(&guid.to_string())
    );
    let file = scene_file::parse(&text, path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, None, &file, &HashSet::new())?;
    game.levels.track_new(parent.and_then(|p| game.levels.scene_of(p)).or(scene), &spawned);
    let made = spawned.first().map(|(_, entity)| *entity).context("the sound's Obj was made")?;
    settle(&mut game.engine.world, made, parent, position);
    Ok(made)
}

/// A file's name without its folder or extension, as Objs placed from it
/// are named.
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// A model's meshes, materials and clips, as the Project panel shows them
/// when the model is opened out: each mesh by the name the model gives it,
/// then each material it uses, then each clip, each with what a field takes
/// for it.
/// Nothing when the model cannot be read.
pub fn model_parts(game: &mut Game, renderer: &mut Renderer, path: &str) -> Vec<AssetPart> {
    let Some(guid) = game.content.db().guid_of(path) else { return Vec::new() };
    let Ok(nodes) = game.content.model(renderer, guid) else { return Vec::new() };
    let mut meshes: Vec<AssetPart> = Vec::new();
    let mut materials: Vec<String> = Vec::new();
    for node in nodes {
        let reference = format!("{path}#{}", node.mesh_part);
        if !meshes.iter().any(|mesh| mesh.reference.as_deref() == Some(&reference)) {
            let name = if node.name.is_empty() { format!("Mesh {}", meshes.len() + 1) } else { node.name.clone() };
            meshes.push(AssetPart { name, icon: "box", reference: Some(reference) });
        }
        if let Some(material) = node.material_part.as_ref().filter(|m| !materials.contains(m)) {
            materials.push(material.clone());
        }
    }
    let named = game.content.loaded_model_materials(guid).unwrap_or_default();
    let materials = materials.iter().enumerate().map(|(index, material)| AssetPart {
        name: named
            .iter()
            .find(|(part, name)| part == material && !name.is_empty())
            .map_or_else(|| format!("Material {}", index + 1), |(_, name)| name.clone()),
        icon: "material",
        reference: Some(format!("{path}#{material}")),
    });
    let clips = game.content.model_animation(guid).map_or_else(Vec::new, |animation| {
        let clip = |name: &String| {
            AssetPart { name: name.clone(), icon: "play", reference: Some(format!("{path}#clip:{name}")) }
        };
        animation.clips.iter().map(|c| clip(&c.name)).collect()
    });
    meshes.into_iter().chain(materials).chain(clips).collect()
}

/// A part of a model, as placing it needs: its name, where the model puts
/// it, its mesh in the model, and its material, the model's or another.
struct ModelPart {
    name: String,
    transform: Transform,
    mesh: String,
    material: Option<String>,
}

/// A scene file's entry for an Obj of a model at `path`, drawing `part`.
fn model_obj(path: &str, id: u32, name: &str, parent: Option<u32>, part: Option<&ModelPart>) -> String {
    let string = |text: &str| toml::Value::String(text.to_string());
    let mut entry = format!("\n[[obj]]\nid = {id}\nname = {}\n", string(name));
    if let Some(parent) = parent {
        entry += &format!("parent = {parent}\n");
    }
    if let Some(part) = part {
        entry += &format!("[obj.renderer]\nmesh = {}\n", string(&format!("{path}#{}", part.mesh)));
        if let Some(material) = &part.material {
            entry += &format!("material = {}\n", string(material));
        }
    }
    entry
}

/// Puts an Obj just placed inside `parent`, or at the top moved by
/// `position`, and numbers its name as a copy's when an Obj has it already.
fn settle(world: &mut World, made: Entity, parent: Option<Entity>, position: Vec3) {
    match parent {
        Some(parent) => {
            world.insert(made, Parent(parent));
        }
        None => {
            if let Some(transform) = world.get_mut::<Transform>(made) {
                transform.position += position;
            }
        }
    }
    let name = world.get::<Name>(made).map(|n| n.0.clone()).unwrap_or_default();
    let taken =
        world.query_ref::<(Entity, &Name)>().without::<Destroying>().into_iter().any(|(e, n)| e != made && n.0 == name);
    if taken {
        let numbered = copy_name(world, &name);
        world.insert(made, Name(numbered));
    }
}

/// Copies an Obj and everything under it, beside it in its scene, named
/// as Unity names copies. Returns the copy.
pub fn duplicate(game: &mut Game, renderer: Option<&mut Renderer>, root: Entity) -> Result<Entity> {
    let objs: Vec<(ObjPath, Entity)> =
        family(&game.engine.world, root).into_iter().map(|entity| (ObjPath::default(), entity)).collect();
    let (mut file, ids) = scene_file::capture_scene_with_ids(&game.engine.world, &mut game.content, &objs);
    (file.sky, file.post) = (None, None);
    let id = ids.get(&root).copied().context("the Obj can be copied")?;
    let name = game.engine.world.get::<Name>(root).map(|n| n.0.clone()).unwrap_or_default();
    if let Some(obj) = file.objs.iter_mut().find(|obj| obj.id == id) {
        obj.name = copy_name(&game.engine.world, &name);
    }
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, renderer, &file, &HashSet::new())?;
    game.levels.track_new(game.levels.scene_of(root), &spawned);
    let copy = spawned.iter().find(|(path, _)| *path == ObjPath::new(id)).map(|(_, e)| *e);
    let copy = copy.context("the copy was made")?;
    if let Some(parent) = game.engine.world.get::<Parent>(root).map(|p| p.0) {
        game.engine.world.insert(copy, Parent(parent));
    }
    // Just after the original, as Unity puts a copy.
    if let Some(scene) = game.levels.scene_of(copy) {
        let copied = family(&game.engine.world, copy);
        let after = next_sibling(game, root, scene, &copied);
        let ordered: Vec<Entity> = game.levels.tracked(&copied).into_iter().map(|t| t.entity).collect();
        game.levels.move_objs(&ordered, scene, after);
    }
    Ok(copy)
}

/// Makes a prefab of an Obj and everything under it, written to `file`
/// (`path` from the project's folder): the Obj as its root, at the origin,
/// keeping its rotation and scale. Then, as in Unity, the Obj becomes an
/// instance of it: an instance goes where it stood, with its parent,
/// Transform, name and place in the scene, and it is taken away. Returns the
/// instance and what was taken, for one step to undo.
pub fn make_prefab(
    game: &mut Game,
    renderer: Option<&mut Renderer>,
    root: Entity,
    file: &std::path::Path,
    path: &str,
) -> Result<(Entity, Taken)> {
    let objs: Vec<(ObjPath, Entity)> =
        family(&game.engine.world, root).into_iter().map(|entity| (ObjPath::default(), entity)).collect();
    let (mut scene, ids) = scene_file::capture_scene_with_ids(&game.engine.world, &mut game.content, &objs);
    let id = ids.get(&root).copied().context("the Obj can be captured")?;
    if let Some(top) = scene.objs.iter_mut().find(|obj| obj.id == id) {
        (top.parent, top.position) = (None, None);
    }
    let prefab = PrefabFile { format: PREFAB_FORMAT, objs: scene.objs };
    let text = format!("# Voxear prefab\n{}", toml::to_string(&prefab)?);
    std::fs::write(file, text).with_context(|| format!("writing {}", file.display()))?;
    game.content.rescan()?;

    let world = &game.engine.world;
    let (parent, transform) = (world.get::<Parent>(root).map(|p| p.0), world.get::<Transform>(root).copied());
    let name = world.get::<Name>(root).map(|n| n.0.clone()).unwrap_or_default();
    let reference = toml::Value::String(path.to_string());
    let one = scene_file::parse(&format!("format = 1\n\n[[obj]]\nid = 1\nprefab = {reference}\n"), path)?;
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, renderer, &one, &HashSet::new())?;
    let scene_of = game.levels.scene_of(root);
    game.levels.track_new(scene_of, &spawned);
    let instance = spawned.iter().find(|(at, _)| *at == ObjPath::new(1)).map(|(_, e)| *e);
    let instance = instance.context("the instance was made")?;
    let world = &mut game.engine.world;
    if let Some(parent) = parent {
        world.insert(instance, Parent(parent));
    }
    if let Some(transform) = transform {
        world.insert(instance, transform);
    }
    world.insert(instance, Name(name));
    // Where the Obj was in its scene, then without it.
    if let Some(scene) = game.levels.scene_of(instance) {
        let made = family(&game.engine.world, instance);
        let after = next_sibling(game, root, scene, &made);
        let ordered: Vec<Entity> = game.levels.tracked(&made).into_iter().map(|t| t.entity).collect();
        game.levels.move_objs(&ordered, scene, after);
    }
    Ok((instance, take(game, root)))
}

/// Takes an Obj and everything under it out of the world and the level's
/// scenes, keeping what is needed to bring them back with [`bring_back`].
pub fn take(game: &mut Game, root: Entity) -> Taken {
    let family = family(&game.engine.world, root);
    let world = &game.engine.world;
    let parents = family.iter().filter_map(|&e| world.get::<Parent>(e).map(|p| (e, p.0))).collect();
    let tracked = game.levels.untrack(&family);
    let mut parts: Vec<Part> = Vec::new();
    for &entity in &family {
        let found = tracked.iter().find(|t| t.entity == entity);
        let (scene, path, at) =
            found.map_or((None, ObjPath::default(), usize::MAX), |t| (Some(t.scene), t.path.clone(), t.index));
        match parts.iter_mut().find(|part| part.scene == scene) {
            Some(part) => part.paths.push((path, entity, at)),
            None => parts.push(Part { scene, paths: vec![(path, entity, at)], file: SceneFile::default() }),
        }
    }
    for part in &mut parts {
        let objs: Vec<(ObjPath, Entity)> = part.paths.iter().map(|(path, entity, _)| (path.clone(), *entity)).collect();
        let (file, ids) = scene_file::capture_scene_with_ids(&game.engine.world, &mut game.content, &objs);
        part.file = SceneFile { lighting: None, sky: None, post: None, ..file };
        // Objs no scene kept are known by the ids the file gave them.
        for (path, entity, _) in &mut part.paths {
            if let (true, Some(&id)) = (path.is_root(), ids.get(entity)) {
                *path = ObjPath::new(id);
            }
        }
    }
    game.engine.destroy_objs(&[root]);
    Taken { root, parents, parts }
}

/// Brings taken Objs back as they were, into the scenes they were in, and
/// returns the new entity of each one taken.
pub fn bring_back(
    game: &mut Game,
    mut renderer: Option<&mut Renderer>,
    taken: &Taken,
) -> Result<HashMap<Entity, Entity>> {
    let mut renamed = HashMap::new();
    for part in &taken.parts {
        let renderer = renderer.as_deref_mut();
        let spawned = scene_file::spawn(&mut game.engine, &mut game.content, renderer, &part.file, &HashSet::new())?;
        for (path, entity) in &spawned {
            if let Some((_, old, _)) = part.paths.iter().find(|(p, ..)| p == path) {
                renamed.insert(*old, *entity);
            }
        }
        match part.scene {
            Some(scene) => {
                // Back at the places they had, so the scene's order is as it was.
                let place =
                    |path: &ObjPath| part.paths.iter().find(|(p, ..)| p == path).map_or(usize::MAX, |(.., at)| *at);
                let tracked: Vec<Tracked> = spawned
                    .iter()
                    .map(|(path, entity)| Tracked { scene, path: path.clone(), entity: *entity, index: place(path) })
                    .collect();
                game.levels.track(&tracked);
            }
            None => {
                game.levels.track_new(None, &spawned);
            }
        }
    }
    // A scene file keeps parents only among its own Objs: the top one's, and
    // any across scenes, are put back here.
    let world = &mut game.engine.world;
    for (child, parent) in &taken.parents {
        let parent = renamed.get(parent).copied().unwrap_or(*parent);
        let Some(&child) = renamed.get(child) else { continue };
        if world.is_alive(parent) && !world.has::<Destroying>(parent) && !world.has::<Parent>(child) {
            world.insert(child, Parent(parent));
        }
    }
    // A LOD Group that stayed shows the levels' Objs that came back.
    let groups: Vec<Entity> = world.query_ref::<(Entity, &LodGroup)>().into_iter().map(|(e, _)| e).collect();
    for group in groups {
        let Some(group) = world.get_mut::<LodGroup>(group) else { continue };
        for obj in group.levels.iter_mut().flat_map(|level| level.objs.iter_mut()) {
            *obj = renamed.get(obj).copied().unwrap_or(*obj);
        }
    }
    renamed.get(&taken.root).context("the top Obj came back")?;
    Ok(renamed)
}

/// Where an Obj is moved to in the Hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    /// Inside another Obj, after the Objs already there.
    Into(Entity),
    /// Beside another Obj, under the same parent: just before it, or just
    /// after it.
    Before(Entity),
    After(Entity),
    /// At the top of a scene, after the Objs already there.
    SceneEnd(AssetGuid),
}

/// Moves an Obj, and everything under it, keeping where it is in the world,
/// as Unity does. It goes into the scene of where it lands, and to its place
/// in that scene's order. Returns where it was and is now, or `None` when
/// nothing changed or it would go inside itself.
pub fn move_obj(game: &mut Game, entity: Entity, to: Destination) -> Option<(Placing, Placing)> {
    let world = &game.engine.world;
    let family = family(world, entity);
    let target = match to {
        Destination::Into(t) | Destination::Before(t) | Destination::After(t) => Some(t),
        Destination::SceneEnd(_) => None,
    };
    if target.is_some_and(|t| family.contains(&t)) {
        return None;
    }
    let parent = match to {
        Destination::Into(t) => Some(t),
        Destination::Before(t) | Destination::After(t) => world.get::<Parent>(t).map(|p| p.0),
        Destination::SceneEnd(_) => None,
    };
    let scene = match (to, target) {
        (Destination::SceneEnd(scene), _) => Some(scene),
        (_, Some(t)) => game.levels.scene_of(t),
        _ => None,
    };
    let scene = scene.or_else(|| game.levels.scene_of(entity)).or_else(|| game.levels.active_scene_guid())?;
    let anchor = match to {
        Destination::Before(t) => Some(t),
        Destination::After(t) => next_sibling(game, t, scene, &family),
        Destination::Into(_) | Destination::SceneEnd(_) => None,
    };
    let before = Placing::of(game, entity)?;
    let world = &game.engine.world;
    let inside = parent.map_or(Mat4::IDENTITY, |p| world_matrix(world, p));
    let (scale, rotation, position) = (inside.inverse() * world_matrix(world, entity)).to_scale_rotation_translation();
    let transform = Transform::new(position, rotation.normalize(), scale);
    // In the order the scenes keep them, so it moves as a block.
    let ordered: Vec<Entity> = game.levels.tracked(&family).into_iter().map(|t| t.entity).collect();
    let tracked = game.levels.move_objs(&ordered, scene, anchor);
    let after = Placing { parent, transform, tracked };
    after.apply(game, entity);
    let moved = after.parent != before.parent || after.tracked != before.tracked;
    moved.then_some((before, after))
}

/// The next Obj under the same parent as `of` in its scene's order, leaving
/// out the Objs being moved.
fn next_sibling(game: &Game, of: Entity, scene: AssetGuid, moving: &[Entity]) -> Option<Entity> {
    let world = &game.engine.world;
    let parent = world.get::<Parent>(of).map(|p| p.0);
    let order = game.levels.scene_order().into_iter().find(|(guid, ..)| *guid == scene)?.2;
    let at = order.iter().position(|e| *e == of)?;
    order[at + 1..]
        .iter()
        .copied()
        .find(|&e| !moving.contains(&e) && world.get::<Parent>(e).map(|p| p.0) == parent && !world.has::<Destroying>(e))
}

/// An Obj's place in the world, from its own Transform and its parents'.
pub fn world_matrix(world: &World, entity: Entity) -> Mat4 {
    let local = |entity: Entity| world.get::<Transform>(entity).map_or(Mat4::IDENTITY, Transform::matrix);
    let mut matrix = local(entity);
    let mut parent = world.get::<Parent>(entity).map(|p| p.0);
    // A cycle should not happen, but must not hang the editor if it does.
    for _ in 0..256 {
        let Some(at) = parent else { break };
        matrix = local(at) * matrix;
        parent = world.get::<Parent>(at).map(|p| p.0);
    }
    matrix
}

/// "Cube" copied is "Cube (1)", and "Cube (1)" is "Cube (2)", as in Unity:
/// the lowest number no Obj named after the same thing has yet.
fn copy_name(world: &World, name: &str) -> String {
    let base = numbered(name).map_or(name, |(base, _)| base);
    let used: HashSet<u32> = world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .filter_map(|(_, n)| numbered(&n.0).filter(|(b, _)| *b == base).map(|(_, number)| number))
        .collect();
    let number = (1..).find(|n| !used.contains(n)).unwrap_or(1);
    format!("{base} ({number})")
}

/// A name ending in a number in brackets, and the number.
fn numbered(name: &str) -> Option<(&str, u32)> {
    let (base, rest) = name.rsplit_once(" (")?;
    Some((base, rest.strip_suffix(')')?.parse().ok()?))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use voxear_engine::{Engine, Project};

    /// A level of one scene: a Box, holding a Lid, and a Crate beside them.
    pub(crate) fn game() -> (tempfile::TempDir, Game) {
        let dir = tempfile::tempdir().unwrap();
        let scene = "format = 1\n[[obj]]\nid = 1\nname = \"Box\"\nposition = [2.0, 0.0, 0.0]\n\
                     [[obj]]\nid = 2\nname = \"Lid\"\nparent = 1\nposition = [0.0, 1.0, 0.0]\n\
                     [[obj]]\nid = 3\nname = \"Crate\"\n";
        let files = [
            ("voxear.toml", "[project]\nname = \"Objs\"\nstart_level = \"Levels/Start.level\"\n"),
            ("Levels/Start.level", "format = 1\n[[scene]]\nscene = \"Scenes/Start.scene\"\n"),
            ("Scenes/Start.scene", scene),
        ];
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
        game.load_level("Levels/Start.level", None).unwrap();
        (dir, game)
    }

    pub(crate) fn named(game: &Game, name: &str) -> Option<Entity> {
        let world = &game.engine.world;
        let found = world.query_ref::<(Entity, &Name)>().without::<Destroying>().into_iter().find(|(_, n)| n.0 == name);
        found.map(|(e, _)| e)
    }

    fn saved(dir: &tempfile::TempDir, game: &mut Game) -> String {
        game.levels.save(&game.engine, &mut game.content, dir.path()).unwrap();
        std::fs::read_to_string(dir.path().join("Scenes/Start.scene")).unwrap()
    }

    #[test]
    fn deleted_objs_come_back_with_their_ids_and_parents() {
        let (dir, mut game) = game();
        let before = saved(&dir, &mut game);
        let (lid, box_) = (named(&game, "Lid").unwrap(), named(&game, "Box").unwrap());
        let taken = take(&mut game, box_);
        assert!(named(&game, "Box").is_none() && named(&game, "Lid").is_none(), "the Box went, and the Lid in it");
        assert!(!saved(&dir, &mut game).contains("Lid"), "and they are gone from the scene");

        let renamed = bring_back(&mut game, None, &taken).unwrap();
        let (new_box, new_lid) = (named(&game, "Box").unwrap(), named(&game, "Lid").unwrap());
        assert_eq!(renamed.get(&lid), Some(&new_lid));
        assert_eq!(game.engine.world.get::<Parent>(new_lid).map(|p| p.0), Some(new_box), "the Lid is in the Box again");
        let after = saved(&dir, &mut game);
        for line in before.lines().filter(|l| l.starts_with("id =") || l.starts_with("parent =")) {
            assert!(after.contains(line), "{line} is kept:\n{after}");
        }
    }

    #[test]
    fn copies_are_numbered_and_made_objs_get_new_ids() {
        let (dir, mut game) = game();
        let crate_ = named(&game, "Crate").unwrap();
        let copy = duplicate(&mut game, None, crate_).unwrap();
        assert_eq!(game.engine.world.get::<Name>(copy).unwrap().0, "Crate (1)");
        let again = duplicate(&mut game, None, copy).unwrap();
        assert_eq!(game.engine.world.get::<Name>(again).unwrap().0, "Crate (2)");
        let lamp = create(&mut game, None, 5, None, None, Vec3::new(1.0, 2.0, 3.0)).unwrap();
        assert_eq!(game.engine.world.get::<Transform>(lamp).unwrap().position, Vec3::new(1.0, 2.0, 3.0));
        let text = saved(&dir, &mut game);
        for id in 1..=6 {
            assert_eq!(text.matches(&format!("id = {id}\n")).count(), 1, "one Obj has id {id}:\n{text}");
        }
        assert!(text.contains("name = \"Point Light\""));
    }

    #[test]
    fn every_new_obj_is_made_and_the_rifle_has_what_a_bolt_action_needs() {
        let (_dir, mut game) = game();
        for kind in 0..NEW_OBJS.len() {
            create(&mut game, None, kind, None, None, Vec3::ZERO).unwrap();
        }
        let world = &game.engine.world;
        let rifle = named(&game, "Rifle").unwrap();
        let gun = world.get::<voxear_engine::guns::Gun>(rifle).unwrap().clone();
        let missing = voxear_engine::guns::mechanism::missing_parts(world, rifle, gun.kind);
        assert!(missing.is_empty(), "every part: {missing:?}");
        // The blocks are where the preset's sockets say: the butt at the stock's back, the muzzle at the barrel's
        // end, the front sight's top on the sight line.
        let block = |name: &str| {
            let entity =
                world.query_ref::<(Entity, &Name, &Parent)>().into_iter().find(|(_, n, p)| n.0 == name && p.0 == rifle);
            let transform = world.get::<Transform>(entity.unwrap().0).unwrap();
            (transform.position, transform.scale)
        };
        let ((stock, stock_size), (barrel, barrel_size)) = (block("Stock"), block("Barrel"));
        assert!((stock.z - stock_size.z / 2.0 - gun.butt[2]).abs() < 0.01, "butt: {stock} {stock_size}");
        assert!((barrel.z + barrel_size.z / 2.0 - gun.muzzle[2]).abs() < 0.01, "muzzle: {barrel} {barrel_size}");
        let (front, front_size) = block("Front Sight");
        assert!((front.y + front_size.y / 2.0 - gun.front_sight[1]).abs() < 0.01, "front sight: {front}");
        assert!((front.z - gun.front_sight[2]).abs() < 0.01, "front sight: {front}");
    }

    /// The Objs' names in their scene's order.
    fn order(game: &Game) -> Vec<String> {
        let (_, _, entities) = game.levels.scene_order().into_iter().next().unwrap();
        entities.iter().map(|&e| game.engine.world.get::<Name>(e).unwrap().0.clone()).collect()
    }

    #[test]
    fn objs_move_to_their_places_and_come_back_to_them() {
        let (dir, mut game) = game();
        let (box_, lid, crate_) =
            (named(&game, "Box").unwrap(), named(&game, "Lid").unwrap(), named(&game, "Crate").unwrap());
        move_obj(&mut game, crate_, Destination::Before(box_)).unwrap();
        assert_eq!(order(&game), ["Crate", "Box", "Lid"]);
        let (before, after) = move_obj(&mut game, lid, Destination::After(crate_)).unwrap();
        assert_eq!(order(&game), ["Crate", "Lid", "Box"], "out of the Box, beside the Crate");
        assert_eq!(game.engine.world.get::<Parent>(lid), None);
        before.apply(&mut game, lid);
        assert_eq!(order(&game), ["Crate", "Box", "Lid"], "undone, back where it was");
        assert_eq!(game.engine.world.get::<Parent>(lid).map(|p| p.0), Some(box_));
        after.apply(&mut game, lid);
        assert!(move_obj(&mut game, lid, Destination::After(crate_)).is_none(), "already there");

        let taken = take(&mut game, box_);
        assert_eq!(order(&game), ["Crate", "Lid"]);
        bring_back(&mut game, None, &taken).unwrap();
        assert_eq!(order(&game), ["Crate", "Lid", "Box"], "brought back at its place");
        let text = saved(&dir, &mut game);
        let at = |name: &str| text.find(&format!("name = \"{name}\"")).unwrap();
        assert!(at("Crate") < at("Lid") && at("Lid") < at("Box"), "and saved in that order:\n{text}");
        let copy = duplicate(&mut game, None, crate_).unwrap();
        assert_eq!(order(&game)[..2], ["Crate".to_string(), "Crate (1)".to_string()], "a copy goes just after");
        let _ = copy;
    }

    #[test]
    fn placed_prefabs_keep_their_own_offset_and_are_numbered() {
        let (dir, mut game) = game();
        let prefab = "format = 1\n[[obj]]\nid = 1\nname = \"Lamp\"\nposition = [0.0, 0.6, 0.0]\n\
                      [[obj]]\nid = 2\nname = \"Glow\"\nparent = 1\n";
        std::fs::write(dir.path().join("Lamp.prefab"), prefab).unwrap();
        game.content.rescan().unwrap();
        let lamp = place_prefab(&mut game, None, "Lamp.prefab", None, None, Vec3::new(4.0, 0.0, 2.0)).unwrap();
        assert_eq!(game.engine.world.get::<Transform>(lamp).unwrap().position, Vec3::new(4.0, 0.6, 2.0));
        assert_eq!(game.engine.world.get::<Name>(lamp).unwrap().0, "Lamp", "named for its prefab");
        let crate_ = named(&game, "Crate").unwrap();
        let inside =
            place_prefab(&mut game, None, "Lamp.prefab", Some(crate_), None, Vec3::new(9.0, 9.0, 9.0)).unwrap();
        assert_eq!(game.engine.world.get::<Name>(inside).unwrap().0, "Lamp (1)", "the second is numbered");
        assert_eq!(game.engine.world.get::<Parent>(inside).map(|p| p.0), Some(crate_));
        assert_eq!(game.engine.world.get::<Transform>(inside).unwrap().position, Vec3::new(0.0, 0.6, 0.0));
        let text = saved(&dir, &mut game);
        assert_eq!(text.matches("prefab = \"").count(), 2, "both are instances in the scene:\n{text}");
        assert!(!text.contains("name = \"Glow\""), "whose members come from the prefab:\n{text}");
    }

    #[test]
    fn reparenting_keeps_the_obj_where_it_is_in_the_world() {
        let (_dir, mut game) = game();
        let (lid, crate_) = (named(&game, "Lid").unwrap(), named(&game, "Crate").unwrap());
        let world_position = |game: &Game, e: Entity| world_matrix(&game.engine.world, e).w_axis.truncate();
        let at = world_position(&game, lid);
        assert_eq!(at, Vec3::new(2.0, 1.0, 0.0));
        let (before, after) = move_obj(&mut game, lid, Destination::Into(crate_)).unwrap();
        assert_eq!(game.engine.world.get::<Parent>(lid).map(|p| p.0), Some(crate_));
        assert!(world_position(&game, lid).distance(at) < 1e-5, "it stays put in the world");
        assert!(move_obj(&mut game, crate_, Destination::Into(lid)).is_none(), "not inside itself");
        before.apply(&mut game, lid);
        assert_eq!(game.engine.world.get::<Transform>(lid).unwrap().position, Vec3::new(0.0, 1.0, 0.0));
        after.apply(&mut game, lid);
        assert!(world_position(&game, lid).distance(at) < 1e-5);
    }
}
