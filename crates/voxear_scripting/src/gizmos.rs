//! Behaviours' gizmos (`DrawGizmos` in Voxear.Core), which the editor asks
//! for each frame: in edit mode for behaviours kept as data, through copies
//! Voxear.Core keeps and fills again only when their values change; and
//! while the game plays, for the running behaviours. What they draw goes
//! into debug draw marked as gizmos, which only the views showing gizmos
//! draw. Requests cross as JSON, through the Inspector's calls.

use anyhow::{Result, bail};
use serde_json::json;
use voxear_ecs::{Entity, World};
use voxear_scene::BehaviourSpec;

use crate::ScriptRuntime;

/// A behaviour kept as data in edit mode whose gizmos to draw: its Obj, its
/// values, and whether its Obj is selected or under the pointer.
#[derive(Clone, Copy, Debug)]
pub struct GizmoTarget<'a> {
    pub entity: Entity,
    pub spec: &'a BehaviourSpec,
    pub selected: bool,
    pub hovered: bool,
}

impl ScriptRuntime {
    /// Draws the gizmos of behaviours kept as data in edit mode.
    pub fn draw_gizmos(&mut self, world: &mut World, targets: &[GizmoTarget]) -> Result<()> {
        let behaviours: Vec<serde_json::Value> = targets
            .iter()
            .map(|target| {
                json!({
                    "behaviour": target.spec.type_name,
                    "entity": target.entity.to_bits(),
                    "fields": serde_json::to_value(&*target.spec.fields).unwrap_or_default(),
                    "selected": target.selected,
                    "hovered": target.hovered,
                })
            })
            .collect();
        self.ask_gizmos(world, &json!({ "action": "gizmos", "behaviours": behaviours }))
    }

    /// Draws the running behaviours' gizmos while the game plays.
    pub fn draw_live_gizmos(
        &mut self,
        world: &mut World,
        selected: Option<Entity>,
        hovered: Option<Entity>,
    ) -> Result<()> {
        let bits = |entity: Option<Entity>| entity.map_or(0, Entity::to_bits);
        self.ask_gizmos(
            world,
            &json!({ "action": "live-gizmos", "selected": bits(selected), "hovered": bits(hovered) }),
        )
    }

    fn ask_gizmos(&mut self, world: &mut World, request: &serde_json::Value) -> Result<()> {
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("{error}");
        }
        Ok(())
    }
}
