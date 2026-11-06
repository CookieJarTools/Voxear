//! Settling (inertialization): when what plays changes, each bone's offset
//! from the new pose, and how fast it was moving, die away as a critically
//! damped spring does, so nothing pops.

use voxear_core::math::{Quat, Vec3};
use voxear_render::BonePose;

/// Each bone's offset from the new pose, and how fast it was changing, when
/// the change came, and how long ago.
#[derive(Clone, Debug)]
pub(crate) struct Settling {
    offsets: Vec<(Vec3, Vec3, Vec3, Vec3)>,
    pub since: f32,
    pub halflife: f32,
}

impl Settling {
    /// Each bone's offset from the new pose and how fast it was changing,
    /// from the last two poses shown and the new pose now and a frame
    /// before, each two `delta` apart.
    pub fn between(old: [&[BonePose]; 2], new: [&[BonePose]; 2], delta: f32, halflife: f32) -> Self {
        let offsets = (0..old[0].len().min(new[0].len()).min(old[1].len()).min(new[1].len()))
            .map(|bone| {
                let ([now, then], [target, target_then]) = (old.map(|p| p[bone]), new.map(|p| p[bone]));
                let moved = now.translation - target.translation;
                let speed =
                    ((now.translation - then.translation) - (target.translation - target_then.translation)) / delta;
                let turned = turn(now.rotation, target.rotation);
                let spin = (turn(now.rotation, then.rotation) - turn(target.rotation, target_then.rotation)) / delta;
                (moved, speed, turned, spin)
            })
            .collect();
        Self { offsets, since: 0.0, halflife: halflife.max(1e-3) }
    }

    /// Moves on by `delta` and adds what is left of each bone's offset to
    /// `pose`; false once the offsets have died away.
    pub fn settle(&mut self, delta: f32, pose: &mut [BonePose]) -> bool {
        self.since += delta;
        self.apply(pose);
        self.since <= self.halflife * 8.0
    }

    /// Adds what is left of each bone's offset to `pose`, the offsets dying
    /// away as a critically damped spring does.
    fn apply(&self, pose: &mut [BonePose]) {
        let y = 4.0 * std::f32::consts::LN_2 / self.halflife;
        let t = self.since;
        let fade = |x: Vec3, v: Vec3| (x + (v + x * y) * t) * (-y * t).exp();
        for (bone, &(moved, speed, turned, spin)) in pose.iter_mut().zip(&self.offsets) {
            bone.translation += fade(moved, speed);
            bone.rotation = (Quat::from_scaled_axis(fade(turned, spin)) * bone.rotation).normalize();
        }
    }
}

/// The turn from `to` to `from`, as an axis scaled by its angle, the
/// shorter way round.
fn turn(from: Quat, to: Quat) -> Vec3 {
    let mut q = from * to.inverse();
    if q.w < 0.0 {
        q = -q;
    }
    q.to_scaled_axis()
}
