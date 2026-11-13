//! A graph into steps: one program for each of its output nodes, asked for
//! from the output back through the wires, so a node no output reaches is
//! never lowered at all.

use std::collections::{HashMap, HashSet};

use crate::{
    graph::{Graph, NodeId, PropertyKind},
    ir::{Binary, Builder, Given, Program, Ternary, Unary, ValueId},
    nodes::{MathOp, NodeKind, PortTy, SURFACE_OUTPUTS, Stage},
    types::Ty,
};

/// Something wrong with a graph that stops it compiling, and the node it is
/// wrong at, when it is one node's.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileError {
    pub node: Option<NodeId>,
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

fn error(node: Option<NodeId>, message: impl Into<String>) -> CompileError {
    CompileError { node, message: message.into() }
}

/// How deep one value may be asked for through others before the graph is
/// taken to run round in a loop.
const DEEPEST: usize = 512;

struct Lowering<'g> {
    graph: &'g Graph,
    build: Builder,
    made: HashMap<(NodeId, usize), ValueId>,
    asking: HashSet<NodeId>,
}

/// The program `graph`'s output node for `stage` runs, none when the graph
/// has no such node or it sets nothing.
pub fn lower(graph: &Graph, stage: Stage) -> Result<Option<Program>, CompileError> {
    let outputs: Vec<_> = graph.nodes.iter().filter(|node| node.kind.output_stage() == Some(stage)).collect();
    let Some(output) = outputs.first() else { return Ok(None) };
    if outputs.len() > 1 {
        return Err(error(Some(outputs[1].id), format!("a graph has one {} Output at most", stage.name())));
    }
    let mut lowering = Lowering { graph, build: Builder::new(stage), made: HashMap::new(), asking: HashSet::new() };
    let mut set = false;
    for (slot, port) in output.kind.inputs().iter().enumerate() {
        let wired = graph.link_into(output.id, slot).is_some();
        let literal = output.literals.iter().any(|literal| literal.input == slot);
        if !wired && !literal {
            continue;
        }
        let PortTy::Fixed(ty) = port.ty else { unreachable!("an output node's inputs are fixed") };
        lowering.build.node = Some(output.id);
        let value = lowering.input(output.id, slot, PortTy::Fixed(ty), 0)?;
        lowering.build.output(slot, value);
        set = true;
    }
    Ok(set.then(|| lowering.build.finish()))
}

impl Lowering<'_> {
    /// The value wired into input `index` of `node`, as `want`: what is
    /// wired in, or the numbers the node keeps, or the port's own.
    fn input(&mut self, node: NodeId, index: usize, want: PortTy, depth: usize) -> Result<ValueId, CompileError> {
        let value = match self.graph.link_into(node, index).copied() {
            Some(link) => {
                let value = self.value(link.from, link.output, depth + 1)?;
                self.build.node = Some(node);
                value
            }
            None => {
                let kept = self.graph.node(node).and_then(|n| n.literals.iter().find(|l| l.input == index));
                let port = &self.graph.node(node).expect("asked of a node that is there").kind.inputs()[index];
                let numbers = kept.map_or(port.default, |literal| literal.value);
                match want {
                    PortTy::Fixed(Ty::Bool) => self.build.constant(Ty::Bool, [f32::from(numbers[0] != 0.0); 4]),
                    PortTy::Fixed(ty) => self.build.constant(ty, numbers),
                    PortTy::Any => self.build.number(numbers[0]),
                }
            }
        };
        let is_truth = self.build.ty(value) == Ty::Bool;
        match want {
            PortTy::Fixed(Ty::Bool) if !is_truth => {
                Err(error(Some(node), "a condition takes true or false, as a Compare gives; a number was wired in"))
            }
            PortTy::Fixed(Ty::Bool) => Ok(value),
            _ if is_truth => Err(error(Some(node), "numbers are wanted here; true or false was wired in")),
            PortTy::Fixed(ty) => Ok(self.build.convert(value, ty)),
            PortTy::Any => Ok(value),
        }
    }

    /// The inputs of `node` from `first` on, each taken as numbers, all made
    /// as wide as the widest of them.
    fn widest(&mut self, node: NodeId, count: usize, first: usize, depth: usize) -> Result<Vec<ValueId>, CompileError> {
        let mut values = Vec::with_capacity(count);
        for index in first..first + count {
            values.push(self.input(node, index, PortTy::Any, depth)?);
        }
        let width = values.iter().map(|v| self.build.ty(*v).width()).max().unwrap_or(1);
        let ty = Ty::of_width(width);
        Ok(values.into_iter().map(|v| self.build.convert(v, ty)).collect())
    }

    /// Output `output` of `node`.
    fn value(&mut self, node: NodeId, output: usize, depth: usize) -> Result<ValueId, CompileError> {
        if let Some(&made) = self.made.get(&(node, output)) {
            return Ok(made);
        }
        let Some(found) = self.graph.node(node) else {
            return Err(error(None, "a wire comes from a node that is not there"));
        };
        if depth > DEEPEST || !self.asking.insert(node) {
            return Err(error(Some(node), "the wires run round in a loop through this node"));
        }
        let stage = self.build.stage;
        if !found.kind.in_stage(stage) {
            let what = found.kind.title();
            return Err(error(Some(node), format!("{what} is not there in the {} stage", stage.name())));
        }
        let value = self.lower_node(node, &found.kind, output, depth)?;
        self.asking.remove(&node);
        self.made.insert((node, output), value);
        Ok(value)
    }

    fn lower_node(
        &mut self,
        node: NodeId,
        kind: &NodeKind,
        output: usize,
        depth: usize,
    ) -> Result<ValueId, CompileError> {
        self.build.node = Some(node);
        let value = match kind {
            NodeKind::SurfaceOutput | NodeKind::VertexOutput | NodeKind::LightOutput => {
                return Err(error(Some(node), "an output node gives nothing to wire on"));
            }
            NodeKind::Input { input } => self.build.given(Given::Input(*input), input.ty()),
            NodeKind::Surface => self.build.given(Given::Surface(output), SURFACE_OUTPUTS[output].1),
            NodeKind::Constant { ty, value } => self.build.constant(*ty, *value),
            NodeKind::Color { value } => match output {
                0 => self.build.constant(Ty::Float3, linear(*value)),
                _ => self.build.number(value[3]),
            },
            NodeKind::Property { property } => {
                let Some(found) = self.graph.property(*property) else {
                    return Err(error(Some(node), "it reads a property the graph no longer has"));
                };
                let Some(ty) = found.kind.ty() else {
                    return Err(error(Some(node), format!("{} is a picture: read it with Sample Picture", found.name)));
                };
                self.build.property(*property, ty)
            }
            NodeKind::SamplePicture { property } => {
                let found = self.graph.property(*property);
                if found.is_none_or(|p| p.kind != PropertyKind::Picture) {
                    return Err(error(Some(node), "it samples a property that is not a picture"));
                }
                let uv = self.input(node, 0, PortTy::Fixed(Ty::Float2), depth)?;
                self.build.node = Some(node);
                let rgba = self.build.sample(*property, uv);
                match output {
                    0 => rgba,
                    1 => self.build.swizzle(rgba, vec![0, 1, 2]),
                    n => self.build.extract(rgba, n - 2),
                }
            }
            NodeKind::Math { op } => self.math(node, *op, depth)?,
            NodeKind::Compare { op } => {
                let a = self.input(node, 0, PortTy::Fixed(Ty::Float), depth)?;
                let b = self.input(node, 1, PortTy::Fixed(Ty::Float), depth)?;
                self.build.node = Some(node);
                self.build.binary(Binary::Compare(*op), a, b)
            }
            NodeKind::Branch => {
                let condition = self.input(node, 0, PortTy::Fixed(Ty::Bool), depth)?;
                let sides = self.widest(node, 2, 1, depth)?;
                self.build.node = Some(node);
                self.build.select(condition, sides[0], sides[1], true)
            }
            NodeKind::Split => {
                let whole = self.input(node, 0, PortTy::Any, depth)?;
                self.build.node = Some(node);
                if output < self.build.ty(whole).width() {
                    self.build.extract(whole, output)
                } else {
                    self.build.number(0.0)
                }
            }
            NodeKind::Combine { ty } => {
                let mut parts = Vec::new();
                for index in 0..ty.width() {
                    parts.push(self.input(node, index, PortTy::Fixed(Ty::Float), depth)?);
                }
                self.build.node = Some(node);
                self.build.construct(parts, *ty)
            }
            NodeKind::Swizzle { components } => {
                if components.is_empty() || components.len() > 4 {
                    return Err(error(Some(node), "a Swizzle picks from one to four numbers"));
                }
                let mut whole = self.input(node, 0, PortTy::Any, depth)?;
                self.build.node = Some(node);
                if components.iter().any(|c| c.index() >= self.build.ty(whole).width()) {
                    whole = self.build.convert(whole, Ty::Float4);
                }
                self.build.swizzle(whole, components.iter().map(|c| c.index() as u8).collect())
            }
            NodeKind::Random => {
                let seed = self.input(node, 0, PortTy::Any, depth)?;
                self.build.node = Some(node);
                let width = self.build.ty(seed).width();
                // A vector's numbers mixed into one first, each by a different amount.
                let mixed = if width > 1 {
                    let weights = self.build.constant(Ty::of_width(width), [1.0, 57.31, 113.17, 271.93]);
                    self.build.binary(Binary::Dot, seed, weights)
                } else {
                    seed
                };
                self.build.unary(Unary::Random, mixed)
            }
        };
        Ok(value)
    }

    fn math(&mut self, node: NodeId, op: MathOp, depth: usize) -> Result<ValueId, CompileError> {
        use MathOp as M;
        let unary = |op| -> Option<Unary> {
            Some(match op {
                M::Negate => Unary::Negate,
                M::Absolute => Unary::Abs,
                M::Floor => Unary::Floor,
                M::Ceiling => Unary::Ceil,
                M::Fraction => Unary::Frac,
                M::Sine => Unary::Sin,
                M::Cosine => Unary::Cos,
                M::SquareRoot => Unary::Sqrt,
                M::Exponential => Unary::Exp,
                M::Saturate => Unary::Saturate,
                M::Sign => Unary::Sign,
                M::Normalize => Unary::Normalize,
                M::Length => Unary::Length,
                _ => return None,
            })
        };
        if let Some(unary) = unary(op) {
            let a = self.input(node, 0, PortTy::Any, depth)?;
            self.build.node = Some(node);
            return Ok(self.build.unary(unary, a));
        }
        if op == M::OneMinus {
            let a = self.input(node, 0, PortTy::Any, depth)?;
            self.build.node = Some(node);
            let ty = self.build.ty(a);
            let one = self.build.constant(ty, [1.0; 4]);
            return Ok(self.build.binary(Binary::Sub, one, a));
        }
        let ternary = match op {
            M::Lerp => Some(Ternary::Lerp),
            M::Clamp => Some(Ternary::Clamp),
            M::Smoothstep => Some(Ternary::Smoothstep),
            _ => None,
        };
        if let Some(ternary) = ternary {
            let v = self.widest(node, 3, 0, depth)?;
            self.build.node = Some(node);
            return Ok(self.build.ternary(ternary, v[0], v[1], v[2]));
        }
        let binary = match op {
            M::Add => Binary::Add,
            M::Subtract => Binary::Sub,
            M::Multiply => Binary::Mul,
            M::Divide => Binary::Div,
            M::Minimum => Binary::Min,
            M::Maximum => Binary::Max,
            M::Power => Binary::Pow,
            M::Step => Binary::Step,
            M::Dot => Binary::Dot,
            M::Distance => Binary::Distance,
            M::Reflect => Binary::Reflect,
            M::Cross => Binary::Cross,
            _ => unreachable!("every other op is handled above"),
        };
        let v = if op == M::Cross {
            vec![
                self.input(node, 0, PortTy::Fixed(Ty::Float3), depth)?,
                self.input(node, 1, PortTy::Fixed(Ty::Float3), depth)?,
            ]
        } else {
            self.widest(node, 2, 0, depth)?
        };
        self.build.node = Some(node);
        Ok(self.build.binary(binary, v[0], v[1]))
    }
}

/// A colour as it looks on screen, as light: what a Color node's numbers
/// are worked out in.
fn linear(srgb: [f32; 4]) -> [f32; 4] {
    let channel = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    [channel(srgb[0]), channel(srgb[1]), channel(srgb[2]), srgb[3]]
}
