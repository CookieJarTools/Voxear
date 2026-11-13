//! Water flowing through a world of blocks, a step at a time.
//!
//! Water placed, or a generated sea, is a **source**: full, still, never
//! draining. Water runs from it: down first, into the block under it,
//! which is then full and **falling**; and where it stands on ground or on
//! water not falling, out sideways, an eighth of a block lower each block it goes, until
//! after [`REACH`] blocks it runs out, so a lake runs into a hole dug beside
//! it. Falling water runs out sideways only where it lands on ground, so a
//! waterfall keeps to its column and a lake it falls into takes it. A block
//! between two sources, on ground or water, becomes a source itself, so a
//! gap between two pools fills to the brim. Water no longer fed dries a step
//! at a time, from where its feed was cut.
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
    is_ground, is_water, keeps_water_out,
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

/// Where a source's fills start that say how high its surface stands in a
/// world drawn smooth (see [`surface_fill`]), and how many steps of a block
/// they have.
const SURFACE: Fill = 176;
const SURFACE_STEPS: f32 = 77.0;

/// Where the fills of a source with a current start, in a world of cubes: a
/// generated river's water, still to flowing as any source is, that moves
/// things along its current and stands as high in its block as the river's
/// water does, so a river's surface comes down in eighths of a block as
/// water placed and running out does. Then eight a way it flows, a height
/// each, of [`WAYS`]. They are the fills a world drawn smooth keeps its
/// sea's surface in, which a world of cubes has no use for.
pub(super) const CURRENT: Fill = SURFACE;

/// The ways a current flows, along x and z, from +x round towards +z.
pub const WAYS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.707_106_77, 0.707_106_77],
    [0.0, 1.0],
    [-0.707_106_77, 0.707_106_77],
    [-1.0, 0.0],
    [-0.707_106_77, -0.707_106_77],
    [0.0, -1.0],
    [0.707_106_77, -0.707_106_77],
];

/// How fast a river's current flows, in metres a second.
pub const RIVER_SPEED: f32 = 1.5;

/// The fill of a river's source flowing along `flow` (x and z, its speed
/// left out) that stands `eighths` of its block high, 1 to 8: the nearest
/// of [`WAYS`].
pub fn current_fill(flow: [f32; 2], eighths: u8) -> Fill {
    let way = (0..WAYS.len())
        .max_by(|&a, &b| {
            let along = |at: usize| WAYS[at][0] * flow[0] + WAYS[at][1] * flow[1];
            along(a).total_cmp(&along(b))
        })
        .unwrap_or(0);
    CURRENT + (way * 8) as Fill + eighths.clamp(1, 8) - 1
}

/// Which of [`WAYS`] a river's source of fill `fill` flows, and how many
/// eighths of its block it stands high, where it has a current (see
/// [`current_fill`]): in a world of cubes alone.
pub fn current_parts(fill: Fill) -> Option<(usize, u8)> {
    let at = usize::from(fill.checked_sub(CURRENT)?);
    (at < WAYS.len() * 8).then_some((at / 8, (at % 8) as u8 + 1))
}

/// How a source of fill `fill` flows, in metres a second along x and z,
/// where it has a current (see [`current_fill`]): in a world of cubes alone.
pub fn current_of(fill: Fill) -> Option<[f32; 2]> {
    let (way, _) = current_parts(fill)?;
    Some(WAYS[way].map(|v| v * RIVER_SPEED))
}

/// How high a block of water of fill `fill` stands in its block, in
/// sixteenths, and the way up from it where it is not full, in a world of
/// cubes: a river's source as high as its current's fill says, its way up
/// straight up, as the top of water does that stands lower than its block.
pub fn water_height(fill: Fill) -> (u8, Option<usize>) {
    match (Water::of(fill), current_parts(fill)) {
        (Water::Source, Some((_, eighths))) if eighths < 8 => (eighths * 2, Some(UP)),
        (water, _) => water.height(),
    }
}

/// The fill of a source whose surface, in a world drawn smooth, stands
/// `above` blocks over its middle, from nothing to a whole block (the
/// middle of the block over it): as a generated sea's top blocks are, so it
/// lies at its sea level. To flowing it is a source like any other.
pub fn surface_fill(above: f32) -> Fill {
    SURFACE + (above.clamp(0.0, 1.0) * SURFACE_STEPS).round() as Fill
}

/// How far over a block of water's middle its surface stands in a world
/// drawn smooth, in blocks: a half for a full block, its top; as its fill
/// says for a sea's top block; and for flowing water as high as it flows,
/// but never under a twentieth of a block, so the thinnest still shows.
pub fn smooth_surface(fill: Fill) -> f32 {
    let above = match fill {
        SURFACE..FALLING => f32::from(fill - SURFACE) / SURFACE_STEPS,
        _ => match Water::of(fill) {
            Water::Flowing { level, .. } => f32::from(level) / 8.0 - 0.5,
            _ => 0.5,
        },
    };
    above.max(0.05)
}

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

/// The way up from water, as a facing (+x, -x, +y, -y, +z, -z).
pub const UP: usize = 2;

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
/// are water, and `smooth` whether the world is
/// drawn smooth, where dried blocks keep how far out of the ground they are.
pub fn step(
    map: &BlockMap,
    faces: &[BlockFaces],
    smooth: bool,
    near: &BTreeSet<[i32; 3]>,
) -> Vec<([i32; 3], BlockId, Fill)> {
    let Some((low, high)) = bounds(map) else { return Vec::new() };
    let inside = |at: [i32; 3]| (0..3).all(|i| (low[i]..=high[i]).contains(&at[i]));
    // Past the world's chunks is as good as ground: water goes no further.
    let block = |at: [i32; 3]| inside(at).then(|| map.get(at));
    let water = |at: [i32; 3]| block(at).filter(|&b| is_water(faces, b)).map(|b| (b, Water::of(map.fill(at))));
    let on_ground = |at: [i32; 3]| block(at).is_none_or(|b| keeps_water_out(faces, b));
    // What water can stand on: ground, or more water that is not falling away under it.
    let held = |at: [i32; 3]| on_ground(at) || water(at).is_some_and(|(_, water)| water != Water::Falling);
    let mut changes = Vec::new();
    for &at in near {
        let Some(now) = block(at) else { continue };
        if keeps_water_out(faces, now) {
            continue;
        }
        let was = water(at);
        if matches!(was, Some((_, Water::Source))) {
            continue;
        }
        let up = UP;
        // Fed from above, it falls; else by the fullest water beside it that stands on something, falling water
        // only on ground; and between two sources, on something itself, it is one.
        let fed = match water(beside(at, up)) {
            Some((kind, _)) => Some((kind, Water::Falling)),
            None => {
                let mut best: Option<(u8, BlockId)> = None;
                let mut sources: Option<(u32, BlockId)> = None;
                for facing in (0..6).filter(|facing| facing / 2 != up / 2) {
                    let from = beside(at, facing);
                    let Some((kind, feeding)) = water(from) else { continue };
                    let under = beside(from, up ^ 1);
                    let stands = if feeding == Water::Falling { on_ground(under) } else { held(under) };
                    if !stands {
                        continue;
                    }
                    if feeding == Water::Source {
                        let (count, least) = sources.unwrap_or((0, kind));
                        sources = Some((count + 1, least.min(kind)));
                    }
                    let level = feeding.feeds();
                    if level > 0 && best.is_none_or(|(l, k)| level > l || level == l && kind < k) {
                        best = Some((level, kind));
                    }
                }
                match sources {
                    Some((count, kind)) if count >= 2 && held(beside(at, up ^ 1)) => Some((kind, Water::Source)),
                    _ => best.map(|(level, kind)| (kind, Water::Flowing { level, up: up as u8 })),
                }
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
        // A sea's top block drawn smooth keeps where its surface lies, and is a source.
        assert_eq!(Water::of(surface_fill(0.2)), Water::Source);
        assert!((smooth_surface(surface_fill(0.2)) - 0.2).abs() < 0.01);
        assert!((smooth_surface(surface_fill(0.8)) - 0.8).abs() < 0.01, "over its top, into the air");
        assert_eq!((smooth_surface(FULL), smooth_surface(Water::Flowing { level: 6, up: 2 }.fill())), (0.5, 0.25));
        assert_eq!(Water::Flowing { level: 7, up: 2 }.height(), (14, Some(2)));
        assert_eq!(Water::Falling.height(), (16, None));
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
    fn a_lake_runs_into_a_hole_dug_beside_it_and_a_gap_between_two_fills_to_the_brim() {
        // A lake 3 deep and 6 across in a stone bank, a hole 2 deep dug into the bank beside it.
        let faces = faces();
        let mut map = BlockMap::default();
        for z in 0..12 {
            for x in 0..20 {
                for y in 0..4 {
                    map.set([x, y, z], 1);
                }
            }
        }
        for z in 3..9 {
            for x in 3..9 {
                for y in 1..4 {
                    map.set([x, y, z], 2);
                }
            }
        }
        let mut world = BlockWorld::new(map, &faces, 1.0);
        world.set_blocks((9..13).flat_map(|x| (2..4).map(move |y| ([x, y, 5], AIR))));
        settle(&mut world, 100);
        let water = |at: [i32; 3]| (world.blocks.get(at) == 2).then(|| Water::of(world.blocks.fill(at)));
        assert_eq!(water([9, 3, 5]), Some(Water::Flowing { level: 7, up: 2 }), "its top runs in");
        assert_eq!(water([9, 2, 5]), Some(Water::Falling), "filled to its floor");
        // Landed on the hole's floor, it runs on along it, lower further in.
        let floor = [10, 11, 12].map(|x| water([x, 2, 5]));
        assert_eq!(floor, [7, 6, 5].map(|level| Some(Water::Flowing { level, up: 2 })));
        // A gap a block wide between two pools: a source, full, once both are beside it.
        let mut gap = BlockMap::default();
        for x in 0..7 {
            gap.set([x, 0, 0], 1);
        }
        let mut world = BlockWorld::new(gap, &faces, 1.0);
        world.set_blocks([([1, 1, 0], 2), ([3, 1, 0], 2)]);
        settle(&mut world, 20);
        assert_eq!(Water::of(world.blocks.fill([2, 1, 0])), Water::Source);
        assert_eq!(Water::of(world.blocks.fill([4, 1, 0])), Water::Flowing { level: 7, up: 2 });
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
        let once = step(&map, &faces, false, &near);
        assert_eq!(once.len(), 8, "four round each source");
        let beside_a_source = |at: [i32; 3]| [[5, 1, 5], [12, 1, 9]].iter().any(|s| reached(*s).any(|r| r == at));
        assert!(once.iter().all(|&(at, block, fill)| beside_a_source(at) && block == 2 && fill == 128 + 16 + 7));
    }
}
