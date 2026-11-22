---
order: 5
---

Paint where the land, the mountains and the biomes of a world of blocks without end go, and its generator does the rest: painted land grows a coast of its own with beaches and shallows, a painted mountain gets the generator's own peaks, valleys and gullies, and rivers, climate and generated structures follow the painted ground. Paint the other way and the land sinks into the sea.

Painting changes no blocks: it is kept in a `.maps` file beside the world, in tiles only where painted, 8 metres a pixel for land and height and 4 for biomes. Only the ground under a stroke is made again, and the far view and the map show it from the start.

## The brushes

Select the Block World, pick **Build**, and choose one of the map brushes as its **Brush**:

| Brush | What it paints |
| --- | --- |
| Land | Land rising out of the sea; with Shift, sea sinking the land |
| Height | Height under the generator's own ridges; with Shift, lowered ground |
| Biome | The **Biome** picked, of those the world lists, chosen before the climate rules, its edges ragged; with Shift, taken away |
| Erase | The land, height and biome maps back to the generator alone |
| Area | The **World Area** picked, painted with its number (see [[World Areas]]); with Shift, taken off |

**Radius** is how wide the brush is, in metres; **Strength**, from 0 to 1, how much a dab adds; **Falloff**, how much of the radius eases out to nothing. Hold Ctrl to smooth the land or height under the brush instead. Each stroke is one step to undo.

The ground near the camera is made again as you paint; the rest of the world, nearer or further, only where a stroke reached.

## Rivers and the rest follow

The generator works out its rivers, its climate and where its structures stand from the painted ground, so a mountain painted beside a river feeds it and the river widens downstream, a hill painted in a cold climate may wear snow, and a town is built only where the ground still suits it. Paint, then look round the edges of what you painted.

## The Painted Map node

A generator can read the paint itself: the **Painted Map** node, under **Sources**, gives the world's Land, Height or Biome map as a field, its value where painted and **Default** elsewhere, to blend into a graph of your own. In the generator's own preview, with no world, it is Default everywhere.

## From the command line and scripts

```
vox editor call paint.map map=land at=[60,0,230] radius=110 strength=1
vox editor call paint.map map=height at=[60,0,230] radius=50 strength=0.7 falloff=0.7
vox editor call paint.map map=biome biome=Biomes/Desert.biome at=[300,0,300] radius=60
vox editor call paint.map map=erase at=[60,0,230] radius=120
```

```csharp
// An island raised where the player stands, as a quest's reward.
Painting.Paint(PaintMap.Land, player.Position, radius: 120f, strength: 1f);
Painting.Paint(PaintMap.Height, player.Position, radius: 60f, strength: 0.5f, mode: PaintMode.Add);
```
