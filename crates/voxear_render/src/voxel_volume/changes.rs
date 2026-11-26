//! Voxels of an endless Voxel Volume dug out or placed one at a time
//! ([`VoxelChanges`]), over everything its edits make: what a game's
//! digging and building leaves.
//!
//! They are kept brick by brick, eight voxels a side: for each brick with a
//! voxel set, which of its voxels are set and what each is, air or one of
//! the volume's kinds of placed voxel (a ground or paving, and a tint). A
//! brick is shared between copies until it changes, so a volume copied for
//! undo or sent to the renderer costs nothing, and only the bricks that
//! changed are made again.
//!
//! They are saved in region files ([`VoxelChanges::regions`]), sixteen by
//! sixteen sector columns to a file, each brick its place, a bit a voxel
//! and a byte a voxel set, packed with zstd: a brick dug or built a voxel at
//! a time costs at most 600 bytes before it is packed, and a region with a
//! few hundred voxels changed under a kilobyte.

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, bail, ensure};
use voxear_core::math::IVec3;

use super::{VoxelHeld, VoxelMaterial};

/// How many voxels a brick is a side, and a region's sector columns a side,
/// in bricks.
const BRICK: i32 = 8;
pub const REGION_BRICKS: i32 = 128;
/// The most kinds of placed voxel a volume keeps: a voxel's code is a byte,
/// 0 for air.
pub const MOST_KINDS: usize = 255;

/// What a region file starts with, and its version.
const MAGIC: &[u8; 4] = b"VXRG";
const VERSION: u32 = 1;

/// A kind of voxel placed: its ground or paving, and its tint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VoxelKind {
    pub material: VoxelMaterial,
    pub tint: Option<[u8; 3]>,
}

/// A brick's voxels set: a bit each, and each one's code, 0 for air, else
/// its kind's place among the volume's kinds and one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedBrick {
    pub set: [u64; 8],
    pub codes: Box<[u8; 512]>,
}

impl Default for ChangedBrick {
    fn default() -> Self {
        Self { set: [0; 8], codes: Box::new([0; 512]) }
    }
}

impl ChangedBrick {
    fn index(local: IVec3) -> usize {
        (local.x + BRICK * (local.y + BRICK * local.z)) as usize
    }

    fn is_set(&self, index: usize) -> bool {
        self.set[index >> 6] >> (index & 63) & 1 != 0
    }

    fn is_empty(&self) -> bool {
        self.set.iter().all(|&word| word == 0)
    }

    /// How many of its voxels are set.
    pub fn count(&self) -> u32 {
        self.set.iter().map(|word| word.count_ones()).sum()
    }
}

/// The voxels of an endless volume set one at a time: dug out (air) or
/// placed (a kind), over what its ground and edits make.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelChanges {
    kinds: Vec<VoxelKind>,
    bricks: BTreeMap<[i32; 3], Arc<ChangedBrick>>,
}

/// What a voxel is set to: dug out, or placed.
pub type VoxelSet = Option<VoxelKind>;

impl VoxelChanges {
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty()
    }

    /// Its kinds of placed voxel, a voxel's code less one naming its place.
    pub fn kinds(&self) -> &[VoxelKind] {
        &self.kinds
    }

    /// Each brick with a voxel set, by its place in bricks from the volume's
    /// corner, in order.
    pub fn bricks(&self) -> impl Iterator<Item = (&[i32; 3], &Arc<ChangedBrick>)> {
        self.bricks.iter()
    }

    /// How many voxels are set.
    pub fn count(&self) -> u64 {
        self.bricks.values().map(|brick| u64::from(brick.count())).sum()
    }

    fn split(cell: IVec3) -> ([i32; 3], IVec3) {
        let brick = cell.to_array().map(|a| a.div_euclid(BRICK));
        (brick, IVec3::from(cell.to_array().map(|a| a.rem_euclid(BRICK))))
    }

    /// The code of `kind`, added where it is new: none where there are
    /// [`MOST_KINDS`] already.
    fn code_of(&mut self, kind: VoxelKind) -> Option<u8> {
        if let Some(at) = self.kinds.iter().position(|k| *k == kind) {
            return Some(at as u8 + 1);
        }
        if self.kinds.len() >= MOST_KINDS {
            return None;
        }
        self.kinds.push(kind);
        Some(self.kinds.len() as u8)
    }

    /// Sets voxel `cell` (in voxels from the volume's corner) to `to`: dug
    /// out for none, else placed. Gives whether it was set: not where the
    /// volume has every kind it can keep and `to` is a new one.
    pub fn set(&mut self, cell: IVec3, to: VoxelSet) -> bool {
        let code = match to {
            None => 0,
            Some(kind) => match self.code_of(kind) {
                Some(code) => code,
                None => return false,
            },
        };
        let (brick, local) = Self::split(cell);
        let index = ChangedBrick::index(local);
        let changed = Arc::make_mut(self.bricks.entry(brick).or_default());
        changed.set[index >> 6] |= 1 << (index & 63);
        changed.codes[index] = code;
        true
    }

    /// Lets voxel `cell` go, back to what the ground and the edits make.
    pub fn unset(&mut self, cell: IVec3) {
        let (brick, local) = Self::split(cell);
        let index = ChangedBrick::index(local);
        let Some(changed) = self.bricks.get_mut(&brick) else { return };
        if !changed.is_set(index) {
            return;
        }
        let changed = Arc::make_mut(changed);
        changed.set[index >> 6] &= !(1 << (index & 63));
        changed.codes[index] = 0;
        if changed.is_empty() {
            self.bricks.remove(&brick);
        }
    }

    /// What voxel `cell` is set to: none where it is not set.
    pub fn get(&self, cell: IVec3) -> Option<VoxelHeld> {
        let (brick, local) = Self::split(cell);
        let changed = self.bricks.get(&brick)?;
        let index = ChangedBrick::index(local);
        if !changed.is_set(index) {
            return None;
        }
        Some(match changed.codes[index] {
            0 => VoxelHeld::Air,
            code => VoxelHeld::Fill(self.kinds[usize::from(code) - 1].material),
        })
    }

    /// Whether any voxel is set in the brick at `brick`.
    pub fn has_brick(&self, brick: [i32; 3]) -> bool {
        self.bricks.contains_key(&brick)
    }

    /// Whether any voxel placed (not dug) is in a brick of the column of
    /// bricks at (`x`, `z`), within `reach` bricks each way across.
    pub fn placed_near(&self, x: i32, z: i32, reach: i32) -> Option<f32> {
        let mut nearest: Option<f32> = None;
        for (brick, changed) in &self.bricks {
            let (dx, dz) = ((brick[0] - x).abs(), (brick[2] - z).abs());
            if dx > reach || dz > reach || !changed.codes.iter().any(|&code| code != 0) {
                continue;
            }
            let across = ((dx - 1).max(0) as f32).hypot((dz - 1).max(0) as f32) * BRICK as f32;
            nearest = Some(nearest.map_or(across, |n| n.min(across)));
        }
        nearest
    }

    /// The bricks, least and most corners in voxels, where these and `older`
    /// differ.
    pub fn changed_from(&self, older: &Self) -> Vec<([i32; 3], [i32; 3])> {
        let same_kinds = self.kinds == older.kinds;
        let mut boxes = Vec::new();
        let mut note = |brick: &[i32; 3]| {
            let low = brick.map(|a| a * BRICK);
            boxes.push((low, low.map(|a| a + BRICK - 1)));
        };
        for (brick, changed) in &self.bricks {
            let same =
                older.bricks.get(brick).is_some_and(|old| (Arc::ptr_eq(old, changed) || old == changed) && same_kinds);
            if !same {
                note(brick);
            }
        }
        for brick in older.bricks.keys().filter(|brick| !self.bricks.contains_key(*brick)) {
            note(brick);
        }
        boxes
    }

    /// Its bricks by the region file each is kept in: the region's place,
    /// in regions along X and Z, and its file's bytes.
    pub fn regions(&self) -> Vec<([i32; 2], Vec<u8>)> {
        let mut by: BTreeMap<[i32; 2], Vec<([i32; 3], &ChangedBrick)>> = BTreeMap::new();
        for (brick, changed) in &self.bricks {
            let region = [brick[0].div_euclid(REGION_BRICKS), brick[2].div_euclid(REGION_BRICKS)];
            by.entry(region).or_default().push((*brick, changed));
        }
        by.into_iter().map(|(region, bricks)| (region, self.region_bytes(region, &bricks))).collect()
    }

    /// A region file: its magic and version, where it is, its kinds (a
    /// ground or paving a byte, whether tinted, and its tint), how many
    /// bricks, then packed with zstd each brick's place in the region (X and
    /// Z a byte, Y two), its voxels' bits and the codes of those set.
    fn region_bytes(&self, region: [i32; 2], bricks: &[([i32; 3], &ChangedBrick)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&region[0].to_le_bytes());
        out.extend_from_slice(&region[1].to_le_bytes());
        out.push(self.kinds.len() as u8);
        for kind in &self.kinds {
            out.push(kind.material as u8);
            out.push(u8::from(kind.tint.is_some()));
            out.extend_from_slice(&kind.tint.unwrap_or_default());
        }
        out.extend_from_slice(&(bricks.len() as u32).to_le_bytes());
        let mut payload = Vec::new();
        for (brick, changed) in bricks {
            payload.push((brick[0] - region[0] * REGION_BRICKS) as u8);
            payload.extend_from_slice(&(brick[1] as i16).to_le_bytes());
            payload.push((brick[2] - region[1] * REGION_BRICKS) as u8);
            for word in changed.set {
                payload.extend_from_slice(&word.to_le_bytes());
            }
            payload.extend((0..512).filter(|&index| changed.is_set(index)).map(|index| changed.codes[index]));
        }
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&zstd::bulk::compress(&payload, 3).unwrap_or_default());
        out
    }

    /// Takes in a region file's bricks, over any it has there already; its
    /// kinds are added to these, the codes made these' own.
    pub fn read_region(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() >= 21 && &bytes[..4] == MAGIC, "not a region of voxel changes");
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        if word(4) != VERSION {
            bail!("a region of voxel changes of version {}, which this Voxear cannot read", word(4));
        }
        let region = [word(8) as i32, word(12) as i32];
        let kinds = usize::from(bytes[16]);
        let mut at = 17;
        let mut codes = vec![0u8; kinds + 1];
        for code in codes.iter_mut().skip(1) {
            let kind = bytes.get(at..at + 5).context("the region ends early")?;
            let material = *VoxelMaterial::ALL.get(usize::from(kind[0])).context("a voxel of no ground")?;
            let tint = (kind[1] != 0).then(|| [kind[2], kind[3], kind[4]]);
            *code = self.code_of(VoxelKind { material, tint }).context("too many kinds of voxel")?;
            at += 5;
        }
        let count = u32::from_le_bytes(bytes.get(at..at + 4).context("the region ends early")?.try_into()?) as usize;
        let length = u32::from_le_bytes(bytes.get(at + 4..at + 8).context("the region ends early")?.try_into()?);
        let payload = zstd::bulk::decompress(&bytes[at + 8..], length as usize)?;
        let mut p = 0;
        let mut take = |n: usize| -> Result<&[u8]> {
            let piece = payload.get(p..p + n).context("the region's bricks end early")?;
            p += n;
            Ok(piece)
        };
        for _ in 0..count {
            let place = take(4)?;
            let brick = [
                i32::from(place[0]) + region[0] * REGION_BRICKS,
                i32::from(i16::from_le_bytes([place[1], place[2]])),
                i32::from(place[3]) + region[1] * REGION_BRICKS,
            ];
            let mut changed = ChangedBrick::default();
            for word in changed.set.iter_mut() {
                *word = u64::from_le_bytes(take(8)?.try_into()?);
            }
            let set: Vec<usize> = (0..512).filter(|&index| changed.is_set(index)).collect();
            let read = take(set.len())?;
            for (&index, &code) in set.iter().zip(read) {
                changed.codes[index] = *codes.get(usize::from(code)).context("a voxel of no kind")?;
            }
            if !changed.is_empty() {
                self.bricks.insert(brick, Arc::new(changed));
            }
        }
        Ok(())
    }

    /// These made from region files' bytes.
    pub fn from_regions<'a>(regions: impl IntoIterator<Item = &'a [u8]>) -> Result<Self> {
        let mut changes = Self::default();
        for bytes in regions {
            changes.read_region(bytes)?;
        }
        Ok(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voxels_dug_and_placed_are_kept_and_read_back_from_their_regions() {
        let mut changes = VoxelChanges::default();
        let brick = VoxelKind { material: VoxelMaterial::Bricks, tint: Some([200, 60, 60]) };
        // A tunnel dug a voxel at a time, across two regions, and a wall built in it.
        for x in -40..40 {
            for y in 30..34 {
                assert!(changes.set(IVec3::new(x * 16, y, 5), None));
            }
        }
        for y in 30..34 {
            assert!(changes.set(IVec3::new(3, y, 6), Some(brick)));
        }
        assert_eq!(changes.get(IVec3::new(-16, 31, 5)), Some(VoxelHeld::Air));
        assert_eq!(changes.get(IVec3::new(3, 33, 6)), Some(VoxelHeld::Fill(VoxelMaterial::Bricks)));
        assert_eq!(changes.get(IVec3::new(3, 34, 6)), None, "not set");
        assert_eq!(changes.count(), 80 * 4 + 4);
        let regions = changes.regions();
        assert_eq!(regions.len(), 2, "on both sides of X 0");
        let bytes: usize = regions.iter().map(|(_, bytes)| bytes.len()).sum();
        assert!(bytes < 2000, "{bytes} bytes");
        let back = VoxelChanges::from_regions(regions.iter().map(|(_, bytes)| bytes.as_slice())).unwrap();
        assert_eq!(back, changes);
        // Let go, a voxel is what the ground makes again, and an empty brick is let go too.
        let mut fewer = changes.clone();
        for y in 30..34 {
            fewer.unset(IVec3::new(3, y, 6));
        }
        assert_eq!(fewer.get(IVec3::new(3, 31, 6)), None);
        let changed = fewer.changed_from(&changes);
        // The two bricks the wall was in, which keep the tunnel's voxels.
        assert_eq!(changed, vec![([0, 24, 0], [7, 31, 7]), ([0, 32, 0], [7, 39, 7])]);
    }
}
