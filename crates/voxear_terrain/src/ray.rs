//! Rays against the ground: where a ray first meets a terrain's triangles.
//!
//! The ray is clipped to the terrain's box, then walked quad by quad along
//! x and z. A quad whose corners all lie below the ray's lowest point
//! across it, or all above its highest, is passed over; the rest have their
//! two triangles tested, but for those a hole cuts away.

use voxear_core::math::Vec3;

use crate::heights::Heightmap;

/// Where a ray met the ground, in the terrain's own space (metres from its
/// corner), how far along the ray, and which way the triangle there faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    pub point: Vec3,
    pub distance: f32,
    pub normal: Vec3,
}

/// The first place a ray from `origin` along `direction` (in the terrain's
/// space) meets the ground within `max_distance`, from above or below,
/// passing through holes.
pub fn raycast(map: &Heightmap, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<GroundHit> {
    cast(map, origin, direction, max_distance, true)
}

/// As [`raycast`], but meeting the ground across holes as if they were not
/// cut: where a brush that fills them is.
pub fn raycast_whole(map: &Heightmap, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<GroundHit> {
    cast(map, origin, direction, max_distance, false)
}

fn cast(map: &Heightmap, origin: Vec3, direction: Vec3, max_distance: f32, holes: bool) -> Option<GroundHit> {
    let direction = direction.try_normalize()?;
    let [w, l] = map.size();
    let [low, high] = map.range();
    let (start, end) = clip(origin, direction, Vec3::new(0.0, low, 0.0), Vec3::new(w, high, l), max_distance)?;
    let spacing = map.spacing();
    let [qx, qz] = map.quads();
    let at = origin + direction * start;
    let mut cell = [
        ((at.x / spacing).floor() as i64).clamp(0, i64::from(qx) - 1),
        ((at.z / spacing).floor() as i64).clamp(0, i64::from(qz) - 1),
    ];
    let step = [direction.x.signum() as i64, direction.z.signum() as i64];
    // How far along the ray to the next line between quads each way, and how far apart those lines are.
    let next_line = |axis: usize, place: i64| {
        let (component, from) = if axis == 0 { (direction.x, origin.x) } else { (direction.z, origin.z) };
        if component == 0.0 {
            return f32::MAX;
        }
        let line = if component > 0.0 { (place + 1) as f32 } else { place as f32 } * spacing;
        (line - from) / component
    };
    let mut crossing = [next_line(0, cell[0]), next_line(1, cell[1])];
    let gap = [
        if direction.x == 0.0 { f32::MAX } else { spacing / direction.x.abs() },
        if direction.z == 0.0 { f32::MAX } else { spacing / direction.z.abs() },
    ];
    let mut near = start;
    loop {
        let far = crossing[0].min(crossing[1]).min(end);
        if let Some(hit) = quad_hit(map, cell, origin, direction, (near, far), holes) {
            return Some(hit);
        }
        if far >= end {
            return None;
        }
        let axis = if crossing[0] < crossing[1] { 0 } else { 1 };
        cell[axis] += step[axis];
        crossing[axis] += gap[axis];
        near = far;
        if cell[0] < 0 || cell[1] < 0 || cell[0] >= i64::from(qx) || cell[1] >= i64::from(qz) {
            return None;
        }
    }
}

/// The two triangles of quad `cell`, tested if the ray's height across it
/// (from `span.0` to `span.1` along it) overlaps the quad's.
fn quad_hit(
    map: &Heightmap,
    [x, z]: [i64; 2],
    origin: Vec3,
    direction: Vec3,
    span: (f32, f32),
    holes: bool,
) -> Option<GroundHit> {
    let corners = [map.sample(x, z), map.sample(x + 1, z), map.sample(x, z + 1), map.sample(x + 1, z + 1)];
    let low = corners.iter().copied().fold(f32::MAX, f32::min);
    let high = corners.iter().copied().fold(f32::MIN, f32::max);
    let [y0, y1] = [origin.y + direction.y * span.0, origin.y + direction.y * span.1];
    if y0.min(y1) > high + 1e-4 || y0.max(y1) < low - 1e-4 {
        return None;
    }
    let s = map.spacing();
    let point = |dx: i64, dz: i64, h: f32| Vec3::new((x + dx) as f32 * s, h, (z + dz) as f32 * s);
    let a = point(0, 0, corners[0]);
    let b = point(1, 0, corners[1]);
    let c = point(0, 1, corners[2]);
    let d = point(1, 1, corners[3]);
    // Split from the low corner to the high, as the ground is (see `heights`); a triangle with a corner on a hole
    // is cut away.
    let hole = |dx: i64, dz: i64| holes && map.is_hole(x + dx, z + dz);
    if hole(0, 0) || hole(1, 1) {
        return None;
    }
    let hits = [
        (!hole(0, 1)).then(|| triangle(origin, direction, a, c, d)).flatten(),
        (!hole(1, 0)).then(|| triangle(origin, direction, a, d, b)).flatten(),
    ];
    let (distance, normal) = hits.into_iter().flatten().min_by(|p, q| p.0.total_cmp(&q.0))?;
    Some(GroundHit { point: origin + direction * distance, distance, normal })
}

/// Where a ray meets triangle `a b c` from either side: how far along it,
/// and the triangle's upward normal.
fn triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let det = ab.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let t_vec = origin - a;
    let u = t_vec.dot(p) / det;
    if !(-1e-6..=1.0 + 1e-6).contains(&u) {
        return None;
    }
    let q = t_vec.cross(ab);
    let v = direction.dot(q) / det;
    if v < -1e-6 || u + v > 1.0 + 1e-6 {
        return None;
    }
    let distance = ac.dot(q) / det;
    let normal = ab.cross(ac).normalize();
    (distance >= 0.0).then_some((distance, if normal.y < 0.0 { -normal } else { normal }))
}

/// The stretch of a ray within a box, `max_distance` at most.
fn clip(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3, max_distance: f32) -> Option<(f32, f32)> {
    let mut near = 0.0f32;
    let mut far = max_distance;
    for axis in 0..3 {
        let (o, d, lo, hi) = (origin[axis], direction[axis], min[axis], max[axis]);
        if d.abs() < 1e-12 {
            if o < lo - 1e-4 || o > hi + 1e-4 {
                return None;
            }
            continue;
        }
        let (mut t0, mut t1) = ((lo - o) / d, (hi - o) / d);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        near = near.max(t0 - 1e-4);
        far = far.min(t1 + 1e-4);
        if near > far {
            return None;
        }
    }
    Some((near.max(0.0), far))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ray_straight_down_lands_on_the_ground_under_it() {
        let map = Heightmap::from_fn(2, 2, 0.5, |x, z| (x * 0.05).sin() * 4.0 + z * 0.02);
        let hit = raycast(&map, Vec3::new(123.4, 50.0, 200.1), Vec3::NEG_Y, 100.0).expect("the ground");
        let expected = map.height_at(123.4, 200.1);
        assert!((hit.point.y - expected).abs() < 1e-3, "{} and {expected}", hit.point.y);
        assert!((hit.distance - (50.0 - expected)).abs() < 1e-3);
        assert!(hit.normal.distance(map.face_normal_at(123.4, 200.1)) < 1e-3);
    }

    #[test]
    fn a_ray_along_a_valley_hits_the_hill_ahead() {
        // Flat, then a wall of ground rising at x = 100.
        let map = Heightmap::from_fn(1, 1, 1.0, |x, _| if x >= 100.0 { 20.0 } else { 0.0 });
        let hit = raycast(&map, Vec3::new(10.0, 5.0, 50.0), Vec3::X, 500.0).expect("the hill");
        assert!((hit.point.x - 99.25).abs() < 0.01, "{}", hit.point.x);
        assert!(hit.normal.x < -0.5);
    }

    #[test]
    fn a_ray_that_misses_or_is_too_short_finds_nothing() {
        let map = Heightmap::flat(1, 1, 1.0, 0.0);
        assert_eq!(raycast(&map, Vec3::new(10.0, 5.0, 10.0), Vec3::Y, 100.0), None);
        assert_eq!(raycast(&map, Vec3::new(10.0, 5.0, 10.0), Vec3::NEG_Y, 4.0), None);
        assert_eq!(raycast(&map, Vec3::new(-10.0, 5.0, 10.0), Vec3::NEG_X, 100.0), None);
        // From outside, at a slant, onto it.
        let hit = raycast(&map, Vec3::new(-10.0, 10.0, 10.0), Vec3::new(1.0, -0.5, 0.0), 100.0).expect("the ground");
        assert!((hit.point.x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn a_ray_from_under_the_ground_meets_it_from_below() {
        let map = Heightmap::flat(1, 1, 1.0, 3.0);
        let hit = raycast(&map, Vec3::new(10.0, -5.0, 10.0), Vec3::Y, 100.0).expect("the ground from below");
        assert!((hit.distance - 8.0).abs() < 1e-3);
        assert!(hit.normal.y > 0.99);
    }
}
