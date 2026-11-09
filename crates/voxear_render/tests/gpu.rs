//! End-to-end checks on a real GPU, rendering offscreen. Each test skips
//! itself (with a note) on machines without a usable Vulkan device.

use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_render::{
    Camera, CookedModel, Light, MaterialDesc, MeshRenderer, Primitive, RenderStats, Renderer, RendererSettings,
    ViewOptions,
};
use voxear_scene::Transform;

const FRAME: f32 = 1.0 / 60.0;

fn renderer() -> Option<Renderer> {
    renderer_with(&RendererSettings::default())
}

fn renderer_with(settings: &RendererSettings) -> Option<Renderer> {
    match Renderer::new_offscreen(640, 360, settings) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no usable GPU ({error:#})");
            None
        }
    }
}

/// Renders `frames` frames and returns the stats of the last one. Stats come
/// back a couple of frames late, so a few extra frames are rendered first.
fn run(renderer: &mut Renderer, world: &World, time: &mut f32, frames: u32) -> RenderStats {
    for _ in 0..frames + 2 {
        *time += FRAME;
        renderer.render(world, *time).expect("frame renders");
    }
    renderer.stats()
}

/// A camera looking down +Z at a wall, with a row of spheres hidden behind
/// it and another row in plain view.
fn wall_scene(renderer: &mut Renderer) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -10.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((
        MeshRenderer::new(cube, material),
        Transform::new(Vec3::new(-4.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 6.0, 0.5)),
    ));
    for i in 0..5 {
        let hidden = Vec3::new(-6.0 + i as f32, 1.0, 6.0);
        let visible = Vec3::new(2.0 + i as f32, 1.0, 6.0);
        world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(hidden)));
        world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(visible)));
    }
    world
}

#[test]
fn the_wall_hides_what_is_behind_it() {
    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut time = 0.0;
    let stats = run(&mut renderer, &world, &mut time, 4);
    assert_eq!(stats.instances, 11);
    assert_eq!(stats.occlusion_culled, 5, "{stats:?}");
    assert_eq!(stats.drawn_early + stats.drawn_late, 6, "{stats:?}");
}

#[test]
fn a_lamp_only_reaches_the_clusters_around_it() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = wall_scene(&mut renderer);
    world.spawn((Light::point(Vec3::ONE, 5.0, 2.0), Transform::from_position(Vec3::new(4.0, 1.0, 4.0))));
    let mut time = 0.0;
    let stats = run(&mut renderer, &world, &mut time, 2);
    assert_eq!(stats.cluster_max_lights, 1);
    assert!(stats.cluster_light_links > 0, "{stats:?}");
    // A 2 m light 14 m away should touch a small part of the 3,456-cluster grid.
    assert!(stats.cluster_light_links < 200, "{stats:?}");
}

#[test]
fn lod_changes_fade_then_settle() {
    let Some(mut renderer) = renderer() else { return };
    // One large sphere close up, so it starts at its most detailed LOD.
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -4.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world
        .spawn((MeshRenderer::new(sphere, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0))));
    let mut time = 0.0;
    let settled = run(&mut renderer, &world, &mut time, 4);
    assert_eq!(settled.fade_draws, 0, "first appearances snap: {settled:?}");

    // A much looser error budget moves the sphere to a coarser LOD.
    renderer.set_view_options(ViewOptions { lod_threshold: 16.0, ..ViewOptions::default() });
    let fading = run(&mut renderer, &world, &mut time, 1);
    assert!(fading.fade_draws > 0, "{fading:?}");

    // Fades take a quarter of a second.
    let done = run(&mut renderer, &world, &mut time, 30);
    assert_eq!(done.fade_draws, 0, "{done:?}");
}

#[test]
fn a_renderer_can_force_a_lod_or_bias_the_pick() {
    let Some(mut renderer) = renderer() else { return };
    // A large sphere close up, at its most detailed LOD.
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -4.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let close = Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0));
    let ball = world.spawn((MeshRenderer::new(sphere, material), close));
    let mut time = 0.0;
    let near = run(&mut renderer, &world, &mut time, 4);
    assert!(near.lods[0] > 0, "{near:?}");

    // Forced, it draws the LOD it is told at any distance.
    world.get_mut::<MeshRenderer>(ball).unwrap().forced_lod = Some(2);
    let forced = run(&mut renderer, &world, &mut time, 30);
    assert!(forced.lods[2] > 0 && forced.lods[0] == 0, "{forced:?}");

    // A large bias lets far more error through, so a simpler LOD comes sooner.
    fn renderer_of(world: &mut World, ball: voxear_ecs::Entity) -> &mut MeshRenderer {
        world.get_mut::<MeshRenderer>(ball).unwrap()
    }
    let settings = renderer_of(&mut world, ball);
    (settings.forced_lod, settings.lod_bias) = (None, 6.0);
    let biased = run(&mut renderer, &world, &mut time, 30);
    assert_eq!(biased.lods[0], 0, "{biased:?}");

    // Where its LODs end, by how much of the screen it covers: close up it covers more than all of it, so ends
    // past the whole screen send it to its simplest, and ends at nothing keep it at its most detailed.
    renderer_of(&mut world, ball).lod_transitions = Some([3.0, 2.9, 2.8, 2.7, 2.6]);
    let simplest = run(&mut renderer, &world, &mut time, 30);
    assert_eq!(simplest.lods[0], 0, "{simplest:?}");
    renderer_of(&mut world, ball).lod_transitions = Some([0.0; 5]);
    let detailed = run(&mut renderer, &world, &mut time, 30);
    assert!(detailed.lods[0] > 0 && detailed.lods[1..].iter().all(|&n| n == 0), "{detailed:?}");
}

#[test]
fn an_obj_new_to_its_place_in_the_list_does_not_fade_in() {
    let Some(mut renderer) = renderer() else { return };
    // A small sphere far off, drawn at a coarse LOD, until it settles.
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -4.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let far =
        world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(Vec3::new(0.0, 0.0, 80.0))));
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 4);

    // Another, close up, takes its place in the list, as when a scene opens
    // in place of another: it shows at once, not fading in from the old LOD.
    world.despawn(far);
    world
        .spawn((MeshRenderer::new(sphere, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0))));
    let swapped = run(&mut renderer, &world, &mut time, 1);
    assert_eq!(swapped.fade_draws, 0, "{swapped:?}");
}

/// How a render culls meshlets: not at all, drawing whole LODs; with a
/// compute pass and indirect draws; or with task and mesh shaders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Meshlets {
    Off,
    Compute,
    MeshShaders,
}

/// The ways meshlets are culled on this GPU.
const CULLED: [Meshlets; 2] = [Meshlets::Compute, Meshlets::MeshShaders];

/// A renderer that culls meshlets as `meshlets` says, with these view
/// options, or none on a GPU without the mesh shaders it asks for.
fn renderer_culling(meshlets: Meshlets, options: ViewOptions) -> Option<Renderer> {
    let mut renderer = renderer()?;
    renderer.set_mesh_shaders(meshlets == Meshlets::MeshShaders);
    if meshlets == Meshlets::MeshShaders && !renderer.uses_mesh_shaders() {
        eprintln!("skipping the mesh shader path: this GPU has no mesh shaders");
        return None;
    }
    renderer.set_view_options(ViewOptions { meshlet_culling: meshlets != Meshlets::Off, ..options });
    Some(renderer)
}

/// A sunlit floor with a wall across it, a row of spheres behind the wall, a
/// large one in plain view, one in full detail half out of it and, with
/// `far_caster`, one off to the side whose shadow falls in view: meshlets go
/// for every reason, in the view and the shadows.
fn meshlet_scene(renderer: &mut Renderer, far_caster: bool) -> World {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 2.0, -10.0))));
    let sun = Quat::from_rotation_x(0.9) * Quat::from_rotation_y(-0.6);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let place = |position: Vec3, scale: Vec3| Transform::new(position, Quat::IDENTITY, scale);
    world.spawn((MeshRenderer::new(plane, material), place(Vec3::new(0.0, 0.0, 10.0), Vec3::splat(60.0))));
    world.spawn((MeshRenderer::new(cube, material), place(Vec3::new(-4.0, 1.5, 0.0), Vec3::new(8.0, 3.0, 0.5))));
    for i in 0..5 {
        world
            .spawn((MeshRenderer::new(sphere, material), place(Vec3::new(-6.5 + i as f32 * 1.5, 1.0, 4.0), Vec3::ONE)));
    }
    world.spawn((MeshRenderer::new(sphere, material), place(Vec3::new(3.0, 1.5, -4.0), Vec3::splat(3.0))));
    let edge = world.spawn((MeshRenderer::new(sphere, material), place(Vec3::new(-14.5, 2.0, 4.0), Vec3::splat(4.0))));
    world.get_mut::<MeshRenderer>(edge).unwrap().forced_lod = Some(0);
    if far_caster {
        world.spawn((MeshRenderer::new(sphere, material), place(Vec3::new(21.0, 8.0, 4.0), Vec3::splat(4.0))));
    }
    world
}

/// The scene's last frame, its meshlets culled as `meshlets` says, in a
/// debug view, and the stats of the frame `run` reports.
fn meshlet_render(meshlets: Meshlets, far_caster: bool, debug_view: u32) -> Option<(Vec<u8>, RenderStats)> {
    let mut renderer = renderer_culling(meshlets, ViewOptions { debug_view, ..ViewOptions::default() })?;
    let world = meshlet_scene(&mut renderer, far_caster);
    let stats = run(&mut renderer, &world, &mut 0.0, 6);
    Some((renderer.read_pixels().unwrap().2, stats))
}

/// How many pixels of two images differ by more than a step in any channel.
fn differing_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1)).count()
}

#[test]
fn meshlet_culling_draws_the_same_image() {
    let Some((whole, whole_stats)) = meshlet_render(Meshlets::Off, true, 0) else { return };
    assert_eq!(whole_stats.meshlets, 0, "without meshlet culling, nothing is counted: {whole_stats:?}");
    let mut culled_stats = Vec::new();
    for meshlets in CULLED {
        let Some((culled, stats)) = meshlet_render(meshlets, true, 0) else { continue };
        assert_eq!(differing_pixels(&culled, &whole), 0, "{meshlets:?}: {stats:?}");
        assert!(stats.meshlets_drawn > 0 && stats.meshlets_drawn < stats.meshlets, "some go: {stats:?}");
        assert_eq!(
            stats.meshlets,
            stats.meshlets_frustum_culled
                + stats.meshlets_cone_culled
                + stats.meshlets_occlusion_culled
                + stats.meshlets_drawn,
            "every meshlet tested is drawn or culled for one reason"
        );
        assert!(stats.meshlets_frustum_culled > 0, "part of a sphere lies out of view: {stats:?}");
        assert_eq!(stats.drawn_early + stats.drawn_late, whole_stats.drawn_early + whole_stats.drawn_late);
        culled_stats.push(stats);
    }
    if let [compute, mesh] = culled_stats[..] {
        let counts = |s: RenderStats| {
            [s.meshlets, s.meshlets_frustum_culled, s.meshlets_cone_culled, s.meshlets_drawn, s.shadow_meshlets]
        };
        assert_eq!(counts(compute), counts(mesh), "both paths cull the same meshlets");
    }
}

#[test]
fn both_paths_colour_the_same_meshlets() {
    let meshlets_view = voxear_render::DEBUG_VIEW_MESHLETS;
    let (Some((compute, _)), Some((mesh, _))) = (
        meshlet_render(Meshlets::Compute, false, meshlets_view),
        meshlet_render(Meshlets::MeshShaders, false, meshlets_view),
    ) else {
        return;
    };
    assert_eq!(differing_pixels(&compute, &mesh), 0);
}

/// One sphere in full detail, alone, scaled by `scale`, its meshlets culled
/// as `meshlets` says, in a debug view: its last frame and the stats `run`
/// reports.
fn lone_sphere(meshlets: Meshlets, scale: Vec3, debug_view: u32) -> Option<(Vec<u8>, RenderStats)> {
    let mut renderer = renderer_culling(meshlets, ViewOptions { debug_view, ..ViewOptions::default() })?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -6.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let ball = world.spawn((MeshRenderer::new(sphere, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, scale)));
    world.get_mut::<MeshRenderer>(ball).unwrap().forced_lod = Some(0);
    let stats = run(&mut renderer, &world, &mut 0.0, 4);
    Some((renderer.read_pixels().unwrap().2, stats))
}

#[test]
fn meshlets_facing_away_are_culled() {
    let mirror = Vec3::new(-1.0, 1.0, 1.0);
    let Some((whole, _)) = lone_sphere(Meshlets::Off, Vec3::ONE, 0) else { return };
    let (whole_mirrored, _) = lone_sphere(Meshlets::Off, mirror, 0).unwrap();
    for meshlets in CULLED {
        let Some((culled, stats)) = lone_sphere(meshlets, Vec3::ONE, 0) else { continue };
        // Its meshlets' normals spread too wide for those around its outline to be judged, but the ones at its
        // back face away whole.
        assert_eq!(stats.meshlets_frustum_culled, 0, "{stats:?}");
        assert!(stats.meshlets_cone_culled * 8 > stats.meshlets, "its back faces away: {stats:?}");
        assert!(stats.meshlets_drawn * 2 >= stats.meshlets, "and the half facing the camera is drawn: {stats:?}");
        assert_eq!(differing_pixels(&culled, &whole), 0, "{meshlets:?}");

        // Mirrored, its triangles' winding turns round on screen, so the rasterizer keeps the inside of its far
        // half, and the cones turn round with it.
        let (culled, mirrored) = lone_sphere(meshlets, mirror, 0).unwrap();
        assert!(mirrored.meshlets_cone_culled > 0, "{mirrored:?}");
        assert_eq!(differing_pixels(&culled, &whole_mirrored), 0, "{meshlets:?}");

        // The wireframe view shows the back faces, so nothing faces away there.
        let (_, wireframe) = lone_sphere(meshlets, Vec3::ONE, voxear_render::DEBUG_VIEW_WIREFRAME).unwrap();
        assert_eq!(wireframe.meshlets_cone_culled, 0, "{wireframe:?}");
    }
}

/// A flat grid of `n` by `n` quads a unit wide, in the XY plane and facing
/// -Z, towards a camera behind it.
fn grid(n: u32) -> voxear_render::MeshData {
    let mut mesh = voxear_render::MeshData::default();
    for y in 0..=n {
        for x in 0..=n {
            mesh.vertices.push(voxear_render::Vertex {
                position: [x as f32 / n as f32 - 0.5, y as f32 / n as f32 - 0.5, 0.0],
                normal: [0.0, 0.0, -1.0],
                uv: [x as f32 / n as f32, y as f32 / n as f32],
                tangent: [1.0, 0.0, 0.0, 1.0],
                ..voxear_render::Vertex::default()
            });
        }
    }
    let at = |x: u32, y: u32| y * (n + 1) + x;
    for y in 0..n {
        for x in 0..n {
            mesh.indices.extend([at(x, y), at(x, y + 1), at(x + 1, y), at(x, y + 1), at(x + 1, y + 1), at(x + 1, y)]);
        }
    }
    mesh
}

#[test]
fn a_draw_whose_meshlets_all_survive_draws_every_one() {
    let render = |meshlets: Meshlets| {
        let mut renderer = renderer_culling(meshlets, ViewOptions::default())?;
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -0.9))));
        let mesh = renderer.add_mesh(&grid(128)).unwrap();
        let material = renderer.add_material(&MaterialDesc::default()).unwrap();
        let wall = world.spawn((MeshRenderer::new(mesh, material), Transform::IDENTITY));
        world.get_mut::<MeshRenderer>(wall).unwrap().forced_lod = Some(0);
        let stats = run(&mut renderer, &world, &mut 0.0, 3);
        Some((renderer.read_pixels().unwrap().2, stats))
    };
    let Some((whole, _)) = render(Meshlets::Off) else { return };
    for meshlets in CULLED {
        let Some((culled, stats)) = render(meshlets) else { continue };
        // Far more meshlets than one round of the meshlet pass, every one of them in view and facing it.
        assert!(stats.meshlets > 200, "{stats:?}");
        assert_eq!(stats.meshlets_drawn, stats.meshlets, "{stats:?}");
        assert_eq!(differing_pixels(&culled, &whole), 0, "{meshlets:?}");
    }
}

#[test]
fn meshlets_newly_in_view_behind_the_wall_are_culled() {
    for meshlets in CULLED {
        let Some(mut renderer) = renderer_culling(meshlets, ViewOptions::default()) else { continue };
        let mut world = meshlet_scene(&mut renderer, false);
        let mut time = 0.0;
        run(&mut renderer, &world, &mut time, 4);
        // A tall pillar and a ball appear half behind the wall: new this frame, they are drawn late, after the
        // depth pyramid, which hides the meshlets of their lower halves.
        let cube = renderer.primitive(Primitive::Cube).unwrap();
        let material = renderer.add_material(&MaterialDesc::default()).unwrap();
        let pillar = Transform::new(Vec3::new(-2.0, 3.0, 3.0), Quat::IDENTITY, Vec3::new(0.5, 6.0, 0.5));
        world.spawn((MeshRenderer::new(cube, material), pillar));
        let sphere = renderer.primitive(Primitive::Sphere).unwrap();
        let ball = Transform::new(Vec3::new(-2.0, 2.5, 3.0), Quat::IDENTITY, Vec3::splat(2.5));
        world.spawn((MeshRenderer::new(sphere, material), ball));
        let appeared = run(&mut renderer, &world, &mut time, 1);
        assert!(appeared.drawn_late > 0, "{meshlets:?}: {appeared:?}");
        assert!(appeared.meshlets_occlusion_culled > 0, "{meshlets:?}: {appeared:?}");
    }
}

#[test]
fn shadows_with_meshlet_culling_match_and_reach_from_out_of_view() {
    let Some((whole, _)) = meshlet_render(Meshlets::Off, false, 0) else { return };
    for meshlets in CULLED {
        let (Some((with_caster, stats)), Some((without, _))) =
            (meshlet_render(meshlets, true, 0), meshlet_render(meshlets, false, 0))
        else {
            continue;
        };
        assert!(stats.shadow_meshlets > 0, "{stats:?}");
        assert!(differing_pixels(&with_caster, &without) > 100, "the caster out of view still shadows the floor in it");
        assert_eq!(differing_pixels(&without, &whole), 0, "{meshlets:?}");
    }
}

#[test]
fn the_meshlets_view_colours_each_meshlet() {
    for meshlets in CULLED {
        let options = ViewOptions { debug_view: voxear_render::DEBUG_VIEW_MESHLETS, ..Default::default() };
        let Some(mut renderer) = renderer_culling(meshlets, options) else { continue };
        let mut world = World::new();
        world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -2.5))));
        let sphere = renderer.primitive(Primitive::Sphere).unwrap();
        let material = renderer.add_material(&MaterialDesc::default()).unwrap();
        let ball = world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(Vec3::ZERO)));
        world.get_mut::<MeshRenderer>(ball).unwrap().forced_lod = Some(0);
        run(&mut renderer, &world, &mut 0.0, 3);
        let (width, height, pixels) = renderer.read_pixels().unwrap();
        let mut colours = std::collections::HashSet::new();
        for y in (height / 4..height * 3 / 4).step_by(8) {
            for x in (width / 2 - height / 4..width / 2 + height / 4).step_by(8) {
                let i = ((y * width + x) * 4) as usize;
                colours.insert([pixels[i], pixels[i + 1], pixels[i + 2]].map(|c| c / 16));
            }
        }
        assert!(colours.len() > 8, "{meshlets:?}: only {} colours across the sphere", colours.len());
    }
}

/// A camera-facing quad, two-sided so winding never hides it, with a base
/// colour texture and a flat normal map, embedded as PNGs.
fn textured_quad() -> Vec<u8> {
    let mut buffer = Vec::new();
    let positions = [[-1f32, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]];
    for p in positions {
        p.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for _ in 0..4 {
        [0f32, 0.0, -1.0].iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for uv in [[0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]] {
        uv.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    }
    for index in [0u16, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2] {
        buffer.extend_from_slice(&index.to_le_bytes());
    }
    let png = |pixel: [u8; 4]| {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 4, 4);
            encoder.set_color(png::ColorType::Rgba);
            let pixels: Vec<u8> = (0..16).flat_map(|_| pixel).collect();
            encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        }
        bytes
    };
    let data = |bytes: &[u8], mime: &str| format!("data:{mime};base64,{}", base64::encode(bytes));
    format!(
        r#"{{
          "asset": {{ "version": "2.0" }},
          "buffers": [{{ "uri": "{buffer}", "byteLength": {length} }}],
          "bufferViews": [
            {{ "buffer": 0, "byteOffset": 0, "byteLength": 48 }},
            {{ "buffer": 0, "byteOffset": 48, "byteLength": 48 }},
            {{ "buffer": 0, "byteOffset": 96, "byteLength": 32 }},
            {{ "buffer": 0, "byteOffset": 128, "byteLength": 24 }}
          ],
          "accessors": [
            {{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-1, -1, 0], "max": [1, 1, 0] }},
            {{ "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3" }},
            {{ "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2" }},
            {{ "bufferView": 3, "componentType": 5123, "count": 12, "type": "SCALAR" }}
          ],
          "images": [{{ "uri": "{colour}" }}, {{ "uri": "{normal}" }}],
          "textures": [{{ "source": 0 }}, {{ "source": 1 }}],
          "materials": [{{
            "pbrMetallicRoughness": {{ "baseColorTexture": {{ "index": 0 }}, "metallicFactor": 0, "roughnessFactor": 1 }},
            "normalTexture": {{ "index": 1 }}
          }}],
          "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }}, "indices": 3, "material": 0 }}] }}],
          "nodes": [{{ "mesh": 0 }}],
          "scenes": [{{ "nodes": [0] }}]
        }}"#,
        buffer = data(&buffer, "application/octet-stream"),
        length = buffer.len(),
        colour = data(&png([220, 120, 40, 255]), "image/png"),
        normal = data(&png([128, 128, 255, 255]), "image/png"),
    )
    .into_bytes()
}

/// Renders the quad with its textures as given, or cooked (BC7 and BC5 with
/// mips, written out and read back), and returns the pixel at the centre and
/// at a corner.
fn render_quad(cooked: bool) -> Option<([u8; 4], [u8; 4])> {
    let mut renderer = renderer()?;
    let gltf = textured_quad();
    let mut model = voxear_render::prepare_model("Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
    let imported = if cooked {
        model.compress_textures();
        let bytes = model.cook().unwrap().to_bytes();
        renderer.upload_cooked(&CookedModel::from_bytes("Quad.gltf", &bytes).unwrap()).unwrap()
    } else {
        renderer.upload_model(&model).unwrap()
    };
    Some(centre_and_corner(&mut renderer, &imported))
}

fn centre_and_corner(renderer: &mut Renderer, imported: &voxear_render::ImportedModel) -> ([u8; 4], [u8; 4]) {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    world.spawn((
        MeshRenderer::new(imported.nodes[0].mesh, imported.nodes[0].material),
        Transform::IDENTITY,
    ));
    run(renderer, &world, &mut 0.0, 8);
    let (width, height, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    (at(width / 2, height / 2), at(2, 2))
}

/// A cooked quad whose 512×512 gradient texture spans many GDeflate tiles.
fn large_cooked_quad() -> CookedModel {
    let gltf = textured_quad();
    let mut model = voxear_render::prepare_model("Quad.gltf", &mut |_| Ok(gltf.clone())).unwrap();
    model.replace_texture(
        0,
        512,
        512,
        (0..512 * 512).flat_map(|i| [(i % 512 / 2) as u8, (i / 1024) as u8, 90, 255]).collect(),
    );
    model.compress_textures();
    CookedModel::from_bytes("Quad.gltf", &model.cook().unwrap().to_bytes()).unwrap()
}

#[test]
fn the_gpu_and_the_cpu_decompress_to_the_same_image() {
    let Some(mut renderer) = renderer() else { return };
    if !renderer.decompresses_on_gpu() {
        eprintln!("skipping: this GPU cannot decompress GDeflate");
        return;
    }
    let model = large_cooked_quad();
    assert!(model.is_compressed());
    let on_gpu = renderer.upload_cooked(&model).unwrap();
    assert!(renderer.asset_usage().decompressed_on_gpu > 512 * 512, "the GPU did the decompressing");
    let gpu = centre_and_corner(&mut renderer, &on_gpu);

    let Some(mut cpu_renderer) = renderer_without_gpu_decompression() else { return };
    let on_cpu = cpu_renderer.upload_cooked(&model).unwrap();
    assert_eq!(cpu_renderer.asset_usage().decompressed_on_gpu, 0);
    assert_eq!(centre_and_corner(&mut cpu_renderer, &on_cpu), gpu);
    assert_ne!(gpu.0, gpu.1, "the quad covers the centre");
}

fn renderer_without_gpu_decompression() -> Option<Renderer> {
    let mut renderer = renderer()?;
    renderer.set_gpu_decompression(false);
    Some(renderer)
}

#[test]
fn cooked_textures_look_like_their_source() {
    let (Some(source), Some(cooked)) = (render_quad(false), render_quad(true)) else { return };
    assert_ne!(source.0, source.1, "the quad covers the centre");
    let difference = (0..3).map(|c| source.0[c].abs_diff(cooked.0[c])).max().unwrap();
    assert!(difference <= 4, "BC7 and BC5 stay within a few levels: {:?} vs {:?}", source.0, cooked.0);
}

#[test]
fn the_ui_draws_over_the_frame() {
    use voxear_ui::{Color, Style, TextStyle, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let style = Style::column().at(20.0, 20.0).width(200.0).padding(16.0).radius(12.0);
    let panel = ui.panel(root, style.background(Color::hex(0xff0000)).border(2.0, Color::hex(0x00ff00)));
    ui.label(panel, "Voxear", TextStyle::default().size(24.0));
    let (width, height) = (640, 360);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let [x, y, w, h] = ui.rect(panel).map(|v| v as u32);
    assert_eq!(at(x + w - 20, y + h / 2), [255, 0, 0], "the panel is filled");
    assert_eq!(at(x, y + h / 2), [0, 255, 0], "its left edge is the border");
    assert_eq!(at(x + w - 1, y + h / 2), [0, 255, 0], "and its right");
    let corner = at(x, y);
    assert!(corner[0] < 200 && corner[1] < 200, "the corner is rounded off, got {corner:?}");
    let text = (x + 16..x + 100).flat_map(|tx| (y + 16..y + h - 16).map(move |ty| (tx, ty)));
    let brightest = text.map(|(tx, ty)| at(tx, ty).into_iter().min().unwrap()).max().unwrap();
    assert!(brightest > 230, "white text is drawn, brightest {brightest}");

    renderer.set_ui(&voxear_ui::UiFrame::default()).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = (((y + h / 2) * width + x + w - 20) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "an empty frame clears the UI");
}

/// The game's UI is laid out for its own area and drawn moved there, under
/// the window's UI, each with its own glyph atlas.
#[test]
fn the_games_ui_draws_in_its_area_under_the_windows() {
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let (width, height) = (640, 360);
    let mut game = Ui::new();
    let root = game.root();
    game.panel(root, Style::default().at(10.0, 10.0).width(60.0).height(60.0).background(Color::hex(0xff0000)));
    renderer.set_game_ui(&game.frame(300, 200, 1.0), [200, 100]).unwrap();
    let mut window = Ui::new();
    let root = window.root();
    window.panel(root, Style::default().at(240.0, 100.0).width(20.0).height(200.0).background(Color::hex(0x0000ff)));
    renderer.set_ui(&window.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(225, 135), [255, 0, 0], "the game's panel, moved to its area at 200, 100");
    assert_ne!(at(35, 35), [255, 0, 0], "not where it would be in the window's own space");
    assert_eq!(at(250, 135), [0, 0, 255], "the window's UI over it");

    renderer.set_game_ui(&voxear_ui::UiFrame::default(), [0, 0]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = ((135 * width + 225) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "an empty frame takes the game's UI away");
}

/// A second output, as the window of a panel floated on the desktop, draws
/// the views put there and a UI of its own, and is cleared for its UI when
/// no view draws into it.
#[test]
fn a_second_output_draws_its_own_views_and_ui() {
    use voxear_render::OutputId;
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let camera = world.spawn((Camera::default(), Transform::IDENTITY));
    let magenta = [255, 0, 255];
    renderer.set_clear_color([1.0, 0.0, 1.0, 1.0]);
    renderer.set_view_rect(Some([0, 0, 100, 100]));
    let (width, height) = (320, 200);
    let second = renderer.add_offscreen_output(width, height).unwrap();
    assert_eq!(renderer.outputs(), [OutputId::MAIN, second]);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(20.0, 20.0).width(60.0).height(60.0).background(Color::hex(0xff0000)));
    renderer.set_output_ui(second, &ui.frame(width, height, 1.0)).unwrap();
    let options = ViewOptions { camera: Some(camera), ..ViewOptions::default() };
    renderer.set_more_views_in(&[(second, [100, 0, 200, 200], options)]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);

    let (w, h, pixels) = renderer.read_output_pixels(second).unwrap();
    assert_eq!((w, h), (width, height));
    let at = |pixels: &[u8], width: u32, x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(&pixels, width, 50, 50), [255, 0, 0], "its own UI");
    assert_eq!(at(&pixels, width, 10, 150), magenta, "cleared around its view");
    assert_ne!(at(&pixels, width, 200, 100), magenta, "its view is drawn");
    let (main_width, _, main) = renderer.read_pixels().unwrap();
    assert_ne!(at(&main, main_width, 50, 50), [255, 0, 0], "the first output has none of its UI");
    assert_eq!(at(&main, main_width, 200, 100), magenta, "nor its view");

    renderer.set_more_views_in(&[]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_output_pixels(second).unwrap();
    assert_eq!(at(&pixels, width, 200, 100), magenta, "with no view it is cleared for its UI");
    assert_eq!(at(&pixels, width, 50, 50), [255, 0, 0]);

    renderer.remove_output(second);
    run(&mut renderer, &world, &mut 0.0, 1);
    assert!(renderer.read_output_pixels(second).is_err(), "gone");
    assert_eq!(renderer.outputs(), [OutputId::MAIN]);
}

/// Captures read an area of the output back, before the UI draws or after,
/// in the order asked for.
#[test]
fn captures_read_an_area_back_before_or_after_the_ui() {
    use voxear_render::Capture;
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(100.0).height(100.0).background(Color::hex(0xff0000)));
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);
    assert!(renderer.take_capture().is_none(), "none asked for");

    renderer.request_capture(Capture { area: Some([10, 10, 40, 30]), before_ui: true });
    renderer.request_capture(Capture { area: Some([10, 10, 40, 30]), before_ui: false });
    renderer.request_capture(Capture { area: Some([600, 340, 100, 100]), before_ui: false });
    run(&mut renderer, &world, &mut time, 3);
    let under = renderer.take_capture().expect("the view before the UI");
    let over = renderer.take_capture().expect("the window after it");
    let edge = renderer.take_capture().expect("an area off the edge");
    assert!(renderer.take_capture().is_none());

    assert_eq!((under.width, under.height, under.rgba.len()), (40, 30, 40 * 30 * 4));
    let first = |image: &voxear_render::CapturedImage| [image.rgba[0], image.rgba[1], image.rgba[2], image.rgba[3]];
    assert_ne!(first(&under)[..3], [255, 0, 0], "under the panel, the scene");
    assert_eq!(first(&over), [255, 0, 0, 255], "over it, the panel, opaque");
    assert_eq!((edge.width, edge.height), (40, 20), "kept inside the output");
}

/// The frame debugger lists the frame's passes and their images, and shows
/// the one looked at as it stood after its pass, in the channels asked for.
#[test]
fn the_frame_debugger_shows_an_image_after_its_pass() {
    use voxear_render::{Channels, FrameLook};
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(100.0).height(100.0).background(Color::hex(0xff0000)));
    let shown = ui.image(root);
    ui.set_inline_style(shown, "position: absolute; left: 320px; top: 0px; width: 320px; height: 180px");
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    renderer.set_frame_debugger(true, None);
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);

    let passes = renderer.frame_passes();
    let names: Vec<&str> = passes.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"Opaque (early)") && names.contains(&"Composite"), "{names:?}");
    let opaque = passes.iter().find(|p| p.name == "Opaque (early)").unwrap();
    let depth = opaque.images.iter().find(|i| i.name == "depth").expect("the opaque pass draws depth");
    assert!(depth.writes && depth.depth && depth.channels == 1 && depth.viewable);
    assert_eq!(depth.size, [640, 360]);
    let ui_pass = passes.iter().position(|p| p.name == "UI").unwrap();
    let output = passes[ui_pass].images.iter().position(|i| i.name == "output").unwrap();
    assert_eq!(passes[ui_pass].images[output].used, "Drawn into");
    assert!(renderer.frame_debugger_view().is_none(), "nothing looked at yet");

    let look = FrameLook { pass: ui_pass, image: output, ..FrameLook::default() };
    renderer.set_frame_debugger(true, Some(look));
    run(&mut renderer, &world, &mut time, 1);
    let (image, size) = renderer.frame_debugger_view().expect("a view of what is looked at");
    assert_eq!(size, [640, 360]);
    ui.set_image(shown, Some(image));
    renderer.set_ui(&ui.frame(640, 360, 1.0)).unwrap();
    run(&mut renderer, &world, &mut time, 1);
    let at = |pixels: &[u8], x: u32, y: u32| {
        let i = ((y * 640 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    assert_eq!(at(&pixels, 50, 50), [255, 0, 0], "the red panel itself");
    let red = at(&pixels, 345, 25);
    assert!(red[0] > 240 && red[1] < 16 && red[2] < 16, "the view of the window after the UI, at half size: {red:?}");

    renderer.set_frame_debugger(true, Some(FrameLook { channels: Channels::Green, ..look }));
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    assert!(at(&pixels, 345, 25).iter().all(|&c| c < 16), "red has no green, so the green channel is black");

    // Depth, looked at with more exposure, shows the wall in grey.
    let pass = renderer.frame_passes().iter().position(|p| p.name == "Opaque (early)").unwrap();
    let image = renderer.frame_passes()[pass].images.iter().position(|i| i.name == "depth").unwrap();
    let depth = FrameLook { pass, image, mip: 0, channels: Channels::Red, exposure: 8.0 };
    renderer.set_frame_debugger(true, Some(depth));
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let wall = at(&pixels, 400, 100);
    assert!(wall[0] > 0 && wall[0] == wall[1] && wall[1] == wall[2], "the wall's depth as grey: {wall:?}");
    assert_eq!(at(&pixels, 600, 20), [0, 0, 0], "the sky is as far as depth goes");

    renderer.set_frame_debugger(false, None);
    run(&mut renderer, &world, &mut time, 1);
    assert!(renderer.frame_passes().is_empty() && renderer.frame_debugger_view().is_none(), "off, it keeps nothing");
}

/// A panel with `backdrop-filter` shows the view under it blurred: the
/// wall's sharp edge behind it comes out soft, while beside it the edge stays
/// sharp. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the image.
#[test]
fn backdrops_blur_the_view_behind_them() {
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let world = wall_scene(&mut renderer);
    let (width, height) = (640, 360);
    run(&mut renderer, &world, &mut 0.0, 2);
    let (_, _, plain) = renderer.read_pixels().unwrap();
    // The wall's right edge, as the largest jump between neighbours along a row.
    let edge = |pixels: &[u8], y: u32, from: u32, to: u32| {
        let at = |x: u32| {
            let i = ((y * width + x) * 4) as usize;
            pixels[i] as i32 + pixels[i + 1] as i32 + pixels[i + 2] as i32
        };
        (from..to).map(|x| (at(x + 1) - at(x)).abs()).max().unwrap()
    };
    let (row, left, right) = (150, 250, 390);
    let sharp = edge(&plain, row, left, right);
    assert!(sharp > 60, "the wall has a sharp edge to blur: {sharp}");

    let mut ui = Ui::new();
    let root = ui.root();
    let mut style = Style::column().at(left as f32, 100.0).width((right - left) as f32).height(100.0).radius(10.0);
    style.background = Some(Color::hex(0x000000).with_alpha(0));
    style.backdrop_blur = 16.0;
    ui.panel(root, style);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 2);
    let (_, _, frosted) = renderer.read_pixels().unwrap();
    let soft = edge(&frosted, row, left + 12, right - 12);
    assert!(soft * 3 < sharp, "behind the panel the edge is blurred: {soft} against {sharp}");
    assert!(edge(&frosted, 260, left, right) > 60, "below the panel it is still sharp");
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("backdrop.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&frosted).unwrap();
    }
}

/// A styled list of 100,000 rows and a tree, scrolled, with a selection,
/// drawn over a frame. Set `VOXEAR_TEST_OUTPUT` to a folder to keep the image.
#[test]
fn lists_and_trees_draw_clipped_and_selected() {
    use voxear_ui::{ItemFns, NodeId, Style, TextStyle, TreeItems, Ui};

    struct Folders;
    impl TreeItems for Folders {
        fn children(&self, parent: Option<u64>) -> Vec<u64> {
            match parent {
                None => (1..=6).collect(),
                Some(folder @ 1..=6) => (1..=3).map(|i| folder * 10 + i).collect(),
                Some(_) => Vec::new(),
            }
        }
        fn make(&mut self, ui: &mut Ui, row: NodeId) {
            ui.label(row, "", TextStyle::default());
        }
        fn bind(&mut self, ui: &mut Ui, row: NodeId, item: u64) {
            let label = ui.children(row)[0];
            let name = if item < 10 { format!("Folder {item}") } else { format!("Obj {item}") };
            ui.set_text(label, &name);
        }
    }

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let sheet = r#"
        .side { padding: 6px; background-color: #111216; border-radius: 10px; font-size: 12px; color: #c8c9d0; }
        .list-row { padding: 0px 8px; border-radius: 6px; }
        .list-row:hover { background-color: #1c1d22; }
        .list-row:selected { background-color: #2b2f55; color: #ffffff; }
        .tree-arrow { color: #6b6c75; }
    "#;
    ui.add_sheet(root, voxear_ui::StyleSheet::parse("Test.vss", sheet));
    let side = ui.panel(root, Style::default().at(16.0, 16.0).width(260.0).height(300.0));
    ui.add_class(side, "side");
    let list = ui.list_view(
        side,
        22.0,
        100_000,
        ItemFns {
            make: |ui: &mut Ui, row: NodeId| {
                ui.label(row, "", TextStyle::default());
            },
            bind: |ui: &mut Ui, row: NodeId, index: usize| {
                let label = ui.children(row)[0];
                ui.set_text(label, &format!("Item {index}"));
            },
        },
    );
    ui.set_style(list, Style::column().scroll().grow());
    let other = ui.panel(root, Style::default().at(300.0, 16.0).width(260.0).height(300.0));
    ui.add_class(other, "side");
    let tree = ui.tree_view(other, 22.0, Folders);
    ui.set_style(tree, Style::column().scroll().grow());
    ui.set_expanded(tree, 2, true);

    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    ui.set_scroll(list, [0.0, 22.0 * 49_998.0]);
    ui.set_selection(list, &[50_000]);
    ui.frame(width, height, 1.0);
    let rows: Vec<NodeId> =
        ui.children(tree).into_iter().filter(|&r| ui.has_class(r, "list-row") && ui.is_visible(r)).collect();
    let file = *rows.iter().find(|&&r| ui.rect(r)[1] > ui.rect(rows[0])[1] + 50.0).unwrap();
    let [x, y, w, h] = ui.rect(file);
    ui.pointer_move(x + w / 2.0, y + h / 2.0);
    ui.pointer_button(true);
    ui.pointer_button(false);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("lists.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }

    let at = |x: f32, y: f32| {
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let selected = [0x2b, 0x2f, 0x55];
    let [lx, ly, lw, _] = ui.viewport(list);
    assert_eq!(at(lx + lw - 30.0, ly + 2.0 * 22.0 + 11.0), selected, "item 50,000 is third from the top, selected");
    assert_eq!(at(x + w - 20.0, y + h / 2.0), selected, "the clicked tree row is selected");
    assert_eq!(ui.selected_items(tree).len(), 1);
}

/// The input controls with the default theme: a focused text field with a
/// selection, a toggle, a slider, a foldout and a button. Set
/// `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn controls_draw_with_the_default_theme() {
    use voxear_ui::{Key, Modifiers, Shape, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <Panel style="position: absolute; left: 16px; top: 16px; width: 280px; padding: 12px; gap: 8px;
                          background-color: #0f1011; border-radius: 12px; border: 1px solid #1f2023">
                <TextField name="name" placeholder="Name" value="Firefly spawner" />
                <Toggle name="fly" value="true">Can fly</Toggle>
                <Toggle name="static" class="checkbox" value="true">Static</Toggle>
                <Slider name="speed" min="0" max="20" value="6" step="0.5" />
                <Segmented value="1">
                    <Segment icon="select" />
                    <Segment icon="move" />
                    <Segment icon="rotate" />
                    <Segment icon="scale" />
                    <Segment text="Pivot" icon="pivot" />
                </Segmented>
                <Foldout text="Movement" open="false">
                    <Label>Speed</Label>
                </Foldout>
                <Button>Bake path</Button>
                <Button style="border: 1px dashed #2e3035; background-color: transparent; color: #8a8f98">
                    <Icon icon="plus" />
                    <Label>Add Component</Label>
                </Button>
            </Panel>
            <Panel name="overlay" style="position: absolute; left: 340px; top: 40px; width: 200px; height: 120px;
                          border-radius: 12px; background-color: rgba(15, 16, 17, 0.82);
                          border: 1px solid #2e3035; box-shadow: 0px 16px 40px rgba(0, 0, 0, 0.4)" />
            <Panel name="canvas" style="position: absolute; left: 340px; top: 200px; width: 120px; height: 120px" />
        </vui>"#;
    let document = ui.load_markup(root, "Form.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let canvas = ui.find(document, "canvas").unwrap();
    let red = voxear_ui::Color::hex(0xeb5757);
    ui.set_shapes(
        canvas,
        vec![
            Shape::Line { from: [60.0, 60.0], to: [100.0, 60.0], width: 2.0, color: red },
            Shape::Circle { center: [100.0, 60.0], radius: 9.0, fill: red, border: None },
            Shape::Circle {
                center: [60.0, 60.0],
                radius: 40.0,
                fill: voxear_ui::Color::TRANSPARENT,
                border: Some((1.0, voxear_ui::Color::hex(0x2e3035))),
            },
        ],
    );
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let name = ui.find(document, "name").unwrap();
    ui.focus(name);
    ui.set_modifiers(Modifiers { shift: false, control: true });
    ui.key_down(Key::Right);
    ui.set_modifiers(Modifiers { shift: true, control: true });
    ui.key_down(Key::Right);
    ui.set_modifiers(Modifiers::default());
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("controls.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let at = |x: f32, y: f32| {
        let i = ((y as u32 * width + x as u32) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let accent = [0x79, 0x88, 0xff];
    let fly = ui.find(document, "fly").unwrap();
    let pill = ui.children(fly)[0];
    let [x, y, w, h] = ui.rect(pill);
    assert_eq!((w, h), (26.0, 14.0), "a toggle is a 26 by 14 pill");
    assert_eq!(at(x + 5.0, y + 7.0), accent, "an on switch fills with the accent");
    assert_eq!(at(x + 19.0, y + 7.0), [0xff, 0xff, 0xff], "with its knob over on the right");
    let check_box = ui.children(ui.find(document, "static").unwrap())[0];
    let [x, y, ..] = ui.rect(check_box);
    assert_eq!(at(x + 2.0, y + 7.0), accent, "a ticked checkbox fills with the accent");
    let [x, y, _, h] = ui.rect(name);
    assert_eq!(at(x, y + h / 2.0), accent, "the focused field's border is the accent");
    let [x, y, w, h] = ui.rect(ui.find(document, "overlay").unwrap());
    let (below, beside) = (at(x + w / 2.0, y + h + 10.0), at(x + w + 90.0, y + h + 10.0));
    assert!(below[0] + 8 < beside[0], "the overlay's shadow darkens what is under it: {below:?} {beside:?}");
    let [x, y, ..] = ui.rect(canvas);
    assert_eq!(at(x + 100.0, y + 60.0), [0xeb, 0x57, 0x57], "a circle drawn on a canvas");
    assert_eq!(at(x + 80.0, y + 60.0), [0xeb, 0x57, 0x57], "and the line to it");
    assert!(ui.take_warnings().is_empty());
}

/// A split view, tabs, a grid-laid inspector and an open dropdown, as the
/// editor will use them. Set `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn panes_draw_an_editor_like_layout() {
    use voxear_ui::Ui;

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <SplitView name="split" size="170" style="position: absolute; left: 12px; top: 12px; right: 12px; bottom: 12px;
                                                       font-size: 12px; color: #c8c9d0">
                <Panel style="padding: 10px; gap: 6px; background-color: #111216; border-radius: 10px; flex-grow: 1">
                    <Label style="font-weight: 600; color: #e8e8ec">Hierarchy</Label>
                    <Label>Main Camera</Label>
                    <Label>Sun</Label>
                    <Label>Lamp</Label>
                </Panel>
                <Tabs selected="2" style="padding: 10px; background-color: #111216; border-radius: 10px; flex-grow: 1">
                    <Tab text="Scene" />
                    <Tab text="Game" />
                    <Tab text="Inspector">
                        <Panel style="display: grid; grid-template-columns: 90px 1fr; gap: 8px; align-items: center">
                            <Label>Name</Label><TextField value="Lamp" />
                            <Label>Shading</Label>
                            <Dropdown name="shading" value="0"><Option>Lit</Option><Option>Unlit</Option><Option>Wireframe</Option></Dropdown>
                            <Label>Intensity</Label><Slider min="0" max="10" value="3" />
                            <Label>Shadows</Label><Toggle value="true">Cast</Toggle>
                        </Panel>
                    </Tab>
                </Tabs>
            </SplitView>
        </vui>"#;
    let document = ui.load_markup(root, "Editor.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let shading = ui.find(document, "shading").unwrap();
    ui.open_dropdown(shading);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("panes.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let split = ui.find(document, "split").unwrap();
    let divider = ui.children(split)[1];
    let [x, y, _, h] = ui.rect(divider);
    let i = (((y + h / 2.0) as u32 * width + x as u32 + 1) * 4) as usize;
    assert_eq!(pixels[i..i + 3], [0x25, 0x27, 0x2b], "the divider between the panes");
    assert!(ui.take_warnings().is_empty());
}

/// A docked editor layout, caught mid-drag: the ghost of a tab and the
/// preview of where it would land. Set `VOXEAR_TEST_OUTPUT` to keep the image.
#[test]
fn docking_draws_areas_and_a_drag_preview() {
    use voxear_ui::{NodeId, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let mut ui = Ui::new();
    let root = ui.root();
    let markup = r#"
        <vui>
            <DockSpace name="dock" layout="row(150, [hierarchy, project], column(230, [scene, game], row(250, [console], [inspector])))"
                       style="position: absolute; left: 8px; top: 8px; right: 8px; bottom: 8px; font-size: 12px; color: #c8c9d0">
                <DockPanel id="hierarchy" title="Hierarchy"><Label>Main Camera</Label><Label>Sun</Label><Label>Lamp</Label></DockPanel>
                <DockPanel id="project" title="Project"><Label>Assets</Label></DockPanel>
                <DockPanel id="scene" title="Scene" />
                <DockPanel id="game" title="Game" />
                <DockPanel id="console" title="Console"><Label>Loaded game scripts</Label></DockPanel>
                <DockPanel id="inspector" title="Inspector">
                    <Panel style="display: grid; grid-template-columns: 70px 1fr; gap: 6px; align-items: center">
                        <Label>Name</Label><TextField value="Lamp" />
                        <Label>Intensity</Label><Slider min="0" max="10" value="3" />
                    </Panel>
                </DockPanel>
            </DockSpace>
        </vui>"#;
    let document = ui.load_markup(root, "Editor.vui", markup, &mut |_| Err("no files".into())).unwrap();
    let (width, height) = (640, 360);
    ui.frame(width, height, 1.0);
    let dock = ui.find(document, "dock").unwrap();
    fn find_tab(ui: &Ui, node: NodeId, title: &str) -> Option<NodeId> {
        if ui.has_class(node, "tab") && ui.text(node) == title {
            return Some(node);
        }
        ui.children(node).into_iter().find_map(|child| find_tab(ui, child, title))
    }
    let console = find_tab(&ui, dock, "Console").unwrap();
    let scene_area = ui.parent(ui.parent(find_tab(&ui, dock, "Scene").unwrap()).unwrap()).unwrap();
    let [cx, cy, cw, ch] = ui.rect(console);
    let [sx, sy, _, sh] = ui.rect(scene_area);
    ui.pointer_move(cx + cw / 2.0, cy + ch / 2.0);
    ui.pointer_button(true);
    ui.pointer_move(sx + 40.0, sy + sh / 2.0 - 10.0);
    ui.pointer_move(sx + 30.0, sy + sh / 2.0);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("dock.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let [x, y, ..] = ui.rect(scene_area);
    let i = (((y + 4.0) as u32 * width + x as u32) * 4) as usize;
    assert!(pixels[i + 2] > 0xa0, "the preview's accent edge over the scene's left half");
    assert!(ui.take_warnings().is_empty());
}

/// The 3D view drawn into part of the output, as the editor's Scene panel
/// does, and through a chosen camera rather than the first one.
#[test]
fn the_view_draws_into_a_rect_through_a_chosen_camera() {
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -100.0))));
    let chosen = world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material =
        renderer.add_material(&MaterialDesc { base_color: [1.0, 0.1, 0.1, 1.0], ..Default::default() }).unwrap();
    world.spawn((MeshRenderer::new(cube, material), Transform::IDENTITY));
    renderer.set_view_rect(Some([320, 40, 200, 200]));
    renderer.set_view_options(ViewOptions { camera: Some(chosen), ..ViewOptions::default() });
    run(&mut renderer, &world, &mut 0.0, 3);
    let (width, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(10, 10), [0, 0, 0], "outside the view is cleared");
    assert_eq!(at(600, 300), [0, 0, 0]);
    let centre = at(420, 140);
    // Lit only by the sky, so a dim red, but clearly red.
    assert!(
        centre[0] > 60 && centre[0] > 2 * centre[1],
        "the near camera sees the red cube filling the view's middle: {centre:?}"
    );
    let sky = at(325, 45);
    assert!(sky[2] > sky[0], "the view's corner shows sky: {sky:?}");
}

#[test]
fn a_preview_draws_an_object_on_its_background_for_the_ui_to_show() {
    use voxear_render::{PreviewOptions, PreviewPart};
    use voxear_ui::{ImageRef, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let red =
        renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    let part = PreviewPart { mesh: cube, material: red, transform: voxear_core::math::Mat4::IDENTITY };
    let options = PreviewOptions { size: 128, background: [0.0, 0.0, 1.0, 1.0], ..PreviewOptions::default() };
    let id = renderer.add_preview(vec![part], options);
    assert!(renderer.preview(id).is_none(), "drawn with the next frame");
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 1);
    let image = renderer.preview(id).expect("drawn");
    assert_eq!(image.size, 128);

    // Shown at its own size by an image in the UI.
    let mut ui = Ui::new();
    let root = ui.root();
    let shown = ui.image(root);
    ui.set_inline_style(shown, "position: absolute; left: 20px; top: 20px; width: 128px; height: 128px");
    ui.set_image(shown, Some(ImageRef { texture: image.texture.0, uv: image.uv }));
    let _ = Style::default();
    let (width, height) = (640, 360);
    renderer.set_ui(&ui.frame(width, height, 1.0)).unwrap();
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join("preview.png")).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    let middle = at(20 + 64, 20 + 64);
    assert!(middle[0] > 80 && middle[0] > middle[1] * 2 && middle[0] > middle[2] * 2, "the red cube, got {middle:?}");
    let corner = at(22, 22);
    assert!(corner[2] > 200 && corner[0] < 40, "the blue background, got {corner:?}");

    // Asked again with a clear background, the cube's edges are smooth and the rest shows what is behind.
    renderer.set_preview(id, None, PreviewOptions { background: [0.0; 4], ..options });
    run(&mut renderer, &world, &mut time, 1);
    run(&mut renderer, &world, &mut time, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = ((22 * width + 22) * 4) as usize;
    assert!(pixels[i + 2] < 200, "no blue once the background is clear, got {:?}", &pixels[i..i + 4]);
    renderer.free_preview(id);
    assert!(renderer.preview(id).is_none());
}

#[test]
fn a_previews_pixels_read_back_and_save_as_a_png() {
    use voxear_render::{PreviewOptions, PreviewPart};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let green =
        renderer.add_material(&MaterialDesc { base_color: [0.1, 0.8, 0.2, 1.0], ..MaterialDesc::default() }).unwrap();
    let part = PreviewPart { mesh: sphere, material: green, transform: voxear_core::math::Mat4::IDENTITY };
    let id = renderer.add_preview(vec![part], PreviewOptions { size: 64, ..PreviewOptions::default() });
    renderer.read_preview(id);
    let mut time = 0.0;
    let mut pixels = None;
    for _ in 0..6 {
        run(&mut renderer, &world, &mut time, 0);
        pixels = pixels.or_else(|| renderer.preview_pixels(id));
    }
    let pixels = pixels.expect("read back within a few frames");
    assert_eq!((pixels.size, pixels.rgba.len()), (64, 64 * 64 * 4));
    let at = |x: usize, y: usize| &pixels.rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    let middle = at(32, 32);
    assert!(middle[3] == 255 && middle[1] > middle[0] && middle[1] > middle[2], "a green ball, got {middle:?}");
    assert_eq!(at(1, 1)[3], 0, "on a clear background");
    assert!(renderer.preview_pixels(id).is_none(), "handed over once");

    let file = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("preview-ball.png");
    pixels.save_png(&file).unwrap();
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&file).unwrap()));
    let info = decoder.read_info().unwrap().info().clone();
    assert_eq!((info.width, info.height, info.color_type), (64, 64, png::ColorType::Rgba));
}

#[test]
fn a_full_preview_atlas_grows_and_draws_everything_again() {
    use voxear_render::{PreviewOptions, PreviewPart};

    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let part = PreviewPart { mesh: cube, material, transform: voxear_core::math::Mat4::IDENTITY };
    // The first atlas holds sixteen of these; the twentieth makes it grow.
    let ids: Vec<_> = (0..20)
        .map(|_| renderer.add_preview(vec![part], PreviewOptions { size: 256, ..PreviewOptions::default() }))
        .collect();
    let mut time = 0.0;
    for _ in 0..8 {
        run(&mut renderer, &world, &mut time, 0);
        for &id in &ids {
            renderer.preview(id);
        }
    }
    let images: Vec<_> = ids.iter().map(|&id| renderer.preview(id).expect("every preview is drawn")).collect();
    let far = images.iter().map(|image| image.uv[2].max(image.uv[3])).fold(0.0, f32::max);
    assert!(far <= 1.0 && far > 0.5, "in a bigger atlas, where 256 pixels is an eighth: {far}");
    assert!(images.iter().all(|image| image.uv[2] - image.uv[0] == 0.125), "each a 256-pixel cell of 2048");
}

/// Where the camera of [`corner_scene`] stands, looking into the corner.
const CORNER_CAMERA: Vec3 = Vec3::new(1.5, 2.2, -2.5);

fn corner_camera_rotation() -> Quat {
    let forward = (Vec3::new(-1.0, 0.6, 2.0) - CORNER_CAMERA).normalize();
    Quat::from_rotation_y(forward.x.atan2(forward.z)) * Quat::from_rotation_x(-forward.y.asin())
}

/// A floor lit only by the sky, with two walls meeting it in a corner at
/// (-2, 0, 3), and post-processing that keeps the image steady: no TAA, so
/// no jitter, and fixed exposure.
fn corner_scene(renderer: &mut Renderer, ambient_occlusion: voxear_render::AmbientOcclusion) -> World {
    use voxear_render::{AntiAliasing, AntiAliasingMode, PostProcess};
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(CORNER_CAMERA, corner_camera_rotation(), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let place = |position: Vec3, scale: Vec3| Transform::new(position, Quat::IDENTITY, scale);
    world.spawn((MeshRenderer::new(plane, material), place(Vec3::ZERO, Vec3::splat(30.0))));
    world.spawn((MeshRenderer::new(cube, material), place(Vec3::new(0.0, 1.5, 3.25), Vec3::new(8.0, 3.0, 0.5))));
    world.spawn((MeshRenderer::new(cube, material), place(Vec3::new(-2.25, 1.5, 0.0), Vec3::new(0.5, 3.0, 8.0))));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { ambient_occlusion, anti_aliasing, ..PostProcess::neutral() });
    world
}

/// The pixel `point` of the corner scene lands on.
fn corner_pixel(point: Vec3) -> (u32, u32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_rotation_translation(corner_camera_rotation(), CORNER_CAMERA).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "{point} is in view");
    ((x * 640.0) as u32, (y * 360.0) as u32)
}

/// On the floor right in the corner, and out in the open, away from the walls.
const IN_THE_CORNER: Vec3 = Vec3::new(-1.95, 0.0, 2.95);
const OPEN_FLOOR: Vec3 = Vec3::new(1.0, 0.0, 0.5);

/// The corner scene's last frame, meshlets culled as `meshlets` says.
fn corner_render(
    meshlets: Meshlets,
    ambient_occlusion: voxear_render::AmbientOcclusion,
    debug_view: u32,
) -> Option<Vec<u8>> {
    let mut renderer = renderer_culling(meshlets, ViewOptions { debug_view, ..ViewOptions::default() })?;
    let world = corner_scene(&mut renderer, ambient_occlusion);
    run(&mut renderer, &world, &mut 0.0, 6);
    Some(renderer.read_pixels().unwrap().2)
}

/// Keeps a 640×360 image as `name` when `VOXEAR_TEST_OUTPUT` names a folder.
fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, 640, 360);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

fn rgb_at(pixels: &[u8], (x, y): (u32, u32)) -> [i32; 3] {
    let i = ((y * 640 + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]].map(i32::from)
}

#[test]
fn ambient_occlusion_darkens_a_corner_but_not_an_open_floor() {
    use voxear_render::AmbientOcclusion;
    let on = AmbientOcclusion::default();
    let off = AmbientOcclusion { enabled: false, ..on };
    let (Some(lit), Some(occluded)) =
        (corner_render(Meshlets::Compute, off, 0), corner_render(Meshlets::Compute, on, 0))
    else {
        return;
    };
    keep_image("corner without ao.png", &lit);
    keep_image("corner with ao.png", &occluded);
    let brightness = |pixels: &[u8], point: Vec3| rgb_at(pixels, corner_pixel(point)).iter().sum::<i32>();
    let (corner_lit, corner_occluded) = (brightness(&lit, IN_THE_CORNER), brightness(&occluded, IN_THE_CORNER));
    assert!(corner_occluded * 100 < corner_lit * 85, "the corner darkens: {corner_occluded} from {corner_lit}");
    let (open_lit, open_occluded) =
        (rgb_at(&lit, corner_pixel(OPEN_FLOOR)), rgb_at(&occluded, corner_pixel(OPEN_FLOOR)));
    assert!(
        open_lit.iter().zip(open_occluded).all(|(a, b)| a.abs_diff(b) <= 2),
        "the open floor does not: {open_occluded:?} against {open_lit:?}"
    );

    // The debug view shows what is left of the sky's light, as grey.
    let Some(view) = corner_render(Meshlets::Compute, on, voxear_render::DEBUG_VIEW_AMBIENT_OCCLUSION) else { return };
    keep_image("corner ao view.png", &view);
    let (corner, open) = (rgb_at(&view, corner_pixel(IN_THE_CORNER)), rgb_at(&view, corner_pixel(OPEN_FLOOR)));
    assert!(corner[0] == corner[1] && corner[1] == corner[2] && corner[0] < 200, "{corner:?}");
    assert!(open.iter().all(|&c| c >= 250), "{open:?}");
}

#[test]
fn ambient_occlusion_at_strength_0_matches_it_off() {
    use voxear_render::AmbientOcclusion;
    let none = AmbientOcclusion { strength: 0.0, ..AmbientOcclusion::default() };
    let off = AmbientOcclusion { enabled: false, ..none };
    // Every way of drawing, as each draws its depth first in its own way.
    for meshlets in [Meshlets::Off, Meshlets::Compute, Meshlets::MeshShaders] {
        let (Some(without), Some(with)) = (corner_render(meshlets, off, 0), corner_render(meshlets, none, 0)) else {
            continue;
        };
        let differing = without.iter().zip(&with).filter(|(a, b)| a != b).count();
        assert_eq!(differing, 0, "{meshlets:?}: drawn depth first, the same image to the pixel");
    }
}

#[test]
fn a_tint_colours_only_occluded_light() {
    use voxear_render::AmbientOcclusion;
    let grey = AmbientOcclusion::default();
    let purple = AmbientOcclusion { tint: [0.6, 0.1, 0.7], ..grey };
    let (Some(plain), Some(tinted)) =
        (corner_render(Meshlets::Compute, grey, 0), corner_render(Meshlets::Compute, purple, 0))
    else {
        return;
    };
    let (open_plain, open_tinted) =
        (rgb_at(&plain, corner_pixel(OPEN_FLOOR)), rgb_at(&tinted, corner_pixel(OPEN_FLOOR)));
    assert!(
        open_plain.iter().zip(open_tinted).all(|(a, b)| a.abs_diff(b) <= 1),
        "the open floor is untouched: {open_tinted:?} against {open_plain:?}"
    );
    let (corner_plain, corner_tinted) =
        (rgb_at(&plain, corner_pixel(IN_THE_CORNER)), rgb_at(&tinted, corner_pixel(IN_THE_CORNER)));
    let gained = [0, 1, 2].map(|c| corner_tinted[c] - corner_plain[c]);
    assert!(
        gained[0] > 4 && gained[2] > 4 && gained[1] < gained[0].min(gained[2]),
        "the corner turns purple, more red and blue than green: {corner_tinted:?} from {corner_plain:?}"
    );
}

#[test]
fn half_resolution_ambient_occlusion_looks_as_full_resolution_does() {
    use voxear_render::AmbientOcclusion;
    let half = AmbientOcclusion::default();
    let full = AmbientOcclusion { full_resolution: true, ..half };
    let view = voxear_render::DEBUG_VIEW_AMBIENT_OCCLUSION;
    let (Some(searched), Some(filled)) =
        (corner_render(Meshlets::Compute, full, view), corner_render(Meshlets::Compute, half, view))
    else {
        return;
    };
    keep_image("corner ao full resolution.png", &searched);
    keep_image("corner ao half resolution.png", &filled);
    // Over the whole picture, grey by grey, half resolution strays little on average.
    let greys = |pixels: &[u8]| pixels.chunks_exact(4).map(|pixel| i32::from(pixel[0])).collect::<Vec<_>>();
    let (searched_greys, filled_greys) = (greys(&searched), greys(&filled));
    let total: i32 = searched_greys.iter().zip(&filled_greys).map(|(a, b)| (a - b).abs()).sum();
    let mean = total as f32 / searched_greys.len() as f32;
    assert!(mean < 3.0, "on average {mean} greys apart");
    let corner = |pixels: &[u8]| rgb_at(pixels, corner_pixel(IN_THE_CORNER))[0];
    assert!(corner(&searched).abs_diff(corner(&filled)) <= 12, "{} and {}", corner(&searched), corner(&filled));
    assert!(rgb_at(&filled, corner_pixel(OPEN_FLOOR)).iter().all(|&c| c >= 250), "the open floor stays open");
}

const LAMP_CAMERA: Vec3 = Vec3::new(0.0, 10.0, 0.0);

/// Looking straight down, with +Z up the screen.
fn lamp_camera_rotation() -> Quat {
    Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
}

/// A floor at night with no light but `lights`, seen from above, and
/// post-processing that keeps the image steady.
fn lamp_render(lights: &[(Light, Transform)]) -> Option<Vec<u8>> {
    lamp_render_with(renderer()?, lights)
}

/// The same, with a renderer already holding what the lights need.
fn lamp_render_with(mut renderer: Renderer, lights: &[(Light, Transform)]) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, PostProcess, Sky};
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(LAMP_CAMERA, lamp_camera_rotation(), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    for &(light, transform) in lights {
        world.spawn((light, transform));
    }
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

/// How bright the floor is at `point`, red, green and blue together.
fn floor_brightness(pixels: &[u8], point: Vec3) -> i32 {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_rotation_translation(lamp_camera_rotation(), LAMP_CAMERA).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "{point} is in view");
    rgb_at(pixels, ((x * 640.0) as u32, (y * 360.0) as u32)).iter().sum()
}

/// One metre up, facing down at the floor, or up away from it.
fn hanging(facing_down: bool) -> Transform {
    let angle = if facing_down { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
    Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::from_rotation_x(angle), Vec3::ONE)
}

#[test]
fn area_lights_light_what_lies_in_front_of_them_and_fade_with_distance() {
    let warm = Vec3::new(1.0, 0.9, 0.8);
    for light in [Light::rectangle(warm, 4.0, 6.0, 1.0, 0.5), Light::disc(warm, 4.0, 6.0, 0.4)] {
        let (Some(down), Some(up)) = (lamp_render(&[(light, hanging(true))]), lamp_render(&[(light, hanging(false))]))
        else {
            return;
        };
        keep_image(&format!("{:?} light.png", light.kind), &down);
        let under = floor_brightness(&down, Vec3::ZERO);
        let near = floor_brightness(&down, Vec3::new(2.0, 0.0, 0.0));
        let far = floor_brightness(&down, Vec3::new(4.0, 0.0, 0.0));
        let beyond = floor_brightness(&down, Vec3::new(7.5, 0.0, 0.0));
        assert!(under > 150, "{:?}: the floor under the light is lit: {under}", light.kind);
        assert!(under > near && near > far, "{:?}: it fades: {under}, {near}, {far}", light.kind);
        // Black is dithered by a step or two, so dark means no more than that.
        assert!(beyond <= 6, "{:?}: nothing beyond its range: {beyond}", light.kind);
        let behind = floor_brightness(&up, Vec3::ZERO);
        assert!(behind <= 6, "{:?}: facing away, the floor is dark: {behind}", light.kind);
    }
}

#[test]
fn a_tube_lights_along_its_length() {
    let tube = Light::tube(Vec3::ONE, 4.0, 6.0, 4.0, 0.05);
    let along_x = Transform::from_position(Vec3::new(0.0, 1.0, 0.0));
    let Some(pixels) = lamp_render(&[(tube, along_x)]) else { return };
    keep_image("tube light.png", &pixels);
    let middle = floor_brightness(&pixels, Vec3::ZERO);
    let towards_end = floor_brightness(&pixels, Vec3::new(1.5, 0.0, 0.0));
    let beside = floor_brightness(&pixels, Vec3::new(0.0, 0.0, 1.5));
    let past_end = floor_brightness(&pixels, Vec3::new(3.5, 0.0, 0.0));
    assert!(middle > 150, "{middle}");
    assert!(towards_end * 10 > middle * 7, "nearly as bright along it: {towards_end} of {middle}");
    assert!(beside < towards_end, "dimmer off to the side at the same distance from the middle: {beside}");
    assert!(past_end < towards_end, "and past its end: {past_end}");

    // A point light of the same intensity gives the same light from far off.
    let point = Light::point(Vec3::ONE, 4.0, 6.0);
    let Some(single) = lamp_render(&[(point, along_x)]) else { return };
    let (tube_far, point_far) = (
        floor_brightness(&pixels, Vec3::new(0.0, 0.0, 4.5)),
        floor_brightness(&single, Vec3::new(0.0, 0.0, 4.5)),
    );
    assert!(tube_far > 0 && point_far > 0, "{tube_far} {point_far}");
}

/// The sky alone, with the atmosphere lit by a sun `elevation` degrees up,
/// in front of a camera at head height looking along `looking`.
fn atmosphere_render(elevation: f32, looking: Vec3) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, PostProcess, Sky, SkyMode};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let camera = Quat::from_rotation_arc(Vec3::Z, looking.normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.7, 0.0), camera, Vec3::ONE)));
    let towards_sun = Vec3::new(0.0, elevation.to_radians().sin(), elevation.to_radians().cos());
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    world.spawn((Light::directional(Vec3::ONE, 1.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { mode: SkyMode::Atmosphere, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { anti_aliasing, ..PostProcess::neutral() });
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

/// The pixel a direction lands on, seen from a camera looking along `looking`.
fn sky_pixel(looking: Vec3, direction: Vec3) -> (u32, u32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_quat(Quat::from_rotation_arc(Vec3::Z, looking.normalize())).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * direction.extend(0.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "{direction} is in view");
    ((x * 640.0) as u32, (y * 360.0) as u32)
}

/// A direction `elevation` degrees up and `across` degrees to the right of +Z.
fn bearing(elevation: f32, across: f32) -> Vec3 {
    let (e, a) = (elevation.to_radians(), across.to_radians());
    Vec3::new(e.cos() * a.sin(), e.sin(), e.cos() * a.cos())
}

#[test]
fn the_atmosphere_is_brightest_towards_the_sun_and_reddens_as_it_sets() {
    let ahead = bearing(8.0, 0.0);
    let behind = Vec3::new(-ahead.x, ahead.y, -ahead.z);
    let (Some(towards), Some(away)) = (atmosphere_render(20.0, ahead), atmosphere_render(20.0, behind)) else {
        return;
    };
    keep_image("atmosphere towards the sun.png", &towards);
    keep_image("atmosphere away from the sun.png", &away);
    // Beside the sun, against the same part of the sky the other way.
    let near_sun = rgb_at(&towards, sky_pixel(ahead, bearing(14.0, 15.0)));
    let mirrored = bearing(14.0, 15.0);
    let opposite = rgb_at(&away, sky_pixel(behind, Vec3::new(-mirrored.x, mirrored.y, -mirrored.z)));
    let sum = |rgb: [i32; 3]| rgb.iter().sum::<i32>();
    assert!(sum(near_sun) > sum(opposite), "brighter by the sun: {near_sun:?} against {opposite:?}");
    assert!(opposite[2] > opposite[0], "blue away from the sun: {opposite:?}");

    let (Some(noon), Some(sunset)) = (atmosphere_render(60.0, ahead), atmosphere_render(2.0, ahead)) else { return };
    keep_image("atmosphere at noon.png", &noon);
    keep_image("atmosphere at sunset.png", &sunset);
    let horizon = sky_pixel(ahead, bearing(4.0, 20.0));
    let warmth = |rgb: [i32; 3]| rgb[0] as f32 / rgb[2].max(1) as f32;
    let (at_noon, at_sunset) = (rgb_at(&noon, horizon), rgb_at(&sunset, horizon));
    assert!(
        warmth(at_sunset) > warmth(at_noon) * 1.3,
        "redder at sunset: {at_sunset:?} against {at_noon:?} at noon"
    );
}

/// A floor at night lit by `light` hanging facing down, after `prepare`
/// gives the renderer what the light needs.
fn hanging_light_render(prepare: impl FnOnce(&mut Renderer) -> Light, height: f32) -> Option<Vec<u8>> {
    let mut renderer = renderer()?;
    let light = prepare(&mut renderer);
    let place = Transform::new(Vec3::new(0.0, height, 0.0), Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), Vec3::ONE);
    lamp_render_with(renderer, &[(light, place)])
}

/// The floor seen from above, lit by `light` two metres up and two to the
/// left, with a box half way between them, so its shadow falls round the
/// middle when the light casts one. A small flame is drawn round the light,
/// as the Sponza demo's candles have, which must not shut its light in.
fn boxed_light_render(light: Light, meshlets: Meshlets) -> Option<Vec<u8>> {
    let mut renderer = renderer_culling(meshlets, ViewOptions::default())?;
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(LAMP_CAMERA, lamp_camera_rotation(), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    world.spawn((MeshRenderer::new(cube, material), Transform::new(Vec3::new(-1.0, 1.0, 0.0), Quat::IDENTITY, Vec3::splat(0.6))));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    let at = Vec3::new(-2.0, 2.0, 0.0);
    world.spawn((light, Transform::new(at, down, Vec3::ONE)));
    let flame = Transform::new(at, Quat::IDENTITY, Vec3::new(0.08, 0.14, 0.08));
    world.spawn((MeshRenderer::new(sphere, material), flame));
    world.insert_resource(voxear_render::Sky { ambient: 0.0, ..voxear_render::Sky::default() });
    let anti_aliasing = voxear_render::AntiAliasing { mode: voxear_render::AntiAliasingMode::Off, ..Default::default() };
    world.insert_resource(voxear_render::PostProcess { anti_aliasing, ..voxear_render::PostProcess::neutral() });
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn local_lights_cast_shadows_unless_told_not_to() {
    let white = Vec3::ONE;
    let kinds = [
        Light::point(white, 6.0, 10.0),
        Light::spot(white, 6.0, 10.0, 110.0, 130.0),
        Light::rectangle(white, 6.0, 10.0, 0.3, 0.3),
        Light::tube(white, 6.0, 10.0, 0.4, 0.03),
    ];
    let paths = [Meshlets::Off, Meshlets::Compute, Meshlets::MeshShaders];
    for (light, meshlets) in kinds.into_iter().flat_map(|light| paths.map(|meshlets| (light, meshlets))) {
        assert!(light.shadows, "{:?} casts shadows by default", light.kind);
        let unshadowed = Light { shadows: false, ..light };
        let (Some(open), Some(shadowed)) = (boxed_light_render(unshadowed, meshlets), boxed_light_render(light, meshlets))
        else {
            continue;
        };
        keep_image(&format!("{:?} light shadow.png", light.kind), &shadowed);
        // Behind the box from the light, and as far on the light's other side, where nothing is in the way.
        let (behind, clear) = (Vec3::new(0.2, 0.0, 0.0), Vec3::new(-4.2, 0.0, 0.0));
        let (open_behind, open_clear) = (floor_brightness(&open, behind), floor_brightness(&open, clear));
        let (shadow, lit) = (floor_brightness(&shadowed, behind), floor_brightness(&shadowed, clear));
        assert!(open_behind > 60, "{:?}: without shadows, the light reaches past the box: {open_behind}", light.kind);
        assert!(shadow * 4 < open_behind, "{:?}: with them, the box shades the floor: {shadow} against {open_behind}", light.kind);
        assert!((lit - open_clear).abs() <= 6, "{:?}: where nothing is in the way, as lit as before: {lit}, {open_clear}", light.kind);
    }
}

#[test]
fn a_spot_lights_cookie_shapes_its_light() {
    let half_and_half = |renderer: &mut Renderer| {
        // Black on the left, white on the right, at a size uploads keep as it is.
        let rgba: Vec<u8> = (0..16 * 4).flat_map(|i| if i % 16 < 8 { [0, 0, 0, 255] } else { [255; 4] }).collect();
        let source = voxear_render::TextureSource { name: "half", width: 16, height: 4, rgba: &rgba, srgb: false };
        let cookie = renderer.add_texture(&source).unwrap();
        Light { cookie: Some(cookie), ..Light::spot(Vec3::ONE, 6.0, 8.0, 50.0, 60.0) }
    };
    let Some(pixels) = hanging_light_render(half_and_half, 3.0) else { return };
    keep_image("spot cookie.png", &pixels);
    let (dark, lit) =
        (floor_brightness(&pixels, Vec3::new(-0.8, 0.0, 0.0)), floor_brightness(&pixels, Vec3::new(0.8, 0.0, 0.0)));
    assert!(lit > 100, "the white half shines: {lit}");
    assert!(dark <= 6, "the black half does not: {dark}");
}

#[test]
fn an_ies_profile_dims_a_light_where_its_fitting_is_dark() {
    const DOWNLIGHT: &str = "IESNA:LM-63-2002\nTILT=NONE\n1 1000 1.0 7 1 1 2 0.1 0.1 0.0\n1.0 1.0 12\n\
                             0 15 30 45 60 75 90\n0\n1000 950 800 500 200 50 0\n";
    let profile = voxear_render::IesProfile::parse(DOWNLIGHT).unwrap();
    let plain = |_: &mut Renderer| Light::point(Vec3::ONE, 4.0, 8.0);
    let profiled = move |renderer: &mut Renderer| {
        let profile = renderer.add_light_profile(&profile).unwrap();
        Light { profile: Some(profile), ..Light::point(Vec3::ONE, 4.0, 8.0) }
    };
    let (Some(bare), Some(shaped)) = (hanging_light_render(plain, 1.0), hanging_light_render(profiled, 1.0)) else {
        return;
    };
    keep_image("ies profile.png", &shaped);
    let (under_bare, under_shaped) = (floor_brightness(&bare, Vec3::ZERO), floor_brightness(&shaped, Vec3::ZERO));
    assert!(under_shaped * 10 >= under_bare * 9, "straight down it is as bright: {under_shaped} of {under_bare}");
    let aside = Vec3::new(3.0, 0.0, 0.0);
    let (aside_bare, aside_shaped) = (floor_brightness(&bare, aside), floor_brightness(&shaped, aside));
    assert!(aside_shaped * 3 < aside_bare, "far to the side, the fitting is dark: {aside_shaped} of {aside_bare}");
}

/// Fog at head height looking out over open ground under an overhead sun,
/// with a roof shading the left half of the view and, optionally, a lamp
/// floating in front.
fn fog_render(volumetric: bool, lamp: bool) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, Fog, PostProcess, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, 0.0))));
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, down, Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let roof = Transform::new(Vec3::new(-50.0, 12.0, 50.0), Quat::IDENTITY, Vec3::new(100.0, 0.5, 120.0));
    world.spawn((MeshRenderer::new(cube, material), roof));
    if lamp {
        let at = Transform::from_position(Vec3::new(1.5, 1.5, 4.0));
        world.spawn((Light::point(Vec3::new(1.0, 0.6, 0.3), 8.0, 3.0), at));
    }
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let fog = Fog { volumetric, density: 0.05, height_falloff: 0.0001, ..Fog::default() };
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    world.insert_resource(PostProcess { fog, anti_aliasing, ..PostProcess::neutral() });
    run(&mut renderer, &world, &mut 0.0, 8);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn volumetric_fog_is_dark_in_shadow_and_glows_round_a_lamp() {
    let (Some(shafts), Some(lamp), Some(plain)) = (fog_render(true, false), fog_render(true, true), fog_render(false, true))
    else {
        return;
    };
    keep_image("volumetric fog.png", &shafts);
    keep_image("volumetric fog lamp.png", &lamp);
    keep_image("plain fog lamp.png", &plain);
    let sum = |pixels: &[u8], at: (u32, u32)| rgb_at(pixels, at).iter().sum::<i32>();
    // Just above the horizon, looking out under the roof and beside it.
    let (shaded, sunlit) = (sum(&shafts, (120, 170)), sum(&shafts, (520, 170)));
    assert!(sunlit > shaded + 30, "the sunlit fog is brighter than the fog under the roof: {sunlit} against {shaded}");
    let (plain_left, plain_right) = (sum(&plain, (120, 170)), sum(&plain, (520, 170)));
    assert!(plain_left.abs_diff(plain_right) <= 6, "plain fog knows nothing of shadows: {plain_left}, {plain_right}");

    // The lamp lights the fog round it, which plain fog cannot.
    let beside_lamp = |pixels: &[u8]| sum(pixels, (400, 190));
    assert!(beside_lamp(&lamp) > beside_lamp(&shafts) + 20, "{} against {}", beside_lamp(&lamp), beside_lamp(&shafts));
    let red_glow = rgb_at(&lamp, (400, 190))[0] - rgb_at(&shafts, (400, 190))[0];
    assert!(red_glow > 0, "the glow takes the lamp's colour");
}

/// A tall wall on a floor seen from above, ending at z = 0, its shadow cast
/// along +X by a sun 40 degrees from straight down, as soft as `softness`
/// makes it. The shadow's edge runs along X from where the wall ends.
fn post_shadow_render(softness: f32) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, PostProcess, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(LAMP_CAMERA, lamp_camera_rotation(), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = Transform::new(Vec3::new(0.0, 3.0, -5.0), Quat::IDENTITY, Vec3::new(0.5, 6.0, 10.0));
    world.spawn((MeshRenderer::new(cube, material), wall));
    let towards_sun = Vec3::new(-40f32.to_radians().sin(), 40f32.to_radians().cos(), 0.0);
    let sun = Quat::from_rotation_arc(Vec3::Z, -towards_sun);
    let light = Light { softness, ..Light::directional(Vec3::ONE, 2.0) };
    world.spawn((light, Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

/// How wide, in metres, the shadow's edge goes from mostly shaded to mostly
/// lit, across it at `x`, averaged along a band either side of it so each
/// pixel's noise evens out.
fn penumbra_width(pixels: &[u8], x: f32) -> f32 {
    let step = 0.02;
    let across = |z: f32| (-4..=4).map(|i| floor_brightness(pixels, Vec3::new(x + i as f32 * 0.04, 0.0, z))).sum::<i32>();
    let values: Vec<i32> = (0..150).map(|i| across(-1.5 + i as f32 * step)).collect();
    let (shaded, lit) = (*values.iter().min().unwrap(), *values.iter().max().unwrap());
    let range = (lit - shaded) as f32;
    let between = values.iter().filter(|&&v| (v - shaded) as f32 > range * 0.2 && ((v - shaded) as f32) < range * 0.8);
    between.count() as f32 * step
}

#[test]
fn a_soft_sun_softens_shadows_more_the_farther_they_fall() {
    let (Some(hard), Some(soft)) = (post_shadow_render(0.0), post_shadow_render(4.0)) else { return };
    keep_image("hard shadow.png", &hard);
    keep_image("soft shadow.png", &soft);
    let (soft_near, soft_far) = (penumbra_width(&soft, 0.6), penumbra_width(&soft, 4.5));
    let hard_far = penumbra_width(&hard, 4.5);
    assert!(soft_far > soft_near * 2.0, "wider far from the wall: {soft_far} m against {soft_near} m by its foot");
    assert!(soft_far > hard_far * 2.0, "wider than a hard shadow: {soft_far} m against {hard_far} m");
}

/// A box on a floor 120 m away, beyond the reach of shadow maps, seen
/// through a narrow lens under a low sun, with contact shadows on or off.
fn far_box_render(contact: bool) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, ContactShadows, PostProcess, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let camera = Camera { fov_y_degrees: 4.0, ..Camera::default() };
    let looking = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -20.0, 120.0).normalize());
    world.spawn((camera, Transform::new(Vec3::new(0.0, 20.0, 0.0), looking, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    let floor = Transform::new(Vec3::new(0.0, 0.0, 120.0), Quat::IDENTITY, Vec3::splat(40.0));
    world.spawn((MeshRenderer::new(plane, material), floor));
    world.spawn((MeshRenderer::new(cube, material), Transform::from_position(Vec3::new(0.0, 0.5, 120.0))));
    // Low, from the left, so the box's shadow falls to its right on screen.
    let towards_sun = Vec3::new(-1.0, 0.5, 0.0).normalize();
    world.spawn((Light::directional(Vec3::ONE, 2.0), Transform::new(Vec3::ZERO, Quat::from_rotation_arc(Vec3::Z, -towards_sun), Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, ..AntiAliasing::default() };
    // No sharpening, which rings round any new dark edge.
    let anti_aliasing = AntiAliasing { sharpness: 0.0, ..anti_aliasing };
    let contact_shadows = ContactShadows { enabled: contact, length: 1.5, thickness: 1.0, steps: 24 };
    let mut post = PostProcess { anti_aliasing, contact_shadows, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn contact_shadows_darken_the_floor_by_a_box_where_shadow_maps_do_not_reach() {
    let (Some(without), Some(with)) = (far_box_render(false), far_box_render(true)) else { return };
    keep_image("far box without contact shadows.png", &without);
    keep_image("far box with contact shadows.png", &with);
    // The floor just to the box's right, where its shadow falls.
    let darkened = (0..640 * 360).filter(|&i| {
        let (a, b) = (&without[i * 4..i * 4 + 3], &with[i * 4..i * 4 + 3]);
        a.iter().zip(b).map(|(a, b)| i32::from(*a) - i32::from(*b)).sum::<i32>() > 30
    });
    let count = darkened.count();
    // The floor is seen nearly edge on from so far away, so the strip is thin.
    assert!(count > 200, "contact shadows darken part of the floor: {count} pixels");
    let lighter = (0..640 * 360).filter(|&i| {
        let (a, b) = (&without[i * 4..i * 4 + 3], &with[i * 4..i * 4 + 3]);
        a.iter().zip(b).map(|(a, b)| i32::from(*b) - i32::from(*a)).sum::<i32>() > 6
    });
    assert_eq!(lighter.count(), 0, "and lighten nothing");
}

/// A lookup table of `size`, made by `grade` from each colour it holds, in
/// the strip `[post.grading] lut` reads.
fn lut(size: u32, grade: impl Fn([u8; 3]) -> [u8; 3]) -> Vec<u8> {
    let level = |value: u32| ((value * 255 + (size - 1) / 2) / (size - 1)) as u8;
    let mut rgba = Vec::new();
    for green in 0..size {
        for blue in 0..size {
            for red in 0..size {
                let [r, g, b] = grade([level(red), level(green), level(blue)]);
                rgba.extend([r, g, b, 255]);
            }
        }
    }
    rgba
}

/// Coloured cubes on a grey floor in daylight, with `post` and, perhaps, a
/// lookup table and post volumes, the camera at `camera`.
fn graded_render(
    post: voxear_render::PostProcess,
    table: Option<&[u8]>,
    volumes: &[(voxear_render::PostVolume, Transform)],
    camera: Vec3,
) -> Option<Vec<u8>> {
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(camera, Quat::from_rotation_x(0.3), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    for (x, colour) in [(-1.5, [0.9, 0.2, 0.2, 1.0]), (0.0, [0.2, 0.8, 0.3, 1.0]), (1.5, [0.2, 0.3, 0.9, 1.0])] {
        let material = renderer.add_material(&MaterialDesc { base_color: colour, ..MaterialDesc::default() }).unwrap();
        let place = Transform::from_position(Vec3::new(x + camera.x, 0.5, camera.z + 5.0));
        world.spawn((MeshRenderer::new(cube, material), place));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 2.5), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    for (volume, place) in volumes {
        world.spawn((volume.clone(), *place));
    }
    let mut post = post;
    post.grading.lut = table.map(|rgba| renderer.add_lut(rgba, (rgba.len() as f64 / 4.0).cbrt().round() as u32).unwrap());
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

fn steady_post() -> voxear_render::PostProcess {
    use voxear_render::{AntiAliasing, AntiAliasingMode, PostProcess};
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    PostProcess { anti_aliasing, ..PostProcess::neutral() }
}

/// The largest difference in any channel between two pictures.
fn largest_difference(a: &[u8], b: &[u8]) -> i32 {
    a.iter().zip(b).map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs()).max().unwrap_or(0)
}

#[test]
fn a_lookup_table_grades_the_picture_as_much_as_it_is_asked() {
    let at = Vec3::new(0.0, 1.6, 0.0);
    let neutral = lut(32, |colour| colour);
    let reds = lut(32, |[r, g, b]| [((u32::from(r) + u32::from(g) + u32::from(b)) / 3) as u8, 0, 0]);
    let (Some(plain), Some(through_neutral), Some(red)) = (
        graded_render(steady_post(), None, &[], at),
        graded_render(steady_post(), Some(&neutral), &[], at),
        graded_render(steady_post(), Some(&reds), &[], at),
    ) else {
        return;
    };
    keep_image("graded red.png", &red);
    let off = largest_difference(&plain, &through_neutral);
    assert!(off <= 3, "a neutral table changes nothing but rounding: {off}");
    let green_left = red.chunks_exact(4).map(|pixel| i32::from(pixel[1])).max().unwrap();
    assert!(green_left <= 2, "everything is red: green reaches {green_left}");

    let mut half = steady_post();
    half.grading.lut_contribution = 0.5;
    let Some(halfway) = graded_render(half, Some(&reds), &[], at) else { return };
    let pixel = (320, 250);
    let (a, b, c) = (rgb_at(&plain, pixel)[1], rgb_at(&halfway, pixel)[1], rgb_at(&red, pixel)[1]);
    assert!(b < a && b > c, "half the table: green {b} between {a} and {c}");
}

#[test]
fn a_post_volume_changes_the_picture_where_the_camera_is_inside_it() {
    use voxear_render::{PostVolume, VolumeShape};
    let grey: toml::Table = toml::from_str("grading = { saturation = 0.0 }").unwrap();
    let volume = PostVolume { shape: VolumeShape::Box, blend_distance: 2.0, overrides: grey, ..PostVolume::default() };
    let room = Transform::new(Vec3::new(0.0, 2.0, 0.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 8.0));
    let (inside, outside) = (Vec3::new(0.0, 1.6, 0.0), Vec3::new(30.0, 1.6, 0.0));
    let (Some(plain), Some(within), Some(beyond)) = (
        graded_render(steady_post(), None, &[], inside),
        graded_render(steady_post(), None, &[(volume.clone(), room)], inside),
        graded_render(steady_post(), None, &[(volume, room)], outside),
    ) else {
        return;
    };
    keep_image("post volume inside.png", &within);
    let colourful = |pixels: &[u8]| pixels.chunks_exact(4).map(|p| i32::from(p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2]))).max().unwrap();
    assert!(colourful(&plain) > 60, "the cubes have colour: {}", colourful(&plain));
    assert!(colourful(&within) <= 3, "inside the volume everything is grey: {}", colourful(&within));
    assert!(colourful(&beyond) > 60, "well outside it, colour is back: {}", colourful(&beyond));
}

/// The coloured cubes after enough frames for TAA to settle, drawn at
/// `scale` of the picture's size, with TAA on or off.
fn upscaled_render(scale: f32, taa: bool) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode};
    let mode = if taa { AntiAliasingMode::Taa } else { AntiAliasingMode::Off };
    let anti_aliasing = AntiAliasing { mode, sharpness: 0.0, render_scale: scale, ..AntiAliasing::default() };
    let post = voxear_render::PostProcess { anti_aliasing, ..steady_post() };
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.6, 0.0), Quat::from_rotation_x(0.3), Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    for (x, colour) in [(-1.5, [0.9, 0.2, 0.2, 1.0]), (0.0, [0.2, 0.8, 0.3, 1.0]), (1.5, [0.2, 0.3, 0.9, 1.0])] {
        let material = renderer.add_material(&MaterialDesc { base_color: colour, ..MaterialDesc::default() }).unwrap();
        let place = Transform::new(Vec3::new(x, 0.5, 5.0), Quat::from_rotation_y(0.4), Vec3::ONE);
        world.spawn((MeshRenderer::new(cube, material), place));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 2.5), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 40);
    Some(renderer.read_pixels().unwrap().2)
}

/// The average difference per channel between two pictures.
fn mean_difference(a: &[u8], b: &[u8]) -> f32 {
    let total: i64 = a.iter().zip(b).map(|(a, b)| (i64::from(*a) - i64::from(*b)).abs()).sum();
    total as f32 / a.len() as f32
}

#[test]
fn taa_upscales_a_scene_drawn_at_half_size_close_to_the_full_one() {
    let (Some(full), Some(half)) = (upscaled_render(1.0, true), upscaled_render(0.5, true)) else { return };
    keep_image("taa full size.png", &full);
    keep_image("taa upscaled from half.png", &half);
    let apart = mean_difference(&full, &half);
    assert!(apart < 2.0, "upscaled from a quarter of the pixels, it is close: {apart} apart on average");

    // Without TAA there is nothing to build the picture from, so the scale is not used.
    let (Some(plain), Some(plain_scaled)) = (upscaled_render(1.0, false), upscaled_render(0.5, false)) else { return };
    assert_eq!(mean_difference(&plain, &plain_scaled), 0.0, "without TAA the scene is drawn at full size");
}

/// A white floor seen from above, with a sunlit red wall standing on it
/// along X at z = 1.
fn gi_render(bounce: Bounce) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = bounce_renderer(bounce)?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(LAMP_CAMERA, lamp_camera_rotation(), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.9, 0.9, 1.0], ..MaterialDesc::default() }).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, white), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = Transform::new(Vec3::new(2.0, 1.0, 1.25), Quat::IDENTITY, Vec3::new(3.0, 2.0, 0.5));
    world.spawn((MeshRenderer::new(cube, red), wall));
    // The sun from the open side, low enough to light the wall's face towards the camera's floor.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.6, 0.8).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.global_illumination = bounce_settings(bounce);
    world.insert_resource(post);
    // Long enough for every probe to be traced again since the GI warmed up.
    run(&mut renderer, &world, &mut 0.0, 34);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn gi_bounces_a_walls_colour_and_keeps_the_sky_from_behind_it() {
    let (Some(plain), Some(lit)) = (gi_render(Bounce::Off), gi_render(Bounce::Traced)) else { return };
    keep_image("gi off.png", &plain);
    keep_image("gi on.png", &lit);
    // On the floor in front of the red wall's sunlit face, the floor turns redder.
    let near_wall = |pixels: &[u8]| rgb_at(pixels, floor_pixel(Vec3::new(2.0, 0.0, 0.5)));
    let (before, after) = (near_wall(&plain), near_wall(&lit));
    let redness = |rgb: [i32; 3]| rgb[0] - (rgb[1] + rgb[2]) / 2;
    assert!(redness(after) > redness(before) + 6, "the wall's red bounces onto the floor: {after:?} from {before:?}");
    // In the wall's shadow behind it, past where its top hides the floor from above, the wall keeps out
    // some of the sky, which the plain ambient cannot do; the sunlit floor round it gives some back.
    let behind = |pixels: &[u8]| rgb_at(pixels, floor_pixel(Vec3::new(2.0, 0.0, 2.4))).iter().sum::<i32>();
    let open = |pixels: &[u8]| rgb_at(pixels, floor_pixel(Vec3::new(-5.0, 0.0, -3.0))).iter().sum::<i32>();
    assert!(behind(&lit) * 100 < behind(&plain) * 97, "darker behind the wall: {} from {}", behind(&lit), behind(&plain));
    assert!(open(&lit) * 10 > open(&plain) * 8, "out in the open, little changes: {} from {}", open(&lit), open(&plain));
}

/// The pixel the floor at `point` lands on, seen from the lamp camera.
fn floor_pixel(point: Vec3) -> (u32, u32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_rotation_translation(lamp_camera_rotation(), LAMP_CAMERA).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    ((x * 640.0) as u32, (y * 360.0) as u32)
}

/// A polished floor with a red cube on it, seen from head height, with
/// TAA, after enough frames that reflections have last frame's picture to
/// read, with screen-space reflections on or off.
fn reflection_render(on: bool) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, Reflections};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::from_rotation_x(0.25), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let polished = MaterialDesc { base_color: [0.05, 0.05, 0.05, 1.0], roughness: 0.05, ..MaterialDesc::default() };
    let floor = renderer.add_material(&polished).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, floor), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    world.spawn((MeshRenderer::new(cube, red), Transform::new(Vec3::new(0.0, 0.5, 4.0), Quat::IDENTITY, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Taa, sharpness: 0.0, ..AntiAliasing::default() };
    let reflections = Reflections { enabled: on, ..Reflections::default() };
    world.insert_resource(voxear_render::PostProcess { anti_aliasing, reflections, ..steady_post() });
    run(&mut renderer, &world, &mut 0.0, 12);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_polished_floor_reflects_what_stands_on_it() {
    let (Some(without), Some(with)) = (reflection_render(false), reflection_render(true)) else { return };
    keep_image("floor without reflections.png", &without);
    keep_image("floor with reflections.png", &with);
    // Just below the cube's foot on screen, the floor mirrors its red front: how much redder than it is
    // green and blue there, on average.
    let redness = |pixels: &[u8]| {
        let patch: Vec<i32> = (200..240)
            .flat_map(|y| (290..350).map(move |x| (x, y)))
            .map(|at| {
                let [r, g, b] = rgb_at(pixels, at);
                r - (g + b) / 2
            })
            .collect();
        patch.iter().sum::<i32>() / patch.len() as i32
    };
    let (red_without, red_with) = (redness(&without), redness(&with));
    assert!(red_with > red_without + 20, "the cube shows in the floor: {red_with} redder there, from {red_without}");
}

/// Where the chrome ball of [`chrome_ball_render`] stands, and how big it is.
const CHROME_BALL: Vec3 = Vec3::new(0.0, 0.9, 0.0);
const CHROME_BALL_RADIUS: f32 = 0.9;

/// A chrome ball on a grey floor between a purple and a blue cube, seen
/// from three metres, after enough frames of TAA that screen-space
/// reflections show what is round it, turned `turn` radians about its
/// upright.
fn chrome_ball_render(turn: f32) -> Option<Vec<u8>> {
    use voxear_render::{AntiAliasing, AntiAliasingMode, Reflections};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let eye = Vec3::new(0.0, 1.6, -3.0);
    let forward = (CHROME_BALL - eye).normalize();
    let looking = Quat::from_rotation_y(forward.x.atan2(forward.z)) * Quat::from_rotation_x(-forward.y.asin());
    world.spawn((Camera::default(), Transform::new(eye, looking, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let mut material = |base_color: [f32; 4], metallic: f32, roughness: f32| {
        renderer.add_material(&MaterialDesc { base_color, metallic, roughness, ..MaterialDesc::default() }).unwrap()
    };
    let (grey, purple, blue) = (
        material([0.42, 0.44, 0.46, 1.0], 0.0, 0.9),
        material([0.55, 0.36, 0.96, 1.0], 0.0, 0.35),
        material([0.24, 0.62, 0.98, 1.0], 0.0, 0.5),
    );
    let chrome = material([0.95, 0.93, 0.9, 1.0], 1.0, 0.05);
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(30.0))));
    for (x, colour) in [(-2.2, purple), (2.2, blue)] {
        let place = Transform::new(Vec3::new(x, 0.75, -0.5), Quat::IDENTITY, Vec3::splat(1.5));
        world.spawn((MeshRenderer::new(cube, colour), place));
    }
    let ball = Transform::new(CHROME_BALL, Quat::from_rotation_y(turn), Vec3::splat(2.0 * CHROME_BALL_RADIUS));
    world.spawn((MeshRenderer::new(sphere, chrome), ball));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -1.0, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Taa, sharpness: 0.0, ..AntiAliasing::default() };
    let reflections = Reflections { enabled: true, ..Reflections::default() };
    let mut post = voxear_render::PostProcess { anti_aliasing, reflections, ..steady_post() };
    post.ambient_occlusion.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 12);
    Some(renderer.read_pixels().unwrap().2)
}

/// The pixels within `share` of a ball's radius of its centre on screen, at
/// the middle of a 640×360 picture whose camera looks at it from `distance`.
fn ball_pixels(distance: f32, radius: f32, share: f32) -> Vec<(u32, u32)> {
    let pixels_per_unit = 180.0 / 30f32.to_radians().tan();
    let across = pixels_per_unit * (radius / distance).asin().tan() * share;
    let reach = across.ceil() as i32;
    (-reach..=reach)
        .flat_map(|y| (-reach..=reach).map(move |x| (x, y)))
        .filter(|&(x, y)| ((x * x + y * y) as f32) < across * across)
        .map(|(x, y)| ((320 + x) as u32, (180 + y) as u32))
        .collect()
}

#[test]
fn a_chrome_ball_mirrors_its_surroundings_smoothly_not_triangle_by_triangle() {
    // Turned by half the width of one of its triangles, a smooth ball mirrors the same; one whose reflections are
    // traced from its flat triangles shows each as a block of one colour, and the blocks move as it turns.
    let half_a_triangle = std::f32::consts::PI / 64.0;
    let (Some(still), Some(turned)) = (chrome_ball_render(0.0), chrome_ball_render(half_a_triangle)) else { return };
    keep_image("chrome ball.png", &still);
    keep_image("chrome ball turned.png", &turned);
    let distance = Vec3::new(0.0, 1.6, -3.0).distance(CHROME_BALL);
    let ball = ball_pixels(distance, CHROME_BALL_RADIUS, 0.85);
    let moved = |at: &(u32, u32)| {
        let (a, b) = (rgb_at(&still, *at), rgb_at(&turned, *at));
        (0..3).map(|c| a[c].abs_diff(b[c])).sum::<u32>() > 24
    };
    let changed = ball.iter().filter(|at| moved(at)).count();
    assert!(changed * 100 < ball.len(), "{changed} of the ball's {} pixels changed as it turned", ball.len());
}

#[test]
fn a_lod_crossfade_leaves_no_dark_patches_of_ambient_occlusion() {
    // A matte ball alone in the sky, which nothing occludes, seen through ambient occlusion's debug view.
    let options = ViewOptions { debug_view: voxear_render::DEBUG_VIEW_AMBIENT_OCCLUSION, ..ViewOptions::default() };
    let Some(mut renderer) = renderer_culling(Meshlets::Compute, options) else { return };
    let mut world = World::new();
    let camera = world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -3.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(sphere, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(3.0))));
    let mut time = 0.0;
    let near = run(&mut renderer, &world, &mut time, 4);
    assert!(near.lods[0] > 0 && near.fade_draws == 0, "{near:?}");

    // Backing away until it crossfades to a simpler LOD, and a few frames into the fade, where the depth is
    // either LOD's, pixel by pixel.
    let mut distance = 3.0;
    while run(&mut renderer, &world, &mut time, 0).fade_draws == 0 {
        distance += 0.05;
        assert!(distance < 12.0, "the ball never changed LOD");
        world.get_mut::<Transform>(camera).unwrap().position = Vec3::new(0.0, 0.0, -distance);
    }
    let fading = run(&mut renderer, &world, &mut time, 4);
    assert!(fading.fade_draws > 0, "{fading:?}");
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("lod crossfade ao.png", &pixels);
    let ball = ball_pixels(distance, 1.5, 0.85);
    let darkened = ball.iter().filter(|&&at| rgb_at(&pixels, at)[0] < 245).count();
    assert!(darkened * 200 < ball.len(), "{darkened} of the ball's {} pixels are occluded", ball.len());
}

/// The lamp camera's floor under a sun 40 degrees from straight down, a
/// tall wall's shadow falling along +X from where it ends at z = 0, with
/// `light` as the sun and, perhaps, a look's `sun_look` over it.
fn shadow_tint_render(light: Light, sun_look: Option<voxear_render::SunLook>) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(LAMP_CAMERA, lamp_camera_rotation(), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let material = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((MeshRenderer::new(plane, material), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = Transform::new(Vec3::new(0.0, 3.0, -5.0), Quat::IDENTITY, Vec3::new(0.5, 6.0, 10.0));
    world.spawn((MeshRenderer::new(cube, material), wall));
    let towards_sun = Vec3::new(-40f32.to_radians().sin(), 40f32.to_radians().cos(), 0.0);
    world.spawn((light, Transform::new(Vec3::ZERO, Quat::from_rotation_arc(Vec3::Z, -towards_sun), Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    if let Some(look) = sun_look {
        world.insert_resource(look);
    }
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

/// In the wall's shadow, and out in the sun.
const IN_SHADOW: Vec3 = Vec3::new(2.0, 0.0, -2.5);
const IN_SUN: Vec3 = Vec3::new(2.0, 0.0, 2.5);

#[test]
fn a_shadow_tint_colours_shade_far_more_than_sunlit_ground() {
    let plain = Light::directional(Vec3::ONE, 2.0);
    let tinted = Light { shadow_tint: Vec3::new(0.4, 0.05, 0.3), ..plain };
    let (Some(before), Some(after)) = (shadow_tint_render(plain, None), shadow_tint_render(tinted, None)) else {
        return;
    };
    keep_image("shadow untinted.png", &before);
    keep_image("shadow tinted.png", &after);
    let (shade_before, shade_after) = (rgb_at(&before, floor_pixel(IN_SHADOW)), rgb_at(&after, floor_pixel(IN_SHADOW)));
    let purpleness = |rgb: [i32; 3]| rgb[0] + rgb[2] - 2 * rgb[1];
    assert!(purpleness(shade_after) > purpleness(shade_before) + 10, "{shade_after:?} from {shade_before:?}");
    // The sun at an angle leaves a little of the tint on lit ground too, as the light turns away.
    let (sun_before, sun_after) = (rgb_at(&before, floor_pixel(IN_SUN)), rgb_at(&after, floor_pixel(IN_SUN)));
    let (in_sun, in_shade) = (purpleness(sun_after) - purpleness(sun_before), purpleness(shade_after) - purpleness(shade_before));
    assert!(in_sun * 3 < in_shade, "the sunlit floor changes far less: {sun_after:?} from {sun_before:?}");
}

#[test]
fn a_sun_look_colours_the_sun_and_tints_its_shadows() {
    use voxear_render::SunLook;
    let light = Light::directional(Vec3::ONE, 2.0);
    let look = SunLook {
        tint: Vec3::new(1.0, 0.6, 0.3),
        strength: 1.0,
        softness: 0.5,
        shadow_tint: Vec3::new(0.0, 0.0, 0.4),
        shadow_distance: voxear_render::SHADOW_DISTANCE,
        shadow_split: voxear_render::SHADOW_SPLIT,
    };
    let (Some(before), Some(after)) = (shadow_tint_render(light, None), shadow_tint_render(light, Some(look))) else {
        return;
    };
    let warmth = |rgb: [i32; 3]| rgb[0] - rgb[2];
    let (sun_before, sun_after) = (rgb_at(&before, floor_pixel(IN_SUN)), rgb_at(&after, floor_pixel(IN_SUN)));
    assert!(warmth(sun_after) > warmth(sun_before) + 20, "the sun turns warm: {sun_after:?} from {sun_before:?}");
    let (shade_before, shade_after) = (rgb_at(&before, floor_pixel(IN_SHADOW)), rgb_at(&after, floor_pixel(IN_SHADOW)));
    assert!(warmth(shade_after) < warmth(shade_before) - 5, "and its shadows blue: {shade_after:?} from {shade_before:?}");
}

/// A thin green panel standing between the camera and a low sun, seen from
/// its shaded side, lit as leaves are or as a plain surface.
fn backlit_panel_render(foliage: bool) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.0, -4.0), Quat::IDENTITY, Vec3::ONE)));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let leaf = MaterialDesc { base_color: [0.2, 0.6, 0.1, 1.0], foliage, ..MaterialDesc::default() };
    let material = renderer.add_material(&leaf).unwrap();
    world.spawn((MeshRenderer::new(cube, material), Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(4.0, 3.0, 0.05))));
    // Low, ahead of the camera, behind the panel.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.2, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn foliage_lets_the_sun_through_from_behind() {
    let (Some(plain), Some(leaf)) = (backlit_panel_render(false), backlit_panel_render(true)) else { return };
    keep_image("backlit panel.png", &plain);
    keep_image("backlit leaf.png", &leaf);
    let (before, after) = (rgb_at(&plain, (320, 180)), rgb_at(&leaf, (320, 180)));
    assert!(after[1] > before[1] + 20, "the leaf glows green against the sun: {after:?} from {before:?}");
    assert!(after[1] > after[0] && after[1] > after[2], "in its own colour: {after:?}");
}

/// A bright white sphere on black with bloom tinted `tint`.
fn bloom_render(tint: [f32; 3]) -> Option<Vec<u8>> {
    use voxear_render::{Bloom, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -5.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let glowing = MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [40.0; 3], ..MaterialDesc::default() };
    let material = renderer.add_material(&glowing).unwrap();
    world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(Vec3::ZERO)));
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let mut post = steady_post();
    post.bloom = Bloom { enabled: true, intensity: 0.2, tint, ..Bloom::default() };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_bloom_tint_colours_the_glow_but_not_the_light() {
    let (Some(white), Some(gold)) = (bloom_render([1.0; 3]), bloom_render([1.0, 0.7, 0.2])) else { return };
    keep_image("bloom white.png", &white);
    keep_image("bloom gold.png", &gold);
    // Beside the sphere, where only its glow reaches.
    let (glow_white, glow_gold) = (rgb_at(&white, (420, 180)), rgb_at(&gold, (420, 180)));
    assert!(glow_white[0] > 5, "the glow reaches out: {glow_white:?}");
    assert!(glow_gold[0] - glow_gold[2] > glow_white[0] - glow_white[2] + 5, "{glow_gold:?} from {glow_white:?}");
}

/// How a test scene's light bounces.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bounce {
    Off,
    /// Probes whose rays are traced by the GPU's ray tracing.
    Traced,
    /// The same probes, their rays traced in a compute shader.
    Software,
}

/// A renderer for `bounce`, or none where this GPU cannot trace rays.
fn bounce_renderer(bounce: Bounce) -> Option<Renderer> {
    let renderer = renderer_with(&RendererSettings { software_rays: bounce == Bounce::Software, ..Default::default() })?;
    if bounce == Bounce::Traced && !renderer.gpu().ray_tracing {
        eprintln!("skipping the ray-traced scene: this GPU has no ray tracing");
        return None;
    }
    Some(renderer)
}

/// The GI settings for `bounce`.
fn bounce_settings(bounce: Bounce) -> voxear_render::GlobalIllumination {
    voxear_render::GlobalIllumination {
        enabled: bounce != Bounce::Off,
        probe_spacing: 0.5,
        ..Default::default()
    }
}

/// A white floor seen from head height, looking along +Z and a little
/// down, with a red wall standing behind the camera, out of view, lit by a
/// sun ahead, under a dim sky so bounced light shows as it does indoors.
fn wall_behind_render(bounce: Bounce) -> Option<Vec<u8>> {
    wall_behind_cut_out(bounce, None)
}

/// The wall-behind scene, its wall cut out where its picture's alpha, all
/// `alpha` over, is below a half.
fn wall_behind_cut_out(bounce: Bounce, alpha: Option<u8>) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = bounce_renderer(bounce)?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::from_rotation_x(0.6), Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let white = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.9, 0.9, 1.0], ..MaterialDesc::default() }).unwrap();
    let picture = alpha.map(|alpha| {
        let rgba: Vec<u8> = (0..16).flat_map(|_| [255, 255, 255, alpha]).collect();
        let source = voxear_render::TextureSource { name: "holes", width: 4, height: 4, rgba: &rgba, srgb: true };
        renderer.add_texture(&source).unwrap()
    });
    let red = MaterialDesc {
        base_color: [0.9, 0.05, 0.05, 1.0],
        base_color_texture: picture,
        alpha_cutoff: alpha.map(|_| 0.5),
        ..MaterialDesc::default()
    };
    let red = renderer.add_material(&red).unwrap();
    world.spawn((MeshRenderer::new(plane, white), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = Transform::new(Vec3::new(0.0, 2.0, -1.0), Quat::IDENTITY, Vec3::new(8.0, 4.0, 0.5));
    world.spawn((MeshRenderer::new(cube, red), wall));
    // Ahead of the camera and low, lighting the wall's face towards the floor in view, and the floor little.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.25, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.global_illumination = bounce_settings(bounce);
    world.insert_resource(post);
    // Long enough for every probe to be traced again since the GI warmed up.
    run(&mut renderer, &world, &mut 0.0, 34);
    Some(renderer.read_pixels().unwrap().2)
}

/// When in the day the courtyard below is lit.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TimeOfDay {
    Dawn,
    Morning,
    Noon,
    Dusk,
    Midnight,
}

impl TimeOfDay {
    /// The light the sky's sun (or at midnight, the moon) comes from: its
    /// height above the horizon and its bearing from +Z towards +X, in
    /// degrees, its colour and its strength.
    fn light(self) -> (f32, f32, Vec3, f32) {
        match self {
            TimeOfDay::Dawn => (3.0, 120.0, Vec3::ONE, 3.0),
            TimeOfDay::Morning => (25.0, 140.0, Vec3::ONE, 3.0),
            TimeOfDay::Noon => (70.0, 180.0, Vec3::ONE, 3.0),
            TimeOfDay::Dusk => (3.0, 240.0, Vec3::ONE, 3.0),
            TimeOfDay::Midnight => (40.0, 200.0, Vec3::new(0.6, 0.7, 1.0), 0.06),
        }
    }
}

/// A courtyard of pale stone, ten metres across, walled on three sides and
/// open to the south, with a roofed walk along its north wall and a post in
/// its middle, seen from the south; lit by the atmosphere's sun at `time`
/// (or the moon), perhaps with torches under the walk, GI on or off, and
/// `post` for the exposure.
fn courtyard_render(time: TimeOfDay, torches: bool, gi: bool, post: voxear_render::PostProcess) -> Option<Vec<u8>> {
    use voxear_render::{GlobalIllumination, Sky, SkyMode};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let looking = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.35, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 3.0, -8.0), looking, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let stone = renderer.add_material(&MaterialDesc { base_color: [0.7, 0.66, 0.6, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, stone), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    let block = |position: Vec3, scale: Vec3| (MeshRenderer::new(cube, stone), Transform::new(position, Quat::IDENTITY, scale));
    world.spawn(block(Vec3::new(0.0, 3.0, 5.25), Vec3::new(11.0, 6.0, 0.5)));
    world.spawn(block(Vec3::new(5.25, 3.0, 0.0), Vec3::new(0.5, 6.0, 11.0)));
    world.spawn(block(Vec3::new(-5.25, 3.0, 0.0), Vec3::new(0.5, 6.0, 11.0)));
    world.spawn(block(Vec3::new(0.0, 3.1, 4.0), Vec3::new(10.0, 0.2, 2.0)));
    world.spawn(block(Vec3::new(0.0, 1.5, 0.0), Vec3::new(1.0, 3.0, 1.0)));
    let (elevation, bearing, colour, strength) = time.light();
    let (e, b) = (elevation.to_radians(), bearing.to_radians());
    let towards = Vec3::new(e.cos() * b.sin(), e.sin(), e.cos() * b.cos());
    world.spawn((Light::directional(colour, strength), Transform::new(Vec3::ZERO, Quat::from_rotation_arc(Vec3::Z, -towards), Vec3::ONE)));
    if torches {
        for x in [-3.0, 0.0, 3.0] {
            let torch = Light::point(Vec3::new(1.0, 0.55, 0.2), 6.0, 4.0);
            world.spawn((torch, Transform::from_position(Vec3::new(x, 2.0, 4.6))));
        }
    }
    world.insert_resource(Sky { mode: SkyMode::Atmosphere, ..Sky::default() });
    let mut post = post;
    post.contact_shadows.enabled = false;
    post.global_illumination = GlobalIllumination { enabled: gi, probe_spacing: 0.5, ..GlobalIllumination::default() };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 34);
    Some(renderer.read_pixels().unwrap().2)
}

/// How bright a picture is on average, from 0 to 255.
fn mean_brightness(pixels: &[u8]) -> f32 {
    let total: u64 = pixels.chunks(4).map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2])).sum();
    total as f32 / (pixels.len() / 4 * 3) as f32
}

/// How much redder than blue a patch round `at` is, against how bright it is.
fn warmth(pixels: &[u8], (x, y): (u32, u32)) -> f32 {
    let (mut red, mut blue, mut all) = (0, 0, 0);
    for dy in 0..8 {
        for dx in 0..8 {
            let [r, g, b] = rgb_at(pixels, (x + dx, y + dy));
            (red, blue, all) = (red + r, blue + b, all + r + g + b);
        }
    }
    (red - blue) as f32 / all.max(1) as f32
}

/// How much redder than blue a whole picture is, against how bright it is.
fn picture_warmth(pixels: &[u8]) -> f32 {
    let (mut red, mut blue, mut all) = (0i64, 0i64, 0i64);
    for p in pixels.chunks(4) {
        let [r, g, b] = [p[0], p[1], p[2]].map(i64::from);
        (red, blue, all) = (red + r, blue + b, all + r + g + b);
    }
    (red - blue) as f32 / all.max(1) as f32
}

#[test]
fn the_courtyard_brightens_from_dawn_to_noon_and_darkens_through_dusk_to_midnight() {
    use TimeOfDay::*;
    let Some(_) = renderer() else { return };
    let render = |time| courtyard_render(time, false, true, steady_post()).unwrap();
    let pictures = [Dawn, Morning, Noon, Dusk, Midnight].map(|time| (time, render(time)));
    for (time, pixels) in &pictures {
        keep_image(&format!("courtyard {time:?}.png"), pixels);
    }
    let [dawn, morning, noon, dusk, midnight] = pictures.each_ref().map(|(_, pixels)| mean_brightness(pixels));
    assert!(noon > morning && morning > dawn, "brighter as the sun climbs: {dawn:.0}, {morning:.0}, {noon:.0}");
    assert!(noon > dusk, "darker as it sets: {noon:.0} to {dusk:.0}");
    assert!((dawn - dusk).abs() < dawn * 0.25, "dawn and dusk mirror each other: {dawn:.0} and {dusk:.0}");
    assert!(midnight * 4.0 < dusk, "midnight, under the moon, far darker than dusk: {midnight:.0} against {dusk:.0}");
}

#[test]
fn a_low_sun_lights_walls_warmer_than_the_noon_sun() {
    use TimeOfDay::*;
    let Some(_) = renderer() else { return };
    let render = |time| courtyard_render(time, false, true, steady_post()).unwrap();
    let (dawn, noon, dusk) = (render(Dawn), render(Noon), render(Dusk));
    // Dawn lights the west wall, on the left, and dusk the east wall, on the right.
    let (left, right) = ((30, 100), (600, 100));
    let (dawn_left, noon_left) = (warmth(&dawn, left), warmth(&noon, left));
    let (dusk_right, noon_right) = (warmth(&dusk, right), warmth(&noon, right));
    assert!(dawn_left > noon_left + 0.15, "dawn's light is redder: {dawn_left:.2} against noon's {noon_left:.2}");
    assert!(dusk_right > noon_right + 0.15, "and dusk's: {dusk_right:.2} against noon's {noon_right:.2}");
}

#[test]
fn torches_light_the_night_warm_and_their_light_bounces_with_gi() {
    let Some(_) = renderer() else { return };
    let render = |torches, gi| courtyard_render(TimeOfDay::Midnight, torches, gi, steady_post()).unwrap();
    let (moonlit, plain, bounced) = (render(false, true), render(true, false), render(true, true));
    keep_image("courtyard torches without gi.png", &plain);
    keep_image("courtyard torches with gi.png", &bounced);
    let (cold, warm) = (picture_warmth(&moonlit), picture_warmth(&bounced));
    assert!(warm > cold + 0.1, "torchlight warms the night: {warm:.2} against the moon's {cold:.2}");
    let (direct, with_bounce) = (mean_brightness(&plain), mean_brightness(&bounced));
    assert!(with_bounce > direct * 1.2, "their light bounces off the walk onto the walls: {with_bounce:.0} from {direct:.0}");
}

#[test]
fn auto_exposure_keeps_midnight_darker_than_noon() {
    let Some(_) = renderer() else { return };
    let render = |time| courtyard_render(time, false, true, voxear_render::PostProcess::default()).unwrap();
    let (noon, midnight) = (mean_brightness(&render(TimeOfDay::Noon)), mean_brightness(&render(TimeOfDay::Midnight)));
    assert!(midnight * 3.0 < noon, "the eye adapts, but night stays night: {midnight:.0} against {noon:.0}");
}

/// The sky seen looking up at the moon or the sun 40 degrees high,
/// under the atmosphere or a dark gradient, with auto exposure as a scene
/// has it.
fn night_sky_render(body: voxear_render::SkyBody, atmosphere: bool) -> Option<Vec<u8>> {
    use voxear_render::{Sky, SkyMode};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let towards = Vec3::new(0.0, 40f32.to_radians().sin(), 40f32.to_radians().cos());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.7, 0.0), Quat::from_rotation_arc(Vec3::Z, towards), Vec3::ONE)));
    // The moon as games make it, bright enough to see by; the sun as by day.
    let light = match body {
        voxear_render::SkyBody::Moon => Light { body, ..Light::directional(Vec3::new(0.6, 0.7, 1.0), 0.15) },
        voxear_render::SkyBody::Sun => Light::directional(Vec3::ONE, 3.0),
    };
    world.spawn((light, Transform::new(Vec3::ZERO, Quat::from_rotation_arc(Vec3::Z, -towards), Vec3::ONE)));
    let sky = match atmosphere {
        true => Sky { mode: SkyMode::Atmosphere, ..Sky::default() },
        false => Sky {
            zenith: Vec3::new(0.002, 0.003, 0.008),
            horizon: Vec3::new(0.004, 0.005, 0.01),
            ground: Vec3::splat(0.002),
            ..Sky::default()
        },
    };
    world.insert_resource(sky);
    let anti_aliasing = voxear_render::AntiAliasing { mode: voxear_render::AntiAliasingMode::Off, ..Default::default() };
    world.insert_resource(voxear_render::PostProcess { anti_aliasing, ..Default::default() });
    run(&mut renderer, &world, &mut 0.0, 30);
    Some(renderer.read_pixels().unwrap().2)
}

/// Pixels in the picture's top third far brighter than the sky round them: stars.
fn star_count(pixels: &[u8]) -> usize {
    let bright = |at| rgb_at(pixels, at).iter().sum::<i32>();
    (2..118u32)
        .flat_map(|y| (2..638u32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let around = (bright((x - 2, y)) + bright((x + 2, y)) + bright((x, y - 2)) + bright((x, y + 2))) / 4;
            bright((x, y)) > around + 60 && bright((x, y)) >= bright((x + 1, y)) && bright((x, y)) >= bright((x, y + 1))
        })
        .count()
}

#[test]
fn under_the_moon_the_sky_is_dark_with_stars_and_the_moons_disc() {
    use voxear_render::SkyBody;
    let (Some(moonlit), Some(sunlit)) = (night_sky_render(SkyBody::Moon, true), night_sky_render(SkyBody::Sun, true))
    else {
        return;
    };
    keep_image("night sky moon.png", &moonlit);
    keep_image("night sky sun.png", &sunlit);
    let sky = |pixels: &[u8]| mean_brightness(&pixels[..640 * 100 * 4]);
    assert!(star_count(&moonlit) > 5, "stars show under the moon: {}", star_count(&moonlit));
    assert_eq!(star_count(&sunlit), 0, "but not in a day sky");
    // Looking straight at it, the moon's disc lies in the middle of the picture, far brighter than the sky.
    let moon = rgb_at(&moonlit, (320, 180)).iter().sum::<i32>() as f32 / 3.0;
    assert!(moon > sky(&moonlit) * 3.0 + 60.0, "the moon's disc shows: {moon:.0} against the sky's {:.0}", sky(&moonlit));
    assert!(sky(&moonlit) < sky(&sunlit) / 2.0, "the night sky is darker: {:.0} against {:.0}", sky(&moonlit), sky(&sunlit));
    let Some(gradient) = night_sky_render(SkyBody::Moon, false) else { return };
    assert!(star_count(&gradient) > 5, "and over a dark gradient sky too");
}


/// A closed box of walls, floor and roof, three metres inside each way,
/// under a bright sun and sky, seen from inside. Nothing outside can light
/// the inside.
fn sealed_box_render(bounce: Bounce) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = bounce_renderer(bounce)?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.5, -1.0), Quat::IDENTITY, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    let slab = |position: Vec3, scale: Vec3| (MeshRenderer::new(cube, pale), Transform::new(position, Quat::IDENTITY, scale));
    world.spawn(slab(Vec3::new(0.0, 1.5, 1.75), Vec3::new(4.0, 3.0, 0.5)));
    world.spawn(slab(Vec3::new(0.0, 1.5, -1.75), Vec3::new(4.0, 3.0, 0.5)));
    world.spawn(slab(Vec3::new(1.75, 1.5, 0.0), Vec3::new(0.5, 3.0, 4.0)));
    world.spawn(slab(Vec3::new(-1.75, 1.5, 0.0), Vec3::new(0.5, 3.0, 4.0)));
    world.spawn(slab(Vec3::new(0.0, 3.25, 0.0), Vec3::new(4.0, 0.5, 4.0)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 8.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.global_illumination = bounce_settings(bounce);
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 30);
    Some(renderer.read_pixels().unwrap().2)
}

/// Checks that nothing lights the sealed box's inside with `bounce`'s GI,
/// where the plain sky light, without GI, does.
fn assert_sealed_box_stays_dark(bounce: Bounce, name: &str) {
    let (Some(open_sky), Some(traced)) = (sealed_box_render(Bounce::Off), sealed_box_render(bounce)) else {
        return;
    };
    keep_image("sealed box plain sky.png", &open_sky);
    keep_image(name, &traced);
    let (lit, dark) = (mean_brightness(&open_sky), mean_brightness(&traced));
    assert!(lit > 20.0, "without GI, the plain sky light reaches inside: {lit:.1}");
    assert!(dark < 1.0, "with {bounce:?} GI, nothing does: {dark:.2}");
}

/// The sealed box's ceiling, lit only by what a lamp shining down on the
/// floor bounces up: how bright it is with the lamp on, and how bright a
/// third of a second after it goes out.
fn bounce_after_lamp_goes_out(bounce: Bounce) -> Option<(f32, f32)> {
    use voxear_render::Sky;
    let mut renderer = bounce_renderer(bounce)?;
    let mut world = World::new();
    let up = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, 1.0, 0.3).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.2, -1.0), up, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    // A room of middling grey, as most are: light bouncing round it loses half at each wall.
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(60.0))));
    let slab = |position: Vec3, scale: Vec3| (MeshRenderer::new(cube, pale), Transform::new(position, Quat::IDENTITY, scale));
    world.spawn(slab(Vec3::new(0.0, 1.5, 1.75), Vec3::new(4.0, 3.0, 0.5)));
    world.spawn(slab(Vec3::new(0.0, 1.5, -1.75), Vec3::new(4.0, 3.0, 0.5)));
    world.spawn(slab(Vec3::new(1.75, 1.5, 0.0), Vec3::new(0.5, 3.0, 4.0)));
    world.spawn(slab(Vec3::new(-1.75, 1.5, 0.0), Vec3::new(0.5, 3.0, 4.0)));
    world.spawn(slab(Vec3::new(0.0, 3.25, 0.0), Vec3::new(4.0, 0.5, 4.0)));
    let down = Quat::from_rotation_arc(Vec3::Z, -Vec3::Y);
    let lamp = world.spawn((Light::spot(Vec3::ONE, 40.0, 6.0, 30.0, 50.0), Transform::new(Vec3::new(0.0, 2.9, 0.0), down, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    post.global_illumination = bounce_settings(bounce);
    // Fixed, so the eye does not brighten the dark room after.
    post.exposure.auto = false;
    world.insert_resource(post);
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 90);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("bounce lamp on.png", &pixels);
    let lit = mean_brightness(&pixels);
    world.despawn(lamp);
    run(&mut renderer, &world, &mut time, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("bounce lamp off.png", &pixels);
    let after = mean_brightness(&pixels);
    Some((lit, after))
}

#[test]
fn bounced_light_goes_out_soon_after_its_lamp() {
    let Some((lit, after)) = bounce_after_lamp_goes_out(Bounce::Traced) else { return };
    assert!(lit > 10.0, "the ceiling takes the light the floor bounces: {lit:.1}");
    assert!(after < lit * 0.15, "a third of a second after the lamp goes out, its bounce has too: {after:.1} of {lit:.1}");
}

#[test]
fn no_light_leaks_into_a_sealed_box_with_ray_traced_gi() {
    assert_sealed_box_stays_dark(Bounce::Traced, "sealed box ray traced.png");
}

#[test]
fn no_light_leaks_into_a_sealed_box_with_software_rays() {
    assert_sealed_box_stays_dark(Bounce::Software, "sealed box software rays.png");
}

/// How much redder than green and blue the floor is along the bottom of
/// the wall-behind picture, nearest the wall.
fn floor_redness(pixels: &[u8]) -> i32 {
    let patch: Vec<i32> = (330..350)
        .flat_map(|y| (200..440).map(move |x| (x, y)))
        .map(|at| {
            let [r, g, b] = rgb_at(pixels, at);
            r - (g + b) / 2
        })
        .collect();
    patch.iter().sum::<i32>() / patch.len() as i32
}

/// Checks that `bounce`'s GI carries the hidden wall's red onto the floor.
fn assert_floor_takes_the_hidden_walls_red(bounce: Bounce, name: &str) {
    let (Some(plain), Some(lit)) = (wall_behind_render(Bounce::Off), wall_behind_render(bounce)) else { return };
    keep_image("wall behind gi off.png", &plain);
    keep_image(name, &lit);
    let (before, after) = (floor_redness(&plain), floor_redness(&lit));
    assert!(after > before + 6, "the floor takes the hidden wall's red: {after} redder, from {before}");
}

#[test]
fn ray_traced_gi_bounces_the_colour_of_a_wall_the_camera_cannot_see() {
    assert_floor_takes_the_hidden_walls_red(Bounce::Traced, "wall behind ray traced.png");
}

#[test]
fn software_rays_bounce_the_colour_of_a_wall_the_camera_cannot_see() {
    assert_floor_takes_the_hidden_walls_red(Bounce::Software, "wall behind software rays.png");
}

/// Checks that `bounce`'s rays pass through a cut-out wall's holes: one cut
/// away all over bounces no red, and one whose picture is solid bounces it
/// as a plain wall does.
fn assert_rays_pass_through_cut_outs(bounce: Bounce, name: &str) {
    let renders = (wall_behind_render(Bounce::Off), wall_behind_cut_out(bounce, Some(0)), wall_behind_cut_out(bounce, Some(255)));
    let (Some(plain), Some(holes), Some(solid)) = renders else { return };
    keep_image(name, &holes);
    let (before, through, kept) = (floor_redness(&plain), floor_redness(&holes), floor_redness(&solid));
    assert!(through <= before + 2, "a wall cut away all over bounces no red: {through}, from {before}");
    assert!(kept > before + 6, "a cut-out wall whose picture is solid bounces its red: {kept}, from {before}");
}

#[test]
fn ray_traced_gi_passes_through_a_cut_out_materials_holes() {
    assert_rays_pass_through_cut_outs(Bounce::Traced, "cut-out wall ray traced.png");
}

#[test]
fn software_rays_pass_through_a_cut_out_materials_holes() {
    assert_rays_pass_through_cut_outs(Bounce::Software, "cut-out wall software rays.png");
}

/// A red wall and a blue one side by side, meeting at the middle of the
/// picture, seen from three metres off through a glass pane of `pane`'s
/// material standing a metre and a half in front of them, or with no pane.
fn glass_render(pane: Option<MaterialDesc>) -> Option<Vec<u8>> {
    glass_panes_render(&pane.map(|desc| (desc, -1.5)).into_iter().collect::<Vec<_>>())
}

/// The glass scene with panes of these materials, each this far along +Z,
/// towards the walls.
fn glass_panes_render(panes: &[(MaterialDesc, f32)]) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, -3.0))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.6, 0.6, 0.6, 1.0], ..MaterialDesc::default() }).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    let blue = renderer.add_material(&MaterialDesc { base_color: [0.05, 0.1, 0.9, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = |x: f32| Transform::new(Vec3::new(x, 2.0, 0.0), Quat::IDENTITY, Vec3::new(4.0, 4.0, 0.2));
    world.spawn((MeshRenderer::new(cube, red), wall(-2.0)));
    world.spawn((MeshRenderer::new(cube, blue), wall(2.0)));
    for &(desc, z) in panes {
        let glass = renderer.add_material(&desc).unwrap();
        let at = Transform::new(Vec3::new(0.0, 1.5, z), Quat::IDENTITY, Vec3::new(2.0, 1.5, 0.02));
        world.spawn((MeshRenderer::new(cube, glass), at));
    }
    // From behind the camera and above, lighting the walls' faces.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

/// The average colour of a patch of the picture, `half` pixels each way
/// round `centre`.
fn patch_rgb(pixels: &[u8], centre: (u32, u32), half: u32) -> [i32; 3] {
    let points: Vec<[i32; 3]> = (centre.1 - half..=centre.1 + half)
        .flat_map(|y| (centre.0 - half..=centre.0 + half).map(move |x| (x, y)))
        .map(|at| rgb_at(pixels, at))
        .collect();
    let count = points.len() as i32;
    [0, 1, 2].map(|c| points.iter().map(|p| p[c]).sum::<i32>() / count)
}

/// Clear glass, as the built-in material is.
fn clear_glass() -> MaterialDesc {
    MaterialDesc { roughness: 0.05, glass: true, ..MaterialDesc::default() }
}

#[test]
fn clear_glass_shows_what_lies_behind_it() {
    let (Some(open), Some(glazed)) = (glass_render(None), glass_render(Some(clear_glass()))) else { return };
    keep_image("glass none.png", &open);
    keep_image("glass clear.png", &glazed);
    // Through the pane's middle, left of the seam: the red wall, much as without the pane.
    let (before, after) = (patch_rgb(&open, (250, 180), 6), patch_rgb(&glazed, (250, 180), 6));
    assert!(after[0] > after[1] + 60 && after[0] > after[2] + 60, "the red wall shows through: {after:?}");
    let apart = (0..3).map(|c| (after[c] - before[c]).abs()).max().unwrap_or(0);
    assert!(apart < 30, "and looks much as it does without the pane: {after:?} from {before:?}");
}

#[test]
fn tinted_glass_colours_what_lies_behind_it() {
    let green = MaterialDesc { base_color: [0.3, 1.0, 0.3, 1.0], ..clear_glass() };
    let (Some(clear), Some(tinted)) = (glass_render(Some(clear_glass())), glass_render(Some(green))) else { return };
    keep_image("glass green.png", &tinted);
    let (before, after) = (patch_rgb(&clear, (250, 180), 6), patch_rgb(&tinted, (250, 180), 6));
    assert!(after[0] * 10 < before[0] * 8, "green glass takes red out: {after:?} from {before:?}");
    let greener = |rgb: [i32; 3]| rgb[1] - rgb[0];
    assert!(greener(after) > greener(before) + 30, "and leaves the green: {after:?} from {before:?}");
}

#[test]
fn rough_glass_blurs_what_lies_behind_it() {
    let frosted = MaterialDesc { roughness: 0.6, ..clear_glass() };
    let (Some(clear), Some(rough)) = (glass_render(Some(clear_glass())), glass_render(Some(frosted))) else { return };
    keep_image("glass frosted.png", &rough);
    // How sharply red turns to blue across the seam, a few pixels either side of it.
    let sharpness = |pixels: &[u8]| {
        let (left, right) = (patch_rgb(pixels, (312, 180), 2), patch_rgb(pixels, (328, 180), 2));
        (left[0] - right[0]).abs() + (left[2] - right[2]).abs()
    };
    let (sharp, soft) = (sharpness(&clear), sharpness(&rough));
    assert!(soft * 2 < sharp, "frosted glass softens the seam: {soft} against {sharp}");
}

#[test]
fn glass_shows_through_glass() {
    let green = MaterialDesc { base_color: [0.3, 1.0, 0.3, 1.0], ..clear_glass() };
    let behind = glass_panes_render(&[(green, -0.8)]);
    let both = glass_panes_render(&[(clear_glass(), -1.5), (green, -0.8)]);
    let (Some(behind), Some(both)) = (behind, both) else { return };
    keep_image("glass behind glass.png", &both);
    let (alone, through) = (patch_rgb(&behind, (250, 180), 6), patch_rgb(&both, (250, 180), 6));
    let greener = |rgb: [i32; 3]| rgb[1] - rgb[0];
    assert!(greener(alone) > -40, "the green pane alone tints the red wall: {alone:?}");
    let apart = (greener(through) - greener(alone)).abs();
    assert!(apart < 25, "and shows through the clear pane: {through:?} for {alone:?}");
}

/// A green glass ball behind a large frosted pane turned across the view,
/// the ball's middle nearer the camera than the pane's.
fn ball_behind_frosted_render() -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, 0.0))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let pale = MaterialDesc { base_color: [0.6, 0.6, 0.6, 1.0], ..MaterialDesc::default() };
    let pale = renderer.add_material(&pale).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let frosted = renderer.add_material(&MaterialDesc { roughness: 0.8, ..clear_glass() }).unwrap();
    let green = renderer.add_material(&MaterialDesc { base_color: [0.1, 1.0, 0.1, 1.0], ..clear_glass() }).unwrap();
    // From (-2, 1.5) to (10, 7.5) across the floor, its middle six metres off; the ball four and a half off, behind
    // it, as the pane passes two and a half off in front of the camera.
    let across = Vec3::new(12.0, 0.0, 6.0);
    let turn = Quat::from_rotation_arc(Vec3::X, across.normalize());
    let pane = Transform::new(Vec3::new(4.0, 1.5, 4.5), turn, Vec3::new(across.length(), 3.0, 0.02));
    world.spawn((MeshRenderer::new(cube, frosted), pane));
    world.spawn((MeshRenderer::new(sphere, green), Transform::new(BALL_BEHIND, Quat::IDENTITY, Vec3::splat(0.6))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

const BALL_BEHIND: Vec3 = Vec3::new(0.0, 1.5, 4.5);

#[test]
fn glass_behind_frosted_glass_shows_blurred_through_it() {
    let Some(pixels) = ball_behind_frosted_render() else { return };
    keep_image("ball behind frosted glass.png", &pixels);
    // The ball is straight ahead, in the middle of the picture; how sharply its green ends a little out from it.
    let (inside, outside) = (patch_rgb(&pixels, (320, 180), 3), patch_rgb(&pixels, (320 + 40, 180), 3));
    let greener = |rgb: [i32; 3]| rgb[1] - (rgb[0] + rgb[2]) / 2;
    let edge = greener(inside) - greener(outside);
    assert!(edge < 25, "seen through the frosted pane, not drawn over it: {inside:?} against {outside:?}");
}

/// A dim room seen through a frosted pane, with a ball of light behind it
/// `size` pixels across, as small as a highlight on a shiny ball or as big as
/// a lamp, `shift` pixels right of the picture's middle, or none.
fn speck_behind_frosted_render(shift: Option<f32>, size: f32) -> Option<Vec<u8>> {
    use voxear_render::{Exposure, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, -3.0))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.6, 0.6, 0.6, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let wall = Transform::new(Vec3::new(0.0, 2.0, 0.5), Quat::IDENTITY, Vec3::new(8.0, 4.0, 0.2));
    world.spawn((MeshRenderer::new(cube, pale), wall));
    let frosted = renderer.add_material(&MaterialDesc { roughness: 0.45, ..clear_glass() }).unwrap();
    let pane = Transform::new(Vec3::new(0.0, 1.5, -1.5), Quat::IDENTITY, Vec3::new(2.0, 1.5, 0.02));
    world.spawn((MeshRenderer::new(cube, frosted), pane));
    if let Some(shift) = shift {
        let bright = MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [40.0; 3], ..MaterialDesc::default() };
        let bright = renderer.add_material(&bright).unwrap();
        // A pixel three metres off is about a centimetre across, as the camera sees 60 degrees over 360 pixels.
        let pixel = 2.0 * 3.0 * 30f32.to_radians().tan() / 360.0;
        let at = Vec3::new(shift * pixel, 1.5, 0.0);
        world.spawn((MeshRenderer::new(sphere, bright), Transform::new(at, Quat::IDENTITY, Vec3::splat(size * pixel))));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 0.1), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.02, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    // Exposed for a dark room, as candlelight is, so what is dim in the picture's own numbers shows.
    post.exposure = Exposure { compensation: 5.0, ..post.exposure };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_highlight_behind_frosted_glass_does_not_flash_as_it_moves() {
    let Some(dark) = speck_behind_frosted_render(None, 0.0) else { return };
    let brightness = |pixels: &[u8]| patch_rgb(pixels, (320, 180), 12).iter().sum::<i32>();
    let shifts = [0.0, 0.25, 0.5, 0.75, 1.0];
    let glows: Vec<i32> = shifts
        .iter()
        .filter_map(|&shift| speck_behind_frosted_render(Some(shift), 1.5))
        .enumerate()
        .map(|(n, pixels)| {
            keep_image(&format!("speck behind frosted glass {n}.png"), &pixels);
            brightness(&pixels) - brightness(&dark)
        })
        .collect();
    let (least, most) = (*glows.iter().min().unwrap(), *glows.iter().max().unwrap());
    assert!(most - least <= 6, "the speck's glow holds steady as it moves by quarter pixels: {glows:?}");
    // A lamp a few dozen pixels across still glows through the pane.
    let Some(lamp) = speck_behind_frosted_render(Some(0.0), 30.0) else { return };
    keep_image("lamp behind frosted glass.png", &lamp);
    let glow = brightness(&lamp) - brightness(&dark);
    assert!(glow > 150, "a lamp behind the pane glows through it: {glow}");
}

/// A project's custom shaders as the engine writes them (`voxear_engine::
/// shaders::dispatch_code`), by hand: 1 sets the colour, 2 is unlit, 3
/// moves vertices up a metre, 4 lights in two flat bands.
const CUSTOM_SHADERS: &str = r#"
#pragma once
namespace shader_1 {
struct Paint { float3 color; };
void surface(Paint paint, SurfaceInput input, inout SurfaceOutput surface) {
    surface.color = paint.color;
    surface.roughness = 1.0;
}
}
namespace shader_2 {
[Unlit]
struct Glow { float3 color; };
void surface(Glow glow, SurfaceInput input, inout SurfaceOutput surface) { surface.color = glow.color; }
}
namespace shader_3 {
struct Lift { float height; };
float3 vertex(Lift lift, VertexInput input) { return float3(0.0, lift.height, 0.0); }
}
namespace shader_4 {
struct Toon { float3 color; };
float3 light(Toon toon, LightInput light, SurfaceOutput surface) {
    return (dot(surface.normal, light.direction) > 0.3 ? 1.0 : 0.2) * toon.color * light.color;
}
}
float4* customValues(uint material) { return buffers()->materialProperties + material * MATERIAL_PROPERTIES; }
void customSurface(uint material, SurfaceInput input, inout SurfaceOutput surface) {
    switch (buffers()->materials[material].shader) {
    case 1: { shader_1::Paint p; p.color = customValues(material)[0].xyz; shader_1::surface(p, input, surface); break; }
    case 2: { shader_2::Glow p; p.color = customValues(material)[0].xyz; shader_2::surface(p, input, surface); break; }
    default: break;
    }
}
float3 customVertex(uint material, VertexInput input) {
    switch (buffers()->materials[material].shader) {
    case 3: { shader_3::Lift p; p.height = customValues(material)[0].x; return shader_3::vertex(p, input); }
    default: return 0.0;
    }
}
bool customUnlit(uint material) { return buffers()->materials[material].shader == 2; }
bool customLighting(uint material) { return buffers()->materials[material].shader == 4; }
float3 customLight(uint material, LightInput light, SurfaceOutput surface) {
    shader_4::Toon p; p.color = customValues(material)[0].xyz;
    return shader_4::light(p, light, surface);
}
"#;

/// A white ball on a grey floor under the sun, seen from in front, drawn
/// with custom shader `shader` and its first property `value`, or the
/// standard shader for 0.
/// When a test's custom shaders compile: while it draws, as the editor
/// compiles them, or before, as a game build does.
enum Compiled {
    WhileDrawing,
    Before,
}

fn custom_shader_render(shader: u32, value: [f32; 4], compiled: Compiled) -> Option<Vec<u8>> {
    use voxear_render::{CustomShaderState, Sky};
    let mut renderer = renderer()?;
    match compiled {
        Compiled::WhileDrawing => {
            renderer.set_custom_shaders(CUSTOM_SHADERS.to_string());
        }
        Compiled::Before => {
            let dir = std::env::temp_dir().join(format!("voxear-built-shaders-{}", std::process::id()));
            let modules = voxear_render::compile_custom_shaders(CUSTOM_SHADERS, &dir).unwrap();
            renderer.use_custom_modules(&modules).unwrap();
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -4.0))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let grey = MaterialDesc { base_color: [0.3, 0.3, 0.3, 1.0], ..MaterialDesc::default() };
    let grey = renderer.add_material(&grey).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let mut ball = MaterialDesc { base_color: [1.0; 4], shader, ..MaterialDesc::default() };
    ball.properties[0] = value;
    let ball = renderer.add_material(&ball).unwrap();
    world.spawn((MeshRenderer::new(sphere, ball), Transform::new(Vec3::new(0.0, 1.0, 0.0), Quat::IDENTITY, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.5, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    // Compiling takes seconds, on its own thread, drawing with the shaders as they were meanwhile.
    for _ in 0..600 {
        run(&mut renderer, &world, &mut 0.0, 1);
        match renderer.custom_shaders() {
            CustomShaderState::Compiling => std::thread::sleep(std::time::Duration::from_millis(50)),
            CustomShaderState::Failed(error) => panic!("the custom shaders did not compile: {error}"),
            _ => break,
        }
    }
    assert_eq!(*renderer.custom_shaders(), CustomShaderState::Ready);
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn custom_shaders_set_the_surface_light_it_their_way_go_unlit_and_move_vertices() {
    let Some(standard) = custom_shader_render(0, [0.0; 4], Compiled::WhileDrawing) else { return };
    keep_image("custom standard.png", &standard);
    // The ball's middle, a little up and left, lit by the sun.
    let at = (300, 150);
    let white = patch_rgb(&standard, at, 3);
    assert!(white[0] > 120 && (white[0] - white[2]).abs() < 30, "the standard white ball: {white:?}");

    let painted = custom_shader_render(1, [0.05, 0.1, 1.0, 0.0], Compiled::WhileDrawing).unwrap();
    keep_image("custom surface.png", &painted);
    let blue = patch_rgb(&painted, at, 3);
    assert!(blue[2] > blue[0] + 60, "a surface function paints it blue: {blue:?}");
    let built = custom_shader_render(1, [0.05, 0.1, 1.0, 0.0], Compiled::Before).unwrap();
    let built = patch_rgb(&built, at, 3);
    let alike = built.iter().zip(blue).all(|(a, b)| (a - b).abs() < 6);
    assert!(alike, "compiled before, as a game is: {built:?} where compiled while drawing gave {blue:?}");

    let unlit = custom_shader_render(2, [0.0, 1.0, 0.0, 0.0], Compiled::WhileDrawing).unwrap();
    keep_image("custom unlit.png", &unlit);
    let (lit_side, dark_side) = (patch_rgb(&unlit, at, 3), patch_rgb(&unlit, (340, 205), 3));
    assert!(lit_side[1] > lit_side[0] + 100, "unlit green: {lit_side:?}");
    assert!((lit_side[1] - dark_side[1]).abs() < 12, "the same all over, lit or not: {lit_side:?} {dark_side:?}");

    let toon = custom_shader_render(4, [1.0, 0.4, 0.1, 0.0], Compiled::WhileDrawing).unwrap();
    keep_image("custom light.png", &toon);
    let band = patch_rgb(&toon, at, 3);
    assert!(band[0] > band[2] + 60, "lit by its own light function, in its colour: {band:?}");

    let lifted = custom_shader_render(3, [1.0, 0.0, 0.0, 0.0], Compiled::WhileDrawing).unwrap();
    keep_image("custom vertex.png", &lifted);
    // A metre above the ball: sky, a little blue, without the lift, and the ball, grey, with it.
    let above = (320, 105);
    let blueness = |rgb: [i32; 3]| rgb[2] - rgb[0];
    let (sky, ball) = (patch_rgb(&standard, above, 3), patch_rgb(&lifted, above, 3));
    assert!(blueness(ball) + 15 < blueness(sky), "a vertex function lifts it: {ball:?} where the sky was {sky:?}");
}

/// A floor in the sun, seen from above and in front, with a pane of glass
/// standing two metres wide across the sun's light, or none: the floor
/// where the light comes through the pane is in view, the pane itself is
/// not in front of it.
fn glass_light_render(pane: Option<MaterialDesc>) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -5.0, -5.5).normalize());
    world.spawn((Camera::default(), Transform::new(GLASS_LIGHT_CAMERA, look, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let pale = MaterialDesc { base_color: [0.7, 0.7, 0.7, 1.0], ..MaterialDesc::default() };
    let pale = renderer.add_material(&pale).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    if let Some(desc) = pane {
        let glass = renderer.add_material(&desc).unwrap();
        let at = Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, Vec3::new(2.0, 2.0, 0.02));
        world.spawn((MeshRenderer::new(cube, glass), at));
    }
    // Down through the pane towards +Z, onto the floor from a little under a metre to a little over four behind it.
    let sun = Quat::from_rotation_arc(Vec3::Z, GLASS_SUN.normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 8);
    Some(renderer.read_pixels().unwrap().2)
}

const GLASS_LIGHT_CAMERA: Vec3 = Vec3::new(0.0, 5.0, 7.5);
const GLASS_SUN: Vec3 = Vec3::new(0.0, -0.6, 1.0);

/// The colour of the floor at `point` in the glass light picture.
fn glass_floor_rgb(pixels: &[u8], point: Vec3) -> [i32; 3] {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -5.0, -5.5).normalize());
    let view = Mat4::from_rotation_translation(look, GLASS_LIGHT_CAMERA).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * point.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "{point} is in view");
    patch_rgb(pixels, ((x * 640.0) as u32, (y * 360.0) as u32), 3)
}

/// Where the sun's light through the pane at `height` up its middle, `x`
/// across, meets the floor.
fn through_pane(x: f32, height: f32) -> Vec3 {
    let sun = GLASS_SUN.normalize();
    Vec3::new(x, 1.5 + height, 0.0) + sun * ((1.5 + height) / -sun.y)
}

#[test]
fn light_through_glass_takes_its_colour() {
    let green = MaterialDesc { base_color: [0.3, 1.0, 0.3, 1.0], ..clear_glass() };
    let renders = (glass_light_render(None), glass_light_render(Some(clear_glass())), glass_light_render(Some(green)));
    let (Some(open), Some(clear), Some(tinted)) = renders else { return };
    keep_image("glass light none.png", &open);
    keep_image("glass light clear.png", &clear);
    keep_image("glass light green.png", &tinted);
    let behind = through_pane(0.0, 0.0);
    let [lit, through, green] = [&open, &clear, &tinted].map(|pixels| glass_floor_rgb(pixels, behind));
    assert!(lit[0] > 100, "the floor is in the sun: {lit:?}");
    assert!(through[0] * 10 > lit[0] * 8, "clear glass lets most of the sun through: {through:?} of {lit:?}");
    assert!(green[0] * 10 < lit[0] * 6, "green glass takes red out of it: {green:?} of {lit:?}");
    assert!(green[1] > green[0] + 40, "and leaves the floor green: {green:?}");
}

#[test]
fn frosted_glass_spreads_the_light_it_lets_through() {
    let blue = MaterialDesc { base_color: [0.15, 0.3, 1.0, 1.0], ..clear_glass() };
    let frosted = MaterialDesc { roughness: 0.9, ..blue };
    let (Some(sharp), Some(soft)) = (glass_light_render(Some(blue)), glass_light_render(Some(frosted))) else { return };
    keep_image("glass light blue.png", &sharp);
    keep_image("glass light frosted.png", &soft);
    // Just past the pane's side, far behind it, where its sharp light ends.
    let past = through_pane(1.25, 0.5);
    let blueness = |rgb: [i32; 3]| rgb[2] - rgb[0];
    let (outside, spread) = (glass_floor_rgb(&sharp, past), glass_floor_rgb(&soft, past));
    assert!(blueness(outside) < 10, "clear glass's light ends at its outline: {outside:?}");
    assert!(blueness(spread) > blueness(outside) + 10, "frosted glass's spreads past it: {spread:?} from {outside:?}");
}

/// The camera looking along a low sun at a small ball high above a floor,
/// at `height`, whose shadow falls far ahead of it, cascades away.
const BALL_CAMERA: Vec3 = Vec3::new(0.0, 3.0, -2.0);
const BALL_LOOK: Vec3 = Vec3::new(0.0, -0.3, 1.0);
const BALL_SUN: Vec3 = Vec3::new(0.0, -0.5, 1.0);

fn high_ball_render(ball: bool) -> Option<Vec<u8>> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, BALL_LOOK.normalize());
    world.spawn((Camera::default(), Transform::new(BALL_CAMERA, look, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let pale = MaterialDesc { base_color: [0.7, 0.7, 0.7, 1.0], ..MaterialDesc::default() };
    let pale = renderer.add_material(&pale).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(80.0))));
    if ball {
        let leaves = MaterialDesc { base_color: [0.2, 0.4, 0.1, 1.0], foliage: true, ..MaterialDesc::default() };
        let leaves = renderer.add_material(&leaves).unwrap();
        // A tree's ball of leaves: under a metre across, four up.
        let high = Transform::new(Vec3::new(0.0, 4.0, 0.0), Quat::IDENTITY, Vec3::splat(1.3));
        world.spawn((MeshRenderer::new(sphere, leaves), high));
    }
    let sun = Quat::from_rotation_arc(Vec3::Z, BALL_SUN.normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let mut post = steady_post();
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_small_caster_high_up_shadows_the_floor_far_from_it() {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let (Some(open), Some(shaded)) = (high_ball_render(false), high_ball_render(true)) else { return };
    keep_image("high ball.png", &shaded);
    // Where the sun through the ball's middle meets the floor, eight metres on.
    let sun = BALL_SUN.normalize();
    let shadow = Vec3::new(0.0, 4.0, 0.0) + sun * (4.0 / -sun.y);
    let look = Quat::from_rotation_arc(Vec3::Z, BALL_LOOK.normalize());
    let view = Mat4::from_rotation_translation(look, BALL_CAMERA).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * shadow.extend(1.0);
    let (x, y) = (clip.x / clip.w * 0.5 + 0.5, clip.y / clip.w * 0.5 + 0.5);
    assert!((0.0..1.0).contains(&x) && (0.0..1.0).contains(&y), "the shadow is in view");
    let at = ((x * 640.0) as u32, (y * 360.0) as u32);
    let (lit, dark) = (patch_rgb(&open, at, 3), patch_rgb(&shaded, at, 3));
    assert!(dark[0] * 2 < lit[0], "the ball's shadow is there, cascades from the ball: {dark:?} against {lit:?}");
}

/// A dark pole standing against the sky in front of the camera, drawn with
/// a material that bends in the wind, in a Wind Zone of `strength` blowing
/// to the camera's right, or none.
fn wind_render(strength: Option<f32>) -> Option<Vec<u8>> {
    use voxear_render::{Sky, WindZone};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 3.0, -9.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plant = MaterialDesc { base_color: [0.02, 0.02, 0.02, 1.0], wind_bend: 2.0, ..MaterialDesc::default() };
    let dark = renderer.add_material(&plant).unwrap();
    world.spawn((MeshRenderer::new(cube, dark), Transform::new(Vec3::new(0.0, 3.0, 0.0), Quat::IDENTITY, Vec3::new(0.3, 6.0, 0.3))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.2, -0.7, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if let Some(strength) = strength {
        // Steady, so the pole leans as far every frame.
        let wind = WindZone { strength, gusts: 0.0, turbulence: 0.0, ..WindZone::default() };
        world.spawn((wind, Transform::new(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), Vec3::ONE)));
    }
    world.insert_resource(Sky::default());
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

/// Across row `y`, the middle of the pole: where the darkest pixels are.
fn pole_centre(pixels: &[u8], y: u32) -> f32 {
    let dark: Vec<u32> = (0..640).filter(|&x| rgb_at(pixels, (x, y)).iter().sum::<i32>() < 350).collect();
    assert!(!dark.is_empty(), "the pole crosses row {y}");
    dark.iter().sum::<u32>() as f32 / dark.len() as f32
}

#[test]
fn plants_bend_downwind_from_the_ground_up() {
    let (Some(still), Some(windy)) = (wind_render(None), wind_render(Some(3.0))) else { return };
    keep_image("wind still.png", &still);
    keep_image("wind blowing.png", &windy);
    let (top, bottom) = (85, 260);
    let lean = |pixels: &[u8]| pole_centre(pixels, top) - pole_centre(pixels, bottom);
    assert!(lean(&still).abs() < 2.0, "without wind the pole stands straight: {}", lean(&still));
    assert!(lean(&windy) > 8.0, "in the wind its top leans downwind: {}", lean(&windy));
    // A cube has vertices only at its ends, so the foot is the row just above its base.
    assert!((pole_centre(&windy, 282) - pole_centre(&still, 282)).abs() < 2.0, "and its foot stays put");
}

#[test]
fn foliage_bends_away_from_what_passes_through_it_and_springs_back() {
    use voxear_render::{FoliageInteraction, Sky};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 3.0, -9.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let plant = MaterialDesc { base_color: [0.02, 0.02, 0.02, 1.0], wind_bend: 1.0, ..MaterialDesc::default() };
    let dark = renderer.add_material(&plant).unwrap();
    let pole = Transform::new(Vec3::new(0.0, 3.0, 0.0), Quat::IDENTITY, Vec3::new(0.3, 6.0, 0.3));
    world.spawn((MeshRenderer::new(cube, dark), pole));
    world.insert_resource(Sky::default());
    world.insert_resource(steady_post());
    // Something standing just left of the pole, reaching over it, with no wind at all.
    let pusher = world.spawn((FoliageInteraction { radius: 1.5, strength: 1.0 }, Transform::from_position(Vec3::new(-0.6, 0.0, 0.0))));
    let mut time = 0.0;
    run(&mut renderer, &world, &mut time, 3);
    let pushed = renderer.read_pixels().unwrap().2;
    keep_image("foliage pushed.png", &pushed);
    let lean = |pixels: &[u8]| pole_centre(pixels, 85) - pole_centre(pixels, 260);
    assert!(lean(&pushed) > 8.0, "the pole leans away from what stands beside it: {}", lean(&pushed));

    world.despawn(pusher);
    run(&mut renderer, &world, &mut time, 300);
    let after = renderer.read_pixels().unwrap().2;
    assert!(lean(&after).abs() < 2.0, "and springs back once it has gone: {}", lean(&after));
}

#[test]
fn a_pulsing_glow_rises_and_falls_and_a_steady_one_stays() {
    use voxear_render::{Glow, Sky};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -5.0))));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let glowing = |glow| MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [0.0, 2.0, 1.0], glow, glow_speed: 0.5, ..MaterialDesc::default() };
    let pulse = renderer.add_material(&glowing(Glow::Pulse)).unwrap();
    let steady = renderer.add_material(&glowing(Glow::Steady)).unwrap();
    world.spawn((MeshRenderer::new(sphere, pulse), Transform::from_position(Vec3::new(-1.0, 0.0, 0.0))));
    world.spawn((MeshRenderer::new(sphere, steady), Transform::from_position(Vec3::new(1.0, 0.0, 0.0))));
    world.insert_resource(Sky { ambient: 0.0, ..Sky::default() });
    world.insert_resource(steady_post());
    let mut time = 0.0;
    let mut seen = Vec::new();
    // Over two seconds, one turn of the pulse, every quarter of a second.
    for _ in 0..8 {
        run(&mut renderer, &world, &mut time, 15);
        let pixels = renderer.read_pixels().unwrap().2;
        seen.push((rgb_at(&pixels, (240, 180))[1], rgb_at(&pixels, (400, 180))[1]));
    }
    let (pulsing, steady): (Vec<i32>, Vec<i32>) = seen.into_iter().unzip();
    let spread = |values: &[i32]| values.iter().max().unwrap() - values.iter().min().unwrap();
    assert!(spread(&pulsing) > 60, "the pulse rises and falls: {pulsing:?}");
    assert!(spread(&steady) < 8, "the steady glow stays: {steady:?}");
}

#[test]
fn one_obj_draws_a_whole_model_each_part_where_the_model_puts_it() {
    use voxear_render::{ModelPart, ModelRenderer, Sky};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 0.0, -6.0))));
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    let green = renderer.add_material(&MaterialDesc { base_color: [0.05, 0.9, 0.05, 1.0], ..MaterialDesc::default() }).unwrap();
    let part = |material, x: f32| ModelPart {
        mesh: cube,
        material,
        transform: voxear_core::math::Mat4::from_translation(Vec3::new(x, 0.0, 0.0)),
        node: 0,
        skin: None,
    };
    // Half again as large, so its parts sit a half again as far apart.
    let model = world.spawn((ModelRenderer::new(vec![part(red, -1.0), part(green, 1.0)]), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(1.5))));
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, Quat::from_rotation_x(0.5), Vec3::ONE)));
    world.insert_resource(Sky { ambient: 1.0, ..Sky::default() });
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut 0.0, 3);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("model renderer.png", &pixels);
    let (left, right) = (rgb_at(&pixels, (210, 180)), rgb_at(&pixels, (430, 180)));
    assert!(left[0] > left[1] * 2, "the red part on the left: {left:?}");
    assert!(right[1] > right[0] + 60, "the green part on the right: {right:?}");
    let [r, g, _] = rgb_at(&pixels, (320, 180));
    assert!((r - g).abs() < 40, "the sky in the gap between them, neither part: {:?}", rgb_at(&pixels, (320, 180)));
    let stats = renderer.stats();
    assert_eq!(stats.instances, 2, "each part an instance");
    world.despawn(model);
    run(&mut renderer, &world, &mut 0.0, 1);
    assert_eq!(renderer.stats().instances, 0);
}

#[test]
fn a_mesh_without_normals_does_not_black_out_the_picture_through_bloom() {
    use voxear_render::Sky;
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -4.0))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(20.0))));
    // A cube whose normals and tangents have been lost, as some models' seams have them.
    let mut broken = voxear_render::MeshData::cube();
    for vertex in &mut broken.vertices {
        vertex.normal = [0.0; 3];
        vertex.tangent = [0.0; 4];
    }
    let mesh = renderer.add_mesh(&broken).unwrap();
    world.spawn((MeshRenderer::new(mesh, pale), Transform::from_position(Vec3::new(0.0, 1.0, 0.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    let mut post = steady_post();
    post.bloom.enabled = true;
    post.bloom.intensity = 0.3;
    post.bloom.scatter = 0.95;
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 0.0, 3);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("no normals.png", &pixels);
    // NaN from the cube would spread over every pixel through bloom; the floor round it stays lit.
    let floor = rgb_at(&pixels, (100, 300)).iter().sum::<i32>();
    assert!(floor > 150, "the floor is lit, not blacked out: {floor}");
    assert!(rgb_at(&pixels, (320, 160)).iter().sum::<i32>() > 60, "and the cube is drawn: {:?}", rgb_at(&pixels, (320, 160)));
}

/// A green leafy ball over pale ground, the sun behind it throwing its
/// shadow towards the camera, at `time` of the year in Seasons that turn
/// every plant together, or with no Seasons.
fn seasons_render(time: Option<f32>, leaves: voxear_render::Leaves) -> Option<Vec<u8>> {
    use voxear_render::{Seasons, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let looking = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.3, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 2.0, -5.0), looking, Vec3::ONE)));
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.8, 0.8, 0.8, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    let leaf = MaterialDesc { base_color: [0.15, 0.5, 0.08, 1.0], foliage: true, leaves, ..MaterialDesc::default() };
    let material = renderer.add_material(&leaf).unwrap();
    world.spawn((MeshRenderer::new(sphere, material), Transform::new(Vec3::new(0.0, 1.5, 0.0), Quat::IDENTITY, Vec3::splat(2.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.6, -0.8).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if let Some(time) = time {
        world.spawn((Seasons { time_of_year: time, variation: 0.0, ..Seasons::default() }, Transform::default()));
    }
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn leaves_turn_in_autumn_and_take_snow_in_winter_unless_they_stay() {
    use voxear_render::{Leaves, Season};
    let render = |season: Option<Season>, leaves| seasons_render(season.map(Season::time), leaves);
    let Some(none) = render(None, Leaves::Turn) else { return };
    let (summer, autumn) = (render(Some(Season::Summer), Leaves::Turn).unwrap(), render(Some(Season::Autumn), Leaves::Turn).unwrap());
    let (winter, pine) = (render(Some(Season::Winter), Leaves::Turn).unwrap(), render(Some(Season::Winter), Leaves::Stay).unwrap());
    keep_image("seasons summer.png", &summer);
    keep_image("seasons autumn.png", &autumn);
    keep_image("seasons winter.png", &winter);
    keep_image("seasons winter pine.png", &pine);
    let (ball, top, shadow) = ((320, 150), (320, 62), (320, 300));
    let sum = |pixel: [i32; 3]| pixel.iter().sum::<i32>();
    let difference = |a: [i32; 3], b: [i32; 3]| a.iter().zip(b).map(|(x, y)| (x - y).abs()).max().unwrap_or(0);
    assert!(difference(rgb_at(&summer, ball), rgb_at(&none, ball)) <= 2, "summer leaves are as made");
    let [r, g, _] = rgb_at(&summer, ball);
    assert!(g > r, "green in summer: {:?}", rgb_at(&summer, ball));
    let [r, g, _] = rgb_at(&autumn, ball);
    assert!(r > g, "and orange in autumn: {:?}", rgb_at(&autumn, ball));
    // Winter: snow on top, and the leaves all still there, shadow and all.
    assert!(sum(rgb_at(&winter, top)) > sum(rgb_at(&summer, top)) + 60, "snow on top in winter: {:?}", rgb_at(&winter, top));
    assert!((sum(rgb_at(&winter, shadow)) - sum(rgb_at(&summer, shadow))).abs() < 30, "the leaves stay on, shadow and all");
    // A pine's needles stay green under their snow.
    let [r, g, _] = rgb_at(&pine, ball);
    assert!(g > r, "a pine stays green: {:?}", rgb_at(&pine, ball));
}

/// A field of small dark plants scattered over pale ground ahead of the
/// camera, 80 metres deep, as one Foliage, drawn out to `draw` metres.
fn foliage_render(draw: f32) -> Option<(Vec<u8>, RenderStats)> {
    foliage_tinted(draw, [0.02, 0.03, 0.02], ([1.0; 3], [1.0; 3]))
}

/// The same field, its plants `color` and tinted between two colours.
fn foliage_tinted(draw: f32, color: [f32; 3], (healthy, dry): ([f32; 3], [f32; 3])) -> Option<(Vec<u8>, RenderStats)> {
    use voxear_render::{Foliage, Sky, scatter};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let looking = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.25, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 2.0, -2.0), looking, Vec3::ONE)));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let pale = renderer.add_material(&MaterialDesc { base_color: [0.9, 0.9, 0.85, 1.0], ..MaterialDesc::default() }).unwrap();
    let [r, g, b] = color;
    let dark = renderer.add_material(&MaterialDesc { base_color: [r, g, b, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, pale), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(400.0))));
    let plants = scatter([40.0, 80.0], 4.0, 3, [0.15, 0.25]);
    assert!(plants.len() > 12_000, "a field of them: {}", plants.len());
    let foliage = Foliage {
        mesh: cube,
        material: dark,
        plants: std::sync::Arc::new(plants),
        draw_distance: draw,
        fade_distance: 3.0,
        shadow_distance: 15.0,
        healthy_color: healthy,
        dry_color: dry,
        color_spread: 2.0,
        impostors: voxear_render::Impostors::Auto,
        impostor_distance: 60.0,
    };
    world.spawn((foliage, Transform::from_position(Vec3::new(0.0, 0.1, 40.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.4).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    world.insert_resource(steady_post());
    let stats = run(&mut renderer, &world, &mut 0.0, 3);
    Some((renderer.read_pixels().unwrap().2, stats))
}

/// How many of a band of rows' pixels are dark: plants, against the pale ground.
fn dark_share(pixels: &[u8], rows: std::ops::Range<u32>) -> f32 {
    let points: Vec<(u32, u32)> = rows.flat_map(|y| (0..640).map(move |x| (x, y))).collect();
    let dark = points.iter().filter(|&&at| rgb_at(pixels, at).iter().sum::<i32>() < 150).count();
    dark as f32 / points.len() as f32
}

#[test]
fn plants_are_tinted_between_their_healthy_and_dry_colours_in_patches() {
    // White plants, tinted between red and blue in patches two metres across.
    let Some((pixels, _)) = foliage_tinted(25.0, [0.9; 3], ([1.0, 0.1, 0.1], [0.1, 0.1, 1.0])) else { return };
    keep_image("foliage tinted.png", &pixels);
    let near: Vec<[i32; 3]> = (300..360).flat_map(|y| (0..640).step_by(4).map(move |x| (x, y))).map(|at| rgb_at(&pixels, at)).collect();
    let reddish = near.iter().filter(|[r, _, b]| *r > *b + 40).count();
    let bluish = near.iter().filter(|[r, _, b]| *b > *r + 40).count();
    assert!(reddish > 50 && bluish > 50, "patches of both: {reddish} reddish, {bluish} bluish");
}

#[test]
fn foliage_draws_thousands_of_plants_out_to_its_draw_distance() {
    let Some((pixels, stats)) = foliage_render(25.0) else { return };
    keep_image("foliage field.png", &pixels);
    // Near the camera, the bottom of the picture; past 30 metres, the rows just under the horizon, which is at
    // row 96 with the camera two metres up looking 14 degrees down.
    let (near, far) = (dark_share(&pixels, 300..360), dark_share(&pixels, 100..118));
    assert!(near > 0.15, "plants cover the ground near the camera: {near}");
    assert!(far < 0.01, "and none show past their draw distance: {far}");
    let Some((all, _)) = foliage_render(200.0) else { return };
    assert!(dark_share(&all, 100..118) > 0.05, "where a longer draw distance shows them");
    let drawn = stats.drawn_early + stats.drawn_late;
    assert!(stats.instances > 1000 && drawn > 1000, "drawn as instances: {} of {}", drawn, stats.instances);
    assert!(stats.instances < 8000, "only those within their draw distance: {}", stats.instances);
}

/// A spire leaning to one side, so a picture of it upside down or turned
/// the wrong way shows: a square base two metres across, its tip a metre
/// over to +X and three up.
fn leaning_spire() -> voxear_render::MeshData {
    let mut mesh = voxear_render::MeshData::default();
    let tip = Vec3::new(1.0, 3.0, 0.0);
    let base = [Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, -1.0), Vec3::new(1.0, 0.0, 1.0), Vec3::new(-1.0, 0.0, 1.0)];
    for side in 0..4 {
        let (a, b) = (base[side], base[(side + 1) % 4]);
        let normal = (tip - a).cross(b - a).normalize();
        let at = mesh.vertices.len() as u32;
        for corner in [a, b, tip] {
            mesh.vertices.push(voxear_render::Vertex {
                position: corner.to_array(),
                normal: normal.to_array(),
                uv: [0.5, 0.5],
                tangent: [1.0, 0.0, 0.0, 1.0],
                ..voxear_render::Vertex::default()
            });
        }
        mesh.indices.extend([at, at + 2, at + 1]);
    }
    mesh
}

/// One spire, red, as a Foliage's single plant turned a quarter round, 18
/// metres in front of the camera, drawn as itself or as its impostor.
fn spire_render(impostors: voxear_render::Impostors) -> Option<Vec<u8>> {
    use voxear_render::{Foliage, Plant, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, -18.0))));
    let spire = renderer.add_mesh(&leaning_spire()).unwrap();
    let red = renderer.add_material(&MaterialDesc { base_color: [0.8, 0.05, 0.03, 1.0], ..MaterialDesc::default() }).unwrap();
    let plants = vec![Plant { position: [0.0, 0.0, 0.0], yaw: 90.0, scale: 1.0 }];
    let foliage = Foliage {
        mesh: spire,
        material: red,
        plants: std::sync::Arc::new(plants),
        draw_distance: 100.0,
        fade_distance: 2.0,
        shadow_distance: 0.0,
        healthy_color: [1.0; 3],
        dry_color: [1.0; 3],
        color_spread: 8.0,
        impostors,
        impostor_distance: 5.0,
    };
    // The Obj turned too, so the impostor follows the plant's turn and its Obj's together.
    world.spawn((foliage, Transform::new(Vec3::ZERO, Quat::from_rotation_y(0.6), Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.4).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky::default());
    world.insert_resource(steady_post());
    // The first frame asks for the impostor, the next bakes it, and those after draw it.
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn far_plants_are_drawn_as_impostors_that_look_like_them() {
    use voxear_render::Impostors;
    let Some(mesh) = spire_render(Impostors::Off) else { return };
    let impostor = spire_render(Impostors::From).unwrap();
    keep_image("impostor mesh.png", &mesh);
    keep_image("impostor picture.png", &impostor);
    let red = |pixels: &[u8]| -> Vec<bool> {
        pixels.chunks(4).map(|p| i32::from(p[0]) > i32::from(p[1]) + 40 && i32::from(p[0]) > i32::from(p[2]) + 40).collect()
    };
    let (seen, pictured) = (red(&mesh), red(&impostor));
    let both = seen.iter().zip(&pictured).filter(|(a, b)| **a && **b).count();
    let either = seen.iter().zip(&pictured).filter(|(a, b)| **a || **b).count();
    let covered = seen.iter().filter(|a| **a).count();
    assert!(covered > 500, "the spire is in view: {covered} pixels");
    assert_ne!(mesh, impostor, "the impostor is drawn, not the mesh");
    // The same shape, the right way up and round, give or take the picture's angle and its pixels.
    let overlap = both as f32 / either.max(1) as f32;
    assert!(overlap > 0.8, "the impostor covers what the spire does: {overlap:.2} of {covered} pixels");
    // And lit much as it is: the average red within it.
    let average = |pixels: &[u8], mask: &[bool]| {
        let (sum, n) = pixels.chunks(4).zip(mask).filter(|(_, m)| **m).fold((0u32, 0u32), |(s, n), (p, _)| (s + u32::from(p[0]), n + 1));
        sum as f32 / n.max(1) as f32
    };
    let (lit, pictured_lit) = (average(&mesh, &seen), average(&impostor, &pictured));
    assert!((lit - pictured_lit).abs() < 30.0, "lit alike: {lit:.0} and {pictured_lit:.0}");
}

/// The pole of `wind_render` in a steady wind, its top's vertices given wind
/// masks `top` and its foot's none.
fn masked_pole_render(top: [u8; 4]) -> Option<Vec<u8>> {
    use voxear_render::{MeshData, Sky, WindZone};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 3.0, -9.0))));
    let mut pole = MeshData::primitive(Primitive::Cube);
    for vertex in pole.vertices.iter_mut().filter(|vertex| vertex.position[1] > 0.0) {
        vertex.wind = top;
    }
    let pole = renderer.add_mesh(&pole).unwrap();
    let plant = MaterialDesc { base_color: [0.02, 0.02, 0.02, 1.0], wind_bend: 1.0, ..MaterialDesc::default() };
    let dark = renderer.add_material(&plant).unwrap();
    world.spawn((MeshRenderer::new(pole, dark), Transform::new(Vec3::new(0.0, 3.0, 0.0), Quat::IDENTITY, Vec3::new(0.3, 6.0, 0.3))));
    let wind = WindZone { strength: 2.0, gusts: 0.0, turbulence: 0.0, ..WindZone::default() };
    world.spawn((wind, Transform::new(Vec3::ZERO, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2), Vec3::ONE)));
    world.insert_resource(Sky::default());
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut 0.0, 3);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn wind_masks_sway_branches_further_than_the_trunk() {
    let Some(plain) = masked_pole_render(voxear_render::NO_WIND_MASKS) else { return };
    let branch = masked_pole_render([0, 0, 255, 0]).unwrap();
    keep_image("wind masks plain.png", &plain);
    keep_image("wind masks branch.png", &branch);
    let (top, bottom) = (85, 260);
    let lean = |pixels: &[u8]| pole_centre(pixels, top) - pole_centre(pixels, bottom);
    assert!(lean(&plain) > 4.0, "the pole leans in the wind: {}", lean(&plain));
    assert!(lean(&branch) > lean(&plain) + 4.0, "a branch sways further downwind: {} and {}", lean(&branch), lean(&plain));
    assert!((pole_centre(&branch, 282) - pole_centre(&plain, 282)).abs() < 2.0, "and the trunk's foot stays put");
}

/// The sea in `weather`, seen from four metres up, the sun low ahead.
fn ocean_render(weather: voxear_render::Weather) -> Option<Vec<u8>> {
    ocean_spray_render(weather, voxear_render::Ocean::default().spray)
}

/// The sea of ocean_render, with as much spray as `spray` says.
fn ocean_spray_render(weather: voxear_render::Weather, spray: f32) -> Option<Vec<u8>> {
    use voxear_render::{Ocean, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.08, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 4.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.35, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let turned = Transform::new(Vec3::ZERO, Quat::from_rotation_y(0.4), Vec3::ONE);
    world.spawn((Ocean { weather, spray, ..Ocean::default() }, turned));
    let mut time = 40.0;
    run(&mut renderer, &world, &mut time, 20);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn the_sea_is_blue_water_and_a_storm_breaks_into_whitecaps() {
    use voxear_render::Weather;
    let Some(calm) = ocean_render(Weather::Calm) else { return };
    keep_image("ocean calm.png", &calm);
    let stormy = ocean_render(Weather::Stormy).unwrap();
    keep_image("ocean stormy.png", &stormy);
    // Below the horizon, away from the sun's path: water, its blue over its red.
    let water = patch_rgb(&calm, (560, 320), 6);
    assert!(water[2] > water[0] + 15, "the sea is blue: {water:?}");
    // Whitecaps: bright, nearly grey pixels below the horizon, out of the sun's glitter on the left.
    let foam = |pixels: &[u8]| {
        (220..360u32)
            .flat_map(|y| (400..640u32).map(move |x| (x, y)))
            .filter(|&at| {
                let [r, g, b] = rgb_at(pixels, at);
                r > 95 && (b - r).abs() < 40 && (g - r).abs() < 40
            })
            .count()
    };
    let (calm_foam, storm_foam) = (foam(&calm), foam(&stormy));
    assert!(storm_foam > calm_foam * 2 + 50, "a storm's whitecaps: {storm_foam} pixels against {calm_foam} when calm");
}

#[test]
fn a_storm_throws_spray_off_its_breaking_crests() {
    use voxear_render::Weather;
    let Some(bare) = ocean_spray_render(Weather::Stormy, 0.0) else { return };
    let spray = ocean_spray_render(Weather::Stormy, 2.0).unwrap();
    keep_image("ocean spray.png", &spray);
    // Pale drops where the bare sea was darker, below the horizon.
    let paler = (200..360u32)
        .flat_map(|y| (0..640u32).map(move |x| (x, y)))
        .filter(|&at| rgb_at(&spray, at).iter().sum::<i32>() > rgb_at(&bare, at).iter().sum::<i32>() + 45)
        .count();
    assert!(paler > 25, "spray off the crests: {paler} pixels");
}

#[test]
fn a_calm_wind_zone_smooths_the_sea_inside_it() {
    use voxear_render::{Ocean, Sky, Weather, WindShape, WindZone};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // From high over the sea, looking down, the left half under a calm Box.
    let (eye, look) = (Vec3::new(0.0, 40.0, 0.0), Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -1.0, 0.001).normalize()));
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    world.spawn((Ocean { weather: Weather::Stormy, ..Ocean::default() }, Transform::default()));
    let calm = WindZone { shape: WindShape::Box, strength: 0.0, size: [60.0, 20.0, 120.0], falloff: 2.0, ..WindZone::default() };
    world.spawn((calm, Transform::from_position(Vec3::new(-32.0, 0.0, 0.0))));
    run(&mut renderer, &world, &mut 40.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("calm wind zone.png", &pixels);
    // How much the sea's brightness varies over a patch: its waves and their foam.
    let rough = |centre: Vec3| {
        let (x, y) = on_screen(eye, look, centre);
        let values: Vec<f32> = (0..24u32)
            .flat_map(|dy| (0..24u32).map(move |dx| (x - 12 + dx, y - 12 + dy)))
            .map(|at| rgb_at(&pixels, at).iter().sum::<i32>() as f32)
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
    };
    let (inside, outside) = (rough(Vec3::new(-12.0, 0.0, 0.0)), rough(Vec3::new(12.0, 0.0, 0.0)));
    assert!(inside < outside * 0.6, "the sea's roughness inside the calm zone {inside}, outside it {outside}");
}

/// A stream over sand, level, then down through rapids, then level again.
fn stream_render(time: f32) -> Option<(Vec<u8>, Vec3, Quat)> {
    use voxear_render::{Sky, Stream};
    use voxear_scene::{Path, PathPoint};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let eye = Vec3::new(7.0, 5.0, -3.0);
    let look = Quat::from_rotation_arc(Vec3::Z, (Vec3::new(0.0, -1.5, 12.0) - eye).normalize());
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.6, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -3.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    let point = |y: f32, z: f32| PathPoint { position: [0.0, y, z], width: 4.0 };
    let path = Path { points: vec![point(0.0, 0.0), point(0.0, 12.0), point(-1.5, 17.0), point(-1.5, 30.0)], closed: false };
    world.spawn((Stream::default(), path, Transform::default()));
    let mut time = time;
    run(&mut renderer, &world, &mut time, 10);
    Some((renderer.read_pixels().unwrap().2, eye, look))
}

#[test]
fn a_stream_runs_along_its_path_and_breaks_white_where_it_runs_steep() {
    let Some((pixels, eye, look)) = stream_render(10.0) else { return };
    keep_image("stream.png", &pixels);
    // Its water where it runs level, the sand's warmth gone under it.
    let water = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(0.0, 0.0, 6.0)), 2);
    assert!(water[1] > water[0] + 10, "the stream's water: {water:?}");
    let sand = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(5.0, -3.0, 6.0)), 2);
    assert!(sand[0] > sand[2] + 10, "dry sand beside it: {sand:?}");
    // White water from the drop, churning on past its foot.
    let rapids = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(0.0, -1.5, 18.5)), 2);
    assert!(rapids.iter().all(|&c| c > 120), "rapids: {rapids:?}");
    // Its ripples drift downstream: half a second on, the water there is not as it was.
    let (later, ..) = stream_render(10.5).unwrap();
    let at = on_screen(eye, look, Vec3::new(0.0, 0.0, 6.0));
    let changed = (0..8u32).flat_map(|dy| (0..8u32).map(move |dx| (at.0 - 4 + dx, at.1 - 4 + dy))).filter(|&p| rgb_at(&pixels, p) != rgb_at(&later, p)).count();
    assert!(changed > 16, "its ripples move: {changed} of 64 pixels changed");
}

#[test]
fn lava_glows_where_it_sets_out_crusts_over_as_it_runs_and_a_lake_of_it_cracks() {
    use voxear_render::{Liquid, Pool, Sky, Stream};
    use voxear_scene::{Path, PathPoint};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let eye = Vec3::new(0.0, 12.0, -8.0);
    let look = Quat::from_rotation_arc(Vec3::Z, (Vec3::new(0.0, 0.0, 15.0) - eye).normalize());
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    // Dusk, so the glow shows.
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.3, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 0.3), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let rock = MaterialDesc { base_color: [0.12, 0.11, 0.1, 1.0], roughness: 0.8, ..MaterialDesc::default() };
    let rock = renderer.add_material(&rock).unwrap();
    world.spawn((MeshRenderer::new(plane, rock), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(200.0))));
    // Level, at a metre a second: as many seconds old as metres from where it sets out, crusted over after ten.
    let point = |z: f32| PathPoint { position: [0.0, 0.3, z], width: 4.0 };
    let path = Path { points: vec![point(0.0), point(20.0), point(40.0)], closed: false };
    world.spawn((Stream { liquid: Liquid::Lava, cooling: 10.0, ..Stream::default() }, path, Transform::default()));
    let lake = Pool { liquid: Liquid::Lava, size: [6.0, 6.0], depth: 0.5, ..Pool::default() };
    world.spawn((lake, Transform::from_position(Vec3::new(8.0, 0.2, 14.0))));
    run(&mut renderer, &world, &mut 100.0, 10);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("lava.png", &pixels);
    let fresh = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(0.0, 0.3, 3.5)), 3);
    assert!(fresh[0] > 150 && fresh[0] > fresh[1] + 50 && fresh[1] > fresh[2], "molten orange where it sets out: {fresh:?}");
    let old = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(0.0, 0.3, 30.0)), 4);
    assert!((old[0] - old[2]) * 4 < fresh[0] - fresh[2], "crusted over, its glow gone, thirty metres on: {old:?}");
    // The lake: dark plates, and molten lava glowing between them.
    let (centre, corner) = (on_screen(eye, look, Vec3::new(8.0, 0.2, 14.0)), on_screen(eye, look, Vec3::new(10.0, 0.2, 16.0)));
    let reach = centre.0.abs_diff(corner.0).max(centre.1.abs_diff(corner.1)).max(4);
    let area: Vec<[i32; 3]> = (centre.1 - reach..centre.1 + reach)
        .flat_map(|y| (centre.0 - reach..centre.0 + reach).map(move |x| (x, y)))
        .map(|at| rgb_at(&pixels, at))
        .collect();
    let glowing = area.iter().filter(|c| c[0] > 120 && c[0] > c[2] + 60).count();
    let dark = area.iter().filter(|c| c[0] < 50).count();
    assert!(glowing * 20 > area.len() && dark * 4 > area.len(), "plates and cracks: {glowing} glowing, {dark} dark of {}", area.len());
}

#[test]
fn up_close_the_sea_has_ripples_finer_than_its_waves() {
    use voxear_render::{Ocean, Sky};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Two metres over the water, looking down into the sun's glints.
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.6, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 2.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.4, -1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    world.spawn((Ocean::default(), Transform::default()));
    run(&mut renderer, &world, &mut 30.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("sea up close.png", &pixels);
    // How much each pixel differs from the next across the lower half: ripples a few pixels wide make it large,
    // the waves' long smooth swells small.
    let grey = |at: (u32, u32)| {
        let [r, g, b] = rgb_at(&pixels, at);
        (r + g + b) / 3
    };
    let (mut total, mut count) = (0, 0);
    for y in 200..340u32 {
        for x in 40..600u32 {
            total += (grey((x + 1, y)) - grey((x, y))).abs();
            count += 1;
        }
    }
    // About 2.2 with the ripple cascades, 0.7 without them.
    let detail = total as f32 / count as f32;
    assert!(detail > 1.4, "ripples up close: {detail}");
}

#[test]
fn half_in_the_water_the_view_splits_at_the_waterline() {
    use voxear_render::{Pool, Sky};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    // Two centimetres under a still pool, looking along it: the near plane crosses the surface above its middle.
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, -0.02, 0.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.2, -0.75, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -8.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    world.spawn((Pool { size: [60.0, 60.0], depth: 9.0, ..Pool::default() }, Transform::from_position(Vec3::new(0.0, 0.0, 10.0))));
    // A windless day far off, so the Pool, which has the sea's short waves, lies still.
    let still = voxear_render::Ocean { weather: voxear_render::Weather::Custom, wind: 0.2, swell_wind: 0.2, ..Default::default() };
    world.spawn((still, Transform::from_position(Vec3::new(0.0, -50.0, 0.0))));
    run(&mut renderer, &world, &mut 30.0, 10);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("half in.png", &pixels);
    // Above the waterline the sky, as it is, bluest; below it, the water's green over everything.
    let sky = patch_rgb(&pixels, (320, 40), 4);
    let under = patch_rgb(&pixels, (320, 300), 4);
    assert!(sky[2] > sky[1] && under[1] > under[0] + 15 && under[1] > under[2], "sky {sky:?} over water {under:?}");
    // The meniscus: a dark line across the view between them.
    let line = (60..200u32).find(|&y| {
        let [r, g, b] = rgb_at(&pixels, (320, y));
        r + g + b < (sky[0] + sky[1] + sky[2]) / 2
    });
    assert!(line.is_some_and(|y| y < 180), "a dark line above the middle: {line:?}");
}

/// A still sea seen from a metre and a half under it, looking up across
/// Snell's window towards a red box standing out of the water, or none.
fn window_render(boxed: bool) -> Option<Vec<u8>> {
    use voxear_render::{Ocean, Sky, Weather};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, 0.55, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, -1.5, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -6.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    if boxed {
        let cube = renderer.primitive(Primitive::Cube).unwrap();
        let red = MaterialDesc { base_color: [0.9, 0.05, 0.05, 1.0], roughness: 0.8, ..MaterialDesc::default() };
        let red = renderer.add_material(&red).unwrap();
        world.spawn((MeshRenderer::new(cube, red), Transform::new(Vec3::new(0.0, 1.0, 7.0), Quat::IDENTITY, Vec3::new(6.0, 14.0, 2.0))));
    }
    let still = Ocean { weather: Weather::Custom, wind: 0.5, swell_wind: 0.2, ..Ocean::default() };
    world.spawn((still, Transform::default()));
    run(&mut renderer, &world, &mut 30.0, 10);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn from_under_the_sea_what_stands_over_it_shows_through_snells_window() {
    let (Some(open), Some(boxed)) = (window_render(false), window_render(true)) else { return };
    keep_image("window.png", &boxed);
    // The box above the water, bent down into the window, reddens the surface seen from under it.
    let redder = (0..180u32)
        .flat_map(|y| (0..640u32).map(move |x| (x, y)))
        .filter(|&at| {
            let ([r, g, _], [r0, g0, _]) = (rgb_at(&boxed, at), rgb_at(&open, at));
            (r - g) - (r0 - g0) > 12
        })
        .count();
    assert!(redder > 200, "the box through the window: {redder} pixels");
}

#[test]
fn a_stormy_sea_calms_and_stops_at_the_beach() {
    use voxear_render::{Ocean, Sky, Weather};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let eye = Vec3::new(-6.0, 4.0, -10.0);
    let look = Quat::from_rotation_arc(Vec3::Z, (Vec3::new(0.0, 0.0, 14.0) - eye).normalize());
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.7, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    // A beach rising one in ten from under the sea, the shore at z = 10.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    let tilt = 0.1f32.atan();
    let beach = Transform::new(Vec3::new(0.0, -1.0, 10.0 - 10.0), Quat::from_rotation_x(-tilt), Vec3::new(200.0, 2.0, 200.0));
    world.spawn((MeshRenderer::new(cube, sand), beach));
    world.spawn((Ocean { weather: Weather::Stormy, ..Ocean::default() }, Transform::default()));
    run(&mut renderer, &world, &mut 30.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("beach.png", &pixels);
    // A metre up the beach, ten centimetres above the sea: sand, where a storm's waves would wash over it.
    let shore = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(0.0, 0.1, 11.0)), 2);
    assert!(shore[0] > shore[2] + 15, "sand above the waterline: {shore:?}");
    let sea = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(4.0, 0.0, -2.0)), 3);
    assert!(sea[2] > sea[0] || sea.iter().all(|&c| c > 150), "water out at sea: {sea:?}");
}

/// A closed room with a roof six metres up, its floor flooded 0.4 m deep by
/// a stormy sea that runs on outside its walls, lit by a lamp, at `time`;
/// or the room alone, dry.
fn flooded_room_render(sea: bool, time: f32) -> Option<Vec<u8>> {
    use voxear_render::{Ocean, Sky, Weather};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(ROOM_EYE, room_look(), Vec3::ONE)));
    world.spawn((Light::point(Vec3::ONE, 40.0, 30.0), Transform::from_position(Vec3::new(0.0, 4.0, 0.0))));
    world.insert_resource(Sky { ambient: 0.3, ..Sky::default() });
    let (plane, cube) = (renderer.primitive(Primitive::Plane).unwrap(), renderer.primitive(Primitive::Cube).unwrap());
    let colour = |base_color: [f32; 4]| MaterialDesc { base_color, ..MaterialDesc::default() };
    let grey = renderer.add_material(&colour([0.5, 0.5, 0.5, 1.0])).unwrap();
    let red = renderer.add_material(&colour([0.8, 0.1, 0.1, 1.0])).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(20.0))));
    let block = |at: Vec3, size: Vec3| Transform::new(at, Quat::IDENTITY, size);
    for (at, size) in [
        (Vec3::new(0.0, 3.0, 10.0), Vec3::new(20.0, 6.0, 0.5)),
        (Vec3::new(0.0, 3.0, -10.0), Vec3::new(20.0, 6.0, 0.5)),
        (Vec3::new(10.0, 3.0, 0.0), Vec3::new(0.5, 6.0, 20.0)),
        (Vec3::new(-10.0, 3.0, 0.0), Vec3::new(0.5, 6.0, 20.0)),
    ] {
        world.spawn((MeshRenderer::new(cube, red), block(at, size)));
    }
    world.spawn((MeshRenderer::new(cube, grey), block(Vec3::new(0.0, 6.25, 0.0), Vec3::new(20.5, 0.5, 20.5))));
    if sea {
        let level = Transform::from_position(Vec3::new(0.0, 0.4, 0.0));
        world.spawn((Ocean { weather: Weather::Stormy, ..Ocean::default() }, level));
    }
    let mut time = time;
    run(&mut renderer, &world, &mut time, 20);
    Some(renderer.read_pixels().unwrap().2)
}

const ROOM_EYE: Vec3 = Vec3::new(0.0, 1.6, -7.0);

fn room_look() -> Quat {
    Quat::from_rotation_arc(Vec3::Z, (Vec3::new(0.0, 0.4, 9.0) - ROOM_EYE).normalize())
}

#[test]
fn a_sea_floods_a_room_under_its_roof_and_lies_calm_inside_its_walls() {
    let (Some(dry), Some(early)) = (flooded_room_render(false, 30.0), flooded_room_render(true, 30.0)) else { return };
    let late = flooded_room_render(true, 33.3).unwrap();
    keep_image("flooded room.png", &early);
    keep_image("flooded room later.png", &late);
    // The floor halfway to the far wall, under the roof: water over it, not the ground taken for the roof.
    let floor = on_screen(ROOM_EYE, room_look(), Vec3::new(0.0, 0.4, 2.0));
    let (bare, flooded) = (patch_rgb(&dry, floor, 4), patch_rgb(&early, floor, 4));
    let apart = (0..3).map(|c| (bare[c] - flooded[c]).abs()).sum::<i32>();
    assert!(apart > 15, "water over the floor: {flooded:?} against the dry floor {bare:?}");
    // Half a metre above the still water, on the far wall: dry wall, red, as the storm outside cannot reach in.
    let wall = on_screen(ROOM_EYE, room_look(), Vec3::new(0.0, 0.9, 9.7));
    for (pixels, when) in [(&early, "early"), (&late, "later")] {
        let rgb = patch_rgb(pixels, wall, 2);
        assert!(rgb[0] > rgb[2] + 25, "the wall above the water stays dry ({when}): {rgb:?}");
    }
}

/// A calm sea at night over sand three metres down, with a lamp a metre
/// under its surface, `lamp`, or none; with the sea or without it; seen from
/// `eye`, looking at the sand under the lamp.
fn lamp_in_water_render(sea: bool, lamp: bool, eye: Vec3) -> Option<Vec<u8>> {
    use voxear_render::{Ocean, Sky, Weather};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, (LAMP_SAND - eye).normalize());
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.02, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    let floor = Transform::new(Vec3::new(0.0, -3.0, 0.0), Quat::IDENTITY, Vec3::splat(60.0));
    world.spawn((MeshRenderer::new(plane, sand), floor));
    if lamp {
        let light = Transform::from_position(LAMP_SAND + Vec3::new(0.0, 2.0, 0.0));
        world.spawn((Light::point(Vec3::ONE, 30.0, 12.0), light));
    }
    if sea {
        world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    }
    let mut post = steady_post();
    post.exposure = voxear_render::Exposure { compensation: 2.0, ..post.exposure };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 20.0, 20);
    Some(renderer.read_pixels().unwrap().2)
}

const LAMP_SAND: Vec3 = Vec3::new(0.0, -3.0, 6.0);

#[test]
fn a_lamp_under_water_glows_in_it_and_lights_the_sand_through_it() {
    let above = Vec3::new(0.0, 3.0, -2.0);
    let render = |sea: bool, lamp: bool| lamp_in_water_render(sea, lamp, above);
    let (Some(dry), Some(wet), Some(dark)) = (render(false, true), render(true, true), render(true, false)) else {
        return;
    };
    let under = Vec3::new(0.0, -1.5, -2.0);
    let below = lamp_in_water_render(true, true, under).unwrap();
    keep_image("lamp in air.png", &dry);
    keep_image("lamp in water.png", &wet);
    keep_image("lamp in water from under.png", &below);
    let at = |eye: Vec3, point: Vec3| {
        on_screen(eye, Quat::from_rotation_arc(Vec3::Z, (LAMP_SAND - eye).normalize()), point)
    };
    // The sand under the lamp, lit through two metres of water: dimmer, and its red taken first.
    let sand = at(above, LAMP_SAND);
    let (in_air, in_water) = (patch_rgb(&dry, sand, 4), patch_rgb(&wet, sand, 4));
    assert!(in_water[0] * 10 < in_air[0] * 7, "the water dims the lamp's light: {in_water:?} against {in_air:?}");
    assert!(in_water[1] > in_water[0], "and takes its red first: {in_water:?}");
    // Seen from under the water, the water round the lamp glows, more than further off.
    let lamp = LAMP_SAND + Vec3::new(0.0, 2.0, 0.0);
    let round = patch_rgb(&below, at(under, lamp), 3);
    let off = patch_rgb(&below, at(under, lamp + Vec3::new(6.0, 0.0, 0.0)), 3);
    let bright = |rgb: [i32; 3]| rgb.iter().sum::<i32>();
    assert!(bright(round) > bright(off) + 60, "a glow round the lamp: {round:?} against {off:?} further off");
    // From above, the water over it is brighter than with no lamp.
    let over = at(above, lamp);
    let (unlit, lit) = (patch_rgb(&dark, over, 4), patch_rgb(&wet, over, 4));
    assert!(bright(lit) > bright(unlit) + 60, "the water over it lit: {lit:?} against {unlit:?}");
}

#[test]
fn a_lamp_over_the_water_glints_on_it() {
    let above = Vec3::new(0.0, 3.0, -2.0);
    let (Some(dark), Some(lit)) = (lamp_over_water_render(false), lamp_over_water_render(true)) else { return };
    keep_image("lamp over water.png", &lit);
    let look = Quat::from_rotation_arc(Vec3::Z, (LAMP_SAND - above).normalize());
    // Where the lamp half a metre over the water shows in it: the water straight under it, seen from the camera.
    let under = on_screen(above, look, Vec3::new(0.0, 0.0, 6.0));
    let bright = |rgb: [i32; 3]| rgb.iter().sum::<i32>();
    let (before, after) = (patch_rgb(&dark, under, 6), patch_rgb(&lit, under, 6));
    assert!(bright(after) > bright(before) + 60, "the lamp lights the water under it: {after:?} against {before:?}");
}

/// The calm night sea of lamp_in_water_render, with a lamp half a metre
/// over its surface, or none.
fn lamp_over_water_render(lamp: bool) -> Option<Vec<u8>> {
    use voxear_render::{Ocean, Sky, Weather};
    let mut renderer = renderer()?;
    let mut world = World::new();
    let eye = Vec3::new(0.0, 3.0, -2.0);
    let look = Quat::from_rotation_arc(Vec3::Z, (LAMP_SAND - eye).normalize());
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.02, ..Sky::default() });
    if lamp {
        world.spawn((Light::point(Vec3::ONE, 10.0, 12.0), Transform::from_position(Vec3::new(0.0, 0.5, 6.0))));
    }
    world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    let mut post = steady_post();
    post.exposure = voxear_render::Exposure { compensation: 2.0, ..post.exposure };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut 20.0, 20);
    Some(renderer.read_pixels().unwrap().2)
}

/// A Particle System, `system`, at `at`, before a grey wall standing three
/// metres along +Z, seen from six metres off, `seconds` after it started;
/// or none.
fn particles_render(system: Option<voxear_render::ParticleSystem>, at: Vec3, seconds: f32) -> Option<Vec<u8>> {
    use voxear_render::{ParticlePlayback, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, -6.0))));
    world.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grey = MaterialDesc { base_color: [0.3, 0.3, 0.3, 1.0], ..MaterialDesc::default() };
    let grey = renderer.add_material(&grey).unwrap();
    let wall = Transform::new(Vec3::new(0.0, 2.0, 3.0), Quat::IDENTITY, Vec3::new(12.0, 6.0, 0.2));
    world.spawn((MeshRenderer::new(cube, grey), wall));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 1.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let mut time = 10.0;
    if let Some(system) = system {
        let playback = ParticlePlayback { started: time - seconds, stopped: None };
        world.spawn((system, playback, Transform::from_position(at)));
    }
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut time, 2);
    Some(renderer.read_pixels().unwrap().2)
}

/// Sparks: bright, added to what is behind them, shooting up out of a cone.
fn sparks() -> voxear_render::ParticleSystem {
    use voxear_render::{ParticleBlend, ParticleSystem};
    ParticleSystem {
        rate: 200.0,
        lifetime: [1.0, 1.5],
        speed: [3.0, 4.0],
        size: [0.08, 0.12],
        angle: 25.0,
        gravity: 1.0,
        color: [1.0, 0.6, 0.2, 1.0],
        color_end: [1.0, 0.3, 0.1, 1.0],
        intensity: 4.0,
        blend: ParticleBlend::Additive,
        lit: false,
        ..ParticleSystem::default()
    }
}

/// How many pixels of `pixels` are much brighter in red than in `before`.
fn brighter_pixels(pixels: &[u8], before: &[u8]) -> usize {
    let all = (0..360u32).flat_map(|y| (0..640u32).map(move |x| (x, y)));
    all.filter(|&at| rgb_at(pixels, at)[0] > rgb_at(before, at)[0] + 40).count()
}

#[test]
fn a_particle_system_sends_out_sparks_that_fall_back_and_stops_when_it_is_done() {
    use voxear_render::ParticleSystem;
    let Some(empty) = particles_render(None, Vec3::ZERO, 0.0) else { return };
    let going = particles_render(Some(sparks()), Vec3::ZERO, 2.0).unwrap();
    keep_image("particles sparks.png", &going);
    let shown = brighter_pixels(&going, &empty);
    assert!(shown > 300, "sparks over the wall: {shown} pixels");
    // A round of one second that does not loop, three seconds on: every spark has died.
    let once = ParticleSystem { looping: false, duration: 1.0, ..sparks() };
    let done = particles_render(Some(once), Vec3::ZERO, 3.0).unwrap();
    assert_eq!(brighter_pixels(&done, &empty), 0, "nothing once it is done");
    // Behind the wall, rising slowly, none show over it.
    let slow = ParticleSystem { gravity: 0.0, speed: [0.5, 1.0], ..sparks() };
    let behind = particles_render(Some(slow), Vec3::new(0.0, 0.5, 5.0), 2.0).unwrap();
    assert_eq!(brighter_pixels(&behind, &empty), 0, "hidden behind the wall");
}

#[test]
fn curves_over_life_set_a_particles_opacity_colour_and_speed() {
    use voxear_core::curve::{Curve, Gradient};
    use voxear_render::ParticleSystem;
    let Some(empty) = particles_render(None, Vec3::ZERO, 0.0) else { return };
    let render = |system: ParticleSystem| particles_render(Some(system), Vec3::ZERO, 2.0).unwrap();
    let hidden = render(ParticleSystem { opacity_curve: Curve::flat(0.0), ..sparks() });
    assert_eq!(brighter_pixels(&hidden, &empty), 0, "no opacity all their lives, none show");
    let blue = render(ParticleSystem { color_gradient: Gradient::new(&[[0.0, 0.1, 0.2, 1.0]]), ..sparks() });
    keep_image("particles blue.png", &blue);
    let bluer = (0..360u32)
        .flat_map(|y| (0..640u32).map(move |x| (x, y)))
        .filter(|&at| rgb_at(&blue, at)[2] > rgb_at(&empty, at)[2] + 40 && rgb_at(&blue, at)[0] < rgb_at(&blue, at)[2])
        .count();
    assert!(bluer > 200, "blue all their lives: {bluer} pixels");
    // Not flying at all, they only fall from where they set out: none above it.
    let still = render(ParticleSystem { speed_curve: Curve::flat(0.0), ..sparks() });
    let upper = (0..170u32).flat_map(|y| (0..640u32).map(move |x| (x, y)));
    let above = upper.filter(|&at| rgb_at(&still, at)[0] > rgb_at(&empty, at)[0] + 40).count();
    let flying = brighter_pixels(&render(sparks()), &empty);
    assert!(above == 0 && flying > 300, "none rise when they do not fly: {above} pixels up, {flying} when they do");
}

/// Sparks in thick height fog, `near` or far off, in front of a dark wall
/// sixty metres away; or none.
fn fogged_sparks_render(sparks_at: Option<Vec3>) -> Option<Vec<u8>> {
    use voxear_render::{Fog, ParticlePlayback, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.5, -6.0))));
    world.insert_resource(Sky { ambient: 0.1, ..Sky::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let dark = MaterialDesc { base_color: [0.05, 0.05, 0.05, 1.0], ..MaterialDesc::default() };
    let dark = renderer.add_material(&dark).unwrap();
    let wall = Transform::new(Vec3::new(0.0, 5.0, 54.0), Quat::IDENTITY, Vec3::new(200.0, 30.0, 0.2));
    world.spawn((MeshRenderer::new(cube, dark), wall));
    let mut time = 10.0;
    if let Some(at) = sparks_at {
        let playback = ParticlePlayback { started: time - 2.0, stopped: None };
        world.spawn((sparks(), playback, Transform::from_position(at)));
    }
    let mut post = steady_post();
    post.fog = Fog { enabled: true, density: 0.05, height_falloff: 0.01, color: Some([0.6, 0.6, 0.6]), ..post.fog };
    world.insert_resource(post);
    run(&mut renderer, &world, &mut time, 2);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn particles_are_fogged_as_far_off_as_they_are() {
    let Some(empty) = fogged_sparks_render(None) else { return };
    let near = fogged_sparks_render(Some(Vec3::new(-1.5, 0.5, -1.0))).unwrap();
    let far = fogged_sparks_render(Some(Vec3::new(1.5, 0.5, 30.0))).unwrap();
    keep_image("particles fog near.png", &near);
    keep_image("particles fog far.png", &far);
    let (shown_near, shown_far) = (brighter_pixels(&near, &empty), brighter_pixels(&far, &empty));
    assert!(shown_near > 300, "sparks five metres off show through the fog: {shown_near} pixels");
    assert!(shown_far * 4 < shown_near, "those thirty-five off fade into it: {shown_far} against {shown_near}");
}

#[test]
fn smoke_rises_spreading_and_greys_what_is_behind_it() {
    use voxear_render::ParticleSystem;
    let Some(empty) = particles_render(None, Vec3::ZERO, 0.0) else { return };
    let smoke = ParticleSystem {
        rate: 30.0,
        lifetime: [3.0, 4.0],
        speed: [0.6, 0.9],
        size: [0.4, 0.6],
        size_end: 3.0,
        angle: 15.0,
        gravity: -0.05,
        drag: 0.3,
        spin: 30.0,
        color: [0.8, 0.8, 0.8, 0.6],
        color_end: [0.6, 0.6, 0.6, 0.0],
        ..ParticleSystem::default()
    };
    let rising = particles_render(Some(smoke), Vec3::ZERO, 5.0).unwrap();
    keep_image("particles smoke.png", &rising);
    // Over the wall above the source, the smoke paler than the dark wall.
    let (wall, smoky) = (patch_rgb(&empty, (320, 200), 6), patch_rgb(&rising, (320, 200), 6));
    assert!(smoky[0] > wall[0] + 15, "smoke over the wall: {smoky:?} against {wall:?}");
}

/// A ball of smoke `seconds` after it started, its cells filled by hand
/// as the engine would in the open, two metres up and six off, before a
/// dark wall, with a thin pale post in front of it; or none.
fn smoke_render(seconds: Option<f32>) -> Option<Vec<u8>> {
    smoke_shot_render(seconds, false, false)
}

/// The smoke of smoke_render, lasting two seconds and dispersing over
/// four, `seconds` after it started.
fn smoke_ending_render(seconds: f32) -> Option<Vec<u8>> {
    smoke_with_render(Some(seconds), false, false, |smoke| voxear_render::Smoke { lasts: 2.0, fade: 4.0, ..smoke })
}

/// The smoke of smoke_render, and with `shot`, a shot from the camera
/// through it a moment before, as a player's own; with `lamp`, a red lamp
/// in it.
fn smoke_shot_render(seconds: Option<f32>, shot: bool, lamp: bool) -> Option<Vec<u8>> {
    smoke_with_render(seconds, shot, lamp, |smoke| smoke)
}

/// The smoke of smoke_shot_render, its settings changed by `set`.
fn smoke_with_render(
    seconds: Option<f32>,
    shot: bool,
    lamp: bool,
    set: impl Fn(voxear_render::Smoke) -> voxear_render::Smoke,
) -> Option<Vec<u8>> {
    use std::sync::Arc;
    use voxear_render::{SMOKE_GRID, Sky, Smoke, SmokeFill, smoke_origin};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 2.0, -6.0))));
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let dark = MaterialDesc { base_color: [0.05, 0.05, 0.05, 1.0], ..MaterialDesc::default() };
    let dark = renderer.add_material(&dark).unwrap();
    let pale = MaterialDesc { base_color: [0.9, 0.5, 0.2, 1.0], ..MaterialDesc::default() };
    let pale = renderer.add_material(&pale).unwrap();
    let wall = Transform::new(Vec3::new(0.0, 4.0, 8.0), Quat::IDENTITY, Vec3::new(30.0, 10.0, 0.2));
    world.spawn((MeshRenderer::new(cube, dark), wall));
    let post = Transform::new(Vec3::new(0.0, 2.0, -4.5), Quat::IDENTITY, Vec3::new(0.05, 6.0, 0.05));
    world.spawn((MeshRenderer::new(cube, pale), post));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if lamp {
        let red = Light::point(Vec3::new(1.0, 0.1, 0.05), 5.0, 4.0);
        world.spawn((red, Transform::from_position(Vec3::new(-1.0, 2.0, -1.0))));
    }
    let mut time = 10.0;
    if let Some(seconds) = seconds {
        let smoke = set(Smoke { cell: 0.4, spread: 2.0, ..Smoke::default() });
        let middle = Vec3::new(0.0, 2.0, 0.0);
        let n = SMOKE_GRID as i32;
        let cells: Vec<u8> = (0..n * n * n)
            .map(|i| {
                let c = Vec3::new((i % n) as f32, ((i / n) % n) as f32, (i / (n * n)) as f32) - (n / 2) as f32;
                // How far out, in half cells, as the engine counts.
                if c.length() <= 8.0 { (c.length() * 2.0).round() as u8 + 1 } else { 0 }
            })
            .collect();
        let fill = SmokeFill {
            cells: Arc::from(cells),
            steps: 16,
            origin: smoke_origin(middle, 0.4),
            filled_for: (smoke, middle),
            started: time - seconds,
            ends: None,
        };
        let mut holes = voxear_render::SmokeHoles::default();
        if shot {
            holes.shoot(Vec3::new(0.0, 2.0, -6.0), Vec3::new(1.2, 2.0, 8.0), time - 0.05);
        }
        world.spawn((smoke, fill, holes, Transform::from_position(middle)));
    }
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut time, 16);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn smoke_fills_its_cells_greys_the_scene_and_keeps_its_edge_on_what_stands_before_it() {
    let Some(clear) = smoke_render(None) else { return };
    let smoky = smoke_render(Some(4.0)).unwrap();
    let early = smoke_render(Some(0.3)).unwrap();
    keep_image("smoke.png", &smoky);
    keep_image("smoke early.png", &early);
    let middle = on_screen(Vec3::new(0.0, 2.0, -6.0), Quat::IDENTITY, Vec3::new(0.6, 2.0, 0.0));
    let (before, after) = (patch_rgb(&clear, middle, 3), patch_rgb(&smoky, middle, 3));
    let greyer = after.iter().sum::<i32>() > before.iter().sum::<i32>() + 90;
    assert!(greyer, "the smoke over the dark wall: {after:?} against {before:?}");
    // The post in front, seen sharp: its pixels as without the smoke, though those beside it are smoky.
    let post = on_screen(Vec3::new(0.0, 2.0, -6.0), Quat::IDENTITY, Vec3::new(0.0, 2.0, -4.5));
    let (bare, through) = (rgb_at(&clear, post), rgb_at(&smoky, post));
    let apart = (0..3).map(|c| (bare[c] - through[c]).abs()).max().unwrap();
    assert!(apart < 25, "the post before the smoke stays as it was: {through:?} against {bare:?}");
    let changed = |pixels: &[u8]| {
        (0..360u32).flat_map(|y| (0..640u32).map(move |x| (x, y))).filter(|&at| {
            (0..3).map(|c| (rgb_at(pixels, at)[c] - rgb_at(&clear, at)[c]).abs()).max().unwrap() > 20
        }).count()
    };
    let (small, full) = (changed(&early), changed(&smoky));
    assert!(small * 2 < full, "spreading out from its middle: {small} pixels early, {full} once spread");
}

#[test]
fn smoke_disperses_into_wisps_and_is_gone_at_its_end() {
    let Some(clear) = smoke_render(None) else { return };
    let whole = smoke_ending_render(2.0).unwrap();
    let going = smoke_ending_render(4.5).unwrap();
    let gone = smoke_ending_render(6.5).unwrap();
    keep_image("smoke dispersing.png", &going);
    let changed = |pixels: &[u8]| {
        let all = (0..360u32).flat_map(|y| (0..640u32).map(move |x| (x, y)));
        let apart = |at: (u32, u32)| (0..3).map(|c| (rgb_at(pixels, at)[c] - rgb_at(&clear, at)[c]).abs()).max().unwrap();
        all.filter(|&at| apart(at) > 20).count()
    };
    let (a, b, c) = (changed(&whole), changed(&going), changed(&gone));
    assert!(b * 10 < a * 9 && b > a / 10, "breaking up: {b} pixels of smoke, from {a}");
    assert_eq!(c, 0, "gone at the end");
}

#[test]
fn a_lamp_in_smoke_lights_it_round_itself() {
    let Some(plain) = smoke_shot_render(Some(4.0), false, false) else { return };
    let lit = smoke_shot_render(Some(4.0), false, true).unwrap();
    keep_image("smoke lamp.png", &lit);
    let near = on_screen(Vec3::new(0.0, 2.0, -6.0), Quat::IDENTITY, Vec3::new(-1.0, 2.0, -1.0));
    let (before, after) = (patch_rgb(&plain, near, 3), patch_rgb(&lit, near, 3));
    assert!(after[0] > before[0] + 30 && after[0] > after[2] + 30, "red round the lamp: {after:?} against {before:?}");
}

#[test]
fn a_shot_cuts_a_tunnel_through_smoke() {
    let Some(clear) = smoke_shot_render(None, false, false) else { return };
    let shot = smoke_shot_render(Some(4.0), true, false).unwrap();
    keep_image("smoke shot.png", &shot);
    let eye = Vec3::new(0.0, 2.0, -6.0);
    // Down the shot's tunnel, the dark wall behind shows again; half a metre above it, the smoke stays.
    let on = on_screen(eye, Quat::IDENTITY, Vec3::new(1.2, 2.0, 8.0));
    let above = on_screen(eye, Quat::IDENTITY, Vec3::new(1.2, 3.0, 8.0));
    let bright = |rgb: [i32; 3]| rgb.iter().sum::<i32>();
    let (tunnel, smoke, wall) = (patch_rgb(&shot, on, 1), patch_rgb(&shot, above, 1), patch_rgb(&clear, on, 1));
    assert!(bright(tunnel) + 60 < bright(smoke), "darker along the tunnel: {tunnel:?} against {smoke:?} above it");
    assert!(bright(tunnel) < bright(wall) + 150, "the wall shows through: {tunnel:?} against {wall:?}");
}

/// Where `point` is on screen in a 640×360 picture from a camera at `eye`
/// looking along `look`, as the default camera's 60° projection puts it.
fn on_screen(eye: Vec3, look: Quat, point: Vec3) -> (u32, u32) {
    use voxear_core::math::{Mat4, camera::lh::proj::vulkan};
    let view = Mat4::from_rotation_translation(look, eye).inverse();
    let clip = vulkan::perspective_infinite_reverse(60f32.to_radians(), 640.0 / 360.0, 0.1) * view * point.extend(1.0);
    (((clip.x / clip.w * 0.5 + 0.5) * 640.0) as u32, ((clip.y / clip.w * 0.5 + 0.5) * 360.0) as u32)
}

#[test]
fn a_dry_area_keeps_the_sea_out_and_a_pool_holds_water_above_it() {
    use voxear_render::{DryArea, Ocean, Pool, Sky, Weather};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let (eye, look) = (Vec3::new(0.0, 6.0, -8.0), Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.35, 1.0).normalize()));
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.6, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -3.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    // Down past the floor, so the hole is dry to the sand.
    let hole = Vec3::new(-4.0, 0.0, 6.0);
    world.spawn((DryArea { size: [3.0, 8.0, 7.0], ..DryArea::default() }, Transform::from_position(hole)));
    // A block of water raised over the sea, three metres deep, standing on nothing.
    let pond = Vec3::new(5.0, 4.0, 8.0);
    world.spawn((Pool { size: [6.0, 6.0], depth: 3.0, ..Pool::default() }, Transform::from_position(pond)));
    run(&mut renderer, &world, &mut 10.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("water bodies.png", &pixels);
    // The sand three metres down at the hole's far end, seen past its open top, warm where the sea is blue.
    let floor = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(-4.0, -3.0, 9.0)), 2);
    assert!(floor[0] > floor[2] + 8, "the sand at the bottom of the dry hole: {floor:?}");
    // Its far face a wall of water, the sea standing beyond it.
    let wall = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(-4.0, -1.0, 9.5)), 2);
    assert!(wall[1] > wall[0] + 15, "a wall of water at the hole's far end: {wall:?}");
    let sea = patch_rgb(&pixels, on_screen(eye, look, hole + Vec3::new(4.0, 0.0, -1.0)), 3);
    assert!(sea[2] > sea[0], "the sea beside it: {sea:?}");
    // The pool's water, raised over the sea, and its side seen down to its bottom, against the sky behind.
    let water = patch_rgb(&pixels, on_screen(eye, look, pond), 3);
    assert!(water[1] > water[0] + 10, "the pool's water: {water:?}");
    let side = patch_rgb(&pixels, on_screen(eye, look, pond + Vec3::new(0.0, -2.0, -3.0)), 2);
    assert!(side[1] > side[0] + 10, "the pool's side, a wall of water: {side:?}");
}

#[test]
fn a_dry_area_with_a_bilge_holds_level_water_inside_it() {
    use voxear_render::{DryArea, Ocean, Sky, Weather};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let (eye, look) = (Vec3::new(0.0, 6.0, -8.0), Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.35, 1.0).normalize()));
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.6, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -3.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    // Two dry holes down past the sand, the right one rolled a little and holding water a metre and a half over the
    // sand: its bilge two and a half metres up from its middle's bottom.
    let (dry, wet) = (Vec3::new(-4.0, 0.0, 6.0), Vec3::new(4.0, 0.0, 6.0));
    let hole = DryArea { size: [3.0, 8.0, 7.0], ..DryArea::default() };
    world.spawn((hole, Transform::from_position(dry)));
    let roll = Quat::from_rotation_z(0.15);
    world.spawn((DryArea { bilge: 2.5, ..hole }, Transform::new(wet, roll, Vec3::ONE)));
    run(&mut renderer, &world, &mut 10.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("bilge.png", &pixels);
    let bare = patch_rgb(&pixels, on_screen(eye, look, dry + Vec3::new(0.0, -3.0, 3.0)), 2);
    assert!(bare[0] > bare[2] + 8, "the sand at the bottom of the dry hole: {bare:?}");
    // The same sand under the bilge's water, the red taken from it.
    let under = patch_rgb(&pixels, on_screen(eye, look, wet + Vec3::new(0.0, -1.5, 2.0)), 2);
    assert!(under[1] > under[0], "the sand under the bilge: {under:?} beside {bare:?}");
    let sea = patch_rgb(&pixels, on_screen(eye, look, wet + Vec3::new(4.0, 0.0, -1.0)), 3);
    assert!(sea[2] > sea[0], "the sea beside it: {sea:?}");
}

#[test]
fn a_mesh_dry_area_keeps_the_sea_out_of_its_inside() {
    use voxear_render::{DryArea, DryShape, Ocean, Sky, Weather};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let (eye, look) = (Vec3::new(0.0, 6.0, -8.0), Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.35, 1.0).normalize()));
    world.spawn((Camera::default(), Transform::new(eye, look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.6, 0.6).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.6, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.76, 0.66, 0.48, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -3.0, 0.0), Quat::IDENTITY, Vec3::splat(200.0))));
    world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    // A ball ten metres across at the sea's level, kept dry by its mesh alone, which is not drawn.
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let hull = Vec3::new(-4.0, 0.0, 5.0);
    let dry = DryArea { shape: DryShape::Mesh, mesh: Some(sphere), ..DryArea::default() };
    world.spawn((dry, Transform::new(hull, Quat::IDENTITY, Vec3::splat(10.0))));
    run(&mut renderer, &world, &mut 10.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("mesh dry area.png", &pixels);
    // The sand three metres down inside it, seen with no water over it, warm where the sea is blue.
    let floor = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(-4.0, -3.0, 6.0)), 2);
    assert!(floor[0] > floor[2] + 8, "the sand in the dry ball: {floor:?}");
    let sea = patch_rgb(&pixels, on_screen(eye, look, Vec3::new(5.0, 0.0, 4.0)), 3);
    assert!(sea[2] > sea[0], "the sea beside it: {sea:?}");
}

#[test]
fn under_the_sea_the_water_tints_all_and_caustics_light_the_sand() {
    use voxear_render::{Ocean, Sky, Weather};
    let Some(mut renderer) = renderer() else { return };
    let mut world = World::new();
    let look = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.0, -0.5, 1.0).normalize());
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, -2.0, 0.0), look, Vec3::ONE)));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.2, -0.8, 0.5).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.8, ..Sky::default() });
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sand = MaterialDesc { base_color: [0.8, 0.72, 0.55, 1.0], roughness: 0.9, ..MaterialDesc::default() };
    let sand = renderer.add_material(&sand).unwrap();
    world.spawn((MeshRenderer::new(plane, sand), Transform::new(Vec3::new(0.0, -5.0, 0.0), Quat::IDENTITY, Vec3::splat(300.0))));
    world.spawn((Ocean { weather: Weather::Calm, ..Ocean::default() }, Transform::default()));
    run(&mut renderer, &world, &mut 20.0, 20);
    let pixels = renderer.read_pixels().unwrap().2;
    keep_image("under the sea.png", &pixels);
    // The sand three metres under the camera, tinted: more green than red, though sand is redder than green.
    let floor = patch_rgb(&pixels, (320, 260), 4);
    assert!(floor[1] > floor[0], "the sand seen through the water: {floor:?}");
    // Caustics: bright lines and darker gaps across the sand.
    let lights: Vec<i32> = (200..340u32).step_by(4).flat_map(|y| (160..480u32).step_by(4).map(move |x| (x, y)))
        .map(|at| rgb_at(&pixels, at)[1]).collect();
    let (low, high) = (lights.iter().min().unwrap(), lights.iter().max().unwrap());
    assert!(high - low > 25, "caustics light the sand unevenly: from {low} to {high}");
}

/// The corner scene in the sun with a box standing in the open, its parent
/// hidden or not, or with no box at all.
fn hidden_render(box_hidden: Option<bool>) -> Option<Vec<u8>> {
    let mut renderer = renderer()?;
    let mut world = corner_scene(&mut renderer, voxear_render::AmbientOcclusion::default());
    let sun = Quat::from_rotation_y(0.6) * Quat::from_rotation_x(0.9);
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    if let Some(hidden) = box_hidden {
        let parent = world.spawn((Transform::from_position(Vec3::new(1.0, 0.0, 0.5)),));
        if hidden {
            world.insert(parent, voxear_scene::Hidden);
        }
        let cube = renderer.primitive(Primitive::Cube).unwrap();
        let material = renderer.add_material(&MaterialDesc::default()).unwrap();
        let place = Transform::from_position(Vec3::new(0.0, 0.5, 0.0));
        world.spawn((MeshRenderer::new(cube, material), place, voxear_scene::Parent(parent)));
        voxear_scene::propagate_transforms(&mut world);
    }
    run(&mut renderer, &world, &mut 0.0, 6);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_hidden_obj_and_those_under_it_leave_no_pixels_and_no_shadow() {
    let renders = (hidden_render(None), hidden_render(Some(false)), hidden_render(Some(true)));
    let (Some(none), Some(shown), Some(hidden)) = renders else { return };
    keep_image("hidden_shown.png", &shown);
    assert!(differing_pixels(&none, &shown) > 2000, "the box shows, and its shadow");
    assert_eq!(differing_pixels(&none, &hidden), 0, "hidden, nothing of it");
}

/// Where each point of the test mist's trail is: an arc across the view,
/// six metres off, from its oldest, made 1.2 seconds ago, to its newest.
fn mist_point(i: usize) -> (Vec3, f32) {
    let t = i as f32 / 15.0;
    let at = Vec3::new(-4.0 + 7.0 * t, 1.5 + 0.6 * (t * 3.0).sin(), 2.0 - t);
    (at, 1.2 * (1.0 - t))
}

/// The smoke scene with the test mist in it, or none, seen from `eye`
/// looking along `facing`.
fn mist_render(mist: bool, eye: Vec3, facing: Quat) -> Option<Vec<u8>> {
    use voxear_render::{Mist, MistTrail, Sky};
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(eye, facing, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.4, ..Sky::default() });
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let dark = MaterialDesc { base_color: [0.05, 0.05, 0.05, 1.0], ..MaterialDesc::default() };
    let dark = renderer.add_material(&dark).unwrap();
    let wall = Transform::new(Vec3::new(0.0, 4.0, 8.0), Quat::IDENTITY, Vec3::new(30.0, 10.0, 0.2));
    world.spawn((MeshRenderer::new(cube, dark), wall));
    let floor = Transform::new(Vec3::new(0.0, -0.1, 0.0), Quat::IDENTITY, Vec3::new(30.0, 0.2, 30.0));
    world.spawn((MeshRenderer::new(cube, dark), floor));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.3, -0.6, 1.0).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let mut time = 10.0;
    if mist {
        let points = (0..=15).map(mist_point).map(|(at, age)| (at, time - age)).collect();
        world.spawn((Mist { emitting: false, ..Mist::default() }, MistTrail { points }, Transform::default()));
    }
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut time, 16);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_mist_trail_shows_along_its_path_thickest_where_it_was_left_last() {
    let (eye, ahead) = (Vec3::new(0.0, 2.0, -6.0), Quat::IDENTITY);
    let Some(clear) = mist_render(false, eye, ahead) else { return };
    let misty = mist_render(true, eye, ahead).unwrap();
    keep_image("mist.png", &misty);
    let change = |world: Vec3| {
        let at = on_screen(eye, ahead, world);
        let (before, after) = (patch_rgb(&clear, at, 3), patch_rgb(&misty, at, 3));
        (0..3).map(|c| after[c] - before[c]).sum::<i32>()
    };
    let (head, _) = mist_point(14);
    let (tail, _) = mist_point(1);
    let (newest, oldest) = (change(head), change(tail + Vec3::Y * 0.4 * 1.1));
    assert!(newest > 60, "the mist where it was left last: {newest}");
    assert!(oldest < newest / 2, "thinner where it is older: {oldest} against {newest}");
    assert!(change(Vec3::new(0.0, 4.5, 1.5)).abs() < 10, "none well above it");
    // From inside its newest end, looking back along it.
    let back = Quat::from_rotation_arc(Vec3::Z, (mist_point(0).0 - head).normalize());
    let inside = mist_render(true, head, back).unwrap();
    keep_image("mist inside.png", &inside);
}

/// A height map of round bumps, `bumps` to a side, as a displaced
/// material's.
fn bumps_texture(renderer: &mut Renderer, bumps: f32) -> voxear_render::TextureId {
    let size = 256u32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let (u, v) = ((i % size) as f32 / size as f32, (i / size) as f32 / size as f32);
            let h = 0.5 + 0.5 * (u * std::f32::consts::TAU * bumps).sin() * (v * std::f32::consts::TAU * bumps).sin();
            let h = (h * 255.0).round() as u8;
            [h, h, h, 255]
        })
        .collect();
    let source = voxear_render::TextureSource { name: "bumps", width: size, height: size, rgba: &rgba, srgb: false };
    renderer.add_texture(&source).unwrap()
}

/// A grid lying flat, facing up, its lines along X at `xs` and along Z at
/// the same, from -1 to 1, so its squares are as uneven as the spacing.
fn ground_grid(xs: &[f32]) -> voxear_render::MeshData {
    let mut mesh = voxear_render::MeshData::default();
    let n = xs.len() as u32;
    for &z in xs {
        for &x in xs {
            mesh.vertices.push(voxear_render::Vertex {
                position: [x, 0.0, z],
                normal: [0.0, 1.0, 0.0],
                uv: [(x + 1.0) * 0.5, (z + 1.0) * 0.5],
                tangent: [1.0, 0.0, 0.0, 1.0],
                ..voxear_render::Vertex::default()
            });
        }
    }
    let at = |x: u32, z: u32| z * n + x;
    for z in 0..n - 1 {
        for x in 0..n - 1 {
            mesh.indices.extend([at(x, z), at(x, z + 1), at(x + 1, z), at(x, z + 1), at(x + 1, z + 1), at(x + 1, z)]);
        }
    }
    mesh
}

/// A ground of `xs`, `scale` metres across, its material displaced by
/// `displacement` metres of bumps, seen from `eye` looking along `facing`,
/// over a bright floor two metres under it; and the frame's stats.
fn displaced_render(xs: &[f32], scale: f32, displacement: f32, eye: Vec3, facing: Quat) -> Option<(Vec<u8>, RenderStats)> {
    displaced_render_with(xs, scale, displacement, (eye, facing), false)
}

/// As displaced_render, with ambient occlusion on when `occluded`, so the
/// pieces are drawn as depth first and shaded over it.
fn displaced_render_with(
    xs: &[f32],
    scale: f32,
    displacement: f32,
    (eye, facing): (Vec3, Quat),
    occluded: bool,
) -> Option<(Vec<u8>, RenderStats)> {
    use voxear_render::Sky;
    let mut renderer = renderer()?;
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::new(eye, facing, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.5, ..Sky::default() });
    let height = bumps_texture(&mut renderer, 6.0);
    let ground = renderer.add_mesh(&ground_grid(xs)).unwrap();
    let mud = MaterialDesc {
        base_color: [0.25, 0.18, 0.12, 1.0],
        height_texture: Some(height),
        displacement,
        detail: 4.0,
        ..MaterialDesc::default()
    };
    let mud = renderer.add_material(&mud).unwrap();
    world.spawn((MeshRenderer::new(ground, mud), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(scale))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let bright = MaterialDesc { base_color: [0.0, 0.0, 0.0, 1.0], emissive: [0.0, 40.0, 0.0], ..MaterialDesc::default() };
    let bright = renderer.add_material(&bright).unwrap();
    let floor = Transform::new(Vec3::new(0.0, -2.0, 0.0), Quat::IDENTITY, Vec3::splat(scale * 4.0));
    world.spawn((MeshRenderer::new(plane, bright), floor));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, -0.6, 0.7).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    let ambient_occlusion = voxear_render::AmbientOcclusion { enabled: occluded, ..voxear_render::AmbientOcclusion::default() };
    world.insert_resource(voxear_render::PostProcess { ambient_occlusion, ..steady_post() });
    let stats = run(&mut renderer, &world, &mut 0.0, 4);
    Some((renderer.read_pixels().unwrap().2, stats))
}

#[test]
fn a_displaced_ground_is_diced_and_shows_its_bumps_against_the_sky() {
    let xs: Vec<f32> = (0..=8).map(|i| i as f32 / 4.0 - 1.0).collect();
    let (eye, ahead) = (Vec3::new(0.0, 0.5, -9.0), Quat::from_rotation_x(0.04));
    let Some((flat, flat_stats)) = displaced_render(&xs, 10.0, 0.0, eye, ahead) else { return };
    let (bumpy, stats) = displaced_render(&xs, 10.0, 0.8, eye, ahead).unwrap();
    keep_image("displaced.png", &bumpy);
    keep_image("displaced flat.png", &flat);
    assert_eq!(flat_stats.diced_patches, 0, "with no displacement, drawn as it is");
    assert!(stats.diced_patches > 0 && stats.dice_overflow == 0, "{stats:?}");
    assert!(differing_pixels(&flat, &bumpy) > 20_000, "the bumps change the ground");
    // Its far edge against the sky rises and falls with the bumps: the first ground row from the top, by column.
    let sky = rgb_at(&bumpy, (320, 5));
    let edge = |x: u32| (0..360u32).find(|&y| (0..3).map(|c| (rgb_at(&bumpy, (x, y))[c] - sky[c]).abs()).max().unwrap() > 25);
    let rows: Vec<u32> = (40..600).step_by(4).filter_map(edge).collect();
    let (top, bottom) = (rows.iter().min().unwrap(), rows.iter().max().unwrap());
    assert!(bottom - top >= 4, "a bumpy horizon: rows {top} to {bottom}");
}

#[test]
fn uneven_triangles_diced_side_by_side_leave_no_crack() {
    // Squares growing fast across the ground, so each triangle is split and cut differently from its neighbours.
    let xs: Vec<f32> = (0..=10).map(|i| (i as f32 / 10.0).powi(3) * 2.0 - 1.0).collect();
    let down = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2 * 0.999);
    for (height, displacement) in [(3.0, 0.6), (8.0, 1.2)] {
        let Some((pixels, stats)) = displaced_render(&xs, 4.0, displacement, Vec3::new(0.0, height, 0.0), down) else {
            return;
        };
        keep_image(&format!("displaced cracks {height}.png"), &pixels);
        assert!(stats.diced_patches > 0);
        // The bright floor under it shows through nowhere within the ground.
        let seen = (60..300u32).flat_map(|y| (180..460u32).map(move |x| (x, y)));
        let through: Vec<(u32, u32)> = seen.filter(|&at| rgb_at(&pixels, at)[1] > 200).collect();
        assert!(through.is_empty(), "the floor shows through at {} pixels, first {:?}", through.len(), through.first());
    }
}

#[test]
fn displaced_ground_drawn_as_depth_first_looks_the_same() {
    let xs: Vec<f32> = (0..=8).map(|i| i as f32 / 4.0 - 1.0).collect();
    let view = (Vec3::new(0.0, 0.5, -9.0), Quat::from_rotation_x(0.04));
    let Some((plain, _)) = displaced_render_with(&xs, 10.0, 0.8, view, false) else { return };
    let (deep, stats) = displaced_render_with(&xs, 10.0, 0.8, view, true).unwrap();
    keep_image("displaced depth first.png", &deep);
    assert!(stats.diced_patches > 0);
    // Ambient occlusion darkens the dips a little; the ground is all there, nowhere the sky or floor instead.
    let changed = differing_pixels_by(&plain, &deep, 60);
    assert!(changed < 640 * 360 / 50, "{changed} pixels far apart");
}

/// How many pixels differ in some channel by more than `by`.
fn differing_pixels_by(a: &[u8], b: &[u8], by: u8) -> usize {
    a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > by)).count()
}

/// The bent bar (two bones, a skin and a clip) from its glTF, uploaded, and
/// drawn bright before a dark wall: at rest, or with its tip's bone turned
/// a quarter about Z, and a cooked copy too.
fn bent_bar() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut put = |values: &[f32]| values.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    put(&[-0.1, 0.0, 0.0, 0.1, 0.0, 0.0, 0.1, 2.0, 0.0, -0.1, 2.0, 0.0]);
    let mut rest = Vec::new();
    for index in [0u16, 1, 2, 0, 2, 3] {
        rest.extend_from_slice(&index.to_le_bytes());
    }
    for joints in [[0u16, 0, 0, 0], [0, 0, 0, 0], [1, 0, 0, 0], [1, 0, 0, 0]] {
        joints.iter().for_each(|j| rest.extend_from_slice(&j.to_le_bytes()));
    }
    buffer.extend(rest);
    let mut put = |values: &[f32]| values.iter().for_each(|v| buffer.extend_from_slice(&v.to_le_bytes()));
    put(&[1.0, 0.0, 0.0, 0.0].repeat(4));
    put(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    put(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0]);
    put(&[0.0, 1.0]);
    let half = std::f32::consts::FRAC_PI_4;
    put(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, half.sin(), half.cos()]);
    let data = format!("data:application/octet-stream;base64,{}", base64::encode(&buffer));
    format!(
        r#"{{
          "asset": {{ "version": "2.0" }},
          "buffers": [{{ "uri": "{data}", "byteLength": {} }}],
          "bufferViews": [
            {{ "buffer": 0, "byteOffset": 0, "byteLength": 48 }},
            {{ "buffer": 0, "byteOffset": 48, "byteLength": 12 }},
            {{ "buffer": 0, "byteOffset": 60, "byteLength": 32 }},
            {{ "buffer": 0, "byteOffset": 92, "byteLength": 64 }},
            {{ "buffer": 0, "byteOffset": 156, "byteLength": 128 }},
            {{ "buffer": 0, "byteOffset": 284, "byteLength": 8 }},
            {{ "buffer": 0, "byteOffset": 292, "byteLength": 32 }}
          ],
          "accessors": [
            {{ "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3", "min": [-0.1, 0, 0], "max": [0.1, 2, 0] }},
            {{ "bufferView": 1, "componentType": 5123, "count": 6, "type": "SCALAR" }},
            {{ "bufferView": 2, "componentType": 5123, "count": 4, "type": "VEC4" }},
            {{ "bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC4" }},
            {{ "bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4" }},
            {{ "bufferView": 5, "componentType": 5126, "count": 2, "type": "SCALAR", "min": [0], "max": [1] }},
            {{ "bufferView": 6, "componentType": 5126, "count": 2, "type": "VEC4" }}
          ],
          "meshes": [{{ "primitives": [{{ "attributes": {{ "POSITION": 0, "JOINTS_0": 2, "WEIGHTS_0": 3 }}, "indices": 1 }}] }}],
          "skins": [{{ "joints": [0, 1], "inverseBindMatrices": 4 }}],
          "animations": [{{ "name": "Bend",
            "samplers": [{{ "input": 5, "output": 6 }}],
            "channels": [{{ "sampler": 0, "target": {{ "node": 1, "path": "rotation" }} }}] }}],
          "nodes": [
            {{ "name": "Root", "children": [1] }},
            {{ "name": "Tip", "translation": [0, 1, 0] }},
            {{ "name": "Bar", "mesh": 0, "skin": 0, "translation": [5, 0, 0] }}
          ],
          "scenes": [{{ "nodes": [0, 2] }}]
        }}"#,
        buffer.len()
    )
    .into_bytes()
}


fn bar_render(cooked: bool, bent: bool) -> Option<Vec<u8>> {
    bar_render_hiding(cooked, bent, false)
}

/// The bar drawn, and with `hide_tip`, its tip's bone hidden from the camera.
fn bar_render_hiding(cooked: bool, bent: bool, hide_tip: bool) -> Option<Vec<u8>> {
    use std::sync::Arc;
    use voxear_render::{ModelPart, ModelRenderer, Pose, Sky};
    let mut renderer = renderer()?;
    let gltf = bent_bar();
    let model = voxear_render::prepare_model("Bar.gltf", &mut |_| Ok(gltf.clone())).unwrap();
    let imported = match cooked {
        true => {
            let bytes = model.cook().unwrap().to_bytes();
            renderer.upload_cooked(&CookedModel::from_bytes("Bar.gltf", &bytes).unwrap()).unwrap()
        }
        false => renderer.upload_model(&model).unwrap(),
    };
    let mut world = World::new();
    // From the side the bar faces.
    let turned = Quat::from_rotation_y(std::f32::consts::PI);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 1.0, 4.0), turned, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 1.0, ..Sky::default() });
    let bright = renderer.add_material(&MaterialDesc { emissive: [4.0, 4.0, 4.0], ..MaterialDesc::default() }).unwrap();
    let parts = imported
        .nodes
        .iter()
        .map(|node| ModelPart {
            mesh: node.mesh,
            material: bright,
            transform: voxear_core::math::Mat4::IDENTITY,
            node: 0,
            skin: node.skin.map(|s| s as u32),
        })
        .collect();
    let animation = imported.animation.clone().unwrap();
    let mut pose = animation.skeleton.rest();
    if bent {
        pose[1].rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    }
    let bones: Arc<[voxear_core::math::Mat4]> = animation.skeleton.model_matrices(&pose).into();
    let drawn = ModelRenderer { animation: Some(animation), ..ModelRenderer::new(parts) };
    let bar = world.spawn((drawn, Pose { bones }, Transform::default()));
    if hide_tip {
        world.insert(bar, voxear_render::HiddenBones { bones: vec![false, true] });
    }
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let dark = renderer.add_material(&MaterialDesc { base_color: [0.02, 0.02, 0.02, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(cube, dark), Transform::new(Vec3::new(0.0, 1.0, -3.0), Quat::IDENTITY, Vec3::new(20.0, 20.0, 0.2))));
    world.insert_resource(steady_post());
    run(&mut renderer, &world, &mut 0.0, 4);
    Some(renderer.read_pixels().unwrap().2)
}

#[test]
fn a_skinned_bar_bends_where_its_bone_turns() {
    let Some(rest) = bar_render(false, false) else { return };
    let bent = bar_render(false, true).unwrap();
    let cooked = bar_render(true, true).unwrap();
    keep_image("bar rest.png", &rest);
    keep_image("bar bent.png", &bent);
    let (eye, ahead) = (Vec3::new(0.0, 1.0, 4.0), Quat::from_rotation_y(std::f32::consts::PI));
    let lit = |pixels: &[u8], at: Vec3| rgb_at(pixels, on_screen(eye, ahead, at))[0] > 150;
    // At rest the bar stands straight up. Bent, its top corners, bound to the tip's bone, swing a quarter turn
    // about the joint a metre up, to a metre out to one side; its four corners make it lean from its foot to
    // there.
    assert!(lit(&rest, Vec3::new(0.0, 1.7, 0.0)) && lit(&rest, Vec3::new(0.0, 0.5, 0.0)));
    assert!(lit(&bent, Vec3::new(0.0, 0.05, 0.0)), "its foot stays");
    assert!(!lit(&bent, Vec3::new(0.0, 1.7, 0.0)), "its top no longer stands up");
    let leaning = [-0.5f32, 0.5].map(|x| lit(&bent, Vec3::new(x, 0.5, 0.0)));
    assert!(leaning[0] ^ leaning[1], "it leans to one side: {leaning:?}");
    assert_eq!(differing_pixels(&bent, &cooked), 0, "cooked, the same");
}

#[test]
fn skin_bound_to_a_hidden_bone_is_left_out_where_the_camera_sees() {
    // The bar's top corners are bound to its tip's bone, its foot's to the root, so how much it is bound to the tip
    // rises along it: hidden, the tip leaves out all of the bar from where it is a quarter bound to the tip, half a
    // metre up, and moves nothing.
    let Some(hidden) = bar_render_hiding(false, false, true) else { return };
    keep_image("bar hidden.png", &hidden);
    let (eye, ahead) = (Vec3::new(0.0, 1.0, 4.0), Quat::from_rotation_y(std::f32::consts::PI));
    let lit = |at: Vec3| rgb_at(&hidden, on_screen(eye, ahead, at))[0] > 150;
    assert!(lit(Vec3::new(0.0, 0.15, 0.0)) && lit(Vec3::new(0.0, 0.38, 0.0)), "the foot stays");
    assert!(!lit(Vec3::new(0.0, 0.65, 0.0)) && !lit(Vec3::new(0.0, 1.5, 0.0)), "the rest is left out");
}

/// A UI Panel in the world shows its UI's colours exactly as designed, being
/// drawn after post-processing; a box in front of it hides it; a fine
/// pattern on it, far smaller on screen than in its texture, comes out as
/// its average rather than as pixels picked from it; and the renderer says
/// how large it showed.
#[test]
fn a_ui_panel_shows_its_colours_in_the_world_hidden_behind_what_is_in_front() {
    use voxear_core::math::{Mat4, Vec4};
    use voxear_render::{UiPanel, UiPanelFrame};
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let (width, height) = (640u32, 360u32);
    let mut world = World::new();
    let lens = Camera::default();
    world.spawn((lens, Transform::IDENTITY));
    // A 2 by 1 metre panel four metres ahead: red on its left half, one-pixel black and white stripes on its right.
    let panel = world.spawn((
        Transform::from_position(Vec3::new(0.0, 0.0, 4.0)),
        UiPanel { size: [2.0, 1.0], ..UiPanel::default() },
    ));
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(500.0).height(500.0).background(Color::hex(0xff0000)));
    ui.panel(root, Style::default().at(500.0, 0.0).width(500.0).height(500.0).background(Color::hex(0x000000)));
    for stripe in (500..1000).step_by(2) {
        let white = Style::default().at(stripe as f32, 0.0).width(1.0).height(500.0).background(Color::hex(0xffffff));
        ui.panel(root, white);
    }
    let frame = ui.frame(1000, 500, 1.0);
    renderer.set_ui_panels(&[UiPanelFrame { entity: panel, frame: &frame, size: [1000, 500] }]).unwrap();
    // A small box two metres ahead, in front of the panel's red half's top.
    let cube = renderer.primitive(Primitive::Cube).unwrap();
    let grey = renderer.add_material(&MaterialDesc::default()).unwrap();
    world.spawn((
        MeshRenderer::new(cube, grey),
        Transform::new(Vec3::new(-0.25, 0.125, 2.0), Quat::IDENTITY, Vec3::splat(0.15)),
    ));
    run(&mut renderer, &world, &mut 0.0, 1);

    // Where a point in the world shows, as the renderer draws it.
    let aspect = width as f32 / height as f32;
    let projection = voxear_core::math::camera::lh::proj::vulkan::perspective_infinite_reverse(
        lens.fov_y_degrees.to_radians(),
        aspect,
        lens.near,
    );
    let pixel = |point: Vec3| {
        let clip: Vec4 = projection * Mat4::IDENTITY * point.extend(1.0);
        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
        (((x + 1.0) * 0.5 * width as f32) as u32, ((y + 1.0) * 0.5 * height as f32) as u32)
    };
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |point: Vec3| {
        let (x, y) = pixel(point);
        let i = ((y * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    assert_eq!(at(Vec3::new(-0.5, -0.25, 4.0)), [255, 0, 0], "its red, exactly, untouched by exposure and tone");
    assert_ne!(at(Vec3::new(-0.5, 0.25, 4.0)), [255, 0, 0], "hidden behind the box");
    let striped = at(Vec3::new(0.5, 0.0, 4.0));
    // Half white and half black, averaged in linear light, is about 188 in sRGB.
    assert!(striped.iter().all(|&c| (170..=205).contains(&c)), "averaged by its mips: {striped:?}");
    let shown = renderer.ui_panel_sizes().get(&panel).copied().expect("it showed");
    let (left, _) = pixel(Vec3::new(-1.0, 0.0, 4.0));
    let (right, _) = pixel(Vec3::new(1.0, 0.0, 4.0));
    assert!((shown[0] - (right - left) as f32).abs() < 3.0, "as wide as it showed: {shown:?}, {}", right - left);

    renderer.set_ui_panels(&[]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let (x, y) = pixel(Vec3::new(-0.5, -0.25, 4.0));
    let i = ((y * width + x) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "a panel not handed over is gone");
}

/// A two-sided UI Panel seen from behind reads the right way round there:
/// its page's left half is on the viewer's left. One-sided, it shows nothing.
#[test]
fn a_two_sided_ui_panel_reads_the_right_way_round_from_behind() {
    use voxear_render::{UiPanel, UiPanelFrame};
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let (width, height) = (640u32, 360u32);
    let mut world = World::new();
    // Four metres behind the panel, looking back at it along -Z.
    let turned = Quat::from_rotation_y(std::f32::consts::PI);
    world.spawn((Camera::default(), Transform::new(Vec3::new(0.0, 0.0, 8.0), turned, Vec3::ONE)));
    let panel = world.spawn((
        Transform::from_position(Vec3::new(0.0, 0.0, 4.0)),
        UiPanel { size: [2.0, 1.0], ..UiPanel::default() },
    ));
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(500.0).height(500.0).background(Color::hex(0xff0000)));
    ui.panel(root, Style::default().at(500.0, 0.0).width(500.0).height(500.0).background(Color::hex(0x0000ff)));
    let frame = ui.frame(1000, 500, 1.0);
    renderer.set_ui_panels(&[UiPanelFrame { entity: panel, frame: &frame, size: [1000, 500] }]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let at = |x: u32| {
        let i = (((height / 2) * width + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // The panel spans the middle of the view; a quarter of its width either side of the middle is well inside it.
    let quarter = 30;
    assert_eq!(at(width / 2 - quarter), [255, 0, 0], "the page's left, red, on the viewer's left");
    assert_eq!(at(width / 2 + quarter), [0, 0, 255], "and its right, blue, on the right");

    world.get_mut::<UiPanel>(panel).unwrap().two_sided = false;
    run(&mut renderer, &world, &mut 0.0, 1);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let i = (((height / 2) * width + width / 2 - quarter) * 4) as usize;
    assert_ne!(pixels[i..i + 3], [255, 0, 0], "one-sided, nothing from behind");
}

/// A UI Panel seen nearly edge on keeps what runs across its short side
/// sharp: fine stripes up it, as fine as text, still stand apart, not
/// blurred grey by the way the panel is squeezed the other way.
#[test]
fn a_ui_panel_seen_from_the_side_stays_sharp() {
    use voxear_render::{UiPanel, UiPanelFrame};
    use voxear_ui::{Color, Style, Ui};

    let Some(mut renderer) = renderer() else { return };
    let (width, height) = (640u32, 360u32);
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::IDENTITY));
    // Two metres ahead, turned nearly edge on.
    let turned = Quat::from_rotation_y(80f32.to_radians());
    let panel = world.spawn((
        Transform::new(Vec3::new(0.0, 0.0, 2.0), turned, Vec3::ONE),
        UiPanel { size: [1.6, 0.9], ..UiPanel::default() },
    ));
    let mut ui = Ui::new();
    let root = ui.root();
    ui.panel(root, Style::default().at(0.0, 0.0).width(800.0).height(450.0).background(Color::hex(0x000000)));
    for band in (0..450).step_by(16) {
        let white = Style::default().at(0.0, band as f32).width(800.0).height(8.0).background(Color::hex(0xffffff));
        ui.panel(root, white);
    }
    let frame = ui.frame(800, 450, 1.0);
    renderer.set_ui_panels(&[UiPanelFrame { entity: panel, frame: &frame, size: [800, 450] }]).unwrap();
    run(&mut renderer, &world, &mut 0.0, 3);
    let (_, _, pixels) = renderer.read_pixels().unwrap();
    let red = |x: u32, y: u32| pixels[((y * width + x) * 4) as usize];
    // The panel's middle is straight ahead, in the middle of the view; down that column the stripes still swing
    // from dark to light.
    let x = width / 2;
    let column: Vec<u8> = (height / 2 - 30..height / 2 + 30).map(|y| red(x, y)).collect();
    let (low, high) = (column.iter().min().unwrap(), column.iter().max().unwrap());
    assert!(high - low > 150, "the stripes blurred together at column {x}: {low} to {high}");
}
