//! Boxes things are made of, to keep a hand or a finger out of them: each
//! a box with its place, turn and scale, and its corners in its own space.
//! A palm is put on the surface a line comes out at, a point is pushed out
//! of the box it is deepest in, and a finger closes until it is near one.

use voxear_core::math::{Mat4, Vec3};

/// A box somewhere: its place, turn and scale, and its corners in its own
/// space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub frame: Mat4,
    pub least: Vec3,
    pub most: Vec3,
}

impl Shape {
    /// How far `point` is outside the box, 0 inside it.
    pub fn distance(&self, point: Vec3) -> f32 {
        let local = self.frame.inverse().transform_point3(point);
        let nearest = local.clamp(self.least, self.most);
        self.frame.transform_vector3(local - nearest).length()
    }

    /// Where a line from `from` along `way`, a metre long, leaves the box,
    /// as a distance along it; none when `from` is not in it.
    pub fn exit(&self, from: Vec3, way: Vec3) -> Option<f32> {
        let into = self.frame.inverse();
        let (start, step) = (into.transform_point3(from), into.transform_vector3(way));
        if start.cmplt(self.least - 1e-5).any() || start.cmpgt(self.most + 1e-5).any() {
            return None;
        }
        let mut out = f32::INFINITY;
        for axis in 0..3 {
            if step[axis] > 1e-7 {
                out = out.min((self.most[axis] - start[axis]) / step[axis]);
            } else if step[axis] < -1e-7 {
                out = out.min((self.least[axis] - start[axis]) / step[axis]);
            }
        }
        // The same share of the way in either space, and `way` is a metre long.
        out.is_finite().then_some(out)
    }

    /// How far and which way `point` must move to be `room` out of the
    /// box: from inside, out through its nearest face but those facing
    /// against `side`; from outside, straight away from it; nothing when it
    /// is that far out already.
    pub fn push(&self, point: Vec3, room: f32, side: Vec3) -> Vec3 {
        let into = self.frame.inverse();
        let local = into.transform_point3(point);
        let nearest = local.clamp(self.least, self.most);
        let outside = self.frame.transform_vector3(local - nearest);
        if outside.length() > 1e-6 {
            return outside.normalize() * (room - outside.length()).max(0.0);
        }
        // Inside: out through the face nearest in metres.
        let faces = (0..6).map(|face| {
            let (axis, most) = (face / 2, face % 2 == 1);
            let mut normal = Vec3::ZERO;
            normal[axis] = if most { 1.0 } else { -1.0 };
            let depth = if most { self.most[axis] - local[axis] } else { local[axis] - self.least[axis] };
            let way = self.frame.transform_vector3(normal);
            (way.normalize_or_zero(), depth * way.length())
        });
        let faces = faces.filter(|(way, _)| way.dot(side) > -0.3);
        let (way, depth) = faces.min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((side, 0.0));
        way * (depth + room)
    }

    /// The box carried by `to`.
    pub fn moved(&self, to: Mat4) -> Shape {
        Shape { frame: to * self.frame, ..*self }
    }
}

/// Where a line from `from` along `way` comes out of the boxes it starts
/// in; `from` itself when it starts in none.
pub fn surface(shapes: &[Shape], from: Vec3, way: Vec3) -> Vec3 {
    let way = way.normalize_or_zero();
    let out = shapes.iter().filter_map(|shape| shape.exit(from, way)).fold(0.0f32, f32::max);
    from + way * out
}

/// The move that takes `point` `room` clear of every box, pushing it out
/// of the one it is deepest in, never out of the far side from `side`: a
/// hand moving through what it holds stays on its own side of it, rather
/// than coming out of whichever side is nearer as it passes the middle.
pub fn push_out(shapes: &[Shape], point: Vec3, room: f32, side: Vec3) -> Vec3 {
    shapes
        .iter()
        .map(|shape| shape.push(point, room, side))
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap_or(Vec3::ZERO)
}

/// How far `point` is from the nearest box.
pub fn nearest(shapes: &[Shape], point: Vec3) -> f32 {
    shapes.iter().map(|shape| shape.distance(point)).fold(f32::INFINITY, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> Shape {
        // A box 4.5 cm wide and 12 tall, its middle at the origin: a unit cube scaled.
        let frame = Mat4::from_scale(Vec3::new(0.045, 0.12, 0.36));
        Shape { frame, least: Vec3::splat(-0.5), most: Vec3::splat(0.5) }
    }

    #[test]
    fn a_line_out_of_a_box_meets_its_side_and_distances_are_in_metres() {
        let shapes = [stock()];
        let side = surface(&shapes, Vec3::ZERO, Vec3::X);
        assert!((side.x - 0.0225).abs() < 1e-5 && side.y.abs() < 1e-6, "{side}");
        let below = surface(&shapes, Vec3::new(0.0, 0.01, 0.0), Vec3::NEG_Y);
        assert!((below.y + 0.06).abs() < 1e-5, "{below}");
        assert_eq!(surface(&shapes, Vec3::new(0.1, 0.0, 0.0), Vec3::X), Vec3::new(0.1, 0.0, 0.0), "outside: as it was");
        assert!((nearest(&shapes, Vec3::new(0.0425, 0.0, 0.0)) - 0.02).abs() < 1e-5);
        assert_eq!(nearest(&shapes, Vec3::ZERO), 0.0);
        // Inside, out through the nearest side; outside but too near, out to the room asked for.
        let pushed = push_out(&shapes, Vec3::new(0.015, 0.0, 0.0), 0.01, Vec3::X);
        assert!((pushed - Vec3::new(0.0175, 0.0, 0.0)).length() < 1e-5, "{pushed}");
        let pushed = push_out(&shapes, Vec3::new(0.0, 0.065, 0.0), 0.01, Vec3::X);
        assert!((pushed - Vec3::new(0.0, 0.005, 0.0)).length() < 1e-5, "{pushed}");
        assert_eq!(push_out(&shapes, Vec3::new(0.0, 0.2, 0.0), 0.01, Vec3::X), Vec3::ZERO);
        // Just past the middle toward the left, a hand from the right still goes out on the right.
        let pushed = push_out(&shapes, Vec3::new(-0.002, 0.0, 0.0), 0.01, Vec3::X);
        assert!(pushed.x > 0.02, "{pushed}");
    }
}
