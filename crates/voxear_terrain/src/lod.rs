//! Levels of detail: which patches of the ground to draw, and how finely.
//!
//! The terrain is a quadtree of nodes. A node at level 0 is `LEAF_QUADS`
//! quads across, and each level up is twice as wide, up to a root as wide
//! as the terrain. Each level is drawn out to a distance from the camera,
//! each twice as far as the level below, and its grid is always
//! `PATCH_QUADS` quads a patch, so far ground is drawn as coarsely as it is
//! seen. A node is drawn as its four quarters, its patches; where only some
//! of a node's children are near enough for a finer level, its other
//! quarters are drawn at its own.
//!
//! Over the last stretch of each level's distance its vertices slide onto
//! the next level's coarser grid (a geomorph), so a level meets the next
//! without cracks or a visible pop. What decides it is a vertex's own
//! distance from the camera, in the vertex shader; this module gives each
//! level the distances it slides over.

use voxear_core::math::Vec3;

use crate::heights::{Heightmap, LEAF_QUADS, LEAVES};

/// How many quads a patch's grid is across.
pub const PATCH_QUADS: u32 = 32;

/// How much of each level's stretch, nearest its far end, its vertices
/// slide onto the next level's grid over.
const MORPH_SHARE: f32 = 0.3;

/// The lowest and highest heights of every node at every level.
#[derive(Clone, Debug)]
pub struct LodTree {
    levels: Vec<Level>,
    quads: [u32; 2],
    spacing: f32,
}

#[derive(Clone, Debug)]
struct Level {
    nodes: [u32; 2],
    ranges: Vec<[f32; 2]>,
}

impl LodTree {
    pub fn new(map: &Heightmap) -> Self {
        let quads = map.quads();
        let leaves = [quads[0] / LEAF_QUADS, quads[1] / LEAF_QUADS];
        let mut ranges = vec![[0.0; 2]; (leaves[0] * leaves[1]) as usize];
        for tz in 0..map.tiles_z() {
            for tx in 0..map.tiles_x() {
                let tile = map.tile(tx, tz);
                for lz in 0..LEAVES {
                    for lx in 0..LEAVES {
                        let (x, z) = (tx * LEAVES + lx, tz * LEAVES + lz);
                        ranges[(z * leaves[0] + x) as usize] = tile.leaf_range(lx, lz);
                    }
                }
            }
        }
        let mut levels = vec![Level { nodes: leaves, ranges }];
        while levels.last().is_some_and(|level| level.nodes[0] > 1 || level.nodes[1] > 1) {
            let below = levels.last().expect("a level");
            let nodes = [below.nodes[0].div_ceil(2), below.nodes[1].div_ceil(2)];
            let mut ranges = vec![[f32::MAX, f32::MIN]; (nodes[0] * nodes[1]) as usize];
            for z in 0..below.nodes[1] {
                for x in 0..below.nodes[0] {
                    let [low, high] = below.ranges[(z * below.nodes[0] + x) as usize];
                    let range = &mut ranges[((z / 2) * nodes[0] + x / 2) as usize];
                    *range = [range[0].min(low), range[1].max(high)];
                }
            }
            levels.push(Level { nodes, ranges });
        }
        Self { levels, quads, spacing: map.spacing() }
    }

    /// How many levels there are, the root's the last.
    pub fn levels(&self) -> usize {
        self.levels.len()
    }

    /// How many metres across a node at level 0 is.
    pub fn leaf_size(&self) -> f32 {
        LEAF_QUADS as f32 * self.spacing
    }

    /// A node's box, in metres from the terrain's corner; `None` for a node
    /// past the terrain's edge.
    fn bounds(&self, level: usize, x: u32, z: u32) -> Option<(Vec3, Vec3)> {
        let Level { nodes, ranges } = &self.levels[level];
        if x >= nodes[0] || z >= nodes[1] {
            return None;
        }
        let [low, high] = ranges[(z * nodes[0] + x) as usize];
        let size = (LEAF_QUADS << level) as f32 * self.spacing;
        let [w, l] = [self.quads[0] as f32 * self.spacing, self.quads[1] as f32 * self.spacing];
        let min = Vec3::new(x as f32 * size, low, z as f32 * size);
        let max = Vec3::new(((x + 1) as f32 * size).min(w), high, ((z + 1) as f32 * size).min(l));
        Some((min, max))
    }
}

/// How far each level is drawn to, and over what distances its vertices
/// slide onto the next level's grid: `[start, end]`, in metres from the
/// camera. The root's goes on for ever.
#[derive(Clone, Debug, PartialEq)]
pub struct LodRanges {
    pub morph: Vec<[f32; 2]>,
}

impl LodRanges {
    /// Ranges for `levels` levels, the first out to `detail_distance`
    /// metres, or twice a level-0 node's width if that is further, each
    /// level after twice as far.
    pub fn new(detail_distance: f32, leaf_size: f32, levels: usize) -> Self {
        let first = detail_distance.max(leaf_size * 2.0);
        let mut morph = Vec::with_capacity(levels);
        let mut near = 0.0;
        for level in 0..levels {
            if level + 1 == levels {
                morph.push([f32::MAX, f32::MAX]);
                break;
            }
            let far = first * (1u64 << level) as f32;
            morph.push([far - (far - near) * MORPH_SHARE, far]);
            near = far;
        }
        Self { morph }
    }

    /// How far level `level` is drawn to: for ever for the last, and any
    /// past the levels these ranges are for.
    pub fn end(&self, level: usize) -> f32 {
        match self.morph.get(level) {
            Some(range) if level + 1 < self.morph.len() => range[1],
            _ => f32::MAX,
        }
    }
}

/// A square of the ground to draw: its corner, in metres from the
/// terrain's, how wide it is, and its level, whose grid is `PATCH_QUADS`
/// quads across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    pub origin: [f32; 2],
    pub size: f32,
    pub level: u32,
}

/// What to draw of a terrain seen from `camera`, in metres from its
/// corner: its patches within `planes` (inward, `n · p + d >= 0`, in the
/// terrain's own space too), each as finely as `ranges` say for how far
/// they are, added to `out`.
pub fn select(tree: &LodTree, camera: Vec3, ranges: &LodRanges, planes: &[[f32; 4]], out: &mut Vec<Patch>) {
    let root = tree.levels() - 1;
    let selector = Selector { tree, camera, ranges, planes };
    let nodes = tree.levels[root].nodes;
    for z in 0..nodes[1] {
        for x in 0..nodes[0] {
            if let Picked::Outside = selector.pick(root, x, z, out) {
                // Only the root's range goes on for ever, but a tree too shallow for its ranges draws its root.
                selector.whole(root, x, z, out);
            }
        }
    }
}

enum Picked {
    /// Past the level's distance: the node above draws its quarter.
    Outside,
    /// Drawn, or out of view.
    Done,
}

struct Selector<'a> {
    tree: &'a LodTree,
    camera: Vec3,
    ranges: &'a LodRanges,
    planes: &'a [[f32; 4]],
}

impl Selector<'_> {
    fn pick(&self, level: usize, x: u32, z: u32, out: &mut Vec<Patch>) -> Picked {
        let Some((min, max)) = self.tree.bounds(level, x, z) else { return Picked::Done };
        if !self.near(min, max, self.ranges.end(level)) {
            return Picked::Outside;
        }
        if !self.in_view(min, max) {
            return Picked::Done;
        }
        if level == 0 || !self.near(min, max, self.ranges.end(level - 1)) {
            self.whole(level, x, z, out);
            return Picked::Done;
        }
        for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            if let Picked::Outside = self.pick(level - 1, x * 2 + cx, z * 2 + cz, out) {
                // The child's square, drawn at this level: one of this node's quarters.
                self.quarter(level, x, z, cx, cz, out);
            }
        }
        Picked::Done
    }

    /// A node's four quarters at its own level.
    fn whole(&self, level: usize, x: u32, z: u32, out: &mut Vec<Patch>) {
        for (cx, cz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            self.quarter(level, x, z, cx, cz, out);
        }
    }

    /// Quarter `(cx, cz)` of node `(x, z)` at `level`, drawn at that level,
    /// unless it is past the terrain's edge or out of view.
    fn quarter(&self, level: usize, x: u32, z: u32, cx: u32, cz: u32, out: &mut Vec<Patch>) {
        let spacing = self.tree.spacing;
        let size = (PATCH_QUADS << level) as f32 * spacing;
        let origin = [(x * 2 + cx) as f32 * size, (z * 2 + cz) as f32 * size];
        let [w, l] = [self.tree.quads[0] as f32 * spacing, self.tree.quads[1] as f32 * spacing];
        if origin[0] >= w || origin[1] >= l {
            return;
        }
        // Above level 0 a quarter is a node of the level below, whose heights are known more closely.
        let heights = match level {
            0 => self.tree.bounds(0, x, z),
            _ => self.tree.bounds(level - 1, x * 2 + cx, z * 2 + cz),
        };
        let Some((min, max)) = heights else { return };
        let corner = Vec3::new(origin[0], min.y, origin[1]);
        let far = Vec3::new((origin[0] + size).min(w), max.y, (origin[1] + size).min(l));
        if self.in_view(corner, far) {
            out.push(Patch { origin, size, level: level as u32 });
        }
    }

    /// Whether any of the box is within `distance` of the camera.
    fn near(&self, min: Vec3, max: Vec3, distance: f32) -> bool {
        if distance == f32::MAX {
            return true;
        }
        self.camera.clamp(min, max).distance_squared(self.camera) <= distance * distance
    }

    fn in_view(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|&[x, y, z, d]| {
            let corner = Vec3::new(
                if x >= 0.0 { max.x } else { min.x },
                if y >= 0.0 { max.y } else { min.y },
                if z >= 0.0 { max.z } else { min.z },
            );
            x * corner.x + y * corner.y + z * corner.z + d >= 0.0
        })
    }
}

/// `planes`, inward in the world, as they are in the space of a terrain
/// whose corner is at `origin`.
pub fn planes_from(planes: &[[f32; 4]], origin: Vec3) -> Vec<[f32; 4]> {
    planes.iter().map(|&[x, y, z, d]| [x, y, z, d + x * origin.x + y * origin.y + z * origin.z]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(patches: &[Patch]) -> f32 {
        patches.iter().map(|patch| patch.size * patch.size).sum()
    }

    #[test]
    fn a_terrain_far_away_is_drawn_whole_at_its_coarsest() {
        let map = Heightmap::flat(4, 4, 1.0, 0.0);
        let tree = LodTree::new(&map);
        assert_eq!(tree.levels(), 5);
        let ranges = LodRanges::new(64.0, tree.leaf_size(), tree.levels());
        let mut patches = Vec::new();
        select(&tree, Vec3::new(-50_000.0, 0.0, -50_000.0), &ranges, &[], &mut patches);
        assert_eq!(patches.len(), 4);
        assert!(patches.iter().all(|patch| patch.level == 4));
        assert_eq!(area(&patches), 1024.0 * 1024.0);
    }

    #[test]
    fn near_the_camera_the_ground_is_finest_and_every_metre_is_drawn_once() {
        let map = Heightmap::flat(4, 4, 0.5, 0.0);
        let tree = LodTree::new(&map);
        let ranges = LodRanges::new(64.0, tree.leaf_size(), tree.levels());
        let mut patches = Vec::new();
        let camera = Vec3::new(100.0, 2.0, 300.0);
        select(&tree, camera, &ranges, &[], &mut patches);
        let [w, l] = map.size();
        assert!((area(&patches) - w * l).abs() < 1.0, "{} of {}", area(&patches), w * l);
        let under = patches.iter().find(|patch| {
            (patch.origin[0]..patch.origin[0] + patch.size).contains(&camera.x)
                && (patch.origin[1]..patch.origin[1] + patch.size).contains(&camera.z)
        });
        assert_eq!(under.map(|patch| patch.level), Some(0));
        // No two patches overlap.
        for (i, a) in patches.iter().enumerate() {
            for b in &patches[i + 1..] {
                let apart = a.origin[0] + a.size <= b.origin[0]
                    || b.origin[0] + b.size <= a.origin[0]
                    || a.origin[1] + a.size <= b.origin[1]
                    || b.origin[1] + b.size <= a.origin[1];
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn patches_beside_each_other_are_at_most_a_level_apart() {
        let map = Heightmap::flat(8, 8, 0.5, 0.0);
        let tree = LodTree::new(&map);
        let ranges = LodRanges::new(40.0, tree.leaf_size(), tree.levels());
        let mut patches = Vec::new();
        select(&tree, Vec3::new(333.0, 1.0, 512.0), &ranges, &[], &mut patches);
        for a in &patches {
            for b in &patches {
                let touch = a.origin[0] <= b.origin[0] + b.size
                    && b.origin[0] <= a.origin[0] + a.size
                    && a.origin[1] <= b.origin[1] + b.size
                    && b.origin[1] <= a.origin[1] + a.size;
                if touch {
                    assert!(a.level.abs_diff(b.level) <= 1, "{a:?} beside {b:?}");
                }
            }
        }
    }

    #[test]
    fn what_is_out_of_view_is_left_out() {
        let map = Heightmap::flat(4, 4, 1.0, 0.0);
        let tree = LodTree::new(&map);
        let ranges = LodRanges::new(64.0, tree.leaf_size(), tree.levels());
        let mut all = Vec::new();
        let camera = Vec3::new(512.0, 10.0, 512.0);
        select(&tree, camera, &ranges, &[], &mut all);
        // Only what is past x = 600.
        let mut some = Vec::new();
        select(&tree, camera, &ranges, &planes_from(&[[1.0, 0.0, 0.0, -600.0]], Vec3::ZERO), &mut some);
        assert!(!some.is_empty() && some.len() < all.len());
        assert!(some.iter().all(|patch| patch.origin[0] + patch.size > 600.0));
        // The same plane, for a terrain whose corner is 100 m along: only what is past 500 m of its own.
        let mut moved = Vec::new();
        select(&tree, camera, &ranges, &planes_from(&[[1.0, 0.0, 0.0, -600.0]], Vec3::X * 100.0), &mut moved);
        assert!(moved.iter().all(|patch| patch.origin[0] + patch.size > 500.0));
        assert!(moved.len() > some.len());
    }

    #[test]
    fn each_level_slides_onto_the_next_by_its_far_end() {
        let ranges = LodRanges::new(50.0, 16.0, 4);
        assert_eq!(ranges.morph[0], [35.0, 50.0]);
        assert_eq!(ranges.morph[1], [85.0, 100.0]);
        assert_eq!(ranges.morph[2], [170.0, 200.0]);
        assert_eq!(ranges.end(3), f32::MAX);
        // Never nearer than twice a leaf, so a node and its neighbours stay within a level.
        assert_eq!(LodRanges::new(1.0, 32.0, 3).end(0), 64.0);
    }

    #[test]
    fn a_terrain_not_a_power_of_two_across_draws_only_itself() {
        let map = Heightmap::flat(3, 1, 1.0, 0.0);
        let tree = LodTree::new(&map);
        let ranges = LodRanges::new(64.0, tree.leaf_size(), tree.levels());
        let mut patches = Vec::new();
        select(&tree, Vec3::new(400.0, 5.0, 100.0), &ranges, &[], &mut patches);
        let [w, _] = map.size();
        assert!(patches.iter().all(|patch| patch.origin[0] < w && patch.origin[1] < 256.0));
    }
}
