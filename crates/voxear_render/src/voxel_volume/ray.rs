//! Where a ray meets an endless Voxel Volume's ground, worked out on the
//! CPU as the GPU makes it (`shaders/voxel_world.slang`), to the voxel: its
//! own ground's hills, or its generator's height, or its generator's Volume
//! with its overhangs and caves, cut by its rivers as the GPU's columns are
//! (`voxel_volume/rivers.rs`); the floor solid; and its edits over that,
//! in order, the Voxel Prints printed into it with them. Its grass, flowers and trees are not met, as the CPU does not
//! make them. Every machine finds the same, as an edit sent over the network
//! wants.
//!
//! The ray is walked a voxel at a time near the ground, and in steps as long
//! as the ground and the balls filled are far from it elsewhere.

use std::collections::HashMap;

use voxear_core::math::{IVec3, Vec3};

use super::{MOST_VOXELS_A_METRE, VoxelVolume, rivers::RiverLayer};

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
const HEIGHT_STEP: f32 = 0.4;
const DENSITY_STEP: f32 = 0.3;

/// What the ground of an endless volume is, voxel by voxel, with what was
/// worked out kept.
struct Ground<'a> {
    volume: &'a VoxelVolume,
    voxel: f32,
    tall: i32,
    heights: HashMap<(i32, i32), i32>,
    /// What a generator's Volume's columns carry up to their cells.
    columns: HashMap<(i32, i32), Vec<f32>>,
    /// Its generator's rivers, where it runs any.
    rivers: Option<std::sync::Arc<RiverLayer>>,
}

impl Ground<'_> {
    /// The ground's height at voxel column (`x`, `z`), in voxels over the
    /// floor: its top voxel.
    fn height(&mut self, x: i32, z: i32) -> i32 {
        if let Some(&h) = self.heights.get(&(x, z)) {
            return h;
        }
        let v = self.voxel;
        let (mx, mz) = ((x as f32 + 0.5) * v, (z as f32 + 0.5) * v);
        let h = match (&self.volume.generator, &self.volume.ground) {
            (Some(generator), _) => {
                let (gx, gz) = (mx + generator.at[0], mz + generator.at[1]);
                let height = match &self.rivers {
                    Some(rivers) => rivers.bare().height_at(gx, gz) + self.cut(x, z),
                    None => generator.world.height_at(gx, gz),
                };
                ((height - generator.floor) / v).floor() as i32
            }
            (None, Some(ground)) => (ground.height_at(mx, mz) / v).floor() as i32,
            (None, None) => -1,
        };
        self.heights.insert((x, z), h);
        h
    }

    /// How far into a generator's Volume voxel `c` is, in metres (out of it
    /// where negative): none without one.
    fn density(&mut self, c: IVec3) -> Option<f32> {
        let generator = self.volume.generator.as_ref()?;
        let volume = generator.world.volume()?;
        let v = self.voxel;
        let (gx, gz) = ((c.x as f32 + 0.5) * v + generator.at[0], (c.z as f32 + 0.5) * v + generator.at[1]);
        // Carried up from the ground without the rivers, as the GPU's columns carry it, and the rivers' cut moving
        // the surface after.
        let bare = self.rivers.as_ref().map(|rivers| rivers.bare()).unwrap_or(&generator.world);
        let carried = self.columns.entry((c.x, c.z)).or_insert_with(|| {
            let height = bare.height_at(gx, gz);
            volume.carried(&[gx], &[gz], &[height]).rows().iter().map(|row| row[0]).collect()
        });
        let rows: Vec<&[f32]> = carried.iter().map(std::slice::from_ref).collect();
        let gy = (c.y as f32 + 0.5) * v + generator.floor;
        let (mut density, mut ground) = ([0.0], [0.0]);
        let cells = voxear_terrain::generate::Cells { x: &[gx], y: &[gy], z: &[gz], carried: &rows, size: v };
        volume.densities(cells, false, &mut density, &mut ground);
        let cut = if self.rivers.is_some() { self.cut(c.x, c.z) } else { 0.0 };
        Some(density[0] + cut)
    }

    /// How much the rivers raise the ground at voxel column (`x`, `z`), as
    /// the GPU reads it from the points of the column it is in.
    fn cut(&self, x: i32, z: i32) -> f32 {
        let Some(rivers) = &self.rivers else { return 0.0 };
        let column = [x.div_euclid(64), z.div_euclid(64)];
        let grid = rivers.grid(column);
        if !grid.laid {
            return 0.0;
        }
        grid.cut_at(x - column[0] * 64, z - column[1] * 64)
    }

    /// Whether voxel `c` is solid as the ground makes it, before its edits.
    fn made(&mut self, c: IVec3) -> bool {
        if c.y <= 0 {
            return true;
        }
        if c.y >= self.tall {
            return false;
        }
        match self.density(c) {
            Some(density) => density > 0.0,
            None => c.y <= self.height(c.x, c.z),
        }
    }

    /// Whether voxel `c` is solid, its edits over the ground in order: a
    /// print's solid voxels and a ball filled are solid, a ball carved out
    /// is not.
    fn solid(&mut self, c: IVec3) -> bool {
        let mut solid = self.made(c);
        let middle = (c.as_vec3() + 0.5) * self.voxel;
        for edit in self.volume.edits.iter() {
            // Only those near enough are worked out.
            if middle.distance_squared(edit.at) > (edit.radius + self.voxel) * (edit.radius + self.voxel) {
                continue;
            }
            if edit.holds(c, self.voxel) != 0 {
                solid = edit.print.is_some() || edit.fill.is_some();
            }
        }
        solid
    }

    /// How far point `p` (in voxels) surely is from anything solid, in
    /// voxels: over the ground and out of every ball filled and every
    /// print's reach, by how far.
    fn clear(&mut self, p: Vec3) -> f32 {
        let c = p.floor().as_ivec3();
        let ground = match self.density(c) {
            Some(density) => -density / self.voxel * DENSITY_STEP,
            None => (p.y - (self.height(c.x, c.z) + 1) as f32) * HEIGHT_STEP,
        };
        let world = p * self.voxel;
        let fills = self.volume.edits.iter().filter(|edit| edit.fill.is_some() || edit.print.is_some());
        let near = fills.map(|edit| (world.distance(edit.at) - edit.radius) / self.voxel).fold(f32::MAX, f32::min);
        ground.min(near - 1.0).min(p.y)
    }
}

/// Where the ray from `origin` along `direction`, in the world, first meets
/// the ground of the endless volume whose corner is at `corner`, within
/// `reach` metres: none where it meets none.
pub fn raycast(volume: &VoxelVolume, corner: Vec3, origin: Vec3, direction: Vec3, reach: f32) -> Option<VoxelRayHit> {
    let d = direction.normalize_or_zero();
    if d == Vec3::ZERO || !volume.endless {
        return None;
    }
    let voxel = 1.0 / volume.voxels_per_metre.clamp(1, MOST_VOXELS_A_METRE) as f32;
    let tall = (volume.size.y / voxel).ceil() as i32;
    let rivers =
        volume.generator.as_ref().and_then(|generator| super::rivers::layer_of(generator, volume.voxels_per_metre));
    let mut ground = Ground { volume, voxel, tall, heights: HashMap::new(), columns: HashMap::new(), rivers };
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
    if ground.solid((o + d * t).floor().as_ivec3()) {
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
        if ground.solid(c) {
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
            if ground.solid(c) {
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
