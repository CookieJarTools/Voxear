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

## Making a level

1. Right-click in the Project panel and choose **New Scene**, and type its name. Do the same with **New Level**.
2. Open the new `.level` file in a text editor and list its scenes:

```toml
# Voxear level
format = 1

[[scene]]
scene = "Scenes/Castle.scene"

[[scene]]
scene = "Scenes/Castle Lights.scene"
```

The first scene is the active one. Put `active = true` on another to make it active instead. The level uses the active scene's sky.

A scene marked `required = false` is left out, rather than stopping the level, if it fails to load.

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

## Objs that stay from level to level

Most Objs leave when their level does. An Obj marked as persistent stays through every change of level, children and all. Mark it in its scene file:

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
