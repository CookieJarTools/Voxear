//! Parent/child transforms. `Transform` is always local; `WorldTransform` is
//! derived from it once per frame, after LateUpdate, so everything that runs
//! later (rendering, physics sync) sees final positions.

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
/// missing parent is treated as the world root. Only a `WorldTransform` that
/// comes out different is written, so the chunks of Objs that stood still
/// are not marked changed.
pub fn propagate_transforms(world: &mut World) {
    let mut nodes = world.remove_resource::<Propagation>().unwrap_or_default();
    nodes.gather(world);
    nodes.resolve();
    for &(entity, matrix) in &nodes.moved {
        match world.get_mut::<WorldTransform>(entity) {
            Some(world_transform) => world_transform.0 = matrix,
            None => drop(world.insert(entity, WorldTransform(matrix))),
        }
    }
    world.insert_resource(nodes);
}

/// What one pass of [`propagate_transforms`] works on, kept in the world
/// between frames so its room is not asked for again: every Obj with a
/// `Transform`, by its entity's index.
#[derive(Default)]
struct Propagation {
    nodes: Vec<Node>,
    /// The indices in use this pass.
    used: Vec<u32>,
    /// Parents met on the way up from an Obj, resolved on the way down.
    path: Vec<u32>,
    /// The `WorldTransform`s that came out different, or that were missing.
    moved: Vec<(Entity, Mat4)>,
    pass: u32,
}

#[derive(Clone, Copy)]
struct Node {
    entity: Entity,
    local: Mat4,
    parent: Option<Entity>,
    /// Its `WorldTransform` before this pass, if it had one.
    before: Option<Mat4>,
    world: Mat4,
    /// The pass it was gathered in, and whether it is resolved yet, or on
    /// the path being resolved (which only a loop of parents meets).
    pass: u32,
    state: State,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Gathered,
    OnPath,
    Resolved,
}

impl Propagation {
    fn gather(&mut self, world: &World) {
        self.pass = self.pass.wrapping_add(1).max(1);
        self.used.clear();
        self.moved.clear();
        let pass = self.pass;
        let query = world.query_ref::<(Entity, &Transform, Option<&Parent>, Option<&WorldTransform>)>();
        for (entity, transform, parent, before) in query {
            let index = entity.index() as usize;
            if index >= self.nodes.len() {
                let empty = Node {
                    entity: Entity::NULL,
                    local: Mat4::IDENTITY,
                    parent: None,
                    before: None,
                    world: Mat4::IDENTITY,
                    pass: 0,
                    state: State::Gathered,
                };
                self.nodes.resize(index + 1, empty);
            }
            self.nodes[index] = Node {
                entity,
                local: Mat4::from_scale_rotation_translation(transform.scale, transform.rotation, transform.position),
                parent: parent.map(|p| p.0),
                before: before.map(|w| w.0),
                world: Mat4::IDENTITY,
                pass,
                state: State::Gathered,
            };
            self.used.push(index as u32);
        }
    }

    /// The node of a live Obj gathered this pass.
    fn node_of(&self, entity: Entity) -> Option<u32> {
        let index = entity.index();
        let node = self.nodes.get(index as usize)?;
        (node.pass == self.pass && node.entity == entity).then_some(index)
    }

    fn resolve(&mut self) {
        for i in 0..self.used.len() {
            let index = self.used[i];
            if self.nodes[index as usize].state == State::Resolved {
                continue;
            }
            // Up to the first parent resolved already, or a root, then down again.
            self.path.clear();
            let mut at = index;
            let mut above = Mat4::IDENTITY;
            loop {
                let node = &mut self.nodes[at as usize];
                node.state = State::OnPath;
                self.path.push(at);
                let Some(parent) = node.parent.and_then(|parent| self.node_of(parent)) else { break };
                let up = &self.nodes[parent as usize];
                match up.state {
                    State::Resolved => {
                        above = up.world;
                        break;
                    }
                    // A loop of parents, or one too deep to be real: the rest of it is taken as a root.
                    State::OnPath => break,
                    State::Gathered if self.path.len() >= MAX_DEPTH => break,
                    State::Gathered => at = parent,
                }
            }
            for &at in self.path.iter().rev() {
                let node = &mut self.nodes[at as usize];
                above *= node.local;
                node.world = above;
                node.state = State::Resolved;
                if node.before != Some(above) {
                    self.moved.push((node.entity, above));
                }
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

    #[test]
    fn still_objs_are_not_written_again_and_a_moved_parent_carries_its_children() {
        let mut world = World::new();
        let root = world.spawn(Transform::from_position(Vec3::new(1.0, 0.0, 0.0)));
        let child = world.spawn((Transform::from_position(Vec3::new(0.0, 1.0, 0.0)), Parent(root)));
        let still = world.spawn(Transform::from_position(Vec3::new(5.0, 0.0, 0.0)));
        propagate_transforms(&mut world);
        let since = world.advance_tick();

        propagate_transforms(&mut world);
        let written = world.query_ref::<&WorldTransform>().changed::<WorldTransform>(since).count();
        assert_eq!(written, 0, "nothing moved, so no WorldTransform is written");

        world.get_mut::<Transform>(root).unwrap().position = Vec3::new(2.0, 0.0, 0.0);
        propagate_transforms(&mut world);
        let position = |e| world.get::<WorldTransform>(e).unwrap().0.w_axis.truncate();
        assert_eq!(position(root), Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(position(child), Vec3::new(2.0, 1.0, 0.0));
        assert_eq!(position(still), Vec3::new(5.0, 0.0, 0.0));
    }

    #[test]
    fn a_reused_entity_index_and_a_parent_spawned_after_its_child_are_followed() {
        let mut world = World::new();
        let gone = world.spawn(Transform::from_position(Vec3::X));
        propagate_transforms(&mut world);
        world.despawn(gone);
        // The child is made first, so its parent comes later in the query.
        let child = world.spawn(Transform::from_position(Vec3::Y));
        let parent = world.spawn(Transform::from_position(Vec3::Z));
        world.insert(child, Parent(parent));
        propagate_transforms(&mut world);
        assert_eq!(world.get::<WorldTransform>(child).unwrap().0.w_axis.truncate(), Vec3::new(0.0, 1.0, 1.0));
    }
}
