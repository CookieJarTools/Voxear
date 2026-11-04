---
order: 6
---

Plants change with the year: fresh green in spring, as made in summer, red and gold in autumn, bare and snowy in winter. One **Seasons** turns every plant in the level together, and each plant a little earlier or later than its neighbours, so a wood in autumn has green, yellow and red trees side by side, and its leaves thin out unevenly before winter.

## A Seasons

Add a **Seasons** to any Obj (Add Component, then Rendering). It starts at summer, where plants look as they were made, so adding it changes nothing until the year moves.

| Setting | What it does |
| --- | --- |
| Time of year | Where the year is, from 0 to 4: spring at 0, summer at 1, autumn at 2, winter at 3, and spring again at 4. Between two, plants are part way from one to the next |
| Season | The season the year is nearest. Pick one to turn the year to it |
| Colour | The colour leaves turn this season. The leaves' texture keeps its light and dark under it |
| Colour amount | How far leaves turn to that colour: 0 for their own colour, 1 for all the way |
| Leaves | How many leaves are on the plants: 1 for all, 0 for bare |
| Snow | How much snow lies on plants, on what faces up |
| Variation | How unevenly plants follow the year: 0 for all together, 1 for up to half a season early or late. Plants' shades differ more as it rises |
| Patch size | How wide the patches of plants that turn together, and dry together, are, in metres |
| Dryness | How much of a field dries out, in patches: 0 for none, 1 for the most |
| Dry colour | The colour dry leaves turn |
| Wetness | How wet plants are, after rain: darker, and shining |

**Colour**, **Colour amount**, **Leaves** and **Snow** belong to one season: the Inspector shows the season's the year is nearest, so pick a season to set its look. The defaults:

| Season | Colour | Colour amount | Leaves | Snow |
| --- | --- | --- | --- | --- |
| Spring | Fresh green | 0.35 | 1 | 0 |
| Summer | Leaves as made | 0 | 1 | 0 |
| Autumn | Orange | 0.8 | 1 | 0 |
| Winter | Brown | 0.9 | 0 | 0.6 |

Only the level's first Seasons is used. Without one, plants stay as made.

In a scene file, each season's look is a table of its own:

```toml
[[obj]]
id = 41
name = "Seasons"
[obj.seasons]
time_of_year = 2.0
variation = 0.6
[obj.seasons.autumn]
color = [0.8, 0.3, 0.1]
color_amount = 0.9
```

## What follows the seasons

- **Leaves**, materials with **Foliage** on, take each season's colour, dry out in patches and fall. A material's **Leaves** setting says which of those it does: **Fall** (the default) for broadleaf trees, **Turn** for grass and hedges, which turn colour but stay, and **Stay** for pines, which stay as made. See [[Materials]].
- **Plants**, foliage or anything else with a **Wind bend**, such as a tree's trunk, take snow and wetness.

Leaves fall in clumps, triangle by triangle, so a canopy thins out rather than fading. Their shadows go with them where the graphics card has mesh shaders; on older cards, a bare tree's shadow keeps its leaves.

A whole plant turns together, by where it stands, so a tree is never half one season and half another.

## Turning the year from code

`Seasons.TimeOfYear` reads and sets the level's time of year, and `Seasons.Season` the season it is nearest:

```csharp
public sealed class Calendar : Behaviour
{
	protected override void Update()
	{
		// A year every two minutes.
		Seasons.TimeOfYear += Time.Delta * 4f / 120f;
	}
}
```

`Seasons.Season = Season.Winter;` turns it to winter at once. Without a Seasons in the level, setting the year does nothing and says so in the Console; `Seasons.Exists` tells whether there is one.
