//! Voxel Volumes drawn on a real GPU, offscreen. Each test skips itself
//! (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AmbientOcclusion, AntiAliasing, AntiAliasingMode, BoxBone, BoxModel, Camera, Light, ModelBox, PostProcess,
    Renderer, RendererSettings, Sky, VoxelGround, VoxelStamp, VoxelVolume, Voxels,
};
use voxear_scene::Transform;

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

/// Renders until every volume is made, then `frames` more.
fn render(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    let started = std::time::Instant::now();
    loop {
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
        if !renderer.voxel_volumes_making() {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "the volume is made within a minute");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
/// above `(8, ·, 8)`: +x to the right, +z up the screen.
fn from_above(height: f32, x: f32, z: f32) -> (u32, u32) {
    let half = (30f32).to_radians().tan() * height;
    let u = (x - 8.0) / (half * WIDTH as f32 / HEIGHT as f32);
    let v = (z - 8.0) / half;
    (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
}

/// A red pillar of cubes, 1/8 m each, a metre square and three metres tall.
fn pillar() -> BoxModel {
    let mut model = BoxModel { pixels_per_block: 8.0, ..BoxModel::default() };
    let mut bone = BoxBone::new("root", None, Vec3::ZERO);
    bone.boxes.push(ModelBox {
        voxels: Some(Voxels::filled([8, 24, 8], 1)),
        ..ModelBox::new(Vec3::new(-4.0, 0.0, -4.0), [8, 24, 8], [0, 0])
    });
    model.bones.push(bone);
    model.palette.push([220, 30, 30, 255]);
    model
}

/// A volume 16 m square with flat bare ground a metre up and the pillar in
/// its middle, seen straight down from 20 m over the ground, the sun low in
/// the west.
fn field(ambient_occlusion: bool) -> World {
    let mut world = World::new();
    let ground = VoxelGround { height: 1.0, hills: 0.0, grass: 0.0, flowers: 0.0, ..VoxelGround::default() };
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 6.0, 16.0),
        ground: Some(ground),
        stamps: vec![VoxelStamp { model: Arc::new(pillar()), at: Vec3::new(8.0, 1.0, 8.0), turn: 0.0, scale: 1.0 }],
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(8.0, 21.0, 8.0), down, Vec3::ONE)));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let ao = AmbientOcclusion { enabled: ambient_occlusion, ..AmbientOcclusion::default() };
    world.insert_resource(PostProcess { anti_aliasing, ambient_occlusion: ao, ..PostProcess::neutral() });
    world
}

fn check_field(pixels: &[u8]) {
    let at = |x: f32, z: f32| {
        let (px, py) = from_above(20.0, x, z);
        rgb_at(pixels, px, py)
    };
    // The ground's top, lit: green.
    let [r, g, b] = at(3.0, 12.0);
    assert!(g > r && g > b && g > 50, "the ground: {r} {g} {b}");
    // The pillar's top: red.
    let [r, g, b] = at(8.0, 8.0);
    assert!(r > g * 2 && r > b * 2 && r > 60, "the pillar's top: {r} {g} {b}");
    // East of the pillar, in its traced shadow, darker than open ground the same distance west.
    let brightness = |x: f32, z: f32| at(x, z).iter().sum::<i32>();
    let (open, shadowed) = (brightness(4.0, 8.0), brightness(11.5, 8.0));
    assert!(shadowed * 3 < open * 2, "east of the pillar in its shadow: {shadowed} against {open}");
    // Off the volume: the sky, not green.
    let (px, py) = from_above(20.0, -6.0, 8.0);
    let [r, g, b] = rgb_at(pixels, px.min(WIDTH - 1), py);
    assert!(!(g > r + 20 && g > b + 20), "off the volume is not ground: {r} {g} {b}");
}

#[test]
fn a_volumes_ground_and_a_stamped_model_are_drawn_lit_and_the_model_shadows_the_ground() {
    let Some(mut renderer) = renderer() else { return };
    let world = field(false);
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel field.png", &pixels);
    check_field(&pixels);
    let counts = renderer.stats().voxels;
    assert_eq!(counts.volumes, 1);
    assert!(counts.bricks > 0 && counts.bytes > 0, "{counts:?}");
}

#[test]
fn a_volume_is_drawn_the_same_through_the_depth_pass_with_ambient_occlusion_on() {
    let Some(mut renderer) = renderer() else { return };
    let world = field(true);
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel field with ao.png", &pixels);
    check_field(&pixels);
}

#[test]
fn a_volume_changed_is_made_again_and_one_taken_away_is_drawn_no_more() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = field(false);
    render(&mut renderer, &world, 1);
    let entity = world.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap().0;
    // The pillar moved west: its top now red where the ground was.
    world.get_mut::<VoxelVolume>(entity).unwrap().stamps[0].at = Vec3::new(4.0, 1.0, 8.0);
    let pixels = render(&mut renderer, &world, 2);
    let (px, py) = from_above(20.0, 4.0, 8.0);
    let [r, g, _] = rgb_at(&pixels, px, py);
    assert!(r > g * 2, "the pillar moved: {r} {g}");
    world.remove::<VoxelVolume>(entity);
    let pixels = render(&mut renderer, &world, 2);
    let (px, py) = from_above(20.0, 3.0, 12.0);
    let [r, g, b] = rgb_at(&pixels, px, py);
    assert!(!(g > r + 20 && g > b + 20), "the volume is gone: {r} {g} {b}");
    assert_eq!(renderer.stats().voxels.volumes, 0);
}

/// Ground with grass and flowers seen straight down from 12 m over `(x, z)`,
/// on a volume at the world's middle: endless, or 16 m square.
fn ground_from_above(endless: bool, x: f32, z: f32) -> World {
    let mut world = World::new();
    let ground = VoxelGround { height: 2.0, hills: 1.0, grass: 0.6, flowers: 0.3, ..VoxelGround::default() };
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 8.0, 16.0),
        ground: Some(ground),
        endless,
        reach: 24.0,
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(x, 14.0, z), down, Vec3::ONE)));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

#[test]
fn an_endless_volumes_ground_made_on_the_gpu_is_the_ground_a_volume_makes_on_the_cpu() {
    let Some(mut first) = renderer() else { return };
    let made_here = render(&mut first, &ground_from_above(false, 8.0, 8.0), 3);
    drop(first);
    let Some(mut second) = renderer() else { return };
    let endless = render(&mut second, &ground_from_above(true, 8.0, 8.0), 3);
    keep_image("voxel ground on the cpu.png", &made_here);
    keep_image("voxel ground on the gpu.png", &endless);
    // Over the middle of the 16 m square, well inside it: the same ground, flowers and shadows.
    let (mut far, mut count) = (0, 0);
    for y in HEIGHT / 4..HEIGHT * 3 / 4 {
        for x in WIDTH / 2 - HEIGHT / 4..WIDTH / 2 + HEIGHT / 4 {
            let (a, b) = (rgb_at(&made_here, x, y), rgb_at(&endless, x, y));
            far += usize::from((0..3).any(|i| (a[i] - b[i]).abs() > 24));
            count += 1;
        }
    }
    assert!(far * 200 < count, "{far} of {count} pixels differ by more than 24 levels");
    let [r, g, b] = rgb_at(&endless, WIDTH / 2, HEIGHT / 2);
    assert!(g > r && g > b, "grass under the camera: {r} {g} {b}");
}

#[test]
fn an_endless_volume_makes_its_ground_round_the_camera_wherever_it_goes() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = ground_from_above(true, 8.0, 8.0);
    render(&mut renderer, &world, 1);
    let camera = world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().next().unwrap().0;
    // Two hundred metres east and as far south of where it started, and below the world's middle.
    world.get_mut::<Transform>(camera).unwrap().position = Vec3::new(208.0, 14.0, -192.0);
    let pixels = render(&mut renderer, &world, 2);
    keep_image("voxel ground moved.png", &pixels);
    // The sky is a mid grey here; ground is green, flowers, or grass in its blades' shadow.
    let sky = (0..HEIGHT)
        .step_by(8)
        .flat_map(|y| (0..WIDTH).step_by(8).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b] = rgb_at(&pixels, x, y);
            (r - g).abs() < 10 && (g - b).abs() < 10 && (100..180).contains(&r)
        })
        .count();
    let all = (HEIGHT / 8 * WIDTH / 8) as usize;
    assert!(sky * 100 < all, "ground under all of the view: {sky} of {all} pixels are sky");
    assert!(renderer.stats().voxels.bytes > 0);
}
