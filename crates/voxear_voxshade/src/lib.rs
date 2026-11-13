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
pub mod graph;
pub mod ir;
pub mod lower;
pub mod nodes;
pub mod starters;
pub mod types;

use std::collections::HashMap;
use std::fmt::Write;

pub use cost::{NodeCost, StageCost};
pub use graph::{Graph, Link, Literal, Node, NodeId, Property, PropertyId, PropertyKind};
pub use lower::CompileError;
pub use nodes::{CompareOp, Component, Input, MathOp, NodeKind, Port, PortTy, Stage};
pub use types::Ty;

/// A graph compiled: the shader's Slang, and what each stage and node costs.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    pub source: String,
    pub stages: Vec<(Stage, StageCost)>,
    pub nodes: HashMap<NodeId, NodeCost>,
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
    graph.check().map_err(|e| vec![CompileError { node: None, message: format!("{e:#}") }])?;
    let mut programs = Vec::new();
    let mut errors = Vec::new();
    for stage in Stage::ALL {
        match lower::lower(graph, stage) {
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
    let mut nodes = HashMap::new();
    let stages = programs.iter().map(|(stage, program)| (*stage, cost::program(program, &mut nodes))).collect();
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
    source.push_str("}\n");
    let random = programs.iter().any(|(_, p)| p.steps.iter().any(|s| s.op == ir::Op::Unary(ir::Unary::Random)));
    if random {
        source.push('\n');
        source.push_str(RANDOM);
    }
    for (stage, program) in &programs {
        let head = match stage {
            Stage::Surface => {
                format!("void surface({} props, SurfaceInput input, inout SurfaceOutput surface)", graph.name)
            }
            Stage::Vertex => format!("float3 vertex({} props, VertexInput input)", graph.name),
            Stage::Light => format!("float3 light({} props, LightInput light, SurfaceOutput surface)", graph.name),
        };
        let _ = write!(source, "\n{head}\n{{\n{}}}\n", emit::body(graph, program, *stage));
    }
    Ok(Compiled { source, stages, nodes })
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
