---
order: 3
---

A world generator is a graph of **nodes** joined by wires (see [[Generating Terrain]]). Each node makes a number at every place on the ground: a **height**, in metres, or a **mask**, from 0 where something is absent to 1 where it is wholly there. A wire carries what one node makes into another, and the **Output** node takes the finished ground.

This page lists every kind of node: what goes into it, what comes out, and its rows.

A few things hold for all of them:

- A length along the ground (a Size, a Radius, a Spacing) is multiplied by the generator's **Scale**. Heights are not.
- A node with noise of its own takes it from the generator's **Seed** and the node itself, so two Noise nodes never make the same noise, and a node keeps its noise when others are added or removed.
- An input with nothing wired into it uses the row of the same name, where the node has one.

## Sources

Sources make ground from nothing: they have no height input.

### Landmass

Continents, mountain ranges and gullies in one node: all a generator was before generators were graphs. A generator made then is this node wired to the Output still. Its rows are the preset's settings, listed in [[Generating Terrain#How a landmass shapes ground|How a landmass shapes ground]].

**Out:** Height.

### Noise

Noise over the ground, from **Low** where it is least to **High** where it is most.

| Row | What it does |
| --- | --- |
| Style | **Fractal** for rolling ground, **Ridged** for sharp ridges with soft valleys between, **Billow** for rounded heaps with creases between |
| Size | How wide its largest features are, in metres |
| Octaves | How many layers of finer detail it has, each half as large as the last |
| Low, High | What it makes at its least and at its most |

**Out:** Noise.

### Cells

The ground split into cells, each the ground nearest one of the points scattered over it.

| Row | What it does |
| --- | --- |
| Size | How far apart the points are, in metres |
| Jitter | How far each point strays from an even grid: 0 for cells in rows, 1 for ragged ones |

| Out | What it makes |
| --- | --- |
| Distance | How far the nearest point is: 0 at the point, 1 a cell's width away. Through a Curve it is a pit or a mound round every point: craters, hummocks, islets |
| Edges | How far the nearest border between two cells is: 0 on the border, 1 half a cell's width from it. The borders are cracks, or the walls between pans of dried mud |
| Value | A number of the cell's own from 0 to 1, the same all over the cell. Each cell is a step of its own height: mesas, fields, paving |

### Falloff

1 round a middle, easing to nothing further out. Multiplied into a height it leaves an island, or lets a level's ground sink into the sea at its edges.

| Row | What it does |
| --- | --- |
| Shape | **Round** for a disc, **Square** for a square with its sides along X and Z |
| X, Z | Where its middle is, in metres |
| Radius | How far from the middle it stays whole, in metres |
| Feather | Over how many metres beyond that it eases to nothing |

**Out:** Falloff.

### Constant

One number, the same everywhere.

**Row:** Value. **Out:** Value.

## Shape

Shape nodes change what is wired into them.

### Curve

What is wired in, read through a curve you draw. **From** and **To** say which numbers lie at the curve's two ends; the curve's height is what comes out. The curve passes smoothly through its keys and never overshoots them.

**In:** In. **Rows:** Curve, From, To. **Out:** Out.

### Terrace

The ground cut into steps, each a level tread and a riser up to the next.

| Row | What it does |
| --- | --- |
| Step | How high each step is, in metres. A tread lies at each whole number of steps: at 0, 20 and 40 for a Step of 20 |
| Sharpness | How much of each step is level tread: 0 leaves the ground as it was, 1 makes level steps with sheer risers |

**In:** In. **Out:** Out.

### Clamp

What is wired in, held between **Min** and **Max**.

| Row | What it does |
| --- | --- |
| Min, Max | The least and the most that comes out |
| Smooth | Rounds the corner where the ground meets either, over this many metres, so a sea bed held level or a plateau cut flat has no crease round it |

**In:** In. **Out:** Out.

### Math

Two numbers at each place, added, subtracted, multiplied, divided, or the lesser (**Min**) or greater (**Max**) of them. A number divided by 0 gives 0.

**In:** A, B. **Rows:** Operation, A, B. **Out:** Result.

### Blend

**A** where the mask is 0, **B** where it is 1, and a mix of the two between.

**In:** A, B, Mask. **Rows:** A, B, Mask. **Out:** Result.

### Warp

What is wired in, read a little way from where it is asked for, so straight lines wind and round shapes turn ragged. A Warp bends everything behind it together.

| Row | What it does |
| --- | --- |
| Size | Over how many metres the bending changes |
| Strength | How far a place is moved at most, in metres |
| Octaves | How many layers of finer bending it has |

**In:** In. **Out:** Out.

### Smooth

What is wired in, averaged round each place, so creases round off and small bumps flatten.

**In:** In. **Row:** Radius, how far round each place it averages, in metres. **Out:** Out.

## Masks

Each mask is 1 where the ground wired in fits its rows, and eases to nothing over its **Feather** outside them. Wire a mask into a Blend, or multiply it into what should only be there.

Angles are typed in degrees.

### Height Mask

Where the ground is between two heights: snow above a line, sand along a shore.

| Row | What it does |
| --- | --- |
| Min, Max | The heights it covers, in metres |
| Feather | Over how many metres below Min and above Max it eases to nothing |

**In:** Height. **Out:** Mask.

### Slope Mask

Where the ground is between two slopes: rock on cliffs, grass on what is level enough to hold it.

| Row | What it does |
| --- | --- |
| Min, Max | The slopes it covers, from 0° for level ground to 90° for straight up |
| Feather | Over how many degrees outside them it eases to nothing |
| Step | How far off the slope is measured, in metres. A longer Step follows the lie of the land and passes over small bumps |

**In:** Height. **Out:** Mask.

### Curvature

Where the ground bends. Both of its masks are 0 on level ground and on an even slope, however steep.

| Row | What it does |
| --- | --- |
| Size | How wide the crests and floors it finds are, in metres |
| Bend | How sharply the ground must turn to count wholly: the angle at which it falls away either side of a crest, or rises either side of a floor |
| Feather | Over how many degrees below Bend it eases to nothing |

| Out | What it makes |
| --- | --- |
| Ridges | 1 along crests and on peaks |
| Valleys | 1 along valley floors and in hollows |

**In:** Height.

### Climate Mask

Where it is as warm and as wet as its rows say. Wire a Climate node's Temperature and Humidity into it. With only one of them wired in, the other counts for any.

| Row | What it does |
| --- | --- |
| Biome | Sets both ranges to those of a kind of country: Snow, Tundra, Taiga, Grassland, Forest, Desert, Savanna or Rainforest. Change a range and it shows **Custom** |
| Temperature min, Temperature max | The temperatures it covers, in °C |
| Humidity min, Humidity max | The humidities it covers, from 0 for dry to 1 for wet |
| Temperature feather, Humidity feather | How far outside each range it eases to nothing |

**In:** Temperature, Humidity. **Out:** Mask.

## Erosion

### Gullies

The height wired in, with gullies cut down its slopes as running water cuts them. Small gullies run down the walls of larger ones and into them. Steep ground is cut deepest; level ground, crests and valley floors are left. Nothing is cut under the generator's sea.

| Row | What it does |
| --- | --- |
| Strength | How deeply it cuts: 0 for none, 1 for gullies a quarter as deep as they are apart on steep ground |
| Spacing | How far apart the largest gullies are, in metres. Keep it well under the width of the hills they run down |
| Octaves | In how many sizes gullies are cut, each half as far apart as the last |

**In:** Height. **Out:** Height.

Gullies cut by how steep the ground is, so nearly level ground gets none. To cut streams into a plain, multiply its height by 5 or so before the Gullies node and divide by the same after.

## Climate

### Climate

How warm and how wet the world is at each place, cooler the higher the ground. With nothing wired into its Height it is cooled by the ground it is asked about. Its rows are a generator's climate settings, listed in [[Generating Terrain#Climate|Climate]].

**In:** Height. **Out:** Temperature, Humidity.

## Output

What the generator makes. With nothing wired into Height the ground lies at the sea level; with no climate wired in it is 15 °C by the sea, cooler higher up, and half wet.

**In:** Height, Temperature, Humidity.

## What a node costs

Most nodes work each place out once. A few read the ground round each place, and everything wired in behind them is worked out again for every extra read:

| Node | Reads behind it |
| --- | --- |
| Slope Mask, Curvature, Gullies | 5 times |
| Smooth | 9 times |

These multiply. A Gullies node behind a Smooth works its own input out 45 times. Put the dear nodes as far along the graph as they will go, and add fine detail after them rather than before.

## Nodes on a planet

Round a planet (see [[Voxel Terrain#Planets|Planets]]) the same graph makes the ground, with no seams:

- Noise, Cells and Warp are read in three dimensions.
- A Falloff's middle is the point of the planet's upper half over its X and Z: the north pole, with both at 0.
- Slope Mask, Curvature and Smooth read the ground round each place along the planet's surface.
- Gullies leave the ground as it is: they need flat ground to run down.
