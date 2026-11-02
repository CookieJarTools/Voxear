---
order: 2
---

A **scene** is a file of Objs: a room, a stretch of landscape, the game's menus. A **level** is a list of scenes that load together, such as "the first floor of the castle". The game always loads a level, never a scene on its own.

Splitting a level into scenes has two uses:

- **Sharing.** Every level can use the same scene for the things that are always there, such as the player and the music, instead of each holding its own copy.
- **Working together.** Two people can change two scenes of the same level at once without their changes getting in each other's way.

A small game can have one scene per level, and that is fine.

## Opening a level

Click the top button on the strip at the far left of the editor to open **Levels**. It lists every level, and every scene that is in no level. Click a level to open it, or click its play button to open it and play straight away.

You can also double-click a level in the Project panel, or find it with the command bar (Ctrl+K).

The level the game starts with is `start_level` in your project's `voxear.toml`.

## Saving a level

**Ctrl+S** saves the level you are editing. Only the scenes you changed are written. One scene in each level is its **active scene**: new Objs you make go into it.

## Opening a scene alongside a level

You can open another scene next to the ones already open, to see the two together or to change both at once. Unity calls this opening a scene additively.

Right-click the scene in the Project panel and choose **Open Additively**. You can also drag the scene onto the Hierarchy or the Scene view.

The scene opens with the others, and its Objs show in the Hierarchy under its name. While it is not part of the level, its name is marked **Not in level**. Change it and save it as you would any scene. New Objs still go into the level's active scene.

Right-click the scene's name in the Hierarchy to choose what happens to it:

- **Add to the … Level** makes the scene part of the level you are editing. The level opens it from now on.
- **Close Scene** closes it, and any other scene you no longer need open. Save first, or its changes go with it.

Opening another level closes the scenes you opened alongside.

## Making a level

1. Right-click in the Project panel and choose **New Scene**, and type its name. Do the same with **New Level**.
2. Click the new level in the Project panel. The Inspector shows its scenes, which is none yet.
3. Click **Add Scene** and pick your scene from the list. Type to search it. You can also drag the scene from the Project panel onto **Add Scene**.

Add as many scenes as the level needs. Each scene in the list is a field, like a reference field on a script. Click it to find the scene in the Project panel. To put another scene in its place, click the arrow at its end, or drag a scene onto it. To take one out, click the cross at the end of its row. To change the order, drag a scene by the dots at its left: the others slide out of its way. The level's file changes straight away. If the level is open, the Hierarchy shows the change too.

If the level is open, you can also open the scene alongside it and add it from the Hierarchy, as "Opening a scene alongside a level" above shows.

Each scene in the list has three boxes, and a weight:

- **Active**: new Objs you make go into this scene, and the level uses this scene's sky. A level always has one active scene. Until you tick another, it is the first.
- **Required**: if a required scene fails to load, the level does not load. Untick it to let the level load without that scene.
- **Reload**: the scene loads again whenever the game moves to this level, even when the level before had it loaded too. Use it for a scene that should start fresh each time, such as a puzzle room. Without it, a scene both levels share stays as it was.
- **Weight**: how much the scene counts for in a loading bar while the level loads, against its other scenes. A scene twice as big as the rest might weigh 2. Type it and press **Enter**.

A level is a small text file. The Inspector writes it for you, but you can also write it by hand:

```toml
# Voxear level
format = 1

[[scene]]
scene = "Scenes/Castle.scene"

[[scene]]
scene = "Scenes/Castle Lights.scene"
active = true
required = false
reload = true
weight = 2.0
```

The Inspector names each scene by its ID from its `.meta` file, with the scene's path in a comment beside it. That way, renaming the scene does not break the level.

## Levels that stand on other levels

Most games have things every level needs: the player, the music, the on-screen display. Put them in a level of their own, then have your other levels **stand on** it:

```toml
# Voxear level
format = 1
stands_on = "Levels/World.level"

[[scene]]
scene = "Scenes/Day.scene"
```

To choose the level it stands on, select the level in the Project panel and use **Stands on** at the top of its scenes, as you would a scene: pick from its arrow, drag a level onto it, or pick **None**. A level that already stands on this one cannot be chosen, as the two would go round in a circle.

The scenes of the level stood on load first. When the game moves from one level to another that stands on the same one, those scenes stay loaded, untouched. Only the scenes that differ are unloaded and loaded. A scene marked **Reload** loads again anyway.

## Objs that stay from level to level

Most Objs leave when their level does. An Obj marked as persistent stays through every change of level, children and all.

To mark one, select it in the Hierarchy and tick **Persistent** in the Inspector. Then save. The scene file now says so:

```toml
[[obj]]
id = 7
name = "Music"
persistent = true
```

Unlike in some other engines, going back to the scene a persistent Obj came from does not make a second copy of it.

In the editor, opening another level always unloads the one you were in, persistent Objs included. They only stay while the game runs.

## Changing level from code

```csharp
Level.Load("Levels/Night.level");
```

See [[Loading Levels]] for loading in the background, with a loading screen.
