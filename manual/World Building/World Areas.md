---
order: 7
---

A **World Area** marks a part of a world: a town, a forest, a dungeon's floor. It has **tags**, and each tag carries a data asset of your game's own there, such as a town's laws or a region's music. Scripts ask what is at a place and get the data back. The world's generator keeps its structures out of an area, or only inside areas of a tag.

A World Area stores no blocks and changes no ground.

## Making an area

1. Make an empty Obj in the middle of the area, and choose **Add Component ▸ Terrain ▸ World Area**.
2. Drag the dots in the Scene view to shape its outline. The faint dot halfway along an edge adds a corner there. Shift-click a corner to take it away, down to three.
3. Set **Tags** to how many tags it has, then pick each **Tag** and the **Data** it carries here.

Or paint the area: set **Shape** to **Painted**, then on the world's Block World pick the **Area** brush and the area under **World Area**, and paint. Shift takes the area off where you paint. The first time an area is painted it gets its **Paint Number**, its colour on the world's Area map. Painted areas are kept in the world's `.maps` file beside its painted land (see [[Painting Land]]).

The Scene view draws each outline tinted by its first tag. Turn them off under **Gizmos ▸ Areas and Routes**.

## Tags

A tag is a data asset: choose **New Data Asset… ▸ World ▸ Area Tag** and name the file for the tag, as `Town.asset`. A tag has nothing in it but its name.

What a tag carries is any data asset (see [[Data Assets]]). Write a Definition for it:

```csharp
[CreateAsset("World/Town Laws")]
public sealed class TownLaws : Definition
{
	public bool NoWeapons;
	public float Curfew = 22f;
}
```

## Settings

| Setting | What it does |
| --- | --- |
| Shape | **Outline**: the outline round its Obj. **Painted**: what is painted with its number |
| Heights | **Any** holds every height. **Between** holds only from **Low** to **High**, in metres up, so a cave or a dungeon floor under a town is an area of its own |
| Priority | Where two areas hold a place, the higher priority's tags are asked first |
| No Structures | The generator builds none of its structures here: no trees, no towns, no dungeons |
| Tags, Tag, Data | Its tags, and the data asset each carries here |

## The world's defaults

Where no area holds a tag, the world's own data is used. Put **Area Defaults** on the world's Obj and list a tag, its data, and a **Biome**: a biome's entry holds in that biome, and an entry with no biome holds anywhere else. A desert can have its own music without an area drawn round it.

## Structures in areas

A structure's placement in its `.structure` file can keep it within areas of some tags, or out of areas of others:

```toml
[placement]
within = ["Areas/Town.asset"]        # only in an area with one of these tags
never_within = ["Areas/Forest.asset"] # never in an area with one of these
```

A structure with `within` and no such area anywhere is never built. An area's **Heights** count for the ground where the structure stands. Changing an area makes again only the ground round it.

## From scripts

`Areas.At` gives the areas holding a place, highest priority first. `Areas.Value<T>` gives the data a tag carries there, of the first area with the tag, or the world's default:

```csharp
public AreaTag Town;

protected override void Update()
{
	if (Areas.Value<TownLaws>(Obj.Position, Town) is { NoWeapons: true })
	{
		Obj.Get<Holster>()?.Put();
	}
}
```

`Obj.WorldArea` and `Obj.AddWorldArea` give every setting above: `Outline` as `Vector2`s and `Tags` as `AreaEntry`s. `WorldArea.Paint` paints a painted area round a place. `Obj.AreaDefaults` gives the world's defaults.

From the command line:

```
vox editor call world_area.set_outline obj=Town outline='[[-40,-40],[40,-40],[40,40],[-40,40]]'
vox editor call areas.at at=[120,10,95]
vox editor call areas.value at=[120,10,95] tag='{"path":"Areas/Town.asset"}'
vox editor call paint.map map=area area=Marsh at=[300,0,80] radius=30
```
