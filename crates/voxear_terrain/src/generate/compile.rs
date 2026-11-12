//! Compiling a graph into programs: for each thing asked of a generator
//! (the ground's height, its climate, how far into the ground each place
//! in space is), the steps that make it, in order.
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
//!
//! A Volume is compiled in two programs ([`Voxels`]). One runs once a cell,
//! given where the cell's middle is. What a Volume node reads of the ground
//! (a height, a mask) is the same for every cell of a column, so it is
//! compiled into the other, run once a column, and carried up to the cells
//! as a row they are given ([`Compiler::column`]). Only where a node has
//! moved the cells it asks about is a Field worked out a cell at a time.
//! The Volume is compiled twice into the cells' program: whole, and as the
//! ground under its caves, with what its Combine nodes take away left in.
//! The ground alone is what far levels of blocks are made from, what tells
//! open air from a cave's, and what a block's depth under the surface is
//! counted in.

use std::{collections::HashMap, fmt, sync::Arc};

use super::{
    caves::Network,
    eval::{Given, MOST_INPUTS, Program, Step},
    graph::{Graph, NodeId},
    nodes::{Output, PortKind},
};

/// A row of numbers, by the place among a program's steps of the step that
/// fills it.
pub(crate) type V = u32;

/// The deepest one node may read through others before the graph is taken
/// to run round in a loop.
const DEEPEST: usize = 256;

/// How far over its ground a Volume is read at most, in metres: where what
/// could be solid at any height (noise through space) is cut off. Ground
/// that stands further over the ground than this says where, a column at a
/// time (see [`Compiler::note_span`]).
pub(crate) const MOST_OVER: f32 = 64.0;

/// Counted into every program's hash: raised whenever a kernel's numbers
/// change, so what was made by the old ones is made again.
const VERSION: u32 = 1;

/// Where a node is asked for its output: at rows of `x` and `z`, in metres
/// in the generator's world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Places {
    pub x: V,
    pub z: V,
}

/// Where a node is asked for a Volume: at cells, rows of where their
/// middles are: `x` and `z` in metres in the generator's world, and `y`
/// metres up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Cells {
    pub x: V,
    pub y: V,
    pub z: V,
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

    /// Bytes as they are: a key made before.
    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend(bytes);
        self
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    /// A hash of it, the same on every machine.
    pub fn hash(&self) -> u64 {
        crate::layers::stable_hash(&self.0)
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
    /// In a program run once a cell: the program run once a column beside
    /// it, and the rows of that carried up to the cells.
    carry: Option<Box<Carry<'a>>>,
    /// The node and output that make the ground this program is given, so
    /// asked for at its own places they are not worked out again.
    ground_from: Option<(NodeId, usize)>,
    /// Whether what a Combine node takes away is taken: not while the
    /// ground under a Volume's caves is compiled.
    carving: bool,
    volumes: HashMap<(NodeId, usize, Cells, bool), V>,
    making_volumes: Vec<(NodeId, usize, Cells)>,
    /// The rows that are the generator's own ground made solid and
    /// nothing more.
    plain: Vec<V>,
    /// How far over its column's ground a row may be solid, in metres, for
    /// those that may at all.
    rises: HashMap<V, f32>,
    /// How far under its column's ground a row may be anything but solid,
    /// in metres, for those known to be solid under that.
    depths: HashMap<V, f32>,
    /// The rows carried up from each column that say between which heights
    /// ground stands apart from the column's own: its foot, then its top.
    spans: Vec<(V, V)>,
    /// Those of the spans where ground stands, which a far view shows.
    stands: Vec<(V, V)>,
    /// The rows that are nothing at all but within the spans noted: what
    /// is hollowed out of the ground there.
    hollows: Vec<V>,
    steps: Vec<Step>,
    /// Each step's key, in order: what its hash is made from.
    keys: Vec<Vec<u8>>,
    shared: HashMap<Vec<u8>, V>,
    made: HashMap<(NodeId, usize, Places), V>,
    making: Vec<(NodeId, usize, Places)>,
    faults: Vec<Fault>,
    /// The generator's own ground as a program of its own, once a node has asked for it.
    own_ground: Option<Arc<Program>>,
    /// The first network of cave rooms compiled into it.
    caves: Option<Arc<Network>>,
}

/// The program run once a column beside one run once a cell, as it is
/// compiled, which of its rows the cells are given, in order, and which of
/// those may be read between columns (see [`Compiler::column_between`]).
struct Carry<'a> {
    columns: Compiler<'a>,
    rows: Vec<V>,
    between: Vec<bool>,
}

impl<'a> Compiler<'a> {
    fn new(graph: &'a Graph, world: World, grounded: bool) -> Self {
        Self {
            graph,
            world,
            grounded,
            carry: None,
            ground_from: None,
            carving: true,
            volumes: HashMap::new(),
            making_volumes: Vec::new(),
            plain: Vec::new(),
            rises: HashMap::new(),
            depths: HashMap::new(),
            spans: Vec::new(),
            stands: Vec::new(),
            hollows: Vec::new(),
            steps: Vec::new(),
            keys: Vec::new(),
            shared: HashMap::new(),
            made: HashMap::new(),
            making: Vec::new(),
            faults: Vec::new(),
            own_ground: None,
            caves: None,
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
        let mut key = vec![0xFE];
        key.extend(given.code());
        self.step(key, Step::Given(given))
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

    /// Whether what a node takes away from a Volume is taken: not while the
    /// ground under its caves is compiled, when the node passes on what it
    /// would take from.
    pub fn carving(&self) -> bool {
        self.carving
    }

    /// The generator's own ground at `at`: what the program is given, at
    /// its own places in one given it; else what is wired into the Output's
    /// height there, or the sea level.
    pub fn ground_at(&mut self, at: Places) -> V {
        if self.grounded && at == self.places() {
            return self.given(Given::Ground);
        }
        let output = self.graph.output().map(|node| node.id);
        let height = output.and_then(|output| self.input(output, Output::HEIGHT, at));
        height.unwrap_or_else(|| self.constant(self.world.sea_level))
    }

    /// The generator's own ground as a program of its own: for a kernel that asks the ground's height somewhere
    /// other than at the places it is run over.
    pub fn own_ground(&mut self) -> Arc<Program> {
        if let Some(program) = &self.own_ground {
            return program.clone();
        }
        let mut cx = Compiler::new(self.graph, self.world, false);
        let at = cx.places();
        let output = self.graph.output().map(|node| node.id);
        let height = output.and_then(|output| cx.input(output, Output::HEIGHT, at));
        let height = height.unwrap_or_else(|| cx.constant(self.world.sea_level));
        let program = Arc::new(cx.finish(&[height]).0);
        self.own_ground = Some(program.clone());
        program
    }

    /// Says `caves` is a network of cave rooms this program hollows: the first said is the generator's.
    pub fn note_caves(&mut self, caves: Arc<Network>) {
        self.caves.get_or_insert(caves);
    }

    /// The cells the program is given.
    pub fn cells(&mut self) -> Cells {
        Cells { x: self.given(Given::X), y: self.given(Given::Y), z: self.given(Given::Z) }
    }

    /// How large the cells asked about are, in metres.
    pub fn size(&mut self) -> V {
        self.given(Given::Size)
    }

    /// A row made once for each column the cells `at` are in, by `make`
    /// from the columns' places. For the program's own cells it is made in
    /// the program run once a column, and carried up; for cells a node has
    /// moved it is made here, where each cell's own column is.
    pub fn column(&mut self, at: Cells, make: impl FnOnce(&mut Compiler<'a>, Places) -> V) -> V {
        self.carried(at, false, make)
    }

    /// A row made once a column as [`Compiler::column`] makes it, that
    /// changes slowly from one column to the next and need not be exact: a
    /// mask. Where a whole grid of columns is asked about at once, it is
    /// made at every few columns of the grid and read between them (see
    /// `Volume::carried_grid`), which what is slow to make is worth.
    pub fn column_between(&mut self, at: Cells, make: impl FnOnce(&mut Compiler<'a>, Places) -> V) -> V {
        self.carried(at, true, make)
    }

    fn carried(&mut self, at: Cells, between: bool, make: impl FnOnce(&mut Compiler<'a>, Places) -> V) -> V {
        if at == self.cells()
            && let Some(carry) = self.carry.as_mut()
        {
            let places = carry.columns.places();
            let row = make(&mut carry.columns, places);
            let found = carry.rows.iter().position(|&carried| carried == row);
            let place = found.unwrap_or_else(|| {
                carry.rows.push(row);
                carry.between.push(between);
                carry.rows.len() - 1
            });
            // A row anything wants exact is made at every column.
            carry.between[place] &= between;
            debug_assert!(place < 256, "{place} rows carried up");
            return self.given(Given::Carried(place.min(255) as u8));
        }
        make(self, Places { x: at.x, z: at.z })
    }

    /// What is wired into Volume input `input` of `node`, at cells `at`:
    /// none with no wire.
    pub fn volume(&mut self, node: NodeId, input: usize, at: Cells) -> Option<V> {
        let link = *self.graph.link_into(node, input)?;
        Some(self.volume_output(link.from, link.output, at))
    }

    /// What is wired into Field input `input` of a Volume node `node`, for
    /// the columns the cells `at` are in: none with no wire.
    pub fn field(&mut self, node: NodeId, input: usize, at: Cells) -> Option<V> {
        let link = *self.graph.link_into(node, input)?;
        Some(self.column(at, |cx, places| cx.output(link.from, link.output, places)))
    }

    /// Output `port` of `node` at cells `at`: a Volume's own steps, or a
    /// Field's row for each cell's column, the same all the way up it.
    fn volume_output(&mut self, node: NodeId, port: usize, at: Cells) -> V {
        let graph = self.graph;
        let Some(found) = graph.node(node) else {
            self.fault(Fault::Lost(node));
            return self.constant(0.0);
        };
        if found.kind.outputs().get(port).is_none_or(|port| port.kind != PortKind::Volume) {
            return self.column(at, |cx, places| cx.output(node, port, places));
        }
        let asked = (node, port, at, self.carving);
        if let Some(&row) = self.volumes.get(&asked) {
            return row;
        }
        let making = (node, port, at);
        if self.making_volumes.contains(&making) || self.making.len() + self.making_volumes.len() >= DEEPEST {
            self.fault(Fault::Loop(node));
            return self.constant(0.0);
        }
        self.making_volumes.push(making);
        let row = found.kind.compile_volume(self, node, port, at);
        self.making_volumes.pop();
        self.volumes.insert(asked, row);
        row
    }

    /// Says `row` is the generator's own ground made solid and nothing
    /// more: its number at a cell is how far under its column's ground the
    /// cell is.
    pub fn set_plain(&mut self, row: V) {
        if !self.plain.contains(&row) {
            self.plain.push(row);
        }
    }

    /// How far over its column's ground `row` may be solid, in metres.
    pub fn rise(&self, row: V) -> f32 {
        self.rises.get(&row).copied().unwrap_or(0.0)
    }

    /// Says `row` may be solid as far as `rise` metres over its column's
    /// ground.
    pub fn set_rise(&mut self, row: V, rise: f32) {
        if rise > 0.0 {
            self.rises.insert(row, rise);
        }
    }

    /// How far under its column's ground `row` may be anything but
    /// solid, in metres: without end, unless a node has said.
    pub fn depth(&self, row: V) -> f32 {
        self.depths.get(&row).copied().unwrap_or(f32::INFINITY)
    }

    /// Says `row` is solid from `depth` metres under its column's ground
    /// down, but for the spans noted (see [`Compiler::note_span`]).
    pub fn set_depth(&mut self, row: V, depth: f32) {
        if depth.is_finite() {
            self.depths.insert(row, depth.max(0.0));
        }
    }

    /// Whether rows made once a column for the cells `at` are carried up
    /// to them: so for a Volume's own cells, and not for cells a node has
    /// moved or a program that makes every node's preview, where such a
    /// row would be made again for every cell.
    pub fn carries(&mut self, at: Cells) -> bool {
        at == self.cells() && self.carry.is_some()
    }

    /// Whether `row` is a hollow that is nothing but within the spans
    /// noted, so that taking it out of ground leaves the ground solid as
    /// deep as it was everywhere else.
    pub fn hollow(&self, row: V) -> bool {
        self.hollows.contains(&row)
    }

    /// Says `row` is such a hollow.
    pub fn set_hollow(&mut self, row: V) {
        if !self.hollows.contains(&row) {
            self.hollows.push(row);
        }
    }

    /// Says ground stands, or is hollowed, apart from a column's own
    /// ground between the heights two rows give, in metres up: `foot` and
    /// `top`, each made once a column by [`Compiler::column`] for the cells
    /// `at`. A column with nothing there gives a top under its foot. What
    /// makes a world of blocks then asks about the cells between, however
    /// far from the ground they are, and about none of the air or rock
    /// between those and the ground.
    ///
    /// `stands` says ground stands there, as wide as the column and whole
    /// from foot to top, so that a far view may show it from these two
    /// numbers alone; a hollow, or ground too thin to say so of, is not.
    pub fn note_span(&mut self, at: Cells, foot: V, top: V, stands: bool) {
        if stands && !self.stands.contains(&(foot, top)) {
            self.stands.push((foot, top));
        }
        let carried = |cx: &Self, row: V| matches!(cx.steps.get(row as usize), Some(Step::Given(Given::Carried(_))));
        if at == self.cells() && carried(self, foot) && carried(self, top) && !self.spans.contains(&(foot, top)) {
            self.spans.push((foot, top));
        }
    }

    /// Output `port` of `node` at `at`.
    fn output(&mut self, node: NodeId, port: usize, at: Places) -> V {
        let asked = (node, port, at);
        if let Some(&row) = self.made.get(&asked) {
            return row;
        }
        if self.ground_from == Some((node, port)) && self.grounded && at == self.places() {
            return self.given(Given::Ground);
        }
        if self.making.contains(&asked) || self.making.len() + self.making_volumes.len() >= DEEPEST {
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

    /// The places the program is given.
    fn places(&mut self) -> Places {
        Places { x: self.given(Given::X), z: self.given(Given::Z) }
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

/// A graph's programs: its ground's height at each place, its climate
/// there (how warm, then how wet) on ground of a height it is given, and
/// its Volume, where its Output has one wired in.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub height: Program,
    pub climate: Program,
    pub volume: Option<Voxels>,
    /// A mask of where the ground is bare rock, where one is wired in.
    pub rock: Option<Program>,
    /// How far under the land round about the ground lies, where it is wired in.
    pub sunk: Option<Program>,
}

/// A graph's Volume, compiled.
#[derive(Debug)]
pub(crate) struct Voxels {
    /// Run once a column, given its place and its ground's height: makes
    /// the rows carried up to its cells.
    pub columns: Program,
    /// Which of those rows may be read between the columns of a grid; the
    /// columns' program for the rows that may not, in their order, and for
    /// those that may.
    pub between: Vec<bool>,
    pub exact: Program,
    pub spread: Program,
    /// Run once a cell, given its middle, its size and its column's
    /// carried rows: makes how far into the ground it is, in metres, then
    /// how far into the ground under the caves.
    pub cells: Program,
    /// The cells' program for the ground under the caves alone.
    pub coarse: Program,
    /// Whether the ground under the caves is the generator's own ground
    /// made solid and nothing more, so a cell's number there is its depth
    /// under its column's surface.
    pub plain: bool,
    /// Whether anything is taken out of that ground: caves.
    pub caved: bool,
    /// How far over a column's ground its cells may be solid, in metres.
    pub reach: f32,
    /// How far under a column's ground its cells may be anything but
    /// solid, in metres: without end where nothing says.
    pub depth: f32,
    /// The carried rows that say between which heights ground stands, or
    /// is hollowed, apart from a column's own: each a foot and a top, by
    /// their place among the carried rows. The second of each says whether
    /// it is ground that stands there, and not a hollow.
    pub spans: Vec<([usize; 2], bool)>,
    /// The columns' program for those rows alone, a foot then a top for
    /// each.
    pub spanned: Program,
    pub hash: u64,
    /// The network of cave rooms it hollows, where it has one.
    pub caves: Option<Arc<Network>>,
}

/// The Volume wired into `graph`'s Output, compiled: none with no wire.
fn voxels(graph: &Graph, world: World, faults: &mut Vec<Fault>) -> Option<Voxels> {
    let output = graph.output()?.id;
    graph.link_into(output, Output::VOLUME)?;
    let mut columns = Compiler::new(graph, world, true);
    columns.ground_from = graph.link_into(output, Output::HEIGHT).map(|link| (link.from, link.output));
    let mut cx = Compiler::new(graph, world, false);
    cx.carry = Some(Box::new(Carry { columns, rows: Vec::new(), between: Vec::new() }));
    let at = cx.cells();
    let whole = cx.volume(output, Output::VOLUME, at)?;
    cx.carving = false;
    let ground = cx.volume(output, Output::VOLUME, at)?;
    let (plain, reach) = (cx.plain.contains(&ground), cx.rise(ground).min(MOST_OVER));
    let depth = if cx.depth(whole) > cx.depth(ground) { cx.depth(whole) } else { cx.depth(ground) };
    let place = |cx: &Compiler, row: V| match cx.steps.get(row as usize) {
        Some(Step::Given(Given::Carried(place))) => usize::from(*place),
        _ => 0,
    };
    let stands = cx.stands.clone();
    let spans: Vec<([usize; 2], bool)> = (cx.spans.iter())
        .map(|&(foot, top)| ([place(&cx, foot), place(&cx, top)], stands.contains(&(foot, top))))
        .collect();
    let carry = cx.carry.take()?;
    let caves = cx.caves.take();
    let (cells, found) = cx.finish(&[whole, ground]);
    let Carry { columns, rows, between } = *carry;
    let (columns, more) = columns.finish(&rows);
    for fault in found.into_iter().chain(more) {
        if !faults.contains(&fault) {
            faults.push(fault);
        }
    }
    let coarse = cells.only(&[ground]);
    let pick = |spread: bool| -> Vec<V> {
        rows.iter().zip(&between).filter(|(_, between)| **between == spread).map(|(row, _)| *row).collect()
    };
    let (exact, spread) = (columns.only(&pick(false)), columns.only(&pick(true)));
    let spanned: Vec<V> = spans.iter().flat_map(|(span, _)| span.map(|place| rows[place])).collect();
    let spanned = columns.only(&spanned);
    let mut bytes = columns.hash.to_le_bytes().to_vec();
    bytes.extend(cells.hash.to_le_bytes());
    bytes.extend(between.iter().map(|between| u8::from(*between)));
    let hash = crate::layers::stable_hash(&bytes);
    let caved = whole != ground;
    Some(Voxels {
        columns,
        between,
        exact,
        spread,
        cells,
        coarse,
        plain,
        caved,
        reach,
        depth,
        spans,
        spanned,
        hash,
        caves,
    })
}

/// Compiles `graph` in `world`, adding what is wrong with it to `faults`.
pub(crate) fn compile(graph: &Graph, world: World, faults: &mut Vec<Fault>) -> Compiled {
    let output = graph.output().map(|node| node.id);
    let mut cx = Compiler::new(graph, world, false);
    if output.is_none() {
        cx.fault(Fault::NoOutput);
    }
    let at = cx.places();
    let height = output.and_then(|output| cx.input(output, Output::HEIGHT, at));
    let height = height.unwrap_or_else(|| cx.constant(world.sea_level));
    let (height, found) = cx.finish(&[height]);
    faults.extend(found);

    let mut cx = Compiler::new(graph, world, true);
    let at = cx.places();
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
    let volume = voxels(graph, world, faults);
    // Where the ground is bare rock, and how far it is sunk, on ground it is given: for a graph with them wired in.
    let mut wired = |input: usize| {
        let output = output.filter(|output| graph.link_into(*output, input).is_some())?;
        let mut cx = Compiler::new(graph, world, true);
        cx.ground_from = graph.link_into(output, Output::HEIGHT).map(|link| (link.from, link.output));
        let at = cx.places();
        let field = cx.input(output, input, at)?;
        let (program, found) = cx.finish(&[field]);
        for fault in found {
            if !faults.contains(&fault) {
                faults.push(fault);
            }
        }
        Some(program)
    };
    let rock = wired(Output::ROCK);
    let sunk = wired(Output::SUNK);
    Compiled { height, climate, volume, rock, sunk }
}

/// A program making every output of every node of `graph` at its places,
/// given the ground's height for what reads it, and which node and output
/// each row it makes is: what the graph editor draws each node's preview
/// from. A Volume is made at cells as high as the program is given, of the
/// size it is given.
pub(crate) fn compile_all(graph: &Graph, world: World) -> (Program, Vec<(NodeId, usize)>) {
    let mut cx = Compiler::new(graph, world, true);
    let (at, cells) = (cx.places(), cx.cells());
    let mut order: Vec<&super::graph::Node> = graph.nodes.iter().collect();
    order.sort_by_key(|node| node.id);
    let (mut rows, mut which) = (Vec::new(), Vec::new());
    for node in order {
        for (port, kind) in node.kind.outputs().iter().enumerate() {
            rows.push(match kind.kind {
                PortKind::Volume => cx.volume_output(node.id, port, cells),
                _ => cx.output(node.id, port, at),
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        eval::Givens,
        nodes::{NodeKind, Noise, Solid},
    };

    /// A row moved by so much: how a node bends where it reads.
    #[derive(Debug)]
    struct Shift(f32);

    impl Kernel for Shift {
        fn key(&self, key: &mut Key) {
            key.name("shift").number(self.0);
        }

        fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
            for (out, at) in out.iter_mut().zip(inputs[0]) {
                *out = at + self.0;
            }
        }
    }

    #[test]
    fn a_field_is_carried_up_its_column_unless_a_node_bent_where_it_is_read() {
        let mut graph = Graph::default();
        let hills = graph.add(NodeKind::Noise(Noise { size: 80.0, high: 30.0, ..Noise::default() }), [0.0, 0.0]);
        let solid = graph.add(NodeKind::Solid(Solid::default()), [200.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [400.0, 0.0]);
        assert!(graph.connect(hills, 0, solid, Solid::HEIGHT) && graph.connect(hills, 0, output, Output::HEIGHT));
        let world = World { seed: 3, scale: 1.0, sea_level: 0.0 };
        let mut columns = Compiler::new(&graph, world, true);
        columns.ground_from = Some((hills, 0));
        let mut cx = Compiler::new(&graph, world, false);
        cx.carry = Some(Box::new(Carry { columns, rows: Vec::new(), between: Vec::new() }));
        // The hills made solid at the cells asked about, and at cells 25 m along x from them.
        let own = cx.cells();
        let moved = Cells { x: cx.run(Shift(25.0), &[own.x]), ..own };
        let (straight, bent) = (cx.volume_output(solid, 0, own), cx.volume_output(solid, 0, moved));
        assert!(cx.plain.contains(&straight) && !cx.plain.contains(&bent));
        let carry = cx.carry.take().unwrap();
        assert_eq!(carry.rows.len(), 1, "the hills' height at each column, for the cells left where they were");
        let (program, faults) = cx.finish(&[straight, bent]);
        assert!(faults.is_empty(), "{faults:?}");
        // Run over cells up one column: the carried height serves the first, and the second works the hills
        // out again 25 m away, at every cell.
        let height = compile(&graph, world, &mut Vec::new()).height;
        let at = |x: f32, z: f32| {
            let mut found = [0.0];
            height.run(1, Givens { x: &[x], z: &[z], ..Givens::default() }, &mut [&mut found]);
            found[0]
        };
        let (x, z, ys) = (40.0, -15.0, [0.0, 7.5, 31.0]);
        let (here, there) = (at(x, z), at(x + 25.0, z));
        assert!((here - there).abs() > 0.1);
        let (mut under, mut under_moved) = ([0.0; 3], [0.0; 3]);
        let given = Givens { x: &[x], y: &ys, z: &[z], carried: &[&[here]], size: 1.0, ..Givens::default() };
        program.run(3, given, &mut [&mut under, &mut under_moved]);
        for (k, y) in ys.iter().enumerate() {
            assert_eq!(under[k].to_bits(), (here - y).to_bits());
            assert_eq!(under_moved[k].to_bits(), (there - y).to_bits());
        }
    }
}
