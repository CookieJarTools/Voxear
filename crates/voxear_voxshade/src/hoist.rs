//! Work a material's properties alone decide, worked out once rather than
//! at every pixel or vertex.
//!
//! A step whose rate is [`Rate::Material`] gives the same value everywhere
//! a material draws, until its properties change: the colour a tint and a
//! brightness make, a direction property made a unit long. Those read by
//! work that changes more often are picked as roots, the costliest first,
//! as many as the material has room left for after its own properties. The
//! engine works each root out on the CPU when the material's properties
//! are set (see [`Derived`]) and hands it to the shader as a hidden
//! property, so the GPU reads it rather than working it out; the steps only
//! a root needs are left out of the Slang altogether.

use std::collections::HashMap;

use crate::{
    cost,
    eval::{Inputs, run},
    graph::PropertyId,
    ir::{Op, Program, Rate, ValueId},
    types::Ty,
};

/// How many values a material holds for its shader on the GPU: its own
/// properties, and what is worked out from them (`MATERIAL_PROPERTIES`).
pub const SLOTS: usize = 16;

/// The steps of `program` to work out on the CPU, at most `room` of them,
/// in the order they run.
pub fn roots(program: &Program, room: usize) -> Vec<ValueId> {
    let count = program.steps.len();
    // The highest rate each step is read at, and whether an output reads it.
    let mut read_at: Vec<Option<Rate>> = vec![None; count];
    for step in &program.steps {
        for arg in &step.args {
            let slot = &mut read_at[arg.0 as usize];
            *slot = Some(slot.map_or(step.rate, |rate| rate.max(step.rate)));
        }
    }
    for (_, id) in &program.outputs {
        read_at[id.0 as usize] = Some(Rate::Varying);
    }
    // What working each step out takes, with the material's work under it.
    let mut under = vec![0u32; count];
    for (index, step) in program.steps.iter().enumerate() {
        let own = cost::of(step, program).0;
        let below: u32 = step
            .args
            .iter()
            .filter(|arg| program.step(**arg).rate == Rate::Material)
            .map(|arg| under[arg.0 as usize])
            .sum();
        under[index] = own + below;
    }
    let mut picked: Vec<(u32, ValueId)> = program
        .steps
        .iter()
        .enumerate()
        .filter(|(index, step)| {
            step.rate == Rate::Material
                && step.ty != Ty::Bool
                && !matches!(step.op, Op::Const(_) | Op::Property(_))
                && read_at[*index].is_some_and(|rate| rate > Rate::Material)
                && under[*index] > 0
        })
        .map(|(index, _)| (under[index], ValueId(index as u32)))
        .collect();
    picked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    picked.truncate(room);
    let mut roots: Vec<ValueId> = picked.into_iter().map(|(_, id)| id).collect();
    roots.sort();
    roots
}

/// What a shader works out on the CPU from a material's properties: each
/// stage's program and the roots of it it hands on, in order, each a
/// hidden property after the shader's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Derived {
    pub runs: Vec<(Program, Vec<ValueId>)>,
    /// The graph's properties, in the order a material holds their values.
    pub properties: Vec<PropertyId>,
}

impl Derived {
    /// How many values it hands on, and of what types.
    pub fn types(&self) -> Vec<Ty> {
        self.runs.iter().flat_map(|(program, roots)| roots.iter().map(|id| program.step(*id).ty)).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.iter().all(|(_, roots)| roots.is_empty())
    }

    /// The values it hands on, as the GPU holds them, worked out from the
    /// material's own: `values[n]` the GPU's value of property `n`, a
    /// colour in linear light.
    pub fn values(&self, values: &[[f32; 4]]) -> Vec<[f32; 4]> {
        let place: HashMap<PropertyId, usize> = self.properties.iter().enumerate().map(|(n, id)| (*id, n)).collect();
        let property = |id: PropertyId| place.get(&id).and_then(|&n| values.get(n)).copied().unwrap_or([0.0; 4]);
        let inputs = Inputs { given: &|_| [0.0; 4], property: &property, sample: &|_, _| [1.0; 4] };
        let mut worked = Vec::new();
        let mut out = Vec::new();
        for (program, roots) in &self.runs {
            run(program, &inputs, &mut worked);
            out.extend(roots.iter().map(|id| worked[id.0 as usize]));
        }
        out
    }
}
