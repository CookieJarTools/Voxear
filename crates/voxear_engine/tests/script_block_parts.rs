//! Scripts and blocks of parts: a door put down takes its upper block, a
//! lever linked to it opens it, a stair is set in a state and read back, a
//! door kept locked by a listener stays shut, and the states and the link
//! come back from the changes' bytes. Builds a copy of Hello with a world of
//! blocks and a door, a lever and a stair; skips itself without .NET and
//! the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::file::BlockFile;

const KEEPER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Keeper : Behaviour
{
	private int frame;
	private int used;
	private byte[] kept = [];
	private readonly Vector3 door = new(4.5f, 8.5f, 4.5f);
	private readonly Vector3 locked = new(6.5f, 8.5f, 4.5f);
	private readonly Vector3 lever = new(10.5f, 8.5f, 4.5f);
	private readonly Vector3 stair = new(12.5f, 8.5f, 4.5f);

	protected override void Start()
	{
		Blocks.Used += use => used++;
		Blocks.Using += use =>
		{
			if (Vector3.Distance(use.Position, locked) < 0.5f)
			{
				use.Cancel = true;
			}
		};
	}

	protected override void Update()
	{
		frame++;
		switch (frame)
		{
			case 2:
				Build();
				break;
			case 3:
				Log.Info($"applied {Blocks.ApplyEdits(kept)}, door open {Blocks.StateAt(door).Open}, links {Blocks.LinksFrom(lever).Count}");
				break;
		}
	}

	private void Build()
	{
		var doorBlock = Block.Load("Blocks/Door.block");
		var leverBlock = Block.Load("Blocks/Lever.block");
		var stairs = Block.Load("Blocks/Stairs.block");
		Log.Info($"door {doorBlock.Shape} usable {doorBlock.IsUsable}, stairs usable {stairs.IsUsable}");
		Blocks.Set(door, doorBlock);
		Blocks.Set(locked, doorBlock);
		Log.Info($"door upper {Blocks.DefinitionAt(door + Vector3.UnitY) == doorBlock}, half {Blocks.StateAt(door + Vector3.UnitY).Half}");
		Blocks.Set(lever, leverBlock, new BlockState(BlockFacing.Down));
		Log.Info($"linked {Blocks.Link(lever, door)}, again {Blocks.Link(lever, door)}");
		Log.Info($"used lever {Blocks.Use(lever)}, lever on {Blocks.StateAt(lever).On}");
		Log.Info($"door open {Blocks.StateAt(door).Open}, upper open {Blocks.StateAt(door + Vector3.UnitY).Open}");
		Log.Info($"used locked {Blocks.Use(locked)}, locked open {Blocks.StateAt(locked).Open}");
		Blocks.Set(stair, stairs, new BlockState(BlockFacing.East, BlockHalf.Upper));
		var turned = Blocks.StateAt(stair);
		Log.Info($"stair {turned.Facing} {turned.Half}, used stone {Blocks.Use(new Vector3(0.5f, 7.5f, 0.5f))}");
		Log.Info($"used {used}");
		kept = Blocks.CaptureEdits();
		Log.Info($"captured {kept.Length} bytes");
		Blocks.Unlink(lever);
		Blocks.SetState(door, Blocks.StateAt(door) with { Open = false }, animate: false);
		Log.Info($"shut {!Blocks.StateAt(door).Open}, links {Blocks.LinksFrom(lever).Count}");
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
name = "Keeper"
behaviours = ["Hello.Keeper"]
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
fn scripts_put_down_use_and_link_blocks_of_parts_and_their_states_come_back() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Keeper.cs"), KEEPER).unwrap();
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Field.blockworld"), file.to_text()).unwrap();
    std::fs::write(dir.path().join("Blocks/Door.block"), "color = [0.5, 0.35, 0.2]\nshape = \"door\"\n").unwrap();
    std::fs::write(dir.path().join("Blocks/Lever.block"), "color = [0.4, 0.4, 0.4]\nshape = \"lever\"\n").unwrap();
    std::fs::write(dir.path().join("Blocks/Stairs.block"), "color = [0.6, 0.6, 0.6]\nshape = \"stairs\"\n").unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..6 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("door Door usable True, stairs usable False"), "{lines:?}");
    assert!(said("door upper True, half Upper"), "a door put down takes its upper block: {lines:?}");
    assert!(said("linked True, again False"), "{lines:?}");
    assert!(said("used lever True, lever on True"), "{lines:?}");
    assert!(said("door open True, upper open True"), "the lever opens the door it is linked to, whole: {lines:?}");
    assert!(said("used locked False, locked open False"), "a listener keeps a door shut: {lines:?}");
    assert!(said("stair East Upper, used stone False"), "{lines:?}");
    assert!(said("used 1"), "only the lever was used: {lines:?}");
    let bytes: usize =
        lines.iter().find_map(|line| line.strip_prefix("captured ")?.strip_suffix(" bytes")?.parse().ok()).unwrap();
    assert!(bytes < 400, "doors, a lever, a stair and a link in {bytes} bytes");
    assert!(said("shut True, links 0"), "{lines:?}");
    assert!(said("applied True, door open True, links 1"), "the open door and its link come back: {lines:?}");
}
