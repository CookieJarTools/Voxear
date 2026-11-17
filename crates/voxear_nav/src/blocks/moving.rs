//! Moving along the places themselves, for what walks without a Character
//! Controller: each step into the next block column taken only where the
//! walker can step there, and where it cannot, the move that way given up,
//! so it slides along what is in its way.

use super::{BlockNav, PlaceRef, walking::Walker};

/// Where something standing on place `from`, its feet at `at` (in blocks
/// from the world's corner), ends after moving `by` blocks along the
/// ground, kept `margin` blocks from the side of any block column it
/// cannot step into: the place it is on, and its feet, on that place's
/// floor.
pub fn slide(nav: &BlockNav, from: PlaceRef, at: [f32; 3], by: [f32; 2], walker: Walker, margin: f32) -> (PlaceRef, [f32; 3]) {
    let margin = margin.clamp(0.0, 0.45);
    let mut place = from;
    let mut point = [at[0], at[2]];
    for axis in 0..2 {
        let mut left = by[axis];
        // A column at a time, so a long move is stepped through each it crosses.
        while left != 0.0 {
            let cell = point[axis].floor();
            let way = left.signum();
            let edge = if way > 0.0 { cell + 1.0 } else { cell };
            let to_edge = edge - point[axis];
            // Within this column, keeping the margin from a side it cannot step past.
            let lead = point[axis] + left + way * margin;
            if (lead - edge) * way < 0.0 || lead == edge {
                point[axis] += left;
                break;
            }
            let (dx, dz) = if axis == 0 { (way as i32, 0) } else { (0, way as i32) };
            match nav.step(place, dx, dz, walker) {
                Some(next) if (point[axis] + left - edge) * way >= 0.0 => {
                    // Into the next column, and on from its side.
                    place = next;
                    point[axis] = edge + way * 1e-4;
                    left -= to_edge + way * 1e-4;
                }
                Some(_) => {
                    point[axis] += left;
                    break;
                }
                None => {
                    point[axis] = edge - way * margin;
                    break;
                }
            }
        }
    }
    let feet = nav.point(place).map_or(at[1], |floor| floor[1]);
    (place, [point[0], feet, point[1]])
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::blocks::{
        BlockNavBuilder,
        tests::{FACES, STONE, floor},
        walking::WALKERS,
    };

    #[test]
    fn a_move_steps_from_column_to_column_and_slides_along_a_wall() {
        let mut blocks = floor();
        for z in 0..16 {
            blocks.set([6, 1, z], STONE);
            blocks.set([6, 2, z], STONE);
        }
        blocks.set([3, 1, 8], STONE);
        let mut builder = BlockNavBuilder::new(&WALKERS);
        builder.settle(&Arc::new(blocks), &Arc::from(&FACES[..]));
        let nav = builder.nav();
        let walker = Walker::blocks(2);
        let start = nav.standing_at([2.5, 1.0, 2.5], walker, 1.0).unwrap();
        // Along x, over two columns.
        let (place, at) = slide(nav, start, [2.5, 1.0, 2.5], [2.0, 0.0], walker, 0.3);
        assert_eq!(nav.cell(place), Some([4, 1, 2]));
        assert!((at[0] - 4.5).abs() < 1e-3 && (at[2] - 2.5).abs() < 1e-3, "{at:?}");
        // Into the wall, and along it: kept off it, still going along.
        let (place, at) = slide(nav, place, at, [3.0, 1.5], walker, 0.3);
        assert_eq!(nav.cell(place), Some([5, 1, 4]));
        assert!((at[0] - 5.7).abs() < 1e-3 && (at[2] - 4.0).abs() < 1e-3, "{at:?}");
        // Up onto a block a block high: its feet on top of it.
        let near = nav.standing_at([3.5, 1.0, 7.5], walker, 1.0).unwrap();
        let (place, at) = slide(nav, near, [3.5, 1.0, 7.5], [0.0, 1.0], walker, 0.3);
        assert_eq!(nav.cell(place), Some([3, 2, 8]));
        assert_eq!(at[1], 2.0);
    }
}
