//! A script and the Guns module: it gives one Obj a Gun of a kind and
//! another a Gun Holder holding it, aiming. Builds a copy of Hello with the
//! module on and one more script; skips itself without .NET and the
//! managed build.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};

const ARMOURER: &str = r#"using System;
using Voxear;

namespace Hello;

public sealed class Armourer : SystemBase
{
	protected override void OnCreate()
	{
		var gun = Obj.Find("Ground").AddGun();
		gun.Kind = GunKind.OpenBoltSmg;
		var holder = Obj.Find("Main Camera").AddGunHolder();
		holder.Gun = gun;
		holder.Aiming = true;
		holder.Sway = 0.5f;
		Log.Info($"gun {gun.Kind} {gun.Mass} {gun.FireRate} {holder.Gun == gun} {holder.Aiming} {holder.Sway}");
		Obj.Find("Main Camera").AddFirstPersonView();
		var fired = holder.Fire(out var shot);
		Log.Info($"fired {fired} {MathF.Round(shot.Direction.Length(), 3)} {holder.Fire()}");
	}

	protected override void Update()
	{
	}
}
"#;

#[test]
fn with_the_guns_module_a_script_arms_a_holder() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Armourer.cs"), ARMOURER).unwrap();
    let mut project = Project::open(dir.path()).unwrap();
    project.set_module(voxear_engine::modules::Module::Guns, true).unwrap();
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
    assert!(lines.iter().any(|l| l == "gun OpenBoltSmg 4 550 True True 0.5"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "fired True 1 False"), "once, then not again so soon: {lines:#?}");
    let world = &game.engine.world;
    let holders: Vec<voxear_engine::guns::GunHolder> =
        world.query_ref::<&voxear_engine::guns::GunHolder>().into_iter().cloned().collect();
    assert!(holders.len() == 1 && holders[0].aiming && holders[0].gun.is_some());
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
