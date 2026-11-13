//! Saplings that grow into trees: a block whose shape is a sapling, and
//! which names the plant it grows into (`block::BlockDef::grows`).
//!
//! A world keeps the saplings growing in it in a short list ([`Saplings`]):
//! where each stands, how long it has grown, how far grown it is, and how
//! long it has gone without light or room. As it grows it stands as the
//! same tree at a share of its size ([`stage_model`]): grown from the seed
//! its place gives, so every stage is the tree it will be, smaller. The
//! light at a place is the sky over it, a little less for each leaf over
//! it and none under ground ([`light_at`]); a stage has room where every
//! block of it is air, or the stage before it, or what a plant grows
//! through ([`has_room`]).
//!
//! What a world keeps of them is a few bytes a sapling ([`Saplings::to_bytes`]):
//! saved with its changes, and sent to a player who joins.

use voxear_ecs::Component;

use super::plant::{PlantDef, PlantModel};

/// How many blocks over a sapling are looked at for its light.
pub const SKY_REACH: i32 = 24;

/// How much light a leaf over a place takes, from 0 to 1.
pub const LEAF_SHADE: f32 = 0.2;

/// One sapling growing: where it stands, how many seconds it has grown,
/// the stage it stands at (0 for the sapling, `stages` for the whole
/// plant), and how many seconds it has gone without light or room.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sapling {
    pub at: [i32; 3],
    pub grown: f32,
    pub stage: u8,
    pub idle: f32,
}

/// The saplings growing in a world of blocks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saplings {
    pub list: Vec<Sapling>,
}

impl Component for Saplings {}

const MAGIC: &[u8; 4] = b"VXSP";

impl Saplings {
    /// As bytes: `VXSP`, how many, and each where it stands, then how many
    /// tenths of a second it has grown and has gone idle, and its stage:
    /// nineteen bytes a sapling.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.list.len() * 19);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(self.list.len() as u32).to_le_bytes());
        for sapling in &self.list {
            for v in sapling.at {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.extend_from_slice(&((sapling.grown * 10.0).round().clamp(0.0, u32::MAX as f32) as u32).to_le_bytes());
            bytes.extend_from_slice(&((sapling.idle * 10.0).round().clamp(0.0, u16::MAX as f32) as u16).to_le_bytes());
            bytes.push(sapling.stage);
        }
        bytes
    }

    /// Read back from [`to_bytes`](Self::to_bytes): none where they are not
    /// such bytes, and with how many bytes they took.
    pub fn from_bytes(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.get(..4)? != MAGIC {
            return None;
        }
        let count = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
        let end = 8usize.checked_add(count.checked_mul(19)?)?;
        let body = bytes.get(8..end)?;
        let list = body
            .chunks_exact(19)
            .map(|entry| {
                let word = |at: usize| i32::from_le_bytes(entry[at..at + 4].try_into().expect("four bytes"));
                Sapling {
                    at: [word(0), word(4), word(8)],
                    grown: u32::from_le_bytes(entry[12..16].try_into().expect("four bytes")) as f32 / 10.0,
                    idle: f32::from(u16::from_le_bytes(entry[16..18].try_into().expect("two bytes"))) / 10.0,
                    stage: entry[18],
                }
            })
            .collect();
        Some((Self { list }, end))
    }

    /// The sapling standing at `at`, where one is.
    pub fn at(&self, at: [i32; 3]) -> Option<&Sapling> {
        self.list.iter().find(|sapling| sapling.at == at)
    }
}

/// The seed a sapling standing at `at` grows from: the same wherever it is
/// grown, so a tree grown again is the same tree.
pub fn seed_of(at: [i32; 3]) -> u32 {
    let mut h = (at[0] as u32).wrapping_mul(0x9E37_79B9) ^ (at[1] as u32).wrapping_mul(0x85EB_CA77);
    h ^= (at[2] as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 13)
}

/// `plant` grown to `share` of its size: as tall, as thick, as wide, with
/// clumps of leaves big enough to reach past the wood they grow round.
pub fn young(plant: &PlantDef, share: f32) -> PlantDef {
    let share = share.clamp(0.05, 1.0);
    let scaled = |[from, to]: [f32; 2]| [(from * share).max(1.0), (to * share).max(1.0)];
    PlantDef {
        height: scaled(plant.height),
        trunk: (plant.trunk * share).max(0.3),
        crown_width: scaled(plant.crown_width),
        crown_height: scaled(plant.crown_height),
        leaf_size: (plant.leaf_size * share.sqrt()).max(2.4),
        roots: 0,
        vines: if share < 1.0 { 0.0 } else { plant.vines },
        ..plant.clone()
    }
}

/// The blocks of a sapling of `plant` standing at `at` at stage `stage`,
/// from its foot, none under it: the whole plant at its last stage.
pub fn stage_model(plant: &PlantDef, at: [i32; 3], stage: u8) -> PlantModel {
    let stages = plant.stages.max(1);
    let share = f32::from(stage) / stages as f32;
    let mut model = if u32::from(stage) >= stages { plant.clone() } else { young(plant, share) }.grow(seed_of(at));
    model.blocks.retain(|(at, _, _)| at[1] >= 0);
    model
}

/// How much light falls on the block at `at`, from 0 to 1: the sky's, a
/// little less for each leaf over it, none once any other block of ground
/// is over it within [`SKY_REACH`]; `over` says what each block over it is.
pub fn light_at(over: &dyn Fn([i32; 3]) -> Shade, at: [i32; 3]) -> f32 {
    let mut light = 1.0f32;
    for up in 1..=SKY_REACH {
        match over([at[0], at[1] + up, at[2]]) {
            Shade::Open => {}
            Shade::Leaf => light -= LEAF_SHADE,
            Shade::Ground => return 0.0,
        }
        if light <= 0.0 {
            return 0.0;
        }
    }
    light
}

/// What a block over a sapling does to its light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    Open,
    Leaf,
    Ground,
}

/// Whether a stage of a tree, its blocks each where it would stand, has
/// room: `free` says of each place whether it is air or what a plant grows
/// through, or a block of the stage before.
pub fn has_room(blocks: &[[i32; 3]], free: &dyn Fn([i32; 3]) -> bool) -> bool {
    blocks.iter().all(|&at| free(at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::plant::PlantPart;

    #[test]
    fn saplings_are_nineteen_bytes_each_and_read_back() {
        let saplings = Saplings {
            list: vec![
                Sapling { at: [10, 64, -3], grown: 125.5, stage: 1, idle: 0.0 },
                Sapling { at: [-400, 70, 12_000], grown: 0.0, stage: 0, idle: 31.2 },
            ],
        };
        let bytes = saplings.to_bytes();
        assert_eq!(bytes.len(), 8 + 2 * 19);
        let (back, used) = Saplings::from_bytes(&bytes).unwrap();
        assert_eq!((back, used), (saplings, bytes.len()));
        assert!(Saplings::from_bytes(&bytes[..bytes.len() - 1]).is_none());
        assert!(Saplings::from_bytes(b"VXBT").is_none());
        assert_eq!(Saplings::default().to_bytes().len(), 8);
    }

    #[test]
    fn a_sapling_grows_into_the_same_tree_smaller_at_each_stage_and_the_whole_tree_at_its_last() {
        let oak = PlantDef::default();
        let at = [5, 20, 9];
        let sizes: Vec<(usize, i32)> = (1..=3)
            .map(|stage| {
                let model = stage_model(&oak, at, stage);
                assert!(model.blocks.iter().all(|(at, _, _)| at[1] >= 0), "none under its foot");
                (model.blocks.len(), model.bounds().unwrap().1[1])
            })
            .collect();
        assert!(sizes.windows(2).all(|pair| pair[1].0 > pair[0].0 && pair[1].1 > pair[0].1), "{sizes:?}");
        // Its last stage is the whole tree the seed of its place grows, but its roots.
        let whole = oak.grow(seed_of(at));
        let over: Vec<_> = whole.blocks.iter().filter(|(at, _, _)| at[1] >= 0).cloned().collect();
        assert_eq!(stage_model(&oak, at, 3).blocks, over);
        // Its first is small: a trunk of a few blocks and some leaves, however small its plant's clumps are.
        for leaf_size in [1.5, 2.6, 4.0] {
            let first = stage_model(&PlantDef { leaf_size, ..oak.clone() }, at, 1);
            assert!(first.count(PlantPart::Trunk) >= 2 && first.count(PlantPart::Leaf) >= 8, "{leaf_size}: {first:?}");
        }
        assert_ne!(seed_of(at), seed_of([5, 20, 10]));
    }

    #[test]
    fn light_is_the_sky_less_for_each_leaf_over_and_none_under_ground() {
        let open = |_: [i32; 3]| Shade::Open;
        assert_eq!(light_at(&open, [0, 0, 0]), 1.0);
        let crown = |at: [i32; 3]| if (5..8).contains(&at[1]) { Shade::Leaf } else { Shade::Open };
        assert!((light_at(&crown, [0, 0, 0]) - 0.4).abs() < 1e-5);
        let cave = |at: [i32; 3]| if at[1] == 10 { Shade::Ground } else { Shade::Open };
        assert_eq!(light_at(&cave, [0, 0, 0]), 0.0);
        // Far over it, past what is looked at, is open sky.
        assert_eq!(light_at(&cave, [0, -30, 0]), 1.0);
        assert!(has_room(&[[0, 1, 0], [0, 2, 0]], &|at| at[1] < 3));
        assert!(!has_room(&[[0, 1, 0], [0, 3, 0]], &|at| at[1] < 3));
    }
}
