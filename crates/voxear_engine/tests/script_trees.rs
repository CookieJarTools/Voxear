//! Scripts and trees: a behaviour builds a tree of logs and leaves, mines
//! its foot by hand a slice at a time until it is cut through, hears the
//! tree land and break into logs, and takes them up off the ground. Builds
//! a copy of Hello with a world of blocks, a log, leaves and an item
//! added; skips itself without .NET and the managed build. A test of its
//! own, as the .NET runtime starts once in a process.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::file::{BlockFile, BlockKind};

const LUMBERJACK: &str = r#"using System.Linq;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Lumberjack : Behaviour
{
	private static readonly Vector3 Landing = new(12f, 9f, 8.5f);
	private int frame;

	protected override void Start()
	{
		Trees.Felled += tree => Log.Info($"felled east {tree.Away.X > 0.9f}, {tree.Drops.Count} drops of {tree.Drops[0].Item?.Name}, {tree.Drops.Sum(drop => drop.Count)} in all");
		Blocks.Mined += mined => Log.Info($"mined {mined.Block.Name}");
	}

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			Plant();
		}
		if (frame == 3)
		{
			Chop();
		}
		if (frame == 240)
		{
			var lying = ItemDrops.Near(Landing, 10f);
			Log.Info($"lying {lying.Length}, resting on the ground {lying.All(drop => drop.Position.Y < 8.2f)}");
			Log.Info($"collected {ItemDrops.Collect(Landing, 10f).Sum(stack => stack.Count)}, left {ItemDrops.Near(Landing, 10f).Length}");
		}
	}

	private static void Plant()
	{
		var log = Block.Load("Blocks/Oak Log.block");
		var leaves = Block.Load("Blocks/Oak Leaves.block");
		Log.Info($"{log.Name}: shape {log.Shape}, chops {log.Chops}, fells {log.Fells}");
		for (var y = 8; y <= 13; y++)
		{
			Blocks.Set(new Vector3(8.5f, y + 0.5f, 8.5f), log);
		}
		for (var x = -2; x <= 2; x++)
		{
			for (var y = -1; y <= 2; y++)
			{
				for (var z = -2; z <= 2; z++)
				{
					if ((x != 0 || z != 0) && x * x + y * y + z * z <= 5)
					{
						Blocks.Set(new Vector3(8.5f + x, 13.5f + y, 8.5f + z), leaves);
					}
				}
			}
		}
	}

	private static void Chop()
	{
		Blocks.Raycast(new Vector3(4.5f, 8.5f, 8.5f), Vector3.UnitX, out var foot);
		for (var hit = 0; hit < 4; hit++)
		{
			var left = Trees.SlicesLeft(foot);
			var broke = Blocks.Mine(foot, null, out var drops);
			Log.Info($"hit {hit}: {left} left, broke {broke}, {drops.Length} drops, now {(broke ? 0 : Blocks.StateAt(foot.Middle).Across)} across");
		}
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
name = "Lumberjack"
behaviours = ["Hello.Lumberjack"]
"#;

#[test]
fn a_script_chops_a_tree_down_a_slice_a_hit_hears_it_land_and_takes_up_its_logs() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("Scripts/Lumberjack.cs", LUMBERJACK);
    write(
        "Blocks/Oak Log.block",
        "color = [0.5, 0.35, 0.2]\nshape = \"log\"\n\n[[drops]]\nitem = \"Items/Oak Log.asset\"\ncount = [1, 1]\n",
    );
    write("Blocks/Oak Leaves.block", "color = [0.2, 0.5, 0.2]\nshape = \"bushy\"\nsways = true\n");
    write("Items/Oak Log.asset", "type = \"Voxear.Item\"\n\n[fields]\nplaces = \"Blocks/Oak Log.block\"\n");
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.blocks = vec![BlockKind::colored("Dirt", [0.45, 0.32, 0.2])];
    (file.ground.top, file.ground.under, file.ground.bottom) = (1, 1, 1);
    write("Blocks/Field.blockworld", &file.to_text());
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..242 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("Oak Log: shape Log, chops 4, fells True"), "{lines:?}");
    // Three slices off, each by hand, then cut through: its own log dropped to whoever mined it.
    assert!(said("hit 0: 4 left, broke False, 0 drops, now 12 across"), "{lines:?}");
    assert!(said("hit 1: 3 left, broke False, 0 drops, now 8 across"), "{lines:?}");
    assert!(said("hit 2: 2 left, broke False, 0 drops, now 4 across"), "{lines:?}");
    assert!(said("hit 3: 1 left, broke True, 1 drops, now 0 across") && said("mined Oak Log"), "{lines:?}");
    // The five logs over the cut fall east, away from the side it was mined from, and break into a log each.
    assert!(said("felled east True, 5 drops of Oak Log, 5 in all"), "{lines:?}");
    // Lying on the ground, those close together in one stack.
    assert!(lines.iter().any(|line| line.starts_with("lying ") && line.ends_with("on the ground True")), "{lines:?}");
    assert!(said("collected 5, left 0"), "{lines:?}");
}

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
