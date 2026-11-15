//! The pieces a dungeon's built stone is made of, each drawn into a grid at
//! a place: a column, a pointed arch, a balustrade, hooded and armoured
//! statues, a monolith, an obelisk, a throne on its dais, a relief carved
//! into a wall, a beast's head over an archway.

use super::build::{Grid, Kit};
use crate::blocks::chunk::{AIR, BlockId};

/// A place to draw a piece: its foot, and the way it faces along x or z.
/// Its own right is a quarter turn from facing, and up is up.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub(crate) foot: [i32; 3],
    pub(crate) facing: [i32; 2],
}

impl Frame {
    /// The block `right` blocks to its right, `up` up and `ahead` ahead.
    pub(crate) fn at(&self, right: i32, up: i32, ahead: i32) -> [i32; 3] {
        let side = [-self.facing[1], self.facing[0]];
        [
            self.foot[0] + side[0] * right + self.facing[0] * ahead,
            self.foot[1] + up,
            self.foot[2] + side[1] * right + self.facing[1] * ahead,
        ]
    }
}

/// A column standing on the floor block at `foot`, `height` blocks up to
/// the top of its capital, `thick` blocks a side: a base of trim a block
/// wider each way, a shaft of stone, a capital of trim as wide as the base.
pub(crate) fn column(kit: &Kit, grid: &mut Grid, foot: [i32; 3], height: i32, thick: i32) {
    let b = &kit.blocks;
    let (from, to) = (-(thick - 1) / 2, thick / 2);
    for y in 1..=height {
        let wide = y == 1 || y >= height - 1;
        let (f, t) = if wide { (from - 1, to + 1) } else { (from, to) };
        for dz in f..=t {
            for dx in f..=t {
                let block = if wide { b.trim } else { b.stone };
                grid.build([foot[0] + dx, foot[1] + y, foot[2] + dz], block);
            }
        }
    }
}

/// A broken column on the ground at `foot`: a base of trim, a shaft two
/// blocks a side broken off at `height`, its top ragged, a fallen block
/// beside it.
pub(crate) fn broken_column(kit: &Kit, grid: &mut Grid, foot: [i32; 3], height: i32, seed: u32) {
    let b = &kit.blocks;
    for dz in -1..=2 {
        for dx in -1..=2 {
            grid.build([foot[0] + dx, foot[1] + 1, foot[2] + dz], b.trim);
        }
    }
    for dz in 0..=1 {
        for dx in 0..=1 {
            let ragged = (super::noise::hash(dx, dz, 0, seed) % 3) as i32;
            for up in 2..=(height - ragged).max(2) {
                grid.build([foot[0] + dx, foot[1] + up, foot[2] + dz], b.stone);
            }
        }
    }
    let fallen = [[3, 0], [0, 3], [-2, 1], [1, -2]][(seed % 4) as usize];
    grid.build([foot[0] + fallen[0], foot[1] + 1, foot[2] + fallen[1]], b.stone);
}

/// A pointed arch of `block` from block column `a` to block column `b`
/// (the faces it springs from), its feet at height `spring`, `ring` blocks
/// deep, `thick` blocks through; filled solid over its ring up to `fill`
/// where given, as an arcade's wall is. Its span runs straight from `a` to
/// `b` at any angle.
#[allow(clippy::too_many_arguments)]
pub(crate) fn arch(
    grid: &mut Grid,
    block: BlockId,
    a: [f32; 2],
    b: [f32; 2],
    spring: i32,
    ring: i32,
    thick: i32,
    fill: Option<i32>,
) {
    let span = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    if span < 1.0 {
        return;
    }
    let dir = [(b[0] - a[0]) / span, (b[1] - a[1]) / span];
    let across = [-dir[1], dir[0]];
    let h = span * 0.5;
    let radius = h * 1.6;
    let steps = (span * 2.0).ceil() as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let u = (t * span - h).abs();
        let curve = (radius * radius - (u - (h - radius)).powi(2)).max(0.0).sqrt();
        let from = spring + curve.floor() as i32;
        let to = fill.unwrap_or(spring + (curve.floor() as i32) + ring - 1).max(from);
        for k in 0..thick {
            let off = k as f32 - (thick - 1) as f32 * 0.5;
            let x = (a[0] + dir[0] * t * span + across[0] * off).floor() as i32;
            let z = (a[1] + dir[1] * t * span + across[1] * off).floor() as i32;
            for y in from..=to {
                grid.build([x, y, z], block);
            }
        }
    }
}

/// How high a pointed arch spanning `span` blocks rises over its feet.
pub(crate) fn arch_rise(span: f32) -> i32 {
    let h = span * 0.5;
    let radius = h * 1.6;
    (radius * radius - (radius - h) * (radius - h)).sqrt().ceil() as i32
}

/// A balustrade on the floor block at `at`: a post every other block, a
/// rail of slabs over them where the dungeon has slabs, else a low wall of
/// trim.
pub(crate) fn balustrade(kit: &Kit, grid: &mut Grid, at: [i32; 3]) {
    let b = &kit.blocks;
    let [x, y, z] = at;
    if !grid.open([x, y + 1, z]) {
        return;
    }
    if b.slab != AIR {
        if (x + z).rem_euclid(2) == 0 {
            grid.build([x, y + 1, z], b.trim);
        }
        if grid.open([x, y + 2, z]) {
            grid.build([x, y + 2, z], b.slab);
        }
    } else {
        grid.build([x, y + 1, z], b.trim);
    }
}

/// A statue `height` blocks tall on a plinth at `frame`, facing out of it:
/// a hooded figure in a robe, its face hollow, its hands on a sword planted
/// before it; or armoured, with broad shoulders and a helm.
pub(crate) fn statue(kit: &Kit, grid: &mut Grid, frame: Frame, height: i32, armoured: bool) {
    let b = &kit.blocks;
    let stone = b.statue;
    let blade = b.trim;
    let plinth = 2;
    for up in 1..=plinth {
        for ahead in -2..=2 {
            for right in -2..=2 {
                grid.build(frame.at(right, up, ahead), b.trim);
            }
        }
    }
    let body = (height - plinth).max(4);
    for up in 0..body {
        let t = up as f32 / body as f32;
        let y = plinth + 1 + up;
        // Its half width across and half depth front to back at this height.
        let (side, depth) = if t < 0.68 {
            (1.9 - 0.55 * t, 1.5 - 0.3 * t)
        } else if t < 0.78 {
            if armoured { (2.7, 1.5) } else { (2.0, 1.3) }
        } else {
            let head = (t - 0.78) / 0.22;
            let taper = if armoured { 1.25 - head * 0.35 } else { 1.45 - head * 1.15 };
            (taper.max(0.35), (taper + 0.1).max(0.4))
        };
        for ahead in -3..=3 {
            for right in -3..=3 {
                let (r, a) = (right as f32, ahead as f32);
                if (r / side).powi(2) + (a / depth).powi(2) > 1.0 {
                    continue;
                }
                // The face: a hollow under the hood's front.
                let face = !armoured && (0.80..0.93).contains(&t) && a >= depth * 0.45 && right == 0 && ahead > 0;
                if face {
                    grid.set(frame.at(right, y, ahead), AIR, super::build::Tag::Air);
                    continue;
                }
                grid.build(frame.at(right, y, ahead), stone);
            }
        }
        // A helm's crest.
        if armoured && t >= 0.95 {
            grid.build(frame.at(0, y + 1, 0), b.trim);
        }
    }
    // The sword before it, point down on the plinth, the hands on its hilt.
    let front = 2;
    let hilt = plinth + 1 + (body as f32 * 0.55) as i32;
    for y in plinth + 1..=hilt + 1 {
        grid.build(frame.at(0, y, front), blade);
    }
    for right in -1..=1 {
        grid.build(frame.at(right, hilt - 1, front), blade);
    }
    grid.build(frame.at(0, hilt + 1, front - 1), stone);
}

/// A monolith on a pillar rising from `foot`: a pillar `pillar` blocks
/// high, then a great carved block five wide and seven high, banded with
/// trim, a recessed panel on each face.
pub(crate) fn monolith(kit: &Kit, grid: &mut Grid, foot: [i32; 3], pillar: i32) {
    let b = &kit.blocks;
    for y in 1..=pillar {
        for dz in -1..=1 {
            for dx in -1..=1 {
                grid.build([foot[0] + dx, foot[1] + y, foot[2] + dz], b.stone);
            }
        }
    }
    let base = foot[1] + pillar;
    for up in 1..=7 {
        for dz in -2..=2i32 {
            for dx in -2..=2i32 {
                let band = up == 1 || up == 7 || up == 4;
                let rim = dx.abs() == 2 || dz.abs() == 2;
                let panel = rim && !band && dx.abs() + dz.abs() < 4 && (2..=6).contains(&up);
                let block = if band {
                    b.trim
                } else if panel {
                    b.cracked
                } else {
                    b.statue
                };
                grid.build([foot[0] + dx, base + up, foot[2] + dz], block);
            }
        }
    }
    grid.build([foot[0], base + 8, foot[2]], b.trim);
}

/// An obelisk on a stepped plinth at `foot`: a shaft tapering from five
/// blocks a side to one over `height`, banded with trim, a point on top.
pub(crate) fn obelisk(kit: &Kit, grid: &mut Grid, foot: [i32; 3], height: i32) {
    let b = &kit.blocks;
    for (step, half) in [(1, 5), (2, 4), (3, 3)] {
        for dz in -half..=half {
            for dx in -half..=half {
                grid.build([foot[0] + dx, foot[1] + step, foot[2] + dz], b.trim);
            }
        }
    }
    for up in 0..height {
        let t = up as f32 / height as f32;
        let half = if t < 0.3 {
            2
        } else if t < 0.8 {
            1
        } else {
            0
        };
        for dz in -half..=half {
            for dx in -half..=half {
                let band = up % 7 == 6;
                grid.build([foot[0] + dx, foot[1] + 4 + up, foot[2] + dz], if band { b.trim } else { b.statue });
            }
        }
    }
    grid.build([foot[0], foot[1] + 4 + height, foot[2]], b.trim);
}

/// A throne on a dais of three steps at `frame`, facing out of it.
pub(crate) fn throne(kit: &Kit, grid: &mut Grid, frame: Frame) {
    let b = &kit.blocks;
    for (step, half) in [(1, 5), (2, 4), (3, 3)] {
        for ahead in -half..=half {
            for right in -half..=half {
                grid.build(frame.at(right, step, ahead), b.trim);
            }
        }
    }
    let seat = 4;
    for right in -1..=1 {
        grid.build(frame.at(right, seat, 0), b.statue);
        for up in seat..seat + 6 {
            grid.build(frame.at(right, up, -1), b.statue);
        }
    }
    grid.build(frame.at(0, seat + 6, -1), b.trim);
    for right in [-2, 2] {
        grid.build(frame.at(right, seat, 0), b.statue);
        grid.build(frame.at(right, seat + 1, 0), b.trim);
        grid.build(frame.at(right, seat, -1), b.statue);
    }
}

/// A relief on a wall, at `frame` (its foot the middle of its bottom, facing
/// out of the wall), `width` wide and `height` high: a border of trim round
/// a panel of stone, a robed figure standing in it under a disc, standing a
/// block proud of the panel.
pub(crate) fn relief(kit: &Kit, grid: &mut Grid, frame: Frame, width: i32, height: i32) {
    let b = &kit.blocks;
    let half = width / 2;
    for up in 0..height {
        for right in -half..=half {
            let border = up == 0 || up == height - 1 || right.abs() == half;
            grid.build(frame.at(right, up, 0), if border { b.trim } else { b.stone });
            // The figure: a robe widening downwards, a head, a disc over it, and arms raised to the disc.
            let t = up as f32 / height as f32;
            let r = right.abs() as f32;
            let w = half as f32;
            let robe = t > 0.08 && t < 0.62 && r <= w * (0.28 - t * 0.25);
            let head = (0.62..0.72).contains(&t) && r <= w * 0.1;
            let disc = ((t - 0.86) * height as f32).powi(2) + (r * 1.0).powi(2) <= (w * 0.22).powi(2);
            let arms = (0.55..0.85).contains(&t) && (r - w * (0.12 + (t - 0.55) * 0.9)).abs() < 0.8;
            if !border && (robe || head || disc || arms) {
                grid.build(frame.at(right, up, 1), if disc { b.trim } else { b.statue });
            }
        }
    }
}

/// A beast's head carved over an archway, at `frame` (its foot the middle
/// of its jaw, facing out): a brow, eyes, a snout and horns curling up.
pub(crate) fn beast_head(kit: &Kit, grid: &mut Grid, frame: Frame) {
    let b = &kit.blocks;
    let rows: [&str; 6] = ["h.....h", "h.....h", ".hssssh.", "sseseess", "sssssss", ".snnns."];
    for (row, line) in rows.iter().enumerate() {
        let up = (rows.len() - 1 - row) as i32;
        for (column, mark) in line.chars().enumerate() {
            let right = column as i32 - 3;
            match mark {
                's' => grid.build(frame.at(right, up, 0), b.statue),
                'h' => grid.build(frame.at(right, up, 0), b.trim),
                'n' => {
                    grid.build(frame.at(right, up, 0), b.statue);
                    grid.build(frame.at(right, up, 1), b.statue);
                }
                'e' => {
                    let eye = if b.light != AIR { b.light } else { b.trim };
                    grid.build(frame.at(right, up, 0), eye);
                }
                _ => {}
            }
        }
    }
}

/// A brazier in an iron cage at `at` (the first block of air over a
/// floor): a stand, the fire, bars round it.
pub(crate) fn caged_brazier(kit: &Kit, grid: &mut Grid, at: [i32; 3]) {
    let b = &kit.blocks;
    let iron = if b.metal != AIR { b.metal } else { b.trim };
    grid.keep(at, b.trim);
    grid.keep([at[0], at[1] + 1, at[2]], iron);
    if b.light != AIR {
        grid.keep([at[0], at[1] + 2, at[2]], b.light);
    }
    grid.keep([at[0], at[1] + 3, at[2]], iron);
}
