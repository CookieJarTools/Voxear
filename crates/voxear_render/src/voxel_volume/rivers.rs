//! Rivers in an endless Voxel Volume's ground: a generator's Rivers node
//! finds its rivers and lakes on the CPU, over a grid of points round each
//! place, which the GPU's program cannot. On the GPU the node passes its
//! ground through; here the CPU works out, for each column, how much the
//! rivers cut the ground down (or raise it, in a rim) at points eight
//! voxels apart, and how high their water stands and how far it falls. The
//! GPU adds that change, read smoothly between the points, to the height it
//! made, as a world of blocks takes its rivers from the CPU.
//!
//! The points are worked out on threads of their own as columns come into
//! the window, the nearest first, and kept; a column is made once its
//! points are. The ray the CPU walks through the ground
//! (`voxel_volume/ray.rs`) reads the same points, so it meets the ground
//! the GPU makes. The water itself is drawn from them too: a quad of water
//! on each square of points with water at a corner ([`water_quads`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex, Weak};

use voxear_terrain::generate::{WorldGen, nodes::NO_WATER};

use voxear_core::math::Vec3;

use super::{MOST_VOXELS_A_METRE, VoxelGenerator};

/// How many points a column's grid has a side: one every [`GRID_STEP`]
/// voxels, eight steps across the column's 64 voxels and one more each
/// side for the border its heights are made with.
pub(crate) const GRID: usize = 11;
pub(crate) const GRID_POINTS: usize = GRID * GRID;
pub(crate) const GRID_STEP: i32 = 8;

/// The most columns' points kept before those furthest from the window are
/// let go: about 1.5 KB each.
const MOST_KEPT: usize = 8192;

/// What the rivers make of one column, at each point of its grid from its
/// corner one step back along X and Z: how many metres they raise the
/// ground there (cut down where less than nothing), how high their water
/// stands, in metres of the generator's world ([`NO_WATER`] where there is
/// none), and how many metres of a waterfall pour down there.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RiverGrid {
    pub cut: [f32; GRID_POINTS],
    /// How high the generator's ground stands there with its rivers, in
    /// metres of its world: the bed under their water.
    pub ground: [f32; GRID_POINTS],
    pub water: [f32; GRID_POINTS],
    pub falls: [f32; GRID_POINTS],
    /// Whether the rivers change anything here: a cut or water anywhere.
    pub laid: bool,
}

impl RiverGrid {
    /// A grid where the rivers change nothing.
    pub fn dry() -> Self {
        Self {
            cut: [0.0; GRID_POINTS],
            ground: [0.0; GRID_POINTS],
            water: [NO_WATER; GRID_POINTS],
            falls: [0.0; GRID_POINTS],
            laid: false,
        }
    }

    /// How much the rivers raise the ground at voxel column (`x`, `z`) of
    /// the column, from its corner (-4 to 67 with its border), read between
    /// the four points round its middle as the GPU reads them
    /// (`riverCut` in `shaders/voxel_world.slang`).
    pub fn cut_at(&self, x: i32, z: i32) -> f32 {
        let (i, fx) = between(x);
        let (j, fz) = between(z);
        let at = |i: usize, j: usize| self.cut[j * GRID + i];
        let low = at(i, j) + (at(i + 1, j) - at(i, j)) * fx;
        let high = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * fx;
        low + (high - low) * fz
    }

    /// How high the highest water stands among the four points round voxel
    /// column (`x`, `z`) of the column, from its corner, as the GPU reads it
    /// (`riverAt`): [`NO_WATER`] where there is none.
    pub fn water_near(&self, x: i32, z: i32) -> f32 {
        let ((i, _), (j, _)) = (between(x), between(z));
        let at = |i: usize, j: usize| self.water[j * GRID + i];
        at(i, j).max(at(i + 1, j)).max(at(i, j + 1)).max(at(i + 1, j + 1))
    }
}

/// Which point a voxel column `x` of a column lies after along one axis,
/// and how far on from it to the next.
fn between(x: i32) -> (usize, f32) {
    let u = (x as f32 + 0.5) * (1.0 / GRID_STEP as f32) + 1.0;
    let i = u.floor().clamp(0.0, (GRID - 2) as f32);
    (i as usize, u - i)
}

/// Whether there is water at a height the rivers give.
pub(crate) fn wet(water: f32) -> bool {
    water > NO_WATER * 0.5
}

/// The columns' points kept, those asked for, nearest first, and those
/// being worked out.
#[derive(Default)]
struct Kept {
    grids: HashMap<[i32; 2], Arc<RiverGrid>>,
    asked: VecDeque<[i32; 2]>,
    working: HashSet<[i32; 2]>,
    /// The column asked for first last time: the window's middle.
    middle: [i32; 2],
    workers: bool,
}

/// A generator's rivers, laid into the columns of endless volumes of so
/// many voxels a metre, where its corner lies at `at` in the generator's
/// world: the generator, it without its rivers, and the columns' points.
pub(crate) struct RiverLayer {
    full: Arc<WorldGen>,
    bare: WorldGen,
    at: [f32; 2],
    voxel: f32,
    kept: Mutex<Kept>,
    wake: Condvar,
}

/// The layers made, shared by every volume laying the same rivers and by
/// the CPU's rays.
static LAYERS: Mutex<Vec<Weak<RiverLayer>>> = Mutex::new(Vec::new());

/// The rivers `generator` lays into an endless volume of `voxels_per_metre`:
/// none where its graph runs none.
pub(crate) fn layer_of(generator: &VoxelGenerator, voxels_per_metre: u32) -> Option<Arc<RiverLayer>> {
    if !generator.world.has_rivers() {
        return None;
    }
    let voxel = 1.0 / voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
    let mut layers = LAYERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    layers.retain(|layer| layer.strong_count() > 0);
    let same = |layer: &RiverLayer| {
        layer.voxel == voxel
            && layer.at == generator.at
            && (Arc::ptr_eq(&layer.full, &generator.world) || layer.full.hash() == generator.world.hash())
    };
    if let Some(found) = layers.iter().filter_map(Weak::upgrade).find(|layer| same(layer)) {
        return Some(found);
    }
    let layer = Arc::new(RiverLayer {
        full: generator.world.clone(),
        bare: generator.world.without_rivers(),
        at: generator.at,
        voxel,
        kept: Mutex::new(Kept::default()),
        wake: Condvar::new(),
    });
    layers.push(Arc::downgrade(&layer));
    Some(layer)
}

impl RiverLayer {
    /// The generator without its rivers, as the GPU makes its ground.
    pub fn bare(&self) -> &WorldGen {
        &self.bare
    }

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Column `column`'s points, where they are worked out already.
    pub fn ready(&self, column: [i32; 2]) -> Option<Arc<RiverGrid>> {
        self.kept().grids.get(&column).cloned()
    }

    /// Column `column`'s points, worked out now where they are not kept.
    pub fn grid(&self, column: [i32; 2]) -> Arc<RiverGrid> {
        if let Some(grid) = self.ready(column) {
            return grid;
        }
        let grid = Arc::new(self.make(column));
        self.keep(column, grid.clone());
        grid
    }

    /// Asks for `columns`' points, the first first, in place of what was
    /// asked for before: those not kept or being worked out are worked out
    /// on the layer's threads.
    pub fn ask(self: &Arc<Self>, columns: &[[i32; 2]]) {
        let mut kept = self.kept();
        if let Some(&first) = columns.first() {
            kept.middle = first;
        }
        let asked: VecDeque<[i32; 2]> = columns
            .iter()
            .filter(|column| !kept.grids.contains_key(*column) && !kept.working.contains(*column))
            .copied()
            .collect();
        kept.asked = asked;
        if !kept.workers && !kept.asked.is_empty() {
            kept.workers = true;
            let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 3).clamp(2, 6);
            for n in 0..threads {
                let layer = Arc::downgrade(self);
                let _ = std::thread::Builder::new().name(format!("Voxel rivers {n}")).spawn(move || work(&layer));
            }
        }
        drop(kept);
        self.wake.notify_all();
    }

    fn keep(&self, column: [i32; 2], grid: Arc<RiverGrid>) {
        let mut kept = self.kept();
        kept.working.remove(&column);
        kept.grids.insert(column, grid);
        if kept.grids.len() > MOST_KEPT {
            // The half furthest from the window go.
            let middle = kept.middle;
            let far = |c: &[i32; 2]| (c[0] - middle[0]).pow(2) + (c[1] - middle[1]).pow(2);
            let mut distances: Vec<i32> = kept.grids.keys().map(far).collect();
            distances.sort_unstable();
            let keep_within = distances[distances.len() / 2];
            kept.grids.retain(|column, _| far(column) <= keep_within);
        }
    }

    /// Works out column `column`'s points: the generator's height with its
    /// rivers and without, and its water and falls, at each.
    fn make(&self, column: [i32; 2]) -> RiverGrid {
        let v = self.voxel;
        let mut xs = Vec::with_capacity(GRID_POINTS);
        let mut zs = Vec::with_capacity(GRID_POINTS);
        for j in 0..GRID as i32 {
            for i in 0..GRID as i32 {
                xs.push(self.at[0] + (column[0] * 64 + (i - 1) * GRID_STEP) as f32 * v);
                zs.push(self.at[1] + (column[1] * 64 + (j - 1) * GRID_STEP) as f32 * v);
            }
        }
        let (mut cut, mut ground) = ([0.0; GRID_POINTS], [0.0; GRID_POINTS]);
        self.full.heights(&xs, &zs, &mut cut);
        let (water, falls) =
            self.full.water(&xs, &zs).unwrap_or_else(|| (vec![NO_WATER; GRID_POINTS], vec![0.0; GRID_POINTS]));
        self.bare.heights(&xs, &zs, &mut ground);
        let mut grid = RiverGrid::dry();
        for at in 0..GRID_POINTS {
            grid.ground[at] = cut[at];
            grid.cut[at] = cut[at] - ground[at];
            grid.water[at] = water[at];
            grid.falls[at] = falls[at];
        }
        grid.laid = grid.cut.iter().any(|&cut| cut != 0.0) || grid.water.iter().any(|&water| wet(water));
        grid
    }
}

/// How far under a river's bed a place still has its water over it, in
/// metres: the voxels of its bed, and a little more.
const UNDER_BED: f32 = 1.0;

/// The water of an endless Voxel Volume's rivers and lakes at a place: how
/// high its surface stands in the world, and which way and how fast its
/// current runs, in metres a second along X and Z (nothing where it lies
/// level, as a lake does), as [`WorldGen::current`] works it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelWaterPoint {
    pub height: f32,
    pub current: Vec3,
}

/// The water of the rivers and lakes of the endless volume `volume`, its
/// corner at `corner`, at `at` in the world, read between the points the
/// GPU's ground is cut and its water drawn from: its surface over the
/// place, under it, or as far over it as the ground lets it be, and its
/// current. None where its generator runs no rivers, where none has water
/// there, under the volume's floor, or more than a metre under the bed.
///
/// A square of points with water at a corner has water all over, its dry
/// corners as high as its highest wet one, as [`water_quads`] draws it.
pub fn water_at(volume: &super::VoxelVolume, corner: Vec3, at: Vec3) -> Option<VoxelWaterPoint> {
    let generator = volume.generator.as_ref().filter(|_| volume.endless)?;
    let layer = layer_of(generator, volume.voxels_per_metre)?;
    let voxel = layer.voxel;
    if at.y < corner.y {
        return None;
    }
    // In voxels from the volume's corner, along X and Z.
    let (x, z) = ((at.x - corner.x) / voxel, (at.z - corner.z) / voxel);
    let (water, bed) = layer.water_between(x, z)?;
    let height = |water: f32| corner.y + water - generator.floor;
    if at.y < height(bed) - UNDER_BED {
        return None;
    }
    // Its height a metre either way along each axis, for its current.
    let step = 1.0 / voxel;
    let side = |dx: f32, dz: f32| layer.water_between(x + dx, z + dz).map_or(NO_WATER, |(water, _)| water);
    let around = [water, side(-step, 0.0), side(step, 0.0), side(0.0, -step), side(0.0, step)];
    let [cx, cz] = voxear_terrain::generate::current_between(around, 1.0).unwrap_or([0.0; 2]);
    Some(VoxelWaterPoint { height: height(water), current: Vec3::new(cx, 0.0, cz) })
}

impl RiverLayer {
    /// How high the water stands at (`x`, `z`), in voxels from the
    /// volume's corner, and the bed under it, in metres of the generator's
    /// world, read between the four points round it: none where none of
    /// them has water.
    fn water_between(&self, x: f32, z: f32) -> Option<(f32, f32)> {
        let column = [(x / 64.0).floor() as i32, (z / 64.0).floor() as i32];
        let grid = self.grid(column);
        let along = |x: f32, column: i32| {
            let u = (x - (column * 64) as f32) / GRID_STEP as f32 + 1.0;
            let i = u.floor().clamp(0.0, (GRID - 2) as f32);
            (i as usize, u - i)
        };
        let ((i, fx), (j, fz)) = (along(x, column[0]), along(z, column[1]));
        let points = [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)].map(|(i, j)| j * GRID + i);
        let highest = points.iter().map(|&at| grid.water[at]).filter(|&water| wet(water)).fold(f32::MIN, f32::max);
        if highest == f32::MIN {
            return None;
        }
        let read = |values: [f32; 4]| {
            let low = values[0] + (values[1] - values[0]) * fx;
            let high = values[2] + (values[3] - values[2]) * fx;
            low + (high - low) * fz
        };
        let water = read(points.map(|at| if wet(grid.water[at]) { grid.water[at] } else { highest }));
        Some((water, read(points.map(|at| grid.ground[at]))))
    }
}

/// How many words a column's entry in the table the GPU reads takes
/// (`RIVER_WORDS` in `shaders/voxel_world.slang`): where it is, whether the
/// rivers change anything there, a word spare, then its cuts and its water.
const ENTRY_WORDS: usize = 4 + 2 * GRID_POINTS;

/// A volume's rivers as its columns are made with them: its layer, and the
/// table of points the GPU reads, a slot for each column of a window round
/// the volume's, `ring` columns wider each side, which column each holds
/// and the frame it was last in the window.
pub(crate) struct LaidRivers {
    pub layer: Arc<RiverLayer>,
    pub table: voxear_rhi::Buffer,
    pub window: u32,
    ring: i32,
    written: Vec<Option<[i32; 2]>>,
    seen: Vec<u64>,
    frame: u64,
}

impl LaidRivers {
    /// The table for a volume whose window is `window` columns a side, the
    /// trees of a column reaching `ring` columns round it.
    pub fn new(gpu: &voxear_rhi::Gpu, layer: Arc<RiverLayer>, window: u32, ring: u32) -> anyhow::Result<Self> {
        let window = window + 2 * ring;
        let slots = (window * window) as usize;
        let usage =
            voxear_rhi::vk::BufferUsageFlags::STORAGE_BUFFER | voxear_rhi::vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
        let bytes = (slots * ENTRY_WORDS * 4) as u64;
        let table = gpu.create_buffer("voxel world rivers", bytes, usage, voxear_rhi::MemoryLocation::CpuToGpu)?;
        // Every slot holds a column nowhere.
        let nowhere: Vec<u32> = (0..slots).flat_map(|_| [i32::MIN as u32, i32::MIN as u32, 0, 0]).collect();
        for (slot, words) in nowhere.chunks(4).enumerate() {
            table.write((slot * ENTRY_WORDS * 4) as u64, words);
        }
        Ok(Self { layer, table, window, ring: ring as i32, written: vec![None; slots], seen: vec![0; slots], frame: 0 })
    }

    fn slot_of(&self, column: [i32; 2]) -> usize {
        let w = self.window as i32;
        (column[1].rem_euclid(w) * w + column[0].rem_euclid(w)) as usize
    }

    /// Follows the volume's window, from column `least`, `window` a side,
    /// round the camera's column `middle`: asks for the points of the
    /// columns round it not worked out yet, the nearest first, and writes
    /// those worked out into the table, in slots no frame in flight may
    /// still read for the column they held.
    pub fn follow(&mut self, least: [i32; 2], window: u32, middle: [i32; 2]) {
        self.frame += 1;
        let (r, w) = (self.ring, window as i32 + 2 * self.ring);
        let mut missing = Vec::new();
        let mut waiting = Vec::new();
        for dz in 0..w {
            for dx in 0..w {
                let column = [least[0] - r + dx, least[1] - r + dz];
                let slot = self.slot_of(column);
                if self.written[slot] == Some(column) {
                    self.seen[slot] = self.frame;
                } else {
                    waiting.push(column);
                }
            }
        }
        for column in waiting {
            let slot = self.slot_of(column);
            let free =
                self.written[slot].is_none() || self.frame > self.seen[slot] + voxear_rhi::FRAMES_IN_FLIGHT as u64 + 1;
            match self.layer.ready(column) {
                Some(grid) if free => {
                    let mut words = Vec::with_capacity(ENTRY_WORDS);
                    words.extend([column[0] as u32, column[1] as u32, u32::from(grid.laid), 0]);
                    words.extend(grid.cut.iter().chain(&grid.water).map(|v| v.to_bits()));
                    self.table.write((slot * ENTRY_WORDS * 4) as u64, &words);
                    self.written[slot] = Some(column);
                    self.seen[slot] = self.frame;
                }
                Some(_) => {}
                None => missing.push(column),
            }
        }
        if !missing.is_empty() {
            let far = |c: &[i32; 2]| (c[0] - middle[0]).pow(2) + (c[1] - middle[1]).pow(2);
            missing.sort_unstable_by_key(far);
            self.layer.ask(&missing);
        }
    }

    /// Whether column `column` can be made: its points and those of the
    /// columns its trees may stand in are in the table.
    pub fn ready_for(&self, column: [i32; 2]) -> bool {
        (-self.ring..=self.ring).all(|dz| {
            (-self.ring..=self.ring).all(|dx| {
                let near = [column[0] + dx, column[1] + dz];
                self.written[self.slot_of(near)] == Some(near)
            })
        })
    }
}

/// A layer's thread: works out the columns asked for, the first first,
/// until the layer is let go.
fn work(layer: &Weak<RiverLayer>) {
    loop {
        let Some(strong) = layer.upgrade() else { return };
        let column = {
            let mut kept = strong.kept();
            match kept.asked.pop_front() {
                Some(column) => {
                    kept.working.insert(column);
                    Some(column)
                }
                None => {
                    let (_kept, _) = strong
                        .wake
                        .wait_timeout(kept, std::time::Duration::from_millis(250))
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    None
                }
            }
        };
        if let Some(column) = column {
            let grid = Arc::new(strong.make(column));
            strong.keep(column, grid);
        }
    }
}

/// A square of water over an endless volume's rivers and lakes, as
/// `voxelWaterVertex` in `shaders/water.slang` draws it: its least corner
/// along X and Z in the world and how wide it is; how high each corner is,
/// in the order (0, 0), (1, 0), (0, 1), (1, 1); which way and how fast it
/// flows, in metres a second; whether it is the face of a waterfall (bit
/// 0), and the sides of it, a still top, that a fall lands beside (bits 1
/// to 4: +x, -x, +z and -z).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVoxelWater {
    pub corner: [f32; 2],
    pub size: f32,
    pub fall: u32,
    pub heights: [f32; 4],
    pub flow: [f32; 2],
    pub pad: [f32; 2],
}

/// How much higher a waterfall's top is than its foot, in metres, for a
/// square to be drawn as its face.
const FALL_FACE: f32 = 0.3;

/// The water over column `column`'s squares of points, in the world, the
/// volume's corner at `corner` and its floor at `floor` metres of the
/// generator's world, its voxels `voxel` across: a quad on each square with
/// water at any corner, its dry corners as high as its highest wet one, so
/// the water reaches into the bank beside it and the ground hides it there.
pub(crate) fn water_quads(
    grid: &RiverGrid,
    column: [i32; 2],
    (corner, floor, voxel): (voxear_core::math::Vec3, f32, f32),
) -> Vec<GpuVoxelWater> {
    if !grid.water.iter().any(|&water| wet(water)) {
        return Vec::new();
    }
    let step = GRID_STEP as f32 * voxel;
    let at = |i: usize, j: usize| grid.water[j * GRID + i];
    let height = |water: f32| corner.y + water - floor;
    // How high each square's corners stand, where it has water.
    let square = |i: usize, j: usize| -> Option<[f32; 4]> {
        let corners = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
        let highest = corners.into_iter().filter(|&water| wet(water)).fold(f32::MIN, f32::max);
        (highest > f32::MIN).then(|| corners.map(|water| height(if wet(water) { water } else { highest })))
    };
    let falling = |i: usize, j: usize| {
        square(i, j).is_some_and(|h| {
            let (low, high) = h.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
            let fell =
                [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)].iter().any(|&(a, b)| grid.falls[b * GRID + a] > 0.0);
            fell && high - low > FALL_FACE
        })
    };
    let mut quads = Vec::new();
    // The column's own squares: from its corner's point to the far corner's.
    for j in 1..GRID - 2 {
        for i in 1..GRID - 2 {
            let Some(heights) = square(i, j) else { continue };
            let mut fall = u32::from(falling(i, j));
            if fall == 0 {
                // A still top beside a fall's face, where the fall lands: the sides it lies beside, and lower.
                let low = heights.iter().copied().fold(f32::MAX, f32::min);
                let sides = [(i + 1, j), (i.wrapping_sub(1), j), (i, j + 1), (i, j.wrapping_sub(1))];
                for (bit, &(a, b)) in sides.iter().enumerate() {
                    if a < GRID - 1
                        && b < GRID - 1
                        && falling(a, b)
                        && square(a, b).is_some_and(|h| h.iter().any(|&h| h > low + FALL_FACE))
                    {
                        fall |= 2 << bit;
                    }
                }
            }
            // Down the water's slope, faster the steeper, as a generator's current runs, read over three squares
            // along each axis where they have water. Water falling less than 3 mm a metre lies still; the way it
            // flows is one of sixteen and its speed in half metres a second, so a stretch of river flows as one and
            // its ripples run on unbroken from square to square.
            let level = |i: usize, j: usize| Some(at(i, j)).filter(|&water| wet(water));
            let across = |a: usize, b: usize, other: usize, x: bool| {
                let (low, high) =
                    if x { (level(a, other), level(b, other)) } else { (level(other, a), level(other, b)) };
                low.zip(high).map(|(low, high)| (high - low) / ((b - a) as f32 * step))
            };
            let slope = |x: bool, along: usize, side: usize| {
                let (from, to) = (along.saturating_sub(1), (along + 2).min(GRID - 1));
                let rows = [side, side + 1]
                    .map(|other| across(from, to, other, x).or_else(|| across(along, along + 1, other, x)));
                let found: Vec<f32> = rows.into_iter().flatten().collect();
                if found.is_empty() { 0.0 } else { found.iter().sum::<f32>() / found.len() as f32 }
            };
            let (gx, gz) = (slope(true, i, j), slope(false, j, i));
            let steep = (gx * gx + gz * gz).sqrt();
            let flow = match steep < 0.003 || fall & 1 != 0 {
                true => [0.0, 0.0],
                false => {
                    let speed = ((6.0 * steep.sqrt()).clamp(0.4, 4.0) * 2.0).round() / 2.0;
                    let way = ((-gz).atan2(-gx) / std::f32::consts::TAU * 16.0).round() * std::f32::consts::TAU / 16.0;
                    [way.cos() * speed, way.sin() * speed]
                }
            };
            let x = corner.x + (column[0] * 64 + (i as i32 - 1) * GRID_STEP) as f32 * voxel;
            let z = corner.z + (column[1] * 64 + (j as i32 - 1) * GRID_STEP) as f32 * voxel;
            quads.push(GpuVoxelWater { corner: [x, z], size: step, fall, heights, flow, pad: [0.0; 2] });
        }
    }
    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_is_read_between_its_points_from_its_border_to_its_far_side() {
        let mut grid = RiverGrid::dry();
        for j in 0..GRID {
            for i in 0..GRID {
                grid.cut[j * GRID + i] = i as f32 + 100.0 * j as f32;
            }
        }
        // A voxel's middle half a voxel past a point reads a sixteenth of the way on.
        assert_eq!(grid.cut_at(0, 0), 1.0 + 1.0 / 16.0 + 100.0 * (1.0 + 1.0 / 16.0));
        // The border's first and the far side's last read inside the grid.
        assert!((grid.cut_at(-4, -4) - (0.5 + 1.0 / 16.0) * 101.0).abs() < 1e-3);
        assert!((grid.cut_at(67, 67) - (9.0 + 3.5 / 8.0) * 101.0).abs() < 1e-3);
    }

    #[test]
    fn water_reaches_into_the_bank_and_a_fall_is_drawn_as_its_face() {
        let mut grid = RiverGrid::dry();
        // A river along X, three points wide, falling 3 m between its fifth and sixth points.
        for j in 4..7 {
            for i in 0..GRID {
                let at = j * GRID + i;
                grid.water[at] = if i <= 5 { 10.0 } else { 7.0 };
                grid.falls[at] = if i == 5 { 3.0 } else { 0.0 };
            }
        }
        grid.laid = true;
        let quads = water_quads(&grid, [0, 0], (voxear_core::math::Vec3::ZERO, 0.0, 1.0 / 16.0));
        // Squares 3 to 6 along Z have water at a corner: 4 rows of 8.
        assert_eq!(quads.len(), 32);
        let faces: Vec<_> = quads.iter().filter(|quad| quad.fall & 1 != 0).collect();
        assert!(!faces.is_empty() && faces.iter().all(|quad| quad.corner[0] == 2.0));
        assert!(quads.iter().any(|quad| quad.fall & 4 != 0), "the pool beside the fall's foot churns");
        // A dry corner stands as high as the wet one beside it.
        let edge = quads.iter().find(|quad| quad.corner == [0.0, 1.0]).expect("a square on the bank");
        assert_eq!(edge.heights, [10.0; 4]);
    }
}
