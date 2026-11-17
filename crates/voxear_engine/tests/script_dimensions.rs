//! Dimensions from C#: a behaviour makes a dimension from its asset, asks
//! of its blocks from the level, moves the camera and itself into it and
//! hears it entered, makes an Obj there and a pair of Portals between the
//! two, and puts a save of the scene back: its changes and where its Objs
//! were. On a copy of Hello. A file of its own, so it runs in a process of
//! its own, since .NET starts only once per process. Skips itself without
//! .NET.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, dimensions, scripts};
use voxear_terrain::blocks::file::BlockFile;

const DT: f32 = 1.0 / 60.0;

const SCRIPT: &str = r##"using System.Numerics;
using System.Threading.Tasks;
using Voxear;

namespace Hello;

public sealed class Traveller : Behaviour
{
	private static readonly Vector3 Tower = new(4.5f, 0f, 4.5f);

	protected override void OnEnable()
	{
		Dimension.Entered += Heard;
		Dimension.Left += HeardLeft;
	}

	protected override void OnDisable()
	{
		Dimension.Entered -= Heard;
		Dimension.Left -= HeardLeft;
	}

	protected override async void Start()
	{
		Log.Info($"start {Dimension.Current.IsLevel} {Obj.Dimension.IsLevel} {Dimensions.Made.Count}");
		var asset = Dimension.Load("Dimensions/Caves.dimension");
		var caves = await Dimensions.Make(asset);
		Log.Info($"made {caves.IsMade} {caves.IsLevel} {caves.Kind} {caves.Asset == asset} {caves.KeepLoaded} {caves.Sky.HasSky} {caves.Weather.HasWeather} {Dimensions.Made.Count} {asset.Made() == caves}");
		// Asked from the level, the caves' ground is theirs alone.
		var top = Top(caves);
		Log.Info($"ground {top > -10f} {Blocks.At(new Vector3(Tower.X, top - 0.5f, Tower.Z))}");
		var block = caves.Blocks.At(new Vector3(Tower.X, top - 0.5f, Tower.Z));
		var at = new Vector3(Tower.X, top + 2.5f, Tower.Z);
		Log.Info($"built {caves.Blocks.Set(at, block)} {caves.Blocks.At(at) != 0} {Blocks.At(at)}");
		// The camera and this Obj into the caves.
		var camera = Obj.Find("Main Camera");
		var (first, second) = (camera.MoveTo(caves, new Vector3(2f, top + 2f, 2f)), Obj.MoveTo(caves, new Vector3(6f, top + 1f, 6f)));
		var went = await first && await second;
		// The game follows its camera into the caves as the next frame begins.
		await Frames(2);
		Log.Info($"moved {went} {Obj.Dimension == caves} {camera.Dimension == caves} {Dimension.Current == caves}");
		var copy = Obj.Instantiate(Obj.Find("Pyramid"));
		Log.Info($"copy {copy.Dimension == caves}");
		var (way, back) = Portal.CreatePair(DimensionInstance.Level, new Vector3(0f, 0f, 30f), Quaternion.Identity, caves, new Vector3(8f, top, 8f), Quaternion.Identity, new Vector2(3f, 4f));
		Log.Info($"portals {way.Obj.Dimension.IsLevel} {back.Obj.Dimension == caves} {way.To == back}");
		// A save of the scene, then both back in the level and the tower taken down: put back, both are in the caves again with it.
		var state = SceneState.Capture();
		await camera.MoveTo(DimensionInstance.Level, new Vector3(0f, 2f, -10f));
		await Obj.MoveTo(DimensionInstance.Level, Vector3.Zero);
		caves.Blocks.Break(at, 0.4f);
		await Frames(2);
		Log.Info($"undone {Obj.Dimension.IsLevel} {caves.Blocks.At(at)}");
		state.Apply();
		await Frames(4);
		Log.Info($"loaded {Obj.Dimension == caves} {camera.Dimension == caves} {caves.Blocks.At(at) != 0}");
	}

	private static async Task Frames(int count)
	{
		for (var frame = 0; frame < count; frame++)
		{
			await Task.Yield();
		}
	}

	private static float Top(DimensionInstance dimension)
	{
		for (var y = 20f; y > -10f; y -= 1f)
		{
			if (dimension.Blocks.At(new Vector3(Tower.X, y - 0.5f, Tower.Z)) != 0)
			{
				return y;
			}
		}
		return -20f;
	}

	private void Heard(DimensionInstance dimension)
	{
		Log.Info($"entered {dimension.Kind} {dimension.IsLevel}");
	}

	private void HeardLeft(DimensionInstance dimension)
	{
		Log.Info($"left {dimension.IsLevel}");
	}
}
"##;

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
fn a_script_makes_a_dimension_moves_into_it_asks_of_its_blocks_and_a_save_puts_it_back() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Traveller.cs"), SCRIPT).unwrap();
    // A dimension of caves: flat ground of its own, in the dark.
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    std::fs::create_dir_all(dir.path().join("Blocks")).unwrap();
    std::fs::write(dir.path().join("Blocks/Caves.blockworld"), file.to_text()).unwrap();
    let caves = "format = 1\n\n[dimension]\nkind = \"caves\"\n\n[[obj]]\nid = 1\nname = \"Rock\"\n\
                 [obj.block_world]\ndata = \"Blocks/Caves.blockworld\"\n";
    std::fs::create_dir_all(dir.path().join("Dimensions")).unwrap();
    std::fs::write(dir.path().join("Dimensions/Caves.dimension"), caves).unwrap();
    let scene = dir.path().join("Scenes/Day.scene");
    let before = std::fs::read_to_string(&scene).unwrap();
    let added =
        "\n[[obj]]\nid = 900\nname = \"Traveller\"\nposition = [3.0, 1.0, 2.0]\nbehaviours = [\"Hello.Traveller\"]\n";
    std::fs::write(&scene, format!("{before}{added}")).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
        panic!(
            "{error:#}: {}",
            lines.iter().filter(|line| line.contains("error")).cloned().collect::<Vec<_>>().join("\n")
        );
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let lines = || -> Vec<String> { log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect() };
    for _ in 0..240 {
        game.tick(DT, None);
        if lines().iter().any(|line| line.starts_with("loaded")) {
            break;
        }
    }
    let lines = lines();
    let said = |text: &str| lines.iter().any(|line| line == text);
    assert!(said("start True True 1"), "the level's alone: {lines:?}");
    assert!(said("made True False Caves True True False False 2 True"), "{lines:?}");
    assert!(said("ground True 0"), "the caves' ground, none of the level's there: {lines:?}");
    assert!(said("built True True 0"), "a block in the caves, none in the level: {lines:?}");
    assert!(said("moved True True True True"), "{lines:?}");
    assert!(said("left True") && said("entered Caves False"), "heard going into the caves: {lines:?}");
    assert!(said("copy True"), "made in the dimension seen from: {lines:?}");
    assert!(said("portals True True True"), "{lines:?}");
    assert!(said("undone True 0"), "{lines:?}");
    assert!(said("loaded True True True"), "the save put both and the tower back in the caves: {lines:?}");
    let errors: Vec<&String> =
        lines.iter().filter(|line| line.contains("Exception") || line.contains("could not")).collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(game.engine.world.resource::<dimensions::Dimensions>().is_some_and(|made| made.made().count() == 1));
}
