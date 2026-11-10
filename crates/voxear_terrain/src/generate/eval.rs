//! Running a compiled graph: its steps in order over a batch of places,
//! each filling one row of numbers from the rows before it.
//!
//! Places are taken a block at a time, so the rows stay small enough to
//! sit in a core's cache whatever is asked for. A step works each place
//! out by itself from the same numbers in the same order whichever block
//! it falls in, so one place alone gets the bits it gets among thousands.

use std::{cell::Cell, sync::Arc};

use super::compile::{Kernel, V};

/// How many places are worked out at a time.
pub(crate) const BLOCK: usize = 512;

/// The most rows one step reads.
pub(crate) const MOST_INPUTS: usize = 8;

/// What a program is given for every place it is run over: where the place
/// is along each axis, and how high the ground asked about is there. A
/// program run once a cell is also given how large the cells asked about
/// are, in metres, the radius of the planet they are round, and the rows
/// carried up to each cell from its column (see `compile::Carry`), by
/// their place among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Given {
    X,
    Y,
    Z,
    Ground,
    Size,
    Radius,
    Carried(u8),
}

impl Given {
    /// What tells it from every other, in a step's key. The first four are
    /// as they were before programs ran once a cell, so the hash of a
    /// generator with no Volume is what it was.
    pub fn code(self) -> Vec<u8> {
        match self {
            Given::X => vec![0],
            Given::Y => vec![1],
            Given::Z => vec![2],
            Given::Ground => vec![3],
            Given::Size => vec![4],
            Given::Radius => vec![5],
            Given::Carried(row) => vec![6, row],
        }
    }
}

/// One step of a program: a row copied from what it is given, or filled by
/// a kernel from rows before it.
#[derive(Debug)]
pub(crate) enum Step {
    Given(Given),
    Run { kernel: Arc<dyn Kernel>, inputs: Vec<V> },
}

/// What a program is run over: each place's `x`, `y` and `z` and the
/// ground's height there, as many of each as there are places, or none of
/// one the program does not read; one number alone stands for every place,
/// as a column's `x` and `z` do for each cell up it. For cells, also the
/// rows carried up from their columns, the cells' size and their planet's
/// radius.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Givens<'a> {
    pub x: &'a [f32],
    pub y: &'a [f32],
    pub z: &'a [f32],
    pub ground: &'a [f32],
    pub carried: &'a [&'a [f32]],
    pub size: f32,
    pub radius: f32,
}

/// A compiled graph: its steps in the order they run, the rows that are
/// what it makes, and a hash of all that decides what it makes.
#[derive(Debug)]
pub(crate) struct Program {
    pub steps: Vec<Step>,
    pub outputs: Vec<V>,
    pub hash: u64,
}

thread_local! {
    /// The rows a thread runs programs in, kept between runs.
    static ROWS: Cell<Vec<Vec<f32>>> = const { Cell::new(Vec::new()) };
}

impl Program {
    /// Whether any of its steps reads `given`.
    pub fn reads(&self, given: Given) -> bool {
        self.steps.iter().any(|step| matches!(step, Step::Given(read) if *read == given))
    }

    /// The steps `outputs` of its rows are made by and no others, in its
    /// order: a program making those alone.
    pub fn only(&self, outputs: &[V]) -> Program {
        let mut needed = vec![false; self.steps.len()];
        for &output in outputs {
            needed[output as usize] = true;
        }
        for at in (0..self.steps.len()).rev() {
            if let (true, Step::Run { inputs, .. }) = (needed[at], &self.steps[at]) {
                for &input in inputs {
                    needed[input as usize] = true;
                }
            }
        }
        let mut moved = vec![0 as V; self.steps.len()];
        let mut steps = Vec::new();
        for (at, step) in self.steps.iter().enumerate().filter(|(at, _)| needed[*at]) {
            moved[at] = steps.len() as V;
            steps.push(match step {
                Step::Given(given) => Step::Given(*given),
                Step::Run { kernel, inputs } => Step::Run {
                    kernel: kernel.clone(),
                    inputs: inputs.iter().map(|&input| moved[input as usize]).collect(),
                },
            });
        }
        Program { steps, outputs: outputs.iter().map(|&output| moved[output as usize]).collect(), hash: self.hash }
    }

    /// Runs it over `count` places, filling each of `outs` with one of what
    /// it makes, in the order of its outputs.
    pub fn run(&self, count: usize, given: Givens, outs: &mut [&mut [f32]]) {
        let mut rows = ROWS.take();
        if rows.len() < self.steps.len() {
            rows.resize_with(self.steps.len(), || vec![0.0; BLOCK]);
        }
        let mut start = 0;
        while start < count {
            let len = (count - start).min(BLOCK);
            for (at, step) in self.steps.iter().enumerate() {
                let (before, rest) = rows.split_at_mut(at);
                let out = &mut rest[0][..len];
                match step {
                    Step::Given(which) => {
                        let all = match *which {
                            Given::X => given.x,
                            Given::Y => given.y,
                            Given::Z => given.z,
                            Given::Ground => given.ground,
                            Given::Size => std::slice::from_ref(&given.size),
                            Given::Radius => std::slice::from_ref(&given.radius),
                            Given::Carried(row) => given.carried.get(usize::from(row)).copied().unwrap_or(&[]),
                        };
                        match (all, all.get(start..start + len)) {
                            ([one], _) => out.fill(*one),
                            (_, Some(part)) => out.copy_from_slice(part),
                            (_, None) => out.fill(0.0),
                        }
                    }
                    Step::Run { kernel, inputs } => {
                        let mut read: [&[f32]; MOST_INPUTS] = [&[]; MOST_INPUTS];
                        for (slot, &input) in read.iter_mut().zip(inputs) {
                            *slot = &before[input as usize][..len];
                        }
                        kernel.run(&read[..inputs.len().min(MOST_INPUTS)], out);
                    }
                }
            }
            for (out, &row) in outs.iter_mut().zip(&self.outputs) {
                out[start..start + len].copy_from_slice(&rows[row as usize][..len]);
            }
            start += len;
        }
        ROWS.set(rows);
    }
}
