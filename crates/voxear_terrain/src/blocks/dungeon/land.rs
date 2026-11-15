//! The land over and round a dungeon: the ground's top on a coarse grid
//! round its site, what keeps every space under it, and where its entrance
//! goes.

use super::EntranceKind;
use super::noise;
use crate::blocks::structure::Ground;

/// How far apart the land is read, in blocks.
pub(crate) const STEP: i32 = 4;

/// How far round each reading the lowest ground is looked for, in
/// readings: what keeps rock beside a space's walls as well as over it.
const SIDE: i32 = 2;

/// The ground's top on a grid round a dungeon's site, read a tile at a time
/// as the plan asks for it, and the lowest of it near each reading.
pub(crate) struct Land<'a> {
    ground: &'a Ground<'a>,
    tiles: std::cell::RefCell<std::collections::HashMap<[i32; 2], Vec<i32>>>,
    wet: std::cell::RefCell<std::collections::HashMap<[i32; 2], Option<f32>>>,
    pub(crate) sea: i32,
}

/// How many readings a side a tile holds.
const TILE: i32 = 16;

impl<'a> Land<'a> {
    pub(crate) fn new(ground: &'a Ground<'a>) -> Self {
        let sea = (ground.world.sea_level() / ground.size - 0.5).ceil() as i32 - 1;
        Self { ground, tiles: Default::default(), wet: Default::default(), sea }
    }

    /// The ground it reads.
    pub(crate) fn ground(&self) -> &'a Ground<'a> {
        self.ground
    }

    /// The reading nearest block column `at`, by its place on the grid.
    fn reading(at: [i32; 2]) -> [i32; 2] {
        [(at[0] + STEP / 2).div_euclid(STEP), (at[1] + STEP / 2).div_euclid(STEP)]
    }

    /// The ground's top at reading `cell`.
    fn read(&self, cell: [i32; 2]) -> i32 {
        let tile = [cell[0].div_euclid(TILE), cell[1].div_euclid(TILE)];
        let inside = [cell[0].rem_euclid(TILE), cell[1].rem_euclid(TILE)];
        let mut tiles = self.tiles.borrow_mut();
        let tops = tiles.entry(tile).or_insert_with(|| {
            let columns: Vec<[i32; 2]> = (0..TILE)
                .flat_map(|z| (0..TILE).map(move |x| [(tile[0] * TILE + x) * STEP, (tile[1] * TILE + z) * STEP]))
                .collect();
            self.ground.tops(&columns)
        });
        tops[(inside[1] * TILE + inside[0]) as usize]
    }

    /// The ground's top nearest block column `at`.
    pub(crate) fn top(&self, at: [i32; 2]) -> i32 {
        self.read(Self::reading(at))
    }

    /// The lowest ground within a few blocks of column `at`.
    pub(crate) fn lowest(&self, at: [i32; 2]) -> i32 {
        let c = Self::reading(at);
        let mut least = i32::MAX;
        for dz in -SIDE..=SIDE {
            for dx in -SIDE..=SIDE {
                least = least.min(self.read([c[0] + dx, c[1] + dz]));
            }
        }
        least
    }

    /// The lowest ground over and round the blocks from `low` to `high`,
    /// along x and z.
    pub(crate) fn lowest_over(&self, low: [i32; 2], high: [i32; 2]) -> i32 {
        let (a, b) = (Self::reading(low), Self::reading(high));
        let mut least = i32::MAX;
        for z in a[1] - SIDE..=b[1] + SIDE {
            for x in a[0] - SIDE..=b[0] + SIDE {
                least = least.min(self.read([x, z]));
            }
        }
        least
    }

    /// How high water stands over column `at`, in blocks: the sea's where
    /// the ground is under it, a river's or a lake's; none where it is dry.
    fn water(&self, at: [i32; 2]) -> Option<f32> {
        let cell = Self::reading(at);
        if let Some(kept) = self.wet.borrow().get(&cell) {
            return *kept;
        }
        let top = self.read(cell);
        let ground = self.ground;
        let column = [cell[0] * STEP, cell[1] * STEP];
        let x = ground.position[0] + (column[0] as f32 + 0.5) * ground.size;
        let z = ground.position[1] + (column[1] as f32 + 0.5) * ground.size;
        let river = ground.world.water(&[x], &[z]).map(|(water, _)| water[0] / ground.size);
        let level = match river {
            Some(level) if level > top as f32 + 0.5 => Some(level),
            _ if top <= self.sea => Some(self.sea as f32 + 1.0),
            _ => None,
        };
        self.wet.borrow_mut().insert(cell, level);
        level
    }

    fn wet(&self, at: [i32; 2]) -> bool {
        self.water(at).is_some()
    }

    /// Whether water stands at or over `level` anywhere before an entrance
    /// at `at` facing `out`: over its terrace and its approach, 20 blocks
    /// out and 14 either side.
    fn flooded_before(&self, at: [i32; 2], out: [i32; 2], level: i32) -> bool {
        let side = [-out[1], out[0]];
        (0..=20).step_by(4).any(|ahead| {
            (-14..=14).step_by(7).any(|right| {
                let p = [at[0] + out[0] * ahead + side[0] * right, at[1] + out[1] * ahead + side[1] * right];
                self.water(p).is_some_and(|water| water >= level as f32)
            })
        })
    }

    /// Where round block `site` the entrance goes, of `kinds` (any where
    /// none), `search` blocks round it at most: the best place the land
    /// offers, by kind and then by nearness. None where it offers none.
    ///
    /// - A cliff gate where the land rises 12 or more blocks within 8, the
    ///   gate facing away from the rise.
    /// - A temple door on a slope of 20 to 40 degrees, facing down it.
    /// - A sinkhole on level ground.
    ///
    /// The grand stair runs from the entrance into the land, away from the
    /// way it faces, so wherever it is the land must stay over the stair.
    pub(crate) fn entrance(&self, site: [i32; 2], kinds: &[EntranceKind], search: f32, seed: u32) -> Option<Entrance> {
        let may = |kind: EntranceKind| kinds.is_empty() || kinds.contains(&kind);
        let ways: [[i32; 2]; 4] = [[1, 0], [0, 1], [-1, 0], [0, -1]];
        let reach = (search as i32 / STEP) * STEP;
        let mut best: Option<(f32, Entrance)> = None;
        for dz in (-reach..=reach).step_by(STEP as usize) {
            for dx in (-reach..=reach).step_by(STEP as usize) {
                let distance = ((dx * dx + dz * dz) as f32).sqrt();
                if distance > search {
                    continue;
                }
                let at = [site[0] + dx, site[1] + dz];
                let height = self.top(at);
                if self.wet(at) || height <= self.sea + 1 {
                    continue;
                }
                // How far the land rises 8 blocks each way, and the way it rises most.
                let rise = |way: [i32; 2], far: i32| self.top([at[0] + way[0] * far, at[1] + way[1] * far]) - height;
                let (steepest, most) =
                    ways.iter().map(|&way| (way, rise(way, 8))).max_by_key(|(_, rise)| *rise).expect("four ways");
                let level = ways.iter().all(|&way| rise(way, 8).abs() <= 2 && rise(way, 16).abs() <= 4);
                let out = [-steepest[0], -steepest[1]];
                // The approach is open: the land before it does not rise.
                let open = rise(out, 8) <= 1 && !self.wet([at[0] + out[0] * 8, at[1] + out[1] * 8]);
                let (kind, mut score) = if may(EntranceKind::Cliff) && most >= 12 && open {
                    (EntranceKind::Cliff, 3.0 + (most as f32 / 30.0).min(1.0))
                } else if may(EntranceKind::Temple) && (3..=7).contains(&most) && open {
                    (EntranceKind::Temple, 2.0)
                } else if may(EntranceKind::Sinkhole) && level {
                    (EntranceKind::Sinkhole, 1.0)
                } else {
                    continue;
                };
                // A sinkhole's dungeon goes the way the land is highest; a gate's into the rise behind it.
                let into = match kind {
                    EntranceKind::Sinkhole => {
                        let highest =
                            ways.iter().max_by_key(|&&way| self.top([at[0] + way[0] * 40, at[1] + way[1] * 40]));
                        *highest.expect("four ways")
                    }
                    _ => steepest,
                };
                // The land stays well over a grand stair running in from it: falling one block in every two and
                // a half along, ten high, with rock over it once past the entrance's own cut.
                let covered = (24..=64).step_by(8).all(|along| {
                    let floor = height - along * 2 / 5;
                    self.lowest([at[0] + into[0] * along, at[1] + into[1] * along]) >= floor + 12
                });
                if !covered && kind != EntranceKind::Sinkhole {
                    continue;
                }
                // Never where water stands over its threshold, before it or round a sinkhole.
                let before = if kind == EntranceKind::Sinkhole { [0, 0] } else { out };
                if self.flooded_before(at, before, height)
                    || (kind == EntranceKind::Sinkhole && self.flooded_before(at, [-out[0], -out[1]], height))
                {
                    continue;
                }
                score -= distance / search * 0.5 + noise::unit(at[0], 0, at[1], seed) * 0.1;
                // How far in the cliff's face stands, where the gate is cut.
                let face = match kind {
                    EntranceKind::Cliff => (1..=12).find(|&along| rise(into, along) >= 10).unwrap_or(4),
                    _ => 0,
                };
                let entrance = Entrance { kind, at: [at[0], height, at[1]], into, face };
                if best.as_ref().is_none_or(|(least, _)| score > *least) {
                    best = Some((score, entrance));
                }
            }
        }
        best.map(|(_, entrance)| entrance)
    }
}

/// Where a dungeon's entrance stands: its kind, the block of ground at its
/// threshold, and the way into the land from it, along x or z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entrance {
    pub kind: EntranceKind,
    pub at: [i32; 3],
    pub into: [i32; 2],
    /// How many blocks in from its threshold a cliff's face stands; 0 for
    /// the other kinds.
    pub face: i32,
}

impl Entrance {
    /// How many blocks in from its threshold its grand stair starts: behind
    /// a cliff gate's innermost frame, a block in for a temple door.
    pub fn mouth(&self) -> i32 {
        match self.kind {
            EntranceKind::Cliff => self.face + 8,
            _ => 1,
        }
    }
}
