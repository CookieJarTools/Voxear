---
order: 4
---

A **Foliage** grows one plant in many places at once: a meadow of grass, a forest floor of ferns, a field of flowers. Thousands of plants are drawn without an Obj each, so a level can have far more of them than of Objs, and each is still culled, given its LOD, moved by the [[Wind]] and pushed aside by what passes through it, as any mesh is.

## Adding foliage

Add a **Foliage** to an Obj (Add Component, then Rendering). It starts as small cubes scattered over ten metres round the Obj; change its mesh and material in the scene file to your plant's, as a Mesh Renderer names them.

| Setting | What it does |
| --- | --- |
| Draw distance | How far from the camera plants are drawn, in metres. Grass looks right at 40 to 80; bushes and ferns further |
| Fade distance | Over how many metres short of the draw distance they fade out, so they thin away rather than stop at a line |
| Shadow distance | How far from the camera they cast shadows, in metres. Grass far away casts shadows too small to see; 0 for none at all |
| Healthy colour, Dry colour | Two colours the plants are tinted between, as Unity's grass is: each plant takes some of each, in patches, so a field is greener in places and drier in others. White for both leaves the plants as made |
| Colour patches | How wide those patches are, in metres |

## Where plants grow

Plants grow in the Obj's space, so moving, turning or scaling the Obj moves all of them. They grow in two ways, which you can use together.

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

**Placed** one by one, each as `[x, y, z, turn, size]`, its place in the Obj's space, its turn in degrees and its size:

```toml
plants = [[1.0, 0.0, 2.0, 45.0, 1.1], [3.5, 0.0, -1.0, 200.0, 0.9]]
```

Scattered plants stand on the Obj's level, so scatter over flat ground, or use several Foliage Objs at different heights.

## How many

A view draws up to 65,536 meshes, Objs and plants together. Plants are counted only within their draw distance of the camera, so a grass clump to a square metre drawn out to 60 metres is about 11,000 plants however large the field. A plant mesh can be a whole clump of blades: fewer, larger plants draw faster than many small ones.

> [!NOTE]
> Click a Foliage's Obj in the Hierarchy to select it: clicking a plant in the Scene view does not select it yet.
