---
order: 6
---

A terrain can go on **without end**. Its ground is made round the camera as it goes, from its layers, so a world has no edge to walk off: wherever the camera goes, there is ground.

## Making a terrain endless

Under **Settings**, tick **Endless**. Its **Size** and **Resolution** give way to:

| Setting | What it does |
| --- | --- |
| Spacing | How many metres apart its heights are: 0.25 to 4 |
| View distance | How far round the camera its ground is made, in metres |

Give it a **Generate** layer (see [[Generating Terrain]]), and its generator's world goes on wherever you look, mountains, coasts and climate with it. Untick **Endless** and it is a terrain of a set size again, made from its Obj. Each change is one step to undo.

## How the ground is made

The ground is made in tiles of 256 heights a side, 256 metres at a metre's spacing, as many round the camera as its **View distance** reaches, up to 33 each way. When the camera goes into another tile, the tiles left behind are let go and those that came into reach are made on threads of their own, the nearest first, so the game never waits for them. The ground under the camera and round it is made at once, so there is always ground to stand on. A tile not made yet is drawn as nothing, and nothing stands on it.

In a game the ground is made round the camera; in the editor, round where the Scene view looks from.

Each tile is made alone, from where its heights are, so tiles made apart meet exactly and the same ground comes back when you return. Nothing of it is kept but its `.terrain` file: no `.heights` beside it, and a built game makes its ground as it goes, from the generators and pictures its layers name. Its surfaces, by their rules, climate and biomes (see [[Surfaces]]), and grass growing on them are made with each tile.

A change to a layer makes the ground again, showing the old until the new is made. A stamp moved makes only the ground it reaches again, and a surface's rules changed only the shares over the ground as it is.

Each tile takes about a megabyte, its heights, shares and grass, on the CPU and again on the GPU: a **View distance** of 1500 m at a metre's spacing keeps 13 tiles each way, 169 in all.

## What makes its ground

Only layers worked out from where each height is make ground without end: **Generate**, **Noise**, **Stamp**, **Spline**, **Terrace** and **Smooth**. Stamps and splines stay where they are put, measured from the terrain's Obj. **Add Layer** offers only these.

Brushes, **Import**, **Erosion** and **Rivers** layers, holes and digging work on terrains of a set size: they paint into or work over the whole terrain at once. On an endless terrain the Sculpt, Paint and Grass brushes paint nothing, and changes from scripts (see [[Changing Terrain from Scripts]]) leave it as it is. Place a terrain of a set size over it where a battlefield needs shaping by hand.

## Far from the middle

Positions are exact to about half a millimetre within 8 km of the world's middle; further out, things that move begin to shake. To walk an endless terrain as far as the player likes, switch the [[Floating Origin]] on in Project Settings ▸ World: once the camera is 2 km from the world's middle, everything moves back towards it by whole kilometres, the ground with it. The tiles already made stay as they were, and the ground carries on round the camera.
