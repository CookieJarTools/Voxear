//! How what is built in an endless Voxel Volume bears its own weight, and
//! where it gives way under it ([`strain`]).
//!
//! The built voxels near a change are taken as a graph of half-metre
//! bricks, each as heavy as its voxels and made of what most of them are,
//! joined where neighbouring bricks' voxels touch, as strongly as the
//! faces that touch are many, and to what holds them up where they touch
//! it. The loads on every join under gravity are found one of two ways
//! ([`VoxelStrain`]): by solving the joins as stiff springs between bricks
//! that move and turn (a direct sparse solve, its factors made again only
//! from the first brick a broken join touches), or by sending each brick's
//! weight down the shortest way to what holds. Each join's force is split
//! into pull or push along it, shear across it and bending about it, each
//! against what the weaker of its bricks bears. A join breaks where any of
//! them goes over what it bore at rest by more than that, so whatever
//! stands at rest stands. What a break leaves joined to nothing that holds
//! comes loose, and the loads are found again, round after round, until
//! nothing more gives way.

use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use voxear_core::math::{IVec3, Vec3};

use super::{
    VoxelPiece,
    collapse::{AIR, BUILT, Grid, HOLDS, SIDES, VoxelStuff, piece_of},
};

/// How the loads on what is built are found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelStrain {
    /// Not at all: only what is cut off from what holds comes loose.
    Off,
    /// Each brick's weight sent down the shortest way to what holds: fast,
    /// but loads it moves to new ways break more than they would.
    Paths,
    /// Solved as stiff joins between bricks that move and turn: slower, and
    /// true to arches and to loads shared between supports.
    #[default]
    Solved,
}

impl VoxelStrain {
    pub const ALL: [Self; 3] = [Self::Off, Self::Paths, Self::Solved];

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Paths => "Paths",
            Self::Solved => "Solved",
        }
    }
}

/// What a stuff bears, in pascals: pulled, pushed, sheared and bent.
#[derive(Clone, Copy, Debug)]
struct Bears {
    pull: f64,
    push: f64,
    shear: f64,
    bend: f64,
    /// How stiff it is, against stone.
    stiff: f64,
}

fn bears(stuff: VoxelStuff) -> Bears {
    match stuff {
        VoxelStuff::Earth => Bears { pull: 1.5e3, push: 1.5e5, shear: 6e3, bend: 1.5e3, stiff: 0.05 },
        VoxelStuff::Stone => Bears { pull: 30e3, push: 3e6, shear: 60e3, bend: 30e3, stiff: 1.0 },
        VoxelStuff::Brick => Bears { pull: 20e3, push: 2e6, shear: 40e3, bend: 20e3, stiff: 0.6 },
        VoxelStuff::Wood => Bears { pull: 120e3, push: 120e3, shear: 40e3, bend: 120e3, stiff: 0.3 },
        VoxelStuff::Glass => Bears { pull: 8e3, push: 1e6, shear: 8e3, bend: 8e3, stiff: 0.7 },
    }
}

/// The weaker of two stuffs, by what they bear bent.
fn weaker(a: VoxelStuff, b: VoxelStuff) -> VoxelStuff {
    if bears(a).bend <= bears(b).bend { a } else { b }
}

const GRAVITY: f64 = 9.81;
/// How stiff stone is, in pascals: only how stiff each join is against the
/// others matters to the loads.
const STIFF: f64 = 1e9;
/// The most bricks a check goes over round a change: past them what is
/// built is taken as holding.
pub const MOST_STRAIN_BRICKS: usize = 4000;
/// The most rounds of loads found again after breaks.
const MOST_ROUNDS: usize = 40;

type D3 = [f64; 3];

fn sub(a: D3, b: D3) -> D3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: D3, b: D3) -> D3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: D3, s: f64) -> D3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: D3, b: D3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: D3, b: D3) -> D3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn length(a: D3) -> f64 {
    dot(a, a).sqrt()
}

/// A brick of the graph: how heavy, where its weight's middle is, what it
/// is made of, and whether it has gone (come loose, or never built).
#[derive(Clone, Debug)]
struct Node {
    brick: IVec3,
    mass: f64,
    middle: D3,
    stuff: VoxelStuff,
    gone: bool,
}

/// What tells a join from another: the brick it starts from and which way
/// it goes, a side of [`SIDES`], or [`HELD`] for what holds the brick.
pub type JoinKey = (IVec3, u8);
const HELD: u8 = 6;

/// A join between two bricks (`b` none where it is to what holds them up):
/// how many voxel faces touch, the middle of where they do, which way it
/// goes from `a`, and what the weaker side is made of.
#[derive(Clone, Debug)]
struct Join {
    key: JoinKey,
    a: usize,
    b: Option<usize>,
    faces: u32,
    point: D3,
    normal: D3,
    stuff: VoxelStuff,
    broken: bool,
}

/// The loads on a join: pull along it (pushed where less than none),
/// shear across it, and its bending moment about it, in newtons and
/// newton metres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JoinLoad {
    pub pull: f32,
    pub shear: [f32; 3],
    pub moment: [f32; 3],
}

/// What is kept of what was built between one change and the next: the
/// loads every join bore at rest, and the joins broken that left both
/// sides held.
#[derive(Clone, Debug, Default)]
pub struct VoxelStrainMemory {
    pub rest: HashMap<JoinKey, JoinLoad>,
    pub cracked: HashSet<JoinKey>,
}

/// What came loose in a round of breaks: the voxels to dig out, the
/// pieces and the crumbs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelRound {
    pub dig: Vec<IVec3>,
    pub pieces: Vec<VoxelPiece>,
    pub crumbs: Vec<Vec3>,
    /// How many joins broke.
    pub broken: usize,
}

/// What finding the loads did: how many bricks and joins, how many rounds
/// and how long the loads took to find, in seconds, all rounds together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelStrainDone {
    pub bricks: usize,
    pub joins: usize,
    pub rounds: usize,
    pub seconds: f32,
    /// How long of that finding the loads took, and how many rows of the
    /// solve's factors were made, all rounds together, against how many a
    /// round has.
    pub solving: f32,
    pub factored: usize,
    pub rows: usize,
    pub broken: usize,
    /// What came loose, a round at a time, the first first.
    pub loose: Vec<VoxelRound>,
}

/// The graph of bricks.
struct Graph {
    nodes: Vec<Node>,
    joins: Vec<Join>,
    /// Each node's joins.
    of: Vec<Vec<usize>>,
    voxel: f64,
}

impl Graph {
    /// The graph of `bricks` in `grid`: the bricks' built voxels, joined
    /// where they touch, and held where they touch what holds or built
    /// voxels of bricks not among them. Joins in `cracked` are left out.
    fn of(grid: &mut Grid, bricks: &[IVec3], cracked: &HashSet<JoinKey>) -> Self {
        let voxel = f64::from(grid.voxel);
        let index: HashMap<IVec3, usize> = bricks.iter().enumerate().map(|(i, b)| (*b, i)).collect();
        let mut nodes = Vec::with_capacity(bricks.len());
        let mut joins: Vec<Join> = Vec::new();
        let mut join_at: HashMap<JoinKey, usize> = HashMap::new();
        for (i, &b) in bricks.iter().enumerate() {
            let (mut mass, mut middle) = (0.0, [0.0; 3]);
            let mut votes: HashMap<VoxelStuff, f64> = HashMap::new();
            let each = voxel.powi(3);
            for z in 0..8 {
                for y in 0..8 {
                    for x in 0..8 {
                        let c = b * 8 + IVec3::new(x, y, z);
                        if grid.class(c) != BUILT {
                            continue;
                        }
                        let look = grid.look_of(c);
                        let stuff = grid.looks.get(usize::from(look)).map_or(VoxelStuff::Stone, |l| l.stuff);
                        let m = f64::from(stuff.density()) * each;
                        mass += m;
                        *votes.entry(stuff).or_default() += m;
                        let at = scale(add([c.x as f64, c.y as f64, c.z as f64], [0.5; 3]), voxel);
                        middle = add(middle, scale(at, m));
                        for (s, side) in SIDES.iter().enumerate() {
                            let n = c + *side;
                            let face = scale(
                                add(
                                    add([c.x as f64, c.y as f64, c.z as f64], [0.5; 3]),
                                    scale([side.x as f64, side.y as f64, side.z as f64], 0.5),
                                ),
                                voxel,
                            );
                            let class = grid.class(n);
                            let nb = n.div_euclid(IVec3::splat(8));
                            let key = match class {
                                HOLDS => Some((b, HELD, None)),
                                BUILT if nb != b => match index.get(&nb) {
                                    // Each join once, from the brick on its least side.
                                    Some(&j) if s % 2 == 0 => Some((b, s as u8, Some(j))),
                                    Some(_) => None,
                                    // Built past the bricks gone over: taken as holding.
                                    None => Some((b, HELD + 1 + s as u8, None)),
                                },
                                _ => None,
                            };
                            let Some((from, way, to)) = key else { continue };
                            let key = (from, way);
                            if cracked.contains(&key) {
                                continue;
                            }
                            let at = *join_at.entry(key).or_insert_with(|| {
                                joins.push(Join {
                                    key,
                                    a: i,
                                    b: to,
                                    faces: 0,
                                    point: [0.0; 3],
                                    normal: [0.0; 3],
                                    stuff,
                                    broken: false,
                                });
                                joins.len() - 1
                            });
                            let join = &mut joins[at];
                            join.faces += 1;
                            join.point = add(join.point, face);
                            // From what holds toward the brick; else from this brick on.
                            let way = [side.x as f64, side.y as f64, side.z as f64];
                            join.normal = add(join.normal, if to.is_some() { way } else { scale(way, -1.0) });
                            join.stuff = weaker(join.stuff, stuff);
                        }
                    }
                }
            }
            let stuff = votes.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map_or(VoxelStuff::Stone, |(s, _)| s);
            let middle = if mass > 0.0 { scale(middle, 1.0 / mass) } else { scale(add(b.as_dvec(), [4.0; 3]), voxel) };
            nodes.push(Node { brick: b, mass, middle, stuff, gone: mass <= 0.0 });
        }
        for join in &mut joins {
            join.point = scale(join.point, 1.0 / f64::from(join.faces.max(1)));
            let n = length(join.normal);
            join.normal = if n > 1e-9 { scale(join.normal, 1.0 / n) } else { [0.0, 1.0, 0.0] };
            if let Some(b) = join.b {
                join.stuff = weaker(join.stuff, nodes[b].stuff);
            }
        }
        let mut of = vec![Vec::new(); nodes.len()];
        for (j, join) in joins.iter().enumerate() {
            of[join.a].push(j);
            if let Some(b) = join.b {
                of[b].push(j);
            }
        }
        Self { nodes, joins, of, voxel }
    }

    /// The nodes no longer joined through whole joins to what holds, which
    /// come loose; each set that is joined together.
    fn unheld(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let mut held = vec![false; n];
        let mut queue: VecDeque<usize> = VecDeque::new();
        for join in self.joins.iter().filter(|j| !j.broken && j.b.is_none()) {
            if !self.nodes[join.a].gone && !held[join.a] {
                held[join.a] = true;
                queue.push_back(join.a);
            }
        }
        while let Some(i) = queue.pop_front() {
            for &j in &self.of[i] {
                let join = &self.joins[j];
                let Some(b) = join.b.filter(|_| !join.broken) else { continue };
                let other = if join.a == i { b } else { join.a };
                if !held[other] && !self.nodes[other].gone {
                    held[other] = true;
                    queue.push_back(other);
                }
            }
        }
        let mut seen = vec![false; n];
        let mut sets = Vec::new();
        for start in 0..n {
            if held[start] || seen[start] || self.nodes[start].gone {
                continue;
            }
            let mut set = vec![start];
            seen[start] = true;
            let mut at = 0;
            while at < set.len() {
                let i = set[at];
                at += 1;
                for &j in &self.of[i] {
                    let join = &self.joins[j];
                    let Some(b) = join.b.filter(|_| !join.broken) else { continue };
                    let other = if join.a == i { b } else { join.a };
                    if !seen[other] && !self.nodes[other].gone {
                        seen[other] = true;
                        set.push(other);
                    }
                }
            }
            sets.push(set);
        }
        sets
    }

    /// How much each load of `load` on join `j` goes over what it bore at
    /// rest, `rest`, against what it bears: over 1 breaks it. A join that
    /// bears less than it did, as a wall whose roof has fallen, is none
    /// the worse.
    fn over(&self, j: usize, load: JoinLoad, rest: JoinLoad) -> f64 {
        let join = &self.joins[j];
        let area = f64::from(join.faces) * self.voxel * self.voxel;
        let bears = bears(join.stuff);
        let (pull, was) = (f64::from(load.pull), f64::from(rest.pull));
        let axial = if pull > 0.0 {
            (pull - was.max(0.0)) / (bears.pull * area)
        } else {
            (-pull - (-was).max(0.0)) / (bears.push * area)
        };
        let size = |a: [f32; 3]| length(a.map(f64::from));
        let shear = (size(load.shear) - size(rest.shear)) / (bears.shear * area);
        let bend = (size(load.moment) - size(rest.moment)) / (bears.bend * area.powf(1.5) / 6.0);
        axial.max(shear).max(bend)
    }
}

trait AsD {
    fn as_dvec(self) -> D3;
}

impl AsD for IVec3 {
    fn as_dvec(self) -> D3 {
        [self.x as f64 * 8.0, self.y as f64 * 8.0, self.z as f64 * 8.0]
    }
}

/// The springs of a join: its stiffness along and across, and turned
/// about and along it.
fn springs(graph: &Graph, j: usize) -> (D3, f64, f64, f64, f64) {
    let join = &graph.joins[j];
    let area = f64::from(join.faces) * graph.voxel * graph.voxel;
    let e = STIFF * bears(join.stuff).stiff;
    let g = e * 0.4;
    let a = &graph.nodes[join.a];
    let long = match join.b {
        Some(b) => length(sub(graph.nodes[b].middle, a.middle)),
        None => 2.0 * length(sub(join.point, a.middle)),
    }
    .max(graph.voxel);
    let i = area * area / 12.0;
    (join.normal, e * area / long, g * area / long, e * i / long, g * 2.0 * i / long)
}

/// The 3 by 3 matrices of a join's springs: across and along, and about.
fn spring_matrices(n: D3, kn: f64, ks: f64, kb: f64, kt: f64) -> ([[f64; 3]; 3], [[f64; 3]; 3]) {
    let mut d = [[0.0; 3]; 3];
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for k in 0..3 {
            let nn = n[i] * n[k];
            let id = if i == k { 1.0 } else { 0.0 };
            d[i][k] = kn * nn + ks * (id - nn);
            r[i][k] = kt * nn + kb * (id - nn);
        }
    }
    (d, r)
}

/// How a node's six ways of moving move the point `r` from its middle:
/// three by six, its moving and then its turning.
fn moved_by(r: D3) -> [[f64; 6]; 3] {
    // u + θ × r = u - [r]× θ
    [[1.0, 0.0, 0.0, 0.0, r[2], -r[1]], [0.0, 1.0, 0.0, -r[2], 0.0, r[0]], [0.0, 0.0, 1.0, r[1], -r[0], 0.0]]
}

/// Bᵀ D C, six by six, with the turning springs `r` added to the turns.
fn block(b: &[[f64; 6]; 3], d: &[[f64; 3]; 3], c: &[[f64; 6]; 3], r: &[[f64; 3]; 3]) -> [[f64; 6]; 6] {
    let mut dc = [[0.0; 6]; 3];
    for i in 0..3 {
        for k in 0..6 {
            dc[i][k] = (0..3).map(|m| d[i][m] * c[m][k]).sum();
        }
    }
    let mut out = [[0.0; 6]; 6];
    for i in 0..6 {
        for k in 0..6 {
            out[i][k] = (0..3).map(|m| b[m][i] * dc[m][k]).sum();
        }
    }
    for i in 0..3 {
        for k in 0..3 {
            out[3 + i][3 + k] += r[i][k];
        }
    }
    out
}

/// A symmetric matrix kept as an envelope (every row from its first
/// nonzero to its diagonal), factored as L Lᵀ, made again from a row on.
struct Envelope {
    first: Vec<usize>,
    start: Vec<usize>,
    a: Vec<f64>,
    l: Vec<f64>,
}

impl Envelope {
    fn new(first: Vec<usize>) -> Self {
        let mut start = Vec::with_capacity(first.len() + 1);
        let mut total = 0;
        for (r, &f) in first.iter().enumerate() {
            start.push(total);
            total += r - f + 1;
        }
        start.push(total);
        Self { first, start, a: vec![0.0; total], l: vec![0.0; total] }
    }

    fn at(&self, r: usize, c: usize) -> usize {
        self.start[r] + c - self.first[r]
    }

    /// Adds `v` at row `r`, column `c`, `c` at most `r`.
    fn add(&mut self, r: usize, c: usize, v: f64) {
        let at = self.at(r, c);
        self.a[at] += v;
    }

    /// Factors it from row `from` on: the rows before stay as they were.
    fn factor(&mut self, from: usize) {
        let n = self.first.len();
        for r in from..n {
            let fr = self.first[r];
            for c in fr..=r {
                let fc = self.first[c];
                let k0 = fr.max(fc);
                let (ro, co) = (self.start[r] - fr, self.start[c] - fc);
                let mut s = self.a[ro + c];
                for k in k0..c {
                    s -= self.l[ro + k] * self.l[co + k];
                }
                if c < r {
                    let d = self.l[self.start[c] + c - fc];
                    self.l[ro + c] = s / d;
                } else {
                    self.l[ro + c] = s.max(1e-12).sqrt();
                }
            }
        }
    }

    fn solve(&self, b: &mut [f64]) {
        let n = self.first.len();
        for r in 0..n {
            let fr = self.first[r];
            let ro = self.start[r] - fr;
            let mut s = b[r];
            for k in fr..r {
                s -= self.l[ro + k] * b[k];
            }
            b[r] = s / self.l[ro + r];
        }
        for r in (0..n).rev() {
            let fr = self.first[r];
            let ro = self.start[r] - fr;
            b[r] /= self.l[ro + r];
            let v = b[r];
            for k in fr..r {
                b[k] -= self.l[ro + k] * v;
            }
        }
    }
}

/// The direct solve: the graph's nodes in an order that keeps the
/// envelope narrow, the matrix and its factors.
struct Solver {
    order: Vec<usize>,
    place: Vec<usize>,
    matrix: Envelope,
    /// The first row whose factors are out of date.
    stale: usize,
}

impl Solver {
    fn new(graph: &Graph) -> Self {
        let n = graph.nodes.len();
        let neighbours = |i: usize| {
            graph.of[i].iter().filter_map(move |&j| {
                let join = &graph.joins[j];
                join.b.map(|b| if join.a == i { b } else { join.a })
            })
        };
        // Reversed Cuthill and McKee: each part from one of its fewest joined.
        let mut order = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        let mut by_degree: Vec<usize> = (0..n).collect();
        by_degree.sort_by_key(|&i| neighbours(i).count());
        for &start in &by_degree {
            if seen[start] {
                continue;
            }
            seen[start] = true;
            let mut at = order.len();
            order.push(start);
            while at < order.len() {
                let i = order[at];
                at += 1;
                let mut next: Vec<usize> = neighbours(i).filter(|&k| !seen[k]).collect();
                next.sort_by_key(|&k| neighbours(k).count());
                next.dedup();
                for k in next {
                    if !seen[k] {
                        seen[k] = true;
                        order.push(k);
                    }
                }
            }
        }
        order.reverse();
        let mut place = vec![0; n];
        for (p, &i) in order.iter().enumerate() {
            place[i] = p;
        }
        let mut first = vec![0; n * 6];
        for (p, &i) in order.iter().enumerate() {
            let least = neighbours(i).map(|k| place[k]).fold(p, usize::min);
            for k in 0..6 {
                first[p * 6 + k] = least * 6;
            }
        }
        let mut solver = Self { order, place, matrix: Envelope::new(first), stale: 0 };
        for j in (0..graph.joins.len()).filter(|&j| !graph.joins[j].broken) {
            solver.put(graph, j, 1.0);
        }
        for i in 0..n {
            if graph.nodes[i].gone {
                solver.forget(graph, i);
            } else {
                solver.settle(graph, i);
            }
        }
        solver
    }

    /// Adds join `j`'s springs to the matrix, times `sign`.
    fn put(&mut self, graph: &Graph, j: usize, sign: f64) {
        let join = &graph.joins[j];
        let (n, kn, ks, kb, kt) = springs(graph, j);
        let (d, r) = spring_matrices(n, kn, ks, kb, kt);
        let ba = moved_by(sub(join.point, graph.nodes[join.a].middle));
        let pa = self.place[join.a];
        let aa = block(&ba, &d, &ba, &r);
        self.put_block(pa, pa, &aa, sign);
        if let Some(b) = join.b {
            let bb_ = moved_by(sub(join.point, graph.nodes[b].middle));
            let pb = self.place[b];
            let bb = block(&bb_, &d, &bb_, &r);
            self.put_block(pb, pb, &bb, sign);
            let ab = block(&ba, &d, &bb_, &r);
            // The pair's coupling is less the product.
            let (lo, hi, m) = if pa < pb { (pa, pb, transpose(&ab)) } else { (pb, pa, ab) };
            self.put_block(hi, lo, &m, -sign);
            self.stale = self.stale.min(lo * 6);
        } else {
            self.stale = self.stale.min(pa * 6);
        }
    }

    /// Adds `m` at block row `r`, block column `c`, times `sign`: of a
    /// diagonal block only its lower half.
    fn put_block(&mut self, r: usize, c: usize, m: &[[f64; 6]; 6], sign: f64) {
        for i in 0..6 {
            for k in 0..6 {
                let (row, col) = (r * 6 + i, c * 6 + k);
                if col <= row {
                    self.matrix.add(row, col, m[i][k] * sign);
                }
            }
        }
    }

    /// Keeps a node with no whole join from leaving the matrix without a
    /// solution: it is held still.
    fn settle(&mut self, graph: &Graph, i: usize) {
        let whole = graph.of[i].iter().any(|&j| !graph.joins[j].broken);
        if whole && !graph.nodes[i].gone {
            return;
        }
        let p = self.place[i];
        for k in 0..6 {
            let r = p * 6 + k;
            let at = self.matrix.at(r, r);
            if self.matrix.a[at].abs() < 1e-6 {
                self.matrix.a[at] = 1.0;
                self.stale = self.stale.min(r);
            }
        }
    }

    /// Takes join `j` out, as broken.
    fn take(&mut self, graph: &Graph, j: usize) {
        self.put(graph, j, -1.0);
    }

    /// Takes node `i` out, gone: its joins taken already.
    fn forget(&mut self, graph: &Graph, i: usize) {
        let p = self.place[i];
        for k in 0..6 {
            let r = p * 6 + k;
            let (f, s) = (self.matrix.first[r], self.matrix.start[r]);
            for c in f..=r {
                self.matrix.a[s + c - f] = 0.0;
            }
            let at = self.matrix.at(r, r);
            self.matrix.a[at] = 1.0;
        }
        self.stale = self.stale.min(p * 6);
        let _ = graph;
    }

    /// Each join's loads under the graph's weight.
    fn loads(&mut self, graph: &Graph) -> Vec<JoinLoad> {
        if self.stale < self.matrix.first.len() {
            self.matrix.factor(self.stale);
            self.stale = self.matrix.first.len();
        }
        let n = graph.nodes.len();
        let mut q = vec![0.0; n * 6];
        for (i, node) in graph.nodes.iter().enumerate() {
            if !node.gone {
                q[self.place[i] * 6 + 1] = -node.mass * GRAVITY;
            }
        }
        self.matrix.solve(&mut q);
        let moved = |i: usize| -> [f64; 6] {
            let p = self.place[i] * 6;
            [q[p], q[p + 1], q[p + 2], q[p + 3], q[p + 4], q[p + 5]]
        };
        let at = |b: &[[f64; 6]; 3], u: &[f64; 6]| -> D3 { [0, 1, 2].map(|r| (0..6).map(|k| b[r][k] * u[k]).sum()) };
        graph
            .joins
            .iter()
            .enumerate()
            .map(|(j, join)| {
                if join.broken {
                    return JoinLoad::default();
                }
                let (n, kn, ks, kb, kt) = springs(graph, j);
                let (d, r) = spring_matrices(n, kn, ks, kb, kt);
                let ua = moved(join.a);
                let pa = at(&moved_by(sub(join.point, graph.nodes[join.a].middle)), &ua);
                let (pb, tb) = match join.b {
                    Some(b) => {
                        let ub = moved(b);
                        (at(&moved_by(sub(join.point, graph.nodes[b].middle)), &ub), [ub[3], ub[4], ub[5]])
                    }
                    None => ([0.0; 3], [0.0; 3]),
                };
                // From what holds toward the brick, a held join's way is the other way about.
                let (delta, phi) = match join.b {
                    Some(_) => (sub(pb, pa), sub(tb, [ua[3], ua[4], ua[5]])),
                    None => (sub(pa, pb), sub([ua[3], ua[4], ua[5]], tb)),
                };
                let force: D3 = [0, 1, 2].map(|i| (0..3).map(|k| d[i][k] * delta[k]).sum());
                let moment: D3 = [0, 1, 2].map(|i| (0..3).map(|k| r[i][k] * phi[k]).sum());
                let pull = dot(force, n);
                let shear = sub(force, scale(n, pull));
                JoinLoad { pull: pull as f32, shear: shear.map(|a| a as f32), moment: moment.map(|a| a as f32) }
            })
            .collect()
    }
}

fn transpose(m: &[[f64; 6]; 6]) -> [[f64; 6]; 6] {
    let mut t = [[0.0; 6]; 6];
    for i in 0..6 {
        for k in 0..6 {
            t[i][k] = m[k][i];
        }
    }
    t
}

/// The load paths: each brick's weight, with what it carries, sent down
/// the shortest way to what holds, shared between the joins that way by
/// how many faces they have; bent by how far what it carries is from the
/// join, across.
fn path_loads(graph: &Graph) -> Vec<JoinLoad> {
    let n = graph.nodes.len();
    // How far each is from what holds, going down cheaper than across, and across cheaper than up.
    let cost = |join: &Join, from: usize| -> u32 {
        let up = if join.a == from { join.normal[1] } else { -join.normal[1] };
        if up < -0.5 {
            2
        } else if up > 0.5 {
            6
        } else {
            3
        }
    };
    let mut far = vec![u32::MAX; n];
    let mut queue: BinaryHeap<std::cmp::Reverse<(u32, usize)>> = BinaryHeap::new();
    for join in graph.joins.iter().filter(|j| !j.broken && j.b.is_none()) {
        if !graph.nodes[join.a].gone && far[join.a] != 0 {
            far[join.a] = 0;
            queue.push(std::cmp::Reverse((0, join.a)));
        }
    }
    while let Some(std::cmp::Reverse((d, i))) = queue.pop() {
        if d > far[i] {
            continue;
        }
        for &j in &graph.of[i] {
            let join = &graph.joins[j];
            let Some(b) = join.b.filter(|_| !join.broken) else { continue };
            let other = if join.a == i { b } else { join.a };
            if graph.nodes[other].gone {
                continue;
            }
            // The cost of going from `other` to `i`.
            let step = cost(join, other);
            if d + step < far[other] {
                far[other] = d + step;
                queue.push(std::cmp::Reverse((d + step, other)));
            }
        }
    }
    let mut order: Vec<usize> = (0..n).filter(|&i| far[i] != u32::MAX && !graph.nodes[i].gone).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(far[i]));
    // Each carries its weight and its middle, what it is given added.
    let mut carried: Vec<(f64, D3)> =
        graph.nodes.iter().map(|node| (node.mass * GRAVITY, scale(node.middle, node.mass * GRAVITY))).collect();
    let mut loads = vec![JoinLoad::default(); graph.joins.len()];
    for i in order {
        let (weight, sum) = carried[i];
        if weight <= 0.0 {
            continue;
        }
        let middle = scale(sum, 1.0 / weight);
        // The joins down its way: to what holds, or to nearer bricks.
        let down: Vec<(usize, Option<usize>)> = graph.of[i]
            .iter()
            .filter_map(|&j| {
                let join = &graph.joins[j];
                if join.broken {
                    return None;
                }
                match join.b {
                    None => Some((j, None)),
                    Some(b) => {
                        let other = if join.a == i { b } else { join.a };
                        (far[other] < far[i] && !graph.nodes[other].gone).then_some((j, Some(other)))
                    }
                }
            })
            .collect();
        let faces: f64 = down.iter().map(|(j, _)| f64::from(graph.joins[*j].faces)).sum();
        for (j, to) in down {
            let join = &graph.joins[j];
            let share = f64::from(join.faces) / faces.max(1.0);
            let w = weight * share;
            // The force the join takes from this brick, down; along its way from `a`.
            let force: D3 = [0.0, -w, 0.0];
            let n = match (join.b, join.a == i) {
                (None, _) => scale(join.normal, -1.0),
                (Some(_), true) => join.normal,
                (Some(_), false) => scale(join.normal, -1.0),
            };
            // Pulled where the force goes the way the join goes from the brick, pushed against it.
            let along = dot(force, n);
            let shear = sub(force, scale(n, along));
            let arm = sub(middle, join.point);
            let moment = cross(arm, force);
            let pull = -along;
            loads[j] = JoinLoad {
                pull: (f64::from(loads[j].pull) + pull) as f32,
                shear: [0, 1, 2].map(|k| loads[j].shear[k] + shear[k] as f32),
                moment: [0, 1, 2].map(|k| loads[j].moment[k] + moment[k] as f32),
            };
            if let Some(to) = to {
                carried[to].0 += w;
                carried[to].1 = add(carried[to].1, scale(middle, w));
            }
        }
    }
    loads
}

/// The loads of `graph` by `strain`'s way, with `solver` for a solve.
fn loads_by(strain: VoxelStrain, graph: &Graph, solver: &mut Option<Solver>) -> Vec<JoinLoad> {
    match strain {
        VoxelStrain::Solved => solver.get_or_insert_with(|| Solver::new(graph)).loads(graph),
        _ => path_loads(graph),
    }
}

/// The bricks of built voxels round `near` (voxels of bricks with built
/// voxels that touch what changed), as many as [`MOST_STRAIN_BRICKS`],
/// nearest first across the bricks.
pub(super) fn bricks_round(grid: &mut Grid, near: &[IVec3]) -> Vec<IVec3> {
    let mut seen: HashSet<IVec3> = HashSet::new();
    let mut order: Vec<IVec3> = Vec::new();
    for c in near {
        for side in SIDES.iter().chain(std::iter::once(&IVec3::ZERO)) {
            let b = (*c + *side).div_euclid(IVec3::splat(8));
            if seen.insert(b) && grid.touch(b).built > 0 {
                order.push(b);
            }
        }
    }
    let mut at = 0;
    while at < order.len() && order.len() < MOST_STRAIN_BRICKS {
        let b = order[at];
        at += 1;
        let touch = grid.touch(b);
        for (s, side) in SIDES.iter().enumerate() {
            if touch.sides[s] > 0 && seen.insert(b + *side) && order.len() < MOST_STRAIN_BRICKS {
                order.push(b + *side);
            }
        }
    }
    order
}

/// Finds the loads on what is built round `near` in `after` (the volume as
/// it is after a change, voxels in `near` changed), against what they were
/// at rest in `before` (or as `memory` keeps them), and what gives way,
/// round after round: given back a round at a time, the voxels to dig out
/// and the pieces they make, each round's taken out of `after_grid` too.
pub(super) fn strain(
    strain: VoxelStrain,
    before: &mut Grid,
    after: &mut Grid,
    near: &[IVec3],
    memory: &mut VoxelStrainMemory,
) -> VoxelStrainDone {
    let mut done = VoxelStrainDone::default();
    if strain == VoxelStrain::Off || near.is_empty() {
        return done;
    }
    let started = std::time::Instant::now();
    let bricks = bricks_round(before, near);
    if bricks.is_empty() {
        return done;
    }
    // At rest: the loads before the change, for the joins not yet known.
    let at_rest = Graph::of(before, &bricks, &memory.cracked);
    let mut solver = None;
    let rest_loads = loads_by(strain, &at_rest, &mut solver);
    for (join, load) in at_rest.joins.iter().zip(&rest_loads) {
        memory.rest.entry(join.key).or_insert(*load);
    }
    let mut graph = Graph::of(after, &bricks, &memory.cracked);
    done.bricks = graph.nodes.iter().filter(|n| !n.gone).count();
    done.joins = graph.joins.len();
    let mut solver: Option<Solver> = None;
    let mut round = VoxelRound::default();
    for _ in 0..MOST_ROUNDS {
        done.rounds += 1;
        // What has come loose first, whatever broke it.
        let loose = graph.unheld();
        if !loose.is_empty() {
            for set in loose {
                let mut voxels: Vec<IVec3> = Vec::new();
                for &i in &set {
                    graph.nodes[i].gone = true;
                    for &j in &graph.of[i].clone() {
                        if !graph.joins[j].broken {
                            graph.joins[j].broken = true;
                            if let Some(solver) = solver.as_mut() {
                                solver.take(&graph, j);
                            }
                        }
                    }
                    if let Some(solver) = solver.as_mut() {
                        solver.forget(&graph, i);
                    }
                    let b = graph.nodes[i].brick;
                    for z in 0..8 {
                        for y in 0..8 {
                            for x in 0..8 {
                                let c = b * 8 + IVec3::new(x, y, z);
                                if after.class(c) == BUILT {
                                    voxels.push(c);
                                }
                            }
                        }
                    }
                }
                take_set(after, voxels, &mut round);
            }
            if !round.dig.is_empty() || round.broken > 0 {
                done.loose.push(std::mem::take(&mut round));
            }
        }
        let solving = std::time::Instant::now();
        let stale = solver.as_ref().map_or(0, |solver| solver.stale);
        let loads = loads_by(strain, &graph, &mut solver);
        done.solving += solving.elapsed().as_secs_f32();
        if let Some(solver) = &solver {
            done.rows = solver.matrix.first.len();
            done.factored += done.rows - stale.min(done.rows);
        }
        // The joins over what they bear: the worst, and all nearly as bad.
        let overs: Vec<(usize, f64)> = graph
            .joins
            .iter()
            .enumerate()
            .filter(|(_, join)| !join.broken)
            .map(|(j, join)| (j, graph.over(j, loads[j], memory.rest.get(&join.key).copied().unwrap_or_default())))
            .filter(|(_, over)| *over > 1.0)
            .collect();
        let worst = overs.iter().map(|(_, o)| *o).fold(0.0, f64::max);
        if overs.is_empty() {
            break;
        }
        for (j, over) in overs {
            if over < worst * 0.5 {
                continue;
            }
            graph.joins[j].broken = true;
            memory.cracked.insert(graph.joins[j].key);
            if let Some(solver) = solver.as_mut() {
                solver.take(&graph, j);
            }
            round.broken += 1;
            done.broken += 1;
        }
        let ends: Vec<usize> = (0..graph.nodes.len()).collect();
        if let Some(solver) = solver.as_mut() {
            for i in ends {
                solver.settle(&graph, i);
            }
        }
    }
    if !round.dig.is_empty() {
        done.loose.push(round);
    }
    // Cracks whose bricks are gone are forgotten.
    let gone: HashSet<IVec3> = graph.nodes.iter().filter(|n| n.gone).map(|n| n.brick).collect();
    memory.cracked.retain(|key| !gone.contains(&key.0));
    memory.rest.retain(|key, _| !gone.contains(&key.0));
    done.seconds = started.elapsed().as_secs_f32();
    done
}

/// Takes `voxels` out of `grid` into `round`: each set of them that touch
/// a piece, or crumbs where too few.
fn take_set(grid: &mut Grid, voxels: Vec<IVec3>, round: &mut VoxelRound) {
    let all: HashSet<IVec3> = voxels.iter().copied().collect();
    let mut seen: HashSet<IVec3> = HashSet::new();
    for &start in &voxels {
        if !seen.insert(start) {
            continue;
        }
        let mut set = vec![start];
        let mut at = 0;
        while at < set.len() {
            let c = set[at];
            at += 1;
            for side in SIDES {
                let n = c + side;
                if all.contains(&n) && seen.insert(n) {
                    set.push(n);
                }
            }
        }
        set.sort_by_key(|c| [c.z, c.y, c.x]);
        if set.len() >= super::collapse::CRUMBS {
            round.pieces.push(piece_of(grid, &set));
        } else {
            let b = set[0].div_euclid(IVec3::splat(8));
            round.crumbs.push(((b * 8).as_vec3() + 4.0) * grid.voxel);
        }
        round.dig.extend(set.iter().copied());
    }
    for c in &voxels {
        grid.take(*c);
    }
    let _ = AIR;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_core::math::Quat;

    use super::super::collapse::tests::{flat, house};
    use super::*;
    use crate::{VoxelEdit, VoxelMaterial, VoxelVolume};

    fn joins_of(volume: &VoxelVolume, near: &[IVec3]) -> usize {
        let mut grid = Grid::new(volume, None);
        let bricks = bricks_round(&mut grid, near);
        Graph::of(&mut grid, &bricks, &HashSet::new()).joins.len()
    }

    /// The loads of the graph round `near` in `volume`, by both ways.
    fn both(volume: &VoxelVolume, near: &[IVec3]) -> (Vec<JoinLoad>, Vec<JoinLoad>, f32, f32) {
        let mut grid = Grid::new(volume, None);
        let bricks = bricks_round(&mut grid, near);
        let graph = Graph::of(&mut grid, &bricks, &HashSet::new());
        let started = std::time::Instant::now();
        let paths = path_loads(&graph);
        let path_time = started.elapsed().as_secs_f32();
        let started = std::time::Instant::now();
        let solved = Solver::new(&graph).loads(&graph);
        (paths, solved, path_time, started.elapsed().as_secs_f32())
    }

    #[test]
    fn a_column_bears_its_weight_pushed_and_a_beam_out_of_a_wall_bends() {
        let mut volume = flat();
        // A stone column a quarter metre square and 2 m tall on the ground.
        Arc::make_mut(&mut volume.edits).push(VoxelEdit::boxed(
            Vec3::new(4.0, 1.0, 4.0),
            Vec3::new(0.5, 2.0, 0.5),
            Quat::IDENTITY,
            Some(VoxelMaterial::Ashlar),
        ));
        let near = [IVec3::new(60, 40, 60)];
        let (paths, solved, ..) = both(&volume, &near);
        let mut grid = Grid::new(&volume, None);
        let bricks = bricks_round(&mut grid, &near);
        let graph = Graph::of(&mut grid, &bricks, &HashSet::new());
        let weight: f64 = graph.nodes.iter().map(|n| n.mass * GRAVITY).sum();
        // Held at its foot, pushed by all of it.
        let held: Vec<usize> = (0..graph.joins.len()).filter(|&j| graph.joins[j].b.is_none()).collect();
        for loads in [&paths, &solved] {
            let push: f64 = held
                .iter()
                .map(|&j| -(f64::from(loads[j].pull) * graph.joins[j].normal[1] + f64::from(loads[j].shear[1])))
                .sum();
            assert!((push - weight).abs() < weight * 0.02, "pushed {push} N of {weight}");
        }
        // A beam out of a wall, a metre and a half: bent most where it leaves the wall, by both.
        let mut volume = flat();
        let edits = Arc::make_mut(&mut volume.edits);
        edits.push(VoxelEdit::boxed(
            Vec3::new(4.0, 1.0, 4.0),
            Vec3::new(0.5, 3.0, 2.0),
            Quat::IDENTITY,
            Some(VoxelMaterial::Ashlar),
        ));
        edits.push(VoxelEdit::boxed(
            Vec3::new(5.0, 3.0, 4.0),
            Vec3::new(1.5, 0.5, 0.5),
            Quat::IDENTITY,
            Some(VoxelMaterial::Planks),
        ));
        let near = [IVec3::new(90, 52, 64)];
        let (paths, solved, ..) = both(&volume, &near);
        let mut grid = Grid::new(&volume, None);
        let bricks = bricks_round(&mut grid, &near);
        let graph = Graph::of(&mut grid, &bricks, &HashSet::new());
        // The beam's joins along X, by how far out they are.
        let mut beam: Vec<(f64, usize)> = (0..graph.joins.len())
            .filter(|&j| {
                graph.joins[j].b.is_some() && graph.joins[j].normal[0].abs() > 0.9 && graph.joins[j].point[0] > 4.3
            })
            .map(|j| (graph.joins[j].point[0], j))
            .collect();
        beam.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(beam.len() >= 2, "{beam:?}");
        let moment = |loads: &[JoinLoad], j: usize| length(loads[j].moment.map(f64::from));
        for loads in [&paths, &solved] {
            let at_wall = beam.iter().filter(|(x, _)| *x < 4.6).map(|(_, j)| moment(loads, *j)).fold(0.0, f64::max);
            let out = beam.iter().filter(|(x, _)| *x > 5.2).map(|(_, j)| moment(loads, *j)).fold(0.0, f64::max);
            assert!(at_wall > out * 2.0 && at_wall > 100.0, "{at_wall} N m at the wall, {out} out");
        }
    }

    #[test]
    fn a_house_at_rest_stands_and_its_corner_blown_out_brings_its_roof_down_by_either_way() {
        for way in [VoxelStrain::Paths, VoxelStrain::Solved] {
            let mut volume = flat();
            house(&mut volume, Vec3::new(4.0, 1.0, 4.0), 5.0, 4.0, 2.5);
            volume.strain = way;
            // At rest nothing gives way.
            let near = [IVec3::new(64, 40, 64)];
            let mut memory = VoxelStrainMemory::default();
            let (mut a, mut b) = (Grid::new(&volume, None), Grid::new(&volume, None));
            let done = strain(way, &mut a, &mut b, &near, &mut memory);
            assert!(done.loose.is_empty() && done.broken == 0, "{way:?} at rest: {done:?}");
            // The corner blown out, a ball 2.6 m across, at the foot.
            let mut memory = VoxelStrainMemory::default();
            let blast = super::super::collapse::VoxelBlast { at: Vec3::new(4.0, 2.0, 4.0), radius: 1.3 };
            let (_, blasted) = super::super::collapse::blast(&volume, blast, &mut memory);
            eprintln!("{way:?}: {} dug, {} loose at once", blasted.dug, blasted.settled.loosened.loose);
            let done = blasted.settled.strain;
            let loose: usize = done.loose.iter().map(|r| r.dig.len()).sum();
            eprintln!(
                "{way:?}: {} bricks, {} joins, {} rounds, {} broken, {} voxels loose in {} release rounds, {:.1} ms, solving {:.1} ms, {} of {} rows a round factored",
                done.bricks,
                done.joins,
                done.rounds,
                done.broken,
                loose,
                done.loose.len(),
                done.seconds * 1000.0,
                done.solving * 1000.0,
                done.factored / done.rounds.max(1),
                done.rows
            );
            assert!(done.broken > 0 && loose > 4000, "{way:?}: the roof over the corner comes down: {loose}");
            // Not the whole house.
            let all = joins_of(&volume, &near);
            assert!(all > 0);
        }
    }

    #[test]
    fn measure_the_two_ways_on_a_large_house() {
        let mut volume = flat();
        house(&mut volume, Vec3::new(2.0, 1.0, 2.0), 10.0, 8.0, 5.0);
        let near = [IVec3::new(32, 40, 32)];
        let (paths, solved, path_time, solve_time) = both(&volume, &near);
        let mut grid = Grid::new(&volume, None);
        let bricks = bricks_round(&mut grid, &near);
        eprintln!(
            "{} bricks, {} joins: paths {:.2} ms, solved {:.1} ms",
            bricks.len(),
            paths.len(),
            path_time * 1000.0,
            solve_time * 1000.0
        );
        assert_eq!(paths.len(), solved.len());
    }
}
