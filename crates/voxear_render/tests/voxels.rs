//! Voxel Volumes drawn on a real GPU, offscreen. Each test skips itself
//! (with a note) on machines without a usable Vulkan device.

use std::sync::Arc;

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    AmbientOcclusion, AntiAliasing, AntiAliasingMode, BoxBone, BoxModel, Camera, Light, MaterialDesc, MeshRenderer,
    ModelBox, PostProcess, Primitive, Renderer, RendererSettings, Sky, VoxelEdit, VoxelGenerator, VoxelGround,
    VoxelMaterial, VoxelPrint, VoxelPrintMaterial, VoxelPrintSettings, VoxelStamp, VoxelTrees, VoxelVolume, Voxels,
    WindZone,
};
use voxear_scene::Transform;
use voxear_terrain::blocks::plant::{PlantDef, PlantKind};
use voxear_terrain::generate::{
    Cells, Landmass, OwnVoxels, Preset, WorldGen,
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
        generator: Some(VoxelGenerator { world, floor: ground - 24.0, at: [0.0, 0.0] }),
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
                let floor =
                    scene.query_ref::<&VoxelVolume>().into_iter().next().unwrap().generator.as_ref().unwrap().floor;
                scene.spawn((
                    voxear_render::Ocean::default(),
                    Transform::from_position(Vec3::new(0.0, sea - floor, 0.0)),
                ));
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
        VoxelTrees { plant: Arc::new(oak), per_hectare: 60.0, wood: [104, 78, 56], leaves: [70, 128, 46] },
        VoxelTrees { plant: Arc::new(pine), per_hectare: 40.0, wood: [92, 64, 44], leaves: [38, 92, 52] },
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

#[test]
fn the_cpus_trees_stand_where_the_gpu_draws_them() {
    let Some(mut renderer) = renderer() else { return };
    // A wood from 20 m over it, straight down, its leaves red and its wood blue, in still air.
    let mut trees = oaks_and_pines();
    for kind in &mut trees {
        (kind.wood, kind.leaves) = ([40, 40, 230], [230, 40, 40]);
    }
    let hills = Arc::new(without_rivers(&starter("Rolling Hills")));
    let mut scene = generated_view(hills, 8.0, 8.0, 20.0, 90.0);
    scene.query::<&mut VoxelVolume>().into_iter().next().unwrap().trees = trees;
    scene.spawn((WindZone { strength: 0.0, ..WindZone::default() }, Transform::default()));
    let pixels = render(&mut renderer, &scene, 3);
    keep_image("cpu trees from above.png", &pixels);
    let (camera, place) = scene.query::<(&Camera, &Transform)>().into_iter().next().map(|(c, t)| (c.clone(), t.clone())).unwrap();
    let volume = scene.query::<&VoxelVolume>().into_iter().next().unwrap().clone();
    let bare = VoxelVolume { trees: Vec::new(), ..volume.clone() };
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let tan = (camera.fov_y_for(aspect) * 0.5).tan();
    let (forward, up) = (place.rotation * Vec3::Z, place.rotation * Vec3::Y);
    let right = up.cross(forward);
    let (mut agree, mut all, mut gpu_trees, mut cpu_trees) = (0, 0, 0, 0);
    let started = std::time::Instant::now();
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
            let gpu = sum <= 6.0 || r / sum > 0.5 || b / sum > 0.5;
            all += 1;
            agree += usize::from(cpu == gpu);
            gpu_trees += usize::from(gpu);
            cpu_trees += usize::from(cpu);
        }
    }
    eprintln!("{agree} of {all} agree; trees at {gpu_trees} on the GPU, {cpu_trees} on the CPU, in {:?}", started.elapsed());
    assert!(gpu_trees * 4 > all, "the wood covers the view: {gpu_trees} of {all}");
    assert!(agree * 100 >= all * 95, "the CPU's trees are the GPU's: {agree} of {all} agree");
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
}

/// A house 4 m square and 3 m tall printed on flat endless ground at
/// `(8, 2, 8)`: stone walls a quarter of a metre thick, with what `has`
/// says: a roof as thick; a door 1 m wide and 2 m tall in its east wall; a
/// lantern of glowing voxels in its north-west corner on the floor; a pane
/// of glass 1 m square in its west wall; and a red pillar inside behind it.
fn house(has: House) -> Arc<VoxelPrint> {
    let House { roof, door: doorway, lantern, window, pillar } = has;
    let stone = [150, 146, 138, 255];
    let flame = [255, 186, 80, 255];
    let glass = [196, 222, 230, 255];
    let mut settings = VoxelPrintSettings::default();
    settings.materials = vec![
        VoxelPrintMaterial { colour: [255, 186, 80], glass: 0, light: 14 },
        VoxelPrintMaterial { colour: [196, 222, 230], glass: 14, light: 0 },
    ];
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
    let mut shot = |has: House, name: &str| {
        let pixels = render(&mut renderer, &printed_from_above(house(has), height), 12);
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
