//! Generated ground a chunk column at a time: every column of blocks'
//! surface and layers from its generator's heights and climate, asked for
//! at once, then how far into the ground the generator's Volume puts each
//! cell up it (see `cells`).
//!
//! Where the Volume is the ground's height made solid and nothing more, a
//! block's depth under the surface is how far under its column's height it
//! is, and that is its layer and its fill. Where the ground is pushed about
//! the number is no depth, so layers are counted down each column from the
//! open air, through the chunks above and past caves. A world drawn smooth
//! then also measures each fill against the blocks round it, and lays its
//! top layer along slopes as a skin (`cells::skinned`), a ring of columns
//! round the chunk's own made for both.

use super::{
    BlockGenerate,
    cells::{Sea, cell, counted, distance, skinned, source},
};
use crate::{
    blocks::{
        chunk::{AIR, BlockId, CHUNK, Chunk},
        file::BlockFile,
    },
    generate::Cells,
};

/// One column of the ground: its surface's height in blocks, and the blocks
/// its top layers are, deepest last, as `(block, layers)`.
pub(super) struct Column {
    pub surface: f32,
    pub layers: [(BlockId, f32); 2],
    pub bottom: BlockId,
}

impl Column {
    /// The block of its layer `depth` blocks under its surface.
    pub fn block(&self, depth: f32) -> BlockId {
        let [(top, top_depth), (under, under_depth)] = self.layers;
        if depth <= top_depth {
            top
        } else if depth <= top_depth + under_depth {
            under
        } else {
            self.bottom
        }
    }
}

/// How many blocks further into or out of the ground than a block's middle
/// its fill is no longer measured against the blocks round it: a whole
/// block or none, on any slope a block can show.
const MEASURED: f32 = 8.0;

/// The chunks of chunk column `[cx, cz]` of the ground `generate` makes for
/// `file` with generator `world`, at level `level` of coarseness: each level
/// blocks twice as large as the one before, counted in its own blocks and
/// chunks, for far views; its columns within `limit` blocks along x and z
/// from the world's corner where it has one. Coarser levels leave out caves.
pub fn column_chunks(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &crate::generate::WorldGen,
    level: u32,
    [cx, cz]: [i32; 2],
    limit: Option<[u32; 2]>,
) -> Vec<([i32; 3], Chunk)> {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let ground = &file.ground;
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    let side = CHUNK as i32;
    let reach = |v: i32, limit: Option<u32>| {
        let end = (v + 1) * side;
        v * side..limit.map_or(end, |limit| end.min(limit.div_ceil(scale as u32) as i32))
    };
    let within = |axis: usize| limit.map(|limit| limit[axis]);
    let (across, along) = (reach(cx, within(0)), reach(cz, within(1)));
    if across.is_empty() || along.is_empty() {
        return Vec::new();
    }
    let source = source(generate, world, file.block_size.max(0.01));
    let world = source.world();
    let Some(volume) = world.volume() else { return Vec::new() };
    let plain = volume.plain();
    // Fills measured against the blocks round them read a ring of columns round the chunk's own, and a block
    // under and over.
    let ring = i32::from(file.smooth && !plain);
    let (first_x, first_z) = (across.start - ring, along.start - ring);
    let wide = (across.end - first_x + ring) as usize;
    let deep = (along.end - first_z + ring) as usize;
    let places: Vec<(i32, i32)> =
        (0..deep as i32).flat_map(|z| (0..wide as i32).map(move |x| (first_x + x, first_z + z))).collect();
    let own = |(x, z): (i32, i32)| across.contains(&x) && along.contains(&z);
    // Every column's ground and climate, and what the Volume reads of it, asked of the generator at once.
    let middle = |v: i32, axis: usize| generate.position[axis] + (v as f32 + 0.5) * size;
    let (xs, zs): (Vec<f32>, Vec<f32>) = places.iter().map(|&(x, z)| (middle(x, 0), middle(z, 1))).unzip();
    let (mut heights, mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.heights(&xs, &zs, &mut heights);
    world.climates(&xs, &zs, &heights, &mut warmth, &mut wetness);
    // A plain height with no caves in it, or with them left out of a coarse level, needs nothing asked: every
    // block is as deep as its column's height says.
    let asked = !plain || (volume.caved() && level == 0);
    let carried = match asked {
        true => volume.carried_grid([first_x, first_z], [wide, deep], |axis, v| middle(v, axis), &heights),
        false => Default::default(),
    };
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
    let columns: Vec<Column> = (0..places.len()).map(|at| column(heights[at], (warmth[at], wetness[at]))).collect();
    // How far over a column's surface its ground may reach, in blocks.
    let over_reach = volume.reach() / size;
    // The sea's top, in this level's blocks, where it has a sea, as high at every level, so none shows a step where
    // its rings meet: drawn smooth, its sea level, which its top blocks keep; of cubes, where the finest level's
    // blocks put it.
    let sea = known(generate.sea);
    let finest = (world.sea_level() / file.block_size.max(0.01) - 0.5).ceil();
    let sea_top = match (sea == AIR, file.smooth) {
        (true, _) => f32::MIN,
        (false, true) => world.sea_level() / size,
        (false, false) => finest / scale,
    };
    // How the sea stands at block `y`: of cubes, how many eighths of the block it fills, a coarser level's top
    // block of it only partly.
    let sea_at = |y: i32| Sea {
        block: sea,
        over: sea_top - (y as f32 + 0.5),
        eighths: (!file.smooth).then(|| ((sea_top - y as f32).min(1.0) * 8.0).round()),
    };
    let surfaces = places.iter().zip(&columns).filter(|(place, _)| own(**place)).map(|(_, column)| column.surface);
    let highest = surfaces.fold(f32::MIN, f32::max).max(sea_top) + over_reach + 1.0;
    let low = generate.bottom.div_euclid(scale as i32);
    let (low_key, high_key) = (low.div_euclid(side), (highest.ceil() as i32).div_euclid(side));
    if high_key < low_key {
        return Vec::new();
    }
    // The highest block made, and the lowest and highest read.
    let top = (high_key + 1) * side - 1;
    let (first_y, last_y) = (low - ring, top + ring);
    let tall = (last_y - first_y + 1) as usize;
    let ys: Vec<f32> = (first_y..=last_y).map(|y| (y as f32 + 0.5) * size).collect();
    // How high each column is read: past where its ground can reach a block. Measured against the blocks round
    // them, as high as any column two away is, so a block near a cliff has those beside it read too.
    let reaches: Vec<i32> =
        columns.iter().map(|column| ((column.surface + over_reach + 2.0).floor() as i32).clamp(low, top)).collect();
    let read_to = |at: usize| -> i32 {
        if ring == 0 {
            return reaches[at];
        }
        let (x, z) = (at % wide, at / wide);
        let near = |v: usize, most: usize| v.saturating_sub(2)..(v + 3).min(most);
        let about = near(z, deep).flat_map(|z| near(x, wide).map(move |x| z * wide + x));
        about.map(|at| reaches[at]).fold(i32::MIN, i32::max) + 1
    };
    // How far into the ground the Volume puts each cell read, in blocks, a column after another from the lowest
    // block up, and the same with the caves left whole. Kept for every column only where fills are measured.
    let kept = if ring == 0 { 1 } else { places.len() };
    let (mut inside, mut under) = (vec![0.0f32; kept * tall], vec![0.0f32; kept * tall]);
    let (mut row, mut ground_row) = (vec![0.0f32; tall], vec![0.0f32; tall]);
    let mut read = vec![0usize; places.len()];
    let mut made: Vec<(i32, (BlockId, Option<u8>))> = Vec::new();
    let mut chunks: Vec<Option<Chunk>> = vec![None; (high_key - low_key + 1) as usize];
    let mut put = |x: i32, y: i32, z: i32, block: BlockId, fill: Option<u8>| {
        let chunk = chunks[(y.div_euclid(side) - low_key) as usize].get_or_insert_with(|| Chunk::filled(AIR));
        let (lx, ly, lz) = ((x - cx * side) as u32, y.rem_euclid(side) as u32, (z - cz * side) as u32);
        match fill {
            Some(fill) => chunk.set_cell(lx, ly, lz, block, fill),
            None => chunk.set(lx, ly, lz, block),
        }
    };
    let read_column = |at: usize, inside: &mut [f32], under: &mut [f32], row: &mut [f32], ground_row: &mut [f32]| {
        let (x, z) = places[at];
        let count = (read_to(at) - first_y + 1) as usize;
        // The generator's own Volume is read where the world lies in the generator's world; the world's own
        // overhangs and caves from the world's corner, as they always were.
        let (px, pz) = match source.own() {
            true => (xs[at], zs[at]),
            false => ((x as f32 + 0.5) * size, (z as f32 + 0.5) * size),
        };
        if asked {
            let cells = Cells { x: &[px], y: &ys[..count], z: &[pz], carried: &carried.column(at), size };
            volume.densities(cells, level > 0, &mut row[..count], &mut ground_row[..count]);
        }
        for k in 0..count {
            let y = first_y + k as i32;
            // Under a plain height a block's depth is counted from its column's surface in blocks, exactly, and
            // a block the caves leave alone is as deep as that.
            let plainly = columns[at].surface - (y as f32 + 0.5);
            under[k] = if plain { plainly } else { ground_row[k] / size };
            inside[k] = if !asked || plain && row[k] == ground_row[k] { plainly } else { row[k] / size };
        }
        // What is over what was read is as far out as the last of it.
        let (last_inside, last_under) = (inside[count - 1], under[count - 1]);
        inside[count..].fill(last_inside);
        under[count..].fill(last_under);
        count
    };
    if ring > 0 {
        for (at, read) in read.iter_mut().enumerate() {
            let from = at * tall;
            let (inside, under) = (&mut inside[from..from + tall], &mut under[from..from + tall]);
            *read = read_column(at, inside, under, &mut row, &mut ground_row);
        }
    }
    for at in (0..places.len()).filter(|&at| own(places[at])) {
        let ((x, z), column) = (places[at], &columns[at]);
        let from = if ring == 0 { 0 } else { at * tall };
        if ring == 0 {
            read[at] = read_column(at, &mut inside[..tall], &mut under[..tall], &mut row, &mut ground_row);
        }
        // Down the column from the highest block made, for the count of its layers: open air first, where only
        // the sea is.
        made.clear();
        let mut above = 0.0;
        for y in (low..=top).rev() {
            let k = (y - first_y) as usize;
            if k >= read[at] {
                made.extend(cell(column, -2.0, -2.0, 0.0, -2.0, sea_at(y), file.smooth).map(|cell| (y, cell)));
                continue;
            }
            let (within, ground) = (inside[from + k], under[from + k]);
            let near = ring > 0 && within.abs() < MEASURED;
            let depth = match (plain, near && ground > 0.0) {
                (true, _) => ground,
                (false, false) => counted(above, ground),
                (false, true) => {
                    let beside = |step: usize| [under[from + k - step], under[from + k + step]];
                    let beside = [beside(tall), beside(1), beside(wide * tall)];
                    skinned(counted(above, ground), ground, beside, [0.0, 1.0, 0.0])
                }
            };
            above = depth;
            let filled = if near {
                let beside = |step: usize| [inside[from + k - step], inside[from + k + step]];
                distance(within, [beside(tall), beside(1), beside(wide * tall)])
            } else {
                within
            };
            made.extend(cell(column, within, ground, depth, filled, sea_at(y), file.smooth).map(|cell| (y, cell)));
        }
        // Kept from the lowest up, as chunks' blocks are.
        for &(y, (block, fill)) in made.iter().rev() {
            put(x, y, z, block, fill);
        }
    }
    (low_key..).zip(chunks).filter_map(|(cy, chunk)| Some(([cx, cy, cz], chunk?))).collect()
}
