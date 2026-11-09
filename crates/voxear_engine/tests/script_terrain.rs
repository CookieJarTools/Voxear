//! Scripts and terrain: a behaviour asks how high the ground is and which
//! way it faces, casts a ray onto it, and physics' rays find it too; and a
//! behaviour changes it, with a crater and a mound, and puts it back from
//! its edits as bytes and from a save of the scene. Builds a copy of Hello
//! with a sloping field added; skips itself without .NET and the managed
//! build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::{Heightmap, file::TerrainFile, surfaces::Surface};

const MUD: &str = "5afe0000000000000000000000000e01";

const PROBE: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Probe : Behaviour
{
	protected override void Start()
	{
		Log.Info($"terrain exists {Terrain.Exists}");
		var at = new Vector3(16f, 50f, 0f);
		Log.Info($"height {Terrain.HeightAt(at):0.00}, normal x {Terrain.NormalAt(at).X:0.00}");
		Log.Info($"off the field {float.IsNegativeInfinity(Terrain.HeightAt(new Vector3(500f, 0f, 0f)))}");
		if (Terrain.TryGetGround(at, out var ground))
		{
			Log.Info($"ground of {ground.Terrain.Name}");
		}
		if (Terrain.Raycast(at, -Vector3.UnitY, out var hit))
		{
			Log.Info($"ray met {hit.Obj.Name} at {hit.Point.Y:0.00}, {hit.Distance:0.00} down");
		}
		if (Physics.Raycast(at, -Vector3.UnitY, out var collided))
		{
			Log.Info($"physics met {collided.Obj.Name} at {collided.Point.Y:0.0}");
		}
	}
}
"#;

const DIGGER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Digger : Behaviour
{
	private static readonly Vector3 Middle = new(16f, 50f, 0f);
	private static readonly Vector3 Mound = new(-16f, 50f, 0f);

	private int frame;
	private byte[] kept = [];
	private SceneState? state;

	protected override void Update()
	{
		frame++;
		switch (frame)
		{
			case 2:
				Dig();
				break;
			case 4:
				Restore();
				break;
			case 5:
				state!.Apply();
				Log.Info($"saved scene, mound {Terrain.HeightAt(Mound):0.00}, crater {Terrain.HeightAt(Middle):0.00}");
				break;
			case 6:
				// A shell's crater, dug into the ground, at x = -40: the ground there 4.4 m up.
				var blast = new Vector3(-40f, 4.4f, 20f);
				Log.Info($"exploded {Terrain.Explode(blast, 2f)}, its middle {Terrain.HeightAt(blast):0.0}");
				break;
			case 8:
				var through = Physics.Raycast(new Vector3(-40f, 20f, 20f), -Vector3.UnitY, out var met);
				Log.Info($"physics met the shell hole at {met.Point.Y:0.0}, {through}");
				break;
		}
	}

	// After the probe has asked about the ground as built.
	private void Dig()
	{
		var mud = MaterialAsset.Load("Materials/Mud.material");
		Log.Info($"before {Terrain.HeightAt(Middle):0.00}");
		Log.Info($"crater {Terrain.Crater(Middle, 3f, 1f, mud)}, after {Terrain.HeightAt(Middle):0.00}");
		Log.Info($"missed {Terrain.Raise(new Vector3(500f, 0f, 0f), 2f, 1f)}");
		Log.Info($"painted {Terrain.Paint(Middle, 1f, mud)}, not a surface {Terrain.Paint(Middle, 1f, MaterialAsset.Load("Materials/Nothing.material"))}");
		kept = Terrain.CaptureEdits();
		Log.Info($"captured {kept.Length} bytes");
		state = SceneState.Capture();
		Terrain.Raise(Mound, 4f, 2f);
		Log.Info($"raised {Terrain.HeightAt(Mound):0.00}");
	}

	private void Restore()
	{
		if (Physics.Raycast(Middle, -Vector3.UnitY, out var hit))
		{
			Log.Info($"physics met the crater at {hit.Point.Y:0.0}");
		}
		Log.Info($"applied {Terrain.ApplyEdits(kept)}, mound {Terrain.HeightAt(Mound):0.00}, crater {Terrain.HeightAt(Middle):0.00}");
		Terrain.Raise(Mound, 4f, 2f);
	}
}
"#;

/// A field 128 m across, its corner at (-64, 2, -64), rising 0.1 m a metre
/// along x, and an Obj each for the probe and the digger.
const OBJS: &str = r#"
[[obj]]
id = 40
name = "Field"
position = [-64.0, 2.0, -64.0]
[obj.terrain]
data = "Terrains/Field.terrain"

[[obj]]
id = 41
name = "Probe"
behaviours = ["Hello.Probe"]

[[obj]]
id = 42
name = "Digger"
behaviours = ["Hello.Digger"]
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
fn scripts_ask_about_the_ground_change_it_and_put_it_back() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    // The field has two surfaces: the ground, and mud.
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Probe.cs"), PROBE).unwrap();
    std::fs::write(dir.path().join("Scripts/Digger.cs"), DIGGER).unwrap();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Mud.material"), "color = [0.3, 0.2, 0.1]\n").unwrap();
    std::fs::write(dir.path().join("Materials/Mud.material.meta"), format!("guid = \"{MUD}\"\n")).unwrap();
    let mut file = TerrainFile { size: 128.0, resolution: 257, ..TerrainFile::default() };
    file.surfaces = vec![Surface::new(None), Surface::new(Some(MUD.into()))];
    std::fs::create_dir_all(dir.path().join("Terrains")).unwrap();
    std::fs::write(dir.path().join("Terrains/Field.terrain"), file.to_text()).unwrap();
    let heights = Heightmap::from_fn(file.tiles(), file.tiles(), file.spacing(), |x, _| x * 0.1);
    std::fs::write(dir.path().join("Terrains/Field.heights"), voxear_terrain::file::write_heights(&heights, 0)).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..10 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("terrain exists True"), "{lines:?}");
    // At x = 16, 80 m along the slope from its corner 2 m up: 10 m.
    assert!(said("height 10.00, normal x -0.10"), "{lines:?}");
    assert!(said("off the field True"), "{lines:?}");
    assert!(said("ground of Field"), "{lines:?}");
    assert!(said("ray met Field at 10.00, 40.00 down"), "{lines:?}");
    assert!(said("physics met Field at 10.0"), "physics stands things on the same ground: {lines:?}");

    // Then the digger changes it.
    assert!(said("before 10.00"), "{lines:?}");
    assert!(said("crater True, after 9.00"), "a metre down at its middle: {lines:?}");
    assert!(said("missed False"), "{lines:?}");
    assert!(said("painted True, not a surface False"), "the mud found by its material: {lines:?}");
    let bytes: usize =
        lines.iter().find_map(|line| line.strip_prefix("captured ")?.strip_suffix(" bytes")?.parse().ok()).unwrap();
    assert!(bytes < 160, "a crater and a paint in {bytes} bytes");
    // At x = -16, 48 m along the slope from its corner 2 m up: 6.8 m, and two more.
    assert!(said("raised 8.80"), "{lines:?}");
    assert!(said("physics met the crater at 9.0"), "physics follows the ground: {lines:?}");
    assert!(said("applied True, mound 6.80, crater 9.00"), "{lines:?}");
    assert!(said("saved scene, mound 6.80, crater 9.00"), "{lines:?}");
    // A bowl 2 m round, its middle 0.67 m under the point at 4.4: its floor 2.67 m down, at 1.7, for scripts and
    // physics alike.
    assert!(said("exploded True, its middle 1.7"), "{lines:?}");
    assert!(said("physics met the shell hole at 1.7, True"), "{lines:?}");
}
