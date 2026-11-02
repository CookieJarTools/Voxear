//! Behaviours' gizmos (`DrawGizmos` in Voxear.Core), drawn each frame for
//! the Scene views that show gizmos: while editing, from each behaviour's
//! scene values, for the types that draw any; while playing, by the running
//! behaviours. Each is told whether its Obj is selected, and whether the
//! pointer is over it in the Scene view in use. What they draw goes into
//! debug draw marked as gizmos, which the Game view leaves out.

use voxear_ecs::Entity;
use voxear_scripting::GizmoTarget;

use super::Editor;

impl Editor {
    /// Draws this frame's gizmos, after debug draw counted down the last
    /// frame's, so they show in the frame they are drawn for.
    pub(super) fn draw_scene_gizmos(&mut self) {
        if !self.all_scene_views().any(|view| view.show_gizmos) {
            return;
        }
        let (selected, hovered) = (self.selected(), self.hovered_obj);
        if self.is_playing() {
            if let Err(error) = self.game.engine.draw_live_gizmos(selected, hovered) {
                tracing::warn!("could not draw the behaviours' gizmos: {error:#}");
            }
            return;
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
            return;
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
        let drawn = self.with_edit_scripts(|scripts, game| scripts.draw_gizmos(&mut game.engine.world, &targets));
        if let Some(Err(error)) = drawn {
            tracing::warn!("could not draw the behaviours' gizmos: {error:#}");
        }
    }
}
