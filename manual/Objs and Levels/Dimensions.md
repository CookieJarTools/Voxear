---
order: 6
---

A **dimension** is a world of its own inside the game: its own [[Block Worlds|Block World]], its own sky, sun and weather, and its own coordinates. Two dimensions can use the same coordinates and never meet. Nothing in one is drawn, lit, heard, collided with or made round a player in the other. Players go from one to another through [[Portals]].

The level's own scenes are its first dimension. Every level has it, and a level made before dimensions is that one alone. Any other dimension is a `.dimension` file.

## A dimension's file

A `.dimension` file is written as a [[Scenes and Levels|scene]] is: its Objs, its lighting, its `[sky]` and `[post]`. On top of that, a `[dimension]` table holds its settings. Its Objs are its world: a Block World for its ground, its lights, its sounds, whatever stands in it.

```toml
format = 1

[dimension]
kind = "caves"

[[obj]]
id = 1
name = "Rock"
[obj.block_world]
data = "Blocks/Deep Rock.blockworld"

[[obj]]
id = 2
name = "Way Back"
position = [32.0, 12.0, 32.0]
[obj.portal]
size = [3.0, 4.0]
```

| Setting | What it does |
| --- | --- |
| **Kind** | What the dimension is for, which sets good values for the rest. **Overworld**: open land under a sky of its own, with weather, on the level's clock. **Dungeon**: enclosed, one for each place that opens it. **Caves**: enclosed, only one |
| **Sky** | Whether it has a sky. Without one it is enclosed: dark all round, no sun and no light from the sky, only its own lamps and glowing blocks. With one, its sky is its file's lighting, `[sky]` and `[post]`, or the level's where it has none |
| **Weather** | Whether its lighting's weather runs in it. Without, it is always clear |
| **Clock** | Whose time of day it keeps. **Level**: its sun stands where the level's does. **Own**: its first Climate's. **None**: its sun stays where it was put |
| **Keep loaded** | How many seconds it stays loaded after nothing needs it any more: 30 |
| **Instances** | **One**, or **Each site**: one for each place that opens it, as each dungeon is its own |

## Portals into a dimension

A Portal in the level leads into a dimension when its **Leads To** is a Portal in that dimension's file. In the scene file that is `into`, the `.dimension` and the Portal's Obj in it:

```toml
[[obj]]
id = 2
name = "Gate"
position = [32.0, 4.0, 20.0]
rotation = [0.0, 180.0, 0.0]

[obj.portal]
size = [3.0, 4.0]
into = { dimension = "Dimensions/Caves.dimension", portal = 2 }
```

The dimension is loaded once a camera comes near enough to see through the Portal. Its Portal then leads back too, if it led nowhere. The Portal shows its mist until the far side's ground is made, then the far side, as it shows any other.

A Portal into another dimension works like one within a world:

- **Seen through.** The opening shows the far dimension as it is: its own ground and Objs, under its own sky, lit by its own sun. A dungeon's dark makes its door a dark doorway. From inside, the land outside is seen in daylight.
- **Walked through.** Whatever goes through comes out in the other dimension, everything under it with it, still moving as it was.
- **Cameras.** A camera that follows a player looks through the opening while the player is through and it is not, then goes after the player. A camera under the player goes with it.

The game is seen from the dimension its camera is in. Its sky, sun, post-processing and weather are that dimension's. Sound is heard only from what plays there, and rain falls only there.

Once nothing has needed a dimension for its **Keep loaded** seconds, the game lets it go: no camera is in it, and no Portal near a camera leads into it. Coming near again loads it again.

## Not done yet

- Changes to a dimension's blocks are not kept once it is let go, and saves do not keep them.
- The editor does not show or edit a dimension yet. Write its file by hand, or copy a scene's Objs into it.
- Scripts cannot load a dimension or move an Obj into one yet, except by sending it through a Portal.
- Dungeons are still built in the land, not in dimensions of their own.
- A networked game sends no dimensions to the players who join.
