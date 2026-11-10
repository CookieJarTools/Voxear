//! Ground of blocks made by a world generator (see `crate::generate`): its
//! heights a column at a time, the same mountains, coasts and climate a
//! terrain's Generate layer lays, and how far into the ground each block
//! is from the generator's Volume, which pushes cliffs in and out so they
//! lean and overhang, and hollows the ground with caves. A generator with
//! no Volume has the world's own overhangs and caves wired into it as the
//! nodes that make them (see `cells`), so one way makes both.
//!
//! Each column takes its world's ground layers (`file::Ground`'s top, under
//! and bottom blocks), but where a kind of block's cover takes its climate
//! and height (`file::BlockCover`), that kind lies on top instead. Where it
//! has a sea block, the open air below its generator's sea level is filled
//! with it, as water is (caves under the ground stay dry). Every level's sea
//! stands as high, so rings of chunks meet without steps: drawn smooth, at
//! the sea level, its top blocks keeping where its surface lies; of cubes,
//! where the finest level's does, a coarser level's top block of it only as
//! full as that, in eighths. Every
//! chunk is made apart, on every core, from where its blocks are alone, so
//! the same ground comes from the same file anywhere.
//!
//! A planet's ground lies all round a middle at its world's corner (see
//! [`BlockPlanet`]): its generator's heights and climate read round it, once,
//! on the six faces of a cube about a block apart, and every block's depth
//! measured out from the middle.

use serde::{Deserialize, Serialize};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK},
    file::BlockFile,
};
use crate::generate::WorldGen;

mod cells;
mod columns;
mod planet;

pub use cells::CAVE_ROOF;
pub use columns::column_chunks;
pub use planet::planet_ground;

/// How a world of blocks' ground is generated: by the world generator
/// `world` names (a reference to a `.worldgen`), from `position` metres in
/// its world, down to block `bottom` (counted from the world's corner, so
/// negative goes below it). While the generator's graph has no Volume, its
/// steep ground is pushed in and out by as many metres as `overhangs`, and
/// as much of the ground hollowed out as `caves` says, from 0 for none to
/// 1; a generator with a Volume makes both itself, and these two are left
/// unread.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockGenerate {
    pub world: Option<String>,
    pub position: [f32; 2],
    pub bottom: i32,
    pub overhangs: f32,
    pub caves: f32,
    /// The block the air below its generator's sea level is filled with, by
    /// its place in the list from 1: air for none. A planet's sea is its
    /// radius from its middle.
    #[serde(skip_serializing_if = "is_air")]
    pub sea: BlockId,
    /// For a world without end, made round the camera as it goes: how far
    /// (see `stream`). Its ground's size is then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endless: Option<BlockEndless>,
    /// For a planet, its ground all round a middle at the world's corner: how
    /// large. Its ground's size, its place in its generator's world, its
    /// lowest block and going on without end are then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planet: Option<BlockPlanet>,
}

impl Default for BlockGenerate {
    fn default() -> Self {
        Self {
            world: None,
            position: [0.0, 0.0],
            bottom: -16,
            overhangs: 0.0,
            caves: 0.3,
            sea: AIR,
            endless: None,
            planet: None,
        }
    }
}

fn is_air(block: &BlockId) -> bool {
    *block == AIR
}

/// A planet: its sea `radius` metres from its middle, and its ground as high
/// over that as its generator's heights are over its sea level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockPlanet {
    pub radius: f32,
}

impl Default for BlockPlanet {
    fn default() -> Self {
        Self { radius: 100.0 }
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

/// The ground `generate` makes for `file` with generator `world`: its
/// ground's size of chunk columns, each on every core.
pub fn generate(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen) -> BlockMap {
    if let Some(planet) = generate.planet {
        return planet_ground(file, generate, Some(world), planet);
    }
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

#[cfg(test)]
mod tests {
    use super::{planet::Round, *};
    use crate::{
        blocks::{
            chunk::Chunk,
            file::{BlockCover, BlockKind, Ground},
        },
        generate::Preset,
        surfaces::Biome,
    };

    fn generated(caves: f32, overhangs: f32) -> (BlockFile, WorldGen) {
        // Mountains 300 m high in a cold world, so some of it is snowed.
        let mut land = Preset::Alpine.landmass();
        land.height = 300.0;
        land.climate.temperature = 0.0;
        let world = WorldGen::from(land);
        let mut file = BlockFile {
            block_size: 2.0,
            ground: Ground { size: [64, 64], ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("alpine".into()),
                position: [500.0, 800.0],
                bottom: -40,
                overhangs,
                caves,
                sea: AIR,
                endless: None,
                planet: None,
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
        // Overhangs push steep ground alone: on these slopes, which are no cliffs, the ground is as it was.
        let (leaning, _) = generated(0.0, 10.0);
        assert_eq!(leaning.make_ground(Some(&world)), map);
    }

    /// Cliffs: hills of noise 150 m high and 60 m across, as a generator.
    fn cliffs() -> WorldGen {
        use crate::generate::{Graph, NodeKind, nodes};
        let mut graph = Graph::default();
        let noise = nodes::Noise { size: 60.0, octaves: 1, high: 150.0, ..nodes::Noise::default() };
        let hills = graph.add(NodeKind::Noise(noise), [0.0, 0.0]);
        let output = graph.add(NodeKind::Output(nodes::Output::default()), [300.0, 0.0]);
        assert!(graph.connect(hills, 0, output, nodes::Output::HEIGHT));
        WorldGen::new(7, 1.0, 0.0, graph)
    }

    /// A world of metre blocks 64 across on the cliffs, its ground pushed and hollowed as told.
    fn on_cliffs(overhangs: f32, caves: f32, smooth: bool) -> BlockFile {
        let generate =
            BlockGenerate { world: Some("cliffs".into()), bottom: -8, overhangs, caves, ..BlockGenerate::default() };
        let ground = Ground { size: [64, 64], under_depth: 3, ..Ground::default() };
        BlockFile { block_size: 1.0, ground, smooth, generate: Some(generate), ..BlockFile::default() }
    }

    /// How far into the ground under the caves each block up column `(x, z)` of `file`'s ground is, in blocks
    /// from `low` up: what tells the open air from a cave's.
    fn grounds(file: &BlockFile, world: &WorldGen, [x, z]: [i32; 2], low: i32, count: usize) -> Vec<f32> {
        let generate = file.generate.as_ref().unwrap();
        let source = cells::source(generate, world, file.block_size);
        let volume = source.world().volume().unwrap();
        let (px, pz) = (x as f32 + 0.5, z as f32 + 0.5);
        let height = world.height_at(px, pz);
        let carried =
            volume.carried_grid([x, z], [1, 1], |axis, v| generate.position[axis] + (v as f32 + 0.5), &[height]);
        let ys: Vec<f32> = (0..count).map(|k| (low + k as i32) as f32 + 0.5).collect();
        let (mut inside, mut ground) = (vec![0.0; count], vec![0.0; count]);
        let cells = crate::generate::Cells { x: &[px], y: &ys, z: &[pz], carried: &carried.column(0), size: 1.0 };
        volume.densities(cells, false, &mut inside, &mut ground);
        ground
    }

    #[test]
    fn layers_are_counted_down_from_the_open_air_on_overhanging_ground_and_through_caves() {
        let world = cliffs();
        let file = on_cliffs(20.0, 0.5, false);
        let (top, under, bottom) = (file.ground.top, file.ground.under, file.ground.bottom);
        let map = file.make_ground(Some(&world));
        let (low, count) = (-8, 190usize);
        let (mut overhangs, mut ledges, mut seams, mut caves) = (0, 0, 0, 0);
        for z in 0..64 {
            for x in 0..64 {
                let ground = grounds(&file, &world, [x, z], low, count);
                // Down the column: the first block under open air is the top block, the three under it the
                // under block, the rest the bottom one, counted through cave air but again under every overhang.
                let mut deep = 0;
                for k in (0..count - 1).rev() {
                    let (y, block) = (low + k as i32, map.get([x, low + k as i32, z]));
                    deep = if ground[k] > 0.0 { deep + 1 } else { 0 };
                    if block == AIR {
                        caves += usize::from(ground[k] > 0.0);
                        continue;
                    }
                    assert!(ground[k] > 0.0, "ground in the open air at {x}, {y}, {z}");
                    let layer = [top, under, under, under].get(deep - 1).copied().unwrap_or(bottom);
                    assert_eq!(block, layer, "block {deep} down at {x}, {y}, {z}");
                    // Under an overhang: ground with open air under it has none of the top block on its
                    // underside unless it is one block thick, and the ground below starts with it again.
                    if k > 0 && ground[k - 1] <= 0.0 {
                        overhangs += 1;
                        assert!(deep == 1 || block != top, "the top block under an overhang at {x}, {y}, {z}");
                        ledges += usize::from((0..k - 1).any(|below| ground[below] > 0.0));
                    }
                    seams += usize::from(deep == 1 && y.rem_euclid(CHUNK as i32) == 0);
                }
            }
        }
        assert!(overhangs > 40 && ledges > 20, "{overhangs} blocks hang over open air, {ledges} over more ground");
        assert!(seams > 20 && caves > 1000, "{seams} top blocks lowest in their chunk, {caves} blocks of cave");
        // Drawn smooth, the top block follows slopes as a skin: a block with open air over it is of the top
        // block unless it stands in a wall, and so are some its surface steps down past, with ground over them,
        // which would else show the block under. Under an overhang there is none still.
        let file = on_cliffs(20.0, 0.0, true);
        let map = file.make_ground(Some(&world));
        let (mut tops, mut bare, mut skin) = (0, 0, 0);
        for z in 0..64 {
            for x in 0..64 {
                let ground = grounds(&file, &world, [x, z], low, count);
                for k in (1..count - 1).filter(|&k| ground[k] > 0.0) {
                    let (y, block) = (low + k as i32, map.get([x, low + k as i32, z]));
                    assert_ne!(block, AIR, "ground at {x}, {y}, {z}");
                    let (over, under_it) = (ground[k + 1] <= 0.0, ground[k - 1] <= 0.0);
                    tops += usize::from(over);
                    bare += usize::from(over && block != top);
                    skin += usize::from(!over && block == top);
                    assert!(over || !under_it || block != top, "the top block under an overhang at {x}, {y}, {z}");
                }
            }
        }
        assert!(tops > 3000 && skin > 100, "{tops} blocks under open air, {skin} more of the top block down slopes");
        assert!(bare > 0 && bare < tops / 3, "{bare} of {tops} blocks under open air stand in walls, bare");
    }

    #[test]
    fn overhangs_and_caves_moved_into_the_generator_make_the_ground_the_world_made_with_its_own() {
        use crate::generate::OwnVoxels;
        for (overhangs, caves) in [(0.0, 0.4), (14.0, 0.5), (5.0, 0.0)] {
            for smooth in [false, true] {
                let world = cliffs();
                let own = on_cliffs(overhangs, caves, smooth);
                // What Move to Generator does: the numbers into the graph, as nodes and rows, and none left on
                // the world.
                let mut graph = world.graph().clone();
                assert!(graph.add_voxels(OwnVoxels { overhangs, caves, roof: 3.0 * own.block_size }, true));
                let moved_world = world.with_graph(graph);
                assert!(moved_world.volume().is_some() && world.volume().is_none());
                let moved = on_cliffs(0.0, 0.0, smooth);
                let (was, now) = (own.make_ground(Some(&world)), moved.make_ground(Some(&moved_world)));
                assert!(was == now, "overhangs {overhangs}, caves {caves}, smooth {smooth}");
                assert!(was.chunks().count() > 4);
                // The generator's rows change the ground from then on.
                let fewer = moved_world.with_exposed(1, crate::generate::Value::Number(0.0)).expect("the Tunnels row");
                assert!(caves == 0.0 || moved.make_ground(Some(&fewer)) != now);
            }
        }
    }

    /// A hash of every block and fill of `chunks`, in their order.
    fn hash_of<'a>(chunks: impl IntoIterator<Item = (&'a [i32; 3], &'a Chunk)>) -> u64 {
        let mut bytes = Vec::new();
        for (key, chunk) in chunks {
            bytes.extend(key.iter().flat_map(|v| v.to_le_bytes()));
            for (x, y, z) in (0..CHUNK).flat_map(|y| (0..CHUNK).flat_map(move |z| (0..CHUNK).map(move |x| (x, y, z)))) {
                bytes.extend(chunk.get(x, y, z).to_le_bytes());
                bytes.push(chunk.fill(x, y, z));
            }
        }
        crate::layers::stable_hash(&bytes)
    }

    /// Three chunk columns of `file`'s ground at `level`, hashed: over the first high ground found in `world`, so
    /// they hold mountainsides, snow and caves rather than sea floor.
    fn columns_hash(file: &BlockFile, world: &WorldGen, level: u32) -> u64 {
        let spots = (0..200).flat_map(|i| (0..200).map(move |j| [i as f32 * 250.0, j as f32 * 250.0]));
        let position = spots.into_iter().find(|&[x, z]| world.height_at(x, z) > 150.0).expect("high ground");
        let generate = BlockGenerate { position, ..file.generate.clone().expect("generated") };
        let made: Vec<([i32; 3], Chunk)> = [[0, 0], [1, 0], [-3, 7]]
            .into_iter()
            .flat_map(|at| column_chunks(file, &generate, world, level, at, None))
            .collect();
        let kinds: std::collections::BTreeSet<BlockId> =
            made.iter().flat_map(|(_, chunk)| (0..CHUNK).map(|v| chunk.get(v, v, v))).collect();
        assert!(made.len() >= 3 && kinds.len() >= 3, "{} chunks of {kinds:?}", made.len());
        hash_of(made.iter().map(|(key, chunk)| (key, chunk)))
    }

    #[test]
    fn generated_chunks_are_the_same_blocks_to_the_bit_as_cubes_and_smooth_near_and_far() {
        // As they were made before generators were graphs: covered by climate, hollowed by caves, with a sea.
        let (mut file, world) = generated(0.5, 0.0);
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        file.generate.as_mut().unwrap().sea = file.blocks.len() as BlockId;
        let smooth = BlockFile { smooth: true, ..file.clone() };
        let hashes = [columns_hash(&file, &world, 0), columns_hash(&file, &world, 2), columns_hash(&smooth, &world, 0)];
        assert_eq!(
            hashes,
            [0xb5c2_959f_3398_5e27, 0x06fa_ee9e_a714_aca3, 0x7257_8dd2_041f_3d95],
            "generated blocks changed: were they meant to? {hashes:#x?}"
        );
    }

    /// Hills, or level rock, with a network of cave rooms taken out, and a world of metre blocks on it.
    fn networked(layout: crate::generate::nodes::Laid, level: bool, caves: bool) -> (BlockFile, WorldGen) {
        use crate::generate::{
            Graph, NodeKind,
            nodes::{CaveNetwork, Combine, Combining, Constant, Noise, Output, Solid},
        };
        let mut graph = Graph::default();
        let height = match level {
            true => graph.add(NodeKind::Constant(Constant { value: 70.0 }), [0.0, 0.0]),
            false => {
                graph.add(NodeKind::Noise(Noise { size: 300.0, low: 20.0, high: 90.0, ..Noise::default() }), [0.0, 0.0])
            }
        };
        let solid = graph.add(NodeKind::Solid(Solid::default()), [0.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [0.0, 0.0]);
        assert!(graph.connect(height, 0, solid, Solid::HEIGHT) && graph.connect(height, 0, output, Output::HEIGHT));
        if caves {
            let network = CaveNetwork { layout, entrances: 0.5, ..CaveNetwork::default() };
            let network = graph.add(NodeKind::CaveNetwork(network), [0.0, 0.0]);
            let take = Combine { operation: Combining::Subtract, ..Combine::default() };
            let take = graph.add(NodeKind::Combine(take), [0.0, 0.0]);
            assert!(graph.connect(solid, 0, take, Combine::A) && graph.connect(network, 0, take, Combine::B));
            assert!(graph.connect(take, 0, output, Output::VOLUME));
        } else {
            assert!(graph.connect(solid, 0, output, Output::VOLUME));
        }
        let world = WorldGen::new(11, 1.0, 0.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        let file = BlockFile {
            block_size: 1.0,
            ground: Ground { size: [64, 64], ..Ground::default() },
            generate: Some(BlockGenerate {
                world: Some("caves".into()),
                position: [0.0, 0.0],
                bottom: -32,
                overhangs: 0.0,
                caves: 0.0,
                sea: AIR,
                endless: None,
                planet: None,
            }),
            ..BlockFile::default()
        };
        (file, world)
    }

    #[test]
    fn a_chunk_column_of_a_cave_network_made_alone_is_the_one_made_among_its_neighbours() {
        use crate::generate::nodes::Laid;
        for (layout, smooth) in [(Laid::Endless, false), (Laid::Endless, true), (Laid::Level, false)] {
            let (file, world) = networked(layout, layout == Laid::Level, true);
            let file = BlockFile { smooth, ..file };
            let generate = file.generate.clone().unwrap();
            // Over the level's first rooms, or wherever an endless network lies.
            let columns: [[i32; 2]; 3] = [[0, 0], [1, 0], [0, 1]];
            // Alone, each on a thread that has made nothing else.
            let alone: Vec<u64> = columns
                .iter()
                .map(|&at| {
                    let (file, generate, world) = (file.clone(), generate.clone(), world.clone());
                    let made = std::thread::spawn(move || column_chunks(&file, &generate, &world, 0, at, None));
                    let made = made.join().unwrap();
                    hash_of(made.iter().map(|(key, chunk)| (key, chunk)))
                })
                .collect();
            // Among their neighbours, on one thread, the ring round them made first and backwards.
            let mut ring: Vec<[i32; 2]> = (-1..=2).flat_map(|x| (-1..=2).map(move |z| [x, z])).collect();
            ring.reverse();
            let mut hollow = 0;
            for at in ring {
                let made = column_chunks(&file, &generate, &world, 0, at, None);
                // Air with ground somewhere over it in its chunk: a cave's.
                let roofed = |chunk: &Chunk| {
                    (0..CHUNK).any(|x| {
                        let under = |y: u32, z: u32| chunk.get(x, y - 1, z) == AIR && chunk.get(x, y, z) != AIR;
                        (0..CHUNK).any(|z| (1..CHUNK).any(|y| under(y, z)))
                    })
                };
                hollow += made.iter().filter(|(_, chunk)| roofed(chunk)).count();
            }
            assert!(hollow > 0, "{layout:?}: no cave in sixteen chunk columns");
            for (at, alone) in columns.iter().zip(&alone) {
                let made = column_chunks(&file, &generate, &world, 0, *at, None);
                let among = hash_of(made.iter().map(|(key, chunk)| (key, chunk)));
                assert_eq!(among, *alone, "{layout:?}, smooth {smooth}: chunk column {at:?}");
            }
        }
    }

    #[test]
    #[ignore = "a timing, to be read: run it optimised"]
    fn timing_of_a_chunk_column_with_and_without_a_cave_network() {
        use crate::generate::nodes::Laid;
        let time = |file: &BlockFile, world: &WorldGen| {
            let generate = file.generate.clone().unwrap();
            let mut least = f32::MAX;
            for _ in 0..3 {
                let started = std::time::Instant::now();
                for at in [[0, 0], [1, 0], [0, 1], [1, 1], [3, 5], [-4, 2], [7, -3], [2, 8]] {
                    std::hint::black_box(column_chunks(file, &generate, world, 0, at, None));
                }
                least = least.min(started.elapsed().as_secs_f32() * 1000.0 / 8.0);
            }
            least
        };
        let kinds = [("endless under hills", Laid::Endless, false), ("a level in rock", Laid::Level, true)];
        for (name, layout, level) in kinds {
            let (file, with) = networked(layout, level, true);
            let (_, without) = networked(layout, level, false);
            let smooth = BlockFile { smooth: true, ..file.clone() };
            println!(
                "{name}: a chunk column of cubes {:.2} ms without, {:.2} ms with; smooth {:.2} and {:.2} ms",
                time(&file, &without),
                time(&file, &with),
                time(&smooth, &without),
                time(&smooth, &with)
            );
        }
    }

    #[test]
    fn generated_chunks_with_overhangs_are_the_same_blocks_to_the_bit() {
        // Apart from those without. Pinned again when overhangs became a node: they push steep ground alone,
        // layers are counted down from the open air, and smooth fills are distances. At level 2 the blocks, 8 m
        // a side, are too large to hold a bulge, so the far ground is the ground with no overhangs.
        let (file, world) = generated(0.5, 6.0);
        let smooth = BlockFile { smooth: true, ..file.clone() };
        let hashes = [columns_hash(&file, &world, 0), columns_hash(&file, &world, 2), columns_hash(&smooth, &world, 0)];
        assert_eq!(
            hashes,
            [0x879c_99e5_626f_1886, 0x06fa_ee9e_a714_aca3, 0xfc09_8f2b_e038_785a],
            "overhanging blocks changed: were they meant to? {hashes:#x?}"
        );
    }

    #[test]
    #[ignore = "a planet's grid takes its angles from the machine's own tan and atan, so these bits are this machine's"]
    fn a_planets_blocks_are_the_same_to_the_bit_on_this_machine() {
        let mut land = Preset::RollingHills.landmass();
        land.height = 12.0;
        let world = WorldGen::from(land);
        let planet = BlockPlanet { radius: 60.0 };
        let generate = BlockGenerate { world: Some("hills".into()), caves: 0.3, planet: Some(planet), ..Default::default() };
        let file = BlockFile { generate: Some(generate), smooth: true, ..BlockFile::default() };
        let map = file.make_ground(Some(&world));
        let mut chunks: Vec<_> = map.chunks().map(|(key, chunk)| (key, &**chunk)).collect();
        chunks.sort_unstable_by_key(|(key, _)| **key);
        let hash = hash_of(chunks);
        assert_eq!(hash, 0x036e_d4ff_b4fd_c0dd, "a planet's blocks changed: were they meant to? {hash:#x}");
    }

    #[test]
    fn generated_ground_below_its_sea_lies_under_water_up_to_the_sea() {
        let (mut file, _) = generated(0.0, 0.0);
        let world = Preset::Archipelago.world();
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let water = file.blocks.len() as BlockId;
        // A spot of sea floor 6 to 12 m under the sea, which column (30, 40) of 2 m blocks lies over.
        let spots = (0..400).flat_map(|i| (0..400).map(move |j| (i as f32 * 25.0, j as f32 * 25.0)));
        let (px, pz) = spots
            .into_iter()
            .find(|&(px, pz)| (6.0..12.0).contains(&(world.sea_level() - world.height_at(px, pz))))
            .expect("sea floor somewhere");
        let (x, z) = (30, 40);
        let generate = file.generate.as_mut().unwrap();
        generate.position = [px - (x as f32 + 0.5) * 2.0, pz - (z as f32 + 0.5) * 2.0];
        generate.sea = water;
        let map = file.make_ground(Some(&world));
        let top = (-40..400).rev().find(|&y| map.get([x, y, z]) != AIR).expect("the column");
        assert_eq!(map.get([x, top, z]), water, "water over it");
        let middle = |y: i32| (y as f32 + 0.5) * 2.0;
        assert!(middle(top) < world.sea_level() && middle(top + 1) >= world.sea_level(), "up to the sea");
        let ground = (-40..top).rev().find(|&y| map.get([x, y, z]) != water).unwrap();
        let deep = world.sea_level() - world.height_at(px, pz);
        assert!(((top - ground) as f32 - deep / 2.0).abs() <= 1.0, "{} blocks for {deep} m", top - ground);
        // Nowhere is water above the sea.
        for (key, chunk) in map.chunks() {
            for y in 0..32 {
                let above = middle(key[1] * 32 + y as i32) >= world.sea_level();
                assert!(!above || (0..32).all(|z| (0..32).all(|x| chunk.get(x, y, z) != water)));
            }
        }
        // Drawn smooth, the sea also takes the ground's top block where its surface lies under the sea with no sea
        // over it, so its surface reaches the shore: no block of ground under the sea's level with air over it has
        // its ground under the sea.
        let smooth = BlockFile { smooth: true, ..file.clone() };
        let map = smooth.make_ground(Some(&world));
        let position = smooth.generate.as_ref().unwrap().position;
        let mut shore = 0;
        for cx in 0..64 {
            for cz in 0..64 {
                let height =
                    world.height_at(position[0] + (cx as f32 + 0.5) * 2.0, position[1] + (cz as f32 + 0.5) * 2.0);
                for y in -20..40 {
                    let (block, over) = (map.get([cx, y, cz]), map.get([cx, y + 1, cz]));
                    if block != AIR && block != water && over == AIR && middle(y) < world.sea_level() {
                        assert!(height >= world.sea_level() - 0.01, "dry ground under the sea at {cx}, {y}, {cz}");
                    }
                    shore += usize::from(block == water && height > middle(y));
                }
            }
        }
        assert!(shore > 0, "some of the sea lies over ground that reaches into its blocks");
        // Under it the sea keeps its floor, as deep as the ground lies.
        let top = (-40..400).rev().find(|&y| map.get([x, y, z]) != AIR).expect("the column");
        let ground = (-40..top).rev().find(|&y| map.get([x, y, z]) != water).expect("a sea floor");
        assert_ne!(map.get([x, ground, z]), AIR, "ground under the smooth sea");
        assert!(((top - ground) as f32 - deep / 2.0).abs() <= 1.0, "{} smooth blocks for {deep} m", top - ground);
        // Without a sea block, none.
        file.generate.as_mut().unwrap().sea = AIR;
        assert_ne!(file.make_ground(Some(&world)).get([x, top, z]), water);
    }

    #[test]
    fn a_planets_ground_lies_all_round_its_middle_on_its_generators_heights_read_round_it() {
        let mut land = Preset::RollingHills.landmass();
        land.height = 12.0;
        let world = WorldGen::parse(&WorldGen::from(land).to_text()).unwrap();
        let planet = BlockPlanet { radius: 60.0 };
        let generate =
            BlockGenerate { world: Some("hills".into()), caves: 0.0, planet: Some(planet), ..Default::default() };
        let file = BlockFile { generate: Some(generate), smooth: true, ..BlockFile::default() };
        let map = file.make_ground(Some(&world));
        // Its middle is its base block, and far out is air.
        assert_eq!(map.get([0, 0, 0]), file.ground.bottom);
        assert_eq!(map.get([0, 100, 0]), AIR);
        // Out along six ways and between them, the surface where the generator's heights put it, to a tenth of a block.
        let surface = |way: [f32; 3]| {
            let length = way.iter().map(|v| v * v).sum::<f32>().sqrt();
            let way = way.map(|v| v / length);
            let at = |r: f32| way.map(|v| (v * r - 0.5).round() as i32);
            // The last block of ground going out, and how far into the ground its middle is.
            let r = (0..1200).map(|i| i as f32 * 0.1).filter(|&r| map.get(at(r)) != AIR).fold(0.0, f32::max);
            let block = at(r);
            let middle = block.map(|v| v as f32 + 0.5);
            let out = middle.iter().map(|v| v * v).sum::<f32>().sqrt();
            out + super::super::chunk::depth_of(map.fill(block))
        };
        for way in [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.6, 0.7, -0.4], [-0.5, 0.5, 0.5]] {
            let length = way.iter().map(|v: &f32| v * v).sum::<f32>().sqrt();
            let at = way.map(|v| v / length * planet.radius);
            let expected = planet.radius + world.height_round(at) - world.sea_level();
            assert!((surface(way) - expected).abs() < 1.0, "out along {way:?}: {} for {expected}", surface(way));
        }
        // Read on a cube's faces, its ground runs on across their edges: no step where two faces meet.
        let source = cells::source(file.generate.as_ref().unwrap(), &world, 1.0);
        let round = Round::of(source.world(), source.world().volume().unwrap(), planet.radius, 1.0);
        let either = |e: f32| round.at([1.0, 1.0 + e, 0.3]).surface;
        assert!((either(0.002) - either(-0.002)).abs() < 0.1, "{} and {}", either(0.002), either(-0.002));
        // Colder at its poles than round its middle.
        let (pole, _) = world.climate_round([0.0, planet.radius, 0.0], 0.0);
        let (middle, _) = world.climate_round([planet.radius, 0.0, 0.0], 0.0);
        assert!(pole < middle - 10.0, "{pole} at a pole, {middle} round its middle");
    }
}
