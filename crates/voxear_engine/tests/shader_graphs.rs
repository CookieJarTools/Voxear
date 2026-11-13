//! Shader graphs against the shaders written by hand that they were made
//! from: the graph Hologram and Sway compiled into the renderer's shaders
//! beside the built-in ones, each drawn on a ball on a real GPU, and the
//! two pictures compared, and the two shaders' cost on the GPU. Skips
//! itself on machines without a usable Vulkan device. With
//! `VOXEAR_TEST_OUTPUT` set, the pictures are kept there.

use std::collections::BTreeSet;

use voxear_assets::AssetGuid;
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::World;
use voxear_engine::{
    default_on_gpu,
    shaders::{BuiltinShader, ShaderRef, ShaderRegistry, dispatch_code, read},
};
use voxear_render::{
    AntiAliasing, AntiAliasingMode, Camera, CustomShaderState, Light, MaterialDesc, MeshRenderer, PostProcess,
    Primitive, Renderer, RendererSettings, Sky,
};
use voxear_scene::Transform;
use voxear_shader_graph::{compile, starters};

const SIZE: (u32, u32) = (640, 360);

fn keep_image(name: &str, pixels: &[u8]) {
    if let Ok(dir) = std::env::var("VOXEAR_TEST_OUTPUT") {
        let file = std::fs::File::create(std::path::Path::new(&dir).join(name)).unwrap();
        let mut encoder = png::Encoder::new(file, SIZE.0, SIZE.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.write_header().unwrap().write_image_data(pixels).unwrap();
    }
}

/// A grey ball over a grey floor in the sun, drawn with `shader` at its
/// default settings at `time`, the camera `back` metres from it; and, with
/// `timed`, the least of sixty frames' GPU time.
fn ball(
    renderer: &mut Renderer,
    registry: &ShaderRegistry,
    shader: ShaderRef,
    time: f32,
    back: f32,
    timed: bool,
) -> (Vec<u8>, f32) {
    let mut world = World::new();
    world.spawn((Camera::default(), Transform::from_position(Vec3::new(0.0, 1.0, -back))));
    let plane = renderer.primitive(Primitive::Plane).unwrap();
    let sphere = renderer.primitive(Primitive::Sphere).unwrap();
    let mut desc = MaterialDesc { base_color: [0.5, 0.5, 0.5, 1.0], ..MaterialDesc::default() };
    let (number, info) = registry.get(shader).expect("the shader is compiled in");
    desc.shader = number;
    desc.max_offset = info.max_offset;
    for (slot, property) in desc.properties.iter_mut().zip(&info.properties) {
        *slot = default_on_gpu(property);
    }
    let material = renderer.add_material(&desc).unwrap();
    let grey =
        renderer.add_material(&MaterialDesc { base_color: [0.3, 0.3, 0.3, 1.0], ..MaterialDesc::default() }).unwrap();
    world.spawn((MeshRenderer::new(plane, grey), Transform::new(Vec3::ZERO, Quat::IDENTITY, Vec3::splat(40.0))));
    world.spawn((MeshRenderer::new(sphere, material), Transform::from_position(Vec3::new(0.0, 1.0, 0.0))));
    let sun = Quat::from_rotation_arc(Vec3::Z, Vec3::new(1.0, -0.5, 0.3).normalize());
    world.spawn((Light::directional(Vec3::ONE, 3.0), Transform::new(Vec3::ZERO, sun, Vec3::ONE)));
    world.insert_resource(Sky { ambient: 0.2, ..Sky::default() });
    let anti_aliasing = AntiAliasing { mode: AntiAliasingMode::Off, sharpness: 0.0, ..AntiAliasing::default() };
    let mut post = PostProcess { anti_aliasing, ..PostProcess::neutral() };
    post.ambient_occlusion.enabled = false;
    post.contact_shadows.enabled = false;
    world.insert_resource(post);
    for _ in 0..6 {
        renderer.render(&world, time).unwrap();
    }
    let pixels = renderer.read_pixels().unwrap().2;
    let mut least = f32::INFINITY;
    for _ in 0..if timed { 60 } else { 0 } {
        renderer.render(&world, time).unwrap();
        least = least.min(renderer.gpu_frame_ms());
    }
    (pixels, least)
}

/// How many pixels differ by more than `by` in any channel.
fn differing(a: &[u8], b: &[u8], by: u8) -> usize {
    a.chunks_exact(4).zip(b.chunks_exact(4)).filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > by)).count()
}

#[test]
fn the_graph_hologram_and_sway_draw_what_the_hand_written_ones_do_at_the_same_cost() {
    let graphs = [starters::hologram(), starters::sway()];
    let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
    let project: Vec<(AssetGuid, String, String)> = graphs
        .iter()
        .zip(1..)
        .map(|(graph, n)| {
            let compiled = compile(graph).unwrap_or_else(|errors| panic!("{}: {errors:?}", graph.name));
            let info = read(&compiled.source).unwrap_or_else(|error| panic!("{error:#}\n{}", compiled.source));
            let hand =
                BuiltinShader::ALL.into_iter().find(|b| b.title() == graph.name).expect("made from a built-in one");
            let built_in = read(hand.source()).unwrap();
            assert_eq!(info.properties, built_in.properties, "{}: the same properties", graph.name);
            assert_eq!((info.unlit, info.max_offset), (built_in.unlit, built_in.max_offset));
            (guid(n), format!("Shaders/{}.shadergraph", graph.name), compiled.source)
        })
        .collect();
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let used: BTreeSet<_> = [BuiltinShader::Hologram, BuiltinShader::Sway].into_iter().collect();
    let registry = ShaderRegistry::new(project, &used);
    renderer.set_custom_shaders(dispatch_code(&registry));
    let world = World::new();
    for _ in 0..1200 {
        let _ = renderer.render(&world, 0.0);
        match renderer.custom_shaders() {
            CustomShaderState::Compiling => std::thread::sleep(std::time::Duration::from_millis(50)),
            CustomShaderState::Failed(error) => panic!("the graphs' shaders did not compile: {error}"),
            _ => break,
        }
    }
    assert_eq!(*renderer.custom_shaders(), CustomShaderState::Ready);

    for (n, built_in) in [(1, BuiltinShader::Hologram), (2, BuiltinShader::Sway)] {
        let name = built_in.name();
        for time in [1.0, 1.4] {
            let (hand, _) = ball(&mut renderer, &registry, ShaderRef::Builtin(built_in), time, 2.4, false);
            let (graph, _) = ball(&mut renderer, &registry, ShaderRef::Asset(guid(n)), time, 2.4, false);
            keep_image(&format!("graph {name} hand {time}.png"), &hand);
            keep_image(&format!("graph {name} graph {time}.png"), &graph);
            let (apart, far_apart) = (differing(&hand, &graph, 2), differing(&hand, &graph, 24));
            let all = (SIZE.0 * SIZE.1) as usize;
            assert!(apart * 100 < all, "{name} at {time}: {apart} pixels differ by more than 2 levels");
            assert!(far_apart * 1000 < all, "{name} at {time}: {far_apart} pixels differ by more than 24 levels");
        }
        // Close up, the ball filling the view, the time the GPU takes for a frame of each.
        // Each timed twice, in turn, so neither has the GPU warmed up for it alone.
        let (mut hand, mut graph) = (f32::INFINITY, f32::INFINITY);
        for _ in 0..2 {
            hand = hand.min(ball(&mut renderer, &registry, ShaderRef::Builtin(built_in), 1.0, 1.3, true).1);
            graph = graph.min(ball(&mut renderer, &registry, ShaderRef::Asset(guid(n)), 1.0, 1.3, true).1);
        }
        println!("{name}: {hand:.3} ms written by hand, {graph:.3} ms from the graph");
        assert!(graph <= hand * 1.1 + 0.02, "{name}: the graph's frame takes {graph} ms against {hand} ms");
    }
}
