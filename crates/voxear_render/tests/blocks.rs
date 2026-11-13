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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
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

#[test]
fn a_world_too_large_to_upload_in_a_frame_comes_over_several_the_old_one_drawn_till_it_is_whole() {
    use voxear_terrain::blocks::BlockMap;
    let Some(mut renderer) = renderer() else { return };
    // A field 64 m a side, seen from above.
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    let (_, faces) = file.faces();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material, surfaces: Arc::from([]), voxels: Default::default() };
    let mut world = World::new();
    let field = world.spawn((BlockWorld::new(file.make_ground(None), &faces, 1.0), look, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 80.0, 32.0), down, Vec3::ONE)));
    render(&mut renderer, &world, 2);
    let old = renderer.stats().blocks;
    // Made again of blocks half the size, every other one: about 38 MB of faces, more than a frame uploads.
    let mut checks = BlockMap::default();
    for x in 0..128 {
        for y in 0..96 {
            for z in (0..128).filter(|z| (x + y + z) % 2 == 0) {
                checks.set([x, y, z], 3);
            }
        }
    }
    world.insert(field, BlockWorld::new(checks, &faces, 0.5));
    let counts: Vec<_> = (0..8)
        .map(|_| {
            render(&mut renderer, &world, 1);
            renderer.stats().blocks
        })
        .collect();
    // The field drawn while the new world comes, then the new world whole, the same each frame after.
    let arrived = counts.iter().position(|counts| *counts != old).expect("the new world drawn");
    assert!(arrived >= 2, "came in {} frames: {counts:?}", arrived + 1);
    assert!(counts[arrived..].windows(2).all(|two| two[0] == two[1]), "whole once drawn: {counts:?}");
    assert!(counts[arrived].triangles > old.triangles * 10, "{:?} against {old:?}", counts[arrived]);
}

#[test]
fn a_coarser_chunk_of_a_world_without_end_is_drawn_twice_as_large() {
    use std::collections::HashMap;
    use voxear_terrain::blocks::{BlockMap, mesh::mesh_chunk};
    let Some(mut renderer) = renderer() else { return };
    // A floor a block thick over one chunk, its own at (0, 0, 0), and the same as a level-1 chunk at (1, 0, 0):
    // 32 m of floor from 0, and 64 m of coarser floor from 64 m, its chunks being twice as large.
    let file = BlockFile::default();
    let (looks, faces) = file.faces();
    let mut floor = BlockMap::default();
    for z in 0..32 {
        for x in 0..32 {
            floor.set([x, 0, z], 3);
        }
    }
    let mut world_blocks = BlockWorld::new(floor.clone(), &faces, 1.0);
    let mut moved = BlockMap::default();
    for z in 0..32 {
        for x in 0..32 {
            moved.set([32 + x, 0, z], 1);
        }
    }
    let coarse = mesh_chunk(&moved, [1, 0, 0], &faces);
    world_blocks.far = Arc::new(HashMap::from([([1, 1, 0, 0], Arc::new(coarse))]));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((world_blocks, look, Transform::default()));
    // Straight down from 90 m over (64, ·, 16): +x to the right.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(64.0, 91.0, 16.0), down, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 1.0, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("blocks coarse.png", &pixels);
    // Seen from 90 m, the view is about 52 m half across: x from 12 to 116.
    let half = (30f32).to_radians().tan() * 90.0 * WIDTH as f32 / HEIGHT as f32;
    let column = |x: f32| ((x - 64.0) / half * 0.5 + 0.5) * WIDTH as f32;
    let at = |x: f32| rgb_at(&pixels, column(x) as u32, HEIGHT / 2);
    let green = |[r, g, b]: [i32; 3]| g > r + 10 && g > b + 10;
    assert!(!green(at(20.0)), "its own chunk: stone");
    assert!(!green(at(50.0)), "nothing between");
    assert!(
        green(at(70.0)) && green(at(110.0)),
        "the coarser chunk's grass, 64 m wide: {:?} {:?}",
        at(70.0),
        at(110.0)
    );
}

#[test]
fn a_world_drawn_smooth_is_a_round_hill_its_top_and_sides_blended_with_no_gaps_between_chunks() {
    use voxear_terrain::blocks::{
        BlockMap,
        chunk::{AIR, fill_of},
        file::BlockKind,
    };
    let Some(mut renderer) = renderer() else { return };
    // One kind of block, its top one surface and its sides another.
    let mut file = BlockFile { smooth: true, ..BlockFile::default() };
    file.blocks = vec![BlockKind { top: Some("Top".into()), material: Some("Sides".into()), ..BlockKind::default() }];
    // Ground 4 blocks up, 64 a side, and a round hill 20 higher in its middle, steepest 11.5 m out, at 57 degrees.
    let height = |x: f32, z: f32| {
        let away = ((x - 32.0).powi(2) + (z - 32.0).powi(2)) / (20.0f32 * 20.0);
        4.0 + 20.0 * (1.0 - away).max(0.0).powi(2)
    };
    let mut blocks = BlockMap::default();
    for z in 0..64 {
        for x in 0..64 {
            let surface = height(x as f32 + 0.5, z as f32 + 0.5);
            for y in 0..(surface.ceil() as i32 + 1) {
                let depth = surface - (y as f32 + 0.5);
                let block = if depth > 0.0 { 1 } else { AIR };
                blocks.set_cell([x, y, z], block, fill_of(depth, depth > 0.0));
            }
        }
    }
    let (looks, faces) = file.faces();
    assert_eq!((faces[1].top, faces[1].sides, faces[1].bottom), (0, 1, 1));
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            // Its top green, its sides brown.
            let color = if look.material.as_deref() == Some("Top") { [0.2, 0.6, 0.15] } else { [0.5, 0.3, 0.15] };
            let [r, g, b] = color;
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((BlockWorld::made(blocks, &faces, 1.0, true), look, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 44.0, 32.0), down, Vec3::ONE)));
    // The sun in the west (-x), shining east.
    let towards_sun = Vec3::new(-40f32.to_radians().cos(), 40f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 4);
    keep_image("blocks smooth hill.png", &pixels);
    // Seen from 44 m: a point on the ground below, as far below the camera as the ground there.
    let at = |x: f32, z: f32| {
        let (px, py) = from_above(44.0 - height(x, z), x, z);
        rgb_at(&pixels, px, py)
    };
    let green = |[r, g, b]: [i32; 3]| g > r + 10 && g > b;
    let brown = |[r, g, b]: [i32; 3]| r > g + 10 && g > b;
    assert!(green(at(32.0, 32.0)), "the hill's flat top: {:?}", at(32.0, 32.0));
    assert!(green(at(4.0, 50.0)), "the flat ground: {:?}", at(4.0, 50.0));
    // Its steepest side facing the sun: its sides' surface.
    assert!(brown(at(20.5, 32.0)), "the hill's steep side: {:?}", at(20.5, 32.0));
    // Lit on the side facing the sun, darker away from it.
    let brightness = |x: f32, z: f32| at(x, z).iter().sum::<i32>();
    assert!(brightness(22.0, 32.0) > brightness(42.0, 32.0), "lit from the west");
    // Across where its chunks meet (x 32 and z 32, down the middle of the view), ground everywhere, never the sky.
    let sky = |[r, g, b]: [i32; 3]| b > r + 15 && b > g + 5;
    for y in HEIGHT / 8..HEIGHT * 7 / 8 {
        for x in [WIDTH / 2 - 1, WIDTH / 2] {
            assert!(!sky(rgb_at(&pixels, x, y)), "a gap at ({x}, {y}): {:?}", rgb_at(&pixels, x, y));
        }
    }
    for x in WIDTH / 8..WIDTH * 7 / 8 {
        for y in [HEIGHT / 2 - 1, HEIGHT / 2] {
            assert!(!sky(rgb_at(&pixels, x, y)), "a gap at ({x}, {y}): {:?}", rgb_at(&pixels, x, y));
        }
    }
}

#[test]
fn water_blocks_are_drawn_over_the_ground_under_them_which_shows_through_tinted() {
    use voxear_terrain::blocks::{
        BlockMap,
        chunk::{AIR, BlockId},
        file::BlockKind,
    };
    let Some(mut renderer) = renderer() else { return };
    // Grass 4 blocks up, 64 a side, and a pit 24 blocks a side and 3 deep in its middle, its floor stone in the west
    // and grass in the east; then the same pit full of water.
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
    let water = file.blocks.len() as BlockId;
    let mut dug = file.make_ground(None);
    for z in 20..44 {
        for x in 20..44 {
            for y in 1..4 {
                dug.set([x, y, z], AIR);
            }
            dug.set([x, 0, z], if x < 32 { 3 } else { 1 });
        }
    }
    let mut flooded = dug.clone();
    for z in 20..44 {
        for x in 20..44 {
            for y in 1..4 {
                flooded.set([x, y, z], water);
            }
        }
    }
    let (looks, faces) = file.faces();
    assert!(faces[usize::from(water)].water);
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            // Linear, as the engine makes a colour's material.
            let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            let [r, g, b] = look.color.map(linear);
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let mut draw = |blocks: BlockMap, name: &str| {
        let mut world = World::new();
        world.spawn((BlockWorld::new(blocks, &faces, 1.0), look.clone(), Transform::default()));
        // Straight down onto its middle from 40 m over the grass, the sun high in the west.
        let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 44.0, 32.0), down, Vec3::ONE)));
        let towards_sun = Vec3::new(-60f32.to_radians().cos(), 60f32.to_radians().sin(), 0.2).normalize();
        let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
        world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
        world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
        let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
        world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
        let pixels = render(&mut renderer, &world, 4);
        keep_image(name, &pixels);
        move |x: f32, z: f32| {
            let (px, py) = from_above(40.0, x, z);
            rgb_at(&pixels, px, py)
        }
    };
    let dry = draw(dug, "blocks pit.png");
    let wet = draw(flooded, "blocks water.png");
    let apart = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
    // The grass round the pit as it was.
    for (x, z) in [(10.0, 40.0), (54.0, 24.0)] {
        assert!(apart(dry(x, z), wet(x, z)) < 24, "the grass at ({x}, {z}): {:?} and {:?}", dry(x, z), wet(x, z));
    }
    // Over the pit's floor, the water drawn over it, bluer.
    let blue = |[r, _, b]: [i32; 3]| b - r;
    for (x, z) in [(25.0, 32.0), (39.0, 30.0)] {
        let (before, after) = (dry(x, z), wet(x, z));
        assert!(apart(before, after) > 20, "water over ({x}, {z}): {before:?} and {after:?}");
        assert!(blue(after) > blue(before) + 8, "bluer under water: {before:?} and {after:?}");
    }
    // And the floor under it seen through it: its stone and its grass still apart.
    let (stone, grass) = (wet(25.0, 32.0), wet(39.0, 30.0));
    assert!(apart(stone, grass) > 20, "the floor through the water: {stone:?} and {grass:?}");
    assert!(grass[1] - grass[0] > stone[1] - stone[0], "greener over the grass: {grass:?} and {stone:?}");
}

#[test]
fn flowing_water_is_drawn_as_high_as_it_stands_lower_the_further_it_runs() {
    use voxear_terrain::blocks::{BlockMap, file::BlockKind};
    let Some(mut renderer) = renderer() else { return };
    // A ledge of stone a block wide along x, a source on it near one end, run out along it and spilling off its sides.
    let mut file = BlockFile { blocks: vec![BlockKind::colored("Stone", [0.5; 3])], ..BlockFile::default() };
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
    let (looks, faces) = file.faces();
    let mut ledge = BlockMap::default();
    for x in 0..24 {
        ledge.set([x, 0, 1], 1);
    }
    let mut blocks = BlockWorld::new(ledge, &faces, 1.0);
    blocks.set_blocks([([2, 1, 1], 2)]);
    for _ in 0..60 {
        blocks.flow();
    }
    assert!(blocks.flowing.is_empty(), "settled");
    let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color.map(linear);
            let material = MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() };
            SurfaceLook {
                material: renderer.add_material(&material).unwrap(),
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
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((blocks, look, Transform::default()));
    // From in front, level with the water, looking along +z: +x to the right.
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(6.0, 1.5, -6.0))));
    let towards_sun = Vec3::new(-0.3, 0.8, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 4);
    keep_image("blocks flowing.png", &pixels);
    // Where a point on the plane z = 0, the front of the water spilt off the ledge, lands on screen.
    let at = |x: f32, y: f32| {
        let (half, aspect) = ((30f32).to_radians().tan() * 6.0, WIDTH as f32 / HEIGHT as f32);
        let (u, v) = ((x - 6.0) / (half * aspect), (y - 1.5) / half);
        rgb_at(&pixels, ((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
    };
    let apart = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
    // Beside the source it stands seven eighths of a block up; six blocks on, an eighth, the sky above it.
    let (near, far, far_low) = (at(2.5, 1.6), at(8.5, 1.6), at(8.5, 1.06));
    // The sky at the same height, past where the water reaches.
    let sky = at(11.5, 1.6);
    assert!(apart(far, sky) < 16, "nothing over the far water at 1.6: {far:?} against the sky's {sky:?}");
    assert!(apart(near, sky) > 30, "water near the source at 1.6: {near:?} against the sky's {sky:?}");
    assert!(apart(far_low, sky) > 30, "the far water low down: {far_low:?} against the sky's {sky:?}");
}

/// Grass 4 blocks up, 64 a side, with a pit 24 blocks a side and 3 deep in its middle, its floor sand, full of
/// water to the grass's top, but for a pillar of stone as high as the water in each of the columns `pillars`; and the
/// look it is drawn with, the sun high in the west.
fn pond(renderer: &mut Renderer, pillars: &[[i32; 2]]) -> World {
    use voxear_terrain::blocks::{chunk::BlockId, file::BlockKind};
    let mut file = BlockFile::default();
    file.ground.size = [64, 64];
    file.ground.height = 4;
    file.blocks.push(BlockKind::colored("Sand", [0.86, 0.78, 0.55]));
    let sand = file.blocks.len() as BlockId;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
    let water = file.blocks.len() as BlockId;
    let mut blocks = file.make_ground(None);
    for z in 20..44 {
        for x in 20..44 {
            blocks.set([x, 0, z], sand);
            for y in 1..4 {
                blocks.set([x, y, z], if pillars.contains(&[x, z]) { 3 } else { water });
            }
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            // Linear, as the engine makes a colour's material.
            let linear = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            let [r, g, b] = look.color.map(linear);
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let mut world = World::new();
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    let towards_sun = Vec3::new(-60f32.to_radians().cos(), 60f32.to_radians().sin(), 0.2).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

#[test]
fn the_bed_under_water_blocks_is_lit_the_same_with_the_camera_near_the_water_as_far_from_it() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond(&mut renderer, &[]);
    // Straight down onto the pond's middle: from 2.6 m over the water, then from 1.5 m, where the camera is near
    // enough for the water round it to be a body for the view under it.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let camera = world.spawn((Camera::default(), Transform::default()));
    let mut seen = |world: &mut World, over: f32, name: &str| {
        world.insert(camera, Transform::new(Vec3::new(32.0, 4.0 + over, 32.0), down, Vec3::ONE));
        let pixels = render(&mut renderer, world, 8);
        keep_image(name, &pixels);
        // The bed under the middle of the picture, and a little to each side of it.
        [(0, 0), (-40, 10), (50, -30)]
            .map(|(dx, dy)| rgb_at(&pixels, (WIDTH as i32 / 2 + dx) as u32, (HEIGHT as i32 / 2 + dy) as u32))
    };
    let far = seen(&mut world, 2.6, "blocks pond from far.png");
    let near = seen(&mut world, 1.5, "blocks pond from near.png");
    let apart = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
    for (far, near) in far.into_iter().zip(near) {
        assert!(apart(far, near) < 24, "the bed from far {far:?} and from near {near:?}");
    }
}

#[test]
fn water_blocks_look_the_same_inside_the_stretch_of_them_round_the_camera_as_outside_it() {
    let Some(mut renderer) = renderer() else { return };
    // Down across the pond from 1.6 m over it. A pillar to the water's top six blocks on ends the stretch of water
    // round the camera there, so the water past the pillar lies outside it; and with a pillar under the camera too,
    // out of sight, there is no such stretch at all.
    let across = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(30f32.to_radians());
    let mut seen = |pillars: &[[i32; 2]], name: &str| {
        let mut world = pond(&mut renderer, pillars);
        world.spawn((Camera::default(), Transform::new(Vec3::new(26.5, 5.6, 32.5), across, Vec3::ONE)));
        let pixels = render(&mut renderer, &world, 12);
        keep_image(name, &pixels);
        pixels
    };
    let with = seen(&[[32, 32]], "blocks stretch round the camera.png");
    let without = seen(&[[32, 32], [26, 32]], "blocks no stretch round the camera.png");
    // The two pictures alike all over, but for a pixel here and there of the sun's glints.
    let apart = |a: [i32; 3], b: [i32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).sum::<i32>();
    let (mut total, mut off, mut count) = (0, 0, 0);
    for y in (8..HEIGHT - 8).step_by(4) {
        for x in (8..WIDTH - 8).step_by(4) {
            let d = apart(rgb_at(&with, x, y), rgb_at(&without, x, y));
            total += d;
            off += usize::from(d > 40);
            count += 1;
        }
    }
    let mean = total as f32 / count as f32;
    assert!(mean < 6.0 && off * 200 < count, "apart by {mean:.1} a pixel, {off} of {count} by more than 40");
}

#[test]
fn a_camera_just_under_water_blocks_sees_air_over_one_level_waterline_and_water_under_it() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond(&mut renderer, &[]);
    // In the pond, 3 cm under its top, looking level along it: the near plane's top is out of the water.
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 3.97, 23.0), Quat::IDENTITY, Vec3::ONE)));
    // Where the near plane, 0.1 m off and 60 degrees tall, crosses the water's top: this far down the picture.
    let half = 0.1 * 30f32.to_radians().tan();
    let line = ((1.0 - 0.03 / half) * 0.5 * HEIGHT as f32) as i32;
    // Water is its own green, far from the sky's grey and blue.
    let watery = |[r, g, b]: [i32; 3]| g - r > 30 && g > b;
    let mut time = 0.0;
    for (frames, name) in
        [(20, "blocks half under a.png"), (37, "blocks half under b.png"), (53, "blocks half under c.png")]
    {
        for _ in 0..frames {
            time += 1.0 / 60.0;
            renderer.render(&world, time).expect("frame renders");
        }
        let pixels = renderer.read_pixels().expect("pixels").2;
        keep_image(name, &pixels);
        for x in [60, 200, 320, 470, 600] {
            // Air down to the waterline, a few rows off it, and water from there to the picture's foot.
            for y in (4..line - 8).step_by(6) {
                let above = rgb_at(&pixels, x, y as u32);
                assert!(!watery(above), "air over the waterline at ({x}, {y}) after {time:.2} s: {above:?}");
            }
            for y in (line + 8..HEIGHT as i32 - 4).step_by(6) {
                let under = rgb_at(&pixels, x, y as u32);
                assert!(watery(under), "water under the waterline at ({x}, {y}) after {time:.2} s: {under:?}");
            }
        }
    }
}

#[test]
fn a_smooth_world_without_end_runs_to_the_horizon_with_no_gaps_where_its_rings_meet() {
    use voxear_terrain::{
        blocks::{
            BlockMap,
            edits::BlockEdits,
            generate::BlockGenerate,
            stream::{BlockMaking, BlockStreamer},
        },
        generate::Preset,
    };
    let Some(mut renderer) = renderer() else { return };
    // Rolling hills without end, in blocks 1 m a side, near chunks out to 64 m and coarser rings to 1500 m.
    let generate = BlockGenerate { caves: 0.0, ..BlockGenerate::default() };
    let file = BlockFile { generate: Some(generate.clone()), smooth: true, ..BlockFile::default() };
    let (looks, faces) = file.faces();
    let hills = Arc::new(Preset::RollingHills.world());
    let making = BlockMaking { file, generate, world: hills.clone(), faces: faces.clone().into(), far_folder: None, far_built: Vec::new() };
    let mut blocks = BlockWorld::made(BlockMap::default(), &faces, 1.0, true);
    let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
    let mut streamer = BlockStreamer::default();
    let start = std::time::Instant::now();
    loop {
        streamer.update(&mut blocks, &mut base, &mut pending, making.clone(), (64.0, 1500.0), [0.0, 0.0]);
        if streamer.settled() {
            break;
        }
        assert!(start.elapsed().as_secs() < 240, "never settled");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(!blocks.far.is_empty() && blocks.far.keys().any(|key| key[0] >= 3), "rings out to the third at least");
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((blocks, look, Transform::default()));
    // From 40 m over the ground at the middle, looking along +x and a little down, the sun low from one side so
    // any groove or wall across the ground shows.
    let ground = hills.height_at(0.0, 0.0);
    let look = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-0.2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, ground + 40.0, 0.0), look, Vec3::ONE)));
    let towards_sun = Vec3::new(0.3, 0.35, 1.0).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 6);
    keep_image("blocks rings.png", &pixels);
    // Below the horizon, ground everywhere: no sky through a gap between rings.
    let sky = |[r, g, b]: [i32; 3]| b > r + 15 && b > g + 5;
    let mut gaps = 0;
    for y in HEIGHT * 3 / 4..HEIGHT {
        for x in 0..WIDTH {
            gaps += u32::from(sky(rgb_at(&pixels, x, y)));
        }
    }
    assert_eq!(gaps, 0, "pixels of sky through the ground");
}

#[test]
fn mountains_stand_on_the_horizon_sixteen_kilometres_off_with_no_gaps_where_the_far_views_levels_meet() {
    use voxear_terrain::{
        blocks::{
            BlockMap,
            edits::BlockEdits,
            generate::BlockGenerate,
            stream::{BlockMaking, BlockStreamer},
        },
        generate::Preset,
    };
    let Some(mut renderer) = renderer() else { return };
    // Mountains without end, in blocks 1 m a side, near chunks out to 128 m and the far view to 16 km.
    let generate = BlockGenerate { caves: 0.0, position: [2000.0, 3000.0], bottom: -600, ..BlockGenerate::default() };
    let file = BlockFile { generate: Some(generate.clone()), ..BlockFile::default() };
    let (looks, faces) = file.faces();
    let alps = Arc::new(Preset::Alpine.world());
    let making = BlockMaking { file, generate, world: alps.clone(), faces: faces.clone().into(), far_folder: None, far_built: Vec::new() };
    let mut blocks = BlockWorld::made(BlockMap::default(), &faces, 1.0, false);
    let (mut base, mut pending) = (Arc::new(BlockMap::default()), Arc::new(BlockEdits::default()));
    let mut streamer = BlockStreamer::default();
    let start = std::time::Instant::now();
    loop {
        streamer.update(&mut blocks, &mut base, &mut pending, making.clone(), (128.0, 16_000.0), [0.0, 0.0]);
        if streamer.settled() {
            break;
        }
        assert!(start.elapsed().as_secs() < 240, "never settled");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let levels: std::collections::BTreeSet<i32> = blocks.far.keys().map(|key| key[0]).collect();
    assert_eq!(levels.last(), Some(&7), "out to the eighth level, 4 km columns: {levels:?}");
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((blocks, look, Transform::default()));
    // From 150 m over the ground at the middle, looking along +x at the horizon, the sun low from one side.
    let ground = alps.height_at(2000.0, 3000.0);
    let look = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(-0.06);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, ground + 150.0, 0.0), look, Vec3::ONE)));
    let towards_sun = Vec3::new(0.3, 0.35, 1.0).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 6);
    keep_image("blocks far view.png", &pixels);
    // The sky's own light blue: dark faces in shadow are blueish too, but far darker.
    let sky = |[r, g, b]: [i32; 3]| b > r + 15 && b > g + 5 && b > 120;
    // Under the highest ground in each column of pixels, ground all the way down: no sky through a join.
    let mut tops = Vec::new();
    let mut gaps = 0;
    for x in 0..WIDTH {
        let Some(top) = (0..HEIGHT).find(|&y| !sky(rgb_at(&pixels, x, y))) else { continue };
        tops.push(top);
        gaps += (top..HEIGHT).filter(|&y| sky(rgb_at(&pixels, x, y))).count();
    }
    assert!(gaps <= 4, "{gaps} pixels of sky through the ground, past a few on the peaks' edges");
    // The horizon is mountains: it rises and falls across the picture.
    let (high, low) = (tops.iter().min().copied().unwrap_or(0), tops.iter().max().copied().unwrap_or(0));
    assert!(tops.len() == WIDTH as usize && low - high > 10, "a horizon of peaks, from row {high} to {low}");
}

#[test]
fn a_generated_worlds_biomes_show_as_different_country_snow_forest_grass_desert_rock_and_shore() {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
        },
        generate::{BiomeBlocks, BiomeDef, Biomes, Preset, WorldBiomes},
    };
    let Some(mut renderer) = renderer() else { return };
    // Mountains 2 km across in blocks 8 m a side, their climate changing over 700 m so several biomes lie in view,
    // and tall enough to be cold on top.
    let mut land = Preset::Alpine.landmass();
    land.climate.size = 700.0;
    land.climate.temperature = 14.0;
    land.climate.temperature_range = 16.0;
    land.climate.humidity_range = 0.45;
    land.height = 320.0;
    let world = voxear_terrain::generate::WorldGen::from(land);
    // Where the generator has land with a coast: the first stretch found, looked for a kilometre at a time, that
    // is mostly over the sea with some of it under.
    let over_sea = |corner: [f32; 2]| {
        let high = |i: usize| world.height_at(corner[0] + (i % 5) as f32 * 500.0, corner[1] + (i / 5) as f32 * 500.0);
        (0..25).filter(|&i| high(i) > world.sea_level() + 10.0).count()
    };
    let position = (0..1600)
        .map(|i| [(i % 40) as f32 * 1000.0 - 20_000.0, (i / 40) as f32 * 1000.0 - 20_000.0])
        .find(|&corner| (17..24).contains(&over_sea(corner)))
        .expect("land with a coast somewhere within 20 km");
    let generate = BlockGenerate { caves: 0.0, bottom: -48, position, ..BlockGenerate::default() };
    let mut file = BlockFile {
        block_size: 8.0,
        ground: Ground { size: [256, 256], ..Ground::default() },
        generate: Some(generate),
        ..BlockFile::default()
    };
    // A sea to its generator's sea level.
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.05, 0.3, 0.45]) });
    file.generate.as_mut().unwrap().sea = file.blocks.len() as u16;
    // The world's own three blocks, then each biome's.
    let colours: [(&str, [f32; 3]); 8] = [
        ("Snow", [0.93, 0.95, 0.97]),
        ("Tundra", [0.52, 0.55, 0.42]),
        ("Needles", [0.13, 0.3, 0.17]),
        ("Grass", [0.36, 0.6, 0.22]),
        ("Sand", [0.9, 0.82, 0.55]),
        ("Dry Grass", [0.72, 0.66, 0.3]),
        ("Rock", [0.42, 0.4, 0.4]),
        ("Jungle", [0.1, 0.42, 0.12]),
    ];
    let first = file.blocks.len() as u16 + 1;
    for (name, colour) in colours {
        file.blocks.push(BlockKind::colored(name, colour));
    }
    let [snow, tundra, needles, grass, sand, dry, rock, jungle] = std::array::from_fn(|i| first + i as u16);
    let biome = |temperature: [f32; 2], humidity: [f32; 2]| BiomeDef { temperature, humidity, ..BiomeDef::default() };
    let of = |top: u16| BiomeBlocks {
        top,
        steep: rock,
        steep_slope: 0.9,
        shore: sand,
        shore_height: 6.0,
        ..BiomeBlocks::default()
    };
    let list = vec![
        (biome([-60.0, -2.0], [0.0, 1.0]), BiomeBlocks { shore: 0, ..of(snow) }),
        (biome([-2.0, 5.0], [0.0, 1.0]), of(tundra)),
        (biome([5.0, 12.0], [0.45, 1.0]), of(needles)),
        (biome([5.0, 22.0], [0.0, 0.55]), of(grass)),
        (biome([22.0, 60.0], [0.0, 0.3]), of(sand)),
        (biome([22.0, 60.0], [0.3, 0.6]), of(dry)),
        (biome([12.0, 60.0], [0.6, 1.0]), of(jungle)),
    ];
    let (defs, blocks): (Vec<_>, Vec<_>) = list.into_iter().unzip();
    let placed = WorldBiomes { biomes: Biomes::new(defs, 16.0, 0.45, world.seed()), blocks };
    file.generate.as_mut().unwrap().placed = Some(Arc::new(placed));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut scene = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let made = BlockWorld::made(file.make_ground(Some(&world)), &faces, 8.0, false);
    assert!(made.meshes.len() > 40, "{} chunks drawn", made.meshes.len());
    scene.spawn((made, look, Transform::default()));
    // From high over one corner, looking across it and down.
    let turn = Quat::from_rotation_y(0.785) * Quat::from_rotation_x(0.62);
    scene.spawn((Camera::default(), Transform::new(Vec3::new(150.0, 950.0, 150.0), turn, Vec3::ONE)));
    let towards_sun = Vec3::new(0.4, 0.8, 0.3).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.35, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    // A world this large comes to the GPU over many frames.
    let pixels = render(&mut renderer, &scene, 120);
    keep_image("blocks biomes.png", &pixels);
    // Country of at least four kinds in view: pixels nearest each biome's colour, by which channel leads.
    let (mut white, mut green, mut sandy, mut grey) = (0, 0, 0, 0);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = rgb_at(&pixels, x, y);
            white += u32::from(r > 170 && g > 170 && b > 170);
            green += u32::from(g > r + 20 && g > b + 20);
            sandy += u32::from(r > b + 40 && g > b + 25 && r >= g);
            grey += u32::from((r - g).abs() < 12 && (g - b).abs() < 12 && r > 50 && r < 150);
        }
    }
    let kinds = [white, green, sandy, grey].iter().filter(|&&count| count > 300).count();
    assert!(kinds >= 3, "snow {white}, green {green}, sand {sandy}, rock {grey}");
}

#[test]
fn a_generated_world_builds_a_wood_of_grown_trees_and_huts_standing_level_on_its_hills() {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
            plant::{PlantDef, PlantKind},
            structure::{GroundFit, PlacedStructure, StructureBlocks, WorldStructures},
        },
        generate::{Preset, structures::Placement},
    };
    let Some(mut renderer) = renderer() else { return };
    let world = Preset::RollingHills.world();
    let mut file = BlockFile {
        block_size: 1.0,
        ground: Ground { size: [128, 128], ..Ground::default() },
        generate: Some(BlockGenerate { caves: 0.0, bottom: -12, ..BlockGenerate::default() }),
        ..BlockFile::default()
    };
    let first = file.blocks.len() as u16 + 1;
    for (name, colour) in [
        ("Oak Log", [0.36, 0.26, 0.17]),
        ("Oak Leaves", [0.22, 0.48, 0.18]),
        ("Pine Log", [0.3, 0.2, 0.14]),
        ("Pine Needles", [0.1, 0.3, 0.17]),
        ("Planks", [0.72, 0.55, 0.34]),
        ("Roof", [0.55, 0.2, 0.15]),
    ] {
        file.blocks.push(BlockKind::colored(name, colour));
    }
    let [oak_log, oak_leaves, pine_log, pine_needles, planks, roof] = std::array::from_fn(|i| first + i as u16);
    // A hut 7 by 7: a floor and walls of planks with a door, under a roof that steps in.
    let (sx, sy, sz) = (7usize, 7usize, 7usize);
    let mut hut = vec![0u16; sx * sy * sz];
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1;
                let door = z == 0 && x == 3 && (1..3).contains(&y);
                let inset = y.saturating_sub(4);
                let in_roof = y >= 4 && x >= inset && z >= inset && x < sx - inset && z < sz - inset;
                hut[(y * sz + z) * sx + x] = match () {
                    _ if y == 0 => planks,
                    _ if in_roof
                        && (y == sy - 1 || x == inset || z == inset || x == sx - 1 - inset || z == sz - 1 - inset) =>
                    {
                        roof
                    }
                    _ if y < 4 && edge && !door => planks,
                    _ => 0,
                };
            }
        }
    }
    let land = |spacing: f32, separation: f32, chance: f32, slope: f32, salt: u32| Placement {
        spacing,
        separation,
        chance,
        slope,
        salt,
        height: Some([1.0, 1000.0]),
        ..Placement::default()
    };
    let pine = PlantDef {
        kind: PlantKind::Conifer,
        height: [12.0, 17.0],
        crown_width: [6.0, 8.0],
        crown_height: [9.0, 12.0],
        bend: 0.05,
        ..PlantDef::default()
    };
    let list = vec![
        PlacedStructure {
            placement: land(48.0, 20.0, 1.0, 18.0, 5),
            ground: GroundFit::Flatten,
            sink: 0,
            blocks: StructureBlocks::Model { size: [7, 7, 7], blocks: hut },
            biomes: Vec::new(),
        },
        PlacedStructure {
            placement: land(13.0, 6.0, 0.7, 35.0, 1),
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Plant { plant: PlantDef::default(), wood: oak_log, leaves: oak_leaves, vine: 0 },
            biomes: Vec::new(),
        },
        PlacedStructure {
            placement: land(17.0, 7.0, 0.6, 40.0, 2),
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Plant { plant: pine, wood: pine_log, leaves: pine_needles, vine: 0 },
            biomes: Vec::new(),
        },
    ];
    file.generate.as_mut().unwrap().built = Some(Arc::new(WorldStructures { list }));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut scene = World::new();
    let made = BlockWorld::made(file.make_ground(Some(&world)), &faces, 1.0, false);
    let count = |block: u16| -> usize {
        let mut n = 0;
        for z in 0..128 {
            for x in 0..128 {
                for y in -12..160 {
                    n += usize::from(made.blocks.get([x, y, z]) == block);
                }
            }
        }
        n
    };
    let (huts, oaks, pines) = (count(roof), count(oak_log), count(pine_log));
    assert!(huts > 50 && oaks > 100 && pines > 100, "roof {huts}, oak {oaks}, pine {pines}");
    let ground_at = |x: f32, z: f32| world.height_at(x, z);
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    scene.spawn((made, look, Transform::default()));
    // From over one corner of the hills, 30 m up, looking across them and a little down.
    // Cameras look along +z; turned to look back towards the world's corner.
    let turn = Quat::from_rotation_y(3.927) * Quat::from_rotation_x(0.32);
    let eye = Vec3::new(134.0, ground_at(134.0, 134.0).max(ground_at(110.0, 110.0)) + 30.0, 134.0);
    scene.spawn((Camera::default(), Transform::new(eye, turn, Vec3::ONE)));
    let towards_sun = Vec3::new(-0.5, 0.6, -0.3).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.35, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &scene, 120);
    keep_image("blocks structures.png", &pixels);
    // Leaves of both kinds in view.
    let mut green = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = rgb_at(&pixels, x, y);
            green += u32::from(g > r + 15 && g > b + 10);
        }
    }
    assert!(green > 4000, "{green} pixels of leaves");
}

#[test]
fn a_generated_world_builds_a_village_of_houses_round_a_well_with_paths_from_their_doors() {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
            plant::PlantDef,
            structure::{GroundFit, PlacedStructure, StructureBlocks, Town, TownHouseKind, TownPiece, WorldStructures},
        },
        generate::{Preset, structures::Placement},
    };
    let Some(mut renderer) = renderer() else { return };
    let world = Preset::RollingHills.world();
    let mut file = BlockFile {
        block_size: 1.0,
        ground: Ground { size: [160, 160], ..Ground::default() },
        generate: Some(BlockGenerate { caves: 0.0, bottom: -12, ..BlockGenerate::default() }),
        ..BlockFile::default()
    };
    let first = file.blocks.len() as u16 + 1;
    for (name, colour) in [
        ("Stone", [0.5, 0.5, 0.52]),
        ("Planks", [0.74, 0.56, 0.35]),
        ("Roof", [0.62, 0.22, 0.16]),
        ("Gravel", [0.62, 0.58, 0.5]),
        ("Water", [0.15, 0.35, 0.6]),
        ("Log", [0.36, 0.26, 0.17]),
        ("Leaves", [0.22, 0.48, 0.18]),
    ] {
        file.blocks.push(BlockKind::colored(name, colour));
    }
    let [stone, planks, roof, gravel, water, log, leaves] = std::array::from_fn(|i| first + i as u16);
    // A house: a stone footing, plank walls with a door in the middle of its -z side and windows, a roof that
    // steps in to a ridge.
    let house = |[sx, wall, sz]: [usize; 3]| {
        let sy = wall + 1 + sx.div_ceil(2);
        let mut blocks = vec![0u16; sx * sy * sz];
        for y in 0..sy {
            for z in 0..sz {
                for x in 0..sx {
                    let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1;
                    let door = z == 0 && x == sx / 2 && (1..3).contains(&y);
                    let window = y == 2 && edge && !door && (x + z) % 3 == 1 && !(x == 0 && z == 0);
                    let up = y.saturating_sub(wall + 1);
                    let roofed = y > wall && x >= up && x < sx - up;
                    let ridge = roofed && (x == up || x == sx - 1 - up || y == sy - 1 || z == 0 || z == sz - 1);
                    blocks[(y * sz + z) * sx + x] = match () {
                        _ if y == 0 => stone,
                        _ if y <= wall && edge && !door && !window => planks,
                        _ if y > wall && ridge => roof,
                        _ => 0,
                    };
                }
            }
        }
        TownPiece { size: [sx as u32, sy as u32, sz as u32], blocks, ground: GroundFit::Flatten, sink: 0 }
    };
    // A well: a ring of stone round water, under a small roof on posts.
    let mut well = vec![0u16; 5 * 5 * 5];
    for y in 0..5 {
        for z in 0..5 {
            for x in 0..5 {
                let edge = x == 0 || z == 0 || x == 4 || z == 4;
                let post = (x == 0 || x == 4) && (z == 0 || z == 4);
                well[(y * 5 + z) * 5 + x] = match y {
                    0 => {
                        if edge {
                            stone
                        } else {
                            water
                        }
                    }
                    1 => {
                        if edge {
                            stone
                        } else {
                            0
                        }
                    }
                    2 | 3 => {
                        if post {
                            log
                        } else {
                            0
                        }
                    }
                    _ => roof,
                };
            }
        }
    }
    let village = Town {
        middle: Some(TownPiece { size: [5, 5, 5], blocks: well, ground: GroundFit::Flatten, sink: 0 }),
        houses: vec![
            TownHouseKind::anywhere(house([7, 4, 9]), 2.0),
            TownHouseKind::anywhere(house([5, 3, 7]), 2.0),
            TownHouseKind::anywhere(house([9, 4, 7]), 1.0),
        ],
        count: [12, 18],
        radius: [34.0, 60.0],
        path: gravel,
        path_width: 3,
        wander: 0.5,
        branching: 0.6,
        roads: false,
        road: 0,
        road_width: 3,
        bridge: 0,
        link: 0.0,
    };
    let land = |spacing: f32, separation: f32, chance: f32, slope: f32, salt: u32| Placement {
        spacing,
        separation,
        chance,
        slope,
        salt,
        height: Some([1.0, 1000.0]),
        ..Placement::default()
    };
    let list = vec![
        PlacedStructure {
            placement: land(400.0, 200.0, 1.0, 12.0, 9),
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Town(village),
            biomes: Vec::new(),
        },
        PlacedStructure {
            placement: land(15.0, 7.0, 0.5, 35.0, 1),
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Plant { plant: PlantDef::default(), wood: log, leaves, vine: 0 },
            biomes: Vec::new(),
        },
    ];
    // Where the village stands: the first site its placement keeps whose ground fits, within 2 km.
    let site = list[0]
        .placement
        .tries_near(world.seed(), [-2000.0; 2], [2000.0; 2], 0.0)
        .into_iter()
        .find(|site| {
            let height = world.height_at(site.at[0], site.at[1]);
            let east = world.height_at(site.at[0] + 1.0, site.at[1]);
            let north = world.height_at(site.at[0], site.at[1] + 1.0);
            let slope = ((east - height).powi(2) + (north - height).powi(2)).sqrt();
            height > world.sea_level() + 1.0 && slope < 0.2
        })
        .expect("a village site within 2 km");
    let position = [site.at[0] - 80.0, site.at[1] - 80.0];
    file.generate.as_mut().unwrap().position = position;
    file.generate.as_mut().unwrap().built = Some(Arc::new(WorldStructures { list }));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let made = BlockWorld::made(file.make_ground(Some(&world)), &faces, 1.0, false);
    let count = |block: u16| -> usize {
        let mut n = 0;
        for z in 0..160 {
            for x in 0..160 {
                for y in -12..200 {
                    n += usize::from(made.blocks.get([x, y, z]) == block);
                }
            }
        }
        n
    };
    let (roofs, paths) = (count(roof), count(gravel));
    assert!(roofs > 300 && paths > 150, "roof {roofs}, paths {paths}");
    let middle_height = world.height_at(site.at[0], site.at[1]);
    let mut scene = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    scene.spawn((made, look, Transform::default()));
    // From 70 m over the village's south-west, looking at its middle; cameras look along +z.
    let turn = Quat::from_rotation_y(0.785) * Quat::from_rotation_x(0.75);
    let eye = Vec3::new(30.0, middle_height + 70.0, 30.0);
    scene.spawn((Camera::default(), Transform::new(eye, turn, Vec3::ONE)));
    let towards_sun = Vec3::new(-0.4, 0.75, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &scene, 120);
    keep_image("blocks village.png", &pixels);
    let mut red = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = rgb_at(&pixels, x, y);
            red += u32::from(r > g + 40 && r > b + 40);
        }
    }
    assert!(red > 1500, "{red} pixels of roof");
}

#[test]
fn towns_are_joined_by_roads_that_follow_the_ground_and_cross_water_on_bridges() {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
            structure::{GroundFit, PlacedStructure, StructureBlocks, Town, TownHouseKind, TownPiece, WorldStructures},
        },
        generate::{Preset, structures::Placement},
    };
    let Some(mut renderer) = renderer() else { return };
    let world = Preset::RollingHills.world();
    let mut file = BlockFile {
        block_size: 2.0,
        ground: Ground { size: [400, 400], ..Ground::default() },
        generate: Some(BlockGenerate { caves: 0.0, bottom: -20, ..BlockGenerate::default() }),
        ..BlockFile::default()
    };
    let first = file.blocks.len() as u16 + 1;
    for (name, colour) in [
        ("Planks", [0.74, 0.56, 0.35]),
        ("Roof", [0.62, 0.22, 0.16]),
        ("Gravel", [0.66, 0.62, 0.54]),
        ("Road", [0.45, 0.36, 0.26]),
        ("Water", [0.12, 0.32, 0.55]),
    ] {
        file.blocks.push(BlockKind { water: name == "Water", ..BlockKind::colored(name, colour) });
    }
    let [planks, roof, gravel, road, water] = std::array::from_fn(|i| first + i as u16);
    file.generate.as_mut().unwrap().sea = water;
    // Small houses of planks under a roof, 4 by 4 blocks of 2 m.
    let mut house = vec![0u16; 4 * 4 * 4];
    for y in 0..4 {
        for z in 0..4 {
            for x in 0..4 {
                let edge = x == 0 || z == 0 || x == 3 || z == 3;
                house[(y * 4 + z) * 4 + x] = match y {
                    0 => planks,
                    1 | 2 if edge && !(z == 0 && x == 2 && y == 1) => planks,
                    3 => roof,
                    _ => 0,
                };
            }
        }
    }
    let piece = TownPiece { size: [4, 4, 4], blocks: house, ground: GroundFit::Flatten, sink: 0 };
    let town = Town {
        middle: Some(TownPiece { size: [3, 1, 3], blocks: vec![gravel; 9], ground: GroundFit::Flatten, sink: 0 }),
        houses: vec![TownHouseKind::anywhere(piece, 1.0)],
        count: [6, 10],
        radius: [14.0, 24.0],
        path: gravel,
        path_width: 2,
        wander: 0.5,
        branching: 0.5,
        roads: true,
        road,
        road_width: 2,
        bridge: planks,
        link: 1.0,
    };
    let placement = Placement {
        spacing: 300.0,
        separation: 120.0,
        chance: 1.0,
        slope: 25.0,
        height: Some([2.0, 400.0]),
        ..Placement::default()
    };
    // An 800 m square with at least four towns' sites on land in it, looked for a kilometre at a time.
    let on_land = |corner: [f32; 2]| {
        placement
            .tries_near(world.seed(), [corner[0] + 60.0, corner[1] + 60.0], [corner[0] + 740.0, corner[1] + 740.0], 0.0)
            .iter()
            .filter(|site| world.height_at(site.at[0], site.at[1]) > world.sea_level() + 2.0)
            .count()
    };
    let corner = (0..400)
        .map(|i| [(i % 20) as f32 * 1000.0 - 10_000.0, (i / 20) as f32 * 1000.0 - 10_000.0])
        .find(|&corner| on_land(corner) >= 4)
        .expect("four towns' sites on land within 10 km");
    file.generate.as_mut().unwrap().position = corner;
    let list = vec![PlacedStructure {
        placement,
        ground: GroundFit::Leave,
        sink: 0,
        blocks: StructureBlocks::Town(town),
        biomes: Vec::new(),
    }];
    file.generate.as_mut().unwrap().built = Some(Arc::new(WorldStructures { list }));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let made = BlockWorld::made(file.make_ground(Some(&world)), &faces, 2.0, false);
    let mut roads = 0;
    for z in 0..400 {
        for x in 0..400 {
            for y in -20..200 {
                roads += usize::from(made.blocks.get([x, y, z]) == road);
            }
        }
    }
    assert!(roads > 300, "{roads} blocks of road");
    let mut scene = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    scene.spawn((made, look, Transform::default()));
    // From high over the middle of the 800 m square, looking down at it; cameras look along +z.
    let turn = Quat::from_rotation_y(0.0) * Quat::from_rotation_x(1.2);
    scene.spawn((Camera::default(), Transform::new(Vec3::new(400.0, 520.0, 160.0), turn, Vec3::ONE)));
    let towards_sun = Vec3::new(-0.4, 0.8, -0.4).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &scene, 200);
    keep_image("blocks roads.png", &pixels);
}

#[test]
fn a_dungeon_cut_open_shows_its_rooms_joined_by_corridors_and_its_stair() {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
            structure::{
                Dungeon, GroundFit, PlacedStructure, RoomRole, StructureBlocks, TownHouseKind, TownPiece,
                WorldStructures,
            },
        },
        generate::{Preset, structures::Placement},
    };
    let Some(mut renderer) = renderer() else { return };
    let world = Preset::RollingHills.world();
    let mut file = BlockFile {
        block_size: 1.0,
        ground: Ground { size: [320, 320], ..Ground::default() },
        generate: Some(BlockGenerate { caves: 0.0, bottom: -60, ..BlockGenerate::default() }),
        ..BlockFile::default()
    };
    let first = file.blocks.len() as u16 + 1;
    for (name, colour) in [
        ("Bricks", [0.42, 0.42, 0.46]),
        ("Flagstones", [0.62, 0.58, 0.5]),
        ("Gold", [0.95, 0.75, 0.2]),
        ("Moss", [0.25, 0.45, 0.2]),
    ] {
        file.blocks.push(BlockKind::colored(name, colour));
    }
    let [bricks, flagstones, gold, moss] = std::array::from_fn(|i| first + i as u16);
    // A treasure room: walls of bricks round a floor with a heap of gold in its middle, far from the stair.
    let room = |size: [usize; 3], middle: u16| {
        let [sx, sy, sz] = size;
        let mut blocks = vec![0u16; sx * sy * sz];
        for y in 0..sy {
            for z in 0..sz {
                for x in 0..sx {
                    let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1 || y == sy - 1;
                    let heap = y == 1 && (x as i32 - sx as i32 / 2).abs() <= 1 && (z as i32 - sz as i32 / 2).abs() <= 1;
                    blocks[(y * sz + z) * sx + x] = match () {
                        _ if y == 0 => flagstones,
                        _ if edge => bricks,
                        _ if heap => middle,
                        _ => 0,
                    };
                }
            }
        }
        TownPiece { size: [sx as u32, sy as u32, sz as u32], blocks, ground: GroundFit::Leave, sink: 0 }
    };
    // A throne room for the boss with gold in its middle; treasuries of gold at the loot rooms; moss rooms on the way.
    let kind =
        |piece: TownPiece, most: u32| TownHouseKind { piece, weight: 1.0, most, zone: [0.0, 1.0], biomes: Vec::new() };
    let dungeon = Dungeon {
        entrance: None,
        rooms: vec![
            (RoomRole::Boss, kind(room([15, 7, 15], gold), 1)),
            (RoomRole::Loot, kind(room([7, 4, 7], gold), 0)),
            (RoomRole::Any, kind(room([7, 5, 7], moss), 0)),
        ],
        count: [14, 18],
        main_path: 0.5,
        halls: 0.35,
        corridor: [4, 10],
        loops: 0.5,
        floors: [1, 1],
        floor_gap: 14,
        depth: [26, 30],
        caves: 0,
        wall: bricks,
        floor: flagstones,
        corridor_width: 3,
        corridor_height: 3,
        room_size: [7, 5, 7],
        hall_size: [13, 7, 13],
        loot_size: [5, 4, 5],
        boss_size: [17, 9, 17],
    };
    let placement = Placement {
        spacing: 600.0,
        separation: 100.0,
        chance: 1.0,
        slope: 40.0,
        height: Some([2.0, 1000.0]),
        ..Placement::default()
    };
    // Over the first site within 2 km on land.
    let site = placement
        .tries_near(world.seed(), [-2000.0; 2], [2000.0; 2], 0.0)
        .into_iter()
        .find(|site| world.height_at(site.at[0], site.at[1]) > world.sea_level() + 2.0)
        .expect("a site on land");
    file.generate.as_mut().unwrap().position = [site.at[0] - 160.0, site.at[1] - 160.0];
    let list = vec![PlacedStructure {
        placement,
        ground: GroundFit::Leave,
        sink: 0,
        blocks: StructureBlocks::Dungeon(dungeon),
        biomes: Vec::new(),
    }];
    file.generate.as_mut().unwrap().built = Some(Arc::new(WorldStructures { list }));
    let (looks, faces) = file.faces();
    let mut map = file.make_ground(Some(&world));
    // Cut open: everything from three blocks over the dungeon's floor up taken away, its ceilings with it.
    let mut floors = std::collections::HashMap::<i32, usize>::new();
    for z in 0..320 {
        for x in 0..320 {
            for y in -60..200 {
                if map.get([x, y, z]) == flagstones {
                    *floors.entry(y).or_default() += 1;
                }
            }
        }
    }
    let floor = floors.iter().max_by_key(|(_, count)| **count).map(|(y, _)| *y).expect("a dungeon floor");
    let golds =
        (0..320 * 320 * 8).filter(|i| map.get([i % 320, floor + 1 + (i / 102400), (i / 320) % 320]) == gold).count();
    assert!(golds >= 9, "{golds} blocks of gold: the treasure room");
    for z in 0..320 {
        for x in 0..320 {
            for y in floor + 3..200 {
                map.set([x, y, z], 0);
            }
        }
    }
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut scene = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    scene.spawn((BlockWorld::made(map.clone(), &faces, 1.0, false), look, Transform::default()));
    // Straight down over the middle of its floor, high enough to hold all of it; cameras look along +z.
    let (mut low, mut high) = ([i32::MAX; 2], [i32::MIN; 2]);
    for z in 0..320 {
        for x in 0..320 {
            if map.get([x, floor, z]) == flagstones {
                (low, high) = ([low[0].min(x), low[1].min(z)], [high[0].max(x), high[1].max(z)]);
            }
        }
    }
    let middle = [(low[0] + high[0]) as f32 * 0.5, (low[1] + high[1]) as f32 * 0.5];
    let extent = ((high[0] - low[0]) as f32 * HEIGHT as f32 / WIDTH as f32).max((high[1] - low[1]) as f32) * 0.5 + 8.0;
    let up = extent / (30.0f32).to_radians().tan();
    let turn = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    scene.spawn((
        Camera::default(),
        Transform::new(Vec3::new(middle[0], floor as f32 + up, middle[1]), turn, Vec3::ONE),
    ));
    let towards_sun = Vec3::new(-0.3, 0.9, -0.2).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.45, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &scene, 150);
    keep_image("blocks dungeon.png", &pixels);
}

/// A world of blocks `size` a side, `side` blocks across, made from
/// `world` with its corner at `position`, with `biomes` (each a biome and
/// its top block's name and colour) after the world's own three blocks,
/// seen from `eye` metres up over its corner, turned `yaw` round and tipped
/// `pitch` down, and kept as `name`. Gives the pixels and how many chunks
/// were drawn.
fn generated_view(
    renderer: &mut Renderer,
    world: &voxear_terrain::generate::WorldGen,
    (size, side, position): (f32, u32, [f32; 2]),
    biomes: &[(voxear_terrain::generate::BiomeDef, &str, [f32; 3])],
    (eye, yaw, pitch): (Vec3, f32, f32),
    name: &str,
) -> (Vec<u8>, usize) {
    use voxear_terrain::{
        blocks::{
            file::{BlockKind, Ground},
            generate::BlockGenerate,
        },
        generate::{BiomeBlocks, Biomes, WorldBiomes},
    };
    let generate = BlockGenerate { caves: 0.0, bottom: -200, position, ..BlockGenerate::default() };
    let mut file = BlockFile {
        block_size: size,
        ground: Ground { size: [side, side], ..Ground::default() },
        generate: Some(generate),
        ..BlockFile::default()
    };
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.05, 0.3, 0.45]) });
    file.generate.as_mut().unwrap().sea = file.blocks.len() as u16;
    let mut defs = Vec::new();
    let mut blocks = Vec::new();
    for (biome, block, colour) in biomes {
        file.blocks.push(BlockKind::colored(block, *colour));
        defs.push(biome.clone());
        blocks.push(BiomeBlocks { top: file.blocks.len() as u16, ..BiomeBlocks::default() });
    }
    if !defs.is_empty() {
        let placed = WorldBiomes { biomes: Biomes::new(defs, 15.0, 0.3, world.seed()), blocks };
        file.generate.as_mut().unwrap().placed = Some(Arc::new(placed));
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
                leafy: false,
                mixed: false,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut scene = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let made = BlockWorld::made(file.make_ground(Some(world)), &faces, size, false);
    let chunks = made.meshes.len();
    scene.spawn((made, look, Transform::default()));
    let turn = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    scene.spawn((Camera::default(), Transform::new(eye, turn, Vec3::ONE)));
    let towards_sun = Vec3::new(0.45, 0.7, 0.25).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(renderer, &scene, 150);
    keep_image(name, &pixels);
    (pixels, chunks)
}

/// How many pixels are sky: brighter blue than anything on the ground.
fn sky_pixels(pixels: &[u8]) -> usize {
    (0..WIDTH * HEIGHT)
        .filter(|at| {
            let [r, _, b] = rgb_at(pixels, at % WIDTH, at / WIDTH);
            b > r + 15 && b > 120
        })
        .count()
}

#[test]
fn fantasy_presets_stand_towers_blades_and_great_peaks_and_cut_rifts_down_to_floors_of_their_own() {
    use voxear_terrain::generate::{BiomeDef, starters};
    let Some(mut renderer) = renderer() else { return };
    let preset = |name: &str| starters().into_iter().find(|starter| starter.name == name).unwrap().world(1, 1.0);
    let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
    let grass = (land.clone(), "Grass", [0.36, 0.6, 0.22]);
    // An eye `over` metres over the ground at `at` in a world whose corner is at `corner`, in the world's own metres.
    let eye = |world: &voxear_terrain::generate::WorldGen, corner: [f32; 2], at: [f32; 2], over: f32| {
        Vec3::new(at[0], world.height_at(corner[0] + at[0], corner[1] + at[1]) + over, at[1])
    };
    // Towers of karst over a green plain, from a little over the plain at one corner.
    let karst = preset("Karst");
    let (pixels, chunks) = generated_view(
        &mut renderer,
        &karst,
        (2.0, 400, [0.0, 0.0]),
        std::slice::from_ref(&grass),
        (eye(&karst, [0.0, 0.0], [20.0, 20.0], 40.0), 0.785, 0.08),
        "blocks karst.png",
    );
    assert!(chunks > 100, "{chunks} chunks of karst");
    assert!(sky_pixels(&pixels) < (WIDTH * HEIGHT) as usize * 3 / 4, "towers stand into the sky");
    // Blades of rock across green valleys.
    let shattered = preset("Shattered");
    let (_, chunks) = generated_view(
        &mut renderer,
        &shattered,
        (2.0, 400, [0.0, 0.0]),
        std::slice::from_ref(&grass),
        (eye(&shattered, [0.0, 0.0], [20.0, 20.0], 30.0), 0.785, 0.1),
        "blocks shattered.png",
    );
    assert!(chunks > 100, "{chunks} chunks of blades");
    // Great peaks from the level land between their ranges, in blocks 8 m a side: the tallest summit within
    // 8 km two thirds of the way across, looked at from the near corner.
    let peaks = preset("Great Peaks");
    let summit = (0..1600)
        .map(|at| [(at % 40) as f32 * 400.0 - 8000.0, (at / 40) as f32 * 400.0 - 8000.0])
        .max_by(|a, b| peaks.height_at(a[0], a[1]).total_cmp(&peaks.height_at(b[0], b[1])))
        .unwrap();
    let corner = [summit[0] - 2800.0, summit[1] - 2800.0];
    let snow = (BiomeDef { height: Some([900.0, 9000.0]), ..land.clone() }, "Snow", [0.93, 0.95, 0.97]);
    let (_, chunks) = generated_view(
        &mut renderer,
        &peaks,
        (8.0, 512, corner),
        &[grass.clone(), snow],
        (eye(&peaks, corner, [60.0, 60.0], 150.0), 0.785, -0.12),
        "blocks great peaks.png",
    );
    assert!(chunks > 100, "{chunks} chunks of peaks");
    // Rifts from over their rim, looking down into the deepest within 4 km: fern on the floors, grass on the land.
    let rifts = preset("Rifts");
    let (xs, zs): (Vec<f32>, Vec<f32>) =
        (0..6400).map(|at| ((at % 80) as f32 * 50.0 - 2000.0, (at / 80) as f32 * 50.0 - 2000.0)).unzip();
    let mut heights = vec![0.0; xs.len()];
    rifts.heights(&xs, &zs, &mut heights);
    let sunk = rifts.sunk(&xs, &zs, &heights).expect("the rifts' Sunk");
    // The deepest place open to the sky, with no roof standing over it.
    let roofed = rifts.volume().expect("the rifts' Volume").standing(&xs, &zs, &heights);
    let open = (0..xs.len()).filter(|&at| roofed[at].is_empty());
    let deepest = open.max_by(|&a, &b| sunk[a].total_cmp(&sunk[b])).unwrap();
    let corner = [xs[deepest] - 600.0, zs[deepest] - 600.0];
    let fern = (BiomeDef { sunk: Some([40.0, 5000.0]), ..land.clone() }, "Fern", [0.1, 0.5, 0.45]);
    let fern_again = fern.clone();
    let (_, chunks) = generated_view(
        &mut renderer,
        &rifts,
        (2.0, 512, corner),
        &[grass, fern_again],
        (eye(&rifts, corner, [520.0, 520.0], 80.0), 0.785, 0.85),
        "blocks rifts.png",
    );
    assert!(chunks > 100, "{chunks} chunks of rifts");
    // Straight down into it from over its deepest open place: fern on its floor.
    let (pixels, _) = generated_view(
        &mut renderer,
        &rifts,
        (2.0, 512, corner),
        &[(land.clone(), "Grass", [0.36, 0.6, 0.22]), fern],
        (eye(&rifts, corner, [600.0, 600.0], sunk[deepest] + 60.0), 0.0, std::f32::consts::FRAC_PI_2),
        "blocks rift floor.png",
    );
    let teal = (0..WIDTH * HEIGHT)
        .filter(|at| {
            let [r, g, b] = rgb_at(&pixels, at % WIDTH, at / WIDTH);
            g > r + 30 && b > r + 20
        })
        .count();
    assert!(teal > 500, "{teal} pixels of fern on the rift's floor");
}

#[test]
fn a_generated_river_runs_between_its_banks_and_pours_over_a_waterfall_into_a_pool() {
    use voxear_terrain::generate::{BiomeDef, starters};
    let Some(mut renderer) = renderer() else { return };
    let preset = |name: &str| starters().into_iter().find(|starter| starter.name == name).unwrap().world(7, 1.0);
    let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
    let grass = (land, "Grass", [0.36, 0.6, 0.22]);
    // Water's own colour, darker than the sky: lake green, blue over the ground under it.
    let water = |pixels: &[u8]| {
        (0..WIDTH * HEIGHT)
            .filter(|at| {
                let [r, g, b] = rgb_at(pixels, at % WIDTH, at / WIDTH);
                g > r + 15 && b > r + 15 && b < 150
            })
            .count()
    };
    // A river through rolling hills, from over its bank 30 m off, looking down at it.
    let hills = preset("Rolling Hills");
    let (xs, zs): (Vec<f32>, Vec<f32>) = (0..200 * 200).map(|at| ((at % 200) as f32 * 8.0, (at / 200) as f32 * 8.0)).unzip();
    let (level, _) = hills.water(&xs, &zs).unwrap();
    let flowing = |at: usize| hills.current(xs[at], zs[at]).is_some_and(|[x, z]| x * x + z * z > 0.1);
    let river = (0..xs.len()).find(|&at| level[at] > 2.0 && flowing(at)).expect("a river");
    let corner = [xs[river] - 128.0, zs[river] - 128.0];
    let eye = Vec3::new(107.0, level[river] + 14.0, 107.0);
    let (pixels, _) = generated_view(
        &mut renderer,
        &hills,
        (1.0, 256, corner),
        std::slice::from_ref(&grass),
        (eye, 0.785, 0.42),
        "blocks river.png",
    );
    assert!(water(&pixels) > 2000, "{} pixels of the river", water(&pixels));
    // A waterfall down a mountainside, from 30 m off at its pool's height and a little over.
    let alpine = preset("Alpine");
    let (xs, zs): (Vec<f32>, Vec<f32>) =
        (0..400 * 400).map(|at| ((at % 400) as f32 * 6.0 - 1200.0, (at / 400) as f32 * 6.0 - 1200.0)).unzip();
    let (level, _) = alpine.water(&xs, &zs).unwrap();
    let wet = |at: usize| level[at] > -1000.0;
    let fall = (0..xs.len())
        .filter(|&at| wet(at) && at % 400 + 1 < 400 && wet(at + 1))
        .find_map(|at| {
            let line: (Vec<f32>, Vec<f32>) = (0..=12).map(|k| (xs[at] + k as f32 * 0.5, zs[at])).unzip();
            let (top, falls) = alpine.water(&line.0, &line.1).unwrap();
            (0..line.0.len()).find(|&k| falls[k] > 4.0).map(|k| (line.0[k], line.1[k], top[k], falls[k]))
        })
        .expect("a waterfall");
    let (fx, fz, top, height) = fall;
    let corner = [fx - 128.0, fz - 128.0];
    let eye = Vec3::new(100.0, top - height + 6.0, 128.0);
    let (pixels, _) = generated_view(
        &mut renderer,
        &alpine,
        (1.0, 256, corner),
        std::slice::from_ref(&grass),
        (eye, std::f32::consts::FRAC_PI_2, 0.0),
        "blocks waterfall.png",
    );
    assert!(water(&pixels) > 500, "{} pixels of the waterfall and its pool", water(&pixels));
}

#[test]
fn a_great_cave_mouth_opens_in_the_hillside_into_a_hall() {
    use voxear_terrain::generate::{BiomeDef, starters};
    let Some(mut renderer) = renderer() else { return };
    let caves = starters().into_iter().find(|starter| starter.name == "Highlands").unwrap().world(1, 1.0);
    let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
    let grass = (land, "Grass", [0.36, 0.6, 0.22]);
    let rooms = caves.caves().expect("its caves").rooms_near([0.0, 200.0, 0.0], 2000.0);
    let mouth = rooms.iter().find(|room| room.mouth).expect("a mouth");
    let out = mouth.out.expect("where its passage comes out");
    let corner = [out[0] - 128.0, out[2] - 128.0];
    // From 45 m out down the hillside and a little over it, looking back at its mouth.
    let (dx, dz) = (out[0] - mouth.at[0], out[2] - mouth.at[2]);
    let long = (dx * dx + dz * dz).sqrt().max(0.01);
    let (ax, az) = (dx / long, dz / long);
    let eye = Vec3::new(128.0 + ax * 45.0, out[1] + 12.0, 128.0 + az * 45.0);
    let yaw = (-ax).atan2(-az);
    let (pixels, _) = generated_view(
        &mut renderer,
        &caves,
        (1.0, 256, corner),
        std::slice::from_ref(&grass),
        (eye, yaw, 0.2),
        "blocks cave mouth.png",
    );
    assert!(sky_pixels(&pixels) > 0);
    // From the hall's floor, looking out through its passage.
    let eye = Vec3::new(mouth.floor[0] - corner[0], mouth.floor[1] + 4.0, mouth.floor[2] - corner[1]);
    let (inside, _) = generated_view(
        &mut renderer,
        &caves,
        (1.0, 256, corner),
        std::slice::from_ref(&grass),
        (eye, (ax).atan2(az), 0.0),
        "blocks cave mouth inside.png",
    );
    assert!(sky_pixels(&inside) > 100, "sky through the mouth: {}", sky_pixels(&inside));
}

/// A picture of bricks, 16 pixels a side: three courses with dark mortar between, each brick a shade of its own.
fn bricks(renderer: &mut Renderer) -> voxear_render::TextureId {
    let size = 16u32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let (x, y) = (i % size, i / size);
            let course = y / 5;
            let along = (x + course * 4) % 8;
            let mortar = y % 5 == 4 || along == 7;
            let shade = 150 + ((course * 3 + (x + course * 4) / 8) % 3) * 22;
            let [r, g, b] = if mortar { [58, 54, 52] } else { [shade, shade * 3 / 5, shade / 2] };
            [r as u8, g as u8, b as u8, 255]
        })
        .collect();
    let source = voxear_render::TextureSource { name: "bricks", width: size, height: size, rgba: &rgba, srgb: true };
    renderer.add_texture(&source).unwrap()
}

/// A field of grass with a wall of bricks across it, its faces cut into `voxels`, seen from `eye` looking along
/// `towards`; and how long the frame's GPU work took at its least over a few frames, in milliseconds.
fn small_cubes_render(voxels: voxear_render::BlockVoxels, eye: Vec3, towards: Vec3) -> Option<(Vec<u8>, f32)> {
    let mut renderer = renderer()?;
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 4;
    let mut blocks = file.make_ground(None);
    for x in 8..24 {
        for y in 4..9 {
            blocks.set([x, y, 18], 3);
        }
    }
    let picture = bricks(&mut renderer);
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            // Stone is the wall's: bricks. Grass and dirt have no picture, only their colours.
            let stone = (r - g).abs() < 0.05;
            let desc = match stone {
                true => MaterialDesc { base_color_texture: Some(picture), roughness: 1.0, ..MaterialDesc::default() },
                false => MaterialDesc { base_color: [r, g, b, 1.0], roughness: 1.0, ..MaterialDesc::default() },
            };
            SurfaceLook {
                material: renderer.add_material(&desc).unwrap(),
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
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels };
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    let towards = towards.normalize();
    let facing = Quat::from_rotation_y(towards.x.atan2(towards.z)) * Quat::from_rotation_x(-towards.y.asin());
    world.spawn((Camera::default(), Transform::new(eye, facing, Vec3::ONE)));
    // The sun from behind the camera's left shoulder, high enough to light the ground and the wall.
    let towards_sun = Vec3::new(-0.5, 0.7, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let mut time = 0.0;
    let mut least = f32::INFINITY;
    for _ in 0..40 {
        time += 1.0 / 60.0;
        renderer.render(&world, time).expect("frame renders");
        let spent: f32 = renderer.timings().iter().map(|(_, ms)| *ms).sum();
        if spent > 0.0 {
            least = least.min(spent);
        }
    }
    Some((renderer.read_pixels().expect("pixels").2, least))
}

#[test]
fn a_worlds_faces_are_cut_into_small_cubes_near_the_camera_and_plain_far_off() {
    use voxear_render::BlockVoxels;
    // Standing on the grass, looking a little down at the wall six metres off.
    let (eye, towards) = (Vec3::new(16.0, 5.7, 11.5), Vec3::new(0.15, -0.2, 1.0));
    let near = |voxels: BlockVoxels| small_cubes_render(voxels, eye, towards);
    let Some((plain, plain_ms)) = near(BlockVoxels::default()) else { return };
    let (cubes, cubes_ms) = near(BlockVoxels::of(8)).unwrap();
    let (flat, _) = near(BlockVoxels { count: 8, relief: 0.0 }).unwrap();
    let (fine, _) = near(BlockVoxels::of(16)).unwrap();
    keep_image("blocks plain faces.png", &plain);
    keep_image("blocks small cubes 8.png", &cubes);
    keep_image("blocks small cubes 8 flat.png", &flat);
    keep_image("blocks small cubes 16.png", &fine);
    eprintln!("a frame's passes: {plain_ms:.2} ms plain, {cubes_ms:.2} ms with small cubes");
    let unlike = |a: &[u8], b: &[u8]| {
        let pairs = a.chunks_exact(4).zip(b.chunks_exact(4));
        pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 6)).count() as f32 / (WIDTH * HEIGHT) as f32
    };
    assert!(unlike(&cubes, &plain) > 0.2, "small cubes show over the faces: {}", unlike(&cubes, &plain));
    assert!(unlike(&cubes, &flat) > 0.05, "sunk into them, not only squares: {}", unlike(&cubes, &flat));
    assert!(unlike(&fine, &cubes) > 0.1, "twice as many are finer: {}", unlike(&fine, &cubes));
    // The faces keep their colours: grass still green and about as bright all told.
    let mean = |pixels: &[u8]| {
        let sum = |c: usize| pixels.chunks_exact(4).map(|p| u64::from(p[c])).sum::<u64>() / u64::from(WIDTH * HEIGHT);
        [sum(0), sum(1), sum(2)]
    };
    let (was, now) = (mean(&plain), mean(&cubes));
    assert!((0..3).all(|c| now[c] * 100 > was[c] * 75 && now[c] <= was[c] + 6), "{now:?} against {was:?}");
    // The same picture twice.
    assert!(near(BlockVoxels::of(8)).unwrap().0 == cubes, "the same small cubes each time");

    // From high above, the small cubes are too small to tell apart, and the faces are as they are without.
    let (above, down) = (Vec3::new(16.0, 70.0, 16.0), Vec3::new(0.0, -1.0, 0.02));
    let (far_plain, _) = small_cubes_render(BlockVoxels::default(), above, down).unwrap();
    let (far_cubes, _) = small_cubes_render(BlockVoxels::of(8), above, down).unwrap();
    keep_image("blocks small cubes far.png", &far_cubes);
    assert!(unlike(&far_cubes, &far_plain) < 0.01, "plain far off: {}", unlike(&far_cubes, &far_plain));
}

/// A tree of blocks on a field of grass, a trunk with a crown of leaves over it, seen from in front: its leaves
/// swaying or not, in a Wind Zone of `wind` or none, as it is after one second and after a second and a half.
fn leaves_render(sways: bool, wind: Option<f32>) -> Option<[Vec<u8>; 2]> {
    use voxear_render::WindZone;
    use voxear_terrain::blocks::file::BlockKind;
    let mut renderer = renderer()?;
    let mut file = BlockFile::default();
    file.ground.size = [32, 32];
    file.ground.height = 4;
    file.blocks.push(BlockKind { sways, ..BlockKind::colored("Leaves", [0.16, 0.42, 0.12]) });
    let leaves = file.blocks.len() as u16;
    let mut blocks = file.make_ground(None);
    for y in 7..11 {
        for z in 14..19 {
            for x in 14..19 {
                blocks.set([x, y, z], leaves);
            }
        }
    }
    for y in 4..9 {
        blocks.set([16, y, 16], 3);
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
            let [r, g, b] = look.color;
            let desc = MaterialDesc { base_color: [r, g, b, 1.0], roughness: 1.0, ..MaterialDesc::default() };
            SurfaceLook {
                material: renderer.add_material(&desc).unwrap(),
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
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(16.5, 7.5, 4.0))));
    let towards_sun = Vec3::new(-0.5, 0.7, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if let Some(strength) = wind {
        world.spawn((WindZone { strength, ..WindZone::default() }, Transform::default()));
    }
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let mut time = 0.0;
    let mut after = |frames: u32| {
        for _ in 0..frames {
            time += 1.0 / 60.0;
            renderer.render(&world, time).expect("frame renders");
        }
        renderer.read_pixels().expect("pixels").2
    };
    Some([after(60), after(30)])
}

#[test]
fn leaves_that_sway_stir_in_the_wind_fixed_to_their_wood_and_nothing_else_moves() {
    let Some([calm, calm_later]) = leaves_render(true, None) else { return };
    let [windy, windy_later] = leaves_render(true, Some(2.0)).unwrap();
    let [stiff, stiff_later] = leaves_render(false, Some(2.0)).unwrap();
    keep_image("leaves calm.png", &calm);
    keep_image("leaves in the wind.png", &windy);
    keep_image("leaves in the wind later.png", &windy_later);
    let unlike = |a: &[u8], b: &[u8], rows: std::ops::Range<u32>| {
        let differs = |x: u32, y: u32| (0..3).any(|c| rgb_at(a, x, y)[c].abs_diff(rgb_at(b, x, y)[c]) > 6);
        let count = rows.clone().flat_map(|y| (0..WIDTH).map(move |x| (x, y))).filter(|&(x, y)| differs(x, y)).count();
        count as f32 / (rows.len() as u32 * WIDTH) as f32
    };
    let all = 0..HEIGHT;
    assert!(unlike(&calm, &calm_later, all.clone()) < 0.001, "with no wind nothing moves");
    assert!(unlike(&stiff, &stiff_later, all.clone()) < 0.001, "nor do leaves that do not sway");
    assert!(unlike(&stiff, &calm, all.clone()) < 0.001, "which are as leaves in no wind");
    let moved = unlike(&windy, &windy_later, all.clone());
    assert!(moved > 0.004, "leaves that sway stir from moment to moment: {moved}");
    assert!(unlike(&windy, &calm, all.clone()) > 0.004, "and stand apart from where they are in a calm");
    // The grass at the camera's feet is as still as ever, past the crown's shadow.
    let ground = HEIGHT * 5 / 6..HEIGHT;
    assert!(unlike(&windy, &windy_later, ground.clone()) < 0.001, "the ground does not move");
    assert!(unlike(&windy, &calm, ground) < 0.001, "nor did the wind change it");
    // The trunk stands where it stood: its dark column is in the same place.
    let trunk = |pixels: &[u8]| {
        let dark: Vec<u32> = (0..WIDTH).filter(|&x| rgb_at(pixels, x, 225).iter().sum::<i32>() < 200).collect();
        (dark.first().copied(), dark.last().copied())
    };
    let (stood, stands) = (trunk(&calm), trunk(&windy));
    assert!(stood.0.is_some() && stands == stood, "{stands:?} against {stood:?}");
}

/// A picture of leaves, 16 pixels a side: leaves in three greens with clear gaps between them.
fn leaf_picture(renderer: &mut Renderer) -> voxear_render::TextureId {
    let size = 16u32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let (x, y) = (i % size, i / size);
            let mixed = (x * 7 + y * 13 + (x / 2) * (y / 2) * 5) % 11;
            let clear = mixed < 3;
            let [r, g, b] = [[52, 120, 30], [70, 150, 38], [96, 178, 48]][(mixed % 3) as usize];
            [r, g, b, if clear { 0 } else { 255 }]
        })
        .collect();
    let source = voxear_render::TextureSource { name: "leaves", width: size, height: size, rgba: &rgba, srgb: true };
    renderer.add_texture(&source).unwrap()
}

/// How a tree's leaves and the vines on it are drawn in [`tree_render`].
#[derive(Clone, Copy, PartialEq)]
struct TreeLook {
    /// Whether its leaves are bushy.
    bushy: bool,
    /// Whether its leaves have a cut-out picture rather than a colour alone.
    pictured: bool,
    /// Whether vines hang from its crown and climb its trunk.
    vines: bool,
    wind: Option<f32>,
}

/// A birch of blocks on a field of grass, grown from a seed, seen from `eye` looking `towards`, after one second
/// and after a second and a half; with how many loose quads its chunks hold.
fn tree_render(look: TreeLook, eye: Vec3, towards: Vec3) -> Option<([Vec<u8>; 2], u32)> {
    use voxear_render::WindZone;
    use voxear_terrain::blocks::{
        block::BlockShape,
        file::BlockKind,
        plant::{PlantDef, PlantPart},
    };
    let mut renderer = renderer()?;
    let mut file = BlockFile::default();
    file.ground.size = [48, 48];
    file.ground.height = 4;
    file.blocks.push(BlockKind::colored("Birch Log", [0.86, 0.85, 0.8]));
    let wood = file.blocks.len() as u16;
    let shape = if look.bushy { BlockShape::Bushy } else { BlockShape::Cube };
    let leaf = BlockKind { sways: true, shape, cutout: look.pictured, ..BlockKind::colored("Birch Leaves", [0.3, 0.62, 0.16]) };
    file.blocks.push(BlockKind { material: look.pictured.then(|| "leaves".to_string()), ..leaf });
    let leaves = file.blocks.len() as u16;
    file.blocks.push(BlockKind { sways: true, shape: BlockShape::Vine, ..BlockKind::colored("Vine", [0.2, 0.46, 0.14]) });
    let vine = file.blocks.len() as u16;
    let mut blocks = file.make_ground(None);
    let birch = PlantDef {
        height: [8.0, 8.0],
        trunk: 1.0,
        taper: 0.0,
        bend: 0.0,
        crown_width: [7.0, 7.0],
        crown_height: [6.0, 6.0],
        leaf_size: 3.4,
        leaf_density: 0.9,
        roots: 0,
        ..PlantDef::default()
    };
    for (at, part) in birch.grow(5).blocks {
        let block = if part == PlantPart::Leaf { leaves } else { wood };
        blocks.set([24 + at[0], 4 + at[1], 24 + at[2]], block);
    }
    if look.vines {
        // Up the trunk's side towards the camera, and hanging from under the crown here and there.
        for y in 4..9 {
            blocks.set([24, y, 23], vine);
        }
        for (x, z, long) in [(21, 22, 4), (27, 21, 3), (22, 26, 5), (26, 27, 2), (20, 24, 3)] {
            let Some(under) = (5..16).find(|&y| blocks.get([x, y, z]) == leaves && blocks.get([x, y - 1, z]) == 0) else {
                continue;
            };
            for y in under - long..under {
                blocks.set([x, y, z], vine);
            }
        }
    }
    let picture = look.pictured.then(|| leaf_picture(&mut renderer));
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|face| {
            // A block's colour is sRGB; a material's is linear.
            let [r, g, b] = face.color.map(|c| c.powf(2.2));
            let pictured = face.material.is_some();
            let desc = MaterialDesc {
                base_color: if pictured { [1.0; 4] } else { [r, g, b, 1.0] },
                base_color_texture: picture.filter(|_| pictured),
                alpha_cutoff: pictured.then_some(0.5),
                roughness: 1.0,
                ..MaterialDesc::default()
            };
            SurfaceLook {
                material: renderer.add_material(&desc).unwrap(),
                size: 1.0,
                sharpness: 0.0,
                triplanar: true,
                tracks: 0.0,
                fills_in: 0.0,
                liquid: Default::default(),
                glitter: false,
                leafy: face.leafy,
                mixed: face.mixed,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let block_look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let made = BlockWorld::new(blocks, &faces, 1.0);
    let loose = made.meshes.values().map(|mesh| mesh.loose().1).sum();
    world.spawn((made, block_look, Transform::default()));
    let towards = towards.normalize();
    let turned = Quat::from_rotation_y(towards.x.atan2(towards.z)) * Quat::from_rotation_x(-towards.y.asin());
    world.spawn((Camera::default(), Transform::new(eye, turned, Vec3::ONE)));
    let towards_sun = Vec3::new(-0.5, 0.7, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if let Some(strength) = look.wind {
        world.spawn((WindZone { strength, ..WindZone::default() }, Transform::default()));
    }
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let mut time = 0.0;
    let mut after = |frames: u32| {
        for _ in 0..frames {
            time += 1.0 / 60.0;
            renderer.render(&world, time).expect("frame renders");
        }
        renderer.read_pixels().expect("pixels").2
    };
    Some(([after(60), after(30)], loose))
}

#[test]
fn bushy_leaves_reach_out_past_their_blocks_in_tufts_with_a_picture_or_none_and_sway() {
    let (eye, towards) = (Vec3::new(21.0, 10.5, 14.5), Vec3::new(0.3, 0.0, 1.0));
    let plain = TreeLook { bushy: false, pictured: false, vines: false, wind: None };
    let Some(([cubes, _], none)) = tree_render(plain, eye, towards) else { return };
    let ([bushy, bushy_later], loose) = tree_render(TreeLook { bushy: true, ..plain }, eye, towards).unwrap();
    let ([pictured, _], _) = tree_render(TreeLook { bushy: true, pictured: true, ..plain }, eye, towards).unwrap();
    let ([flat, _], _) = tree_render(TreeLook { pictured: true, ..plain }, eye, towards).unwrap();
    let ([windy, windy_later], _) = tree_render(TreeLook { bushy: true, wind: Some(2.0), ..plain }, eye, towards).unwrap();
    keep_image("leaves cubes.png", &cubes);
    keep_image("leaves bushy.png", &bushy);
    keep_image("leaves bushy pictured.png", &pictured);
    keep_image("leaves bushy later.png", &bushy_later);
    keep_image("leaves cubes pictured.png", &flat);
    keep_image("leaves bushy in the wind.png", &windy);
    assert_eq!(none, 0, "a tree of cubes has no sheets");
    assert!(loose > 100 && loose % 3 == 0, "three sheets to each block of the crown's outside: {loose}");
    // How much of the picture is leaves: green well over red and blue.
    let leafy = |pixels: &[u8]| {
        let green = |p: &[u8]| i32::from(p[1]) > i32::from(p[0]) + 6 && i32::from(p[1]) > i32::from(p[2]) + 6;
        let upper = &pixels[..(WIDTH * HEIGHT / 2 * 4) as usize];
        upper.chunks_exact(4).filter(|p| green(p)).count() as f32 / (WIDTH * HEIGHT / 2) as f32
    };
    eprintln!("leaves: {} cubes, {} bushy, {} pictured cubes, {} pictured bushy", leafy(&cubes), leafy(&bushy), leafy(&flat), leafy(&pictured));
    // The crown of tufts is wider and taller than the crown of cubes: leaves stand where the sky was.
    assert!(leafy(&bushy) > leafy(&cubes) * 1.04, "{} against {}", leafy(&bushy), leafy(&cubes));
    assert!(leafy(&pictured) > leafy(&flat) * 1.1, "{} against {}", leafy(&pictured), leafy(&flat));
    // Its outline is ragged: along the crown's edge, sky and leaf change places far more often than a cube's does.
    let edges = |pixels: &[u8]| {
        let green = |x: u32, y: u32| {
            let [r, g, b] = rgb_at(pixels, x, y);
            g > r + 6 && g > b + 6
        };
        (0..HEIGHT / 2).map(|y| (1..WIDTH).filter(|&x| green(x, y) != green(x - 1, y)).count()).sum::<usize>()
    };
    eprintln!("edges: {} cubes, {} bushy", edges(&cubes), edges(&bushy));
    assert!(edges(&bushy) > edges(&cubes) * 2, "{} against {}", edges(&bushy), edges(&cubes));
    // Still with no wind, stirring in it.
    let unlike = |a: &[u8], b: &[u8]| {
        let pairs = a.chunks_exact(4).zip(b.chunks_exact(4));
        pairs.filter(|(a, b)| (0..3).any(|c| a[c].abs_diff(b[c]) > 6)).count() as f32 / (WIDTH * HEIGHT) as f32
    };
    // A still crown is the same but for a few pixels along its fine edges and shadows.
    let (still, stirred) = (unlike(&bushy, &bushy_later), unlike(&windy, &windy_later));
    eprintln!("moved: {still} with no wind, {stirred} in it");
    assert!(still < 0.01, "with no wind nothing moves: {still}");
    assert!(stirred > 0.03 && stirred > still * 4.0, "tufts stir in the wind: {stirred} against {still}");
}


#[test]
fn vines_cling_to_the_trunk_and_hang_from_the_crown_swaying_where_they_hang_free() {
    let (eye, towards) = (Vec3::new(21.0, 8.5, 15.5), Vec3::new(0.3, 0.0, 1.0));
    let bare = TreeLook { bushy: true, pictured: false, vines: false, wind: None };
    let Some(([without, _], tufts)) = tree_render(bare, eye, towards) else { return };
    let ([with, later], loose) = tree_render(TreeLook { vines: true, ..bare }, eye, towards).unwrap();
    let ([windy, windy_later], _) = tree_render(TreeLook { vines: true, wind: Some(2.0), ..bare }, eye, towards).unwrap();
    keep_image("vines none.png", &without);
    keep_image("vines.png", &with);
    keep_image("vines in the wind.png", &windy);
    assert!(loose > tufts + 10, "the vines' sheets beside the leaves': {loose} against {tufts}");
    let unlike = |a: &[u8], b: &[u8], columns: std::ops::Range<u32>, rows: std::ops::Range<u32>| {
        let differs = |x: u32, y: u32| (0..3).any(|c| rgb_at(a, x, y)[c].abs_diff(rgb_at(b, x, y)[c]) > 6);
        let all = rows.clone().flat_map(|y| columns.clone().map(move |x| (x, y)));
        all.filter(|&(x, y)| differs(x, y)).count() as f32 / (rows.len() * columns.len()) as f32
    };
    // Vines show under the crown and on the trunk, where there were none.
    let under = HEIGHT / 2..HEIGHT * 5 / 6;
    let shown = unlike(&with, &without, 0..WIDTH, under.clone());
    assert!(shown > 0.01, "vines under the crown: {shown}");
    // The one on the trunk stands still in the wind; those hanging free sway.
    let (still, swayed) = (unlike(&with, &later, 0..WIDTH, under.clone()), unlike(&windy, &windy_later, 0..WIDTH, under));
    eprintln!("vines: {shown} shown, {still} moved with no wind, {swayed} in it");
    assert!(still < 0.01 && swayed > still * 3.0 + 0.004, "{swayed} against {still}");
}

#[test]
fn a_look_pack_colours_the_water_of_blocks_and_leaves_the_ground_as_it_was() {
    use voxear_render::{WaterLook, WornLook};
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond(&mut renderer, &[]);
    // Straight down on the pond from above its middle: water in the middle of the picture, grass round it.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(32.0, 44.0, 32.0), down, Vec3::ONE)));
    let own = render(&mut renderer, &world, 8);
    // The level's own lighting, with a pack's water over it: as red as wine.
    let worn = WornLook {
        sky: *world.resource::<Sky>().unwrap(),
        sun: None,
        post: *world.resource::<PostProcess>().unwrap(),
        voxels: None,
        water: WaterLook { deep: Some([0.7, 0.05, 0.1]), ..WaterLook::default() },
        shader: None,
    };
    world.insert_resource(worn);
    let wine = render(&mut renderer, &world, 8);
    keep_image("block water own.png", &own);
    keep_image("block water of a pack.png", &wine);
    let (middle, corner) = ((WIDTH / 2, HEIGHT / 2), (20, 20));
    let (was, now) = (rgb_at(&own, middle.0, middle.1), rgb_at(&wine, middle.0, middle.1));
    assert!(was[1] > was[0] + 10, "the pond is its block's green to begin with: {was:?}");
    assert!(now[0] > now[1] + 20 && now[0] > was[0] + 20, "and the pack's red under it: {now:?} against {was:?}");
    let (grass, still) = (rgb_at(&own, corner.0, corner.1), rgb_at(&wine, corner.0, corner.1));
    assert!((0..3).all(|c| (grass[c] - still[c]).abs() <= 2), "the grass is as it was: {still:?} against {grass:?}");
    // A pack that sets no water leaves each block's own.
    world.insert_resource(WornLook { water: WaterLook::default(), ..worn });
    let back = render(&mut renderer, &world, 8);
    let again = rgb_at(&back, middle.0, middle.1);
    assert!((0..3).all(|c| (again[c] - was[c]).abs() <= 3), "its own again: {again:?} against {was:?}");
}
