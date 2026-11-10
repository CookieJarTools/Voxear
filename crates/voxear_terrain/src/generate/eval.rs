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
/// is along each axis, and how high the ground asked about is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Given {
    X,
    Y,
    Z,
    Ground,
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
/// one the program does not read.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Givens<'a> {
    pub x: &'a [f32],
    pub y: &'a [f32],
    pub z: &'a [f32],
    pub ground: &'a [f32],
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
                        let all = match which {
                            Given::X => given.x,
                            Given::Y => given.y,
                            Given::Z => given.z,
                            Given::Ground => given.ground,
                        };
                        match all.get(start..start + len) {
                            Some(part) => out.copy_from_slice(part),
                            None => out.fill(0.0),
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
