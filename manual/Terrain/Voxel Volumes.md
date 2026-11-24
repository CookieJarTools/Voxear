---
order: 30
---

A **Voxel Volume** is a box of small voxels, a few centimetres across, on an Obj: grass blades a voxel wide, flowers of a handful of voxels, trees whose leaves are clouds of small cubes. It has no meshes. The GPU walks each pixel's ray through the volume's voxels, so millions of them cost about what a picture of them costs, however many there are.

Add one with **Add Component ▸ Rendering ▸ Voxel Volume**, or make one with the Hierarchy's **+ ▸ Voxel Volume**. Its least corner is at its Obj's place.

## Its size and voxels

- **Size**: how large the box is, in metres along X, Y and Z.
- **Voxels a Metre**: how small its voxels are. 16 makes voxels about 6 cm across; 8 makes them 12 cm, and 32 about 3 cm. Halving the voxels' size takes about four to five times the memory.
- **Roughness**: how rough its voxels are, from 0 to 1.

A volume is made again on a thread of its own whenever it changes; until then the last one made is drawn. A meadow 80 m by 120 m at 16 voxels a metre, with sixty trees stamped in, takes under two seconds.

## Its own ground

With **Ground** on, the volume has rolling ground of its own voxels, grass on it and flowers here and there:

- **Ground Height**: how high the ground stands over the volume's floor, in metres.
- **Hills** and **Hill Spacing**: how high its hills rise over that, and how far apart they are.
- **Grass** and **Grass Height**: how much of the ground grows grass, from 0 to 1, and how tall the tallest blades are, in metres.
- **Flowers**: how many flowers grow, from 0 to 1: red with yellow middles, purple spikes, white tufts, and pink ones with yellow middles.
- **Seed**: two volumes alike but for their Seed have different ground.

## Box models stamped in

A [[Box Models|box model]]'s sculpted cubes can be stamped into a volume, turned into its voxels: a tree sculpted at 16 cubes a metre becomes a tree of the volume's voxels. Each stamp is the model, where its feet stand in metres from the volume's corner, how far it is turned about Y in degrees, and how many times its size it is. A model's cubes larger than the volume's voxels fill all the voxels they cover; smaller ones are rounded to the voxels.

The Inspector shows how many models are stamped. Stamps are written in the scene:

```toml
[obj.voxel_volume]
size = [32.0, 12.0, 32.0]
voxels_per_metre = 16
ground_on = true
ground = { height = 2.0, hills = 1.5, grass = 0.6, flowers = 0.2 }

[[obj.voxel_volume.stamps]]
model = "Plants/Oak.boxmodel"
at = [12.0, 2.0, 9.0]
turn = 30.0
```

## How it is drawn

A volume is lit as everything else is: by the sun, with the shadows of meshes, blocks and terrain on it, by lamps, by the sky's light and bounced light, with ambient occlusion and reflections, and in the scene's fog. Its voxels shadow each other, traced through the volume towards the sun.

Far off, where a voxel is smaller than half a pixel, it is drawn as groups of two by two by two, and further still as whole bricks of eight voxels a side in their average colour, so far grass and leaves don't shimmer.

The Profiler's **Voxel coarse rays**, **Depth (voxels)** and **Opaque (voxels)** say what volumes cost. A meadow 80 m by 120 m with sixty trees, seen from in it at 1920 × 1080 on an RTX 3070, takes about 0.1 ms for the coarse rays, 2 ms for the depth pass and 3.8 ms to light.

## What it doesn't do yet

- Its voxels cast no shadow onto meshes, blocks or terrain.
- It isn't turned or scaled with its Obj.
- It is drawn whole, however far away it is.
- Its voxels can't be changed one by one from the editor or from scripts yet.
