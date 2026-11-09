//! Local post volumes, as Unity's Volumes and Unreal's Post Process Volumes:
//! a box or sphere in the level, or the whole level, whose post-processing
//! settings blend in as the camera comes into it. A cave can darken and
//! grade its own look, or a hall of mirrors bloom more, while the rest of
//! the level keeps the scene's settings.

use anyhow::{Context, Result};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

use crate::post::PostProcess;

/// Where a post volume's settings apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VolumeShape {
    /// Everywhere, as a level-wide layer over the scene's settings.
    Global,
    /// Inside the Obj's unit cube, as its Transform sizes it.
    #[default]
    Box,
    /// Inside the sphere round the Obj's unit cube, as wide as its largest scale.
    Sphere,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PostVolume {
    pub shape: VolumeShape,
    /// How far outside its shape, in metres, its settings start to blend in;
    /// 0 switches them on at its edge.
    pub blend_distance: f32,
    /// How much of its settings show at most, from 0 to 1.
    pub weight: f32,
    /// Volumes apply in order of priority, the highest last, so it wins.
    pub priority: i32,
    /// The settings it changes, as a scene's `[post]` writes them, without
    /// a preset: `{ bloom = { intensity = 0.2 } }`.
    pub overrides: toml::Table,
}

impl Component for PostVolume {}

impl Default for PostVolume {
    fn default() -> Self {
        Self { shape: VolumeShape::Box, blend_distance: 1.0, weight: 1.0, priority: 0, overrides: toml::Table::new() }
    }
}

impl PostVolume {
    /// Checks the overrides name settings that exist, with values that fit.
    pub fn check(&self) -> Result<()> {
        PostProcess::voxear().blended(&self.overrides, 1.0).map(drop).context("reading a post volume's settings")
    }

    /// How much of the volume shows at a point: its weight inside it, less
    /// the farther outside, to nothing at its blend distance.
    pub fn influence(&self, transform: &Mat4, point: Vec3) -> f32 {
        let outside = match self.shape {
            VolumeShape::Global => 0.0,
            VolumeShape::Box => {
                let local = transform.inverse().transform_point3(point);
                let nearest = transform.transform_point3(local.clamp(Vec3::splat(-0.5), Vec3::splat(0.5)));
                nearest.distance(point)
            }
            VolumeShape::Sphere => {
                let (scale, _, centre) = transform.to_scale_rotation_translation();
                (centre.distance(point) - 0.5 * scale.abs().max_element()).max(0.0)
            }
        };
        let reach = match self.blend_distance > 0.0 {
            true => (1.0 - outside / self.blend_distance).clamp(0.0, 1.0),
            false => f32::from(outside <= 0.0),
        };
        self.weight.clamp(0.0, 1.0) * reach
    }
}

/// The settings as the camera at `point` sees them: `post` with every post
/// volume blended over it, lowest priority first.
pub fn blend_volumes(world: &World, post: PostProcess, point: Vec3) -> PostProcess {
    let volumes =
        world.query_ref::<(Entity, &PostVolume, &Transform, Option<&WorldTransform>)>().without::<Destroying>();
    let switched = voxear_scene::Switched::of(world);
    let mut found: Vec<(i32, &PostVolume, f32)> = volumes
        .into_iter()
        .filter(|(entity, ..)| switched.is_on(world, *entity, voxear_scene::Switch::PostVolume))
        .filter_map(|(_, volume, local, world)| {
            let transform = world.map_or_else(|| local.matrix(), |world| world.0);
            let amount = volume.influence(&transform, point);
            (amount > 0.0 && !volume.overrides.is_empty()).then_some((volume.priority, volume, amount))
        })
        .collect();
    found.sort_by_key(|(priority, ..)| *priority);
    found.into_iter().fold(post, |post, (_, volume, amount)| match post.blended(&volume.overrides, amount) {
        Ok(blended) => blended,
        Err(error) => {
            tracing::warn!("a post volume's settings could not be read: {error:#}");
            post
        }
    })
}

#[cfg(test)]
mod tests {
    use voxear_core::math::Quat;

    use super::*;

    fn overrides(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn settings_blend_by_how_much_of_a_volume_shows() {
        let base = PostProcess::voxear();
        let table =
            overrides("bloom = { intensity = 0.24 }\nvignette = { intensity = 0.6 }\nfog = { enabled = false }");
        assert_eq!(base.blended(&table, 0.0).unwrap(), base, "nothing of it shows");
        let full = base.blended(&table, 1.0).unwrap();
        assert_eq!((full.bloom.intensity, full.vignette.intensity, full.fog.enabled), (0.24, 0.6, false));
        let half = base.blended(&table, 0.4).unwrap();
        let expected = base.bloom.intensity + (0.24 - base.bloom.intensity) * 0.4;
        assert!((half.bloom.intensity - expected).abs() < 1e-5, "{} against {expected}", half.bloom.intensity);
        assert!(half.fog.enabled, "a switch waits until more than half shows");
        assert!(!base.blended(&table, 0.6).unwrap().fog.enabled);
        assert!(base.blended(&overrides("bloom = { glow = 1.0 }"), 1.0).is_err(), "a setting that does not exist");
    }

    #[test]
    fn a_volume_shows_inside_its_shape_and_fades_out_beyond_it() {
        let volume = PostVolume { blend_distance: 2.0, ..PostVolume::default() };
        // A box 4 wide, 2 high and 4 deep round (10, 1, 0).
        let place = Transform::new(Vec3::new(10.0, 1.0, 0.0), Quat::IDENTITY, Vec3::new(4.0, 2.0, 4.0)).matrix();
        assert_eq!(volume.influence(&place, Vec3::new(10.0, 1.0, 0.0)), 1.0);
        assert_eq!(volume.influence(&place, Vec3::new(13.0, 1.0, 0.0)), 0.5, "a metre out of 2");
        assert_eq!(volume.influence(&place, Vec3::new(20.0, 1.0, 0.0)), 0.0);
        let sphere = PostVolume { shape: VolumeShape::Sphere, weight: 0.5, ..volume.clone() };
        assert_eq!(sphere.influence(&place, Vec3::new(12.0, 1.0, 0.0)), 0.5, "inside, at its weight");
        let global = PostVolume { shape: VolumeShape::Global, ..volume };
        assert_eq!(global.influence(&place, Vec3::splat(1000.0)), 1.0);
    }
}
