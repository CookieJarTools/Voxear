---
order: 1
---

A **Terrain** is a stretch of ground: a field, a valley or a range of hills. It reaches from its Obj's position along +X and +Z, as wide and as long as its **Size**. Only the Obj's position places it: a terrain lies level with the world, so turning or scaling its Obj does nothing to it.

## Making a terrain

Select an Obj and pick **Add Component ▸ Terrain ▸ Terrain**. It starts flat, 512 metres each way, on a new asset of its own in the project's **Terrains** folder, named for its Obj: `Terrains/Ground.terrain`. You can also make one with **New ▸ New Terrain** in the Project panel and pick it as a Terrain's **Data**.

The asset holds the ground, so every Terrain that picks the same asset has the same ground, as two copies of a level would. Changes to the ground are written to the asset when you save the level: the `.terrain` file says how large and how fine the ground is, and its heights are kept beside it in a `.heights` file of the same name. Keep the two together when you move or copy them.

## Size, resolution and heightmap

These are the asset's, so they show only for a terrain that has one.

| Setting | What it does |
| --- | --- |
| Data | The `.terrain` asset its ground is |
| Size | How wide and long the ground is, in metres |
| Resolution | How many heights it has each way: 257 to 8193. More is finer, and takes more memory |
| Heightmap | A grey picture its heights are made from, black lowest and white highest; None for heights of its own |
| Height | How high the heightmap's white is, in metres |

The distance between heights is the **Size** over one less than the **Resolution**: a 512 m terrain at 1025 has a height every half metre. Half a metre suits a battlefield, with trenches and shell holes; a metre or two suits wide open country.

A heightmap from another tool is best as a 16-bit greyscale PNG: an 8-bit one has only 256 steps, which show as terraces on a tall terrain. A raw 16-bit file works too. The picture's top is the terrain's far edge, the one along +Z, as it looks from above. Changing **Size**, **Resolution** or **Height** makes the ground again from the heightmap; a terrain with no heightmap keeps its ground, stretched to fit. Each change is one step to undo.

## How it is drawn

| Setting | What it does |
| --- | --- |
| Material | The material its ground is drawn with |
| Texture size | How many metres one picture of the material covers on the ground |
| Detail distance | How far from the camera the ground is drawn at its finest, in metres |
| Cast shadows | Whether its hills cast the sun's shadows |

The ground near the camera is drawn with a triangle between every pair of heights. Further off it is drawn more coarsely, in levels: each level reaches twice as far as the one before, with half as many triangles each way. Where one level meets the next, the finer one's points slide onto the coarser one's over the last stretch, so the ground changes without cracks or popping as the camera moves. Raise **Detail distance** for hills that keep their shape further off; lower it to draw less. The **LOD** debug view colours each level.

Hills hide what is behind them: terrain is drawn first, so what it covers is not drawn at all. Its ground also lights the world round it, as the GI's rays see it, and the sea's floor follows it.

## Collision

The ground collides as it is drawn near the camera, a triangle between every pair of heights, so a character walks on exactly what is there. A Terrain needs no Collider: physics finds the ground on its own, and rays from **Physics.Raycast** meet it, its Obj as what they hit. The terrain's Obj's **Layer** is the ground's layer.

## The ground in scripts

**Terrain** in a script asks about the level's ground, read from every terrain's heights, the same on every machine and with or without physics.

```csharp
// Keep a marker on the ground as it moves about.
var at = Transform.Position;
Transform.Position = new Vector3(at.X, Terrain.HeightAt(at), at.Z);

// How steep the ground is there, in degrees.
var slope = MathF.Acos(Terrain.NormalAt(at).Y) * 180f / MathF.PI;

// Where a shot from the sky lands.
if (Terrain.Raycast(new Vector3(x, 500f, z), -Vector3.UnitY, out var hit))
{
    Log.Info($"lands on {hit.Obj.Name} at {hit.Point}");
}
```

| Call | What it gives |
| --- | --- |
| `Terrain.Exists` | Whether the level has a terrain |
| `Terrain.HeightAt(position)` | How high the ground is under a point; negative infinity off every terrain |
| `Terrain.NormalAt(position)` | Which way the ground faces there, smoothed as it is drawn |
| `Terrain.TryGetGround(position, out ground)` | Both at once, with the terrain's Obj |
| `Terrain.Raycast(origin, direction, out hit)` | Where a ray first meets the ground, from above or below |
