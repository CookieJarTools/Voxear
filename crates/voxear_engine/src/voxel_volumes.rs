//! Voxel Volumes as scenes write them (`[obj.voxel_volume]`), and made into
//! what the renderer draws (`voxear_render::VoxelVolume`): its stamps' box
//! models read from the project.
//!
//! ```toml
//! [obj.voxel_volume]
//! size = [32.0, 12.0, 32.0]
//! voxels_per_metre = 16
//! ground_on = true
//! ground = { height = 2.0, hills = 1.5, grass = 0.6, flowers = 0.2 }
//!
//! An endless one has `endless = true` and how far it reaches round the
//! camera, `reach = 64.0`. Its ground can be a world generator's, run on
//! the GPU (see `voxear_render::VoxelGenerator`): `generator` names a
//! `.worldgen` in the project, `generator_floor` how high the generator's
//! ground is at the volume's floor (left out, a little under its ground at
//! the volume's corner), and `generator_at` where the volume's corner lies
//! in the generator's world. Its generator's sea fills the level at the
//! generator's sea level, as an Ocean would, unless it has `sea = false` or
//! the level has an Ocean of its own (an Ocean on the volume's Obj stands
//! at that level too). Its trees are kinds of `.voxplant`, each as
//! many to a hectare as it says, scattered where its ground grows grass:
//!
//! [[obj.voxel_volume.trees]]
//! plant = "Plants/Oak.voxplant"
//! per_hectare = 40.0
//!
//! A kind of tree may keep within World Areas of some tags and out of
//! those of others, each an Area Tag data asset, by its number:
//!
//! [[obj.voxel_volume.trees]]
//! plant = "Plants/Apple.voxplant"
//! per_hectare = 120.0
//! within = ["…the Orchard tag's number…"]
//!
//! Crops and ground cover grow on the ground they say, each kind of its own
//! voxels (`meadow_grass`, `wheat`, `barley`, `flax` or `cabbage`), the
//! first that grows on a voxel column taking it in place of the grass:
//!
//! [[obj.voxel_volume.cover]]
//! kind = "wheat"
//! on = ["grass"]
//! soil = "dirt"
//! height = 0.9
//! rows = 0.25
//! density = 0.7
//! within = ["…the Fields tag's number…"]
//!
//! [[obj.voxel_volume.stamps]]
//! model = "Plants/Oak.boxmodel"
//! at = [12.0, 2.0, 9.0]
//! turn = 30.0
//! ```
//!
//! An endless one keeps what was changed in it, in order: balls carved out
//! of it, and filled with a ground (`dirt`, `grass`, `rock`, `sand`, `snow`
//! or `gravel`) or a paving (`cobbles`, `flagstones`, `bricks`, `planks` or
//! `ashlar`), each its middle in metres from the volume's corner:
//!
//! ```toml
//! [[obj.voxel_volume.edits]]
//! at = [12.0, 24.0, 9.0]
//! radius = 1.5
//!
//! [[obj.voxel_volume.edits]]
//! at = [14.0, 24.0, 9.0]
//! radius = 1.0
//! fill = "rock"
//! ```
//!
//! Boxes and banks (a `shape` of `box` or `ramp`) are placed by the middle
//! of their foot, `size` metres along their own X, Y and Z, turned as a
//! print is; an `area` is an outline's `points` along X and Z from `low` up
//! to `high`; a `cover` the ground's top `depth` metres deep within an
//! outline, wherever the ground is. Each clears `clear` metres over its top,
//! and is in `tint`'s colours where it has one:
//!
//! ```toml
//! [[obj.voxel_volume.edits]]
//! shape = "area"
//! points = [[0.0, 0.0], [40.0, 0.0], [40.0, 6.0], [0.0, 6.0]]
//! low = 23.75
//! high = 24.0
//! fill = "cobbles"
//! clear = 3.0
//!
//! [[obj.voxel_volume.edits]]
//! shape = "box"
//! at = [12.0, 21.0, 9.0]
//! size = [4.0, 2.5, 5.0]
//! rotation = [0.0, 15.0, 0.0]
//! ```
//!
//! A Voxel Print printed into it is an edit too, kept in the same order:
//! the `.voxprint`, where its model's origin is, and how it is turned, in
//! degrees about X, Y and Z, as an Obj's rotation is written; `fit` fits it
//! to the ground on a footing of its `fill`, and `tint` shades its colours:
//!
//! ```toml
//! [[obj.voxel_volume.edits]]
//! print = "Prints/Sandbag Wall.voxprint"
//! at = [10.0, 24.0, 6.0]
//! rotation = [0.0, 30.0, 0.0]
//! fit = true
//! fill = "ashlar"
//! tint = [200, 180, 150]
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_assets::AssetGuid;
use voxear_render::{
    VoxelArea, VoxelAreaRules, VoxelBiomes, VoxelChanges, VoxelCover, VoxelCoverKind, VoxelEdit, VoxelGenerator,
    VoxelCut, VoxelFelled, VoxelFelling, VoxelGround, VoxelKind, VoxelMaterial, VoxelRayMeets, VoxelStamp, VoxelTrees, VoxelVolume,
    VoxelWaterLook,
};

use crate::content::Content;

/// A Voxel Volume as a scene writes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelVolumeData {
    /// How large it is, in metres along X, Y and Z, from the Obj's place.
    pub size: [f32; 3],
    pub voxels_per_metre: u32,
    pub roughness: f32,
    /// Whether it has its own ground, and the ground it has.
    pub ground_on: bool,
    pub ground: VoxelGround,
    /// The box models stamped into it.
    pub stamps: Vec<VoxelStampData>,
    /// Whether its ground goes on without end, made round the camera as far
    /// as `reach` metres each way.
    pub endless: bool,
    pub reach: f32,
    /// How far an endless volume's ground is seen past its reach, in coarser
    /// rings, in metres each way: none at or under its reach.
    #[serde(skip_serializing_if = "is_nothing")]
    pub distance: f32,
    /// The world generator an endless volume's ground is made by, a
    /// reference to a `.worldgen`, or to a generated world of blocks for
    /// the ground its chunks are made of: none for its own ground.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
    /// How high the generator's ground is at the volume's floor, in metres
    /// of its world: none for [`auto_floor`] under its ground at the corner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generator_floor: Option<f32>,
    /// Where the volume's corner lies in the generator's world, in metres
    /// along X and Z.
    #[serde(skip_serializing_if = "is_origin")]
    pub generator_at: [f32; 2],
    /// The kinds of tree scattered through an endless volume's ground.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trees: Vec<VoxelTreesData>,
    /// The crops and ground cover grown on an endless volume's ground.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cover: Vec<VoxelCoverData>,
    /// The mobs that spawn on an endless volume's ground round the player.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mobs: Vec<crate::voxel_mobs::VoxelMobsData>,
    /// What was changed in an endless volume, in the order it was changed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<VoxelEditData>,
    /// Where the voxels dug and placed one at a time are kept, a folder of
    /// region files the scene's save writes, and which regions it has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<[i32; 2]>,
    /// Whether an endless volume with a generator has its generator's sea,
    /// where the level has no Ocean of its own: on unless written off.
    #[serde(skip_serializing_if = "is_on")]
    pub sea: bool,
    /// How an endless volume's rivers and lakes are drawn: as voxels of
    /// water, or smooth (`water = "smooth"`).
    #[serde(skip_serializing_if = "is_voxels")]
    pub water: VoxelWaterLook,
    /// What a tree felled in an endless volume becomes once it has fallen
    /// and come to rest: a body of its own, or stamped back into the volume
    /// (`felled = "stamped"`).
    #[serde(skip_serializing_if = "is_body")]
    pub felled: VoxelFelled,
    /// How many seconds its cut grass, cover and crops take to grow back:
    /// never unless written.
    #[serde(skip_serializing_if = "is_nothing")]
    pub regrow: f32,
    /// What holds up what is built in an endless volume, so that what is
    /// cut off from it comes loose: earth and the ground unless written.
    #[serde(skip_serializing_if = "is_earth_anchor")]
    pub anchor: voxear_render::VoxelAnchor,
    /// How an endless volume finds where what is built gives way under its
    /// own weight: solved unless written.
    #[serde(skip_serializing_if = "is_solved")]
    pub strain: voxear_render::VoxelStrain,
    /// What an endless volume's printed voxels are made of: stone unless
    /// written.
    #[serde(skip_serializing_if = "is_stone")]
    pub built: voxear_render::VoxelStuff,
    /// The most pieces of what was built falling at once in an endless
    /// volume: past that the smallest falls to dust.
    #[serde(skip_serializing_if = "is_most_pieces")]
    pub most_pieces: u32,
    /// How its felled trees fall and rest, and whether clippings fly from cut
    /// grass: `[obj.voxel_volume.felling]`, left out as it comes.
    #[serde(skip_serializing_if = "is_usual_felling")]
    pub felling: VoxelFelling,
}

fn is_earth_anchor(anchor: &voxear_render::VoxelAnchor) -> bool {
    *anchor == voxear_render::VoxelAnchor::Earth
}

fn is_solved(strain: &voxear_render::VoxelStrain) -> bool {
    *strain == voxear_render::VoxelStrain::Solved
}

fn is_stone(stuff: &voxear_render::VoxelStuff) -> bool {
    *stuff == voxear_render::VoxelStuff::Stone
}

fn is_most_pieces(most: &u32) -> bool {
    *most == voxear_render::MOST_PIECES
}

fn is_usual_felling(felling: &VoxelFelling) -> bool {
    *felling == VoxelFelling::default()
}

fn is_body(felled: &VoxelFelled) -> bool {
    *felled == VoxelFelled::Body
}

fn is_on(on: &bool) -> bool {
    *on
}

fn is_voxels(look: &VoxelWaterLook) -> bool {
    *look == VoxelWaterLook::Voxels
}

/// A change to an endless volume as a scene writes it, in metres from the
/// volume's corner: a ball, a box, a bank, an area or a cover, carved out or
/// filled with `fill`; or a Voxel Print, a reference to a `.voxprint`, its
/// origin at `at` and turned `rotation` degrees about X, Y and Z.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelEditData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print: Option<String>,
    #[serde(skip_serializing_if = "VoxelEditShape::is_ball")]
    pub shape: VoxelEditShape,
    /// A ball's middle, or the middle of a box's, a bank's or a print's foot.
    #[serde(skip_serializing_if = "is_origin3")]
    pub at: [f32; 3],
    #[serde(skip_serializing_if = "is_one_metre")]
    pub radius: f32,
    /// A box's or a bank's size along its own X, Y and Z.
    #[serde(skip_serializing_if = "is_unit")]
    pub size: [f32; 3],
    #[serde(skip_serializing_if = "is_unturned")]
    pub rotation: [f32; 3],
    /// An area's or a cover's outline, its points along X and Z; an area's
    /// foot and top, and how deep a cover is.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
    #[serde(skip_serializing_if = "is_zero")]
    pub low: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub high: f32,
    #[serde(skip_serializing_if = "is_zero")]
    pub depth: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<VoxelMaterial>,
    /// The colour its fill or print takes, as sRGB.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tint: Option<[u8; 3]>,
    /// How far over its top it clears, in metres.
    #[serde(skip_serializing_if = "is_zero")]
    pub clear: f32,
    /// Whether a print is fitted to the ground, on a footing of its fill.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fit: bool,
}

/// The shape of an edit as a scene names it; a print is an edit with a
/// `print`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoxelEditShape {
    #[default]
    Ball,
    Box,
    Ramp,
    Area,
    Cover,
}

impl VoxelEditShape {
    fn is_ball(&self) -> bool {
        *self == Self::Ball
    }
}

/// A radius of a metre, as unwritten: a print's, which its box takes the
/// place of, is so.
fn is_one_metre(radius: &f32) -> bool {
    *radius == 1.0
}

fn is_unturned(rotation: &[f32; 3]) -> bool {
    *rotation == [0.0; 3]
}

fn is_unit(size: &[f32; 3]) -> bool {
    *size == [1.0; 3]
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_origin3(at: &[f32; 3]) -> bool {
    *at == [0.0; 3]
}

impl Default for VoxelEditData {
    fn default() -> Self {
        Self {
            print: None,
            shape: VoxelEditShape::Ball,
            at: [0.0; 3],
            radius: 1.0,
            size: [1.0; 3],
            rotation: [0.0; 3],
            points: Vec::new(),
            low: 0.0,
            high: 0.0,
            depth: 0.0,
            fill: None,
            tint: None,
            clear: 0.0,
            fit: false,
        }
    }
}

impl VoxelEditData {
    /// A ball carved out, or filled with `fill`.
    pub fn ball(at: [f32; 3], radius: f32, fill: Option<VoxelMaterial>) -> Self {
        Self { at, radius, fill, ..Self::default() }
    }

    /// A box `size` metres, the middle of its foot at `at`, turned
    /// `rotation` degrees about X, Y and Z, carved out or filled.
    pub fn boxed(at: [f32; 3], size: [f32; 3], rotation: [f32; 3], fill: Option<VoxelMaterial>) -> Self {
        Self { shape: VoxelEditShape::Box, at, size, rotation, fill, ..Self::default() }
    }

    /// A bank, as a box is placed, rising along its own Z.
    pub fn ramp(at: [f32; 3], size: [f32; 3], rotation: [f32; 3], fill: Option<VoxelMaterial>) -> Self {
        Self { shape: VoxelEditShape::Ramp, at, size, rotation, fill, ..Self::default() }
    }

    /// The outline `points` from `low` up to `high` metres.
    pub fn area(points: Vec<[f32; 2]>, low: f32, high: f32, fill: Option<VoxelMaterial>) -> Self {
        Self { shape: VoxelEditShape::Area, points, low, high, fill, ..Self::default() }
    }

    /// The ground's top `depth` metres within the outline `points`.
    pub fn cover(points: Vec<[f32; 2]>, depth: f32, fill: Option<VoxelMaterial>) -> Self {
        Self { shape: VoxelEditShape::Cover, points, depth, fill, ..Self::default() }
    }

    /// A print, by its reference, its origin at `at`, turned `rotation`
    /// degrees about X, Y and Z.
    pub fn printed(print: String, at: [f32; 3], rotation: [f32; 3]) -> Self {
        Self { print: Some(print), at, rotation, ..Self::default() }
    }

    /// How it is turned, as a scene writes an Obj's rotation.
    pub fn turn(&self) -> Quat {
        let [x, y, z] = self.rotation;
        Quat::from_euler(EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians())
    }

    /// It moved by `by` metres: from the world into a volume whose corner is
    /// at `-by`, say.
    pub fn moved(&self, by: Vec3) -> Self {
        let mut moved = self.clone();
        moved.at = (Vec3::from(self.at) + by).to_array();
        for point in &mut moved.points {
            *point = [point[0] + by.x, point[1] + by.z];
        }
        if self.shape == VoxelEditShape::Area {
            moved.low += by.y;
            moved.high += by.y;
        }
        moved
    }

    /// The edit the renderer makes, its print read from `content`: none,
    /// with a warning, for a print that cannot be read.
    pub fn made(&self, content: &mut Content) -> Option<VoxelEdit> {
        let edit = match &self.print {
            Some(reference) => {
                let Some((_, print)) = content.voxel_print(reference) else {
                    tracing::warn!("a Voxel Volume's print {reference} is not a Voxel Print in the project");
                    return None;
                };
                let printed = VoxelEdit::printed(print, Vec3::from(self.at), self.turn());
                if self.fit { printed.fitted(self.fill) } else { printed }
            }
            None => self.shaped(),
        };
        Some(edit.tinted(self.tint))
    }

    /// The edit a shape makes, which needs nothing read: not a print's.
    pub fn shaped(&self) -> VoxelEdit {
        let at = Vec3::from(self.at);
        let edit = match self.shape {
            VoxelEditShape::Ball => VoxelEdit::ball(at, self.radius, self.fill),
            VoxelEditShape::Box => VoxelEdit::boxed(at, Vec3::from(self.size), self.turn(), self.fill),
            VoxelEditShape::Ramp => VoxelEdit::ramp(at, Vec3::from(self.size), self.turn(), self.fill),
            VoxelEditShape::Area => VoxelEdit::area(self.points.clone(), self.low, self.high, self.fill),
            VoxelEditShape::Cover => VoxelEdit::cover(self.points.clone(), self.depth, self.fill),
        };
        edit.clearing(self.clear).tinted(self.tint)
    }
}

/// A kind of tree scattered through an endless volume's ground, as a scene
/// writes it: a reference to a `.voxplant`, and how many to a hectare.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelTreesData {
    pub plant: String,
    pub per_hectare: f32,
    /// The World Areas it grows in, by their tags (Area Tag data assets),
    /// and those it never grows in: anywhere where it names none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub within: Vec<AssetGuid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub never_within: Vec<AssetGuid>,
}

impl Default for VoxelTreesData {
    fn default() -> Self {
        Self { plant: String::new(), per_hectare: 40.0, within: Vec::new(), never_within: Vec::new() }
    }
}

/// The rules of what keeps within areas of the tags `within` and out of
/// those of `never`, as the renderer reads them.
pub fn area_rules(within: &[AssetGuid], never: &[AssetGuid]) -> VoxelAreaRules {
    VoxelAreaRules {
        within: within.iter().map(|tag| tag.to_u128()).collect(),
        never: never.iter().map(|tag| tag.to_u128()).collect(),
    }
}

/// A kind of crop or ground cover grown on an endless volume, as a scene
/// writes it (see `voxear_render::VoxelCover`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelCoverData {
    pub kind: VoxelCoverKind,
    /// The ground it grows on: grass where none.
    pub on: Vec<VoxelMaterial>,
    pub density: f32,
    pub height: f32,
    pub rows: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soil: Option<VoxelMaterial>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub within: Vec<AssetGuid>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub never_within: Vec<AssetGuid>,
}

impl Default for VoxelCoverData {
    fn default() -> Self {
        Self::of(VoxelCoverKind::default())
    }
}

impl VoxelCoverData {
    /// A kind as it is first grown: its own height, rows and density, crops
    /// in ploughed earth, anywhere there is grass.
    pub fn of(kind: VoxelCoverKind) -> Self {
        let made = VoxelCover::of(kind);
        Self {
            kind,
            on: made.on,
            density: made.density,
            height: made.height,
            rows: made.rows,
            soil: made.soil,
            within: Vec::new(),
            never_within: Vec::new(),
        }
    }

    /// The cover the renderer grows.
    pub fn made(&self) -> VoxelCover {
        VoxelCover {
            kind: self.kind,
            on: self.on.clone(),
            density: self.density.clamp(0.0, 1.0),
            height: self.height.max(0.0),
            rows: self.rows.max(0.0),
            soil: self.soil,
            areas: area_rules(&self.within, &self.never_within),
        }
    }
}

fn is_origin(at: &[f32; 2]) -> bool {
    *at == [0.0, 0.0]
}

/// How far under a generator's ground at its corner a volume's floor is
/// unless it says, as a share of how tall the volume is: its ground stands
/// two fifths of the way up, with room for hills over it and dales under.
pub const AUTO_FLOOR: f32 = 0.4;

/// The generator's height at a volume's floor where it does not say: its
/// ground at the corner, less [`AUTO_FLOOR`] of how tall the volume is.
pub fn auto_floor(world: &voxear_terrain::generate::WorldGen, data: &VoxelVolumeData) -> f32 {
    let [x, z] = data.generator_at;
    world.height_at(x, z) - data.size[1] * AUTO_FLOOR
}

impl Default for VoxelVolumeData {
    fn default() -> Self {
        let made = VoxelVolume::default();
        Self {
            size: made.size.to_array(),
            voxels_per_metre: made.voxels_per_metre,
            roughness: made.roughness,
            ground_on: made.ground.is_some(),
            ground: made.ground.unwrap_or_default(),
            stamps: Vec::new(),
            endless: made.endless,
            reach: made.reach,
            distance: made.distance,
            generator: None,
            generator_floor: None,
            generator_at: [0.0, 0.0],
            trees: Vec::new(),
            cover: Vec::new(),
            mobs: Vec::new(),
            edits: Vec::new(),
            changes: None,
            regions: Vec::new(),
            sea: made.sea,
            water: made.water,
            felled: made.felled,
            regrow: made.regrow,
            anchor: made.anchor,
            strain: made.strain,
            built: made.built,
            most_pieces: made.most_pieces,
            felling: made.felling,
        }
    }
}

fn is_nothing(value: &f32) -> bool {
    *value == 0.0
}

/// A box model stamped into a volume, as a scene writes it: the model, its
/// feet's place in metres from the volume's corner, how far it is turned
/// about Y in degrees, and how many times its size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelStampData {
    pub model: String,
    pub at: [f32; 3],
    pub turn: f32,
    pub scale: f32,
}

impl Default for VoxelStampData {
    fn default() -> Self {
        Self { model: String::new(), at: [0.0; 3], turn: 0.0, scale: 1.0 }
    }
}

/// How an Obj's Voxel Volume was written, kept for saving and for the
/// Inspector to change.
#[derive(Clone, Debug, PartialEq)]
pub struct VoxelVolumeSource(pub VoxelVolumeData);

impl voxear_ecs::Component for VoxelVolumeSource {}

/// The volume `data` makes, its stamps' models read from `content`: a stamp
/// whose model cannot be read is left out, with a warning.
pub fn made(content: &mut Content, data: &VoxelVolumeData) -> VoxelVolume {
    let mut stamps = Vec::with_capacity(data.stamps.len());
    for stamp in &data.stamps {
        let model = content.parse(&stamp.model).ok().and_then(|reference| reference.guid());
        match model.map(|guid| content.box_model(guid)) {
            Some(Ok(model)) => {
                stamps.push(VoxelStamp { model, at: Vec3::from(stamp.at), turn: stamp.turn, scale: stamp.scale })
            }
            Some(Err(error)) => tracing::warn!("a Voxel Volume's stamp {} could not be read: {error:#}", stamp.model),
            None => tracing::warn!("a Voxel Volume's stamp {} is not a box model in the project", stamp.model),
        }
    }
    let generator = data.generator.as_deref().and_then(|reference| {
        let found = generator_of(content, reference);
        if found.is_none() {
            tracing::warn!(
                "a Voxel Volume's generator {reference} is not a world generator or a generated world of blocks in the project"
            );
        }
        found
    });
    let generator = generator.map(|(world, biomes)| VoxelGenerator {
        floor: data.generator_floor.unwrap_or_else(|| auto_floor(&world, data)),
        at: data.generator_at,
        world,
        biomes,
    });
    let trees = data.trees.iter().filter_map(|kind| trees_of(content, kind)).collect();
    VoxelVolume {
        size: Vec3::from(data.size),
        voxels_per_metre: data.voxels_per_metre,
        roughness: data.roughness,
        ground: data.ground_on.then_some(data.ground),
        stamps,
        endless: data.endless,
        reach: data.reach,
        distance: data.distance,
        generator,
        trees,
        cover: data.cover.iter().map(VoxelCoverData::made).collect(),
        areas: Arc::default(),
        edits: Arc::new(data.edits.iter().filter_map(|edit| edit.made(content)).collect()),
        changes: Arc::new(read_changes(content, data)),
        sea: data.sea,
        water: data.water,
        felled: data.felled,
        regrow: data.regrow.max(0.0),
        anchor: data.anchor,
        strain: data.strain,
        built: data.built,
        most_pieces: data.most_pieces,
        felling: data.felling.kept(),
    }
}

/// The region file of region `at` in the folder `folder`.
pub fn region_path(folder: &str, at: [i32; 2]) -> String {
    format!("{folder}/r.{}.{}.vxr", at[0], at[1])
}

/// The voxels dug and placed in a volume `data` writes, read from its
/// region files: none where it keeps none, with a warning for a region that
/// cannot be read.
fn read_changes(content: &mut Content, data: &VoxelVolumeData) -> VoxelChanges {
    let mut changes = VoxelChanges::default();
    let Some(folder) = &data.changes else { return changes };
    for &region in &data.regions {
        let path = region_path(folder, region);
        match content.read(&path).and_then(|bytes| changes.read_region(&bytes)) {
            Ok(()) => {}
            Err(error) => tracing::warn!("a Voxel Volume's changes in {path} could not be read: {error:#}"),
        }
    }
    changes
}

/// Writes the voxels dug and placed in each endless volume of `world` that
/// `ids` names (its Objs and their ids in the scene at `scene`, a project
/// path) into region files beside the scene, in `project`: a folder for
/// each volume, `<scene>.voxels/<id>`, a file for each region of 128 bricks
/// a side, only those that changed written and those gone taken away; and
/// notes where in `objs`, the scene's Objs as it writes them. Gives the
/// files written.
pub fn save_changes(
    world: &World,
    (project, scene): (&std::path::Path, &str),
    ids: &std::collections::HashMap<Entity, u32>,
    objs: &mut [crate::scene_file::ObjData],
) -> anyhow::Result<Vec<String>> {
    use anyhow::Context;
    let mut written = Vec::new();
    for (&entity, &id) in ids {
        let Some(volume) = world.get::<VoxelVolume>(entity).filter(|volume| volume.endless) else { continue };
        let Some(data) = objs.iter_mut().find(|obj| obj.id == id).and_then(|obj| obj.voxel_volume.as_mut()) else {
            continue;
        };
        let folder = format!("{scene}.voxels/{id}");
        let on_disk = project.join(&folder);
        let regions = volume.changes.regions();
        data.regions = regions.iter().map(|(at, _)| *at).collect();
        data.changes = (!regions.is_empty()).then(|| folder.clone());
        if regions.is_empty() && !on_disk.is_dir() {
            continue;
        }
        std::fs::create_dir_all(&on_disk).with_context(|| format!("making {folder}"))?;
        let mut kept = std::collections::HashSet::new();
        for (at, bytes) in &regions {
            let path = region_path(&folder, *at);
            kept.insert(project.join(&path));
            if std::fs::read(project.join(&path)).ok().as_deref() != Some(bytes.as_slice()) {
                std::fs::write(project.join(&path), bytes).with_context(|| format!("saving {path}"))?;
                written.push(path);
            }
        }
        // Regions no voxel is set in any more.
        for entry in std::fs::read_dir(&on_disk).with_context(|| format!("reading {folder}"))?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "vxr") && !kept.contains(&path) {
                std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            }
        }
        if regions.is_empty() {
            let _ = std::fs::remove_dir(&on_disk);
        }
    }
    Ok(written)
}

/// Digs out (`to` none) or places the voxel at each of `points` in the
/// world, one voxel each, in every endless Voxel Volume a point falls in,
/// or in `only`'s alone, over everything its edits make; kept with the
/// volume and saved in region files beside its scene. Gives how many
/// voxels were set.
pub fn set_voxels(world: &mut World, points: &[Vec3], to: Option<VoxelKind>, only: Option<Entity>) -> usize {
    let volumes: Vec<(Entity, Vec3, f32, f32)> = world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            let per = volume.voxels_per_metre.clamp(1, voxear_render::MOST_VOXELS_A_METRE) as f32;
            (entity, corner, per, volume.size.y)
        })
        .collect();
    let mut set = 0;
    for (entity, corner, per, tall) in volumes {
        let Some(volume) = world.get_mut::<VoxelVolume>(entity) else { continue };
        let changes = Arc::make_mut(&mut volume.changes);
        for point in points {
            let local = *point - corner;
            if local.y < 0.0 || local.y >= tall {
                continue;
            }
            let cell = (local * per).floor().as_ivec3();
            set += usize::from(changes.set(cell, to));
        }
    }
    set
}

/// Lets the voxels at `points` in the world go back to what their volume's
/// ground and edits make, in every endless volume, or `only`'s alone.
pub fn unset_voxels(world: &mut World, points: &[Vec3], only: Option<Entity>) {
    let volumes: Vec<(Entity, Vec3, f32)> = world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            (entity, corner, volume.voxels_per_metre.clamp(1, voxear_render::MOST_VOXELS_A_METRE) as f32)
        })
        .collect();
    for (entity, corner, per) in volumes {
        let Some(volume) = world.get_mut::<VoxelVolume>(entity) else { continue };
        let changes = Arc::make_mut(&mut volume.changes);
        for point in points {
            changes.unset(((*point - corner) * per).floor().as_ivec3());
        }
    }
}

/// The world generator `reference` names: a `.worldgen`, or a world of
/// blocks' ground generator, as its chunks are made by it (its biomes'
/// land, its painted maps, its rivers; see `blocks::world_ground`), with
/// its biomes, whose blocks the volume's ground is then made of.
fn generator_of(
    content: &mut Content,
    reference: &str,
) -> Option<(Arc<voxear_terrain::generate::WorldGen>, Option<Arc<VoxelBiomes>>)> {
    let guid = content.parse(reference).ok().and_then(|parsed| parsed.guid())?;
    if content.db().get(guid).map(|entry| entry.kind) != Some(voxear_assets::AssetKind::BlockWorld) {
        return content.world_gen(reference).map(|world| (world, None));
    }
    let found = crate::blocks::world_ground(content, guid)?;
    let biomes = found.biomes.as_ref().filter(|placed| !placed.biomes.is_empty()).map(|placed| {
        let own = [found.ground.top, found.ground.under, found.ground.bottom];
        let gpu = placed.gpu(own, found.ground.under_depth, found.block_size);
        let colour = |block: u16| found.colours.get(usize::from(block)).copied().unwrap_or([128; 3]);
        Arc::new(VoxelBiomes { colours: gpu.blocks.iter().map(|&block| colour(block)).collect(), words: gpu.words })
    });
    Some((found.generator, biomes))
}

/// Sets what answers `Voxels.Carve`, `Voxels.Fill`, `Voxels.Box` and the other shapes, `Voxels.Dig` and
/// `Voxels.Place`, `Voxels.Print`, `Voxels.Raycast`, `Voxels.WaterAt` and `Voxels.Sea` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_voxel_edits(|world, at, radius, fill| {
        let reach = Vec3::splat(radius.max(0.0));
        let at = Vec3::from(at);
        let carve = fill == -1;
        settling(world, carve, at - reach, at + reach, |world| from_script(world, at.to_array(), radius, fill))
    });
    voxear_scripting::set_voxel_blasts(|world, at, radius, pieces, throw| {
        use crate::voxel_collapse::{BlastOptions, blast};
        use crate::voxel_felling::CutPieces;
        let pieces = if pieces == 1 { CutPieces::Gone } else { CutPieces::Fall };
        let done = blast(world, Vec3::from(at), radius, BlastOptions { pieces, throw, ..BlastOptions::default() }, None);
        [done.reached, done.dug].map(|n| n.min(i32::MAX as usize) as i32)
    });
    voxear_scripting::set_voxel_water(
        |world, at| water_at(world, Vec3::from(at), None).map(|(e, water)| (e, water.height, water.current.to_array())),
        |world, on| {
            if on >= 0 {
                set_sea(world, on == 1, None);
            }
            sea_level(world)
        },
    );
    voxear_scripting::set_voxel_shapes(|world, request| {
        let carve = request.fill == -1;
        let (low, high) = request_reach(request);
        settling(world, carve, low, high, |world| shape_from_script(world, request))
    });
    voxear_scripting::set_voxel_sets(|world, points, to, tint| {
        let points: Vec<Vec3> = points.iter().map(|p| Vec3::from(*p)).collect();
        match to {
            -2 => {
                unset_voxels(world, &points, None);
                points.len() as i32
            }
            to => match material_from_script(to) {
                Some(material) => {
                    let kind = material.map(|material| VoxelKind { material, tint });
                    let (low, high) = points.iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), p| (l.min(*p), h.max(*p)));
                    settling(world, kind.is_none(), low, high, |world| set_voxels(world, &points, kind, None) as i32)
                }
                None => -1,
            },
        }
    });
    voxear_scripting::set_voxel_cutting(|world, felled, regrow| {
        if let Some(&felled) = usize::try_from(felled).ok().and_then(|felled| VoxelFelled::ALL.get(felled)) {
            set_felled(world, felled, None);
        }
        if regrow >= 0.0 {
            set_regrow(world, regrow, None);
        }
        let felled_at = |felled: VoxelFelled| VoxelFelled::ALL.iter().position(|f| *f == felled).unwrap_or(0) as i32;
        cutting(world).map(|(felled, regrow)| (felled_at(felled), regrow))
    });
    voxear_scripting::set_voxel_water_look(|world, look| {
        if let Some(&look) = usize::try_from(look).ok().and_then(|look| VoxelWaterLook::ALL.get(look)) {
            set_water_look(world, look, None);
        }
        water_look(world).map(|look| look as i32)
    });
    voxear_scripting::set_voxel_raycast(|world, origin, direction, reach| {
        raycast(world, Vec3::from(origin), Vec3::from(direction), reach, None)
            .map(|(entity, hit)| (entity, hit.point.to_array(), hit.normal.to_array(), hit.distance))
    });
    voxear_scripting::set_voxel_raycast_meeting(|world, origin, direction, reach, meets| {
        let meets = if meets == 1 { VoxelRayMeets::Cover } else { VoxelRayMeets::Solid };
        raycast_meeting(world, Vec3::from(origin), Vec3::from(direction), reach, meets, None)
            .map(|(entity, hit)| (entity, hit.point.to_array(), hit.normal.to_array(), hit.distance, hit.cover))
    });
    voxear_scripting::set_voxel_cuts(|world, request, toward, pieces, rest| {
        use crate::voxel_felling::{CutOptions, CutPieces, PieceRest};
        use voxear_scripting::VoxelCutRequest;
        let cut = match request {
            VoxelCutRequest::Points(points) => VoxelCut::Points(points.iter().map(|p| Vec3::from(*p)).collect()),
            VoxelCutRequest::Ball { at, radius } => VoxelCut::Ball { at: Vec3::from(*at), radius: *radius },
            VoxelCutRequest::Slice { at, across, radius } => {
                VoxelCut::Slice { at: Vec3::from(*at), across: Vec3::from(*across), radius: *radius }
            }
        };
        let options = CutOptions {
            toward: toward.map(Vec3::from),
            pieces: if pieces == 1 { CutPieces::Gone } else { CutPieces::Fall },
            rest: match rest {
                0 => Some(PieceRest::Stay),
                1 => Some(PieceRest::Stamp),
                2 => Some(PieceRest::Remove),
                _ => None,
            },
        };
        let done = crate::voxel_felling::cut(world, &cut, options, None);
        let counts = [done.reached, done.columns, done.voxels, done.pieces.len()].map(|n| n.min(i32::MAX as usize) as i32);
        (counts, done.pieces.iter().map(script_piece).collect(), done.mown.iter().map(|at| at.to_array()).collect())
    });
    voxear_scripting::set_voxel_felling(|world, given| {
        if let Some([push, rest_after, press_depth, press_to, wood_density, clippings]) = given {
            let felling = VoxelFelling { push, rest_after, press_depth, press_to, wood_density, clippings: clippings != 0.0 };
            set_felling(world, felling, None);
        }
        felling(world).map(|f| {
            [f.push, f.rest_after, f.press_depth, f.press_to, f.wood_density, f32::from(u8::from(f.clippings))]
        })
    });
    voxear_scripting::set_voxel_pieces(|world, entity, act| {
        let piece = crate::voxel_felling::piece(world, entity)?;
        let _ = match act {
            1 => crate::voxel_felling::stamp_piece(world, entity),
            2 => crate::voxel_felling::remove_piece(world, entity),
            _ => true,
        };
        Some(script_piece(&piece))
    });
}

/// Does `change` to the world, and where it dug or carved out (`digs`) what
/// was built in the box from `low` to `high` in the world, settles each
/// endless volume it changed: what it cut off comes loose.
fn settling<R>(world: &mut World, digs: bool, low: Vec3, high: Vec3, change: impl FnOnce(&mut World) -> R) -> R {
    if !digs {
        return change(world);
    }
    let before: Vec<(Entity, VoxelVolume)> = world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .filter(|(_, volume)| volume.endless && volume.anchor != voxear_render::VoxelAnchor::Everything)
        .map(|(entity, volume)| (entity, volume.clone()))
        .collect();
    let made = change(world);
    for (entity, was) in before {
        let changed = world.get::<VoxelVolume>(entity).is_some_and(|now| *now != was);
        if changed {
            crate::voxel_collapse::settle_after(world, entity, was, low, high, crate::voxel_felling::CutPieces::Fall);
        }
    }
    made
}

/// The box a script's shaped edit reaches, in the world: a little past it.
fn request_reach(request: &voxear_scripting::VoxelShapeRequest) -> (Vec3, Vec3) {
    let at = Vec3::from(request.at);
    let size = Vec3::from(request.size);
    let far = size.length().max(request.radius) + 0.5;
    let mut low = at - Vec3::splat(far);
    let mut high = at + Vec3::splat(far);
    for p in &request.points {
        low = low.min(Vec3::new(p[0], request.low.min(at.y) - far, p[1]));
        high = high.max(Vec3::new(p[0], request.high.max(at.y) + far, p[1]));
    }
    (low, high)
}

/// A felled piece as scripts read it.
fn script_piece(piece: &crate::voxel_felling::FelledPiece) -> voxear_scripting::VoxelCutPiece {
    voxear_scripting::VoxelCutPiece {
        obj: piece.obj,
        place: piece.place.to_array(),
        size: piece.size.to_array(),
        logs: piece.logs.min(i32::MAX as usize) as i32,
        leaves: piece.leaves.min(i32::MAX as usize) as i32,
        resting: piece.resting,
    }
}

/// Where a ray from `origin` along `direction`, in the world, first meets
/// the ground of an endless Voxel Volume within `reach` metres, or `only`'s
/// alone: the volume, and where and how far. Its ground as it is made, its
/// trees' logs and leaves, its edits over them; not its grass or flowers.
pub fn raycast(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    reach: f32,
    only: Option<Entity>,
) -> Option<(Entity, voxear_render::VoxelRayHit)> {
    raycast_meeting(world, origin, direction, reach, VoxelRayMeets::Solid, only)
}

/// As [`raycast`], meeting what `meets` says: with
/// [`VoxelRayMeets::Cover`] the grass, cover and crops too, as a blade
/// swung through them does.
pub fn raycast_meeting(
    world: &World,
    origin: Vec3,
    direction: Vec3,
    reach: f32,
    meets: VoxelRayMeets,
    only: Option<Entity>,
) -> Option<(Entity, voxear_render::VoxelRayHit)> {
    world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            voxear_render::voxel_raycast_meeting(volume, corner, origin, direction, reach, meets).map(|hit| (entity, hit))
        })
        .min_by(|a, b| a.1.distance.total_cmp(&b.1.distance))
}

/// The water of the rivers and lakes of the endless Voxel Volumes at `at`
/// in the world, or of `only`'s alone: the volume, how high the water's
/// surface stands and how its current runs (see
/// `voxear_render::voxel_water_at`); the highest where two have water
/// there. Not the sea, which stands at its generator's sea level as an
/// Ocean (`voxear_render::ocean::seas`).
pub fn water_at(world: &World, at: Vec3, only: Option<Entity>) -> Option<(Entity, voxear_render::VoxelWaterPoint)> {
    world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            voxear_render::voxel_water_at(volume, corner, at).map(|water| (entity, water))
        })
        .max_by(|a, b| a.1.height.total_cmp(&b.1.height))
}

/// Draws the rivers and lakes of every endless Voxel Volume as `look`
/// says, or `only`'s alone, in how its scene writes it too: how many it
/// reached.
pub fn set_water_look(world: &mut World, look: VoxelWaterLook, only: Option<Entity>) -> usize {
    let reached: Vec<Entity> = world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .filter(|(entity, volume)| volume.endless && only.is_none_or(|o| o == *entity))
        .map(|(entity, _)| entity)
        .collect();
    for &entity in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.water = look;
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.water = look;
        }
    }
    reached.len()
}

/// Sets what every endless Voxel Volume's felled trees become at rest, or
/// `only`'s alone, and its scene writes it: how many it reached.
pub fn set_felled(world: &mut World, felled: VoxelFelled, only: Option<Entity>) -> usize {
    let reached = endless_volumes(world, only);
    for &entity in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.felled = felled;
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.felled = felled;
        }
    }
    reached.len()
}

/// Sets how many seconds every endless Voxel Volume's cut grass takes to
/// grow back, 0 for never, or `only`'s alone, and its scene writes it: how
/// many it reached.
pub fn set_regrow(world: &mut World, seconds: f32, only: Option<Entity>) -> usize {
    let seconds = if seconds.is_finite() { seconds.clamp(0.0, 1.0e6) } else { 0.0 };
    let reached = endless_volumes(world, only);
    for &entity in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.regrow = seconds;
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.regrow = seconds;
        }
    }
    reached.len()
}

/// Sets how every endless Voxel Volume's felled trees fall and rest, and
/// whether clippings fly from its cut grass, or `only`'s alone, and its
/// scene writes it: how many it reached.
pub fn set_felling(world: &mut World, felling: VoxelFelling, only: Option<Entity>) -> usize {
    let felling = felling.kept();
    let reached = endless_volumes(world, only);
    for &entity in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.felling = felling;
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.felling = felling;
        }
    }
    reached.len()
}

/// How the first endless Voxel Volume's felled trees fall and rest, if
/// there is one.
pub fn felling(world: &World) -> Option<VoxelFelling> {
    world.query_ref::<&VoxelVolume>().into_iter().find(|volume| volume.endless).map(|volume| volume.felling)
}

/// What the first endless Voxel Volume's felled trees become at rest, and
/// how many seconds its cut grass takes to grow back, if there is one.
pub fn cutting(world: &World) -> Option<(VoxelFelled, f32)> {
    world.query_ref::<&VoxelVolume>().into_iter().find(|volume| volume.endless).map(|v| (v.felled, v.regrow))
}

/// The endless volumes of `world`, or `only` where it is one.
fn endless_volumes(world: &World, only: Option<Entity>) -> Vec<Entity> {
    world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .filter(|(entity, volume)| volume.endless && only.is_none_or(|o| o == *entity))
        .map(|(entity, _)| entity)
        .collect()
}

/// How the first endless Voxel Volume draws its rivers and lakes, if there
/// is one.
pub fn water_look(world: &World) -> Option<VoxelWaterLook> {
    world.query_ref::<&VoxelVolume>().into_iter().find(|volume| volume.endless).map(|volume| volume.water)
}

/// Turns the generator's sea of every endless Voxel Volume with one on or
/// off, or `only`'s alone, in how its scene writes it too: how many it
/// reached.
pub fn set_sea(world: &mut World, on: bool, only: Option<Entity>) -> usize {
    let reached: Vec<Entity> = world
        .query_ref::<(Entity, &VoxelVolume)>()
        .into_iter()
        .filter(|(entity, volume)| volume.endless && volume.generator.is_some() && only.is_none_or(|o| o == *entity))
        .map(|(entity, _)| entity)
        .collect();
    for &entity in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            volume.sea = on;
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.sea = on;
        }
    }
    reached.len()
}

/// How high the sea of the first endless Voxel Volume with its generator's
/// sea on stands in the world, if one has.
pub fn sea_level(world: &World) -> Option<f32> {
    world
        .query_ref::<(&VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .without::<voxear_scene::Destroying>()
        .into_iter()
        .find_map(|(volume, local, placed)| {
            volume.sea_height(placed.map_or(local.position, |placed| placed.0.w_axis.truncate()))
        })
}

/// A script's carve (`fill` -1) or fill with the ground numbered as
/// `VoxelMaterial` in Voxear.Core numbers it: how many volumes it reached,
/// or -1 for a ground there is none of.
fn from_script(world: &mut World, at: [f32; 3], radius: f32, fill: i32) -> i32 {
    match material_from_script(fill) {
        Some(fill) => edit(world, Vec3::from(at), radius, fill, None) as i32,
        None => -1,
    }
}

/// The ground a script numbers as `VoxelMaterial` in Voxear.Core numbers
/// it: `Some(None)` for -1, air; none for a number there is none of.
fn material_from_script(number: i32) -> Option<Option<VoxelMaterial>> {
    match number {
        -1 => Some(None),
        number => VoxelMaterial::ALL.get(usize::try_from(number).ok()?).map(|&material| Some(material)),
    }
}

/// A script's shaped edit (`Voxels.Box` and the rest): how many volumes it
/// reached, or -1 for a shape or a ground there is none of.
fn shape_from_script(world: &mut World, request: &voxear_scripting::VoxelShapeRequest) -> i32 {
    let Some(fill) = material_from_script(request.fill) else { return -1 };
    let shape = match request.shape {
        0 => VoxelEditShape::Ball,
        1 => VoxelEditShape::Box,
        2 => VoxelEditShape::Ramp,
        3 => VoxelEditShape::Area,
        4 => VoxelEditShape::Cover,
        _ => return -1,
    };
    let data = VoxelEditData {
        shape,
        at: request.at,
        radius: request.radius.max(0.0),
        size: request.size,
        rotation: request.rotation,
        points: request.points.clone(),
        low: request.low,
        high: request.high,
        depth: request.depth,
        fill,
        tint: request.tint,
        clear: request.clear.max(0.0),
        ..VoxelEditData::default()
    };
    shape_edit(world, &data, None) as i32
}

/// Carves a ball of `radius` metres round `at` in the world out of every
/// endless Voxel Volume it reaches, or fills it with `fill`; or out of
/// `only`'s alone. Each keeps it after what was changed in it before, as
/// its scene writes it, and makes again the columns it reaches. Gives how
/// many volumes it reached.
pub fn edit(world: &mut World, at: Vec3, radius: f32, fill: Option<VoxelMaterial>, only: Option<Entity>) -> usize {
    shape_edit(world, &VoxelEditData::ball(at.to_array(), radius.max(0.0), fill), only)
}

/// Makes the shaped edit `data` (a ball, a box, a bank, an area or a
/// cover, placed in the world) in every endless Voxel Volume it reaches, or
/// in `only`'s alone, as [`edit`] does a ball. A print is left to [`print`].
pub fn shape_edit(world: &mut World, data: &VoxelEditData, only: Option<Entity>) -> usize {
    if data.print.is_some() {
        return 0;
    }
    let made = data.shaped();
    let (low, high) = made.reach();
    let reached = reached_by(world, low, high, only);
    for &(entity, corner) in &reached {
        let local = data.moved(-corner);
        keep(world, entity, local.shaped(), local);
    }
    reached.len()
}

/// The endless volumes (all, or `only`) a box from `low` to `high` in the
/// world reaches, endless across the ground and as tall as each is, with
/// their corners.
fn reached_by(world: &World, low: Vec3, high: Vec3, only: Option<Entity>) -> Vec<(Entity, Vec3)> {
    world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            let inside = high.y >= corner.y && low.y <= corner.y + volume.size.y;
            inside.then_some((entity, corner))
        })
        .collect()
}

/// Keeps `edit` in `entity`'s volume after what was changed in it before,
/// and `data` in what its scene writes.
fn keep(world: &mut World, entity: Entity, edit: VoxelEdit, data: VoxelEditData) {
    if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
        Arc::make_mut(&mut volume.edits).push(edit);
    }
    if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
        source.0.edits.push(data);
    }
}

/// Prints `print` (read from `content` by `reference`) into every endless
/// Voxel Volume its box reaches, or into `only`'s alone: its model's origin
/// at `at` in the world, turned `rotation` degrees about X, Y and Z. Each
/// keeps it after what was changed in it before, as its scene writes it,
/// and makes again the columns it reaches. Gives how many volumes it
/// reached, or none for a reference that is not a Voxel Print.
pub fn print(
    content: &mut Content,
    world: &mut World,
    reference: &str,
    at: Vec3,
    rotation: [f32; 3],
    only: Option<Entity>,
) -> Option<usize> {
    print_as(content, world, &VoxelEditData::printed(reference.to_string(), at.to_array(), rotation), only)
}

/// Prints the print edit `data` (placed in the world, its print by
/// reference, fitted and tinted as it says) as [`print`] does.
pub fn print_as(
    content: &mut Content,
    world: &mut World,
    data: &VoxelEditData,
    only: Option<Entity>,
) -> Option<usize> {
    let (guid, voxel_print) = content.voxel_print(data.print.as_deref()?)?;
    let data = VoxelEditData { print: Some(guid.to_string()), ..data.clone() };
    let made = |data: &VoxelEditData| {
        let printed = VoxelEdit::printed(voxel_print.clone(), Vec3::from(data.at), data.turn());
        if data.fit { printed.fitted(data.fill) } else { printed }.tinted(data.tint)
    };
    let (low, high) = made(&data).reach();
    let reached = reached_by(world, low, high, only);
    for &(entity, corner) in &reached {
        let local = data.moved(-corner);
        keep(world, entity, made(&local), local);
    }
    Some(reached.len())
}

/// Prints what scripts asked to print this frame (`Voxels.Print`), in order.
pub(crate) fn print_from_scripts(content: &mut Content, world: &mut World) {
    for request in voxear_scripting::take_voxel_prints() {
        let data = VoxelEditData {
            fit: request.fit,
            fill: material_from_script(request.footing).flatten(),
            tint: request.tint,
            ..VoxelEditData::printed(request.print.clone(), request.at, request.rotation)
        };
        if print_as(content, world, &data, None).is_none() {
            tracing::error!(target: "script", "Voxels.Print: {} is not a Voxel Print in the project", request.print);
        }
    }
}

/// A kind of tree as the renderer draws it, its plant read from `content`
/// and its colours its wood's and its leaves' blocks': none, with a
/// warning, where its plant cannot be read.
fn trees_of(content: &mut Content, kind: &VoxelTreesData) -> Option<VoxelTrees> {
    let Some((_, plant)) = content.plant(&kind.plant) else {
        tracing::warn!("a Voxel Volume's trees {} are not a plant in the project", kind.plant);
        return None;
    };
    let conifer = plant.kind == voxear_terrain::blocks::plant::PlantKind::Conifer;
    let mut color = |block: Option<&String>, otherwise: [u8; 3]| {
        let found = block.and_then(|reference| content.block(reference));
        found.map_or(otherwise, |(_, block)| block.color.map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8))
    };
    let wood = color(plant.wood.as_ref(), [104, 78, 56]);
    let leaves = color(plant.leaves.as_ref(), if conifer { [38, 92, 52] } else { [70, 128, 46] });
    let areas = area_rules(&kind.within, &kind.never_within);
    Some(VoxelTrees { plant, per_hectare: kind.per_hectare.max(0.0), wood, leaves, areas })
}

/// Gives `entity` the Voxel Volume `data` makes, and keeps `data` as how it
/// is written: for a volume added or changed.
pub fn set(content: &mut Content, world: &mut World, entity: Entity, data: VoxelVolumeData) {
    let mut volume = made(content, &data);
    // The voxels dug and placed since it was saved stay as they are.
    if let Some(had) = world.get::<VoxelVolume>(entity) {
        volume.changes = had.changes.clone();
    }
    // The areas it had, until they are found again, so its ground is not made twice.
    if let Some(had) = world.get::<VoxelVolume>(entity).filter(|_| keeps_to_areas(&volume, Some(&data))) {
        volume.areas = had.areas.clone();
    }
    world.insert(entity, volume);
    world.insert(entity, VoxelVolumeSource(data));
}

/// Makes again every volume whose ground world generator `guid` makes, as
/// after the generator changed (read again from `content`).
pub fn regenerate(content: &mut Content, world: &mut World, guid: voxear_assets::AssetGuid) {
    let using: Vec<(Entity, VoxelVolumeData)> = world
        .query_ref::<(Entity, &VoxelVolumeSource)>()
        .into_iter()
        .filter(|(_, source)| {
            let reference = source.0.generator.as_deref();
            reference.and_then(|r| content.parse(r).ok()).and_then(|parsed| parsed.guid()) == Some(guid)
        })
        .map(|(entity, source)| (entity, source.0.clone()))
        .collect();
    for (entity, data) in using {
        set(content, world, entity, data);
    }
}

/// Makes again every volume printing the Voxel Print `guid`, as after it
/// was made again (read again from `content`).
pub fn reprint(content: &mut Content, world: &mut World, guid: voxear_assets::AssetGuid) {
    let using: Vec<(Entity, VoxelVolumeData)> = world
        .query_ref::<(Entity, &VoxelVolumeSource)>()
        .into_iter()
        .filter(|(_, source)| {
            source.0.edits.iter().filter_map(|edit| edit.print.as_deref()).any(|reference| {
                content.parse(reference).ok().and_then(|parsed| parsed.guid()) == Some(guid)
            })
        })
        .map(|(entity, source)| (entity, source.0.clone()))
        .collect();
    for (entity, data) in using {
        set(content, world, entity, data);
    }
}

/// Whether any of an endless volume's trees, cover or mobs (as `data`
/// writes them) keeps within or out of World Areas.
pub fn keeps_to_areas(volume: &VoxelVolume, data: Option<&VoxelVolumeData>) -> bool {
    let ruled = |rules: &VoxelAreaRules| !rules.within.is_empty() || !rules.never.is_empty();
    let mobs = data.is_some_and(|data| data.mobs.iter().any(|kind| !kind.within.is_empty() || !kind.never_within.is_empty()));
    volume.endless
        && (mobs
            || volume.trees.iter().any(|kind| ruled(&kind.areas))
            || volume.cover.iter().any(|kind| ruled(&kind.areas)))
}

/// Gives every endless volume whose trees or cover keep to World Areas the
/// outlined areas of its dimension with tags, laid out from its corner:
/// each frame, its ground made again only where they changed. Painted
/// areas belong to a world of blocks' Area map and are left out.
pub fn sync_areas(world: &mut World) {
    use voxear_scene::{Destroying, Transform, WorldTransform};
    let volumes: Vec<(Entity, Vec3)> = world
        .query_ref::<(Entity, &VoxelVolume, Option<&VoxelVolumeSource>, &Transform, Option<&WorldTransform>)>()
        .into_iter()
        .filter(|(_, volume, source, ..)| keeps_to_areas(volume, source.map(|source| &source.0)))
        .map(|(entity, _, _, local, placed)| (entity, placed.map_or(local.position, |placed| placed.0.w_axis.truncate())))
        .collect();
    if volumes.is_empty() {
        return;
    }
    let areas: Vec<(voxear_scene::DimensionId, VoxelArea)> = world
        .query_ref::<(Entity, &crate::world_areas::WorldArea, &Transform, Option<&WorldTransform>)>()
        .without::<Destroying>()
        .into_iter()
        .filter(|(obj, area, ..)| {
            area.shape == crate::world_areas::AreaShape::Outline
                && area.tags.iter().any(|entry| entry.tag.is_some())
                && voxear_scene::is_active(world, *obj)
        })
        .map(|(obj, area, local, placed)| {
            // Not placed in the world yet, before its first frame: where its own place puts it.
            let placed = placed.map_or_else(|| local.matrix(), |placed| placed.0);
            let found = VoxelArea {
                outline: area.world_outline(&placed),
                low: area.low,
                high: area.high,
                tags: area.tags.iter().filter_map(|entry| entry.tag).map(AssetGuid::to_u128).collect(),
            };
            (voxear_scene::dimension_of(world, obj), found)
        })
        .collect();
    for (entity, corner) in volumes {
        let here = voxear_scene::dimension_of(world, entity);
        let laid: Vec<VoxelArea> = areas
            .iter()
            .filter(|(dimension, _)| *dimension == here)
            .map(|(_, area)| VoxelArea {
                outline: area.outline.iter().map(|&[x, z]| [x - corner.x, z - corner.z]).collect(),
                low: area.low.map(|low| low - corner.y),
                high: area.high.map(|high| high - corner.y),
                tags: area.tags.clone(),
            })
            .collect();
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity)
            && *volume.areas != laid
        {
            volume.areas = Arc::new(laid);
        }
    }
}

/// Takes `entity`'s Voxel Volume away.
pub fn remove(world: &mut World, entity: Entity) {
    world.remove::<VoxelVolume>(entity);
    world.remove::<VoxelVolumeSource>(entity);
}

/// Every box model stamped into a volume `data` keeps, by its reference.
pub fn models(data: &VoxelVolumeData) -> impl Iterator<Item = &str> {
    data.stamps.iter().map(|stamp| stamp.model.as_str())
}

/// A model kept for stamping, shared with every volume that stamps it.
pub(crate) type SharedModel = Arc<voxear_render::BoxModel>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_written_in_a_scene_is_read_back_as_it_was_and_unwritten_fields_take_their_defaults() {
        let data = VoxelVolumeData {
            size: [32.0, 12.0, 24.0],
            voxels_per_metre: 8,
            ground: VoxelGround { flowers: 0.5, ..VoxelGround::default() },
            stamps: vec![VoxelStampData {
                model: "Plants/Oak.boxmodel".into(),
                at: [1.0, 2.0, 3.0],
                turn: 30.0,
                scale: 1.5,
            }],
            ..VoxelVolumeData::default()
        };
        let text = toml::to_string(&data).unwrap();
        assert_eq!(toml::from_str::<VoxelVolumeData>(&text).unwrap(), data);
        let short: VoxelVolumeData =
            toml::from_str("size = [4.0, 4.0, 4.0]\n[[stamps]]\nmodel = \"Rock.boxmodel\"").unwrap();
        assert!(short.ground_on, "a volume has its own ground unless it says not");
        assert_eq!(short.voxels_per_metre, 16);
        assert_eq!(short.stamps[0].scale, 1.0);
        assert_eq!(short.generator, None, "a volume makes its own ground unless it names a generator");
        let generated = VoxelVolumeData {
            endless: true,
            generator: Some("Worlds/Hills.worldgen".into()),
            generator_floor: Some(40.0),
            generator_at: [100.0, -20.0],
            ..VoxelVolumeData::default()
        };
        let text = toml::to_string(&generated).unwrap();
        assert!(text.contains("generator = \"Worlds/Hills.worldgen\""), "{text}");
        assert_eq!(toml::from_str::<VoxelVolumeData>(&text).unwrap(), generated);
        assert!(!toml::to_string(&data).unwrap().contains("generator"), "nothing written for none");
    }

    #[test]
    fn boxes_banks_areas_covers_and_fitted_prints_are_written_and_read_back_and_placed_from_the_world() {
        let edits = vec![
            VoxelEditData { clear: 2.0, ..VoxelEditData::boxed([1.0, 2.0, 3.0], [4.0, 0.5, 2.0], [0.0, 20.0, 0.0], Some(VoxelMaterial::Cobbles)) },
            VoxelEditData::ramp([1.0, 2.0, 3.0], [2.0, 1.0, 4.0], [0.0; 3], Some(VoxelMaterial::Dirt)),
            VoxelEditData { tint: Some([200, 60, 60]), ..VoxelEditData::area(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]], 1.0, 1.5, Some(VoxelMaterial::Planks)) },
            VoxelEditData::cover(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 3.0]], 0.25, Some(VoxelMaterial::Bricks)),
            VoxelEditData { fit: true, fill: Some(VoxelMaterial::Ashlar), ..VoxelEditData::printed("Prints/Hut.voxprint".into(), [3.0, 1.0, 2.0], [0.0; 3]) },
        ];
        let data = VoxelVolumeData { endless: true, edits, ..VoxelVolumeData::default() };
        let text = toml::to_string(&data).unwrap();
        for written in ["shape = \"box\"", "shape = \"ramp\"", "shape = \"area\"", "shape = \"cover\"", "fill = \"cobbles\"", "fit = true", "tint = [200, 60, 60]"] {
            assert!(text.contains(written), "{written} in {text}");
        }
        assert_eq!(toml::from_str::<VoxelVolumeData>(&text).unwrap(), data);
        // Made from the world into a volume whose corner is at (100, -10, 0): the box's foot, the area's outline and heights.
        let mut world = World::new();
        let volume = VoxelVolume { endless: true, size: Vec3::new(16.0, 40.0, 16.0), ..VoxelVolume::default() };
        let obj = world.spawn((volume, VoxelVolumeSource(VoxelVolumeData::default()), voxear_scene::Transform::from_position(Vec3::new(100.0, -10.0, 0.0))));
        assert_eq!(shape_edit(&mut world, &data.edits[0].moved(Vec3::new(100.0, 5.0, 0.0)), None), 1);
        assert_eq!(shape_edit(&mut world, &data.edits[2].moved(Vec3::new(100.0, 5.0, 0.0)), None), 1);
        assert_eq!(shape_edit(&mut world, &VoxelEditData::area(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], 50.0, 51.0, None), None), 0, "over its top");
        let source = &world.get::<VoxelVolumeSource>(obj).unwrap().0;
        assert_eq!(source.edits[0].at, [1.0, 17.0, 3.0]);
        assert_eq!((source.edits[1].points[1], source.edits[1].low, source.edits[1].high), ([4.0, 0.0], 16.0, 16.5));
        let made = world.get::<VoxelVolume>(obj).unwrap();
        assert_eq!(made.edits[1].tint, Some([200, 60, 60]));
        assert!(matches!(made.edits[0].shape, voxear_render::VoxelShape::Box { .. }) && made.edits[0].clear == 2.0);
    }

    #[test]
    fn a_body_dropped_on_an_endless_volume_rests_on_its_hills_and_falls_into_a_pit_carved_under_it() {
        use crate::physics::{Collider, PhysicsScene, Rigidbody, VoxelCollider};
        let mut world = World::new();
        voxear_scene::register_obj_components(&mut world);
        let ground = VoxelGround { height: 3.0, hills: 2.0, ..VoxelGround::default() };
        let volume = VoxelVolume { endless: true, size: Vec3::new(16.0, 16.0, 16.0), ground: Some(ground), ..VoxelVolume::default() };
        let corner = Vec3::new(-50.0, -10.0, 20.0);
        let obj = world.spawn((voxear_scene::Transform::from_position(corner),));
        world.insert(obj, volume.clone());
        let at = Vec3::new(-41.3, 0.0, 27.6);
        let falling = world.spawn((voxear_scene::Transform::from_position(at + Vec3::Y * 6.0),));
        world.insert(falling, Collider { size: [0.5, 0.5, 0.5], ..Collider::default() });
        world.insert(falling, Rigidbody::default());
        sync_colliders(&mut world);
        assert!(world.get::<VoxelCollider>(obj).is_some(), "an endless volume collides");
        let mut physics = PhysicsScene::new();
        let mut settle = |world: &mut World, physics: &mut PhysicsScene| {
            for _ in 0..240 {
                sync_colliders(world);
                physics.step(world, 1.0 / 60.0);
            }
            world.get::<voxear_scene::Transform>(falling).unwrap().position
        };
        let rests = settle(&mut world, &mut physics);
        let top = voxear_render::voxel_raycast(&volume, corner, rests, -Vec3::Y, 20.0).expect("the hills").point.y;
        assert!((rests.y - 0.25 - top).abs() < 0.08, "it rests on the hills' voxels: at {rests}, their top at {top}");
        // A pit carved under it: it falls in, a metre down.
        edit(&mut world, rests - Vec3::Y * 0.3, 1.2, None, None);
        let fallen = settle(&mut world, &mut physics);
        assert!(fallen.y < rests.y - 0.6, "it fell into the pit: from {rests} to {fallen}");
        // A volume no longer endless collides no more.
        world.get_mut::<VoxelVolume>(obj).unwrap().endless = false;
        sync_colliders(&mut world);
        assert!(world.get::<VoxelCollider>(obj).is_none());
    }

    #[test]
    fn edits_are_kept_in_order_written_and_read_back_and_reach_only_the_endless_volumes_a_ball_falls_in() {
        let data = VoxelVolumeData {
            endless: true,
            size: [16.0, 40.0, 16.0],
            edits: vec![
                VoxelEditData::ball([1.0, 20.0, 2.0], 1.5, None),
                VoxelEditData::ball([3.0, 21.0, 2.0], 1.0, Some(VoxelMaterial::Rock)),
                VoxelEditData::printed("Prints/Crate.voxprint".into(), [4.0, 20.0, 2.0], [0.0, 30.0, 0.0]),
            ],
            ..VoxelVolumeData::default()
        };
        let text = toml::to_string(&data).unwrap();
        assert!(text.contains("fill = \"rock\""), "{text}");
        assert!(text.contains("print = \"Prints/Crate.voxprint\"") && text.contains("rotation = [0.0, 30.0, 0.0]"), "{text}");
        assert_eq!(text.matches("radius").count(), 1, "a radius of a metre, as a print's, goes unwritten: {text}");
        assert_eq!(toml::from_str::<VoxelVolumeData>(&text).unwrap(), data);
        let mut world = World::new();
        let at = voxear_scene::Transform::from_position(Vec3::new(100.0, -10.0, 0.0));
        let volume = VoxelVolume { endless: true, size: Vec3::new(16.0, 40.0, 16.0), ..VoxelVolume::default() };
        let endless = world.spawn((volume.clone(), VoxelVolumeSource(data.clone()), at));
        let plain = world.spawn((VoxelVolume::default(), voxear_scene::Transform::default()));
        // In the endless volume, however far across; over its top, in none.
        assert_eq!(edit(&mut world, Vec3::new(5000.0, 5.0, -300.0), 2.0, Some(VoxelMaterial::Sand), None), 1);
        assert_eq!(edit(&mut world, Vec3::new(0.0, 40.0, 0.0), 2.0, None, None), 0, "over the volume's top");
        let made = world.get::<VoxelVolume>(endless).unwrap();
        assert_eq!(made.edits.len(), 1);
        assert_eq!(made.edits[0].at(), Vec3::new(4900.0, 15.0, -300.0), "from the volume's corner");
        let source = &world.get::<VoxelVolumeSource>(endless).unwrap().0;
        assert_eq!(source.edits.len(), 4, "after the three it was written with");
        assert_eq!(source.edits[3].fill, Some(VoxelMaterial::Sand));
        assert!(world.get::<VoxelVolume>(plain).unwrap().edits.is_empty(), "a volume of a set size is not edited");
    }
}

/// An endless Voxel Volume's solid voxels as physics asks for them: its
/// ground, edits and trees' logs, as the CPU finds them.
struct Solids(voxear_render::VoxelSolids);

impl crate::physics::VoxelSolids for Solids {
    fn voxel(&self) -> f32 {
        self.0.voxel()
    }

    fn fill(&self, low: [i32; 3], size: [i32; 3], solid: &mut [bool]) {
        self.0.fill(low, size, solid);
    }

    fn changed_from(&self, older: &dyn crate::physics::VoxelSolids) -> Option<Vec<([i32; 3], [i32; 3])>> {
        self.0.changed_from(&older.as_any().downcast_ref::<Solids>()?.0)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Gives each endless Voxel Volume a Voxel Collider of its solid voxels,
/// given again when the volume changes, so characters and bodies stand on
/// its ground and bump into its trees' trunks; and takes it from those no
/// longer endless or gone.
pub fn sync_colliders(world: &mut World) {
    use crate::physics::VoxelCollider;
    let mut changed: Vec<(Entity, Option<VoxelCollider>)> = Vec::new();
    for (entity, volume, collider) in world.query_ref::<(Entity, &VoxelVolume, Option<&VoxelCollider>)>().into_iter() {
        let kept = collider.and_then(|collider| collider.0.as_any().downcast_ref::<Solids>());
        if kept.is_some_and(|kept| kept.0.volume() == volume) || (!volume.endless && collider.is_none()) {
            continue;
        }
        let solids = voxear_render::VoxelSolids::of(volume).map(|solids| match kept {
            Some(kept) => solids.keeping(&kept.0),
            None => solids,
        });
        changed.push((entity, solids.map(|solids| VoxelCollider(Arc::new(Solids(solids))))));
    }
    let left: Vec<Entity> = world
        .query_ref::<(Entity, &VoxelCollider)>()
        .into_iter()
        .filter(|(entity, collider)| collider.0.as_any().is::<Solids>() && world.get::<VoxelVolume>(*entity).is_none())
        .map(|(entity, _)| entity)
        .collect();
    changed.extend(left.into_iter().map(|entity| (entity, None)));
    for (entity, collider) in changed {
        match collider {
            Some(collider) => {
                world.insert(entity, collider);
            }
            None => {
                world.remove::<VoxelCollider>(entity);
            }
        }
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// What colliding with an endless volume costs a character walking at
    /// 6 m/s over its own hills, over a generator's, and through a wood:
    /// run by hand, `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_collision_walking() {
        use crate::physics::{CharacterController, PhysicsScene};
        use voxear_terrain::blocks::plant::{PlantDef, PlantKind};
        let hills = voxear_terrain::generate::starters().into_iter().find(|s| s.name == "Rolling Hills").unwrap().world(7, 1.0);
        let hills = Arc::new(hills);
        let floor = hills.height_at(8.0, 8.0) - 24.0;
        let generator = VoxelGenerator { world: hills.clone(), floor, at: [0.0, 0.0], biomes: None };
        let oak = PlantDef { height: [8.0, 12.0], crown_width: [7.0, 10.0], ..PlantDef::default() };
        let pine = PlantDef { kind: PlantKind::Conifer, height: [12.0, 17.0], trunk: 1.0, crown_width: [5.0, 7.0], crown_height: [9.0, 13.0], ..PlantDef::default() };
        let trees = vec![
            VoxelTrees { plant: Arc::new(oak), per_hectare: 60.0, wood: [104, 78, 56], leaves: [70, 128, 46], areas: Default::default() },
            VoxelTrees { plant: Arc::new(pine), per_hectare: 40.0, wood: [92, 64, 44], leaves: [38, 92, 52], areas: Default::default() },
        ];
        for (name, generator, trees) in [
            ("own hills", None, Vec::new()),
            ("Rolling Hills", Some(generator.clone()), Vec::new()),
            ("Rolling Hills, 100 trees a hectare", Some(generator), trees),
        ] {
            let mut world = World::new();
            voxear_scene::register_obj_components(&mut world);
            let volume = VoxelVolume { endless: true, size: Vec3::new(16.0, 48.0, 16.0), generator, trees, ..VoxelVolume::default() };
            let obj = world.spawn((voxear_scene::Transform::default(),));
            world.insert(obj, volume.clone());
            let top = voxear_render::voxel_raycast(&volume, Vec3::ZERO, Vec3::new(8.0, 47.0, 8.0), -Vec3::Y, 60.0).unwrap().point;
            let walker = world.spawn((voxear_scene::Transform::from_position(top + Vec3::Y * 0.05),));
            world.insert(walker, CharacterController::default());
            let mut physics = PhysicsScene::new();
            let started = std::time::Instant::now();
            sync_colliders(&mut world);
            let collider = started.elapsed();
            let started = std::time::Instant::now();
            physics.step(&mut world, 1.0 / 60.0);
            let first = started.elapsed();
            let mut steps = Vec::new();
            let mut grounded = 0;
            for _ in 0..600 {
                let started = std::time::Instant::now();
                sync_colliders(&mut world);
                physics.step(&mut world, 1.0 / 60.0);
                let s = physics.move_character(&mut world, walker, Vec3::new(6.0, -6.0, 1.5), 1.0 / 60.0);
                steps.push(started.elapsed().as_secs_f64() * 1000.0);
                grounded += usize::from(s.is_some_and(|s| s.ground == voxear_physics::Ground::Ground));
            }
            let over = steps.iter().filter(|&&t| t > 2.0).count();
            steps.sort_by(f64::total_cmp);
            let at = world.get::<voxear_scene::Transform>(walker).unwrap().position;
            eprintln!("{name}: collider {collider:?}, first step {first:?}; steps median {:.2} ms, 99th {:.2}, most {:.2}, {over} over 2 ms; on the ground {grounded} of 600; walked to {at}", steps[300], steps[594], steps[599]);
        }
    }
}
