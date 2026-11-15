//! What reflection probes' captures see, on a real GPU: a probe captured at
//! its resolution and from its capture point; a metal's shine; the water of a
//! Pool and the plants of a Foliage round it; a sky drawn from a picture; and
//! the clouds, in reflections and in front of the ground round a camera
//! inside them. Each test skips itself (with a note) on machines without a
//! usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Clouds, Foliage, Impostors, Light, MaterialDesc, MaterialHandle,
    MeshHandle, MeshRenderer, Pool, PostProcess, Primitive, ProbePixels, ProbeResolution, ReflectionProbe, Renderer,
    RendererSettings, Sky,
};
use voxear_scene::{Plant, Plants, Transform};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FRAME: f32 = 1.0 / 60.0;

fn renderer() -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings::default()) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff) / 1024.0;
    match exponent {
        0 => sign * fraction * 2f32.powi(-14),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + fraction) * 2f32.powi(exponent - 15),
    }
}

/// The faces of a cube map, in the order a probe's are kept.
#[derive(Clone, Copy)]
enum Face {
    PlusX,
    MinusX,
    PlusY,
    MinusY,
    PlusZ,
}

/// The light a probe saw at the middle of one of its faces, averaged over a
/// few texels.
fn face_middle(pixels: &ProbePixels, face: Face) -> [f32; 3] {
    let size = pixels.size as usize;
    let mut sum = [0.0; 3];
    for dy in 0..2 {
        for dx in 0..2 {
            let at = face as usize * size * size + (size / 2 - 1 + dy) * size + size / 2 - 1 + dx;
            let texel = pixels.texels[at];
            for c in 0..3 {
                sum[c] += half_to_f32(texel[c]) / 4.0;
            }
        }
    }
    sum
}

/// A world with a camera at `eye` looking along +z, turned by `turn`, the sun
/// high, a plain gradient sky, and no anti-aliasing to wait for.
fn open_world(eye: Vec3) -> World {
    turned_world(eye, Quat::IDENTITY)
}

fn turned_world(eye: Vec3, turn: Quat) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(eye, turn, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.4).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.fog.enabled = false;
    world.insert_resource(post);
    world
}

fn material(
    renderer: &mut Renderer,
    base_color: [f32; 4],
    metallic: f32,
    roughness: f32,
    emissive: [f32; 3],
) -> MaterialHandle {
    renderer
        .add_material(&MaterialDesc { base_color, metallic, roughness, emissive, ..MaterialDesc::default() })
        .unwrap()
}

fn slab(world: &mut World, mesh: MeshHandle, material: MaterialHandle, centre: Vec3, size: Vec3) -> Entity {
    world.spawn((MeshRenderer::new(mesh, material), Transform::new(centre, Quat::IDENTITY, size)))
}

/// Renders until `probe` has been captured as it asks now, then reads what
/// it saw back.
fn captured(renderer: &mut Renderer, world: &World, probe: Entity, time: &mut f32) -> ProbePixels {
    let asked = world.get::<ReflectionProbe>(probe).unwrap().captures;
    for _ in 0..80 {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
        if renderer.reflection_probe_captured(probe) == Some(asked) {
            // A frame more, so the capture's passes have run on the GPU.
            *time += FRAME;
            renderer.render(world, *time).expect("frame renders");
            return renderer.reflection_probe_pixels(probe).unwrap().expect("captured");
        }
    }
    panic!("the probe was never captured");
}

fn capture_again(world: &mut World, probe: Entity) {
    world.get_mut::<ReflectionProbe>(probe).unwrap().capture();
}

fn reddish([r, g, b]: [f32; 3]) -> bool {
    r > 0.2 && r > 2.0 * g && r > 2.0 * b
}

#[test]
fn a_probe_is_captured_at_its_resolution_and_from_its_capture_point() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = open_world(Vec3::new(0.0, 1.0, -6.0));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let red = material(&mut renderer, [0.8, 0.1, 0.1, 1.0], 0.0, 0.9, [3.0, 0.1, 0.1]);
    slab(&mut world, cube, red, Vec3::new(3.0, 1.0, 0.0), Vec3::ONE);
    let probe = ReflectionProbe { resolution: ProbeResolution::Size256, ..ReflectionProbe::default() };
    let probe =
        world.spawn((probe, Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))));
    let mut time = 0.0;
    let middle = captured(&mut renderer, &world, probe, &mut time);
    assert_eq!((middle.size, middle.texels.len()), (256, 256 * 256 * 6), "captured at its resolution");
    let (east, west) = (face_middle(&middle, Face::PlusX), face_middle(&middle, Face::MinusX));
    eprintln!("from the middle: +x {east:?}, -x {west:?}");
    assert!(reddish(east) && !reddish(west), "the red cube to +x from the box's middle");

    // Taken from 3.9 m along x, past the cube, which is now behind it.
    world.get_mut::<ReflectionProbe>(probe).unwrap().capture_offset = Vec3::new(3.9, 0.0, 0.0);
    capture_again(&mut world, probe);
    let moved = captured(&mut renderer, &world, probe, &mut time);
    let (east, west) = (face_middle(&moved, Face::PlusX), face_middle(&moved, Face::MinusX));
    eprintln!("from the capture point: +x {east:?}, -x {west:?}");
    assert!(reddish(west) && !reddish(east), "the red cube to -x from the capture point");

    // Another resolution is captured again in its own array.
    world.get_mut::<ReflectionProbe>(probe).unwrap().resolution = ProbeResolution::Size64;
    capture_again(&mut world, probe);
    let small = captured(&mut renderer, &world, probe, &mut time);
    assert_eq!(small.size, 64);
    assert!(reddish(face_middle(&small, Face::MinusX)), "the same picture, smaller");
}

#[test]
fn a_metal_in_a_capture_shows_what_it_mirrors_rather_than_black() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = open_world(Vec3::new(0.0, 1.0, -6.0));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let green = material(&mut renderer, [0.1, 0.8, 0.1, 1.0], 0.0, 0.9, [0.1, 2.0, 0.1]);
    let metal = material(&mut renderer, [0.95, 0.95, 0.95, 1.0], 1.0, 0.1, [0.0; 3]);
    // A glowing green wall behind the probe, and a metal block in front of it, facing the wall.
    slab(&mut world, cube, green, Vec3::new(-4.0, 1.0, 0.0), Vec3::new(0.2, 6.0, 12.0));
    slab(&mut world, cube, metal, Vec3::new(2.5, 1.0, 0.0), Vec3::new(1.0, 4.0, 4.0));
    let probe = ReflectionProbe::default();
    let probe =
        world.spawn((probe, Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))));
    // What the metal mirrors comes from the probe round it as it was last captured: black before, the sky at the
    // first capture, then the wall.
    let mut time = 0.0;
    let first = face_middle(&captured(&mut renderer, &world, probe, &mut time), Face::PlusX);
    assert!(first.iter().sum::<f32>() > 0.3, "not black, from the first capture: {first:?}");
    capture_again(&mut world, probe);
    let pixels = captured(&mut renderer, &world, probe, &mut time);
    let [r, g, b] = face_middle(&pixels, Face::PlusX);
    eprintln!("the metal block: first {first:?}, then {:?}", [r, g, b]);
    assert!(g > 0.3 && g > 2.0 * r && g > 2.0 * b, "the metal mirrors the green wall: {:?}", [r, g, b]);
}

#[test]
fn water_and_plants_round_a_probe_show_in_its_capture() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = open_world(Vec3::new(0.0, 2.0, -4.0));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let pale = material(&mut renderer, [0.8, 0.8, 0.75, 1.0], 0.0, 0.9, [0.0; 3]);
    let red = material(&mut renderer, [0.8, 0.05, 0.05, 1.0], 0.0, 0.9, [0.0; 3]);
    // Pale ground three metres down.
    slab(&mut world, cube, pale, Vec3::new(0.0, -3.1, 0.0), Vec3::new(80.0, 0.2, 80.0));
    let probe = ReflectionProbe::default();
    let probe =
        world.spawn((probe, Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))));
    let mut time = 0.0;
    let dry = captured(&mut renderer, &world, probe, &mut time);
    let (ground, ahead) = (face_middle(&dry, Face::MinusY), face_middle(&dry, Face::PlusZ));

    // A pond from the ground to y = 0, and a hedge of red plants three metres along +z.
    world.spawn((Pool { size: [40.0, 40.0], depth: 3.0, ..Pool::default() }, Transform::default()));
    let hedge: Vec<Plant> =
        (-6..=6).flat_map(|x| (0..3).map(move |y| Plant::new([x as f32 * 0.5, y as f32, 0.0], 0.0, 1.0))).collect();
    let foliage = Foliage {
        mesh: cube,
        material: red,
        draw_distance: 80.0,
        fade_distance: 3.0,
        shadow_distance: 15.0,
        healthy_color: [1.0; 3],
        dry_color: [1.0; 3],
        color_spread: 2.0,
        impostors: Impostors::Off,
        impostor_distance: 60.0,
    };
    world.spawn((foliage, Plants::new(hedge), Transform::from_position(Vec3::new(0.0, 0.0, 3.0))));
    capture_again(&mut world, probe);
    let wet = captured(&mut renderer, &world, probe, &mut time);
    let (water, plants) = (face_middle(&wet, Face::MinusY), face_middle(&wet, Face::PlusZ));
    eprintln!("down: ground {ground:?}, water {water:?}; ahead: {ahead:?}, plants {plants:?}");
    let lighter = |c: [f32; 3]| c[0] + c[1] + c[2];
    assert!(
        lighter(water) < lighter(ground) * 0.7 && water[2] > water[0],
        "the pond, darker and bluer than the ground under it"
    );
    assert!(reddish(plants) && !reddish(ahead), "the hedge, where the sky was");
}

#[test]
fn a_sky_from_a_picture_is_what_a_probe_reflects() {
    let Some(mut renderer) = renderer() else { return };
    // Red over the horizon, blue under it.
    let (width, height) = (64u32, 32u32);
    let pixels: Vec<[f32; 4]> = (0..width * height)
        .map(|i| if i / width < height / 2 { [2.0, 0.1, 0.1, 1.0] } else { [0.1, 0.1, 2.0, 1.0] })
        .collect();
    let texture = renderer.add_hdr_texture("sky", (width, height), &pixels).unwrap();
    let light = voxear_render::picture_sky_light(&pixels, width, height);
    let mut world = open_world(Vec3::new(0.0, 1.0, -6.0));
    world.insert_resource(Sky { picture_texture: Some(texture), picture_light: Some(light), ..Sky::default() });
    let probe = ReflectionProbe::default();
    let probe =
        world.spawn((probe, Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))));
    let mut time = 0.0;
    let seen = captured(&mut renderer, &world, probe, &mut time);
    let (up, down) = (face_middle(&seen, Face::PlusY), face_middle(&seen, Face::MinusY));
    eprintln!("up {up:?}, down {down:?}");
    assert!(reddish(up), "the picture's red above");
    assert!(down[2] > 3.0 * down[0], "and its blue below");
}

/// A world with a sky of `cover` clouds, low enough that the camera, 60 m up,
/// is inside them where they are.
fn cloudy_world(renderer: &mut Renderer, cover: f32, low: bool) -> (World, Entity) {
    // Looking down at the ground ahead, for the camera in the clouds.
    let turn = if low { Quat::from_rotation_x(0.6) } else { Quat::IDENTITY };
    let mut world = turned_world(Vec3::new(0.0, if low { 60.0 } else { 2.0 }, 0.0), turn);
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grass = material(renderer, [0.2, 0.5, 0.1, 1.0], 0.0, 0.9, [0.0; 3]);
    slab(&mut world, cube, grass, Vec3::new(0.0, -0.1, 0.0), Vec3::new(4000.0, 0.2, 4000.0));
    let clouds = match low {
        true => Clouds { cover, height: 10.0, thickness: 400.0, size: 300.0, density: 1.0, ..Clouds::default() },
        false => Clouds { cover, ..Clouds::default() },
    };
    world.insert_resource(Sky { clouds, ..Sky::default() });
    let probe = ReflectionProbe::default();
    let probe =
        world.spawn((probe, Transform::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0))));
    (world, probe)
}

#[test]
fn clouds_show_in_a_probes_capture_of_the_sky() {
    let Some(mut renderer) = renderer() else { return };
    let mut time = 0.0;
    let (clear, probe) = cloudy_world(&mut renderer, 0.0, false);
    let clear = face_middle(&captured(&mut renderer, &clear, probe, &mut time), Face::PlusY);
    let Some(mut renderer) = self::renderer() else { return };
    let (overcast, probe) = cloudy_world(&mut renderer, 1.0, false);
    let overcast = face_middle(&captured(&mut renderer, &overcast, probe, &mut time), Face::PlusY);
    eprintln!("straight up: clear {clear:?}, overcast {overcast:?}");
    let blueness = |[r, _, b]: [f32; 3]| (b - r) / b.max(1e-4);
    assert!(blueness(overcast) < blueness(clear) * 0.6, "grey cloud where the clear sky was blue");
}

fn render_pixels(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    for _ in 0..frames {
        time += FRAME;
        renderer.render(world, time).expect("frame renders");
    }
    renderer.read_pixels().unwrap().2
}

#[test]
fn the_ground_is_seen_through_the_clouds_round_a_camera_inside_them() {
    let Some(mut renderer) = renderer() else { return };
    let (clear, _) = cloudy_world(&mut renderer, 0.0, true);
    let clear = render_pixels(&mut renderer, &clear, 12);
    let Some(mut renderer) = self::renderer() else { return };
    let (inside, _) = cloudy_world(&mut renderer, 1.0, true);
    let inside = render_pixels(&mut renderer, &inside, 12);
    let at = |pixels: &[u8]| {
        let i = ((HEIGHT * 3 / 4 * WIDTH + WIDTH / 2) * 4) as usize;
        [pixels[i] as f32, pixels[i + 1] as f32, pixels[i + 2] as f32]
    };
    let (clear, inside) = (at(&clear), at(&inside));
    eprintln!("the ground below: clear {clear:?}, in the clouds {inside:?}");
    // The grass's green is washed towards the cloud's grey.
    let greenness = |[r, g, b]: [f32; 3]| g - (r + b) / 2.0;
    assert!(greenness(inside) < greenness(clear) * 0.7, "the ground seen through cloud");
}
