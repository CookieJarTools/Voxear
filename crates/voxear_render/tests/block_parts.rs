//! Blocks of parts drawn on a real GPU, offscreen, beside whole blocks:
//! slabs, stairs, fences, walls, panes, doors, trap doors, levers, buttons
//! and chains, and a door caught half way through its swing. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, GpuBlockMotion, Light, MaterialDesc, PostProcess, Renderer,
    RendererSettings, Sky, SurfaceLook,
};
use voxear_scene::Transform;
use voxear_terrain::blocks::{
    BlockMap, BlockWorld,
    block::{BlockDef, BlockShape},
    chunk::BlockId,
    file::{BlockFile, BlockKind},
    shape::{BlockFacing, BlockHalf, BlockState},
    usable::MovingBlock,
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

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

/// Where the camera stands, looking along -z.
const EYE: Vec3 = Vec3::new(16.0, 5.5, 48.0);

/// The colour at the point `at` in the world, as the camera at [`EYE`] sees it.
fn seen(pixels: &[u8], at: Vec3) -> [i32; 3] {
    let away = EYE.z - at.z;
    let half = (30f32).to_radians().tan() * away;
    let u = (at.x - EYE.x) / (half * WIDTH as f32 / HEIGHT as f32);
    let v = (at.y - EYE.y) / half;
    let (x, y) = (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32);
    let i = ((y * WIDTH + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
}

/// Whether `rgb` is the colour `want` lit: nearest to it of the scene's colours, by hue.
fn is(rgb: [i32; 3], want: [f32; 3]) -> bool {
    let sum = rgb.iter().sum::<i32>().max(1) as f32;
    let share = rgb.map(|c| c as f32 / sum);
    let wanted = want.iter().sum::<f32>().max(1e-3);
    (0..3).all(|i| (share[i] - want[i] / wanted).abs() < 0.12)
}

const GRASS: [f32; 3] = [0.33, 0.52, 0.2];
const RED: [f32; 3] = [0.8, 0.12, 0.1];
const BLUE: [f32; 3] = [0.15, 0.25, 0.85];
const WOOD: [f32; 3] = [0.75, 0.55, 0.15];
const WHITE: [f32; 3] = [0.8, 0.8, 0.8];

/// The world's file: grass, red cubes, and after them a block of each shape
/// and state asked for, numbered from 3 in order, each its own colour.
fn file_of(parts: &[(BlockShape, BlockState, [f32; 3])]) -> BlockFile {
    let mut file = BlockFile {
        blocks: vec![BlockKind::colored("Grass", GRASS), BlockKind::colored("Red", RED)],
        ..BlockFile::default()
    };
    file.ground = voxear_terrain::blocks::file::Ground { size: [48, 48], height: 4, top: 1, under: 1, under_depth: 0, bottom: 1 };
    for (at, &(shape, state, color)) in parts.iter().enumerate() {
        let mut kind = BlockKind::of_state(&format!("Blocks/Part {at}.block"), state.normalized(shape).to_byte());
        BlockDef { shape, ..BlockDef::colored(color) }.fill(&mut kind);
        file.blocks.push(kind);
    }
    file
}

/// A world drawn as `file` says, with the camera at [`EYE`] and the sun.
fn scene(renderer: &mut Renderer, file: &BlockFile, blocks: BlockMap) -> (World, voxear_ecs::Entity) {
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
                leafy: look.leafy,
                mixed: false,
            }
        })
        .collect();
    let motions: Vec<GpuBlockMotion> =
        file.motions().iter().map(|(motion, swing)| GpuBlockMotion::of(motion.as_ref(), *swing)).collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default(), motions: motions.into() };
    let mut world = World::new();
    let entity = world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    world.spawn((Camera::default(), Transform::new(EYE, Quat::from_rotation_y(std::f32::consts::PI), Vec3::ONE)));
    let towards_sun = Vec3::new(0.4, 0.8, 0.6).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.5, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    (world, entity)
}

fn state(facing: BlockFacing) -> BlockState {
    BlockState { facing, ..BlockState::default() }
}

#[test]
fn each_shape_is_drawn_as_its_parts_beside_whole_blocks() {
    let Some(mut renderer) = renderer() else { return };
    let door = BlockState::default();
    let parts = [
        (BlockShape::Slab, state(BlockFacing::Down), BLUE),
        (BlockShape::Slab, state(BlockFacing::North), BLUE),
        (BlockShape::Stairs, door, BLUE),
        (BlockShape::Fence, door, WOOD),
        (BlockShape::Wall, door, WHITE),
        (BlockShape::Pane, door, BLUE),
        (BlockShape::Door, door, WOOD),
        (BlockShape::Door, BlockState { half: BlockHalf::Upper, ..door }, WOOD),
        (BlockShape::Door, BlockState { open: true, ..door }, WOOD),
        (BlockShape::Door, BlockState { open: true, half: BlockHalf::Upper, ..door }, WOOD),
        (BlockShape::TrapDoor, door, WOOD),
        (BlockShape::Lever, BlockState { on: true, ..state(BlockFacing::Down) }, WHITE),
        (BlockShape::Button, state(BlockFacing::Down), WHITE),
        (BlockShape::Chain, door, WHITE),
    ];
    let file = file_of(&parts);
    let number = |at: usize| (at + 3) as BlockId;
    let mut blocks = file.make_ground(None);
    let row = 32;
    let mut put = |x: i32, y: i32, block: BlockId| blocks.set([x, y, row], block);
    put(2, 4, 2);
    put(2, 5, 2);
    put(4, 4, number(0));
    put(6, 4, number(1));
    put(8, 4, number(2));
    for x in 10..=12 {
        put(x, 4, number(3));
    }
    put(14, 4, number(4));
    put(15, 4, 2);
    put(16, 4, number(5));
    put(17, 4, number(5));
    put(19, 4, number(6));
    put(19, 5, number(7));
    put(21, 4, number(8));
    put(21, 5, number(9));
    put(23, 4, number(10));
    put(25, 4, number(11));
    put(27, 4, number(12));
    put(29, 7, 2);
    put(29, 6, number(13));
    put(29, 5, number(13));
    put(31, 4, 2);
    let (world, _) = scene(&mut renderer, &file, blocks);
    let pixels = render(&mut renderer, &world, 4);
    keep_image("block parts.png", &pixels);
    let front = row as f32 + 1.0;
    // A whole block fills its column; a slab its lower half alone, the ground showing over it.
    assert!(is(seen(&pixels, Vec3::new(2.5, 4.75, front)), RED), "the cube's top half");
    assert!(is(seen(&pixels, Vec3::new(4.5, 4.25, front)), BLUE), "the slab's lower half");
    assert!(!is(seen(&pixels, Vec3::new(4.5, 4.8, front)), BLUE), "nothing over the slab");
    // Fence rails between the posts, a gap between the rails.
    let rail = front - 7.0 / 16.0;
    assert!(is(seen(&pixels, Vec3::new(11.0, 4.0 + 7.5 / 16.0, rail)), WOOD), "the lower rail");
    assert!(!is(seen(&pixels, Vec3::new(11.0, 4.0 + 10.5 / 16.0, rail)), WOOD), "between the rails");
    // A shut door across its block's back; an open one against its side, the way through it clear.
    assert!(is(seen(&pixels, Vec3::new(19.6, 4.5, row as f32 + 0.19)), WOOD), "the shut door");
    assert!(!is(seen(&pixels, Vec3::new(21.6, 4.5, row as f32 + 0.19)), WOOD), "through the open door");
}

#[test]
fn a_door_half_way_through_its_swing_is_drawn_turned_half_way() {
    let Some(mut renderer) = renderer() else { return };
    let shut = BlockState::default();
    let parts = [(BlockShape::Door, shut, WOOD), (BlockShape::Door, BlockState { open: true, ..shut }, WOOD)];
    let file = file_of(&parts);
    let (shut_door, open_door) = (3, 4);
    let at = [16, 4, 32];
    let drawn = |renderer: &mut Renderer, door: BlockId, moving: Option<f64>| {
        let mut blocks = file.make_ground(None);
        blocks.set(at, door);
        let (mut world, entity) = scene(renderer, &file, blocks);
        if let Some(since) = moving {
            let mut world_blocks = world.get::<BlockWorld>(entity).unwrap().clone();
            world_blocks.clock = 10.0;
            world_blocks.moving = Arc::new(vec![MovingBlock { at, started: 10.0 - since, length: 0.3, opening: true }]);
            world.insert(entity, world_blocks);
        }
        render(renderer, &world, 3)
    };
    let shut_pixels = drawn(&mut renderer, shut_door, None);
    let half_pixels = drawn(&mut renderer, open_door, Some(0.15));
    let open_pixels = drawn(&mut renderer, open_door, None);
    keep_image("door shut.png", &shut_pixels);
    keep_image("door half open.png", &half_pixels);
    keep_image("door open.png", &open_pixels);
    let back = 32.0 + 0.1;
    // Shut, its far end is there across the back; half way, it has swung in off it; open, it is gone from there.
    let far_end = Vec3::new(16.88, 4.5, back);
    assert!(is(seen(&shut_pixels, far_end), WOOD), "shut");
    assert!(!is(seen(&half_pixels, far_end), WOOD), "half way its far end has left the back");
    // Half way it crosses the block's middle, which neither shut nor open does from in front.
    let middle = Vec3::new(16.5, 4.5, 32.5);
    assert!(is(seen(&half_pixels, middle), WOOD), "half way across the middle");
    assert!(!is(seen(&open_pixels, Vec3::new(16.6, 4.5, back)), WOOD), "open, nothing across the back");
}
