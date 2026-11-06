//! Mist: a trail of fog a moving Obj leaves behind it while it is
//! emitting, as a wizard dashing in a cloud of mist does. The engine keeps
//! each Mist's trail of points (`MistTrail`); the smoke march draws it with
//! the smokes (`shaders/smoke.slang`), lit as they are, each piece of trail
//! widening, rising and thinning away as it ages.

use serde::{Deserialize, Serialize};
use voxear_core::math::Vec3;
use voxear_ecs::{Component, World};
use voxear_scene::Destroying;

use crate::gpu::GpuMist;

/// The most mists a frame draws, the nearest the camera, and the most
/// points a mist's trail keeps.
pub const MAX_MISTS: usize = 8;
pub const MAX_MIST_POINTS: usize = 32;

/// A trail of mist left behind its Obj as it moves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mist {
    /// Whether it is leaving mist behind it now. Stopped, what it left
    /// lingers and thins away.
    pub emitting: bool,
    /// How wide the trail is where it leaves its Obj, in metres; it widens
    /// to twice this as it thins.
    pub width: f32,
    /// How thick it is: how much of the light a metre of it takes.
    pub density: f32,
    /// Its colour, linear: how much of the light it scatters, by colour.
    pub color: [f32; 3],
    /// How long each piece of trail lasts, in seconds, before it is gone.
    pub linger: f32,
    /// Which swirls: two mists alike but for this look different.
    pub seed: u32,
}

impl Component for Mist {}

impl Default for Mist {
    fn default() -> Self {
        Self { emitting: true, width: 0.8, density: 3.0, color: [0.72, 0.8, 0.95], linger: 1.5, seed: 0 }
    }
}

/// The points a Mist's trail runs through, oldest first, each where its
/// Obj was and when, in the renderer's time. Kept by the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MistTrail {
    pub points: Vec<(Vec3, f32)>,
}

impl Component for MistTrail {}

/// How far each piece of a trail rises as it ages, in metres a second.
pub const MIST_RISE: f32 = 0.4;

impl MistTrail {
    /// Where the point `index` of `mist`'s trail is at `time`, risen as it
    /// has aged, and how wide and thick the trail is there, from 0 to 1: as
    /// `mistDensity` in `shaders/smoke.slang` has it.
    pub fn piece(&self, mist: &Mist, index: usize, time: f32) -> (Vec3, f32, f32) {
        let (at, made) = self.points[index];
        let aged = ((time - made) / mist.linger.max(1e-3)).clamp(0.0, 1.0);
        let radius = mist.width.max(0.01) * 0.5 * (1.0 + aged);
        (at + Vec3::Y * MIST_RISE * (time - made).max(0.0), radius, (1.0 - aged).powf(1.5))
    }
}

/// This frame's mists, the nearest `camera` first, and their trails' points
/// (where, and how old in seconds), at `time`.
pub(crate) fn gather(world: &World, camera: Vec3, time: f32) -> (Vec<GpuMist>, Vec<[f32; 4]>) {
    let query = world.query_ref::<(&Mist, &MistTrail)>();
    let mut mists: Vec<(f32, &Mist, &MistTrail)> = query
        .without::<Destroying>()
        .into_iter()
        .filter(|(mist, trail)| trail.points.iter().any(|&(_, made)| time - made < mist.linger) && mist.density > 0.0)
        .map(|(mist, trail)| {
            let near = trail.points.iter().map(|(at, _)| at.distance_squared(camera)).fold(f32::MAX, f32::min);
            (near, mist, trail)
        })
        .collect();
    mists.sort_by(|a, b| a.0.total_cmp(&b.0));
    mists.truncate(MAX_MISTS);
    let mut points = Vec::new();
    let gpu = mists
        .into_iter()
        .map(|(_, mist, trail)| {
            let first = points.len() as u32;
            let mut low = Vec3::splat(f32::MAX);
            let mut high = Vec3::splat(f32::MIN);
            let kept = &trail.points[trail.points.len().saturating_sub(MAX_MIST_POINTS)..];
            for (i, &(at, made)) in kept.iter().enumerate() {
                let index = trail.points.len() - kept.len() + i;
                let (risen, radius, _) = trail.piece(mist, index, time);
                // Drifting a little off its line as it swirls apart.
                // And as far again as the swirls carry what is read there (`mistDensity`'s warp).
                let reach = radius * 1.6 + mist.width.max(0.01) * 0.8;
                low = low.min(risen - reach);
                high = high.max(risen + reach);
                points.push([at.x, at.y, at.z, time - made]);
            }
            GpuMist {
                low: low.to_array(),
                width: mist.width.max(0.01),
                high: high.to_array(),
                density: mist.density.max(0.0),
                color: mist.color.map(|c| c.clamp(0.0, 1.0)),
                linger: mist.linger.max(1e-3),
                points: first,
                count: kept.len() as u32,
                seed: (mist.seed % 1024) as f32 * 17.31,
                _pad: 0.0,
            }
        })
        .collect();
    (gpu, points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_scene::Transform;

    #[test]
    fn a_trail_widens_rises_and_thins_as_it_ages_and_old_ones_are_not_drawn() {
        let mist = Mist::default();
        let trail = MistTrail { points: vec![(Vec3::ZERO, 0.0), (Vec3::X, 1.0)] };
        let (old, wide, thin) = trail.piece(&mist, 0, 1.0);
        let (new, narrow, thick) = trail.piece(&mist, 1, 1.0);
        assert!(old.y > 0.3 && new.y == 0.0, "the older risen");
        assert!(wide > narrow && thin < thick && thick == 1.0);
        let mut world = World::new();
        world.spawn((mist, trail.clone(), Transform::default()));
        assert_eq!(gather(&world, Vec3::ZERO, 2.0).0.len(), 1);
        assert_eq!(gather(&world, Vec3::ZERO, 2.0).1.len(), 2);
        assert!(gather(&world, Vec3::ZERO, 3.0).0.is_empty(), "all of it gone");
    }
}
