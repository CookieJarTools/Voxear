//! A `.blockworld` file: what a world of blocks is, as text. How large a
//! block is, the kinds of block it has, each with its colour and the
//! materials its faces are drawn with, and the ground its blocks start as.
//!
//! ```toml
//! block_size = 1.0
//!
//! [[blocks]]
//! name = "Grass"
//! color = [0.33, 0.52, 0.2]
//! top = "Materials/Grass.material"      # its top, in place of its material
//! material = "Materials/Dirt.material"  # every other face
//!
//! [ground]
//! size = [64, 64]   # blocks along x and z
//! height = 8        # blocks up
//! top = 1           # blocks by their place in the list, from 1
//! under = 2
//! under_depth = 3
//! bottom = 3
//! ```

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    BlockMap,
    chunk::{AIR, BlockId},
};

/// The most kinds of block a world can have.
pub const MAX_BLOCKS: usize = 255;

/// What a `.blockworld` file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockFile {
    /// How many metres a block is a side.
    pub block_size: f32,
    /// The kinds of block, numbered from 1 in this order.
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
}

/// A kind of block: its name, its colour where its faces have no material,
/// and the materials it is drawn with, each a reference to a `.material`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlockKind {
    pub name: String,
    pub color: [f32; 3],
    /// Every face's material; none for its colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Its top's and bottom's, in place of its material.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bottom: Option<String>,
    /// Where generated ground is covered with it in place of its top block:
    /// none for nowhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<BlockCover>,
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
        Self { name: "Block".into(), color: [0.6, 0.6, 0.6], material: None, top: None, bottom: None, cover: None }
    }
}

impl BlockKind {
    /// A block of a colour alone.
    pub fn colored(name: &str, color: [f32; 3]) -> Self {
        Self { name: name.into(), color, ..Self::default() }
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
        }
    }
}

/// How one face is drawn: with a material, or else its colour.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceLook {
    pub material: Option<String>,
    pub color: [f32; 3],
}

impl BlockFile {
    pub fn parse(text: &str) -> Result<Self> {
        let mut file: Self = toml::from_str(text)?;
        // A planet is made all at once, never without end.
        if let Some(generate) = file.generate.as_mut().filter(|generate| generate.planet.is_some()) {
            generate.endless = None;
        }
        ensure!(file.block_size > 0.0 && file.block_size.is_finite(), "a block is some size");
        ensure!(file.blocks.len() <= MAX_BLOCKS, "a world has at most {MAX_BLOCKS} kinds of block");
        Ok(file)
    }

    pub fn to_text(&self) -> String {
        toml::to_string(self).expect("a block world's file writes as TOML")
    }

    /// The ways its faces are drawn, each once, and for each block by its
    /// number (air first) which of them its top, its sides and its bottom
    /// are: the surfaces a chunk's quads name.
    pub fn faces(&self) -> (Vec<FaceLook>, Vec<[u8; 3]>) {
        let mut looks: Vec<FaceLook> = Vec::new();
        let mut find = |material: &Option<String>, color: [f32; 3]| {
            let look = FaceLook { material: material.clone(), color };
            let at = looks.iter().position(|kept| *kept == look).unwrap_or_else(|| {
                looks.push(look);
                looks.len() - 1
            });
            at.min(255) as u8
        };
        let mut faces = vec![[0; 3]];
        for kind in &self.blocks {
            let top = find(&kind.top.clone().or_else(|| kind.material.clone()), kind.color);
            let sides = find(&kind.material, kind.color);
            let bottom = find(&kind.bottom.clone().or_else(|| kind.material.clone()), kind.color);
            faces.push([top, sides, bottom]);
        }
        (looks, faces)
    }

    /// The blocks its ground starts as: made by `generator` where it is
    /// generated and the generator could be found, else flat. Blocks naming
    /// no kind it has are left out.
    pub fn make_ground(&self, generator: Option<&crate::generate::WorldGen>) -> BlockMap {
        // A planet is round with or without its generator.
        if let Some((generate, planet)) = self.generate.as_ref().and_then(|g| Some((g, g.planet?))) {
            return super::generate::planet_ground(self, generate, generator, planet);
        }
        if let (Some(generate), Some(world)) = (&self.generate, generator) {
            return super::generate::generate(self, generate, world);
        }
        self.flat_ground()
    }

    /// Its flat ground's blocks.
    fn flat_ground(&self) -> BlockMap {
        let mut map = BlockMap::default();
        let known = |block: BlockId| if usize::from(block) <= self.blocks.len() { block } else { AIR };
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
        assert_eq!(looks[faces[1][0] as usize].material.as_deref(), Some("Materials/Grass.material"));
        assert_eq!(faces[1][1], faces[1][2]);
        assert_ne!(faces[2][1], faces[3][1]);
        assert!(BlockFile::parse("block_size = 0.0").is_err());
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
