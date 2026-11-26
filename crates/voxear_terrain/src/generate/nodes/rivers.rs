//! The Rivers node: rivers running down the ground wired into it, lakes in
//! its hollows, and waterfalls where a river drops over a cliff.
//!
//! Water finds its way over a grid of points Spacing apart, each a little
//! off its place by the seed: each drains to whichever of the eight round
//! it is lowest, the steepest way down, and a point with none lower is a
//! hollow. How much ground drains through a point is counted from the
//! points within a kilometre or so up its way. That is all worked out from
//! the points alone, so any chunk anywhere in an endless world finds the
//! same rivers, and they never stop at a seam.
//!
//! Where as much ground as Source drains into a point, in hectares, a river
//! runs from it down the way the water goes, wider and deeper the more
//! drains into it. High ground counts for more than low, as rain and snow
//! gather sooner off hills and mountains: a point counts as much more as it
//! stands over the ground round it, and flat lowland for a quarter, so
//! rivers start up high and seldom out in a flat field. Every river starts
//! from a spring, a little pool at its head, and one that runs into a
//! hollow too shallow for a lake ends in a pond there. One that reaches the
//! sea widens and shallows into its mouth, its banks giving way to the
//! shore, and leaves the sea's floor as it is. Its size goes by what drains
//! into it: Width and Depth are how wide and deep it is where a
//! square kilometre does. Its water falls along every stretch, so it always
//! runs downhill, and it winds as Meander says. The ground is cut away for
//! its bed, a bowl dropping steeply from its edge to a broad middle, and its
//! banks rise from just over its water up its valley's sides, Valley metres
//! either side: in some stretches at once, where it has cut into them as on
//! the outside of a bend, elsewhere along a low shelving strip, reaching
//! further back where it is cut deep. Its water sits in the ground as it
//! lies: kept a quarter of a block under the ground read a few metres
//! either side of it and round each pool and lake, so a pond fills its
//! hollow and a stream cuts down into a slope. Only where the ground dips
//! between the places read does a low rim hold it in, so its water never
//! stands over the air beside it. Where a stretch drops more
//! than Falls metres, the river runs nearly level to a cliff halfway along
//! it and pours over into a pool at its foot. A hollow that as much as
//! Lakes hectares drains into holds a lake as high as the lowest of the
//! ground round it; none at 0.
//!
//! Beside the ground it gives Water, the water's height where there is a
//! river or a lake (far under everything elsewhere), and Falls, how many
//! metres of a waterfall pour down over its pool: wire both into the
//! Output's, and a world of blocks fills its rivers and lakes with its
//! sea's block, and a waterfall's curtain with falling water. Banks is 1 on
//! a river and its banks, fading out across its valley, for a biome to grow
//! sand or reeds by water.
//!
//! Water already standing on the ground, wired into Standing, is taken as
//! the sea is: rivers run into it and leave the ground under it as it is.
//! So a coarser Rivers node, over points further apart, wired in before,
//! lays great rivers, and this node's rivers run into them. Run On carries
//! every river on down its way that far past where it starts, as wide as
//! there, so a great river crosses flat land, where too little drains in to
//! keep it going, to the sea. Where rivers are carried on, a hollow spills
//! only towards lower ground, its river cutting through the rim, so water
//! never comes round to where it was; and each turn is drawn as a curve,
//! so rivers over points far apart do not run in straight lines.
//!
//! It is sums, products, square roots and `floor`, so every machine makes
//! the same rivers.

use crate::generate::gpu::{GpuOp, GpuStep};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::{
    Kind, Port, Value, ValueSpec,
    noise3d::seed_for,
    plateaus::{mix, share},
};
use crate::{
    generate::{
        compile::{Compiler, Kernel, Key, Places, V},
        eval::{Givens, Program},
        graph::NodeId,
    },
    noise::gradient,
};

/// Rivers where `source` hectares drain into a point, `width` metres wide
/// and `depth` deep where a square kilometre does, over points `spacing`
/// metres apart, each running on at least `run_on` metres down its way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rivers {
    pub spacing: f32,
    pub source: f32,
    pub width: f32,
    pub depth: f32,
    pub valley: f32,
    pub falls: f32,
    pub lakes: f32,
    pub meander: f32,
    pub run_on: f32,
    pub seed: u32,
}

impl Default for Rivers {
    fn default() -> Self {
        Self {
            spacing: 64.0,
            source: 10.0,
            width: 14.0,
            depth: 3.0,
            valley: 40.0,
            falls: 5.0,
            lakes: 12.0,
            meander: 0.6,
            run_on: 0.0,
            seed: 0,
        }
    }
}

impl Rivers {
    /// Its inputs: the ground, and water already standing on it.
    pub const GROUND: usize = 0;
    pub const STANDING: usize = 1;
    /// Its outputs.
    pub const HEIGHT: usize = 0;
    pub const WATER: usize = 1;
    pub const FALLS: usize = 2;
    pub const BANKS: usize = 3;
}

/// The water's height where there is none: under everything.
pub const NO_WATER: f32 = -1.0e6;

const VALUES: &[ValueSpec] = &[
    ValueSpec::number("source", "Source", 0.5, 100000.0, "ha"),
    ValueSpec::number("width", "Width", 1.0, 1000.0, "m"),
    ValueSpec::number("depth", "Depth", 0.2, 100.0, "m"),
    ValueSpec::number("valley", "Valley", 0.0, 2000.0, "m"),
    ValueSpec::number("falls", "Falls", 1.0, 1000.0, "m"),
    ValueSpec::number("lakes", "Lakes", 0.0, 100000.0, "ha"),
    ValueSpec::number("meander", "Meander", 0.0, 1.0, ""),
    ValueSpec::number("run_on", "Run On", 0.0, 50000.0, "m").advanced(),
    ValueSpec::number("spacing", "Spacing", 16.0, 1000.0, "m").advanced(),
    ValueSpec::whole("seed", "Seed", 0, u32::MAX).advanced(),
];

const INPUTS: &[Port] = &[Port::field("ground", "Ground"), Port::field("standing", "Standing")];

const OUTPUTS: &[Port] = &[
    Port::field("height", "Height"),
    Port::field("water", "Water"),
    Port::field("falls", "Falls"),
    Port::field("banks", "Banks"),
];

impl Kind for Rivers {
    const TITLE: &'static str = "Rivers";
    const FOLDER: &'static str = "Shape";

    fn inputs(&self) -> &'static [Port] {
        INPUTS
    }

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Number(self.source),
            1 => Value::Number(self.width),
            2 => Value::Number(self.depth),
            3 => Value::Number(self.valley),
            4 => Value::Number(self.falls),
            5 => Value::Number(self.lakes),
            6 => Value::Number(self.meander),
            7 => Value::Number(self.run_on),
            8 => Value::Number(self.spacing),
            9 => Value::Whole(self.seed),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.source = number,
            (1, Value::Number(number)) => self.width = number,
            (2, Value::Number(number)) => self.depth = number,
            (3, Value::Number(number)) => self.valley = number,
            (4, Value::Number(number)) => self.falls = number,
            (5, Value::Number(number)) => self.lakes = number,
            (6, Value::Number(number)) => self.meander = number,
            (7, Value::Number(number)) => self.run_on = number,
            (8, Value::Number(number)) => self.spacing = number,
            (9, Value::Whole(whole)) => self.seed = whole,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let ground = cx.input(node, Self::GROUND, at);
        // Water already standing on the ground, as a coarser network's rivers: read with the ground at each point.
        let standing = cx.input(node, Self::STANDING, at);
        let program = match standing {
            Some(_) => cx.inputs_program(node, &[Self::GROUND, Self::STANDING]),
            None => cx.input_program(node, Self::GROUND),
        };
        let (Some(ground), Some(program)) = (ground, program) else {
            return match port {
                Self::HEIGHT => cx.constant(0.0),
                Self::WATER => cx.constant(NO_WATER),
                _ => cx.constant(0.0),
            };
        };
        let seed = seed_for(cx, node, self.seed);
        let finder = Arc::new(Finder::new(self, cx.world.scale.max(0.01), cx.world.sea_level, seed, program));
        let ran = Ran { finder, what: port.min(Self::BANKS) as u8 };
        match standing {
            Some(standing) => cx.run(ran, &[at.x, at.z, ground, standing]),
            None => cx.run(ran, &[at.x, at.z, ground]),
        }
    }
}

/// Runs `rivers` down `graph`'s ground: a Rivers node between what is
/// wired into its Output's Height and the Output, its Water and Falls wired
/// into the Output's. False, and nothing done, where it has no Output or no
/// ground wired in.
pub fn add_rivers(graph: &mut crate::generate::Graph, rivers: Rivers) -> bool {
    use crate::generate::{NodeKind, nodes::Output};
    let Some(output) = graph.output().map(|node| (node.id, node.at)) else { return false };
    let Some(link) = graph.link_into(output.0, Output::HEIGHT).copied() else { return false };
    let rivers = graph.add(NodeKind::Rivers(rivers), [output.1[0] - 280.0, output.1[1] + 380.0]);
    let wired = graph.connect(link.from, link.output, rivers, Rivers::GROUND)
        && graph.connect(rivers, Rivers::HEIGHT, output.0, Output::HEIGHT)
        && graph.connect(rivers, Rivers::WATER, output.0, Output::WATER)
        && graph.connect(rivers, Rivers::FALLS, output.0, Output::FALLS);
    debug_assert!(wired, "a world's rivers wire up");
    wired
}

/// Runs `great`, a coarser network for great rivers only, down `graph`'s
/// ground, and `rivers` down the ground it leaves, its rivers running into
/// the great rivers' water as into the sea: two Rivers nodes between what
/// is wired into its Output's Height and the Output, the higher of their
/// Water and of their Falls wired into the Output's. False, and nothing
/// done, where it has no Output or no ground wired in.
pub fn add_great_rivers(graph: &mut crate::generate::Graph, great: Rivers, rivers: Rivers) -> bool {
    use crate::generate::{
        NodeKind,
        nodes::{Math, Operation, Output},
    };
    let Some(output) = graph.output().map(|node| (node.id, node.at)) else { return false };
    let Some(link) = graph.link_into(output.0, Output::HEIGHT).copied() else { return false };
    let (x, y) = (output.1[0], output.1[1]);
    let great = graph.add(NodeKind::Rivers(great), [x - 560.0, y + 380.0]);
    let fine = graph.add(NodeKind::Rivers(rivers), [x - 280.0, y + 380.0]);
    let higher = || NodeKind::Math(Math { operation: Operation::Max, a: 0.0, b: 0.0 });
    let water = graph.add(higher(), [x - 140.0, y + 300.0]);
    let falls = graph.add(higher(), [x - 140.0, y + 460.0]);
    let wired = graph.connect(link.from, link.output, great, Rivers::GROUND)
        && graph.connect(great, Rivers::HEIGHT, fine, Rivers::GROUND)
        && graph.connect(great, Rivers::WATER, fine, Rivers::STANDING)
        && graph.connect(fine, Rivers::HEIGHT, output.0, Output::HEIGHT)
        && graph.connect(great, Rivers::WATER, water, 0)
        && graph.connect(fine, Rivers::WATER, water, 1)
        && graph.connect(water, 0, output.0, Output::WATER)
        && graph.connect(great, Rivers::FALLS, falls, 0)
        && graph.connect(fine, Rivers::FALLS, falls, 1)
        && graph.connect(falls, 0, output.0, Output::FALLS);
    debug_assert!(wired, "a world's great rivers wire up");
    wired
}

/// How many points a side a tile of them is, worked out at once.
const TILE: i32 = 32;

/// How far up a point's way the ground draining through it is counted, in
/// points.
const REACH: i32 = 8;

/// How many points either way of its own cell a place looks at for rivers
/// running near it.
const LOOKED: i32 = 2;

/// How many points down its way a river is carried on, at most: its Run On.
const MOST_RUN: i32 = 64;

/// How far water spilt from a hollow is followed to see whether it comes
/// back to it, in steps.
const SPILL_STEPS: usize = 8;

/// How many points from a hollow it may spill over to, and where rivers
/// are carried on, as a great river is across flat land to the sea.
const SPILL: usize = 2;
const CARRIED_SPILL: usize = 4;

/// How far off its place in the grid a point is, at most, as a share of
/// the spacing.
const JITTER: f32 = 0.7;

/// How high a bank stands over its water at the water's edge, in metres: a
/// quarter of a block of a metre, so in a world of blocks laid in half
/// blocks the bank's top block is level with the water's top or half a
/// block over it, and one walks straight out.
const LIP: f32 = 0.25;

/// How far under its water a river's bed lies at its edge, in metres.
const SHELF: f32 = 0.15;

/// How far back from a river's edge the land rises to the valley's side
/// where the river has cut into its bank, as on the outside of a bend, as a
/// share of the valley's width: elsewhere the bank shelves over the whole
/// valley, a low strip along the water. And how long a stretch of either
/// kind runs, about, in metres.
const CUT: f32 = 0.3;
const BANK_STRETCH: f32 = 110.0;

/// How many metres back from the water a bank reaches, at least, for each
/// metre it rises, within the valley: at most about a 1 in 1.5 slope, so
/// a river cut down into the ground has sides of the ground's own top and
/// not bare rock.
const BANK_RUN: f32 = 3.0;

/// Where the ground beside a river, a pond or a lake lies lower than its
/// water, how far past the water's edge the rim that holds it in stands at
/// its lip, and over how many metres more it slopes down to the ground.
const RIM: f32 = 1.5;
const LEVEE: f32 = 7.0;

/// How far past the edge of a river's, a pool's or a lake's water the
/// ground is read to keep its water under, in metres, and how far apart
/// along a river: water is kept a quarter of a block under the lowest of
/// it, so the rim above is seldom raised, only where the ground dips
/// between the places read.
const EDGE_READS: [f32; 3] = [0.5, 2.0, 4.0];
const READ_STEP: f32 = 4.0;

/// How far round a pond dug into rising ground its sides slope up to the
/// ground, in metres.
const POOL_SIDE: f32 = 8.0;

/// How far a lake reaches from its hollow's point, in spacings.
const LAKE_REACH: f32 = 1.6;

/// How much a point counts towards what drains through it: as much more
/// as it stands over the lowest ground within [`RISE_ROUND`] points of it,
/// a whole point for every [`RISE_FULL`] metres, from [`LEAST_WEIGHT`] for
/// flat ground to [`MOST_WEIGHT`].
const RISE_ROUND: i32 = 2;
const RISE_FULL: f32 = 12.0;
const LEAST_WEIGHT: f32 = 0.25;
const MOST_WEIGHT: f32 = 2.5;

/// A spring's pool at a river's head and a pond where one ends: how wide,
/// as a share of the river's width and at least so many metres, and how
/// deep at its middle under its water, in metres.
const POOL_WIDTHS: f32 = 1.4;
const LEAST_POOL: f32 = 7.0;
const POOL_DEPTH: f32 = 1.6;

/// How far over the sea a river's water is still its mouth, in metres: it
/// widens and its banks fall away as its water comes down to the sea, as
/// many times wider at the sea as [`MOUTH_WIDENS`] says, its bed at least
/// [`MOUTH_FLOOR`] metres under the sea, so a world of blocks shows its
/// water meeting the sea's.
const MOUTH: f32 = 3.0;
const MOUTH_WIDENS: f32 = 2.5;
const MOUTH_FLOOR: f32 = 1.5;

/// How far a river running into the sea runs on past its last point, as a
/// share of the stretch to it.
const MOUTH_REACH: f32 = 2.0;

/// How far under the sea a river still runs on to a point, in metres, so
/// it reaches the shore across a beach whose points stand in the shallows.
const SHALLOWS: f32 = 6.0;

/// Into how many straight pieces a river's curve round a turn is cut, to
/// find how near a place is to it.
const BEND_PIECES: usize = 8;

/// Where rivers are carried on, how wide and deep one is at its head, as a
/// share of how wide and deep it would be from what drains into it there:
/// it widens down its first stretch.
const HEAD_WIDTH: f32 = 0.3;
const HEAD_DEPTH: f32 = 0.5;

/// How thick a waterfall's curtain is, in metres.
const CURTAIN: f32 = 1.6;

/// How far past a waterfall its pool is dug deeper, in metres.
const POOL: f32 = 8.0;

/// How much of a waterfall's drop its river falls over the stretches before
/// and after it, each.
const RUN_DOWN: f32 = 0.04;

/// How much each point round one, and the point itself, counts towards the
/// ground it is taken to stand on.
const AVERAGED: [(i32, i32, f32); 9] = [
    (-1, -1, 1.0 / 16.0),
    (0, -1, 2.0 / 16.0),
    (1, -1, 1.0 / 16.0),
    (-1, 0, 2.0 / 16.0),
    (0, 0, 4.0 / 16.0),
    (1, 0, 2.0 / 16.0),
    (-1, 1, 1.0 / 16.0),
    (0, 1, 2.0 / 16.0),
    (1, 1, 1.0 / 16.0),
];

/// The eight points round one, as steps across the grid.
const ROUND: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// One point of the grid water finds its way over.
#[derive(Clone, Copy, Debug, Default)]
struct Point {
    at: [f32; 2],
    /// The point it drains to, as a step across the grid; none for a hollow.
    down: Option<[i8; 2]>,
    /// Whether a river runs from it down its way.
    river: bool,
    /// Whether it is a hollow holding a lake.
    lake: bool,
    /// Whether a river runs from a point as near as a place in its cell
    /// looks, or a lake lies at one: where none does, its cell is left be.
    near: bool,
    /// How high its river's water stands at it, or its lake's.
    level: f32,
    width: f32,
    depth: f32,
    /// How far a pool reaches from it, its water as high as its river's:
    /// a spring's at a river's head, or a pond where a river ends in a
    /// hollow; 0 for none.
    pool: f32,
    /// Whether it lies under water already standing on the ground, which
    /// a river runs into as into the sea.
    drowned: bool,
    /// Whether a river runs into it.
    fed: bool,
}

/// A Rivers node's settings held to their ranges, for the ground its
/// program makes, and the points worked out from them.
#[derive(Debug)]
struct Finder {
    spacing: f32,
    /// How much must drain into a point for a river to start there, and for
    /// a hollow to hold a lake (none at 0), in points of flat ground.
    source: f32,
    lakes: f32,
    width: f32,
    depth: f32,
    valley: f32,
    falls: f32,
    meander: f32,
    /// How many points down its way a river is carried on past where it
    /// starts, as wide as there, and how many points from a hollow it may
    /// spill over to.
    run: i32,
    spill: usize,
    /// How many points either way of its own cell a place looks at for
    /// rivers running near it.
    looked: i32,
    /// How many points round a tile what drains into each is counted for:
    /// those a place in it looks at, and the hollows they run into.
    known: i32,
    /// How far past a tile the points are read to count what drains into
    /// those in it and round it: as far as any point counted can be, the
    /// way from there, as far as a river is carried on, and as far as water
    /// spilt from a hollow on it is followed, so every tile finds the same
    /// ways for its points.
    margin: i32,
    sea: f32,
    seed: u32,
    /// The ground's program, and with it the water standing on the ground
    /// where that is wired in.
    ground: Arc<Program>,
    standing: bool,
    id: u64,
}

/// The tiles of points last worked out, by finder and tile, shared by every
/// thread: a tile takes a while to work out, and the chunks round it are
/// made on many threads at once. Each is worked out by the first thread to
/// ask for it, and any other asking meanwhile waits for it rather than
/// working it out too.
type Tile = Arc<std::sync::OnceLock<Arc<TileMade>>>;

/// A tile's points, and where rivers are carried on, for each of its
/// cells, the points whose rivers, pools or lakes reach a place in it, as
/// steps from it: from `starts[cell]` to `starts[cell + 1]` of `reach`.
#[derive(Debug, Default)]
struct TileMade {
    points: Vec<Point>,
    starts: Vec<u32>,
    reach: Vec<[i8; 2]>,
}
static TILES: Mutex<Vec<(u64, [i32; 2], Tile)>> = Mutex::new(Vec::new());

/// How many tiles are kept.
const KEPT: usize = 48;

/// The ground at the points of blocks of them `GROUND_BLOCK` a side, and
/// the water standing on it, by the ground's program, the seed and the
/// spacing, and the block: kept, shared
/// by every thread, so the tiles round one need not read again what their
/// margins share.
type Grounds = std::collections::HashMap<(u64, [i32; 2]), Arc<Vec<[f32; 2]>>>;
static GROUNDS: std::sync::LazyLock<Mutex<Grounds>> = std::sync::LazyLock::new(Default::default);
const GROUND_BLOCK: i32 = 16;
/// How many blocks are kept before all are let go.
const GROUNDS_KEPT: usize = 8192;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Finder {
    fn new(set: &Rivers, scale: f32, sea: f32, seed: u32, ground: Arc<Program>) -> Self {
        let number = |value: f32, low: f32, high: f32| if value.is_nan() { low } else { value.clamp(low, high) };
        let spacing = number(set.spacing, 16.0, 1000.0) * scale;
        // A point drains this many hectares.
        let hectares = spacing * spacing / 10_000.0;
        let points = |area: f32| (area / hectares).round().max(1.0);
        let run = (number(set.run_on, 0.0, 50000.0) * scale / spacing).ceil().min(MOST_RUN as f32) as i32;
        let standing = ground.outputs.len() > 1;
        let spill = if run > 0 { CARRIED_SPILL } else { SPILL };
        // A place looks as far round for rivers as a hollow spills, so it finds the whole of a stretch over a rim;
        // where turns are drawn as curves, as far again as halfway along the next stretch.
        let looked = if run > 0 { spill as i32 + 2 } else { LOOKED };
        let known = looked + spill as i32 + 1;
        let mut key = Key::default();
        key.name("rivers").number(spacing).number(set.source).number(set.lakes).number(set.width);
        key.number(set.depth).number(set.valley).number(set.falls).number(set.meander).number(sea).whole(seed);
        key.whole(ground.hash as u32).whole((ground.hash >> 32) as u32);
        if run > 0 || standing {
            key.whole(run as u32).whole(u32::from(standing));
        }
        Self {
            spacing,
            source: points(number(set.source, 0.5, 100000.0)),
            lakes: if set.lakes > 0.0 { points(number(set.lakes, 0.0, 100000.0)) } else { 0.0 },
            width: number(set.width, 1.0, 1000.0) * scale,
            depth: number(set.depth, 0.2, 100.0),
            valley: number(set.valley, 0.0, 2000.0) * scale,
            falls: number(set.falls, 1.0, 1000.0),
            meander: number(set.meander, 0.0, 1.0),
            run,
            spill,
            looked,
            known,
            margin: known + 2 * REACH + SPILL_STEPS as i32 + spill as i32 + 4 + run,
            sea,
            seed,
            ground,
            standing,
            id: key.hash(),
        }
    }

    /// Where point `cell` of the grid lies.
    fn place(&self, cell: [i32; 2]) -> [f32; 2] {
        let pick = |part: i32| share(mix(self.seed, [cell[0], cell[1], part]));
        [
            (cell[0] as f32 + 0.5 + JITTER * (pick(0) - 0.5)) * self.spacing,
            (cell[1] as f32 + 0.5 + JITTER * (pick(1) - 0.5)) * self.spacing,
        ]
    }

    /// The points of tile `tile`, kept from the last time they were worked
    /// out where they can be.
    fn tile(&self, tile: [i32; 2]) -> Arc<TileMade> {
        let cell = {
            let mut tiles = TILES.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match tiles.iter().find(|(id, known, _)| *id == self.id && *known == tile) {
                Some(kept) => kept.2.clone(),
                None => {
                    // Each network keeps its own last few, so a fine one's many tiles never push out a coarse one's.
                    if tiles.iter().filter(|(id, _, _)| *id == self.id).count() >= KEPT
                        && let Some(oldest) = tiles.iter().position(|(id, _, _)| *id == self.id)
                    {
                        tiles.remove(oldest);
                    }
                    let cell = Tile::default();
                    tiles.push((self.id, tile, cell.clone()));
                    cell
                }
            }
        };
        // Worked out with the tiles let go of, so threads wanting other tiles go on.
        cell.get_or_init(|| Arc::new(self.make_tile(tile))).clone()
    }

    /// The ground at the points of the `side` by `side` square from point
    /// `first`, row after row, and the water standing on it: read from the
    /// blocks of points kept, those not kept read now, all at once, and kept.
    fn grounds(&self, first: [i32; 2], side: usize) -> Vec<[f32; 2]> {
        let block = |v: i32| v.div_euclid(GROUND_BLOCK);
        let (low, high) = (
            [block(first[0]), block(first[1])],
            [block(first[0] + side as i32 - 1), block(first[1] + side as i32 - 1)],
        );
        let wanted: Vec<[i32; 2]> = (low[1]..=high[1]).flat_map(|z| (low[0]..=high[0]).map(move |x| [x, z])).collect();
        let mut found: Vec<([i32; 2], Arc<Vec<[f32; 2]>>)> = {
            let kept = GROUNDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let id = self.ground.hash ^ u64::from(self.seed) ^ u64::from(self.spacing.to_bits()) << 32;
            wanted.iter().filter_map(|&at| Some((at, kept.get(&(id, at))?.clone()))).collect()
        };
        let missing: Vec<[i32; 2]> =
            wanted.iter().copied().filter(|at| found.iter().all(|(known, _)| known != at)).collect();
        if !missing.is_empty() {
            let each = (GROUND_BLOCK * GROUND_BLOCK) as usize;
            let (xs, zs): (Vec<f32>, Vec<f32>) = missing
                .iter()
                .flat_map(|at| {
                    (0..each as i32).map(move |k| {
                        self.place([at[0] * GROUND_BLOCK + k % GROUND_BLOCK, at[1] * GROUND_BLOCK + k / GROUND_BLOCK])
                    })
                })
                .map(|at| (at[0], at[1]))
                .unzip();
            let mut read = vec![0.0; xs.len()];
            let mut standing = vec![NO_WATER; xs.len()];
            let givens = Givens { x: &xs, z: &zs, ..Givens::default() };
            match self.standing {
                true => self.ground.run(xs.len(), givens, &mut [&mut read, &mut standing]),
                false => self.ground.run(xs.len(), givens, &mut [&mut read]),
            }
            let id = self.ground.hash ^ u64::from(self.seed) ^ u64::from(self.spacing.to_bits()) << 32;
            let mut kept = GROUNDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if kept.len() + missing.len() > GROUNDS_KEPT {
                kept.clear();
            }
            for (k, &at) in missing.iter().enumerate() {
                let both = (k * each..(k + 1) * each).map(|at| [read[at], standing[at]]);
                let values = Arc::new(both.collect::<Vec<_>>());
                kept.insert((id, at), values.clone());
                found.push((at, values));
            }
        }
        let mut grounds = vec![[0.0; 2]; side * side];
        for (at, ground) in grounds.iter_mut().enumerate() {
            let (x, z) = (first[0] + (at % side) as i32, first[1] + (at / side) as i32);
            let held = [block(x), block(z)];
            let values = &found.iter().find(|(known, _)| *known == held).expect("read above").1;
            *ground = values[(z.rem_euclid(GROUND_BLOCK) * GROUND_BLOCK + x.rem_euclid(GROUND_BLOCK)) as usize];
        }
        grounds
    }

    fn make_tile(&self, tile: [i32; 2]) -> TileMade {
        let margin = self.margin;
        let first = [tile[0] * TILE - margin, tile[1] * TILE - margin];
        let side = (TILE + 2 * margin) as usize;
        let index = |x: usize, z: usize| z * side + x;
        let places: Vec<[f32; 2]> = (0..side * side)
            .map(|at| self.place([first[0] + (at % side) as i32, first[1] + (at / side) as i32]))
            .collect();
        let both = self.grounds(first, side);
        let read: Vec<f32> = both.iter().map(|both| both[0]).collect();
        // Which points lie under water already standing on the ground, and how high it stands.
        let standing: Vec<f32> = both.iter().map(|both| both[1]).collect();
        let drowned: Vec<bool> = (0..both.len()).map(|at| standing[at] > read[at]).collect();
        // Each point's ground averaged with the eight round it, so water finds the broad valleys and not the gullies
        // and bumps between points, which leave no hollows to stop it. The window's outer ring keeps its own.
        let mut ground = read.clone();
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let mut sum = 0.0;
                for (dx, dz, weight) in AVERAGED {
                    sum += read[index((x as i32 + dx) as usize, (z as i32 + dz) as usize)] * weight;
                }
                ground[index(x, z)] = sum;
            }
        }
        // Where each point drains: the steepest way down to one of the eight round it. One with none lower is a
        // hollow.
        let mut down: Vec<Option<[i8; 2]>> = vec![None; places.len()];
        let mut hollow = vec![false; places.len()];
        for z in 2..side - 2 {
            for x in 2..side - 2 {
                let here = index(x, z);
                let mut steepest = 0.0f32;
                for (dx, dz) in ROUND {
                    let there = index((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                    let fall = ground[here] - ground[there];
                    if fall <= 0.0 {
                        continue;
                    }
                    let (ax, az) = (places[there][0] - places[here][0], places[there][1] - places[here][1]);
                    let slope = fall / (ax * ax + az * az).sqrt().max(1e-3);
                    if slope > steepest {
                        (steepest, down[here]) = (slope, Some([dx as i8, dz as i8]));
                    }
                }
                hollow[here] = down[here].is_none();
            }
        }
        // A hollow spills over to the lowest point within two of it whose way down does not lead back into it, its
        // river cutting through the ground between; one with none keeps its water, a sink. Each hollow is decided
        // by the ways down alone, near it, so every tile decides it alike.
        let steepest = down.clone();
        let mut spill: Vec<(usize, f32)> = Vec::with_capacity(24);
        let spill_reach = self.spill;
        for z in spill_reach + 1..side - spill_reach - 1 {
            for x in spill_reach + 1..side - spill_reach - 1 {
                let here = index(x, z);
                if !hollow[here] {
                    continue;
                }
                spill.clear();
                let reach = spill_reach as i32;
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        if (dx, dz) != (0, 0) {
                            spill.push((index((x as i32 + dx) as usize, (z as i32 + dz) as usize), 0.0));
                        }
                    }
                }
                for candidate in spill.iter_mut() {
                    candidate.1 = ground[candidate.0];
                }
                spill.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                let leads_back = |from: usize| {
                    let (mut vx, mut vz) = (from % side, from / side);
                    for _ in 0..SPILL_STEPS {
                        let Some([dx, dz]) = steepest[index(vx, vz)] else { return index(vx, vz) == here };
                        (vx, vz) = ((vx as i32 + i32::from(dx)) as usize, (vz as i32 + i32::from(dz)) as usize);
                        if vx < 1 || vz < 1 || vx >= side - 1 || vz >= side - 1 {
                            return false;
                        }
                    }
                    index(vx, vz) == here
                };
                // Where rivers are carried on, a hollow spills only where its water goes on down to lower ground than
                // its own, or the sea: so water never comes round to where it was, and a river carried on never runs
                // in a ring.
                let lower = |from: usize| {
                    let (mut vx, mut vz) = (from % side, from / side);
                    for _ in 0..=SPILL_STEPS {
                        let there = index(vx, vz);
                        if ground[there] < ground[here] {
                            return true;
                        }
                        let Some([dx, dz]) = steepest[there] else { return false };
                        (vx, vz) = ((vx as i32 + i32::from(dx)) as usize, (vz as i32 + i32::from(dz)) as usize);
                        if vx < 1 || vz < 1 || vx >= side - 1 || vz >= side - 1 {
                            return false;
                        }
                    }
                    false
                };
                let goes = |candidate: usize| match self.run > 0 {
                    true => lower(candidate),
                    false => !leads_back(candidate),
                };
                if let Some(&(over, _)) = spill.iter().find(|(candidate, _)| goes(*candidate)) {
                    let (ox, oz) = ((over % side) as i32 - x as i32, (over / side) as i32 - z as i32);
                    down[here] = Some([ox as i8, oz as i8]);
                }
            }
        }
        // How many points drain through each of the tile's, and those round it as far as a river is carried on
        // to them: every point within reach up its way, whichever tile counts it.
        let counted = |x: usize, z: usize| {
            let inner = (margin - self.known - self.run) as usize..(margin + TILE + self.known + self.run) as usize;
            inner.contains(&x) && inner.contains(&z)
        };
        // How much each point counts: more the higher it stands over the ground round it.
        let reach = RISE_ROUND;
        let weight: Vec<f32> = (0..places.len())
            .map(|at| {
                let (x, z) = ((at % side) as i32, (at / side) as i32);
                let mut lowest = ground[at];
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        let (bx, bz) = (x + dx, z + dz);
                        if (0..side as i32).contains(&bx) && (0..side as i32).contains(&bz) {
                            lowest = lowest.min(ground[index(bx as usize, bz as usize)]);
                        }
                    }
                }
                (LEAST_WEIGHT + (ground[at] - lowest) / RISE_FULL).min(MOST_WEIGHT)
            })
            .collect();
        let mut catch = vec![0.0f32; places.len()];
        // Where rivers are carried on, the lowest ground over the sea up each point's way within reach: a river's
        // water stands no higher, so it never rises where it spills over a hollow's rim, but cuts through.
        let mut lowest = ground.clone();
        let mut been: Vec<(i32, i32)> = Vec::new();
        for z in 1..side - 1 {
            for x in 1..side - 1 {
                let (mut vx, mut vz) = (x as i32, z as i32);
                been.clear();
                loop {
                    // Water spilling from a hollow may come round to it again: it is counted once.
                    if been.contains(&(vx, vz)) {
                        break;
                    }
                    been.push((vx, vz));
                    let (ox, oz) = (vx - x as i32, vz - z as i32);
                    if ox * ox + oz * oz > REACH * REACH {
                        break;
                    }
                    if counted(vx as usize, vz as usize) {
                        let there = index(vx as usize, vz as usize);
                        catch[there] += weight[index(x, z)];
                        lowest[there] = lowest[there].min(ground[index(x, z)].max(self.sea));
                    }
                    let Some([dx, dz]) = down[index(vx as usize, vz as usize)] else { break };
                    (vx, vz) = (vx + i32::from(dx), vz + i32::from(dz));
                    if vx < 1 || vz < 1 || vx >= side as i32 - 1 || vz >= side as i32 - 1 {
                        break;
                    }
                }
            }
        }
        // A river carried on down its way from where it starts, as far as Run On says, as wide as there: on across
        // flat land where too little drains in to keep it going, until it meets the sea or standing water, or ends in
        // a hollow.
        let mut carried = catch.clone();
        if self.run > 0 {
            let inner = (margin - self.known - self.run) as usize..(margin + TILE + self.known + self.run) as usize;
            let starts = |at: usize| {
                catch[at] >= self.source && !drowned[at] && ground[at] >= self.sea - SHALLOWS && down[at].is_some()
            };
            for z in 1..side - 1 {
                for x in 1..side - 1 {
                    let start = index(x, z);
                    if !starts(start) {
                        continue;
                    }
                    let (mut vx, mut vz) = (x as i32, z as i32);
                    been.clear();
                    while let Some([dx, dz]) = down[index(vx as usize, vz as usize)] {
                        (vx, vz) = (vx + i32::from(dx), vz + i32::from(dz));
                        let far = (vx - x as i32).abs().max((vz - z as i32).abs());
                        if far > self.run || vx < 1 || vz < 1 || vx >= side as i32 - 1 || vz >= side as i32 - 1 {
                            break;
                        }
                        let there = index(vx as usize, vz as usize);
                        if been.contains(&(vx, vz)) || drowned[there] || ground[there] < self.sea - SHALLOWS {
                            break;
                        }
                        been.push((vx, vz));
                        if inner.contains(&(vx as usize)) && inner.contains(&(vz as usize)) {
                            carried[there] = carried[there].max(catch[start]);
                            lowest[there] = lowest[there].min(lowest[start]);
                        }
                    }
                }
            }
        }
        // A hollow's lake, as high as the lowest of the ground round it, where enough drains into it.
        let lake = |x: usize, z: usize| -> Option<f32> {
            let here = index(x, z);
            if self.lakes == 0.0
                || !hollow[here]
                || drowned[here]
                || catch[here] < self.lakes
                || ground[here] < self.sea
            {
                return None;
            }
            // As high as the lowest of the ground round it, and no higher than where it spills.
            let mut rim = f32::MAX;
            for (dx, dz) in ROUND {
                rim = rim.min(ground[index((x as i32 + dx) as usize, (z as i32 + dz) as usize)]);
            }
            if let Some([dx, dz]) = down[here] {
                rim = rim.min(ground[index((x as i32 + i32::from(dx)) as usize, (z as i32 + i32::from(dz)) as usize)]);
            }
            let level = rim - 0.3;
            (level > ground[here] + 0.5).then_some(level)
        };
        let size = |catch: f32| {
            let area = catch * self.spacing * self.spacing / 1_000_000.0;
            let root = area.sqrt();
            ((self.width * root).clamp(2.5, 120.0), (self.depth * root.sqrt()).clamp(1.0, 20.0))
        };
        // Whether a river runs from a point: enough drains into it, it is over the sea or in its shallows, so it runs
        // on across the shore, and it drains on.
        let river = |at: usize| {
            carried[at] >= self.source && ground[at] >= self.sea - SHALLOWS && down[at].is_some() && !drowned[at]
        };
        let point = |x: usize, z: usize| {
            let here = index(x, z);
            let (width, depth) = size(carried[here]);
            let mut point = Point { at: places[here], down: down[here], width, depth, ..Point::default() };
            if drowned[here] {
                // Under water standing on the ground: a river runs into it as into the sea.
                (point.drowned, point.level) = (true, standing[here]);
                return point;
            }
            let pool = (width * POOL_WIDTHS).max(LEAST_POOL);
            if let Some(level) = lake(x, z) {
                // Its water runs on from it where it spills, as a river where enough drains into it.
                (point.lake, point.level) = (true, level);
                point.river = carried[here] >= self.source;
            } else {
                point.river = river(here);
                let mut level = match self.run > 0 {
                    true => lowest[here].min(ground[here]) - depth,
                    false => ground[here] - depth,
                };
                // Running into a lake, as high as it, and into water standing on the ground.
                if let Some([dx, dz]) = down[here] {
                    let (tx, tz) = ((x as i32 + i32::from(dx)) as usize, (z as i32 + i32::from(dz)) as usize);
                    if let Some(lake) = lake(tx, tz) {
                        level = level.max(lake);
                    }
                    if drowned[index(tx, tz)] {
                        level = level.max(standing[index(tx, tz)]);
                    }
                }
                point.level = level.max(self.sea);
                // A river's head, which no river runs into, rises from a spring's pool. Where rivers are carried on,
                // one spilling into it from further than the points round it counts too.
                let feeds = |dx: i32, dz: i32| {
                    let there = index((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                    down[there] == Some([-dx as i8, -dz as i8]) && river(there)
                };
                let fed = match self.run > 0 {
                    true => {
                        let reach = self.spill as i32;
                        (-reach..=reach).any(|dz| (-reach..=reach).any(|dx| (dx, dz) != (0, 0) && feeds(dx, dz)))
                    }
                    false => ROUND.iter().any(|&(dx, dz)| feeds(dx, dz)),
                };
                point.fed = fed;
                if point.river && !fed {
                    match self.run > 0 {
                        // A river carried on, as a great one is, rises narrow where the streams feeding it gather.
                        true => (point.width, point.depth) = (width * HEAD_WIDTH, depth * HEAD_DEPTH),
                        false => point.pool = pool,
                    }
                }
                // A river that runs into a hollow with no lake to hold it ends in a pond there.
                if down[here].is_none() && carried[here] >= self.source && ground[here] >= self.sea + MOUTH {
                    (point.pool, point.level) = (pool, ground[here] - 0.3);
                }
            }
            point
        };
        // The tile's points and those round it as far as a place in it looks.
        let looked = self.looked;
        let from = (margin - looked) as usize;
        let wide = (TILE + 2 * looked) as usize;
        // Those points and as many more round them as a river's water is kept no higher than up its way: as far as
        // what drains through each is counted.
        let up = (self.spill as i32 + 1 + self.run).min(REACH);
        let (first_up, wide_up) = (from - up as usize, wide + 2 * up as usize);
        let mut all: Vec<Point> =
            (0..wide_up * wide_up).map(|at| point(first_up + at % wide_up, first_up + at / wide_up)).collect();
        self.sit_in_ground(&mut all, wide_up, up as usize, |x, z| {
            point((first_up as i32 + x) as usize, (first_up as i32 + z) as usize)
        });
        let round: Vec<Point> =
            (0..wide * wide).map(|at| all[(at / wide + up as usize) * wide_up + at % wide + up as usize]).collect();
        let wet = |x: usize, z: usize| {
            let point = &round[z * wide + x];
            point.river || point.lake || point.pool > 0.0
        };
        let mut points = Vec::with_capacity((TILE * TILE) as usize);
        for z in looked as usize..(looked + TILE) as usize {
            for x in looked as usize..(looked + TILE) as usize {
                let reach = looked as usize;
                let near = (z - reach..=z + reach).any(|z| (x - reach..=x + reach).any(|x| wet(x, z)));
                points.push(Point { near, ..round[z * wide + x] });
            }
        }
        if self.run == 0 {
            return TileMade { points, ..TileMade::default() };
        }
        // Where rivers are carried on, a place looks far round, so each cell keeps the points whose river, pool or
        // lake can reach a place in it, and a place looks at those alone.
        let mut each: Vec<Vec<[i8; 2]>> = vec![Vec::new(); (TILE * TILE) as usize];
        let valley = self.valley.max(RIM + LEVEE);
        for (at, from_point) in round.iter().enumerate() {
            let (x, z) = (from + at % wide, from + at / wide);
            let mut reached: Vec<[f32; 2]> = vec![from_point.at];
            let mut pad: f32 = 0.0;
            if from_point.lake {
                pad = pad.max(LAKE_REACH * self.spacing + RIM + LEVEE);
            }
            if from_point.pool > 0.0 {
                pad = pad.max(from_point.pool + POOL_SIDE.max(RIM + LEVEE));
            }
            if let Some([dx, dz]) = from_point.down.filter(|_| from_point.river) {
                let (tx, tz) = ((x as i32 + i32::from(dx)) as usize, (z as i32 + i32::from(dz)) as usize);
                let to = point(tx, tz);
                let mut widest = from_point.width.max(to.width);
                reached.push(to.at);
                if self.into_sea(&to) {
                    let on = |k: usize| from_point.at[k] + (to.at[k] - from_point.at[k]) * MOUTH_REACH;
                    reached.push([on(0), on(1)]);
                }
                if let Some([ax, az]) = to.down.filter(|_| to.river && !to.lake) {
                    let after = point((tx as i32 + i32::from(ax)) as usize, (tz as i32 + i32::from(az)) as usize);
                    reached.push([(to.at[0] + after.at[0]) * 0.5, (to.at[1] + after.at[1]) * 0.5]);
                    widest = widest.max(after.width);
                }
                pad = pad.max(valley + widest * 0.5 * (1.0 + MOUTH_WIDENS) + 1.0);
            }
            if pad <= 0.0 {
                continue;
            }
            let low = |k: usize| reached.iter().map(|at| at[k]).fold(f32::MAX, f32::min) - pad;
            let high = |k: usize| reached.iter().map(|at| at[k]).fold(f32::MIN, f32::max) + pad;
            let cells = |k: usize| {
                let first = tile[k] * TILE;
                let low = ((low(k) / self.spacing).floor() as i32).max(first);
                let high = ((high(k) / self.spacing).floor() as i32).min(first + TILE - 1);
                low..=high
            };
            let own = [first[0] + x as i32, first[1] + z as i32];
            for cz in cells(1) {
                for cx in cells(0) {
                    let local = ((cz - tile[1] * TILE) * TILE + cx - tile[0] * TILE) as usize;
                    each[local].push([(own[0] - cx) as i8, (own[1] - cz) as i8]);
                }
            }
        }
        let mut made = TileMade { points, starts: Vec::with_capacity(each.len() + 1), reach: Vec::new() };
        for (cell, steps) in each.into_iter().enumerate() {
            made.starts.push(made.reach.len() as u32);
            made.points[cell].near = !steps.is_empty();
            made.reach.extend(steps);
        }
        made.starts.push(made.reach.len() as u32);
        made
    }
}

impl Finder {
    /// Keeps the water of each river, pool and lake among `points`, `side`
    /// a side, a quarter of a block under the ground round its edge: so it
    /// sits in the ground as it lies, filling a hollow or cut down into it,
    /// and no rim need be raised to hold it in. The ground is read along
    /// each river's stretch down its way, a few metres either side of its
    /// water, and round each pool and lake; a river's water falls along
    /// its stretch, so each place is held to the water's height there. And
    /// a river's water is kept no higher than the water `up` points up its
    /// way, so it never rises down it. `point` is the point so many steps
    /// from the first of them, where it is not among them.
    fn sit_in_ground(&self, points: &mut [Point], side: usize, up: usize, point: impl Fn(i32, i32) -> Point) {
        let index = |x: i32, z: i32| -> Option<usize> {
            let inside = (0..side as i32).contains(&x) && (0..side as i32).contains(&z);
            inside.then(|| z as usize * side + x as usize)
        };
        let near = |x: i32, z: i32| index(x, z).map_or_else(|| point(x, z), |at| points[at]);
        // The places round each one's water, in the world as bent by rivers winding, how far along its stretch
        // each is (less than nothing round a pool or a lake, whose water is level), and for each one, which are
        // its and how high the water stands where its river runs to.
        let mut places: Vec<[f32; 2]> = Vec::new();
        let mut along: Vec<f32> = Vec::new();
        let mut spans: Vec<(usize, std::ops::Range<usize>, f32)> = Vec::new();
        for (at, here) in points.iter().enumerate() {
            if here.drowned {
                continue;
            }
            let start = places.len();
            let mut lower = f32::MAX;
            let (x, z) = ((at % side) as i32, (at / side) as i32);
            if let Some([dx, dz]) = here.down.filter(|_| here.river) {
                let (tx, tz) = (x + i32::from(dx), z + i32::from(dz));
                let to = near(tx, tz);
                lower = to.level;
                let wider = to.river || to.lake;
                let width = |t: f32| if wider { here.width + (to.width - here.width) * t } else { here.width };
                // Either side of a line through `on` running `way`, `width` across, `t` along the stretch.
                let mut sides = |on: [f32; 2], way: [f32; 2], width: f32, t: f32| {
                    let long = (way[0] * way[0] + way[1] * way[1]).sqrt().max(1e-3);
                    let across = [-way[1] / long, way[0] / long];
                    for off in EDGE_READS {
                        for sign in [-1.0, 1.0] {
                            let out = sign * (width * 0.5 + off);
                            places.push([on[0] + across[0] * out, on[1] + across[1] * out]);
                            along.push(t);
                        }
                    }
                };
                let way = [to.at[0] - here.at[0], to.at[1] - here.at[1]];
                let steps = ((way[0] * way[0] + way[1] * way[1]).sqrt() / READ_STEP).ceil().max(2.0) as usize;
                for k in 0..=steps {
                    let t = k as f32 / steps as f32;
                    sides([here.at[0] + way[0] * t, here.at[1] + way[1] * t], way, width(t), t);
                }
                // Where rivers are carried on, its turn at the point it runs to is drawn as a curve.
                if let Some([ax, az]) = to.down.filter(|_| self.run > 0 && to.river && !to.lake) {
                    let after = near(tx + i32::from(ax), tz + i32::from(az));
                    let start = [(here.at[0] + to.at[0]) * 0.5, (here.at[1] + to.at[1]) * 0.5];
                    let end = [(to.at[0] + after.at[0]) * 0.5, (to.at[1] + after.at[1]) * 0.5];
                    for k in 0..=BEND_PIECES / 2 {
                        let u = k as f32 / BEND_PIECES as f32;
                        let (a, b, c) = ((1.0 - u) * (1.0 - u), 2.0 * u * (1.0 - u), u * u);
                        let on = |i: usize| a * start[i] + b * to.at[i] + c * end[i];
                        let tangent =
                            |i: usize| 2.0 * (1.0 - u) * (to.at[i] - start[i]) + 2.0 * u * (end[i] - to.at[i]);
                        sides([on(0), on(1)], [tangent(0), tangent(1)], width(0.5 + u), 0.5 + u);
                    }
                }
            }
            // Round a pool, and round a lake as far as it reaches.
            let mut ring = |reach: f32, count: usize| {
                for off in EDGE_READS {
                    for k in 0..count {
                        let angle = k as f32 / count as f32 * std::f32::consts::TAU;
                        places
                            .push([here.at[0] + angle.cos() * (reach + off), here.at[1] + angle.sin() * (reach + off)]);
                        along.push(-1.0);
                    }
                }
            };
            if here.pool > 0.0 {
                ring(here.pool, 24);
            }
            if here.lake {
                ring(LAKE_REACH * self.spacing, 64);
            }
            if places.len() > start {
                spans.push((at, start..places.len(), lower));
            }
        }
        if spans.is_empty() {
            return;
        }
        // The ground there, or water already standing on it, which holds water in as well.
        let bend = self.meander * self.spacing * 0.45;
        let wide = self.spacing * 1.7;
        let (xs, zs): (Vec<f32>, Vec<f32>) = places
            .iter()
            .map(|&[wx, wz]| {
                // The place the winding moves there: found a step at a time, as the winding changes slowly.
                let (mut x, mut z) = (wx, wz);
                for _ in 0..4 {
                    (x, z) = (
                        wx - bend * gradient(x / wide, z / wide, self.seed.wrapping_add(11)),
                        wz - bend * gradient(x / wide, z / wide, self.seed.wrapping_add(12)),
                    );
                }
                (x, z)
            })
            .unzip();
        let mut read = vec![0.0; xs.len()];
        let mut standing = vec![NO_WATER; xs.len()];
        let givens = Givens { x: &xs, z: &zs, ..Givens::default() };
        match self.standing {
            true => self.ground.run(xs.len(), givens, &mut [&mut read, &mut standing]),
            false => self.ground.run(xs.len(), givens, &mut [&mut read]),
        }
        let held: Vec<f32> = (0..read.len()).map(|k| read[k].max(standing[k]) - LIP).collect();
        // The highest each one's water may stand: where it is too high anywhere, found a step at a time between
        // that and as low as the lowest of the ground read round it, where it would lie level.
        let mut most = vec![f32::MAX; points.len()];
        for (at, span, lower) in spans {
            let level = points[at].level;
            let fits = |upper: f32| {
                span.clone().all(|k| match along[k] < 0.0 {
                    true => upper <= held[k],
                    false => self.water_along(upper, lower.min(upper), along[k]) <= held[k],
                })
            };
            if fits(level) {
                continue;
            }
            let (mut low, mut high) = (span.clone().map(|k| held[k]).fold(level, f32::min), level);
            for _ in 0..16 {
                let middle = (low + high) * 0.5;
                if fits(middle) { low = middle } else { high = middle }
            }
            most[at] = low;
        }
        // A river's water no higher than the water up its way: as far down it as `up` points, but into a lake.
        let mut kept = most.clone();
        for at in 0..points.len() {
            if most[at] == f32::MAX || !points[at].river {
                continue;
            }
            let (mut x, mut z) = ((at % side) as i32, (at / side) as i32);
            for _ in 0..up {
                let Some([dx, dz]) = points[index(x, z).expect("inside")].down else { break };
                (x, z) = (x + i32::from(dx), z + i32::from(dz));
                let Some(there) = index(x, z).filter(|&there| !points[there].lake && !points[there].drowned) else {
                    break;
                };
                kept[there] = kept[there].min(most[at]);
            }
        }
        for (here, kept) in points.iter_mut().zip(kept) {
            if kept < here.level && !here.drowned {
                here.level = kept.max(self.sea);
            }
        }
    }

    /// How high a river's water stands `t` of the way along a stretch from
    /// `upper` high to `lower`, as `reached` lays it: falling evenly, or
    /// where it drops more than Falls, nearly level to a waterfall halfway
    /// and nearly level on from its foot.
    fn water_along(&self, upper: f32, lower: f32, t: f32) -> f32 {
        let drop = upper - lower;
        if drop <= self.falls {
            return upper - drop * t;
        }
        match t < 0.5 {
            true => upper - drop * RUN_DOWN * (t / 0.5),
            false => {
                let foot = lower + drop * RUN_DOWN;
                foot - (foot - lower) * ((t - 0.5) / 0.5)
            }
        }
    }
}

/// Points looked up by their place in the grid, the tiles they are in kept
/// at hand while a row is worked out.
struct Near<'a> {
    finder: &'a Finder,
    tiles: Vec<([i32; 2], Arc<TileMade>)>,
}

impl Near<'_> {
    fn point(&mut self, cell: [i32; 2]) -> Point {
        let (tile, local) = self.made(cell);
        tile.points[local]
    }

    /// The points whose rivers, pools or lakes reach a place in `cell`,
    /// where rivers are carried on.
    fn reaching(&mut self, cell: [i32; 2]) -> Vec<[i32; 2]> {
        let (tile, local) = self.made(cell);
        let steps = &tile.reach[tile.starts[local] as usize..tile.starts[local + 1] as usize];
        steps.iter().map(|&[dx, dz]| [cell[0] + i32::from(dx), cell[1] + i32::from(dz)]).collect()
    }

    /// The tile `cell` is in, and its place in it.
    fn made(&mut self, cell: [i32; 2]) -> (Arc<TileMade>, usize) {
        let tile = [cell[0].div_euclid(TILE), cell[1].div_euclid(TILE)];
        let local = (cell[1].rem_euclid(TILE) * TILE + cell[0].rem_euclid(TILE)) as usize;
        if let Some((_, made)) = self.tiles.iter().find(|(known, _)| *known == tile) {
            return (made.clone(), local);
        }
        let made = self.finder.tile(tile);
        if self.tiles.len() >= 8 {
            self.tiles.remove(0);
        }
        self.tiles.push((tile, made.clone()));
        (made, local)
    }
}

/// What a river makes of a place near it: how far past its edge the place
/// is (less than nothing in it), its water's height there, how high its
/// banks are kept over, its bed, and how many metres of a waterfall pour
/// down there.
#[derive(Clone, Copy, Debug)]
struct Reached {
    past: f32,
    water: f32,
    level: f32,
    bed: f32,
    falls: f32,
}

/// The ground, water, falls and banks a place gets.
struct Made {
    height: f32,
    water: f32,
    falls: f32,
    banks: f32,
}

impl Finder {
    /// What the rivers and lakes near `(x, z)` make of ground `ground` high
    /// there, where water already standing on the ground stands `standing`
    /// high.
    fn at(&self, near: &mut Near, x: f32, z: f32, ground: f32, standing: f32) -> Made {
        let mut made = Made { height: ground, water: NO_WATER, falls: 0.0, banks: 0.0 };
        // Under water standing on the ground, as a great river's, its ground is left as it is.
        if standing > ground {
            return made;
        }
        // Read where the world is bent, so rivers wind between their points.
        let bend = self.meander * self.spacing * 0.45;
        let wide = self.spacing * 1.7;
        let (wx, wz) = (
            x + bend * gradient(x / wide, z / wide, self.seed.wrapping_add(11)),
            z + bend * gradient(x / wide, z / wide, self.seed.wrapping_add(12)),
        );
        let cell = [(wx / self.spacing).floor() as i32, (wz / self.spacing).floor() as i32];
        if !near.point(cell).near {
            return made;
        }
        // How much the river has cut into its banks here: in some stretches, as on the outside of a bend; elsewhere
        // they shelve.
        let (bx, bz) = (wx / BANK_STRETCH, wz / BANK_STRETCH);
        let cut = smooth((gradient(bx, bz, self.seed.wrapping_add(13)) + 0.15) / 0.5);
        let mut nearest: Option<Reached> = None;
        let mut lakes: Vec<Point> = Vec::new();
        let mut pools: Vec<Point> = Vec::new();
        // The points round whose rivers, pools and lakes may reach the place: where rivers are carried on, those its
        // cell keeps.
        let round: Vec<[i32; 2]> = match self.run > 0 {
            true => near.reaching(cell),
            false => (-self.looked..=self.looked)
                .flat_map(|dz| (-self.looked..=self.looked).map(move |dx| [cell[0] + dx, cell[1] + dz]))
                .collect(),
        };
        {
            for from in round {
                let point = near.point(from);
                if point.lake {
                    lakes.push(point);
                }
                if point.pool > 0.0 {
                    pools.push(point);
                }
                let Some([sx, sz]) = point.down.filter(|_| point.river) else { continue };
                let next = [from[0] + i32::from(sx), from[1] + i32::from(sz)];
                let to = near.point(next);
                let found = match self.run > 0 {
                    // Where rivers are carried on, over points far apart, each turn is drawn as a curve.
                    true => {
                        let after = to
                            .down
                            .filter(|_| to.river && !to.lake)
                            .map(|[ax, az]| near.point([next[0] + i32::from(ax), next[1] + i32::from(az)]));
                        self.curved(&point, &to, after.as_ref(), [wx, wz], cut)
                    }
                    false => self.stretch(&point, &to, [wx, wz], cut, 0.0, f32::MAX),
                };
                if let Some(river) = found {
                    if nearest.is_none_or(|nearest| river.past < nearest.past) {
                        nearest = Some(river);
                    }
                }
            }
        }
        // Within a lake, nothing is raised: its water stands over its bed, rivers running into it and out.
        let reach = LAKE_REACH * self.spacing;
        let lake_over = |lake: &Point| {
            let (ox, oz) = (wx - lake.at[0], wz - lake.at[1]);
            ox * ox + oz * oz < reach * reach && ground < lake.level
        };
        let in_lake = lakes.iter().any(lake_over);
        // Under the sea its floor is left as it is, but for the shallows a river's mouth cuts through to meet it: a
        // river ends at the shore, and nothing is raised in the sea.
        if let Some(river) = nearest.filter(|_| ground >= self.sea - MOUTH_FLOOR) {
            // Its banks fall away as its water comes down to the sea at its mouth.
            let lip = river.level + LIP * smooth((river.level - self.sea) / MOUTH);
            let valley = self.valley.max(1.0);
            if river.past < 0.0 {
                made.height = ground.min(river.bed);
                (made.water, made.falls, made.banks) = (river.water, river.falls, 1.0);
            } else if ground > lip {
                // The bank rises from just over the water up the valley's side: at once where the river has cut into
                // it, over a short steep slope; elsewhere along a low shelving strip, the valley's side rising
                // beyond it. Where it is cut deep, its bank reaches back further, so it slopes no steeper than ground
                // grown over.
                let reach = (valley + (valley * CUT - valley) * cut).max((ground - lip) * BANK_RUN).min(valley);
                let t = (river.past / reach).min(1.0);
                let rise = smooth(t) + (t * (2.0 - t) - smooth(t)) * cut;
                made.height = lip + (ground - lip) * rise;
                made.banks = 1.0 - smooth(river.past / (valley * 0.5));
            } else if ground >= self.sea && !in_lake {
                // Where the ground is lower than the water, a low rim holds it in, sloping down to the ground.
                made.height = lip + (ground - lip) * smooth((river.past - RIM) / LEVEE);
                made.banks = 1.0 - smooth(river.past / (valley * 0.5));
            }
        }
        // A spring's pool at a river's head, and a pond where one ends: dug round its point, its water as high as
        // the river's there.
        for pool in pools.into_iter().filter(|_| ground >= self.sea) {
            let (ox, oz) = (wx - pool.at[0], wz - pool.at[1]);
            let off = (ox * ox + oz * oz).sqrt();
            let from = off / pool.pool;
            if from < 1.0 {
                made.height = made.height.min(pool.level - POOL_DEPTH * (1.0 - from * from));
                made.water = made.water.max(pool.level);
                made.banks = 1.0;
            } else if made.water == NO_WATER && !in_lake && off < pool.pool + POOL_SIDE.max(RIM + LEVEE) {
                // A rim where the ground falls away from it, so its water never stands over the air beside it, and
                // sides sloping up to the ground where it rises.
                let lip = pool.level + LIP;
                if made.height < lip {
                    made.height = lip + (made.height - lip) * smooth((off - pool.pool - RIM) / LEVEE);
                } else {
                    made.height = lip + (made.height - lip) * smooth((off - pool.pool) / POOL_SIDE);
                }
            }
        }
        for lake in lakes {
            let (ox, oz) = (wx - lake.at[0], wz - lake.at[1]);
            let from = (ox * ox + oz * oz).sqrt();
            let reach = LAKE_REACH * self.spacing;
            if from < reach && made.height < lake.level {
                made.water = made.water.max(lake.level);
            } else if from < reach + RIM + LEVEE && made.water == NO_WATER && ground >= self.sea {
                // A rim round it where the ground would let it out, but where a river runs in, sloping down to the
                // ground.
                let lip = lake.level + LIP;
                made.height = made.height.max(lip + (made.height - lip) * smooth((from - reach - RIM) / LEVEE));
            }
        }
        made
    }

    /// What the river from `from` down to `to` and on to `after`, where
    /// it goes on, makes of place `at`, its course curving round `to` from
    /// halfway along the stretch before to halfway along the next; and
    /// straight from `from` where it is a river's head.
    fn curved(&self, from: &Point, to: &Point, after: Option<&Point>, at: [f32; 2], cut: f32) -> Option<Reached> {
        let head = if from.fed { None } else { self.stretch(from, to, at, cut, 0.0, 0.5) };
        let rest = match after {
            Some(after) => self.bend(from, to, after, at, cut),
            None => self.stretch(from, to, at, cut, 0.5, f32::MAX),
        };
        match (head, rest) {
            (Some(head), Some(rest)) => Some(if head.past < rest.past { head } else { rest }),
            (head, rest) => head.or(rest),
        }
    }

    /// What the river turning at `to`, from `from` and on to `after`,
    /// makes of place `at`: its course a curve from halfway along the
    /// stretch before to halfway along the next, drawn towards `to`.
    fn bend(&self, from: &Point, to: &Point, after: &Point, at: [f32; 2], cut: f32) -> Option<Reached> {
        let start = [(from.at[0] + to.at[0]) * 0.5, (from.at[1] + to.at[1]) * 0.5];
        let end = [(to.at[0] + after.at[0]) * 0.5, (to.at[1] + after.at[1]) * 0.5];
        let curve = |u: f32| {
            let (a, b, c) = ((1.0 - u) * (1.0 - u), 2.0 * u * (1.0 - u), u * u);
            [a * start[0] + b * to.at[0] + c * end[0], a * start[1] + b * to.at[1] + c * end[1]]
        };
        // The nearest place on the curve, found along a few straight pieces of it.
        let mut nearest = (f32::MAX, 0.0);
        let mut last = start;
        for piece in 1..=BEND_PIECES {
            let u1 = piece as f32 / BEND_PIECES as f32;
            let next = curve(u1);
            let (ax, az) = (next[0] - last[0], next[1] - last[1]);
            let (px, pz) = (at[0] - last[0], at[1] - last[1]);
            let on = ((px * ax + pz * az) / (ax * ax + az * az).max(1e-6)).clamp(0.0, 1.0);
            let (cx, cz) = (px - ax * on, pz - az * on);
            let off = cx * cx + cz * cz;
            if off < nearest.0 {
                nearest = (off, (piece as f32 - 1.0 + on) / BEND_PIECES as f32);
            }
            last = next;
        }
        let (off, u) = (nearest.0.sqrt(), nearest.1);
        match u < 0.5 {
            true => self.reached(from, to, 0.5 + u, off, cut),
            false => self.reached(to, after, u - 0.5, off, cut),
        }
    }

    /// What the stretch of river from `from` down to `to` makes of place
    /// `at`, where it is near enough to change it, its banks as cut there as
    /// `cut` says, from 0 for shelving to 1: the part of it from `first` to
    /// `last` of the way along.
    fn stretch(&self, from: &Point, to: &Point, at: [f32; 2], cut: f32, first: f32, last: f32) -> Option<Reached> {
        let (ax, az) = (to.at[0] - from.at[0], to.at[1] - from.at[1]);
        let long = (ax * ax + az * az).sqrt().max(1e-3);
        let (px, pz) = (at[0] - from.at[0], at[1] - from.at[1]);
        // A river running into the sea runs on straight past its point there, whose place may be on the beach,
        // until it meets the sea: what is under the sea is left as it is.
        let reach = if self.into_sea(to) { MOUTH_REACH } else { 1.0 };
        let on = ((px * ax + pz * az) / (long * long)).clamp(first, reach.min(last));
        let (cx, cz) = (px - ax * on, pz - az * on);
        let off = (cx * cx + cz * cz).sqrt();
        self.reached(from, to, on, off, cut)
    }

    /// Whether a river running down to `to` runs into the sea there, or
    /// water standing on the ground.
    fn into_sea(&self, to: &Point) -> bool {
        !to.river && !to.lake && (to.level <= self.sea || to.drowned)
    }

    /// What the stretch of river from `from` down to `to` makes of a place
    /// `on` of the way along it and `off` metres to its side.
    fn reached(&self, from: &Point, to: &Point, on: f32, off: f32, cut: f32) -> Option<Reached> {
        let (ax, az) = (to.at[0] - from.at[0], to.at[1] - from.at[1]);
        let long = (ax * ax + az * az).sqrt().max(1e-3);
        let t = on.clamp(0.0, 1.0);
        // As wide and deep as its upper end, and the lower's where a river goes on from it.
        let (width, depth) = match to.river || to.lake {
            true => (from.width + (to.width - from.width) * t, from.depth + (to.depth - from.depth) * t),
            false => (from.width, from.depth),
        };
        // Its water falls along every stretch: never higher at its lower end than at its upper.
        let (upper, lower) = (from.level, to.level.min(from.level));
        // Wider at its mouth, as its water comes down to the sea, its bed under the sea's.
        let here = upper - (upper - lower) * t;
        let mouth = 1.0 - smooth((here - self.sea) / MOUTH);
        let width = width * (1.0 + MOUTH_WIDENS * mouth);
        let floor = self.sea - MOUTH_FLOOR * mouth;
        let past = off - width * 0.5;
        if past > self.valley.max(RIM + LEVEE) {
            return None;
        }
        let drop = upper - lower;
        let along = t * long;
        let across = (off / (width * 0.5).max(0.01)).min(1.0);
        // A bowl: dropping steeply from just under its water at its edge, rounding into a broad bed along its middle,
        // more steeply still where it has cut into its banks.
        let across2 = across * across;
        let edge = across2 + (across2 * across2 - across2) * cut;
        let bowl = (1.0 - edge).max(0.0).sqrt();
        let bed = |level: f32, deeper: f32| {
            let bed = level - SHELF - (depth + deeper - SHELF).max(0.0) * bowl;
            if mouth > 0.0 { bed.min(floor) } else { bed }
        };
        if drop <= self.falls {
            let level = upper - drop * t;
            return Some(Reached { past, water: level, level, bed: bed(level, 0.0), falls: 0.0 });
        }
        // A waterfall halfway: nearly level to its lip, over it into a pool, nearly level on from there.
        let lip = long * 0.5;
        let (top, foot) = (upper - drop * RUN_DOWN, lower + drop * RUN_DOWN);
        if along < lip {
            let level = upper - (upper - top) * (along / lip);
            return Some(Reached { past, water: level, level, bed: bed(level, 0.0), falls: 0.0 });
        }
        let level = foot - (foot - lower) * ((along - lip) / (long - lip).max(1e-3));
        let pool = depth * 0.8 * (1.0 - (along - lip) / POOL).max(0.0);
        let curtain = along < lip + CURTAIN;
        let (water, falls) = if curtain { (top, top - level) } else { (level, 0.0) };
        Some(Reached { past, water, level, bed: bed(level, pool), falls })
    }
}

/// A Rivers node's ground, water, falls or banks at each place, by `what`
/// (its output's number), from rows of x, z and the ground wired in, and
/// the water standing on it where that is wired in too.
#[derive(Debug)]
struct Ran {
    finder: Arc<Finder>,
    what: u8,
}

impl Kernel for Ran {
    fn gpu(&self) -> Option<GpuStep> {
        // The rivers are found over a grid of points round each place, on the CPU alone: on the GPU the ground
        // runs on as it was, with no water.
        let step = match usize::from(self.what) {
            Rivers::HEIGHT => GpuStep::new(GpuOp::Copy).whole(2),
            Rivers::WATER => GpuStep::new(GpuOp::Constant).number(NO_WATER),
            _ => GpuStep::new(GpuOp::Constant).number(0.0),
        };
        Some(step.leaving_out("rivers"))
    }

    fn key(&self, key: &mut Key) {
        key.name("rivers ran").whole(self.finder.id as u32).whole((self.finder.id >> 32) as u32);
        key.whole(u32::from(self.what));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        // A Rivers node's outputs each ask the same places in turn: they are worked out once, for all of them. Not
        // while the places are worked out, which may run another Rivers node's on this thread for the ground.
        let what = usize::from(self.what).min(Rivers::BANKS);
        let same = |made: &RanMade| {
            made.id == self.finder.id
                && made.rows.len() == inputs.len()
                && made.rows.iter().zip(inputs).all(|(kept, row)| kept.as_slice() == *row)
        };
        let found = MADE.with_borrow(|kept| {
            let made = kept.iter().find(|made| same(made))?;
            for (out, made) in out.iter_mut().zip(&made.made) {
                *out = made[what];
            }
            Some(())
        });
        if found.is_some() {
            return;
        }
        let mut near = Near { finder: &self.finder, tiles: Vec::new() };
        let standing = inputs.get(3).copied().unwrap_or_default();
        let made: Vec<[f32; 4]> = (0..out.len())
            .map(|at| {
                let standing = standing.get(at).copied().unwrap_or(NO_WATER);
                let made = self.finder.at(&mut near, inputs[0][at], inputs[1][at], inputs[2][at], standing);
                [made.height, made.water, made.falls, made.banks]
            })
            .collect();
        for (out, made) in out.iter_mut().zip(&made) {
            *out = made[what];
        }
        // Kept for each of the last few nodes, as a great river's node and the one after it ask in turn.
        let rows = inputs.iter().map(|row| row.to_vec()).collect();
        MADE.with_borrow_mut(|kept| {
            if kept.len() >= MADE_KEPT {
                kept.remove(0);
            }
            kept.push(RanMade { id: self.finder.id, rows, made });
        });
    }
}

/// The places a Rivers node last worked out on this thread, by its
/// finder, and its height, water, falls and banks at each.
#[derive(Default)]
struct RanMade {
    id: u64,
    rows: Vec<Vec<f32>>,
    made: Vec<[f32; 4]>,
}

const MADE_KEPT: usize = 4;

thread_local! {
    static MADE: std::cell::RefCell<Vec<RanMade>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, WorldGen, nodes::Output, nodes::testing, starters};

    /// Starter `name`'s ground with rivers run down it, their water and
    /// falls wired into the Output.
    pub(crate) fn rivered(name: &str, set: Rivers) -> WorldGen {
        let starter = starters().into_iter().find(|starter| starter.name == name).expect("a starter");
        let mut graph = starter.graph;
        let output = graph.output().expect("an output").id;
        let link = *graph.link_into(output, Output::HEIGHT).expect("its ground");
        let rivers = graph.add(NodeKind::Rivers(set), [0.0, 0.0]);
        assert!(graph.connect(link.from, link.output, rivers, Rivers::GROUND));
        assert!(graph.connect(rivers, Rivers::HEIGHT, output, Output::HEIGHT));
        assert!(graph.connect(rivers, Rivers::WATER, output, Output::WATER));
        assert!(graph.connect(rivers, Rivers::FALLS, output, Output::FALLS));
        WorldGen::new(7, 1.0, starter.sea_level, graph)
    }

    /// Places `step` metres apart over a square `side` places a side from
    /// `(x, z)`.
    fn grid(x: f32, z: f32, step: f32, side: usize) -> (Vec<f32>, Vec<f32>) {
        (0..side * side).map(|at| (x + (at % side) as f32 * step, z + (at / side) as f32 * step)).unzip()
    }

    fn wet(water: f32) -> bool {
        water > NO_WATER * 0.5
    }

    /// Starter `name`'s world and the same world with its rivers taken out: the ground its Rivers node is given.
    fn with_and_without(name: &str) -> (WorldGen, WorldGen) {
        let starter = starters().into_iter().find(|starter| starter.name == name).expect("a starter");
        let mut graph = starter.graph.clone();
        let output = graph.output().expect("an output").id;
        let rivers = graph.link_into(output, Output::HEIGHT).expect("its ground").from;
        let ground = *graph.link_into(rivers, Rivers::GROUND).expect("its rivers' ground");
        assert!(graph.connect(ground.from, ground.output, output, Output::HEIGHT));
        (starter.world(7, 1.0), WorldGen::new(7, 1.0, starter.sea_level, graph))
    }

    #[test]
    fn a_world_without_its_rivers_is_the_ground_its_rivers_are_given() {
        for name in ["Rolling Hills", "Alpine"] {
            let (world, plain) = with_and_without(name);
            let bare = world.without_rivers();
            assert!(world.has_rivers() && !bare.has_rivers(), "{name}");
            let (xs, zs) = grid(-400.0, -400.0, 16.0, 50);
            let (mut a, mut b, mut c) = (vec![0.0; xs.len()], vec![0.0; xs.len()], vec![0.0; xs.len()]);
            bare.heights(&xs, &zs, &mut a);
            plain.heights(&xs, &zs, &mut b);
            world.heights(&xs, &zs, &mut c);
            assert_eq!(a, b, "{name}: its ground as its Rivers node is given it");
            assert!(a.iter().zip(&c).any(|(a, c)| a != c), "{name}: its rivers cut somewhere");
            assert!(bare.water(&xs, &zs).is_none_or(|(water, _)| !water.iter().any(|&w| wet(w))), "{name}: no water");
        }
    }

    #[test]
    fn a_river_meets_the_sea_through_its_mouth_and_raises_nothing_out_in_the_sea() {
        let (world, plain) = with_and_without("Archipelago");
        let sea = world.sea_level();
        let (xs, zs) = grid(-1600.0, -1600.0, 8.0, 400);
        let (mut heights, mut ground) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
        world.heights(&xs, &zs, &mut heights);
        plain.heights(&xs, &zs, &mut ground);
        let (water, _) = world.water(&xs, &zs).unwrap();
        // Under the sea, nothing is higher than it was: no banks or walls run out over its floor.
        let raised = (0..xs.len()).filter(|&at| ground[at] < sea && heights[at] > ground[at] + 0.01).count();
        assert_eq!(raised, 0, "places under the sea raised");
        // And rivers reach it: water of a river as low as the sea, beside the sea's own.
        let open = |at: usize| {
            let (x, z) = ((at % 400) as i32, (at / 400) as i32);
            (-2..=2).any(|dz: i32| {
                (-2..=2).any(|dx: i32| {
                    let (bx, bz) = (x + dx, z + dz);
                    (0..400).contains(&bx) && (0..400).contains(&bz) && ground[(bz * 400 + bx) as usize] < sea - 2.0
                })
            })
        };
        let mouths = (0..xs.len()).filter(|&at| wet(water[at]) && water[at] < sea + 1.0 && open(at)).count();
        assert!(mouths > 3, "{mouths} places where a river meets the sea");
    }

    /// How much of the ground within `reach` metres of the middle of the water standing highest (`highest`) or
    /// lowest within `near` metres of `(x, z)` is under water, a metre apart: a pool's or a pond's all of it, a
    /// river's narrow end little.
    fn pooled(world: &WorldGen, (x, z): (f32, f32), near: f32, reach: f32, highest: bool) -> f32 {
        let side = (near * 2.0) as usize + 1;
        let (xs, zs) = grid(x - near, z - near, 1.0, side);
        let (water, _) = world.water(&xs, &zs).unwrap();
        let wet_at: Vec<usize> = (0..xs.len()).filter(|&at| wet(water[at])).collect();
        let level = match highest {
            true => wet_at.iter().map(|&at| water[at]).fold(f32::MIN, f32::max),
            false => wet_at.iter().map(|&at| water[at]).fold(f32::MAX, f32::min),
        };
        let flat: Vec<usize> = wet_at.into_iter().filter(|&at| (water[at] - level).abs() < 0.02).collect();
        let middle = flat.iter().fold([0.0, 0.0], |sum, &at| [sum[0] + xs[at], sum[1] + zs[at]]);
        let middle = middle.map(|part| part / flat.len() as f32);
        let side = (reach * 2.0) as usize + 1;
        let (xs, zs) = grid(middle[0] - reach, middle[1] - reach, 1.0, side);
        let (water, _) = world.water(&xs, &zs).unwrap();
        let round: Vec<usize> = (0..xs.len())
            .filter(|&at| {
                (xs[at] - xs[side * side / 2]).powi(2) + (zs[at] - zs[side * side / 2]).powi(2) <= reach * reach
            })
            .collect();
        round.iter().filter(|&&at| wet(water[at])).count() as f32 / round.len() as f32
    }

    #[test]
    fn every_river_rises_from_a_spring_and_one_ending_inland_ends_in_a_pond() {
        let (world, _) = with_and_without("Rolling Hills");
        let sea = world.sea_level();
        let (xs, zs) = grid(-1600.0, -1600.0, 8.0, 400);
        let (water, _) = world.water(&xs, &zs).unwrap();
        let river = |at: usize| wet(water[at]) && water[at] > sea + 3.0;
        let inside = |at: usize| (30..370).contains(&(at % 400)) && (30..370).contains(&(at / 400));
        let near = |at: usize, reach: i32| {
            let (x, z) = ((at % 400) as i32, (at / 400) as i32);
            (-reach..=reach).flat_map(move |dz| (-reach..=reach).map(move |dx| ((z + dz) * 400 + x + dx) as usize))
        };
        let lone = |at: usize| near(at, 3).filter(|&k| river(k)).count() < 20;
        // A river's head: its water the highest within 200 m, with no lake round it. Its pool fills the ground round
        // the highest of it, where a river's narrow end would fill little.
        let heads: Vec<usize> = (0..xs.len())
            .filter(|&at| inside(at) && river(at) && lone(at))
            .filter(|&at| near(at, 25).all(|k| !river(k) || water[k] <= water[at] + 0.01))
            .collect();
        assert!(heads.len() >= 3, "{} heads", heads.len());
        for &at in heads.iter().take(8) {
            let pooled = pooled(&world, (xs[at], zs[at]), 12.0, 4.0, true);
            assert!(pooled > 0.6, "the head at ({}, {}) is {pooled} under water", xs[at], zs[at]);
        }
        // Where a river ends inland, its water the lowest within 200 m, it ends in a pond.
        let ends: Vec<usize> = (0..xs.len())
            .filter(|&at| inside(at) && river(at) && lone(at))
            .filter(|&at| near(at, 25).all(|k| !river(k) || water[k] >= water[at] - 0.01))
            .collect();
        for &at in ends.iter().take(8) {
            let pooled = pooled(&world, (xs[at], zs[at]), 12.0, 4.0, false);
            assert!(pooled > 0.6, "the end at ({}, {}) is {pooled} under water", xs[at], zs[at]);
        }
    }

    #[test]
    fn rivers_run_down_valleys_between_banks_that_hold_their_water_in() {
        let world = rivered("Rolling Hills", Rivers::default());
        testing::checked(&world);
        let side = 400;
        let (xs, zs) = grid(-800.0, -800.0, 5.0, side);
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let (water, _) = world.water(&xs, &zs).expect("water wired in");
        let wet_places: Vec<usize> = (0..xs.len()).filter(|&at| wet(water[at])).collect();
        let share = wet_places.len() as f32 / xs.len() as f32;
        assert!(share > 0.003 && share < 0.2, "{share} of the land under rivers and lakes");
        // The ground under water lies under it.
        assert!(wet_places.iter().all(|&at| heights[at] <= water[at] + 1e-3));
        // A metre either way from water, the ground is as high as the water or there is water too: no water
        // stands over lower dry ground beside it, as a wall of it would.
        let (mut asked, mut walls) = (0, 0);
        for &at in wet_places.iter().step_by(3) {
            let xs = [xs[at] - 1.0, xs[at] + 1.0, xs[at], xs[at]];
            let zs = [zs[at], zs[at], zs[at] - 1.0, zs[at] + 1.0];
            let mut beside = [0.0; 4];
            world.heights(&xs, &zs, &mut beside);
            let (beside_water, _) = world.water(&xs, &zs).unwrap();
            for k in 0..4 {
                asked += 1;
                if !wet(beside_water[k]) && beside[k] < water[at] - 0.3 {
                    walls += 1;
                }
            }
        }
        assert!(walls * 1000 <= asked, "{walls} of {asked} places beside water lower than it and dry");
        // Rivers run downhill: a few metres on the way the water flows, it stands no higher.
        let mut uphill = 0;
        let mut flowing = 0;
        for &at in wet_places.iter().step_by(2) {
            let Some([fx, fz]) = world.current(xs[at], zs[at]) else { continue };
            let speed = (fx * fx + fz * fz).sqrt();
            if speed < 1e-3 {
                continue;
            }
            flowing += 1;
            let on = [xs[at] + fx / speed * 3.0, zs[at] + fz / speed * 3.0];
            let (there, _) = world.water(&[on[0]], &[on[1]]).unwrap();
            if wet(there[0]) && there[0] > water[at] + 0.05 {
                uphill += 1;
            }
        }
        assert!(flowing > 100, "{flowing} places where the water flows");
        assert!(uphill * 50 <= flowing, "{uphill} of {flowing} run uphill");
    }

    #[test]
    fn water_sits_in_the_ground_and_raises_no_rim_round_it() {
        let (world, plain) = with_and_without("Rolling Hills");
        let sea = world.sea_level();
        let (side, step) = (400, 2.0);
        let (xs, zs) = grid(-400.0, -400.0, step, side);
        let (mut heights, mut ground) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
        world.heights(&xs, &zs, &mut heights);
        plain.heights(&xs, &zs, &mut ground);
        let (water, _) = world.water(&xs, &zs).unwrap();
        // The dry places within 12 m of water over the sea: none stands much over the ground as it lay.
        let reach = (12.0 / step) as i32;
        let by_water = |at: usize| {
            let (x, z) = ((at % side) as i32, (at / side) as i32);
            (-reach..=reach).any(|dz| {
                (-reach..=reach).any(|dx| {
                    let (bx, bz) = (x + dx, z + dz);
                    let there = (bz * side as i32 + bx) as usize;
                    (0..side as i32).contains(&bx) && (0..side as i32).contains(&bz) && wet(water[there])
                })
            })
        };
        let dry: Vec<usize> = (0..xs.len()).filter(|&at| !wet(water[at]) && ground[at] > sea && by_water(at)).collect();
        let raised = dry.iter().filter(|&&at| heights[at] > ground[at] + 0.5).count();
        assert!(dry.len() > 2000, "{} dry places by water", dry.len());
        assert!(raised * 100 <= dry.len(), "{raised} of {} dry places by water raised over the ground", dry.len());
    }

    /// Starter `name`'s ground with great rivers run down it before its own rivers: a network over points 256 m
    /// apart, each river carried on 12 km.
    fn with_great_rivers(name: &str) -> WorldGen {
        let (_, plain) = with_and_without(name);
        let mut graph = plain.graph().clone();
        let great = Rivers {
            spacing: 256.0,
            source: 300.0,
            width: 26.0,
            depth: 4.0,
            valley: 160.0,
            falls: 8.0,
            lakes: 0.0,
            run_on: 12_000.0,
            seed: 1,
            ..Rivers::default()
        };
        assert!(add_great_rivers(&mut graph, great, Rivers::default()));
        WorldGen::new(7, 1.0, plain.sea_level(), graph)
    }

    /// How many pieces of river and lake water, joined side to side, over `least` square metres, meet the sea, in
    /// a square of places `step` metres apart, `side` a side.
    fn meeting_the_sea(heights: &[f32], water: &[f32], sea: f32, side: usize, step: f32, least: f32) -> usize {
        // A river's mouth cuts under the sea's level: water over ground anywhere is a river's or a lake's.
        let river = |at: usize| wet(water[at]) && water[at] > heights[at];
        let salt = |at: usize| heights[at] < sea && !river(at);
        let mut seen = vec![false; heights.len()];
        let mut meeting = 0;
        for start in 0..heights.len() {
            if seen[start] || !river(start) {
                continue;
            }
            seen[start] = true;
            let (mut stack, mut size, mut meets) = (vec![start], 0usize, false);
            while let Some(at) = stack.pop() {
                size += 1;
                let (x, z) = (at % side, at / side);
                let round = [(x > 0, at.wrapping_sub(1)), (x + 1 < side, at + 1)];
                let round = round.into_iter().chain([(z > 0, at.wrapping_sub(side)), (z + 1 < side, at + side)]);
                for (inside, there) in round {
                    if !inside {
                        continue;
                    }
                    meets |= salt(there);
                    if !seen[there] && river(there) {
                        seen[there] = true;
                        stack.push(there);
                    }
                }
            }
            if meets && size as f32 * step * step > least {
                meeting += 1;
            }
        }
        meeting
    }

    #[test]
    fn great_rivers_are_carried_on_to_the_sea_and_the_rest_run_into_them_without_walls() {
        let world = with_great_rivers("Alpine");
        testing::checked(&world);
        let sea = world.sea_level();
        let (side, step) = (500, 16.0);
        let (xs, zs) = grid(-4000.0, -4000.0, step, side);
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let (water, falls) = world.water(&xs, &zs).unwrap();
        // More rivers over a few hectares meet the sea than where there are no great rivers.
        let (own, _) = with_and_without("Alpine");
        let mut own_heights = vec![0.0; xs.len()];
        own.heights(&xs, &zs, &mut own_heights);
        let (own_water, _) = own.water(&xs, &zs).unwrap();
        let great = meeting_the_sea(&heights, &water, sea, side, step, 30_000.0);
        let before = meeting_the_sea(&own_heights, &own_water, sea, side, step, 30_000.0);
        assert!(great > before && great >= 2, "{great} rivers over 3 ha meet the sea, against {before}");
        // Some of it is broad: 60 m across.
        let broad = (0..xs.len()).filter(|&at| {
            let wide = |dx: f32, dz: f32| {
                let (wx, wz) = (vec![xs[at] - dx, xs[at] + dx], vec![zs[at] - dz, zs[at] + dz]);
                world.water(&wx, &wz).unwrap().0.iter().all(|&w| wet(w) && w > sea + 0.5)
            };
            at % 7 == 0 && wet(water[at]) && water[at] > sea + 0.5 && wide(30.0, 0.0) && wide(0.0, 30.0)
        });
        assert!(broad.count() > 0, "no river 60 m across");
        // A metre either way from water, the ground is as high as the water or there is water too, the sea's where it
        // is under the sea, where the rivers run into the great ones as much as anywhere. A waterfall's curtain
        // stands over its pool.
        let (mut asked, mut walls) = (0, 0);
        let curtain = |falls: f32| falls > 0.0;
        let watered = |at: usize| wet(water[at]) && heights[at] < water[at] && !curtain(falls[at]);
        for at in (0..xs.len()).filter(|&at| watered(at)).step_by(3) {
            let near_xs = [xs[at] - 1.0, xs[at] + 1.0, xs[at], xs[at]];
            let near_zs = [zs[at], zs[at], zs[at] - 1.0, zs[at] + 1.0];
            let mut beside = [0.0; 4];
            world.heights(&near_xs, &near_zs, &mut beside);
            let (beside_water, beside_falls) = world.water(&near_xs, &near_zs).unwrap();
            for k in 0..4 {
                asked += 1;
                let filled = wet(beside_water[k]) || beside[k] < sea || curtain(beside_falls[k]);
                if !filled && beside[k] < water[at] - 0.3 {
                    walls += 1;
                }
            }
        }
        assert!(asked > 1000, "{asked} places beside water asked");
        assert!(walls * 1000 <= asked, "{walls} of {asked} places beside water lower than it and dry");
    }

    #[test]
    fn a_river_dropping_down_a_mountainside_pours_over_waterfalls_into_pools() {
        let world = rivered("Alpine", Rivers::default());
        let (step, side) = (6.0, 400);
        let (xs, zs) = grid(-1200.0, -1200.0, step, side);
        let (water, _) = world.water(&xs, &zs).unwrap();
        // Where the water a few metres on stands much lower, a waterfall is between: looked for a metre at a time.
        let mut falls = Vec::new();
        for at in (0..xs.len()).filter(|&at| wet(water[at]) && at % side + 1 < side) {
            for next in [at + 1, at + side] {
                if next >= xs.len() || !wet(water[next]) || (water[at] - water[next]).abs() < 4.5 {
                    continue;
                }
                let line: (Vec<f32>, Vec<f32>) = (0..=24)
                    .map(|k| k as f32 / 24.0)
                    .map(|t| (xs[at] + (xs[next] - xs[at]) * t, zs[at] + (zs[next] - zs[at]) * t))
                    .unzip();
                let (high, fall) = world.water(&line.0, &line.1).unwrap();
                let mut ground = vec![0.0; line.0.len()];
                world.heights(&line.0, &line.1, &mut ground);
                falls.extend((0..line.0.len()).filter(|&k| fall[k] > 0.0).map(|k| (high[k], fall[k], ground[k])));
            }
        }
        assert!(!falls.is_empty(), "no waterfall");
        for &(top, fall, ground) in &falls {
            assert!(fall > 4.0, "a waterfall {fall} m high");
            // Its curtain pours into a pool: the pool's water stands over the ground under it.
            assert!(top - fall > ground, "a curtain from {top} falling {fall} onto ground at {ground}");
        }
    }
}
