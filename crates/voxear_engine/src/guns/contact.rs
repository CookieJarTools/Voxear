//! What a gun's surfaces are to the hands holding it: a box for each of its
//! parts' meshes, as the renderer bounds them, which moves with its part,
//! and for a gun drawn by a skinned model, a box for each of its bones
//! round the vertices the bone moves, which moves as the bone is posed.
//! A palm is put on the surface under a socket, and fingers close until
//! they touch it, so neither goes into the gun.

use voxear_core::math::{Mat4, Vec3};
use voxear_ecs::{Component, Entity, World};
use voxear_render::{MeshRenderer, ModelRenderer, Pose, Renderer};
use voxear_scene::{Hidden, Parent, world_matrix};

use super::gun::Gun;
use super::mechanism::{GunPart, PartRole};

/// A gun's parts' meshes, each with its box in its own space: corners
/// least and most. Found once the renderer can bound them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GunShapes {
    pub pieces: Vec<(Entity, Vec3, Vec3)>,
    /// A skinned model's bones: the Obj drawing it, the bone, and the box
    /// round what the bone moves, in the bone's space.
    pub bones: Vec<(Entity, usize, Vec3, Vec3)>,
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
    /// box: from inside, out through its nearest face but those facing
    /// against `side`; from outside, straight away from it; nothing when it
    /// is that far out already.
    fn push(&self, point: Vec3, room: f32, side: Vec3) -> Vec3 {
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
        world.insert(gun, GunShapes { pieces, bones: bone_boxes(world, gun) });
    }
}

/// The boxes of the bones of each skinned model on or under `gun`.
fn bone_boxes(world: &World, gun: Entity) -> Vec<(Entity, usize, Vec3, Vec3)> {
    let mut boxes = Vec::new();
    for (entity, model) in world.query_ref::<(Entity, &ModelRenderer)>().into_iter() {
        let Some(animation) = model.animation.as_ref().filter(|_| under(world, entity, gun)) else { continue };
        let skins = model.parts.iter().filter_map(|part| part.skin).collect::<std::collections::BTreeSet<_>>();
        for skin in skins.into_iter().filter_map(|skin| animation.skins.get(skin as usize)) {
            let joints = skin.joints.iter().zip(&skin.bounds).filter(|(_, (least, most))| least.cmple(*most).all());
            boxes.extend(joints.map(|(&bone, &(least, most))| (entity, bone, least, most)));
        }
    }
    boxes
}

/// A gun's boxes where they are now, in the gun's space: its shown parts
/// but those `leaving` out, moved as they are, and its bones but the
/// `leaving_bones`, as they are posed.
pub fn shapes_now(world: &World, gun: Entity, leaving: &[Entity], leaving_bones: &[usize]) -> Vec<Shape> {
    let Some(shapes) = world.get::<GunShapes>(gun) else { return Vec::new() };
    let into_gun = world_matrix(world, gun).inverse();
    let parts = shapes
        .pieces
        .iter()
        .filter(|(entity, ..)| !world.has::<Hidden>(*entity) && !leaving.contains(entity))
        .map(|&(entity, least, most)| Shape { frame: into_gun * world_matrix(world, entity), least, most });
    let bones = shapes.bones.iter().filter(|(_, bone, ..)| !leaving_bones.contains(bone)).filter_map(
        |&(entity, bone, least, most)| {
            let placed = posed(world, entity, bone)?;
            Some(Shape { frame: into_gun * world_matrix(world, entity) * placed, least, most })
        },
    );
    parts.chain(bones).collect()
}

/// Where `bone` of the model `entity` draws is, in the model's space: as
/// its Pose has it, or at rest without one.
fn posed(world: &World, entity: Entity, bone: usize) -> Option<Mat4> {
    if let Some(pose) = world.get::<Pose>(entity) {
        return pose.bones.get(bone).copied();
    }
    let skeleton = &world.get::<ModelRenderer>(entity)?.animation.as_ref()?.skeleton;
    skeleton.model_matrices(&skeleton.rest()).get(bone).copied()
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
/// hand moving through the gun stays on its own side of it, rather than
/// coming out of whichever side is nearer as it passes the middle.
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
    use std::sync::Arc;

    use voxear_core::math::Quat;
    use voxear_render::{Bone, BonePose, MaterialHandle, MeshHandle, ModelAnimation, ModelPart, Skeleton, Skin};
    use voxear_scene::Transform;

    use super::*;

    /// A gun drawn by one skinned model, its bolt a bone whose box moves as
    /// the bone is posed, and stays out when the bolt is in hand.
    #[test]
    fn a_skinned_guns_bones_are_boxed_where_they_are_posed() {
        let rest = |translation| BonePose { translation, ..BonePose::default() };
        let skeleton = Skeleton {
            bones: vec![
                Bone { name: "Root".into(), parent: None, rest: rest(Vec3::ZERO) },
                Bone { name: "bolt".into(), parent: Some(0), rest: rest(Vec3::new(0.0, 0.045, 0.04)) },
            ],
        };
        let binds = skeleton.model_matrices(&skeleton.rest()).iter().map(|bone| bone.inverse()).collect();
        let bounds = vec![(Vec3::splat(-0.02), Vec3::splat(0.02)), (Vec3::splat(-0.01), Vec3::splat(0.01))];
        let skin = Skin { joints: vec![0, 1], inverse_binds: binds, bounds };
        let animation = Arc::new(ModelAnimation { skeleton, skins: vec![skin], clips: Vec::new() });
        let part = ModelPart {
            mesh: MeshHandle(0),
            material: MaterialHandle(0),
            transform: Mat4::IDENTITY,
            node: 0,
            skin: Some(0),
        };
        let model = ModelRenderer { animation: Some(animation.clone()), ..ModelRenderer::new(vec![part]) };
        let mut world = World::new();
        let gun = world.spawn((Transform::default(), Gun::default(), model));
        world.insert(gun, GunShapes { pieces: Vec::new(), bones: bone_boxes(&world, gun) });
        assert_eq!(world.get::<GunShapes>(gun).unwrap().bones.len(), 2);

        let at_rest = shapes_now(&world, gun, &[], &[]);
        assert_eq!(nearest(&at_rest, Vec3::new(0.0, 0.045, 0.04)), 0.0, "the bolt's box is round the bolt at rest");
        // The bolt pulled back 8 cm and turned up.
        let mut pose = animation.skeleton.rest();
        pose[1] = BonePose { rotation: Quat::from_rotation_z(1.4), ..rest(Vec3::new(0.0, 0.045, -0.04)) };
        world.insert(gun, Pose { bones: animation.skeleton.model_matrices(&pose).into() });
        let pulled = shapes_now(&world, gun, &[], &[]);
        assert_eq!(nearest(&pulled, Vec3::new(0.0, 0.045, -0.04)), 0.0, "it went back with the bolt");
        assert!(nearest(&pulled, Vec3::new(0.0, 0.045, 0.04)) > 0.005, "and left where it was");
        assert_eq!(shapes_now(&world, gun, &[], &[1]).len(), 1, "a bolt in hand is left out");
    }

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
