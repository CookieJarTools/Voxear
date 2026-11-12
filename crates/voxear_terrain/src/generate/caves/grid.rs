//! The regions a Cave Network's rooms lie in: squares along the ground.
//!
//! A region is three whole numbers: how many regions along x and z it
//! lies, with nothing between.
//!
//! Only sums, products, divisions and square roots: the same regions on
//! every machine.

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

/// Where a region's room stands: the foot of its column, where the
/// ground's height is asked, at no height, and its own x, up and z there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stand {
    pub foot: V3,
    pub x: V3,
    pub up: V3,
    pub z: V3,
}

/// The grid of regions, each `spacing` metres a side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Grid {
    pub spacing: f32,
}

impl Grid {
    /// The region `p` is in.
    pub fn region(&self, p: V3) -> Region {
        [(p[0] / self.spacing).floor() as i32, 0, (p[2] / self.spacing).floor() as i32]
    }

    /// The four regions beside `region`.
    pub fn beside(&self, region: Region) -> [Region; 4] {
        let [i, _, j] = region;
        [[i + 1, 0, j], [i - 1, 0, j], [i, 0, j + 1], [i, 0, j - 1]]
    }

    /// Where the room of `region` stands, moved `jitter` of half a region
    /// from its middle along each way across it, each -1 to 1.
    pub fn stand(&self, region: Region, jitter: [f32; 2]) -> Stand {
        let spacing = self.spacing;
        let along = |cell: i32, jitter: f32| (cell as f32 + 0.5 + jitter * 0.5) * spacing;
        Stand {
            foot: [along(region[0], jitter[0]), 0.0, along(region[2], jitter[1])],
            x: [1.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
        }
    }

    /// The place `rise` metres up from where the ground `ground` metres
    /// high lies over `stand`'s foot.
    pub fn lifted(&self, stand: &Stand, ground: f32, rise: f32) -> V3 {
        [stand.foot[0], ground + rise, stand.foot[2]]
    }

    /// The foot of the column `p` is in, and the way up there.
    pub fn foot(&self, p: V3) -> (V3, V3) {
        ([p[0], 0.0, p[2]], [0.0, 1.0, 0.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_regions_are_squares_and_a_room_stands_upright_in_its_own() {
        let grid = Grid { spacing: 40.0 };
        assert_eq!(grid.region([10.0, 99.0, -1.0]), [0, 0, -1]);
        assert_eq!(grid.beside([2, 0, 3]), [[3, 0, 3], [1, 0, 3], [2, 0, 4], [2, 0, 2]]);
        let stand = grid.stand([2, 0, -1], [0.5, -1.0]);
        assert_eq!(stand.foot, [110.0, 0.0, -40.0]);
        assert_eq!(grid.lifted(&stand, 30.0, -12.0), [110.0, 18.0, -40.0]);
        assert_eq!(grid.region(stand.foot), [2, 0, -1]);
    }
}
