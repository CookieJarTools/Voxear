//! Dicing displaced surfaces on the GPU (`shaders/dice.slang`): a material
//! with a height map and a displacement moves its surface out along its
//! normals, so its triangles are cut finely enough near the camera to show
//! it, and coarsely far off. Each frame the cull lists the displaced
//! instances in view; a compute pass splits each of their triangles into
//! pieces, another sends out each piece in view as a draw of its own grid
//! of triangles, and the depth and opaque passes draw them.
//!
//! Each edge is cut as many times as its own length on screen asks, worked
//! out from its two ends, which the triangles either side of it share, and
//! a vertex on an edge snaps to one of the edge's even steps: so both sides
//! make the same points, and no crack opens between them.

/// The most displaced instances, split triangles and pieces a frame.
pub const MAX_DICED_INSTANCES: u64 = 4096;
pub const MAX_SPLITS: u64 = 65535;
pub const MAX_PATCHES: u64 = 262_144;

/// The counts dicing keeps, as `Buffers::diceCounts`: the split pass's
/// dispatch (how many instances), the piece pass's (how many split
/// triangles), how many pieces were drawn and how many did not fit, as
/// they start each frame.
pub const CLEARED_COUNTS: [u32; 16] = [0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
pub const COUNTS_SIZE: u64 = 16 * 4;
/// Where the split pass's dispatch, the piece pass's and the pieces' count
/// are in the counts, in bytes.
pub const SPLIT_DISPATCH: u64 = 0;
pub const PATCH_DISPATCH: u64 = 4 * 4;
pub const PATCH_COUNT: u64 = 8 * 4;
/// The bytes of one piece's record, and of its draw.
pub const PATCH_SIZE: u64 = 16;

/// A piece of a triangle split into `split` steps a side, as `dicePiece`
/// in `shaders/dice.slang` finds it: its three corners on the split grid,
/// as steps towards the triangle's second and third vertices.
#[cfg(test)]
pub fn piece(split: u32, piece: u32) -> [[i32; 2]; 3] {
    let mut q = (piece as f32).sqrt().floor() as u32;
    q = if (q + 1) * (q + 1) <= piece {
        q + 1
    } else if q * q > piece {
        q - 1
    } else {
        q
    };
    let k = piece - q * q;
    let j = (split - 1 - q) as i32;
    let i = (k / 2) as i32;
    if k & 1 == 0 { [[i, j], [i + 1, j], [i, j + 1]] } else { [[i + 1, j], [i + 1, j + 1], [i, j + 1]] }
}

/// Where a step `t` along an edge cut `cut` times lands, from the edge's
/// lower end: snapped to the nearest of its even steps, as `diceSnap` does.
#[cfg(test)]
pub fn snap(t: f32, cut: u32) -> f32 {
    (t * cut as f32).round() / cut as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn a_split_triangle_s_pieces_cover_it_once_each_way_up() {
        for split in 1..=8 {
            let mut corners = BTreeSet::new();
            let mut area = 0;
            for p in 0..split * split {
                let [a, b, c] = piece(split, p);
                // Turned the same way as the triangle: twice the signed area, in grid steps, is 1.
                let turn = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
                assert_eq!(turn, 1, "piece {p} of {split}");
                area += turn;
                for corner in [a, b, c] {
                    assert!(corner[0] >= 0 && corner[1] >= 0 && corner[0] + corner[1] <= split as i32);
                    corners.insert(corner);
                }
            }
            assert_eq!(area, (split * split) as i32, "covering the triangle");
            assert_eq!(corners.len() as u32, (split + 1) * (split + 2) / 2, "every grid point a corner");
        }
    }

    /// The points along an edge's share from `from` to `to` that a side
    /// cutting it `cut` times makes, snapped to `whole` steps of the edge.
    fn points(from: f32, to: f32, cut: u32, whole: u32) -> BTreeSet<u32> {
        (0..=cut)
            .map(|i| from + (to - from) * i as f32 / cut as f32)
            .map(|t| (snap(t, whole) * whole as f32).round() as u32)
            .collect()
    }

    #[test]
    fn both_sides_of_an_edge_make_the_same_points_however_each_is_split() {
        for whole in 1..=200u32 {
            // Each side splits the edge into as many pieces as its own triangle asks, each piece cut at least its
            // share of the whole.
            let side = |split: u32| -> BTreeSet<u32> {
                let cut = whole.div_ceil(split).min(64);
                (0..split)
                    .flat_map(|k| points(k as f32 / split as f32, (k + 1) as f32 / split as f32, cut, whole))
                    .collect()
            };
            let every: BTreeSet<u32> = (0..=whole).collect();
            for split in 1..=whole.div_ceil(1).min(32) {
                if whole.div_ceil(split) > 64 {
                    continue;
                }
                assert_eq!(side(split), every, "an edge of {whole} steps split {split} ways");
            }
        }
    }
}
