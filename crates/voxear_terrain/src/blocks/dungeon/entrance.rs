//! A dungeon's entrance at the surface, the only part of it the land does
//! not cover: a temple door in a hillside between hooded statues, a gate
//! carved into a cliff in stepped frames, or a stair round a sinkhole's
//! wall. Each has a terrace or a rim, braziers, and something standing
//! tall enough to be seen from far off.

use super::{DungeonLayout, EntranceKind};
use super::build::{Grid, Kit, Tag};
use super::noise;
use super::pieces::{self, Frame};
use crate::blocks::chunk::AIR;

/// Builds the entrance where it reaches `grid`.
pub(crate) fn build(kit: &Kit, grid: &mut Grid) {
    let entrance = kit.layout.entrance;
    let (lo, hi) = grid.columns();
    let e = entrance.at;
    if e[0] + 40 < lo[0] || e[0] - 40 > hi[0] || e[2] + 40 < lo[1] || e[2] - 40 > hi[1] {
        return;
    }
    if let Some(piece) = &kit.dungeon.entrance {
        own(kit, grid, piece);
        return;
    }
    let frame = Frame { foot: e, facing: [-entrance.into[0], -entrance.into[1]] };
    match entrance.kind {
        EntranceKind::Temple => temple(kit, grid, frame),
        EntranceKind::Cliff => cliff(kit, grid, frame),
        EntranceKind::Sinkhole => sinkhole(kit, grid, frame),
    }
}

/// Your own entrance: its model stood on the threshold, its door (the side
/// that faces -z) towards the way out, the grand stair behind it.
fn own(kit: &Kit, grid: &mut Grid, piece: &crate::blocks::structure::TownPiece) {
    let e = kit.layout.entrance;
    // Turned so its -z side faces out of the land.
    let out = [-e.into[0], -e.into[1]];
    let turns = match out {
        [0, -1] => 0,
        [1, 0] => 1,
        [0, 1] => 2,
        _ => 3,
    };
    for ([x, y, z], block) in crate::blocks::structure::model_pieces(piece.size, &piece.blocks) {
        let [tx, tz] = match turns {
            0 => [x, z],
            1 => [-z, x],
            2 => [-x, -z],
            _ => [z, -x],
        };
        let block = (kit.turn)(block, turns);
        grid.build([e.at[0] + tx, e.at[1] + 1 - piece.sink + y, e.at[2] + tz], block);
    }
}

/// The ground's top block at column `(x, z)`.
fn top(kit: &Kit, x: i32, z: i32) -> i32 {
    kit.ground.top(x, z)
}

/// A terrace before an entrance at `frame`: its floor at the threshold's
/// height from `near` to `far` blocks out and `half` either side, built up
/// under it to the ground, the ground cut away over it, low walls at its
/// sides where the ground is higher.
fn terrace(kit: &Kit, grid: &mut Grid, frame: Frame, near: i32, far: i32, half: i32) {
    let b = &kit.blocks;
    let level = frame.foot[1];
    let (lo, hi) = grid.columns();
    for ahead in near..=far {
        for right in -half - 1..=half + 1 {
            let at = frame.at(right, 0, ahead);
            if at[0] < lo[0] || at[0] > hi[0] || at[2] < lo[1] || at[2] > hi[1] {
                continue;
            }
            let ground = top(kit, at[0], at[2]);
            let side = right.abs() == half + 1;
            if side {
                for y in level + 1..=ground.max(level + 1) {
                    grid.build([at[0], y, at[2]], b.stone);
                }
                continue;
            }
            for y in ground.min(level - 1)..level {
                grid.build([at[0], y, at[2]], b.stone);
            }
            let edge = ahead == far || right.abs() == half;
            grid.keep([at[0], level, at[2]], if edge { b.trim } else { b.floor });
            for y in level + 1..=ground.max(level + 16) {
                if y > ground {
                    break;
                }
                grid.set([at[0], y, at[2]], AIR, Tag::Air);
            }
        }
    }
}

/// A temple door in a hillside: a facade of stone with a pointed arch over
/// the door, a stepped gable, two hooded statues before it, a terrace,
/// then a path of stepping stones out past broken pillars.
fn temple(kit: &Kit, grid: &mut Grid, frame: Frame) {
    let b = &kit.blocks;
    let width = kit.dungeon.sizes_stair();
    let door = width / 2;
    let spring = 8;
    let tall = 21;
    let half = 13i32;
    terrace(kit, grid, frame, 1, 12, half - 1);
    // The facade, three blocks deep into the hill, the door left open.
    for ahead in -2..=0 {
        for right in -half..=half {
            for up in 1..=tall {
                let step = (right.abs() - 6).max(0);
                if up > tall - step {
                    continue;
                }
                let u = right.abs() as f32;
                let h = door as f32 + 0.5;
                let radius = h * 1.6;
                let curve = if u <= h { (radius * radius - (u - (h - radius)).powi(2)).max(0.0).sqrt() } else { -1.0 };
                let opening = right.abs() <= door && (up as f32) <= spring as f32 + curve;
                if opening {
                    continue;
                }
                let ring = right.abs() <= door + 1 && (up as f32) <= spring as f32 + curve + 1.5;
                let block = if ring || up == tall - step || up == 1 { b.trim } else { b.stone };
                grid.build(frame.at(right, up, ahead), block);
            }
        }
    }
    // A portico behind it running back into the hill: walls either side of the stair and a roof over it, until
    // the hill rises over the roof and hides the rest.
    for ahead in -28..=-3 {
        for right in -half..=half {
            let at = frame.at(right, 0, ahead);
            if kit.ground.top(at[0], at[2]) > frame.foot[1] + tall + 1 {
                continue;
            }
            for up in 1..=tall {
                let step = (right.abs() - 6).max(0);
                let roof = up == tall - step || up == tall - step - 1;
                let wall = right.abs() >= half - 2 && up < tall - step;
                if up > tall - step || !(roof || wall) {
                    continue;
                }
                grid.build(frame.at(right, up, ahead), if roof && ahead % 6 == 0 { b.trim } else { b.stone });
            }
        }
    }
    // The statues either side of the door.
    for side in [-1, 1] {
        let foot = frame.at(side * 9, 0, 4);
        pieces::statue(kit, grid, Frame { foot, facing: frame.facing }, 18, false);
        pieces::caged_brazier(kit, grid, frame.at(side * 5, 1, 9));
    }
    // Stepping stones out across the ground, broken pillars beside them.
    let seed = frame.foot[0] as u32 ^ (frame.foot[2] as u32).rotate_left(11);
    let (lo, hi) = grid.columns();
    for ahead in 11..=34 {
        let wander = (noise::smooth([ahead as f32, 0.0, 0.0], 9.0, [1.0, 1.0, 1.0], seed) * 4.0).round() as i32;
        for right in [wander - 1, wander, wander + 1] {
            let at = frame.at(right, 0, ahead);
            if at[0] < lo[0] || at[0] > hi[0] || at[2] < lo[1] || at[2] > hi[1] {
                continue;
            }
            if (ahead + right).rem_euclid(3) == 0 || noise::unit(at[0], 0, at[2], seed) < 0.35 {
                continue;
            }
            let ground = top(kit, at[0], at[2]);
            grid.keep([at[0], ground, at[2]], b.floor);
        }
        if ahead % 8 == 6 {
            for side in [-1, 1] {
                let at = frame.at(wander + side * 5, 0, ahead);
                if at[0] < lo[0] || at[0] > hi[0] || at[2] < lo[1] || at[2] > hi[1] {
                    continue;
                }
                let ground = top(kit, at[0], at[2]);
                let height = 2 + (noise::hash(at[0], 0, at[2], seed) % 5) as i32;
                pieces::broken_column(kit, grid, [at[0], ground, at[2]], height, seed ^ at[0] as u32);
            }
        }
    }
}

/// A gate carved into a cliff: four frames stepping in and down into the
/// rock from its face, each lined with trim, a pillar in the middle of the
/// innermost dividing the way in; great statues in the outer frame; the
/// ground before the face cut level to it, and a terrace before that.
fn cliff(kit: &Kit, grid: &mut Grid, frame: Frame) {
    let b = &kit.blocks;
    let face = kit.layout.entrance.face.max(1);
    terrace(kit, grid, frame, 1, 12, 13);
    // The ground between the threshold and the face, cut away level, as wide as the outer frame.
    for ahead in -(face - 1)..=0 {
        for right in -15i32..=15 {
            let at = frame.at(right, 0, ahead);
            grid.keep(at, if right.abs() == 15 { b.trim } else { b.floor });
            for up in 1..=30 {
                grid.set(frame.at(right, up, ahead), AIR, Tag::Air);
            }
        }
    }
    for band in 0..4 {
        let half = 14 - band * 3;
        let tall = 26 - band * 5;
        let outer = -(face - 1) - band * 2;
        for ahead in [outer, outer - 1] {
            for right in -half - 1..=half + 1 {
                for up in 0..=tall + 1 {
                    let at = frame.at(right, up, ahead);
                    let inside = right.abs() <= half && up >= 1 && up <= tall;
                    if inside {
                        grid.set(at, AIR, Tag::Air);
                    } else if up > 0 {
                        // Banded every few blocks, as the stepped frames are carved.
                        grid.build(at, if (up % 5 == 0) || right.abs() == half + 1 { b.trim } else { b.stone });
                    }
                }
                grid.keep(frame.at(right, 0, ahead), b.floor);
            }
        }
    }
    // The innermost frame's pillar.
    let inner = -(face - 1) - 6;
    for up in 1..=11 {
        for ahead in [inner, inner - 1] {
            grid.build(frame.at(0, up, ahead), if up == 1 || up == 11 { b.trim } else { b.stone });
        }
    }
    for side in [-1, 1] {
        let foot = frame.at(side * 10, 0, -(face - 1) - 1);
        pieces::statue(kit, grid, Frame { foot, facing: frame.facing }, 20, true);
        pieces::caged_brazier(kit, grid, frame.at(side * 6, 1, 10));
    }
}

/// How wide a sinkhole's shaft is, from its middle, in blocks.
pub(crate) const SINKHOLE_RADIUS: f32 = 9.0;

/// Which way from the middle of a sinkhole's shaft the head of the stair
/// winding down its wall lies, as an angle about +y from +x: where one
/// standing at its rim starts down.
pub(crate) fn sinkhole_stair_head(layout: &DungeonLayout) -> f32 {
    let middle = SINKHOLE_RADIUS - 2.0;
    let top_s = (layout.entrance.at[1] + 1) * 2;
    let bottom_s = (layout.shaft_floor + 1) * 2;
    let into = layout.entrance.into;
    let end_angle = (into[1] as f32).atan2(into[0] as f32);
    end_angle - (top_s - bottom_s) as f32 / middle
}

/// A stair round the wall of a round shaft open to the sky, down to the
/// first floor's landing: a kerb round its rim, broken columns standing
/// round it, braziers.
fn sinkhole(kit: &Kit, grid: &mut Grid, frame: Frame) {
    let b = &kit.blocks;
    let centre = frame.foot;
    let floor = kit.layout.shaft_floor;
    let radius = SINKHOLE_RADIUS;
    let middle = radius - 2.0;
    let top_s = (centre[1] + 1) * 2;
    let bottom_s = (floor + 1) * 2;
    // Ends facing the way into the land, where the grand stair goes on.
    let start = sinkhole_stair_head(kit.layout);
    let (lo, hi) = grid.columns();
    let seed = centre[0] as u32 ^ (centre[2] as u32).rotate_left(7);
    for z in (centre[2] - 14).max(lo[1])..=(centre[2] + 14).min(hi[1]) {
        for x in (centre[0] - 14).max(lo[0])..=(centre[0] + 14).min(hi[0]) {
            let (dx, dz) = ((x - centre[0]) as f32 + 0.5, (z - centre[2]) as f32 + 0.5);
            let d = (dx * dx + dz * dz).sqrt();
            let ground = top(kit, x, z);
            let wobble = noise::smooth([x as f32, 0.0, z as f32], 5.0, [1.0, 1.0, 1.0], seed) * 1.2;
            if d < radius + wobble {
                for y in floor + 1..=ground.max(centre[1]) + 1 {
                    grid.set([x, y, z], AIR, Tag::Air);
                }
                grid.keep([x, floor, z], b.floor);
                if d < radius - 4.5 {
                    continue;
                }
                // The stair: every turn of it over this column.
                let angle = (dz.atan2(dx) - start).rem_euclid(std::f32::consts::TAU);
                let mut turn = 0;
                loop {
                    let along = (angle + turn as f32 * std::f32::consts::TAU) * middle;
                    let surface = top_s - along.floor() as i32;
                    if surface < bottom_s {
                        break;
                    }
                    let y = surface.div_euclid(2);
                    if surface % 2 == 1 {
                        grid.keep([x, y, z], if b.slab != AIR { b.slab } else { b.floor });
                    }
                    grid.keep([x, y - 1, z], b.floor);
                    grid.build([x, y - 2, z], b.stone);
                    // A balustrade along its inner edge, over the drop.
                    if d < radius - 3.8 {
                        let floor_y = if surface % 2 == 0 { y - 1 } else { y };
                        pieces::balustrade(kit, grid, [x, floor_y, z]);
                    }
                    turn += 1;
                }
            } else if d < radius + 2.0 {
                grid.keep([x, ground, z], b.trim);
            }
        }
    }
    for i in 0..8 {
        let angle = i as f32 / 8.0 * std::f32::consts::TAU + 0.3;
        let x = centre[0] + (angle.cos() * (radius + 4.0)).round() as i32;
        let z = centre[2] + (angle.sin() * (radius + 4.0)).round() as i32;
        if x < lo[0] - 1 || x > hi[0] + 1 || z < lo[1] - 1 || z > hi[1] + 1 {
            continue;
        }
        let ground = top(kit, x, z);
        let height = 2 + (noise::hash(i, 0, 0, seed) % 7) as i32;
        pieces::broken_column(kit, grid, [x, ground, z], height, seed ^ i as u32);
    }
    for side in [-1, 1] {
        let at = frame.at(side * 13, 0, 0);
        if at[0] < lo[0] || at[0] > hi[0] || at[2] < lo[1] || at[2] > hi[1] {
            continue;
        }
        let ground = top(kit, at[0], at[2]);
        pieces::caged_brazier(kit, grid, [at[0], ground + 1, at[2]]);
    }
}
