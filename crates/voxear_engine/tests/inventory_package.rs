//! The inventory package in a game: a copy of Hello that takes it as a
//! folder package, with a world of blocks, a tool and what a block drops. An
//! Obj's Inventory starts with what its scene gives it, takes what a block
//! mined with the tool in hand drops, wears the tool, shows its hotbar and
//! its screen, and comes back from a save of the scene as it was. The
//! package is the folder `VOXEAR_INVENTORY_PACKAGE` names; without it, .NET
//! or the managed build, the test skips itself. A file of its own, as the
//! .NET runtime starts once in a process.

use std::path::{Path, PathBuf};

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::blocks::file::{BlockFile, BlockKind};

const DRIVER: &str = r#"using System.Numerics;
using Voxear;
using Voxear.Inventories;

namespace Hello;

public sealed class PackDriver : Behaviour
{
	private int frame;
	private SceneState? state;

	protected override void Update()
	{
		frame++;
		var inventory = Obj.Inventory!;
		var screen = Obj.InventoryScreen!;
		var cobble = Definition.Load<Item>("Items/Cobblestone.asset")!;
		switch (frame)
		{
			case 2:
				Log.Info($"starts with {inventory.Held}, {inventory.Slots.Count} places, {inventory.HotbarSize} in the hotbar");
				Blocks.Raycast(new Vector3(0.5f, 20f, 0.5f), -Vector3.UnitY, out var hit);
				inventory.Add(Blocks.Mine(hit, inventory.Held.Item));
				inventory.WearHeld();
				Log.Info($"mined: {inventory.Count(cobble)} cobblestone, the pick worn {inventory.Held.Wear}");
				break;
			case 3:
				screen.Open();
				Log.Info($"screen open {screen.IsOpen}");
				state = SceneState.Capture();
				break;
			case 4:
				inventory.Remove(cobble, 2);
				inventory.Selected = 3;
				Log.Info($"spent: {inventory.Count(cobble)} cobblestone, in hand {inventory.Held}");
				state!.Apply();
				Log.Info($"read back: {inventory.Count(cobble)} cobblestone, in hand {inventory.Held}, worn {inventory.Held.Wear}");
				break;
			case 5:
				screen.Close();
				Log.Info($"screen open {screen.IsOpen}");
				break;
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
name = "Carrier"
behaviours = [
    "Voxear.Inventories.Inventory",
    "Voxear.Inventories.Hotbar",
    "Voxear.Inventories.InventoryScreen",
    "Hello.PackDriver",
]
fields."Voxear.Inventories.Inventory" = { starts = [{ item = "Items/Pick.asset", count = 1 }] }
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &Path, to: &Path) {
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
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello"), dir.path());
    dir
}

#[test]
fn the_inventory_package_carries_what_blocks_drop_shows_it_and_comes_back_from_a_save() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Some(package) = std::env::var_os("VOXEAR_INVENTORY_PACKAGE").map(PathBuf::from) else {
        return eprintln!("skipping: VOXEAR_INVENTORY_PACKAGE names no inventory package");
    };
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    let write = |path: &str, text: &str| {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let package = package.canonicalize().expect("the inventory package's folder");
    write(
        "packages.toml",
        &format!("[packages]\nvoxear-inventory = {{ path = {:?} }}\n", package.display().to_string()),
    );
    write("Scripts/PackDriver.cs", DRIVER);
    // Stone that a pickaxe gets three cobblestones from, and a pick that wears out in sixty blocks.
    let stone = "color = [0.5, 0.5, 0.52]\ntool = \"Tools/Pickaxe.asset\"\ntier = 1\n\n\
                 [[drops]]\nitem = \"Items/Cobblestone.asset\"\ncount = [3, 3]\n";
    write("Blocks/Stone.block", stone);
    write("Tools/Pickaxe.asset", "type = \"Voxear.ToolKind\"\n");
    write("Items/Cobblestone.asset", "type = \"Voxear.Item\"\n");
    let pick = "type = \"Voxear.Item\"\n\n[fields]\ntool = \"Tools/Pickaxe.asset\"\n\
                tier = 1\nstacksize = 1\ndurability = 60\n";
    write("Items/Pick.asset", pick);
    let mut file = BlockFile::default();
    file.ground.size = [16, 16];
    file.blocks = vec![BlockKind::of("Blocks/Stone.block")];
    (file.ground.top, file.ground.under, file.ground.bottom) = (1, 1, 1);
    write("Blocks/Field.blockworld", &file.to_text());
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
    std::fs::write(&scene, text).unwrap();

    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let slots = |game: &mut Game| {
        game.ui.frame(1.0 / 60.0, (1280, 720), 1.0);
        let ui = game.ui.ui();
        let mut found = 0;
        let mut stack = vec![ui.root()];
        while let Some(node) = stack.pop() {
            found += usize::from(ui.has_class(node, "slot") && ui.is_visible(node));
            stack.extend(ui.children(node));
        }
        found
    };
    for _ in 0..2 {
        game.tick(1.0 / 60.0, None);
    }
    assert_eq!(slots(&mut game), 9, "its hotbar's nine places show");
    game.tick(1.0 / 60.0, None);
    // Its screen: the thirty-six places, a grid of four, what they make and what is in the hand, over the hotbar.
    assert_eq!(slots(&mut game), 9 + 36 + 4 + 2, "its screen shows every place");
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    assert_eq!(slots(&mut game), 9, "closed, the hotbar alone again");
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("starts with 1 Pick, 36 places, 9 in the hotbar"), "{lines:#?}");
    assert!(said("mined: 3 cobblestone, the pick worn 1"), "{lines:#?}");
    assert!(said("screen open True") && said("screen open False"), "{lines:#?}");
    assert!(said("spent: 1 cobblestone, in hand nothing"), "{lines:#?}");
    // A save of the scene keeps each item as which data asset it is, how many, and how worn.
    assert!(said("read back: 3 cobblestone, in hand 1 Pick, worn 1"), "{lines:#?}");
}

#[test]
fn the_inventory_packages_sample_reads_whole_and_everything_it_names_is_there() {
    use voxear_assets::{AssetDb, AssetKind, AssetRef};
    use voxear_terrain::blocks::block::BlockDef;
    let Some(package) = std::env::var_os("VOXEAR_INVENTORY_PACKAGE").map(PathBuf::from) else {
        return eprintln!("skipping: VOXEAR_INVENTORY_PACKAGE names no inventory package");
    };
    // The sample as a game takes it: copied into the project, its files' IDs with them.
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            match entry.path().is_dir() {
                true => copy(&entry.path(), &to.join(entry.file_name())),
                false => drop(std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap()),
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    copy(&package.join("Samples/Survival"), &dir.path().join("Samples/Inventory/Survival"));
    let (db, _) = AssetDb::scan(dir.path()).unwrap();
    let named = |reference: &str| AssetRef::parse(reference, &db).ok().and_then(|parsed| parsed.guid());
    let kind_of = |reference: &str| named(reference).and_then(|guid| db.get(guid)).map(|entry| entry.kind);
    let (mut blocks, mut data) = (0, 0);
    for entry in db.entries() {
        let text = std::fs::read_to_string(db.file(&entry.path)).unwrap();
        match entry.kind {
            AssetKind::Block => {
                blocks += 1;
                let block = BlockDef::parse(&text).unwrap_or_else(|error| panic!("{}: {error:#}", entry.path));
                let asked = block.tool.iter().chain(block.drops.iter().filter_map(|drop| drop.item.as_ref()));
                for reference in asked {
                    assert_eq!(kind_of(reference), Some(AssetKind::Data), "{} names {reference}", entry.path);
                }
            }
            AssetKind::Data => {
                data += 1;
                let table: toml::Table = toml::from_str(&text).unwrap();
                let fields = table.get("fields").and_then(toml::Value::as_table).cloned().unwrap_or_default();
                for (key, value) in fields.iter().filter(|(key, _)| key.as_str() != "shape") {
                    let Some(reference) = value.as_str() else { continue };
                    assert!(
                        kind_of(reference).is_some(),
                        "{}'s {key} names {reference}, which is not there",
                        entry.path
                    );
                }
            }
            _ => {}
        }
    }
    assert!(blocks >= 10 && data >= 25, "{blocks} blocks and {data} data assets");
    // An ID is its file's own, kept when the sample is copied: stone is the block its item places.
    let stone = named("Samples/Inventory/Survival/Blocks/Stone.block").expect("stone");
    let item = std::fs::read_to_string(dir.path().join("Samples/Inventory/Survival/Items/Stone.asset")).unwrap();
    assert!(item.contains(&stone.to_string()), "{item}");
}
