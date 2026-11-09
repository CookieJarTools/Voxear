//! Where an Obj is in the hierarchy, for scripts (`Obj.Parent` and
//! `Obj.Children` in Voxear.Core): the Obj it is inside, and the Objs
//! directly inside it, in the order the engine's own systems go through
//! them, by their entities' numbers.

use voxear_ecs::{Entity, World};
use voxear_scene::Parent;
use voxear_scripting::ComponentKind;

use crate::cameras::obj_value;

// What `values` reads, in the low byte of its number, as `ObjHierarchy` in
// Voxear.Core numbers them; which child is the number's higher bytes.
const PARENT: i32 = 0;
const CHILD_COUNT: i32 = 1;
const CHILD: i32 = 2;

pub(crate) fn register() {
    voxear_scripting::set_kind_values(ComponentKind::Hierarchy, values);
}

fn values(world: &mut World, entity: Entity, what: i32, written: Option<[f32; 4]>) -> Option<[f32; 4]> {
    if written.is_some() || !world.is_alive(entity) {
        return None;
    }
    match what & 0xff {
        PARENT => Some(obj_value(world.get::<Parent>(entity).map(|parent| parent.0))),
        CHILD_COUNT => Some([children(world, entity).len() as f32, 0.0, 0.0, 0.0]),
        CHILD => children(world, entity).get((what >> 8) as usize).map(|&child| obj_value(Some(child))),
        _ => None,
    }
}

/// The Objs whose parent is `entity`, by their entities' numbers.
pub(crate) fn children(world: &World, entity: Entity) -> Vec<Entity> {
    let mut children: Vec<Entity> = world
        .query_ref::<(Entity, &Parent)>()
        .into_iter()
        .filter(|(_, parent)| parent.0 == entity)
        .map(|(child, _)| child)
        .collect();
    children.sort_by_key(|child| child.index());
    children
}

#[cfg(test)]
mod tests {
    use voxear_scene::Transform;

    use super::*;
    use crate::cameras::value_obj;

    #[test]
    fn an_objs_parent_and_children_are_read_in_the_engines_order() {
        let mut world = World::new();
        let top = world.spawn((Transform::default(),));
        let first = world.spawn((Transform::default(), Parent(top)));
        let second = world.spawn((Transform::default(), Parent(top)));
        let inner = world.spawn((Transform::default(), Parent(second)));
        let parent = |world: &mut World, of| value_obj(values(world, of, PARENT, None).unwrap());
        assert_eq!(parent(&mut world, top), None, "the top is inside nothing");
        assert_eq!(parent(&mut world, inner), Some(second));
        assert_eq!(values(&mut world, top, CHILD_COUNT, None), Some([2.0, 0.0, 0.0, 0.0]), "not the one further in");
        let child = |world: &mut World, at: i32| values(world, top, at << 8 | CHILD, None).and_then(value_obj);
        assert_eq!((child(&mut world, 0), child(&mut world, 1)), (Some(first), Some(second)));
        assert_eq!(child(&mut world, 2), None, "past the last");
        assert_eq!(values(&mut world, top, PARENT, Some([0.0; 4])), None, "nothing is set through it");
    }
}
