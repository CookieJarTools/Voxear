//! The Cells node: the ground split into cells, each the ground nearest
//! one of the points scattered over it. A point lies in every square of a
//! grid Size metres wide, moved about within its square by Jitter, so the
//! cells are even with none and ragged with all of it.
//!
//! It makes three things, each from 0 to 1:
//!
//! - **Distance**: how far the nearest point is, 0 at the point and 1 a
//!   cell's width away. Through a curve it is a pit or a mound round every
//!   point: craters, dunes, hummocks.
//! - **Edges**: how far the nearest border between two cells is, 0 on the
//!   border and 1 half a cell's width from it. The borders are cracks, or
//!   the walls between pans of dried mud.
//! - **Value**: a number of its cell's own, the same all over the cell.
//!   Each cell is a step of its own height: mesas, fields, paving.
//!
//! Round a planet the points lie in the cubes of a grid through it, so the
//! cells have no seams.

use serde::{Deserialize, Serialize};

use super::{Kind, Port, Value, ValueSpec, node_seed};
use crate::generate::{
    compile::{Compiler, Kernel, Key, Places, V},
    graph::NodeId,
};

/// Cells round points scattered a `size` metres apart, each moved within
/// its square of the grid by `jitter`, 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cells {
    pub size: f32,
    pub jitter: f32,
}

impl Default for Cells {
    fn default() -> Self {
        Self { size: 100.0, jitter: 1.0 }
    }
}

impl Cells {
    /// Its outputs, among its outputs.
    pub const DISTANCE: usize = 0;
    pub const EDGES: usize = 1;
    pub const VALUE: usize = 2;
}

const VALUES: &[ValueSpec] =
    &[ValueSpec::number("size", "Size", 0.01, f32::MAX, "m"), ValueSpec::number("jitter", "Jitter", 0.0, 1.0, "")];

const OUTPUTS: &[Port] =
    &[Port::field("distance", "Distance"), Port::field("edges", "Edges"), Port::field("value", "Value")];

impl Kind for Cells {
    const TITLE: &'static str = "Cells";
    const FOLDER: &'static str = "Sources";

    fn outputs(&self) -> &'static [Port] {
        OUTPUTS
    }

    fn values(&self) -> &'static [ValueSpec] {
        VALUES
    }

    fn value(&self, at: usize) -> Option<Value> {
        Some(match at {
            0 => Value::Number(self.size),
            1 => Value::Number(self.jitter),
            _ => return None,
        })
    }

    fn set(&mut self, at: usize, value: Value) {
        match (at, value) {
            (0, Value::Number(number)) => self.size = number,
            (1, Value::Number(number)) => self.jitter = number,
            _ => {}
        }
    }

    fn compile(&self, cx: &mut Compiler, node: NodeId, port: usize, at: Places) -> V {
        let scatter = Scatter {
            what: port.min(Cells::VALUE),
            size: (self.size * cx.world.scale.max(0.01)).max(0.01),
            jitter: self.jitter.clamp(0.0, 1.0),
            seed: node_seed(cx.world, node),
            round: matches!(at, Places::Round { .. }),
        };
        match at {
            Places::Flat { x, z } => cx.run(scatter, &[x, z]),
            Places::Round { x, y, z } => cx.run(scatter, &[x, y, z]),
        }
    }
}

/// One of a Cells node's outputs, from rows of x and z, or of x, y and z
/// round a planet.
#[derive(Debug)]
struct Scatter {
    what: usize,
    size: f32,
    jitter: f32,
    seed: u32,
    round: bool,
}

impl Kernel for Scatter {
    fn key(&self, key: &mut Key) {
        key.name("cells").whole(self.what as u32).number(self.size).number(self.jitter).whole(self.seed);
        key.whole(u32::from(self.round));
    }

    fn run(&self, inputs: &[&[f32]], out: &mut [f32]) {
        for (at, out) in out.iter_mut().enumerate() {
            *out = match self.round {
                false => self.read([inputs[0][at], inputs[1][at]]),
                true => self.read([inputs[0][at], inputs[1][at], inputs[2][at]]),
            };
        }
    }
}

/// The most points looked at round one place: those of the cubes two away
/// each way, in three dimensions.
const MOST: usize = 125;

/// A number for a square or cube of the grid, well mixed.
fn mixed<const N: usize>(cell: [i32; N], seed: u32) -> u32 {
    const BY: [u32; 3] = [0x8DA6_B343, 0xD816_3841, 0xCB1A_B31F];
    let mut h = seed;
    for (axis, &at) in cell.iter().enumerate() {
        h ^= (at as u32).wrapping_mul(BY[axis]);
        h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    }
    again(h)
}

/// A number mixed once more: another, as well mixed, from the same cell.
fn again(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// The points found round a place, each by where it lies from the place, in
/// cells along each axis, and its cell's number.
struct Found<const N: usize> {
    from: [[f32; N]; MOST],
    cell: [u32; MOST],
    count: usize,
}

impl<const N: usize> Found<N> {
    /// The nearest of them, and how far it is, squared.
    fn nearest(&self) -> (usize, f32) {
        let (mut nearest, mut least) = (0, f32::MAX);
        for at in 0..self.count {
            let far = self.from[at].iter().map(|v| v * v).sum::<f32>();
            if far < least {
                (nearest, least) = (at, far);
            }
        }
        (nearest, least)
    }

    /// How far the place is from the nearest border of the cell of point
    /// `nearest`: the least, over every other point, of how far it is from
    /// the line halfway between the two.
    fn border(&self, nearest: usize) -> f32 {
        let own = self.from[nearest];
        let mut least = f32::MAX;
        for at in (0..self.count).filter(|&at| at != nearest) {
            let other = self.from[at];
            let (mut along, mut length) = (0.0, 0.0);
            for axis in 0..N {
                let apart = other[axis] - own[axis];
                along += (own[axis] + other[axis]) * 0.5 * apart;
                length += apart * apart;
            }
            if length > 1e-12 {
                let far = along / length.sqrt();
                if far < least {
                    least = far;
                }
            }
        }
        least
    }
}

impl Scatter {
    /// Its output at `place`, in metres.
    fn read<const N: usize>(&self, place: [f32; N]) -> f32 {
        // The square or cube of the grid the place is in, where in it the place lies, and how near its nearest side.
        let (mut cell, mut inside, mut margin) = ([0i32; N], [0.0f32; N], 1.0f32);
        for axis in 0..N {
            let scaled = place[axis] / self.size;
            let floor = scaled.floor();
            (cell[axis], inside[axis]) = (floor as i32, scaled - floor);
            let side = if inside[axis] < 0.5 { inside[axis] } else { 1.0 - inside[axis] };
            if side < margin {
                margin = side;
            }
        }
        // The points of the cells round it; and of those a cell further out only where one of them might be nearer
        // than what was found, which is seldom. A point that far out is at least `1 + margin` cells away.
        let mut found = Found { from: [[0.0; N]; MOST], cell: [0; MOST], count: 0 };
        self.gather(&mut found, cell, inside, 1, 0);
        let (mut nearest, mut least) = found.nearest();
        let reach = 1.0 + margin;
        if self.what != Cells::EDGES {
            if least > reach * reach {
                self.gather(&mut found, cell, inside, 2, 1);
                (nearest, least) = found.nearest();
            }
            return match self.what {
                Cells::DISTANCE => {
                    let far = least.sqrt();
                    if far < 1.0 { far } else { 1.0 }
                }
                _ => (again(found.cell[nearest] ^ 0x68E3_1DA4) >> 8) as f32 / 16_777_216.0,
            };
        }
        // A border with a point further out is at least half of how much further away that point is.
        let mut border = found.border(nearest);
        if least > reach * reach || border > (reach - least.sqrt()) * 0.5 {
            self.gather(&mut found, cell, inside, 2, 1);
            (nearest, _) = found.nearest();
            border = found.border(nearest);
        }
        let edge = border * 2.0;
        if edge < 0.0 {
            0.0
        } else if edge < 1.0 {
            edge
        } else {
            1.0
        }
    }

    /// Adds to `found` the points of the cells up to `reach` from `cell`
    /// each way, but for those within `within` of it, which it has.
    fn gather<const N: usize>(&self, found: &mut Found<N>, cell: [i32; N], inside: [f32; N], reach: i32, within: i32) {
        let side = (2 * reach + 1) as usize;
        let cells = if N == 2 { side * side } else { side * side * side };
        for index in 0..cells {
            let (mut off, mut rest, mut furthest) = ([0i32; N], index, 0);
            for step in &mut off {
                *step = (rest % side) as i32 - reach;
                rest /= side;
                furthest = furthest.max(step.abs());
            }
            if furthest <= within && within > 0 {
                continue;
            }
            let mut at = cell;
            for axis in 0..N {
                at[axis] = cell[axis].wrapping_add(off[axis]);
            }
            let number = mixed(at, self.seed);
            // Sixteen bits of the cell's number move its point along each axis: the third axis from it mixed again.
            let bits = [number & 0xFFFF, number >> 16, again(number) & 0xFFFF];
            let mut from = [0.0f32; N];
            for axis in 0..N {
                let moved = (bits[axis] as f32 / 65536.0 - 0.5) * self.jitter + 0.5;
                from[axis] = off[axis] as f32 + moved - inside[axis];
            }
            found.from[found.count] = from;
            found.cell[found.count] = number;
            found.count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{NodeKind, nodes::testing};

    #[test]
    fn cells_stay_from_nothing_to_one_and_fill_it() {
        for port in [Cells::DISTANCE, Cells::EDGES, Cells::VALUE] {
            let world = testing::world_of(NodeKind::Cells(Cells { size: 60.0, jitter: 1.0 }), port, &[]);
            let (low, high) = testing::checked(&world);
            assert!(low >= 0.0 && high <= 1.0, "output {port}: {low} to {high}");
            assert!(low < 0.05 && high > 0.7, "output {port} fills its range: {low} to {high}");
            let (low, high) = testing::round_range(&world);
            assert!(low >= 0.0 && high <= 1.0 && low < 0.1 && high > 0.6, "output {port} round: {low} to {high}");
        }
    }

    #[test]
    fn a_cell_has_one_value_a_point_at_its_middle_and_borders_at_its_edges() {
        let cells =
            |jitter: f32, port: usize| testing::world_of(NodeKind::Cells(Cells { size: 50.0, jitter }), port, &[]);
        // With no jitter the points lie on the middle of each square: nothing far at one, and a border between two.
        let (distance, edges, value) =
            (cells(0.0, Cells::DISTANCE), cells(0.0, Cells::EDGES), cells(0.0, Cells::VALUE));
        assert!(distance.height_at(25.0, 75.0).abs() < 1e-5);
        assert!((distance.height_at(40.0, 75.0) - 0.3).abs() < 1e-5, "15 m of 50 from its point");
        assert!(edges.height_at(50.0, 20.0).abs() < 1e-5 && (edges.height_at(25.0, 25.0) - 1.0).abs() < 1e-5);
        assert!((edges.height_at(40.0, 25.0) - 0.4).abs() < 1e-5, "10 m from a border, of 25");
        // One value all over a cell, another in the next.
        assert_eq!(value.height_at(3.0, 3.0), value.height_at(47.0, 44.0));
        assert_ne!(value.height_at(3.0, 3.0), value.height_at(53.0, 3.0));
        // With all of it the borders are found from points two cells off as well, so they have no breaks: a short
        // step never changes how far a border is by more than its length.
        // A step of 0.54 m is 0.0108 of a cell, so how far a border is, twice over, changes by 0.0216 at the most.
        let ragged = cells(1.0, Cells::EDGES);
        let mut last = ragged.height_at(0.0, 13.0);
        for i in 1..40_000 {
            let edge = ragged.height_at(i as f32 * 0.5, 13.0 + i as f32 * 0.2);
            assert!((edge - last).abs() <= 0.024, "a break at {i}: {last} to {edge}");
            last = edge;
        }
        // Another node's cells are its own, and so are another seed's.
        let world = cells(1.0, Cells::VALUE);
        assert_ne!(world.with_seed(8).height_at(10.0, 10.0), world.height_at(10.0, 10.0));
        // Twice the scale, the same cells twice as wide.
        assert_eq!(world.with_scale(2.0).height_at(84.0, -36.0), world.height_at(42.0, -18.0));
    }
}
