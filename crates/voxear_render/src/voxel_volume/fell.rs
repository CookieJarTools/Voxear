//! Cutting an endless Voxel Volume ([`cut`]): its grass, cover and crops
//! cut, and its trees felled.
//!
//! Cut grass is a byte a voxel column, how much of its blades is left (see
//! [`super::VoxelChanges::cut`]): the blades fade from there, and grow back
//! or not as the game says. A cut through logs digs them out, and every log
//! no longer joined to the ground through other logs comes loose, with the
//! leaves nearest it: by how the logs touch, not by tree, so a branch cut
//! off falls as a trunk cut through does. What comes loose is taken out of
//! the volume and given back as [`VoxelPiece`]s, for the game to drop as
//! bodies of their own, and to stamp back into the volume where they come
//! to rest ([`stamp`]).
//!
//! A tree whose every voxel over a height comes loose is kept as felled
//! there, by the column of its foot (a few bytes, see [`Felled`]), and the
//! GPU leaves out what it would grow over that height; the rest of what
//! comes loose, a branch, or the kerf, is dug out a voxel at a time. Both
//! go over the network and into the region files with the other changes.

use std::collections::{HashMap, HashSet};

use voxear_core::math::{IVec3, Mat4, Vec3};

use super::{
    MOST_VOXELS_A_METRE, VoxelChanges, VoxelKind, VoxelMaterial, VoxelVolume,
    changes::Felled,
    solid::{GRASS, Ground, VoxelSolids},
    trees::{self, AIR, LEAF, LOG, Tree, Woods},
};

/// Where a cut goes, in metres from the volume's corner.
#[derive(Clone, Debug, PartialEq)]
pub enum VoxelCut {
    /// The voxel at each point, and the grass of its column: a blade's edge
    /// drawn through them.
    Points(Vec<Vec3>),
    /// Every voxel whose middle is within `radius` of `at`, and the grass of
    /// every column it reaches.
    Ball { at: Vec3, radius: f32 },
    /// A blade's swing: the voxels within `radius` of `at` whose middles are
    /// within a voxel of the plane through `at` square to `across`, as thin
    /// as a cut through a trunk can be, and the grass of the columns it
    /// reaches over the ground.
    Slice { at: Vec3, across: Vec3, radius: f32 },
}

impl VoxelCut {
    /// The least and most voxels it may reach.
    fn bounds(&self, voxel: f32) -> (IVec3, IVec3) {
        match self {
            Self::Points(points) => {
                let cells = points.iter().map(|p| (*p / voxel).floor().as_ivec3());
                cells.fold((IVec3::MAX, IVec3::MIN), |(low, high), c| (low.min(c), high.max(c)))
            }
            Self::Ball { at, radius } | Self::Slice { at, radius, .. } => {
                ((((*at - *radius) / voxel).floor()).as_ivec3(), (((*at + *radius) / voxel).floor()).as_ivec3())
            }
        }
    }

    /// Whether the middle of voxel `c` is within it.
    fn holds(&self, c: IVec3, voxel: f32) -> bool {
        let middle = (c.as_vec3() + 0.5) * voxel;
        match self {
            Self::Points(_) => false,
            Self::Ball { at, radius } => middle.distance(*at) <= *radius,
            Self::Slice { at, across, radius } => {
                let across = across.normalize_or(Vec3::Y);
                middle.distance(*at) <= *radius && (middle - *at).dot(across).abs() <= voxel
            }
        }
    }

    /// The voxels it reaches.
    fn cells(&self, voxel: f32) -> Vec<IVec3> {
        match self {
            Self::Points(points) => {
                let mut cells: Vec<IVec3> = points.iter().map(|p| (*p / voxel).floor().as_ivec3()).collect();
                cells.sort_by_key(|c| c.to_array());
                cells.dedup();
                cells
            }
            Self::Ball { .. } | Self::Slice { .. } => {
                let (low, high) = self.bounds(voxel);
                let mut cells = Vec::new();
                for z in low.z..=high.z {
                    for y in low.y..=high.y {
                        for x in low.x..=high.x {
                            let c = IVec3::new(x, y, z);
                            if self.holds(c, voxel) {
                                cells.push(c);
                            }
                        }
                    }
                }
                cells
            }
        }
    }
}

/// What came loose of a volume's trees: its voxels, from the least corner
/// of their box, each a place in its palette and one; which of its colours
/// are wood; and how large a voxel is, in metres.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelPiece {
    /// Its box's least voxel, in voxels from the volume's corner, and how
    /// many voxels it measures each way.
    pub low: IVec3,
    pub size: [u32; 3],
    pub voxels: Vec<([u16; 3], u8)>,
    pub palette: Vec<[u8; 4]>,
    pub wood: Vec<bool>,
    pub voxel: f32,
}

impl VoxelPiece {
    /// Whether palette place and one `place` is wood.
    pub fn is_wood(&self, place: u8) -> bool {
        place > 0 && self.wood.get(usize::from(place) - 1).copied().unwrap_or(false)
    }

    /// How many of its voxels are logs.
    pub fn logs(&self) -> usize {
        self.voxels.iter().filter(|(_, place)| self.is_wood(*place)).count()
    }

    /// The middle of its logs, in metres from its box's least corner: where
    /// it balances.
    pub fn middle(&self) -> Vec3 {
        let logs: Vec<Vec3> =
            self.voxels.iter().filter(|(_, place)| self.is_wood(*place)).map(|(c, _)| cell_middle(*c)).collect();
        let sum = logs.iter().fold(Vec3::ZERO, |sum, c| sum + *c);
        sum / logs.len().max(1) as f32 * self.voxel
    }

    /// Its logs as boxes, in metres from its box's least corner: each its
    /// middle and half its size. Logs side by side are joined into boxes as
    /// long, then as wide and as tall as they go; where that is more than
    /// `most`, the logs are taken two, then four voxels at a time, a block
    /// solid where any of its voxels is.
    pub fn boxes(&self, most: usize) -> Vec<(Vec3, Vec3)> {
        let mut coarse = 1;
        loop {
            let boxes = self.boxes_at(coarse);
            if boxes.len() <= most || coarse >= 16 {
                return boxes;
            }
            coarse *= 2;
        }
    }

    fn boxes_at(&self, coarse: i32) -> Vec<(Vec3, Vec3)> {
        let size = self.size.map(|a| (a as i32 + coarse - 1) / coarse);
        let index = |c: [i32; 3]| (c[0] + size[0] * (c[1] + size[1] * c[2])) as usize;
        let mut solid = vec![false; (size[0] * size[1] * size[2]) as usize];
        for (c, place) in &self.voxels {
            if self.is_wood(*place) {
                solid[index(c.map(|a| i32::from(a) / coarse))] = true;
            }
        }
        let mut boxes = Vec::new();
        let each = coarse as f32 * self.voxel;
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    if !solid[index([x, y, z])] {
                        continue;
                    }
                    // As long along X as it goes, then as deep along Z, then as tall along Y.
                    let mut w = 1;
                    while x + w < size[0] && solid[index([x + w, y, z])] {
                        w += 1;
                    }
                    let mut d = 1;
                    while z + d < size[2] && (x..x + w).all(|i| solid[index([i, y, z + d])]) {
                        d += 1;
                    }
                    let mut h = 1;
                    while y + h < size[1] && (z..z + d).all(|k| (x..x + w).all(|i| solid[index([i, y + h, k])])) {
                        h += 1;
                    }
                    for k in z..z + d {
                        for j in y..y + h {
                            for i in x..x + w {
                                solid[index([i, j, k])] = false;
                            }
                        }
                    }
                    let low = Vec3::new(x as f32, y as f32, z as f32) * each;
                    let half = Vec3::new(w as f32, h as f32, d as f32) * each * 0.5;
                    boxes.push((low + half, half));
                }
            }
        }
        boxes
    }
}

fn cell_middle(c: [u16; 3]) -> Vec3 {
    Vec3::new(f32::from(c[0]), f32::from(c[1]), f32::from(c[2])) + 0.5
}

/// What a cut did: how many columns' grass it cut, how many voxels of logs
/// and leaves it cut through, and what came loose.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelCutDone {
    pub columns: usize,
    pub voxels: usize,
    pub pieces: Vec<VoxelPiece>,
}

/// How tall the tallest of a volume's grass and cover grows, in voxels.
fn cover_layer(volume: &VoxelVolume, voxel: f32) -> i32 {
    let grass = volume.ground.map_or(0.0, |ground| if ground.grass > 0.0 { ground.grass_height } else { 0.0 });
    let tallest = volume.cover.iter().map(|cover| cover.height).fold(grass, f32::max);
    (tallest / voxel).ceil() as i32 + 1
}

/// Cuts `volume` where `cut` goes: the grass, cover and crops of the
/// columns it reaches, and the logs and leaves it reaches, what of the
/// trees comes loose taken out with them. Gives the volume's changes after
/// it, and what it did.
pub fn cut(volume: &VoxelVolume, cut: &VoxelCut) -> (VoxelChanges, VoxelCutDone) {
    let mut changes = (*volume.changes).clone();
    let mut done = VoxelCutDone::default();
    if !volume.endless {
        return (changes, done);
    }
    let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let woods = trees::woods(&volume.trees, block);
    let mut ground = Ground::new(volume, woods.as_deref());
    let voxel = ground.voxel;
    let (low, high) = cut.bounds(voxel);
    if low.cmpgt(high).any() {
        return (changes, done);
    }
    // The grass of each column it reaches, over the column's top.
    let layer = cover_layer(volume, voxel);
    ground.heights_for(low.x, low.z, [high.x - low.x + 1, high.z - low.z + 1]);
    let mut columns: HashSet<[i32; 2]> = HashSet::new();
    match cut {
        VoxelCut::Points(points) => {
            for p in points {
                let c = (*p / voxel).floor().as_ivec3();
                let h = ground.height(c.x, c.z);
                if c.y >= h && c.y <= h + layer + 1 {
                    columns.insert([c.x, c.z]);
                }
            }
        }
        VoxelCut::Ball { at, .. } | VoxelCut::Slice { at, .. } => {
            for z in low.z..=high.z {
                for x in low.x..=high.x {
                    let h = ground.height(x, z);
                    // The voxel of the grass's layer nearest the middle, or any a slice passes through.
                    let reached = match cut {
                        VoxelCut::Slice { .. } => (h + 1..=h + layer).any(|y| cut.holds(IVec3::new(x, y, z), voxel)),
                        _ => {
                            cut.holds(IVec3::new(x, ((at.y / voxel).floor() as i32).clamp(h + 1, h + layer), z), voxel)
                        }
                    };
                    if reached {
                        columns.insert([x, z]);
                    }
                }
            }
        }
    }
    for [x, z] in &columns {
        done.columns += usize::from(changes.cut(*x, *z));
    }
    let Some(woods) = woods.as_deref() else { return (changes, done) };
    // The logs and leaves it reaches are dug out.
    let near = ground.trees_near(low, high);
    if near.is_empty() {
        return (changes, done);
    }
    let edits = ground.edits_near(low, high);
    let mut felled: Vec<Tree> = Vec::new();
    for c in cut.cells(voxel) {
        let reached = near.iter().filter(|tree| woods.at(tree, c.to_array()) != AIR);
        let reached: Vec<Tree> = reached.copied().collect();
        if reached.is_empty() {
            continue;
        }
        let kind = ground.kind(c, &near, &edits);
        if kind != LOG && kind != LEAF {
            continue;
        }
        changes.set(c, None);
        done.voxels += 1;
        if kind == LOG {
            for tree in reached.into_iter().filter(|tree| woods.at(tree, c.to_array()) == LOG) {
                if !felled.contains(&tree) {
                    felled.push(tree);
                }
            }
        }
    }
    if felled.is_empty() {
        return (changes, done);
    }
    let pieces = loosen(volume, woods, &felled, &mut changes);
    done.pieces = pieces;
    (changes, done)
}

/// The fewest voxels of logs that come loose as a piece: fewer are a chip
/// of the kerf, dug out with it.
const CHIP: usize = 48;

/// A piece of a plant's block's logs, those left that touch: a block cut
/// through is in two or more. Which way the block's log lies, and whether
/// the piece reaches the block's first slice along it, its last, and its
/// middle, where the plant's other logs join it from the side.
struct Segment {
    tree: usize,
    axis: usize,
    first: bool,
    last: bool,
    middle: bool,
    voxels: Vec<IVec3>,
}

fn push(segments: &mut Vec<Segment>, segment: Segment) -> usize {
    segments.push(segment);
    segments.len() - 1
}

/// Which segments are joined, each pointing to one joined with it, the
/// first of them all pointing to itself.
struct Joined(Vec<usize>);

impl Joined {
    fn root(&mut self, mut at: usize) -> usize {
        while self.0[at] != at {
            self.0[at] = self.0[self.0[at]];
            at = self.0[at];
        }
        at
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.0[a.max(b)] = a.min(b);
        }
    }
}

/// The piece (0 none) of the log nearest `at` among `buckets`, the logs
/// in buckets of a brick: none within eight bricks.
fn nearest(buckets: &HashMap<IVec3, Vec<(IVec3, u32)>>, at: Vec3) -> Option<u32> {
    let brick = (at / 8.0).floor().as_ivec3();
    for ring in 0..=8i32 {
        let mut best: Option<(f32, u32)> = None;
        for z in -ring..=ring {
            for y in -ring..=ring {
                for x in -ring..=ring {
                    if x.abs().max(y.abs()).max(z.abs()) != ring {
                        continue;
                    }
                    let Some(found) = buckets.get(&(brick + IVec3::new(x, y, z))) else { continue };
                    for (c, label) in found {
                        let d = (c.as_vec3() + 0.5).distance_squared(at);
                        if best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, *label));
                        }
                    }
                }
            }
        }
        if let Some((_, label)) = best {
            return Some(label);
        }
    }
    None
}

/// Takes out of `changes` what of the trees comes loose after logs of
/// `cut` were cut through, and gives it as pieces.
fn loosen(volume: &VoxelVolume, woods: &Woods, cut: &[Tree], changes: &mut VoxelChanges) -> Vec<VoxelPiece> {
    let after = VoxelVolume { changes: std::sync::Arc::new(changes.clone()), ..volume.clone() };
    let mut ground = Ground::new(&after, Some(woods));
    let voxel = ground.voxel;
    // Where they stood, and every tree that reaches there.
    let (mut low, mut high) = (IVec3::MAX, IVec3::MIN);
    for tree in cut {
        let (least, most) = woods.bounds(tree);
        low = low.min(IVec3::from(least) - 1);
        high = high.max(IVec3::from(most) + 1);
    }
    let near = ground.trees_near(low, high);
    // Of those, the trees whose logs may touch the cut ones': the cut trees, and any whose logs' box meets the box of
    // one taken, until no more are.
    let b = woods.block;
    let boxes: Vec<(IVec3, IVec3)> = near
        .iter()
        .map(|tree| {
            let (low, high) = woods.log_bounds(tree);
            (IVec3::from(low), IVec3::from(high))
        })
        .collect();
    let mut taken: Vec<bool> = near.iter().map(|tree| cut.iter().any(|c| c.foot == tree.foot)).collect();
    loop {
        let more: Vec<usize> = (0..near.len())
            .filter(|&i| !taken[i])
            .filter(|&i| {
                (0..near.len()).any(|j| {
                    taken[j] && (boxes[i].0 - 1).cmple(boxes[j].1).all() && (boxes[i].1 + 1).cmpge(boxes[j].0).all()
                })
            })
            .collect();
        if more.is_empty() {
            break;
        }
        more.into_iter().for_each(|i| taken[i] = true);
    }
    let trees: Vec<Tree> = near.iter().zip(&taken).filter(|(_, taken)| **taken).map(|(tree, _)| *tree).collect();
    let mut log_blocks: Vec<_> = trees.iter().map(|tree| woods.log_blocks(tree)).collect();
    let edits = ground.edits_near(low, high);
    // Whether a tree's voxel is there still (nothing set or edited over it, and no ground), and whether it is solid
    // without the trees.
    let there = |ground: &mut Ground, c: IVec3| {
        let (solid, held) = ground.without_trees(c, &edits);
        !solid && !held
    };
    let solid = |ground: &mut Ground, c: IVec3| ground.without_trees(c, &edits).0;
    // Their logs as they are now, block by block of their plants, each block in pieces that touch.
    let mut logs: HashSet<IVec3> = HashSet::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut blocks: HashMap<(usize, [i32; 3]), Vec<usize>> = HashMap::new();
    let mut corners: Vec<(usize, IVec3, Vec<usize>)> = Vec::new();
    for (t, own) in log_blocks.iter_mut().enumerate() {
        for (block, (corner, all)) in std::mem::take(own) {
            let present: Vec<IVec3> = all.iter().map(|c| IVec3::from(*c)).filter(|c| there(&mut ground, *c)).collect();
            if present.is_empty() {
                continue;
            }
            logs.extend(present.iter().copied());
            // Which way its log lies: the way it fills the block from end to end, up where it fills it every way.
            let reach = |a: usize| {
                let (l, m) = all.iter().fold((i32::MAX, i32::MIN), |(l, m), c| (l.min(c[a]), m.max(c[a])));
                m - l + 1
            };
            let axis = match [reach(0) >= b, reach(1) >= b, reach(2) >= b] {
                [true, false, false] => 0,
                [false, false, true] => 2,
                _ => 1,
            };
            let slice = |c: &IVec3| (c[axis] - corner[axis]).clamp(0, b - 1);
            // Its logs left, in pieces that touch: a cut through the block, straight or aslant, leaves two. Worked
            // out on the block's own grid: 0 none, 1 a log, 2 one taken.
            let corner = IVec3::from(corner);
            let inside = |c: IVec3| {
                let l = c - corner;
                (l.cmpge(IVec3::ZERO).all() && l.cmplt(IVec3::splat(b)).all())
                    .then(|| (l.x + b * (l.y + b * l.z)) as usize)
            };
            let mut grid = vec![0u8; (b * b * b) as usize];
            for c in &present {
                if let Some(at) = inside(*c) {
                    grid[at] = 1;
                }
            }
            let mut ids = Vec::new();
            for &start in &present {
                let Some(first) = inside(start).filter(|&at| grid[at] == 1) else { continue };
                grid[first] = 2;
                let mut voxels = vec![start];
                let mut at = 0;
                while at < voxels.len() {
                    let c = voxels[at];
                    at += 1;
                    for i in 0..27 {
                        let n = c + IVec3::new(i % 3 - 1, (i / 3) % 3 - 1, i / 9 - 1);
                        if let Some(k) = inside(n).filter(|&k| grid[k] == 1) {
                            grid[k] = 2;
                            voxels.push(n);
                        }
                    }
                }
                let (mut first, mut last, mut middle) = (false, false, false);
                for c in &voxels {
                    let s = slice(c);
                    first |= s == 0;
                    last |= s == b - 1;
                    middle |= s == b / 2 || s == b / 2 - 1;
                }
                ids.push(push(&mut segments, Segment { tree: t, axis, first, last, middle, voxels }));
            }
            corners.push((t, corner, ids.clone()));
            blocks.insert((t, block), ids);
        }
    }
    // Joined: each block's segments to its neighbours' where they reach toward each other, as the plant grew them,
    // and logs of trees that touch.
    let mut joined = Joined((0..segments.len()).collect());
    let reaching = |ids: &[usize], segments: &[Segment], toward: IVec3| -> Option<usize> {
        let first = *ids.first()?;
        let (axis, way) = (segments[first].axis, toward[segments[first].axis]);
        ids.iter()
            .copied()
            .find(|&id| {
                let segment = &segments[id];
                match way {
                    w if w < 0 => segment.first,
                    w if w > 0 => segment.last,
                    _ => segment.middle,
                }
            })
            .filter(|_| axis < 3)
    };
    for ((t, block), ids) in &blocks {
        for i in 0..27 {
            let d = IVec3::new(i % 3 - 1, (i / 3) % 3 - 1, i / 9 - 1);
            if d == IVec3::ZERO {
                continue;
            }
            let other = (IVec3::from(*block) + d).to_array();
            let Some(theirs) = blocks.get(&(*t, other)) else { continue };
            if let (Some(a), Some(z)) = (reaching(ids, &segments, d), reaching(theirs, &segments, -d)) {
                joined.join(a, z);
            }
        }
    }
    // Logs of trees grown into each other join them where they touch: looked for only in the blocks of a tree within
    // a voxel of a block of another.
    let mut by_place: HashMap<IVec3, Vec<(usize, IVec3)>> = HashMap::new();
    for (t, corner, _) in &corners {
        by_place.entry(corner.div_euclid(IVec3::splat(b))).or_default().push((*t, *corner));
    }
    let mut bordering: Vec<usize> = Vec::new();
    for (t, corner, ids) in &corners {
        let place = corner.div_euclid(IVec3::splat(b));
        let close = (0..27).map(|i| place + IVec3::new(i % 3 - 1, (i / 3) % 3 - 1, i / 9 - 1)).any(|p| {
            by_place
                .get(&p)
                .is_some_and(|others| others.iter().any(|(o, c)| o != t && (*c - *corner).abs().max_element() <= b))
        });
        if close {
            bordering.extend(ids.iter().copied());
        }
    }
    let mut owner: HashMap<IVec3, usize> = HashMap::new();
    for &id in &bordering {
        for c in &segments[id].voxels {
            if let Some(&other) = owner.get(c) {
                joined.join(id, other);
            }
            owner.insert(*c, id);
        }
    }
    for &id in &bordering {
        for c in &segments[id].voxels {
            for i in (0..27).filter(|&i| i != 13) {
                let n = *c + IVec3::new(i % 3 - 1, (i / 3) % 3 - 1, i / 9 - 1);
                if let Some(&other) = owner.get(&n)
                    && segments[other].tree != segments[id].tree
                {
                    joined.join(id, other);
                }
            }
        }
    }
    // Held: what is joined to a log on the ground, or on a log beyond these.
    const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    let mut held_roots: HashSet<usize> = HashSet::new();
    // Without edits, voxels placed or a generator's Volume, nothing is solid but the ground under its top: only logs
    // within a voxel of that are asked about.
    let plain = edits.is_empty()
        && changes.kinds().is_empty()
        && volume.generator.as_ref().is_none_or(|generator| generator.world.volume().is_none());
    for (id, segment) in segments.iter().enumerate() {
        let resting = segment.voxels.iter().any(|c| {
            if plain && c.y > ground.height(c.x, c.z) + 1 {
                return false;
            }
            SIDES.iter().any(|side| {
                let n = *c + *side;
                !logs.contains(&n) && solid(&mut ground, n)
            })
        });
        if resting {
            held_roots.insert(joined.root(id));
        }
    }
    // The rest in pieces, each its number from one; a chip of a few voxels left between two cuts is dug out.
    let mut piece_of: HashMap<IVec3, u32> = HashMap::new();
    let mut numbers: HashMap<usize, u32> = HashMap::new();
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for id in 0..segments.len() {
        *sizes.entry(joined.root(id)).or_default() += segments[id].voxels.len();
    }
    let mut count = 0;
    for id in 0..segments.len() {
        let root = joined.root(id);
        if held_roots.contains(&root) {
            continue;
        }
        if sizes[&root] < CHIP {
            for c in &segments[id].voxels {
                changes.set(*c, None);
                logs.remove(c);
            }
            continue;
        }
        let number = *numbers.entry(root).or_insert_with(|| {
            count += 1;
            count
        });
        for c in &segments[id].voxels {
            piece_of.insert(*c, number);
        }
    }
    if count == 0 {
        return Vec::new();
    }
    // What each tree with a piece loses: its logs in pieces, and its leaves. A tree felled through its trunk, all
    // its logs over what it keeps lost, loses all its leaves over that; else each brick of its leaves goes with the
    // nearest of its logs.
    let mut voxels: HashMap<IVec3, (u32, [u8; 4], bool)> = HashMap::new();
    let losing: HashSet<usize> = segments
        .iter()
        .filter(|segment| segment.voxels.iter().any(|c| piece_of.contains_key(c)))
        .map(|s| s.tree)
        .collect();
    for (t, tree) in trees.iter().enumerate() {
        if !losing.contains(&t) {
            continue;
        }
        let mut own: Vec<([i32; 3], u8)> = Vec::new();
        woods.voxels_of(tree, |c, material| own.push((c, material)));
        let own_logs: Vec<(IVec3, u32)> = own
            .iter()
            .filter(|(c, material)| *material <= 2 && logs.contains(&IVec3::from(*c)))
            .map(|(c, _)| (IVec3::from(*c), piece_of.get(&IVec3::from(*c)).copied().unwrap_or(0)))
            .collect();
        let lost_logs = own_logs.iter().filter(|(_, label)| *label > 0);
        let Some(lowest_lost) = lost_logs.clone().map(|(c, _)| c.y).min() else { continue };
        let kept_logs_top = own_logs.iter().filter(|(_, label)| *label == 0).map(|(c, _)| c.y).max();
        let through = kept_logs_top.is_none_or(|top| top < lowest_lost);
        let mut most: HashMap<u32, usize> = HashMap::new();
        for (_, label) in lost_logs {
            *most.entry(*label).or_default() += 1;
        }
        let biggest = most.iter().max_by_key(|(label, n)| (**n, std::cmp::Reverse(**label))).map(|(label, _)| *label);
        // Its logs in buckets of a brick, a voxel in each two a side, for its leaves to find.
        let mut buckets: HashMap<IVec3, Vec<(IVec3, u32)>> = HashMap::new();
        let mut seen: HashSet<(IVec3, u32)> = HashSet::new();
        for (c, label) in &own_logs {
            if seen.insert((c.div_euclid(IVec3::splat(2)), *label)) {
                buckets.entry(c.div_euclid(IVec3::splat(8))).or_default().push((*c, *label));
            }
        }
        let colour = |m: u8| trees::colour_of(&volume.trees, Woods::kind_of(tree.model), m);
        let mut leaf_bricks: HashMap<IVec3, Option<u32>> = HashMap::new();
        let (mut kept_top, mut lost_top) = (i32::MIN, i32::MIN);
        let mut lost: Vec<IVec3> = Vec::new();
        for (c, material) in own {
            let c = IVec3::from(c);
            let label = if material <= 2 {
                if !logs.contains(&c) {
                    continue;
                }
                piece_of.get(&c).copied()
            } else {
                if !there(&mut ground, c) {
                    continue;
                }
                match through && kept_logs_top.is_none_or(|top| c.y > top) {
                    true => biggest,
                    false => {
                        let brick = c.div_euclid(IVec3::splat(8));
                        *leaf_bricks.entry(brick).or_insert_with(|| nearest(&buckets, (brick * 8).as_vec3() + 4.0))
                    }
                }
                .filter(|&label| label > 0)
            };
            match label {
                Some(label) => {
                    voxels.entry(c).or_insert((label, colour(material), material <= 2));
                    lost.push(c);
                    lost_top = lost_top.max(c.y);
                }
                None => kept_top = kept_top.max(c.y),
            }
        }
        // Felled from over the highest voxel it keeps, where all it loses is not under that; what it loses under
        // that is dug out.
        let cut = if kept_top == i32::MIN { tree.least[1] } else { kept_top + 1 };
        let (least, most) = woods.bounds(tree);
        if cut <= lost_top && cut < tree.cut {
            changes.fell(tree.foot, Felled { cut, low: least, high: most });
        }
        for c in lost.into_iter().filter(|c| c.y < cut) {
            // Not where another tree, which stands, has a voxel too.
            let shared = near.iter().any(|other| other != tree && woods.at(other, c.to_array()) != AIR);
            if !shared {
                changes.set(c, None);
            }
        }
    }
    // The pieces, each from the least corner of its voxels.
    let mut pieces: Vec<(IVec3, IVec3, Vec<(IVec3, [u8; 4], bool)>)> =
        (0..count).map(|_| (IVec3::MAX, IVec3::MIN, Vec::new())).collect();
    for (c, (label, colour, wood)) in voxels {
        let piece = &mut pieces[(label - 1) as usize];
        piece.0 = piece.0.min(c);
        piece.1 = piece.1.max(c);
        piece.2.push((c, colour, wood));
    }
    pieces
        .into_iter()
        .filter(|(_, _, voxels)| voxels.iter().any(|(_, _, wood)| *wood))
        .map(|(least, most, mut found)| {
            found.sort_by_key(|(c, ..)| [c.z, c.y, c.x]);
            let mut palette: Vec<[u8; 4]> = Vec::new();
            let mut wood: Vec<bool> = Vec::new();
            let voxels = found
                .into_iter()
                .map(|(c, colour, is_wood)| {
                    let place = match palette.iter().zip(&wood).position(|(p, w)| *p == colour && *w == is_wood) {
                        Some(at) => at,
                        None => {
                            palette.push(colour);
                            wood.push(is_wood);
                            palette.len() - 1
                        }
                    };
                    ((c - least).to_array().map(|a| a as u16), (place + 1).min(255) as u8)
                })
                .collect();
            let size = (most - least + 1).to_array().map(|a| a as u32);
            VoxelPiece { low: least, size, voxels, palette, wood, voxel }
        })
        .collect()
}

/// Stamps `piece` back into `volume`'s `changes` where it lies: its least
/// corner's place and turn given by `pose`, from metres of the piece's box
/// to metres from the volume's corner. Each voxel of the volume whose
/// middle falls in one of the piece's is placed in that voxel's colour,
/// where nothing solid is already. Gives how many were placed.
pub fn stamp(volume: &VoxelVolume, changes: &mut VoxelChanges, piece: &VoxelPiece, pose: Mat4) -> usize {
    let voxel = 1.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
    let cells: HashMap<[u16; 3], u8> = piece.voxels.iter().copied().collect();
    // The box it lies in, in the volume's voxels.
    let size = Vec3::from(piece.size.map(|a| a as f32)) * piece.voxel;
    let corners = (0..8).map(|i| {
        let pick = |bit: u32, a: f32| if i >> bit & 1 == 0 { 0.0 } else { a };
        pose.transform_point3(Vec3::new(pick(0, size.x), pick(1, size.y), pick(2, size.z)))
    });
    let (least, most) = corners.fold((Vec3::MAX, Vec3::MIN), |(l, m), c| (l.min(c), m.max(c)));
    let (low, high) = ((least / voxel).floor().as_ivec3(), (most / voxel).floor().as_ivec3());
    let tall = (volume.size.y / voxel).ceil() as i32;
    let (low, high) = (low.max(IVec3::new(i32::MIN, 1, i32::MIN)), high.min(IVec3::new(i32::MAX, tall - 1, i32::MAX)));
    if low.cmpgt(high).any() {
        return 0;
    }
    let span = high - low + 1;
    let mut solid = vec![false; (span.x * span.y * span.z) as usize];
    if let Some(solids) =
        VoxelSolids::of(&VoxelVolume { changes: std::sync::Arc::new(changes.clone()), ..volume.clone() })
    {
        solids.fill(low.to_array(), span.to_array(), &mut solid);
    }
    let back = pose.inverse();
    let mut placed = 0;
    for z in 0..span.z {
        for y in 0..span.y {
            for x in 0..span.x {
                if solid[(x + span.x * (y + span.y * z)) as usize] {
                    continue;
                }
                let c = low + IVec3::new(x, y, z);
                let local = back.transform_point3((c.as_vec3() + 0.5) * voxel) / piece.voxel;
                if local.cmplt(Vec3::ZERO).any() {
                    continue;
                }
                let cell = local.floor().as_uvec3();
                if (0..3).any(|a| cell[a] >= piece.size[a]) {
                    continue;
                }
                let Some(&place) = cells.get(&cell.to_array().map(|a| a as u16)) else { continue };
                let [r, g, b, _] = piece.palette[usize::from(place) - 1];
                let kind = VoxelKind { material: VoxelMaterial::Dirt, tint: Some([r, g, b]) };
                placed += usize::from(changes.set(c, Some(kind)));
            }
        }
    }
    placed
}

/// The middle of the trunk of each tree of `volume` standing whole whose
/// foot is in the box from `low` to `high` (in metres from the volume's
/// corner, across X and Z), where it leaves the ground: what a cut through
/// its trunk aims at.
pub fn tree_trunks(volume: &VoxelVolume, low: Vec3, high: Vec3) -> Vec<Vec3> {
    let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
    let Some(woods) = trees::woods(&volume.trees, block) else { return Vec::new() };
    let mut ground = Ground::new(volume, Some(&woods));
    let voxel = ground.voxel;
    let (least, most) = ((low / voxel).floor().as_ivec3(), (high / voxel).floor().as_ivec3());
    let mut trunks = Vec::new();
    for tree in ground.trees_near(IVec3::new(least.x, 0, least.z), IVec3::new(most.x, ground.tall, most.z)) {
        let inside = (least.x..=most.x).contains(&tree.foot[0]) && (least.z..=most.z).contains(&tree.foot[1]);
        if !inside || tree.cut != trees::NOT_CUT {
            continue;
        }
        let h = ground.height(tree.foot[0], tree.foot[1]);
        let b = woods.block as f32;
        let middle = match woods.log_blocks(&tree).get(&[0, 0, 0]) {
            Some((corner, _)) => [corner[0] as f32 + b * 0.5, corner[2] as f32 + b * 0.5],
            None => [tree.foot[0] as f32 + 0.5, tree.foot[1] as f32 + 0.5],
        };
        trunks.push(Vec3::new(middle[0], (h + 1) as f32, middle[1]) * voxel);
    }
    trunks
}

/// How high the grass and cover of voxel column (`x`, `z`) stands over the
/// ground's top `h`, in voxels, as a ray meeting cover finds it: as tall as
/// its kind grows where its ground grows any, and as much of that as a cut
/// left. None where nothing grows.
pub(super) fn cover_over(ground: &mut Ground, x: i32, z: i32) -> Option<(i32, i32)> {
    let volume = ground.volume;
    let voxel = ground.voxel;
    let (h, kind) = ground.kinds_at(&[[x, z]]).pop().flatten()?;
    let covering = volume.cover.iter().find(|cover| {
        cover.on.iter().any(|material| super::growth::ground_kind(*material) == Some(kind)) && cover.density > 0.0
    });
    let tall = match covering {
        Some(cover) => cover.height,
        None if kind == GRASS => volume.ground.map_or(0.0, |g| if g.grass > 0.0 { g.grass_height } else { 0.0 }),
        None => 0.0,
    };
    let left = volume.changes.cut_left(x, z).map_or(1.0, |left| f32::from(left) / f32::from(super::changes::FULL));
    let blades = (tall * left / voxel).round() as i32;
    (blades > 0).then_some((h, blades))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_terrain::blocks::plant::PlantDef;

    use super::*;
    use crate::{VoxelGround, VoxelTrees};

    /// Flat ground two metres up, an oak in most cells.
    fn wood() -> VoxelVolume {
        let oak = PlantDef { height: [8.0, 10.0], crown_width: [6.0, 8.0], ..PlantDef::default() };
        let trees = vec![VoxelTrees {
            plant: Arc::new(oak),
            per_hectare: 300.0,
            wood: [100, 70, 50],
            leaves: [70, 120, 40],
            areas: Default::default(),
        }];
        let ground = VoxelGround { height: 2.0, hills: 0.0, ..VoxelGround::default() };
        VoxelVolume {
            endless: true,
            size: Vec3::new(16.0, 32.0, 16.0),
            ground: Some(ground),
            trees,
            ..VoxelVolume::default()
        }
    }

    /// A tree standing in `volume`, and the middle of its trunk where it leaves the ground, in metres from the
    /// volume's corner: its plant's foot block, as the tree stands on the bricks' grid.
    fn a_tree(volume: &VoxelVolume) -> (Tree, Vec3) {
        let woods = trees::woods(&volume.trees, 16).expect("trees");
        let mut ground = Ground::new(volume, Some(&woods));
        let trees = ground.trees_near(IVec3::new(0, 0, 0), IVec3::new(320, 300, 320));
        let tree = *trees.iter().find(|tree| tree.least[0] > 32 && tree.least[2] > 32).expect("a tree");
        let (corner, _) = woods.log_blocks(&tree)[&[0, 0, 0]];
        let h = ground.height(tree.foot[0], tree.foot[1]);
        let foot = Vec3::new(corner[0] as f32 + 8.0, (h + 1) as f32, corner[2] as f32 + 8.0) / 16.0;
        (tree, foot)
    }

    #[test]
    fn a_trunk_cut_through_fells_the_tree_over_its_stump_and_its_logs_and_leaves_come_loose() {
        let mut volume = wood();
        let (tree, foot) = a_tree(&volume);
        let woods = trees::woods(&volume.trees, 16).unwrap();
        // Its logs over the cut, which all come loose.
        let over = {
            let mut n = 0;
            let (least, most) = woods.bounds(&tree);
            woods.logs_in(&tree, [least[0], ((foot.y + 1.6) * 16.0) as i32, least[2]], most, |_| n += 1);
            n
        };
        let trees = [tree];
        // A ball through the trunk, half a metre up.
        let at = foot + Vec3::Y * 0.8;
        let (changes, done) = cut(&volume, &VoxelCut::Ball { at, radius: 0.8 });
        assert!(done.voxels > 0, "{done:?}");
        let sizes: Vec<usize> = done.pieces.iter().map(|p| p.logs()).collect();
        assert_eq!(done.pieces.len(), 1, "one piece: {sizes:?}, {} voxels cut, tree {tree:?}", done.voxels);
        let piece = &done.pieces[0];
        assert!(piece.logs() >= over, "{} logs, {over} over the cut", piece.logs());
        assert!(piece.voxels.len() > piece.logs(), "with its leaves");
        // Kept as felled over the kerf, only the kerf dug out a voxel at a time.
        let felled = changes.felled(tree.foot).expect("felled");
        assert!(felled.cut as f32 / 16.0 <= at.y + 0.85, "{felled:?}");
        assert!(changes.count() < done.voxels as u64 + 2000, "{} voxels set, {} cut", changes.count(), done.voxels);
        // Over the cut nothing of it stands; under the cut its stump does.
        volume.changes = Arc::new(changes);
        let mut after = Ground::new(&volume, Some(&woods));
        let stood = after.trees_near(IVec3::from(tree.least), IVec3::from(tree.least));
        let now = stood.iter().find(|t| t.foot == tree.foot).expect("the stump stands");
        let top = IVec3::new(tree.foot[0], felled.cut + 20, tree.foot[1]);
        assert_eq!(after.kind(top, &stood, &[]), AIR);
        assert_eq!(woods.at(&trees[0], top.to_array()), LOG, "the trunk was there");
        let stump = IVec3::new(tree.foot[0], ((foot.y + 0.1) * 16.0) as i32, tree.foot[1]);
        assert_eq!(after.kind(stump, &stood, &[]), LOG, "{now:?}");
        // The grass round it is cut.
        assert!(done.columns > 100, "{done:?}");
    }

    #[test]
    fn a_piece_is_boxes_of_its_logs_and_stamps_back_where_it_lies() {
        let volume = wood();
        let (_, foot) = a_tree(&volume);
        let (changes, done) = cut(&volume, &VoxelCut::Ball { at: foot + Vec3::Y * 0.8, radius: 0.8 });
        let piece = &done.pieces[0];
        let boxes = piece.boxes(256);
        assert!(!boxes.is_empty() && boxes.len() <= 256, "{}", boxes.len());
        let volume_of =
            |boxes: &[(Vec3, Vec3)]| boxes.iter().map(|(_, half)| half.x * half.y * half.z * 8.0).sum::<f32>();
        let logs = piece.logs() as f32 * piece.voxel.powi(3);
        assert!(volume_of(&piece.boxes(100_000)) - logs < 1e-3, "exactly the logs at one a voxel");
        // Laid on its side, high over the ground: every voxel of it is placed, turned a quarter.
        let volume = VoxelVolume { changes: Arc::new(changes.clone()), ..volume };
        let pose = Mat4::from_translation(foot + Vec3::new(1.0, 12.0, 0.0))
            * Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let mut stamped = changes.clone();
        let placed = stamp(&volume, &mut stamped, piece, pose);
        assert!(placed as f32 > piece.voxels.len() as f32 * 0.97, "{placed} of {}", piece.voxels.len());
        assert!(stamped.kinds().len() >= 2 && stamped.kinds().len() <= 5, "its colours");
    }

    #[test]
    fn a_branch_cut_off_comes_loose_and_the_tree_stands_unfelled() {
        let volume = wood();
        let (tree, _) = a_tree(&volume);
        let woods = trees::woods(&volume.trees, 16).unwrap();
        // A log far out from the trunk: the end of a branch.
        let (least, most) = woods.bounds(&tree);
        let mut logs = Vec::new();
        woods.logs_in(&tree, least, most, |c| logs.push(IVec3::from(c)));
        let trunk = IVec3::new(tree.foot[0], 0, tree.foot[1]);
        let far = |c: &IVec3| (c.x - trunk.x).abs().max((c.z - trunk.z).abs());
        let end = *logs.iter().max_by_key(|c| far(c)).expect("logs");
        assert!(far(&end) > 24, "a branch reaches {} voxels out", far(&end));
        // Cut a little way in from its end along the way it reaches.
        let toward = (end - trunk).as_vec3() * Vec3::new(1.0, 0.0, 1.0);
        let at = (end.as_vec3() + 0.5 - toward.normalize() * 10.0) / 16.0;
        let (changes, done) = cut(&volume, &VoxelCut::Ball { at, radius: 0.25 });
        if done.pieces.is_empty() {
            // The ball missed the branch's log: not this test's to find.
            assert!(done.voxels == 0 || changes.felled(tree.foot).is_none());
            return;
        }
        assert!(changes.felled(tree.foot).is_none(), "the tree stands");
        let piece = &done.pieces[0];
        assert!(piece.logs() < logs.len() / 2, "a branch, not the tree: {} of {}", piece.logs(), logs.len());
    }
}
