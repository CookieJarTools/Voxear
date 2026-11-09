//! A script finds a humanoid's rig and poses its arm: reaching the right
//! wrist to a place in the world and turning the hand. Builds a copy of
//! Hello with one more script; skips itself without .NET and the managed
//! build.

use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Bone, BonePose, ModelAnimation, ModelRenderer, Skeleton};
use voxear_scene::{Name, Transform};

const REACHER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

[UpdateIn(Stage.Pose)]
public sealed class Reacher : SystemBase
{
	protected override void Update()
	{
		var rig = Obj.Find("Body").Skeleton.Rig;
		if (rig == null)
		{
			Log.Info("rig: none");
			return;
		}
		Log.Info($"rig: {rig.Head.Name} {rig.RightArm.Hand.Name} {rig.Spine.Count} {rig.LeftLeg?.Foot.Name} {rig.RightArm.Fingers[1].Count}");
		var arm = rig.RightArm;
		var missed = arm.Reach(new Vector3(0.15f, 1.3f, 0.3f), new Vector3(0f, 1f, -1f));
		var wrist = arm.Hand.Position;
		static float Shown(float value) => MathF.Round(value, 2) + 0f;
		Log.Info($"reach: {Shown(missed)} {Shown(wrist.X)} {Shown(wrist.Y)} {Shown(wrist.Z)}");
		arm.Aim(Vector3.UnitZ, -Vector3.UnitY);
		Log.Info($"aim: {Shown(Vector3.Dot(arm.Pointing, Vector3.UnitZ))} {Shown(Vector3.Dot(arm.PalmFacing, -Vector3.UnitY))}");
		Log.Info($"grip: {FingerBends.Of(Grip.Trigger)[2, 0] > FingerBends.Of(Grip.Open)[2, 0]}");
	}
}
"#;

/// A small humanoid standing at the origin facing +Z, its arms out to the
/// sides, its bones named as most rigs name them.
fn body() -> Skeleton {
    let mut bones: Vec<Bone> = Vec::new();
    let mut add = |name: String, parent: Option<usize>, at: Vec3| {
        bones.push(Bone { name, parent, rest: BonePose { translation: at, ..BonePose::default() } });
        bones.len() - 1
    };
    let hips = add("hips".into(), None, Vec3::new(0.0, 1.0, 0.0));
    let spine = add("spine".into(), Some(hips), Vec3::new(0.0, 0.15, 0.0));
    let chest = add("chest".into(), Some(spine), Vec3::new(0.0, 0.2, 0.0));
    let neck = add("neck".into(), Some(chest), Vec3::new(0.0, 0.2, 0.0));
    add("head".into(), Some(neck), Vec3::new(0.0, 0.1, 0.0));
    for (side, out) in [("l", -1.0), ("r", 1.0)] {
        let clavicle = add(format!("shoulder_{side}"), Some(chest), Vec3::new(0.05 * out, 0.15, 0.0));
        let upper = add(format!("upper_arm_{side}"), Some(clavicle), Vec3::new(0.15 * out, 0.0, 0.0));
        let forearm = add(format!("forearm_{side}"), Some(upper), Vec3::new(0.28 * out, 0.0, 0.0));
        let hand = add(format!("hand_{side}"), Some(forearm), Vec3::new(0.26 * out, 0.0, 0.0));
        let index = add(format!("index_01_{side}"), Some(hand), Vec3::new(0.08 * out, 0.0, 0.02));
        add(format!("index_02_{side}"), Some(index), Vec3::new(0.03 * out, 0.0, 0.0));
        add(format!("thumb_01_{side}"), Some(hand), Vec3::new(0.03 * out, -0.02, 0.04));
        add(format!("middle_01_{side}"), Some(hand), Vec3::new(0.085 * out, 0.0, 0.0));
        add(format!("pinky_01_{side}"), Some(hand), Vec3::new(0.075 * out, 0.0, -0.035));
    }
    for (side, out) in [("l", -1.0), ("r", 1.0)] {
        let thigh = add(format!("thigh_{side}"), Some(hips), Vec3::new(0.1 * out, -0.05, 0.0));
        let shin = add(format!("shin_{side}"), Some(thigh), Vec3::new(0.0, -0.45, 0.0));
        add(format!("foot_{side}"), Some(shin), Vec3::new(0.0, -0.42, 0.0));
    }
    Skeleton { bones }
}

#[test]
fn a_script_finds_a_humanoids_rig_and_reaches_its_hand_somewhere() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Reacher.cs"), REACHER).unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let animation = Arc::new(ModelAnimation { skeleton: body(), skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
    game.engine.world.spawn((Name("Body".into()), Transform::IDENTITY, model));
    game.tick(1.0 / 60.0, None);
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    assert!(lines.iter().any(|l| l == "rig: head hand_r 2 foot_l 2"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "reach: 0 0.15 1.3 0.3"), "the wrist is where it was sent: {lines:#?}");
    assert!(lines.iter().any(|l| l == "aim: 1 1"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "grip: True"), "{lines:#?}");
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
