//! Bodies of water besides the sea: a Pool fills a box of ground with water
//! (or lava) at its Obj's height, as a lake, a pond or a flooded room; a Dry
//! Area keeps water out of a box, a ball or a capsule, as the inside of a
//! boat's hull, a cave or a diving bell.
//!
//! Each Pool draws its own surface, a grid over its footprint with the sea's
//! smallest waves on it; every water surface leaves out what lies inside a
//! Dry Area, pixel by pixel, so a moving boat's hull is dry exactly where it
//! is. A Pool below the sea's level is dry to the sea, so a pit by the shore
//! can hold a pond lower than the sea round it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Quat, Vec3};
use voxear_ecs::Component;

use crate::gpu::{GpuWaterBody, cols};

/// The most Pools and Dry Areas a frame draws, the nearest the camera first.
pub const MAX_WATER_BODIES: usize = 64;

/// What fills a body of water.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Liquid {
    #[default]
    Water,
    Lava,
}

impl Liquid {
    pub const ALL: [Self; 2] = [Self::Water, Self::Lava];

    pub fn name(self) -> &'static str {
        match self {
            Self::Water => "Water",
            Self::Lava => "Lava",
        }
    }
}

/// A Dry Area's shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DryShape {
    #[default]
    Box,
    Sphere,
    Capsule,
}

impl DryShape {
    pub const ALL: [Self; 3] = [Self::Box, Self::Sphere, Self::Capsule];

    pub fn name(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Capsule => "Capsule",
        }
    }
}

/// Water at its Obj's height over a box of ground `size` wide (x) and long
/// (z), turned with its Obj: a lake, a pond or a flooded room.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pool {
    /// Across and along, in metres.
    pub size: [f32; 2],
    /// How deep the water is, in metres: its walls and bottom show from
    /// outside, so a Pool raised over the ground is a block of water.
    pub depth: f32,
    pub liquid: Liquid,
    /// How much of the sea's long swell it takes, from 0 for none to 1 as
    /// the sea's: its ripples and short waves it has always.
    pub waves: f32,
    /// The colour of its deep water, sRGB.
    pub deep: [f32; 3],
    /// The colour light takes through its ripples, sRGB.
    pub scatter: [f32; 3],
    /// What a metre of it lets through, by colour, sRGB.
    pub clarity: [f32; 3],
    /// How murky it is: 1 as Clarity says, more to see less far into it.
    pub murk: f32,
}

impl Component for Pool {}

impl Default for Pool {
    fn default() -> Self {
        Self {
            size: [10.0, 10.0],
            depth: 4.0,
            liquid: Liquid::Water,
            waves: 0.0,
            deep: [0.02, 0.18, 0.2],
            scatter: [0.15, 0.6, 0.5],
            clarity: [0.75, 0.93, 0.9],
            murk: 1.0,
        }
    }
}

/// Keeps water out of its shape: a box `size` across, a ball as wide as
/// `size.x`, or a capsule as wide as `size.x` and as tall as `size.y`,
/// turned with its Obj.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DryArea {
    pub shape: DryShape,
    /// In metres.
    pub size: [f32; 3],
}

impl Component for DryArea {}

impl Default for DryArea {
    fn default() -> Self {
        Self { shape: DryShape::Box, size: [2.0, 2.0, 4.0] }
    }
}

impl DryArea {
    /// How far `local` (in the Obj's space, unscaled) is outside the shape,
    /// negative inside.
    pub fn distance(&self, local: Vec3) -> f32 {
        let half = Vec3::from(self.size) * 0.5;
        match self.shape {
            DryShape::Box => {
                let q = local.abs() - half;
                q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
            }
            DryShape::Sphere => local.length() - half.x,
            DryShape::Capsule => {
                let reach = (half.y - half.x).max(0.0);
                let along = Vec3::new(local.x, local.y.clamp(-reach, reach), local.z);
                (local - Vec3::new(0.0, along.y, 0.0)).length() - half.x
            }
        }
    }
}

/// A body placed: where its Obj is and how it is turned.
fn placement(position: Vec3, rotation: Quat) -> (Mat4, Mat4) {
    let to_world = Mat4::from_rotation_translation(rotation, position);
    (to_world, to_world.inverse())
}

impl Pool {
    pub(crate) fn gpu(&self, position: Vec3, rotation: Quat) -> GpuWaterBody {
        let linear = |srgb: [f32; 3]| srgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
        let (to_world, to_local) = placement(position, rotation);
        GpuWaterBody {
            to_world: cols(to_world),
            to_local: cols(to_local),
            size: [self.size[0].max(0.01), self.depth.max(0.01), self.size[1].max(0.01)],
            kind: WATER_POOL,
            deep: linear(self.deep),
            shape: 0,
            scatter: linear(self.scatter),
            waves: self.waves.max(0.0),
            absorb: linear(self.clarity).map(|t| -t.max(1e-3).ln() * self.murk.max(0.0)),
            liquid: self.liquid as u32,
        }
    }

    /// Its surface's height over `point`, when the point is over its
    /// footprint and not below its bottom.
    pub fn surface_over(&self, position: Vec3, rotation: Quat, point: Vec3) -> Option<f32> {
        let local = rotation.inverse() * (point - position);
        let inside = local.x.abs() <= self.size[0] * 0.5 && local.z.abs() <= self.size[1] * 0.5;
        (inside && local.y >= -self.depth).then_some(position.y)
    }
}

impl DryArea {
    pub(crate) fn gpu(&self, position: Vec3, rotation: Quat) -> GpuWaterBody {
        let (to_world, to_local) = placement(position, rotation);
        GpuWaterBody {
            to_world: cols(to_world),
            to_local: cols(to_local),
            size: self.size.map(|s| s.max(0.01)),
            kind: WATER_DRY,
            shape: self.shape as u32,
            ..GpuWaterBody::default()
        }
    }

    /// Whether `point` is inside it, placed at `position`, turned by
    /// `rotation`.
    pub fn contains(&self, position: Vec3, rotation: Quat, point: Vec3) -> bool {
        self.distance(rotation.inverse() * (point - position)) < 0.0
    }
}

/// A body's kind, as `WATER_POOL` and `WATER_DRY` in `shaders/water.slang`.
pub const WATER_POOL: u32 = 1;
pub const WATER_DRY: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dry_area_keeps_water_out_of_its_shape_turned_with_its_obj() {
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let hull = DryArea { shape: DryShape::Box, size: [2.0, 2.0, 6.0] };
        // Six metres long along its own z, which the turn lays along the world's x.
        assert!(hull.contains(Vec3::ZERO, turned, Vec3::new(2.5, 0.0, 0.0)));
        assert!(!hull.contains(Vec3::ZERO, turned, Vec3::new(0.0, 0.0, 2.5)));
        let ball = DryArea { shape: DryShape::Sphere, size: [4.0, 4.0, 4.0] };
        assert!(ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(11.9, 0.0, 0.0)));
        assert!(!ball.contains(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::new(12.1, 0.0, 0.0)));
        let bell = DryArea { shape: DryShape::Capsule, size: [2.0, 6.0, 2.0] };
        assert!(bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(0.0, 2.8, 0.0)) && !bell.contains(Vec3::ZERO, Quat::IDENTITY, Vec3::new(1.2, 0.0, 0.0)));
    }

    #[test]
    fn a_pools_surface_is_over_its_footprint_at_its_height() {
        let pond = Pool { size: [4.0, 10.0], ..Pool::default() };
        let at = Vec3::new(0.0, 2.0, 0.0);
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(1.5, -1.0, 4.5)), Some(2.0));
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(2.5, 0.0, 0.0)), None);
        assert_eq!(pond.surface_over(at, Quat::IDENTITY, Vec3::new(0.0, -3.0, 0.0)), None, "under its bottom, 4 m down");
    }
}
