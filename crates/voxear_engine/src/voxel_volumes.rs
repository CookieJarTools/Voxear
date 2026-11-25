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
//! in the generator's world. Its trees are kinds of `.voxplant`, each as
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

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{VoxelEdit, VoxelGenerator, VoxelGround, VoxelMaterial, VoxelStamp, VoxelTrees, VoxelVolume};

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
    /// reference to a `.worldgen`: none for its own ground.
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
}

/// A change to an endless volume as a scene writes it: a ball, its middle
/// in metres from the volume's corner, carved out or filled with a ground.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VoxelEditData {
    pub at: [f32; 3],
    pub radius: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<VoxelMaterial>,
}

impl Default for VoxelEditData {
    fn default() -> Self {
        Self { at: [0.0; 3], radius: 1.0, fill: None }
    }
}

impl From<VoxelEditData> for VoxelEdit {
    fn from(data: VoxelEditData) -> Self {
        Self { at: Vec3::from(data.at), radius: data.radius.max(0.0), fill: data.fill }
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
        let found = content.world_gen(reference);
        if found.is_none() {
            tracing::warn!("a Voxel Volume's generator {reference} is not a world generator in the project");
        }
        found
    });
    let generator = generator.map(|world| VoxelGenerator {
        floor: data.generator_floor.unwrap_or_else(|| auto_floor(&world, data)),
        at: data.generator_at,
        world,
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
        edits: Arc::new(data.edits.iter().map(|&edit| edit.into()).collect()),
    }
}

/// Sets what answers `Voxels.Carve`, `Voxels.Fill` and `Voxels.Raycast` in scripts.
pub(crate) fn register() {
    voxear_scripting::set_voxel_edits(from_script);
    voxear_scripting::set_voxel_raycast(|world, origin, direction, reach| {
        raycast(world, Vec3::from(origin), Vec3::from(direction), reach, None)
            .map(|(entity, hit)| (entity, hit.point.to_array(), hit.normal.to_array(), hit.distance))
    });
}

/// Where a ray from `origin` along `direction`, in the world, first meets
/// the ground of an endless Voxel Volume within `reach` metres, or `only`'s
/// alone: the volume, and where and how far. Its ground as it is made, its
/// edits over it; not its grass, flowers or trees.
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
            inside.then_some((entity, VoxelEditData { at: (at - corner).to_array(), radius, fill }))
        })
        .collect();
    for &(entity, data) in &reached {
        if let Some(volume) = world.get_mut::<VoxelVolume>(entity) {
            Arc::make_mut(&mut volume.edits).push(data.into());
        }
        if let Some(source) = world.get_mut::<VoxelVolumeSource>(entity) {
            source.0.edits.push(data);
        }
    }
    reached.len()
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
    fn edits_are_kept_in_order_written_and_read_back_and_reach_only_the_endless_volumes_a_ball_falls_in() {
        let data = VoxelVolumeData {
            endless: true,
            size: [16.0, 40.0, 16.0],
            edits: vec![
                VoxelEditData { at: [1.0, 20.0, 2.0], radius: 1.5, fill: None },
                VoxelEditData { at: [3.0, 21.0, 2.0], radius: 1.0, fill: Some(VoxelMaterial::Rock) },
            ],
            ..VoxelVolumeData::default()
        };
        let text = toml::to_string(&data).unwrap();
        assert!(text.contains("fill = \"rock\""), "{text}");
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
        assert_eq!(source.edits.len(), 3, "after the two it was written with");
        assert_eq!(source.edits[2].fill, Some(VoxelMaterial::Sand));
        assert!(world.get::<VoxelVolume>(plain).unwrap().edits.is_empty(), "a volume of a set size is not edited");
    }
}
