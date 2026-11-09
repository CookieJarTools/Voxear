//! A project's packages' C# built with its scripts and run as part of them:
//! two folder packages, one needing the other, and a game script that uses
//! their types. The packages' behaviours, windows and commands are
//! described and found as the game's own, and a change to a package's
//! script reloads into the running game. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips
//! itself without .NET.

use std::path::Path;

use voxear_core::logging::LogCapture;
use voxear_engine::{
    Engine, Game, Project, new_project, package_scripts, script_types,
    scripts::{self, ScriptsFor},
};

const ORBIT_MATH: &str = r#"using Voxear;

namespace OrbitCore;

/// <summary>Sums for things that go round.</summary>
public static class OrbitMath
{
	/// <summary>How far round an orbit of this many seconds is after one second, in degrees.</summary>
	public static float DegreesPerSecond(float period) => 360f / period;

	/// <summary>Whether this machine moves the orbits, from the Networking module the game has on.</summary>
	public static bool Moves(NetRole role) => role != NetRole.Client;
}
"#;

const ORBITER: &str = r#"using OrbitCore;
using Voxear;

namespace OrbitTools;

/// <summary>Goes round the middle of the scene.</summary>
public class Orbiter : Behaviour
{
	[SerializeField] private float period = 4f;

	/// <summary>How long one time round takes, in seconds.</summary>
	public float Period => period;

	protected override void Start()
	{
		Log.Info($"Orbiter {Obj.Name} turns {OrbitMath.DegreesPerSecond(period)} degrees a second");
	}
}
"#;

const ORBIT_WINDOW: &str = r#"using Voxear;

namespace OrbitTools;

[Window("Orbit Tools", Icon = IconName.Settings)]
internal sealed class OrbitWindow : EditorWindow
{
}
"#;

const ORBIT_COMMANDS: &str = r#"using Voxear;

namespace OrbitTools;

internal static class OrbitCommands
{
	[Command("Say Hello from Orbit Tools")]
	private static void SayHello()
	{
#if VOXEAR_EDITOR
		Log.Info("Orbit Tools says hello from the editor");
#else
		Log.Info("Orbit Tools says hello");
#endif
	}
}
"#;

const SATELLITE: &str = r#"using OrbitTools;
using Voxear;

namespace Orbits;

/// <summary>An orbiter of the game's own, from the package's.</summary>
public sealed class Satellite : Orbiter
{
	protected override void Awake()
	{
		Log.Info($"Satellite {Obj.Name} goes round every {Period} seconds");
	}
}
"#;

const SCENE: &str = "# Voxear scene\nformat = 1\n\n[[obj]]\nid = 1\nname = \"Moon\"\nbehaviours = [\"OrbitTools.Orbiter\"]\n\n\
                     [[obj]]\nid = 2\nname = \"Probe\"\nbehaviours = [\"Orbits.Satellite\"]\n";

fn write(folder: &Path, file: &str, text: &str) {
    let path = folder.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn manifest(name: &str, dependencies: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nengine = \">=0.1\"\n\n[dependencies]\n{dependencies}")
}

fn logged(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|line| line.message.clone()).collect()
}

/// A new project beside two folder packages, inside the repository's
/// build folder: `orbit-tools` needs `orbit-core`, and the game's
/// `Satellite` is an `Orbiter` from `orbit-tools`.
fn project_with_packages() -> tempfile::TempDir {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let core = dir.path().join("orbit-core");
    write(&core, "package.toml", &manifest("orbit-core", ""));
    write(&core, "Scripts/OrbitMath.cs", ORBIT_MATH);
    let tools = dir.path().join("orbit-tools");
    write(&tools, "package.toml", &manifest("orbit-tools", "orbit-core = { path = \"../orbit-core\" }\n"));
    write(&tools, "Scripts/Orbiter.cs", ORBITER);
    write(&tools, "Scripts/Editor/OrbitWindow.cs", ORBIT_WINDOW);
    write(&tools, "Scripts/OrbitCommands.cs", ORBIT_COMMANDS);

    let folder = dir.path().join("Orbits");
    let request = new_project::NewProject {
        folder: folder.clone(),
        name: "Orbits".into(),
        template: "empty".into(),
        setup: Some(vec!["none".into()]),
        ide: None,
        git: false,
        modding: false,
        steam_app_id: None,
    };
    new_project::create(&request, &mut |_| {}).unwrap();
    write(&folder, "packages.toml", "[packages]\norbit-tools = { path = \"../orbit-tools\" }\n");
    write(&folder, "Scripts/Satellite.cs", SATELLITE);
    write(&folder, "Scenes/Main.scene", SCENE);
    dir
}

#[test]
fn packages_scripts_build_load_and_reload_with_the_games() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = project_with_packages();
    let root = dir.path().join("Orbits");
    let mut project = Project::open(&root).unwrap();
    // The game has Networking on, and a package's scripts use it with the game's.
    project.set_module(voxear_engine::modules::Module::Networking, true).unwrap();

    // Built: an assembly for each package, the game's using them, with the list beside it.
    let types = script_types::describe(&project).unwrap();
    let csproj = std::fs::read_to_string(root.join("Scripts/Orbits.Scripts.csproj")).unwrap();
    assert!(!csproj.contains("Packages"), "the game's own csproj is left as it was");
    let props = std::fs::read_to_string(root.join(".voxear").join(package_scripts::PROPS_FILE)).unwrap();
    assert!(
        props.contains("Packages/orbit-core/OrbitCore.csproj")
            && props.contains("Packages/orbit-tools/OrbitTools.csproj")
    );
    let tools = std::fs::read_to_string(root.join(".voxear/Packages/orbit-tools/OrbitTools.csproj")).unwrap();
    assert!(tools.contains("../orbit-core/OrbitCore.csproj"), "orbit-tools builds against orbit-core: {tools}");
    let built = root.join(".voxear/Scripts").join(scripts::CONFIGURATION);
    let list = std::fs::read_to_string(built.join(package_scripts::PACKAGE_ASSEMBLIES)).unwrap();
    assert_eq!(list, "OrbitCore.dll\nOrbitTools.dll\n", "each after the one it needs");
    assert!(built.join("OrbitTools.dll").is_file() && built.join("OrbitTools.xml").is_file(), "with its /// comments");

    // Described with the game's: the package's behaviour, window and command.
    assert!(types.behaviour("OrbitTools.Orbiter").is_some_and(|orbiter| orbiter.field("period").is_some()));
    assert!(types.behaviour("Orbits.Satellite").is_some_and(|satellite| satellite.is_a("OrbitTools.Orbiter")));
    assert!(types.windows.iter().any(|window| window.name == "OrbitTools.OrbitWindow"), "{:?}", types.windows);
    assert!(types.commands.iter().any(|command| command.type_name == "OrbitTools.OrbitCommands"));
    let code = std::fs::read_to_string(script_types::code_path(&project).unwrap()).unwrap();
    assert!(code.contains("OrbitCore.OrbitMath") && code.contains("Sums for things that go round."), "Code Reference");

    // A player build's scripts, built apart from the editor's, carry the packages' with them.
    let release = ScriptsFor::Build { development: false, server: false };
    let defines = scripts::built_in_defines(voxear_engine::engine_version(), "linux-x64", release);
    let assembly = scripts::build_for(&project, "Release", &defines).unwrap().expect("Orbits has scripts");
    let folder = assembly.parent().unwrap();
    assert_ne!(folder, built);
    let list = std::fs::read_to_string(folder.join(package_scripts::PACKAGE_ASSEMBLIES)).unwrap();
    assert_eq!(list, "OrbitCore.dll\nOrbitTools.dll\n");
    assert!(folder.join("OrbitCore.dll").is_file() && folder.join("OrbitTools.dll").is_file());

    // Run: the package's behaviour and the game's one made from it, and the package's command.
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Orbits has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.watch_scripts();
    game.start(None).unwrap();
    game.tick(1.0 / 60.0, None);
    let lines = logged(&log);
    assert!(lines.iter().any(|line| line.ends_with("with 2 package assemblies")), "{lines:?}");
    assert!(lines.contains(&"Orbiter Moon turns 90 degrees a second".to_string()), "{lines:?}");
    assert!(lines.contains(&"Satellite Probe goes round every 4 seconds".to_string()), "{lines:?}");
    game.engine.run_command("OrbitTools.OrbitCommands", "SayHello").unwrap();
    assert!(logged(&log).contains(&"Orbit Tools says hello from the editor".to_string()), "built with VOXEAR_EDITOR");

    // A package's script changed: only its assembly builds again, and the running game reloads it.
    let orbiter = dir.path().join("orbit-tools/Scripts/Orbiter.cs");
    std::fs::write(&orbiter, ORBITER.replace("protected override void Start()", "protected override void OnEnable()"))
        .unwrap();
    let started = std::time::Instant::now();
    while !logged(&log).iter().any(|line| line == "reloaded the changed scripts into the running game") {
        assert!(started.elapsed().as_secs() < 300, "never reloaded: {:?}", logged(&log));
        game.tick(1.0 / 60.0, None);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let enabled = logged(&log).iter().filter(|line| *line == "Orbiter Moon turns 90 degrees a second").count();
    assert_eq!(enabled, 2, "the new build's OnEnable ran after the reload: {:?}", logged(&log));
}
