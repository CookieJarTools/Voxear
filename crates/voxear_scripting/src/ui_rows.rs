//! The rows of the game UI's list and tree views, which the scripts make and
//! fill (`ListView` and `TreeView` in Voxear.Core). Each frame, after the
//! scripts' updates, the game lays its UI out, and the scripts make the rows
//! that came on screen and fill those that show another item, while the game
//! lends its UI to answer what they ask of their elements. The rows cross as
//! JSON, as the Inspector's drawers do.

use anyhow::{Result, bail};
use serde_json::json;
use voxear_ecs::World;

use crate::{ScriptRuntime, ScriptServices, ServicesScope};

/// A row of a list or tree view for the scripts to make, fill, or both, its
/// view and itself as the scripts number elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowWork {
    pub list: u32,
    pub row: u32,
    /// Whether the row is new, so what it shows is made first.
    pub made: bool,
    /// The item it shows now, when that changed: a list's by its index, a
    /// tree's by its id.
    pub item: Option<u64>,
}

impl ScriptRuntime {
    /// Has the scripts make and fill `rows`, lending `services` for what
    /// they ask of the rows' elements. What their code threw is logged.
    pub fn fill_rows(&mut self, world: &mut World, services: &mut dyn ScriptServices, rows: &[RowWork]) -> Result<()> {
        let rows: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| json!({ "list": row.list, "row": row.row, "made": row.made, "item": row.item }))
            .collect();
        let _services = ServicesScope::enter(services);
        let answer: serde_json::Value = serde_json::from_str(
            &self.inspector_call(world, &json!({ "action": "ui-rows", "rows": rows }).to_string())?,
        )?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("the UI's list rows could not be filled: {error}");
        }
        Ok(())
    }
}
