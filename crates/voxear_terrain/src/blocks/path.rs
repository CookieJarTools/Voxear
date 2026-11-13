//! Paths through a world of blocks, for what walks in one: from one place
//! standing to another, a block at a time along the ground, up a step of
//! one block and down a drop of a few, round what is in the way.
//!
//! A place can be stood in when the block under it is ground and it and
//! the blocks over it, as many as the walker is tall, are not. Water is not
//! ground: a walker wades where the bottom is a block down, and goes round
//! what is deeper. [`find_path`] searches outwards from the start, the
//! places that seem nearest the goal first, and gives up once it has
//! looked at more places than it is allowed, so a goal that cannot be
//! reached costs no more than that.

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
};

use super::{BlockFaces, BlockMap, PART_TALL, is_ground, is_water, part_of};

/// How many blocks a walker drops from a ledge at most.
pub const DEEPEST_DROP: i32 = 3;

/// Whether a walker `tall` blocks high can stand in `at`: ground under it,
/// but a fence or a wall, which nothing stands on, nothing of ground in it
/// or over it for as high as it is (an open door or gate is none), and
/// water no higher than its feet.
pub fn can_stand(blocks: &BlockMap, faces: &[BlockFaces], at: [i32; 3], tall: u32) -> bool {
    let block = |up: i32| blocks.get([at[0], at[1] + up, at[2]]);
    let clear = (0..tall.max(1) as i32).all(|up| !is_ground(faces, block(up)));
    let under = block(-1);
    is_ground(faces, under) && part_of(faces, under) & PART_TALL == 0 && clear && !is_water(faces, block(1))
}

/// The place to stand in the column of `at`, at its height or the nearest
/// to it within `reach` blocks up or down: where something that hovers a
/// little over the ground, or is asked to go to a point inside it, stands.
pub fn standing_near(blocks: &BlockMap, faces: &[BlockFaces], at: [i32; 3], tall: u32, reach: i32) -> Option<[i32; 3]> {
    let mut by = 0;
    while by <= reach {
        for y in [at[1] - by, at[1] + by] {
            if can_stand(blocks, faces, [at[0], y, at[2]], tall) {
                return Some([at[0], y, at[2]]);
            }
        }
        by += 1;
    }
    None
}

/// The places to step to from `from`: each of the four beside it at its
/// height, or one block up, or down as far as [`DEEPEST_DROP`], where a
/// walker `tall` blocks high can stand and has room to get there.
fn steps<'a>(
    blocks: &'a BlockMap,
    faces: &'a [BlockFaces],
    from: [i32; 3],
    tall: u32,
) -> impl Iterator<Item = [i32; 3]> + 'a {
    let solid = move |at: [i32; 3]| is_ground(faces, blocks.get(at));
    [[1, 0], [-1, 0], [0, 1], [0, -1]].into_iter().filter_map(move |[x, z]| {
        let beside = [from[0] + x, from[1], from[2] + z];
        // Level, or down: the way must be clear at its own height first.
        if (0..tall.max(1) as i32).all(|up| !solid([beside[0], beside[1] + up, beside[2]])) {
            return (0..=DEEPEST_DROP)
                .map(|down| [beside[0], beside[1] - down, beside[2]])
                .find(|at| can_stand(blocks, faces, *at, tall));
        }
        // Up a block: room over its own head to rise, and to stand there.
        let up = [beside[0], beside[1] + 1, beside[2]];
        let room = !solid([from[0], from[1] + tall.max(1) as i32, from[2]]);
        (room && can_stand(blocks, faces, up, tall)).then_some(up)
    })
}

/// The way from `from` to `to`, both places to stand, counted in blocks
/// from the world's corner: every place to step through in turn, `from`
/// first and `to` last. `tall` is how many blocks high the walker is, and
/// `most` how many places may be looked at before giving up. `None` where
/// there is no way, or none found by then.
pub fn find_path(
    blocks: &BlockMap,
    faces: &[BlockFaces],
    from: [i32; 3],
    to: [i32; 3],
    tall: u32,
    most: usize,
) -> Option<Vec<[i32; 3]>> {
    if !can_stand(blocks, faces, from, tall) || !can_stand(blocks, faces, to, tall) {
        return None;
    }
    let ahead = |at: [i32; 3]| ((at[0] - to[0]).abs() + (at[1] - to[1]).abs() + (at[2] - to[2]).abs()) as u32;
    // Each place reached: how far it is along the best way found to it, and the place before it on that way.
    let mut reached: HashMap<[i32; 3], (u32, [i32; 3])> = HashMap::from([(from, (0, from))]);
    let mut open = BinaryHeap::from([Reverse((ahead(from), 0u32, from))]);
    let mut looked = 0;
    while let Some(Reverse((_, so_far, at))) = open.pop() {
        if at == to {
            let mut path = vec![to];
            while let Some(&(_, before)) =
                reached.get(path.last()?).filter(|(_, before)| before != path.last().unwrap())
            {
                path.push(before);
            }
            path.reverse();
            return Some(path);
        }
        // A better way to it was found since it was put in.
        if reached.get(&at).is_some_and(|(best, _)| *best < so_far) {
            continue;
        }
        looked += 1;
        if looked > most {
            return None;
        }
        for next in steps(blocks, faces, at, tall) {
            // A step up or down is a little further than one along the flat.
            let cost = so_far + 2 + (next[1] - at[1]).unsigned_abs();
            if reached.get(&next).is_none_or(|(best, _)| cost < *best) {
                reached.insert(next, (cost, at));
                open.push(Reverse((cost + 2 * ahead(next), cost, next)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACES: [BlockFaces; 3] = [BlockFaces::new(0, 0, 0), BlockFaces::new(0, 0, 0), BlockFaces::water(0)];
    const STONE: u16 = 1;
    const WATER: u16 = 2;

    /// A floor of stone, 16 blocks a side, its top at height 1.
    fn floor() -> BlockMap {
        let mut blocks = BlockMap::default();
        for x in 0..16 {
            for z in 0..16 {
                blocks.set([x, 0, z], STONE);
            }
        }
        blocks
    }

    fn path(blocks: &BlockMap, from: [i32; 3], to: [i32; 3]) -> Option<Vec<[i32; 3]>> {
        find_path(blocks, &FACES, from, to, 2, 2000)
    }

    #[test]
    fn a_place_is_stood_in_with_ground_under_it_and_room_over_it() {
        let mut blocks = floor();
        assert!(can_stand(&blocks, &FACES, [3, 1, 3], 2));
        assert!(!can_stand(&blocks, &FACES, [3, 2, 3], 2), "in the air");
        assert!(!can_stand(&blocks, &FACES, [3, 0, 3], 2), "in the ground");
        blocks.set([3, 2, 3], STONE);
        assert!(!can_stand(&blocks, &FACES, [3, 1, 3], 2), "no room for its head");
        assert!(can_stand(&blocks, &FACES, [3, 1, 3], 1), "but one a block high fits under");
        assert_eq!(standing_near(&blocks, &FACES, [5, 4, 5], 2, 4), Some([5, 1, 5]), "down to the floor");
        assert_eq!(standing_near(&blocks, &FACES, [5, 4, 5], 2, 2), None, "not within reach");
    }

    #[test]
    fn the_way_over_a_flat_floor_is_as_short_as_any() {
        let found = path(&floor(), [1, 1, 1], [6, 1, 4]).unwrap();
        assert_eq!((found[0], *found.last().unwrap()), ([1, 1, 1], [6, 1, 4]));
        assert_eq!(found.len(), 5 + 3 + 1, "five along and three across, a block at a time");
        for pair in found.windows(2) {
            let moved: i32 = (0..3).map(|axis| (pair[1][axis] - pair[0][axis]).abs()).sum();
            assert_eq!(moved, 1, "one block at a time, never cornerwise: {pair:?}");
        }
        assert_eq!(path(&floor(), [1, 1, 1], [1, 1, 1]), Some(vec![[1, 1, 1]]), "already there");
    }

    #[test]
    fn a_wall_is_gone_round_a_step_climbed_and_a_ledge_dropped_from() {
        // A wall two blocks high across the floor, with a gap at one end.
        let mut blocks = floor();
        for z in 0..15 {
            blocks.set([8, 1, z], STONE);
            blocks.set([8, 2, z], STONE);
        }
        let found = path(&blocks, [4, 1, 2], [12, 1, 2]).unwrap();
        assert!(found.contains(&[8, 1, 15]), "through the gap: {found:?}");
        // With the gap shut there is no way, and the search stops.
        blocks.set([8, 1, 15], STONE);
        blocks.set([8, 2, 15], STONE);
        assert_eq!(path(&blocks, [4, 1, 2], [12, 1, 2]), None);

        // A step one block high is climbed; two are not.
        let mut blocks = floor();
        for x in 8..16 {
            for z in 0..16 {
                blocks.set([x, 1, z], STONE);
            }
        }
        let found = path(&blocks, [4, 1, 2], [12, 2, 2]).unwrap();
        assert!(found.contains(&[7, 1, 2]) && found.contains(&[8, 2, 2]), "up at the step: {found:?}");
        let back = path(&blocks, [12, 2, 2], [4, 1, 2]).unwrap();
        assert!(back.contains(&[7, 1, 2]), "and down again off it");
        for x in 8..16 {
            for z in 0..16 {
                blocks.set([x, 2, z], STONE);
            }
        }
        assert_eq!(path(&blocks, [4, 1, 2], [12, 3, 2]), None, "two blocks up is a wall");
        assert!(path(&blocks, [12, 3, 2], [4, 1, 2]).is_some(), "but a drop it can take");
    }

    #[test]
    fn deep_water_is_gone_round_and_shallow_water_waded() {
        // A pool two blocks deep across the floor's middle, with a ford one block deep at one end.
        let mut blocks = floor();
        for x in 6..10 {
            for z in 0..16 {
                blocks.set([x, 0, z], WATER);
                blocks.set([x, -1, z], WATER);
                blocks.set([x, -2, z], STONE);
            }
        }
        assert_eq!(path(&blocks, [3, 1, 4], [12, 1, 4]), None, "no way over water too deep to stand in");
        for x in 6..10 {
            blocks.set([x, -1, 14], STONE);
        }
        let found = path(&blocks, [3, 1, 4], [12, 1, 4]).unwrap();
        assert!(found.contains(&[7, 0, 14]), "waded where the bottom is a block down: {found:?}");
    }

    #[test]
    fn a_search_gives_up_once_it_has_looked_at_as_many_places_as_it_may() {
        let blocks = floor();
        assert!(find_path(&blocks, &FACES, [0, 1, 0], [15, 1, 15], 2, 2000).is_some());
        assert_eq!(find_path(&blocks, &FACES, [0, 1, 0], [15, 1, 15], 2, 5), None);
    }
}
