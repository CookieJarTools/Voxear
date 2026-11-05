//! A Path: points in its Obj's space joined by a smooth curve through every
//! one of them, each with a width, open or closed into a loop. A Stream runs
//! along one; roads and rails can later.
//!
//! The curve is a centripetal Catmull-Rom spline, which passes through each
//! point without looping or overshooting where the points are unevenly
//! spaced, so a point dragged in the Scene view bends the curve only near
//! it.

use serde::{Deserialize, Serialize};
use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::Component;

/// One point of a Path, in its Obj's space, and how wide the Path is there,
/// in metres.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathPoint {
    pub position: [f32; 3],
    pub width: f32,
}

impl Default for PathPoint {
    fn default() -> Self {
        Self { position: [0.0; 3], width: 4.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Path {
    pub points: Vec<PathPoint>,
    /// Whether the last point joins back to the first.
    pub closed: bool,
}

impl Component for Path {}

impl Default for Path {
    /// Twenty metres along the Obj's forward, bending to its right and
    /// falling a metre and a half, as water would run.
    fn default() -> Self {
        let point = |x: f32, y: f32, z: f32| PathPoint { position: [x, y, z], width: 4.0 };
        Self { points: vec![point(0.0, 0.0, 0.0), point(0.0, -0.5, 10.0), point(4.0, -1.5, 20.0)], closed: false }
    }
}

/// A place along a Path: where it is, which way the Path runs there (unit),
/// how wide it is, and how far along it, in metres from its start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSample {
    pub position: Vec3,
    pub tangent: Vec3,
    pub width: f32,
    pub along: f32,
}

impl Path {
    /// How many stretches of curve join its points.
    pub fn segments(&self) -> usize {
        match self.points.len() {
            0 | 1 => 0,
            n if self.closed && n > 2 => n,
            n => n - 1,
        }
    }

    /// Point `index`, wrapping round a closed Path and running straight on
    /// past an open one's ends.
    fn point(&self, index: isize) -> (Vec3, f32) {
        let n = self.points.len() as isize;
        let at = |i: isize| {
            let p = self.points[i.rem_euclid(n) as usize];
            (Vec3::from(p.position), p.width)
        };
        match (self.closed && n > 2, index) {
            (true, _) => at(index),
            (false, i) if i < 0 => (2.0 * at(0).0 - at(1).0, at(0).1),
            (false, i) if i >= n => (2.0 * at(n - 1).0 - at(n - 2).0, at(n - 1).1),
            (false, i) => at(i),
        }
    }

    /// The point `t` (0 to 1) of the way along stretch `segment`, in the
    /// Obj's space, and the width there.
    pub fn point_at(&self, segment: usize, t: f32) -> (Vec3, f32) {
        let i = segment as isize;
        let ((p0, _), (p1, w1), (p2, w2), (p3, _)) = (self.point(i - 1), self.point(i), self.point(i + 1), self.point(i + 2));
        // Centripetal: each knot as far from the last as the square root of the distance between their points.
        let knot = |a: Vec3, b: Vec3| a.distance(b).sqrt().max(1e-4);
        let (t0, t1) = (0.0, knot(p0, p1));
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        let u = t1 + (t2 - t1) * t;
        let mix = |a: Vec3, b: Vec3, ta: f32, tb: f32| a * ((tb - u) / (tb - ta)) + b * ((u - ta) / (tb - ta));
        let (a1, a2, a3) = (mix(p0, p1, t0, t1), mix(p1, p2, t1, t2), mix(p2, p3, t2, t3));
        let (b1, b2) = (mix(a1, a2, t0, t2), mix(a2, a3, t1, t3));
        let eased = t * t * (3.0 - 2.0 * t);
        (mix(b1, b2, t1, t2), w1 + (w2 - w1) * eased)
    }

    /// Places along the whole Path about `spacing` metres apart, each
    /// stretch in as many even steps as fit, through `to_world` (the Obj's
    /// matrix): the first point first, and the last, or the first again for
    /// a closed Path. Widths are in metres, unscaled.
    pub fn samples(&self, to_world: Mat4, spacing: f32) -> Vec<PathSample> {
        let spacing = spacing.max(0.01);
        let mut points: Vec<(Vec3, f32)> = Vec::new();
        for segment in 0..self.segments() {
            // Its length from a few chords, for how many steps it takes.
            let chords: Vec<Vec3> = (0..=8).map(|i| to_world.transform_point3(self.point_at(segment, i as f32 / 8.0).0)).collect();
            let length: f32 = chords.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
            let steps = (length / spacing).ceil().max(1.0) as usize;
            let first = if segment == 0 { 0 } else { 1 };
            for step in first..=steps {
                let (point, width) = self.point_at(segment, step as f32 / steps as f32);
                points.push((to_world.transform_point3(point), width));
            }
        }
        let mut along = 0.0;
        (0..points.len())
            .map(|i| {
                let (position, width) = points[i];
                if i > 0 {
                    along += position.distance(points[i - 1].0);
                }
                let (before, after) = (points[i.saturating_sub(1)].0, points[(i + 1).min(points.len() - 1)].0);
                let tangent = (after - before).normalize_or(Vec3::Z);
                PathSample { position, tangent, width, along }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight() -> Path {
        let point = |z: f32, width: f32| PathPoint { position: [0.0, 0.0, z], width };
        Path { points: vec![point(0.0, 2.0), point(10.0, 4.0), point(20.0, 2.0)], closed: false }
    }

    #[test]
    fn the_curve_passes_through_every_point_with_its_width() {
        let path = Path::default();
        for (segment, point) in path.points.iter().take(2).enumerate() {
            let (at, width) = path.point_at(segment, 0.0);
            assert!(at.distance(Vec3::from(point.position)) < 1e-4, "point {segment}: {at}");
            assert_eq!(width, point.width);
        }
        let (end, _) = path.point_at(1, 1.0);
        assert!(end.distance(Vec3::from(path.points[2].position)) < 1e-4);
    }

    #[test]
    fn samples_run_evenly_from_end_to_end_measured_along_it() {
        let samples = straight().samples(Mat4::IDENTITY, 1.0);
        assert_eq!(samples.len(), 21, "a metre apart over twenty");
        assert!(samples[0].position.distance(Vec3::ZERO) < 1e-4);
        assert!((samples[20].along - 20.0).abs() < 1e-3, "{}", samples[20].along);
        assert!(samples.iter().all(|s| s.tangent.distance(Vec3::Z) < 1e-3), "along +Z all the way");
        assert!((samples[10].width - 4.0).abs() < 1e-4 && (samples[5].width - 3.0).abs() < 1e-4, "widths eased between points");
    }

    #[test]
    fn a_closed_path_comes_back_to_its_start_and_moves_with_its_obj() {
        let point = |x: f32, z: f32| PathPoint { position: [x, 0.0, z], width: 1.0 };
        let ring = Path { points: vec![point(0.0, 0.0), point(10.0, 0.0), point(10.0, 10.0), point(0.0, 10.0)], closed: true };
        assert_eq!(ring.segments(), 4);
        let moved = Mat4::from_translation(Vec3::new(100.0, 5.0, 0.0));
        let samples = ring.samples(moved, 2.0);
        assert!(samples[0].position.distance(Vec3::new(100.0, 5.0, 0.0)) < 1e-4);
        assert!(samples.last().unwrap().position.distance(samples[0].position) < 1e-4, "round to its start");
    }
}
