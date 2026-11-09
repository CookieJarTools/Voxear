//! Prefabs on a throwaway project, headless (no GPU, no scripts).

use std::collections::BTreeMap;

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Content, Engine, ObjPath,
    prefab::{self, FromPrefab, PrefabInstance},
    scene_file,
};
use voxear_render::Light;
use voxear_scene::{
    AttachedBehaviour, BehaviourHandle, BehaviourSpec, Behaviours, Name, Parent, Placement, Switch, Transform,
};

const LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Lamp"
position = [0.0, 3.0, 0.0]

[[obj]]
id = 2
name = "Glow"
parent = 1
position = [0.0, 0.5, 0.0]
light = { type = "point", color = [1.0, 1.0, 1.0], intensity = 5.0 }
"#;

// A variant: its root is a Lamp, tinted red, with a child of its own.
const RED_LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Red Lamp"
prefab = "Prefabs/Lamp.prefab"
[obj.overrides.2]
light = { type = "point", color = [1.0, 0.0, 0.0], intensity = 5.0 }

[[obj]]
id = 2
name = "Spark"
parent = 1
"#;

// Nesting: a post with a lamp on top.
const STREET_LIGHT: &str = r#"
format = 1

[[obj]]
id = 1
name = "Street Light"

[[obj]]
id = 2
prefab = "Prefabs/Lamp.prefab"
parent = 1
position = [0.0, 4.0, 0.0]
"#;

const SCENE: &str = r#"
format = 1

[[obj]]
id = 1
name = "Street"

[[obj]]
id = 2
prefab = "Prefabs/StreetLight.prefab"
name = "Corner Light"
parent = 1
position = [10.0, 0.0, 0.0]
[obj.overrides."2/2"]
position = [0.0, 1.0, 0.0]

[[obj]]
id = 3
prefab = "Prefabs/RedLamp.prefab"
position = [-5.0, 0.0, 0.0]
"#;

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn street() -> tempfile::TempDir {
    project(&[
        ("Prefabs/Lamp.prefab", LAMP),
        ("Prefabs/RedLamp.prefab", RED_LAMP),
        ("Prefabs/StreetLight.prefab", STREET_LIGHT),
        ("Scenes/Street.scene", SCENE),
    ])
}

fn find(engine: &Engine, name: &str) -> Entity {
    let found = engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name);
    found.unwrap_or_else(|| panic!("no Obj named {name}")).0
}

fn parent_name(engine: &Engine, entity: Entity) -> String {
    let parent = engine.world.get::<Parent>(entity).expect("has a parent").0;
    engine.world.get::<Name>(parent).unwrap().0.clone()
}

#[test]
fn nested_prefabs_and_variants_expand_with_their_overrides() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Scenes/Street.scene")).unwrap();
    let objs = prefab::expand(&mut content, &scene.objs).unwrap();
    let paths: Vec<String> = objs.iter().map(|o| o.path.to_string()).collect();
    assert_eq!(paths, ["1", "2", "2/2", "2/2/2", "3", "3/1/2", "3/2"]);

    let mut engine = Engine::new();
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let transform = |name| *engine.world.get::<Transform>(find(&engine, name)).unwrap();

    // The instance names and places the root; the nested lamp keeps its name.
    assert_eq!(parent_name(&engine, find(&engine, "Corner Light")), "Street");
    assert_eq!(transform("Corner Light").position, Vec3::new(10.0, 0.0, 0.0));
    let lamps: Vec<Entity> = engine
        .world
        .query_ref::<(Entity, &Name)>()
        .into_iter()
        .filter(|(_, n)| n.0 == "Lamp")
        .map(|(e, _)| e)
        .collect();
    assert_eq!(lamps.len(), 1);
    assert_eq!(parent_name(&engine, lamps[0]), "Corner Light");
    assert_eq!(engine.world.get::<Transform>(lamps[0]).unwrap().position, Vec3::new(0.0, 4.0, 0.0));

    // The scene overrides a Glow two prefabs deep; the variant tints its own.
    let glows: Vec<(Entity, Light, Transform)> = engine
        .world
        .query_ref::<(Entity, &Name, &Light, &Transform)>()
        .into_iter()
        .filter(|(_, n, ..)| n.0 == "Glow")
        .map(|(e, _, l, t)| (e, *l, *t))
        .collect();
    assert_eq!(glows.len(), 2);
    let corner = glows.iter().find(|(e, ..)| parent_name(&engine, *e) == "Lamp").unwrap();
    assert_eq!(corner.2.position, Vec3::new(0.0, 1.0, 0.0));
    let red = glows.iter().find(|(e, ..)| parent_name(&engine, *e) == "Red Lamp").unwrap();
    assert_eq!(red.1.color, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(parent_name(&engine, find(&engine, "Spark")), "Red Lamp");

    // Every Obj from an instance points at the outermost instance's root.
    let corner_light = find(&engine, "Corner Light");
    assert!(engine.world.has::<PrefabInstance>(corner_light));
    let link = engine.world.get::<FromPrefab>(corner.0).unwrap();
    assert_eq!((link.instance, link.path.to_string()), (corner_light, "2/2".to_string()));
}

#[test]
fn saving_keeps_instances_and_writes_changes_as_overrides() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let scene = scene_file::read(&dir.path().join("Scenes/Street.scene")).unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();

    // Move the red lamp's glow, as an editor would.
    let red_glow = engine
        .world
        .query_ref::<(Entity, &Name, &Parent)>()
        .into_iter()
        .find(|(_, n, p)| n.0 == "Glow" && engine.world.get::<Name>(p.0).unwrap().0 == "Red Lamp")
        .unwrap()
        .0;
    engine.world.get_mut::<Transform>(red_glow).unwrap().position = Vec3::new(0.0, 2.0, 0.0);

    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    assert_eq!(saved.objs.len(), 3, "Objs inside instances are not written on their own");
    let corner = saved.objs.iter().find(|o| o.name == "Corner Light").unwrap();
    assert!(corner.prefab.is_some());
    assert_eq!(corner.position, Some([10.0, 0.0, 0.0]));
    assert_eq!(corner.overrides.keys().collect::<Vec<_>>(), ["2/2"]);
    let red = saved.objs.iter().find(|o| o.prefab.is_some() && o.name.is_empty()).expect("keeps the variant's name");
    assert_eq!(red.overrides.keys().collect::<Vec<_>>(), ["1/2"]);
    assert_eq!(red.overrides["1/2"].get("position").unwrap().to_string(), "[0.0, 2.0, 0.0]");

    // And it loads back the same.
    let path = dir.path().join("Scenes/Copy.scene");
    scene_file::write(&path, &saved).unwrap();
    let mut reloaded = Engine::new();
    let copy = scene_file::read(&path).unwrap();
    scene_file::spawn(&mut reloaded, &mut content, None, &copy, &Default::default()).unwrap();
    assert_eq!(reloaded.world.entity_count(), engine.world.entity_count());
    let moved = reloaded
        .world
        .query_ref::<(&Name, &Transform)>()
        .into_iter()
        .any(|(n, t)| n.0 == "Glow" && t.position == Vec3::new(0.0, 2.0, 0.0));
    assert!(moved);
}

#[test]
fn instantiate_places_a_copy_under_a_parent() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let id = content.prefab(None, "Prefabs/RedLamp.prefab").unwrap();
    assert_eq!(content.prefab(None, "Prefabs/RedLamp.prefab").unwrap(), id, "loading twice returns the same prefab");
    let template = content.template(id).unwrap().clone();

    let holder = engine.world.spawn((Name("Holder".into()), Transform::IDENTITY));
    let placement = Placement::at(Vec3::new(1.0, 2.0, 3.0));
    let first = engine.instantiate(&template, Some(holder), Some(placement)).unwrap();
    let second = engine.instantiate(&template, None, None).unwrap();

    assert_eq!(engine.world.get::<Name>(first).unwrap().0, "Red Lamp");
    assert_eq!(engine.world.get::<Parent>(first).unwrap().0, holder);
    assert_eq!(engine.world.get::<Transform>(first).unwrap().position, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(engine.world.get::<Transform>(second).unwrap().position, Vec3::new(0.0, 3.0, 0.0), "the prefab's own");
    assert!(engine.world.get::<Parent>(second).is_none());
    let link = engine.world.get::<FromPrefab>(first).unwrap();
    assert_eq!((link.instance, link.path.clone()), (first, ObjPath::default()));
    // Holder, then two copies of the root, glow and spark.
    assert_eq!(engine.world.entity_count(), 7);
}

#[test]
fn mistakes_are_reported_clearly() {
    let looped = project(&[
        ("A.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"A\"\n[[obj]]\nid = 2\nparent = 1\nprefab = \"B.prefab\"\n"),
        ("B.prefab", "format = 1\n[[obj]]\nid = 1\nprefab = \"A.prefab\"\n"),
        ("Two.prefab", "format = 1\n[[obj]]\nid = 1\nname = \"A\"\n[[obj]]\nid = 2\nname = \"B\"\n"),
        ("Typo.prefab", "format = 1\n[[obj]]\nid = 1\nprefab = \"Two.prefab\"\n"),
    ]);
    let mut content = Content::open(looped.path()).unwrap();
    let error = |reference: &str, content: &mut Content| format!("{:#}", content.prefab(None, reference).unwrap_err());
    assert!(error("A.prefab", &mut content).contains("contains an instance of itself"));
    assert!(error("Two.prefab", &mut content).contains("more than one root"));

    let lamp = project(&[("Lamp.prefab", LAMP)]);
    let mut content = Content::open(lamp.path()).unwrap();
    let scene = |overrides: &str| -> scene_file::SceneFile {
        toml::from_str(&format!("format = 1\n[[obj]]\nid = 1\nprefab = \"Lamp.prefab\"\n{overrides}")).unwrap()
    };
    let expand =
        |text: &str, content: &mut Content| format!("{:#}", prefab::expand(content, &scene(text).objs).unwrap_err());
    assert!(expand("[obj.overrides.9]\nname = \"X\"\n", &mut content).contains("no Obj at that path"));
    assert!(expand("[obj.overrides.2]\nparent = 1\n", &mut content).contains("`parent` cannot be overridden"));
    assert!(expand("[obj.overrides.2]\nglow = 1\n", &mut content).contains("glow"), "typos are caught");
}

const TURRET: &str = r#"
format = 1

[[obj]]
id = 1
name = "Turret"
behaviours = ["Game.Turret"]
fields."Game.Turret" = { range = 10.0, target = "Player" }

[[obj]]
id = 2
name = "Barrel"
parent = 1
behaviours = ["Game.Recoil"]
fields."Game.Recoil" = { kick = 0.2, recover = 4.0 }
"#;

#[test]
fn behaviour_fields_layer_one_by_one() {
    let dir = project(&[("Turret.prefab", TURRET)]);
    let mut content = Content::open(dir.path()).unwrap();
    let scene: scene_file::SceneFile = toml::from_str(
        r#"
format = 1
[[obj]]
id = 1
prefab = "Turret.prefab"
fields."Game.Turret" = { range = 25.0 }
[obj.overrides.2.fields."Game.Recoil"]
kick = 0.5
"#,
    )
    .unwrap();
    let objs = prefab::expand(&mut content, &scene.objs).unwrap();
    let field = |path: &str, behaviour: &str, name: &str| {
        let obj = objs.iter().find(|o| o.path.to_string() == path).unwrap();
        obj.data.fields[behaviour][name].clone()
    };
    assert_eq!(field("1", "Game.Turret", "range"), toml::Value::Float(25.0));
    assert_eq!(field("1", "Game.Turret", "target"), toml::Value::from("Player"), "untouched fields stay");
    assert_eq!(field("1/2", "Game.Recoil", "kick"), toml::Value::Float(0.5));
    assert_eq!(field("1/2", "Game.Recoil", "recover"), toml::Value::Float(4.0));

    let stray: scene_file::SceneFile =
        toml::from_str("format = 1\n[[obj]]\nid = 1\nname = \"A\"\nfields.\"Game.Missing\" = { x = 1 }\n").unwrap();
    let flat = prefab::expand(&mut content, &stray.objs).unwrap();
    let error = format!("{:#}", voxear_engine::Template::resolve(&mut content, None, &flat).unwrap_err());
    assert!(error.contains("fields for Game.Missing, which is not one of its behaviours"), "{error}");
}

#[test]
fn behaviour_fields_are_saved_as_loaded() {
    let dir = project(&[("Turret.prefab", TURRET)]);
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let scene: scene_file::SceneFile = toml::from_str("format = 1\n[[obj]]\nid = 1\nname = \"Turret\"\n").unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    // Headless runs have no scripts, so the behaviour is attached by hand.
    let turret = spawned[0].1;
    let fields: toml::Table = toml::from_str("range = 12.0").unwrap();
    let spec = BehaviourSpec { fields: std::sync::Arc::new(fields), ..BehaviourSpec::new("Game.Turret") };
    engine.world.insert(turret, Behaviours(vec![AttachedBehaviour { handle: BehaviourHandle(0), spec }]));

    let saved = scene_file::capture(&engine.world, &mut content, &[turret]);
    let text = toml::to_string(&saved).unwrap();
    assert!(text.contains("behaviours = [\"Game.Turret\"]"), "{text}");
    let reloaded: scene_file::SceneFile = toml::from_str(&text).unwrap();
    assert_eq!(reloaded.objs[0].fields["Game.Turret"]["range"], toml::Value::Float(12.0));
}

#[test]
fn csharp_components_load_in_edit_mode_and_save_as_they_came() {
    let dir = project(&[("Turret.prefab", TURRET)]);
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    // An instance that adds a component to the prefab's root, and an Obj
    // with a behaviour and a component of its own.
    let scene: scene_file::SceneFile = toml::from_str(
        r#"
format = 1
[[obj]]
id = 1
prefab = "Turret.prefab"
components = ["Game.Spin"]
fields."Game.Spin" = { speed = 3.0 }

[[obj]]
id = 2
name = "Mill"
behaviours = ["Game.Creak"]
components = ["Game.Spin", "Game.Windy"]
fields."Game.Spin" = { speed = 90.0 }
"#,
    )
    .unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let mill = find(&engine, "Mill");
    let kept = &engine.world.get::<voxear_engine::EditBehaviours>(mill).unwrap().0;
    let kinds: Vec<(&str, bool)> = kept.iter().map(|s| (s.type_name.as_str(), s.component)).collect();
    assert_eq!(kinds, [("Game.Creak", false), ("Game.Spin", true), ("Game.Windy", true)]);
    assert_eq!(kept[1].fields["speed"], toml::Value::Float(90.0));

    let entities: Vec<Entity> = spawned.iter().map(|(_, entity)| *entity).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let instance = saved.objs.iter().find(|o| o.prefab.is_some()).unwrap();
    assert_eq!(instance.components, ["Game.Spin"]);
    assert!(instance.behaviours.is_empty(), "the prefab's own behaviour is not written again");
    assert_eq!(instance.fields["Game.Spin"]["speed"], toml::Value::Float(3.0));
    let saved_mill = saved.objs.iter().find(|o| o.name == "Mill").unwrap();
    assert_eq!(saved_mill.behaviours, ["Game.Creak"]);
    assert_eq!(saved_mill.components, ["Game.Spin", "Game.Windy"]);
    assert_eq!(saved_mill.fields["Game.Spin"]["speed"], toml::Value::Float(90.0));
}

const PULSING_LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Lamp"
behaviours = ["Game.Pulse"]
fields."Game.Pulse" = { target = { obj = 2 }, all = [{ obj = 1 }, { obj = 2 }] }

[[obj]]
id = 2
name = "Bulb"
parent = 1
"#;

const LAMP_POST: &str = r#"
format = 1

[[obj]]
id = 1
name = "Post"
behaviours = ["Game.Watch"]
fields."Game.Watch" = { bulb = { obj = "2/2" } }

[[obj]]
id = 2
prefab = "PulsingLamp.prefab"
parent = 1
"#;

/// The entity a bound reference field points at.
fn bound(spec: &BehaviourSpec, field: &str) -> Vec<Entity> {
    let entity = |value: &toml::Value| match value.as_table().and_then(|t| t.get("entity")) {
        Some(toml::Value::Integer(bits)) => Entity::from_bits(*bits as u64),
        other => panic!("{field} is not bound to an entity: {other:?}"),
    };
    match &spec.fields[field] {
        toml::Value::Array(items) => items.iter().map(entity).collect(),
        value => vec![entity(value)],
    }
}

#[test]
fn obj_references_point_at_each_instances_own_objs() {
    let dir = project(&[("PulsingLamp.prefab", PULSING_LAMP), ("LampPost.prefab", LAMP_POST)]);
    let mut content = Content::open(dir.path()).unwrap();
    let id = content.prefab(None, "LampPost.prefab").unwrap();
    let template = content.template(id).unwrap().clone();
    let mut world = voxear_ecs::World::new();

    for _ in 0..2 {
        let entities = template.spawn_objs(&mut world, None, None);
        let index = |name: &str| template.objs.iter().position(|o| o.name == name).unwrap();
        let (post, lamp, bulb) = (index("Post"), index("Lamp"), index("Bulb"));

        let watch = &template.behaviours(post, &entities)[0];
        assert_eq!(bound(watch, "bulb"), [entities[bulb]], "a reference into a nested prefab");
        let pulse = &template.behaviours(lamp, &entities)[0];
        assert_eq!(bound(pulse, "target"), [entities[bulb]], "a reference inside the nested prefab");
        assert_eq!(bound(pulse, "all"), [entities[lamp], entities[bulb]], "references in a list");
    }
}

/// An arm, and a holder under it whose Bone fields point at bones of its
/// model: one on its own, one in a list.
const ARM: &str = r#"
format = 1

[[obj]]
id = 1
name = "Arm"

[[obj]]
id = 2
name = "Holder"
parent = 1
behaviours = ["Game.Holder"]
fields."Game.Holder" = { grip = { obj = 1, bone = "hand_r" }, spares = [{ obj = 1, bone = "hand_l" }] }
"#;

#[test]
fn a_bone_fields_bone_stays_with_its_obj_through_instances_and_saves() {
    let dir = project(&[("Arm.prefab", ARM)]);
    let mut content = Content::open(dir.path()).unwrap();
    let scene: scene_file::SceneFile =
        toml::from_str("format = 1\n[[obj]]\nid = 4\nname = \"Left\"\n[[obj]]\nid = 6\nprefab = \"Arm.prefab\"\n").unwrap();

    // In an instance, the reference is to the instance's own arm, its bone kept.
    let flat = prefab::expand(&mut content, &scene.objs).unwrap();
    let holder = flat.iter().find(|o| o.path.to_string() == "6/2").expect("the instance's holder");
    let fields = &holder.data.fields["Game.Holder"];
    let written = |text: &str| toml::from_str::<toml::Table>(text).unwrap()["v"].clone();
    assert_eq!(fields["grip"], written("v = { obj = 6, bone = \"hand_r\" }"));
    assert_eq!(fields["spares"], written("v = [{ obj = 6, bone = \"hand_l\" }]"));

    // Bound to the entity it became, as scripts are handed it, the bone still beside it.
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let arm = spawned.iter().find(|(path, _)| path.to_string() == "6").unwrap().1;
    let holder = spawned.iter().find(|(path, _)| path.to_string() == "6/2").unwrap().1;
    let kept = engine.world.get::<voxear_engine::EditBehaviours>(holder).expect("kept as data in edit mode");
    let grip = kept.0[0].fields["grip"].as_table().unwrap().clone();
    assert_eq!(grip.get("entity"), Some(&toml::Value::Integer(arm.to_bits() as i64)));
    assert_eq!(grip.get("bone"), Some(&toml::Value::String("hand_r".into())));

    // Saved, the reference is a path in the file again, with its bone.
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let text = toml::to_string(&saved).unwrap();
    let again: scene_file::SceneFile = toml::from_str(&text).unwrap();
    let instance = again.objs.iter().find(|o| o.prefab.is_some()).unwrap();
    let flat = prefab::expand(&mut content, &again.objs).unwrap();
    let holder = flat.iter().find(|o| o.path.to_string() == format!("{}/2", instance.id)).unwrap();
    let grip = holder.data.fields["Game.Holder"]["grip"].as_table().unwrap();
    assert_eq!(grip.get("obj"), Some(&toml::Value::Integer(i64::from(instance.id))), "{text}");
    assert_eq!(grip.get("bone"), Some(&toml::Value::String("hand_r".into())), "{text}");

    // An Obj of the scene's own pointing at a bone saves the same way.
    let left = spawned.iter().find(|(path, _)| path.to_string() == "4").unwrap().1;
    let bound = format!("grip = {{ entity = {}, bone = \"head\" }}", arm.to_bits() as i64);
    let spec = BehaviourSpec { fields: std::sync::Arc::new(toml::from_str(&bound).unwrap()), ..BehaviourSpec::new("Game.Look") };
    engine.world.insert(left, voxear_engine::EditBehaviours(vec![spec]));
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let left = saved.objs.iter().find(|o| o.name == "Left").unwrap();
    let instance = saved.objs.iter().find(|o| o.prefab.is_some()).unwrap();
    let expected = format!("v = {{ obj = {}, bone = \"head\" }}", instance.id);
    assert_eq!(left.fields["Game.Look"]["grip"], written(&expected));
}

#[test]
fn obj_references_stay_inside_their_file_and_save_as_paths() {
    let dir = project(&[("Lamp.prefab", LAMP)]);
    let mut content = Content::open(dir.path()).unwrap();
    let stray: scene_file::SceneFile = toml::from_str(
        "format = 1\n[[obj]]\nid = 1\nname = \"A\"\nbehaviours = [\"Game.X\"]\nfields.\"Game.X\" = { other = { obj = 9 } }\n",
    )
    .unwrap();
    let flat = prefab::expand(&mut content, &stray.objs).unwrap();
    let error = format!("{:#}", voxear_engine::Template::resolve(&mut content, None, &flat).unwrap_err());
    assert!(error.contains("refers to Obj 9, which is not in the same file"), "{error}");

    // A watcher pointing at the glow inside a lamp instance saves the reference as a path.
    let scene: scene_file::SceneFile =
        toml::from_str("format = 1\n[[obj]]\nid = 7\nname = \"Watcher\"\n[[obj]]\nid = 8\nprefab = \"Lamp.prefab\"\n")
            .unwrap();
    let mut engine = Engine::new();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let watcher = spawned[0].1;
    let glow = spawned.iter().find(|(path, _)| path.to_string() == "8/2").unwrap().1;
    let fields: toml::Table = toml::from_str(&format!("glow = {{ entity = {} }}", glow.to_bits() as i64)).unwrap();
    let spec = BehaviourSpec { fields: std::sync::Arc::new(fields), ..BehaviourSpec::new("Game.Watch") };
    engine.world.insert(watcher, Behaviours(vec![AttachedBehaviour { handle: BehaviourHandle(0), spec }]));

    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let watcher = saved.objs.iter().find(|o| o.name == "Watcher").unwrap();
    let instance = saved.objs.iter().find(|o| o.prefab.is_some()).unwrap();
    assert_eq!(
        watcher.fields["Game.Watch"]["glow"].to_string(),
        format!("{{ obj = \"{}/2\" }}", instance.id),
        "saved ids are renumbered, and the reference follows"
    );
}

const SLEEPER: &str = r#"
format = 1

[[obj]]
id = 1
name = "Sleeper"
behaviours = ["Game.Nap", "Game.Snore"]
enabled = { "Game.Nap" = false }
"#;

/// Whether the Obj's behaviour of this type starts enabled, as edit mode
/// keeps it.
fn starts_enabled(engine: &Engine, entity: Entity, type_name: &str) -> bool {
    let kept = &engine.world.get::<voxear_engine::EditBehaviours>(entity).unwrap().0;
    kept.iter().find(|spec| spec.type_name == type_name).unwrap().enabled
}

/// Ticks or unticks a behaviour as the Inspector does in edit mode.
fn tick(engine: &mut Engine, entity: Entity, type_name: &str, on: bool) {
    let kept = &mut engine.world.get_mut::<voxear_engine::EditBehaviours>(entity).unwrap().0;
    kept.iter_mut().find(|spec| spec.type_name == type_name).unwrap().enabled = on;
}

#[test]
fn unticked_behaviours_load_disabled_and_save_as_switched() {
    let dir = project(&[("Turret.prefab", TURRET), ("Sleeper.prefab", SLEEPER)]);
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    engine.set_edit_mode(true);
    let scene: scene_file::SceneFile = toml::from_str(
        r#"
format = 1
[[obj]]
id = 1
prefab = "Turret.prefab"
enabled = { "Game.Turret" = false }

[[obj]]
id = 2
name = "Mill"
behaviours = ["Game.Creak", "Game.Hum"]
enabled = { "Game.Hum" = false }

[[obj]]
id = 3
prefab = "Sleeper.prefab"
"#,
    )
    .unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let (turret, barrel, mill, sleeper) =
        (find(&engine, "Turret"), find(&engine, "Barrel"), find(&engine, "Mill"), find(&engine, "Sleeper"));
    assert!(!starts_enabled(&engine, turret, "Game.Turret"), "the instance unticks its prefab's behaviour");
    assert!(starts_enabled(&engine, barrel, "Game.Recoil"));
    assert!(starts_enabled(&engine, mill, "Game.Creak") && !starts_enabled(&engine, mill, "Game.Hum"));
    assert!(!starts_enabled(&engine, sleeper, "Game.Nap"), "as its prefab has it");

    tick(&mut engine, turret, "Game.Turret", true);
    tick(&mut engine, barrel, "Game.Recoil", false);
    tick(&mut engine, mill, "Game.Creak", false);
    tick(&mut engine, mill, "Game.Hum", true);
    tick(&mut engine, sleeper, "Game.Nap", true);
    let entities: Vec<Entity> = spawned.iter().map(|(_, entity)| *entity).collect();
    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let instance = |prefab: &str| saved.objs.iter().find(|o| o.prefab.as_deref() == Some(prefab)).unwrap();
    assert!(instance("Turret.prefab").enabled.is_empty(), "back as its prefab has it");
    assert_eq!(instance("Turret.prefab").overrides["2"]["enabled"].to_string(), r#"{ "Game.Recoil" = false }"#);
    assert_eq!(instance("Sleeper.prefab").enabled, BTreeMap::from([("Game.Nap".to_string(), true)]));
    let saved_mill = saved.objs.iter().find(|o| o.name == "Mill").unwrap();
    assert_eq!(saved_mill.enabled, BTreeMap::from([("Game.Creak".to_string(), false)]), "only what is off, by type");

    // And it loads back so.
    let path = dir.path().join("Copy.scene");
    scene_file::write(&path, &saved).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"Game.Creak\" = false") && !text.contains("\"Game.Hum\" ="), "{text}");
    let mut reloaded = Engine::new();
    reloaded.set_edit_mode(true);
    scene_file::spawn(&mut reloaded, &mut content, None, &scene_file::read(&path).unwrap(), &Default::default()).unwrap();
    let on = |name: &str, type_name: &str| starts_enabled(&reloaded, find(&reloaded, name), type_name);
    assert!(on("Turret", "Game.Turret") && !on("Barrel", "Game.Recoil"));
    assert!(!on("Mill", "Game.Creak") && on("Mill", "Game.Hum") && on("Sleeper", "Game.Nap"));
}

#[test]
fn an_instance_saves_what_it_switches_otherwise_than_its_prefab() {
    let dir = street();
    let mut content = Content::open(dir.path()).unwrap();
    let mut engine = Engine::new();
    let scene = scene_file::read(&dir.path().join("Scenes/Street.scene")).unwrap();
    let spawned = scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let red = find(&engine, "Red Lamp");
    let red_glow = engine
        .world
        .query_ref::<(Entity, &Name, &Parent)>()
        .into_iter()
        .find(|(_, n, p)| n.0 == "Glow" && p.0 == red)
        .unwrap()
        .0;
    voxear_scene::set_active(&mut engine.world, red, false);
    voxear_scene::set_component_on(&mut engine.world, red_glow, Switch::Light, false);

    let saved = scene_file::capture(&engine.world, &mut content, &entities);
    let instance = saved.objs.iter().find(|o| o.prefab.as_deref() == Some("Prefabs/RedLamp.prefab")).unwrap();
    assert_eq!(instance.active, Some(false));
    let glow = &instance.overrides["1/2"];
    assert_eq!(glow.get("enabled").unwrap().to_string(), "{ light = false }");
    let corner = saved.objs.iter().find(|o| o.name == "Corner Light").unwrap();
    assert!(corner.active.is_none() && !corner.overrides.values().any(|o| o.contains_key("enabled")));

    // And it loads back off.
    let path = dir.path().join("Scenes/Copy.scene");
    scene_file::write(&path, &saved).unwrap();
    let mut reloaded = Engine::new();
    let copy = scene_file::read(&path).unwrap();
    scene_file::spawn(&mut reloaded, &mut content, None, &copy, &Default::default()).unwrap();
    let red = find(&reloaded, "Red Lamp");
    assert!(!voxear_scene::is_active_self(&reloaded.world, red));
    let glows = reloaded.world.query_ref::<(Entity, &Name, &Parent)>().into_iter().filter(|(_, n, _)| n.0 == "Glow");
    let (lit, unlit): (Vec<_>, Vec<_>) =
        glows.partition(|(e, ..)| voxear_scene::is_component_on(&reloaded.world, *e, Switch::Light));
    assert_eq!((lit.len(), unlit.len()), (1, 1), "only the red lamp's glow is off");
}
