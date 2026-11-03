//! A scene's colour lookup table and post volumes: loaded with the level,
//! and written back when the scene is saved.

use voxear_engine::{Engine, Game, Project};
use voxear_render::{PostProcess, PostVolume, Renderer, RendererSettings, VolumeShape};
use voxear_scene::Name;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let cave = "format = 1\n\n[post.grading]\nlut = \"Looks/Neutral.png\"\nlut_contribution = 0.5\n\n\
                [[obj]]\nid = 1\nname = \"Cave\"\nscale = [8.0, 4.0, 8.0]\n[obj.post_volume]\nshape = \"sphere\"\n\
                blend_distance = 3.0\npriority = 2\n[obj.post_volume.post]\nbloom = { intensity = 0.2 }\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Looks\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Cave.scene\"\n"),
        ("Scenes/Cave.scene", cave),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    // A neutral lookup table, 256×16.
    let size = 16u32;
    let level = |value: u32| (value * 255 / (size - 1)) as u8;
    let mut rgba = Vec::new();
    for green in 0..size {
        for blue in 0..size {
            for red in 0..size {
                rgba.extend([level(red), level(green), level(blue), 255]);
            }
        }
    }
    let picture = dir.path().join("Looks/Neutral.png");
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(std::fs::File::create(&picture).unwrap(), size * size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    dir
}

#[test]
fn a_scenes_lookup_table_and_post_volume_load_and_save() {
    let Ok(mut renderer) = Renderer::new_offscreen(64, 64, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();
    game.prepare_post(&mut renderer);

    let post = *game.engine.world.resource::<PostProcess>().unwrap();
    assert!(post.grading.lut.is_some(), "the lookup table is uploaded and set");
    assert_eq!(post.grading.lut_contribution, 0.5);

    let world = &game.engine.world;
    let (cave, volume) = world
        .query_ref::<(voxear_ecs::Entity, &Name, &PostVolume)>()
        .into_iter()
        .find(|(_, name, _)| name.0 == "Cave")
        .map(|(entity, _, volume)| (entity, volume.clone()))
        .expect("the cave's volume is loaded");
    assert_eq!((volume.shape, volume.blend_distance, volume.priority), (VolumeShape::Sphere, 3.0, 2));
    renderer.render(world, 0.0).unwrap();

    let saved = voxear_engine::scene_file::capture(&game.engine.world, &mut game.content, &[cave]);
    let written = saved.objs[0].post_volume.as_ref().expect("the volume is saved");
    assert_eq!(written.component().unwrap(), volume);

    // A setting that does not exist is an error when the scene loads, not a silent nothing.
    std::fs::write(
        dir.path().join("Scenes/Cave.scene"),
        "format = 1\n[[obj]]\nid = 1\nname = \"Cave\"\n[obj.post_volume.post]\nbloom = { glow = 1.0 }\n",
    )
    .unwrap();
    let mut broken = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    assert!(broken.start(Some(&mut renderer)).is_err());
}
