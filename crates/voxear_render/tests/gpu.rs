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
    match Renderer::new_offscreen(640, 360, &RendererSettings::default()) {
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
