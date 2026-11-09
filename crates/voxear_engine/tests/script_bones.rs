//! A script reads a model's bones and bends one in `Stage.Pose`: names,
//! parents and places, and a turned shoulder carrying its elbow round.
//! Builds a copy of Hello with one more script; skips itself without .NET
//! and the managed build.

use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::{Quat, Vec3};
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Bone, BonePose, ModelAnimation, ModelRenderer, Skeleton};
use voxear_scene::{Name, Transform};

const BENDER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

[UpdateIn(Stage.Pose)]
public sealed class Bender : SystemBase
{
	protected override void Update()
	{
		var skeleton = Obj.Find("Arm").Skeleton;
		var elbow = skeleton.Find("elbow");
		Log.Info($"bones: {skeleton.BoneCount} {elbow.Name} {elbow.Parent.Name} {elbow.Index} {skeleton.Find("Wrist").IsValid} {Obj.Find("Ground").Skeleton.IsValid}");
		var shoulder = skeleton[0];
		shoulder.Rotation = Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.PI / 2f);
		var at = elbow.Position;
		static float Shown(float value) => MathF.Round(value, 2) + 0f;
		Log.Info($"elbow at {Shown(at.X):0.00} {Shown(at.Y):0.00} {Shown(at.Z):0.00}, {elbow.RestPosition.X:0.00} at rest");
	}
}
"#;

#[test]
fn a_script_reads_a_models_bones_and_turning_one_carries_the_next_round() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Bender.cs"), BENDER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    // An arm: a shoulder a metre up, an elbow 30 cm out along X.
    let rest = |translation| BonePose { translation, ..BonePose::default() };
    let skeleton = Skeleton {
        bones: vec![
            Bone { name: "Shoulder".into(), parent: None, rest: rest(Vec3::ZERO) },
            Bone { name: "Elbow".into(), parent: Some(0), rest: rest(Vec3::new(0.3, 0.0, 0.0)) },
        ],
    };
    let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
    let at = Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::ONE);
    game.engine.world.spawn((Name("Arm".into()), at, model));
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(lines.iter().any(|l| l == "bones: 2 Elbow Shoulder 1 False False"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "elbow at 0.00 1.00 -0.30, 0.30 at rest"), "{lines:#?}");
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
