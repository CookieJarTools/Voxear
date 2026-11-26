//! Trees felled in an endless Voxel Volume while a game runs: what a cut
//! through a trunk takes loose falls as a body, comes to rest on the
//! ground, and stays a body or is stamped back into the volume where it
//! lies; and cut grass fades (headless: no GPU, no scripts).

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    voxel_felling::{CutOptions, CutPieces, PieceFall, PieceRest, VoxelPieceBody},
    voxel_volumes::VoxelVolumeSource,
};
use voxear_render::{VoxelCut, VoxelFelled, VoxelVolume};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::plant::PlantDef;

/// A volume of its own gentle hills with oaks in most cells, its corner at the world's middle, a camera over it.
const SCENE: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Ground\"\n[obj.voxel_volume]\nsize = [16.0, 32.0, 16.0]\n\
                     endless = true\nreach = 32.0\n[obj.voxel_volume.ground]\nheight = 4.0\nhills = 0.5\n\
                     [[obj.voxel_volume.trees]]\nplant = \"Plants/Oak.voxplant\"\nper_hectare = 120.0\n\n[[obj]]\nid = 2\n\
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
    let trunks = voxear_render::voxel_tree_trunks(&volume, Vec3::new(-12.0, 0.0, -12.0), Vec3::new(28.0, 0.0, 28.0));
    // The one with the most room along +X, the way it falls, so no other tree holds it up.
    let room = |t: &Vec3| {
        let others = trunks.iter().filter(|o| *o != t);
        others.map(|o| if o.x > t.x && (o.z - t.z).abs() < 6.0 { o.x - t.x } else { 99.0 }).fold(99.0, f32::min)
    };
    let mut order: Vec<Vec3> = trunks.clone();
    order.sort_by(|a, b| room(b).total_cmp(&room(a)));
    // The first of them a cut takes loose: one grown into another stays up.
    for trunk in order {
        // An axe's cut: a slice across the trunk a metre up.
        let cut = VoxelCut::Slice { at: trunk + Vec3::Y, across: Vec3::new(0.2, 1.0, 0.0), radius: 0.9 };
        let started = std::time::Instant::now();
        let options = CutOptions { toward: Some(Vec3::X), ..CutOptions::default() };
        let done = voxear_engine::voxel_felling::cut(&mut game.engine.world, &cut, options, None);
        if done.pieces.is_empty() {
            continue;
        }
        let regions: usize = crate::volume(game).changes.regions().iter().map(|(_, bytes)| bytes.len()).sum();
        eprintln!(
            "felled in {:?}: {} voxels cut, {} columns, kept in {regions} bytes of region files",
            started.elapsed(),
            done.voxels,
            done.columns
        );
        return (trunk, done.pieces.iter().filter_map(|piece| piece.obj).collect());
    }
    panic!("no tree comes loose");
}

/// Plays until every piece has come to rest, or is gone, `most` seconds at most: how long it took.
fn play_until_rested(game: &mut Game, most: f32) -> f32 {
    let mut time = 0.0;
    while time < most {
        game.tick(1.0 / 60.0, None);
        time += 1.0 / 60.0;
        let falling =
            game.engine.world.query_ref::<&VoxelPieceBody>().into_iter().any(|body| body.fall != PieceFall::Resting);
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
    // It tips over its stump the way it was cut, never back over it.
    let (mut took, mut behind) = (0.0, f32::MAX);
    while took < 60.0 {
        game.tick(1.0 / 60.0, None);
        took += 1.0 / 60.0;
        let place = game.engine.world.get::<Transform>(piece).unwrap().position;
        behind = behind.min(place.x - start.x);
        if game.engine.world.get::<VoxelPieceBody>(piece).is_none_or(|body| body.fall == PieceFall::Resting) {
            break;
        }
    }
    assert!(behind > -0.3, "it swung back from where it stood: {behind:.2} m");
    let body = game.engine.world.get::<VoxelPieceBody>(piece).expect("it stays a body").clone();
    let place = game.engine.world.get::<Transform>(piece).unwrap().clone();
    let up = place.rotation * Vec3::Y;
    eprintln!("rested after {took:.1} s at {} (from {start}), its up now {up}", place.position);
    assert_eq!(body.fall, PieceFall::Resting, "at rest within 60 s");
    // Well over, onto the ground or its crown, its top the way it was cut.
    assert!(up.y < 0.85 && up.x > 0.5, "fallen over the way it was cut, not standing: up {up}");
    assert!(place.position.x > trunk.x + 1.0, "fallen along +X: {} from the trunk at {trunk}", place.position);
    // On the ground: its logs' middle, which is in its crown, over the ground by about the crown's half, not through it.
    let under = voxear_render::voxel_ground_at(&volume(&game), place.position.x, place.position.z).expect("ground").0;
    assert!(
        place.position.y > under - 0.5 && place.position.y < under + 6.0,
        "{} over ground at {under}",
        place.position.y
    );
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
    let body = game.engine.world.get::<VoxelPieceBody>(pieces[0]).unwrap().clone();
    let voxels = body.piece.voxels.len();
    // Its voxels a volume of their own under it, which turns with it.
    let look = game.engine.world.get::<VoxelVolume>(body.look).expect("its voxels' volume").clone();
    assert!(!look.endless && look.stamps.len() == 1, "{look:?}");
    assert_eq!(game.engine.world.get::<voxear_scene::Parent>(body.look).map(|p| p.0), Some(pieces[0]));
    let took = play_until_rested(&mut game, 60.0);
    eprintln!("stamped after {took:.1} s");
    assert!(game.engine.world.get::<VoxelPieceBody>(pieces[0]).is_none(), "the piece's Obj is gone");
    let after = volume(&game).changes.count();
    let placed = after.saturating_sub(before) as usize;
    assert!(placed * 2 > voxels, "its voxels placed in the volume: {placed} of {voxels}");
    // Its own volume is drawn a moment longer, while the ground's is made again, then let go.
    for _ in 0..90 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(game.engine.world.get::<VoxelVolume>(body.look).is_none(), "its own voxels let go");
}

#[test]
fn a_cut_takes_its_pieces_away_or_says_what_they_become_at_rest_and_a_game_stamps_one_when_it_chooses() {
    let (_dir, mut game) = game(VoxelFelled::Body);
    let volume = volume(&game);
    let trunks = voxear_render::voxel_tree_trunks(&volume, Vec3::new(-16.0, 0.0, -16.0), Vec3::new(32.0, 0.0, 32.0));
    let mut fell = |game: &mut Game, options: CutOptions| {
        trunks
            .iter()
            .map(|trunk| {
                let cut = VoxelCut::Slice { at: *trunk + Vec3::Y, across: Vec3::Y, radius: 0.9 };
                voxear_engine::voxel_felling::cut(&mut game.engine.world, &cut, options, None)
            })
            .find(|done| !done.pieces.is_empty())
            .expect("a tree comes loose")
    };
    // Taken away at once: no Obj, but where it was and how much wood it had, for the game's own logs.
    let gone = fell(&mut game, CutOptions { pieces: CutPieces::Gone, ..CutOptions::default() });
    let piece = gone.pieces[0];
    assert!(piece.obj.is_none() && piece.logs > 1000 && piece.leaves > 1000, "{piece:?}");
    assert!(piece.size.y > 3.0, "a tree's height: {piece:?}");
    // Taken away at rest, whatever the volume says.
    let removed = fell(&mut game, CutOptions { rest: Some(PieceRest::Remove), ..CutOptions::default() });
    let removed = removed.pieces[0].obj.expect("a body while it falls");
    // Left to the game: it asks, and stamps it once at rest.
    let kept = fell(&mut game, CutOptions::default());
    let kept = kept.pieces[0].obj.expect("a body");
    assert!(!voxear_engine::voxel_felling::piece(&game.engine.world, kept).unwrap().resting);
    play_until_rested(&mut game, 60.0);
    assert!(game.engine.world.get::<VoxelPieceBody>(removed).is_none(), "taken away at rest");
    let resting = voxear_engine::voxel_felling::piece(&game.engine.world, kept).expect("still a body");
    assert!(resting.resting, "{resting:?}");
    let before = crate::volume(&game).changes.count();
    assert!(voxear_engine::voxel_felling::stamp_piece(&mut game.engine.world, kept));
    assert!(crate::volume(&game).changes.count() > before + 1000, "stamped where it lies");
    assert!(voxear_engine::voxel_felling::piece(&game.engine.world, kept).is_none());
}

#[test]
fn how_trees_fall_and_grass_grows_back_are_set_for_every_volume_and_kept_for_its_scene() {
    use voxear_engine::voxel_volumes::{cutting, felling, set_felled, set_felling, set_regrow};
    use voxear_render::VoxelFelling;
    let (_dir, mut game) = game(VoxelFelled::Body);
    let world = &mut game.engine.world;
    assert_eq!(set_felled(world, VoxelFelled::Stamped, None), 1);
    assert_eq!(set_regrow(world, -5.0, None), 1, "under none is never");
    let slow = VoxelFelling { push: 0.2, wood_density: 900.0, clippings: false, ..VoxelFelling::default() };
    assert_eq!(set_felling(world, VoxelFelling { push: f32::NAN, ..slow }, None), 1);
    assert_eq!(cutting(world), Some((VoxelFelled::Stamped, 0.0)));
    assert_eq!(felling(world), Some(VoxelFelling { push: 0.6, ..slow }), "a push of no number is the usual one");
    let ground = named(&game, "Ground");
    let data = game.engine.world.get::<VoxelVolumeSource>(ground).unwrap().0.clone();
    assert_eq!((data.felled, data.regrow, data.felling.clippings), (VoxelFelled::Stamped, 0.0, false));
    // Cut grass with no clippings plays none, and says where it was cut.
    let effects = |game: &Game| game.engine.world.query_ref::<&voxear_render::ParticleSystem>().into_iter().count();
    let before = effects(&game);
    let trunks = voxear_render::voxel_tree_trunks(&volume(&game), Vec3::splat(-12.0), Vec3::splat(28.0));
    let open = trunks[0] + Vec3::new(2.5, 0.2, 0.0);
    let done = voxear_engine::voxel_felling::cut(
        &mut game.engine.world,
        &VoxelCut::Ball { at: open, radius: 0.6 },
        CutOptions::default(),
        None,
    );
    assert!(done.columns > 0 && !done.mown.is_empty(), "{done:?}");
    assert_eq!(effects(&game), before, "no clippings");
}
