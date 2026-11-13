//! What a compiled graph costs, as its nodes and stages show it: the work
//! left once everything that could be folded, merged or left out has been,
//! counted in rough instructions, and the pictures read.
//!
//! The counts are a guide, not a measure: a sum or a product is one for each
//! number, a division or a root about four, a power about nine, as the GPUs
//! the engine runs on take them. They are there to show where a graph's
//! work is, and that a node which comes to nothing costs nothing.

use std::collections::HashMap;

use crate::{
    graph::NodeId,
    ir::{Binary, Op, Program, Rate, Step, Ternary, Unary},
};

/// The rough instructions and the picture reads of one step.
pub fn of(step: &Step, program: &Program) -> (u32, u32) {
    let width = |n: usize| program.step(step.args[n]).ty.width() as u32;
    let own = step.ty.width() as u32;
    let alu = match &step.op {
        Op::Const(_) | Op::Given(_) | Op::Property(_) | Op::Construct | Op::Swizzle(_) => 0,
        Op::Sample(_) => return (2, 1),
        Op::Unary(op) => match op {
            Unary::Negate | Unary::Abs | Unary::Saturate => 0,
            Unary::Floor | Unary::Ceil | Unary::Frac | Unary::Sign => own,
            Unary::Sin | Unary::Cos | Unary::Sqrt | Unary::Exp => own * 4,
            Unary::Normalize => width(0) * 2 + 4,
            Unary::Length => width(0) + 4,
            Unary::Random => 8,
        },
        Op::Binary(op) => match op {
            Binary::Add | Binary::Sub | Binary::Mul | Binary::Min | Binary::Max | Binary::Step => own,
            Binary::Div => own * 4,
            Binary::Pow => own * 9,
            Binary::Dot => width(0),
            Binary::Cross => 6,
            Binary::Distance => width(0) * 2 + 4,
            Binary::Reflect => width(0) * 3,
            Binary::Compare(_) => 1,
        },
        Op::Ternary(op) => match op {
            Ternary::Lerp | Ternary::Clamp => own * 2,
            Ternary::Smoothstep => own * 6,
        },
        Op::Select { .. } => own,
    };
    (alu, 0)
}

/// What a node costs once compiled: its instructions and picture reads in
/// each stage it runs in. A node whose work was folded away, merged into
/// another's or left out costs nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeCost {
    pub alu: u32,
    pub samples: u32,
}

/// What a stage costs: its instructions and picture reads, how many of the
/// instructions are the same all over a draw (worked out from properties
/// and the time alone, which could be done once rather than each time),
/// and how many of its branches are real.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageCost {
    pub alu: u32,
    pub samples: u32,
    pub uniform_alu: u32,
    pub branches: u32,
}

/// What `program` costs, as a whole and node by node.
pub fn program(program: &Program, nodes: &mut HashMap<NodeId, NodeCost>) -> StageCost {
    let mut total = StageCost { branches: crate::emit::real_count(program) as u32, ..StageCost::default() };
    for step in &program.steps {
        let (alu, samples) = of(step, program);
        total.alu += alu;
        total.samples += samples;
        if step.rate == Rate::Uniform {
            total.uniform_alu += alu;
        }
        if let Some(node) = step.node {
            let cost = nodes.entry(node).or_default();
            cost.alu += alu;
            cost.samples += samples;
        }
    }
    total
}
