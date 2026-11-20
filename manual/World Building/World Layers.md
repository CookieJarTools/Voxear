---
order: 1
---

A **world layer** changes what the ground of a world of blocks without end comes out as, without storing a block. It is data on an Obj, saved with the scene. Move the Obj, change it or delete it, and the chunks under its old and new place are made again: the ground comes back as the generator made it.

A [[Ground Fit]], a [[Roads|Road]] and an Area Fill are world layers. Painted maps are coming.

## The order ground is made in

The ground of each column of blocks is worked out in this order:

1. **The generator's ground**, as its graph makes it.
2. **World layers.** Ground Fits and Area Fills first, then Roads. Within each, where two meet, the higher **Priority** wins. Between two of the same priority the order is the same on every machine.
3. **Blocks placed and broken by hand**, from the world's `.blocks` file, on top of everything. A block you placed inside a layer's reach stays where you put it.

Each column is worked out from where it is and nothing else. So chunks made apart meet exactly, and the same ground comes back every time, on every machine and over the network.

## What a change makes again

A world keeps its layers by the 32-block chunk column each reaches, so making a chunk asks only the layers that reach it. A capital city of thousands of buildings costs a lookup per column.

When a layer changes, only the columns under where it was and where it is now are made again:

- **Near columns** are made again in the background. Your hand edits are put back on, and the old blocks are drawn until the new ones are ready.
- **The far view and the map** show layers from the start. Raised quarters of a city show as raised from kilometres off, without the camera having been there.
- **Navigation** picks up the new ground with the chunks.

Dragging a house in the Scene view remakes the ground under it as you drag.

## Seeing the bare ground

To see the ground without its layers, turn on **Gizmos ▸ Bare Ground** in the Scene view. The scene is not changed. From the command line:

```
vox editor call world_layers.hide fits=true
vox editor call world_layers.hide fits=false
```

`world_layers.status` lists each world's layers: how many Ground Fits, and how many chunk columns they reach.

Layers shape worlds without end. A Block World of a set size is generated once and kept as blocks.
