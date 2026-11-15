//! A world of blocks' sea in rough weather, drawn on a real GPU, offscreen.
//! Each test skips itself (with a note) on machines without a usable Vulkan
//! device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec2, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings,
    Sky, SurfaceLook,
    sea::{RoughSea, SeaLook, Whirlpool},
    weather::{Weather, WeatherKind, WeatherPlace, WeatherPlaces},
    weather_maps::{COVER_CELL, COVER_SIDE, FLOOR_CELL, FLOOR_REACH, FLOOR_SIDE, Grid, LYING_CELL, Lying, WeatherMaps},
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
                leafy: false,
                mixed: false,
                tint: 0,
            }
        })
        .collect();
    let default = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = BlockLook { material: default, surfaces: Arc::from(surfaces), voxels: Default::default(), motions: Arc::from([]) };
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
        world.insert_resource(RoughSea { look, everywhere: 0.85, heading: 0.6, time: 20.0, ..RoughSea::default() });
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
    let storm = WeatherPlace {
        middle: Vec2::new(middle - 55.0, middle - 45.0),
        radius: 60.0,
        rough: 1.0,
        ..WeatherPlace::default()
    };
    world.insert_resource(WeatherPlaces(vec![storm]));
    let pool = Whirlpool { middle: Vec2::new(middle + 2.0, middle - 44.0), radius: 16.0, depth: 7.0 };
    for (look, name) in LOOKS.into_iter().skip(1) {
        world.insert_resource(RoughSea { look, heading: 0.6, time: 20.0, whirlpools: vec![pool], ..RoughSea::default() });
        // Calm water at the island's shore, the storm's sea beyond it.
        let towards_storm = (Vec3::new(middle + 12.0, top + 14.0, middle + 14.0), Vec3::new(middle - 40.0, top, middle - 34.0));
        let pixels = seen(&mut renderer, &mut world, towards_storm, 20.0);
        keep_image(&format!("sea {name} storm edge.png"), &pixels);
        let over_pool = (Vec3::new(middle + 2.0, top + 13.0, middle - 16.0), Vec3::new(middle + 2.0, top - 3.0, middle - 44.0));
        let pixels = seen(&mut renderer, &mut world, over_pool, 20.0);
        keep_image(&format!("sea {name} whirlpool.png"), &pixels);
    }
}

/// How bright a pixel is, from 0 to 255.
fn bright(pixels: &[u8], x: u32, y: u32) -> i32 {
    let i = ((y * WIDTH + x) * 4) as usize;
    (i32::from(pixels[i]) + i32::from(pixels[i + 1]) + i32::from(pixels[i + 2])) / 3
}

#[test]
fn a_storm_far_off_is_a_dark_cloud_over_its_place_with_a_curtain_under_it_in_a_clear_sky() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    world.insert_resource(Sky { ambient: 0.35, clouds: voxear_render::Clouds { height: 500.0, thickness: 400.0, ..voxear_render::Clouds::blocks(0.25) }, ..Sky::default() });
    // From the island, looking west over the sea, the storm three kilometres off and a little to the right.
    let camera = (Vec3::new(middle, top + 6.0, middle), Vec3::new(middle - 400.0, top + 90.0, middle));
    let clear = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("storm far off clear.png", &clear);
    let storm = |kind: WeatherKind| WeatherPlace {
        middle: Vec2::new(middle - 3000.0, middle + 900.0),
        radius: 900.0,
        amounts: kind.amounts(),
        rough: 1.0,
        ..WeatherPlace::default()
    };
    world.insert_resource(WeatherPlaces(vec![storm(WeatherKind::Storm)]));
    let stormy = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("storm far off.png", &stormy);
    world.insert_resource(WeatherPlaces(vec![storm(WeatherKind::Sandstorm)]));
    let sand = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("sandstorm far off.png", &sand);
    // The sky towards the storm, under its cloud, is darker than it was; away from it, as it was.
    let over = |pixels: &[u8], x: u32| (0..60).map(|y| bright(pixels, x, HEIGHT / 2 + 40 + y)).sum::<i32>() / 60;
    let towards = WIDTH * 2 / 3;
    assert!(over(&stormy, towards) < over(&clear, towards) - 12, "{} and {}", over(&stormy, towards), over(&clear, towards));
    let away = WIDTH / 6;
    assert!((over(&stormy, away) - over(&clear, away)).abs() < 12, "{} and {}", over(&stormy, away), over(&clear, away));
    // Blown sand is dun where rain is grey.
    let dun = |pixels: &[u8]| {
        let i = (((HEIGHT / 2 + 110) * WIDTH + towards) * 4) as usize;
        i32::from(pixels[i]) - i32::from(pixels[i + 2])
    };
    assert!(dun(&sand) > dun(&stormy) + 10, "{} and {}", dun(&sand), dun(&stormy));
    // Standing under it, the weather is the viewer's own: the whole sky dark.
    let mut weather = Weather::default();
    weather.set_here(WeatherKind::Storm.amounts());
    world.insert_resource(weather);
    world.insert_resource(WeatherPlaces(vec![WeatherPlace { middle: Vec2::new(middle, middle), ..storm(WeatherKind::Storm) }]));
    let under = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("storm overhead.png", &under);
    assert!(over(&under, away) < over(&clear, away) - 20, "{} and {}", over(&under, away), over(&clear, away));
}

#[test]
fn the_starter_plants_of_the_seas_bed_are_each_their_own_shape_and_colour() {
    use voxear_render::{MeshRenderer, Primitive, StarterPlant};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = renderer.add_material(&MaterialDesc { base_color: [0.6, 0.5, 0.26, 1.0], roughness: 1.0, ..MaterialDesc::default() });
    world.spawn((MeshRenderer::new(plane, sand.unwrap()), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(200.0))));
    // In a row, the kelp behind them.
    let places = [
        (StarterPlant::Seagrass, Vec3::new(-5.2, 0.0, 0.0)),
        (StarterPlant::BranchCoral, Vec3::new(-3.5, 0.0, 0.0)),
        (StarterPlant::BrainCoral, Vec3::new(-1.7, 0.0, 0.0)),
        (StarterPlant::TableCoral, Vec3::new(0.3, 0.0, 0.0)),
        (StarterPlant::FanCoral, Vec3::new(2.4, 0.0, 0.0)),
        (StarterPlant::TubeSponge, Vec3::new(4.2, 0.0, 0.0)),
        (StarterPlant::Clam, Vec3::new(5.8, 0.0, 0.0)),
        (StarterPlant::Kelp, Vec3::new(8.0, 0.0, 2.0)),
    ];
    for (plant, at) in places {
        assert!(plant.under_water());
        let text = plant.model().to_text();
        let model = voxear_render::prepare_model("Plant.boxmodel", &mut |file| match file {
            "Plant.boxmodel" => Ok(text.clone().into_bytes()),
            other => anyhow::bail!("no {other}"),
        })
        .unwrap();
        let imported = renderer.upload_model(&model).unwrap();
        world.spawn((MeshRenderer::new(imported.nodes[0].mesh, imported.nodes[0].material), Transform::from_position(at)));
    }
    let towards_sun = Vec3::new(-0.4, 0.8, -0.5).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.5, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let row = seen(&mut renderer, &mut world, (Vec3::new(0.3, 1.5, -7.2), Vec3::new(0.3, 0.6, 0.0)), 1.0);
    keep_image("sea plants in a row.png", &row);
    let kelp = seen(&mut renderer, &mut world, (Vec3::new(8.0, 4.2, -9.5), Vec3::new(8.0, 4.0, 2.0)), 1.0);
    keep_image("sea plants kelp.png", &kelp);
    // Each stands out from the sand in a colour of its own: pink, blue and purple among them.
    let hues = |pixels: &[u8]| {
        let (mut pink, mut blue, mut green) = (0, 0, 0);
        for pixel in pixels.chunks_exact(4) {
            let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(i32::from);
            pink += usize::from(r > g + 50 && r > b + 30);
            blue += usize::from(b > r + 40 && b > g + 15);
            green += usize::from(g > r + 25 && g > b + 15);
        }
        (pink, blue, green)
    };
    let (pink, blue, green) = hues(&row);
    assert!(pink > 800 && blue > 800 && green > 300, "{pink} pink, {blue} blue, {green} green");
    assert!(hues(&kelp).2 + hues(&kelp).0 > 0 || kelp.chunks_exact(4).any(|p| p[1] > p[2] + 20), "kelp in view");
}

#[test]
fn a_rough_sea_costs_little_more_to_draw_than_a_flat_one_and_a_warm_one_glows_at_night() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    // Looking out over the roughest sea from low down, where every pixel of water is walked furthest.
    let camera = (Vec3::new(middle - 7.5, top + 3.0, middle - 7.5), Vec3::new(middle - 40.0, top - 5.0, middle - 46.0));
    seen(&mut renderer, &mut world, camera, 20.0);
    // Every pass's time added up, the least of sixty frames, since the GPU is shared.
    let mut spent = |renderer: &mut Renderer, world: &World| {
        let frame = |renderer: &mut Renderer, at: u32| {
            renderer.render(world, 21.0 + at as f32 / 60.0).expect("frame renders");
            renderer.timings().iter().map(|(_, ms)| *ms).sum::<f32>()
        };
        (0..60).map(|at| frame(renderer, at)).fold(f32::MAX, f32::min)
    };
    let mut times = Vec::new();
    for (look, name) in LOOKS {
        world.insert_resource(RoughSea { look, everywhere: 1.0, heading: 0.6, time: 20.0, ..RoughSea::default() });
        times.push((name, spent(&mut renderer, &world)));
    }
    eprintln!("a frame of the roughest sea, at {WIDTH}x{HEIGHT}: {times:?} ms");
    let flat = times[0].1;
    for (name, ms) in &times[1..] {
        assert!(*ms < flat + 2.0, "{name}: {ms} ms against a flat sea's {flat}");
    }
    // At night, glowing: the water that breaks, and specks over the rest.
    let night = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, 0.2, 0.5).normalize());
    let suns: Vec<_> = world.query_ref::<(voxear_ecs::Entity, &Light)>().into_iter().map(|(e, _)| e).collect();
    for sun in suns {
        *world.get_mut::<Transform>(sun).unwrap() = Transform::new(Vec3::ZERO, night, Vec3::ONE);
    }
    world.insert_resource(Sky { ambient: 0.02, ..Sky::default() });
    let glowing = |glow: f32| RoughSea { everywhere: 0.8, heading: 0.6, time: 20.0, glow, ..RoughSea::default() };
    world.insert_resource(glowing(0.0));
    let dark = seen(&mut renderer, &mut world, camera, 20.0);
    world.insert_resource(glowing(1.0));
    let lit = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("sea glowing at night.png", &lit);
    let blue = |pixels: &[u8]| pixels.chunks_exact(4).filter(|p| p[2] > 90 && p[2] > p[0] + 40).count();
    assert!(blue(&lit) > blue(&dark) + 300, "{} glowing pixels against {}", blue(&lit), blue(&dark));
}

/// The mean colour of the rows `rows` of a picture, across its middle half.
fn mean(pixels: &[u8], rows: std::ops::Range<u32>) -> [i32; 3] {
    let mut sum = [0i64; 3];
    let mut count = 0i64;
    for y in rows {
        for x in WIDTH / 4..WIDTH * 3 / 4 {
            let i = ((y * WIDTH + x) * 4) as usize;
            for (c, total) in sum.iter_mut().enumerate() {
                *total += i64::from(pixels[i + c]);
            }
            count += 1;
        }
    }
    sum.map(|total| (total / count.max(1)) as i32)
}

#[test]
fn under_a_seas_faces_the_camera_is_under_water_only_under_its_swell() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    let centre = Vec2::new(middle + 2.0, middle - 44.0);
    let pool = Whirlpool { middle: centre, radius: 16.0, depth: 7.0 };
    let whirling = RoughSea { look: SeaLook::SmallCubes, time: 20.0, whirlpools: vec![pool], ..RoughSea::default() };
    // The sky over the sea, from just over its faces, looking along them: what the top of each view is to show.
    let sky_from = |at: Vec2| (Vec3::new(at.x, top + 1.5, at.y), Vec3::new(centre.x, top + 1.5, centre.y + 0.01));
    world.insert_resource(RoughSea { time: 20.0, ..RoughSea::default() });
    let rim = centre + Vec2::new(0.0, 6.0);
    let sky = mean(&seen(&mut renderer, &mut world, sky_from(rim), 20.0), 0..HEIGHT / 6);
    let near = |a: [i32; 3], b: [i32; 3], by: i32| a.iter().zip(b).all(|(a, b)| (a - b).abs() <= by);

    // In the whirlpool's funnel, a metre under the faces and over its water: the sky over the rim, the water below.
    world.insert_resource(whirling.clone());
    let sunk = whirling.sampler(&[], [0.0; 2]).at(rim.x, rim.y).sunk;
    assert!(sunk > 1.8, "the funnel is {sunk} m deep there");
    let eye = Vec3::new(rim.x, top - 1.0, rim.y);
    let across = seen(&mut renderer, &mut world, (eye, Vec3::new(centre.x, top - 1.0, centre.y + 0.01)), 20.0);
    keep_image("sea in a whirlpool's funnel.png", &across);
    let over = mean(&across, 0..HEIGHT / 6);
    assert!(near(over, sky, 14), "the sky from the funnel {over:?}, from over the sea {sky:?}");
    let down = seen(&mut renderer, &mut world, (eye, Vec3::new(centre.x, top - 7.0, centre.y)), 20.0);
    keep_image("sea down a whirlpool's funnel.png", &down);
    let [r, _, b] = mean(&down, HEIGHT * 2 / 5..HEIGHT * 3 / 5);
    assert!(b > r + 10, "water down in the funnel, not its sand floor: red {r}, blue {b}");

    // Under its water, deeper down the funnel: under water still, the sky nowhere to be seen as it is.
    let deep = Vec3::new(centre.x, top - 6.5, centre.y + 2.0);
    let sunk = whirling.sampler(&[], [0.0; 2]).at(deep.x, deep.z).sunk;
    assert!(top - sunk > deep.y + 0.5, "{} m of water over the camera", top - sunk - deep.y);
    let under = seen(&mut renderer, &mut world, (deep, Vec3::new(centre.x, top - 6.5, centre.y + 20.0)), 20.0);
    keep_image("sea under a whirlpool's water.png", &under);
    let up = mean(&under, 0..HEIGHT / 6);
    assert!(!near(up, sky, 30), "from under the water {up:?}, the sky {sky:?}");

    // In a trough of a rough sea, under the faces and over the water.
    let rough =
        RoughSea { look: SeaLook::SmallCubes, everywhere: 1.0, heading: 0.6, time: 20.0, ..RoughSea::default() };
    let swell = rough.sampler(&[], [0.0; 2]);
    let (trough, sunk) = (0..400)
        .map(|i| Vec2::new(26.0 + (i % 20) as f32, 104.0 + (i / 20) as f32 * 1.5))
        .map(|at| (at, swell.at(at.x, at.y).sunk))
        .fold((Vec2::ZERO, 0.0), |deepest, (at, sunk)| if sunk > deepest.1 { (at, sunk) } else { deepest });
    assert!(sunk > 2.0, "the deepest trough found is {sunk} m");
    world.insert_resource(RoughSea { time: 20.0, ..RoughSea::default() });
    let ahead = Vec2::new(trough.x - 30.0, trough.y + 10.0);
    let level = |y: f32| (Vec3::new(trough.x, y, trough.y), Vec3::new(ahead.x, y + 3.0, ahead.y));
    let calm = mean(&seen(&mut renderer, &mut world, level(top + 1.5), 20.0), 0..HEIGHT / 6);
    world.insert_resource(rough);
    let in_trough = seen(&mut renderer, &mut world, level(top - 0.7), 20.0);
    keep_image("sea in a trough.png", &in_trough);
    let over = mean(&in_trough, 0..HEIGHT / 6);
    assert!(near(over, calm, 14), "the sky from the trough {over:?}, from over a calm sea {calm:?}");
}

/// How high the island's ground stands at `x`, `z`, as [`sea`] makes it, in blocks: over the water within nine
/// blocks of its middle, shelving away to the floor.
fn island_ground(x: f32, z: f32) -> f32 {
    let middle = SIDE as f32 * 0.5;
    let far = ((x.floor() - middle).powi(2) + (z.floor() - middle).powi(2)).sqrt();
    (TOP as f32 + 2.0 - (far - 7.0).max(0.0) * 0.55).round().max(1.0)
}

/// A grid of `side` cells `cell` wide from `corner`, each as `value` says at its middle.
fn grid<T: Copy>(corner: Vec2, cell: f32, side: usize, value: impl Fn(f32, f32) -> T) -> Grid<T> {
    let cells: Vec<T> = (0..side * side)
        .map(|n| value(corner.x + ((n % side) as f32 + 0.5) * cell, corner.y + ((n / side) as f32 + 0.5) * cell))
        .collect();
    Grid { corner, cell, side, cells: cells.into() }
}

/// How many pixels in `rows` are white water: bright, and nearly grey.
fn white_pixels(pixels: &[u8], rows: std::ops::Range<u32>) -> usize {
    rows.flat_map(|y| (0..WIDTH).map(move |x| ((y * WIDTH + x) * 4) as usize))
        .filter(|&i| {
            let [r, g, b] = [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from);
            r.min(g).min(b) > 150 && r.max(g).max(b) - r.min(g).min(b) < 45
        })
        .count()
}

#[test]
fn a_rough_sea_calms_in_the_shallows_round_an_island_and_breaks_on_its_shore() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    // A long gentle beach along the sea's east side, rising a block in seven out of the floor to over the water.
    let beach = |x: f32| (1.0 + (x - 100.0) / 7.0).floor().clamp(1.0, top + 1.0);
    let blocks = world.query_ref::<(voxear_ecs::Entity, &BlockWorld)>().into_iter().next().unwrap().0;
    {
        let world_of = world.get_mut::<BlockWorld>(blocks).unwrap();
        let mut map = (*world_of.blocks).clone();
        // The sea's own sand and water, from a column out at sea.
        let (sand, water) = (map.get([10, 0, 10]), map.get([10, TOP - 1, 10]));
        for x in 100..SIDE {
            for z in 0..SIDE {
                let ground = beach(x as f32) as i32;
                for y in 0..TOP + 2 {
                    let block = if y < ground { sand } else if y < TOP { water } else { 0 };
                    map.set([x, y, z], block);
                }
            }
        }
        let faces: Vec<_> = world_of.faces.to_vec();
        *world_of = BlockWorld::new(map, &faces, 1.0);
    }
    let ground = |x: f32, z: f32| if x >= 100.0 { beach(x) } else { island_ground(x, z) };
    let floor = grid(Vec2::new(middle - 128.0, middle - 128.0), FLOOR_CELL, FLOOR_SIDE, |x, z| (top - ground(x, z)).clamp(0.0, FLOOR_REACH));
    world.insert_resource(RoughSea { look: SeaLook::SmallCubes, everywhere: 1.0, heading: 0.6, time: 20.0, ..RoughSea::default() });
    // The beach from out at sea, the swell running in to it.
    let camera = (Vec3::new(88.0, top + 9.0, middle - 20.0), Vec3::new(150.0, top - 4.0, middle + 6.0));
    let open = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("sea shore before.png", &open);
    world.insert_resource(WeatherMaps { floor: Some(floor), ..WeatherMaps::default() });
    let shore = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("sea shore breakers.png", &shore);
    let later = seen(&mut renderer, &mut world, camera, 22.0);
    keep_image("sea shore breakers later.png", &later);
    let close = (Vec3::new(118.0, top + 12.0, middle - 22.0), Vec3::new(146.0, top, middle + 4.0));
    for (n, seconds) in [20.0, 21.5, 23.0].into_iter().enumerate() {
        keep_image(&format!("sea shore breakers close {n}.png"), &seen(&mut renderer, &mut world, close, seconds));
    }
    let above = (Vec3::new(128.0, top + 45.0, middle - 0.5), Vec3::new(128.5, top, middle));
    keep_image("sea shore breakers above.png", &seen(&mut renderer, &mut world, above, 20.0));
    // White water along the shore where it breaks, more than the open swell's crests give there.
    let rows = HEIGHT / 3..HEIGHT;
    let (before, after) = (white_pixels(&open, rows.clone()), white_pixels(&shore, rows.clone()));
    assert!(after > before + 2000, "white water by the shore: {after} pixels, against {before}");
    // And the breakers move: two seconds on, they are somewhere else.
    let moved = shore.chunks(4).zip(later.chunks(4)).filter(|(a, b)| (i32::from(a[0]) - i32::from(b[0])).abs() > 40).count();
    assert!(moved > 2000, "{moved} pixels changed");
}

#[test]
fn a_frozen_sea_is_ice_still_in_a_storm_with_floes_where_it_is_part_frozen() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    world.insert_resource(RoughSea { look: SeaLook::SmallCubes, everywhere: 1.0, heading: 0.6, time: 20.0, ..RoughSea::default() });
    // Frozen hard west of the island, part frozen across its middle, open water east of it.
    let lying = grid(Vec2::new(-16.0, -16.0), LYING_CELL, 7, |x, _| Lying {
        ice: if x < middle - 16.0 { 1.0 } else if x < middle + 16.0 { 0.5 } else { 0.0 },
        celsius: -6.0,
        height: top,
        ..Lying::default()
    });
    world.insert_resource(WeatherMaps { lying: Some(lying), ..WeatherMaps::default() });
    let camera = (Vec3::new(middle, top + 40.0, middle + 45.0), Vec3::new(middle, top, middle - 10.0));
    let pixels = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("sea frozen.png", &pixels);
    // West, on the right as the camera looks north, pale and grey as ice; east, on the left, the sea's own blue.
    let colour = |x0: u32, x1: u32| {
        let mut sum = [0i64; 3];
        let rows = HEIGHT / 4..HEIGHT / 2;
        let count = (rows.len() as u32 * (x1 - x0)) as i64;
        for y in rows {
            for x in x0..x1 {
                let i = ((y * WIDTH + x) * 4) as usize;
                for c in 0..3 {
                    sum[c] += i64::from(pixels[i + c]);
                }
            }
        }
        sum.map(|s| (s / count) as i32)
    };
    let (ice, sea) = (colour(WIDTH * 4 / 5, WIDTH - 20), colour(20, WIDTH / 5));
    let brightness = |c: [i32; 3]| c.iter().sum::<i32>() / 3;
    assert!(brightness(ice) > brightness(sea) + 40, "ice {ice:?}, sea {sea:?}");
    assert!(ice[2] - ice[0] < sea[2] - sea[0], "the ice less blue than the sea: {ice:?}, {sea:?}");
}

#[test]
fn under_a_roof_the_ground_stays_dry_and_rain_kept_out_from_under_roofs_falls_on_it() {
    use voxear_render::{ParticlePlayback, ParticleSpace, ParticleSystem, Starter};
    let Some(mut renderer) = renderer() else { return };
    let mut world = sea(&mut renderer);
    let (middle, top) = (SIDE as f32 * 0.5, TOP as f32);
    // A roof of stone twelve blocks a side, four blocks over the island's grass.
    let roof = top + 6.0;
    let blocks = world.query_ref::<(voxear_ecs::Entity, &BlockWorld)>().into_iter().next().unwrap().0;
    {
        let world_of = world.get_mut::<BlockWorld>(blocks).unwrap();
        let mut map = (*world_of.blocks).clone();
        for x in 74..86 {
            for z in 74..86 {
                map.set([x, roof as i32, z], 3);
            }
        }
        let faces: Vec<_> = world_of.faces.to_vec();
        *world_of = BlockWorld::new(map, &faces, 1.0);
    }
    let under = |x: f32, z: f32| (74.0..86.0).contains(&x) && (74.0..86.0).contains(&z);
    let cover = grid(Vec2::new(middle - 24.0, middle - 24.0), COVER_CELL, COVER_SIDE, |x, z| {
        if under(x, z) { roof + 1.0 } else { island_ground(x, z).max(top) }
    });
    let lying = grid(Vec2::new(-16.0, -16.0), LYING_CELL, 7, |_, _| Lying { wet: 1.0, celsius: 15.0, height: top, ..Lying::default() });
    // From under the roof's edge, looking in under it across the grass.
    let camera = (Vec3::new(middle, top + 3.5, middle - 10.0), Vec3::new(middle, top + 2.0, middle + 6.0));
    let plain = seen(&mut renderer, &mut world, camera, 20.0);
    world.insert_resource(WeatherMaps { lying: Some(lying.clone()), ..WeatherMaps::default() });
    let wet = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("roof wet everywhere.png", &wet);
    world.insert_resource(WeatherMaps { lying: Some(lying.clone()), cover: Some(cover.clone()), ..WeatherMaps::default() });
    let sheltered = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("roof dry under it.png", &sheltered);
    // The grass under the roof, low in the middle of the view, is as it is in dry weather when the roof keeps the
    // rain off it, and darker and glossier when nothing does.
    let changed = |a: &[u8], b: &[u8]| {
        let mut count = 0;
        for y in HEIGHT / 2 + 10..HEIGHT * 7 / 10 {
            for x in WIDTH * 2 / 5..WIDTH * 3 / 5 {
                let i = ((y * WIDTH + x) * 4) as usize;
                count += usize::from((0..3).any(|c| (i32::from(a[i + c]) - i32::from(b[i + c])).abs() > 6));
            }
        }
        count
    };
    let (soaked, kept_dry) = (changed(&wet, &plain), changed(&sheltered, &plain));
    assert!(soaked > 3000, "wet without the roof's cover: {soaked} pixels changed");
    assert!(kept_dry * 20 < soaked, "and dry with it: {kept_dry} pixels changed against {soaked}");

    // Rain falling over the roof: through it unless it is kept out from under roofs.
    let base = Starter::Rain.effect().emitters[0].system;
    let rain = |sheltered: bool| ParticleSystem {
        box_size: [12.0, base.box_size[1], 12.0],
        rate: base.rate * 4.0,
        space: ParticleSpace::World,
        sheltered,
        ..base
    };
    let over = Transform::from_position(Vec3::new(middle, roof + 8.0, middle));
    let dry = seen(&mut renderer, &mut world, camera, 20.0);
    let falling = world.spawn((rain(false), ParticlePlayback { started: 0.0, stopped: None }, over));
    let through = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("roof rain through.png", &through);
    *world.get_mut::<ParticleSystem>(falling).unwrap() = rain(true);
    let kept_out = seen(&mut renderer, &mut world, camera, 20.0);
    keep_image("roof rain kept out.png", &kept_out);
    let differs = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).filter(|(a, b)| (0..3).any(|c| (i32::from(a[c]) - i32::from(b[c])).abs() > 12)).count();
    let (seen_through, seen_kept) = (differs(&through, &dry), differs(&kept_out, &dry));
    assert!(seen_through > 500, "rain falls through the roof without it: {seen_through} pixels");
    assert!(seen_kept * 10 < seen_through, "and none under it with it: {seen_kept} pixels against {seen_through}");
}
