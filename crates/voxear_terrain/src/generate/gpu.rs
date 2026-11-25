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
    /// What a cell is given: how high its middle is, how large it is, and
    /// the number its column carries up at the place its one number names.
    Y = 20,
    Size = 21,
    Carried = 22,
    Under = 23,
    Join = 24,
    Noise3d = 25,
    Steep = 26,
    Push = 27,
    Tunnels = 28,
    Chambers = 29,
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
    /// Its Volume, where it has one that is more than its height made solid
    /// and that runs on the GPU.
    pub volume: Option<GpuVolume>,
    /// What it leaves out of the ground the CPU makes, by name: what its
    /// kernels leave out, and its Volume where that cannot run on the GPU,
    /// with the kernels that cannot.
    pub left_out: Vec<String>,
}

/// A generator's Volume as the GPU runs it. Its columns' program runs with
/// the height's, a voxel column at a time, and what it carries up each
/// column is kept in [`GpuVolume::carried`]'s slots there; its cells'
/// program runs a voxel at a time, given the voxel's middle, its size and
/// those numbers, and leaves how far into the ground the voxel is, in
/// metres (out of it where negative), in [`GpuVolume::density`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuVolume {
    pub words: Vec<u32>,
    pub steps: u32,
    pub slots: u32,
    pub density: u32,
    pub carried: Vec<u32>,
    /// How far over a column's ground it may be solid, and how far under it
    /// anything but solid, in metres: without end where nothing says.
    pub reach: f32,
    pub depth: f32,
    /// The carried numbers that say between which heights ground stands or
    /// is hollowed apart from a column's own: a foot and a top, by their
    /// place among the carried numbers.
    pub spans: Vec<[u32; 2]>,
}

/// The steps of `program` for the GPU, its rows counted from `start`, each
/// thing it is given made as `given` says: none where it cannot be, which
/// is noted in `unsupported` as is each kernel that cannot run there.
fn lower(
    program: &Program,
    start: u32,
    given: impl Fn(Given) -> Option<(GpuStep, Vec<u32>)>,
    unsupported: &mut Vec<String>,
) -> Vec<(GpuStep, Vec<u32>)> {
    let mut steps = Vec::with_capacity(program.steps.len());
    for step in &program.steps {
        steps.push(match step {
            Step::Given(which) => given(*which).unwrap_or_else(|| {
                unsupported.push(format!("{which:?}"));
                (GpuStep::new(GpuOp::Constant).number(0.0), Vec::new())
            }),
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
        });
    }
    steps
}

/// Steps as the words the shader reads, `kept` rows held to the end: the
/// words, each row's slot, how many slots, and what the steps leave out.
fn encode(steps: &[(GpuStep, Vec<u32>)], kept: &[u32]) -> Result<(Vec<u32>, Vec<u32>, u32, Vec<String>), String> {
    let (slot_of, slots) = slots(steps, kept);
    if slots > GPU_SLOTS as u32 {
        return Err(format!("{slots} rows at once, of {GPU_SLOTS}"));
    }
    let mut words = Vec::new();
    let mut left_out = Vec::new();
    for (at, (step, inputs)) in steps.iter().enumerate() {
        words.push(step.op as u32);
        words.push(inputs.len() as u32);
        words.extend(inputs.iter().map(|&input| slot_of[input as usize]));
        words.push(slot_of[at]);
        words.push(step.words.len() as u32);
        words.extend(&step.words);
        if let Some(what) = step.left_out
            && !left_out.iter().any(|known| known == what)
        {
            left_out.push(what.to_owned());
        }
    }
    Ok((words, slot_of, slots, left_out))
}

impl WorldGen {
    /// Its height, climate and rock as one program the GPU runs a column at
    /// a time, with its Volume where it can run there too; or the names of
    /// the kernels its height, climate or rock could not run there.
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
            let given = |which: Given| match which {
                Given::X => Some((GpuStep::new(GpuOp::X), Vec::new())),
                Given::Z => Some((GpuStep::new(GpuOp::Z), Vec::new())),
                Given::Ground if part > 0 => Some((GpuStep::new(GpuOp::Copy).whole(0), vec![height])),
                _ => None,
            };
            steps.extend(lower(program, start, given, &mut unsupported));
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
        let rock = compiled.rock.as_ref().map(|_| outputs[3]);
        let mut kept: Vec<u32> =
            [Some(outputs[0]), Some(outputs[1]), Some(outputs[2]), rock].into_iter().flatten().collect();
        let mut left_out = Vec::new();
        // Its Volume, unless it is the height made solid with nothing taken out: its columns' steps after the rest,
        // given the height made first, and its cells' in a program of their own.
        let mut volume = None;
        if let Some(voxels) = compiled.volume.as_ref().filter(|voxels| !voxels.plain || voxels.caved) {
            let mut missing = Vec::new();
            let start = steps.len() as u32;
            let given = |which: Given| match which {
                Given::X => Some((GpuStep::new(GpuOp::X), Vec::new())),
                Given::Z => Some((GpuStep::new(GpuOp::Z), Vec::new())),
                Given::Ground => Some((GpuStep::new(GpuOp::Copy).whole(0), vec![height])),
                _ => None,
            };
            let columns = lower(&voxels.columns, start, given, &mut missing);
            let given = |which: Given| match which {
                Given::X => Some((GpuStep::new(GpuOp::X), Vec::new())),
                Given::Y => Some((GpuStep::new(GpuOp::Y), Vec::new())),
                Given::Z => Some((GpuStep::new(GpuOp::Z), Vec::new())),
                Given::Size => Some((GpuStep::new(GpuOp::Size), Vec::new())),
                Given::Carried(place) => Some((GpuStep::new(GpuOp::Carried).whole(u32::from(place)), Vec::new())),
                Given::Ground => None,
            };
            let cells = lower(&voxels.cells, 0, given, &mut missing);
            let density = voxels.cells.outputs[0];
            let made = encode(&cells, &[density]);
            if let Err(error) = &made {
                missing.push(error.clone());
            }
            if missing.is_empty()
                && let Ok((words, slot_of, slots, cells_left_out)) = made
            {
                let carried: Vec<u32> = voxels.columns.outputs.iter().map(|&output| output + start).collect();
                kept.extend(&carried);
                steps.extend(columns);
                left_out.extend(cells_left_out);
                let spans = voxels.spans.iter().map(|([foot, top], _)| [*foot as u32, *top as u32]).collect();
                volume = Some((
                    carried,
                    GpuVolume {
                        words,
                        steps: cells.len() as u32,
                        slots,
                        density: slot_of[density as usize],
                        carried: Vec::new(),
                        reach: voxels.reach,
                        depth: voxels.depth,
                        spans,
                    },
                ));
            } else {
                missing.sort();
                missing.dedup();
                left_out.push(format!("overhangs and caves ({})", missing.join(", ")));
            }
        }
        let (words, slot_of, slots, made_left_out) = encode(&steps, &kept).map_err(|error| vec![error])?;
        for what in made_left_out {
            if !left_out.contains(&what) {
                left_out.push(what);
            }
        }
        let slot = |row: u32| slot_of[row as usize];
        let volume =
            volume.map(|(carried, volume)| GpuVolume { carried: carried.into_iter().map(slot).collect(), ..volume });
        Ok(GpuProgram {
            words,
            steps: steps.len() as u32,
            slots,
            height: slot(outputs[0]),
            warmth: slot(outputs[1]),
            wetness: slot(outputs[2]),
            rock: rock.map_or(u32::MAX, slot),
            volume,
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
                    // A Volume runs on the GPU, or is left out saying why.
                    let left = program.left_out.iter().any(|what| what.starts_with("overhangs and caves"));
                    let solid = world.volume().is_some_and(|volume| volume.plain() && !volume.caved());
                    let asked = world.volume().is_some() && !solid;
                    assert_eq!(asked, left || program.volume.is_some(), "{}", starter.name);
                    if let Some(volume) = &program.volume {
                        eprintln!(
                            "  its Volume: {} steps in {} slots, {} carried",
                            volume.steps,
                            volume.slots,
                            volume.carried.len()
                        );
                    }
                }
                Err(names) => {
                    eprintln!("{}: not on the GPU for {names:?}", starter.name);
                    assert!(!flat.contains(&starter.name) && !names.is_empty(), "{}: {names:?}", starter.name);
                }
            }
        }
        let hills = starters().into_iter().find(|starter| starter.name == "Rolling Hills").unwrap();
        let program = hills.world(1, 1.0).gpu_program().expect("rolling hills run on the GPU");
        assert!(program.left_out.iter().any(|what| what == "rivers"));
        assert!(program.slots < 24, "{} slots for {} steps", program.slots, program.steps);
    }
}
