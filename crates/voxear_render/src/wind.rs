//! Wind for plants: a Wind Zone on an Obj blows along its +Z, and every
//! plant whose material says it moves in the wind bends with it, as The
//! Vegetation Engine's wind does in Unity. The shader is `shaders/wind.slang`,
//! which every pass that places vertices calls, so a plant's shadow, depth
//! and motion move with it.
//!
//! Plants need no setup: a plant bends from the ground up by its height,
//! read from its mesh's bounds, so it needs its pivot at its base; its
//! leaves flutter along their own normals; and gusts are noise drifting
//! downwind across the level, so a field of grass shows waves going
//! through it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Quat, Vec3};
use voxear_ecs::Component;

use crate::gpu::GpuWind;

/// Wind over the whole level, blowing along its Obj's +Z. The first one in
/// the level is used, as Unity's directional Wind Zone.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WindZone {
    /// How hard it blows: 1 a breeze that sways trees, 3 a gale.
    pub strength: f32,
    /// How much gusts add to it, from 0 for steady wind.
    pub gusts: f32,
    /// How wide a gust is across, in metres.
    pub gust_size: f32,
    /// How fast gusts travel downwind, in metres a second.
    pub gust_speed: f32,
    /// How much plants sway back and forth and across the wind, rather than
    /// lean steadily with it.
    pub turbulence: f32,
}

impl Component for WindZone {}

impl Default for WindZone {
    fn default() -> Self {
        Self { strength: 1.0, gusts: 0.5, gust_size: 20.0, gust_speed: 6.0, turbulence: 0.5 }
    }
}

impl WindZone {
    /// The wind as shaders read it, blowing along `rotation`'s +Z.
    pub(crate) fn gpu(&self, rotation: Quat) -> GpuWind {
        let along = rotation * Vec3::Z;
        let flat = Vec3::new(along.x, 0.0, along.z).normalize_or(Vec3::Z);
        GpuWind {
            direction: [flat.x, flat.z],
            strength: self.strength.max(0.0),
            gusts: self.gusts.max(0.0),
            gust_size: self.gust_size.max(0.1),
            gust_speed: self.gust_speed,
            turbulence: self.turbulence.max(0.0),
            _pad: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_blows_level_along_its_objs_forward() {
        let tilted = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_x(0.4);
        let wind = WindZone::default().gpu(tilted);
        assert!((wind.direction[0] - 1.0).abs() < 1e-4 && wind.direction[1].abs() < 1e-4, "{:?}", wind.direction);
        assert_eq!(WindZone { strength: -1.0, ..WindZone::default() }.gpu(Quat::IDENTITY).strength, 0.0);
    }
}
