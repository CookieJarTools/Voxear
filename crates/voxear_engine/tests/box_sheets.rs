//! A box model drawn with a picture in place of its sheet, as a Model
//! Renderer's `sheet` asks: the picture's pixels on its faces and its tint
//! over them, the same picture drawn with one material however many Objs
//! wear it, and a picture of another size refused. Needs a GPU; skips
//! itself without one.

use std::sync::Arc;

use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::World;
use voxear_engine::{Content, prefab, scene_file::ModelData};
use voxear_render::{
    BoxModel, BoxSkin, Camera, Face, Light, ModelRenderer, Pose, Renderer, RendererSettings, Sheet, Sky,
};
use voxear_scene::Transform;

const SIZE: (u32, u32) = (160, 90);

/// A project with a box model a block across, its white front the upper
/// left of a sheet of 16 pixels each way, and pictures for its sheet.
fn project() -> tempfile::TempDir {
    let mut model = BoxModel { pixels_per_block: 8.0, sheet: Sheet::filled(16, 16, [255; 4]), ..BoxModel::default() };
    let mut faces = [None; 6];
    faces[Face::Front.index()] = Some([0.0, 0.0, 8.0, 8.0]);
    let body = model.add_bone("Body", None, Vec3::ZERO);
    model.add_box(body, Vec3::new(-4.0, 0.0, -2.0), [8, 8, 4], [0, 0]).skin = BoxSkin::Faces(faces);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("Faces")).unwrap();
    std::fs::write(dir.path().join("Faces/Plain.boxmodel"), model.to_text()).unwrap();
    let blue = Sheet::filled(16, 16, [40, 60, 220, 255]);
    std::fs::write(dir.path().join("Faces/Blue.png"), blue.to_png().unwrap()).unwrap();
    std::fs::write(dir.path().join("Faces/Small.png"), Sheet::filled(8, 8, [0; 4]).to_png().unwrap()).unwrap();
    dir
}

/// The middle of the picture, with `drawn` standing a few metres in front of
/// the camera.
fn seen(renderer: &mut Renderer, drawn: ModelRenderer) -> [i32; 3] {
    let mut world = World::new();
    let toward = Quat::from_rotation_y(std::f32::consts::PI);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 0.5, 3.0), toward, Vec3::ONE)));
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, toward, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 1.0, ..Sky::default() });
    let skeleton = &drawn.animation.as_ref().expect("a box model has bones").skeleton;
    let bones: Arc<[Mat4]> = skeleton.model_matrices(&skeleton.rest()).into();
    world.spawn((drawn, Pose { bones }, Transform::default()));
    let mut time = 0.0;
    for _ in 0..6 {
        time += 1.0 / 60.0;
        renderer.render(&world, time).unwrap();
    }
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let at = (((SIZE.1 / 2) * width + SIZE.0 / 2) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2]].map(i32::from)
}

#[test]
fn a_box_model_draws_a_picture_in_place_of_its_sheet() {
    let mut renderer = match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => renderer,
        Err(error) => return eprintln!("skipping: no usable GPU ({error:#})"),
    };
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let plain = ModelData::of("Faces/Plain.boxmodel");
    let blue = ModelData { sheet: Some("Faces/Blue.png".into()), ..plain.clone() };

    let own = prefab::model_renderer(&mut content, &mut renderer, &plain).unwrap();
    let pictured = prefab::model_renderer(&mut content, &mut renderer, &blue).unwrap();
    assert!(own.box_model && pictured.box_model);
    assert_ne!(own.parts[0].material, pictured.parts[0].material, "drawn with the picture");
    let again = prefab::model_renderer(&mut content, &mut renderer, &blue).unwrap();
    assert_eq!(again.parts[0].material, pictured.parts[0].material, "one material for every Obj wearing it");

    let small = ModelData { sheet: Some("Faces/Small.png".into()), ..plain.clone() };
    let refused = prefab::model_renderer(&mut content, &mut renderer, &small).unwrap_err();
    assert!(format!("{refused:#}").contains("is 8 × 8 pixels, but"), "{refused:#}");

    let white = seen(&mut renderer, own);
    assert!(white.iter().all(|&channel| channel > 150), "its own white sheet: {white:?}");
    let drawn = seen(&mut renderer, pictured.clone());
    assert!(drawn[2] > drawn[0] + 80 && drawn[2] > drawn[1] + 60, "the blue picture: {drawn:?}");
    // Tinted red over the blue, it goes dark: a tint multiplies the picture as it does the sheet.
    let tinted = seen(&mut renderer, ModelRenderer { tints: [[1.0, 0.2, 0.2], [1.0; 3], [1.0; 3]], ..pictured });
    assert!(tinted[2] < drawn[2] - 60, "the picture tinted: {tinted:?} from {drawn:?}");
}

fn offscreen() -> Option<Renderer> {
    match Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// The box model of [`project`], its sheet filled with `colour`, as its file has it.
fn plain_text(colour: [u8; 4]) -> String {
    let dir = project();
    let text = std::fs::read_to_string(dir.path().join("Faces/Plain.boxmodel")).unwrap();
    let mut model = BoxModel::parse(&text).unwrap();
    model.sheet = Sheet::filled(16, 16, colour);
    model.to_text()
}

#[test]
fn a_box_model_kept_while_only_its_picture_draws_keeps_its_own_material_and_takes_its_new_sheet_to_it() {
    let Some(mut renderer) = offscreen() else { return };
    let dir = project();
    let mut content = Content::open(dir.path()).unwrap();
    let plain = ModelData::of("Faces/Plain.boxmodel");
    let blue = ModelData { sheet: Some("Faces/Blue.png".into()), ..plain.clone() };
    // Drawn only with the blue picture: its mesh is in use, its own material is not.
    let pictured = prefab::model_renderer(&mut content, &mut renderer, &blue).unwrap();
    let mut world = World::new();
    world.spawn((pictured,));
    let released = content.release_unused(&world, Some(&mut renderer), &Default::default());
    assert_eq!(released.models, 0, "kept, as its mesh is drawn");
    // A few frames, for what was freed to be let go.
    for frame in 0..6 {
        renderer.render(&world, frame as f32 / 60.0).unwrap();
    }
    // Another box model loaded since, red, and then the first one's file changed to green.
    std::fs::write(dir.path().join("Faces/Red.boxmodel"), plain_text([220, 40, 40, 255])).unwrap();
    content.rescan().unwrap();
    let red = prefab::model_renderer(&mut content, &mut renderer, &ModelData::of("Faces/Red.boxmodel")).unwrap();
    std::fs::write(dir.path().join("Faces/Plain.boxmodel"), plain_text([40, 200, 40, 255])).unwrap();
    let guid = content.parse("Faces/Plain.boxmodel").unwrap().guid().unwrap();
    content.reimport_model(&mut renderer, guid).unwrap();
    // Each keeps its own picture.
    let seen_red = seen(&mut renderer, red);
    assert!(seen_red[0] > seen_red[1] + 80, "the red model stays red: {seen_red:?}");
    let own = prefab::model_renderer(&mut content, &mut renderer, &plain).unwrap();
    let green = seen(&mut renderer, own);
    assert!(green[1] > green[0] + 80, "the first model's own sheet, green now: {green:?}");
}

#[test]
fn a_clip_added_to_a_box_model_not_drawn_is_found_by_what_plays_it_once_its_file_is_read_again() {
    use voxear_engine::animation::{Animator, Clips, give_clips};
    let Some(mut renderer) = offscreen() else { return };
    let dir = project();
    std::fs::write(dir.path().join("Faces/Moves.boxmodel"), plain_text([255; 4])).unwrap();
    let mut content = Content::open(dir.path()).unwrap();
    let wave = "Faces/Moves.boxmodel#clip:Wave";
    let mut world = World::new();
    world.spawn((Animator { clip: wave.into(), ..Animator::default() },));
    give_clips(&mut world, &mut content);
    assert!(world.resource::<Clips>().unwrap().get(wave).is_none(), "no Wave yet");
    // Wave added to the file, which only lends its clips: read again as a changed model file is.
    let mut model = BoxModel::parse(&plain_text([255; 4])).unwrap();
    model.clips.push(voxear_render::BoxClip::new("Wave"));
    std::fs::write(dir.path().join("Faces/Moves.boxmodel"), model.to_text()).unwrap();
    let guid = content.parse("Faces/Moves.boxmodel").unwrap().guid().unwrap();
    content.reimport_model(&mut renderer, guid).unwrap();
    give_clips(&mut world, &mut content);
    let found = world.resource::<Clips>().unwrap().get(wave).map(|source| source.clip().name.clone());
    assert_eq!(found.as_deref(), Some("Wave"));
}
