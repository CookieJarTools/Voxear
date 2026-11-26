//! Voxel Prints: models made into voxels (a `.voxprint`), solid all through,
//! which are printed into an endless Voxel Volume's ground where a person
//! or a script places them, as a ball is carved or filled.
//!
//! A print is a box of voxels at so many a metre, each a place in its own
//! palette of a few colours, and the place in the box its model's own
//! origin is at, which is where it is placed. Its voxels are kept brick by
//! brick, as the GPU reads them: a word for each brick of the box naming
//! the brick's place among those kept, or [`EMPTY`], and for each brick
//! kept, its 512 bits of which voxels are solid and where its materials
//! start, a byte a solid voxel in the order of its bits.
//!
//! Its file is small: a few numbers, its palette and how it was made, then
//! each brick as one byte where it is empty or all of one colour, the rest
//! its bits and bytes, the whole packed with zstd. A box a metre a side of
//! sandbags at 16 voxels a metre is a few kilobytes.

use std::hash::{Hash, Hasher};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};

/// How many voxels a brick is a side, and the words a kept brick takes.
pub const BRICK: u32 = 8;
pub const BRICK_WORDS: usize = 17;
/// A brick of the box with nothing in it.
pub const EMPTY: u32 = u32::MAX;
/// The most voxels a print is along a side, and the most colours its
/// palette has.
pub const LARGEST_PRINT: u32 = 2048;
pub const MOST_COLOURS: u32 = 64;

const MAGIC: &[u8; 4] = b"VXPR";
const VERSION: u32 = 1;
const BRICK_CELLS: usize = 512;

/// What a print's model leaves inside its skin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelPrintFill {
    /// Solid wherever the outside cannot reach: a closed model is solid
    /// all through, so carving into it finds more of it.
    #[default]
    Solid,
    /// Solid too where its skin is round a place on five of its six sides,
    /// for a model whose skin has holes, as a scan open underneath.
    Sealed,
    /// Its skin alone, a voxel thick.
    Shell,
}

impl VoxelPrintFill {
    pub const ALL: [Self; 3] = [Self::Solid, Self::Sealed, Self::Shell];

    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Sealed => "Sealed",
            Self::Shell => "Shell",
        }
    }
}

/// A picture a part of a print's model is coloured from, in place of its
/// own, as for a model whose file names none: the part (`material0`), and
/// a picture in the project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelPrintPicture {
    pub part: String,
    pub picture: String,
}

/// How a print is made from its model, kept in its file so it can be made
/// again as the model changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelPrintSettings {
    /// The model it is made from, a reference to a `.gltf`, `.glb`, `.fbx`
    /// or `.boxmodel`.
    pub model: String,
    /// How many voxels make a metre: as many as the volumes it is printed
    /// into, for voxels of the same size as theirs.
    pub voxels_per_metre: u32,
    /// Multiplies the model's size.
    pub scale: f32,
    /// The most colours its voxels are in, each the nearest of them.
    pub colours: u32,
    pub fill: VoxelPrintFill,
    /// Pictures its parts are coloured from, in place of their own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pictures: Vec<VoxelPrintPicture>,
    /// Colours of its voxels made glass, seen through, or lights, glowing
    /// as a lantern's flame does and lighting what is round them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<VoxelPrintMaterial>,
}

/// A colour of a print's voxels made glass or a light.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelPrintMaterial {
    /// The colour, red, green and blue as sRGB.
    pub colour: [u8; 3],
    /// How see-through, from 0, solid, to 15, as clear as glass is made:
    /// 14 for a window, less for frosted or coloured glass.
    pub glass: u8,
    /// How brightly it glows, from 0, not at all, to 15, as a block's light:
    /// a step less of it for each metre from it.
    pub light: u8,
}

impl Default for VoxelPrintSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            voxels_per_metre: 16,
            scale: 1.0,
            colours: 32,
            fill: VoxelPrintFill::Solid,
            pictures: Vec::new(),
            materials: Vec::new(),
        }
    }
}

/// A model made into voxels, to be printed into endless Voxel Volumes.
#[derive(Clone, Debug)]
pub struct VoxelPrint {
    /// How many voxels it is along X, Y and Z.
    pub size: [u32; 3],
    pub voxels_per_metre: u32,
    /// Where its model's origin is, in voxels from its least corner: the
    /// point placed where it is printed.
    pub origin: Vec3,
    /// Its colours, as sRGB.
    pub palette: Vec<[u8; 4]>,
    /// How it was made.
    pub settings: VoxelPrintSettings,
    /// For each brick of its box, X fastest then Y then Z, its place among
    /// those kept, or [`EMPTY`].
    pub grid: Vec<u32>,
    /// Each brick kept: its bits, then where its materials start.
    pub bricks: Vec<u32>,
    /// A byte a solid voxel: its place in the palette.
    pub materials: Vec<u8>,
    /// How many voxels are solid.
    pub solid: u64,
    /// What tells it from another print: a hash of its voxels and colours.
    pub id: u64,
}

impl PartialEq for VoxelPrint {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.size == other.size && self.voxels_per_metre == other.voxels_per_metre
    }
}

impl VoxelPrint {
    /// A print of the voxels `at` gives, a palette place and one or 0 for
    /// none, over a box `size` voxels, its model's origin at `origin`.
    pub fn from_voxels(
        size: [u32; 3],
        voxels_per_metre: u32,
        origin: Vec3,
        palette: Vec<[u8; 4]>,
        settings: VoxelPrintSettings,
        at: impl Fn([u32; 3]) -> u8,
    ) -> Self {
        let across = bricks_across(size);
        let mut grid = vec![EMPTY; (across[0] * across[1] * across[2]) as usize];
        let mut bricks = Vec::new();
        let mut materials = Vec::new();
        let mut solid = 0;
        for bz in 0..across[2] {
            for by in 0..across[1] {
                for bx in 0..across[0] {
                    let mut words = [0u32; BRICK_WORDS];
                    words[16] = materials.len() as u32;
                    for index in 0..BRICK_CELLS as u32 {
                        let cell = [bx * BRICK + index % 8, by * BRICK + (index / 8) % 8, bz * BRICK + index / 64];
                        if (0..3).any(|a| cell[a] >= size[a]) {
                            continue;
                        }
                        let material = at(cell);
                        if material != 0 {
                            words[(index >> 5) as usize] |= 1 << (index & 31);
                            materials.push(material - 1);
                            solid += 1;
                        }
                    }
                    if words[..16].iter().any(|&w| w != 0) {
                        grid[(bx + across[0] * (by + across[1] * bz)) as usize] = (bricks.len() / BRICK_WORDS) as u32;
                        bricks.extend_from_slice(&words);
                    }
                }
            }
        }
        let mut print = Self {
            size,
            voxels_per_metre: voxels_per_metre.max(1),
            origin,
            palette,
            settings,
            grid,
            bricks,
            materials,
            solid,
            id: 0,
        };
        print.id = print.hash_of();
        print
    }

    /// Colour `place` of its palette as a volume's palette keeps it: its
    /// red, green and blue, and over them how see-through it is, by its
    /// alpha or its settings' materials, and how it glows, by those
    /// ([`kind_byte`](super::kind_byte)).
    pub fn volume_colour(&self, place: usize) -> [u8; 4] {
        let [r, g, b, alpha] = self.palette.get(place).copied().unwrap_or([200, 200, 200, 255]);
        let material = self.settings.materials.iter().find(|material| material.colour == [r, g, b]);
        let (glass, light) = material.map_or((0, 0), |material| (material.glass.min(15), material.light));
        let alpha = alpha.min(255 - glass * 17);
        [r, g, b, super::kind_byte(alpha, light)]
    }

    /// Makes `materials` its colours made glass or lights, in place of what
    /// they were, telling it from the print it was.
    pub fn set_materials(&mut self, materials: Vec<VoxelPrintMaterial>) {
        self.settings.materials = materials;
        self.id = self.hash_of();
    }

    /// How many bricks its box is along each side.
    pub fn bricks_across(&self) -> [u32; 3] {
        bricks_across(self.size)
    }

    /// The voxel at `cell`: its palette place and one, or 0 for none.
    pub fn get(&self, cell: [i32; 3]) -> u8 {
        if (0..3).any(|a| cell[a] < 0 || cell[a] >= self.size[a] as i32) {
            return 0;
        }
        let across = self.bricks_across();
        let b = cell.map(|a| a as u32 / BRICK);
        let brick = self.grid[(b[0] + across[0] * (b[1] + across[1] * b[2])) as usize];
        if brick == EMPTY {
            return 0;
        }
        let words = &self.bricks[brick as usize * BRICK_WORDS..][..BRICK_WORDS];
        let l = cell.map(|a| a as u32 % BRICK);
        let index = (l[0] + 8 * (l[1] + 8 * l[2])) as usize;
        if words[index >> 5] >> (index & 31) & 1 == 0 {
            return 0;
        }
        let before: u32 = words[..index >> 5].iter().map(|w| w.count_ones()).sum::<u32>()
            + (words[index >> 5] & ((1u32 << (index & 31)) - 1)).count_ones();
        self.materials[(words[16] + before) as usize] + 1
    }

    /// How large it is, in metres.
    pub fn metres(&self) -> Vec3 {
        Vec3::new(self.size[0] as f32, self.size[1] as f32, self.size[2] as f32) / self.voxels_per_metre as f32
    }

    /// Its box's least and most corners in metres from its origin, before
    /// it is turned.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let per = self.voxels_per_metre as f32;
        let size = Vec3::new(self.size[0] as f32, self.size[1] as f32, self.size[2] as f32);
        (-self.origin / per, (size - self.origin) / per)
    }

    /// The box a print placed at `at` and turned `rotation` reaches, least
    /// and most corners, in the same space as `at`.
    pub fn placed_bounds(&self, at: Vec3, rotation: Quat) -> (Vec3, Vec3) {
        let (low, high) = self.bounds();
        let mut least = Vec3::splat(f32::MAX);
        let mut most = Vec3::splat(f32::MIN);
        for corner in 0..8 {
            let c = Vec3::new(
                if corner & 1 == 0 { low.x } else { high.x },
                if corner & 2 == 0 { low.y } else { high.y },
                if corner & 4 == 0 { low.z } else { high.z },
            );
            let p = at + rotation * c;
            least = least.min(p);
            most = most.max(p);
        }
        (least, most)
    }

    /// A bit for each of its columns of voxels, X fastest then Z, set where
    /// the column has any solid: what it stands over, where grass and trees
    /// do not grow.
    pub fn footprint(&self) -> Vec<u32> {
        let mut bits = vec![0u32; (self.size[0] * self.size[2]).div_ceil(32) as usize];
        let across = self.bricks_across();
        for bz in 0..across[2] {
            for by in 0..across[1] {
                for bx in 0..across[0] {
                    let brick = self.grid[(bx + across[0] * (by + across[1] * bz)) as usize];
                    if brick == EMPTY {
                        continue;
                    }
                    let words = &self.bricks[brick as usize * BRICK_WORDS..][..16];
                    for index in 0..BRICK_CELLS as u32 {
                        if words[(index >> 5) as usize] >> (index & 31) & 1 == 0 {
                            continue;
                        }
                        let (x, z) = (bx * BRICK + index % 8, bz * BRICK + index / 64);
                        if x < self.size[0] && z < self.size[2] {
                            let column = x + self.size[0] * z;
                            bits[(column >> 5) as usize] |= 1 << (column & 31);
                        }
                    }
                }
            }
        }
        bits
    }

    /// Whether it stands over column (`x`, `z`) of its voxels.
    pub fn covers(&self, x: i32, z: i32, footprint: &[u32]) -> bool {
        if x < 0 || z < 0 || x >= self.size[0] as i32 || z >= self.size[2] as i32 {
            return false;
        }
        let column = (x + self.size[0] as i32 * z) as u32;
        footprint.get((column >> 5) as usize).is_some_and(|word| word >> (column & 31) & 1 != 0)
    }

    /// The bytes its GPU record and voxels take.
    pub fn bytes(&self) -> usize {
        self.grid.len() * 4 + self.bricks.len() * 4 + self.materials.len()
    }

    fn hash_of(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.size, self.voxels_per_metre, self.origin.to_array().map(f32::to_bits)).hash(&mut hasher);
        (&self.palette, &self.grid, &self.bricks, &self.materials).hash(&mut hasher);
        // Which colours are glass or lights changes how it is printed.
        self.settings.materials.hash(&mut hasher);
        hasher.finish()
    }

    /// Its file: a few numbers, how it was made and its palette, then its
    /// bricks packed with zstd.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let settings = toml::to_string(&self.settings)?;
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        for word in [VERSION, self.size[0], self.size[1], self.size[2], self.voxels_per_metre] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        for part in self.origin.to_array() {
            out.extend_from_slice(&part.to_le_bytes());
        }
        out.extend_from_slice(&(self.palette.len() as u32).to_le_bytes());
        for colour in &self.palette {
            out.extend_from_slice(colour);
        }
        out.extend_from_slice(&(settings.len() as u32).to_le_bytes());
        out.extend_from_slice(settings.as_bytes());
        // Each brick: none, all one colour, all solid in many, or some solid.
        let mut payload = Vec::new();
        for &brick in &self.grid {
            if brick == EMPTY {
                payload.push(0);
                continue;
            }
            let words = &self.bricks[brick as usize * BRICK_WORDS..][..BRICK_WORDS];
            let count: u32 = words[..16].iter().map(|w| w.count_ones()).sum();
            let own = &self.materials[words[16] as usize..(words[16] + count) as usize];
            let full = count as usize == BRICK_CELLS;
            if full && own.iter().all(|&m| m == own[0]) {
                payload.extend_from_slice(&[1, own[0]]);
            } else if full {
                payload.push(2);
                payload.extend_from_slice(own);
            } else {
                payload.push(3);
                for word in &words[..16] {
                    payload.extend_from_slice(&word.to_le_bytes());
                }
                payload.extend_from_slice(own);
            }
        }
        let packed = zstd::bulk::compress(&payload, 19)?;
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&packed);
        Ok(out)
    }

    /// The print a `.voxprint` file holds.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() >= 4 && &bytes[..4] == MAGIC, "not a voxel print");
        let mut at = 4;
        let mut word = || -> Result<u32> {
            let four = bytes.get(at..at + 4).context("the voxel print ends early")?;
            at += 4;
            Ok(u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
        };
        let version = word()?;
        if version != VERSION {
            bail!("a voxel print of version {version}, which this Voxear cannot read");
        }
        let size = [word()?, word()?, word()?];
        ensure!(size.iter().all(|&a| (1..=LARGEST_PRINT).contains(&a)), "a voxel print of size {size:?}");
        let voxels_per_metre = word()?;
        let origin = Vec3::new(f32::from_bits(word()?), f32::from_bits(word()?), f32::from_bits(word()?));
        let colours = word()? as usize;
        ensure!(colours <= 255, "a voxel print of {colours} colours");
        drop(word);
        let take = |at: &mut usize, n: usize| -> Result<&[u8]> {
            let piece = bytes.get(*at..*at + n).context("the voxel print ends early")?;
            *at += n;
            Ok(piece)
        };
        let palette: Vec<[u8; 4]> = take(&mut at, colours * 4)?.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
        let length = u32::from_le_bytes(take(&mut at, 4)?.try_into()?) as usize;
        let settings: VoxelPrintSettings = toml::from_str(std::str::from_utf8(take(&mut at, length)?)?)?;
        let unpacked = u32::from_le_bytes(take(&mut at, 4)?.try_into()?) as usize;
        let payload = zstd::bulk::decompress(&bytes[at..], unpacked)?;
        let across = bricks_across(size);
        let count = (across[0] * across[1] * across[2]) as usize;
        let mut grid = vec![EMPTY; count];
        let mut bricks = Vec::new();
        let mut materials = Vec::new();
        let mut solid = 0u64;
        let mut p = 0;
        let mut read = |n: usize| -> Result<&[u8]> {
            let piece = payload.get(p..p + n).context("the voxel print's bricks end early")?;
            p += n;
            Ok(piece)
        };
        for cell in grid.iter_mut() {
            let tag = read(1)?[0];
            if tag == 0 {
                continue;
            }
            let mut words = [0u32; BRICK_WORDS];
            words[16] = materials.len() as u32;
            match tag {
                1 => {
                    words[..16].fill(u32::MAX);
                    materials.extend(std::iter::repeat_n(read(1)?[0], BRICK_CELLS));
                }
                2 => {
                    words[..16].fill(u32::MAX);
                    materials.extend_from_slice(read(BRICK_CELLS)?);
                }
                3 => {
                    for word in words.iter_mut().take(16) {
                        *word = u32::from_le_bytes(read(4)?.try_into()?);
                    }
                    let n: u32 = words[..16].iter().map(|w| w.count_ones()).sum();
                    materials.extend_from_slice(read(n as usize)?);
                }
                _ => bail!("a voxel print's brick of kind {tag}"),
            }
            solid += words[..16].iter().map(|w| u64::from(w.count_ones())).sum::<u64>();
            *cell = (bricks.len() / BRICK_WORDS) as u32;
            bricks.extend_from_slice(&words);
        }
        ensure!(
            materials.iter().all(|&m| usize::from(m) < palette.len().max(1)),
            "a voxel print's colour out of its palette"
        );
        let mut print = Self {
            size,
            voxels_per_metre: voxels_per_metre.max(1),
            origin,
            palette,
            settings,
            grid,
            bricks,
            materials,
            solid,
            id: 0,
        };
        print.id = print.hash_of();
        Ok(print)
    }
}

fn bricks_across(size: [u32; 3]) -> [u32; 3] {
    size.map(|a| a.div_ceil(BRICK).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ball() -> VoxelPrint {
        let palette = vec![[200, 40, 40, 255], [40, 200, 40, 255]];
        VoxelPrint::from_voxels(
            [20, 12, 9],
            16,
            Vec3::new(10.0, 0.0, 4.5),
            palette,
            VoxelPrintSettings::default(),
            |c| {
                let d = [c[0] as f32 - 9.5, c[1] as f32 - 5.5, c[2] as f32 - 4.0];
                if d[0] * d[0] / 81.0 + d[1] * d[1] / 30.0 + d[2] * d[2] / 16.0 <= 1.0 {
                    1 + u8::from(c[1] > 6)
                } else {
                    0
                }
            },
        )
    }

    #[test]
    fn a_prints_voxels_are_found_again_and_come_back_from_its_file_as_they_were() {
        let print = ball();
        assert_eq!(print.get([9, 5, 4]), 1);
        assert_eq!(print.get([9, 8, 4]), 2);
        assert_eq!(print.get([0, 0, 0]), 0);
        assert_eq!(print.get([30, 0, 0]), 0, "outside its box");
        assert!(print.solid > 500);
        let bytes = print.to_bytes().unwrap();
        let back = VoxelPrint::from_bytes(&bytes).unwrap();
        assert_eq!(back, print);
        assert_eq!(back.id, print.id);
        assert_eq!(back.origin, print.origin);
        assert_eq!(back.palette, print.palette);
        assert_eq!(back.solid, print.solid);
        for z in 0..9 {
            for y in 0..12 {
                for x in 0..20 {
                    assert_eq!(back.get([x, y, z]), print.get([x, y, z]), "{x} {y} {z}");
                }
            }
        }
        assert!(VoxelPrint::from_bytes(&bytes[..bytes.len() - 3]).is_err(), "a file cut short");
    }

    #[test]
    fn a_full_block_of_one_colour_takes_a_couple_of_bytes() {
        let solid =
            VoxelPrint::from_voxels([64, 64, 64], 16, Vec3::ZERO, vec![[90; 4]], VoxelPrintSettings::default(), |_| 1);
        let bytes = solid.to_bytes().unwrap();
        // 512 bricks of 512 voxels each.
        assert_eq!(solid.solid, 64 * 64 * 64);
        assert!(bytes.len() < 400, "{} bytes", bytes.len());
        assert_eq!(VoxelPrint::from_bytes(&bytes).unwrap().get([63, 63, 63]), 1);
    }

    #[test]
    fn a_print_turned_a_quarter_reaches_the_box_turned_with_it() {
        let print = ball();
        let (low, high) = print.bounds();
        assert_eq!(low, Vec3::new(-10.0, 0.0, -4.5) / 16.0);
        assert_eq!(high, Vec3::new(10.0, 12.0, 4.5) / 16.0);
        let at = Vec3::new(5.0, 1.0, 2.0);
        let (least, most) = print.placed_bounds(at, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2));
        // A quarter turn about Y takes X to -Z and Z to X.
        assert!((least - (at + Vec3::new(-4.5, 0.0, -10.0) / 16.0)).abs().max_element() < 1e-5, "{least}");
        assert!((most - (at + Vec3::new(4.5, 12.0, 10.0) / 16.0)).abs().max_element() < 1e-5, "{most}");
    }
}
