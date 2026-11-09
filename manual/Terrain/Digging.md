---
order: 11
---

Heights give the ground one height at each place, so they cannot make what a battlefield is full of: a trench with straight walls, a shell hole's undercut lip, a dugout under the parapet, a tunnel. Digging can. Where a terrain is dug, its ground there becomes voxels, cubes a quarter of a metre a side on ground of half-metre samples, which digging takes away and filling puts back in any shape. Everywhere else the terrain stays heights, and the two meet with no seam.

```csharp
public MaterialAsset Earth;

private void Landed(Vector3 at)
{
	Terrain.Explode(at, radius: 2.5f, walls: Earth);
}
```

## Digging in the editor

The **Sculpt** tool's **Dig** brush digs a ball the brush's **Size** round out of the ground where you click, and one after another along where you drag. Click again in the same place to dig deeper, and hold **Shift** to fill instead. **Strength** sets how deep each ball sits: at 0.5 it is half under the ground, so a click digs a bowl as deep as the brush is wide; at 1 deeper, at 0 shallower. **Bares** picks the surface laid on what it digs open. The **Fill** brush heaps ground up instead, as high as its strength.

What the brushes dig goes in a **Dig** layer, made on top of the list the first time and picked after, and each stroke is one step to undo.

### Trenches

**Add Layer ▸ Trench** digs a trench along a line, as a Spline lays a road: a new one runs across the middle of the terrain, and you drag its dots in the Scene view to place it, as for any [[Layers#Splines|Spline]]. It follows the ground between its dots, a couple of metres at a time.

| Setting | What it does |
| --- | --- |
| Width | How wide its floor is, in metres |
| Dig | Whether the line is dug as a trench; off, it is a Spline that lays the ground level |
| Depth | How deep its floor is under the ground, in metres |
| Wall lean | How far its walls lean back, in metres out for each metre up: 0 for upright revetted walls |
| Step height | How high its fire step stands above the floor, along its front wall |
| Step width | How deep the fire step is; 0 for none |
| Parapet | How high the earth dug out is heaped along its front; 0 for none |
| Front | Which side is its front, going along from its first dot: where the fire step and parapet are |
| Surface | The surface laid on what it digs open |

Digging in layers is done over the ground every other layer makes, whatever its place in the list, in the order the layers are in. Change the ground under a trench, and it is dug again into the new ground.

## Digging from scripts

| Call | What it does |
| --- | --- |
| `Terrain.Dig(point, radius, walls)` | Digs a ball of ground `radius` metres round out of the terrain, its middle at `point` |
| `Terrain.Fill(point, radius)` | Puts a ball of ground back |
| `Terrain.Tunnel(from, to, radius, walls)` | Digs a tunnel `radius` metres round from one point to another, rounded at its ends. Along the ground it is a trench; under it, a way through |
| `Terrain.Explode(point, radius, walls)` | Blows a crater out of the ground at `point`: a bowl `radius` metres round whose middle is a third of that under the point, with the earth thrown up round its lip |

Each returns whether it reached a terrain. They are edits like the others in [[Changing Terrain from Scripts]], so they come as `TerrainEdit`s too, are the same on every machine, are kept in a save of the scene and are given to players who join. A dig is about 20 bytes.

`walls` is the material of one of the terrain's surfaces (see [[Surfaces]]), laid on the ground a dig bares: earth under grass, chalk under turf. Leave it out and what is bared is the terrain's own surfaces there. Walls take every surface's pictures from the side as well as from above, so they do not stretch down a trench's sides.

## What changes with the ground

- Physics stands things on dug ground at once, and rays meet it, from **Physics.Raycast** and **Terrain.Raycast** alike.
- **Terrain.HeightAt** over a pit or a trench is the height of its floor: the top of the ground there.
- Grass is cleared where digging opens the ground, and grows on as before right up to the edge.
- Plants standing on ground taken from under them come down.

## How dug ground is kept

The terrain is cut into columns 8 m a side on ground of half-metre samples, each a stack of chunks of 32 voxels a side. The first dig into a column turns the whole column into voxels, made from its heights, and the heights in the column become holes for them to stand in. Each dig after that changes only the voxels near its shape, and only the chunks it reaches are made again.

A dug column's chunks are kept in memory, a few hundred kilobytes each. A save keeps the digs themselves while they are fewer than a few thousand, and after that only how each dug chunk differs from the heights it was made from.

Dug ground is drawn at its finest wherever it is seen, and its walls cast shadows. The light bounced round a level sees the ground as it was before it was dug. Track Makers leave no tracks on dug ground, and water does not stand in it: puddles on a dug surface are only wet.
