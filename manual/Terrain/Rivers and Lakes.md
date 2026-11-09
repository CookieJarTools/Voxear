---
order: 5
---

A **Rivers** layer finds where water would run over the ground below it and cuts rivers there: they start where enough ground drains into one place, wind down the valleys, and widen and deepen as more water joins them, always running downhill. Where a hollow would fill before it spilt over, a lake stands. **Add Water** then fills them with [[Streams]] and [[Pools and Dry Areas|Pools]], so they flow and shine as any water you place.

It suits ground made by a [[Generating Terrain|generator]] or noise: put it above them, and above an [[Erosion]] layer if there is one.

## Cutting rivers

1. Pick **Layers** along the top of the Terrain in the Inspector.
2. **Add Layer ▸ Rivers**. It cuts the rivers it finds at once.
3. Change its settings, then press **Carve Again** to cut them as they now say.
4. Press **Add Water** to fill them.

| Setting | What it does |
| --- | --- |
| Source | How much ground drains into one place before a river starts there, in hectares (a hectare is 100 m by 100 m). Less makes more, smaller streams, starting higher up |
| Width | How wide a river's floor is where a square kilometre of ground drains into it, in metres. Where four times as much does, it is twice as wide |
| Depth | How deep its bed is cut there, in metres; deeper as more drains into it |

The button reads **Carve Again** whenever the ground below the layer, or its own settings, changed since it last cut its rivers. Each time it cuts is one step to undo.

Water runs off the terrain's edges and into the sea. Over a [[Generating Terrain|generated]] terrain whose sea reaches the ground, rivers end where they meet the sea, and nothing is cut under it.

## Adding water

**Add Water** makes an Obj named **Rivers and Lakes** at the terrain's corner, holding:

- a **Stream** along each river, on a Path that runs down the middle of its bed, its water as wide as the river's banks at its height;
- a **Pool** over each lake, at the height it fills to: a lake of a winding shape is covered by a few Pools side by side, none over ground outside it that lies lower than its water.

They are ordinary Streams and Pools: change their look, or move their Paths' points, as for any other. Each river flows downhill as fast as its slope makes it, with white water where it runs steep, and slows into the lake or sea it reaches. The whole is one step to undo, and deleting the Obj takes it away. Add Water again after **Carve Again**, deleting the old water first, so the water follows the rivers' new beds.

## How rivers are found

The ground below the layer is worked on as a grid of points, as [[Erosion]] works on it. Every hollow is filled, in the mind of the layer, to where it would spill, so all water has a way off the ground: each point sends its water to the neighbour it drains through, and the ground draining through each point is counted. A river runs where the count passes **Source**, its bed cut under the filled ground along its course and its banks rising as steeply as they go up. A lake stands where a filled hollow is half a metre deep or more and a fifth of a hectare across or more.

It takes a moment on the CPU, and is the same every time for the same ground. What it cut is kept in the terrain's `.layers` file, as what the [[Sculpting]] brushes paint is, so a game reads the ground as made.
