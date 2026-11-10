//! Compiling a graph into programs: for each thing asked of a generator
//! (the ground's height, its climate), the steps that make it, in order.
//!
//! Compiling starts at the Output node and asks each node it reaches for
//! the steps behind one of its outputs at some places; the node asks in
//! turn for what its inputs read. Nothing the Output does not reach is
//! compiled. A step is a kernel and the rows it reads, and two steps with
//! the same kernel reading the same rows are one, so a node wired to two
//! others runs once. Places are rows as well: a node that reads its input
//! somewhere else (bent, or a little way off to find a slope) asks for it
//! at rows it has made, and the nodes behind it compile again for those.
//!
//! The order of the steps comes from the wires alone, never from the order
//! nodes lie in a file or in memory, so the same graph is the same program
//! everywhere.

use std::{collections::HashMap, fmt, sync::Arc};

use super::{
    eval::{Given, MOST_INPUTS, Program, Step},
    graph::{Graph, NodeId},
    nodes::Output,
};

/// A row of numbers, by the place among a program's steps of the step that
/// fills it.
pub(crate) type V = u32;

/// The deepest one node may read through others before the graph is taken
/// to run round in a loop.
const DEEPEST: usize = 256;

/// Counted into every program's hash: raised whenever a kernel's numbers
/// change, so what was made by the old ones is made again.
const VERSION: u32 = 1;

/// Where a node is asked for its output: on flat ground at rows of `x` and
/// `z`, in metres in the generator's world, or round a planet at rows of
/// `x`, `y` and `z`, in metres from its middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Places {
    Flat { x: V, z: V },
    Round { x: V, y: V, z: V },
}

/// What a step does: fills `out` from `inputs`, a number for each place
/// from the numbers the inputs have for that place alone.
pub(crate) trait Kernel: fmt::Debug + Send + Sync {
    /// Everything that tells it from every other kernel: what it is and
    /// each of its values, to the bit.
    fn key(&self, key: &mut Key);

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]);
}

/// What tells a kernel from every other, as bytes.
#[derive(Default)]
pub(crate) struct Key(Vec<u8>);

impl Key {
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.0.extend(name.as_bytes());
        self.0.push(0);
        self
    }

    pub fn number(&mut self, number: f32) -> &mut Self {
        self.0.extend(number.to_bits().to_le_bytes());
        self
    }

    pub fn whole(&mut self, whole: u32) -> &mut Self {
        self.0.extend(whole.to_le_bytes());
        self
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        self.whole(text.len() as u32);
        self.0.extend(text.as_bytes());
        self
    }
}

/// What is wrong with a graph. It still makes ground: what cannot be read
/// is taken as nothing, or as the sea level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// It has no Output node.
    NoOutput,
    /// A wire leads back round to this node.
    Loop(NodeId),
    /// A wire comes from this node, which is missing.
    Lost(NodeId),
    /// This Climate node shapes the ground with no height wired into it to
    /// cool by.
    NoGround(NodeId),
}

/// A generator's own values, which every node reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct World {
    pub seed: u32,
    pub scale: f32,
    pub sea_level: f32,
}

/// One program being compiled.
pub(crate) struct Compiler<'a> {
    graph: &'a Graph,
    pub world: World,
    /// Whether it is given the height of the ground asked about.
    grounded: bool,
    steps: Vec<Step>,
    /// Each step's key, in order: what its hash is made from.
    keys: Vec<Vec<u8>>,
    shared: HashMap<Vec<u8>, V>,
    made: HashMap<(NodeId, usize, Places), V>,
    making: Vec<(NodeId, usize, Places)>,
    faults: Vec<Fault>,
}

impl<'a> Compiler<'a> {
    fn new(graph: &'a Graph, world: World, grounded: bool) -> Self {
        Self {
            graph,
            world,
            grounded,
            steps: Vec::new(),
            keys: Vec::new(),
            shared: HashMap::new(),
            made: HashMap::new(),
            making: Vec::new(),
            faults: Vec::new(),
        }
    }

    fn step(&mut self, key: Vec<u8>, step: Step) -> V {
        if let Some(&row) = self.shared.get(&key) {
            return row;
        }
        let row = self.steps.len() as V;
        self.steps.push(step);
        self.keys.push(key.clone());
        self.shared.insert(key, row);
        row
    }

    /// The row of one of what the program is given.
    pub fn given(&mut self, given: Given) -> V {
        self.step(vec![0xFE, given as u8], Step::Given(given))
    }

    /// The row `kernel` fills from `inputs`: one made before, where the
    /// same kernel already reads the same rows.
    pub fn run(&mut self, kernel: impl Kernel + 'static, inputs: &[V]) -> V {
        debug_assert!(inputs.len() <= MOST_INPUTS, "{kernel:?} reads {} rows", inputs.len());
        let mut key = Key::default();
        kernel.key(&mut key);
        key.0.push(0xFF);
        for input in inputs {
            key.0.extend(input.to_le_bytes());
        }
        self.step(key.0, Step::Run { kernel: Arc::new(kernel), inputs: inputs.to_vec() })
    }

    /// A row of `value` at every place.
    pub fn constant(&mut self, value: f32) -> V {
        self.run(Constant(value), &[])
    }

    /// What is wired into input `input` of `node`, at `at`: none with no
    /// wire.
    pub fn input(&mut self, node: NodeId, input: usize, at: Places) -> Option<V> {
        let link = *self.graph.link_into(node, input)?;
        Some(self.output(link.from, link.output, at))
    }

    /// The height of the ground asked about, for `node` to read: the sea
    /// level, and a fault, in a program that is given none.
    pub fn ground(&mut self, node: NodeId) -> V {
        if self.grounded {
            return self.given(Given::Ground);
        }
        self.fault(Fault::NoGround(node));
        self.constant(self.world.sea_level)
    }

    pub fn fault(&mut self, fault: Fault) {
        if !self.faults.contains(&fault) {
            self.faults.push(fault);
        }
    }

    /// Output `port` of `node` at `at`.
    fn output(&mut self, node: NodeId, port: usize, at: Places) -> V {
        let asked = (node, port, at);
        if let Some(&row) = self.made.get(&asked) {
            return row;
        }
        if self.making.contains(&asked) || self.making.len() >= DEEPEST {
            self.fault(Fault::Loop(node));
            return self.constant(0.0);
        }
        let graph = self.graph;
        let Some(found) = graph.node(node) else {
            self.fault(Fault::Lost(node));
            return self.constant(0.0);
        };
        self.making.push(asked);
        let row = found.kind.compile(self, node, port, at);
        self.making.pop();
        self.made.insert(asked, row);
        row
    }

    /// The places the program is given, flat or round.
    fn places(&mut self, round: bool) -> Places {
        match round {
            false => Places::Flat { x: self.given(Given::X), z: self.given(Given::Z) },
            true => Places::Round { x: self.given(Given::X), y: self.given(Given::Y), z: self.given(Given::Z) },
        }
    }

    /// The program making `outputs`, and what was found wrong on the way.
    fn finish(self, outputs: &[V]) -> (Program, Vec<Fault>) {
        let mut bytes: Vec<u8> = VERSION.to_le_bytes().to_vec();
        for key in &self.keys {
            bytes.extend((key.len() as u32).to_le_bytes());
            bytes.extend(key);
        }
        for output in outputs {
            bytes.extend(output.to_le_bytes());
        }
        let hash = crate::layers::stable_hash(&bytes);
        (Program { steps: self.steps, outputs: outputs.to_vec(), hash }, self.faults)
    }
}

/// A graph's programs for flat ground or round a planet: its ground's
/// height at each place, and its climate there (how warm, then how wet) on
/// ground of a height it is given.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub height: Program,
    pub climate: Program,
}

/// Compiles `graph` in `world`, for flat ground or `round` a planet, adding
/// what is wrong with it to `faults`.
pub(crate) fn compile(graph: &Graph, world: World, round: bool, faults: &mut Vec<Fault>) -> Compiled {
    let output = graph.output().map(|node| node.id);
    let mut cx = Compiler::new(graph, world, false);
    if output.is_none() {
        cx.fault(Fault::NoOutput);
    }
    let at = cx.places(round);
    let height = output.and_then(|output| cx.input(output, Output::HEIGHT, at));
    let height = height.unwrap_or_else(|| cx.constant(world.sea_level));
    let (height, found) = cx.finish(&[height]);
    faults.extend(found);

    let mut cx = Compiler::new(graph, world, true);
    let at = cx.places(round);
    let warmth = output.and_then(|output| cx.input(output, Output::TEMPERATURE, at));
    let warmth = warmth.unwrap_or_else(|| {
        let ground = cx.given(Given::Ground);
        cx.run(Mild, &[ground])
    });
    let wetness = output.and_then(|output| cx.input(output, Output::HUMIDITY, at));
    let wetness = wetness.unwrap_or_else(|| cx.constant(0.5));
    let (climate, found) = cx.finish(&[warmth, wetness]);
    for fault in found {
        if !faults.contains(&fault) {
            faults.push(fault);
        }
    }
    Compiled { height, climate }
}

/// A program making every output of every node of `graph` at flat places,
/// given the ground's height for what reads it, and which node and output
/// each row it makes is: what the graph editor draws each node's preview
/// from.
pub(crate) fn compile_all(graph: &Graph, world: World) -> (Program, Vec<(NodeId, usize)>) {
    let mut cx = Compiler::new(graph, world, true);
    let at = cx.places(false);
    let mut order: Vec<&super::graph::Node> = graph.nodes.iter().collect();
    order.sort_by_key(|node| node.id);
    let (mut rows, mut which) = (Vec::new(), Vec::new());
    for node in order {
        for port in 0..node.kind.outputs().len() {
            rows.push(cx.output(node.id, port, at));
            which.push((node.id, port));
        }
    }
    (cx.finish(&rows).0, which)
}

/// The same number at every place.
#[derive(Debug)]
struct Constant(f32);

impl Kernel for Constant {
    fn key(&self, key: &mut Key) {
        key.name("constant").number(self.0);
    }

    fn run(&self, _: &[&[f32]], out: &mut [f32]) {
        out.fill(self.0);
    }
}

/// How warm it is with no climate wired in: 15 °C at nothing high, cooling
/// 6.5 °C for every 1000 m up.
#[derive(Debug)]
struct Mild;

impl Kernel for Mild {
    fn key(&self, key: &mut Key) {
        key.name("mild");
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (out, height) in out.iter_mut().zip(inputs[0]) {
            *out = 15.0 - 6.5 * height / 1000.0;
        }
    }
}
