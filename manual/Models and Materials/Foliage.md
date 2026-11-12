---
order: 4
---

A **Foliage** grows one plant in many places at once: a meadow of grass, a forest floor of ferns, a field of flowers. Thousands of plants are drawn without an Obj each, so a level can have far more of them than of Objs, and each is still culled, given its LOD, moved by the [[Wind]] and pushed aside by what passes through it, as any mesh is.

## Adding foliage

Add a **Foliage** to an Obj (Add Component, then Rendering). It starts as small cubes scattered over ten metres round the Obj; change its mesh and material in the scene file to your plant's, as a Mesh Renderer names them. A plant can be a [[Box Models|box model]] sculpted in the editor.

| Setting | What it does |
| --- | --- |
| Draw distance | How far from the camera plants are drawn, in metres. Grass looks right at 40 to 80; bushes and ferns further |
| Fade distance | Over how many metres short of the draw distance they fade out, so they thin away rather than stop at a line |
| Shadow distance | How far from the camera they cast shadows, in metres. Grass far away casts shadows too small to see; 0 for none at all |
| Healthy colour, Dry colour | Two colours the plants are tinted between, as Unity's grass is: each plant takes some of each, in patches, so a field is greener in places and drier in others. White for both leaves the plants as made |
| Colour patches | How wide those patches are, in metres |
| Impostors | Whether far plants are drawn as pictures of themselves (see below): **Auto** for the engine to choose, **Off** for never, or **From distance** |
| Impostors from | With From distance: how far from the camera, in metres, plants become pictures |

## Where plants grow

Plants grow in the Obj's space, so moving, turning or scaling the Obj moves all of them. They grow in four ways: **painted** onto the ground in the Scene view with the Foliage's **Paint** tool, each dropped onto the terrain or mesh below it and leaning with it (see [[Painting and Cutting Plants]]); scattered; placed one by one; and grown by a Block World by itself (see [[Foliage#Growing on a Block World]]).

**Scattered** over an area round the Obj, evenly but not in rows, each turned its own way and sized at random within a range:

```toml
[obj.foliage]
mesh = "Models/Grass.glb#mesh0.0"
material = "Materials/Grass.material"
draw_distance = 60.0
[obj.foliage.scatter]
size = [80.0, 80.0]   # metres across, along X and Z
density = 2.0         # plants to a square metre
seed = 0              # another seed scatters them another way
scale = [0.8, 1.2]    # the smallest and largest
```

**Placed** one by one, each as `[x, y, z, turn, size]`, its place in the Obj's space, its turn in degrees and its size, and for a plant leaning with a slope two numbers more, the x and z of the way its top points:

```toml
plants = [[1.0, 0.0, 2.0, 45.0, 1.1], [3.5, 0.0, -1.0, 200.0, 0.9, 0.2, 0.0]]
```

Scattered plants stand on the Obj's level, so scatter over flat ground, or paint plants instead, which find the ground themselves. Painting turns a Foliage's scattered plants into placed ones, to paint among them.

## Growing on a Block World

A Foliage under a [[Block Worlds|Block World]] can grow by itself. The world works out where its plants stand from its own ground, one column of chunks at a time. So a world without end grows its plants as its ground is made round the camera, and the same world grows the same plants every time it is opened, on every machine.

Make the Foliage's Obj a child of the Block World's Obj, open the Foliage's **Grow** tool and tick **Grow on Block World**. A Block World's **Add plant** row does all of this for you (see [[Block Worlds#Plants a world grows]]).

| Setting | What it does |
| --- | --- |
| On block | The block the plants stand on. None for any ground |
| Biome | Sets Temperature and Humidity to a climate at once: Forest, Desert, Snow and the rest. **Any** grows everywhere |
| Temperature, Humidity | Tick one to grow only between two temperatures, in °C, or between two humidities, from 0 for dry to 1 for wet, as the world's generator gives them. A world with no generator has no climate, so these change nothing there |
| Height | Tick to grow only between two heights over the sea, in metres |
| Steepest ground | Plants stay off ground steeper than this |
| Spacing | How far apart plants are tried, in metres: one try to a square that size. 1 gives a plant to every block of a world of metre blocks |
| Least apart | The nearest two plants may stand to each other, in metres |
| Chance | How many of the tries grow, from none to all |
| Patch size | Plants grow in patches about this many metres across, with as much bare ground between them. 0 grows them everywhere |
| Sizes | The smallest and the largest |
| Align to blocks | Stands each plant on the middle of its block, turned by quarter turns, so it sits square with the blocks. Off, a plant stands anywhere on its block, turned any way |
| Seed | Another seed grows the plants in other places. Give each Foliage of a world its own, or two kinds of plant try the same places |

A plant stands on the highest block of its place, under the open sky, and only on ground the world made: nothing grows on what was built afterwards. Dig the block from under a plant, change it, or build on the plant's place, and the plant is gone. Put the ground back as it was and the plant is back. Nothing about grown plants is saved: a save keeps the world's blocks, and the plants follow from them.

Plants are grown only as far as they are drawn. A Foliage's grown plants reach its **Draw distance** from the camera and no further than the world's own blocks, its **View distance**. Past that the far view shows the ground bare for now. In the editor they grow round where the Scene view looks from.

Trunks and the Cut prefab work for grown plants as they do for painted ones. A Foliage that grows can still have plants painted or placed on it; they are drawn beside the grown ones.

```toml
[obj.foliage]
mesh = "Plants/Tree.boxmodel#mesh0.0"
material = "Plants/Tree.boxmodel#material0"
draw_distance = 250.0
[obj.foliage.grow]
on = ["Blocks/Grass.block"]   # the blocks it stands on; left out for any ground
humidity = [0.45, 1.0]        # only where it is wet enough
spacing = 7.0                 # metres between tries
separation = 4.0              # the least between two plants
chance = 0.6                  # how many tries grow
slope = 25.0                  # the steepest ground, in degrees
scale = [0.8, 1.3]
seed = 4
```

`temperature` and `height` are ranges like `humidity`; `patches` is the patch size; `aligned = false` lets plants stand anywhere on their block.

Clicking a grown plant in the Scene view selects its Foliage, as clicking a painted one does.

Not done yet: a plant cut down from a script comes back when its ground is made again, as the camera leaves and returns.

## How many

A view draws up to 65,536 meshes, Objs and plants together. Plants are counted only within their draw distance of the camera, so a grass clump to a square metre drawn out to 60 metres is about 11,000 plants however large the field. A plant mesh can be a whole clump of blades: fewer, larger plants draw faster than many small ones.

Clicking a plant in the Scene view selects its Foliage's Obj. Trees that collide, what one becomes when it is cut down, and plants in scripts are in [[Painting and Cutting Plants]]. Something dragged into the Scene view over plants lands on the ground among them, not on top of them.

## Far trees

A tree far away covers a few pixels but still costs its whole mesh. So past a distance, each plant is drawn as an **impostor** instead: a flat picture of the plant that turns to face the camera. The pictures are taken by the engine itself, the first time they are needed, from 64 directions all round the plant's upper half, colour and normals both, so an impostor is lit by the sun, the sky and bounced light as the plant would be, and shows the side of the plant that faces you.

On **Auto**, plants two metres across or larger become impostors past 40 metres, or past a third of the draw distance if that is further; grass and small plants never do, as their own meshes are already cheap. Set **From distance** to choose the distance yourself, or **Off** to always draw the mesh.

```toml
[obj.foliage]
impostors = "from"        # auto, off or from
impostor_distance = 60.0
```

Impostors cast no shadows, so keep the shadow distance within the impostor distance; and rays for bounced light and reflections pass through them, as they would through a flat card.
