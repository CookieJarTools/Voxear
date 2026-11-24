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
use voxear_ecs::{Entity, World};
use voxear_engine::{
    Game,
    local_link::{ErrorKind, LinkError},
    prefab::ObjPath,
    script_types::ScriptTypes,
};

use crate::{
    fields::{Pick, Step, Target},
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
    /// Two numbers: a place or a size on a graph's canvas.
    Pair,
    /// A list of whole numbers, such as a graph's nodes.
    Wholes,
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
                ArgKind::Pair => "pair",
                ArgKind::Wholes => "wholes",
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
        about: "What the editor has open: the project, its level and scenes, whether it plays, unsaved edits, the selection and how many steps there are to undo. `dotnet_started` says whether .NET runs its scripts yet, which it does from the first Play on, for a debugger to attach.",
        args: &[],
        changes: false,
        returns: "{ project, level, scenes, active_scene, playing, paused, building_scripts, dotnet_started, unsaved, selection, undo, redo, tools_may_change, headless }",
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
        name: "inspector.mode",
        about: "An Inspector's mode, as its ⋯ menu switches it: Normal, or Debug, which shows what is really there: every component as the scene file writes it, the engine's own state, every field of each C# behaviour, private ones too, and while playing, the live state behind the components.",
        args: &[
            maybe("inspector", ArgKind::Whole, "Which Inspector: 1 for the first, the default, 2 for the next…"),
            maybe("mode", ArgKind::Text, "Normal or Debug; left out, only says which it is in."),
        ],
        changes: false,
        returns: "{ inspector, mode }",
    },
    CallInfo {
        name: "inspector.debug",
        about: "What the Inspector's Debug mode shows of an Obj or an asset, row by row: each row's section, label, value, its path in the scene file where it has one, and a note (private, not saved, live…). An asset's are its GUID, kind, .meta, what it depends on and what uses it.",
        args: &[
            maybe("obj", ArgKind::Obj, "The Obj; left out, the one selected."),
            maybe("asset", ArgKind::Asset, "An asset, in place of an Obj."),
        ],
        changes: false,
        returns: "{ rows: [{ section, label, value, path, note }] }",
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
            maybe(
                "position",
                ArgKind::Vector,
                "Where, in metres; under a parent, from the parent. Left out, a kind that stands on the ground (Portals, Car) goes on the ground in the middle of the Scene view.",
            ),
            maybe("name", ArgKind::Text, "Its name, in place of the kind's."),
            maybe(
                "on_ground",
                ArgKind::Toggle,
                "Stands it on the ground under `position`: a mesh, a terrain or a Block World's top, a slab's half a block down.",
            ),
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
            maybe(
                "on_ground",
                ArgKind::Toggle,
                "Stands it on the ground under where it is, or under `position`: a mesh, a terrain or a Block World's top, a slab's half a block down.",
            ),
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
        name: "obj.parent",
        about: "Puts an Obj inside another, or at the top of its scene without `parent`, as a script's Obj.SetParent does, while the game plays too: kept where it is in the world unless `keep_world` is false. Refused inside itself, and while playing, into another scene or dimension.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            maybe("parent", ArgKind::Obj, "The Obj it goes inside; left out, the top of its scene."),
            maybe(
                "keep_world",
                ArgKind::Toggle,
                "Keeps it where it is in the world, as it does left out; false keeps its position, rotation and scale from its new parent.",
            ),
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
        name: "view.get",
        about: "Where the Scene view's camera is and which way it looks, as position and rotation in degrees.",
        args: &[],
        changes: false,
        returns: "{ position, rotation, forward }",
    },
    CallInfo {
        name: "view.set",
        about: "Moves and turns the Scene view's camera: to `position`, turned to `rotation` in degrees or to look at `look_at`.",
        args: &[
            maybe("position", ArgKind::Vector, "Where the camera goes, in metres."),
            maybe("rotation", ArgKind::Vector, "Degrees about x, y and z, as an Obj's rotation."),
            maybe("look_at", ArgKind::Vector, "A point to look at, in place of `rotation`."),
        ],
        changes: false,
        returns: "{ position, rotation, forward }",
    },
    CallInfo {
        name: "blocks.get",
        about: "The block at a point of the world, in metres, in a Block World: `obj`'s, or the first with one there. `shows` is the block it is drawn as where that is another, as grass under a block shows as its Covered block. `sky_light` and `block_light` are the light kept for the block, each from 0 to 15, in a world of cubes.",
        args: &[arg("at", ArgKind::Vector, "Metres, in the world."), maybe("obj", ArgKind::Obj, "The Block World.")],
        changes: false,
        returns: "{ obj, cell, block, shows, sky_light, block_light }",
    },
    CallInfo {
        name: "blocks.set",
        about: "Puts a block at a point of the world, in metres, or air for `null`, in a Block World: `obj`'s, or every one there, as a script's Blocks.Set does. A block the world does not hold yet is added to its blocks.",
        args: &[
            arg("at", ArgKind::Vector, "Metres, in the world."),
            arg("block", ArgKind::Asset, "A .block, or null for air."),
            maybe("obj", ArgKind::Obj, "The Block World."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "blocks.fill",
        about: "Fills the box between two points of the world, in metres, with a block, or air for `null`, as a script's Blocks.Fill does.",
        args: &[
            arg("from", ArgKind::Vector, "One corner, in metres."),
            arg("to", ArgKind::Vector, "The opposite corner, in metres."),
            arg("block", ArgKind::Asset, "A .block, or null for air."),
            maybe("obj", ArgKind::Obj, "The Block World."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "blocks.restore",
        about: "Puts the blocks in the box between two points of the world, in metres, back as its ground was generated, its world layers with it: what was placed or broken by hand there goes. Only where the world has made its chunks. One step to undo.",
        args: &[
            arg("from", ArgKind::Vector, "One corner, in metres."),
            arg("to", ArgKind::Vector, "The opposite corner, in metres."),
            maybe("obj", ArgKind::Obj, "The Block World."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "graph.comments",
        about: "The comments of a graph, a generator (.worldgen) or a VoxShade shader (.voxshade): each its number, its top left corner and size on the canvas, its title and its text.",
        args: &[arg("asset", ArgKind::Asset, "A .worldgen or a .voxshade.")],
        changes: false,
        returns: "{ comments: [{ id, at, size, title, text }] }",
    },
    CallInfo {
        name: "graph.comment.add",
        about: "Adds a comment to a graph: a frame drawn behind its nodes, with a title and a few lines of text. Round `around`'s nodes, as the canvas's Add Comment Round Picked makes one, or at `at`. A step to undo where the graph is open in its panel.",
        args: &[
            arg("asset", ArgKind::Asset, "A .worldgen or a .voxshade."),
            arg("title", ArgKind::Text, ""),
            maybe("text", ArgKind::Text, "Lines under the title."),
            maybe("around", ArgKind::Wholes, "The nodes it frames, by number, as the file numbers them."),
            maybe("at", ArgKind::Pair, "Its top left corner on the canvas; the graph's corner when left out."),
            maybe("size", ArgKind::Pair, "How wide and high it is."),
        ],
        changes: true,
        returns: "{ comment }",
    },
    CallInfo {
        name: "graph.comment.set",
        about: "Changes a comment of a graph: its title, its text, where it is or how large. Moving it does not move the nodes in it, as dragging it does.",
        args: &[
            arg("asset", ArgKind::Asset, "A .worldgen or a .voxshade."),
            arg("comment", ArgKind::Whole, "Its number, as `graph.comments` gives it."),
            maybe("title", ArgKind::Text, ""),
            maybe("text", ArgKind::Text, ""),
            maybe("at", ArgKind::Pair, ""),
            maybe("size", ArgKind::Pair, ""),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "graph.comment.remove",
        about: "Takes a comment out of a graph, leaving its nodes.",
        args: &[
            arg("asset", ArgKind::Asset, "A .worldgen or a .voxshade."),
            arg("comment", ArgKind::Whole, "Its number."),
        ],
        changes: true,
        returns: "{ removed }",
    },
    CallInfo {
        name: "block_world.write_land",
        about: "Writes the land a Block World lays into its generator as it makes ground (its Levels, its biomes' land, the rivers, its Overhangs and Caves) into the generator's graph, as the Inspector's Write Land into Generator does: ordinary nodes laid out under the graph's own, each part in a comment. The world then lays none in itself, so its ground is what it was.",
        args: &[arg("obj", ArgKind::Obj, "The Block World.")],
        changes: true,
        returns: "{ generator, nodes, comments }",
    },
    CallInfo {
        name: "dungeon.starter",
        about: "Makes the starter temple dungeon for a Block World, as the Inspector's Add Temple Dungeon does: its blocks with their pictures in a Dungeon folder beside the world's Blocks, and Structures/Temple Dungeon.structure listed under the world's generator. Nothing a project made already is written over.",
        args: &[arg("obj", ArgKind::Obj, "The Block World.")],
        changes: true,
        returns: "{ structure, changed }",
    },
    CallInfo {
        name: "dungeon.layouts",
        about: "The dungeons a Block World's generator lays out near a point: each one's entrance (its kind, where it stands and the way it faces), its floors, its spaces (kind, floor, middle, size, upper level, depth under its floor, whether on the main path, how far along its floor), its ways (kind, main, the spaces it joins by their place in the list, its middle line) and its lights, all in the level's metres.",
        args: &[
            maybe("obj", ArgKind::Obj, "The Block World; every one when left out."),
            maybe("near", ArgKind::Vector, "Where to look round, in metres; the editor's camera when left out."),
            maybe("reach", ArgKind::Number, "How far round to look, in metres: 500 when left out."),
        ],
        changes: false,
        returns: "{ dungeons: [{ world, structure, site, place, dimension, stage, floors, entrance, spaces, ways, lights }] }",
    },
    CallInfo {
        name: "towns.near",
        about: "The towns a Block World's generator lays out whose sites lie near a point, worked out with none of their ground made: each town's structure, its site's seed, its middle, and its buildings (the middle first, then houses nearest the middle first), each with its structure, where its floor's middle is, its size before turning, how many quarter turns it is turned, its yaw, which way its door faces and the place just outside its door, all in the level's metres.",
        args: &[
            maybe("obj", ArgKind::Obj, "The Block World; every one when left out."),
            maybe("near", ArgKind::Vector, "Where to look round, in metres; the editor's camera when left out."),
            maybe("reach", ArgKind::Number, "How far round to look, in metres: 500 when left out."),
        ],
        changes: false,
        returns: "{ towns: [{ world, structure, site, middle, buildings: [{ structure, middle, at, size, turns, yaw, front, door }] }] }",
    },
    CallInfo {
        name: "dungeon.open",
        about: "Makes the dimension of a dungeon that is in a dimension of its own, as its gate's Open Dungeon does, and shows its way out in the Scene view: to look round it while editing. Its dimension is made by code, as the game makes it, and nothing of it is saved.",
        args: &[
            maybe("site", ArgKind::Value, "The dungeon's site as `dungeon.layouts` gives it, or where it is: [x, z] in metres."),
            maybe("obj", ArgKind::Obj, "A dungeon's gate, the Portal standing in it, in place of its site; or the Block World to look for the site in."),
        ],
        changes: false,
        returns: "{ dimension, way_out }",
    },
    CallInfo {
        name: "dimension.list",
        about: "The dimensions made: the level's own first, then each other, with its number, its name, its kind and settings, the `.dimension` file it was made from (none for one made by code, as a dungeon's), its key (which of its file's it is, as each dungeon's), and its Objs at the top. `shown` is the dimension the Scene view in use shows.",
        args: &[],
        changes: false,
        returns: "{ shown, dimensions: [{ dimension, name, file, key, kind, sky, weather, clock, keep_loaded, instances, saved, objs }] }",
    },
    CallInfo {
        name: "dimension.show",
        about: "Shows a dimension in the Scene view in use, from where it last looked from there, as its Dimension menu does; the game's sky, sun and weather are then that dimension's.",
        args: &[arg("dimension", ArgKind::Value, "Its number, as `dimension.list` gives it (0 for the level's), or its `.dimension` file once made.")],
        changes: false,
        returns: "{ shown }",
    },
    CallInfo {
        name: "dimension.create",
        about: "Makes a new `.dimension` file of a kind, as Project ▸ + ▸ New Dimension does: its settings, and a Portal, Way Back, for a Portal in the level to lead into.",
        args: &[
            arg("kind", ArgKind::Choice(&["overworld", "dungeon", "caves"]), "What it is for, which sets good values for the rest of its settings."),
            maybe("folder", ArgKind::Text, "The folder to make it in, from the project's: Dimensions when left out."),
        ],
        changes: true,
        returns: "{ asset: { guid, path } }",
    },
    CallInfo {
        name: "dimension.make",
        about: "Makes the dimension of a `.dimension` file: while editing, opens the file alongside the level, its Objs to edit and save under a header of their own; while playing, made as a script would make it. The Scene view in use then shows it.",
        args: &[arg("dimension", ArgKind::Asset, "The .dimension file.")],
        changes: false,
        returns: "{ dimension }",
    },
    CallInfo {
        name: "dimension.let_go",
        about: "Lets a dimension go, its Objs with it: a `.dimension` file open in the editor closes, which with edits not saved needs `discard`; one made by code, as a dungeon's, goes. A Scene view showing it goes back to the level's.",
        args: &[
            arg("dimension", ArgKind::Value, "Its number, as `dimension.list` gives it, or its `.dimension` file."),
            maybe("discard", ArgKind::Toggle, "Let it go with its file's edits not saved."),
        ],
        changes: true,
        returns: "{}",
    },
    CallInfo {
        name: "obj.move_to_dimension",
        about: "Moves an Obj at the top, everything under it with it, into another dimension, at the same place in that dimension's coordinates or at `at`. While editing it moves into the file of the dimension it goes into (the level's active scene for the level's); a dimension made by code takes Objs only while playing.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj."),
            arg("dimension", ArgKind::Value, "Its number, as `dimension.list` gives it (0 for the level's), or its `.dimension` file."),
            maybe("at", ArgKind::Vector, "Where it goes there, in metres."),
        ],
        changes: true,
        returns: "{ dimension }",
    },
    CallInfo {
        name: "navigation.status",
        about: "The navigation kept for each Block World something walks in: how many chunks have places to stand, how many places, how much it keeps, how often it has been made again from the world's blocks and how long that took, and whether it is up to date with them; and how many Nav Agents and Block Mobs there are.",
        args: &[],
        changes: false,
        returns: "{ worlds: [{ world, chunks, places, kilobytes, version, made, build_ms, settled, model_cells }], agents, mobs, walkable }",
    },
    CallInfo {
        name: "ground_fit.land",
        about: "Sets an Obj with a Ground Fit down on the ground of the world of blocks in its dimension, as the generator and every other Ground Fit make it, its own fit left out: where it lands dropped in the Scene view. At its own place along x and z, or at `at`. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Ground Fit."),
            maybe("at", ArgKind::Vector, "Where to set it down along x and z, in metres; its own place when left out."),
        ],
        changes: true,
        returns: "{ position }",
    },
    CallInfo {
        name: "blocks.bake_far",
        about: "Starts baking a streamed Block World's far view in the background: every tile of each level from `from` out to the coarsest, over the whole world where it has a set size, or `reach` metres round where it is seen from where it has none, with the trees and structures streaming builds on them, kept in the project's .voxear/FarView. Streaming reads them as they are kept, the coarsest first. A bake stopped and started again goes on where it was. Ask how far it has got with blocks.bake_far_status.",
        args: &[
            maybe("obj", ArgKind::Obj, "The Block World; the selected one or the first streamed one when left out."),
            maybe("from", ArgKind::Whole, "The finest level baked, 3 (the far view's first) when left out."),
            maybe("reach", ArgKind::Number, "For a world without end, how far round to bake, in metres; 16384 when left out."),
        ],
        changes: false,
        returns: "{ world, from }",
    },
    CallInfo {
        name: "blocks.bake_far_status",
        about: "How far a bake of a far view has got: the world, tiles done of all and the level under way; or, with none under way, how the last ended and how many tiles it made.",
        args: &[],
        changes: false,
        returns: "{ baking, world, done, total, level } or { baking: false, last }",
    },
    CallInfo {
        name: "blocks.bake_far_stop",
        about: "Stops a far view's bake after the tiles it is making; what it kept stays.",
        args: &[],
        changes: false,
        returns: "{ baking, ... }",
    },
    CallInfo {
        name: "blocks.columns",
        about: "Each world of blocks without end streamed last frame: the chunk columns made round where it is seen from, those made and let go that frame, and how many Objs are kept while their ground is away (see Streams With Ground).",
        args: &[],
        changes: false,
        returns: "{ worlds: [{ world, near, came, went }], kept }",
    },
    CallInfo {
        name: "road.set_points",
        about: "Sets every point of an Obj's Road, in the Obj's own space, each with how wide the road is there in blocks; with `land`, each point set down on the ground as it is without the road. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Road."),
            arg("points", ArgKind::Value, "The points, each `[x, y, z, width]`, two at least."),
            maybe("land", ArgKind::Toggle, "Whether each point is set down on the ground."),
        ],
        changes: true,
        returns: "{ points }",
    },
    CallInfo {
        name: "road.set_mix",
        about: "Sets the blocks an Obj's Road or Area Fill is laid with, each with its weight, and the pattern of them by their place from 1, a row a block along, 0 for the mix. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Road or Area Fill."),
            arg("mix", ArgKind::Value, "The blocks, each `{ \"block\": <asset>, \"weight\": n }`."),
            maybe("pattern", ArgKind::Value, "Rows of digits, as `[\"1121\"]`."),
        ],
        changes: true,
        returns: "{ mix, pattern }",
    },
    CallInfo {
        name: "area_fill.set_outline",
        about: "Sets the outline of an Obj's Area Fill along x and z, in metres in the Obj's own space, three corners at least. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Area Fill."),
            arg("outline", ArgKind::Value, "The corners, each `[x, z]`."),
        ],
        changes: true,
        returns: "{ outline }",
    },
    CallInfo {
        name: "world_area.set_outline",
        about: "Sets the outline of an Obj's World Area along x and z, in metres in the Obj's own space, three corners at least, and makes it an outline area. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the World Area."),
            arg("outline", ArgKind::Value, "The corners, each `[x, z]`."),
        ],
        changes: true,
        returns: "{ outline }",
    },
    CallInfo {
        name: "areas.at",
        about: "The World Areas holding a place, in the dimension the Scene view shows, highest priority first: each its Obj and its tags with their data.",
        args: &[arg("at", ArgKind::Vector, "The place, in metres.")],
        changes: false,
        returns: "[{ obj, priority, tags: [{ tag, data }] }]",
    },
    CallInfo {
        name: "areas.value",
        about: "The data asset an area tag carries at a place: of the first World Area there with the tag, or where none has it, of the world's Area Defaults for the biome there, then for anywhere. Null where nothing carries the tag.",
        args: &[
            arg("at", ArgKind::Vector, "The place, in metres."),
            arg("tag", ArgKind::Asset, "The Area Tag data asset."),
        ],
        changes: false,
        returns: "{ data } or null",
    },
    CallInfo {
        name: "route.set_points",
        about: "Sets every point of an Obj's Route, in the Obj's own space; with `land`, each point set down on the ground. One step to undo.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Route."),
            arg("points", ArgKind::Value, "The points, each `[x, y, z]`, two at least."),
            maybe("land", ArgKind::Toggle, "Whether each point is set down on the ground."),
        ],
        changes: true,
        returns: "{ points }",
    },
    CallInfo {
        name: "route.lay_sea",
        about: "Lays an Obj's Route over the sea between its first and last points, over water deeper than its Draught and further than its Shore from land, searched over the world's generated ground however far off. One step to undo.",
        args: &[arg("obj", ArgKind::Obj, "The Obj with the Route.")],
        changes: true,
        returns: "{ points, aground }",
    },
    CallInfo {
        name: "routes.find",
        about: "The way from one place to another along the routes of a kind (`road`, every Road among them, when left out; `sea`, `air`, or a Route Kind data asset): its points, its length in metres and the Route Stops it passes. Null where none joins them.",
        args: &[
            arg("from", ArgKind::Vector, "Where from, in metres."),
            arg("to", ArgKind::Vector, "Where to, in metres."),
            maybe("kind", ArgKind::Value, "road, sea, air, or a Route Kind asset."),
        ],
        changes: false,
        returns: "{ points, length, stops }",
    },
    CallInfo {
        name: "routes.nearest",
        about: "The nearest place to `at` on a route of a kind: its Obj, the place, and how far along the route it lies in metres. Null with none.",
        args: &[
            arg("at", ArgKind::Vector, "The place, in metres."),
            maybe("kind", ArgKind::Value, "road, sea, air, or a Route Kind asset."),
        ],
        changes: false,
        returns: "{ route, point, along }",
    },
    CallInfo {
        name: "roads.nearest",
        about: "The road nearest a place along the ground, within `reach` metres (50 when left out), among those over the world of blocks there: its Obj, the nearest point of its middle, and how far along it that is in metres. Null where none is near.",
        args: &[
            arg("at", ArgKind::Vector, "The place, in metres."),
            maybe("reach", ArgKind::Number, "How far off it may be, in metres."),
        ],
        changes: false,
        returns: "{ road, point, along }",
    },
    CallInfo {
        name: "place.prefab",
        about: "Makes an instance of a prefab on the ground of the world of blocks there, at `at` along x and z, turned `turn` degrees; with `face_road`, turned to face the road nearest and set `setback` metres back from its edge, as its Placement says. Lands on the ground without its own Ground Fit. One step to undo.",
        args: &[
            arg("prefabs", ArgKind::Value, "The prefab, as a list of one: `[{ \"prefab\": <asset> }]`."),
            arg("at", ArgKind::Vector, "Where, in metres; up is found from the ground."),
            maybe("turn", ArgKind::Number, "How far it is turned about up, in degrees."),
            maybe("face_road", ArgKind::Toggle, "Whether it faces the road nearest."),
            maybe("setback", ArgKind::Number, "How far back from the road's edge, in metres: 1 when left out."),
            maybe("parent", ArgKind::Obj, "The Obj it goes under."),
        ],
        changes: true,
        returns: "{ objs }",
    },
    CallInfo {
        name: "place.line_up",
        about: "Lines prefabs up along a Road, each facing it and set back from its edge a little more or less than the last, as often as its weight, no two alike side by side, none where something with a Placement or a Ground Fit stands: on `side` (left, right or both), from `from` to `to` metres along it. The same `seed` gives the same street. One step to undo.",
        args: &[
            arg("road", ArgKind::Obj, "The Obj with the Road."),
            arg("prefabs", ArgKind::Value, "The prefabs, each `{ \"prefab\": <asset>, \"weight\": n }`."),
            maybe("side", ArgKind::Text, "left, right or both: both when left out."),
            maybe("from", ArgKind::Number, "Where the stretch starts, in metres along the road."),
            maybe("to", ArgKind::Number, "Where it ends, in metres along; its end when left out."),
            maybe("setback", ArgKind::Number, "How far back from the edge, in metres: 1 when left out."),
            maybe("jitter", ArgKind::Number, "How much more or less, in metres: 0.5 when left out."),
            maybe("gap", ArgKind::Number, "How far apart, in metres: 1 when left out."),
            maybe("seed", ArgKind::Whole, "The seed."),
            maybe("parent", ArgKind::Obj, "The Obj they go under."),
        ],
        changes: true,
        returns: "{ objs }",
    },
    CallInfo {
        name: "place.scatter",
        about: "Scatters prefabs over an outline or an Area Fill's, about `density` of them each 100 square metres, as often as their weights, turned any way, none where something stands. One step to undo.",
        args: &[
            arg("prefabs", ArgKind::Value, "The prefabs, each `{ \"prefab\": <asset>, \"weight\": n }`."),
            maybe("area", ArgKind::Obj, "An Obj with an Area Fill whose outline to fill."),
            maybe("outline", ArgKind::Value, "Or the outline, each corner `[x, z]` in metres."),
            maybe("density", ArgKind::Number, "How many each 100 square metres: 1 when left out."),
            maybe("seed", ArgKind::Whole, "The seed."),
            maybe("parent", ArgKind::Obj, "The Obj they go under."),
        ],
        changes: true,
        returns: "{ objs }",
    },
    CallInfo {
        name: "place.brush",
        about: "A stroke of the prefab brush: prefabs scattered within `radius` metres of `at`, about `density` each 100 square metres; or, with `remove`, the instances of those prefabs there taken away. One step to undo.",
        args: &[
            arg("prefabs", ArgKind::Value, "The prefabs, each `{ \"prefab\": <asset>, \"weight\": n }`."),
            arg("at", ArgKind::Vector, "The brush's middle, in metres."),
            maybe("radius", ArgKind::Number, "Its radius, in metres: 5 when left out."),
            maybe("density", ArgKind::Number, "How many each 100 square metres: 4 when left out."),
            maybe("remove", ArgKind::Toggle, "Whether it takes them away."),
            maybe("seed", ArgKind::Whole, "The seed."),
            maybe("parent", ArgKind::Obj, "The Obj they go under."),
        ],
        changes: true,
        returns: "{ objs } or { removed }",
    },
    CallInfo {
        name: "paint.map",
        about: "A dab of a map brush on a world of blocks without end's generator: `land`, `height`, `biome` (the one named, as the world lists it), `area` (a World Area, painted with its number) or `erase` back to the generator alone, round `at`, `radius` metres across; `take_away` paints sea, lowers, or takes the biome or area off. Only the columns it reaches are made again. One step to undo.",
        args: &[
            arg("map", ArgKind::Text, "land, height, biome, area or erase."),
            arg("at", ArgKind::Vector, "The dab's middle, in metres."),
            maybe("radius", ArgKind::Number, "How wide, in metres: 40 when left out."),
            maybe("strength", ArgKind::Number, "How strong, 0 to 1: 0.3 when left out."),
            maybe("falloff", ArgKind::Number, "How much of the radius eases out, 0 to 1: 0.5 when left out."),
            maybe("take_away", ArgKind::Toggle, "Paints sea, lowers, or takes the biome away."),
            maybe("smooth", ArgKind::Toggle, "Evens the land or height out."),
            maybe("biome", ArgKind::Asset, "For `biome`, the .biome the world lists."),
            maybe("area", ArgKind::Obj, "For `area`, the Obj with the World Area."),
            maybe("obj", ArgKind::Obj, "The Block World."),
        ],
        changes: true,
        returns: "{ changed }",
    },
    CallInfo {
        name: "world_layers.status",
        about: "The world layers over each world of blocks without end's ground, as it was last streamed: how many Ground Fits, how many chunk columns they reach, and which kinds are hidden to show the bare ground.",
        args: &[],
        changes: false,
        returns: "{ worlds: [{ world, fits, columns }], hidden: { fits } }",
    },
    CallInfo {
        name: "scene_view.culling",
        about: "Whether the Scene views draw only what the game's camera draws while playing, and a selected camera's while editing (Gizmos ▸ Game Camera's Culling): its frustum, draw distances, LODs and the Game view's depth cull them, not their own camera. Answers the camera they cull from, and what the first view drawn culled a few frames ago.",
        args: &[maybe("on", ArgKind::Toggle, "Whether they cull from the game's camera.")],
        changes: false,
        returns: "{ on, camera, first_view: { frustum_culled, occlusion_culled, meshlets_occlusion_culled } }",
    },
    CallInfo {
        name: "scene_view.projection",
        about: "Whether the Scene view in use sees orthographic, every ray parallel, or in perspective, as its header's Ortho button says; sets it when `projection` is given. Turned orthographic it sees as much as it did 10 m ahead; its Size is the camera's own, which `field.set` on the camera also sets.",
        args: &[maybe("projection", ArgKind::Choice(&["perspective", "orthographic"]), "How it sees.")],
        changes: false,
        returns: "{ projection, size }",
    },
    CallInfo {
        name: "world_layers.hide",
        about: "Hides world layers of a kind from every world's ground, to see the bare ground under them, or shows them again. A view setting: the scene is not changed.",
        args: &[maybe("fits", ArgKind::Toggle, "Whether Ground Fits are hidden.")],
        changes: false,
        returns: "{ hidden: { fits } }",
    },
    CallInfo {
        name: "navigation.bake",
        about: "Bakes the open level's nav mesh from its still colliders, as its Inspector's Bake Navigation does, for agents on levels of models; written beside the level as its `.nav` file. Answers once the bake is done.",
        args: &[],
        changes: true,
        returns: "{ path, polygons }",
    },
    CallInfo {
        name: "navigation.path",
        about: "Finds the way from one place to another over the Block World the first stands in, as a Nav Agent finds it, for something as tall as `height` that swims, opens doors and jumps: its corners in metres, whether it reaches (Complete, Partial as near as it can get, Continues for the first part of a long way, None), what it costs, how long it is, and how long finding it took.",
        args: &[
            arg("from", ArgKind::Vector, "Where the way begins, in metres."),
            arg("to", ArgKind::Vector, "Where it goes, in metres."),
            maybe("height", ArgKind::Number, "How tall the walker is, in metres: 1.8 when left out."),
        ],
        changes: false,
        returns: "{ status, corners, cost, length, ms }",
    },
    CallInfo {
        name: "navigation.nearest",
        about: "The place to stand nearest a point, on its floor, in metres: where a Nav Agent sent there would end. Null where there is none within reach.",
        args: &[
            arg("at", ArgKind::Vector, "The point, in metres."),
            maybe("reach", ArgKind::Number, "How far off a place may be, in metres: 4 when left out."),
        ],
        changes: false,
        returns: "{ place }",
    },
    CallInfo {
        name: "navigation.agents",
        about: "Every Nav Agent and Block Mob in the level: its Obj, where it is, where it is going, whether it has arrived, how its way ends, how far it has to go, its velocity, and the corners of its way.",
        args: &[],
        changes: false,
        returns: "{ agents: [{ obj, at, destination, arrived, status, remaining, velocity, corners }] }",
    },
    CallInfo {
        name: "nav_agent.set_destination",
        about: "Sends an Obj's Nav Agent somewhere, as setting its Destination from a script does, or stops it with no `to`. Agents walk only while playing.",
        args: &[
            arg("obj", ArgKind::Obj, "The Obj with the Nav Agent."),
            maybe("to", ArgKind::Vector, "Where to go, in metres; it stands where it is when left out."),
            maybe(
                "dimension",
                ArgKind::Value,
                "The dimension `to` is in, by its number as `dimension.list` gives it or its `.dimension` file once made, where it is not the agent's own: it walks to a Portal into it and through.",
            ),
        ],
        changes: true,
        returns: "{}",
    },
    CallInfo {
        name: "reflection_probe.capture",
        about: "Captures a Reflection Probe again, as its Capture button does: what it shows then stays until it is captured again or baked.",
        args: &[arg("obj", ArgKind::Obj, "An Obj with a Reflection Probe.")],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "lighting.bake",
        about: "Bakes the open level's lighting, as Bake Lighting does: the light falling on every Obj with a Lightmap goes into lightmaps, and on every Light Probe Group's probes into spherical harmonics, traced on the GPU at the level's [post.global_illumination] settings, then written beside the level as its .lightmaps file, which the game loads with the level. Answers once it is written.",
        args: &[],
        changes: true,
        returns: "{ file, bytes, parts, lightmaps, probes, texels, seconds, tracing_seconds }",
    },
    CallInfo {
        name: "lighting.bake_cancel",
        about: "Gives up the lighting bake under way, if one is: the level keeps the bake it had.",
        args: &[],
        changes: false,
        returns: "{ cancelled }",
    },
    CallInfo {
        name: "lighting.bake_status",
        about: "Whether a lighting bake is under way and how far it has gone, from 0 to 1, how the last one ended, and how many lightmaps and light probes the open level has baked.",
        args: &[],
        changes: false,
        returns: "{ baking, progress, last, lightmaps, probes }",
    },
    CallInfo {
        name: "lighting.probe_light",
        about: "The baked light probes' light at a point, blended from the probes round it as an Obj there is lit: the light a surface facing `normal` takes, the average from every way, and the nine spherical harmonics, red, green and blue each.",
        args: &[
            arg("position", ArgKind::Vector, "Metres, in the world."),
            maybe("normal", ArgKind::Vector, "The way the surface faces; up when left out."),
        ],
        changes: false,
        returns: "{ found, light, average, harmonics }",
    },
    CallInfo {
        name: "reflection_probes.bake",
        about: "Bakes the open level's reflection probes, as Bake Reflection Probes does: each probe captured once is captured again, then written beside the level as its .probes file, which the game loads with the level. Answers once it is written.",
        args: &[],
        changes: true,
        returns: "{ probes, file, bytes }",
    },
    CallInfo {
        name: "reverb_probes.bake",
        about: "Bakes the open level's reverb probes, as Bake Reverb Probes does: every probe of every Reverb Probe's box is traced on the GPU, then how long sound rings at each is written beside the level as its .reverb file, which the game rings by when it does not trace rays itself. Answers once it is written.",
        args: &[],
        changes: true,
        returns: "{ probes, inside, file, bytes }",
    },
    CallInfo {
        name: "audio.rays",
        about: "Ray-traced audio in the running game: whether it is on, its quality and its budget of GPU time a frame, as a script's Audio.RayTraced, RayQuality and RayBudget set them until play stops (a project's own are its voxear.toml's [audio] ray_traced, ray_quality and ray_budget, a level's its [audio]); and what it does now: whether it traces, its rays a frame, the pass's least GPU time over the last 60 frames, and how long the listener's room rings in each band.",
        args: &[
            maybe("on", ArgKind::Toggle, "Trace sound by rays."),
            maybe("quality", ArgKind::Choice(&["low", "medium", "high"]), "How many rays at most."),
            maybe("budget", ArgKind::Number, "The most GPU time a frame, in milliseconds."),
        ],
        changes: false,
        returns: "{ on, quality, budget, tracing, rays, cost_ms, room: [low, middle, high] }",
    },
    CallInfo {
        name: "commands",
        about: "Every command the command palette runs: the editor's own, those code adds, and the scripts' [Command] methods.",
        args: &[],
        changes: false,
        returns: "{ commands: [{ id, title, keys }] }",
    },
    CallInfo {
        name: "game.calls",
        about: "The game's own calls: each static method its scripts mark [Call], with what it does, its arguments (each one's name, the kind of value it takes, its C# type, whether it must be given, and an enum's names), what it gives back, when it may run (`both`, `editing` or `playing`) and whether it changes the level. Each is called by its name as any call is, or through `game.call`.",
        args: &[],
        changes: false,
        returns: "{ calls: [{ name, about, args, returns, when, changes }] }",
    },
    CallInfo {
        name: "game.call",
        about: "Makes one of the game's own calls, as `game.calls` lists them: against the level being edited, the scripts starting first if they have not, what it changes one step to undo; or against the game while it plays. Gives back what the method returned, as a save writes it.",
        args: &[
            arg("name", ArgKind::Text, "The call's name, as `game.calls` lists it."),
            maybe("args", ArgKind::Value, "Its arguments, a JSON object by their names."),
        ],
        changes: false,
        returns: "{ result }",
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
        name: "lighting.get",
        about: "The open scene's lighting as the Lighting page shows it: every setting in effect, laid out as a \
                .lighting file writes them, the paths `lighting.set` takes, and which upscalers run on this GPU.",
        args: &[],
        changes: false,
        returns: "{ lighting, changeable, own, settings, paths: [path], upscalers: [{ name, runs, why }] }",
    },
    CallInfo {
        name: "lighting.set",
        about: "Changes one setting of the open scene's lighting, as the Lighting page does, as a step to undo: in \
                the lighting settings it names, or in its own while it names none. `sun.shadows` to `cascades`, \
                `post.anti_aliasing.upscaler` to `fsr`.",
        args: &[
            arg("path", ArgKind::Text, "As `lighting.get` lists them under `paths`."),
            arg("value", ArgKind::Value, "A switch, a number, a word or a list of numbers; null takes it out."),
        ],
        changes: true,
        returns: "As lighting.get",
    },
    CallInfo {
        name: "lighting.scene",
        about: "Names the lighting settings the open scene is lit by, as the Lighting page's Lighting menu does: \
                built-in ones such as `builtin:golden_hour`, a .lighting file of the project's, or null for none, \
                so the scene lights itself by its own settings. The scene keeps them when it is saved.",
        args: &[arg("lighting", ArgKind::Value, "`builtin:` and a look's name, a .lighting asset, or null.")],
        changes: true,
        returns: "As lighting.get",
    },
    CallInfo {
        name: "lighting.copy",
        about: "Makes new lighting settings in the Lighting folder, a copy of those the open scene is lit by, or of \
                its own, and lights the scene by them, as the Lighting page's Make a Copy and New Lighting Settings \
                do: a way to change built-in lighting.",
        args: &[],
        changes: true,
        returns: "As lighting.get",
    },
    CallInfo {
        name: "look_pack.list",
        about: "Every look pack there is to wear, the engine's and the project's, each with what `look_pack.wear` \
                takes to wear it, and the pack the Scene view and the game wear.",
        args: &[],
        changes: false,
        returns: "{ packs: [{ reference, name, description, source }], worn }",
    },
    CallInfo {
        name: "look_pack.wear",
        about: "Wears a look pack in the Scene view and the game, as the Lighting page's Look pack menu does and a \
                script's LookPacks.Use; null takes it off.",
        args: &[maybe(
            "pack",
            ArgKind::Value,
            "As `look_pack.list` gives it: `builtin:sunlit`, or a .lookpack of the project's.",
        )],
        changes: true,
        returns: "{ worn }",
    },
    CallInfo {
        name: "look_pack.get",
        about: "A look pack of the project's as its file writes it, the setting paths `look_pack.set` takes, and the \
                project's look shaders with their properties, for the pack's `[shader]`.",
        args: &[arg("pack", ArgKind::Asset, "A .lookpack of the project's.")],
        changes: false,
        returns: "{ pack, settings, paths: [path], shaders: [{ shader, properties: [{ name, kind }] }] }",
    },
    CallInfo {
        name: "look_pack.set",
        about: "Writes one setting of a look pack of the project's, as its Inspector page does, as a step to undo, \
                and wears it again where it is worn: `sun.tint`, `water.deep`, `blocks.voxels`, `description`; \
                `shader.shader` names its look shader, starting its properties afresh, and `shader.` and a \
                property's name sets one. Refused where the pack could not then be worn.",
        args: &[
            arg("pack", ArgKind::Asset, "A .lookpack of the project's."),
            arg("path", ArgKind::Text, "As `look_pack.get` lists them under `paths`, or `shader.` and a name."),
            arg("value", ArgKind::Value, "A switch, a number, a word or a list of numbers; null takes it out."),
        ],
        changes: true,
        returns: "As look_pack.get",
    },
    CallInfo {
        name: "texture_pack.list",
        about: "Every texture pack there is to wear, the engine's and the project's, each with what \
                `texture_pack.wear` takes to wear it, and the pack worn.",
        args: &[],
        changes: false,
        returns: "{ packs: [{ reference, name, description, source }], worn }",
    },
    CallInfo {
        name: "texture_pack.wear",
        about: "Wears a texture pack, as the Lighting page's Texture pack menu does and a script's TexturePacks.Use; \
                null takes it off.",
        args: &[maybe("pack", ArgKind::Value, "As `texture_pack.list` gives it.")],
        changes: true,
        returns: "{ worn }",
    },
    CallInfo {
        name: "audio.mix",
        about: "The running game's volumes, from 0 to 1, and whether it plays for headphones and rings with the \
                room's reverb, after setting those given, as a script's Audio.MasterVolume, MusicVolume, \
                EffectsVolume, Headphones and Reverb set them. Setting needs the game playing.",
        args: &[
            maybe("master", ArgKind::Number, "The volume over everything."),
            maybe("music", ArgKind::Number, "The music bus's volume."),
            maybe("effects", ArgKind::Number, "The effects bus's volume."),
            maybe("headphones", ArgKind::Toggle, "3D sound heard as through headphones."),
            maybe("reverb", ArgKind::Toggle, "3D sound rings as the room does."),
        ],
        changes: false,
        returns: "{ master, music, effects, headphones, reverb }",
    },
    CallInfo {
        name: "display",
        about: "The game's window, its size and whether it fills the screen, and its HDR: what is asked for, as a \
                script's Screen.SetHdr asks, what is shown and what the display offers. HDR asked for while the \
                game plays is made so when the game shows in a window of its own.",
        args: &[
            maybe("hdr", ArgKind::Choice(&["off", "on", "hdr10", "scrgb"]), "The HDR output to ask for."),
            maybe("paper_white", ArgKind::Number, "How bright white paper is, in nits."),
            maybe("peak", ArgKind::Number, "The brightest the display shows, in nits."),
        ],
        changes: false,
        returns: "{ width, height, fullscreen, hdr, paper_white, peak, output, offered: { hdr10, scrgb } }",
    },
    CallInfo {
        name: "shader_pack.list",
        about: "Every shader pack there is to wear, the engine's, the project's and a player's, each with what \
                `shader_pack.wear` takes to wear it, and the pack the Scene view wears: its passes, its options and \
                how its last compile went.",
        args: &[],
        changes: false,
        returns: "{ packs: [{ reference, name, description, source }], worn: { reference, standing, passes, options } }",
    },
    CallInfo {
        name: "shader_pack.wear",
        about: "Wears a shader pack in the Scene view and the game, as the Lighting page's Shader pack menu does, \
                compiled before this answers, and draws it again whenever one of its files is saved; null takes it \
                off. A pack that does not compile answers with Slang's message, and the one worn stays.",
        args: &[maybe(
            "pack",
            ArgKind::Value,
            "As `shader_pack.list` gives it: `builtin:evening`, a pack.toml of the project's, or `file:` and a folder; \
             left out or null for none.",
        )],
        changes: false,
        returns: "{ worn: { reference, standing, passes: [{ name, program, at, reads, writes }], options } }",
    },
    CallInfo {
        name: "shader_pack.options",
        about: "The worn shader pack's passes and options, each with its label, tooltip, kind and value now; with \
                `option` and `value`, sets that option first, as its row on the pack's Inspector page does.",
        args: &[
            maybe("option", ArgKind::Text, "An option's name, as `options` lists it."),
            maybe("value", ArgKind::Value, "A number, true or false, or one of a choice's choices."),
        ],
        changes: false,
        returns: "{ worn: { reference, standing, passes, options: [{ key, label, about, kind, value }] } }",
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
        name: "box_model.sheet_save",
        about: "Writes a box model's sheet beside it as a picture, `Model Sheet.png` for `Model.boxmodel`, to paint in another program.",
        args: &[arg("asset", ArgKind::Asset, "The box model.")],
        changes: true,
        returns: "{ picture }",
    },
    CallInfo {
        name: "box_model.sheet_load",
        about: "Paints a box model's sheet from a picture of the project, pixel for pixel, and saves the model. A picture of another size makes the sheet that size, each face where it was on it.",
        args: &[arg("asset", ArgKind::Asset, "The box model."), arg("picture", ArgKind::Asset, "The picture.")],
        changes: true,
        returns: "{ asset, sheet: [width, height] }",
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
        name: "profiler.frames",
        about: "The latest frames the Profiler took: each one's CPU samples in the order of their tree, with how deep each ran, and the GPU's passes. Reading keeps frames sampled for a few seconds, even with the Profiler hidden, so read again within that to watch.",
        args: &[
            maybe("after", ArgKind::Whole, "From an earlier read's `next`; the frames kept when left out."),
            maybe("limit", ArgKind::Whole, "The most frames, the latest; 120 when left out."),
        ],
        changes: false,
        returns: "{ frames: [{ frame, frame_ms, cpu_ms, gpu_ms, gpu_frame_ms, blocks: { draws, triangles, shadow_draws }, mesh_pool: { … as mesh_pool.get }, posed: { skinned, by_bones, weighted, unseen, shared, baked, at_rest }, samples: [{ name, depth, ms, self_ms, calls, alloc? }], gpu: [{ pass, ms }] }], next }",
    },
    CallInfo {
        name: "crowds.get",
        about: "The crowd as the game plays: whether tiers are on, the Near, Middle and Records beyond distances (Project Settings ▸ Crowds), how many characters are near, middle and far this frame and kept as records far off, and how the renderer posed models' parts (skinned, by their bones where drawn, where drawn for want of room, unseen, sharing a pose, from baked clips, at rest).",
        args: &[],
        changes: false,
        returns: "{ enabled, near, middle, records_beyond, counts: { near, middle, far, records }, posed: { skinned, by_bones, weighted, unseen, shared, baked, at_rest } }",
    },
    CallInfo {
        name: "mesh_pool.get",
        about: "The vertex and index space every mesh shares, and who holds it: models (with built-in meshes and skinned parts' poses), worlds of blocks' lighting (their chunks' meshes for the GI's and sound's rays, and how far from the camera the farthest is), what is being freed, and what is free; the share lighting may take (Project Settings ▸ Quality ▸ Block Worlds ▸ Lighting share), and how many lighting chunks have been given back for models to fit, of those at once.",
        args: &[],
        changes: false,
        returns: "{ vertex_capacity, index_capacity, model_vertices, model_indices, lighting_vertices, lighting_indices, lighting_meshes, lighting_reach, freeing_vertices, freeing_indices, free_vertices, free_indices, lighting_share, given_back, given_back_at_once }",
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
        about: "The editor's own settings, as Preferences shows them: its categories, and each setting with its category and section, label, tooltip (what it does), type, value, default, whether it is changed, and its options, range and unit. A choice's value is its option's label; a colour is #rrggbb; a folder is its path or null.",
        args: &[
            maybe("category", ArgKind::Text, "Only this category's, as `categories` names them."),
            maybe("search", ArgKind::Text, "Only those with these words in their label, section, category or tooltip."),
        ],
        changes: false,
        returns: "{ categories: [{ name, about, manual }], settings: [{ key, category, section, label, tooltip, type, value, default, changed, advanced, options?, min?, max?, unit?, whole? }] }",
    },
    CallInfo {
        name: "settings.set",
        about: "Changes one of the editor's own settings by its key, as Preferences does: a value of its type, a choice by its option's label, or null for its default. A number outside its range is refused, saying the range.",
        args: &[arg("key", ArgKind::Text, ""), maybe("value", ArgKind::Value, "Null for its default.")],
        changes: false,
        returns: "{ value }",
    },
    CallInfo {
        name: "settings.find",
        about: "Finds a setting by what it does, anywhere the editor has one: Project Settings, Preferences, the Lighting panel, the components of the open level's Objs and import settings. Rough words match each setting's name, where it is and its tooltip, and words that mean the same (shimmer finds anti-aliasing), best first, with the words matched, the line of its tooltip that matched, and what turns on one hidden for now.",
        args: &[
            arg("query", ArgKind::Text, "What the setting does, in your own words."),
            maybe("most", ArgKind::Whole, "How many to give back at most; 20 by default."),
        ],
        changes: false,
        returns: "{ found: [{ key, label, where, line, matched, score, hidden }] }",
    },
    CallInfo {
        name: "settings.show",
        about: "Opens a setting where it is, scrolled to it and marked: Project Settings or Preferences by its key from `project_settings.get` or `settings.get`, or anything `settings.find` found by its key (the Lighting panel's, an Obj's component's, import settings').",
        args: &[arg("key", ArgKind::Text, "")],
        changes: false,
        returns: "{}",
    },
    CallInfo {
        name: "project_settings.get",
        about: "The project's settings, as Project Settings shows them: its categories, and each setting with its key (its table and key in voxear.toml), category and section, label, tooltip, type, value, default, whether it is changed, and its options, range and unit. Settings of type part or button are parts of the window, not values.",
        args: &[
            maybe("category", ArgKind::Text, "Only this category's: Audio, Physics, Time…"),
            maybe("search", ArgKind::Text, "Only those with these words in their label, section, category or tooltip."),
        ],
        changes: false,
        returns: "{ categories: [{ name, about, manual }], settings: [{ key, category, section, label, tooltip, type, value, default, changed, advanced, options?, min?, max?, unit?, whole?, count?, asset? }] }",
    },
    CallInfo {
        name: "project.get",
        about: "The project's settings: its voxear.toml as JSON.",
        args: &[],
        changes: false,
        returns: "{ project }",
    },
    CallInfo {
        name: "world_quality.get",
        about: "What making worlds of blocks spends now: its preset, threads, columns at once, megabytes uploaded a frame, view and far limits in metres, near, far and plant details (how many times as far as a world says), and the largest a coarser ring's blocks show in pixels; whether this machine keeps its own; and the project's.",
        args: &[],
        changes: false,
        returns: "{ quality, threads, columns, upload, view_distance, far_distance, near_detail, far_detail, plant_detail, far_block_pixels, machine, project }",
    },
    CallInfo {
        name: "world_quality.set",
        about: "Changes what making worlds of blocks spends, at once: a preset sets every number, and a number given after it makes it Custom. Written into this machine's Graphics.toml where it keeps its own, else into voxear.toml.",
        args: &[
            maybe("quality", ArgKind::Choice(&["low", "medium", "high", "ultra", "custom"]), ""),
            maybe("threads", ArgKind::Whole, ""),
            maybe("columns", ArgKind::Whole, "Chunk columns made at once."),
            maybe("upload", ArgKind::Whole, "Megabytes of chunks to the GPU a frame."),
            maybe("view_distance", ArgKind::Number, "The furthest a world's own chunks reach, in metres."),
            maybe("far_distance", ArgKind::Number, "The furthest its coarser rings reach, in metres."),
            maybe("near_detail", ArgKind::Number, "How many times as far as the world says its own chunks reach."),
            maybe("far_detail", ArgKind::Number, "How many times as far as the world says its rings reach."),
            maybe("plant_detail", ArgKind::Number, "How many times as far as each Foliage says plants are drawn."),
            maybe(
                "far_block_pixels",
                ArgKind::Number,
                "The largest a coarser ring's blocks show where it begins, in pixels of a screen 1080 tall: the rings are pushed out until they are no larger. 0 for where the distances put them.",
            ),
            maybe(
                "machine",
                ArgKind::Toggle,
                "True keeps it for this machine, beside the game's saves, in place of the project's; false forgets this machine's own and goes back to the project's.",
            ),
        ],
        changes: true,
        returns: "as world_quality.get",
    },
    CallInfo {
        name: "project.set",
        about: "Changes one of the project's settings, by its key from `project_settings.get`, as Project Settings does: written into voxear.toml (only where it differs from the default) and put into effect, at once or the next time the game plays. A choice by its option's label, an asset by its path, null for the default. A value outside its range is refused, saying the range.",
        args: &[
            arg("key", ArgKind::Text, "As `audio.reverb`, `physics.gravity`, `world.quality`."),
            maybe("value", ArgKind::Value, "Null for its default."),
        ],
        changes: true,
        returns: "{ value }",
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
        name: "layout.use",
        about: "Shows a layout of the panels by its name, as the Layout menu does: Default, Profiling, or one saved.",
        args: &[arg("name", ArgKind::Text, "As the Layout menu names it.")],
        changes: false,
        returns: "{ layout }",
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

pub fn opt_pair(args: &Json, name: &str) -> Result<Option<[f32; 2]>, LinkError> {
    let Some(value) = args.get(name).filter(|value| !value.is_null()) else { return Ok(None) };
    let numbers = numbers(value).filter(|n| n.len() == 2).ok_or_else(|| bad(format!("`{name}` is two numbers")))?;
    Ok(Some([numbers[0], numbers[1]]))
}

pub fn opt_wholes(args: &Json, name: &str) -> Result<Option<Vec<u32>>, LinkError> {
    let Some(value) = args.get(name).filter(|value| !value.is_null()) else { return Ok(None) };
    let wholes = value.as_array().and_then(|list| {
        list.iter().map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok())).collect::<Option<Vec<u32>>>()
    });
    wholes.map(Some).ok_or_else(|| bad(format!("`{name}` is a list of whole numbers")))
}

pub(crate) fn numbers(value: &Json) -> Option<Vec<f32>> {
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
            | Part::Portal
            | Part::Collider2D
            | Part::Rigidbody2D
            | Part::Vehicle
            | Part::Wheel
            | Part::PostVolume
            | Part::ReflectionProbe
            | Part::ReverbProbe
            | Part::Lightmap
            | Part::LightProbeGroup
            | Part::WindZone
            | Part::ParticleSystem
            | Part::ForceField
            | Part::Smoke
            | Part::Mist
            | Part::VoxelVolume
            | Part::UiPanel
            | Part::ScreenUi
            | Part::LocalizedText
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
            | Part::NavAgent
            | Part::NavArea
            | Part::NavLink
            | Part::GroundFit
            | Part::StreamsWithGround
            | Part::Road
            | Part::AreaFill
            | Part::WorldArea
            | Part::AreaDefaults
            | Part::Route
            | Part::RouteStop
            | Part::Placement
            | Part::PrefabBrush
            | Part::Walkable
            | Part::NavObstacle
            | Part::CrowdMember
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
            | Part::TrackMaker
            | Part::WaterWake => true,
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

/// Every field of a component on an Obj, whichever of its tabs the
/// Inspector shows: a Block World's Build and Settings, a Terrain's and a
/// Foliage's tools. The Inspector lists one tab's at a time; a tool reaches
/// them all.
pub fn every_prop(world: &mut World, types: &ScriptTypes, entity: Entity, part: &Part) -> Vec<Prop> {
    fn over<T: Copy>(
        world: &mut World,
        tools: &[T],
        mut tool: impl FnMut(&mut World) -> &mut T,
        mut list: impl FnMut(&World) -> Vec<Prop>,
    ) -> Vec<Prop> {
        let shown = *tool(world);
        let mut all: Vec<Prop> = Vec::new();
        for &each in tools {
            *tool(world) = each;
            for prop in list(world) {
                if !all.contains(&prop) {
                    all.push(prop);
                }
            }
        }
        *tool(world) = shown;
        all
    }
    let list = |world: &World| props::props(world, entity, part, types);
    match part {
        Part::BlockWorld | Part::VoxelTerrain => {
            let tools = [crate::block_props::BlockTool::Settings, crate::block_props::BlockTool::Build];
            over(world, &tools, |world| &mut crate::block_props::ui_mut(world).tool, list)
        }
        Part::Terrain => {
            let tools = crate::terrain_props::TerrainTool::ALL;
            over(world, &tools, |world| &mut crate::terrain_props::ui_mut(world).tool, list)
        }
        Part::Foliage => {
            let tools = crate::plants_props::FoliageTool::ALL;
            over(world, &tools, |world| &mut crate::plants_props::ui_mut(world).tool, list)
        }
        _ => list(world),
    }
}

/// Each field of a component on an Obj with the name a tool gives it:
/// a built-in field by the name its locks give it, a script field by its
/// path, and a second field of the same name with `#2` after it.
pub fn named_props(game: &mut Game, types: &ScriptTypes, entity: Entity, part: &Part) -> Vec<(String, Prop)> {
    let mut named: Vec<(String, Prop)> = Vec::new();
    for prop in every_prop(&mut game.engine.world, types, entity, part) {
        let base = match &prop {
            Prop::Field { path, .. } => path_id(path),
            // One a prefab's locks have no name for, as most of a Block World's, by its row's label.
            other => Some(other.lock_key()).filter(|key| !key.is_empty()).unwrap_or_else(|| snake(&props::label(other))),
        };
        let taken = named.iter().filter(|(name, _)| *name == base || name.starts_with(&format!("{base}#"))).count();
        let name = if taken == 0 { base } else { format!("{base}#{}", taken + 1) };
        named.push((name, prop));
    }
    named
}

/// A label as a field's name: `Block size` is `block_size`.
fn snake(label: &str) -> String {
    let words: Vec<String> = label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    words.join("_")
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
pub fn resolve_prop(game: &mut Game, types: &ScriptTypes, entity: Entity, part: &Part, id: &str) -> Result<Prop, LinkError> {
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
            let target = match (script_target(game, types, entity, prop), props::obj_target(world, entity, prop)) {
                (Some(Target::Obj(target)), _) | (_, Some(target)) => Some(obj_ref(game, target)),
                _ => chosen(game, types, entity, prop, &label),
            };
            ("obj", target.unwrap_or(Json::Null), Some(("label", json!(label))))
        }
        Some(Value::Asset(label)) => {
            let path = match (script_target(game, types, entity, prop), props::asset_path(game, entity, prop)) {
                (Some(Target::Asset(path)), _) | (_, Some(path)) => Some(asset_ref(game, &path)),
                _ => chosen(game, types, entity, prop, &label),
            };
            ("asset", path.unwrap_or(Json::Null), Some(("label", json!(label))))
        }
        Some(Value::List(count)) => ("list", json!(count), None),
        Some(Value::Group) => ("group", Json::Null, None),
        Some(Value::Fixed(text)) => ("fixed", json!(text), None),
        Some(Value::Curve(curve)) => ("curve", json!(curve.keys()), None),
        Some(Value::Gradient(gradient)) => ("gradient", json!(gradient.keys()), None),
        None => ("none", Json::Null, None),
    };
    let mut field = json!({ "field": name, "label": props::label(prop), "type": kind, "value": value });
    if let Some((key, more)) = extra {
        field[key] = more;
    }
    if let Some(unit) = props::unit_on(world, entity, prop) {
        field["unit"] = json!(unit);
    }
    field
}

/// What a script's reference field points at.
fn script_target(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop) -> Option<Target> {
    let Prop::Field { behaviour, path } = prop else { return None };
    crate::fields::target(&game.engine.world, types, entity, behaviour, path)
}

/// A reference field's value as its menu has it, for one set by a key or a
/// name rather than an Obj or a file, as a Map's Open Map action is.
fn chosen(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, label: &str) -> Option<Json> {
    let choices = props::choices(game, types, entity, prop);
    let (_, pick) = choices.into_iter().find(|(shown, _)| shown == label)?;
    pick_json(game, &pick)
}

/// A choice of a reference field's menu as `field.choices` gives it.
pub fn pick_json(game: &Game, pick: &Pick) -> Option<Json> {
    match pick {
        Pick::Obj(Some(target)) => Some(obj_ref(game, *target)),
        Pick::Asset(Some(path)) => Some(asset_ref(game, path)),
        Pick::Key(Some(key)) => Some(json!({ "key": key })),
        Pick::Portal(portal) => Some(json!({ "dimension": portal.file, "obj": portal.portal.to_string() })),
        Pick::Obj(None) | Pick::Asset(None) | Pick::Key(None) => None,
    }
}

/// A Portal in a dimension's file as a tool names it, `{ dimension, obj }`,
/// its file by path or GUID and its Obj by its id, written as `pick_json`
/// writes it, so the two compare.
fn file_portal_json(game: &Game, json: &Json) -> Option<Json> {
    let dimension = json.get("dimension")?;
    let (_, path, kind) = resolve_project_asset(game, dimension).ok()?;
    let obj = match json.get("obj")? {
        Json::Number(id) => id.to_string(),
        Json::String(path) => path.clone(),
        _ => return None,
    };
    (kind == AssetKind::Dimension).then(|| json!({ "dimension": path, "obj": obj }))
}

/// The choice of a reference field's menu a tool's value names: by the
/// value `field.choices` gives it, or by its label.
pub fn offered_pick(game: &Game, types: &ScriptTypes, entity: Entity, prop: &Prop, json: &Json) -> Option<Pick> {
    let choices = props::choices(game, types, entity, prop);
    let json = &file_portal_json(game, json).unwrap_or_else(|| json.clone());
    let by_value = choices.iter().find(|(_, pick)| pick_json(game, pick).is_some_and(|value| value == *json));
    let by_label = || json.as_str().and_then(|text| choices.iter().find(|(label, _)| label == text));
    by_value.or_else(by_label).map(|(_, pick)| pick.clone())
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
        AssetKind::Dimension => ("dimension", true),
        AssetKind::Level => ("level", true),
        AssetKind::Prefab => ("prefab", true),
        AssetKind::Ui => ("ui", true),
        AssetKind::Sound => ("sound", false),
        AssetKind::InputActions => ("inputactions", true),
        AssetKind::Strings => ("strings", true),
        AssetKind::Font => ("font", false),
        AssetKind::Paths => ("paths", false),
        AssetKind::ReflectionProbes => ("probes", false),
        AssetKind::Lightmaps => ("lightmaps", false),
        AssetKind::AssetTable => ("assettable", true),
        AssetKind::LightProfile => ("lightprofile", true),
        AssetKind::Lighting => ("lighting", true),
        AssetKind::LookPack => ("lookpack", true),
        AssetKind::ShaderPack => ("shaderpack", true),
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
        AssetKind::VoxShade => ("voxshade", true),
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
