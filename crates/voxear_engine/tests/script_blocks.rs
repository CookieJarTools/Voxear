//! Scripts and blocks: a behaviour asks which block is where, casts a ray
//! onto a Block World, breaks and places blocks and blasts a hole, physics
//! follows, and the changes come back from their bytes and from a save of
//! the scene; and another sets how much making worlds may spend. Builds a copy of Hello with a world of blocks added; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::file::BlockFile;

const BUILDER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Builder : Behaviour
{
	private int frame;
	private byte[] kept = [];
	private SceneState? state;

	protected override void Update()
	{
		frame++;
		switch (frame)
		{
			case 2:
				Build();
				break;
			case 4:
				Log.Info($"applied {Blocks.ApplyEdits(kept)}, top {Blocks.At(new Vector3(0.5f, 7.5f, 0.5f))}, on it {Blocks.At(new Vector3(0.5f, 8.5f, 0.5f))}");
				break;
			case 5:
				state!.Apply();
				Log.Info($"saved scene, blast {Blocks.At(new Vector3(20.5f, 6.5f, 20.5f))}");
				break;
			case 7:
				if (Physics.Raycast(new Vector3(20.5f, 30f, 20.5f), -Vector3.UnitY, out var met))
				{
					Log.Info($"physics met the blast hole at {met.Point.Y:0.0}");
				}
				break;
		}
	}

	private void Build()
	{
		Log.Info($"top {Blocks.At(new Vector3(0.5f, 7.5f, 0.5f))}, under {Blocks.At(new Vector3(0.5f, 2.5f, 0.5f))}, air {Blocks.At(new Vector3(0.5f, 9.5f, 0.5f))}");
		if (Blocks.Raycast(new Vector3(0.5f, 20f, 0.5f), -Vector3.UnitY, out var hit))
		{
			Log.Info($"ray met {hit.Obj.Name} block {hit.Block} at {hit.Point.Y:0.0}, {hit.Distance:0.0} down, normal {hit.Normal.Y}");
			Log.Info($"placed {Blocks.Place(hit, 3)}, placed again {Blocks.Place(hit, 3)}");
		}
		Log.Info($"blasted {Blocks.Break(new Vector3(20.5f, 6.5f, 20.5f), 2f)}");
		kept = Blocks.CaptureEdits();
		Log.Info($"captured {kept.Length} bytes");
		state = SceneState.Capture();
		Blocks.Fill(new Vector3(0f, 8f, 0f), new Vector3(3.5f, 11.5f, 3.5f), 2);
		Log.Info($"filled, on it {Blocks.At(new Vector3(0.5f, 8.5f, 0.5f))}");
	}
}
"#;

const OBJS: &str = r#"
[[obj]]
id = 40
name = "Blocks"
[obj.block_world]
data = "Blocks/Field.blockworld"

[[obj]]
id = 42
name = "Builder"
behaviours = ["Hello.Builder"]

[[obj]]
id = 43
name = "Options"
behaviours = ["Hello.Quality"]
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
fn scripts_ask_about_blocks_change_them_and_put_them_back() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Builder.cs"), BUILDER).unwrap();
    std::fs::write(dir.path().join("Scripts/Quality.cs"), QUALITY).unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
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
    assert!(said("top 1, under 3, air 0"), "grass over stone: {lines:?}");
    assert!(said("ray met Blocks block 1 at 8.0, 12.0 down, normal 1"), "{lines:?}");
    assert!(said("placed True, placed again False"), "the same face, where the first went: {lines:?}");
    // The blocks within 2 m of a block's middle: 33, less the one above the ground.
    assert!(said("blasted 32"), "a ball 2 m round of blocks: {lines:?}");
    let bytes: usize =
        lines.iter().find_map(|line| line.strip_prefix("captured ")?.strip_suffix(" bytes")?.parse().ok()).unwrap();
    assert!(bytes < 256, "a block and a blast in {bytes} bytes");
    assert!(said("filled, on it 2"), "{lines:?}");
    assert!(said("applied True, top 1, on it 3"), "the fill undone, the placed block back: {lines:?}");
    assert!(said("saved scene, blast 0"), "{lines:?}");
    // Down to the block 2 m under its middle, whose top is 4 m up.
    assert!(said("physics met the blast hole at 4.0"), "physics follows the blocks: {lines:?}");
    // How much making worlds may spend: as the project has it, then a preset, then a number set by hand.
    use voxear_terrain::{WorldBudget, WorldQuality};
    assert!(said("quality High, columns 24"), "as the project has it: {lines:?}");
    assert!(said("quality Low, threads 2, columns 2, upload 4"), "a preset sets every number: {lines:?}");
    assert!(said("quality Custom, view 80, far 1024"), "a number set by hand makes it Custom: {lines:?}");
    let low = WorldQuality::Low.budget().unwrap();
    let custom = WorldBudget { quality: WorldQuality::Custom, view_distance: 80.0, ..low };
    assert_eq!(WorldBudget::of(&game.engine.world), custom, "the streamer and the renderer read it");
    assert_eq!(voxear_terrain::workers::threads(), 2, "two threads make terrain");
}

const QUALITY: &str = r#"using Voxear;

namespace Hello;

public sealed class Quality : Behaviour
{
	protected override void Start()
	{
		Log.Info($"quality {World.Quality}, columns {World.ColumnsAtOnce}");
		World.Quality = WorldQuality.Low;
		Log.Info($"quality {World.Quality}, threads {World.Threads}, columns {World.ColumnsAtOnce}, upload {World.UploadMegabytes}");
		World.ViewDistance = 80f;
		Log.Info($"quality {World.Quality}, view {World.ViewDistance}, far {World.FarDistance}");
	}
}
"#;
