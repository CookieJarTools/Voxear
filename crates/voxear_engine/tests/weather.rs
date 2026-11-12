//! The weather's rain and snow, drawn: an engine with a camera over a wide
//! ground, ticked into a storm, with a renderer drawing each frame. Skips
//! itself without a GPU. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the
//! pictures.

use voxear_core::math::{Quat, Vec3};
use voxear_engine::{
    Engine,
    weather::{Weather, WeatherAmounts, WeatherKind},
};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MeshRenderer, PostProcess, Primitive, Renderer,
    RendererSettings, Sky, SkyMode,
};
use voxear_scene::Transform;

const SIZE: (u32, u32) = (640, 360);

/// What a camera at head height sees a few seconds after a dull, dry day turns to weather adding `amounts`.
fn picture(amounts: WeatherAmounts) -> Option<Vec<u8>> {
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return None;
    };
    let mut engine = Engine::new();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let material = renderer.add_material(&MaterialDesc { roughness: 1.0, ..MaterialDesc::default() }).unwrap();
    let world = &mut engine.world;
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.7, 0.0))));
    let down = Quat::from_rotation_x(50f32.to_radians());
    world.spawn((Light::directional(Vec3::ONE, 1.0), Transform::new(Vec3::ZERO, down, Vec3::ONE)));
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(4000.0))));
    world.insert_resource(Sky { mode: SkyMode::Atmosphere, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let mut weather = Weather::custom(dull_day());
    weather.set_amounts(amounts, 0.0);
    world.insert_resource(weather);
    // Long enough for the first drops to reach the ground and the first flakes to drift down past the camera.
    for _ in 0..240 {
        engine.tick(1.0 / 60.0);
        renderer.render(&engine.world, engine.time().elapsed as f32).expect("frame renders");
    }
    Some(renderer.read_pixels().unwrap().2)
}

fn keep(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, SIZE.0, SIZE.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// A day with rain's sky and wind and nothing falling.
fn dull_day() -> WeatherAmounts {
    WeatherAmounts { rain: 0.0, snow: 0.0, ..WeatherKind::Rain.amounts() }
}

/// How many pixels of two pictures' skies are plainly unlike: the rows above the horizon, where what has fallen on
/// the ground does not show.
fn unlike(a: &[u8], b: &[u8]) -> usize {
    let sky = (SIZE.0 * 170 * 4) as usize;
    let pairs = a[..sky].chunks_exact(4).zip(b[..sky].chunks_exact(4));
    pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 5)).count()
}

#[test]
fn rain_and_snow_fall_in_front_of_the_camera() {
    // The same dull day three ways, so only what falls differs.
    let day = dull_day();
    let Some(dry) = picture(day) else { return };
    let wet = picture(WeatherAmounts { rain: 1.0, ..day }).unwrap();
    let snowy = picture(WeatherAmounts { snow: 1.0, ..day }).unwrap();
    keep("weather dry.png", &dry);
    keep("weather rain.png", &wet);
    keep("weather snow.png", &snowy);
    let (rain, snow) = (unlike(&wet, &dry), unlike(&snowy, &dry));
    assert!(rain > 1000, "rain shows all over the sky: {rain} pixels");
    assert!(snow > 300, "and snow: {snow} pixels");
    assert!(unlike(&wet, &snowy) > 800, "which do not look alike");
}
