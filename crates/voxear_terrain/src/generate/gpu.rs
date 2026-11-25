//! A generator's programs for the GPU: the steps that make the ground's
//! height, its climate and its bare rock, a column at a time, as words a
//! shader reads and runs (`voxear_render/shaders/generator.slang`).
//!
//! Each step is a kernel saying how it runs there ([`Kernel::gpu`]): what it
//! does, as one of the shader's operations, and its numbers. The height's
//! program runs first, and the climate's and the rock's after it, given the
//! height it made. Each step's row is kept in a slot of the shader's, and a
//! slot is used again once nothing after reads what is in it, so a graph of
//! a few hundred steps runs in a few dozen.
//!
//! A kernel the GPU cannot run makes no program at all: the generator says
//! which ([`GpuProgram::unsupported`]). A few run there in part, as a
//! Rivers node passing its ground through without the rivers it would cut
//! (see [`GpuProgram::left_out`]).

use voxear_core::curve::Curve;

use super::{
    compile::Kernel,
    eval::{Given, Program, Step},
    world::WorldGen,
};

/// The most slots a program's rows are kept in on the GPU.
pub const GPU_SLOTS: usize = 48;

/// What each step does on the GPU: the shader's operations, numbered as it
/// numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum GpuOp {
    X = 0,
    Z = 1,
    /// A row as it is, from the input its one number names.
    Copy = 2,
    Constant = 3,
    Math = 4,
    Noise = 5,
    Curve = 6,
    Warp = 7,
    Shift = 8,
    Gullies = 9,
    Clamp = 10,
    Blend = 11,
    Terrace = 12,
    HeightMask = 13,
    Cells = 14,
    Warmth = 15,
    Wetness = 16,
    Mild = 17,
    Landmass = 18,
    Falloff = 19,
}

/// One step as a kernel writes it for the GPU: its operation and numbers.
#[derive(Debug)]
pub(crate) struct GpuStep {
    pub op: GpuOp,
    pub words: Vec<u32>,
    /// What it leaves out of what it does on the CPU, if anything.
    pub left_out: Option<&'static str>,
}

impl GpuStep {
    pub fn new(op: GpuOp) -> Self {
        Self { op, words: Vec::new(), left_out: None }
    }

    pub fn number(mut self, number: f32) -> Self {
        self.words.push(number.to_bits());
        self
    }

    pub fn whole(mut self, whole: u32) -> Self {
        self.words.push(whole);
        self
    }

    /// A curve: how many keys, then each key's place and value.
    pub fn curve(mut self, curve: &Curve) -> Self {
        self.words.push(curve.keys().len() as u32);
        for [place, value] in curve.keys() {
            self.words.extend([place.to_bits(), value.to_bits()]);
        }
        self
    }

    pub fn leaving_out(mut self, what: &'static str) -> Self {
        self.left_out = Some(what);
        self
    }
}

/// A generator's programs as the GPU runs them, a column at a time.
///
/// `words` holds each step in turn: its operation, how many rows it reads
/// and the slots they are in, the slot it writes, how many numbers it has
/// and the numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuProgram {
    pub words: Vec<u32>,
    pub steps: u32,
    /// How many slots its rows take.
    pub slots: u32,
    /// The slots that hold the ground's height in metres, how warm it is in
    /// °C, how wet from 0 to 1, and the rock mask (`u32::MAX` for none).
    pub height: u32,
    pub warmth: u32,
    pub wetness: u32,
    pub rock: u32,
    /// What its kernels leave out of the ground the CPU makes, by name.
    pub left_out: Vec<&'static str>,
}

impl WorldGen {
    /// Its height, climate and rock as one program the GPU runs a column at
    /// a time; or the names of the kernels it could not run there.
    pub fn gpu_program(&self) -> Result<GpuProgram, Vec<String>> {
        let compiled = self.compiled();
        let mut parts: Vec<&Program> = vec![&compiled.height, &compiled.climate];
        if let Some(rock) = &compiled.rock {
            parts.push(rock);
        }
        // Every step of every part in one list, the climate's and the rock's reading the height made first.
        let mut unsupported = Vec::new();
        let mut steps: Vec<(GpuStep, Vec<u32>)> = Vec::new();
        let mut outputs = Vec::new();
        let mut height = 0;
        for (part, program) in parts.iter().enumerate() {
            let start = steps.len() as u32;
            for step in &program.steps {
                let made = match step {
                    Step::Given(Given::X) => (GpuStep::new(GpuOp::X), Vec::new()),
                    Step::Given(Given::Z) => (GpuStep::new(GpuOp::Z), Vec::new()),
                    Step::Given(Given::Ground) if part > 0 => (GpuStep::new(GpuOp::Copy).whole(0), vec![height]),
                    Step::Given(given) => {
                        unsupported.push(format!("{given:?}"));
                        (GpuStep::new(GpuOp::Constant).number(0.0), Vec::new())
                    }
                    Step::Run { kernel, inputs } => {
                        let inputs = inputs.iter().map(|&input| input + start).collect();
                        match kernel.gpu() {
                            Some(step) => (step, inputs),
                            None => {
                                unsupported.push(name_of(kernel.as_ref()));
                                (GpuStep::new(GpuOp::Constant).number(0.0), inputs)
                            }
                        }
                    }
                };
                steps.push(made);
            }
            let made: Vec<u32> = program.outputs.iter().map(|&output| output + start).collect();
            if part == 0 {
                height = made[0];
            }
            outputs.extend(made);
        }
        if !unsupported.is_empty() {
            unsupported.sort();
            unsupported.dedup();
            return Err(unsupported);
        }
        // Its Volume is asked a cell at a time, on the CPU alone: on the GPU the ground is its height made solid.
        let mut left_out = Vec::new();
        if compiled.volume.is_some() {
            left_out.push("overhangs and caves");
        }
        let rock = compiled.rock.as_ref().map(|_| outputs[3]);
        let kept = [Some(outputs[0]), Some(outputs[1]), Some(outputs[2]), rock];
        let (slot_of, slots) = slots(&steps, &kept.iter().flatten().copied().collect::<Vec<_>>());
        if slots > GPU_SLOTS as u32 {
            return Err(vec![format!("{slots} rows at once, of {GPU_SLOTS}")]);
        }
        let mut words = Vec::new();
        for (at, (step, inputs)) in steps.iter().enumerate() {
            words.push(step.op as u32);
            words.push(inputs.len() as u32);
            words.extend(inputs.iter().map(|&input| slot_of[input as usize]));
            words.push(slot_of[at]);
            words.push(step.words.len() as u32);
            words.extend(&step.words);
            if let Some(what) = step.left_out
                && !left_out.contains(&what)
            {
                left_out.push(what);
            }
        }
        let slot = |row: u32| slot_of[row as usize];
        Ok(GpuProgram {
            words,
            steps: steps.len() as u32,
            slots,
            height: slot(outputs[0]),
            warmth: slot(outputs[1]),
            wetness: slot(outputs[2]),
            rock: rock.map_or(u32::MAX, slot),
            left_out,
        })
    }
}

/// A slot for each step's row, used again once every step reading it has
/// run, `kept` rows held to the end; and how many slots that takes.
fn slots(steps: &[(GpuStep, Vec<u32>)], kept: &[u32]) -> (Vec<u32>, u32) {
    let mut last = vec![0usize; steps.len()];
    for (at, (_, inputs)) in steps.iter().enumerate() {
        last[at] = at;
        for &input in inputs {
            last[input as usize] = at;
        }
    }
    for &row in kept {
        last[row as usize] = usize::MAX;
    }
    let mut slot_of = vec![0u32; steps.len()];
    let mut free: Vec<u32> = Vec::new();
    let mut count = 0u32;
    let mut ending: Vec<Vec<usize>> = vec![Vec::new(); steps.len()];
    for (at, &until) in last.iter().enumerate() {
        if until != usize::MAX && until < steps.len() {
            ending[until].push(at);
        }
    }
    for at in 0..steps.len() {
        // The rows read for the last time here are let go after this step is given its own slot, so it never
        // writes over what it reads.
        slot_of[at] = free.pop().unwrap_or_else(|| {
            count += 1;
            count - 1
        });
        for &done in &ending[at] {
            if done != at {
                free.push(slot_of[done]);
            }
        }
        if last[at] == at {
            free.push(slot_of[at]);
        }
    }
    (slot_of, count)
}

/// A kernel's name, for saying which cannot run on the GPU.
fn name_of(kernel: &dyn Kernel) -> String {
    let text = format!("{kernel:?}");
    text.split([' ', '(', '{']).next().unwrap_or(&text).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::starters;

    #[test]
    fn the_starters_of_hills_and_plains_run_on_the_gpu_and_the_rest_say_what_does_not() {
        let flat = ["Battlefield Plains", "Rolling Hills", "Alpine", "Archipelago", "Canyons", "Caves", "Cave Level"];
        for starter in starters() {
            let world = starter.world(1, 1.0);
            match world.gpu_program() {
                Ok(program) => {
                    eprintln!(
                        "{}: {} steps, {} slots, leaving out {:?}",
                        starter.name, program.steps, program.slots, program.left_out
                    );
                    assert!(program.slots as usize <= GPU_SLOTS && program.steps > 0, "{}", starter.name);
                    let caved = program.left_out.contains(&"overhangs and caves");
                    assert_eq!(caved, world.volume().is_some(), "{}", starter.name);
                }
                Err(names) => {
                    eprintln!("{}: not on the GPU for {names:?}", starter.name);
                    assert!(!flat.contains(&starter.name) && !names.is_empty(), "{}: {names:?}", starter.name);
                }
            }
        }
        let hills = starters().into_iter().find(|starter| starter.name == "Rolling Hills").unwrap();
        let program = hills.world(1, 1.0).gpu_program().expect("rolling hills run on the GPU");
        assert!(program.left_out.contains(&"rivers"));
        assert!(program.slots < 24, "{} slots for {} steps", program.slots, program.steps);
    }
}
