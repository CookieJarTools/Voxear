//! A light's cookie and IES profile, named by a scene: loaded with the
//! level, and written back when the scene is saved.

use voxear_engine::{Engine, Game, Project, scene_file::LightData};
use voxear_render::{Light, Renderer, RendererSettings};
use voxear_scene::Name;

const DOWNLIGHT: &str = "IESNA:LM-63-2002\nTILT=NONE\n1 1000 1.0 7 1 1 2 0.1 0.1 0.0\n1.0 1.0 12\n\
                         0 15 30 45 60 75 90\n0\n1000 950 800 500 200 50 0\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let lamp = "format = 1\n[[obj]]\nid = 1\nname = \"Lamp\"\n[obj.light]\ntype = \"spot\"\n\
                cookie = \"Pictures/Grid.png\"\nprofile = \"Lights/Down.ies\"\n";
    let files = [
        ("voxear.toml", "[project]\nname = \"Lamps\"\nstart_level = \"Levels/Main.level\"\n"),
        ("Levels/Main.level", "format = 1\n[[scene]]\nscene = \"Scenes/Lamp.scene\"\n"),
        ("Scenes/Lamp.scene", lamp),
        ("Lights/Down.ies", DOWNLIGHT),
    ];
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    // A small chequered picture, written as PNG.
    let picture = dir.path().join("Pictures/Grid.png");
    std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
    let file = std::fs::File::create(&picture).unwrap();
    let mut encoder = png::Encoder::new(file, 8, 8);
    encoder.set_color(png::ColorType::Rgba);
    let rgba: Vec<u8> =
        (0..64).flat_map(|i| if (i % 8 + i / 8) % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] }).collect();
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    dir
}

#[test]
fn a_lights_cookie_and_profile_load_with_its_level_and_save_with_its_scene() {
    let Ok(mut renderer) = Renderer::new_offscreen(64, 64, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let dir = project();
    let mut game = Game::new(Project::open(dir.path()).unwrap(), Engine::new()).unwrap();
    game.start(Some(&mut renderer)).unwrap();

    let world = &game.engine.world;
    let (lamp, light) = world
        .query_ref::<(voxear_ecs::Entity, &Name, &Light)>()
        .into_iter()
        .find(|(_, name, _)| name.0 == "Lamp")
        .map(|(entity, _, light)| (entity, *light))
        .expect("the lamp is loaded");
    assert!(light.cookie.is_some(), "its cookie is uploaded");
    assert!(light.profile.is_some(), "its profile is uploaded");
    renderer.render(world, 0.0).unwrap();

    let saved = voxear_engine::scene_file::capture(&game.engine.world, &mut game.content, &[lamp]);
    let Some(LightData::Spot { cookie: Some(cookie), profile: Some(profile), .. }) = &saved.objs[0].light else {
        panic!("the saved light names both: {:?}", saved.objs[0].light);
    };
    let path = |reference: &str| {
        let guid = game.content.parse(reference).unwrap().guid().unwrap();
        game.content.db().get(guid).unwrap().path.clone()
    };
    assert_eq!(path(cookie), "Pictures/Grid.png");
    assert_eq!(path(profile), "Lights/Down.ies");
}
