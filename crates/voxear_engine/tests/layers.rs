//! Layers from a project's `voxear.toml` through scene files and prefabs:
//! an Obj's layer named in its file, read into the world by number, and
//! written back by name, or by number for a layer with none.

use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scene_file};
use voxear_scene::{Layer, Layers, Name};

const PROJECT: &str = "[project]\nname = \"Layered\"\n\n[layers]\nnames = [\"Default\", \"\", \"Player\", \"Enemy\"]\n";

const SCENE: &str = r#"
format = 1

[[obj]]
id = 1
name = "Hero"
layer = "Player"

[[obj]]
id = 2
name = "Ghost"
layer = 9

[[obj]]
id = 3
name = "Stranger"
layer = "Nobody"

[[obj]]
id = 4
name = "Grunt"
prefab = "Prefabs/Grunt.prefab"
layer = "Player"
"#;

const GRUNT: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Grunt\"\nlayer = \"Enemy\"\n";

fn find(game: &Game, name: &str) -> Entity {
    let found = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name);
    found.unwrap_or_else(|| panic!("no Obj named {name}")).0
}

#[test]
fn objs_are_on_the_layers_their_files_name() {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [("voxear.toml", PROJECT), ("Scenes/Yard.scene", SCENE), ("Prefabs/Grunt.prefab", GRUNT)] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    assert_eq!(game.engine.world.resource::<Layers>().unwrap().find("Enemy"), Some(3), "the project's layers");
    let scene = scene_file::read(&dir.path().join("Scenes/Yard.scene")).unwrap();
    let spawned = scene_file::spawn(&mut game.engine, &mut game.content, None, &scene, &Default::default()).unwrap();
    let layer = |game: &Game, name: &str| game.engine.world.get::<Layer>(find(game, name)).copied();
    assert_eq!(layer(&game, "Hero"), Some(Layer(2)));
    assert_eq!(layer(&game, "Ghost"), Some(Layer(9)), "a layer with no name, by number");
    assert_eq!(layer(&game, "Stranger"), None, "a name the project lacks is Default");
    assert_eq!(layer(&game, "Grunt"), Some(Layer(2)), "an instance's layer over its prefab's");

    // Saved, each is written by its name, or its number; the instance keeps its own.
    game.engine.world.insert(find(&game, "Hero"), Layer(3));
    let entities: Vec<Entity> = spawned.iter().map(|(_, e)| *e).collect();
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &entities);
    let text = toml::to_string(&saved).unwrap();
    assert!(text.contains("layer = \"Enemy\"") && text.contains("layer = 9"), "{text}");
    let grunt = saved.objs.iter().find(|o| o.name == "Grunt" || o.prefab.is_some()).unwrap();
    assert_eq!(grunt.layer, Some(scene_file::LayerRef::Name("Player".into())), "{text}");
}
