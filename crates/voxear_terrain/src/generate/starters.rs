//! Graphs to start a world generator from: five kinds of ground, each
//! built from the node library and tuned by eye, with a few of its values
//! shown as rows of the generator's own (its height first), so it is a
//! preset and a few numbers until its graph is opened.
//!
//! They carry the names of a landmass's presets and make new ground under
//! them: a file made from a landmass preset is a Landmass node still, and
//! makes the ground it made.
//!
//! After the five comes one that is no kind of ground: a Cave Level, solid
//! rock with one bounded cave of rooms in it, for worlds of blocks.

use voxear_core::curve::Curve;

use super::{
    caves::{Room, RoomRules},
    graph::{Graph, NodeId},
    landmass::Climate,
    nodes::{
        Arches, Bend, Blend, CaveNetwork, Cellular, Clamp, Combine, Combining, Constant, Gullies, HeightMask, Laid,
        Math, NodeKind, Noise, Operation, Output, Overhangs, Plateaus, RaisedLand, Remap, STEEPEST, SlopeMask, Solid,
        Terrace,
    },
    world::WorldGen,
};
use crate::noise::NoiseStyle;

/// A graph to start a generator from: its name, the sea level that suits
/// it, and its graph.
#[derive(Clone, Debug, PartialEq)]
pub struct Starter {
    pub name: &'static str,
    pub sea_level: f32,
    pub graph: Graph,
}

impl Starter {
    /// The generator it starts, with this seed and scale.
    pub fn world(&self, seed: u32, scale: f32) -> WorldGen {
        WorldGen::new(seed, scale, self.sea_level, self.graph.clone())
    }

    /// `world` with this starter's ground in place of its own: its seed,
    /// its scale and the area its previews show kept, and the nodes that
    /// make its Volume carried over with their rows (see
    /// [`Graph::take_volume`]). A starter with a Volume of its own keeps
    /// that instead: the cave level is its cave.
    pub fn reshapes(&self, world: &WorldGen) -> WorldGen {
        let mut graph = self.graph.clone();
        if graph.volume_nodes().is_empty() {
            graph.take_volume(world.graph());
        }
        WorldGen::new(world.seed(), world.scale(), self.sea_level, graph).with_preview_area(world.preview_area())
    }

    /// Whether `graph` is this starter's still: the same nodes and wires,
    /// whatever its rows are set to and wherever its nodes lie. The nodes
    /// that make its Volume alone are not looked at: a starter is the
    /// ground, and stays that starter with overhangs and caves wired in.
    pub fn shapes(&self, graph: &Graph) -> bool {
        let graph = &graph.without_volume();
        let mut own = self.graph.without_volume();
        if graph.exposed != own.exposed {
            return false;
        }
        // Its rows set as the graph's are, and both with their nodes in the order of their numbers, nowhere.
        for row in 0..own.exposed.len() {
            let Some(value) = graph.exposed_value(row) else { return false };
            own.set_exposed(row, value);
        }
        let laid = |graph: &Graph| {
            let mut nodes: Vec<_> = graph.nodes.iter().map(|node| (node.id, &node.kind)).collect();
            nodes.sort_by_key(|(id, _)| *id);
            let mut links = graph.links.clone();
            links.sort_by_key(|link| (link.to, link.input));
            (nodes.into_iter().map(|(id, kind)| (id, kind.clone())).collect::<Vec<_>>(), links)
        };
        laid(&own) == laid(graph)
    }
}

/// The graphs a generator can start from, in the order a list shows them.
pub fn starters() -> Vec<Starter> {
    vec![
        battlefield_plains(),
        rolling_hills(),
        alpine(),
        archipelago(),
        canyons(),
        caves(),
        mesas(),
        sky_islands(),
        highlands(),
        cave_level(),
    ]
}

impl WorldGen {
    /// The name of the starter its graph is still (see
    /// [`Starter::shapes`]); none once its graph is its own, or for a
    /// landmass.
    pub fn starter(&self) -> Option<&'static str> {
        starters().into_iter().find(|starter| starter.shapes(self.graph())).map(|starter| starter.name)
    }
}

/// What feeds one of a node's inputs: another node's first output, or a
/// number of the node's own.
#[derive(Clone, Copy)]
enum In {
    Wire(NodeId),
    Value(f32),
}

impl From<NodeId> for In {
    fn from(node: NodeId) -> Self {
        In::Wire(node)
    }
}

impl From<f32> for In {
    fn from(value: f32) -> Self {
        In::Value(value)
    }
}

/// A graph being laid out: its nodes in columns from left to right, each
/// column filling downwards.
struct Layout {
    graph: Graph,
    column: f32,
    row: f32,
}

impl Layout {
    fn new() -> Self {
        Self { graph: Graph::default(), column: 0.0, row: 0.0 }
    }

    /// Starts the next column.
    fn next(&mut self) {
        (self.column, self.row) = (self.column + 1.0, 0.0);
    }

    fn add(&mut self, kind: NodeKind) -> NodeId {
        let at = [self.column * 280.0, self.row * 190.0];
        self.row += 1.0;
        self.graph.add(kind, at)
    }

    fn wire(&mut self, from: NodeId, output: usize, to: NodeId, input: usize) {
        let wired = self.graph.connect(from, output, to, input);
        debug_assert!(wired, "a starter's wire from {from:?} into {to:?}");
    }

    fn noise(&mut self, style: NoiseStyle, size: f32, octaves: u32, low: f32, high: f32) -> NodeId {
        self.add(NodeKind::Noise(Noise { style, size, octaves, low, high }))
    }

    /// A Math node of `a` and `b`.
    fn math(&mut self, operation: Operation, a: impl Into<In>, b: impl Into<In>) -> NodeId {
        let (a, b) = (a.into(), b.into());
        let value = |input: In| if let In::Value(value) = input { value } else { 0.0 };
        let node = self.add(NodeKind::Math(Math { operation, a: value(a), b: value(b) }));
        for (input, from) in [a, b].into_iter().enumerate() {
            if let In::Wire(from) = from {
                self.wire(from, 0, node, input);
            }
        }
        node
    }

    /// `input` through a curve with these keys, read from `low` to `high`.
    fn curve(&mut self, input: NodeId, low: f32, high: f32, keys: &[[f32; 2]]) -> NodeId {
        let node = self.add(NodeKind::Curve(Remap { curve: Curve::new(keys), low, high }));
        self.wire(input, 0, node, 0);
        node
    }

    /// A node of `kind` with `input` wired into its first input.
    fn after(&mut self, input: NodeId, kind: NodeKind) -> NodeId {
        let node = self.add(kind);
        self.wire(input, 0, node, 0);
        node
    }

    /// A Blend of `a` and `b` by `mask`.
    fn blend(&mut self, a: impl Into<In>, b: impl Into<In>, mask: NodeId) -> NodeId {
        let (a, b) = (a.into(), b.into());
        let value = |input: In, empty: f32| if let In::Value(value) = input { value } else { empty };
        let node = self.add(NodeKind::Blend(Blend { a: value(a, 0.0), b: value(b, 1.0), mask: 0.5 }));
        for (input, from) in [a, b].into_iter().enumerate() {
            if let In::Wire(from) = from {
                self.wire(from, 0, node, input);
            }
        }
        self.wire(mask, 0, node, Blend::MASK);
        node
    }

    fn expose(&mut self, node: NodeId, value: &str, label: &str) {
        let at = self.graph.node(node).and_then(|node| node.kind.values().iter().position(|spec| spec.name == value));
        let shown = at.is_some_and(|at| self.graph.expose(node, at, label));
        debug_assert!(shown, "a starter's row {label}");
    }

    /// Ends the graph: `height` into its Output, with a climate beside it
    /// whose temperature and humidity are rows after those shown so far.
    fn finish(mut self, height: NodeId, climate: Climate) -> Graph {
        self.next();
        let output = self.add(NodeKind::Output(Output::default()));
        let weather = self.add(NodeKind::Climate(climate));
        self.wire(height, 0, output, Output::HEIGHT);
        self.wire(weather, Climate::TEMPERATURE_OUT, output, Output::TEMPERATURE);
        self.wire(weather, Climate::HUMIDITY_OUT, output, Output::HUMIDITY);
        self.expose(weather, "temperature", "Temperature");
        self.expose(weather, "humidity", "Humidity");
        self.graph
    }
}

use NoiseStyle::{Fractal, Ridged};
use Operation::{Add, Multiply};

/// Wide, nearly level farmland rolling gently, with low rises and the
/// shallow dips of streams; and, with Craters above nothing, shell holes
/// where the shelling was heaviest.
fn battlefield_plains() -> Starter {
    let mut g = Layout::new();
    let broad = g.noise(Fractal, 2600.0, 2, 0.0, 1.0);
    let swells = g.noise(Fractal, 700.0, 2, 0.0, 1.0);
    let knolls = g.noise(Fractal, 200.0, 2, 0.0, 1.0);
    g.next();
    let broad = g.math(Multiply, broad, 0.6);
    let swells = g.math(Multiply, swells, 0.32);
    let knolls = g.math(Multiply, knolls, 0.08);
    g.next();
    let land = g.math(Add, broad, swells);
    let land = g.math(Add, land, knolls);
    g.next();
    let land = g.curve(land, 0.2, 0.8, &[[0.0, 0.0], [0.4, 0.2], [0.6, 0.4], [0.85, 0.82], [1.0, 1.0]]);
    let land = g.after(land, NodeKind::Warp(Bend { size: 700.0, strength: 100.0, octaves: 3 }));
    g.next();
    let high = g.math(Multiply, land, 40.0);
    g.expose(high, "b", "Height");
    // Streams cut by how steep the ground is, and this ground is nearly level: so they are cut into it drawn six
    // times as high, and the cut ground brought down again.
    let tall = g.math(Multiply, high, 6.0);
    g.next();
    let cut = g.after(tall, NodeKind::Gullies(Gullies { strength: 0.5, spacing: 150.0, octaves: 3 }));
    g.expose(cut, "strength", "Streams");
    let cut = g.math(Operation::Divide, cut, 6.0);
    let bumps = g.noise(Fractal, 20.0, 3, -0.25, 0.25);
    g.next();
    let ground = g.math(Add, cut, bumps);
    // Shell holes: a bowl with a raised lip round each point of two scatters, small holes and large ones, where a
    // broad field says the shells fell.
    let small = g.add(NodeKind::Cells(Cellular { size: 14.0, jitter: 1.0 }));
    let large = g.add(NodeKind::Cells(Cellular { size: 30.0, jitter: 1.0 }));
    let fell = g.noise(Fractal, 260.0, 2, 0.0, 1.0);
    g.next();
    let bowl: &[[f32; 2]] = &[[0.0, -1.0], [0.12, -0.85], [0.24, 0.12], [0.32, 0.0], [1.0, 0.0]];
    let small = g.curve(small, 0.0, 1.0, bowl);
    let large = g.curve(large, 0.0, 1.0, bowl);
    let fell = g.curve(fell, 0.45, 0.65, &[[0.0, 0.0], [1.0, 1.0]]);
    g.next();
    let large = g.math(Multiply, large, 1.7);
    let holes = g.math(Add, small, large);
    g.next();
    let holes = g.math(Multiply, holes, fell);
    let holes = g.math(Multiply, holes, 0.0);
    g.expose(holes, "b", "Craters");
    g.next();
    let ground = g.math(Add, ground, holes);
    let climate = Climate { humidity: 0.55, ..Climate::default() };
    Starter { name: "Battlefield Plains", sea_level: 0.0, graph: g.finish(ground, climate) }
}

/// Hills and dales, with no sea.
fn rolling_hills() -> Starter {
    let mut g = Layout::new();
    let broad = g.noise(Fractal, 2200.0, 2, 0.0, 1.0);
    let hills = g.noise(Fractal, 600.0, 3, 0.0, 1.0);
    let knolls = g.noise(Fractal, 180.0, 2, 0.0, 1.0);
    g.next();
    let broad = g.math(Multiply, broad, 0.55);
    let hills = g.math(Multiply, hills, 0.37);
    let knolls = g.math(Multiply, knolls, 0.08);
    g.next();
    let land = g.math(Add, broad, hills);
    let land = g.math(Add, land, knolls);
    g.next();
    let land = g.curve(land, 0.2, 0.8, &[[0.0, 0.0], [0.25, 0.08], [0.5, 0.4], [0.8, 0.85], [1.0, 1.0]]);
    let land = g.after(land, NodeKind::Warp(Bend { size: 800.0, strength: 120.0, octaves: 3 }));
    g.next();
    let high = g.math(Multiply, land, 120.0);
    g.expose(high, "b", "Height");
    let cut = g.after(high, NodeKind::Gullies(Gullies { strength: 0.5, spacing: 180.0, octaves: 3 }));
    g.expose(cut, "strength", "Gullies");
    let bumps = g.noise(Fractal, 45.0, 3, -0.6, 0.6);
    g.next();
    let ground = g.math(Add, cut, bumps);
    let climate = Climate { humidity: 0.6, ..Climate::default() };
    Starter { name: "Rolling Hills", sea_level: 0.0, graph: g.finish(ground, climate) }
}

/// Ranges of high, sharp mountains with deep valleys, and coasts at their
/// feet.
fn alpine() -> Starter {
    let mut g = Layout::new();
    let land = g.noise(Fractal, 6000.0, 4, 0.0, 1.0);
    let ridges = g.noise(Ridged, 2600.0, 5, 0.0, 1.0);
    g.next();
    let coast = g.curve(land, 0.2, 0.8, &[[0.0, -0.3], [0.35, -0.05], [0.45, 0.03], [0.6, 0.15], [1.0, 0.3]]);
    let inland = g.curve(land, 0.2, 0.8, &[[0.0, 0.0], [0.4, 0.0], [0.5, 0.3], [0.7, 0.9], [1.0, 1.0]]);
    let peaks = g.curve(ridges, 0.03, 0.87, &[[0.0, -0.12], [0.3, -0.05], [0.6, 0.35], [0.85, 0.75], [1.0, 0.85]]);
    g.next();
    let ranges = g.math(Multiply, inland, peaks);
    let land = g.math(Add, coast, ranges);
    g.next();
    let land = g.after(land, NodeKind::Warp(Bend { size: 2000.0, strength: 300.0, octaves: 3 }));
    let high = g.math(Multiply, land, 900.0);
    g.expose(high, "b", "Height");
    g.next();
    // Crags on the high ground only.
    let crags = g.noise(Ridged, 320.0, 3, 0.0, 70.0);
    let up = g.after(high, NodeKind::HeightMask(HeightMask { min: 380.0, max: 100_000.0, feather: 260.0 }));
    g.next();
    let crags = g.math(Multiply, crags, up);
    let rough = g.math(Add, high, crags);
    g.next();
    let cut = g.after(rough, NodeKind::Gullies(Gullies { strength: 0.8, spacing: 260.0, octaves: 4 }));
    g.expose(cut, "strength", "Gullies");
    let bumps = g.noise(Fractal, 40.0, 3, -0.7, 0.7);
    g.next();
    let ground = g.math(Add, cut, bumps);
    let climate = Climate { temperature: 8.0, humidity: 0.6, size: 8000.0, ..Climate::default() };
    Starter { name: "Alpine", sea_level: 0.0, graph: g.finish(ground, climate) }
}

/// Islands scattered over a sea.
fn archipelago() -> Starter {
    let mut g = Layout::new();
    let broad = g.noise(Fractal, 2800.0, 3, 0.0, 1.0);
    let ragged = g.noise(Fractal, 700.0, 3, -0.07, 0.07);
    // A mound round each point of a scatter, which lifts an islet out of the shallows where the sea is not deep.
    let scatter = g.add(NodeKind::Cells(Cellular { size: 1500.0, jitter: 1.0 }));
    let hills = g.noise(Fractal, 520.0, 3, 0.0, 1.0);
    g.next();
    let mounds = g.curve(scatter, 0.0, 0.32, &[[0.0, 0.2], [1.0, 0.0]]);
    let land = g.math(Add, broad, ragged);
    g.next();
    let land = g.math(Add, land, mounds);
    g.next();
    // The sea's bed, a shelf under the shallows, the beach, and the ground rising inland.
    let coast =
        g.curve(land, 0.2, 0.8, &[[0.0, -0.5], [0.45, -0.22], [0.6, -0.03], [0.66, 0.02], [0.78, 0.14], [1.0, 0.5]]);
    let inland = g.curve(land, 0.2, 0.8, &[[0.0, 0.0], [0.64, 0.0], [0.8, 0.6], [1.0, 1.0]]);
    g.next();
    let hills = g.math(Multiply, inland, hills);
    let hills = g.math(Multiply, hills, 0.5);
    g.next();
    let land = g.math(Add, coast, hills);
    let land = g.after(land, NodeKind::Warp(Bend { size: 1200.0, strength: 260.0, octaves: 3 }));
    g.next();
    let high = g.math(Multiply, land, 140.0);
    g.expose(high, "b", "Height");
    let bed = g.after(high, NodeKind::Clamp(Clamp { min: -45.0, max: 100_000.0, smooth: 15.0 }));
    g.next();
    let cut = g.after(bed, NodeKind::Gullies(Gullies { strength: 0.7, spacing: 170.0, octaves: 3 }));
    g.expose(cut, "strength", "Gullies");
    let bumps = g.noise(Fractal, 35.0, 3, -0.4, 0.4);
    g.next();
    let ground = g.math(Add, cut, bumps);
    let climate = Climate { temperature: 24.0, temperature_range: 4.0, humidity: 0.75, ..Climate::default() };
    Starter { name: "Archipelago", sea_level: 0.0, graph: g.finish(ground, climate) }
}

/// High ground in broad steps, cut by steep-sided canyons, their walls in
/// ledges and cut back into by ravines.
fn canyons() -> Starter {
    let mut g = Layout::new();
    let lie = g.noise(Fractal, 3000.0, 3, 0.0, 1.0);
    let lines = g.noise(Ridged, 2200.0, 1, 0.0, 1.0);
    g.next();
    // A canyon runs along each line the ridged noise draws: nothing cut on the plateau, a wall, then its floor.
    let plateau = g.curve(lie, 0.2, 0.8, &[[0.0, 0.6], [1.0, 1.0]]);
    let cut = g.curve(lines, 0.72, 1.0, &[[0.0, 0.0], [0.15, 0.0], [0.5, 0.75], [0.65, 0.97], [0.8, 1.0], [1.0, 1.0]]);
    g.next();
    // The canyons' floors lie at one level, whatever the height of the ground they cut: a tenth of the way up,
    // which is one ledge.
    let land = g.blend(plateau, 0.1, cut);
    let land = g.after(land, NodeKind::Warp(Bend { size: 1200.0, strength: 220.0, octaves: 3 }));
    g.next();
    // The ledges and ravines are cut into ground 220 m high, whatever the Height: then the ground is brought to it.
    let tall = g.math(Multiply, land, 220.0);
    let ledges = g.after(tall, NodeKind::Terrace(Terrace { step: 22.0, sharpness: 0.7 }));
    g.next();
    // Ravines run down the walls and cut back into the rim; the level plateau is left. A ravine reads the slope
    // from a little way off, so it would cut the floor beside a wall as well: the floors are held at one level,
    // the one a floor nothing cut comes to through the rounded foot of the walls (its ledge at 22 m, and 4 more).
    let ravines = g.after(ledges, NodeKind::Gullies(Gullies { strength: 1.2, spacing: 220.0, octaves: 3 }));
    let floored = g.after(ravines, NodeKind::Clamp(Clamp { min: 26.0, max: 100_000.0, smooth: 4.0 }));
    g.next();
    let level = g.math(Operation::Divide, floored, 220.0);
    let high = g.math(Multiply, level, 220.0);
    g.expose(high, "b", "Height");
    g.expose(ravines, "strength", "Ravines");
    let bumps = g.noise(Fractal, 35.0, 3, -0.5, 0.5);
    g.next();
    let ground = g.math(Add, high, bumps);
    let climate = Climate { temperature: 22.0, humidity: 0.15, humidity_range: 0.15, ..Climate::default() };
    Starter { name: "Canyons", sea_level: 0.0, graph: g.finish(ground, climate) }
}

/// Rocky hills over caves without end, every room made by rules: halls
/// with pillars and ledges and smaller chambers between, in three layers
/// one under another, joined by winding tunnels of many widths and by
/// ramps from layer to layer, with ways round and dead ends, and here and
/// there a way up to the surface. For a world of blocks; a Terrain shows
/// the hills alone.
fn caves() -> Starter {
    let mut g = Layout::new();
    let hills = Noise { size: 420.0, octaves: 4, low: 48.0, high: 110.0, ..Noise::default() };
    let hills = g.add(NodeKind::Noise(hills));
    g.expose(hills, "high", "Height");
    g.next();
    let solid = g.after(hills, NodeKind::Solid(Solid::default()));
    // Halls tens of metres across and many high, of several shapes stacked up and out, with a floor that
    // rolls and steps; chambers half their size between; near enough that rooms meet now and then.
    let hall = RoomRules {
        size: [38.0, 60.0],
        shapes: 6,
        height: 22.0,
        roughness: 1.6,
        floor: 0.6,
        pillars: 2,
        hanging: 5,
        ledges: 3,
        exits: 5,
        exits_apart: 0.8,
    };
    let chamber = RoomRules { size: [20.0, 32.0], shapes: 4, height: 11.0, pillars: 1, hanging: 3, ledges: 2, ..hall };
    let network = CaveNetwork {
        spacing: 72.0,
        depth: 44.0,
        tunnel_width: 4.0,
        tunnel_width_max: 9.0,
        winding: 1.0,
        slope_limit: 0.6,
        branches: 0.35,
        loops: 0.4,
        roughness: 1.4,
        entrances: 0.2,
        // Three layers of rooms, for a world with a great deal of rock under it.
        layers: 3,
        layer_depth: 56.0,
        rooms: vec![Room { weight: 2.0, ..Room::made(hall) }, Room { weight: 3.0, ..Room::made(chamber) }],
        ..CaveNetwork::default()
    };
    let caves = g.add(NodeKind::CaveNetwork(network));
    g.expose(caves, "depth", "Depth");
    g.expose(caves, "layers", "Layers");
    g.expose(caves, "loops", "Loops");
    g.expose(caves, "entrances", "Entrances");
    g.next();
    let cut = g.add(NodeKind::Combine(Combine { operation: Combining::Subtract, ..Combine::default() }));
    g.wire(solid, 0, cut, Combine::A);
    g.wire(caves, 0, cut, Combine::B);
    let mut graph = g.finish(hills, Climate::default());
    let output = graph.output().map(|node| node.id);
    let wired = output.is_some_and(|output| graph.connect(cut, 0, output, Output::VOLUME));
    debug_assert!(wired, "the caves' Volume");
    Starter { name: "Caves", sea_level: 0.0, graph }
}

/// Wide flat-topped tablelands at several heights, hundreds of metres over
/// valley floors of their own, with sheer walls between, stairs of broad
/// steps from level to level here and there, and a river's bed winding
/// along the floors.
fn mesas() -> Starter {
    let mut g = Layout::new();
    let land = Plateaus {
        size: 900.0,
        levels: 5,
        low: 0.0,
        high: 260.0,
        wall: 14.0,
        ramps: 0.4,
        talus: 10.0,
        valleys: 0.4,
        river: 4.0,
        canyon: 110.0,
        ..Plateaus::default()
    };
    let land = g.add(NodeKind::Plateaus(land));
    g.expose(land, "high", "Height");
    g.expose(land, "levels", "Levels");
    g.expose(land, "ramps", "Ramps");
    let roll = g.noise(Fractal, 110.0, 2, -1.2, 1.2);
    g.next();
    let ground = g.math(Add, land, roll);
    // Natural bridges carry a tableland across the river's canyon, and the walls lean and bulge.
    g.next();
    let arches = Arches { span_from: 110.0, span_to: 230.0, thickness: 10.0, width: 30.0, ..Arches::default() };
    let arches = g.add(NodeKind::Arches(arches));
    g.expose(arches, "often", "Arches");
    g.next();
    let rough = g.after(arches, NodeKind::Overhangs(Overhangs { amount: 5.0, size: 22.0, ..Overhangs::default() }));
    let climate = Climate { temperature: 20.0, humidity: 0.5, ..Climate::default() };
    let mut graph = with_volume(g.finish(ground, climate), rough);
    // The walls and their rubble are bare rock.
    let output = graph.output().map(|node| node.id);
    let wired = output.is_some_and(|output| graph.connect(land, Plateaus::WALLS, output, Output::ROCK));
    debug_assert!(wired, "the mesas' rock");
    Starter { name: "Mesas", sea_level: -1.6, graph }
}

/// `graph` with `volume` wired into its Output's Volume.
fn with_volume(mut graph: Graph, volume: NodeId) -> Graph {
    let output = graph.output().map(|node| node.id);
    let wired = output.is_some_and(|output| graph.connect(volume, 0, output, Output::VOLUME));
    debug_assert!(wired, "a starter's Volume");
    graph
}

/// Low green hills and a sea, and over them islands of land standing in
/// the sky, wide and flat on top and tapering underneath.
fn sky_islands() -> Starter {
    let mut g = Layout::new();
    let hills = g.noise(Fractal, 520.0, 4, -30.0, 70.0);
    g.expose(hills, "high", "Height");
    g.next();
    let islands = RaisedLand {
        size_from: 90.0,
        size_to: 340.0,
        height_from: 110.0,
        height_to: 300.0,
        often: 0.5,
        thickness: 60.0,
        ..RaisedLand::default()
    };
    let islands = g.add(NodeKind::RaisedLand(islands));
    g.expose(islands, "often", "Islands");
    g.expose(islands, "height_to", "Island Height");
    g.next();
    let rough = g.after(islands, NodeKind::Overhangs(Overhangs { amount: 4.0, size: 18.0, ..Overhangs::default() }));
    let climate = Climate { temperature: 16.0, humidity: 0.6, ..Climate::default() };
    Starter { name: "Sky Islands", sea_level: 0.0, graph: with_volume(g.finish(hills, climate), rough) }
}

/// Tall, rugged highlands: ridges hundreds of metres high cut into ledges,
/// with leaning crags, a natural bridge here and there, and caves in four
/// layers reaching far down into the rock.
fn highlands() -> Starter {
    let mut g = Layout::new();
    let lie = g.noise(Fractal, 2600.0, 3, 0.25, 1.0);
    let ridges = g.noise(Ridged, 900.0, 5, 0.0, 1.0);
    g.next();
    let ridges = g.after(ridges, NodeKind::Warp(Bend { size: 700.0, strength: 170.0, octaves: 2 }));
    g.next();
    let shaped = g.math(Multiply, lie, ridges);
    g.next();
    let tall = g.math(Multiply, shaped, 640.0);
    g.expose(tall, "b", "Height");
    g.next();
    let ledges = g.after(tall, NodeKind::Terrace(Terrace { step: 38.0, sharpness: 0.72 }));
    g.expose(ledges, "step", "Ledges");
    let bumps = g.noise(Fractal, 60.0, 3, -2.5, 2.5);
    g.next();
    let ground = g.math(Add, ledges, bumps);
    g.next();
    let arches = Arches { span_from: 40.0, span_to: 130.0, thickness: 7.0, width: 16.0, often: 0.8, seed: 0 };
    let arches = g.add(NodeKind::Arches(arches));
    let hall = RoomRules {
        size: [30.0, 52.0],
        shapes: 5,
        height: 18.0,
        roughness: 1.5,
        floor: 0.5,
        pillars: 2,
        hanging: 4,
        ledges: 2,
        exits: 5,
        exits_apart: 0.8,
    };
    let network = CaveNetwork {
        spacing: 84.0,
        depth: 50.0,
        tunnel_width: 4.0,
        tunnel_width_max: 8.0,
        winding: 1.0,
        slope_limit: 0.6,
        branches: 0.3,
        loops: 0.4,
        roughness: 1.3,
        entrances: 0.25,
        layers: 4,
        layer_depth: 70.0,
        rooms: vec![Room::made(hall)],
        ..CaveNetwork::default()
    };
    let caves = g.add(NodeKind::CaveNetwork(network));
    g.expose(caves, "layers", "Cave Layers");
    g.next();
    let rough = g.after(arches, NodeKind::Overhangs(Overhangs { amount: 7.0, size: 26.0, ..Overhangs::default() }));
    g.next();
    let cut = g.add(NodeKind::Combine(Combine { operation: Combining::Subtract, ..Combine::default() }));
    g.wire(rough, 0, cut, Combine::A);
    g.wire(caves, 0, cut, Combine::B);
    let climate = Climate { temperature: 9.0, humidity: 0.55, ..Climate::default() };
    // Ground steeper than about 50 degrees is bare rock.
    let steep = g.after(ground, NodeKind::SlopeMask(SlopeMask { min: 1.2, max: STEEPEST, ..SlopeMask::default() }));
    let mut graph = with_volume(g.finish(ground, climate), cut);
    let output = graph.output().map(|node| node.id);
    let wired = output.is_some_and(|output| graph.connect(steep, 0, output, Output::ROCK));
    debug_assert!(wired, "the highlands' rock");
    Starter { name: "Highlands", sea_level: 0.0, graph }
}

/// Solid rock with one bounded cave in it: a main path of rooms with side
/// rooms off it, its first room Depth under the rock's top. For a world of
/// blocks; a Terrain shows the rock's level top alone.
fn cave_level() -> Starter {
    let mut g = Layout::new();
    let rock = g.add(NodeKind::Constant(Constant { value: 64.0 }));
    g.expose(rock, "value", "Height");
    g.next();
    let solid = g.after(rock, NodeKind::Solid(Solid::default()));
    let network = CaveNetwork { layout: Laid::Level, depth: 32.0, entrances: 0.0, ..CaveNetwork::default() };
    let caves = g.add(NodeKind::CaveNetwork(network));
    g.expose(caves, "main_rooms", "Rooms");
    g.expose(caves, "side_rooms", "Side Rooms");
    g.expose(caves, "depth", "Depth");
    g.next();
    let cut = g.add(NodeKind::Combine(Combine { operation: Combining::Subtract, ..Combine::default() }));
    g.wire(solid, 0, cut, Combine::A);
    g.wire(caves, 0, cut, Combine::B);
    let mut graph = g.finish(rock, Climate::default());
    let output = graph.output().map(|node| node.id);
    let wired = output.is_some_and(|output| graph.connect(cut, 0, output, Output::VOLUME));
    debug_assert!(wired, "the cave level's Volume");
    Starter { name: "Cave Level", sea_level: 0.0, graph }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::generate::{Preset, Value, nodes::testing};

    /// A hash of a generator's ground: its height at places over it, to the bit.
    fn ground_hash(world: &WorldGen) -> u64 {
        let bits: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        crate::layers::stable_hash(bytemuck::cast_slice(&bits))
    }

    /// The starters that are kinds of ground: the first five.
    fn grounds() -> Vec<Starter> {
        starters().into_iter().take(5).collect()
    }

    #[test]
    fn the_caves_are_hills_over_rooms_made_by_rules_without_end() {
        let starter = starters().into_iter().find(|starter| starter.name == "Caves").expect("the caves");
        let world = starter.world(1, 1.0);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        assert_eq!(world.starter(), Some("Caves"));
        assert_eq!(WorldGen::parse(&world.to_text()).unwrap(), world);
        let labels: Vec<&str> = world.graph().exposed.iter().map(|exposed| exposed.label.as_str()).collect();
        assert_eq!(labels, ["Height", "Depth", "Layers", "Loops", "Entrances", "Temperature", "Humidity"]);
        let caves = world.caves().expect("its caves");
        assert_eq!(caves.layout(), crate::generate::CaveLayout::Endless);
        let rooms = caves.rooms_near([0.0, 40.0, 0.0], 600.0);
        assert!(rooms.len() > 30 && rooms.iter().all(|room| room.made), "{} rooms", rooms.len());
        // Halls and chambers both, each under the hills, with places on its rock.
        let wide = |room: &crate::generate::PlacedRoom| room.bounds[1][0] - room.bounds[0][0];
        assert!(rooms.iter().any(|room| wide(room) > 70.0) && rooms.iter().any(|room| wide(room) < 60.0));
        // In three layers, each 56 m under the one over it.
        assert!((0..3).all(|layer| rooms.iter().any(|room| room.region[1] == layer)));
        for room in &rooms {
            let under = world.height_at(room.at[0], room.at[2]) - room.at[1] - room.region[1] as f32 * 56.0;
            assert!((30.0..=58.0).contains(&under), "a room {under} m under where its layer starts");
            assert!(!room.floors.is_empty() && !room.ceilings.is_empty());
        }
        assert_ne!(starter.world(2, 1.0).caves().unwrap().rooms_near([0.0, 40.0, 0.0], 600.0), rooms);
    }

    #[test]
    fn mesas_sky_islands_and_highlands_stand_hundreds_of_metres_tall_with_a_volume_each() {
        let pins: [(&str, u64, f32); 3] = [
            ("Mesas", 0x642f_a272_1498_4fb1, 200.0),
            ("Sky Islands", 0x3f7d_04bc_1fc6_a925, 40.0),
            ("Highlands", 0x2d6a_a25a_149d_c1b0, 350.0),
        ];
        let mut changed = Vec::new();
        for (name, hash, tall) in pins {
            let starter = starters().into_iter().find(|starter| starter.name == name).expect(name);
            let world = starter.world(1, 1.0);
            let (low, high) = testing::checked(&world);
            assert!(high - low > tall, "{name}: {low} to {high} m");
            let volume = world.volume().expect("a Volume");
            // What stands over the ground, and the caves under it, say where they are: no air or rock between is
            // asked about.
            assert!(volume.spanned() && volume.depth() < 20.0, "{name}: {}", volume.depth());
            assert_eq!(world.graph().exposed[0].label, "Height", "{name}");
            assert_eq!(world.starter(), Some(name));
            if ground_hash(&world) != hash {
                changed.push(format!("{name}: {:#018x}", ground_hash(&world)));
            }
        }
        assert!(changed.is_empty(), "the ground of these changed: was it meant to? {changed:#?}");
    }

    #[test]
    fn the_cave_level_is_rock_with_a_cave_of_rooms_and_rows_for_its_size() {
        let starter = starters().pop().unwrap();
        assert_eq!(starter.name, "Cave Level");
        let world = starter.world(1, 1.0);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        assert_eq!(world.starter(), Some("Cave Level"));
        assert_eq!(WorldGen::parse(&world.to_text()).unwrap(), world);
        let labels: Vec<&str> = world.graph().exposed.iter().map(|exposed| exposed.label.as_str()).collect();
        assert_eq!(labels, ["Height", "Rooms", "Side Rooms", "Depth", "Temperature", "Humidity"]);
        assert_eq!(world.height_at(123.0, -45.0), 64.0);
        let volume = world.volume().expect("its Volume");
        assert!(volume.plain() && volume.caved());
        let rooms = world.caves().expect("its caves").rooms();
        assert_eq!(rooms.len(), 12);
        assert!(rooms.iter().all(|room| room.at[1] > 10.0 && room.at[1] < 54.0), "under the rock's top");
        // Its Rooms row is the main path's length; another seed is another cave.
        let longer = world.with_exposed(1, Value::Whole(12)).expect("its Rooms row");
        assert_eq!(longer.caves().unwrap().rooms().len(), 16);
        // Picked for a generator with caves of its own, it keeps its cave; hills picked for it take the cave along.
        let mut caved = starters()[1].graph.clone();
        assert!(caved.add_voxels(crate::generate::OwnVoxels { overhangs: 0.0, caves: 0.4, roof: 3.0 }, false));
        let picked = starter.reshapes(&WorldGen::new(4, 1.0, 0.0, caved));
        let rooms_of = |world: &WorldGen| world.caves().map(|caves| caves.rooms().len());
        assert_eq!((picked.starter(), rooms_of(&picked)), (Some("Cave Level"), Some(12)));
        let hills = starters()[1].reshapes(&world);
        assert_eq!((hills.starter(), hills.caves().is_some()), (Some("Rolling Hills"), true));
        let other = starter.world(2, 1.0).caves().unwrap().rooms();
        let regions = |rooms: &[crate::generate::PlacedRoom]| rooms.iter().map(|room| room.region).collect::<Vec<_>>();
        assert_ne!(regions(&other), regions(&rooms));
    }

    #[test]
    fn each_starter_makes_the_same_ground_to_the_bit_on_every_machine() {
        let hashes: [(&str, u64); 5] = [
            ("Battlefield Plains", 0xa2b9_e986_fb60_f2e3),
            ("Rolling Hills", 0x5678_e709_8974_f830),
            ("Alpine", 0x55de_f55e_af01_7b20),
            ("Archipelago", 0xb060_b6af_bb14_4e66),
            ("Canyons", 0x299d_7c2d_8373_cc20),
        ];
        let starters = grounds();
        assert_eq!(starters.len(), hashes.len());
        let mut changed = Vec::new();
        for (starter, (name, hash)) in starters.iter().zip(hashes) {
            assert_eq!(starter.name, name);
            let made = ground_hash(&starter.world(1, 1.0));
            if made != hash {
                changed.push(format!("{name}: {made:#018x}"));
            }
            assert_ne!(ground_hash(&starter.world(2, 1.0)), made, "another seed, other ground");
        }
        assert!(changed.is_empty(), "the ground of these changed: was it meant to? {changed:#?}");
    }

    #[test]
    fn starters_are_whole_graphs_with_rows_and_are_found_again_by_their_shape() {
        let starters = grounds();
        let names: Vec<&str> = starters.iter().map(|starter| starter.name).collect();
        assert_eq!(names, Preset::ALL.map(Preset::name), "the presets' names, in their order");
        for starter in &starters {
            // Whole, the same bits in any batch, and read back from its text as written.
            let world = starter.world(1, 1.0);
            let (low, high) = testing::checked(&world);
            assert!(low > -400.0 && high < 1400.0 && high - low > 10.0, "{}: {low} to {high} m", starter.name);
            // Its rows: its height first, and its climate's temperature and humidity last.
            let labels: Vec<&str> = world.graph().exposed.iter().map(|exposed| exposed.label.as_str()).collect();
            assert_eq!(labels[0], "Height", "{}: {labels:?}", starter.name);
            assert_eq!(labels[labels.len() - 2..], ["Temperature", "Humidity"], "{}: {labels:?}", starter.name);
            assert!((4..=6).contains(&labels.len()), "{}: {labels:?}", starter.name);
            // Twice the height, ground twice as far above the sea at its highest, or near it.
            let tall =
                world.with_exposed(0, Value::Number(2.0 * world.graph().exposed_value(0).unwrap().number().unwrap()));
            let (_, higher) = testing::range(&testing::flat(&tall.expect("its height"), 4000));
            assert!(higher > high * 1.6, "{}: {high} m, then {higher} m", starter.name);
            // Found again by its shape whatever its seed, scale, sea level and rows; not once its graph is its own.
            let mut changed = world.with_seed(9).with_scale(2.0).with_sea_level(5.0);
            for row in 0..labels.len() {
                let value = changed.graph().exposed_value(row).unwrap().number().unwrap();
                changed = changed.with_exposed(row, Value::Number(value * 0.5 + 0.1)).expect("a number");
            }
            let mut moved = changed.graph().clone();
            moved.nodes.iter_mut().for_each(|node| node.at = [node.at[1], node.at[0]]);
            moved.nodes.reverse();
            assert_eq!(changed.with_graph(moved).starter(), Some(starter.name));
            let mut own = world.graph().clone();
            own.add(NodeKind::Noise(Noise::default()), [0.0, 0.0]);
            assert_eq!(world.with_graph(own).starter(), None);
            let mut rewired = world.graph().clone();
            rewired.links.pop();
            assert_eq!(world.with_graph(rewired).starter(), None);
            // Its climate is wired in, cooled by the ground asked about.
            let (sea, peak) = (world.climate(0.0, 0.0, 0.0), world.climate(0.0, 0.0, 1000.0));
            assert!((sea.0 - peak.0 - 6.5).abs() < 1e-3 && (0.0..=1.0).contains(&sea.1), "{}", starter.name);
        }
        // A landmass is no starter, and no starter a landmass.
        assert_eq!(Preset::Alpine.world().starter(), None);
        assert_eq!(starters[2].world(1, 1.0).as_landmass(), None);
        // The plains have no craters until their row says so, and then only holes: nowhere higher than a lip.
        let plains = starters[0].world(1, 1.0);
        let row = plains.graph().exposed.iter().position(|exposed| exposed.label == "Craters").expect("its row");
        let cratered = plains.with_exposed(row, Value::Number(1.5)).unwrap();
        let (was, now) = (testing::flat(&plains, 4000), testing::flat(&cratered, 4000));
        let deepest = (0..4000).map(|i| was[i] - now[i]).fold(0.0, f32::max);
        assert!((0..4000).all(|i| now[i] <= was[i] + 0.8) && deepest > 1.0, "holes {deepest} m deep at the most");
    }

    /// The time it takes to make a tile of 289 by 289 heights, a terrain's tile with its edges: the lesser of two
    /// goes, in milliseconds.
    fn tile_time(world: &WorldGen) -> f32 {
        let side = 289;
        let xs: Vec<f32> = (0..side * side).map(|i| (i % side) as f32 * 2.0 - 300.0).collect();
        let zs: Vec<f32> = (0..side * side).map(|i| (i / side) as f32 * 2.0 + 150.0).collect();
        let mut heights = vec![0.0; side * side];
        let mut least = f32::MAX;
        for _ in 0..2 {
            let started = Instant::now();
            world.heights(&xs, &zs, &mut heights);
            least = least.min(started.elapsed().as_secs_f32() * 1000.0);
        }
        assert!(heights.iter().all(|height| height.is_finite()));
        least
    }

    #[test]
    fn a_tile_of_each_starter_takes_no_longer_than_twice_a_landmasss() {
        // Against the Alpine landmass, timed here and now, so a slow machine or a busy one measures both alike.
        let landmass = tile_time(&Preset::Alpine.world());
        for starter in starters() {
            let time = tile_time(&starter.world(1, 1.0));
            println!("{}: {time:.1} ms a tile, the Alpine landmass {landmass:.1} ms", starter.name);
            assert!(time < landmass * 2.0, "{} takes {time} ms a tile, a landmass {landmass} ms", starter.name);
        }
    }
}
