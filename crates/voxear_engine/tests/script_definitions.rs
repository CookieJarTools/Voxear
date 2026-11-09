//! Data assets: an `.asset` file of a C# Definition's fields, loaded by
//! path from a script, one instance however often it is loaded, and refused
//! as another type. Builds a copy of Hello with one more script; skips
//! itself without .NET and the managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const STATS: &str = r#"using System.Collections.Generic;
using Voxear;

namespace Hello;

[CreateAsset("Weapons/Weapon Stats")]
public sealed class WeaponStats : Definition
{
	public float Damage = 10f;
	public string Title = "";
	public List<int> Shots = new List<int>();
}

public sealed class Armour : Definition
{
	public float Weight;
}

public sealed class Armoury : SystemBase
{
	protected override void OnCreate()
	{
		var rifle = Definition.Load<WeaponStats>("Weapons/Rifle.asset");
		var again = Definition.Load<WeaponStats>("Weapons/Rifle.asset");
		Log.Info($"stats: {rifle!.Name} {rifle.Path} {rifle.Damage} {rifle.Title} {string.Join(",", rifle.Shots)} {ReferenceEquals(rifle, again)}");
		Log.Info($"as armour: {Definition.Load<Armour>("Weapons/Rifle.asset") == null}");
		Log.Info($"missing: {Definition.Load<WeaponStats>("Weapons/Nothing.asset") == null}");
	}

	protected override void Update()
	{
	}
}
"#;

const RIFLE: &str = "type = \"Hello.WeaponStats\"\n\n[fields]\ndamage = 35.5\ntitle = \"Carbine\"\nshots = [1, 2, 3]\n";

#[test]
fn a_script_loads_a_data_asset_once_and_only_as_its_own_type() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Armoury.cs"), STATS).unwrap();
    std::fs::create_dir_all(dir.path().join("Weapons")).unwrap();
    std::fs::write(dir.path().join("Weapons/Rifle.asset"), RIFLE).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(lines.iter().any(|l| l == "stats: Rifle Weapons/Rifle.asset 35.5 Carbine 1,2,3 True"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "as armour: True"), "{lines:#?}");
    assert!(lines.iter().any(|l| l.contains("holds WeaponStats, not Armour")), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "missing: True"), "{lines:#?}");
}

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
