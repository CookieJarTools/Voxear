//! Undo and redo for edits made in the editor. Each edit is an Obj's name
//! and Transform before and after; typing into one field records one step,
//! not one per key, as edits to the same Obj close together merge.

use std::time::{Duration, Instant};

use voxear_ecs::{Entity, World};
use voxear_scene::{Name, Transform};

/// Edits to the same Obj closer together than this are one step.
const MERGE_WITHIN: Duration = Duration::from_millis(800);

/// What an edit changes about an Obj.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjState {
    pub name: String,
    pub transform: Transform,
}

impl ObjState {
    pub fn of(world: &World, entity: Entity) -> Option<Self> {
        let name = world.get::<Name>(entity)?.0.clone();
        let transform = *world.get::<Transform>(entity)?;
        Some(Self { name, transform })
    }

    fn apply(&self, world: &mut World, entity: Entity) {
        if let Some(name) = world.get_mut::<Name>(entity) {
            name.0.clone_from(&self.name);
        }
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            *transform = self.transform;
        }
    }
}

/// One step: an Obj as it was, and as the edit left it.
#[derive(Clone, Debug)]
pub struct Edit {
    pub entity: Entity,
    pub before: ObjState,
    pub after: ObjState,
}

#[derive(Default)]
pub struct History {
    done: Vec<(Edit, Instant)>,
    undone: Vec<Edit>,
}

impl History {
    /// Notes an edit already made. One to the same Obj soon after the last
    /// becomes part of it; a new edit means nothing is left to redo.
    pub fn record(&mut self, edit: Edit) {
        self.undone.clear();
        let now = Instant::now();
        if let Some((last, at)) = self.done.last_mut()
            && last.entity == edit.entity
            && now.duration_since(*at) < MERGE_WITHIN
        {
            last.after = edit.after;
            *at = now;
            return;
        }
        self.done.push((edit, now));
    }

    /// Takes back the last edit, and returns the Obj it was on.
    pub fn undo(&mut self, world: &mut World) -> Option<Entity> {
        let (edit, _) = self.done.pop()?;
        edit.before.apply(world, edit.entity);
        let entity = edit.entity;
        self.undone.push(edit);
        Some(entity)
    }

    /// Makes the last undone edit again, and returns the Obj it is on.
    pub fn redo(&mut self, world: &mut World) -> Option<Entity> {
        let edit = self.undone.pop()?;
        edit.after.apply(world, edit.entity);
        let entity = edit.entity;
        // Not merged with what came before, so it undoes on its own again.
        self.done.push((edit, Instant::now() - MERGE_WITHIN));
        Some(entity)
    }

    /// Forgets everything, as when another level opens and the Objs go.
    pub fn clear(&mut self) {
        self.done.clear();
        self.undone.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxear_core::math::Vec3;

    fn moved(world: &mut World, entity: Entity, x: f32, history: &mut History) {
        let before = ObjState::of(world, entity).unwrap();
        world.get_mut::<Transform>(entity).unwrap().position.x = x;
        let after = ObjState::of(world, entity).unwrap();
        history.record(Edit { entity, before, after });
    }

    #[test]
    fn edits_undo_redo_and_merge_while_typing() {
        let mut world = World::new();
        let entity = world.spawn((Name("Crate".into()), Transform::IDENTITY));
        let x = |world: &World| world.get::<Transform>(entity).unwrap().position.x;
        let mut history = History::default();
        moved(&mut world, entity, 1.0, &mut history);
        moved(&mut world, entity, 12.0, &mut history);
        assert_eq!(history.undo(&mut world), Some(entity));
        assert_eq!(x(&world), 0.0, "typing 1 then 12 is one step");
        assert_eq!(history.redo(&mut world), Some(entity));
        assert_eq!(x(&world), 12.0);
        assert_eq!(history.undo(&mut world), Some(entity));
        assert!(history.undo(&mut world).is_none(), "nothing more to undo");
        world.get_mut::<Transform>(entity).unwrap().scale = Vec3::splat(2.0);
        moved(&mut world, entity, 3.0, &mut history);
        assert!(history.redo(&mut world).is_none(), "a new edit leaves nothing to redo");
    }
}
