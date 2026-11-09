//! Terrain drawn on a real GPU, offscreen. Each test skips itself (with a
//! note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings, Sky,
    TerrainLook, ViewOptions,
};
use voxear_scene::Transform;
use voxear_terrain::{Heightmap, Terrain};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn render(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..frames {
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn rgb_at(pixels: &[u8], x: u32, y: u32) -> [i32; 3] {
    let i = ((y * WIDTH + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
}

/// Rolling hills 1 km across, red, seen from low over them, so the ground
/// far off is drawn at coarse levels and near at the finest.
fn hills(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    let heights = Heightmap::from_fn(4, 4, 1.0, |x, z| {
        (x * 0.031).sin() * 14.0 + (z * 0.023).cos() * 11.0 + (x * 0.11 + z * 0.07).sin() * 2.5
    });
    let red = MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() };
    let material = renderer.add_material(&red).unwrap();
    let terrain = Terrain { detail_distance: 40.0, ..Terrain::new(heights) };
    world.spawn((
        terrain,
        TerrainLook { material, texture_size: 4.0 },
        Transform::from_position(Vec3::new(-512.0, 0.0, -100.0)),
    ));
    let look = Quat::from_rotation_x(0.12);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 32.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

#[test]
fn hills_are_drawn_whole_with_no_cracks_between_their_levels() {
    let Some(mut renderer) = renderer() else { return };
    let world = hills(&mut renderer);
    // Unlit, the ground is its own red and the sky anything else: a crack would let the sky through.
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain hills unlit.png", &pixels);
    let mut sky = Vec::new();
    for y in HEIGHT * 2 / 3..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = rgb_at(&pixels, x, y);
            if r < 150 || g > 60 || b > 60 {
                sky.push((x, y));
            }
        }
    }
    assert!(
        sky.is_empty(),
        "{} pixels of the bottom third are not ground, first {:?}",
        sky.len(),
        &sky[..sky.len().min(8)]
    );

    // Its levels of detail, each in its own colour: more than one, near and far.
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_LOD, ..ViewOptions::default() });
    let levels = render(&mut renderer, &world, 2);
    keep_image("terrain hills levels.png", &levels);
    let near = rgb_at(&levels, WIDTH / 2, HEIGHT - 4);
    let far = rgb_at(&levels, WIDTH / 2, HEIGHT / 2 + 12);
    assert_ne!(near, far, "the ground near and far is drawn at different levels");
}

#[test]
fn a_ridge_shades_the_ground_on_its_far_side_from_the_sun() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Flat ground 128 m across with a ridge 20 m high along z at x = 60 to 64.
    let heights = Heightmap::from_fn(1, 1, 0.5, |x, _| if (60.0..=64.0).contains(&x) { 20.0 } else { 0.0 });
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((Terrain::new(heights), TerrainLook { material: grey, texture_size: 4.0 }, Transform::default()));
    // Straight down onto its middle, within the sun's shadow distance, +z up the screen and +x to the right.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(64.0, 60.0, 64.0), down, Vec3::ONE)));
    // The sun low in the west (-x), shining east.
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain ridge shadow.png", &pixels);
    // Where on screen a point of the ground is, from the camera 60 m above (64, 64).
    let half = (30f32).to_radians().tan() * 60.0;
    let pixel = |x: f32, z: f32| {
        let u = (x - 64.0) / (half * WIDTH as f32 / HEIGHT as f32);
        let v = (z - 64.0) / half;
        (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
    };
    let brightness = |(x, z): (f32, f32)| {
        let (px, py) = pixel(x, z);
        rgb_at(&pixels, px, py).iter().sum::<i32>()
    };
    let (lit, shaded) = (brightness((30.0, 64.0)), brightness((80.0, 64.0)));
    assert!(shaded * 2 < lit, "the ground east of the ridge is in its shadow: {shaded} against {lit} lit");
}
