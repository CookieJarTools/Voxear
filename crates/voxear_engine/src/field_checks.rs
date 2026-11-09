//! The build's check of every scene and prefab, as Odin's validator has it:
//! a `[Required]` field left empty, and a value a `[ValidateInput]` method
//! says is wrong. It reads the files as written, each Obj's own values over
//! what its scripts start them as; a prefab's instances in a scene are left
//! to the prefab's own file. What it finds is listed, and the build goes on.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use serde::Serialize;
use voxear_assets::{AssetDb, AssetKind};
use voxear_scene::BehaviourSpec;

use crate::{
    scene_file,
    script_types::{ScriptField, ScriptTypes, nice_name, same_field},
};

/// Something a check found: where, and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldProblem {
    /// The scene or prefab, by project path.
    pub file: String,
    pub obj: String,
    pub behaviour: String,
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for FieldProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let behaviour = self.behaviour.rsplit('.').next().unwrap_or(&self.behaviour);
        write!(f, "{}: {} ({behaviour}): {}", self.file, self.obj, self.message)
    }
}

/// Asks a `[ValidateInput]` method about a field of a behaviour, as the
/// spec sets it: what is wrong, empty when the method said nothing, or
/// `None` when the value is right or the method could not be asked.
pub type Validate<'a> = dyn FnMut(&BehaviourSpec, &str, &str) -> Option<String> + 'a;

/// Checks every scene and prefab in the project at `root`.
pub fn check(root: &Path, types: &ScriptTypes, validate: &mut Validate) -> Result<Vec<FieldProblem>> {
    let (db, _) = AssetDb::scan(root)?;
    let mut files: Vec<String> = db
        .entries()
        .filter(|entry| matches!(entry.kind, AssetKind::Scene | AssetKind::Prefab))
        .map(|entry| entry.path.clone())
        .collect();
    files.sort();
    let mut problems = Vec::new();
    for file in files {
        let scene = scene_file::read(&root.join(&file)).with_context(|| format!("reading {file}"))?;
        for obj in scene.objs.iter().filter(|obj| obj.prefab.is_none()) {
            for name in obj.behaviours.iter().chain(&obj.components) {
                let Some(described) = types.behaviour(name) else { continue };
                let set = obj.fields.get(name).cloned().unwrap_or_default();
                let spec = BehaviourSpec { fields: Arc::new(set), ..BehaviourSpec::new(name) };
                for field in &described.fields {
                    let place = (file.as_str(), obj.name.as_str(), name.as_str());
                    problems.extend(check_field(place, field, &spec, validate));
                }
            }
        }
    }
    Ok(problems)
}

/// What is wrong with one field, if anything.
fn check_field(
    (file, obj, behaviour): (&str, &str, &str),
    field: &ScriptField,
    spec: &BehaviourSpec,
    validate: &mut Validate,
) -> Option<FieldProblem> {
    let meta = &field.inspector;
    let label = meta.label.clone().unwrap_or_else(|| nice_name(&field.name));
    let set = spec.fields.iter().find(|(key, _)| same_field(key, &field.name)).map(|(_, value)| value);
    let value = set.or(field.default.as_ref());
    let message = if let Some(required) = meta.required.as_ref().filter(|_| !is_set(value)) {
        Some(if required.is_empty() { format!("{label} is required") } else { required.clone() })
    } else {
        let check = meta.validate.as_ref()?;
        let said = validate(spec, &field.name, &check.method)?;
        Some(
            [said, check.message.clone()]
                .into_iter()
                .find(|m| !m.is_empty())
                .unwrap_or(format!("{label} is not valid")),
        )
    }?;
    Some(FieldProblem {
        file: file.to_string(),
        obj: obj.to_string(),
        behaviour: behaviour.to_string(),
        field: field.name.clone(),
        message,
    })
}

/// Whether a reference is set: not nothing, an empty path, or an Obj not
/// saved with its scene.
fn is_set(value: Option<&toml::Value>) -> bool {
    match value {
        None => false,
        Some(toml::Value::String(path)) => !path.is_empty(),
        Some(toml::Value::Table(table)) => !table.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIBED: &str = r#"{"behaviours": [{"name": "Game.Tower", "bases": [], "interfaces": [], "fields": [
      {"name": "door", "type": {"kind": "obj"}, "default": null, "inspector": {"required": ""}},
      {"name": "roof", "type": {"kind": "ref", "asset": "Voxear.Prefab"}, "default": null,
       "inspector": {"required": "A tower needs its roof"}},
      {"name": "paint", "type": {"kind": "text"}, "default": "Red",
       "inspector": {"validate": {"method": "IsColour", "message": "Pick a colour the tower has"}}}
    ]}], "types": {}, "commands": []}"#;

    #[test]
    fn empty_required_fields_and_wrong_values_are_found() {
        let dir = tempfile::tempdir().unwrap();
        let scene = "format = 1\n\
            [[obj]]\nid = 1\nname = \"Good\"\nbehaviours = [\"Game.Tower\"]\n\
            [obj.fields.\"Game.Tower\"]\ndoor = { obj = 2 }\nroof = \"Prefabs/Roof.prefab\"\n\
            [[obj]]\nid = 2\nname = \"Bad\"\nbehaviours = [\"Game.Tower\"]\n\
            [obj.fields.\"Game.Tower\"]\npaint = \"Green\"\n";
        std::fs::create_dir_all(dir.path().join("Scenes")).unwrap();
        std::fs::write(dir.path().join("Scenes/Town.scene"), scene).unwrap();
        let types = crate::script_types::parse(DESCRIBED).unwrap();
        let mut asked = Vec::new();
        let mut validate = |spec: &BehaviourSpec, field: &str, method: &str| {
            asked.push(format!("{}.{method}({field})", spec.type_name));
            let paint = spec.fields.get("paint").and_then(|p| p.as_str()).unwrap_or("Red");
            (paint != "Red").then(String::new)
        };
        let problems = check(dir.path(), &types, &mut validate).unwrap();
        let said: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(
            said,
            [
                "Scenes/Town.scene: Bad (Tower): Door is required",
                "Scenes/Town.scene: Bad (Tower): A tower needs its roof",
                "Scenes/Town.scene: Bad (Tower): Pick a colour the tower has",
            ]
        );
        assert_eq!(asked.len(), 2, "each Obj's paint was asked about");
    }
}
