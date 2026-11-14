//! Reflection probes on a real GPU: a mirror floor in a closed room shows
//! the room's walls once a probe is there, rather than the sky; with box
//! projection the walls' reflections line up with where the walls are; and a
//! probe captured once stays as it was until it is captured again, or loaded
//! from what it was baked as. Each test skips itself (with a note) on
//! machines without a usable Vulkan device. The probes are captured by the
//! GPU's rays where it has them, and in software too.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MaterialHandle, MeshRenderer, PostProcess, Primitive,
    ReflectionProbe, ReflectionProbeBake, Renderer, RendererSettings,
};
use voxear_scene::Transform;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FRAME: f32 = 1.0 / 60.0;
/// Enough frames for a probe to settle, be captured and be shaded with.
const SETTLE: u32 = 24;

fn renderer(software: bool) -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings { software_rays: software, ..Default::default() }) {
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

fn rgb(pixels: &[u8], x: u32, y: u32) -> [i32; 3] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [pixels[at] as i32, pixels[at + 1] as i32, pixels[at + 2] as i32]
}

fn run(renderer: &mut Renderer, world: &World, time: &mut f32, frames: u32) {
    for _ in 0..frames {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
    }
}

/// Where the boundary between the far wall's green and blue halves is.
const SPLIT: f32 = 2.0;

/// A closed room eight metres square and four high, lit by a lamp under its
/// ceiling: its far wall green to the left of x = 2 and blue to its right,
/// its other walls and ceiling grey, and a mirror for a floor. Its own
/// reflections are off, so the floor shows only the probe or the sky. The
/// camera stands at x = 2, over the far wall's boundary, looking ahead and
/// down at the floor.
struct Room {
    world: World,
    green_wall: Entity,
    red: MaterialHandle,
}

fn build_room(renderer: &mut Renderer) -> Room {
    let mut world = World::new();
    let eye = Vec3::new(SPLIT, 1.6, -3.0);
    world.spawn((Camera::default(), Transform::new(eye, Quat::from_rotation_x(0.45), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let mut material = |base_color: [f32; 4], metallic: f32, roughness: f32, emissive: [f32; 3]| {
        renderer
            .add_material(&MaterialDesc { base_color, metallic, roughness, emissive, ..MaterialDesc::default() })
            .unwrap()
    };
    let grey = material([0.5, 0.5, 0.5, 1.0], 0.0, 0.9, [0.0; 3]);
    let green = material([0.1, 0.8, 0.1, 1.0], 0.0, 0.9, [0.1, 1.5, 0.1]);
    let blue = material([0.1, 0.1, 0.8, 1.0], 0.0, 0.9, [0.1, 0.1, 1.5]);
    let red = material([0.8, 0.1, 0.1, 1.0], 0.0, 0.9, [1.5, 0.1, 0.1]);
    let mirror = material([0.95, 0.95, 0.95, 1.0], 1.0, 0.02, [0.0; 3]);
    let slab = |world: &mut World, material, centre: Vec3, size: Vec3| {
        world.spawn((MeshRenderer::new(cube, material), Transform::new(centre, Quat::IDENTITY, size)))
    };
    slab(&mut world, mirror, Vec3::new(0.0, -0.1, 0.0), Vec3::new(8.4, 0.2, 8.4));
    slab(&mut world, grey, Vec3::new(0.0, 4.1, 0.0), Vec3::new(8.4, 0.2, 8.4));
    slab(&mut world, grey, Vec3::new(-4.1, 2.0, 0.0), Vec3::new(0.2, 4.4, 8.4));
    slab(&mut world, grey, Vec3::new(4.1, 2.0, 0.0), Vec3::new(0.2, 4.4, 8.4));
    slab(&mut world, grey, Vec3::new(0.0, 2.0, -4.1), Vec3::new(8.4, 4.4, 0.2));
    let green_wall =
        slab(&mut world, green, Vec3::new((SPLIT - 4.0) / 2.0, 2.0, 4.1), Vec3::new(SPLIT + 4.0, 4.4, 0.2));
    slab(&mut world, blue, Vec3::new((SPLIT + 4.0) / 2.0, 2.0, 4.1), Vec3::new(4.0 - SPLIT, 4.4, 0.2));
    world.spawn((Light::point(Vec3::ONE, 40.0, 12.0), Transform::from_position(Vec3::new(0.0, 3.5, 0.0))));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.reflections.enabled = false;
    post.ambient_occlusion.enabled = false;
    world.insert_resource(post);
    Room { world, green_wall, red }
}

/// A probe filling the room, captured at its middle.
fn add_probe(world: &mut World, box_projection: bool) -> Entity {
    let probe = ReflectionProbe { box_projection, blend_distance: 0.0, ..ReflectionProbe::default() };
    world.spawn((probe, Transform::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))))
}

/// The floor's rows in the picture, where it mirrors the far wall.
const FLOOR_ROWS: std::ops::Range<u32> = 130..240;

/// How the floor's colour leans, on average over its rows: green less blue,
/// and blue and green over red.
fn floor_colour(pixels: &[u8]) -> [i32; 3] {
    let mut sum = [0; 3];
    let mut count = 0;
    for y in FLOOR_ROWS.step_by(4) {
        for x in (120..380).step_by(4) {
            let [r, g, b] = rgb(pixels, x, y);
            sum = [sum[0] + r, sum[1] + g, sum[2] + b];
            count += 1;
        }
    }
    sum.map(|channel| channel / count)
}

/// Where, across each of the floor's rows, its mirror turns from green to
/// blue, averaged over the rows where it does.
fn reflected_split(pixels: &[u8]) -> Option<f32> {
    let mut found = Vec::new();
    for y in FLOOR_ROWS.step_by(2) {
        let greener = |x: u32| {
            let [_, g, b] = rgb(pixels, x, y);
            g > b + 8
        };
        let bluer = |x: u32| {
            let [_, g, b] = rgb(pixels, x, y);
            b > g + 8
        };
        if let Some(x) = (8..WIDTH - 8).find(|&x| greener(x - 4) && bluer(x + 4)) {
            found.push(x as f32);
        }
    }
    (!found.is_empty()).then(|| found.iter().sum::<f32>() / found.len() as f32)
}

fn mirrors_the_room(software: bool) {
    let Some(mut renderer) = renderer(software) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer);
    run(&mut renderer, &room.world, &mut time, SETTLE);
    let without = renderer.read_pixels().unwrap().2;
    add_probe(&mut room.world, true);
    run(&mut renderer, &room.world, &mut time, SETTLE);
    let with = renderer.read_pixels().unwrap().2;
    let name = if software { "software" } else { "traced" };
    keep_image(&format!("room floor without a probe ({name}).png"), &without);
    keep_image(&format!("room floor with a probe ({name}).png"), &with);
    // Without the probe the floor mirrors the plain sky, a pale blue; with it, the far wall, mostly green.
    let [r0, g0, b0] = floor_colour(&without);
    let [r1, g1, b1] = floor_colour(&with);
    assert!(g1 > r1 + 30 && g1 > b1 + 15, "the floor mirrors the green far wall: {:?}", [r1, g1, b1]);
    assert!(b0 > g0 + 10, "without a probe, it mirrors the sky: {:?}", [r0, g0, b0]);
}

#[test]
fn a_mirror_floor_in_a_closed_room_shows_its_walls_once_a_probe_is_there() {
    mirrors_the_room(false);
}

#[test]
fn a_mirror_floor_shows_its_walls_from_a_probe_captured_in_software() {
    mirrors_the_room(true);
}

#[test]
fn box_projection_lines_the_walls_reflections_up_with_the_walls() {
    let renders: Vec<Option<f32>> = [true, false]
        .into_iter()
        .map(|box_projection| {
            let mut renderer = renderer(false)?;
            let mut time = 0.0;
            let mut room = build_room(&mut renderer);
            add_probe(&mut room.world, box_projection);
            run(&mut renderer, &room.world, &mut time, SETTLE);
            let pixels = renderer.read_pixels().unwrap().2;
            keep_image(&format!("room floor, box projection {box_projection}.png"), &pixels);
            Some(reflected_split(&pixels))
        })
        .collect::<Option<_>>()
        .unwrap_or_default();
    let [Some(projected), Some(plain)] = renders[..] else {
        assert!(renders.is_empty(), "the floor shows the far wall's split: {renders:?}");
        return;
    };
    // The camera stands over the split, so its reflection runs straight down the middle of the picture; looked up
    // from the probe's middle, two metres to the side, it lands well off to one side.
    let middle = WIDTH as f32 / 2.0;
    assert!((projected - middle).abs() < 6.0, "box projected, the split is in the middle: at {projected}");
    assert!((plain - middle).abs() > 30.0, "without it, the split is off to the side: at {plain}");
}

#[test]
fn a_probe_captured_once_stays_the_same_until_it_is_captured_again() {
    let Some(mut renderer) = renderer(false) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer);
    let probe = add_probe(&mut room.world, true);
    run(&mut renderer, &room.world, &mut time, SETTLE);
    let captured = renderer.read_pixels().unwrap().2;
    let baked = renderer.reflection_probe_pixels(probe).unwrap().expect("the probe has been captured");

    // The far wall turns red; the probe keeps what it saw.
    let red = room.red;
    room.world.get_mut::<MeshRenderer>(room.green_wall).unwrap().material = red;
    run(&mut renderer, &room.world, &mut time, SETTLE);
    let kept = renderer.read_pixels().unwrap().2;
    // Captured again, it sees the red.
    room.world.get_mut::<ReflectionProbe>(probe).unwrap().capture();
    run(&mut renderer, &room.world, &mut time, 4);
    let again = renderer.read_pixels().unwrap().2;
    keep_image("probe captured.png", &captured);
    keep_image("probe kept after the wall changed.png", &kept);
    keep_image("probe captured again.png", &again);
    let (before, after, recaptured) = (floor_colour(&captured), floor_colour(&kept), floor_colour(&again));
    let moved = (0..3).map(|c| before[c].abs_diff(after[c])).max().unwrap();
    assert!(moved <= 2, "the floor mirrors what the probe saw at first: {before:?} then {after:?}");
    assert!(recaptured[0] > before[0] + 15, "captured again, it mirrors the red wall: {before:?} to {recaptured:?}");

    // Loaded from what it was baked as, a probe shows what was captured, with no capture of its own.
    let Some(mut fresh) = renderer_again() else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut fresh);
    room.world.get_mut::<MeshRenderer>(room.green_wall).unwrap().material = room.red;
    let loaded = add_probe(&mut room.world, true);
    room.world.insert(loaded, ReflectionProbeBake(std::sync::Arc::new(baked)));
    run(&mut fresh, &room.world, &mut time, 4);
    let from_bake = fresh.read_pixels().unwrap().2;
    keep_image("probe loaded from its bake.png", &from_bake);
    let shown = floor_colour(&from_bake);
    let off = (0..3).map(|c| before[c].abs_diff(shown[c])).max().unwrap();
    assert!(off <= 2, "loaded, the floor mirrors the green wall it was baked with: {before:?} and {shown:?}");
}

fn renderer_again() -> Option<Renderer> {
    renderer(false)
}
