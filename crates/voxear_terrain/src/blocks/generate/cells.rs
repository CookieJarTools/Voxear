//! One cell of generated ground: where its generator's Volume comes from,
//! and the block and fill a cell is from what the Volume says of it.
//!
//! A generator whose Output has a Volume wired in makes the ground in three
//! dimensions itself. For one that has none, the world's own overhangs and
//! caves (`BlockGenerate`'s numbers) are wired into a copy of its graph as
//! the nodes that make them (`generate::Graph::add_voxels`), so ground is
//! made one way whichever keeps the numbers.

use std::sync::{Arc, Mutex};

use super::{BlockGenerate, columns::Column};
use crate::{
    blocks::{
        chunk::{AIR, BlockId, Fill, fill_of},
        flow::{Water, surface_fill},
    },
    generate::{NodeKind, OwnVoxels, WorldGen, nodes::Output},
};

/// How many blocks under the surface a world's own caves keep clear of, so
/// they open onto it only here and there.
pub const CAVE_ROOF: f32 = 3.0;

/// How short the slope of how far in the ground blocks are is taken to be
/// at least, in blocks a block, where a distance is measured against it. A
/// height made solid changes by a block for every block down, and faster
/// on a slope, where the number overstates how far the surface is. Where
/// pushing or a cave makes it change more slowly, the number is left as it
/// is: a block where it runs level is not taken to be far from a surface,
/// and blocks side by side are never measured so unlike that the deeper
/// reads as the nearer.
const LEAST_SLOPE: f32 = 1.0;

/// The generator a world of blocks' ground is made by.
pub(super) enum Source<'a> {
    /// The world's generator, whose graph has a Volume.
    Own(&'a WorldGen),
    /// The world's generator with the world's own overhangs and caves wired
    /// into its graph.
    Moved(Arc<WorldGen>),
}

impl Source<'_> {
    pub fn world(&self) -> &WorldGen {
        match self {
            Source::Own(world) => world,
            Source::Moved(world) => world,
        }
    }

    /// Whether the Volume is the generator's own, read where the world lies
    /// in the generator's world. A world's own overhangs and caves are read
    /// from the world's corner, as they always were.
    pub fn own(&self) -> bool {
        matches!(self, Source::Own(_))
    }
}

/// The generators made with worlds' own overhangs and caves wired in, by
/// the generator's hash and the numbers' bits: the last few asked for, so
/// each chunk made does not compile its own.
type Moved = Vec<((u64, [u32; 3]), Arc<WorldGen>)>;
static MOVED: Mutex<Moved> = Mutex::new(Vec::new());
const MOVED_KEPT: usize = 16;

/// The generator the ground `generate` asks of `world` is made by, for a
/// world of blocks `block_size` metres a side.
pub(super) fn source<'a>(generate: &BlockGenerate, world: &'a WorldGen, block_size: f32) -> Source<'a> {
    if world.volume().is_some() {
        return Source::Own(world);
    }
    let own = OwnVoxels {
        overhangs: generate.overhangs.max(0.0),
        caves: generate.caves.clamp(0.0, 1.0),
        roof: CAVE_ROOF * block_size,
    };
    let key = (world.hash(), [own.overhangs, own.caves, own.roof].map(f32::to_bits));
    let mut moved = MOVED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, found)) = moved.iter().find(|(kept, _)| *kept == key) {
        return Source::Moved(found.clone());
    }
    let mut graph = world.graph().clone();
    if graph.output().is_none() {
        graph.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
    }
    graph.add_voxels(own, false);
    let made = Arc::new(world.with_graph(graph));
    if moved.len() >= MOVED_KEPT {
        moved.remove(0);
    }
    moved.push((key, made.clone()));
    Source::Moved(made)
}

/// How deep under the surface a block of ground is, in blocks, for its
/// layer, counted down its column from the open air: `above` is the depth
/// of the block over it (nothing for open air), and `ground` how far into
/// the ground under the caves its own middle is. The first block under the
/// air is as deep as its middle, a block at most, and each under it a
/// block deeper, whatever pushed the ground about on the way.
pub(super) fn counted(above: f32, ground: f32) -> f32 {
    if ground <= 0.0 {
        0.0
    } else if above > 0.0 {
        above + 1.0
    } else if ground < 1.0 {
        ground
    } else {
        1.0
    }
}

/// How far from the surface a block's middle is, in blocks, from how far
/// into the ground the Volume puts it (`inside`) and those either side of
/// it along each axis (`beside`, as pairs before and after): the number
/// over how fast it changes from block to block, which is its distance
/// where the ground is steep or pushed about and the number alone is not.
pub(super) fn distance(inside: f32, beside: [[f32; 2]; 3]) -> f32 {
    let [x, y, z] = beside.map(|[before, after]| (after - before) * 0.5);
    let slope = (x * x + y * y + z * z).sqrt();
    inside / if slope > LEAST_SLOPE { slope } else { LEAST_SLOPE }
}

/// How steep a slope the ground's layers still follow in a world drawn
/// smooth, as how much of the way the ground deepens is downwards: three
/// tenths, for ground up to about 70° from level. Steeper is a wall.
const SKIN_FACING: f32 = 0.3;

/// How deep under the surface a block of ground is, for its layer, in a
/// world drawn smooth whose ground is more than a height. Counted down its
/// column alone, every step down a slope would show the layer under the
/// top one, in stripes. So where the ground faces up, a block is no deeper
/// than it is from the open air, and the layers follow the slope as a
/// skin; and on a wall, which nothing lies on, it is deeper than every
/// layer, so walls and the undersides of overhangs are bare.
///
/// `counted` is its depth counted down its column, `ground` how far into
/// the ground under the caves it is, `beside` the same for the blocks
/// either side of it along each axis, and `up` the way up.
pub(super) fn skinned(counted: f32, ground: f32, beside: [[f32; 2]; 3], up: [f32; 3]) -> f32 {
    let slope = beside.map(|[before, after]| (after - before) * 0.5);
    let length = (slope[0] * slope[0] + slope[1] * slope[1] + slope[2] * slope[2]).sqrt();
    let downwards = -(slope[0] * up[0] + slope[1] * up[1] + slope[2] * up[2]);
    if downwards <= length * SKIN_FACING {
        return f32::MAX;
    }
    let from_air = ground / if length > LEAST_SLOPE { length } else { LEAST_SLOPE };
    if from_air < counted { from_air } else { counted }
}

/// The sea at one cell, or a river's or a lake's: the block it is, air for
/// none; how many blocks under its top the cell's middle is; for flat
/// ground of cubes, how many eighths of the cell it fills, a coarser level's
/// top block of it only partly, so every level's sea stands as high;
/// whether it falls there, a waterfall's curtain; and, of cubes, the fill
/// of its whole blocks where it flows along a current, a river's.
#[derive(Clone, Copy)]
pub(super) struct Sea {
    pub block: BlockId,
    pub over: f32,
    pub eighths: Option<f32>,
    pub falling: bool,
    pub current: Option<Fill>,
}

impl Sea {
    /// How the sea of `block`, its top `top` blocks up, stands at block `y`:
    /// of cubes, how many eighths of the block it fills, a coarser level's
    /// top block of it only partly.
    pub fn at(block: BlockId, top: f32, y: i32, smooth: bool) -> Sea {
        let eighths = (!smooth).then(|| ((top - y as f32).min(1.0) * 8.0).round());
        Sea { block, over: top - (y as f32 + 0.5), eighths, falling: false, current: None }
    }
}

/// What one cell is: its block, and its fill where it keeps one; none for
/// air that keeps neither.
///
/// `inside` is how far into the ground its middle is, in blocks, and
/// `ground` the same with the caves left whole, so out of both is open
/// air. `deep` is its depth under the surface, for its layer, and `filled`
/// what its fill is made from in a world drawn `smooth`.
pub(super) fn cell(
    column: &Column,
    inside: f32,
    ground: f32,
    deep: f32,
    filled: f32,
    sea: Sea,
    smooth: bool,
) -> Option<(BlockId, Option<Fill>)> {
    let block = if inside > 0.0 { column.block(deep) } else { AIR };
    let open = ground <= 0.0;
    // The sea fills the open air below its level. Drawn smooth, it also takes the ground's top block where the
    // ground's surface lies under the sea and the block over it stands clear of it, which would else leave no sea
    // there: so the sea's surface reaches the shore, and keeps its floor.
    let shore = smooth && block != AIR && sea.over <= 1.0 && ground < sea.over;
    let wet = match sea.eighths {
        Some(eighths) if !smooth => open && eighths >= 1.0,
        _ => sea.over > 0.0 && (open || shore),
    };
    if wet {
        let fill = match (smooth, sea.eighths) {
            // A waterfall's curtain falls, as water poured over a ledge does.
            (false, _) if sea.falling => Some(Water::Falling.fill()),
            // The sea's top blocks keep where its surface lies.
            (true, _) => Some(surface_fill(sea.over)),
            // A river's whole blocks keep which way it flows.
            (false, Some(eighths)) if eighths >= 8.0 && sea.current.is_some() => sea.current,
            // Kept as flowing water keeps how high it stands; a coarse level's blocks never flow.
            (false, Some(eighths)) if eighths < 8.0 => Some(Water::Flowing { level: eighths as u8, up: 2 }.fill()),
            (false, _) => None,
        };
        Some((sea.block, fill))
    } else if smooth && (block != AIR || filled > -1.0) {
        // The air a block from the surface keeps how far it is from it too.
        Some((block, Some(fill_of(filled, block != AIR))))
    } else if block != AIR {
        Some((block, None))
    } else {
        None
    }
}
