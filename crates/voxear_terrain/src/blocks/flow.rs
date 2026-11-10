//! Water flowing through a world of blocks, a step at a time.
//!
//! Water placed, or a generated sea, is a **source**: full, still, never
//! draining. Water runs from it: down first, into the block under it,
//! which is then full and **falling**; and where it stands on ground, out
//! sideways, an eighth of a block lower each block it goes, until after
//! [`REACH`] blocks it runs out. Water falling, or landing on water, runs
//! out no further, so a waterfall keeps to its column and a lake it falls
//! into takes it. Water no longer fed dries a step at a time, from where its
//! feed was cut. On a planet, down is towards its middle: along whichever
//! axis a block lies furthest out from it.
//!
//! A step looks only at the blocks near where something changed the step
//! before (or was built, broken or dug), each worked out from the blocks as
//! they were before the step, so the order they are looked at in changes
//! nothing and every machine flows the same. Water stays within the
//! chunks its world reaches.
//!
//! A block of flowing water keeps how high it stands and the way up from it
//! in its fill (see `chunk::Fill`), so saves and the network carry it as
//! they carry any change of a block.

use std::collections::BTreeSet;

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId, CHUNK, EMPTY, FULL, Fill, depth_of, fill_of},
    is_ground, is_water,
};

/// How many blocks water runs on sideways from what feeds it.
pub const REACH: u8 = 7;

/// How many steps water takes a second.
pub const STEPS_A_SECOND: f32 = 5.0;

/// A source's fill, and falling water's.
const SOURCE: Fill = FULL;
const FALLING: Fill = 254;

/// Where flowing water's fills start: then eight a way up, a level each.
const FLOWING: Fill = 128;

/// One block of water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Water {
    /// Full, still and never draining: water placed, or a generated sea.
    Source,
    /// Full, fed from above.
    Falling,
    /// Run out sideways: `level` eighths of a block high, from 1 to
    /// [`REACH`], standing on its side facing away from `up` (a facing:
    /// +x, -x, +y, -y, +z, -z).
    Flowing { level: u8, up: u8 },
}

impl Water {
    /// The water a block of water's fill says: a source unless it says
    /// otherwise.
    pub fn of(fill: Fill) -> Self {
        // Eight fills a way up, the first of them none.
        let (up, level) = (fill.wrapping_sub(FLOWING) / 8, fill.wrapping_sub(FLOWING) % 8);
        match fill {
            FALLING => Self::Falling,
            _ if fill >= FLOWING && up < 6 && level > 0 => Self::Flowing { level, up },
            _ => Self::Source,
        }
    }

    /// Its fill.
    pub fn fill(self) -> Fill {
        match self {
            Self::Source => SOURCE,
            Self::Falling => FALLING,
            Self::Flowing { level, up } => FLOWING + up * 8 + level.clamp(1, REACH),
        }
    }

    /// How high it stands in its block, in sixteenths, and the way up from
    /// it where it is not full.
    pub fn height(self) -> (u8, Option<usize>) {
        match self {
            Self::Flowing { level, up } => (level * 2, Some(usize::from(up))),
            _ => (16, None),
        }
    }

    /// How far on it runs sideways: a full block's [`REACH`] blocks, and one
    /// fewer each block it has run.
    fn feeds(self) -> u8 {
        match self {
            Self::Flowing { level, .. } => level.saturating_sub(1),
            _ => REACH,
        }
    }
}

/// Which way water falls in a world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fall {
    /// Down, along -y.
    Down,
    /// Towards the world's corner, a planet's middle.
    Middle,
}

impl Fall {
    /// The way up from block `at`, as a facing (+x, -x, +y, -y, +z, -z): on
    /// a planet, out from its middle along the axis the block's middle lies
    /// furthest out along, the first of those as far.
    pub fn up(self, at: [i32; 3]) -> usize {
        match self {
            Self::Down => 2,
            Self::Middle => {
                let middle = at.map(|v| v as f32 + 0.5);
                let mut axis = 0;
                for i in 1..3 {
                    if middle[i].abs() > middle[axis].abs() {
                        axis = i;
                    }
                }
                axis * 2 + usize::from(middle[axis] < 0.0)
            }
        }
    }
}

/// The block one step from `at` the way facing `facing` faces.
pub fn beside(at: [i32; 3], facing: usize) -> [i32; 3] {
    let mut next = at;
    next[facing / 2] += if facing & 1 == 0 { 1 } else { -1 };
    next
}

/// The blocks a world's chunks reach, lowest and highest corner.
fn bounds(map: &BlockMap) -> Option<([i32; 3], [i32; 3])> {
    let side = CHUNK as i32;
    map.chunks().map(|(key, _)| *key).fold(None, |kept, key| {
        let (low, high) = (key.map(|v| v * side), key.map(|v| v * side + side - 1));
        Some(match kept {
            None => (low, high),
            Some((l, h)) => (std::array::from_fn(|i| l[i].min(low[i])), std::array::from_fn(|i| h[i].max(high[i]))),
        })
    })
}

/// The changes one step of flowing makes to `map`, each block given in
/// blocks from the world's corner as it becomes: for each of the blocks
/// `near`, the water that feeds it now, or none. `faces` says which kinds
/// are water, `fall` which way is down, and `smooth` whether the world is
/// drawn smooth, where dried blocks keep how far out of the ground they are.
pub fn step(
    map: &BlockMap,
    faces: &[BlockFaces],
    fall: Fall,
    smooth: bool,
    near: &BTreeSet<[i32; 3]>,
) -> Vec<([i32; 3], BlockId, Fill)> {
    let Some((low, high)) = bounds(map) else { return Vec::new() };
    let inside = |at: [i32; 3]| (0..3).all(|i| (low[i]..=high[i]).contains(&at[i]));
    // Past the world's chunks is as good as ground: water goes no further.
    let block = |at: [i32; 3]| inside(at).then(|| map.get(at));
    let water = |at: [i32; 3]| block(at).filter(|&b| is_water(faces, b)).map(|b| (b, Water::of(map.fill(at))));
    let on_ground = |at: [i32; 3]| block(at).is_none_or(|b| is_ground(faces, b));
    let mut changes = Vec::new();
    for &at in near {
        let Some(now) = block(at) else { continue };
        if is_ground(faces, now) {
            continue;
        }
        let was = water(at);
        if matches!(was, Some((_, Water::Source))) {
            continue;
        }
        let up = fall.up(at);
        // Fed from above, it falls; else by the fullest water beside it that cannot fall itself.
        let fed = match water(beside(at, up)) {
            Some((kind, _)) => Some((kind, Water::Falling)),
            None => {
                let mut best: Option<(u8, BlockId)> = None;
                for facing in (0..6).filter(|facing| facing / 2 != up / 2) {
                    let from = beside(at, facing);
                    let Some((kind, feeding)) = water(from) else { continue };
                    if !on_ground(beside(from, fall.up(from) ^ 1)) {
                        continue;
                    }
                    let level = feeding.feeds();
                    if level > 0 && best.is_none_or(|(l, k)| level > l || level == l && kind < k) {
                        best = Some((level, kind));
                    }
                }
                best.map(|(level, kind)| (kind, Water::Flowing { level, up: up as u8 }))
            }
        };
        match (was, fed) {
            (None, None) => {}
            (Some(_), None) => {
                let fill = if smooth { dried(map, faces, at) } else { EMPTY };
                changes.push((at, AIR, fill));
            }
            (was, Some((kind, water))) => {
                if was != Some((kind, water)) {
                    changes.push((at, kind, water.fill()));
                }
            }
        }
    }
    changes
}

/// The fill of a block of water dried in a world drawn smooth: a block
/// further out of the ground than the ground beside it, as the smooth
/// mesher takes water to be.
fn dried(map: &BlockMap, faces: &[BlockFaces], at: [i32; 3]) -> Fill {
    let out = (0..6)
        .map(|facing| beside(at, facing))
        .filter(|&next| is_ground(faces, map.get(next)))
        .map(|next| depth_of(map.fill(next)) - 1.0)
        .fold(-1.0f32, f32::max);
    fill_of(out.min(-1e-3), false)
}

/// The blocks a change at `at` can set flowing: it and those beside it.
pub fn reached(at: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
    std::iter::once(at).chain((0..6).map(move |facing| beside(at, facing)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{
        BlockWorld,
        file::{BlockFile, BlockKind},
    };

    /// Stone 1, water 2.
    fn faces() -> Vec<BlockFaces> {
        let mut file = BlockFile { blocks: vec![BlockKind::colored("Stone", [0.5; 3])], ..BlockFile::default() };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        file.faces().1
    }

    /// Steps `world`'s water until it settles, at most `steps` times; how
    /// many it took.
    fn settle(world: &mut BlockWorld, steps: usize) -> usize {
        (1..=steps).find(|_| world.flow() == 0 && world.flowing.is_empty()).expect("settled")
    }

    #[test]
    fn a_fill_keeps_the_water_it_was_made_of() {
        for water in
            [Water::Source, Water::Falling, Water::Flowing { level: 1, up: 0 }, Water::Flowing { level: 7, up: 5 }]
        {
            assert_eq!(Water::of(water.fill()), water);
        }
        assert_eq!(Water::of(200), Water::Source, "any other fill of water is a source");
        assert_eq!(Water::Flowing { level: 7, up: 2 }.height(), (14, Some(2)));
        assert_eq!(Water::Falling.height(), (16, None));
        // On a planet, up is out from its middle along the axis a block lies furthest out along.
        assert_eq!([Fall::Down.up([5, -40, 2]), Fall::Middle.up([5, -40, 2]), Fall::Middle.up([30, 2, -9])], [2, 3, 0]);
    }

    #[test]
    fn a_source_on_a_floor_runs_out_seven_blocks_lower_each_and_off_its_edge_falls() {
        // A stone floor 12 blocks along x and 20 across, a block of water on it near one end, and another floor
        // 4 blocks below.
        let faces = faces();
        let mut map = BlockMap::default();
        for z in 0..20 {
            for x in 0..12 {
                map.set([x, 4, z], 1);
            }
            for x in 0..40 {
                map.set([x, 0, z], 1);
            }
        }
        map.set([1, 5, 1], 2);
        let mut world = BlockWorld::new(map, &faces, 1.0);
        world.flowing = std::sync::Arc::new(reached([1, 5, 1]).collect());
        let steps = settle(&mut world, 200);
        let water = |at: [i32; 3]| {
            let blocks = &world.blocks;
            (blocks.get(at) == 2).then(|| Water::of(blocks.fill(at)))
        };
        // Along the upper floor, an eighth lower each block.
        for x in 2..9 {
            assert_eq!(water([x, 5, 1]), Some(Water::Flowing { level: 8 - (x - 1) as u8, up: 2 }), "at {x}");
        }
        assert_eq!(water([9, 5, 1]), None, "seven blocks and no more");
        assert_eq!(water([1, 5, 1]), Some(Water::Source), "the source stays");
        assert_eq!(water([1, 6, 1]), None, "nothing runs up");
        // The upper floor cut short within its reach: it runs off the end, falls and runs on along the floor below.
        let mut far = world.clone();
        far.set_blocks((5..12).flat_map(|x| (0..20).map(move |z| ([x, 4, z], AIR))));
        settle(&mut far, 200);
        let below = |x: i32, y: i32| (far.blocks.get([x, y, 1]) == 2).then(|| Water::of(far.blocks.fill([x, y, 1])));
        assert_eq!(below(5, 5), Some(Water::Flowing { level: 4, up: 2 }), "at the edge");
        assert_eq!((below(5, 4), below(5, 1)), (Some(Water::Falling), Some(Water::Falling)), "falling");
        assert_eq!(below(6, 1), Some(Water::Flowing { level: 7, up: 2 }), "run on from where it lands");
        assert!(steps > 7 && steps < 40, "{steps} steps");
        // Its source taken away, it all dries.
        far.set_blocks([([1, 5, 1], AIR)]);
        settle(&mut far, 200);
        let left = (0..40).flat_map(|x| (0..8).flat_map(move |y| (0..20).map(move |z| [x, y, z])));
        assert_eq!(left.filter(|&at| far.blocks.get(at) == 2).count(), 0);
    }

    #[test]
    fn on_a_voxel_terrain_planet_water_falls_towards_its_middle_and_dries_back_to_air_by_the_ground() {
        // A ball of stone round the corner, drawn smooth, a source on its +x side: it runs out over that side,
        // never up.
        let faces = faces();
        let mut map = BlockMap::default();
        for z in -8..8 {
            for y in -8..8 {
                for x in -8..8 {
                    map.set([x, y, z], 1);
                }
            }
        }
        let mut world = BlockWorld::made(map, &faces, 1.0, true);
        world.planet = Some(8.0);
        world.set_blocks([([8, 0, 0], 2)]);
        settle(&mut world, 100);
        let water =
            |world: &BlockWorld, at: [i32; 3]| (world.blocks.get(at) == 2).then(|| Water::of(world.blocks.fill(at)));
        assert_eq!(water(&world, [8, 3, 0]), Some(Water::Flowing { level: 5, up: 0 }), "along the side, up along +x");
        assert_eq!(water(&world, [8, 0, -2]), Some(Water::Flowing { level: 6, up: 0 }));
        assert_eq!(water(&world, [9, 0, 0]), None, "never out from the middle");
        assert!(world.meshes.values().any(|mesh| mesh.water().1 > 0), "drawn as cubes");
        // Its source taken away, it dries, each block air just out of the ground beside it.
        world.set_blocks([([8, 0, 0], AIR)]);
        settle(&mut world, 100);
        assert_eq!(water(&world, [8, 3, 0]), None);
        assert_eq!(world.blocks.fill([8, 3, 0]), 127);
    }

    #[test]
    fn a_step_moves_water_a_block_on_from_the_blocks_as_they_were() {
        let faces = faces();
        let mut map = BlockMap::default();
        for x in 0..20 {
            for z in 0..20 {
                map.set([x, 0, z], 1);
            }
        }
        map.set([5, 1, 5], 2);
        map.set([12, 1, 9], 2);
        let near: BTreeSet<[i32; 3]> = (0..20).flat_map(|x| (0..20).map(move |z| [x, 1, z])).collect();
        // Only the blocks beside each source, though those beside them are looked at after them.
        let once = step(&map, &faces, Fall::Down, false, &near);
        assert_eq!(once.len(), 8, "four round each source");
        let beside_a_source = |at: [i32; 3]| [[5, 1, 5], [12, 1, 9]].iter().any(|s| reached(*s).any(|r| r == at));
        assert!(once.iter().all(|&(at, block, fill)| beside_a_source(at) && block == 2 && fill == 128 + 16 + 7));
    }
}
