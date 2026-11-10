//! Scripts on a planet: a behaviour asks which way gravity pulls beside one
//! and far from it, and walks a Character Controller dropped beside it onto
//! its side and round it, upright to it all the way, as its First-Person
//! View and its Obj are. Builds a copy of Hello with a planet added; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::{
    file::BlockFile,
    generate::{BlockGenerate, BlockPlanet},
};

const PLANET_WALKER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class PlanetWalker : Behaviour
{
	private static readonly Vector3 middle = new(100f, 0f, 0f);
	private int frame;

	protected override void Update()
	{
		frame++;
		var controller = Obj.CharacterController;
		if (frame == 1)
		{
			var by = Physics.GravityAt(Obj.WorldPosition);
			var far = Physics.GravityAt(new Vector3(100f, 500f, 0f));
			Log.Info($"pulled {by.X:0.00} of {by.Length():0.00} by it, {far.Y:0.00} far off, up {controller.Up.X:0.00}");
		}
		var walk = frame > 120 ? Vector3.Cross(Vector3.UnitZ, controller.Up) * 3f : Vector3.Zero;
		var grounded = controller.SimpleMove(walk);
		if (frame == 120 || frame == 360)
		{
			var at = Obj.WorldPosition - middle;
			var round = MathF.Atan2(at.Y, at.X) * 180f / MathF.PI;
			var upright = Vector3.Dot(controller.Up, Vector3.Normalize(at));
			var looking = Vector3.Dot(Vector3.Transform(Vector3.UnitY, Obj.FirstPersonView.Rotation), Vector3.Normalize(at));
			var standing = Vector3.Dot(Obj.Up, Vector3.Normalize(at));
			Log.Info($"frame {frame}: {at.Length():0.00} out, {round:0.0} round, {Math.Abs(at.Z):0.00} aside, grounded {grounded}, upright {upright:0.0000}, view {looking:0.0000}, obj {standing:0.0000}");
		}
	}
}
"#;

const PLANET_OBJS: &str = r#"
[[obj]]
id = 50
name = "Planet"
position = [100.0, 0.0, 0.0]
[obj.block_world]
data = "Blocks/Planet.blockworld"

[[obj]]
id = 51
name = "Walker"
position = [136.0, 0.0, 0.0]
behaviours = ["Hello.PlanetWalker"]
[obj.character]
[obj.first_person]
"#;

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
fn a_script_asks_which_way_a_planet_pulls_and_walks_a_character_round_it() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/PlanetWalker.cs"), PLANET_WALKER).unwrap();
    // A smooth planet, level all round 30 m from its middle.
    let planet = BlockPlanet { radius: 30.0 };
    let generate = BlockGenerate { caves: 0.0, planet: Some(planet), ..BlockGenerate::default() };
    let file = BlockFile { generate: Some(generate), smooth: true, ..BlockFile::default() };
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Planet.blockworld"), file.to_text()).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + PLANET_OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..370 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    // 6 m over its +x side: pulled along -x, the world's own gravity far off, standing away from its middle.
    assert!(said("pulled -9.81 of 9.81 by it, -9.81 far off, up 1.00"), "{lines:?}");
    let at_frame = |frame: u32| -> [f32; 7] {
        let line = lines.iter().find_map(|line| line.strip_prefix(&format!("frame {frame}: "))).expect("its line");
        let numbers: Vec<f32> = line
            .split([' ', ','])
            .filter_map(|word| match word {
                "True" => Some(1.0),
                "False" => Some(0.0),
                _ => word.parse().ok(),
            })
            .collect();
        numbers.try_into().unwrap_or_else(|_| panic!("seven numbers: {line}"))
    };
    // Fallen along -x to its ground, upright to it, its First-Person View and Obj too.
    let [out, round, aside, grounded, upright, view, obj] = at_frame(120);
    assert!((out - 30.0).abs() < 0.6 && round.abs() < 1.0 && aside < 0.5, "on its +x side: {lines:?}");
    assert!(grounded == 1.0 && upright > 0.999, "standing on it: {lines:?}");
    assert!(view > 0.999 && obj > 0.999, "its view and Obj upright to it: {lines:?}");
    // 4 s at 3 m/s round it: 12 m of 30 m round, about 23 degrees.
    let [out, round, aside, grounded, upright, view, obj] = at_frame(360);
    assert!((out - 30.0).abs() < 0.6 && round > 18.0 && aside < 0.5, "walked round it: {lines:?}");
    assert!(grounded == 1.0 && upright > 0.999, "still standing on it: {lines:?}");
    assert!(view > 0.999 && obj > 0.999, "its view and Obj still upright to it: {lines:?}");
}
