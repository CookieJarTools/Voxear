//! Graphs to start a world generator from: five kinds of ground, each
//! built from the node library and tuned by eye, with a few of its values
//! shown as rows of the generator's own (its height first), so it is a
//! preset and a few numbers until its graph is opened.
//!
//! They carry the names of a landmass's presets and make new ground under
//! them: a file made from a landmass preset is a Landmass node still, and
//! makes the ground it made.

use voxear_core::curve::Curve;

use super::{
    graph::{Graph, NodeId},
    landmass::Climate,
    nodes::{Bend, Blend, Cells, Clamp, Gullies, HeightMask, Math, NodeKind, Noise, Operation, Output, Remap, Terrace},
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

    /// Whether `graph` is this starter's still: the same nodes and wires,
    /// whatever its rows are set to and wherever its nodes lie.
    pub fn shapes(&self, graph: &Graph) -> bool {
        if graph.exposed != self.graph.exposed {
            return false;
        }
        // Its rows set as the graph's are, and both with their nodes in the order of their numbers, nowhere.
        let mut own = self.graph.clone();
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
    vec![battlefield_plains(), rolling_hills(), alpine(), archipelago(), canyons()]
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
    let small = g.add(NodeKind::Cells(Cells { size: 14.0, jitter: 1.0 }));
    let large = g.add(NodeKind::Cells(Cells { size: 30.0, jitter: 1.0 }));
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
    let scatter = g.add(NodeKind::Cells(Cells { size: 1500.0, jitter: 1.0 }));
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

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::generate::{Preset, Value, nodes::testing};

    /// A hash of a generator's ground: its height at places over flat ground and round a planet, to the bit.
    fn ground_hash(world: &WorldGen) -> u64 {
        let mut bits: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        let round = |i: usize| [i as f32 * 3.7 - 5000.0, 4000.0 - i as f32 * 1.9, i as f32 * 2.3 + 100.0];
        bits.extend((0..1024).map(|i| world.height_round(round(i)).to_bits()));
        crate::layers::stable_hash(bytemuck::cast_slice(&bits))
    }

    #[test]
    fn each_starter_makes_the_same_ground_to_the_bit_on_every_machine() {
        let hashes: [(&str, u64); 5] = [
            ("Battlefield Plains", 0xf61d_7f19_9276_6b02),
            ("Rolling Hills", 0xc807_4899_6df0_60ae),
            ("Alpine", 0x1d47_111f_7d82_50c7),
            ("Archipelago", 0x7bff_4835_90c8_2cc7),
            ("Canyons", 0xbf21_c6c1_cfc9_9aeb),
        ];
        let starters = starters();
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
        let starters = starters();
        let names: Vec<&str> = starters.iter().map(|starter| starter.name).collect();
        assert_eq!(names, Preset::ALL.map(Preset::name), "the presets' names, in their order");
        for starter in &starters {
            // Whole, the same bits in any batch flat and round, and read back from its text as written.
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
