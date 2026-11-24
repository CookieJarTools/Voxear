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
//! [[obj.voxel_volume.stamps]]
//! model = "Plants/Oak.boxmodel"
//! at = [12.0, 2.0, 9.0]
//! turn = 30.0
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_render::{VoxelGround, VoxelStamp, VoxelVolume};

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
    VoxelVolume {
        size: Vec3::from(data.size),
        voxels_per_metre: data.voxels_per_metre,
        roughness: data.roughness,
        ground: data.ground_on.then_some(data.ground),
        stamps,
    }
}

/// Gives `entity` the Voxel Volume `data` makes, and keeps `data` as how it
/// is written: for a volume added or changed.
pub fn set(content: &mut Content, world: &mut World, entity: Entity, data: VoxelVolumeData) {
    let volume = made(content, &data);
    world.insert(entity, volume);
    world.insert(entity, VoxelVolumeSource(data));
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
    }
}
