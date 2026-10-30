//! Parent/child transforms. `Transform` is always local; `WorldTransform` is
//! derived from it once per frame, after LateUpdate, so everything that runs
//! later (rendering, physics sync) sees final positions.

use std::collections::HashMap;

use voxear_core::math::Mat4;
use voxear_ecs::{Component, Entity, World};

use crate::obj::Transform;

/// Makes this Obj's `Transform` relative to another Obj.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parent(pub Entity);
impl Component for Parent {}

/// Local-to-world matrix, written by [`propagate_transforms`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTransform(pub Mat4);
impl Component for WorldTransform {}

impl Default for WorldTransform {
    fn default() -> Self {
        Self(Mat4::IDENTITY)
    }
}

/// Recomputes every `WorldTransform` from local transforms and parents. A
/// missing parent is treated as the world root.
pub fn propagate_transforms(world: &mut World) {
    let mut locals: HashMap<Entity, (Mat4, Option<Entity>)> = HashMap::new();
    for (entity, transform, parent) in world.query_ref::<(Entity, &Transform, Option<&Parent>)>() {
        let local = Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position);
        locals.insert(entity, (local, parent.map(|p| p.0)));
    }

    let mut resolved: HashMap<Entity, Mat4> = HashMap::with_capacity(locals.len());
    let entities: Vec<Entity> = locals.keys().copied().collect();
    for entity in entities {
        resolve(entity, &locals, &mut resolved, 0);
    }

    for (entity, matrix) in resolved {
        match world.get_mut::<WorldTransform>(entity) {
            Some(world_transform) => world_transform.0 = matrix,
            None => {
                world.insert(entity, WorldTransform(matrix));
            }
        }
    }
}

/// An Obj's local-to-world matrix from its current transforms. Unlike
/// `WorldTransform`, which is worked out once per frame, it is up to date
/// mid-frame, for placing new Objs in world space.
pub fn world_matrix(world: &World, entity: Entity) -> Mat4 {
    let mut matrix = Mat4::IDENTITY;
    let mut current = Some(entity);
    for _ in 0..MAX_DEPTH {
        let Some(e) = current else { break };
        let Some(t) = world.get::<Transform>(e) else { break };
        matrix = Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.position) * matrix;
        current = world.get::<Parent>(e).map(|p| p.0);
    }
    matrix
}

/// Deep enough for any real hierarchy; deeper means a cycle.
const MAX_DEPTH: usize = 256;

fn resolve(
    entity: Entity,
    locals: &HashMap<Entity, (Mat4, Option<Entity>)>,
    resolved: &mut HashMap<Entity, Mat4>,
    depth: usize,
) -> Mat4 {
    if let Some(&matrix) = resolved.get(&entity) {
        return matrix;
    }
    let Some(&(local, parent)) = locals.get(&entity) else {
        return Mat4::IDENTITY;
    };
    let matrix = match parent {
        Some(parent) if depth < MAX_DEPTH && locals.contains_key(&parent) => {
            resolve(parent, locals, resolved, depth + 1) * local
        }
        _ => local,
    };
    resolved.insert(entity, matrix);
    matrix
}

#[cfg(test)]
mod tests {
    use voxear_core::math::{Quat, Vec3};

    use super::*;

    #[test]
    fn children_follow_their_parents() {
        let mut world = World::new();
        let root = world.spawn(Transform::new(
            Vec3::new(10.0, 0.0, 0.0),
            Quat::from_rotation_y(90f32.to_radians()),
            Vec3::ONE,
        ));
        let child = world.spawn((Transform::from_position(Vec3::new(0.0, 0.0, 1.0)), Parent(root)));
        let grandchild = world.spawn((Transform::from_position(Vec3::new(0.0, 2.0, 0.0)), Parent(child)));

        propagate_transforms(&mut world);

        let position = |e| world.get::<WorldTransform>(e).unwrap().0.w_axis.truncate();
        assert!((position(root) - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-5);
        // A quarter turn about Y takes local +Z to world +X.
        assert!((position(child) - Vec3::new(11.0, 0.0, 0.0)).length() < 1e-5);
        assert!((position(grandchild) - Vec3::new(11.0, 2.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn world_matrix_is_current_before_propagation() {
        let mut world = World::new();
        let root = world.spawn(Transform::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY, Vec3::splat(2.0)));
        let child = world.spawn((Transform::from_position(Vec3::new(0.0, 1.0, 0.0)), Parent(root)));
        assert!((world_matrix(&world, child).w_axis.truncate() - Vec3::new(10.0, 2.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn cycles_and_dead_parents_do_not_hang() {
        let mut world = World::new();
        let a = world.spawn(Transform::IDENTITY);
        let b = world.spawn((Transform::IDENTITY, Parent(a)));
        world.insert(a, Parent(b));
        let dead = world.spawn(Transform::IDENTITY);
        world.despawn(dead);
        let orphan = world.spawn((Transform::from_position(Vec3::X), Parent(dead)));

        propagate_transforms(&mut world);
        assert!(world.has::<WorldTransform>(a) && world.has::<WorldTransform>(b));
        assert_eq!(world.get::<WorldTransform>(orphan).unwrap().0.w_axis.truncate(), Vec3::X);
    }
}
