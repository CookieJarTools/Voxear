//! Data assets in the editor: `.asset` files, each the fields of the C#
//! `Definition` its `type` names. The Inspector shows one with the rows it
//! shows a behaviour's fields with, on an Obj made for it alone and kept
//! out of sight ([`spawn_holder`]); what is changed there is written back
//! to the file, its `type` first and then `[fields]` ([`text`]). The
//! Project panel's Create menu makes new ones ([`create`]).

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use voxear_ecs::{Entity, World};
use voxear_engine::{
    EditBehaviours,
    script_types::{BehaviourType, nice_name},
};
use voxear_scene::{BehaviourSpec, EditorOnly, Name, Transform};

use crate::project_files::unique;

/// Marks the Obj a data asset shows on: the file it is, by its project
/// path, and its fields as they were last read from or written to it, to
/// tell when the Inspector changed them.
#[derive(Clone, Debug, PartialEq)]
pub struct DataAssetHolder {
    pub path: String,
    pub written: toml::Table,
}
impl voxear_ecs::Component for DataAssetHolder {}

/// A data asset's file as it reads: the `Definition` it names, by its full
/// name, and the fields it sets.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataFile {
    pub type_name: String,
    pub fields: toml::Table,
}

/// Reads a data asset's file.
pub fn read(file: &Path) -> Result<DataFile> {
    let text = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let table: toml::Table = toml::from_str(&text).with_context(|| format!("reading {}", file.display()))?;
    let type_name = table.get("type").and_then(toml::Value::as_str).unwrap_or_default().to_string();
    let fields = table.get("fields").and_then(toml::Value::as_table).cloned().unwrap_or_default();
    Ok(DataFile { type_name, fields })
}

/// A data asset's file as it is written: its `type` first, then the fields
/// it sets under `[fields]`.
pub fn text(type_name: &str, fields: &toml::Table) -> Result<String> {
    let body = toml::Table::from_iter([("fields".to_string(), toml::Value::Table(fields.clone()))]);
    let kind = toml::Value::String(type_name.to_string());
    Ok(format!("type = {kind}\n\n{}", toml::to_string(&body)?))
}

/// Makes the Obj a data asset at `path` shows on: named for the file, out
/// of the Hierarchy and never saved, with the Definition's fields as the
/// one behaviour it has.
pub fn spawn_holder(world: &mut World, path: &str, data: DataFile) -> Entity {
    let name = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".asset").to_string();
    let spec =
        BehaviourSpec { type_name: data.type_name, fields: Arc::new(data.fields.clone()), ..BehaviourSpec::default() };
    let holder = DataAssetHolder { path: path.to_string(), written: data.fields };
    world.spawn((Name(name), Transform::IDENTITY, EditorOnly, EditBehaviours(vec![spec]), holder))
}

/// The fields of the Definition a holder shows, as the Inspector has left
/// them, with its type.
pub fn fields_of(world: &World, holder: Entity) -> Option<(String, Arc<toml::Table>)> {
    let spec = world.get::<EditBehaviours>(holder)?.0.first()?;
    Some((spec.type_name.clone(), spec.fields.clone()))
}

/// What a Definition is called in the Create menu and on new data assets:
/// the last part of its `[CreateAsset]` path, or its class name in words.
pub fn new_name(definition: &BehaviourType) -> String {
    let menu = definition.menu.as_deref().map(|path| path.trim().trim_matches('/'));
    match menu.and_then(|path| path.rsplit('/').next()).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => nice_name(definition.short_name()),
    }
}

/// Makes a new data asset of a Definition in `folder` (from `root`), named
/// for it as other new files are, `New Weapon Stats.asset`, its fields left
/// as the script starts them. Returns its path.
pub fn create(root: &Path, folder: &str, definition: &BehaviourType) -> Result<String> {
    let name = unique(&root.join(folder), &format!("New {}", new_name(definition)), ".asset");
    let path = if folder.is_empty() { name } else { format!("{folder}/{name}") };
    let file = root.join(&path);
    std::fs::write(&file, text(&definition.name, &toml::Table::new())?)
        .with_context(|| format!("making {}", file.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_data_asset_is_written_type_first_and_reads_back() {
        let fields: toml::Table = toml::from_str("damage = 35.5\nshots = [1, 2, 3]\nwave = { count = 2 }\n").unwrap();
        let written = text("Hello.WeaponStats", &fields).unwrap();
        assert!(written.starts_with("type = \"Hello.WeaponStats\"\n\n[fields]\n"), "{written}");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Rifle.asset"), &written).unwrap();
        let read = read(&dir.path().join("Rifle.asset")).unwrap();
        assert_eq!(read, DataFile { type_name: "Hello.WeaponStats".into(), fields });
    }

    #[test]
    fn a_new_data_asset_is_named_for_its_menu_place_and_sets_nothing() {
        let named = |menu: Option<&str>| BehaviourType {
            name: "Hello.WeaponStats".into(),
            bases: Vec::new(),
            interfaces: Vec::new(),
            documentation: None,
            fields: Vec::new(),
            buttons: Vec::new(),
            gizmos: false,
            icon: None,
            inspector: None,
            menu: menu.map(str::to_string),
        };
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Weapons")).unwrap();
        let first = create(dir.path(), "Weapons", &named(Some("Weapons/Weapon Stats"))).unwrap();
        let second = create(dir.path(), "Weapons", &named(Some("Weapons/Weapon Stats"))).unwrap();
        assert_eq!(
            (first.as_str(), second.as_str()),
            ("Weapons/New Weapon Stats.asset", "Weapons/New Weapon Stats 1.asset")
        );
        assert_eq!(create(dir.path(), "", &named(None)).unwrap(), "New Weapon Stats.asset");
        let made = read(&dir.path().join(&first)).unwrap();
        assert_eq!(made, DataFile { type_name: "Hello.WeaponStats".into(), fields: toml::Table::new() });
    }
}
