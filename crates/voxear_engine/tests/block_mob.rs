//! A Block Mob in a level with a world of blocks: it stands on the blocks,
//! walks to where it is told a block at a time, climbs a step a block
//! high, wanders no further than it is let, and its Mob Motion animates it
//! as it goes (headless: no GPU, no scripts).

use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, block_mob::BlockMob};
use voxear_render::{BoneRole, BoxModel, Pose};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

/// A field of blocks 32 across, six deep, its corner at -16 so its middle is the world's, and a walker on it.
const FIELD: &str = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                     [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n[[obj]]\nid = 2\nname = \"Walker\"\n\
                     position = [0.5, 6.3, 0.5]\n[obj.model]\nmodel = \"Mobs/Walker.boxmodel\"\n[obj.mob_motion]\n\
                     [obj.character]\n[obj.block_mob]\nspeed = 3.0\n";

/// A body on two legs with a head, every bone saying what it is.
fn walker() -> String {
    let mut model = BoxModel::default();
    let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
    model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
    let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
    model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
    for (name, x) in [("RightLeg", 2.0), ("LeftLeg", -2.0)] {
        let leg = model.add_bone(name, Some(body), Vec3::new(x, 0.0, 0.0));
        model.add_box(leg, Vec3::new(-2.0, -12.0, -2.0), [4, 12, 4], [0, 16]);
        model.bones[leg].role = BoneRole::Leg;
    }
    (model.bones[body].role, model.bones[head].role) = (BoneRole::Body, BoneRole::Head);
    model.to_text()
}

fn game() -> (tempfile::TempDir, Game, Entity, Entity) {
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let files = [
        ("voxear.toml", "[project]\nname = \"Mobs\"\nstart_level = \"Levels/Main.level\"\n".to_string()),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Blocks/Field.blockworld", file.to_text()),
        ("Mobs/Walker.boxmodel", walker()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let named = |game: &Game, name: &str| {
        game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
    };
    let (field, walker) = (named(&game, "Blocks"), named(&game, "Walker"));
    // It comes to rest on the blocks.
    for _ in 0..60 {
        game.tick(1.0 / 60.0, None);
    }
    (dir, game, field, walker)
}

fn at(game: &Game, entity: Entity) -> Vec3 {
    game.engine.world.get::<Transform>(entity).unwrap().position
}

/// Ticks until the mob says it is where it was sent, or `most` frames have gone: how many went.
fn until_arrived(game: &mut Game, mob: Entity, most: usize) -> usize {
    for frame in 0..most {
        game.tick(1.0 / 60.0, None);
        if game.engine.world.get::<BlockMob>(mob).is_some_and(|mob| mob.arrived) {
            return frame + 1;
        }
    }
    most
}

#[test]
fn a_block_mob_stands_on_the_blocks_and_walks_to_where_it_is_sent_animated_as_it_goes() {
    let (_dir, mut game, _, walker) = game();
    let start = at(&game, walker);
    assert!((start.y - 6.0).abs() < 0.1, "standing on the field, six blocks up: {start}");
    assert!(game.engine.world.get::<BlockMob>(walker).unwrap().arrived, "with nowhere to go");
    assert!((start.x - 0.5).abs() < 0.05 && (start.z - 0.5).abs() < 0.05, "it stays where it is put");

    let to = Vec3::new(6.5, 6.0, 4.5);
    game.engine.world.get_mut::<BlockMob>(walker).unwrap().move_to(to);
    // Its legs swing as it goes: the right foot's bone leaves where it stands at rest.
    let mut swung = 0.0f32;
    let mut frames = 0;
    while frames < 600 && !(frames > 0 && game.engine.world.get::<BlockMob>(walker).unwrap().arrived) {
        game.tick(1.0 / 60.0, None);
        frames += 1;
        if let Some(pose) = game.engine.world.get::<Pose>(walker) {
            swung = swung.max(pose.bones[2].transform_point3(Vec3::new(0.0, -0.75, 0.0)).z.abs());
        }
    }
    let now = at(&game, walker);
    assert!(frames < 600, "it got there: at {now} after {frames} frames");
    assert!((now.x - to.x).abs() < 0.3 && (now.z - to.z).abs() < 0.3, "{now}");
    // Seven and a half metres or so a block at a time, at three metres a second.
    assert!(frames > 120, "on foot, not at once: {frames} frames");
    assert!(swung > 0.2, "its Mob Motion walks it: {swung}");
    let facing = game.engine.world.get::<Transform>(walker).unwrap().rotation * Vec3::Z;
    assert!(facing.y.abs() < 1e-3, "upright, facing the way it went: {facing}");
}

#[test]
fn a_block_mob_climbs_a_step_one_block_high_and_goes_round_one_too_high() {
    let (_dir, mut game, field, walker) = game();
    // A platform one block up, east of it, and a wall three blocks high along its north side with a gap.
    let mut changes = Vec::new();
    for x in 20..26 {
        for z in 14..19 {
            changes.push(([x, 6, z], 1));
        }
    }
    for x in 12..22 {
        for y in 6..9 {
            changes.push(([x, y, 21], 1));
        }
    }
    game.engine.world.get_mut::<BlockWorld>(field).unwrap().set_blocks(changes);
    game.tick(1.0 / 60.0, None);

    // Onto the platform: a block higher than it started.
    game.engine.world.get_mut::<BlockMob>(walker).unwrap().move_to(Vec3::new(6.5, 7.0, 0.5));
    let frames = until_arrived(&mut game, walker, 900);
    let now = at(&game, walker);
    assert!(frames < 900, "it got onto the platform: {now}");
    assert!((now.y - 7.0).abs() < 0.15 && (now.x - 6.5).abs() < 0.4, "{now}");

    // To the far side of the wall: round its end, never over it.
    game.engine.world.get_mut::<BlockMob>(walker).unwrap().move_to(Vec3::new(0.5, 6.0, 8.5));
    let mut highest = 0.0f32;
    let mut frames = 0;
    while frames < 1500 && !(frames > 0 && game.engine.world.get::<BlockMob>(walker).unwrap().arrived) {
        game.tick(1.0 / 60.0, None);
        frames += 1;
        highest = highest.max(at(&game, walker).y);
    }
    let now = at(&game, walker);
    assert!(frames < 1500, "it found the way round: {now}");
    assert!((now.x - 0.5).abs() < 0.4 && (now.z - 8.5).abs() < 0.4, "{now}");
    assert!(highest < 7.3, "it never climbed the wall: {highest}");
}

#[test]
fn a_block_mob_set_to_wander_goes_here_and_there_and_no_further_than_it_may() {
    let (_dir, mut game, _, walker) = game();
    let home = at(&game, walker);
    game.engine.world.get_mut::<BlockMob>(walker).unwrap().wander(4.0);
    let (mut furthest, mut travelled, mut last) = (0.0f32, 0.0f32, home);
    for _ in 0..1800 {
        game.tick(1.0 / 60.0, None);
        let now = at(&game, walker);
        furthest = furthest.max((now - home).length());
        travelled += (now - last).length();
        last = now;
    }
    assert!(travelled > 3.0, "it went somewhere in half a minute: {travelled} m");
    assert!(furthest < 4.0 * 1.5 + 1.0, "and stayed near where it began: {furthest} m");
    // Told to stop, it stands.
    game.engine.world.get_mut::<BlockMob>(walker).unwrap().stop();
    for _ in 0..30 {
        game.tick(1.0 / 60.0, None);
    }
    let stood = at(&game, walker);
    for _ in 0..60 {
        game.tick(1.0 / 60.0, None);
    }
    assert!((at(&game, walker) - stood).length() < 0.05);
}
