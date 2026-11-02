//! Behaviours' gizmos (`DrawGizmos` in Voxear.Core), drawn each frame for
//! the Scene views that show gizmos: while editing, from each behaviour's
//! scene values, for the types that draw any; while playing, by the running
//! behaviours. Each is told whether its Obj is selected, and whether the
//! pointer is over it in the Scene view in use. What they draw goes into
//! debug draw marked as gizmos, which the Game view leaves out.
//!
//! The handles they show (`Handles` in Voxear.Core) are told what the
//! pointer does to them. While editing, the fields a drag changes come
//! back to be written into the scene, as the Inspector would.

use voxear_ecs::Entity;
use voxear_scripting::{GizmoAnswer, GizmoTarget};

use super::Editor;
use crate::fields;

impl Editor {
    /// Draws this frame's gizmos, after debug draw counted down the last
    /// frame's, so they show in the frame they are drawn for.
    pub(super) fn draw_scene_gizmos(&mut self) {
        let answer = self.ask_scene_gizmos();
        if let Some(Err(error)) = &answer {
            tracing::warn!("could not draw the behaviours' gizmos: {error:#}");
        }
        let GizmoAnswer { handles, changed } = answer.and_then(Result::ok).unwrap_or_default();
        self.script_handles.show(handles);
        for (entity, behaviour, values) in changed {
            for (name, value) in values {
                fields::set_raw(&mut self.game.engine.world, entity, &behaviour, &name, value);
            }
        }
    }

    /// The gizmos drawn, and what they gave back, or `None` with nothing to draw.
    fn ask_scene_gizmos(&mut self) -> Option<anyhow::Result<GizmoAnswer>> {
        if !self.all_scene_views().any(|view| view.show_gizmos) {
            return None;
        }
        let (selected, hovered) = (self.selected(), self.hovered_obj);
        let input = self.script_handles.input();
        if self.is_playing() {
            return Some(self.game.engine.draw_live_gizmos(selected, hovered, &input));
        }
        let types = self.inspector.types();
        let world = &self.game.engine.world;
        let drawing: Vec<(Entity, voxear_scene::BehaviourSpec)> = world
            .query_ref::<(Entity, &voxear_engine::EditBehaviours)>()
            .into_iter()
            .flat_map(|(entity, kept)| kept.0.iter().map(move |spec| (entity, spec.clone())))
            .filter(|(_, spec)| types.behaviour(&spec.type_name).is_some_and(|t| t.gizmos))
            .collect();
        if drawing.is_empty() {
            return None;
        }
        let targets: Vec<GizmoTarget> = drawing
            .iter()
            .map(|(entity, spec)| GizmoTarget {
                entity: *entity,
                spec,
                selected: Some(*entity) == selected,
                hovered: Some(*entity) == hovered,
            })
            .collect();
        // The first time, this starts the scripts, as drawers do; until they have, there is nothing to draw.
        self.with_edit_scripts(|scripts, game| scripts.draw_gizmos(&mut game.engine.world, &targets, &input))
    }
}
