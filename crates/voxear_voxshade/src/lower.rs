//! A graph into steps: one program for each of its output nodes, asked for
//! from the output back through the wires, so a node no output reaches is
//! never lowered at all.
//!
//! A sub-graph is worked in where it is used, its steps made through the
//! same builder as the graph's own, so what it works out is folded, merged
//! and left out with everything round it, as if its nodes were the graph's.
//! Its steps and anything wrong in it are counted against the Sub-graph node.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use voxear_assets::AssetGuid;

use crate::{
    graph::{Graph, NodeId, PropertyKind, is_identifier},
    ir::{Binary, Builder, CodeFunction, Given, Program, Ternary, Unary, ValueId},
    nodes::{MathOp, NodeKind, Pin, PortTy, SURFACE_OUTPUTS, Stage},
    types::Ty,
};

/// The sub-graphs a graph's Sub-graph nodes can use, by their files' GUIDs.
pub trait Library {
    fn sub_graph(&self, guid: AssetGuid) -> Option<&Graph>;
}

impl Library for HashMap<AssetGuid, Graph> {
    fn sub_graph(&self, guid: AssetGuid) -> Option<&Graph> {
        self.get(&guid)
    }
}

/// How many sub-graphs deep one may be used inside another.
const NESTED: usize = 16;

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

/// How deep one value may be asked for through others, which keeps a very
/// long chain of nodes from running the compiler out of stack.
const DEEPEST: usize = 512;

struct Lowering<'g, 'b> {
    graph: &'g Graph,
    build: &'b mut Builder,
    made: HashMap<(NodeId, usize), ValueId>,
    asking: HashSet<NodeId>,
    library: Option<&'g dyn Library>,
    within: Option<Within>,
}

/// A sub-graph being worked into the graph being compiled: the node there
/// its steps count against, its inputs' values, and the sub-graphs it is
/// inside, to stop one using itself.
struct Within {
    owner: NodeId,
    arguments: Vec<ValueId>,
    inside: Vec<AssetGuid>,
}

impl<'g, 'b> Lowering<'g, 'b> {
    fn new(graph: &'g Graph, build: &'b mut Builder, library: Option<&'g dyn Library>) -> Self {
        Self { graph, build, made: HashMap::new(), asking: HashSet::new(), library, within: None }
    }
}

/// Whether `node` is where `graph`'s program for `stage` ends: its output
/// node for the stage, or a sub-graph's Sub-graph Output.
fn is_output(graph: &Graph, kind: &NodeKind, stage: Stage) -> bool {
    if graph.sub_graph { *kind == NodeKind::SubGraphOutput } else { kind.output_stage() == Some(stage) }
}

/// The program `graph`'s output node for `stage` runs, none when the graph
/// has no such node or it sets nothing.
pub fn lower(graph: &Graph, stage: Stage, library: Option<&dyn Library>) -> Result<Option<Program>, CompileError> {
    let outputs: Vec<_> = graph.nodes.iter().filter(|node| is_output(graph, &node.kind, stage)).collect();
    let Some(output) = outputs.first() else { return Ok(None) };
    if outputs.len() > 1 {
        let title = outputs[1].kind.title();
        return Err(error(Some(outputs[1].id), format!("a graph has one {title} at most")));
    }
    let mut build = Builder::new(stage);
    let mut lowering = Lowering::new(graph, &mut build, library);
    let mut set = false;
    for (slot, port) in graph.inputs_of(&output.kind).iter().enumerate() {
        let wired = graph.link_into(output.id, slot).is_some();
        let literal = output.literals.iter().any(|literal| literal.input == slot);
        if !wired && !literal {
            continue;
        }
        let PortTy::Fixed(ty) = port.ty else { unreachable!("an output node's inputs are fixed") };
        lowering.mark(output.id);
        let value = lowering.input(output.id, slot, PortTy::Fixed(ty), 0)?;
        lowering.build.output(slot, value);
        set = true;
    }
    Ok(set.then(|| build.finish()))
}

/// Every node's every output in `stage`, for previews: one program with no
/// step left out, and which step each node's output, and each output
/// node's input, is. A node that cannot run in `stage`, or is wrong, is
/// left out.
pub fn lower_all(
    graph: &Graph,
    stage: Stage,
    library: Option<&dyn Library>,
) -> (Program, HashMap<(NodeId, usize), ValueId>) {
    let mut build = Builder::new(stage);
    let mut lowering = Lowering::new(graph, &mut build, library);
    let mut found = HashMap::new();
    for node in &graph.nodes {
        let outputs = graph.outputs_of(&node.kind);
        if node.kind.output_stage().is_some() || node.kind == NodeKind::SubGraphOutput {
            // An output node's inputs, as what it shows.
            for (slot, port) in graph.inputs_of(&node.kind).iter().enumerate() {
                lowering.asking.clear();
                if let Ok(value) = lowering.input(node.id, slot, port.ty, 0) {
                    found.insert((node.id, slot), value);
                }
            }
            continue;
        }
        for output in 0..outputs.len() {
            lowering.asking.clear();
            if let Ok(value) = lowering.value(node.id, output, 0) {
                found.insert((node.id, output), value);
            }
        }
    }
    (build.program(), found)
}

impl Lowering<'_, '_> {
    /// Counts the steps made from now on against `node`, or against the
    /// Sub-graph node a sub-graph is worked in by.
    fn mark(&mut self, node: NodeId) {
        self.build.node = Some(self.within.as_ref().map_or(node, |within| within.owner));
    }

    /// The value wired into input `index` of `node`, as `want`: what is
    /// wired in, or the numbers the node keeps, or the port's own.
    fn input(&mut self, node: NodeId, index: usize, want: PortTy, depth: usize) -> Result<ValueId, CompileError> {
        let value = match self.graph.link_into(node, index).copied() {
            Some(link) => {
                let value = self.value(link.from, link.output, depth + 1)?;
                self.mark(node);
                value
            }
            None => {
                let found = self.graph.node(node).expect("asked of a node that is there");
                let kept = found.literals.iter().find(|l| l.input == index);
                let port = self.graph.inputs_of(&found.kind).get(index).map_or([0.0; 4], |port| port.default);
                let numbers = kept.map_or(port, |literal| literal.value);
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
        if depth > DEEPEST {
            return Err(error(Some(node), "more nodes one after another than a shader can take, through this node"));
        }
        if !self.asking.insert(node) {
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
        self.mark(node);
        let value = match kind {
            NodeKind::SurfaceOutput | NodeKind::VertexOutput | NodeKind::LightOutput | NodeKind::SubGraphOutput => {
                return Err(error(Some(node), "an output node gives nothing to wire on"));
            }
            NodeKind::Input { input } => self.build.given(Given::Input(*input), input.ty()),
            NodeKind::Surface => self.build.given(Given::Surface(output), SURFACE_OUTPUTS[output].1),
            NodeKind::Constant { ty, value } => self.build.constant(*ty, *value),
            NodeKind::Color { value } => match output {
                0 => self.build.constant(Ty::Float3, linear(*value)),
                _ => self.build.number(value[3]),
            },
            NodeKind::Property { .. } | NodeKind::SamplePicture { .. } if self.graph.sub_graph => {
                return Err(error(Some(node), "a sub-graph has no properties: bring values in through its inputs"));
            }
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
                self.mark(node);
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
                self.mark(node);
                self.build.binary(Binary::Compare(*op), a, b)
            }
            NodeKind::Branch => {
                let condition = self.input(node, 0, PortTy::Fixed(Ty::Bool), depth)?;
                let sides = self.widest(node, 2, 1, depth)?;
                self.mark(node);
                self.build.select(condition, sides[0], sides[1], true)
            }
            NodeKind::Split => {
                let whole = self.input(node, 0, PortTy::Any, depth)?;
                self.mark(node);
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
                self.mark(node);
                self.build.construct(parts, *ty)
            }
            NodeKind::Swizzle { components } => {
                if components.is_empty() || components.len() > 4 {
                    return Err(error(Some(node), "a Swizzle picks from one to four numbers"));
                }
                let mut whole = self.input(node, 0, PortTy::Any, depth)?;
                self.mark(node);
                if components.iter().any(|c| c.index() >= self.build.ty(whole).width()) {
                    whole = self.build.convert(whole, Ty::Float4);
                }
                self.build.swizzle(whole, components.iter().map(|c| c.index() as u8).collect())
            }
            NodeKind::Random => {
                let seed = self.input(node, 0, PortTy::Any, depth)?;
                self.mark(node);
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
            NodeKind::Code { name, inputs, outputs, code } => {
                self.code(node, name, inputs, outputs, code, output, depth)?
            }
            NodeKind::SubGraphInput { input } => {
                let Some((place, pin)) = self.graph.input(*input) else {
                    return Err(error(Some(node), "it reads an input the sub-graph no longer has"));
                };
                match &self.within {
                    Some(within) => within.arguments[place],
                    None => self.build.given(Given::Argument(place), pin.ty),
                }
            }
            NodeKind::SubGraph { graph, name, .. } => self.sub_graph(node, *graph, name, output, depth)?,
        };
        Ok(value)
    }

    /// Output `output` of a Slang node: a call of its function.
    #[allow(clippy::too_many_arguments)]
    fn code(
        &mut self,
        node: NodeId,
        name: &str,
        inputs: &[Pin],
        outputs: &[Pin],
        code: &str,
        output: usize,
        depth: usize,
    ) -> Result<ValueId, CompileError> {
        if outputs.is_empty() {
            return Err(error(Some(node), "a Slang node gives one output at least"));
        }
        let mut names = HashSet::new();
        let pins = || inputs.iter().chain(outputs);
        if pins().any(|pin| !is_identifier(&pin.name) || !names.insert(pin.name.as_str()) || pin.ty == Ty::Bool) {
            return Err(error(
                Some(node),
                "each of a Slang node's inputs and outputs needs a name of its own, of letters, digits and _, and numbers",
            ));
        }
        let mut args = Vec::with_capacity(inputs.len());
        for (index, pin) in inputs.iter().enumerate() {
            args.push(self.input(node, index, PortTy::Fixed(pin.ty), depth)?);
        }
        self.mark(node);
        let function = CodeFunction {
            name: if is_identifier(name) { name.to_string() } else { String::new() },
            inputs: inputs.iter().map(|pin| (pin.name.clone(), pin.ty)).collect(),
            outputs: outputs.iter().map(|pin| (pin.name.clone(), pin.ty)).collect(),
            code: code.to_string(),
        };
        let call = self.build.call(Arc::new(function), args);
        if output >= outputs.len() {
            return Err(error(Some(node), "a wire comes from an output the Slang node no longer has"));
        }
        Ok(self.build.call_out(call, output))
    }

    /// Output `output` of a Sub-graph node: the sub-graph's own, worked in
    /// here with the values wired into the node as its inputs.
    fn sub_graph(
        &mut self,
        node: NodeId,
        guid: AssetGuid,
        name: &str,
        output: usize,
        depth: usize,
    ) -> Result<ValueId, CompileError> {
        let Some(sub) = self.library.and_then(|library| library.sub_graph(guid)) else {
            return Err(error(Some(node), format!("the sub-graph {name} is not in the project")));
        };
        if !sub.sub_graph {
            return Err(error(Some(node), format!("{name} is a shader, not a sub-graph")));
        }
        let mut inside = self.within.as_ref().map_or(Vec::new(), |within| within.inside.clone());
        if inside.contains(&guid) || inside.len() >= NESTED {
            return Err(error(Some(node), format!("{name} is used inside itself")));
        }
        inside.push(guid);
        let mut arguments = Vec::with_capacity(sub.inputs.len());
        for (index, pin) in sub.inputs.iter().enumerate() {
            arguments.push(self.input(node, index, PortTy::Fixed(pin.ty), depth)?);
        }
        let Some(out) = sub.nodes.iter().find(|n| n.kind == NodeKind::SubGraphOutput) else {
            return Err(error(Some(node), format!("{name} has no Sub-graph Output")));
        };
        let Some(pin) = sub.outputs.get(output) else {
            return Err(error(Some(node), format!("a wire comes from an output {name} no longer has")));
        };
        let owner = self.within.as_ref().map_or(node, |within| within.owner);
        let library = self.library;
        let mut inner = Lowering::new(sub, &mut *self.build, library);
        inner.within = Some(Within { owner, arguments, inside });
        inner.mark(out.id);
        let value = inner
            .input(out.id, output, PortTy::Fixed(pin.ty), depth + 1)
            .map_err(|wrong| error(Some(node), format!("in {name}: {}", wrong.message)))?;
        self.mark(node);
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
            self.mark(node);
            return Ok(self.build.unary(unary, a));
        }
        if op == M::OneMinus {
            let a = self.input(node, 0, PortTy::Any, depth)?;
            self.mark(node);
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
            self.mark(node);
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
        self.mark(node);
        Ok(self.build.binary(binary, v[0], v[1]))
    }
}

/// A colour as it looks on screen, as light: what a Color node's numbers
/// are worked out in.
fn linear(srgb: [f32; 4]) -> [f32; 4] {
    let channel = |c: f32| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    [channel(srgb[0]), channel(srgb[1]), channel(srgb[2]), srgb[3]]
}
