//! A `.blockworld` file: what a world of blocks is, as text. How large a
//! block is, which blocks it has numbered, and the ground its blocks start
//! as.
//!
//! A world can hold any block of the project. It numbers a block the first
//! time it holds one, and `[[blocks]]` is that numbering: its palette, kept
//! by the engine and the editor, out of sight, and never one to edit by
//! hand. Each entry is a block (see `block`), named by its asset, or a kind
//! of the world's own: a colour, as a model of small blocks brings its
//! colours, or as worlds kept all their kinds once, materials and water
//! too. A block's number is its entry's place, from 1, and never changes:
//! entries are only ever added at the end, and an entry once taken out
//! leaves its place empty.
//!
//! ```toml
//! block_size = 1.0
//!
//! [[blocks]]
//! block = "Blocks/Grass.block"
//!
//! [[blocks]]
//! removed = true              # the block once numbered 2
//!
//! [[blocks]]
//! name = "Color 7"            # a kind of its own
//! color = [0.8, 0.2, 0.2]
//!
//! [ground]
//! size = [64, 64]   # blocks along x and z
//! height = 8        # blocks up
//! top = 1           # blocks by their number
//! under = 3
//! under_depth = 3
//! bottom = 3
//! ```

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    BlockFaces, BlockMap,
    chunk::{AIR, BlockId},
};

/// The most blocks a world can number, those taken out counted: as many as
/// a block's number can say.
pub const MAX_BLOCKS: usize = BlockId::MAX as usize;

/// The most ways of drawing a face a world can have: as many as its quads
/// can name (see `BlockFile::faces`).
pub const MAX_LOOKS: usize = 256;

/// The most blocks across a world's flat ground is, a side.
pub const MOST_ACROSS: u32 = 4096;

/// The smallest and the largest a block is a side, in metres.
pub const SMALLEST_BLOCK: f32 = 0.1;
pub const LARGEST_BLOCK: f32 = 10.0;

/// What a `.blockworld` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockFile {
    /// How many metres a block is a side.
    pub block_size: f32,
    /// The blocks it has numbered, from 1 in this order, the entries taken
    /// out counted: its palette.
    pub blocks: Vec<BlockKind>,
    /// The ground its blocks start as.
    pub ground: Ground,
    /// Where its ground is made by a world generator rather than flat: which,
    /// and how (see `generate`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<super::generate::BlockGenerate>,
    /// Whether it is drawn smooth, its surface running between its blocks'
    /// middles as full as each is (see `smooth`): a Voxel Terrain.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub smooth: bool,
    /// How many small cubes its cubes' faces are drawn as made of along a
    /// block's side: none at 0.
    #[serde(skip_serializing_if = "is_zero")]
    pub voxels: u32,
    /// How many small cubes deep the deepest of them are sunk into a face.
    #[serde(skip_serializing_if = "is_usual_relief")]
    pub relief: f32,
}

/// The most small cubes a face is cut into along a block's side, and how
/// deep they sink at most and unless a world says.
pub const MOST_VOXELS: u32 = 64;
pub const DEEPEST_RELIEF: f32 = 4.0;
pub const USUAL_RELIEF: f32 = 2.0;

fn is_zero(count: &u32) -> bool {
    *count == 0
}

fn is_usual_relief(relief: &f32) -> bool {
    *relief == USUAL_RELIEF
}

/// An entry of a world's palette. One naming a `block` is drawn as
/// that asset says, and its own look is filled in from it where the world
/// is made (the file keeps only which). One naming none is a kind of the
/// world's own: its name, its colour where its faces have no material, and
/// the materials it is drawn with, each a reference to a `.material`. One
/// `removed` is the place a block was taken out of, kept so the blocks
/// after it keep their numbers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, into = "Written")]
pub struct BlockKind {
    /// The `.block` it is, by reference.
    pub block: Option<String>,
    /// Whether it was taken out of the list.
    pub removed: bool,
    pub name: String,
    pub color: [f32; 3],
    /// Every face's material; none for its colour.
    pub material: Option<String>,
    /// Its top's and bottom's, in place of its material.
    pub top: Option<String>,
    pub bottom: Option<String>,
    /// Where generated ground is covered with it in place of its top block:
    /// none for nowhere.
    pub cover: Option<BlockCover>,
    /// Whether it is water: drawn see-through with the water's look, its
    /// colour the water's own, its faces only where it meets air; nothing
    /// stands on it, and what floats floats in it.
    pub water: bool,
    /// Whether it is seen through where its faces' pictures are clear, as
    /// leaves are: so one of its materials is cut out. Never written: it
    /// is read from its materials where the world is made (see
    /// `voxear_engine::blocks::resolved`), as a named block's look is.
    #[serde(skip)]
    pub cutout: bool,
    /// Whether it sways in the wind, as leaves do: its block says so, or
    /// one of its materials bends in the wind. Never written: it is read
    /// where the world is made, as `cutout` is.
    #[serde(skip)]
    pub sways: bool,
    /// Which of its faces join those of its own kind beside it, as panes
    /// of glass do: those drawn with a material whose pictures are the five
    /// tiles of a face that joins, a bit each ([`JOINS_SIDES`],
    /// [`JOINS_TOP`], [`JOINS_BOTTOM`]). Never written: it is read from its
    /// materials where the world is made, as `cutout` is.
    #[serde(skip)]
    pub connected: u8,
    /// How far up the order of blocks that meet it lies (see
    /// `block::BlockDef::overlap`). Never written: its block says, or the
    /// texture pack worn.
    #[serde(skip)]
    pub overlap: u8,
}

/// Over a look's number, the look it is with another block's picture over
/// its edge: a look and its twin are that far apart (see
/// [`BlockFile::faces`]).
pub const MIXED_LOOK: u8 = 0x80;

/// The bits of a kind's faces that join: its material's, on its sides and
/// wherever its top or bottom has none of its own; its top's; its bottom's.
pub const JOINS_SIDES: u8 = 1;
pub const JOINS_TOP: u8 = 2;
pub const JOINS_BOTTOM: u8 = 4;

/// An entry as its file says it: one naming a block says which and where it
/// covers, one taken out only that, and a kind of the world's own all its
/// look.
#[derive(Serialize)]
struct Written {
    #[serde(skip_serializing_if = "Option::is_none")]
    block: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    removed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bottom: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    water: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    cover: Option<BlockCover>,
}

impl From<BlockKind> for Written {
    fn from(kind: BlockKind) -> Self {
        let own = kind.is_inline();
        Self {
            block: kind.block.filter(|_| !kind.removed),
            removed: kind.removed,
            name: own.then_some(kind.name),
            color: own.then_some(kind.color),
            material: kind.material.filter(|_| own),
            top: kind.top.filter(|_| own),
            bottom: kind.bottom.filter(|_| own),
            water: own && kind.water,
            cover: kind.cover.filter(|_| !kind.removed),
        }
    }
}

/// Where a kind of block covers generated ground: where its climate and
/// height are within these ranges (a range left out is any), and how many
/// blocks deep. Later kinds in the list cover those before them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockCover {
    /// In °C.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<[f32; 2]>,
    /// From 0 for dry to 1 for wet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub humidity: Option<[f32; 2]>,
    /// In metres in the generator's world.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<[f32; 2]>,
    pub depth: u32,
}

impl Default for BlockCover {
    fn default() -> Self {
        Self { temperature: None, humidity: None, height: None, depth: 1 }
    }
}

impl BlockCover {
    /// Whether it covers ground `height` metres up, `temperature` °C and
    /// `humidity` wet.
    pub fn covers(&self, height: f32, (temperature, humidity): (f32, f32)) -> bool {
        let within = |range: Option<[f32; 2]>, v: f32| range.is_none_or(|[low, high]| (low..=high).contains(&v));
        within(self.temperature, temperature) && within(self.humidity, humidity) && within(self.height, height)
    }
}

impl Default for BlockKind {
    fn default() -> Self {
        Self {
            block: None,
            removed: false,
            name: "Block".into(),
            color: [0.6, 0.6, 0.6],
            material: None,
            top: None,
            bottom: None,
            cover: None,
            water: false,
            cutout: false,
            sways: false,
            connected: 0,
            overlap: 0,
        }
    }
}

impl BlockKind {
    /// A block of a colour alone.
    pub fn colored(name: &str, color: [f32; 3]) -> Self {
        Self { name: name.into(), color, ..Self::default() }
    }

    /// An entry naming the `.block` at `reference`.
    pub fn of(reference: &str) -> Self {
        Self { block: Some(reference.into()), ..Self::default() }
    }

    /// The place a block was taken out of.
    pub fn gap() -> Self {
        Self { removed: true, ..Self::default() }
    }

    /// Whether it is a block, rather than the place one was taken out of.
    pub fn is_live(&self) -> bool {
        !self.removed
    }

    /// Whether it is a kind of the world's own, its look kept in the
    /// world's file.
    pub fn is_inline(&self) -> bool {
        !self.removed && self.block.is_none()
    }
}

/// Flat ground of blocks: `size` blocks along x and z from the world's
/// corner, `height` blocks up, its top layer of one block, `under_depth`
/// layers of another under it, and the rest of a third.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ground {
    pub size: [u32; 2],
    pub height: u32,
    pub top: BlockId,
    pub under: BlockId,
    pub under_depth: u32,
    pub bottom: BlockId,
}

impl Default for Ground {
    fn default() -> Self {
        Self { size: [64, 64], height: 8, top: 1, under: 2, under_depth: 3, bottom: 3 }
    }
}

impl Default for BlockFile {
    fn default() -> Self {
        Self {
            block_size: 1.0,
            blocks: vec![
                BlockKind::colored("Grass", [0.33, 0.52, 0.2]),
                BlockKind::colored("Dirt", [0.45, 0.32, 0.2]),
                BlockKind::colored("Stone", [0.5, 0.5, 0.52]),
            ],
            ground: Ground::default(),
            generate: None,
            smooth: false,
            voxels: 0,
            relief: USUAL_RELIEF,
        }
    }
}

/// How one face is drawn: with a material, or else its colour; whether its
/// material's pictures are the five tiles of a face that joins those beside
/// it; and whether it is a look's twin, drawn the same but for another
/// block's picture over its edge (see `mesh`).
#[derive(Clone, Debug, PartialEq)]
pub struct FaceLook {
    pub material: Option<String>,
    pub color: [f32; 3],
    pub connected: bool,
    pub mixed: bool,
}

/// A key a file's generated ground may still hold, from when ground could
/// lie all round a middle: left unread, so the file reads as the flat
/// ground it otherwise says.
const OLD_ROUND: &str = "planet";

impl BlockFile {
    pub fn parse(text: &str) -> Result<Self> {
        let mut file: Self = match text.contains(OLD_ROUND) {
            false => toml::from_str(text)?,
            true => {
                let mut table: toml::Table = toml::from_str(text)?;
                if let Some(toml::Value::Table(generate)) = table.get_mut("generate") {
                    generate.remove(OLD_ROUND);
                }
                table.try_into()?
            }
        };
        ensure!(file.block_size > 0.0 && file.block_size.is_finite(), "a block is some size");
        file.block_size = file.block_size.clamp(SMALLEST_BLOCK, LARGEST_BLOCK);
        ensure!(file.blocks.len() <= MAX_BLOCKS, "a world numbers at most {MAX_BLOCKS} blocks");
        // An entry is one thing: a place left empty, a block (its look its asset's) or a kind of its own.
        for kind in &mut file.blocks {
            if kind.removed {
                *kind = BlockKind::gap();
            } else if kind.block.is_some() {
                *kind = BlockKind { block: kind.block.take(), cover: kind.cover, ..BlockKind::default() };
            }
        }
        file.forget_gaps();
        Ok(file)
    }

    /// Whether `block` is one of its blocks: an entry of its palette not taken
    /// out.
    pub fn lists(&self, block: BlockId) -> bool {
        usize::from(block).checked_sub(1).and_then(|at| self.blocks.get(at)).is_some_and(BlockKind::is_live)
    }

    /// The block numbered `block`'s entry, where it has one.
    pub fn kind(&self, block: BlockId) -> Option<&BlockKind> {
        usize::from(block).checked_sub(1).and_then(|at| self.blocks.get(at)).filter(|kind| kind.is_live())
    }

    /// Makes its ground of air wherever it names a block taken out, so no
    /// block is ever made with an empty place's number.
    pub fn forget_gaps(&mut self) {
        let gone: Vec<bool> = self.blocks.iter().map(|kind| kind.removed).collect();
        let kept = |block: &mut BlockId| {
            if usize::from(*block).checked_sub(1).and_then(|at| gone.get(at)).copied().unwrap_or(false) {
                *block = AIR;
            }
        };
        kept(&mut self.ground.top);
        kept(&mut self.ground.under);
        kept(&mut self.ground.bottom);
        if let Some(generate) = &mut self.generate {
            kept(&mut generate.sea);
        }
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a block world's file writes as TOML")
    }

    /// The file with its palette cut after the last entry its ground can be
    /// made of: one its ground or its sea names, or one that covers. A
    /// block numbered after those changes nothing of what its ground is, so
    /// a world's ground is made from this, and is not made again as the
    /// world comes to hold more blocks (see `stream::BlockMaking`).
    pub fn for_ground(&self) -> BlockFile {
        let sea = self.generate.as_ref().map_or(AIR, |generate| generate.sea);
        let named = [self.ground.top, self.ground.under, self.ground.bottom, sea].map(usize::from);
        let covering = self.blocks.iter().rposition(|kind| kind.cover.is_some()).map_or(0, |at| at + 1);
        let mut file = self.clone();
        file.blocks.truncate(named.into_iter().fold(covering, usize::max));
        file
    }

    /// Makes its blocks `size` metres a side with its ground as large in
    /// metres as it was: what it counts in blocks (its ground's size, height
    /// and under layers, its lowest block, how deep its kinds cover) is
    /// counted again in blocks of the new size. Between `SMALLEST_BLOCK` and
    /// `LARGEST_BLOCK`, and no smaller than keeps its ground within
    /// `MOST_ACROSS` blocks a side; gives the size it took.
    pub fn set_block_size(&mut self, size: f32) -> f32 {
        let widest = self.ground.size.into_iter().max().unwrap_or(0).max(self.ground.height) as f32;
        let size = if size.is_nan() { self.block_size } else { size.clamp(SMALLEST_BLOCK, LARGEST_BLOCK) };
        let size = size.max(self.block_size * widest / MOST_ACROSS as f32);
        let ratio = self.block_size / size;
        let count = |n: u32| if n == 0 { 0 } else { ((n as f32 * ratio).round() as u32).max(1) };
        self.ground.size = self.ground.size.map(count);
        self.ground.height = count(self.ground.height);
        self.ground.under_depth = count(self.ground.under_depth);
        if let Some(generate) = &mut self.generate {
            generate.bottom = (generate.bottom as f32 * ratio).round() as i32;
        }
        for cover in self.blocks.iter_mut().filter_map(|kind| kind.cover.as_mut()) {
            cover.depth = count(cover.depth);
        }
        self.block_size = size;
        size
    }

    /// The ways its faces are drawn, each once, and for each block by its
    /// number (air first) which of them its top, its sides and its bottom
    /// are and whether it is water: the surfaces a chunk's quads name. A
    /// place a block was taken out of has no look. Its entries naming
    /// blocks are drawn as they stand here, so fill them in first (see
    /// `block::BlockDef::fill`).
    ///
    /// Where a block of it creeps over those beside it (an `overlap` over
    /// 0), every look has a twin [`MIXED_LOOK`] after it, drawn the same
    /// but for another block's picture over its edge, and each block says
    /// which of its faces can take one (`BlockFaces::mixes`). The looks
    /// between are fillers no face names. A world of more looks than
    /// `MIXED_LOOK` has no room for twins, and its blocks keep to
    /// themselves.
    pub fn faces(&self) -> (Vec<FaceLook>, Vec<BlockFaces>) {
        let (mut looks, mut faces) = self.own_faces();
        let twins = usize::from(MIXED_LOOK);
        let creeps = self.blocks.iter().any(|kind| kind.is_live() && !kind.water && kind.overlap > 0);
        if !creeps || looks.len() > twins {
            faces.iter_mut().for_each(|faces| faces.mixes = 0);
            return (looks, faces);
        }
        let own = looks.clone();
        let filler = FaceLook { material: None, color: [0.6; 3], connected: false, mixed: false };
        looks.resize(twins, filler);
        looks.extend(own.into_iter().map(|look| FaceLook { mixed: true, ..look }));
        (looks, faces)
    }

    /// Its looks without their twins, and each block's faces (see
    /// [`faces`](Self::faces)).
    fn own_faces(&self) -> (Vec<FaceLook>, Vec<BlockFaces>) {
        let mut looks: Vec<FaceLook> = Vec::new();
        let mut find = |material: &Option<String>, color: [f32; 3], connected: bool| {
            // A face with no picture has nothing to join with.
            let connected = connected && material.is_some();
            let look = FaceLook { material: material.clone(), color, connected, mixed: false };
            let at = looks.iter().position(|kept| *kept == look).unwrap_or_else(|| {
                looks.push(look);
                looks.len() - 1
            });
            at.min(255) as u8
        };
        let mut faces = vec![BlockFaces::default()];
        for kind in &self.blocks {
            if kind.removed {
                faces.push(BlockFaces::default());
                continue;
            }
            // Which of its faces join: each by the material it is drawn with.
            let joins = |own: &Option<String>, bit: u8| {
                let bit = if own.is_some() { bit } else { JOINS_SIDES };
                !kind.water && kind.connected & bit != 0 && (own.is_some() || kind.material.is_some())
            };
            let joined = [joins(&kind.top, JOINS_TOP), joins(&kind.material, JOINS_SIDES), joins(&kind.bottom, JOINS_BOTTOM)];
            let top = find(&kind.top.clone().or_else(|| kind.material.clone()), kind.color, joined[0]);
            let sides = find(&kind.material, kind.color, joined[1]);
            let bottom = find(&kind.bottom.clone().or_else(|| kind.material.clone()), kind.color, joined[2]);
            let color = kind.color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
            let (see_through, sways) = (kind.cutout && !kind.water, kind.sways && !kind.water);
            let connects = u8::from(joined[0]) * JOINS_TOP | u8::from(joined[1]) * JOINS_SIDES | u8::from(joined[2]) * JOINS_BOTTOM;
            // A face takes another block's picture over its edge where it is plain ground: not seen through, not
            // swaying, and not one that joins.
            let plain = !kind.water && !see_through && !sways;
            let takes = |joined: bool, bit: u8| if plain && !joined { bit } else { 0 };
            let mixes = takes(joined[0], JOINS_TOP) | takes(joined[1], JOINS_SIDES) | takes(joined[2], JOINS_BOTTOM);
            let overlap = if kind.water { 0 } else { kind.overlap };
            let drawn = BlockFaces::new(top, sides, bottom);
            faces.push(BlockFaces { water: kind.water, see_through, sways, connects, mixes, overlap, color, ..drawn });
        }
        (looks, faces)
    }

    /// How many ways of drawing a face it has, of the `MAX_LOOKS` it can,
    /// their twins not counted.
    pub fn looks(&self) -> usize {
        self.own_faces().0.len()
    }

    /// The blocks its ground starts as: made by `generator` where it is
    /// generated and the generator could be found, else flat. Blocks naming
    /// no kind it has are left out.
    pub fn make_ground(&self, generator: Option<&crate::generate::WorldGen>) -> BlockMap {
        if let (Some(generate), Some(world)) = (&self.generate, generator) {
            return super::generate::generate(self, generate, world);
        }
        self.flat_ground()
    }

    /// Its flat ground's blocks.
    fn flat_ground(&self) -> BlockMap {
        let mut map = BlockMap::default();
        let known = |block: BlockId| if self.lists(block) { block } else { AIR };
        let ground = &self.ground;
        for y in 0..ground.height {
            let block = if y + 1 == ground.height {
                ground.top
            } else if y + 1 + ground.under_depth >= ground.height {
                ground.under
            } else {
                ground.bottom
            };
            let block = known(block);
            if block == AIR {
                continue;
            }
            for z in 0..ground.size[1] {
                for x in 0..ground.size[0] {
                    map.set([x as i32, y as i32, z as i32], block);
                }
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_worlds_file_reads_as_written_and_names_each_face_look_once() {
        let mut file = BlockFile::default();
        file.blocks[0].top = Some("Materials/Grass.material".into());
        let text = file.to_text();
        assert_eq!(BlockFile::parse(&text).unwrap(), file);
        let (looks, faces) = file.faces();
        // Grass's top its material, its sides and bottom its colour; dirt and stone a colour each.
        assert_eq!(looks.len(), 4);
        assert_eq!(faces.len(), 4, "air, then each block");
        assert_eq!(looks[faces[1].top as usize].material.as_deref(), Some("Materials/Grass.material"));
        assert_eq!(faces[1].sides, faces[1].bottom);
        assert_ne!(faces[2].sides, faces[3].sides);
        assert!(faces.iter().all(|faces| !faces.water));
        assert!(BlockFile::parse("block_size = 0.0").is_err());
        // Blocks no smaller than a tenth of a metre and no larger than ten, read or set.
        assert_eq!(BlockFile::parse("block_size = 0.01").unwrap().block_size, SMALLEST_BLOCK);
        assert_eq!(BlockFile::parse("block_size = 500.0").unwrap().block_size, LARGEST_BLOCK);
        let mut sized = BlockFile::default();
        assert_eq!(sized.set_block_size(0.0), SMALLEST_BLOCK);
        assert_eq!(sized.ground.size, [640, 640]);
        assert_eq!(sized.set_block_size(1000.0), LARGEST_BLOCK);
        assert_eq!(sized.ground.size, [6, 6]);
        // A kind of water reads back as water, and its faces say so.
        file.blocks.push(BlockKind { water: true, ..BlockKind::colored("Water", [0.04, 0.46, 0.33]) });
        let read = BlockFile::parse(&file.to_text()).unwrap();
        assert!(read.blocks[3].water && !read.blocks[0].water);
        assert!(read.faces().1[4].water);
    }

    #[test]
    fn small_cubes_are_written_only_where_a_world_has_them() {
        let plain = toml::to_string(&BlockFile::default()).unwrap();
        assert!(!plain.contains("voxels") && !plain.contains("relief"), "{plain}");
        let file = BlockFile { voxels: 8, ..BlockFile::default() };
        let written = toml::to_string(&file).unwrap();
        assert!(written.contains("voxels = 8") && !written.contains("relief"), "sunk as deep as usual: {written}");
        assert_eq!(toml::from_str::<BlockFile>(&written).unwrap(), file);
        let deep = BlockFile { voxels: 16, relief: 3.5, ..BlockFile::default() };
        assert_eq!(toml::from_str::<BlockFile>(&toml::to_string(&deep).unwrap()).unwrap(), deep);
    }

    #[test]
    fn an_older_block_worlds_file_reads_as_it_did_and_is_written_back_the_same() {
        // As worlds were written while they kept all their kinds themselves.
        let older = r#"block_size = 0.5

[[blocks]]
name = "Grass"
color = [0.33, 0.52, 0.2]
material = "Materials/Dirt.material"
top = "Materials/Grass.material"

[[blocks]]
name = "Sand"
color = [0.8, 0.7, 0.4]

[blocks.cover]
height = [-200.0, 3.0]
depth = 3

[[blocks]]
name = "Water"
color = [0.04, 0.46, 0.33]
water = true

[ground]
size = [128, 96]
height = 8
top = 1
under = 2
under_depth = 3
bottom = 2
"#;
        let file = BlockFile::parse(older).unwrap();
        assert_eq!(file.blocks.len(), 3);
        assert!(file.blocks.iter().all(|kind| kind.is_inline()));
        assert_eq!(file.blocks[0].top.as_deref(), Some("Materials/Grass.material"));
        assert_eq!(file.blocks[1].cover.map(|cover| cover.depth), Some(3));
        assert!(file.blocks[2].water && file.faces().1[3].water);
        assert_eq!(file.to_text(), older, "nothing about it is written differently");
    }

    #[test]
    fn a_kind_taken_out_leaves_its_number_empty_and_the_rest_keep_theirs() {
        let mut file = BlockFile::default();
        file.blocks[0] = BlockKind { cover: Some(BlockCover::default()), ..BlockKind::of("Blocks/Grass.block") };
        file.blocks[1] = BlockKind::gap();
        let text = file.to_text();
        assert!(text.contains("block = \"Blocks/Grass.block\"") && text.contains("removed = true"), "{text}");
        assert_eq!(text.matches("name = ").count(), 1, "only stone, a kind of its own, says its look: {text}");
        // Read back: three entries still, the ground naming the empty place made air there.
        let read = BlockFile::parse(&text).unwrap();
        assert_eq!(read.blocks.len(), 3);
        assert_eq!(read.blocks[0], file.blocks[0]);
        assert!(read.blocks[1].removed && read.blocks[2].is_inline());
        assert_eq!((read.ground.top, read.ground.under, read.ground.bottom), (1, AIR, 3));
        assert!(read.lists(1) && !read.lists(2) && read.lists(3) && !read.lists(4) && !read.lists(AIR));
        assert_eq!(read.kind(3).map(|kind| kind.name.as_str()), Some("Stone"));
        // The place taken out has no look and is no ground; stone is still number 3.
        let (looks, faces) = read.faces();
        assert_eq!((looks.len(), faces.len()), (2, 4));
        assert_eq!(faces[2], BlockFaces::default());
        let map = read.make_ground(None);
        assert_eq!([7, 6, 4, 0].map(|y| map.get([0, y, 0])), [1, AIR, AIR, 3]);
        // An entry is one thing: a look written beside a block it names is the block's, not the file's.
        let both = "[[blocks]]\nblock = \"Blocks/Grass.block\"\nname = \"Mine\"\ncolor = [1.0, 0.0, 0.0]\n";
        assert_eq!(BlockFile::parse(both).unwrap().blocks[0], BlockKind::of("Blocks/Grass.block"));
    }

    #[test]
    fn a_block_numbered_after_those_its_ground_is_made_of_leaves_the_file_its_ground_is_made_from_as_it_was() {
        let mut file = BlockFile::default();
        file.blocks[1].cover = Some(BlockCover::default());
        file.ground = Ground { top: 1, under: 1, bottom: 2, ..Ground::default() };
        // Stone, the third, is no ground and covers nothing: the ground is made from grass and dirt alone.
        let made_from = file.for_ground();
        assert_eq!(made_from.blocks.len(), 2);
        assert_eq!(made_from.make_ground(None), file.make_ground(None));
        // The world comes to hold more blocks: its ground is made from the same file still.
        file.blocks.push(BlockKind::of("Blocks/Gold.block"));
        file.blocks.push(BlockKind::colored("Color 5", [1.0, 0.0, 0.0]));
        assert_eq!(file.for_ground(), made_from);
        // A block its ground names, its sea names or that covers is one its ground is made of.
        file.ground.bottom = 4;
        assert_eq!(file.for_ground().blocks.len(), 4);
        file.blocks[4].cover = Some(BlockCover::default());
        assert_eq!(file.for_ground(), file);
        let mut sea = BlockFile { generate: Some(Default::default()), ..BlockFile::default() };
        sea.ground = Ground { top: 1, under: 1, bottom: 1, ..Ground::default() };
        sea.generate.as_mut().unwrap().sea = 3;
        assert_eq!(sea.for_ground().blocks.len(), 3);
        sea.generate.as_mut().unwrap().sea = AIR;
        assert_eq!(sea.for_ground().blocks.len(), 1);
    }

    #[test]
    fn a_file_whose_generated_ground_names_a_planet_reads_as_flat_ground() {
        // As a world was written while its ground could lie all round a middle.
        let older = r#"block_size = 1.0

[[blocks]]
name = "Planet Rock"
color = [0.5, 0.5, 0.5]

[ground]
size = [48, 48]
height = 8
top = 1
under = 1
under_depth = 3
bottom = 1

[generate]
world = "0123456789abcdef0123456789abcdef"
position = [0.0, 0.0]
bottom = -16
overhangs = 0.0
caves = 0.3

[generate.planet]
radius = 60.0
"#;
        let file = BlockFile::parse(older).expect("an old file still reads");
        let now = BlockFile::parse(&older.replace("\n[generate.planet]\nradius = 60.0\n", "")).unwrap();
        assert_eq!(file, now, "as the same file with no planet");
        assert_eq!(file.blocks[0].name, "Planet Rock");
        assert!(!file.to_text().contains("[generate.planet]"), "and is written back without one");
        // Its ground is its generator's, flat: nothing below its lowest block, and the same all along its edge.
        let world = crate::generate::Preset::RollingHills.world();
        let map = file.make_ground(Some(&world));
        assert_eq!(map, now.make_ground(Some(&world)));
        assert!((0..48).all(|x| map.get([x, -16, 0]) != AIR && map.get([x, -17, 0]) == AIR));
        // Written inline it reads the same; any other key it does not know is still an error.
        let inline = older.replace("\n[generate.planet]\nradius = 60.0\n", "planet = { radius = 60.0 }\n");
        assert_eq!(BlockFile::parse(&inline).unwrap(), now);
        assert!(BlockFile::parse(&inline.replace("planet = {", "moon = {")).is_err());
    }

    #[test]
    fn flat_ground_is_grass_over_dirt_over_stone() {
        let file =
            BlockFile { ground: Ground { size: [40, 3], height: 6, ..Ground::default() }, ..BlockFile::default() };
        let map = file.make_ground(None);
        assert_eq!([5, 4, 3, 2, 0].map(|y| map.get([39, y, 2])), [1, 2, 2, 2, 3]);
        assert_eq!((map.get([40, 5, 0]), map.get([0, 6, 0])), (AIR, AIR));
        assert_eq!(map.chunks().count(), 2, "40 blocks along x reach a second chunk");
    }
}
