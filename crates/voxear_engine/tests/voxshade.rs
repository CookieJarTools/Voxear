//! VoxShade shaders against the shaders written by hand that they were made
//! from: the graph Hologram and Sway compiled into the renderer's shaders
//! beside the built-in ones, each drawn on a ball on a real GPU, and the
//! two pictures compared, and the two shaders' cost on the GPU. Skips
//! itself on machines without a usable Vulkan device. And a graph with
//! long work on its properties alone, drawn with that work done once on the
//! CPU and with it done at every pixel; and one using a sub-graph and a
//! Slang node against the same work in plain nodes. With
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
use voxear_voxshade::{
    Graph, Input, MathOp, NodeKind, Options, Pin, PropertyKind, Ty, compile, compile_with, starters,
};

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
    if let Some(worked) = registry.worked(shader) {
        voxear_engine::shaders::fill_worked(&mut desc.properties, info.properties.len(), &worked);
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
            // As the project has it: the graph's file, which the registry compiles.
            (guid(n), format!("Shaders/{}.voxshade", graph.name), graph.to_toml())
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

/// A long chain of work on two numbers alone, made brighter or darker by
/// the way the surface faces the camera, and tinted.
fn property_heavy() -> Graph {
    let mut g = Graph::new("Heavy");
    let a = g.add_property("start", PropertyKind::Number, [0.7; 4]);
    let b = g.add_property("scale", PropertyKind::Number, [1.3; 4]);
    let tint = g.add_property("tint", PropertyKind::Color, [0.9, 0.5, 0.2, 0.0]);
    let a = g.add(NodeKind::Property { property: a }, [0.0; 2]);
    let b = g.add(NodeKind::Property { property: b }, [0.0; 2]);
    let tint = g.add(NodeKind::Property { property: tint }, [0.0; 2]);
    let mut x = a;
    for _ in 0..200 {
        let times = g.add(NodeKind::Math { op: MathOp::Multiply }, [0.0; 2]);
        g.link(x, 0, times, 0);
        g.link(b, 0, times, 1);
        let sine = g.add(NodeKind::Math { op: MathOp::Sine }, [0.0; 2]);
        g.link(times, 0, sine, 0);
        x = sine;
    }
    let normal = g.add(NodeKind::Input { input: Input::Normal }, [0.0; 2]);
    let view = g.add(NodeKind::Input { input: Input::View }, [0.0; 2]);
    let facing = g.add(NodeKind::Math { op: MathOp::Dot }, [0.0; 2]);
    g.link(normal, 0, facing, 0);
    g.link(view, 0, facing, 1);
    let lit = g.add(NodeKind::Math { op: MathOp::Multiply }, [0.0; 2]);
    g.link(x, 0, lit, 0);
    g.link(facing, 0, lit, 1);
    let color = g.add(NodeKind::Math { op: MathOp::Multiply }, [0.0; 2]);
    g.link(tint, 0, color, 0);
    g.link(lit, 0, color, 1);
    let out = g.nodes.iter().find(|n| n.kind == NodeKind::SurfaceOutput).unwrap().id;
    g.link(color, 0, out, 0);
    g
}

#[test]
fn work_on_a_graphs_properties_alone_done_once_on_the_cpu_draws_the_same_and_costs_the_gpu_less() {
    let graph = property_heavy();
    let hoisted = compile(&graph).unwrap_or_else(|errors| panic!("{errors:?}"));
    let every_pixel = compile_with(&graph, Options { hoist: false, ..Options::default() }).unwrap();
    assert_eq!(hoisted.derived.types().len(), 1, "the chain's end, handed on: {}", hoisted.source);
    assert_eq!(hoisted.source.matches("sin(").count(), 0, "{}", hoisted.source);
    assert_eq!(every_pixel.source.matches("sin(").count(), 200);
    let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
    let project = vec![
        (guid(1), "Shaders/Heavy.voxshade".to_string(), graph.to_toml()),
        // The same graph compiled with all its work on the GPU, as Slang.
        (guid(2), "Shaders/Heavy everywhere.shader".to_string(), every_pixel.source.clone()),
    ];
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    let registry = ShaderRegistry::new(project, &BTreeSet::new());
    renderer.set_custom_shaders(dispatch_code(&registry));
    let world = World::new();
    for _ in 0..1200 {
        let _ = renderer.render(&world, 0.0);
        match renderer.custom_shaders() {
            CustomShaderState::Compiling => std::thread::sleep(std::time::Duration::from_millis(50)),
            CustomShaderState::Failed(error) => panic!("the shaders did not compile: {error}"),
            _ => break,
        }
    }
    assert_eq!(*renderer.custom_shaders(), CustomShaderState::Ready);
    let (once, _) = ball(&mut renderer, &registry, ShaderRef::Asset(guid(1)), 1.0, 2.4, false);
    let (everywhere, _) = ball(&mut renderer, &registry, ShaderRef::Asset(guid(2)), 1.0, 2.4, false);
    keep_image("heavy once.png", &once);
    keep_image("heavy everywhere.png", &everywhere);
    let all = (SIZE.0 * SIZE.1) as usize;
    // The shadow's soft edge is noisy from one draw to the next; the ball itself matches.
    let (apart, far_apart) = (differing(&once, &everywhere, 2), differing(&once, &everywhere, 24));
    assert!(apart * 100 < all, "{apart} pixels differ by more than 2 levels");
    assert!(far_apart * 1000 < all, "{far_apart} pixels differ by more than 24 levels");
    // Not just both black: the ball is tinted.
    assert!(once.chunks_exact(4).any(|p| p[0] > p[2] + 20), "the ball is drawn tinted");
    let (mut cpu, mut gpu) = (f32::INFINITY, f32::INFINITY);
    for _ in 0..2 {
        cpu = cpu.min(ball(&mut renderer, &registry, ShaderRef::Asset(guid(1)), 1.0, 1.3, true).1);
        gpu = gpu.min(ball(&mut renderer, &registry, ShaderRef::Asset(guid(2)), 1.0, 1.3, true).1);
    }
    println!("Heavy: {cpu:.3} ms with its property work done once, {gpu:.3} ms with it at every pixel");
    assert!(cpu < gpu, "worked out once: {cpu} ms against {gpu} ms");
}

/// A rim of glow: the starter sub-graph's edges, cubed, made a colour by a
/// Slang node, or the same in plain nodes.
fn rim(with_sub_graph: bool, edges: AssetGuid) -> Graph {
    let mut g = starters::lit(if with_sub_graph { "RimNodes" } else { "RimPlain" });
    let out = g.nodes.iter().find(|n| n.kind == NodeKind::SurfaceOutput).unwrap().id;
    let math = |op| NodeKind::Math { op };
    if with_sub_graph {
        let sub = starters::sub_graph("Edges");
        let kind =
            NodeKind::SubGraph { graph: edges, name: sub.name.clone(), inputs: sub.inputs, outputs: sub.outputs };
        let used = g.add(kind, [0.0; 2]);
        g.set_literal(used, 0, [3.0; 4]);
        let code = NodeKind::Code {
            name: "Glow".into(),
            inputs: vec![Pin::new("edge", Ty::Float)],
            outputs: vec![Pin::new("glow", Ty::Float3)],
            code: "glow = float3(edge, edge * 0.5, 0.0) * 2.0;".into(),
        };
        let glow = g.add(code, [0.0; 2]);
        g.link(used, 0, glow, 0);
        g.link(glow, 0, out, 5);
    } else {
        let normal = g.add(NodeKind::Input { input: Input::Normal }, [0.0; 2]);
        let view = g.add(NodeKind::Input { input: Input::View }, [0.0; 2]);
        let dot = g.add(math(MathOp::Dot), [0.0; 2]);
        let abs = g.add(math(MathOp::Absolute), [0.0; 2]);
        let away = g.add(math(MathOp::OneMinus), [0.0; 2]);
        let power = g.add(math(MathOp::Power), [0.0; 2]);
        let half = g.add(math(MathOp::Multiply), [0.0; 2]);
        let combine = g.add(NodeKind::Combine { ty: Ty::Float3 }, [0.0; 2]);
        let twice = g.add(math(MathOp::Multiply), [0.0; 2]);
        g.link(normal, 0, dot, 0);
        g.link(view, 0, dot, 1);
        g.link(dot, 0, abs, 0);
        g.link(abs, 0, away, 0);
        g.link(away, 0, power, 0);
        g.set_literal(power, 1, [3.0; 4]);
        g.link(power, 0, half, 0);
        g.set_literal(half, 1, [0.5; 4]);
        g.link(power, 0, combine, 0);
        g.link(half, 0, combine, 1);
        g.link(combine, 0, twice, 0);
        g.set_literal(twice, 1, [2.0; 4]);
        g.link(twice, 0, out, 5);
    }
    g
}

#[test]
fn a_graph_with_a_sub_graph_and_a_slang_node_draws_what_the_same_in_plain_nodes_does() {
    let guid = |n: u32| format!("{n:032x}").parse::<AssetGuid>().unwrap();
    let project = vec![
        (guid(1), "Shaders/Edges.voxshade".to_string(), starters::sub_graph("Edges").to_toml()),
        (guid(2), "Shaders/RimNodes.voxshade".to_string(), rim(true, guid(1)).to_toml()),
        (guid(3), "Shaders/RimPlain.voxshade".to_string(), rim(false, guid(1)).to_toml()),
    ];
    let registry = ShaderRegistry::new(project, &BTreeSet::new());
    for shader in registry.project_shaders() {
        assert!(shader.info.is_ok(), "{}: {:?}", shader.path, shader.info);
    }
    assert_eq!(registry.project_shaders().count(), 2, "the sub-graph is no shader");
    let Ok(mut renderer) = Renderer::new_offscreen(SIZE.0, SIZE.1, &RendererSettings::default()) else {
        eprintln!("skipping: no usable GPU");
        return;
    };
    renderer.set_custom_shaders(dispatch_code(&registry));
    let world = World::new();
    for _ in 0..1200 {
        let _ = renderer.render(&world, 0.0);
        match renderer.custom_shaders() {
            CustomShaderState::Compiling => std::thread::sleep(std::time::Duration::from_millis(50)),
            CustomShaderState::Failed(error) => panic!("the shaders did not compile: {error}"),
            _ => break,
        }
    }
    assert_eq!(*renderer.custom_shaders(), CustomShaderState::Ready);
    let (nodes, _) = ball(&mut renderer, &registry, ShaderRef::Asset(guid(2)), 1.0, 2.4, false);
    let (plain, _) = ball(&mut renderer, &registry, ShaderRef::Asset(guid(3)), 1.0, 2.4, false);
    keep_image("rim nodes.png", &nodes);
    keep_image("rim plain.png", &plain);
    let all = (SIZE.0 * SIZE.1) as usize;
    let (apart, far_apart) = (differing(&nodes, &plain, 2), differing(&nodes, &plain, 24));
    assert!(apart * 100 < all, "{apart} pixels differ by more than 2 levels");
    assert!(far_apart * 1000 < all, "{far_apart} pixels differ by more than 24 levels");
    // The rim glows orange: somewhere on the ball red is well over blue.
    assert!(nodes.chunks_exact(4).any(|p| i32::from(p[0]) > i32::from(p[2]) + 60), "the rim glows");
}
