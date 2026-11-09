//! Changing a terrain while a game runs: a shell's crater, a trench dug
//! lower, ground raised or flattened, a surface painted, a hole cut or
//! filled, grass grown or cleared. Each [`Edit`] is a small command,
//! applied straight to the terrain's tiles (the layers are the editor's,
//! and a built game has none) with nothing but additions, multiplications
//! and `smoothstep`s, so the same edits give the same ground on every
//! machine: what a networked game sends to every player, a few dozen bytes
//! each ([`Edit::to_bytes`]).
//!
//! What every edit together changed from the ground as built is kept small
//! ([`write_edits`]): only the tiles they reached, each as how it differs
//! from the tile as built, so a save of a battlefield holds the craters
//! and not the field, and a player joining a game is sent the same.

use std::sync::Arc;

use anyhow::{Context, Result, ensure};

use crate::{
    Terrain,
    grass::{GrassMap, GrassTile},
    heights::{HOLE_WORDS, HeightTile, Heightmap, TILE_LEN, TILE_QUADS, TILE_SAMPLES},
    layers::{Area, smoothstep},
    surfaces::{SHARES, WeightTile, Weights},
};

/// One change to a terrain's ground, in metres from its corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    /// Raises the ground within `radius` metres of `at` by `height`, most
    /// at the middle and easing to nothing at the edge; less than nothing
    /// lowers it.
    Raise { at: [f32; 2], radius: f32, height: f32 },
    /// A crater `radius` metres round and `depth` deep, its rim thrown up
    /// round it, the grass in it cleared, and surface `surface` laid on
    /// its walls where one is given.
    Crater { at: [f32; 2], radius: f32, depth: f32, surface: Option<u8> },
    /// Brings the ground within `radius` metres to `height`, easing in at
    /// the edge.
    Flatten { at: [f32; 2], radius: f32, height: f32 },
    /// Lays surface `surface` over the ground within `radius` metres, as
    /// much as `strength`, 0 to 1, at the middle.
    Paint { at: [f32; 2], radius: f32, surface: u8, strength: f32 },
    /// Cuts the ground away within `radius` metres, or fills the holes
    /// there again.
    Holes { at: [f32; 2], radius: f32, cut: bool },
    /// Brings grass kind `kind`, or every kind with `None`, towards
    /// `amount` (0 cleared, 1 grown fully) within `radius` metres.
    Grass { at: [f32; 2], radius: f32, kind: Option<u8>, amount: f32 },
    /// Digs a ball of ground `radius` metres round out of it, `at` in
    /// metres from the terrain's corner, up and down too, laying surface
    /// `surface` on what it bares where one is given (see `dig`).
    Dig { at: [f32; 3], radius: f32, surface: Option<u8> },
    /// Puts a ball of ground back.
    Fill { at: [f32; 3], radius: f32 },
    /// Digs a tunnel `radius` metres round from `from` to `to`, rounded at
    /// its ends: a trench along the ground, or a way under it.
    Tunnel { from: [f32; 3], to: [f32; 3], radius: f32, surface: Option<u8> },
    /// A shell's crater blown out of the ground at `at`: a bowl `radius`
    /// metres round, its middle a third of that under `at`, and the earth
    /// thrown up round its lip, laying surface `surface` on what it bares.
    Explode { at: [f32; 3], radius: f32, surface: Option<u8> },
}

impl Edit {
    /// Where it is and how far it reaches.
    pub fn reach(&self) -> ([f32; 2], f32) {
        match *self {
            // A crater's rim reaches past its radius.
            Edit::Crater { at, radius, .. } => (at, radius * RIM_REACH),
            Edit::Raise { at, radius, .. }
            | Edit::Flatten { at, radius, .. }
            | Edit::Paint { at, radius, .. }
            | Edit::Holes { at, radius, .. }
            | Edit::Grass { at, radius, .. } => (at, radius),
            Edit::Dig { at, radius, .. } | Edit::Fill { at, radius } => ([at[0], at[2]], radius),
            Edit::Explode { at, radius, .. } => ([at[0], at[2]], radius * EXPLOSION_REACH),
            Edit::Tunnel { from, to, radius, .. } => {
                let middle = [(from[0] + to[0]) * 0.5, (from[2] + to[2]) * 0.5];
                let half = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt() * 0.5;
                (middle, half + radius)
            }
        }
    }

    /// The shapes a digging edit carves, in order, each digging or filling;
    /// none for an edit of the heights.
    pub fn carving(&self) -> Vec<(crate::dig::Shape, crate::dig::Carve, Option<u8>)> {
        use crate::dig::{Carve, Shape};
        match *self {
            Edit::Dig { at, radius, surface } => vec![(Shape::Ball { at, radius }, Carve::Dig, surface)],
            Edit::Fill { at, radius } => vec![(Shape::Ball { at, radius }, Carve::Fill, None)],
            Edit::Tunnel { from, to, radius, surface } => {
                vec![(Shape::Capsule { from, to, radius }, Carve::Dig, surface)]
            }
            Edit::Explode { at, radius, surface } => {
                // Earth thrown up round the lip first, in a ring of mounds, then the bowl cut through their insides.
                let mut shapes: Vec<_> = (0..RIM_MOUNDS)
                    .map(|i| {
                        let turn = i as f32 / RIM_MOUNDS as f32 * std::f32::consts::TAU;
                        let (sin, cos) = turn.sin_cos();
                        let mound = [at[0] + cos * radius, at[1] - radius * 0.1, at[2] + sin * radius];
                        (Shape::Ball { at: mound, radius: radius * 0.3 }, Carve::Fill, None)
                    })
                    .collect();
                let middle = [at[0], at[1] - radius / 3.0, at[2]];
                shapes.push((Shape::Ball { at: middle, radius }, Carve::Dig, surface));
                shapes
            }
            _ => Vec::new(),
        }
    }

    /// Whether it is an edit at all: a radius more than nothing and every
    /// number a number, as one from another machine may not be.
    pub fn is_sound(&self) -> bool {
        let numbers: &[f32] = match self {
            Edit::Raise { at, radius, height } | Edit::Flatten { at, radius, height } => {
                &[at[0], at[1], *radius, *height]
            }
            Edit::Crater { at, radius, depth, .. } => &[at[0], at[1], *radius, *depth],
            Edit::Paint { at, radius, strength, .. } => &[at[0], at[1], *radius, *strength],
            Edit::Holes { at, radius, .. } => &[at[0], at[1], *radius],
            Edit::Grass { at, radius, amount, .. } => &[at[0], at[1], *radius, *amount],
            Edit::Dig { at, radius, .. } | Edit::Fill { at, radius } => &[at[0], at[1], at[2], *radius],
            Edit::Tunnel { from, to, radius, .. } => &[from[0], from[1], from[2], to[0], to[1], to[2], *radius],
            Edit::Explode { at, radius, .. } => &[at[0], at[1], at[2], *radius],
        };
        numbers.iter().all(|v| v.is_finite()) && self.reach().1 > 0.0
    }

    /// The same edit `by` metres along, x, y and z, as from the world into
    /// a terrain's space: a height to flatten to moved up with it.
    pub fn moved(mut self, by: [f32; 3]) -> Self {
        let shift = |p: &mut [f32; 3]| *p = [p[0] + by[0], p[1] + by[1], p[2] + by[2]];
        match &mut self {
            Edit::Flatten { at, height, .. } => {
                *at = [at[0] + by[0], at[1] + by[2]];
                *height += by[1];
            }
            Edit::Raise { at, .. }
            | Edit::Crater { at, .. }
            | Edit::Paint { at, .. }
            | Edit::Holes { at, .. }
            | Edit::Grass { at, .. } => *at = [at[0] + by[0], at[1] + by[2]],
            Edit::Dig { at, .. } | Edit::Fill { at, .. } | Edit::Explode { at, .. } => shift(at),
            Edit::Tunnel { from, to, .. } => {
                shift(from);
                shift(to);
            }
        }
        self
    }

    /// As few bytes as say it: its kind, then its numbers.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(24);
        let put = |out: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|v| out.extend(v.to_le_bytes()));
        match *self {
            Edit::Raise { at, radius, height } => {
                out.push(0);
                put(&mut out, &[at[0], at[1], radius, height]);
            }
            Edit::Crater { at, radius, depth, surface } => {
                out.push(1);
                put(&mut out, &[at[0], at[1], radius, depth]);
                out.push(surface.unwrap_or(0xFF));
            }
            Edit::Flatten { at, radius, height } => {
                out.push(2);
                put(&mut out, &[at[0], at[1], radius, height]);
            }
            Edit::Paint { at, radius, surface, strength } => {
                out.push(3);
                put(&mut out, &[at[0], at[1], radius, strength]);
                out.push(surface);
            }
            Edit::Holes { at, radius, cut } => {
                out.push(4);
                put(&mut out, &[at[0], at[1], radius]);
                out.push(u8::from(cut));
            }
            Edit::Grass { at, radius, kind, amount } => {
                out.push(5);
                put(&mut out, &[at[0], at[1], radius, amount]);
                out.push(kind.unwrap_or(0xFF));
            }
            Edit::Dig { at, radius, surface } => {
                out.push(6);
                put(&mut out, &[at[0], at[1], at[2], radius]);
                out.push(surface.unwrap_or(0xFF));
            }
            Edit::Fill { at, radius } => {
                out.push(7);
                put(&mut out, &[at[0], at[1], at[2], radius]);
            }
            Edit::Explode { at, radius, surface } => {
                out.push(9);
                put(&mut out, &[at[0], at[1], at[2], radius]);
                out.push(surface.unwrap_or(0xFF));
            }
            Edit::Tunnel { from, to, radius, surface } => {
                out.push(8);
                put(&mut out, &[from[0], from[1], from[2], to[0], to[1], to[2], radius]);
                out.push(surface.unwrap_or(0xFF));
            }
        }
        out
    }

    /// An edit from [`to_bytes`](Self::to_bytes)'s bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let number = |i: usize| -> Result<f32> {
            let at = 1 + 4 * i;
            Ok(f32::from_le_bytes(bytes.get(at..at + 4).context("an edit ends early")?.try_into().expect("four")))
        };
        let byte = |i: usize| -> Result<u8> { bytes.get(1 + 4 * i).copied().context("an edit ends early") };
        let at = [number(0)?, number(1)?];
        let optional = |value: u8| (value != 0xFF).then_some(value);
        Ok(match bytes.first().context("an empty edit")? {
            0 => Edit::Raise { at, radius: number(2)?, height: number(3)? },
            1 => Edit::Crater { at, radius: number(2)?, depth: number(3)?, surface: optional(byte(4)?) },
            2 => Edit::Flatten { at, radius: number(2)?, height: number(3)? },
            3 => Edit::Paint { at, radius: number(2)?, strength: number(3)?, surface: byte(4)? },
            4 => Edit::Holes { at, radius: number(2)?, cut: byte(3)? != 0 },
            5 => Edit::Grass { at, radius: number(2)?, amount: number(3)?, kind: optional(byte(4)?) },
            6 => {
                Edit::Dig { at: [number(0)?, number(1)?, number(2)?], radius: number(3)?, surface: optional(byte(4)?) }
            }
            7 => Edit::Fill { at: [number(0)?, number(1)?, number(2)?], radius: number(3)? },
            9 => Edit::Explode {
                at: [number(0)?, number(1)?, number(2)?],
                radius: number(3)?,
                surface: optional(byte(4)?),
            },
            8 => Edit::Tunnel {
                from: [number(0)?, number(1)?, number(2)?],
                to: [number(3)?, number(4)?, number(5)?],
                radius: number(6)?,
                surface: optional(byte(7)?),
            },
            kind => anyhow::bail!("no edit of kind {kind}"),
        })
    }
}

/// How far past a crater's radius its rim reaches, as a share of it.
const RIM_REACH: f32 = 1.4;
/// How many mounds of earth an explosion throws up round its lip, and how
/// far past its radius they reach, as a share of it.
const RIM_MOUNDS: usize = 12;
const EXPLOSION_REACH: f32 = 1.3;

/// How a crater stands at `r`, its distance from the middle over its
/// radius: a bowl `depth` deep and a rim a fifth of that, peaking at its
/// edge. Smoothsteps alone, so the same on every machine.
fn crater(r: f32, depth: f32) -> f32 {
    let bowl = 1.0 - smoothstep(0.0, 1.0, r);
    let rim = smoothstep(0.7, 1.0, r) * (1.0 - smoothstep(1.0, RIM_REACH, r));
    -depth * bowl * bowl + depth * 0.2 * rim
}

/// How finely edits set the ground's heights: to the nearest 1/1024 m,
/// which no one sees, so a save keeps each height changed as a whole
/// number of steps from the one built, and not as all the bits of a float.
const STEPS: f32 = 1024.0;

fn on_step(height: f32) -> f32 {
    (height * STEPS).round() / STEPS
}

/// Height `now` as a whole number of steps from `built`'s nearest; none
/// where it is not on a step, as a height past 16 km is not.
fn steps_from(built: f32, now: f32) -> Option<i32> {
    let steps = (now * STEPS).round();
    (steps / STEPS == now && steps.abs() < 1.6e7).then(|| steps as i32 - (built * STEPS).round() as i32)
}

/// The samples of the whole terrain an edit reaching `radius` metres round
/// `at` covers, `[x0, z0, x1, z1]`, within it; `None` where it misses.
fn samples(heights: &Heightmap, at: [f32; 2], radius: f32) -> Option<[i64; 4]> {
    let spacing = heights.spacing();
    let [qx, qz] = heights.quads().map(i64::from);
    let first = |v: f32| ((v - radius) / spacing).floor() as i64;
    let last = |v: f32| ((v + radius) / spacing).ceil() as i64;
    let (x0, z0, x1, z1) = (first(at[0]), first(at[1]), last(at[0]), last(at[1]));
    (x1 >= 0 && z1 >= 0 && x0 <= qx && z0 <= qz).then(|| [x0.max(0), z0.max(0), x1.min(qx), z1.min(qz)])
}

/// The tiles holding any of `rect`'s samples, a sample on a tile's edge in
/// both.
fn tiles_of(heights: &Heightmap, [x0, z0, x1, z1]: [i64; 4]) -> Vec<(u32, u32)> {
    let quads = i64::from(TILE_QUADS);
    let span = |a: i64, b: i64, tiles: u32| {
        let first = ((a - 1).max(0) / quads) as u32;
        let last = ((b / quads) as u32).min(tiles - 1);
        first..=last
    };
    let mut found = Vec::new();
    for tz in span(z0, z1, heights.tiles_z()) {
        for tx in span(x0, x1, heights.tiles_x()) {
            found.push((tx, tz));
        }
    }
    found
}

/// Calls `each` for every sample of tile `(tx, tz)` within `rect`, with its
/// place in the tile and in the whole terrain.
fn for_samples(tx: u32, tz: u32, [x0, z0, x1, z1]: [i64; 4], mut each: impl FnMut(usize, i64, i64)) {
    let (ox, oz) = (i64::from(tx * TILE_QUADS), i64::from(tz * TILE_QUADS));
    for z in z0.max(oz)..=z1.min(oz + i64::from(TILE_QUADS)) {
        for x in x0.max(ox)..=x1.min(ox + i64::from(TILE_QUADS)) {
            each(((z - oz) as u32 * TILE_SAMPLES + (x - ox) as u32) as usize, x, z);
        }
    }
}

/// How far `(x, z)`, a sample of ground `spacing` metres apart, is from
/// `at`, over `radius`.
fn reach_of(x: i64, z: i64, spacing: f32, at: [f32; 2], radius: f32) -> f32 {
    let (dx, dz) = (x as f32 * spacing - at[0], z as f32 * spacing - at[1]);
    (dx * dx + dz * dz).sqrt() / radius.max(1e-4)
}

/// Changes `ground`'s tiles as `edit` says, in metres from its corner, and
/// says the area it changed; nothing where the edit misses the terrain, or
/// is not one (a radius of nothing, or a number that is not).
pub fn apply(ground: &mut Terrain, edit: &Edit) -> Option<Area> {
    if !edit.is_sound() {
        return None;
    }
    let (at, reach) = edit.reach();
    let carving = edit.carving();
    if !carving.is_empty() {
        let mut dug = ground.dug.as_deref().cloned().unwrap_or_default();
        let mut area: Option<Area> = None;
        for (shape, carve, surface) in carving {
            let Some((holed, _)) = dug.carve(&ground.heights, &shape, carve, surface) else { continue };
            if let Some(holed) = holed {
                ground.heights = Arc::new(holed);
            }
            let (low, high) = shape.bounds();
            let [x0, z0, x1, z1] = area.unwrap_or([low.x, low.z, high.x, high.z]);
            area = Some([x0.min(low.x), z0.min(low.z), x1.max(high.x), z1.max(high.z)]);
        }
        let area = area?;
        let opened =
            |grass: &Arc<GrassMap>| Arc::new(crate::dig::clear_grass(grass, &ground.heights, &dug, Some(area)));
        ground.grass = ground.grass.as_ref().map(opened);
        ground.dug = Some(Arc::new(dug));
        return Some(area);
    }
    let rect = samples(&ground.heights, at, reach)?;
    let spacing = ground.heights.spacing();
    let tiles = tiles_of(&ground.heights, rect);
    let area = [rect[0] as f32 * spacing, rect[1] as f32 * spacing, rect[2] as f32 * spacing, rect[3] as f32 * spacing];
    // The ground's heights and holes first.
    let shape = |r: f32, height: f32| -> Option<f32> {
        match *edit {
            Edit::Raise { height: by, .. } if r < 1.0 => Some(height + by * (1.0 - smoothstep(0.0, 1.0, r))),
            Edit::Crater { depth, .. } if r < RIM_REACH => Some(height + crater(r, depth)),
            Edit::Flatten { height: to, .. } if r < 1.0 => {
                Some(height + (to - height) * (1.0 - smoothstep(0.7, 1.0, r)))
            }
            _ => None,
        }
    };
    let radius = match *edit {
        Edit::Crater { radius, .. } => radius,
        _ => reach,
    };
    if matches!(edit, Edit::Raise { .. } | Edit::Crater { .. } | Edit::Flatten { .. } | Edit::Holes { .. }) {
        let mut changed = Vec::new();
        for &(tx, tz) in &tiles {
            let tile = ground.heights.tile(tx, tz);
            let mut samples = tile.samples().to_vec();
            let mut holes: Option<Vec<u32>> = tile.holes().map(<[u32]>::to_vec);
            for_samples(tx, tz, rect, |index, x, z| {
                let r = reach_of(x, z, spacing, at, radius);
                if let Some(height) = shape(r, samples[index]).map(on_step)
                    && height != samples[index]
                {
                    samples[index] = height;
                }
                if let Edit::Holes { cut, .. } = *edit
                    && r <= 1.0
                {
                    let bits = holes.get_or_insert_with(|| vec![0; HOLE_WORDS]);
                    let (word, bit) = (index / 32, index % 32);
                    bits[word] = if cut { bits[word] | 1 << bit } else { bits[word] & !(1 << bit) };
                }
            });
            changed.push((tx, tz, HeightTile::from_samples(samples).with_holes(holes.map(Vec::into_boxed_slice))));
        }
        ground.heights = Arc::new(ground.heights.with_tiles(changed));
    }
    // Then the surfaces: painted, or a crater's walls.
    let paint = match *edit {
        Edit::Paint { surface, strength, .. } => Some((surface, strength, 1.0)),
        Edit::Crater { surface: Some(surface), .. } => Some((surface, 1.0, RIM_REACH)),
        _ => None,
    };
    if let (Some((surface, strength, out_to)), Some(weights)) = (paint, &ground.weights) {
        let mut tiles_now = weights.tiles().to_vec();
        for &(tx, tz) in &tiles {
            let index = (tz * weights.tiles_x() + tx) as usize;
            let mut samples = tiles_now[index].samples().to_vec();
            for_samples(tx, tz, rect, |i, x, z| {
                let r = reach_of(x, z, spacing, at, radius) / out_to;
                let amount = strength.clamp(0.0, 1.0) * (1.0 - smoothstep(0.6, 1.0, r));
                if amount > 0.0 {
                    samples[i] = laid(samples[i], surface, amount);
                }
            });
            tiles_now[index] = Arc::new(WeightTile::from_samples(samples));
        }
        ground.weights = Some(Arc::new(Weights::from_tiles(weights.tiles_x(), weights.tiles_z(), tiles_now)));
    }
    // And the grass: grown or cleared, or gone from a crater.
    let grass_change = match *edit {
        Edit::Grass { kind, amount, .. } => Some((kind, amount, 1.0)),
        Edit::Crater { .. } => Some((None, 0.0, RIM_REACH)),
        _ => None,
    };
    if let (Some((kind, amount, out_to)), Some(grass)) = (grass_change, &ground.grass) {
        let words = grass.words();
        let mut tiles_now = grass.tiles().to_vec();
        let tiles_x = (ground.heights.tiles_x()) as usize;
        for &(tx, tz) in &tiles {
            let index = tz as usize * tiles_x + tx as usize;
            let mut samples = tiles_now[index].samples().to_vec();
            for_samples(tx, tz, rect, |i, x, z| {
                let r = reach_of(x, z, spacing, at, radius) / out_to;
                let pull = 1.0 - smoothstep(0.7, 1.0, r);
                if pull <= 0.0 {
                    return;
                }
                for k in 0..words * 4 {
                    if kind.is_some_and(|kind| usize::from(kind) != k) {
                        continue;
                    }
                    let word = &mut samples[i * words + k / 4];
                    let shift = 8 * (k % 4);
                    let now = ((*word >> shift) & 0xFF) as f32;
                    let goal = amount.clamp(0.0, 1.0) * 255.0;
                    let byte = (now + (goal - now) * pull).round().clamp(0.0, 255.0) as u32;
                    *word = (*word & !(0xFF << shift)) | byte << shift;
                }
            });
            tiles_now[index] = Arc::new(GrassTile::from_samples(samples));
        }
        ground.grass =
            Some(Arc::new(GrassMap::from_tiles(ground.heights.tiles_x(), ground.heights.tiles_z(), words, tiles_now)));
    }
    Some(area)
}

/// A sample's shares with surface `surface` laid over them, `amount` of
/// the way to all of it.
fn laid(packed: [u32; 2], surface: u8, amount: f32) -> [u32; 2] {
    let [ids, shares] = packed;
    let mut all: Vec<(u32, f32)> = (0..SHARES)
        .map(|i| ((ids >> (8 * i)) & 0xFF, ((shares >> (8 * i)) & 0xFF) as f32 / 255.0 * (1.0 - amount)))
        .filter(|(_, share)| *share > 0.0)
        .collect();
    match all.iter_mut().find(|(id, _)| *id == u32::from(surface)) {
        Some(found) => found.1 += amount,
        None => all.push((u32::from(surface), amount)),
    }
    all.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut best = [(0u32, 0.0f32); SHARES];
    for (place, share) in best.iter_mut().zip(all) {
        *place = share;
    }
    crate::surfaces::pack(&best)
}

const MAGIC: &[u8; 8] = b"VXTEDITS";
/// 2 added the dug ground's chunks after the tiles.
const VERSION: u32 = 2;
/// Which form edits are kept in: the tiles they changed, or the edits.
const TILES: u8 = 0;
const COMMANDS: u8 = 1;
/// The most edits a terrain remembers one by one; past them, only the
/// tiles they changed.
const MOST_COMMANDS: usize = 4096;

/// A terrain's ground as built and what scripts did to it since: on a
/// terrain's Obj from its first edit, for a save of the scene or a player
/// joining a game, which get the smaller of the two forms: the edits one by
/// one, to apply again in order, or the tiles they changed.
#[derive(Clone, Debug)]
pub struct TerrainEdits {
    built: Terrain,
    /// Every edit since it was built, in order, while those alone changed
    /// it and are not too many.
    commands: Option<Vec<Edit>>,
}

impl voxear_ecs::Component for TerrainEdits {}

impl TerrainEdits {
    /// Edits from `built` on, none yet.
    pub fn new(built: &Terrain) -> Self {
        Self { built: built.clone(), commands: Some(Vec::new()) }
    }

    /// The terrain as it was built.
    pub fn built(&self) -> &Terrain {
        &self.built
    }

    /// Whether anything changed it since it was built, as far as is known:
    /// an edit, or edits loaded.
    pub fn has_edits(&self) -> bool {
        self.commands.as_ref().is_none_or(|commands| !commands.is_empty())
    }

    /// Applies `edit` to `terrain`, as [`apply`] does, and remembers it.
    pub fn apply(&mut self, terrain: &mut Terrain, edit: &Edit) -> Option<Area> {
        let area = apply(terrain, edit)?;
        if let Some(commands) = &mut self.commands {
            commands.push(*edit);
            if commands.len() > MOST_COMMANDS {
                self.commands = None;
            }
        }
        Some(area)
    }

    /// What changed from the terrain as built to `now`, in whichever form
    /// is smaller.
    pub fn save(&self, now: &Terrain) -> Vec<u8> {
        let tiles = write_edits(&self.built, now);
        match &self.commands {
            Some(commands) => Some(write_commands(commands)).filter(|c| c.len() <= tiles.len()).unwrap_or(tiles),
            None => tiles,
        }
    }

    /// Puts `now` back as it was built, then makes the changes `bytes`
    /// (from [`save`](Self::save) or [`write_edits`]) hold.
    pub fn load(&mut self, now: &mut Terrain, bytes: &[u8]) -> Result<()> {
        let loaded = match read_saved(&self.built, bytes)? {
            Saved::Commands(commands) => {
                let mut loaded = self.built.clone();
                commands.iter().for_each(|edit| _ = apply(&mut loaded, edit));
                self.commands = Some(commands);
                loaded
            }
            Saved::Tiles(loaded) => {
                self.commands = None;
                loaded
            }
        };
        now.heights = loaded.heights;
        now.weights = loaded.weights;
        now.grass = loaded.grass;
        now.dug = loaded.dug;
        Ok(())
    }
}

/// Edits one by one, to apply again in order.
pub fn write_commands(edits: &[Edit]) -> Vec<u8> {
    let mut out = Vec::with_capacity(17 + edits.len() * 19);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.push(COMMANDS);
    out.extend_from_slice(&(edits.len() as u32).to_le_bytes());
    for edit in edits {
        let bytes = edit.to_bytes();
        out.push(bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
    out
}

/// What saved edits hold: the edits, or the terrain with the tiles they
/// changed.
enum Saved {
    Commands(Vec<Edit>),
    Tiles(Terrain),
}

/// What changed from `built` to `now`, as few bytes as say it: each tile
/// changed, its heights, holes, shares and grass as they differ from the
/// tile as built (each word XORed with the built one's, so what did not
/// change is nothing), split into planes of bytes and compressed.
pub fn write_edits(built: &Terrain, now: &Terrain) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.push(TILES);
    let tiles = built.heights.tiles().len();
    let mut parts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    for index in 0..tiles.min(now.heights.tiles().len()) {
        let (was, is) = (&built.heights.tiles()[index], &now.heights.tiles()[index]);
        let mut flags = 0u8;
        let mut words: Vec<u32> = Vec::new();
        // Each part only where a word of it changed.
        let mut part = |flag: u8, changes: Vec<u32>| {
            if changes.iter().any(|&word| word != 0) {
                flags |= flag;
                words.extend(changes);
            }
        };
        if !Arc::ptr_eq(was, is) {
            let (was_samples, is_samples) = (was.samples(), is.samples());
            match stepped(was_samples, is_samples) {
                Some(steps) => part(1, steps),
                None => part(16, was_samples.iter().zip(is_samples).map(|(a, b)| a.to_bits() ^ b.to_bits()).collect()),
            }
            let zero = [0u32; HOLE_WORDS];
            let (a, b) = (was.holes().unwrap_or(&zero), is.holes().unwrap_or(&zero));
            part(2, a.iter().zip(b).map(|(a, b)| a ^ b).collect());
        }
        if let (Some(a), Some(b)) = (&built.weights, &now.weights)
            && !Arc::ptr_eq(&a.tiles()[index], &b.tiles()[index])
        {
            let (a, b) = (a.tiles()[index].samples(), b.tiles()[index].samples());
            part(4, a.iter().zip(b).flat_map(|(a, b)| [a[0] ^ b[0], a[1] ^ b[1]]).collect());
        }
        if let (Some(a), Some(b)) = (&built.grass, &now.grass)
            && a.words() == b.words()
            && !Arc::ptr_eq(&a.tiles()[index], &b.tiles()[index])
        {
            part(8, a.tiles()[index].samples().iter().zip(b.tiles()[index].samples()).map(|(a, b)| a ^ b).collect());
        }
        if flags != 0 {
            parts.push((index as u32, flags, planes(&words)));
        }
    }
    out.extend_from_slice(&(parts.len() as u32).to_le_bytes());
    for (index, flags, bytes) in parts {
        let packed = zstd::bulk::compress(&bytes, 9).expect("compressing in memory");
        out.extend_from_slice(&index.to_le_bytes());
        out.push(flags);
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
    }
    // Then each dug chunk changed, as it differs from the one built, or from the heights where none was.
    let empty = crate::dig::Excavation::default();
    let (was, is) = (built.dug.as_deref().unwrap_or(&empty), now.dug.as_deref().unwrap_or(&empty));
    let grid = crate::dig::Grid::of(&built.heights);
    let mut dug = Vec::new();
    for (key, chunk) in is.chunks() {
        let base = match was.chunk(*key) {
            Some(same) if Arc::ptr_eq(same, chunk) => continue,
            Some(before) => before.as_ref().clone(),
            None => crate::dig::Excavation::fresh(&built.heights, grid, *key),
        };
        let mut words: Vec<u32> =
            base.density.iter().zip(chunk.density.iter()).map(|(a, b)| a.to_bits() ^ b.to_bits()).collect();
        let marks: Vec<u8> = base.dug.iter().zip(chunk.dug.iter()).map(|(a, b)| a ^ b).collect();
        words.extend(
            marks.chunks(4).map(|four| u32::from_le_bytes(std::array::from_fn(|i| four.get(i).copied().unwrap_or(0)))),
        );
        if words.iter().any(|&word| word != 0) {
            dug.push((*key, planes(&words)));
        }
    }
    out.extend_from_slice(&(dug.len() as u32).to_le_bytes());
    for (key, bytes) in dug {
        let packed = zstd::bulk::compress(&bytes, 9).expect("compressing in memory");
        for v in [key.x, key.y, key.z] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
    }
    out
}

/// The heights changed from `built` to `now` as steps: which samples
/// changed, a bit each, then how far each one's steps from the one built
/// are from what its neighbours before it foretell (zigzagged, small either
/// way), since the ground an edit leaves is smooth. Empty where none
/// changed; none where one is not on a step.
fn stepped(built: &[f32], now: &[f32]) -> Option<Vec<u32>> {
    let mut changed = vec![0u32; HOLE_WORDS];
    let mut steps = vec![0i32; TILE_LEN];
    let mut out = Vec::new();
    for (index, (&was, &is)) in built.iter().zip(now).enumerate() {
        if was.to_bits() == is.to_bits() {
            continue;
        }
        steps[index] = steps_from(was, is)?;
        changed[index / 32] |= 1 << (index % 32);
        out.push(zigzag(steps[index].wrapping_sub(foretold(&steps, index))));
    }
    if out.is_empty() {
        return Some(Vec::new());
    }
    changed.extend(out);
    Some(changed)
}

/// What a sample's steps are, from the samples before it, left, above and
/// above left, as if the ground there were a slope: nothing where they
/// changed nothing.
fn foretold(steps: &[i32], index: usize) -> i32 {
    let row = TILE_SAMPLES as usize;
    let (x, z) = (index % row, index / row);
    let left = if x > 0 { steps[index - 1] } else { 0 };
    let up = if z > 0 { steps[index - row] } else { 0 };
    let corner = if x > 0 && z > 0 { steps[index - row - 1] } else { 0 };
    left.wrapping_add(up).wrapping_sub(corner)
}

fn zigzag(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

fn unzigzag(value: u32) -> i32 {
    (value >> 1) as i32 ^ -((value & 1) as i32)
}

/// Words as four planes of bytes, lowest first, which compress better.
fn planes(words: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; words.len() * 4];
    for (i, word) in words.iter().enumerate() {
        for (plane, byte) in word.to_le_bytes().into_iter().enumerate() {
            out[plane * words.len() + i] = byte;
        }
    }
    out
}

fn words_of(planes: &[u8]) -> Vec<u32> {
    let count = planes.len() / 4;
    (0..count).map(|i| u32::from_le_bytes([0, 1, 2, 3].map(|plane| planes[plane * count + i]))).collect()
}

/// `built` with what `bytes` (from [`write_edits`], [`write_commands`] or
/// [`TerrainEdits::save`]) says changed.
pub fn read_edits(built: &Terrain, bytes: &[u8]) -> Result<Terrain> {
    Ok(match read_saved(built, bytes)? {
        Saved::Commands(commands) => {
            let mut loaded = built.clone();
            commands.iter().for_each(|edit| _ = apply(&mut loaded, edit));
            loaded
        }
        Saved::Tiles(loaded) => loaded,
    })
}

fn read_saved(built: &Terrain, bytes: &[u8]) -> Result<Saved> {
    ensure!(bytes.get(..8) == Some(MAGIC), "not a terrain's edits");
    let number = |at: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(bytes.get(at..at + 4).context("the edits end early")?.try_into().expect("four")))
    };
    let version = number(8)?;
    ensure!((1..=VERSION).contains(&version), "edits of version {version}");
    let form = *bytes.get(12).context("the edits end early")?;
    let count = number(13)? as usize;
    let mut at = 17;
    if form == COMMANDS {
        let mut commands = Vec::with_capacity(count.min(MOST_COMMANDS));
        for _ in 0..count {
            let len = usize::from(*bytes.get(at).context("the edits end early")?);
            commands.push(Edit::from_bytes(bytes.get(at + 1..at + 1 + len).context("the edits end early")?)?);
            at += 1 + len;
        }
        return Ok(Saved::Commands(commands));
    }
    ensure!(form == TILES, "edits in a form of {form}");
    let mut ground = built.clone();
    let mut heights: Vec<(u32, u32, HeightTile)> = Vec::new();
    let mut weights = built.weights.as_ref().map(|w| w.tiles().to_vec());
    let mut grass = built.grass.as_ref().map(|g| g.tiles().to_vec());
    let tiles_x = built.heights.tiles_x();
    for _ in 0..count {
        let index = number(at)? as usize;
        let flags = *bytes.get(at + 4).context("the edits end early")?;
        let (size, packed) = (number(at + 5)? as usize, number(at + 9)? as usize);
        let data = bytes.get(at + 13..at + 13 + packed).context("the edits end early")?;
        at += 13 + packed;
        ensure!(index < built.heights.tiles().len(), "tile {index} of a terrain of {}", built.heights.tiles().len());
        let words = words_of(&zstd::bulk::decompress(data, size).context("a tile's edits")?);
        let mut words = words.into_iter();
        let mut take = |count: usize| -> Result<Vec<u32>> {
            let taken: Vec<u32> = words.by_ref().take(count).collect();
            ensure!(taken.len() == count, "a tile's edits end early");
            Ok(taken)
        };
        let was = &built.heights.tiles()[index];
        let mut samples = was.samples().to_vec();
        let mut holes = was.holes().map(<[u32]>::to_vec);
        if flags & 1 != 0 {
            let changed = take(HOLE_WORDS)?;
            let count = changed.iter().map(|word| word.count_ones() as usize).sum();
            let mut residues = take(count)?.into_iter();
            let mut steps = vec![0i32; TILE_LEN];
            for (index, sample) in samples.iter_mut().enumerate() {
                if changed[index / 32] & 1 << (index % 32) != 0 {
                    let residue = unzigzag(residues.next().expect("counted"));
                    steps[index] = foretold(&steps, index).wrapping_add(residue);
                    *sample = (((*sample * STEPS).round() as i32).wrapping_add(steps[index])) as f32 / STEPS;
                }
            }
        }
        if flags & 16 != 0 {
            for (sample, change) in samples.iter_mut().zip(take(TILE_LEN)?) {
                *sample = f32::from_bits(sample.to_bits() ^ change);
            }
        }
        if flags & 2 != 0 {
            let bits = holes.get_or_insert_with(|| vec![0; HOLE_WORDS]);
            for (word, change) in bits.iter_mut().zip(take(HOLE_WORDS)?) {
                *word ^= change;
            }
        }
        if flags & (1 | 2 | 16) != 0 {
            let (tx, tz) = (index as u32 % tiles_x, index as u32 / tiles_x);
            heights.push((tx, tz, HeightTile::from_samples(samples).with_holes(holes.map(Vec::into_boxed_slice))));
        }
        if flags & 4 != 0 {
            let tiles = weights.as_mut().context("edits to surfaces of a terrain without any")?;
            let changes = take(TILE_LEN * 2)?;
            let samples = tiles[index].samples().iter().zip(changes.chunks(2)).map(|(s, c)| [s[0] ^ c[0], s[1] ^ c[1]]);
            tiles[index] = Arc::new(WeightTile::from_samples(samples.collect()));
        }
        if flags & 8 != 0 {
            let tiles = grass.as_mut().context("edits to grass of a terrain without any")?;
            let count = tiles[index].samples().len();
            let samples = tiles[index].samples().iter().zip(take(count)?).map(|(s, c)| s ^ c).collect();
            tiles[index] = Arc::new(GrassTile::from_samples(samples));
        }
    }
    if !heights.is_empty() {
        ground.heights = Arc::new(built.heights.with_tiles(heights));
    }
    if let (Some(tiles), Some(was)) = (weights, &built.weights) {
        ground.weights = Some(Arc::new(Weights::from_tiles(was.tiles_x(), was.tiles_z(), tiles)));
    }
    if let (Some(tiles), Some(was)) = (grass, &built.grass) {
        ground.grass = Some(Arc::new(GrassMap::from_tiles(tiles_x, built.heights.tiles_z(), was.words(), tiles)));
    }
    if version >= 2 {
        let count = number(at)? as usize;
        at += 4;
        let grid = crate::dig::Grid::of(&built.heights);
        let mut dug = built.dug.as_deref().cloned().unwrap_or_default();
        let corners = crate::dig::CORNERS.pow(3);
        for _ in 0..count {
            let key =
                crate::dig::ChunkKey { x: number(at)? as i32, y: number(at + 4)? as i32, z: number(at + 8)? as i32 };
            let (size, packed) = (number(at + 12)? as usize, number(at + 16)? as usize);
            let data = bytes.get(at + 20..at + 20 + packed).context("the edits end early")?;
            at += 20 + packed;
            let words = words_of(&zstd::bulk::decompress(data, size).context("a dug chunk's edits")?);
            ensure!(words.len() == corners + corners.div_ceil(4), "a dug chunk's edits of the wrong size");
            let mut chunk = match dug.chunk(key) {
                Some(before) => before.as_ref().clone(),
                None => crate::dig::Excavation::fresh(&built.heights, grid, key),
            };
            for (density, change) in chunk.density.iter_mut().zip(&words) {
                *density = f32::from_bits(density.to_bits() ^ change);
            }
            let marks = words[corners..].iter().flat_map(|word| word.to_le_bytes());
            for (mark, change) in chunk.dug.iter_mut().zip(marks) {
                *mark ^= change;
            }
            dug = dug.with_chunk(key, chunk);
        }
        if !dug.is_empty() {
            ground.dug = Some(Arc::new(dug));
        }
    }
    Ok(Saved::Tiles(ground))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field() -> Terrain {
        let mut terrain = Terrain::new(Heightmap::from_fn(2, 2, 0.5, |x, z| (x * 0.05).sin() + z * 0.01));
        let shares = (0..4).map(|_| Arc::new(WeightTile::all(0))).collect();
        terrain.weights = Some(Arc::new(Weights::from_tiles(2, 2, shares)));
        let grass = (0..4).map(|_| Arc::new(GrassTile::from_samples(vec![0xFF; TILE_LEN]))).collect();
        terrain.grass = Some(Arc::new(GrassMap::from_tiles(2, 2, 1, grass)));
        terrain
    }

    #[test]
    fn a_crater_on_a_tiles_edge_digs_both_clears_its_grass_and_lays_its_dirt() {
        let built = field();
        // On the line between the first two tiles, 128 m along.
        let crater = Edit::Crater { at: [128.0, 40.0], radius: 4.0, depth: 2.0, surface: Some(2) };
        let mut now = built.clone();
        let area = apply(&mut now, &crater);
        assert!(area.is_some_and(|[x0, _, x1, _]| x0 < 123.0 && x1 > 133.0));
        let before = built.heights.height_at(128.0, 40.0);
        assert!((now.heights.height_at(128.0, 40.0) - (before - 2.0)).abs() < 1e-3, "two metres down at its middle");
        assert!(now.heights.height_at(132.0, 40.0) > built.heights.height_at(132.0, 40.0), "its rim thrown up");
        assert_eq!(now.heights.height_at(140.0, 40.0), built.heights.height_at(140.0, 40.0), "nothing past its rim");
        // The edge samples, in both tiles, agree.
        let (left, right) = (now.heights.tile(0, 0), now.heights.tile(1, 0));
        assert_eq!(left.get(256, 80), right.get(0, 80));
        assert!(Arc::ptr_eq(now.heights.tile(0, 1), built.heights.tile(0, 1)), "tiles it misses are kept");
        assert_eq!(now.grass.as_ref().unwrap().density_at(0, 256, 80), 0, "its grass gone");
        assert_eq!(now.weights.as_ref().unwrap().shares_at(256, 80)[0], (2, 255), "dirt on its walls");
        assert_eq!(now.weights.as_ref().unwrap().shares_at(300, 80)[0], (0, 255));
    }

    #[test]
    fn edits_read_back_exactly_from_a_few_bytes_and_as_commands() {
        let built = field();
        let edits = [
            Edit::Crater { at: [30.0, 30.0], radius: 5.0, depth: 1.5, surface: Some(1) },
            Edit::Raise { at: [200.0, 60.0], radius: 6.0, height: -0.8 },
            Edit::Flatten { at: [60.0, 200.0], radius: 3.0, height: 0.0 },
            Edit::Paint { at: [90.0, 90.0], radius: 2.0, surface: 3, strength: 0.5 },
            Edit::Holes { at: [150.0, 150.0], radius: 1.5, cut: true },
            Edit::Grass { at: [10.0, 240.0], radius: 3.0, kind: Some(0), amount: 0.0 },
        ];
        let mut now = built.clone();
        for edit in &edits {
            let bytes = edit.to_bytes();
            assert!(bytes.len() <= 18, "{} bytes", bytes.len());
            assert_eq!(Edit::from_bytes(&bytes).unwrap(), *edit);
            apply(&mut now, edit);
        }
        let saved = write_edits(&built, &now);
        assert!(saved.len() < 16 * 1024, "six edits in {} bytes", saved.len());
        let loaded = read_edits(&built, &saved).expect("edits");
        for (a, b) in loaded.heights.tiles().iter().zip(now.heights.tiles()) {
            assert_eq!(a.samples(), b.samples());
            assert_eq!(a.holes(), b.holes());
        }
        assert_eq!(loaded.weights, now.weights);
        assert_eq!(loaded.grass, now.grass);
        assert!(write_edits(&built, &built).len() <= 21, "nothing changed is nothing written");
        assert!(read_edits(&built, b"VXTEDITS").is_err());
        let mut same = built.clone();
        assert_eq!(
            apply(&mut same, &Edit::Raise { at: [-50.0, 10.0], radius: 2.0, height: 1.0 }),
            None,
            "off the terrain"
        );
        assert_eq!(apply(&mut same, &Edit::Raise { at: [10.0, 10.0], radius: f32::NAN, height: 1.0 }), None);
        assert!(Arc::ptr_eq(&same.heights, &built.heights));
    }

    #[test]
    fn a_terrains_edits_save_as_whichever_form_is_smaller() {
        let built = field();
        let mut kept = TerrainEdits::new(&built);
        let mut now = built.clone();
        let crater = Edit::Crater { at: [40.0, 40.0], radius: 3.0, depth: 1.0, surface: None };
        kept.apply(&mut now, &crater);
        let saved = kept.save(&now);
        assert!(saved.len() < 40, "one crater as itself, {} bytes", saved.len());
        // Many craters in one place change the same tiles: those are smaller.
        for i in 0..200 {
            kept.apply(
                &mut now,
                &Edit::Crater { at: [40.0 + (i % 7) as f32, 40.0], radius: 3.0, depth: 0.1, surface: None },
            );
        }
        let saved = kept.save(&now);
        assert!(saved.len() < write_commands(&[crater; 201]).len(), "{} bytes", saved.len());
        // Either form loads back the same ground, on another machine as built.
        let mut joined = built.clone();
        TerrainEdits::new(&built).load(&mut joined, &saved).expect("edits");
        assert_eq!(joined, now);
        let mut joined = built.clone();
        TerrainEdits::new(&built).load(&mut joined, &write_commands(&[crater])).expect("edits");
        assert!(joined.heights.height_at(40.0, 40.0) < built.heights.height_at(40.0, 40.0) - 0.9);
    }

    #[test]
    fn digging_reads_back_exactly_from_its_commands_and_its_chunks() {
        let built = field();
        let mut kept = TerrainEdits::new(&built);
        let mut now = built.clone();
        let ground = built.heights.height_at(40.0, 40.0);
        let edits = [
            Edit::Dig { at: [40.0, ground, 40.0], radius: 2.0, surface: Some(1) },
            Edit::Tunnel {
                from: [36.0, ground - 1.0, 40.0],
                to: [44.0, ground - 1.0, 40.0],
                radius: 0.8,
                surface: None,
            },
            Edit::Fill { at: [40.0, ground - 0.5, 41.0], radius: 0.6 },
        ];
        for edit in &edits {
            assert_eq!(Edit::from_bytes(&edit.to_bytes()).unwrap(), *edit);
            assert!(kept.apply(&mut now, edit).is_some());
        }
        assert!(now.dug.as_ref().is_some_and(|dug| dug.is_dug(5, 5)));
        // 40 m is on the edge between the fifth and sixth columns of 8 m: inside them and between them are holes,
        // and their edges with columns not dug are kept.
        assert!(now.heights.is_hole(82, 82) && now.heights.is_hole(80, 82), "the columns under the dig cut out");
        assert!(!now.heights.is_hole(64, 82) && !now.heights.is_hole(82, 96), "their edges with ground not dug kept");
        assert_eq!(now.dug.as_ref().unwrap().columns(), [(4, 4), (4, 5), (5, 4), (5, 5)]);
        let commands = kept.save(&now);
        let tiles = write_edits(&built, &now);
        for saved in [&commands, &tiles] {
            let mut joined = built.clone();
            TerrainEdits::new(&built).load(&mut joined, saved).expect("edits");
            assert_eq!(joined.dug, now.dug);
            assert_eq!(joined.heights.tile(0, 0).holes(), now.heights.tile(0, 0).holes());
        }
        assert!(commands.len() < 120, "three digs as themselves in {} bytes", commands.len());
        // Grass where the dig opened the ground, and none beside it lost.
        let grass = now.grass.as_ref().unwrap();
        assert_eq!(grass.density_at(0, 80, 80), 0);
        assert_eq!(grass.density_at(0, 100, 80), 255, "the dug column's grass beside the dig stays");
    }

    #[test]
    fn an_explosion_blows_a_bowl_with_a_lip_thrown_up_round_it() {
        let mut terrain = Terrain::new(Heightmap::flat(1, 1, 0.5, 0.0));
        let blast = Edit::Explode { at: [64.0, 0.0, 64.0], radius: 2.0, surface: None };
        assert_eq!(Edit::from_bytes(&blast.to_bytes()).unwrap(), blast);
        apply(&mut terrain, &blast).expect("on the terrain");
        let dug = terrain.dug.as_ref().unwrap();
        let (middle, _) = dug.surface_at(&terrain.heights, 64.0, 64.0).unwrap();
        assert!(middle < -2.0, "a bowl {middle} deep, its middle a third under the point");
        let (lip, _) = dug.surface_at(&terrain.heights, 66.2, 64.0).unwrap();
        assert!(lip > 0.15, "earth thrown up round it: {lip}");
    }

    #[test]
    fn a_tile_cratered_all_over_reads_back_exactly_in_either_form() {
        let mut built = Terrain::new(Heightmap::from_fn(1, 1, 0.5, |x, z| (x * 0.05).sin() + z * 0.01));
        built.weights = Some(Arc::new(Weights::from_tiles(1, 1, vec![Arc::new(WeightTile::all(0))])));
        let mut kept = TerrainEdits::new(&built);
        let mut now = built.clone();
        let mut seed = 12345u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0
        };
        for _ in 0..300 {
            let at = [next() * 128.0, next() * 128.0];
            kept.apply(
                &mut now,
                &Edit::Crater { at, radius: 2.0 + next() * 4.0, depth: 0.5 + next(), surface: Some(1) },
            );
        }
        let commands = kept.save(&now);
        assert!(commands.len() < 300 * 20 + 32, "300 craters as themselves in {} bytes", commands.len());
        let tiles = write_edits(&built, &now);
        assert!(tiles.len() < 200 * 1024, "a tile torn up all over in {} bytes", tiles.len());
        for saved in [&commands, &tiles] {
            let loaded = read_edits(&built, saved).expect("edits");
            assert_eq!(loaded.heights.tile(0, 0).samples(), now.heights.tile(0, 0).samples());
            assert_eq!(loaded.weights, now.weights);
        }
        // Each height an edit set is a whole number of steps.
        let set =
            now.heights.tile(0, 0).samples().iter().zip(built.heights.tile(0, 0).samples()).filter(|(a, b)| a != b);
        assert!(set.into_iter().all(|(height, _)| (height * STEPS).fract() == 0.0));
    }
}
