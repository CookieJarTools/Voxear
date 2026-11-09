//! A behaviour's Bone, AnimationClip and Definition fields, set in a scene,
//! arrive in C#: the bone by its Obj and name, found once its model has it,
//! the clip as its reference, and the data asset loaded. Builds a copy of
//! Hello with one more script and a scene of its own; skips itself without
//! .NET and the managed build.

use std::sync::Arc;

use voxear_core::logging::LogCapture;
use voxear_core::math::Vec3;
use voxear_ecs::Entity;
use voxear_engine::{Engine, Game, Project, scripts};
use voxear_render::{Bone, BonePose, ModelAnimation, ModelRenderer, Skeleton};
use voxear_scene::Name;

const HOLDER: &str = r#"using Voxear;

namespace Hello;

public sealed class WeaponStats : Definition
{
	public float Damage = 10f;
}

public sealed class Holder : Behaviour
{
	public Bone Grip;
	public AnimationClip Reload;
	public WeaponStats? Stats;

	protected override void Update()
	{
		Log.Info($"fields: {Grip.Obj.Name} {Grip.Name} {Grip.IsValid} {Grip.Index} {Reload.Reference} {Stats?.Name} {Stats?.Damage}");
	}
}
"#;

const SCENE: &str = r#"format = 1

[[obj]]
id = 1
name = "Arm"

[[obj]]
id = 2
name = "Holder"
behaviours = ["Hello.Holder"]
fields."Hello.Holder" = { grip = { obj = 1, bone = "Elbow" }, reload = "Models/Pyramid.gltf#clip:Reload", stats = "Weapons/Rifle.asset" }
"#;

#[test]
fn bone_clip_and_data_asset_fields_set_in_a_scene_arrive_in_a_script() {
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
    write("Scripts/Holder.cs", HOLDER);
    write("Scenes/Holder.scene", SCENE);
    write("Levels/Holder.level", "format = 1\n[[scene]]\nscene = \"Scenes/Holder.scene\"\n");
    write("Weapons/Rifle.asset", "type = \"Hello.WeaponStats\"\n\n[fields]\ndamage = 35.5\n");
    let manifest = std::fs::read_to_string(dir.path().join("voxear.toml")).unwrap();
    write("voxear.toml", &manifest.replace("Levels/Day.level", "Levels/Holder.level"));
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(None).unwrap();
    let lines = || -> Vec<String> { log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect() };

    // The arm draws no model yet, so its bone is only named.
    game.tick(1.0 / 60.0, None);
    let first = "fields: Arm Elbow False -1 Models/Pyramid.gltf#clip:Reload Rifle 35.5";
    assert!(lines().iter().any(|l| l == first), "{:#?}", lines());

    // Once it draws a model with an elbow, the same field finds it.
    let rest = |translation| BonePose { translation, ..BonePose::default() };
    let skeleton = Skeleton {
        bones: vec![
            Bone { name: "Shoulder".into(), parent: None, rest: rest(Vec3::ZERO) },
            Bone { name: "Elbow".into(), parent: Some(0), rest: rest(Vec3::new(0.3, 0.0, 0.0)) },
        ],
    };
    let animation = Arc::new(ModelAnimation { skeleton, skins: Vec::new(), clips: Vec::new() });
    let model = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(Vec::new()) };
    let arm = game.engine.world.query_ref::<(Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Arm").unwrap().0;
    game.engine.world.insert(arm, model);
    game.tick(1.0 / 60.0, None);
    let found = "fields: Arm Elbow True 1 Models/Pyramid.gltf#clip:Reload Rifle 35.5";
    assert!(lines().iter().any(|l| l == found), "{:#?}", lines());
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
