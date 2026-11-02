//! LOD Groups in scene and prefab files, headless (no GPU, no scripts).

use voxear_ecs::Entity;
use voxear_engine::{Content, Engine, lod_groups::FileRef, scene_file};
use voxear_render::{LodGroup, LodMeasure};
use voxear_scene::Name;

// A rock whose levels are its children, the last a flat card.
const ROCK: &str = r#"
format = 1

[[obj]]
id = 1
name = "Rock"
[obj.lod_group]
levels = [{ objs = [2], until = 0.5 }, { objs = [3], until = 0.2 }, { objs = [4], until = 0.05 }]

[[obj]]
id = 2
name = "Rock High"
parent = 1

[[obj]]
id = 3
name = "Rock Low"
parent = 1

[[obj]]
id = 4
name = "Rock Card"
parent = 1
"#;

const FIELD: &str = r#"
format = 1

[[obj]]
id = 1
name = "Field"

[[obj]]
id = 2
name = "Boulder"
parent = 1
prefab = "Rock.prefab"

[[obj]]
id = 3
name = "Pebble"
parent = 1
prefab = "Rock.prefab"
[obj.lod_group]
measure = "distance"
cull = false
levels = [{ objs = ["3/2"], until = 10.0 }, { objs = ["3/3"], until = 30.0 }]
"#;

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        std::fs::write(dir.path().join(path), text).unwrap();
    }
    dir
}

fn named(engine: &Engine, name: &str) -> Vec<Entity> {
    let all = engine.world.query_ref::<(Entity, &Name)>();
    all.into_iter().filter(|(_, n)| n.0 == name).map(|(e, _)| e).collect()
}

fn parent_of(engine: &Engine, entity: Entity) -> Option<Entity> {
    engine.world.get::<voxear_scene::Parent>(entity).map(|p| p.0)
}

/// Each level's Objs, by name.
fn level_names(engine: &Engine, group: &LodGroup) -> Vec<String> {
    let name = |entity: &Entity| engine.world.get::<Name>(*entity).map_or("?".to_string(), |n| n.0.clone());
    group.levels.iter().map(|level| level.objs.iter().map(name).collect::<Vec<_>>().join(", ")).collect()
}

#[test]
fn a_lod_groups_levels_load_as_its_objs_and_save_back_the_same() {
    let dir = project(&[("Rock.scene", ROCK)]);
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Rock.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let rock = named(&engine, "Rock")[0];
    let group = engine.world.get::<LodGroup>(rock).expect("the rock has a LOD Group").clone();
    assert_eq!(level_names(&engine, &group), ["Rock High", "Rock Low", "Rock Card"]);
    assert_eq!(group.levels.iter().map(|l| l.until).collect::<Vec<_>>(), [0.5, 0.2, 0.05]);
    assert!(group.cull && group.measure == LodMeasure::ScreenSize);

    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    assert_eq!(saved.objs, scene.objs, "saved as it was written");
}

#[test]
fn each_instance_of_a_prefab_has_its_own_levels_and_saves_only_what_it_changed() {
    let dir = project(&[("Rock.prefab", ROCK), ("Field.scene", FIELD)]);
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Field.scene")).unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();

    for root in ["Boulder", "Pebble"] {
        let root = named(&engine, root)[0];
        let group = engine.world.get::<LodGroup>(root).expect("each instance has the group");
        for &level in group.levels.iter().flat_map(|level| &level.objs) {
            assert_eq!(parent_of(&engine, level), Some(root), "a level is the instance's own child");
        }
    }
    let pebble = named(&engine, "Pebble")[0];
    let group = engine.world.get::<LodGroup>(pebble).unwrap().clone();
    assert_eq!((group.measure, group.cull), (LodMeasure::Distance, false));
    assert_eq!(level_names(&engine, &group), ["Rock High", "Rock Low"]);

    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let instance = |name: &str| saved.objs.iter().find(|o| o.name == name).unwrap();
    assert_eq!(instance("Boulder").lod_group, None, "the prefab's own group is not written again");
    let levels = &instance("Pebble").lod_group.as_ref().expect("the instance's own group").levels;
    let id = instance("Pebble").id;
    assert_eq!(levels[0].objs, [FileRef::Path(format!("{id}/2"))]);
}

#[test]
fn a_level_outside_its_file_is_a_mistake() {
    let dir = project(&[]);
    let mut content = Content::open(dir.path()).unwrap();
    let stray: scene_file::SceneFile =
        toml::from_str("format = 1\n[[obj]]\nid = 1\nname = \"A\"\nlod_group = { levels = [{ objs = [9], until = 0.5 }] }\n")
            .unwrap();
    let flat = voxear_engine::prefab::expand(&mut content, &stray.objs).unwrap();
    let error = format!("{:#}", voxear_engine::Template::resolve(&mut content, None, &flat).unwrap_err());
    assert!(error.contains("a level of its LOD Group is Obj 9, which is not in the same file"), "{error}");
}
