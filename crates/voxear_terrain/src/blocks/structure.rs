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
/// winding stair down from a way in at the surface. Its rooms lie on a grid
/// `spacing` blocks apart, grown out one at a time from the stair's foot so
/// every room is reached, with a few more corridors making loops. Each room
/// is a kind of its own, a structure with a model, chosen by rules as a
/// town's houses are; where no kind may stand, a plain room of its walls
/// is built. A room's doors are in the middles of its sides, a block over
/// its floor: the corridors open them.
///
/// ```toml
/// [dungeon]
/// entrance = "Structures/Crypt Door.structure"  # at the surface, over the stair; none for a bare stair
/// count = [8, 14]             # rooms, from and to
/// spacing = 16                # blocks between rooms' middles
/// depth = [24, 40]            # blocks under the surface, from and to
/// wall = "Blocks/Stone Bricks.block"
/// floor = "Blocks/Cobblestone.block"
/// corridor_width = 3
/// corridor_height = 3
/// loops = 0.2                 # how many more corridors join rooms already joined, 0 to 1
/// room_size = [7, 5, 7]       # a plain room's size, blocks along x, up and along z
///
/// [[dungeon.rooms]]
/// structure = "Structures/Treasure Room.structure"
/// weight = 1.0
/// most = 1
/// zone = [0.7, 1.0]           # how far from the stair, as a share of the furthest room, from and to
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonDef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance: Option<String>,
    pub rooms: Vec<TownHouse>,
    pub count: [u32; 2],
    pub spacing: u32,
    pub depth: [u32; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
    pub corridor_width: u32,
    pub corridor_height: u32,
    pub loops: f32,
    pub room_size: [u32; 3],
}

impl Default for DungeonDef {
    fn default() -> Self {
        Self {
            entrance: None,
            rooms: Vec::new(),
            count: [8, 14],
            spacing: 16,
            depth: [24, 40],
            wall: None,
            floor: None,
            corridor_width: 3,
            corridor_height: 3,
            loops: 0.2,
            room_size: [7, 5, 7],
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
    Plant { plant: PlantDef, wood: BlockId, leaves: BlockId },
    Town(Town),
    Dungeon(Dungeon),
}

/// A dungeon as a world builds it: its way in, its kinds of room (with the
/// rules a town's houses have, their zone the share of the way from the
/// stair to the furthest room), how many rooms and how far apart, how deep,
/// its walls' and floors' blocks, its corridors' size, how many loops, and
/// a plain room's size.
#[derive(Clone, Debug, PartialEq)]
pub struct Dungeon {
    pub entrance: Option<TownPiece>,
    pub rooms: Vec<TownHouseKind>,
    pub count: [u32; 2],
    pub spacing: u32,
    pub depth: [u32; 2],
    pub wall: BlockId,
    pub floor: BlockId,
    pub corridor_width: u32,
    pub corridor_height: u32,
    pub loops: f32,
    pub room_size: [u32; 3],
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
            StructureBlocks::Plant { plant, wood, leaves } => plant
                .grow(site.seed)
                .blocks
                .into_iter()
                .map(|(at, part)| (at, if part == PlantPart::Leaf { *leaves } else { *wood }))
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
/// blocks along x and z, its kind (none for a plain room), how far it
/// reaches along x and z, and how many corridors from the stair; and each
/// corridor, the rooms it joins.
struct DungeonPlan {
    rooms: Vec<([i32; 2], Option<usize>, [i32; 3], u32)>,
    corridors: Vec<(usize, usize)>,
}

impl Dungeon {
    /// The largest any room reaches from its middle along the ground, in
    /// blocks.
    fn room_half(&self) -> i32 {
        let largest = self.rooms.iter().map(|kind| kind.piece.size[0].max(kind.piece.size[2])).max().unwrap_or(0);
        (largest.max(self.room_size[0]).max(self.room_size[2]) / 2) as i32
    }

    /// How far apart its rooms' middles are: its spacing, or more where its
    /// largest room needs it, room for a corridor between.
    fn spacing(&self) -> i32 {
        (self.spacing as i32).max(self.room_half() * 2 + 4)
    }

    /// How many grid steps its rooms may lie from the stair.
    fn steps(&self) -> i32 {
        ((self.count[0].max(self.count[1]) as f32).sqrt().ceil() as i32 + 1).max(2)
    }

    /// How far its rooms reach from its way in, in blocks.
    fn reach(&self) -> f32 {
        ((self.steps() + 1) * self.spacing() + self.room_half() + 4) as f32
    }

    /// Its rooms on the grid, grown out from the stair, the stair's foot the
    /// first room, and the kind each is.
    fn plan(&self, draws: &mut Draws) -> DungeonPlan {
        let [least, most] = [self.count[0].min(self.count[1]), self.count[0].max(self.count[1])];
        let wanted = (least + draws.next() % (most - least + 1).max(1)) as usize + 1;
        let steps = self.steps();
        let ways = [[1, 0], [0, 1], [-1, 0], [0, -1]];
        let mut cells: Vec<[i32; 2]> = vec![[0, 0]];
        let mut corridors: Vec<(usize, usize)> = Vec::new();
        let mut tries = 0;
        while cells.len() < wanted && tries < wanted * 30 {
            tries += 1;
            // From the stair's foot only once: the stair has one way out.
            let from = if cells.len() == 1 { 0 } else { 1 + draws.next() as usize % (cells.len() - 1) };
            let way = ways[draws.next() as usize % 4];
            let cell = [cells[from][0] + way[0], cells[from][1] + way[1]];
            if cells.contains(&cell) || cell[0].abs() > steps || cell[1].abs() > steps {
                continue;
            }
            cells.push(cell);
            corridors.push((from, cells.len() - 1));
        }
        // Loops: rooms next to each other, not yet joined, joined by chance; the stair never.
        for a in 1..cells.len() {
            for b in a + 1..cells.len() {
                let next = (cells[a][0] - cells[b][0]).abs() + (cells[a][1] - cells[b][1]).abs() == 1;
                let joined = corridors.iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a));
                if next && !joined && draws.unit() < self.loops.clamp(0.0, 1.0) {
                    corridors.push((a, b));
                }
            }
        }
        // How many corridors each room is from the stair.
        let mut away = vec![u32::MAX; cells.len()];
        away[0] = 0;
        let mut next = vec![0usize];
        while let Some(room) = next.pop() {
            for &(a, b) in &corridors {
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
        let furthest = away.iter().copied().filter(|&d| d != u32::MAX).max().unwrap_or(1).max(1);
        // Kinds, nearest the stair first, by the rules a town's houses have.
        let mut order: Vec<usize> = (1..cells.len()).collect();
        order.sort_by_key(|&room| away[room]);
        let mut built = vec![0u32; self.rooms.len()];
        let mut kinds: Vec<Option<usize>> = vec![None; cells.len()];
        for room in order {
            let share = away[room] as f32 / furthest as f32;
            let may = |at: usize| {
                let kind = &self.rooms[at];
                let [from, to] = [kind.zone[0].min(kind.zone[1]), kind.zone[0].max(kind.zone[1])];
                (kind.most == 0 || built[at] < kind.most) && share >= from - 1e-4 && share <= to + 1e-4
            };
            let total: f32 = (0..self.rooms.len()).filter(|&at| may(at)).map(|at| self.rooms[at].weight.max(0.0)).sum();
            if total <= 0.0 {
                continue;
            }
            let mut pick = draws.unit() * total;
            let chosen = (0..self.rooms.len()).filter(|&at| may(at)).find(|&at| {
                pick -= self.rooms[at].weight.max(0.0);
                pick < 0.0
            });
            if let Some(at) = chosen {
                built[at] += 1;
                kinds[room] = Some(at);
            }
        }
        let spacing = self.spacing();
        let rooms = cells
            .iter()
            .enumerate()
            .map(|(room, cell)| {
                let size = match kinds[room] {
                    Some(kind) => self.rooms[kind].piece.size,
                    None if room == 0 => [5, 1, 5],
                    None => self.room_size,
                };
                let half = [size[0] as i32 / 2, size[1] as i32, size[2] as i32 / 2];
                ([cell[0] * spacing, cell[1] * spacing], kinds[room], half, away[room])
            })
            .collect();
        DungeonPlan { rooms, corridors }
    }

    /// Its blocks at a site whose middle is block `site` on `ground`, in the
    /// order they are put: its way in at the surface, the stair winding down
    /// to its floor `depth` blocks under the site's ground, its rooms, and
    /// the corridors between them lined with its walls, opening each room's
    /// wall where they reach it. Ground the corridors cut through is cleared.
    fn blocks(&self, seed: u32, site: [i32; 2], ground: &Ground, under: BlockId) -> Vec<([i32; 3], BlockId)> {
        let mut draws = Draws(seed ^ 0x0D06_E0A5);
        let plan = self.plan(&mut draws);
        let surface = ground.top(site[0], site[1]);
        let [least, most] = [self.depth[0].min(self.depth[1]), self.depth[0].max(self.depth[1])];
        let floor = surface - (least + draws.next() % (most - least + 1).max(1)) as i32;
        let (wall, floor_block) =
            (if self.wall == AIR { under } else { self.wall }, if self.floor == AIR { under } else { self.floor });
        let height = self.corridor_height.max(2) as i32;
        let mut out: Vec<([i32; 3], BlockId)> = Vec::new();
        let at = |offset: [i32; 2]| [site[0] + offset[0], site[1] + offset[1]];
        // Its way in, on the ground over the stair.
        if let Some(entrance) = &self.entrance {
            for ([x, y, z], block) in model_pieces(entrance.size, &entrance.blocks) {
                out.push(([site[0] + x, surface + 1 - entrance.sink + y, site[1] + z], block));
            }
        }
        // The stair: a shaft five blocks a side lined with walls, a pillar in its middle, steps winding round it a
        // block up for every block along, down from over the ground to the floor.
        let ring = [[-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0]];
        let top = surface + 3;
        for y in floor..=top {
            for dz in -2..=2i32 {
                for dx in -2..=2i32 {
                    let [x, z] = at([dx, dz]);
                    let edge = dx.abs() == 2 || dz.abs() == 2;
                    let block = if edge {
                        if y > surface { AIR } else { wall }
                    } else if dx == 0 && dz == 0 {
                        if y > surface { AIR } else { wall }
                    } else {
                        let step = ring.iter().position(|cell| *cell == [dx, dz]).unwrap_or(0) as i32;
                        if y == floor || (y - floor).rem_euclid(8) == step { floor_block } else { AIR }
                    };
                    out.push(([x, y, z], block));
                }
            }
        }
        // Its rooms: plain ones of its walls and floor, the rest their models, ground cleared out of each first.
        let mut boxes: Vec<([i32; 2], [i32; 3])> = Vec::new();
        for &(middle, kind, half, _) in plan.rooms.iter().skip(1) {
            let middle = at(middle);
            boxes.push((middle, half));
            let tall = half[1];
            for y in floor..floor + tall {
                for dz in -half[2]..=half[2] {
                    for dx in -half[0]..=half[0] {
                        let edge = dx.abs() == half[0] || dz.abs() == half[2] || y == floor || y == floor + tall - 1;
                        let block = match (kind, edge) {
                            (Some(_), _) => AIR,
                            (None, true) => {
                                if y == floor {
                                    floor_block
                                } else {
                                    wall
                                }
                            }
                            (None, false) => AIR,
                        };
                        out.push(([middle[0] + dx, y, middle[1] + dz], block));
                    }
                }
            }
            if let Some(kind) = kind {
                let piece = &self.rooms[kind].piece;
                for ([x, y, z], block) in model_pieces(piece.size, &piece.blocks) {
                    out.push(([middle[0] + x, floor + y, middle[1] + z], block));
                }
            }
        }
        // Its corridors: from one room's middle to the next, along x and then z, cut through the ground, floored
        // and lined with walls outside the rooms, opening a room's wall where they reach it, a block over its floor.
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
        let stair = |cell: [i32; 2]| (cell[0] - site[0]).abs() <= 1 && (cell[1] - site[1]).abs() <= 1;
        cut.retain(|&cell| !inside(cell) && !stair(cell));
        for &cell in &cut {
            for y in floor + 1..floor + 1 + height {
                out.push(([cell[0], y, cell[1]], AIR));
            }
            out.push(([cell[0], floor, cell[1]], floor_block));
            // Lined with walls round it, but where a room's or the stair's own wall is.
            if !in_wall(cell) {
                out.push(([cell[0], floor + 1 + height, cell[1]], wall));
            }
        }
        for &cell in &cut {
            for beside in [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, -1], [1, -1], [-1, 1]] {
                let next = [cell[0] + beside[0], cell[1] + beside[1]];
                if cut.contains(&next)
                    || in_wall(next)
                    || stair(next)
                    || (next[0] - site[0]).abs() <= 2 && (next[1] - site[1]).abs() <= 2
                {
                    continue;
                }
                for y in floor..=floor + 1 + height {
                    out.push(([next[0], y, next[1]], wall));
                }
            }
        }
        out
    }
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
        biomes.at(px, pz, Place { temperature, humidity, above_sea: height - self.world.sea_level() })
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
    (size, position, bottom): (f32, [f32; 2], i32),
    [cx, cz]: [i32; 2],
    under: BlockId,
) {
    let side = CHUNK as i32;
    let ground = Ground { world, size, position, biomes };
    // The column's own blocks, in metres in the generator's world.
    let low = [position[0] + (cx * side) as f32 * size, position[1] + (cz * side) as f32 * size];
    let high = [low[0] + side as f32 * size, low[1] + side as f32 * size];
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
            biomes.at(site.at[0], site.at[1], Place { temperature, humidity, above_sea })
        });
        structure.placement.fits(SiteGround { above_sea, slope, biome }, |b| structure.biomes.contains(&b))
    };
    let within = |x: i32, z: i32| x.div_euclid(side) == cx && z.div_euclid(side) == cz;
    let farthest = structures.list.iter().map(PlacedStructure::reach).fold(0.0f32, f32::max);
    let most_apart = structures.list.iter().map(|structure| structure.placement.apart).fold(0.0f32, f32::max);
    let look = (farthest * 2.0 + 64.0) * size + most_apart.max(0.0);
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
                let mut put = |y: i32, block: BlockId| put_block(chunks, [cx, cz], bottom, [x, y, z], block);
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
        // Sites of earlier structures that may stand in this one's way are found further out, by its reach too.
        // Sites are looked for as far out for every structure, so a later one finds every earlier site that may
        // stand in its way or within its distance, from any chunk alike.
        for site in structure.placement.tries_near(world.seed(), low, high, look) {
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
            // Only its blocks within this column are built here.
            let (lowest, highest) = (cx * side - reach.ceil() as i32 - 1, (cx + 1) * side + reach.ceil() as i32 + 1);
            if block[0] < lowest
                || block[0] > highest
                || block[1] < cz * side - reach.ceil() as i32 - 1
                || block[1] > (cz + 1) * side + reach.ceil() as i32 + 1
            {
                continue;
            }
            let mut put = |x: i32, y: i32, z: i32, block: BlockId| {
                if within(x, z) {
                    put_block(chunks, [cx, cz], bottom, [x, y, z], block);
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
                _ => vec![Part {
                    at: block,
                    pieces: structure.pieces(&site),
                    ground: structure.ground,
                    sink: structure.sink,
                }],
            };
            for part in parts {
                let foot = ground.top(part.at[0], part.at[1]);
                build_part(&mut put, &ground, &part, foot, under);
            }
        }
    }
    chunks.retain(|(_, chunk)| !chunk.is_air());
    chunks.sort_by_key(|(key, _)| key[1]);
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

    /// A dungeon of walls 30 and floors 31, a treasure room of block 32 at most once in the furthest third.
    fn crypt() -> Dungeon {
        let treasure = TownPiece { size: [7, 4, 7], blocks: vec![32; 7 * 4 * 7], ground: GroundFit::Leave, sink: 0 };
        Dungeon {
            entrance: None,
            rooms: vec![TownHouseKind { piece: treasure, weight: 5.0, most: 1, zone: [0.66, 1.0], biomes: Vec::new() }],
            count: [8, 12],
            spacing: 16,
            depth: [24, 30],
            wall: 30,
            floor: 31,
            corridor_width: 3,
            corridor_height: 3,
            loops: 0.3,
            room_size: [7, 5, 7],
        }
    }

    #[test]
    fn a_dungeons_rooms_are_all_reached_from_its_stair_and_keep_to_their_rules() {
        let dungeon = crypt();
        let mut treasures = 0;
        for seed in 0..40u32 {
            let plan = dungeon.plan(&mut Draws(seed));
            let rooms = plan.rooms.len() - 1;
            assert!((8..=12).contains(&rooms), "seed {seed}: {rooms} rooms");
            // Every room reached from the stair, through the corridors.
            assert!(plan.rooms.iter().all(|room| room.3 != u32::MAX), "seed {seed}: a room no corridor reaches");
            // The stair has one way out; rooms are a grid step apart where a corridor joins them, and none share a place.
            assert_eq!(plan.corridors.iter().filter(|&&(a, b)| a == 0 || b == 0).count(), 1);
            for &(a, b) in &plan.corridors {
                let (pa, pb) = (plan.rooms[a].0, plan.rooms[b].0);
                assert_eq!((pa[0] - pb[0]).abs() + (pa[1] - pb[1]).abs(), dungeon.spacing(), "seed {seed}");
            }
            for (i, a) in plan.rooms.iter().enumerate() {
                assert!(plan.rooms[i + 1..].iter().all(|b| b.0 != a.0));
            }
            // At most one treasure room, and in the furthest third.
            let furthest = plan.rooms.iter().map(|room| room.3).max().unwrap() as f32;
            let found: Vec<_> = plan.rooms.iter().filter(|room| room.1 == Some(0)).collect();
            assert!(found.len() <= 1, "seed {seed}: {} treasure rooms", found.len());
            for room in found {
                assert!(
                    room.3 as f32 / furthest >= 0.66 - 1e-4,
                    "seed {seed}: treasure {} of {furthest} from the stair",
                    room.3
                );
                treasures += 1;
            }
        }
        assert!(treasures >= 30, "{treasures} treasure rooms in forty dungeons");
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
}
