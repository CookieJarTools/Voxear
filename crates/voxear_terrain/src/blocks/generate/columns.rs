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

/// One column of the ground: its surface's height in blocks, the blocks
/// its top layers are, deepest last, as `(block, layers)`, and the vines its
/// biome hangs in it.
pub(super) struct Column {
    pub surface: f32,
    pub layers: [(BlockId, f32); 2],
    pub bottom: BlockId,
    pub vines: Vines,
}

/// The vines of a column's biome: their block (air for none), how many of
/// the places one could hang from have one, how long they hang in blocks,
/// and how many metres under the surface they still hang.
#[derive(Clone, Copy, Default)]
pub(super) struct Vines {
    pub block: BlockId,
    pub amount: f32,
    pub length: [u32; 2],
    pub depth: f32,
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

/// Each column's surface and layers at level `level`, from the generator's
/// `heights` and climate at `xs` and `zs`, in metres in its world: its top
/// layer the last kind of block whose cover takes its climate and height,
/// as many metres deep at every level, at least a block.
pub(super) fn columns_at(
    file: &BlockFile,
    world: &crate::generate::WorldGen,
    level: u32,
    (xs, zs, heights): (&[f32], &[f32], &[f32]),
) -> Vec<Column> {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let ground = &file.ground;
    let known = |block: BlockId| if usize::from(block) <= file.blocks.len() { block } else { AIR };
    let (mut warmth, mut wetness) = (vec![0.0; xs.len()], vec![0.0; xs.len()]);
    world.climates(xs, zs, heights, &mut warmth, &mut wetness);
    // How steeply the ground rises at each column, where a biome has blocks for steep ground: from its height a
    // block further along x and along z.
    let placed = file.generate.as_ref().and_then(|generate| generate.placed.as_deref());
    let placed = placed.filter(|placed| !placed.biomes.is_empty());
    let slopes: Vec<f32> = match placed.filter(|placed| placed.any_steep()) {
        Some(_) => {
            let beside = |shift: [f32; 2]| {
                let (xs, zs): (Vec<f32>, Vec<f32>) =
                    xs.iter().zip(zs).map(|(x, z)| (x + shift[0], z + shift[1])).unzip();
                let mut heights = vec![0.0; xs.len()];
                world.heights(&xs, &zs, &mut heights);
                heights
            };
            let (along_x, along_z) = (beside([size, 0.0]), beside([0.0, size]));
            (0..heights.len())
                .map(|at| {
                    let (dx, dz) = ((along_x[at] - heights[at]) / size, (along_z[at] - heights[at]) / size);
                    (dx * dx + dz * dz).sqrt()
                })
                .collect()
        }
        None => Vec::new(),
    };
    let sea_level = world.sea_level();
    // Where the generator says the ground is bare rock, a column is its bottom block to the top.
    let rock = world.rock(xs, zs, heights);
    let bare = |at: usize| rock.as_ref().is_some_and(|rock| rock[at]);
    // How far the ground is sunk under the land round about, where the generator says and a biome asks.
    let sunk = placed.filter(|placed| placed.any_sunk()).and_then(|_| world.sunk(xs, zs, heights));
    let column = |at: usize| -> Column {
        let (height, climate) = (heights[at], (warmth[at], wetness[at]));
        let cover = file.blocks.iter().enumerate().rev().find_map(|(at, kind)| {
            kind.cover.filter(|c| c.covers(height, climate)).map(|c| ((at + 1) as BlockId, c.depth))
        });
        // The blocks of the biome the place is, each left to the world's own where the biome names none.
        let above_sea = height - sea_level;
        let sunk = sunk.as_ref().map_or(0.0, |sunk| sunk[at]);
        let place = crate::generate::biomes::Place { temperature: climate.0, humidity: climate.1, above_sea, sunk };
        let biome = placed.and_then(|placed| placed.at(xs[at], zs[at], place));
        let (biome_top, biome_under) =
            biome.map_or((AIR, AIR), |blocks| blocks.at(above_sea, slopes.get(at).copied().unwrap_or(0.0)));
        let or = |block: BlockId, own: BlockId| if known(block) == AIR { known(own) } else { block };
        let top = match cover {
            Some((block, depth)) => (block, (depth.max(1) as f32 / scale).max(1.0)),
            None => (or(biome_top, ground.top), 1.0),
        };
        let bottom = or(biome.map_or(AIR, |blocks| blocks.deep), ground.bottom);
        let vines = biome.filter(|blocks| blocks.vine != AIR).map_or(Vines::default(), |blocks| Vines {
            block: blocks.vine,
            amount: blocks.vine_amount,
            length: blocks.vine_length,
            depth: blocks.vine_depth,
        });
        if bare(at) {
            return Column { surface: height / size, layers: [(bottom, 1.0), (bottom, 0.0)], bottom, vines };
        }
        let under_depth = biome.filter(|blocks| blocks.under != AIR).map_or(ground.under_depth, |b| b.under_depth);
        Column {
            surface: height / size,
            layers: [top, (or(biome_under, ground.under), under_depth as f32 / scale)],
            bottom,
            vines,
        }
    };
    (0..heights.len()).map(column).collect()
}

/// The sea's block, air for none, and its top in level `level`'s blocks, as
/// high at every level, so none shows a step where its rings meet: drawn
/// smooth, its sea level, which its top blocks keep; of cubes, where the
/// finest level's blocks put it.
pub(super) fn sea_of(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &crate::generate::WorldGen,
    level: u32,
) -> (BlockId, f32) {
    let scale = (1u32 << level.min(16)) as f32;
    let size = file.block_size.max(0.01) * scale;
    let sea = if usize::from(generate.sea) <= file.blocks.len() { generate.sea } else { AIR };
    let finest = (world.sea_level() / file.block_size.max(0.01) - 0.5).ceil();
    let top = match (sea == AIR, file.smooth) {
        (true, _) => f32::MIN,
        (false, true) => world.sea_level() / size,
        (false, false) => finest / scale,
    };
    (sea, top)
}

/// Each column's own water at level `level`, where the generator has
/// rivers and lakes and they stand over the sea: its top in the level's
/// blocks, as `sea_of` puts the sea's, and the block from which it falls
/// as a waterfall's curtain (none falling at `f32::MAX`). None where the
/// world has no sea's block to fill them with, or the generator no water.
pub(super) fn waters_of(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &crate::generate::WorldGen,
    level: u32,
    (xs, zs): (&[f32], &[f32]),
) -> Option<Vec<(f32, f32)>> {
    waters_and_currents(file, generate, world, level, (xs, zs), false).map(|(waters, _)| waters)
}

/// Each column's own water as [`waters_of`] gives it, and, with `currents`,
/// which way it flows where it flows along a river's current: of cubes, at
/// the finest level, where a river's top then lies as high as the river's
/// water, its top block only partly full (see `flow::current_fill`).
pub(super) fn waters_and_currents(
    file: &BlockFile,
    generate: &BlockGenerate,
    world: &crate::generate::WorldGen,
    level: u32,
    (xs, zs): (&[f32], &[f32]),
    currents: bool,
) -> Option<(Vec<(f32, f32)>, Vec<Option<[f32; 2]>>)> {
    let (sea, sea_top) = sea_of(file, generate, world, level);
    if sea == AIR {
        return None;
    }
    let (water, falls) = world.water(xs, zs)?;
    let scale = (1u32 << level.min(16)) as f32;
    let (finest, size) = (file.block_size.max(0.01), file.block_size.max(0.01) * scale);
    // In blocks of the level, kept where the finest level's blocks put it, as the sea is.
    let blocks = |metres: f32| match file.smooth {
        true => metres / size,
        false => (metres / finest - 0.5).ceil() / scale,
    };
    let none = (f32::MIN, f32::MAX);
    let mut made = (0..water.len())
        .map(|at| match water[at] > crate::generate::nodes::NO_WATER * 0.5 {
            true if blocks(water[at]) > sea_top => {
                let from = if falls[at] > 0.0 { blocks(water[at] - falls[at]) } else { f32::MAX };
                (blocks(water[at]), from)
            }
            _ => none,
        })
        .collect::<Vec<_>>();
    if made.iter().all(|&water| water == none) {
        return None;
    }
    let flows = |at: usize| {
        let wet = made[at] != none && made[at].1 == f32::MAX;
        world.current(xs[at], zs[at]).filter(|&[x, z]| wet && x * x + z * z > 1e-4)
    };
    let currents: Vec<Option<[f32; 2]>> = match currents && !file.smooth && level == 0 {
        true => (0..made.len()).map(flows).collect(),
        false => vec![None; made.len()],
    };
    // A river's top as high as its water, in eighths of a block, as water placed and running out steps down.
    for (at, current) in currents.iter().enumerate() {
        if current.is_some() {
            made[at].0 = water[at] / finest;
        }
    }
    Some((made, currents))
}

/// The most blocks a piece of ground may hold, standing loose in the air
/// with nothing under or beside it, and be cleared away: more is ground
/// standing apart on purpose, an island in the sky or a slab of rock.
const LOOSE: usize = 256;

/// Clears every piece of ground in a chunk column's `chunks` that stands
/// loose in the air: a few blocks touching no others, as a cave or a
/// passage cut under ground leaning out over it may leave hanging. A piece
/// reaching under the column's lowest block may be held up there, and
/// stays. One reaching past its sides is followed on into the columns
/// beside it, `beyond` saying whether each block there is ground, by where
/// it is from this column's corner and its lowest chunk's foot: so each
/// column finds the same piece and clears its own part of it alike. Water
/// (`water`) holds nothing up.
fn clear_loose(chunks: &mut [([i32; 3], Chunk)], water: BlockId, beyond: &dyn Fn([i32; 3]) -> bool) {
    let side = CHUNK as i32;
    let (Some(low), Some(high)) = (chunks.iter().map(|(at, _)| at[1]).min(), chunks.iter().map(|(at, _)| at[1]).max())
    else {
        return;
    };
    let tall = ((high - low + 1) * side) as usize;
    let index = |[x, y, z]: [i32; 3]| (y as usize * CHUNK as usize + z as usize) * CHUNK as usize + x as usize;
    // Which blocks are ground, the column's from its lowest chunk up.
    let mut ground = vec![false; tall * (CHUNK * CHUNK) as usize];
    for (at, chunk) in chunks.iter() {
        if chunk.is_air() {
            continue;
        }
        let from = (at[1] - low) * side;
        for y in 0..CHUNK {
            for z in 0..CHUNK {
                for x in 0..CHUNK {
                    let block = chunk.get(x, y, z);
                    ground[index([x as i32, from + y as i32, z as i32])] = block != AIR && block != water;
                }
            }
        }
    }
    let own = |[x, _, z]: [i32; 3]| (0..side).contains(&x) && (0..side).contains(&z);
    const WAYS: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];
    let step = |p: [i32; 3], w: [i32; 3]| [p[0] + w[0], p[1] + w[1], p[2] + w[2]];
    let within = |p: [i32; 3]| own(p) && (0..tall as i32).contains(&p[1]);
    // First, all ground joined within the column to its foot, the lowest blocks of ground in it, is held up: nearly
    // all of it, asked about nowhere else.
    let mut held = vec![false; ground.len()];
    let mut edge: Vec<[i32; 3]> = Vec::new();
    let row = (CHUNK * CHUNK) as usize;
    let foot = (0..tall).find(|&y| ground[y * row..(y + 1) * row].iter().any(|&solid| solid)).unwrap_or(0) as i32;
    for z in 0..side {
        for x in 0..side {
            if ground[index([x, foot, z])] {
                held[index([x, foot, z])] = true;
                edge.push([x, foot, z]);
            }
        }
    }
    while let Some(p) = edge.pop() {
        for w in WAYS {
            let q = step(p, w);
            if within(q) && ground[index(q)] && !held[index(q)] {
                held[index(q)] = true;
                edge.push(q);
            }
        }
    }
    // Then each piece left, joined within the column: one larger than LOOSE stands apart on purpose; a smaller one
    // reaching nowhere past the column's sides is loose; one reaching past them is followed on there.
    let mut loose = vec![false; ground.len()];
    let mut seen = vec![false; ground.len()];
    let asked = std::cell::RefCell::new(std::collections::HashMap::new());
    let solid_beyond = |at: [i32; 3]| *asked.borrow_mut().entry(at).or_insert_with(|| beyond(at));
    let mut piece: Vec<[i32; 3]> = Vec::new();
    for at in 0..ground.len() {
        if !ground[at] || held[at] || seen[at] {
            continue;
        }
        let (across, up) = (CHUNK as usize, (CHUNK * CHUNK) as usize);
        let first = [(at % across) as i32, (at / up) as i32, (at / across % across) as i32];
        piece.clear();
        piece.push(first);
        seen[at] = true;
        let mut next_at = 0;
        while next_at < piece.len() {
            let p = piece[next_at];
            next_at += 1;
            for w in WAYS {
                let q = step(p, w);
                if within(q) && ground[index(q)] && !seen[index(q)] {
                    seen[index(q)] = true;
                    piece.push(q);
                }
            }
        }
        if piece.len() > LOOSE {
            continue;
        }
        let sides = piece.iter().any(|&p| {
            WAYS.iter().any(|&w| {
                let q = step(p, w);
                !own(q) && q[1] >= foot && solid_beyond(q)
            })
        });
        // Reaching past the sides: followed on there, as far as LOOSE blocks in all, held up where it grows past
        // that, reaches under the column's foot or meets ground of this column held up.
        let holds = sides && {
            let mut all: Vec<[i32; 3]> = piece.clone();
            let mut grows = 0;
            let mut found = false;
            while grows < all.len() && !found {
                let p = all[grows];
                grows += 1;
                for w in WAYS {
                    let q = step(p, w);
                    if q[1] < foot || within(q) && held[index(q)] || all.len() >= LOOSE {
                        found = true;
                        break;
                    }
                    let solid = if own(q) { within(q) && ground[index(q)] } else { solid_beyond(q) };
                    if solid && !all.contains(&q) {
                        all.push(q);
                    }
                }
            }
            found
        };
        if !holds {
            for &p in &piece {
                loose[index(p)] = true;
            }
        }
    }
    for (at, chunk) in chunks.iter_mut() {
        let from = (at[1] - low) * side;
        for y in 0..CHUNK {
            for z in 0..CHUNK {
                for x in 0..CHUNK {
                    if loose[index([x as i32, from + y as i32, z as i32])] {
                        chunk.set(x, y, z, AIR);
                    }
                }
            }
        }
    }
}

/// What vines are hung by: every column read, which of them are the chunk
/// column's own, row after row along x, how many of those make a row, the
/// sea's block, the generator's seed and how many metres a block is.
struct Hung<'a> {
    columns: &'a [Column],
    own: &'a [usize],
    across: usize,
    sea: BlockId,
    seed: u32,
    size: f32,
}

/// A number for a block of the world and `salt`, from whole numbers alone:
/// the same wherever its column is made.
fn vine_number(seed: u32, [x, y, z]: [i32; 3], salt: u32) -> u32 {
    let mut h = seed ^ salt.wrapping_mul(0x27D4_EB2F) ^ (x as u32).wrapping_mul(0x9E37_79B1);
    h = h.rotate_left(13) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h = h.rotate_left(11) ^ (z as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// Hangs each column's biome's vines in `chunks`, a chunk column's chunks
/// from the lowest up, the first block of the first at `corner`. A vine
/// starts in a block of open air with ground over it, no deeper under its
/// column's surface than its biome says, one such block in so many as its
/// amount says, and three times as often where ground stands beside it in
/// the column; it hangs down as many blocks as its length says, through
/// the open air until it meets something. Each is decided from the
/// column's own blocks, so a column made alone has the vines it has among
/// its neighbours.
fn hang_vines(chunks: &mut [Option<Chunk>], hung: &Hung, corner: [i32; 3]) {
    let side = CHUNK as i32;
    let tall = chunks.len() as i32 * side;
    let deep = (hung.own.len() / hung.across.max(1)) as i32;
    let across = hung.across as i32;
    let block = |chunks: &[Option<Chunk>], x: i32, y: i32, z: i32| -> BlockId {
        if y < 0 || y >= tall {
            return AIR;
        }
        chunks[(y / side) as usize].as_ref().map_or(AIR, |chunk| chunk.get(x as u32, (y % side) as u32, z as u32))
    };
    // Whether every block of a chunk is the same: air, or one block through and through.
    let plain = |chunk: &Option<Chunk>| chunk.as_ref().is_none_or(|chunk| chunk.is_air() || chunk.is_whole());
    for z in 0..deep.min(side) {
        for x in 0..across.min(side) {
            let column = &hung.columns[hung.own[(z * across + x) as usize]];
            let vines = column.vines;
            if vines.block == AIR || vines.amount <= 0.0 {
                continue;
            }
            let at = [corner[0] + x, corner[2] + z];
            let ground = |block: BlockId| block != AIR && block != hung.sea && block != vines.block;
            let mut y = tall - 2;
            while y >= 0 {
                // Chunks of one block through and through have no place for one but where they meet.
                let chunk = (y / side) as usize;
                if y % side != side - 1 && plain(&chunks[chunk]) {
                    y = chunk as i32 * side - 1;
                    continue;
                }
                let here = y;
                y -= 1;
                if block(chunks, x, here, z) != AIR || !ground(block(chunks, x, here + 1, z)) {
                    continue;
                }
                let height = corner[1] + here;
                if (column.surface - height as f32) * hung.size > vines.depth {
                    continue;
                }
                let beside = [[1, 0], [-1, 0], [0, 1], [0, -1]].into_iter().any(|[dx, dz]| {
                    let (bx, bz) = (x + dx, z + dz);
                    (0..across).contains(&bx) && (0..deep).contains(&bz) && ground(block(chunks, bx, here, bz))
                });
                let chance = vines.amount * if beside { 1.0 } else { 0.34 };
                let place = [at[0], height, at[1]];
                if (vine_number(hung.seed, place, 1) >> 8) as f32 / 16_777_216.0 >= chance {
                    continue;
                }
                let [from, to] = vines.length;
                let long = from + vine_number(hung.seed, place, 2) % (to.max(from) - from + 1);
                for down in 0..long as i32 {
                    let hangs = here - down;
                    if hangs < 0 || block(chunks, x, hangs, z) != AIR {
                        break;
                    }
                    let chunk = chunks[(hangs / side) as usize].get_or_insert_with(|| Chunk::filled(AIR));
                    chunk.set(x as u32, (hangs % side) as u32, z as u32, vines.block);
                }
                // On from under the vine.
                y = y.min(here - long as i32);
            }
        }
    }
}

/// How many blocks further into or out of the ground than a block's middle
/// its fill is no longer measured against the blocks round it: a whole
/// block or none, on any slope a block can show.
const MEASURED: f32 = 8.0;

#[cfg(test)]
thread_local! {
    /// For a test: every cell asked about and every block put one at a time, as before any was left out.
    pub(super) static ALL_ASKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether cells the Volume says nothing is in, and whole chunks of rock, are left out.
fn leaving_out() -> bool {
    #[cfg(test)]
    return !ALL_ASKED.get();
    #[cfg(not(test))]
    true
}

/// A depth under the surface past any layer's.
const DEEP: f32 = 1.0e9;

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
    let mut heights = vec![0.0; xs.len()];
    world.heights(&xs, &zs, &mut heights);
    // A plain height with no caves in it, or with them left out of a coarse level, needs nothing asked: every
    // block is as deep as its column's height says.
    let asked = !plain || (volume.caved() && level == 0);
    let carried = match asked {
        true => volume.carried_grid([first_x, first_z], [wide, deep], |axis, v| middle(v, axis), &heights),
        false => Default::default(),
    };
    let columns = columns_at(file, world, level, (&xs, &zs, &heights));
    // Ground standing apart from a column's own, an arch or raised land, takes the layers of ground as high
    // as its top: the cover its own height and climate give it, and not the valley's under it. Every span reaching
    // well over the column's ground counts, not only those whole from foot to top: a tower's edge is as much of it.
    let stood: Vec<(usize, f32)> = match asked && volume.spanned() {
        true => (0..places.len())
            .filter_map(|at| {
                let over = volume.spans(&carried, at).map(|[_, top]| top).filter(|&top| top > heights[at] + 3.0);
                Some((at, over.reduce(f32::max).or_else(|| volume.stands(&carried, at))?))
            })
            .collect(),
        false => Vec::new(),
    };
    let mut raised: Vec<Option<Column>> = Vec::new();
    if !stood.is_empty() {
        let pick = |from: &[f32]| stood.iter().map(|&(at, _)| from[at]).collect::<Vec<f32>>();
        let tops: Vec<f32> = stood.iter().map(|&(_, top)| top).collect();
        let made = columns_at(file, world, level, (&pick(&xs), &pick(&zs), &tops));
        raised.resize_with(places.len(), || None);
        for (&(at, _), column) in stood.iter().zip(made) {
            // Kept apart from the ground's own surface, which its cells are measured from. Its top block over rock:
            // the ledges of a tower or an island's sides are rock with green on them, not striped with soil.
            let layers = [column.layers[0], (column.bottom, 0.0)];
            raised[at] = Some(Column { surface: columns[at].surface, layers, ..column });
        }
    }
    // How far over a column's surface its ground may reach, in blocks.
    let over_reach = volume.reach() / size;
    let (sea, sea_top) = sea_of(file, generate, world, level);
    // Rivers and lakes over the sea, each column's own, and which way each river's water flows.
    let (waters, currents) = waters_and_currents(file, generate, world, level, (&xs, &zs), true).unzip();
    let sea_at = |at: usize, y: i32| match &waters {
        Some(waters) if waters[at].0 > sea_top => {
            let (top, falls_from) = waters[at];
            let current = currents.as_ref().and_then(|currents| currents[at]);
            Sea { falling: y as f32 >= falls_from, current, ..Sea::at(sea, top, y, file.smooth) }
        }
        _ => Sea::at(sea, sea_top, y, file.smooth),
    };
    let water_top = |at: usize| waters.as_ref().map_or(sea_top, |waters| waters[at].0.max(sea_top));
    let surfaces = (0..places.len()).filter(|&at| own(places[at])).map(|at| columns[at].surface.max(water_top(at)));
    let mut highest = surfaces.fold(f32::MIN, f32::max).max(sea_top) + over_reach + 1.0;
    if asked && volume.spanned() {
        let own_columns = (0..places.len()).filter(|&at| own(places[at]));
        let tops = own_columns.flat_map(|at| volume.spans(&carried, at)).map(|[_, span_top]| span_top / size);
        highest = tops.fold(highest, f32::max) + over_reach + MEASURED + 3.0;
    }
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
    // Ground standing far over a column's own, and hollows far under it, are read between the heights the
    // Volume gives for them, and the rock and air between those and the ground are not asked about.
    let room = MEASURED + 2.0;
    let near = volume.depth() / size + room;
    let spans: Vec<Vec<[f32; 2]>> = match asked && leaving_out() && (volume.spanned() || near.is_finite()) {
        true => (0..places.len())
            .map(|at| {
                let from_ground = [columns[at].surface - near, columns[at].surface + over_reach + 2.0];
                let apart = volume.spans(&carried, at).map(|[foot, span_top]| {
                    [foot / size - room, span_top / size + over_reach + room]
                });
                std::iter::once(from_ground).chain(apart).collect()
            })
            .collect(),
        false => Vec::new(),
    };
    let highest_of = |at: usize| match spans.get(at) {
        Some(spans) => spans.iter().map(|span| span[1]).fold(f32::MIN, f32::max),
        None => columns[at].surface + over_reach + 2.0,
    };
    let reaches: Vec<i32> = (0..columns.len()).map(|at| (highest_of(at).floor() as i32).clamp(low, top)).collect();
    let read_to = |at: usize| -> i32 {
        if ring == 0 {
            return reaches[at];
        }
        let (x, z) = (at % wide, at / wide);
        let near = |v: usize, most: usize| v.saturating_sub(2)..(v + 3).min(most);
        let about = near(z, deep).flat_map(|z| near(x, wide).map(move |x| z * wide + x));
        about.map(|at| reaches[at]).fold(i32::MIN, i32::max) + 1
    };
    // Whole chunks of rock under everything asked about and every layer are not made a block at a time: those
    // from block `rock_to` up to `rock_from`, of the one block every column has under its layers.
    let rock = columns.first().map_or(AIR, |column| column.bottom);
    let whole = across.len() == CHUNK as usize && along.len() == CHUNK as usize;
    let alike = rock != AIR && columns.iter().all(|column| column.bottom == rock);
    let (mut rock_to, mut rock_from) = (low, low);
    if whole && alike && leaving_out() {
        let layered = |column: &Column| column.layers[0].1 + column.layers[1].1 + 2.0;
        let lowest = |at: usize| match (asked, spans.get(at)) {
            (false, _) => columns[at].surface - 1.0,
            (true, None) => f32::MIN,
            (true, Some(spans)) => spans.iter().map(|span| span[0]).fold(f32::MAX, f32::min),
        };
        let under = (0..columns.len()).map(|at| lowest(at) - layered(&columns[at])).fold(f32::MAX, f32::min);
        let under = (under.floor().max(low as f32) as i32).min(top);
        let first = low.div_euclid(side) + i32::from(low.rem_euclid(side) != 0);
        if under.div_euclid(side) > first {
            (rock_to, rock_from) = (first * side, under.div_euclid(side) * side);
        }
    }
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
        match (asked, spans.get(at)) {
            (false, _) => {}
            (true, None) => {
                let cells = Cells { x: &[px], y: &ys[..count], z: &[pz], carried: &carried.column(at), size };
                volume.densities(cells, level > 0, &mut row[..count], &mut ground_row[..count]);
            }
            (true, Some(spans)) => {
                // What is asked about nowhere is as far under its column's ground as it lies, or over it.
                for k in 0..count {
                    row[k] = heights[at] - ys[k];
                    ground_row[k] = row[k];
                }
                let carried = carried.column(at);
                let mut done = 0;
                let mut spans: Vec<[usize; 2]> = spans
                    .iter()
                    .map(|span| span.map(|y| (y.floor() as i64 - i64::from(first_y)).clamp(0, count as i64) as usize))
                    .collect();
                spans.sort_unstable();
                for [from, to] in spans {
                    let (from, to) = (from.max(done), (to + 1).min(count));
                    if from < to {
                        let cells = Cells { x: &[px], y: &ys[from..to], z: &[pz], carried: &carried, size };
                        volume.densities(cells, level > 0, &mut row[from..to], &mut ground_row[from..to]);
                        done = to;
                    }
                }
            }
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
        let ((x, z), ground_column) = (places[at], &columns[at]);
        let raised_column = raised.get(at).and_then(Option::as_ref);
        // What stands clear of the column's own ground: from this block up.
        let clear = (ground_column.surface + over_reach + 3.0).floor() as i32;
        let from = if ring == 0 { 0 } else { at * tall };
        if ring == 0 {
            read[at] = read_column(at, &mut inside[..tall], &mut under[..tall], &mut row, &mut ground_row);
        }
        // Down the column from the highest block made, for the count of its layers: open air first, where only
        // the sea is.
        made.clear();
        let mut above = 0.0;
        for y in (rock_from..=top).rev().chain((low..rock_to).rev()) {
            let k = (y - first_y) as usize;
            let column = raised_column.filter(|_| y >= clear).unwrap_or(ground_column);
            // Under the rock left out every block is deeper than any layer.
            if y == rock_to - 1 {
                above = DEEP;
            }
            if k >= read[at] {
                made.extend(cell(column, -2.0, -2.0, 0.0, -2.0, sea_at(at, y), file.smooth).map(|cell| (y, cell)));
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
            made.extend(cell(column, within, ground, depth, filled, sea_at(at, y), file.smooth).map(|cell| (y, cell)));
        }
        // Kept from the lowest up, as chunks' blocks are.
        for &(y, (block, fill)) in made.iter().rev() {
            put(x, y, z, block, fill);
        }
    }
    let whole_rock = (rock_to.div_euclid(side) - low_key) as usize..(rock_from.div_euclid(side) - low_key) as usize;
    for chunk in &mut chunks[whole_rock] {
        *chunk = Some(Chunk::filled(rock));
    }
    // Vines, at the world's own level and where it is drawn as cubes: under ledges, overhangs and cave roofs.
    if level == 0 && !file.smooth && columns.iter().any(|column| column.vines.amount > 0.0) {
        let own: Vec<usize> = (0..places.len()).filter(|&at| own(places[at])).collect();
        let hung = Hung { columns: &columns, own: &own, across: across.len(), sea, seed: world.seed(), size };
        hang_vines(&mut chunks, &hung, [across.start, low_key * side, along.start]);
    }
    let mut chunks: Vec<([i32; 3], Chunk)> =
        (low_key..).zip(chunks).filter_map(|(cy, chunk)| Some(([cx, cy, cz], chunk?))).collect();
    // What its generator builds on its ground, at its own level alone: far off, structures are too small to see.
    // Rock a cave or a passage cut away from under, left loose in the air, is cleared, before anything is built:
    // followed into the columns beside where it reaches past this one's sides.
    if level == 0 && !file.smooth {
        let foot = chunks.iter().map(|(at, _)| at[1]).min().unwrap_or(0) * side;
        let beyond = |[x, y, z]: [i32; 3]| {
            let (bx, by, bz) = (cx * side + x, foot + y, cz * side + z);
            let (gx, gz) = (middle(bx, 0), middle(bz, 1));
            let mut height = [0.0];
            world.heights(&[gx], &[gz], &mut height);
            let cy = (by as f32 + 0.5) * size;
            if !asked {
                return cy < height[0];
            }
            let carried = volume.carried(&[gx], &[gz], &height);
            let rows: Vec<&[f32]> = carried.rows().iter().map(Vec::as_slice).collect();
            let (px, pz) = match source.own() {
                true => (gx, gz),
                false => ((bx as f32 + 0.5) * size, (bz as f32 + 0.5) * size),
            };
            let (mut inside, mut under) = ([0.0], [0.0]);
            let cells = Cells { x: &[px], y: &[cy], z: &[pz], carried: &rows, size };
            volume.densities(cells, false, &mut inside, &mut under);
            inside[0] > 0.0
        };
        clear_loose(&mut chunks, sea, &beyond);
    }
    let built = generate.built.as_deref().filter(|built| !built.list.is_empty());
    if let Some(built) = built.filter(|_| level == 0) {
        let biomes = generate.placed.as_deref().map(|placed| &placed.biomes).filter(|biomes| !biomes.is_empty());
        // Built up under with the world's own under block.
        let under = file.ground.under;
        let under = if usize::from(under) <= file.blocks.len() { under } else { AIR };
        let at = (size, generate.position, generate.bottom);
        crate::blocks::structure::build(&mut chunks, built, world, biomes, at, [cx, cz], under);
    }
    chunks.into_iter().map(|(at, chunk)| (at, chunk.tidied())).collect()
}
