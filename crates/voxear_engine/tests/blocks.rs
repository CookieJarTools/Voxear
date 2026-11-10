//! A Block World in a scene: its blocks made from its `.blockworld` asset
//! as it loads, with or without a renderer, stood on by physics, and
//! written back as written.

use voxear_engine::{
    Engine, Game, Project,
    scene_file::{self, BlockWorldSource},
};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const FIELD: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                     [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n[[obj]]\nid = 2\nname = \"Ball\"\n\
                     position = [0.0, 20.0, 0.0]\n[obj.collider]\nshape = \"sphere\"\nradius = 0.5\n[obj.rigidbody]\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let files = [
        ("voxear.toml", "[project]\nname = \"Blocks\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn a_block_world_is_made_from_its_asset_stood_on_and_saved_as_written() {
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let named = |game: &Game, name: &str| {
        let world = &game.engine.world;
        world.query_ref::<(voxear_ecs::Entity, &voxear_scene::Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let (field, ball) = (named(&game, "Blocks"), named(&game, "Ball"));
    let blocks = game.engine.world.get::<BlockWorld>(field).expect("its blocks, without a renderer too");
    assert_eq!((blocks.blocks.get([0, 5, 0]), blocks.blocks.get([31, 0, 31]), blocks.blocks.get([0, 6, 0])), (1, 3, 0));
    assert_eq!(blocks.meshes.len(), 1, "one chunk of ground");
    // Dropped from 20 m, the ball comes to rest on the grass, 6 blocks up.
    for _ in 0..180 {
        game.tick(1.0 / 60.0, None);
    }
    let at = game.engine.world.get::<voxear_scene::Transform>(ball).unwrap().position;
    assert!((at.y - 6.5).abs() < 0.05, "on the blocks: {at}");
    let saved = scene_file::capture(&game.engine.world, &mut game.content, &[field]);
    let written = saved.objs[0].block_world.as_ref().expect("the world of blocks is saved");
    assert_eq!((written.data.as_str(), written.cast_shadows), ("Blocks/Field.blockworld", true));
    assert!(game.engine.world.get::<BlockWorldSource>(field).is_some());
}

#[test]
fn a_block_world_loads_with_the_changes_kept_beside_it() {
    use voxear_terrain::blocks::edits::BlockEdits;
    let dir = project();
    // A tower of stone on the grass, kept as changes from the ground.
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let base = file.make_ground();
    let mut built = base.clone();
    for y in 6..10 {
        built.set([5, y, 5], 3);
    }
    let edits = BlockEdits::between(&base, &built);
    std::fs::write(dir.path().join("Blocks/Field.blocks"), edits.to_bytes()).unwrap();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let blocks = game.engine.world.query_ref::<&BlockWorld>().into_iter().next().expect("the world").clone();
    assert_eq!([5, 6, 9, 10].map(|y| blocks.blocks.get([5, y, 5])), [1, 3, 3, 0]);
}
