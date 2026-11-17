//! A dungeon in a dimension of its own: its whole world, made of rock.
//!
//! In the land such a dungeon is only its gate (see `plan::plan`): the
//! entrance, a short grand stair and a landing, where a Portal leads in. Its
//! own dimension is a world of rock all round. The dungeon is planned on the
//! rock's flat top as on level land: a sinkhole with the grand stair winding
//! down its wall, the floors under it, as deep as they go. Over the
//! sinkhole's mouth a hall is cut out under a roof of rock, where the Portal
//! back out stands by the head of the stair.

use std::sync::Arc;

use super::{Dungeon, DungeonLayout, build, entrance, plan};
use crate::blocks::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK, Chunk},
    structure::Ground,
};
use crate::generate::WorldGen;

/// The flat rock's top, in blocks: the dimension's level ground.
pub const ROCK_TOP: i32 = 0;
/// How high the hall over the sinkhole is, over the rock's top, and how
/// far round the sinkhole's middle it reaches, in blocks.
const HALL_HIGH: i32 = 10;
const HALL_ROUND: i32 = 20;
/// How much rock is left round everything, in blocks: over the hall's roof
/// and past the dungeon's furthest space.
const ROCK_ROUND: i32 = 6;
/// How far from the sinkhole's middle its Portal stands, in blocks: past
/// the rim, clear of its kerb.
const PORTAL_OUT: f32 = 15.0;
/// How many columns a side the dungeon is built in at once.
const TILE: i32 = 64;

/// A dungeon built in a dimension of its own.
pub struct InRock {
    /// Its world's blocks: the rock, the hall and the dungeon.
    pub blocks: BlockMap,
    pub layout: Arc<DungeonLayout>,
    /// Where the Portal back out stands, in blocks: the block it stands on
    /// is under it.
    pub arrival: [i32; 3],
    /// Which way the Portal faces, towards the sinkhole: its front, the way
    /// one coming out of it walks, as a unit along x and z.
    pub facing: [f32; 2],
}

/// Builds `dungeon` `floors` deep in a world of `rock`, its blocks `size`
/// metres a side. `world` is the land's generator, which it is planned with
/// but never asks: the rock is flat. `turn` turns a block a number of
/// quarters, as a world turns its structures' blocks. None where it cannot be
/// planned.
pub fn in_rock(
    dungeon: &Dungeon,
    world: &WorldGen,
    size: f32,
    (seed, floors): (u32, u32),
    rock: BlockId,
    turn: &dyn Fn(BlockId, u8) -> BlockId,
) -> Option<InRock> {
    let ground = Ground { world, size, position: [0.0, 0.0], biomes: None, halved: false, flat: Some(ROCK_TOP) };
    let layout = Arc::new(plan::plan_in_rock(dungeon, &ground, seed, floors)?);
    let mouth = layout.entrance.at;
    // Rock over everything it holds and round it, whole chunks of it.
    let low = [
        layout.low[0].min(mouth[0] - HALL_ROUND) - ROCK_ROUND,
        layout.low[1] - ROCK_ROUND,
        layout.low[2].min(mouth[2] - HALL_ROUND) - ROCK_ROUND,
    ];
    let high = [
        layout.high[0].max(mouth[0] + HALL_ROUND) + ROCK_ROUND,
        ROCK_TOP + HALL_HIGH + ROCK_ROUND,
        layout.high[2].max(mouth[2] + HALL_ROUND) + ROCK_ROUND,
    ];
    let side = CHUNK as i32;
    let chunk = |at: i32| at.div_euclid(side);
    let mut blocks = BlockMap::default();
    let filled = Chunk::filled(rock);
    for cz in chunk(low[2])..=chunk(high[2]) {
        for cy in chunk(low[1])..=chunk(high[1]) {
            for cx in chunk(low[0])..=chunk(high[0]) {
                blocks.put_chunk([cx, cy, cz], filled.clone());
            }
        }
    }
    // Over the top of the rock round the sinkhole, a hall under a roof of rock, its edge wandering a little.
    for z in mouth[2] - HALL_ROUND..=mouth[2] + HALL_ROUND {
        for x in mouth[0] - HALL_ROUND..=mouth[0] + HALL_ROUND {
            let (dx, dz) = ((x - mouth[0]) as f32 + 0.5, (z - mouth[2]) as f32 + 0.5);
            let edge = HALL_ROUND as f32 - 2.0 + super::noise::unit(x, z, 23, seed) * 2.0;
            let d = (dx * dx + dz * dz).sqrt();
            if d > edge {
                continue;
            }
            // Lower towards its edge, as a cave's roof comes down.
            let high = ROCK_TOP + HALL_HIGH - ((d / edge).powi(2) * 4.0) as i32;
            for y in ROCK_TOP + 1..=high {
                blocks.set([x, y, z], AIR);
            }
        }
    }
    // The dungeon carved and built into it, a tile of columns at a time.
    let (first, last) = ([low[0], low[2]], [high[0], high[2]]);
    let mut z = first[1];
    while z <= last[1] {
        let mut x = first[0];
        while x <= last[0] {
            let columns = ([x, z], [(x + TILE - 1).min(last[0]), (z + TILE - 1).min(last[1])]);
            build(dungeon, &layout, &ground, rock, columns, turn, &mut |at, block| blocks.set(at, block));
            x += TILE;
        }
        z += TILE;
    }
    // A chunk carved all to air is no chunk, as ground made any other way has none: what is drawn walks through
    // the air between chunks by what each kept chunk lets it see.
    let carved: Vec<[i32; 3]> = blocks.chunks().filter(|(_, chunk)| chunk.is_air()).map(|(key, _)| *key).collect();
    for key in carved {
        blocks.put_chunk(key, Chunk::filled(AIR));
    }
    // The Portal back out by the head of the stair down the sinkhole's wall, facing the sinkhole.
    let head = entrance::sinkhole_stair_head(&layout);
    let way = [head.cos(), head.sin()];
    let arrival = [
        (mouth[0] as f32 + 0.5 + way[0] * PORTAL_OUT).floor() as i32,
        ROCK_TOP + 1,
        (mouth[2] as f32 + 0.5 + way[1] * PORTAL_OUT).floor() as i32,
    ];
    // A fire either side of it, on a block of trim, to see the hall by.
    let kit = dungeon.blocks.filled(rock);
    if kit.light != AIR {
        let across = [-way[1], way[0]];
        for side in [-1.0f32, 1.0] {
            let x = (arrival[0] as f32 + 0.5 + across[0] * 3.0 * side).floor() as i32;
            let z = (arrival[2] as f32 + 0.5 + across[1] * 3.0 * side).floor() as i32;
            blocks.set([x, ROCK_TOP + 1, z], kit.trim);
            blocks.set([x, ROCK_TOP + 2, z], kit.light);
        }
    }
    Some(InRock { blocks, layout, arrival, facing: [-way[0], -way[1]] })
}
