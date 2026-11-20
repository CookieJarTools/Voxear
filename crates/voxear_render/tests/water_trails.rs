//! Trails on water, drawn on a real GPU, offscreen: a pond of blocks and a
//! Pool, with a swimmer and a boat moving across them. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BlockLook, Camera, Light, MaterialDesc, Pool, PostProcess, Renderer,
    RendererSettings, Sky, Stir, Stirs, SurfaceLook, TrailFlow, TrailLook, TrailQuality, WaterTrails,
    water_trails::FLOW_CELLS,
};
use voxear_scene::Transform;
use voxear_terrain::blocks::{
    BlockWorld,
    chunk::BlockId,
    file::{BlockFile, BlockKind},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
/// How many blocks a side the pond is, and how high its water stands.
const SIDE: i32 = 96;
const TOP: i32 = 6;

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

/// The sun in the west and the look everything is drawn with, no
/// anti-aliasing so frames are alike.
fn lit(world: &mut World) {
    let towards_sun = Vec3::new(-50f32.to_radians().cos(), 50f32.to_radians().sin(), 0.35).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.35, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
}

/// A pond of still water [`SIDE`] blocks a side, its top at [`TOP`], over a
/// floor of sand three blocks down.
fn pond(renderer: &mut Renderer) -> World {
    let mut file = BlockFile::default();
    file.ground.size = [SIDE as u32, SIDE as u32];
    file.ground.height = TOP as u32;
    file.blocks.push(BlockKind::colored("Sand", [0.86, 0.78, 0.55]));
    let sand = file.blocks.len() as BlockId;
    file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.03, 0.3, 0.42]) });
    let water = file.blocks.len() as BlockId;
    let mut blocks = file.make_ground(None);
    for z in 0..SIDE {
        for x in 0..SIDE {
            for y in 0..TOP + 2 {
                let block = match () {
                    () if y < TOP - 3 => sand,
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
    let look = BlockLook {
        material: default,
        surfaces: Arc::from(surfaces),
        voxels: Default::default(),
        motions: Arc::from([]),
        light: Default::default(),
        ground_cover: Default::default(),
    };
    let mut world = World::new();
    world.spawn((BlockWorld::new(blocks, &faces, 1.0), look, Transform::default()));
    lit(&mut world);
    world
}

/// A Pool of water 60 m a side, its top at 0, over ground it does not show.
fn pool_world() -> World {
    let mut world = World::new();
    let pool = Pool { size: [60.0, 60.0], depth: 3.0, ..Pool::default() };
    world.spawn((pool, Transform::from_position(Vec3::ZERO)));
    lit(&mut world);
    world
}

/// Points the view from `eye` at `target`.
fn look(world: &mut World, (eye, target): (Vec3, Vec3)) {
    let cameras: Vec<_> = world.query_ref::<(Entity, &Camera)>().into_iter().map(|(e, _)| e).collect();
    for camera in cameras {
        world.despawn(camera);
    }
    let towards = (target - eye).normalize();
    let rotation = Quat::from_rotation_y(towards.x.atan2(towards.z)) * Quat::from_rotation_x(-towards.y.asin());
    world.spawn((Camera::default(), Transform::new(eye, rotation, Vec3::ONE)));
}

/// A swimmer treading water with its head out, its feet 1.2 m under a
/// surface at `surface`.
fn swimmer(entity: Entity, surface: f32) -> Stir {
    Stir {
        entity,
        ends: [Vec3::ZERO; 2],
        radius: 0.45,
        surface,
        reach: [0.0, 1.8],
        flow: Vec3::ZERO,
        strength: 1.0,
        foam: 1.0,
        bob: 1.0,
    }
}

/// A boat 4 m long and 1.4 m wide, its keel half a metre under a surface at
/// `surface`, its length along x.
fn boat(entity: Entity, surface: f32) -> Stir {
    Stir {
        entity,
        ends: [Vec3::new(1.3, 0.0, 0.0), Vec3::new(-1.3, 0.0, 0.0)],
        radius: 0.7,
        surface,
        reach: [-0.5, 0.8],
        flow: Vec3::ZERO,
        strength: 1.0,
        foam: 1.0,
        bob: 0.0,
    }
}

/// Moves what stirs the water from `from` along `velocity` for `seconds` at
/// 60 frames a second from `start`, drawing each frame, then lets the water
/// run on `after` seconds with it gone; gives back the last frame.
fn swim(
    renderer: &mut Renderer,
    world: &mut World,
    (stir, from, velocity): (impl Fn(Entity, f32) -> Stir, Vec3, Vec3),
    (start, seconds, after): (f32, f32, f32),
    surface: f32,
) -> Vec<u8> {
    let body = world.spawn(Transform::from_position(from));
    let frames = (seconds * 60.0) as u32;
    for frame in 0..frames {
        let at = from + velocity * (frame as f32 / 60.0);
        *world.get_mut::<Transform>(body).unwrap() = Transform::from_position(at);
        world.insert_resource(Stirs(vec![stir(body, surface)]));
        renderer.render(world, start + frame as f32 / 60.0).expect("frame renders");
    }
    world.despawn(body);
    world.insert_resource(Stirs(Vec::new()));
    let end = start + frames as f32 / 60.0;
    for frame in 0..((after * 60.0) as u32).max(1) {
        renderer.render(world, end + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

/// The same water left alone over as long.
fn calm(renderer: &mut Renderer, world: &mut World, (start, seconds): (f32, f32)) -> Vec<u8> {
    world.insert_resource(Stirs(Vec::new()));
    for frame in 0..((seconds * 60.0) as u32).max(1) {
        renderer.render(world, start + frame as f32 / 60.0).expect("frame renders");
    }
    renderer.read_pixels().expect("pixels").2
}

/// The same scene in two renderers drawn at the same moments, what stirs the
/// water put only in the first: the water's own ripples move on as it is
/// drawn, so what differs between the two is the trails alone.
struct Twin {
    stirred: (Renderer, World),
    calm: (Renderer, World),
}

impl Twin {
    fn new(scene: impl Fn(&mut Renderer) -> World) -> Option<Self> {
        let (mut a, mut b) = (renderer()?, renderer()?);
        let (world_a, world_b) = (scene(&mut a), scene(&mut b));
        Some(Self { stirred: (a, world_a), calm: (b, world_b) })
    }

    fn each(&mut self, mut change: impl FnMut(&mut World)) {
        change(&mut self.stirred.1);
        change(&mut self.calm.1);
    }

    /// [`swim`] in the first and [`calm`] in the second over the same frames:
    /// the stirred picture and the calm one.
    fn swim(
        &mut self,
        moving: (impl Fn(Entity, f32) -> Stir, Vec3, Vec3),
        (start, seconds, after): (f32, f32, f32),
        surface: f32,
    ) -> (Vec<u8>, Vec<u8>) {
        let (renderer, world) = &mut self.stirred;
        let stirred = swim(renderer, world, moving, (start, seconds, after), surface);
        let (renderer, world) = &mut self.calm;
        let frames = (seconds * 60.0) as u32 as f32 / 60.0 + ((after * 60.0) as u32).max(1) as f32 / 60.0;
        (stirred, calm(renderer, world, (start, frames)))
    }

    /// Both left alone over the same frames.
    fn rest(&mut self, (start, seconds): (f32, f32)) -> (Vec<u8>, Vec<u8>) {
        let (renderer, world) = &mut self.stirred;
        let a = calm(renderer, world, (start, seconds));
        let (renderer, world) = &mut self.calm;
        (a, calm(renderer, world, (start, seconds)))
    }
}

/// How far apart two pictures are over a box of pixels, `[x0, y0, x1, y1]`,
/// on average across their colours.
fn apart(a: &[u8], b: &[u8], [x0, y0, x1, y1]: [u32; 4]) -> f32 {
    let mut sum = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * WIDTH + x) * 4) as usize;
            sum += (0..3).map(|c| u64::from(a[i + c].abs_diff(b[i + c]))).sum::<u64>();
        }
    }
    sum as f32 / ((x1 - x0) * (y1 - y0) * 3) as f32
}

/// How many pixels of a box are near white, as foam is.
fn white(pixels: &[u8], [x0, y0, x1, y1]: [u32; 4]) -> usize {
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * WIDTH + x) * 4) as usize;
            count += usize::from(pixels[i..i + 3].iter().all(|&c| c > 170));
        }
    }
    count
}

/// Looking down over the pond's middle from a few metres up, a little way
/// back.
fn over_the_pond() -> (Vec3, Vec3) {
    let middle = SIDE as f32 * 0.5;
    (Vec3::new(middle, TOP as f32 + 7.0, middle - 9.0), Vec3::new(middle, TOP as f32, middle + 2.0))
}

/// Looking straight down on the pond's middle from `high` metres over it.
fn down_on_the_pond(high: f32) -> (Vec3, Vec3) {
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    (Vec3::new(middle, surface + high, middle - 0.01), Vec3::new(middle, surface, middle))
}

/// The whole picture but its top, the sky.
const WATER: [u32; 4] = [0, HEIGHT / 3, WIDTH, HEIGHT];

#[test]
fn a_swimmer_leaves_ripples_and_foam_behind_on_a_world_of_blocks_and_they_fade() {
    let Some(mut twin) = Twin::new(pond) else { return };
    twin.each(|world| world.insert_resource(WaterTrails { look: TrailLook::Smooth, ..WaterTrails::default() }));
    twin.each(|world| look(world, over_the_pond()));
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    twin.rest((10.0, 1.0));
    // Swimming across the view, left to right, at 2.5 m/s for two seconds.
    let from = Vec3::new(middle - 3.5, surface - 1.2, middle + 1.0);
    let (swum, still) = twin.swim((swimmer, from, Vec3::new(2.5, 0.0, 0.0)), (11.0, 2.0, 0.05), surface);
    keep_image("trails swimmer.png", &swum);
    keep_image("trails swimmer calm.png", &still);
    let stirred = apart(&swum, &still, WATER);
    assert!(stirred > 1.0, "the water behind a swimmer is stirred: {stirred}");
    let foam = (white(&swum, WATER), white(&still, WATER));
    assert!(foam.0 > foam.1 + 150, "foam behind it: {} against {}", foam.0, foam.1);
    // Long after, the water lies as it would have.
    let (settled, left) = twin.rest((13.05, 25.0));
    keep_image("trails swimmer settled.png", &settled);
    let after = apart(&settled, &left, WATER);
    assert!(after < 0.05, "calm again: {after}");
}

#[test]
fn a_boat_leaves_a_v_of_waves_that_spreads_behind_it() {
    let Some(mut twin) = Twin::new(pond) else { return };
    twin.each(|world| world.insert_resource(WaterTrails { look: TrailLook::Smooth, ..WaterTrails::default() }));
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    // From high over the pond, looking straight down on where it went.
    twin.each(|world| look(world, down_on_the_pond(30.0)));
    twin.rest((10.0, 1.0));
    let from = Vec3::new(middle - 14.0, surface - 0.1, middle);
    let (sailed, still) = twin.swim((boat, from, Vec3::new(6.0, 0.0, 0.0)), (11.0, 3.0, 0.05), surface);
    keep_image("trails boat.png", &sailed);
    keep_image("trails boat calm.png", &still);
    // Down the middle of the view, x along the picture, a metre some 26 pixels: where it passed is stirred along its
    // path, behind it, and the V's arms (some 10° either side at 6 m/s) lie two metres out from its track ten metres
    // back from where it stopped, but not two metres behind it.
    let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
    let track = [cx - 200, cy - 12, cx + 60, cy + 12];
    assert!(apart(&sailed, &still, track) > 4.0, "its track: {}", apart(&sailed, &still, track));
    let beside = |x0: u32, x1: u32, away: u32| {
        let up = [x0, cy - away - 14, x1, cy - away];
        let down = [x0, cy + away, x1, cy + away + 14];
        (apart(&sailed, &still, up) + apart(&sailed, &still, down)) * 0.5
    };
    let (far_back, near) = ((cx - 240, cx - 140), (cx + 40, cx + 100));
    let wide_back = beside(far_back.0, far_back.1, 50);
    let wide_near = beside(near.0, near.1, 50);
    eprintln!("beside its track: {wide_back} far back, {wide_near} near the bow");
    assert!(wide_back > wide_near + 1.0, "spread wider further back: {wide_back} against {wide_near} near the bow");
    assert!(white(&sailed, track) > white(&still, track) + 300, "white water down its track");
}

#[test]
fn trails_lie_on_a_pool_too_and_none_are_drawn_with_quality_off() {
    let Some(mut twin) = Twin::new(|_| pool_world()) else { return };
    twin.each(|world| look(world, (Vec3::new(0.0, 7.0, -9.0), Vec3::new(0.0, 0.0, 2.0))));
    twin.rest((10.0, 1.0));
    let from = Vec3::new(-3.5, -1.2, 1.0);
    let (swum, still) = twin.swim((swimmer, from, Vec3::new(2.5, 0.0, 0.0)), (11.0, 2.0, 0.05), 0.0);
    keep_image("trails pool.png", &swum);
    let stirred = apart(&swum, &still, WATER);
    assert!(stirred > 0.2, "a Pool's water is stirred too: {stirred}");
    // With trails off the same swim leaves the water as it would have been.
    twin.each(|world| world.insert_resource(WaterTrails { quality: TrailQuality::Off, ..WaterTrails::default() }));
    let (swum, still) = twin.swim((swimmer, from, Vec3::new(2.5, 0.0, 0.0)), (30.0, 2.0, 0.05), 0.0);
    assert!(apart(&swum, &still, WATER) < 0.05, "trails off: {}", apart(&swum, &still, WATER));
}

#[test]
fn rings_spread_from_where_something_drops_into_the_water() {
    let Some(mut twin) = Twin::new(pond) else { return };
    twin.each(|world| world.insert_resource(WaterTrails { look: TrailLook::Smooth, ..WaterTrails::default() }));
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    twin.each(|world| look(world, down_on_the_pond(25.0)));
    twin.rest((10.0, 1.0));
    // A crate half a metre across falling at 6 m/s, from a metre over the water to two under it, and the water left
    // to run on for a second and a half.
    let crate_of = |entity: Entity, surface: f32| Stir {
        entity,
        ends: [Vec3::ZERO; 2],
        radius: 0.35,
        surface,
        reach: [-0.3, 0.3],
        flow: Vec3::ZERO,
        strength: 1.0,
        foam: 1.0,
        bob: 0.0,
    };
    let from = Vec3::new(middle, surface + 1.0, middle);
    let (dropped, still) = twin.swim((crate_of, from, Vec3::new(0.0, -6.0, 0.0)), (11.0, 0.5, 1.5), surface);
    keep_image("trails dropped in.png", &dropped);
    // Round where it went in, out to two metres and more: 25 m up, a metre is about 33 pixels.
    let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
    let ring = |near: u32, far: u32| {
        let mut sum = 0.0;
        for (x0, y0) in [(cx + near, cy - 10), (cx - far, cy - 10), (cx - 10, cy + near), (cx - 10, cy - far)] {
            let wide = far - near;
            let boxed = if y0 == cy - 10 { [x0, y0, x0 + wide, y0 + 20] } else { [x0, y0, x0 + 20, y0 + wide] };
            sum += apart(&dropped, &still, boxed);
        }
        sum / 4.0
    };
    let (close, out, beyond) = (ring(0, 30), ring(30, 70), ring(200, 260));
    eprintln!("stirred round where it went in: {close} close, {out} further out, {beyond} far off");
    assert!(close > 1.0 && out > 0.1, "rings round it: {close}, {out}");
    assert!(beyond < 0.05, "not yet far off: {beyond}");
    let foam =
        (white(&dropped, [cx - 40, cy - 40, cx + 40, cy + 40]), white(&still, [cx - 40, cy - 40, cx + 40, cy + 40]));
    assert!(foam.0 > foam.1 + 30, "foam thrown up where it went in: {} against {}", foam.0, foam.1);
}

/// Where across the picture what differs between two pictures lies, on
/// average along x, weighted by how much it differs.
fn middle_of_change(a: &[u8], b: &[u8]) -> f32 {
    let (mut sum, mut weight) = (0.0f64, 0.0f64);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let i = ((y * WIDTH + x) * 4) as usize;
            let change: f64 = (0..3).map(|c| f64::from(a[i + c].abs_diff(b[i + c]))).sum();
            sum += f64::from(x) * change;
            weight += change;
        }
    }
    (sum / weight.max(1.0)) as f32
}

#[test]
fn ripples_drift_down_a_river_with_its_current() {
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    // A swimmer treading water in one place for a second, then gone, and the water left to run on for two more:
    // in still water, then in a river running along x at 1.5 m/s, which carries the swimmer as fast.
    let at = Vec3::new(middle - 2.0, surface - 1.2, middle);
    let rings = |flow: [f32; 2]| {
        let mut twin = Twin::new(pond)?;
        twin.each(|world| world.insert_resource(WaterTrails { look: TrailLook::Smooth, ..WaterTrails::default() }));
        twin.each(|world| look(world, down_on_the_pond(25.0)));
        let corner = [middle - 32.0, middle - 32.0];
        twin.each(|world| {
            world.insert_resource(TrailFlow { corner, cell: 4.0, cells: vec![flow; FLOW_CELLS * FLOW_CELLS] })
        });
        twin.rest((10.0, 1.0));
        let carried =
            |entity: Entity, surface: f32| Stir { flow: Vec3::new(flow[0], 0.0, flow[1]), ..swimmer(entity, surface) };
        let (left, still) = twin.swim((carried, at, Vec3::new(flow[0], 0.0, flow[1])), (11.0, 1.0, 2.0), surface);
        Some(middle_of_change(&left, &still))
    };
    let Some(still_water) = rings([0.0, 0.0]) else { return };
    let Some(river) = rings([1.5, 0.0]) else { return };
    // 25 m up with a 60° view, the picture is about 29 m across: 1.5 m/s for the two seconds after, and half the
    // second it trod, is some 3.75 m, or about 120 pixels.
    eprintln!("the rings' middle across the picture: still {still_water}, in the river {river}");
    assert!(river > still_water + 60.0, "carried downstream: {river} against {still_water}");
}

#[test]
fn on_a_world_of_blocks_trails_stand_in_squares_of_a_quarter_block() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond(&mut renderer);
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    // Close over the water, looking down, so each quarter of a block is many pixels across.
    let close = (Vec3::new(middle, surface + 5.0, middle - 0.01), Vec3::new(middle, surface, middle));
    look(&mut world, close);
    let from = Vec3::new(middle - 3.0, surface - 1.2, middle - 1.2);
    let mut shot = |world: &mut World, renderer: &mut Renderer, look: TrailLook, start: f32| {
        world.insert_resource(WaterTrails { look, ..WaterTrails::default() });
        calm(renderer, world, (start, 1.0));
        swim(renderer, world, (swimmer, from, Vec3::new(1.5, 0.0, 0.0)), (start + 1.0, 2.5, 0.05), surface)
    };
    let blocky = shot(&mut world, &mut renderer, TrailLook::MatchWater, 10.0);
    keep_image("trails blocky.png", &blocky);
    let smooth = shot(&mut world, &mut renderer, TrailLook::Smooth, 40.0);
    keep_image("trails smooth.png", &smooth);
    // Blocky, neighbouring pixels within a square agree; their changes come at the squares' edges, which lie on whole
    // quarters of a block: count how much of the change across each row falls on the columns the edges cross.
    let pixels_per_square = WIDTH as f32 / (2.0 * 5.0 * (30f32.to_radians()).tan() * 16.0 / 9.0) * 0.25;
    let on_edges = |pixels: &[u8]| {
        let (mut edge, mut all) = (0u64, 0u64);
        for y in HEIGHT / 2 - 60..HEIGHT / 2 + 60 {
            for x in 1..WIDTH {
                let (i, j) = (((y * WIDTH + x) * 4) as usize, ((y * WIDTH + x - 1) * 4) as usize);
                let change: u64 = (0..3).map(|c| u64::from(pixels[i + c].abs_diff(pixels[j + c]))).sum();
                all += change;
                let along = (x as f32 - WIDTH as f32 * 0.5) / pixels_per_square;
                if (along - along.round()).abs() * pixels_per_square < 1.5 {
                    edge += change;
                }
            }
        }
        edge as f32 / all.max(1) as f32
    };
    let (squared, rounded) = (on_edges(&blocky), on_edges(&smooth));
    eprintln!("share of the change across rows on the squares' edges: blocky {squared}, smooth {rounded}");
    assert!(squared > rounded * 1.4, "blocky {squared} against smooth {rounded}");
}

#[test]
fn the_trails_cost_little_of_a_frame() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = pond(&mut renderer);
    look(&mut world, over_the_pond());
    let (middle, surface) = (SIDE as f32 * 0.5, TOP as f32);
    let body = world.spawn(Transform::from_position(Vec3::new(middle - 3.5, surface - 1.2, middle + 1.0)));
    for quality in [TrailQuality::Low, TrailQuality::Medium, TrailQuality::High] {
        world.insert_resource(WaterTrails { quality, ..WaterTrails::default() });
        // The least of sixty frames of the passes, as the GPU is shared.
        let mut least = f32::MAX;
        for frame in 0..90u32 {
            let at = Vec3::new(middle - 3.5 + frame as f32 * 0.04, surface - 1.2, middle + 1.0);
            *world.get_mut::<Transform>(body).unwrap() = Transform::from_position(at);
            world.insert_resource(Stirs(vec![swimmer(body, surface)]));
            renderer.render(&world, 10.0 + frame as f32 / 60.0).expect("frame renders");
            let ms: f32 =
                renderer.timings().iter().filter(|(name, _)| name.starts_with("Water trails")).map(|(_, ms)| *ms).sum();
            if frame >= 30 {
                least = least.min(ms);
            }
        }
        eprintln!("water trails at {:?}: {least} ms", quality);
        assert!(least > 0.0, "the pass ran");
        assert!(least < 1.0, "{quality:?}: {least} ms");
    }
}
