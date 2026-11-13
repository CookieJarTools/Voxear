//! What a running editor answers over its link, as `vox editor call` and
//! `vox mcp` reach it: every call, what it takes and what it gives back
//! ([`CALLS`]), and how Objs, assets, components, fields and their values
//! are written as JSON. The calls themselves run in `editor::link_calls`.
//!
//! Nothing here names a component, a field or an asset kind by hand beyond
//! the lists the Inspector and the Project panel already keep: a component
//! Add Component offers, a field the Inspector shows and a file the Project
//! panel makes are reached the same way, so what is added to those is
//! reached too. [`every_part`] and [`asset_kind_id`] match their enums
//! whole, so a new component or asset kind does not build until it says
//! how a tool reaches it.

use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use voxear_assets::{AssetGuid, AssetKind};
use voxear_ecs::Entity;
use voxear_engine::{
    Game,
    local_link::{ErrorKind, LinkError},
    prefab::ObjPath,
    script_types::ScriptTypes,
};

use crate::{
    fields::{Pick, Step},
    props::{self, Part, Prop, Value},
};

/// The file in a project's `.voxear` folder that a running editor's link is
/// found by.
pub const LINK_FILE: &str = "Editor";

/// The folder a project's link file is in.
pub fn link_folder(project_root: &Path) -> PathBuf {
    project_root.join(".voxear")
}

/// What an argument holds, as `--describe` tells a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// An Obj: `{ "scene": "<guid>", "path": "3" }`, or `"<guid>:3"`.
    Obj,
    /// A list of Objs.
    Objs,
    /// An asset: `{ "guid": "…" }`, `{ "path": "…" }`, or either as text.
    Asset,
    /// A component kind, as `components` lists them: `light`, or a script's
    /// full type name.
    Component,
    /// A field of a component, as `obj.get` lists them: `intensity`, or a
    /// script field's path, `waves.2.count`.
    Field,
    /// A field's value, as `obj.get` shows it for that field.
    Value,
    Text,
    Number,
    Whole,
    Toggle,
    /// Three numbers.
    Vector,
    /// One of these words.
    Choice(&'static [&'static str]),
    /// Calls, each `{ "call": "…", "args": { … } }`.
    Calls,
}

impl ArgKind {
    fn describe(self) -> Json {
        match self {
            ArgKind::Choice(options) => json!({ "type": "choice", "options": options }),
            other => json!({ "type": match other {
                ArgKind::Obj => "obj",
                ArgKind::Objs => "objs",
                ArgKind::Asset => "asset",
                ArgKind::Component => "component",
                ArgKind::Field => "field",
                ArgKind::Value => "value",
                ArgKind::Text => "text",
                ArgKind::Number => "number",
                ArgKind::Whole => "whole",
                ArgKind::Toggle => "toggle",
                ArgKind::Vector => "vector",
                ArgKind::Calls => "calls",
                ArgKind::Choice(_) => unreachable!(),
            } }),
        }
    }
}

/// One argument of a call.
#[derive(Clone, Copy, Debug)]
pub struct ArgInfo {
    pub name: &'static str,
    pub kind: ArgKind,
    pub required: bool,
    pub about: &'static str,
}

/// One call: its name, what it does, its arguments, whether it changes the
/// project (and so is one step to undo, and refused when tools may only
/// read), and what it gives back.
#[derive(Clone, Copy, Debug)]
pub struct CallInfo {
    pub name: &'static str,
    pub about: &'static str,
    pub args: &'static [ArgInfo],
    pub changes: bool,
    pub returns: &'static str,
}

const fn arg(name: &'static str, kind: ArgKind, about: &'static str) -> ArgInfo {
    ArgInfo { name, kind, required: true, about }
}

const fn maybe(name: &'static str, kind: ArgKind, about: &'static str) -> ArgInfo {
    ArgInfo { name, kind, required: false, about }
}

/// The views `capture` takes.
pub const VIEWS: &[&str] = &["scene", "game", "editor"];
/// The levels `console.read` keeps to.
pub const LEVELS: &[&str] = &["info", "warning", "error"];

/// Every call the editor answers, easy ones first.
pub const CALLS: &[CallInfo] = &[
    CallInfo {
        name: "editor.state",
        about: "What the editor has open: the project, its level and scenes, whether it plays, unsaved edits, the selection and how many steps there are to undo.",
        args: &[],
        changes: false,
        returns: "{ project, level, scenes, active_scene, playing, paused, unsaved, selection, undo, redo, tools_may_change, headless }",
    },
    CallInfo {
        name: "objs.list",
        about: "The open scenes' Objs in Hierarchy order, each with its parent, or those whose name has `name` in it, that have `component`, or that are under `under`.",
        args: &[
            maybe("name", ArgKind::Text, "Part of a name, any case."),
            maybe("component", ArgKind::Component, "Only Objs with this component."),
            maybe("under", ArgKind::Obj, "Only Objs under this one."),
        ],
        changes: false,
        returns: "{ objs: [{ obj, name, parent, depth, scene, components }] }",
    },
    CallInfo {
        name: "obj.get",
        about: "One Obj: its name, Transform, whether it is on, and each component with every field the Inspector shows, its value, kind and unit.",
        args: &[arg("obj", ArgKind::Obj, "The Obj.")],
        changes: false,
        returns: "{ obj, name, parent, active, persistent, position, rotation, scale, components: [{ component, title, enabled, fields: [{ field, label, type, value, … }] }] }",
    },
    CallInfo {
        name: "components",
        about: "Every component Add Component can put on an Obj: the built-in ones and the scripts' behaviours and components, with the scripts' fields.",
        args: &[],
        changes: false,
        returns: "{ components: [{ component, title, folder, module, script, fields? }] }",
    },
    CallInfo {
        name: "obj.create",
        about: "Makes an Obj: one of the New Object kinds by `kind`, an instance of `prefab`, or a `model` placed whole. Under `parent`, or at the top of `scene` or the active scene, at `position`.",
        args: &[
            maybe("kind", ArgKind::Text, "A New Object kind, as `kinds` lists them: Empty, Cube, Point Light…"),
            maybe("prefab", ArgKind::Asset, "A prefab to make an instance of."),
            maybe("model", ArgKind::Asset, "A model file to place whole."),
            maybe("parent", ArgKind::Obj, "The Obj it goes under."),
            maybe("scene", ArgKind::Asset, "The open scene it goes into."),
            maybe("position", ArgKind::Vector, "Where, in metres; under a parent, from the parent."),
            maybe("name", ArgKind::Text, "Its name, in place of the kind's."),
        ],
        changes: true,
        returns: "{ obj }",
    },
    CallInfo {
        name: "kinds",
        about: "The New Object kinds `obj.create` makes, as the Hierarchy's Create menu lists them, and the file kinds `asset.create` makes.",
        args: &[],
        changes: false,
        returns: "{ objs: [text], assets: [text] }",
    },
    CallInfo {
        name: "obj.set",
        about: "Changes an Obj's own values: its name, where it is (from its parent), its rotation in degrees, its scale, whether it is on, and whether it stays through level changes.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            maybe("name", ArgKind::Text, ""),
            maybe("position", ArgKind::Vector, "Metres."),
            maybe("rotation", ArgKind::Vector, "Degrees about x, y and z, as the scene file writes them."),
            maybe("scale", ArgKind::Vector, ""),
            maybe("active", ArgKind::Toggle, "On or off, and everything under it."),
            maybe("persistent", ArgKind::Toggle, "Stays through level changes."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "obj.duplicate",
        about: "Copies an Obj and everything under it, as Ctrl+D does.",
        args: &[arg("obj", ArgKind::Obj, "The Obj.")],
        changes: true,
        returns: "{ obj }",
    },
    CallInfo {
        name: "obj.delete",
        about: "Deletes an Obj and everything under it. Undo brings them back with the same references.",
        args: &[arg("obj", ArgKind::Obj, "The Obj.")],
        changes: true,
        returns: "{}",
    },
    CallInfo {
        name: "obj.move",
        about: "Moves an Obj in the Hierarchy, keeping where it is in the world: `into` another, just `before` or `after` one, or to the top of `scene`.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            maybe("into", ArgKind::Obj, ""),
            maybe("before", ArgKind::Obj, ""),
            maybe("after", ArgKind::Obj, ""),
            maybe("scene", ArgKind::Asset, ""),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "component.add",
        about: "Adds a component to an Obj, as Add Component does.",
        args: &[arg("obj", ArgKind::Obj, "The Obj."), arg("component", ArgKind::Component, "")],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "component.remove",
        about: "Takes a component off an Obj.",
        args: &[arg("obj", ArgKind::Obj, "The Obj."), arg("component", ArgKind::Component, "")],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "component.enable",
        about: "Switches a component on or off, as its tick in the Inspector does.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("enabled", ArgKind::Toggle, ""),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "field.set",
        about: "Sets one field of a component, as typing or picking it in the Inspector does. Numbers, text, toggles and vectors as JSON; a colour as [r, g, b] from 0 to 1 or \"#rrggbb\"; a choice by its word; an Obj or asset as a reference, or null for none; a curve as [[where, value], …].",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("field", ArgKind::Field, ""),
            arg("value", ArgKind::Value, ""),
        ],
        changes: true,
        returns: "{ changed, value }",
    },
    CallInfo {
        name: "field.reset",
        about: "Takes a script field back to what the script starts it as.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("field", ArgKind::Field, ""),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "field.choices",
        about: "What a reference or choice field may be set to, as its menu offers.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("field", ArgKind::Field, ""),
        ],
        changes: false,
        returns: "{ choices: [{ label, value }] }",
    },
    CallInfo {
        name: "list.add",
        about: "Adds an item to the end of a script's list field.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("field", ArgKind::Field, "The list."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "list.remove",
        about: "Takes an item out of a script's list field, by the item's own field path, as `waves.2`.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("component", ArgKind::Component, ""),
            arg("field", ArgKind::Field, "The item."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "undo",
        about: "Takes back the last step, as Ctrl+Z does: a tool's change, or the person's.",
        args: &[],
        changes: true,
        returns: "{ undone, undo, redo }",
    },
    CallInfo {
        name: "redo",
        about: "Makes the last undone step again.",
        args: &[],
        changes: true,
        returns: "{ redone, undo, redo }",
    },
    CallInfo {
        name: "batch",
        about: "Makes several calls in order as one step to undo. Stops at the first that fails, keeping what was done before it.",
        args: &[arg("calls", ArgKind::Calls, "")],
        changes: true,
        returns: "{ results: [answer], failed? }",
    },
    CallInfo {
        name: "select",
        about: "Selects Objs, the first the active one, or nothing for an empty list.",
        args: &[arg("objs", ArgKind::Objs, "")],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "frame",
        about: "Moves the Scene view's camera to show Objs, or the selection, as F does.",
        args: &[maybe("objs", ArgKind::Objs, "")],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "commands",
        about: "Every command the command palette runs: the editor's own, those code adds, and the scripts' [Command] methods.",
        args: &[],
        changes: false,
        returns: "{ commands: [{ id, title, keys }] }",
    },
    CallInfo {
        name: "command.run",
        about: "Runs a command palette command by its id, as picking it does.",
        args: &[arg("id", ArgKind::Text, "As `commands` lists it, as `command:Save` or `script:Game.Tools.Rebuild`.")],
        changes: true,
        returns: "{}",
    },
    CallInfo {
        name: "save",
        about: "Writes the open scenes that changed back to their files, as Ctrl+S does.",
        args: &[],
        changes: true,
        returns: "{ saved: [path] }",
    },
    CallInfo {
        name: "level.open",
        about: "Opens a level, or a scene on its own, to edit. Refused with edits not saved, unless `discard`.",
        args: &[arg("level", ArgKind::Asset, "A .level or .scene."), maybe("discard", ArgKind::Toggle, "Lose edits not saved.")],
        changes: true,
        returns: "{ level }",
    },
    CallInfo {
        name: "scene.open_alongside",
        about: "Opens a scene alongside those open, as Open Scene Additive does.",
        args: &[arg("scene", ArgKind::Asset, "")],
        changes: true,
        returns: "{}",
    },
    CallInfo {
        name: "scene.reload",
        about: "Reads the open scenes again from their files, as after a tool wrote them. Refused with edits not saved, unless `discard`.",
        args: &[maybe("discard", ArgKind::Toggle, "Lose edits not saved.")],
        changes: true,
        returns: "{ reloaded: [path] }",
    },
    CallInfo {
        name: "files.look",
        about: "Looks at the project's files at once for what a tool changed: new and changed assets, scripts, voxear.toml, and the open scenes.",
        args: &[],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "assets.list",
        about: "The project's assets and its packages', each with its GUID, path and kind.",
        args: &[
            maybe("kind", ArgKind::Text, "Only this kind, as `scene` or `material`."),
            maybe("folder", ArgKind::Text, "Only under this folder."),
        ],
        changes: false,
        returns: "{ assets: [{ guid, path, kind }] }",
    },
    CallInfo {
        name: "asset.get",
        about: "One asset: its GUID, path and kind, and for a text file its content, as JSON when it is TOML.",
        args: &[arg("asset", ArgKind::Asset, "")],
        changes: false,
        returns: "{ guid, path, kind, file, content?, text? }",
    },
    CallInfo {
        name: "asset.create",
        about: "Makes a new file as the Project panel's Create menu does, with a name not yet taken.",
        args: &[
            arg("kind", ArgKind::Text, "As `kinds` lists them: Scene, Material, C# Script…"),
            maybe("folder", ArgKind::Text, "The folder, from the project's; the top when left out."),
        ],
        changes: true,
        returns: "{ asset }",
    },
    CallInfo {
        name: "asset.rename",
        about: "Renames an asset, keeping its GUID, so what uses it still does.",
        args: &[arg("asset", ArgKind::Asset, ""), arg("name", ArgKind::Text, "The new name, without the extension.")],
        changes: true,
        returns: "{ asset }",
    },
    CallInfo {
        name: "asset.move",
        about: "Moves an asset into another folder, keeping its GUID.",
        args: &[arg("asset", ArgKind::Asset, ""), arg("folder", ArgKind::Text, "")],
        changes: true,
        returns: "{ asset }",
    },
    CallInfo {
        name: "asset.duplicate",
        about: "Copies an asset beside itself, with a GUID of its own.",
        args: &[arg("asset", ArgKind::Asset, "")],
        changes: true,
        returns: "{ asset }",
    },
    CallInfo {
        name: "asset.delete",
        about: "Moves an asset to the desktop's trash, where it can be got back.",
        args: &[arg("asset", ArgKind::Asset, "")],
        changes: true,
        returns: "{ trashed }",
    },
    CallInfo {
        name: "play",
        about: "Starts playing, as the Play button does, and answers once the game runs or fails to start; the scripts build first when they changed.",
        args: &[],
        changes: false,
        returns: "{ playing }",
    },
    CallInfo {
        name: "pause",
        about: "Pauses or goes on playing.",
        args: &[maybe("paused", ArgKind::Toggle, "Left out, it flips.")],
        changes: false,
        returns: "{ paused }",
    },
    CallInfo {
        name: "step",
        about: "Plays one frame while paused.",
        args: &[],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "stop",
        about: "Stops playing; the level is as it was before Play.",
        args: &[],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "game.console",
        about: "Runs a line in the console of the game playing in the editor, as typing it there does, and gives back what it logged.",
        args: &[arg("line", ArgKind::Text, "")],
        changes: false,
        returns: "{ lines: [text] }",
    },
    CallInfo {
        name: "console.read",
        about: "The Console's lines after `after` (a number a read gave back as `next`), at `level` or worse.",
        args: &[
            maybe("after", ArgKind::Whole, "From an earlier read's `next`; the lines kept when left out."),
            maybe("level", ArgKind::Choice(LEVELS), ""),
            maybe("limit", ArgKind::Whole, "The most lines, the latest; 200 when left out."),
        ],
        changes: false,
        returns: "{ lines: [{ number, level, message, category, seconds, frame, file?, line?, stack?, obj?, asset? }], next }",
    },
    CallInfo {
        name: "capture",
        about: "Saves a picture of the Scene view, the Game view or the whole editor as a PNG, a frame or two from now. The view shows first if it was behind another tab.",
        args: &[
            arg("view", ArgKind::Choice(VIEWS), ""),
            maybe("path", ArgKind::Text, "Where to write it; in the project's .voxear/Captures when left out."),
        ],
        changes: false,
        returns: "{ path, width, height }",
    },
    CallInfo {
        name: "scripts.build",
        about: "Builds the project's C# scripts and waits, giving back the compiler's errors and warnings, as file, line and message.",
        args: &[],
        changes: false,
        returns: "{ ok, problems: [{ file, line, column, severity, code, message }] }",
    },
    CallInfo {
        name: "settings.get",
        about: "The editor's settings, as the Settings panel shows them.",
        args: &[],
        changes: false,
        returns: "{ settings: [{ key, title, group, type, value, options? }] }",
    },
    CallInfo {
        name: "settings.set",
        about: "Changes one of the editor's settings, by its key.",
        args: &[arg("key", ArgKind::Text, ""), arg("value", ArgKind::Value, "")],
        changes: false,
        returns: "{ value }",
    },
    CallInfo {
        name: "project.get",
        about: "The project's settings: its voxear.toml as JSON.",
        args: &[],
        changes: false,
        returns: "{ project }",
    },
    CallInfo {
        name: "docs.search",
        about: "Finds pages of the manual, the packages' docs and the project's own Docs whose title or text has every word asked.",
        args: &[arg("query", ArgKind::Text, ""), maybe("limit", ArgKind::Whole, "20 when left out.")],
        changes: false,
        returns: "{ pages: [{ reference, title }] }",
    },
    CallInfo {
        name: "docs.read",
        about: "A page's Markdown, by its reference from `docs.search`.",
        args: &[arg("reference", ArgKind::Text, "")],
        changes: false,
        returns: "{ title, text }",
    },
    CallInfo {
        name: "editor.wait",
        about: "Answers after some frames, for what takes a frame or two to show.",
        args: &[maybe("frames", ArgKind::Whole, "1 when left out.")],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "editor.quit",
        about: "Closes the editor. Refused with edits not saved, unless `save` or `discard`.",
        args: &[maybe("save", ArgKind::Toggle, "Save first."), maybe("discard", ArgKind::Toggle, "Lose edits not saved.")],
        changes: false,
        returns: "{}",
    },
];

/// The call named so.
pub fn call_info(name: &str) -> Option<&'static CallInfo> {
    CALLS.iter().find(|call| call.name == name)
}

/// Every call as `--describe --json` and the MCP server list them.
pub fn describe_calls() -> Json {
    let calls: Vec<Json> = CALLS
        .iter()
        .map(|call| {
            let args: Vec<Json> = call
                .args
                .iter()
                .map(|arg| {
                    let mut described = arg.kind.describe();
                    described["name"] = json!(arg.name);
                    described["required"] = json!(arg.required);
                    if !arg.about.is_empty() {
                        described["about"] = json!(arg.about);
                    }
                    described
                })
                .collect();
            json!({ "name": call.name, "about": call.about, "args": args, "changes": call.changes, "returns": call.returns })
        })
        .collect();
    json!({ "calls": calls })
}

/// A call refused: shorthand.
pub fn refuse(kind: ErrorKind, message: impl Into<String>) -> LinkError {
    LinkError::new(kind, message)
}

pub fn bad(message: impl Into<String>) -> LinkError {
    refuse(ErrorKind::BadArguments, message)
}

/// An argument, required.
pub fn need<'a>(args: &'a Json, name: &str) -> Result<&'a Json, LinkError> {
    args.get(name).filter(|value| !value.is_null()).ok_or_else(|| bad(format!("`{name}` is missing")))
}

pub fn need_text<'a>(args: &'a Json, name: &str) -> Result<&'a str, LinkError> {
    need(args, name)?.as_str().ok_or_else(|| bad(format!("`{name}` is text")))
}

pub fn opt_text<'a>(args: &'a Json, name: &str) -> Option<&'a str> {
    args.get(name).and_then(Json::as_str)
}

pub fn opt_bool(args: &Json, name: &str) -> Option<bool> {
    args.get(name).and_then(Json::as_bool)
}

pub fn opt_vector(args: &Json, name: &str) -> Result<Option<[f32; 3]>, LinkError> {
    let Some(value) = args.get(name).filter(|value| !value.is_null()) else { return Ok(None) };
    let numbers = numbers(value).filter(|n| n.len() == 3).ok_or_else(|| bad(format!("`{name}` is three numbers")))?;
    Ok(Some([numbers[0], numbers[1], numbers[2]]))
}

fn numbers(value: &Json) -> Option<Vec<f32>> {
    value.as_array()?.iter().map(|n| n.as_f64().map(|n| n as f32)).collect()
}

/// An Obj as a tool names it: by the scene it was loaded from and its path
/// there, which stays the same through undo, Play and saving; an Obj a
/// running game made, which has no place in a scene, by its entity.
pub fn obj_ref(game: &Game, entity: Entity) -> Json {
    match game.levels.place_of(entity) {
        Some((scene, path)) => json!({ "scene": scene.to_string(), "path": path.to_string() }),
        None => json!({ "entity": entity.to_bits() }),
    }
}

/// The Obj a reference names, if it is still there.
pub fn resolve_obj(game: &Game, reference: &Json) -> Result<Entity, LinkError> {
    let missing = || refuse(ErrorKind::NoSuchObj, format!("no Obj is at {reference}"));
    let place = match reference {
        Json::String(text) => {
            let (scene, path) = text.split_once(':').ok_or_else(|| bad(format!("`{text}` is not `<scene guid>:<path>`")))?;
            Some((scene.to_string(), path.to_string()))
        }
        Json::Object(fields) => match (fields.get("scene"), fields.get("path"), fields.get("entity")) {
            (Some(Json::String(scene)), Some(path), _) => {
                let path = path.as_str().map(str::to_string).or_else(|| path.as_u64().map(|id| id.to_string()));
                Some((scene.clone(), path.ok_or_else(|| bad("an Obj's `path` is text, as `3/2`"))?))
            }
            (_, _, Some(Json::Number(bits))) => {
                let entity = Entity::from_bits(bits.as_u64().ok_or_else(missing)?);
                return game.engine.world.is_alive(entity).then_some(entity).ok_or_else(missing);
            }
            _ => None,
        },
        _ => None,
    };
    let (scene, path) = place.ok_or_else(|| bad(format!("{reference} is not an Obj: give {{ \"scene\", \"path\" }}")))?;
    let scene: AssetGuid = scene.parse().map_err(|_| bad(format!("`{scene}` is not a scene's GUID")))?;
    let path = ObjPath::parse(&path).map_err(|error| bad(format!("{error:#}")))?;
    let entity = game.levels.obj_at(scene, &path).ok_or_else(missing)?;
    game.engine.world.is_alive(entity).then_some(entity).ok_or_else(missing)
}

/// An asset as a tool names it, by its GUID and path. A built-in one, as
/// `builtin:cube`, or a part of a model, has a path only.
pub fn asset_ref(game: &Game, path: &str) -> Json {
    match game.content.db().guid_of(path) {
        Some(guid) => json!({ "guid": guid.to_string(), "path": path }),
        None => json!({ "path": path }),
    }
}

/// The path an asset reference names: a GUID found in the project, or a
/// path as it is.
pub fn resolve_asset(game: &Game, reference: &Json) -> Result<String, LinkError> {
    let db = game.content.db();
    let by_guid = |text: &str| {
        let guid: AssetGuid = text.parse().ok()?;
        db.get(guid).map(|entry| entry.path.clone())
    };
    let missing = || refuse(ErrorKind::NoSuchAsset, format!("no asset is at {reference}"));
    match reference {
        Json::String(text) => Ok(by_guid(text).unwrap_or_else(|| text.clone())),
        Json::Object(fields) => match (fields.get("guid").and_then(Json::as_str), fields.get("path").and_then(Json::as_str)) {
            (Some(guid), _) => by_guid(guid).ok_or_else(missing),
            (None, Some(path)) => Ok(path.to_string()),
            _ => Err(bad(format!("{reference} is not an asset: give {{ \"guid\" }} or {{ \"path\" }}"))),
        },
        _ => Err(bad(format!("{reference} is not an asset"))),
    }
}

/// An asset that must be in the project, as an entry of its database.
pub fn resolve_project_asset(game: &Game, reference: &Json) -> Result<(AssetGuid, String, AssetKind), LinkError> {
    let path = resolve_asset(game, reference)?;
    let db = game.content.db();
    let guid = db.guid_of(&path).ok_or_else(|| refuse(ErrorKind::NoSuchAsset, format!("{path} is not in the project")))?;
    let entry = db.get(guid).ok_or_else(|| refuse(ErrorKind::NoSuchAsset, format!("{path} is not in the project")))?;
    Ok((guid, entry.path.clone(), entry.kind))
}

/// Every built-in component, in Add Component's order, with the folder it
/// lists it in. A script's behaviours and components follow from its
/// description.
pub fn every_part() -> Vec<(Part, &'static str)> {
    // Matched whole: a new kind of component does not build until it is listed here.
    fn listed(part: &Part) -> bool {
        match part {
            Part::Renderer
            | Part::Light
            | Part::Camera
            | Part::Audio
            | Part::Listener
            | Part::LodGroup
            | Part::Collider
            | Part::Rigidbody
            | Part::Character
            | Part::Joint
            | Part::Collider2D
            | Part::Rigidbody2D
            | Part::Vehicle
            | Part::Wheel
            | Part::PostVolume
            | Part::WindZone
            | Part::ParticleSystem
            | Part::ForceField
            | Part::Smoke
            | Part::Mist
            | Part::UiPanel
            | Part::ScreenUi
            | Part::Map
            | Part::MapMarker
            | Part::Climate
            | Part::Storms
            | Part::HeatSource
            | Part::HeatZone
            | Part::HeatBody
            | Part::NetObject
            | Part::Animator
            | Part::MobMotion
            | Part::BlockMob
            | Part::MotionMatcher
            | Part::BoneAttachment
            | Part::FirstPersonView
            | Part::Gun
            | Part::GunHolder
            | Part::GunPart
            | Part::Ocean
            | Part::Pool
            | Part::DryArea
            | Part::CaveShape
            | Part::CaveExit
            | Part::Path
            | Part::Stream
            | Part::Buoyancy
            | Part::Seasons
            | Part::FoliageInteraction
            | Part::Foliage
            | Part::Terrain
            | Part::BlockWorld
            | Part::VoxelTerrain
            | Part::TrackMaker => true,
            // A Model Renderer comes with a model placed whole (`obj.create` with `model`), not from Add Component.
            Part::Model => false,
            Part::Behaviour(_) => false,
        }
    }
    let parts = crate::inspector::built_in_parts();
    debug_assert!(parts.iter().all(|(part, _)| listed(part)));
    parts
}

/// A component's name for tools: the name prefabs' locks and scene files
/// give it, and a script's full type name.
pub fn component_id(part: &Part) -> String {
    match part {
        // An Obj's Block World is one, smooth or not; Add Component offers the smooth one apart.
        Part::VoxelTerrain => "voxel_terrain".into(),
        other => other.lock_key(),
    }
}

/// The component a tool names, among the built-in ones and the scripts'.
pub fn resolve_part(types: &ScriptTypes, id: &str) -> Result<Part, LinkError> {
    if let Some(part) = every_part().into_iter().map(|(part, _)| part).find(|part| component_id(part) == id) {
        return Ok(part);
    }
    if id == "model" {
        return Ok(Part::Model);
    }
    let script = types.behaviours.iter().chain(&types.components).find(|script| script.name == id);
    match script {
        Some(script) => Ok(Part::Behaviour(script.name.clone())),
        None => Err(refuse(ErrorKind::NoSuchComponent, format!("`{id}` is no component; `components` lists them"))),
    }
}

/// The component of an Obj a tool names: one it has.
pub fn part_on(game: &Game, entity: Entity, id: &str) -> Result<Part, LinkError> {
    let parts = props::parts(&game.engine.world, entity);
    parts
        .into_iter()
        .find(|part| component_id(part) == id || (id == "voxel_terrain" && *part == Part::BlockWorld))
        .ok_or_else(|| refuse(ErrorKind::NoSuchComponent, format!("the Obj has no `{id}`")))
}

/// Each field of a component on an Obj with the name a tool gives it:
/// a built-in field by the name its locks give it, a script field by its
/// path, and a second field of the same name with `.2` after it.
pub fn named_props(game: &Game, types: &ScriptTypes, entity: Entity, part: &Part) -> Vec<(String, Prop)> {
    let mut named: Vec<(String, Prop)> = Vec::new();
    for prop in props::props(&game.engine.world, entity, part, types) {
        let base = match &prop {
            Prop::Field { path, .. } => path_id(path),
            other => other.lock_key(),
        };
        let taken = named.iter().filter(|(name, _)| *name == base || name.starts_with(&format!("{base}#"))).count();
        let name = if taken == 0 { base } else { format!("{base}#{}", taken + 1) };
        named.push((name, prop));
    }
    named
}

/// A script field's path as one name: `waves.2.count`.
pub fn path_id(path: &[Step]) -> String {
    let steps: Vec<String> = path
        .iter()
        .map(|step| match step {
            Step::Key(key) => key.clone(),
            Step::Item(item) => item.to_string(),
        })
        .collect();
    steps.join(".")
}

/// The field a tool names on a component of an Obj.
pub fn resolve_prop(game: &Game, types: &ScriptTypes, entity: Entity, part: &Part, id: &str) -> Result<Prop, LinkError> {
    named_props(game, types, entity, part).into_iter().find(|(name, _)| name == id).map(|(_, prop)| prop).ok_or_else(|| {
        refuse(ErrorKind::NoSuchField, format!("{} has no field `{id}`; `obj.get` lists them", component_id(part)))
    })
}

/// A field as `obj.get` shows it: its name, label, the kind of value and
/// the value.
pub fn field_json(game: &Game, types: &ScriptTypes, entity: Entity, name: &str, prop: &Prop) -> Json {
    let world = &game.engine.world;
    let (kind, value, extra) = match props::get(game, types, entity, prop) {
        Some(Value::Number(number)) => ("number", json!(number), None),
        Some(Value::Whole(whole)) => ("whole", json!(whole), None),
        Some(Value::Toggle(on)) => ("toggle", json!(on), None),
        Some(Value::Text(text)) => ("text", json!(text), None),
        Some(Value::Vector(numbers)) => ("vector", json!(numbers), None),
        Some(Value::Color(color)) => ("color", json!(color), None),
        Some(Value::Choice(at, options)) => {
            let picked = options.get(at).cloned().unwrap_or_default();
            ("choice", json!(picked), Some(("options", json!(options))))
        }
        Some(Value::Obj(label)) => {
            let target = props::obj_target(world, entity, prop).map(|target| obj_ref(game, target));
            ("obj", target.unwrap_or(Json::Null), Some(("label", json!(label))))
        }
        Some(Value::Asset(label)) => {
            let path = props::asset_path(game, entity, prop).map(|path| asset_ref(game, &path));
            ("asset", path.unwrap_or(Json::Null), Some(("label", json!(label))))
        }
        Some(Value::List(count)) => ("list", json!(count), None),
        Some(Value::Group) => ("group", Json::Null, None),
        Some(Value::Fixed(text)) => ("fixed", json!(text), None),
        Some(Value::Curve(curve)) => ("curve", json!(curve.keys()), None),
        Some(Value::Gradient(gradient)) => ("gradient", json!(gradient.keys()), None),
        None => ("none", Json::Null, None),
    };
    let mut field = json!({ "field": name, "label": props::title(types, prop), "type": kind, "value": value });
    if let Some((key, more)) = extra {
        field[key] = more;
    }
    if let Some(unit) = props::unit_on(world, entity, prop) {
        field["unit"] = json!(unit);
    }
    field
}

/// What a field is set to from JSON, as the value it has now says it is
/// written: a reference is a [`Pick`], anything else a [`Value`].
pub enum Setting {
    Value(Value),
    Pick(Pick),
}

/// Reads a field's new value from JSON, as `now` (its value now) says it is
/// written.
pub fn read_value(game: &Game, now: &Value, json: &Json) -> Result<Setting, LinkError> {
    let wrong = |what: &str| refuse(ErrorKind::WrongValue, format!("{json} is not {what}"));
    Ok(Setting::Value(match now {
        Value::Number(_) => Value::Number(json.as_f64().ok_or_else(|| wrong("a number"))? as f32),
        Value::Whole(_) => {
            let whole = json.as_i64().or_else(|| json.as_f64().filter(|n| n.fract() == 0.0).map(|n| n as i64));
            Value::Whole(whole.ok_or_else(|| wrong("a whole number"))?)
        }
        Value::Toggle(_) => Value::Toggle(json.as_bool().ok_or_else(|| wrong("true or false"))?),
        Value::Text(_) => Value::Text(json.as_str().ok_or_else(|| wrong("text"))?.to_string()),
        Value::Vector(old) => {
            let numbers = numbers(json).filter(|n| n.len() == old.len());
            Value::Vector(numbers.ok_or_else(|| wrong(&format!("{} numbers", old.len())))?)
        }
        Value::Color(_) => Value::Color(color(json).ok_or_else(|| wrong("a colour: [r, g, b] from 0 to 1, or \"#rrggbb\""))?),
        Value::Choice(_, options) => {
            let at = match json {
                Json::String(word) => options.iter().position(|option| option.eq_ignore_ascii_case(word)),
                Json::Number(index) => index.as_u64().map(|i| i as usize).filter(|i| *i < options.len()),
                _ => None,
            };
            Value::Choice(at.ok_or_else(|| wrong(&format!("one of {}", options.join(", "))))?, options.clone())
        }
        Value::Obj(_) => {
            return Ok(Setting::Pick(Pick::Obj(match json {
                Json::Null => None,
                reference => Some(resolve_obj(game, reference)?),
            })));
        }
        Value::Asset(_) => {
            return Ok(Setting::Pick(Pick::Asset(match json {
                Json::Null => None,
                reference => Some(resolve_asset(game, reference)?),
            })));
        }
        Value::Curve(_) => {
            let keys: Option<Vec<[f32; 2]>> = json.as_array().and_then(|keys| {
                keys.iter().map(|key| numbers(key).filter(|k| k.len() == 2).map(|k| [k[0], k[1]])).collect()
            });
            Value::Curve(voxear_core::curve::Curve::new(&keys.ok_or_else(|| wrong("a curve: [[where, value], …]"))?))
        }
        Value::Gradient(_) => {
            let keys: Option<Vec<[f32; 4]>> = json.as_array().and_then(|keys| {
                keys.iter().map(|key| numbers(key).filter(|k| k.len() == 4).map(|k| [k[0], k[1], k[2], k[3]])).collect()
            });
            Value::Gradient(voxear_core::curve::Gradient::new(
                &keys.ok_or_else(|| wrong("a gradient: [[where, r, g, b], …]"))?,
            ))
        }
        Value::List(_) => return Err(refuse(ErrorKind::WrongValue, "a list changes by its items, with `list.add` and `list.remove`")),
        Value::Group => return Err(refuse(ErrorKind::WrongValue, "a group is set field by field")),
        Value::Fixed(_) => return Err(refuse(ErrorKind::WrongValue, "this field is shown, not edited")),
    }))
}

/// A colour from `[r, g, b]` or `"#rrggbb"`.
fn color(json: &Json) -> Option<[f32; 3]> {
    if let Some(hex) = json.as_str().and_then(|text| text.strip_prefix('#')).filter(|hex| hex.len() == 6) {
        let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok().map(|c| f32::from(c) / 255.0);
        return Some([channel(0)?, channel(2)?, channel(4)?]);
    }
    numbers(json).filter(|n| n.len() == 3).map(|n| [n[0], n[1], n[2]])
}

/// A word for each kind of asset, as `assets.list` filters by, and whether
/// its file is text a tool can read.
pub fn asset_kind_id(kind: AssetKind) -> (&'static str, bool) {
    match kind {
        AssetKind::Model => ("model", false),
        AssetKind::Texture => ("texture", false),
        AssetKind::Material => ("material", true),
        AssetKind::Scene => ("scene", true),
        AssetKind::Level => ("level", true),
        AssetKind::Prefab => ("prefab", true),
        AssetKind::Ui => ("ui", true),
        AssetKind::Sound => ("sound", false),
        AssetKind::InputActions => ("inputactions", true),
        AssetKind::Strings => ("strings", true),
        AssetKind::Paths => ("paths", false),
        AssetKind::AssetTable => ("assettable", true),
        AssetKind::LightProfile => ("lightprofile", true),
        AssetKind::Lighting => ("lighting", true),
        AssetKind::LookPack => ("lookpack", true),
        AssetKind::TexturePack => ("texturepack", true),
        AssetKind::Shader => ("shader", true),
        AssetKind::MotionSet => ("motionset", true),
        AssetKind::GunHandling => ("gunhandling", true),
        AssetKind::Data => ("data", true),
        AssetKind::Terrain => ("terrain", true),
        AssetKind::PromptIcons => ("prompticons", true),
        AssetKind::WorldGen => ("worldgen", true),
        AssetKind::BlockWorld => ("blockworld", true),
        AssetKind::Block => ("block", true),
        AssetKind::Biome => ("biome", true),
        AssetKind::Structure => ("structure", true),
        AssetKind::Plant => ("plant", true),
        AssetKind::VoxModel => ("voxmodel", false),
        AssetKind::Particles => ("particles", true),
    }
}

/// A TOML file's content as JSON, for `asset.get`.
pub fn toml_json(text: &str) -> Option<Json> {
    let table: toml::Table = toml::from_str(text).ok()?;
    serde_json::to_value(table).ok()
}

/// The errors and warnings in a C# build's output, as `file(line,col):
/// error CS0103: message` lines are written.
pub fn compiler_problems(output: &str) -> Vec<Json> {
    let mut problems = Vec::new();
    for line in output.lines() {
        let Some((place, rest)) = line.split_once("): ") else { continue };
        let Some((file, at)) = place.rsplit_once('(') else { continue };
        let mut numbers = at.split(',').map(|n| n.trim().parse::<u32>().ok());
        let (Some(Some(line_number)), column) = (numbers.next(), numbers.next().flatten()) else { continue };
        let (head, message) = rest.split_once(": ").unwrap_or((rest, ""));
        let mut words = head.split_whitespace();
        let (Some(severity), code) = (words.next(), words.next()) else { continue };
        if severity != "error" && severity != "warning" {
            continue;
        }
        // The compiler's last word on a project follows its message in brackets.
        let message = message.rsplit_once(" [").map_or(message, |(message, _)| message);
        let problem = json!({
            "file": file.trim(),
            "line": line_number,
            "column": column,
            "severity": severity,
            "code": code.unwrap_or_default(),
            "message": message.trim(),
        });
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_call_is_described_once_with_its_arguments() {
        let mut names: Vec<&str> = CALLS.iter().map(|call| call.name).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "a call is described twice");
        let described = describe_calls();
        assert_eq!(described["calls"].as_array().unwrap().len(), count);
        assert!(CALLS.iter().all(|call| !call.about.is_empty()));
    }

    #[test]
    fn the_manual_names_every_call() {
        let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manual/The Command Line/Driving the Editor from a Terminal.md");
        let text = std::fs::read_to_string(&page).unwrap();
        let missing: Vec<&str> = CALLS.iter().map(|call| call.name).filter(|name| !text.contains(&format!("`{name}`"))).collect();
        assert!(missing.is_empty(), "the manual's page does not name {missing:?}");
    }

    #[test]
    fn compiler_lines_become_problems() {
        let output = "/p/Scripts/Spinner.cs(12,9): error CS0103: The name 'spin' does not exist in the current context [/p/Game.csproj]\n\
                      Build FAILED.\n/p/Scripts/Old.cs(3,1): warning CS0618: 'X' is obsolete";
        let problems = compiler_problems(output);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0]["line"], 12);
        assert_eq!(problems[0]["column"], 9);
        assert_eq!(problems[0]["code"], "CS0103");
        assert_eq!(problems[0]["message"], "The name 'spin' does not exist in the current context");
        assert_eq!(problems[1]["severity"], "warning");
    }

    #[test]
    fn colours_are_read_as_numbers_or_hex() {
        assert_eq!(color(&json!("#ff0080")), Some([1.0, 0.0, 128.0 / 255.0]));
        assert_eq!(color(&json!([0.1, 0.2, 0.3])), Some([0.1, 0.2, 0.3]));
        assert_eq!(color(&json!([0.1, 0.2])), None);
    }
}
