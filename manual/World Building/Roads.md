---
order: 3
---

A **Road** lays a road, a path or a canal on the ground of a world of blocks without end, along points you drag in the Scene view. Like every [[World Layers|world layer]] it stores no blocks: move its points or delete it and the ground comes back as it was.

## Laying a road

1. Make an empty Obj where the road starts, and choose **Add Component ▸ Terrain ▸ Road**.
2. Drag the dots in the Scene view. A point lands on the ground under it. The faint dot halfway along a stretch adds a point there, and the one past the end adds a point after the last. Shift-click a point to take it away.
3. Drag the small dot at a point's side to set how wide the road is there, so a road can widen into a market.
4. Set **Block** to what it is laid with, and **Edge** and **Kerb** for a kerb each side.

Let go of a road's end over another road and it snaps onto that road's middle, at its height, so the two join.

## Settings

| Setting | What it does |
| --- | --- |
| Ground | **Follow** lays it on the ground as it is. **Grade** cuts and fills the ground to a straight line between its points' heights, climbing no more than **Grade** |
| Width | How wide it is at every point, in blocks. Each point can have its own |
| Block | What it is laid with. **None** keeps the ground's own top |
| Edge, Kerb | The block along each edge, and how many blocks it stands over the road |
| Grade | The most a graded road climbs a block along, in blocks |
| Slope Out | Over how many blocks each side the ground eases back to the generator's, a block a step |
| Steps | **Blocks** climbs a whole block a step. **Slabs** lays a slab before each step up, where the world has slabs |
| Bridge, Deck | Over ground or water more than three blocks under it, a deck of **Deck** on a pier every eight blocks, in place of a bank of earth |
| Tunnel, Roof | Through ground more than four blocks over it, a tunnel four blocks high with a roof of **Roof**, in place of a cutting |
| Water, Depth | Water in it, **Depth** blocks deep: a canal |
| Clear Plants | Generated trees, plants and structures are left out of its reach |
| Priority | Where two roads cross, the higher priority's is laid |

A road stops square at its ends.

## Mixes and patterns

A road can be laid with a mix of blocks, each as often as its weight. The same place always gets the same block. Or it can be laid with a **pattern**: a small picture of the mix's entries by their place from 1, a row a block along the road and a character a block across it, repeated. `0` in a pattern takes from the mix.

```toml
[obj.road]
mix = [{ block = "…cobbles…", weight = 3 }, { block = "…flagstones…", weight = 1 }]
pattern = ["2111112"]   # a line of flagstones each side, cobbles between
```

From the command line:

```
vox editor call road.set_mix obj=713a4d989c654c6ca23b1d63201e0090:9 mix='[{"block":{"path":"Blocks/Cobbles.block"},"weight":3}]'
vox editor call road.set_points obj=713a4d989c654c6ca23b1d63201e0090:9 points='[[0,0,0,5],[40,0,0,9]]' land=true
vox editor call roads.nearest at=[120,0,100] reach=30
```

## Squares and plazas: Area Fill

An **Area Fill** paves an outline with a mix or a pattern of blocks, on the ground as it is (**Level**: **Follow**) or level at its Obj's height (**Level**: **Level**), with a slope out round it. **Size** sets a rectangle round its Obj; any outline comes from scripts, the scene file or `area_fill.set_outline`. A pattern runs a row a block along z and a character a block along x.

## From scripts

`Obj.Road` and `Obj.AddRoad`, and `Obj.AreaFill` and `Obj.AddAreaFill`, give every setting above by its name: `Points` as `RoadPoint`s, `Mix` as `MixBlock`s, and `Pattern` as strings. `Roads.Nearest` finds the road nearest a place:

```csharp
var road = Obj.AddRoad();
road.Points = [new RoadPoint(Vector3.Zero, 5f), new RoadPoint(new Vector3(0f, 1f, 40f), 9f)];
road.Mix = [new MixBlock(Cobbles, 3), new MixBlock(Flagstones, 1)];

if (Roads.Nearest(Obj.Position, 30f) is { } near)
{
	Log.Info($"{near.Road} passes {Vector3.Distance(near.Point, Obj.Position):0} m off");
}
```
