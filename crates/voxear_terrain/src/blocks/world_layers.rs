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

pub use super::area_layer::{AreaLayer, WorldAreas};
pub use super::road_layer::{Bridge, Palette, RoadLayer, Steps, Tunnel};
use super::{
    chunk::{AIR, BlockId, CHUNK},
    road_layer::OnRoad,
    structure::GroundFit,
};

/// A ground fit: the ground under and round a footprint levelled to one
/// height, eased back to the generator's over `slope_out` blocks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    /// The levelled ground's top laid with a mix or a pattern in place of
    /// `top`, along x and across z: an area of paving.
    pub palette: Palette,
    /// Whether it levels to the ground as it is, column by column: its top
    /// laid and what stands on it cleared, as a square that follows a slope.
    pub follow: bool,
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
        let FitLayer {
            id,
            outlines,
            level,
            mode,
            margin,
            slope_out,
            fill,
            top,
            clear_plants,
            priority,
            palette,
            follow,
        } = self;
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
        palette.bytes(&mut bytes);
        bytes.push(u8::from(*follow));
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
        let level = if self.follow { ground } else { self.level };
        if past <= 0.0 {
            let top = match self.mode {
                GroundFit::Leave => ground,
                GroundFit::Foundation => ground.max(level),
                GroundFit::Flatten => level,
                GroundFit::Cut => ground.min(level),
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
            top = top.max(level - steps);
        }
        if lowers(self.mode) {
            top = top.min(level + steps);
        }
        (top != ground || self.clear_plants).then_some((top, false))
    }
}

/// How far past an area's outline the structures it moves may reach, in
/// blocks: two chunks.
const AREA_REACH: i32 = 2 * CHUNK as i32;

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
/// made ground is built of and topped with (air for the column's own); and
/// what a road puts there besides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shaped {
    pub top: i32,
    pub cleared: bool,
    pub fill: BlockId,
    pub top_block: BlockId,
    /// A slab of the top block laid over it: a half step.
    pub half: bool,
    /// Cleared only up to this block, not reached, and a roof of `roof` laid
    /// there where nothing holds one: a tunnel.
    pub open_to: Option<i32>,
    pub roof: BlockId,
    /// The ground over a tunnel: built up to this, the first air over it,
    /// as the layers before the road made it.
    pub over: Option<i32>,
    /// Nothing built up under its top block: a bridge's deck, and under it
    /// down to the ground, a pier of `pier` where it is not air.
    pub hollow: bool,
    pub pier: BlockId,
    /// Blocks of a kind over the top, as many as the number: a kerb.
    pub raise: (u32, BlockId),
    /// Water over the top up to this block, not reached: a canal.
    pub water: Option<(BlockId, i32)>,
}

/// A layer by its kind and its place among those of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerRef {
    Fit(u32),
    Road(u32),
}

/// Every world layer of a world, kept by the chunk column each reaches.
#[derive(Clone, Debug, Default)]
pub struct WorldLayers {
    fits: Vec<FitLayer>,
    roads: Vec<RoadLayer>,
    /// The layers reaching each chunk column, in the order they apply: fits,
    /// then roads.
    cells: HashMap<[i32; 2], Vec<LayerRef>>,
    /// The World Areas over the world, which say where its structures stand.
    areas: WorldAreas,
    /// A number for all of them, the same for the same layers.
    hash: u64,
}

impl PartialEq for WorldLayers {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.fits == other.fits && self.roads == other.roads && self.areas == other.areas
    }
}

impl WorldLayers {
    /// The layers of `fits`, in the order they apply: by priority, then id.
    pub fn new(fits: Vec<FitLayer>) -> Self {
        Self::of(fits, Vec::new())
    }

    /// The layers of `fits` and `roads`: fits first, then roads, each kind by
    /// priority, then id.
    pub fn of(mut fits: Vec<FitLayer>, mut roads: Vec<RoadLayer>) -> Self {
        // A layer with no id of its own is known by what it is: moved, it is another, and touches where it was too.
        for fit in fits.iter_mut().filter(|fit| fit.id == 0) {
            fit.id = crate::layers::stable_hash(&fit.bytes()).max(1);
        }
        for road in roads.iter_mut().filter(|road| road.id == 0) {
            road.id = crate::layers::stable_hash(&road.bytes()).max(1);
        }
        fits.sort_by_key(|fit| (fit.priority, fit.id));
        roads.sort_by_key(|road| (road.priority, road.id));
        let mut cells: HashMap<[i32; 2], Vec<LayerRef>> = HashMap::new();
        for (at, fit) in fits.iter().enumerate() {
            for column in fit.bounds().into_iter().flat_map(columns_of) {
                cells.entry(column).or_default().push(LayerRef::Fit(at as u32));
            }
        }
        for (at, road) in roads.iter().enumerate() {
            for column in road.bounds().into_iter().flat_map(columns_of) {
                cells.entry(column).or_default().push(LayerRef::Road(at as u32));
            }
        }
        let bytes: Vec<u8> =
            fits.iter().flat_map(FitLayer::bytes).chain(roads.iter().flat_map(RoadLayer::bytes)).collect();
        Self { fits, roads, cells, areas: WorldAreas::default(), hash: crate::layers::stable_hash(&bytes) }
    }

    /// These layers with the World Areas `areas` beside them.
    pub fn with_areas(mut self, mut areas: WorldAreas) -> Self {
        for area in areas.areas.iter_mut().filter(|area| area.id == 0) {
            area.id = crate::layers::stable_hash(&area.bytes()).max(1);
        }
        areas.areas.sort_by_key(|area| (area.priority, area.id));
        let mut bytes = self.hash.to_le_bytes().to_vec();
        areas.areas.iter().for_each(|area| bytes.extend(area.bytes()));
        bytes.extend(areas.painted.as_ref().map_or(0, |painted| painted.id).to_le_bytes());
        areas.origin.iter().chain([&areas.block_size]).for_each(|v| bytes.extend(v.to_le_bytes()));
        if !areas.is_empty() {
            self.hash = crate::layers::stable_hash(&bytes);
        }
        self.areas = areas;
        self
    }

    pub fn is_empty(&self) -> bool {
        self.fits.is_empty() && self.roads.is_empty() && self.areas.is_empty()
    }

    pub fn areas(&self) -> &WorldAreas {
        &self.areas
    }

    pub fn fits(&self) -> &[FitLayer] {
        &self.fits
    }

    pub fn roads(&self) -> &[RoadLayer] {
        &self.roads
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

    fn reaching(&self, column: [i32; 2]) -> impl Iterator<Item = LayerRef> + '_ {
        self.cells.get(&column).into_iter().flatten().copied()
    }

    fn fits_reaching(&self, column: [i32; 2]) -> impl Iterator<Item = &FitLayer> {
        self.reaching(column).filter_map(|layer| match layer {
            LayerRef::Fit(at) => Some(&self.fits[at as usize]),
            LayerRef::Road(_) => None,
        })
    }

    /// What its layers make of column `[x, z]`, in the world's own blocks,
    /// whose ground's top (the first block of air over it) is `ground`. None
    /// where none of them changes it.
    pub fn shape(&self, [x, z]: [i32; 2], ground: i32) -> Option<Shaped> {
        let side = CHUNK as i32;
        let mut shaped: Option<Shaped> = None;
        for layer in self.reaching([x.div_euclid(side), z.div_euclid(side)]) {
            let now = shaped.map_or(ground, |shaped| shaped.top);
            let was = shaped.unwrap_or(Shaped { top: ground, ..Shaped::default() });
            shaped = match layer {
                LayerRef::Fit(at) => fit_column(&self.fits[at as usize], [x, z], now, was).or(shaped),
                LayerRef::Road(at) => road_column(&self.roads[at as usize], [x, z], now, was).or(shaped),
            };
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
        let coarse = |y: i32| (y as f32 / scale as f32).round() as i32;
        // Far off, a tunnel, a canal or a kerb is too small to see: the road's top alone shows, and over a tunnel the
        // ground it goes through.
        let tunnelled = shaped.over.is_some();
        Some(Shaped {
            top: coarse(shaped.over.unwrap_or(shaped.top)),
            top_block: if tunnelled { AIR } else { shaped.top_block },
            cleared: shaped.cleared && !tunnelled,
            half: false,
            open_to: None,
            over: None,
            raise: (0, AIR),
            water: None,
            hollow: false,
            ..shaped
        })
    }

    /// Whether a layer keeps what the generator grows off column `[x, z]`,
    /// in the world's blocks.
    pub fn clears_plants(&self, [x, z]: [i32; 2]) -> bool {
        let side = CHUNK as i32;
        self.reaching([x.div_euclid(side), z.div_euclid(side)]).any(|layer| match layer {
            LayerRef::Fit(at) => {
                let fit = &self.fits[at as usize];
                fit.clear_plants && fit.distance([x, z]) <= fit.reach()
            }
            LayerRef::Road(at) => {
                let road = &self.roads[at as usize];
                road.clear_plants && road.shape([x, z], 0).is_some()
            }
        })
    }

    /// Whether anything of a layer reaches the box from `low` to `high`
    /// along x and z, both reached, in the world's blocks, that leaves out
    /// what the generator grows and builds.
    pub fn keeps_out(&self, low: [i32; 2], high: [i32; 2]) -> bool {
        let side = CHUNK as i32;
        let cell = |v: i32| v.div_euclid(side);
        let meets = |bounds: Option<[[i32; 2]; 2]>| {
            bounds.is_some_and(|[l, h]| l[0] <= high[0] && h[0] >= low[0] && l[1] <= high[1] && h[1] >= low[1])
        };
        for cz in cell(low[1])..=cell(high[1]) {
            for cx in cell(low[0])..=cell(high[0]) {
                let keeps = self.reaching([cx, cz]).any(|layer| match layer {
                    LayerRef::Fit(at) => {
                        let fit = &self.fits[at as usize];
                        (fit.clear_plants || fit.mode != GroundFit::Leave) && meets(fit.bounds())
                    }
                    LayerRef::Road(at) => meets(self.roads[at as usize].bounds()),
                });
                if keeps {
                    return true;
                }
            }
        }
        false
    }

    /// The chunk columns whose ground differs between `old` and `new`: under
    /// every layer added, taken away or changed, where it was and where it is.
    pub fn changed_columns(old: &WorldLayers, new: &WorldLayers) -> Vec<[i32; 2]> {
        let mut columns: Vec<[i32; 2]> = Vec::new();
        let mut differ = |was: Vec<(u64, Vec<u8>, Option<[[i32; 2]; 2]>)>,
                          now: Vec<(u64, Vec<u8>, Option<[[i32; 2]; 2]>)>| {
            let was_by: HashMap<u64, &Vec<u8>> = was.iter().map(|(id, bytes, _)| (*id, bytes)).collect();
            let now_by: HashMap<u64, &Vec<u8>> = now.iter().map(|(id, bytes, _)| (*id, bytes)).collect();
            for (id, bytes, bounds) in &was {
                if now_by.get(id) != Some(&bytes) {
                    columns.extend(bounds.iter().copied().flat_map(columns_of));
                }
            }
            for (id, bytes, bounds) in &now {
                if was_by.get(id) != Some(&bytes) {
                    columns.extend(bounds.iter().copied().flat_map(columns_of));
                }
            }
        };
        let fits = |layers: &WorldLayers| layers.fits.iter().map(|fit| (fit.id, fit.bytes(), fit.bounds())).collect();
        let roads =
            |layers: &WorldLayers| layers.roads.iter().map(|road| (road.id, road.bytes(), road.bounds())).collect();
        differ(fits(old), fits(new));
        differ(roads(old), roads(new));
        // An area moves the structures whose sites it holds, and they reach further than their sites.
        let areas = |layers: &WorldLayers| {
            let painted = layers.areas.painted_bounds();
            let grown = |[low, high]: [[i32; 2]; 2]| [low.map(|v| v - AREA_REACH), high.map(|v| v + AREA_REACH)];
            layers
                .areas
                .areas
                .iter()
                .map(|area| (area.id, area.bytes(), if area.painted == 0 { area.bounds() } else { painted }.map(grown)))
                .collect()
        };
        differ(areas(old), areas(new));
        columns.sort_unstable();
        columns.dedup();
        columns
    }

    /// The road nearest `at` along x and z, in blocks, within `reach`, and its
    /// point nearest there.
    pub fn nearest_road(&self, at: [f32; 2], reach: f32) -> Option<(&RoadLayer, [f32; 3])> {
        let distance = |point: [f32; 3]| ((point[0] - at[0]).powi(2) + (point[2] - at[1]).powi(2)).sqrt();
        self.roads
            .iter()
            .filter_map(|road| Some((road, road.nearest_point(at)?.0)))
            .filter(|(_, point)| distance(*point) <= reach)
            .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
    }
}

/// What `fit` makes of column `at` whose ground is now `now`, `was` what the
/// layers before it made there.
fn fit_column(fit: &FitLayer, at: [i32; 2], now: i32, was: Shaped) -> Option<Shaped> {
    let (top, levelled) = fit.shape(at, now)?;
    // Levelled ground takes the fit's own blocks; the eased edge the blocks under it already.
    let (fill, top_block) = match levelled {
        true if !fit.palette.is_empty() => (fit.fill, fit.palette.pick(fit.id, at, at[0], at[1])),
        true => (fit.fill, fit.top),
        false => (was.fill, was.top_block),
    };
    let cleared = was.cleared || fit.clear_plants || levelled && lowers(fit.mode) || top < now;
    Some(Shaped { top, cleared, fill, top_block, ..was })
}

/// What `road` makes of column `at` whose ground is now `now`, `was` what
/// the layers before it made there.
fn road_column(road: &RoadLayer, at: [i32; 2], now: i32, was: Shaped) -> Option<Shaped> {
    let column = road.shape(at, now)?;
    // Beside a tunnel the ground over it is left whole.
    let tunnelled = road.tunnel.is_some_and(|tunnel| now - column.level > tunnel.cover as i32);
    if column.on == OnRoad::Out && tunnelled {
        return None;
    }
    if column.on == OnRoad::Out {
        let cleared = was.cleared || road.clear_plants || column.top < now;
        return Some(Shaped { top: column.top, cleared, ..was });
    }
    let mut shaped = Shaped {
        top: column.top,
        cleared: true,
        fill: was.fill,
        top_block: column.block,
        half: column.half,
        ..Shaped::default()
    };
    // Through ground well over it, a tunnel; over ground well under it, a bridge on piers.
    if let Some(tunnel) = road.tunnel.filter(|tunnel| now - column.top > tunnel.cover as i32) {
        shaped.open_to = Some(column.top + tunnel.height.max(2) as i32);
        shaped.roof = tunnel.roof;
        shaped.over = Some(now);
    }
    if let Some(bridge) = road.bridge.filter(|bridge| column.top - now > bridge.over_water.max(1) as i32) {
        shaped.hollow = true;
        shaped.top_block = bridge.deck;
        let middle = (column.across - road.width_near(at) / 2).abs() <= 0;
        if bridge.pier_every > 0 && column.along.rem_euclid(bridge.pier_every as i32) == 0 && middle {
            shaped.pier = bridge.deck;
        }
    }
    if column.on == OnRoad::Edge && road.kerb > 0 {
        shaped.raise = (road.kerb, road.edge);
    }
    if let Some((water, depth)) = road.water.filter(|(water, _)| *water != AIR) {
        shaped.water = Some((water, column.top));
        shaped.top = column.top - depth.max(1) as i32;
        if column.on == OnRoad::Road {
            shaped.top_block = column.block;
        }
    }
    Some(shaped)
}

/// The chunk columns a box of blocks `[low, high]` covers.
fn columns_of([low, high]: [[i32; 2]; 2]) -> impl Iterator<Item = [i32; 2]> {
    let side = CHUNK as i32;
    let (l, h) = (low.map(|v| v.div_euclid(side)), high.map(|v| v.div_euclid(side)));
    (l[1]..=h[1]).flat_map(move |z| (l[0]..=h[0]).map(move |x| [x, z]))
}

/// Puts what `shaped` makes of a column into it, through `put` of its blocks
/// by height: built up to the new top with `block_at`'s blocks (by depth
/// under the new top, the fill and top given where they are not air), and
/// cleared over it to `highest` with `open`'s (air, or the sea where it
/// stands), where it is cleared or lowered; and a road's slab, tunnel, deck,
/// pier, kerb and water. `ground` is the column's top as the generator made
/// it, and `slab_of` a block's slab, air where it has none.
#[allow(clippy::too_many_arguments)]
pub fn shape_column(
    shaped: Shaped,
    ground: i32,
    highest: i32,
    block_at: &dyn Fn(f32) -> BlockId,
    open: &dyn Fn(i32) -> BlockId,
    slab_of: &dyn Fn(BlockId) -> BlockId,
    put: &mut dyn FnMut(i32, BlockId),
) {
    let Shaped { top, cleared, fill, top_block, half, open_to, roof, over, hollow, pier, raise, water } = shaped;
    let made = |y: i32| {
        let depth = top as f32 - (y as f32 + 0.5);
        match (depth < 1.0, fill, top_block) {
            (true, _, block) if block != AIR => block,
            (false, block, _) if block != AIR => block,
            _ => block_at(depth),
        }
    };
    // A tunnel's ground over it, built up as the layers before the road made it, its own layers by depth.
    if let Some(over) = over.filter(|over| *over > ground) {
        for y in ground - 1..over {
            put(y, block_at(over as f32 - (y as f32 + 0.5)));
        }
    }
    if cleared || top < ground {
        for y in top..open_to.unwrap_or(highest.max(ground) + 1) {
            put(y, open(y));
        }
        if let Some(roof_at) = open_to.filter(|_| roof != AIR) {
            put(roof_at, roof);
        }
    }
    if hollow {
        // A deck, and a pier under it down to the ground; nothing else built up.
        put(top - 1, made(top - 1));
        if pier != AIR {
            for y in (ground - 1).min(top - 1)..top - 1 {
                put(y, pier);
            }
        }
    } else {
        // Built up, from its old top block, buried now; or lowered, the new top's layers laid on what is left; or
        // level, only a top of its own laid.
        let relaid = 4;
        let laid = match top.cmp(&ground) {
            _ if over.is_some() => top - 1..top,
            std::cmp::Ordering::Greater => ground - 1..top,
            std::cmp::Ordering::Less => top - relaid..top,
            std::cmp::Ordering::Equal if top_block != AIR => top - 1..top,
            std::cmp::Ordering::Equal => 0..0,
        };
        for y in laid {
            put(y, made(y));
        }
    }
    let mut over = top;
    if half {
        let slab = slab_of(made(top - 1));
        if slab != AIR {
            put(top, slab);
            over += 1;
        }
    }
    for y in over..over + raise.0 as i32 {
        put(y, raise.1);
    }
    if let Some((water, level)) = water {
        for y in top..level {
            put(y, water);
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
            ..FitLayer::default()
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
    fn a_road_tunnels_through_high_ground_bridges_low_ground_and_holds_a_canals_water() {
        let road = RoadLayer {
            id: 9,
            points: vec![[0.0, 10.0, 0.5], [64.0, 10.0, 0.5]],
            widths: vec![5.0, 5.0],
            palette: Palette { mix: vec![(3, 1)], pattern: Vec::new() },
            edge: 7,
            kerb: 1,
            grade: Some(1.0),
            slope_out: 2,
            steps: Steps::Blocks,
            bridge: Some(Bridge { deck: 8, over_water: 2, pier_every: 8 }),
            tunnel: Some(Tunnel { cover: 3, height: 4, roof: 9 }),
            water: None,
            clear_plants: true,
            priority: 0,
        };
        let layers = WorldLayers::of(Vec::new(), vec![road.clone()]);
        let tunnel = layers.shape([10, 0], 30).unwrap();
        assert_eq!((tunnel.top, tunnel.open_to, tunnel.roof), (10, Some(14), 9));
        let bridge = layers.shape([16, 0], 2).unwrap();
        assert!(bridge.hollow && bridge.top_block == 8 && bridge.pier == 8, "{bridge:?}");
        let kerb = layers.shape([16, 2], 10).unwrap();
        assert_eq!(kerb.raise, (1, 7));
        // A column through the tunnel: cut out four high under its roof, the ground over it left.
        let mut column = vec![1u16; 30];
        column.extend([0u16; 4]);
        let mut put = |y: i32, block: BlockId| column[y as usize] = block;
        shape_column(tunnel, 30, 33, &|_| 2, &|_| AIR, &|_| AIR, &mut put);
        assert_eq!(&column[9..16], &[3, 0, 0, 0, 0, 9, 1]);
        // A canal: dug in by its depth, water up to the road's height.
        let canal =
            WorldLayers::of(Vec::new(), vec![RoadLayer { water: Some((5, 2)), tunnel: None, bridge: None, ..road }]);
        let dug = canal.shape([10, 0], 10).unwrap();
        assert_eq!((dug.top, dug.water), (8, Some((5, 10))));
        assert!(canal.nearest_road([20.0, 3.0], 4.0).is_some() && canal.nearest_road([20.0, 30.0], 4.0).is_none());
    }

    #[test]
    fn shaping_a_column_builds_up_and_clears() {
        let mut column = vec![1u16; 6];
        column.extend([0u16; 14]);
        let shaped = Shaped { top: 9, cleared: true, fill: 3, top_block: 4, ..Shaped::default() };
        let mut put = |y: i32, block: BlockId| column[y as usize] = block;
        shape_column(shaped, 6, 19, &|_| 2, &|_| AIR, &|_| AIR, &mut put);
        assert_eq!(&column[..12], &[1, 1, 1, 1, 1, 3, 3, 3, 4, 0, 0, 0]);
    }
}
