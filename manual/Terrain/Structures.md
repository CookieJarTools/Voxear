---
order: 18
---

Things a world's generator builds where it finds ground for them: a house, a well, a ruin, a tree, a whole town. Each is a `.structure` file, listed on a generated [[Block Worlds|Block World]] or [[Voxel Terrain]] under its generator. A structure is built the same from whichever chunk reaches it first, so a house standing across two chunks is whole, and every player sees the same world.

A structure is built of one of three things:

- **A model**: its own blocks, kept in its file.
- **A plant**: a tree or a bush grown from a `.voxplant` afresh at every place, so no two trees of a wood are alike.
- **A town**: houses and a middle, each a structure with a model, laid out along streets (see [Towns](#towns)).
- **A dungeon**: rooms under the ground joined by corridors, on one floor or several (see [Dungeons](#dungeons)).

## Where structures stand

Every structure has a placement. The world is cut into squares **Spacing** apart, and each square has one try at a place within it. Its rules decide which tries are built:

| Rule | What it does |
| --- | --- |
| Spacing | How far apart its tries are, in metres. Larger is rarer |
| Separation | The least between two of its own places, in metres, under its spacing |
| Chance | How many of its tries are built, from 0 for none to 1 for all |
| Apart | The least between one of its places and another structure's, in metres. Where two structures both keep a distance, the larger is kept |
| On | Land, Water or Underground |
| Height | How high over the sea its ground may be, from and to, in metres. Left out for any |
| Slope | The steepest ground it stands on, in degrees |
| Biomes | The biomes it stands in (see [[Surfaces#Biomes|Biomes]]). None for any |

Spacing, Chance and Apart together keep a world from being crowded: a large town with a spacing of 2 km, a chance of 0.5 and Apart of 600 m comes rarely and alone, while a well with a spacing of 200 m is common.

Structures are built in the order the world lists them. One listed later keeps clear of what earlier ones built: its trunk off their streets, its crown off their houses. So list towns first, then lone buildings, then woods.

## Trees of blocks

A structure that names a **plant** grows a tree or a bush of blocks at each of its places, from a `.voxplant` file: a few numbers that say what kind of plant it is, and a seed of its own at every place, so a wood of one file has no two trees alike.

| Setting | What it does |
| --- | --- |
| `kind` | `broadleaf`, a trunk under a rounded crown of forking branches; `conifer`, rings of branches shorter towards the top; or `bush`, stems from the ground |
| `height` | How tall the trunk is, in blocks, from and to |
| `trunk`, `taper` | How thick the trunk is at its foot, in blocks, and how much thinner at its top, from 0 to 1 |
| `bend` | How far the trunk leans and wanders: 0 for straight |
| `crown_width`, `crown_height` | How wide and how tall the crown is, in blocks, each from and to |
| `branches` | How thick with branches the crown is, from 0 to 1 |
| `leaf_size`, `leaf_density` | How large a clump of leaves is, in blocks, and how full, from 0 to 1 |
| `roots` | How many roots run out from its foot |
| `vines` | How many of its trees have vines, from 0 to 1: one or two up the trunk, and some hanging under the crown's edge |
| `wood`, `leaves`, `vine` | The [[Blocks|blocks]] its wood, its leaves and its vines are |

```toml
kind = "broadleaf"
height = [7.0, 11.0]
trunk = 1.2
crown_width = [7.0, 10.0]
crown_height = [5.0, 7.0]
vines = 0.3
wood = "Blocks/Oak Log.block"
leaves = "Blocks/Oak Leaves.block"
vine = "Blocks/Vine.block"
```

Give the leaves a block whose **Shape** is **Bushy** for a soft crown (see [[Blocks#Bushy leaves]]), and the vines one whose Shape is **Vine**. A tree's vines hang only in open air: none where a hillside rises into them.

**Starter Trees of Blocks**, on a Block World's **Add plant** row, makes three of these, an oak, a birch and a pine, with their blocks and a structure each, and lists the structures on the world (see [[Block Worlds#Trees of blocks]]). Where the world has the starter biomes, oaks keep to the Forest and the Rainforest, birches to the Forest, the Grassland and the Taiga, and pines to the Taiga.

## How the ground meets a structure

| Ground | What it does |
| --- | --- |
| Leave | The ground as it is: for trees and standing stones |
| Foundation | Ground built up under its lowest blocks, so nothing stands on air on a slope |
| Flatten | A foundation, and the hillside cleared from over its footprint as high as it stands |

**Sink** stands it that many blocks into the ground.

## Towns

A town grows as a village does. Streets run out from its middle and take the way that climbs least, so they bend round hills rather than over them, and now and then a shorter street branches off. They stop at ground too steep to walk, at the sea, and near another street. Houses stand along both sides, set back a block from the street, turned to face it, with a short path from the door. They are built nearest the middle first, so a town is thickest there and thins out along its streets.

| Setting | What it does |
| --- | --- |
| Middle | The structure at its middle: a well, a square, a hall |
| Houses | The kinds of house it builds (see below) |
| Count | How many houses, from and to |
| Radius | How far its main streets run, in blocks, from and to |
| Path, Path width | The block its streets are made of, and how wide they are |
| Wander | How much its streets bend, from 0 for straight |
| Branching | How often its streets branch, from 0 for never |
| Roads | Whether it has roads to the next towns of its kind. Off unless switched on |
| Road, Road width, Bridge | The block of its roads, how wide they are, and the block of the bridges where they cross water |
| Link | How many of the roads to its nearest towns are made, from 0 to 1 |

A house's door is in the middle of the side of its model that faces -z.

Each kind of house has rules of its own, so a town can have many cottages, one chapel near its middle and barns out at its edge:

| Rule | What it does |
| --- | --- |
| Weight | How often it is chosen against the others |
| Most | At most how many a town has. 0 for no limit |
| Zone | How far out from the middle it stands, as a share of the town's reach, from and to: 0 to 0.3 near the middle, 0.6 to 1 at the edge |
| Biomes | The biomes it stands in, so a town spread over two biomes builds each its own way. None for any |

A small hamlet and a large town are two structures, each with a town of its own size and a placement of its own: the hamlet common, the town rarer and kept further apart from the rest.

### Roads between towns

With **Roads** on, a town is joined to the towns of its kind next to it. A road leaves one town and heads for the next, curving round hills where it is far from either and heading straight in as it nears a town. It crosses water on a bridge a block over the water, with posts down to the bed. Which towns are joined is decided by the two towns alone, so a road is the same from every chunk it crosses.

## Dungeons

A dungeon is rooms under the ground joined by corridors, reached by a stair winding down from the surface. It goes down floor by floor: each floor is laid out afresh, and a stair at the far end of its main path winds down to the next, which starts at the stair's foot. Each floor is laid out the way well-loved dungeons are:

- **A main path** of rooms leads from the stair's foot to its far end: the stair down to the next floor, or on the last floor the **boss's room**. Some rooms along it are **great halls**.
- **Side branches** leave the main path, one to three rooms long, each ending in a dead end where the **loot** is.
- **Loops** join rooms that lie near each other but are three or more corridors apart along the paths, so not every way back is the way you came. A loop never reaches the boss's room, a loot room or the stair down, so those stay dead ends with one way in.

So every floor has its loot, and the boss waits at the bottom.

Rooms come in many sizes and are fitted on one at a time beside the room they leave, so every room can be reached. Paths turn now and then rather than running in a line.

### Your own rooms

A dungeon's rooms are kinds you list, each a structure with a model, each with a **role**:

| Role | Where it stands |
| --- | --- |
| Any | Any room on the main path or a side branch |
| Hall | A great hall on the main path |
| Loot | A dead end at the end of a side branch |
| Boss | The room at the end of the last floor's main path |

Each kind also has the rules a town's houses have: **Weight** against the others of its role, **Most** a floor has, and a **Zone**, here how far from the floor's stair as a share of the way to its furthest room. Where no kind of a room's role may stand, a plain room of the dungeon's walls is built, as large as that role's rooms are. So a dungeon with no kinds at all still has halls, loot rooms and a boss's room, and listing a throne room as Boss swaps it in for the plain one.

| Setting | What it does |
| --- | --- |
| Entrance | The structure at the surface over the stair: a crypt door, a ruined tower. None for a bare stair |
| Rooms | The kinds of room it builds, each with its role and rules |
| Count | How many rooms on each floor, from and to |
| Floors | How many floors, one under the next, from and to |
| Floor gap | How far down each floor is from the one over it, in blocks. Never less than its tallest room needs |
| Main path | The share of a floor's rooms on its main path |
| Halls | The share of the main path that is great halls |
| Corridor | How long its corridors are, in blocks, from and to |
| Loops | How many of the joins between near rooms are made, from 0 to 1 |
| Depth | How far under the ground its first floor is, in blocks, from and to |
| Caves | How far one of its rooms breaks through to a cave of the world's, in blocks. 48 by default; 0 for never |
| Wall, Floor | The blocks its walls, plain rooms and stair are made of, and its floors |
| Corridor width, Corridor height | How wide and high its corridors are |
| Room size, Hall size, Loot size, Boss size | The sizes of its plain rooms of each role, in blocks along x, up and along z |

A room's doors are in the middles of its sides, a block over its floor: the corridors open its walls there. The stair winds round a pillar, a block up for every block along.

A dungeon in a world with [[Caves|caves]] breaks into them. The room of any floor nearest a cave room, within **Caves** blocks of it, has a rough tunnel cut from its middle to that room's floor, four blocks high and as wide as five or so, never steeper than it runs along. So a dungeon can be found from a cave, and a cave from a dungeon, and the way down from the surface may be either.

## Not yet

- Structures are built near the camera only. Far off, past a world's own chunks, they do not show.
- Structures and towns are made by writing their files. The editor has no tools for them yet.
