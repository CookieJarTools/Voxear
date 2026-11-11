//! A landmass: continents, ranges and gullies made from a seed in one piece.
//! It was the whole of a world generator before generators were graphs (see
//! `super`), is what a `.worldgen` file written then still reads as, and is
//! one of a graph's nodes now, making the same ground to the bit.
//!
//! Plain noise makes ground equally bumpy everywhere, with no plains beside
//! mountains and no valleys that lead anywhere. A landmass instead reads a
//! few broad fields over the world, each meaning one thing, and shapes the
//! ground from them by curves:
//!
//! - **Continents**: from deep sea, through the coast, to far inland.
//! - **Ruggedness**: from smooth country to rugged.
//! - **Ridges**: ridged noise, from the valleys between ranges to their
//!   crests, which run in long lines that wind and branch.
//!
//! The ground stands at the **Coast** curve's height for its place across
//! the continents, and rises and falls from there by its relief (the
//! **Inland** curve, how much relief land so far inland has, times the
//! **Relief** curve, how much its ruggedness gives it) as the **Peaks**
//! curve shapes its peaks and valleys. Crags roughen high peaks, and bumps
//! the ground everywhere. Every field is read through the same bending of
//! the world (its warp), so coasts and ranges wind rather than run straight.
//!
//! Then water cuts gullies down its slopes (`eroded`): in each of a few
//! octaves, stripes run down the slope the ground has there, cut deeper the
//! steeper it is, each octave finer and following the ground as the last
//! left it, so small gullies run down the walls of larger ones and join
//! them, as streams join a river. Ridges and valley floors, where the
//! ground is level across, are left as they were.
//!
//! Its world has a climate too ([`Landmass::climate`], a graph's Climate
//! node): how warm and how wet it is at each place, over fields larger than
//! its continents, and colder the higher the ground stands. Its terrain's
//! surfaces can cover the ground by it, so biomes follow the world: desert
//! where it is hot and dry, forest where it is mild and wet, snow on the
//! high peaks.
//!
//! It is all sums, products, square roots and `floor`, with no trigonometry
//! or powers, so a seed makes the same ground on every machine, and a world
//! made from one need only send its seed.

use serde::{Deserialize, Serialize};
use voxear_core::curve::Curve;

use crate::noise::{self, NoiseStyle};

/// A landmass: its seed, how large and high its ground is, and how it is
/// shaped; with its world's climate, as a `.worldgen` file once kept them
/// together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Landmass {
    pub seed: u32,
    /// How large everything is: 1 for each field's own size, 2 for twice
    /// as large.
    pub scale: f32,
    /// The sea's height, in metres: where the Coast curve's 0 is.
    pub sea_level: f32,
    /// How far the curves' 1 is above the sea, in metres: the height of
    /// the highest mountains.
    pub height: f32,
    pub continents: Field,
    pub ruggedness: Field,
    pub ridges: Field,
    /// How sharp ridges' crests are: 0 rounded, 1 knife-edged.
    pub sharpness: f32,
    pub warp: Warp,
    /// The ground's height across the continents, from the deepest sea (0)
    /// to furthest inland (1), as a share of `height`.
    pub coast: Curve,
    /// How much relief the land has across the continents, 0 to 1.
    pub inland: Curve,
    /// How much relief it has from smooth (0) to rugged (1), 0 to 1.
    pub relief: Curve,
    /// How its relief rises and falls from valleys (0) to peaks (1), from
    /// -1 to 1.
    pub peaks: Curve,
    /// How rough high peaks are, as a share of `height`.
    pub crags: f32,
    pub erosion: Erosion,
    pub climate: Climate,
    /// How high the small bumps over all the ground are, in metres.
    pub bumps: f32,
}

/// One of a generator's fields: how wide its largest features are, in
/// metres, and in how many octaves, each half as large.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub size: f32,
    pub octaves: u32,
}

/// How deeply water cuts gullies down a generator's slopes: 0 for none, 1
/// for gullies a quarter as deep as they are apart on steep ground; how far
/// apart its largest gullies are, in metres; and in how many octaves, each
/// half as far apart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Erosion {
    pub strength: f32,
    pub spacing: f32,
    pub octaves: u32,
}

/// How warm and wet a generator's world is: its temperature where the sea
/// is, in °C, and its humidity, 0 for dry to 1 for wet, each on average and
/// how far either way it strays from that, over fields `size` metres
/// across; and how many °C it cools for every 1000 m the ground rises
/// above the sea.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Climate {
    pub size: f32,
    pub temperature: f32,
    pub temperature_range: f32,
    pub humidity: f32,
    pub humidity_range: f32,
    pub cooling: f32,
}

impl Default for Climate {
    fn default() -> Self {
        Self {
            size: 5000.0,
            temperature: 12.0,
            temperature_range: 8.0,
            humidity: 0.5,
            humidity_range: 0.3,
            cooling: 6.5,
        }
    }
}

impl Climate {
    /// How warm it is on ground `above` metres over the sea where it is
    /// `temperature` °C at the sea.
    pub fn cooled(&self, temperature: f32, above: f32) -> f32 {
        temperature - self.cooling * above.max(0.0) / 1000.0
    }

    /// Its field `part` at `spot` of a world with `seed` and `scale`, -1 to 1.
    fn read(&self, spot: Spot, part: u32, seed: u32, scale: f32) -> f32 {
        let size = (self.size * scale.max(0.01)).max(1.0);
        signed(spot.over(size).fractal(NoiseStyle::Fractal, 3, seed_of(seed, part)))
    }

    /// How warm it is at `spot` of a world with `seed` and `scale`, in °C,
    /// on ground `above` metres over the sea.
    pub(crate) fn warmth(&self, spot: Spot, above: f32, seed: u32, scale: f32) -> f32 {
        let warmth = self.temperature + self.read(spot, 8, seed, scale) * self.temperature_range;
        self.cooled(warmth, above)
    }

    /// How wet it is at `spot` of a world with `seed` and `scale`, 0 to 1.
    pub(crate) fn wetness(&self, spot: Spot, seed: u32, scale: f32) -> f32 {
        (self.humidity + self.read(spot, 9, seed, scale) * self.humidity_range).clamp(0.0, 1.0)
    }
}

/// How many degrees colder than its middle a planet is at `at`, in metres
/// from its middle: up to `POLES_COLDER` at its poles, along y.
pub(crate) fn colder_towards_poles(at: [f32; 3]) -> f32 {
    let length = at.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let toward_pole = at[1] / length;
    POLES_COLDER * toward_pole * toward_pole
}

/// How a generator bends the world before it reads its fields: over how
/// many metres, and how many metres at most.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Warp {
    pub size: f32,
    pub strength: f32,
}

/// What a generator reads and makes at a point: its fields there, each
/// from -1 to 1 (peaks and valleys, folded from its ridges), and the
/// ground's height, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub continents: f32,
    pub ruggedness: f32,
    pub peaks: f32,
    pub height: f32,
}

/// Ground a generator starts from, to change from there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Wide, nearly level farmland rolling gently, no sea: the ground for
    /// a battle.
    BattlefieldPlains,
    /// Hills and dales, no sea.
    RollingHills,
    /// Ranges of sharp, high mountains with deep valleys, coasts at their
    /// feet.
    Alpine,
    /// Islands scattered over a sea.
    Archipelago,
    /// High, flat plateaus cut by steep-sided canyons.
    Canyons,
}

impl Preset {
    pub const ALL: [Preset; 5] =
        [Preset::BattlefieldPlains, Preset::RollingHills, Preset::Alpine, Preset::Archipelago, Preset::Canyons];

    pub fn name(self) -> &'static str {
        match self {
            Preset::BattlefieldPlains => "Battlefield Plains",
            Preset::RollingHills => "Rolling Hills",
            Preset::Alpine => "Alpine",
            Preset::Archipelago => "Archipelago",
            Preset::Canyons => "Canyons",
        }
    }

    /// The generator it is, with seed 1.
    pub fn world(self) -> super::WorldGen {
        self.landmass().into()
    }

    /// The landmass it is, with seed 1.
    pub fn landmass(self) -> Landmass {
        let field = |size: f32, octaves: u32| Field { size, octaves };
        let warp = |size: f32, strength: f32| Warp { size, strength };
        let erosion = |strength: f32, spacing: f32, octaves: u32| Erosion { strength, spacing, octaves };
        let base = Landmass {
            seed: 1,
            scale: 1.0,
            sea_level: 0.0,
            height: 120.0,
            continents: field(4000.0, 4),
            ruggedness: field(1500.0, 3),
            ridges: field(1100.0, 3),
            sharpness: 0.3,
            warp: warp(900.0, 160.0),
            coast: Curve::new(&[[0.0, 0.35], [1.0, 0.5]]),
            inland: Curve::flat(1.0),
            relief: Curve::new(&[[0.0, 0.15], [0.5, 0.45], [1.0, 0.8]]),
            peaks: Curve::new(&[[0.0, -0.4], [0.5, 0.1], [1.0, 0.6]]),
            crags: 0.0,
            erosion: erosion(0.35, 160.0, 3),
            climate: Climate { humidity: 0.6, ..Climate::default() },
            bumps: 1.5,
        };
        match self {
            Preset::RollingHills => base,
            Preset::BattlefieldPlains => Landmass {
                height: 40.0,
                ruggedness: field(1200.0, 3),
                ridges: field(900.0, 2),
                sharpness: 0.2,
                warp: warp(700.0, 100.0),
                coast: Curve::new(&[[0.0, 0.2], [1.0, 0.3]]),
                relief: Curve::new(&[[0.0, 0.05], [0.6, 0.2], [1.0, 0.4]]),
                peaks: Curve::new(&[[0.0, -0.3], [0.5, 0.0], [1.0, 0.4]]),
                erosion: erosion(0.15, 120.0, 2),
                climate: Climate { humidity: 0.55, ..Climate::default() },
                bumps: 0.8,
                ..base
            },
            Preset::Alpine => Landmass {
                height: 900.0,
                continents: field(6000.0, 4),
                ruggedness: field(2500.0, 3),
                ridges: field(2600.0, 3),
                sharpness: 0.9,
                warp: warp(2000.0, 300.0),
                coast: Curve::new(&[[0.0, -0.3], [0.35, -0.05], [0.45, 0.03], [0.6, 0.15], [1.0, 0.3]]),
                inland: Curve::new(&[[0.0, 0.0], [0.4, 0.0], [0.5, 0.3], [0.7, 0.9], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.2], [0.4, 0.5], [1.0, 1.0]]),
                peaks: Curve::new(&[[0.0, -0.12], [0.3, -0.05], [0.6, 0.35], [0.85, 0.75], [1.0, 0.85]]),
                crags: 0.12,
                erosion: erosion(0.7, 260.0, 4),
                climate: Climate { temperature: 8.0, humidity: 0.6, size: 8000.0, ..Climate::default() },
                bumps: 2.0,
                ..base
            },
            Preset::Archipelago => Landmass {
                height: 140.0,
                continents: field(2500.0, 4),
                ruggedness: field(1200.0, 3),
                ridges: field(900.0, 3),
                sharpness: 0.6,
                warp: warp(1000.0, 220.0),
                coast: Curve::new(&[[0.0, -0.6], [0.5, -0.25], [0.65, 0.0], [0.75, 0.08], [1.0, 0.3]]),
                inland: Curve::new(&[[0.0, 0.0], [0.6, 0.0], [0.75, 0.6], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.3], [1.0, 0.8]]),
                peaks: Curve::new(&[[0.0, -0.2], [0.5, 0.1], [1.0, 0.6]]),
                crags: 0.03,
                erosion: erosion(0.5, 160.0, 4),
                climate: Climate { temperature: 24.0, temperature_range: 4.0, humidity: 0.75, ..Climate::default() },
                bumps: 1.0,
                ..base
            },
            Preset::Canyons => Landmass {
                height: 220.0,
                continents: field(5000.0, 3),
                ruggedness: field(2000.0, 3),
                ridges: field(1400.0, 4),
                sharpness: 0.8,
                warp: warp(1200.0, 220.0),
                coast: Curve::new(&[[0.0, 0.9], [1.0, 1.0]]),
                relief: Curve::new(&[[0.0, 0.4], [1.0, 0.9]]),
                peaks: Curve::new(&[[0.0, 0.2], [0.6, 0.15], [0.72, -0.2], [0.85, -0.9], [1.0, -1.0]]),
                erosion: erosion(0.6, 120.0, 3),
                climate: Climate { temperature: 22.0, humidity: 0.15, humidity_range: 0.15, ..Climate::default() },
                bumps: 1.0,
                ..base
            },
        }
    }
}

impl Default for Landmass {
    fn default() -> Self {
        Preset::RollingHills.landmass()
    }
}

/// How far noise's usual spread is stretched to fill -1 to 1: fractal
/// noise seldom strays far from its middle.
const SPREAD: f32 = 2.6;

/// Where ridged noise's usual values sit, and how far they are stretched to
/// fill -1 to 1, from the valleys between ridges to their crests.
const RIDGE_MIDDLE: f32 = 0.45;
const RIDGE_SPREAD: f32 = 2.4;

/// Noise from 0 to 1 as -1 to 1, stretched by `SPREAD` and held within.
pub(crate) fn signed(value: f32) -> f32 {
    ((value - 0.5) * 2.0 * SPREAD).clamp(-1.0, 1.0)
}

/// How many degrees colder a planet's poles are than its middle.
pub const POLES_COLDER: f32 = 30.0;

/// Where a world generator reads its fields: a point of its flat world, in
/// metres along x and z, or of a round one's surface, in metres from its
/// middle, read in three dimensions so a planet has no seams.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Spot {
    Flat(f32, f32),
    Round([f32; 3]),
}

impl Spot {
    /// The same spot measured in units `size` metres long.
    fn over(self, size: f32) -> Self {
        match self {
            Spot::Flat(x, z) => Spot::Flat(x / size, z / size),
            Spot::Round(at) => Spot::Round(at.map(|v| v / size)),
        }
    }

    fn times(self, by: f32) -> Self {
        match self {
            Spot::Flat(x, z) => Spot::Flat(x * by, z * by),
            Spot::Round(at) => Spot::Round(at.map(|v| v * by)),
        }
    }

    /// Fractal noise of `style` here, 0 to 1.
    fn fractal(self, style: NoiseStyle, octaves: u32, seed: u32) -> f32 {
        match self {
            Spot::Flat(x, z) => noise::fractal(style, x, z, octaves, seed),
            Spot::Round(at) => noise::fractal3(style, at, octaves, seed),
        }
    }

    /// Gradient noise here, about -1 to 1.
    fn gradient(self, seed: u32) -> f32 {
        match self {
            Spot::Flat(x, z) => noise::gradient(x, z, seed),
            Spot::Round([x, y, z]) => noise::gradient3(x, y, z, seed),
        }
    }

    /// The spot moved by `bend(part)` metres along each of its axes.
    fn bent(self, bend: impl Fn(u32) -> f32) -> Self {
        match self {
            Spot::Flat(x, z) => Spot::Flat(x + bend(1), z + bend(2)),
            Spot::Round([x, y, z]) => Spot::Round([x + bend(1), y + bend(10), z + bend(2)]),
        }
    }
}

/// Ridged fractal noise at `spot`, 0 to 1, as `noise::fractal` makes it,
/// but each octave's crease rounded off where the noise under it is within
/// `soft` of its middle, so crests can be rounded.
fn ridged(spot: Spot, octaves: u32, seed: u32, soft: f32) -> f32 {
    let (mut sum, mut total, mut amplitude, mut frequency, mut weight) = (0.0, 0.0, 1.0, 1.0, 1.0f32);
    for octave in 0..octaves.clamp(1, 12) {
        let n = spot.times(frequency).gradient(seed.wrapping_add(octave.wrapping_mul(0x632B_E5AB)));
        // |n|, rounded into a curve within `soft` of the middle, the same as it outside.
        let crease = if n.abs() < soft { (n * n / soft + soft) * 0.5 } else { n.abs() };
        let ridge = (1.0 - crease).max(0.0);
        let ridge = ridge * ridge * weight;
        weight = (ridge * 2.0).clamp(0.0, 1.0);
        sum += ridge * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (sum / total).clamp(0.0, 1.0)
}

/// Two directions along a planet's surface at `place`, in metres from its
/// middle: each of length 1, at right angles to each other and to the way
/// out.
pub(crate) fn tangents([x, y, z]: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let length = (x * x + y * y + z * z).sqrt();
    if length <= 1e-6 {
        return ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    }
    let out = [x / length, y / length, z / length];
    // The first lies across the way out and y, or across x near a pole, where the way out is nearly y.
    let across = if out[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
    let first = cross(across, out);
    let size = (first[0] * first[0] + first[1] * first[1] + first[2] * first[2]).sqrt();
    let first = [first[0] / size, first[1] / size, first[2] / size];
    (first, cross(out, first))
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// A number for lattice cell `(x, z)`, well mixed.
fn cell_hash(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x8DA6_B343) ^ (z as u32).wrapping_mul(0xD816_3841);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// How far, in cells, a cell's gullies reach round its point.
const GULLY_REACH: f32 = 1.5;

/// Gullies at `(x, z)` (in cells, a gully's width each) running along
/// `along`, a direction in x and z: -1 on their floors, rising to 1 between
/// them, V-shaped at the floor and rounded between; and how that changes
/// along x and z, per cell. Each cell has a point of its own, its gullies'
/// stripes laid round it at a phase of its own, and blended with its
/// neighbours' as they reach, so gullies wander and end rather than run in
/// rows. With `joined`, how it changes eases to nothing on the floors, where
/// it would turn about from one wall to the other.
fn gullies(x: f32, z: f32, along: [f32; 2], seed: u32, joined: bool) -> (f32, f32, f32) {
    let across = [-along[1], along[0]];
    let reach2 = GULLY_REACH * GULLY_REACH;
    let (mut sum, mut weights) = (0.0f32, 0.0f32);
    let (mut dsum, mut dweights) = ([0.0f32; 2], [0.0f32; 2]);
    let (x0, x1) = ((x - GULLY_REACH).floor() as i32, (x + GULLY_REACH).floor() as i32);
    let (z0, z1) = ((z - GULLY_REACH).floor() as i32, (z + GULLY_REACH).floor() as i32);
    for cz in z0..=z1 {
        for cx in x0..=x1 {
            let h = cell_hash(cx, cz, seed);
            let jitter = |bits: u32| (bits & 0xFFFF) as f32 / 65536.0;
            let (px, pz) = (cx as f32 + jitter(h), cz as f32 + jitter(h >> 16));
            let (dx, dz) = (x - px, z - pz);
            let r2 = dx * dx + dz * dz;
            if r2 >= reach2 {
                continue;
            }
            let t = 1.0 - r2 / reach2;
            let w = t * t * t;
            let dw = [-6.0 * t * t * dx / reach2, -6.0 * t * t * dz / reach2];
            let phase = jitter(cell_hash(cx, cz, seed ^ 0x5bd1_e995));
            let s = dx * across[0] + dz * across[1] + phase;
            let u = s - s.floor();
            let wave = 8.0 * u * (1.0 - u) - 1.0;
            let mut slope = 8.0 - 16.0 * u;
            if joined {
                // Whole from an eighth of the way up either wall.
                let up = if u < 0.5 { u } else { 1.0 - u };
                if up < 0.125 {
                    slope *= up * 8.0;
                }
            }
            sum += w * wave;
            weights += w;
            for axis in 0..2 {
                dsum[axis] += dw[axis] * wave + w * slope * across[axis];
                dweights[axis] += dw[axis];
            }
        }
    }
    if weights <= 1e-6 {
        return (0.0, 0.0, 0.0);
    }
    let value = sum / weights;
    let d = |axis: usize| (dsum[axis] * weights - sum * dweights[axis]) / (weights * weights);
    (value, d(0), d(1))
}

/// A number for lattice cell `(x, y, z)`, well mixed.
fn cell_hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    cell_hash(x, z, seed ^ (y as u32).wrapping_mul(0xC2B2_AE3D).rotate_left(13))
}

/// How far, in cells, a cell's gullies reach round its point where cells
/// fill space: a little further than across a cell from corner to corner,
/// so some cell's point is always within reach.
const GULLY_REACH_ROUND: f32 = 1.75;

/// Gullies at `at` (in cells, a gully's width each) on a planet's surface,
/// running along `along`, a direction along the ground there, `across` the
/// direction along the ground at right angles to it: as [`gullies`] makes
/// them, with cells that fill space in place of a plane, so the stripes
/// wrap a planet with no seam. How it changes is along each of the three
/// axes, per cell.
fn gullies_round(at: [f32; 3], across: [f32; 3], seed: u32, joined: bool) -> (f32, [f32; 3]) {
    let reach2 = GULLY_REACH_ROUND * GULLY_REACH_ROUND;
    let (mut sum, mut weights) = (0.0f32, 0.0f32);
    let (mut dsum, mut dweights) = ([0.0f32; 3], [0.0f32; 3]);
    let first = at.map(|v| (v - GULLY_REACH_ROUND).floor() as i32);
    let last = at.map(|v| (v + GULLY_REACH_ROUND).floor() as i32);
    let jitter = |bits: u32| (bits & 0xFFFF) as f32 / 65536.0;
    for cz in first[2]..=last[2] {
        for cy in first[1]..=last[1] {
            for cx in first[0]..=last[0] {
                let h = cell_hash3(cx, cy, cz, seed);
                let more = cell_hash3(cx, cy, cz, seed ^ 0x5bd1_e995);
                let point = [cx as f32 + jitter(h), cy as f32 + jitter(h >> 16), cz as f32 + jitter(more >> 16)];
                let d = [at[0] - point[0], at[1] - point[1], at[2] - point[2]];
                let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                if r2 >= reach2 {
                    continue;
                }
                let t = 1.0 - r2 / reach2;
                let w = t * t * t;
                let s = d[0] * across[0] + d[1] * across[1] + d[2] * across[2] + jitter(more);
                let u = s - s.floor();
                let wave = 8.0 * u * (1.0 - u) - 1.0;
                let mut slope = 8.0 - 16.0 * u;
                if joined {
                    // Whole from an eighth of the way up either wall.
                    let up = if u < 0.5 { u } else { 1.0 - u };
                    if up < 0.125 {
                        slope *= up * 8.0;
                    }
                }
                sum += w * wave;
                weights += w;
                for axis in 0..3 {
                    let dw = -6.0 * t * t * d[axis] / reach2;
                    dsum[axis] += dw * wave + w * slope * across[axis];
                    dweights[axis] += dw;
                }
            }
        }
    }
    if weights <= 1e-6 {
        return (0.0, [0.0; 3]);
    }
    let d = |axis: usize| (dsum[axis] * weights - sum * dweights[axis]) / (weights * weights);
    (sum / weights, [d(0), d(1), d(2)])
}

/// Gullies cut down the slopes of any ground: how deeply, how far apart
/// and in how many octaves; in a world of this scale and sea level; from
/// this seed. A landmass cuts its own with them, and a graph's Gullies node
/// whatever height is wired into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Gullied {
    pub erosion: Erosion,
    pub scale: f32,
    pub sea_level: f32,
    pub seed: u32,
    /// Whether each octave's gullies join the floors of the octave before
    /// without a step. An octave runs down the slope the one before left,
    /// and that slope turns about across a gully's floor, from one wall's to
    /// the other's: so the next octave's gullies meet there out of step,
    /// which leaves a step in the ground along the floor. Joined, the slope
    /// they follow eases to the floor's own as they near it, so they turn
    /// to run along it and meet. A landmass's are not joined: its ground is
    /// what it was.
    pub joined: bool,
}

impl Gullied {
    /// How far apart the largest gullies are, in metres of the world.
    fn spacing(&self) -> f32 {
        (self.erosion.spacing * self.scale.max(0.01)).max(1.0)
    }

    /// How far off each way the ground's height is read to find its slope,
    /// in metres.
    pub fn step(&self) -> f32 {
        self.spacing() * 0.15
    }

    /// `height`, the ground at `(x, z)`, with gullies cut down its slopes:
    /// in each octave, stripes run down the slope as the octaves before left
    /// it, cut deeper the steeper it is. None under the sea, and shallower
    /// towards it. `around` is the ground's height [`Gullied::step`] metres
    /// off: further along x, back along x, further along z, back along z.
    pub fn cut(&self, x: f32, z: f32, height: f32, around: [f32; 4]) -> f32 {
        let scale = self.scale.max(0.01);
        let mut spacing = self.spacing();
        // The ground's slope round the point, from its height a little way off each way.
        let step = spacing * 0.15;
        let (mut gx, mut gz) = ((around[0] - around[1]) / (2.0 * step), (around[2] - around[3]) / (2.0 * step));
        let mut cut = 0.0;
        for octave in 0..self.erosion.octaves.min(8) {
            let slope = (gx * gx + gz * gz).sqrt();
            if slope > 1e-4 {
                let along = [gx / slope, gz / slope];
                let seed = seed_of(self.seed, 20 + octave);
                let (value, dx, dz) = gullies(x / spacing, z / spacing, along, seed, self.joined);
                // From nothing between gullies to their whole depth on their floors, deepest from about 30° up.
                let depth = self.erosion.strength * spacing * 0.25 * crate::layers::smoothstep(0.0, 0.6, slope);
                let share = (value - 1.0) * 0.5;
                cut += depth * share;
                // The next octave runs down the slope as this one left it: off a gully's walls, into it.
                gx += depth * 0.5 * dx / spacing;
                gz += depth * 0.5 * dz / spacing;
            }
            spacing *= 0.5;
        }
        // None under the sea: gullies grow shallower towards the shore, over as many metres as they are deep.
        let deepest = self.erosion.strength * self.erosion.spacing * scale * 0.25;
        let above = crate::layers::smoothstep(self.sea_level, self.sea_level + deepest.max(1.0), height);
        // And never below the sea from above it, so no pools are left along the shore.
        (height + cut * above).max(height.min(self.sea_level))
    }

    /// `height`, the ground at `at` on a planet's surface (in metres from
    /// its middle), with gullies cut down its slopes as [`Gullied::cut`]
    /// cuts flat ground's. `around` is the ground's height
    /// [`Gullied::step`] metres off along `tangents`, two directions along
    /// the ground there at right angles: further along the first, back
    /// along it, further along the second, back along it. The slope is
    /// kept as a direction in space, so which way the tangents point does
    /// not show in the ground.
    pub fn cut_round(&self, at: [f32; 3], tangents: ([f32; 3], [f32; 3]), height: f32, around: [f32; 4]) -> f32 {
        let scale = self.scale.max(0.01);
        // None under the sea, as on flat ground: and there nothing to work out.
        let deepest = self.erosion.strength * self.erosion.spacing * scale * 0.25;
        let above = crate::layers::smoothstep(self.sea_level, self.sea_level + deepest.max(1.0), height);
        if above <= 0.0 {
            return height;
        }
        let mut spacing = self.spacing();
        let step = spacing * 0.15;
        let (first, second) = tangents;
        let (a, b) = ((around[0] - around[1]) / (2.0 * step), (around[2] - around[3]) / (2.0 * step));
        let mut g = [0, 1, 2].map(|axis| a * first[axis] + b * second[axis]);
        // The way out, which the slope is kept at right angles to.
        let out = cross(first, second);
        let mut cut = 0.0;
        for octave in 0..self.erosion.octaves.min(8) {
            let slope = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt();
            if slope > 1e-4 {
                let along = g.map(|v| v / slope);
                let across = cross(out, along);
                let seed = seed_of(self.seed, 20 + octave);
                let (value, d) = gullies_round(at.map(|v| v / spacing), across, seed, self.joined);
                let depth = self.erosion.strength * spacing * 0.25 * crate::layers::smoothstep(0.0, 0.6, slope);
                cut += depth * (value - 1.0) * 0.5;
                // The next octave runs down the slope as this one left it, along the ground.
                let up = d[0] * out[0] + d[1] * out[1] + d[2] * out[2];
                for axis in 0..3 {
                    g[axis] += depth * 0.5 * (d[axis] - up * out[axis]) / spacing;
                }
            }
            spacing *= 0.5;
        }
        (height + cut * above).max(height.min(self.sea_level))
    }
}

/// The seed of a generator's part `part`, from its own.
pub(crate) fn seed_of(seed: u32, part: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9E37_79B9) ^ part.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// `curve` at `t`, 0 to 1, through its keys smoothly: a curve of cubics,
/// each between two keys, that never overshoots them, so a value between
/// two keys stays between them and the ground has no crease at a key.
/// `empty` for a curve with none.
pub fn smooth(curve: &Curve, t: f32, empty: f32) -> f32 {
    let keys = curve.keys();
    let (first, last) = match keys {
        [] => return empty,
        [only] => return only[1],
        [first, .., last] => (*first, *last),
    };
    if t <= first[0] {
        return first[1];
    }
    if t >= last[0] {
        return last[1];
    }
    let at = keys.windows(2).position(|pair| t <= pair[1][0]).unwrap_or(keys.len() - 2);
    // The slope across each stretch between keys, and at each key the average of those either side, held flat
    // where they turn and kept small enough not to overshoot.
    let secant = |i: usize| {
        let (a, b) = (keys[i], keys[i + 1]);
        if b[0] - a[0] > 1e-6 { (b[1] - a[1]) / (b[0] - a[0]) } else { 0.0 }
    };
    let tangent = |i: usize| {
        if i == 0 {
            return secant(0);
        }
        if i == keys.len() - 1 {
            return secant(i - 1);
        }
        let (before, after) = (secant(i - 1), secant(i));
        if before * after <= 0.0 { 0.0 } else { (before + after) * 0.5 }
    };
    let slope = secant(at);
    let (mut m0, mut m1) = (tangent(at), tangent(at + 1));
    if slope == 0.0 {
        (m0, m1) = (0.0, 0.0);
    } else {
        let (alpha, beta) = (m0 / slope, m1 / slope);
        let size = alpha * alpha + beta * beta;
        if size > 9.0 {
            let shrink = 3.0 / size.sqrt();
            (m0, m1) = (alpha * shrink * slope, beta * shrink * slope);
        }
    }
    let (a, b) = (keys[at], keys[at + 1]);
    let span = b[0] - a[0];
    if span <= 1e-6 {
        return b[1];
    }
    let s = (t - a[0]) / span;
    let (s2, s3) = (s * s, s * s * s);
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    h00 * a[1] + h10 * span * m0 + h01 * b[1] + h11 * span * m1
}

impl Landmass {
    /// The preset it is shaped as, whatever its seed, scale, sea level,
    /// height, temperature and humidity; none once its shape is its own.
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|preset| {
            let world = preset.landmass();
            let shaped = Landmass {
                seed: self.seed,
                scale: self.scale,
                sea_level: self.sea_level,
                height: self.height,
                climate: Climate {
                    temperature: self.climate.temperature,
                    humidity: self.climate.humidity,
                    ..world.climate
                },
                ..world
            };
            shaped == *self
        })
    }

    /// How warm and wet it is at `(x, z)`, in metres in its own world, on
    /// ground `height` metres high: its temperature, in °C, and its
    /// humidity, 0 to 1.
    pub fn climate(&self, x: f32, z: f32, height: f32) -> (f32, f32) {
        self.climate_at(Spot::Flat(x, z), height)
    }

    /// How warm and wet it is at `at` on a planet's surface, in metres from
    /// its middle, on ground `height` metres high (see
    /// [`Landmass::height_round`]): as [`Landmass::climate`] says, and up to
    /// `POLES_COLDER` degrees colder towards its poles, along y.
    pub fn climate_round(&self, at: [f32; 3], height: f32) -> (f32, f32) {
        let (warmth, humidity) = self.climate_at(Spot::Round(at), height);
        (warmth - colder_towards_poles(at), humidity)
    }

    fn climate_at(&self, spot: Spot, height: f32) -> (f32, f32) {
        let warmth = self.climate.warmth(spot, height - self.sea_level, self.seed, self.scale);
        (warmth, self.climate.wetness(spot, self.seed, self.scale))
    }

    /// Shaped as `preset`, keeping its seed and scale.
    pub fn with_preset(&self, preset: Preset) -> Self {
        Landmass { seed: self.seed, scale: self.scale, ..preset.landmass() }
    }

    /// What it reads and makes at `(x, z)`, in metres in its own world.
    pub fn sample(&self, x: f32, z: f32) -> Sample {
        let (mut sample, relief) = self.shaped(Spot::Flat(x, z));
        if self.erosion.strength > 0.0 && self.erosion.octaves > 0 {
            sample.height = self.eroded(x, z, sample.height);
        }
        sample.height += self.bumped(Spot::Flat(x, z), relief);
        sample
    }

    /// The ground's height at `at` on a planet's surface, in metres from its
    /// middle along the way out to it, its fields read round the planet so
    /// they have no seams: as [`Landmass::height_at`] gives it on flat
    /// ground, its gullies cut down the slope it has along the planet's
    /// surface there (see [`Gullied::cut_round`]).
    pub fn height_round(&self, at: [f32; 3]) -> f32 {
        let (sample, relief) = self.shaped(Spot::Round(at));
        let mut height = sample.height;
        if self.erosion.strength > 0.0 && self.erosion.octaves > 0 {
            let cut = Gullied {
                erosion: self.erosion,
                scale: self.scale,
                sea_level: self.sea_level,
                seed: self.seed,
                joined: true,
            };
            let step = cut.step();
            let (first, second) = tangents(at);
            let off = |by: [f32; 3], sign: f32| {
                self.shaped(Spot::Round([0, 1, 2].map(|axis| at[axis] + by[axis] * step * sign))).0.height
            };
            let around = [off(first, 1.0), off(first, -1.0), off(second, 1.0), off(second, -1.0)];
            height = cut.cut_round(at, (first, second), height, around);
        }
        height + self.bumped(Spot::Round(at), relief)
    }

    /// How far bumps raise the ground at `spot`, with `relief` there.
    fn bumped(&self, spot: Spot, relief: f32) -> f32 {
        if self.bumps == 0.0 {
            return 0.0;
        }
        let size = 30.0 * self.scale.max(0.01);
        let bump = signed(spot.over(size).fractal(NoiseStyle::Fractal, 4, seed_of(self.seed, 7)));
        self.bumps * bump * (0.3 + 0.7 * relief)
    }

    /// Its ground at `spot` before water cuts it and bumps roughen it: its
    /// fields there and its height, and how much relief the ground has.
    fn shaped(&self, spot: Spot) -> (Sample, f32) {
        let scale = self.scale.max(0.01);
        let fractal = |style: NoiseStyle, size: f32, octaves: u32, part: u32, spot: Spot| {
            let size = (size * scale).max(1.0);
            spot.over(size).fractal(style, octaves, seed_of(self.seed, part))
        };
        // The world bent before its fields are read, the same for all of them.
        let bent = match self.warp.strength > 0.0 {
            true => {
                let reach = self.warp.strength * scale;
                let bend = |part: u32| signed(fractal(NoiseStyle::Fractal, self.warp.size, 3, part, spot)) * reach;
                spot.bent(bend)
            }
            false => spot,
        };
        let read =
            |field: Field, part: u32| signed(fractal(NoiseStyle::Fractal, field.size, field.octaves, part, bent));
        let continents = read(self.continents, 3);
        let ruggedness = read(self.ruggedness, 4);
        // Ridged noise runs its ridges along where the noise under it crosses its middle: long lines that wind and
        // branch, as ranges do.
        let size = (self.ridges.size * scale).max(1.0);
        let soft = (1.0 - self.sharpness.clamp(0.0, 1.0)) * 0.35;
        let ridged = ridged(bent.over(size), self.ridges.octaves, seed_of(self.seed, 5), soft);
        let peaks = ((ridged - RIDGE_MIDDLE) * RIDGE_SPREAD).clamp(-1.0, 1.0);
        let across = (continents + 1.0) * 0.5;
        let ground = smooth(&self.coast, across, 0.0);
        let relief =
            (smooth(&self.inland, across, 1.0) * smooth(&self.relief, (ruggedness + 1.0) * 0.5, 1.0)).clamp(0.0, 1.0);
        let shape = smooth(&self.peaks, (peaks + 1.0) * 0.5, 0.0);
        let mut share = ground + relief * shape;
        if self.crags > 0.0 && peaks > 0.0 {
            let crag = fractal(NoiseStyle::Ridged, self.ridges.size / 8.0, 3, 6, bent);
            share += self.crags * relief * peaks * peaks * crag;
        }
        let height = self.sea_level + share * self.height;
        (Sample { continents, ruggedness, peaks, height }, relief)
    }

    /// `height`, the ground at `(x, z)`, with gullies cut down its slopes
    /// (see [`Gullied`]), which are read from its height a little way off
    /// each way.
    fn eroded(&self, x: f32, z: f32, height: f32) -> f32 {
        let cut = Gullied {
            erosion: self.erosion,
            scale: self.scale,
            sea_level: self.sea_level,
            seed: self.seed,
            joined: false,
        };
        let step = cut.step();
        let at = |dx: f32, dz: f32| self.shaped(Spot::Flat(x + dx, z + dz)).0.height;
        cut.cut(x, z, height, [at(step, 0.0), at(-step, 0.0), at(0.0, step), at(0.0, -step)])
    }

    /// The ground's height at `(x, z)`, in metres in its own world.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        self.sample(x, z).height
    }

    /// As read from the text of a `.worldgen` file written before
    /// generators were graphs.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// As such a file's text.
    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_makes_the_same_ground_to_the_bit_and_another_seed_other_ground() {
        let world = Preset::Alpine.landmass();
        let heights: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        let again: Vec<u32> =
            (0..4096).map(|i| world.height_at(i as f32 * 7.3 - 9000.0, i as f32 * 3.1).to_bits()).collect();
        assert_eq!(heights, again);
        // The same bits on every machine: a hash of them as they were made here.
        let hash = crate::layers::stable_hash(bytemuck::cast_slice(&heights));
        assert_eq!(hash, 0x121a7f7a90c39192, "the ground changed: was it meant to? {hash:#x}");
        let other = Landmass { seed: 2, ..world.clone() };
        assert_ne!(other.height_at(100.0, 100.0), world.height_at(100.0, 100.0));
    }

    #[test]
    fn round_heights_and_climate_are_the_same_bits_too() {
        let world = Preset::Alpine.landmass();
        let at = |i: usize| [i as f32 * 3.7 - 5000.0, 4000.0 - i as f32 * 1.9, i as f32 * 2.3 + 100.0];
        let round: Vec<u32> = (0..2048).map(|i| world.height_round(at(i)).to_bits()).collect();
        let hash = crate::layers::stable_hash(bytemuck::cast_slice(&round));
        assert_eq!(hash, 0x2c83_3130_d725_f030, "a planet's ground changed: was it meant to? {hash:#x}");
        let mut climate = Vec::new();
        for i in 0..2048 {
            let (x, z, height) = (i as f32 * 53.0 - 40_000.0, i as f32 * 31.0, i as f32 * 0.4 - 50.0);
            let (warmth, wet) = world.climate(x, z, height);
            let (cold, damp) = world.climate_round(at(i), height);
            climate.extend([warmth, wet, cold, damp].map(f32::to_bits));
        }
        let hash = crate::layers::stable_hash(bytemuck::cast_slice(&climate));
        assert_eq!(hash, 0x88c9_2696_6aa8_c038, "the climate changed: was it meant to? {hash:#x}");
    }

    #[test]
    fn gullies_only_cut_run_down_the_slope_and_leave_level_ground_alone() {
        let world = Preset::Alpine.landmass();
        let dry = Landmass { erosion: Erosion { strength: 0.0, ..world.erosion }, ..world.clone() };
        let (mut lower, mut points) = (0.0, 0);
        for i in 0..2000 {
            let (x, z) = ((i % 50) as f32 * 37.0 + 3000.0, (i / 50) as f32 * 41.0 + 3000.0);
            let (cut, was) = (world.height_at(x, z), dry.height_at(x, z));
            assert!(cut <= was + 1e-3, "at {x}, {z}: raised from {was} to {cut}");
            lower += was - cut;
            points += 1;
        }
        assert!(lower / points as f32 > 0.5, "cut into the mountains: {} m on average", lower / points as f32);
        // Level ground, with no relief, is left as it is.
        let level = Landmass { coast: Curve::flat(0.2), relief: Curve::flat(0.0), bumps: 0.0, ..world.clone() };
        let flat = Landmass { erosion: Erosion { strength: 0.0, ..level.erosion }, ..level.clone() };
        assert_eq!(level.height_at(5000.0, 5000.0), flat.height_at(5000.0, 5000.0));
        // Running along x, they change far more across them than along them.
        let (mut along, mut across) = (0.0, 0.0);
        for i in 0..4000 {
            let (x, z) = ((i % 64) as f32 * 0.173, (i / 64) as f32 * 0.191);
            let (_, dx, dz) = gullies(x, z, [1.0, 0.0], 7, false);
            (along, across) = (along + dx.abs(), across + dz.abs());
        }
        assert!(across > along * 3.0, "across {across}, along {along}");
    }

    #[test]
    fn a_world_is_warmer_and_wetter_in_places_and_colder_up_high() {
        let world = Preset::Alpine.landmass();
        let (mut low, mut high) = (f32::MAX, f32::MIN);
        for i in 0..400 {
            let (x, z) = ((i % 20) as f32 * 1000.0, (i / 20) as f32 * 1000.0);
            let (warmth, wet) = world.climate(x, z, world.sea_level);
            assert!((0.0..=1.0).contains(&wet));
            (low, high) = (low.min(warmth), high.max(warmth));
            assert_eq!(world.climate(x, z, world.sea_level), (warmth, wet), "the same every time");
        }
        assert!(high - low > 6.0, "warmer and colder places: {low} to {high} °C");
        // 6.5 °C colder 1000 m up, and as wet.
        let (sea, wet) = world.climate(500.0, 500.0, 0.0);
        let (peak, wet_up_high) = world.climate(500.0, 500.0, 1000.0);
        assert!((sea - peak - 6.5).abs() < 1e-3 && wet == wet_up_high);
        // Its temperature and humidity are its own, not its shape's.
        let warmer =
            Landmass { climate: Climate { temperature: 30.0, humidity: 0.1, ..world.climate }, ..world.clone() };
        assert_eq!(warmer.preset(), Some(Preset::Alpine));
    }

    #[test]
    fn a_smooth_curve_goes_through_its_keys_and_never_past_them() {
        let curve = Curve::new(&[[0.0, 0.0], [0.3, 1.0], [0.6, 1.0], [1.0, -0.5]]);
        for key in curve.keys() {
            assert!((smooth(&curve, key[0], 9.0) - key[1]).abs() < 1e-6);
        }
        for i in 0..=1000 {
            let t = i as f32 / 1000.0;
            let v = smooth(&curve, t, 9.0);
            assert!((-0.5..=1.0).contains(&v), "{t}: {v}");
        }
        // Flat between two keys of the same value, and nothing at all for no keys.
        assert!((smooth(&curve, 0.45, 9.0) - 1.0).abs() < 1e-6);
        assert_eq!(smooth(&Curve::default(), 0.5, 9.0), 9.0);
    }

    #[test]
    fn fields_fill_their_range_and_presets_are_found_by_their_shape() {
        let world = Preset::Archipelago.landmass();
        let (mut low, mut high, mut sea, mut peaks_low, mut peaks_high) = (0, 0, 0, 0, 0);
        for i in 0..20_000 {
            let s = world.sample((i % 200) as f32 * 61.0, (i / 200) as f32 * 61.0);
            peaks_low += usize::from(s.peaks < -0.6);
            peaks_high += usize::from(s.peaks > 0.6);
            low += usize::from(s.continents < -0.6);
            high += usize::from(s.continents > 0.6);
            sea += usize::from(s.height < world.sea_level);
            assert!((-1.0..=1.0).contains(&s.peaks));
        }
        assert!(low > 1000 && high > 1000, "continents reach both ends: {low} low, {high} high");
        assert!(peaks_low > 1000 && peaks_high > 1000, "valleys and crests: {peaks_low} low, {peaks_high} high");
        assert!((5_000..17_000).contains(&sea), "an archipelago is mostly sea: {sea} of 20,000");
        // Changing its seed, scale, sea level or height keeps its preset; changing its shape does not.
        let moved = Landmass { seed: 9, scale: 2.0, sea_level: 10.0, height: 50.0, ..world.clone() };
        assert_eq!(Landmass { sharpness: 0.1, ..world.clone() }.preset(), None);
        assert_eq!(moved.preset(), Some(Preset::Archipelago));
        assert_eq!(Landmass { crags: 0.5, ..world }.preset(), None);
        // Read back from its file as written.
        let alpine = Preset::Alpine.landmass();
        assert_eq!(Landmass::parse(&alpine.to_text()).unwrap(), alpine);
    }
}
