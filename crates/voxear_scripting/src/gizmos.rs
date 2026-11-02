//! Behaviours' gizmos (`DrawGizmos` in Voxear.Core), which the editor asks
//! for each frame: in edit mode for behaviours kept as data, through copies
//! Voxear.Core keeps and fills again only when their values change; and
//! while the game plays, for the running behaviours. What they draw goes
//! into debug draw marked as gizmos, which only the views showing gizmos
//! draw. Requests cross as JSON, through the Inspector's calls.
//!
//! `Handles` in Voxear.Core are drawn and dragged by the editor: each frame
//! it says which handle its pointer holds and where to, or which it
//! clicked, and is told which handles the gizmos showed and, in edit mode,
//! which fields dragging one changed, for it to write into the scene.

use anyhow::{Result, bail};
use serde_json::json;
use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_scene::BehaviourSpec;

use crate::{ScriptRuntime, inspector::to_toml};

/// A behaviour kept as data in edit mode whose gizmos to draw: its Obj, its
/// values, and whether its Obj is selected or under the pointer.
#[derive(Clone, Copy, Debug)]
pub struct GizmoTarget<'a> {
    pub entity: Entity,
    pub spec: &'a BehaviourSpec,
    pub selected: bool,
    pub hovered: bool,
}

/// One of a behaviour's handles: the behaviour, by its Obj and type, and
/// which of its handles, counted in the order its gizmos show them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HandleId {
    pub entity: Entity,
    pub behaviour: String,
    pub id: i32,
}

/// What the editor's pointer does to the handles this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandleInput {
    /// The handle held, and where to: a point in the world for a position,
    /// a radius, or a distance along a slider.
    pub drag: Option<(HandleId, Vec3, f32)>,
    /// The button clicked.
    pub clicked: Option<HandleId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptHandleKind {
    Position,
    Radius,
    Slider,
    Button,
}

/// A handle the gizmos showed: where it is, and for a radius or slider, how
/// far out and along what.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptHandle {
    pub id: HandleId,
    pub kind: ScriptHandleKind,
    pub at: Vec3,
    pub direction: Vec3,
    pub number: f32,
    pub color: Option<[f32; 4]>,
}

/// What drawing the gizmos gave back: the handles shown; the fields the
/// pointer's handle changed on a behaviour kept as data, as a scene writes
/// them, by Obj and behaviour; and how many gizmo lines each Obj's
/// behaviours drew, in the order they drew them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GizmoAnswer {
    pub handles: Vec<ScriptHandle>,
    pub changed: Vec<(Entity, String, toml::Table)>,
    pub drawn: Vec<(Entity, usize)>,
}

impl ScriptRuntime {
    /// Draws the gizmos of behaviours kept as data in edit mode.
    pub fn draw_gizmos(
        &mut self,
        world: &mut World,
        targets: &[GizmoTarget],
        input: &HandleInput,
    ) -> Result<GizmoAnswer> {
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
        self.ask_gizmos(world, json!({ "action": "gizmos", "behaviours": behaviours }), input)
    }

    /// Draws the running behaviours' gizmos while the game plays, but for
    /// the types `hidden` names.
    pub fn draw_live_gizmos(
        &mut self,
        world: &mut World,
        (selected, hovered): (Option<Entity>, Option<Entity>),
        hidden: &[String],
        input: &HandleInput,
    ) -> Result<GizmoAnswer> {
        let bits = |entity: Option<Entity>| entity.map_or(0, Entity::to_bits);
        let request = json!({
            "action": "live-gizmos",
            "selected": bits(selected),
            "hovered": bits(hovered),
            "hidden": hidden,
        });
        self.ask_gizmos(world, request, input)
    }

    fn ask_gizmos(
        &mut self,
        world: &mut World,
        mut request: serde_json::Value,
        input: &HandleInput,
    ) -> Result<GizmoAnswer> {
        let named = |id: &HandleId| json!({ "entity": id.entity.to_bits(), "behaviour": id.behaviour, "id": id.id });
        if let Some((id, point, number)) = &input.drag {
            let mut drag = named(id);
            drag["point"] = json!(point.to_array());
            drag["number"] = json!(number);
            request["drag"] = drag;
        }
        if let Some(id) = &input.clicked {
            request["clicked"] = named(id);
        }
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("{error}");
        }
        Ok(read_answer(answer))
    }
}

fn read_answer(mut answer: serde_json::Value) -> GizmoAnswer {
    let entity = |value: &serde_json::Value| value["entity"].as_u64().map(Entity::from_bits);
    let vector = |value: &serde_json::Value| {
        let part = |i: usize| value.get(i).and_then(serde_json::Value::as_f64).unwrap_or_default() as f32;
        Vec3::new(part(0), part(1), part(2))
    };
    let handles = answer["handles"].as_array().into_iter().flatten().filter_map(|shown| {
        let kind = match shown["kind"].as_u64()? {
            0 => ScriptHandleKind::Position,
            1 => ScriptHandleKind::Radius,
            2 => ScriptHandleKind::Slider,
            3 => ScriptHandleKind::Button,
            _ => return None,
        };
        let id = HandleId {
            entity: entity(shown)?,
            behaviour: shown["behaviour"].as_str()?.to_string(),
            id: i32::try_from(shown["id"].as_i64()?).ok()?,
        };
        let color = shown["color"]
            .as_array()
            .filter(|parts| parts.len() == 4)
            .map(|parts| std::array::from_fn(|i| parts[i].as_f64().unwrap_or_default() as f32));
        Some(ScriptHandle {
            id,
            kind,
            at: vector(&shown["at"]),
            direction: vector(&shown["direction"]),
            number: shown["number"].as_f64().unwrap_or_default() as f32,
            color,
        })
    });
    let handles = handles.collect();
    let changed =
        answer["changed"].as_array_mut().map(std::mem::take).into_iter().flatten().filter_map(|mut change| {
            let fields = match change["fields"].take() {
                serde_json::Value::Object(fields) => fields,
                _ => return None,
            };
            let fields = fields.into_iter().filter_map(|(name, value)| Some((name, to_toml(value)?))).collect();
            Some((entity(&change)?, change["behaviour"].as_str()?.to_string(), fields))
        });
    let drawn = answer["drawn"].as_array().into_iter().flatten().filter_map(|pair| {
        Some((Entity::from_bits(pair.get(0)?.as_u64()?), usize::try_from(pair.get(1)?.as_u64()?).ok()?))
    });
    GizmoAnswer { handles, changed: changed.collect(), drawn: drawn.collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_gives_its_handles_and_the_fields_they_changed() {
        let answer = json!({
            "handles": [
                { "entity": 7, "behaviour": "Hello.Guard", "id": 0, "kind": 0, "at": [1, 2, 3], "direction": [0, 0, 0], "number": 0 },
                { "entity": 7, "behaviour": "Hello.Guard", "id": 1, "kind": 1, "at": [0, 0, 0], "direction": [0, 0, 0], "number": 2.5, "color": [1, 0, 0, 1] },
                { "entity": 7, "behaviour": "Hello.Guard", "id": 2, "kind": 9, "at": [0, 0, 0], "direction": [0, 0, 0], "number": 0 },
            ],
            "changed": [{ "entity": 7, "behaviour": "Hello.Guard", "fields": { "Target": [1.5, 2, 3], "Note": null } }],
            "drawn": [[7, 96], [9, 12]],
        });
        let answer = read_answer(answer);
        assert_eq!(answer.handles.len(), 2, "a kind it does not know is left out");
        assert_eq!(answer.handles[0].kind, ScriptHandleKind::Position);
        assert_eq!(answer.handles[0].at, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!((answer.handles[1].number, answer.handles[1].color), (2.5, Some([1.0, 0.0, 0.0, 1.0])));
        let (entity, behaviour, fields) = &answer.changed[0];
        assert_eq!((*entity, behaviour.as_str()), (Entity::from_bits(7), "Hello.Guard"));
        assert_eq!(fields.len(), 1, "null has no TOML, so it is left out");
        assert_eq!(fields["Target"].as_array().map(Vec::len), Some(3));
        assert_eq!(answer.drawn, [(Entity::from_bits(7), 96), (Entity::from_bits(9), 12)]);
    }
}
