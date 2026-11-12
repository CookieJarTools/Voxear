//! A world of blocks' sea in rough weather, drawn on a real GPU, offscreen.
//! Each test skips itself (with a note) on machines without a usable Vulkan
//! device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings,
    Sky, SurfaceLook,
    sea::{RoughPlace, RoughSea, SeaLook, Whirlpool},
};
use voxear_scene::Transform;
use voxear_terrain::blocks::{
    BlockWorld,
    chunk::BlockId,
    file::{BlockFile, BlockKind},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
/// How many blocks a side the sea is, and how high its top stands.
const SIDE: i32 = 160;
const TOP: i32 = 8;

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

/// A sea [`SIDE`] blocks a side over a floor of sand, with a round island of
/// sand and grass in its middle and a post of stone standing out of the
/// water, and the look it is drawn with, the sun in the west.
fn sea(renderer: &mut Renderer) -> World {
    let mut file = BlockFile::default();
    file.ground.size = [SIDE as u32, SIDE as u32];
    file.ground.height = TOP as u32;
    file.blocks.push(BlockKind::colored("Sand", [0.86, 0.78, 0.55]));
    let sand = file.blocks.len() as BlockId;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.03, 0.3, 0.42]) });
    let water = file.blocks.len() as BlockId;
    let mut blocks = file.make_ground(None);
    let middle = SIDE as f32 * 0.5;
    for z in 0..SIDE {
        for x in 0..SIDE {
            let far = ((x as f32 - middle).powi(2) + (z as f32 - middle).powi(2)).sqrt();
            // The island's ground: over the water within 9 blocks, shelving away to the floor by 22.
            let ground = (TOP as f32 + 2.0 - (far - 7.0).max(0.0) * 0.55).round().max(1.0) as i32;
            let post = (x, z) == (58, 66) || (x, z) == (100, 60);
            for y in 0..TOP + 3 {
                let block = match () {
                    () if post && y < TOP + 2 => 3,
                    () if y < ground.min(TOP - 1) => sand,
                    () if y < ground => {
                        if y == ground - 1 && ground > TOP { 1 } else { sand }
                    }
                    () if y < TOP => water,
                    () => 0,
                };
                blocks.set([x, y, z], block);
            }
        }
    }
    let (looks, faces) = file.faces();
    let surfaces: Vec<SurfaceLook> = looks
        .iter()
        .map(|look| {
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
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default() };
    let mut world = World::new();
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    let towards_sun = Vec3::new(-50f32.to_radians().cos(), 50f32.to_radians().sin(), 0.35).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.35, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

/// The sea drawn from `eye` looking at `target`, `seconds` into its swell.
fn seen(renderer: &mut Renderer, world: &mut World, (eye, target): (Vec3, Vec3), seconds: f32) -> Vec<u8> {
    let cameras: Vec<_> = world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().map(|(e, _)| e).collect();
    for camera in cameras {
        world.despawn(camera);
    }
    let towards = (target - eye).normalize();
    let rotation = Quat::from_rotation_y(towards.x.atan2(towards.z)) * Quat::from_rotation_x(-towards.y.asin());
    world.spawn((Camera::default(), Transform::new(eye, rotation, Vec3::ONE)));
    for frame in 0..6 {
        renderer.render(world, seconds + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

const LOOKS: [(SeaLook, &str); 4] =
    [(SeaLook::Flat, "flat"), (SeaLook::Steps, "steps"), (SeaLook::SmallCubes, "small cubes"), (SeaLook::Smooth, "smooth")];

#[test]
fn a_rough_sea_is_drawn_as_steps_as_small_cubes_or_smooth() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    // Out to sea from just over the water, and the island from a mast's height.
    let views = [
        ("from the water", (Vec3::new(middle - 7.5, top + 3.0, middle - 7.5), Vec3::new(middle - 40.0, top - 5.0, middle - 46.0))),
        ("from above", (Vec3::new(middle + 34.0, top + 20.0, middle + 38.0), Vec3::new(middle - 4.0, top, middle - 6.0))),
    ];
    for (look, name) in LOOKS {
        world.insert_resource(RoughSea { look, everywhere: 0.85, heading: 0.6, height: 3.0, ..RoughSea::default() });
        for (view, camera) in views {
            let pixels = seen(&mut renderer, &mut world, camera, 20.0);
            keep_image(&format!("sea {name} {view}.png"), &pixels);
        }
    }
}

#[test]
fn a_storm_roughens_the_sea_under_it_and_a_whirlpool_draws_it_down() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    let storm = RoughPlace { middle: Vec2::new(middle - 55.0, middle - 45.0), radius: 60.0, strength: 1.0 };
    let pool = Whirlpool { middle: Vec2::new(middle + 2.0, middle - 44.0), radius: 16.0, depth: 7.0 };
    for (look, name) in LOOKS.into_iter().skip(1) {
        let rough = RoughSea { look, heading: 0.6, height: 3.0, storms: vec![storm], whirlpools: vec![pool], ..RoughSea::default() };
        world.insert_resource(rough);
        // Calm water at the island's shore, the storm's sea beyond it.
        let towards_storm = (Vec3::new(middle + 12.0, top + 14.0, middle + 14.0), Vec3::new(middle - 40.0, top, middle - 34.0));
        let pixels = seen(&mut renderer, &mut world, towards_storm, 20.0);
        keep_image(&format!("sea {name} storm edge.png"), &pixels);
        let over_pool = (Vec3::new(middle + 2.0, top + 13.0, middle - 16.0), Vec3::new(middle + 2.0, top - 3.0, middle - 44.0));
        let pixels = seen(&mut renderer, &mut world, over_pool, 20.0);
        keep_image(&format!("sea {name} whirlpool.png"), &pixels);
    }
}
