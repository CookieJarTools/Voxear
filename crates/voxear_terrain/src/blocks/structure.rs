//! A `.structure` file: something a world's generator builds where its
//! placement finds ground for it: a house, a well, a ruin, a tree. Its
//! blocks are a model of its own, kept in the file, or a plant grown from a
//! `.voxplant` afresh at every site, so no two trees of a wood are alike.
//! Its placement says where it stands (see `crate::generate::structures`),
//! and how the ground meets it: left as it is, built up under it, or built
//! up under it and cleared over it.
//!
//! ```toml
//! ground = "flatten"   # or "foundation", "leave"
//! sink = 1             # blocks it stands into the ground
//!
//! [placement]
//! spacing = 160.0
//! separation = 48.0
//! chance = 0.5
//! slope = 12.0
//!
//! [model]
//! size = [5, 4, 5]                                    # blocks along x, up and along z
//! palette = ["Blocks/Planks.block", "Blocks/Log.block"]
//! runs = "25:2 3:2 1:0 1:2 ..."                        # count:entry, along x, then z, then up; 0 leaves the block be
//! ```
//!
//! A world numbers the blocks a structure names as it does a biome's (see
//! [`StructureBlocks`]); building it into a chunk column needs nothing
//! else, so a chunk made alone builds the part of a structure that reaches
//! it exactly as it is built among its neighbours.

use serde::{Deserialize, Serialize};

use super::{
    chunk::{AIR, BlockId, CHUNK, Chunk},
    plant::{PlantDef, PlantPart},
};
use crate::generate::{
    Biomes, WorldGen,
    biomes::Place,
    structures::{Placement, Site, SiteGround},
};

/// How the ground meets a structure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroundFit {
    /// The ground as it is: a tree, a standing stone.
    Leave,
    /// Built up under its lowest blocks, down to the ground, so nothing
    /// stands on air on a slope.
    #[default]
    Foundation,
    /// A foundation, and the ground cleared over its footprint as high as
    /// it stands, so a hillside does not run through it.
    Flatten,
}

impl GroundFit {
    pub const ALL: [GroundFit; 3] = [GroundFit::Leave, GroundFit::Foundation, GroundFit::Flatten];

    pub fn name(self) -> &'static str {
        match self {
            GroundFit::Leave => "Leave",
            GroundFit::Foundation => "Foundation",
            GroundFit::Flatten => "Flatten",
        }
    }
}

/// A structure's own blocks: `size` blocks along x, up and along z, each an
/// entry of `palette` from 1 (a reference to a `.block`), or 0 for leaving
/// the block there be, kept as runs of the same along x, then z, then up.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StructureModel {
    pub size: [u32; 3],
    pub palette: Vec<String>,
    pub runs: String,
}

impl StructureModel {
    /// The model of `blocks`, `size` of them along x, up and along z, kept
    /// along x, then z, then up, each an entry of `palette` from 1 or 0.
    pub fn of(size: [u32; 3], palette: Vec<String>, blocks: &[u16]) -> Self {
        let mut runs = String::new();
        let mut at = 0;
        while at < blocks.len() {
            let entry = blocks[at];
            let count = blocks[at..].iter().take_while(|&&b| b == entry).count();
            if !runs.is_empty() {
                runs.push(' ');
            }
            runs.push_str(&format!("{count}:{entry}"));
            at += count;
        }
        Self { size, palette, runs }
    }

    /// Its blocks, along x, then z, then up, each an entry of its palette
    /// from 1 or 0: none where its runs do not fill its size or name an
    /// entry it has not.
    pub fn blocks(&self) -> Option<Vec<u16>> {
        let count = self.size.iter().map(|&v| v as usize).product::<usize>();
        let mut blocks = Vec::with_capacity(count);
        for run in self.runs.split_whitespace() {
            let (many, entry) = run.split_once(':')?;
            let (many, entry): (usize, u16) = (many.parse().ok()?, entry.parse().ok()?);
            if usize::from(entry) > self.palette.len() || blocks.len() + many > count {
                return None;
            }
            blocks.extend(std::iter::repeat_n(entry, many));
        }
        (blocks.len() == count).then_some(blocks)
    }
}

/// What a `.structure` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StructureDef {
    pub ground: GroundFit,
    /// How many blocks it stands into the ground.
    pub sink: i32,
    pub placement: Placement,
    /// A plant grown afresh at every site, by reference to a `.voxplant`,
    /// in place of a model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<StructureModel>,
}

impl Default for StructureDef {
    fn default() -> Self {
        Self { ground: GroundFit::Foundation, sink: 0, placement: Placement::default(), plant: None, model: None }
    }
}

impl StructureDef {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(text)?)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }
}

/// What a structure is built of in a world: its model's blocks by the
/// world's numbers, or a plant with its wood and leaves.
#[derive(Clone, Debug, PartialEq)]
pub enum StructureBlocks {
    Model { size: [u32; 3], blocks: Vec<BlockId> },
    Plant { plant: PlantDef, wood: BlockId, leaves: BlockId },
}

/// A structure as a world builds it: where, how the ground meets it, how
/// deep it stands, what of, and the world's biomes it stands in, by their
/// place in the world's list (none for any).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedStructure {
    pub placement: Placement,
    pub ground: GroundFit,
    pub sink: i32,
    pub blocks: StructureBlocks,
    pub biomes: Vec<usize>,
}

/// A world's structures, in the order it lists them; a structure listed
/// later gives way to one listed earlier where their sites would overlap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldStructures {
    pub list: Vec<PlacedStructure>,
}

/// One block of a structure as built: where it is from its site's foot, and
/// what.
type Piece = ([i32; 3], BlockId);

impl PlacedStructure {
    /// Its blocks at `site`, from the foot of the site, turned and mirrored
    /// as the site is; a model is centred on its site along x and z.
    fn pieces(&self, site: &Site) -> Vec<Piece> {
        let raw: Vec<Piece> = match &self.blocks {
            StructureBlocks::Model { size, blocks } => {
                let [sx, sy, sz] = size.map(|v| v as i32);
                let mut out = Vec::new();
                for y in 0..sy {
                    for z in 0..sz {
                        for x in 0..sx {
                            let block = blocks[((y * sz + z) * sx + x) as usize];
                            if block != AIR {
                                out.push(([x - sx / 2, y, z - sz / 2], block));
                            }
                        }
                    }
                }
                out
            }
            StructureBlocks::Plant { plant, wood, leaves } => plant
                .grow(site.seed)
                .blocks
                .into_iter()
                .map(|(at, part)| (at, if part == PlantPart::Leaf { *leaves } else { *wood }))
                .filter(|(_, block)| *block != AIR)
                .collect(),
        };
        raw.into_iter()
            .map(|([x, y, z], block)| {
                let x = if site.mirrored { -x } else { x };
                let [x, z] = match site.turns % 4 {
                    0 => [x, z],
                    1 => [-z, x],
                    2 => [-x, -z],
                    _ => [z, -x],
                };
                ([x, y, z], block)
            })
            .collect()
    }

    /// How far from its site it reaches along the ground, in blocks, at the
    /// most.
    fn reach(&self) -> f32 {
        match &self.blocks {
            StructureBlocks::Model { size, .. } => (size[0].max(size[2]) as f32 * 0.71) + 1.0,
            StructureBlocks::Plant { plant, .. } => plant.crown_width[1].max(plant.trunk) * 0.6 + 4.0,
        }
    }
}

/// The ground under a place, for building: the generator's height there,
/// in metres in its world.
struct Ground<'a> {
    world: &'a WorldGen,
    size: f32,
    position: [f32; 2],
}

impl Ground<'_> {
    /// The highest block of ground in column `(x, z)`, in blocks from the
    /// world's corner, as generated ground puts it.
    fn top(&self, x: i32, z: i32) -> i32 {
        let height = self.world.height_at(
            self.position[0] + (x as f32 + 0.5) * self.size,
            self.position[1] + (z as f32 + 0.5) * self.size,
        );
        (height / self.size - 0.5).ceil() as i32 - 1
    }
}

/// Builds `structures` into the chunks of chunk column `[cx, cz]` of a world
/// of `size`-metre blocks whose corner lies at `position` in `world`'s, its
/// ground going down to block `bottom`, its biomes `biomes` where it has
/// any: every structure whose site's ground fits and which reaches the
/// column, the parts of it within the column, in the order they are
/// listed, a structure's site left out where an earlier one's reaches it.
#[allow(clippy::too_many_arguments)]
pub fn build(
    chunks: &mut Vec<([i32; 3], Chunk)>,
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&Biomes>,
    (size, position, bottom): (f32, [f32; 2], i32),
    [cx, cz]: [i32; 2],
    under: BlockId,
) {
    let side = CHUNK as i32;
    let ground = Ground { world, size, position };
    // The column's own blocks, in metres in the generator's world.
    let low = [position[0] + (cx * side) as f32 * size, position[1] + (cz * side) as f32 * size];
    let high = [low[0] + side as f32 * size, low[1] + side as f32 * size];
    // Every site kept, with its place in blocks and the blocks it takes up along the ground, to leave out a later
    // structure's where an earlier one's stands.
    let mut kept: Vec<([i32; 2], f32)> = Vec::new();
    for structure in &structures.list {
        let reach = structure.reach();
        // Sites of earlier structures that may stand in this one's way are found further out, by its reach too.
        for site in structure.placement.tries_near(world.seed(), low, high, (reach + 64.0) * size) {
            let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
            let foot = ground.top(block[0], block[1]);
            let height = world.height_at(site.at[0], site.at[1]);
            let (east, north) =
                (world.height_at(site.at[0] + size, site.at[1]), world.height_at(site.at[0], site.at[1] + size));
            let slope = ((east - height) * (east - height) + (north - height) * (north - height)).sqrt() / size;
            let above_sea = height - world.sea_level();
            let biome = biomes.and_then(|biomes| {
                let (temperature, humidity) = world.climate(site.at[0], site.at[1], height);
                biomes.at(site.at[0], site.at[1], Place { temperature, humidity, above_sea })
            });
            let fits =
                structure.placement.fits(SiteGround { above_sea, slope, biome }, |b| structure.biomes.contains(&b));
            if !fits {
                continue;
            }
            let crowded = kept.iter().any(|(other, other_reach)| {
                let (dx, dz) = ((other[0] - block[0]) as f32, (other[1] - block[1]) as f32);
                (dx * dx + dz * dz).sqrt() < other_reach + reach
            });
            if crowded {
                continue;
            }
            kept.push((block, reach));
            // Only its blocks within this column are built here.
            let within = |x: i32, z: i32| x.div_euclid(side) == cx && z.div_euclid(side) == cz;
            let (lowest, highest) = (cx * side - reach.ceil() as i32 - 1, (cx + 1) * side + reach.ceil() as i32 + 1);
            if block[0] < lowest
                || block[0] > highest
                || block[1] < cz * side - reach.ceil() as i32 - 1
                || block[1] > (cz + 1) * side + reach.ceil() as i32 + 1
            {
                continue;
            }
            let base = foot + 1 - structure.sink;
            let pieces = structure.pieces(&site);
            let mut put = |x: i32, y: i32, z: i32, block: BlockId| {
                if !within(x, z) || y < bottom {
                    return;
                }
                let key = [cx, y.div_euclid(side), cz];
                let at = match chunks.iter().position(|(k, _)| *k == key) {
                    Some(at) => at,
                    None => {
                        chunks.push((key, Chunk::filled(AIR)));
                        chunks.len() - 1
                    }
                };
                let (lx, ly, lz) = ((x - cx * side) as u32, y.rem_euclid(side) as u32, (z - cz * side) as u32);
                chunks[at].1.set(lx, ly, lz, block);
            };
            if structure.ground != GroundFit::Leave {
                // Each column of its footprint: its lowest block, cleared over, and built up under.
                let mut footprint: Vec<([i32; 2], i32, BlockId)> = Vec::new();
                for &([x, y, z], piece) in &pieces {
                    let column = [block[0] + x, block[1] + z];
                    match footprint.iter_mut().find(|(at, ..)| *at == column) {
                        Some(found) if y < found.1 => (found.1, found.2) = (y, piece),
                        Some(_) => {}
                        None => footprint.push((column, y, piece)),
                    }
                }
                let tallest = pieces.iter().map(|(at, _)| at[1]).max().unwrap_or(0);
                for ([x, z], lowest, _) in footprint {
                    let top = ground.top(x, z);
                    let floor = base + lowest;
                    for y in (top + 1).min(floor)..floor {
                        put(x, y, z, under);
                    }
                    if structure.ground == GroundFit::Flatten {
                        for y in floor..=top.max(floor).min(base + tallest + 2) {
                            put(x, y, z, AIR);
                        }
                    }
                }
            }
            for ([x, y, z], piece) in pieces {
                put(block[0] + x, base + y, block[1] + z, piece);
            }
        }
    }
    chunks.retain(|(_, chunk)| !chunk.is_air());
    chunks.sort_by_key(|(key, _)| key[1]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{Preset, structures::SiteOn};

    #[test]
    fn a_structure_is_written_and_read_back_and_its_model_keeps_its_blocks() {
        let blocks: Vec<u16> = (0..60).map(|i| [0, 1, 1, 2][i % 4]).collect();
        let model =
            StructureModel::of([5, 3, 4], vec!["Blocks/Planks.block".into(), "Blocks/Log.block".into()], &blocks);
        assert_eq!(model.blocks(), Some(blocks));
        let house = StructureDef { ground: GroundFit::Flatten, sink: 1, model: Some(model), ..StructureDef::default() };
        let text = house.to_text();
        assert!(text.contains("ground = \"flatten\""), "{text}");
        assert_eq!(StructureDef::parse(&text).unwrap(), house);
        assert_eq!(StructureDef::parse("").unwrap(), StructureDef::default());
        assert!(StructureDef::parse("grond = \"leave\"").is_err());
        // Runs that do not fill its size, or name an entry it has not, are no model.
        let short = StructureModel { size: [2, 2, 2], palette: vec!["a".into()], runs: "7:1".into() };
        assert_eq!(short.blocks(), None);
        let unknown = StructureModel { size: [2, 2, 2], palette: vec!["a".into()], runs: "8:2".into() };
        assert_eq!(unknown.blocks(), None);
    }

    /// A hut 5 blocks a side and 4 high, walls of block 5 round a floor of 6, open inside, on level ground
    /// alone; and trees of wood 7 and leaves 8 on land.
    fn hut_and_trees() -> WorldStructures {
        let (sx, sy, sz) = (5, 4, 5);
        let mut blocks = vec![AIR; sx * sy * sz];
        for y in 0..sy {
            for z in 0..sz {
                for x in 0..sx {
                    let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1;
                    blocks[(y * sz + z) * sx + x] = if y == 0 {
                        6
                    } else if edge {
                        5
                    } else {
                        AIR
                    };
                }
            }
        }
        let hut = PlacedStructure {
            placement: Placement {
                spacing: 90.0,
                separation: 30.0,
                chance: 0.9,
                slope: 20.0,
                height: Some([1.0, 400.0]),
                ..Placement::default()
            },
            ground: GroundFit::Flatten,
            sink: 0,
            blocks: StructureBlocks::Model { size: [sx as u32, sy as u32, sz as u32], blocks },
            biomes: Vec::new(),
        };
        let trees = PlacedStructure {
            placement: Placement {
                spacing: 14.0,
                separation: 5.0,
                chance: 0.7,
                salt: 1,
                slope: 35.0,
                on: SiteOn::Land,
                ..Placement::default()
            },
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Plant { plant: PlantDef::default(), wood: 7, leaves: 8 },
            biomes: Vec::new(),
        };
        WorldStructures { list: vec![hut, trees] }
    }

    #[test]
    fn a_chunk_column_builds_the_part_of_every_structure_that_reaches_it_as_it_is_built_among_its_neighbours() {
        let world = Preset::RollingHills.world();
        let structures = hut_and_trees();
        let make = |[cx, cz]: [i32; 2]| {
            // Plain ground of block 1 up to the generator's height, a block a metre.
            let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
            let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0] };
            for z in 0..32 {
                for x in 0..32 {
                    let (gx, gz) = (cx * 32 + x, cz * 32 + z);
                    for y in -4..=ground.top(gx, gz) {
                        let key = [cx, y.div_euclid(32), cz];
                        let at = chunks.iter().position(|(k, _)| *k == key).unwrap_or_else(|| {
                            chunks.push((key, Chunk::filled(AIR)));
                            chunks.len() - 1
                        });
                        chunks[at].1.set(x as u32, y.rem_euclid(32) as u32, z as u32, 1);
                    }
                }
            }
            build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -4), [cx, cz], 2);
            chunks
        };
        let count = |chunks: &[([i32; 3], Chunk)], block: BlockId| {
            let mut n = 0;
            for (_, chunk) in chunks {
                for y in 0..32 {
                    for z in 0..32 {
                        for x in 0..32 {
                            n += usize::from(chunk.get(x, y, z) == block);
                        }
                    }
                }
            }
            n
        };
        // Over sixteen columns: huts and a wood stand in them.
        let columns: Vec<Vec<([i32; 3], Chunk)>> = (0..16).map(|i| make([i % 4 - 2, i / 4 - 2])).collect();
        let walls: usize = columns.iter().map(|c| count(c, 5)).sum();
        let (wood, leaves): (usize, usize) =
            (columns.iter().map(|c| count(c, 7)).sum(), columns.iter().map(|c| count(c, 8)).sum());
        assert!(walls >= 48, "{walls} blocks of hut wall");
        assert!(wood > 200 && leaves > 1000, "{wood} of wood, {leaves} of leaves");
        // The same column made again is the same, block for block: nothing depends on what was made before.
        for (i, column) in columns.iter().enumerate().step_by(5) {
            let again = make([i as i32 % 4 - 2, i as i32 / 4 - 2]);
            assert_eq!(column.len(), again.len());
            for ((ka, a), (kb, b)) in column.iter().zip(&again) {
                assert_eq!(ka, kb);
                for y in 0..32 {
                    for z in 0..32 {
                        for x in 0..32 {
                            assert_eq!(a.get(x, y, z), b.get(x, y, z), "column {i} chunk {ka:?} at ({x}, {y}, {z})");
                        }
                    }
                }
            }
        }
        // Every hut's floor is level and whole, and nothing of the ground stands inside its walls.
        let block_at = |x: i32, y: i32, z: i32| {
            let column = &columns[((z.div_euclid(32) + 2) * 4 + x.div_euclid(32) + 2) as usize];
            column
                .iter()
                .find(|(k, _)| k[1] == y.div_euclid(32))
                .map_or(AIR, |(_, c)| c.get(x.rem_euclid(32) as u32, y.rem_euclid(32) as u32, z.rem_euclid(32) as u32))
        };
        let mut huts = 0;
        for z in -62..62 {
            for x in -62..62 {
                // A hut's floor corner: floor here and four along x and z, walls over the corner.
                let Some(y) = (-4..120).find(|&y| block_at(x, y, z) == 6 && block_at(x, y + 1, z) == 5) else {
                    continue;
                };
                let corner = block_at(x - 1, y, z) != 6 && block_at(x, y, z - 1) != 6;
                if !corner || block_at(x + 4, y, z + 4) != 6 {
                    continue;
                }
                huts += 1;
                for dz in 0..5 {
                    for dx in 0..5 {
                        assert_eq!(block_at(x + dx, y, z + dz), 6, "hut floor at ({}, {y}, {})", x + dx, z + dz);
                        // Under the floor, ground or foundation, never air.
                        assert_ne!(block_at(x + dx, y - 1, z + dz), AIR, "air under a hut at ({}, {})", x + dx, z + dz);
                        if (1..4).contains(&dx) && (1..4).contains(&dz) {
                            for up in 1..4 {
                                assert_eq!(block_at(x + dx, y + up, z + dz), AIR, "ground inside a hut");
                            }
                        }
                    }
                }
            }
        }
        assert!(huts >= 1, "a hut found whole");
    }
}
