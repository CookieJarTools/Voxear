//! A `.block` file: one block, as text. Its look (its colour, the materials
//! its faces are drawn with, and whether it is a liquid) and its rules: how
//! long it takes to break, with which kind of tool, and what it drops. A
//! world of blocks lists the blocks it is made of (see `file`), so every
//! world listing a block draws it the same, and a change to the block
//! reaches them all.
//!
//! Its look is all this crate reads: how a world's faces are made and what
//! is stood on. Its rules are kept here to be written back as they were
//! read, and are a game's scripts' to apply (`Block` in Voxear.Core).
//!
//! ```toml
//! color = [0.5, 0.5, 0.52]
//! material = "Materials/Stone.material"  # every face; `top` and `bottom` in its place for those
//! hardness = 1.5                         # seconds to break by hand
//! tool = "Tools/Pickaxe.asset"           # the kind of tool that breaks it fast
//! tier = 1                               # the lowest tier of that tool that gets its drops
//!
//! [[drops]]
//! item = "Items/Cobblestone.asset"
//! count = [1, 1]                         # from so many to so many
//! chance = 1.0
//!
//! [sounds]
//! break = "Sounds/Stone Break.ogg"       # and `place`, `step`
//! ```
//!
//! ```toml
//! color = [0.04, 0.46, 0.33]  # the water's own colour
//! liquid = "water"            # see-through, and things float in it
//! ```
//!
//! ```toml
//! color = [0.3, 0.62, 0.16]
//! shape = "bushy"             # a cube with tufts of leaves reaching out of it; or "vine"
//! sways = true
//! ```
//!
//! ```toml
//! color = [0.45, 0.3, 0.16]
//! shape = "door"              # or "slab", "stairs", "fence", "wall", "pane", "trap_door", "fence_gate",
//!                             # "lever", "button", "chain", "rope", or "boxes" for its own
//!
//! [motion]                    # how it swings open; a door's own unless it says
//! keys = [{ time = 0.0, turn = 0.0 }, { time = 0.4, turn = 95.0 }, { time = 0.5, turn = 90.0 }]
//! ```
//!
//! ```toml
//! shape = "boxes"             # a lantern
//! hangs = true
//! swing = 4.0
//!
//! [[boxes]]
//! from = [5, 1, 5]
//! to = [11, 9, 11]
//! ```
//!
//! ```toml
//! color = [0.9, 0.3, 0.05]
//! heat = { celsius = 1100.0, reach = 4.0 }  # magma: hot round it, as ice is cold
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{
    file::BlockKind,
    shape::{BlockFacing, BlockMotion, BoxDef, CustomShape, MOST_BOXES, MOST_SWING, PRESS_TIME, PartLook, SWING},
};

/// The colour a block that cannot be found is drawn in, loud enough to see.
pub const MISSING: [f32; 3] = [1.0, 0.0, 1.0];

/// What a `.block` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockDef {
    /// Its colour where a face has no material; a liquid's own colour.
    pub color: [f32; 3],
    /// Every face's material; none for its colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Its top's and bottom's, in place of its material.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    /// How far up the order of blocks that meet it lies: where it lies
    /// beside a block with a lower one, in one plane, its picture creeps
    /// over that block's edge (see `mesh`). 0 keeps to itself.
    #[serde(skip_serializing_if = "is_no_overlap")]
    pub overlap: u8,
    /// Which liquid it is, where it is one.
    #[serde(skip_serializing_if = "Liquid::is_none")]
    pub liquid: Liquid,
    /// Whether it sways in the wind, as leaves do: its corners that touch
    /// no still block move a little with the level's wind.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sways: bool,
    /// Its shape: a cube, a cube with tufts of leaves, or a vine.
    #[serde(skip_serializing_if = "BlockShape::is_cube")]
    pub shape: BlockShape,
    /// How many seconds it takes to break by hand.
    pub hardness: f32,
    /// Whether nothing breaks it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unbreakable: bool,
    /// The kind of tool that breaks it fast: a reference to a Tool Kind
    /// data asset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The lowest tier of its kind of tool that gets its drops; 0 for
    /// anything, a hand too.
    #[serde(skip_serializing_if = "is_zero")]
    pub tier: u32,
    /// A data asset of the game's own for it: what its scripts want to know
    /// of it, or rules of its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// What it drops when it is broken.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub drops: Vec<BlockDrop>,
    /// The sounds of breaking it, placing it and stepping on it.
    #[serde(skip_serializing_if = "BlockSounds::is_none")]
    pub sounds: BlockSounds,
    /// How hot or cold it is, for what is near it: magma, fire, ice and
    /// snow. None for a block that is as warm as where it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heat: Option<BlockHeat>,
    /// Whether a player can use it: a door, a trap door, a gate, a lever
    /// and a button are always; any other block says so, and using it is
    /// for a game's scripts to answer (see `shape::UseKind`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub usable: bool,
    /// How many seconds a button stays on once pressed; [`PRESS_TIME`]
    /// unless it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub press_time: Option<f32>,
    /// How far a hanging block swings at most, in degrees, in the wind and
    /// as things brush past it; [`SWING`] unless it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swing: Option<f32>,
    /// Whether a block of its own boxes hangs from what is over it and
    /// sways, as a chain does: a lantern.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hangs: bool,
    /// How its moving parts go from shut to open, or off to on; its
    /// shape's own unless it says (see `shape::BlockMotion::default_for`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<BlockMotion>,
    /// A block of its own boxes' boxes, facing north, at most
    /// [`MOST_BOXES`].
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub boxes: Vec<BoxDef>,
    /// What a block of its own boxes' moving boxes turn about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hinge: Option<BoxHinge>,
}

/// What the moving boxes of a block of its own boxes turn about: a point in
/// sixteenths of a block from its corner, and the way of the axis, which
/// they turn about as a right hand curls and move along.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoxHinge {
    pub pivot: [f32; 3],
    pub axis: BlockFacing,
}

impl Default for BoxHinge {
    fn default() -> Self {
        Self { pivot: [8.0, 0.0, 8.0], axis: BlockFacing::Up }
    }
}

/// A block's own temperature and how far it reaches: at the block it is
/// `celsius`, and it fades to where it is by `reach` metres away.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockHeat {
    pub celsius: f32,
    pub reach: f32,
}

impl Default for BlockHeat {
    fn default() -> Self {
        Self { celsius: 600.0, reach: 3.0 }
    }
}

/// The farthest a block's heat reaches, in metres.
pub const MAX_HEAT_REACH: f32 = 16.0;

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn is_no_overlap(overlap: &u8) -> bool {
    *overlap == 0
}

/// Something a block drops: an item, by reference to its data asset, from
/// `count[0]` to `count[1]` of it, `chance` of the time from 0 to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockDrop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub count: [u32; 2],
    pub chance: f32,
}

impl Default for BlockDrop {
    fn default() -> Self {
        Self { item: None, count: [1, 1], chance: 1.0 }
    }
}

/// A block's sounds, each a reference to a sound: breaking it, placing it,
/// and a step on it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockSounds {
    #[serde(rename = "break", skip_serializing_if = "Option::is_none")]
    pub broken: Option<String>,
    #[serde(rename = "place", skip_serializing_if = "Option::is_none")]
    pub placed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

impl BlockSounds {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

/// Which liquid a block is. Water is drawn see-through with the water's
/// look, its faces only where it meets air; nothing stands on it, what
/// floats floats in it, and it flows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liquid {
    #[default]
    None,
    Water,
}

impl Liquid {
    pub fn is_none(&self) -> bool {
        *self == Liquid::None
    }
}

/// The shape a block is drawn in: a whole cube, leaves and vines, or a
/// block of parts, a few boxes in sixteenths of a block (see `shape`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockShape {
    /// A cube, as most blocks are.
    #[default]
    Cube,
    /// A cube with tufts of leaves reaching out past it wherever it meets
    /// open air, so a crown of such blocks is a soft mass of leaves.
    Bushy,
    /// A vine: a thin sheet against each block of ground beside it, or two
    /// crossed sheets where it hangs free. It is no ground: nothing stands
    /// on it, and it hides nothing.
    Vine,
    /// Half a block, against the side it faces: lying low or high, or
    /// standing up against a side.
    Slab,
    /// A step up towards the way it faces, turning a corner where another
    /// stair meets it from the side; upside down in its upper half.
    Stairs,
    /// A post, with rails to the fences, gates and whole blocks beside it.
    Fence,
    /// A thick post, with walls to the walls, gates and whole blocks
    /// beside it, and none where it runs straight on.
    Wall,
    /// A thin sheet, reaching to the panes and whole blocks beside it.
    Pane,
    /// A door against the side it faces, two blocks high, that swings open
    /// on its hinge.
    Door,
    /// A hatch lying in its half of the block, that swings up on its hinge.
    TrapDoor,
    /// A gate between two posts, that swings open.
    FenceGate,
    /// A handle on a plate that tips over, turning on and off.
    Lever,
    /// A button that sinks when pressed, on for a moment.
    Button,
    /// Links hanging from what is over them, swaying a little.
    Chain,
    /// A rope hanging from what is over it, swaying a little.
    Rope,
    /// The block's own boxes (see `BlockDef::boxes`).
    Boxes,
}

impl BlockShape {
    pub const ALL: [BlockShape; 16] = [
        BlockShape::Cube,
        BlockShape::Bushy,
        BlockShape::Vine,
        BlockShape::Slab,
        BlockShape::Stairs,
        BlockShape::Fence,
        BlockShape::Wall,
        BlockShape::Pane,
        BlockShape::Door,
        BlockShape::TrapDoor,
        BlockShape::FenceGate,
        BlockShape::Lever,
        BlockShape::Button,
        BlockShape::Chain,
        BlockShape::Rope,
        BlockShape::Boxes,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlockShape::Cube => "Cube",
            BlockShape::Bushy => "Bushy",
            BlockShape::Vine => "Vine",
            BlockShape::Slab => "Slab",
            BlockShape::Stairs => "Stairs",
            BlockShape::Fence => "Fence",
            BlockShape::Wall => "Wall",
            BlockShape::Pane => "Pane",
            BlockShape::Door => "Door",
            BlockShape::TrapDoor => "Trap Door",
            BlockShape::FenceGate => "Fence Gate",
            BlockShape::Lever => "Lever",
            BlockShape::Button => "Button",
            BlockShape::Chain => "Chain",
            BlockShape::Rope => "Rope",
            BlockShape::Boxes => "Boxes",
        }
    }

    pub fn is_cube(&self) -> bool {
        *self == BlockShape::Cube
    }

    pub fn is_vine(&self) -> bool {
        *self == BlockShape::Vine
    }

    /// Whether it is drawn as sheets besides or in place of a cube: bushy
    /// leaves and vines.
    pub fn is_loose(&self) -> bool {
        matches!(self, BlockShape::Bushy | BlockShape::Vine)
    }

    /// Whether it is a block of parts, drawn as boxes (see `shape`).
    pub fn is_part(&self) -> bool {
        !matches!(self, BlockShape::Cube | BlockShape::Bushy | BlockShape::Vine)
    }

    /// Whether it hangs from what is over it, and sways.
    pub fn hangs(&self) -> bool {
        matches!(self, BlockShape::Chain | BlockShape::Rope)
    }

    /// Whether characters and mobs walk through it, as through a vine:
    /// levers, buttons, chains, ropes, and doors, gates and trap doors
    /// while they are open.
    pub fn passes(&self, open: bool) -> bool {
        match self {
            BlockShape::Lever | BlockShape::Button | BlockShape::Chain | BlockShape::Rope => true,
            BlockShape::Door | BlockShape::TrapDoor | BlockShape::FenceGate => open,
            _ => false,
        }
    }

    /// Whether it collides a block and a half high, so nothing steps or
    /// jumps over it: fences, walls, and gates while they are shut.
    pub fn is_tall(&self, open: bool) -> bool {
        match self {
            BlockShape::Fence | BlockShape::Wall => true,
            BlockShape::FenceGate => !open,
            _ => false,
        }
    }
}

impl Default for BlockDef {
    fn default() -> Self {
        Self {
            color: [0.6, 0.6, 0.6],
            material: None,
            top: None,
            bottom: None,
            overlap: 0,
            liquid: Liquid::None,
            sways: false,
            shape: BlockShape::Cube,
            hardness: 1.0,
            unbreakable: false,
            tool: None,
            tier: 0,
            data: None,
            drops: Vec::new(),
            sounds: BlockSounds::default(),
            heat: None,
            usable: false,
            press_time: None,
            swing: None,
            hangs: false,
            motion: None,
            boxes: Vec::new(),
            hinge: None,
        }
    }
}

impl BlockDef {
    pub fn parse(text: &str) -> Result<Self> {
        let mut block: Self = toml::from_str(text)?;
        block.hardness = if block.hardness.is_finite() { block.hardness.max(0.0) } else { 1.0 };
        for drop in &mut block.drops {
            drop.count = [drop.count[0].min(drop.count[1]), drop.count[0].max(drop.count[1])];
            drop.chance = if drop.chance.is_finite() { drop.chance.clamp(0.0, 1.0) } else { 1.0 };
        }
        if let Some(heat) = &mut block.heat {
            heat.celsius = if heat.celsius.is_finite() { heat.celsius.clamp(-273.0, 5000.0) } else { 20.0 };
            heat.reach = if heat.reach.is_finite() { heat.reach.clamp(0.0, MAX_HEAT_REACH) } else { 0.0 };
        }
        block.press_time = block.press_time.map(|t| if t.is_finite() { t.clamp(0.05, 60.0) } else { PRESS_TIME });
        block.swing = block.swing.map(|s| if s.is_finite() { s.clamp(0.0, MOST_SWING) } else { SWING });
        block.motion = block.motion.take().map(BlockMotion::tidied);
        block.boxes.truncate(MOST_BOXES);
        Ok(block)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a block's file writes as TOML")
    }

    /// A block of a colour alone.
    pub fn colored(color: [f32; 3]) -> Self {
        Self { color, ..Self::default() }
    }

    /// The block a kind kept in a world's own file is: its look and whether
    /// it is water, with the rules a new block has.
    pub fn from_kind(kind: &BlockKind) -> Self {
        Self {
            color: kind.color,
            material: kind.material.clone(),
            top: kind.top.clone(),
            bottom: kind.bottom.clone(),
            liquid: if kind.water { Liquid::Water } else { Liquid::None },
            ..Self::default()
        }
    }

    /// Whether it is drawn as `other` is, whatever their rules.
    pub fn looks_like(&self, other: &Self) -> bool {
        let look = |block: &Self| (block.color, block.material.clone(), block.top.clone(), block.bottom.clone());
        let shape = |block: &Self| (block.liquid, block.overlap, block.shape);
        look(self) == look(other) && shape(self) == shape(other) && self.part_look() == other.part_look()
    }

    /// What a world draws of it as a block of parts: whether it is used,
    /// how a button presses, how far it swings, its motion (its own or its
    /// shape's), and its own boxes.
    pub fn part_look(&self) -> PartLook {
        let shape = if self.liquid.is_none() { self.shape } else { BlockShape::Cube };
        let hangs = shape.hangs() || shape == BlockShape::Boxes && self.hangs;
        let custom = (shape == BlockShape::Boxes).then(|| {
            let hinge = self.hinge.unwrap_or_default();
            CustomShape::of(&self.boxes, hinge.pivot, hinge.axis)
        });
        let moves = custom.is_some_and(|custom| custom.boxes().iter().any(|part| part.moves.is_some()));
        let motion = match shape {
            BlockShape::Boxes if !moves => None,
            _ => self.motion.clone().filter(|motion| !motion.keys.is_empty()).or_else(|| BlockMotion::default_for(shape)),
        };
        PartLook {
            usable: self.usable,
            press_time: self.press_time.unwrap_or(PRESS_TIME),
            swing: if hangs { self.swing.unwrap_or(SWING) } else { 0.0 },
            hangs,
            motion,
            custom,
        }
    }

    /// Gives `kind`, a world's entry naming this block, the block's look.
    pub fn fill(&self, kind: &mut BlockKind) {
        kind.color = self.color;
        kind.material = self.material.clone();
        kind.top = self.top.clone();
        kind.bottom = self.bottom.clone();
        kind.water = self.liquid == Liquid::Water;
        kind.sways = self.sways;
        kind.overlap = if self.liquid.is_none() { self.overlap } else { 0 };
        kind.shape = if kind.water { BlockShape::Cube } else { self.shape };
        kind.part = self.part_look();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_reads_as_written_and_fills_a_kinds_look() {
        let grass = BlockDef {
            top: Some("Materials/Grass.material".into()),
            material: Some("Materials/Dirt.material".into()),
            ..BlockDef::colored([0.33, 0.52, 0.2])
        };
        assert_eq!(BlockDef::parse(&grass.to_text()).unwrap(), grass);
        assert!(!grass.to_text().contains("liquid"), "a block that is no liquid does not say so");
        let water = BlockDef { liquid: Liquid::Water, ..BlockDef::colored([0.04, 0.46, 0.33]) };
        assert!(water.to_text().contains("liquid = \"water\""));
        assert_eq!(BlockDef::parse(&water.to_text()).unwrap(), water);
        assert!(BlockDef::parse("colour = [1.0, 0.0, 0.0]").is_err(), "a key it does not know is a mistake");

        // A world's entry naming the block is drawn as the block says, and a kind made a block is the same block.
        let mut kind = BlockKind::of("Blocks/Grass.block");
        grass.fill(&mut kind);
        assert_eq!((kind.color, kind.top.as_deref()), ([0.33, 0.52, 0.2], Some("Materials/Grass.material")));
        assert!(!kind.water);
        assert_eq!(BlockDef::from_kind(&kind), grass);
        water.fill(&mut kind);
        assert!(kind.water && kind.material.is_none() && kind.top.is_none());
        // A block that creeps over those beside it says how far up the order it lies; a liquid never does.
        let creeping = BlockDef { overlap: 2, ..grass.clone() };
        assert!(creeping.to_text().contains("overlap = 2") && !grass.to_text().contains("overlap"));
        assert_eq!(BlockDef::parse(&creeping.to_text()).unwrap(), creeping);
        creeping.fill(&mut kind);
        assert!(kind.overlap == 2 && !creeping.looks_like(&grass));
        BlockDef { overlap: 2, ..water.clone() }.fill(&mut kind);
        assert_eq!(kind.overlap, 0);
        assert_eq!(BlockDef::from_kind(&kind), water);
    }

    #[test]
    fn a_blocks_shape_is_written_only_where_it_is_no_cube_and_reaches_its_worlds_faces() {
        use crate::blocks::{file::BlockFile, is_bushy, is_ground, is_vine};
        let leaves = BlockDef { shape: BlockShape::Bushy, sways: true, ..BlockDef::colored([0.3, 0.62, 0.16]) };
        let vine = BlockDef { shape: BlockShape::Vine, ..BlockDef::colored([0.2, 0.46, 0.14]) };
        assert!(leaves.to_text().contains("shape = \"bushy\"") && vine.to_text().contains("shape = \"vine\""));
        assert!(!BlockDef::default().to_text().contains("shape"), "a cube does not say so");
        assert_eq!(BlockDef::parse(&leaves.to_text()).unwrap(), leaves);
        assert_eq!(BlockDef::parse(&vine.to_text()).unwrap(), vine);
        assert!(BlockDef::parse("shape = \"ball\"").is_err(), "a shape there is none of");
        assert!(!leaves.looks_like(&BlockDef { shape: BlockShape::Cube, ..leaves.clone() }));
        // A world listing them draws them so: leaves ground with tufts, a vine no ground; water is no shape.
        let mut file = BlockFile::default();
        for block in [&leaves, &vine, &BlockDef { liquid: Liquid::Water, shape: BlockShape::Vine, ..vine.clone() }] {
            let mut kind = BlockKind::of("Blocks/Some.block");
            block.fill(&mut kind);
            file.blocks.push(kind);
        }
        let (looks, faces) = file.faces();
        assert!(is_bushy(&faces, 4) && is_ground(&faces, 4) && faces[4].sways);
        assert!(is_vine(&faces, 5) && !is_ground(&faces, 5) && !crate::blocks::hides(&faces, 5));
        assert!(faces[6].water && !is_vine(&faces, 6) && faces[6].shape.is_cube());
        // Their faces are drawn as leaves, a colour's of its own beside a cube's of the same colour.
        assert!(looks[faces[4].sides as usize].leafy && looks[faces[5].sides as usize].leafy);
        assert!(!looks[faces[1].sides as usize].leafy);
        let mut same_colour = BlockKind::of("Blocks/Other.block");
        BlockDef::colored(leaves.color).fill(&mut same_colour);
        file.blocks.push(same_colour);
        let (looks, faces) = file.faces();
        assert_ne!(faces[7].sides, faces[4].sides);
        assert!(!looks[faces[7].sides as usize].leafy);
    }

    #[test]
    fn a_blocks_rules_read_as_written_and_a_block_written_before_it_had_any_reads_with_a_new_blocks() {
        // As blocks were written while they had a look alone.
        let older = "color = [0.5, 0.5, 0.52]\nmaterial = \"Materials/Stone.material\"\n";
        let read = BlockDef::parse(older).unwrap();
        assert_eq!((read.hardness, read.unbreakable, read.tier), (1.0, false, 0));
        assert!(read.tool.is_none() && read.drops.is_empty() && read.sounds.is_none() && read.data.is_none());
        let stone = BlockDef {
            hardness: 1.5,
            tool: Some("Tools/Pickaxe.asset".into()),
            tier: 1,
            data: Some("Blocks/Data/Stone.asset".into()),
            drops: vec![
                BlockDrop { item: Some("Items/Cobblestone.asset".into()), ..BlockDrop::default() },
                BlockDrop { item: Some("Items/Flint.asset".into()), count: [1, 3], chance: 0.1 },
            ],
            sounds: BlockSounds { broken: Some("Sounds/Stone Break.ogg".into()), ..BlockSounds::default() },
            ..read.clone()
        };
        let text = stone.to_text();
        assert!(text.contains("[[drops]]") && text.contains("[sounds]\nbreak = "), "{text}");
        assert!(!text.contains("unbreakable") && !text.contains("place"), "what it has not is not said: {text}");
        assert_eq!(BlockDef::parse(&text).unwrap(), stone);
        assert!(stone.looks_like(&read) && !stone.looks_like(&BlockDef::default()));
        // A count written the wrong way round, a chance past sure and a hardness below nothing are put right.
        let odd = "hardness = -2.0\n[[drops]]\nitem = \"Items/Flint.asset\"\ncount = [3, 1]\nchance = 4.0\n";
        let read = BlockDef::parse(odd).unwrap();
        assert_eq!((read.hardness, read.drops[0].count, read.drops[0].chance), (0.0, [1, 3], 1.0));
    }

    #[test]
    fn a_hot_block_says_how_hot_and_how_far_and_a_block_without_heat_says_nothing() {
        let magma = BlockDef { heat: Some(BlockHeat { celsius: 1100.0, reach: 4.0 }), ..BlockDef::colored([0.9, 0.3, 0.05]) };
        let text = magma.to_text();
        assert!(text.contains("celsius = 1100"), "{text}");
        assert_eq!(BlockDef::parse(&text).unwrap(), magma);
        assert!(!BlockDef::default().to_text().contains("heat"));
        let far = BlockDef::parse("[heat]\ncelsius = -30.0\nreach = 900.0\n").unwrap().heat.unwrap();
        assert_eq!((far.celsius, far.reach), (-30.0, MAX_HEAT_REACH));
    }
}
