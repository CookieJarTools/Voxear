//! A dungeon's spaces and ways cut out of the rock: each the air it needs,
//! with an edge that wanders a block or three in and out, layered where the
//! rock is, and a vaulted or domed roof, so the rock rounds into the space
//! rather than meeting it in a box. Built stone is laid into the air after.

use super::build::{Grid, Tag};
use super::noise;
use super::plan::{DungeonLayout, Space, SpaceKind, Way, WayKind};
use crate::blocks::chunk::AIR;

/// How far a space's wall wanders out past its plan, at the most, in
/// blocks, by its kind: natural spaces most.
fn wander(kind: SpaceKind) -> f32 {
    match kind {
        SpaceKind::Gorge | SpaceKind::Chasm => 3.5,
        SpaceKind::Descent => 1.5,
        SpaceKind::Hall | SpaceKind::Flooded | SpaceKind::Arena => 2.5,
        _ => 2.0,
    }
}

/// How square a space is in plan: 2 for round, higher for boxier.
fn squareness(space: &Space) -> f32 {
    match space.kind {
        SpaceKind::Arena | SpaceKind::Descent => 2.0,
        SpaceKind::Chasm => 3.0,
        SpaceKind::Gorge => 4.0,
        _ => 7.0,
    }
}

/// Where a space reaches at column `(x, z)`, by its plan: how far out from
/// its middle as a share of its half, along the long way and across, and
/// the share of its outline (1 at its edge), each with the wander added.
fn spread(space: &Space, x: i32, z: i32, y: i32) -> (f32, f32) {
    let w = wander(space.kind);
    let p = [x as f32 + 0.5, y as f32, z as f32 + 0.5];
    let wobble = (noise::rough(p, 7.0, [1.0, 0.55, 1.0], space.seed) * 0.5 + 0.5) * w;
    let nx = (x - space.at[0]) as f32 / (space.half[0] as f32 + wobble);
    let nz = (z - space.at[2]) as f32 / (space.half[1] as f32 + wobble);
    let k = squareness(space);
    let outline = (nx.abs().powf(k) + nz.abs().powf(k)).powf(1.0 / k);
    let across = if space.along_x { nz.abs() } else { nx.abs() };
    (outline, across)
}

/// Carves every space and way of `layout` that reaches `grid`.
pub(crate) fn carve(layout: &DungeonLayout, grid: &mut Grid) {
    let (lo, hi) = grid.columns();
    for space in layout.spaces.iter().filter(|space| space.height >= 0) {
        let reach = wander(space.kind).ceil() as i32 + 1;
        let (slo, shi) = (space.low(), space.high());
        if shi[0] + reach < lo[0] || slo[0] - reach > hi[0] || shi[1] + reach < lo[1] || slo[1] - reach > hi[1] {
            continue;
        }
        carve_space(space, grid);
    }
    for &(at, [from, to]) in &layout.shafts {
        if at[0] + 4 < lo[0] || at[0] - 4 > hi[0] || at[1] + 4 < lo[1] || at[1] - 4 > hi[1] {
            continue;
        }
        // A crack two or three blocks across, wandering a little as it rises.
        for y in from..=to {
            let sway = [
                noise::smooth([0.0, y as f32, 0.0], 9.0, [1.0, 1.0, 1.0], at[0] as u32) * 2.0,
                noise::smooth([0.0, y as f32, 0.0], 9.0, [1.0, 1.0, 1.0], at[1] as u32 ^ 0x51) * 2.0,
            ];
            for dz in -3..=3 {
                for dx in -3..=3 {
                    let (x, z) = (at[0] + dx, at[1] + dz);
                    let d = ((dx as f32 - sway[0]).powi(2) + (dz as f32 - sway[1]).powi(2)).sqrt();
                    let width = 1.3 + noise::unit(x, y / 3, z, 0x5A) * 0.6;
                    if d <= width {
                        grid.set([x, y, z], AIR, Tag::Air);
                    }
                }
            }
        }
    }
    for way in &layout.ways {
        if way.high[0] + 3 < lo[0] || way.low[0] - 3 > hi[0] || way.high[1] + 3 < lo[1] || way.low[1] - 3 > hi[1] {
            continue;
        }
        carve_way(way, grid);
    }
}

fn carve_space(space: &Space, grid: &mut Grid) {
    let (lo, hi) = grid.columns();
    let reach = wander(space.kind).ceil() as i32 + 1;
    let floor = space.at[1];
    let top = space.top();
    for z in (space.at[2] - space.half[1] - reach).max(lo[1])..=(space.at[2] + space.half[1] + reach).min(hi[1]) {
        for x in (space.at[0] - space.half[0] - reach).max(lo[0])..=(space.at[0] + space.half[0] + reach).min(hi[0]) {
            for y in (space.bottom() + 1).max(grid.low[1])..=(top + 4).min(grid.top()) {
                if open(space, x, y, z, floor, top) {
                    grid.set([x, y, z], AIR, Tag::Air);
                }
            }
        }
    }
}

/// Whether block `(x, y, z)` is open air in `space`, whose floor block is at
/// `floor` and whose highest open block is about `top`.
fn open(space: &Space, x: i32, y: i32, z: i32, floor: i32, top: i32) -> bool {
    let (outline, across) = spread(space, x, z, y);
    if outline > 1.0 {
        return false;
    }
    let p = [x as f32 + 0.5, y as f32, z as f32 + 0.5];
    let roof_wobble = noise::rough([p[0], 0.0, p[2]], 6.0, [1.0, 1.0, 1.0], space.seed ^ 0x5A17) * wander(space.kind);
    match space.kind {
        // A pointed vault over its length: walls two thirds of its height, then rising to a ridge.
        SpaceKind::Hall | SpaceKind::Flooded => {
            if y <= floor - if space.kind == SpaceKind::Flooded { space.below } else { 0 } {
                return false;
            }
            // The flooded hall's basin is narrower than the hall: walkways along its sides.
            if y <= floor && across > 0.62 {
                return false;
            }
            let wall = space.height as f32 * 0.62;
            let rise = space.height as f32 - wall;
            let roof = floor as f32 + 1.0 + wall + rise * (1.0 - across.powf(1.4)).max(0.0) + roof_wobble;
            (y as f32) < roof && outline <= 1.0
        }
        // Layered walls, wider as they rise, overhanging here and there.
        SpaceKind::Gorge => {
            if y <= floor {
                return false;
            }
            let bed = noise::smooth([0.0, (y / 3) as f32 * 3.0, 0.0], 9.0, [1.0, 1.0, 1.0], space.seed ^ 0x6E57) * 0.18;
            let widen = (y - floor) as f32 / (top - floor).max(1) as f32 * 0.35;
            let width = 0.62 + widen + bed;
            let roof = top as f32 + roof_wobble;
            across <= width && (y as f32) < roof
        }
        // A dome over walkways round a pit; the pit narrower than the dome, down to its bed.
        SpaceKind::Chasm => {
            if y <= floor {
                return y > floor - space.below
                    && outline < 0.66 + noise::smooth(p, 5.0, [1.0, 0.5, 1.0], space.seed) * 0.06;
            }
            let roof =
                floor as f32 + 1.0 + space.height as f32 * (1.0 - outline * outline).max(0.0).sqrt() + roof_wobble;
            (y as f32) < roof
        }
        SpaceKind::Arena => {
            if y <= floor {
                return false;
            }
            let roof = floor as f32
                + 1.0
                + space.height as f32 * (1.0 - outline.powf(2.5)).max(0.0).sqrt().max(0.55)
                + roof_wobble;
            (y as f32) < roof
        }
        // A round shaft from its foot to over its upper level, a well in its middle going on down.
        SpaceKind::Descent => {
            if y <= floor {
                return y > floor - space.below && outline < 0.28;
            }
            (y as f32) < top as f32 + roof_wobble.abs()
        }
        // Rooms: a shallow vault over walls most of its height.
        _ => {
            if y <= floor {
                return false;
            }
            let wall = space.height as f32 * 0.75;
            let rise = space.height as f32 - wall;
            let short = if space.half[0] < space.half[1] {
                (x - space.at[0]) as f32 / space.half[0].max(1) as f32
            } else {
                (z - space.at[2]) as f32 / space.half[1].max(1) as f32
            };
            let roof = floor as f32 + 1.0 + wall + rise * (1.0 - short.abs().min(1.0).powi(2)) + roof_wobble * 0.6;
            (y as f32) < roof
        }
    }
}

/// A way's half width at its walls, in blocks, its wobble added at block
/// `p`.
fn way_half(way: &Way, p: [f32; 3]) -> f32 {
    let r = way.width as f32 * 0.5;
    let built = way.kind == WayKind::Grand;
    let wobble = noise::rough(p, 5.0, [1.0, 0.6, 1.0], way.to as u32 ^ 0xA11E) * 0.5 + 0.5;
    r + if built { 0.4 } else { wobble * 1.6 }
}

fn carve_way(way: &Way, grid: &mut Grid) {
    let (lo, hi) = grid.columns();
    for cell in &way.cells {
        let [x, z] = cell.at;
        if x < lo[0] || x > hi[0] || z < lo[1] || z > hi[1] {
            continue;
        }
        let from = cell.surface.div_euclid(2);
        for y in from..=from + way.height + 2 {
            let p = [x as f32 + 0.5, y as f32, z as f32 + 0.5];
            let half = way_half(way, p);
            if cell.off > half {
                continue;
            }
            // An arched roof: full height in the middle, lower at the walls.
            let u = (cell.off / half).min(1.0);
            let roof = from as f32 + way.height as f32 * (0.7 + 0.3 * (1.0 - u * u).sqrt()) + 0.5;
            if (y as f32) < roof {
                grid.set([x, y, z], AIR, Tag::Air);
            }
        }
    }
}
