//! Smoke: a cloud that fills the space round its Obj as smoke from a
//! grenade does, flowing through doors and round corners but not through
//! walls, drawn by `shaders/smoke.slang`.
//!
//! The engine fills each Smoke's grid of `SMOKE_GRID`³ cells outwards from
//! its Obj against the level's colliders (`SmokeFill`), noting how many
//! steps out each cell is, so the cloud grows out from where it started.
//! The renderer marches through the grids at half the view's size, lit by
//! the sun through the cloud and by the light round it, and lays the result
//! over the scene, fogged as far off as the cloud is.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, World};
use voxear_rhi::vk;
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::{
    gpu::{GpuSmoke, NO_TEXTURE, Push},
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    pipelines::Pipelines,
};

/// How many cells a smoke's grid has along each side.
pub const SMOKE_GRID: usize = 32;
/// The most smokes a frame draws, the nearest the camera.
pub const MAX_SMOKES: usize = 16;
/// How many 32-bit words of cells a smoke takes, four cells to a word.
pub const SMOKE_WORDS: usize = SMOKE_GRID * SMOKE_GRID * SMOKE_GRID / 4;
/// How many bricks of 4×4×4 cells a smoke's grid has along each side, and
/// how many words their map takes, after the cells (see `SmokeFill::bricks`).
const SMOKE_BRICKS: usize = SMOKE_GRID / 4;
const BRICK_WORDS: usize = SMOKE_BRICKS * SMOKE_BRICKS * SMOKE_BRICKS / 32;
/// How many words each smoke takes in this frame's cells: its cells, then
/// its bricks' map.
pub const SMOKE_STRIDE: usize = SMOKE_WORDS + BRICK_WORDS;

/// A cloud of smoke filling the space round its Obj, as a smoke grenade's:
/// as much as a ball `size` across would hold, spreading round walls.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Smoke {
    /// How much it fills: as much as a ball this many metres across holds.
    pub size: f32,
    /// How fine its cells are, in metres: finer follows walls more closely
    /// and reaches less far.
    pub cell: f32,
    /// How thick it is: how much of the light a metre of it takes.
    pub density: f32,
    /// Its colour, linear: how much of the light it scatters, by colour.
    pub color: [f32; 3],
    /// How long it takes to spread out, in seconds: most of the way in the
    /// first third of it, as a grenade's cloud bursts out and then settles.
    pub spread: f32,
    /// How long it lasts before it starts to thin, in seconds; 0 for ever.
    pub lasts: f32,
    /// How long it takes to thin away, in seconds.
    pub fade: f32,
    /// Which swirls: two smokes alike but for this look different.
    pub seed: u32,
    /// Whether it starts as its Obj appears; off, it waits for a script to
    /// play it, as a grenade's does for its fuse.
    pub play_on_start: bool,
}

impl Component for Smoke {}

impl Default for Smoke {
    fn default() -> Self {
        Self {
            size: 8.0,
            cell: 0.4,
            density: 2.5,
            color: [0.85, 0.85, 0.85],
            spread: 1.2,
            lasts: 0.0,
            fade: 3.0,
            seed: 0,
            play_on_start: true,
        }
    }
}

impl Smoke {
    /// How many cells it fills at most.
    pub fn budget(&self) -> usize {
        let radius = self.size.max(0.0) * 0.5;
        let cell = self.cell.max(0.05);
        let cells = (4.0 / 3.0 * std::f32::consts::PI * radius.powi(3) / cell.powi(3)).ceil() as usize;
        cells.clamp(1, SMOKE_GRID * SMOKE_GRID * SMOKE_GRID)
    }

    /// How far it has spread out `age` seconds after it started, from 0 to
    /// 1: fast at first, slowing as it settles.
    pub fn grown(&self, age: f32) -> f32 {
        let t = (age / self.spread.max(1e-3)).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// How thick it is `age` seconds after it started, from 0 to 1, as it
    /// thins at the end of its life.
    pub fn thickness(&self, age: f32) -> f32 {
        1.0 - self.dispersed(age)
    }

    /// How far it has dispersed `age` seconds after it started, from 0 to
    /// 1: none until it has lasted its time, then over its fade.
    pub fn dispersed(&self, age: f32) -> f32 {
        if self.lasts <= 0.0 {
            return 0.0;
        }
        ((age - self.lasts) / self.fade.max(1e-3)).clamp(0.0, 1.0)
    }
}

/// A Smoke's cells as filled round where it was: for each cell, 0 where the
/// smoke does not reach, or 1 more than how many steps out from the middle
/// it is; and how many steps out the farthest is. Set by the engine, with
/// the Smoke and the place it was filled for and when, in the renderer's
/// time, so it is filled again when either changes.
#[derive(Clone, Debug, PartialEq)]
pub struct SmokeFill {
    pub cells: Arc<[u8]>,
    pub steps: u8,
    /// The grid's first corner, in the world.
    pub origin: Vec3,
    pub filled_for: (Smoke, Vec3),
    pub started: f32,
    /// When a script told it to disperse, whatever its `lasts` says.
    pub ends: Option<f32>,
}

impl Component for SmokeFill {}

impl SmokeFill {
    /// How far `smoke` has dispersed at `time`, from 0 to 1: after it has
    /// lasted its time, or since a script told it to.
    pub fn dispersed(&self, smoke: &Smoke, time: f32) -> f32 {
        let told = self.ends.map_or(0.0, |ends| ((time - ends) / smoke.fade.max(1e-3)).clamp(0.0, 1.0));
        smoke.dispersed(time - self.started).max(told)
    }

    /// Which bricks of 4×4×4 cells have smoke in them or in a brick next to
    /// them, a bit each, `SMOKE_BRICKS`³ of them: the march reads no cell in
    /// the others, so it passes empty space cheaply. Next to as well, as the
    /// swirls read cells up to a cell or so away.
    fn bricks(&self) -> [u32; BRICK_WORDS] {
        let n = SMOKE_GRID as i32;
        let side = SMOKE_BRICKS as i32;
        let mut full = [false; SMOKE_BRICKS * SMOKE_BRICKS * SMOKE_BRICKS];
        for (at, _) in self.cells.iter().enumerate().filter(|(_, c)| **c != 0) {
            let at = at as i32;
            let (x, y, z) = (at % n / 4, (at / n) % n / 4, at / (n * n) / 4);
            full[((z * side + y) * side + x) as usize] = true;
        }
        let mut words = [0u32; BRICK_WORDS];
        for (bit, word) in (0..full.len()).map(|b| (b, b / 32)) {
            let (x, y, z) = (bit as i32 % side, (bit as i32 / side) % side, bit as i32 / (side * side));
            let near = (-1..=1).flat_map(|dz| (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (dx, dy, dz))));
            let filled = near.map(|(dx, dy, dz)| (x + dx, y + dy, z + dz)).any(|(x, y, z)| {
                (0..side).contains(&x)
                    && (0..side).contains(&y)
                    && (0..side).contains(&z)
                    && full[((z * side + y) * side + x) as usize]
            });
            words[word] |= u32::from(filled) << (bit % 32);
        }
        words
    }

    /// The cells, four to a word, as the shaders read them.
    fn words(&self) -> impl Iterator<Item = u32> + '_ {
        self.cells
            .chunks(4)
            .map(|four| four.iter().enumerate().fold(0u32, |word, (i, &c)| word | u32::from(c) << (i * 8)))
    }
}

/// A Smoke a script played at `at`, in the renderer's time: filled again
/// where its Obj is then, and starting from then.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeCue {
    pub at: f32,
}

impl Component for SmokeCue {}

/// The most shots and blasts a smoke keeps the holes of; a new one takes the
/// place of the oldest.
pub const MAX_SHOTS: usize = 32;
pub const MAX_BLASTS: usize = 5;
/// How wide a shot's hole opens, in metres, and when it starts and ends
/// filling back in, in seconds.
const SHOT_RADIUS: f32 = 0.5;
const SHOT_REFILL: [f32; 2] = [2.0, 5.0];
/// When a blast's hole starts and ends filling back in, in seconds, and how
/// much of the smoke it leaves while open.
const BLAST_REFILL: [f32; 2] = [3.0, 10.0];
/// How long a fresh blast's shell of pushed-out smoke lasts, in seconds.
const BLAST_SHELL: f32 = 3.0;
const BLAST_LEFT: f32 = 0.02;
/// How much thicker a fresh blast leaves the shell of smoke it pushed out.
const BLAST_PUSH: f32 = 1.5;

/// A shot through a smoke, from where it went in to where it came out, and
/// when, in the renderer's time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeShot {
    pub from: Vec3,
    pub to: Vec3,
    pub time: f32,
}

/// A blast in or by a smoke: where, how far it clears, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeBlast {
    pub centre: Vec3,
    pub radius: f32,
    pub time: f32,
}

/// The holes shots and blasts have made in a smoke, which fill back in: a
/// shot's along its line in a second or so, a blast's in a few seconds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SmokeHoles {
    pub shots: Vec<SmokeShot>,
    pub blasts: Vec<SmokeBlast>,
}

impl Component for SmokeHoles {}

impl SmokeHoles {
    /// A shot from `from` to `to` at `time`, taking the oldest's place when
    /// full.
    pub fn shoot(&mut self, from: Vec3, to: Vec3, time: f32) {
        keep_newest(&mut self.shots, SmokeShot { from, to, time }, MAX_SHOTS, |shot| shot.time);
    }

    /// A blast at `centre` clearing `radius` metres round it, at `time`.
    pub fn blast(&mut self, centre: Vec3, radius: f32, time: f32) {
        keep_newest(&mut self.blasts, SmokeBlast { centre, radius: radius.max(0.0), time }, MAX_BLASTS, |b| b.time);
    }

    /// How much of the smoke at `at` its holes leave at `now`: 1 untouched,
    /// near 0 where a hole is open, and more than 1 in the shell a fresh
    /// blast has pushed the smoke out into.
    pub fn left(&self, at: Vec3, now: f32) -> f32 {
        let shots = self.shots.iter().map(|shot| 1.0 - shot_open(shot, at, now));
        let blasts = self.blasts.iter().map(|blast| 1.0 - blast_open(blast, at, now));
        shots.chain(blasts).product()
    }

    /// Forgets the holes long filled back in by `now`.
    pub fn forget_filled(&mut self, now: f32) {
        self.shots.retain(|shot| now - shot.time < SHOT_REFILL[1]);
        self.blasts.retain(|blast| now - blast.time < BLAST_REFILL[1] * 1.5);
    }
}

fn keep_newest<T>(list: &mut Vec<T>, new: T, most: usize, time: impl Fn(&T) -> f32) {
    if list.len() >= most {
        let oldest = list.iter().enumerate().min_by(|a, b| time(a.1).total_cmp(&time(b.1))).map(|(i, _)| i);
        list.remove(oldest.unwrap_or(0));
    }
    list.push(new);
}

fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    let t = ((x - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How open a shot's hole is at `at` at `now`, from 0 to 1: as wide as
/// `SHOT_RADIUS` round its line at once, filling back in over
/// `SHOT_REFILL`. As `shotOpen` in `shaders/smoke.slang`.
fn shot_open(shot: &SmokeShot, at: Vec3, now: f32) -> f32 {
    let age = now - shot.time;
    if age < 0.0 {
        return 0.0;
    }
    let line = shot.to - shot.from;
    let along = ((at - shot.from).dot(line) / line.length_squared().max(1e-6)).clamp(0.0, 1.0);
    let off = at.distance(shot.from + line * along);
    (1.0 - smoothstep(SHOT_RADIUS * 0.5, SHOT_RADIUS, off)) * (1.0 - smoothstep(SHOT_REFILL[0], SHOT_REFILL[1], age))
}

/// How open a blast's hole is: all of its radius, filling back in slowly at
/// first, then faster. As `blastOpen` in `shaders/smoke.slang`.
fn blast_open(blast: &SmokeBlast, at: Vec3, now: f32) -> f32 {
    let age = now - blast.time;
    if age < 0.0 {
        return 0.0;
    }
    let off = at.distance(blast.centre);
    let inside = 1.0 - smoothstep(blast.radius * 0.8, blast.radius, off);
    let refilled = smoothstep(BLAST_REFILL[0], BLAST_REFILL[1], age).powf(1.8);
    // Pushed out into a shell round it, thicker while the blast is fresh: as much less than nothing open there.
    let shell = (-((off - blast.radius * 1.15) / (blast.radius * 0.2).max(1e-3)).powi(2)).exp();
    let fresh = 1.0 - smoothstep(0.0, BLAST_SHELL, age);
    inside * (1.0 - refilled) * (1.0 - BLAST_LEFT) - shell * fresh * BLAST_PUSH
}

/// How many texels the noise has along each side of its tile, how many
/// lattice cells, and how wide each tile is laid out in the atlas, with a
/// texel of the opposite side round it so filtering wraps.
const NOISE_SIDE: usize = 64;
const NOISE_CELLS: usize = 8;
const NOISE_TILE: usize = NOISE_SIDE + 2;
const NOISE_TILES: usize = 8;

/// The smoke's noise, baked once, `NOISE_SIDE` texels a side and
/// `NOISE_CELLS` lattice cells across, each slice of depth a tile of an 8×8
/// atlas, tiling: three gradient noises, one to each of red, green and blue,
/// for the swirls; and in alpha, billows, round puffs packed together, for
/// the cloud's body. Gradient noise has no grid in it, as value noise does;
/// turned between octaves in `smokeNoise`, no rows show. Each channel is
/// spread out to the same range, about 0.17 either side of a half. Gives
/// back the atlas's side and its RGBA bytes.
pub(crate) fn noise_atlas() -> (u32, Vec<u8>) {
    let side = NOISE_TILE * NOISE_TILES;
    let n = NOISE_SIDE;
    let mut values = vec![[0.0f32; 4]; n * n * n];
    for (at, value) in values.iter_mut().enumerate() {
        let (x, y, z) = (at % n, (at / n) % n, at / (n * n));
        let p = Vec3::new(x as f32, y as f32, z as f32) * (NOISE_CELLS as f32 / n as f32);
        for (channel, v) in value.iter_mut().enumerate().take(3) {
            *v = gradient_noise(p, channel as u32);
        }
        value[3] = billows(p) + 0.35 * gradient_noise(p, 3);
    }
    // Each channel spread to the same width about its middle.
    let mut spread = [0.0f32; 4];
    let mut means = [0.0f32; 4];
    for channel in 0..4 {
        let mean = values.iter().map(|v| v[channel]).sum::<f32>() / values.len() as f32;
        let variance = values.iter().map(|v| (v[channel] - mean).powi(2)).sum::<f32>() / values.len() as f32;
        (spread[channel], means[channel]) = (0.17 / variance.sqrt().max(1e-6), mean);
    }
    let mut bytes = vec![0u8; side * side * 4];
    for z in 0..n {
        let (tx, ty) = (z % NOISE_TILES, z / NOISE_TILES);
        for j in 0..NOISE_TILE {
            for i in 0..NOISE_TILE {
                // The tile's own texels, with the opposite side's round it.
                let (x, y) = ((i + n - 1) % n, (j + n - 1) % n);
                let value = values[(z * n + y) * n + x];
                let texel = ((ty * NOISE_TILE + j) * side + tx * NOISE_TILE + i) * 4;
                for channel in 0..4 {
                    bytes[texel + channel] =
                        ((0.5 + (value[channel] - means[channel]) * spread[channel]).clamp(0.0, 1.0) * 255.0).round()
                            as u8;
                }
            }
        }
    }
    (side as u32, bytes)
}

/// Round puffs at `p`, one in each lattice cell, tiling every `NOISE_CELLS`:
/// 1 at a puff's middle, falling away to its edge, where it meets the next
/// in a rounded crease, never a flat one, whose straight edges would show.
fn billows(p: Vec3) -> f32 {
    /// How round the creases between puffs are.
    const ROUND: f32 = 0.12;
    let cells = NOISE_CELLS as i32;
    let base = p.floor();
    let mut near = 0.0f32;
    for k in 0..27 {
        let c = base + Vec3::new((k % 3 - 1) as f32, ((k / 3) % 3 - 1) as f32, (k / 9 - 1) as f32);
        let [x, y, z] = [c.x, c.y, c.z].map(|v| (v as i32).rem_euclid(cells) as u32);
        let mut h = x.wrapping_mul(0x27d4eb2d) ^ y.wrapping_mul(0x165667b1) ^ z.wrapping_mul(0x85ebca6b);
        let mut next = || {
            h ^= h >> 15;
            h = h.wrapping_mul(0x2c1b3c6d);
            h ^= h >> 12;
            h = h.wrapping_mul(0x297a2d39);
            h ^= h >> 15;
            (h & 0xFFFF) as f32 / 65535.0
        };
        // Kept from the cell's walls, so the puffs are evenly sized.
        let point = c + Vec3::new(next(), next(), next()) * 0.7 + 0.15;
        near += (-point.distance(p) / ROUND).exp();
    }
    // The distance to the nearest puff's middle, blended smoothly with the next nearest's.
    let nearest = -near.max(1e-30).ln() * ROUND;
    1.0 - nearest.clamp(0.0, 1.0)
}

/// Gradient noise at `p`, tiling every `NOISE_CELLS`, about -1 to 1, its
/// gradients chosen by `seed`.
fn gradient_noise(p: Vec3, seed: u32) -> f32 {
    let cells = NOISE_CELLS as i32;
    let base = p.floor();
    let f = p - base;
    let fade = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let gradient = |c: [i32; 3]| {
        let [x, y, z] = c.map(|v| v.rem_euclid(cells) as u32);
        let mut h = x.wrapping_mul(0x8da6b343)
            ^ y.wrapping_mul(0xd8163841)
            ^ z.wrapping_mul(0xcb1ab31f)
            ^ seed.wrapping_mul(0x9e3779b9);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1e995);
        h ^= h >> 15;
        // One of twelve edge directions of a cube, as is usual for gradient noise.
        const WAYS: [[f32; 3]; 12] = [
            [1.0, 1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [-1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
            [-1.0, 0.0, -1.0],
            [0.0, 1.0, 1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, -1.0, -1.0],
        ];
        Vec3::from(WAYS[(h % 12) as usize])
    };
    let corner = |o: [i32; 3]| {
        let c = [base.x as i32 + o[0], base.y as i32 + o[1], base.z as i32 + o[2]];
        gradient(c).dot(f - Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32))
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner([0, 0, 0]), corner([1, 0, 0]), fade.x);
    let x10 = lerp(corner([0, 1, 0]), corner([1, 1, 0]), fade.x);
    let x01 = lerp(corner([0, 0, 1]), corner([1, 0, 1]), fade.x);
    let x11 = lerp(corner([0, 1, 1]), corner([1, 1, 1]), fade.x);
    lerp(lerp(x00, x10, fade.y), lerp(x01, x11, fade.y), fade.z)
}

/// The grid's first corner for a Smoke at `middle`, `cell` metres a cell,
/// the middle cell's centre on it.
pub fn smoke_origin(middle: Vec3, cell: f32) -> Vec3 {
    middle - Vec3::splat((SMOKE_GRID / 2) as f32 * cell + cell * 0.5)
}

/// This frame's smokes, the nearest `camera` first, and their cells, at
/// `time`.
pub(crate) fn gather(world: &World, camera: Vec3, time: f32) -> (Vec<GpuSmoke>, Vec<u32>, Vec<[f32; 4]>) {
    let query = world.query_ref::<(&Smoke, &SmokeFill, Option<&SmokeHoles>, &Transform, Option<&WorldTransform>)>();
    let mut smokes: Vec<(f32, &Smoke, &SmokeFill, Option<&SmokeHoles>)> = query
        .without::<Destroying>()
        .into_iter()
        .map(|(smoke, fill, holes, transform, at)| {
            let middle = at.map_or(transform.position, |w| w.0.w_axis.truncate());
            (middle.distance_squared(camera), smoke, fill, holes)
        })
        .filter(|(_, smoke, fill, _)| fill.dispersed(smoke, time) < 1.0 && fill.steps > 0)
        .collect();
    smokes.sort_by(|a, b| a.0.total_cmp(&b.0));
    smokes.truncate(MAX_SMOKES);
    let mut cells = Vec::with_capacity(smokes.len() * SMOKE_STRIDE);
    let mut holes = Vec::new();
    let gpu = smokes
        .into_iter()
        .map(|(_, smoke, fill, made)| {
            let first = cells.len() as u32;
            cells.extend(fill.words());
            cells.extend(fill.bricks());
            // Each shot two rows, where from and when, then where to; each blast where and when, then how far.
            let first_hole = holes.len() as u32;
            let (shots, blasts) = made.map_or((&[][..], &[][..]), |h| (&h.shots[..], &h.blasts[..]));
            for shot in shots {
                holes.push([shot.from.x, shot.from.y, shot.from.z, shot.time]);
                holes.push([shot.to.x, shot.to.y, shot.to.z, 0.0]);
            }
            for blast in blasts {
                holes.push([blast.centre.x, blast.centre.y, blast.centre.z, blast.time]);
                holes.push([blast.radius, 0.0, 0.0, 0.0]);
            }
            let age = time - fill.started;
            GpuSmoke {
                origin: fill.origin.to_array(),
                cell: fill.filled_for.0.cell.max(0.05),
                color: smoke.color.map(|c| c.clamp(0.0, 1.0)),
                density: smoke.density.max(0.0),
                grown: smoke.grown(age),
                steps: f32::from(fill.steps),
                seed: (smoke.seed % 1024) as f32 * 17.31,
                age,
                cells: first,
                holes: first_hole,
                shots: shots.len() as u32,
                blasts: blasts.len() as u32,
                dispersed: fill.dispersed(smoke, time),
                _pad: [0.0; 3],
            }
        })
        .collect();
    (gpu, cells, holes)
}

/// The smokes a frame draws, after the fog (see `post_passes`): how many,
/// and what their lighting reads.
#[derive(Clone)]
pub struct Draw {
    pub count: u32,
    pub mists: u32,
    /// The baked noise (`noise_atlas`), as a texture.
    pub noise: u32,
    /// The scene's depth, and the shadow atlas, the GI's probes and the
    /// atmosphere's tables.
    pub depth: ImageId,
    pub sampled: Vec<ImageId>,
    /// The lamps' cluster lists, which light it.
    pub clusters: BufferId,
}

/// The two images the smoke is smoothed over frames in, half the view's
/// size: one written each frame, the other last frame's.
const HISTORY_NAMES: [&str; 2] = ["smoke history 0", "smoke history 1"];

/// Records the smokes: marched into an image of their own, with how far
/// into them their light comes from, one of each 2×2 of the view's
/// half-size pixels a frame, in turn; put together at half size with last
/// frame's where the smoke was then, so it does not shimmer; then laid
/// over `hdr`, each pixel from the half-size ones nearest it in depth, so
/// the smoke's edge stays sharp along what stands in front of it.
/// `history` is which of the two history images this frame writes.
pub(crate) fn record(
    fg: &mut FrameGraph<'_>,
    (gpu, bindless): (&voxear_rhi::Gpu, &mut voxear_rhi::Bindless),
    (pipelines, push): (Pipelines, Push),
    (draw, hdr, extent, history): (Draw, ImageId, vk::Extent2D, usize),
    fog_volume: Option<ImageId>,
) -> anyhow::Result<()> {
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
    let half = vk::Extent2D { width: extent.width.div_ceil(2), height: extent.height.div_ceil(2) };
    // Marched at a quarter of the half size, a different pixel of each 2×2 each frame.
    let quarter = vk::Extent2D { width: half.width.div_ceil(2), height: half.height.div_ceil(2) };
    let rgba = voxear_rhi::ImageDesc::new(half.width, half.height, vk::Format::R16G16B16A16_SFLOAT, usage);
    let marched_desc =
        voxear_rhi::ImageDesc::new(quarter.width, quarter.height, vk::Format::R16G16B16A16_SFLOAT, usage);
    let marched = fg.create_image(gpu, bindless, "smoke", marched_desc)?;
    let far = voxear_rhi::ImageDesc::new(quarter.width, quarter.height, vk::Format::R32_SFLOAT, usage);
    let distance = fg.create_image(gpu, bindless, "smoke distance", far)?;
    let now = fg.history_image(gpu, bindless, HISTORY_NAMES[history], rgba)?;
    let then = fg.history_image(gpu, bindless, HISTORY_NAMES[1 - history], rgba)?;
    // Last frame's only when it was drawn, not on the first frame or after the smoke was last seen.
    let kept = !fg.is_new(then);
    let Draw { count, mists, noise, depth, sampled, clusters } = draw;
    let depth_texture = fg.texture(depth).0;
    let volume = fog_volume.map_or(NO_TEXTURE, |volume| fg.texture(volume).0);
    let mut images = vec![
        (marched, ImageAccess::ColorAttachment),
        (distance, ImageAccess::ColorAttachment),
        (depth, ImageAccess::Sampled),
    ];
    images.extend(sampled.iter().chain(&fog_volume).map(|&image| (image, ImageAccess::Sampled)));
    let march = Push {
        texture: noise,
        textures: [depth_texture, volume, count, mists],
        size: [half.width, half.height],
        ..push
    };
    fg.add_pass("Smoke", &images, &[(clusters, BufferAccess::ShaderRead)], move |ctx| {
        let target = |image| {
            vk::RenderingAttachmentInfo::default()
                .image_view(ctx.view(image))
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::DONT_CARE)
                .store_op(vk::AttachmentStoreOp::STORE)
        };
        let color = [target(marched), target(distance)];
        crate::passes::begin_rendering(ctx, quarter, &color, None);
        crate::passes::fullscreen(ctx, pipelines.smoke_march, &march);
    });
    let (marched_texture, distance_texture, then_texture) =
        (fg.texture(marched).0, fg.texture(distance).0, fg.texture(then).0);
    let images = [
        (now, ImageAccess::ColorAttachment),
        (marched, ImageAccess::Sampled),
        (distance, ImageAccess::Sampled),
        (then, ImageAccess::Sampled),
    ];
    let textures = [distance_texture, then_texture, u32::from(kept), 0];
    let resolve = Push { texture: marched_texture, textures, size: [quarter.width, quarter.height], ..push };
    fg.add_pass("Smoke over frames", &images, &[], move |ctx| {
        crate::post_passes::draw(ctx, pipelines.smoke_resolve, &resolve, now, false);
    });
    let now_texture = fg.texture(now).0;
    let images = [
        (hdr, ImageAccess::ColorAttachment),
        (now, ImageAccess::Sampled),
        (depth, ImageAccess::Sampled),
        (distance, ImageAccess::Sampled),
    ];
    let textures = [depth_texture, distance_texture, 0, 0];
    let lay = Push { texture: now_texture, textures, size: [half.width, half.height], ..push };
    fg.add_pass("Smoke over the scene", &images, &[], move |ctx| {
        crate::post_passes::draw(ctx, pipelines.smoke_composite, &lay, hdr, true);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shot_opens_a_tunnel_along_its_line_that_fills_back_in() {
        let mut holes = SmokeHoles::default();
        holes.shoot(Vec3::new(-5.0, 1.0, 0.0), Vec3::new(5.0, 1.0, 0.0), 10.0);
        assert!(holes.left(Vec3::new(2.0, 1.0, 0.0), 10.1) < 0.01, "open along its line");
        assert_eq!(holes.left(Vec3::new(2.0, 2.0, 0.0), 10.1), 1.0, "a metre off it, untouched");
        assert!(holes.left(Vec3::new(2.0, 1.0, 0.0), 11.9) < 0.01, "still open two seconds on");
        assert_eq!(holes.left(Vec3::new(2.0, 1.0, 0.0), 15.0), 1.0, "filled back in");
        assert_eq!(holes.left(Vec3::new(2.0, 1.0, 0.0), 9.0), 1.0, "not before it");
        for i in 0..40 {
            holes.shoot(Vec3::ZERO, Vec3::X, 20.0 + i as f32);
        }
        assert_eq!(holes.shots.len(), MAX_SHOTS, "the newest kept");
        holes.forget_filled(100.0);
        assert!(holes.shots.is_empty());
    }

    #[test]
    fn a_blast_clears_a_ball_that_fills_back_in_over_seconds() {
        let mut holes = SmokeHoles::default();
        holes.blast(Vec3::ZERO, 3.0, 0.0);
        let at = Vec3::new(1.0, 0.0, 0.0);
        assert!((holes.left(at, 0.3) - BLAST_LEFT).abs() < 1e-3, "a little left in it");
        let (soon, later) = (holes.left(at, 2.0), holes.left(at, 4.0));
        assert!(soon < later && later < 1.0, "filling back: {soon} then {later}");
        assert_eq!(holes.left(at, 10.0), 1.0);
        assert!((holes.left(Vec3::new(6.0, 0.0, 0.0), 0.3) - 1.0).abs() < 1e-3, "well past its radius");
        assert!(holes.left(Vec3::new(3.45, 0.0, 0.0), 0.3) > 2.0, "pushed out just past it");
        assert!(holes.left(Vec3::new(3.45, 0.0, 0.0), 4.0) < 1.01, "and even again later");
    }

    #[test]
    fn a_smoke_fills_as_much_as_a_ball_its_size_and_thins_at_the_end() {
        let smoke = Smoke { size: 4.0, cell: 0.5, lasts: 10.0, fade: 2.0, ..Smoke::default() };
        // A ball four metres across holds about 33.5 cubic metres: 268 cells of half a metre.
        assert_eq!(smoke.budget(), 269);
        assert_eq!((smoke.thickness(5.0), smoke.thickness(11.0), smoke.thickness(13.0)), (1.0, 0.5, 0.0));
        assert_eq!(Smoke::default().thickness(1e6), 1.0, "one with no end lasts for ever");
    }

    #[test]
    fn cells_go_to_the_shaders_four_to_a_word() {
        let fill = SmokeFill {
            cells: Arc::from(vec![1, 2, 3, 4, 0, 0, 0, 9]),
            steps: 9,
            origin: Vec3::ZERO,
            filled_for: (Smoke::default(), Vec3::ZERO),
            started: 0.0,
            ends: None,
        };
        assert_eq!(fill.words().collect::<Vec<_>>(), [0x04030201, 0x09000000]);
    }
}
