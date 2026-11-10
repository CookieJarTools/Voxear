---
order: 18
---

Caves made of rooms: chambers you build as prefabs, set in the ground by a world generator and joined by winding tunnels. A **Cave Network** node places them, either without end under a whole world or as one bounded level to play in. They are for [[Block Worlds]] and [[Voxel Terrain|Voxel Terrains]], flat or round a planet.

The winding caves a world makes from noise, [[Block Worlds#Overhangs and caves in the generator|Tunnels and Chambers]], need no rooms and can run beside these.

## A room is a prefab

Make a [[Prefabs|prefab]] and give its Objs two components, both under **Add Component ▸ Terrain**:

| Component | What it is |
| --- | --- |
| **Cave Shape** | One shape of the room. A room is as many as you like, overlapping |
| **Cave Exit** | Where a tunnel may join the room. It leaves along the Obj's forward |

Move, turn and scale the Objs with the usual tools. Every Cave Shape in the prefab is outlined in the Scene view, so the room shows whole while you work on one shape: orange for a shape that carves, grey for one that fills, and the selected one brighter with its inner margin drawn inside it. A Cave Exit is an arrow with a ring.

### Cave Shape

| Setting | What it does |
| --- | --- |
| Shape | **Ball**, **Half Ball** (a dome standing on the Obj, for a room with a level floor) or **Box** |
| Mode | **Carve** takes rock out. **Fill** puts it back into what the shapes before it carved: a pillar, a ledge, a wall across a hall |
| Size | How wide, high and long it is, in metres, times the Obj's scale |
| Chance | How likely a room placed in the ground has this shape, from 0 to 1. With less than 1, one prefab gives many rooms: an alcove here, none there |
| Inner Margin | How far in from the shape the walls' roughness may come, in metres. Past it the room is always clear |

Shapes count in the order of the prefab's Objs, so a Fill comes after what it fills. The room's place is its prefab's root: build it with the floor at the root's height, since that is the height a network sets the room at.

### Cave Exit

Put an exit at the room's edge, pointing out. A tunnel starts just inside it and leaves the way it points. A room is turned so its exits face the rooms it is joined to, so exits on several sides give the best fit; where a room has fewer exits than tunnels, two tunnels share one. A room with no exits is joined at its middle.

Anything else in the prefab is left alone by the generator. A script can put the prefab's other Objs into a room (see [[#Caves from scripts]]).

## The Cave Network node

Open the generator's graph (see [[Generator Graphs]]) and add **Cave Network**, under Voxels. Its output is a cave: wire it into **B** of a **Combine** set to **Subtract**, with the ground in **A**, and the Combine into the Output's **Volume**.

| Row | What it does |
| --- | --- |
| Layout | **Endless** or **Level**, below |
| Spacing | How far apart rooms lie, in metres. Never less than the largest room and a tunnel's bends need |
| Depth | How far under the ground the rooms' floors lie, in metres |
| Tunnel Width | How wide tunnels are, in metres. Their floors are level across |
| Winding | How far tunnels stray from the straight way, from 0 to 1 |
| Roughness | How far walls are pushed in and out, in metres |
| Entrances | Endless only: how many rooms have a way up to the surface, from 0 to 1 |
| Main Rooms, Side Rooms | Level only: how many rooms its main path has, and how many lie off it |

Under **Advanced**, **Roof** is how far under the ground everything but an entrance stays, and **Seed** gives the network a seed of its own.

### Rooms

Under its rows the node lists its rooms. **Add Room** picks a prefab from the project; each room's number is its weight, how often it is chosen beside the others. With no room listed the network places a plain domed chamber, so it makes caves as soon as it is wired in.

The generator keeps each room's shapes and exits as numbers in its own file, read from the prefab when you pick it and again whenever the prefab is saved or changes on disk. Making ground reads only those numbers: a built game, and a player joining one, needs the generator's file and no prefab, and every machine makes the same caves from it.

A room is turned by quarter turns and mirrored as it is placed, so a few prefabs make many rooms.

### Endless

Rooms in every direction, one to a region of the grid `Spacing` across, each set a little differently in its region and `Depth` under the ground at its own middle, give or take a quarter. Most neighbouring rooms are joined by a tunnel. **Entrances** gives that share of rooms a tunnel up to the open air; the rest are reached from below.

Any part of the world is made by itself and meets what is made beside it, as far as the world goes. Round a planet the regions lie on the six sides of a cube pressed onto the sphere, rooms stand upright wherever they are, and tunnels cross from side to side.

### Level

One bounded cave, laid out once from the seed: a main path of **Main Rooms** that never crosses itself, and **Side Rooms** off it, each a dead end or the start of one. Every room is joined by a tunnel to the room it was reached from, so all of it can be walked from the first room. The first room lies `Depth` under the ground over it, and the others within half of `Depth` above or below.

A level wants rock to be carved from. The **Cave Level** preset is that: level rock with one cave in it, its **Height**, **Rooms**, **Side Rooms** and **Depth** as rows. Make a generator, pick Cave Level as its Preset, and give it to a Block World whose ground reaches round the level and down past it. Round a planet a level lies flat under the planet's top.

## Caves from scripts

**Caves** gives a script the level's rooms: where to put the player, the enemies and the thing they came for.

```csharp
public Prefab Relic;

protected override void Start()
{
	// The rooms of the level's cave: the main path's first, in order.
	Obj.Find("Player").Position = Caves.Start!.SpawnPoint();
	Obj.Instantiate(Relic, Caves.End!.SpawnPoint(), Quaternion.Identity);

	foreach (var room in Caves.Rooms)
	{
		if (!room.OnMainPath)
		{
			Log.Info($"a side room at {room.Position}");
		}
	}
}
```

| Call | What it gives |
| --- | --- |
| `Caves.Rooms` | The level's rooms: the main path's first, in order, then the side rooms. Empty with no cave level |
| `Caves.Start`, `Caves.End` | The first and last rooms of the main path |
| `room.SpawnPoint()` | A place on the room's floor with room to stand |
| `room.RandomPoint(random, reach)` | A place on its floor near that, for scattering things |
| `room.Exits` | Its exits: each one's `Position`, `Direction`, whether a tunnel leaves by it (`Used`) and the room that tunnel `LeadsTo` |
| `room.Neighbours` | Every room a tunnel joins it to |

A room also has its `Index`, `Position`, `Rotation`, `Up`, `Mirrored`, `OnMainPath`, the corners of a box that holds it (`BoundsMin`, `BoundsMax`) and the `Prefab` it was made from. `room.Point(local)` turns a place in the prefab into where it lies in the level, which is how to put a prefab's own Objs into a room:

```csharp
// A torch where the prefab has one, in every room made from it.
var torch = room.Point(new Vector3(3f, 2f, 0f));
```

### More than one world, and caves without end

`Caves` is the caves of the level's first world of blocks that has any. `Caves.Of(world)` gives a **CaveNetwork** for the world on another Obj, and a network answers more:

| Call | What it gives |
| --- | --- |
| `network.Layout` | `CaveLayout.Endless` or `CaveLayout.Level` |
| `network.Rooms` | As `Caves.Rooms` |
| `network.RoomsNear(position, radius)` | The rooms within `radius` metres, nearest first. This is how to find rooms in a network without end, which has no list of them |
| `network.RoomAt(position)` | The room a place is inside; null in a tunnel or in rock |

Rooms are where the generator put them, whatever has been dug or built since.
