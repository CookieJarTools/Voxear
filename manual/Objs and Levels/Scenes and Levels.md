---
order: 2
---

# Scenes and Levels

A **scene** is a file of Objs: a room, a stretch of landscape, the game's menus. A **level** is a list of scenes that load together, such as "the first floor of the castle". The game always loads a level, never a scene on its own.

Splitting a level into scenes has two uses:

- **Sharing.** Every level can use the same scene for the things that are always there, such as the player and the music, instead of each holding its own copy.
- **Working together.** Two people can change two scenes of the same level at once without their changes getting in each other's way.

A small game can have one scene per level, and that is fine.

## Opening a level

Click the top button on the strip at the far left of the editor to open **Levels**. It lists every level, and every scene that is in no level. Click a level to open it, or click its play button to open it and play straight away.

You can also double-click a level in the Project panel, or find it with the command bar (Ctrl+K).

The level the game starts with is `start_level` in your project's `voxear.toml`.

## Saving

**Ctrl+S** saves the level you are editing. Only the scenes you changed are written. One scene in each level is its **active scene**: new Objs you make go into it.

## Making a level

1. Right-click in the Project panel and choose **New Scene**, and type its name. Do the same with **New Level**.
2. Click the new level in the Project panel. The Inspector shows its scenes, which is none yet.
3. Click **Add Scene** and pick your scene from the list.

Add as many scenes as the level needs. To take one out, click the cross at the end of its row. The level's file changes straight away. If the level is open, the Hierarchy shows the change too.

Each scene in the list has two boxes:

- **Active**: new Objs you make go into this scene, and the level uses this scene's sky. A level always has one active scene. Until you tick another, it is the first.
- **Required**: if a required scene fails to load, the level does not load. Untick it to let the level load without that scene.

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

The scenes of the level stood on load first. When the game moves from one level to another that stands on the same one, those scenes stay loaded, untouched. Only the scenes that differ are unloaded and loaded.

## Objs that stay

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
