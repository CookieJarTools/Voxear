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

/// The opening's rectangle in the picture, `inset` pixels in from its edges,
/// as the camera sees the Portal on `portal`.
fn opening(world: &World, camera: Entity, portal: Entity, inset: i32) -> [u32; 4] {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let at = *world.get::<Transform>(camera).unwrap();
    let pose = voxear_render::PortalPose::of_obj(world, portal).unwrap();
    let size = world.get::<voxear_render::Portal>(portal).unwrap().size;
    let view = Mat4::from_rotation_translation(at.rotation, at.position).inverse();
    let lens = Camera::default();
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let projection = vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), aspect, lens.near);
    let [x0, y0, x1, y1] = voxear_render::portals::rect_in_view(pose.corners(size), projection * view).expect("in view");
    let px = |x: f32| ((x * 0.5 + 0.5) * WIDTH as f32) as i32;
    let py = |y: f32| ((y * 0.5 + 0.5) * HEIGHT as f32) as i32;
    let clamp = |v: i32, most: u32| v.clamp(0, most as i32) as u32;
    [clamp(px(x0) + inset, WIDTH), clamp(py(y0) + inset, HEIGHT), clamp(px(x1) - inset, WIDTH), clamp(py(y1) - inset, HEIGHT)]
}

/// How many pixels of a rectangle differ between two pictures by more than
/// `by` in any channel, and how many it holds.
fn differing(a: &[u8], b: &[u8], [x0, y0, x1, y1]: [u32; 4], by: u8) -> (usize, usize) {
    let (mut differ, mut all) = (0, 0);
    for y in y0..y1 {
        for x in x0..x1 {
            let at = ((y * WIDTH + x) * 4) as usize;
            all += 1;
            differ += usize::from((0..3).any(|c| a[at + c].abs_diff(b[at + c]) > by));
        }
    }
    (differ, all)
}

/// The level's sky, a plain blue gradient, and the other dimension's, the
/// air lit by a low sun of its own, reddening.
fn skies() -> (Sky, Sky) {
    let level = Sky { ambient: 0.3, ..Sky::default() };
    let other = Sky { mode: voxear_render::SkyMode::Atmosphere, ambient: 0.3, haze: 3.0, ..Sky::default() };
    (level, other)
}

/// Makes `dimension` the one the world is seen from, as the engine does
/// when the camera goes into it: the world's sky becomes its own, and the
/// other's is kept for views through Portals into it.
fn seen_from(world: &mut World, dimension: DimensionId) {
    let (level, other) = skies();
    let (own, elsewhere, kept) = match dimension.is_level() {
        true => (level, OTHER, other),
        false => (other, DimensionId::LEVEL, level),
    };
    world.insert_resource(own);
    world.insert_resource(voxear_scene::CurrentDimension(dimension));
    let look = voxear_render::DimensionLook { sky: kept, sun: None, post: world.resource::<PostProcess>().copied(), weather: Default::default() };
    world.insert_resource(voxear_render::DimensionLooks([(elsewhere, look)].into_iter().collect()));
}

#[test]
fn a_portal_into_another_dimension_shows_its_ground_under_its_own_sky_and_back() {
    use voxear_render::{Carry, Portal, PortalPose, portals::PortalQuality};
    let Some(mut renderer) = renderer() else { return };
    let (mut world, camera, _) = worlds(&mut renderer);
    // The level's lamp is no part of this.
    let lamps: Vec<Entity> = world.query_ref::<(Entity, &Light)>().into_iter().filter(|(_, light)| light.kind == voxear_render::LightKind::Point).map(|(entity, _)| entity).collect();
    lamps.into_iter().for_each(|lamp| drop(world.despawn(lamp)));
    // A Gate on the green field facing -z, and the Way Back on the red one, 8 m higher, facing +z.
    let size = voxear_core::math::Vec2::new(3.0, 4.0);
    let half_turn = Quat::from_rotation_y(std::f32::consts::PI);
    let gate = world.spawn((Portal::default(), Transform::new(Vec3::new(32.0, 4.0, 30.0), half_turn, Vec3::ONE)));
    let back = world.spawn((Portal::default(), Transform::new(Vec3::new(32.0, 12.0, 20.0), Quat::IDENTITY, Vec3::ONE), InDimension(OTHER)));
    *world.get_mut::<Portal>(gate).unwrap() = Portal::to(size, back);
    *world.get_mut::<Portal>(back).unwrap() = Portal::to(size, gate);
    world.insert_resource(PortalQuality { shadow_distance: 1000.0, ..PortalQuality::default() });
    let looking = |eye: Vec3, target: Vec3| {
        let way = (target - eye).normalize();
        Quat::from_rotation_y(way.x.atan2(way.z)) * Quat::from_rotation_x(-way.y.asin())
    };
    let eye = Vec3::new(32.5, 5.6, 24.0);
    *world.get_mut::<Transform>(camera).unwrap() = Transform::new(eye, looking(eye, Vec3::new(32.0, 6.5, 30.0)), Vec3::ONE);
    seen_from(&mut world, DimensionId::LEVEL);

    // From the level, through the Gate; and from where going through takes the camera, in the other dimension.
    let check = |renderer: &mut Renderer, world: &mut World, near: Entity, far: Entity, name: &str| {
        let here = world.get::<InDimension>(camera).copied();
        let through = frames(renderer, world, 4);
        keep_image(&format!("dimensions through from {name}.png"), &through);
        let rect = opening(world, camera, near, 6);
        let carry = Carry::between(&PortalPose::of_obj(world, near).unwrap(), &PortalPose::of_obj(world, far).unwrap());
        let at = *world.get::<Transform>(camera).unwrap();
        let there = voxear_scene::dimension_of(world, far);
        *world.get_mut::<Transform>(camera).unwrap() = Transform::new(carry.point(at.position), carry.turn(at.rotation), Vec3::ONE);
        voxear_scene::set_dimension(world, camera, there);
        seen_from(world, there);
        // The far Portal leading nowhere, which shows nothing, as the camera looks out past it.
        world.get_mut::<Portal>(far).unwrap().to = None;
        let standing = frames(renderer, world, 4);
        keep_image(&format!("dimensions standing past {name}.png"), &standing);
        // Back as it was.
        world.get_mut::<Portal>(far).unwrap().to = Some(near);
        *world.get_mut::<Transform>(camera).unwrap() = at;
        match here {
            Some(own) => drop(world.insert(camera, own)),
            None => drop(world.remove::<InDimension>(camera)),
        }
        seen_from(world, here.map_or(DimensionId::LEVEL, |own| own.0));
        let (differ, all) = differing(&through, &standing, rect, 12);
        assert!(all > 4000, "{name}: {all} pixels in the opening");
        assert!(differ * 50 < all, "{name}: {differ} of {all} pixels in the opening differ from standing there");
        (through, rect)
    };
    let (into, rect) = check(&mut renderer, &mut world, gate, back, "the gate");
    let (green, red) = counts(&into);
    assert!(green > 1000 && red > 1000, "the green field round the Gate and the red field through it: {green}, {red}");
    // The other's sky in the top of the opening is its own, reddened air, not the level's blue over the Gate.
    let pixel = |pixels: &[u8], x: u32, y: u32| {
        let at = ((y * WIDTH + x) * 4) as usize;
        [pixels[at] as i32, pixels[at + 1] as i32, pixels[at + 2] as i32]
    };
    let inside = pixel(&into, (rect[0] + rect[2]) / 2, rect[1] + 2);
    let outside = pixel(&into, rect[2] + 30, rect[1] + 2);
    assert!(inside != outside, "the sky through the opening is not the level's: {inside:?} and {outside:?}");

    // And from the other dimension, looking back out of the Way Back at the level.
    world.insert(camera, InDimension(OTHER));
    seen_from(&mut world, OTHER);
    let eye = Vec3::new(32.5, 13.6, 26.0);
    *world.get_mut::<Transform>(camera).unwrap() = Transform::new(eye, looking(eye, Vec3::new(32.0, 14.5, 20.0)), Vec3::ONE);
    let (out, _) = check(&mut renderer, &mut world, back, gate, "the way back");
    let (green, red) = counts(&out);
    assert!(green > 1000 && red > 1000, "the red field round the Way Back and the green field through it: {green}, {red}");
}
