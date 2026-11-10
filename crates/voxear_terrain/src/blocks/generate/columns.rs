//! Generated ground a chunk column at a time: every column of blocks'
//! surface and layers from its generator's heights and climate, asked for
//! at once, then each cell down it (see `cells`).

use super::{
    BlockGenerate,
    cells::{cell_at, pushed},
};
use crate::{
    blocks::{
        chunk::{AIR, BlockId, CHUNK, Chunk, fill_of},
        file::BlockFile,
        flow::surface_fill,
    },
    generate::WorldGen,
};

/// One column of the ground: its surface's height in blocks, and the blocks
/// its top layers are, deepest last, as `(block, layers)`.
pub(super) struct Column {
    pub surface: f32,
    pub layers: [(BlockId, f32); 2],
    pub bottom: BlockId,
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
    let seed = world.seed();
    let side = CHUNK as i32;
    let reach = |v: i32, limit: Option<u32>| {
        let end = (v + 1) * side;
        v * side..limit.map_or(end, |limit| end.min(limit.div_ceil(scale as u32) as i32))
    };
    let within = |axis: usize| limit.map(|limit| limit[axis]);
    let places: Vec<(i32, i32)> = reach(cz, within(1)).flat_map(|z| reach(cx, within(0)).map(move |x| (x, z))).collect();
    if places.is_empty() {
        return Vec::new();
    }
    // Every column's ground and climate, asked of the generator at once.
    let middle = |v: i32, axis: usize| generate.position[axis] + (v as f32 + 0.5) * size;
    let (xs, zs): (Vec<f32>, Vec<f32>) = places.iter().map(|&(x, z)| (middle(x, 0), middle(z, 1))).unzip();
    let (mut heights, mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.heights(&xs, &zs, &mut heights);
    world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
    // Each column's surface and layers, worked out once.
    let column = |height: f32, climate: (f32, f32)| -> Column {
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
    let columns: Vec<((i32, i32), Column)> =
        places.iter().enumerate().map(|(at, &place)| (place, column(heights[at], (warmth[at], wetness[at])))).collect();
    let push = generate.overhangs.max(0.0) / size;
    let coarse = BlockGenerate { caves: if level == 0 { generate.caves } else { 0.0 }, ..generate.clone() };
    // The sea's top, in this level's blocks, where it has a sea.
    let sea = known(generate.sea);
    let sea_level = if sea == AIR { f32::MIN } else { world.sea_level() / size };
    let highest = columns.iter().map(|(_, c)| c.surface).fold(f32::MIN, f32::max).max(sea_level) + push + 1.0;
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
                let middle = [*x as f32 + 0.5, y as f32 + 0.5, *z as f32 + 0.5];
                let under = column.surface - middle[1];
                let place = middle.map(|v| v * size);
                let (block, depth, open) = cell_at(column, under, place, size, &coarse, push, seed, f32::INFINITY);
                // The sea fills the open air below its level. Drawn smooth, it also takes the ground's top block
                // where the ground's surface lies under the sea and the block over it stands clear of it, which
                // would else leave no sea there: so the sea's surface reaches the shore, and keeps its floor.
                let over = sea_level - middle[1];
                let shore = file.smooth && block != AIR && over <= 1.0 && pushed(under, place, push, seed) < over;
                let block = if over > 0.0 && (open || shore) { sea } else { block };
                let (lx, lz) = ((x - cx * side) as u32, (z - cz * side) as u32);
                // Drawn smooth, the air a block from the surface keeps how far it is from it too, and the sea's top
                // blocks where its surface lies.
                let is_sea = sea != AIR && block == sea;
                if file.smooth && is_sea {
                    chunk.set_cell(lx, ly as u32, lz, sea, surface_fill(over));
                    any = true;
                } else if file.smooth && (block != AIR || depth > -1.0) {
                    chunk.set_cell(lx, ly as u32, lz, block, fill_of(depth, block != AIR));
                    any = true;
                } else if block != AIR {
                    chunk.set(lx, ly as u32, lz, block);
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
