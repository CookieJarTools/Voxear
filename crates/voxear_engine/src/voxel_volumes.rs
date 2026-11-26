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
//! [[obj.voxel_volume.stamps]]
//! model = "Plants/Oak.boxmodel"
//! at = [12.0, 2.0, 9.0]
//! turn = 30.0
//! ```
//!
//! An endless one keeps what was changed in it, in order: balls carved out
//! of it, and filled with a ground (`dirt`, `grass`, `rock`, `sand`, `snow`
//! or `gravel`), each its middle in metres from the volume's corner:
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
//! A Voxel Print printed into it is an edit too, kept in the same order:
//! the `.voxprint`, where its model's origin is, and how it is turned, in
//! degrees about X, Y and Z, as an Obj's rotation is written:
//!
//! ```toml
//! [[obj.voxel_volume.edits]]
//! print = "Prints/Sandbag Wall.voxprint"
//! at = [10.0, 24.0, 6.0]
//! rotation = [0.0, 30.0, 0.0]
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::{EulerRot, Quat, Vec3};
use voxear_ecs::{Entity, World};
use voxear_render::{
    VoxelBiomes, VoxelEdit, VoxelGenerator, VoxelGround, VoxelMaterial, VoxelStamp, VoxelTrees, VoxelVolume,
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
    /// What was changed in an endless volume, in the order it was changed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<VoxelEditData>,
    /// Whether an endless volume with a generator has its generator's sea,
    /// where the level has no Ocean of its own: on unless written off.
    #[serde(skip_serializing_if = "is_on")]
    pub sea: bool,
}

fn is_on(on: &bool) -> bool {
    *on
}

/// A change to an endless volume as a scene writes it: a ball, its middle
/// in metres from the volume's corner, carved out or filled with a ground;
/// or a Voxel Print, a reference to a `.voxprint`, its origin at `at` and
/// turned `rotation` degrees about X, Y and Z.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelEditData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print: Option<String>,
    pub at: [f32; 3],
    #[serde(skip_serializing_if = "is_one_metre")]
    pub radius: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<VoxelMaterial>,
    #[serde(skip_serializing_if = "is_unturned")]
    pub rotation: [f32; 3],
}

/// A radius of a metre, as unwritten: a print's, which its box takes the
/// place of, is so.
fn is_one_metre(radius: &f32) -> bool {
    *radius == 1.0
}

fn is_unturned(rotation: &[f32; 3]) -> bool {
    *rotation == [0.0; 3]
}

impl Default for VoxelEditData {
    fn default() -> Self {
        Self { print: None, at: [0.0; 3], radius: 1.0, fill: None, rotation: [0.0; 3] }
    }
}

impl VoxelEditData {
    /// A ball carved out, or filled with `fill`.
    pub fn ball(at: [f32; 3], radius: f32, fill: Option<VoxelMaterial>) -> Self {
        Self { at, radius, fill, ..Self::default() }
    }

    /// A print, by its reference, its origin at `at`, turned `rotation`
    /// degrees about X, Y and Z.
    pub fn printed(print: String, at: [f32; 3], rotation: [f32; 3]) -> Self {
        Self { print: Some(print), at, radius: 1.0, fill: None, rotation }
    }

    /// How it is turned, as a scene writes an Obj's rotation.
    pub fn turn(&self) -> Quat {
        let [x, y, z] = self.rotation;
        Quat::from_euler(EulerRot::YXZ, y.to_radians(), x.to_radians(), z.to_radians())
    }

    /// The edit the renderer makes, its print read from `content`: none,
    /// with a warning, for a print that cannot be read.
    pub fn made(&self, content: &mut Content) -> Option<VoxelEdit> {
        let Some(reference) = &self.print else {
            return Some(VoxelEdit::ball(Vec3::from(self.at), self.radius, self.fill));
        };
        let Some((_, print)) = content.voxel_print(reference) else {
            tracing::warn!("a Voxel Volume's print {reference} is not a Voxel Print in the project");
            return None;
        };
        Some(VoxelEdit::printed(print, Vec3::from(self.at), self.turn()))
    }
}

/// A kind of tree scattered through an endless volume's ground, as a scene
/// writes it: a reference to a `.voxplant`, and how many to a hectare.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelTreesData {
    pub plant: String,
    pub per_hectare: f32,
}

impl Default for VoxelTreesData {
    fn default() -> Self {
        Self { plant: String::new(), per_hectare: 40.0 }
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
            generator: None,
            generator_floor: None,
            generator_at: [0.0, 0.0],
            trees: Vec::new(),
            edits: Vec::new(),
            sea: made.sea,
        }
    }
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
        generator,
        trees,
        edits: Arc::new(data.edits.iter().filter_map(|edit| edit.made(content)).collect()),
        sea: data.sea,
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

/// Sets what answers `Voxels.Carve`, `Voxels.Fill`, `Voxels.Print`, `Voxels.Raycast`, `Voxels.WaterAt` and
/// `Voxels.Sea` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_voxel_edits(from_script);
    voxear_scripting::set_voxel_water(
        |world, at| water_at(world, Vec3::from(at), None).map(|(e, water)| (e, water.height, water.current.to_array())),
        |world, on| {
            if on >= 0 {
                set_sea(world, on == 1, None);
            }
            sea_level(world)
        },
    );
    voxear_scripting::set_voxel_raycast(|world, origin, direction, reach| {
        raycast(world, Vec3::from(origin), Vec3::from(direction), reach, None)
            .map(|(entity, hit)| (entity, hit.point.to_array(), hit.normal.to_array(), hit.distance))
    });
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
    world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            voxear_render::voxel_raycast(volume, corner, origin, direction, reach).map(|hit| (entity, hit))
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
    let fill = match fill {
        -1 => None,
        number => match VoxelMaterial::ALL.get(number as usize) {
            Some(&material) => Some(material),
            None => return -1,
        },
    };
    edit(world, Vec3::from(at), radius, fill, None) as i32
}

/// Carves a ball of `radius` metres round `at` in the world out of every
/// endless Voxel Volume it reaches, or fills it with `fill`; or out of
/// `only`'s alone. Each keeps it after what was changed in it before, as
/// its scene writes it, and makes again the columns it reaches. Gives how
/// many volumes it reached.
pub fn edit(world: &mut World, at: Vec3, radius: f32, fill: Option<VoxelMaterial>, only: Option<Entity>) -> usize {
    let radius = radius.max(0.0);
    let reached: Vec<(Entity, VoxelEditData)> = world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            // Endless across the ground, as tall as the volume.
            let inside = at.y + radius >= corner.y && at.y - radius <= corner.y + volume.size.y;
            inside.then_some((entity, VoxelEditData::ball((at - corner).to_array(), radius, fill)))
        })
        .collect();
    for (entity, data) in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(*entity) {
            Arc::make_mut(&mut volume.edits).push(VoxelEdit::ball(Vec3::from(data.at), data.radius, data.fill));
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(*entity) {
            source.0.edits.push(data.clone());
        }
    }
    reached.len()
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
    let (guid, voxel_print) = content.voxel_print(reference)?;
    let data = VoxelEditData::printed(guid.to_string(), at.to_array(), rotation);
    let turn = data.turn();
    let (low, high) = voxel_print.placed_bounds(at, turn);
    let reached: Vec<(Entity, Vec3)> = world
        .query_ref::<(Entity, &VoxelVolume, &voxear_scene::Transform, Option<&voxear_scene::WorldTransform>)>()
        .into_iter()
        .filter(|(entity, volume, ..)| volume.endless && only.is_none_or(|only| only == *entity))
        .filter_map(|(entity, volume, local, placed)| {
            let corner = placed.map_or(local.position, |placed| placed.0.w_axis.truncate());
            let inside = high.y >= corner.y && low.y <= corner.y + volume.size.y;
            inside.then_some((entity, corner))
        })
        .collect();
    for &(entity, corner) in &reached {
        let local = VoxelEditData { at: (at - corner).to_array(), ..data.clone() };
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            let edit = VoxelEdit::printed(voxel_print.clone(), Vec3::from(local.at), turn);
            Arc::make_mut(&mut volume.edits).push(edit);
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.edits.push(local);
        }
    }
    Some(reached.len())
}

/// Prints what scripts asked to print this frame (`Voxels.Print`), in order.
pub(crate) fn print_from_scripts(content: &mut Content, world: &mut World) {
    for request in voxear_scripting::take_voxel_prints() {
        let at = Vec3::from(request.at);
        if print(content, world, &request.print, at, request.rotation, None).is_none() {
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
    Some(VoxelTrees { plant, per_hectare: kind.per_hectare.max(0.0), wood, leaves })
}

/// Gives `entity` the Voxel Volume `data` makes, and keeps `data` as how it
/// is written: for a volume added or changed.
pub fn set(content: &mut Content, world: &mut World, entity: Entity, data: VoxelVolumeData) {
    let volume = made(content, &data);
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
        assert_eq!(made.edits[0].at, Vec3::new(4900.0, 15.0, -300.0), "from the volume's corner");
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
        let oak = PlantDef { height: [8.0, 12.0], crown_width: [7.0, 10.0], ..PlantDef::default() };
        let pine = PlantDef { kind: PlantKind::Conifer, height: [12.0, 17.0], trunk: 1.0, crown_width: [5.0, 7.0], crown_height: [9.0, 13.0], ..PlantDef::default() };
        let trees = vec![
            VoxelTrees { plant: Arc::new(oak), per_hectare: 60.0, wood: [104, 78, 56], leaves: [70, 128, 46] },
            VoxelTrees { plant: Arc::new(pine), per_hectare: 40.0, wood: [92, 64, 44], leaves: [38, 92, 52] },
        ];
        for (name, generator, trees) in [
            ("own hills", None, Vec::new()),
            ("Rolling Hills", Some(VoxelGenerator { world: hills.clone(), floor, at: [0.0, 0.0] }), Vec::new()),
            ("Rolling Hills, 100 trees a hectare", Some(VoxelGenerator { world: hills.clone(), floor, at: [0.0, 0.0] }), trees),
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
