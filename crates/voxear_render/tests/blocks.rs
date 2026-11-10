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
use voxear_terrain::blocks::{BlockFaces, BlockWorld, file::BlockFile};

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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces) };
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    world.spawn((world_blocks, BlockLook { material: default, surfaces: Arc::from(surfaces) }, Transform::default()));
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
    assert_eq!(faces[1], BlockFaces::new(0, 1, 1), "top, sides and bottom");
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces) };
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
fn a_planet_drawn_smooth_is_round_its_hills_lit_on_its_sunny_side() {
    use voxear_terrain::{
        blocks::generate::{BlockGenerate, BlockPlanet},
        generate::Preset,
    };
    let Some(mut renderer) = renderer() else { return };
    // A planet 30 m to its sea, its hills 4 m high, at the middle of the world.
    let mut hills = Preset::RollingHills.world();
    hills.height = 4.0;
    let generate = BlockGenerate { caves: 0.0, planet: Some(BlockPlanet { radius: 30.0 }), ..BlockGenerate::default() };
    let file = BlockFile { generate: Some(generate), smooth: true, ..BlockFile::default() };
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces) };
    world.spawn((BlockWorld::made(file.make_ground(Some(&hills)), &faces, 1.0, true), look, Transform::default()));
    // Seen from 100 m along +z, looking back at its middle; the sun from the west.
    let back = Quat::from_rotation_y(std::f32::consts::PI);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 0.0, 100.0), back, Vec3::ONE)));
    let towards_sun = Vec3::new(-1.0, 0.3, 0.4).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 4);
    keep_image("blocks planet.png", &pixels);
    // About 31 m round seen from 100 m: 17.4 degrees, a little under 105 pixels at 360 pixels for 60 degrees.
    let reach = (31.0f32 / 100.0).atan().to_degrees() / 30.0 * (HEIGHT as f32 / 2.0);
    let (cx, cy) = (WIDTH as f32 / 2.0, HEIGHT as f32 / 2.0);
    // Its grass, lit or in its own shadow, is greener than anything round it.
    let grass = |[r, g, b]: [i32; 3]| g > r + 3 && g > b + 3;
    let at = |dx: f32, dy: f32| rgb_at(&pixels, (cx + dx) as u32, (cy + dy) as u32);
    for (dx, dy) in [(0.0, 0.0), (0.8, 0.0), (-0.8, 0.0), (0.0, 0.8), (0.0, -0.8), (0.55, 0.55)] {
        assert!(
            grass(at(dx * reach, dy * reach)),
            "ground at ({dx}, {dy}) of its reach: {:?}",
            at(dx * reach, dy * reach)
        );
    }
    for (dx, dy) in [(1.25, 0.0), (-1.25, 0.0), (0.0, 1.25), (0.0, -1.25)] {
        assert!(
            !grass(at(dx * reach, dy * reach)),
            "nothing past it at ({dx}, {dy}): {:?}",
            at(dx * reach, dy * reach)
        );
    }
    // Lit on its sunny side, the screen's right, darker on its night side.
    let brightness = |dx: f32| at(dx * reach, 0.0).iter().sum::<i32>();
    assert!(brightness(0.6) > brightness(-0.6) + 60, "{} against {}", brightness(0.6), brightness(-0.6));
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces) };
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
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    world.spawn((blocks, BlockLook { material: default, surfaces: Arc::from(surfaces) }, Transform::default()));
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
