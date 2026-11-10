//! Worlds of blocks drawn on a real GPU, offscreen. Each test skips itself
//! (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings,
    Sky, SurfaceLook,
};
use voxear_scene::Transform;
use voxear_terrain::blocks::{BlockWorld, file::BlockFile};

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

/// Where on screen a point is, seen straight down from `height` metres
/// above `(32, ·, 32)`: +x to the right, +z up the screen.
fn from_above(height: f32, x: f32, z: f32) -> (u32, u32) {
    let half = (30f32).to_radians().tan() * height;
    let u = (x - 32.0) / (half * WIDTH as f32 / HEIGHT as f32);
    let v = (z - 32.0) / half;
    (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
}

#[test]
fn a_field_of_blocks_is_drawn_in_its_blocks_colours_shaded_at_a_pillars_foot_and_shadowed_by_it() {
    let Some(mut renderer) = renderer() else { return };
    // Grass over dirt over stone, 64 blocks a side and 4 up, and a stone pillar 8 blocks up in its middle.
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    let mut blocks = file.make_ground(None);
    for y in 4..12 {
        for (x, z) in [(32, 32), (33, 32), (32, 33), (33, 33)] {
            blocks.set([x, y, z], 3);
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let material =
                renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
            SurfaceLook {
                material: material.unwrap(),
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces) };
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    // Straight down onto its middle from 40 m over the grass, +z up the screen.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 44.0, 32.0), down, Vec3::ONE)));
    // The sun low in the west (-x), shining east.
    let towards_sun = Vec3::new(-40f32.to_radians().cos(), 40f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 4);
    keep_image("blocks field.png", &pixels);
    let at = |x: f32, z: f32| {
        let (px, py) = from_above(40.0, x, z);
        rgb_at(&pixels, px, py)
    };
    // Open grass, lit: green.
    let [r, g, b] = at(12.0, 50.0);
    assert!(g > r && g > b && g > 60, "grass: {r} {g} {b}");
    // The pillar's top, nearer the camera: stone, grey.
    let [r, g, b] = at(33.0, 33.0);
    assert!((r - g).abs() < 20 && (g - b).abs() < 20 && r > 40, "the pillar's top: {r} {g} {b}");
    // East of the pillar, in its shadow; and at its foot on the west, in the sun but closed in.
    let brightness = |x: f32, z: f32| at(x, z).iter().sum::<i32>();
    let (open, shadowed) = (brightness(20.0, 33.0), brightness(38.0, 33.0));
    assert!(shadowed * 3 < open * 2, "east of the pillar in its shadow: {shadowed} against {open}");
    let foot = brightness(31.6, 33.0);
    assert!(foot < open, "darker at the pillar's foot: {foot} against {open}");
}
