---
order: 6
---

A **dimension** is a world of its own inside the game: its own [[Block Worlds|Block World]], its own sky, sun and weather, and its own coordinates. Two dimensions can use the same coordinates and never meet. Nothing in one is drawn, lit, heard, collided with or made round a player in the other. Players go from one to another through [[Portals]].

The level's own scenes are its first dimension. Every level has it, and a level made before dimensions is that one alone. Any other dimension is a `.dimension` file, or a dungeon's: each [[Structures#Dungeons|dungeon]] is a dimension of its own, entered through a Portal in its gate.

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

## In the editor

**Project ▸ + ▸ New Dimension** makes a `.dimension` file of a kind: Overworld, Dungeon or Caves. It holds one Portal, **Way Back**, for a Portal in the level to lead into. Selected in the Project, its Inspector shows its settings, each with what it does and what it is by default; a change is written into the file at once. **Kind** puts the others back to what the kind gives.

**Open** opens the dimension beside the level, as a scene opens alongside it. Its Objs are listed under a header of their own in the Hierarchy, marked **Dimension**. They edit, undo and save as a scene's, into the dimension's file. New Objs made while the Scene view shows it go into it too. Close it from its header's menu.

The Scene view shows one dimension at a time. Pick which in its **Dimension** menu, beside its shading, which shows once more than the level's is open. Each Scene view keeps where its camera was in each dimension, so going back to one is going back to where you looked from. Selecting an Obj of another dimension, in the Hierarchy or anywhere else, shows that dimension. The sky, sun and weather are those of the dimension shown.

A [[Structures#Dungeons|dungeon]] in a dimension of its own has a Portal standing in its gate in the editor too, as in the game. Click its opening to select it. Its Inspector has **Open Dungeon**, which makes the dungeon's dimension, as the game makes it, and shows its way out. The Hierarchy lists the dungeon under a header of its own, marked **Not saved**: it is made from its structure each time. **Go Through** on its way out comes back to the gate.

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

The dimension is loaded once a camera comes close to the Portal: within the project's **Dimension distance**, 24 m. Until then the Portal shows its mist. The dimension's Portal then leads back too, if it led nowhere, and the far side shows as any other Portal's does.

Only one Portal into a dimension loaded this way is open at a time: the nearest within the Dimension distance. A dungeon's gate counts as one. So besides the level's, one such dimension at most is ever loaded. Come closer to a second Portal than to the first, and the first dimension is let go at once before the second is loaded. The first Portal shows its mist again. Set the distance in **Project Settings ▸ Graphics ▸ Portals**, or in `voxear.toml`:

```toml
[portals]
dimension_distance = 24.0
```

A Portal into another dimension works like one within a world:

- **Seen through.** The opening shows the far dimension as it is: its own ground and Objs, under its own sky, lit by its own sun. A dungeon's dark makes its door a dark doorway. From inside, the land outside is seen in daylight.
- **Walked through.** Whatever goes through comes out in the other dimension, everything under it with it, still moving as it was.
- **Cameras.** A camera that follows a player looks through the opening while the player is through and it is not, then goes after the player. A camera under the player goes with it.

The game is seen from the dimension its camera is in. Its sky, sun, post-processing and weather are that dimension's. Sound is heard only from what plays there, and rain falls only there.

Once nothing has needed a dimension for its **Keep loaded** seconds, the game lets it go: no camera is in it, and the open Portal does not lead into it. Coming close again loads it again.

## In a networked game

Players can be in different dimensions at once: one in a dungeon, another in the town, a third in a second dungeon.

- **The server** makes the ground round every player, in that player's own dimension, and round the far side of the Portal open for them. The one-dimension limit is for each player: each has one Portal open into a dimension loaded for it, so a server with players in three dungeons has three dungeons loaded. A dimension stays loaded while a player is in it or has a Portal open into it, and for its Keep loaded seconds after.
- **Each player's machine** makes only the dimensions its own player is in or near. The server tells it which to make and when it may let one go.
- **A dimension's changes come first.** When a player comes near a Portal into a dimension, the server sends every change made to that dimension's blocks: tunnels dug, walls built, doors left open. The nearest to where the player comes out go first. Until all of them are in, the Portal shows its mist on that player's machine, and the server takes none of the player's Objs through it. Changes made while players are inside go as your game's own RPCs.
- **Objs** go to a player only while they are in a dimension the player has (see [[Sharing Objs]]). The server decides when an Obj goes through a Portal into another dimension.

## Not done yet

- Changes to a dimension's blocks are not kept once it is let go, and saves do not keep them.
- A dungeon opened in the editor is shown at its first stage: dungeons do not grow yet.
- In play mode, a Scene view starts in the level's dimension, whichever it showed while editing.
- Scripts cannot load a dimension or move an Obj into one yet, except by sending it through a Portal.
- In a networked game, a dimension made by code with no `.dimension` file, other than a dungeon's, is not sent to players. The server makes only worlds of blocks round each player, not terrains.
