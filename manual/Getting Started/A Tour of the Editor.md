---
order: 3
---

When a project opens, the editor shows its first level, ready to change. Nothing in the game runs yet: your scripts start when you press Play.

## The parts of the editor window

- **Hierarchy**, on the left: every Obj in the open level, as a tree. An Obj is one thing in your game's world, such as a light, a crate or the player. See [[Objs and Components]].
- **Scene**, in the middle: the level as you edit it. Click things here to select them, and drag their handles to move them.
- **Game**, beside Scene: the level through the game's camera, as a player will see it.
- **VoxDocs**, beside Game: your project's own pages, and this manual.
- **Project** and **Console**, under the Scene: the files in your project, and the messages your game and the editor write.
- **Inspector**, on the right: everything about the Obj or file you have selected, ready to change.

Along the top are the **command bar**, the **Play**, **Pause** and **Step** buttons, the **Window** button that lists every panel, and **Build**. The narrow strip on the far left, the side bar, opens **Levels**, to open another level, and **Settings**, and holds any buttons your game adds (see [[Side Bar Items]]).

The bar at the bottom says what the editor is doing: building your scripts, playing, or that you have changes to save.

In the **Project** panel, drag a file or folder onto a folder, among the files or in the tree on the left, to move it there; drag it onto a folder higher up the tree to move it out. Its `.meta` file goes with it, so everything that uses it still finds it. The slider at the top right sets how big the files show, as in Unity: all the way left shows them as rows.

## The command bar

Press **Ctrl+K** and start typing. The command bar finds commands, settings, Objs in the level, files and pages, and runs or opens the one you pick. It is the quickest way to do almost anything, and it shows each command's shortcut, so you learn them as you go.

## Moving around the Scene

Hold the **right mouse button** over the Scene view to fly:

| Key | What it does |
| --- | --- |
| W, A, S, D | Forward, left, back, right |
| E and Q | Up and down |
| Shift | Three times as fast |
| Mouse wheel | Faster or slower, for as long as you fly |

Move the mouse while you hold the button to look around. Let go to stop flying.

To find an Obj, select it and press **F**, or double-click it in the Hierarchy. The camera glides to show it whole, looking the same way it did. **Ctrl+Shift+F** does the opposite, as in Unity: it moves the selected Objs to where the camera is, facing the way it looks, which is handy for placing a camera or a light just where you are looking from.

## Selecting and moving Objs

Click an Obj in the Scene or the Hierarchy to select it. Then pick a tool, at the top left of the Scene view:

| Key | Tool |
| --- | --- |
| Q | Select only |
| W | Move: drag an arrow to move along it, a square to move along its two axes, or the middle to move freely |
| E | Rotate: drag a ring to turn around it, the outer ring to turn around the way you look, or inside the rings to turn freely |
| R | Scale: drag a block to stretch along it, a square to stretch along its two axes, or the middle to scale evenly |

Hold **Ctrl** while you drag to snap: moves go in steps of the size shown in the Scene view (0.25 m to begin with), turns in steps of 15°, and scaling in tenths. **Pivot** and **Global**, beside the tools, choose where the handles sit and which way they point.

To select several Objs, hold **Ctrl** and click each in the Scene view or the Hierarchy; Ctrl and a click on one already selected takes it out again, and **Shift** and a click in the Hierarchy selects every Obj between. You can also drag a box over the Scene view from an empty spot: everything it touches is selected, added to what was with Ctrl or Shift held. The handles sit on the Obj you selected first, and the rest move, turn and scale with it, all as one step to undo. **Delete** and **Ctrl+D** act on them all, and the Scene view outlines each one while there are several. The Inspector shows the first Obj, with how many it edits beside its name. It shows only the components every selected Obj has, and a value they hold differently shows **—**. A value you change there goes to every selected Obj, and for Position, Rotation, Scale and other values with several parts, only the part you typed in changes on each.

## Undo and saving

- **Ctrl+Z** undoes and **Ctrl+Y** redoes. A whole drag is one step.
- **Ctrl+S** saves the level. Only the scenes you changed are written.
- If you open another level with changes not saved, the editor asks you to save first, or to click again to leave them.

## Arranging the editor's panels

Drag a panel's tab to move it somewhere else in the window. Right-click a tab to close it, to float it in a window of its own, or to **Reset Layout**. The **Window** button at the top lists every panel, so you can bring back one you closed.

**Float**, on the same menu, takes a panel out of the editor into a window of its own, which you can put anywhere, on another screen too. Letting go of a dragged tab where it cannot dock does the same, as does dragging it past the edge of the editor's window. A Scene or Game view keeps drawing there. Drag the tab back onto the editor to dock it again, or close the window to close its panels. The editor remembers the window with the layout, so it opens again next time.

To see the level from two places at once, choose **Add Scene View** from a panel's options button (the **⋯** at the right end of its tabs) or from a right-click on a tab. Each Scene view has its own camera and its own shading menu, so one can show Wireframe while another shows Lit. The tools work in whichever Scene view the pointer is over. **Add Inspector** works the same way, and each extra Inspector has a padlock that keeps it on the Obj it shows. **Add Hierarchy** opens another Hierarchy, with its own filter, and **Add Project** opens another Project panel, so you can have two folders open and drag between them. An extra window is gone for good once you close it.

To select nothing, click the empty space below the Objs in the Hierarchy.

Once you have clicked in the Hierarchy, the Project panel, the Console or any other list, the arrow keys move through it: **Up** and **Down** a row at a time, with **Shift** to select every row on the way, **Home** and **End** to the first and last, and **Page Up** and **Page Down** a screenful at a time. In a tree, **Right** opens what is selected, or goes inside it once it is open, and **Left** closes it, or goes to what it is in.

The **Layout** button at the top left, which shows the name of the layout in use, switches between arrangements of the panels. **Default** is the one described here; **Profiling** holds the tools for finding out why a game runs slowly. Each layout stays as you left it. **Save Layout As** keeps your own arrangement under a name, to switch back to later, and **Reset Layout** puts the one in use back as it started.

## Editor shortcuts

| Key | What it does |
| --- | --- |
| Ctrl+K | Command bar |
| Ctrl+S | Save |
| Ctrl+Z | Undo |
| Ctrl+Y, or Ctrl+Shift+Z | Redo |
| Ctrl+D | Duplicate the selection |
| Delete | Delete the selection |
| Q, W, E, R | Select, Move, Rotate and Scale tools |
| F | Frame the selection in the Scene view |
| Ctrl+Shift+F | Move the selection to the Scene view's camera |

Next: [[Playing Your Game]].
