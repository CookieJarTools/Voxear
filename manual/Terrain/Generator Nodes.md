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

### Peaks

Great mountains. Ranges of sharp ridges rise from level land, and over them stand summits far higher than the rest, horns with ridges running down from them.

| Row | What it does |
| --- | --- |
| Size | How far apart the great summits are, in metres |
| Height | How high the tallest stand over Base, in metres |
| Base | The height of the level land between the ranges |
| Ranges | How much of the land is mountains, 0 to 1 |
| Sharpness | From rounded summits at 0 to horns and knife-edged ridges at 1 |
| Ridges | How much the ridges running from a summit stand out against the summit itself, 0 to 1 |

**Out:** Height, Crests (1 along the ridges' crests).

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

### Plateaus

Land in wide, level tablelands at uneven heights, with walls wherever two of unlike height meet. Tablelands of one height side by side are one, so they come in every shape and size.

| Row | What it does |
| --- | --- |
| Size | How wide a tableland runs before the next, in metres |
| Levels | How many heights there are, 2 to 12 |
| Height From, Height To | The valley floor's height and the highest tableland's, in metres. The levels between lie at uneven heights |
| Wall | How steep the walls stand |
| Ramps | How many of the walls have a ramp built against them, 0 to 1: a stair of steps a player walks up from one level to the next |
| Rim | Over 0, a lip raised along each tableland's edge, as many metres high. Under 0, the edge rounded off by as much |
| Talus | How high the slope of rubble at a wall's foot is, in metres |
| Valleys | How much of the land the lowest level takes, 0 to 1 |
| River | How deep a winding river bed is cut into the lowest level, in metres. Set the generator's Sea Level a little under Height From to fill it |
| Canyon | How wide a canyon the river cuts through the tablelands it meets, in metres, down to the valley floor. With 0 the river keeps to the valleys |
| Wander | How far the walls' lines bend, 0 to 1 |

Under Advanced: **Ramp Slope** (how steep the stairs climb), **Step** (how high each stair is, 1 m to be walked up in a world of 1 m blocks) and **Seed**.

A mask wired into **Ramps** keeps the ramps to where it is over a half.

**In:** Ramps. **Out:** Height, Walls (1 on walls and rubble), Floor (1 on the lowest level, clear of the walls).

### Chasms

The ground wired in, cut open by rifts and sinkholes: sheer-walled, hundreds of metres deep if you like, with ground at their floors as open to the sky as any other. A chasm's floor takes its column's layers, its biome, plants and structures as the land over it does.

| Row | What it does |
| --- | --- |
| Kind | **Rifts**, long winding cracks that run for kilometres and taper out at their ends; **Sinkholes**, round pits with ragged rims; or **Both** |
| Size | How far a rift runs before it bends, in metres. Sinkholes are a quarter of that apart |
| Width From, Width To | How wide a rift or a sinkhole is, in metres |
| Depth From, Depth To | How far down its floor lies under the ground it cuts, in metres |
| How Often | How much of the land has one, 0 to 1 |
| Wall | How steep the walls stand |
| Ledges | Over 0, the walls cut into shelves every so many metres down |
| Floor | How high the low hills its floor rolls in are, in metres |

A mask wired into **Mask** keeps the chasms to where it is over a half.

**In:** Ground, Mask. **Out:** Height, Floor (1 on the floors), Walls (1 on the walls and their ledges), Sunk (how far under the ground it cut the ground lies, in metres), Rim (the ground as it was before it was cut).

Wire Sunk into the Output's **Sunk** for biomes that keep to chasm floors, Walls into its **Rock** for bare walls, and Rim into a Roof's **Top** to cover chasms over into caverns.

### Rivers

Rivers running down the ground wired in, lakes in its hollows, and waterfalls where a river drops over a cliff. Wire its Height, Water and Falls into the Output's, and a [[Block Worlds|Block World]] fills its rivers and lakes with its **Sea block** and pours its waterfalls as falling water.

Water finds its way over a grid of points 64 m apart: each drains the steepest way down, and a hollow spills over its rim. How much ground drains through a point is counted from the points up its way within half a kilometre, worked out from the points alone, so every chunk of an endless world finds the same rivers and none stops at a seam.

| Row | What it does |
| --- | --- |
| Source | How much ground drains into a place before a river starts there, in hectares. Less makes more, smaller streams |
| Width | How wide a river is where a square kilometre drains into it, in metres. Where four times as much does, it is twice as wide |
| Depth | How deep its bed is there, in metres |
| Valley | How far either side the ground slopes down to its banks, in metres |
| Falls | Over how many metres' drop between two points the river pours over a waterfall, halfway, into a pool |
| Lakes | How much ground drains into a hollow before it holds a lake, in hectares; none at 0. A lake stands as high as the lowest of the ground round it, and rivers run in and out of it |
| Meander | How much the rivers wind, 0 to 1 |

Under Advanced: **Spacing** (how far apart the grid's points are) and **Seed**.

A river's water always runs downhill. Its bed is cut into the ground, and where the ground beside it is lower than its water, banks are raised, so water never stands over the air beside it.

**In:** Ground. **Out:** Height, Water (the water's height where there is a river or a lake), Falls (how many metres of a waterfall pour down there), Banks (1 on a river and its banks, fading out across its valley).

The water flows down its slope: a [[Block Worlds#Rivers and currents|river in a Block World]] carries what floats in it, and drags characters wading it.

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

## Voxels

Voxel nodes make the ground in three dimensions, for worlds of blocks and voxels. Where the nodes above make a number at each place on the ground, these make one at each place in space: how many metres inside the ground it is. They are wired into the Output's **Volume**, and a terrain of heights never reads them. Their sizes are metres whatever the generator's Scale.

| Node | What it makes |
| --- | --- |
| Solid | A height made solid: the generator's own ground with nothing wired in |
| Overhangs | Ground pushed in and out where its Mask says, and steep ground alone with no mask wired in |
| Tunnels | Long winding caves |
| Chambers | Wide low caves |
| Cave Network | Rooms made as prefabs, joined by winding tunnels: without end, or as one bounded level (see [[Caves]]) |
| Arches | Natural bridges, laid where two high places face one another across low ground |
| Raised Land | Islands of land standing high over the ground, floating or on a stalk |
| Spires | Towers of rock standing sheer out of the ground, grown over on top |
| Fins | Thin tilted blades of rock in rows |
| Roof | Ground laid back over low ground at a height wired in, so a chasm becomes a cavern under land like any other |
| Noise 3D | Noise through space |
| Combine | Two of these made one: Union, Subtract or Smooth |

A height or a mask from any node above can be wired into them: a Height Mask into the Overhangs' Mask keeps cliffs to the high ground. Their rows, and how a Block World's own Overhangs and Caves become these nodes, are in [[Block Worlds#Overhangs and caves in the generator|Overhangs and caves in the generator]].

### Arches

A span of ground from one high place to another across lower ground, with an opening under it and a top to walk over. The node looks the ground over a region at a time and lays a bridge where it finds high ground at both ends of a line and low ground between, so they stand across canyons and gaps and nowhere else.

| Row | What it does |
| --- | --- |
| Span From, Span To | How short and how long a bridge may be, in metres |
| Thickness | How thick it is at its middle, in metres. Its underside is an arch, thicker by a sixth of its span where it springs from the ground, and rounded across |
| How Often | How many of the places a bridge could stand get one, 0 to 1 |
| Width | How wide its top is, in metres |
| Windows | How many smaller openings are cut clean through thin ridges and walls, 0 to 1: a round way through at the height of the lower ground either side, with the ridge left whole over it |

**In:** Ground. **Out:** Ground.

### Raised Land

Islands of ground standing high over the land below, flat on top and tapering underneath. An island's top is ground like any other and takes its column's layers.

| Row | What it does |
| --- | --- |
| Size From, Size To | How far across an island may be, in metres |
| Height From, Height To | How far over the highest ground under it an island's top stands, in metres |
| How Often | How much of the sky has an island, 0 to 1 |
| Thickness | How deep an island is at its middle, in metres |
| Joined | Floating, or Stalk: a column of rock from the ground up into its middle |

**In:** Ground. **Out:** Ground.

### Spires

Towers of rock standing up out of the ground, from squat stacks to needles many times as tall as they are wide, sheer-sided and grooved where softer layers weathered back, their tops rounded and grown over.

| Row | What it does |
| --- | --- |
| Size From, Size To | How wide a spire is at its foot, in metres |
| Height From, Height To | How high its top stands over the ground at its foot, in metres |
| Spacing | How far apart the places a spire could stand are, in metres |
| How Often | How many of those places have one, 0 to 1 |
| Taper | How much it narrows towards its top, from a column at 0 to a point at 1 |
| Lean | How far it tips over, each its own way, 0 to 1 |
| Bands | How far apart the grooves round it are, in metres. 0 for none |
| Clusters | From scattered alone at 0 to gathered in groups with open ground between at 1 |

**In:** Ground. **Out:** Ground, Footprint (1 over every column a spire stands over).

### Fins

Blades of rock jutting from the ground, thin and tilted, standing in rows as tilted layers of hard rock stand where softer ground round them wore away. Neighbouring rows run the same way, turning slowly across the land, so a field of them reads as one tilted layer.

| Row | What it does |
| --- | --- |
| Length From, Length To | How long a fin runs along the ground, in metres |
| Height From, Height To | How high it stands, in metres |
| Thickness | How thick it is at its foot, in metres. It thins to an edge along its top |
| Dip | How far it leans, from upright |
| Rows | Up to how many stand side by side in a group |
| Spacing | How far apart the places a group could stand are, in metres |
| How Often | How many of those places have one, 0 to 1 |
| Jagged | How much its top is broken into teeth, 0 to 1 |

**In:** Ground. **Out:** Ground, Footprint (1 over every column a fin stands over).

Wire a Footprint into the Output's **Rock** and the fins or spires are bare rock from foot to top. Without it, a spire's top and every ledge on its sides are grown over.

### Roof

Ground laid back over low ground as a lid at the height wired into **Top**. Wire a Chasms node's Rim into it, and the roof's top lies where the land was before the chasm was cut: walking over it the land goes on as it did, until a hole shows the floor far below. A roof is laid only where Top is higher over the ground than the roof is thick, so it meets a chasm's walls where they are shallow and leaves the land round it alone.

| Row | What it does |
| --- | --- |
| Thickness | How thick the roof is, in metres |
| Cover | How much of the low ground is roofed over, 0 to 1, in broad stretches, so some chasms are open and some are caverns |
| Openings | How much of a roof has fallen in, 0 to 1 |
| Opening Size | How wide the holes are, in metres |
| Pillars | How many columns of rock stand from the floor up to the roof, 0 to 1 |

A mask wired into **Mask** keeps the roof to where it is over a half.

**In:** Ground, Top, Mask. **Out:** Ground.

The floor under a roof is the generator's ground, with its own layers, biome, plants and structures: a forest can grow in a cavern, lit through the holes over it.

An arch's top, an island's, a spire's, a fin's and a roof's take the top block of ground as high as they stand: the cover their own height and climate give it, not the valley's under them. Under that top block they are rock, so a tower's ledges and an island's sides are rock with green on them, not striped with soil. These nodes say where they stand, a column at a time, so a world of blocks asks about no more air than they fill, and they show in the far view. Overhangs after them roughens them. A Volume is read no further than 64 m over the generator's ground but for these.

## Output

What the generator makes. With nothing wired into Height the ground lies at the sea level; with no climate wired in it is 15 °C by the sea, cooler higher up, and half wet. With nothing wired into Volume a world of blocks makes the ground under the height solid, with the world's own Overhangs and Caves.

**Rock** takes a mask of where the ground is bare. Where it is over a half, a world of blocks leaves the top and under blocks off the column and shows its base block, so walls and crags are rock and not striped with soil. Wire a Slope Mask into it, or a Plateaus node's Walls.

**Sunk** takes how far under the land round about the ground lies, in metres: a Chasms node's Sunk. A world's biomes can keep to ground sunk so far (see [[Block Worlds#Deep ground|Deep ground]]), so a chasm's floor or a cavern's grows country of its own.

**Water** takes how high water stands where a river or a lake is, and **Falls** how many metres of it pour down as a waterfall's curtain: a Rivers node's. A world of blocks fills the air under the water with its Sea block, and a waterfall's curtain with falling water.

**In:** Height, Temperature, Humidity, Volume, Rock, Sunk, Water, Falls.

## What a node costs

Most nodes work each place out once. A few read the ground round each place, and everything wired in behind them is worked out again for every extra read:

| Node | Reads behind it |
| --- | --- |
| Slope Mask, Curvature, Gullies | 5 times |
| Smooth | 9 times |

A **Rivers** node reads the ground it runs down once more for each point of its grid, 64 m apart, a tile of them at a time. The first chunks of a place wait a moment for its tile.

These multiply. A Gullies node behind a Smooth works its own input out 45 times. Put the dear nodes as far along the graph as they will go, and add fine detail after them rather than before.
