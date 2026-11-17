---
order: 9
---

**Preferences** holds the editor's own settings: how it looks, how the Scene view's camera and handles behave, how Play starts, which code editor scripts open in. They are yours, not the game's: they are kept in the project's `.voxear/Editor.toml`, which stays out of version control, so everyone working on a project sets their own. The game's settings are in [[Project Settings]].

Open it from the side bar on the far left (the button at the bottom, under Project Settings), or type *Preferences* in the command bar. It opens over the left of the editor and is laid out as Project Settings is: the categories down the left, a search across the top that looks through every category and every tooltip, a tooltip on each setting with its default, a bar beside a setting changed from its default, and **Reset to Default** on a setting's right-click menu.

## Categories

| Category | What is in it |
| --- | --- |
| General | Whether prefabs in the Project panel open out to the Objs they hold |
| Appearance | The theme, Dark, Light or Custom, and Custom's base colour, accent colour and contrast |
| Console | Row height, colouring by kind, alternating rows and categories; namespaces, parameters and My code only in traces; how many lines are held, and clearing on Play. **Open Console Settings** opens the Console's own settings, with a live sample, where its colours are picked (see [[The Console]]) |
| External Tools | The code editor scripts open in, and the folder watched for Megascans downloaded from Fab (see [[Megascans from Fab]]) |
| Keys | Every command that has keys, and its keys. They cannot be changed yet |
| Play Mode | Start paused, save before Play, reload scripts while playing, pause on error; and for networked Play, playing as a host or a server's player, how many test players, and how bad their network is (see [[Playing Your Game]]) |
| Scene View | The camera's fly speed and speeding up; the handles' snap, position and space; shading, stats and the axis gizmo; and which gizmos show |
| Tools | Whether programs reaching the editor through `vox editor call` and `vox mcp` may change the project (see [[Driving the Editor from a Terminal]]) |

The command bar finds these settings too: pick a toggle to flip it, a choice to move it on, and anything else to open Preferences at it.

## From the command line

```
vox editor call settings.get category="Scene View"
vox editor call settings.set key=fly_speed value=12
vox editor call settings.set key=code_editor value=Rider
vox editor call settings.show key=snap
```

`settings.get` lists each setting with its key, category, section, label, tooltip, type, value, default, and its options or range. `settings.set` takes a choice by its label, and `null` for the default.
