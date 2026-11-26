//! What an endless Voxel Volume's voxels are, worked out on the CPU as the
//! GPU makes them (`shaders/voxel_world.slang`), to the voxel: its own
//! ground's hills, or its generator's height, or its generator's Volume
//! with its overhangs and caves; the floor solid; its trees' logs and
//! leaves, placed by the same hashes on the same ground; and its edits over
//! that, in order, the Voxel Prints printed into it with them. Its grass
//! and flowers are left out. Every machine finds the same, as an edit sent
//! over the network wants.
//!
//! Rays ([`super::ray`]) and collision ([`VoxelSolids`]) both ask it.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use voxear_core::math::{IVec3, Vec3};

use super::{
    MOST_VOXELS_A_METRE, VoxelGround, VoxelVolume,
    trees::{self, AIR, GROUND, LOG, Tree, Woods},
};

/// How far a ray's step looks for trees, in voxels.
const TREE_LOOK: f32 = 96.0;

/// What the ground of an endless volume is, voxel by voxel, with what was
/// worked out kept.
pub(super) struct Ground<'a> {
    pub volume: &'a VoxelVolume,
    pub voxel: f32,
    pub tall: i32,
    woods: Option<&'a Woods>,
    /// How many voxels tall the GPU's columns are, which a tree's foot must
    /// stand under.
    rows: i32,
    seed: u32,
    heights: HashMap<(i32, i32), i32>,
    /// What a generator's Volume's columns carry up to their cells.
    columns: HashMap<(i32, i32), Vec<f32>>,
    /// The tree each cell holds, where it has one.
    cells: HashMap<(i32, i32), Option<Tree>>,
    /// Trees placed kept between one ground and the next of the same volume.
    shared: Option<&'a Mutex<Cells>>,
}

/// The tree each cell holds, where it has one.
type Cells = HashMap<(i32, i32), Option<Tree>>;

impl<'a> Ground<'a> {
    pub fn new(volume: &'a VoxelVolume, woods: Option<&'a Woods>) -> Self {
        let voxel = 1.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
        let tall = (volume.size.y / voxel).ceil() as i32;
        let sector = 64.0 * voxel;
        let rows = ((volume.size.y.max(sector) / sector).ceil() as i32).clamp(1, 16) * 64;
        let seed = volume.ground.map_or(VoxelGround::default().seed, |ground| ground.seed);
        let (heights, columns, cells) = (HashMap::new(), HashMap::new(), HashMap::new());
        Self { volume, voxel, tall, woods, rows, seed, heights, columns, cells, shared: None }
    }

    /// The ground's height at voxel column (`x`, `z`), in voxels over the
    /// floor: its top voxel.
    pub fn height(&mut self, x: i32, z: i32) -> i32 {
        if let Some(&h) = self.heights.get(&(x, z)) {
            return h;
        }
        let v = self.voxel;
        let (mx, mz) = ((x as f32 + 0.5) * v, (z as f32 + 0.5) * v);
        let h = match (&self.volume.generator, &self.volume.ground) {
            (Some(generator), _) => {
                let height = generator.world.height_at(mx + generator.at[0], mz + generator.at[1]);
                ((height - generator.floor) / v).floor() as i32
            }
            (None, Some(ground)) => (ground.height_at(mx, mz) / v).floor() as i32,
            (None, None) => -1,
        };
        self.heights.insert((x, z), h);
        h
    }

    /// Works out the heights of the voxel columns from (`x`, `z`), `count`
    /// a side, at once: a generator's heights run as one row.
    pub fn heights_for(&mut self, x: i32, z: i32, count: [i32; 2]) {
        let Some(generator) = self.volume.generator.clone() else { return };
        let v = self.voxel;
        let (mut xs, mut zs, mut keys) = (Vec::new(), Vec::new(), Vec::new());
        for j in 0..count[1] {
            for i in 0..count[0] {
                if !self.heights.contains_key(&(x + i, z + j)) {
                    xs.push((x as f32 + i as f32 + 0.5) * v + generator.at[0]);
                    zs.push((z as f32 + j as f32 + 0.5) * v + generator.at[1]);
                    keys.push((x + i, z + j));
                }
            }
        }
        let mut heights = vec![0.0; xs.len()];
        generator.world.heights(&xs, &zs, &mut heights);
        for (key, height) in keys.into_iter().zip(heights) {
            self.heights.insert(key, ((height - generator.floor) / v).floor() as i32);
        }
    }

    /// What a generator's Volume carries up voxel column (`x`, `z`): none
    /// without one.
    fn carried(&mut self, x: i32, z: i32) -> Option<&Vec<f32>> {
        let generator = self.volume.generator.as_ref()?;
        let volume = generator.world.volume()?;
        let v = self.voxel;
        let (gx, gz) = ((x as f32 + 0.5) * v + generator.at[0], (z as f32 + 0.5) * v + generator.at[1]);
        Some(self.columns.entry((x, z)).or_insert_with(|| {
            let height = generator.world.height_at(gx, gz);
            volume.carried(&[gx], &[gz], &[height]).rows().iter().map(|row| row[0]).collect()
        }))
    }

    /// How far into a generator's Volume each voxel of column (`x`, `z`)
    /// from `y` up is, `count` of them, in metres (out of it where
    /// negative): none without one.
    pub fn densities(&mut self, x: i32, y: i32, z: i32, count: usize) -> Option<Vec<f32>> {
        let carried = self.carried(x, z)?.clone();
        let generator = self.volume.generator.as_ref()?;
        let volume = generator.world.volume()?;
        let v = self.voxel;
        let (gx, gz) = ((x as f32 + 0.5) * v + generator.at[0], (z as f32 + 0.5) * v + generator.at[1]);
        let rows: Vec<&[f32]> = carried.iter().map(std::slice::from_ref).collect();
        let ys: Vec<f32> = (0..count).map(|i| (y as f32 + i as f32 + 0.5) * v + generator.floor).collect();
        let (mut density, mut ground) = (vec![0.0; count], vec![0.0; count]);
        let cells = voxear_terrain::generate::Cells { x: &[gx], y: &ys, z: &[gz], carried: &rows, size: v };
        volume.densities(cells, false, &mut density, &mut ground);
        Some(density)
    }

    /// How far into a generator's Volume voxel `c` is, in metres: none
    /// without one.
    pub fn density(&mut self, c: IVec3) -> Option<f32> {
        self.densities(c.x, c.y, c.z, 1).map(|d| d[0])
    }

    /// Whether voxel `c` is solid as the ground makes it, before its trees
    /// and edits.
    pub fn made(&mut self, c: IVec3) -> bool {
        if c.y <= 0 {
            return true;
        }
        if c.y >= self.tall {
            return false;
        }
        match self.density(c) {
            Some(density) => density > 0.0,
            None => c.y <= self.height(c.x, c.z),
        }
    }

    /// Whether the ground at each voxel column of `feet` grows grass, by
    /// the rules the GPU's tops pass has (`grassAt`), and how high it is
    /// there.
    fn grass_at(&mut self, feet: &[[i32; 2]]) -> Vec<Option<i32>> {
        let Some(generator) = self.volume.generator.clone() else {
            return feet
                .iter()
                .map(|&[x, z]| Some(self.height(x, z)).filter(|&h| h >= 0 && h < self.rows - 1))
                .collect();
        };
        if feet.is_empty() {
            return Vec::new();
        }
        // Each foot's height and those two voxels each way, at once.
        let v = self.voxel;
        let offsets = [[0, 0], [2, 0], [-2, 0], [0, 2], [0, -2]];
        let place = |x: i32, z: i32| ((x as f32 + 0.5) * v + generator.at[0], (z as f32 + 0.5) * v + generator.at[1]);
        let (mut xs, mut zs) = (Vec::new(), Vec::new());
        for &[x, z] in feet {
            for [dx, dz] in offsets {
                let (gx, gz) = place(x + dx, z + dz);
                xs.push(gx);
                zs.push(gz);
            }
        }
        let world = &generator.world;
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let up = |height: f32| ((height - generator.floor) / v).floor() as i32;
        let (fx, fz): (Vec<f32>, Vec<f32>) = feet.iter().map(|&[x, z]| place(x, z)).unzip();
        let raw: Vec<f32> = (0..feet.len()).map(|i| heights[i * 5]).collect();
        let (mut warmth, mut wetness) = (vec![0.0; feet.len()], vec![0.0; feet.len()]);
        world.climates(&fx, &fz, &raw, &mut warmth, &mut wetness);
        let rock = world.rock(&fx, &fz, &raw);
        let mut found = Vec::with_capacity(feet.len());
        for (i, &[x, z]) in feet.iter().enumerate() {
            let at = |k: usize| up(heights[i * 5 + k]);
            let h = at(0);
            let across = (at(1) - at(2)).abs().max((at(3) - at(4)).abs());
            let above = h as f32 * v + generator.floor - world.sea_level();
            let (warm, wet) = (warmth[i], wetness[i]);
            let rocky = rock.as_ref().is_some_and(|rock| rock[i]);
            let bare = rocky || across as f32 / 4.0 > 1.0 || above < 1.0 || warm < 0.0 || (wet < 0.12 && warm > 18.0);
            if h < 0 || h >= self.rows - 1 || bare {
                found.push(None);
                continue;
            }
            // Where the Volume moved the ground, or hollowed it under the foot, nothing is planted.
            found.push(match self.densities(x, h, z, 2) {
                Some(d) => (d[0] > 0.0 && d[1] <= 0.0).then_some(h),
                None => Some(h),
            });
        }
        found
    }

    /// The tree cell `cell` holds: none where it holds none, or where the
    /// ground there grows no grass.
    fn tree_in(&mut self, cell: (i32, i32)) -> Option<Tree> {
        if let Some(&tree) = self.cells.get(&cell) {
            return tree;
        }
        self.place(&[cell]);
        self.cells.get(&cell).copied().flatten()
    }

    /// Places the trees of `cells` not placed yet: with a generator, its
    /// heights, climate and rock at all their feet worked out at once.
    fn place(&mut self, cells: &[(i32, i32)]) {
        let Some(woods) = self.woods else { return };
        let mut cells: Vec<(i32, i32)> = cells.iter().filter(|cell| !self.cells.contains_key(cell)).copied().collect();
        if let Some(shared) = self.shared {
            let shared = shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            cells.retain(|cell| match shared.get(cell) {
                Some(&tree) => {
                    self.cells.insert(*cell, tree);
                    false
                }
                None => true,
            });
        }
        if cells.is_empty() {
            return;
        }
        let seed = self.seed + 91;
        let held: Vec<((i32, i32), Option<(usize, [i32; 2])>)> =
            cells.iter().map(|&cell| (cell, woods.in_cell([cell.0, cell.1], seed, self.voxel))).collect();
        let feet: Vec<[i32; 2]> = held.iter().filter_map(|(_, tree)| tree.map(|(_, foot)| foot)).collect();
        let grass = self.grass_at(&feet);
        let mut grass = grass.into_iter();
        let mut placed = Vec::with_capacity(held.len());
        for (cell, tree) in held {
            let tree = tree.and_then(|(model, foot)| Some(woods.stood(model, foot, grass.next().flatten()?)));
            placed.push((cell, tree));
        }
        if let Some(shared) = self.shared {
            shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).extend(placed.iter().copied());
        }
        self.cells.extend(placed);
    }

    /// The trees whose boxes reach the box of voxels from `low` to `high`.
    pub fn trees_near(&mut self, low: IVec3, high: IVec3) -> Vec<Tree> {
        let Some(woods) = self.woods else { return Vec::new() };
        let v = self.voxel;
        let cell = |voxels: i32, by: f32| ((voxels as f32 * v + by) / trees::CELL).floor() as i32;
        let (reach, mut found) = (woods.reach, Vec::new());
        let (xs, zs) = (cell(low.x, -reach)..=cell(high.x + 1, reach), cell(low.z, -reach)..=cell(high.z + 1, reach));
        let all: Vec<(i32, i32)> = zs.clone().flat_map(|cz| xs.clone().map(move |cx| (cx, cz))).collect();
        self.place(&all);
        for cz in cell(low.z, -reach)..=cell(high.z + 1, reach) {
            for cx in cell(low.x, -reach)..=cell(high.x + 1, reach) {
                let Some(tree) = self.tree_in((cx, cz)) else { continue };
                let (least, most) = woods.bounds(&tree);
                if (0..3).all(|a| most[a] >= low[a] && least[a] <= high[a]) {
                    found.push(tree);
                }
            }
        }
        found
    }

    /// What `trees` have at voxel `c`: a log where any has one, else leaves
    /// where any has them. Where trees cross, the GPU draws whichever it
    /// meets first; a log there is still to be bumped into.
    pub fn tree_at(&self, trees: &[Tree], c: IVec3) -> u8 {
        let Some(woods) = self.woods else { return AIR };
        trees.iter().map(|tree| woods.at(tree, c.to_array())).fold(AIR, |most, kind| match (most, kind) {
            (LOG, _) | (_, LOG) => LOG,
            (AIR, kind) => kind,
            (most, _) => most,
        })
    }

    /// What voxel `c` is, `trees` those that may reach it, and `edits` the
    /// indices of the edits that may: the ground as it is made, a tree's
    /// voxel where the ground leaves none, and each edit over it in turn
    /// (a print's solid voxels and a ball filled are ground, a ball carved
    /// out is air).
    pub fn kind(&mut self, c: IVec3, trees: &[Tree], edits: &[usize]) -> u8 {
        let mut kind = if self.made(c) { GROUND } else { self.tree_at(trees, c) };
        for &e in edits {
            let edit = &self.volume.edits[e];
            if edit.holds(c, self.voxel) != 0 {
                kind = if edit.print.is_some() || edit.fill.is_some() { GROUND } else { AIR };
            }
        }
        kind
    }

    /// What voxel `c` is, looking up the trees and edits that reach it.
    pub fn at(&mut self, c: IVec3) -> u8 {
        let trees = if self.woods.is_some() { self.trees_near(c, c) } else { Vec::new() };
        let edits = self.edits_near(c, c);
        self.kind(c, &trees, &edits)
    }

    /// The indices of the edits whose reach meets the box of voxels from
    /// `low` to `high`.
    pub fn edits_near(&self, low: IVec3, high: IVec3) -> Vec<usize> {
        let v = self.voxel;
        let (low, high) = (low.as_vec3() * v - v, (high + 1).as_vec3() * v + v);
        let edits = self.volume.edits.iter().enumerate();
        let near = edits.filter(|(_, edit)| {
            let (least, most) = edit.reach();
            least.cmple(high).all() && most.cmpge(low).all()
        });
        near.map(|(e, _)| e).collect()
    }

    /// How far point `p` (in voxels) surely is from anything solid, in
    /// voxels: over the ground, out of every ball filled and every print's
    /// reach, and out of every tree's box, by how far.
    pub fn clear(&mut self, p: Vec3) -> f32 {
        let c = p.floor().as_ivec3();
        let ground = match self.density(c) {
            Some(density) => -density / self.voxel * super::ray::DENSITY_STEP,
            None => (p.y - (self.height(c.x, c.z) + 1) as f32) * super::ray::HEIGHT_STEP,
        };
        let world = p * self.voxel;
        let fills = self.volume.edits.iter().filter(|edit| edit.fill.is_some() || edit.print.is_some());
        let near = fills.map(|edit| (world.distance(edit.at) - edit.radius) / self.voxel).fold(f32::MAX, f32::min);
        let mut clear = ground.min(near - 1.0).min(p.y);
        if let Some(woods) = self.woods
            && clear > 0.0
        {
            // Trees within as far as the step would go, a few metres at most, which the step is held to.
            clear = clear.min(TREE_LOOK);
            let by = clear.ceil() as i32;
            for tree in self.trees_near(c - by, c + by) {
                let (least, most) = woods.bounds(&tree);
                let off =
                    Vec3::from([0, 1, 2].map(|a| (least[a] as f32 - p[a]).max(p[a] - (most[a] + 1) as f32).max(0.0)));
                clear = clear.min(off.length() - 1.0);
            }
        }
        clear
    }
}

/// What an endless Voxel Volume's voxels are, as the CPU finds them, for
/// collision: its ground and its trees' logs solid, their leaves, its grass
/// and flowers open. Made from the volume, which it keeps; cheap to make
/// again after an edit, its trees' voxels kept for all volumes alike.
#[derive(Clone)]
pub struct VoxelSolids {
    volume: VoxelVolume,
    woods: Option<Arc<Woods>>,
    /// The trees placed so far, kept for the next volume alike but for its edits.
    placed: Arc<Mutex<Cells>>,
}

impl std::fmt::Debug for VoxelSolids {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoxelSolids").field("voxels_per_metre", &self.volume.voxels_per_metre).finish_non_exhaustive()
    }
}

impl VoxelSolids {
    /// Those of `volume`: none but for an endless one.
    pub fn of(volume: &VoxelVolume) -> Option<Self> {
        if !volume.endless {
            return None;
        }
        let block = volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE);
        Some(Self { volume: volume.clone(), woods: trees::woods(&volume.trees, block), placed: Arc::default() })
    }

    /// The volume they are of.
    pub fn volume(&self) -> &VoxelVolume {
        &self.volume
    }

    /// How large a voxel is, in metres.
    pub fn voxel(&self) -> f32 {
        1.0 / self.volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32
    }

    /// Whether each voxel of the box from `low`, `size` voxels a side (in
    /// voxels from the volume's corner), is solid: written to `solid`
    /// along X first, then Y, then Z.
    pub fn fill(&self, low: [i32; 3], size: [i32; 3], solid: &mut [bool]) {
        let (low, size) = (IVec3::from(low), IVec3::from(size).max(IVec3::ZERO));
        let count = (size.x * size.y * size.z) as usize;
        let solid = &mut solid[..count];
        solid.fill(false);
        if count == 0 {
            return;
        }
        let high = low + size - 1;
        let mut ground = Ground::new(&self.volume, self.woods.as_deref());
        ground.shared = Some(&self.placed);
        ground.heights_for(low.x, low.z, [size.x, size.z]);
        let trees = ground.trees_near(low, high);
        let edits = ground.edits_near(low, high);
        let volume = self.volume.generator.as_ref().and_then(|generator| generator.world.volume());
        let over = volume.as_ref().map_or(0.0, |volume| volume.reach());
        let (tall, voxel) = (ground.tall, ground.voxel);
        let index = |x: i32, y: i32, z: i32| (x + size.x * (y + size.y * z)) as usize;
        // Which voxels an edit carved out, which no log fills.
        let mut edited = vec![false; count];
        // A voxel an edit carved, for the logs to leave out.
        const EDITED: u8 = 255;
        for z in 0..size.z {
            for x in 0..size.x {
                let (wx, wz) = (low.x + x, low.z + z);
                let h = ground.height(wx, wz);
                // The column's ground as made: by its height, or by the Volume within its reach over that.
                let top = match volume {
                    Some(_) => (h + (over / voxel).ceil() as i32 + 1).min(high.y).min(tall - 1),
                    None => h.min(high.y).min(tall - 1),
                };
                let from = low.y.max(1);
                let densities = match (&volume, top >= from) {
                    (Some(_), true) => ground.densities(wx, from, wz, (top - from + 1) as usize),
                    _ => None,
                };
                for y in 0..size.y {
                    let wy = low.y + y;
                    let made = match &densities {
                        _ if wy <= 0 => true,
                        _ if wy > top => false,
                        Some(d) => d[(wy - from) as usize] > 0.0,
                        None => volume.is_none(),
                    };
                    let mut kind = if made { GROUND } else { AIR };
                    for &e in &edits {
                        let edit = &self.volume.edits[e];
                        if edit.holds(IVec3::new(wx, wy, wz), voxel) != 0 {
                            kind = if edit.print.is_some() || edit.fill.is_some() { GROUND } else { EDITED };
                        }
                    }
                    solid[index(x, y, z)] = kind == GROUND;
                    edited[index(x, y, z)] = kind == EDITED;
                }
            }
        }
        // The trees' logs where the ground leaves none and no edit carved them.
        let Some(woods) = self.woods.as_deref() else { return };
        let (least, most) = (low.to_array(), high.to_array());
        for tree in &trees {
            woods.logs_in(tree, least, most, |c| {
                let at = index(c[0] - low.x, c[1] - low.y, c[2] - low.z);
                if !edited[at] {
                    solid[at] = true;
                }
            });
        }
    }

    /// These, keeping the trees `older` placed where its ground and trees
    /// are these', as after an edit.
    pub fn keeping(mut self, older: &Self) -> Self {
        if self.changed_from(older).is_some() {
            self.placed = older.placed.clone();
        }
        self
    }

    /// The boxes of voxels, least and most corners, where these voxels may
    /// differ from `older`'s: none where any may, as when anything but its
    /// edits changed.
    pub fn changed_from(&self, older: &Self) -> Option<Vec<([i32; 3], [i32; 3])>> {
        let (new, old) = (&self.volume, &older.volume);
        let same_trees = match (&self.woods, &older.woods) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let same = new.voxels_per_metre == old.voxels_per_metre
            && new.size.y == old.size.y
            && new.ground == old.ground
            && new.generator == old.generator
            && same_trees;
        if !same {
            return None;
        }
        if Arc::ptr_eq(&new.edits, &old.edits) || new.edits == old.edits {
            return Some(Vec::new());
        }
        // An edit added or taken away at the end, as a brush and its undo do; anything else changes all.
        let shared = new.edits.iter().zip(old.edits.iter()).take_while(|(a, b)| a == b).count();
        let v = self.voxel();
        let changed = new.edits[shared..].iter().chain(&old.edits[shared..]);
        let boxes = changed.map(|edit| {
            let (least, most) = edit.reach();
            (
                (least / v).floor().as_ivec3().to_array().map(|a| a - 1),
                (most / v).ceil().as_ivec3().to_array().map(|a| a + 1),
            )
        });
        Some(boxes.collect())
    }
}

#[cfg(test)]
mod tests {
    use voxear_terrain::{
        blocks::plant::PlantDef,
        generate::{NodeKind, OwnVoxels, WorldGen, starters},
    };

    use super::super::trees::LEAF;
    use super::*;
    use crate::{VoxelEdit, VoxelGenerator, VoxelMaterial, VoxelTrees};

    fn wood() -> Vec<VoxelTrees> {
        let oak = PlantDef { height: [8.0, 12.0], crown_width: [7.0, 10.0], ..PlantDef::default() };
        vec![VoxelTrees { plant: Arc::new(oak), per_hectare: 120.0, wood: [100, 70, 50], leaves: [70, 120, 40] }]
    }

    fn hills() -> VoxelVolume {
        let ground = VoxelGround { height: 3.0, hills: 2.0, ..VoxelGround::default() };
        VoxelVolume {
            endless: true,
            size: Vec3::new(16.0, 24.0, 16.0),
            ground: Some(ground),
            trees: wood(),
            ..VoxelVolume::default()
        }
    }

    /// Alpine with a world of blocks' overhangs, tunnels and chambers in its Volume, its great halls taken out.
    fn caved_alpine() -> WorldGen {
        let found = starters().into_iter().find(|starter| starter.name == "Alpine").expect("Alpine");
        let world = found.world(7, 1.0);
        let mut graph = world.graph().clone();
        assert!(graph.add_voxels(OwnVoxels { overhangs: 4.0, caves: 0.8, roof: 0.0 }, false));
        let halls: Vec<_> = graph
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::CaveNetwork(_)))
            .map(|node| node.id)
            .collect();
        for node in halls {
            graph.remove(node);
        }
        WorldGen::new(world.seed(), world.scale(), world.sea_level(), graph)
    }

    /// Whether `fill` finds each voxel of the box from `low` as `at` does, one by one.
    fn fill_matches_each(volume: &VoxelVolume, low: [i32; 3], size: [i32; 3]) -> usize {
        let solids = VoxelSolids::of(volume).expect("endless");
        let mut solid = vec![false; (size[0] * size[1] * size[2]) as usize];
        solids.fill(low, size, &mut solid);
        let mut ground = Ground::new(volume, solids.woods.as_deref());
        let mut count = 0;
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    let c = IVec3::new(low[0] + x, low[1] + y, low[2] + z);
                    let kind = ground.at(c);
                    let filled = solid[(x + size[0] * (y + size[1] * z)) as usize];
                    assert_eq!(filled, kind == GROUND || kind == LOG, "at {c}: {kind}");
                    count += usize::from(filled);
                }
            }
        }
        count
    }

    #[test]
    fn a_box_filled_at_once_is_solid_where_each_voxel_is_and_carves_and_fills_change_it() {
        let mut volume = hills();
        let low = [10, 40, -20];
        let solid = fill_matches_each(&volume, low, [34, 34, 34]);
        assert!(solid > 0 && solid < 34 * 34 * 34, "the ground's top crosses the box: {solid}");
        volume.edits = Arc::new(vec![
            VoxelEdit::ball(Vec3::new(1.5, 3.0, -0.5), 0.8, None),
            VoxelEdit::ball(Vec3::new(2.0, 4.0, 0.0), 0.5, Some(VoxelMaterial::Rock)),
        ]);
        assert_ne!(fill_matches_each(&volume, low, [34, 34, 34]), solid, "edited");
        // In the mountains, through the generator's Volume.
        let alpine = Arc::new(caved_alpine());
        let floor = alpine.height_at(40.0, 40.0) - 20.0;
        let generated = VoxelVolume {
            size: Vec3::new(16.0, 48.0, 16.0),
            generator: Some(VoxelGenerator { world: alpine, floor, at: [0.0, 0.0] }),
            ..hills()
        };
        let low = [40 * 16 - 17, 20 * 16 - 17, 40 * 16 - 17];
        let solid = fill_matches_each(&generated, low, [34, 34, 34]);
        assert!(solid > 0 && solid < 34 * 34 * 34, "the mountain's ground crosses the box: {solid}");
    }

    #[test]
    fn a_trees_logs_are_solid_and_its_leaves_are_not() {
        let volume = hills();
        let solids = VoxelSolids::of(&volume).expect("endless");
        let woods = solids.woods.as_deref().expect("trees");
        let mut ground = Ground::new(&volume, Some(woods));
        let trees = ground.trees_near(IVec3::new(-160, 0, -160), IVec3::new(160, 380, 160));
        assert!(trees.len() > 4, "{} trees in 20 m square", trees.len());
        let tree = trees[0];
        let (least, most) = woods.bounds(&tree);
        let (mut logs, mut leaves) = (Vec::new(), Vec::new());
        for z in least[2]..=most[2] {
            for y in least[1]..=most[1] {
                for x in least[0]..=most[0] {
                    match woods.at(&tree, [x, y, z]) {
                        LOG => logs.push([x, y, z]),
                        LEAF => leaves.push([x, y, z]),
                        _ => {}
                    }
                }
            }
        }
        assert!(logs.len() > 100 && leaves.len() > 1000, "{} logs, {} leaves", logs.len(), leaves.len());
        // Each found as the walk finds it, a log solid to collide with and leaves not, but where the ground is.
        let mut solid = [false];
        for &c in logs.iter().step_by(37) {
            if ground.made(IVec3::from(c)) {
                continue;
            }
            solids.fill(c, [1, 1, 1], &mut solid);
            assert!(solid[0], "a log at {c:?}");
        }
        let mut open = 0;
        for &c in leaves.iter().step_by(97) {
            // Where the ground is, or another tree's log, is solid.
            if ground.made(IVec3::from(c)) || ground.at(IVec3::from(c)) != LEAF {
                continue;
            }
            open += 1;
            solids.fill(c, [1, 1, 1], &mut solid);
            assert!(!solid[0], "leaves at {c:?}");
        }
        assert!(open > 5, "{open} leaves looked at");
        // A ray down onto a crown meets its leaves, where the ground without trees is further.
        let top = leaves.iter().max_by_key(|c| c[1]).copied().unwrap();
        let from = (IVec3::from(top).as_vec3() + Vec3::new(0.5, 40.0, 0.5)) / 16.0;
        let hit = super::super::raycast(&volume, Vec3::ZERO, from, -Vec3::Y, 100.0).expect("the crown");
        let bare = VoxelVolume { trees: Vec::new(), ..volume.clone() };
        let under = super::super::raycast(&bare, Vec3::ZERO, from, -Vec3::Y, 100.0).expect("the ground");
        assert!((hit.distance - 40.0 / 16.0).abs() < 0.07 && under.distance > hit.distance + 1.0, "{hit:?} {under:?}");
    }
}
