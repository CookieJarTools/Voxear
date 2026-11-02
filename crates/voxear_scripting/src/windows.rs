//! The editor's windows written in a game's C# (`EditorWindow` in
//! Voxear.Core). The editor opens each, numbered by itself, and is given the
//! markup to build in its panel; then, every frame it shows, the window
//! raises what was clicked or changed in it and updates, while the editor
//! lends its UI to answer what it asks of its elements. Voxear.Core's
//! `EditorWindows` keeps them open; questions and answers cross as JSON, as
//! the Inspector's drawers' do.

use anyhow::{Context, Result, bail};
use serde_json::json;
use voxear_ecs::World;

use crate::{ScriptRuntime, ScriptServices, ServicesScope};

/// How a window's update went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowUpdate {
    Updated,
    /// Its code threw this, which is logged.
    Threw(String),
    /// The scripts let go of it, as when they reload, so it is to be opened again.
    Gone,
}

impl ScriptRuntime {
    /// Opens a window of type `type_name` as `window`, and gives the markup
    /// to build it from.
    pub fn open_window(&mut self, world: &mut World, window: u32, type_name: &str) -> Result<String> {
        let answer =
            self.ask_windows(world, &json!({ "action": "window-open", "window": window, "type": type_name }))?;
        let markup = answer.get("markup").and_then(|m| m.as_str()).context("Voxear.Core gave the window no markup")?;
        Ok(markup.to_owned())
    }

    /// Updates each window, its elements in the document given with it,
    /// lending `services` for what they ask of them, and says how each went.
    pub fn update_windows(
        &mut self,
        world: &mut World,
        services: &mut dyn ScriptServices,
        windows: &[(u32, u32)],
    ) -> Result<Vec<WindowUpdate>> {
        let asked: Vec<serde_json::Value> =
            windows.iter().map(|&(window, document)| json!({ "window": window, "document": document })).collect();
        let _services = ServicesScope::enter(services);
        let answer = self.ask_windows(world, &json!({ "action": "window-update", "windows": asked }))?;
        let answers = answer.get("windows").and_then(|w| w.as_array()).cloned().unwrap_or_default();
        Ok(answers
            .iter()
            .map(|answer| match (answer.get("error").and_then(|e| e.as_str()), answer.get("gone")) {
                (Some(error), _) => WindowUpdate::Threw(error.to_owned()),
                (None, Some(serde_json::Value::Bool(true))) => WindowUpdate::Gone,
                _ => WindowUpdate::Updated,
            })
            .collect())
    }

    /// Closes a window, which lets go of what it holds.
    pub fn close_window(&mut self, world: &mut World, window: u32) -> Result<()> {
        self.ask_windows(world, &json!({ "action": "window-close", "window": window }))?;
        Ok(())
    }

    fn ask_windows(&mut self, world: &mut World, request: &serde_json::Value) -> Result<serde_json::Value> {
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("{error}");
        }
        Ok(answer)
    }
}
