//! Working a program out on the CPU, step by step, as the GPU would: for
//! the editor's previews of each node, and for what a material's
//! properties alone decide, which is worked out once rather than at every
//! pixel.

use crate::{
    graph::PropertyId,
    ir::{Op, Program, Ternary, fold_binary, fold_ternary, fold_unary},
};

/// What a program is given as it runs: what its stage is given, each
/// property's value, and a picture read at a place.
pub struct Inputs<'a> {
    pub given: &'a dyn Fn(crate::ir::Given) -> [f32; 4],
    pub property: &'a dyn Fn(PropertyId) -> [f32; 4],
    pub sample: &'a dyn Fn(crate::ir::Picture, [f32; 2]) -> [f32; 4],
}

/// Runs every step of `program`, its values into `values`, a step each.
pub fn run(program: &Program, given: &Inputs<'_>, values: &mut Vec<[f32; 4]>) {
    values.clear();
    values.reserve(program.steps.len());
    for step in &program.steps {
        let arg = |n: usize| values[step.args[n].0 as usize];
        let width = |n: usize| program.steps[step.args[n].0 as usize].ty.width();
        let value = match &step.op {
            Op::Const(value) => *value,
            Op::Given(what) => (given.given)(*what),
            Op::Property(property) => (given.property)(*property),
            Op::Sample(property) => {
                let uv = arg(0);
                (given.sample)(*property, [uv[0], uv[1]])
            }
            Op::Unary(op) => fold_unary(*op, arg(0), width(0)),
            Op::Binary(op) => fold_binary(*op, arg(0), arg(1), width(0)),
            Op::Ternary(op) => {
                let width = if *op == Ternary::Smoothstep { width(2) } else { step.ty.width() };
                fold_ternary(*op, arg(0), arg(1), arg(2), width)
            }
            Op::Select { .. } => {
                if arg(0)[0] != 0.0 {
                    arg(1)
                } else {
                    arg(2)
                }
            }
            Op::Construct => {
                let mut value = [0.0; 4];
                for (slot, part) in value.iter_mut().zip(0..step.args.len()) {
                    *slot = arg(part)[0];
                }
                value
            }
            // Slang written by hand is not run on the CPU: what it gives is not known here.
            Op::Call(_) | Op::CallOut(_) => [f32::NAN; 4],
            Op::Swizzle(picks) => {
                let from = arg(0);
                let mut value = [0.0; 4];
                for (slot, pick) in value.iter_mut().zip(picks) {
                    *slot = from[*pick as usize];
                }
                value
            }
        };
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        graph::Graph,
        ir::Given as Input,
        lower::lower_all,
        nodes::{Input as Kind, MathOp, NodeKind, Stage},
    };

    #[test]
    fn a_graph_works_out_on_the_cpu_as_its_slang_would() {
        // sin(uv.x * 3) + time, at uv (0.5, 0.25) and time 2.
        let mut graph = Graph::new("Test");
        let uv = graph.add(NodeKind::Input { input: Kind::Uv }, [0.0; 2]);
        let split = graph.add(NodeKind::Split, [0.0; 2]);
        let times = graph.add(NodeKind::Math { op: MathOp::Multiply }, [0.0; 2]);
        let sine = graph.add(NodeKind::Math { op: MathOp::Sine }, [0.0; 2]);
        let time = graph.add(NodeKind::Input { input: Kind::Time }, [0.0; 2]);
        let add = graph.add(NodeKind::Math { op: MathOp::Add }, [0.0; 2]);
        graph.link(uv, 0, split, 0);
        graph.link(split, 0, times, 0);
        graph.set_literal(times, 1, [3.0; 4]);
        graph.link(times, 0, sine, 0);
        graph.link(sine, 0, add, 0);
        graph.link(time, 0, add, 1);
        graph.link(add, 0, graph.nodes[0].id, 1);
        let (program, found) = lower_all(&graph, Stage::Surface, None);
        let given = |what: Input| match what {
            Input::Input(Kind::Uv) => [0.5, 0.25, 0.0, 0.0],
            Input::Input(Kind::Time) => [2.0; 4],
            _ => [0.0; 4],
        };
        let mut values = Vec::new();
        run(&program, &Inputs { given: &given, property: &|_| [0.0; 4], sample: &|_, _| [1.0; 4] }, &mut values);
        let at = |node, output| values[found[&(node, output)].0 as usize];
        assert_eq!(at(split, 1)[0], 0.25);
        assert!((at(add, 0)[0] - (1.5f32.sin() + 2.0)).abs() < 1e-6);
        assert!((at(graph.nodes[0].id, 1)[0] - at(add, 0)[0]).abs() < 1e-6, "the output's Alpha");
        assert_eq!(at(graph.nodes[0].id, 0), [0.0; 4], "nothing in its Color, which takes its port's own");
    }
}
