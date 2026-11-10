//! Ground of blocks made by a world generator (see `crate::generate`): its
//! heights a column at a time, the same mountains, coasts and climate a
//! terrain's Generate layer lays, pushed in and out by noise in three
//! dimensions so cliffs lean and overhang, and hollowed below by caves: long
//! tunnels where two noises are both near nothing, and chambers where a
//! third is high.
//!
//! Each column takes its world's ground layers (`file::Ground`'s top, under
//! and bottom blocks), but where a kind of block's cover takes its climate
//! and height (`file::BlockCover`), that kind lies on top instead. Every
//! chunk is made apart, on every core, from where its blocks are alone, so
//! the same ground comes from the same file anywhere.

use serde::{Deserialize, Serialize};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK, Chunk},
    file::BlockFile,
};
use crate::{generate::WorldGen, noise::gradient3};

/// How a world of blocks' ground is generated: by the world generator
/// `world` names (a reference to a `.worldgen`), from `position` metres in
/// its world, down to block `bottom` (counted from the world's corner, so
/// negative goes below it), its surface pushed in and out by as many metres
/// as `overhangs`, and as much of it below hollowed out as `caves` says,
/// from 0 for none to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockGenerate {
    pub world: Option<String>,
    pub position: [f32; 2],
    pub bottom: i32,
    pub overhangs: f32,
    pub caves: f32,
    /// For a world without end, made round the camera as it goes: how far
    /// (see `stream`). Its ground's size is then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endless: Option<BlockEndless>,
}

impl Default for BlockGenerate {
    fn default() -> Self {
        Self { world: None, position: [0.0, 0.0], bottom: -16, overhangs: 0.0, caves: 0.3, endless: None }
    }
}

/// How far round the camera a world of blocks without end is made: its own
/// chunks out to `distance` metres, and coarser ones drawn out to `far`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockEndless {
    pub distance: f32,
    pub far: f32,
}

impl Default for BlockEndless {
    fn default() -> Self {
        Self { distance: 128.0, far: 1024.0 }
    }
}

/// How far apart the bumps of the ground's push in and out are, in metres.
const OVERHANG_SIZE: f32 = 14.0;

/// How wide caves' tunnels wind, and how far apart their chambers are, in
/// metres.
const TUNNEL_SIZE: f32 = 22.0;
const CHAMBER_SIZE: f32 = 40.0;

/// How many blocks under the surface caves keep clear of, so they open onto
/// it only here and there.
const CAVE_ROOF: f32 = 3.0;

/// One column of the ground: its surface's height in blocks, and the blocks
/// its top layers are, deepest last, as `(block, layers)`.
struct Column {
    surface: f32,
    layers: [(BlockId, f32); 2],
    bottom: BlockId,
}

/// The ground `generate` makes for `file` with generator `world`: its
/// ground's size of chunk columns, each on every core.
pub fn generate(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen) -> BlockMap {
    let ground = &file.ground;
    let columns_x = ground.size[0].div_ceil(CHUNK) as i32;
    let columns_z = ground.size[1].div_ceil(CHUNK) as i32;
    let keys: Vec<[i32; 2]> = (0..columns_z).flat_map(|z| (0..columns_x).map(move |x| [x, z])).collect();
    let made = crate::workers::each(&keys, |&key| column_chunks(file, generate, world, 0, key, Some(ground.size)));
    let mut map = BlockMap::default();
    for (key, chunk) in made.into_iter().flatten() {
        map.put_chunk(key, chunk);
    }
    map
}

/// The chunks of chunk column `[cx, cz]` of the ground `generate` makes for
/// `file` with generator `world`, at level `level` of coarseness: each level
/// blocks twice as large as the one before, counted in its own blocks and
/// chunks, for far views; its columns within `limit` blocks along x and z
/// from the world's corner where it has one. Coarser levels leave out caves.
pub fn column_chunks(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &WorldGen,
    level: u32,
    [cx, cz]: [i32; 2],
    limit: Option<[u32; 2]>,
) -> Vec<([i32; 3], Chunk)> {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let ground = &file.ground;
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    let seed = world.seed;
    // Each column's surface and layers, worked out once.
    let column = |x: i32, z: i32| -> Column {
        let (px, pz) = (generate.position[0] + (x as f32 + 0.5) * size, generate.position[1] + (z as f32 + 0.5) * size);
        let height = world.height_at(px, pz);
        let climate = world.climate(px, pz, height);
        let cover = file.blocks.iter().enumerate().rev().find_map(|(at, kind)| {
            kind.cover.filter(|c| c.covers(height, climate)).map(|c| ((at + 1) as BlockId, c.depth))
        });
        // Layers as many metres deep at every level, at least a block.
        let top = match cover {
            Some((block, depth)) => (block, (depth.max(1) as f32 / scale).max(1.0)),
            None => (known(ground.top), 1.0),
        };
        Column {
            surface: height / size,
            layers: [top, (known(ground.under), ground.under_depth as f32 / scale)],
            bottom: known(ground.bottom),
        }
    };
    let side = CHUNK as i32;
    let reach = |v: i32, limit: Option<u32>| {
        let end = (v + 1) * side;
        v * side..limit.map_or(end, |limit| end.min(limit.div_ceil(scale as u32) as i32))
    };
    let mut columns = Vec::with_capacity((side * side) as usize);
    for z in reach(cz, limit.map(|l| l[1])) {
        for x in reach(cx, limit.map(|l| l[0])) {
            columns.push(((x, z), column(x, z)));
        }
    }
    if columns.is_empty() {
        return Vec::new();
    }
    let push = generate.overhangs.max(0.0) / size;
    let coarse = BlockGenerate { caves: if level == 0 { generate.caves } else { 0.0 }, ..generate.clone() };
    let highest = columns.iter().map(|(_, c)| c.surface).fold(f32::MIN, f32::max) + push + 1.0;
    let low = generate.bottom.div_euclid(scale as i32);
    let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
    let (low_key, high_key) = (low.div_euclid(side), (highest.ceil() as i32).div_euclid(side));
    for cy in low_key..=high_key {
        let mut chunk = Chunk::filled(AIR);
        let mut any = false;
        for ((x, z), column) in &columns {
            for ly in 0..side {
                let y = cy * side + ly;
                if y < low {
                    continue;
                }
                let block = block_at(column, [*x, y, *z], size, &coarse, push, seed);
                if block != AIR {
                    chunk.set((x - cx * side) as u32, ly as u32, (z - cz * side) as u32, block);
                    any = true;
                }
            }
        }
        if any {
            chunks.push(([cx, cy, cz], chunk));
        }
    }
    chunks
}

/// The block at `at` in its column: solid below its surface pushed in and
/// out, hollowed by caves, of the column's layer that deep.
fn block_at(column: &Column, at: [i32; 3], size: f32, generate: &BlockGenerate, push: f32, seed: u32) -> BlockId {
    let [x, y, z] = at;
    let middle = y as f32 + 0.5;
    let place = [x as f32 + 0.5, middle, z as f32 + 0.5].map(|v| v * size);
    let mut depth = column.surface - middle;
    if push > 0.0 && depth.abs() < push + 1.0 {
        let s = OVERHANG_SIZE;
        depth += gradient3(place[0] / s, place[1] / s, place[2] / s, seed.wrapping_add(41)) * push;
    }
    if depth <= 0.0 {
        return AIR;
    }
    // Caves: tunnels where two noises are both near nothing, chambers where a third is high.
    if generate.caves > 0.0 && depth > CAVE_ROOF {
        let caves = generate.caves.clamp(0.0, 1.0);
        let t = TUNNEL_SIZE;
        let tunnel = gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(53)).abs()
            + gradient3(place[0] / t, place[1] / t, place[2] / t, seed.wrapping_add(67)).abs();
        let c = CHAMBER_SIZE;
        let chamber = gradient3(place[0] / c, place[1] / (c * 0.6), place[2] / c, seed.wrapping_add(79));
        if tunnel < 0.14 * caves.sqrt() || chamber > 0.62 - 0.22 * caves {
            return AIR;
        }
    }
    let [(top, top_depth), (under, under_depth)] = column.layers;
    if depth <= top_depth {
        top
    } else if depth <= top_depth + under_depth {
        under
    } else {
        column.bottom
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::file::{BlockCover, BlockKind, Ground},
        generate::Preset,
        surfaces::Biome,
    };

    fn generated(caves: f32, overhangs: f32) -> (BlockFile, WorldGen) {
        // Mountains 300 m high in a cold world, so some of it is snowed.
        let mut world = Preset::Alpine.world();
        world.height = 300.0;
        world.climate.temperature = 0.0;
        let mut file = BlockFile {
            block_size: 2.0,
            ground: Ground { size: [64, 64], ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("alpine".into()),
                position: [500.0, 800.0],
                bottom: -40,
                overhangs,
                caves,
                endless: None,
            }),
            ..BlockFile::default()
        };
        // Snow where it is below freezing.
        let (temperature, humidity) = Biome::Snow.ranges();
        file.blocks.push(BlockKind {
            cover: Some(BlockCover { temperature, humidity, height: None, depth: 2 }),
            ..BlockKind::colored("Snow", [0.95; 3])
        });
        (file, world)
    }

    #[test]
    fn generated_ground_follows_its_generator_snowed_up_high_and_hollowed_by_caves() {
        let (file, world) = generated(0.0, 0.0);
        let map = file.make_ground(Some(&world));
        // Each column's top block lies at its generator's height there, in 2 m blocks.
        let surface = |map: &BlockMap, x: i32, z: i32| (-40..400).rev().find(|&y| map.get([x, y, z]) != AIR);
        let mut snowed = 0;
        for (x, z) in [(3, 5), (30, 40), (60, 12), (17, 63)] {
            let (px, pz) = (500.0 + (x as f32 + 0.5) * 2.0, 800.0 + (z as f32 + 0.5) * 2.0);
            let height = world.height_at(px, pz);
            let top = surface(&map, x, z).expect("ground in every column");
            assert!(((top + 1) as f32 * 2.0 - height).abs() <= 2.0, "column ({x}, {z}): top {top} for {height} m");
            let cold = world.climate(px, pz, height).0 < -2.0;
            let block = map.get([x, top, z]);
            assert_eq!(block == 4, cold, "snow where it is below freezing: {block} at {height} m");
            snowed += usize::from(cold);
            assert_eq!(map.get([x, -40, z]), 3, "stone at the bottom");
            assert_eq!(map.get([x, -41, z]), AIR, "nothing below it");
        }
        let _ = snowed;
        // The same again from the same file; and with caves, ground hollowed out below.
        assert_eq!(file.make_ground(Some(&world)), map);
        let (caved, _) = generated(0.8, 0.0);
        let hollow = caved.make_ground(Some(&world));
        let solid = |map: &BlockMap| {
            map.chunks()
                .map(|(_, c)| {
                    (0..32)
                        .flat_map(|y| (0..32).flat_map(move |z| (0..32).map(move |x| (x, y, z))))
                        .filter(|&(x, y, z)| c.get(x, y, z) != AIR)
                        .count()
                })
                .sum::<usize>()
        };
        let (full, hollowed) = (solid(&map), solid(&hollow));
        assert!(hollowed < full * 97 / 100 && hollowed > full * 50 / 100, "{hollowed} of {full} blocks left by caves");
        // With overhangs, the surface pushed in and out: some blocks with air under them above ground.
        let (leaning, _) = generated(0.0, 10.0);
        let pushed = leaning.make_ground(Some(&world));
        let overhanging = (0..64).flat_map(|z| (0..64).map(move |x| (x, z))).any(|(x, z)| {
            let top = surface(&pushed, x, z).unwrap_or(-40);
            (top - 6..top).any(|y| pushed.get([x, y, z]) == AIR)
        });
        assert!(overhanging, "air under the ground somewhere above the caves' reach");
    }
}
