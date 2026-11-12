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

Plants grow in the Obj's space, so moving, turning or scaling the Obj moves all of them. They grow in three ways: **painted** onto the ground in the Scene view with the Foliage's **Paint** tool, each dropped onto the terrain or mesh below it and leaning with it (see [[Painting and Cutting Plants]]); scattered; and placed one by one.

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
