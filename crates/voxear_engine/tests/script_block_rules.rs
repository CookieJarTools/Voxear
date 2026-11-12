//! Scripts and blocks' rules: a behaviour reads what a block says, mines
//! blocks by hand and with a tool for what they drop, takes the time a
//! block takes, lists every block of the project, puts down a block its
//! world has not held, reaches through water and takes it up in a bucket,
//! and puts its changes back from their bytes. Builds a copy of Hello with a world of blocks, blocks and items
//! added; skips itself without .NET and the managed build. A test of its
//! own, as the .NET runtime starts once in a process.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::file::BlockFile;

const MINER: &str = r#"using System.Linq;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Miner : Behaviour
{
	private static readonly Vector3 Down = -Vector3.UnitY;
	private int frame;
	private byte[] kept = [];
	private Vector3 planked;

	protected override void Start()
	{
		Blocks.Mining += mining => { if (mining.Held == null) mining.Drops.Clear(); };
		Blocks.Mined += mined => Log.Info($"mined {mined.Block.Name} with {mined.Held?.Name ?? "a hand"}");
		Blocks.Placed += placed => Log.Info($"put down {placed.Block.Name} from {placed.Item?.Name}");
	}

	protected override void Update()
	{
		frame++;
		if (frame == 2)
		{
			Mine();
			Place();
			Pour();
			kept = Blocks.CaptureEdits();
		}
		if (frame == 4)
		{
			Blocks.Fill(new Vector3(0f, 8f, 0f), new Vector3(31.5f, 12f, 31.5f), Blocks.Air);
			Log.Info($"cleared, planks {Blocks.At(planked)}");
			Log.Info($"applied {Blocks.ApplyEdits(kept)}, planks {Blocks.At(planked)} named {Blocks.DefinitionAt(planked).Name}");
		}
	}

	private void Mine()
	{
		var stone = Block.Load("Blocks/Stone.block");
		var pick = Definition.Load<Item>("Items/Pick.asset");
		Log.Info($"every block: {string.Join(", ", Blocks.All.Select(block => block.Name))}");
		Log.Info($"{stone.Name}: hardness {stone.Hardness}, tool {stone.Tool?.Name}, tier {stone.Tier}, drops {stone.Drops.Count}");
		Log.Info($"by hand {stone.BreakTime():0.0} s, with the pick {stone.BreakTime(pick):0.0} s, drops to a hand {stone.DropsFor()}, to the pick {stone.DropsFor(pick)}");
		Blocks.Raycast(new Vector3(0.5f, 20f, 0.5f), Down, out var hit);
		var under = Blocks.DefinitionAt(new Vector3(0.5f, 5.5f, 0.5f));
		Log.Info($"met {hit.Definition.Name}, the same {hit.Definition == stone}, under it a block {under.IsValid}");
		Log.Info($"by hand got {Blocks.Mine(hit).Length}, left {Blocks.At(hit.Middle)}");
		Blocks.Raycast(new Vector3(1.5f, 20f, 0.5f), Down, out var second);
		var got = Blocks.Mine(second, pick);
		Log.Info($"with the pick got {got.Length}: {got[0]}");
		var miner = new BlockMiner();
		Blocks.Raycast(new Vector3(2.5f, 20f, 0.5f), Down, out var third);
		Log.Info($"dug 0.3 s: {miner.Dig(third, pick, out _, 0.3f)}, {miner.Progress:0.0} through");
		Log.Info($"dug 0.6 s: {miner.Dig(third, pick, out var dug, 0.3f)}, got {dug.Length}, left {Blocks.At(third.Middle)}");
	}

	private void Place()
	{
		var planks = Definition.Load<Item>("Items/Planks.asset")!;
		Blocks.Raycast(new Vector3(8.5f, 20f, 8.5f), Down, out var floor);
		planked = floor.Before;
		Log.Info($"planks placed {Blocks.Place(floor, planks)}, numbered {Blocks.At(planked)}, named {Blocks.DefinitionAt(planked).Name}");
	}

	private void Pour()
	{
		var water = Block.Load("Blocks/Water.block");
		var bucket = Definition.Load<Item>("Items/Bucket.asset")!;
		var planks = Definition.Load<Item>("Items/Planks.asset")!;
		var over = new Vector3(12.5f, 20f, 12.5f);
		Blocks.Set(new Vector3(12.5f, 8.5f, 12.5f), water);
		Blocks.Raycast(over, Down, out var ground);
		Blocks.Raycast(over, Down, out var wet, 1000f, liquids: true);
		Log.Info($"through the water to {ground.Definition.Name} at {ground.Point.Y:0.0}, or onto {wet.Definition.Name} at {wet.Point.Y:0.0}");
		Log.Info($"a pick takes up {Blocks.Scoop(wet, Definition.Load<Item>("Items/Pick.asset")!)?.Name ?? "nothing"}");
		var full = Blocks.Scoop(wet, bucket)!;
		Log.Info($"the bucket takes up {full.Name}, leaving {Blocks.At(wet.Middle)}");
		Log.Info($"poured {Blocks.Place(ground, full)}, leaves {full.Leaves?.Name}, there {Blocks.DefinitionAt(ground.Before).Name}");
		Log.Info($"planks into the water {Blocks.Place(ground, planks)}, there {Blocks.DefinitionAt(ground.Before).Name}");
	}
}
"#;

const MINER_OBJS: &str = r#"
[[obj]]
id = 40
name = "Blocks"
[obj.block_world]
data = "Blocks/Field.blockworld"

[[obj]]
id = 42
name = "Miner"
behaviours = ["Hello.Miner"]
"#;

#[test]
fn scripts_mine_blocks_by_their_rules_place_blocks_a_world_does_not_list_and_reach_through_water() {
    use voxear_terrain::blocks::file::BlockKind;
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
    write("Scripts/Miner.cs", MINER);
    // Stone wants a pickaxe of the first tier and drops two cobblestones; planks are a block the world has not held.
    let stone = "color = [0.5, 0.5, 0.52]\nhardness = 2.0\ntool = \"Tools/Pickaxe.asset\"\ntier = 1\n\n\
                 [[drops]]\nitem = \"Items/Cobblestone.asset\"\ncount = [2, 2]\n";
    write("Blocks/Stone.block", stone);
    write("Blocks/Planks.block", "color = [0.6, 0.4, 0.2]\n");
    write("Blocks/Water.block", "color = [0.04, 0.46, 0.33]\nliquid = \"water\"\n");
    write("Tools/Pickaxe.asset", "type = \"Voxear.ToolKind\"\n");
    write("Items/Cobblestone.asset", "type = \"Voxear.Item\"\n");
    write(
        "Items/Pick.asset",
        "type = \"Voxear.Item\"\n\n[fields]\ntool = \"Tools/Pickaxe.asset\"\ntier = 1\nspeed = 4.0\n",
    );
    write("Items/Planks.asset", "type = \"Voxear.Item\"\n\n[fields]\nplaces = \"Blocks/Planks.block\"\n");
    write("Items/Bucket.asset", "type = \"Voxear.Item\"\n");
    let full = "type = \"Voxear.Item\"\n\n[fields]\nplaces = \"Blocks/Water.block\"\nleaves = \"Items/Bucket.asset\"\n";
    write("Items/Water Bucket.asset", full);
    // Its world: stone over dirt, a kind of its own, with water numbered third.
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.blocks = vec![
        BlockKind::of("Blocks/Stone.block"),
        BlockKind::colored("Dirt", [0.45, 0.32, 0.2]),
        BlockKind::of("Blocks/Water.block"),
    ];
    (file.ground.top, file.ground.under, file.ground.bottom) = (1, 2, 2);
    write("Blocks/Field.blockworld", &file.to_text());
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + MINER_OBJS;
    std::fs::write(&scene, text).unwrap();
    let project = Project::open(dir.path()).unwrap();
    scripts::build(&project).unwrap();
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    for _ in 0..8 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    // Every block of the project, whether a world holds it or not, in the order of their paths.
    assert!(said("every block: Planks, Stone, Water"), "{lines:?}");
    // A block's rules, as its file has them, and what they come to with a tool.
    assert!(said("Stone: hardness 2, tool Pickaxe, tier 1, drops 1"), "{lines:?}");
    assert!(said("by hand 2.0 s, with the pick 0.5 s, drops to a hand False, to the pick True"), "{lines:?}");
    assert!(said("met Stone, the same True, under it a block False"), "dirt is the world's own kind: {lines:?}");
    // Mined by hand it breaks and drops nothing; with the pick, its two cobblestones; a miner takes its time.
    assert!(said("by hand got 0, left 0") && said("mined Stone with a hand"), "{lines:?}");
    assert!(said("with the pick got 1: 2 Cobblestone") && said("mined Stone with Pick"), "{lines:?}");
    assert!(said("dug 0.3 s: False, 0.6 through") && said("dug 0.6 s: True, got 1, left 0"), "{lines:?}");
    // Planks, which the world had not held, are numbered after its three as they are first put down.
    assert!(said("planks placed True, numbered 4, named Planks") && said("put down Planks from Planks"), "{lines:?}");
    // A ray passes through water to the ground, or meets it when asked; only what holds water takes it up, and a
    // block is placed through it, taking its place.
    assert!(said("through the water to Stone at 8.0, or onto Water at 9.0"), "{lines:?}");
    assert!(said("a pick takes up nothing"), "{lines:?}");
    assert!(said("the bucket takes up Water Bucket, leaving 0"), "{lines:?}");
    assert!(said("poured True, leaves Bucket, there Water"), "{lines:?}");
    assert!(said("planks into the water True, there Planks"), "{lines:?}");
    // What was changed is kept with which block each number is, and comes back as it was.
    assert!(said("cleared, planks 0"), "{lines:?}");
    assert!(said("applied True, planks 4 named Planks"), "{lines:?}");
    // The world keeps the planks' number with its other three; its file had three as it was read, and is not
    // written by a game, so the planks' number is this machine's own, and said with the changes kept.
    let asset = game.engine.world.query_ref::<(voxear_ecs::Entity, &voxear_engine::blocks::BlockWorldAsset)>();
    let (entity, asset) = asset.into_iter().next().unwrap();
    assert_eq!((asset.from_file, asset.file.blocks.len(), asset.drawn.blocks.len()), (3, 4, 4));
    assert!(asset.file.blocks[3].block.is_some() && asset.file.blocks[3].is_live());
    let since = voxear_engine::blocks::blocks_numbered_since(&game.engine.world, entity);
    assert_eq!(since.len(), 1);
    assert_eq!(since[0].0, 4);
    let on_disk = BlockFile::parse(&std::fs::read_to_string(dir.path().join("Blocks/Field.blockworld")).unwrap());
    assert_eq!(on_disk.unwrap().blocks.len(), 3);
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
