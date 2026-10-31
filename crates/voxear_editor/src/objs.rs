//! Making, duplicating, deleting and reparenting Objs, as the Hierarchy
//! does. Objs are copied and brought back through scene files of them
//! (`scene_file::capture_scene` and `spawn`), so everything a scene holds
//! comes back, prefab instances included. They keep their place in the
//! level's scenes (`Levels::untrack` and `track`), so a save writes them
//! where they were, with the ids they had. A scene's order is its Objs'
//! order in the Hierarchy, so Objs moved or brought back keep their places
//! in it too.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use voxear_assets::AssetGuid;
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game, ObjPath, Tracked,
    prefab::{PREFAB_FORMAT, PrefabFile},
    scene_file::{self, SceneFile},
};
use voxear_render::Renderer;
use voxear_scene::{Destroying, Name, Parent, Transform};

/// What the Hierarchy's + makes, as Unity's GameObject menu: what the menu
/// says, the new Obj's name, and the rest of its scene file entry.
pub const NEW_OBJS: [(&str, &str, &str); 8] = [
    ("Empty", "Obj", ""),
    ("Cube", "Cube", "[obj.renderer]\nmesh = \"builtin:cube\""),
    ("Sphere", "Sphere", "[obj.renderer]\nmesh = \"builtin:sphere\""),
    ("Plane", "Plane", "[obj.renderer]\nmesh = \"builtin:plane\""),
    ("Directional Light", "Directional Light", "rotation = [50.0, -30.0, 0.0]\n[obj.light]\ntype = \"directional\""),
    ("Point Light", "Point Light", "[obj.light]\ntype = \"point\""),
    ("Spot Light", "Spot Light", "rotation = [90.0, 0.0, 0.0]\n[obj.light]\ntype = \"spot\""),
    ("Camera", "Camera", "[obj.camera]"),
];

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
    let (_, name, rest) = NEW_OBJS[kind.min(NEW_OBJS.len() - 1)];
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
        part.file = SceneFile { sky: None, post: None, ..file };
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
