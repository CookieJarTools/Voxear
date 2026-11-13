//! VoxShade shaders: nodes wired together into a shader, compiled to Slang
//! that costs what Slang written by hand does.
//!
//! A graph is compiled in three steps:
//!
//! 1. **Lowering** ([`lower`]): from each output node back through the
//!    wires into a list of typed steps, a program a stage. Nodes no output
//!    reaches are never lowered.
//! 2. **Working out what can be** ([`ir::Builder`], as each step is made):
//!    numbers known now are worked out, work that changes nothing goes, the
//!    same work asked for twice is done once, and what no output reads is
//!    left out. Each step knows how often its value changes: never, once a
//!    draw (from properties and the time), or from place to place.
//! 3. **Writing Slang** ([`emit`]): the same properties struct and
//!    `surface`, `vertex` and `light` functions a hand-written `.shader`
//!    has, so the engine compiles it as it compiles any. A Branch whose
//!    sides are worth skipping is a real `if` with only that side's work in
//!    it; the rest are selects.
//!
//! What it costs comes with it ([`cost`]): each node's instructions and
//! picture reads once all of that is done, and each stage's.

pub mod cost;
pub mod emit;
pub mod eval;
pub mod graph;
pub mod hoist;
pub mod ir;
pub mod lower;
pub mod nodes;
pub mod starters;
pub mod types;

use std::collections::HashMap;
use std::fmt::Write;

pub use cost::{NodeCost, StageCost};
pub use graph::{Graph, Link, Literal, Node, NodeId, Property, PropertyId, PropertyKind};
pub use hoist::Derived;
pub use lower::{CompileError, Library};
pub use nodes::{CompareOp, Component, Input, MathOp, NodeKind, Pin, Port, PortTy, Stage};
pub use types::Ty;

/// A graph compiled: the shader's Slang, what each stage and node costs,
/// and what a material using it works out on the CPU from its properties.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    pub source: String,
    pub stages: Vec<(Stage, StageCost)>,
    pub nodes: HashMap<NodeId, NodeCost>,
    pub derived: hoist::Derived,
}

/// How a graph is compiled.
#[derive(Clone, Copy)]
pub struct Options<'a> {
    /// Work the properties alone decide is worked out on the CPU, once a
    /// material, rather than on the GPU.
    pub hoist: bool,
    /// The project's sub-graphs, which its Sub-graph nodes use.
    pub library: Option<&'a dyn Library>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self { hoist: true, library: None }
    }
}

/// The whole-number hash the Random node runs, as `ir::random` works it out.
const RANDOM: &str = "float graphRandom(float seed)
{
    uint state = asuint(seed) * 747796405u + 2891336453u;
    state = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return float((state >> 22u) ^ state) / 4294967296.0;
}
";

/// Compiles `graph` into a shader's Slang, or says everything that stops it.
pub fn compile(graph: &Graph) -> Result<Compiled, Vec<CompileError>> {
    compile_with(graph, Options::default())
}

pub fn compile_with(graph: &Graph, options: Options) -> Result<Compiled, Vec<CompileError>> {
    graph.check().map_err(|e| vec![CompileError { node: None, message: format!("{e:#}") }])?;
    if graph.sub_graph {
        return compile_sub_graph(graph, options);
    }
    let mut programs = Vec::new();
    let mut errors = Vec::new();
    for stage in Stage::ALL {
        match lower::lower(graph, stage, options.library) {
            Ok(Some(program)) => programs.push((stage, program)),
            Ok(None) => {}
            Err(error) => errors.push(error),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    if programs.is_empty() && !graph.unlit {
        return Err(vec![CompileError { node: None, message: "wire something into an output node".into() }]);
    }
    // What each stage works out on the CPU, as many as the material has room for after its own properties.
    let mut room = if options.hoist { hoist::SLOTS.saturating_sub(graph.properties.len()) } else { 0 };
    let mut derived = hoist::Derived { runs: Vec::new(), properties: graph.properties.iter().map(|p| p.id).collect() };
    let mut plans = Vec::new();
    for (_, program) in &programs {
        let roots = hoist::roots(program, room);
        room -= roots.len();
        let first = derived.types().len();
        plans.push(emit::Plan::new(program, &roots, first));
        derived.runs.push((program.clone(), roots));
    }
    let mut nodes = HashMap::new();
    let stages = programs
        .iter()
        .zip(&plans)
        .map(|((stage, program), plan)| (*stage, cost::program(program, plan, &mut nodes)))
        .collect();
    let mut source = String::new();
    let _ = writeln!(source, "// {}: made in VoxShade. Change it there, not here.\n", graph.name);
    if graph.unlit {
        source.push_str("[Unlit]\n");
    }
    if graph.max_offset > 0.0 {
        let _ = writeln!(source, "[MaxOffset({})]", graph.max_offset);
    }
    let _ = writeln!(source, "struct {}\n{{", graph.name);
    for property in &graph.properties {
        source.push_str(&declaration(property));
    }
    // What a material works out from its properties, on the CPU, read here as it reads them.
    for (slot, ty) in derived.types().iter().enumerate() {
        let _ = writeln!(source, "    {} {};", ty.slang(), emit::derived_name(slot));
    }
    source.push_str("}\n");
    let random = programs.iter().any(|(_, p)| p.steps.iter().any(|s| s.op == ir::Op::Unary(ir::Unary::Random)));
    if random {
        source.push('\n');
        source.push_str(RANDOM);
    }
    // Each Slang node's function, once however many nodes and stages call it.
    let mut declared = std::collections::HashSet::new();
    for (_, program) in &programs {
        for step in &program.steps {
            if let ir::Op::Call(function) = &step.op
                && declared.insert(function.slang_name())
            {
                source.push('\n');
                source.push_str(&function.declaration());
            }
        }
    }
    for ((stage, program), plan) in programs.iter().zip(&plans) {
        let head = match stage {
            Stage::Surface => {
                format!("void surface({} props, SurfaceInput input, inout SurfaceOutput surface)", graph.name)
            }
            Stage::Vertex => format!("float3 vertex({} props, VertexInput input)", graph.name),
            Stage::Light => format!("float3 light({} props, LightInput light, SurfaceOutput surface)", graph.name),
        };
        let _ = write!(source, "\n{head}\n{{\n{}}}\n", emit::body(graph, program, *stage, plan));
    }
    if derived.is_empty() {
        derived.runs.clear();
    }
    Ok(Compiled { source, stages, nodes, derived })
}

/// A sub-graph compiled on its own, to be shown: what its nodes cost and
/// what is wrong with them, and its work as a Slang function of its
/// inputs. It is compiled in the first stage its nodes can all run in.
fn compile_sub_graph(graph: &Graph, options: Options) -> Result<Compiled, Vec<CompileError>> {
    let mut first_wrong = None;
    for stage in [Stage::Surface, Stage::Light, Stage::Vertex] {
        let program = match lower::lower(graph, stage, options.library) {
            Ok(Some(program)) => program,
            Ok(None) => {
                return Err(vec![CompileError {
                    node: None,
                    message: "wire something into the Sub-graph Output".into(),
                }]);
            }
            Err(wrong) => {
                first_wrong.get_or_insert(wrong);
                continue;
            }
        };
        let plan = emit::Plan::new(&program, &[], 0);
        let mut nodes = HashMap::new();
        let cost = cost::program(&program, &plan, &mut nodes);
        let given = match stage {
            Stage::Surface => "SurfaceInput input, SurfaceOutput surface",
            Stage::Light => "LightInput light, SurfaceOutput surface",
            Stage::Vertex => "VertexInput input",
        };
        let inputs = graph.inputs.iter().map(|pin| format!("{} {}", pin.ty.slang(), pin.name));
        let outputs = graph.outputs.iter().map(|pin| format!("out {} {}", pin.ty.slang(), pin.name));
        let parameters: Vec<String> = std::iter::once(given.to_string()).chain(inputs).chain(outputs).collect();
        let mut source = format!(
            "// {}: a VoxShade sub-graph, worked into each shader that uses it.\n\nvoid {}({})\n{{\n",
            graph.name,
            graph.name,
            parameters.join(", ")
        );
        source += &emit::body(graph, &program, stage, &plan);
        source += "}\n";
        return Ok(Compiled { source, stages: vec![(stage, cost)], nodes, derived: hoist::Derived::default() });
    }
    Err(vec![first_wrong.expect("one stage at least was tried")])
}

/// A property as its struct declares it.
fn declaration(property: &Property) -> String {
    let mut attributes = vec!["Property".to_string()];
    if matches!(property.kind, PropertyKind::Color | PropertyKind::ColorAlpha) {
        attributes.push("Color".into());
    }
    if let Some([low, high]) = property.range {
        attributes.push(format!("Range({low}, {high})"));
    }
    if !property.tooltip.is_empty() {
        attributes.push(format!("Tooltip(\"{}\")", property.tooltip.replace(['"', '\\'], "'")));
    }
    let attributes = attributes.join(", ");
    let name = &property.name;
    match property.kind.ty() {
        None => format!("    [{attributes}] Picture {name};\n"),
        Some(ty) => {
            let value = &property.default[..ty.width()];
            let numbers: Vec<String> = value.iter().map(|v| format!("{v:?}")).collect();
            let default =
                if ty == Ty::Float { numbers[0].clone() } else { format!("{}({})", ty.slang(), numbers.join(", ")) };
            format!("    [{attributes}] {} {name} = {default};\n", ty.slang())
        }
    }
}

#[cfg(test)]
mod tests;
