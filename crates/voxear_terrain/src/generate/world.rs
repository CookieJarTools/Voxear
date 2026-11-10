//! A world generator, as a `.worldgen` file keeps it and as everything
//! that makes ground reads it: its seed, scale and sea level, its graph,
//! and the graph compiled, ready to run.
//!
//! A generator is never changed once made: changing one makes another,
//! compiled again. So what holds one can tell by its hash whether the
//! ground it makes changed, whatever else did (a node moved on the canvas,
//! a row's label).

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use super::{
    compile::{self, Compiled, Fault, World},
    eval::{Givens, Program},
    graph::{Graph, NodeId, Written, WrittenExposed, WrittenLink, WrittenNode},
    landmass::{Landmass, Preset},
    nodes::Value,
};

/// A world generator: its seed, how large everything along its ground is,
/// its sea's height in metres, and the graph that shapes its ground.
#[derive(Clone, Debug)]
pub struct WorldGen {
    seed: u32,
    scale: f32,
    sea_level: f32,
    graph: Graph,
    /// The ground the graph editor's previews show: nothing that makes ground reads it.
    preview: PreviewArea,
    made: Arc<Made>,
}

/// The square of ground the graph editor's previews show: its middle, in
/// metres in the generator's world, and how wide it is. Kept in the file,
/// and no part of the generator's hash.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewArea {
    pub center: [f32; 2],
    pub size: f32,
}

impl Default for PreviewArea {
    fn default() -> Self {
        Self { center: [0.0, 0.0], size: 4000.0 }
    }
}

impl PreviewArea {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A generator's graph compiled, for flat ground and round a planet; a
/// hash of everything that decides the ground and climate it makes; and
/// what is wrong with the graph.
#[derive(Debug)]
struct Made {
    flat: Compiled,
    round: Compiled,
    hash: u64,
    faults: Vec<Fault>,
    /// Every node's outputs, compiled when first asked for.
    previews: OnceLock<(Program, Vec<(NodeId, usize)>)>,
}

/// What one output of one node makes over the places asked about.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub node: NodeId,
    pub output: usize,
    pub values: Vec<f32>,
}

/// A generator as its file keeps it.
#[derive(Serialize, Deserialize)]
struct File {
    #[serde(default = "one")]
    seed: u32,
    #[serde(default = "whole")]
    scale: f32,
    #[serde(default)]
    sea_level: f32,
    #[serde(default, skip_serializing_if = "PreviewArea::is_default")]
    preview: PreviewArea,
    nodes: Vec<WrittenNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<WrittenLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exposed: Vec<WrittenExposed>,
}

fn one() -> u32 {
    1
}

fn whole() -> f32 {
    1.0
}

impl PartialEq for WorldGen {
    fn eq(&self, other: &Self) -> bool {
        (self.seed, self.scale, self.sea_level) == (other.seed, other.scale, other.sea_level)
            && self.graph == other.graph
            && self.preview == other.preview
    }
}

impl Default for WorldGen {
    fn default() -> Self {
        Landmass::default().into()
    }
}

impl From<Landmass> for WorldGen {
    /// The generator a landmass is alone: its seed, scale and sea level,
    /// and a graph of it and its climate (see [`Graph::of_landmass`]).
    fn from(land: Landmass) -> Self {
        Self::new(land.seed, land.scale, land.sea_level, Graph::of_landmass(&land))
    }
}

impl WorldGen {
    pub fn new(seed: u32, scale: f32, sea_level: f32, graph: Graph) -> Self {
        let world = World { seed, scale, sea_level };
        let mut faults = Vec::new();
        let flat = compile::compile(&graph, world, false, &mut faults);
        let round = compile::compile(&graph, world, true, &mut Vec::new());
        let mut bytes = Vec::new();
        bytes.extend(seed.to_le_bytes());
        bytes.extend(scale.to_bits().to_le_bytes());
        bytes.extend(sea_level.to_bits().to_le_bytes());
        for program in [&flat.height, &flat.climate, &round.height, &round.climate] {
            bytes.extend(program.hash.to_le_bytes());
        }
        let hash = crate::layers::stable_hash(&bytes);
        Self {
            seed,
            scale,
            sea_level,
            graph,
            preview: PreviewArea::default(),
            made: Arc::new(Made { flat, round, hash, faults, previews: OnceLock::new() }),
        }
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// How large everything along its ground is: 1 for each node's own
    /// sizes, 2 for twice as large.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Its sea's height, in metres.
    pub fn sea_level(&self) -> f32 {
        self.sea_level
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// A hash of everything that decides the ground and climate it makes:
    /// the same for two generators that make the same, whatever else of
    /// them differs, and the same on every machine.
    pub fn hash(&self) -> u64 {
        self.made.hash
    }

    /// What is wrong with its graph: nothing, for one that is whole.
    pub fn faults(&self) -> &[Fault] {
        &self.made.faults
    }

    /// The ground the graph editor's previews show.
    pub fn preview_area(&self) -> PreviewArea {
        self.preview
    }

    /// Showing `preview` in the graph editor's previews: the same ground, compiled as it was.
    pub fn with_preview_area(&self, preview: PreviewArea) -> Self {
        Self { preview, ..self.clone() }
    }

    pub fn with_seed(&self, seed: u32) -> Self {
        Self::new(seed, self.scale, self.sea_level, self.graph.clone()).with_preview_area(self.preview)
    }

    pub fn with_scale(&self, scale: f32) -> Self {
        Self::new(self.seed, scale, self.sea_level, self.graph.clone()).with_preview_area(self.preview)
    }

    pub fn with_sea_level(&self, sea_level: f32) -> Self {
        Self::new(self.seed, self.scale, sea_level, self.graph.clone()).with_preview_area(self.preview)
    }

    pub fn with_graph(&self, graph: Graph) -> Self {
        Self::new(self.seed, self.scale, self.sea_level, graph).with_preview_area(self.preview)
    }

    /// With exposed row `row` set to `value`: none where it has no such
    /// row, or the row takes no such value.
    pub fn with_exposed(&self, row: usize, value: Value) -> Option<Self> {
        let exposed = self.graph.exposed.get(row)?;
        let spec = self.graph.node(exposed.node)?.kind.values().get(exposed.value)?;
        spec.kept(value)?;
        let mut graph = self.graph.clone();
        graph.set_exposed(row, value);
        Some(self.with_graph(graph))
    }

    /// The landmass it is, where its graph is one landmass's and nothing
    /// more (see [`Graph::as_landmass`]): what a file written before
    /// generators were graphs reads as.
    pub fn as_landmass(&self) -> Option<Landmass> {
        self.graph.as_landmass(self.seed, self.scale, self.sea_level)
    }

    /// The preset it is shaped as, whatever its seed, scale, sea level,
    /// height, temperature and humidity; none once its shape is its own.
    pub fn preset(&self) -> Option<Preset> {
        self.as_landmass()?.preset()
    }

    /// Shaped as `preset`, keeping its seed and scale.
    pub fn with_preset(&self, preset: Preset) -> Self {
        Self::from(Landmass { seed: self.seed, scale: self.scale, ..preset.landmass() }).with_preview_area(self.preview)
    }

    /// The ground's height at `(x, z)`, in metres in its own world.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let mut height = [0.0];
        self.heights(&[x], &[z], &mut height);
        height[0]
    }

    /// The ground's height at each `(xs[i], zs[i])`, into `heights`: the
    /// bits [`WorldGen::height_at`] gives for each alone.
    pub fn heights(&self, xs: &[f32], zs: &[f32], heights: &mut [f32]) {
        let count = xs.len().min(zs.len()).min(heights.len());
        self.made.flat.height.run(count, Givens { x: xs, z: zs, ..Givens::default() }, &mut [heights]);
    }

    /// The ground's height at `at` on a planet's surface, in metres from its
    /// middle along the way out to it, its graph read round the planet so
    /// it has no seams. Nodes that need flat ground to work on (a
    /// landmass's gullies, which run down its slope) leave it as it is.
    pub fn height_round(&self, at: [f32; 3]) -> f32 {
        let mut height = [0.0];
        self.heights_round(&[at], &mut height);
        height[0]
    }

    /// The ground's height at each of `at` on a planet's surface, into
    /// `heights` (see [`WorldGen::height_round`]).
    pub fn heights_round(&self, at: &[[f32; 3]], heights: &mut [f32]) {
        let count = at.len().min(heights.len());
        let [x, y, z] = apart(&at[..count]);
        self.made.round.height.run(count, Givens { x: &x, y: &y, z: &z, ..Givens::default() }, &mut [heights]);
    }

    /// How warm and wet it is at `(x, z)`, in metres in its own world, on
    /// ground `height` metres high: its temperature, in °C, and its
    /// humidity, 0 to 1.
    pub fn climate(&self, x: f32, z: f32, height: f32) -> (f32, f32) {
        let (mut warmth, mut wetness) = ([0.0], [0.0]);
        self.climates(&[x], &[z], &[height], &mut warmth, &mut wetness);
        (warmth[0], wetness[0])
    }

    /// How warm and wet it is at each `(xs[i], zs[i])` on ground
    /// `heights[i]` metres high, into `warmth` and `wetness` (see
    /// [`WorldGen::climate`]).
    pub fn climates(&self, xs: &[f32], zs: &[f32], heights: &[f32], warmth: &mut [f32], wetness: &mut [f32]) {
        let count = xs.len().min(zs.len()).min(heights.len()).min(warmth.len()).min(wetness.len());
        let given = Givens { x: xs, z: zs, ground: heights, ..Givens::default() };
        self.made.flat.climate.run(count, given, &mut [warmth, wetness]);
    }

    /// How warm and wet it is at `at` on a planet's surface, in metres from
    /// its middle, on ground `height` metres high (see
    /// [`WorldGen::height_round`]): as [`WorldGen::climate`] says, and
    /// colder towards its poles, along y.
    pub fn climate_round(&self, at: [f32; 3], height: f32) -> (f32, f32) {
        let (mut warmth, mut wetness) = ([0.0], [0.0]);
        self.climates_round(&[at], &[height], &mut warmth, &mut wetness);
        (warmth[0], wetness[0])
    }

    /// How warm and wet it is at each of `at` on a planet's surface, on
    /// ground `heights[i]` metres high, into `warmth` and `wetness`.
    pub fn climates_round(&self, at: &[[f32; 3]], heights: &[f32], warmth: &mut [f32], wetness: &mut [f32]) {
        let count = at.len().min(heights.len()).min(warmth.len()).min(wetness.len());
        let [x, y, z] = apart(&at[..count]);
        let given = Givens { x: &x, y: &y, z: &z, ground: heights };
        self.made.round.climate.run(count, given, &mut [warmth, wetness]);
    }

    /// What every output of every node makes at each `(xs[i], zs[i])`, in
    /// the order of the nodes' numbers: for the graph editor's previews.
    /// Nodes reading the ground asked about read the generator's own.
    pub fn previews(&self, xs: &[f32], zs: &[f32]) -> Vec<Preview> {
        let world = World { seed: self.seed, scale: self.scale, sea_level: self.sea_level };
        let (program, which) = self.made.previews.get_or_init(|| compile::compile_all(&self.graph, world));
        let count = xs.len().min(zs.len());
        let mut ground = vec![0.0; count];
        self.heights(xs, zs, &mut ground);
        let mut rows = vec![vec![0.0; count]; which.len()];
        let mut outs: Vec<&mut [f32]> = rows.iter_mut().map(Vec::as_mut_slice).collect();
        program.run(count, Givens { x: xs, z: zs, ground: &ground, ..Givens::default() }, &mut outs);
        which.iter().zip(rows).map(|(&(node, output), values)| Preview { node, output, values }).collect()
    }

    /// As read from a `.worldgen` file's text: a graph, or a file written
    /// before generators were graphs, which reads as the landmass it kept
    /// and makes the ground it made.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let table: toml::Table = toml::from_str(text)?;
        if !table.contains_key("nodes") {
            return Ok(Landmass::parse(text)?.into());
        }
        let file: File = toml::from_str(text)?;
        let graph = Graph::read(Written { nodes: file.nodes, links: file.links, exposed: file.exposed })?;
        Ok(Self::new(file.seed, file.scale, file.sea_level, graph).with_preview_area(file.preview))
    }

    /// As a `.worldgen` file's text.
    pub fn to_text(&self) -> String {
        let Written { nodes, links, exposed } = self.graph.written();
        let (seed, scale, sea_level, preview) = (self.seed, self.scale, self.sea_level, self.preview);
        let file = File { seed, scale, sea_level, preview, nodes, links, exposed };
        toml::to_string(&file).unwrap_or_default()
    }
}

/// Points' x, y and z, each in a row of its own.
fn apart(at: &[[f32; 3]]) -> [Vec<f32>; 3] {
    [0, 1, 2].map(|axis| at.iter().map(|point| point[axis]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{
        Link, NodeKind,
        nodes::{Constant, Land, Math, Noise, Operation, Output, Remap},
    };

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    /// Hills of noise shaped by a curve, raised by a constant: a graph of every plain kind of node.
    fn hills() -> WorldGen {
        let mut graph = Graph::default();
        let noise = graph.add(NodeKind::Noise(Noise { size: 300.0, high: 2.0, ..Noise::default() }), [0.0, 0.0]);
        let curve = graph.add(NodeKind::Curve(Remap { high: 2.0, ..Remap::default() }), [200.0, 0.0]);
        let base = graph.add(NodeKind::Constant(Constant { value: 12.0 }), [200.0, 150.0]);
        let scaled = graph
            .add(NodeKind::Math(Math { operation: Operation::Multiply, b: 40.0, ..Math::default() }), [400.0, 0.0]);
        let sum = graph.add(NodeKind::Math(Math::default()), [600.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [800.0, 0.0]);
        assert!(graph.connect(noise, 0, curve, 0) && graph.connect(curve, 0, scaled, 0));
        assert!(graph.connect(scaled, 0, sum, 0) && graph.connect(base, 0, sum, 1));
        assert!(graph.connect(sum, 0, output, Output::HEIGHT));
        WorldGen::new(5, 1.0, 3.0, graph)
    }

    #[test]
    fn a_file_from_before_graphs_reads_as_its_landmass_and_makes_the_same_ground_to_the_bit() {
        // Alpine as a `.worldgen` file kept it then, word for word.
        let old = r#"
seed = 1
scale = 1.0
sea_level = 0.0
height = 900.0
sharpness = 0.9
coast = [[0.0, -0.3], [0.35, -0.05], [0.45, 0.03], [0.6, 0.15], [1.0, 0.3]]
inland = [[0.0, 0.0], [0.4, 0.0], [0.5, 0.3], [0.7, 0.9], [1.0, 1.0]]
relief = [[0.0, 0.2], [0.4, 0.5], [1.0, 1.0]]
peaks = [[0.0, -0.12], [0.3, -0.05], [0.6, 0.35], [0.85, 0.75], [1.0, 0.85]]
crags = 0.12
bumps = 2.0

[continents]
size = 6000.0
octaves = 4

[ruggedness]
size = 2500.0
octaves = 3

[ridges]
size = 2600.0
octaves = 3

[warp]
size = 2000.0
strength = 300.0

[erosion]
strength = 0.7
spacing = 260.0
octaves = 4

[climate]
size = 8000.0
temperature = 8.0
temperature_range = 8.0
humidity = 0.6
humidity_range = 0.3
cooling = 6.5
"#;
        let world = WorldGen::parse(old).unwrap();
        assert_eq!(world.as_landmass(), Some(Preset::Alpine.landmass()));
        assert_eq!(world, Preset::Alpine.world());
        assert_eq!(world.preset(), Some(Preset::Alpine));
        assert!(world.faults().is_empty());
        let heights: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        let hash = crate::layers::stable_hash(bytemuck::cast_slice(&heights));
        assert_eq!(hash, 0x121a7f7a90c39192, "the ground an old file makes changed: {hash:#x}");
        // Written again it is a graph, which reads back the same.
        let text = world.to_text();
        assert!(text.contains("[[nodes]]") && text.contains("kind = \"landmass\""), "{text}");
        let again = WorldGen::parse(&text).unwrap();
        assert_eq!(again, world);
        assert_eq!(again.to_text(), text);
        assert_eq!(again.hash(), world.hash());
    }

    #[test]
    fn a_landmass_in_a_graph_makes_its_ground_and_climate_to_the_bit_flat_and_round() {
        for preset in Preset::ALL {
            let land = Landmass { seed: 7, scale: 1.3, sea_level: 4.0, ..preset.landmass() };
            let world = WorldGen::from(land.clone());
            for i in 0..600 {
                let (x, z, height) = (i as f32 * 41.7 - 9000.0, 3000.0 - i as f32 * 17.3, i as f32 * 1.7 - 80.0);
                let at = [x, i as f32 * 5.1 - 900.0, z];
                assert_eq!(world.height_at(x, z).to_bits(), land.height_at(x, z).to_bits(), "{preset:?} at {x}, {z}");
                assert_eq!(world.height_round(at).to_bits(), land.height_round(at).to_bits(), "{preset:?} at {at:?}");
                let (made, meant) = (world.climate(x, z, height), land.climate(x, z, height));
                assert_eq!([made.0.to_bits(), made.1.to_bits()], [meant.0.to_bits(), meant.1.to_bits()]);
                let (made, meant) = (world.climate_round(at, height), land.climate_round(at, height));
                assert_eq!([made.0.to_bits(), made.1.to_bits()], [meant.0.to_bits(), meant.1.to_bits()]);
            }
        }
    }

    #[test]
    fn many_places_at_once_get_the_bits_each_gets_alone() {
        for world in [Preset::Alpine.world(), hills()] {
            for count in [1usize, 7, 289, 513, 1500] {
                let xs: Vec<f32> = (0..count).map(|i| i as f32 * 13.7 - 2000.0).collect();
                let zs: Vec<f32> = (0..count).map(|i| 900.0 - i as f32 * 3.3).collect();
                let grounds: Vec<f32> = (0..count).map(|i| i as f32 * 0.9 - 30.0).collect();
                let round: Vec<[f32; 3]> = (0..count).map(|i| [xs[i], i as f32 * 2.0 - 500.0, zs[i]]).collect();
                let (mut heights, mut warmth, mut wetness) = (vec![0.0; count], vec![0.0; count], vec![0.0; count]);
                world.heights(&xs, &zs, &mut heights);
                assert_eq!(
                    bits(&heights),
                    bits(&(0..count).map(|i| world.height_at(xs[i], zs[i])).collect::<Vec<_>>())
                );
                world.climates(&xs, &zs, &grounds, &mut warmth, &mut wetness);
                let alone: Vec<(f32, f32)> = (0..count).map(|i| world.climate(xs[i], zs[i], grounds[i])).collect();
                assert_eq!(bits(&warmth), bits(&alone.iter().map(|c| c.0).collect::<Vec<_>>()));
                assert_eq!(bits(&wetness), bits(&alone.iter().map(|c| c.1).collect::<Vec<_>>()));
                world.heights_round(&round, &mut heights);
                assert_eq!(bits(&heights), bits(&round.iter().map(|&at| world.height_round(at)).collect::<Vec<_>>()));
                world.climates_round(&round, &grounds, &mut warmth, &mut wetness);
                let alone: Vec<(f32, f32)> = (0..count).map(|i| world.climate_round(round[i], grounds[i])).collect();
                assert_eq!(bits(&warmth), bits(&alone.iter().map(|c| c.0).collect::<Vec<_>>()));
            }
        }
        // And the same from every core at once as from one.
        let world = Arc::new(hills());
        let rows: Vec<u32> = (0..64).collect();
        let row = |&j: &u32| -> Vec<u32> {
            let xs: Vec<f32> = (0..300).map(|i| i as f32 * 5.0).collect();
            let mut heights = vec![0.0; 300];
            world.heights(&xs, &vec![j as f32 * 5.0; 300], &mut heights);
            bits(&heights)
        };
        assert_eq!(crate::workers::each(&rows, row), rows.iter().map(row).collect::<Vec<_>>());
    }

    #[test]
    fn a_graph_of_nodes_makes_ground_from_its_wires_and_values() {
        let world = hills();
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        assert_eq!(world.as_landmass(), None, "no landmass");
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for i in 0..4000 {
            let height = world.height_at((i % 80) as f32 * 23.0, (i / 80) as f32 * 29.0);
            (low, high) = (low.min(height), high.max(height));
        }
        // Noise of 0 to 2 through a straight curve read from 0 to 2 is 0 to 1; times 40, and 12 more.
        assert!(low >= 12.0 && high <= 52.0 && high - low > 12.0, "hills from {low} to {high}");
        // Another seed makes other hills, a larger scale wider ones, and the sea level is the generator's own.
        assert_ne!(world.with_seed(6).height_at(100.0, 100.0), world.height_at(100.0, 100.0));
        assert_eq!(world.with_scale(2.0).height_at(200.0, 600.0).to_bits(), world.height_at(100.0, 300.0).to_bits());
        assert_eq!((world.seed(), world.scale(), world.sea_level()), (5, 1.0, 3.0));
        // With no climate wired in it is mild, cooler up high, and half wet.
        assert_eq!(world.climate(10.0, 10.0, 1000.0), (15.0 - 6.5, 0.5));
        // Each node's own output, for its preview: the last Math's is the ground.
        let (xs, zs) = ([10.0, 250.0, 900.0], [40.0, -300.0, 77.0]);
        let previews = world.previews(&xs, &zs);
        assert_eq!(previews.len(), 5, "every node with an output");
        let ground: Vec<f32> = (0..3).map(|i| world.height_at(xs[i], zs[i])).collect();
        assert_eq!(bits(&previews[4].values), bits(&ground));
        assert!(previews[0].values.iter().all(|noise| (0.0..=2.0).contains(noise)));
        assert_eq!(previews[2].values, vec![12.0; 3]);
        // Round a planet the same graph is read in three dimensions.
        let round = world.height_round([300.0, 400.0, -200.0]);
        assert!((12.0..=52.0).contains(&round), "{round}");
    }

    #[test]
    fn a_graph_reads_back_from_its_text_as_it_was_written() {
        let world = hills();
        let text = world.to_text();
        let read = WorldGen::parse(&text).unwrap();
        assert_eq!(read, world, "{text}");
        assert_eq!(read.to_text(), text);
        assert_eq!(read.height_at(321.0, -45.0).to_bits(), world.height_at(321.0, -45.0).to_bits());
        // A wire to a node or port it has none of is an error, not other ground.
        assert!(WorldGen::parse(&text.replace("out = \"noise\"", "out = \"sound\"")).is_err());
        assert!(WorldGen::parse(&text.replace("from = 1", "from = 99")).is_err());
        assert!(WorldGen::parse(&text.replace("kind = \"noise\"", "kind = \"static\"")).is_err());
    }

    #[test]
    fn a_generators_hash_follows_the_ground_it_makes_and_nothing_else() {
        let world = hills();
        let mut moved = world.graph().clone();
        for node in &mut moved.nodes {
            node.at = [node.at[1], node.at[0] + 50.0];
        }
        // Nodes moved, a row shown, a node nothing reads, the file's order turned round: the same ground.
        moved.expose(NodeId(3), 0, "Base");
        moved.add(NodeKind::Noise(Noise::default()), [0.0, 500.0]);
        moved.nodes.reverse();
        moved.links.reverse();
        let moved = world.with_graph(moved);
        assert_ne!(moved, world);
        assert_eq!(moved.hash(), world.hash());
        assert_eq!(moved.height_at(50.0, 60.0).to_bits(), world.height_at(50.0, 60.0).to_bits());
        // Any value the ground is made from changes it; so do the seed, scale and sea level.
        let raised = moved.with_exposed(0, Value::Number(20.0)).expect("the row");
        assert_ne!(raised.hash(), world.hash());
        assert!((raised.height_at(50.0, 60.0) - world.height_at(50.0, 60.0) - 8.0).abs() < 1e-4);
        assert!(
            moved.with_exposed(0, Value::Choice(1)).is_none() && moved.with_exposed(4, Value::Number(1.0)).is_none()
        );
        for other in [world.with_seed(9), world.with_scale(1.5), world.with_sea_level(0.0)] {
            assert_ne!(other.hash(), world.hash());
        }
        // A landmass's rows are its generator's: its height, and its climate's temperature and humidity.
        let alpine = Preset::Alpine.world();
        let labels: Vec<&str> = alpine.graph().exposed.iter().map(|exposed| exposed.label.as_str()).collect();
        assert_eq!(labels, ["Height", "Temperature", "Humidity"]);
        let lower = alpine.with_exposed(0, Value::Number(300.0)).expect("its height");
        assert_eq!(lower.as_landmass().map(|land| land.height), Some(300.0));
        assert_eq!(lower.preset(), Some(Preset::Alpine), "its shape still");
    }

    #[test]
    fn the_ground_its_previews_show_is_kept_in_its_file_and_out_of_its_hash() {
        let world = hills();
        assert_eq!(world.preview_area(), PreviewArea::default());
        assert!(!world.to_text().contains("preview"), "left out of a file while it is the usual area");
        let moved = world.with_preview_area(PreviewArea { center: [1200.0, -300.0], size: 900.0 });
        assert_ne!(moved, world);
        assert_eq!(moved.hash(), world.hash());
        assert_eq!(moved.height_at(50.0, 60.0).to_bits(), world.height_at(50.0, 60.0).to_bits());
        let read = WorldGen::parse(&moved.to_text()).unwrap();
        assert_eq!(read, moved, "{}", moved.to_text());
        assert_eq!(read.to_text(), moved.to_text());
        // Whatever else of it changes, it shows the same ground still.
        let area = moved.preview_area();
        let others = [
            moved.with_seed(9),
            moved.with_scale(2.0),
            moved.with_sea_level(1.0),
            moved.with_preset(Preset::Alpine),
            moved.with_graph(Graph::default()),
        ];
        assert!(others.iter().all(|other| other.preview_area() == area));
    }

    #[test]
    fn a_broken_graph_still_makes_ground_and_says_what_is_wrong() {
        // No Output: the sea level everywhere.
        let empty = WorldGen::new(1, 1.0, 7.0, Graph::default());
        assert_eq!(empty.faults(), [Fault::NoOutput]);
        assert_eq!(empty.height_at(5.0, 5.0), 7.0);
        // A loop, which only a file changed by hand can hold: nothing where it closes.
        let mut graph = Graph::default();
        let a = graph.add(NodeKind::Math(Math { b: 1.0, ..Math::default() }), [0.0, 0.0]);
        let b = graph.add(NodeKind::Math(Math { b: 2.0, ..Math::default() }), [0.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
        assert!(graph.connect(a, 0, b, 0) && graph.connect(b, 0, output, 0));
        assert!(!graph.connect(b, 0, a, 0), "a wire that would close a loop is refused");
        graph.links.push(Link { from: b, output: 0, to: a, input: 0 });
        let looped = WorldGen::new(1, 1.0, 0.0, graph);
        assert!(matches!(looped.faults(), [Fault::Loop(_)]), "{:?}", looped.faults());
        assert!(looped.height_at(0.0, 0.0).is_finite());
        // A Climate shaping the ground with no height to cool by reads the sea level, and says so.
        let mut graph = Graph::of_landmass(&Preset::RollingHills.landmass());
        graph.links.retain(|link| link.input != Output::HEIGHT);
        assert!(graph.connect(NodeId(2), 0, NodeId(3), Output::HEIGHT));
        let cold = WorldGen::new(1, 1.0, 0.0, graph);
        assert_eq!(cold.faults(), [Fault::NoGround(NodeId(2))]);
        // The kinds of node there are, each with ports and rows the editor can draw.
        for kind in NodeKind::all() {
            assert!(!kind.title().is_empty());
            for (at, spec) in kind.values().iter().enumerate() {
                assert!(
                    kind.value(at).is_some_and(|value| spec.kept(value).is_some()),
                    "{} {}",
                    kind.title(),
                    spec.label
                );
            }
            assert!(kind.value(kind.values().len()).is_none());
        }
        let mut land = NodeKind::Landmass(Land::default());
        assert!(land.set_value(Land::PRESET, Value::Choice(2)) && !land.set_value(Land::PRESET, Value::Choice(2)));
        assert_eq!(land, NodeKind::Landmass(Land::from(&Preset::Alpine.landmass())));
        assert!(land.set_value(8, Value::Number(5.0)), "sharpness, held to 1");
        assert_eq!(land.value(8), Some(Value::Number(1.0)));
        assert_eq!(land.value(Land::PRESET), Some(Value::Choice(5)), "its shape its own now");
    }
}
