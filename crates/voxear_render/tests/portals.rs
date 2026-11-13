//! Portals on a real GPU: an opening shows the place its Portal leads to as
//! a camera standing there would see it, only where the opening shows, and
//! a Portal seen through a Portal shows its own far side in turn. Each test
//! skips itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, MeshRenderer, Portal, PortalPose, PostProcess,
    Primitive, Renderer, RendererSettings,
    portals::{Carry, PortalQuality},
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

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let at = ((y * WIDTH + x) * 4) as usize;
    [pixels[at], pixels[at + 1], pixels[at + 2]]
}

/// Whether a pixel lies inside the opening of the first place's Portal as the camera sees it, `inset` pixels in from
/// its edges.
fn in_opening(corners: &[(f32, f32); 4], (x, y): (u32, u32), inset: f32) -> bool {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let mut sign = 0.0f32;
    for edge in 0..4 {
        let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let length = (ex * ex + ey * ey).sqrt().max(1e-6);
        // How far the pixel is to one side of the edge.
        let side = (ex * (py - a.1) - ey * (px - a.0)) / length;
        if side.abs() < inset || (sign != 0.0 && side.signum() != sign) {
            return false;
        }
        sign = side.signum();
    }
    true
}

/// How many pixels of the rectangle differ by more than `by` in any colour.
fn differing(a: &[u8], b: &[u8], [x0, y0, x1, y1]: [u32; 4], by: u8) -> usize {
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let (p, q) = (pixel(a, x, y), pixel(b, x, y));
            count += usize::from(p.iter().zip(q).any(|(p, q)| p.abs_diff(q) > by));
        }
    }
    count
}

/// Three places far apart, each a slab of ground of its own colour under
/// the same sun: the first with red posts, the second with a yellow tower,
/// a blue block and a magenta one, the third with a wall of cyan. A Portal
/// stands in the first, 6 m along +z, facing back down it; the Portal it
/// leads to stands in the second, turned `turn` round; and two more join
/// the second place to the third.
struct Places {
    world: World,
    near: Entity,
    far: Entity,
    /// The second place's Portal to the third, and the third's.
    onward: Entity,
    last: Entity,
    camera: Entity,
}

const SECOND: Vec3 = Vec3::new(30_000.0, 40.0, -12_000.0);
const THIRD: Vec3 = Vec3::new(-9_000.0, 10.0, 21_000.0);
const SIZE: Vec2 = Vec2::new(2.0, 3.0);

fn places(renderer: &mut Renderer, turn: f32) -> Places {
    let mut world = World::new();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let mut colour = |r: f32, g: f32, b: f32| {
        renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], roughness: 0.9, ..MaterialDesc::default() }).unwrap()
    };
    let (grey, red, green, yellow, blue, magenta, sand, cyan) = (
        colour(0.5, 0.5, 0.5),
        colour(0.9, 0.1, 0.1),
        colour(0.2, 0.6, 0.2),
        colour(0.9, 0.8, 0.1),
        colour(0.1, 0.2, 0.9),
        colour(0.9, 0.1, 0.8),
        colour(0.8, 0.7, 0.5),
        colour(0.1, 0.9, 0.9),
    );
    let block = |world: &mut World, material, at: Vec3, turn: Quat, size: Vec3| {
        world.spawn((MeshRenderer::new(cube, material), Transform::new(at, turn, size)));
    };
    // The first place: ground, and a post each side of the way to the Portal.
    block(&mut world, grey, Vec3::new(0.0, -0.5, 0.0), Quat::IDENTITY, Vec3::new(60.0, 1.0, 60.0));
    block(&mut world, red, Vec3::new(-3.0, 1.5, 6.0), Quat::IDENTITY, Vec3::new(0.6, 3.0, 0.6));
    block(&mut world, red, Vec3::new(3.0, 1.5, 6.0), Quat::IDENTITY, Vec3::new(0.6, 3.0, 0.6));
    // The second, laid out in front of its Portal, whichever way that faces.
    let second = Quat::from_rotation_y(turn);
    let at = |x: f32, y: f32, z: f32| SECOND + second * Vec3::new(x, y, z);
    block(&mut world, green, at(0.0, -0.5, 0.0), second, Vec3::new(80.0, 1.0, 80.0));
    block(&mut world, yellow, at(0.4, 4.0, 9.0), second, Vec3::new(1.0, 8.0, 1.0));
    block(&mut world, blue, at(2.5, 1.0, 5.0), second, Vec3::new(2.0, 2.0, 2.0));
    block(&mut world, magenta, at(-2.2, 0.5, 4.0), second, Vec3::new(1.0, 1.0, 1.0));
    // The third: a wall across the way out of its Portal.
    block(&mut world, sand, THIRD + Vec3::new(0.0, -0.5, 0.0), Quat::IDENTITY, Vec3::new(60.0, 1.0, 60.0));
    block(&mut world, cyan, THIRD + Vec3::new(0.0, 5.0, 8.0), Quat::IDENTITY, Vec3::new(30.0, 10.0, 1.0));

    let half_turn = Quat::from_rotation_y(std::f32::consts::PI);
    let near = world.spawn((Portal::default(), Transform::new(Vec3::new(0.0, 0.0, 6.0), half_turn, Vec3::ONE)));
    let far = world.spawn((Portal::default(), Transform::new(SECOND, second, Vec3::ONE)));
    // In the second place, 7 m in front of its Portal and facing it.
    let onward = world.spawn((Portal::default(), Transform::new(at(-0.6, 0.0, 7.0), second * half_turn, Vec3::ONE)));
    let last = world.spawn((Portal::default(), Transform::new(THIRD, Quat::IDENTITY, Vec3::ONE)));
    for (from, to) in [(near, far), (far, near), (onward, last), (last, onward)] {
        *world.get_mut::<Portal>(from).unwrap() = Portal::to(SIZE, to);
    }
    let camera = world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.6, 0.0), Quat::IDENTITY, Vec3::ONE)));
    let sun = Quat::from_rotation_y(-0.6) * Quat::from_rotation_x(0.9);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    post.global_illumination.enabled = false;
    world.insert_resource(post);
    // Shadows on the far side from any distance, as a camera standing there has them.
    world.insert_resource(PortalQuality { shadow_distance: 1000.0, ..PortalQuality::default() });
    Places { world, near, far, onward, last, camera }
}

fn frames(renderer: &mut Renderer, world: &World, count: u32) -> Vec<u8> {
    for frame in 0..count {
        renderer.render(world, 1.0 + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

/// Turned to look from `eye` at `target`.
fn looking(eye: Vec3, target: Vec3) -> Quat {
    let way = (target - eye).normalize();
    Quat::from_rotation_y(way.x.atan2(way.z)) * Quat::from_rotation_x(-way.y.asin())
}

/// The pixel a point of the world shows at, seen from the camera.
fn on_screen(world: &World, camera: Entity, point: Vec3) -> (u32, u32) {
    let (x, y) = on_screen_exact(world, camera, point);
    (x as u32, y as u32)
}

/// Where in the picture a point of the world shows, in pixels.
fn on_screen_exact(world: &World, camera: Entity, point: Vec3) -> (f32, f32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let at = *world.get::<Transform>(camera).unwrap();
    let view = Mat4::from_rotation_translation(at.rotation, at.position).inverse();
    let lens = Camera::default();
    let projection = vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), WIDTH as f32 / HEIGHT as f32, lens.near);
    let clip = projection * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    (x * WIDTH as f32, y * HEIGHT as f32)
}

/// The rectangle of pixels the first place's opening covers, `inset` pixels
/// in from its edges, seen from the camera.
fn opening(world: &World, camera: Entity, portal: Entity, inset: i32) -> [u32; 4] {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let at = *world.get::<Transform>(camera).unwrap();
    let pose = PortalPose::of_obj(world, portal).unwrap();
    let view = Mat4::from_rotation_translation(at.rotation, at.position).inverse();
    let lens = Camera::default();
    let projection = vulkan::perspective_infinite_reverse(lens.fov_y_degrees.to_radians(), WIDTH as f32 / HEIGHT as f32, lens.near);
    let [x0, y0, x1, y1] = voxear_render::portals::rect_in_view(pose.corners(SIZE), projection * view).expect("in view");
    let px = |x: f32| ((x * 0.5 + 0.5) * WIDTH as f32) as i32;
    let py = |y: f32| ((y * 0.5 + 0.5) * HEIGHT as f32) as i32;
    let clamp = |v: i32, most: u32| v.clamp(0, most as i32) as u32;
    [clamp(px(x0) + inset, WIDTH), clamp(py(y0) + inset, HEIGHT), clamp(px(x1) - inset, WIDTH), clamp(py(y1) - inset, HEIGHT)]
}

#[test]
fn an_opening_shows_the_far_place_as_a_camera_standing_there_sees_it() {
    let Some(mut renderer) = renderer() else { return };
    // From straight on, from off to one side and low, and from close by and high: and the far Portal facing three ways.
    let eyes = [Vec3::new(0.0, 1.6, 0.0), Vec3::new(-3.5, 0.9, 1.5), Vec3::new(0.8, 2.4, 4.6)];
    for (shot, (&eye, turn)) in eyes.iter().zip([0.0f32, 1.9, -2.6]).enumerate() {
        let mut places = places(&mut renderer, turn);
        // Only the one pair, as the camera carried over sees no Portal at all.
        for portal in [places.onward, places.last] {
            places.world.despawn(portal);
        }
        let turned = looking(eye, Vec3::new(0.0, 1.5, 6.0));
        *places.world.get_mut::<Transform>(places.camera).unwrap() = Transform::new(eye, turned, Vec3::ONE);
        let through = frames(&mut renderer, &places.world, 3);
        keep_image(&format!("portal {shot} through.png"), &through);
        let rect = opening(&places.world, places.camera, places.near, 0);
        assert!((rect[2] - rect[0]) * (rect[3] - rect[1]) > 3000, "the opening shows large enough to judge: {rect:?}");
        let near_pose = PortalPose::of_obj(&places.world, places.near).unwrap();
        let corners = near_pose.corners(SIZE).map(|corner| {
            let (x, y) = on_screen_exact(&places.world, places.camera, corner);
            (x, y)
        });
        let foot_at = on_screen(&places.world, places.camera, Vec3::new(0.0, 0.1, 6.0));

        // The camera carried to where going through the Portal would take it, with no Portal in the way.
        let (near, far) = (
            PortalPose::of_obj(&places.world, places.near).unwrap(),
            PortalPose::of_obj(&places.world, places.far).unwrap(),
        );
        let carry = Carry::between(&near, &far);
        for portal in [places.near, places.far] {
            places.world.despawn(portal);
        }
        *places.world.get_mut::<Transform>(places.camera).unwrap() =
            Transform::new(carry.point(eye), carry.turn(turned), Vec3::ONE);
        let there = frames(&mut renderer, &places.world, 3);
        keep_image(&format!("portal {shot} there.png"), &there);

        // Every pixel well inside the opening is the far place's, as near as soft shadows' grain allows.
        let (mut all, mut differ) = (0, 0);
        for y in rect[1]..rect[3] {
            for x in rect[0]..rect[2] {
                if in_opening(&corners, (x, y), 2.0) {
                    all += 1;
                    let (p, q) = (pixel(&through, x, y), pixel(&there, x, y));
                    differ += usize::from(p.iter().zip(q).any(|(p, q)| p.abs_diff(q) > 12));
                }
            }
        }
        assert!(all > 2500, "shot {shot}: {all} pixels inside the opening");
        assert!(differ * 100 < all, "shot {shot}: {differ} of {all} pixels in the opening differ from the far place");
        // And it is the far place: its green ground, not the first place's grey, at the foot of the opening.
        let foot = pixel(&through, foot_at.0, foot_at.1);
        assert!(foot[1] > foot[0] + 20 && foot[1] > foot[2] + 20, "shot {shot}: green ground at its foot, {foot:?}");
    }
}

#[test]
fn only_the_opening_changes_and_what_stands_in_front_of_it_stays() {
    let Some(mut renderer) = renderer() else { return };
    let mut places = places(&mut renderer, 0.7);
    // A white post in the first place, between the camera and the opening, across its left half.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = renderer.add_material(&MaterialDesc::default()).unwrap();
    places.world.spawn((
        MeshRenderer::new(cube, white),
        Transform::new(Vec3::new(-0.45, 1.0, 4.0), Quat::IDENTITY, Vec3::new(0.5, 2.0, 0.5)),
    ));
    let open = frames(&mut renderer, &places.world, 3);
    keep_image("portal open.png", &open);
    places.world.get_mut::<Portal>(places.near).unwrap().open = false;
    let closed = frames(&mut renderer, &places.world, 3);
    keep_image("portal closed.png", &closed);

    let rect = opening(&places.world, places.camera, places.near, -3);
    // Round the opening nothing differs, whether it is open or not, but the grain of soft shadows' edges, which
    // changes from frame to frame.
    let outside = differing(&open, &closed, [0, 0, WIDTH, HEIGHT], 16) - differing(&open, &closed, rect, 16);
    assert!(outside < 20, "{outside} pixels outside the opening changed as it closed");
    // In it, the far place shows open and its mist closed: bluish, with no green ground.
    let inside = opening(&places.world, places.camera, places.near, 4);
    let foot = (inside[2] - 6, inside[3] - 3);
    let (far, mist) = (pixel(&open, foot.0, foot.1), pixel(&closed, foot.0, foot.1));
    assert!(far[1] > far[0] + 20 && far[1] > far[2] + 20, "the far ground: {far:?}");
    assert!(mist[2] > mist[0] && mist[2] >= mist[1], "the mist: {mist:?}");
    // The mist is its Portal's own blue all over the opening, not what stands behind it.
    let middle = on_screen(&places.world, places.camera, Vec3::new(0.5, 1.8, 6.0));
    let mist = pixel(&closed, middle.0, middle.1);
    assert!(mist[2] > mist[0] + 25 && mist[2] > mist[1] + 10, "the mist in the middle of the opening: {mist:?}");
    // The post in front of it is drawn over both alike.
    let post = (WIDTH / 2 - 22, HEIGHT / 2 + 10);
    let (a, b) = (pixel(&open, post.0, post.1), pixel(&closed, post.0, post.1));
    assert!(a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 4), "the post in front of the opening: {a:?} {b:?}");
    assert!(a.iter().all(|&c| c > 120) && a[0].abs_diff(a[2]) < 12, "it is white: {a:?}");
}

#[test]
fn a_portal_seen_through_a_portal_shows_its_own_far_side_when_asked_to() {
    let Some(mut renderer) = renderer() else { return };
    let mut places = places(&mut renderer, 0.0);
    // Where the second place's onward Portal shows in the first's opening: found by closing it and looking for
    // what changes.
    let quality = PortalQuality { depth: 2, shadow_distance: 1000.0, ..PortalQuality::default() };
    places.world.insert_resource(quality);
    let deep = frames(&mut renderer, &places.world, 3);
    keep_image("portal deep.png", &deep);
    places.world.insert_resource(PortalQuality { depth: 1, ..quality });
    let shallow = frames(&mut renderer, &places.world, 3);
    keep_image("portal shallow.png", &shallow);

    let rect = opening(&places.world, places.camera, places.near, 2);
    let mut cyan = 0;
    let mut mist = 0;
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            let (d, s) = (pixel(&deep, x, y), pixel(&shallow, x, y));
            if d.iter().zip(s).all(|(d, s)| d.abs_diff(s) <= 6) {
                continue;
            }
            // Two deep, the third place's cyan wall; one deep, the onward Portal's mist.
            cyan += usize::from(d[1] > d[0] + 40 && d[2] > d[0] + 40);
            mist += usize::from(s[2] > s[0] && s[2] >= s[1]);
        }
    }
    assert!(cyan > 300, "the third place's wall shows through both: {cyan} pixels");
    assert!(mist > 300, "one deep, the second Portal shows its mist: {mist} pixels");
    // Outside the first opening nothing differs.
    let wide = opening(&places.world, places.camera, places.near, -3);
    let outside = differing(&deep, &shallow, [0, 0, WIDTH, HEIGHT], 16) - differing(&deep, &shallow, wide, 16);
    assert!(outside < 20, "{outside} pixels outside the opening changed");
}

/// A world of blocks with two fields 20 km apart, each with a Portal that
/// leads to the other: the first plain grass with a stone pillar; the second
/// with a pit of water, a tall pillar of dirt and steps of stone. The
/// camera stands in the first, looking at its Portal.
fn fields(renderer: &mut Renderer) -> (World, [Entity; 3]) {
    use std::sync::Arc;
    use voxear_render::{BlockLook, Sky, SurfaceLook};
    use voxear_terrain::blocks::{
        BlockWorld,
        chunk::BlockId,
        file::{BlockFile, BlockKind},
    };
    const FAR: i32 = 20_000;
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
    let water = file.blocks.len() as BlockId;
    let mut blocks = file.make_ground(None);
    for y in 4..10 {
        blocks.set([20, y, 44], 3);
    }
    // The second field, the same ground 20 km along x.
    for z in 0..64 {
        for x in 0..64 {
            for y in 0..4 {
                blocks.set([FAR + x, y, z], if y == 3 { 1 } else { 2 });
            }
        }
    }
    for z in 40..52 {
        for x in 24..40 {
            for y in 1..4 {
                blocks.set([FAR + x, y, z], water);
            }
            blocks.set([FAR + x, 0, z], 3);
        }
    }
    for y in 4..16 {
        blocks.set([FAR + 36, y, 36], 2);
    }
    for step in 0..5 {
        for y in 4..5 + step {
            blocks.set([FAR + 24 + step, y, 34], 3);
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            let [r, g, b] = look.color.map(linear);
            let material = renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let mut world = World::new();
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    // On the grass, 4 m up: the first looks back down -z at the camera, the second out over the water.
    let half_turn = Quat::from_rotation_y(std::f32::consts::PI);
    let near = world.spawn((Portal::default(), Transform::new(Vec3::new(32.0, 4.0, 40.0), half_turn, Vec3::ONE)));
    let far = world.spawn((Portal::default(), Transform::new(Vec3::new(FAR as f32 + 32.0, 4.0, 30.0), Quat::IDENTITY, Vec3::ONE)));
    *world.get_mut::<Portal>(near).unwrap() = Portal::to(SIZE, far);
    *world.get_mut::<Portal>(far).unwrap() = Portal::to(SIZE, near);
    let eye = Vec3::new(31.0, 5.7, 34.5);
    let camera = world.spawn((Camera::default(), Transform::new(eye, looking(eye, Vec3::new(32.0, 5.4, 40.0)), Vec3::ONE)));
    let towards_sun = Vec3::new(-0.5, 0.75, -0.3).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.reflections.enabled = false;
    post.global_illumination.enabled = false;
    world.insert_resource(post);
    world.insert_resource(PortalQuality { shadow_distance: 1000.0, ..PortalQuality::default() });
    (world, [near, far, camera])
}

#[test]
fn blocks_and_their_water_show_through_an_opening_as_they_stand_twenty_kilometres_off() {
    let Some(mut renderer) = renderer() else { return };
    let (mut world, [near, far, camera]) = fields(&mut renderer);
    let through = frames(&mut renderer, &world, 4);
    keep_image("portal blocks through.png", &through);
    let rect = opening(&world, camera, near, 0);
    let near_pose = PortalPose::of_obj(&world, near).unwrap();
    let corners = near_pose.corners(SIZE).map(|corner| on_screen_exact(&world, camera, corner));
    // The camera carried to the second field, with no Portal in the way.
    let carry = Carry::between(&near_pose, &PortalPose::of_obj(&world, far).unwrap());
    let at = *world.get::<Transform>(camera).unwrap();
    for portal in [near, far] {
        world.despawn(portal);
    }
    *world.get_mut::<Transform>(camera).unwrap() = Transform::new(carry.point(at.position), carry.turn(at.rotation), Vec3::ONE);
    let there = frames(&mut renderer, &world, 4);
    keep_image("portal blocks there.png", &there);
    let (mut all, mut differ, mut wet) = (0, 0, 0);
    for y in rect[1]..rect[3] {
        for x in rect[0]..rect[2] {
            if in_opening(&corners, (x, y), 2.0) {
                all += 1;
                let (p, q) = (pixel(&through, x, y), pixel(&there, x, y));
                differ += usize::from(p.iter().zip(q).any(|(p, q)| p.abs_diff(q) > 14));
                // The water: more green and blue than red, and not the grass's green.
                wet += usize::from(p[2] > p[0] + 12 && p[1] > p[0] + 12);
            }
        }
    }
    assert!(all > 6000, "{all} pixels inside the opening");
    assert!(differ * 50 < all, "{differ} of {all} pixels in the opening differ from the far field");
    assert!(wet * 20 > all, "the far field's water shows in the opening: {wet} of {all} pixels");
}

/// Hills of blocks 256 a side with a wood of pillars, at the world's middle
/// and again 20 km along x, the camera on the first looking along +z, at
/// 1920 × 1080: the frame's cost with no Portal, with one whose opening
/// covers a third of the picture and leads to the second hills, and with
/// two in view.
fn timing_world(renderer: &mut Renderer) -> (World, Entity) {
    use std::sync::Arc;
    use voxear_render::{BlockLook, Sky, SurfaceLook};
    use voxear_terrain::blocks::{BlockMap, BlockWorld, file::BlockFile};
    const FAR: i32 = 20_000;
    let file = BlockFile::default();
    let mut blocks = BlockMap::default();
    for at in [0, FAR] {
        for z in -128..128 {
            for x in -128..128 {
                let (fx, fz) = (x as f32, z as f32);
                let top = (6.0 + (fx * 0.07).sin() * 3.0 + (fz * 0.05).cos() * 3.0 + (fx * 0.13 + fz * 0.11).sin()) as i32;
                for y in 0..top {
                    blocks.set([at + x, y, z], if y + 1 == top { 1 } else if y + 4 > top { 2 } else { 3 });
                }
                // A pillar every seven blocks each way, as tall as a tree.
                if x.rem_euclid(7) == 3 && z.rem_euclid(7) == 3 && (x.abs() > 4 || z < 0) {
                    for y in top..top + 6 {
                        blocks.set([at + x, y, z], 2);
                    }
                }
            }
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let material = renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let mut world = World::new();
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::from_position(Vec3::new(0.0, -10.0, 0.0))));
    let eye = Vec3::new(0.0, 1.0, -4.0);
    let camera = world.spawn((Camera::default(), Transform::new(eye, Quat::IDENTITY, Vec3::ONE)));
    let towards_sun = Vec3::new(-0.5, 0.75, -0.3).normalize();
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, Quat::from_rotation_arc(Vec3::Z, -towards_sun), Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    (world, camera)
}

/// The least GPU time of `frames` frames after `settle` more, in milliseconds:
/// on a GPU other programs use, the least is the frame's own.
fn least_frame(renderer: &mut Renderer, world: &World, settle: u32, frames: u32) -> f32 {
    let mut least = f32::MAX;
    for frame in 0..settle + frames {
        renderer.render(world, 1.0 + frame as f32 / 60.0).expect("frame renders");
        if frame >= settle {
            least = least.min(renderer.gpu_frame_ms());
        }
    }
    least
}

/// What one Portal whose opening covers a third of the picture may add to a
/// frame at 1080p, and two in view, in milliseconds.
const ONE_PORTAL_MS: f32 = 1.5;
const TWO_PORTALS_MS: f32 = 2.5;

#[test]
fn a_portal_over_a_third_of_the_picture_and_two_in_view_cost_no_more_than_their_budget() {
    let renderer = Renderer::new_offscreen(1920, 1080, &RendererSettings::default());
    let Ok(mut renderer) = renderer else { return eprintln!("skipping: no usable GPU") };
    let (mut world, camera) = timing_world(&mut renderer);
    let none = least_frame(&mut renderer, &world, 20, 60);

    // The opening 3 m wide and 4 m high, 4 m in front of the camera: a third of the picture.
    let half_turn = Quat::from_rotation_y(std::f32::consts::PI);
    let size = Vec2::new(3.0, 4.0);
    let place = |world: &mut World, near: Vec3, far: Vec3| {
        let a = world.spawn((Portal::default(), Transform::new(near, half_turn, Vec3::ONE)));
        let b = world.spawn((Portal::default(), Transform::new(far, Quat::IDENTITY, Vec3::ONE)));
        *world.get_mut::<Portal>(a).unwrap() = Portal::to(size, b);
        *world.get_mut::<Portal>(b).unwrap() = Portal::to(size, a);
        [a, b]
    };
    let first = place(&mut world, Vec3::new(0.0, -1.0, 0.0), Vec3::new(20_000.0, -4.0, 10.0));
    let pose = PortalPose::of_obj(&world, first[0]).unwrap();
    let at = *world.get::<Transform>(camera).unwrap();
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_rotation_translation(at.rotation, at.position).inverse();
    let projection = vulkan::perspective_infinite_reverse(60f32.to_radians(), 16.0 / 9.0, 0.1);
    let [x0, y0, x1, y1] = voxear_render::portals::rect_in_view(pose.corners(size), projection * view).unwrap();
    let cover = (x1 - x0) * (y1 - y0) * 0.25;
    assert!((0.28..0.4).contains(&cover), "the opening covers {cover} of the picture");
    let one = least_frame(&mut renderer, &world, 20, 60);
    // Two in view: the first moved to the left and a second to the right, each a sixth of the picture.
    world.get_mut::<Transform>(first[0]).unwrap().position = Vec3::new(-2.6, -1.0, 4.0);
    place(&mut world, Vec3::new(2.6, -1.0, 4.0), Vec3::new(20_040.0, -4.0, -30.0));
    let two = least_frame(&mut renderer, &world, 20, 60);
    eprintln!("frame: {none:.2} ms with no Portal, {one:.2} with one over a third of the picture, {two:.2} with two");
    assert!(one - none <= ONE_PORTAL_MS, "one Portal added {:.2} ms", one - none);
    assert!(two - none <= TWO_PORTALS_MS, "two Portals added {:.2} ms", two - none);
}
