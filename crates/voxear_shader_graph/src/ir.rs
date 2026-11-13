//! The form a graph is compiled into before any Slang is written: a list
//! of steps, each made once from steps before it, every value typed.
//!
//! Steps are made through a [`Builder`], which works out what it can as
//! each is asked for: a sum of numbers known now is the number it comes to
//! (constant folding), work that changes nothing goes (adding nothing,
//! times one, the saturate of a saturate), and a step asked for again with
//! the same inputs is the step already made (common work merged). So by the
//! time a graph is lowered, nodes that came to nothing have no steps, and
//! two nodes doing the same work share them. What no output reads is left
//! out when the program is finished ([`Program::finish`]).

use std::collections::HashMap;

use crate::{
    graph::{NodeId, PropertyId},
    nodes::{CompareOp, Input, Stage},
    types::Ty,
};

/// A step's place in its program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

/// What a shader's surface, vertex or light function is given, as its steps read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Given {
    Input(Input),
    /// One of the surface's values as it stands, by its place in `SURFACE_OUTPUTS`.
    Surface(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unary {
    Negate,
    Abs,
    Floor,
    Ceil,
    Frac,
    Sin,
    Cos,
    Sqrt,
    Exp,
    Saturate,
    Sign,
    Normalize,
    Length,
    /// A number from 0 to 1 from the whole-number bits of its seed.
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
    Pow,
    Step,
    Dot,
    Cross,
    Distance,
    Reflect,
    Compare(CompareOp),
}

impl Binary {
    /// Whether its two inputs can change places.
    fn commutes(self) -> bool {
        matches!(self, Self::Add | Self::Mul | Self::Min | Self::Max | Self::Dot | Self::Distance)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ternary {
    Lerp,
    Clamp,
    Smoothstep,
}

/// What a step does.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Numbers known now, as many as its type is wide; a truth as 0 or 1.
    Const([f32; 4]),
    Given(Given),
    Property(PropertyId),
    /// A picture property read at the place its one input says.
    Sample(PropertyId),
    Unary(Unary),
    Binary(Binary),
    Ternary(Ternary),
    /// One of two values, as its first input holds. `branch` when it came
    /// from a Branch node, and may be worked out as a real branch.
    Select {
        branch: bool,
    },
    /// A vector made of its inputs' numbers, in order.
    Construct,
    /// Numbers of its input picked, by place.
    Swizzle(Vec<u8>),
}

/// How often a value changes: never; once a draw, as a property or the
/// time; or from one place to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rate {
    Constant,
    Uniform,
    Varying,
}

/// One step: what it does, the steps it reads, its type, how often it
/// changes, and the node it was made for, to count its cost against.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub op: Op,
    pub args: Vec<ValueId>,
    pub ty: Ty,
    pub rate: Rate,
    pub node: Option<NodeId>,
}

/// A stage's steps in the order they run, and what its outputs read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    pub steps: Vec<Step>,
    /// Each output the stage sets, by its place among the output node's
    /// inputs, and the step whose value it takes.
    pub outputs: Vec<(usize, ValueId)>,
}

impl Program {
    pub fn step(&self, id: ValueId) -> &Step {
        &self.steps[id.0 as usize]
    }

    /// The program with only the steps an output reads, in the same order.
    pub fn finish(self) -> Self {
        let mut used = vec![false; self.steps.len()];
        for (_, id) in &self.outputs {
            used[id.0 as usize] = true;
        }
        for index in (0..self.steps.len()).rev() {
            if used[index] {
                for arg in &self.steps[index].args {
                    used[arg.0 as usize] = true;
                }
            }
        }
        let mut renumber = vec![ValueId(u32::MAX); self.steps.len()];
        let mut steps = Vec::new();
        for (index, step) in self.steps.into_iter().enumerate() {
            if used[index] {
                renumber[index] = ValueId(steps.len() as u32);
                let args = step.args.iter().map(|arg| renumber[arg.0 as usize]).collect();
                steps.push(Step { args, ..step });
            }
        }
        let outputs = self.outputs.into_iter().map(|(slot, id)| (slot, renumber[id.0 as usize])).collect();
        Self { steps, outputs }
    }
}

/// What tells two steps apart, to the bit: the same key is the same step.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    op: String,
    args: Vec<ValueId>,
    ty: Ty,
}

/// Makes a program's steps, folding, simplifying and merging as it goes.
pub struct Builder {
    pub stage: Stage,
    program: Program,
    made: HashMap<Key, ValueId>,
    /// The node steps are being made for now.
    pub node: Option<NodeId>,
}

impl Builder {
    pub fn new(stage: Stage) -> Self {
        Self { stage, program: Program::default(), made: HashMap::new(), node: None }
    }

    pub fn output(&mut self, slot: usize, value: ValueId) {
        self.program.outputs.push((slot, value));
    }

    pub fn finish(self) -> Program {
        self.program.finish()
    }

    pub fn ty(&self, id: ValueId) -> Ty {
        self.program.step(id).ty
    }

    /// The numbers a step comes to, when it is known now.
    pub fn known(&self, id: ValueId) -> Option<[f32; 4]> {
        match self.program.step(id).op {
            Op::Const(value) => Some(value),
            _ => None,
        }
    }

    pub fn constant(&mut self, ty: Ty, value: [f32; 4]) -> ValueId {
        let mut kept = [0.0; 4];
        kept[..ty.width()].copy_from_slice(&value[..ty.width()]);
        self.push(Op::Const(kept), Vec::new(), ty)
    }

    pub fn number(&mut self, value: f32) -> ValueId {
        self.constant(Ty::Float, [value; 4])
    }

    pub fn given(&mut self, given: Given, ty: Ty) -> ValueId {
        self.push(Op::Given(given), Vec::new(), ty)
    }

    /// `value` as `ty`: a single number spread to every place, a wider value
    /// cut short, a narrower one filled out with zeros.
    pub fn convert(&mut self, value: ValueId, ty: Ty) -> ValueId {
        let from = self.ty(value);
        if from == ty || ty == Ty::Bool || from == Ty::Bool {
            return value;
        }
        if let Some(known) = self.known(value) {
            let numbers = if from == Ty::Float { [known[0]; 4] } else { known };
            return self.constant(ty, numbers);
        }
        let (from_width, to_width) = (from.width(), ty.width());
        if from_width == 1 {
            return self.push(Op::Construct, vec![value; to_width], ty);
        }
        if to_width < from_width {
            return self.push(Op::Swizzle((0..to_width as u8).collect()), vec![value], ty);
        }
        let zero = self.number(0.0);
        let mut parts: Vec<ValueId> = (0..from_width).map(|i| self.extract(value, i)).collect();
        parts.resize(to_width, zero);
        self.push(Op::Construct, parts, ty)
    }

    /// Number `index` of `value`.
    pub fn extract(&mut self, value: ValueId, index: usize) -> ValueId {
        if self.ty(value) == Ty::Float {
            return value;
        }
        if let Some(known) = self.known(value) {
            return self.number(known[index]);
        }
        // Picking from a vector just made picks what went into it.
        let step = self.program.step(value);
        if step.op == Op::Construct {
            return step.args[index];
        }
        self.push(Op::Swizzle(vec![index as u8]), vec![value], Ty::Float)
    }

    pub fn swizzle(&mut self, value: ValueId, picks: Vec<u8>) -> ValueId {
        let ty = Ty::of_width(picks.len());
        let width = self.ty(value).width();
        let picks: Vec<u8> = picks.into_iter().map(|pick| pick.min(width as u8 - 1)).collect();
        if picks.len() == width && picks.iter().enumerate().all(|(i, &pick)| i == pick as usize) {
            return value;
        }
        if picks.len() == 1 {
            return self.extract(value, picks[0] as usize);
        }
        if let Some(known) = self.known(value) {
            let mut numbers = [0.0; 4];
            for (i, &pick) in picks.iter().enumerate() {
                numbers[i] = known[pick as usize];
            }
            return self.constant(ty, numbers);
        }
        self.push(Op::Swizzle(picks), vec![value], ty)
    }

    pub fn construct(&mut self, parts: Vec<ValueId>, ty: Ty) -> ValueId {
        if parts.iter().all(|part| self.known(*part).is_some()) {
            let mut numbers = [0.0; 4];
            for (i, part) in parts.iter().enumerate() {
                numbers[i] = self.known(*part).expect("checked above")[0];
            }
            return self.constant(ty, numbers);
        }
        // Putting back together what was taken apart.
        let whole = parts.first().and_then(|first| match &self.program.step(*first).op {
            Op::Swizzle(picks) if picks.len() == 1 && picks[0] == 0 => Some(self.program.step(*first).args[0]),
            _ => None,
        });
        if let Some(whole) = whole.filter(|whole| self.ty(*whole) == ty)
            && parts.iter().enumerate().all(|(i, part)| {
                let step = self.program.step(*part);
                step.op == Op::Swizzle(vec![i as u8]) && step.args[0] == whole
            })
        {
            return whole;
        }
        self.push(Op::Construct, parts, ty)
    }

    pub fn property(&mut self, property: PropertyId, ty: Ty) -> ValueId {
        self.push(Op::Property(property), Vec::new(), ty)
    }

    pub fn sample(&mut self, property: PropertyId, uv: ValueId) -> ValueId {
        self.push(Op::Sample(property), vec![uv], Ty::Float4)
    }

    pub fn unary(&mut self, op: Unary, a: ValueId) -> ValueId {
        let ty = match op {
            Unary::Length | Unary::Random => Ty::Float,
            _ => self.ty(a),
        };
        if let Some(value) = self.known(a) {
            return self.constant(ty, fold_unary(op, value, self.ty(a).width()));
        }
        let inner = self.program.step(a);
        match (op, &inner.op) {
            (Unary::Saturate, Op::Unary(Unary::Saturate))
            | (Unary::Abs, Op::Unary(Unary::Abs))
            | (Unary::Floor, Op::Unary(Unary::Floor))
            | (Unary::Normalize, Op::Unary(Unary::Normalize)) => return a,
            (Unary::Negate, Op::Unary(Unary::Negate)) => return inner.args[0],
            _ => {}
        }
        self.push(Op::Unary(op), vec![a], ty)
    }

    pub fn binary(&mut self, op: Binary, a: ValueId, b: ValueId) -> ValueId {
        let ty = match op {
            Binary::Dot | Binary::Distance => Ty::Float,
            Binary::Cross => Ty::Float3,
            Binary::Compare(_) => Ty::Bool,
            _ => self.ty(a),
        };
        let width = self.ty(a).width();
        if let (Some(x), Some(y)) = (self.known(a), self.known(b)) {
            return self.constant(ty, fold_binary(op, x, y, width));
        }
        let is = |id: ValueId, n: f32| self.known(id).is_some_and(|v| v[..width].iter().all(|&c| c == n));
        match op {
            Binary::Add if is(b, 0.0) => return a,
            Binary::Add if is(a, 0.0) => return b,
            Binary::Sub if is(b, 0.0) => return a,
            Binary::Mul | Binary::Div if is(b, 1.0) => return a,
            Binary::Mul if is(a, 1.0) => return b,
            Binary::Mul if is(a, 0.0) || is(b, 0.0) => return self.constant(ty, [0.0; 4]),
            Binary::Pow if is(b, 1.0) => return a,
            Binary::Pow if is(b, 2.0) => return self.binary(Binary::Mul, a, a),
            Binary::Sub if a == b => return self.constant(ty, [0.0; 4]),
            Binary::Min | Binary::Max if a == b => return a,
            _ => {}
        }
        let (a, b) = if op.commutes() && b < a { (b, a) } else { (a, b) };
        self.push(Op::Binary(op), vec![a, b], ty)
    }

    pub fn ternary(&mut self, op: Ternary, a: ValueId, b: ValueId, c: ValueId) -> ValueId {
        let ty = self.ty(if op == Ternary::Smoothstep { c } else { a });
        let width = ty.width();
        if let (Some(x), Some(y), Some(z)) = (self.known(a), self.known(b), self.known(c)) {
            return self.constant(ty, fold_ternary(op, x, y, z, width));
        }
        let is = |id: ValueId, n: f32| self.known(id).is_some_and(|v| v[..self.ty(id).width()].iter().all(|&c| c == n));
        match op {
            Ternary::Lerp if is(c, 0.0) || a == b => return a,
            Ternary::Lerp if is(c, 1.0) => return b,
            Ternary::Clamp if is(b, 0.0) && is(c, 1.0) => return self.unary(Unary::Saturate, a),
            _ => {}
        }
        self.push(Op::Ternary(op), vec![a, b, c], ty)
    }

    /// `yes` where `condition` holds, else `no`; `branch` as for [`Op::Select`].
    pub fn select(&mut self, condition: ValueId, yes: ValueId, no: ValueId, branch: bool) -> ValueId {
        if let Some(known) = self.known(condition) {
            return if known[0] != 0.0 { yes } else { no };
        }
        if yes == no {
            return yes;
        }
        let ty = self.ty(yes);
        self.push(Op::Select { branch }, vec![condition, yes, no], ty)
    }

    /// A step, or the one already made that does the same.
    fn push(&mut self, op: Op, args: Vec<ValueId>, ty: Ty) -> ValueId {
        let key = Key { op: key_of(&op), args: args.clone(), ty };
        if let Some(&made) = self.made.get(&key) {
            return made;
        }
        let rate = match &op {
            Op::Const(_) => Rate::Constant,
            Op::Property(_) => Rate::Uniform,
            Op::Given(Given::Input(input)) if input.is_uniform() => Rate::Uniform,
            Op::Given(_) => Rate::Varying,
            _ => args.iter().map(|arg| self.program.step(*arg).rate).max().unwrap_or(Rate::Constant),
        };
        let id = ValueId(self.program.steps.len() as u32);
        self.program.steps.push(Step { op, args, ty, rate, node: self.node });
        self.made.insert(key, id);
        id
    }
}

/// What tells an op from every other, its numbers to the bit.
fn key_of(op: &Op) -> String {
    match op {
        Op::Const(value) => format!("const {:?}", value.map(f32::to_bits)),
        other => format!("{other:?}"),
    }
}

fn per_component(width: usize, f: impl Fn(usize) -> f32) -> [f32; 4] {
    let mut out = [0.0; 4];
    for (i, slot) in out.iter_mut().enumerate().take(width) {
        *slot = f(i);
    }
    out
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A whole-number hash of a number's bits, as the shaders' `graphRandom` does it.
pub fn random(seed: f32) -> f32 {
    let mut state = seed.to_bits().wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    state = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    state = (state >> 22) ^ state;
    state as f32 / 4_294_967_296.0
}

fn length(v: [f32; 4], width: usize) -> f32 {
    v[..width].iter().map(|c| c * c).sum::<f32>().sqrt()
}

fn fold_unary(op: Unary, a: [f32; 4], width: usize) -> [f32; 4] {
    let each = |f: fn(f32) -> f32| per_component(width, |i| f(a[i]));
    match op {
        Unary::Negate => each(|x| -x),
        Unary::Abs => each(f32::abs),
        Unary::Floor => each(f32::floor),
        Unary::Ceil => each(f32::ceil),
        Unary::Frac => each(|x| x - x.floor()),
        Unary::Sin => each(f32::sin),
        Unary::Cos => each(f32::cos),
        Unary::Sqrt => each(f32::sqrt),
        Unary::Exp => each(f32::exp),
        Unary::Saturate => each(|x| x.clamp(0.0, 1.0)),
        Unary::Sign => each(|x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }),
        Unary::Normalize => {
            let l = length(a, width);
            per_component(width, |i| if l > 0.0 { a[i] / l } else { 0.0 })
        }
        Unary::Length => [length(a, width); 4],
        Unary::Random => [random(a[0]); 4],
    }
}

fn fold_binary(op: Binary, a: [f32; 4], b: [f32; 4], width: usize) -> [f32; 4] {
    let each = |f: fn(f32, f32) -> f32| per_component(width, |i| f(a[i], b[i]));
    match op {
        Binary::Add => each(|x, y| x + y),
        Binary::Sub => each(|x, y| x - y),
        Binary::Mul => each(|x, y| x * y),
        Binary::Div => each(|x, y| x / y),
        Binary::Min => each(f32::min),
        Binary::Max => each(f32::max),
        Binary::Pow => each(f32::powf),
        Binary::Step => each(|edge, x| if x >= edge { 1.0 } else { 0.0 }),
        Binary::Dot => [(0..width).map(|i| a[i] * b[i]).sum(); 4],
        Binary::Distance => [length(per_component(width, |i| a[i] - b[i]), width); 4],
        Binary::Cross => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0], 0.0],
        Binary::Reflect => {
            let d: f32 = (0..width).map(|i| a[i] * b[i]).sum();
            per_component(width, |i| a[i] - 2.0 * d * b[i])
        }
        Binary::Compare(compare) => [if compare.holds(a[0], b[0]) { 1.0 } else { 0.0 }; 4],
    }
}

fn fold_ternary(op: Ternary, a: [f32; 4], b: [f32; 4], c: [f32; 4], width: usize) -> [f32; 4] {
    match op {
        Ternary::Lerp => per_component(width, |i| a[i] + (b[i] - a[i]) * c[i]),
        Ternary::Clamp => per_component(width, |i| a[i].max(b[i]).min(c[i])),
        Ternary::Smoothstep => per_component(width, |i| smoothstep(a[i], b[i], c[i])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_known_now_are_worked_out_and_work_that_changes_nothing_goes() {
        let mut build = Builder::new(Stage::Surface);
        let (two, three) = (build.number(2.0), build.number(3.0));
        let six = build.binary(Binary::Mul, two, three);
        assert_eq!(build.known(six).map(|value| value[0]), Some(6.0));
        let uv = build.given(Given::Input(Input::Uv), Ty::Float2);
        let one = build.constant(Ty::Float2, [1.0; 4]);
        assert_eq!(build.binary(Binary::Mul, uv, one), uv, "times one");
        let zero = build.constant(Ty::Float2, [0.0; 4]);
        assert_eq!(build.binary(Binary::Add, zero, uv), uv, "plus nothing");
        let saturated = build.unary(Unary::Saturate, uv);
        assert_eq!(build.unary(Unary::Saturate, saturated), saturated);
        let exponent = build.constant(Ty::Float2, [2.0; 4]);
        let squared = build.binary(Binary::Pow, uv, exponent);
        assert_eq!(build.program.step(squared).op, Op::Binary(Binary::Mul), "squared is times itself");
    }

    #[test]
    fn the_same_work_asked_for_twice_is_one_step_and_unread_steps_are_left_out() {
        let mut build = Builder::new(Stage::Surface);
        let uv = build.given(Given::Input(Input::Uv), Ty::Float2);
        let time = build.given(Given::Input(Input::Time), Ty::Float);
        let x = build.extract(uv, 0);
        let first = build.binary(Binary::Add, x, time);
        let second = build.binary(Binary::Add, time, x);
        assert_eq!(first, second, "either way round");
        let sin = build.unary(Unary::Sin, first);
        let unread = build.unary(Unary::Cos, first);
        build.output(0, sin);
        let program = build.finish();
        assert!(!program.steps.iter().any(|step| step.op == Op::Unary(Unary::Cos)), "{unread:?} is read by nothing");
        assert_eq!(program.steps.len(), 5, "uv, its x, the time, the sum and its sine: {:?}", program.steps);
        let time = program.steps.iter().find(|step| step.op == Op::Given(Given::Input(Input::Time))).unwrap();
        assert_eq!(time.rate, Rate::Uniform, "the time is the same all over a draw");
        assert_eq!(program.steps[4].rate, Rate::Varying);
    }

    #[test]
    fn a_vector_taken_apart_and_put_back_together_is_itself() {
        let mut build = Builder::new(Stage::Surface);
        let position = build.given(Given::Input(Input::Position), Ty::Float3);
        let parts: Vec<ValueId> = (0..3).map(|i| build.extract(position, i)).collect();
        assert_eq!(build.construct(parts.clone(), Ty::Float3), position);
        let made = build.construct(vec![parts[2], parts[0], parts[1]], Ty::Float3);
        assert_eq!(build.extract(made, 0), parts[2], "a part of what was just made");
    }
}
