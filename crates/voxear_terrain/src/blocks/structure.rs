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

/// A dungeon: rooms under the ground joined by corridors, reached by a
/// winding stair down from a way in at the surface, on one floor or
/// several, one under the next. It is laid out as dungeons people enjoy
/// are: on each floor a main path of rooms from the stair to its far end,
/// a stair down to the next floor there and the boss's room on the last, some of them great halls, with side
/// branches off it that end in dead ends where the loot is, and a few
/// loops joining rooms that lie near each other but far apart along the
/// paths, so not everything is a walk back. Rooms are of many sizes, each
/// fitted on beside the last one at a time, so every room can be reached.
///
/// Each room is a kind with a role, a structure with a model, chosen by
/// rules as a town's houses are: your own rooms, swapped in by role. Where
/// no kind of a room's role may stand, a plain room of the dungeon's walls
/// is built, as large as that role's rooms are. A room's doors are in the
/// middles of its sides, a block over its floor: the corridors open them.
///
/// ```toml
/// [dungeon]
/// entrance = "Structures/Crypt Door.structure"  # at the surface, over the stair; none for a bare stair
/// count = [10, 16]            # rooms, from and to
/// main_path = 0.5             # the share of them on the main path to the boss
/// halls = 0.3                 # the share of the main path that is great halls
/// corridor = [4, 9]           # corridors' lengths, in blocks, from and to
/// loops = 0.3                 # how many joins between rooms near each other are made, 0 to 1
/// floors = [1, 3]             # floors, one under the next, from and to
/// floor_gap = 14              # blocks from one floor down to the next
/// depth = [12, 20]            # the first floor's blocks under the surface, from and to
/// caves = 48                  # how far a room breaks through to a cave of the world's, in blocks; 0 never
/// wall = "Blocks/Stone Bricks.block"
/// floor = "Blocks/Cobblestone.block"
/// corridor_width = 3
/// corridor_height = 3
/// room_size = [7, 5, 7]       # a plain room's size, blocks along x, up and along z; halls,
/// hall_size = [13, 7, 13]     # loot rooms and the boss's room each have their own
/// loot_size = [5, 4, 5]
/// boss_size = [17, 9, 17]
///
/// [[dungeon.rooms]]
/// structure = "Structures/Throne Room.structure"
/// role = "boss"               # any, hall, loot or boss
///
/// [[dungeon.rooms]]
/// structure = "Structures/Treasury.structure"
/// role = "loot"
/// weight = 1.0
/// most = 2                    # on each floor
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance: Option<String>,
    pub rooms: Vec<DungeonRoom>,
    pub count: [u32; 2],
    pub main_path: f32,
    pub halls: f32,
    pub corridor: [u32; 2],
    pub loops: f32,
    pub floors: [u32; 2],
    pub floor_gap: u32,
    pub depth: [u32; 2],
    pub caves: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
    pub corridor_width: u32,
    pub corridor_height: u32,
    pub room_size: [u32; 3],
    pub hall_size: [u32; 3],
    pub loot_size: [u32; 3],
    pub boss_size: [u32; 3],
}

impl Default for DungeonDef {
    fn default() -> Self {
        Self {
            entrance: None,
            rooms: Vec::new(),
            count: [10, 16],
            main_path: 0.5,
            halls: 0.3,
            corridor: [4, 9],
            loops: 0.3,
            floors: [1, 3],
            floor_gap: 14,
            depth: [12, 20],
            caves: 48,
            wall: None,
            floor: None,
            corridor_width: 3,
            corridor_height: 3,
            room_size: [7, 5, 7],
            hall_size: [13, 7, 13],
            loot_size: [5, 4, 5],
            boss_size: [17, 9, 17],
        }
    }
}

/// What a room of a dungeon is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomRole {
    /// Any room on the way: on the main path or a side branch.
    #[default]
    Any,
    /// A great hall on the main path.
    Hall,
    /// A dead end at the end of a side branch, where the loot is.
    Loot,
    /// The room at the end of the main path.
    Boss,
}

impl RoomRole {
    pub const ALL: [RoomRole; 4] = [RoomRole::Any, RoomRole::Hall, RoomRole::Loot, RoomRole::Boss];

    pub fn name(self) -> &'static str {
        match self {
            RoomRole::Any => "Any",
            RoomRole::Hall => "Hall",
            RoomRole::Loot => "Loot",
            RoomRole::Boss => "Boss",
        }
    }
}

/// One kind of room of a dungeon, by reference to a `.structure` with a
/// model: its role, how often it is chosen against the others of its role,
/// at most how many a floor has (0 for no limit), and how far from the
/// floor's stair it stands, as a share of the way to its furthest room.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonRoom {
    pub structure: String,
    pub role: RoomRole,
    pub weight: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub most: u32,
    pub zone: [f32; 2],
}

impl Default for DungeonRoom {
    fn default() -> Self {
        Self { structure: String::new(), role: RoomRole::Any, weight: 1.0, most: 0, zone: [0.0, 1.0] }
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
    Plant { plant: PlantDef, wood: BlockId, leaves: BlockId, vine: BlockId },
    Town(Town),
    Dungeon(Dungeon),
}

/// A dungeon as a world builds it: its way in, its kinds of room with their
/// roles (a kind's zone the share of the way from the stair to the furthest
/// room), and the rest of its settings as its file has them, its blocks by
/// the world's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Dungeon {
    pub entrance: Option<TownPiece>,
    pub rooms: Vec<(RoomRole, TownHouseKind)>,
    pub count: [u32; 2],
    pub main_path: f32,
    pub halls: f32,
    pub corridor: [u32; 2],
    pub loops: f32,
    pub floors: [u32; 2],
    pub floor_gap: u32,
    pub depth: [u32; 2],
    /// How far one of its rooms breaks through to a cave of the world's, in
    /// blocks: the nearest within this of any room. None at 0.
    pub caves: u32,
    pub wall: BlockId,
    pub floor: BlockId,
    pub corridor_width: u32,
    pub corridor_height: u32,
    pub room_size: [u32; 3],
    pub hall_size: [u32; 3],
    pub loot_size: [u32; 3],
    pub boss_size: [u32; 3],
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
            StructureBlocks::Model { size, blocks } => model_pieces(*size, blocks),
            StructureBlocks::Plant { plant, wood, leaves, vine } => plant
                .grow(site.seed)
                .blocks
                .into_iter()
                .map(|(at, part)| match part {
                    PlantPart::Leaf => (at, *leaves),
                    PlantPart::Vine => (at, *vine),
                    _ => (at, *wood),
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
            StructureBlocks::Dungeon(dungeon) => dungeon.entrance.as_ref().map_or(3.5, TownPiece::radius),
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
fn model_pieces(size: [u32; 3], blocks: &[BlockId]) -> Vec<Piece> {
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
            parts.push(Part { at: centre, pieces, ground: middle.ground, sink: middle.sink });
        }
        let middle_radius = self.middle.as_ref().map_or(1.5, TownPiece::radius);
        let streets = self.streets(&mut draws, centre, middle_radius, ground);
        let mut paths: Vec<[i32; 2]> = Vec::new();
        let width = self.path_width.max(1) as f32;
        for street in &streets {
            for &(point, _) in &street.points {
                lay(&mut paths, point, width);
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
            stubs.extend(stub);
            parts.push(Part { at, pieces, ground: house.ground, sink: house.sink });
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
    /// chunk it crosses finds the same road.
    fn road_between(&self, seed: u32, from: [i32; 2], to: [i32; 2], ground: &Ground) -> Vec<[f32; 2]> {
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

/// A dungeon's rooms and corridors as laid out: each room's middle, in
/// blocks along x and z from the stair, its kind (none for a plain room),
/// how far it reaches along x, up and along z, and how many corridors from
/// the stair; what each room is for; and each corridor, the rooms it joins.
struct DungeonPlan {
    rooms: Vec<([i32; 2], Option<usize>, [i32; 3], u32)>,
    roles: Vec<Spot>,
    corridors: Vec<(usize, usize)>,
}

/// What a room of a dungeon is as laid out: the stair's foot, a room on
/// the main path or a great hall on it, the boss's room at its end, a room
/// on a side branch, or loot at a branch's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spot {
    Stair,
    Path,
    Hall,
    Boss,
    /// The end of a floor's main path, but the last's: a stair down.
    Down,
    Branch,
    Loot,
}

impl Spot {
    /// Whether a kind of room with `role` may stand here.
    fn takes(self, role: RoomRole) -> bool {
        match self {
            Spot::Stair | Spot::Down => false,
            Spot::Path | Spot::Branch => role == RoomRole::Any,
            // A great hall only: where there is no kind of hall, a plain one as large as halls are.
            Spot::Hall => role == RoomRole::Hall,
            Spot::Boss => role == RoomRole::Boss,
            Spot::Loot => role == RoomRole::Loot,
        }
    }
}

impl Dungeon {
    /// The largest any room reaches from its middle along the ground, in
    /// blocks.
    fn room_half(&self) -> i32 {
        let kinds = self.rooms.iter().map(|(_, kind)| kind.piece.size[0].max(kind.piece.size[2]));
        let plain = [self.room_size, self.hall_size, self.loot_size, self.boss_size].map(|size| size[0].max(size[2]));
        (kinds.chain(plain).max().unwrap_or(0) / 2) as i32
    }

    /// How far its rooms reach from its way in, in blocks: every room in a
    /// line, each as large as the largest, a longest corridor between.
    fn reach(&self) -> f32 {
        let rooms = self.count[0].max(self.count[1]) as f32 + 1.0;
        let step = (self.room_half() * 2) as f32 + self.corridor[0].max(self.corridor[1]) as f32 + 2.0;
        // Rooms are placed outwards along a main path and branches, so few lie in a line; half of them is far.
        // Each floor starts where the one over it ends.
        // And as far again as a way broken through to a cave runs.
        (rooms * 0.5 + 2.0) * step * self.floors[0].max(self.floors[1]).max(1) as f32 + self.caves as f32
    }

    /// How large a plain room of `place` is, a little more or less by `draws`.
    fn plain_size(&self, place: Spot, draws: &mut Draws) -> [u32; 3] {
        let size = match place {
            Spot::Stair => return [5, 1, 5],
            Spot::Hall => self.hall_size,
            Spot::Boss => self.boss_size,
            Spot::Loot => self.loot_size,
            Spot::Path | Spot::Branch | Spot::Down => self.room_size,
        };
        // Odd along the ground, so a room has a middle block, and two blocks more or less now and then.
        let vary = |v: u32, draws: &mut Draws| {
            let v = (v as i32 + [0, 0, 2, -2][(draws.next() % 4) as usize]).max(3);
            (v | 1) as u32
        };
        [vary(size[0], draws), size[1].max(3), vary(size[2], draws)]
    }

    /// Its rooms laid out: a main path out from the stair to the boss's
    /// room, some of it halls, then side branches off it ending in loot, each
    /// room fitted on beside the one it leaves, clear of the rest and of
    /// their corridors; then loops between rooms that lie near each other but
    /// are at least three corridors apart, never to the boss's room or a
    /// loot room; then each room's kind, nearest the stair first.
    fn plan(&self, draws: &mut Draws, bottom: bool) -> DungeonPlan {
        let [least, most] = [self.count[0].min(self.count[1]), self.count[0].max(self.count[1])];
        let wanted = (least + draws.next() % (most - least + 1).max(1)).max(2) as usize;
        let main = ((wanted as f32 * self.main_path.clamp(0.1, 1.0)).round() as usize).clamp(2, wanted);
        let ways = [[1, 0], [0, 1], [-1, 0], [0, -1]];
        let mut rooms: Vec<([i32; 2], Option<usize>, [i32; 3], u32)> = vec![([0, 0], None, [2, 1, 2], 0)];
        let mut roles = vec![Spot::Stair];
        let mut corridors: Vec<(usize, usize)> = Vec::new();
        let mut kinds_built = vec![0u32; self.rooms.len()];
        // A kind for a room of `place`, by weight among those of its role not yet as many as they may be.
        let pick = |place: Spot, built: &[u32], draws: &mut Draws| -> Option<usize> {
            let may = |at: usize| {
                let (role, kind) = &self.rooms[at];
                place.takes(*role) && (kind.most == 0 || built[at] < kind.most)
            };
            let total: f32 =
                (0..self.rooms.len()).filter(|&at| may(at)).map(|at| self.rooms[at].1.weight.max(0.0)).sum();
            if total <= 0.0 {
                return None;
            }
            let mut left = draws.unit() * total;
            (0..self.rooms.len()).filter(|&at| may(at)).find(|&at| {
                left -= self.rooms[at].1.weight.max(0.0);
                left < 0.0
            })
        };
        // Fits a room of `place` on beside room `from`, heading `way` or turning from it: where it lands, its
        // kind and size, or none where nothing fits.
        let corridor = [self.corridor[0].min(self.corridor[1]) as i32, self.corridor[0].max(self.corridor[1]) as i32];
        let fit = |rooms: &Vec<([i32; 2], Option<usize>, [i32; 3], u32)>,
                   from: usize,
                   way: [i32; 2],
                   place: Spot,
                   built: &[u32],
                   draws: &mut Draws|
         -> Option<([i32; 2], Option<usize>, [i32; 3], [i32; 2])> {
            let kind = pick(place, built, draws);
            let size = match kind {
                Some(at) => self.rooms[at].1.piece.size,
                None => self.plain_size(place, draws),
            };
            let half = [size[0] as i32 / 2, size[1] as i32, size[2] as i32 / 2];
            let (start, start_half) = (rooms[from].0, rooms[from].2);
            // Ahead or a turn to either side; never back the way it came.
            let turns = [way, [-way[1], way[0]], [way[1], -way[0]]];
            // A third of the time a turn is tried first, so paths wind rather than run on in a line.
            let turn_first = draws.unit() < 0.35;
            for attempt in 0..6 {
                let ahead = (attempt < 2) != turn_first;
                let dir = turns[if ahead { 0 } else { 1 + (draws.next() % 2) as usize }];
                let length = corridor[0] + (draws.next() % (corridor[1] - corridor[0] + 1).max(1) as u32) as i32;
                let along = if dir[0] != 0 { start_half[0] + half[0] } else { start_half[2] + half[2] } + length + 1;
                // A little to one side, as far as its door still meets the corridor.
                let room_side = if dir[0] != 0 { half[2].min(start_half[2]) } else { half[0].min(start_half[0]) };
                let shift = (draws.next() % (room_side as u32 + 1)) as i32 - room_side / 2;
                let middle = [
                    start[0] + dir[0] * along + if dir[0] == 0 { shift } else { 0 },
                    start[1] + dir[1] * along + if dir[1] == 0 { shift } else { 0 },
                ];
                // Clear of every room, a corridor's width and a wall between, and its corridor of every other room.
                let clear = rooms.iter().enumerate().all(|(at, (other, _, other_half, _))| {
                    let gap = 4;
                    let apart = (other[0] - middle[0]).abs() > other_half[0] + half[0] + gap
                        || (other[1] - middle[1]).abs() > other_half[2] + half[2] + gap;
                    let crossed = at != from && crosses(start, middle, *other, *other_half);
                    apart && !crossed
                });
                if clear {
                    return Some((middle, kind, half, dir));
                }
            }
            None
        };
        // The main path, out from the stair, heading one way and turning now and then.
        let mut way = ways[(draws.next() % 4) as usize];
        let mut last = 0;
        for step in 0..main {
            let end = step + 1 == main;
            let place = if end && bottom {
                Spot::Boss
            } else if end {
                Spot::Down
            } else if step > 0 && draws.unit() < self.halls.clamp(0.0, 1.0) {
                Spot::Hall
            } else {
                Spot::Path
            };
            let Some((middle, kind, half, dir)) = fit(&rooms, last, way, place, &kinds_built, draws) else { break };
            if let Some(at) = kind {
                kinds_built[at] += 1;
            }
            rooms.push((middle, kind, half, 0));
            roles.push(place);
            corridors.push((last, rooms.len() - 1));
            last = rooms.len() - 1;
            way = dir;
        }
        // Whatever ended the main path, its last room is the boss's, or the stair down from a floor but the last.
        if let Some(role) = roles.last_mut().filter(|role| **role != Spot::Stair) {
            *role = if bottom { Spot::Boss } else { Spot::Down };
        }
        // Side branches off the main path, one to three rooms, the last of each a loot room.
        let mut tries = 0;
        while rooms.len() - 1 < wanted && tries < wanted * 8 {
            tries += 1;
            let path: Vec<usize> =
                (1..rooms.len()).filter(|&at| matches!(roles[at], Spot::Path | Spot::Hall)).collect();
            if path.is_empty() {
                break;
            }
            let mut from = path[(draws.next() as usize) % path.len()];
            let length = (1 + draws.next() % 3).min((wanted + 1 - rooms.len()) as u32).max(1);
            let mut way = ways[(draws.next() % 4) as usize];
            for step in 0..length {
                let place = if step + 1 == length { Spot::Loot } else { Spot::Branch };
                let Some((middle, kind, half, dir)) = fit(&rooms, from, way, place, &kinds_built, draws) else {
                    // A branch that cannot go on ends where it is: its last room is the loot.
                    if step > 0
                        && let Some(role) = roles.last_mut()
                    {
                        *role = Spot::Loot;
                    }
                    break;
                };
                if let Some(at) = kind {
                    kinds_built[at] += 1;
                }
                rooms.push((middle, kind, half, 0));
                roles.push(place);
                corridors.push((from, rooms.len() - 1));
                from = rooms.len() - 1;
                way = dir;
            }
        }
        // How many corridors each room is from the stair.
        let away_from = |corridors: &[(usize, usize)], start: usize| {
            let mut away = vec![u32::MAX; rooms.len()];
            away[start] = 0;
            let mut next = vec![start];
            while let Some(room) = next.pop() {
                for &(a, b) in corridors {
                    let other = if a == room {
                        b
                    } else if b == room {
                        a
                    } else {
                        continue;
                    };
                    if away[other] > away[room] + 1 {
                        away[other] = away[room] + 1;
                        next.push(other);
                    }
                }
            }
            away
        };
        // Loops: rooms near each other, both on the way (not the stair, the boss's room or loot), three corridors
        // or more apart, joined by chance where their corridor crosses no other room.
        let open = |at: usize| matches!(roles[at], Spot::Path | Spot::Hall | Spot::Branch);
        for a in 1..rooms.len() {
            for b in a + 1..rooms.len() {
                if !open(a) || !open(b) {
                    continue;
                }
                let (pa, pb) = (rooms[a].0, rooms[b].0);
                let gap = (pa[0] - pb[0]).abs() - rooms[a].2[0] - rooms[b].2[0];
                let gap = gap.max((pa[1] - pb[1]).abs() - rooms[a].2[2] - rooms[b].2[2]);
                if gap > corridor[1] * 2 + 8 || away_from(&corridors, a)[b] < 3 {
                    continue;
                }
                let crossed =
                    (0..rooms.len()).filter(|&c| c != a && c != b).any(|c| crosses(pa, pb, rooms[c].0, rooms[c].2));
                if !crossed && draws.unit() < self.loops.clamp(0.0, 1.0) {
                    corridors.push((a, b));
                }
            }
        }
        let away = away_from(&corridors, 0);
        for (room, distance) in rooms.iter_mut().zip(&away) {
            room.3 = *distance;
        }
        // Kinds' zones: a kind out of its zone gives way to a plain room of its role.
        let furthest = away.iter().copied().filter(|&d| d != u32::MAX).max().unwrap_or(1).max(1) as f32;
        for room in rooms.iter_mut() {
            if let Some(at) = room.1 {
                let zone = self.rooms[at].1.zone;
                let share = room.3 as f32 / furthest;
                if share < zone[0].min(zone[1]) - 1e-4 || share > zone[0].max(zone[1]) + 1e-4 {
                    room.1 = None;
                }
            }
        }
        DungeonPlan { rooms, roles, corridors }
    }

    /// Its blocks at a site whose middle is block `site` on `ground`, in the
    /// order they are put: its way in at the surface, the stair winding down
    /// to its first floor `depth` blocks under the site's ground, and each
    /// floor's rooms and corridors, a stair winding down from the end of each
    /// floor's main path to the next floor `floor_gap` blocks under it,
    /// which starts there.
    fn blocks(&self, seed: u32, site: [i32; 2], ground: &Ground, under: BlockId) -> Vec<([i32; 3], BlockId)> {
        let mut draws = Draws(seed ^ 0x0D06_E0A5);
        let surface = ground.top(site[0], site[1]);
        let [least, most] = [self.depth[0].min(self.depth[1]), self.depth[0].max(self.depth[1])];
        let mut level = surface - (least + draws.next() % (most - least + 1).max(1)) as i32;
        let [fewest, floors] = [self.floors[0].min(self.floors[1]).max(1), self.floors[0].max(self.floors[1]).max(1)];
        let floors = fewest + draws.next() % (floors - fewest + 1);
        let (wall, floor_block) =
            (if self.wall == AIR { under } else { self.wall }, if self.floor == AIR { under } else { self.floor });
        let mut out: Vec<([i32; 3], BlockId)> = Vec::new();
        // Its way in, on the ground over the stair.
        if let Some(entrance) = &self.entrance {
            for ([x, y, z], block) in model_pieces(entrance.size, &entrance.blocks) {
                out.push(([site[0] + x, surface + 1 - entrance.sink + y, site[1] + z], block));
            }
        }
        // The stair down from over the ground, its walls stopping at the ground.
        stair(&mut out, site, (level, surface + 3, surface), (wall, floor_block));
        let mut origin = site;
        // Each room's middle, over its floor, and how far it reaches either way, on every floor.
        let mut rooms: Vec<([i32; 3], [i32; 3])> = Vec::new();
        for floor in 0..floors {
            let last = floor + 1 == floors;
            let mut floor_draws = Draws(seed ^ 0x0D06_E0A5 ^ (floor + 1).wrapping_mul(0x9E37_79B9));
            let plan = self.plan(&mut floor_draws, last);
            self.floor_blocks(&mut out, &plan, origin, level, (wall, floor_block));
            for (&(middle, _, half, _), role) in plan.rooms.iter().zip(&plan.roles) {
                if *role != Spot::Stair {
                    rooms.push(([origin[0] + middle[0], level + 1, origin[1] + middle[1]], half));
                }
            }
            if last {
                break;
            }
            // A stair down from the middle of the room ending the main path, a hole in its floor, to the next floor.
            let Some(down) = (0..plan.roles.len()).find(|&at| plan.roles[at] == Spot::Down) else { break };
            let middle = plan.rooms[down].0;
            // Far enough down that the tallest room under clears the floor over it.
            let tallest = [self.room_size, self.hall_size, self.loot_size, self.boss_size]
                .iter()
                .map(|size| size[1])
                .chain(self.rooms.iter().map(|(_, kind)| kind.piece.size[1]))
                .max()
                .unwrap_or(0);
            let next = level - self.floor_gap.max(tallest + 3) as i32;
            origin = [origin[0] + middle[0], origin[1] + middle[1]];
            stair(&mut out, origin, (next, level, level), (wall, floor_block));
            level = next;
        }
        out.extend(self.into_caves(seed, &rooms, ground));
        out
    }

    /// A way broken through from the room of `rooms` nearest a cave of the
    /// world's generator to that cave's nearest room, where one lies within
    /// `caves` blocks of it and the way climbs or falls no steeper than it
    /// runs along: a rough tunnel of air from the room's middle, over its
    /// floor, to the cave room's floor, so the dungeon and the caves are
    /// one. Nothing where none does.
    fn into_caves(&self, seed: u32, rooms: &[([i32; 3], [i32; 3])], ground: &Ground) -> Vec<([i32; 3], BlockId)> {
        let (Some(caves), Some(&(first, _))) = (ground.world.caves().filter(|_| self.caves > 0), rooms.first())
        else {
            return Vec::new();
        };
        let (size, [px, pz]) = (ground.size, ground.position);
        let metres =
            |[x, y, z]: [i32; 3]| [px + (x as f32 + 0.5) * size, y as f32 * size, pz + (z as f32 + 0.5) * size];
        let block = |[x, y, z]: [f32; 3]| {
            [((x - px) / size).floor() as i32, (y / size).floor() as i32, ((z - pz) / size).floor() as i32]
        };
        let near = caves.rooms_near(metres(first), (self.caves as f32 + self.reach()) * size);
        // The room and the cave room nearest each other, their way no steeper than it runs.
        let mut best: Option<(f32, [i32; 3], [i32; 3])> = None;
        for (middle, half) in rooms {
            for cave in &near {
                let floor = block(cave.floor);
                let end = [floor[0], floor[1] + 1, floor[2]];
                let (dx, dy, dz) = (end[0] - middle[0], end[1] - middle[1], end[2] - middle[2]);
                let along = ((dx * dx + dz * dz) as f32).sqrt();
                let past = along - half[0].max(half[2]) as f32;
                if past > self.caves as f32 || dy.abs() as f32 > along {
                    continue;
                }
                if best.is_none_or(|(least, ..)| past < least) {
                    best = Some((past, *middle, end));
                }
            }
        }
        let Some((_, from, to)) = best else { return Vec::new() };
        // Cut along the way, two blocks either side and four high, wider here and there, rounded at its corners.
        let mut draws = Draws(seed ^ 0xCA7E_0A11);
        let steps = (0..3).map(|axis| (from[axis] - to[axis]).abs()).max().unwrap_or(0).max(1) * 2;
        let mut cut = std::collections::BTreeSet::new();
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let at = |axis: usize| (from[axis] as f32 + (to[axis] - from[axis]) as f32 * t).round() as i32;
            let wide = 2 + i32::from(draws.next() % 4 == 0);
            for dz in -wide..=wide {
                for dx in -wide..=wide {
                    if dx.abs() + dz.abs() > wide + 1 {
                        continue;
                    }
                    for dy in 0..=3 {
                        cut.insert([at(0) + dx, at(1) + dy, at(2) + dz]);
                    }
                }
            }
        }
        cut.into_iter().map(|at| (at, AIR)).collect()
    }

    /// One floor's rooms and corridors, its stair's foot at block `origin`
    /// and its floor at block `level`: plain rooms of its walls and floor and
    /// the rest their models, ground cleared out of each first; then the
    /// corridors from one room's middle to the next, along x and then z, cut
    /// through the ground, floored and lined with walls outside the rooms,
    /// opening a room's wall where they reach it, a block over its floor.
    fn floor_blocks(
        &self,
        out: &mut Vec<([i32; 3], BlockId)>,
        plan: &DungeonPlan,
        origin: [i32; 2],
        level: i32,
        (wall, floor_block): (BlockId, BlockId),
    ) {
        let floor = level;
        let height = self.corridor_height.max(2) as i32;
        let at = |offset: [i32; 2]| [origin[0] + offset[0], origin[1] + offset[1]];
        let mut boxes: Vec<([i32; 2], [i32; 3])> = Vec::new();
        let rooms = plan.rooms.iter().zip(&plan.roles).filter(|(_, role)| **role != Spot::Stair);
        for (&(middle, kind, half, _), role) in rooms {
            let middle = at(middle);
            boxes.push((middle, half));
            let tall = half[1];
            for y in floor..floor + tall {
                for dz in -half[2]..=half[2] {
                    for dx in -half[0]..=half[0] {
                        // The stair down's hole is left to the stair.
                        if *role == Spot::Down && dx.abs() <= 2 && dz.abs() <= 2 && y == floor {
                            continue;
                        }
                        let edge = dx.abs() == half[0] || dz.abs() == half[2] || y == floor || y == floor + tall - 1;
                        let block = match (kind, edge) {
                            (None, true) if y == floor => floor_block,
                            (None, true) => wall,
                            _ => AIR,
                        };
                        out.push(([middle[0] + dx, y, middle[1] + dz], block));
                    }
                }
            }
            if let Some(kind) = kind {
                let piece = &self.rooms[kind].1.piece;
                for ([x, y, z], block) in model_pieces(piece.size, &piece.blocks) {
                    out.push(([middle[0] + x, floor + y, middle[1] + z], block));
                }
            }
        }
        let width = self.corridor_width.max(1) as i32;
        let (from_w, to_w) = (-(width - 1) / 2, width / 2);
        let inside = |cell: [i32; 2]| {
            boxes
                .iter()
                .any(|(middle, half)| (cell[0] - middle[0]).abs() < half[0] && (cell[1] - middle[1]).abs() < half[2])
        };
        let in_wall = |cell: [i32; 2]| {
            boxes
                .iter()
                .any(|(middle, half)| (cell[0] - middle[0]).abs() <= half[0] && (cell[1] - middle[1]).abs() <= half[2])
        };
        let mut cut: Vec<[i32; 2]> = Vec::new();
        for &(a, b) in &plan.corridors {
            let (a, b) = (at(plan.rooms[a].0), at(plan.rooms[b].0));
            let mut line = Vec::new();
            let (mut x, mut z) = (a[0], a[1]);
            line.push([x, z]);
            while x != b[0] {
                x += (b[0] - x).signum();
                line.push([x, z]);
            }
            while z != b[1] {
                z += (b[1] - z).signum();
                line.push([x, z]);
            }
            for [x, z] in line {
                for d in from_w..=to_w {
                    for cell in [[x + d, z], [x, z + d]] {
                        if !cut.contains(&cell) {
                            cut.push(cell);
                        }
                    }
                }
            }
        }
        // Not inside a room, nor inside the stair but for its doorway, and so not through a room's things.
        let stair_at = |cell: [i32; 2]| (cell[0] - origin[0]).abs() <= 1 && (cell[1] - origin[1]).abs() <= 1;
        cut.retain(|&cell| !inside(cell) && !stair_at(cell));
        for &cell in &cut {
            for y in floor + 1..floor + 1 + height {
                out.push(([cell[0], y, cell[1]], AIR));
            }
            out.push(([cell[0], floor, cell[1]], floor_block));
            if !in_wall(cell) {
                out.push(([cell[0], floor + 1 + height, cell[1]], wall));
            }
        }
        for &cell in &cut {
            for beside in [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, -1], [1, -1], [-1, 1]] {
                let next = [cell[0] + beside[0], cell[1] + beside[1]];
                let by_stair = (next[0] - origin[0]).abs() <= 2 && (next[1] - origin[1]).abs() <= 2;
                if cut.contains(&next) || in_wall(next) || by_stair {
                    continue;
                }
                for y in floor..=floor + 1 + height {
                    out.push(([next[0], y, next[1]], wall));
                }
            }
        }
    }
}

/// A stair at block `at`: a shaft five blocks a side lined with walls, a
/// pillar in its middle, steps winding round it a block up for every block
/// along, from block `bottom` up to `top`, its walls and pillar stopping at
/// `walls_to`, a floor all round at its foot.
fn stair(
    out: &mut Vec<([i32; 3], BlockId)>,
    at: [i32; 2],
    (bottom, top, walls_to): (i32, i32, i32),
    (wall, floor): (BlockId, BlockId),
) {
    let ring = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]];
    for y in bottom..=top {
        for dz in -2..=2i32 {
            for dx in -2..=2i32 {
                let edge = dx.abs() == 2 || dz.abs() == 2;
                let block = if edge || (dx == 0 && dz == 0) {
                    if y > walls_to { AIR } else { wall }
                } else {
                    let step = ring.iter().position(|cell| *cell == [dx, dz]).unwrap_or(0) as i32;
                    if y == bottom || (y - bottom).rem_euclid(8) == step { floor } else { AIR }
                };
                out.push(([at[0] + dx, y, at[1] + dz], block));
            }
        }
    }
}

/// Whether a corridor from `a` to `b`, along x and then along z, passes
/// through the room at `middle` reaching `half` along x, up and along z,
/// and a block round it.
fn crosses(a: [i32; 2], b: [i32; 2], middle: [i32; 2], half: [i32; 3]) -> bool {
    let near = |[x, z]: [i32; 2]| (x - middle[0]).abs() <= half[0] + 2 && (z - middle[1]).abs() <= half[2] + 2;
    let (mut x, mut z) = (a[0], a[1]);
    // Its ends are in the rooms it joins; the rest is what is looked at.
    while x != b[0] {
        x += (b[0] - x).signum();
        if near([x, z]) {
            return true;
        }
    }
    while z != b[1] {
        z += (b[1] - z).signum();
        if near([x, z]) && [x, z] != b {
            return true;
        }
    }
    false
}

/// A street as grown: each step's place, in blocks, and the way it heads
/// there.
struct Street {
    points: Vec<([f32; 2], [f32; 2])>,
}

/// Lays a street `width` blocks wide round `point` into `cells`.
fn lay(cells: &mut Vec<[i32; 2]>, point: [f32; 2], width: f32) {
    let reach = width * 0.5;
    let (from, to) = ((point[0] - reach).round() as i32, (point[0] + reach).round() as i32);
    let (near, far) = ((point[1] - reach).round() as i32, (point[1] + reach).round() as i32);
    for z in near..=far {
        for x in from..=to {
            let (dx, dz) = (x as f32 - point[0], z as f32 - point[1]);
            if dx * dx + dz * dz <= reach * reach + 0.5 && !cells.contains(&[x, z]) {
                cells.push([x, z]);
            }
        }
    }
}

/// The ground under a place, for building: the generator's height there,
/// in metres in its world.
struct Ground<'a> {
    world: &'a WorldGen,
    size: f32,
    position: [f32; 2],
    biomes: Option<&'a Biomes>,
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

    /// Which of the world's biomes column `(x, z)` is; none where it lists
    /// none.
    fn biome(&self, [x, z]: [i32; 2]) -> Option<usize> {
        let biomes = self.biomes?;
        let (px, pz) =
            (self.position[0] + (x as f32 + 0.5) * self.size, self.position[1] + (z as f32 + 0.5) * self.size);
        let height = self.world.height_at(px, pz);
        let (temperature, humidity) = self.world.climate(px, pz, height);
        let sunk = sunk_at(self.world, biomes, [px, pz], height);
        biomes.at(px, pz, Place { temperature, humidity, above_sea: height - self.world.sea_level(), sunk })
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

/// How far ground `height` metres high at `at` is sunk under the land round
/// about, where any of `biomes` asks.
fn sunk_at(world: &WorldGen, biomes: &Biomes, at: [f32; 2], height: f32) -> f32 {
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
    biomes: Option<&Biomes>,
    at: (f32, [f32; 2], i32),
    [cx, cz]: [i32; 2],
    under: BlockId,
) {
    let mut set = |block_at: [i32; 3], block: BlockId| put_block(chunks, [cx, cz], at.2, block_at, block);
    build_over(&mut set, structures, world, biomes, at, ([cx, cz], [cx + 1, cz + 1]), under);
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
    biomes: Option<&Biomes>,
    at: (f32, [f32; 2], i32),
    area: ([i32; 2], [i32; 2]),
    under: BlockId,
) {
    build_looking(set, structures, world, biomes, at, area, under, &looks(structures, at.0));
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
/// the columns, by its place in the list.
#[allow(clippy::too_many_arguments)]
fn build_looking(
    set: &mut dyn FnMut([i32; 3], BlockId),
    structures: &WorldStructures,
    world: &WorldGen,
    biomes: Option<&Biomes>,
    (size, position, bottom): (f32, [f32; 2], i32),
    (first, end): ([i32; 2], [i32; 2]),
    under: BlockId,
    looks: &[f32],
) {
    let side = CHUNK as i32;
    let ground = Ground { world, size, position, biomes };
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
    let mut kept: Vec<Taken> = Vec::new();
    // Whether the ground at a site fits a structure's placement.
    let fits = |structure: &PlacedStructure, site: &Site| {
        let height = world.height_at(site.at[0], site.at[1]);
        let (east, north) =
            (world.height_at(site.at[0] + size, site.at[1]), world.height_at(site.at[0], site.at[1] + size));
        let slope = ((east - height) * (east - height) + (north - height) * (north - height)).sqrt() / size;
        let above_sea = height - world.sea_level();
        let biome = biomes.and_then(|biomes| {
            let (temperature, humidity) = world.climate(site.at[0], site.at[1], height);
            let sunk = sunk_at(world, biomes, site.at, height);
            biomes.at(site.at[0], site.at[1], Place { temperature, humidity, above_sea, sunk })
        });
        structure.placement.fits(SiteGround { above_sea, slope, biome }, |b| structure.biomes.contains(&b))
    };
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
            let mut cells: Vec<[i32; 2]> = Vec::new();
            for road in &roads {
                for &point in road {
                    lay(&mut cells, point, town.road_width.max(1) as f32);
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
        for site in structure.placement.tries_near(world.seed(), low, high, looks[which]) {
            let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
            if !fits(structure, &site) {
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
            if kept
                .iter()
                .filter(|taken| taken.which != which)
                .any(|taken| taken.meets(block, footprint, reach) || too_near(taken))
            {
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
                for ([x, y, z], block) in dungeon.blocks(site.seed, block, &ground, under) {
                    put(x, y, z, block);
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
                    vec![Part { at: block, pieces, ground: structure.ground, sink: structure.sink }]
                }
            };
            for part in parts {
                let foot = ground.top(part.at[0], part.at[1]);
                build_part(&mut put, &ground, &part, foot, under);
            }
        }
    }
}

/// Puts `block` at block `at` of chunk column `[cx, cz]`'s chunks, a chunk
/// made where there is none, and nothing below block `bottom`.
fn put_block(
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
) -> Vec<Vec<[f32; 2]>> {
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

impl Taken {
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
            for y in (top + 1).min(floor)..floor {
                put(x, y, z, under);
            }
            if part.ground == GroundFit::Flatten {
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
            blocks: StructureBlocks::Plant { plant: PlantDef::default(), wood: 7, leaves: 8, vine: 0 },
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
            let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
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
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
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
        let structures = WorldStructures { list: vec![town] };
        let make = |[cx, cz]: [i32; 2]| {
            let mut chunks: Vec<([i32; 3], Chunk)> = Vec::new();
            build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -4), [cx, cz], 2);
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
    fn roads_join_neighbouring_towns_over_the_ground_the_same_from_every_chunk_they_cross() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
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
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: Some(&biomes) };
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
        let structures = WorldStructures { list };
        let mut found: Vec<(BlockId, [i32; 3])> = Vec::new();
        for cz in -5..5 {
            for cx in -5..5 {
                let mut chunks = Vec::new();
                build(&mut chunks, &structures, &world, None, (1.0, [0.0, 0.0], -64), [cx, cz], 2);
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
            placed(place(2, 240.0, 60.0), StructureBlocks::Dungeon(crypt())),
        ];
        let mut thick = hut_and_trees().list;
        thick[0].placement = place(3, 70.0, 20.0);
        thick[1].placement = place(4, 11.0, 0.0);
        list.extend(thick);
        let structures = WorldStructures { list };
        let at = (1.0, [300.0, -200.0], -64);
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
            build_looking(&mut |at, block| sets.push((at, block)), &structures, &world, None, at, area, 2, looks);
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

    /// A dungeon of walls 30 and floors 31, with a throne room of block 33 for its boss and treasuries of block 32,
    /// two at most, for its loot.
    fn crypt() -> Dungeon {
        let room = |size: [u32; 3], block: BlockId| TownPiece {
            size,
            blocks: vec![block; (size[0] * size[1] * size[2]) as usize],
            ground: GroundFit::Leave,
            sink: 0,
        };
        let kind = |piece: TownPiece, most: u32| TownHouseKind {
            piece,
            weight: 1.0,
            most,
            zone: [0.0, 1.0],
            biomes: Vec::new(),
        };
        Dungeon {
            entrance: None,
            rooms: vec![
                (RoomRole::Boss, kind(room([15, 8, 15], 33), 1)),
                (RoomRole::Loot, kind(room([5, 4, 5], 32), 2)),
            ],
            count: [12, 16],
            main_path: 0.5,
            halls: 0.3,
            corridor: [4, 9],
            loops: 0.4,
            floors: [1, 1],
            floor_gap: 14,
            depth: [24, 30],
            caves: 0,
            wall: 30,
            floor: 31,
            corridor_width: 3,
            corridor_height: 3,
            room_size: [7, 5, 7],
            hall_size: [13, 7, 13],
            loot_size: [5, 4, 5],
            boss_size: [17, 9, 17],
        }
    }

    #[test]
    fn a_dungeon_has_a_main_path_to_its_boss_and_branches_ending_in_loot_every_room_reached() {
        let dungeon = crypt();
        let (mut loops, mut halls, mut treasuries, mut sizes) = (0, 0, 0, std::collections::HashSet::new());
        for seed in 0..40u32 {
            let plan = dungeon.plan(&mut Draws(seed), true);
            let rooms = plan.rooms.len() - 1;
            assert!(rooms >= 8 && rooms <= 16, "seed {seed}: {rooms} rooms");
            assert!(plan.rooms.iter().all(|room| room.3 != u32::MAX), "seed {seed}: a room no corridor reaches");
            let joins = |room: usize| plan.corridors.iter().filter(|&&(a, b)| a == room || b == room).count();
            // One boss's room, a dead end, the throne room, and the furthest room on the main path.
            let bosses: Vec<usize> = (0..plan.roles.len()).filter(|&at| plan.roles[at] == Spot::Boss).collect();
            assert_eq!(bosses.len(), 1, "seed {seed}");
            let boss = bosses[0];
            assert_eq!(joins(boss), 1, "seed {seed}: the boss's room a dead end");
            assert_eq!(plan.rooms[boss].1, Some(0), "seed {seed}: the throne room");
            // Loot at dead ends, at most two treasuries, the rest plain loot rooms.
            for at in (0..plan.roles.len()).filter(|&at| plan.roles[at] == Spot::Loot) {
                assert_eq!(joins(at), 1, "seed {seed}: a loot room not a dead end");
            }
            let here = plan.rooms.iter().filter(|room| room.1 == Some(1)).count();
            assert!(here <= 2, "seed {seed}: {here} treasuries");
            treasuries += here;
            halls += plan.roles.iter().filter(|role| **role == Spot::Hall).count();
            // No two rooms overlap.
            for (i, a) in plan.rooms.iter().enumerate() {
                for b in &plan.rooms[i + 1..] {
                    let apart = (a.0[0] - b.0[0]).abs() > a.2[0] + b.2[0] || (a.0[1] - b.0[1]).abs() > a.2[2] + b.2[2];
                    assert!(apart, "seed {seed}: rooms at {:?} and {:?} overlap", a.0, b.0);
                }
                sizes.insert(a.2);
            }
            // Loops: more corridors than a tree's.
            loops += plan.corridors.len() - (plan.rooms.len() - 1);
        }
        assert!(
            loops >= 10 && halls >= 20 && treasuries >= 30,
            "{loops} loops, {halls} halls, {treasuries} treasuries"
        );
        assert!(sizes.len() >= 6, "rooms of {} sizes", sizes.len());
    }

    #[test]
    fn a_dungeon_is_built_under_the_ground_its_stair_open_from_the_surface_to_its_floor() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
        let dungeon = crypt();
        let site = [40, -30];
        let blocks = dungeon.blocks(5, site, &ground, 2);
        let last = |at: [i32; 3]| blocks.iter().rev().find(|(place, _)| *place == at).map(|(_, block)| *block);
        let surface = ground.top(site[0], site[1]);
        // The stair's floor and the steps between, the pillar in its middle, open over them to the sky.
        let floor = (surface - 30..=surface - 24)
            .find(|&y| last([site[0], y, site[1]]) == Some(30) && last([site[0], y - 1, site[1]]).is_none())
            .expect("the stair's foot");
        for y in floor + 1..=surface + 3 {
            let open = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]]
                .iter()
                .filter(|[dx, dz]| last([site[0] + dx, y, site[1] + dz]) == Some(AIR))
                .count();
            assert!(open >= 6, "{open} of eight open round the stair {} blocks up", y - floor);
            let step =
                [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]][((y - floor) % 8) as usize];
            assert_eq!(last([site[0] + step[0], y, site[1] + step[1]]), Some(31), "a step {} blocks up", y - floor);
        }
        // Rooms at the floor's level: walls round air a block over the floor, and floors of the dungeon's floor block.
        let rooms_open = blocks.iter().filter(|([_, y, _], block)| *y == floor + 2 && *block == AIR).count();
        assert!(rooms_open > 300, "{rooms_open} blocks of open air at walking height");
        // The same from the same seed.
        assert_eq!(dungeon.blocks(5, site, &ground, 2), blocks);
    }

    #[test]
    fn a_floor_over_the_last_ends_its_main_path_in_a_stair_down_and_has_no_boss() {
        let dungeon = crypt();
        for seed in 0..40u32 {
            let plan = dungeon.plan(&mut Draws(seed), false);
            let at = |spot: Spot| (0..plan.roles.len()).filter(|&at| plan.roles[at] == spot).collect::<Vec<_>>();
            assert!(at(Spot::Boss).is_empty(), "seed {seed}: a boss over the last floor");
            let down = at(Spot::Down);
            assert_eq!(down.len(), 1, "seed {seed}");
            assert_eq!(plan.rooms[down[0]].1, None, "seed {seed}: the stair down's room a plain one");
            let joins = plan.corridors.iter().filter(|&&(a, b)| a == down[0] || b == down[0]).count();
            assert_eq!(joins, 1, "seed {seed}: the stair down a dead end");
            assert!(!at(Spot::Loot).is_empty(), "seed {seed}: no loot on the floor");
        }
    }

    #[test]
    fn a_dungeon_breaks_through_into_the_nearest_cave_and_with_no_reach_into_none() {
        let world = crate::generate::starters().into_iter().find(|s| s.name == "Caves").unwrap().world(1, 1.0);
        let caves = world.caves().expect("its caves");
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
        let dungeon = Dungeon { depth: [30, 30], caves: 48, ..crypt() };
        // Of a few sites, the dungeons that find a cave near enough.
        let mut broken = 0;
        for site in [[40, -30], [-120, 60], [200, 200], [-260, -180], [90, 330]] {
            let way = dungeon.blocks(5, site, &ground, 2);
            let none = Dungeon { caves: 0, ..dungeon.clone() }.blocks(5, site, &ground, 2);
            let cut: Vec<[i32; 3]> = way[none.len()..].iter().map(|(at, _)| *at).collect();
            assert!(way[none.len()..].iter().all(|(_, block)| *block == AIR), "the way through is air");
            if cut.is_empty() {
                continue;
            }
            broken += 1;
            // It ends on a cave room's floor, within reach of the dungeon's rooms.
            let near = caves.rooms_near([site[0] as f32, 0.0, site[1] as f32], 400.0);
            let on_floor = near.iter().any(|room| {
                let floor = room.floor.map(|v| v.floor() as i32);
                let over = |at: &&[i32; 3]| (at[0] - floor[0]).abs() <= 1 && (at[2] - floor[2]).abs() <= 1;
                cut.iter().filter(over).any(|at| at[1] == floor[1] + 1)
            });
            assert!(on_floor, "the way from {site:?} reaches no cave room's floor");
            // Never steeper than it runs: four high all along, so it can be walked.
            assert!(cut.len() > 40, "{} blocks cut", cut.len());
        }
        assert!(broken >= 1, "no dungeon of five broke into a cave");
    }

    #[test]
    fn a_dungeon_of_three_floors_goes_down_floor_by_floor_its_boss_on_the_last() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0], biomes: None };
        let dungeon = Dungeon { floors: [3, 3], depth: [12, 12], ..crypt() };
        let site = [40, -30];
        let blocks = dungeon.blocks(5, site, &ground, 2);
        let mut last = std::collections::HashMap::new();
        for &(at, block) in &blocks {
            last.insert(at, block);
        }
        // Floors: the levels with many blocks of floor, three of them, each further down by at least its gap.
        let mut floors = std::collections::BTreeMap::new();
        for (at, block) in &last {
            if *block == 31 {
                *floors.entry(at[1]).or_insert(0) += 1;
            }
        }
        let levels: Vec<i32> = floors.iter().filter(|(_, count)| **count > 150).map(|(y, _)| *y).collect();
        assert_eq!(levels.len(), 3, "floors at {floors:?}");
        assert!(levels.windows(2).all(|pair| pair[1] - pair[0] >= 14), "floors at {levels:?}");
        // The boss's throne on the lowest floor only, a treasury on more than one.
        let throne: Vec<i32> = last.iter().filter(|(_, block)| **block == 33).map(|(at, _)| at[1]).collect();
        assert!(!throne.is_empty() && throne.iter().all(|&y| y >= levels[0] && y < levels[1]), "throne at {throne:?}");
        let treasuries: std::collections::HashSet<usize> = last
            .iter()
            .filter(|(_, block)| **block == 32)
            .map(|(at, _)| levels.iter().rposition(|&y| at[1] >= y).unwrap_or(0))
            .collect();
        assert!(treasuries.len() >= 2, "treasuries on floors {treasuries:?}");
        // Each floor's stair goes down to the floor under: a pillar all the way between, open round it.
        let ring = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]];
        for pair in levels.windows(2) {
            let shaft = last.keys().filter(|at| at[1] == pair[0] + 1).any(|&[x, _, z]| {
                (pair[0] + 1..pair[1]).all(|y| {
                    let open = ring.iter().filter(|[dx, dz]| last.get(&[x + dx, y, z + dz]) != Some(&30)).count();
                    last.get(&[x, y, z]) == Some(&30) && open >= 7
                })
            });
            assert!(shaft, "no stair from {} down to {}", pair[1], pair[0]);
        }
        assert_eq!(dungeon.blocks(5, site, &ground, 2), blocks);
    }
}
