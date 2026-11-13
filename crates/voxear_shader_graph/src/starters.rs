//! Graphs every project can start a shader from: some of the engine's
//! built-in shaders made again of nodes, which draw what the hand-written
//! ones draw.

use crate::{
    graph::{Graph, NodeId, PropertyId, PropertyKind},
    nodes::{CompareOp, Component, Input, MathOp, NodeKind},
    types::Ty,
};

/// What goes into a port while a starter is put together: a node's first
/// output, another of its outputs, or numbers kept on the port.
#[derive(Clone, Copy)]
enum In {
    Wire(NodeId),
    Out(NodeId, usize),
    Num(f32),
    Vec3([f32; 3]),
}

use In::{Num, Out, Vec3, Wire};

/// Puts a graph together a node at a time, then lays the nodes out in
/// columns by how far they are from the graph's inputs.
struct Make {
    graph: Graph,
}

impl Make {
    fn new(name: &str) -> Self {
        Self { graph: Graph { name: name.into(), ..Graph::default() } }
    }

    /// A property, and a node that reads it.
    fn property(
        &mut self,
        name: &str,
        kind: PropertyKind,
        default: [f32; 4],
        range: Option<[f32; 2]>,
        tooltip: &str,
    ) -> NodeId {
        let id = self.declare(name, kind, default, range, tooltip);
        self.graph.add(NodeKind::Property { property: id }, [0.0, 0.0])
    }

    fn declare(
        &mut self,
        name: &str,
        kind: PropertyKind,
        default: [f32; 4],
        range: Option<[f32; 2]>,
        tooltip: &str,
    ) -> PropertyId {
        let id = self.graph.add_property(name, kind, default);
        if let Some(property) = self.graph.properties.iter_mut().find(|p| p.id == id) {
            (property.range, property.tooltip) = (range, tooltip.into());
        }
        id
    }

    fn node(&mut self, kind: NodeKind, inputs: &[In]) -> NodeId {
        let node = self.graph.add(kind, [0.0, 0.0]);
        for (index, input) in inputs.iter().enumerate() {
            match *input {
                Wire(from) => self.graph.link(from, 0, node, index),
                Out(from, output) => self.graph.link(from, output, node, index),
                Num(n) => self.graph.set_literal(node, index, [n; 4]),
                Vec3([x, y, z]) => self.graph.set_literal(node, index, [x, y, z, 0.0]),
            }
        }
        node
    }

    fn math(&mut self, op: MathOp, inputs: &[In]) -> NodeId {
        self.node(NodeKind::Math { op }, inputs)
    }

    fn input(&mut self, input: Input) -> NodeId {
        self.node(NodeKind::Input { input }, &[])
    }

    /// A Vector 3 of these numbers, as a node.
    fn vector(&mut self, value: [f32; 3]) -> NodeId {
        self.node(NodeKind::Constant { ty: Ty::Float3, value: [value[0], value[1], value[2], 0.0] }, &[])
    }

    fn finish(mut self) -> Graph {
        // Each node a column to the right of what feeds it, the outputs last.
        let mut column: std::collections::HashMap<NodeId, usize> = std::collections::HashMap::new();
        for _ in 0..self.graph.nodes.len() {
            for link in &self.graph.links {
                let next = column.get(&link.from).copied().unwrap_or(0) + 1;
                let at = column.entry(link.to).or_insert(0);
                *at = (*at).max(next);
            }
        }
        let mut rows = std::collections::HashMap::new();
        for node in &mut self.graph.nodes {
            let x = column.get(&node.id).copied().unwrap_or(0);
            let y = rows.entry(x).or_insert(0usize);
            node.at = [x as f32 * 220.0, *y as f32 * 140.0];
            *y += 1;
        }
        self.graph
    }
}

/// `frac(sin(n * 12.9898) * 43758.5453)`, the hand-written shaders' hash of a number.
fn sine_hash(make: &mut Make, n: In) -> NodeId {
    let scaled = make.math(MathOp::Multiply, &[n, Num(12.9898)]);
    let sine = make.math(MathOp::Sine, &[Wire(scaled)]);
    let spread = make.math(MathOp::Multiply, &[Wire(sine), Num(43758.5453)]);
    make.math(MathOp::Fraction, &[Wire(spread)])
}

/// A figure of light, brighter at its edges, with lines scrolling up it and a flicker, as the built-in Hologram.
pub fn hologram() -> Graph {
    let mut m = Make::new("Hologram");
    m.graph.unlit = true;
    let tint = m.property("tint", PropertyKind::Color, [0.2, 0.75, 1.0, 0.0], None, "");
    let brightness = m.property("brightness", PropertyKind::Number, [1.2; 4], Some([0.0, 20.0]), "");
    let edges = m.property(
        "edges",
        PropertyKind::Number,
        [0.7; 4],
        Some([0.0, 1.0]),
        "How much brighter the edges are than the middle",
    );
    let lines =
        m.property("lines", PropertyKind::Number, [30.0; 4], Some([0.0, 100.0]), "How many lines a metre. 0 for none");
    let scroll = m.property(
        "scroll",
        PropertyKind::Number,
        [0.3; 4],
        Some([-5.0, 5.0]),
        "How fast the lines move up it, in metres a second",
    );
    let flicker = m.property("flicker", PropertyKind::Number, [0.15; 4], Some([0.0, 1.0]), "");
    let pattern =
        m.declare("pattern", PropertyKind::Picture, [0.0; 4], None, "A pattern over it, as hexagons for a force field");
    let size = m.property(
        "patternSize",
        PropertyKind::Number,
        [1.0; 4],
        Some([0.1, 10.0]),
        "How many metres the pattern covers",
    );

    let (normal, view, time) = (m.input(Input::Normal), m.input(Input::View), m.input(Input::Time));
    // Brighter at the edges, where the surface turns away from the camera.
    let facing_dot = m.math(MathOp::Dot, &[Wire(normal), Wire(view)]);
    let facing = m.math(MathOp::Absolute, &[Wire(facing_dot)]);
    let away = m.math(MathOp::OneMinus, &[Wire(facing)]);
    let edge = m.math(MathOp::Power, &[Wire(away), Num(2.0)]);
    let rim = m.math(MathOp::Multiply, &[Wire(edge), Num(1.6)]);
    let rim = m.math(MathOp::Add, &[Wire(rim), Num(0.2)]);
    let body = m.math(MathOp::Lerp, &[Num(1.0), Wire(rim), Wire(edges)]);
    // Lines scrolling up it, when it has any.
    let level = m.input(Input::LevelPosition);
    let height = m.node(NodeKind::Swizzle { components: vec![Component::Y] }, &[Wire(level)]);
    let moved = m.math(MathOp::Multiply, &[Wire(time), Wire(scroll)]);
    let along = m.math(MathOp::Subtract, &[Wire(height), Wire(moved)]);
    let banded = m.math(MathOp::Multiply, &[Wire(along), Wire(lines)]);
    let band = m.math(MathOp::Fraction, &[Wire(banded)]);
    let rising = m.math(MathOp::Smoothstep, &[Num(0.75), Num(0.85), Wire(band)]);
    let falling = m.math(MathOp::Smoothstep, &[Num(0.92), Num(1.0), Wire(band)]);
    let falling = m.math(MathOp::OneMinus, &[Wire(falling)]);
    let dim = m.math(MathOp::Multiply, &[Wire(rising), Wire(falling)]);
    let lined = m.math(MathOp::Lerp, &[Num(1.0), Num(0.35), Wire(dim)]);
    let any_lines = m.node(NodeKind::Compare { op: CompareOp::Greater }, &[Wire(lines), Num(0.0)]);
    let shown = m.node(NodeKind::Branch, &[Wire(any_lines), Wire(lined), Num(1.0)]);
    // Now and then a sudden dip, over a constant shimmer.
    let ticks = m.math(MathOp::Multiply, &[Wire(time), Num(14.0)]);
    let tick = m.math(MathOp::Floor, &[Wire(ticks)]);
    let chance = sine_hash(&mut m, Wire(tick));
    let dip = m.math(MathOp::Step, &[Num(0.88), Wire(chance)]);
    let dip = m.math(MathOp::Multiply, &[Wire(dip), Num(0.6)]);
    let fast = m.math(MathOp::Multiply, &[Wire(time), Num(31.0)]);
    let wave = m.math(MathOp::Sine, &[Wire(fast)]);
    let wave = m.math(MathOp::Multiply, &[Wire(wave), Num(0.5)]);
    let wave = m.math(MathOp::Add, &[Wire(wave), Num(0.5)]);
    let shimmer = m.math(MathOp::Multiply, &[Wire(wave), Num(0.15)]);
    let both = m.math(MathOp::Add, &[Wire(dip), Wire(shimmer)]);
    let lost = m.math(MathOp::Multiply, &[Wire(flicker), Wire(both)]);
    let flickering = m.math(MathOp::OneMinus, &[Wire(lost)]);
    // The pattern, laid on from three sides by where the surface is.
    let (position, object) = (m.input(Input::Position), m.input(Input::ObjectPosition));
    let local = m.math(MathOp::Subtract, &[Wire(position), Wire(object)]);
    let p = m.math(MathOp::Divide, &[Wire(local), Wire(size)]);
    let normal_abs = m.math(MathOp::Absolute, &[Wire(normal)]);
    let weights = m.math(MathOp::Power, &[Wire(normal_abs), Num(4.0)]);
    let parts = m.node(NodeKind::Split, &[Wire(weights)]);
    let sum = m.math(MathOp::Add, &[Out(parts, 0), Out(parts, 1)]);
    let sum = m.math(MathOp::Add, &[Wire(sum), Out(parts, 2)]);
    let sum = m.math(MathOp::Maximum, &[Wire(sum), Num(1e-4)]);
    let weights = m.math(MathOp::Divide, &[Wire(weights), Wire(sum)]);
    let mut reads = Vec::new();
    for pick in [[Component::Z, Component::Y], [Component::X, Component::Z], [Component::X, Component::Y]] {
        let uv = m.node(NodeKind::Swizzle { components: pick.to_vec() }, &[Wire(p)]);
        let read = m.node(NodeKind::SamplePicture { property: pattern }, &[Wire(uv)]);
        reads.push(read);
    }
    let three = m.node(NodeKind::Combine { ty: Ty::Float3 }, &[Out(reads[0], 2), Out(reads[1], 2), Out(reads[2], 2)]);
    let patterned = m.math(MathOp::Dot, &[Wire(three), Wire(weights)]);
    // All of it together.
    let color = m.math(MathOp::Multiply, &[Wire(tint), Wire(brightness)]);
    let color = m.math(MathOp::Multiply, &[Wire(color), Wire(body)]);
    let color = m.math(MathOp::Multiply, &[Wire(color), Wire(shown)]);
    let color = m.math(MathOp::Multiply, &[Wire(color), Wire(flickering)]);
    let color = m.math(MathOp::Multiply, &[Wire(color), Wire(patterned)]);
    let out = m.node(NodeKind::SurfaceOutput, &[Wire(color)]);
    m.graph.set_literal(out, 5, [0.0; 4]);
    m.finish()
}

/// Vertices swaying to and fro, more the farther from the Obj's foot, in a
/// wave along the shape, as the built-in Sway.
pub fn sway() -> Graph {
    use std::f32::consts::TAU;
    let mut m = Make::new("Sway");
    m.graph.max_offset = 2.1;
    let amount =
        m.property("amount", PropertyKind::Number, [0.15; 4], Some([0.0, 2.0]), "How far it sways at most, in metres");
    let speed =
        m.property("speed", PropertyKind::Number, [0.8; 4], Some([0.0, 10.0]), "How many times a second it sways");
    let ripple = m.property(
        "rippleLength",
        PropertyKind::Number,
        [3.0; 4],
        Some([0.1, 20.0]),
        "How far along it from one ripple to the next, in metres",
    );
    let reach = m.property("reach", PropertyKind::Vector3, [0.0, 2.0, 0.0, 0.0], None, "From the Obj's place to its part that sways most, in metres. Up for a plant or a sign, along the pole for a flag");
    let direction = m.property("direction", PropertyKind::Vector3, [1.0, 0.0, 0.0, 0.0], None, "Which way it sways");

    let (position, object, level, time) =
        (m.input(Input::Position), m.input(Input::ObjectPosition), m.input(Input::LevelPosition), m.input(Input::Time));
    let along = m.math(MathOp::Subtract, &[Wire(position), Wire(object)]);
    let reach_length = m.math(MathOp::Length, &[Wire(reach)]);
    let reach_length = m.math(MathOp::Maximum, &[Wire(reach_length), Num(1e-3)]);
    let tip = m.math(MathOp::Divide, &[Wire(reach), Wire(reach_length)]);
    // Still at the foot, swaying most at the far end.
    let out = m.math(MathOp::Dot, &[Wire(along), Wire(tip)]);
    let far = m.math(MathOp::Divide, &[Wire(out), Wire(reach_length)]);
    let far = m.math(MathOp::Saturate, &[Wire(far)]);
    // Each Obj its own phase, by where it stands from the level's middle.
    let placed = m.math(MathOp::Subtract, &[Wire(level), Wire(along)]);
    let seed = m.vector([12.9898, 78.233, 37.719]);
    let mixed = m.math(MathOp::Dot, &[Wire(placed), Wire(seed)]);
    let sine = m.math(MathOp::Sine, &[Wire(mixed)]);
    let spread = m.math(MathOp::Multiply, &[Wire(sine), Num(43758.5453)]);
    let own = m.math(MathOp::Fraction, &[Wire(spread)]);
    let own = m.math(MathOp::Multiply, &[Wire(own), Num(TAU)]);
    let swings = m.math(MathOp::Multiply, &[Wire(speed), Wire(time)]);
    let ripples = m.math(MathOp::Divide, &[Wire(out), Wire(ripple)]);
    let phase = m.math(MathOp::Subtract, &[Wire(swings), Wire(ripples)]);
    let phase = m.math(MathOp::Multiply, &[Num(TAU), Wire(phase)]);
    let phase = m.math(MathOp::Add, &[Wire(phase), Wire(own)]);
    let nudged = m.math(MathOp::Add, &[Wire(direction), Vec3([1e-5, 0.0, 0.0])]);
    let way = m.math(MathOp::Normalize, &[Wire(nudged)]);
    let across = m.math(MathOp::Cross, &[Wire(way), Wire(tip)]);
    let across_length = m.math(MathOp::Length, &[Wire(across)]);
    let crosses = m.node(NodeKind::Compare { op: CompareOp::Greater }, &[Wire(across_length), Num(1e-3)]);
    let across_unit = m.math(MathOp::Normalize, &[Wire(across)]);
    let zero = m.vector([0.0; 3]);
    let across = m.node(NodeKind::Branch, &[Wire(crosses), Wire(across_unit), Wire(zero)]);
    let main = m.math(MathOp::Sine, &[Wire(phase)]);
    let main = m.math(MathOp::Multiply, &[Wire(way), Wire(main)]);
    let second = m.math(MathOp::Multiply, &[Wire(phase), Num(1.7)]);
    let second = m.math(MathOp::Add, &[Wire(second), Num(1.3)]);
    let second = m.math(MathOp::Sine, &[Wire(second)]);
    let second = m.math(MathOp::Multiply, &[Wire(second), Num(0.3)]);
    let second = m.math(MathOp::Multiply, &[Wire(across), Wire(second)]);
    let motion = m.math(MathOp::Add, &[Wire(main), Wire(second)]);
    let motion = m.math(MathOp::Multiply, &[Wire(motion), Wire(amount)]);
    let far_squared = m.math(MathOp::Multiply, &[Wire(far), Wire(far)]);
    let motion = m.math(MathOp::Multiply, &[Wire(motion), Wire(far_squared)]);
    m.node(NodeKind::VertexOutput, &[Wire(motion)]);
    m.finish()
}
