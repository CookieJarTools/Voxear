//! Voxel Volumes drawn on a real GPU, offscreen. Each test skips itself
//! (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AmbientOcclusion, AntiAliasing, AntiAliasingMode, BoxBone, BoxModel, Camera, Light, MaterialDesc, MeshRenderer,
    ModelBox, PostProcess, Primitive, Renderer, RendererSettings, Sky, VoxelArea, VoxelAreaRules, VoxelBiomes,
    VoxelCover, VoxelCoverKind, VoxelEdit, VoxelGenerator, VoxelGround, VoxelMaterial, VoxelPrint, VoxelPrintMaterial,
    VoxelPrintSettings, VoxelStamp, VoxelTrees, VoxelVolume, Voxels, WindZone,
};
use voxear_render::{GlobalIllumination, Ocean, Weather};
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
fn a_volumes_voxels_shadow_a_model_standing_by_them() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = field(true);
    // A white slab of a model 4 m square on the ground east of the pillar, across its shadow.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let white = renderer.add_material(&white).unwrap();
    let slab = Transform::new(Vec3::new(12.0, 1.1, 8.0), Quat::IDENTITY, Vec3::new(4.0, 0.2, 4.0));
    world.spawn((MeshRenderer::new(cube, white), slab));
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel shadow on a model.png", &pixels);
    let brightness = |x: f32, z: f32| {
        let (px, py) = from_above(20.0, x, z);
        rgb_at(&pixels, px, py).iter().sum::<i32>()
    };
    // On the slab, in the pillar's shadow along its middle, and lit either side of it.
    let shadowed = brightness(11.0, 8.0);
    for open in [brightness(11.0, 6.6), brightness(11.0, 9.4)] {
        assert!(shadowed * 3 < open * 2, "the pillar's shadow on the slab: {shadowed} against {open}");
    }
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
    // The volume made on the CPU is drawn a few hundred pixels otherwise from one run to the next where a ray grazes
    // a blade of grass, and a ray worked out a little otherwise moves which: about 110 to 180 differ.
    assert!(far * 100 < count, "{far} of {count} pixels differ by more than 24 levels");
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
    // Straight down from 8 m over the river's water, the volume's floor 24 m under its bed, its water smooth.
    let mut scene = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    let mut bare = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    for volume in bare.query::<&mut VoxelVolume>() {
        volume.generator.as_mut().unwrap().world = Arc::new(rivered.without_rivers());
    }
    for volume in scene.query::<&mut VoxelVolume>().into_iter().chain(bare.query::<&mut VoxelVolume>()) {
        volume.water = voxear_render::VoxelWaterLook::Smooth;
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

/// Measures what an endless volume's height costs on the same ground, Rolling
/// Hills with oaks and pines at reach 64, 48 m tall and taller: its bytes, a
/// still view's passes looking along the ground and up at the sky (the least
/// of 60 frames) and the making while flying at 10 m/s.
/// Run by hand: `cargo test -p voxear_render --test voxels measure_how_tall -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_how_tall_an_endless_volume_is() {
    let Ok(mut renderer) = Renderer::new_offscreen(1920, 1080, &RendererSettings::default()) else { return };
    let hills = Arc::new(starter("Rolling Hills"));
    for tall in [48.0, 128.0, 320.0, 512.0] {
        for pitch in [8.0, -40.0] {
            let mut scene = generated_view(hills.clone(), 8.0, 8.0, 2.0, pitch);
            for volume in scene.query::<&mut VoxelVolume>() {
                volume.reach = 64.0;
                volume.trees = oaks_and_pines();
                volume.size.y = tall;
            }
            renderer.render(&scene, 0.0).unwrap();
            assert!(renderer.make_voxel_volumes(&scene, std::time::Duration::from_secs(60)).unwrap());
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
            let bytes = renderer.stats().voxels.bytes / 1_000_000;
            eprintln!("{tall} m, pitch {pitch}: {bytes} MB, frame {frame:.2} ms, {least:?}");
            if pitch > 0.0 {
                let mut making = Vec::new();
                for i in 0..360 {
                    let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
                    camera.position.x += 10.0 / 60.0;
                    renderer.render(&scene, 2.0 + i as f32 / 60.0).unwrap();
                    if let Some((_, ms)) = renderer.timings().iter().find(|(pass, _)| pass == "Voxel world making") {
                        making.push(*ms);
                    }
                }
                making.sort_by(f32::total_cmp);
                if !making.is_empty() {
                    eprintln!("{tall} m: making on {} frames, median {:.2} ms", making.len(), making[making.len() / 2]);
                }
            }
        }
    }
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

#[test]
fn an_endless_volumes_rivers_drawn_as_voxels_of_water_show_its_water_and_not_its_squares() {
    let Some(mut renderer) = renderer() else { return };
    let rivered = Arc::new(starter("Rolling Hills"));
    let ([x, z], water, bed) = river_in(&rivered, 0.8);
    // Straight down from 8 m over the river's water, as smooth water and as voxels of water.
    let mut smooth = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    for volume in smooth.query::<&mut VoxelVolume>() {
        volume.water = voxear_render::VoxelWaterLook::Smooth;
    }
    let voxels = generated_view(rivered.clone(), x, z, water - bed + 8.0, 90.0);
    assert_eq!(
        voxels.query_ref::<&VoxelVolume>().into_iter().next().unwrap().water,
        voxear_render::VoxelWaterLook::Voxels
    );
    let drawn_smooth = render(&mut renderer, &smooth, 3);
    let made = renderer.stats().voxels.bytes;
    let drawn = render(&mut renderer, &voxels, 3);
    keep_image("river of voxels from above.png", &drawn);
    assert_eq!(renderer.stats().voxels.water_squares, 0, "its water is the columns' own");
    assert!(renderer.stats().voxels.bytes >= made, "the same room");
    // Water under the camera: green-blue, not the grass or gravel of the bed, and near what smooth water shows.
    let (mut watery, mut near) = (0, 0);
    for dy in 0..40 {
        for dx in 0..40 {
            let (px, py) = (WIDTH / 2 + dx - 20, HEIGHT / 2 + dy - 20);
            let [r, g, b] = rgb_at(&drawn, px, py);
            watery += usize::from(b > r && g > r);
            let smooth = rgb_at(&drawn_smooth, px, py);
            near += usize::from((0..3).all(|i| ([r, g, b][i] - smooth[i]).abs() < 60));
        }
    }
    assert!(watery > 1200, "water of voxels under the camera: {watery} of 1600 pixels");
    assert!(near > 800, "and about as dark as smooth water: {near} of 1600 pixels");
    // The CPU's ray goes through it to the bed, as through smooth water.
    let camera = voxels.query_ref::<(&Camera, &Transform)>().into_iter().next().unwrap().1.position;
    let volume = voxels.query_ref::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let hit = voxear_render::voxel_raycast(&volume, Vec3::ZERO, camera, -Vec3::Y, 100.0).expect("the bed");
    assert!((hit.point.y - 24.0).abs() < 0.25, "the ray meets the bed at {:.2}", hit.point.y);
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
            // `RIVER_WATER=smooth` draws the rivers' water smooth.
            if std::env::var("RIVER_WATER").is_ok_and(|look| look == "smooth") {
                volume.water = voxear_render::VoxelWaterLook::Smooth;
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
    // Its inside lies in the pit's shadow, a dark red: red well over green and blue, as no dirt or grass is.
    let [r, g, b] = around(&carved, 8.0, 8.0);
    assert!(r > 20 && r > 3 * g.max(b), "inside the crate: {r} {g} {b}");
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

/// What a house of [`house`] has.
#[derive(Clone, Copy, Default)]
struct House {
    roof: bool,
    door: bool,
    lantern: bool,
    window: bool,
    pillar: bool,
    /// The window's glass red and less clear.
    red_glass: bool,
}

/// A house 4 m square and 3 m tall printed on flat endless ground at
/// `(8, 2, 8)`: stone walls a quarter of a metre thick, with what `has`
/// says: a roof as thick; a door 1 m wide and 2 m tall in its east wall; a
/// lantern of glowing voxels in its north-west corner on the floor; a pane
/// of glass 1 m square in its west wall; and a red pillar inside behind it.
fn house(has: House) -> Arc<VoxelPrint> {
    let House { roof, door: doorway, lantern, window, pillar, red_glass } = has;
    let stone = [150, 146, 138, 255];
    let flame = [255, 186, 80, 255];
    let (glass, clear) = if red_glass { ([220, 40, 30], 8) } else { ([196, 222, 230], 14) };
    let mut settings = VoxelPrintSettings::default();
    settings.materials = vec![
        VoxelPrintMaterial { colour: [255, 186, 80], glass: 0, light: 14 },
        VoxelPrintMaterial { colour: glass, glass: clear, light: 0 },
    ];
    let glass = [glass[0], glass[1], glass[2], 255];
    let print = VoxelPrint::from_voxels(
        [64, 48, 64],
        16,
        Vec3::new(32.0, 0.0, 32.0),
        vec![stone, flame, glass, [200, 30, 30, 255]],
        settings,
        |c| {
            let [x, y, z] = c.map(|a| a as i32);
            let wall = x < 4 || x >= 60 || z < 4 || z >= 60;
            let top = y >= 44;
            let door = doorway && x >= 60 && (24..40).contains(&z) && y < 32;
            let pane = x < 4 && (24..40).contains(&z) && (16..32).contains(&y);
            let lamp = (6..10).contains(&x) && (6..10).contains(&z) && y < 6;
            let red = (16..32).contains(&x) && (20..44).contains(&z) && y < 40;
            if pillar && red {
                4
            } else if lantern && lamp {
                2
            } else if window && pane {
                3
            } else if (wall && !door) || (roof && top) {
                1
            } else {
                0
            }
        },
    );
    Arc::new(print)
}

/// Flat endless ground 2 m up with `print` printed at `(8, 2, 8)`, seen
/// straight down from `height` metres over the ground there, the sun low in
/// the west, and the sky's light all round.
fn printed_from_above(print: Arc<VoxelPrint>, height: f32) -> World {
    let mut world = World::new();
    let ground = VoxelGround { height: 2.0, hills: 0.0, grass: 0.0, flowers: 0.0, ..VoxelGround::default() };
    let edits = Arc::new(vec![VoxelEdit::printed(print, Vec3::new(8.0, 2.0, 8.0), Quat::IDENTITY)]);
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 8.0, 16.0),
        ground: Some(ground),
        endless: true,
        reach: 24.0,
        edits,
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((
        Camera { near: 0.05, ..Camera::default() },
        Transform::new(Vec3::new(8.0, 2.0 + height, 8.0), down, Vec3::ONE),
    ));
    let towards_sun = Vec3::new(-35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    world
}

/// The mean of the pixels within `r` of where `(x, z)` on the ground is,
/// seen from `height` metres over it.
fn mean_around(pixels: &[u8], height: f32, (x, z): (f32, f32), r: i32) -> [i32; 3] {
    let (px, py) = from_above(height, x, z);
    let mut sum = [0; 3];
    let mut n = 0;
    for dy in -r..=r {
        for dx in -r..=r {
            let c = rgb_at(pixels, (px as i32 + dx) as u32, (py as i32 + dy) as u32);
            (0..3).for_each(|i| sum[i] += c[i]);
            n += 1;
        }
    }
    sum.map(|v| v / n)
}

#[test]
fn a_room_under_a_roof_is_dark_but_for_its_door_and_its_lantern() {
    let Some(mut renderer) = renderer() else { return };
    // Seen from just under where the roof is, 2.6 m over the floor: the floor's middle and the corner by the lantern.
    let height = 2.6;
    let middle = (7.6, 8.4);
    let corner = (6.6, 6.6);
    // A white box of a model on the floor, its top half a metre up, by the south wall.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let white = renderer.add_material(&white).unwrap();
    let model = (8.8, 7.0);
    let mut shot = |has: House, name: &str| {
        let mut scene = printed_from_above(house(has), height);
        let placed = Transform::new(Vec3::new(model.0, 2.25, model.1), Quat::IDENTITY, Vec3::splat(0.5));
        scene.spawn((MeshRenderer::new(cube, white), placed));
        let pixels = render(&mut renderer, &scene, 12);
        keep_image(name, &pixels);
        pixels
    };
    let open = shot(House::default(), "room open to the sky.png");
    let sealed = shot(House { roof: true, ..House::default() }, "room sealed.png");
    let door = shot(House { roof: true, door: true, ..House::default() }, "room with a door.png");
    let lit = shot(House { roof: true, lantern: true, ..House::default() }, "room sealed with a lantern.png");
    let sum = |c: [i32; 3]| c.iter().sum::<i32>();
    let at = |pixels: &[u8], (x, z): (f32, f32)| sum(mean_around(pixels, height, (x, z), 4));
    let (open_middle, sealed_middle) = (at(&open, middle), at(&sealed, middle));
    // The model under the roof is as dark as the voxels round it.
    let (open_model, sealed_model) = (at(&open, model), at(&sealed, model));
    assert!(sealed_model * 4 < open_model, "the model sealed under the roof: {sealed_model} against {open_model}");
    assert!(
        sealed_middle * 4 < open_middle,
        "sealed under the roof: {sealed_middle} against {open_middle} open to the sky"
    );
    // A door lets some of the sky in: the floor by it lighter than by the far wall, which is lighter than sealed.
    let (by_door, far) = (at(&door, (9.4, 8.0)), at(&door, (6.6, 8.0)));
    assert!(
        by_door > far && far > sealed_middle,
        "by the door {by_door}, at the far wall {far}, sealed {sealed_middle}"
    );
    // The lantern lights the floor round it, warm, and the corner across the room less.
    let by_lantern = mean_around(&lit, height, corner, 4);
    let [r, _, b] = by_lantern;
    assert!(sum(by_lantern) > at(&sealed, corner) * 3, "by the lantern: {by_lantern:?}");
    assert!(r > b + 20, "the lantern's light is warm: {by_lantern:?}");
    let across = at(&lit, (9.4, 9.4));
    assert!(across < sum(by_lantern), "across the room {across}, by the lantern {}", sum(by_lantern));
}

#[test]
fn a_window_of_glass_voxels_shows_what_is_behind_it() {
    let Some(mut renderer) = renderer() else { return };
    // From 3 m west of the house, looking east at the middle of its west wall, with a red pillar inside behind it.
    let mut shot = |window: bool, name: &str| {
        let mut scene = printed_from_above(house(House { window, pillar: true, ..House::default() }), 2.6);
        let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
        *camera =
            Transform::new(Vec3::new(3.0, 3.5, 8.0), Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), Vec3::ONE);
        let pixels = render(&mut renderer, &scene, 12);
        keep_image(name, &pixels);
        pixels
    };
    let glass = shot(true, "window of glass.png");
    let wall = shot(false, "wall without a window.png");
    // The lower part of the window, where the sun through it falls on the pillar behind: the wall over the window
    // shades the pillar higher up.
    let middle = |pixels: &[u8]| {
        let mut sum = [0; 3];
        for dy in -6..=6 {
            for dx in -6..=6 {
                let c = rgb_at(pixels, (WIDTH as i32 / 2 + dx) as u32, (HEIGHT as i32 / 2 + 36 + dy) as u32);
                (0..3).for_each(|i| sum[i] += c[i]);
            }
        }
        sum.map(|v| v / 169)
    };
    // The red pillar through the glass, a little paler for it; the stone wall where there is none.
    let [r, g, b] = middle(&glass);
    assert!(r > g + 30 && r > b + 30, "the pillar through the window: {r} {g} {b}");
    let [r, g, b] = middle(&wall);
    assert!((r - g).abs() < 20 && (g - b).abs() < 20, "the wall: {r} {g} {b}");
    // Beside the window, the wall alike in both.
    let beside = |pixels: &[u8]| rgb_at(pixels, WIDTH / 2, HEIGHT / 2 + 100);
    let (a, b) = (beside(&glass), beside(&wall));
    assert!((0..3).all(|i| (a[i] - b[i]).abs() < 12), "the wall beside the window: {a:?} and {b:?}");
}

#[test]
fn a_lamp_inside_a_house_of_voxels_lights_no_ground_outside_its_walls() {
    let Some(mut renderer) = renderer() else { return };
    // At night, seen from 10 m over the ground: the ground a metre outside the west wall, and a model's top there.
    let height = 10.0;
    let outside = (5.0, 8.0);
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let white = renderer.add_material(&white).unwrap();
    let model = (5.0, 9.6);
    let mut shot = |lamp: Option<Vec3>, name: &str| {
        let mut scene = printed_from_above(house(House { roof: true, ..House::default() }), height);
        for light in scene.query::<&mut Light>() {
            light.intensity = 0.0;
        }
        scene.insert_resource(Sky { ambient: 0.02, ..Sky::default() });
        if let Some(at) = lamp {
            scene.spawn((Light::point(Vec3::new(1.0, 0.85, 0.6), 60.0, 9.0), Transform::from_position(at)));
        }
        let placed = Transform::new(Vec3::new(model.0, 2.25, model.1), Quat::IDENTITY, Vec3::splat(0.5));
        scene.spawn((MeshRenderer::new(cube, white), placed));
        let pixels = render(&mut renderer, &scene, 6);
        keep_image(name, &pixels);
        pixels
    };
    let dark = shot(None, "lamp none.png");
    let inside = shot(Some(Vec3::new(8.0, 3.5, 8.0)), "lamp inside the house.png");
    let out = shot(Some(Vec3::new(3.5, 3.5, 8.0)), "lamp outside the house.png");
    let sum = |c: [i32; 3]| c.iter().sum::<i32>();
    let at = |pixels: &[u8], (x, z): (f32, f32)| sum(mean_around(pixels, height, (x, z), 3));
    let (none, from_inside, from_outside) = (at(&dark, outside), at(&inside, outside), at(&out, outside));
    // The lamp outside lights the ground there; the one inside, behind the wall, doesn't.
    assert!(from_outside > none + 60, "lit by the lamp beside it: {from_outside} against {none}");
    assert!(from_inside < none + 12, "not lit through the wall: {from_inside} against {none}");
    // Nor the model standing outside.
    let (none, from_inside) = (at(&dark, model), at(&inside, model));
    assert!(from_inside < none + 12, "the model outside not lit through the wall: {from_inside} against {none}");
}

#[test]
fn the_sun_through_red_glass_falls_red() {
    let Some(mut renderer) = renderer() else { return };
    // Roofless, the low sun from the west through the window onto the floor, seen from 8 m over it: where the floor
    // is brightest, the patch of sun.
    let height = 8.0;
    let mut shot = |red_glass: bool, name: &str| {
        let has = House { window: true, red_glass, ..House::default() };
        let pixels = render(&mut renderer, &printed_from_above(house(has), height), 6);
        keep_image(name, &pixels);
        let mut brightest = [0; 3];
        for k in 0..400 {
            let (x, z) = (6.4 + (k % 20) as f32 * 0.16, 7.4 + (k / 20) as f32 * 0.06);
            let c = mean_around(&pixels, height, (x, z), 1);
            if c.iter().sum::<i32>() > brightest.iter().sum::<i32>() {
                brightest = c;
            }
        }
        brightest
    };
    let clear = shot(false, "sun through clear glass.png");
    let red = shot(true, "sun through red glass.png");
    // The grass floor is green where the sun comes through clear glass; through red, far less green, as red as it.
    assert!(clear[1] > clear[0], "through clear glass: {clear:?}");
    assert!(red[1] * 10 < clear[1] * 7, "through red glass, less green: {red:?} against {clear:?}");
    assert!(red[0] * clear[1] > clear[0] * red[1] * 3 / 2, "through red glass, redder: {red:?} against {clear:?}");
}

#[test]
fn voxels_shadow_the_sea_beside_them() {
    let Some(mut renderer) = renderer() else { return };
    // The field's pillar moved to the volume's east edge, in calm sea half a metre deep: its shadow on the open
    // water east of the volume, which the sea's surface takes as it takes the shadow maps'.
    let mut world = field(false);
    for volume in world.query::<&mut VoxelVolume>() {
        volume.stamps[0].at = Vec3::new(15.0, 1.0, 8.0);
    }
    let calm = Ocean { weather: Weather::Custom, wind: 0.2, swell_wind: 0.2, ..Ocean::default() };
    world.spawn((calm, Transform::from_position(Vec3::new(0.0, 1.5, 0.0))));
    let pixels = render(&mut renderer, &world, 6);
    keep_image("voxel shadow on the sea.png", &pixels);
    let brightness = |x: f32, z: f32| mean_around(&pixels, 19.5, (x, z), 4).iter().sum::<i32>();
    let (open, shadowed) = (brightness(17.5, 5.0), brightness(17.5, 8.0));
    assert!(shadowed * 4 < open * 3, "the pillar's shadow on the sea: {shadowed} against {open}");
}

#[test]
fn with_the_gi_on_its_probes_see_the_voxels_a_shut_room_dark_and_sun_in_at_its_door_bouncing_round_it() {
    let Some(mut renderer) = renderer() else { return };
    // As the room under a roof, lit by the GI's probes in place of the light kept round the camera, the sun low in
    // the east so it shines in at the door onto the floor. Looking up from the floor at the ceiling, which takes only
    // what bounces off that patch of sun.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut shot = |has: House, name: &str| {
        let mut scene = printed_from_above(house(has), 2.6);
        scene.spawn((MeshRenderer::new(cube, grey), Transform::from_position(Vec3::new(-6.0, 2.5, -6.0))));
        let up = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        for (_, place) in scene.query::<(&Camera, &mut Transform)>() {
            *place = Transform::new(Vec3::new(7.4, 2.4, 8.0), up, Vec3::ONE);
        }
        let towards_sun = Vec3::new(35f32.to_radians().cos(), 35f32.to_radians().sin(), 0.0);
        // A bright sun, so what bounces off its patch on the floor shows at this exposure.
        for (light, place) in scene.query::<(&mut Light, &mut Transform)>() {
            place.rotation = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
            light.intensity = 30.0;
        }
        let mut post = PostProcess::neutral();
        post.anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
        post.global_illumination =
            voxear_render::GlobalIllumination { enabled: true, probe_spacing: 0.5, ..Default::default() };
        scene.insert_resource(post);
        let pixels = render(&mut renderer, &scene, 200);
        keep_image(name, &pixels);
        pixels
    };
    let sealed = shot(House { roof: true, ..House::default() }, "gi ceiling sealed.png");
    let door = shot(House { roof: true, door: true, ..House::default() }, "gi ceiling with a door.png");
    // The ceiling round the middle of the picture.
    let ceiling = |pixels: &[u8]| {
        let mut sum = 0;
        for k in 0..25 {
            let (x, y) = (WIDTH / 2 - 100 + (k % 5) * 50, HEIGHT / 2 - 60 + (k / 5) * 30);
            sum += rgb_at(pixels, x, y).iter().sum::<i32>();
        }
        sum / 25
    };
    let (sealed, door) = (ceiling(&sealed), ceiling(&door));
    assert!(sealed < 6, "shut under the roof, the ceiling dark: {sealed}");
    assert!(door > sealed + 10, "the sun in at the door bouncing up to the ceiling: {door} against {sealed} shut");
}

/// A room 15 m square and 5 m tall sealed in stone a metre thick, as the
/// blocks test's sealed room is, seen from a corner towards where its lamp,
/// a metre cube of voxels glowing as a block of light 15 in the colour a
/// world of blocks' light has, stands on the floor; the room's inside from
/// 24 to 39 m along X and Z.
fn sealed_voxel_room(renderer: &mut Renderer, lamp: bool) -> Vec<u8> {
    let stone = [128, 128, 132, 255];
    let glow = [255, 183, 108, 255];
    let mut settings = VoxelPrintSettings::default();
    settings.materials = vec![VoxelPrintMaterial { colour: [255, 183, 108], glass: 0, light: 15 }];
    let print = VoxelPrint::from_voxels([272, 112, 272], 16, Vec3::ZERO, vec![stone, glow], settings, |c| {
        let [x, y, z] = c.map(|a| a as i32);
        let inside = (16..256).contains(&x) && (16..96).contains(&y) && (16..256).contains(&z);
        let lit = lamp && (176..192).contains(&x) && (16..32).contains(&y) && (176..192).contains(&z);
        if lit {
            2
        } else if inside {
            0
        } else {
            1
        }
    });
    let mut world = World::new();
    let ground = VoxelGround { height: 2.0, hills: 0.0, grass: 0.0, flowers: 0.0, ..VoxelGround::default() };
    let edits = Arc::new(vec![VoxelEdit::printed(Arc::new(print), Vec3::new(23.0, 2.0, 23.0), Quat::IDENTITY)]);
    let volume = VoxelVolume {
        size: Vec3::new(16.0, 12.0, 16.0),
        ground: Some(ground),
        endless: true,
        reach: 32.0,
        edits,
        ..VoxelVolume::default()
    };
    world.spawn((volume, Transform::default()));
    let view = Quat::from_rotation_y(45f32.to_radians()) * Quat::from_rotation_x(0.25);
    world.spawn((Camera::default(), Transform::new(Vec3::new(25.5, 6.0, 25.5), view, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 4.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 1.0, ..Sky::default() });
    let mut post = PostProcess::neutral();
    post.anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    world.insert_resource(post);
    render(renderer, &world, 12)
}

#[test]
fn a_sealed_room_of_voxels_is_dark_but_round_its_lamp_as_a_room_of_blocks_is() {
    let Some(mut renderer) = renderer() else { return };
    let dark = sealed_voxel_room(&mut renderer, false);
    let lamp = sealed_voxel_room(&mut renderer, true);
    keep_image("voxels sealed room.png", &dark);
    keep_image("voxels sealed room lamp.png", &lamp);
    let mean = |pixels: &[u8]| {
        pixels.chunks(4).map(|p| f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2])).sum::<f32>()
            / (pixels.len() / 4 * 3) as f32
    };
    // As the blocks test asks of its room: no light gets in, the lamp lights it, warm.
    let (unlit, lit) = (mean(&dark), mean(&lamp));
    assert!(unlit < 3.0, "no light gets into it: {unlit:.2}");
    assert!(lit > unlit + 15.0, "the lamp lights it: {lit:.1} against {unlit:.2}");
    let warm = lamp.chunks(4).filter(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) > 60);
    let (red, blue) = warm.fold((0u64, 0u64), |(r, b), p| (r + u64::from(p[0]), b + u64::from(p[2])));
    assert!(red > blue * 3 / 2, "the lamp's light is warm: red {red} against blue {blue}");
}

/// The frame's cost at 1920×1080 of a street of houses printed into Rolling
/// Hills without its rivers, seen from 20 m up, with models
/// standing among them: each pass and the whole frame the least of 60
/// frames, with the post-processing off and with its usual ambient
/// occlusion and contact shadows. Their windows' colours are a fifth solid,
/// so they are glass where voxels can be see-through. Run by hand:
/// `cargo test -p voxear_render --test voxels measure_a_street_of_houses -- --ignored --nocapture`,
/// with `VOXEAR_TEST_OUTPUT` naming a folder for the pictures.
#[test]
#[ignore]
fn measure_a_street_of_houses() {
    let Ok(mut renderer) = Renderer::new_offscreen(1920, 1080, &RendererSettings::default()) else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let stone = [150, 146, 138, 255];
    let glass = [196, 222, 230, 50];
    let roof = [120, 60, 44, 255];
    let print = VoxelPrint::from_voxels(
        [80, 64, 96],
        16,
        Vec3::new(40.0, 0.0, 48.0),
        vec![stone, glass, roof],
        VoxelPrintSettings::default(),
        |c| {
            let [x, y, z] = c.map(|a| a as i32);
            // A roof rising to a ridge along Z, a quarter of a metre thick, over walls with gables at its ends.
            let ridge = 44 + (40 - (x - 40).abs()) / 2;
            let roofing = y >= 44 && y <= ridge && y + 4 > ridge;
            let wall = ((x < 4 || x >= 76) && y < 44) || ((z < 4 || z >= 92) && y <= ridge);
            let window = (x < 4 || x >= 76) && (z % 32) >= 10 && (z % 32) < 22 && (16..34).contains(&y);
            let door = z < 4 && (32..48).contains(&x) && y < 34;
            if roofing {
                3
            } else if window {
                2
            } else if wall && !door {
                1
            } else {
                0
            }
        },
    );
    let print = Arc::new(print);
    let cases = [
        ("post off", PostProcess::neutral(), false),
        ("post on", PostProcess::default(), false),
        ("at night with lamps", PostProcess::default(), true),
        (
            "with the GI",
            PostProcess {
                global_illumination: GlobalIllumination { enabled: true, ..Default::default() },
                ..PostProcess::default()
            },
            false,
        ),
    ];
    for (name, post, night) in cases {
        let mut scene = generated_view(hills.clone(), 8.0, 8.0, 20.0, 35.0);
        let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
        let mut edits = Vec::new();
        for (i, &(x, z)) in
            [(1.0, 22.0), (15.0, 20.0), (2.0, 34.0), (16.0, 32.0), (0.0, 46.0), (14.0, 44.0)].iter().enumerate()
        {
            let Some(ground) = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(x, 47.0, z), -Vec3::Y, 47.0)
            else {
                continue;
            };
            let turn = Quat::from_rotation_y(if i % 2 == 0 { 0.0 } else { std::f32::consts::PI });
            edits.push(VoxelEdit::printed(print.clone(), ground.point - Vec3::Y * 0.5, turn));
        }
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.reach = 64.0;
            volume.edits = Arc::new(edits.clone());
        }
        let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
        scene.insert_resource(PostProcess { anti_aliasing, ..post });
        // At night, the sun out and a lamp by each of the twelve figures.
        if night {
            for light in scene.query::<&mut Light>() {
                light.intensity = 0.0;
            }
        }
        // Models standing in the street: boxes a person's size.
        let cube = renderer.primitive(Primitive::Cube).unwrap();
        let cloth = MaterialDesc { base_color: [0.5, 0.35, 0.25, 1.0], roughness: 0.8, ..MaterialDesc::default() };
        let cloth = renderer.add_material(&cloth).unwrap();
        for k in 0..12 {
            let (x, z) = (6.5 + (k % 3) as f32 * 1.5, 22.0 + (k / 3) as f32 * 6.0);
            if let Some(ground) =
                voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(x, 47.0, z), -Vec3::Y, 47.0)
            {
                let placed = Transform::new(ground.point + Vec3::Y * 0.9, Quat::IDENTITY, Vec3::new(0.5, 1.8, 0.3));
                scene.spawn((MeshRenderer::new(cube, cloth), placed));
                if night {
                    let lamp = Light::point(Vec3::new(1.0, 0.8, 0.55), 40.0, 8.0);
                    scene.spawn((lamp, Transform::from_position(ground.point + Vec3::new(0.8, 2.5, 0.0))));
                }
            }
        }
        renderer.render(&scene, 0.0).unwrap();
        assert!(renderer.make_voxel_volumes(&scene, std::time::Duration::from_secs(60)).unwrap());
        let mut least: std::collections::BTreeMap<String, f32> = Default::default();
        let mut frame = f32::MAX;
        for i in 0..90 {
            renderer.render(&scene, i as f32 / 60.0).unwrap();
            if i >= 30 {
                for (pass, ms) in renderer.timings() {
                    let kept = least.entry(pass.clone()).or_insert(f32::MAX);
                    *kept = kept.min(*ms);
                }
                frame = frame.min(renderer.gpu_frame_ms());
            }
        }
        let voxels: Vec<_> = least.iter().filter(|(pass, _)| pass.contains("oxel") || pass.contains("DDGI")).collect();
        let depth: f32 = least.iter().filter(|(pass, _)| pass.starts_with("Depth")).map(|(_, ms)| ms).sum();
        eprintln!("{name}: frame {frame:.2} ms, depth passes {depth:.2} ms, {voxels:?}");
        if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let (w, h, pixels) = renderer.read_pixels().unwrap();
            let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("street, {name}.png"))).unwrap();
            let mut encoder = png::Encoder::new(file, w, h);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
    }
}

/// Rolling Hills with oaks and pines seen from over a hillside across the
/// land, looking down at `pitch` degrees: full detail as far as `reach`,
/// then coarser rings as far as `distance`.
fn far_view(reach: f32, distance: f32, pitch: f32) -> World {
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut scene = with_trees(generated_view(hills, 8.0, 8.0, 40.0, pitch));
    for volume in scene.query::<&mut VoxelVolume>() {
        volume.trees.iter_mut().for_each(|kind| kind.per_hectare *= 0.3);
        volume.reach = reach;
        volume.distance = distance;
        volume.size.y = 64.0;
    }
    scene
}

/// What `renderer` holds on the GPU over the box of voxels from `low`,
/// `size` a side, for `scene`'s endless volume, against what the CPU finds
/// (`VoxelSolids`): every voxel the GPU keeps is solid on the CPU with open
/// air beside it, and every such voxel the GPU keeps, where `edited` says a
/// voxel or one beside it is edited (given each voxel and its column's
/// ground). Gives how many differ there, how many the GPU keeps, and what
/// it keeps at each, X fastest. Elsewhere the GPU keeps a row of voxels
/// under the ground's sides that no one sees, and a generator's height may
/// round to the voxel over or under on the CPU at a few columns, which are
/// counted apart.
fn gpu_against_cpu(
    renderer: &Renderer,
    scene: &World,
    (low, size): ([i32; 3], [i32; 3]),
    edited: &dyn Fn(voxear_core::math::IVec3, i32) -> bool,
) -> (usize, usize, Vec<u32>) {
    let (entity, volume) = scene
        .query_ref::<(voxear_ecs::Entity, &VoxelVolume)>()
        .into_iter()
        .map(|(entity, volume)| (entity, volume.clone()))
        .next()
        .unwrap();
    let mut cells = Vec::new();
    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                cells.push([low[0] + x, low[1] + y, low[2] + z]);
            }
        }
    }
    let (found, _) = renderer.voxels_on_gpu(entity, &cells).unwrap().expect("an endless volume");
    // The CPU's, with a border of one for the air beside each.
    let solids = voxear_render::VoxelSolids::of(&volume).unwrap();
    let wide = size.map(|a| a + 2);
    let mut solid = vec![false; (wide[0] * wide[1] * wide[2]) as usize];
    solids.fill(low.map(|a| a - 1), wide, &mut solid);
    let at = |x: i32, y: i32, z: i32| solid[((x + 1) + wide[0] * ((y + 1) + wide[1] * (z + 1))) as usize];
    let heights = solids.heights([low[0] - 1, low[2] - 1], [wide[0], wide[2]]);
    let height = |x: i32, z: i32| heights[((x + 1) + wide[0] * (z + 1)) as usize];
    let near = |x: i32, y: i32, z: i32| {
        [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)].iter().any(|&(dx, dy, dz)| {
            let c = voxear_core::math::IVec3::new(low[0] + x + dx, low[1] + y + dy, low[2] + z + dz);
            edited(c, height(x + dx, z + dz))
        })
    };
    let (mut differ, mut kept, mut apart) = (0, 0, 0);
    let mut shown = 0;
    for z in 0..size[2] {
        for y in 0..size[1] {
            for x in 0..size[0] {
                let seen = at(x, y, z)
                    && [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                        .iter()
                        .any(|&(dx, dy, dz)| !at(x + dx, y + dy, z + dz));
                let gpu = found[(x + size[0] * (y + size[1] * z)) as usize] != 0;
                kept += usize::from(gpu);
                if gpu != seen && !near(x, y, z) {
                    apart += 1;
                } else if gpu != seen {
                    differ += 1;
                    if shown < 8 {
                        eprintln!("at {:?}: the GPU {}, the CPU {}", [low[0] + x, low[1] + y, low[2] + z], gpu, seen);
                        shown += 1;
                    }
                }
            }
        }
    }
    if apart > 0 {
        eprintln!("{apart} voxels of the ground no edit reaches differ");
    }
    (differ, kept, found)
}

#[test]
fn an_endless_volume_hundreds_of_metres_tall_keeps_its_highest_ground_as_the_cpu_does() {
    use voxear_terrain::generate::{
        nodes::{PaintedMapKind, PaintedMapNode},
        painted::{Painted, PaintedMaps},
    };
    let Some(mut renderer) = renderer() else { return };
    // A hill painted 360 m high round (60, 0), steep on its sides, in a volume 400 m tall whose floor is under the
    // low ground: its top is a hundred sectors up, its columns' bands far apart.
    let painted = Painted::new(9, PaintedMaps::default());
    painted.change(|maps| maps.height.paint([60.0, 0.0], 70.0, 0.6, &|_, cover| Some(cover * 360.0)));
    let height =
        NodeKind::PaintedMap(PaintedMapNode { map: PaintedMapKind::Height, default: 0.0, painted: Some(painted) });
    let world = Arc::new(node_world(height, 0, &[]));
    let summit = world.height_at(60.0, 0.0);
    assert!(summit > 300.0, "the summit stands {summit:.1} m high");
    let mut scene = generated_view(world.clone(), -10.0, 0.0, 20.0, -25.0);
    // The summit is flat: its top half a voxel from a voxel's edge, where the GPU's rounding and the CPU's agree.
    let low_ground = world.height_at(-10.0, 0.0) - 8.0;
    let floor = summit - (((summit - low_ground) * 16.0).floor() + 0.5) / 16.0;
    for volume in scene.query::<&mut VoxelVolume>() {
        volume.size.y = 400.0;
        volume.reach = 96.0;
        volume.ground = Some(VoxelGround { grass: 0.0, flowers: 0.0, ..VoxelGround::default() });
        volume.generator.as_mut().unwrap().floor = floor;
    }
    {
        let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
        camera.position.y = world.height_at(-10.0, 0.0) - floor + 20.0;
        camera.rotation = Quat::from_rotation_y(90f32.to_radians()) * Quat::from_rotation_x(-25f32.to_radians());
    }
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("a hill 360 m high.png", &pixels);
    let volume = scene.query_ref::<&VoxelVolume>().into_iter().next().unwrap().clone();
    // The summit's voxels, and the slope's halfway up, on the GPU as the CPU has them.
    let per = 16;
    for (x, z) in [(60.0f32, 0.0f32), (60.0 - 40.0, 0.0)] {
        let top = ((world.height_at(x, z) - floor) * per as f32) as i32;
        let low = [(x as i32) * per - 16, top - 40, (z as i32) * per - 16];
        let size = [32, 64, 32];
        let (_, kept, found) = gpu_against_cpu(&renderer, &scene, (low, size), &|_, _| false);
        let mut solid = vec![false; found.len()];
        voxear_render::VoxelSolids::of(&volume).unwrap().fill(low, size, &mut solid);
        // Every voxel the CPU has open to the air the GPU keeps, and the GPU keeps none where the CPU has air; the GPU
        // also keeps a row under its ground's top, hidden.
        let at = |x: i32, y: i32, z: i32| (x + size[0] * (y + size[1] * z)) as usize;
        let (mut missing, mut extra, mut open) = (0, 0, 0);
        for z in 1..size[2] - 1 {
            for y in 1..size[1] - 1 {
                for x in 1..size[0] - 1 {
                    let cpu = solid[at(x, y, z)];
                    let shown = cpu
                        && [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
                            .iter()
                            .any(|&(dx, dy, dz)| !solid[at(x + dx, y + dy, z + dz)]);
                    let gpu = found[at(x, y, z)] != 0;
                    open += usize::from(shown);
                    missing += usize::from(shown && !gpu);
                    extra += usize::from(gpu && !cpu);
                }
            }
        }
        eprintln!(
            "at {x}, {z}, {} m up: {kept} kept, {open} open on the CPU, {missing} missing, {extra} over",
            top / per
        );
        assert!(open > 500, "the CPU's ground {} m up: {open} voxels open to the air", top / per);
        assert!(
            missing * 100 < open && extra * 100 < open,
            "the GPU's ground {} m up is the CPU's: {missing} missing, {extra} over",
            top / per
        );
    }
    // The CPU's ray from over the summit meets it where its ground is.
    let over = Vec3::new(60.0, 399.0, 0.0);
    let hit = voxear_render::voxel_raycast(&volume, Vec3::ZERO, over, -Vec3::Y, 400.0).expect("the summit");
    assert!(
        (hit.point.y - (summit - floor)).abs() < 0.2,
        "the ray meets the summit at {:.2}, not {:.2}",
        hit.point.y,
        summit - floor
    );
}

/// Rolling Hills seen from 14 m over (8, 8), with no grass, flowers or
/// trees, so the CPU and the GPU keep the same voxels.
fn bare_hills_view() -> World {
    let mut scene = generated_view(Arc::new(without_rivers(&starter("Rolling Hills"))), 8.0, 8.0, 14.0, 90.0);
    for volume in scene.query::<&mut VoxelVolume>() {
        volume.ground = Some(VoxelGround { grass: 0.0, flowers: 0.0, ..VoxelGround::default() });
    }
    scene
}

/// The mean colour of each 8×8 block of pixels: the tones a picture shows,
/// without the voxels' own grain.
fn tones(pixels: &[u8]) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for by in 0..HEIGHT / 8 {
        for bx in 0..WIDTH / 8 {
            let mut sum = [0; 3];
            for y in by * 8..by * 8 + 8 {
                for x in bx * 8..bx * 8 + 8 {
                    let c = rgb_at(pixels, x, y);
                    (0..3).for_each(|i| sum[i] += c[i]);
                }
            }
            out.push(sum.map(|s| s / 64));
        }
    }
    out
}

#[test]
fn an_endless_volume_is_seen_past_its_reach_in_coarser_rings() {
    let Some(mut renderer) = renderer() else { return };
    // Full detail to 24 m alone, then with rings to 400 m, then full detail to 192 m with the same rings past it.
    let started = std::time::Instant::now();
    let near = render(&mut renderer, &far_view(24.0, 0.0, 12.0), 3);
    let near_bytes = renderer.stats().voxels.bytes;
    eprintln!("without rings: {:.1} s", started.elapsed().as_secs_f32());
    let ringed = render(&mut renderer, &far_view(24.0, 400.0, 12.0), 3);
    let ringed_bytes = renderer.stats().voxels.bytes;
    eprintln!("with rings: {:.1} s", started.elapsed().as_secs_f32());
    let detailed = render(&mut renderer, &far_view(192.0, 400.0, 12.0), 3);
    eprintln!("in full detail: {:.1} s", started.elapsed().as_secs_f32());
    keep_image("far without rings.png", &near);
    keep_image("far in rings.png", &ringed);
    keep_image("far in full detail.png", &detailed);
    eprintln!("{} MB without rings, {} MB with", near_bytes / 1_000_000, ringed_bytes / 1_000_000);
    // Ground, grass and leaves are green; the sky and the haze grey.
    let green = |pixels: &[u8]| {
        (HEIGHT / 3..HEIGHT)
            .step_by(4)
            .flat_map(|y| (0..WIDTH).step_by(4).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = rgb_at(pixels, x, y);
                g > r + 4 && g > b + 4
            })
            .count()
    };
    let (seen, ringed_seen, all) = (green(&near), green(&ringed), (HEIGHT * 2 / 3 / 4 * WIDTH / 4) as usize);
    eprintln!("ground in the lower two thirds: {seen} of {all} without rings, {ringed_seen} with");
    assert!(seen * 3 < all, "past 24 m there is nothing without rings: {seen} of {all}");
    assert!(ringed_seen * 2 > all && ringed_seen > seen * 10, "the rings carry the ground on: {ringed_seen} of {all}");
    // Where the rings stand in for full detail, within 192 m, its tones are near full detail's, and no line shows where
    // they meet: blocks of 8×8 pixels compared where full detail has ground.
    let reference = render(&mut renderer, &far_view(192.0, 0.0, 12.0), 3);
    keep_image("far in full detail alone.png", &reference);
    let (a, b, c) = (tones(&ringed), tones(&detailed), tones(&reference));
    let ground: Vec<usize> = (0..c.len()).filter(|&i| c[i][1] > c[i][2] + 4).collect();
    let differences: Vec<i32> =
        ground.iter().map(|&i| (0..3).map(|k| (a[i][k] - b[i][k]).abs()).max().unwrap()).collect();
    let mean = differences.iter().sum::<i32>() as f32 / differences.len() as f32;
    let far = differences.iter().filter(|&&d| d > 40).count();
    let average = |t: &[[i32; 3]]| {
        ground
            .iter()
            .fold([0; 3], |s, &i| [s[0] + t[i][0], s[1] + t[i][1], s[2] + t[i][2]])
            .map(|v| v / ground.len() as i32)
    };
    eprintln!(
        "tones against full detail over {} blocks of ground: mean {mean:.2}, {far} differ by over 40; average {:?} against {:?}",
        ground.len(),
        average(&a),
        average(&b)
    );
    assert!(mean < 20.0 && far * 10 < ground.len(), "the rings' tones are full detail's");
    assert!((0..3).all(|k| (average(&a)[k] - average(&b)[k]).abs() < 8), "and as bright and as green on average");
    // Moving the camera a tenth of a metre over the edge of a sector moves the full-detail window and the rings by a
    // sector: what changes then, against the same move within a sector, is what pops where the rings meet.
    let moved = |from: f32, renderer: &mut Renderer| {
        let mut scene = far_view(24.0, 400.0, 12.0);
        let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
        camera.position.z = from - 0.05;
        let before = render(renderer, &scene, 3);
        let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
        camera.position.z = from + 0.05;
        let after = render(renderer, &scene, 3);
        let changed = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| (0..3).any(|i| (rgb_at(&before, x, y)[i] - rgb_at(&after, x, y)[i]).abs() > 24))
            .count();
        (changed, before, after)
    };
    let (within, still_before, still_after) = moved(8.2, &mut renderer);
    keep_image("rings before within a sector.png", &still_before);
    keep_image("rings after within a sector.png", &still_after);
    let (over, before, after) = moved(8.0, &mut renderer);
    keep_image("rings before a sector.png", &before);
    keep_image("rings past a sector.png", &after);
    eprintln!("pixels changed moving 10 cm: {within} within a sector, {over} over the edge of one");
    assert!(over * 4 < within * 5, "nothing pops where the rings meet: {over} pixels change against {within}");
}

/// Rolling Hills with a hundred oaks and pines a hectare at 1920×1080,
/// full detail to 64 m, seen from over a hillside across the land: today's
/// view, and the same with rings to a kilometre and to two; then without
/// the trees. What it keeps,
/// how long its window takes to make, the least of 60 frames, and what
/// making columns costs flying over it.
#[test]
#[ignore]
fn measure_an_endless_volume_seen_far_in_rings() {
    let Ok(mut renderer) = Renderer::new_offscreen(1920, 1080, &RendererSettings::default()) else { return };
    let cases = [(0.0, true), (1024.0, true), (2048.0, true), (0.0, false), (2048.0, false)];
    for (distance, trees) in cases {
        let mut scene = far_view(64.0, distance, 6.0);
        for volume in scene.query::<&mut VoxelVolume>() {
            volume.trees = if trees { oaks_and_pines() } else { Vec::new() };
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
                    if pass.contains("oxel") {
                        let kept = least.entry(pass.clone()).or_insert(f32::MAX);
                        *kept = kept.min(*ms);
                    }
                }
                frame = frame.min(renderer.gpu_frame_ms());
            }
        }
        let name = format!("seen to {distance} m{}", if trees { "" } else { " without trees" });
        eprintln!("{name}: window made in {window:.2} s, {} MB", renderer.stats().voxels.bytes / 1_000_000);
        eprintln!("{name}: still view, least of 60: frame {frame:.2} ms, {least:?}");
        let pixels = renderer.read_pixels().expect("pixels").2;
        if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
            let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}.png"))).unwrap();
            let mut encoder = png::Encoder::new(file, 1920, 1080);
            encoder.set_color(png::ColorType::Rgba);
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        // Flying along x at 10 m/s for nine seconds: every making pass of a frame, the rings' included.
        let mut making = Vec::new();
        for i in 0..540 {
            let camera = scene.query::<(&Camera, &mut Transform)>().into_iter().next().unwrap().1;
            camera.position.x += 10.0 / 60.0;
            renderer.render(&scene, 2.0 + i as f32 / 60.0).unwrap();
            let ms: f32 =
                renderer.timings().iter().filter(|(pass, _)| pass == "Voxel world making").map(|(_, ms)| ms).sum();
            if ms > 0.0 {
                making.push(ms);
            }
        }
        making.sort_by(f32::total_cmp);
        if !making.is_empty() {
            eprintln!(
                "{name}: flying, making on {} of 540 frames: median {:.2} ms, least {:.2}, most {:.2}",
                making.len(),
                making[making.len() / 2],
                making[0],
                making[making.len() - 1]
            );
        }
    }
}

#[test]
fn an_endless_volumes_boxes_banks_areas_covers_and_fitted_prints_are_the_cpus_to_the_voxel() {
    let Some(mut renderer) = renderer() else { return };
    let mut scene = bare_hills_view();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = |x: f32, z: f32| {
        voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(x, 47.0, z), -Vec3::Y, 60.0).expect("the ground").point
    };
    let turn = |degrees: f32| Quat::from_rotation_y(degrees.to_radians());
    // A hut 3 m a side and 2.5 m tall, a voxel's shell with a door, its origin at the middle of its foot.
    let hut = VoxelPrint::from_voxels([48, 40, 48], 16, Vec3::new(24.0, 0.0, 24.0), vec![[180, 150, 110, 255], [90, 60, 40, 255]], VoxelPrintSettings::default(), |c| {
        let wall = c[0] == 0 || c[0] == 47 || c[2] == 0 || c[2] == 47;
        let door = c[2] == 0 && (20..28).contains(&c[0]) && c[1] < 30;
        if c[1] == 39 { 2 } else if wall && !door { 1 } else { 0 }
    });
    let l_shape = vec![[1.0, 9.0], [7.0, 9.0], [7.0, 12.0], [4.0, 12.0], [4.0, 15.0], [1.0, 15.0]];
    let plot = ground(4.0, 12.0).y;
    let edits = vec![
        VoxelEdit::boxed(ground(4.0, 4.0), Vec3::new(3.0, 0.5, 2.0), turn(20.0), Some(VoxelMaterial::Cobbles)).clearing(1.5),
        VoxelEdit::ramp(ground(12.0, 3.0) - Vec3::Y, Vec3::new(2.0, 1.5, 4.0), turn(-30.0), Some(VoxelMaterial::Dirt)),
        VoxelEdit::area(l_shape, plot - 0.5, plot + 0.25, Some(VoxelMaterial::Flagstones)).clearing(2.0),
        VoxelEdit::cover(vec![[10.0, 10.0], [15.0, 10.5], [14.5, 15.0], [10.5, 14.0]], 0.25, Some(VoxelMaterial::Bricks)).clearing(0.5),
        VoxelEdit::printed(Arc::new(hut), ground(8.0, 20.0), turn(12.0)).fitted(Some(VoxelMaterial::Ashlar)),
        VoxelEdit::boxed(ground(16.0, 16.0), Vec3::new(2.0, 1.0, 2.0), Quat::IDENTITY, Some(VoxelMaterial::Planks)).tinted(Some([200, 60, 60])),
        VoxelEdit::boxed(ground(3.0, 18.0) - Vec3::Y * 3.0, Vec3::new(3.0, 2.5, 3.0), turn(45.0), None),
    ];
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(edits.clone());
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("shaped edits from above.png", &pixels);
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let voxel = 1.0 / 16.0;
    let mut wrong = Vec::new();
    for (i, edit) in edits.iter().enumerate() {
        let (least, most) = edit.reach();
        let least = if least.y < -1000.0 { Vec3::new(least.x, 18.0, least.z) } else { least };
        let most = if most.y > 1000.0 { Vec3::new(most.x, 30.0, most.z) } else { most };
        let low = (least / voxel).floor().as_ivec3() - 3;
        let high = (most / voxel).ceil().as_ivec3() + 3;
        let size = (high - low).to_array();
        let reaches = |c: voxear_core::math::IVec3, ground: i32| edit.held(c, voxel, ground) != voxear_render::VoxelHeld::Not;
        let (differ, kept, _) = gpu_against_cpu(&renderer, &scene, (low.to_array(), size), &reaches);
        let count = (size[0] * size[1] * size[2]) as usize;
        eprintln!("edit {i}: {differ} of {count} voxels differ, {kept} kept");
        assert!(kept > 100, "edit {i}: the GPU keeps its voxels: {kept}");
        if differ != 0 {
            wrong.push((i, differ));
        }
    }
    assert!(wrong.is_empty(), "the GPU's voxels are the CPU's but for edits (edit, voxels): {wrong:?}");
    // The cobbled box's top, seen: its stones' three shades and their joints, and nothing else.
    let entity = scene.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap().0;
    let colours_at = |cells: &[[i32; 3]]| {
        let (found, palette) = renderer.voxels_on_gpu(entity, cells).unwrap().unwrap();
        found.into_iter().filter(|&m| m != 0).map(|m| palette[m as usize - 1]).collect::<Vec<_>>()
    };
    let top = |at: Vec3, height: f32| {
        let y = ((at.y + height) / voxel).floor() as i32 - 1;
        let middle = (at / voxel).as_ivec3();
        (-8..8).flat_map(move |dz| (-8..8).map(move |dx| [middle.x + dx, y, middle.z + dz])).collect::<Vec<_>>()
    };
    let cobbles = colours_at(&top(edits[0].at(), 0.5));
    let paving = &voxear_render::VOXEL_PAVING_COLORS;
    assert!(cobbles.len() == 256, "the box's top is all there: {}", cobbles.len());
    assert!(cobbles.iter().all(|c| paving[..4].contains(c)), "cobbles' colours: {cobbles:?}");
    assert!(cobbles.contains(&paving[3]), "with joints between the stones");
    assert!((0..3).filter(|&k| cobbles.contains(&paving[k])).count() >= 2, "in more than one shade");
    // The tinted planks: their average the tint's.
    let planks = colours_at(&top(edits[5].at(), 1.0));
    assert!(planks.len() == 256);
    let mean = [0, 1, 2].map(|i| planks.iter().map(|c| f32::from(c[i])).sum::<f32>() / planks.len() as f32);
    assert!(mean[0] > 150.0 && mean[1] < 90.0 && mean[2] < 90.0, "red planks: {mean:?}");
    // Over the street's level the L is cleared two metres; under it, flagstones.
    let _ = volume;
}

#[test]
fn voxels_dug_and_placed_one_at_a_time_are_the_cpus_and_go_over_the_edits() {
    let Some(mut renderer) = renderer() else { return };
    let mut scene = bare_hills_view();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(8.0, 47.0, 8.0), -Vec3::Y, 60.0).unwrap().point;
    let top = (ground * 16.0).floor().as_ivec3() - voxear_core::math::IVec3::Y;
    // A box of ashlar, a trench dug a voxel wide through it and the ground, and a wall of red bricks built in it.
    let slab = VoxelEdit::boxed(ground, Vec3::new(3.0, 0.5, 3.0), Quat::IDENTITY, Some(VoxelMaterial::Ashlar));
    let mut changes = voxear_render::VoxelChanges::default();
    let red = voxear_render::VoxelKind { material: VoxelMaterial::Bricks, tint: Some([200, 40, 40]) };
    for x in -24..24 {
        for y in -6..9 {
            changes.set(top + voxear_core::math::IVec3::new(x, y, 0), None);
        }
    }
    for x in -6..6 {
        for y in 9..20 {
            changes.set(top + voxear_core::math::IVec3::new(x, y, 3), Some(red));
        }
    }
    {
        let volumes = scene.query::<&mut VoxelVolume>();
        let volume = volumes.into_iter().next().unwrap();
        volume.edits = Arc::new(vec![slab]);
        volume.changes = Arc::new(changes);
    }
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("voxels dug and placed from above.png", &pixels);
    let low = (top - voxear_core::math::IVec3::new(30, 10, 30)).to_array();
    let made = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let reaches = |c: voxear_core::math::IVec3, ground: i32| {
        made.changes.get(c).is_some() || made.edits[0].held(c, 1.0 / 16.0, ground) != voxear_render::VoxelHeld::Not
    };
    let (differ, kept, found) = gpu_against_cpu(&renderer, &scene, (low, [60, 40, 60]), &reaches);
    eprintln!("{differ} differ, {kept} kept");
    assert_eq!(differ, 0, "the GPU's voxels are the CPU's");
    // The trench is open through the slab, and the wall stands red.
    let entity = scene.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap().0;
    let (_, palette) = renderer.voxels_on_gpu(entity, &[]).unwrap().unwrap();
    let at = |c: voxear_core::math::IVec3| {
        let d = c - IVec3::from(low);
        found[(d.x + 60 * (d.y + 40 * d.z)) as usize]
    };
    use voxear_core::math::IVec3;
    assert_eq!(at(top + IVec3::new(0, 4, 0)), 0, "dug through the slab");
    let wall = at(top + IVec3::new(0, 19, 3));
    assert!(wall != 0, "the wall's top");
    let [r, g, b, _] = palette[wall as usize - 1];
    // A brick's shade or its joint's, both in the red tint.
    assert!(r > 150 && r > g + 100 && r > b + 100, "red bricks: {r} {g} {b}");
}

#[test]
fn sixty_kinds_of_building_each_keep_their_own_colours() {
    let Some(mut renderer) = renderer() else { return };
    let mut scene = bare_hills_view();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    // A house a metre and a half a side, sixteen colours in stripes, printed in sixty tints in a square round the camera.
    let colours: Vec<[u8; 4]> = (0..16u8).map(|k| [80 + k * 8, 120 - k * 4, 60 + k * 5, 255]).collect();
    let house = Arc::new(VoxelPrint::from_voxels([24, 24, 24], 16, Vec3::new(12.0, 0.0, 12.0), colours.clone(), VoxelPrintSettings::default(), |c| {
        (c[0] % 16) as u8 + 1
    }));
    let mut edits = Vec::new();
    let mut placed = Vec::new();
    for k in 0..60u32 {
        let (x, z) = ((k % 8) as f32 * 2.0 - 0.0, (k / 8) as f32 * 2.0 - 0.0);
        let at = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(x, 47.0, z), -Vec3::Y, 60.0).unwrap().point;
        let tint = [(k * 37 % 200 + 40) as u8, (k * 91 % 200 + 40) as u8, (k * 53 % 200 + 40) as u8];
        edits.push(VoxelEdit::printed(house.clone(), at, Quat::IDENTITY).tinted(Some(tint)));
        placed.push((at, tint));
    }
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(edits);
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("sixty tints from above.png", &pixels);
    let entity = scene.query_ref::<(voxear_ecs::Entity, &VoxelVolume)>().into_iter().next().unwrap().0;
    let mean = [0, 1, 2].map(|i| colours.iter().map(|c| f32::from(c[i])).sum::<f32>() / 16.0);
    let mut wrong = 0;
    let mut looked = 0;
    let edits = scene.query::<&VoxelVolume>().into_iter().next().unwrap().edits.clone();
    for (edit, (at, tint)) in edits.iter().zip(&placed) {
        // The house's roof, all of its sixteen stripes, each its colour in the house's tint, as the CPU says it is.
        let middle = (*at * 16.0).floor().as_ivec3();
        let cells: Vec<[i32; 3]> = (-13..13).flat_map(|dx| (20..26).map(move |dy| [middle.x + dx, middle.y + dy, middle.z])).collect();
        let (found, palette) = renderer.voxels_on_gpu(entity, &cells).unwrap().unwrap();
        for (cell, place) in cells.iter().zip(found) {
            let voxear_render::VoxelHeld::Colour(index) = edit.held(voxear_core::math::IVec3::from(*cell), 1.0 / 16.0, -1000) else { continue };
            if place == 0 {
                continue;
            }
            let colour = colours[usize::from(index) - 1];
            let want = [0, 1, 2].map(|i| (f32::from(colour[i]) / mean[i] * f32::from(tint[i])).round().clamp(0.0, 255.0) as u8);
            looked += 1;
            if palette[place as usize - 1][..3] != want {
                wrong += 1;
            }
        }
    }
    assert!(looked > 60 * 20, "the roofs are seen: {looked}");
    eprintln!("{wrong} of {looked} roof voxels not in their own colour");
    assert_eq!(wrong, 0, "every house in its own colours");
}

#[test]
fn a_column_takes_over_a_thousand_edits_as_the_cpu_does() {
    let Some(mut renderer) = renderer() else { return };
    let mut scene = bare_hills_view();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(6.0, 47.0, 6.0), -Vec3::Y, 60.0).unwrap().point;
    // A stepped pyramid of 1,330 slabs a quarter of a metre a side and an eighth tall, in one column of 4 m.
    let mut edits = Vec::new();
    for layer in 0..12 {
        let side = 19 - layer;
        for k in 0..side * 7 {
            let (i, j) = (k % side, k / side);
            let at = Vec3::new(4.25 + i as f32 * 0.17 + layer as f32 * 0.08, ground.y + layer as f32 * 0.125, 4.25 + j as f32 * 0.4);
            let material = [VoxelMaterial::Ashlar, VoxelMaterial::Bricks, VoxelMaterial::Cobbles][(layer + k) as usize % 3];
            edits.push(VoxelEdit::boxed(at, Vec3::new(0.25, 0.125, 0.25), Quat::IDENTITY, Some(material)));
        }
    }
    let count = edits.len();
    assert!(count > 1024, "{count} edits");
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().edits = Arc::new(edits);
    render(&mut renderer, &scene, 3);
    let low = ((Vec3::new(4.0, ground.y, 4.0) * 16.0).floor().as_ivec3() - 4).to_array();
    let made = scene.query::<&VoxelVolume>().into_iter().next().unwrap().edits.clone();
    let boxes: Vec<(Vec3, Vec3)> = made.iter().map(|edit| edit.reach()).collect();
    let reaches = |c: voxear_core::math::IVec3, ground: i32| {
        let p = (c.as_vec3() + 0.5) / 16.0;
        made.iter().zip(&boxes).any(|(edit, (least, most))| {
            p.cmpge(*least).all() && p.cmple(*most).all() && edit.held(c, 1.0 / 16.0, ground) != voxear_render::VoxelHeld::Not
        })
    };
    let (differ, kept, _) = gpu_against_cpu(&renderer, &scene, (low, [72, 36, 72]), &reaches);
    eprintln!("{count} edits: {differ} voxels differ, {kept} kept");
    assert!(kept > 1000);
    assert_eq!(differ, 0, "every edit drawn as the CPU finds it");
}

#[test]
fn an_endless_volumes_ground_far_from_its_obj_is_drawn_as_near_it() {
    // The same ground seen 24 km from the volume's Obj, and with the volume's Obj under the camera and its offset there.
    let Some(mut far_renderer) = renderer() else { return };
    let world = Arc::new(without_rivers(&starter("Rolling Hills")));
    let far = 24000.0;
    let mut scene = generated_view(world.clone(), far + 8.0, far + 8.0, 6.0, 25.0);
    let made = render(&mut far_renderer, &scene, 3);
    keep_image("far from its obj.png", &made);
    drop(far_renderer);
    let Some(mut near_renderer) = renderer() else { return };
    let ground = world.height_at(far + 8.0, far + 8.0);
    scene = generated_view(world, 8.0, 8.0, 6.0, 25.0);
    for volume in scene.query::<&mut VoxelVolume>() {
        let generator = volume.generator.as_mut().unwrap();
        generator.at = [far, far];
        generator.floor = ground - 24.0;
    }
    let near = render(&mut near_renderer, &scene, 3);
    keep_image("near its obj.png", &near);
    // Grass and flowers are placed by the voxel's place in the generator's world, which the offset moves, so the two
    // are not the same pixel for pixel: the ground's shading is the same over squares of 20 pixels, and a streaked
    // picture has many more pixels the same as the one beside them than as the one under them.
    let block = |pixels: &[u8], bx: u32, by: u32| {
        let mut sum = [0i32; 3];
        for y in by * 20..by * 20 + 20 {
            for x in bx * 20..bx * 20 + 20 {
                let c = rgb_at(pixels, x, y);
                (0..3).for_each(|i| sum[i] += c[i]);
            }
        }
        sum.map(|s| s / 400)
    };
    let blocks = (0..HEIGHT / 20).flat_map(|by| (0..WIDTH / 20).map(move |bx| (bx, by)));
    let differ = blocks
        .filter(|&(bx, by)| {
            let (a, b) = (block(&made, bx, by), block(&near, bx, by));
            (0..3).any(|i| (a[i] - b[i]).abs() > 12)
        })
        .count();
    let runs = |pixels: &[u8]| {
        let same = |a: [i32; 3], b: [i32; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= 2);
        let (mut across, mut down) = (0, 0);
        for y in 180..HEIGHT - 1 {
            for x in 0..WIDTH - 1 {
                across += usize::from(same(rgb_at(pixels, x, y), rgb_at(pixels, x + 1, y)));
                down += usize::from(same(rgb_at(pixels, x, y), rgb_at(pixels, x, y + 1)));
            }
        }
        across as f32 / down.max(1) as f32
    };
    let (far_runs, near_runs) = (runs(&made), runs(&near));
    eprintln!("{differ} of 576 squares differ; across over down {far_runs:.2} far, {near_runs:.2} near");
    assert!(differ * 8 < 576, "{differ} squares of 20 pixels differ from the ground drawn near its Obj");
    assert!(far_runs < near_runs * 1.2, "streaked: {far_runs:.2} pixels the same across for each down, {near_runs:.2} near");
}

#[test]
fn trees_felled_are_gone_over_their_stumps_on_the_gpu_as_on_the_cpu_and_cut_grass_fades() {
    let Some(mut renderer) = renderer() else { return };
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut trees = oaks_and_pines();
    for kind in &mut trees {
        (kind.wood, kind.leaves) = ([40, 40, 230], [230, 40, 40]);
    }
    let mut scene = generated_view(hills, 8.0, 8.0, 20.0, 90.0);
    {
        let mut volumes = scene.query::<&mut VoxelVolume>();
        let volume = volumes.into_iter().next().unwrap();
        volume.trees = trees;
        let ground = volume.ground.as_mut().unwrap();
        (ground.flowers, ground.grass) = (0.0, 1.0);
    }
    scene.spawn((WindZone { strength: 0.0, ..WindZone::default() }, Transform::default()));
    let before = render(&mut renderer, &scene, 3);
    keep_image("felling before.png", &before);
    let (entity, volume) = scene
        .query_ref::<(voxear_ecs::Entity, &VoxelVolume)>()
        .into_iter()
        .map(|(entity, volume)| (entity, volume.clone()))
        .next()
        .unwrap();
    // Every tree in view cut through its trunk, a little over the ground.
    let trunks = voxear_render::voxel_tree_trunks(&volume, Vec3::new(-14.0, 0.0, -4.0), Vec3::new(30.0, 0.0, 20.0));
    assert!(trunks.len() >= 3, "trees to fell: {}", trunks.len());
    // And a ball of grass cut beside each, the blades over the ground's top at its middle counted before and after.
    let solids = voxear_render::VoxelSolids::of(&volume).unwrap();
    let mown: Vec<Vec3> = trunks
        .iter()
        .map(|trunk| {
            let at = *trunk + Vec3::new(2.5, 0.0, 0.0);
            let h = solids.heights([(at.x * 16.0).floor() as i32, (at.z * 16.0).floor() as i32], [1, 1])[0];
            Vec3::new(at.x, (h + 1) as f32 / 16.0, at.z)
        })
        .collect();
    let blades_at = |renderer: &Renderer, solids: &voxear_render::VoxelSolids| {
        let mut blades = 0;
        for at in &mown {
            let c = (*at * 16.0).floor().as_ivec3();
            for column in [[c.x, c.z], [c.x + 3, c.z], [c.x, c.z + 3], [c.x - 3, c.z - 3]] {
                let h = solids.heights(column, [1, 1])[0];
                let cells: Vec<[i32; 3]> = (1..12).map(|dy| [column[0], h + dy, column[1]]).collect();
                let (found, _) = renderer.voxels_on_gpu(entity, &cells).unwrap().expect("an endless volume");
                blades += found.iter().filter(|&&m| m != 0).count();
            }
        }
        blades
    };
    let blades_before = blades_at(&renderer, &solids);
    let mut cut = volume.clone();
    let mut pieces = 0;
    let started = std::time::Instant::now();
    for trunk in &trunks {
        let (changes, done) = voxear_render::voxel_cut(&cut, &voxear_render::VoxelCut::Ball { at: *trunk + Vec3::Y * 1.2, radius: 0.8 });
        pieces += done.pieces.len();
        cut.changes = Arc::new(changes);
    }
    let felling = started.elapsed();
    for at in &mown {
        let (changes, _) = voxear_render::voxel_cut(&cut, &voxear_render::VoxelCut::Ball { at: *at, radius: 0.6 });
        cut.changes = Arc::new(changes);
    }
    eprintln!("{} trees felled in {pieces} pieces, in {felling:?}", trunks.len());
    // Where two trees grow into each other, one may hold the other up.
    assert!(pieces * 10 >= trunks.len() * 8, "most come loose: {pieces} pieces of {} trees", trunks.len());
    // And a slice a quarter of a metre over the ground beside each, the other way: stubble under it.
    let stubble: Vec<Vec3> = mown.iter().map(|at| *at - Vec3::new(5.0, 0.0, 0.0)).collect();
    for at in &stubble {
        let h = solids.heights([(at.x * 16.0).floor() as i32, (at.z * 16.0).floor() as i32], [1, 1])[0];
        let slice = voxear_render::VoxelCut::Slice { at: Vec3::new(at.x, (h + 1) as f32 / 16.0 + 0.25, at.z), across: Vec3::Y, radius: 0.6 };
        let (changes, _) = voxear_render::voxel_cut(&cut, &slice);
        cut.changes = Arc::new(changes);
    }
    *scene.query::<&mut VoxelVolume>().into_iter().next().unwrap() = cut.clone();
    let after = render(&mut renderer, &scene, 3);
    keep_image("felling after.png", &after);
    let leaves = |pixels: &[u8]| {
        (0..HEIGHT)
            .step_by(2)
            .flat_map(|y| (0..WIDTH).step_by(2).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let [r, g, b] = rgb_at(pixels, x, y).map(|c| c as f32);
                r / (r + g + b).max(1.0) > 0.5
            })
            .count()
    };
    let (were, are) = (leaves(&before), leaves(&after));
    eprintln!("leaves at {were} pixels before, {are} after");
    assert!(are * 2 < were, "the felled trees' crowns are gone: {are} of {were}");
    // Up each trunk: its stump under the cut on the GPU and the CPU, nothing of it over the cut on either; and the
    // grass about it, which was cut, is gone.
    let solids = voxear_render::VoxelSolids::of(&cut).unwrap();
    let (mut agree, mut all, mut stumps) = (0, 0, 0);
    for trunk in &trunks {
        let c = (*trunk * 16.0).floor().as_ivec3();
        let cells: Vec<[i32; 3]> = (-2..80).map(|dy| [c.x, c.y + dy, c.z]).collect();
        let (found, _) = renderer.voxels_on_gpu(entity, &cells).unwrap().expect("an endless volume");
        let mut solid = vec![false; cells.len()];
        solids.fill([c.x, c.y - 2, c.z], [1, cells.len() as i32, 1], &mut solid);
        for (i, (&gpu, &cpu)) in found.iter().zip(&solid).enumerate() {
            all += 1;
            agree += usize::from((gpu != 0) == cpu);
            if i >= 2 && i < 6 && gpu != 0 {
                stumps += 1;
            }
        }
    }
    let blades = blades_at(&renderer, &solids);
    // Under the slice the blades stand four voxels tall at most; nothing over that.
    let (mut short, mut over) = (0, 0);
    for at in &stubble {
        let c = (*at * 16.0).floor().as_ivec3();
        let h = solids.heights([c.x, c.z], [1, 1])[0];
        let cells: Vec<[i32; 3]> = (1..12).map(|dy| [c.x, h + dy, c.z]).collect();
        let (found, _) = renderer.voxels_on_gpu(entity, &cells).unwrap().expect("an endless volume");
        short += found[..4].iter().filter(|&&m| m != 0).count();
        over += found[4..].iter().filter(|&&m| m != 0).count();
    }
    eprintln!("stubble: {short} voxels under the slice, {over} over it");
    assert!(short > 0 && over == 0, "the slice leaves stubble: {short} under, {over} over");
    eprintln!("{agree} of {all} voxels up the trunks agree; {stumps} stump voxels; {blades_before} blade voxels, {blades} left");
    assert!(blades_before > mown.len() * 4, "grass grew there: {blades_before}");
    assert!(agree * 100 >= all * 97, "the GPU's felled trees are the CPU's: {agree} of {all}");
    assert!(stumps * 2 >= trunks.len() * 4, "the stumps stand: {stumps}");
    assert_eq!(blades, 0, "the cut grass is gone");
}

#[test]
fn a_volume_is_turned_with_its_obj_its_voxels_lit_and_shadowing_as_they_lie() {
    let Some(mut renderer) = renderer() else { return };
    // Turned a third of the way round the up axis about its middle: the field from above is the same field, its
    // pillar's top red where it was and its shadow still to the east, as the sun is where it was.
    let mut world = field(false);
    let turn = Quat::from_rotation_y(30f32.to_radians());
    for (_, place) in world.query::<(&VoxelVolume, &mut Transform)>() {
        place.rotation = turn;
        place.position = Vec3::new(8.0, 0.0, 8.0) - turn * Vec3::new(8.0, 0.0, 8.0);
    }
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel field turned.png", &pixels);
    check_field(&pixels);
    // The volume's square turned too: its corner, unturned on the ground, is sky now, and ground lies past its old side.
    let at = |x: f32, z: f32| {
        let (px, py) = from_above(20.0, x, z);
        rgb_at(&pixels, px.min(WIDTH - 1), py.min(HEIGHT - 1))
    };
    let green = |[r, g, b]: [i32; 3]| g > r + 20 && g > b + 20;
    assert!(!green(at(0.5, 0.5)), "the unturned corner is not ground now: {:?}", at(0.5, 0.5));
    assert!(green(at(8.0, -1.0)), "the turned square reaches past the old side: {:?}", at(8.0, -1.0));

    // A pillar tipped over onto its side by turning its volume a quarter round Z: red along X, a metre wide.
    let mut world = field(false);
    let tipped = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    for (volume, place) in world.query::<(&mut VoxelVolume, &mut Transform)>() {
        volume.ground = None;
        place.rotation = tipped;
        // The pillar's foot (8, 1, 8) of the volume lies at (9, 1, 8) of the world.
        place.position = Vec3::new(9.0, 1.0, 8.0) - tipped * Vec3::new(8.0, 1.0, 8.0);
    }
    let pixels = render(&mut renderer, &world, 4);
    keep_image("voxel pillar tipped.png", &pixels);
    let red = |x: f32, z: f32| {
        let (px, py) = from_above(20.0, x, z);
        let [r, g, b] = rgb_at(&pixels, px, py);
        r > g * 2 && r > b * 2 && r > 40
    };
    for x in [6.5, 7.5, 8.5] {
        assert!(red(x, 8.0), "the tipped pillar lies along X at {x}");
    }
    assert!(!red(8.0, 6.0) && !red(8.0, 10.0) && !red(4.0, 8.0), "and is a metre wide, three long");
}

#[test]
fn a_house_blown_in_is_drawn_as_the_cpu_finds_it_after_what_came_loose_was_dug_out() {
    use voxear_core::math::IVec3;
    let Some(mut renderer) = renderer() else { return };
    let mut scene = bare_hills_view();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let ground = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(8.0, 47.0, 8.0), -Vec3::Y, 60.0).unwrap().point;
    // A house of bricks with a roof of planks, sunk a little into the hill.
    let foot = ground - Vec3::Y * 0.5;
    let edits = vec![
        VoxelEdit::boxed(foot, Vec3::new(5.0, 3.0, 4.0), Quat::IDENTITY, Some(VoxelMaterial::Bricks)),
        VoxelEdit::boxed(foot + Vec3::Y * 0.5, Vec3::new(4.5, 2.5, 3.5), Quat::IDENTITY, None),
        VoxelEdit::boxed(foot + Vec3::Y * 3.0, Vec3::new(5.5, 0.25, 4.5), Quat::IDENTITY, Some(VoxelMaterial::Planks)),
    ];
    let corner = foot + Vec3::new(-2.5, 2.0, -2.0);
    let mut built = volume.clone();
    built.edits = Arc::new(edits);
    // Blown out at a corner, and every round of what gave way dug out, as the game does a fifth of a second apart.
    let mut memory = voxear_render::VoxelStrainMemory::default();
    let blast = voxear_render::VoxelBlast { at: corner, radius: 1.8 };
    let (mut changes, done) = voxear_render::voxel_blast(&built, blast, &mut memory);
    let rounds = &done.settled.strain.loose;
    let loose: usize = rounds.iter().map(|round| round.dig.len()).sum();
    eprintln!("{} dug, {} rounds, {loose} voxels gave way", done.dug, rounds.len());
    assert!(loose > 1000, "{loose}");
    for c in rounds.iter().flat_map(|round| round.dig.iter()) {
        changes.set(*c, None);
    }
    let changed = changes.clone();
    {
        let volumes = scene.query::<&mut VoxelVolume>();
        let volume = volumes.into_iter().next().unwrap();
        volume.edits = built.edits.clone();
        volume.changes = Arc::new(changes);
    }
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("voxels house blown in.png", &pixels);
    let low = ((foot - Vec3::new(3.5, 1.0, 3.0)) * 16.0).floor().as_ivec3();
    let made = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let reaches = |c: IVec3, ground: i32| {
        changed.get(c).is_some() || made.edits.iter().any(|edit| edit.held(c, 1.0 / 16.0, ground) != voxear_render::VoxelHeld::Not)
    };
    let (differ, kept, _) = gpu_against_cpu(&renderer, &scene, (low.to_array(), [112, 80, 96]), &reaches);
    eprintln!("{differ} differ, {kept} kept");
    assert!(kept > 1000);
    assert_eq!(differ, 0, "the GPU's voxels are the CPU's");
}
