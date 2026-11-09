---
order: 3
---

A **world generator** makes ground from a seed: coasts, plains, hills and mountain ranges, as large as you like. Rather than noise that is equally bumpy everywhere, it reads a few broad fields over its world and shapes the ground from them, so plains lie beside ranges and valleys run between them. A terrain lays its ground with a **Generate** layer, and you can stamp, sculpt and dig over it as over any other.

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

**New ▸ World Generator** in the Project panel makes one. The first settings are all most worlds need:

| Setting | What it does |
| --- | --- |
| Preset | The kind of ground to start from (see below). Picking one reshapes the generator, keeping its seed and scale |
| Seed | Which world of that kind it makes. Each number makes other ground |
| Scale | How large everything is: 2 makes every coast, range and valley twice as large |
| Sea level | The height of the sea, in metres |
| Height | How high the highest mountains rise above the sea, in metres |

| Preset | Its ground |
| --- | --- |
| Battlefield Plains | Wide, nearly level farmland rolling gently, with no sea |
| Rolling Hills | Hills and dales, with no sea |
| Alpine | Ranges of high, sharp mountains with deep valleys, and coasts at their feet |
| Archipelago | Islands scattered over a sea |
| Canyons | High plateaus cut by steep-sided canyons |

Change anything under **Advanced** and the preset shows **Custom**.

Each change is saved as you make it, and every terrain in the open level generated from it is made again at once, so you see the ground change as you type. A generator edited in a text editor is read again when you come back to Voxear.

## How a generator shapes ground

A generator reads three fields over its world, each from one end of its range to the other:

- **Continents**: from deep sea, through the coast, to far inland.
- **Ruggedness**: from smooth country to rugged.
- **Ridges**: from the valleys between ranges to their crests, which run in long lines that wind and branch.

Four curves turn those into ground, each edited in its row under **Advanced** by dragging its keys. A press where there is no key adds one, and a key dragged off the graph is taken out.

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
| Bumps | How high the small bumps over all the ground are, in metres |

## The same ground everywhere

A generator's ground is worked out with nothing but sums, products, square roots and rounding down, so a seed makes exactly the same ground on every machine. A terrain keeps the heights it made in its `.heights` file, so a built game reads them as made and needs no generator.
