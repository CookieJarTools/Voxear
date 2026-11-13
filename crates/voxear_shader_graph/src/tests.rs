//! Graphs compiled whole: what the Slang comes to, what the nodes cost, and
//! what stops a graph compiling.

use super::*;

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
    assert!(compile(&g).unwrap_err()[0].message.contains("Sample Picture"));

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
    assert_eq!(sway.source.matches("normalize(").count(), 2, "{}", sway.source);
    // Read back from their files as they were made.
    for graph in [starters::hologram(), starters::sway()] {
        assert_eq!(Graph::from_toml(&graph.to_toml()).unwrap(), graph);
    }
}
