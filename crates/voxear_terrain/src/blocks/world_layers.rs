//! World layers: what a scene puts over a generated world's ground without
//! storing blocks. Each is data on an Obj, handed here in the world's own
//! blocks, and worked out per column from where the column is alone, so
//! chunks made apart meet exactly and the same ground comes back every time.
//!
//! The ground of a column is worked out in this order: the generator's, then
//! world layers by kind (ground fits first) and within a kind by priority,
//! then by id, then the hand edits of the world's `.blocks` file. Edits are
//! kept as differences from the ground with its layers, so a block placed by
//! hand inside a layer's reach stays.
//!
//! [`WorldLayers`] keeps them by chunk column, so making a column asks only
//! those that reach it, and finds the columns a change touches
//! ([`WorldLayers::changed_columns`]), so only those are made again.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    chunk::{AIR, BlockId, CHUNK},
    structure::GroundFit,
};

/// A ground fit: the ground under and round a footprint levelled to one
/// height, eased back to the generator's over `slope_out` blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FitLayer {
    /// What it is known by between one set of layers and the next, as the
    /// Obj it is on: a change to it touches its old place and its new one.
    pub id: u64,
    /// Its footprint: outlines along x and z in the world's blocks, each a
    /// closed loop of corners in either turn.
    pub outlines: Vec<Vec<[f32; 2]>>,
    /// The block its floor stands on top of, plus one: the first block of air
    /// over the levelled ground, in the world's blocks.
    pub level: i32,
    pub mode: GroundFit,
    /// How far past the footprint the ground is levelled, in blocks.
    pub margin: f32,
    /// Over how many blocks past the margin the ground eases back to the
    /// generator's, a block up or down for each block across, so it can be
    /// walked; none for a sheer edge, as a retaining wall.
    pub slope_out: u32,
    /// What made ground is built of, and topped with; air for the column's
    /// own, as its biome gives them.
    pub fill: BlockId,
    pub top: BlockId,
    /// Whether what the generator grows and builds is left out within its
    /// reach: trees, plants, structures.
    pub clear_plants: bool,
    pub priority: i32,
}

/// Whether a fit raises the ground under it, and whether it lowers it.
fn raises(mode: GroundFit) -> bool {
    matches!(mode, GroundFit::Foundation | GroundFit::Flatten)
}

fn lowers(mode: GroundFit) -> bool {
    matches!(mode, GroundFit::Flatten | GroundFit::Cut)
}

impl FitLayer {
    /// Everything it is, as bytes, the same on every machine.
    pub fn bytes(&self) -> Vec<u8> {
        let FitLayer { id, outlines, level, mode, margin, slope_out, fill, top, clear_plants, priority } = self;
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend(id.to_le_bytes());
        for outline in outlines {
            bytes.extend((outline.len() as u32).to_le_bytes());
            outline.iter().flatten().for_each(|v| bytes.extend(v.to_le_bytes()));
        }
        bytes.extend(level.to_le_bytes());
        bytes.push(*mode as u8);
        bytes.extend(margin.to_le_bytes());
        bytes.extend(slope_out.to_le_bytes());
        bytes.extend(fill.to_le_bytes());
        bytes.extend(top.to_le_bytes());
        bytes.push(u8::from(*clear_plants));
        bytes.extend(priority.to_le_bytes());
        bytes
    }

    /// How far past its footprint it reaches, in blocks.
    pub fn reach(&self) -> f32 {
        self.margin.max(0.0) + self.slope_out as f32
    }

    /// The box everything it changes lies within, along x and z, in blocks:
    /// `[low, high]`, the high side reached.
    pub fn bounds(&self) -> Option<[[i32; 2]; 2]> {
        let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
        for corner in self.outlines.iter().flatten() {
            for axis in 0..2 {
                low[axis] = low[axis].min(corner[axis]);
                high[axis] = high[axis].max(corner[axis]);
            }
        }
        if low[0] > high[0] || !low.iter().chain(&high).all(|v| v.is_finite()) {
            return None;
        }
        let reach = self.reach() + 1.0;
        Some([low.map(|v| (v - reach).floor() as i32), high.map(|v| (v + reach).ceil() as i32)])
    }

    /// How far the middle of column `[x, z]` lies from its footprint, in
    /// blocks: 0 inside it.
    pub fn distance(&self, [x, z]: [i32; 2]) -> f32 {
        let point = [x as f32 + 0.5, z as f32 + 0.5];
        let mut nearest = f32::MAX;
        for outline in self.outlines.iter().filter(|outline| outline.len() >= 3) {
            if inside(outline, point) {
                return 0.0;
            }
            for (at, &a) in outline.iter().enumerate() {
                let b = outline[(at + 1) % outline.len()];
                nearest = nearest.min(to_segment(point, a, b));
            }
        }
        nearest
    }

    /// The ground's new top in column `[x, z]`, the first block of air over
    /// it, from its top `ground`; and whether the column is within its
    /// levelled part (footprint and margin). None where it changes nothing.
    fn shape(&self, at: [i32; 2], ground: i32) -> Option<(i32, bool)> {
        let distance = self.distance(at);
        let past = distance - self.margin.max(0.0);
        if past <= 0.0 {
            let top = match self.mode {
                GroundFit::Leave => ground,
                GroundFit::Foundation => ground.max(self.level),
                GroundFit::Flatten => self.level,
                GroundFit::Cut => ground.min(self.level),
            };
            return Some((top, true));
        }
        // Eased out a block a step: `steps` blocks past the margin the ground is within `steps` of the level.
        let steps = past.ceil() as i32;
        if steps > self.slope_out as i32 {
            return None;
        }
        let mut top = ground;
        if raises(self.mode) {
            top = top.max(self.level - steps);
        }
        if lowers(self.mode) {
            top = top.min(self.level + steps);
        }
        (top != ground || self.clear_plants).then_some((top, false))
    }
}

/// Whether `point` lies inside the loop `outline`, by the crossings of a ray
/// along x.
fn inside(outline: &[[f32; 2]], point: [f32; 2]) -> bool {
    let mut within = false;
    for (at, &a) in outline.iter().enumerate() {
        let b = outline[(at + 1) % outline.len()];
        if (a[1] > point[1]) != (b[1] > point[1]) {
            let cross = a[0] + (point[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if point[0] < cross {
                within = !within;
            }
        }
    }
    within
}

fn to_segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (ab, ap) = ([b[0] - a[0], b[1] - a[1]], [point[0] - a[0], point[1] - a[1]]);
    let length = ab[0] * ab[0] + ab[1] * ab[1];
    let along = if length > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1]) / length).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dz) = (ap[0] - ab[0] * along, ap[1] - ab[1] * along);
    (dx * dx + dz * dz).sqrt()
}

/// What the layers make of one column: its ground's new top (the first block
/// of air over it), whether everything over that is cleared, and the blocks
/// made ground is built of and topped with (air for the column's own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shaped {
    pub top: i32,
    pub cleared: bool,
    pub fill: BlockId,
    pub top_block: BlockId,
}

/// Every world layer of a world, kept by the chunk column each reaches.
#[derive(Clone, Debug, Default)]
pub struct WorldLayers {
    fits: Vec<FitLayer>,
    /// The fits reaching each chunk column, by their place in `fits`.
    cells: HashMap<[i32; 2], Vec<u32>>,
    /// A number for all of them, the same for the same layers.
    hash: u64,
}

impl PartialEq for WorldLayers {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.fits == other.fits
    }
}

impl WorldLayers {
    /// The layers of `fits`, in the order they apply: by priority, then id.
    pub fn new(mut fits: Vec<FitLayer>) -> Self {
        // A fit with no id of its own is known by what it is: moved, it is another, and touches where it was too.
        for fit in fits.iter_mut().filter(|fit| fit.id == 0) {
            fit.id = crate::layers::stable_hash(&fit.bytes()).max(1);
        }
        fits.sort_by_key(|fit| (fit.priority, fit.id));
        let mut cells: HashMap<[i32; 2], Vec<u32>> = HashMap::new();
        for (at, fit) in fits.iter().enumerate() {
            for column in fit.bounds().into_iter().flat_map(columns_of) {
                cells.entry(column).or_default().push(at as u32);
            }
        }
        let bytes: Vec<u8> = fits.iter().flat_map(FitLayer::bytes).collect();
        Self { fits, cells, hash: crate::layers::stable_hash(&bytes) }
    }

    pub fn is_empty(&self) -> bool {
        self.fits.is_empty()
    }

    pub fn fits(&self) -> &[FitLayer] {
        &self.fits
    }

    /// How many chunk columns its layers reach.
    pub fn columns(&self) -> usize {
        self.cells.len()
    }

    /// A number for all its layers, the same for the same ones.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Whether any layer reaches chunk column `column`.
    pub fn reaches(&self, column: [i32; 2]) -> bool {
        self.cells.contains_key(&column)
    }

    /// Whether any layer reaches chunk column `[cx, cz]` of level `level`,
    /// whose chunks are each `2^level` of the world's own across.
    pub fn reaches_level(&self, level: u32, [cx, cz]: [i32; 2]) -> bool {
        let count = 1i32 << level.min(16);
        if self.cells.len() < (count * count) as usize {
            let [x0, z0] = [cx * count, cz * count];
            return self.cells.keys().any(|&[x, z]| (x0..x0 + count).contains(&x) && (z0..z0 + count).contains(&z));
        }
        (0..count).any(|z| (0..count).any(|x| self.reaches([cx * count + x, cz * count + z])))
    }

    fn reaching(&self, column: [i32; 2]) -> impl Iterator<Item = &FitLayer> {
        self.cells.get(&column).into_iter().flatten().map(|&at| &self.fits[at as usize])
    }

    /// What its layers make of column `[x, z]`, in the world's own blocks,
    /// whose ground's top (the first block of air over it) is `ground`. None
    /// where none of them changes it.
    pub fn shape(&self, [x, z]: [i32; 2], ground: i32) -> Option<Shaped> {
        let side = CHUNK as i32;
        let mut shaped: Option<Shaped> = None;
        for fit in self.reaching([x.div_euclid(side), z.div_euclid(side)]) {
            let now = shaped.map_or(ground, |shaped| shaped.top);
            let Some((top, levelled)) = fit.shape([x, z], now) else { continue };
            let was = shaped.unwrap_or(Shaped { top: ground, cleared: false, fill: AIR, top_block: AIR });
            // Levelled ground takes the fit's own blocks; the eased edge the blocks under it already.
            let (fill, top_block) = match levelled {
                true => (fit.fill, fit.top),
                false => (was.fill, was.top_block),
            };
            let cleared = was.cleared || fit.clear_plants || levelled && lowers(fit.mode) || top < now;
            shaped = Some(Shaped { top, cleared, fill, top_block });
        }
        shaped
    }

    /// As [`WorldLayers::shape`] at level `level` of coarseness, in that
    /// level's blocks: the column's middle in the world's own blocks asked.
    pub fn shape_at(&self, level: u32, [x, z]: [i32; 2], ground: i32) -> Option<Shaped> {
        if level == 0 {
            return self.shape([x, z], ground);
        }
        let scale = 1i32 << level.min(16);
        let middle = [x * scale + scale / 2, z * scale + scale / 2];
        let shaped = self.shape(middle, ground * scale)?;
        Some(Shaped { top: (shaped.top as f32 / scale as f32).round() as i32, ..shaped })
    }

    /// Whether a layer keeps what the generator grows off column `[x, z]`,
    /// in the world's blocks.
    pub fn clears_plants(&self, [x, z]: [i32; 2]) -> bool {
        let side = CHUNK as i32;
        self.reaching([x.div_euclid(side), z.div_euclid(side)])
            .filter(|fit| fit.clear_plants)
            .any(|fit| fit.distance([x, z]) <= fit.reach())
    }

    /// Whether anything of a layer reaches the box from `low` to `high`
    /// along x and z, both reached, in the world's blocks, that leaves out
    /// what the generator grows and builds.
    pub fn keeps_out(&self, low: [i32; 2], high: [i32; 2]) -> bool {
        let side = CHUNK as i32;
        let cell = |v: i32| v.div_euclid(side);
        for cz in cell(low[1])..=cell(high[1]) {
            for cx in cell(low[0])..=cell(high[0]) {
                let meets = |fit: &&FitLayer| {
                    fit.bounds()
                        .is_some_and(|[l, h]| l[0] <= high[0] && h[0] >= low[0] && l[1] <= high[1] && h[1] >= low[1])
                };
                if self
                    .reaching([cx, cz])
                    .filter(|fit| fit.clear_plants || fit.mode != GroundFit::Leave)
                    .any(|fit| meets(&fit))
                {
                    return true;
                }
            }
        }
        false
    }

    /// The chunk columns whose ground differs between `old` and `new`: under
    /// every layer added, taken away or changed, where it was and where it is.
    pub fn changed_columns(old: &WorldLayers, new: &WorldLayers) -> Vec<[i32; 2]> {
        fn by_id(layers: &WorldLayers) -> HashMap<u64, &FitLayer> {
            layers.fits.iter().map(|fit| (fit.id, fit)).collect()
        }
        let (was, now) = (by_id(old), by_id(new));
        let mut columns: Vec<[i32; 2]> = Vec::new();
        let mut touch = |fit: &FitLayer| columns.extend(fit.bounds().into_iter().flat_map(columns_of));
        for (id, fit) in &was {
            if now.get(id) != Some(fit) {
                touch(fit);
            }
        }
        for (id, fit) in &now {
            if was.get(id) != Some(fit) {
                touch(fit);
            }
        }
        // A fit unchanged under one that changed applies in another order only if its priority ties: it is made
        // again with the column anyway.
        columns.sort_unstable();
        columns.dedup();
        columns
    }
}

/// The chunk columns a box of blocks `[low, high]` covers.
fn columns_of([low, high]: [[i32; 2]; 2]) -> impl Iterator<Item = [i32; 2]> {
    let side = CHUNK as i32;
    let (l, h) = (low.map(|v| v.div_euclid(side)), high.map(|v| v.div_euclid(side)));
    (l[1]..=h[1]).flat_map(move |z| (l[0]..=h[0]).map(move |x| [x, z]))
}

/// Puts what `shaped` makes of a column into it, through `get` and `put` of
/// its blocks by height: built up to the new top with `block_at`'s blocks (by
/// depth under the new top, the fill and top given where they are not air),
/// and cleared over it to `highest` with `open`'s (air, or the sea where it
/// stands), where it is cleared or lowered. `ground` is the column's top as
/// the generator made it.
pub fn shape_column(
    shaped: Shaped,
    ground: i32,
    highest: i32,
    block_at: &dyn Fn(f32) -> BlockId,
    open: &dyn Fn(i32) -> BlockId,
    put: &mut dyn FnMut(i32, BlockId),
) {
    let Shaped { top, cleared, fill, top_block } = shaped;
    let made = |y: i32| {
        let depth = top as f32 - (y as f32 + 0.5);
        match (depth < 1.0, fill, top_block) {
            (true, _, block) if block != AIR => block,
            (false, block, _) if block != AIR => block,
            _ => block_at(depth),
        }
    };
    // Built up, from its old top block, buried now; or lowered, the new top's layers laid on what is left; or
    // level, only a top of its own laid.
    let relaid = 4;
    let laid = match top.cmp(&ground) {
        std::cmp::Ordering::Greater => ground - 1..top,
        std::cmp::Ordering::Less => top - relaid..top,
        std::cmp::Ordering::Equal if top_block != AIR => top - 1..top,
        std::cmp::Ordering::Equal => 0..0,
    };
    for y in laid {
        put(y, made(y));
    }
    if cleared || top < ground {
        for y in top..=highest.max(ground) {
            put(y, open(y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(mode: GroundFit, slope_out: u32) -> FitLayer {
        FitLayer {
            id: 1,
            outlines: vec![vec![[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]]],
            level: 10,
            mode,
            margin: 1.0,
            slope_out,
            fill: AIR,
            top: AIR,
            clear_plants: false,
            priority: 0,
        }
    }

    #[test]
    fn a_foundation_raises_its_footprint_and_eases_out_a_block_a_step() {
        let layers = WorldLayers::new(vec![fit(GroundFit::Foundation, 4)]);
        assert_eq!(layers.shape([4, 4], 5).map(|s| s.top), Some(10));
        // Higher ground is left as it is under a foundation.
        assert_eq!(layers.shape([4, 4], 14).map(|s| (s.top, s.cleared)), Some((14, false)));
        // Past the margin: a block down for each block out, until it meets the ground or the slope out ends.
        assert_eq!(layers.shape([9, 4], 5).map(|s| s.top), Some(9));
        assert_eq!(layers.shape([10, 4], 5).map(|s| s.top), Some(8));
        assert_eq!(layers.shape([12, 4], 5).map(|s| s.top), Some(6));
        assert_eq!(layers.shape([14, 4], 5), None);
        // Walkable: no step more than a block between neighbours.
        let tops: Vec<i32> = (0..20).map(|x| layers.shape([x, 4], 5).map_or(5, |s| s.top)).collect();
        assert!(tops.windows(2).all(|pair| (pair[0] - pair[1]).abs() <= 1), "{tops:?}");
    }

    #[test]
    fn flatten_and_cut_lower_and_clear_and_a_wall_has_no_slope() {
        let flat = WorldLayers::new(vec![fit(GroundFit::Flatten, 0)]);
        let shaped = flat.shape([4, 4], 20).unwrap();
        assert_eq!((shaped.top, shaped.cleared), (10, true));
        assert_eq!(flat.shape([4, 4], 3).unwrap().top, 10);
        assert_eq!(flat.shape([10, 4], 20), None);
        let cut = WorldLayers::new(vec![fit(GroundFit::Cut, 2)]);
        assert_eq!(cut.shape([4, 4], 3).unwrap().top, 3);
        assert_eq!(cut.shape([4, 4], 30).unwrap().top, 10);
        assert_eq!(cut.shape([10, 4], 30).unwrap().top, 12);
    }

    #[test]
    fn later_priorities_win_and_only_changed_layers_touch_columns() {
        let low = FitLayer { id: 1, level: 10, ..fit(GroundFit::Flatten, 0) };
        let high = FitLayer { id: 2, level: 14, priority: 1, ..fit(GroundFit::Flatten, 0) };
        let layers = WorldLayers::new(vec![high.clone(), low.clone()]);
        assert_eq!(layers.shape([4, 4], 0).unwrap().top, 14);
        let far = FitLayer { id: 3, outlines: vec![vec![[200.0, 0.0], [208.0, 0.0], [208.0, 8.0]]], ..low.clone() };
        let before = WorldLayers::new(vec![low.clone(), far.clone()]);
        let moved = FitLayer { level: 12, ..far.clone() };
        let after = WorldLayers::new(vec![low, moved]);
        let changed = WorldLayers::changed_columns(&before, &after);
        assert!(changed.iter().all(|c| c[0] >= 6), "{changed:?}");
        assert!(changed.contains(&[6, 0]));
        assert!(WorldLayers::changed_columns(&before, &before).is_empty());
    }

    #[test]
    fn a_turned_outline_is_measured_where_it_lies() {
        let diamond = FitLayer {
            outlines: vec![vec![[4.0, 0.0], [8.0, 4.0], [4.0, 8.0], [0.0, 4.0]]],
            ..fit(GroundFit::Flatten, 0)
        };
        assert_eq!(diamond.distance([3, 3]), 0.0);
        assert!(diamond.distance([0, 0]) > 1.5);
    }

    #[test]
    fn shaping_a_column_builds_up_and_clears() {
        let mut column = vec![1u16; 6];
        column.extend([0u16; 14]);
        let shaped = Shaped { top: 9, cleared: true, fill: 3, top_block: 4 };
        let mut put = |y: i32, block: BlockId| column[y as usize] = block;
        shape_column(shaped, 6, 19, &|_| 2, &|_| AIR, &mut put);
        assert_eq!(&column[..12], &[1, 1, 1, 1, 1, 3, 3, 3, 4, 0, 0, 0]);
    }
}
