//! The running game's behaviours as the Inspector shows and edits them
//! while it plays: each field scenes can set, read from the instance as a
//! scene writes it; a value set on the instance; and a `[Button]` or
//! `[OnValueChanged]` method called on it. Voxear.Core's `LiveFields`
//! answers, through the Inspector's JSON calls. The data assets the
//! scripts loaded are changed here too, as the Inspector writes their
//! files, which `Definitions` answers.

use anyhow::{Result, bail};
use serde_json::json;
use voxear_ecs::World;
use voxear_scene::BehaviourHandle;

use crate::{ScriptRuntime, inspector::to_toml};

impl ScriptRuntime {
    /// Each behaviour's fields as they are now, as a scene writes them, or
    /// `None` for one no longer running.
    pub fn live_fields(&mut self, world: &mut World, handles: &[BehaviourHandle]) -> Result<Vec<Option<toml::Table>>> {
        let handles: Vec<u32> = handles.iter().map(|handle| handle.0).collect();
        let answer = self.ask_live(world, &json!({ "action": "live-read", "handles": handles }))?;
        let behaviours = answer.get("behaviours").and_then(|b| b.as_array()).cloned().unwrap_or_default();
        Ok(behaviours
            .into_iter()
            .map(|fields| match to_toml(fields) {
                Some(toml::Value::Table(table)) => Some(table),
                _ => None,
            })
            .collect())
    }

    /// Sets a field of a running behaviour to a value as a scene writes it.
    pub fn set_live_field(
        &mut self,
        world: &mut World,
        handle: BehaviourHandle,
        field: &str,
        value: &toml::Value,
    ) -> Result<()> {
        let request = json!({ "action": "live-write", "handle": handle.0, "field": field, "value": value });
        self.ask_live(world, &request).map(drop)
    }

    /// Calls a method that takes nothing on a running behaviour.
    pub fn call_live(&mut self, world: &mut World, handle: BehaviourHandle, method: &str) -> Result<()> {
        let request = json!({ "action": "live-call", "handle": handle.0, "method": method });
        self.ask_live(world, &request).map(drop)
    }

    /// Changes the instance the scripts loaded of the data asset `guid`,
    /// as the Inspector changed its file: the fields in `changed` set, as
    /// a scene writes them, and those `reset` names back to the script's
    /// values. What holds it sees the new values at once. False when the
    /// scripts have not loaded it.
    pub fn refill_data_asset(
        &mut self,
        world: &mut World,
        guid: &str,
        changed: &toml::Table,
        reset: &[String],
    ) -> Result<bool> {
        let request = json!({ "action": "data-refill", "guid": guid, "fields": changed, "reset": reset });
        let answer = self.ask_live(world, &request)?;
        Ok(answer.get("refilled").and_then(|r| r.as_bool()).unwrap_or(false))
    }

    /// Lets go of every data asset the scripts loaded, as a run of the game
    /// ends, so values its code set on them go too, and the next run reads
    /// their files again.
    pub fn forget_data_assets(&mut self, world: &mut World) -> Result<()> {
        self.ask_live(world, &json!({ "action": "data-forget" })).map(drop)
    }

    fn ask_live(&mut self, world: &mut World, request: &serde_json::Value) -> Result<serde_json::Value> {
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("{error}");
        }
        Ok(answer)
    }
}
