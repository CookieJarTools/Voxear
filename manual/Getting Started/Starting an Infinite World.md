---
order: 7
---

The **Infinite World** template starts a project in a world of blocks that goes on without end, with a player to walk it. Pick it under **Template** when you make a project in the launcher, or from a terminal:

```bash
vox new "Far Lands" --template infinite-world
vox editor "Far Lands"
```

Press Play and you stand in it. Nothing of the world is saved in the project: its ground is made round the player as they go, from a generator and its settings, so the whole template is a fraction of a megabyte.

## What is in it

| What | Where |
| --- | --- |
| The world: a [[Block Worlds|Block World]] without end, its land in [[Block Worlds#Levels|levels]] of long slopes and low walls, with rivers, lakes and a sea, and a [[Block Worlds#The floor|floor]] at its bottom | `Blocks/Endless.blockworld`, on the **World** Obj |
| Eleven [[Biomes|biomes]], from snow and tundra through forest, blossom groves and grassland to savanna, desert and volcanic fields, each a region of its own | `Biomes/` |
| The blocks they are made of, the trees that grow in them, and plants on the ground | `Blocks/`, `Plants/` |
| [[Structures]] the world builds as it goes: trees, lone oaks, villages and dungeons | `Structures/` |
| A player of blocks you walk with, and a camera that follows them | the **Player** and **Main Camera** Objs, `Scripts/` |
| A map and a minimap of what has been seen | the **Map** Obj |
| Weather that comes and goes | the **Weather** Obj |
| A sun and sky | `Scenes/Sky.scene` |

The level is `Levels/Infinite World.level`. The project switches on the **Terrain**, **Vegetation** and **Water** [[Modules|modules]].

## Playing it

| Do | Keys | Gamepad |
| --- | --- | --- |
| Walk | W A S D | Left stick |
| Look | Mouse | Right stick |
| Run | Left Shift | Left stick press |
| Jump | Space | South button |
| Open a door, pull a lever | E | West button |
| Zoom the camera in and out | Mouse wheel | |
| Open the map | M | |
| Free the pointer, and hold it again | Escape, then click | |

The keys are the project's [[Reading Input|input actions]], in `Input.inputactions`, so you change them there.

## Making it your own

Select the **World** Obj and its settings are in the Inspector:

- **Levels** sets how the land climbs: how many levels, from what height to what, how wide an area is, and how often plains, hills, canyons, spires, peaks and basins come.
- **Biomes** lists the biomes; open one to change its blocks, its plants and the land it shapes.
- **View distance** and **Far distance** say how far the world's own blocks and its far view reach. A player's machine can draw further or nearer with its own [[Block Worlds#Quality and detail|world quality]].
- **Generator** is the world generator the ground comes from: open it to change the land itself in its graph.

Add a structure to its list to have the world build it, or take one off to stop it. The scripts in `Scripts/` are yours to change: `PlayerController` walks the player, `OrbitCamera` follows them, `UsesBlocks` lets them use doors and levers, and `MapOpenAction` opens the map.
