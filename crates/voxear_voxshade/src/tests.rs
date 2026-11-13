//! Graphs compiled whole: what the Slang comes to, what the nodes cost, and
//! what stops a graph compiling.

use super::*;
use crate::nodes::Pin;

/// A graph with a Surface Output (node 1) and these nodes, wired by `links`.
fn graph(nodes: Vec<NodeKind>) -> (Graph, Vec<NodeId>) {
    let mut graph = Graph::new("Test");
    let ids = nodes.into_iter().map(|kind| graph.add(kind, [0.0, 0.0])).collect();
    (graph, ids)
}

const SURFACE: NodeId = NodeId(1);
const COLOR: usize = 0;
const ALPHA: usize = 1;

fn compiled(graph: &Graph) -> Compiled {
    compile(graph).unwrap_or_else(|errors| panic!("{errors:?}"))
}

fn math(op: MathOp) -> NodeKind {
    NodeKind::Math { op }
}

#[test]
fn numbers_known_when_compiling_are_written_as_what_they_come_to_and_cost_nothing() {
    let (mut g, ids) = graph(vec![
        NodeKind::Constant { ty: Ty::Float, value: [2.0; 4] },
        NodeKind::Constant { ty: Ty::Float, value: [3.0; 4] },
        math(MathOp::Multiply),
        math(MathOp::Sine),
    ]);
    g.link(ids[0], 0, ids[2], 0);
    g.link(ids[1], 0, ids[2], 1);
    g.link(ids[2], 0, ids[3], 0);
    g.link(ids[3], 0, SURFACE, ALPHA);
    let out = compiled(&g);
    let alpha = format!("surface.alpha = {:?};", 6.0f32.sin());
    assert!(out.source.contains(&alpha), "{}", out.source);
    assert!(!out.source.contains("float v"), "no work left to do: {}", out.source);
    assert_eq!(out.stages, [(Stage::Surface, StageCost::default())]);
    assert!(out.nodes.values().all(|cost| *cost == NodeCost::default()));
}

#[test]
fn two_nodes_doing_the_same_work_do_it_once_and_a_node_no_output_reaches_is_left_out() {
    let (mut g, ids) = graph(vec![
        NodeKind::Input { input: Input::Uv },
        NodeKind::Split,
        math(MathOp::Sine),
        math(MathOp::Sine),
        math(MathOp::Add),
        math(MathOp::Cosine),
    ]);
    g.link(ids[0], 0, ids[1], 0);
    g.link(ids[1], 0, ids[2], 0);
    g.link(ids[1], 0, ids[3], 0);
    g.link(ids[2], 0, ids[4], 0);
    g.link(ids[3], 0, ids[4], 1);
    g.link(ids[1], 1, ids[5], 0);
    g.link(ids[4], 0, SURFACE, ALPHA);
    let out = compiled(&g);
    assert_eq!(out.source.matches("sin(").count(), 1, "{}", out.source);
    assert!(!out.source.contains("cos("), "nothing reads the cosine");
    assert_eq!(out.nodes[&ids[2]].alu, 4, "the first sine counts the work");
    assert!(out.nodes.get(&ids[3]).is_none_or(|cost| cost.alu == 0), "the second shares it");
    assert!(out.nodes.get(&ids[5]).is_none(), "the cosine is not compiled at all");
    assert!(out.source.contains("input.uv.x"), "{}", out.source);
}

#[test]
fn a_branch_worth_skipping_is_a_real_branch_with_its_side_inside_it() {
    let mut g = Graph::new("Test");
    let switch = g.add_property("useMap", PropertyKind::Number, [1.0; 4]);
    let map = g.add_property("map", PropertyKind::Picture, [0.0; 4]);
    let on = g.add(NodeKind::Property { property: switch }, [0.0, 0.0]);
    let compare = g.add(NodeKind::Compare { op: CompareOp::Greater }, [0.0, 0.0]);
    let uv = g.add(NodeKind::Input { input: Input::Uv }, [0.0, 0.0]);
    let read = g.add(NodeKind::SamplePicture { property: map }, [0.0, 0.0]);
    let branch = g.add(NodeKind::Branch, [0.0, 0.0]);
    g.link(on, 0, compare, 0);
    g.link(uv, 0, read, 0);
    g.link(compare, 0, branch, 0);
    g.link(read, 1, branch, 1);
    g.link(branch, 0, SURFACE, COLOR);
    let out = compiled(&g);
    let source = &out.source;
    let branch_at = source.find("if (").unwrap_or_else(|| panic!("a real branch: {source}"));
    let sample_at = source.find(".sample(").expect("the picture is read");
    assert!(sample_at > branch_at, "read only when the branch is taken: {source}");
    assert_eq!(out.stages[0].1.branches, 1);
    assert_eq!(out.stages[0].1.samples, 1);

    // With nothing worth skipping on either side, and a condition that changes from pixel to pixel, a select.
    let mut cheap = Graph::new("Test");
    let uv = cheap.add(NodeKind::Input { input: Input::Uv }, [0.0, 0.0]);
    let split = cheap.add(NodeKind::Split, [0.0, 0.0]);
    let compare = cheap.add(NodeKind::Compare { op: CompareOp::Less }, [0.0, 0.0]);
    let branch = cheap.add(NodeKind::Branch, [0.0, 0.0]);
    cheap.link(uv, 0, split, 0);
    cheap.link(split, 0, compare, 0);
    cheap.set_literal(compare, 1, [0.5; 4]);
    cheap.link(compare, 0, branch, 0);
    cheap.link(split, 1, branch, 1);
    cheap.link(branch, 0, cheap.nodes[0].id, ALPHA);
    let out = compiled(&cheap);
    assert!(!out.source.contains("if (") && out.source.contains(" ? "), "{}", out.source);
}

#[test]
fn a_colour_is_worked_out_in_light_and_a_random_number_is_the_same_on_the_cpu_and_the_gpu() {
    let (mut g, ids) = graph(vec![NodeKind::Color { value: [0.5, 1.0, 0.0, 1.0] }, NodeKind::Random]);
    g.link(ids[0], 0, SURFACE, COLOR);
    g.set_literal(ids[1], 0, [3.5; 4]);
    g.link(ids[1], 0, SURFACE, ALPHA);
    let out = compiled(&g);
    assert!(out.source.contains("surface.color = float3(0.21404114, 1.0, 0.0);"), "{}", out.source);
    assert!(out.source.contains(&format!("surface.alpha = {:?};", ir::random(3.5))), "{}", out.source);
    // Wired to something that changes, the shader works it out, with the same hash.
    let time = g.add(NodeKind::Input { input: Input::Time }, [0.0, 0.0]);
    g.link(time, 0, ids[1], 0);
    let out = compiled(&g);
    assert!(out.source.contains("float graphRandom(float seed)") && out.source.contains("graphRandom(input.time)"));
}

#[test]
fn what_stops_a_graph_compiling_is_said_at_the_node() {
    let (mut g, ids) = graph(vec![NodeKind::Input { input: Input::LightDirection }]);
    g.link(ids[0], 0, SURFACE, COLOR);
    let errors = compile(&g).unwrap_err();
    assert_eq!(errors[0].node, Some(ids[0]));
    assert!(errors[0].message.contains("Surface"), "{}", errors[0].message);

    let (mut g, ids) = graph(vec![NodeKind::Compare { op: CompareOp::Less }]);
    g.link(ids[0], 0, SURFACE, ALPHA);
    assert!(compile(&g).unwrap_err()[0].message.contains("true or false"));

    let (mut g, ids) = graph(vec![math(MathOp::Add), math(MathOp::Add)]);
    g.link(ids[0], 0, ids[1], 0);
    g.link(ids[1], 0, ids[0], 0);
    g.link(ids[1], 0, SURFACE, ALPHA);
    assert!(compile(&g).unwrap_err()[0].message.contains("loop"));

    let mut g = Graph::new("Test");
    let map = g.add_property("map", PropertyKind::Picture, [0.0; 4]);
    let read = g.add(NodeKind::Property { property: map }, [0.0, 0.0]);
    g.link(read, 0, SURFACE, ALPHA);
    assert!(compile(&g).unwrap_err()[0].message.contains("wire it into a Sample"));

    let empty = Graph::new("Test");
    assert!(compile(&empty).unwrap_err()[0].message.contains("output"), "nothing to draw");
}

#[test]
fn the_starters_compile_to_shaders_shaped_as_hand_written_ones() {
    let hologram = compiled(&starters::hologram());
    let source = &hologram.source;
    assert!(source.contains("[Unlit]\nstruct Hologram\n{"), "{source}");
    assert!(source.contains("[Property, Color] float3 tint = float3(0.2, 0.75, 1.0);"), "{source}");
    assert!(source.contains("] Picture pattern;"), "{source}");
    assert!(source.contains("void surface(Hologram props, SurfaceInput input, inout SurfaceOutput surface)"));
    assert!(source.contains("surface.emission = float3(0.0, 0.0, 0.0);"), "{source}");
    assert_eq!(hologram.stages[0].1.samples, 3, "the pattern from three sides");
    assert_eq!(hologram.stages[0].1.branches, 1, "the lines only when it has any");
    let sway = compiled(&starters::sway());
    assert!(sway.source.contains("[MaxOffset(2.1)]\nstruct Sway"), "{}", sway.source);
    assert!(sway.source.contains("float3 vertex(Sway props, VertexInput input)"), "{}", sway.source);
    let unhoisted = compile_with(&starters::sway(), Options { hoist: false, ..Options::default() }).unwrap();
    assert_eq!(unhoisted.source.matches("normalize(").count(), 2, "{}", unhoisted.source);
    assert!(unhoisted.derived.is_empty() && !unhoisted.source.contains("worked0"));
    // Read back from their files as they were made.
    assert!(compiled(&starters::lit("Plain")).source.contains("surface.color = "));
    for graph in [starters::hologram(), starters::sway(), starters::lit("Plain")] {
        assert_eq!(Graph::from_toml(&graph.to_toml()).unwrap(), graph);
    }
}

#[test]
fn work_the_properties_alone_decide_is_done_once_on_the_cpu_and_handed_to_the_shader() {
    let sway = compiled(&starters::sway());
    // Which way it sways and the way along it, each made a unit long, and how far it reaches.
    assert_eq!(sway.source.matches("normalize(").count(), 0, "{}", sway.source);
    assert_eq!(sway.derived.types(), vec![Ty::Float3, Ty::Float, Ty::Float3, Ty::Float3]);
    assert!(sway.source.contains("    float3 worked0;\n    float worked1;\n"), "{}", sway.source);
    assert!(sway.source.contains("props.worked2"), "{}", sway.source);
    let defaults: Vec<[f32; 4]> = starters::sway().properties.iter().map(|p| p.default).collect();
    let values = sway.derived.values(&defaults);
    let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(&b).all(|(a, b)| (a - b).abs() < 1e-5);
    assert!(values.iter().any(|v| near(*v, [0.0, 1.0, 0.0, 0.0])), "the reach, a unit long: {values:?}");
    assert!(values.iter().any(|v| near(*v, [1.0, 0.0, 0.0, 0.0])), "the way it sways: {values:?}");
    // Its own properties changed, what is worked out from them changes with them.
    let mut turned = defaults.clone();
    let direction = starters::sway().properties.iter().position(|p| p.name == "direction").unwrap();
    turned[direction] = [0.0, 0.0, 3.0, 0.0];
    assert!(sway.derived.values(&turned).iter().any(|v| near(*v, [0.0, 0.0, 1.0, 0.0])));
    // What is moved off the GPU is counted apart.
    let (gpu, cpu) = (
        &sway.stages[0].1,
        compile_with(&starters::sway(), Options { hoist: false, ..Options::default() }).unwrap().stages[0].1.clone(),
    );
    assert!(gpu.alu < cpu.alu && gpu.once > 0, "{gpu:?} against {cpu:?}");
    assert_eq!(gpu.alu + gpu.once, cpu.alu + cpu.once);
}

#[test]
fn a_material_with_no_room_left_works_everything_out_on_the_gpu() {
    let mut g = Graph::new("Full");
    let mut last = None;
    for n in 0..hoist::SLOTS {
        let p = g.add_property(&format!("p{n}"), PropertyKind::Number, [1.0; 4]);
        let read = g.add(NodeKind::Property { property: p }, [0.0, 0.0]);
        last = Some(read);
    }
    let sine = g.add(math(MathOp::Sine), [0.0, 0.0]);
    g.link(last.unwrap(), 0, sine, 0);
    g.link(sine, 0, SURFACE, ALPHA);
    let full = compiled(&g);
    assert!(full.derived.is_empty() && full.source.contains("sin("), "{}", full.source);
}

fn ripple() -> NodeKind {
    NodeKind::Code {
        name: "Ripple".into(),
        inputs: vec![Pin::new("x", Ty::Float)],
        outputs: vec![Pin::new("y", Ty::Float), Pin::new("lit", Ty::Float3)],
        code: "y = sin(x * 3.0);\nlit = float3(y, y * 0.5, 0.0);".into(),
    }
}

#[test]
fn a_slang_node_is_a_function_the_shader_declares_once_and_calls_once_for_the_same_inputs() {
    let (mut g, ids) = graph(vec![NodeKind::Input { input: Input::Uv }, NodeKind::Split, ripple(), ripple()]);
    g.link(ids[0], 0, ids[1], 0);
    g.link(ids[1], 0, ids[2], 0);
    g.link(ids[1], 0, ids[3], 0);
    g.link(ids[2], 0, SURFACE, ALPHA);
    g.link(ids[3], 1, SURFACE, COLOR);
    let out = compiled(&g);
    let source = &out.source;
    assert_eq!(source.matches("void Ripple_").count(), 1, "{source}");
    assert!(source.contains("(float x, out float y, out float3 lit)\n{\n    y = sin(x * 3.0);\n"), "{source}");
    assert_eq!(source.matches("Ripple_").count(), 2, "declared once and called once: {source}");
    assert!(source.contains("surface.color = v") && source.contains("_1;"), "its second output: {source}");
    assert!(out.nodes.values().any(|cost| cost.alu > 0), "a guess at what it costs");
    assert_eq!(Graph::from_toml(&g.to_toml()).unwrap(), g);
}

#[test]
fn a_slang_node_is_never_worked_out_on_the_cpu_and_its_names_are_checked() {
    let mut g = Graph::new("Test");
    let p = g.add_property("amount", PropertyKind::Number, [0.5; 4]);
    let read = g.add(NodeKind::Property { property: p }, [0.0; 2]);
    let code = g.add(ripple(), [0.0; 2]);
    let normal = g.add(NodeKind::Input { input: Input::Normal }, [0.0; 2]);
    let times = g.add(math(MathOp::Multiply), [0.0; 2]);
    g.link(read, 0, code, 0);
    g.link(code, 1, times, 0);
    g.link(normal, 0, times, 1);
    g.link(times, 0, SURFACE, COLOR);
    let out = compiled(&g);
    assert!(out.derived.is_empty() && out.source.contains("Ripple_"), "{}", out.source);
    let mut wrong = g.clone();
    if let NodeKind::Code { outputs, .. } = &mut wrong.nodes.iter_mut().find(|n| n.id == code).unwrap().kind {
        outputs[1].name = "y".into();
    }
    let errors = compile(&wrong).unwrap_err();
    assert_eq!(errors[0].node, Some(code));
    assert!(errors[0].message.contains("name of its own"), "{}", errors[0].message);
}

/// A sub-graph: how much a surface faces away from the camera, to a power.
fn fresnel() -> Graph {
    let mut sub = Graph::new_sub_graph("Fresnel");
    sub.outputs[0].ty = Ty::Float;
    let power = sub.add_input("power", Ty::Float);
    sub.inputs[0].default = [2.0; 4];
    let normal = sub.add(NodeKind::Input { input: Input::Normal }, [0.0; 2]);
    let view = sub.add(NodeKind::Input { input: Input::View }, [0.0; 2]);
    let dot = sub.add(math(MathOp::Dot), [0.0; 2]);
    let away = sub.add(math(MathOp::OneMinus), [0.0; 2]);
    let raised = sub.add(math(MathOp::Power), [0.0; 2]);
    let input = sub.add(NodeKind::SubGraphInput { input: power }, [0.0; 2]);
    sub.link(normal, 0, dot, 0);
    sub.link(view, 0, dot, 1);
    sub.link(dot, 0, away, 0);
    sub.link(away, 0, raised, 0);
    sub.link(input, 0, raised, 1);
    let out = sub.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput).unwrap().id;
    sub.link(raised, 0, out, 0);
    sub
}

fn guid(n: u128) -> voxear_assets::AssetGuid {
    format!("{n:032x}").parse().unwrap()
}

fn uses(sub: &Graph, at: voxear_assets::AssetGuid) -> NodeKind {
    NodeKind::SubGraph { graph: at, name: sub.name.clone(), inputs: sub.inputs.clone(), outputs: sub.outputs.clone() }
}

#[test]
fn a_sub_graph_is_worked_in_where_it_is_used_and_costs_what_its_nodes_would() {
    let sub = fresnel();
    let library: HashMap<_, _> = [(guid(7), sub.clone())].into();
    let options = Options { library: Some(&library), ..Options::default() };
    // Used twice with the same input: its work is done once.
    let (mut g, ids) = graph(vec![uses(&sub, guid(7)), uses(&sub, guid(7)), math(MathOp::Add)]);
    g.set_literal(ids[0], 0, [3.0; 4]);
    g.set_literal(ids[1], 0, [3.0; 4]);
    g.link(ids[0], 0, ids[2], 0);
    g.link(ids[1], 0, ids[2], 1);
    g.link(ids[2], 0, SURFACE, ALPHA);
    let out = compile_with(&g, options).unwrap();
    assert_eq!(out.source.matches("pow(").count(), 1, "{}", out.source);
    assert!(out.source.contains("3.0") && !out.source.contains("Fresnel"), "{}", out.source);
    assert!(out.nodes[&ids[0]].alu > 0, "its work counted against the node that uses it");
    // The same nodes put in the graph itself cost the same.
    let (mut flat, f) = graph(vec![
        NodeKind::Input { input: Input::Normal },
        NodeKind::Input { input: Input::View },
        math(MathOp::Dot),
        math(MathOp::OneMinus),
        math(MathOp::Power),
        math(MathOp::Add),
    ]);
    flat.link(f[0], 0, f[2], 0);
    flat.link(f[1], 0, f[2], 1);
    flat.link(f[2], 0, f[3], 0);
    flat.link(f[3], 0, f[4], 0);
    flat.set_literal(f[4], 1, [3.0; 4]);
    flat.link(f[4], 0, f[5], 0);
    flat.link(f[4], 0, f[5], 1);
    flat.link(f[5], 0, SURFACE, ALPHA);
    assert_eq!(out.stages, compiled(&flat).stages);
    let (mut plain, p) = graph(vec![uses(&sub, guid(7))]);
    plain.link(p[0], 0, SURFACE, ALPHA);
    // Its input left alone takes the sub-graph's own default, 2, a power made a product.
    let source = compile_with(&plain, options).unwrap().source;
    assert!(!source.contains("pow(") && source.contains(" * "), "{source}");
    assert_eq!(Graph::from_toml(&g.to_toml()).unwrap(), g);
    assert_eq!(Graph::from_toml(&sub.to_toml()).unwrap(), sub);
}

#[test]
fn a_sub_graph_missing_used_inside_itself_or_with_properties_says_so_at_its_node() {
    let sub = fresnel();
    let (mut g, ids) = graph(vec![uses(&sub, guid(7))]);
    g.link(ids[0], 0, SURFACE, ALPHA);
    let errors = compile(&g).unwrap_err();
    assert_eq!(errors[0].node, Some(ids[0]));
    assert!(errors[0].message.contains("not in the project"), "{}", errors[0].message);
    // A sub-graph that uses itself.
    let mut looped = fresnel();
    let inner = looped.add(uses(&sub, guid(8)), [0.0; 2]);
    let out = looped.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput).unwrap().id;
    looped.link(inner, 0, out, 0);
    let library: HashMap<_, _> = [(guid(8), looped.clone())].into();
    let (mut g, ids) = graph(vec![uses(&looped, guid(8))]);
    g.link(ids[0], 0, SURFACE, ALPHA);
    let errors = compile_with(&g, Options { library: Some(&library), ..Options::default() }).unwrap_err();
    assert!(errors[0].message.contains("inside itself"), "{}", errors[0].message);
    // Properties belong to shaders.
    let mut owning = fresnel();
    let p = owning.add_property("amount", PropertyKind::Number, [1.0; 4]);
    let read = owning.add(NodeKind::Property { property: p }, [0.0; 2]);
    let out = owning.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput).unwrap().id;
    owning.link(read, 0, out, 0);
    assert!(compile(&owning).unwrap_err()[0].message.contains("no properties"));
}

#[test]
fn a_sub_graph_compiled_on_its_own_is_a_function_of_its_inputs() {
    let out = compiled(&fresnel());
    assert!(
        out.source.contains("void Fresnel(SurfaceInput input, SurfaceOutput surface, float power, out float result)"),
        "{}",
        out.source
    );
    assert!(out.source.contains("pow(") && out.source.contains("    result = v"), "{}", out.source);
    assert_eq!(out.stages[0].0, Stage::Surface);
}

#[test]
fn a_particle_graph_compiles_to_an_update_and_a_look_with_its_properties_as_numbers() {
    let out = compiled(&starters::particles("Swirl"));
    let source = &out.source;
    assert!(
        source.contains("void particleUpdate(ParticleUpdateInput input, inout ParticleUpdateOutput output)"),
        "{source}"
    );
    assert!(source.contains("void particleLook(ParticleLookInput input, inout ParticleLookOutput output)"), "{source}");
    assert!(
        source.contains("cross(") && source.contains("input.objectPosition") && source.contains("    output.force = ")
    );
    assert!(source.contains("input.life") && source.contains("output.alpha = ") && source.contains("output.size = "));
    assert!(
        !source.contains("output.velocity") && !source.contains("output.color"),
        "unwired, kept as the system has them"
    );
    assert!(!source.contains("struct") && !source.contains("props."), "its swirl worked in as a number: {source}");
    assert!(source.contains("2.0"), "{source}");
    assert_eq!(out.stages.iter().map(|(stage, _)| *stage).collect::<Vec<_>>(), [Stage::Update, Stage::Look]);
    let graph = starters::particles("Swirl");
    assert_eq!(Graph::from_toml(&graph.to_toml()).unwrap(), graph);
}

#[test]
fn a_particle_graph_takes_only_particle_outputs_and_inputs() {
    // A Surface Output in a particle graph, and an Update Output in a shader's.
    let mut particles = starters::particles("Swirl");
    let surface = particles.add(NodeKind::SurfaceOutput, [0.0; 2]);
    let errors = compile(&particles).unwrap_err();
    assert_eq!(errors[0].node, Some(surface));
    assert!(errors[0].message.contains("draws no surface"), "{}", errors[0].message);
    let mut shader = Graph::new("Test");
    let update = shader.add(NodeKind::UpdateOutput, [0.0; 2]);
    assert_eq!(compile(&shader).unwrap_err()[0].node, Some(update));
    // A material's inputs are not a particle's, nor a particle's in a shader.
    let mut uv = starters::particles("Swirl");
    let read = uv.add(NodeKind::Input { input: Input::Uv }, [0.0; 2]);
    let look = uv.nodes.iter().find(|n| n.kind == NodeKind::LookOutput).unwrap().id;
    uv.link(read, 0, look, 2);
    assert!(compile(&uv).unwrap_err()[0].message.contains("not there in the Look stage"));
    let (mut g, ids) = graph(vec![NodeKind::Input { input: Input::Age }]);
    g.link(ids[0], 0, SURFACE, ALPHA);
    assert!(compile(&g).unwrap_err()[0].message.contains("Surface stage"));
    // Delta Time only where the particles are stepped.
    let mut step = starters::particles("Swirl");
    let delta = step.add(NodeKind::Input { input: Input::DeltaTime }, [0.0; 2]);
    step.link(delta, 0, look, 2);
    assert!(compile(&step).unwrap_err()[0].message.contains("Look"));
    // A picture, read where its particles are drawn, from the system's room for three.
    let mut pictured = starters::particles("Swirl");
    let map = pictured.add_property("map", PropertyKind::Picture, [0.0; 4]);
    let read = pictured.add(NodeKind::Property { property: map }, [0.0; 2]);
    let sample = pictured.add(NodeKind::Sample, [0.0; 2]);
    pictured.link(read, 0, sample, 0);
    pictured.link(sample, 2, look, 2);
    let source = compiled(&pictured).source;
    assert!(source.contains("particleGraphPicture(input.pictures.x, float2(0.0, 0.0))"), "{source}");
    for n in 0..3 {
        pictured.add_property(&format!("more{n}"), PropertyKind::Picture, [0.0; 4]);
    }
    assert!(compile(&pictured).unwrap_err()[0].message.contains("3 picture properties at most"));
}

/// A sub-graph that reads the picture handed in at its input `pattern` at the UV handed in.
fn read_pattern() -> Graph {
    let mut sub = Graph::new_sub_graph("ReadPattern");
    sub.outputs[0].ty = Ty::Float;
    let pattern = sub.add_input("pattern", Ty::Float);
    sub.inputs[0].picture = true;
    let at = sub.add_input("at", Ty::Float2);
    let picture = sub.add(NodeKind::SubGraphInput { input: pattern }, [0.0; 2]);
    let uv = sub.add(NodeKind::SubGraphInput { input: at }, [0.0; 2]);
    let sample = sub.add(NodeKind::Sample, [0.0; 2]);
    sub.link(picture, 0, sample, 0);
    sub.link(uv, 0, sample, 1);
    let out = sub.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput).unwrap().id;
    sub.link(sample, 2, out, 0);
    sub
}

#[test]
fn a_picture_goes_by_wire_into_a_sample_and_into_a_sub_graph() {
    // A picture property's node gives the picture, which a Sample reads.
    let mut g = Graph::new("Test");
    let map = g.add_property("map", PropertyKind::Picture, [0.0; 4]);
    let read = g.add(NodeKind::Property { property: map }, [0.0; 2]);
    let uv = g.add(NodeKind::Input { input: Input::Uv }, [0.0; 2]);
    let sample = g.add(NodeKind::Sample, [0.0; 2]);
    g.link(read, 0, sample, 0);
    g.link(uv, 0, sample, 1);
    g.link(sample, 2, SURFACE, ALPHA);
    let source = compiled(&g).source;
    assert!(source.contains("props.map.sample(input.uv)"), "{source}");
    // Into a sub-graph and read there, as if read here.
    let sub = read_pattern();
    let library: HashMap<_, _> = [(guid(4), sub.clone())].into();
    let mut using = Graph::new("Test");
    let map = using.add_property("map", PropertyKind::Picture, [0.0; 4]);
    let read = using.add(NodeKind::Property { property: map }, [0.0; 2]);
    let uv = using.add(NodeKind::Input { input: Input::Uv }, [0.0; 2]);
    let used = using.add(uses(&sub, guid(4)), [0.0; 2]);
    using.link(read, 0, used, 0);
    using.link(uv, 0, used, 1);
    using.link(used, 0, SURFACE, ALPHA);
    let options = Options { library: Some(&library), ..Options::default() };
    let source = compile_with(&using, options).unwrap().source;
    assert!(source.contains("props.map.sample(input.uv)"), "{source}");
    assert_eq!(compile_with(&using, options).unwrap().stages, compiled(&g).stages, "the same work");
    // Nothing wired into it, or numbers: said at the node.
    let mut bare = using.clone();
    bare.links.retain(|l| !(l.to == used && l.input == 0));
    let wrong = &compile_with(&bare, options).unwrap_err()[0];
    assert!(wrong.node == Some(used) && wrong.message.contains("wire a picture into its pattern"), "{wrong:?}");
    let mut numbers = bare.clone();
    numbers.link(uv, 0, used, 0);
    assert!(compile_with(&numbers, options).unwrap_err()[0].message.contains("only a picture goes into its pattern"));
    // On its own, a function of a picture.
    let alone = compiled(&sub).source;
    assert!(
        alone.contains("Picture pattern, float2 at, out float result") && alone.contains("pattern.sample(at)"),
        "{alone}"
    );
    assert_eq!(Graph::from_toml(&sub.to_toml()).unwrap(), sub);
}
