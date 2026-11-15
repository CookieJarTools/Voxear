---
order: 18
---

Things a world's generator builds where it finds ground for them: a house, a well, a ruin, a tree, a whole town. Each is a `.structure` file, listed on a generated [[Block Worlds|Block World]] or [[Voxel Terrain]] under its generator. A structure is built the same from whichever chunk reaches it first, so a house standing across two chunks is whole, and every player sees the same world.

A structure is built of one of three things:

- **A model**: its own blocks, kept in its file.
- **A plant**: a tree or a bush grown from a `.voxplant` afresh at every place, so no two trees of a wood are alike.
- **A town**: houses and a middle, each a structure with a model, laid out along streets (see [Towns](#towns)).
- **A dungeon**: great spaces carved into the rock under the land, reached by a grand stair from an entrance in the land (see [Dungeons](#dungeons)).

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
| By water | For a plant: whether it may stand on the shore and by rivers and lakes, as reeds do. Off by default: a tree keeps at least 2.5 m over the sea, and higher where its biome's shore reaches, and 6 m from a river's or a lake's water |

Spacing, Chance and Apart together keep a world from being crowded: a large town with a spacing of 2 km, a chance of 0.5 and Apart of 600 m comes rarely and alone, while a well with a spacing of 200 m is common.

Structures are built in the order the world lists them. One listed later keeps clear of what earlier ones built: its trunk off their streets, its crown off their houses. So list towns first, then lone buildings, then woods.

## Trees of blocks

A structure that names a **plant** grows a tree or a bush of blocks at each of its places, from a `.voxplant` file: a few numbers that say what kind of plant it is, and a seed of its own at every place, so a wood of one file has no two trees alike.

Select a plant in the Project panel and the Inspector shows its settings, under **Trunk**, **Crown**, **Blocks** and **Saplings**: each saved as it is changed, and every world growing it grows it anew. In its file:

| Setting | What it does |
| --- | --- |
| `kind` | `broadleaf`, a trunk under a rounded crown of forking branches; `conifer`, branches each one way round, each at a height of its own and rising a little, shorter towards the top; or `bush`, stems from the ground |
| `height` | How tall the trunk is, in blocks, from and to |
| `trunk`, `taper` | How thick the trunk is at its foot, in blocks, and how much thinner at its top, from 0 to 1 |
| `branch` | How thick its branches are, in blocks: logs as thin as this, from a quarter of a block |
| `bend` | How far the trunk leans and wanders: 0 for straight |
| `crown_width`, `crown_height` | How wide and how tall the crown is, in blocks, each from and to |
| `branches` | How thick with branches the crown is, from 0 to 1 |
| `leaf_size`, `leaf_density` | How large a clump of leaves is, in blocks, and how full, from 0 to 1 |
| `roots` | How many roots run out from its foot |
| `vines` | How many of its trees have vines, from 0 to 1: one or two up the trunk, and some hanging under the crown's edge |
| `wood`, `leaves`, `vine` | The [[Blocks|blocks]] its wood, its leaves and its vines are |
| `sapling` | The [[Blocks#Saplings|sapling]] that grows into it |
| `saplings` | How many saplings it scatters where its crown lands as it is felled, from and to |
| `grow_time` | How long a sapling takes to grow into it, in seconds of play; the Inspector shows minutes |
| `stages` | How many sizes it grows through after the sapling, the last the whole plant |
| `light` | The least light a sapling of it needs to grow, from 0 to 1: 1 is open sky, and each leaf over it takes a fifth |

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

Give the wood a block whose **Shape** is **Log** for a trunk that thins as it rises and branches of thin logs, the leaves one whose Shape is **Bushy** for a soft crown (see [[Blocks#Bushy leaves]]), and the vines one whose Shape is **Vine**. A plain cube named as a plant's wood is taken as a log, so a block made before logs had their shape still grows a tree that thins; its trunk thins most low down, and leaves always cover the wood at the top of its crown and the ends of its branches. A tree's vines hang only in open air: none where a hillside rises into them.

**Starter Trees of Blocks**, on a Block World's **Add plant** row, makes three of these, an oak, a birch and a pine, with their blocks and a structure each, and lists the structures on the world (see [[Block Worlds#Trees of blocks]]). Where the world has the starter biomes, oaks keep to the Forest and the Rainforest, birches to the Forest, the Grassland and the Taiga, and pines to the Taiga. The [[Biomes#Showcase biomes|showcase biomes]] bring two more of their own, a maple in the Autumn Woods and a blossom tree in the Blossom Grove.

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

A dungeon is a set of great spaces carved into the rock under the land: a gate in the land, a grand stair going down, and floors of halls, rooms, gorges and chasms below, the boss at the bottom. Only its entrance comes up to the surface. Every space keeps rock over and round it, whatever the land does over it, so no room ever comes out through a slope, an escarpment's wall or a lake's bed.

The quickest way to have one is **Add Temple Dungeon** on a generated Block World (or `vox editor call dungeon.starter`). It makes the temple's blocks, with their pictures, in a Dungeon folder beside the world's Blocks, and writes `Structures/Temple Dungeon.structure`, listed under the world's generator.

### The entrance in the land

Each dungeon looks round its place, up to 48 m, for where the land offers an entrance:

| Entrance | Where | What stands there |
| --- | --- | --- |
| Cliff gate | Where the land rises 12 m or more within 8 m: an escarpment's wall, a canyon's side | Four frames stepping into the rock from its face, trim round each, a pillar dividing the way in, great armoured statues, the ground before the face cut level, a terrace before it |
| Temple door | A slope of 20 to 40 degrees | A stone front with a pointed arch over the door and a stepped gable, a portico running back into the hill, two hooded statues, a terrace, then stepping stones out past broken pillars |
| Sinkhole stair | Level ground | A round shaft open to the sky, a stair winding down its wall, a kerb and broken columns round its rim |

The dungeon is then laid out into the land behind its entrance, under the high side. Where the land round a place offers none of the kinds the dungeon may use, or no room under it for even one floor over the world's lowest block, no dungeon is built there.

A grand stair runs down from the entrance to the first floor's landing: a block down for every two along, half steps of slabs, a landing every ten blocks down, pillars and pointed arches over it.

### Floors and spaces

Each floor is planned as a graph first:

1. The **landing**, or on a lower floor the foot of the shaft down.
2. A **threshold room**, where the first fight is.
3. **Two ways round** to a pillared hall: one down through a **gorge**, the other by a room and a long stair.
4. The **pillared hall**, on the floor's lower level.
5. Two ways on: a **flooded hall** and a **chasm**.
6. The **end**: a shaft down to the next floor, or on the last floor the **boss's arena**.

Loot waits in **treasuries** off the side rooms, and a narrow **shortcut** behind an iron gate leads from the end back to the start. Each floor is turned the way that keeps it under the highest land, and every passage is routed round the other spaces and under the land too.

| Space | Size (Medium) | What is in it |
| --- | --- | --- |
| Room | 22 m across, 12 m high | Pilasters, a raised edge with steps at its ends, two thick columns for cover, braziers in the corners |
| Pillared hall | 56 m long, 28 m high | Columns down both sides, pointed arches carrying a gallery with a balustrade, armoured statues in the bays, ribs across the vault, and at its far end a broad flight of stairs up to an apse with a glowing reliquary under a carved relief, daylight falling on it where level land lies over the hall |
| Gorge | 40 m long, 26 m deep | Layered walls, a terrace and a carved archway with caged braziers at its top, a stair down its walls, rubble on its flagstones, blue crystal in the rock |
| Chasm | 44 m across, a 24 m pit | A walkway round a pit of lava, a balustrade at its edge, bridges on piers to an island with an obelisk, a ring of columns and arches |
| Flooded hall | 48 m long | Green water round carved stelae on pillars, wide walkways and steps down into the water, a relief on its far wall |
| Boss's arena | 40 m across, 24 m high | Rings in its floor, a ring of columns and arches, a throne on a dais facing the way in |
| Shaft down | 20 m across | A stair winding down its wall, a balustrade on its inner edge, past a well that goes on down to lava |

Every space is cut out of the rock with an edge that wanders in and out, so natural rock rounds into it. Built stone is laid crisp into it after: flagstones, walls dressed in stone to head height, columns, arches and stairs. Then it is worn and lit:

- **Ruin:** built stone breaks away at its exposed edges, with rubble under where it fell. Never on a floor or a stair's tread.
- **Wear:** stone is cracked where most exposed, and mossy near the way in.
- **Overgrowth:** vines hang from the roofs near the way in, thinning with depth.
- **Lights:** braziers along the main path every 14 m, blue crystal on side ways, pools of firelight down the halls, lava glowing in the chasm.
- **Shafts:** where level land lies over a great hall, a crack lets daylight fall into it.

### Settings

The easy way in is a theme, a size and how many floors. Everything else has a good value for the theme:

```toml
[dungeon]
theme = "temple"
size = "medium"             # small, medium or large
floors = [1, 3]
```

| Setting | What it does |
| --- | --- |
| Theme | The look and the set pieces. Temple: an old temple carved into natural rock |
| Size | Small (a threshold, a hall, a gorge and the end, about 120 m across), Medium (about 200 m) or Large (a second hall and more loot, about 260 m) |
| Floors | How many floors, one under the next, from and to |
| Entrances | The kinds of entrance it may use: Cliff, Temple, Sinkhole. None for any the land offers |
| Cover | The least rock over and round every space, in blocks. 8 by default |
| Caves | How far a space breaks through to a cave of the world's, in blocks. 48 by default; 0 for never |
| Shafts | Cracks of daylight over its great halls, where level land lies over them |
| Blocks | Its blocks by what they are for (see below) |
| Passes | How worn and lit it is: Ruin, Wear, Overgrowth and Lights, each 0 to 1 |
| Sizes | The sizes of its spaces, in blocks: Passage (width, height), Stair (width), Room, Hall, Gorge, Chasm, Arena, and Floor Gap. Each as its size says where left at 0 |
| Entrance | Your own entrance in place of the theme's: a structure with a model, its door on the side that faces -z |
| Rooms | Your own pieces, by role (see below) |

**Blocks** names each block by what it is for: Stone (walls, columns, arches), Trim (capitals, bases, arch rings, balustrades), Floor (flagstones), Rubble, Cracked, Mossy, Slab (half steps), Stairs, Statue, Metal (cages, stands, gates), Light (a brazier's fire), Cold Light (blue crystal), Water, Lava and Vine. Any left out is the nearest: the stone for most, the ground's own rock where there is no stone. What needs a slab, iron, lights, water, lava or vines does without where there are none.

A dungeon written before themes keeps loading: its Wall and Floor blocks become its Stone and Floor, and its old room counts and sizes are read and left unused.

### Your own pieces

Your own pieces stand in the spaces of their role in place of the theme's furnishing. Each is a structure with a model, with a **Role**:

| Role | Where it stands |
| --- | --- |
| Any | A room where fights happen |
| Hall | A pillared hall |
| Loot | A treasury |
| Boss | The boss's arena |

Each kind also has **Weight** against the others of its role, **Most** a floor has, and a **Zone**: how far along the floor it stands, from 0 at its start to 1 at its end. A piece stands in the middle of its space, on its floor, and the space is made large enough to hold it and a little more.

```toml
[[dungeon.rooms]]
structure = "Structures/Throne.structure"
role = "boss"
```

### From scripts and the command line

A script reads the dungeons near a place with **Dungeons**. Each gives its entrance, its spaces by kind, where each is and how large, the ways between them and its lights, for putting players, enemies and loot in them:

```csharp
var dungeon = Dungeons.Nearest(Player.Position);
Obj.Instantiate(boss, dungeon!.Boss!.Position, Quaternion.Identity);
foreach (var room in dungeon.SpacesOf(DungeonSpaceKind.Room))
{
	SpawnGuards(room.SpawnPoint());
}
```

`vox editor call dungeon.layouts` gives the same, as JSON in the level's metres, and `vox editor call dungeon.starter` makes the starter dungeon. A dungeon's settings are its file's, read and changed with `asset.get` and `asset.set`.

A dungeon in a world with [[Caves|caves]] breaks into them: the space nearest a cave room, within **Caves** blocks of it, has a rough passage cut to that room's floor. So a dungeon can be found from a cave, and a cave from a dungeon.

## Doors and stairs in a structure

A structure's model can put down [[Blocks#Blocks of parts|blocks of parts]] standing a way: a door facing out of a house, its stairs rising to a loft. Under `[[model.states]]`, give the entry of the palette and how it stands. For a door, list the block twice in the palette, once for its lower block and once for its upper:

```toml
[model]
palette = ["Blocks/Planks.block", "Blocks/Oak Door.block", "Blocks/Oak Door.block"]

[[model.states]]
entry = 2
facing = "north"

[[model.states]]
entry = 3
facing = "north"
half = "upper"
```

A structure placed turned, as a town turns each house to face its street, turns each block's way with it, and one placed mirrored hangs its doors on the other side.

## Not yet

- A structure's levers and buttons are not linked to its doors. Link them from a script once it stands.

- Structures are built near the camera only. Far off, past a world's own chunks, they do not show.
- Structures and towns are made by writing their files. The editor has no tools for them yet, but for a plant's own settings in the Inspector and Add Temple Dungeon.
- A dungeon has one theme so far, the temple. Its enemies, loot and the boss are a script's to place, from `Dungeons`.
