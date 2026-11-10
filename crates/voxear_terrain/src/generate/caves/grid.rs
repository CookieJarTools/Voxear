//! The regions a Cave Network's rooms lie in: squares along flat ground,
//! or the squares of a grid on each of the six sides of a cube round a
//! planet, pressed onto its sphere.
//!
//! A region is three whole numbers. On flat ground they are how many
//! regions along x and z it lies, with nothing between. Round a planet
//! they are where its middle lies on a cube `2 n` across, counted in half
//! regions from the cube's middle: the side it is on has `n` or `-n`, and
//! the two others are odd or even as `n` is not. Stepping off a side's
//! edge is stepping onto the side next to it, in whole numbers, so two
//! regions are neighbours of each other or neither is.
//!
//! Only sums, products, divisions and square roots: the same regions on
//! every machine.

use super::rooms::{greater, lesser};

pub(crate) type Region = [i32; 3];

pub(crate) type V3 = [f32; 3];

pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn times(a: V3, by: f32) -> V3 {
    [a[0] * by, a[1] * by, a[2] * by]
}

pub(crate) fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `a` made one long: `or` where it has no length.
pub(crate) fn unit(a: V3, or: V3) -> V3 {
    let length = dot(a, a).sqrt();
    if length > 1.0e-6 { [a[0] / length, a[1] / length, a[2] / length] } else { or }
}

/// How much of a quarter of the way round a planet a region is narrower
/// than at the middle of a cube's side, at its corners: regions are counted
/// so that the narrowest is still as wide as asked.
const NARROWEST: f32 = 0.7;

/// How a side's grid is drawn in at its edges, so regions near them are
/// nearly as large on the sphere as those in its middle.
const DRAWN: f32 = 0.28;

/// From a place on a cube's side, -1 to 1 across it, to where on the
/// side's grid it is, -1 to 1.
fn drawn(a: f32) -> f32 {
    a * (1.0 + DRAWN) / (1.0 + DRAWN * a * a)
}

/// From where on a side's grid, -1 to 1, to the place on the cube's side.
fn undrawn(s: f32) -> f32 {
    let wide = 1.0 + DRAWN;
    let under = greater(wide * wide - 4.0 * DRAWN * s * s, 0.0);
    2.0 * s / (wide + under.sqrt())
}

/// Where a region's room stands: the foot of its column, where the
/// ground's height is asked (on flat ground at no height, round a planet on
/// its sea), and its own x, up and z there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stand {
    pub foot: V3,
    pub x: V3,
    pub up: V3,
    pub z: V3,
}

/// The grid of regions: on flat ground, or round a planet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Grid {
    Flat {
        spacing: f32,
    },
    Round {
        /// How many regions across each side of the cube.
        n: i32,
        radius: f32,
        sea_level: f32,
    },
}

impl Grid {
    /// The grid round a planet of `radius` metres with regions no narrower
    /// than `spacing`.
    pub fn round(spacing: f32, radius: f32, sea_level: f32) -> Self {
        let quarter = std::f32::consts::FRAC_PI_2 * greater(radius, 1.0);
        let n = (quarter * NARROWEST / greater(spacing, 1.0)).floor();
        Grid::Round { n: if n >= 1.0 { lesser(n, 4096.0) as i32 } else { 1 }, radius, sea_level }
    }

    /// The region `p` is in.
    pub fn region(&self, p: V3) -> Region {
        match *self {
            Grid::Flat { spacing } => [(p[0] / spacing).floor() as i32, 0, (p[2] / spacing).floor() as i32],
            Grid::Round { n, .. } => {
                let (side, sign) = side_of(p);
                let over = p[side].abs();
                let mut region = [0; 3];
                region[side] = sign * n;
                for axis in [(side + 1) % 3, (side + 2) % 3] {
                    let s = if over > 0.0 { drawn(p[axis] / over) } else { 0.0 };
                    let cell = ((s + 1.0) * 0.5 * n as f32).floor() as i32;
                    region[axis] = 2 * cell.clamp(0, n - 1) + 1 - n;
                }
                region
            }
        }
    }

    /// The four regions beside `region`.
    pub fn beside(&self, region: Region) -> [Region; 4] {
        match *self {
            Grid::Flat { .. } => {
                let [i, _, j] = region;
                [[i + 1, 0, j], [i - 1, 0, j], [i, 0, j + 1], [i, 0, j - 1]]
            }
            Grid::Round { n, .. } => {
                let side = (0..3).find(|&axis| region[axis].abs() == n).unwrap_or(1);
                let step = |axis: usize, by: i32| {
                    let mut next = region;
                    next[axis] += 2 * by;
                    // Off the side's edge: onto the side that way, in its row along the edge.
                    if next[axis].abs() > n - 1 {
                        next[axis] = by * n;
                        next[side] = region[side].signum() * (n - 1);
                    }
                    next
                };
                let (a, b) = ((side + 1) % 3, (side + 2) % 3);
                [step(a, 1), step(a, -1), step(b, 1), step(b, -1)]
            }
        }
    }

    /// Where the room of `region` stands, moved `jitter` of half a region
    /// from its middle along each way across it, each -1 to 1.
    pub fn stand(&self, region: Region, jitter: [f32; 2]) -> Stand {
        match *self {
            Grid::Flat { spacing } => {
                let along = |cell: i32, jitter: f32| (cell as f32 + 0.5 + jitter * 0.5) * spacing;
                Stand {
                    foot: [along(region[0], jitter[0]), 0.0, along(region[2], jitter[1])],
                    x: [1.0, 0.0, 0.0],
                    up: [0.0, 1.0, 0.0],
                    z: [0.0, 0.0, 1.0],
                }
            }
            Grid::Round { n, radius, .. } => {
                let side = (0..3).find(|&axis| region[axis].abs() == n).unwrap_or(1);
                let (a, b) = ((side + 1) % 3, (side + 2) % 3);
                let mut out = [0.0; 3];
                out[side] = region[side].signum() as f32;
                out[a] = undrawn((region[a] as f32 + jitter[0]) / n as f32);
                out[b] = undrawn((region[b] as f32 + jitter[1]) / n as f32);
                let mut top = [0.0; 3];
                top[side] = out[side];
                let up = unit(out, top);
                let mut across = [0.0; 3];
                across[a] = 1.0;
                let x = unit(sub(across, times(up, dot(across, up))), across);
                Stand { foot: times(up, radius), x, up, z: cross(x, up) }
            }
        }
    }

    /// The place `rise` metres up from where the ground `ground` metres
    /// high lies over `stand`'s foot.
    pub fn lifted(&self, stand: &Stand, ground: f32, rise: f32) -> V3 {
        match *self {
            Grid::Flat { .. } => [stand.foot[0], ground + rise, stand.foot[2]],
            Grid::Round { radius, sea_level, .. } => times(stand.up, radius + ground - sea_level + rise),
        }
    }

    /// The foot of the column `p` is in, and the way up there.
    pub fn foot(&self, p: V3) -> (V3, V3) {
        match *self {
            Grid::Flat { .. } => ([p[0], 0.0, p[2]], [0.0, 1.0, 0.0]),
            Grid::Round { radius, .. } => {
                let up = unit(p, [0.0, 1.0, 0.0]);
                (times(up, radius), up)
            }
        }
    }
}

/// The side of the cube round a planet's middle the way out to `p` leaves
/// by: its axis, and 1 or -1 for which end.
fn side_of(p: V3) -> (usize, i32) {
    let [x, y, z] = [p[0].abs(), p[1].abs(), p[2].abs()];
    let side = if x >= y && x >= z {
        0
    } else if y >= z {
        1
    } else {
        2
    };
    (side, if p[side] < 0.0 { -1 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_regions_are_squares_and_a_room_stands_upright_in_its_own() {
        let grid = Grid::Flat { spacing: 40.0 };
        assert_eq!(grid.region([10.0, 99.0, -1.0]), [0, 0, -1]);
        assert_eq!(grid.beside([2, 0, 3]), [[3, 0, 3], [1, 0, 3], [2, 0, 4], [2, 0, 2]]);
        let stand = grid.stand([2, 0, -1], [0.5, -1.0]);
        assert_eq!(stand.foot, [110.0, 0.0, -40.0]);
        assert_eq!(grid.lifted(&stand, 30.0, -12.0), [110.0, 18.0, -40.0]);
        assert_eq!(grid.region(stand.foot), [2, 0, -1]);
    }

    #[test]
    fn round_a_planet_every_region_has_four_neighbours_that_have_it_for_one() {
        for n in [1, 2, 3, 7] {
            let grid = Grid::Round { n, radius: 500.0, sea_level: 0.0 };
            let mut regions = Vec::new();
            for side in 0..3 {
                for sign in [-1, 1] {
                    for a in 0..n {
                        for b in 0..n {
                            let mut region = [0; 3];
                            region[side] = sign * n;
                            region[(side + 1) % 3] = 2 * a + 1 - n;
                            region[(side + 2) % 3] = 2 * b + 1 - n;
                            regions.push(region);
                        }
                    }
                }
            }
            for &region in &regions {
                let beside = grid.beside(region);
                for (at, next) in beside.iter().enumerate() {
                    assert!(regions.contains(next), "{region:?} beside {next:?}, of {n}");
                    assert!(grid.beside(*next).contains(&region), "{next:?} has not {region:?} beside it, of {n}");
                    assert!(*next != region && !beside[..at].contains(next), "{region:?}: {beside:?}");
                }
                // Its room stands in it, upright, wherever in it it is moved to.
                for jitter in [[0.0, 0.0], [0.9, -0.9], [-0.9, 0.9]] {
                    let stand = grid.stand(region, jitter);
                    assert_eq!(grid.region(stand.foot), region, "{jitter:?} of {n}");
                    let length = dot(stand.foot, stand.foot).sqrt();
                    assert!((length - 500.0).abs() < 0.01);
                    for (a, b) in [(stand.x, stand.up), (stand.up, stand.z), (stand.x, stand.z)] {
                        assert!(dot(a, b).abs() < 1.0e-5);
                    }
                    assert!((dot(cross(stand.z, stand.x), stand.up) - 1.0).abs() < 1.0e-4, "x, up and z as x, y, z");
                }
            }
        }
    }

    #[test]
    fn regions_round_a_planet_are_no_narrower_than_asked_and_much_of_a_size() {
        let (spacing, radius) = (60.0, 800.0);
        let grid = Grid::round(spacing, radius, 0.0);
        let Grid::Round { n, .. } = grid else { panic!() };
        assert!(n >= 10, "{n} across a side");
        let (mut least, mut most) = (f32::MAX, 0.0f32);
        for a in 0..n {
            for b in 0..n {
                let region = [2 * a + 1 - n, n, 2 * b + 1 - n];
                let here = grid.stand(region, [0.0, 0.0]).foot;
                for next in grid.beside(region) {
                    let there = grid.stand(next, [0.0, 0.0]).foot;
                    let apart = dot(sub(here, there), sub(here, there)).sqrt();
                    (least, most) = (least.min(apart), most.max(apart));
                }
            }
        }
        assert!(least >= spacing && most < least * 1.7, "from {least} to {most} m apart");
    }
}
