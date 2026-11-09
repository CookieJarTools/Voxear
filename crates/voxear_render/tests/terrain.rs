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
        surfaces.push(SurfaceLook { material: material.unwrap(), size: 4.0, sharpness: 0.0, triplanar: false });
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
    let look = GrassLook { mesh: cube, material: green.unwrap(), density: 40.0, scale: [0.12, 0.15], draw_distance: 80.0 };
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
