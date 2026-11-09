//! Scripts and terrain: a behaviour asks how high the ground is and which
//! way it faces, casts a ray onto it, and physics' rays find it too. Builds
//! a copy of Hello with a sloping field added; skips itself without .NET
//! and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::{Heightmap, file::TerrainFile};

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

/// A field 128 m across, its corner at (-64, 2, -64), rising 0.1 m a metre
/// along x, and an Obj with the probe.
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
fn scripts_ask_the_ground_and_cast_rays_onto_it() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Probe.cs"), PROBE).unwrap();
    let file = TerrainFile { size: 128.0, resolution: 257, ..TerrainFile::default() };
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
}
