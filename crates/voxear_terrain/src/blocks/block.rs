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
//! color = [0.9, 0.3, 0.05]
//! heat = { celsius = 1100.0, reach = 4.0 }  # magma: hot round it, as ice is cold
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::file::BlockKind;

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

/// The shape a block is drawn in.
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
}

impl BlockShape {
    pub const ALL: [BlockShape; 3] = [BlockShape::Cube, BlockShape::Bushy, BlockShape::Vine];

    pub fn name(self) -> &'static str {
        match self {
            BlockShape::Cube => "Cube",
            BlockShape::Bushy => "Bushy",
            BlockShape::Vine => "Vine",
        }
    }

    pub fn is_cube(&self) -> bool {
        *self == BlockShape::Cube
    }

    pub fn is_vine(&self) -> bool {
        *self == BlockShape::Vine
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
        look(self) == look(other) && shape(self) == shape(other)
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
