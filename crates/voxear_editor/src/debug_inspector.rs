//! The Inspector's Debug mode, as Unity's: switched from the Inspector's
//! `⋯` menu, each Inspector its own, it shows what is really there rather
//! than what the usual sections draw. For an Obj: its entity, name,
//! parent, scene path and world transform, its prefab links and behaviour
//! handles; every component as its scene file writes it, references with
//! the asset or Obj they hold beside them; every field of each C# behaviour,
//! private and unsaved ones too, with the attributes that group, hide or
//! draw them set aside; and while playing, the live state behind the
//! components: a body's velocity and whether it sleeps, an Audio Source's
//! voice, and the LOD drawn. For an asset: its GUID, kind and path, its
//! `.meta`, what it names and what names it. Every row is read-only; its
//! menu copies the value, or its path in the scene file.

use std::collections::BTreeMap;

use voxear_assets::AssetGuid;
use voxear_core::math::{EulerRot, Vec3};
use voxear_ecs::Entity;
use voxear_engine::{
    EditBehaviours, Game,
    prefab::{FromPrefab, PrefabInstance},
};
use voxear_scene::{Behaviours, Layer, Name, Parent, ScenePersistence};
use voxear_scripting::DebugField;
use voxear_ui::{MenuItem, NodeId, Ui};

use crate::inspector::{HeaderEnd, number, prop, section};

/// One row: the section it is in, its label and value, where it is kept
/// (its path in the scene file, or what names it), and a word or two on
/// what it is (`private`, `not saved`, `live`), or nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugRow {
    pub section: String,
    pub label: String,
    pub value: String,
    pub path: String,
    pub note: String,
}

impl DebugRow {
    fn new(section: &str, label: &str, value: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            section: section.into(),
            label: label.into(),
            value: value.into(),
            path: path.into(),
            note: String::new(),
        }
    }

    fn note(self, note: &str) -> Self {
        Self { note: note.into(), ..self }
    }

    /// The row as `vox editor call inspector.debug` gives it.
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "section": self.section,
            "label": self.label,
            "value": self.value,
            "path": self.path,
            "note": self.note,
        })
    }
}

/// What the game knows of an Obj while it plays, beyond its components.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LiveState {
    /// Its body's velocity and spin, and whether it sleeps.
    pub body: Option<(Vec3, Vec3, bool)>,
    /// Its Audio Source's voice: playing, paused or stopped, its volume, and where it is heard from.
    pub voice: Option<(&'static str, f32, Option<Vec3>)>,
    /// The LOD of its mesh drawn, and how many it has.
    pub lod: Option<(usize, usize)>,
    /// The time its clip is at, in seconds, and whether it loops.
    pub clip: Option<(f32, bool)>,
}

/// A behaviour's fields as its code has them, from `ScriptRuntime::debug_fields`.
pub struct ScriptFields {
    pub type_name: String,
    pub running: bool,
    pub fields: Result<Vec<DebugField>, String>,
}

/// Every row of an Obj (see the module's notes).
pub fn obj_rows(game: &mut Game, entity: Entity, scripts: &[ScriptFields], live: Option<LiveState>) -> Vec<DebugRow> {
    let mut rows = engine_rows(game, entity);
    rows.extend(scene_rows(game, entity));
    for script in scripts {
        rows.extend(script_rows(script));
    }
    if let Some(live) = live {
        rows.extend(live_rows(&live));
    }
    rows
}

/// The Obj as the engine keeps it, apart from its components.
fn engine_rows(game: &Game, entity: Entity) -> Vec<DebugRow> {
    let world = &game.engine.world;
    let obj = "Obj";
    let mut rows = vec![
        DebugRow::new(obj, "Entity", entity_text(entity), "").note("engine"),
        DebugRow::new(obj, "Name", world.get::<Name>(entity).map_or_else(String::new, |n| n.0.clone()), "name"),
        DebugRow::new(obj, "Scene path", scene_path(game, entity), ""),
    ];
    let parent = world.get::<Parent>(entity).map(|parent| parent.0);
    rows.push(DebugRow::new(obj, "Parent", parent.map_or_else(|| "None".into(), |p| obj_text(game, p)), "parent"));
    let children = crate::props::children(game, entity).len();
    rows.push(DebugRow::new(obj, "Children", children.to_string(), "").note("engine"));
    let layer = world.get::<Layer>(entity).map_or(0, |layer| layer.0);
    rows.push(DebugRow::new(obj, "Layer", layer.to_string(), "layer"));
    let persistent = world.get::<ScenePersistence>(entity).is_some();
    rows.push(DebugRow::new(obj, "Persistent", persistent.to_string(), "persistent"));

    let placed = "World Transform";
    let (scale, rotation, position) = voxear_scene::world_matrix(world, entity).to_scale_rotation_translation();
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    let degrees = Vec3::new(x, y, z) * (180.0 / std::f32::consts::PI);
    rows.push(DebugRow::new(placed, "Position", vector(position), "").note("engine"));
    rows.push(DebugRow::new(placed, "Rotation", vector(degrees), "").note("engine"));
    rows.push(DebugRow::new(placed, "Scale", vector(scale), "").note("engine"));

    let prefab = "Prefab";
    if let Some(instance) = world.get::<PrefabInstance>(entity) {
        rows.push(DebugRow::new(prefab, "Instance of", guid_text(game, instance.0), "prefab"));
    }
    if let Some(from) = world.get::<FromPrefab>(entity) {
        rows.push(DebugRow::new(prefab, "In instance", obj_text(game, from.instance), "").note("engine"));
        rows.push(DebugRow::new(prefab, "Path in prefab", from.path.to_string(), "").note("engine"));
    }

    let behaviours = "Behaviours";
    if let Some(running) = world.get::<Behaviours>(entity) {
        for attached in &running.0 {
            let handle = format!("handle {}", attached.handle.0);
            rows.push(DebugRow::new(behaviours, &attached.spec.type_name, handle, "").note("running"));
        }
    } else if let Some(kept) = world.get::<EditBehaviours>(entity) {
        for spec in &kept.0 {
            rows.push(DebugRow::new(behaviours, &spec.type_name, "not running", "").note("edit"));
        }
    }
    rows
}

/// Every component as the scene file writes the Obj, a section each.
fn scene_rows(game: &mut Game, entity: Entity) -> Vec<DebugRow> {
    let scene = voxear_engine::scene_file::capture(&game.engine.world, &mut game.content, &[entity]);
    let Some(obj) = scene.objs.first() else { return Vec::new() };
    let Ok(toml::Value::Table(table)) = toml::Value::try_from(obj) else { return Vec::new() };
    let mut rows = Vec::new();
    let mut tables: BTreeMap<String, toml::Table> = BTreeMap::new();
    for (key, value) in &table {
        match value {
            toml::Value::Table(inner) => {
                tables.insert(key.clone(), inner.clone());
            }
            toml::Value::Array(items) if items.iter().all(toml::Value::is_table) && !items.is_empty() => {
                for (at, item) in items.iter().enumerate() {
                    let named = item.get("type").and_then(toml::Value::as_str).map(str::to_string);
                    let title = named.unwrap_or_else(|| format!("{key} {}", at + 1));
                    tables.insert(format!("{key}[{at}] {title}"), item.as_table().cloned().unwrap_or_default());
                }
            }
            other => rows.push(DebugRow::new("Scene File", key, value_text(game, other), key.as_str())),
        }
    }
    for (key, inner) in tables {
        let (path, title) = match key.split_once(' ') {
            Some((path, title)) => (path.to_string(), title.to_string()),
            None => (key.clone(), title_of(&key)),
        };
        flatten(game, &title, &path, &inner, &mut rows);
    }
    rows
}

/// A table's values as rows of `section`, inner tables' as dotted keys.
fn flatten(game: &Game, section: &str, path: &str, table: &toml::Table, rows: &mut Vec<DebugRow>) {
    for (key, value) in table {
        let at = format!("{path}.{key}");
        match value {
            toml::Value::Table(inner) => flatten(game, section, &at, inner, rows),
            other => {
                let label = at.split_once('.').map_or(at.as_str(), |(_, rest)| rest).to_string();
                rows.push(DebugRow::new(section, &label, value_text(game, other), at));
            }
        }
    }
}

/// A C# behaviour's fields, every one, as its code has them.
fn script_rows(script: &ScriptFields) -> Vec<DebugRow> {
    let section = format!("{} (C#)", script.type_name);
    let fields = match &script.fields {
        Ok(fields) => fields,
        Err(error) => return vec![DebugRow::new(&section, "Not read", error.clone(), "")],
    };
    let instance = if script.running { "running instance" } else { "copy from the scene" };
    let mut rows = vec![DebugRow::new(&section, "Read from", instance, "")];
    for field in fields {
        let mut words = vec![field.access.clone()];
        if !field.saved {
            words.push("not saved".into());
        }
        if field.declared != script.type_name.rsplit('.').next().unwrap_or_default() {
            words.push(format!("from {}", field.declared));
        }
        let label = format!("{}: {}", field.name, field.type_name);
        let path = if field.saved { format!("behaviours.{}.{}", script.type_name, field.name) } else { String::new() };
        rows.push(DebugRow::new(&section, &label, field.value.clone(), path).note(&words.join(", ")));
    }
    rows
}

/// What the running game knows of the Obj, beyond its components.
fn live_rows(live: &LiveState) -> Vec<DebugRow> {
    let section = "Live";
    let mut rows = Vec::new();
    if let Some((velocity, spin, asleep)) = live.body {
        rows.push(DebugRow::new(section, "Velocity", format!("{} m/s", vector(velocity)), "").note("live"));
        rows.push(DebugRow::new(section, "Angular velocity", format!("{} rad/s", vector(spin)), "").note("live"));
        rows.push(DebugRow::new(section, "Asleep", asleep.to_string(), "").note("live"));
    }
    if let Some((state, volume, at)) = live.voice {
        rows.push(DebugRow::new(section, "Voice", state, "").note("live"));
        rows.push(DebugRow::new(section, "Voice volume", number(volume), "").note("live"));
        if let Some(at) = at {
            rows.push(DebugRow::new(section, "Heard from", vector(at), "").note("live"));
        }
    }
    if let Some((time, looping)) = live.clip {
        rows.push(DebugRow::new(section, "Clip time", format!("{} s", number(time)), "").note("live"));
        rows.push(DebugRow::new(section, "Clip loops", looping.to_string(), "").note("live"));
    }
    if let Some((drawn, count)) = live.lod {
        rows.push(DebugRow::new(section, "LOD drawn", format!("{drawn} of {count}"), "").note("live"));
    }
    rows
}

/// Every row of an asset of the project, by its path.
pub fn asset_rows(game: &Game, path: &str) -> Vec<DebugRow> {
    let db = game.content.db();
    let section = "Asset";
    let Some(guid) = db.guid_of(path) else {
        return vec![DebugRow::new(section, "Path", path, ""), DebugRow::new(section, "GUID", "None", "")];
    };
    let mut rows =
        vec![DebugRow::new(section, "GUID", guid.to_string(), "guid"), DebugRow::new(section, "Path", path, "")];
    if let Some(entry) = db.get(guid) {
        rows.push(DebugRow::new(section, "Kind", format!("{:?}", entry.kind), ""));
        rows.push(DebugRow::new(section, "Mount", format!("{:?}", entry.mount), ""));
    }
    let file = db.file(path);
    if let Ok(size) = std::fs::metadata(&file).map(|meta| meta.len()) {
        rows.push(DebugRow::new(section, "Size", format!("{size} bytes"), ""));
    }
    let meta = voxear_assets::meta_path(&file);
    let text = std::fs::read_to_string(&meta).unwrap_or_default();
    match text.parse::<toml::Table>() {
        Ok(table) => flatten(game, ".meta", "meta", &table, &mut rows),
        Err(_) if text.is_empty() => rows.push(DebugRow::new(".meta", "File", "None", "")),
        Err(error) => rows.push(DebugRow::new(".meta", "Not read", error.to_string(), "")),
    }
    let needs = voxear_engine::content_build::asset_needs(db, guid).unwrap_or_default();
    for needed in &needs {
        rows.push(DebugRow::new("Depends On", &guid_text(game, *needed), needed.to_string(), ""));
    }
    if needs.is_empty() {
        rows.push(DebugRow::new("Depends On", "Nothing", "", ""));
    }
    let users = users_of(game, guid);
    for user in &users {
        rows.push(DebugRow::new("Used By", &guid_text(game, *user), user.to_string(), ""));
    }
    if users.is_empty() {
        rows.push(DebugRow::new("Used By", "Nothing", "", ""));
    }
    rows
}

/// The project's assets that name `guid`, as a build finds what each needs.
fn users_of(game: &Game, guid: AssetGuid) -> Vec<AssetGuid> {
    let db = game.content.db();
    let mut users: Vec<(String, AssetGuid)> = db
        .entries()
        .filter(|entry| entry.guid != guid && entry.mount == voxear_assets::Mount::Project)
        .filter(|entry| voxear_engine::content_build::asset_needs(db, entry.guid).is_ok_and(|n| n.contains(&guid)))
        .map(|entry| (entry.path.clone(), entry.guid))
        .collect();
    users.sort();
    users.into_iter().map(|(_, guid)| guid).collect()
}

/// A value as the scene file writes it, a GUID with the asset's path after.
fn value_text(game: &Game, value: &toml::Value) -> String {
    match value {
        toml::Value::String(text) => match text.parse::<AssetGuid>() {
            Ok(guid) => guid_text(game, guid),
            Err(_) => format!("\"{text}\""),
        },
        toml::Value::Float(number) => crate::inspector::number(*number as f32),
        toml::Value::Array(items) => {
            let shown: Vec<String> = items.iter().map(|item| value_text(game, item)).collect();
            format!("[{}]", shown.join(", "))
        }
        other => other.to_string(),
    }
}

/// An asset's GUID and the path it is at, or the GUID alone where no asset has it.
fn guid_text(game: &Game, guid: AssetGuid) -> String {
    match game.content.db().get(guid) {
        Some(entry) => format!("{guid} ({})", entry.path),
        None => format!("{guid} (missing)"),
    }
}

/// An Obj's entity and its scene path.
fn obj_text(game: &Game, entity: Entity) -> String {
    format!("{} ({})", entity_text(entity), scene_path(game, entity))
}

fn entity_text(entity: Entity) -> String {
    format!("{}v{} (bits {})", entity.index(), entity.generation(), entity.to_bits())
}

/// An Obj's names from the top of its scene down, as `Player/Arm/Hand`.
fn scene_path(game: &Game, entity: Entity) -> String {
    let world = &game.engine.world;
    let mut names = Vec::new();
    let mut at = Some(entity);
    while let Some(obj) = at.filter(|_| names.len() < 64) {
        names.push(world.get::<Name>(obj).map_or_else(|| "Obj".to_string(), |name| name.0.clone()));
        at = world.get::<Parent>(obj).map(|parent| parent.0);
    }
    names.reverse();
    names.join("/")
}

fn vector(v: Vec3) -> String {
    format!("{}, {}, {}", number(v.x), number(v.y), number(v.z))
}

/// A scene file's key as a title: `mesh_renderer` as Mesh Renderer.
fn title_of(key: &str) -> String {
    let words = key.split('_').map(|word| {
        let mut letters = word.chars();
        letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
    });
    words.collect::<Vec<_>>().join(" ")
}

/// The rows as the Inspector shows them: a section each, and each row's menu
/// to copy its value or its path.
pub struct DebugView {
    root: NodeId,
    rows: Vec<(NodeId, NodeId, DebugRow)>,
    /// The sections' titles and rows' labels shown, to build again only when they change.
    shape: Vec<(String, String)>,
    /// When its rows were last read.
    read: Option<std::time::Instant>,
}

impl DebugView {
    pub fn new(root: NodeId) -> Self {
        Self { root, rows: Vec::new(), shape: Vec::new(), read: None }
    }

    /// Whether its rows are to be read again: at first, then each `every`
    /// seconds, noting it.
    pub fn due(&mut self, every: f32) -> bool {
        let due = self.read.is_none_or(|read| read.elapsed().as_secs_f32() >= every);
        if due {
            self.read = Some(std::time::Instant::now());
        }
        due
    }

    /// Whether its rows have been read once.
    pub fn read_once(&self) -> bool {
        self.read.is_some()
    }

    /// Shows `rows`: their values in place where the same rows show,
    /// or built again.
    pub fn show(&mut self, ui: &mut Ui, rows: Vec<DebugRow>) {
        let shape: Vec<(String, String)> = rows.iter().map(|row| (row.section.clone(), row.label.clone())).collect();
        if shape == self.shape {
            for ((_, value, shown), row) in self.rows.iter_mut().zip(rows) {
                if shown.value != row.value {
                    ui.set_text(*value, &row.value);
                }
                *shown = row;
            }
            return;
        }
        for child in ui.children(self.root) {
            ui.remove(child);
        }
        let banner = ui.add_label(self.root, "Debug");
        ui.add_class(banner, "debug-banner");
        self.rows.clear();
        let mut content: Option<(String, NodeId)> = None;
        for row in rows {
            if content.as_ref().is_none_or(|(title, _)| *title != row.section) {
                let (_, rows_node, _) = section(ui, self.root, &row.section, "info", HeaderEnd::Nothing);
                content = Some((row.section.clone(), rows_node));
            }
            let Some((_, rows_node)) = &content else { continue };
            let line = prop(ui, *rows_node, &row.label);
            ui.add_class(line, "debug-row");
            let value = ui.add_label(line, &row.value);
            ui.add_class(value, "debug-value");
            let tip = match (row.note.is_empty(), row.path.is_empty()) {
                (true, true) => row.label.clone(),
                (false, true) => format!("{}\n{}", row.label, row.note),
                (true, false) => format!("{}\n{}", row.label, row.path),
                (false, false) => format!("{}\n{}\n{}", row.label, row.note, row.path),
            };
            ui.set_tooltip(line, &tip, None);
            if !row.note.is_empty() {
                let note = ui.add_label(line, &row.note);
                ui.add_class(note, "debug-note");
            }
            self.rows.push((line, value, row));
        }
        self.shape = shape;
    }

    /// The text a row's menu asked to copy this frame.
    pub fn update(&mut self, ui: &mut Ui) -> Option<String> {
        let mut copied = None;
        for (line, _, row) in &self.rows {
            if ui.context_clicked(*line) {
                let at = ui.pointer_position().unwrap_or_default();
                let items = [MenuItem::new("Copy Value"), MenuItem::new("Copy Path").enabled(!row.path.is_empty())];
                ui.open_menu(*line, at, &items);
            }
            match ui.menu_choice(*line) {
                Some(0) => copied = Some(row.value.clone()),
                Some(_) => copied = Some(row.path.clone()),
                None => {}
            }
        }
        copied
    }

    /// The rows shown.
    pub fn rows(&self) -> impl Iterator<Item = &DebugRow> {
        self.rows.iter().map(|(_, _, row)| row)
    }

    /// The row element of the row with `label` in `section`, for tests.
    pub fn row(&self, section: &str, label: &str) -> Option<NodeId> {
        self.rows.iter().find(|(_, _, row)| row.section == section && row.label == label).map(|(line, ..)| *line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_fields_read_as_the_inspector_names_them() {
        assert_eq!(title_of("mesh_renderer"), "Mesh Renderer");
        let script = ScriptFields {
            type_name: "Game.Spinner".into(),
            running: false,
            fields: Ok(vec![DebugField {
                name: "turns".into(),
                type_name: "Int32".into(),
                declared: "Spinner".into(),
                access: "private".into(),
                saved: false,
                value: "3".into(),
            }]),
        };
        let rows = script_rows(&script);
        assert_eq!(rows[0].value, "copy from the scene");
        assert_eq!((rows[1].label.as_str(), rows[1].note.as_str()), ("turns: Int32", "private, not saved"));
        assert!(rows[1].path.is_empty(), "a field scenes do not save has no path in one");
    }
}
