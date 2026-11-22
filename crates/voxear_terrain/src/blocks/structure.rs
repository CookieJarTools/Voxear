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
//!
//! [[model.states]]                                    # a palette entry placed facing a way, open, a door's top
//! entry = 2
//! facing = "east"
//! half = "upper"
//! ```
//!
//! A structure placed turned turns each block's state with it (see
//! [`WorldStructures::turns`]): a door facing east in a house turned a
//! quarter faces south.
//!
//! A world numbers the blocks a structure names as it does a biome's (see
//! [`StructureBlocks`]); building it into a chunk column needs nothing
//! else, so a chunk made alone builds the part of a structure that reaches
//! it exactly as it is built among its neighbours.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use super::dungeon::{
    Dungeon, DungeonBlockNames, DungeonBlocks, DungeonDef, DungeonPasses, DungeonPlace, DungeonRoom, DungeonSize,
    DungeonSizes, DungeonTheme, EntranceKind, RoomRole,
};
use super::{
    chunk::{AIR, BlockId, CHUNK, Chunk},
    plant::{PlantDef, PlantPart},
    world_layers::WorldLayers,
};
use crate::generate::{
    Biomes, WorldBiomes, WorldGen,
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
    /// Only the ground over its footprint cleared, as high as it stands: a
    /// cutting, with nothing built up under it.
    Cut,
}

impl GroundFit {
    pub const ALL: [GroundFit; 4] = [GroundFit::Leave, GroundFit::Foundation, GroundFit::Flatten, GroundFit::Cut];

    pub fn name(self) -> &'static str {
        match self {
            GroundFit::Leave => "Leave",
            GroundFit::Foundation => "Foundation",
            GroundFit::Flatten => "Flatten",
            GroundFit::Cut => "Cut",
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
    /// The state each entry of its palette that has one is placed in, by
    /// its place in the palette from 1: a door's way and half, a stair's.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<PaletteState>,
}

/// The state a structure's palette entry is placed in (see
/// `shape::BlockState`), by the entry's place from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteState {
    pub entry: u16,
    #[serde(flatten)]
    pub state: super::shape::BlockState,
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
        Self { size, palette, runs, states: Vec::new() }
    }

    /// The state its palette entry `entry` (from 1) is placed in, where it
    /// says.
    pub fn state_of(&self, entry: usize) -> Option<super::shape::BlockState> {
        self.states.iter().find(|placed| usize::from(placed.entry) == entry).map(|placed| placed.state)
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
    /// A town of other structures laid out round a middle, in place of a
    /// model or a plant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub town: Option<TownDef>,
    /// A dungeon under the ground, in place of a model, a plant or a town.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dungeon: Option<DungeonDef>,
}

impl Default for StructureDef {
    fn default() -> Self {
        Self {
            ground: GroundFit::Foundation,
            sink: 0,
            placement: Placement::default(),
            plant: None,
            model: None,
            town: None,
            dungeon: None,
        }
    }
}

/// A town grown as a village grows: a middle (a well, a square, a hall),
/// streets running out from it that follow the lie of the land, bending
/// round slopes and branching as they go, and houses along them, each a
/// structure with a model, chosen by weight, set back from the street and
/// turned to face it, with a short path from its door. Houses come nearest
/// the middle first, so a town is thickest there and thins out along its
/// streets. A house's door is on the side of its model that faces -z, in
/// its middle.
///
/// ```toml
/// [town]
/// middle = "Structures/Well.structure"
/// count = [8, 14]             # houses, from and to
/// radius = [24.0, 48.0]       # how far its main streets run, in blocks, from and to
/// path = "Blocks/Gravel.block"
/// path_width = 3
/// wander = 0.5                # how much its streets bend, 0 for straight
/// branching = 0.5             # how often they branch, 0 for never
/// roads = true               # roads to the next towns of its kind; off unless switched on
/// road = "Blocks/Gravel.block"
/// road_width = 3
/// bridge = "Blocks/Planks.block" # where a road crosses water
/// link = 0.7                  # how many of the roads to its nearest towns are made, 0 to 1
///
/// [[town.houses]]
/// structure = "Structures/Cottage.structure"
/// weight = 3.0
///
/// [[town.houses]]             # a rare one: once a town at most, near its middle, in two biomes
/// structure = "Structures/Chapel.structure"
/// weight = 0.5
/// most = 1                    # at most so many in a town; 0 for no limit
/// zone = [0.0, 0.4]           # how far out it stands, as a share of the town's reach, from and to
/// biomes = ["Biomes/Grassland.biome", "Biomes/Forest.biome"]  # none for any
/// ```
///
/// A small hamlet and a large town are two structures with towns of their
/// own sizes, each with its own placement: the larger rarer, further apart
/// and kept clear of the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TownDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle: Option<String>,
    pub houses: Vec<TownHouse>,
    pub count: [u32; 2],
    pub radius: [f32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub path_width: u32,
    pub wander: f32,
    pub branching: f32,
    /// Whether it has roads to the next towns of its kind.
    pub roads: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    pub road_width: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge: Option<String>,
    pub link: f32,
}

impl Default for TownDef {
    fn default() -> Self {
        Self {
            middle: None,
            houses: Vec::new(),
            count: [8, 14],
            radius: [24.0, 48.0],
            path: None,
            path_width: 3,
            wander: 0.5,
            branching: 0.5,
            roads: false,
            road: None,
            road_width: 3,
            bridge: None,
            link: 0.7,
        }
    }
}

/// One kind of house of a town, by reference to a `.structure` with a
/// model: how often it is chosen against the others, at most how many a
/// town has (0 for no limit), how far out from the middle it stands, as a
/// share of the town's reach, and the biomes it stands in (none for any).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TownHouse {
    pub structure: String,
    pub weight: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub most: u32,
    pub zone: [f32; 2],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub biomes: Vec<String>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Default for TownHouse {
    fn default() -> Self {
        Self { structure: String::new(), weight: 1.0, most: 0, zone: [0.0, 1.0], biomes: Vec::new() }
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
    /// `logs` numbers its wood in each way it stands as a log thinner than
    /// a block or lying down, as its states' bytes: empty where its wood is
    /// no log, and every block of it is `wood`, whole.
    Plant { plant: PlantDef, wood: BlockId, leaves: BlockId, vine: BlockId, logs: Vec<(u8, BlockId)> },
    Town(Town),
    Dungeon(Dungeon),
}

/// A piece of a town: a model and how the ground meets it.
#[derive(Clone, Debug, PartialEq)]
pub struct TownPiece {
    pub size: [u32; 3],
    pub blocks: Vec<BlockId>,
    pub ground: GroundFit,
    pub sink: i32,
}

impl TownPiece {
    /// How far from its middle it stands along the ground, in blocks.
    fn radius(&self) -> f32 {
        self.size[0].max(self.size[2]) as f32 * 0.71 + 1.0
    }
}

/// A kind of house as a town builds it: its piece, weight, at most how many
/// (0 for no limit), how far out as a share of the town's reach, and the
/// world's biomes it stands in, by their place in the world's list (none
/// for any).
#[derive(Clone, Debug, PartialEq)]
pub struct TownHouseKind {
    pub piece: TownPiece,
    pub weight: f32,
    pub most: u32,
    pub zone: [f32; 2],
    pub biomes: Vec<usize>,
}

impl TownHouseKind {
    /// A kind with `piece` and `weight` that stands anywhere in a town.
    pub fn anywhere(piece: TownPiece, weight: f32) -> Self {
        Self { piece, weight, most: 0, zone: [0.0, 1.0], biomes: Vec::new() }
    }
}

/// A town as a world builds it: its pieces by the world's numbers, how many
/// houses and how far its main streets run, in blocks, its streets' block
/// (air for none) and width, and how much they bend and branch; and its
/// roads to the next towns of its kind: their block (air for none), width,
/// the block of their bridges over water, and how many are made.
#[derive(Clone, Debug, PartialEq)]
pub struct Town {
    pub middle: Option<TownPiece>,
    pub houses: Vec<TownHouseKind>,
    pub count: [u32; 2],
    pub radius: [f32; 2],
    pub path: BlockId,
    pub path_width: u32,
    pub wander: f32,
    pub branching: f32,
    pub roads: bool,
    pub road: BlockId,
    pub road_width: u32,
    pub bridge: BlockId,
    pub link: f32,
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
/// Beside them, each block of theirs that faces a way, by its number, as
/// it is numbered turned each of four ways, and then mirrored each of the
/// four (see [`WorldStructures::turned_block`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldStructures {
    pub list: Vec<PlacedStructure>,
    pub turns: std::collections::BTreeMap<BlockId, [BlockId; 8]>,
    /// What each structure of the list was read from, in the same order,
    /// where whoever read them says: for scripts to name them.
    pub sources: Vec<StructureSource>,
}

/// The files a structure of a world was read from: its `.structure`'s
/// reference, and for a town, its middle's and each of its kinds of house's,
/// in the order of the town's list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StructureSource {
    pub structure: String,
    pub middle: Option<String>,
    pub houses: Vec<String>,
}

impl WorldStructures {
    /// The number of block `block` turned `turns` quarter turns about the
    /// up axis, mirrored along x first where `mirrored` says: the block
    /// itself where it faces no way.
    pub fn turned_block(&self, block: BlockId, turns: u8, mirrored: bool) -> BlockId {
        match self.turns.get(&block) {
            Some(turned) => turned[usize::from(turns % 4) + if mirrored { 4 } else { 0 }],
            None => block,
        }
    }
}

/// One block of a structure as built: where it is from its site's foot, and
/// what.
type Piece = ([i32; 3], BlockId);

impl PlacedStructure {
    /// Its blocks at `site`, from the foot of the site, turned and mirrored
    /// as the site is; a model is centred on its site along x and z.
    fn pieces(&self, site: &Site) -> Vec<Piece> {
        let raw: Vec<Piece> = match &self.blocks {
            StructureBlocks::Model { size, blocks } => model_pieces(*size, blocks),
            StructureBlocks::Plant { plant, wood, leaves, vine, logs } => plant
                .grow(site.seed)
                .blocks
                .into_iter()
                .map(|(at, part, state)| match part {
                    PlantPart::Leaf => (at, *leaves),
                    PlantPart::Vine => (at, *vine),
                    // A log lying along x lies along z once turned a quarter, as its site is turned below.
                    _ => (at, log_number(*wood, logs, state.turned(site.turns, false))),
                })
                .filter(|(_, block)| *block != AIR)
                .collect(),
            // A town's are its pieces', each built at its own place (see `town_layout`).
            StructureBlocks::Town(_) | StructureBlocks::Dungeon(_) => Vec::new(),
        };
        turned(raw, site.turns, site.mirrored)
    }

    /// How far from its site it stands on the ground, in blocks: what keeps
    /// it clear of another structure's site. A model's whole footprint; a
    /// plant's trunk and a little round it, so a wood's crowns may meet.
    fn footprint(&self) -> f32 {
        match &self.blocks {
            StructureBlocks::Model { size, .. } => (size[0].max(size[2]) as f32 * 0.71) + 1.0,
            StructureBlocks::Plant { plant, .. } => plant.trunk.max(1.0) + 2.0,
            // A town keeps clear of what came before it by its middle; what comes after keeps clear of its streets
            // and buildings themselves (see `build`).
            StructureBlocks::Town(town) => town.middle.as_ref().map_or(3.0, TownPiece::radius) + 2.0,
            // A dungeon stands on the ground by its way in alone.
            StructureBlocks::Dungeon(dungeon) => dungeon.footprint(),
        }
    }

    /// The columns of ground it stands on, from its site, as it stands
    /// unturned, each with how many blocks down from its top must be ground
    /// (see [`Ground::holds`]): a plant's trunk, three down, as deep as its
    /// roots reach; a model's middle and corners, a town's middle's. None
    /// for a dungeon, which is under the ground.
    fn feet(&self) -> Vec<([i32; 2], i32)> {
        let model = |size: [u32; 3]| {
            let (sx, sz) = (size[0] as i32, size[2] as i32);
            let (low, high) = ([-(sx / 2), -(sz / 2)], [sx - 1 - sx / 2, sz - 1 - sz / 2]);
            let corners = [[low[0], low[1]], [high[0], low[1]], [low[0], high[1]], [high[0], high[1]]];
            std::iter::once([0, 0]).chain(corners).map(|at| (at, 1)).collect()
        };
        match &self.blocks {
            StructureBlocks::Plant { .. } => vec![([0, 0], 3)],
            StructureBlocks::Model { size, .. } => model(*size),
            StructureBlocks::Town(town) => {
                town.middle.as_ref().map_or_else(|| vec![([0, 0], 1)], |middle| model(middle.size))
            }
            StructureBlocks::Dungeon(_) => Vec::new(),
        }
    }

    /// How far from its site it reaches along the ground, in blocks, at the
    /// most.
    fn reach(&self) -> f32 {
        match &self.blocks {
            StructureBlocks::Model { size, .. } => (size[0].max(size[2]) as f32 * 0.71) + 1.0,
            StructureBlocks::Plant { plant, .. } => plant.crown_width[1].max(plant.trunk) * 0.6 + 4.0,
            StructureBlocks::Town(town) => town.reach(),
            StructureBlocks::Dungeon(dungeon) => dungeon.reach(),
        }
    }
}

/// The number of the log `wood` standing as `state`, of those `logs`
/// numbers: `wood` itself where it is whole and standing up, or where its
/// wood is no log.
pub fn log_number(wood: BlockId, logs: &[(u8, BlockId)], state: super::shape::BlockState) -> BlockId {
    let byte = state.normalized(super::block::BlockShape::Log).to_byte();
    logs.iter().find(|(kept, _)| *kept == byte).map_or(wood, |(_, number)| *number)
}

/// `pieces` turned `turns` quarter turns about the up axis, mirrored along
/// x first where `mirrored` says.
fn turned(pieces: Vec<Piece>, turns: u8, mirrored: bool) -> Vec<Piece> {
    pieces
        .into_iter()
        .map(|([x, y, z], block)| {
            let x = if mirrored { -x } else { x };
            ([turn([x, z], turns)[0], y, turn([x, z], turns)[1]], block)
        })
        .collect()
}

/// `[x, z]` turned `turns` quarter turns about the up axis.
fn turn([x, z]: [i32; 2], turns: u8) -> [i32; 2] {
    match turns % 4 {
        0 => [x, z],
        1 => [-z, x],
        2 => [-x, -z],
        _ => [z, -x],
    }
}

/// A model's blocks, centred along x and z on its middle, from its foot up.
pub(crate) fn model_pieces(size: [u32; 3], blocks: &[BlockId]) -> Vec<Piece> {
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

/// Numbers from a seed, one after another: the same on every machine.
struct Draws(u32);

impl Draws {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut h = self.0;
        h = (h ^ (h >> 15)).wrapping_mul(h | 1);
        h ^= h.wrapping_add((h ^ (h >> 7)).wrapping_mul(h | 61));
        h ^ (h >> 14)
    }

    fn unit(&mut self) -> f32 {
        unit_of(self.next())
    }

    fn between(&mut self, [from, to]: [f32; 2]) -> f32 {
        from + (to - from) * self.unit()
    }
}

fn unit_of(hash: u32) -> f32 {
    (hash >> 8) as f32 / 16_777_216.0
}

/// A building of a town as laid out: where its middle stands, in blocks,
/// its blocks turned to face the town's middle, and how the ground meets it.
struct Part {
    at: [i32; 2],
    pieces: Vec<Piece>,
    ground: GroundFit,
    sink: i32,
    /// How its pieces were turned, for the states of its blocks to turn.
    turns: u8,
    mirrored: bool,
    /// A town's house: its kind, by its place in the town's list, and the
    /// block just outside its door, from its middle. None for a town's
    /// middle and for a structure of its own.
    house: Option<(usize, [i32; 2])>,
}

impl Part {
    /// How far its blocks reach from its middle along x and along z.
    fn half(&self) -> [i32; 2] {
        let reach = |axis: usize| self.pieces.iter().map(|(at, _)| at[axis].abs()).max().unwrap_or(0);
        [reach(0), reach(2)]
    }
}

impl Town {
    /// How far its buildings reach from its middle, in blocks: its longest
    /// street, set back by the street's width and its largest house.
    fn reach(&self) -> f32 {
        let largest =
            self.houses.iter().map(|kind| kind.piece.size[0].max(kind.piece.size[2]) as f32).fold(0.0f32, f32::max);
        self.radius[1].max(self.radius[0]) + self.path_width as f32 + largest * 1.5 + 4.0
    }

    /// Its buildings and streets at a site whose middle is block `centre`,
    /// on `ground`: the middle there; streets grown out from it (see
    /// `streets`); houses along them, nearest the middle first, up to its
    /// count, each on ground level enough and clear of the streets and the
    /// rest, turned to face its street, with a short path from its door to
    /// it. The blocks along x and z its streets and paths run over come
    /// second.
    fn layout(&self, seed: u32, centre: [i32; 2], ground: &Ground) -> (Vec<Part>, Vec<[i32; 2]>) {
        let mut draws = Draws(seed ^ 0x7A3C_15E1);
        let mut parts = Vec::new();
        if let Some(middle) = &self.middle {
            let pieces = model_pieces(middle.size, &middle.blocks);
            let (ground, sink) = (middle.ground, middle.sink);
            parts.push(Part { at: centre, pieces, ground, sink, turns: 0, mirrored: false, house: None });
        }
        let middle_radius = self.middle.as_ref().map_or(1.5, TownPiece::radius);
        let streets = self.streets(&mut draws, centre, middle_radius, ground);
        let (mut paths, mut laid) = (Vec::new(), crate::coord_hash::CoordSet::default());
        let width = self.path_width.max(1) as f32;
        for street in &streets {
            for &(point, _) in &street.points {
                lay(&mut paths, &mut laid, point, width);
            }
        }
        let total: f32 = self.houses.iter().map(|kind| kind.weight.max(0.0)).sum();
        let reach = self.reach().max(1.0);
        let mut built = vec![0u32; self.houses.len()];
        let [least, most] = [self.count[0].min(self.count[1]), self.count[0].max(self.count[1])];
        let wanted = least + (draws.next() % (most - least + 1).max(1));
        if total <= 0.0 || self.path == AIR && streets.is_empty() {
            return (parts, paths);
        }
        // Every place along a street a house might front, either side, nearest the middle first.
        let mut lots: Vec<([f32; 2], [f32; 2], f32)> = Vec::new();
        for street in &streets {
            for (at, &(point, heading)) in street.points.iter().enumerate().skip(1) {
                if at % 3 != 0 {
                    continue;
                }
                for side in [1.0f32, -1.0] {
                    let out = [-heading[1] * side, heading[0] * side];
                    let away = ((point[0] - centre[0] as f32).powi(2) + (point[1] - centre[1] as f32).powi(2)).sqrt();
                    lots.push((point, out, away + draws.unit() * 16.0));
                }
            }
        }
        lots.sort_by(|a, b| a.2.total_cmp(&b.2));
        let fronts = [[0.0f32, -1.0], [1.0, 0.0], [0.0, 1.0], [-1.0, 0.0]];
        let mut placed = 0;
        let mut stubs: Vec<[i32; 2]> = Vec::new();
        for (point, out, _) in lots {
            if placed >= wanted {
                break;
            }
            // A kind of house by weight, among those that may stand here: this far out, in this biome, not yet as
            // many as it may be; turned so its front faces back along `out`, to the street.
            let lot = [point[0].round() as i32, point[1].round() as i32];
            let share = (((lot[0] - centre[0]).pow(2) + (lot[1] - centre[1]).pow(2)) as f32).sqrt() / reach;
            let biome = if self.houses.iter().any(|kind| !kind.biomes.is_empty()) { ground.biome(lot) } else { None };
            let may = |at: usize| {
                let kind = &self.houses[at];
                let [from, to] = [kind.zone[0].min(kind.zone[1]), kind.zone[0].max(kind.zone[1])];
                (kind.most == 0 || built[at] < kind.most)
                    && share >= from
                    && share <= to.max(from)
                    && (kind.biomes.is_empty() || biome.is_some_and(|biome| kind.biomes.contains(&biome)))
            };
            let total: f32 =
                (0..self.houses.len()).filter(|&at| may(at)).map(|at| self.houses[at].weight.max(0.0)).sum();
            if total <= 0.0 {
                continue;
            }
            let mut pick = draws.unit() * total;
            let Some(which) = (0..self.houses.len()).filter(|&at| may(at)).find(|&at| {
                pick -= self.houses[at].weight.max(0.0);
                pick < 0.0
            }) else {
                continue;
            };
            let house = &self.houses[which].piece;
            let turns = (0..4u8)
                .max_by(|&a, &b| {
                    let dot = |t: u8| -(fronts[t as usize][0] * out[0] + fronts[t as usize][1] * out[1]);
                    dot(a).total_cmp(&dot(b))
                })
                .unwrap_or(0);
            // How deep it is from its front, once turned, and set back that far and a block from the street's edge.
            let deep = if turns % 2 == 0 { house.size[2] } else { house.size[0] } as f32;
            let back = width * 0.5 + 1.5 + deep * 0.5;
            let at = [(point[0] + out[0] * back).round() as i32, (point[1] + out[1] * back).round() as i32];
            let half = [house.size[0] as i32 / 2, house.size[2] as i32 / 2];
            let half = if turns % 2 == 0 { half } else { [half[1], half[0]] };
            // Clear of every building by a block at least, footprint to footprint.
            let clear = parts.iter().all(|part: &Part| {
                let other = part.half();
                (part.at[0] - at[0]).abs() > other[0] + half[0] + 1
                    || (part.at[1] - at[1]).abs() > other[1] + half[1] + 1
            });
            let covers = |cell: &[i32; 2]| (cell[0] - at[0]).abs() <= half[0] && (cell[1] - at[1]).abs() <= half[1];
            if !clear || paths.iter().any(covers) || stubs.iter().any(covers) {
                continue;
            }
            // Level enough: its corners' ground within three blocks of each other, and over the sea.
            let corners = [[-1, -1], [1, -1], [-1, 1], [1, 1]]
                .map(|[sx, sz]| ground.top(at[0] + sx * half[0], at[1] + sz * half[1]));
            let (low, high) = (corners.iter().min().copied().unwrap_or(0), corners.iter().max().copied().unwrap_or(0));
            if high - low > 3 || ground.under_sea(at[0], at[1]) {
                continue;
            }
            let pieces = turned(model_pieces(house.size, &house.blocks), turns, false);
            let door = turn([0, -(house.size[2] as i32) / 2 - 1], turns);
            // Its path from the door back to the street; a house whose door does not reach one is not built.
            let mut cell = [at[0] + door[0], at[1] + door[1]];
            let step = [door[0].signum(), door[1].signum()];
            let mut stub = Vec::new();
            while !paths.contains(&cell) && stub.len() < 5 {
                stub.push(cell);
                cell = [cell[0] + step[0], cell[1] + step[1]];
            }
            if !paths.contains(&cell) {
                continue;
            }
            // On ground, its middle and corners: not over a cave's opening.
            let feet = [[0, 0], [-1, -1], [1, -1], [-1, 1], [1, 1]]
                .map(|[sx, sz]| ([at[0] + sx * half[0], at[1] + sz * half[1]], 1));
            if !ground.holds(&feet).into_iter().all(|holds| holds) {
                continue;
            }
            stubs.extend(stub);
            let (ground, sink, house) = (house.ground, house.sink, Some((which, door)));
            parts.push(Part { at, pieces, ground, sink, turns, mirrored: false, house });
            placed += 1;
            built[which] += 1;
        }
        for cell in stubs {
            if !paths.contains(&cell) {
                paths.push(cell);
            }
        }
        // No street under a building: streets were laid first, but a middle stands on its own ground.
        paths.retain(|cell| {
            !parts.iter().any(|part| {
                part.pieces.iter().any(|([x, y, z], _)| *y == 0 && [part.at[0] + x, part.at[1] + z] == *cell)
            })
        });
        (parts, paths)
    }

    /// Streets grown out from the middle at `centre`: three or four main
    /// streets, evenly round with a little in it, each from the middle's
    /// edge out as far as its radius, a step of two blocks at a time. At each
    /// step a street goes on the way of three, ahead or a little to either
    /// side, that climbs least, with a little wander in it, so it bends round
    /// hills rather than over them; it stops at ground too steep to walk, at
    /// the sea, or near another street. Now and then one branches off to a
    /// side, shorter, and so may its branches once more.
    fn streets(&self, draws: &mut Draws, centre: [i32; 2], middle_radius: f32, ground: &Ground) -> Vec<Street> {
        // Turning by a small angle, without trigonometry: cos and sin of 12 and 24 degrees, and a right angle.
        const TURNS: [(f32, f32); 3] = [(0.9781, 0.2079), (1.0, 0.0), (0.9781, -0.2079)];
        let rotate = |[x, z]: [f32; 2], (c, s): (f32, f32)| [x * c - z * s, x * s + z * c];
        let height = |p: [f32; 2]| ground.top(p[0].round() as i32, p[1].round() as i32) as f32;
        let mains = 3 + draws.next() % 2;
        let first = draws.unit();
        let mut queue: Vec<([f32; 2], [f32; 2], f32, u32)> = Vec::new();
        for main in 0..mains {
            // Evenly round, by a turn of the first way through a whole turn's share, built from quarter and twelfth turns.
            let share = (main as f32 + first * 0.6) / mains as f32;
            let mut way = [1.0f32, 0.0];
            let twelfths = (share * 30.0).round() as u32;
            for _ in 0..twelfths {
                way = rotate(way, (0.9781, 0.2079));
            }
            let start =
                [centre[0] as f32 + way[0] * (middle_radius + 1.0), centre[1] as f32 + way[1] * (middle_radius + 1.0)];
            queue.push((start, way, draws.between(self.radius), 0));
        }
        let mut streets: Vec<Street> = Vec::new();
        let wander = self.wander.clamp(0.0, 1.0);
        while let Some((start, mut heading, length, depth)) = queue.pop() {
            let mut street = Street { points: vec![(start, heading)] };
            let mut at = start;
            let mut since_branch = 0;
            let mut walked = 0.0;
            while walked < length {
                let here = height(at);
                let best = TURNS
                    .iter()
                    .map(|&turning| {
                        let way = rotate(heading, turning);
                        let next = [at[0] + way[0] * 2.0, at[1] + way[1] * 2.0];
                        // How steeply it climbs, and a little wander so streets on level ground still bend.
                        let cost = (height(next) - here).abs() + draws.unit() * wander * 1.5;
                        (cost, way, next)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .expect("three ways");
                let (_, way, next) = best;
                // Too steep to walk, or into the sea, or onto another street: it ends here.
                let near_other = streets.iter().any(|other| {
                    other.points.iter().any(|&(p, _)| (p[0] - next[0]).powi(2) + (p[1] - next[1]).powi(2) < 9.0)
                });
                let from_middle = ((next[0] - centre[0] as f32).powi(2) + (next[1] - centre[1] as f32).powi(2)).sqrt();
                // Another street is looked for only once this one is clear of the street it leaves.
                let near_other = near_other && walked >= 6.0;
                if (height(next) - here).abs() > 1.6
                    || ground.under_sea(next[0].round() as i32, next[1].round() as i32)
                    || near_other
                    || from_middle < middle_radius
                {
                    break;
                }
                heading = way;
                at = next;
                walked += 2.0;
                since_branch += 1;
                street.points.push((at, heading));
                // A branch off to one side, now and then, shorter than the street it leaves.
                if depth < 2 && since_branch >= 6 && draws.unit() < self.branching.clamp(0.0, 1.0) * 0.18 {
                    since_branch = 0;
                    let side = if draws.next() & 1 == 0 { (0.0, 1.0) } else { (0.0, -1.0) };
                    let out = rotate(heading, side);
                    let start = [at[0] + out[0] * 2.0, at[1] + out[1] * 2.0];
                    queue.push((start, out, (length - walked).max(8.0) * 0.6, depth + 1));
                }
            }
            if street.points.len() > 2 {
                streets.push(street);
            }
        }
        streets
    }
}

impl Town {
    /// The road from the town at block `from` to the one at block `to`,
    /// each step's place, in blocks: out of the first town's middle and
    /// on to the second's, two blocks a step, each step the way of five
    /// round the way it heads that climbs least and turns it least from its
    /// end, pulled harder to its end the nearer it is, so far from either
    /// town it curves round hills and nearer it heads in; a little wander in
    /// it from `seed`, and water crossed where there is no way round near. Worked out from its two towns alone, so every
    /// chunk it crosses finds the same road; and worked out once, kept for every thread, as each chunk column and far
    /// tile along it would else walk it again, reading the ground twice at each of its turns (see [`ROADS`]).
    fn road_between(&self, seed: u32, from: [i32; 2], to: [i32; 2], ground: &Ground) -> Arc<Vec<[f32; 2]>> {
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (ground.world.hash(), ground.size.to_bits(), ground.position.map(f32::to_bits), ground.halved)
                .hash(&mut hasher);
            (seed, from, to, self.radius.map(f32::to_bits), self.wander.to_bits()).hash(&mut hasher);
            hasher.finish()
        };
        let cell = {
            let mut kept = ROADS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            kept.get(&key).unwrap_or_else(|| {
                let cell = Arc::default();
                kept.insert(key, Arc::clone(&cell));
                cell
            })
        };
        cell.get_or_init(|| Arc::new(self.walk_road(seed, from, to, ground))).clone()
    }

    /// [`Town::road_between`], walked.
    fn walk_road(&self, seed: u32, from: [i32; 2], to: [i32; 2], ground: &Ground) -> Vec<[f32; 2]> {
        // Turning by 18 and 35 degrees either side, without trigonometry.
        const TURNS: [(f32, f32); 5] =
            [(0.8192, -0.5736), (0.9511, -0.3090), (1.0, 0.0), (0.9511, 0.3090), (0.8192, 0.5736)];
        let rotate = |[x, z]: [f32; 2], (c, s): (f32, f32)| [x * c - z * s, x * s + z * c];
        let height = |p: [f32; 2]| ground.top(p[0].round() as i32, p[1].round() as i32) as f32;
        let wet = |p: [f32; 2]| ground.under_sea(p[0].round() as i32, p[1].round() as i32);
        let mut draws = Draws(seed ^ 0x51ED_270B);
        let end = [to[0] as f32, to[1] as f32];
        let gap = self.radius[0].min(self.radius[1]) * 0.5;
        let towards = |p: [f32; 2]| {
            let (dx, dz) = (end[0] - p[0], end[1] - p[1]);
            let length = (dx * dx + dz * dz).sqrt().max(1e-3);
            ([dx / length, dz / length], length)
        };
        let (way, length) = towards([from[0] as f32, from[1] as f32]);
        let mut at = [from[0] as f32 + way[0] * gap, from[1] as f32 + way[1] * gap];
        let mut heading = way;
        let mut points = vec![at];
        for _ in 0..((length / 2.0) * 2.0) as usize + 8 {
            let (way, left) = towards(at);
            if left <= gap {
                break;
            }
            let here = height(at);
            // Nearer its end it must head there; further off it may wander round what is in its way.
            let pull = 1.0 + 6.0 * (1.0 - (left / length).clamp(0.0, 1.0)).powi(2);
            let best = TURNS
                .iter()
                .map(|&turning| {
                    let turned = rotate(heading, turning);
                    let next = [at[0] + turned[0] * 2.0, at[1] + turned[1] * 2.0];
                    let climb = (height(next) - here).abs();
                    let off = (1.0 - (turned[0] * way[0] + turned[1] * way[1])) * pull;
                    let water = if wet(next) { 1.5 } else { 0.0 };
                    let cost = climb * 2.0 + off + water + draws.unit() * 0.6 * self.wander.clamp(0.0, 1.0);
                    (cost, turned, next)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .expect("five ways");
            heading = best.1;
            at = best.2;
            points.push(at);
        }
        points
    }
}

/// The roads between towns worked out, by what each was worked out from: its
/// world, where the world's blocks lie in it, its two towns and its town's
/// settings; [`ROADS_KEPT`] of those asked for lately, more than the far
/// view's structures reach.
type KeptRoads = crate::coord_hash::Lately<u64, Arc<std::sync::OnceLock<Arc<Vec<[f32; 2]>>>>>;
static ROADS: std::sync::LazyLock<std::sync::Mutex<KeptRoads>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(KeptRoads::new(ROADS_KEPT)));
const ROADS_KEPT: usize = 1 << 12;

/// A street as grown: each step's place, in blocks, and the way it heads
/// there.
struct Street {
    points: Vec<([f32; 2], [f32; 2])>,
}

/// Lays a street `width` blocks wide round `point` into `cells`, each
/// cell once: those already laid are in `laid`.
fn lay(cells: &mut Vec<[i32; 2]>, laid: &mut crate::coord_hash::CoordSet<[i32; 2]>, point: [f32; 2], width: f32) {
    let reach = width * 0.5;
    let (from, to) = ((point[0] - reach).round() as i32, (point[0] + reach).round() as i32);
    let (near, far) = ((point[1] - reach).round() as i32, (point[1] + reach).round() as i32);
    for z in near..=far {
        for x in from..=to {
            let (dx, dz) = (x as f32 - point[0], z as f32 - point[1]);
            if dx * dx + dz * dz <= reach * reach + 0.5 && laid.insert([x, z]) {
                cells.push([x, z]);
            }
        }
    }
}

/// How high over the sea a plant stands at least, in metres, whatever its
/// biome's shore: a beach is bare.
const BEACH: f32 = 2.5;

/// How far from a river's or a lake's water a plant keeps, in metres.
pub(crate) const BY_WATER: f32 = 6.0;

/// How high over the sea the plants `structure` grows keep, in metres,
/// wherever they stand: as high as the shore of any biome they keep to, of
/// every biome where they keep to none, and over a beach. None for a
/// structure that is no plant, or a plant that grows by the water. So far
/// off, where a ring's block may hold a beach and the land behind it, a
/// wood keeps to the land a tree stands on near.
pub fn plant_shore(structure: &PlacedStructure, biomes: Option<&WorldBiomes>) -> Option<f32> {
    let plant = matches!(structure.blocks, StructureBlocks::Plant { .. });
    if !plant || structure.placement.by_water {
        return None;
    }
    let list = biomes.map_or(&[][..], |biomes| biomes.biomes.list());
    let kept = |at: &usize| structure.biomes.is_empty() || structure.biomes.contains(at);
    let shores = list.iter().enumerate().filter(|(at, _)| kept(at)).map(|(_, biome)| biome.ground.shore_height);
    Some(shores.fold(BEACH, f32::max))
}

/// The ground under a place, for building: the generator's height there,
/// in metres in its world.
#[derive(Clone, Copy)]
pub(crate) struct Ground<'a> {
    pub(crate) world: &'a WorldGen,
    pub(crate) size: f32,
    pub(crate) position: [f32; 2],
    pub(crate) biomes: Option<&'a WorldBiomes>,
    /// Whether its top is laid in half blocks (see `generate::BlockGenerate::slabs`).
    pub(crate) halved: bool,
    /// Where its top is flat rock at one height, as in a dungeon's own
    /// dimension, that block: there is no sea, river or cave in it, and the
    /// generator is not asked.
    pub(crate) flat: Option<i32>,
}

impl Ground<'_> {
    /// [`Ground::top`] of each of `columns`, the generator asked for all at
    /// once.
    pub(crate) fn tops(&self, columns: &[[i32; 2]]) -> Vec<i32> {
        if let Some(top) = self.flat {
            return vec![top; columns.len()];
        }
        let xs: Vec<f32> = columns.iter().map(|c| self.position[0] + (c[0] as f32 + 0.5) * self.size).collect();
        let zs: Vec<f32> = columns.iter().map(|c| self.position[1] + (c[1] as f32 + 0.5) * self.size).collect();
        let mut heights = vec![0.0; columns.len()];
        self.world.heights(&xs, &zs, &mut heights);
        heights.into_iter().map(|height| self.top_of(height)).collect()
    }

    /// The highest whole block of ground under ground `height` metres high
    /// (see [`Ground::top`]).
    fn top_of(&self, height: f32) -> i32 {
        let surface = height / self.size;
        let slab = (surface - 0.25).floor();
        if self.halved && surface - slab < 0.75 {
            return slab as i32 - 1;
        }
        (surface - 0.5).ceil() as i32 - 1
    }

    /// The highest whole block of ground in column `(x, z)`, in blocks from
    /// the world's corner, as generated ground puts it: under the slab where
    /// its top is laid in half blocks and one lies there, so what is built
    /// stands in the slab's place, not half a block over it.
    pub(crate) fn top(&self, x: i32, z: i32) -> i32 {
        if let Some(top) = self.flat {
            return top;
        }
        let height = self.world.height_at(
            self.position[0] + (x as f32 + 0.5) * self.size,
            self.position[1] + (z as f32 + 0.5) * self.size,
        );
        self.top_of(height)
    }

    /// Whether the highest whole block of ground in each of `feet`'s
    /// columns (see [`Ground::top`]), and the blocks under it down to as
    /// many as the number beside the column, are still ground once the
    /// generator's Volume has pushed and hollowed it, as the column's chunk
    /// is made: no cave, ravine or overhang has opened them. What stands on
    /// the ground stands only where it holds. All hold where the generator
    /// has no Volume, or its Volume takes nothing out.
    pub(crate) fn holds(&self, feet: &[([i32; 2], i32)]) -> Vec<bool> {
        let volume = self.world.volume().filter(|volume| !volume.plain() || volume.caved());
        let Some(volume) = volume.filter(|_| !feet.is_empty()) else { return vec![true; feet.len()] };
        let size = self.size;
        let middle = |axis: usize, v: i32| self.position[axis] + (v as f32 + 0.5) * size;
        let columns: Vec<[i32; 2]> = feet.iter().map(|(column, _)| *column).collect();
        let (xs, zs): (Vec<f32>, Vec<f32>) = columns.iter().map(|&[x, z]| (middle(0, x), middle(1, z))).unzip();
        let mut heights = vec![0.0; columns.len()];
        self.world.heights(&xs, &zs, &mut heights);
        let carried = volume.carried_columns(&columns, middle, &heights);
        // Each block asked about, read where its chunk reads it: a world's own caves from its corner.
        let corner = self.world.volume_at_corner();
        let (mut cx, mut cy, mut cz, mut of) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (at, &([x, z], deep)) in feet.iter().enumerate() {
            let top = self.top_of(heights[at]);
            let (px, pz) = match corner {
                true => ((x as f32 + 0.5) * size, (z as f32 + 0.5) * size),
                false => (xs[at], zs[at]),
            };
            for y in top + 1 - deep.max(1)..=top {
                cx.push(px);
                cy.push((y as f32 + 0.5) * size);
                cz.push(pz);
                of.push(at);
            }
        }
        let rows: Vec<Vec<f32>> = carried.rows().iter().map(|row| of.iter().map(|&at| row[at]).collect()).collect();
        let rows: Vec<&[f32]> = rows.iter().map(Vec::as_slice).collect();
        let (mut inside, mut whole) = (vec![0.0; of.len()], vec![0.0; of.len()]);
        let cells = crate::generate::Cells { x: &cx, y: &cy, z: &cz, carried: &rows, size };
        volume.densities(cells, false, &mut inside, &mut whole);
        // Under a plain height, a block the caves leave alone is ground: every one asked about is under the surface.
        let plain = volume.plain();
        let mut holds = vec![true; feet.len()];
        for (k, &at) in of.iter().enumerate() {
            holds[at] &= inside[k] > 0.0 || plain && inside[k] == whole[k];
        }
        holds
    }

    /// Which of the world's biomes column `(x, z)` is; none where it lists
    /// none.
    fn biome(&self, [x, z]: [i32; 2]) -> Option<usize> {
        let biomes = self.biomes?;
        let (px, pz) =
            (self.position[0] + (x as f32 + 0.5) * self.size, self.position[1] + (z as f32 + 0.5) * self.size);
        let height = self.world.height_at(px, pz);
        let (temperature, humidity) = self.world.climate(px, pz, height);
        let sunk = sunk_at(self.world, &biomes.biomes, [px, pz], height);
        biomes.index_at(px, pz, Place { temperature, humidity, above_sea: height - self.world.sea_level(), sunk })
    }

    /// Whether the ground in column `(x, z)` lies under its generator's sea.
    fn under_sea(&self, x: i32, z: i32) -> bool {
        let height = self.world.height_at(
            self.position[0] + (x as f32 + 0.5) * self.size,
            self.position[1] + (z as f32 + 0.5) * self.size,
        );
        height <= self.world.sea_level()
    }
}

/// Whether sites fit their structures' placements on a world's ground: each
/// worked out once, a whole structure's sites in a batch, and kept for every
/// thread. A site is asked about by every chunk column and every ring of the
/// far view it reaches, and each answer reads the ground, the climate and
/// the water there.
struct SiteFits<'a> {
    ground: Ground<'a>,
    /// What tells each structure's answers from every other's: its world,
    /// biomes and placement, where the world's blocks lie and what it
    /// stands on.
    keys: Vec<(*const PlacedStructure, u64)>,
}

/// The answers kept, by structure and site, in shards so threads seldom
/// wait on one another; each shard keeps `FITS_KEPT` of those asked for
/// lately, as the far view's rings ask about millions of sites.
type Fits = std::sync::Mutex<crate::coord_hash::Lately<(u64, [u32; 2]), bool>>;
static FITS: std::sync::LazyLock<Vec<Fits>> =
    std::sync::LazyLock::new(|| (0..16).map(|_| Fits::new(crate::coord_hash::Lately::new(FITS_KEPT))).collect());
const FITS_KEPT: usize = 1 << 17;

impl<'a> SiteFits<'a> {
    fn new(ground: Ground<'a>, structures: &WorldStructures) -> Self {
        let (world, size) = (ground.world, ground.size);
        let lie = ground.biomes.map_or(0, |biomes| crate::layers::stable_hash(format!("{biomes:?}").as_bytes()));
        let laid = format!("{:?}{}{}", ground.position, ground.halved, world.volume_at_corner());
        let keys = structures
            .list
            .iter()
            .map(|structure| {
                let plant = matches!(structure.blocks, StructureBlocks::Plant { .. });
                let feet = structure.feet();
                let own = format!("{:?}{:?}{plant}{size}{laid}{feet:?}", structure.placement, structure.biomes);
                let bytes = [world.hash().to_le_bytes(), lie.to_le_bytes()].concat();
                let key = crate::layers::stable_hash(&[bytes, own.into_bytes()].concat());
                (structure as *const PlacedStructure, key)
            })
            .collect();
        Self { ground, keys }
    }

    /// Whether each of `sites` fits `structure`.
    fn fits(&self, structure: &PlacedStructure, sites: &[Site]) -> Vec<bool> {
        let key = self.keys.iter().find(|(of, _)| std::ptr::eq(*of, structure)).map_or(0, |(_, key)| *key);
        let place = |site: &Site| (key, site.at.map(f32::to_bits));
        let shard = |place: &(u64, [u32; 2])| {
            let mixed = place.0 ^ u64::from(place.1[0]).rotate_left(17) ^ u64::from(place.1[1]).rotate_left(41);
            &FITS[(mixed % FITS.len() as u64) as usize]
        };
        let mut answers: Vec<Option<bool>> = sites
            .iter()
            .map(|site| {
                let place = place(site);
                shard(&place).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&place)
            })
            .collect();
        let missing: Vec<usize> = (0..sites.len()).filter(|&at| answers[at].is_none()).collect();
        if !missing.is_empty() {
            let asked: Vec<&Site> = missing.iter().map(|&at| &sites[at]).collect();
            for (at, fits) in missing.into_iter().zip(self.work_out(structure, &asked)) {
                answers[at] = Some(fits);
                let place = place(&sites[at]);
                shard(&place).lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(place, fits);
            }
        }
        answers.into_iter().map(|fits| fits.unwrap_or(false)).collect()
    }

    /// Whether each of `sites` fits `structure`, worked out together: its
    /// ground's height over the sea, its slope and its biome; a plant's
    /// distance from the shore, rivers and lakes; and whether what it
    /// stands on is ground once caves and overhangs are cut, so nothing
    /// stands over a cave's opening or a ravine, near or far.
    fn work_out(&self, structure: &PlacedStructure, sites: &[&Site]) -> Vec<bool> {
        let mut fits = self.placed(structure, sites);
        let feet = structure.feet();
        let standing: Vec<usize> = (0..sites.len()).filter(|&at| fits[at]).collect();
        if feet.is_empty() || standing.is_empty() {
            return fits;
        }
        // Every column each stands on, turned as it is, by the block its site falls on.
        let Ground { size, position, .. } = self.ground;
        let asked: Vec<([i32; 2], i32)> = standing
            .iter()
            .flat_map(|&at| {
                let site = sites[at];
                let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
                let turned =
                    turned(feet.iter().map(|&([x, z], deep)| ([x, deep, z], 0)).collect(), site.turns, site.mirrored);
                turned.into_iter().map(move |([x, deep, z], _)| ([block[0] + x, block[1] + z], deep))
            })
            .collect();
        let holds = self.ground.holds(&asked);
        for (k, &at) in standing.iter().enumerate() {
            fits[at] = holds[k * feet.len()..(k + 1) * feet.len()].iter().all(|&holds| holds);
        }
        fits
    }

    /// Whether each of `sites` fits `structure` by the generator's own
    /// ground (see [`SiteFits::work_out`]): all but what caves cut.
    fn placed(&self, structure: &PlacedStructure, sites: &[&Site]) -> Vec<bool> {
        let (world, size, count) = (self.ground.world, self.ground.size, sites.len());
        // Each site, and a block east and north of it, for its slope.
        let mut xs = Vec::with_capacity(count * 3);
        let mut zs = Vec::with_capacity(count * 3);
        for (dx, dz) in [(0.0, 0.0), (size, 0.0), (0.0, size)] {
            xs.extend(sites.iter().map(|site| site.at[0] + dx));
            zs.extend(sites.iter().map(|site| site.at[1] + dz));
        }
        let mut heights = vec![0.0; xs.len()];
        world.heights(&xs, &zs, &mut heights);
        let (xs, zs) = (&xs[..count], &zs[..count]);
        let (height, east, north) = (&heights[..count], &heights[count..count * 2], &heights[count * 2..]);
        let biome_of: Vec<Option<usize>> = match self.ground.biomes {
            Some(biomes) => {
                let (mut warmth, mut wetness) = (vec![0.0; count], vec![0.0; count]);
                world.climates(xs, zs, height, &mut warmth, &mut wetness);
                let sunk = biomes
                    .biomes
                    .list()
                    .iter()
                    .any(|biome| biome.sunk.is_some())
                    .then(|| world.sunk(xs, zs, height))
                    .flatten();
                (0..count)
                    .map(|at| {
                        let above_sea = height[at] - world.sea_level();
                        let sunk = sunk.as_ref().map_or(0.0, |sunk| sunk[at]);
                        let place = Place { temperature: warmth[at], humidity: wetness[at], above_sea, sunk };
                        biomes.index_at(xs[at], zs[at], place)
                    })
                    .collect()
            }
            None => vec![None; count],
        };
        let mut fits: Vec<bool> = (0..count)
            .map(|at| {
                let (rise_east, rise_north) = (east[at] - height[at], north[at] - height[at]);
                let slope = (rise_east * rise_east + rise_north * rise_north).sqrt() / size;
                // Where the land lies in levels, a site's height counts from the floor of its area.
                let lift = self.ground.biomes.map_or(0.0, |biomes| biomes.lift(xs[at], zs[at], height[at]));
                let ground = SiteGround { above_sea: height[at] - lift - world.sea_level(), slope, biome: biome_of[at] };
                structure.placement.fits(ground, |b| structure.biomes.contains(&b))
            })
            .collect();
        // A plant keeps off the shore and away from rivers and lakes, but where it is one that grows by water.
        let plant = matches!(structure.blocks, StructureBlocks::Plant { .. });
        if !plant || structure.placement.by_water {
            return fits;
        }
        for (at, fits) in fits.iter_mut().enumerate() {
            let biome = biome_of[at].and_then(|b| self.ground.biomes?.biomes.list().get(b));
            let shore = biome.map_or(0.0, |biome| biome.ground.shore_height);
            if height[at] - world.sea_level() < shore.max(BEACH) {
                *fits = false;
            }
        }
        let still: Vec<usize> = (0..count).filter(|&at| fits[at]).collect();
        let round = [(0.0, 0.0), (BY_WATER, 0.0), (-BY_WATER, 0.0), (0.0, BY_WATER), (0.0, -BY_WATER)];
        let (wx, wz): (Vec<f32>, Vec<f32>) = still
            .iter()
            .flat_map(|&at| round.iter().map(move |(dx, dz)| (xs[at] + dx, zs[at] + dz)))
            .unzip();
        if let Some((water, _)) = world.water(&wx, &wz) {
            let dry = |water: &f32| *water < crate::generate::nodes::NO_WATER * 0.5;
            for (k, &at) in still.iter().enumerate() {
                fits[at] = water[k * round.len()..(k + 1) * round.len()].iter().all(dry);
            }
        }
        fits
    }
}

/// How far ground `height` metres high at `at` is sunk under the land round
/// about, where any of `biomes` asks.
pub(crate) fn sunk_at(world: &WorldGen, biomes: &Biomes, at: [f32; 2], height: f32) -> f32 {
    if !biomes.list().iter().any(|biome| biome.sunk.is_some()) {
        return 0.0;
    }
    world.sunk(&[at[0]], &[at[1]], &[height]).map_or(0.0, |sunk| sunk[0])
}

/// Builds `structures` into the chunks of chunk column `[cx, cz]` of a world
/// of `size`-metre blocks whose corner lies at `position` in `world`'s, its
/// ground going down to block `bottom`, its biomes `biomes` where it has
/// any: every structure whose site's ground fits and which reaches the
/// column, the parts of it within the column, in the order they are
/// listed, a structure's site left out where its footprint meets an
/// earlier one's (its own sites are kept apart by its placement alone, and
/// a plant's footprint is its trunk, so a wood's crowns may meet).
#[allow(clippy::too_many_arguments)]
pub fn build(
    chunks: &mut Vec<([i32; 3], Chunk)>,
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    at: (f32, [f32; 2], i32, bool),
    [cx, cz]: [i32; 2],
    under: BlockId,
    layers: Option<&WorldLayers>,
) {
    let mut set = |block_at: [i32; 3], block: BlockId| put_block(chunks, [cx, cz], at.2, block_at, block);
    let area = ([cx, cz], [cx + 1, cz + 1]);
    build_looking(&mut set, structures, world, biomes, at, area, under, (&looks(structures, at.0), &|_| None, layers));
    chunks.retain(|(_, chunk)| !chunk.is_air());
    chunks.sort_by_key(|(key, _)| key[1]);
}

/// As [`build`], over every chunk column from `first` up to `end` at once,
/// each block built handed to `set` with its place in the world's blocks,
/// in the order it is built: air where the ground is cleared, a block set
/// twice as it was set last. What a map of the ground is made from, which
/// has no chunks to build into.
#[allow(clippy::too_many_arguments)]
pub fn build_over(
    set: &mut dyn FnMut([i32; 3], BlockId),
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    at: (f32, [f32; 2], i32, bool),
    area: ([i32; 2], [i32; 2]),
    under: BlockId,
) {
    build_looking(set, structures, world, biomes, at, area, under, (&looks(structures, at.0), &|_| None, None));
}

/// [`build_over`], each structure standing on the ground `feet` gives for
/// the column its site is on, as the highest block of ground there, where
/// it gives any; else on the generator's own. The far view's coarser rings
/// stand each tree and house on their own coarse ground this way, which is
/// sampled at the middles of their larger blocks: put where the generator
/// has the ground, one on a sharp ridge or in a saddle would hang over the
/// ring's ground or sink into it.
#[allow(clippy::too_many_arguments)]
pub fn build_over_standing(
    set: &mut dyn FnMut([i32; 3], BlockId),
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    at: (f32, [f32; 2], i32, bool),
    area: ([i32; 2], [i32; 2]),
    under: BlockId,
    feet: &dyn Fn([i32; 2]) -> Option<i32>,
    layers: Option<&WorldLayers>,
) {
    build_looking(set, structures, world, biomes, at, area, under, (&looks(structures, at.0), feet, layers));
}

/// The dungeons `structures` lays out within `reach` metres of `near`, a
/// point in metres in `world` (the generator's own world), in a world of
/// blocks laid as `at` says (block size, corner, lowest block, half-block
/// top): each structure's place in the list, its site, and the layout kept
/// for it, as building its chunks finds it. Sites whose ground does not
/// fit their placement are left out; a site an earlier structure keeps
/// another from is not.
pub fn dungeons_near(
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    at: (f32, [f32; 2], i32, bool),
    near: [f32; 2],
    reach: f32,
) -> Vec<(usize, Site, Arc<super::dungeon::DungeonLayout>)> {
    let checks =
        SiteFits::new(Ground { world, size: at.0, position: at.1, biomes, halved: at.3, flat: None }, structures);
    let mut found = Vec::new();
    for (which, structure) in structures.list.iter().enumerate() {
        let StructureBlocks::Dungeon(dungeon) = &structure.blocks else { continue };
        let far = reach + dungeon.reach() * at.0;
        let sites = structure.placement.tries_near(world.seed(), near, near, far);
        let fitting = checks.fits(structure, &sites);
        for (site, fits) in sites.into_iter().zip(fitting) {
            if !fits || kept_out(structures, &checks, which, &site, at.0, world.seed()) {
                continue;
            }
            if let Some(layout) = super::dungeon::layout_at(dungeon, world, at, &site) {
                found.push((which, site, layout));
            }
        }
    }
    found
}

/// A town as a world lays it out, worked out from its site and the
/// generator's ground alone, with no chunk made: the structure's place in
/// the world's list, its site, the block of its middle, and its buildings,
/// the middle first.
#[derive(Clone, Debug, PartialEq)]
pub struct TownPlan {
    pub which: usize,
    pub site: Site,
    pub centre: [i32; 2],
    pub buildings: Vec<TownBuilding>,
}

/// A building of a town: its kind of house by its place in the town's list
/// (none for its middle), the block its middle stands over, the block its
/// lowest blocks are laid in, its size before it is turned, how many
/// quarter turns it is turned (its door, on its model's -z side, then faces
/// that way round), and the block just outside its door.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TownBuilding {
    pub house: Option<usize>,
    pub at: [i32; 2],
    pub floor: i32,
    pub size: [u32; 3],
    pub turns: u8,
    pub door: Option<[i32; 2]>,
}

/// The towns of `structures` whose sites lie within `reach` metres of
/// `near` (plus each town's own reach), in metres in the generator's world,
/// over the ground `world` makes, laid as `at` says (as
/// [`dungeons_near`]): each laid out as building its chunks lays it, its
/// buildings standing on the generator's ground. World layers over the
/// ground are not looked at.
pub fn towns_near(
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    at: (f32, [f32; 2], i32, bool),
    near: [f32; 2],
    reach: f32,
) -> Vec<TownPlan> {
    let ground = Ground { world, size: at.0, position: at.1, biomes, halved: at.3, flat: None };
    let checks = SiteFits::new(Ground { world, size: at.0, position: at.1, biomes, halved: at.3, flat: None }, structures);
    let mut found = Vec::new();
    for (which, structure) in structures.list.iter().enumerate() {
        let StructureBlocks::Town(town) = &structure.blocks else { continue };
        let far = reach + town.reach() * at.0;
        let sites = structure.placement.tries_near(world.seed(), near, near, far);
        let fitting = checks.fits(structure, &sites);
        for (site, fits) in sites.into_iter().zip(fitting) {
            if !fits || kept_out(structures, &checks, which, &site, at.0, world.seed()) {
                continue;
            }
            let centre = [0, 1].map(|axis| ((site.at[axis] - at.1[axis]) / at.0).floor() as i32);
            let (parts, _) = town.layout(site.seed, centre, &ground);
            let buildings = parts
                .iter()
                .map(|part| {
                    let piece = match part.house {
                        Some((kind, _)) => &town.houses[kind].piece,
                        None => town.middle.as_ref().expect("a town's part that is no house is its middle"),
                    };
                    TownBuilding {
                        house: part.house.map(|(kind, _)| kind),
                        at: part.at,
                        floor: ground.top(part.at[0], part.at[1]) + 1 - part.sink,
                        size: piece.size,
                        turns: part.turns,
                        door: part.house.map(|(_, door)| [part.at[0] + door[0], part.at[1] + door[1]]),
                    }
                })
                .collect();
            found.push(TownPlan { which, site, centre, buildings });
        }
    }
    found
}

/// Whether a site of structure `which` is kept out by a site of one listed
/// before it standing too near: within both their footprints, or the
/// distance either keeps. Towns' streets and buildings are not looked at.
fn kept_out(structures: &WorldStructures, checks: &SiteFits, which: usize, site: &Site, size: f32, seed: u32) -> bool {
    let own = &structures.list[which];
    let (footprint, apart) = (own.footprint(), own.placement.apart.max(0.0) / size);
    structures.list[..which].iter().any(|earlier| {
        let reach = (footprint + earlier.footprint()).max(apart).max(earlier.placement.apart.max(0.0) / size) * size;
        let sites = earlier.placement.tries_near(seed, site.at, site.at, reach);
        let fitting = checks.fits(earlier, &sites);
        sites.iter().zip(fitting).any(|(other, fits)| {
            let d = ((other.at[0] - site.at[0]).powi(2) + (other.at[1] - site.at[1]).powi(2)).sqrt() / size;
            let keeps = (earlier.placement.apart.max(0.0) / size).max(apart);
            fits && (d < footprint + earlier.footprint() || d < keeps)
        })
    })
}

/// How far past the columns being built each structure's sites are looked
/// for, in metres, for what is built there to be what any other columns
/// would build: as far as it reaches itself, and as far as any site of a
/// structure listed after it is looked for and then as near as a site of
/// that one may stand to one of its own before it is kept from standing.
/// So a wood's trees, which nothing listed after them keeps clear of, are
/// looked for a crown's width off, and not as far as a town reaches.
fn looks(structures: &WorldStructures, size: f32) -> Vec<f32> {
    let list = &structures.list;
    let apart = |structure: &PlacedStructure| structure.placement.apart.max(0.0) / size;
    // How near a site of `later` may stand to a kept site of `earlier` and be left out for it (see
    // `Taken::meets`): by their footprints, by all the later one reaches from an earlier town's buildings and
    // streets, which lie within the town's own reach, and by the distance either keeps.
    let between = |earlier: &PlacedStructure, later: &PlacedStructure| {
        let round = earlier.footprint() + later.footprint();
        let town = match &earlier.blocks {
            StructureBlocks::Town(_) => (earlier.reach() + later.reach().max(later.footprint()) + 2.0) * 1.5,
            _ => 0.0,
        };
        round.max(town).max(apart(earlier)).max(apart(later)) + 2.0
    };
    let mut far = vec![0.0f32; list.len()];
    for at in (0..list.len()).rev() {
        let own = list[at].reach() + 2.0;
        let for_later = (at + 1..list.len()).map(|next| far[next] + between(&list[at], &list[next]));
        far[at] = for_later.fold(own, f32::max);
    }
    far.into_iter().map(|blocks| blocks * size).collect()
}

/// [`build_over`], each structure's sites looked for `looks` metres past
/// the columns, by its place in the list, each standing where `feet` says
/// (see [`build_over_standing`]).
#[allow(clippy::too_many_arguments)]
fn build_looking(
    set: &mut dyn FnMut([i32; 3], BlockId),
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&WorldBiomes>,
    (size, position, bottom, halved): (f32, [f32; 2], i32, bool),
    (first, end): ([i32; 2], [i32; 2]),
    under: BlockId,
    (looks, feet, layers): (&[f32], &dyn Fn([i32; 2]) -> Option<i32>, Option<&WorldLayers>),
) {
    let side = CHUNK as i32;
    let ground = Ground { world, size, position, biomes, halved, flat: None };
    // Nothing is built below the world's lowest block.
    let mut set = |at: [i32; 3], block: BlockId| {
        if at[1] >= bottom {
            set(at, block);
        }
    };
    // The columns' own blocks, in metres in the generator's world.
    let low = [position[0] + (first[0] * side) as f32 * size, position[1] + (first[1] * side) as f32 * size];
    let high = [position[0] + (end[0] * side) as f32 * size, position[1] + (end[1] * side) as f32 * size];
    // Every site kept, with its place in blocks and the blocks it takes up along the ground, to leave out a later
    // structure's where an earlier one's stands.
    let mut kept = KeptSites::default();
    // Whether the ground at a site fits a structure's placement: worked out for all of a structure's sites at once,
    // and kept for every thread (see `SiteFits`).
    let checks = SiteFits::new(ground, structures);
    let fits = |structure: &PlacedStructure, site: &Site| checks.fits(structure, std::slice::from_ref(site))[0];
    let within = |x: i32, z: i32| {
        let (cx, cz) = (x.div_euclid(side), z.div_euclid(side));
        cx >= first[0] && cx < end[0] && cz >= first[1] && cz < end[1]
    };
    for (which, structure) in structures.list.iter().enumerate() {
        let (reach, footprint) = (structure.reach(), structure.footprint());
        let apart = structure.placement.apart.max(0.0) / size;
        // A town's roads to the next towns of its kind, laid before any of its buildings.
        if let StructureBlocks::Town(town) = &structure.blocks
            && town.roads
            && town.road != AIR
        {
            let roads = roads_near(structure, town, world.seed(), (low, high), &ground, &fits);
            let (mut cells, mut laid) = (Vec::new(), crate::coord_hash::CoordSet::default());
            for road in &roads {
                for &point in road.iter() {
                    lay(&mut cells, &mut laid, point, town.road_width.max(1) as f32);
                }
            }
            let sea_top = (world.sea_level() / size - 0.5).ceil() as i32 - 1;
            for &[x, z] in cells.iter().filter(|&&[x, z]| within(x, z)) {
                let mut put = |y: i32, block: BlockId| set([x, y, z], block);
                let top = ground.top(x, z);
                if ground.under_sea(x, z) && town.bridge != AIR {
                    // A deck a block over the water, posts down to the bed every few blocks.
                    let deck = sea_top + 1;
                    put(deck, town.bridge);
                    for y in deck + 1..deck + 4 {
                        put(y, AIR);
                    }
                    if x.rem_euclid(4) == 0 && z.rem_euclid(4) == 0 {
                        for y in top + 1..deck {
                            put(y, town.bridge);
                        }
                    }
                } else {
                    put(top, town.road);
                    for y in top + 1..top + 4 {
                        put(y, AIR);
                    }
                }
            }
            kept.push(Taken { which, site: None, round: None, buildings: Vec::new(), streets: cells });
        }
        // Sites are looked for as far out as any that is built here, or that may stand in the way of one of a
        // later structure's that is, can lie (see `looks`): so a later one finds every earlier site that may
        // stand in its way or within its distance, from any chunk alike.
        let sites = structure.placement.tries_near(world.seed(), low, high, looks[which]);
        let fitting = checks.fits(structure, &sites);
        for (site, fitting) in sites.into_iter().zip(fitting) {
            let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
            if !fitting {
                continue;
            }
            // Kept clear of what earlier structures take up; its own are kept apart by its placement's separation.
            // And as far from another structure's site as the further either keeps.
            let too_near = |taken: &Taken| {
                taken.site.is_some_and(|(other, other_apart)| {
                    let (dx, dz) = ((other[0] - block[0]) as f32, (other[1] - block[1]) as f32);
                    (dx * dx + dz * dz).sqrt() < apart.max(other_apart)
                })
            };
            // Only those kept whose ground reaches as far as this one's could meet it.
            let by = footprint.ceil() + reach.ceil() + apart + 1.0;
            if kept.near(block, by).filter(|taken| taken.which != which).any(|taken| {
                taken.meets(block, footprint, reach) || too_near(taken)
            }) {
                continue;
            }
            // Nor where a world layer leaves out what the generator builds (see `world_layers`).
            let round = (footprint + reach).ceil() as i32;
            if layers.is_some_and(|layers| layers.keeps_out(block.map(|v| v - round), block.map(|v| v + round))) {
                continue;
            }
            // A town's layout, worked out once: what it takes up is its streets and buildings.
            let layout = match &structure.blocks {
                StructureBlocks::Town(town) => Some(town.layout(site.seed, block, &ground)),
                _ => None,
            };
            let (buildings, streets) = match &layout {
                Some((parts, paths)) => (parts.iter().map(|part| (part.at, part.half())).collect(), paths.clone()),
                None => (Vec::new(), Vec::new()),
            };
            let taken_site = Some((block, apart));
            kept.push(Taken {
                which,
                site: taken_site,
                round: layout.is_none().then_some((block, footprint)),
                buildings,
                streets,
            });
            // Only its blocks within these columns are built here.
            let past = reach.ceil() as i32 + 1;
            if block[0] < first[0] * side - past
                || block[0] > end[0] * side + past
                || block[1] < first[1] * side - past
                || block[1] > end[1] * side + past
            {
                continue;
            }
            let mut put = |x: i32, y: i32, z: i32, block: BlockId| {
                if within(x, z) {
                    set([x, y, z], block);
                }
            };
            if let StructureBlocks::Dungeon(dungeon) = &structure.blocks {
                let key = super::dungeon::layout_key(dungeon, &ground, block, bottom);
                let layout = super::dungeon::cached_layout(key, || {
                    super::dungeon::plan(dungeon, &ground, block, site.seed, bottom)
                });
                if let Some(layout) = layout {
                    let turn = |placed: BlockId, turns: u8| structures.turned_block(placed, turns, false);
                    let columns = ([first[0] * side, first[1] * side], [end[0] * side - 1, end[1] * side - 1]);
                    super::dungeon::build(dungeon, &layout, &ground, under, columns, &turn, &mut |at, placed| {
                        set(at, placed)
                    });
                }
                continue;
            }
            let parts = match (&structure.blocks, layout) {
                (StructureBlocks::Town(town), Some((parts, paths))) => {
                    // Paths first, along the ground, the air over them cleared a few blocks up.
                    for [x, z] in paths {
                        if !within(x, z) {
                            continue;
                        }
                        let top = ground.top(x, z);
                        put(x, top, z, town.path);
                        for y in top + 1..top + 4 {
                            put(x, y, z, AIR);
                        }
                    }
                    parts
                }
                _ => {
                    let mut pieces = structure.pieces(&site);
                    // A plant's vines hang in the open air: none where the ground rises into them.
                    if let StructureBlocks::Plant { vine, .. } = &structure.blocks {
                        let base = ground.top(block[0], block[1]) + 1 - structure.sink;
                        let buried = |at: &[i32; 3]| base + at[1] <= ground.top(block[0] + at[0], block[1] + at[2]);
                        pieces.retain(|(at, piece)| *vine == AIR || piece != vine || !buried(at));
                    }
                    let (turns, mirrored) = (site.turns, site.mirrored);
                    let (ground, sink) = (structure.ground, structure.sink);
                    vec![Part { at: block, pieces, ground, sink, turns, mirrored, house: None }]
                }
            };
            for mut part in parts {
                let foot = feet(part.at).unwrap_or_else(|| ground.top(part.at[0], part.at[1]));
                if !structures.turns.is_empty() && (part.turns % 4 != 0 || part.mirrored) {
                    for (_, block) in &mut part.pieces {
                        *block = structures.turned_block(*block, part.turns, part.mirrored);
                    }
                }
                build_part(&mut put, &ground, &part, foot, under);
            }
        }
    }
}

/// Puts `block` at block `at` of chunk column `[cx, cz]`'s chunks, a chunk
/// made where there is none, and nothing below block `bottom`.
pub(crate) fn put_block(
    chunks: &mut Vec<([i32; 3], Chunk)>,
    [cx, cz]: [i32; 2],
    bottom: i32,
    [x, y, z]: [i32; 3],
    block: BlockId,
) {
    let side = CHUNK as i32;
    if y < bottom {
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
}

/// The roads of `town`, a town `structure` builds, that may cross the box
/// from `low` to `high`, in metres in the generator's world: each a road's
/// steps (see `Town::road_between`). Towns join the towns in the cells next
/// to theirs along x, along z and one way corner to corner, each pair by its own
/// chance of `link`, decided from the pair's cells alone, so every chunk
/// agrees which roads there are.
fn roads_near(
    structure: &PlacedStructure,
    town: &Town,
    seed: u32,
    (low, high): ([f32; 2], [f32; 2]),
    ground: &Ground,
    fits: &dyn Fn(&PlacedStructure, &Site) -> bool,
) -> Vec<Arc<Vec<[f32; 2]>>> {
    let size = ground.size;
    let spacing = structure.placement.spacing.max(1.0);
    // Towns as far off as a road between two cells' towns could bend through here.
    let reach = spacing * 2.6;
    let towns: Vec<Site> = structure
        .placement
        .tries_near(seed, low, high, reach)
        .into_iter()
        .filter(|site| fits(structure, site))
        .collect();
    let block = |site: &Site| [0, 1].map(|axis| ((site.at[axis] - ground.position[axis]) / size).floor() as i32);
    let mut roads = Vec::new();
    for a in &towns {
        // One way corner to corner alone, so no two roads cross in an X between four towns.
        for step in [[1, 0], [0, 1], [1, 1]] {
            let cell = [a.cell[0] + step[0], a.cell[1] + step[1]];
            let Some(b) = towns.iter().find(|b| b.cell == cell) else { continue };
            let pair = a.cell[0].wrapping_mul(73_856_093)
                ^ a.cell[1].wrapping_mul(19_349_663)
                ^ cell[0].wrapping_mul(83_492_791)
                ^ cell[1].wrapping_mul(50_331_653);
            let pair = Draws(seed ^ structure.placement.salt ^ pair as u32).next();
            if unit_of(pair) >= town.link.clamp(0.0, 1.0) {
                continue;
            }
            // Only those whose way could cross the box are worked out: within half their length of the line.
            let (from, to) = (block(a), block(b));
            let length = (((to[0] - from[0]).pow(2) + (to[1] - from[1]).pow(2)) as f32).sqrt();
            let margin = (length * 0.5 + town.road_width as f32) * size;
            let box_low = [a.at[0].min(b.at[0]) - margin, a.at[1].min(b.at[1]) - margin];
            let box_high = [a.at[0].max(b.at[0]) + margin, a.at[1].max(b.at[1]) + margin];
            if box_high[0] < low[0] || box_low[0] > high[0] || box_high[1] < low[1] || box_low[1] > high[1] {
                continue;
            }
            roads.push(town.road_between(pair, from, to, ground));
        }
    }
    roads
}

/// What a kept site takes up along the ground: a round footprint, or the
/// boxes of a town's buildings and streets, each a middle and how far it
/// reaches along x and z.
struct Taken {
    which: usize,
    /// Its site, in blocks, and the least its structure keeps between its
    /// sites and another's, in blocks; none for a town's roads.
    site: Option<([i32; 2], f32)>,
    round: Option<([i32; 2], f32)>,
    /// A town's buildings, which what comes after keeps clear of by all it
    /// reaches, so no crown hangs into a house; and its streets, kept clear
    /// of by what stands on the ground.
    buildings: Vec<([i32; 2], [i32; 2])>,
    streets: Vec<[i32; 2]>,
}

/// The sites kept so far, and a grid of where each reaches, so a site
/// looks only at those near it rather than at every one: a far tile's
/// trees are tens of thousands.
#[derive(Default)]
struct KeptSites {
    all: Vec<Taken>,
    cells: std::collections::HashMap<[i32; 2], Vec<u32>>,
}

/// How wide a cell of the grid is, in blocks.
const KEPT_CELL: i32 = 32;

impl KeptSites {
    fn push(&mut self, taken: Taken) {
        let at = self.all.len() as u32;
        let [low, high] = taken.reaches();
        let cell = |v: i32| v.div_euclid(KEPT_CELL);
        for z in cell(low[1])..=cell(high[1]) {
            for x in cell(low[0])..=cell(high[0]) {
                self.cells.entry([x, z]).or_default().push(at);
            }
        }
        self.all.push(taken);
    }

    /// Every one kept whose ground reaches within `by` blocks of `at`, and
    /// perhaps a few more, each once.
    fn near(&self, at: [i32; 2], by: f32) -> impl Iterator<Item = &Taken> {
        let by = by.ceil() as i32;
        let cell = |v: i32| v.div_euclid(KEPT_CELL);
        let mut found: Vec<u32> = Vec::new();
        for z in cell(at[1] - by)..=cell(at[1] + by) {
            for x in cell(at[0] - by)..=cell(at[0] + by) {
                found.extend(self.cells.get(&[x, z]).into_iter().flatten());
            }
        }
        found.sort_unstable();
        found.dedup();
        found.into_iter().map(|at| &self.all[at as usize])
    }
}

impl Taken {
    /// The corners of the box everything it takes up lies within, in
    /// blocks, as far as anything of it keeps another away.
    fn reaches(&self) -> [[i32; 2]; 2] {
        let (mut low, mut high) = ([i32::MAX; 2], [i32::MIN; 2]);
        let mut take = |middle: [i32; 2], half: [i32; 2]| {
            for axis in 0..2 {
                low[axis] = low[axis].min(middle[axis] - half[axis]);
                high[axis] = high[axis].max(middle[axis] + half[axis]);
            }
        };
        if let Some((middle, apart)) = self.site {
            take(middle, [apart.ceil() as i32 + 1; 2]);
        }
        if let Some((middle, round)) = self.round {
            take(middle, [round.ceil() as i32 + 1; 2]);
        }
        for &(middle, half) in &self.buildings {
            take(middle, half);
        }
        for &cell in &self.streets {
            take(cell, [0, 0]);
        }
        if low[0] > high[0] {
            return [[0, 0], [0, 0]];
        }
        [low, high]
    }

    /// Whether something standing `footprint` blocks round `at` and
    /// reaching `reach` blocks round it meets it.
    fn meets(&self, at: [i32; 2], footprint: f32, reach: f32) -> bool {
        let round = self.round.is_some_and(|(middle, other)| {
            let (dx, dz) = ((middle[0] - at[0]) as f32, (middle[1] - at[1]) as f32);
            (dx * dx + dz * dz).sqrt() < other + footprint
        });
        let (stands, reaches) = (footprint.ceil() as i32, reach.ceil() as i32);
        let near = |middle: &[i32; 2], half: [i32; 2], by: i32| {
            (middle[0] - at[0]).abs() <= half[0] + by && (middle[1] - at[1]).abs() <= half[1] + by
        };
        round
            || self.buildings.iter().any(|(middle, half)| near(middle, *half, reaches))
            || self.streets.iter().any(|cell| near(cell, [0, 0], stands))
    }
}

/// Builds `part` standing on block `foot` of its column: the ground built up
/// under it, or cleared over it too, as its fit says, then its blocks.
fn build_part(put: &mut impl FnMut(i32, i32, i32, BlockId), ground: &Ground, part: &Part, foot: i32, under: BlockId) {
    let (block, pieces) = (part.at, &part.pieces);
    let base = foot + 1 - part.sink;
    if part.ground != GroundFit::Leave {
        // Each column of its footprint: its lowest block, cleared over, and built up under.
        let mut footprint: Vec<([i32; 2], i32, BlockId)> = Vec::new();
        for &([x, y, z], piece) in pieces {
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
            if part.ground != GroundFit::Cut {
                for y in (top + 1).min(floor)..floor {
                    put(x, y, z, under);
                }
            }
            if matches!(part.ground, GroundFit::Flatten | GroundFit::Cut) {
                for y in floor..=top.max(floor).min(base + tallest + 2) {
                    put(x, y, z, AIR);
                }
            }
        }
    }
    for &([x, y, z], piece) in pieces {
        put(block[0] + x, base + y, block[1] + z, piece);
    }
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
        let short = StructureModel { size: [2, 2, 2], palette: vec!["a".into()], runs: "7:1".into(), states: Vec::new() };
        assert_eq!(short.blocks(), None);
        let unknown = StructureModel { size: [2, 2, 2], palette: vec!["a".into()], runs: "8:2".into(), states: Vec::new() };
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
            blocks: StructureBlocks::Plant { plant: PlantDef::default(), wood: 7, leaves: 8, vine: 0, logs: Vec::new() },
            biomes: Vec::new(),
        };
        WorldStructures { list: vec![hut, trees], ..WorldStructures::default() }
    }

    #[test]
    fn trees_keep_off_the_beach_and_away_from_rivers_but_where_they_grow_by_water() {
        // The islands' coast where a river runs out into the sea, 256 m a side.
        let world = crate::generate::starters().into_iter().find(|s| s.name == "Archipelago").unwrap().world(7, 1.0);
        let sea = world.sea_level();
        let position = [592.0, 64.0];
        let trees = |by_water: bool| {
            let trees = PlacedStructure {
                placement: Placement {
                    spacing: 8.0,
                    separation: 3.0,
                    chance: 0.9,
                    slope: 35.0,
                    by_water,
                    ..Placement::default()
                },
                ground: GroundFit::Leave,
                sink: 0,
                blocks: StructureBlocks::Plant {
                    plant: PlantDef::default(),
                    wood: 7,
                    leaves: 8,
                    vine: 0,
                    logs: Vec::new(),
                },
                biomes: Vec::new(),
            };
            let structures = WorldStructures { list: vec![trees], ..WorldStructures::default() };
            // Each trunk's foot: the lowest of its wood in a column.
            let mut feet: std::collections::HashMap<[i32; 2], i32> = std::collections::HashMap::new();
            let mut set = |at: [i32; 3], block: BlockId| {
                if block == 7 {
                    let foot = feet.entry([at[0], at[2]]).or_insert(at[1]);
                    *foot = (*foot).min(at[1]);
                }
            };
            build_over(&mut set, &structures, &world, None, (1.0, position, -64, false), ([0, 0], [8, 8]), 2);
            // Those whose wood reaches down to the ground: trunks and their roots, not branches over the ground.
            feet.into_iter()
                .map(|([x, z], foot)| ([position[0] + x as f32 + 0.5, position[1] + z as f32 + 0.5], foot))
                .filter(|&([x, z], foot)| foot as f32 <= world.height_at(x, z) + 1.0)
                .map(|(at, _)| at)
                .collect::<Vec<_>>()
        };
        let wet = |[x, z]: [f32; 2]| {
            let round = [(0.0, 0.0), (2.0, 0.0), (-2.0, 0.0), (0.0, 2.0), (0.0, -2.0)];
            let (xs, zs): (Vec<f32>, Vec<f32>) = round.iter().map(|(dx, dz)| (x + dx, z + dz)).unzip();
            let (water, _) = world.water(&xs, &zs).unwrap();
            water.iter().any(|&water| water > crate::generate::nodes::NO_WATER * 0.5)
        };
        let shore = |at: [f32; 2]| world.height_at(at[0], at[1]) - sea < 1.5;
        let dry = trees(false);
        assert!(dry.len() > 100, "{} trees", dry.len());
        assert!(dry.iter().all(|&at| !shore(at) && !wet(at)), "trees on the beach or by the river");
        // Those that grow by water come down to the shore and the river's banks.
        let by = trees(true);
        assert!(by.len() > dry.len() && by.iter().any(|&at| shore(at) || wet(at)), "{} by water", by.len());
    }

    #[test]
    fn a_chunk_column_builds_the_part_of_every_structure_that_reaches_it_as_it_is_built_among_its_neighbours() {
        let world = Preset::RollingHills.world();
        let structures = hut_and_trees();
        let make = |[cx, cz]: [i32; 2]| {
            // Plain ground of block 1 up to the generator's height, a block a metre.
            let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
            let ground =
                Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false, flat: None };
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
            build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -4, false), [cx, cz], 2, None);
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

    #[test]
    fn every_vine_of_a_wood_hangs_from_its_trees_leaves_or_wood_never_from_the_air() {
        let world = Preset::RollingHills.world();
        // A close wood of oaks with vines on every tree, of logs 7, leaves 8 and vines 9, over plain ground 1.
        let oak = PlantDef { vines: 1.0, ..PlantDef::default() };
        let trees = PlacedStructure {
            placement: Placement {
                spacing: 9.0,
                separation: 4.0,
                chance: 0.9,
                salt: 2,
                slope: 40.0,
                on: SiteOn::Land,
                ..Placement::default()
            },
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Plant { plant: oak, wood: 7, leaves: 8, vine: 9, logs: Vec::new() },
            biomes: Vec::new(),
        };
        let structures = WorldStructures { list: vec![trees], ..WorldStructures::default() };
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false, flat: None };
        let make = |[cx, cz]: [i32; 2]| {
            let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
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
            build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -4, false), [cx, cz], 2, None);
            chunks
        };
        let columns: Vec<Vec<([i32; 3], Chunk)>> = (0..16).map(|i| make([i % 4 - 2, i / 4 - 2])).collect();
        let block_at = |x: i32, y: i32, z: i32| {
            let column = &columns[((z.div_euclid(32) + 2) * 4 + x.div_euclid(32) + 2) as usize];
            column
                .iter()
                .find(|(k, _)| k[1] == y.div_euclid(32))
                .map_or(AIR, |(_, c)| c.get(x.rem_euclid(32) as u32, y.rem_euclid(32) as u32, z.rem_euclid(32) as u32))
        };
        let (mut vines, mut loose) = (0, Vec::new());
        for z in -60..60 {
            for x in -60..60 {
                for y in -4..80 {
                    if block_at(x, y, z) != 9 {
                        continue;
                    }
                    vines += 1;
                    // Up through the vine to what it hangs from; or, beside it, the trunk it climbs.
                    let over = (1..16).map(|up| block_at(x, y + up, z)).find(|&block| block != 9).unwrap_or(AIR);
                    let beside =
                        [[1, 0], [-1, 0], [0, 1], [0, -1]].iter().any(|d| block_at(x + d[0], y, z + d[1]) == 7);
                    if over == AIR && !beside {
                        loose.push([x, y, z]);
                    }
                }
            }
        }
        assert!(vines > 100, "{vines} blocks of vine in the wood");
        let first = &loose[..loose.len().min(12)];
        assert!(loose.is_empty(), "{} of {vines} blocks of vine hang from the air: {first:?}", loose.len());
    }

    /// A town of block 11's paths: a well of block 10, 5 by 3 by 5, and cottages of walls 12 with a door in the
    /// middle of their -z side, 7 by 5 by 7 and 5 by 4 by 5.
    fn village() -> Town {
        let model = |[sx, sy, sz]: [usize; 3], wall: BlockId| {
            let mut blocks = vec![AIR; sx * sy * sz];
            for y in 0..sy {
                for z in 0..sz {
                    for x in 0..sx {
                        let edge = x == 0 || z == 0 || x == sx - 1 || z == sz - 1;
                        let door = z == 0 && x == sx / 2 && (1..3).contains(&y);
                        blocks[(y * sz + z) * sx + x] = if y == 0 || (edge && !door) { wall } else { AIR };
                    }
                }
            }
            TownPiece { size: [sx as u32, sy as u32, sz as u32], blocks, ground: GroundFit::Flatten, sink: 0 }
        };
        Town {
            middle: Some(model([5, 3, 5], 10)),
            houses: vec![
                TownHouseKind::anywhere(model([7, 5, 7], 12), 2.0),
                TownHouseKind::anywhere(model([5, 4, 5], 12), 1.0),
            ],
            count: [8, 14],
            radius: [24.0, 44.0],
            path: 11,
            path_width: 3,
            wander: 0.5,
            branching: 0.6,
            roads: false,
            road: AIR,
            road_width: 3,
            bridge: AIR,
            link: 0.8,
        }
    }

    #[test]
    fn a_town_grows_streets_from_its_middle_with_houses_along_them_apart_facing_them() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false, flat: None };
        let town = village();
        let mut houses_laid = 0;
        for seed in 0..12u32 {
            let centre = [seed as i32 * 97 - 600, 300 - seed as i32 * 53];
            let (parts, paths) = town.layout(seed, centre, &ground);
            assert_eq!(town.layout(seed, centre, &ground).1, paths, "the same from the same seed");
            assert_eq!(parts[0].at, centre, "the well in the middle");
            let houses = &parts[1..];
            assert!(houses.len() <= 14, "seed {seed}: {} houses", houses.len());
            houses_laid += houses.len();
            for (i, a) in parts.iter().enumerate() {
                for b in &parts[i + 1..] {
                    let (ha, hb) = (a.half(), b.half());
                    let apart = (a.at[0] - b.at[0]).abs() > ha[0] + hb[0] || (a.at[1] - b.at[1]).abs() > ha[1] + hb[1];
                    assert!(apart, "seed {seed}: buildings at {:?} and {:?} overlap", a.at, b.at);
                }
            }
            for house in houses {
                // Its door: the gap in its lowest wall row, on the side facing the middle.
                let wall: Vec<[i32; 2]> =
                    house.pieces.iter().filter(|(at, _)| at[1] == 1).map(|(at, _)| [at[0], at[2]]).collect();
                let half = wall.iter().map(|[x, z]| x.abs().max(z.abs())).max().unwrap();
                let ring: Vec<[i32; 2]> =
                    (-half..=half).flat_map(|a| [[a, -half], [a, half], [-half, a], [half, a]]).collect();
                let door = ring.iter().find(|cell| !wall.contains(cell)).expect("a door");
                // A path starts just outside it.
                let outside = [house.at[0] + door[0] + door[0].signum(), house.at[1] + door[1] + door[1].signum()];
                let near =
                    paths.iter().any(|cell| (cell[0] - outside[0]).abs() <= 1 && (cell[1] - outside[1]).abs() <= 1);
                assert!(near, "seed {seed}: no path at the door of the house at {:?}", house.at);
            }
            // Paths run on to the middle: each cell is joined to the well's edge through the others.
            let mut reached: Vec<[i32; 2]> = paths
                .iter()
                .filter(|cell| (((cell[0] - centre[0]).pow(2) + (cell[1] - centre[1]).pow(2)) as f32).sqrt() < 6.0)
                .copied()
                .collect();
            let mut at = 0;
            while at < reached.len() {
                let cell = reached[at];
                for next in paths.iter().filter(|o| (o[0] - cell[0]).abs() <= 1 && (o[1] - cell[1]).abs() <= 1) {
                    if !reached.contains(next) {
                        reached.push(*next);
                    }
                }
                at += 1;
            }
            assert_eq!(reached.len(), paths.len(), "seed {seed}: every path joined to the middle, each block once");
        }
        assert!(houses_laid >= 12 * 6, "{houses_laid} houses in twelve towns");
    }

    #[test]
    fn a_town_is_built_across_chunk_columns_the_same_from_any_of_them() {
        let world = Preset::RollingHills.world();
        let town = PlacedStructure {
            placement: Placement {
                spacing: 120.0,
                separation: 60.0,
                chance: 1.0,
                slope: 25.0,
                height: Some([1.0, 400.0]),
                ..Placement::default()
            },
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Town(village()),
            biomes: Vec::new(),
        };
        let structures = WorldStructures { list: vec![town], ..WorldStructures::default() };
        let make = |[cx, cz]: [i32; 2]| {
            let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
            build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -4, false), [cx, cz], 2, None);
            chunks
        };
        let count = |chunks: &[([i32; 3], Chunk)], block: BlockId| -> usize {
            chunks
                .iter()
                .map(|(_, c)| (0..32 * 32 * 32).filter(|i| c.get(i % 32, i / 1024, (i / 32) % 32) == block).count())
                .sum()
        };
        let columns: Vec<Vec<([i32; 3], Chunk)>> = (0..36).map(|i| make([i % 6 - 3, i / 6 - 3])).collect();
        let (wells, walls, paths): (usize, usize, usize) = (
            columns.iter().map(|c| count(c, 10)).sum(),
            columns.iter().map(|c| count(c, 12)).sum(),
            columns.iter().map(|c| count(c, 11)).sum(),
        );
        assert!(wells >= 25 && walls >= 400 && paths >= 60, "well {wells}, walls {walls}, paths {paths}");
        for i in [7, 14, 21, 28] {
            let again = make([i as i32 % 6 - 3, i as i32 / 6 - 3]);
            assert_eq!(count(&again, 12), count(&columns[i], 12), "column {i} made again");
            assert_eq!(count(&again, 11), count(&columns[i], 11));
        }
    }

    #[test]
    fn a_towns_plan_worked_out_with_no_chunk_made_is_the_town_its_chunks_build() {
        let world = Preset::RollingHills.world();
        let town = PlacedStructure {
            placement: Placement {
                spacing: 120.0,
                separation: 60.0,
                chance: 1.0,
                slope: 25.0,
                height: Some([1.0, 400.0]),
                ..Placement::default()
            },
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Town(village()),
            biomes: Vec::new(),
        };
        let structures = WorldStructures { list: vec![town], ..WorldStructures::default() };
        let laid = (1.0, [0.0, 0.0], -4, false);
        let plans = towns_near(&structures, &world, None, laid, [0.0, 0.0], 64.0);
        assert!(!plans.is_empty(), "a town near the middle");
        assert_eq!(towns_near(&structures, &world, None, laid, [0.0, 0.0], 64.0), plans, "the same every time");
        // Every column the first town's buildings stand in, built as the streamer builds them.
        let plan = &plans[0];
        let mut columns: Vec<[i32; 2]> = Vec::new();
        for building in &plan.buildings {
            for at in [building.at, building.door.unwrap_or(building.at)] {
                let column = at.map(|v| v.div_euclid(32));
                if !columns.contains(&column) {
                    columns.push(column);
                }
            }
        }
        let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
        for &column in &columns {
            build(&mut chunks, &structures, &world, None, laid, column, 2, None);
        }
        let block_at = |[x, y, z]: [i32; 3]| {
            let key = [x.div_euclid(32), y.div_euclid(32), z.div_euclid(32)];
            let chunk = chunks.iter().find(|(k, _)| *k == key).map(|(_, chunk)| chunk);
            chunk.map_or(AIR, |chunk| {
                chunk.get(x.rem_euclid(32) as u32, y.rem_euclid(32) as u32, z.rem_euclid(32) as u32)
            })
        };
        // Its middle a well of block 10, its houses walls of block 12, each floor where the plan says.
        let middle = &plan.buildings[0];
        assert_eq!(middle.house, None, "the middle first");
        assert_eq!(middle.at, plan.centre);
        assert_eq!(block_at([middle.at[0], middle.floor, middle.at[1]]), 10, "the well's floor");
        assert!(plan.buildings.len() > 4, "{} buildings", plan.buildings.len());
        for house in &plan.buildings[1..] {
            assert!(house.house.is_some());
            assert_eq!(block_at([house.at[0], house.floor, house.at[1]]), 12, "a floor at {:?}", house.at);
            assert_eq!(block_at([house.at[0], house.floor + 1, house.at[1]]), AIR, "inside, over it");
            // The block outside its door is clear at the floor's level and the one over it.
            let door = house.door.expect("a house has a door");
            let half = house.size[2] as i32 / 2 + 1;
            let out = [door[0] - house.at[0], door[1] - house.at[1]];
            assert_eq!(out[0].abs().max(out[1].abs()), half, "just outside its front wall: {out:?}");
            assert_eq!(block_at([door[0], house.floor + 1, door[1]]), AIR, "the door's way clear at {door:?}");
        }
    }

    #[test]
    fn roads_join_neighbouring_towns_over_the_ground_the_same_from_every_chunk_they_cross() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None, halved: false, flat: None };
        let town = Town { roads: true, road: 13, bridge: 14, ..village() };
        let structure = PlacedStructure {
            placement: Placement {
                spacing: 260.0,
                separation: 120.0,
                chance: 1.0,
                slope: 30.0,
                ..Placement::default()
            },
            ground: GroundFit::Leave,
            sink: 0,
            blocks: StructureBlocks::Town(town.clone()),
            biomes: Vec::new(),
        };
        let fits = |_: &PlacedStructure, _: &Site| true;
        // Every road through a 2 km square, asked for at once and a 64 m box at a time.
        let all = roads_near(&structure, &town, 4, ([-1000.0; 2], [1000.0; 2]), &ground, &fits);
        assert!(all.len() >= 30, "{} roads", all.len());
        for road in &all {
            // Steps two blocks apart, each its road's own, ending near the next town.
            for pair in road.windows(2) {
                let apart = ((pair[0][0] - pair[1][0]).powi(2) + (pair[0][1] - pair[1][1]).powi(2)).sqrt();
                assert!(apart < 2.5, "a step of {apart}");
            }
        }
        for z in -3..3 {
            for x in -3..3 {
                let (low, high) = ([x as f32 * 64.0, z as f32 * 64.0], [(x + 1) as f32 * 64.0, (z + 1) as f32 * 64.0]);
                for road in roads_near(&structure, &town, 4, (low, high), &ground, &fits) {
                    assert!(all.contains(&road), "a road from one box is the same road from all of them");
                }
            }
        }
        // Each road ends at a town: within its gap of a site of the placement.
        let sites = structure.placement.tries_near(4, [-3000.0; 2], [3000.0; 2], 0.0);
        for road in &all {
            let last = road.last().unwrap();
            let near = sites.iter().any(|site| {
                ((site.at[0] - last[0]).powi(2) + (site.at[1] - last[1]).powi(2)).sqrt() <= town.radius[0] * 0.5 + 2.5
            });
            assert!(near, "a road ending at {last:?}, nowhere near a town");
        }
        // Roads keep to the gentler ground: on these hills, most steps climb no more than a block.
        let steps: Vec<f32> = all
            .iter()
            .flat_map(|road| {
                road.windows(2).map(|pair| {
                    (ground.top(pair[0][0].round() as i32, pair[0][1].round() as i32)
                        - ground.top(pair[1][0].round() as i32, pair[1][1].round() as i32))
                    .abs() as f32
                })
            })
            .collect();
        let gentle = steps.iter().filter(|&&climb| climb <= 1.0).count();
        assert!(gentle * 100 >= steps.len() * 95, "{gentle} of {} steps climb a block or less", steps.len());
    }

    #[test]
    fn a_towns_houses_keep_to_their_rules_how_many_how_far_out_and_in_which_biomes() {
        use crate::generate::{BiomeDef, biomes::Biomes};
        let world = Preset::RollingHills.world();
        // Two biomes split where this stretch of the world is half warmer: kind 3 stands only in the warmer.
        let warmths: Vec<f32> = (0..400)
            .map(|i| {
                let (x, z) = ((i % 20) as f32 * 100.0 - 1000.0, (i / 20) as f32 * 100.0 - 1000.0);
                world.climate(x, z, world.height_at(x, z)).0
            })
            .collect();
        let mut sorted = warmths.clone();
        sorted.sort_by(f32::total_cmp);
        let split = sorted[200];
        let cool = BiomeDef { temperature: [-60.0, split], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let warm = BiomeDef { temperature: [split, 60.0], humidity: [0.0, 1.0], ..BiomeDef::default() };
        let biomes = Biomes::new(vec![cool, warm], 15.0, 0.3, world.seed());
        let biomes = WorldBiomes { regions: None, biomes, blocks: Vec::new() };
        let ground =
            Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: Some(&biomes), halved: false, flat: None };
        let base = village();
        let piece = |wall: BlockId| TownPiece {
            blocks: base.houses[1].piece.blocks.iter().map(|&b| if b == AIR { AIR } else { wall }).collect(),
            ..base.houses[1].piece.clone()
        };
        let town = Town {
            houses: vec![
                TownHouseKind::anywhere(piece(12), 3.0),
                // A chapel: once a town at most, within the inner third.
                TownHouseKind { piece: piece(13), weight: 2.0, most: 1, zone: [0.0, 0.33], biomes: Vec::new() },
                // A barn: only out past the middle half.
                TownHouseKind { piece: piece(14), weight: 2.0, most: 0, zone: [0.5, 1.0], biomes: Vec::new() },
                // A warm house: only in the warmer biome.
                TownHouseKind { piece: piece(15), weight: 2.0, most: 0, zone: [0.0, 1.0], biomes: vec![1] },
            ],
            count: [14, 20],
            ..base
        };
        let reach = town.reach();
        let (mut chapels, mut barns, mut warm_houses) = (0, 0, 0);
        for seed in 0..24u32 {
            let centre = [seed as i32 * 83 - 1000, 700 - seed as i32 * 61];
            let (parts, _) = town.layout(seed, centre, &ground);
            let wall = |part: &Part| part.pieces.iter().find(|(at, _)| at[1] == 1).map(|(_, block)| *block);
            let mut chapels_here = 0;
            for part in &parts[1..] {
                let out = (((part.at[0] - centre[0]).pow(2) + (part.at[1] - centre[1]).pow(2)) as f32).sqrt() / reach;
                match wall(part) {
                    Some(13) => {
                        chapels_here += 1;
                        // Its lot is within the inner third; it stands a little further out than its lot.
                        assert!(out <= 0.33 + 12.0 / reach, "seed {seed}: a chapel {out} of the way out");
                    }
                    Some(14) => {
                        barns += 1;
                        assert!(out >= 0.5 - 12.0 / reach, "seed {seed}: a barn {out} of the way out");
                    }
                    Some(15) => {
                        warm_houses += 1;
                    }
                    _ => {}
                }
            }
            assert!(chapels_here <= 1, "seed {seed}: {chapels_here} chapels");
            chapels += chapels_here;
            // A warm house only where it is warm, by its lot along the street, so near its own place.
            for part in parts[1..].iter().filter(|part| wall(part) == Some(15)) {
                let nearby = (-6..=6).flat_map(|dz| (-6..=6).map(move |dx| [part.at[0] + dx, part.at[1] + dz]));
                assert!(
                    nearby.into_iter().any(|cell| ground.biome(cell) == Some(1)),
                    "seed {seed}: a warm house in the cold"
                );
            }
        }
        assert!(
            chapels >= 8 && barns >= 8 && warm_houses >= 8,
            "{chapels} chapels, {barns} barns, {warm_houses} warm houses"
        );
    }

    #[test]
    fn a_structure_keeps_its_distance_from_others_sites_from_any_chunk() {
        let world = Preset::RollingHills.world();
        let marker = |block: BlockId| StructureBlocks::Model { size: [1, 1, 1], blocks: vec![block] };
        let place = |salt: u32, spacing: f32, apart: f32| Placement {
            spacing,
            separation: 16.0,
            chance: 1.0,
            slope: 80.0,
            salt,
            apart,
            ..Placement::default()
        };
        let list = vec![
            PlacedStructure {
                placement: place(1, 110.0, 0.0),
                ground: GroundFit::Leave,
                sink: 0,
                blocks: marker(20),
                biomes: Vec::new(),
            },
            PlacedStructure {
                placement: place(2, 40.0, 70.0),
                ground: GroundFit::Leave,
                sink: 0,
                blocks: marker(21),
                biomes: Vec::new(),
            },
        ];
        let structures = WorldStructures { list, ..WorldStructures::default() };
        let mut found: Vec<(BlockId, [i32; 3])> = Vec::new();
        for cz in -5..5 {
            for cx in -5..5 {
                let mut chunks = Vec::new();
                build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -64, false), [cx, cz], 2, None);
                for (key, chunk) in &chunks {
                    for i in 0..32 * 32 * 32u32 {
                        let block = chunk.get(i % 32, i / 1024, (i / 32) % 32);
                        if block == 20 || block == 21 {
                            found.push((
                                block,
                                [
                                    key[0] * 32 + (i % 32) as i32,
                                    key[1] * 32 + (i / 1024) as i32,
                                    key[2] * 32 + ((i / 32) % 32) as i32,
                                ],
                            ));
                        }
                    }
                }
            }
        }
        let (a, b): (Vec<_>, Vec<_>) = found.iter().partition(|(block, _)| *block == 20);
        assert!(a.len() >= 4 && b.len() >= 4, "{} of the first, {} of the second", a.len(), b.len());
        for (_, first) in &a {
            for (_, second) in &b {
                let apart = (((first[0] - second[0]).pow(2) + (first[2] - second[2]).pow(2)) as f32).sqrt();
                assert!(apart >= 69.0, "{first:?} and {second:?} only {apart} apart");
            }
        }
    }

    #[test]
    fn looking_for_each_structures_sites_only_as_far_as_they_matter_builds_what_looking_everywhere_does() {
        let world = Preset::RollingHills.world();
        let place = |salt: u32, spacing: f32, apart: f32| Placement {
            spacing,
            separation: spacing * 0.4,
            chance: 0.8,
            slope: 80.0,
            salt,
            apart,
            ..Placement::default()
        };
        let placed = |placement: Placement, blocks: StructureBlocks| PlacedStructure {
            placement,
            ground: GroundFit::Foundation,
            sink: 0,
            blocks,
            biomes: Vec::new(),
        };
        // A town, a dungeon reaching far under the ground, and what stands thick about them: the hut and the
        // trees, the trees last, kept clear of everything before them.
        let mut list = vec![
            placed(place(1, 300.0, 0.0), StructureBlocks::Town(village())),
            placed(place(2, 240.0, 60.0), StructureBlocks::Dungeon(crate::blocks::dungeon::tests::temple())),
        ];
        let mut thick = hut_and_trees().list;
        thick[0].placement = place(3, 70.0, 20.0);
        thick[1].placement = place(4, 11.0, 0.0);
        list.extend(thick);
        let structures = WorldStructures { list, ..WorldStructures::default() };
        let at = (1.0, [300.0, -200.0], -64, false);
        let near = looks(&structures, at.0);
        // Each as far as it reaches at the least, the first furthest, the trees no further than their crowns
        // and a little: nothing after them asks for more.
        let reaches: Vec<f32> = structures.list.iter().map(PlacedStructure::reach).collect();
        assert!(near.iter().zip(&reaches).all(|(look, reach)| look > reach), "{near:?} {reaches:?}");
        assert!(near.windows(2).all(|pair| pair[0] >= pair[1]), "{near:?}");
        assert!(near[3] < 16.0, "the trees are looked for {} m off", near[3]);
        let everywhere = {
            let farthest = reaches.iter().copied().fold(0.0f32, f32::max);
            vec![farthest * 2.0 + 64.0 + 60.0; structures.list.len()]
        };
        assert!(near[0] <= everywhere[0] * 1.5 && near[3] * 20.0 < everywhere[3]);
        let built = |area: ([i32; 2], [i32; 2]), looks: &[f32]| {
            let mut sets: Vec<([i32; 3], BlockId)> = Vec::new();
            let generators: &dyn Fn([i32; 2]) -> Option<i32> = &|_| None;
            build_looking(
                &mut |at, block| sets.push((at, block)),
                &structures,
                &world,
                None,
                at,
                area,
                2,
                (looks, generators, None),
            );
            sets
        };
        let mut blocks = 0;
        for column in [[0, 0], [3, -2], [-7, 5], [10, 10], [-12, -3], [6, 14]] {
            let area = (column, [column[0] + 1, column[1] + 1]);
            let sets = built(area, &near);
            assert_eq!(sets, built(area, &everywhere), "chunk column {column:?}");
            blocks += sets.len();
        }
        // An area of columns at once builds what its columns build one by one.
        let area = ([-8, -4], [-4, 0]);
        use std::collections::HashMap;
        let whole: HashMap<[i32; 3], BlockId> = built(area, &near).into_iter().collect();
        let mut by_column: HashMap<[i32; 3], BlockId> = HashMap::new();
        for cz in area.0[1]..area.1[1] {
            for cx in area.0[0]..area.1[0] {
                by_column.extend(built(([cx, cz], [cx + 1, cz + 1]), &near));
            }
        }
        assert!(whole == by_column, "{} blocks at once, {} column by column", whole.len(), by_column.len());
        assert!(blocks > 500 && whole.len() > 500, "{blocks} blocks in six columns, {} in the area", whole.len());
    }

}
