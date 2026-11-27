//! A script reads the Project Settings the game runs by and changes those a
//! running game can, on a copy of Hello: the time steps, the floating
//! origin, the quality budgets and the network read, and the portals', the
//! water trails' and navigation's costs changed where the game reads them.
//! A file of its own, so it runs in a process of its own, since .NET
//! starts only once per process. Skips itself without .NET or the managed
//! build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const SCRIPT: &str = r#"using System;
using Voxear;

namespace Hello;

public sealed class Options : SystemBase
{
	private static int frame;

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			var time = ProjectSettings.Time;
			Log.Info($"read: {time.FixedStep} {time.MaxFixedSteps} {ProjectSettings.FloatingOrigin.Mode} {ProjectSettings.Network.TickRate} {ProjectSettings.Quality.Particles > 0}");
			ProjectSettings.ChangePortals(portals =>
			{
				portals.ViewDistance = 40f;
				portals.Light = PortalLighting.Picture;
			});
			ProjectSettings.ChangeWaterTrails(trails => trails.Quality = WaterTrailQuality.Off);
			ProjectSettings.ChangeNavigationCosts(costs => costs.Road = 0.25f);
			Log.Info($"changed: {ProjectSettings.Portals.ViewDistance} {ProjectSettings.Portals.Light} {ProjectSettings.WaterTrails.Quality} {ProjectSettings.NavigationCosts.Road}");
		}
		if (frame == 3)
		{
			Log.Info("done");
		}
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against. Its own sounds stay out, as large files the
/// test does not need.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let sound = name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".mp3");
            if name.starts_with('.') || name == "bin" || name == "obj" || sound {
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
fn a_script_reads_the_project_settings_and_changes_those_a_running_game_can() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Options.cs"), SCRIPT).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    for _ in 0..300 {
        game.tick(1.0 / 60.0, None);
        if said(&log).iter().any(|line| line == "done") {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().filter(|line| line.starts_with(start)).cloned().collect::<Vec<_>>();
    let step = game.project.file.time.fixed_step;
    assert_eq!(report("read: "), [format!("read: {step} 5 Off 60 True")], "{lines:#?}");
    assert_eq!(report("changed: "), ["changed: 40 Picture Off 0.25"], "{lines:#?}");
    let world = &game.engine.world;
    let portals = world.resource::<voxear_render::portals::PortalQuality>().copied().unwrap();
    assert_eq!((portals.view_distance, portals.light), (40.0, voxear_render::PortalLight::Picture));
    assert_eq!(
        world.resource::<voxear_render::WaterTrails>().map(|t| t.quality),
        Some(voxear_render::TrailQuality::Off)
    );
}
