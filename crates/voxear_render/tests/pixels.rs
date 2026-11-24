//! Orthographic cameras, drawing in pixels and snapping to the pixels' grid,
//! on a real GPU, rendering offscreen. Each test skips itself (with a note)
//! on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MeshRenderer, Pixels, PostProcess, Primitive,
    Projection, Renderer, RendererSettings,
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;
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

fn post(pixels: Pixels) -> PostProcess {
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    PostProcess { anti_aliasing, pixels, ..PostProcess::neutral() }
}

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn frames(renderer: &mut Renderer, world: &World, time: &mut f32, count: u32) -> Vec<u8> {
    for _ in 0..count {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
    }
    renderer.read_pixels().unwrap().2
}

/// A red cube 2 m ahead and a green one of the same size 12 m ahead, to its
/// right, on a grey floor under a sun, seen along +Z by `camera`.
fn two_cubes(renderer: &mut Renderer, camera: Camera, pixels: Pixels) -> (World, Entity) {
    let mut world = World::new();
    let eye = world.spawn((camera, Transform::from_position(Vec3::new(0.0, 0.5, -2.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let colour = |renderer: &mut Renderer, rgb: [f32; 3]| {
        let desc = MaterialDesc { base_color: [rgb[0], rgb[1], rgb[2], 1.0], ..MaterialDesc::default() };
        renderer.add_material(&desc).unwrap()
    };
    let (red, green, grey) =
        (colour(renderer, [0.9, 0.05, 0.05]), colour(renderer, [0.05, 0.9, 0.05]), colour(renderer, [0.4, 0.4, 0.4]));
    world.spawn((MeshRenderer::new(cube, red), Transform::from_position(Vec3::new(-1.2, 0.5, 0.0))));
    world.spawn((MeshRenderer::new(cube, green), Transform::from_position(Vec3::new(1.2, 0.5, 10.0))));
    let floor = Transform::new(Vec3::new(0.0, -0.05, 5.0), Quat::IDENTITY, Vec3::new(30.0, 0.1, 30.0));
    world.spawn((MeshRenderer::new(cube, grey), floor));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 2.5), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(post(pixels));
    (world, eye)
}

/// How many pixels across the widest row of mostly `channel` coloured
/// pixels is.
fn widest(pixels: &[u8], channel: usize) -> u32 {
    (0..HEIGHT)
        .map(|y| {
            (0..WIDTH)
                .filter(|x| {
                    let p = &pixels[((y * WIDTH + x) * 4) as usize..][..3];
                    let others = (0..3).filter(|c| *c != channel).map(|c| p[c]).max().unwrap();
                    p[channel] > 40 && p[channel] > others.saturating_mul(2)
                })
                .count() as u32
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn an_orthographic_camera_draws_things_the_same_size_however_far() {
    let Some(mut renderer) = renderer() else { return };
    let ortho = Camera { projection: Projection::Orthographic, size: 2.0, ..Camera::default() };
    let (world, _) = two_cubes(&mut renderer, ortho, Pixels::default());
    let picture = frames(&mut renderer, &world, &mut 0.0, 4);
    keep_image("orthographic cubes.png", &picture);
    let (near, far) = (widest(&picture, 0), widest(&picture, 1));
    // 4 m of height on 360 rows: a 1 m cube is 90 pixels across at every distance.
    assert!(near.abs_diff(90) <= 2, "the near cube is 90 pixels across: {near}");
    assert!(near.abs_diff(far) <= 1, "and so is the far one: {near} and {far}");

    let Some(mut renderer) = self::renderer() else { return };
    let (world, _) = two_cubes(&mut renderer, Camera::default(), Pixels::default());
    let picture = frames(&mut renderer, &world, &mut 0.0, 4);
    let (near, far) = (widest(&picture, 0), widest(&picture, 1));
    assert!(near > far * 3, "a perspective camera draws the far one smaller: {near} and {far}");
}

#[test]
fn drawn_in_pixels_each_block_of_screen_pixels_is_one_colour() {
    let Some(mut renderer) = renderer() else { return };
    let pixels = Pixels { enabled: true, size: 4, height: 0, snap: false };
    let (world, _) = two_cubes(&mut renderer, Camera::default(), pixels);
    let picture = frames(&mut renderer, &world, &mut 0.0, 4);
    keep_image("drawn in pixels.png", &picture);
    // 640 by 360 is 160 by 90 blocks of 4, which start at the corner when the sizes divide evenly.
    let at = |x: u32, y: u32| &picture[((y * WIDTH + x) * 4) as usize..][..3];
    let mut mixed = 0;
    for block_y in 0..HEIGHT / 4 {
        for block_x in 0..WIDTH / 4 {
            let first = at(block_x * 4, block_y * 4);
            let same = (0..16).all(|i| at(block_x * 4 + i % 4, block_y * 4 + i / 4) == first);
            mixed += usize::from(!same);
        }
    }
    assert_eq!(mixed, 0, "every block of 4 by 4 screen pixels is a single colour");

    // Drawn at the screen's own size, the cubes are as large and as coloured.
    let Some(mut renderer) = self::renderer() else { return };
    let (world, _) = two_cubes(&mut renderer, Camera::default(), Pixels::default());
    let full = frames(&mut renderer, &world, &mut 0.0, 4);
    let (small, large) = (widest(&picture, 0), widest(&full, 0));
    assert!(small.abs_diff(large) <= 4, "the near cube is as wide drawn in pixels: {small} and {large}");
}

/// Glides an orthographic camera right a tenth of a drawn pixel a frame and
/// keeps each frame, drawn in 4-pixel pixels, snapped or not.
fn glide(snap: bool) -> Option<Vec<Vec<u8>>> {
    let mut renderer = renderer()?;
    let ortho = Camera { projection: Projection::Orthographic, size: 2.0, ..Camera::default() };
    let pixels = Pixels { enabled: true, size: 4, height: 0, snap };
    let (mut world, eye) = two_cubes(&mut renderer, ortho, pixels);
    // A drawn pixel is 4 screen pixels; 360 screen pixels cover 4 m.
    let drawn_pixel = 4.0 * 4.0 / HEIGHT as f32;
    let mut time = 0.0;
    let mut kept = Vec::new();
    for frame in 0..12 {
        world.get_mut::<Transform>(eye).unwrap().position.x = frame as f32 * 0.1 * drawn_pixel;
        kept.push(frames(&mut renderer, &world, &mut time, 2));
    }
    Some(kept)
}

/// How many pixels in the middle of `picture` differ by more than a little
/// from `first` moved left by `shift` screen pixels.
fn moved_difference(first: &[u8], picture: &[u8], shift: u32) -> usize {
    let mut differing = 0;
    for y in 40..HEIGHT - 40 {
        for x in 40..WIDTH - 40 {
            let a = &first[((y * WIDTH + x + shift) * 4) as usize..][..3];
            let b = &picture[((y * WIDTH + x) * 4) as usize..][..3];
            differing += usize::from(a.iter().zip(b).any(|(a, b)| a.abs_diff(*b) > 6));
        }
    }
    differing
}

#[test]
fn snapping_moves_the_whole_picture_in_whole_screen_pixels_without_crawling() {
    let Some(snapped) = glide(true) else { return };
    keep_image("snapped first.png", &snapped[0]);
    keep_image("snapped last.png", &snapped[11]);
    // A tenth of a 4-pixel block a frame: the picture moves 0.4 screen pixels a frame, a whole one now and then, and
    // is otherwise the same picture.
    for (frame, picture) in snapped.iter().enumerate() {
        let shift = (frame as f32 * 0.4).round() as u32;
        let apart = moved_difference(&snapped[0], picture, shift);
        assert!(apart <= 40, "frame {frame}, moved {shift} pixels, is the first frame moved: {apart} pixels differ");
    }

    // Not snapped, the camera's glide changes which of the world's points each drawn pixel shows, and the edges
    // crawl.
    let Some(crawling) = glide(false) else { return };
    let crawled = (0..12).map(|frame| moved_difference(&crawling[0], &crawling[frame], (frame as f32 * 0.4).round() as u32));
    let most = crawled.max().unwrap();
    assert!(most > 150, "without snapping the picture is not the first moved: {most} pixels differ at most");
}
