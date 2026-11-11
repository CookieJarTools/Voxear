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

/// Hills with grass on them swaying in a gusty wind, a grove whose tint
/// changes in patches, cubes whose glow flickers each at its own pace, and
/// the sun casting shadows over them: patterns read from where things are.
/// Placed `out` from the world's middle, which is `origin` from the level's.
fn patterned(renderer: &mut Renderer, origin: voxear_core::math::I64Vec3, out: Vec3) -> World {
    use std::sync::Arc;
    use voxear_render::{Foliage, GrassLook, Impostors, MeshRenderer, Primitive, WindZone};
    use voxear_scene::{Plant, Plants, WorldOrigin};
    use voxear_terrain::grass::{GrassMap, GrassTile};
    let mut world = World::new();
    world.insert_resource(WorldOrigin(origin));
    let side = voxear_terrain::TILE_SAMPLES;
    let map = GrassMap::from_tiles(1, 1, 1, vec![Arc::new(GrassTile::from_samples(vec![255; (side * side) as usize]))]);
    let heights = Heightmap::from_fn(1, 1, 0.5, |x, z| (x * 0.11).sin() * 1.5 + (z * 0.07).cos() * 1.2);
    let terrain = Terrain { grass: Some(Arc::new(map)), ..Terrain::new(heights) };
    let ground = renderer.add_material(&MaterialDesc { base_color: [0.45, 0.4, 0.3, 1.0], ..MaterialDesc::default() });
    let swaying =
        MaterialDesc { base_color: [0.2, 0.7, 0.2, 1.0], wind_bend: 1.0, wind_flutter: 1.0, ..MaterialDesc::default() };
    let blade = renderer.add_material(&swaying).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grass = GrassLook {
        mesh: cube,
        material: blade,
        density: 4.0,
        scale: [0.08, 0.5],
        draw_distance: 40.0,
        springs_back: 0.0,
    };
    let look = TerrainLook { grass: Arc::from([Some(grass)]), ..TerrainLook::new(ground.unwrap(), 4.0) };
    world.spawn((terrain, look, Transform::from_position(out + Vec3::new(-64.0, 0.0, -20.0))));
    let foliage = Foliage {
        mesh: cube,
        material: blade,
        draw_distance: 100.0,
        fade_distance: 1.0,
        shadow_distance: 100.0,
        healthy_color: [0.3, 1.0, 0.3],
        dry_color: [1.0, 0.8, 0.2],
        color_spread: 3.0,
        impostors: Impostors::Off,
        impostor_distance: 90.0,
    };
    let plants =
        (0..40).map(|n| Plant::new([(n % 8) as f32 * 2.5 - 9.0, 3.0, (n / 8) as f32 * 2.5], 0.3 * n as f32, 1.4));
    world.spawn((foliage, Plants::new(plants.collect()), Transform::from_position(out + Vec3::new(0.0, 0.0, 14.0))));
    let flickering = MaterialDesc {
        emissive: [3.0, 1.2, 0.3],
        glow: voxear_render::Glow::Flicker,
        glow_speed: 1.0,
        ..MaterialDesc::default()
    };
    let lamp = renderer.add_material(&flickering).unwrap();
    for n in 0..6 {
        let at = out + Vec3::new(n as f32 * 1.7 - 4.0, 4.5, 9.0);
        world.spawn((MeshRenderer::new(cube, lamp), Transform::new(at, Quat::IDENTITY, Vec3::splat(0.4))));
    }
    world.spawn((WindZone { strength: 1.5, gusts: 1.0, gust_size: 6.0, ..WindZone::default() }, Transform::default()));
    let look = Quat::from_rotation_x(0.25);
    world.spawn((Camera::default(), Transform::new(out + Vec3::new(0.0, 7.0, -6.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

/// The fifth frame of `world`, moving it by `by` before the last with the
/// renderer told, as the floating origin does.
fn fifth_frame(renderer: &mut Renderer, world: &mut World, by: Option<IVec3>) -> Vec<u8> {
    for frame in 0..5 {
        if frame == 4
            && let Some(by) = by
        {
            voxear_scene::move_objs(world, by);
            voxear_scene::propagate_transforms(world);
            renderer.shift_origin(by.as_vec3());
        }
        renderer.render(world, 1.0 + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

#[test]
fn patterns_keep_their_place_as_the_middle_moves_and_far_out_match_near() {
    use voxear_core::math::I64Vec3;
    // 3 km out from the level's middle, up and across, the world's middle there.
    let middle = I64Vec3::new(3072, 1024, -2048);
    let Some(mut renderer) = renderer() else { return };
    let mut there = patterned(&mut renderer, middle, Vec3::ZERO);
    let still = fifth_frame(&mut renderer, &mut there, None);
    keep_image("patterns near.png", &still);
    let all = (WIDTH * HEIGHT) as usize;
    // The same place drawn from the level's middle for four frames, its world's middle then moving out to it: the
    // grass, the grove's patches, the flicker, the wind and the sun's shadows each read from the level's middle,
    // and none changes.
    let Some(mut renderer) = self::renderer() else { return };
    let mut moving = patterned(&mut renderer, I64Vec3::ZERO, middle.as_vec3());
    let moved = fifth_frame(&mut renderer, &mut moving, Some(-middle.as_ivec3()));
    keep_image("patterns moved.png", &moved);
    let changed = differing_pixels(&still, &moved);
    assert!(changed * 1000 < all, "{changed} of {all} pixels changed as the middle moved");
    // The same scene 131 km further out, twice as far as any pattern takes to repeat, is the same picture, as exact
    // out there as near the level's middle.
    let Some(mut renderer) = self::renderer() else { return };
    let mut far = patterned(&mut renderer, middle + I64Vec3::new(131_072, 0, -131_072), Vec3::ZERO);
    let out = fifth_frame(&mut renderer, &mut far, None);
    keep_image("patterns far.png", &out);
    let changed = differing_pixels(&still, &out);
    assert!(changed * 1000 < all, "{changed} of {all} pixels differ 131 km out");
}
