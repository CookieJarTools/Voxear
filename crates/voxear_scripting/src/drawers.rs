//! The drawers scripts draw the Inspector's fields with (`ValueDrawer` and
//! `AttributeDrawer` in Voxear.Core). They are opened when the Inspector is
//! built, each giving the markup the editor builds it from, and drawn with
//! the field's value, innermost first, while the editor lends its UI to
//! answer what they ask of their elements. Voxear.Core's `InspectorDrawers`
//! keeps them between draws; questions and answers cross as JSON, as the
//! Inspector's other questions do.

use anyhow::{Result, bail};
use serde_json::json;
use voxear_ecs::{Entity, World};

use crate::{ScriptRuntime, ScriptServices, ServicesScope, inspector::to_toml};

/// One step on the way from a type to one of its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldStep<'a> {
    Field(&'a str),
    Item(usize),
}

/// A field to open drawers for.
#[derive(Clone, Debug)]
pub struct DrawerField<'a> {
    /// The behaviour or component the field is on, and the way there.
    pub type_name: &'a str,
    pub path: Vec<FieldStep<'a>>,
    /// The drawers' types, outermost first, as the scripts' description
    /// names them.
    pub drawers: &'a [String],
    pub label: &'a str,
}

/// A field to draw: which of those opened, by its place, the document each
/// of its drawers' markup became, outermost first and as far as they were
/// built, and its value as a scene writes it.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawRequest {
    pub field: usize,
    pub documents: Vec<u32>,
    pub value: Option<toml::Value>,
}

/// What the drawers made of a field's value.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawn {
    pub value: Option<toml::Value>,
    /// Whether it is another value than the one they were given.
    pub changed: bool,
}

impl ScriptRuntime {
    /// Opens the drawers of each field of `entity` for the Inspector
    /// `owner`, letting go of those it opened before, and gives each field's
    /// markups, outermost first, or why it could not be opened, which is
    /// logged. A field with no path is its behaviour, drawn by its inspector.
    pub fn open_drawers(
        &mut self,
        world: &mut World,
        owner: u32,
        entity: Entity,
        fields: &[DrawerField],
    ) -> Result<Vec<Result<Vec<String>, String>>> {
        let fields: Vec<serde_json::Value> = fields
            .iter()
            .map(|field| {
                let path: Vec<serde_json::Value> = field
                    .path
                    .iter()
                    .map(|step| match step {
                        FieldStep::Field(name) => json!(name),
                        FieldStep::Item(index) => json!(index),
                    })
                    .collect();
                json!({ "type": field.type_name, "path": path, "drawers": field.drawers, "label": field.label })
            })
            .collect();
        let opening = json!({ "action": "open", "owner": owner, "entity": entity.to_bits(), "fields": fields });
        let answers = self.ask_drawers(world, &opening)?;
        Ok(answers
            .into_iter()
            .map(|answer| {
                let markups = answer?.get("markups").and_then(|m| m.as_array()).cloned().unwrap_or_default();
                Ok(markups.into_iter().filter_map(|m| m.as_str().map(str::to_owned)).collect())
            })
            .collect())
    }

    /// Draws each field of the Inspector `owner` with its drawers, lending
    /// `services` for what they ask of their elements, and gives what each
    /// made of its value, or what it threw, which is logged.
    pub fn draw_fields(
        &mut self,
        world: &mut World,
        services: &mut dyn ScriptServices,
        owner: u32,
        fields: &[DrawRequest],
    ) -> Result<Vec<Result<Drawn, String>>> {
        let fields: Vec<serde_json::Value> = fields
            .iter()
            .map(|field| json!({ "field": field.field, "documents": field.documents, "value": field.value }))
            .collect();
        let _services = ServicesScope::enter(services);
        let answers = self.ask_drawers(world, &json!({ "action": "draw", "owner": owner, "fields": fields }))?;
        Ok(answers
            .into_iter()
            .map(|answer| {
                let answer = answer?;
                let changed = answer.get("changed").and_then(|c| c.as_bool()).unwrap_or(false);
                Ok(Drawn { value: answer.get("value").cloned().and_then(to_toml), changed })
            })
            .collect())
    }

    /// Asks Voxear.Core, and gives its answer for each field, or the error
    /// it gave for it.
    fn ask_drawers(
        &mut self,
        world: &mut World,
        request: &serde_json::Value,
    ) -> Result<Vec<Result<serde_json::Value, String>>> {
        let answer: serde_json::Value = serde_json::from_str(&self.inspector_call(world, &request.to_string())?)?;
        if let Some(error) = answer.get("error").and_then(|e| e.as_str()) {
            bail!("the Inspector's drawers could not be asked: {error}");
        }
        let fields = answer.get("fields").and_then(|f| f.as_array()).cloned().unwrap_or_default();
        Ok(fields
            .into_iter()
            .map(|field| match field.get("error").and_then(|e| e.as_str()) {
                Some(error) => Err(error.to_owned()),
                None => Ok(field),
            })
            .collect())
    }
}
