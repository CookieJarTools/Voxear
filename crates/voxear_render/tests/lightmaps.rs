//! Baked lighting on a real GPU: a closed room lit by the sun through one
//! window bakes the light it bounces onto its ceiling and walls, by the
//! GPU's rays and in software; light probes baked in it light a ball that
//! moves through it, brighter near the sunlit floor; and the same room bakes
//! the same lightmaps every time. Each test skips itself (with a note) on
//! machines without a usable Vulkan device.

use std::collections::BTreeMap;

use voxear_core::math::{Mat4, Quat, Vec3, camera::lh::proj::vulkan};
use voxear_ecs::{Entity, World};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, BakedLighting, Camera, GiSource, Light, LightBake, LightProbeGroup, Lightmap,
    LightmapBake, MaterialDesc, MeshRenderer, PostProcess, Primitive, ProbeFill, Renderer, RendererSettings, Sky,
};
use voxear_scene::Transform;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const FRAME: f32 = 1.0 / 60.0;
const EYE: Vec3 = Vec3::new(0.0, 1.5, -2.9);

fn renderer(software: bool) -> Option<Renderer> {
    match Renderer::new_offscreen(WIDTH, HEIGHT, &RendererSettings { software_rays: software, ..Default::default() }) {
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

fn run(renderer: &mut Renderer, world: &World, time: &mut f32, frames: u32) {
    for _ in 0..frames {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
    }
}

fn camera_rotation() -> Quat {
    Quat::from_rotation_x(0.09)
}

/// Where `point` lands in the picture, from the room's camera.
fn pixel_of(point: Vec3, eye: Vec3, rotation: Quat) -> (u32, u32) {
    let view = Mat4::from_rotation_translation(rotation, eye).inverse();
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), aspect, 0.1) * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    ((x * WIDTH as f32) as u32, (y * HEIGHT as f32) as u32)
}

/// The brightness round a pixel, its channels added, averaged over 7×7.
fn brightness(pixels: &[u8], (x, y): (u32, u32)) -> f32 {
    let mut sum = 0u32;
    for dy in 0..7 {
        for dx in 0..7 {
            let at = (((y + dy - 3) * WIDTH + x + dx - 3) * 4) as usize;
            sum += u32::from(pixels[at]) + u32::from(pixels[at + 1]) + u32::from(pixels[at + 2]);
        }
    }
    sum as f32 / 49.0
}

/// A closed room six metres square and three high, inside from -3 to 3 on
/// x and z, its floor at 0, with one window in its +x wall, 1.5 m wide and
/// 1.2 m high from a metre up, through which a low sun shines onto the
/// floor. The sky gives no light of its own, so all that reaches the
/// ceiling is what the sunlit floor bounces. Every wall has a Lightmap.
struct Room {
    world: World,
    slabs: Vec<Entity>,
}

fn build_room(renderer: &mut Renderer, source: GiSource) -> Room {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(EYE, camera_rotation(), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grey = renderer
        .add_material(&MaterialDesc { base_color: [0.7, 0.7, 0.7, 1.0], roughness: 0.9, ..MaterialDesc::default() })
        .unwrap();
    let mut slabs = Vec::new();
    let mut slab = |world: &mut World, low: Vec3, high: Vec3| {
        let transform = Transform::new((low + high) * 0.5, Quat::IDENTITY, high - low);
        slabs.push(world.spawn((MeshRenderer::new(cube, grey), transform, Lightmap::default())));
    };
    let t = 0.2;
    slab(&mut world, Vec3::new(-3.0 - t, -t, -3.0 - t), Vec3::new(3.0 + t, 0.0, 3.0 + t));
    slab(&mut world, Vec3::new(-3.0 - t, 3.0, -3.0 - t), Vec3::new(3.0 + t, 3.0 + t, 3.0 + t));
    slab(&mut world, Vec3::new(-3.0 - t, 0.0, -3.0 - t), Vec3::new(-3.0, 3.0, 3.0 + t));
    slab(&mut world, Vec3::new(-3.0, 0.0, -3.0 - t), Vec3::new(3.0, 3.0, -3.0));
    slab(&mut world, Vec3::new(-3.0, 0.0, 3.0), Vec3::new(3.0, 3.0, 3.0 + t));
    // The +x wall, round its window.
    slab(&mut world, Vec3::new(3.0, 0.0, -3.0 - t), Vec3::new(3.0 + t, 3.0, -0.75));
    slab(&mut world, Vec3::new(3.0, 0.0, 0.75), Vec3::new(3.0 + t, 3.0, 3.0 + t));
    slab(&mut world, Vec3::new(3.0, 0.0, -0.75), Vec3::new(3.0 + t, 1.0, 0.75));
    slab(&mut world, Vec3::new(3.0, 2.2, -0.75), Vec3::new(3.0 + t, 3.0, 0.75));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(-1.0, -0.7, 0.0).normalize());
    world.spawn((
        Light { softness: 0.0, ..Light::directional(Vec3::ONE, 3.0) },
        Transform::new(Vec3::ZERO, sun, Vec3::ONE),
    ));
    world.insert_resource(Sky {
        zenith: Vec3::ZERO,
        horizon: Vec3::ZERO,
        ground: Vec3::ZERO,
        ambient: 0.0,
        ..Sky::default()
    });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.reflections.enabled = false;
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    // Two stops up: what only bounced light reaches is dim, and the tonemapper's toe would crush it.
    post.exposure.compensation = 2.0;
    let gi = &mut post.global_illumination;
    (gi.enabled, gi.source, gi.samples, gi.bounces, gi.texels_per_metre, gi.lightmap_size) =
        (true, source, 96, 3, 8.0, 512);
    world.insert_resource(post);
    Room { world, slabs }
}

/// Bakes the world's lighting, fast, and returns what it made.
fn bake(renderer: &mut Renderer, world: &World, time: &mut f32) -> LightBake {
    run(renderer, world, time, 3);
    renderer.set_light_bake_pace(16);
    let (parts, texels, probes) = renderer.start_light_bake(world).expect("the bake starts");
    assert!(parts > 0 && texels > 0, "{parts} parts, {texels} texels, {probes} probes");
    for _ in 0..600 {
        run(renderer, world, time, 1);
        if let Some(made) = renderer.take_light_bake() {
            return made.expect("the bake finishes");
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the bake did not finish");
}

/// Puts a bake on the world, as the engine does when the level opens.
fn put_on(world: &mut World, bake: &LightBake) {
    let mut places: BTreeMap<u64, (Entity, Vec<Option<voxear_render::LightmapPlace>>)> = BTreeMap::new();
    for &(entity, part, place) in &bake.places {
        let (_, list) = places.entry(entity.to_bits()).or_insert_with(|| (entity, Vec::new()));
        list.resize(list.len().max(part as usize + 1), None);
        list[part as usize] = Some(place);
    }
    for (_, (entity, list)) in places {
        world.insert(entity, LightmapBake(list.into()));
    }
    world.insert_resource(BakedLighting::new(bake.atlases.clone(), bake.probes.clone()));
}

fn bounces_through_the_window(software: bool) {
    let Some(mut renderer) = renderer(software) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer, GiSource::Baked);
    run(&mut renderer, &room.world, &mut time, 4);
    let unbaked = renderer.read_pixels().unwrap().2;
    let made = bake(&mut renderer, &room.world, &mut time);
    assert_eq!(made.places.len(), room.slabs.len(), "a lightmap place for every slab");
    eprintln!("baked {} texels in {:.2} s ({:.2} s tracing)", made.texels, made.seconds, made.tracing_seconds);
    put_on(&mut room.world, &made);
    run(&mut renderer, &room.world, &mut time, 3);
    let baked = renderer.read_pixels().unwrap().2;
    let name = if software { "software" } else { "traced" };
    keep_image(&format!("room through a window, unbaked ({name}).png"), &unbaked);
    keep_image(&format!("room through a window, baked ({name}).png"), &baked);
    let at = |point: Vec3| pixel_of(point, EYE, camera_rotation());
    let ceiling = at(Vec3::new(0.0, 3.0, 2.0));
    let far_wall = at(Vec3::new(-1.5, 1.4, 3.0));
    let patch = at(Vec3::new(0.8, 0.0, 0.0));
    let shaded_floor = at(Vec3::new(-2.0, 0.0, 2.0));
    // The sunlit patch is lit either way; everything the sun does not reach is black unbaked.
    assert!(brightness(&unbaked, patch) > 150.0, "the sun lights the floor: {}", brightness(&unbaked, patch));
    for (what, place) in [("ceiling", ceiling), ("far wall", far_wall), ("shaded floor", shaded_floor)] {
        let (before, after) = (brightness(&unbaked, place), brightness(&baked, place));
        eprintln!("{what}: {before} unbaked, {after} baked");
        assert!(before < 4.0, "unbaked, the {what} takes no light: {before}");
        assert!(after > 40.0, "baked, the light bounced onto the {what} lights it: {after}");
    }
    // Bounced light is brighter near where it bounced from than across the room.
    let near = brightness(&baked, at(Vec3::new(0.8, 3.0, 1.0)));
    assert!(near > brightness(&baked, at(Vec3::new(-2.6, 3.0, 2.6))), "the ceiling over the patch is the brighter");
}

#[test]
fn a_closed_room_lit_through_a_window_bakes_its_bounced_light() {
    bounces_through_the_window(false);
}

#[test]
fn a_closed_room_bakes_its_bounced_light_traced_in_software() {
    bounces_through_the_window(true);
}

#[test]
fn the_same_room_bakes_the_same_lightmaps_every_time() {
    let Some(mut renderer) = renderer(false) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer, GiSource::Baked);
    room.world.spawn((
        LightProbeGroup::default(),
        Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, Vec3::new(5.0, 2.0, 5.0)),
    ));
    let first = bake(&mut renderer, &room.world, &mut time);
    let second = bake(&mut renderer, &room.world, &mut time);
    assert!(first.atlases == second.atlases, "the lightmaps come out the same, texel for texel");
    assert_eq!(first.places, second.places);
    assert_eq!(first.probes, second.probes, "and the probes");
}

#[test]
fn light_probes_light_a_ball_moving_through_the_room() {
    let Some(mut renderer) = renderer(false) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer, GiSource::Baked);
    let fill = Some(ProbeFill { spacing: 1.0 });
    let group = LightProbeGroup { points: Vec::new(), fill };
    room.world.spawn((group, Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, Vec3::new(5.6, 2.6, 5.6))));
    let made = bake(&mut renderer, &room.world, &mut time);
    assert!(made.probes.positions.len() >= 100, "{} probes", made.probes.positions.len());
    put_on(&mut room.world, &made);
    // A ball with no lightmap, as a moving Obj has, out of the sun's way: in the dark corner, then beside the patch.
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let white =
        renderer.add_material(&MaterialDesc { base_color: [0.9, 0.9, 0.9, 1.0], ..MaterialDesc::default() }).unwrap();
    let ball = room
        .world
        .spawn((MeshRenderer::new(sphere, white), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(0.5))));
    let camera = room.world.query_ref::<(Entity, &Camera)>().into_iter().next().unwrap().0;
    let mut look = |world: &mut World, renderer: &mut Renderer, time: &mut f32, name: &str, place: Vec3| {
        world.get_mut::<Transform>(ball).unwrap().position = place;
        let eye = place + Vec3::new(0.0, 0.3, -1.4);
        let turn = Quat::from_rotation_x(0.2);
        *world.get_mut::<Transform>(camera).unwrap() = Transform::new(eye, turn, Vec3::ONE);
        run(renderer, world, time, 3);
        let pixels = renderer.read_pixels().unwrap().2;
        keep_image(&format!("ball lit by probes, {name}.png"), &pixels);
        // The ball's side facing the camera, which the sun does not reach.
        brightness(&pixels, pixel_of(place + Vec3::new(0.0, 0.0, -0.24), eye, turn))
    };
    let (corner, beside) = (Vec3::new(-2.4, 0.6, 1.6), Vec3::new(0.5, 0.5, 1.4));
    // Without the probes, nothing but the sun lights the ball, and it is out of the sun.
    let baked = room.world.remove_resource::<BakedLighting>();
    let dark = look(&mut room.world, &mut renderer, &mut time, "without probes", beside);
    room.world.insert_resource(baked.expect("put on above"));
    let seen = [
        look(&mut room.world, &mut renderer, &mut time, "corner", corner),
        look(&mut room.world, &mut renderer, &mut time, "by the patch", beside),
    ];
    let probes = &made.probes;
    // Light from below, as an underside takes it: over the sunlit patch, the floor it sees is far brighter.
    let below = |at: Vec3| probes.light_at(at).expect("probes round it").irradiance(Vec3::NEG_Y).x;
    let over = Vec3::new(0.8, 0.5, 0.0);
    eprintln!("the ball: {dark} without probes, {seen:?} with; from below {} and {}", below(corner), below(over));
    assert!(dark < 2.0, "without probes the ball is dark: {dark}");
    assert!(seen[0] > dark + 40.0, "the probes' bounced light reaches the ball in the corner: {seen:?}");
    assert!(seen[1] > seen[0], "and more of it beside the sunlit patch: {seen:?}");
    assert!(below(over) > below(corner) * 3.0, "the probes over the sunlit patch take far more from below");
}

/// A wide floor under an open sky, no sun, seen from above: what lights it
/// is the sky alone, all of which it sees. `gi` none for no bounced light.
fn open_floor(renderer: &mut Renderer, gi: Option<GiSource>) -> World {
    let mut world = World::new();
    let eye = Vec3::new(0.0, 3.0, -3.0);
    world.spawn((Camera::default(), Transform::new(eye, Quat::from_rotation_x(0.7), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let desc = MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], roughness: 1.0, ..MaterialDesc::default() };
    let grey = renderer.add_material(&desc).unwrap();
    let floor = Transform::new(Vec3::new(0.0, -0.5, 0.0), Quat::IDENTITY, Vec3::new(200.0, 1.0, 200.0));
    world.spawn((MeshRenderer::new(cube, grey), floor, Lightmap { scale: 0.05, ..Lightmap::default() }));
    world.insert_resource(Sky::default());
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.reflections.enabled = false;
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    let lit = &mut post.global_illumination;
    (lit.enabled, lit.samples, lit.texels_per_metre) = (gi.is_some(), 128, 2.0);
    lit.source = gi.unwrap_or_default();
    world.insert_resource(post);
    world
}

#[test]
fn an_open_floor_bakes_the_skys_light_as_plain_shading_gives_it() {
    let middle = (WIDTH / 2, HEIGHT / 2);
    let mut seen = Vec::new();
    for gi in [None, Some(GiSource::Baked), Some(GiSource::Realtime)] {
        let Some(mut renderer) = renderer(false) else { return };
        let mut time = 0.0;
        let mut world = open_floor(&mut renderer, gi);
        if gi == Some(GiSource::Baked) {
            let made = bake(&mut renderer, &world, &mut time);
            put_on(&mut world, &made);
        }
        run(&mut renderer, &world, &mut time, 60);
        let pixels = renderer.read_pixels().unwrap().2;
        keep_image(&format!("open floor, {gi:?}.png"), &pixels);
        seen.push(brightness(&pixels, middle));
    }
    eprintln!("an open floor: {seen:?} plain, baked and real-time");
    assert!((seen[1] - seen[0]).abs() < seen[0] * 0.08 + 3.0, "baked as the plain sky light gives it: {seen:?}");
}

#[test]
fn the_closed_room_bakes_about_the_light_the_real_time_probes_find() {
    let Some(mut renderer) = renderer(false) else { return };
    let mut time = 0.0;
    let mut room = build_room(&mut renderer, GiSource::Baked);
    let made = bake(&mut renderer, &room.world, &mut time);
    put_on(&mut room.world, &made);
    run(&mut renderer, &room.world, &mut time, 3);
    let baked = renderer.read_pixels().unwrap().2;
    let Some(mut renderer) = self::renderer(false) else { return };
    let room = build_room(&mut renderer, GiSource::Realtime);
    run(&mut renderer, &room.world, &mut time, 240);
    let realtime = renderer.read_pixels().unwrap().2;
    keep_image("room through a window, real-time.png", &realtime);
    let at = |point: Vec3| pixel_of(point, EYE, camera_rotation());
    for (what, point) in [
        ("ceiling", Vec3::new(0.0, 3.0, 2.0)),
        ("far wall", Vec3::new(-1.5, 1.4, 3.0)),
        ("shaded floor", Vec3::new(-2.0, 0.0, 2.0)),
    ] {
        eprintln!("{what}: baked {}, real-time {}", brightness(&baked, at(point)), brightness(&realtime, at(point)));
    }
}

/// The room with what makes ties for the rays: a red rug lying exactly in the floor's top and a green panel
/// exactly in the west wall's face, so their triangles and the floor's and wall's meet rays at the same distance,
/// and a scatter of small blocks for the hierarchies to sort; and two lamps, spawned in either order, so the world
/// keeps them in either order.
fn tied_room(renderer: &mut Renderer, lamps_reversed: bool) -> World {
    let mut room = build_room(renderer, GiSource::Baked);
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let mut material = |color: [f32; 4]| {
        renderer.add_material(&MaterialDesc { base_color: color, ..MaterialDesc::default() }).unwrap()
    };
    let (red, green, grey) =
        (material([0.8, 0.1, 0.1, 1.0]), material([0.1, 0.8, 0.1, 1.0]), material([0.6, 0.6, 0.6, 1.0]));
    let world = &mut room.world;
    let rug = Transform::new(Vec3::new(0.5, -0.05, 0.3), Quat::IDENTITY, Vec3::new(3.0, 0.1, 2.0));
    world.spawn((MeshRenderer::new(cube, red), rug, Lightmap::default()));
    let panel = Transform::new(Vec3::new(-3.05, 1.5, 0.5), Quat::IDENTITY, Vec3::new(0.1, 2.0, 3.0));
    world.spawn((MeshRenderer::new(cube, green), panel, Lightmap::default()));
    for i in 0..40 {
        let (x, z) = ((i % 8) as f32 * 0.7 - 2.5, (i / 8) as f32 * 0.9 - 2.2);
        let block = Transform::new(Vec3::new(x, 0.15, z), Quat::from_rotation_y(i as f32), Vec3::splat(0.3));
        world.spawn((MeshRenderer::new(cube, grey), block, Lightmap::default()));
    }
    room.world.spawn((
        LightProbeGroup::default(),
        Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, Vec3::new(5.0, 2.0, 5.0)),
    ));
    let mut lamps = vec![
        (Light::point(Vec3::new(1.0, 0.6, 0.3), 8.0, 6.0), Vec3::new(-1.5, 2.5, -1.0)),
        (Light { radius: 0.3, ..Light::disc(Vec3::new(0.3, 0.5, 1.0), 10.0, 6.0, 0.3) }, Vec3::new(1.5, 2.8, 1.5)),
    ];
    if lamps_reversed {
        lamps.reverse();
    }
    for (light, at) in lamps {
        let down = Quat::from_rotation_arc(Vec3::Z, Vec3::NEG_Y);
        room.world.spawn((light, Transform::new(at, down, Vec3::ONE)));
    }
    room.world
}

/// Bakes the tied room in a renderer of its own, after `wait` frames, `pace` dispatches a frame, under an
/// atmosphere sky with the camera at `eye`; after more than ten frames, its lamps spawned the other way round.
fn tied_bake(software: bool, wait: u32, pace: usize, eye: Vec3) -> Option<LightBake> {
    let mut renderer = renderer(software)?;
    let mut time = 0.0;
    let mut world = tied_room(&mut renderer, wait > 10);
    world.insert_resource(Sky { mode: voxear_render::SkyMode::Atmosphere, ..Sky::default() });
    let camera = world.query_ref::<(Entity, &Camera)>().into_iter().next().unwrap().0;
    world.get_mut::<Transform>(camera).unwrap().position = eye;
    run(&mut renderer, &world, &mut time, wait);
    renderer.set_light_bake_pace(pace);
    renderer.start_light_bake(&world).unwrap();
    for _ in 0..2000 {
        run(&mut renderer, &world, &mut time, 1);
        if let Some(made) = renderer.take_light_bake() {
            return Some(made.unwrap());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("the bake did not finish");
}

fn same_in_another_renderer(software: bool) {
    let moved = EYE + Vec3::new(1.5, 0.7, 2.0);
    let (Some(first), Some(second)) = (tied_bake(software, 3, 16, EYE), tied_bake(software, 40, 3, moved)) else {
        return;
    };
    let texels: usize = first.atlases.iter().map(|a| a.light.len()).sum();
    let differing: usize = first
        .atlases
        .iter()
        .zip(&second.atlases)
        .map(|(a, b)| a.light.chunks(16).zip(b.light.chunks(16)).filter(|(x, y)| x != y).count())
        .sum();
    assert_eq!(differing, 0, "{differing} of {} blocks differ", texels / 16);
    assert!(first.atlases == second.atlases, "the directions too");
    assert_eq!(first.places, second.places);
    assert_eq!(first.probes, second.probes, "and the probes");
}

#[test]
fn a_bake_comes_out_the_same_in_another_renderer_where_surfaces_meet_rays_together() {
    same_in_another_renderer(false);
}

#[test]
fn a_bake_traced_in_software_comes_out_the_same_in_another_renderer() {
    same_in_another_renderer(true);
}
