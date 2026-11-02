//! Prefab overrides in the editor, as Unity's: the Inspector marks the
//! values an Obj of a prefab instance changes from its prefab, a row's or a
//! component's menu reverts them or applies them to the prefab, and the
//! instance root's Overrides button does either for the whole instance.
//!
//! Reverting is a step to undo. Applying writes the prefab's file, which
//! undo does not take back, and the other instances in the open scenes take
//! the values they had not changed themselves.

use std::collections::HashMap;

use voxear_ecs::Entity;
use voxear_engine::prefab_overrides::{self, ValuePath};
use voxear_render::Renderer;

use super::Editor;
use crate::history::{Edit, ObjState};

impl Editor {
    /// Marks what the Obj in the Inspector changes from its prefab, worked
    /// out again whenever the Obj changes.
    pub(super) fn update_overrides(&mut self) {
        let shown = self.inspector.shown().filter(|_| !self.is_playing() && self.prefab.is_none());
        let seen = shown.and_then(|entity| Some((entity, ObjState::of(&self.game.engine.world, entity)?)));
        if seen == self.overrides_seen {
            return;
        }
        let game = &mut self.game;
        let found = shown.map(|entity| prefab_overrides::overrides(&game.engine.world, &mut game.content, entity));
        self.overrides_seen = seen;
        self.inspector.show_overrides(&mut self.ui, found.unwrap_or_default());
    }

    /// Puts the prefab's values at `paths` back on an Obj. The caller
    /// records the step.
    pub(super) fn revert_overrides(&mut self, entity: Entity, paths: &[ValuePath], renderer: &mut Renderer) {
        let game = &mut self.game;
        let reverted = prefab_overrides::revert(&mut game.engine.world, &mut game.content, Some(renderer), entity, paths);
        if let Err(error) = reverted {
            self.notify(&format!("Could not revert: {error:#}"));
        }
    }

    /// Puts back everything the instance `entity` is part of changes from
    /// its prefab, as one step to undo.
    pub(super) fn revert_instance(&mut self, entity: Entity, renderer: &mut Renderer) {
        let game = &mut self.game;
        let Some(member) = prefab_overrides::member(&game.engine.world, entity) else { return };
        let changes = prefab_overrides::instance_overrides(&game.engine.world, &mut game.content, member.root);
        if changes.is_empty() {
            return self.notify("Nothing differs from the prefab");
        }
        let mut edits = Vec::new();
        for (obj, paths) in changes {
            let Some(before) = ObjState::of(&self.game.engine.world, obj) else { continue };
            self.revert_overrides(obj, &paths, renderer);
            let Some(after) = ObjState::of(&self.game.engine.world, obj) else { continue };
            if before != after {
                edits.push(Edit::Changed { entity: obj, before, after });
            }
        }
        if !edits.is_empty() {
            self.history.record_step(Edit::Group(edits));
            self.unsaved = true;
        }
    }

    /// Writes what the instance `entity` is part of changes into its prefab.
    pub(super) fn apply_instance(&mut self, entity: Entity, renderer: &mut Renderer) {
        let game = &mut self.game;
        let Some(member) = prefab_overrides::member(&game.engine.world, entity) else { return };
        let changes = prefab_overrides::instance_overrides(&game.engine.world, &mut game.content, member.root);
        if changes.is_empty() {
            return self.notify("Nothing differs from the prefab");
        }
        self.apply_overrides(&changes, renderer);
    }

    /// Writes each Obj's values at its paths into their prefab, and gives
    /// the prefab's other instances in the open scenes the values they had
    /// not changed themselves.
    pub(super) fn apply_overrides(&mut self, changes: &[(Entity, Vec<ValuePath>)], renderer: &mut Renderer) {
        let game = &mut self.game;
        let world = &game.engine.world;
        let Some(member) = changes.first().and_then(|(entity, _)| prefab_overrides::member(world, *entity)) else {
            return;
        };
        let others: Vec<Entity> = (prefab_overrides::instances_of(world, member.prefab).into_iter())
            .filter(|&root| root != member.root)
            .collect();
        let kept: HashMap<Entity, Vec<ValuePath>> = others
            .iter()
            .flat_map(|&root| prefab_overrides::instance_overrides(world, &mut game.content, root))
            .collect();
        let applied: Vec<_> = changes
            .iter()
            .filter_map(|(entity, paths)| Some((prefab_overrides::member(world, *entity)?.path, paths.clone())))
            .collect();
        let path = match prefab_overrides::apply(world, &mut game.content, changes) {
            Ok(path) => path,
            Err(error) => return self.notify(&format!("Could not apply to the prefab: {error:#}")),
        };
        let world = &mut game.engine.world;
        let followed =
            prefab_overrides::follow_prefab(world, &mut game.content, Some(renderer), &applied, &kept, &others);
        if let Err(error) = followed {
            tracing::warn!("the other instances of {path} show it when the level opens again: {error:#}");
        }
        // The instance's file form no longer lists what it applied.
        self.unsaved = true;
        self.overrides_seen = None;
        self.asset_changed(renderer, &path);
        let name = path.rsplit('/').next().unwrap_or(&path);
        self.notify(&format!("Applied to {name}"));
    }
}
