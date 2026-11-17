use std::sync::Arc;

use voxear_terrain::blocks::{BlockFaces, BlockMap, path};

use super::*;
use crate::blocks::{
    BlockNavBuilder, PlaceKind,
    tests::{FACES, STONE, WATER, floor},
};

fn nav_of(blocks: &BlockMap) -> Arc<BlockNav> {
    nav_with(blocks, &FACES)
}

fn nav_with(blocks: &BlockMap, faces: &[BlockFaces]) -> Arc<BlockNav> {
    let mut builder = BlockNavBuilder::new(&crate::blocks::walking::WALKERS);
    builder.settle(&Arc::new(blocks.clone()), &Arc::from(faces));
    builder.nav().clone()
}

fn cell(at: [f32; 3]) -> [i32; 3] {
    at.map(|v| v.floor() as i32)
}

/// The middle of block `at`'s floor, in blocks.
fn on(at: [i32; 3]) -> [f32; 3] {
    [at[0] as f32 + 0.5, at[1] as f32, at[2] as f32 + 0.5]
}

fn way(nav: &BlockNav, from: [i32; 3], to: [i32; 3]) -> FoundPath {
    find_path(nav, &PathAsk::new(on(from), on(to), Walker::blocks(2)))
}

/// How far a way goes, along its corners.
fn length(found: &FoundPath) -> f32 {
    found.corners.windows(2).map(|pair| distance(pair[0], pair[1])).sum()
}

#[test]
fn a_way_over_a_flat_floor_is_one_straight_line() {
    let nav = nav_of(&floor());
    let found = way(&nav, [1, 1, 1], [12, 1, 7]);
    assert_eq!(found.status, PathStatus::Complete);
    assert_eq!(found.corners.len(), 2, "straight from one to the other: {:?}", found.corners);
    assert_eq!((cell(found.corners[0]), cell(found.corners[1])), ([1, 1, 1], [12, 1, 7]));
    let octile = 5.0 + 6.0 * std::f32::consts::SQRT_2;
    assert!((found.cost - octile).abs() < 1e-3, "{} against {octile}", found.cost);
    let found = way(&nav, [3, 1, 3], [3, 1, 3]);
    assert_eq!((found.status, found.corners.len()), (PathStatus::Complete, 1), "already there");
}

#[test]
fn a_wall_is_gone_round_through_its_gap_and_a_shut_one_gives_the_nearest_way() {
    let mut blocks = floor();
    for z in 0..15 {
        blocks.set([8, 1, z], STONE);
        blocks.set([8, 2, z], STONE);
    }
    let found = way(&nav_of(&blocks), [4, 1, 2], [12, 1, 2]);
    assert_eq!(found.status, PathStatus::Complete);
    assert!(found.corners.iter().any(|at| cell(*at)[2] == 15), "through the gap: {:?}", found.corners);
    assert!(found.corners.len() <= 4, "pulled tight: {:?}", found.corners);
    // Every step between corners is walkable: the corners never cut the wall.
    for pair in found.corners.windows(2) {
        let (a, b) = (cell(pair[0]), cell(pair[1]));
        assert!(!(a[0] < 8 && b[0] > 8 && a[2].max(b[2]) < 15), "through the wall: {pair:?}");
    }
    blocks.set([8, 1, 15], STONE);
    blocks.set([8, 2, 15], STONE);
    let found = way(&nav_of(&blocks), [4, 1, 2], [12, 1, 2]);
    assert_eq!(found.status, PathStatus::Partial);
    assert_eq!(cell(*found.corners.last().unwrap())[0], 7, "up to the wall: {:?}", found.corners);
}

#[test]
fn a_step_of_a_block_is_climbed_two_are_not_and_a_drop_is_taken() {
    let mut blocks = floor();
    for x in 8..16 {
        for z in 0..16 {
            blocks.set([x, 1, z], STONE);
        }
    }
    let nav = nav_of(&blocks);
    assert_eq!(way(&nav, [4, 1, 2], [12, 2, 2]).status, PathStatus::Complete);
    assert_eq!(way(&nav, [12, 2, 2], [4, 1, 2]).status, PathStatus::Complete);
    for x in 8..16 {
        for z in 0..16 {
            blocks.set([x, 2, z], STONE);
        }
    }
    let nav = nav_of(&blocks);
    assert_eq!(way(&nav, [4, 1, 2], [12, 3, 2]).status, PathStatus::Partial, "two blocks up is a wall");
    assert_eq!(way(&nav, [12, 3, 2], [4, 1, 2]).status, PathStatus::Complete, "but a drop it takes");
}

#[test]
fn deep_water_is_gone_round_and_a_ford_waded_at_its_cost() {
    let mut blocks = floor();
    for x in 6..10 {
        for z in 0..16 {
            blocks.set([x, 0, z], WATER);
            blocks.set([x, -1, z], WATER);
            blocks.set([x, -2, z], STONE);
        }
    }
    assert_eq!(way(&nav_of(&blocks), [3, 1, 4], [12, 1, 4]).status, PathStatus::Partial);
    for x in 6..10 {
        blocks.set([x, -1, 14], STONE);
    }
    let found = way(&nav_of(&blocks), [3, 1, 4], [12, 1, 4]);
    assert_eq!(found.status, PathStatus::Complete);
    assert!(found.places.iter().any(|at| nav_of(&blocks).place(*at).unwrap().0.kind == PlaceKind::Wade));
}

/// A field of `size` blocks a side with hills, walls, pits and water, from `seed`.
fn field(size: i32, seed: u32) -> BlockMap {
    let mut blocks = BlockMap::default();
    let mut state = seed.wrapping_mul(2_654_435_761) | 1;
    let mut chance = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state % 1000) as f32 / 1000.0
    };
    for x in 0..size {
        for z in 0..size {
            let wave = ((x as f32 * 0.21).sin() + (z as f32 * 0.17).cos()) * 1.6;
            let height = (wave + 3.0).round() as i32;
            for y in 0..height {
                blocks.set([x, y, z], STONE);
            }
            match chance() {
                c if c < 0.08 => {
                    for y in height..height + 3 {
                        blocks.set([x, y, z], STONE);
                    }
                }
                c if c < 0.1 => {
                    blocks.set([x, height - 1, z], WATER);
                    blocks.set([x, height - 2, z], WATER);
                }
                c if c < 0.12 => blocks.set([x, height - 1, z], WATER),
                _ => {}
            }
        }
    }
    blocks
}

/// The places to stand in `blocks` the old way, a column at a time.
fn standing(blocks: &BlockMap, size: i32, seed: u32, count: usize) -> Vec<[i32; 3]> {
    let mut state = seed | 1;
    let mut found = Vec::new();
    while found.len() < count {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let (x, z) = ((state >> 8) as i32 % size, (state >> 20) as i32 % size);
        if let Some(at) = path::standing_near(blocks, &FACES, [x, 6, z], 2, 8) {
            found.push(at);
        }
    }
    found
}

#[test]
fn ways_reach_where_a_search_a_block_at_a_time_reaches_and_the_coarse_graph_keeps_them_short() {
    // How much dearer each way is than the cheapest there is, leaning towards the goal and not.
    let (mut leaning, mut cheapest) = (Vec::new(), Vec::new());
    for seed in 1..4 {
        let size = 96;
        let blocks = field(size, seed);
        let nav = nav_of(&blocks);
        let places = standing(&blocks, size, seed * 7, 24);
        for pair in places.chunks(2) {
            let (from, to) = (pair[0], pair[1]);
            let plain = path::find_path(&blocks, &FACES, from, to, 2, 1_000_000);
            let found = way(&nav, from, to);
            let reached = found.status == PathStatus::Complete;
            assert_eq!(reached, plain.is_some(), "seed {seed}, {from:?} to {to:?}: {:?}", found.status);
            if !reached {
                continue;
            }
            // The same search over places alone, every region open to it: the cheapest way there is.
            let ask = PathAsk::new(on(from), on(to), Walker::blocks(2));
            let alone = SCRATCH.with(|scratch| {
                let start = nav.nearest(on(from), ask.walker, 1.0, 2.0).unwrap();
                let goal = nav.nearest(on(to), ask.walker, 3.0, 4.0);
                fine(&mut scratch.borrow_mut(), &nav, None, start, goal, on(to), None, &ask, &Joins::default())
            });
            assert!(alone.reached);
            let best = alone.cost.max(1.0);
            assert!(found.cost <= best * GREED + 0.5, "seed {seed}: {} against {best} at best", found.cost);
            assert!(length(&found) <= found.cost + 0.01, "pulled tight, never longer");
            leaning.push(found.cost / best);
            let mut exact = ask.clone();
            exact.greed = 1.0;
            cheapest.push(find_path(&nav, &exact).cost / best);
        }
    }
    let mean = |costs: &[f32]| costs.iter().sum::<f32>() / costs.len() as f32;
    let worst = |costs: &[f32]| costs.iter().copied().fold(0.0, f32::max);
    assert!(leaning.len() > 10);
    // Over regions and then places, not leaning: as short as the cheapest, near enough.
    assert!(worst(&cheapest) < 1.05, "the coarse graph alone: {} at worst", worst(&cheapest));
    // Leaning towards the goal: a little dearer on the whole.
    assert!(mean(&leaning) < 1.05, "leaning: {} on average", mean(&leaning));
}

#[test]
fn a_long_way_is_found_place_by_place_only_as_far_as_asked() {
    let blocks = field(256, 9);
    let nav = nav_of(&blocks);
    let (from, to) = ([2, 6, 2], [250, 6, 250]);
    let mut ask = PathAsk::new(on(from), on(to), Walker::blocks(2));
    ask.refine = Some(2);
    let first = find_path(&nav, &ask);
    assert_eq!(first.status, PathStatus::Continues);
    let end = *first.corners.last().unwrap();
    assert!(distance(end, on(from)) < 96.0, "about two chunks on: {end:?}");
    ask.refine = None;
    let whole = find_path(&nav, &ask);
    assert_eq!(whole.status, PathStatus::Complete);
    assert!(whole.looked > first.looked);
}

#[test]
fn a_wander_goes_somewhere_it_can_walk_to_within_its_reach() {
    let nav = nav_of(&floor());
    let from = nav.nearest(on([8, 1, 8]), Walker::blocks(2), 1.0, 2.0).unwrap();
    for seed in 0..20 {
        let to = random_near(&nav, from, Walker::blocks(2), &AreaCosts::default(), 5.0, seed).unwrap();
        let (a, b) = (nav.point(from).unwrap(), nav.point(to).unwrap());
        let far = distance(a, b);
        assert!((5.0 / 3.0..=5.0).contains(&far), "{far}");
    }
}

/// A wall of stone across the floor at x = 8, two blocks high, its only way through a door at z = 4.
fn walled_with_a_door() -> (BlockMap, Vec<BlockFaces>) {
    use voxear_terrain::blocks::{PART_DRAWN, PART_SOLID, PART_USED, block::BlockShape};
    let mut faces = FACES.to_vec();
    let door =
        BlockFaces { shape: BlockShape::Door, part: PART_DRAWN | PART_SOLID | PART_USED, ..BlockFaces::default() };
    faces.push(door);
    let mut blocks = floor();
    for z in 0..16 {
        let block = if z == 4 { 3 } else { STONE };
        blocks.set([8, 1, z], block);
        blocks.set([8, 2, z], block);
    }
    (blocks, faces)
}

#[test]
fn a_shut_door_is_gone_through_by_what_opens_doors_and_is_a_wall_to_the_rest() {
    let (blocks, faces) = walled_with_a_door();
    let nav = nav_with(&blocks, &faces);
    let (from, to) = (on([4, 1, 4]), on([12, 1, 4]));
    let plain = find_path(&nav, &PathAsk::new(from, to, Walker::blocks(2)));
    assert_eq!(plain.status, PathStatus::Partial, "no way through for what opens no doors");
    let opener = Walker::blocks(2).with(false, true, 0);
    let found = find_path(&nav, &PathAsk::new(from, to, opener));
    assert_eq!(found.status, PathStatus::Complete);
    assert_eq!(found.doors, vec![[8, 1, 4]], "through the door: {:?}", found.corners);
}

#[test]
fn deep_water_is_swum_across_by_what_swims() {
    let mut blocks = floor();
    for x in 6..10 {
        for z in 0..16 {
            for y in -2..=0 {
                blocks.set([x, y, z], WATER);
            }
            blocks.set([x, -3, z], STONE);
        }
    }
    let nav = nav_of(&blocks);
    let (from, to) = (on([3, 1, 4]), on([12, 1, 4]));
    assert_eq!(find_path(&nav, &PathAsk::new(from, to, Walker::blocks(2))).status, PathStatus::Partial);
    let swimmer = Walker::blocks(2).with(true, false, 0);
    let found = find_path(&nav, &PathAsk::new(from, to, swimmer));
    assert_eq!(found.status, PathStatus::Complete, "{:?}", found.corners);
    assert!(found.places.iter().any(|at| nav.place(*at).unwrap().0.kind == PlaceKind::Swim));
}

#[test]
fn a_gap_is_jumped_by_what_jumps_and_only_as_wide_as_it_jumps() {
    // A trench across the floor, four blocks deep, one block wide; then one two blocks wide.
    let trench = |wide: i32| {
        let mut blocks = floor();
        for x in 0..16 {
            for z in 0..16 {
                for y in -4..0 {
                    blocks.set([x, y, z], STONE);
                }
            }
        }
        for x in 8..8 + wide {
            for z in 0..16 {
                for y in -3..=0 {
                    blocks.set([x, y, z], 0);
                }
            }
        }
        blocks
    };
    let (from, to) = (on([4, 1, 4]), on([13, 1, 4]));
    for (wide, jump, reaches) in [(1, 0, false), (1, 1, true), (2, 1, false), (2, 2, true)] {
        let nav = nav_of(&trench(wide));
        let found = find_path(&nav, &PathAsk::new(from, to, Walker::blocks(2).with(false, false, jump)));
        assert_eq!(found.status == PathStatus::Complete, reaches, "{wide} wide, jumping {jump}: {:?}", found.corners);
        if reaches {
            let at = found.jumps.iter().position(|jumped| *jumped).expect("a corner jumped to");
            let (before, after) = (cell(found.corners[at - 1]), cell(found.corners[at]));
            assert_eq!((before[0], after[0]), (7, 8 + wide), "from the edge to the far edge: {:?}", found.corners);
        }
    }
}

#[test]
fn a_road_is_walked_cheaply_ground_nothing_walks_on_is_gone_round_and_a_box_gives_its_area() {
    // A road (block 3) across the floor at z = 2, and a strip nothing walks on (block 4) across it at x = 8,
    // with a gap at z = 14.
    let mut faces = FACES.to_vec();
    faces.push(BlockFaces { nav_area: crate::ROAD + 1, ..BlockFaces::new(0, 0, 0) });
    faces.push(BlockFaces { nav_area: crate::NOT_WALKABLE + 1, ..BlockFaces::new(0, 0, 0) });
    let mut blocks = floor();
    for x in 0..16 {
        blocks.set([x, 0, 2], 3);
    }
    let nav = nav_with(&blocks, &faces);
    let along = way(&nav, [1, 1, 2], [13, 1, 2]);
    assert!((along.cost - 12.0 * 0.6).abs() < 1e-3, "along the road, at its cost: {}", along.cost);
    for z in 0..14 {
        blocks.set([8, 0, z], 4);
    }
    let nav = nav_with(&blocks, &faces);
    let round = way(&nav, [1, 1, 2], [13, 1, 2]);
    assert_eq!(round.status, PathStatus::Complete);
    assert!(round.corners.iter().any(|at| cell(*at)[2] >= 14), "through the gap: {:?}", round.corners);
    // A box over the gap, nothing to walk on: no way through.
    let mut ask = PathAsk::new(on([1, 1, 2]), on([13, 1, 2]), Walker::blocks(2));
    ask.boxes.push(crate::AreaBox { min: [7.0, 0.0, 13.0], max: [10.0, 4.0, 16.0], area: crate::NOT_WALKABLE });
    assert_eq!(find_path(&nav, &ask).status, PathStatus::Partial);
}

#[test]
fn a_link_is_taken_where_there_is_no_other_way_and_its_ends_are_corners() {
    // A wall two blocks high all the way across the floor: no way over it but a link.
    let mut blocks = floor();
    for z in 0..16 {
        blocks.set([8, 1, z], STONE);
        blocks.set([8, 2, z], STONE);
    }
    let nav = nav_of(&blocks);
    let mut ask = PathAsk::new(on([3, 1, 4]), on([13, 1, 4]), Walker::blocks(2));
    assert_eq!(find_path(&nav, &ask).status, PathStatus::Partial);
    ask.links.push(crate::Link { from: on([7, 1, 10]), to: on([9, 1, 10]), both_ways: false, cost: 1.0, kind: 3 });
    let over = find_path(&nav, &ask);
    assert_eq!(over.status, PathStatus::Complete, "{:?}", over.corners);
    let at = over.links.iter().position(|link| *link == Some(3)).expect("a corner reached by the link");
    assert_eq!((cell(over.corners[at - 1]), cell(over.corners[at])), ([7, 1, 10], [9, 1, 10]));
    // One way only: no way back over it.
    let back = find_path(&nav, &PathAsk { from: on([13, 1, 4]), to: on([3, 1, 4]), ..ask.clone() });
    assert_eq!(back.status, PathStatus::Partial);
}
