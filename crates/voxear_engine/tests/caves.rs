//! Cave rooms: a prefab of Cave Shape and Cave Exit Objs read into the
//! numbers a world generator keeps, read again when the prefab changes, and
//! a behaviour that finds a cave level's rooms, exits and floors. Builds a
//! copy of Hello; the script half skips itself without .NET and the managed
//! build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, caves, scripts};
use voxear_terrain::{
    blocks::{file::BlockFile, generate::BlockGenerate},
    generate::{RoomForm, WorldGen, starters},
};

/// A room: a hall on the prefab's root, a side chamber on a child that is
/// moved, turned a quarter turn about y and twice as large, a pillar put
/// back, and one way out along the hall.
const HALL: &str = r#"format = 1

[[obj]]
id = 1
name = "Hall"
position = [100.0, 50.0, 0.0]
[obj.cave_shape]
shape = "box"
size = [16.0, 5.0, 6.0]

[[obj]]
id = 2
name = "Side"
parent = 1
position = [0.0, 0.0, 6.0]
rotation = [0.0, 90.0, 0.0]
scale = [2.0, 1.0, 1.0]
[obj.cave_shape]
shape = "half_ball"
size = [4.0, 4.0, 6.0]
chance = 0.5
margin = 0.25

[[obj]]
id = 3
name = "Pillar"
parent = 2
position = [1.0, 0.0, 0.0]
[obj.cave_shape]
shape = "box"
mode = "fill"
size = [1.0, 10.0, 1.0]

[[obj]]
id = 4
name = "Way Out"
parent = 1
position = [8.0, 1.5, 0.0]
rotation = [0.0, 90.0, 0.0]
cave_exit = {}
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

/// The Cave Level starter with the hall as its one room, not yet read.
fn level_of_halls() -> WorldGen {
    let world = starters().pop().expect("the cave level").world(3, 1.0);
    let mut graph = world.graph().clone();
    for node in &mut graph.nodes {
        if let Some(rooms) = node.kind.rooms_mut() {
            rooms.push(voxear_terrain::generate::Room {
                prefab: Some("Prefabs/Hall.prefab".into()),
                ..Default::default()
            });
        }
    }
    world.with_graph(graph)
}

#[test]
fn a_room_prefab_is_read_into_a_generators_numbers_and_again_when_it_changes() {
    let dir = hello_copy();
    std::fs::write(dir.path().join("Prefabs/Hall.prefab"), HALL).unwrap();
    let project = Project::open(dir.path()).unwrap();
    let mut game = Game::new(project, Engine::new()).unwrap();
    let (shapes, exits) = caves::room_of(&mut game.content, "Prefabs/Hall.prefab").unwrap();
    assert_eq!(shapes.len(), 3);
    // The root's own place and turn are not the room's: the room is in the root's space.
    assert_eq!((shapes[0].shape, shapes[0].at, shapes[0].size), (RoomForm::Box, [0.0; 3], [16.0, 5.0, 6.0]));
    assert!(!shapes[0].fill && shapes[0].turn == voxear_terrain::generate::caves::UNTURNED);
    // The child's scale sizes its shape along its own axes, and its quarter turn lays its x along the room's -z.
    assert_eq!((shapes[1].shape, shapes[1].at, shapes[1].size), (RoomForm::HalfBall, [0.0, 0.0, 6.0], [8.0, 4.0, 6.0]));
    assert_eq!((shapes[1].chance, shapes[1].margin), (0.5, 0.25));
    assert_eq!(shapes[1].turn, [0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0]);
    // A child of the child: 1 m along the child's x, twice as far for its scale, which is the room's -z.
    assert!(shapes[2].fill && shapes[2].at == [0.0, 0.0, 4.0], "{:?}", shapes[2]);
    assert_eq!(shapes[2].size, [2.0, 10.0, 1.0]);
    assert_eq!(exits.len(), 1);
    assert_eq!((exits[0].at, exits[0].way), ([8.0, 1.5, 0.0], [1.0, 0.0, 0.0]), "forward, turned to the room's x");

    // A generator that lists the room takes its numbers, and makes its caves from them.
    let listed = level_of_halls();
    let read = caves::read_rooms(&mut game.content, listed.graph()).expect("the room read for the first time");
    let read = listed.with_graph(read);
    assert_ne!(read.hash(), listed.hash(), "other ground than with the room not yet read");
    assert!(caves::read_rooms(&mut game.content, read.graph()).is_none(), "read again, nothing changed");
    let rooms = read.caves().unwrap().rooms(None);
    assert!(rooms.iter().all(|room| room.prefab.as_deref() == Some("Prefabs/Hall.prefab") && room.exits.len() == 1));
    let guid = game.content.parse("Prefabs/Hall.prefab").unwrap().guid().unwrap();
    assert!(
        caves::lists_room(&game.content, read.graph(), guid)
            && !caves::lists_room(&game.content, starters()[5].world(1, 1.0).graph(), guid)
    );
    // Written to its file and read back, it needs no prefab to make the same ground.
    assert_eq!(WorldGen::parse(&read.to_text()).unwrap().hash(), read.hash());

    // The prefab changed: its hall longer. Read again, the generator's ground changes.
    std::fs::write(
        dir.path().join("Prefabs/Hall.prefab"),
        HALL.replace("size = [16.0, 5.0, 6.0]", "size = [20.0, 5.0, 6.0]"),
    )
    .unwrap();
    game.content.forget("Prefabs/Hall.prefab");
    let longer = caves::read_rooms(&mut game.content, read.graph()).expect("the room changed");
    assert_eq!(longer.nodes.iter().find_map(|node| node.kind.rooms()).unwrap()[0].shapes[0].size, [20.0, 5.0, 6.0]);
    assert_ne!(read.with_graph(longer).hash(), read.hash());
}

const FINDER: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Finder : Behaviour
{
	private int frame;

	protected override void Update()
	{
		frame++;
		if (frame != 3)
		{
			return;
		}
		var rooms = Caves.Rooms;
		var start = Caves.Start!;
		var end = Caves.End!;
		Log.Info($"rooms {rooms.Count}, layout {Caves.Network!.Layout}, world {Caves.Network.World.Name}");
		Log.Info($"start {start.Index} main {start.OnMainPath}, end {end.Index} main {end.OnMainPath}, last {rooms[^1].Index} main {rooms[^1].OnMainPath}");
		var spot = start.SpawnPoint();
		Log.Info($"over the floor {Blocks.At(spot + new Vector3(0f, 0.5f, 0f))}, under it {Blocks.At(spot - new Vector3(0f, 1.5f, 0f)) != 0}");
		var leads = "";
		foreach (var room in start.Neighbours)
		{
			leads += $"{room.Index} ";
		}
		Log.Info($"start leads to {leads.Trim()}, its exit to {start.Exits[0].LeadsTo!.Index}, used {start.Exits[0].Used}");
		var back = false;
		foreach (var room in rooms[1].Neighbours)
		{
			back |= ReferenceEquals(room, start);
		}
		Log.Info($"room 1 leads back {back}, prefab loaded {start.Prefab.IsValid}, upright {start.Up.Y}");
		var network = Caves.Of(Obj.Find("Cave"))!;
		var near = network.RoomsNear(start.Position, 6f);
		Log.Info($"near the start {near.Count}, the same room {ReferenceEquals(near[0], network.Rooms[0])}, inside {network.RoomAt(spot + Vector3.UnitY)?.Index}, in rock {network.RoomAt(spot - new Vector3(0f, 9f, 0f)) == null}");
		var exitAt = start.Exits[0].Position;
		Log.Info($"an exit {Vector3.Distance(exitAt, start.Point(new Vector3(8f, 1.5f, 0f))):0.00} m from where the prefab has it, {Blocks.At(exitAt)}");
	}
}
"#;

const OBJS: &str = r#"
[[obj]]
id = 40
name = "Cave"
position = [10.0, 0.0, -20.0]
[obj.block_world]
data = "Blocks/Cave.blockworld"

[[obj]]
id = 42
name = "Finder"
behaviours = ["Hello.Finder"]
"#;

#[test]
fn scripts_find_a_cave_levels_rooms_exits_and_floors() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Finder.cs"), FINDER).unwrap();
    std::fs::write(dir.path().join("Prefabs/Hall.prefab"), HALL).unwrap();
    // The generator with the room's numbers in its file, as the editor writes it.
    {
        let project = Project::open(dir.path()).unwrap();
        let mut game = Game::new(project, Engine::new()).unwrap();
        let listed = level_of_halls();
        let read = caves::read_rooms(&mut game.content, listed.graph()).unwrap();
        std::fs::create_dir_all(dir.path().join("Worlds")).unwrap();
        std::fs::write(dir.path().join("Worlds/Cave.worldgen"), listed.with_graph(read).to_text()).unwrap();
    }
    // A world of metre blocks round the level's first room, its corner away from the level's middle.
    let mut file = BlockFile { block_size: 1.0, ..BlockFile::default() };
    file.ground.size = [64, 64];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Cave.worldgen".into()),
        position: [0.0, 0.0],
        bottom: 0,
        ..Default::default()
    });
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Cave.blockworld"), file.to_text()).unwrap();
    let scene = dir.path().join("Scenes/Environment.scene");
    let text = std::fs::read_to_string(&scene).unwrap() + OBJS;
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
    assert!(said("rooms 12, layout Level, world Cave"), "{lines:?}");
    assert!(said("start 0 main True, end 7 main True, last 11 main False"), "{lines:?}");
    assert!(said("over the floor 0, under it True"), "air to stand in, rock to stand on: {lines:?}");
    // The next room of the main path and two side rooms off the first, all by the hall's one exit.
    assert!(said("start leads to 8 1 10, its exit to 8, used True"), "{lines:?}");
    assert!(said("room 1 leads back True, prefab loaded True, upright 1"), "{lines:?}");
    assert!(said("near the start 1, the same room True, inside 0, in rock True"), "{lines:?}");
    assert!(said("an exit 0.00 m from where the prefab has it, 0"), "{lines:?}");
}
