//! Dimensions on a real GPU: two worlds of blocks at the same place, each in
//! a dimension of its own, and a camera that sees only the one it is in,
//! lit only by that dimension's lights. Each test skips itself (with a note)
//! on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings,
    Sky, SurfaceLook,
};
use voxear_scene::{DimensionId, InDimension, Transform};
use voxear_terrain::blocks::{
    BlockWorld,
    file::{BlockFile, BlockKind},
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
/// The second dimension.
const OTHER: DimensionId = DimensionId(1);

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn frames(renderer: &mut Renderer, world: &World, count: u32) -> Vec<u8> {
    for frame in 0..count {
        renderer.render(world, 1.0 + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

/// A field of blocks 64 a side, `height` high, all of one colour, its corner
/// at the world's middle.
fn field(renderer: &mut Renderer, world: &mut World, color: [f32; 3], height: u32) -> Entity {
    let mut file = BlockFile::default();
    file.blocks = vec![BlockKind::colored("Ground", color)];
    file.ground.size = [64, 64];
    file.ground.height = height;
    (file.ground.top, file.ground.under, file.ground.bottom) = (1, 1, 1);
    let blocks = file.make_ground(None);
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let material = renderer.add_material(&MaterialDesc {
                base_color: [look.color[0], look.color[1], look.color[2], 1.0],
                ..MaterialDesc::default()
            });
            SurfaceLook {
                material: material.unwrap(),
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
                leafy: false,
                mixed: false,
                tint: 0,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook {
        material: default,
        surfaces: Arc::from(surfaces),
        voxels: Default::default(),
        motions: Default::default(),
        light: Default::default(),
    };
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()))
}

/// A green field in the level's dimension and a red one, higher, at the same
/// place in the other, each under a sun of its own; the camera over them,
/// looking down across both.
fn worlds(renderer: &mut Renderer) -> (World, Entity, Entity) {
    let mut world = World::new();
    field(renderer, &mut world, [0.1, 0.7, 0.1], 4);
    let red = field(renderer, &mut world, [0.8, 0.1, 0.1], 12);
    world.insert(red, InDimension(OTHER));
    let towards_sun = Vec3::new(-0.4, 0.8, -0.3).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let other_sun = world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert(other_sun, InDimension(OTHER));
    // A lamp of the level's, bright blue, near the middle: nothing of it falls on the other dimension's ground.
    world.spawn((
        Light::point(Vec3::new(0.1, 0.2, 1.0), 400.0, 30.0),
        Transform::from_position(Vec3::new(32.0, 14.0, 32.0)),
    ));
    let eye = Vec3::new(32.0, 40.0, 0.0);
    let way = (Vec3::new(32.0, 0.0, 40.0) - eye).normalize();
    let turn = Quat::from_rotation_y(way.x.atan2(way.z)) * Quat::from_rotation_x(-way.y.asin());
    let camera = world.spawn((Camera::default(), Transform::new(eye, turn, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    post.global_illumination.enabled = false;
    world.insert_resource(post);
    (world, camera, red)
}

/// How many pixels are mostly green, and mostly red.
fn counts(pixels: &[u8]) -> (usize, usize) {
    let (mut green, mut red) = (0, 0);
    for pixel in pixels.chunks_exact(4) {
        let [r, g, b] = [pixel[0] as i32, pixel[1] as i32, pixel[2] as i32];
        green += usize::from(g > r + 30 && g > b + 30);
        red += usize::from(r > g + 30 && r > b + 30);
    }
    (green, red)
}

/// The pixel a point of the world shows at, seen from the camera.
fn on_screen(world: &World, camera: Entity, point: Vec3) -> (u32, u32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let at = *world.get::<Transform>(camera).unwrap();
    let view = Mat4::from_rotation_translation(at.rotation, at.position).inverse();
    let lens = Camera::default();
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let projection = vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), aspect, lens.near);
    let clip = projection * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    ((x * WIDTH as f32) as u32, (y * HEIGHT as f32) as u32)
}

/// How much bluer the ground `height` high is under the lamp than out of its
/// reach, by the field's far corner: the lamp's light on it, in the blue
/// channel's steps.
fn lamp_light(world: &World, camera: Entity, pixels: &[u8], height: f32) -> i32 {
    let blue = |(x, y): (u32, u32)| pixels[((y * WIDTH + x) * 4 + 2) as usize] as i32;
    let under = on_screen(world, camera, Vec3::new(32.0, height, 32.0));
    let edge = on_screen(world, camera, Vec3::new(6.0, height, 56.0));
    blue(under) - blue(edge)
}

#[test]
fn a_camera_sees_only_the_world_and_lights_of_the_dimension_it_is_in() {
    let Some(mut renderer) = renderer() else { return };
    let (mut world, camera, _) = worlds(&mut renderer);
    let level = frames(&mut renderer, &world, 4);
    keep_image("dimensions level.png", &level);
    let all = (WIDTH * HEIGHT) as usize;
    let (green, red) = counts(&level);
    assert!(green * 3 > all, "the level's green field fills the view: {green} of {all}");
    assert_eq!(red, 0, "nothing of the other dimension's red field shows");
    let lit = lamp_light(&world, camera, &level, 4.0);
    assert!(lit > 20, "the level's blue lamp lights its own ground: {lit} bluer under it");

    // The camera in the other dimension, at the same place: its red field alone, lit by its own sun, not the lamp.
    world.insert(camera, InDimension(OTHER));
    let other = frames(&mut renderer, &world, 4);
    keep_image("dimensions other.png", &other);
    let (green, red) = counts(&other);
    assert!(red * 3 > all, "the other dimension's red field fills the view: {red} of {all}");
    assert_eq!(green, 0, "nothing of the level's green field shows");
    let lit = lamp_light(&world, camera, &other, 12.0);
    assert!(lit.abs() <= 3, "the level's lamp, 2 m over this ground, lights nothing of it: {lit} bluer under it");

    // And back: the level's as it was within two frames, its world's chunks kept on the GPU all the while, and
    // nothing of the other's shadows on it.
    world.remove::<InDimension>(camera);
    let again = frames(&mut renderer, &world, 2);
    keep_image("dimensions again.png", &again);
    let (green, red) = counts(&again);
    assert!(green * 3 > all && red == 0, "the level's field again at once: {green} green, {red} red");
    let shaded =
        level.chunks_exact(4).zip(again.chunks_exact(4)).filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 12));
    let shaded = shaded.count();
    assert!(shaded * 200 < all, "the picture is as it was before: {shaded} of {all} pixels differ");
}
