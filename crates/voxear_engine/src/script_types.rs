//! What a project's behaviours are, as the C# compiler built them, for the
//! editor: each behaviour a scene can add, the types it derives from and
//! implements, and the fields a scene can set on it, with the kind of value
//! each takes and the value a new instance starts with.
//!
//! [`describe`] builds the scripts and runs the engine's `Voxear.ScriptInfo`
//! tool on the build. The tool reads it with reflection, under the same
//! rules that set fields when the game runs (`BehaviourFields`), and writes
//! `.voxear/Scripts/Behaviours.json`. [`cached`] reads the last one, and
//! [`is_stale`] says whether a script changed since it was written.
//!
//! The tool writes the Code Reference beside it, `Code.json`: every public
//! type of the scripts and of Voxear.Core, with its members and their `///`
//! comments, which VoxDocs shows as pages ([`code_path`]).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use crate::{Project, core_assembly, scripts};

/// How long describing a build may take before it is stopped, as when a
/// script's constructor never returns.
const DESCRIBE_LIMIT: std::time::Duration = std::time::Duration::from_secs(2 * 60);

/// The description's file, beside the scripts' build.
const DESCRIPTION: &str = "Behaviours.json";
/// The Code Reference's file, beside the description.
const CODE: &str = "Code.json";

/// The project's behaviours and C# components, the `[Serializable]` types
/// their fields use, and the `[Command]` methods for the editor's palette.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ScriptTypes {
    pub behaviours: Vec<BehaviourType>,
    /// `IComponent` structs, described as behaviours are, with no bases.
    #[serde(default)]
    pub components: Vec<BehaviourType>,
    /// `SystemBase` classes, as the engine will run them.
    #[serde(default)]
    pub systems: Vec<ScriptSystem>,
    /// Each `[Serializable]` type's fields, by full name.
    #[serde(rename = "types")]
    pub structs: BTreeMap<String, NestedType>,
    #[serde(default)]
    pub commands: Vec<ScriptCommand>,
    /// The `[DocsCheck]` methods, for a VoxDocs guide's checks.
    #[serde(default)]
    pub checks: Vec<ScriptCheck>,
}

/// A static method marked `[DocsCheck]`: what its step says, and where it
/// is, which a page names as `check:code/<type>.<method>`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ScriptCheck {
    pub title: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub method: String,
}

/// A C# system: its full name, the engine stage it runs in ("Update"),
/// and its `[ExecutionOrder]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ScriptSystem {
    pub name: String,
    pub stage: String,
    #[serde(default)]
    pub order: i32,
}

/// A static method marked `[Command]`: what the palette calls it, keys that
/// run it ("Ctrl+Shift+L", or empty), and where it is.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ScriptCommand {
    pub title: String,
    #[serde(default)]
    pub keys: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub method: String,
}

impl ScriptTypes {
    /// The behaviour or C# component of this name, which the Inspector shows
    /// the same way.
    pub fn behaviour(&self, name: &str) -> Option<&BehaviourType> {
        self.behaviours.iter().chain(&self.components).find(|b| b.name == name)
    }

    pub fn is_component(&self, name: &str) -> bool {
        self.components.iter().any(|c| c.name == name)
    }

    pub fn fields_of(&self, structure: &str) -> &[ScriptField] {
        self.structs.get(structure).map_or(&[], |s| s.fields.as_slice())
    }
}

/// A behaviour class, and the fields a scene can set on it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BehaviourType {
    /// The full name, as scenes write it: `Hello.Spinner`.
    pub name: String,
    /// The classes it derives from, nearest first, up to `Behaviour`.
    #[serde(default)]
    pub bases: Vec<String>,
    #[serde(default)]
    pub interfaces: Vec<String>,
    /// The VoxDocs page its `[Documentation]` names, by path or id, with a
    /// heading after a hash if any.
    #[serde(default)]
    pub documentation: Option<String>,
    /// Base classes' fields first, then its own, in the order declared.
    pub fields: Vec<ScriptField>,
    /// Its `[Button]` methods, which the Inspector shows under its fields.
    #[serde(default)]
    pub buttons: Vec<ScriptButton>,
    /// Whether it draws gizmos: it overrides `DrawGizmos`.
    #[serde(default)]
    pub gizmos: bool,
}

/// A `[Button]` method, which takes nothing, and what its button says, or
/// empty for the method's name in words.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ScriptButton {
    pub method: String,
    #[serde(default)]
    pub label: String,
}

impl BehaviourType {
    /// The class name without its namespace.
    pub fn short_name(&self) -> &str {
        self.name.rsplit(['.', '+']).next().unwrap_or(&self.name)
    }

    /// The field a scene's key names: keys match ignoring case and
    /// underscores, as `BehaviourFields` sets them.
    pub fn field(&self, key: &str) -> Option<&ScriptField> {
        self.fields.iter().find(|f| same_field(&f.name, key))
    }

    /// Whether a field of type `name` can point at this behaviour: it is
    /// that class, derives from it, or implements it.
    pub fn is_a(&self, name: &str) -> bool {
        self.name == name || self.bases.iter().any(|b| b == name) || self.interfaces.iter().any(|i| i == name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct NestedType {
    pub fields: Vec<ScriptField>,
}

/// One field a scene can set.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct ScriptField {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    /// What it starts as, as a scene writes it; `None` for nothing, as a
    /// reference starts.
    #[serde(default, deserialize_with = "toml_value")]
    pub default: Option<toml::Value>,
    /// What its inspector attributes ask of the Inspector.
    #[serde(default)]
    pub inspector: FieldInspector,
    /// For a C# component's field, where it lies in the struct's bytes, so
    /// the editor can read and write it in a running game.
    #[serde(default)]
    pub layout: Option<FieldLayout>,
}

/// Where a component's field lies: its offset and size in bytes, and
/// whether a whole number (or an enum's value) is signed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct FieldLayout {
    pub offset: u32,
    pub size: u32,
    #[serde(default)]
    pub signed: bool,
}

/// What a field's inspector attributes ask for, as Odin Inspector's do (see
/// `Voxear.Core`'s `Inspector` folder); all empty for a field with none.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct FieldInspector {
    pub group: Option<FieldGroup>,
    /// A title and line above the field.
    pub title: Option<String>,
    /// Lower first; fields without one are 0, in the order declared.
    pub order: i32,
    pub indent: u32,
    /// Words to label it with instead of its name.
    pub label: Option<String>,
    pub hide_label: bool,
    pub show_if: Option<FieldCondition>,
    pub hide_if: Option<FieldCondition>,
    pub enable_if: Option<FieldCondition>,
    pub disable_if: Option<FieldCondition>,
    pub read_only: bool,
    /// Kept out of the Inspector.
    pub hidden: bool,
    pub range: Option<[f64; 2]>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// A unit after the value, such as "m/s".
    pub suffix: Option<String>,
    /// Lines a text field shows.
    pub multiline: Option<u32>,
    /// A bar from the first value to the second.
    pub progress: Option<[f64; 2]>,
    /// An enum as a row of buttons.
    pub enum_buttons: bool,
    /// A preview of the referenced asset, this many pixels.
    pub preview: Option<u32>,
    /// The field must be set: what the error says, or empty for the usual.
    pub required: Option<String>,
    pub info: Vec<InfoBox>,
    pub tooltip: Option<String>,
    /// The VoxDocs page `[Documentation]` names, for a book button beside it.
    pub documentation: Option<String>,
    /// The VoxDocs page, or section of one, `[Excerpt]` shows above it.
    pub excerpt: Option<String>,
    /// A method `[OnValueChanged]` calls when the field is changed.
    pub on_changed: Option<String>,
    /// A member `[ValueDropdown]` asks for the values to offer.
    pub dropdown: Option<String>,
    /// A method `[ValidateInput]` asks whether the value is right.
    pub validate: Option<FieldValidate>,
    /// A list of `[Serializable]` values shown as a table, `[TableList]`.
    pub table: bool,
    /// How a list shows its items, `[ListDrawerSettings]`.
    pub list: Option<ListSettings>,
    /// Colours `[ColorPalette]` offers, as hex.
    pub palette: Vec<String>,
    /// The drawers scripts draw its value with, by type, outermost first:
    /// an `AttributeDrawer` for each of its attributes that has one, then the
    /// `ValueDrawer` of its type.
    pub drawers: Vec<String>,
    /// For a list, the drawers of each of its items.
    pub item_drawers: Vec<String>,
    /// A handle that edits it in the Scene view, as `[PositionHandle]` and
    /// the others ask.
    pub handle: Option<FieldHandle>,
}

/// A Scene view handle on a field (`[PositionHandle]`, `[RadiusHandle]`,
/// `[DirectionHandle]`, `[BoundsHandle]`, `[ArcHandle]` and
/// `[SliderHandle]` in Voxear.Core), drawn for the selected Obj.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FieldHandle {
    pub kind: HandleKind,
    /// A point or direction in the world's space, rather than the Obj's own.
    #[serde(default)]
    pub world: bool,
    /// For a slider, which of the Obj's axes: 0, 1 or 2 for X, Y or Z.
    #[serde(default)]
    pub axis: usize,
    /// For an arc, how far from the Obj it is drawn, in metres.
    #[serde(default)]
    pub radius: Option<f32>,
    /// Its colour as hex, or none for the usual.
    #[serde(default)]
    pub color: Option<String>,
}

/// What a field's Scene view handle edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HandleKind {
    /// A Vector3 point, or each point of a list of them.
    Position,
    /// A float radius around the Obj.
    Radius,
    /// A Vector3 direction from the Obj, with its length.
    Direction,
    /// A Vector3 size of a box around the Obj.
    Bounds,
    /// A float angle in degrees, open around the Obj's forward.
    Arc,
    /// A float distance along one of the Obj's axes.
    Slider,
}

/// A `[ListDrawerSettings]`: how many items show at a time, 0 for all, and
/// whether each is labelled by its index, rather than not at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ListSettings {
    pub per_page: usize,
    pub index_labels: bool,
}

/// A `[ValidateInput]`: the method to ask, and what the error says when it
/// answers false, or empty for the usual.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct FieldValidate {
    pub method: String,
    pub message: String,
}

/// A group a field shows in, by name: a foldout, a box, a row, or a tab of
/// the group's tabs.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FieldGroup {
    pub kind: GroupKind,
    pub name: String,
    #[serde(default)]
    pub tab: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    Foldout,
    Box,
    Horizontal,
    Tab,
}

/// Another field of the same behaviour, and the value it is compared with:
/// with none, the condition holds while the field is true, non-zero, not
/// empty or set.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FieldCondition {
    pub member: String,
    #[serde(default, deserialize_with = "toml_value")]
    pub value: Option<toml::Value>,
}

/// A message shown in a box above a field, while `visible_if` holds when it
/// is given.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InfoBox {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: InfoKind,
    #[serde(default)]
    pub visible_if: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfoKind {
    Info,
    Warning,
    Error,
}

/// The kind of value a field takes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum FieldKind {
    /// A `float` or `double`.
    Number,
    /// An integer of any size.
    Whole,
    Bool,
    Text,
    Enum {
        names: Vec<String>,
    },
    /// A `Vector2`, `Vector3` or `Vector4`.
    Vector {
        size: usize,
    },
    /// A `Quaternion`, which scenes write as Euler degrees.
    Rotation,
    /// A `Color`, in sRGB with alpha, which scenes write as `"#ff8800"` or
    /// as three or four numbers from 0 to 1.
    Color,
    Obj,
    /// A behaviour on another Obj, of this type or one deriving from or
    /// implementing it.
    Behaviour {
        name: String,
    },
    /// An asset loaded with the scene, such as a `Prefab`, by its type.
    Asset {
        asset: String,
    },
    /// A `Ref<T>`: an asset's path, loaded when the script asks.
    Ref {
        asset: String,
    },
    List {
        element: Box<FieldKind>,
    },
    /// A `[Serializable]` type, written as a table of its fields.
    Struct {
        name: String,
    },
    /// A type scenes cannot set.
    Unsupported {
        name: String,
    },
}

/// A field's name as a label, as Unity shows it: `degreesPerSecond` is
/// "Degrees Per Second", and `rare_chance` "Rare Chance".
pub fn nice_name(name: &str) -> String {
    let mut words = String::new();
    let mut previous: Option<char> = None;
    for c in name.trim_start_matches('_').chars() {
        if c == '_' {
            previous = Some(' ');
            continue;
        }
        let starts_word = previous.is_some_and(|p| {
            p == ' ' || (c.is_uppercase() && p.is_lowercase()) || (c.is_ascii_digit() && !p.is_ascii_digit())
        });
        if starts_word {
            words.push(' ');
        }
        let first = words.is_empty() || words.ends_with(' ');
        words.extend(if first { c.to_uppercase().collect::<Vec<_>>() } else { vec![c] });
        previous = Some(c);
    }
    words
}

/// Whether two field names are the same to a scene: ignoring case and
/// underscores, so `speed_limit` sets `SpeedLimit`.
pub fn same_field(a: &str, b: &str) -> bool {
    let plain = |name: &str| name.chars().filter(|&c| c != '_').flat_map(char::to_lowercase).collect::<String>();
    plain(a) == plain(b)
}

/// Where the description of the project's scripts is kept.
pub fn description_path(project: &Project) -> Option<PathBuf> {
    Some(project.scripts_assembly(scripts::CONFIGURATION)?.parent()?.join(DESCRIPTION))
}

/// Where the Code Reference is kept, written with the description.
pub fn code_path(project: &Project) -> Option<PathBuf> {
    Some(description_path(project)?.with_file_name(CODE))
}

/// The scripts as last described, if they have been.
pub fn cached(project: &Project) -> Option<ScriptTypes> {
    let text = std::fs::read_to_string(description_path(project)?).ok()?;
    parse(&text).ok()
}

/// Whether a script or the scripts project changed since the description
/// was written, or there is none. A project without scripts never is.
pub fn is_stale(project: &Project) -> bool {
    if project.scripts_project().is_none() {
        return false;
    }
    let written = description_path(project).and_then(|path| modified(&path));
    match (written, newest_source(project)) {
        (Some(written), Some(newest)) => newest > written,
        _ => true,
    }
}

/// Whether a script or the scripts project changed after `time`.
pub fn changed_since(project: &Project, time: SystemTime) -> bool {
    newest_source(project).is_some_and(|newest| newest > time)
}

fn newest_source(project: &Project) -> Option<SystemTime> {
    let csproj = project.scripts_project()?;
    let mut sources = vec![csproj.clone()];
    if let Some(folder) = csproj.parent() {
        source_files(folder, &mut sources);
    }
    sources.iter().filter_map(|path| modified(path)).max()
}

/// Builds the scripts and describes what the build holds. Takes seconds, so
/// the editor runs it off its main thread. Empty for a project without
/// scripts.
pub fn describe(project: &Project) -> Result<ScriptTypes> {
    if project.scripts_project().is_none() {
        return Ok(ScriptTypes::default());
    }
    // Held until the description is written, so two at once cannot mix their files.
    let _building = scripts::lock();
    scripts::build_unlocked(project)?;
    let assembly = project.scripts_assembly(scripts::CONFIGURATION).context("scripts project has no file name")?;
    let tool = core_assembly(scripts::CONFIGURATION).with_file_name("Voxear.ScriptInfo.dll");
    ensure!(tool.is_file(), "{} is missing; build managed/Voxear.slnx first", tool.display());
    let out = description_path(project).context("scripts project has no file name")?;
    let code = code_path(project).context("scripts project has no file name")?;
    let dotnet = scripts::dotnet_exe()?;
    let mut command = Command::new(&dotnet);
    command.arg(&tool).arg(&assembly).arg(&out).arg(&code).env("DOTNET_NOLOGO", "1");
    // The tool makes each behaviour to read its starting values, which runs the scripts' own constructors.
    let output = scripts::output_within(&mut command, DESCRIBE_LIMIT).context("describing the scripts")?;
    if !output.status.success() {
        for line in String::from_utf8_lossy(&output.stderr).lines().filter(|l| !l.trim().is_empty()) {
            tracing::error!("{line}");
        }
    }
    ensure!(output.status.success(), "describing the scripts failed");
    let text = std::fs::read_to_string(&out).with_context(|| format!("reading {}", out.display()))?;
    parse(&text)
}

pub fn parse(text: &str) -> Result<ScriptTypes> {
    serde_json::from_str(text).context("reading the scripts' description")
}

/// The `.cs` files in a folder and those inside it, leaving out build output.
fn source_files(folder: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() && !matches!(name.as_str(), "bin" | "obj") && !name.starts_with('.') {
            source_files(&path, files);
        } else if name.ends_with(".cs") {
            files.push(path);
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A JSON value as TOML, which has no null: `null` is `None`, and inside a
/// list it is an empty table, which reads as no Obj.
fn toml_value<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<toml::Value>, D::Error> {
    let json = serde_json::Value::deserialize(deserializer)?;
    Ok(from_json(&json))
}

fn from_json(json: &serde_json::Value) -> Option<toml::Value> {
    use serde_json::Value as Json;
    Some(match json {
        Json::Null => return None,
        Json::Bool(b) => toml::Value::Boolean(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) if !n.to_string().contains(['.', 'e', 'E']) => toml::Value::Integer(i),
            _ => toml::Value::Float(n.as_f64()?),
        },
        Json::String(s) => toml::Value::String(s.clone()),
        Json::Array(items) => toml::Value::Array(
            items
                .iter()
                .map(|item| from_json(item).unwrap_or_else(|| toml::Value::Table(toml::Table::new())))
                .collect(),
        ),
        Json::Object(table) => {
            toml::Value::Table(table.iter().filter_map(|(k, v)| Some((k.clone(), from_json(v)?))).collect())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_read_with_kinds_and_starting_values() {
        let text = r#"{
            "behaviours": [{
                "name": "Hello.Boss", "bases": ["Hello.Enemy"], "interfaces": ["Hello.IHurt"],
                "fields": [
                    { "name": "health", "type": { "kind": "whole" }, "default": 10 },
                    { "name": "rage", "type": { "kind": "number" }, "default": 0.5 },
                    { "name": "lamps", "type": { "kind": "list", "element": { "kind": "obj" } }, "default": [null] },
                    { "name": "wave", "type": { "kind": "struct", "name": "Hello.Wave" }, "default": { "count": 2, "who": null } },
                    { "name": "mood", "type": { "kind": "enum", "names": ["Calm", "Angry"] }, "default": "Calm" }
                ]
            }],
            "types": { "Hello.Wave": { "fields": [{ "name": "count", "type": { "kind": "whole" }, "default": 0 }] } }
        }"#;
        let types = parse(text).unwrap();
        let boss = types.behaviour("Hello.Boss").unwrap();
        assert!(boss.is_a("Hello.Enemy") && boss.is_a("Hello.IHurt") && !boss.is_a("Hello.Spinner"));
        assert_eq!(boss.field("Health").unwrap().default, Some(toml::Value::Integer(10)));
        assert_eq!(boss.fields[1].default, Some(toml::Value::Float(0.5)));
        let lamps = &boss.fields[2];
        assert_eq!(lamps.kind, FieldKind::List { element: Box::new(FieldKind::Obj) });
        assert_eq!(lamps.default, Some(toml::Value::Array(vec![toml::Value::Table(toml::Table::new())])));
        let wave = boss.fields[3].default.as_ref().unwrap().as_table().unwrap();
        assert!(wave.contains_key("count") && !wave.contains_key("who"), "nothing is left out");
        assert_eq!(types.fields_of("Hello.Wave")[0].name, "count");
    }

    /// Needs .NET and the managed build, as play mode does.
    #[test]
    fn hellos_behaviours_are_described_from_their_build() {
        if voxear_scripting::find_dotnet().is_none() || !core_assembly(scripts::CONFIGURATION).is_file() {
            return eprintln!("skipping: needs .NET and the managed build");
        }
        let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/Hello");
        let types = describe(&Project::open(&hello).unwrap()).unwrap();
        let spawner = types.behaviour("Hello.FireflySpawner").expect("FireflySpawner is described");
        let fields: Vec<&str> = spawner.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(fields, ["firefly", "rareFirefly", "lamps", "interval", "spread", "rareChance"]);
        assert_eq!(spawner.fields[0].kind, FieldKind::Asset { asset: "Voxear.Prefab".into() });
        assert_eq!(spawner.fields[3].default, Some(toml::Value::Float(0.25)));
        let manager = types.behaviour("Hello.GameManager").unwrap();
        assert_eq!(
            manager.fields[0].kind,
            FieldKind::List { element: Box::new(FieldKind::Struct { name: "Hello.LevelStop".into() }) }
        );
        assert_eq!(types.fields_of("Hello.LevelStop").len(), 2);
        assert!(cached(&Project::open(&hello).unwrap()).is_some(), "and the description is kept");

        // The Code Reference beside it: the scripts' public types and the engine's.
        let code = std::fs::read_to_string(code_path(&Project::open(&hello).unwrap()).unwrap()).unwrap();
        let code: serde_json::Value = serde_json::from_str(&code).unwrap();
        let keys: Vec<&str> = code["types"].as_array().unwrap().iter().filter_map(|t| t["key"].as_str()).collect();
        assert!(keys.contains(&"Hello.Spinner") && keys.contains(&"Voxear.Prefab"), "{keys:?}");
    }
}
