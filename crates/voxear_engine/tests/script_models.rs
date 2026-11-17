//! A script tints the model an Obj draws and puts another model in its
//! place through `Obj.Model`: two hairs made on one rig keep the pose, a
//! model of other bones is posed afresh, the tint stays through each, and
//! what is not a model leaves the last one drawn. With an offscreen GPU
//! where there is one, so the new model's parts are drawn. Builds a copy of
//! Hello with one more script; skips itself without .NET and the managed
//! build.

use voxear_core::logging::LogCapture;
use voxear_core::math::{Quat, Vec3};
use voxear_engine::{
    Engine, Game, Project,
    scene_file::{ModelData, ModelSource},
    scripts,
};
use voxear_render::{BoxModel, ModelRenderer, Pose};
use voxear_scene::{Name, Transform};

const DRESSER: &str = r##"using System;
using System.Numerics;
using Voxear;

namespace Hello;

public sealed class Dresser : SystemBase
{
	private static int frame;

	private static string Hex(Color color)
	{
		static int Byte(float channel) => (int)MathF.Round(channel * 255f);
		return $"#{Byte(color.R):x2}{Byte(color.G):x2}{Byte(color.B):x2}";
	}

	protected override void Update()
	{
		var wearer = Obj.Find("Wearer");
		var model = wearer.Model;
		switch (frame++)
		{
			case 0:
				Log.Info($"plain: {Obj.Find("Ground").Model.IsValid} {model.IsValid} {Hex(model.Tint)} {model.Model == ModelAsset.Load("Characters/Long.boxmodel")}");
				model.Tint = Color.Hex("#c08040");
				var head = wearer.Skeleton.Find("Head");
				head.LocalPosition = new Vector3(0f, 2f, 0f);
				model.Model = ModelAsset.Load("Characters/Short.boxmodel");
				Log.Info($"short: {model.Model.IsSet} {wearer.Skeleton.BoneCount} {Hex(model.Tint)} {wearer.Skeleton.Find("Head").LocalPosition.Y:0.0}");
				break;
			case 1:
				model.Model = ModelAsset.Load("Characters/Tail.boxmodel");
				Log.Info($"tail: {wearer.Skeleton.BoneCount} {wearer.Skeleton.Find("Tail").IsValid} {Hex(model.Tint)}");
				model.Model = ModelAsset.Load("Levels/Day.level");
				Log.Info($"not a model: {wearer.Skeleton.Find("Tail").IsValid}");
				var bare = Obj.Find("Bare");
				var added = bare.AddModel(ModelAsset.Load("Characters/Long.boxmodel"));
				Log.Info($"added: {added.IsValid} {bare.Skeleton.BoneCount} {Hex(added.Tint)}");
				break;
			case 2:
				Obj.Find("Bare").Model.Remove();
				Log.Info($"removed: {Obj.Find("Bare").Model.IsValid} {Obj.Find("Bare").Skeleton.IsValid}");
				model.SetTint(TintSlot.Second, Color.Hex("#204060"));
				Log.Info($"second: {Hex(model.GetTint(TintSlot.Second))} {Hex(model.GetTint(TintSlot.Third))} {Hex(model.Tint)}");
				var body = Obj.Find("Bare");
				body.AddModel(ModelAsset.Load("Characters/Tail.boxmodel"));
				model.Follows = body;
				Log.Info($"follows: {model.Follows == body} {Obj.Find("Ground").Model.Follows.IsValid}");
				break;
			case 3:
				var follower = Obj.Find("Wearer").Model;
				Log.Info($"followed: {follower.Follows == Obj.Find("Bare")}");
				follower.Follows = Obj.None;
				Log.Info($"let go: {follower.Follows.IsValid}");
				break;
			case 4:
				var dressed = Obj.Find("Wearer").Model;
				dressed.SetMaterial(MaterialAsset.Load("Materials/Gold.material"));
				dressed.Sheet = Texture.Load("Characters/Striped.png");
				Log.Info($"dressed: {dressed.PartCount} {dressed.PartName(0)} {dressed.GetMaterial(0).IsSet} {dressed.Sheet.IsSet}");
				dressed.Sheet = Texture.Load("Characters/Small.png");
				dressed.SetMaterial(0, MaterialAsset.Load("Characters/Small.png"));
				Log.Info($"kept: {dressed.Sheet.IsSet} {dressed.GetMaterial(0).IsSet}");
				dressed.SetMaterial(MaterialAsset.None);
				dressed.Sheet = Texture.None;
				Log.Info($"own: {dressed.GetMaterial(0).IsSet} {dressed.Sheet.IsSet}");
				break;
		}
	}
}
"##;

/// A head on a body, with hair of `hair` pixels each way, as a box model's
/// file; with a tail too, its bones are other bones.
fn character(hair: u32, tail: bool) -> String {
    let mut model = BoxModel::default();
    let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
    model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
    let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
    model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [hair, hair, hair], [0, 0]);
    if tail {
        let tail = model.add_bone("Tail", Some(body), Vec3::new(0.0, 4.0, -2.0));
        model.add_box(tail, Vec3::new(-1.0, -1.0, -8.0), [2, 2, 8], [0, 32]);
    }
    model.to_text()
}

/// Hello with the script and the models, built, and a game on it.
fn game(log: &LogCapture) -> Option<(tempfile::TempDir, Game)> {
    if voxear_scripting::find_dotnet().is_none() || !voxear_engine::core_assembly("Debug").is_file() {
        eprintln!("skipping: needs .NET and the managed build");
        return None;
    }
    let dir = hello_copy();
    std::fs::write(dir.path().join("Scripts/Dresser.cs"), DRESSER).unwrap();
    std::fs::create_dir_all(dir.path().join("Characters")).unwrap();
    for (name, hair, tail) in [("Long", 8, false), ("Short", 6, false), ("Tail", 8, true)] {
        std::fs::write(dir.path().join(format!("Characters/{name}.boxmodel")), character(hair, tail)).unwrap();
    }
    // A sheet of the models' size, one too small, and a material.
    let striped = voxear_render::Sheet::filled(64, 64, [200, 30, 30, 255]).to_png().unwrap();
    std::fs::write(dir.path().join("Characters/Striped.png"), striped).unwrap();
    let small = voxear_render::Sheet::filled(8, 8, [200, 30, 30, 255]).to_png().unwrap();
    std::fs::write(dir.path().join("Characters/Small.png"), small).unwrap();
    std::fs::create_dir_all(dir.path().join("Materials")).unwrap();
    std::fs::write(dir.path().join("Materials/Gold.material"), "color = [1.0, 0.8, 0.2]\n").unwrap();
    let project = Project::open(dir.path()).unwrap();
    if let Err(error) = scripts::build(&project) {
        let lines: Vec<String> = log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect();
        panic!("{error:#}: {lines:#?}");
    }
    let mut engine = Engine::new();
    engine.set_scripts(scripts::start(&project).unwrap().expect("Hello has scripts"));
    Some((dir, Game::new(project, engine).unwrap()))
}

fn lines(log: &LogCapture) -> Vec<String> {
    log.lock().unwrap().lines.iter().map(|l| l.message.clone()).collect()
}

/// The Objs the script dresses: one drawing the long hair, one drawing
/// nothing yet.
fn wearers(game: &mut Game, renderer: Option<&mut voxear_render::Renderer>) {
    let at = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    let data = ModelData::of("Characters/Long.boxmodel");
    let drawn = match renderer {
        Some(renderer) => voxear_engine::prefab::model_renderer(&mut game.content, renderer, &data).unwrap(),
        None => {
            let guid = game.content.db().guid_of("Characters/Long.boxmodel").unwrap();
            ModelRenderer { animation: game.content.clip_file(guid), ..ModelRenderer::new(Vec::new()) }
        }
    };
    game.engine.world.spawn((Name("Wearer".into()), at, drawn, ModelSource(data)));
    game.engine.world.spawn((Name("Bare".into()), at));
}

#[test]
fn a_script_tints_an_objs_model_and_puts_another_in_its_place() {
    let mut renderer = voxear_render::Renderer::new_offscreen(64, 64, &voxear_render::RendererSettings::default())
        .map_err(|error| eprintln!("drawing nothing: no usable GPU ({error:#})"))
        .ok();
    let log = LogCapture::default();
    let _logging = tracing::subscriber::set_default(voxear_core::logging::capturing_only(log.clone()));
    let Some((_dir, mut game)) = game(&log) else { return };
    game.start(renderer.as_mut()).unwrap();
    wearers(&mut game, renderer.as_mut());
    game.tick(1.0 / 60.0, renderer.as_mut());

    // The first frame: tinted, and drawing the short hair on the same bones, posed as the script posed them.
    let world = &game.engine.world;
    let entity = world.query_ref::<(voxear_ecs::Entity, &Name)>().into_iter().find(|(_, n)| n.0 == "Wearer").unwrap().0;
    let drawn = world.get::<ModelRenderer>(entity).expect("still drawing");
    let tint = [0xc0, 0x80, 0x40].map(|channel| channel as f32 / 255.0);
    let near = |a: [f32; 3]| a.iter().zip(tint).all(|(a, b)| (a - b).abs() < 1e-3);
    assert!(near(drawn.tints[0]), "{:?}", drawn.tints);
    assert!(near(world.get::<ModelSource>(entity).unwrap().0.tint), "kept as a scene writes it");
    assert!(world.get::<Pose>(entity).is_some(), "the pose the script gave stays with the same bones");
    if renderer.is_some() {
        assert_eq!(drawn.parts.len(), 1, "the short hair's one part");
        assert_eq!(drawn.gpu_tints()[0], 0xc0 | 0x80 << 8 | 0x40 << 16 | 0xff << 24);
    }

    for _ in 0..4 {
        game.tick(1.0 / 60.0, renderer.as_mut());
    }
    let lines = lines(&log);
    let has = |line: &str| assert!(lines.iter().any(|l| l == line), "{line}: {lines:#?}");
    // Named by the path it was given, so the same path is the same model.
    has("plain: False True #ffffff True");
    // The same bones keep the pose the script gave the head.
    has("short: True 2 #c08040 2.0");
    has("tail: 3 True #c08040");
    has("not a model: True");
    assert!(lines.iter().any(|l| l.contains("Levels/Day.level is not a model")), "{lines:#?}");
    has("added: True 2 #ffffff");
    has("removed: False False");
    has("second: #204060 #ffffff #c08040");
    has("follows: True False");
    has("followed: True");
    has("let go: False");
    // A part's material and a box model's sheet are drawn where there is a renderer to draw them.
    if renderer.is_some() {
        has("dressed: 1 Tail True True");
        assert!(lines.iter().any(|l| l.contains("Characters/Small.png is 8 × 8 pixels")), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("Characters/Small.png is not a material")), "{lines:#?}");
        has("kept: True True");
        has("own: False False");
    }
    let world = &game.engine.world;
    let tail = game.content.db().guid_of("Characters/Tail.boxmodel").unwrap();
    assert_eq!(world.get::<ModelSource>(entity).unwrap().0.model, tail.to_string(), "kept by its GUID");
    let tint2 = [0x20, 0x40, 0x60].map(|channel| channel as f32 / 255.0);
    let saved = &world.get::<ModelSource>(entity).unwrap().0;
    assert!(saved.tint2.iter().zip(tint2).all(|(a, b)| (a - b).abs() < 1e-3), "{:?}", saved.tint2);
    assert!(!saved.follow_parent, "another Obj than its parent is followed for this run alone");
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
