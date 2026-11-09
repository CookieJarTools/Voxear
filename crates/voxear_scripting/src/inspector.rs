//! The Inspector's questions to a behaviour's code in edit mode, when it has
//! no instance: a `[Button]` or `[OnValueChanged]` method to call, a
//! `[ValidateInput]` method to ask about a field, or a `[ValueDropdown]`
//! member's values. Voxear.Core's `InspectorCalls` makes a copy of the
//! behaviour from the scene's values, bound to its Obj, asks it, and lets it
//! go; questions and answers cross as JSON.

use anyhow::{Context, Result, bail, ensure};
use voxear_ecs::{Entity, World};
use voxear_scene::BehaviourSpec;

use crate::{ScriptRuntime, interop::WorldScope};

/// What the Inspector asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorAsk<'a> {
    /// Calls a method that takes nothing.
    Call(&'a str),
    /// Asks a method whether a field's value is right.
    Validate { method: &'a str, field: &'a str },
    /// The values a member offers.
    Dropdown(&'a str),
}

/// What the behaviour answered.
#[derive(Clone, Debug, PartialEq)]
pub enum InspectorAnswer {
    /// The fields a call changed, as scenes write them.
    Changed(toml::Table),
    /// Whether the value is right, and what the method said about it.
    Valid(bool, String),
    /// Each value offered, with its name.
    Options(Vec<(String, toml::Value)>),
}

impl ScriptRuntime {
    /// Asks the behaviour `spec` describes, on `entity`, as the Inspector
    /// does in edit mode. What it throws is logged, and is the error.
    pub fn inspector(
        &mut self,
        world: &mut World,
        entity: Entity,
        spec: &BehaviourSpec,
        ask: InspectorAsk,
    ) -> Result<InspectorAnswer> {
        let (action, member, field) = match ask {
            InspectorAsk::Call(method) => ("call", method, ""),
            InspectorAsk::Validate { method, field } => ("validate", method, field),
            InspectorAsk::Dropdown(member) => ("dropdown", member, ""),
        };
        let request = serde_json::json!({
            "behaviour": spec.type_name,
            "entity": entity.to_bits(),
            "fields": serde_json::to_value(&*spec.fields)?,
            "action": action,
            "member": member,
            "field": field,
        });
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)
            .context("Voxear.Core's answer is not JSON")?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("{}.{member} threw: {error}", spec.type_name);
        }
        Ok(match ask {
            InspectorAsk::Call(_) => {
                let changed = answer.get("changed").and_then(|c| c.as_object()).cloned().unwrap_or_default();
                InspectorAnswer::Changed(changed.into_iter().filter_map(|(k, v)| Some((k, to_toml(v)?))).collect())
            }
            InspectorAsk::Validate { .. } => {
                let valid = answer.get("valid").and_then(|v| v.as_bool()).unwrap_or(true);
                let message = answer.get("message").and_then(|m| m.as_str()).unwrap_or_default();
                InspectorAnswer::Valid(valid, message.to_owned())
            }
            InspectorAsk::Dropdown(_) => {
                let options = answer.get("options").and_then(|o| o.as_array()).cloned().unwrap_or_default();
                let named = options.into_iter().filter_map(|option| {
                    let label = option.get("label")?.as_str()?.to_owned();
                    Some((label, to_toml(option.get("value")?.clone())?))
                });
                InspectorAnswer::Options(named.collect())
            }
        })
    }

    /// Sends the request and returns the answer, asking for it again into a
    /// larger buffer when it did not fit, without running anything twice.
    pub(crate) fn inspector_call(&mut self, world: &mut World, request: &str) -> Result<String> {
        let call = self.api().inspector_call.expect("validated at start");
        let _scope = WorldScope::enter(world);
        let mut buffer = vec![0u8; 16 * 1024];
        let mut length =
            unsafe { call(request.as_ptr(), request.len() as i32, buffer.as_mut_ptr(), buffer.len() as i32) };
        ensure!(length >= 0, "Voxear.Core could not answer the Inspector; the Console says why");
        if length as usize > buffer.len() {
            buffer.resize(length as usize, 0);
            length = unsafe { call(std::ptr::null(), 0, buffer.as_mut_ptr(), buffer.len() as i32) };
        }
        buffer.truncate(length.max(0) as usize);
        String::from_utf8(buffer).context("Voxear.Core's answer is not UTF-8")
    }
}

/// A JSON value as a scene writes it; null, which TOML has no word for, is
/// left out.
pub(crate) fn to_toml(value: serde_json::Value) -> Option<toml::Value> {
    Some(match value {
        serde_json::Value::Null => return None,
        serde_json::Value::Bool(b) => toml::Value::Boolean(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64()?),
        },
        serde_json::Value::String(s) => toml::Value::String(s),
        serde_json::Value::Array(items) => toml::Value::Array(items.into_iter().filter_map(to_toml).collect()),
        serde_json::Value::Object(table) => {
            toml::Value::Table(table.into_iter().filter_map(|(k, v)| Some((k, to_toml(v)?))).collect())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_answers_become_scene_values() {
        let json = serde_json::json!({ "speed": 2.5, "count": 3, "name": "Wolf", "gone": null, "at": [1, 2.5] });
        let toml = to_toml(json).unwrap();
        let table = toml.as_table().unwrap();
        assert_eq!(table["speed"], toml::Value::Float(2.5));
        assert_eq!(table["count"], toml::Value::Integer(3));
        assert!(!table.contains_key("gone"), "null is left out");
        assert_eq!(table["at"], toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Float(2.5)]));
    }
}
