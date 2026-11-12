//! Forces on simulated particles: a **Force Field** on an Obj pulls
//! particles to it, drives them away, swirls them round it or blows them
//! along, anything within its reach, less the further out; and a blast
//! throws them out from where it went off, for a moment. Only Particle
//! Systems that take forces are moved, each as much as its Forces says (see
//! `particle_sim`, which hands a frame's fields and blasts to the pass that
//! steps the particles).

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, World};
use voxear_scene::{Destroying, Transform, WorldTransform};

/// The most fields and blasts that move particles in a frame.
pub const MAX_FORCES: usize = 16;
/// How long a blast throws particles out for, in seconds.
pub const BLAST_TIME: f32 = 0.3;

/// What a Force Field does to the particles within its reach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForceKind {
    /// Pulls them toward its Obj.
    #[default]
    Attract,
    /// Drives them away from its Obj.
    Repel,
    /// Swirls them round its Obj's Y axis.
    Vortex,
    /// Blows them along its Obj's +Z, as a fan or a draught does.
    Blow,
}

impl ForceKind {
    pub const ALL: [Self; 4] = [Self::Attract, Self::Repel, Self::Vortex, Self::Blow];

    /// As the editor shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Attract => "Attract",
            Self::Repel => "Repel",
            Self::Vortex => "Vortex",
            Self::Blow => "Blow",
        }
    }
}

/// Something that moves the particles round its Obj: those of every
/// Particle System that takes forces, within `radius` metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ForceField {
    pub kind: ForceKind,
    /// How hard it moves a particle at its middle, in metres a second
    /// faster each second; less further out, nothing at its radius.
    pub strength: f32,
    /// How far it reaches, in metres.
    pub radius: f32,
}

impl Component for ForceField {}

impl Default for ForceField {
    fn default() -> Self {
        Self { kind: ForceKind::Attract, strength: 10.0, radius: 5.0 }
    }
}

/// A blast: particles within `radius` of `at` are thrown out from it, those
/// at its middle to `speed` metres a second, over [`BLAST_TIME`] from when
/// it `started`, in the renderer's time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub at: Vec3,
    pub radius: f32,
    pub speed: f32,
    pub started: f32,
}

/// The blasts going off, a resource of the world: whoever sets one off adds
/// it, and takes it out once it is over ([`ParticleBlasts::keep_live`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParticleBlasts(pub Vec<Blast>);

impl ParticleBlasts {
    /// Drops the blasts that are over by `now`.
    pub fn keep_live(&mut self, now: f32) {
        self.0.retain(|blast| now - blast.started < BLAST_TIME);
    }
}

/// A force as `shaders/particle_sim.slang` reads it (`Force` in
/// `shaders/common.slang`): where it is and how far it reaches, the axis it
/// swirls round or blows along, how hard, and which kind, as `ForceKind`
/// numbers them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuForce {
    pub position: [f32; 3],
    pub radius: f32,
    pub axis: [f32; 3],
    pub strength: f32,
    pub kind: u32,
    pub _pad: [u32; 3],
}

/// This frame's forces at `time`: the blasts still going off, then the
/// world's Force Fields, as many as [`MAX_FORCES`] between them.
pub(crate) fn gather(world: &World, time: f32) -> Vec<GpuForce> {
    let blasts = world.resource::<ParticleBlasts>().map(|blasts| blasts.0.as_slice()).unwrap_or_default();
    let going = blasts.iter().filter(|blast| (0.0..BLAST_TIME).contains(&(time - blast.started)));
    // Thrown out to its speed by the time it is over.
    let mut forces: Vec<GpuForce> = going
        .filter(|blast| blast.radius > 0.0 && blast.speed > 0.0)
        .map(|blast| GpuForce {
            position: blast.at.to_array(),
            radius: blast.radius,
            strength: blast.speed / BLAST_TIME,
            kind: ForceKind::Repel as u32,
            ..GpuForce::default()
        })
        .collect();
    let fields = world.query_ref::<(&ForceField, &Transform, Option<&WorldTransform>)>();
    for (field, transform, at) in fields.without::<Destroying>() {
        if field.strength == 0.0 || field.radius <= 0.0 {
            continue;
        }
        let model = at.map_or_else(|| transform.matrix(), |w| w.0);
        let axis = match field.kind {
            ForceKind::Blow => model.z_axis.truncate(),
            _ => model.y_axis.truncate(),
        };
        forces.push(GpuForce {
            position: model.w_axis.truncate().to_array(),
            radius: field.radius,
            axis: axis.normalize_or(Vec3::Y).to_array(),
            strength: field.strength,
            kind: field.kind as u32,
            _pad: [0; 3],
        });
    }
    forces.truncate(MAX_FORCES);
    forces
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::Quat;

    #[test]
    fn blasts_come_before_fields_and_are_gone_once_they_are_over() {
        let mut world = World::new();
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let fan = ForceField { kind: ForceKind::Blow, strength: 4.0, radius: 3.0 };
        world.spawn((fan, Transform::new(Vec3::new(1.0, 2.0, 3.0), turned, Vec3::ONE)));
        world.spawn((ForceField { strength: 0.0, ..ForceField::default() }, Transform::default()));
        let blast = Blast { at: Vec3::new(0.0, 1.0, 0.0), radius: 6.0, speed: 9.0, started: 10.0 };
        world.insert_resource(ParticleBlasts(vec![blast]));
        let forces = gather(&world, 10.1);
        assert_eq!(forces.len(), 2, "the blast and the fan; a field of no strength moves nothing");
        assert_eq!((forces[0].kind, forces[0].radius), (ForceKind::Repel as u32, 6.0));
        assert!((forces[0].strength - 9.0 / BLAST_TIME).abs() < 1e-3, "to its speed by the time it is over");
        // The fan blows along its Obj's +Z, which is turned to the world's +X.
        assert_eq!(forces[1].kind, ForceKind::Blow as u32);
        assert!((forces[1].axis[0] - 1.0).abs() < 1e-4 && forces[1].axis[2].abs() < 1e-4, "{:?}", forces[1].axis);
        assert_eq!(gather(&world, 10.0 + BLAST_TIME + 0.01).len(), 1, "the blast is over");
        assert_eq!(gather(&world, 9.9).len(), 1, "and has not gone off before it started");
        let blasts = world.resource_mut::<ParticleBlasts>().unwrap();
        blasts.keep_live(10.5);
        assert!(blasts.0.is_empty());
    }
}
