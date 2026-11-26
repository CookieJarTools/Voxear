//! Where a ray meets an endless Voxel Volume's ground, its trees' logs and
//! leaves, and its edits, as the CPU finds them ([`super::solid`]). Its
//! grass and flowers are not met.
//!
//! The ray is walked a voxel at a time near the ground, and in steps as long
//! as the ground, the balls filled and the trees are far from it elsewhere.

use voxear_core::math::{IVec3, Vec3};

use super::{
    VoxelVolume,
    solid::Ground,
    trees::{self, AIR},
};

/// Where a ray met a volume's ground: the point, in the world, on the face
/// of the voxel it met; which way that face looks; and how far along the
/// ray, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelRayHit {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

/// How many voxels the walk takes a voxel at a time before it looks again
/// how far the ground is.
const FINE_STEPS: usize = 192;
/// How near the ground, in voxels, the walk goes a voxel at a time.
const NEAR: f32 = 6.0;
/// The share of how far the ground is over or under a point that a step
/// takes: the ground climbs at most about one in two between, and a
/// Volume's numbers change up to three times as fast as distance.
pub(super) const HEIGHT_STEP: f32 = 0.4;
pub(super) const DENSITY_STEP: f32 = 0.3;

/// Whether voxel `c` is anything a ray meets.
fn is(ground: &mut Ground, c: IVec3) -> bool {
    ground.at(c) != AIR
}

/// Where the ray from `origin` along `direction`, in the world, first meets
/// the ground of the endless volume whose corner is at `corner`, within
/// `reach` metres: none where it meets none.
pub fn raycast(volume: &VoxelVolume, corner: Vec3, origin: Vec3, direction: Vec3, reach: f32) -> Option<VoxelRayHit> {
    let d = direction.normalize_or_zero();
    if d == Vec3::ZERO || !volume.endless {
        return None;
    }
    let woods = trees::woods(&volume.trees, volume.voxels_per_metre.clamp(1, super::MOST_VOXELS_A_METRE));
    let mut ground = Ground::new(volume, woods.as_deref());
    let (voxel, tall) = (ground.voxel, ground.tall);
    let o = (origin - corner) / voxel;
    let reach = reach / voxel;
    // Into the volume's height: under its top, and over its floor, which is solid.
    let mut t = 0.0f32;
    if o.y >= tall as f32 {
        if d.y >= 0.0 {
            return None;
        }
        t = (o.y - tall as f32) / -d.y;
    }
    let hit = |t: f32, axis: Option<usize>| {
        let mut normal = -d;
        if let Some(axis) = axis {
            normal = Vec3::ZERO;
            normal[axis] = -d[axis].signum();
        }
        VoxelRayHit { point: corner + (o + d * t) * voxel, normal, distance: t * voxel }
    };
    if is(&mut ground, (o + d * t).floor().as_ivec3()) {
        return (t <= reach).then(|| hit(t, None));
    }
    while t < reach {
        let clear = ground.clear(o + d * t);
        if clear > NEAR {
            t += clear;
            continue;
        }
        // Near the ground: a voxel at a time, stepping whole cells.
        let p = o + d * t;
        let mut c = p.floor().as_ivec3();
        // A ray along an axis goes a hair's breadth across it, as the GPU's walk takes it.
        let across = d.map(|a| if a.abs() < 1e-7 { 1e-7f32.copysign(a) } else { a });
        let step = IVec3::new(across.x.signum() as i32, across.y.signum() as i32, across.z.signum() as i32);
        let inv = Vec3::new(1.0 / across.x, 1.0 / across.y, 1.0 / across.z);
        let edge = |c: IVec3, a: usize| (c[a] as f32 + if across[a] > 0.0 { 1.0 } else { 0.0 } - o[a]) * inv[a];
        let mut next = Vec3::new(edge(c, 0), edge(c, 1), edge(c, 2));
        if is(&mut ground, c) {
            return Some(hit(t, None));
        }
        for _ in 0..FINE_STEPS {
            let axis = if next.x < next.y && next.x < next.z {
                0
            } else if next.y < next.z {
                1
            } else {
                2
            };
            t = next[axis];
            if t >= reach {
                return None;
            }
            c[axis] += step[axis];
            next[axis] = edge(c, axis);
            if is(&mut ground, c) {
                return Some(hit(t, Some(axis)));
            }
            if c.y >= tall && d.y >= 0.0 {
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use voxear_core::math::Quat;

    use super::*;
    use crate::{VoxelEdit, VoxelGround, VoxelMaterial};

    fn flat() -> VoxelVolume {
        let ground = VoxelGround { height: 2.0, hills: 0.0, ..VoxelGround::default() };
        VoxelVolume { endless: true, size: Vec3::new(16.0, 8.0, 16.0), ground: Some(ground), ..VoxelVolume::default() }
    }

    #[test]
    fn a_ray_meets_the_ground_on_the_top_of_its_voxels_and_goes_into_a_pit_and_onto_a_fill() {
        let mut volume = flat();
        let corner = Vec3::new(100.0, 10.0, -50.0);
        let down = Vec3::new(0.0, -1.0, 0.0);
        // Flat ground 2 m up: its top voxel's top is 2 m and a voxel up.
        let from = corner + Vec3::new(3.03, 7.0, 3.03);
        let hit = raycast(&volume, corner, from, down, 100.0).expect("the ground");
        assert!((hit.point.y - (corner.y + 2.0 + 1.0 / 16.0)).abs() < 1e-3, "{hit:?}");
        assert_eq!(hit.normal, Vec3::Y);
        assert!((hit.distance - (7.0 - 2.0 - 1.0 / 16.0)).abs() < 1e-3);
        // Along the ground and away from it: nothing.
        assert!(raycast(&volume, corner, from, Vec3::new(1.0, 0.2, 0.0), 1000.0).is_none(), "upwards");
        assert!(raycast(&volume, corner, from, down, 3.0).is_none(), "out of reach");
        // A pit carved where the ray comes down, and a ball of rock filled further along over the ground.
        let pit = VoxelEdit::ball(Vec3::new(3.0, 2.0, 3.0), 1.0, None);
        let rock = VoxelEdit::ball(Vec3::new(9.0, 4.0, 3.0), 0.5, Some(VoxelMaterial::Rock));
        volume.edits = Arc::new(vec![pit, rock]);
        let deeper = raycast(&volume, corner, from, down, 100.0).expect("the pit's floor");
        assert!(deeper.point.y < corner.y + 1.1 && deeper.point.y > corner.y + 0.9, "{deeper:?}");
        let across = raycast(&volume, corner, corner + Vec3::new(3.0, 4.0, 3.0), Vec3::X, 100.0).expect("the rock");
        assert!((across.point.x - (corner.x + 8.5)).abs() < 0.07 && across.normal == -Vec3::X, "{across:?}");
    }

    #[test]
    fn a_ray_from_far_off_finds_the_hills_where_walking_every_voxel_would() {
        let ground = VoxelGround { height: 3.0, hills: 2.0, ..VoxelGround::default() };
        let volume = VoxelVolume { ground: Some(ground), ..flat() };
        let from = Vec3::new(-40.0, 7.5, 5.0);
        for slope in [-0.05f32, -0.1, -0.2, -0.4] {
            let d = Vec3::new(1.0, slope, 0.3).normalize();
            let hit = raycast(&volume, Vec3::ZERO, from, d, 200.0).expect("the hills");
            // The first voxel along the ray that is solid, found by walking it in small steps.
            let walked = (0..40_000).map(|i| i as f32 * 0.005).find(|&t| {
                let p = (from + d * t) * 16.0;
                let c = p.floor();
                c.y <= 0.0 || c.y <= (ground.height_at((c.x + 0.5) / 16.0, (c.z + 0.5) / 16.0) * 16.0).floor()
            });
            assert!((hit.distance - walked.unwrap()).abs() < 0.02, "{slope}: {hit:?} against {walked:?}");
        }
    }

    #[test]
    fn a_ray_meets_a_print_where_its_voxels_are_turned_with_it_and_a_carve_after_it_cuts_it() {
        use crate::{VoxelPrint, VoxelPrintSettings};
        // A wall 2 m long, 1 m tall and a quarter thick, its origin at the middle of its foot.
        let print = VoxelPrint::from_voxels(
            [32, 16, 4],
            16,
            Vec3::new(16.0, 0.0, 2.0),
            vec![[120, 100, 80, 255]],
            VoxelPrintSettings::default(),
            |_| 1,
        );
        let print = Arc::new(print);
        let mut volume = flat();
        let foot = Vec3::new(8.0, 2.0 + 1.0 / 16.0, 8.0);
        volume.edits = Arc::new(vec![VoxelEdit::printed(print.clone(), foot, Quat::IDENTITY)]);
        // Along Z, through it: its face an eighth of a metre before its middle.
        let from = Vec3::new(8.5, 2.5, 0.0);
        let hit = raycast(&volume, Vec3::ZERO, from, Vec3::Z, 100.0).expect("the wall");
        assert!((hit.point.z - (8.0 - 0.125)).abs() < 1e-3 && hit.normal == -Vec3::Z, "{hit:?}");
        // Down onto its top.
        let top = raycast(&volume, Vec3::ZERO, Vec3::new(8.5, 6.0, 8.0), -Vec3::Y, 100.0).expect("its top");
        assert!((top.point.y - (foot.y + 1.0)).abs() < 1e-3, "{top:?}");
        // Turned a quarter, it runs along Z: a ray along Z through it meets its end, a metre before its middle.
        let turned = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        volume.edits = Arc::new(vec![VoxelEdit::printed(print.clone(), foot, turned)]);
        let from = Vec3::new(8.03, 2.5, 0.0);
        let hit = raycast(&volume, Vec3::ZERO, from, Vec3::Z, 100.0).expect("its end");
        assert!((hit.point.z - 7.0).abs() < 1e-3, "{hit:?}");
        // A ball carved out after it cuts through it.
        let carve = VoxelEdit::ball(Vec3::new(8.03, 2.5, 7.0), 0.4, None);
        volume.edits = Arc::new(vec![VoxelEdit::printed(print, foot, turned), carve]);
        let hit = raycast(&volume, Vec3::ZERO, from, Vec3::Z, 100.0).expect("further in");
        assert!(hit.point.z > 7.3, "{hit:?}");
    }
}
