//! What an instance changes from its prefab, put back from the prefab or
//! written onto it, headless (no GPU, no scripts).

use voxear_ecs::Entity;
use voxear_engine::{
    Content, Engine,
    prefab_overrides::{self, ValuePath},
    scene_file,
};
use voxear_render::Light;
use voxear_scene::{Name, Transform};

const LAMP: &str = r#"
format = 1

[[obj]]
id = 1
name = "Lamp"
[obj.light]
type = "point"
color = [1.0, 1.0, 1.0]
intensity = 4.0

[[obj]]
id = 2
name = "Bulb"
parent = 1
position = [0.0, 1.0, 0.0]
"#;

const STREET: &str = r#"
format = 1

[[obj]]
id = 1
name = "Red Lamp"
prefab = "Lamp.prefab"
position = [4.0, 0.0, 0.0]

[[obj]]
id = 2
name = "Plain Lamp"
prefab = "Lamp.prefab"

[[obj]]
id = 3
name = "Bright Lamp"
prefab = "Lamp.prefab"
[obj.light]
type = "point"
color = [1.0, 1.0, 1.0]
intensity = 9.0
"#;

fn named(engine: &Engine, name: &str) -> Entity {
    let all = engine.world.query_ref::<(Entity, &Name)>();
    all.into_iter().find(|(_, n)| n.0 == name).map(|(e, _)| e).unwrap_or_else(|| panic!("no {name}"))
}

fn light(engine: &Engine, name: &str) -> Light {
    *engine.world.get::<Light>(named(engine, name)).unwrap()
}

#[test]
fn an_instances_changes_are_listed_reverted_and_applied_to_its_prefab() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Lamp.prefab"), LAMP).unwrap();
    std::fs::write(dir.path().join("Street.scene"), STREET).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Street.scene")).unwrap();
    let mut engine = Engine::new();
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();

    // Placing an instance is not a change; its light's colour and a child's move are.
    let red = named(&engine, "Red Lamp");
    assert!(prefab_overrides::overrides(&engine.world, &mut content, red).is_empty(), "placed, named, unchanged");
    engine.world.get_mut::<Light>(red).unwrap().color = [1.0, 0.2, 0.2].into();
    let bulb = prefab_overrides::members(&engine.world, red)
        .into_iter()
        .find(|(path, _)| !path.is_root())
        .map(|(_, entity)| entity)
        .unwrap();
    engine.world.get_mut::<Transform>(bulb).unwrap().position.y = 1.5;
    let color = ValuePath::new(&["light", "color"]);
    assert_eq!(prefab_overrides::overrides(&engine.world, &mut content, red), [color.clone()]);
    assert_eq!(prefab_overrides::overrides(&engine.world, &mut content, bulb), [ValuePath::new(&["position"])]);
    let all = prefab_overrides::instance_overrides(&engine.world, &mut content, red);
    assert_eq!(all.len(), 2, "the root and its bulb: {all:?}");

    // Revert puts the prefab's value back, and only that.
    prefab_overrides::revert(&mut engine.world, &mut content, None, bulb, &[ValuePath::new(&["position"])]).unwrap();
    assert_eq!(engine.world.get::<Transform>(bulb).unwrap().position.y, 1.0);
    assert_eq!(light(&engine, "Red Lamp").color.y, 0.2, "the other change stays");

    // Apply writes it into the prefab, and the lamps that kept the prefab's colour take it.
    let bright = named(&engine, "Bright Lamp");
    let others: Vec<Entity> = prefab_overrides::instances_of(&engine.world, content.db().guid_of("Lamp.prefab").unwrap())
        .into_iter()
        .filter(|&root| root != red)
        .collect();
    let kept = others
        .iter()
        .flat_map(|&root| prefab_overrides::instance_overrides(&engine.world, &mut content, root))
        .collect();
    let path = prefab_overrides::apply(&engine.world, &mut content, &[(red, vec![color.clone()])]).unwrap();
    assert_eq!(path, "Lamp.prefab");
    let written = std::fs::read_to_string(dir.path().join("Lamp.prefab")).unwrap();
    assert!(written.contains("color = [1.0, 0.4845, 0.4845]"), "the prefab has the red, in sRGB: {written}");
    let applied = [(voxear_engine::prefab::ObjPath::default(), vec![color.clone()])];
    prefab_overrides::follow_prefab(&mut engine.world, &mut content, None, &applied, &kept, &others).unwrap();
    let red_in = |name: &str| (light(&engine, name).color.y - 0.2).abs() < 1e-3;
    assert!(red_in("Plain Lamp"), "an unchanged lamp follows its prefab");
    assert!(red_in("Bright Lamp"), "the colour reaches a lamp that changed something else");
    assert_eq!(light(&engine, "Bright Lamp").intensity, 9.0, "and its own change stays");
    assert!(prefab_overrides::overrides(&engine.world, &mut content, red).is_empty(), "nothing left to apply");
    assert_eq!(
        prefab_overrides::overrides(&engine.world, &mut content, bright),
        [ValuePath::new(&["light", "intensity"])]
    );
}
