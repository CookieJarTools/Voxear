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
use voxear_ecs::{Component, Entity, World};
use voxear_scene::{Destroying, Switch, Switched, Transform};

use crate::{FoliageMotion, gpu::GpuWind, weather::WeatherAmounts};

/// Where a Wind Zone blows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindShape {
    /// Over the whole level: the first one sways the plants and can blow the
    /// sea's wind (an Ocean's Wind Source).
    #[default]
    Global,
    /// Only inside a ball or a box, turned with its Obj, where it makes the
    /// sea's short waves as rough as its strength says.
    Sphere,
    Box,
}

impl WindShape {
    pub const ALL: [Self; 3] = [Self::Global, Self::Sphere, Self::Box];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Sphere => "Sphere",
            Self::Box => "Box",
        }
    }
}

/// Wind blowing along its Obj's +Z: over the whole level, the first Global
/// one used, as Unity's directional Wind Zone; or a gust of its own over
/// part of the sea, a calm bay behind an island or a squall out at sea,
/// inside a Sphere or a Box.
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
    /// Where it blows: everywhere, or inside a ball or a box, where the sea's
    /// short waves are as rough as its strength says, 0 calm and 1 as the sea
    /// is.
    pub shape: WindShape,
    /// A Box's size, or a Sphere's width (x), in metres.
    pub size: [f32; 3],
    /// How far outside its shape its roughness fades into the sea's, in metres.
    pub falloff: f32,
}

impl Component for WindZone {}

impl Default for WindZone {
    fn default() -> Self {
        Self {
            strength: 1.0,
            gusts: 0.5,
            gust_size: 20.0,
            gust_speed: 6.0,
            turbulence: 0.5,
            shape: WindShape::Global,
            size: [60.0, 20.0, 60.0],
            falloff: 15.0,
        }
    }
}

impl WindZone {
    /// A Sphere or Box's sway over the sea's short waves, as the shaders read
    /// it among the water bodies: placed at `position`, turned by `rotation`.
    pub(crate) fn gpu_local(&self, position: Vec3, rotation: Quat) -> crate::gpu::GpuWaterBody {
        let to_world = voxear_core::math::Mat4::from_rotation_translation(rotation, position);
        crate::gpu::GpuWaterBody {
            to_world: crate::gpu::cols(to_world),
            to_local: crate::gpu::cols(to_world.inverse()),
            size: self.size.map(|s| s.max(0.01)),
            kind: crate::water_bodies::WATER_WIND,
            shape: if self.shape == WindShape::Sphere { 1 } else { 0 },
            waves: self.strength.max(0.0),
            lava: [self.falloff.max(0.01), 0.0, 0.0, 0.0],
            ..crate::gpu::GpuWaterBody::default()
        }
    }

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
            bend: 1.0,
        }
    }
}

/// The level's first Global Wind Zone that is on, and how its Obj is
/// turned, which blows over the whole level.
pub(crate) fn level_zone(world: &World) -> Option<(WindZone, Quat)> {
    let switched = Switched::of(world);
    let zones = world.query_ref::<(Entity, &WindZone, &Transform)>().without::<Destroying>();
    let global = |(entity, zone, _): &(Entity, &WindZone, &Transform)| {
        zone.shape == WindShape::Global && switched.is_on(world, *entity, Switch::WindZone)
    };
    zones.into_iter().find(global).map(|(_, zone, transform)| (*zone, transform.rotation))
}

/// The wind plants feel over the level under `weather`: the first Global
/// Wind Zone that is on, or with none a fair day's wind as hard as
/// `motion` says, either blowing harder in rain and storms; and how far
/// plants bend, as `motion` says. Strength 0 in still weather with neither.
pub fn plants_wind(world: &World, weather: WeatherAmounts, motion: FoliageMotion) -> GpuWind {
    let zone = level_zone(world).map(|(zone, rotation)| (weather.wind(zone), rotation));
    let zone = zone.or_else(|| weather.wind_over(motion.wind));
    let mut wind = zone.map_or_else(GpuWind::default, |(zone, rotation)| zone.gpu(rotation));
    wind.bend = motion.bend;
    wind
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

    #[test]
    fn plants_feel_a_wind_zone_or_else_a_fair_days_wind_and_bend_as_the_game_says() {
        let mut world = World::new();
        let fair = WeatherAmounts::default();
        let motion = FoliageMotion { wind: 0.4, bend: 2.0, ..FoliageMotion::default() };
        let breeze = plants_wind(&world, fair, motion);
        assert_eq!((breeze.strength, breeze.bend), (0.4, 2.0));
        assert!((breeze.direction[0] - 1.0).abs() < 1e-4, "along x, as clouds drift: {:?}", breeze.direction);
        let still = plants_wind(&world, fair, FoliageMotion { wind: 0.0, ..motion });
        assert_eq!((still.strength, still.bend), (0.0, 2.0), "still, but pushes still bend plants");
        world.spawn((WindZone { strength: 1.5, ..WindZone::default() }, Transform::default()));
        let zone = plants_wind(&world, fair, motion);
        assert_eq!(zone.strength, 1.5, "a Wind Zone in place of the fair day's wind");
        assert!((zone.direction[1] - 1.0).abs() < 1e-4, "along its Obj's +Z: {:?}", zone.direction);
    }
}
