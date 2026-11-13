//! Steps into Slang: a `.shader`'s properties struct and its `surface`,
//! `vertex` and `light` functions, as one written by hand has them.
//!
//! A step whose value is known, given or a property is written where it is
//! read; every other step is a local of its own, in the order the steps
//! run. A Branch whose sides are worth skipping becomes a real `if`, with
//! the steps only that side needs inside it, so they run only when it is
//! taken; the rest become a `?:`, which runs both.
//!
//! Where a step goes: each is placed in the innermost block every one of
//! its readers is in, so a step read by both sides of a branch, or by
//! something after it, is worked out once before it.

use std::fmt::Write;

use crate::{
    cost,
    graph::Graph,
    ir::{Binary, Given, Op, Program, Rate, Step, Ternary, Unary, ValueId},
    nodes::{Input, Stage},
    types::Ty,
};

/// How much work, at the least, one side of a branch has to hold before it
/// is worth a real branch: a uniform condition is the same all over a draw,
/// so no pixels wait on others and a little work is enough; one that
/// changes from pixel to pixel splits the pixels that run together, which
/// only pays for a picture read or a good deal of work.
const UNIFORM_BRANCH_FROM: u32 = 2;
const VARYING_BRANCH_FROM: u32 = 16;

/// Where a step is placed: the function's body, or one side of a branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Block {
    /// The block it is inside, none for the body.
    parent: Option<usize>,
    depth: usize,
}

/// The steps placed in each block, the branches made real, and each step's block.
struct Placing {
    blocks: Vec<Block>,
    of_step: Vec<usize>,
    /// For each real branch, by its step, its two sides' blocks.
    sides: Vec<Option<[usize; 2]>>,
}

/// Whether a step is written where it is read rather than as a local.
fn written_in_place(step: &Step) -> bool {
    matches!(step.op, Op::Const(_) | Op::Given(_) | Op::Property(_))
}

fn common_block(blocks: &[Block], mut a: usize, mut b: usize) -> usize {
    while blocks[a].depth > blocks[b].depth {
        a = blocks[a].parent.expect("deeper than the body");
    }
    while blocks[b].depth > blocks[a].depth {
        b = blocks[b].parent.expect("deeper than the body");
    }
    while a != b {
        a = blocks[a].parent.expect("not the body");
        b = blocks[b].parent.expect("not the body");
    }
    a
}

/// Places every step, `real` saying which branch steps are real branches.
fn place(program: &Program, real: &[bool]) -> Placing {
    let count = program.steps.len();
    let mut blocks = vec![Block { parent: None, depth: 0 }];
    let mut of_step: Vec<Option<usize>> = vec![None; count];
    let mut sides = vec![None; count];
    for (_, id) in &program.outputs {
        of_step[id.0 as usize] = Some(0);
    }
    // Readers come after what they read, so going backwards every step's readers are placed before it is.
    let mut wanted: Vec<Option<usize>> = of_step.clone();
    for index in (0..count).rev() {
        let Some(block) = wanted[index] else { continue };
        of_step[index] = Some(block);
        let step = &program.steps[index];
        if real[index] {
            let depth = blocks[block].depth + 1;
            let yes = blocks.len();
            blocks.push(Block { parent: Some(block), depth });
            blocks.push(Block { parent: Some(block), depth });
            sides[index] = Some([yes, yes + 1]);
        }
        for (place, arg) in step.args.iter().enumerate() {
            let needed_in = match sides[index] {
                Some([yes, no]) if place > 0 => {
                    if place == 1 {
                        yes
                    } else {
                        no
                    }
                }
                _ => block,
            };
            let slot = &mut wanted[arg.0 as usize];
            *slot = Some(slot.map_or(needed_in, |other| common_block(&blocks, other, needed_in)));
        }
    }
    let of_step = of_step.into_iter().map(|block| block.unwrap_or(0)).collect();
    Placing { blocks, of_step, sides }
}

/// The work placed in each block, by the cost of its steps.
fn block_costs(program: &Program, placing: &Placing) -> Vec<(u32, u32)> {
    let mut costs = vec![(0, 0); placing.blocks.len()];
    for (index, step) in program.steps.iter().enumerate() {
        let (alu, samples) = cost::of(step, program);
        // A block's cost counts what is in the blocks inside it too.
        let mut block = Some(placing.of_step[index]);
        while let Some(b) = block {
            costs[b].0 += alu;
            costs[b].1 += samples;
            block = placing.blocks[b].parent;
        }
    }
    costs
}

/// Which of the program's Branch steps are worth real branches: those with
/// a side whose own work is worth skipping, settled by placing again until
/// nothing changes, as a branch made a select moves its sides' work out.
pub fn real_branches(program: &Program) -> Vec<bool> {
    let mut real: Vec<bool> = program.steps.iter().map(|step| step.op == Op::Select { branch: true }).collect();
    loop {
        let placing = place(program, &real);
        let costs = block_costs(program, &placing);
        let mut changed = false;
        for (index, step) in program.steps.iter().enumerate() {
            let Some(sides) = placing.sides[index] else { continue };
            let from = match program.step(step.args[0]).rate {
                Rate::Varying => VARYING_BRANCH_FROM,
                _ => UNIFORM_BRANCH_FROM,
            };
            let worth = sides.iter().any(|&side| costs[side].1 > 0 || costs[side].0 >= from);
            if !worth {
                real[index] = false;
                changed = true;
            }
        }
        if !changed {
            return real;
        }
    }
}

/// Writes numbers as Slang reads them.
fn number(value: f32) -> String {
    if value.is_finite() {
        let text = format!("{value:?}");
        if text.contains('.') || text.contains('e') { text } else { format!("{text}.0") }
    } else if value.is_nan() {
        "(0.0 / 0.0)".into()
    } else if value > 0.0 {
        "(1.0 / 0.0)".into()
    } else {
        "(-1.0 / 0.0)".into()
    }
}

fn constant(ty: Ty, value: [f32; 4]) -> String {
    match ty {
        Ty::Bool => (value[0] != 0.0).to_string(),
        Ty::Float => number(value[0]),
        _ => {
            let numbers: Vec<String> = value[..ty.width()].iter().map(|&v| number(v)).collect();
            format!("{}({})", ty.slang(), numbers.join(", "))
        }
    }
}

/// What a stage's functions call what they are given.
fn given(stage: Stage, given: Given) -> String {
    let surface = ["color", "alpha", "normal", "metallic", "roughness", "emission"];
    match given {
        Given::Surface(slot) => format!("surface.{}", surface[slot]),
        Given::Input(input) => {
            let field = match input {
                Input::Uv => "uv",
                Input::Position => "position",
                Input::Normal => "normal",
                Input::View => "view",
                Input::ObjectPosition => "objectPosition",
                Input::LevelPosition => "levelPosition",
                Input::Time => "time",
                Input::Thickness => "thickness",
                Input::LightDirection => "direction",
                Input::LightColor => "color",
                Input::LightView => "view",
            };
            let from = if stage == Stage::Light { "light" } else { "input" };
            format!("{from}.{field}")
        }
    }
}

struct Writer<'a> {
    graph: &'a Graph,
    program: &'a Program,
    stage: Stage,
    placing: Placing,
    text: String,
}

impl Writer<'_> {
    /// How step `id` is read where it is used.
    fn read(&self, id: ValueId) -> String {
        let step = self.program.step(id);
        match &step.op {
            Op::Const(value) => constant(step.ty, *value),
            Op::Given(what) => given(self.stage, *what),
            Op::Property(property) => {
                format!("props.{}", self.graph.property(*property).map_or("missing", |p| p.name.as_str()))
            }
            _ => format!("v{}", id.0),
        }
    }

    /// What step `index` works out, as an expression.
    fn expression(&self, step: &Step) -> String {
        let a = |n: usize| self.read(step.args[n]);
        let call = |name: &str| {
            format!("{name}({})", step.args.iter().map(|arg| self.read(*arg)).collect::<Vec<_>>().join(", "))
        };
        match &step.op {
            Op::Const(_) | Op::Given(_) | Op::Property(_) => unreachable!("written in place"),
            Op::Sample(property) => {
                let name = self.graph.property(*property).map_or("missing", |p| p.name.as_str());
                format!("props.{name}.sample({})", a(0))
            }
            Op::Unary(op) => match op {
                Unary::Negate => format!("-{}", a(0)),
                Unary::Abs => call("abs"),
                Unary::Floor => call("floor"),
                Unary::Ceil => call("ceil"),
                Unary::Frac => call("frac"),
                Unary::Sin => call("sin"),
                Unary::Cos => call("cos"),
                Unary::Sqrt => call("sqrt"),
                Unary::Exp => call("exp"),
                Unary::Saturate => call("saturate"),
                Unary::Sign => format!("{}(sign({}))", step.ty.slang(), a(0)),
                Unary::Normalize => call("normalize"),
                Unary::Length => call("length"),
                Unary::Random => call("graphRandom"),
            },
            Op::Binary(op) => match op {
                Binary::Add => format!("{} + {}", a(0), a(1)),
                Binary::Sub => format!("{} - {}", a(0), a(1)),
                Binary::Mul => format!("{} * {}", a(0), a(1)),
                Binary::Div => format!("{} / {}", a(0), a(1)),
                Binary::Min => call("min"),
                Binary::Max => call("max"),
                Binary::Pow => call("pow"),
                Binary::Step => call("step"),
                Binary::Dot => call("dot"),
                Binary::Cross => call("cross"),
                Binary::Distance => call("distance"),
                Binary::Reflect => call("reflect"),
                Binary::Compare(compare) => format!("{} {} {}", a(0), compare.slang(), a(1)),
            },
            Op::Ternary(op) => call(match op {
                Ternary::Lerp => "lerp",
                Ternary::Clamp => "clamp",
                Ternary::Smoothstep => "smoothstep",
            }),
            Op::Select { .. } => format!("{} ? {} : {}", a(0), a(1), a(2)),
            Op::Construct => call(step.ty.slang()),
            Op::Swizzle(picks) => {
                let letters: String = picks.iter().map(|&pick| ['x', 'y', 'z', 'w'][pick as usize]).collect();
                format!("{}.{letters}", a(0))
            }
        }
    }

    /// Writes the steps placed in `block`, at `indent`.
    fn block(&mut self, block: usize, indent: usize) {
        let pad = "    ".repeat(indent);
        for index in 0..self.program.steps.len() {
            if self.placing.of_step[index] != block {
                continue;
            }
            let step = &self.program.steps[index];
            if written_in_place(step) {
                continue;
            }
            let ty = step.ty.slang();
            match self.placing.sides[index] {
                Some([yes, no]) => {
                    let _ = writeln!(self.text, "{pad}{ty} v{index};");
                    let _ = writeln!(self.text, "{pad}if ({})\n{pad}{{", self.read(step.args[0]));
                    self.block(yes, indent + 1);
                    let _ = writeln!(
                        self.text,
                        "{pad}    v{index} = {};\n{pad}}}\n{pad}else\n{pad}{{",
                        self.read(step.args[1])
                    );
                    self.block(no, indent + 1);
                    let _ = writeln!(self.text, "{pad}    v{index} = {};\n{pad}}}", self.read(step.args[2]));
                }
                None => {
                    let expression = self.expression(step);
                    let _ = writeln!(self.text, "{pad}{ty} v{index} = {expression};");
                }
            }
        }
    }
}

/// The body of a stage's function, its outputs set at its end.
pub fn body(graph: &Graph, program: &Program, stage: Stage) -> String {
    let real = real_branches(program);
    let placing = place(program, &real);
    let mut writer = Writer { graph, program, stage, placing, text: String::new() };
    writer.block(0, 1);
    let surface = ["color", "alpha", "normal", "metallic", "roughness", "emission"];
    for &(slot, id) in &program.outputs {
        let value = writer.read(id);
        let _ = match stage {
            Stage::Surface => writeln!(writer.text, "    surface.{} = {value};", surface[slot]),
            Stage::Vertex | Stage::Light => writeln!(writer.text, "    return {value};"),
        };
    }
    writer.text
}

/// How many of the steps' branches are real.
pub fn real_count(program: &Program) -> usize {
    real_branches(program).iter().filter(|&&real| real).count()
}
