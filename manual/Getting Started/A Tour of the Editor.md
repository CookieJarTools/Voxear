---
order: 3
---

# A Tour of the Editor

When a project opens, the editor shows its first level, ready to change. Nothing in the game runs yet: your scripts start when you press Play.

## The parts of the window

- **Hierarchy**, on the left: every Obj in the open level, as a tree. An Obj is one thing in your game's world, such as a light, a crate or the player. See [[Objs and Components]].
- **Scene**, in the middle: the level as you edit it. Click things here to select them, and drag their handles to move them.
- **Game**, beside Scene: the level through the game's camera, as a player will see it.
- **VoxDocs**, beside Game: your project's own pages, and this manual.
- **Project** and **Console**, under the Scene: the files in your project, and the messages your game and the editor write.
- **Inspector**, on the right: everything about the Obj or file you have selected, ready to change.

Along the top are the **command bar**, the **Play**, **Pause** and **Step** buttons, the **Window** button that lists every panel, and **Build**. The narrow strip on the far left opens **Levels**, to open another level, and **Settings**.

The bar at the bottom says what the editor is doing: building your scripts, playing, or that you have changes to save.

## The command bar

Press **Ctrl+K** and start typing. The command bar finds commands, settings, Objs in the level, files and pages, and runs or opens the one you pick. It is the quickest way to do almost anything, and it shows each command's shortcut, so you learn them as you go.

## Moving around the Scene

Hold the **right mouse button** over the Scene view to fly:

| Key | Does |
| --- | --- |
| W, A, S, D | Forward, left, back, right |
| E and Q | Up and down |
| Shift | Three times as fast |
| Mouse wheel | Faster or slower, for as long as you fly |

Move the mouse while you hold the button to look around. Let go to stop flying.

## Selecting and moving things

Click an Obj in the Scene or the Hierarchy to select it. Then pick a tool, at the top left of the Scene view:

| Key | Tool |
| --- | --- |
| Q | Select only |
| W | Move: drag an arrow to move along it, or the middle to move freely |
| E | Rotate: drag a ring to turn around it |
| R | Scale: drag a block to stretch along it, or the middle to scale evenly |

Hold **Ctrl** while you drag to snap: moves go in steps of the size shown in the Scene view (0.25 m to begin with), turns in steps of 15°, and scaling in tenths. **Pivot** and **Global**, beside the tools, choose where the handles sit and which way they point.

## Undo and saving

- **Ctrl+Z** undoes and **Ctrl+Y** redoes. A whole drag is one step.
- **Ctrl+S** saves the level. Only the scenes you changed are written.
- If you open another level with changes not saved, the editor asks you to save first, or to click again to leave them.

## Arranging the panels

Drag a panel's tab to move it somewhere else in the window. Right-click a tab to close it, to float it in a window of its own, or to **Reset Layout**. The **Window** button at the top lists every panel, so you can bring back one you closed.

**World** and **Profiling**, at the top left, are two layouts, each remembered on its own. Profiling holds the tools for finding out why a game runs slowly.

## Shortcuts

| Key | Does |
| --- | --- |
| Ctrl+K | Command bar |
| Ctrl+S | Save |
| Ctrl+Z | Undo |
| Ctrl+Y, or Ctrl+Shift+Z | Redo |
| Ctrl+D | Duplicate the selection |
| Delete | Delete the selection |
| Q, W, E, R | Select, Move, Rotate and Scale tools |

Next: [[Playing Your Game]].
