//! Terrain drawn on a real GPU, offscreen. Each test skips itself (with a
//! note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, Light, MaterialDesc, PostProcess, Renderer, RendererSettings, Sky,
    TerrainLook, ViewOptions,
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

/// Rolling hills 1 km across, red, seen from low over them, so the ground
/// far off is drawn at coarse levels and near at the finest.
fn hills(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    let heights = Heightmap::from_fn(4, 4, 1.0, |x, z| {
        (x * 0.031).sin() * 14.0 + (z * 0.023).cos() * 11.0 + (x * 0.11 + z * 0.07).sin() * 2.5
    });
    let red = MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() };
    let material = renderer.add_material(&red).unwrap();
    let terrain = Terrain { detail_distance: 40.0, ..Terrain::new(heights) };
    world.spawn((
        terrain,
        TerrainLook::new(material, 4.0),
        Transform::from_position(Vec3::new(-512.0, 0.0, -100.0)),
    ));
    let look = Quat::from_rotation_x(0.12);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 32.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

#[test]
fn hills_are_drawn_whole_with_no_cracks_between_their_levels() {
    let Some(mut renderer) = renderer() else { return };
    let world = hills(&mut renderer);
    // Unlit, the ground is its own red and the sky anything else: a crack would let the sky through.
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain hills unlit.png", &pixels);
    let mut sky = Vec::new();
    for y in HEIGHT * 2 / 3..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b] = rgb_at(&pixels, x, y);
            if r < 150 || g > 60 || b > 60 {
                sky.push((x, y));
            }
        }
    }
    assert!(
        sky.is_empty(),
        "{} pixels of the bottom third are not ground, first {:?}",
        sky.len(),
        &sky[..sky.len().min(8)]
    );

    // Its levels of detail, each in its own colour: more than one, near and far.
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_LOD, ..ViewOptions::default() });
    let levels = render(&mut renderer, &world, 2);
    keep_image("terrain hills levels.png", &levels);
    let near = rgb_at(&levels, WIDTH / 2, HEIGHT - 4);
    let far = rgb_at(&levels, WIDTH / 2, HEIGHT / 2 + 12);
    assert_ne!(near, far, "the ground near and far is drawn at different levels");
}

#[test]
fn a_ridge_shades_the_ground_on_its_far_side_from_the_sun() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Flat ground 128 m across with a ridge 20 m high along z at x = 60 to 64.
    let heights = Heightmap::from_fn(1, 1, 0.5, |x, _| if (60.0..=64.0).contains(&x) { 20.0 } else { 0.0 });
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((Terrain::new(heights), TerrainLook::new(grey, 4.0), Transform::default()));
    // Straight down onto its middle, within the sun's shadow distance, +z up the screen and +x to the right.
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(64.0, 60.0, 64.0), down, Vec3::ONE)));
    // The sun low in the west (-x), shining east.
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain ridge shadow.png", &pixels);
    // Where on screen a point of the ground is, from the camera 60 m above (64, 64).
    let half = (30f32).to_radians().tan() * 60.0;
    let pixel = |x: f32, z: f32| {
        let u = (x - 64.0) / (half * WIDTH as f32 / HEIGHT as f32);
        let v = (z - 64.0) / half;
        (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
    };
    let brightness = |(x, z): (f32, f32)| {
        let (px, py) = pixel(x, z);
        rgb_at(&pixels, px, py).iter().sum::<i32>()
    };
    let (lit, shaded) = (brightness((30.0, 64.0)), brightness((80.0, 64.0)));
    assert!(shaded * 2 < lit, "the ground east of the ridge is in its shadow: {shaded} against {lit} lit");
}

#[test]
fn a_tile_changed_after_it_was_drawn_is_drawn_changed() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let red = MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() };
    let material = renderer.add_material(&red).unwrap();
    let flat = Heightmap::flat(2, 2, 0.5, 0.0);
    let field = world.spawn((
        Terrain::new(flat),
        TerrainLook::new(material, 4.0),
        Transform::from_position(Vec3::new(-128.0, 0.0, -128.0)),
    ));
    // Looking along the ground from just above it: flat, the sky fills the top half.
    let look = Quat::from_rotation_x(0.05);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 2.0, -100.0), look, Vec3::ONE)));
    world.insert_resource(PostProcess {
        anti_aliasing: AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() },
        ..PostProcess::neutral()
    });
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let ground = |pixels: &[u8]| {
        (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = rgb_at(pixels, x, y);
                r > 150 && g < 60 && b < 60
            })
            .count()
    };
    let before = ground(&render(&mut renderer, &world, 3));
    // A wall of ground 40 m high across the far tiles.
    let heights = world.get::<Terrain>(field).unwrap().heights.clone();
    let wall = voxear_terrain::HeightTile::flat(40.0);
    let raised = heights.with_tiles([(0, 1, wall.clone()), (1, 1, wall)]);
    world.get_mut::<Terrain>(field).unwrap().heights = std::sync::Arc::new(raised);
    let pixels = render(&mut renderer, &world, 2);
    keep_image("terrain changed tile.png", &pixels);
    let after = ground(&pixels);
    assert!(after > before + WIDTH as usize * 20, "the raised tiles fill more of the view: {before} then {after}");
}

/// Where on screen a point of level ground is, seen straight down from
/// `height` metres above `(64, 64)`: +x to the right, +z up the screen.
fn from_above(height: f32, x: f32, z: f32) -> (u32, u32) {
    let half = (30f32).to_radians().tan() * height;
    let u = (x - 64.0) / (half * WIDTH as f32 / HEIGHT as f32);
    let v = (z - 64.0) / half;
    (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
}

/// A camera straight down from `height` metres over `(64, 64)`, and no
/// anti-aliasing to blur what is read back.
fn look_down(world: &mut World, height: f32) {
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(64.0, height, 64.0), down, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
}

#[test]
fn thirty_two_surfaces_are_drawn_each_where_its_shares_put_it_and_blended_between() {
    use std::sync::Arc;
    use voxear_render::SurfaceLook;
    use voxear_terrain::surfaces::{WeightTile, Weights};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // 32 colours, each its own.
    let levels = [0.2, 0.45, 0.7, 0.95];
    let colour = |i: usize| [levels[i & 3], levels[(i >> 2) & 3], if i & 16 == 0 { 0.2 } else { 0.9 }];
    let mut surfaces = Vec::new();
    for i in 0..32 {
        let [r, g, b] = colour(i);
        let material = renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
        let material = material.unwrap();
        surfaces.push(SurfaceLook {
            material,
            size: 4.0,
            sharpness: 0.0,
            triplanar: false,
            tracks: 0.0,
            fills_in: 0.0,
            liquid: voxear_terrain::surfaces::Liquid::None,
            glitter: false,
            leafy: false,
        });
    }
    // 128 m across in stripes 4 m wide along x, each all of one surface.
    let side = voxear_terrain::TILE_SAMPLES;
    let samples = (0..side * side).map(|i| [((i % side) / 8).min(31), 255]).collect();
    let weights = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::from_samples(samples))]);
    let terrain = Terrain { weights: Some(Arc::new(weights)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    let plain = renderer.add_material(&MaterialDesc::default()).unwrap();
    let look = TerrainLook { surfaces: surfaces.into(), ..TerrainLook::new(plain, 4.0) };
    world.spawn((terrain, look, Transform::default()));
    look_down(&mut world, 70.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain surfaces.png", &pixels);
    let seen: Vec<[i32; 3]> = (0..32)
        .map(|stripe| {
            let (x, y) = from_above(70.0, stripe as f32 * 4.0 + 2.0, 64.0);
            rgb_at(&pixels, x, y)
        })
        .collect();
    // Each stripe nearer its own colour than any other's, as the screen shows them.
    let distance = |a: [i32; 3], b: [i32; 3]| (0..3).map(|c| (a[c] - b[c]).pow(2)).sum::<i32>();
    // Each stripe its own colour, brighter in each channel as its material is.
    let distinct: std::collections::BTreeSet<[i32; 3]> = seen.iter().copied().collect();
    assert_eq!(distinct.len(), 32, "32 colours: {seen:?}");
    for (stripe, &shown) in seen.iter().enumerate() {
        for other in 0..32 {
            for channel in 0..3 {
                if colour(other)[channel] > colour(stripe)[channel] {
                    assert!(seen[other][channel] > shown[channel], "stripe {other} over {stripe}: {seen:?}");
                }
            }
        }
    }
    // Between stripes 10 and 11, the half a metre between their samples blends from one to the other.
    let (a, b) = (seen[10], seen[11]);
    let between = (0..24).any(|step| {
        let (x, y) = from_above(70.0, 43.0 + step as f32 * 0.08, 64.0);
        let c = rgb_at(&pixels, x, y);
        let span = distance(a, b) as f32;
        distance(c, a) as f32 > span * 0.1 && distance(c, b) as f32 > span * 0.1
    });
    assert!(between, "a blend between {a:?} and {b:?}");
}

#[test]
fn a_hole_shows_the_ground_below_it_and_lets_the_sun_through() {
    use std::sync::Arc;
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Grey ground with a hole 12 m across in its middle, over green ground 5 m below.
    let side = voxear_terrain::TILE_SAMPLES;
    let mut bits = vec![0u32; voxear_terrain::heights::HOLE_WORDS];
    for z in 0..side {
        for x in 0..side {
            let (dx, dz) = (x as f32 * 0.5 - 64.0, z as f32 * 0.5 - 64.0);
            if dx * dx + dz * dz <= 36.0 {
                let index = (z * side + x) as usize;
                bits[index / 32] |= 1 << (index % 32);
            }
        }
    }
    let holed = voxear_terrain::HeightTile::flat(0.0).with_holes(Some(bits.into()));
    let ground = Heightmap::from_tiles(1, 1, 0.5, vec![Arc::new(holed)]);
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    let green = MaterialDesc { base_color: [0.1, 0.9, 0.1, 1.0], ..MaterialDesc::default() };
    let green = renderer.add_material(&green).unwrap();
    world.spawn((Terrain::new(ground), TerrainLook::new(grey, 4.0), Transform::default()));
    let below = Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0));
    world.spawn((below, TerrainLook::new(green, 4.0), Transform::from_position(Vec3::new(0.0, -5.0, 0.0))));
    look_down(&mut world, 40.0);
    // The sun high overhead.
    let towards_sun = Vec3::new(0.15, 1.0, 0.1).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.05, ..Sky::default() });
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let unlit = render(&mut renderer, &world, 3);
    keep_image("terrain hole unlit.png", &unlit);
    let (cx, cy) = from_above(40.0, 64.0, 64.0);
    let [r, g, b] = rgb_at(&unlit, cx, cy);
    assert!(g > r + 60 && g > b + 60, "green through the hole: {:?}", [r, g, b]);
    let (ex, ey) = from_above(40.0, 84.0, 64.0);
    let [r, g, b] = rgb_at(&unlit, ex, ey);
    assert!((r - g).abs() < 20 && (g - b).abs() < 20, "grey beside it: {:?}", [r, g, b]);

    // Lit, the sun reaches the green through the hole: the ground over it casts no shadow there.
    renderer.set_view_options(ViewOptions::default());
    let lit = render(&mut renderer, &world, 3);
    keep_image("terrain hole lit.png", &lit);
    let through = rgb_at(&lit, cx, cy)[1];
    let beside = rgb_at(&lit, ex, ey).iter().sum::<i32>() / 3;
    assert!(through * 2 > beside, "the green under the hole is sunlit: {through} against {beside} beside it");
}

#[test]
fn grass_grows_thickly_where_its_densities_say_more_blades_than_the_scene_has_room_for() {
    use std::sync::Arc;
    use voxear_render::{GrassLook, Primitive};
    use voxear_terrain::grass::{GrassMap, GrassTile};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Thick on the left half (x under 64 m), none on the right.
    let side = voxear_terrain::TILE_SAMPLES;
    let samples = (0..side * side).map(|i| if i % side < 128 { 255 } else { 0 }).collect();
    let map = GrassMap::from_tiles(1, 1, 1, vec![Arc::new(GrassTile::from_samples(samples))]);
    let terrain = Terrain { grass: Some(Arc::new(map)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    let grey = renderer.add_material(&MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], ..MaterialDesc::default() });
    let green = renderer.add_material(&MaterialDesc { base_color: [0.1, 0.9, 0.1, 1.0], ..MaterialDesc::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    // Forty to a square metre: some 85,000 blades in view, past the 65,536 instances a view has room for.
    let look = GrassLook {
        mesh: cube,
        material: green.unwrap(),
        density: 40.0,
        scale: [0.12, 0.15],
        draw_distance: 80.0,
        springs_back: 20.0,
    };
    let look = TerrainLook { grass: Arc::from([Some(look)]), ..TerrainLook::new(grey.unwrap(), 4.0) };
    world.spawn((terrain, look, Transform::default()));
    look_down(&mut world, 30.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain grass.png", &pixels);
    let green_share = |x0: u32, x1: u32| {
        let all = (x1 - x0) * (HEIGHT - 20);
        let green = (10..HEIGHT - 10)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = rgb_at(&pixels, x, y);
                g > r + 60 && g > b + 60
            })
            .count() as u32;
        green as f32 / all as f32
    };
    let (left, _) = from_above(30.0, 50.0, 64.0);
    let (right, _) = from_above(30.0, 78.0, 64.0);
    let thick = green_share(left - 60, left + 20);
    let bare = green_share(right - 20, right + 60);
    assert!(thick > 0.3, "the left half grassy: {thick}");
    assert!(bare < 0.02, "none on the right: {bare}");
}

#[test]
fn a_track_maker_presses_a_track_into_snow_that_stays_until_it_fills_in() {
    use std::sync::Arc;
    use voxear_render::{SurfaceLook, TrackMaker};
    use voxear_terrain::surfaces::{WeightTile, Weights};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Brown ground under white snow, which takes tracks all the way.
    let mut look = |colour: [f32; 3], tracks: f32| {
        let [r, g, b] = colour;
        let material = renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() });
        SurfaceLook {
            material: material.unwrap(),
            size: 4.0,
            sharpness: 0.0,
            triplanar: false,
            tracks,
            fills_in: 0.0,
            liquid: voxear_terrain::surfaces::Liquid::None,
            glitter: false,
            leafy: false,
        }
    };
    let surfaces = vec![look([0.3, 0.2, 0.1], 0.0), look([0.9, 0.9, 0.9], 1.0)];
    let weights = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(1))]);
    let terrain = Terrain { weights: Some(Arc::new(weights)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    let plain = renderer.add_material(&MaterialDesc::default()).unwrap();
    let field = world.spawn((
        terrain,
        TerrainLook { surfaces: surfaces.into(), ..TerrainLook::new(plain, 4.0) },
        Transform::default(),
    ));
    look_down(&mut world, 20.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    // A foot on the ground walking along x across the middle.
    let foot =
        world.spawn((TrackMaker { width: 0.5, depth: 0.15 }, Transform::from_position(Vec3::new(58.0, 0.0, 64.0))));
    let mut time = 0.0;
    let mut frames = |renderer: &mut Renderer, world: &mut World, count: u32, walk: f32| {
        for _ in 0..count {
            time += 1.0 / 60.0;
            if let Some(transform) = world.get_mut::<Transform>(foot) {
                transform.position.x += walk;
            }
            renderer.render(world, time).expect("frame renders");
        }
        renderer.read_pixels().expect("pixels").2
    };
    let pixels = frames(&mut renderer, &mut world, 20, 0.6);
    keep_image("terrain tracks.png", &pixels);
    let brightness = |pixels: &[u8], x: f32, z: f32| {
        let (u, v) = from_above(20.0, x, z);
        rgb_at(pixels, u, v).iter().sum::<i32>()
    };
    let (track, beside, ahead) =
        (brightness(&pixels, 63.0, 64.0), brightness(&pixels, 63.0, 66.0), brightness(&pixels, 56.0, 64.0));
    assert!(track < beside - 150, "the track shows the ground under the snow: {track} beside {beside}");
    assert!((beside - ahead).abs() < 20, "untouched snow either side: {beside} and {ahead}");

    // Lifted off the ground, it leaves nothing more; what it left stays.
    world.get_mut::<Transform>(foot).unwrap().position.y = 0.5;
    let pixels = frames(&mut renderer, &mut world, 10, 0.6);
    assert!(brightness(&pixels, 63.0, 64.0) < beside - 150, "the track stays");
    assert!((brightness(&pixels, 74.0, 64.0) - beside).abs() < 20, "and none where it went in the air");

    // The camera goes far enough off that the map scrolls past the track, and back: it was left behind.
    let camera =
        world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().next().map(|(entity, _)| entity).unwrap();
    world.get_mut::<Transform>(camera).unwrap().position.x += 100.0;
    frames(&mut renderer, &mut world, 2, 0.0);
    world.get_mut::<Transform>(camera).unwrap().position.x -= 100.0;
    let pixels = frames(&mut renderer, &mut world, 2, 0.0);
    keep_image("terrain tracks scrolled.png", &pixels);
    assert!((brightness(&pixels, 63.0, 64.0) - beside).abs() < 20, "the map forgot what it scrolled past");

    // A track in snow that fills in over half a second is gone in a second.
    let mut look = world.get::<TerrainLook>(field).unwrap().clone();
    let mut filling: Vec<SurfaceLook> = look.surfaces.to_vec();
    filling[1].fills_in = 0.5;
    look.surfaces = filling.into();
    world.insert(field, look);
    let foot_at = world.get_mut::<Transform>(foot).unwrap();
    foot_at.position = Vec3::new(58.0, 0.0, 64.0);
    let pixels = frames(&mut renderer, &mut world, 10, 0.6);
    assert!(brightness(&pixels, 63.0, 64.0) < beside - 150, "fresh tracks show at once");
    world.get_mut::<Transform>(foot).unwrap().position.y = 0.5;
    let pixels = frames(&mut renderer, &mut world, 60, 0.0);
    assert!((brightness(&pixels, 63.0, 64.0) - beside).abs() < 20, "and fill in");
}

#[test]
fn grass_lies_flat_where_a_track_maker_trod_and_stands_up_again_over_its_kinds_time() {
    use std::sync::Arc;
    use voxear_render::{GrassLook, MeshData, Primitive, TrackMaker};
    use voxear_terrain::grass::{GrassMap, GrassTile};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let side = voxear_terrain::TILE_SAMPLES;
    let map = GrassMap::from_tiles(1, 1, 1, vec![Arc::new(GrassTile::from_samples(vec![255; (side * side) as usize]))]);
    let terrain = Terrain { grass: Some(Arc::new(map)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    let grey = renderer.add_material(&MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], ..MaterialDesc::default() });
    let green =
        renderer.add_material(&MaterialDesc { base_color: [0.1, 0.9, 0.1, 1.0], ..MaterialDesc::default() }).unwrap();
    // Thin blades standing from their foot, a fifth as wide as they are tall: seen from above, they cover far more
    // of the ground lying down than standing.
    let mut blade = MeshData::primitive(Primitive::Cube);
    for vertex in &mut blade.vertices {
        let [x, y, z] = vertex.position;
        vertex.position = [x * 0.2, y + 0.5, z * 0.2];
    }
    let blade = renderer.add_mesh(&blade).unwrap();
    let kind = |springs_back: f32| GrassLook {
        mesh: blade,
        material: green,
        density: 40.0,
        scale: [0.3, 0.3],
        draw_distance: 80.0,
        springs_back,
    };
    let look = TerrainLook { grass: Arc::from([Some(kind(0.0))]), ..TerrainLook::new(grey.unwrap(), 4.0) };
    let field = world.spawn((terrain, look, Transform::default()));
    look_down(&mut world, 20.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    // A foot walking along x across the middle.
    let foot =
        world.spawn((TrackMaker { width: 0.5, depth: 0.15 }, Transform::from_position(Vec3::new(58.0, 0.0, 64.0))));
    let mut time = 0.0;
    let mut frames = |renderer: &mut Renderer, world: &mut World, count: u32, walk: f32| {
        for _ in 0..count {
            time += 1.0 / 60.0;
            if let Some(transform) = world.get_mut::<Transform>(foot) {
                transform.position.x += walk;
            }
            renderer.render(world, time).expect("frame renders");
        }
        renderer.read_pixels().expect("pixels").2
    };
    // How much of a strip along x from 61 to 67 m, a few pixels deep about `z`, is green.
    let green_share = |pixels: &[u8], z: f32| {
        let ((x0, v), (x1, _)) = (from_above(20.0, 61.0, z), from_above(20.0, 67.0, z));
        let all = (x1 - x0) * 5;
        let green = (v - 2..=v + 2)
            .flat_map(|y| (x0..x1).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = rgb_at(pixels, x, y);
                g > r + 60 && g > b + 60
            })
            .count() as u32;
        green as f32 / all as f32
    };
    frames(&mut renderer, &mut world, 20, 0.6);
    // Lifted off the ground, and a second on: grass that never springs back still lies flat.
    world.get_mut::<Transform>(foot).unwrap().position.y = 0.5;
    let pixels = frames(&mut renderer, &mut world, 60, 0.0);
    keep_image("terrain grass trodden.png", &pixels);
    let (trodden, beside) = (green_share(&pixels, 64.0), green_share(&pixels, 66.0));
    assert!(trodden > beside + 0.15, "the blades lie flat along the track: {trodden} against {beside} beside it");

    // Grass that springs back in half a second stands up again.
    world.get_mut::<TerrainLook>(field).unwrap().grass = Arc::from([Some(kind(0.5))]);
    let pixels = frames(&mut renderer, &mut world, 30, 0.0);
    keep_image("terrain grass stood up.png", &pixels);
    let (stood, beside) = (green_share(&pixels, 64.0), green_share(&pixels, 66.0));
    assert!((stood - beside).abs() < 0.08, "the blades stand up again: {stood} against {beside} beside them");
}

#[test]
fn wet_ground_puddles_lava_and_glitter_each_change_how_their_surface_looks() {
    use std::sync::Arc;
    use voxear_render::SurfaceLook;
    use voxear_terrain::{
        runtime::{Edit, apply},
        surfaces::{Liquid, WeightTile, Weights},
    };
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let mut look = |colour: [f32; 3], liquid: Liquid, glitter: bool| {
        let [r, g, b] = colour;
        let desc = MaterialDesc { base_color: [r, g, b, 1.0], roughness: 0.8, ..MaterialDesc::default() };
        let material = renderer.add_material(&desc).unwrap();
        SurfaceLook {
            material,
            size: 4.0,
            sharpness: 0.0,
            triplanar: false,
            tracks: 0.0,
            fills_in: 0.0,
            liquid,
            glitter,
            leafy: false,
        }
    };
    let mud = [0.5, 0.4, 0.3];
    let surfaces = vec![
        look(mud, Liquid::None, false),
        look(mud, Liquid::Wet, false),
        look(mud, Liquid::Puddles, false),
        look([0.12, 0.11, 0.1], Liquid::Lava, false),
        look([0.85, 0.87, 0.9], Liquid::None, true),
    ];
    // Stripes 6 m wide along x from x = 49: plain, wet, puddles, lava, then glittering snow.
    let side = voxear_terrain::TILE_SAMPLES;
    let stripe = |i: u32| (((i % side) as f32 * 0.5 - 49.0) / 6.0).floor().clamp(0.0, 4.0) as u32;
    let weights = Weights::from_tiles(
        1,
        1,
        vec![Arc::new(WeightTile::from_samples((0..side * side).map(|i| [stripe(i), 255]).collect()))],
    );
    let mut terrain = Terrain { weights: Some(Arc::new(weights)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    // A crater each in the wet ground and among the puddles.
    for at in [[58.0, 64.0], [64.0, 64.0]] {
        apply(&mut terrain, &Edit::Crater { at, radius: 2.0, depth: 0.5, surface: None });
    }
    let plain = renderer.add_material(&MaterialDesc::default()).unwrap();
    let field = world.spawn((
        terrain,
        TerrainLook { surfaces: surfaces.into(), ..TerrainLook::new(plain, 4.0) },
        Transform::default(),
    ));
    look_down(&mut world, 20.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain liquids unlit.png", &pixels);
    let brightness = |pixels: &[u8], x: f32, z: f32| {
        let (u, v) = from_above(20.0, x, z);
        rgb_at(pixels, u, v).iter().sum::<i32>()
    };
    let (dry, wet) = (brightness(&pixels, 52.0, 58.0), brightness(&pixels, 58.0, 58.0));
    // About half as bright, which the screen's gamma shows as some four fifths.
    assert!(wet < dry * 85 / 100, "wet ground is darker: {wet} against {dry}");
    let (level, puddle) = (brightness(&pixels, 64.0, 58.0), brightness(&pixels, 64.0, 64.0));
    assert!((level - wet).abs() < 20, "level puddled ground is as wet: {level} against {wet}");
    assert!(puddle < level * 85 / 100, "water stands in the crater, darker still: {puddle} against {level}");
    assert!(brightness(&pixels, 58.0, 64.0) > puddle + 20, "and not in the crater that is only wet");

    // Lit, the lava glows orange through its cracks.
    renderer.set_view_options(ViewOptions::default());
    let sun = Quat::from_rotation_y(0.3) * Quat::from_rotation_x(0.35);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain liquids lit.png", &pixels);
    let glowing = (0..40)
        .flat_map(|z| (0..12).map(move |x| (67.5 + x as f32 * 0.4, 54.0 + z as f32 * 0.5)))
        .filter(|&(x, z)| {
            let (u, v) = from_above(20.0, x, z);
            let [r, g, b] = rgb_at(&pixels, u, v);
            r > 140 && r > g + 30 && g > b
        })
        .count();
    assert!(glowing > 20, "lava's cracks glow: {glowing} of 480 points");

    // From low down looking towards the sun over the snow, its grains sparkle; without Glitter, none do.
    let camera =
        world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().next().map(|(entity, _)| entity).unwrap();
    let low = Quat::from_rotation_y(std::f32::consts::PI) * Quat::from_rotation_x(0.22);
    *world.get_mut::<Transform>(camera).unwrap() = Transform::new(Vec3::new(76.0, 1.8, 60.0), low, Vec3::ONE);
    let sparks = |pixels: &[u8]| {
        let near = (HEIGHT * 2 / 3..HEIGHT).flat_map(|y| (WIDTH / 3..WIDTH * 2 / 3).map(move |x| (x, y)));
        let all: Vec<i32> = near.map(|(x, y)| rgb_at(pixels, x, y).iter().sum::<i32>()).collect();
        let mean = all.iter().sum::<i32>() / all.len() as i32;
        all.iter().filter(|&&b| b > mean + 120).count()
    };
    let glittering = render(&mut renderer, &world, 3);
    keep_image("terrain glitter.png", &glittering);
    let mut plain_snow = world.get::<TerrainLook>(field).unwrap().clone();
    let mut looks = plain_snow.surfaces.to_vec();
    looks[4].glitter = false;
    plain_snow.surfaces = looks.into();
    world.insert(field, plain_snow);
    let still = render(&mut renderer, &world, 3);
    let (on, off) = (sparks(&glittering), sparks(&still));
    assert!(on > 10 && off < on / 4, "the snow sparkles with Glitter: {on} bright points, {off} without");
}

#[test]
fn a_model_whose_material_blends_into_the_ground_takes_the_grounds_look_at_its_foot() {
    use std::sync::Arc;
    use voxear_render::{MeshRenderer, Primitive, SurfaceLook};
    use voxear_terrain::surfaces::{Liquid, WeightTile, Weights};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Green ground, all one surface.
    let desc = MaterialDesc { base_color: [0.0, 0.8, 0.0, 1.0], ..MaterialDesc::default() };
    let green = renderer.add_material(&desc).unwrap();
    let surface = SurfaceLook {
        material: green,
        size: 4.0,
        sharpness: 0.0,
        triplanar: false,
        tracks: 0.0,
        fills_in: 0.0,
        liquid: Liquid::None,
        glitter: false,
        leafy: false,
    };
    let weights = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(0))]);
    let terrain = Terrain { weights: Some(Arc::new(weights)), ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    let plain = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((
        terrain,
        TerrainLook { surfaces: Arc::from([surface]), ..TerrainLook::new(plain, 4.0) },
        Transform::default(),
    ));
    // A red block two metres tall, half a metre sunk, seen side on from five metres off.
    let red = MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ground_blend: 0.5, ..MaterialDesc::default() };
    let red = renderer.add_material(&red).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let block = Transform::new(Vec3::new(64.0, 0.5, 64.0), Quat::IDENTITY, Vec3::splat(2.0));
    world.spawn((MeshRenderer::new(cube, red), block));
    world.spawn((Camera::default(), Transform::new(Vec3::new(64.0, 0.8, 58.0), Quat::IDENTITY, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain ground blend.png", &pixels);
    // About 62 pixels a metre on the block's face, five metres off.
    let at = |y: f32| rgb_at(&pixels, WIDTH / 2, (HEIGHT as f32 / 2.0 + (0.8 - y) * 62.0) as u32);
    let ([fr, fg, _], [tr, tg, _]) = (at(0.08), at(1.3));
    assert!(fg > fr, "its foot takes the ground's green: {:?}", at(0.08));
    assert!(tr > tg * 2, "its top stays red: {:?}", at(1.3));

    // Without the blend, red to the ground.
    renderer
        .update_material(red, &MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() })
        .unwrap();
    let pixels = render(&mut renderer, &world, 2);
    let [r, g, _] = rgb_at(&pixels, WIDTH / 2, (HEIGHT as f32 / 2.0 + (0.8 - 0.08) * 62.0) as u32);
    assert!(r > g * 2, "red to the ground without it: {:?}", [r, g]);
}

#[test]
fn ground_dug_out_of_a_terrain_is_drawn_in_its_dug_surface_with_no_gap_at_its_rim() {
    use std::sync::Arc;
    use voxear_render::{GrassLook, Primitive, SurfaceLook};
    use voxear_terrain::{
        grass::{GrassMap, GrassTile},
        runtime::{Edit, apply},
        surfaces::{Liquid, WeightTile, Weights},
    };
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let mut look = |colour: [f32; 3]| {
        let [r, g, b] = colour;
        let desc = MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() };
        let material = renderer.add_material(&desc).unwrap();
        SurfaceLook {
            material,
            size: 4.0,
            sharpness: 0.0,
            triplanar: false,
            tracks: 0.0,
            fills_in: 0.0,
            liquid: Liquid::None,
            glitter: false,
            leafy: false,
        }
    };
    // Green grass over the field, and brown earth for what digging bares.
    let surfaces = vec![look([0.1, 0.7, 0.1]), look([0.5, 0.3, 0.1])];
    let weights = Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(0))]);
    // And red grass all over it, thinly.
    let grass = GrassTile::from_samples(vec![0xFF; voxear_terrain::heights::TILE_LEN]);
    let grass = Some(Arc::new(GrassMap::from_tiles(1, 1, 1, vec![Arc::new(grass)])));
    let mut terrain =
        Terrain { weights: Some(Arc::new(weights)), grass, ..Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0)) };
    apply(&mut terrain, &Edit::Dig { at: [64.0, 0.0, 64.0], radius: 3.0, surface: Some(1) });
    assert!(terrain.dug.is_some());
    let plain = renderer.add_material(&MaterialDesc::default()).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [1.0, 0.0, 0.0, 1.0], ..MaterialDesc::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let blades = GrassLook {
        mesh: cube,
        material: red.unwrap(),
        density: 4.0,
        scale: [0.1, 0.1],
        draw_distance: 80.0,
        springs_back: 20.0,
    };
    let look =
        TerrainLook { surfaces: surfaces.into(), grass: Arc::from([Some(blades)]), ..TerrainLook::new(plain, 4.0) };
    world.spawn((terrain, look, Transform::default()));
    look_down(&mut world, 20.0);
    renderer.set_view_options(ViewOptions { debug_view: voxear_render::DEBUG_VIEW_UNLIT, ..ViewOptions::default() });
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain dug.png", &pixels);
    // Red blades in a square metre: in the dug column beside the pit, as on the ground not dug, and none in the pit.
    let blades = |x0: f32, z0: f32| {
        let mut count = 0;
        for i in 0..24 {
            for j in 0..24 {
                let (x, y) = from_above(20.0, x0 + i as f32 / 24.0, z0 + j as f32 / 24.0);
                let [r, g, b] = rgb_at(&pixels, x, y);
                count += usize::from(r > 150 && g < 80 && b < 80);
            }
        }
        count
    };
    let (dug_column, not_dug, pit) = (blades(59.5, 59.5), blades(50.5, 59.5), blades(63.5, 63.5));
    assert!(dug_column > 5 && not_dug > 5, "grass grows in the dug column: {dug_column}, and beyond it: {not_dug}");
    assert_eq!(pit, 0, "and none in the pit");
    let [r, g, _] = rgb_at(&pixels, WIDTH / 2, HEIGHT / 2);
    assert!(r > g, "the pit's floor is the dug earth: {:?}", rgb_at(&pixels, WIDTH / 2, HEIGHT / 2));
    // Round the pit, out past its rim, every pixel is ground (or grass): none of the sky through a gap.
    let mut gaps = Vec::new();
    for ring in 0..60 {
        let turn = ring as f32 / 60.0 * std::f32::consts::TAU;
        for r in [1.0, 2.0, 2.9, 3.05, 3.3, 4.0] {
            let (x, y) = from_above(20.0, 64.0 + turn.cos() * r, 64.0 + turn.sin() * r);
            let [cr, cg, cb] = rgb_at(&pixels, x, y);
            if cb > cr && cb > cg {
                gaps.push((x, y, [cr, cg, cb]));
            }
        }
    }
    assert!(gaps.is_empty(), "{} gaps, first {:?}", gaps.len(), gaps.first());
    let [r, g, _] = rgb_at(&pixels, from_above(20.0, 75.0, 60.0).0, from_above(20.0, 75.0, 60.0).1);
    assert!(g > r || r > 150, "the ground's green, or a red blade, beyond it");

    // From 120 m up, past where its finer levels reach (32 and 64 m, for ground drawn in detail only near), the pit
    // is drawn at its coarsest, whole: dug earth to its rim, and no sky through it.
    let camera =
        world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().next().map(|(entity, _)| entity).unwrap();
    world.get_mut::<Transform>(camera).unwrap().position.y = 120.0;
    let field =
        world.query_ref::<(voxear_ecs::Entity, &Terrain)>().into_iter().next().map(|(entity, _)| entity).unwrap();
    world.get_mut::<Terrain>(field).unwrap().detail_distance = 10.0;
    let pixels = render(&mut renderer, &world, 3);
    keep_image("terrain dug far.png", &pixels);
    let mut earth = 0;
    for ring in 0..24 {
        let turn = ring as f32 / 24.0 * std::f32::consts::TAU;
        for r in [0.0, 0.8, 1.6, 2.2] {
            let (x, y) = from_above(120.0, 64.0 + turn.cos() * r, 64.0 + turn.sin() * r);
            let [cr, cg, cb] = rgb_at(&pixels, x, y);
            assert!(!(cb > cr && cb > cg), "sky through the pit at {r} m out: {:?}", [cr, cg, cb]);
            earth += usize::from(cr > cg);
        }
    }
    assert!(earth > 80, "dug earth across the pit: {earth} of 96");
}

#[test]
fn erosion_wears_high_ground_down_into_low_keeping_its_volume_and_slumps_a_cliff() {
    use voxear_terrain::erosion::{Weather, grid};
    let Some(renderer) = renderer() else { return };
    // Generated mountains, 2 km across, a cell every 4 m.
    let world = voxear_terrain::generate::Preset::Alpine.world();
    let heights = Heightmap::from_fn(2, 2, 4.0, |x, z| world.height_at(x + 3000.0, z + 5000.0));
    let ground = grid(&heights);
    let eroded = renderer.erode(&ground, &Weather::default()).expect("eroded");
    let changes: Vec<f32> = ground.heights.iter().zip(&eroded).map(|(a, b)| b - a).collect();
    let moved: f32 = changes.iter().map(|c| c.abs()).sum();
    let gained: f32 = changes.iter().sum();
    assert!(moved > 10_000.0, "the ground worn: {moved} m moved");
    assert!(gained.abs() < moved * 0.01, "as much ground after as before: {gained} of {moved} moved");
    // The highest tenth of the ground loses some, and the lowest tenth gains it.
    let mut by_height: Vec<usize> = (0..changes.len()).collect();
    by_height.sort_by(|&a, &b| ground.heights[a].total_cmp(&ground.heights[b]));
    let tenth = changes.len() / 10;
    let mean = |cells: &[usize]| cells.iter().map(|&i| changes[i]).sum::<f32>() / cells.len() as f32;
    let (low, high) = (mean(&by_height[..tenth]), mean(&by_height[changes.len() - tenth..]));
    assert!(high < 0.0 && low > 0.0, "worn from the heights ({high} m) into the lows ({low} m)");

    // With no rain, a cliff 20 m high slumps back to about the slope loose ground stands at, 45° here.
    let cliff = Heightmap::from_fn(1, 1, 1.0, |x, _| if x < 128.0 { 20.0 } else { 0.0 });
    let ground = grid(&cliff);
    let dry = Weather { rain: 0.0, slump: 45.0, steps: 1500, ..Weather::default() };
    let slumped = renderer.erode(&ground, &dry).expect("eroded");
    let row = |x: u32| slumped[(128 * ground.cells[0] + x) as usize];
    let steepest = (100..160).map(|x| (row(x) - row(x + 1)).abs()).fold(0.0f32, f32::max);
    assert!(steepest < 1.3, "no steeper than about a metre a metre: {steepest}");
    assert!(row(122) < 19.0 && row(134) > 1.0, "its top worn back and its foot heaped: {} and {}", row(122), row(134));
}

#[test]
fn a_terrain_without_ends_window_moved_under_the_camera_draws_the_same_ground() {
    use std::{collections::HashMap, sync::Arc};
    use voxear_terrain::{HeightTile, TILE_QUADS, TILE_SAMPLES};
    let Some(mut renderer) = renderer() else { return };
    // Hills over the world, tile by tile, 128 m a tile, each made once and shared as a window keeps it.
    let ground = |x: f32, z: f32| (x * 0.05).sin() * 6.0 + (z * 0.04).cos() * 5.0 + (x * 0.13 + z * 0.09).sin();
    let mut made: HashMap<[i32; 2], Arc<HeightTile>> = HashMap::new();
    let mut window = |origin: [i32; 2]| -> Vec<Arc<HeightTile>> {
        (0..9)
            .map(|i| [origin[0] + i % 3, origin[1] + i / 3])
            .map(|at| {
                made.entry(at)
                    .or_insert_with(|| {
                        let place = |tile: i32, sample: u32| (tile * TILE_QUADS as i32 + sample as i32) as f32 * 0.5;
                        let samples = (0..TILE_SAMPLES * TILE_SAMPLES)
                            .map(|i| ground(place(at[0], i % TILE_SAMPLES), place(at[1], i / TILE_SAMPLES)))
                            .collect();
                        Arc::new(HeightTile::from_samples(samples))
                    })
                    .clone()
            })
            .collect()
    };
    let mut world = World::new();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    let heights = Heightmap::from_tiles(3, 3, 0.5, window([-1, -1]));
    let terrain = Terrain { origin: Some([-1, -1]), detail_distance: 40.0, ..Terrain::new(heights) };
    // Seen from over the edge between its tiles (0, 0) and (1, 0), which both windows have.
    let corner = Transform::from_position(Vec3::new(-64.0, 0.0, 0.0));
    let entity = world.spawn((terrain, TerrainLook::new(grey, 4.0), corner));
    look_down(&mut world, 60.0);
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let before = render(&mut renderer, &world, 4);
    keep_image("terrain endless before.png", &before);
    // A tile along x: the window from (0, -1), its tiles each where they were in the world, in their slots turned.
    let moved = world.get::<Terrain>(entity).unwrap().heights.with_shared_tiles(window([0, -1]));
    let terrain = world.get_mut::<Terrain>(entity).unwrap();
    (terrain.heights, terrain.origin) = (Arc::new(moved), Some([0, -1]));
    let after = render(&mut renderer, &world, 4);
    keep_image("terrain endless after.png", &after);
    let differences: Vec<i32> = before.iter().zip(&after).map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs()).collect();
    let far_off = differences.iter().filter(|&&d| d > 6).count();
    assert!(far_off * 1000 < differences.len(), "{far_off} of {} values differ by more than 6", differences.len());
    // And lit as hills: brighter facing the sun, darker away from it.
    let values: Vec<i32> = (0..HEIGHT)
        .step_by(9)
        .flat_map(|y| (0..WIDTH).step_by(9).map(move |x| (x, y)))
        .map(|(x, y)| rgb_at(&after, x, y).iter().sum())
        .collect();
    let (darkest, brightest) = (values.iter().min().unwrap(), values.iter().max().unwrap());
    assert!(brightest - darkest > 60, "shaded by its slopes: from {darkest} to {brightest}");
}
