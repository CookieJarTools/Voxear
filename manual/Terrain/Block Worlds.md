---
order: 17
---

A **Block World** is ground made of blocks: a cube each, of the kinds its world has, grass, dirt, stone or anything you make, from big blocks to build with down to small ones to sculpt with. It reaches from its Obj's position along +X, +Y and +Z, and only the Obj's position places it.

## Making a world of blocks

Select an Obj and pick **Add Component ▸ Terrain ▸ Block World**. It starts as flat ground 64 blocks a side and 8 up, grass over three layers of dirt over stone, on a new asset of its own in the project's **Blocks** folder, named for its Obj: `Blocks/Ground.blockworld`. Every Block World that picks the same asset has the same blocks.

| Setting | What it does |
| --- | --- |
| Data | The `.blockworld` asset its blocks are |
| Block size | How many metres a block is a side |
| Ground size | How many blocks its ground reaches along X and Z |
| Ground height | How many blocks high its ground is |
| Cast shadows | Whether its blocks cast the sun's shadows |

A change to its ground makes its blocks again at once. Each change is one step to undo, and is written to the asset when you save the level. Its blocks are made from the asset as it loads, the same on every machine, so the `.blockworld` file is all that is kept.

## Kinds of block

A world's kinds of block are listed in its `.blockworld` file, numbered from 1 in the order they come. Each has a name and a colour, and can have materials: one for every face, and others for its top and its bottom.

```toml
[[blocks]]
name = "Grass"
color = [0.33, 0.52, 0.2]
top = "Materials/Grass.material"
material = "Materials/Dirt.material"
```

A face with a material is drawn with it, its pictures repeated a block at a time and taken from the side the face looks, so stripes on a side run straight down it. A face with none is drawn in the block's colour. The ground's layers name their blocks by number.

## How blocks are drawn

The blocks are kept in chunks of 32 a side, only the chunks with blocks in them, so a world is as large as its blocks and no larger. A face is drawn only where a block meets air, and faces side by side drawn the same way are drawn as one, so a field of grass is a handful of quads. Each corner of a face is a little darker as the blocks round it close it in, which gives edges and hollows their depth.

The camera draws only the chunks in view, and of each only the faces turned towards it. The sun's shadows fall from every face. Blocks light the world round them too, as the GI's rays see them.

## Collision

A world of blocks collides as it is drawn: physics stands things on its blocks, a mesh for each chunk, and rays from **Physics.Raycast** meet them, its Obj as what they hit. It needs no Collider. Its Obj's **Layer** is its blocks' layer.
