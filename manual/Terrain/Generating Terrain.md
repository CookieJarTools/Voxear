---
order: 3
---

A **world generator** makes ground from a seed: coasts, plains, hills and mountain ranges, as large as you like. Rather than noise that is equally bumpy everywhere, it reads a few broad fields over its world and shapes the ground from them, so plains lie beside ranges and valleys run between them. A terrain lays its ground with a **Generate** layer, and you can stamp, sculpt and dig over it as over any other.

A generator is a graph of nodes underneath. This page is the short way: a preset and a few rows. To build ground of your own from nodes, see [[Generator Graphs]].

## Generating a terrain

1. Pick **Layers** along the top of the Terrain in the Inspector.
2. **Add Layer ▸ Generate**. The layer picks the project's world generator, or makes one beside the terrain's file if the project has none, and lays its ground at once.
3. Click the generator in the Project panel, or open it from the layer's **Generator** field, to change it.

| Setting | What it does |
| --- | --- |
| Generator | The world generator the ground comes from |
| Offset | Where the terrain's corner is in the generator's world, in metres along X and Z. Move it to use another stretch of the same world |

A new Generate layer **Sets** the ground, so the layers below it count for nothing until you change its **Blend**. Put stamps, sculpting and splines above it.

The same generator makes the same ground for every terrain that uses it, so terrains side by side, each offset by its own place, join with no seam.

## World generators

**New ▸ World Generator** in the Project panel makes one: the **Rolling Hills** preset, with seed 1. Select it there and the Inspector shows its own rows, which are all most worlds need:

| Setting | What it does |
| --- | --- |
| Preset | The kind of ground to start from (see [[Generating Terrain#Presets|Presets]]). Picking one puts that preset's graph in place of the generator's, keeping its seed and scale |
| Seed | Which world of that kind it makes. Each number makes other ground |
| Scale | How large everything is: 2 makes every coast, range and valley twice as large |
| Sea level | The height of the sea, in metres |
| Height | How high the ground rises, in metres |
| Temperature | How warm it is by the sea on average, in °C |
| Humidity | How wet it is on average, from 0 for dry to 1 for wet |

Each preset has a row or two more of its own, listed with it below.

**Open Graph**, under the rows, shows the generator's graph in the Generator panel; so does a double click on the generator in the Project panel.

Seed, Scale and Sea level are the generator's own. The rest are values of nodes in its graph that the graph **exposes** as rows here, and a graph can expose any others (see [[Generator Graphs#Exposing a value|Exposing a value]]).

Each change is saved as you make it, and every terrain in the open level generated from it is made again at once, so you see the ground change as you type. A row dragged by its label makes the ground again when you let go. A generator edited in a text editor is read again when you come back to Voxear.

## Presets

A preset is a graph of nodes built and tuned for one kind of ground, with a few of its values as rows:

| Preset | Its ground | Its own rows |
| --- | --- | --- |
| Battlefield Plains | Wide, nearly level farmland rolling gently, with low rises and the shallow dips of streams | **Streams**: how deeply the streams cut. **Craters**: shell holes where the shelling was heaviest; none at 0 |
| Rolling Hills | Hills and dales whose valleys branch, with no sea | **Gullies**: how deeply water has cut the valleys |
| Alpine | Ranges of high, sharp mountains with deep valleys, and coasts at their feet | **Gullies** |
| Archipelago | Islands scattered over a sea, with ragged coasts, shallows and islets | **Gullies** |
| Canyons | High ground in broad steps, cut by steep-sided canyons with ledges down their walls | **Ravines**: how deeply ravines cut back into the canyons' walls |

The Preset row is there while the generator's graph is a preset's still, whatever its rows are set to. Change the graph itself, by adding a node, moving a wire or changing a value that is not one of its rows, and the ground is your own: the row goes, since picking a preset would put its graph in place of yours. [[Generator Graphs]] is about the graph, and [[Generator Nodes]] about what each node does.

### Generators made before presets were graphs

A generator made in an earlier version of Voxear is a single **Landmass** node, and makes exactly the ground it always made. Its Preset row picks among the landmass's own presets instead. They have the same five names and make the older ground, and **Custom** shows once the landmass's shape is its own.

To give a terrain the newer ground, make a new generator and pick it in the Generate layer's **Generator** field.

## How a landmass shapes ground

A **Landmass** node is continents, mountain ranges and gullies in one node: what every generator was before generators were graphs, and a node you can still add to any graph. Open the graph and pick the node, and the Inspector shows its settings under **Advanced**. Change any of them and its preset shows **Custom**.

A landmass reads three fields over its world, each from one end of its range to the other:

- **Continents**: from deep sea, through the coast, to far inland.
- **Ruggedness**: from smooth country to rugged.
- **Ridges**: from the valleys between ranges to their crests, which run in long lines that wind and branch.

Four curves turn those into ground, each edited in its row by dragging its keys. A press where there is no key adds one, and a key dragged off the graph is taken out.

| Curve | Across | What it gives |
| --- | --- | --- |
| Coast | Continents, from deep sea to far inland | The ground's height there, as a share of **Height**: below 0 is under the sea |
| Inland | Continents | How much relief land so far inland has, 0 to 1: none at sea, more inland |
| Relief | Ruggedness, from smooth to rugged | How much relief that country has, 0 to 1 |
| Peaks | Ridges, from valleys to crests | How the relief rises and falls, from -1 to 1 |

The ground stands at the **Coast** curve's height and rises or falls from there by its relief, **Inland** times **Relief**, as **Peaks** shapes it. So a coast can rise into plains, then hills, then sharp ranges further in, all as the curves say. The curves are drawn smooth through their keys, and never overshoot them.

| Setting | What it does |
| --- | --- |
| Continents size, Ruggedness size, Ridges size | How wide each field's largest features are, in metres |
| Continents octaves, Ruggedness octaves, Ridges octaves | How many layers of finer detail each field has, each half as large as the last |
| Sharpness | How sharp ridges' crests are: 0 rounded, for hills; 1 knife-edged, for mountains |
| Warp size, Warp strength | How the world is bent before the fields are read, so coasts and ranges wind rather than run straight: over how many metres, and by how many metres at most |
| Crags | How rough the highest peaks are, as a share of **Height** |
| Erosion | How deeply water cuts gullies down the slopes: 0 for none, 1 for gullies a quarter as deep as they are apart on steep ground |
| Gully spacing | How far apart the largest gullies are, in metres |
| Gully octaves | In how many sizes gullies are cut, each half as far apart as the last |
| Bumps | How high the small bumps over all the ground are, in metres |

## Gullies

Water running off ground cuts gullies down its slopes, and the gullies join as they go down, as streams join a river. A generator cuts them in a few sizes, the largest first: each size runs down the slope the ground has after the sizes before cut it, so small gullies run down the walls of larger ones and into them. Steep ground is cut deepest; ridges and valley floors, level across, are left as they are, so peaks stay sharp and valleys flat-bottomed. Gullies grow shallower towards the sea and stop at its edge.

Gullies make mountains look worn by weather rather than stretched noise, and canyon walls fluted. For the channels running water carves, and the fans of what it washes down, add an [[Erosion]] layer above the Generate layer.

## Climate

A generator's world has a climate: how warm and wet each place is, which its terrains' surfaces can cover by (see [[Surfaces#Biomes|Biomes]]), so deserts, forests and snowfields fall where the world puts them and follow the ground as it changes. Each preset has a climate to suit it: Canyons hot and dry, Archipelago warm and wet, Alpine cool.

The climate is the **Climate** node of the generator's graph. Its Temperature and Humidity are rows of the generator; pick the node in the graph for the rest, under **Advanced**:

| Setting | What it does |
| --- | --- |
| Climate size | How wide its warmer and colder, wetter and drier places are, in metres. Smaller gives more biomes on one terrain |
| Temperature range | How far its temperature strays from its average, in °C |
| Humidity range | How far its humidity strays from its average |
| Cooling | How many °C colder it is for every 1000 m the ground rises above the sea |

## The same ground everywhere

A generator's ground is worked out with nothing but sums, products, square roots and rounding down, so a seed makes exactly the same ground on every machine. A terrain keeps the heights it made in its `.heights` file, so a built game reads them as made and needs no generator. A terrain without end makes its ground as it is seen, from its generator, in the game as in the editor: see [[Endless Worlds]].
