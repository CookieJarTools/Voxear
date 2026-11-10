//! The world's origin moving (see `voxear_scene::WorldOrigin`) on a real
//! GPU: what the renderer keeps from last frame moves with everything else,
//! so the frame after a move is the frame before it, with no blur. Each test
//! skips itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::{IVec3, Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MotionBlur, PostProcess, Renderer, RendererSettings,
    TerrainLook,
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

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn differing_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2)).count()
}

/// Hills lit from the side, seen from low over them 2 km out, with a row of
/// posts in front, drawn with full motion blur and no TAA: anything the
/// renderer took for motion would smear them.
fn hills_far_out(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    let out = Vec3::new(2048.0, 0.0, 0.0);
    let heights = Heightmap::from_fn(4, 4, 1.0, |x, z| {
        (x * 0.031).sin() * 14.0 + (z * 0.023).cos() * 11.0 + (x * 0.11 + z * 0.07).sin() * 2.5
    });
    let red = renderer.add_material(&MaterialDesc { base_color: [1.0, 0.1, 0.1, 1.0], ..MaterialDesc::default() });
    let terrain = Terrain { detail_distance: 40.0, ..Terrain::new(heights) };
    world.spawn((
        terrain,
        TerrainLook::new(red.unwrap(), 4.0),
        Transform::from_position(out + Vec3::new(-512.0, 0.0, -100.0)),
    ));
    let cube = renderer.primitive(voxear_render::Primitive::Cube).unwrap();
    let white = renderer.add_material(&MaterialDesc::default()).unwrap();
    for post in 0..12 {
        let at = out + Vec3::new(post as f32 * 1.5 - 8.0, 30.0, 14.0);
        world.spawn((
            voxear_render::MeshRenderer::new(cube, white),
            Transform::new(at, Quat::IDENTITY, Vec3::new(0.5, 3.0, 0.5)),
        ));
    }
    let look = Quat::from_rotation_x(0.12);
    world.spawn((Camera::default(), Transform::new(out + Vec3::new(0.0, 32.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let motion_blur = MotionBlur { enabled: true, intensity: 1.0 };
    world.insert_resource(PostProcess { anti_aliasing, motion_blur, ..PostProcess::neutral() });
    world
}

/// The last of four still frames, then the frame after the world moves 2 km
/// back to the middle, the renderer told of it when `told`.
fn before_and_after_a_move(told: bool) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut renderer = renderer()?;
    let mut world = hills_far_out(&mut renderer);
    let mut time = 0.0;
    for _ in 0..4 {
        time += 1.0 / 60.0;
        renderer.render(&world, time).expect("frame renders");
    }
    let before = renderer.read_pixels().expect("pixels").2;
    let by = IVec3::new(-2048, 0, 0);
    voxear_scene::move_objs(&mut world, by);
    voxear_scene::propagate_transforms(&mut world);
    if told {
        renderer.shift_origin(by.as_vec3());
    }
    renderer.render(&world, time + 1.0 / 60.0).expect("frame renders");
    let after = renderer.read_pixels().expect("pixels").2;
    Some((before, after))
}

#[test]
fn the_frame_after_the_origin_moves_is_the_frame_before_it_with_no_blur() {
    let Some((before, after)) = before_and_after_a_move(true) else { return };
    keep_image("origin before.png", &before);
    keep_image("origin after.png", &after);
    let changed = differing_pixels(&before, &after);
    let all = (WIDTH * HEIGHT) as usize;
    assert!(changed * 1000 < all, "{changed} of {all} pixels changed as the world moved");

    // Untold, the renderer takes the move for 2 km of motion in a frame, and blurs the posts across the ground.
    let Some((before, smeared)) = before_and_after_a_move(false) else { return };
    keep_image("origin untold.png", &smeared);
    let changed = differing_pixels(&before, &smeared);
    assert!(changed * 20 > all, "only {changed} of {all} pixels smeared: the test would not see a blur");
}
