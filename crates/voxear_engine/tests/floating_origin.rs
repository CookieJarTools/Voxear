//! The floating origin (see `voxear_engine::origin`): far from the level's
//! middle the world moves back towards the camera by whole metres, and
//! things move there as they do near the middle, ground made round the
//! camera carries on, and nothing kept is made again.

use std::sync::Arc;

use voxear_core::math::{DVec3, IVec3, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Content, Engine, Game, Project,
    origin::{OriginMode, OriginSettings},
    scene_file,
};
use voxear_scene::{Name, Transform, WorldOrigin};

fn find(world: &voxear_ecs::World, name: &str) -> Entity {
    world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).unwrap().0
}

fn write(dir: &std::path::Path, files: &[(&str, String)]) {
    for (path, text) in files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// A ramp tilted 15° down towards +x, and a ball dropped on its high end
/// to roll down it, `x` metres along from the level's middle.
fn ramp(x: f32) -> String {
    format!(
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Ramp\"\nposition = [{x}, 0.0, 0.0]\nrotation = [0.0, 0.0, -15.0]\n\
         collider = {{ size = [40.0, 1.0, 6.0] }}\n\n[[obj]]\nid = 2\nname = \"Ball\"\nposition = [{}, 6.0, 0.3]\n\
         collider = {{ shape = \"sphere\", radius = 0.5 }}\nrigidbody = {{ mass = 2.0 }}\n",
        x - 14.0
    )
}

/// The ball's path down the ramp `x` metres out, a place a frame from the
/// level's middle, less `x`; with `follow`, the world's middle moves with
/// the ball as it says. Gives how many times it moved, too.
fn roll(x: f32, follow: Option<OriginSettings>) -> (Vec<DVec3>, usize) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Ramp.scene"), ramp(x)).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let scene = scene_file::read(&dir.path().join("Ramp.scene")).unwrap();
    let mut engine = Engine::new();
    scene_file::spawn(&mut engine, &mut content, None, &scene, &Default::default()).unwrap();
    let ball = find(&engine.world, "Ball");
    let (mut path, mut moves) = (Vec::new(), 0);
    for _ in 0..150 {
        let at = engine.world.get::<Transform>(ball).unwrap().position;
        if let Some(by) = follow.as_ref().and_then(|settings| settings.move_for(at)) {
            engine.move_origin(by);
            moves += 1;
        }
        engine.tick(1.0 / 60.0);
        let at = engine.world.get::<Transform>(ball).unwrap().position;
        path.push(WorldOrigin::of(&engine.world).to_level(at) - DVec3::new(f64::from(x), 0.0, 0.0));
    }
    (path, moves)
}

fn farthest_apart(a: &[DVec3], b: &[DVec3]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a.distance(*b)).fold(0.0, f64::max)
}

#[test]
fn a_body_far_out_moves_as_smoothly_as_near_the_middle() {
    let (near, _) = roll(0.0, None);
    assert!(near[149].x - near[0].x > 2.0, "the ball rolled down the ramp: {} to {}", near[0], near[149]);
    // 100 km out, the world's middle moving with the ball every metre it rolls.
    let every_metre = OriginSettings { mode: OriginMode::Always, distance: 1.0, step: 1 };
    let (far, moves) = roll(100_000.0, Some(every_metre));
    assert!(moves >= 3, "the middle moved as it rolled: {moves} times");
    let apart = farthest_apart(&near, &far);
    assert!(apart < 1e-3, "it rolled as it does near the middle, at most {apart} m off");
    // With the middle left where it is, f32 counts 100 km out in steps of 8 mm.
    let (still, _) = roll(100_000.0, None);
    let off = farthest_apart(&near, &still);
    assert!(off > apart * 10.0 && off > 2e-3, "the middle left still, it went {off} m astray");
}

/// A terrain without end made by a generator, 4 m between samples, and a
/// camera over it 5 km from the level's middle.
fn endless_terrain() -> tempfile::TempDir {
    use voxear_terrain::{
        file::{Endless, TerrainFile},
        generate::Preset,
        layers::{Layer, LayerKind},
    };
    let dir = tempfile::tempdir().unwrap();
    let generate = LayerKind::Generate { world: Some("Worlds/Hills.worldgen".into()), position: [100.0, 0.0] };
    let file = TerrainFile {
        layers: vec![Layer::new(generate)],
        endless: Some(Endless { distance: 900.0, spacing: 4.0 }),
        ..TerrainFile::default()
    };
    let scene = "format = 1\n\n[[obj]]\nid = 1\nname = \"World\"\n[obj.terrain]\ndata = \"Terrains/World.terrain\"\n\n\
                 [[obj]]\nid = 2\nname = \"Eye\"\nposition = [5000.0, 300.0, -3000.0]\n[obj.camera]\nfov = 60.0\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Far\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/World.scene\"\n".into()),
            ("Scenes/World.scene", scene.into()),
            ("Terrains/World.terrain", file.to_text()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
    dir
}

/// Each tile of a terrain without end's window, by where it is from its
/// Obj, and what it was made as.
fn tiles(game: &Game, terrain: Entity) -> std::collections::HashMap<[i32; 2], Arc<voxear_terrain::HeightTile>> {
    let terrain = game.engine.world.get::<voxear_terrain::Terrain>(terrain).unwrap();
    let [ox, oz] = terrain.origin.unwrap();
    let across = terrain.heights.tiles_x() as i32;
    let made = terrain.heights.tiles().iter().enumerate().filter(|(_, tile)| !voxear_terrain::endless::is_unmade(tile));
    made.map(|(index, tile)| ([ox + index as i32 % across, oz + index as i32 / across], tile.clone())).collect()
}

#[test]
fn ground_without_end_carries_on_as_the_middle_moves_with_the_camera() {
    use voxear_terrain::generate::{Preset, WorldGen};
    let dir = endless_terrain();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let hills = WorldGen::parse(&Preset::RollingHills.world().to_text()).unwrap();
    let (terrain, eye) = (find(&game.engine.world, "World"), find(&game.engine.world, "Eye"));
    // The first frame moves the middle to the camera, by whole kilometres, before the ground is made.
    game.tick(1.0 / 60.0, None);
    let origin = WorldOrigin::of(&game.engine.world);
    assert_eq!(origin.0.to_array(), [5120, 0, -3072]);
    assert_eq!(game.engine.world.get::<Transform>(eye).unwrap().position, Vec3::new(-120.0, 300.0, 72.0));
    let ground = voxear_terrain::ground_at(&game.engine.world, -120.0, 72.0).expect("ground under the camera");
    assert!((ground.height - hills.height_at(5100.0, -3000.0)).abs() < 0.05, "{} as made", ground.height);
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(voxear_engine::terrain::streamed(&game.engine.world));

    // Flown 5 km on, 100 m a frame: the middle moves twice, and no tile kept from one frame to the next is made again.
    let mut moves = 0;
    for _ in 0..50 {
        let (kept, was) = (tiles(&game, terrain), WorldOrigin::of(&game.engine.world));
        game.engine.world.get_mut::<Transform>(eye).unwrap().position.x += 100.0;
        game.tick(1.0 / 60.0, None);
        moves += usize::from(WorldOrigin::of(&game.engine.world) != was);
        for (place, tile) in tiles(&game, terrain) {
            assert!(kept.get(&place).is_none_or(|before| Arc::ptr_eq(before, &tile)), "tile {place:?} made again");
        }
        // Physics stands things on the ground where it is drawn, across the moves too.
        let at = game.engine.world.get::<Transform>(eye).unwrap().position;
        let physics = game.engine.physics().expect("physics");
        let hit = physics.raycast(at, Vec3::NEG_Y, 1000.0, u32::MAX, false).expect("ground under the camera");
        let drawn = voxear_terrain::ground_at(&game.engine.world, at.x, at.z).unwrap().height;
        assert!((hit.point.y - drawn).abs() < 0.05, "physics' ground at {} and the drawn at {drawn}", hit.point.y);
    }
    assert_eq!(moves, 2);
    let level = WorldOrigin::of(&game.engine.world).to_level(game.engine.world.get::<Transform>(eye).unwrap().position);
    assert_eq!(
        level,
        DVec3::new(10_000.0, 300.0, -3000.0),
        "the camera is where it was flown, from the level's middle"
    );
    let ground = voxear_terrain::ground_at(&game.engine.world, level.x as f32 - 5120.0 - 4096.0, 72.0).unwrap();
    assert!((ground.height - hills.height_at(10_100.0, -3000.0)).abs() < 0.05);
}

#[test]
fn a_block_world_without_end_keeps_its_chunks_as_the_middle_moves() {
    use voxear_terrain::{
        blocks::{
            BlockWorld,
            file::BlockFile,
            generate::{BlockEndless, BlockGenerate},
        },
        generate::Preset,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut file = BlockFile { block_size: 2.0, ..BlockFile::default() };
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -4,
        caves: 0.0,
        endless: Some(BlockEndless { distance: 70.0, far: 0.0 }),
        ..BlockGenerate::default()
    });
    let scene = "format = 1\n\n[[obj]]\nid = 1\nname = \"Blocks\"\nposition = [-16.0, 0.0, -16.0]\n\
                 [obj.block_world]\ndata = \"Blocks/Field.blockworld\"\n\n[[obj]]\nid = 2\nname = \"Eye\"\n\
                 position = [40.0, 60.0, 40.0]\n[obj.camera]\nfov = 60.0\n";
    // The middle moves every chunk, 64 m, the camera goes past it.
    let project = "[project]\nname = \"Blocks\"\nstart_level = \"Levels/Main.level\"\n\n\
                   [floating_origin]\nmode = \"always\"\ndistance = 64\nstep = 64\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", project.into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", scene.into()),
            ("Blocks/Field.blockworld", file.to_text()),
            ("Worlds/Hills.worldgen", Preset::RollingHills.world().to_text()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    let (blocks, eye) = (find(&game.engine.world, "Blocks"), find(&game.engine.world, "Eye"));
    let settle = |game: &mut Game| {
        for _ in 0..3000 {
            game.tick(1.0 / 60.0, None);
            if voxear_engine::blocks::streamed(&game.engine.world) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("the blocks round the camera were never all made");
    };
    settle(&mut game);
    // The middle moved three chunks under a still camera: every chunk is kept as it was, and nothing more is made.
    let kept = game.engine.world.get::<BlockWorld>(blocks).unwrap().meshes.clone();
    game.move_origin(IVec3::new(-192, 0, 0), None);
    for _ in 0..5 {
        game.tick(1.0 / 60.0, None);
    }
    assert!(voxear_engine::blocks::streamed(&game.engine.world));
    let now = &game.engine.world.get::<BlockWorld>(blocks).unwrap().meshes;
    assert_eq!(now.len(), kept.len());
    for (key, mesh) in now.iter() {
        assert!(kept.get(key).is_some_and(|before| Arc::ptr_eq(before, mesh)), "chunk {key:?} made again");
    }
    // Flown on, the middle moving with the camera as it goes, the ground is made round it as before.
    let mut moves = 0;
    for _ in 0..12 {
        let was = WorldOrigin::of(&game.engine.world);
        game.engine.world.get_mut::<Transform>(eye).unwrap().position.x += 24.0;
        game.tick(1.0 / 60.0, None);
        moves += usize::from(WorldOrigin::of(&game.engine.world) != was);
    }
    assert!(moves >= 3, "the middle moved as the camera went: {moves} times");
    settle(&mut game);
    // Its corner moved with the world by whole chunks, and the ground under the camera is there.
    let origin = WorldOrigin::of(&game.engine.world);
    let corner = game.engine.world.get::<Transform>(blocks).unwrap().position;
    assert_eq!(origin.to_level(corner), DVec3::new(-16.0, 0.0, -16.0));
    assert_eq!(origin.0.x % 64, 0);
    let at = game.engine.world.get::<Transform>(eye).unwrap().position;
    let world = game.engine.world.get::<BlockWorld>(blocks).unwrap();
    let column = ((at - corner) / 2.0).floor().as_ivec3();
    assert!((-4..100).any(|y| world.blocks.get([column.x, y, column.z]) != 0), "ground under the camera");
    assert!(origin.to_level(at).x > 300.0, "{}", origin.to_level(at));
}

#[test]
fn a_level_of_a_set_size_keeps_its_middle_where_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let scene =
        "format = 1\n\n[[obj]]\nid = 1\nname = \"Eye\"\nposition = [6000.0, 2.0, 0.0]\n[obj.camera]\nfov = 60.0\n";
    write(
        dir.path(),
        &[
            ("voxear.toml", "[project]\nname = \"Field\"\nstart_level = \"Levels/Main.level\"\n".into()),
            ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n".into()),
            ("Scenes/Field.scene", scene.into()),
        ],
    );
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    assert_eq!(WorldOrigin::of(&game.engine.world), WorldOrigin::default(), "no ground without end, no move");
    // Moved by the game itself, it starts again from the level's own middle when a level loads.
    game.move_origin(IVec3::new(-4096, 0, 0), None);
    assert_eq!(game.engine.world.get::<Transform>(find(&game.engine.world, "Eye")).unwrap().position.x, 1904.0);
    game.load_level("Levels/Main.level", None).unwrap();
    assert_eq!(WorldOrigin::of(&game.engine.world), WorldOrigin::default());
    assert_eq!(game.engine.world.get::<Transform>(find(&game.engine.world, "Eye")).unwrap().position.x, 6000.0);
}

const LOOKOUT: &str = r#"using System.Collections.Generic;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Lookout : Behaviour
{
	[WorldPosition] private Vector3 lastSeen = new(10f, 0f, 5f);
	[WorldPosition] private List<Vector3> patrol = [new(1f, 0f, 0f), new(2f, 0f, 0f)];
	private Vector3 unmarked = new(10f, 0f, 5f);

	protected override void OnEnable()
	{
		Origin.Moved += Heard;
	}

	protected override void OnDisable()
	{
		Origin.Moved -= Heard;
	}

	private void Heard(Vector3 by)
	{
		var level = Origin.ToLevel(Obj.Position);
		Log.Info($"moved {by.X:0}, seen {lastSeen.X:0}, patrol {patrol[1].X:0}, unmarked {unmarked.X:0}");
		Log.Info($"middle {Origin.Position.X:0}, here {Obj.Position.X:0}, from the level's {level.X:0}, back {Origin.FromLevel(level).X:0}");
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "bin" || name == "obj" {
                continue;
            }
            if entry.path().is_dir() {
                copy(&entry.path(), &to.join(&name));
            } else {
                std::fs::copy(entry.path(), to.join(&name)).unwrap();
            }
        }
    }
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    copy(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

#[test]
fn scripts_hear_the_middle_move_and_their_marked_positions_move_with_it() {
    use voxear_core::logging::LogCapture;
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Lookout.cs"), LOOKOUT).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let lookout =
        "\n[[obj]]\nid = 40\nname = \"Lookout\"\nposition = [3000.0, 0.0, 0.0]\nbehaviours = [\"Hello.Lookout\"]\n";
    std::fs::write(&scene, std::fs::read_to_string(&scene).unwrap() + lookout).unwrap();
    let project = Project::open(dir.path()).unwrap();
    voxear_engine::scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(voxear_engine::scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    game.move_origin(IVec3::new(-2048, 0, 0), None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("moved -2048, seen -2038, patrol -2046, unmarked 10"), "{lines:?}");
    assert!(said("middle 2048, here 952, from the level's 3000, back 952"), "{lines:?}");
}
