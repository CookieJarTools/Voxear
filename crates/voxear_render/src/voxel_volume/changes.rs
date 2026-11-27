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
//!
//! Beside them they keep what was cut: the voxel columns whose grass, cover
//! or crops were cut ([`CutColumns`]), a byte each for how many voxels of
//! their blades are left under the cut, kept eight by eight columns as the
//! bricks are; and the trees felled ([`Felled`]), each by the voxel column
//! of its foot, with how high its stump stands. Both go in the same region
//! files.

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
const VERSION: u32 = 2;

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

    pub fn is_set(&self, index: usize) -> bool {
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

/// Eight by eight voxel columns whose grass, cover or crops were cut: for
/// each, how many voxels of its blades are left over its ground, what was
/// under the cut, from 0 (cut to the ground) to [`FULL`] (not cut).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutColumns {
    /// A bit a column cut, X fastest.
    pub set: u64,
    pub left: [u8; 64],
}

impl Default for CutColumns {
    fn default() -> Self {
        Self { set: 0, left: [FULL; 64] }
    }
}

/// How many voxels of a column's blades are left where none is cut: all.
pub const FULL: u8 = 255;

/// A tree felled, by the voxel column of its foot: nothing of it is left
/// from `cut` up (in voxels from the volume's floor), and its box, least
/// and most corners in voxels, is where it stood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Felled {
    pub cut: i32,
    pub low: [i32; 3],
    pub high: [i32; 3],
}

/// The voxels of an endless volume set one at a time: dug out (air) or
/// placed (a kind), over what its ground and edits make; and its cut grass
/// and felled trees.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelChanges {
    kinds: Vec<VoxelKind>,
    bricks: BTreeMap<[i32; 3], Arc<ChangedBrick>>,
    /// Cut columns, by the column of bricks they are in.
    cuts: BTreeMap<[i32; 2], Arc<CutColumns>>,
    felled: BTreeMap<[i32; 2], Felled>,
}

/// What one region file holds: its bricks, its cut columns' bricks and its
/// felled trees.
#[derive(Default)]
struct Region<'a> {
    bricks: Vec<([i32; 3], &'a ChangedBrick)>,
    cuts: Vec<([i32; 2], &'a CutColumns)>,
    felled: Vec<([i32; 2], &'a Felled)>,
}

/// What a voxel is set to: dug out, or placed.
pub type VoxelSet = Option<VoxelKind>;

impl VoxelChanges {
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty() && self.cuts.is_empty() && self.felled.is_empty()
    }

    /// Whether no voxel is set one at a time: none dug or placed.
    pub fn no_voxels(&self) -> bool {
        self.bricks.is_empty()
    }

    /// How many voxels of the blades of voxel column (`x`, `z`) are left
    /// over its ground: none where it was never cut, or has grown back.
    pub fn cut_left(&self, x: i32, z: i32) -> Option<u8> {
        let (brick, index) = Self::split_column(x, z);
        let cut = self.cuts.get(&brick)?;
        (cut.set >> index & 1 != 0).then_some(cut.left[index])
    }

    /// Each column of bricks with columns cut, in order.
    pub fn cuts(&self) -> impl Iterator<Item = (&[i32; 2], &Arc<CutColumns>)> {
        self.cuts.iter()
    }

    fn split_column(x: i32, z: i32) -> ([i32; 2], usize) {
        let brick = [x.div_euclid(BRICK), z.div_euclid(BRICK)];
        (brick, (x.rem_euclid(BRICK) + BRICK * z.rem_euclid(BRICK)) as usize)
    }

    /// Cuts the grass, cover or crops of voxel column (`x`, `z`) `keep`
    /// voxels over its ground: what is under that stays, the rest is gone.
    /// Gives whether that is lower than it was cut before.
    pub fn cut(&mut self, x: i32, z: i32, keep: u8) -> bool {
        let (brick, index) = Self::split_column(x, z);
        let cut = Arc::make_mut(self.cuts.entry(brick).or_default());
        let before = if cut.set >> index & 1 == 0 { FULL } else { cut.left[index] };
        cut.set |= 1 << index;
        cut.left[index] = before.min(keep);
        keep < before
    }

    /// Grows the cut columns back by `grow` voxels, letting go of those
    /// grown to `tallest`, as tall as anything grows there. Gives whether
    /// any changed.
    pub fn tend_cuts(&mut self, grow: u8, tallest: u8) -> bool {
        if grow == 0 {
            return false;
        }
        let changed = !self.cuts.is_empty();
        self.cuts.retain(|_, cut| {
            let cut = Arc::make_mut(cut);
            let set = cut.set;
            for index in (0..64).filter(|&index| set >> index & 1 != 0) {
                cut.left[index] = cut.left[index].saturating_add(grow);
                if cut.left[index] >= tallest {
                    (cut.set, cut.left[index]) = (cut.set & !(1 << index), FULL);
                }
            }
            cut.set != 0
        });
        changed
    }

    /// Whether any column is cut.
    pub fn any_cut(&self) -> bool {
        !self.cuts.is_empty()
    }

    /// The tree whose foot is on voxel column `foot`, where it was felled.
    pub fn felled(&self, foot: [i32; 2]) -> Option<Felled> {
        self.felled.get(&foot).copied()
    }

    /// Each tree felled, by the column of its foot.
    pub fn all_felled(&self) -> impl Iterator<Item = (&[i32; 2], &Felled)> {
        self.felled.iter()
    }

    /// Fells the tree whose foot is on `foot`: nothing of it left from its
    /// `cut` up, lower than it was felled before.
    pub fn fell(&mut self, foot: [i32; 2], felled: Felled) {
        let kept = self.felled.entry(foot).or_insert(felled);
        kept.cut = kept.cut.min(felled.cut);
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

    /// The code voxel `cell` is set to, its kind's place and one, 0 for air
    /// or where it is not set.
    pub fn code(&self, cell: IVec3) -> u8 {
        let (brick, local) = Self::split(cell);
        self.bricks.get(&brick).map_or(0, |changed| changed.codes[ChangedBrick::index(local)])
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
        // A tree felled, or let stand again: its box.
        for (foot, felled) in &self.felled {
            if older.felled.get(foot) != Some(felled) {
                boxes.push((felled.low, felled.high));
            }
        }
        for (foot, felled) in &older.felled {
            if !self.felled.contains_key(foot) {
                boxes.push((felled.low, felled.high));
            }
        }
        boxes
    }

    /// The columns of bricks, least and most voxel columns along X and Z,
    /// whose cut grass differs from `older`'s: which leaves every voxel as
    /// solid as it was.
    pub fn cuts_changed_from(&self, older: &Self) -> Vec<([i32; 2], [i32; 2])> {
        let mut boxes = Vec::new();
        let mut note = |brick: &[i32; 2]| {
            let low = brick.map(|a| a * BRICK);
            boxes.push((low, low.map(|a| a + BRICK - 1)));
        };
        for (brick, cut) in &self.cuts {
            if older.cuts.get(brick).is_none_or(|old| !(Arc::ptr_eq(old, cut) || old == cut)) {
                note(brick);
            }
        }
        for brick in older.cuts.keys().filter(|brick| !self.cuts.contains_key(*brick)) {
            note(brick);
        }
        boxes
    }

    /// Its bricks by the region file each is kept in: the region's place,
    /// in regions along X and Z, and its file's bytes.
    pub fn regions(&self) -> Vec<([i32; 2], Vec<u8>)> {
        let mut by: BTreeMap<[i32; 2], Region> = BTreeMap::new();
        let region_of = |x: i32, z: i32| [x.div_euclid(REGION_BRICKS), z.div_euclid(REGION_BRICKS)];
        for (brick, changed) in &self.bricks {
            by.entry(region_of(brick[0], brick[2])).or_default().bricks.push((*brick, changed));
        }
        for (brick, cut) in &self.cuts {
            by.entry(region_of(brick[0], brick[1])).or_default().cuts.push((*brick, cut));
        }
        for (foot, felled) in &self.felled {
            let region = region_of(foot[0].div_euclid(BRICK), foot[1].div_euclid(BRICK));
            by.entry(region).or_default().felled.push((*foot, felled));
        }
        by.into_iter().map(|(region, held)| (region, self.region_bytes(region, &held))).collect()
    }

    /// A region file: its magic and version, where it is, its kinds (a
    /// ground or paving a byte, whether tinted, and its tint), how many
    /// bricks, then packed with zstd each brick's place in the region (X and
    /// Z a byte, Y two), its voxels' bits and the codes of those set; then
    /// how many columns of bricks with cut grass, each its place (X and Z a
    /// byte), its bits cut and fading and what is left of each column cut;
    /// then how many trees felled, each its foot's column and its stump's
    /// height and box, as whole numbers of four bytes.
    fn region_bytes(&self, region: [i32; 2], held: &Region) -> Vec<u8> {
        let bricks = &held.bricks;
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
        payload.extend_from_slice(&(held.cuts.len() as u32).to_le_bytes());
        for (brick, cut) in &held.cuts {
            payload.push((brick[0] - region[0] * REGION_BRICKS) as u8);
            payload.push((brick[1] - region[1] * REGION_BRICKS) as u8);
            payload.extend_from_slice(&cut.set.to_le_bytes());
            // Once which were still fading; kept so files stay as they were.
            payload.extend_from_slice(&0u64.to_le_bytes());
            payload.extend((0..64).filter(|&index| cut.set >> index & 1 != 0).map(|index| cut.left[index]));
        }
        payload.extend_from_slice(&(held.felled.len() as u32).to_le_bytes());
        for (foot, felled) in &held.felled {
            let numbers = [foot[0], foot[1], felled.cut].into_iter().chain(felled.low).chain(felled.high);
            numbers.for_each(|n| payload.extend_from_slice(&n.to_le_bytes()));
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
        let version = word(4);
        if version == 0 || version > VERSION {
            bail!("a region of voxel changes of version {version}, which this Voxear cannot read");
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
        // The first version kept no cut grass or felled trees.
        if version < 2 {
            return Ok(());
        }
        let count = u32::from_le_bytes(take(4)?.try_into()?);
        for _ in 0..count {
            let place = take(2)?;
            let brick = [i32::from(place[0]) + region[0] * REGION_BRICKS, i32::from(place[1]) + region[1] * REGION_BRICKS];
            let mut cut = CutColumns { set: u64::from_le_bytes(take(8)?.try_into()?), ..CutColumns::default() };
            // Once which were still fading.
            take(8)?;
            let set: Vec<usize> = (0..64).filter(|&index| cut.set >> index & 1 != 0).collect();
            for (&index, &left) in set.iter().zip(take(set.len())?) {
                cut.left[index] = left;
            }
            if cut.set != 0 {
                self.cuts.insert(brick, Arc::new(cut));
            }
        }
        let count = u32::from_le_bytes(take(4)?.try_into()?);
        for _ in 0..count {
            let mut n = [0i32; 9];
            for number in &mut n {
                *number = i32::from_le_bytes(take(4)?.try_into()?);
            }
            let felled = Felled { cut: n[2], low: [n[3], n[4], n[5]], high: [n[6], n[7], n[8]] };
            self.fell([n[0], n[1]], felled);
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

    /// The region a column of bricks is kept in.
    fn region_of(x: i32, z: i32) -> [i32; 2] {
        [x.div_euclid(REGION_BRICKS), z.div_euclid(REGION_BRICKS)]
    }

    /// Every region holding a brick, a cut or a felled tree, in order.
    pub fn region_keys(&self) -> Vec<[i32; 2]> {
        let mut keys: Vec<[i32; 2]> = self.bricks.keys().map(|b| Self::region_of(b[0], b[2])).collect();
        keys.extend(self.cuts.keys().map(|b| Self::region_of(b[0], b[1])));
        keys.extend(self.felled.keys().map(|f| Self::region_of(f[0].div_euclid(BRICK), f[1].div_euclid(BRICK))));
        keys.sort();
        keys.dedup();
        keys
    }

    /// Region `region` as region files, as many as it takes for none to
    /// hold more than about `most` bytes before packing: its bricks shared
    /// among them, its cut grass and felled trees in the first. None where
    /// it holds nothing.
    pub fn region_parts(&self, region: [i32; 2], most: usize) -> Vec<Vec<u8>> {
        let mut parts = vec![Region::default()];
        let mut size = 0;
        for (brick, changed) in &self.bricks {
            if Self::region_of(brick[0], brick[2]) != region {
                continue;
            }
            let bytes = 4 + 64 + changed.count() as usize;
            if size + bytes > most && size > 0 {
                parts.push(Region::default());
                size = 0;
            }
            size += bytes;
            parts.last_mut().expect("a part").bricks.push((*brick, changed));
        }
        for (brick, cut) in &self.cuts {
            if Self::region_of(brick[0], brick[1]) == region {
                parts[0].cuts.push((*brick, cut));
            }
        }
        for (foot, felled) in &self.felled {
            if Self::region_of(foot[0].div_euclid(BRICK), foot[1].div_euclid(BRICK)) == region {
                parts[0].felled.push((*foot, felled));
            }
        }
        let empty = |part: &Region| part.bricks.is_empty() && part.cuts.is_empty() && part.felled.is_empty();
        if parts.iter().all(empty) {
            return Vec::new();
        }
        parts.iter().map(|part| self.region_bytes(region, part)).collect()
    }

    /// Lets go of everything kept in region `region`: its bricks, cut grass
    /// and felled trees, back to what the ground and the edits make.
    pub fn clear_region(&mut self, region: [i32; 2]) {
        self.bricks.retain(|b, _| Self::region_of(b[0], b[2]) != region);
        self.cuts.retain(|b, _| Self::region_of(b[0], b[1]) != region);
        self.felled.retain(|f, _| Self::region_of(f[0].div_euclid(BRICK), f[1].div_euclid(BRICK)) != region);
    }

    /// What `older` must take to become these: each brick, column of cut
    /// grass and felled tree that differs, the bricks voxel by voxel.
    pub fn delta_from(&self, older: &Self) -> VoxelDelta {
        let mut delta = VoxelDelta::default();
        let kind_of = |changes: &Self, code: u8| match code {
            0 => None,
            code => Some(changes.kinds[usize::from(code) - 1]),
        };
        let mut keys: Vec<[i32; 3]> = self.bricks.keys().chain(older.bricks.keys()).copied().collect();
        keys.sort();
        keys.dedup();
        let same_kinds = self.kinds == older.kinds;
        for key in keys {
            let (now, was) = (self.bricks.get(&key), older.bricks.get(&key));
            if let (Some(now), Some(was)) = (now, was)
                && same_kinds
                && (Arc::ptr_eq(now, was) || now == was)
            {
                continue;
            }
            let mut voxels = Vec::new();
            for index in 0..512 {
                let at = |brick: Option<&Arc<ChangedBrick>>, changes: &Self| {
                    brick.filter(|b| b.is_set(index)).map(|b| kind_of(changes, b.codes[index]))
                };
                let (to, from) = (at(now, self), at(was, older));
                if to != from {
                    voxels.push((index as u16, to));
                }
            }
            if !voxels.is_empty() {
                delta.bricks.push((key, voxels));
            }
        }
        let mut columns: Vec<[i32; 2]> = self.cuts.keys().chain(older.cuts.keys()).copied().collect();
        columns.sort();
        columns.dedup();
        for key in columns {
            let (now, was) = (self.cuts.get(&key), older.cuts.get(&key));
            let same = match (now, was) {
                (Some(now), Some(was)) => Arc::ptr_eq(now, was) || now == was,
                (None, None) => true,
                _ => false,
            };
            if !same {
                delta.cuts.push((key, now.map(|cut| (**cut).clone())));
            }
        }
        let mut feet: Vec<[i32; 2]> = self.felled.keys().chain(older.felled.keys()).copied().collect();
        feet.sort();
        feet.dedup();
        for foot in feet {
            let now = self.felled.get(&foot).copied();
            if now != older.felled.get(&foot).copied() {
                delta.felled.push((foot, now));
            }
        }
        delta
    }

    /// Takes `delta` in, as [`VoxelChanges::delta_from`] made it from these:
    /// gives whether every voxel set found room for its kind.
    pub fn apply_delta(&mut self, delta: &VoxelDelta) -> bool {
        let mut all = true;
        for (brick, voxels) in &delta.bricks {
            let low = IVec3::from(brick.map(|a| a * BRICK));
            for &(index, to) in voxels {
                let index = i32::from(index);
                let cell = low + IVec3::new(index % BRICK, (index / BRICK) % BRICK, index / (BRICK * BRICK));
                match to {
                    None => self.unset(cell),
                    Some(to) => all &= self.set(cell, to),
                }
            }
        }
        for (key, cut) in &delta.cuts {
            match cut {
                Some(cut) => drop(self.cuts.insert(*key, Arc::new(cut.clone()))),
                None => drop(self.cuts.remove(key)),
            }
        }
        for (foot, felled) in &delta.felled {
            match felled {
                Some(felled) => drop(self.felled.insert(*foot, *felled)),
                None => drop(self.felled.remove(foot)),
            }
        }
        all
    }
}

/// What one volume's changes must take to become another's (see
/// [`VoxelChanges::delta_from`]): its bricks voxel by voxel, each voxel let
/// go (none) or set to air or a kind; its columns of cut grass and its
/// felled trees whole, or let go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelDelta {
    pub bricks: Vec<([i32; 3], Vec<(u16, Option<VoxelSet>)>)>,
    pub cuts: Vec<([i32; 2], Option<CutColumns>)>,
    pub felled: Vec<([i32; 2], Option<Felled>)>,
}

/// Payloads longer than this are packed with zstd.
const PACK_OVER: usize = 160;
/// A brick with at most this many voxels changed names them by place, more
/// by a bit each.
const LISTED: usize = 30;

impl VoxelDelta {
    pub fn is_empty(&self) -> bool {
        self.bricks.is_empty() && self.cuts.is_empty() && self.felled.is_empty()
    }

    /// How many voxels it sets or lets go.
    pub fn voxels(&self) -> usize {
        self.bricks.iter().map(|(_, voxels)| voxels.len()).sum()
    }

    /// As bytes: a byte for whether the rest is packed with zstd (and then
    /// its length), then the kinds it uses (a ground or paving and a tint),
    /// then each brick (its place zigzagged, how many voxels, their places
    /// in the brick listed or as 512 bits, and each one's code: 0 let go, 1
    /// air, a kind's place and two), each column of cut grass and each
    /// felled tree.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut kinds: Vec<VoxelKind> = Vec::new();
        for (_, voxels) in &self.bricks {
            for (_, to) in voxels {
                if let Some(Some(kind)) = to
                    && !kinds.contains(kind)
                {
                    kinds.push(*kind);
                }
            }
        }
        let mut out = Vec::new();
        put_varint(&mut out, kinds.len() as u64);
        for kind in &kinds {
            out.push(kind.material as u8);
            out.push(u8::from(kind.tint.is_some()));
            if let Some(tint) = kind.tint {
                out.extend_from_slice(&tint);
            }
        }
        put_varint(&mut out, self.bricks.len() as u64);
        for (brick, voxels) in &self.bricks {
            brick.iter().for_each(|&a| put_zigzag(&mut out, a));
            put_varint(&mut out, voxels.len() as u64);
            if voxels.len() <= LISTED {
                voxels.iter().for_each(|(index, _)| out.extend_from_slice(&index.to_le_bytes()));
            } else {
                let mut bits = [0u64; 8];
                voxels.iter().for_each(|&(index, _)| bits[usize::from(index) >> 6] |= 1 << (index & 63));
                bits.iter().for_each(|word| out.extend_from_slice(&word.to_le_bytes()));
            }
            // In the order the places were given: listed as they come, or by place from the bits.
            let mut sorted: Vec<&(u16, Option<VoxelSet>)> = voxels.iter().collect();
            if voxels.len() > LISTED {
                sorted.sort_by_key(|(index, _)| *index);
            }
            for (_, to) in sorted {
                let code = match to {
                    None => 0,
                    Some(None) => 1,
                    Some(Some(kind)) => 2 + kinds.iter().position(|k| k == kind).expect("listed") as u64,
                };
                put_varint(&mut out, code);
            }
        }
        put_varint(&mut out, self.cuts.len() as u64);
        for (key, cut) in &self.cuts {
            key.iter().for_each(|&a| put_zigzag(&mut out, a));
            out.push(u8::from(cut.is_some()));
            if let Some(cut) = cut {
                out.extend_from_slice(&cut.set.to_le_bytes());
                out.extend((0..64).filter(|&index| cut.set >> index & 1 != 0).map(|index| cut.left[index]));
            }
        }
        put_varint(&mut out, self.felled.len() as u64);
        for (foot, felled) in &self.felled {
            foot.iter().for_each(|&a| put_zigzag(&mut out, a));
            out.push(u8::from(felled.is_some()));
            if let Some(felled) = felled {
                [felled.cut].iter().chain(&felled.low).chain(&felled.high).for_each(|&n| put_zigzag(&mut out, n));
            }
        }
        if out.len() > PACK_OVER
            && let Ok(packed) = zstd::bulk::compress(&out, 3)
            && packed.len() + 5 < out.len()
        {
            let mut packed_out = vec![1];
            put_varint(&mut packed_out, out.len() as u64);
            packed_out.extend_from_slice(&packed);
            return packed_out;
        }
        let mut plain = vec![0];
        plain.extend_from_slice(&out);
        plain
    }

    /// The delta in `bytes`, as [`VoxelDelta::to_bytes`] wrote it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let unpacked;
        let body = match bytes.first() {
            Some(0) => &bytes[1..],
            Some(1) => {
                let mut at = 1;
                let length = get_varint(bytes, &mut at)? as usize;
                ensure!(length <= 64 << 20, "a delta of voxels too large");
                unpacked = zstd::bulk::decompress(&bytes[at..], length)?;
                &unpacked[..]
            }
            _ => bail!("not a delta of voxels"),
        };
        let mut at = 0;
        let byte = |at: &mut usize| -> Result<u8> {
            let b = *body.get(*at).context("the delta ends early")?;
            *at += 1;
            Ok(b)
        };
        let mut kinds = Vec::new();
        for _ in 0..get_varint(body, &mut at)? {
            let material = *VoxelMaterial::ALL.get(usize::from(byte(&mut at)?)).context("a voxel of no ground")?;
            let tint = match byte(&mut at)? {
                0 => None,
                _ => Some([byte(&mut at)?, byte(&mut at)?, byte(&mut at)?]),
            };
            kinds.push(VoxelKind { material, tint });
        }
        let mut delta = VoxelDelta::default();
        for _ in 0..get_varint(body, &mut at)? {
            let brick = [get_zigzag(body, &mut at)?, get_zigzag(body, &mut at)?, get_zigzag(body, &mut at)?];
            let count = get_varint(body, &mut at)? as usize;
            ensure!(count <= 512, "a brick of more than 512 voxels");
            let places: Vec<u16> = if count <= LISTED {
                (0..count).map(|_| Ok(u16::from_le_bytes([byte(&mut at)?, byte(&mut at)?]))).collect::<Result<_>>()?
            } else {
                let mut bits = [0u64; 8];
                for word in &mut bits {
                    let piece = body.get(at..at + 8).context("the delta ends early")?;
                    *word = u64::from_le_bytes(piece.try_into()?);
                    at += 8;
                }
                (0..512u16).filter(|&i| bits[usize::from(i) >> 6] >> (i & 63) & 1 != 0).collect()
            };
            ensure!(places.len() == count && places.iter().all(|&p| p < 512), "a brick's voxels out of place");
            let mut voxels = Vec::with_capacity(count);
            for index in places {
                let to = match get_varint(body, &mut at)? {
                    0 => None,
                    1 => Some(None),
                    code => Some(Some(*kinds.get(code as usize - 2).context("a voxel of no kind")?)),
                };
                voxels.push((index, to));
            }
            delta.bricks.push((brick, voxels));
        }
        for _ in 0..get_varint(body, &mut at)? {
            let key = [get_zigzag(body, &mut at)?, get_zigzag(body, &mut at)?];
            let cut = match byte(&mut at)? {
                0 => None,
                _ => {
                    let piece = body.get(at..at + 8).context("the delta ends early")?;
                    let mut cut = CutColumns { set: u64::from_le_bytes(piece.try_into()?), ..CutColumns::default() };
                    at += 8;
                    for index in (0..64).filter(|&index| cut.set >> index & 1 != 0) {
                        cut.left[index] = byte(&mut at)?;
                    }
                    Some(cut)
                }
            };
            delta.cuts.push((key, cut));
        }
        for _ in 0..get_varint(body, &mut at)? {
            let foot = [get_zigzag(body, &mut at)?, get_zigzag(body, &mut at)?];
            let felled = match byte(&mut at)? {
                0 => None,
                _ => {
                    let mut n = [0i32; 7];
                    for number in &mut n {
                        *number = get_zigzag(body, &mut at)?;
                    }
                    Some(Felled { cut: n[0], low: [n[1], n[2], n[3]], high: [n[4], n[5], n[6]] })
                }
            };
            delta.felled.push((foot, felled));
        }
        Ok(delta)
    }
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_zigzag(out: &mut Vec<u8>, value: i32) {
    put_varint(out, u64::from(((value << 1) ^ (value >> 31)) as u32));
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at).context("the delta ends early")?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("a number too long")
}

fn get_zigzag(bytes: &[u8], at: &mut usize) -> Result<i32> {
    let zigzag = get_varint(bytes, at)? as u32;
    Ok((zigzag >> 1) as i32 ^ -((zigzag & 1) as i32))
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

    #[test]
    fn a_delta_takes_one_volumes_changes_to_anothers_and_reads_back_from_its_bytes() {
        let mut was = VoxelChanges::default();
        let brick = VoxelKind { material: VoxelMaterial::Bricks, tint: Some([200, 60, 60]) };
        let planks = VoxelKind { material: VoxelMaterial::Planks, tint: None };
        for x in 0..40 {
            was.set(IVec3::new(x, 30, 5), Some(planks));
        }
        was.cut(3, 3, 2);
        // The other placed its kinds in another order, dug and built more, cut more grass and felled a tree.
        let mut now = VoxelChanges::default();
        now.set(IVec3::new(-9, 2, 2), Some(brick));
        for x in 0..40 {
            now.set(IVec3::new(x, 30, 5), Some(planks));
        }
        now.unset(IVec3::new(-9, 2, 2));
        for x in -20..20 {
            for y in 0..12 {
                now.set(IVec3::new(x, y, 1), if (x + y) % 3 == 0 { Some(brick) } else { None });
            }
        }
        now.set(IVec3::new(7, 30, 5), None);
        now.cut(3, 3, 2);
        now.cut(-4, 9, 0);
        now.fell([1, 1], Felled { cut: 4, low: [-8, 0, -8], high: [8, 90, 8] });
        let delta = now.delta_from(&was);
        assert!(delta.voxels() > 400 && delta.voxels() < 500, "{} voxels", delta.voxels());
        assert_eq!((delta.cuts.len(), delta.felled.len()), (1, 1));
        let bytes = delta.to_bytes();
        assert_eq!(VoxelDelta::from_bytes(&bytes).unwrap(), delta);
        assert!(bytes.len() < 400, "{} bytes", bytes.len());
        let mut taken = was.clone();
        assert!(taken.apply_delta(&VoxelDelta::from_bytes(&bytes).unwrap()));
        assert!(taken.delta_from(&now).is_empty() && now.delta_from(&taken).is_empty(), "the same voxels");
        // A voxel dug alone costs a few bytes.
        let mut one = now.clone();
        one.set(IVec3::new(100, 40, 100), None);
        assert!(one.delta_from(&now).to_bytes().len() < 16);
        // Back the other way, and as regions in parts.
        let mut back = now.clone();
        back.apply_delta(&was.delta_from(&now));
        assert!(back.delta_from(&was).is_empty());
        let region = now.region_keys()[0];
        let parts = now.region_parts(region, 200);
        assert!(parts.len() > 1, "cut into parts");
        let mut read = VoxelChanges::default();
        parts.iter().for_each(|part| read.read_region(part).unwrap());
        read.clear_region([9, 9]);
        let mut only = now.clone();
        now.region_keys().into_iter().filter(|r| *r != region).for_each(|r| only.clear_region(r));
        assert!(read.delta_from(&only).is_empty() && only.delta_from(&read).is_empty());
    }

    #[test]
    fn cut_grass_keeps_what_was_under_the_cut_grows_back_and_is_kept_with_felled_trees_in_the_regions() {
        let mut changes = VoxelChanges::default();
        for x in -3..3 {
            assert!(changes.cut(x, 7, 4));
        }
        assert!(!changes.cut(0, 7, 6), "cut lower already");
        assert!(changes.cut(0, 7, 1), "cut lower now");
        assert_eq!(changes.cut_left(0, 7), Some(1));
        assert_eq!(changes.cut_left(1, 7), Some(4));
        assert_eq!(changes.cut_left(0, 8), None);
        let felled = Felled { cut: 40, low: [-20, 30, -20], high: [20, 200, 20] };
        changes.fell([-1, 2], felled);
        changes.fell([-1, 2], Felled { cut: 60, ..felled });
        assert_eq!(changes.felled([-1, 2]).map(|f| f.cut), Some(40), "the lower cut is kept");
        let regions = changes.regions();
        assert_eq!(regions.len(), 2, "on both sides of X 0");
        let back = VoxelChanges::from_regions(regions.iter().map(|(_, bytes)| bytes.as_slice())).unwrap();
        assert_eq!(back, changes);
        // Growing back a voxel at a time, let go once as tall as anything grows there.
        let before = changes.clone();
        assert!(changes.tend_cuts(2, 8));
        assert_eq!(changes.cut_left(0, 7), Some(3));
        assert_eq!(changes.cuts_changed_from(&before), vec![([-8, 0], [-1, 7]), ([0, 0], [7, 7])]);
        assert!(changes.changed_from(&before).is_empty(), "no voxel is any more or less solid");
        while changes.tend_cuts(2, 8) {}
        assert_eq!(changes.cut_left(0, 7), None);
        assert!(!changes.any_cut() && !changes.is_empty(), "the felled tree");
    }
}
