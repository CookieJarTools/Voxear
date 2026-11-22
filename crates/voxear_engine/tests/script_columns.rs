//! Columns and far figures from C#: a behaviour in a world of blocks without
//! end, as the Infinite World template makes it, hears the columns made
//! round it and let go once the camera leaves, asks whether the ground and
//! the nav are made where it stands and far off, and sets a figure to be
//! drawn far off (headless: the figure's place, not its model). A file of its own, so it runs in a process of its own,
//! since .NET starts only once per process. Skips itself without .NET.

use std::time::{Duration, Instant};

use voxear_core::{logging::LogCapture, math::Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    Engine, Game, Project,
    crowd_records::RecordGhost,
    new_project::{NewProject, create},
    scripts,
};
use voxear_scene::{Name, Transform};

const DT: f32 = 1.0 / 30.0;

const SCRIPT: &str = r##"using System.Numerics;
using Voxear;

namespace ColumnsTest;

public sealed class ColumnsScript : Behaviour
{
	private int made;
	private int letGo;
	private bool ground;
	private bool ready;
	private bool gone;

	protected override void Start()
	{
		Blocks.ColumnMade += column =>
		{
			made++;
			if (column.Contains(Obj.Transform.Position))
			{
				Log.Info($"here {column.Size} {column.World.IsValid}");
			}
		};
		Blocks.ColumnLetGo += _ => letGo++;
	}

	protected override void Update()
	{
		var here = Obj.Transform.Position;
		if (!ground && Blocks.IsMade(here))
		{
			ground = true;
			Log.Info($"made {made > 0} far {Blocks.IsMade(here + new Vector3(5000f, 0f, 0f))}");
			var figure = new FarFigure(7, Prefab.Load("Characters/Figure.prefab"), here + new Vector3(0f, 0f, 300f));
			FarFigures.Set([figure]);
		}
		if (ground && !ready && Navigation.IsReady(here, 16f))
		{
			ready = true;
			Log.Info("ready True");
		}
		if (!gone && letGo > 0)
		{
			gone = true;
			Log.Info("let go True");
		}
	}
}
"##;

const FIGURE: &str =
    "format = 1\n\n[[obj]]\nid = 1\nname = \"Figure\"\n[obj.model]\nmodel = \"Characters/Player.boxmodel\"\n";

fn named(game: &Game, name: &str) -> Option<Entity> {
    game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == name).map(|(entity, _)| entity)
}

/// Ticks until `done`, for at most two minutes.
fn until(game: &mut Game, done: &mut dyn FnMut(&Game) -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(120) {
        if done(game) {
            return true;
        }
        game.tick(DT, None);
    }
    done(game)
}

#[test]
fn a_script_hears_columns_made_and_let_go_asks_where_ground_is_made_and_draws_a_far_figure() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let parent = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let folder = parent.path().join("Columns");
    let request = NewProject {
        folder: folder.clone(),
        name: "Columns".into(),
        template: "infinite-world".into(),
        setup: None,
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    create(&request, &mut |_| {}).unwrap();
    // Without the template's own scripts the camera stays where the test puts it; the one here is the test's.
    let scene_path = folder.join("Scenes/Infinite World.scene");
    let mut scene = voxear_engine::scene_file::read(&scene_path).unwrap();
    for obj in &mut scene.objs {
        obj.behaviours.clear();
        obj.fields.clear();
        obj.enabled.clear();
    }
    voxear_engine::scene_file::write(&scene_path, &scene).unwrap();
    let before = std::fs::read_to_string(&scene_path).unwrap();
    let added = "\n[[obj]]\nid = 900\nname = \"Listener\"\nposition = [147.5, 7.5, 90.5]\nbehaviours = [\"ColumnsTest.ColumnsScript\"]\n";
    std::fs::write(&scene_path, format!("{before}{added}")).unwrap();
    std::fs::write(folder.join("Scripts/ColumnsScript.cs"), SCRIPT).unwrap();
    std::fs::write(folder.join("Characters/Figure.prefab"), FIGURE).unwrap();

    let project = Project::open(&folder).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("the project has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |line: &str| log.lock().unwrap().lines.iter().any(|said| said.message == line);
    let lines = || log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect::<Vec<_>>();

    // The ground round the camera is made: told of its columns, the one it stands in among them, and not 5 km off.
    assert!(until(&mut game, &mut |_| said("made True far False")), "{:?}", lines());
    assert!(said("here 32 True"), "{:?}", lines());
    assert!(until(&mut game, &mut |_| said("ready True")), "{:?}", lines());
    // The figure it set, 300 m ahead, drawn there with no Obj of its own (its model needs a GPU, which this has not).
    let ghosts = |game: &Game| game.engine.world.query_ref::<&RecordGhost>().into_iter().count();
    let figure = |game: &Game| {
        let world = &game.engine.world;
        world.query_ref::<(&RecordGhost, &Transform)>().into_iter().map(|(_, placed)| placed.position).next()
    };
    let there = Vec3::new(147.5, 7.5, 390.5);
    assert!(
        until(&mut game, &mut |game| figure(game).is_some_and(|at| at.distance(there) < 0.01)),
        "{:?}",
        figure(&game)
    );

    // Every camera and the player go 5 km off: the columns behind them are let go, and the figure is too far to draw.
    let movers: Vec<Entity> = {
        let world = &game.engine.world;
        let mut movers: Vec<Entity> =
            world.query_ref::<(Entity, &voxear_render::Camera)>().into_iter().map(|(entity, _)| entity).collect();
        movers.extend(named(&game, "Player"));
        movers.retain(|entity| !world.has::<voxear_scene::Parent>(*entity));
        movers
    };
    for mover in movers {
        game.engine.world.get_mut::<Transform>(mover).unwrap().position.x += 5000.0;
    }
    assert!(until(&mut game, &mut |_| said("let go True")), "{:?}", lines());
    assert_eq!(ghosts(&game), 0);
}
