//! Voxel Volumes drawn on a real GPU, offscreen. Each test skips itself
//! (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AmbientOcclusion, AntiAliasing, AntiAliasingMode, BoxBone, BoxModel, Camera, Light, ModelBox, PostProcess,
    Renderer, RendererSettings, Sky, VoxelArea, VoxelAreaRules, VoxelBiomes, VoxelCover, VoxelCoverKind, VoxelEdit, VoxelGenerator,
    VoxelGround, VoxelMaterial, VoxelPrint, VoxelPrintSettings, VoxelStamp, VoxelTrees, VoxelVolume, Voxels, WindZone,
};
use voxear_scene::Transform;
use voxear_terrain::blocks::plant::{PlantDef, PlantKind};
use voxear_terrain::generate::{
    Cells, Graph, Landmass, OwnVoxels, Preset, WorldGen,
    nodes::{NodeKind, Output, Rivers},
    starters,
};

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

/// Renders until every volume is made, then `frames` more.
fn render(renderer: &mut Renderer, world: &World, frames: u32) -> Vec<u8> {
    let mut time = 0.0;
    let started = std::time::Instant::now();
    loop {
        time += 1.0 / 60.0;
        renderer.render(world, time).expect("frame renders");
        if !renderer.voxel_volumes_making() {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "the volume is made within a minute");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
/// above `(8, ·, 8)`: +x to the right, +z up the screen.
fn from_above(height: f32, x: f32, z: f32) -> (u32, u32) {
    let half = (30f32).to_radians().tan() * height;
    let u = (x - 8.0) / (half * WIDTH as f32 / HEIGHT as f32);
    let v = (z - 8.0) / half;
    (((u + 1.0) * 0.5 * WIDTH as f32) as u32, ((1.0 - v) * 0.5 * HEIGHT as f32) as u32)
}

/// A red pillar of cubes, 1/8 m each, a metre square and three metres tall.
fn pillar() -> BoxModel {
    let mut model = BoxModel { pixels_per_block: 8.0, ..BoxModel::default() };
    let mut bone = BoxBone::new("root", None, Vec3::ZERO);
    bone.boxes.push(ModelBox {
        voxels: Some(Voxels::filled([8, 24, 8], 1)),
        ..ModelBox::new(Vec3::new(-4.0, 0.0, -4.0), [8, 24, 8], [0, 0])
    });
    model.bones.push(bone);
    model.palette.push([220, 30, 30, 255]);
    model
}

/// A volume 16 m square with flat bare ground a metre up and the pillar in
/// its middle, seen straight down from 20 m over the ground, the sun low in
/// the west.
fn field(ambient_occlusion: bool) -> World {
    let mut world = World::new();
    let ground = VoxelGround { height: 1.0, hills: 0.0, grass: 0.0, flowers: 0.0, ..VoxelGround::default() };
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 6.0, 16.0),
        ground: Some(ground),
        stamps: vec![VoxelStamp { model: Arc::new(pillar()), at: Vec3::new(8.0, 1.0, 8.0), turn: 0.0, scale: 1.0 }],
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(8.0, 21.0, 8.0), down, Vec3::ONE)));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let ao = AmbientOcclusion { enabled: ambient_occlusion, ..AmbientOcclusion::default() };
    world.insert_resource(PostProcess { anti_aliasing, ambient_occlusion: ao, ..PostProcess::neutral() });
    world
}

fn check_field(pixels: &[u8]) {
    let at = |x: f32, z: f32| {
        let (px, py) = from_above(20.0, x, z);
        rgb_at(pixels, px, py)
    };
    // The ground's top, lit: green.
    let [r, g, b] = at(3.0, 12.0);
    assert!(g > r && g > b && g > 50, "the ground: {r} {g} {b}");
    // The pillar's top: red.
    let [r, g, b] = at(8.0, 8.0);
    assert!(r > g * 2 && r > b * 2 && r > 60, "the pillar's top: {r} {g} {b}");
    // East of the pillar, in its traced shadow, darker than open ground the same distance west.
    let brightness = |x: f32, z: f32| at(x, z).iter().sum::<i32>();
    let (open, shadowed) = (brightness(4.0, 8.0), brightness(11.5, 8.0));
    assert!(shadowed * 3 < open * 2, "east of the pillar in its shadow: {shadowed} against {open}");
    // Off the volume: the sky, not green.
    let (px, py) = from_above(20.0, -6.0, 8.0);
    let [r, g, b] = rgb_at(pixels, px.min(WIDTH - 1), py);
    assert!(!(g > r + 20 && g > b + 20), "off the volume is not ground: {r} {g} {b}");
}

#[test]
fn a_volumes_ground_and_a_stamped_model_are_drawn_lit_and_the_model_shadows_the_ground() {
    let Some(mut renderer) = renderer() else { return };
    let world = field(false);
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel field.png", &pixels);
    check_field(&pixels);
    let counts = renderer.stats().voxels;
    assert_eq!(counts.volumes, 1);
    assert!(counts.bricks > 0 && counts.bytes > 0, "{counts:?}");
}

#[test]
fn a_volume_is_drawn_the_same_through_the_depth_pass_with_ambient_occlusion_on() {
    let Some(mut renderer) = renderer() else { return };
    let world = field(true);
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel field with ao.png", &pixels);
    check_field(&pixels);
}

#[test]
fn a_volume_changed_is_made_again_and_one_taken_away_is_drawn_no_more() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = field(false);
    render(&mut renderer, &world, 1);
    let entity = world.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap().0;
    // The pillar moved west: its top now red where the ground was.
    world.get_mut::<VoxelVolume>(entity).unwrap().stamps[0].at = Vec3::new(4.0, 1.0, 8.0);
    let pixels = render(&mut renderer, &world, 2);
    let (px, py) = from_above(20.0, 4.0, 8.0);
    let [r, g, _] = rgb_at(&pixels, px, py);
    assert!(r > g * 2, "the pillar moved: {r} {g}");
    world.remove::<VoxelVolume>(entity);
    let pixels = render(&mut renderer, &world, 2);
    let (px, py) = from_above(20.0, 3.0, 12.0);
    let [r, g, b] = rgb_at(&pixels, px, py);
    assert!(!(g > r + 20 && g > b + 20), "the volume is gone: {r} {g} {b}");
    assert_eq!(renderer.stats().voxels.volumes, 0);
}

/// Ground with grass and flowers seen straight down from 12 m over `(x, z)`,
/// on a volume at the world's middle: endless, or 16 m square.
fn ground_from_above(endless: bool, x: f32, z: f32) -> World {
    let mut world = World::new();
    let ground = VoxelGround { height: 2.0, hills: 1.0, grass: 0.6, flowers: 0.3, ..VoxelGround::default() };
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 8.0, 16.0),
        ground: Some(ground),
        endless,
        reach: 24.0,
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Camera::default(), Transform::new(Vec3::new(x, 14.0, z), down, Vec3::ONE)));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

#[test]
fn an_endless_volumes_ground_made_on_the_gpu_is_the_ground_a_volume_makes_on_the_cpu() {
    let Some(mut first) = renderer() else { return };
    let made_here = render(&mut first, &ground_from_above(false, 8.0, 8.0), 3);
    drop(first);
    let Some(mut second) = renderer() else { return };
    let endless = render(&mut second, &ground_from_above(true, 8.0, 8.0), 3);
    keep_image("voxel ground on the cpu.png", &made_here);
    keep_image("voxel ground on the gpu.png", &endless);
    // Over the middle of the 16 m square, well inside it: the same ground, flowers and shadows.
    let (mut far, mut count) = (0, 0);
    for y in HEIGHT / 4..HEIGHT * 3 / 4 {
        for x in WIDTH / 2 - HEIGHT / 4..WIDTH / 2 + HEIGHT / 4 {
            let (a, b) = (rgb_at(&made_here, x, y), rgb_at(&endless, x, y));
            far += usize::from((0..3).any(|i| (a[i] - b[i]).abs() > 24));
            count += 1;
        }
    }
    assert!(far * 200 < count, "{far} of {count} pixels differ by more than 24 levels");
    let [r, g, b] = rgb_at(&endless, WIDTH / 2, HEIGHT / 2);
    assert!(g > r && g > b, "grass under the camera: {r} {g} {b}");
}

#[test]
fn an_endless_volume_makes_its_ground_round_the_camera_wherever_it_goes() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = ground_from_above(true, 8.0, 8.0);
    render(&mut renderer, &world, 1);
    let camera = world.query_ref::<(voxear_ecs::Entity, &Camera)>().into_iter().next().unwrap().0;
    // Two hundred metres east and as far south of where it started, and below the world's middle.
    world.get_mut::<Transform>(camera).unwrap().position = Vec3::new(208.0, 14.0, -192.0);
    let pixels = render(&mut renderer, &world, 2);
    keep_image("voxel ground moved.png", &pixels);
    // The sky is a mid grey here; ground is green, flowers, or grass in its blades' shadow.
    let sky = (0..HEIGHT)
        .step_by(8)
        .flat_map(|y| (0..WIDTH).step_by(8).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b] = rgb_at(&pixels, x, y);
            (r - g).abs() < 10 && (g - b).abs() < 10 && (100..180).contains(&r)
        })
        .count();
    let all = (HEIGHT / 8 * WIDTH / 8) as usize;
    assert!(sky * 100 < all, "ground under all of the view: {sky} of {all} pixels are sky");
    assert!(renderer.stats().voxels.bytes > 0);
}

/// `world` with its Rivers node taken out, its ground wired straight into
/// its Output: the ground the GPU makes of it, which runs no rivers.
fn without_rivers(world: &WorldGen) -> WorldGen {
    let mut graph = world.graph().clone();
    let output = graph.output().expect("an output").id;
    let rivers = graph.link_into(output, Output::HEIGHT).expect("its ground").from;
    if let Some(ground) = graph.link_into(rivers, Rivers::GROUND).copied() {
        assert!(graph.connect(ground.from, ground.output, output, Output::HEIGHT));
        graph.disconnect(output, Output::WATER);
        graph.disconnect(output, Output::FALLS);
    }
    WorldGen::new(world.seed(), world.scale(), world.sea_level(), graph)
}

/// Where in `world` a river's water stands at least `deep` metres over its
/// bed, the nearest the middle of those looked at, every 8 m within 640 m:
/// the place, and how high its water and its bed are.
fn river_in(world: &WorldGen, deep: f32) -> ([f32; 2], f32, f32) {
    let (mut xs, mut zs) = (Vec::new(), Vec::new());
    for j in 0..160 {
        for i in 0..160 {
            xs.push(i as f32 * 8.0 - 640.0);
            zs.push(j as f32 * 8.0 - 640.0);
        }
    }
    let (water, _) = world.water(&xs, &zs).expect("its water wired in");
    let mut heights = vec![0.0; xs.len()];
    world.heights(&xs, &zs, &mut heights);
    let found = (0..xs.len())
        .filter(|&i| water[i] - heights[i] > deep)
        .min_by(|&a, &b| xs[a].hypot(zs[a]).total_cmp(&xs[b].hypot(zs[b])))
        .expect("a river deep enough");
    ([xs[found], zs[found]], water[found], heights[found])
}

#[test]
fn an_endless_volumes_ground_is_cut_by_its_generators_rivers_with_their_water_over_it() {
    let Some(mut renderer) = renderer() else { return };
    let rivered = Arc::new(starter("Rolling Hills"));
    let ([x, z], water, bed) = river_in(&rivered, 0.8);
    // Straight down from 8 m over the river's water, the volume's floor 24 m under its bed.
    let scene = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    let mut bare = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    for volume in bare.query::<&mut VoxelVolume>() {
        volume.generator.as_mut().unwrap().world = Arc::new(rivered.without_rivers());
    }
    let volume = |scene: &World| scene.query_ref::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let camera = scene.query_ref::<(&Camera, &Transform)>().into_iter().next().unwrap().1.position;
    // The ray worked out on the CPU meets the bed the rivers cut, as deep as the generator makes it.
    let down = -Vec3::Y;
    let hit = voxear_render::voxel_raycast(&volume(&scene), Vec3::ZERO, camera, down, 100.0).expect("the bed");
    let uncut = voxear_render::voxel_raycast(&volume(&bare), Vec3::ZERO, camera, down, 100.0).expect("the ground");
    eprintln!(
        "river at {x} {z}: water {water:.2}, bed {bed:.2}; met at {:.2}, uncut {:.2}",
        hit.point.y, uncut.point.y
    );
    assert!((hit.point.y - 24.0).abs() < 0.25, "the ray meets the bed at {:.2}, not 24", hit.point.y);
    assert!(uncut.point.y > hit.point.y + 0.5, "the ground without the rivers stands higher");
    let cut = render(&mut renderer, &scene, 3);
    let squares = renderer.stats().voxels.water_squares;
    keep_image("river from above.png", &cut);
    let uncut_pixels = render(&mut renderer, &bare, 3);
    keep_image("river taken out from above.png", &uncut_pixels);
    assert!(squares > 0, "water over the river");
    assert_eq!(renderer.stats().voxels.water_squares, 0, "no water without the rivers");
    let middle = |a: &[u8], b: &[u8]| {
        (0..40)
            .flat_map(|dy| (0..40).map(move |dx| (WIDTH / 2 + dx - 20, HEIGHT / 2 + dy - 20)))
            .filter(|&(x, y)| (0..3).any(|i| (rgb_at(a, x, y)[i] - rgb_at(b, x, y)[i]).abs() > 20))
            .count()
    };
    let changed = middle(&cut, &uncut_pixels);
    assert!(changed > 800, "the river shows under the camera: {changed} of 1600 pixels changed");
    // A ball of rock filled where the ray met the bed stands up out of the water under the camera.
    let mut filled = scene;
    filled.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits =
        Arc::new(vec![VoxelEdit::ball(hit.point, water - bed + 0.6, Some(VoxelMaterial::Rock))]);
    let rock = render(&mut renderer, &filled, 3);
    keep_image("river filled from above.png", &rock);
    let risen = middle(&cut, &rock);
    assert!(risen > 400, "the rock filled where the ray met the bed shows: {risen} of 1600 pixels changed");
}

fn starter(name: &str) -> WorldGen {
    let found = starters().into_iter().find(|starter| starter.name == name).expect("a starter of that name");
    found.world(7, 1.0)
}

#[test]
fn a_generators_ground_and_climate_on_the_gpu_are_the_cpus() {
    let Some(renderer) = renderer() else { return };
    let worlds = [
        ("Rolling Hills", without_rivers(&starter("Rolling Hills"))),
        ("Battlefield Plains", without_rivers(&starter("Battlefield Plains"))),
        ("Alpine", without_rivers(&starter("Alpine"))),
        ("Archipelago", without_rivers(&starter("Archipelago"))),
        ("Canyons", without_rivers(&starter("Canyons"))),
        ("an Alpine landmass", WorldGen::from(Landmass { seed: 3, ..Preset::Alpine.landmass() })),
    ];
    // A landmass's own gullies are not joined: each octave turns where the one before has a floor, so its ground
    // steps there, and a rounding either side of a floor lands on the other side of the step.
    let stepped = |name: &str| name.contains("landmass");
    // Places a voxel apart in a patch of ground, and others kilometres apart.
    let mut places: Vec<[f32; 2]> =
        (0..64 * 64).map(|i| [(i % 64) as f32 / 16.0 + 100.0, (i / 64) as f32 / 16.0 - 40.0]).collect();
    places.extend((0..4096).map(|i| [(i % 64) as f32 * 157.3 - 5000.0, (i / 64) as f32 * 149.1 - 4700.0]));
    for (name, world) in &worlds {
        let asked: Vec<[f32; 3]> = places.iter().map(|&[x, z]| [x, 0.0, z]).collect();
        let made = renderer.generated_on_gpu(world, &asked, 1.0 / 16.0).expect("the generator runs on the GPU");
        let xs: Vec<f32> = places.iter().map(|place| place[0]).collect();
        let zs: Vec<f32> = places.iter().map(|place| place[1]).collect();
        let mut heights = vec![0.0; places.len()];
        world.heights(&xs, &zs, &mut heights);
        let (mut warmth, mut wetness) = (vec![0.0; places.len()], vec![0.0; places.len()]);
        world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
        let (mut furthest, mut same, mut near, mut low, mut high) = (0.0f32, 0, 0, f32::MAX, f32::MIN);
        for (at, gpu) in made.iter().enumerate() {
            let apart = (gpu[0] - heights[at]).abs();
            furthest = furthest.max(apart);
            same += usize::from(gpu[0] == heights[at]);
            near += usize::from(apart < (heights[at].abs() * 1e-5).max(0.001));
            (low, high) = (low.min(heights[at]), high.max(heights[at]));
            assert!((gpu[1] - warmth[at]).abs() < 0.01, "{name}: {} °C on the GPU, {} on the CPU", gpu[1], warmth[at]);
            assert!(
                (gpu[2] - wetness[at]).abs() < 0.001,
                "{name}: {} wet on the GPU, {} on the CPU",
                gpu[2],
                wetness[at]
            );
        }
        eprintln!(
            "{name}: heights {low:.1} to {high:.1} m, {same} of {} the same to the bit, {near} within a millimetre, at most {furthest} m apart",
            places.len()
        );
        assert!(high - low > 5.0, "{name}: the ground rises and falls");
        // Each rounding the GPU does its own way moves a height by a hair; a terrace or a steep curve can make a
        // centimetre of it, a sixth of a voxel at 16 a metre.
        let (most, share) = if stepped(name) { (1.0, 970) } else { (0.02, 985) };
        assert!(furthest < most, "{name}: the GPU's heights are as much as {furthest} m from the CPU's");
        assert!(
            near * 1000 >= places.len() * share,
            "{name}: {near} of {} heights within a millimetre, or a hundred-thousandth of their height",
            places.len()
        );
    }
}

/// A generator whose ground is output `port` of a node of `kind`, with
/// output 0 of a node of each of `sources` wired into its inputs in order.
fn node_world(kind: NodeKind, port: usize, sources: &[NodeKind]) -> WorldGen {
    let mut graph = Graph::default();
    let sources: Vec<_> = sources.iter().map(|source| graph.add(source.clone(), [0.0, 0.0])).collect();
    let node = graph.add(kind, [200.0, 0.0]);
    for (input, source) in sources.iter().enumerate() {
        assert!(graph.connect(*source, 0, node, input), "input {input} takes a field");
    }
    let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
    assert!(graph.connect(node, port, output, Output::HEIGHT), "output {port} is a field");
    WorldGen::new(5, 1.0, 0.0, graph)
}

#[test]
fn peaks_plateaus_smoothing_world_edges_and_painted_maps_on_the_gpu_are_the_cpus() {
    use voxear_terrain::generate::{
        nodes::{EdgeShape, Noise, PaintedMapKind, PaintedMapNode, Peaks, Plateaus, Smooth, WorldEdge},
        painted::{Painted, PaintedMaps},
    };
    let Some(renderer) = renderer() else { return };
    // Hills painted high round (100, 100), land painted low along a strip, a biome painted in a patch.
    let painted = Painted::new(9, PaintedMaps::default());
    painted.change(|maps| {
        maps.height.paint([100.0, 100.0], 300.0, 0.5, &|_, cover| Some(cover * 120.0));
        maps.land.paint([-400.0, 50.0], 250.0, 0.2, &|_, cover| Some(1.0 - cover));
        maps.biome.paint([0.0, -200.0], 180.0, 0.0, &|_, _| Some(7.0));
    });
    let read = |map: PaintedMapKind, default: f32| {
        NodeKind::PaintedMap(PaintedMapNode { map, default, painted: Some(painted.clone()) })
    };
    let rough = NodeKind::Noise(Noise { size: 30.0, low: -20.0, high: 20.0, ..Noise::default() });
    let plateaus = Plateaus { river: 4.0, canyon: 30.0, ramps: 0.6, rim: 1.5, wander: 0.6, ..Plateaus::default() };
    let worlds = [
        ("peaks", node_world(NodeKind::Peaks(Peaks::default()), Peaks::HEIGHT, &[])),
        (
            "peaks' crests",
            node_world(NodeKind::Peaks(Peaks { sharpness: 0.8, ..Peaks::default() }), Peaks::CRESTS, &[]),
        ),
        ("plateaus", node_world(NodeKind::Plateaus(plateaus.clone()), Plateaus::HEIGHT, &[])),
        ("plateaus' walls", node_world(NodeKind::Plateaus(plateaus.clone()), Plateaus::WALLS, &[])),
        ("plateaus' floor", node_world(NodeKind::Plateaus(Plateaus { rim: -2.0, ..plateaus }), Plateaus::FLOOR, &[])),
        ("smoothed noise", node_world(NodeKind::Smooth(Smooth { radius: 12.0 }), 0, &[rough.clone()])),
        (
            "a sea past the edge",
            node_world(NodeKind::WorldEdge(WorldEdge { width: 120.0, ..WorldEdge::default() }), 0, &[rough.clone()]),
        ),
        (
            "cliffs past the edge",
            node_world(
                NodeKind::WorldEdge(WorldEdge { shape: EdgeShape::Cliffs, level: 160.0, ..WorldEdge::default() }),
                0,
                &[rough.clone()],
            ),
        ),
        ("inside the edge", node_world(NodeKind::WorldEdge(WorldEdge::default()), 1, &[])),
        ("a painted height", node_world(read(PaintedMapKind::Height, -3.0), 0, &[])),
        ("a painted land", node_world(read(PaintedMapKind::Land, 0.5), 0, &[])),
        ("a painted biome", node_world(read(PaintedMapKind::Biome, -1.0), 0, &[])),
    ];
    // Places a voxel apart in a patch of ground, others a few metres apart over a kilometre, and others kilometres
    // apart.
    let mut places: Vec<[f32; 2]> =
        (0..64 * 64).map(|i| [(i % 64) as f32 / 16.0 + 100.0, (i / 64) as f32 / 16.0 - 40.0]).collect();
    places.extend((0..64 * 64).map(|i| [(i % 64) as f32 * 17.3 - 600.0, (i / 64) as f32 * 16.1 - 500.0]));
    places.extend((0..4096).map(|i| [(i % 64) as f32 * 157.3 - 5000.0, (i / 64) as f32 * 149.1 - 4700.0]));
    let xs: Vec<f32> = places.iter().map(|place| place[0]).collect();
    let zs: Vec<f32> = places.iter().map(|place| place[1]).collect();
    let asked: Vec<[f32; 3]> = places.iter().map(|&[x, z]| [x, 0.0, z]).collect();
    let mut failed = Vec::new();
    for (name, world) in &worlds {
        let made = renderer.generated_on_gpu(world, &asked, 1.0 / 16.0).expect("the generator runs on the GPU");
        let mut heights = vec![0.0; places.len()];
        world.heights(&xs, &zs, &mut heights);
        let (mut furthest, mut near, mut far, mut low, mut high) = (0.0f32, 0, 0, f32::MAX, f32::MIN);
        for (at, gpu) in made.iter().enumerate() {
            let apart = (gpu[0] - heights[at]).abs();
            furthest = furthest.max(apart);
            near += usize::from(apart < (heights[at].abs() * 1e-5).max(0.001));
            far += usize::from(apart > 0.05);
            (low, high) = (low.min(heights[at]), high.max(heights[at]));
        }
        eprintln!(
            "{name}: {low:.2} to {high:.2}, {near} of {} within a millimetre, {far} over 5 cm apart, at most {furthest}",
            places.len()
        );
        // Each rounding the GPU does its own way moves a height by a hair, more on heights of a kilometre; a wall's
        // foot, a stair's tread or a summit's cell lands on the other side of its line where a rounding differs, a
        // few places in ten thousand.
        if high <= low || near * 1000 < places.len() * 980 || far * 1000 > places.len() {
            failed.push(*name);
        }
    }
    assert!(failed.is_empty(), "not the CPU's on the GPU: {failed:?}");
}

#[test]
fn the_biome_an_endless_volume_picks_on_the_gpu_is_the_world_of_blocks_on_the_cpu() {
    use voxear_terrain::generate::{
        BiomeBlocks, BiomeDef, Biomes, WorldBiomes,
        biomes::Place,
        painted::{Painted, PaintedMaps},
    };
    let Some(renderer) = renderer() else { return };
    let world = without_rivers(&starter("Rolling Hills"));
    // Four biomes by warmth and wetness, one keeping to the hilltops, and the second painted over a patch.
    let biome = |temperature: [f32; 2], humidity: [f32; 2], height: Option<[f32; 2]>| BiomeDef {
        temperature,
        humidity,
        height,
        ..BiomeDef::default()
    };
    let list = vec![
        biome([-30.0, 5.0], [0.0, 1.0], None),
        biome([5.0, 15.0], [0.0, 0.5], None),
        biome([5.0, 15.0], [0.5, 1.0], None),
        biome([15.0, 40.0], [0.0, 1.0], None),
        biome([-30.0, 40.0], [0.0, 1.0], Some([60.0, 400.0])),
    ];
    let painted = Painted::new(11, PaintedMaps::default());
    painted.change(|maps| maps.biome.paint([300.0, -200.0], 200.0, 0.0, &|_, _| Some(1.0)));
    let biomes = WorldBiomes {
        biomes: Biomes::new(list, 12.0, 0.4, 7),
        blocks: vec![BiomeBlocks::default(); 5],
        regions: None,
        painted: Some(painted),
    };
    let gpu = biomes.gpu([1, 2, 3], 3, 1.0);
    let voxel_biomes = VoxelBiomes { colours: vec![[100; 3]; gpu.blocks.len()], words: gpu.words };
    let places: Vec<[f32; 2]> =
        (0..8192).map(|i| [(i % 128) as f32 * 9.7 - 300.0, (i / 128) as f32 * 11.3 - 500.0]).collect();
    let picked = renderer.biomes_on_gpu(&world, &voxel_biomes, &places).expect("biomes on the GPU");
    let (mut same, mut twins, mut seen) = (0, 0, [0usize; 5]);
    for (at, &[x, z]) in places.iter().enumerate() {
        let height = world.height_at(x, z);
        let (temperature, humidity) = world.climate(x, z, height);
        let place = Place { temperature, humidity, above_sea: height - world.sea_level(), sunk: 0.0 };
        let cpu = biomes.index_at(x, z, place).map_or(-1, |index| index as i32);
        same += usize::from(cpu == picked[at]);
        // And the CPU's own twin of the GPU's, as collision and rays place trees by.
        let twin = voxel_biomes.biome_of(x, z, temperature, humidity, height - world.sea_level());
        twins += usize::from(twin.map_or(-1, |index| index as i32) == cpu);
        if let Ok(index) = usize::try_from(cpu) {
            seen[index] += 1;
        }
    }
    eprintln!("{same} of {} the same, {twins} on the CPU's twin; the CPU's biomes {seen:?}", places.len());
    assert!(twins * 1000 >= places.len() * 995, "{twins} of {} the same on the CPU's twin", places.len());
    assert!(seen.iter().filter(|&&count| count > 0).count() >= 3, "several biomes: {seen:?}");
    // A place on a border may fall either side of it by a rounding.
    assert!(same * 1000 >= places.len() * 995, "{same} of {} the same", places.len());
}

/// An endless volume made by a starter generator, seen from `height` metres
/// over its ground at `(x, z)` and looking down at `pitch` degrees.
fn generated_view(world: Arc<WorldGen>, x: f32, z: f32, height: f32, pitch: f32) -> World {
    let ground = world.height_at(x, z);
    let mut scene = World::new();
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 48.0, 16.0),
        ground: Some(VoxelGround { grass: 0.5, flowers: 0.2, ..VoxelGround::default() }),
        endless: true,
        reach: 48.0,
        generator: Some(VoxelGenerator { world, floor: ground - 24.0, at: [0.0, 0.0], biomes: None }),
        ..VoxelVolume::default()
    };
    scene.spawn((volume, Transform::default()));
    let look = Quat::from_rotation_x(pitch.to_radians());
    scene.spawn((Camera::default(), Transform::new(Vec3::new(x, 24.0 + height, z), look, Vec3::ONE)));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.3).normalize();
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    scene.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    scene.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    scene.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    scene
}

#[test]
fn the_water_of_an_endless_volumes_rivers_is_found_where_it_is_drawn_with_its_current() {
    let rivered = Arc::new(starter("Rolling Hills"));
    let ([x, z], water, bed) = river_in(&rivered, 0.8);
    let scene = generated_view(rivered.clone(), x, z, 8.0, 90.0);
    let volume = scene.query_ref::<&VoxelVolume>().into_iter().next().unwrap().clone();
    // The volume's floor is 24 m under the river's bed.
    let (top, bottom) = (24.0 + water - bed, 24.0);
    let at = |y: f32| voxear_render::voxel_water_at(&volume, Vec3::ZERO, Vec3::new(x, y, z));
    let found = at(bottom + 0.1).expect("the river's water");
    assert!((found.height - top).abs() < 0.15, "its surface at {:.2}, not {top:.2}", found.height);
    // Its current runs as the generator's does there, worked out from the points its water is drawn from.
    let [cx, cz] = rivered.current(x, z).expect("its current");
    let current = Vec3::new(cx, 0.0, cz);
    eprintln!("river at {x} {z}: surface {:.2} of {top:.2}, current {} of {current}", found.height, found.current);
    if current.length() > 0.5 {
        assert!(found.current.normalize().dot(current.normalize()) > 0.8, "{} along {current}", found.current);
    }
    assert!((found.current.length() - current.length()).abs() < 0.6, "{} as fast as {current}", found.current);
    // Between the points, read smoothly from them, as near the generator's own water.
    let (bx, bz) = (x + 0.23, z + 0.31);
    let (own, _) = rivered.water(&[bx], &[bz]).unwrap();
    let between = voxear_render::voxel_water_at(&volume, Vec3::ZERO, Vec3::new(bx, bottom + 0.1, bz)).unwrap();
    assert!((between.height - (24.0 + own[0] - bed)).abs() < 0.1, "{} between the points", between.height);
    // Over it the same surface; deep under its bed, none; nor on a hill far from water, nor under the floor.
    assert_eq!(at(top + 20.0).map(|water| water.height), Some(found.height));
    assert!(at(bottom - 3.0).is_none(), "no water 3 m under the bed");
    assert!(voxear_render::voxel_water_at(&volume, Vec3::ZERO, Vec3::new(x, -1.0, z)).is_none());
    let hill = (0..4000)
        .map(|i| [(i % 64) as f32 * 9.0 - 300.0, (i / 64) as f32 * 9.0 - 300.0])
        .max_by(|a, b| rivered.height_at(a[0], a[1]).total_cmp(&rivered.height_at(b[0], b[1])))
        .unwrap();
    let up = Vec3::new(hill[0], 24.0 + rivered.height_at(hill[0], hill[1]) - bed + 1.0, hill[1]);
    assert!(voxear_render::voxel_water_at(&volume, Vec3::ZERO, up).is_none(), "no water on the hilltop at {hill:?}");
    // Without rivers, none.
    let mut bare = volume.clone();
    bare.generator.as_mut().unwrap().world = Arc::new(rivered.without_rivers());
    assert!(voxear_render::voxel_water_at(&bare, Vec3::ZERO, Vec3::new(x, bottom + 0.1, z)).is_none());
}

#[test]
fn an_endless_volume_has_its_generators_sea_where_the_level_has_no_ocean() {
    let Some(mut renderer) = renderer() else { return };
    let islands = Arc::new(starter("Archipelago"));
    let sea = islands.sea_level();
    // A shore with the sea 3 m deep 24 m off towards +x, seen from 3 m over the sea, looking out across it.
    let coast = (0..160_000)
        .map(|i| [(i % 400) as f32 * 4.0 - 800.0, (i / 400) as f32 * 4.0 - 800.0])
        .find(|&[x, z]| (islands.height_at(x, z) - sea).abs() < 0.5 && islands.height_at(x + 24.0, z) < sea - 3.0)
        .expect("a shore");
    let from = [coast[0] - 6.0, coast[1]];
    let mut scene =
        generated_view(islands.clone(), from[0], from[1], sea - islands.height_at(from[0], from[1]) + 3.0, 12.0);
    let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
    camera.rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(12f32.to_radians());
    let (volume, floor) = {
        let (entity, volume) = scene.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap();
        (entity, volume.generator.as_ref().unwrap().floor)
    };
    let seas = voxear_render::ocean::seas(&scene);
    assert_eq!(seas.len(), 1, "the volume's sea");
    assert_eq!((seas[0].entity, seas[0].position.y), (volume, sea - floor));
    let with_sea = render(&mut renderer, &scene, 3);
    keep_image("generated sea.png", &with_sea);
    scene.get_mut::<VoxelVolume>(volume).unwrap().sea = false;
    assert!(voxear_render::ocean::seas(&scene).is_empty());
    let without = render(&mut renderer, &scene, 3);
    keep_image("generated sea turned off.png", &without);
    // Out across the water, under the horizon: the sea's water where there was its floor.
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let (mut changed, mut bluer, mut all) = (0, 0, 0);
    for y in h * 55 / 100..h * 70 / 100 {
        for x in w / 4..w * 3 / 4 {
            let at = (y * w + x) * 4;
            let (a, b) = (&with_sea[at..at + 3], &without[at..at + 3]);
            all += 1;
            changed += usize::from(a.iter().zip(b).map(|(a, b)| a.abs_diff(*b) as u32).sum::<u32>() > 24);
            bluer += usize::from(i32::from(a[2]) - i32::from(a[0]) > i32::from(b[2]) - i32::from(b[0]));
        }
    }
    eprintln!("out to sea: {changed} of {all} pixels changed, {bluer} bluer");
    assert!(changed * 2 > all, "the sea shows: {changed} of {all} pixels changed");
    assert!(bluer * 2 > all, "the sea's water is bluer than its floor: {bluer} of {all}");
}

#[test]
fn an_endless_volume_made_by_a_starter_generator_shows_its_ground() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(starter("Rolling Hills"));
    // Straight down from 14 m: the generator's ground under all of the view, grass on it.
    let pixels = render(&mut renderer, &generated_view(hills.clone(), 8.0, 8.0, 14.0, 90.0), 3);
    keep_image("generated ground from above.png", &pixels);
    let (mut green, mut sky, mut all) = (0, 0, 0);
    for y in (0..HEIGHT).step_by(4) {
        for x in (0..WIDTH).step_by(4) {
            let [r, g, b] = rgb_at(&pixels, x, y);
            green += usize::from(g > r + 8 && g > b + 8);
            sky += usize::from((r - g).abs() < 10 && (g - b).abs() < 10 && (100..180).contains(&r));
            all += 1;
        }
    }
    assert!(sky * 100 < all, "ground under all of the view: {sky} of {all} are sky");
    assert!(green * 2 > all, "grass over most of it: {green} of {all} green");
    assert!(renderer.stats().voxels.bytes > 0);
    // Across the hills from 3 m over the ground: ground to the horizon of the reach, and sky over it.
    let pixels = render(&mut renderer, &generated_view(hills, 8.0, 8.0, 3.0, 12.0), 3);
    keep_image("generated ground across.png", &pixels);
    let [r, g, b] = rgb_at(&pixels, WIDTH / 2, HEIGHT - 10);
    assert!(g > r && g > b, "grass at the foot of the view: {r} {g} {b}");
    // Alpine where its ground is steep: rock on the slopes, grey, among the grass.
    let alpine = Arc::new(starter("Alpine"));
    let steep = (0..4000)
        .map(|i| [(i % 64) as f32 * 23.0 + 300.0, (i / 64) as f32 * 23.0 + 300.0])
        .find(|&[x, z]| (alpine.height_at(x + 2.0, z) - alpine.height_at(x - 2.0, z)).abs() > 5.0)
        .expect("steep ground in the mountains");
    let pixels = render(&mut renderer, &generated_view(alpine, steep[0], steep[1], 10.0, 60.0), 3);
    keep_image("generated mountainside.png", &pixels);
    let grey = (0..HEIGHT)
        .step_by(4)
        .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b] = rgb_at(&pixels, x, y);
            (r - g).abs() < 12 && (g - b).abs() < 12 && r > 40 && r < 200
        })
        .count();
    assert!(grey * 10 > 14400, "rock on the mountainside: {grey} of 14400 grey");
}

/// Measures an endless volume made by a generator against one of its own
/// ground, at 1920×1080: the window made before the frames, the columns
/// made while flying, and a still view's passes, the least of 60 frames.
/// Run by hand: `cargo test -p voxear_render --test voxels measure -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_an_endless_volume_made_by_a_generator() {
    let Ok(mut renderer) = Renderer::new_offscreen(1920, 1080, &RendererSettings::default()) else { return };
    let hills = Arc::new(starter("Rolling Hills"));
    let caved = Arc::new(caved_alpine());
    let cases = [
        (false, hills.clone(), false),
        (true, hills.clone(), false),
        (true, hills.clone(), true),
        (true, caved, false),
    ];
    for (generated, world, trees) in cases {
        let caves = world.volume().is_some();
        let mut scene = generated_view(world, 8.0, 8.0, 2.0, 8.0);
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.reach = 64.0;
            if trees {
                volume.trees = oaks_and_pines();
            }
            if !generated {
                volume.generator = None;
                volume.size.y = 16.0;
            }
        }
        if !generated {
            let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
            camera.position.y = 6.0;
        }
        let name = match (generated, caves, trees) {
            (false, _, _) => "its own ground",
            (true, false, false) => "Rolling Hills",
            (true, false, true) => "Rolling Hills with oaks and pines",
            (true, true, _) => "Alpine with overhangs and caves",
        };
        let started = std::time::Instant::now();
        renderer.render(&scene, 0.0).unwrap();
        assert!(renderer.make_voxel_volumes(&scene, std::time::Duration::from_secs(60)).unwrap());
        let window = started.elapsed().as_secs_f32();
        // A still view: the least of 60 frames for each voxel pass and the whole frame.
        let mut least: std::collections::BTreeMap<String, f32> = Default::default();
        let mut frame = f32::MAX;
        for i in 0..90 {
            renderer.render(&scene, i as f32 / 60.0).unwrap();
            if i >= 30 {
                for (pass, ms) in renderer.timings() {
                    if pass.contains("oxel") {
                        let kept = least.entry(pass.clone()).or_insert(f32::MAX);
                        *kept = kept.min(*ms);
                    }
                }
                frame = frame.min(renderer.gpu_frame_ms());
            }
        }
        eprintln!("{name}: window made in {window:.2} s, {} MB", renderer.stats().voxels.bytes / 1_000_000);
        eprintln!("{name}: still view, least of 60: frame {frame:.2} ms, {least:?}");
        // Flying along x at 10 m/s for nine seconds: the making pass on the frames that make columns.
        let mut making = Vec::new();
        for i in 0..540 {
            let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
            camera.position.x += 10.0 / 60.0;
            renderer.render(&scene, 2.0 + i as f32 / 60.0).unwrap();
            if let Some((_, ms)) = renderer.timings().iter().find(|(pass, _)| pass == "Voxel world making") {
                making.push(*ms);
            }
        }
        making.sort_by(f32::total_cmp);
        let most = match (caves, trees) {
            (true, _) => 4,
            (false, true) => 8,
            (false, false) => 16,
        };
        if !making.is_empty() {
            let median = making[making.len() / 2];
            eprintln!(
                "{name}: flying, making on {} of 540 frames: median {median:.2} ms, least {:.2}, most {:.2}; {:.3} ms a column at {} a frame",
                making.len(),
                making[0],
                making[making.len() - 1],
                median / most as f32,
                most
            );
        }
    }
}

/// Measures what growing crops and keeping trees to World Areas costs, at
/// 1920×1080 on Rolling Hills with oaks and pines: with neither; with the
/// trees kept to a wood and the town kept clear among 24 areas of eight
/// corners each; and with wheat, cabbages and meadow grass as well. Each
/// still view's passes are the least of 60 frames; making is the least and
/// median of the frames making columns while flying at 10 m/s.
/// Run by hand: `cargo test -p voxear_render --test voxels measure_growth -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_growth_crops_and_areas() {
    let Ok(mut renderer) = Renderer::new_offscreen(1920, 1080, &RendererSettings::default()) else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let (wood, town, fields) = (41u128, 42u128, 43u128);
    // 24 octagons 30 m across in a ring of rings round the view, of each tag in turn.
    let areas: Vec<VoxelArea> = (0..24)
        .map(|i| {
            let (ring, turn) = (20.0 + (i / 8) as f32 * 35.0, i as f32 * 0.785);
            let middle = [8.0 + ring * turn.cos(), 8.0 + ring * turn.sin()];
            let outline =
                (0..8).map(|k| k as f32 * 0.785).map(|a| [middle[0] + 15.0 * a.cos(), middle[1] + 15.0 * a.sin()]).collect();
            VoxelArea { outline, tags: vec![[wood, town, fields][i % 3]], ..VoxelArea::default() }
        })
        .collect();
    let within = |tag: u128| VoxelAreaRules { within: vec![tag], never: Vec::new() };
    let cases = ["no areas or cover", "trees kept to areas", "trees kept to areas, and cover"];
    for (case, name) in cases.iter().enumerate() {
        let mut scene = generated_view(hills.clone(), 8.0, 8.0, 2.0, 8.0);
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.reach = 64.0;
            volume.trees = oaks_and_pines();
            if case >= 1 {
                volume.trees[0].areas = within(wood);
                volume.trees[1].areas = VoxelAreaRules { within: Vec::new(), never: vec![town] };
                volume.areas = Arc::new(areas.clone());
            }
            if case >= 2 {
                let mut wheat = VoxelCover::of(VoxelCoverKind::Wheat);
                wheat.areas = within(fields);
                let mut cabbages = VoxelCover::of(VoxelCoverKind::Cabbage);
                cabbages.areas = within(town);
                volume.cover = vec![wheat, cabbages, VoxelCover::of(VoxelCoverKind::MeadowGrass)];
            }
        }
        let started = std::time::Instant::now();
        renderer.render(&scene, 0.0).unwrap();
        assert!(renderer.make_voxel_volumes(&scene, std::time::Duration::from_secs(60)).unwrap());
        let window = started.elapsed().as_secs_f32();
        let mut least: std::collections::BTreeMap<String, f32> = Default::default();
        let mut frame = f32::MAX;
        for i in 0..90 {
            renderer.render(&scene, i as f32 / 60.0).unwrap();
            if i >= 30 {
                for (pass, ms) in renderer.timings() {
                    if pass.contains("oxel") {
                        let kept = least.entry(pass.clone()).or_insert(f32::MAX);
                        *kept = kept.min(*ms);
                    }
                }
                frame = frame.min(renderer.gpu_frame_ms());
            }
        }
        eprintln!("{name}: window made in {window:.2} s, {} MB", renderer.stats().voxels.bytes / 1_000_000);
        eprintln!("{name}: still view, least of 60: frame {frame:.2} ms, {least:?}");
        let mut making = Vec::new();
        for i in 0..540 {
            let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
            camera.position.x += 10.0 / 60.0;
            renderer.render(&scene, 2.0 + i as f32 / 60.0).unwrap();
            if let Some((_, ms)) = renderer.timings().iter().find(|(pass, _)| pass == "Voxel world making") {
                making.push(*ms);
            }
        }
        making.sort_by(f32::total_cmp);
        if !making.is_empty() {
            let median = making[making.len() / 2];
            eprintln!("{name}: flying, making on {} of 540 frames: least {:.2} ms, median {median:.2} ms", making.len(), making[0]);
        }
    }
    // The CPU's check of a tree's foot against the areas: a wood's worth of rays down through them.
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 48.0, 16.0),
        endless: true,
        trees: {
            let mut trees = oaks_and_pines();
            trees[0].areas = within(wood);
            trees
        },
        areas: Arc::new(areas),
        generator: Some(VoxelGenerator {
            world: hills.clone(),
            floor: hills.height_at(8.0, 8.0) - 24.0,
            at: [0.0, 0.0],
            biomes: None,
        }),
        ..VoxelVolume::default()
    };
    let bare = VoxelVolume { areas: Arc::default(), trees: oaks_and_pines(), ..volume.clone() };
    for (name, volume) in [("trees anywhere", &bare), ("trees kept to areas", &volume)] {
        let started = std::time::Instant::now();
        let mut met = 0;
        for i in 0..400 {
            let at = Vec3::new(-40.0 + (i % 20) as f32 * 5.0, 47.0, -40.0 + (i / 20) as f32 * 5.0);
            met += usize::from(voxear_render::voxel_raycast(volume, Vec3::ZERO, at, Vec3::NEG_Y, 60.0).is_some());
        }
        eprintln!("CPU, {name}: 400 rays down in {:?} ({met} met)", started.elapsed());
    }
}

/// Pictures and timings of rivers and a coast in endless volumes, at
/// 1920×1080, for comparing by eye: a river with its rivers cut and drawn
/// and one without, and a coast. Each still view's passes are the least of
/// 60 frames; a column's making is the least of the frames making the
/// window over again, each making as many columns as a frame makes at most.
/// Run by hand: `cargo test -p voxear_render --test voxels rivers_and_a_coast -- --ignored --nocapture`,
/// with `VOXEAR_TEST_OUTPUT` naming a folder for the pictures.
#[test]
#[ignore]
fn measure_rivers_and_a_coast() {
    let (w, h) = (1920, 1080);
    let Ok(mut renderer) = Renderer::new_offscreen(w, h, &RendererSettings::default()) else { return };
    let keep = |name: &str, pixels: &[u8]| {
        if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(pixels).unwrap();
        }
    };
    let hills = Arc::new(starter("Rolling Hills"));
    let ([x, z], water, bed) = river_in(&hills, 1.2);
    let islands = Arc::new(starter("Archipelago"));
    let sea = islands.sea_level();
    // A shore: ground within half a metre of the sea, with the sea 3 m deep 24 m off towards +x.
    let coast = (0..160_000)
        .map(|i| [(i % 400) as f32 * 4.0 - 800.0, (i / 400) as f32 * 4.0 - 800.0])
        .find(|&[x, z]| (islands.height_at(x, z) - sea).abs() < 0.5 && islands.height_at(x + 24.0, z) < sea - 3.0)
        .expect("a shore");
    // A waterfall in the mountains, more than 4 m high, found along X: its place, its top and its foot, and which way
    // along X its water runs on below it.
    let alpine = Arc::new(starter("Alpine"));
    let fall = {
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..400 * 400).map(|at| ((at % 400) as f32 * 6.0 - 1200.0, (at / 400) as f32 * 6.0 - 1200.0)).unzip();
        let (level, _) = alpine.water(&xs, &zs).unwrap();
        let wet = |at: usize| level[at] > -1000.0;
        (0..xs.len())
            .filter(|&at| wet(at) && at % 400 + 1 < 400 && wet(at + 1))
            .find_map(|at| {
                let line: (Vec<f32>, Vec<f32>) = (0..=12).map(|k| (xs[at] + k as f32 * 0.5, zs[at])).unzip();
                let (top, falls) = alpine.water(&line.0, &line.1).unwrap();
                let k = (0..line.0.len()).find(|&k| falls[k] > 4.0)?;
                let (near, _) = alpine.water(&[line.0[k] - 3.0, line.0[k] + 3.0], &[line.1[k]; 2]).unwrap();
                Some((line.0[k], line.1[k], top[k], top[k] - falls[k], if near[1] < near[0] { 1.0 } else { -1.0 }))
            })
            .expect("a waterfall")
    };
    let which = std::env::var("RIVER_CASES").unwrap_or_else(|_| "river,bare,trees,coast".into());
    for case in which.split(',') {
        let mut scene = match case {
            "coast" => {
                // From 3 m over the sea at the shore, looking out across it.
                let mut scene = generated_view(
                    islands.clone(),
                    coast[0] - 6.0,
                    coast[1],
                    sea - islands.height_at(coast[0] - 6.0, coast[1]) + 3.0,
                    12.0,
                );
                // Its sea is the generator's, at its sea level.
                let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
                camera.rotation =
                    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(12f32.to_radians());
                scene
            }
            "falls" => {
                // From 18 m downstream at 3 m over its pool, looking back up at it.
                let (fx, fz, top, foot, down) = fall;
                let x = fx + down * 18.0;
                eprintln!("falls at {fx} {fz}: from {top:.1} to {foot:.1}");
                let mut scene = generated_view(alpine.clone(), x, fz, foot + 3.0 - alpine.height_at(x, fz), -8.0);
                let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
                let turn = if down > 0.0 { -std::f32::consts::FRAC_PI_2 } else { std::f32::consts::FRAC_PI_2 };
                camera.rotation = Quat::from_rotation_y(turn) * Quat::from_rotation_x(-8f32.to_radians());
                scene
            }
            "falls-side" => {
                // From 12 m off to its side and downstream, 4 m over its pool, looking at its middle.
                let (fx, fz, top, foot, down) = fall;
                let (x, z) = (fx + down * 9.0, fz + 9.0);
                let mut scene = generated_view(alpine.clone(), x, z, 0.0, 0.0);
                let floor =
                    scene.query_ref::<&VoxelVolume>().into_iter().next().unwrap().generator.as_ref().unwrap().floor;
                let eye = Vec3::new(x, foot + 4.0 - floor, z);
                let to = Vec3::new(fx, (top + foot) * 0.5 - floor, fz) - eye;
                let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
                camera.position = eye;
                camera.rotation =
                    Quat::from_rotation_y(to.x.atan2(to.z)) * Quat::from_rotation_x((-to.y).atan2(to.x.hypot(to.z)));
                scene
            }
            // From 6 m over the river's water, looking 35° down.
            _ => generated_view(hills.clone(), x, z, water - bed + 6.0, 35.0),
        };
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.reach = 64.0;
            if case == "bare" {
                volume.generator.as_mut().unwrap().world = Arc::new(hills.without_rivers());
            }
            if case == "trees" {
                volume.trees = oaks_and_pines();
            }
        }
        let started = std::time::Instant::now();
        renderer.render(&scene, 0.0).unwrap();
        assert!(renderer.make_voxel_volumes(&scene, std::time::Duration::from_secs(120)).unwrap());
        let window = started.elapsed().as_secs_f32();
        let mut least: std::collections::BTreeMap<String, f32> = Default::default();
        let mut frame = f32::MAX;
        for i in 0..90 {
            renderer.render(&scene, i as f32 / 60.0).unwrap();
            if i >= 30 {
                for (pass, ms) in renderer.timings() {
                    if pass.contains("oxel") || pass.contains("Water") || pass.contains("Sea floor") {
                        let kept = least.entry(pass.clone()).or_insert(f32::MAX);
                        *kept = kept.min(*ms);
                    }
                }
                frame = frame.min(renderer.gpu_frame_ms());
            }
        }
        keep(&format!("{case}.png"), &renderer.read_pixels().unwrap().2);
        let stats = renderer.stats().voxels;
        eprintln!(
            "{case}: window made in {window:.2} s, {} MB, {} squares of water",
            stats.bytes / 1_000_000,
            stats.water_squares
        );
        eprintln!("{case}: still view, least of 60: frame {frame:.2} ms, {least:?}");
        // The window made over again, as a changed floor makes it: each frame's making over its columns.
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.generator.as_mut().unwrap().floor += 1e-3;
        }
        let mut each = Vec::new();
        let mut columns = Vec::new();
        for i in 0..200 {
            renderer.render(&scene, 2.0 + i as f32 / 60.0).unwrap();
            columns.push(renderer.stats().voxels.columns_made);
            if let Some((_, ms)) = renderer.timings().iter().find(|(pass, _)| pass == "Voxel world making") {
                each.push(*ms);
            }
            if !renderer.voxel_volumes_making() && i > 10 {
                break;
            }
        }
        let most = columns.iter().copied().max().unwrap_or(1).max(1);
        // Only the frames well inside the run, where every frame made as many columns as it may.
        let full = columns.iter().filter(|&&c| c == most).count();
        let mut kept: Vec<f32> = each.iter().skip(4).take(full.saturating_sub(8)).copied().collect();
        kept.sort_by(f32::total_cmp);
        if !kept.is_empty() {
            eprintln!(
                "{case}: making again, {} frames of {most} columns: a column least {:.3} ms, median {:.3} ms",
                kept.len(),
                kept[0] / most as f32,
                kept[kept.len() / 2] / most as f32
            );
        }
    }
}

/// A volume's generator changed under it, as dragging a field in the
/// Inspector does, makes its ground again in the buffers it has: what it
/// shows then is what a volume made with that generator from the first
/// shows.
#[test]
fn an_endless_volume_given_another_generator_shows_its_ground() {
    let Some(mut changing) = renderer() else { return };
    let plains = Arc::new(starter("Battlefield Plains"));
    let mut scene = generated_view(Arc::new(starter("Rolling Hills")), 8.0, 8.0, 3.0, 12.0);
    render(&mut changing, &scene, 3);
    let bytes = changing.stats().voxels.bytes;
    let wanted = generated_view(plains, 8.0, 8.0, 3.0, 12.0);
    let generator = wanted.query_ref::<&VoxelVolume>().into_iter().next().unwrap().generator.clone();
    let camera = *wanted.query_ref::<(&Camera, &Transform)>().into_iter().next().unwrap().1;
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().generator = generator;
    *scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1 = camera;
    let changed = render(&mut changing, &scene, 3);
    assert_eq!(changing.stats().voxels.bytes, bytes, "the same buffers");
    drop(changing);
    let Some(mut fresh) = renderer() else { return };
    let made = render(&mut fresh, &wanted, 3);
    keep_image("generator changed.png", &changed);
    keep_image("generator from the first.png", &made);
    let differ = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (a, b) = (rgb_at(&changed, x, y), rgb_at(&made, x, y));
            (0..3).any(|i| (a[i] - b[i]).abs() > 8)
        })
        .count();
    // A renderer that has drawn more frames draws a few blades of grass a little otherwise, 0.7% of the pixels here;
    // a column left over from the hills is ground at another height.
    assert!(differ * 100 < (WIDTH * HEIGHT) as usize, "{differ} pixels differ from the volume made with it");
}

/// Oaks and pines of blocks, as a world of blocks grows them.
fn oaks_and_pines() -> Vec<VoxelTrees> {
    let oak = PlantDef { height: [8.0, 12.0], crown_width: [7.0, 10.0], ..PlantDef::default() };
    let pine = PlantDef {
        kind: PlantKind::Conifer,
        height: [12.0, 17.0],
        trunk: 1.0,
        crown_width: [5.0, 7.0],
        crown_height: [9.0, 13.0],
        ..PlantDef::default()
    };
    vec![
        VoxelTrees { plant: Arc::new(oak), per_hectare: 60.0, wood: [104, 78, 56], leaves: [70, 128, 46], areas: Default::default() },
        VoxelTrees { plant: Arc::new(pine), per_hectare: 40.0, wood: [92, 64, 44], leaves: [38, 92, 52], areas: Default::default() },
    ]
}

#[test]
fn an_endless_volume_grows_trees_where_its_ground_grows_grass() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut scene = generated_view(hills.clone(), 8.0, 8.0, 16.0, 16.0);
    let bare = render(&mut renderer, &scene, 3);
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().trees = oaks_and_pines();
    let treed = render(&mut renderer, &scene, 3);
    keep_image("trees across the hills.png", &treed);
    let differ = (0..HEIGHT)
        .step_by(4)
        .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (a, b) = (rgb_at(&bare, x, y), rgb_at(&treed, x, y));
            (0..3).any(|i| (a[i] - b[i]).abs() > 30)
        })
        .count();
    assert!(differ * 10 > 14400, "trees change the view: {differ} of 14400 differ");
    // From above, a wood seen from 30 m.
    let pixels = render(&mut renderer, &with_trees(generated_view(hills, 8.0, 8.0, 30.0, 60.0)), 3);
    keep_image("trees from above.png", &pixels);
    // On the volume's own ground, with no generator, and on mountains with overhangs and caves.
    let mut own = with_trees(generated_view(Arc::new(starter("Rolling Hills")), 8.0, 8.0, 20.0, 30.0));
    own.query::<&mut VoxelVolume>().into_iter().next().unwrap().generator = None;
    let pixels = render(&mut renderer, &own, 3);
    keep_image("trees on its own ground.png", &pixels);
    let pixels = render(&mut renderer, &with_trees(generated_view(Arc::new(caved_alpine()), 8.0, 8.0, 20.0, 30.0)), 3);
    keep_image("trees with caves.png", &pixels);
}

#[test]
fn an_endless_volumes_leaves_and_grass_move_in_the_wind() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    // The view across the trees, at two moments a fifth of a second apart, in a breeze and in still air.
    let mut moments = |strength: f32| {
        let mut scene = with_trees(generated_view(hills.clone(), 8.0, 8.0, 16.0, 16.0));
        scene.spawn((WindZone { strength, ..WindZone::default() }, Transform::default()));
        render(&mut renderer, &scene, 0);
        [2.0, 2.2].map(|time| {
            renderer.render(&scene, time).expect("frame renders");
            renderer.read_pixels().expect("pixels").2
        })
    };
    let differ = |[a, b]: &[Vec<u8>; 2]| {
        (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let (a, b) = (rgb_at(a, x, y), rgb_at(b, x, y));
                (0..3).any(|i| (a[i] - b[i]).abs() > 12)
            })
            .count()
    };
    let breeze = moments(2.0);
    keep_image("wind a.png", &breeze[0]);
    keep_image("wind b.png", &breeze[1]);
    let still = moments(0.0);
    let (moving, resting) = (differ(&breeze), differ(&still));
    eprintln!("{moving} pixels change in a breeze, {resting} in still air");
    assert!(moving > 2000, "leaves and grass move in the wind: {moving} pixels change");
    assert!(resting * 20 < moving, "and keep still without it: {resting} pixels change");
}

#[test]
fn an_endless_volumes_edits_carve_its_ground_and_fill_it() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    // Straight down from 14 m over the hills, where their ground is 24 m up the volume.
    let mut scene = generated_view(hills, 8.0, 8.0, 14.0, 90.0);
    let bare = render(&mut renderer, &scene, 3);
    let edits = vec![
        VoxelEdit::ball(Vec3::new(5.0, 24.0, 8.0), 2.0, None),
        VoxelEdit::ball(Vec3::new(11.0, 24.0, 8.0), 1.5, Some(VoxelMaterial::Rock)),
        // A ball carved into the rock just filled, later, takes a bite out of it.
        VoxelEdit::ball(Vec3::new(12.5, 25.0, 8.0), 0.8, None),
    ];
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(edits);
    let edited = render(&mut renderer, &scene, 3);
    keep_image("edits from above.png", &edited);
    // Averaged over a few pixels round a point of the ground, as the camera sees it straight down.
    let around = |pixels: &[u8], x: f32, z: f32| {
        let (px, py) = from_above(14.0, x, z);
        let mut sum = [0; 3];
        for dy in 0..5 {
            for dx in 0..5 {
                let c = rgb_at(pixels, px + dx - 2, py + dy - 2);
                (0..3).for_each(|i| sum[i] += c[i]);
            }
        }
        sum.map(|v| v / 25)
    };
    // Into the pit: its floor and walls, the dirt under the grass, brown, not the grass's green.
    let (grass, pit) = (around(&bare, 5.0, 8.0), around(&edited, 5.0, 8.0));
    assert!(grass[1] > grass[0] + 20, "grass before: {grass:?}");
    assert!(pit[0] > pit[1] && pit[1] < grass[1] - 15, "the pit: {pit:?} against {grass:?}");
    // The rock filled over the ground: grey.
    let [r, g, b] = around(&edited, 10.5, 8.0);
    assert!((r - g).abs() < 20 && (g - b).abs() < 20 && r > 50, "rock: {r} {g} {b}");
    // Untouched ground away from both stays as it was, but for its grass swaying in the fair day's wind.
    let (before, after) = (around(&bare, 8.0, 3.0), around(&edited, 8.0, 3.0));
    assert!((0..3).all(|i| (before[i] - after[i]).abs() < 12), "the ground between: {after:?} against {before:?}");
    // The same through a generator's Volume, its overhangs and caves: a shaft of balls carved down into the
    // mountains shows, wherever their ground is under the camera.
    let mut scene = generated_view(Arc::new(caved_alpine()), 8.0, 8.0, 14.0, 90.0);
    let bare = render(&mut renderer, &scene, 3);
    let shaft = (0..8).map(|i| VoxelEdit::ball(Vec3::new(8.0, 20.0 + 2.0 * i as f32, 8.0), 2.5, None));
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(shaft.collect());
    let edited = render(&mut renderer, &scene, 3);
    keep_image("edits in the mountains.png", &edited);
    let (before, after) = (around(&bare, 8.0, 8.0), around(&edited, 8.0, 8.0));
    assert!(
        (0..3).map(|i| (before[i] - after[i]).abs()).sum::<i32>() > 30,
        "a pit in the mountains: {after:?} against {before:?}"
    );
}

#[test]
fn an_endless_volume_prints_a_voxel_print_turned_where_it_is_placed_and_a_carve_finds_it_solid() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut scene = generated_view(hills, 8.0, 8.0, 14.0, 90.0);
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(8.0, 40.0, 8.0), -Vec3::Y, 100.0)
        .expect("the ground");
    // A crate 2 m long, a metre wide and tall, red with a white lid, its origin at the middle of its foot.
    let palette = vec![[200, 40, 30, 255], [235, 235, 235, 255]];
    let settings = VoxelPrintSettings::default();
    let print = VoxelPrint::from_voxels([32, 16, 16], 16, Vec3::new(16.0, 0.0, 8.0), palette, settings, |c| {
        if c[1] == 15 { 2 } else { 1 }
    });
    let print = Arc::new(print);
    let turned = Quat::from_rotation_y(30f32.to_radians());
    let crate_at = VoxelEdit::printed(print.clone(), ground.point, turned);
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(vec![crate_at.clone()]);
    let printed = render(&mut renderer, &scene, 3);
    keep_image("print from above.png", &printed);
    let around = |pixels: &[u8], x: f32, z: f32| {
        let (px, py) = from_above(14.0, x, z);
        let mut sum = [0; 3];
        for dy in 0..3 {
            for dx in 0..3 {
                let c = rgb_at(pixels, px + dx - 1, py + dy - 1);
                (0..3).for_each(|i| sum[i] += c[i]);
            }
        }
        sum.map(|v| v / 9)
    };
    // Its white lid over its foot, and along its length as it is turned: 0.8 m along its X.
    let along = turned * Vec3::X;
    for (name, at) in [("middle", Vec3::new(8.0, 0.0, 8.0)), ("along", Vec3::new(8.0, 0.0, 8.0) + along * 0.8)] {
        let [r, g, b] = around(&printed, at.x, at.z);
        assert!(r > 120 && (r - g).abs() < 30 && (g - b).abs() < 30, "the lid at its {name}: {r} {g} {b}");
    }
    // Across it, 0.8 m along its Z, is past its side: the ground, not the lid.
    let past = Vec3::new(8.0, 0.0, 8.0) + (turned * Vec3::Z) * 0.8;
    let [r, g, b] = around(&printed, past.x, past.z);
    assert!(!(r > 120 && (r - g).abs() < 30 && (g - b).abs() < 30), "past its side: {r} {g} {b}");
    // The CPU's ray meets its lid, a metre over the ground.
    let edited = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let lid = voxear_render::voxel_raycast(&edited, Vec3::ZERO, Vec3::new(8.0, 40.0, 8.0), -Vec3::Y, 100.0)
        .expect("the lid");
    assert!((lid.point.y - (ground.point.y + 1.0)).abs() < 0.07, "{lid:?} over {ground:?}");
    // A ball carved into its lid, later, finds it solid inside: red, not the ground's colours, and not hollow.
    let pit = VoxelEdit::ball(ground.point + Vec3::Y * 1.0, 0.4, None);
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(vec![crate_at, pit]);
    let carved = render(&mut renderer, &scene, 3);
    keep_image("print carved from above.png", &carved);
    let [r, g, b] = around(&carved, 8.0, 8.0);
    assert!(r > g + 40 && r > b + 40, "inside the crate: {r} {g} {b}");
}

#[test]
fn no_tree_grass_or_flower_grows_where_a_voxel_print_stands() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let view = || generated_view(hills.clone(), 8.0, 8.0, 30.0, 90.0);
    let volume = view().query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(8.0, 60.0, 8.0), -Vec3::Y, 100.0)
        .expect("the ground");
    // An open-topped box 14 m a side and 3 m tall, its floor and walls a voxel thick, its foot's middle on the ground.
    let palette = vec![[90, 90, 200, 255]];
    let print =
        VoxelPrint::from_voxels([224, 48, 224], 16, Vec3::new(112.0, 0.0, 112.0), palette, Default::default(), |c| {
            u8::from(c[1] == 0 || c[0] == 0 || c[2] == 0 || c[0] == 223 || c[2] == 223)
        });
    let boxed = Arc::new(vec![VoxelEdit::printed(Arc::new(print), ground.point, Quat::IDENTITY)]);
    // The view straight down over the wood at two moments in a breeze, with the box and without it.
    let mut moments = |edits: Arc<Vec<VoxelEdit>>| {
        let mut scene = with_trees(view());
        scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = edits;
        scene.spawn((WindZone { strength: 2.0, ..WindZone::default() }, Transform::default()));
        render(&mut renderer, &scene, 0);
        [2.0, 2.2].map(|time| {
            renderer.render(&scene, time).expect("frame renders");
            renderer.read_pixels().expect("pixels").2
        })
    };
    let open = moments(Arc::new(Vec::new()));
    let inside = moments(boxed);
    keep_image("wood from above.png", &open[0]);
    keep_image("wood boxed from above.png", &inside[0]);
    // Within the box, 4 m in from its walls, as far as crowns outside it lean over them.
    let (low, high) = (from_above(30.0, 4.0, 12.0), from_above(30.0, 12.0, 4.0));
    let changing = |[a, b]: &[Vec<u8>; 2]| {
        (low.1..high.1)
            .flat_map(|y| (low.0..high.0).map(move |x| (x, y)))
            .filter(|&(x, y)| (0..3).any(|i| (rgb_at(a, x, y)[i] - rgb_at(b, x, y)[i]).abs() > 12))
            .count()
    };
    let area = ((high.0 - low.0) * (high.1 - low.1)) as usize;
    let (moving, still) = (changing(&open), changing(&inside));
    eprintln!("in the box's middle, of {area} pixels: {moving} move without it, {still} with it");
    // Grass sways and leaves too: in the box, only the ground's top risen over its floor, which keeps still.
    assert!(moving * 10 > area, "the wood and its grass are there: {moving} pixels move");
    assert!(still * 100 < area, "nothing grows in the box: {still} of {area} pixels change");
}

#[test]
fn a_ray_worked_out_on_the_cpu_meets_the_ground_the_gpu_draws() {
    let Some(mut renderer) = renderer() else { return };
    for (name, world) in [("hills", without_rivers(&starter("Rolling Hills"))), ("mountains", caved_alpine())] {
        // Looking down the slope from 6 m up, through the middle of the picture.
        let mut scene = generated_view(Arc::new(world), 8.0, 8.0, 6.0, 35.0);
        let bare = render(&mut renderer, &scene, 3);
        let (camera, volume) = {
            let camera = scene.query::<(&Camera, &Transform)>().into_iter().next().unwrap().1.clone();
            (camera, scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone())
        };
        let forward = camera.rotation * Vec3::Z;
        let started = std::time::Instant::now();
        let hit =
            voxear_render::voxel_raycast(&volume, Vec3::ZERO, camera.position, forward, 500.0).expect("the ground");
        let took = started.elapsed();
        eprintln!("{name}: met at {:.2} m, {:?}, in {took:?}", hit.distance, hit.normal);
        // A small ball carved there opens a pit in the middle of the picture.
        let pit = VoxelEdit::ball(hit.point, 0.8, None);
        scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(vec![pit]);
        let carved = render(&mut renderer, &scene, 3);
        keep_image(&format!("ray into the {name}.png"), &carved);
        let changed = (0..9)
            .flat_map(|dy| (0..9).map(move |dx| (WIDTH / 2 + dx - 4, HEIGHT / 2 + dy - 4)))
            .filter(|&(x, y)| (0..3).any(|i| (rgb_at(&bare, x, y)[i] - rgb_at(&carved, x, y)[i]).abs() > 25))
            .count();
        assert!(changed > 25, "{name}: the pit where the ray met the ground changes {changed} of 81 pixels");
        assert!(took.as_millis() < 50, "{name}: a ray takes {took:?}");
    }
}

/// How a wood of red leaves and blue wood, seen straight down from 20 m,
/// is drawn on the GPU and found on the CPU, a pixel in twelve each way:
/// each pixel's place on screen, whether the GPU drew a tree there and
/// whether the CPU's ray met one. With `dark_trees`, a pixel too dark to
/// tell is the inside of a crown, as it is in a wood; else it is left out,
/// as where the trees' shade falls on open grass.
fn trees_seen(
    renderer: &mut Renderer,
    mut trees: Vec<VoxelTrees>,
    areas: Vec<VoxelArea>,
    dark_trees: bool,
    name: &str,
) -> Vec<((u32, u32), bool, bool)> {
    for kind in &mut trees {
        (kind.wood, kind.leaves) = ([40, 40, 230], [230, 40, 40]);
    }
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut scene = generated_view(hills, 8.0, 8.0, 20.0, 90.0);
    {
        let mut volumes = scene.query::<&mut VoxelVolume>();
        let volume = volumes.into_iter().next().unwrap();
        (volume.trees, volume.areas) = (trees, Arc::new(areas));
        // No red flowers to be taken for leaves.
        volume.ground.as_mut().unwrap().flowers = 0.0;
    }
    scene.spawn((WindZone { strength: 0.0, ..WindZone::default() }, Transform::default()));
    let pixels = render(renderer, &scene, 3);
    keep_image(name, &pixels);
    let (camera, place) = scene.query::<(&Camera, &Transform)>().into_iter().next().map(|(c, t)| (c.clone(), t.clone())).unwrap();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let bare = VoxelVolume { trees: Vec::new(), ..volume.clone() };
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let tan = (camera.fov_y_for(aspect) * 0.5).tan();
    let (forward, up) = (place.rotation * Vec3::Z, place.rotation * Vec3::Y);
    let right = up.cross(forward);
    let mut seen = Vec::new();
    for py in (6..HEIGHT).step_by(12) {
        for px in (6..WIDTH).step_by(12) {
            let x = ((px as f32 + 0.5) / WIDTH as f32 * 2.0 - 1.0) * tan * aspect;
            let y = (1.0 - (py as f32 + 0.5) / HEIGHT as f32 * 2.0) * tan;
            let d = (forward + right * x + up * y).normalize();
            let ray = |volume: &VoxelVolume| voxear_render::voxel_raycast(volume, Vec3::ZERO, place.position, d, 200.0);
            let (Some(hit), Some(ground)) = (ray(&volume), ray(&bare)) else { continue };
            let cpu = hit.distance < ground.distance - 0.1;
            // Red leaves and blue wood, lit or in shadow: most of the pixel's light red, or blue. Inside the crowns
            // the shade is black, which the ground in the sun never is.
            let [r, g, b] = rgb_at(&pixels, px, py).map(|c| c as f32);
            let sum = (r + g + b).max(1.0);
            if sum <= 30.0 && !dark_trees {
                continue;
            }
            let gpu = sum <= 6.0 || r / sum > 0.5 || b / sum > 0.5;
            seen.push(((px, py), gpu, cpu));
        }
    }
    seen
}

#[test]
fn the_cpus_trees_stand_where_the_gpu_draws_them() {
    let Some(mut renderer) = renderer() else { return };
    let started = std::time::Instant::now();
    let seen = trees_seen(&mut renderer, oaks_and_pines(), Vec::new(), true, "cpu trees from above.png");
    let all = seen.len();
    let agree = seen.iter().filter(|(_, gpu, cpu)| gpu == cpu).count();
    let gpu_trees = seen.iter().filter(|(_, gpu, _)| *gpu).count();
    let cpu_trees = seen.iter().filter(|(_, _, cpu)| *cpu).count();
    eprintln!("{agree} of {all} agree; trees at {gpu_trees} on the GPU, {cpu_trees} on the CPU, in {:?}", started.elapsed());
    assert!(gpu_trees * 4 > all, "the wood covers the view: {gpu_trees} of {all}");
    assert!(agree * 100 >= all * 95, "the CPU's trees are the GPU's: {agree} of {all} agree");
}

/// A square World Area with tag `tag`, from `low` to `high` along X and Z.
fn square_area(tag: u128, low: [f32; 2], high: [f32; 2]) -> VoxelArea {
    let outline = vec![[low[0], low[1]], [high[0], low[1]], [high[0], high[1]], [low[0], high[1]]];
    VoxelArea { outline, tags: vec![tag], ..VoxelArea::default() }
}

#[test]
fn trees_kept_within_a_world_area_grow_only_there_on_the_gpu_and_the_cpu() {
    let Some(mut renderer) = renderer() else { return };
    // The wood's tag holds the west of the view, X under 8 m; a town's, which pines keep out of, its south-west.
    let (wood, town) = (41u128, 42u128);
    let mut trees = oaks_and_pines();
    trees[0].areas = VoxelAreaRules { within: vec![wood], never: Vec::new() };
    trees[1].areas = VoxelAreaRules { within: vec![wood], never: vec![town] };
    let areas = vec![square_area(wood, [-200.0, -200.0], [8.0, 200.0]), square_area(town, [-200.0, -200.0], [8.0, 8.0])];
    let seen = trees_seen(&mut renderer, trees, areas, false, "trees within an area.png");
    let all = seen.len();
    let agree = seen.iter().filter(|(_, gpu, cpu)| gpu == cpu).count();
    let side = |west: bool| seen.iter().filter(move |((px, _), ..)| if west { *px < WIDTH * 3 / 10 } else { *px > WIDTH * 7 / 10 });
    let (west, east) = (side(true).filter(|(_, gpu, _)| *gpu).count(), side(false).filter(|(_, gpu, _)| *gpu).count());
    let cpu_east = side(false).filter(|(_, _, cpu)| *cpu).count();
    eprintln!("{agree} of {all} agree; trees at {west} in the west, {east} in the east ({cpu_east} on the CPU)");
    assert!(agree * 100 >= all * 95, "the CPU's trees are the GPU's: {agree} of {all} agree");
    assert!(west * 4 > side(true).count(), "a wood in the west: {west} of {}", side(true).count());
    assert_eq!((east, cpu_east), (0, 0), "no tree in the east");
}

#[test]
fn crops_grow_in_rows_on_their_ground_within_their_fields_and_meadow_grass_round_them() {
    let Some(mut renderer) = renderer() else { return };
    let fields = 51u128;
    let wheat = VoxelCover {
        areas: VoxelAreaRules { within: vec![fields], never: Vec::new() },
        ..VoxelCover::of(VoxelCoverKind::Wheat)
    };
    let cabbages = VoxelCover::of(VoxelCoverKind::Cabbage);
    let meadow = VoxelCover::of(VoxelCoverKind::MeadowGrass);
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    // Straight down from 6 m: the field to the west of X 8 m, and cabbages in the north-east.
    let mut scene = generated_view(hills, 8.0, 8.0, 6.0, 90.0);
    scene.spawn((WindZone { strength: 0.0, ..WindZone::default() }, Transform::default()));
    let bare = render(&mut renderer, &scene, 3);
    {
        let mut volumes = scene.query::<&mut VoxelVolume>();
        let volume = volumes.into_iter().next().unwrap();
        let mut cabbage_rows = cabbages.clone();
        cabbage_rows.areas = VoxelAreaRules { within: vec![fields + 1], never: Vec::new() };
        volume.cover = vec![wheat, cabbage_rows, meadow];
        volume.areas = Arc::new(vec![
            square_area(fields, [-200.0, -200.0], [8.0, 200.0]),
            square_area(fields + 1, [8.0, 8.0], [200.0, 200.0]),
        ]);
    }
    let started = std::time::Instant::now();
    let grown = render(&mut renderer, &scene, 3);
    eprintln!("grown in {:?}", started.elapsed());
    keep_image("crops from above.png", &grown);
    keep_image("crops bare.png", &bare);
    let count = |pixels: &[u8], xs: std::ops::Range<u32>, ys: std::ops::Range<u32>, test: &dyn Fn([i32; 3]) -> bool| {
        let (mut found, mut all) = (0, 0);
        for y in ys.step_by(3) {
            for x in xs.clone().step_by(3) {
                found += usize::from(test(rgb_at(pixels, x, y)));
                all += 1;
            }
        }
        (found, all)
    };
    // Straw and golden ears in the west, warm and not green; the bare ground there was grass.
    let straw = |[r, g, b]: [i32; 3]| r > b + 30 && r >= g - 4;
    let (west, west_all) = count(&grown, 20..WIDTH * 3 / 10, 20..HEIGHT - 20, &straw);
    let (before, _) = count(&bare, 20..WIDTH * 3 / 10, 20..HEIGHT - 20, &straw);
    eprintln!("straw: {west} of {west_all} in the field, {before} before");
    assert!(west * 3 > west_all && before * 20 < west_all, "a wheat field in the west: {west} of {west_all}, {before} before");
    // The meadow's grass in the south-east, still green, with seed heads changing it from the bare ground's.
    let green = |[r, g, b]: [i32; 3]| g > r + 4 && g > b + 4;
    let (south_east, se_all) = count(&grown, WIDTH * 7 / 10..WIDTH - 20, HEIGHT * 6 / 10..HEIGHT - 20, &green);
    assert!(south_east * 2 > se_all, "meadow grass in the south-east: {south_east} of {se_all} green");
    let changed = (0..HEIGHT).step_by(6).flat_map(|y| (WIDTH * 7 / 10..WIDTH).step_by(6).map(move |x| (x, y)));
    let changed = changed.filter(|&(x, y)| (0..3).any(|i| (rgb_at(&grown, x, y)[i] - rgb_at(&bare, x, y)[i]).abs() > 20)).count();
    assert!(changed > 100, "the meadow and the cabbages change the east: {changed} pixels");
    // At eye height across the field's edge, for looking at.
    let crops = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let mut across = generated_view(crops.generator.as_ref().unwrap().world.clone(), 8.0, 2.0, 0.0, 12.0);
    let ground = voxear_render::voxel_ground_at(&crops, 8.0, 2.0).map_or(24.0, |(height, _)| height);
    {
        let mut cameras = across.query::<(&Camera, &mut Transform)>();
        let (_, place) = cameras.into_iter().next().unwrap();
        place.position.y = ground + 1.7;
    }
    let mut volumes = across.query::<&mut VoxelVolume>();
    *volumes.into_iter().next().unwrap() = crops;
    let pixels = render(&mut renderer, &across, 3);
    keep_image("crops across.png", &pixels);
}

fn with_trees(mut scene: World) -> World {
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().trees = oaks_and_pines();
    scene
}

/// Alpine without its rivers, with a world of blocks' own overhangs, tunnels
/// and chambers wired into its Volume, open to the surface. Its network of
/// great halls is taken out, as the GPU leaves them out.
fn caved_alpine() -> WorldGen {
    let world = without_rivers(&starter("Alpine"));
    let mut graph = world.graph().clone();
    assert!(graph.add_voxels(OwnVoxels { overhangs: 4.0, caves: 0.8, roof: 0.0 }, false));
    let halls: Vec<_> =
        graph.nodes.iter().filter(|node| matches!(node.kind, NodeKind::CaveNetwork(_))).map(|node| node.id).collect();
    for node in halls {
        graph.remove(node);
    }
    WorldGen::new(world.seed(), world.scale(), world.sea_level(), graph)
}

#[test]
fn a_generators_overhangs_and_caves_on_the_gpu_are_the_cpus() {
    let Some(renderer) = renderer() else { return };
    let world = caved_alpine();
    let program = world.gpu_program().expect("it runs on the GPU");
    assert!(program.volume.is_some(), "its Volume runs on the GPU: {:?}", program.left_out);
    // Cells a sixteenth of a metre across, round the ground of columns over the mountains: from 8 m under it to
    // 5 m over, where the overhangs and tunnels are.
    let size = 1.0 / 16.0;
    let mut places = Vec::new();
    for column in 0..1024 {
        let (x, z) = ((column % 32) as f32 * 61.7 - 900.0, (column / 32) as f32 * 59.3 - 800.0);
        let ground = world.height_at(x, z);
        for step in 0..27 {
            places.push([x, ground - 8.0 + step as f32 * 0.5 + 0.03125, z]);
        }
    }
    let made = renderer.generated_on_gpu(&world, &places, size).expect("the generator runs on the GPU");
    let volume = world.volume().expect("a Volume");
    let xs: Vec<f32> = places.iter().map(|p| p[0]).collect();
    let ys: Vec<f32> = places.iter().map(|p| p[1]).collect();
    let zs: Vec<f32> = places.iter().map(|p| p[2]).collect();
    let mut heights = vec![0.0; places.len()];
    world.heights(&xs, &zs, &mut heights);
    let carried = volume.carried(&xs, &zs, &heights);
    let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
    let (mut density, mut ground) = (vec![0.0; places.len()], vec![0.0; places.len()]);
    let cells = Cells { x: &xs, y: &ys, z: &zs, carried: &rows, size };
    volume.densities(cells, false, &mut density, &mut ground);
    let (mut sides, mut near, mut close, mut solid, mut hollowed) = (0, 0, 0, 0, 0);
    for (at, gpu) in made.iter().enumerate() {
        let cpu = density[at];
        sides += usize::from((gpu[4] > 0.0) == (cpu > 0.0));
        if cpu.abs() < 2.0 {
            near += 1;
            close += usize::from((gpu[4] - cpu).abs() < 0.01);
        }
        solid += usize::from(cpu > 0.0);
        // Open air under the ground's height: a tunnel, a chamber or the underside of an overhang.
        hollowed += usize::from(cpu <= 0.0 && ys[at] < heights[at] - 1.0);
    }
    eprintln!(
        "{} cells: {sides} on the same side, {close} of {near} near the surface within 1 cm; {solid} solid, {hollowed} hollowed under the ground",
        places.len()
    );
    assert!(hollowed > places.len() / 100, "caves and overhangs to compare: {hollowed} hollowed cells");
    assert!(sides * 1000 >= places.len() * 998, "{sides} of {} cells on the same side of the ground", places.len());
    assert!(close * 1000 >= near * 990, "{close} of {near} near the surface within a centimetre");
}

#[test]
fn an_endless_volume_shows_its_generators_overhangs_and_caves() {
    let Some(mut renderer) = renderer() else { return };
    let world = Arc::new(caved_alpine());
    // Over steep ground, looking down the slope.
    let steep = (0..4000)
        .map(|i| [(i % 64) as f32 * 23.0 + 300.0, (i / 64) as f32 * 23.0 + 300.0])
        .find(|&[x, z]| (world.height_at(x + 2.0, z) - world.height_at(x - 2.0, z)).abs() > 5.0)
        .expect("steep ground in the mountains");
    let scene = generated_view(world.clone(), steep[0], steep[1], 8.0, 50.0);
    let caved = render(&mut renderer, &scene, 3);
    keep_image("generated caves.png", &caved);
    // The same without its Volume: the ground its height makes solid.
    let plain = Arc::new(without_rivers(&starter("Alpine")));
    let solid = render(&mut renderer, &generated_view(plain, steep[0], steep[1], 8.0, 50.0), 3);
    keep_image("generated without caves.png", &solid);
    let differ = (0..HEIGHT)
        .step_by(4)
        .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let (a, b) = (rgb_at(&caved, x, y), rgb_at(&solid, x, y));
            (0..3).any(|i| (a[i] - b[i]).abs() > 30)
        })
        .count();
    assert!(differ * 20 > 14400, "the overhangs and caves change the view: {differ} of 14400 differ");
    // A cave's mouth: open air from half a metre to two under the ground, by the CPU's own Volume, looked into
    // from four metres over it.
    let volume = world.volume().unwrap();
    let open = |x: f32, z: f32| {
        let ground = world.height_at(x, z);
        let ys = [ground - 0.5, ground - 1.25, ground - 2.0];
        let carried = volume.carried(&[x; 3], &[z; 3], &[ground; 3]);
        let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
        let (mut density, mut under) = ([0.0; 3], [0.0; 3]);
        let cells = Cells { x: &[x; 3], y: &ys, z: &[z; 3], carried: &rows, size: 1.0 / 16.0 };
        volume.densities(cells, false, &mut density, &mut under);
        density.iter().all(|&d| d < 0.0)
    };
    let mouth = (0..20000)
        .map(|i| [(i % 141) as f32 * 7.3 - 500.0, (i / 141) as f32 * 7.1 - 500.0])
        .find(|&[x, z]| open(x, z))
        .expect("a cave open to the sky");
    let pixels = render(&mut renderer, &generated_view(world.clone(), mouth[0], mouth[1], 6.0, 90.0), 3);
    keep_image("generated cave mouth.png", &pixels);
    // And from inside a cave four metres under the ground, looking along it.
    let inside = |x: f32, z: f32| {
        let ground = world.height_at(x, z);
        let carried = volume.carried(&[x], &[z], &[ground]);
        let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
        let (mut density, mut under) = ([0.0], [0.0]);
        let cells = Cells { x: &[x], y: &[ground - 4.0], z: &[z], carried: &rows, size: 1.0 / 16.0 };
        volume.densities(cells, false, &mut density, &mut under);
        density[0] < -1.5
    };
    let cave = (0..40000)
        .map(|i| [(i % 199) as f32 * 3.1 - 300.0, (i / 199) as f32 * 3.3 - 300.0])
        .find(|&[x, z]| inside(x, z))
        .expect("a cave under the ground");
    let pixels = render(&mut renderer, &generated_view(world, cave[0], cave[1], -4.0, 5.0), 3);
    keep_image("generated inside a cave.png", &pixels);
}
