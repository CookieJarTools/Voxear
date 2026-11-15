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

use serde::{Deserialize, Serialize};

use super::{
    BlockMap,
    chunk::{AIR, BlockId, CHUNK},
    file::BlockFile,
};
use crate::generate::WorldGen;

mod cells;
mod columns;
pub mod far;
pub mod tints;

pub use cells::{CAVE_ROOF, landed, lays_land, write_land};
pub use columns::column_chunks;

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
    /// Whether rivers run down its ground where its generator makes no
    /// water of its own: a Rivers node over the generator's ground (see
    /// `generate::nodes::Rivers`). Off for a world written before worlds
    /// had them; a world given generated ground in the editor starts on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rivers: bool,
    /// Whether its ground's top is laid in half blocks: where the ground
    /// lies half a block over a whole one, the top block is a slab of it,
    /// so slopes rise in half steps. Off for a world written before worlds
    /// had them; a world given generated ground in the editor starts on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slabs: bool,
    /// For each block by its number, the number of its slab (see
    /// `file::BlockKind::slab`), air where it has none: filled in as
    /// `placed` is, for the blocks the ground's top can be.
    #[serde(skip)]
    pub halves: Option<std::sync::Arc<Vec<BlockId>>>,
    /// The block the air below its generator's sea level is filled with, by
    /// its place in the list from 1: air for none.
    #[serde(skip_serializing_if = "is_air")]
    pub sea: BlockId,
    /// For a world without end, made round the camera as it goes: how far
    /// (see `stream`). Its ground's size is then left unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endless: Option<BlockEndless>,
    /// The biomes its ground has, each a reference to a `.biome`: each place
    /// takes the blocks of the one its climate is nearest (see
    /// `crate::generate::biomes`). None, and its ground is the world's own
    /// layers everywhere.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub biomes: Vec<String>,
    /// How far from where it lies in its generator's world, in metres, its
    /// biomes' land (see `crate::generate::land`) starts to rise: the
    /// ground within it is the generator's own, so a level built where the
    /// world starts keeps the ground it was built on. Nothing for none.
    #[serde(skip_serializing_if = "is_nothing")]
    pub land_from: f32,
    /// How far apart its biomes' regions are, in metres, where each region
    /// is one biome through (see `crate::generate::regions`): nothing for
    /// each place to take the biome its own climate is nearest.
    #[serde(skip_serializing_if = "is_nothing")]
    pub biome_size: f32,
    /// Where its land lies in levels: areas at a few set heights, sheer
    /// walls between (see `crate::generate::nodes::Levels`). Its biomes'
    /// regions are then its areas, as wide as they are, so each area is one
    /// biome. None for the generator's own hills and mountains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<crate::generate::nodes::Levels>,
    /// Whether its levels and its biomes' land, with the rivers and caves
    /// laid in with them, are written into its generator as nodes (see
    /// `cells::write_land`): it lays none in itself then, and its levels
    /// and biomes say only where its biomes lie.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub land_written: bool,
    /// Its biomes as read, with their blocks by this world's numbers: filled
    /// in by whoever has the project's files, before ground is made.
    #[serde(skip)]
    pub placed: Option<std::sync::Arc<crate::generate::WorldBiomes>>,
    /// The structures its generator builds, each a reference to a
    /// `.structure`, earlier ones first where two would stand together (see
    /// `blocks::structure`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub structures: Vec<String>,
    /// Its structures as read, with their blocks by this world's numbers:
    /// filled in as `placed` is.
    #[serde(skip)]
    pub built: Option<std::sync::Arc<super::structure::WorldStructures>>,
}

fn is_nothing(value: &f32) -> bool {
    *value <= 0.0
}

/// Whether `generate`'s ground has its top laid in half blocks: it says so,
/// and its slabs are numbered.
pub fn halved(generate: &BlockGenerate) -> bool {
    generate.slabs && generate.halves.is_some()
}

impl Default for BlockGenerate {
    fn default() -> Self {
        Self {
            world: None,
            position: [0.0, 0.0],
            bottom: -16,
            overhangs: 0.0,
            caves: 0.3,
            rivers: false,
            slabs: false,
            halves: None,
            sea: AIR,
            endless: None,
            biomes: Vec::new(),
            land_from: 0.0,
            biome_size: 0.0,
            levels: None,
            land_written: false,
            placed: None,
            structures: Vec::new(),
            built: None,
        }
    }
}

fn is_air(block: &BlockId) -> bool {
    *block == AIR
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

/// The ground's surface at places of a world's generated ground, where
/// nothing pushes or hollows it: what the far view shows, and what stands
/// on ground too far off to have its own blocks is stood on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surfaces {
    /// How high the ground is at each place, in metres.
    pub heights: Vec<f32>,
    /// The block on top at each place.
    pub tops: Vec<BlockId>,
    /// How high its sea's top is, in the world's own blocks; none for a
    /// world with no sea.
    pub sea: Option<f32>,
}

/// The surface of `file`'s generated ground at each `(xs[i], zs[i])`, in
/// metres in generator `world`'s world.
pub fn surfaces(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen, xs: &[f32], zs: &[f32]) -> Surfaces {
    let mut heights = vec![0.0; xs.len().min(zs.len())];
    world.heights(xs, zs, &mut heights);
    let columns = columns::columns_at(file, world, 0, (xs, zs, &heights));
    let (sea, sea_top) = columns::sea_of(file, generate, world, 0);
    Surfaces {
        tops: columns.iter().map(|column| column.layers[0].0).collect(),
        heights,
        sea: (sea != AIR).then_some(sea_top),
    }
}

/// How high the top of `file`'s generated sea is over its world's corner, in
/// metres; none for a world with no sea block.
pub fn sea_top(file: &BlockFile, generate: &BlockGenerate, world: &WorldGen) -> Option<f32> {
    let (sea, top) = columns::sea_of(file, generate, world, 0);
    (sea != AIR).then(|| top * file.block_size.max(0.01))
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

#[cfg(test)]
mod tests {
    use super::*;
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
                ..BlockGenerate::default()
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
    fn no_piece_of_ground_is_left_loose_in_the_air_where_caves_and_overhangs_cut_under_it() {
        // Steep ground pushed out in overhangs, over caves, great halls and wide tunnels among them.
        let (file, world) = generated(1.0, 8.0);
        let map = file.make_ground(Some(&world));
        let (side, low, high) = (64, -40, 400);
        let solid = |at: [i32; 3]| map.get(at) != AIR;
        // Every piece of ground joined to nothing but itself, within the map and over its foot: none as small as
        // loose rock, wherever it is. What is known held up, by a piece looked at before, holds up what meets it.
        let mut known: std::collections::HashMap<[i32; 3], bool> = std::collections::HashMap::new();
        let mut loose = Vec::new();
        for y in low + 1..high {
            for z in 0..side {
                for x in 0..side {
                    if !solid([x, y, z]) || known.contains_key(&[x, y, z]) {
                        continue;
                    }
                    let mut piece = vec![[x, y, z]];
                    let mut within = std::collections::HashSet::from([[x, y, z]]);
                    let (mut next, mut held) = (0, false);
                    while next < piece.len() && !held && piece.len() <= 300 {
                        let p = piece[next];
                        next += 1;
                        for w in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
                            let q = [p[0] + w[0], p[1] + w[1], p[2] + w[2]];
                            // Past the map's sides or its foot, it may be held up: what this world has not made.
                            let past = q[0] < 0 || q[2] < 0 || q[0] >= side || q[2] >= side || q[1] <= low;
                            if past || known.get(&q) == Some(&true) {
                                held = true;
                                break;
                            }
                            if solid(q) && within.insert(q) {
                                piece.push(q);
                            }
                        }
                    }
                    let held = held || piece.len() > 256;
                    if !held {
                        loose.push((piece[0], piece.len()));
                    }
                    for p in piece {
                        known.insert(p, held);
                    }
                }
            }
        }
        assert!(loose.is_empty(), "loose pieces of ground: {loose:?}");
    }

    #[test]
    fn where_biomes_keep_to_regions_each_column_is_its_regions_biome() {
        use crate::generate::{BiomeBlocks, BiomeDef, Biomes, Regions, WorldBiomes};
        let (mut file, world) = generated(0.0, 0.0);
        file.blocks.truncate(3);
        for name in ["Moss", "Peat", "Sand"] {
            file.blocks.push(BlockKind::colored(name, [0.5; 3]));
        }
        // Three biomes by warmth, as regions 64 m apart, so the 128 m square holds several.
        let biome = |temperature: [f32; 2]| BiomeDef { temperature, humidity: [0.0, 1.0], ..BiomeDef::default() };
        let list = vec![biome([-60.0, -8.0]), biome([-8.0, -4.0]), biome([-4.0, 60.0])];
        let blocks: Vec<BiomeBlocks> = (4..=6).map(|top| BiomeBlocks { top, ..BiomeBlocks::default() }).collect();
        let biomes = Biomes::new(list, 15.0, 0.3, world.seed());
        let regions = std::sync::Arc::new(Regions::new(64.0, 16.0, biomes.clone(), std::sync::Arc::new(world.clone())));
        let placed = WorldBiomes { regions: Some(regions.clone()), biomes, blocks };
        file.generate.as_mut().unwrap().placed = Some(std::sync::Arc::new(placed));
        let map = file.make_ground(Some(&world));
        let position = file.generate.as_ref().unwrap().position;
        let mut seen = std::collections::BTreeSet::new();
        for z in 0..64 {
            for x in 0..64 {
                let top = (-100..300).rev().map(|y| map.get([x, y, z])).find(|&block| block != AIR).unwrap();
                let (px, pz) = (position[0] + (x as f32 + 0.5) * 2.0, position[1] + (z as f32 + 0.5) * 2.0);
                let region = regions.region(px, pz).unwrap();
                assert_eq!(top, 4 + region as BlockId, "column ({x}, {z}) is not its region's");
                seen.insert(region);
            }
        }
        assert!(seen.len() >= 2, "{seen:?}");
    }

    #[test]
    fn where_the_land_lies_in_levels_it_climbs_in_long_slopes_without_snow_and_the_mountains_are_gone() {
        use crate::generate::{
            BiomeBlocks, BiomeDef, Biomes, Graph, LevelPlan, NodeKind, Regions, WorldBiomes,
            nodes::{Levels, Noise, Output},
        };
        let (mut file, _) = generated(0.0, 0.0);
        // Mountains from 20 m to 300 m, all of it over the sea.
        let mut graph = Graph::default();
        let hills = graph.add(NodeKind::Noise(Noise { size: 200.0, octaves: 3, low: 20.0, high: 300.0, ..Noise::default() }), [0.0; 2]);
        let output = graph.add(NodeKind::Output(Output::default()), [0.0; 2]);
        assert!(graph.connect(hills, 0, output, Output::HEIGHT));
        let world = WorldGen::new(5, 1.0, 0.0, graph);
        file.blocks.truncate(3);
        file.blocks.push(BlockKind::colored("Grass", [0.3, 0.5, 0.2]));
        // Snow over 20 m, which counts from the escarpments' ground: none on it, however high it climbs.
        let cover = BlockCover { temperature: None, humidity: None, height: Some([20.0, 10_000.0]), depth: 1 };
        file.blocks.push(BlockKind { cover: Some(cover), ..BlockKind::colored("Snow", [0.95; 3]) });
        // Areas 90 m across, the land climbing from 4 m to 64 m in escarpments, open plains alone, over the mountains.
        let levels = Levels {
            size: 90.0,
            levels: 3,
            low: 4.0,
            high: 64.0,
            rolling: 0.0,
            steps: 0.0,
            canyons: 0.0,
            spires: 0.0,
            peaks: 0.0,
            basins: 0.0,
            coast: 0.5,
            ..Levels::default()
        };
        let list = vec![BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() }];
        let biomes = Biomes::new(list, 15.0, 0.3, world.seed());
        let regions = Regions::new(90.0, 16.0, biomes.clone(), std::sync::Arc::new(world.clone()));
        let regions = std::sync::Arc::new(regions.with_levels(LevelPlan { keep: None, ..levels.plan() }));
        let blocks = vec![BiomeBlocks { top: 4, ..BiomeBlocks::default() }];
        let generate = file.generate.as_mut().unwrap();
        generate.levels = Some(levels);
        generate.placed = Some(std::sync::Arc::new(WorldBiomes { regions: Some(regions), biomes, blocks }));
        let map = file.make_ground(Some(&world));
        let top = |x: i32, z: i32| (-100..400).rev().find(|&y| map.get([x, y, z]) != AIR).unwrap();
        let tops: Vec<i32> = (0..64 * 64).map(|k| top(k % 64, k / 64)).collect();
        let mut counts = std::collections::BTreeMap::new();
        tops.iter().for_each(|&y| *counts.entry(y).or_insert(0) += 1);
        // The mountains, up to 300 m, are gone: the land lies between the lowest level, 4 m, and the highest, 64 m,
        // with an escarpment's rise of up to about 36 m over it, in blocks of 2 m.
        assert!(tops.iter().all(|&y| (-2..=52).contains(&y)), "{counts:?}");
        assert!(counts.len() > 4, "the land climbs: {counts:?}");
        // Mostly long even slopes: side by side, columns mostly within a block of each other.
        let pairs = (0..64 * 64).filter(|&k| (k + 1) % 64 != 0).collect::<Vec<_>>();
        let gentle = pairs.iter().filter(|&&k| (tops[k] - tops[k + 1]).abs() <= 1).count();
        assert!(gentle * 10 > pairs.len() * 8, "{gentle} of {} side by side alike: {counts:?}", pairs.len());
        // Snow over 20 m counts from the escarpments' ground, so none lies on it, high or low.
        let snowed: Vec<(usize, i32)> =
            (0..64 * 64).filter(|&k| map.get([k as i32 % 64, tops[k], k as i32 / 64]) == 5).map(|k| (k, tops[k])).take(12).collect();
        assert!(snowed.is_empty(), "snow on the escarpments' ground: {snowed:?}");
    }

    #[test]
    fn cliffs_show_their_biomes_rock_in_level_layers_of_its_blocks() {
        use crate::generate::{BiomeBlocks, BiomeDef, Biomes, RockLayer, RockLayers, Strata, WorldBiomes};
        use crate::generate::{
            Graph, NodeKind,
            nodes::{Noise, Output, Terrace},
        };
        let (mut file, _) = generated(0.0, 0.0);
        // Ground in sheer steps 8 m high, so its rock is bared in walls.
        let mut graph = Graph::default();
        let hills = graph.add(NodeKind::Noise(Noise { size: 120.0, octaves: 2, low: 0.0, high: 60.0, ..Noise::default() }), [0.0; 2]);
        let steps = graph.add(NodeKind::Terrace(Terrace { step: 8.0, sharpness: 0.95 }), [0.0; 2]);
        let output = graph.add(NodeKind::Output(Output::default()), [0.0; 2]);
        assert!(graph.connect(hills, 0, steps, 0) && graph.connect(steps, 0, output, Output::HEIGHT));
        let world = WorldGen::new(5, 1.0, -100.0, graph);
        file.blocks.truncate(3);
        for name in ["Sandstone", "Shale"] {
            file.blocks.push(BlockKind::colored(name, [0.6; 3]));
        }
        let layer = |name: &str, thickness| RockLayer { block: Some(name.into()), thickness };
        let rock = RockLayers { tilt: 0.0, wander: 0.0, layers: vec![layer("Sandstone", 6.0), layer("Shale", 4.0)] };
        let number = |name: &str| if name == "Sandstone" { 4 } else { 5 };
        let strata = std::sync::Arc::new(Strata::of(&rock, number).unwrap());
        let blocks = BiomeBlocks { top: 1, steep: 4, steep_slope: 0.7, strata: Some(strata.clone()), ..BiomeBlocks::default() };
        let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let biomes = Biomes::new(vec![land], 15.0, 0.3, world.seed());
        file.generate.as_mut().unwrap().placed = Some(std::sync::Arc::new(WorldBiomes { regions: None, biomes, blocks: vec![blocks] }));
        let map = file.make_ground(Some(&world));
        let generate = file.generate.as_ref().unwrap();
        let (mut shown, mut wrong) = ([0usize; 2], 0);
        for z in 1..63 {
            for x in 1..63 {
                let (px, pz) = (generate.position[0] + (x as f32 + 0.5) * 2.0, generate.position[1] + (z as f32 + 0.5) * 2.0);
                for y in -100..300 {
                    let block = map.get([x, y, z]);
                    if block != 4 && block != 5 {
                        continue;
                    }
                    // Every block of rock is the layer at its height.
                    if block != strata.block((y as f32 + 0.5) * 2.0 - strata.lift(px, pz)) {
                        wrong += 1;
                    }
                    let open = [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|[dx, dz]| map.get([x + dx, y, z + dz]) == AIR);
                    if open {
                        shown[usize::from(block - 4)] += 1;
                    }
                }
            }
        }
        assert_eq!(wrong, 0, "rock not in its layer");
        // The faces of the steep ground show both layers, not the one block top to bottom.
        assert!(shown[0] > 100 && shown[1] > 50, "{shown:?} faces of sandstone and shale");
        // A world without end makes its ground from its blocks as far as the ground is laid with them: the layers'
        // blocks among them.
        assert_eq!(file.for_ground().blocks.len(), 5);
    }

    #[test]
    fn generated_ground_takes_its_blocks_from_the_biome_each_place_is() {
        use crate::generate::{BiomeBlocks, BiomeDef, Biomes, WorldBiomes};
        let (mut file, world) = generated(0.0, 0.0);
        // No snow cover: the biomes say what lies on top. Blocks 1 to 3 are the world's grass, dirt and stone;
        // 4 to 7 are moss, peat, ice and scree.
        file.blocks.truncate(3);
        for name in ["Moss", "Peat", "Ice", "Scree"] {
            file.blocks.push(BlockKind::colored(name, [0.5; 3]));
        }
        let without = file.make_ground(Some(&world));
        // A colder biome of ice, with scree on its steeper ground, and a milder one of moss over peat: split where
        // half this stretch of ground is colder, and at a slope a fair part of it is steeper than.
        let columns = |z: i32| (0..64).map(move |x| (500.0 + (x as f32 + 0.5) * 2.0, 800.0 + (z as f32 + 0.5) * 2.0));
        let mut warmths: Vec<f32> = (0..64)
            .flat_map(columns)
            .map(|(px, pz)| world.climate(px, pz, world.height_at(px, pz)).0)
            .collect();
        warmths.sort_by(f32::total_cmp);
        let slope_at = |(px, pz): (f32, f32)| {
            let height = world.height_at(px, pz);
            let (dx, dz) = (world.height_at(px + 2.0, pz) - height, world.height_at(px, pz + 2.0) - height);
            (dx * dx + dz * dz).sqrt() / 2.0
        };
        let mut slopes: Vec<f32> = (0..64).flat_map(columns).map(slope_at).collect();
        slopes.sort_by(f32::total_cmp);
        let (split, steep) = (warmths[warmths.len() / 2], slopes[slopes.len() * 3 / 4]);
        let cold = BiomeDef { temperature: [-60.0, split], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let mild = BiomeDef { temperature: [split, 40.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let blocks = vec![
            BiomeBlocks { top: 6, steep: 7, steep_slope: steep, ..BiomeBlocks::default() },
            BiomeBlocks { top: 4, under: 5, under_depth: 2, ..BiomeBlocks::default() },
        ];
        let biomes = Biomes::new(vec![cold, mild], 15.0, 0.3, world.seed()).wandering(0.0, 48.0);
        let placed = WorldBiomes { regions: None, biomes, blocks };
        file.generate.as_mut().unwrap().placed = Some(std::sync::Arc::new(placed));
        let map = file.make_ground(Some(&world));
        let surface = |map: &BlockMap, x: i32, z: i32| (-40..400).rev().find(|&y| map.get([x, y, z]) != AIR);
        let (mut ice, mut scree, mut moss) = (0, 0, 0);
        for z in 0..64 {
            for x in 0..64 {
                let (px, pz) = (500.0 + (x as f32 + 0.5) * 2.0, 800.0 + (z as f32 + 0.5) * 2.0);
                let height = world.height_at(px, pz);
                let (dx, dz) = (world.height_at(px + 2.0, pz) - height, world.height_at(px, pz + 2.0) - height);
                let slope = (dx * dx + dz * dz).sqrt() / 2.0;
                let top = surface(&map, x, z).expect("ground in every column");
                assert_eq!(surface(&without, x, z), Some(top), "the ground's shape is the generator's, biomes or none");
                let block = map.get([x, top, z]);
                let warmth = world.climate(px, pz, height).0;
                // Within a hair of the split or the slope, either is right.
                if (warmth - split).abs() < 1e-3 || (slope - steep).abs() < 1e-3 {
                    continue;
                }
                let cold = warmth < split;
                let wanted = match (cold, slope >= steep) {
                    (true, true) => 7,
                    (true, false) => 6,
                    (false, _) => 4,
                };
                assert_eq!(block, wanted, "column ({x}, {z}): {height} m, slope {slope}");
                (ice, scree, moss) = (ice + usize::from(block == 6), scree + usize::from(block == 7), moss + usize::from(block == 4));
                // Under moss, two blocks of peat and then the world's stone; under ice, the world's dirt.
                if block == 4 {
                    assert_eq!((map.get([x, top - 1, z]), map.get([x, top - 2, z])), (5, 5));
                    assert_eq!(map.get([x, top - 4, z]), 3);
                } else if block == 6 {
                    assert_eq!(map.get([x, top - 1, z]), 2);
                }
            }
        }
        assert!(ice > 100 && scree > 20 && moss > 100, "{ice} of ice, {scree} of scree, {moss} of moss");
        // The file says which biomes, and what was read of them is not written.
        file.generate.as_mut().unwrap().biomes = vec!["Biomes/Cold.biome".into()];
        let text = file.to_text();
        assert!(text.contains("biomes = [\"Biomes/Cold.biome\"]") && !text.contains("placed"), "{text}");
        assert_eq!(BlockFile::parse(&text).unwrap().generate.unwrap().placed, None);
    }

    #[test]
    fn a_biomes_vines_hang_under_ledges_and_cave_roofs_through_the_open_air_the_same_from_any_column() {
        use crate::generate::{BiomeBlocks, BiomeDef, Biomes, WorldBiomes};
        // Ground with caves and overhangs, of metre blocks, one biome all over: its vines are block 4.
        let (mut file, world) = generated(0.5, 9.0);
        file.block_size = 1.0;
        // Deep enough under the ground for caves.
        file.generate.as_mut().unwrap().bottom = -150;
        file.blocks.truncate(3);
        file.blocks.push(BlockKind::colored("Vine", [0.2, 0.5, 0.15]));
        let without = file.make_ground(Some(&world));
        let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let place = |amount: f32, depth: f32| {
            let vines = BiomeBlocks { vine: 4, vine_amount: amount, vine_length: [2, 6], vine_depth: depth, ..BiomeBlocks::default() };
            let biomes = Biomes::new(vec![land.clone()], 15.0, 0.3, world.seed());
            Some(std::sync::Arc::new(WorldBiomes { regions: None, biomes, blocks: vec![vines] }))
        };
        file.generate.as_mut().unwrap().placed = place(0.4, 30.0);
        let map = file.make_ground(Some(&world));
        let mut vines = Vec::new();
        for (key, chunk) in map.chunks().filter(|(_, chunk)| chunk.may_hold(|block| block == 4)) {
            for i in 0..CHUNK * CHUNK * CHUNK {
                let (x, y, z) = (i % CHUNK, i / (CHUNK * CHUNK), i / CHUNK % CHUNK);
                if chunk.get(x, y, z) == 4 {
                    vines.push([key[0] * 32 + x as i32, key[1] * 32 + y as i32, key[2] * 32 + z as i32]);
                }
            }
        }
        assert!(vines.len() > 60, "{} blocks of vine", vines.len());
        // Nothing else changed: a vine is only where there was open air.
        for (key, chunk) in map.chunks() {
            for i in 0..CHUNK * CHUNK * CHUNK {
                let (x, y, z) = (i % CHUNK, i / (CHUNK * CHUNK), i / CHUNK % CHUNK);
                let at = [key[0] * 32 + x as i32, key[1] * 32 + y as i32, key[2] * 32 + z as i32];
                let (now, was) = (chunk.get(x, y, z), without.get(at));
                assert!(now == was || (now == 4 && was == AIR), "{at:?}: {was} became {now}");
            }
        }
        // Each hangs from ground: up its column there are only vines until ground, within six blocks.
        let mut beside_a_wall = 0;
        for &[x, y, z] in &vines {
            let over = (1..=6).map(|up| map.get([x, y + up, z])).find(|&block| block != 4);
            assert!(matches!(over, Some(block) if block != AIR), "the vine at {:?} hangs from {over:?}", [x, y, z]);
            let walls = [[1, 0], [-1, 0], [0, 1], [0, -1]];
            beside_a_wall += usize::from(walls.iter().any(|d| ![AIR, 4].contains(&map.get([x + d[0], y, z + d[1]]))));
        }
        assert!(beside_a_wall * 3 > vines.len(), "{beside_a_wall} of {} are beside a wall", vines.len());
        // A chunk column made alone has the vines it has among its neighbours.
        let generate = file.generate.clone().unwrap();
        for column in [[0, 0], [1, 0], [1, 1]] {
            for (key, chunk) in column_chunks(&file, &generate, &world, 0, column, Some([64, 64])) {
                assert_eq!(Some(&chunk), map.chunk(key).map(|chunk| &**chunk), "chunk {key:?}");
            }
        }
        // Fewer with a smaller amount, none below the depth they keep to, none at all with no block for them.
        let count = |amount: f32, depth: f32| {
            let mut file = file.clone();
            file.generate.as_mut().unwrap().placed = place(amount, depth);
            let map = file.make_ground(Some(&world));
            let surface = |x: i32, z: i32| (-150..400).rev().find(|&y| ![AIR, 4].contains(&map.get([x, y, z]))).unwrap();
            let mut found = Vec::new();
            for (key, chunk) in map.chunks().filter(|(_, chunk)| chunk.may_hold(|block| block == 4)) {
                for i in 0..CHUNK * CHUNK * CHUNK {
                    let (x, y, z) = (i % CHUNK, i / (CHUNK * CHUNK), i / CHUNK % CHUNK);
                    if chunk.get(x, y, z) == 4 {
                        let (x, z) = (key[0] * 32 + x as i32, key[2] * 32 + z as i32);
                        found.push(surface(x, z) - (key[1] * 32 + y as i32));
                    }
                }
            }
            found
        };
        assert!(count(0.1, 30.0).len() * 2 < vines.len());
        // Hung no deeper than 4 m under the generator's own surface, and no longer than six blocks.
        let shallow = count(0.4, 4.0);
        assert!(!shallow.is_empty() && shallow.len() < vines.len(), "{} near the surface", shallow.len());
        assert!(count(0.0, 30.0).is_empty());
        // The far rings, of larger blocks, have none.
        let far = column_chunks(&file, &generate, &world, 1, [0, 0], None);
        assert!(far.iter().all(|(_, chunk)| !chunk.may_hold(|block| block == 4)));
    }

    #[test]
    fn a_chasms_floor_takes_the_biome_kept_to_sunk_ground_and_its_roof_the_lands() {
        use crate::generate::{
            BiomeBlocks, BiomeDef, Biomes, Graph, NodeKind, WorldBiomes,
            nodes::{ChasmKind, Chasms, Constant, Output, Roof},
        };
        // Flat land 100 m up, sinkholes cut 80 m and more into it, half roofed over at the land's height.
        let mut graph = Graph::default();
        let land = graph.add(NodeKind::Constant(Constant { value: 100.0 }), [0.0; 2]);
        let chasms = Chasms {
            shape: ChasmKind::Sinkholes,
            often: 0.9,
            width_from: 70.0,
            width_to: 100.0,
            depth_from: 80.0,
            depth_to: 90.0,
            floor: 0.0,
            ..Chasms::default()
        };
        let chasms = graph.add(NodeKind::Chasms(chasms), [200.0, 0.0]);
        let roof = graph
            .add(NodeKind::Roof(Roof { cover: 1.0, openings: 0.3, pillars: 0.0, ..Roof::default() }), [400.0, 0.0]);
        let output = graph.add(NodeKind::Output(Output::default()), [600.0, 0.0]);
        assert!(graph.connect(land, 0, chasms, Chasms::GROUND));
        assert!(graph.connect(chasms, Chasms::HEIGHT, output, Output::HEIGHT));
        assert!(graph.connect(chasms, Chasms::SUNK, output, Output::SUNK));
        assert!(graph.connect(chasms, Chasms::RIM, roof, Roof::TOP));
        assert!(graph.connect(roof, 0, output, Output::VOLUME));
        let world = WorldGen::new(3, 1.0, -50.0, graph);
        assert!(world.faults().is_empty(), "{:?}", world.faults());
        let mut file = BlockFile {
            block_size: 2.0,
            ground: Ground { size: [128, 128], ..Ground::default() },
            generate: Some(BlockGenerate { position: [0.0, 0.0], bottom: -20, sea: AIR, ..BlockGenerate::default() }),
            ..BlockFile::default()
        };
        // Blocks 1 to 3 are the world's grass, dirt and stone; 4 is moss and 5 fern.
        for name in ["Moss", "Fern"] {
            file.blocks.push(BlockKind::colored(name, [0.5; 3]));
        }
        let land = BiomeDef { temperature: [-60.0, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let hollow = BiomeDef { sunk: Some([40.0, 2000.0]), ..land.clone() };
        let blocks =
            vec![BiomeBlocks { top: 4, ..BiomeBlocks::default() }, BiomeBlocks { top: 5, ..BiomeBlocks::default() }];
        let biomes = Biomes::new(vec![land, hollow], 15.0, 0.3, world.seed()).wandering(0.0, 48.0);
        file.generate.as_mut().unwrap().placed = Some(std::sync::Arc::new(WorldBiomes { regions: None, biomes, blocks }));
        let map = file.make_ground(Some(&world));
        let (mut floors, mut roofs, mut open) = (0, 0, 0);
        for z in 0..128 {
            for x in 0..128 {
                let (px, pz) = ((x as f32 + 0.5) * 2.0, (z as f32 + 0.5) * 2.0);
                let height = world.height_at(px, pz);
                // Down the column: the first ground under the sky, and the first under that ground's air.
                let solid: Vec<i32> = (-20..80).rev().filter(|&y| map.get([x, y, z]) != AIR).collect();
                let top = solid[0];
                if height > 99.0 {
                    assert_eq!(map.get([x, top, z]), 4, "the land at ({x}, {z})");
                    continue;
                }
                if height > 30.0 {
                    continue;
                }
                let floor = (-20..top).rev().find(|&y| map.get([x, y, z]) != AIR && map.get([x, y + 1, z]) == AIR);
                if top > 40 {
                    // Roofed: moss on the roof as on the land, fern on the floor under it.
                    assert_eq!(map.get([x, top, z]), 4, "a roof at ({x}, {z})");
                    let floor = floor.expect("a floor under the roof");
                    assert_eq!(map.get([x, floor, z]), 5, "a cavern's floor at ({x}, {z})");
                    (roofs, floors) = (roofs + 1, floors + 1);
                } else {
                    assert_eq!(map.get([x, top, z]), 5, "an open floor at ({x}, {z})");
                    (open, floors) = (open + 1, floors + 1);
                }
            }
        }
        assert!(floors > 300 && roofs > 50 && open > 20, "{floors} floors, {roofs} roofed, {open} open");
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
            [0xc4a8_ae72_922e_e33a, 0x06fa_ee9e_a714_aca3, 0x56ad_b1e9_cadc_ac61],
            "generated blocks changed: were they meant to? {hashes:#x?}"
        );
    }

    /// Hills, or level rock, with a network of cave rooms taken out, and a world of metre blocks on it.
    fn networked(layout: crate::generate::nodes::Laid, level: bool, caves: bool) -> (BlockFile, WorldGen) {
        use crate::generate::{
            Graph, NodeKind, Room, RoomRules,
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
            // Rooms made by rules among a room of set shapes, as a prefab's is.
            let rooms = vec![Room::made(RoomRules::default()), Room::chamber()];
            let network = CaveNetwork { layout, entrances: 0.5, rooms, ..CaveNetwork::default() };
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
                ..BlockGenerate::default()
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

    /// The Caves starter's world of blocks, its lowest block `bottom`.
    fn tall(bottom: i32, smooth: bool) -> (BlockFile, WorldGen) {
        let (file, _) = networked(crate::generate::nodes::Laid::Endless, false, true);
        let generate = BlockGenerate { bottom, ..file.generate.clone().unwrap() };
        let world = crate::generate::starters().into_iter().find(|starter| starter.name == "Caves").unwrap();
        (BlockFile { smooth, generate: Some(generate), ..file }, world.world(11, 1.0))
    }

    #[test]
    fn a_tall_column_leaves_out_deep_rock_and_is_the_blocks_it_is_asked_whole() {
        for smooth in [false, true] {
            let (file, world) = tall(-520, smooth);
            let generate = file.generate.clone().unwrap();
            let volume = world.volume().unwrap();
            assert!(volume.spanned() && volume.depth() == 0.0, "caves say where they are: {}", volume.depth());
            for at in [[0, 0], [3, 5], [-4, 2]] {
                let made = column_chunks(&file, &generate, &world, 0, at, None);
                columns::ALL_ASKED.set(true);
                let whole = column_chunks(&file, &generate, &world, 0, at, None);
                columns::ALL_ASKED.set(false);
                assert_eq!(made.len(), whole.len());
                let mut rock = 0;
                for ((key, chunk), (whole_key, whole)) in made.iter().zip(&whole) {
                    assert_eq!(key, whole_key);
                    assert!(chunk == whole, "smooth {smooth}: chunk {key:?} is not what it is asked whole");
                    rock += usize::from(chunk.bits() == 0 && chunk.is_whole());
                }
                // Three layers of caves reach about 200 blocks down; most of what is under them is whole rock.
                assert!(made.len() >= 17 && rock >= 6, "{rock} chunks of rock among {}", made.len());
            }
        }
    }

    #[test]
    #[ignore = "a timing, to be read: run it optimised"]
    fn timing_of_a_chunk_column_256_512_and_1024_blocks_tall() {
        let time = |file: &BlockFile, world: &WorldGen, all: bool| {
            let generate = file.generate.clone().unwrap();
            let mut least = f32::MAX;
            columns::ALL_ASKED.set(all);
            for _ in 0..3 {
                let started = std::time::Instant::now();
                for at in [[0, 0], [1, 0], [0, 1], [1, 1], [3, 5], [-4, 2], [7, -3], [2, 8]] {
                    std::hint::black_box(column_chunks(file, &generate, world, 0, at, None));
                }
                least = least.min(started.elapsed().as_secs_f32() * 1000.0 / 8.0);
            }
            columns::ALL_ASKED.set(false);
            least
        };
        for blocks in [64, 256, 512, 1024] {
            // The ground's hills stand about 100 blocks over the world's corner.
            let (cubes, world) = tall(100 - blocks, false);
            let (smooth, _) = tall(100 - blocks, true);
            let plain = world.with_graph(world.graph().without_volume());
            println!(
                "{blocks} blocks tall: caves in cubes {:.2} ms (all asked {:.2}), smooth {:.2} ms (all asked {:.2}); \
                 no caves {:.2} ms (all put {:.2})",
                time(&cubes, &world, false),
                time(&cubes, &world, true),
                time(&smooth, &world, false),
                time(&smooth, &world, true),
                time(&cubes, &plain, false),
                time(&cubes, &plain, true),
            );
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
    #[ignore = "a timing, to be read: run it optimised"]
    fn timing_of_a_chunk_column_of_rooms_made_by_rules_against_rooms_of_set_shapes() {
        use crate::generate::{NodeKind, Room, RoomRules, nodes::Laid, starters};
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
        let (file, mixed) = networked(Laid::Endless, false, true);
        // The same hills and network with these rooms listed.
        let with = |rooms: Vec<Room>| {
            let mut graph = mixed.graph().clone();
            for node in &mut graph.nodes {
                if let NodeKind::CaveNetwork(network) = &mut node.kind {
                    network.rooms = rooms.clone();
                }
            }
            mixed.with_graph(graph)
        };
        let caves = starters().into_iter().find(|starter| starter.name == "Caves").unwrap().world(11, 1.0);
        let kinds = [
            ("a room of set shapes", with(vec![Room::chamber()])),
            ("rooms made by rules", with(vec![Room::made(RoomRules::default())])),
            ("the Caves starter", caves.clone()),
            ("the Caves starter's ground and tunnels with a room of set shapes", {
                let mut graph = caves.graph().clone();
                for node in &mut graph.nodes {
                    if let NodeKind::CaveNetwork(network) = &mut node.kind {
                        network.rooms = vec![Room::chamber()];
                    }
                }
                caves.with_graph(graph)
            }),
        ];
        let smooth = BlockFile { smooth: true, ..file.clone() };
        for (name, world) in &kinds {
            let (cubes, smooth) = (time(&file, world), time(&smooth, world));
            println!("{name}: a chunk column of cubes {cubes:.2} ms, smooth {smooth:.2} ms");
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
            [0x5391_a2b4_0bdd_5252, 0x06fa_ee9e_a714_aca3, 0x5346_5916_77b2_355f],
            "overhanging blocks changed: were they meant to? {hashes:#x?}"
        );
    }

    #[test]
    fn a_generated_river_runs_in_blocks_of_the_seas_water_between_banks_flowing_its_way() {
        let world = crate::generate::starters().into_iter().find(|s| s.name == "Rolling Hills").unwrap().world(7, 1.0);
        // A river's water, a few metres wide, somewhere.
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..300 * 300).map(|at| ((at % 300) as f32 * 6.0, (at / 300) as f32 * 6.0)).unzip();
        let (water, _) = world.water(&xs, &zs).expect("rivers wired in");
        let flowing = |at: usize| world.current(xs[at], zs[at]).is_some_and(|[x, z]| x * x + z * z > 0.1);
        let river = |at: &usize| water[*at] > world.sea_level() + 2.0 && flowing(*at);
        let at = (0..xs.len()).find(river).expect("a river somewhere");
        let mut file = BlockFile {
            ground: Ground { size: [48, 48], ..Ground::default() },
            generate: Some(BlockGenerate {
                position: [xs[at] - 24.0, zs[at] - 24.0],
                bottom: -20,
                ..BlockGenerate::default()
            }),
            ..BlockFile::default()
        };
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let sea = file.blocks.len() as BlockId;
        file.generate.as_mut().unwrap().sea = sea;
        let map = file.make_ground(Some(&world));
        // Water in the river's column, its top where the river's is, over the sea.
        let top = (-20..300).rev().find(|&y| map.get([24, y, 24]) != AIR).expect("the column");
        assert_eq!(map.get([24, top, 24]), sea, "the river's water");
        assert!(((top + 1) as f32 - water[at]).abs() <= 1.0, "its top at {} for a river at {}", top + 1, water[at]);
        // Its blocks keep which way it flows.
        let (mut wet, mut flowing, mut walls) = (0, 0, 0);
        for y in -20..300 {
            for z in 1..47 {
                for x in 1..47 {
                    if map.get([x, y, z]) != sea {
                        continue;
                    }
                    wet += 1;
                    flowing += usize::from(crate::blocks::flow::current_of(map.fill([x, y, z])).is_some());
                    // No water stands beside air with no water under it: a wall of it. Beside air over lower water,
                    // it is the river's surface stepping down.
                    let beside = [[x - 1, y, z], [x + 1, y, z], [x, y, z - 1], [x, y, z + 1]];
                    let wall = |&&[x, y, z]: &&[i32; 3]| map.get([x, y, z]) == AIR && map.get([x, y - 1, z]) != sea;
                    walls += beside.iter().filter(wall).count();
                }
            }
        }
        assert!(wet > 50 && flowing * 2 > wet, "{flowing} of {wet} blocks of the river flow");
        assert!(walls * 100 <= wet, "{walls} sides of water against air, of {wet} blocks");
    }

    #[test]
    fn generated_ground_lays_slabs_where_it_lies_half_a_block_over_a_whole_one_so_it_rises_in_half_steps() {
        // Rolling hills, no caves: every column's top is the generator's height, to the half block.
        let world = Preset::RollingHills.world();
        let made = |slabs: bool| {
            let mut file = BlockFile {
                ground: Ground { size: [64, 64], ..Ground::default() },
                generate: Some(BlockGenerate {
                    position: [300.0, -200.0],
                    bottom: -20,
                    caves: 0.0,
                    slabs,
                    ..BlockGenerate::default()
                }),
                ..BlockFile::default()
            };
            // The grass and the dirt each with its slab after the world's own three.
            let (grass, dirt) = (file.ground.top, file.ground.under);
            for block in [grass, dirt] {
                let slab = BlockKind::slab_of(file.kind(block).unwrap());
                file.blocks.push(slab);
            }
            let mut halves = vec![AIR; file.blocks.len() + 1];
            (halves[usize::from(grass)], halves[usize::from(dirt)]) = (4, 5);
            file.generate.as_mut().unwrap().halves = Some(std::sync::Arc::new(halves));
            let map = file.make_ground(Some(&world));
            (file, map)
        };
        let (file, map) = made(true);
        assert_eq!(file.kind(4).map(|kind| kind.shape), Some(crate::blocks::block::BlockShape::Slab));
        let top = |map: &BlockMap, x: i32, z: i32| (-20..300).rev().find(|&y| map.get([x, y, z]) != AIR).unwrap();
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..64 * 64).map(|at| (300.0 + (at % 64) as f32 + 0.5, -200.0 + (at / 64) as f32 + 0.5)).unzip();
        let water = world.water(&xs, &zs).map(|(water, _)| water);
        let (mut slabs, mut checked) = (0, 0);
        for at in 0..64 * 64 {
            let (x, z) = ((at % 64) as i32, (at / 64) as i32);
            let surface = world.height_at(xs[at], zs[at]);
            let y = top(&map, x, z);
            let half = (surface - 0.25).floor();
            // Where a river runs, its water lies on whole blocks.
            if water.as_ref().is_some_and(|water| water[at] > half) {
                continue;
            }
            checked += 1;
            let block = map.get([x, y, z]);
            if surface - half < 0.75 {
                // Within a quarter of a block of a block's middle: that block is a slab, over whole ground.
                assert_eq!((y, block), (half as i32, 4), "column ({x}, {z}) at {surface}");
                assert!([1, 2].contains(&map.get([x, y - 1, z])), "whole ground under the slab");
                slabs += 1;
            } else {
                // Near a whole block: the ground is whole to it, as it always was.
                assert_eq!((y, block), ((surface - 0.5).ceil() as i32 - 1, 1), "column ({x}, {z}) at {surface}");
            }
        }
        // About half the columns, of those checked.
        assert!(checked > 3000 && slabs * 4 > checked && slabs * 4 < checked * 3, "{slabs} slabs of {checked}");
        // Off, there are none; and the slabs are written as such and read back.
        let (_, without) = made(false);
        let none = (0..64 * 64).all(|at| without.get([at % 64, top(&without, at % 64, at / 64), at / 64]) != 4);
        assert!(none, "no slab with the world's slabs off");
        let text = file.to_text();
        assert!(text.contains("slabs = true") && text.contains("slab = true") && !text.contains("halves"), "{text}");
        let read = BlockFile::parse(&text).unwrap();
        assert_eq!(read.blocks, file.blocks);
    }

    #[test]
    fn the_far_view_rises_in_half_steps_as_near_by_and_its_ground_keeps_the_blocks_it_is_laid_with() {
        use crate::blocks::generate::far::{far_chunks, far_tile};
        let world = Preset::RollingHills.world();
        let mut file = BlockFile {
            ground: Ground { size: [64, 64], ..Ground::default() },
            generate: Some(BlockGenerate {
                position: [300.0, -200.0],
                bottom: -20,
                caves: 0.0,
                slabs: true,
                ..BlockGenerate::default()
            }),
            ..BlockFile::default()
        };
        let grass = file.ground.top;
        file.blocks.push(BlockKind::slab_of(file.kind(grass).unwrap()));
        let slab = file.blocks.len() as BlockId;
        // A block numbered after the slab, as a player's would be.
        file.blocks.push(BlockKind::colored("Gold", [0.9, 0.8, 0.1]));
        let mut halves = vec![AIR; file.blocks.len() + 1];
        halves[usize::from(grass)] = slab;
        file.generate.as_mut().unwrap().halves = Some(std::sync::Arc::new(halves));
        // What its ground is made from keeps the slab, and not the block after it.
        assert_eq!(file.for_ground().blocks.len(), usize::from(slab));
        let generate = file.generate.clone().unwrap();
        // A far tile lays the slabs a coarse ring's chunks do, and has some.
        for (level, at) in [(2, [0, 0]), (3, [-1, 1])] {
            let tile = far_tile(&file, &generate, &world, level, at);
            let mut made = far_chunks(&tile, false);
            let mut coarse = column_chunks(&file, &generate, &world, level, at, None);
            made.sort_by_key(|(key, _)| key[1]);
            coarse.sort_by_key(|(key, _)| key[1]);
            assert_eq!(made.len(), coarse.len(), "level {level}");
            for ((key, chunk), (_, want)) in made.iter().zip(&coarse) {
                assert!(chunk == want, "chunk {key:?} of level {level}");
            }
            let slabs = made.iter().map(|(_, chunk)| {
                (0..CHUNK)
                    .flat_map(|z| (0..CHUNK).flat_map(move |y| (0..CHUNK).map(move |x| [x, y, z])))
                    .filter(|&[x, y, z]| chunk.get(x, y, z) == slab)
                    .count()
            });
            assert!(slabs.sum::<usize>() > 100, "slabs far off at level {level}");
        }
    }

    #[test]
    fn a_world_with_rivers_runs_them_down_a_generator_that_has_none_of_its_own() {
        // A landmass, as generators written before graphs are, with no water but its sea.
        let world = Preset::Archipelago.world();
        let mut graph = world.graph().clone();
        assert!(crate::generate::nodes::add_rivers(&mut graph, crate::generate::nodes::Rivers::default()));
        let rivered = world.with_graph(graph);
        // A river's water, well over the sea, somewhere.
        let (xs, zs): (Vec<f32>, Vec<f32>) =
            (0..300 * 300).map(|at| ((at % 300) as f32 * 8.0 - 1200.0, (at / 300) as f32 * 8.0 - 1200.0)).unzip();
        let (water, _) = rivered.water(&xs, &zs).expect("rivers wired in");
        let at = (0..xs.len()).find(|&at| water[at] > world.sea_level() + 4.0).expect("a river somewhere");
        let made = |rivers: bool| {
            let mut file = BlockFile {
                ground: Ground { size: [32, 32], ..Ground::default() },
                generate: Some(BlockGenerate {
                    position: [xs[at] - 16.0, zs[at] - 16.0],
                    bottom: -20,
                    caves: 0.0,
                    rivers,
                    ..BlockGenerate::default()
                }),
                ..BlockFile::default()
            };
            file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
            file.generate.as_mut().unwrap().sea = file.blocks.len() as BlockId;
            let water = file.blocks.len() as BlockId;
            let map = file.make_ground(Some(&world));
            let over_sea = (world.sea_level() + 1.0) as i32;
            let places = (over_sea..300).flat_map(|y| (0..32).flat_map(move |z| (0..32).map(move |x| [x, y, z])));
            places.filter(|&p| map.get(p) == water).count()
        };
        assert!(made(true) > 20, "{} blocks of river water over the sea", made(true));
        assert_eq!(made(false), 0, "no river with the world's rivers off");
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
}
