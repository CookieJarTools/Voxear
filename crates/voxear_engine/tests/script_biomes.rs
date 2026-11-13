//! Scripts and biomes: a behaviour asks which biome a place is, reads its
//! name and colour, and holds one in a field. Builds a copy of Hello with a
//! generated world of blocks that lists two biomes; skips itself without
//! .NET and the managed build. A test of its own, as the .NET runtime
//! starts once in a process.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_terrain::{
    blocks::{file::BlockFile, generate::BlockGenerate},
    generate::Preset,
};

const SURVEYOR: &str = r#"using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Surveyor : Behaviour
{
	public Biome Cool;
	private int frame;

	protected override void Update()
	{
		frame++;
		if (frame != 2)
		{
			return;
		}
		var (cool, warm) = (0, 0);
		var seen = Biome.None;
		for (var z = 0; z < 32; z += 2)
		{
			for (var x = 0; x < 32; x += 2)
			{
				var here = Biomes.At(new Vector3(x + 0.5f, 40f, z + 0.5f));
				cool += here == Cool ? 1 : 0;
				warm += here.IsValid && here != Cool ? 1 : 0;
				seen = here.IsValid && here != Cool ? here : seen;
			}
		}
		Log.Info($"cool {cool > 10}, warm {warm > 10}, all {cool + warm}");
		Log.Info($"the field's {Cool.Name}, coloured {Cool.Color.R:0.0} {Cool.Color.G:0.0} {Cool.Color.B:0.0}; the other {seen.Name}");
		Log.Info($"loaded the same {Biome.Load("Biomes/Cool.biome") == Cool}, past the edge {Biomes.At(new Vector3(500f, 0f, 500f)).IsValid}, none says {Biome.None}");
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
name = "Surveyor"
behaviours = ["Hello.Surveyor"]
fields."Hello.Surveyor" = { Cool = "Biomes/Cool.biome" }
"#;

#[test]
fn scripts_ask_which_biome_a_place_is_and_read_its_name_and_colour() {
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
    write("Scripts/Surveyor.cs", SURVEYOR);
    let hills = Preset::RollingHills.world();
    write("Worlds/Hills.worldgen", &hills.to_text());
    write("Blocks/Moss.block", "color = [0.2, 0.5, 0.2]\n");
    write("Blocks/Sand.block", "color = [0.9, 0.8, 0.5]\n");
    // Two biomes that between them take in every climate, split at how warm the world's middle is.
    let split = hills.climate(16.0, 16.0, hills.height_at(16.0, 16.0)).0;
    let cool = format!(
        "color = [0.2, 0.4, 0.6]\ntemperature = [-60.0, {split}]\nhumidity = [0.0, 1.0]\n\n[ground]\n\
         top = \"Blocks/Moss.block\"\n"
    );
    let warm =
        format!("temperature = [{split}, 60.0]\nhumidity = [0.0, 1.0]\n\n[ground]\ntop = \"Blocks/Sand.block\"\n");
    write("Biomes/Cool.biome", &cool);
    write("Biomes/Warm.biome", &warm);
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.generate = Some(BlockGenerate {
        world: Some("Worlds/Hills.worldgen".into()),
        bottom: -8,
        caves: 0.0,
        biomes: vec!["Biomes/Cool.biome".into(), "Biomes/Warm.biome".into()],
        ..BlockGenerate::default()
    });
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
    for _ in 0..4 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect();
    let said = |text: &str| lines.iter().any(|line| line == text);
    // Every place on its ground is one of its two biomes, each in a fair share of them.
    assert!(said("cool True, warm True, all 256"), "{lines:?}");
    // The biome a field holds is the one a place is; a biome says its name and its colour.
    assert!(said("the field's Cool, coloured 0.2 0.4 0.6; the other Warm"), "{lines:?}");
    assert!(said("loaded the same True, past the edge False, none says None"), "{lines:?}");
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
