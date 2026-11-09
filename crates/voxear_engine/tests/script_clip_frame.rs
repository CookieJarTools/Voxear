//! A script reads a clip as it poses its own skeleton and a body's, without
//! posing anything by it: where the clip has the hips and which way the
//! right palm faces, the same on a body the clip is made to fit, and the
//! arm's length at rest. The clip is made here, lifting the hips and
//! turning the right hand. Builds a copy of Hello with one more script;
//! skips itself without .NET and the managed build.

use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::{Quat, Vec3};
use voxear_engine::animation::{ClipSource, Clips};
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Bone, BonePose, Clip, ModelAnimation, ModelRenderer, Skeleton};
use voxear_scene::{Name, Transform};

const READER: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class FrameReader : SystemBase
{
	private int frame;

	protected override void Update()
	{
		if (++frame != 3)
		{
			return;
		}
		var clip = new AnimationClip("Hold.glb#clip:Hold");
		var held = ClipFrame.Of(clip, 0f);
		var onBody = ClipFrame.Of(clip, Obj.Find("Body").Skeleton, 0f);
		var rest = ClipFrame.AtRest(clip);
		if (held?.Rig is not { } rig || onBody?.Rig is not { } bodyRig || rest?.Rig is not { } restRig)
		{
			Log.Info("frame: none");
			return;
		}
		static float Shown(float value) => MathF.Round(value, 2) + 0f;
		var palm = rig.RightArm.PalmFacing;
		var same = (bodyRig.RightArm.PalmFacing - palm).Length() < 1e-3f;
		var turned = (restRig.RightArm.PalmFacing - palm).Length() > 0.5f;
		var length = restRig.RightArm.Length;
		Log.Info($"frame: {held.BoneCount} hips {Shown(held.Position(rig.Hips).Y)} {Shown(held.LocalPosition(rig.Hips).Y)}");
		Log.Info($"bones: {held.Find("head") == rig.Head} {held.Find("tail")} {rig.RightArm.Fingers[1].Count}");
		Log.Info($"palm: same {same} turned {turned} length {length > 0.5f && length < 0.8f}");
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
fn a_script_reads_a_clip_on_its_own_skeleton_and_on_a_body_without_posing_either() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/FrameReader.cs"), READER).unwrap();
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
    let skeleton = body();
    let animation = Arc::new(ModelAnimation { skeleton: skeleton.clone(), skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
    game.engine.world.spawn((Name("Body".into()), Transform::IDENTITY, model));

    // The clip, of a skeleton built as the body's: the hips 10 cm up, the right hand turned a quarter.
    let mut poses = skeleton.rest();
    poses[0].translation.y += 0.1;
    let hand = skeleton.find("hand_r").unwrap();
    poses[hand].rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let clip = Clip { name: "Hold".into(), frames: 1, poses };
    let source = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: vec![clip] });
    let found = ClipSource { animation: source, index: 0, settings: Default::default() };
    game.engine.world.resource_mut::<Clips>().expect("the game's clips").put("Hold.glb#clip:Hold", found);
    for _ in 0..3 {
        game.tick(1.0 / 60.0, None);
    }
    let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
    let bones = body().bones.len();
    assert!(lines.iter().any(|l| *l == format!("frame: {bones} hips 1.1 1.1")), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "bones: True -1 2"), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "palm: same True turned True length True"), "{lines:#?}");
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
