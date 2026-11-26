//! Voxel Volumes: boxes of voxels a few centimetres across, drawn with no
//! meshes by walking each pixel's ray through them on the GPU
//! (`shaders/voxels.slang`), lit as anything else is.
//!
//! A Voxel Volume is an Obj's box of voxels, its least corner at the Obj's
//! place. It is filled from its own ground (grass and flowers of voxels on
//! rolling ground, [`VoxelGround`]) and from box models stamped into it,
//! their cubes turned into the volume's voxels: a sculpted tree of 1/16 m
//! cubes becomes a tree of the volume's voxels. Its voxels are made on a
//! thread of their own whenever it changes, the last made drawn until then,
//! and kept on the GPU as sparse bricks ([`bricks`]).
//!
//! What it does not do yet (`docs/research/small-voxels.md`, stage 1):
//! its voxels cast no shadow onto meshes, blocks or the ground of a terrain,
//! though they shadow each other and take the shadows of everything else;
//! it is not turned or scaled with its Obj; and it is drawn whole, however
//! far, its far voxels drawn coarser.

pub mod bricks;
mod endless;
mod growth;
pub(crate) use endless::generated_at;
mod ground;
pub mod print;
mod ray;
mod rivers;
mod solid;
mod trees;
mod voxelize;

use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use anyhow::Result;
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_rhi::{Bindless, Buffer, Frames, Gpu, MemoryLocation, Retired, vk};

pub use ground::{GROUND_COLORS, VoxelGround};
pub use growth::{MOST_COVER_KINDS, areas_allow as voxel_areas_allow};
pub use voxear_terrain::generate::structures::AreaRules as VoxelAreaRules;
pub use print::{VoxelPrint, VoxelPrintFill, VoxelPrintPicture, VoxelPrintSettings};
pub use ray::{VoxelRayHit, raycast};
pub use rivers::{GpuVoxelWater, VoxelWaterPoint, water_at};
pub use solid::{VoxelSolids, ground_at as voxel_ground_at};
pub use voxelize::{Picture as VoxelPicture, print_model as voxel_print_model};

use crate::BoxModel;
use bricks::{Builder, Packed};

/// The most volumes a view draws, and its most voxels a metre.
pub const MAX_VOXEL_VOLUMES: usize = 64;
pub const MOST_VOXELS_A_METRE: u32 = 64;
/// The most voxels a volume measures along a side.
pub const LARGEST_VOLUME: u32 = 8192;

/// A box of small voxels on an Obj.
#[derive(Clone, Debug)]
pub struct VoxelVolume {
    /// How large it is, in metres along X, Y and Z, from the Obj's place.
    pub size: Vec3,
    /// How many voxels make a metre.
    pub voxels_per_metre: u32,
    /// How rough its voxels are, from 0 to 1.
    pub roughness: f32,
    /// Its own ground, with grass and flowers, if it has one.
    pub ground: Option<VoxelGround>,
    /// The box models stamped into it.
    pub stamps: Vec<VoxelStamp>,
    /// Whether its ground goes on without end, made on the GPU round the
    /// camera as far as `reach` metres each way; its size's Y is how tall
    /// it is, and its stamps are left out.
    pub endless: bool,
    pub reach: f32,
    /// The world generator an endless volume's ground is made by, in place
    /// of its own ground's hills: none for those.
    pub generator: Option<VoxelGenerator>,
    /// The trees scattered through an endless volume's ground, each kind as
    /// many to a hectare as it says, where the ground grows grass.
    pub trees: Vec<VoxelTrees>,
    /// The crops and ground cover grown on an endless volume's ground, each
    /// kind on the ground it says, the first that grows on a voxel column
    /// taking it, in place of the ground's own grass.
    pub cover: Vec<VoxelCover>,
    /// The World Areas its trees and cover keep within or out of, by their
    /// tags: those of its Obj's dimension, laid out from the volume's corner.
    pub areas: Arc<Vec<VoxelArea>>,
    /// The changes made to an endless volume, in the order they were made:
    /// balls carved out of it or filled with ground, and Voxel Prints
    /// printed into it, a later one over an earlier. Each column is made
    /// with those that reach it.
    pub edits: Arc<Vec<VoxelEdit>>,
    /// Whether an endless volume with a generator has its generator's sea:
    /// an Ocean at its sea level, filling the level, where the scene has no
    /// Ocean of its own (see [`crate::ocean::seas`]).
    pub sea: bool,
}

/// A change made to an endless Voxel Volume: a ball carved out of it, or
/// filled with ground, its voxels' middles within the radius; or a Voxel
/// Print printed into it, its model's origin at `at`.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelEdit {
    /// The ball's middle, or where the print's origin is, in metres from
    /// the volume's corner.
    pub at: Vec3,
    /// Its radius, in metres; for a print, how far its box reaches from
    /// `at`.
    pub radius: f32,
    /// What fills it: none for air, carved out.
    pub fill: Option<VoxelMaterial>,
    /// The print printed there in place of a ball, and how it is turned.
    pub print: Option<VoxelPrinted>,
}

/// A Voxel Print as an edit prints it: the print, and how it is turned
/// about its origin.
#[derive(Clone, Debug)]
pub struct VoxelPrinted {
    pub print: Arc<VoxelPrint>,
    pub rotation: Quat,
}

impl PartialEq for VoxelPrinted {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.print, &other.print) || self.print == other.print) && self.rotation == other.rotation
    }
}

impl VoxelEdit {
    /// A ball `radius` metres round `at`, carved out, or filled with `fill`.
    pub fn ball(at: Vec3, radius: f32, fill: Option<VoxelMaterial>) -> Self {
        Self { at, radius: radius.max(0.0), fill, print: None }
    }

    /// `print` printed with its origin at `at`, turned `rotation`.
    pub fn printed(print: Arc<VoxelPrint>, at: Vec3, rotation: Quat) -> Self {
        let (low, high) = print.bounds();
        let corner = |i: usize| {
            Vec3::new([low.x, high.x][i & 1], [low.y, high.y][(i >> 1) & 1], [low.z, high.z][i >> 2]).length()
        };
        let radius = (0..8).map(corner).fold(0.0, f32::max);
        Self { at, radius, fill: None, print: Some(VoxelPrinted { print, rotation: rotation.normalize() }) }
    }

    /// The box it reaches, least and most corners, in metres from the
    /// volume's corner.
    pub fn reach(&self) -> (Vec3, Vec3) {
        match &self.print {
            Some(printed) => printed.print.placed_bounds(self.at, printed.rotation),
            None => (self.at - self.radius, self.at + self.radius),
        }
    }

    /// Whether a print is looked at round each voxel's middle too, a
    /// quarter of a voxel toward its eight corners, in a volume of voxels
    /// `voxel` metres across: where it is turned off the quarter turns, or
    /// its voxels are smaller than the volume's, so a wall a voxel thick
    /// leaves no gap between voxels that meet only at their edges, and a
    /// thin pole is not missed. A ball never is.
    pub fn widened(&self, voxel: f32) -> bool {
        let Some(printed) = &self.print else { return false };
        let turn = voxear_core::math::Mat3::from_quat(printed.rotation);
        let quarter = turn.to_cols_array().iter().all(|a| a.abs() < 1e-4 || (a.abs() - 1.0).abs() < 1e-4);
        !quarter || printed.print.voxels_per_metre as f32 * voxel > 1.001
    }

    /// Whether voxel `cell` of a volume of voxels `voxel` metres across
    /// is in it, worked out as the GPU works it (`editHeld` in
    /// `shaders/voxel_world.slang`): 0 for not, else for a print its colour
    /// there as a place in the print's palette and one, and 1 for a ball.
    pub fn holds(&self, cell: voxear_core::math::IVec3, voxel: f32) -> u8 {
        let middle = cell.as_vec3() + 0.5;
        match &self.print {
            Some(printed) => {
                let print = &printed.print;
                let scale = print.voxels_per_metre as f32 * voxel;
                let back = printed.rotation.inverse();
                let at = |p: Vec3| {
                    print.get((back * (p - self.at / voxel) * scale + print.origin).floor().as_ivec3().to_array())
                };
                let held = at(middle);
                if held != 0 || !self.widened(voxel) {
                    return held;
                }
                (0..8)
                    .map(|k| Vec3::new((k & 1) as f32, (k >> 1 & 1) as f32, (k >> 2 & 1) as f32) * 0.5 - 0.25)
                    .map(|corner| at(middle + corner))
                    .find(|&held| held != 0)
                    .unwrap_or(0)
            }
            None => {
                let off = middle - self.at / voxel;
                let radius = self.radius / voxel;
                u8::from(off.dot(off) <= radius * radius)
            }
        }
    }
}

/// The ground an edit fills a ball with, its voxels in the shades the
/// volume's own ground or a generator's has: dirt; grass, its top the
/// grass's green; rock; sand; snow; or gravel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelMaterial {
    #[default]
    Dirt,
    Grass,
    Rock,
    Sand,
    Snow,
    Gravel,
}

impl VoxelMaterial {
    pub const ALL: [Self; 6] = [Self::Dirt, Self::Grass, Self::Rock, Self::Sand, Self::Snow, Self::Gravel];

    /// As the editor and scenes name it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt",
            Self::Grass => "Grass",
            Self::Rock => "Rock",
            Self::Sand => "Sand",
            Self::Snow => "Snow",
            Self::Gravel => "Gravel",
        }
    }

    /// The one named so, as [`name`](Self::name) gives it.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|material| material.name().eq_ignore_ascii_case(name))
    }
}

/// A kind of tree scattered through an endless Voxel Volume's ground: a
/// plant of blocks (a `.voxplant`), each block a metre of the volume's
/// voxels, grown from a few seeds and turned four ways, so no two trees
/// side by side are alike. Its logs are round, as thick as the plant grows
/// them; its leaves a thin shell of loose cubes over its crown.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelTrees {
    pub plant: Arc<voxear_terrain::blocks::plant::PlantDef>,
    /// How many to a hectare where the ground grows grass.
    pub per_hectare: f32,
    /// The colours of its wood and its leaves, as sRGB.
    pub wood: [u8; 3],
    pub leaves: [u8; 3],
    /// The World Areas it grows in, by their tags, and those it never grows
    /// in: anywhere where it names none.
    pub areas: VoxelAreaRules,
}

/// A World Area as an endless Voxel Volume's trees and cover keep to it:
/// its outline along X and Z, the lowest and highest ground it holds, in
/// metres from the volume's corner and floor (none for any), and its tags
/// by their assets' numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelArea {
    pub outline: Vec<[f32; 2]>,
    pub low: Option<f32>,
    pub high: Option<f32>,
    pub tags: Vec<u128>,
}

/// A kind of crop or ground cover grown on an endless Voxel Volume's ground,
/// of its voxels: stalks a voxel across with heads of grain or flowers,
/// or round heads of leaves, in rows or scattered.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelCover {
    pub kind: VoxelCoverKind,
    /// The ground it grows on: grass where none.
    pub on: Vec<VoxelMaterial>,
    /// How much of the ground it takes grows a stalk or a head, from 0 to 1.
    pub density: f32,
    /// How tall it grows, in metres: for cabbages, how wide a head is.
    pub height: f32,
    /// How far apart its rows are, in metres, along X: 0 for none,
    /// scattered as grass is.
    pub rows: f32,
    /// What the ground it grows on is turned to, as a ploughed field's
    /// earth: none to leave the ground as it is.
    pub soil: Option<VoxelMaterial>,
    /// The World Areas it grows in, by their tags, and those it never grows
    /// in: anywhere where it names none.
    pub areas: VoxelAreaRules,
}

/// What a kind of cover is, each of its own voxels and colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelCoverKind {
    /// Tall grass gone to seed, scattered.
    #[default]
    MeadowGrass,
    /// Straw stalks with golden ears, in rows.
    Wheat,
    /// Paler and shorter than wheat, with longer ears.
    Barley,
    /// Thin green stems with blue flowers.
    Flax,
    /// Round heads of leaves, in rows.
    Cabbage,
}

impl VoxelCoverKind {
    pub const ALL: [Self; 5] = [Self::MeadowGrass, Self::Wheat, Self::Barley, Self::Flax, Self::Cabbage];

    /// As the editor names it.
    pub fn name(self) -> &'static str {
        match self {
            Self::MeadowGrass => "Meadow Grass",
            Self::Wheat => "Wheat",
            Self::Barley => "Barley",
            Self::Flax => "Flax",
            Self::Cabbage => "Cabbage",
        }
    }

    /// The one named so.
    pub fn named(name: &str) -> Option<Self> {
        let flat = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        Self::ALL.into_iter().find(|kind| flat(kind.name()) == flat(name))
    }

    /// How tall it grows, in metres, how far apart its rows are, and how
    /// much of its ground it takes, as a new kind is given them.
    pub fn defaults(self) -> (f32, f32, f32) {
        match self {
            Self::MeadowGrass => (0.55, 0.0, 0.45),
            Self::Wheat => (0.9, 0.25, 0.7),
            Self::Barley => (0.75, 0.25, 0.7),
            Self::Flax => (0.65, 0.2, 0.6),
            Self::Cabbage => (0.3, 0.55, 0.85),
        }
    }
}

impl VoxelCover {
    /// A kind as it is first grown: on grass, its own height, rows and
    /// density, crops in ploughed earth, anywhere.
    pub fn of(kind: VoxelCoverKind) -> Self {
        let (height, rows, density) = kind.defaults();
        let soil = (kind != VoxelCoverKind::MeadowGrass).then_some(VoxelMaterial::Dirt);
        Self { kind, on: vec![VoxelMaterial::Grass], density, height, rows, soil, areas: VoxelAreaRules::default() }
    }
}

/// A world generator an endless Voxel Volume's ground is made by, run on
/// the GPU a voxel column at a time: the generator, how high its ground is
/// at the volume's floor, in metres of its world, and where the volume's
/// corner lies in its world, along X and Z.
///
/// Its height, climate and rock decide each column: grass where the ground
/// would grow it (with the volume's own ground's grass and flowers), rock
/// where it is steep or the generator says, sand along the shore and in hot
/// dry places, snow below freezing. Its overhangs and caves, and its rivers,
/// are left out (see `voxear_terrain::generate::GpuProgram`).
#[derive(Clone, Debug)]
pub struct VoxelGenerator {
    pub world: Arc<voxear_terrain::generate::WorldGen>,
    pub floor: f32,
    pub at: [f32; 2],
    /// A world of blocks' biomes, whose blocks its ground is made of: none
    /// for ground of grass, rock, sand, snow and gravel by its climate.
    pub biomes: Option<Arc<VoxelBiomes>>,
}

impl PartialEq for VoxelGenerator {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.world, &other.world) || self.world.hash() == other.world.hash())
            && self.floor == other.floor
            && self.at == other.at
            && self.biomes == other.biomes
    }
}

/// A world of blocks' biomes as an endless volume lays their ground: which
/// is where, and what blocks each lays (see `voxear_terrain::generate::GpuBiomes`),
/// with each block's colour, as sRGB, in the order the words name them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoxelBiomes {
    pub words: Vec<u32>,
    pub colours: Vec<[u8; 3]>,
}

impl VoxelBiomes {
    fn number(&self, at: usize) -> f32 {
        f32::from_bits(self.words[at])
    }

    /// The painted biome map's pixel `[px, pz]`, where it is painted
    /// (`paintedPixel` in `shaders/generator.slang`).
    fn painted(&self, map: usize, [px, pz]: [i32; 2]) -> Option<f32> {
        let w = &self.words[map..];
        let (tx, tz) = ((px >> 6) - w[1] as i32, (pz >> 6) - w[2] as i32);
        let (across, down) = (w[3], w[4]);
        if tx < 0 || tz < 0 || tx as u32 >= across || tz as u32 >= down {
            return None;
        }
        let place = w[5 + tx as usize + across as usize * tz as usize];
        if place == u32::MAX {
            return None;
        }
        let at = 5 + (across * down) as usize + place as usize * 4096 + ((pz & 63) * 64 + (px & 63)) as usize;
        Some(f32::from_bits(w[at])).filter(|value| !value.is_nan())
    }

    /// Which biome the place at `(x, z)` of the generator's world is, in
    /// metres, with its climate there, as the GPU picks it (`biomeOf` in
    /// `shaders/voxel_world.slang`): by its place in their list.
    pub fn biome_of(&self, x: f32, z: f32, warmth: f32, wetness: f32, above_sea: f32) -> Option<usize> {
        use voxear_terrain::noise::{NoiseStyle, fractal, gradient};
        let count = *self.words.first()? as usize;
        if count == 0 {
            return None;
        }
        if self.words[9] != u32::MAX {
            let map = self.words[9] as usize;
            let spacing = f32::from_bits(self.words[map]);
            let size = spacing * 3.0;
            let wx = (fractal(NoiseStyle::Fractal, x / size, z / size, 2, 0x51) * 2.0 - 1.0) * spacing * 1.5;
            let wz = (fractal(NoiseStyle::Fractal, x / size, z / size, 2, 0x7a) * 2.0 - 1.0) * spacing * 1.5;
            let pixel = [((x + wx) / spacing).floor() as i32, ((z + wz) / spacing).floor() as i32];
            if self.words[map + 3] != 0
                && let Some(value) = self.painted(map, pixel)
            {
                let index = value.round();
                if index >= 0.0 && index < count as f32 {
                    return Some(index as usize);
                }
            }
        }
        let wander = self.number(1);
        let (mut temperature, mut humidity) = (warmth, wetness);
        if wander > 0.0 {
            let (u, v) = (x / self.number(2), z / self.number(2));
            let seed = self.words[3];
            let off = |part: u32| {
                (gradient(u, v, seed ^ part) + 0.4 * gradient(u * 3.7, v * 3.7, seed ^ part ^ 0x9E37_79B9)) * wander
            };
            temperature += off(1) * self.number(4);
            humidity += off(2) * self.number(5);
        }
        let outside =
            |low: f32, high: f32, value: f32, spread: f32| (low - value).max(value - high).max(0.0) / spread.max(1e-6);
        let mut best: Option<(usize, f32, f32)> = None;
        for i in 0..count {
            let r = self.words[8] as usize + i * voxear_terrain::generate::GPU_BIOME_WORDS;
            let t = outside(self.number(r), self.number(r + 1), temperature, self.number(4));
            let h = outside(self.number(r + 2), self.number(r + 3), humidity, self.number(5));
            let y = outside(self.number(r + 4), self.number(r + 5), above_sea, self.number(6));
            let (away, reach) = (t * t + h * h + y * y, self.number(r + 6));
            if best.is_none_or(|(_, least, narrowest)| away < least || (away == least && reach < narrowest)) {
                best = Some((i, away, reach));
            }
        }
        best.map(|(i, ..)| i)
    }

    /// Whether grass and trees grow on biome `biome`'s ground where it is
    /// rocky, rises `across` voxels over four and stands `above_sea` metres
    /// over the sea, as the GPU has it (`growsOn`): on its own ground, not
    /// its steep ground's or its shore's, where its top block is green.
    pub fn grows(&self, biome: usize, rock: bool, across: i32, above_sea: f32) -> bool {
        let r = self.words[8] as usize + biome * voxear_terrain::generate::GPU_BIOME_WORDS;
        let steep = self.words[r + 10] != u32::MAX && across as f32 / 4.0 >= self.number(r + 11);
        let shore = self.words[r + 12] != u32::MAX && above_sea.abs() <= self.number(r + 13);
        let top = self.words[r + 7];
        let green = self
            .colours
            .get(top as usize)
            .is_some_and(|&[red, g, b]| g > red.saturating_add(8) && g > b.saturating_add(8));
        !rock && !steep && !shore && top != u32::MAX && green
    }
}

impl Default for VoxelVolume {
    fn default() -> Self {
        Self {
            size: Vec3::new(16.0, 8.0, 16.0),
            voxels_per_metre: 16,
            roughness: 0.8,
            ground: Some(VoxelGround::default()),
            stamps: Vec::new(),
            endless: false,
            reach: 64.0,
            generator: None,
            trees: Vec::new(),
            cover: Vec::new(),
            areas: Arc::default(),
            edits: Arc::default(),
            sea: true,
        }
    }
}

impl Component for VoxelVolume {}

impl PartialEq for VoxelVolume {
    fn eq(&self, other: &Self) -> bool {
        self.size == other.size
            && self.voxels_per_metre == other.voxels_per_metre
            && self.roughness == other.roughness
            && self.ground == other.ground
            && self.stamps == other.stamps
            && self.endless == other.endless
            && self.reach == other.reach
            && self.generator == other.generator
            && self.trees == other.trees
            && self.cover == other.cover
            && (Arc::ptr_eq(&self.areas, &other.areas) || self.areas == other.areas)
            && (Arc::ptr_eq(&self.edits, &other.edits) || self.edits == other.edits)
    }
}

/// A box model stamped into a volume: its cubes made the volume's voxels,
/// its feet at `at` metres from the volume's corner, turned `turn` degrees
/// about Y and grown `scale` times.
#[derive(Clone, Debug)]
pub struct VoxelStamp {
    pub model: Arc<BoxModel>,
    pub at: Vec3,
    pub turn: f32,
    pub scale: f32,
}

impl PartialEq for VoxelStamp {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.model, &other.model) || self.model == other.model)
            && self.at == other.at
            && self.turn == other.turn
            && self.scale == other.scale
    }
}

impl VoxelVolume {
    /// How high its generator's sea stands in the world, its corner at
    /// `corner`: none unless it is endless, has a generator and its Sea on,
    /// nor where the sea is under its floor.
    pub fn sea_height(&self, corner: Vec3) -> Option<f32> {
        let generator = self.generator.as_ref().filter(|_| self.endless && self.sea)?;
        let over_floor = generator.world.sea_level() - generator.floor;
        (over_floor > 0.0).then_some(corner.y + over_floor)
    }

    /// How many voxels it measures along each side.
    pub fn voxel_size(&self) -> [u32; 3] {
        let per = self.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
        self.size.to_array().map(|metres| ((metres.max(0.0) * per).round() as u32).clamp(1, LARGEST_VOLUME))
    }

    /// Its voxels, made from its ground and stamps, and the palette they
    /// take their colours from.
    pub fn build(&self) -> (Packed, Vec<[u8; 4]>) {
        let size = self.voxel_size();
        let per_metre = self.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
        let mut builder = Builder::default();
        let mut palette: Vec<[u8; 4]> = GROUND_COLORS.to_vec();
        if let Some(ground) = &self.ground {
            ground.build(&mut builder, size, per_metre);
        }
        let mut places: HashMap<[u8; 4], u8> = HashMap::new();
        for stamp in &self.stamps {
            stamp_into(&mut builder, &mut palette, &mut places, stamp, per_metre);
        }
        let packed = builder.pack(size, &palette);
        (packed, palette)
    }
}

/// The palette place and one of `color`, added where there is room, else
/// the nearest the palette has.
fn place_of(palette: &mut Vec<[u8; 4]>, places: &mut HashMap<[u8; 4], u8>, color: [u8; 4]) -> u8 {
    if let Some(&place) = places.get(&color) {
        return place;
    }
    let place = if palette.len() < 255 {
        palette.push(color);
        palette.len() as u8
    } else {
        let distance = |c: &[u8; 4]| (0..3).map(|i| (i32::from(c[i]) - i32::from(color[i])).pow(2)).sum::<i32>();
        let nearest = palette.iter().enumerate().min_by_key(|(_, c)| distance(c)).map_or(0, |(i, _)| i);
        nearest as u8 + 1
    };
    places.insert(color, place);
    place
}

/// Sets a box model's cubes into `into`: each cube fills the voxels whose
/// middles fall inside it, as the model stands at rest.
fn stamp_into(
    into: &mut Builder,
    palette: &mut Vec<[u8; 4]>,
    places: &mut HashMap<[u8; 4], u8>,
    stamp: &VoxelStamp,
    per_metre: f32,
) {
    let model = &stamp.model;
    let placed = Mat4::from_scale_rotation_translation(
        Vec3::splat(stamp.scale.max(0.01)),
        Quat::from_rotation_y(stamp.turn.to_radians()),
        stamp.at,
    );
    let bones = model.rest_matrices();
    for (bone, matrix) in model.bones.iter().zip(&bones) {
        for model_box in &bone.boxes {
            let Some(voxels) = &model_box.voxels else { continue };
            let (place, reach) = model.box_place(matrix, model_box);
            let to_volume = placed * place;
            let size = model_box.size.map(|a| a.max(1));
            let each = reach * 2.0 / Vec3::new(size[0] as f32, size[1] as f32, size[2] as f32);
            // How far a cube reaches each way in the volume, turned however it is: its corners' reach.
            let half = {
                let x = to_volume.transform_vector3(Vec3::X * each.x * 0.5).abs();
                let y = to_volume.transform_vector3(Vec3::Y * each.y * 0.5).abs();
                let z = to_volume.transform_vector3(Vec3::Z * each.z * 0.5).abs();
                x + y + z
            };
            voxels.each(|cell, color| {
                let local = -reach + (Vec3::new(cell[0] as f32, cell[1] as f32, cell[2] as f32) + 0.5) * each;
                let middle = to_volume.transform_point3(local) * per_metre;
                let place = place_of(palette, places, model.voxel_color(color));
                let low = (middle - half * per_metre).round();
                let high = (middle + half * per_metre).round();
                let (low, high) = (low.as_ivec3(), high.as_ivec3().max(low.as_ivec3() + 1));
                for z in low.z..high.z {
                    for y in low.y..high.y {
                        for x in low.x..high.x {
                            into.set([x, y, z], place);
                        }
                    }
                }
            });
        }
    }
}

/// A volume as the shaders read it (`VoxelVolume` in shaders/common.slang).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuVoxelVolume {
    sectors: u64,
    bricks: u64,
    materials: u64,
    palette: u64,
    sector_count: [i32; 3],
    voxel: f32,
    corner: [f32; 3],
    roughness: f32,
    window_sector: [i32; 3],
    windowed: u32,
    empty_sector: u64,
    pages: u64,
}

/// A view's volumes (`VoxelVolumes` in shaders/common.slang).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuVoxelVolumes {
    count: u32,
    coarse_step: u32,
    coarse_size: [u32; 2],
    coarse: u64,
    lod_scale: f32,
    seen_on: u32,
    seen: u64,
    seen_width: u32,
    pad: u32,
    volumes: [GpuVoxelVolume; MAX_VOXEL_VOLUMES],
}

/// The size of a view's frame buffer of volumes.
pub(crate) const FRAME_SIZE: u64 = size_of::<GpuVoxelVolumes>() as u64;

/// One coarse ray every this many pixels each way.
pub(crate) const COARSE_STEP: u32 = 4;
/// The coarse rays a view keeps room for: enough for a picture 7680 pixels
/// across and 4320 tall; a larger one walks every ray from the camera.
pub(crate) const COARSE_ROOM: u64 = (7680 / COARSE_STEP as u64 + 2) * (4320 / COARSE_STEP as u64 + 2);

/// How many voxels to a pixel the walk stops at coarser levels from, as a
/// share of the plain footprint: groups from half a pixel.
const LOD_SCALE: f32 = 2.0;

/// A volume's voxels as the GPU has them.
struct Resident {
    buffer: Buffer,
    record: GpuVoxelVolume,
    bytes: u64,
    bricks: usize,
}

/// A volume's voxels being made, and what they are made from.
struct Making {
    from: VoxelVolume,
    job: JoinHandle<(Packed, Vec<[u8; 4]>)>,
}

/// Each volume's voxels on the GPU, made again on a thread of their own as
/// a volume changes.
#[derive(Default)]
pub(crate) struct VoxelVolumes {
    resident: HashMap<Entity, (VoxelVolume, Resident)>,
    making: HashMap<Entity, Making>,
    /// Endless volumes, made on the GPU round the camera.
    endless: HashMap<Entity, endless::Endless>,
    /// Each volume drawn this frame and its corner.
    drawn: Vec<(GpuVoxelVolume, Vec3)>,
    /// The water over endless volumes' rivers: how many times each one's
    /// changed as last seen, a count of the changes to all of it, and for
    /// each frame slot its buffer, which change it holds and how many squares.
    water_seen: Vec<(Entity, u64)>,
    water_version: u64,
    water: Vec<(Option<Buffer>, u64, u32, u32)>,
}

/// How many volumes are drawn, and their bricks and bytes on the GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoxelCounts {
    pub volumes: u32,
    pub bricks: u64,
    pub bytes: u64,
    /// How many squares of water lie over endless volumes' rivers and lakes.
    pub water_squares: u32,
    /// How many columns of endless volumes are made this frame.
    pub columns_made: u32,
}

impl VoxelVolumes {
    /// Starts making the voxels of volumes that changed, brings up those
    /// made, and lets go of those gone.
    pub fn prepare(&mut self, gpu: &Gpu, frames: &mut Frames, world: &World) -> Result<()> {
        voxear_core::profile_scope!("Voxel Volumes");
        self.drawn.clear();
        let mut seen = Vec::new();
        let found: Vec<(Entity, VoxelVolume, Vec3)> = world
            .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
            .without::<voxear_scene::Destroying>()
            .without::<voxear_scene::Hidden>()
            .into_iter()
            .map(|(entity, volume, local, placed)| {
                (entity, volume.clone(), placed.map_or(local.position, |placed| placed.0.w_axis.truncate()))
            })
            .collect();
        let camera = camera_of(world);
        for (entity, volume, corner) in found {
            seen.push(entity);
            if volume.endless {
                let fits = self.endless.get(&entity).map(|made| endless::same_ground(&made.from, &volume));
                let refitted = match (fits, self.endless.get_mut(&entity)) {
                    (Some(false), Some(made)) => made.refit(gpu, &volume)?,
                    _ => None,
                };
                match refitted {
                    Some(gone) => gone.into_iter().for_each(|buffer| frames.retire(Retired::Buffer(buffer))),
                    None if fits != Some(true) => {
                        let made = endless::Endless::new(gpu, &volume)?;
                        if let Some(old) = self.endless.insert(entity, made) {
                            old.into_buffers().into_iter().for_each(|buffer| frames.retire(Retired::Buffer(buffer)));
                        }
                    }
                    None => {}
                }
                // A room that ran short is made again half as large again: the window is made over.
                if let Some(grow) = self.endless.get(&entity).and_then(endless::Endless::short) {
                    tracing::info!("an endless Voxel Volume's room ran short: made again with half as much again");
                    let made = endless::Endless::grown(gpu, &volume, grow * 1.5)?;
                    if let Some(old) = self.endless.insert(entity, made) {
                        old.into_buffers().into_iter().for_each(|buffer| frames.retire(Retired::Buffer(buffer)));
                    }
                }
                let made = self.endless.get_mut(&entity).expect("made above");
                made.from.roughness = volume.roughness;
                // New edits make the columns they reach again.
                for buffer in made.set_edits(gpu, &volume.edits)? {
                    frames.retire(Retired::Buffer(buffer));
                }
                made.follow(corner, camera.unwrap_or(corner));
                self.drawn.push(made.drawn());
                continue;
            }
            let current = self.resident.get(&entity).is_some_and(|(made, _)| *made == volume);
            let pending = self.making.get(&entity).is_some_and(|making| making.from == volume);
            if !current && !pending {
                let from = volume.clone();
                let job = std::thread::Builder::new().name("Voxel Volume".into()).spawn(move || from.build())?;
                self.making.insert(entity, Making { from: volume.clone(), job });
            }
            if self.making.get(&entity).is_some_and(|making| making.job.is_finished()) {
                let making = self.making.remove(&entity).expect("found above");
                let (packed, palette) =
                    making.job.join().map_err(|_| anyhow::anyhow!("making a Voxel Volume failed"))?;
                let resident = upload(gpu, &making.from, &packed, &palette)?;
                if let Some((_, old)) = self.resident.insert(entity, (making.from, resident)) {
                    frames.retire(Retired::Buffer(old.buffer));
                }
            }
            if let Some((_, resident)) = self.resident.get(&entity) {
                self.drawn.push((resident.record, corner));
            }
        }
        let gone: Vec<Entity> = self.endless.keys().filter(|entity| !seen.contains(entity)).copied().collect();
        for entity in gone {
            if let Some(made) = self.endless.remove(&entity) {
                made.into_buffers().into_iter().for_each(|buffer| frames.retire(Retired::Buffer(buffer)));
            }
        }
        // An endless volume keeps no voxels of its own: what it made before it became one is let go.
        let endless: Vec<Entity> = self.endless.keys().copied().collect();
        for entity in endless {
            if let Some((_, resident)) = self.resident.remove(&entity) {
                frames.retire(Retired::Buffer(resident.buffer));
            }
        }
        let gone: Vec<Entity> = self.resident.keys().filter(|entity| !seen.contains(entity)).copied().collect();
        for entity in gone {
            if let Some((_, resident)) = self.resident.remove(&entity) {
                frames.retire(Retired::Buffer(resident.buffer));
            }
        }
        self.making.retain(|entity, _| seen.contains(entity));
        Ok(())
    }

    /// Whether any volume is still being made, or an endless one has columns
    /// round the camera still to make.
    pub fn making(&self) -> bool {
        !self.making.is_empty()
            || self.endless.values().any(|made| made.unfinished() || made.short().is_some())
    }

    /// Records the passes that make this frame's columns of endless volumes,
    /// for frame slot `slot`, and gives the buffers they write, which the
    /// voxel passes read.
    pub fn record(&self, fg: &mut FrameGraph<'_>, pipelines: Pipelines, slot: usize) -> Vec<BufferId> {
        let mut pools = Vec::new();
        for made in self.endless.values() {
            let ids: Vec<BufferId> = made.pools().iter().map(|buffer| fg.import_buffer(buffer)).collect();
            if let Some(make) = made.write(slot) {
                let writes: Vec<_> = ids.iter().map(|&id| (id, BufferAccess::ComputeWrite)).collect();
                let recording = made.recording();
                fg.add_pass("Voxel world making", &[], &writes, move |ctx| {
                    recording.record(ctx.gpu, ctx.cmd, (&pipelines, ctx.bindless), make);
                });
            }
            pools.extend(ids);
        }
        pools
    }

    /// Makes what is left to make of the endless volumes round the camera
    /// now, outside the frames, as a game's loading does.
    pub fn make_endless_now(&self, gpu: &Gpu, (pipelines, bindless): (&Pipelines, &Bindless)) -> Result<()> {
        for made in self.endless.values() {
            if let Some(make) = made.write(voxear_rhi::FRAMES_IN_FLIGHT) {
                let recording = made.recording();
                gpu.immediate(|cmd| recording.record(gpu, cmd, (pipelines, bindless), make))?;
            }
        }
        Ok(())
    }

    pub fn counts(&self) -> VoxelCounts {
        let mut counts = VoxelCounts { volumes: self.drawn.len() as u32, ..Default::default() };
        for (_, resident) in self.resident.values() {
            counts.bricks += resident.bricks as u64;
            counts.bytes += resident.bytes;
        }
        counts.bytes += self.endless.values().map(endless::Endless::bytes).sum::<u64>();
        counts.water_squares = self.endless.values().map(|made| made.water().0.0.count() as u32).sum();
        counts.columns_made = self.endless.values().map(endless::Endless::making_now).sum();
        counts
    }

    /// Each volume drawn this frame, and its corner, for every view.
    pub fn drawn(&self) -> std::sync::Arc<[(GpuVoxelVolume, Vec3)]> {
        self.drawn.iter().copied().collect()
    }

    /// The water over endless volumes' rivers and lakes for frame slot
    /// `slot`, as every view draws it: its buffer's address, how many
    /// squares it holds and then how many of waterfalls, written again where
    /// it changed.
    pub fn water(&mut self, gpu: &Gpu, slot: usize) -> Result<(u64, u32, u32)> {
        let mut seen: Vec<(Entity, u64)> =
            self.endless.iter().map(|(entity, made)| (*entity, made.water().1)).collect();
        seen.sort_unstable_by_key(|(entity, _)| entity.to_bits());
        if seen != self.water_seen {
            self.water_seen = seen;
            self.water_version += 1;
        }
        if self.water.len() <= slot {
            self.water.resize_with(slot + 1, || (None, 0, 0, 0));
        }
        let version = self.water_version;
        if self.water[slot].1 != version {
            let mut squares: Vec<GpuVoxelWater> =
                self.endless.values().flat_map(|made| made.water().0.0.copied()).collect();
            let still = squares.len() as u32;
            squares.extend(self.endless.values().flat_map(|made| made.water().0.1.copied()));
            let bytes = (squares.len() * size_of::<GpuVoxelWater>()).max(16) as u64;
            let (buffer, _, _, _) = &mut self.water[slot];
            if buffer.as_ref().is_none_or(|buffer| buffer.size < bytes) {
                let usage = vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS;
                let name = format!("voxel water {slot}");
                // This slot's last frame is done with it.
                if let Some(old) =
                    buffer.replace(gpu.create_buffer(&name, bytes * 3 / 2, usage, MemoryLocation::CpuToGpu)?)
                {
                    gpu.destroy_buffer(old);
                }
            }
            if let Some(buffer) = buffer.as_ref().filter(|_| !squares.is_empty()) {
                buffer.write(0, &squares);
            }
            self.water[slot] = (self.water[slot].0.take(), version, still, squares.len() as u32 - still);
        }
        let (buffer, _, still, falls) = &self.water[slot];
        Ok((buffer.as_ref().map_or(0, |buffer| buffer.address), *still, *falls))
    }
}

/// Writes a view's volumes, `drawn`, into `buffer`, their corners `offset`
/// from where they are (a view through a Portal's), with its coarse rays in
/// `coarse` for a picture `size` pixels: what the view's voxel passes draw,
/// or `None` with none.
pub(crate) fn gather(
    drawn: &[(GpuVoxelVolume, Vec3)],
    (buffer, coarse, seen): (&Buffer, &Buffer, Option<&Buffer>),
    size: [u32; 2],
    offset: Vec3,
) -> Option<ViewVoxels> {
    {
        if drawn.is_empty() {
            return None;
        }
        let coarse_size = [size[0] / COARSE_STEP + 2, size[1] / COARSE_STEP + 2];
        let fits = u64::from(coarse_size[0]) * u64::from(coarse_size[1]) <= COARSE_ROOM;
        let mut volumes = GpuVoxelVolumes {
            count: drawn.len().min(MAX_VOXEL_VOLUMES) as u32,
            coarse_step: if fits { COARSE_STEP } else { 0 },
            coarse_size,
            coarse: coarse.address,
            lod_scale: LOD_SCALE,
            seen_on: u32::from(seen.is_some()),
            seen: seen.map_or(0, |seen| seen.address),
            seen_width: size[0],
            pad: 0,
            volumes: [GpuVoxelVolume::default(); MAX_VOXEL_VOLUMES],
        };
        // The finest first, so the coarse rays' margin is a brick of the coarsest that is nearest the camera.
        let mut drawn: Vec<_> = drawn.iter().take(MAX_VOXEL_VOLUMES).collect();
        drawn.sort_by(|a, b| b.0.voxel.total_cmp(&a.0.voxel));
        for (slot, (record, corner)) in drawn.into_iter().enumerate() {
            volumes.volumes[slot] = GpuVoxelVolume { corner: (*corner - offset).to_array(), ..*record };
        }
        buffer.write(0, std::slice::from_ref(&volumes));
        Some(ViewVoxels { coarse_size, coarse_on: fits, coarse: None, seen: None, pools: Vec::new() })
    }
}

impl VoxelVolumes {
    /// Lets go of every volume's voxels.
    pub fn destroy(&mut self, gpu: &Gpu) {
        for (_, (_, resident)) in self.resident.drain() {
            gpu.destroy_buffer(resident.buffer);
        }
        for (_, making) in self.making.drain() {
            let _ = making.job.join();
        }
        for (_, made) in self.endless.drain() {
            made.destroy(gpu);
        }
        for (buffer, _, _, _) in self.water.drain(..) {
            buffer.into_iter().for_each(|buffer| gpu.destroy_buffer(buffer));
        }
    }
}

/// Where the first camera of `world` is, which endless volumes are made
/// round.
fn camera_of(world: &World) -> Option<Vec3> {
    world
        .query_ref::<(&crate::Camera, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .next()
        .map(|(_, local, placed)| placed.map_or(local.position, |placed| placed.0.w_axis.truncate()))
}

/// What a view's voxel passes draw: its coarse rays, if the picture is not
/// too large for them.
#[derive(Clone, Debug)]
pub(crate) struct ViewVoxels {
    pub coarse_size: [u32; 2],
    pub coarse_on: bool,
    /// The coarse rays' buffer in the view's frame graph, and what the
    /// depth pass met in each pixel, where it is drawn.
    pub coarse: Option<crate::graph::BufferId>,
    pub seen: Option<crate::graph::BufferId>,
    /// What endless volumes' making writes this frame, which the voxel
    /// passes read.
    pub pools: Vec<crate::graph::BufferId>,
}

/// The bytes a view keeps of what its depth pass met, for a picture `size`
/// pixels.
pub(crate) fn seen_bytes(size: [u32; 2]) -> u64 {
    u64::from(size[0]) * u64::from(size[1]) * 16
}

/// A volume's sectors, bricks, materials and palette in one buffer on the GPU.
fn upload(gpu: &Gpu, volume: &VoxelVolume, packed: &Packed, palette: &[[u8; 4]]) -> Result<Resident> {
    let align = |n: usize| n.div_ceil(16) * 16;
    let sectors = bytemuck::cast_slice::<u32, u8>(&packed.sectors);
    let bricks = bytemuck::cast_slice::<u32, u8>(&packed.bricks);
    let colours: Vec<u32> =
        palette.iter().map(|c| u32::from(c[0]) | u32::from(c[1]) << 8 | u32::from(c[2]) << 16).collect();
    let colours = bytemuck::cast_slice::<u32, u8>(&colours);
    let at = [0, align(sectors.len()), align(sectors.len()) + align(bricks.len())];
    let palette_at = at[2] + align(packed.materials.len());
    let total = (palette_at + align(colours.len())).max(16);
    let buffer = gpu.create_buffer(
        "voxel volume",
        total as u64,
        vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        MemoryLocation::GpuOnly,
    )?;
    for (offset, bytes) in [(at[0], sectors), (at[1], bricks), (at[2], &packed.materials[..]), (palette_at, colours)] {
        for (i, piece) in bytes.chunks(64 << 20).enumerate() {
            gpu.upload_buffer(&buffer, (offset + i * (64 << 20)) as u64, piece)?;
        }
    }
    let base = buffer.address;
    let record = GpuVoxelVolume {
        sectors: base + at[0] as u64,
        bricks: base + at[1] as u64,
        materials: base + at[2] as u64,
        palette: base + palette_at as u64,
        sector_count: packed.sector_count,
        voxel: 1.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32,
        corner: [0.0; 3],
        roughness: volume.roughness.clamp(0.0, 1.0),
        window_sector: [0; 3],
        windowed: 0,
        empty_sector: 0,
        pages: 0,
    };
    Ok(Resident { buffer, record, bytes: total as u64, bricks: packed.brick_count() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::IVec3;

    #[test]
    fn the_gpu_records_are_laid_out_as_the_shaders_read_them() {
        assert_eq!(size_of::<GpuVoxelVolume>(), 96);
        assert_eq!(size_of::<GpuVoxelVolumes>(), 48 + 96 * MAX_VOXEL_VOLUMES);
    }

    #[test]
    fn a_print_turned_off_the_quarter_turns_leaves_no_gap_in_a_wall_a_voxel_thick() {
        // A wall a voxel thick, 4 m long along Z, its origin at its foot's middle.
        let wall = VoxelPrint::from_voxels(
            [1, 8, 64],
            16,
            Vec3::new(0.5, 0.0, 32.0),
            vec![[200; 4]],
            Default::default(),
            |_| 1,
        );
        let wall = Arc::new(wall);
        let voxel = 1.0 / 16.0;
        for degrees in [0.0f32, 90.0, 20.0, 30.0, 45.0, 60.0] {
            let at = Vec3::new(4.03, 0.0, 4.01);
            let edit = VoxelEdit::printed(wall.clone(), at, Quat::from_rotation_y(degrees.to_radians()));
            assert_eq!(edit.widened(voxel), degrees % 90.0 != 0.0, "{degrees}°");
            // A row of voxels a voxel up, every one the wall holds.
            let held: std::collections::HashSet<(i32, i32)> = (0..128)
                .flat_map(|z| (0..128).map(move |x| (x, z)))
                .filter(|&(x, z)| edit.holds(IVec3::new(x, 1, z), voxel) != 0)
                .collect();
            // Seen from the side the wall is whole: its voxels all meet face to face, one run end to end.
            let first = *held.iter().next().expect("the wall");
            let mut reached = std::collections::HashSet::from([first]);
            let mut next = vec![first];
            while let Some((x, z)) = next.pop() {
                for step in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let near = (x + step.0, z + step.1);
                    if held.contains(&near) && reached.insert(near) {
                        next.push(near);
                    }
                }
            }
            assert_eq!(reached.len(), held.len(), "{degrees}°: the wall's voxels meet face to face");
            // And it keeps its length, thicker by no more than a voxel or so where turned.
            let most = if degrees % 90.0 == 0.0 { 64 } else { 64 * 3 };
            assert!(held.len() >= 60 && held.len() <= most, "{degrees}°: {} voxels", held.len());
        }
    }

    #[test]
    fn a_volumes_own_ground_has_grass_on_it_and_nothing_under_its_lowest_neighbour() {
        let volume = VoxelVolume {
            size: Vec3::new(4.0, 4.0, 4.0),
            ground: Some(VoxelGround { grass: 1.0, flowers: 0.0, hills: 0.0, ..VoxelGround::default() }),
            ..VoxelVolume::default()
        };
        let (packed, palette) = volume.build();
        assert_eq!(palette.len(), GROUND_COLORS.len());
        // Flat ground two metres up: its top at voxel 32, a blade over it everywhere.
        assert_eq!(packed.get([10, 32, 10]), 1, "the ground's top");
        assert!(packed.get([10, 33, 10]) >= 3, "a blade on it");
        assert_eq!(packed.get([10, 20, 10]), 0, "nothing deep under the ground is kept");
    }

    #[test]
    fn a_stamped_model_fills_the_volumes_voxels_its_cubes_cover() {
        let mut model = BoxModel { pixels_per_block: 8.0, ..BoxModel::default() };
        let mut bone = crate::BoxBone::new("root", None, Vec3::ZERO);
        let mut voxels = crate::Voxels::filled([2, 2, 2], 1);
        voxels.set([2, 2, 2], [1, 1, 1], 0);
        bone.boxes
            .push(crate::ModelBox { voxels: Some(voxels), ..crate::ModelBox::new(Vec3::ZERO, [2, 2, 2], [0, 0]) });
        model.bones.push(bone);
        model.palette.push([10, 20, 30, 255]);
        let volume = VoxelVolume {
            size: Vec3::splat(2.0),
            ground: None,
            stamps: vec![VoxelStamp { model: Arc::new(model), at: Vec3::splat(1.0), turn: 0.0, scale: 1.0 }],
            ..VoxelVolume::default()
        };
        let (packed, palette) = volume.build();
        // A cube of 1/8 m is two by two by two voxels of 1/16 m, from the stamp's place at voxel 16.
        let colour = palette.iter().position(|c| *c == [10, 20, 30, 255]).unwrap() as u8 + 1;
        assert_eq!(packed.get([16, 16, 16]), colour);
        assert_eq!(packed.get([17, 17, 17]), colour);
        assert_eq!(packed.get([19, 19, 19]), 0, "the cube left out stays empty");
        assert_eq!(packed.solid, 7 * 8);
    }
}

use crate::{
    gpu::Push,
    graph::{BufferAccess, BufferId, FrameGraph, ImageAccess, ImageId},
    passes::{attachment, begin_rendering, bind},
    pipelines::Pipelines,
};

/// **Voxel coarse rays**: one ray every few pixels walked against whole
/// bricks, which the voxel passes start their own rays from.
pub(crate) fn record_coarse(fg: &mut FrameGraph<'_>, (pipelines, push): (Pipelines, Push), view: &ViewVoxels) {
    let Some(coarse) = view.coarse.filter(|_| view.coarse_on) else { return };
    let size = view.coarse_size;
    let mut access = vec![(coarse, BufferAccess::ComputeWrite)];
    access.extend(view.pools.iter().map(|&pool| (pool, BufferAccess::ShaderRead)));
    fg.add_pass("Voxel coarse rays", &[], &access, move |ctx| {
        bind(ctx, pipelines.voxel_coarse, vk::PipelineBindPoint::COMPUTE, &push);
        unsafe { ctx.gpu.device.cmd_dispatch(ctx.cmd, size[0].div_ceil(8), size[1].div_ceil(8), 1) };
    });
}

fn coarse_reads(view: &ViewVoxels, buffers: &[(BufferId, BufferAccess)]) -> Vec<(BufferId, BufferAccess)> {
    let mut reads = buffers.to_vec();
    reads.extend(view.pools.iter().map(|&pool| (pool, BufferAccess::ShaderRead)));
    reads.extend(view.coarse.filter(|_| view.coarse_on).map(|coarse| (coarse, BufferAccess::ShaderRead)));
    reads
}

/// **Depth (voxels)**: the voxels in view as depth alone, with each pixel's
/// normal and roughness, over what the depth passes drew.
pub(crate) fn record_depth(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    view: &ViewVoxels,
    targets: crate::passes::DepthTargets,
) {
    let mut reads = coarse_reads(view, &[]);
    reads.extend(view.seen.map(|seen| (seen, BufferAccess::FragmentWrite)));
    fg.add_pass("Depth (voxels)", &targets.images(), &reads, move |ctx| {
        targets.begin(ctx);
        bind(ctx, pipelines.voxel_depth, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0) };
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}

/// **Opaque (voxels)**: the voxels in view lit, over what the opaque passes
/// drew, nearer than it.
pub(crate) fn record_opaque(
    fg: &mut FrameGraph<'_>,
    (pipelines, push): (Pipelines, Push),
    view: &ViewVoxels,
    images: Vec<(ImageId, ImageAccess)>,
    (hdr, velocity, depth): (ImageId, ImageId, ImageId),
    buffers: &[(BufferId, BufferAccess)],
) {
    let mut reads = coarse_reads(view, buffers);
    reads.extend(view.seen.map(|seen| (seen, BufferAccess::ShaderRead)));
    fg.add_pass("Opaque (voxels)", &images, &reads, move |ctx| {
        let layout = vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL;
        let color = [attachment(ctx.view(hdr), layout, None), attachment(ctx.view(velocity), layout, None)];
        let target = attachment(ctx.view(depth), vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, None);
        begin_rendering(ctx, ctx.extent(hdr), &color, Some(&target));
        bind(ctx, pipelines.voxel_forward, vk::PipelineBindPoint::GRAPHICS, &push);
        unsafe { ctx.gpu.device.cmd_draw(ctx.cmd, 3, 1, 0, 0) };
        unsafe { ctx.gpu.device.cmd_end_rendering(ctx.cmd) };
    });
}
