//! A script reads the box round the mesh an Obj draws, once the renderer
//! has the mesh: Hello's Chrome Ball, a sphere scaled up. Builds a copy of
//! Hello with one more script; skips itself without .NET, the managed
//! build or a usable GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Renderer, RendererSettings};

const MEASURER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Measurer : SystemBase
{
	protected override void Update()
	{
		var ball = Obj.Find("Chrome Ball").MeshBox;
		var nothing = Obj.Find("Lamp").MeshBox;
		if (ball is not { } box)
		{
			Log.Info($"box: none, lamp {nothing is null}");
			return;
		}
		static float Shown(float value) => MathF.Round(value, 2) + 0f;
		var top = Vector3.Transform(box.Most, box.Frame);
		Log.Info($"box: {Shown(box.Least.X)} {Shown(box.Most.Y)} top {Shown(top.X)} {Shown(top.Y)} {Shown(top.Z)}, lamp {nothing is null}");
	}
}
"#;

#[test]
fn a_script_reads_the_box_round_an_objs_mesh() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let mut renderer = match Renderer::new_offscreen(64, 64, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Measurer.cs"), MEASURER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    // Without the renderer, nothing knows the ball's box.
    game.tick(1.0 / 60.0, None);
    for _ in 0..3 {
        game.tick(1.0 / 60.0, Some(&mut renderer));
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(lines.iter().any(|l| l == "box: none, lamp True"), "{lines:#?}");
    // A sphere half a metre round, scaled by 1.8 at 0.9 up: its top at 1.8.
    assert!(lines.iter().any(|l| l == "box: -0.5 0.5 top 0.9 1.8 2.5, lamp True"), "{lines:#?}");
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
