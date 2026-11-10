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
    compile::{self, Compiled, Fault, Voxels, World},
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

/// How far under the ground the cells a Volume's preview shows are, in
/// metres: deep enough to be under a cave's roof.
const PREVIEW_UNDER: f32 = 8.0;

/// A generator's Volume: the ground in three dimensions, for worlds of
/// blocks. It is asked in two steps. First what it reads of each column of
/// ground, once a column ([`Volume::carried`]); then how far into the
/// ground cells are, given their columns' rows ([`Volume::densities`]).
#[derive(Clone, Copy, Debug)]
pub struct Volume<'a> {
    flat: &'a Voxels,
    round: &'a Voxels,
    /// The generator's own heights on flat ground.
    height: &'a Program,
}

/// How many columns of a grid apart the rows that may be read between
/// columns are made (see [`Volume::carried_grid`]).
const BETWEEN: i32 = 4;

/// What a Volume reads of the columns of ground asked about, made once for
/// each: rows of numbers, one a column in each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Carried {
    rows: Vec<Vec<f32>>,
}

impl Carried {
    /// The rows, each with a number for every column asked about.
    pub fn rows(&self) -> &[Vec<f32>] {
        &self.rows
    }

    /// Column `at`'s number in every row, each as a row of one: what
    /// [`Cells::carried`] takes for cells up that column.
    pub fn column(&self, at: usize) -> Vec<&[f32]> {
        self.rows.iter().map(|row| &row[at..=at]).collect()
    }
}

/// The cells a Volume is asked about: rows of where their middles are, the
/// rows carried up from their columns, and their size, in metres. A row of
/// one number stands for every cell, as a column's `x` and `z` and its
/// carried rows do for the cells up it.
#[derive(Clone, Copy, Debug)]
pub struct Cells<'a> {
    pub x: &'a [f32],
    pub y: &'a [f32],
    pub z: &'a [f32],
    pub carried: &'a [&'a [f32]],
    pub size: f32,
}

impl Volume<'_> {
    /// Whether the ground under its caves is the generator's height made
    /// solid and nothing more: a cell's number there is then its depth
    /// under its column's surface.
    pub fn plain(&self) -> bool {
        self.flat.plain
    }

    /// Whether anything is taken out of its ground: whether it has caves.
    /// Without any, and [`Volume::plain`], a cell is as far into the
    /// ground as its column's height is over it, and there is nothing to
    /// ask.
    pub fn caved(&self) -> bool {
        self.flat.caved
    }

    /// How far over a column's ground its cells may be solid, in metres.
    pub fn reach(&self) -> f32 {
        self.flat.reach
    }

    /// What it reads of the columns at each `(xs[i], zs[i])`, in metres in
    /// the generator's world, whose ground is `heights[i]` high: the
    /// generator's own heights there.
    pub fn carried(&self, xs: &[f32], zs: &[f32], heights: &[f32]) -> Carried {
        let count = xs.len().min(zs.len()).min(heights.len());
        made(&self.flat.columns, count, Givens { x: xs, z: zs, ground: heights, ..Givens::default() })
    }

    /// What it reads of a grid of columns, as [`Volume::carried`] gives it
    /// for each, but for the rows that change slowly from column to column
    /// (masks, such as where the ground is steep), which are made at every
    /// fourth column of the grid, counted from column 0, and read between
    /// those. They are what is slow to make, each from the ground's height
    /// at several places.
    ///
    /// The grid is `count[0]` columns along x by `count[1]` along z, from
    /// column `first`; column `v` along axis `axis` (0 for x, 1 for z) lies
    /// at `place(axis, v)` metres in the generator's world; `heights` are
    /// the generator's own there, a row along x after another. A column's
    /// numbers are the same whatever grid it is asked for in.
    pub fn carried_grid(
        &self,
        first: [i32; 2],
        count: [usize; 2],
        place: impl Fn(usize, i32) -> f32,
        heights: &[f32],
    ) -> Carried {
        let columns = |axis: usize| (0..count[axis] as i32).map(move |v| first[axis] + v);
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            columns(1).flat_map(|z| columns(0).map(move |x| (x, z))).map(|(x, z)| (place(0, x), place(1, z))).unzip();
        let all = xs.len().min(heights.len());
        let voxels = self.flat;
        let mut exact = made(&voxels.exact, all, Givens { x: &xs, z: &zs, ground: heights, ..Givens::default() }).rows;
        if !voxels.between.contains(&true) || all == 0 {
            return Carried { rows: exact };
        }
        // The columns the slow rows are made at: every fourth, from the last at or before the grid's first to
        // the first at or after its last.
        let lines = |axis: usize| {
            let from = first[axis].div_euclid(BETWEEN) * BETWEEN;
            let last = first[axis] + count[axis] as i32 - 1;
            (from, ((last - from + BETWEEN - 1) / BETWEEN + 1) as usize)
        };
        let ((from_x, wide), (from_z, deep)) = (lines(0), lines(1));
        let at = |from: i32, line: usize| from + line as i32 * BETWEEN;
        let (lx, lz): (Vec<f32>, Vec<f32>) = (0..deep)
            .flat_map(|z| (0..wide).map(move |x| (x, z)))
            .map(|(x, z)| (place(0, at(from_x, x)), place(1, at(from_z, z))))
            .unzip();
        let mut ground = Vec::new();
        if voxels.spread.reads(crate::generate::eval::Given::Ground) {
            ground = vec![0.0; lx.len()];
            self.height.run(lx.len(), Givens { x: &lx, z: &lz, ..Givens::default() }, &mut [&mut ground]);
        }
        let given = Givens { x: &lx, z: &lz, ground: &ground, ..Givens::default() };
        let spread = made(&voxels.spread, lx.len(), given).rows;
        // Each column's number read between the four made round it.
        let between = |row: &[f32]| -> Vec<f32> {
            let mut read = Vec::with_capacity(all);
            for z in columns(1) {
                let (line_z, part_z) = ((z - from_z).div_euclid(BETWEEN) as usize, (z - from_z).rem_euclid(BETWEEN));
                for x in columns(0) {
                    let (line_x, part_x) =
                        ((x - from_x).div_euclid(BETWEEN) as usize, (x - from_x).rem_euclid(BETWEEN));
                    let along = |line_z: usize| {
                        let a = row[line_z * wide + line_x];
                        if part_x == 0 {
                            a
                        } else {
                            a + (row[line_z * wide + line_x + 1] - a) * (part_x as f32 / BETWEEN as f32)
                        }
                    };
                    let a = along(line_z);
                    read.push(if part_z == 0 {
                        a
                    } else {
                        a + (along(line_z + 1) - a) * (part_z as f32 / BETWEEN as f32)
                    });
                }
            }
            read
        };
        let mut spread = spread.iter().map(|row| between(row));
        let rows = voxels.between.iter().map(|between| if *between { spread.next() } else { Some(exact.remove(0)) });
        Carried { rows: rows.map(Option::unwrap_or_default).collect() }
    }

    /// What it reads of the columns out through each of `at` on the surface
    /// of a planet of `radius` metres, whose ground is `heights[i]` high
    /// (see [`WorldGen::height_round`]).
    pub fn carried_round(&self, at: &[[f32; 3]], heights: &[f32], radius: f32) -> Carried {
        let count = at.len().min(heights.len());
        let [x, y, z] = apart(&at[..count]);
        made(&self.round.columns, count, Givens { x: &x, y: &y, z: &z, ground: heights, radius, ..Givens::default() })
    }

    /// How far into the ground each of `cells` is, in metres, out of it
    /// where negative, into `density`; and into `ground` the same with its
    /// caves left whole, which tells the open air (out of both) from a
    /// cave's. A `coarse` asking leaves the caves out of both, as far
    /// levels of a world of blocks do. Their `x` and `z` are metres in the
    /// generator's world, their `y` metres up.
    ///
    /// The numbers are exact within a cell of a surface. Further in or out
    /// they say which side a cell is on, and no less than a cell.
    pub fn densities(&self, cells: Cells, coarse: bool, density: &mut [f32], ground: &mut [f32]) {
        run_cells(self.flat, cells, 0.0, coarse, density, ground);
    }

    /// How far into the ground each of `cells` round a planet of `radius`
    /// metres is (see [`Volume::densities`]): their `x`, `y` and `z` are
    /// metres from its middle.
    pub fn densities_round(&self, cells: Cells, radius: f32, coarse: bool, density: &mut [f32], ground: &mut [f32]) {
        run_cells(self.round, cells, radius, coarse, density, ground);
    }
}

/// The rows `columns` makes over `count` columns.
fn made(columns: &Program, count: usize, given: Givens) -> Carried {
    let mut rows = vec![vec![0.0; count]; columns.outputs.len()];
    let mut outs: Vec<&mut [f32]> = rows.iter_mut().map(Vec::as_mut_slice).collect();
    columns.run(count, given, &mut outs);
    Carried { rows }
}

fn run_cells(voxels: &Voxels, cells: Cells, radius: f32, coarse: bool, density: &mut [f32], ground: &mut [f32]) {
    let count = density.len().min(ground.len());
    let Cells { x, y, z, carried, size } = cells;
    let given = Givens { x, y, z, carried, size, radius, ..Givens::default() };
    if coarse {
        voxels.coarse.run(count, given, &mut [&mut ground[..count]]);
        density[..count].copy_from_slice(&ground[..count]);
    } else {
        voxels.cells.run(count, given, &mut [&mut density[..count], &mut ground[..count]]);
    }
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
        // A generator with no Volume keeps the hash it had before there were any.
        for voxels in [&flat.volume, &round.volume].into_iter().flatten() {
            bytes.extend(voxels.hash.to_le_bytes());
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

    /// Its Volume, the ground in three dimensions, where its Output has one
    /// wired in.
    pub fn volume(&self) -> Option<Volume<'_>> {
        Some(Volume {
            flat: self.made.flat.volume.as_ref()?,
            round: self.made.round.volume.as_ref()?,
            height: &self.made.flat.height,
        })
    }

    /// Its caves of rooms, where its Volume reads a Cave Network: the rooms as they lie in the ground.
    pub fn caves(&self) -> Option<super::caves::Caves> {
        let flat = self.made.flat.volume.as_ref()?.caves.clone()?;
        let round = self.made.round.volume.as_ref()?.caves.clone()?;
        Some(super::caves::Caves::new(flat, round))
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
        let given = Givens { x: &x, y: &y, z: &z, ground: heights, ..Givens::default() };
        self.made.round.climate.run(count, given, &mut [warmth, wetness]);
    }

    /// What every output of every node makes at each `(xs[i], zs[i])`, in
    /// the order of the nodes' numbers: for the graph editor's previews.
    /// Nodes reading the ground asked about read the generator's own. A
    /// Volume shows a slice following the ground, 8 m under it, in cells of
    /// a metre: deep enough to cut through its caves.
    pub fn previews(&self, xs: &[f32], zs: &[f32]) -> Vec<Preview> {
        let world = World { seed: self.seed, scale: self.scale, sea_level: self.sea_level };
        let (program, which) = self.made.previews.get_or_init(|| compile::compile_all(&self.graph, world));
        let count = xs.len().min(zs.len());
        let mut ground = vec![0.0; count];
        self.heights(xs, zs, &mut ground);
        let under: Vec<f32> = ground.iter().map(|height| height - PREVIEW_UNDER).collect();
        let mut rows = vec![vec![0.0; count]; which.len()];
        let mut outs: Vec<&mut [f32]> = rows.iter_mut().map(Vec::as_mut_slice).collect();
        let given = Givens { x: xs, y: &under, z: zs, ground: &ground, size: 1.0, ..Givens::default() };
        program.run(count, given, &mut outs);
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
        Link, NodeKind, OwnVoxels,
        nodes::{Combine, Combining, Constant, Land, Math, Noise, Noise3d, Operation, Output, Remap},
    };

    fn bits(values: &[f32]) -> Vec<u32> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    /// The hash of the Alpine preset's generator as it was before generators had Volumes.
    const ALPINE_HASH: u64 = 0x5331_b75a_e91a_a738;

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

    /// The hills with a Volume of every kind of Volume node: their ground made solid, pushed by overhangs,
    /// hollowed by tunnels and chambers, and rock of noise through space run into it.
    fn hollow_hills() -> WorldGen {
        let hills = hills();
        let mut graph = hills.graph().clone();
        assert!(graph.add_voxels(OwnVoxels { overhangs: 6.0, caves: 0.6, roof: 3.0 }, true));
        let output = graph.output().unwrap().id;
        let hollowed = graph.link_into(output, Output::VOLUME).unwrap().from;
        let rock = Noise3d { size: 30.0, low: -20.0, high: 4.0, ..Noise3d::default() };
        let rock = graph.add(NodeKind::Noise3d(rock), [0.0, 900.0]);
        let joined = Combine { operation: Combining::Smooth, blend: 3.0 };
        let joined = graph.add(NodeKind::Combine(joined), [300.0, 900.0]);
        assert!(graph.connect(hollowed, 0, joined, Combine::A) && graph.connect(rock, 0, joined, Combine::B));
        assert!(graph.connect(joined, 0, output, Output::VOLUME));
        hills.with_graph(graph)
    }

    #[test]
    fn cells_asked_together_get_the_bits_each_gets_alone_flat_and_round() {
        let world = hollow_hills();
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        let volume = world.volume().expect("a Volume wired in");
        assert!(!volume.plain() && volume.reach() > 6.0);
        let radius = 400.0;
        for count in [1usize, 7, 289, 513, 1500] {
            // Cells scattered through the ground and the air over it, each in a column of its own.
            let xs: Vec<f32> = (0..count).map(|i| i as f32 * 3.7 - 600.0).collect();
            let zs: Vec<f32> = (0..count).map(|i| 250.0 - i as f32 * 1.3).collect();
            let mut heights = vec![0.0; count];
            world.heights(&xs, &zs, &mut heights);
            let ys: Vec<f32> = (0..count).map(|i| heights[i] + 9.0 - (i % 23) as f32).collect();
            let carried = volume.carried(&xs, &zs, &heights);
            assert!(carried.rows().len() >= 2, "the ground's height and the overhangs' mask at least");
            let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
            for (coarse, size) in [(false, 1.0), (true, 4.0), (false, 0.5)] {
                let (mut inside, mut ground) = (vec![0.0; count], vec![0.0; count]);
                let cells = Cells { x: &xs, y: &ys, z: &zs, carried: &rows, size };
                volume.densities(cells, coarse, &mut inside, &mut ground);
                let (mut alone, mut alone_ground) = (vec![0.0; count], vec![0.0; count]);
                for i in 0..count {
                    // One cell, its column's place and rows each a row of one.
                    let cells =
                        Cells { x: &xs[i..=i], y: &ys[i..=i], z: &zs[i..=i], carried: &carried.column(i), size };
                    volume.densities(cells, coarse, &mut alone[i..=i], &mut alone_ground[i..=i]);
                }
                assert_eq!(bits(&inside), bits(&alone), "{count} cells, coarse {coarse}");
                assert_eq!(bits(&ground), bits(&alone_ground));
                // The caves are only ever taken out of the ground, and a coarse asking leaves them in it.
                assert!(inside.iter().zip(&ground).all(|(inside, ground)| inside <= ground));
                assert!(!coarse || bits(&inside) == bits(&ground));
                assert!(coarse || count < 200 || bits(&inside) != bits(&ground), "caves among {count} cells");
            }
            // Round a planet, cells along ways out from its middle.
            let ways: Vec<[f32; 3]> = (0..count)
                .map(|i| {
                    let way = [
                        (i as f32 * 0.37).fract() - 0.5,
                        (i as f32 * 0.61).fract() - 0.5,
                        (i as f32 * 0.83).fract() - 0.4,
                    ];
                    let long = way.iter().map(|v| v * v).sum::<f32>().sqrt();
                    way.map(|v| v / long)
                })
                .collect();
            let on: Vec<[f32; 3]> = ways.iter().map(|way| way.map(|v| v * radius)).collect();
            world.heights_round(&on, &mut heights);
            let carried = volume.carried_round(&on, &heights, radius);
            let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
            let out = |i: usize| radius + heights[i] - world.sea_level() + 7.0 - (i % 19) as f32;
            let [x, y, z] = [0, 1, 2].map(|axis| (0..count).map(|i| ways[i][axis] * out(i)).collect::<Vec<f32>>());
            let (mut inside, mut ground) = (vec![0.0; count], vec![0.0; count]);
            volume.densities_round(
                Cells { x: &x, y: &y, z: &z, carried: &rows, size: 1.0 },
                radius,
                false,
                &mut inside,
                &mut ground,
            );
            let (mut alone, mut alone_ground) = (vec![0.0; count], vec![0.0; count]);
            for i in 0..count {
                let cells = Cells { x: &x[i..=i], y: &y[i..=i], z: &z[i..=i], carried: &carried.column(i), size: 1.0 };
                volume.densities_round(cells, radius, false, &mut alone[i..=i], &mut alone_ground[i..=i]);
            }
            assert_eq!(bits(&inside), bits(&alone), "{count} cells round a planet");
            assert_eq!(bits(&ground), bits(&alone_ground));
            if count > 100 {
                for found in [&inside, &ground] {
                    assert!(found.iter().any(|&v| v > 0.0) && found.iter().any(|&v| v < 0.0), "ground and air");
                }
            }
        }
        // A column's cells asked for at once, its place and rows each one number: what block worlds ask.
        let (x, z) = (40.0, -75.0);
        let height = world.height_at(x, z);
        let carried = volume.carried(&[x], &[z], &[height]);
        let ys: Vec<f32> = (0..600).map(|i| height + 12.0 - i as f32 * 0.5).collect();
        let (mut inside, mut ground) = (vec![0.0; 600], vec![0.0; 600]);
        let column = Cells { x: &[x], y: &ys, z: &[z], carried: &carried.column(0), size: 0.5 };
        volume.densities(column, false, &mut inside, &mut ground);
        for i in [0usize, 77, 511, 512, 599] {
            let (mut one, mut one_ground) = ([0.0], [0.0]);
            volume.densities(Cells { y: &ys[i..=i], ..column }, false, &mut one, &mut one_ground);
            assert_eq!((one[0].to_bits(), one_ground[0].to_bits()), (inside[i].to_bits(), ground[i].to_bits()));
        }
        // And the same from every core at once as from one.
        let world = Arc::new(world);
        let columns: Vec<u32> = (0..64).collect();
        let column = |&j: &u32| -> Vec<u32> {
            let volume = world.volume().unwrap();
            let (x, z) = (j as f32 * 9.0, 30.0 - j as f32 * 4.0);
            let height = world.height_at(x, z);
            let carried = volume.carried(&[x], &[z], &[height]);
            let ys: Vec<f32> = (0..200).map(|i| height + 10.0 - i as f32).collect();
            let (mut inside, mut ground) = (vec![0.0; 200], vec![0.0; 200]);
            let cells = Cells { x: &[x], y: &ys, z: &[z], carried: &carried.column(0), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            bits(&inside)
        };
        assert_eq!(crate::workers::each(&columns, column), columns.iter().map(column).collect::<Vec<_>>());
    }

    #[test]
    fn a_grid_of_columns_reads_its_masks_between_every_fourth_and_is_the_same_in_any_grid() {
        // Hills steep enough for overhangs to push: 150 m high and 60 m across.
        let mut graph = Graph::default();
        let steep = Noise { size: 60.0, octaves: 1, high: 150.0, ..Noise::default() };
        let (ground, output) = (
            graph.add(NodeKind::Noise(steep), [0.0, 0.0]),
            graph.add(NodeKind::Output(Output::default()), [300.0, 0.0]),
        );
        assert!(graph.connect(ground, 0, output, Output::HEIGHT));
        assert!(graph.add_voxels(OwnVoxels { overhangs: 6.0, caves: 0.5, roof: 3.0 }, false));
        let world = WorldGen::new(5, 1.0, 0.0, graph);
        let volume = world.volume().unwrap();
        let place = |axis: usize, v: i32| [30.0, -200.0][axis] + (v as f32 + 0.5) * 2.0;
        let grid = |first: [i32; 2], count: [usize; 2]| {
            let columns = |axis: usize| (0..count[axis] as i32).map(move |v| first[axis] + v);
            let at: Vec<(i32, i32)> = columns(1).flat_map(|z| columns(0).map(move |x| (x, z))).collect();
            let (xs, zs): (Vec<f32>, Vec<f32>) = at.iter().map(|&(x, z)| (place(0, x), place(1, z))).unzip();
            let mut heights = vec![0.0; at.len()];
            world.heights(&xs, &zs, &mut heights);
            let exact = volume.carried(&xs, &zs, &heights);
            (at, volume.carried_grid(first, count, place, &heights), exact)
        };
        let (at, read, exact) = grid([-5, 30], [34, 34]);
        assert_eq!(read.rows().len(), exact.rows().len());
        let mut masks = 0;
        for (read, exact) in read.rows().iter().zip(exact.rows()) {
            if bits(read) == bits(exact) {
                continue;
            }
            // A mask: as made at every fourth column, and between those near what it would be made there.
            masks += 1;
            let mut apart = 0.0;
            for (i, &(x, z)) in at.iter().enumerate() {
                if x.rem_euclid(4) == 0 && z.rem_euclid(4) == 0 {
                    assert_eq!(read[i].to_bits(), exact[i].to_bits(), "column {x}, {z}");
                }
                assert!((0.0..=1.0).contains(&read[i]), "column {x}, {z}");
                apart += (read[i] - exact[i]).abs();
            }
            assert!(apart / (at.len() as f32) < 0.1, "{} from the mask made at each column", apart / at.len() as f32);
            assert!(exact.iter().any(|&mask| mask > 0.5) && exact.contains(&0.0));
        }
        assert_eq!(masks, 1, "the overhangs' mask of steep ground");
        // A column has the same numbers whichever grid it is asked for in: chunks made apart agree.
        let (other_at, other, _) = grid([20, 11], [33, 40]);
        let mut shared = 0;
        for (i, place) in at.iter().enumerate() {
            let Some(j) = other_at.iter().position(|other| other == place) else { continue };
            shared += 1;
            for (row, other) in read.rows().iter().zip(other.rows()) {
                assert_eq!(row[i].to_bits(), other[j].to_bits(), "column {place:?}");
            }
        }
        assert!(shared > 100);
        // A grid of one column, and one with nothing read between, are whole too.
        let (_, one, _) = grid([7, -3], [1, 1]);
        assert_eq!(one.rows().iter().map(Vec::len).collect::<Vec<_>>(), vec![1; one.rows().len()]);
    }

    #[test]
    fn a_generators_hash_follows_its_volume_and_one_with_none_keeps_its_own() {
        // As it was before generators had Volumes: nothing a terrain was made from has changed.
        let alpine = Preset::Alpine.world();
        assert!(alpine.volume().is_none());
        assert_eq!(alpine.hash(), ALPINE_HASH, "a generator with no Volume changed its hash: {:#x}", alpine.hash());
        // Volume nodes nothing reads change nothing either.
        let mut graph = alpine.graph().clone();
        let rock = graph.add(NodeKind::Noise3d(Noise3d::default()), [0.0, 600.0]);
        assert_eq!(alpine.with_graph(graph.clone()).hash(), alpine.hash());
        // Wired in, the Volume is part of what it makes; and each value the Volume is made from.
        let output = graph.output().unwrap().id;
        assert!(graph.connect(rock, 0, output, Output::VOLUME));
        let rocky = alpine.with_graph(graph.clone());
        assert_ne!(rocky.hash(), alpine.hash());
        assert_eq!(rocky.height_at(900.0, -40.0).to_bits(), alpine.height_at(900.0, -40.0).to_bits());
        graph.node_mut(rock).unwrap().kind.set_value(1, Value::Number(55.0));
        assert_ne!(alpine.with_graph(graph.clone()).hash(), rocky.hash());
        let hollow = hollow_hills();
        for row in 0..hollow.graph().exposed.len() {
            let changed = hollow.with_exposed(row, Value::Number(0.25)).expect("the row");
            assert_ne!(changed.hash(), hollow.hash(), "{}", hollow.graph().exposed[row].label);
        }
        // Nodes moved about the canvas make the same Volume.
        let mut moved = hollow.graph().clone();
        for node in &mut moved.nodes {
            node.at = [node.at[1] - 7.0, node.at[0]];
        }
        moved.nodes.reverse();
        moved.links.reverse();
        assert_eq!(hollow.with_graph(moved).hash(), hollow.hash());
        // Written and read back it is the same generator.
        let read = WorldGen::parse(&hollow.to_text()).unwrap();
        assert_eq!(read, hollow, "{}", hollow.to_text());
        assert_eq!(read.hash(), hollow.hash());
    }

    #[test]
    fn a_field_is_carried_up_its_column_and_every_node_has_a_preview() {
        // A Field wired straight into the Output's Volume: the same number for every cell of a column.
        let hills = hills();
        let mut graph = hills.graph().clone();
        let output = graph.output().unwrap().id;
        let ground = graph.link_into(output, Output::HEIGHT).unwrap().from;
        assert!(graph.connect(ground, 0, output, Output::VOLUME));
        let world = hills.with_graph(graph);
        let volume = world.volume().unwrap();
        let (xs, zs) = ([10.0, 250.0, 900.0], [40.0, -300.0, 77.0]);
        let heights: Vec<f32> = (0..3).map(|i| world.height_at(xs[i], zs[i])).collect();
        let carried = volume.carried(&xs, &zs, &heights);
        // What makes the generator's own ground is not worked out a second time: it is the height given.
        assert_eq!(carried.rows().len(), 1);
        assert_eq!(bits(&carried.rows()[0]), bits(&heights));
        for (at, &height) in heights.iter().enumerate() {
            let ys = [-50.0, 0.0, 33.0, 2000.0];
            let (mut inside, mut ground) = ([0.0; 4], [0.0; 4]);
            let cells = Cells { x: &xs[at..=at], y: &ys, z: &zs[at..=at], carried: &carried.column(at), size: 1.0 };
            volume.densities(cells, false, &mut inside, &mut ground);
            assert_eq!(bits(&inside), bits(&[height; 4]));
        }
        assert!(!volume.plain(), "a height alone is no ground made solid");
        // Each node's outputs for its preview, a Volume's a slice 8 m under the ground.
        let world = hollow_hills();
        let previews = world.previews(&xs, &zs);
        let outputs: usize = world.graph().nodes.iter().map(|node| node.kind.outputs().len()).sum();
        assert_eq!(previews.len(), outputs);
        for preview in &previews {
            assert!(preview.values.iter().all(|value| value.is_finite()), "{preview:?}");
        }
        let solid = world.graph().nodes.iter().find(|node| matches!(node.kind, NodeKind::Solid(_))).unwrap();
        let slice = previews.iter().find(|preview| preview.node == solid.id).unwrap();
        assert!(slice.values.iter().all(|under| (under - 8.0).abs() < 1e-3), "{slice:?}");
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
