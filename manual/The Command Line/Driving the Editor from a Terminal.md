---
order: 1
---

While the editor is open, `vox editor call` can do from a terminal what you do with the mouse: make Objs, change their components, run Play, read the Console and take a picture of the Scene view. Scripts, automated tests and AI assistants use it to work on a game and check the result without anyone clicking.

Every change goes through the editor's undo, the same as your own edits. Whatever a tool did, **Ctrl+Z** steps back through it.

## A first call

Open the project in the editor, then in a terminal:

```bash
vox editor call editor.state --project MyGame
```

The editor answers in JSON:

```json
{
  "project": "MyGame",
  "level": "Levels/Day.level",
  "scenes": [{ "guid": "713a4d989c654c6ca23b1d63201e0090", "path": "Scenes/Day.scene" }],
  "playing": false,
  "unsaved": false,
  "selection": [],
  "undo": 0,
  "redo": 0
}
```

Run from inside the project's folder, you can leave out `--project`.

Arguments come after the call's name as `name=value` pairs. A value that reads as JSON is taken as JSON, so `position=[0,2,0]` is three numbers and `active=false` is a toggle; anything else is text. You can give the arguments as one JSON object instead: `'{ "kind": "Cube" }'`.

## Naming Objs, components and fields

**An Obj** is named by the scene it is in and its id there, as `objs.list` gives it:

```json
{ "scene": "713a4d989c654c6ca23b1d63201e0090", "path": "3" }
```

The path is the `id` the scene file gives the Obj. An Obj inside a prefab instance has a longer path, as `3/2`. This name stays the same when you undo, save, or play and stop, which the Obj's place in the Hierarchy does not. On the command line you can also write it short, as `713a4d989c654c6ca23b1d63201e0090:3`.

**An asset** is named by its GUID or its path in the project: `{ "guid": "…" }`, `{ "path": "Materials/Stone.material" }`, or either one written plain.

**A component** is named as scene files name it: `light`, `renderer`, `collider`, or a script's full type name, as `Game.Spinner`. `vox editor call components` lists every one the project can add.

**A field** is named as `obj.get` lists it: `intensity` on a Light, or for a script's field, its name, with a dot before each step into a list or a group, as `waves.2.count`.

## A worked example: a lamp

Make a point light called Lamp, three metres up:

```bash
vox editor call obj.create kind="Point Light" name=Lamp position=[0,3,0]
```

```json
{ "obj": { "scene": "713a4d989c654c6ca23b1d63201e0090", "path": "7" } }
```

Look at what it has:

```bash
vox editor call obj.get obj=713a4d989c654c6ca23b1d63201e0090:7
```

The answer lists each component with every field the Inspector shows, its value, and what kind of value it is. Here it is cut short to three fields:

```json
{
  "name": "Lamp",
  "position": [0.0, 3.0, 0.0],
  "components": [{
    "component": "light",
    "title": "Light",
    "enabled": true,
    "fields": [
      { "field": "type", "label": "Type", "type": "choice", "value": "Point", "options": ["Directional", "Point", "Spot", "Rectangle", "Disc", "Tube"] },
      { "field": "color", "label": "Colour", "type": "color", "value": [1.0, 1.0, 1.0] },
      { "field": "intensity", "label": "Intensity", "type": "number", "value": 1.0 }
    ]
  }]
}
```

Make it brighter and warmer:

```bash
vox editor call field.set obj=713a4d989c654c6ca23b1d63201e0090:7 component=light field=intensity value=6
vox editor call field.set obj=713a4d989c654c6ca23b1d63201e0090:7 component=light field=color value='"#ffb060"'
```

See how it looks:

```bash
vox editor call capture view=scene path=lamp.png
```

Not what you wanted? Take the colour back:

```bash
vox editor call undo
```

The Lamp stays in the editor, not yet in the scene's file, until you save. Save from the editor, or with `vox editor call save`.

## Values

A field takes its value in the same form `obj.get` shows it:

| Kind | Written as |
| --- | --- |
| `number`, `whole` | `6`, `0.25` |
| `toggle` | `true`, `false` |
| `text` | `"Hello"` |
| `vector` | `[0, 2, 0]` |
| `color` | `[1, 0.7, 0.4]`, from 0 to 1, or `"#ffb060"` |
| `choice` | one of its options, as `"Spot"` |
| `obj` | an Obj, as above, or `null` for none |
| `asset` | an asset, as above, or `null` for none |
| `curve` | `[[0, 1], [1, 0]]`: where along it, from 0 to 1, and the value there |
| `gradient` | `[[0, 1, 1, 1], [1, 0, 0, 0]]`: where along it, then red, green and blue |

A reference field takes only what its menu in the Inspector offers. `field.choices` lists those. A list changes by its items, with `list.add` and `list.remove`.

## Several changes as one step

`batch` makes several calls in order as one step to undo. Give it the calls on stdin, one JSON object a line:

```bash
vox editor call batch <<'EOF'
{ "call": "obj.create", "args": { "kind": "Cube", "name": "Crate" } }
{ "call": "obj.create", "args": { "kind": "Cube", "name": "Crate" } }
{ "call": "save" }
EOF
```

It stops at the first call that fails, and keeps what came before it.

## When no editor is open

With no editor open on the project, a call says so and ends with exit code 3. Give `--headless` to open the editor without a window for the call instead, working on the project's files:

```bash
vox editor call --headless batch < changes.jsonl
```

A headless editor saves nothing unless a call saves. So for more than one call, use `batch` with `save` last.

`vox editor serve --project MyGame` keeps a headless editor open, answering calls until one closes it with `editor.quit`. If you open the editor while one is serving, the editor you open takes over: the headless one saves and closes.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Done. The answer is on stdout. |
| 1 | The call failed, or its arguments were wrong. Its error is on stdout as JSON, and in words on stderr. |
| 2 | The command line was wrong. |
| 3 | No editor is open on the project. |
| 4 | Refused: the game is playing, the call would lose edits not saved, or the editor lets tools only read. |

An error has a `kind` a script can test for: `no_such_obj`, `no_such_component`, `no_such_field`, `wrong_value`, `unsaved_conflict`, `refused_while_playing`, `not_allowed`, and a few more.

## Every call

`vox editor call --describe` lists every call with its arguments. `--describe --json` gives the same as JSON, and with an editor open it adds the components, palette commands and kinds of Obj and file the project has. That way a tool can find out for itself what it may do.

| Calls | What they do |
| --- | --- |
| `editor.state`, `objs.list`, `obj.get`, `components`, `kinds` | Read what is open, the Objs and their components |
| `obj.create`, `obj.set`, `obj.duplicate`, `obj.delete`, `obj.move` | Make, change, copy, delete and move Objs |
| `component.add`, `component.remove`, `component.enable` | Add, take off, and switch components on or off |
| `field.set`, `field.reset`, `field.choices`, `list.add`, `list.remove` | Change fields, as the Inspector does |
| `undo`, `redo`, `batch` | Step back and forward, and make several changes one step |
| `select`, `frame` | Select Objs, and show them in the Scene view |
| `commands`, `command.run` | Run anything the command palette runs, by its id |
| `save`, `level.open`, `scene.open_alongside`, `scene.reload`, `files.look` | Save, open levels and scenes, and read files changed outside the editor |
| `assets.list`, `asset.get`, `asset.create`, `asset.rename`, `asset.move`, `asset.duplicate`, `asset.delete` | Work with the project's files, as the Project panel does |
| `play`, `pause`, `step`, `stop`, `game.console` | Run the game, and type into its console |
| `console.read` | Read the Console's lines, from where the last read stopped |
| `capture` | Save a picture of the Scene view, the Game view or the whole editor |
| `scripts.build` | Build the C# scripts and get back each error with its file and line |
| `settings.get`, `settings.set`, `project.get` | The editor's settings, and the project's |
| `docs.search`, `docs.read` | Find and read pages of this manual and your own |
| `editor.wait`, `editor.quit` | Wait a few frames, and close the editor |

## Files changed outside the editor

A tool can also write files itself and let the editor pick them up:

- **A scene or level that is open**, written while you have no edits waiting, is read again, keeping the selection. Undo starts afresh, as it does when you open a level.
- **With edits waiting**, the editor leaves the file alone and says so in the status bar. Saving keeps your edits over the file's. `scene.reload` with `discard=true` reads the file and drops your edits.
- **While the game plays**, nothing is read until it stops.
- **`voxear.toml`** is read again, and Project Settings show what it now says.
- **Other assets**, such as materials and prefabs, are read again when you switch back to the editor, and C# scripts build again within a few seconds of changing.

The editor looks at the open scenes and `voxear.toml` twice a second. `files.look` tells it to look at every file at once, as a script should after writing some.

## Letting tools only read

In **Settings**, under **Tools**, turn off **Let tools change the project**. Tools can still read anything, but every call that would change something is refused with exit code 4. A tool can turn the setting off for itself, but only you can turn it back on.

Each change a tool makes writes a line to the Console, starting `[Tools]`, so you can see what was done and when.

## An AI assistant

`vox mcp` serves the same calls as tools for an AI assistant, over the Model Context Protocol, on stdin and stdout. Most assistants that speak it take the command that starts a server:

```bash
vox mcp --project /home/me/Games/MyGame
```

Each call is a tool, with `_` in its name in place of `.`: `obj_get`, `field_set`. One more tool, `vox`, runs the command line's other commands, such as `render` and `build`. `capture` hands the assistant the picture itself, so it sees what it changed.

With the editor open, the assistant works in it while you watch, and you can undo anything it did. With no editor open, `vox mcp` starts a headless one for it and closes it when the assistant goes. In that case, ask it to save what it should keep. `--no-headless` makes it work only with an editor you open.

## Writing your own client

`vox editor call` is one client of a simple protocol, and any program can speak it:

1. Read `.voxear/Editor` in the project's folder. Only you can read it, and it says, as JSON, the `address` the editor listens on and a `secret`. The editor writes it when it opens and removes it when it closes.
2. Connect to the address. It is always on this computer, so other machines cannot reach it.
3. Send the secret on the first line.
4. Send JSON-RPC 2.0 requests, one a line, as `{ "jsonrpc": "2.0", "id": 1, "method": "obj.get", "params": { "obj": "…" } }`, and read each answer's line.

A connection can stay open for as many calls as you like. Calls are answered in order. A call that waits on the editor, as `play` and `capture` do, answers once it is done.

## What tools cannot do yet

Some of the editor's work is done with brushes and canvases rather than fields:

- sculpting and painting terrain
- placing and digging blocks in a Block World
- painting plants
- editing a World Generator's graph
- building a box model in the Model window

For these, a tool can write the files themselves (the `.terrain`, `.blockworld` or `.worldgen` file) and call `files.look`. Calls for them come later.
