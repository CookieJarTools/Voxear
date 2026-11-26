//! Trees felled in an endless Voxel Volume while a game runs: what a cut
//! through a trunk takes loose falls as a body, comes to rest on the
//! ground, and stays a body or is stamped back into the volume where it
//! lies; and cut grass fades (headless: no GPU, no scripts).

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    voxel_felling::{CutPieces, PieceFall, VoxelPieceBody},
    voxel_volumes::VoxelVolumeSource,
};
use voxear_render::{VoxelCut, VoxelFelled, VoxelVolume};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::plant::PlantDef;

/// A volume of its own gentle hills with oaks in most cells, its corner at the world's middle, a camera over it.
const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.voxel_volume]\nsize = [16.0, 32.0, 16.0]\n\
                     endless = true\nreach = 32.0\n[obj.voxel_volume.ground]\nheight = 4.0\nhills = 0.5\n\
                     [[obj.voxel_volume.trees]]\nplant = \"Plants/Oak.voxplant\"\nper_hectare = 300.0\n\n[[obj]]\nid = 2\n\
                     name = \"Eye\"\nposition = [8.0, 20.0, 8.0]\n[obj.camera]\n";

fn game(felled: VoxelFelled) -> (tempfile::TempDir, Game) {
    let dir = tempfile::tempdir().unwrap();
    let oak = PlantDef { height: [8.0, 10.0], crown_width: [6.0, 8.0], ..PlantDef::default() };
    let files = [
        ("voxear.toml", "[project]\nname = \"Wood\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Wood.scene\"\n".to_string()),
        ("Scenes/Wood.scene", SCENE.to_string()),
        ("Plants/Oak.voxplant", oak.to_text()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let ground = named(&game, "Ground");
    let mut data = game.engine.world.get::<VoxelVolumeSource>(ground).unwrap().0.clone();
    data.felled = felled;
    data.regrow = 30.0;
    voxear_engine::voxel_volumes::set(&mut game.content, &mut game.engine.world, ground, data);
    (dir, game)
}

fn named(game: &Game, name: &str) -> Entity {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

fn volume(game: &Game) -> VoxelVolume {
    game.engine.world.get::<VoxelVolume>(named(game, "Ground")).unwrap().clone()
}

/// Cuts through the trunk of a tree near the camera, to fall along +X: the trunk's foot, and the pieces.
fn fell_one(game: &mut Game) -> (Vec3, Vec<Entity>) {
    let volume = volume(game);
    let trunks = voxear_render::voxel_tree_trunks(&volume, Vec3::new(-4.0, 0.0, -4.0), Vec3::new(20.0, 0.0, 20.0));
    let trunk = *trunks.first().expect("a tree near the camera");
    // An axe's cut: a slice across the trunk a metre up.
    let cut = VoxelCut::Slice { at: trunk + Vec3::Y, across: Vec3::new(0.2, 1.0, 0.0), radius: 0.9 };
    let done = voxear_engine::voxel_felling::cut(&mut game.engine.world, &cut, Some(Vec3::X), CutPieces::Fall, None);
    assert!(done.voxels > 0, "{done:?}");
    assert!(!done.pieces.is_empty(), "the tree comes loose");
    (trunk, done.pieces)
}

/// Plays until every piece has come to rest, or is gone, `most` seconds at most: how long it took.
fn play_until_rested(game: &mut Game, most: f32) -> f32 {
    let mut time = 0.0;
    while time < most {
        game.tick(1.0 / 60.0, None);
        time += 1.0 / 60.0;
        let falling = game
            .engine
            .world
            .query_ref::<&VoxelPieceBody>()
            .into_iter()
            .any(|body| body.fall != PieceFall::Resting);
        if !falling {
            break;
        }
    }
    time
}

#[test]
fn a_tree_cut_through_its_trunk_falls_the_way_it_was_cut_and_comes_to_rest_on_the_ground() {
    let (_dir, mut game) = game(VoxelFelled::Body);
    let (trunk, pieces) = fell_one(&mut game);
    let piece = pieces[0];
    let start = game.engine.world.get::<Transform>(piece).unwrap().position;
    let took = play_until_rested(&mut game, 20.0);
    let body = game.engine.world.get::<VoxelPieceBody>(piece).expect("it stays a body").clone();
    let place = game.engine.world.get::<Transform>(piece).unwrap().clone();
    let up = place.rotation * Vec3::Y;
    eprintln!("rested after {took:.1} s at {} (from {start}), its up now {up}", place.position);
    assert_eq!(body.fall, PieceFall::Resting, "at rest within 20 s");
    assert!(up.y < 0.5, "lying down, not standing: up {up}");
    assert!(place.position.x > trunk.x + 1.0, "fallen along +X: {} from the trunk at {trunk}", place.position);
    // On the ground: its logs' middle a little over the ground's top under it, not through it.
    let under = voxear_render::voxel_ground_at(&volume(&game), place.position.x, place.position.z).expect("ground").0;
    assert!(place.position.y > under - 0.5 && place.position.y < under + 3.0, "{} over ground at {under}", place.position.y);
    // Pushed, it moves again: still a body of its own.
    assert!(game.engine.world.get::<voxear_engine::physics::Rigidbody>(piece).is_some_and(|rb| !rb.kinematic));
    // The stump stands, the volume keeps the tree as felled, and the grass about it fades.
    let volume = volume(&game);
    assert_eq!(volume.changes.all_felled().count(), 1);
    let stump = voxear_render::voxel_raycast(&volume, Vec3::ZERO, trunk + Vec3::Y * 0.2 - Vec3::X * 3.0, Vec3::X, 4.0);
    assert!(stump.is_some_and(|hit| !hit.cover), "a ray across meets the stump");
}

#[test]
fn a_felled_tree_stamped_back_where_it_rests_is_the_volumes_voxels_again() {
    let (_dir, mut game) = game(VoxelFelled::Stamped);
    let (_, pieces) = fell_one(&mut game);
    let before = volume(&game).changes.count();
    let voxels = game.engine.world.get::<VoxelPieceBody>(pieces[0]).unwrap().piece.voxels.len();
    let took = play_until_rested(&mut game, 20.0);
    eprintln!("stamped after {took:.1} s");
    assert!(game.engine.world.get::<VoxelPieceBody>(pieces[0]).is_none(), "the piece's Obj is gone");
    let after = volume(&game).changes.count();
    let placed = after.saturating_sub(before) as usize;
    assert!(placed * 2 > voxels, "its voxels placed in the volume: {placed} of {voxels}");
}
