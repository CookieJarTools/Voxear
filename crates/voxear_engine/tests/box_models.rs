//! Box models as assets: a `.boxmodel` file is a model, its bones read
//! without the GPU, drawn by a Model Renderer a scene names it in, and
//! cooked into built content as any model is (headless: no GPU, no scripts).

use voxear_assets::AssetKind;
use voxear_core::math::Vec3;
use voxear_engine::{Content, ContentSettings, Engine, Levels, content_build};
use voxear_render::{BoxModel, ModelRenderer};
use voxear_scene::Name;

const LEVEL: &str = "format = 1\n[[scene]]\nscene = \"Scenes/Field.scene\"\n";
/// An Obj drawing the model, with an Animator: without a GPU only a model something animates is read.
const FIELD: &str = "format = 1\n[[obj]]\nid = 1\nname = \"Walker\"\n[obj.model]\nmodel = \"Mobs/Walker.boxmodel\"\n[obj.animator]\nclip = \"\"\n";

/// A body on two legs with a head, as a box model's file.
fn walker() -> String {
    let mut model = BoxModel::default();
    let body = model.add_bone("Body", None, Vec3::new(0.0, 12.0, 0.0));
    model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 12, 4], [16, 16]);
    let head = model.add_bone("Head", Some(body), Vec3::new(0.0, 12.0, 0.0));
    model.add_box(head, Vec3::new(-4.0, 0.0, -4.0), [8, 8, 8], [0, 0]);
    for (name, x) in [("RightLeg", 2.0), ("LeftLeg", -2.0)] {
        let leg = model.add_bone(name, Some(body), Vec3::new(x, 0.0, 0.0));
        model.add_box(leg, Vec3::new(-2.0, -12.0, -2.0), [4, 12, 4], [0, 16]);
    }
    model.to_text()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let files = [
        ("Levels/Field.level", LEVEL.to_string()),
        ("Scenes/Field.scene", FIELD.to_string()),
        ("Mobs/Walker.boxmodel", walker()),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

/// The bones of the Obj named Walker, as its Model Renderer has them.
fn walkers_bones(engine: &Engine) -> Vec<(String, Option<usize>)> {
    let walker = engine.world.query_ref::<(&Name, &ModelRenderer)>().into_iter().find(|(name, _)| name.0 == "Walker");
    let animation = walker.expect("the scene's Walker").1.animation.clone().expect("a box model has bones");
    animation.skeleton.bones.iter().map(|bone| (bone.name.clone(), bone.parent)).collect()
}

#[test]
fn a_box_model_is_a_model_whose_bones_a_scene_obj_gets() {
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let guid = content.db().guid_of("Mobs/Walker.boxmodel").expect("in the project");
    assert_eq!(content.db().get(guid).unwrap().kind, AssetKind::Model);

    let read = content.clip_file(guid).expect("its bones, read without the GPU");
    let at = |name: &str| read.skeleton.bones[read.skeleton.find(name).unwrap()].rest.translation;
    assert_eq!(at("Body"), Vec3::new(0.0, 0.75, 0.0), "sixteen pixels to a metre");
    assert_eq!(at("LeftLeg"), Vec3::new(-0.125, 0.0, 0.0), "from its parent's pivot");
    assert!(read.clips.is_empty());

    let (mut engine, mut levels) = (Engine::new(), Levels::default());
    levels.load(&mut engine, &mut content, None, "Levels/Field.level").unwrap();
    engine.tick(1.0 / 60.0);
    let bones = walkers_bones(&engine);
    assert_eq!(
        bones,
        [("Body".into(), None), ("Head".into(), Some(0)), ("RightLeg".into(), Some(0)), ("LeftLeg".into(), Some(0))]
    );
    // Its Animator, with no clip to play, poses it at rest: the head two blocks' worth of pixels up.
    let pose = engine.world.query_ref::<&voxear_render::Pose>().into_iter().next().expect("posed by its Animator");
    assert!((pose.bones[1].w_axis.truncate() - Vec3::new(0.0, 1.5, 0.0)).length() < 1e-5);
}

#[test]
fn a_box_model_is_cooked_into_built_content_and_read_from_it() {
    let dir = project();
    let output = dir.path().join("Built");
    let settings = ContentSettings { merge_below_kb: 0, ..ContentSettings::default() };
    let report = content_build::build(dir.path(), &output, &settings, &[]).unwrap();
    assert_eq!((report.cooked, report.cached), (1, 0));
    assert!(report.bundles.iter().any(|bundle| bundle.assets.contains(&"Mobs/Walker.boxmodel".to_string())));

    let mut content = Content::open_bundles(dir.path(), &output).unwrap();
    let (mut engine, mut levels) = (Engine::new(), Levels::default());
    levels.load(&mut engine, &mut content, None, "Levels/Field.level").unwrap();
    engine.tick(1.0 / 60.0);
    assert_eq!(walkers_bones(&engine).len(), 4);

    // A box moved in the file is a model changed: it cooks again.
    let moved = walker().replace("from = [-4.0, 0.0, -4.0]", "from = [-4.0, 1.0, -4.0]");
    assert_ne!(moved, walker());
    std::fs::write(dir.path().join("Mobs/Walker.boxmodel"), moved).unwrap();
    let again = content_build::build(dir.path(), &output, &settings, &[]).unwrap();
    assert_eq!((again.cooked, again.cached), (1, 0));
}
