//! A script sets the component fields the Inspector has and scripts did
//! not reach before, on a copy of Hello with a renderer to load what they
//! name: a Light's cookie, cookie size, shadow tint and body; a Particle
//! System's texture, tiles and mesh; a First-Person View's twist; a Bone
//! Attachment's bone; a Nav Area and a Nav Link; a Dry Area's mesh; an
//! Animator; and the storms' seed. A file of its own, so it runs in a
//! process of its own, since .NET starts only once per process. Skips
//! itself without .NET, the managed build or a GPU.

use voxear_core::logging::LogCapture;
use voxear_engine::{Engine, Game, Project, animation::BoneAttachment, scripts};
use voxear_render::{DryArea, Light, ParticleSystem, Renderer, RendererSettings, SkyBody};

const SCRIPT: &str = r#"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Reaching : SystemBase
{
	private static int frame;
	private static Obj held;

	protected override void Update()
	{
		frame++;
		var lamp = Obj.Find("Lamp");
		if (frame == 2)
		{
			var light = Obj.Instantiate(lamp).AddLight(LightKind.Directional);
			light.Kind = LightKind.Directional;
			light.CookieSize = 7f;
			light.ShadowTint = new Color(0.2f, 0.1f, 0.05f);
			light.Body = SkyBody.Moon;
			light.Cookie = Texture.Load("Textures/Cookie.png");
			Log.Info($"light: {light.CookieSize} {MathF.Round(light.ShadowTint.R, 2)} {light.Body} {light.Cookie.IsSet}");
			light.Cookie = Texture.None;
			Log.Info($"no cookie: {light.Cookie.IsSet}");
			light.Cookie = Texture.Load("Textures/Cookie.png");

			var sparks = Obj.Instantiate(lamp).AddParticles();
			sparks.Texture = ParticleTexture.Puff;
			sparks.Tiles = (4, 2);
			sparks.Animate = true;
			sparks.Mesh = ParticleMesh.Chunk;
			Log.Info($"particles: {sparks.Texture} {sparks.Tiles.Across}x{sparks.Tiles.Down} {sparks.Animate} {sparks.Mesh} {sparks.Graph.IsSet}");

			var view = Obj.Instantiate(lamp).AddFirstPersonView();
			view.Twist = 0.3f;
			Log.Info($"twist: {MathF.Round(view.Twist, 2)}");

			held = Obj.Instantiate(lamp);
			held.AddBoneAttachment();

			var road = Obj.Instantiate(lamp).AddNavArea();
			road.Mark = NavAreaMark.Water;
			road.Size = new Vector3(6f, 2f, 3f);
			var ladder = Obj.Instantiate(lamp).AddNavLink();
			ladder.Kind = NavLinkKind.Jump;
			ladder.Cost = 3f;
			Log.Info($"nav: {road.Mark} {road.Size.X} {ladder.Kind} {ladder.Cost} {ladder.BothWays}");

			var hold = Obj.Instantiate(lamp).AddDryArea();
			hold.Shape = DryShape.Mesh;
			hold.Mesh = MeshAsset.Sphere;
			Log.Info($"dry: {hold.Mesh == MeshAsset.Sphere} {hold.InsideHeight > 0f}");
			Log.Info($"storms: {Storms.Seed}");

			var walker = Obj.Instantiate(lamp).AddAnimator();
			walker.Speed = 0.5f;
			walker.Play(new AnimationClip("Walk"), looping: false);
			Log.Info($"animator: {Obj.Instantiate(lamp).Animator is null} {walker.Clip} {walker.Speed} {walker.Looping} {walker.BlendTime}");
			walker.Stop();
			Log.Info($"stopped: {walker.Clip.IsValid}");
		}
		if (frame == 3)
		{
			// The test names its bone between the frames, as a scene does.
			var attachment = held.BoneAttachment;
			Log.Info($"bone: {attachment.Bone.Name}");
			attachment.Bone = Bone.None;
			Log.Info($"no bone: '{attachment.Bone.Name}'");
		}
		if (frame == 4)
		{
			Log.Info("done");
		}
	}
}
"#;

/// A copy of Hello, inside the repository, where its scripts find the
/// engine to build against. Its own sounds stay out, as large files the
/// test does not need.
fn hello_copy() -> tempfile::TempDir {
    fn copy(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let sound = name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".mp3");
            if name.starts_with('.') || name == "bin" || name == "obj" || sound {
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
fn a_script_sets_the_fields_the_inspector_has() {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        return eprintln!("skipping: needs .NET and the managed build");
    }
    let Ok(mut renderer) = Renderer::new_offscreen(320, 180, &RendererSettings::default()) else {
        return eprintln!("skipping: no usable GPU");
    };
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Reaching.cs"), SCRIPT).unwrap();
    std::fs::create_dir_all(dir.path().join("Textures")).unwrap();
    std::fs::copy(dir.path().join("Docs/How Hello Works/images/Day.png"), dir.path().join("Textures/Cookie.png"))
        .unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    let mut game = Game::new(project, engine).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    let said = |log: &LogCapture| -> Vec<String> {
        let lines = log.lock().unwrap();
        lines.lines.iter().filter(|l| l.target == "script").map(|l| l.message.clone()).collect()
    };
    for _ in 0..300 {
        game.tick(1.0 / 60.0, Some(&mut renderer));
        let world = &mut game.engine.world;
        let held: Vec<_> =
            world.query_ref::<(voxear_ecs::Entity, &BoneAttachment)>().into_iter().map(|(e, _)| e).collect();
        for entity in held {
            if let Some(attachment) = world.get_mut::<BoneAttachment>(entity).filter(|a| a.bone.is_empty()) {
                attachment.bone = "hand_r".into();
            }
        }
        if said(&log).iter().any(|line| line == "done") {
            break;
        }
    }
    let lines = said(&log);
    let report = |start: &str| lines.iter().filter(|line| line.starts_with(start)).cloned().collect::<Vec<_>>();
    assert_eq!(report("light: "), ["light: 7 0.2 Moon True"], "{lines:#?}");
    assert_eq!(report("no cookie: "), ["no cookie: False"], "{lines:#?}");
    assert_eq!(report("particles: "), ["particles: builtin:puff 4x2 True builtin:chunk False"], "{lines:#?}");
    assert_eq!(report("twist: "), ["twist: 0.3"], "{lines:#?}");
    assert_eq!(report("bone: "), ["bone: hand_r"], "named before its model has it: {lines:#?}");
    assert_eq!(report("no bone: "), ["no bone: ''"], "{lines:#?}");
    assert_eq!(report("nav: "), ["nav: Water 6 Jump 3 True"], "{lines:#?}");
    assert_eq!(report("dry: "), ["dry: True True"], "{lines:#?}");
    assert_eq!(report("storms: "), ["storms: 0"], "Hello has no storms of its own: {lines:#?}");
    assert_eq!(report("animator: "), ["animator: True Walk 0.5 False 0.2"], "{lines:#?}");
    assert_eq!(report("stopped: "), ["stopped: False"], "{lines:#?}");

    let world = &game.engine.world;
    let moon = world.query_ref::<&Light>().into_iter().find(|light| light.body == SkyBody::Moon).copied();
    let moon = moon.expect("the moon");
    assert!(moon.cookie.is_some() && moon.cookie_size == 7.0, "its cookie loaded");
    let sparks = world.query_ref::<&ParticleSystem>().into_iter().find(|system| system.animate).copied();
    assert_eq!(sparks.map(|system| system.tiles), Some([4, 2]));
    let hold = world.query_ref::<&DryArea>().into_iter().find(|area| area.mesh.is_some()).copied();
    assert!(hold.is_some_and(|hold| hold.bounds.is_some()), "its mesh and its box");
}
