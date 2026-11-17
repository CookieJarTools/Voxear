---
order: 18
---

A [[Block Worlds|Block World]] of cubes is lit by **block light** unless you say otherwise: light kept for every block, worked out from the blocks themselves. Caves come out dark, and torches and lava light them; the world looks the same under every lighting look and at every hour of the day, and none of it costs anything a frame.

## Sky light and block light

Each block keeps two lights, each from 0 to 15:

- **Sky light** comes straight down from open sky at its full 15, through air, water and glass. From there it spreads out a step less each block: sideways, round corners, in under overhangs and down into caves. Leaves and blocks of parts (slabs, stairs, fences, doors) take it in and pass it on, but never let it straight down, so the ground under a tree's crown is in its shade. Solid blocks stop it.
- **Block light** floods out from blocks that give light, a step less each block, the same way. A block's **Light** says how much it gives, from 0 for none to 15: a torch 14, lava 15 (see [[Blocks#Look|a block's settings]]). Light from two blocks does not add up: each block takes the brighter.

Sunlight down a shaft dug into a cave lights the cave floor under it at the full 15, and the floor round it a step less each block out from there. Fill the shaft, and the cave is dark again.

A face takes the light of the blocks in front of it. Each of its corners takes the mean of the blocks touching that corner, so light runs smoothly across faces and round corners, never in steps. Faces side by side are drawn as one only where their corners' light is the same.

## How a face is lit

- **The light from all round** is the sky's light, as much as the face's sky light lets in, and the warm light the world's blocks give, as much as its block light. A face under open sky takes all of the sky's light, one six steps in under an overhang a sixth of it, and a face in a sealed cave almost none.
- **Both follow the time of day and the lighting look.** The sky's light is the sky's of the moment, so a cave mouth is lit by the noon sky at noon and by the night sky at night. Block light shows brightest at night and underground, where the eye has adapted to the dark.
- **The sun** falls as its shadows say. Past where its shadows reach, sky light stands in for them: a face out in the open takes the whole sun, a face under an overhang or in a cave none. Within them, a face with almost no sky light takes no sun either, so sunlight never leaks into a cave through what the shadows miss.
- **Reflections** of the sky are as dark as the sky light leaves them: a polished floor in a cave shows the torches' light, not the open sky's. What the screen's reflections find is as they find it.
- **Water** in the world is lit the same way, so a pool in a cave is dark but where a torch lights it.
- Corners the blocks round them close in are darker still, and ambient occlusion darkens what is near, as without block light.

Bounced light ([[Bounced Light|GI]]) is left out of faces lit by block light, which is what keeps them steady: switching lighting looks, or GI on and off, changes the sky and the sun but never leaves the probes' old light on the blocks for a while. Other things in the scene, models and characters, are lit as before.

A world without end's far rings are lit the same way, each coarse column from its own blocks, so a hillside reads the same near and far, and so does the shade under a wood.

## Settings

On the Block World, under **Settings**:

| Setting | What it does |
| --- | --- |
| Lighting | **Block Light**, as above; or **Scene**, lit as the rest of the scene is: by the GI's probes where GI is on, and the sky's light darkened by ambient occlusion |
| Light color | With Block Light, the colour of the light the world's blocks give: a torch's warm light unless you say |
| Light strength | With Block Light, how bright that light is: 1 as usual, up to 8 |

In the `.blockworld` file, each only where it is not as usual:

```toml
lighting = "scene"              # "blocks" unless it says
glow_color = [1.0, 0.72, 0.42]  # sRGB
glow = 1.0
```

**Scene** is the full way: light bounced off each block's colour, light into a room through its doorway as far as the rays find it. It costs the GI's probes each frame, and its light takes a moment to settle when the lighting changes. A [[Voxel Terrain]] is always lit as the scene is.

## From scripts and the command line

- `Block.Light` is how much light a block gives, from 0 to 15.
- `vox editor call blocks.get at=[x,y,z]` gives the block there with its `sky_light` and `block_light`.
- `vox editor call field.set obj=World component=block_world field=lighting value=Scene` lights a Block World as the scene is, and `value="Block Light"` by block light again; `light_color` and `light_strength` set its blocks' light. `vox mcp` has the same calls.

## What it costs

Light is worked out on the terrain's threads as each column's faces are made, from the eight columns round it, as its faces are: in a world of metre blocks, about a quarter of the time making the column's faces takes. Placing or digging a block floods its light again round it, a few milliseconds, and draws again the chunks whose light changed, beside those its faces did. A chunk keeps a byte a block where its light varies, one where all its blocks have the same light, and nothing under open sky: about 50 KB a column of a generated world. Drawing by it costs under a tenth of a millisecond a frame.

Light is never saved and never sent over the network: every machine works it out from the blocks, so saves and what goes to players who join stay as small as the blocks' changes.
