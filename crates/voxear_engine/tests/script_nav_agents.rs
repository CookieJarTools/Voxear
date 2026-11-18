//! A script gives an Obj a Nav Agent, sends it across a world of blocks
//! and reads what it is doing, asks Navigation for a way, the nearest
//! place to stand and somewhere to wander to, and puts a Nav Obstacle down. Builds a copy of Hello with
//! one more script and a Block World; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_engine::{
    Engine, Game, Project,
    nav_agent::{NavAgent, NavMoves},
    scripts,
};
use voxear_scene::{Name, Transform};
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

const SHEPHERD: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Shepherd : SystemBase
{
	private int frame;

	protected override void Update()
	{
		var sheep = Obj.Find("Sheep");
		frame++;
		if (frame == 1)
		{
			Log.Info($"before: {sheep.NavAgent.IsValid} {sheep.NavAgent.Arrived}");
			var agent = sheep.AddNavAgent();
			agent.Speed = 4f;
			agent.Moves = NavMoves.AlongTheNav;
			agent.SetDestination(new Vector3(20.5f, 6f, 4.5f), Dimension.Current);
			Log.Info($"added: {agent.IsValid} {agent.Speed} {agent.Moves} {agent.Destination}");
			Log.Info($"into: {agent.DestinationDimension.IsLevel}");
			var way = Navigation.FindPath(new Vector3(2.5f, 6f, 2.5f), new Vector3(20.5f, 6f, 4.5f));
			Log.Info($"way: {way.Status} {way.Corners.Length} {way.Length:0}");
			Log.Info($"nearest: {Navigation.Nearest(new Vector3(5.5f, 7.2f, 5.5f))}");
			Log.Info($"reachable: {Navigation.IsReachable(new Vector3(2.5f, 6f, 2.5f), new Vector3(28.5f, 6f, 28.5f))}");
			var wander = Navigation.RandomPlace(new Vector3(10.5f, 6f, 10.5f), 5f);
			Log.Info($"wander: {wander.HasValue && Vector3.Distance(wander.Value, new Vector3(10.5f, 6f, 10.5f)) <= 5.5f}");
			Log.Info($"area: {Navigation.AreaAt(new Vector3(5.5f, 6f, 5.5f))} {Navigation.AreaAt(new Vector3(20.5f, 6f, 20.5f))}");
			var post = Obj.Find("Post").AddNavObstacle();
			post.Size = new Vector3(3f, 2f, 3f);
			Log.Info($"post: {post.IsValid} {post.Size} {post.Carve} {post.StillTime}");
		}
		if (frame == 5)
		{
			var post = Obj.Find("Post").NavObstacle;
			Log.Info($"carving: {post.IsCarving} {Navigation.AreaAt(new Vector3(10.5f, 6f, 10.5f))}");
		}
		if (frame == 30)
		{
			var agent = sheep.NavAgent;
			Log.Info($"walking: {agent.HasPath} {agent.PathStatus} {agent.Arrived} {agent.Velocity.Length() > 1f} {agent.Path.Corners.Length > 1}");
		}
		if (frame == 31)
		{
			sheep.NavAgent.Stop();
			Log.Info($"stopped: {sheep.NavAgent.IsStopped}");
		}
		if (frame == 32)
		{
			sheep.NavAgent.Resume();
			Log.Info($"resumed: {sheep.NavAgent.IsStopped}");
		}
	}
}
"#;

#[test]
fn a_script_sends_a_nav_agent_across_a_world_of_blocks_and_asks_navigation_for_ways() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Shepherd.cs"), SHEPHERD).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // A field of blocks 32 across and six deep under the sheep.
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 6;
    let (map, faces) = (file.make_ground(None), file.faces().1);
    game.engine.world.spawn((BlockWorld::new(map, &faces, 1.0), Transform::default()));
    let sheep = game.engine.world.spawn((Name("Sheep".into()), Transform::from_position(Vec3::new(2.5, 6.0, 2.5))));
    game.engine.world.spawn((Name("Post".into()), Transform::from_position(Vec3::new(10.5, 7.0, 10.5))));
    // A road over one corner of the field.
    let road = voxear_engine::navigation::NavArea { size: [4.0, 2.0, 4.0], area: voxear_terrain::blocks::block::BlockNavArea::Road };
    game.engine.world.spawn((Transform::from_position(Vec3::new(20.5, 6.5, 20.5)), road));
    let lines =
        |log: &LogCapture| -> Vec<String> { log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect() };

    game.tick(1.0 / 60.0, None);
    let said = lines(&log);
    assert!(said.iter().any(|l| l == "before: False True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "added: True 4 AlongTheNav <20.5, 6, 4.5>"), "{said:#?}");
    assert!(said.iter().any(|l| l == "into: True"), "sent within the level's own dimension: {said:#?}");
    assert!(said.iter().any(|l| l == "way: Complete 2 18"), "straight across: {said:#?}");
    assert!(said.iter().any(|l| l == "nearest: <5.5, 6, 5.5>"), "{said:#?}");
    assert!(said.iter().any(|l| l == "reachable: True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "wander: True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "area: Walkable Road"), "{said:#?}");
    assert!(said.iter().any(|l| l == "post: True <3, 2, 3> True 0.5"), "{said:#?}");
    let agent = game.engine.world.get::<NavAgent>(sheep).expect("the script gave it a Nav Agent");
    assert_eq!((agent.speed, agent.moves), (4.0, NavMoves::AlongTheNav));
    assert_eq!(agent.destination, Some(Vec3::new(20.5, 6.0, 4.5)));
    assert_eq!(agent.destination_dimension, Some(voxear_scene::DimensionId::LEVEL));

    for _ in 0..31 {
        game.tick(1.0 / 60.0, None);
    }
    let said = lines(&log);
    assert!(said.iter().any(|l| l == "walking: True Complete False True True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "stopped: True"), "{said:#?}");
    assert!(said.iter().any(|l| l == "resumed: False"), "{said:#?}");
    assert!(said.iter().any(|l| l == "carving: True NotWalkable"), "{said:#?}");
    let moved = game.engine.world.get::<Transform>(sheep).unwrap().position;
    assert!(moved.x > 3.5, "it walked: {moved}");
}

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
