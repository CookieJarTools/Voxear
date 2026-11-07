//! What a gun's surfaces are to the hands holding it: a box for each of its
//! parts' meshes, as the renderer bounds them, which moves with its part.
//! A palm is put on the surface under a socket, and fingers close until
//! they touch it, so neither goes into the gun.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{MeshRenderer, Renderer};
use voxear_scene::{Hidden, Parent, world_matrix};

use super::gun::Gun;
use super::mechanism::{GunPart, PartRole};

/// A gun's parts' meshes, each with its box in its own space: corners
/// least and most. Found once the renderer can bound them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GunShapes {
    pub pieces: Vec<(Entity, Vec3, Vec3)>,
}

impl Component for GunShapes {}

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
    fn exit(&self, from: Vec3, way: Vec3) -> Option<f32> {
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
    /// box: out through its nearest face from inside, straight away from
    /// it from outside; nothing when it is that far out already.
    fn push(&self, point: Vec3, room: f32) -> Vec3 {
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
        let (way, depth) = faces.min_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((Vec3::Y, 0.0));
        way * (depth + room)
    }

    /// The box carried by `to`.
    pub fn moved(&self, to: Mat4) -> Shape {
        Shape { frame: to * self.frame, ..*self }
    }
}

/// Bounds each gun's parts' meshes once there is a renderer to ask: all
/// but a spent case and a clip, which are only looks.
pub fn give_shapes(world: &mut World, renderer: Option<&Renderer>) {
    let Some(renderer) = renderer else { return };
    let guns: Vec<Entity> =
        world.query_ref::<(Entity, &Gun)>().without::<GunShapes>().into_iter().map(|(entity, _)| entity).collect();
    for gun in guns {
        let meshes: Vec<(Entity, MeshRenderer)> =
            world.query_ref::<(Entity, &MeshRenderer)>().into_iter().map(|(e, m)| (e, *m)).collect();
        let pieces = meshes
            .into_iter()
            .filter(|(entity, _)| *entity != gun && under(world, *entity, gun))
            .filter(|(entity, _)| {
                !world.get::<GunPart>(*entity).is_some_and(|part| matches!(part.role, PartRole::Case | PartRole::Clip))
            })
            .filter_map(|(entity, mesh)| renderer.mesh_bounds(mesh.mesh).map(|(least, most)| (entity, least, most)))
            .collect();
        world.insert(gun, GunShapes { pieces });
    }
}

/// A gun's boxes where they are now, in the gun's space: its shown parts
/// but those `leaving` out, moved as they are.
pub fn shapes_now(world: &World, gun: Entity, leaving: &[Entity]) -> Vec<Shape> {
    let Some(shapes) = world.get::<GunShapes>(gun) else { return Vec::new() };
    let into_gun = world_matrix(world, gun).inverse();
    shapes
        .pieces
        .iter()
        .filter(|(entity, ..)| !world.has::<Hidden>(*entity) && !leaving.contains(entity))
        .map(|&(entity, least, most)| Shape { frame: into_gun * world_matrix(world, entity), least, most })
        .collect()
}

/// Where a line from `from` along `way` comes out of the boxes it starts
/// in; `from` itself when it starts in none.
pub fn surface(shapes: &[Shape], from: Vec3, way: Vec3) -> Vec3 {
    let way = way.normalize_or_zero();
    let out = shapes.iter().filter_map(|shape| shape.exit(from, way)).fold(0.0f32, f32::max);
    from + way * out
}

/// The move that takes `point` `room` clear of every box, pushing it out
/// of the one it is deepest in.
pub fn push_out(shapes: &[Shape], point: Vec3, room: f32) -> Vec3 {
    shapes
        .iter()
        .map(|shape| shape.push(point, room))
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap_or(Vec3::ZERO)
}

/// How far `point` is from the nearest box.
pub fn nearest(shapes: &[Shape], point: Vec3) -> f32 {
    shapes.iter().map(|shape| shape.distance(point)).fold(f32::INFINITY, f32::min)
}

fn under(world: &World, obj: Entity, owner: Entity) -> bool {
    let mut at = Some(obj);
    while let Some(here) = at {
        if here == owner {
            return true;
        }
        at = world.get::<Parent>(here).map(|parent| parent.0);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> Shape {
        // A stock 4.5 cm wide and 12 tall, its middle at the origin: a unit cube scaled.
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
        let pushed = push_out(&shapes, Vec3::new(0.015, 0.0, 0.0), 0.01);
        assert!((pushed - Vec3::new(0.0175, 0.0, 0.0)).length() < 1e-5, "{pushed}");
        let pushed = push_out(&shapes, Vec3::new(0.0, 0.065, 0.0), 0.01);
        assert!((pushed - Vec3::new(0.0, 0.005, 0.0)).length() < 1e-5, "{pushed}");
        assert_eq!(push_out(&shapes, Vec3::new(0.0, 0.2, 0.0), 0.01), Vec3::ZERO);
    }
}
