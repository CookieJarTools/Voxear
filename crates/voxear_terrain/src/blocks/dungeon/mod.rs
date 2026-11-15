//! Dungeons: great spaces cut into the rock under the land, reached by a
//! grand stair from an entrance the land itself suggests: a gate carved
//! into a cliff, a temple door in a hillside, a stair round the wall of a
//! sinkhole.
//!
//! A dungeon is planned once for each site (see [`plan`]) and kept: a
//! graph of spaces on each floor (a threshold, two ways round to a pillared
//! hall, set pieces, the boss), each space given its place, its floor's
//! height and its size, and every space kept wholly under the land, with
//! at least `cover` blocks of rock over and round it, whatever the land does
//! over it: a slope, an escarpment's wall, a lake's bed. Only the entrance
//! comes up to the surface. Each chunk column then builds only its own part
//! (see [`build`]): the spaces carved out of the rock with soft natural
//! edges, built stone laid crisp into them (floors, columns, pointed arches,
//! stairs, bridges, balustrades, statues), then worn: broken edges and
//! rubble, cracked and mossy stone, plants near the way in, braziers along
//! the main path and cold light on the side ways.
//!
//! What a `.structure` file says of a dungeon is [`DungeonDef`]; a world
//! builds it as [`Dungeon`], its blocks numbered.

use serde::{Deserialize, Serialize};

use super::chunk::{AIR, BlockId};
use super::structure::{TownHouseKind, TownPiece};

mod build;
mod carve;
mod entrance;
mod land;
mod noise;
mod pieces;
mod plan;
mod rooms;

pub(crate) use build::build;
pub use land::Entrance;
pub use plan::{DungeonLayout, LightKind, LightSpot, Space, SpaceKind, Way, WayCell, WayKind};
pub(crate) use plan::{cached_layout, layout_key, plan};

#[cfg(test)]
pub(crate) mod tests;

/// A dungeon: spaces under the land joined by passages and stairs, on one
/// floor or several, reached by a grand stair from an entrance at the
/// surface.
///
/// The easy way in is a theme, a size and how many floors; everything else
/// has a good value for the theme:
///
/// ```toml
/// [dungeon]
/// theme = "temple"
/// size = "medium"             # small, medium or large
/// floors = [1, 3]
/// ```
///
/// The full way in sets the rest: the kinds of entrance it may use, how
/// much rock it keeps over itself, the blocks by what they are for, how
/// worn and lit it is, the sizes of its spaces, and your own pieces, by
/// role, in place of the theme's:
///
/// ```toml
/// [dungeon]
/// theme = "temple"
/// size = "large"
/// floors = [2, 3]
/// entrances = ["cliff", "temple"]   # none for any the land offers
/// cover = 8                         # the least rock over and round every space, in blocks
/// caves = 48                        # how far a space breaks through to a cave of the world's; 0 never
/// shafts = true                     # cracks of daylight over its great halls where level land lies over them
///
/// [dungeon.blocks]
/// stone = "Blocks/Stone Bricks.block"
/// floor = "Blocks/Flagstones.block"
/// light = "Blocks/Brazier.block"
///
/// [dungeon.passes]
/// ruin = 0.5
/// wear = 0.5
/// overgrowth = 0.5
/// lights = 1.0
///
/// [dungeon.sizes]
/// hall = [56, 28]                   # length and height, in blocks
///
/// [[dungeon.rooms]]
/// structure = "Structures/Throne.structure"
/// role = "boss"
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonDef {
    pub theme: DungeonTheme,
    pub size: DungeonSize,
    pub floors: [u32; 2],
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entrances: Vec<EntranceKind>,
    pub cover: u32,
    pub caves: u32,
    pub shafts: bool,
    pub blocks: DungeonBlockNames,
    pub passes: DungeonPasses,
    pub sizes: DungeonSizes,
    /// Your own entrance at the surface, over the grand stair, its door on
    /// the side of its model that faces -z: in place of the theme's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrance: Option<String>,
    pub rooms: Vec<DungeonRoom>,
    /// What a dungeon written before dungeons had themes said, read and
    /// kept to: its walls' and floors' blocks become its stone and floor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
    /// What a dungeon written before themes set, read so its file still
    /// loads, and no longer used: its spaces are laid out by its theme and
    /// size now.
    #[serde(skip_serializing)]
    pub count: Option<[u32; 2]>,
    #[serde(skip_serializing)]
    pub main_path: Option<f32>,
    #[serde(skip_serializing)]
    pub halls: Option<f32>,
    #[serde(skip_serializing)]
    pub corridor: Option<[u32; 2]>,
    #[serde(skip_serializing)]
    pub loops: Option<f32>,
    #[serde(skip_serializing)]
    pub floor_gap: Option<u32>,
    #[serde(skip_serializing)]
    pub depth: Option<[u32; 2]>,
    #[serde(skip_serializing)]
    pub corridor_width: Option<u32>,
    #[serde(skip_serializing)]
    pub corridor_height: Option<u32>,
    #[serde(skip_serializing)]
    pub room_size: Option<[u32; 3]>,
    #[serde(skip_serializing)]
    pub hall_size: Option<[u32; 3]>,
    #[serde(skip_serializing)]
    pub loot_size: Option<[u32; 3]>,
    #[serde(skip_serializing)]
    pub boss_size: Option<[u32; 3]>,
}

impl Default for DungeonDef {
    fn default() -> Self {
        Self {
            theme: DungeonTheme::Temple,
            size: DungeonSize::Medium,
            floors: [1, 2],
            entrances: Vec::new(),
            cover: 8,
            caves: 48,
            shafts: true,
            blocks: DungeonBlockNames::default(),
            passes: DungeonPasses::default(),
            sizes: DungeonSizes::default(),
            entrance: None,
            rooms: Vec::new(),
            wall: None,
            floor: None,
            count: None,
            main_path: None,
            halls: None,
            corridor: None,
            loops: None,
            floor_gap: None,
            depth: None,
            corridor_width: None,
            corridor_height: None,
            room_size: None,
            hall_size: None,
            loot_size: None,
            boss_size: None,
        }
    }
}

/// The look and the set pieces of a dungeon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DungeonTheme {
    /// An old temple carved into natural rock: pointed arches and columns,
    /// statues in alcoves, a gorge with a stair down its wall, a hall
    /// flooded round carved monoliths, galleries over a red-lit chasm, a
    /// throne at the bottom.
    #[default]
    Temple,
}

impl DungeonTheme {
    pub const ALL: [DungeonTheme; 1] = [DungeonTheme::Temple];

    pub fn name(self) -> &'static str {
        match self {
            DungeonTheme::Temple => "Temple",
        }
    }
}

/// How large a dungeon's floors are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DungeonSize {
    /// A threshold, a hall, one set piece and the end: about 120 m across.
    Small,
    /// Two ways round to the hall and two on to the end, with loot off them:
    /// about 200 m across.
    #[default]
    Medium,
    /// As medium, with a second hall and more loot: about 260 m across.
    Large,
}

impl DungeonSize {
    pub const ALL: [DungeonSize; 3] = [DungeonSize::Small, DungeonSize::Medium, DungeonSize::Large];

    pub fn name(self) -> &'static str {
        match self {
            DungeonSize::Small => "Small",
            DungeonSize::Medium => "Medium",
            DungeonSize::Large => "Large",
        }
    }
}

/// A kind of entrance, sited from the land round a dungeon's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntranceKind {
    /// A gate carved into a cliff or an escarpment's wall, a terrace before
    /// it: where the land rises 12 m or more within 8 m.
    Cliff,
    /// A door under a pointed arch between two hooded statues, set into a
    /// hillside: where the land slopes 20 to 40 degrees.
    Temple,
    /// A stair round the wall of a round shaft open to the sky, broken
    /// columns round its rim: on level ground.
    Sinkhole,
}

impl EntranceKind {
    pub const ALL: [EntranceKind; 3] = [EntranceKind::Cliff, EntranceKind::Temple, EntranceKind::Sinkhole];

    pub fn name(self) -> &'static str {
        match self {
            EntranceKind::Cliff => "Cliff gate",
            EntranceKind::Temple => "Temple door",
            EntranceKind::Sinkhole => "Sinkhole stair",
        }
    }
}

/// A dungeon's blocks by what they are for, each a `.block`; any left out
/// is the theme's nearest: the stone for most, the ground's own rock where
/// there is no stone.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonBlockNames {
    /// Dressed stone: walls, columns, arches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stone: Option<String>,
    /// Carved trim: capitals, bases, arch rings, balustrades.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim: Option<String>,
    /// Flagstones under foot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
    /// Fallen stone, in heaps under broken spans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rubble: Option<String>,
    /// Stone cracked where it is most worn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cracked: Option<String>,
    /// Stone grown over with moss, near water, daylight and the way in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mossy: Option<String>,
    /// A half block of the stone, for grand stairs' half steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slab: Option<String>,
    /// Stairs of the stone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stairs: Option<String>,
    /// Statues' stone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statue: Option<String>,
    /// Iron: cages, gates, railings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metal: Option<String>,
    /// Warm light: a brazier's fire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    /// Cold light: blue crystal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cold_light: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lava: Option<String>,
    /// Plants hanging under openings and round the way in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vine: Option<String>,
}

/// How worn and lit a dungeon is, each from 0 for none to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonPasses {
    /// Built stone broken away at its exposed edges, never on the main
    /// path's floor or a stair's treads, with rubble heaped under it.
    pub ruin: f32,
    /// Stone cracked where most exposed, and mossy near water, daylight and
    /// the way in.
    pub wear: f32,
    /// Vines and moss near the way in, thinning with depth.
    pub overgrowth: f32,
    /// Braziers along the main path, cold light on the side ways, a light of
    /// its own in each set piece.
    pub lights: f32,
}

impl Default for DungeonPasses {
    fn default() -> Self {
        Self { ruin: 0.5, wear: 0.5, overgrowth: 0.5, lights: 1.0 }
    }
}

/// The sizes of a dungeon's spaces, in blocks: each as its size says where
/// left at zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DungeonSizes {
    /// A passage's width and height.
    pub passage: [u32; 2],
    /// A grand stair's width.
    pub stair: u32,
    /// A room where fights happen: across and high.
    pub room: [u32; 2],
    /// A pillared hall: long and high; it is half as wide as long.
    pub hall: [u32; 2],
    /// A gorge: long and deep.
    pub gorge: [u32; 2],
    /// A chasm: across and deep.
    pub chasm: [u32; 2],
    /// The boss's arena: across and high.
    pub arena: [u32; 2],
    /// How far down each floor is from the one over it.
    pub floor_gap: u32,
}

/// What a room of a dungeon is for, for your own pieces in its pools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomRole {
    /// Any room on the way where fights happen.
    #[default]
    Any,
    /// A great hall on the main path.
    Hall,
    /// A dead end off the way, where the loot is.
    Loot,
    /// The boss's arena at the end of the last floor.
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

/// One of your own pieces for a dungeon, by reference to a `.structure`
/// with a model: its role, how often it is chosen against the others of its
/// role, at most how many a floor has (0 for no limit), and how far along
/// the floor it stands, as a share of the way from its start to its end.
/// It stands in the middle of the space it takes, its floor on the space's
/// floor, the space carved round it.
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

fn is_zero(value: &u32) -> bool {
    *value == 0
}

impl Default for DungeonRoom {
    fn default() -> Self {
        Self { structure: String::new(), role: RoomRole::Any, weight: 1.0, most: 0, zone: [0.0, 1.0] }
    }
}

/// A dungeon's blocks by what they are for, by a world's numbers: air for
/// any it has none of, which is then its nearest (see
/// [`DungeonBlocks::filled`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DungeonBlocks {
    pub stone: BlockId,
    pub trim: BlockId,
    pub floor: BlockId,
    pub rubble: BlockId,
    pub cracked: BlockId,
    pub mossy: BlockId,
    /// A half block lying low.
    pub slab: BlockId,
    /// Stairs rising towards north, east, south and west.
    pub stairs: [BlockId; 4],
    pub statue: BlockId,
    pub metal: BlockId,
    pub light: BlockId,
    pub cold_light: BlockId,
    pub water: BlockId,
    pub lava: BlockId,
    pub vine: BlockId,
}

impl DungeonBlocks {
    /// Each it has none of as its nearest: stone the ground's rock `under`,
    /// trim, rubble, cracked, mossy and statues the stone, the floor the
    /// trim. Slabs, stairs, iron, lights, water, lava and vines stay air:
    /// what needs them does without.
    pub fn filled(self, under: BlockId) -> Self {
        let or = |block: BlockId, other: BlockId| if block == AIR { other } else { block };
        let stone = or(self.stone, under);
        let trim = or(self.trim, stone);
        Self {
            stone,
            trim,
            floor: or(self.floor, trim),
            rubble: or(self.rubble, stone),
            cracked: or(self.cracked, stone),
            mossy: or(self.mossy, stone),
            statue: or(self.statue, trim),
            ..self
        }
    }
}

/// A dungeon as a world builds it: its settings as its file has them, its
/// blocks and pieces by the world's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct Dungeon {
    pub theme: DungeonTheme,
    pub size: DungeonSize,
    pub floors: [u32; 2],
    pub entrances: Vec<EntranceKind>,
    pub cover: u32,
    pub caves: u32,
    pub shafts: bool,
    pub blocks: DungeonBlocks,
    pub passes: DungeonPasses,
    pub sizes: DungeonSizes,
    pub entrance: Option<TownPiece>,
    pub rooms: Vec<(RoomRole, TownHouseKind)>,
}

impl Default for Dungeon {
    fn default() -> Self {
        let def = DungeonDef::default();
        Self {
            theme: def.theme,
            size: def.size,
            floors: def.floors,
            entrances: def.entrances,
            cover: def.cover,
            caves: def.caves,
            shafts: def.shafts,
            blocks: DungeonBlocks::default(),
            passes: def.passes,
            sizes: def.sizes,
            entrance: None,
            rooms: Vec::new(),
        }
    }
}

/// How far round its site a dungeon looks for its entrance, in blocks.
pub(crate) const ENTRANCE_SEARCH: f32 = 48.0;

impl Dungeon {
    /// How wide its grand stair is, in blocks.
    pub(crate) fn sizes_stair(&self) -> i32 {
        plan::Sizes::of(self).stair
    }

    /// How wide its passages are, in blocks.
    pub(crate) fn sizes_passage(&self) -> i32 {
        plan::Sizes::of(self).passage[0]
    }

    /// How far from its site it stands on the ground, in blocks: its
    /// entrance, wherever round the site the land puts it.
    pub(crate) fn footprint(&self) -> f32 {
        ENTRANCE_SEARCH + 14.0
    }

    /// How far from its site it reaches under the ground, in blocks, at the
    /// most: its floors laid out as far as they can go, a grand stair from
    /// the entrance before them, and a way broken through to a cave.
    pub(crate) fn reach(&self) -> f32 {
        let floor = plan::Sizes::of(self).floor_reach();
        let floors = self.floors[0].max(self.floors[1]).clamp(1, 4) as f32;
        // Each floor below turns back under the one over it as often as it runs on, so floors reach half again.
        ENTRANCE_SEARCH + 90.0 + floor * (1.0 + (floors - 1.0) * 0.5) + self.caves as f32
    }
}

/// The layout of `dungeon` at `site` of its placement, in a world of
/// blocks `size` metres a side whose corner lies at `position` in `world`,
/// its lowest block `bottom`, its top laid in half blocks where `halved`:
/// as that world plans it when it builds the dungeon, and kept for every
/// chunk of it. None where the land round the site offers no entrance, or
/// no room under it for a floor.
pub fn layout_at(
    dungeon: &Dungeon,
    world: &crate::generate::WorldGen,
    (size, position, bottom, halved): (f32, [f32; 2], i32, bool),
    site: &crate::generate::structures::Site,
) -> Option<std::sync::Arc<DungeonLayout>> {
    let ground = super::structure::Ground { world, size, position, biomes: None, halved };
    let block = [0, 1].map(|axis| ((site.at[axis] - position[axis]) / size).floor() as i32);
    let key = layout_key(dungeon, &ground, block, bottom);
    cached_layout(key, || plan(dungeon, &ground, block, site.seed, bottom))
}
