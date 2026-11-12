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
/// road = "Blocks/Gravel.block" # roads to the next towns of its kind; none for none
/// road_width = 3
/// bridge = "Blocks/Planks.block" # where a road crosses water
/// link = 0.7                  # how many of the roads to its nearest towns are made, 0 to 1
///
/// [[town.houses]]
/// structure = "Structures/Cottage.structure"
/// weight = 3.0
/// ```
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
            road: None,
            road_width: 3,
            bridge: None,
            link: 0.7,
        }
    }
}

/// One kind of house of a town, by reference to a `.structure` with a
/// model, and how often it is chosen against the others.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TownHouse {
    pub structure: String,
    pub weight: f32,
}

impl Default for TownHouse {
    fn default() -> Self {
        Self { structure: String::new(), weight: 1.0 }
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

/// A town as a world builds it: its pieces by the world's numbers, how many
/// houses and how far its main streets run, in blocks, its streets' block
/// (air for none) and width, and how much they bend and branch; and its
/// roads to the next towns of its kind: their block (air for none), width,
/// the block of their bridges over water, and how many are made.
#[derive(Clone, Debug, PartialEq)]
pub struct Town {
    pub middle: Option<TownPiece>,
    pub houses: Vec<(TownPiece, f32)>,
    pub count: [u32; 2],
    pub radius: [f32; 2],
    pub path: BlockId,
    pub path_width: u32,
    pub wander: f32,
    pub branching: f32,
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
            StructureBlocks::Town(_) => Vec::new(),
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
        }
    }

    /// How far from its site it reaches along the ground, in blocks, at the
    /// most.
    fn reach(&self) -> f32 {
        match &self.blocks {
            StructureBlocks::Model { size, .. } => (size[0].max(size[2]) as f32 * 0.71) + 1.0,
            StructureBlocks::Plant { plant, .. } => plant.crown_width[1].max(plant.trunk) * 0.6 + 4.0,
            StructureBlocks::Town(town) => town.reach(),
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
            self.houses.iter().map(|(house, _)| house.size[0].max(house.size[2]) as f32).fold(0.0f32, f32::max);
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
        let total: f32 = self.houses.iter().map(|(_, weight)| weight.max(0.0)).sum();
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
            // A kind of house by weight; turned so its front faces back along `out`, to the street.
            let mut pick = draws.unit() * total;
            let house = self.houses.iter().find(|(_, weight)| {
                pick -= weight.max(0.0);
                pick < 0.0
            });
            let Some((house, _)) = house.or(self.houses.last()) else { break };
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
    let ground = Ground { world, size, position };
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
    for (which, structure) in structures.list.iter().enumerate() {
        let (reach, footprint) = (structure.reach(), structure.footprint());
        // A town's roads to the next towns of its kind, laid before any of its buildings.
        if let StructureBlocks::Town(town) = &structure.blocks
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
            kept.push(Taken { which, round: None, buildings: Vec::new(), streets: cells });
        }
        // Sites of earlier structures that may stand in this one's way are found further out, by its reach too.
        for site in structure.placement.tries_near(world.seed(), low, high, (reach + 64.0) * size) {
            let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
            if !fits(structure, &site) {
                continue;
            }
            // Kept clear of what earlier structures take up; its own are kept apart by its placement's separation.
            if kept.iter().filter(|taken| taken.which != which).any(|taken| taken.meets(block, footprint, reach)) {
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
            kept.push(Taken { which, round: layout.is_none().then_some((block, footprint)), buildings, streets });
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
            houses: vec![(model([7, 5, 7], 12), 2.0), (model([5, 4, 5], 12), 1.0)],
            count: [8, 14],
            radius: [24.0, 44.0],
            path: 11,
            path_width: 3,
            wander: 0.5,
            branching: 0.6,
            road: AIR,
            road_width: 3,
            bridge: AIR,
            link: 0.8,
        }
    }

    #[test]
    fn a_town_grows_streets_from_its_middle_with_houses_along_them_apart_facing_them() {
        let world = Preset::RollingHills.world();
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0] };
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
        let ground = Ground { world: &world, size: 1.0, position: [0.0, 0.0] };
        let town = Town { road: 13, bridge: 14, ..village() };
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
}
