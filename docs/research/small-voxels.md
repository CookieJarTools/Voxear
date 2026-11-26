# Small voxels: worlds built of voxels a few centimetres across

Research into making worlds whose voxels are a few centimetres across, rather than worlds of one-metre blocks with
some small-cube decoration. The look wanted is:

- grass blades and flowers made of cubes a few centimetres across;
- round-crowned trees and tall conifers whose leaves are clouds of small cubes;
- hillsides of small cubes in the distance;
- a whole town and forest built that way out to the horizon, that players can break, place and move a voxel at a
  time.

Written 2026-11-24, measured on the RTX 3070 at 1920×1080. The prototype is
[`crates/voxear_rhi/examples/small_voxels/`](../../crates/voxear_rhi/examples/small_voxels/main.rs). It holds
the world made on the CPU (`main.rs`) and on the GPU (`gpu_make.rs`, `make.slang`), and three ways of drawing
it: the walk and the coarse first pass (`trace.slang`), and bricks drawn as boxes (`raster.slang`).

## The short version

- **Yes, Voxear can make this look, but not by making blocks smaller.**
  - Every block world today is drawn as meshes of quads, and quads are what break.
  - The 256 m scene below, meshed at 1/16 m, has 455 million faces showing: 372 million quads even after joining
    runs of one colour, about 3 GB. Voxear's blocks already struggle at 0.4 m.
- **Tracing the voxels on the GPU works.** A compute shader walks each pixel's ray through a sparse grid of
  bricks, and the world keeps no meshes at all. The prototype draws a 256 m meadow and forest of 1/16 m voxels:
  - 3.0 ms at 1080p for the picture itself;
  - 4.6 ms with traced shadows and ambient occlusion;
  - 343 MB on the GPU.

  The frame time hardly depends on the world's size. The memory grows with its area and with the square of the
  voxels to a metre.
- **The fastest ways found, measured against each other on the same worlds:**
  - **Making the world on the GPU** is 11 to 16 times faster than on all 24 CPU cores. The 256 m world took 225 ms
    against 3.6 s, and both made the same world.
  - **Drawing:** a coarse pass first, one ray every 4×4 pixels against whole bricks, lets every ray start near what
    it will hit. It took 5–45% off the picture.
  - Drawing each brick as a box with triangles and walking only inside it was no faster here.
- **No blocks are needed.** The world is small voxels throughout, with no one-metre grid under it. Memory is kept
  down by:
  - a palette in each brick;
  - sharing bricks that repeat, such as many trees of one kind;
  - making the world again on the GPU as it comes into view, and keeping only what players changed.

  Drawing block worlds as patterns of small voxels stays an option, for the worlds of blocks that exist.
- **Distance is handled in rings.** Voxels are 1/16 m near the camera, then 1/8 and 1/4 m further out, then
  coarser still to the horizon.
- **The plan has seven stages.** Stage 1 alone gives small-voxel models and props in any scene. Stages 1 and 2
  give the meadow picture. Stages 1 to 5 give the town to the horizon, editable a voxel at a time.

## What the references show

- **The night meadow.**
  - Grass blades one voxel across and a few to a dozen tall, at about 1/16 m.
  - Red, purple and white flower heads of 3–5 voxels.
  - Thin pale trunks with round crowns.
  - Tall conifers, every leaf a small cube.
  - A hillside of small cubes behind.
- **The town and forest.**
  - Tiled roofs, stairs, bridges and windows, all of small voxels.
  - Broad trees whose crowns are clouds of cubes.
  - Hills of small voxels to a horizon a kilometre or more off.
  - Break, place, move and copy tools that work on single voxels and on whole objects. Its overlay shows 3.4 ms a
    frame, on hardware not stated.

## What Voxear has today

| Piece | Size of its cubes | Where it stops |
|---|---|---|
| Block Worlds | 1 m by default; 0.1 m to 10 m allowed | Drawn as meshed quads. At 0.4 m a Generated Block World is 758k triangles and 831 ms to load; Voxel Terrain at 0.4 m is 1.26M triangles; the endless world never settled. |
| Relief look (`BlockVoxels`, `blockCubes`) | up to 64 a block side, 4 deep | Shading only: the ray is walked across small cubes inside a face, but silhouettes stay the block's, and no depth is written. |
| Ground cover (`[[ground_cover]]`) | plants of 1/8 m cubes | Scattered on the GPU round the camera to 60–90 m, swaying: 1.7 ms for 3 tufts a block to 60 m. Decoration, not world. |
| Plants of cubes (box models) | 1/8 m, 1/16 for the smallest | About 0.2–0.3 ms per 1,000 on screen, with blocky LODs. |
| Sculpted box models | up to 256 a side, in 16³ chunks | Meshed like blocks. Props and characters, not ground. |
| Trees (`.voxplant`) | 1 m log and leaf blocks | Crowns are blocks of leaves with a picture. |

Below is CubeGame's Realm today, at the player's start, after 600 frames. Its GPU frame took 5.4 ms, the least of
the last 28. Of that, the blocks' depth, opaque and shadow passes took 1.8 ms, and the ground cover about 0.8 ms.

![CubeGame's Realm today](../images/small-voxels/today-realm.webp)

Where it falls short of the references, looked at close up:

- **The ground steps a metre at a time.** The edge of the meadow is a one-metre ledge of dirt, and the far shore
  and hills are one-metre steps.
- **Crowns are one-metre leaf blocks with a picture on each face**, not clouds of cubes. Enlarged twice below,
  against the prototype's crowns of 1/16 m voxels:

  ![Crowns, today against the prototype](../images/small-voxels/crowns-today-vs-prototype.webp)
- **The grass and flowers are close to the reference already**, but they are decoration. They end at their draw
  distance, and nothing under them is small.
- **Nothing smaller than a block can be dug or built.**

Why: every piece of a block world becomes faces of quads on the CPU and is drawn as triangles. That is why 0.4 m
blocks were already slow. At 1/16 m the faces are what break, as the next section measures.

## The options

### A. Smaller blocks everywhere, meshed as now

The prototype's world was counted as it would be meshed:

| World | Faces that show | Quads, runs of one colour joined | Quads at 8 bytes |
|---|---|---|---|
| 256 m at 1/16 m | 455M | 372M | 2,977 MB |
| 256 m at 1/8 m | 95M | 79M | 630 MB |
| 1,024 m at 1/8 m | 1,693M | 1,403M | 11,227 MB |

A grass blade or a leaf voxel stands alone, so joining saves only a fifth. Today's endless block world has 152k
triangles in view. **Not possible.**

### B. Meshes near, something else further out

Within 32 m of the camera at 1/16 m that is still about 18M quads (36M triangles), on the measured 5,700 quads a
square metre. That is tens of milliseconds of triangles, and the far tiers would need tracing anyway. **No.**

### C. Tracing the voxels on the GPU

One compute thread a pixel walks its ray through the grid. The prototype measured this, below. Data structures
weighed:

| Structure | For | Against |
|---|---|---|
| Dense grid | Simplest walk | 4,096 voxels to a cubic metre: impossible past a room |
| Two levels: a grid of bricks | Simple, fast to edit and stream | Big empty spaces still cost a step a brick |
| **Three levels: sectors of 8³ bricks of 8³ voxels (the prototype)** | Steps over a 4 m sector, a 0.5 m brick or one voxel; LODs come free from the masks; a brick is edited and uploaded alone | Rays skimming grass take hundreds of steps |
| Sparse octree | Adapts to anything | Twice the levels, slower walk, harder to edit |
| Shared-node graph (one copy of each repeated subtree) | Very small for repeating worlds | Edits rebuild shared nodes; poor for a world that changes |
| Hardware ray tracing with a box per brick | Uses the RT cores | Intersection shaders are slow; GPUs without ray tracing (which Voxear supports) need the compute walk anyway |

**Chosen: the three levels, walked in compute,** with one more level above the sectors for long rays through
empty sky.

### D. Blocks that hold patterns of small voxels (an option for block worlds)

This is for worlds of blocks that already exist, not for a world of small voxels. The one-metre block world stays
as it is: its chunks, palettes, edits and region files. Each kind of block gets a pattern of 16³ voxels, for
example:

- a grass block is dirt with blades on top;
- a leaf block is a cloud of leaves that thins towards the air beside it;
- stone has cracks.

The tracer walks chunk, then block, then the block's pattern. A world drawn smooth already keeps how full each
block is (`Fill`), and that cuts the pattern into a slope, so hills step 6 cm at a time instead of a metre. A
hash of the place picks a variant and turning of the pattern, so the repeats don't show.

- **Memory:** a block world costs what it does now, plus one pattern per kind of block (4 KB each).
- **Where it falls short:** shapes finer than a block need bricks of their own. That covers a tree's crown, a
  flower or a dug hole. Option C keeps those.

**Chosen: C, a world of small voxels throughout.** D is a later option for today's block worlds.

## The prototype

`cargo run --release -p voxear_rhi --example small_voxels -- 256 16 out`

That makes a world 256 m across at 16 voxels a metre and writes the pictures to `out`. Each view is drawn by every
method in turn. `SMALL_VOXELS_FACES=1` also counts the faces meshing would need, and `SMALL_VOXELS_GROUND_ONLY=1`
leaves out the trees and flowers.

- **The scene.** A meadow of one-voxel grass blades and flowers before a forest of round-crowned trees and
  conifers, with hills behind and to the sides.
- **Generation.** It is made on the CPU on every core and uploaded once, then made again on the GPU and checked
  against the CPU's.
- **The trace.** One compute pass at 1920×1080 walks the rays and casts a second ray for moonlight shadows.
  Ambient occlusion comes from the voxels round each face's corners.
- **Storage.**
  - A sector: 512 bits for its bricks, their counts, and where its bricks start (100 bytes).
  - A brick: 512 bits for its voxels, 64 bits for its 2×2×2 groups, and its average colour (80 bytes).
  - Materials: one byte per solid voxel.
  - The inside of a hill is left out. The ground is filled down only to its lowest neighbour, so no ray can get
    under it. A crown is a shell of leaves 9 voxels deep.
- **Levels of detail.**
  - Where a voxel is under half a pixel, the walk stops at the 2×2×2 groups.
  - Under an eighth of a pixel, it stops at whole bricks in their average colour.
  - A ray that has taken many steps goes coarser the same way, so a ray skimming the grass never runs out of steps.

![The prototype's meadow](../images/small-voxels/prototype-meadow.webp)

![The prototype from above](../images/small-voxels/prototype-over.webp)

![Close up](../images/small-voxels/prototype-close.webp)

The colours and night light are rough: lighting is not what was measured. In Voxear the voxels would take the
engine's own sun, sky, fog, GI and post-processing, as any surface does.

### Measured

Each time is the least of 60 frames after 20 to warm up. "Meadow" is the picture above. "Over" looks down from
30 m. The GPU was shared with other sessions, which used about a fifth of it at the start.

| World | Voxels | Bricks | Solid voxels | GPU memory | Made in (24 threads) | Made a ms | Upload | Meadow | Meadow, picture only | Meadow, no LOD | Over | Over, no LOD |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 256 m | 1/16 m | 1.69M | 198M | 343 MB | 3.8 s | 17 m² | 82 ms | 4.55 ms | 3.02 ms | 5.15 ms | 4.39 ms | 5.90 ms |
| 256 m, ground and grass only | 1/16 m | 0.53M | 58M | 110 MB | 0.85 s | 77 m² | – | 3.52 ms | 2.80 ms | – | – | – |
| 512 m | 1/16 m | 7.18M | 833M | 1,447 MB | 18.5 s | 14 m² | 953 ms | 5.87 ms | 3.83 ms | 6.44 ms | 5.12 ms | 6.92 ms |
| 128 m | 1/32 m | 1.76M | 224M | 384 MB | 5.4 s | 3 m² | 275 ms | 4.69 ms | 3.65 ms | 5.43 ms | 3.81 ms | 5.42 ms |
| 256 m | 1/8 m | 0.37M | 43M | 73 MB | 0.70 s | 94 m² | 22 ms | 3.65 ms | 2.19 ms | 3.86 ms | 4.02 ms | 4.73 ms |
| 1,024 m | 1/8 m | 6.42M | 748M | 1,281 MB | 14.5 s | 72 m² | 1,154 ms | 4.07 ms | 2.75 ms | 4.37 ms | 4.84 ms | 5.80 ms |

What the numbers say:

- **The picture costs about 3 ms at 1080p.** Traced shadows and occlusion add about 1.5 ms. At the 67% render scale
  TAA upscales from, it would be about 2 ms and 3 ms.
- **Memory is about 1.7 bytes a solid voxel.** That is 1.3 KB a square metre for meadow and forest at 1/16 m, and
  each halving of the voxel costs 4.7 times the memory.
  - Trees and flowers are two thirds of it: 233 of 343 MB.
  - Materials are 58%. A palette of 1 to 4 bits a voxel in each brick would bring 343 MB to about 180 MB.
- **The levels of detail save 0.6–1.5 ms.** Their real use is keeping far crowns from shimmering. Without them, a
  hashed crown shows rings of moiré.
- **The world's size barely moves the frame.** 256 m to 512 m at 1/16 m added 1.3 ms, because more far hill shows.
- **Rays skimming the grass are the cost.** The picture below counts each pixel's steps: the meadow's far edge and
  the trees' outlines take hundreds.

  ![Steps a pixel](../images/small-voxels/prototype-steps.webp)
- **Making the world on the CPU: 17 m² a ms at 1/16 m on 24 cores**, with a simple generator. The GPU is 11–16
  times faster: see "Faster ways, measured".
  - Walking at 5 m/s with 256 m in view brings 1,280 m² a second into view: 75 ms of all 24 cores.
  - Running at 10 m/s doubles that.
  - Flying at 40 m/s needs the coarser rings, which are made 4 to 20 times faster.
  - Upload ran at about 4 GB/s.
- **1/8 m against 1/16 m:** a fifth of the memory and two thirds of the time, but blades and flowers turn to
  blocks. The reference is nearer 1/16.

  ![1/16 m and 1/8 m](../images/small-voxels/sixteenth-vs-eighth.webp)

### Not measured

- Composing with the rest of the frame (depth, TAA, water, meshes), and the GI.
- A view with many unique buildings, as in the town picture.
- Instanced models in the bricks.
- Streaming while moving.

## Faster ways, measured

Is the walk the fastest way to draw these worlds, and the CPU the fastest way to make them? Each candidate was
built into the prototype and measured on the same worlds and views. Each round of 80 drew every method in turn, so
other work on the GPU weighed on them alike: other sessions kept it 25–40% busy. Each figure is the least of the
last 60 rounds.

### Making the world on the GPU

The generator was written again as four compute passes:

1. Each column's height, grass blade and top.
2. Each column of bricks lists the bricks that ground, grass or a plant's bounds reach.
3. One group of 512 threads a listed brick, a thread a voxel. It keeps the brick if anything is in it, and writes
   its materials in the order of its bits.
4. One group a sector puts its bricks in order for the walk.

Plants are placed on the CPU, which takes milliseconds.

| World | Voxels | CPU, 24 threads | GPU | Faster | GPU a ms | Placing plants (CPU) | Same world? |
|---|---|---|---|---|---|---|---|
| 256 m | 1/16 m | 3,559 ms | 225 ms | 16× | 291 m² | 33 ms | bricks 0.01% fewer, pictures within 0.01% of pixels |
| 512 m | 1/16 m | 16,363 ms | 1,135 ms | 14× | 231 m² | 142 ms | bricks 0.01% fewer, 0.02% of pixels |
| 128 m | 1/32 m | 4,591 ms | 285 ms | 16× | 57 m² | 11 ms | the same bricks, no pixel more than 24 levels off |
| 256 m | 1/8 m | 661 ms | 54 ms | 12× | 1,214 m² | 34 ms | bricks 0.06% fewer, 0.03% of pixels |
| 1,024 m | 1/8 m | 11,631 ms | 1,089 ms | 11× | 963 m² | 524 ms | bricks 0.04% fewer, 0.03% of pixels |

The few differences come from floating point rounding on the two sides.

- **The brick pass is 95% of the GPU's time**, and it is not tuned. It works through 3.5 to 4.6 times as many
  bricks as it keeps, because a tree's whole bounding box is listed. Testing a brick against the crowns' shells
  before filling it would cut that by about half. Placing plants on the GPU as well would remove the CPU's part.
- **Streaming is cheap with it.** At 1/16 m, walking at 5 m/s with 256 m in view brings 1,280 m² a second into
  view: about 4.4 ms of GPU a second, under a tenth of a millisecond a frame. On the CPU it was 75 ms of all 24
  cores.
- **The CPU still needs what is near**, for collision and for the edits it checks. Those bricks are read back or
  made on the CPU only round the players.

### Three ways of drawing

- **The walk:** as described above.
- **Coarse first:** one ray every 4×4 pixels walks against whole bricks, as if each brick were solid. Each pixel's
  ray then starts a brick and a half short of the nearest distance its four corner rays reached. Where all four
  reached the sky, it draws sky without walking.
- **Bricks as boxes:** a compute pass culls the bricks to the view and sorts them into 32 bands of distance, nearest
  first, so the depth test can throw away bricks behind. Each brick's camera-facing sides are drawn as triangles.
  Each pixel walks only inside its own brick, at most two dozen steps, and writes which voxel it saw. One compute
  pass then shades each pixel once.

"Picture" is the picture alone, and "lit" adds traced shadows and occlusion. "Meadow" is from 1.1 m above the grass,
and "over" from 30 m up.

| World | Voxels | Meadow picture: walk / coarse first / boxes | Meadow lit | Over picture | Over lit |
|---|---|---|---|---|---|
| 256 m | 1/16 m | 3.05 / **2.72** / 3.70 ms | 4.63 / **4.26** / 5.54 ms | 3.08 / **2.04** / 4.40 ms | 4.45 / **3.48** / 7.91 ms |
| 128 m | 1/32 m | 4.14 / **2.35** / 4.47 ms | 4.71 / **3.35** / 5.67 ms | 3.67 / **2.00** / 4.91 ms | 5.49 / **3.95** / 7.78 ms |
| 256 m | 1/8 m | 2.21 / 2.07 / **2.05** ms | 3.66 / **3.52** / 3.54 ms | 2.72 / **2.11** / 3.43 ms | 4.04 / **3.47** / 7.57 ms |
| 1,024 m | 1/8 m | 3.21 / **2.80** / 5.54 ms | 5.59 / **5.00** / 8.38 ms | 3.88 / **3.09** / 6.91 ms | 7.69 / **5.88** / 9.90 ms |

- **Coarse first is the fastest, 5–45% under the walk.** It gains most where rays cross empty air before they hit:
  from above, and at 1/32 m.
  - Its pictures match the walk but for 0.1–5% of pixels, nearly all far ones that stop at a different level of
    detail.
  - It is not exact. Something thinner than the gap between two coarse rays, nearer than both of their hits, could
    be stepped past. A version for the engine walks the coarse rays as cones, so nothing between them is missed.
- **Bricks as boxes was no faster.**
  - Every brick in view, about a million at 256 m, is culled, sorted and drawn, and each pixel is covered by
    several bricks before the nearest wins.
  - Shading inside the brick pass, before the depth test had settled, was far slower: 26 ms with shadows. That is
    why it writes what it saw and shades afterwards.
  - It also misses the brick the camera stands in, and needs care where bricks cross the near plane.
  - It would gain with last frame's depth to cull hidden bricks, but so would the walk, by starting each ray near
    where last frame's ray stopped.
- **Not tried here:**
  - Starting each ray from last frame's distance, moved to this frame's camera: likely the next win after coarse
    first.
  - The GPU's ray tracing hardware with a box per brick.
  - Drawing at a lower resolution for TAA to fill in, which scales with the pixels as any pass does.

## How it fits what exists

- **The renderer.**
  - **The voxel pass.** A voxel pass in the render graph writes the frame's depth and the same surface data the
    meshes write, so lights, fog, TAA, water, glass, particles and post all work over it. Meshes such as
    characters and props draw against its depth as now.
  - **Sun shadows.** Voxels get traced shadows, as in the prototype. Lighting a mesh pixel also traces the sun ray
    through the voxels, and lighting a voxel samples the shadow maps for meshes.
  - **GI.** It already has a software path (`trace_software.slang`). Its rays walk the bricks instead of triangles.
- **A Voxel World of its own.** No blocks under it.
  - It is a component like Block World: its bricks, the rings that stream round the camera, the generator that
    makes them on the GPU, and the changes players made.
  - Today's Block Worlds are left as they are. Drawing them as patterns (option D) is a later option.
- **Generator graphs.**
  - The node graph that makes block worlds now is compiled to a compute shader. Heights, biomes, caves and
    stamps are worked out per voxel, as `make.slang` does by hand.
  - Plant and structure stamps are placed as now and filled on the GPU.
  - A graph that can't run on the GPU runs on the CPU, at a sixteenth of the speed.
- **Streamed worlds and the far view.**
  - Rings round the camera: 1/16 m to about 128 m, then 1/8 m to about 256 m, then 1/4 m to 1 km, then 1/2 m and
    1 m to the horizon. Each ring is made on the GPU as it comes into view, from the generator and the changes.
  - The baked far view's idea carries over: far rings made once and kept on disk, coarsest first.
  - Estimated worst case with every ring unique and no palettes: about 760 MB. With palettes and shared bricks,
    a few hundred.
- **Region files and saves.** A changed brick is kept with its mask and materials, zstd'd. Only bricks a player
  or the editor changed are written, as `.blocks` keeps only changed blocks now. Everything else is made again
  from the generator.
- **Network edits.** Edits go out as commands that every machine plays the same way: carve a ball, set a voxel,
  stamp a model, each with its material. A joining player gets the changed bricks in compacted form, as block
  changes are sent now.
- **Physics.**
  - The bricks round each body, at most a few metres, are kept on the CPU too. They are meshed into collision at
    1/8 m, or given a collision shape that reads the brick masks.
  - Grass, flowers and leaves have a material that doesn't collide.
- **Navigation.** The brick masks give a walkable grid at 1/2 m, and the 2×2×2 groups one at 1/8 m where it
  matters.
- **Light from torches and the sky.** A light grid at 1/2 m, one value a brick, spreads light as block light
  does now and is sampled smoothly at each voxel. It is kept on the GPU beside the bricks.
- **C#.**
  - A new `Voxels` class adds `Carve(middle, radius)`, `Set(position, material)`, `Stamp(model, at, turn)` and
    `Raycast`, the last returning a voxel hit with its material.
  - Materials and models are typed asset references with pickers, never names in strings.
  - Everything is reachable from `vox editor call` and `vox mcp`.
- **Swaying plants.** Traced voxels do not move with a vertex shader.
  - Near the camera, swaying grass and flowers stay today's ground cover meshes, which sway, push aside and
    spring back. Past them, the voxels stand still.
  - Bending the ray by the wind inside plant bricks is a research item. It may tear at the edges.

## The plan

Costs are GPU time at 1080p on the RTX 3070, memory on the GPU, and the work as a guess in sessions like this
one.

| Stage | What it gives | GPU | Memory | Work |
|---|---|---|---|---|
| 1. **Voxel pass and Voxel Volumes** | A sparse brick volume as an Obj, drawn with the coarse first pass into the frame's depth and surfaces, with traced sun shadows; box models stamped into it; seen in the editor and in renders | ~2 ms at 67% scale, +1 ms shadows | 1.3 KB/m² of detail | 3–4 |
| 2. **Voxel World made on the GPU** | A world of small voxels throughout, made by a generator graph compiled to compute shaders, round the camera, in one ring at first; ground, grass and hills | <0.1 ms a frame while walking | 110 MB for 256 m of ground and grass | 3–4 |
| 3. **Plants and trees of small voxels** | Ground cover, grown plants and crowns as voxel stamps filled on the GPU; then models shared between bricks so a forest costs one tree per kind | +0.5 ms | 230 MB a 256 m forest unshared; tens of MB shared | 3 |
| 4. **Editing a voxel at a time** | `Voxels` in C#, an editor brush, changed bricks in region files, edit commands over the network, picking | – | changed bricks only | 2–3 |
| 5. **Distance in rings** (built, below) | 1/16, 1/8, 1/4 m rings and coarser to the horizon, made as they come into view, far rings kept on disk | +0.5 ms (measured: +8 ms for land to the horizon in view) | 400–760 MB (measured: 500–620 MB) | 3 |
| 6. **Lighting at voxel scale** | GI rays in voxels, occlusion and glowing voxels, light from torches and the sky in a grid; begun 2026-11-26: shadows on everything, light indoors, glass | 1–2 ms | – | 2 |
| 7. **Physics, navigation and debris at voxel scale** | Collision from the bricks round each body, broken pieces as falling voxel objects, voxel characters | – | – | 3–4 |

Block Worlds drawn as patterns of small voxels (option D) can follow any time after stage 1, for the worlds of
blocks that exist.

- **Stages 1 to 3 give the night meadow.** Ground, grass and hills come from the generator, and flowers and
  crowns from stamps.
- **Stages 1 to 5 give the town to the horizon,** editable a voxel at a time. Stages 6 and 7 bring the lighting and
  physics up to the same scale.
- **Stage 1 stands alone.** Voxel props, ruins and statues in any scene, today's block worlds included, are worth
  having without the rest.
- **Next measurement:** starting rays from last frame's distance, and coarse rays walked as cones,
  so the first pass is exact.
- **Risks.**
  - Fitting the traced depth and surfaces into TAA and the existing passes without seams where meshes meet
    voxels.
  - Shimmer in far crowns, which needs the levels of detail plus TAA.
  - The cost of rays that skim the grass, which the coarse first pass does not remove.

## Stage 1, built (2026-11-24)

A **Voxel Volume** is a component: a box of small voxels on an Obj, its least corner at the Obj's place, filled from
its own ground (rolling ground, grass blades, flowers) and from box models stamped into it. It is made on a thread
of its own whenever it changes and kept on the GPU as the prototype's sectors and bricks
(`crates/voxear_render/src/voxel_volume.rs`, `shaders/voxels.slang`).

- **Drawn as the frame's other surfaces are.**
  - **Voxel coarse rays**: one ray every 4×4 pixels walked against whole bricks.
  - **Depth (voxels)**: a triangle over the screen whose pixels walk their rays from where the coarse rays allow.
    It writes depth, the normal and roughness that ambient occlusion and reflections read, and keeps what each
    pixel met.
  - **Opaque (voxels)**: lights what the depth pass kept, without walking it again. Lighting comes from the sun with
    the shadow maps of everything else, contact shadows, lamps, the GI's ambient light, ambient occlusion,
    reflections, block light, the atmosphere's haze and clouds. The voxels' own shadow is traced through them
    towards the sun, and depth is written so meshes and blocks meet it.
- **Kept with scenes as `[obj.voxel_volume]`**, its stamps as box model references. Prefabs carry it, and stamped
  models are read once and kept. In the editor it is under **Add Component ▸ Rendering** and **+ ▸ Voxel Volume**,
  with Inspector fields for its size, voxels a metre, roughness and ground, all reachable by `vox editor call`.
  `vox render` waits for volumes to be made, as it waits for streamed ground.
- **Measured**: a meadow 80 × 120 m at 16 voxels a metre with sixty stamped trees, seen from 2 m over the grass at
  1920×1080, least of the last frames: coarse rays 0.08 ms, depth 2.0 ms, opaque 3.8 ms, whole frame 7.3 ms. It
  was made in 1.6 s. Keeping each pixel's hit took the opaque pass from 5.8 to 3.8 ms; most of what is left is
  lighting, the traced shadow included.

![A Voxel Volume in the engine](../images/small-voxels/engine-meadow.webp)

![Close up](../images/small-voxels/engine-meadow-near.webp)

Not done in stage 1, as planned:

- The voxels cast no shadow onto meshes, blocks or terrain.
- A volume is not turned or scaled with its Obj, and is drawn whole however far.
- Stamps are added in the scene file; the Inspector shows how many there are.
- No C# component yet: scripts get voxels with stage 4's editing.

## Stage 2, built (2026-11-24): endless ground made on the GPU

A Voxel Volume with **Endless** on has ground without end. It is made on the GPU round the camera as far as its
**Reach** (`crates/voxear_render/src/voxel_volume/endless.rs`, `shaders/voxel_world.slang`).

- **Its world is a window of sector columns round the camera.** Each column is 4 m a side at 16 voxels a metre and
  as tall as the volume. It is kept in a slot of the window by where it is in the world, wrapping. Each slot has room
  of its own for 384 bricks and 61 KB of materials, so a column made over one that left needs nothing freed.
- **Making a column takes the generator's four passes:** heights and grass, the bricks that could hold anything,
  the bricks' voxels, then each sector's bricks put in order. Up to 16 columns are made a frame, the nearest first,
  in a pass before the views. `vox render` makes the whole window before its frames.
- **The walk reads a slot's entry,** which names the sector it holds. A slot not yet made for the sector the ray is
  in reads as empty, never as stale ground. Slots are found from whole numbers made positive first, so the GPU
  wraps them as the CPU does; a remainder of a negative number did not.
- **The ground is a Voxel Volume's own ground, to the voxel.** A test draws a 16 m volume made on the CPU and an
  endless one made on the GPU from the same camera: they differ in under 0.5% of the pixels compared.
- **Measured** (reach 64 m, 16 voxels a metre, 16 m tall, 1920×1080, the GPU shared at about 27%):
  - about 100 MB on the GPU;
  - the window made in 0.8 s before the frames;
  - flying at 10 m/s, columns made on 69 of 540 frames, 0.8 ms each (median; 0.07 to 2.3 ms);
  - a view full of meadow from 2 m over the grass takes 2.8–3.2 ms for depth and 5.4–5.7 ms to light.

![Endless ground after flying 100 m](../images/small-voxels/endless-fly.webp)

Not done in stage 2:

- Generator graphs were not compiled to the GPU yet: done since, below.
- There are no stamps, trees or structures in endless ground (stage 3), and no coarser rings past the reach
  (stage 5).
- Columns are not kept on the CPU for collision (stage 7).

## Stage 2, generator graphs on the GPU (2026-11-25)

An endless Voxel Volume can be pointed at a project's world generator (**Generator**, a typed pick of
`.worldgen` assets, `generator` in the scene). Its ground is then that generator's land, its graph run on the GPU a
voxel column at a time.

- **How a graph runs there.** A graph is already compiled into steps, each a kernel filling one row of numbers from
  others (`generate/compile.rs`). Each kernel now also says how it runs on the GPU (`Kernel::gpu`): one of twenty
  operations and its numbers, seeds and curves worked out on the CPU. The height's, climate's and rock's programs
  are joined into one list of words, the climate's reading the height made first (`generate/gpu.rs`), and a
  generic shader runs it (`shaders/generator.slang`). Interpreted rather than compiled to a shader of its own, a
  changed graph needs no pipeline built, and the cost per column is the steps' own.
- **Slots.** Each step's row is kept in a slot, used again once nothing after reads it. The starters' 65–99 steps
  run in 12–16 slots of 48.
- **What runs per column, and what per voxel.** Everything a graph's height, climate and rock read runs per voxel
  column: Noise, Math, Curve, Warp (its moved places), Gullies (its four places a step off), Clamp, Blend, Terrace,
  Height Mask, Cells, Falloff, Landmass (warp, fields, ridges, curves, its own gullies and bumps) and Climate. That
  covers the Battlefield Plains, Rolling Hills, Alpine, Archipelago and Canyons starters, and the landmass a plain
  generator is. Per voxel: a generator's Volume, since (below). What is left:
  - A **Rivers** node finds its rivers over a grid of points round each place, with their drainage, on the CPU; on
    the GPU it passes its ground through. Kept on the CPU, rivers could be laid into the columns as a height to cut
    to, as streamed worlds lay them.
  - **Plateaus, Peaks, Raised Land, Spires, Fins, Chasms, Slope Mask, Roof** and **painted maps** are not ported
    yet. A graph with one makes the volume's own ground, and a warning names the node.
- **Blocks into voxels.** Each column's ground is what a world of blocks with no biomes lays, from the generator's
  height, climate and rock, in a pass of its own after the heights (`voxelWorldTops`): grass and its dirt where the
  ground grows grass, with the volume's grass and flowers on it; rock where the ground is steeper than 45° (measured
  four voxels across) or the Rock mask says; sand within a metre of the sea and where it is hot and dry; gravel under
  the sea; snow below freezing. Each takes its shades in a pattern of two voxels, as a block's face is patterned.
  Heights are kept with a border of four voxels round each column, so its slope and the flowers standing on the
  next column are found without asking the graph again.
- **The same ground as the CPU's.** A test runs six generators at 8,192 places on the GPU and the CPU
  (`a_generators_ground_and_climate_on_the_gpu_are_the_cpus`):

  | Generator | Heights | Same to the bit | Within 1 mm (or 1/100,000) | Furthest apart |
  |---|---|---|---|---|
  | Rolling Hills | 3–111 m | 385 | all | 0.2 mm |
  | Battlefield Plains | 2–35 m | 448 | all | 0.07 mm |
  | Alpine | −170–849 m | 185 | 99.5% | 3.4 mm |
  | Archipelago | −45–120 m | 672 | all | 0.6 mm |
  | Canyons | 26–220 m | 5,456 | 98.7% | 11 mm |
  | Alpine landmass | −233–998 m | 185 | 97.8% | 0.60 m |

  The GPU rounds sums of products its own way, so few heights are the same to the bit. Canyons' terraces stretch a
  rounding into a centimetre on their risers. A plain landmass's own gullies are not joined, so each octave turns
  where the one before has a floor and its ground steps there: a rounding either side of a floor lands on the other
  side of a step. Temperature and humidity agree to 0.01 °C and 0.001.
- **A bug found on the way.** A ray starting exactly on the corner of two sectors, going back along both, went back
  and forth between them without moving on, and a quarter of the view was empty: a camera at 8 m over a window
  whose corner is at −40 m is on such a corner. A ray going back along an axis now takes the cell below a boundary
  it lies on (`voxelTrace`).

- **Measured** (reach 64 m, 16 voxels a metre, 1920×1080, the GPU shared at about 28% by another session), against
  the same volume with its own ground:

  | | Its own ground | Rolling Hills |
  |---|---|---|
  | Window made before the frames | 0.48 s | 0.52 s |
  | On the GPU | 102 MB | 103 MB |
  | Flying at 10 m/s: frames making columns | 66 of 540 | 66 of 540 |
  | Making, median (least–most) | 0.72 ms (0.07–1.29) | 1.33 ms (0.22–2.88) |
  | A column, at 16 a frame | 0.045 ms | 0.083 ms |
  | Still view, least of 60: frame | 5.8 ms | 9.2 ms |
  | Depth / opaque (voxels) | 2.3 / 2.6 ms | 3.1 / 4.8 ms |

  Running the graph about doubles a column's making: 78 steps of noise, warp and gullies for each of its 5,184
  voxel columns. The still views differ in what they show, a hillside in the sun against level meadow, more than
  in how the ground was made: the drawing passes read the same bricks either way.

![An endless volume made by the Rolling Hills generator](../images/small-voxels/generated-hills.webp)

![Rock on an Alpine mountainside](../images/small-voxels/generated-mountainside.webp)

## Stage 2, overhangs and caves on the GPU (2026-11-25)

A generator's **Volume**, the ground in three dimensions, now runs on the GPU too, a voxel at a time.

- **Two programs, as on the CPU.** A Volume is compiled into a columns' program, run once a column with the
  height, and a cells' program, run once a cell with its middle, its size and what its column carries up
  (`generate/compile.rs`). On the GPU the columns' program joins the height's, so the columns pass keeps what each
  voxel column carries up beside its height. The cells' program is a word list of its own, run in the bricks pass.
  Three givens are new (a cell's height, its size, a carried number) and seven operations: Solid, Combine (union,
  subtract, smooth), Noise 3D, Overhangs (its steepness mask and its push), Tunnels and Chambers. A Volume made of
  the height alone, with nothing taken out, is not asked at all.
- **What runs and what not.** A world of blocks' own overhangs and caves (`Graph::add_voxels`) is Overhangs,
  Tunnels and Chambers taken out of the ground, and a **Cave Network** of great halls. The network lays its rooms
  and tunnels out over regions on the CPU, so on the GPU it runs in part: it hollows nothing, and its rooms' spans
  are empty ("cave halls" left out). Raised Land, Spires, Fins, Arches and Roof are not ported: a Volume with one is
  left out, and the ground is the height made solid. Of the starters, none has a Volume of only these nodes yet;
  Caves and Cave Level are a Cave Network alone.
- **Bricks of a Volume.** The candidates pass reaches as far over and under each column's ground as the Volume can
  be solid or hollow (its reach and depth, and each column's spans), to the volume's floor where it says without
  end, as noise caves do. Each brick works out which of its voxels and a border of one round them are solid, a
  thousand looks in shared memory, and keeps only solid voxels with open air beside them; a brick of rock or of air
  keeps nothing. It first looks at 64 voxels three apart, and where every one is more than 0.3 m into the ground,
  or out of it, takes the whole as rock or air without the thousand: the Volume's numbers read about as how far a
  place is from the surface, and a cave is wider than the gap. That changed 46 pixels of 230,400 in a test view and
  took 28% off making. The volume's bottom voxels are always solid, so a cave that goes deeper has a floor there.
- **What the voxels are.** The tops pass finds each column's surface: its top solid voxel with open air over it,
  within 4 m of its ground's height. Grass, flowers and the dirt under grass go by it, so they follow ground the
  overhangs pushed out or in. A column whose top is taken away or covered over grows nothing. A voxel open to the
  sky near the surface takes the column's ground (grass, sand, snow, rock); the sides of overhangs and the mouths of
  caves take the layer under it; deeper, cave walls and ground standing apart are rock.
- **The same as the CPU's.** Alpine with a world's own overhangs (4 m), tunnels and chambers (0.8, open to the sky),
  its network taken out, asked at 27,648 cells from 8 m under the ground to 5 m over it
  (`a_generators_overhangs_and_caves_on_the_gpu_are_the_cpus`): 27,647 on the same side of the ground, and all
  9,748 within 2 m of the surface within a centimetre of the CPU's number. 1,594 of the cells are hollowed under the
  ground.
- **Measured** (reach 64 m, 48 m tall, 16 voxels a metre, 1920×1080, the GPU shared at about 35%):

  | | Rolling Hills | Alpine with overhangs and caves |
  |---|---|---|
  | Window made before the frames | 0.54 s | 5.5 s |
  | On the GPU | 103 MB | 204 MB |
  | Columns a frame at most | 16 | 2 |
  | Flying at 10 m/s: frames making columns | 66 of 540 | 367 of 540 |
  | Making, median (least–most) | 1.49 ms (0.24–2.73) | 3.30 ms (1.59–5.57) |
  | A column | 0.09 ms | 1.65 ms |

  A Volume costs about eighteen times what the height alone does to make, and two columns a frame barely keep up
  with flying at 10 m/s. Each column asks about every brick from the floor to its reach, about 3,700, most of them
  rock or air, each a group of 512 threads. Room for bricks is 768 a column, twice the height's, hence the memory.
  Next: a coarser pass over each column of bricks first, so bricks of rock and air are never asked about; and
  solidity worked out once a brick and shared with the bricks beside it, rather than a border each.

![Overhangs and open caves on an Alpine slope](../images/small-voxels/generated-overhangs.webp)

![Inside a cave, looking out](../images/small-voxels/generated-inside-a-cave.webp)

## Stage 2, caves made faster (2026-11-25)

Making a column of Alpine with overhangs and caves now costs a third of what it did, and the window loads in half
the time.

- **A coarse pass before the bricks.** Between the candidates and the bricks, one thread a candidate brick asks
  the Volume at the eight corners of the brick and its border. Where all eight are more than 24 voxels (1.5 m at
  16 a metre) into the ground, or out of it, the brick is dropped before a group of 512 is spent on it; the rest
  go on in a list of their own. With caves, 1,900 candidates a column come down to about 270. Bricks where the
  column's grass and flowers may grow are always kept, as those are not the Volume's.
- **How far the margin must be.** A voxel of the brick and its border is at most 7 voxels from a corner, so a
  Volume whose numbers were true distances would need a margin of 7. Asked with the CPU at 600,000 places within
  3 m of the surface, the numbers change by 1.0 m a metre at the median, 3.2 at the 99th percentile and up to 48
  where a chamber or tunnel changes cell, a jump of about 3 m between voxels. The margin of 24 covers three times
  a distance; at 8, 24 and 48 voxels the test views were the same to the pixel.
- **A faster surface search.** The tops pass went down through the air two voxels at a time, up to 64 looks a
  column. It now steps by half as far as the Volume says the ground is, at least two voxels, and halves the last
  step: 1.0 ms of the 1.6 ms a two-column frame took came down to 0.36 ms, with the same pictures.
- **Room that fits.** Counted on the GPU over 1,087 columns of the measured flight, a cave column keeps 36 bricks
  at the median, 548 at the 99th percentile and up to 775: over the room of 768 there was, so the fullest columns
  lost bricks, out of sight while the bricks were placed in row order. Their materials take at most 32 KB, 41 bytes
  a brick, of the 123 KB set aside. A column with a Volume now has room for 1,024 bricks and 64 material bytes a
  brick: a third more bricks in less memory.
- **Four columns a frame.** With the bricks cheap, the columns pass (the graph's height, climate and what the
  Volume carries up, 72 by 72 a column) is the most of what is left, and two columns leave the GPU half idle.

  | Alpine with overhangs and caves | Before | Now |
  |---|---|---|
  | Window made before the frames | 5.6 s | 2.6 s |
  | On the GPU | 204 MB | 164 MB |
  | Columns a frame at most | 2 | 4 |
  | Flying at 10 m/s: frames making columns | 367 of 540 | 198 of 540 |
  | Making, median (least–most) | 3.37 ms (1.62–6.68) | 1.99 ms (1.38–4.59) |
  | A column | 1.68 ms | 0.50 ms |

  Rolling Hills, with no Volume, is as it was: 0.095 ms a column. What a column costs now is mostly its columns
  pass, the graph interpreted a step at a time; compiling each graph into a shader of its own is next for that.
- **Fields changed in the Inspector.** An endless volume was made from scratch, buffers and all, whenever a field
  that changes its ground changed: each frame of a drag on its generator's floor or a ground field, up to 200 MB a
  frame. Where its window, height and room stay the same it now keeps its buffers, makes its generator's program
  again and makes each column over, showing what it held until then. Its roughness changes how it is drawn alone.

## Stage 3, trees in endless ground (2026-11-25)

An endless Voxel Volume grows trees of a project's `.voxplant` plants, each kind as many to a hectare as it says.

- **Grown on the CPU, placed and filled on the GPU.** Each kind is grown from four seeds, and each plant becomes a
  grid of its blocks, a byte a block: a log (which way it lies and how thick, in eighths of a block), leaves, or
  nothing (`voxel_volume/trees.rs`). An oak's grid is about 12 × 14 × 12 bytes. A trees pass places at most one
  tree in each 5 m cell round the column being made, by hash, turned four ways, where the ground is grass by the
  tops pass's rules, and the Volume leaves the ground where it was. Its height there is the
  generator's, run afresh, as a tree's foot may be outside the column. The candidates pass lists the bricks a tree's
  box reaches; the coarse pass drops those none of its blocks are in; the bricks pass fills the rest.
- **Leaves as clouds of cubes.** Filling each leaf block's skin left crowns that read as boxes. The leaves of the
  eight blocks round each voxel are blended instead, and a shell two voxels deep is kept where the blend crosses a
  half, moved a little by hash at each voxel, 60% filled. A log beside leaves counts as leaves, so a branch inside a
  crown leaves no hollow, and no hidden shell round it. Logs are round, as thick as the plant grew them.

  ![Oaks and pines on a volume's own ground](../images/small-voxels/endless-trees.webp)

  ![A crown close up, enlarged three times](../images/small-voxels/endless-crowns-close.webp)

- **Room.** A wood of 100 oaks and pines a hectare, counted on the GPU over 1,081 columns: 433 bricks a column at
  the median, 1,444 at the 99th percentile, 1,696 at most, of 90 to 110 material bytes each; the ground alone keeps
  170. A crown's shell is thin, but it crosses many half-metre bricks. A thinner shell and no hollows round branches
  took the most from 1,915 to 1,696. As each column's slot has room of its own, it must fit the fullest: the room
  grows by 14 bricks a column for each tree to a hectare, in steps of 512 so dragging the number rebuilds rarely.
- **Measured** (Rolling Hills, reach 64 m, 16 voxels a metre, 1920×1080, the GPU shared):

  | | Rolling Hills | with 100 oaks and pines a hectare |
  |---|---|---|
  | Window made before the frames | 0.54 s | 1.42 s |
  | On the GPU | 103 MB | 426 MB |
  | Columns a frame at most | 16 | 8 |
  | Flying at 10 m/s: frames making columns | 66 of 540 | 110 of 540 |
  | Making, median (least–most) | 1.55 ms (0.25–2.08) | 2.64 ms (0.99–4.53) |
  | A column | 0.097 ms | 0.33 ms |
  | Still frame, depth and lit voxels | 3.0 + 5.0 ms | 4.5 + 5.8 ms |

- **Next.** Room shared by the columns rather than kept for the fullest one in each would take the memory to about a
  third: the median column keeps a quarter of what the fullest does. Then sharing a tree's bricks between all the
  trees of its plant, which the plan has as stage 3's second step. Crowns' rays cost 1.5 ms of depth: levels of
  detail for far crowns would take some of that and keep them from shimmering.

## Stage 3, room shared by the columns (2026-11-25)

Each column's slot had room of its own for bricks and materials, enough for the fullest column. Columns keep far
less on average: grass and flowers on hills 170 bricks and 20 KB of materials against room for 384 and 61 KB; caves
99 and 4.5 KB against 1,024 and 64 KB; a wood of 100 trees a hectare 534 and 47 KB against 1,920 and 233 KB.

- **Pages.** An endless volume's bricks are kept in pages of 64, its materials in pages of 4 KB, all in one shared
  room. A slot lists the pages it holds, at most 64 of bricks and 128 of materials. While a column is made, its
  bricks and materials are kept in its job's own room; then three passes place them: one gives back the pages the
  slot held, one takes the pages the new column needs from those free and places each brick's materials in them,
  none across the end of a page, and the sectors pass copies each brick to its page.
- **The walk.** A sector's entry names its slot and where its bricks start among the slot's. Finding a brick takes
  one more read than before: its page in the slot's list (`voxelBrickAt` in `voxels.slang`). The pictures are the
  same to the pixel.
- **How large.** The room is what a column keeps on average, as measured, a quarter more, for every slot of the
  window. A column that finds no page free marks it, and the volume is made again with room half as large again: a
  test with room for 0.3 of the average grew three times and showed the whole window.

| Reach 64 m | Room of its own | Shared room |
|---|---|---|
| The volume's own ground | 102 MB | 90 MB |
| Rolling Hills | 103 MB | 91 MB |
| With 100 oaks and pines a hectare | 426 MB | 185 MB |
| Alpine with overhangs and caves | 164 MB | 64 MB |

Making a column costs about as much as before (0.36 against 0.33 ms with trees). Of the 90 MB of plain ground,
about 25 MB is the room jobs use while making and the sectors' entries, which do not grow with what is kept.

## Stage 3, trees sharing their plants' bricks (2026-11-25)

Each column filled its trees' voxels itself, working out each leaf voxel from the eight blocks round it, and kept
them: a wood's columns kept 360 more bricks than its ground's, with their materials. Now each plant's voxels are made
once, on the CPU, and a column keeps only a word for most of a tree's bricks.

- **Each plant made once.** Each plant is filled as the bricks pass filled it, in each of four turns
  (`trees::models`), and packed as a volume of its own. Their bricks and materials go first in the shared room, in
  pages no column is given; an oak and a pine, four plants each, take 13 MB in all.
- **On the bricks' grid.** A tree's plant's box is set on the grid of the world's bricks: the tree stands up to
  half a metre aside of where its cell's hash puts it, and up to seven voxels lower, its foot in the ground. A
  brick of the world is then a brick of the plant, whole.
- **Named, not kept.** Where a brick is one tree's alone and the ground has nothing in it, the column keeps the
  number of the plant's brick, a word after its own bricks; the sector's entry marks which of its bricks are named so
  and where their names start. Where two trees cross, or a tree stands in the grass, the bricks pass works the
  brick out from the plants' bricks and the ground and keeps it as its own: 0.6 bricks a column for each tree to a
  hectare.
- **The walk** reads a named brick's name, one more read for those bricks alone (`voxelBrickAt`), and draws the
  plant's brick as any other.

Measured back to back with the code before, twice (Rolling Hills, 100 oaks and pines a hectare, reach 64 m, 1080p):

| | Columns' own trees | Plants' bricks shared |
|---|---|---|
| On the GPU | 185 MB | 120 MB |
| A column made | 0.44 ms | 0.26 to 0.35 ms |
| Window made before the frames | 1.54 to 1.60 s | 1.68 to 1.77 s |
| Depth (voxels), still view | 4.64 ms | 5.13 ms |
| Opaque (voxels), still view | 5.85 to 6.06 ms | 6.19 to 6.20 ms |

Volumes without trees are drawn and made as before, within the runs' noise.

**Tried first: each tree walked through its plant.** Each column listed the trees that reach it, and the walk took
the ray into each tree's plant, moved and turned, as a volume of its own. It kept 96 MB and made a column in
0.20 ms, but the depth pass took 9.4 ms: a ray skimming a wood goes through the boxes of many trees, which overlap,
and walked each through its own voxels, where the bricks of the world are walked once. Without the walks through the
plants it took 3.4 ms; taking the trees nearest first, and shadow rays stopping at the first they meet, saved
nothing. A second walk in the shader also made volumes without trees 0.3 to 0.4 ms slower to draw.

**Next.** Wind, below. The crowns' rays are still most of a wood's depth pass.

## Stage 3, far crowns (2026-11-25)

The walk stops at 2×2×2 groups where a voxel is under a pixel, and at whole bricks under a quarter, by distance and
by how many steps a ray has taken. A group was full where any of its voxels is, in the brick's average colour.

- **Coarser sooner was tried, and left out.** Leaves going to their groups 2, 4 and 8 times nearer than the ground
  took a wood's frame (Rolling Hills, 100 oaks and pines a hectare, reach 64 m, 1080p) from 13.3 ms to 11.7, 10.5
  and 9.7. But a crown 20 to 40 m off, its voxels one or two pixels, then showed as solid lumps of one colour in
  steps, where its voxels are the loose cubes wanted. Within a reach of 64 m there are hardly any crowns far enough
  for coarser levels to look right; they belong with the rings of stage 5.
- **Groups of leaves kept loose.** Keeping only the groups at least half full emptied the crowns: a crown's leaves
  are a shell two voxels deep, which few groups lie wholly in, and the walk saw through to the logs. A plant's
  bricks, and a column's bricks mostly of trees' voxels, are now marked as leaves (over the average colour); their
  groups are all those mostly full and three in five, by hash, of those with a few voxels, as the voxels themselves
  are loosely filled; and a group of leaves takes the colour of a voxel of its own, not the brick's average.
- Against a picture with every ray walked to its voxels, the tones (means over 4×4 pixels) come nearer: from above
  8.6 against 7.2 before, on the volume's own ground 3.6 against 4.5, across the hills 2.4 against 2.7. Drawing costs
  the same (5.19 and 6.28 ms against 5.12 and 6.29 for depth and lit voxels).

![Far crowns at a reach of 128 m: groups full where any voxel is, and loosely filled](../images/small-voxels/far-crowns.webp)

## Wind in voxels (2026-11-25)

Traced voxels do not move with a vertex shader, but the ray can be moved instead. Leaves and grass of an endless
volume move in the level's wind (a Wind Zone, or a fair day's wind), as block leaves sway (`blockSway`).

- **Which bricks.** A brick of leaves with no wood in it sways whole; a brick of grass and flowers with no ground
  in it leans, more the higher up, from nothing at its foot. A brick with any ground or wood keeps still. The mark is
  two bits over a brick's average colour, set where a plant's voxels are made and where the bricks pass works a brick
  out.
- **How.** The walk finds a swaying brick's voxels where they stand moved by walking the brick whole with the ray
  moved back by the sway, and its lean undone, which keeps a ray straight (`voxelSwayedBrick`). The ray keeps its own
  distances, so the hit's place, depth and shadows are where the moved voxel is: shadow rays sway alike.
- **How far.** Downwind as hard as the wind and the gust over it blow, to and fro and across at a pace drifting
  slowly over the ground, so neighbouring bricks move nearly alike: leaves up to 2 voxels, grass 0.2 voxels across
  for each voxel up. Moved voxels are cut at the edges of the brick the ray is in by up to that; no gaps show.
- **What it leaves out.** Blades wholly in the brick with the soil under them keep still, as most short grass does.
  A swaying voxel's motion vector is the camera's alone: it moves under a tenth of a voxel a frame, and working its
  sway twice a pixel cost 2 ms of the lit pass, as the shader carried more.

![Crowns swaying in a breeze, enlarged twice](../images/small-voxels/crowns-in-the-wind.webp)

Measured against the code before, one run against two (reach 64 m, 1080p, a fair day's wind): walking a swaying
brick whole skips looking up its brick again at each voxel, which every ray through grass did, so the passes got
faster.

| Depth + lit voxels | Before | Swaying |
|---|---|---|
| The volume's own ground | 2.36 + 2.84 ms | 1.88 + 1.67 ms |
| Rolling Hills | 3.00 + 5.33 ms | 2.29 + 3.30 ms |
| With 100 oaks and pines a hectare | 5.17 + 6.31 ms | 4.00 + 3.67 ms |
| Alpine with overhangs and caves | 1.11 + 1.94 ms | 1.60 + 2.06 ms |

**Walking every brick whole, tried and left out.** Measured three times in turn against the code before:

- Every ray walking each brick whole made the depth pass 0.1 to 0.8 ms faster and the lit pass 0.4 to 1.0 ms slower,
  through the shadow rays, which start at a surface and cross many thinly filled bricks of grass.
- The camera's rays alone walking bricks whole was faster in both, at first (the hills' frame 7.1 to 6.0 ms). But a
  ray walked whole counts its steps differently and stays at single voxels a little further out, where a shadow ray
  from the same place already walks groups and finds the group the voxel is in over it: lines and bands of the
  ground went dark. Starting such shadows off the group fixed the pictures, and the shadow rays then walk on instead
  of stopping at once: the lit pass took 0.6 to 1.3 ms more than before, and the pictures were no nearer one with
  every ray walked to its voxels.

So only bricks that sway are walked whole. A shadow ray's level is tied to the camera ray's by step counts, which
any faster walk has to keep the same.

## Stage 4 begun: carving and filling (2026-11-25)

An endless volume's ground is never kept on the CPU: it is made on the GPU as columns come into view, and made
again after they leave. So its changes are kept as what was done, not as voxels: balls carved out or filled with a
ground, in order (`VoxelVolume::edits`), as the plan has edits go out over the network.

- **On the GPU.** Every column made takes the edits that reach it (a pass lists them, up to 1,024), and every brick
  those with its border of one (up to 128, in order). In a brick an edit reaches, each of its voxels and its border's
  is solid as the ground or the Volume makes it, then each edit over it in turn; a voxel is kept where open air is
  beside it, so a pit opens walls of the layers under the grass, in the ground's own shades, and a fill is drawn in
  its ground's. Grass and flowers go where the ground's top under them was carved or filled, trees where they were
  carved. A column's candidate bricks reach as high and as low as its edits do, and the coarse pass keeps them.
- **Made again.** A changed list makes again the columns its changed edits reach, a frame or two later.
- **From scripts and tools.** `Voxels.Carve` and `Voxels.Fill` in C#, and `voxels.carve` and `voxels.fill` in
  `vox editor call`, reach every endless volume the ball falls in. The scene saves the list.
- **Where a ray meets it** (`voxel_volume/ray.rs`). On the CPU, worked out as the GPU makes the ground, to the voxel:
  its own ground's hills, the generator's height, or the generator's Volume with its overhangs and caves, then the
  edits. Far from the ground and from the balls filled, the ray takes steps of four tenths of how far over the ground
  it is (three tenths of how far into the Volume's air); near them, a voxel at a time. A ray down the hills 9 m off
  took 0.09 ms, one into the mountains through the Volume 52 m off 0.46 ms; a small ball carved where the CPU's ray
  met the ground opened a pit where the GPU's picture has the middle of its view. Grass, flowers and trees are not
  met, as the CPU does not make them. `Voxels.Raycast` in C#, `voxels.raycast` in `vox editor call`.
- **A brush.** In the Scene view, on a selected endless volume with its Inspector's Brush on: a ball where the
  pointer's ray meets the ground, carving or filling as it is dragged, a ball every third of its size, one step to
  undo a stroke.
- **Not yet.** Changed bricks in region files, so a long list of edits needn't be played again; sending edits over
  the network; setting one voxel; volumes of a set size; collision; rays that meet grass, flowers and trees.

![A pit carved out and a ball of rock filled, a bite carved out of the rock after](../images/small-voxels/edits-from-above.webp)

## Rivers, lakes and the sea in endless ground (2026-11-26)

A generator's **Rivers** node finds its rivers over a grid of points round each place, with what drains through
each, on the CPU (`generate/nodes/rivers.rs`). The GPU's program passes its ground through. So the rivers are laid
in from the CPU, as streamed worlds of blocks take them: as a change of height to add to the GPU's ground
(`voxel_volume/rivers.rs`).

- **Points, not voxels.** For each 4 m column, the CPU works out the generator's height with its rivers and
  without them (`WorldGen::without_rivers`, the graph with every Rivers node passing its ground through, which is
  the GPU's program), its water and its falls, at points eight voxels apart, from one step back of the column's
  corner: 11 × 11 points, 121 places of the graph run three times. The difference is the rivers' cut. The GPU adds
  it, read between the four points round each voxel column, to the height its program made; the CPU's ray reads the
  same points the same way. A generator's Volume has the cut added to how far into it a voxel is, which moves its
  surface by as much.
- **A table of points the GPU reads.** Points are worked out on two to six threads of their own as columns come
  near, the nearest first, and kept (8,192 columns at most, about 1.5 KB each). They are written into a table with a
  slot for each column of a window wider than the volume's by a column, for the heights' border, and by as far as a
  tree's crown reaches, as a tree's foot may stand in a column round the one being made and must stand at the same
  height in each. A column is made once its points and those round it are in the table; a slot is written over only
  frames after its last column left. Columns with no river near read a word and go on.
- **The ground under the water.** Gravel under a river's or a lake's water, sand along its edge; no grass, flowers
  or trees.
- **The water drawn.** A square of water on each square of points with water at a corner, its dry corners as high
  as its highest wet one, so the water reaches into the bank and the ground hides it there. The squares are drawn
  as a world of blocks' water is (`voxelWaterVertex`, `blockWaterLit` in `shaders/water.slang`): ripples carried
  downstream as fast as a generator's current runs, a line of foam where the water meets the ground, the bed seen
  through it, reflections. A square whose corners stand over 0.3 m apart where the generator says water falls is a
  waterfall's face, with white threads running down it; the squares beside its foot churn.
- **The sea floor.** The map of the ground under the sea that calms waves over the shallows and lays foam along the
  shore was drawn from meshes standing still. Voxel ground is drawn into it too: a fragment per texel of each slice
  walks straight down through the volumes from the floor's ceiling, and straight up from its reach for the
  underside (`voxelFloorFragment` in `shaders/voxels.slang`).

- **Measured** (reach 64 m, 16 voxels a metre, 1920×1080, RTX 3070 shared with other sessions; each the least of 60
  frames). The CPU's points: 0.7 ms of one core a column, 0.3 ms at a metre apart, which was too coarse for a stream
  a few metres wide.

  | Rolling Hills | Without rivers | With rivers |
  |---|---|---|
  | A column made, 16 a frame | 0.104 ms | 0.104 ms |
  | Window made before the frames | 0.61 s | 0.61 s |
  | Over the river: depth / lit voxels | 1.61 / 2.90 ms | 1.86 / 3.62 ms |
  | Water drawn | none | 1.77 ms, 3,065 squares |

  With 100 oaks and pines a hectare, a column takes 0.23 ms (least) to 0.34 ms (median), eight a frame, as before.
  The lit voxels over the river cost more as their light is taken through the water. The sea floor's voxels add
  0.025 ms on the frames a slice is drawn again. A test finds a river in Rolling Hills, meets its bed 2.6 m under the
  ground without it with the CPU's ray, within a voxel of the generator's own bed, and fills a ball there that stands
  out of the water in the GPU's picture.

![A river cut into Rolling Hills, before and after](../images/small-voxels/river-before-after.webp)

![A shore of Archipelago, before and after](../images/small-voxels/shore-before-after.webp)

![A waterfall 7.6 m high in Alpine, from beside its pool: a wall of water standing out from the cliff (above), and
falling from its lip (below)](../images/small-voxels/waterfall.webp)

**Waterfalls.** The river's node cuts a cliff at a fall's lip and keeps the water at the fall's top for 1.6 m past it,
over the air, then at its foot. Drawn as squares of that, a fall was a flat wall of water 1.6 m out from the cliff
under a shelf of water, its sides standing out past the rock. Now each point of the fall is placed along it, from
the last point before it (the lip) to the first after it (where it lands), and the squares those points stand on are
drawn as a fine grid (8 × 8 quads a square, `voxelFallVertex`): leaving the lip level and falling ever more steeply,
as thrown water does, landing where the node says and plunging 0.4 m under its pool. Two sheets, the back one nearer
the cliff the lower it is, sway in and out more the lower they are; the front one tears into strands lower down; the
sides are ragged where the river's water ends. Each point has its one way out from the cliff, shared by the squares
round it, or the sheets split along the squares' seams. Its face is pale, hiding the cliff behind it, and lit through
by the sun behind as spray is. The pool churns white where it lands, in patches that come and go, thinning out over
3 m. A fall 7.6 m high and 6 m wide costs the water pass 0.4 ms more at 1080p.

Not done:

- **Water filling a carved pit.** The squares are the generator's water alone. Water of voxels (below) fills what a
  ball carves out under a river's top, as its layer is made wherever the column is open there, but not a pit dug
  beside a river, and nothing flows.
- Lakes and rivers of a Volume's caves: the points say nothing of what is under an overhang.

## Water of voxels (2026-11-26)

A volume's **Water** can be **Voxels** (`VoxelWaterLook`): its rivers and lakes made of voxels as small as the
ground's, in place of the smooth squares over them.

**First try: a walk through the water to the bed.**
- How it worked:
  - Each column put a layer of water voxels at the water's top, as deep as its top stands over the lowest beside it,
    so its steps leave no gap.
  - The lit pass met a water voxel as it meets ground, then walked on through the water to the bed. It lit the bed
    simply and dimmed it by the water the view crossed.
  - Falls were blocks of falling-water voxels as wide as the fall.
- It cost more and looked worse.
  - Rolling Hills' river view: the lit voxels took 9.8 ms, against 3.6 ms and 1.8 ms of squares for smooth water.
  - Each part turned off in turn, in the same view: the walk to the bed was 4.3 ms of that, the water's own shadow
    walked through the voxels 2.3 ms, the foam 0.4 ms, the reflection nothing.
  - A sloping river's top drops a voxel every few voxels: each step's side showed, striping the whole river.
  - The fall was a wall.

**What it does now.**
- **Shades by depth, in place of the walk.** The column's making knows how deep the water is over its ground, so
  each voxel takes one of eight shades, from sandy where it is shallow to deep green-blue, 0.3 m apart. The lit pass
  walks nothing more than the hit: the shade lit by the sky and the sun, the sky's reflection, glints, ripples drifting
  smoothly across it.
- **Steps lit as the top.** A water voxel's side with open air in front and water under that is lit as a top: the
  stripes go.
- **The water's shadow walked coarser:** three times the pixel's footprint, 24 m at most, through the water as through
  air. Ground's shadows and the sea's floor go through water voxels too.
- **Waterfalls as voxels thrown over their lips.**
  - The CPU finds each piece of a fall's lip, half a metre or less: where its corners before the fall are. It gives
    the fall's top and drop, the way out from the cliff, and how fast the water leaves the lip to land where the
    river's node says, 1.6 m out.
  - Each piece throws 1,536 voxel-sized cubes (`voxelDropVertex`). Each has its own moment, place along the lip,
    depth back from the front, speed out (within a fifth) and drift sideways. It falls as thrown water does, worked
    out from the time alone, with nothing kept between frames, so the sheet spreads and tears as it falls.
  - Its colour goes by its speed: the water's blue-green leaving the lip, white as it passes 2 to 7 m/s, each cube
    its own. About half splash back up off the pool, the rest are gone into it, and each leaves the lip again.
  - Where a fall pours the columns make no water: none between its points, and beside them only where a point has
    the pool's water. The pool within a metre or so of where it lands is churned: two white shades whose foam boils.
- **Measured** (1920×1080, RTX 3070 shared, least of 60 frames, whole frames, as the two looks' passes differ):

  | View | Smooth water | Water of voxels |
  |---|---|---|
  | Rolling Hills, over the river | 10.5 ms | 8.6 ms |
  | The same with oaks and pines | 14.1 ms | 12.9 ms |
  | Alpine's 7.6 m fall, from its pool | 8.6 ms | 8.8 ms |
  | The same from its side | 9.9 ms | 8.7 ms |

  A column takes 0.11 ms either way, 0.23 to 0.35 ms with trees. The falling voxels cost 0.19 to 0.25 ms.

![Smooth water (above) and water of voxels (below) over Rolling Hills' river](../images/small-voxels/water-smooth-and-voxels.webp)

![A waterfall of smooth water (above) and of voxels thrown over its lip (below)](../images/small-voxels/waterfall-smooth-and-voxels.webp)

Not done: water of voxels in the far bricks takes its brick's average colour, with no ripples; spray mist round a
fall's foot; the falling voxels collide with nothing, so a ledge in a fall's way does not split it.

## Stage 7 begun: collision (2026-11-26)

Nothing collided with an endless volume: its ground is made on the GPU as columns come into view and never kept on
the CPU. Physics needs the solid voxels near what moves, and only there.

- **What is solid** (`voxel_volume/solid.rs`). The CPU's ground, worked out as the GPU makes it, was the ray's
  (`ray.rs`); it now also places the trees, by the GPU's hashes on the same ground: each cell's kind and foot, the
  ground's height and slope there, the generator's climate and rock, and the Volume's density under the foot, all
  worked out at once for the cells round a box. Each plant's voxels are made on the CPU in its four turns, as the
  GPU's are, and kept for every volume with the same trees. A log is solid, leaves are not; where two trees cross, a
  log wins. A test looking down on a wood with red leaves and blue logs checks the CPU's rays find trees where the
  picture has them.
- **Chunks round what moves** (`voxear_physics` `scene/voxels.rs`). An Obj's `VoxelCollider` says which voxels are
  solid; the engine gives every endless volume one, again whenever it changes. Its voxels are made into a static
  mesh of their faces in chunks of 32 voxels a side, faces side by side in a plane joined into rectangles. Chunks
  are wanted within 5 m across and 2 m up and down of each character and Rigidbody, where it is and where it will be
  in half a second; those within a metre of where it is are made at once if not ready.
- **What it cost to get there.** Measured walking at 6 m/s for ten seconds, with other sessions' builds keeping the
  machine busy (load about 20 on 24 threads):

  | Physics step | Median | Most | Over 2 ms |
  |---|---|---|---|
  | The volume's own hills | 0.17 ms | 3.0 ms | 1 of 600 |
  | Rolling Hills | 0.22 ms | 6.3 ms | 13 of 600 |

  - At first, chunks within a metre were made at once 1 to 2 times a second, at 8 to 23 ms each on a generator's
    ground. A chunk's heights were asked of the generator a column at a time, each tree's foot five times more,
    and every voxel looked in every tree near it.
  - Heights asked as one row, trees' feet all at once, a tree's box checked before its voxels, and each plant's log
    voxels listed and stamped in took a chunk to 2.5 to 5 ms.
  - The rest was waiting: the workers' queue is first come, first served, and a hundred far chunks stood before the
    one needed. The chunks now have a queue of their own, sorted nearest first every step, on six workers.
  - One fault on the way: the chunks wanted at once were measured from where a mover would be in half a second,
    not from where it was, so they came due as soon as they were asked for.
- **Not yet.** Volumes of a set size; physics rays meet only the chunks made round what moves; navigation.

## The sea, and floating and swimming in voxel water (2026-11-26)

The renders above added an Ocean by hand at the generator's sea level, and the engine's water (`voxear_engine::water`)
read Oceans, Pools, Streams and worlds of blocks but not the voxel rivers.

- **The sea is the generator's.** An endless volume with a generator has a **Sea** setting, on unless turned off.
  `voxear_render::ocean::seas` lists the level's seas, the first of them used: each Ocean at its Obj's place, but one
  on an endless generated volume's Obj stands at the generator's sea level; then, where the level has no Ocean, each
  such volume's sea as a default Ocean at `corner.y + sea_level - floor`. The renderer's sea, its Streams' mouths,
  its spray and the engine's water sampler all ask it, so nothing else changed. A sea under the volume's floor is
  never made.
- **The water's points are read on the CPU.** `RiverGrid` now keeps the generator's ground with its rivers at each
  point as well as the water. `voxel_volume::rivers::water_at` reads the four points round a place as the water's
  squares are drawn: a dry corner as high as the highest wet one, so the water meets the bank where the ground hides
  it. It is found over the water and under it to a metre into the bed, not deeper, so a cave under a river stays dry.
  Its current is the generator's (`WorldGen::current`, now `current_between` over five heights a metre apart), read
  from the same points, not run through the graph again: a column's points are worked out once, on the river
  threads, and kept.
- **The engine's water asks it** after Pools and worlds of blocks and before Streams and the sea, so a Buoyancy
  floats on it and drifts with its current, a Character Controller wading it is carried, and everything that reads
  `water::sample` (wakes, splashes, heat) sees it. C# has `Voxels.WaterAt` and `Voxels.Sea`/`SeaLevel`;
  `vox editor call voxels.water_at`.
- **Interpolated, not the highest of four.** `Ground::river` (`solid.rs`) gives the highest water of the four points
  round a column, as the GPU's `riverAt` reads it for grass and trees. What floats reads the points interpolated, as
  the water's squares are drawn, so it rides the surface seen, not a step at each point.

Measured on Rolling Hills (seed 7) at 16 voxels a metre:

- A river 1.6 m deep: the surface found where the generator has it, to the centimetre, and between the points within
  10 cm of the generator's own water; its current the generator's.
- A crate dropped in settles with its middle 2 cm off the surface and drifts 1.84 m down a 1.1 m/s current in 3 s;
  a character standing in it is carried 2.97 m.
- Archipelago's coast, seen from 3 m over the sea: the sea's water in 17,272 of 17,280 pixels out to sea that showed
  its floor with the Sea off.

![The generator's sea at an Archipelago shore](../images/small-voxels/generated-sea.webp)

Not done: what floats over voxel shallows feels the ground only within the chunks made for collision round it, a few
metres, so the sea calms it less than it is drawn; the sea standing under the ground inland, so a cave dug below sea
level is under water where the Sea is on; waterfalls pushing down what is under them.

## What grows where, and mobs (2026-11-26)

The Realm needs crops in its fields, trees only in its woods and orchards and none in the city, and animals on its
ground. A world of blocks does these with ground cover on chosen blocks, structures kept to World Areas by their
tags, and Block Mobs walking its blocks' places. An endless volume now does the same.

- **World Areas** (`voxel_volume/growth.rs`). The engine gives each endless volume whose trees, cover or mobs name
  an Area Tag the outlined World Areas of its dimension, laid out from its corner, again whenever one moves. Only
  areas holding a tag some rule names go to the GPU: each its box, its lowest and highest ground in voxels, its tags
  as bits (at most 32 tags a volume) and its corners. The trees pass asks each tree's foot after `grassAt`
  (`treeAllowed`); `solid.rs` asks the same of each foot with the same numbers in the same order, so collision and
  rays find the trees where they are drawn. A test of a wood kept west of a line, pines kept out of a town in it,
  finds no tree east of it on the GPU or the CPU, and 95% of pixels agreeing.
- **Crops and ground cover.** Each kind is a shape of the tops pass, a voxel column at a time, like grass: meadow
  grass scattered with a few seed heads; wheat, barley and flax as stalks a voxel across in rows, with ears or
  flowers; cabbages as domes of leaves on a grid, pale at the middle, with darker leaves spread low round them. The
  first kind whose ground and areas hold takes the column, in place of the grass, and may turn its ground to dirt.
  The column word was repacked to hold a stalk's head: its length and material beside the stem's, and a bit that
  keeps flowers out of a field.
- **Mobs** (`voxear_engine::voxel_mobs`). A volume lists prefabs to keep round the player, by density, ground and
  areas; the game spawns them on ground no steeper than 45°, found by a ray down, between a near and a far ring, and
  takes them away a fifth again beyond the far ring. A Block Mob on a volume, with no world of blocks under it,
  walks straight where it is sent and wanders to places it can stand, giving up on one after 12 s. Finding ways
  round what stands in the way waits for navigation over voxels.
- **Measured** at 1920 × 1080 on Rolling Hills with 100 oaks and pines a hectare, reach 64 m, the GPU 98% busy with
  other sessions' work (so only the comparison holds):

  | | Making, flying at 10 m/s | Depth + lit, least of 60 |
  |---|---|---|
  | Neither | least 1.01 ms, median 2.00 ms a frame | 4.05 + 3.67 ms |
  | Trees kept to 24 areas of 8 corners | least 1.00 ms, median 1.81 ms | 4.26 + 4.44 ms |
  | And wheat, cabbages and meadow grass | least 1.01 ms, median 1.85 ms | 4.04 + 4.20 ms |

  The areas and cover cost nothing measurable in making a column; the drawing differs by what is in view. 400 CPU
  rays down through the wood took 1.32 s without areas and 1.36 s with them.
- **One fault on the way.** Reading the flower's foot column before checking it was near read past a job's columns
  and lost the device: a test with flowers on caught it, the tree tests having flowers off.

## Stage 4 on: edits for building a city (2026-11-26)

The first city to build in endless ground, a harbour city of streets, squares, a quay, levelled plots, graded banks,
cellars and about sixty kinds of building, wanted more of the edits than balls and prints.

- **Typed shapes** (`voxel_volume/edit.rs`). An edit is a shape, a fill (a ground, a paving, or none for air), a tint
  and how far it clears over its top: a ball; a box turned any way, placed by the middle of its foot (half a metre
  tall it is a slab); a bank, a box whose top slopes along its length; an area, an outline of points from one height
  to another; a cover, the ground's own top within an outline, as deep as it says. One area paves a street whatever
  its length, so a paved city is a few hundred edits, not one a block.
- **One answer for a voxel, worked out twice.** `editHeld` on the GPU and `VoxelEdit::held` on the CPU give the same
  thing for a voxel: nothing, air, a fill with what fills it, or a print's colour. The bricks pass and the CPU's ground
  for collision and rays take each edit over the last in turn. Exactness is checked, not hoped for: a probe pass reads
  an endless volume's voxels back from the GPU (`Renderer::voxels_on_gpu`), and a test compares every voxel round
  each kind of edit with the CPU's, those solid with air beside them, which are what the GPU keeps.
- **Pavings** are patterns on the world's grid, worked out on the GPU from a voxel's place: rows of cobbles,
  flagstones, planks, courses of bricks and ashlar, each stone a shade of three by hash, a voxel of joint round it.
  Twenty colours after the generator's. Tinted, a fill's colours are shaded from the tint as from their own average:
  each edit with a tint has a colour table among the edits' words, a palette place for each of its colours.
- **Covers follow the ground.** A cover holds the voxels from the column's ground's top down as deep as it says;
  the GPU passes each voxel's column height to `editHeld`, the CPU its own. A column's candidate bricks reach as far
  round its ground as a cover does.
- **Fitted prints.** A print can be cleared inside, down to its foot, and stand on a footing of its fill reaching
  down to the ground, in the columns its footprint says it stands over, at most as deep as it is tall.
- **Trees and edits.** A tree grows not where the last edit to hold its foot carved it, nor where the last to hold
  the voxel over it filled it, as a street does. The trees pass now reads the column's list of edits, widened by how
  far a tree's foot can be from it, not every edit there is; which is last is found by number, as the list is not
  in order.
- **More per place.** A column now takes 4,096 edits (and the trees round it), a brick with its border 256.
- **Exact, in whole numbers.** The first comparison found two voxels in 131,328 at a turned box's edge where the CPU
  and the GPU disagreed: the same float sums rounded differently (a fused multiply and add on the GPU, the CPU's
  own order of adds). Every shape is now worked out in whole numbers the CPU makes once (`edit::Fixed`): places in
  sixteenths of a voxel, a turn's rows in 4096ths, products in 64 bits on the GPU. A print's own voxel is found by
  dividing by how many of its voxels and the volume's make a metre, both whole.
- **A voxel at a time** (`voxel_volume/changes.rs`). Dug and placed voxels are kept brick by brick: 512 bits for
  which are set and a byte each, 0 for air or a kind (a ground or paving and a tint, up to 255 a volume). On the
  GPU each changed brick is one more edit after all the others, so the bricks pass needs nothing new; on the CPU a
  voxel set goes over the edits. A changed brick makes again only its column and the trees round it.
- **Region files.** 128 bricks a side (64 m at 16 voxels a metre) to a file beside the scene, its kinds, then packed
  with zstd each brick's place in the region, its bits and its bytes for the voxels set. A tunnel of 320 voxels
  across two regions is under 2 KB; reading one is a zstd unpack and a copy.
- **Colours in banks.** A material stays a byte. Below 128 it is a place every brick shares; above, a place in the
  brick's own bank of 128, which the brick keeps in five spare bits over its average colour. The bricks pass counts
  its voxels' banks and keeps the most; a voxel of another bank takes the shared colour nearest it, kept over its
  colour in the palette. A table's colours (a print in a tint, a tinted fill) go together into one bank, the
  palette's last or the next, so a brick of one building has all of its colours. 4,224 colours: room for sixty kinds
  of building of thirty colours, each in its own tint, and more.

### Edits over the network: a design

What every machine must have alike is the volume's list of edits and its voxels set one at a time; both are already
what the CPU and the GPU make the ground from, and both are small.

- **What is sent.** An edit is its shape in a few numbers (an area its points), its fill, tint and clear: 20 to 60
  bytes, a print its asset's GUID and place. A voxel set is its place and a byte. Neither carries voxels made from
  them: each machine makes those itself, the same to the voxel, as the whole-number shapes make sure.
- **Joining.** The scene's own edits come with the scene. Those a game made as it played, and its voxels set, go to a
  joining player as the level's block edits do (`net/dimensions.rs`): the edits added since the scene loaded, in
  order, then the region files' bytes of its changed bricks, cut into pieces nearest the player first, within a share
  of the connection's budget, then a word that they are all sent. The player's ground is made as they arrive.
- **Playing.** The server is the one that edits: a client asks (dig here, place this, print that), the server checks
  it may and adds it to its list, and sends the edit or the voxels set to every player who has the volume, in order,
  on a reliable channel, with the edit's number in the list so a late one is put in its place. A client may show its
  own dig at once and take the server's answer over it.
- **Saves** keep the same: the edits added since the scene loaded, and the region files.

Not built yet: the messages and the join; the bytes they would carry are what the scene and the region files hold.
## Stage 6 begun: shadows, light indoors and glass (2026-11-26)

The Realm's city moving onto Voxel Volumes needed three things lighting gave its blocks: things standing among the
voxels shadowed by them, rooms and cellars dark but for their lanterns, and windows of glass.

- **Voxels shadow everything** (`voxelShadowFragment` in `shaders/voxels.slang`). The voxels are in no shadow map.
  Once the depth passes have drawn the picture, a pass traces a ray towards the sun from each pixel through the
  voxels, whatever is drawn there, and writes whether it got through. Models, blocks, terrain and grass read it
  with the contact shadows, as they all share one lit fragment shader. A voxel's ray starts as its own lit pass
  started it before, a group or a brick out at the coarser levels, found by the hit the depth pass kept for the
  pixel; anything else's starts a voxel and a half out along its normal, more where a pixel is wider. The voxels'
  lit pass no longer traces its own. Tracing into a shadow map instead was set aside: four cascades of 2048² are
  16M rays a frame, eight times the screen's.
- **Light kept round the camera** (`voxel_volume/light.rs`, `voxelLightCells`, `voxelLightSky` and
  `voxelLightSpread`). This is the plan's grid at the bricks' half metre, as the light kept for blocks is a byte a
  block, 96 × 48 × 96 cells. A cell keeps sky light and glow, with the glow's colour in 16 bits.
  - What lets light through a cell is worked out each frame from its brick: for each side, the lines from that side
    to the cell's middle that meet no voxel. Two cells pass light by the lines open from one middle to the other,
    matched line by line, so a cell is the light at its middle. A wall a few voxels thick anywhere in a brick keeps
    light out, and the air over a floor in a brick's lower half is open to the sky.
  - The sky is found straight down from the top row afresh each frame. Then four steps spread light a step less a
    cell (a level a metre, as blocks), read from last frame's moved as the camera moved, 32 steps when the volumes
    are new.
  - Voxels, models and grass read it between the cells round them that are not solid. Along the axis the surface
    faces, the cell before it is read alone, so a wall's face never takes the light of the far side.
  - It is shared by the lighting with blocks' (`blockSkyShare`, `blockGlowShare`, the same darkest sky and glow
    brightness). A room of voxels with a lamp of light 15 looks as the blocks test's room of blocks does.
- **Glass and glowing voxels.** A palette colour's kind is kept on the GPU after the palette's colours, a byte a
  place: how see-through (0 to 15) and how bright it glows (0 to 15). On the CPU it is `255 - kind` in the colour's
  fourth byte, so every palette until now is plain. A brick with any such voxel, or with water of voxels, is marked
  in the lowest bits of its average colour, so only those bricks are looked into; its top bits are its palette bank.
  - The walk passes through a see-through voxel and keeps the first it met. The depth pass writes it beside what it
    met, and **Glass (voxels)** draws it lit over everything opaque, blended by how solid it is and what it reflects.
  - Sun shadow rays pass through glass and water. In the light kept round the camera, glass, water and glowing voxels
    let light through, and a glowing voxel's cell gives its glow. A glowing voxel shines, its colour times four at light 15.
  - A print's colours are made glass or lights by its settings' `materials`, in its Inspector's **Glass and Lights**
    or `voxel_prints.materials`. Its id takes them in, so volumes printing it are made again.

![The pillar's shadow across a model's slab, which voxels did not shadow before](../images/small-voxels/shadow-on-a-model.webp)

![A house of voxels open to the sky, roofed and shut, roofed with a door, and shut with a lantern](../images/small-voxels/rooms-sky-door-lantern.webp)

![The blocks test's sealed room lit by a lamp block, and the same room of voxels lit by a lamp of voxels](../images/small-voxels/room-blocks-and-voxels.webp)

![A stone wall, and the same wall with a window of glass, the red pillar inside seen through it](../images/small-voxels/window-glass.webp)

What went wrong on the way, worth knowing for the rest of stage 6:

- **Sides in the wrong order.** The six sides were listed y-first and their axis taken as x-first, so light crossed each
  cell by the wrong axis's openness and a sealed room was sky-lit.
- **Lines through a whole cell** kept the sky out of the air over a floor in a cell's lower half, as no line crossed
  the cell, and a flat field went dark.
- **Hollow ground.** An edited brick keeps only the voxels open air is beside, so under the ground's skin and inside
  thick walls there is nothing. Past a volume's window and under its floor, where no brick is, light came round
  underneath and up into rooms. Those are solid now; inside a volume, the hollows are sealed by their skin.
- **Counts, not lines.** Two cells passed light when each had *some* lines open across their side, though not the
  same ones, and light got through the foot of every wall. Lines are matched one by one now.
- **Stale light.** Light kept from the frame before falls a step a step, so a scene changed at once kept its old
  light. Other voxel buffers or palettes, as a generator changed in the Inspector or a print added, start it afresh.
- **A lamp of voxels a metre wide** filled its cells, which were then solid and kept their glow in. Glowing voxels
  let light through now, as a block of light does.

Measured on an RTX 3070 shared with other sessions: a street of six houses (5 × 4 × 6 m, glass windows, gables)
printed into Rolling Hills without its rivers, with a dozen figures among them, seen from 20 m up at 1920 × 1080.
Each number is the least of 60 frames, three runs each, on the commit this was built on and with it:

| | Before | Now |
|---|---|---|
| Frame, post-processing on | 6.57 ms | 6.96 ms |
| Frame, post-processing off | 6.36 ms | 6.79 ms |
| Depth (voxels) | 2.04 ms | 2.21 ms |
| Opaque (voxels), shadows traced in it before | 3.41 ms | 0.88 ms |
| Voxel shadows | – | 2.27 ms |
| Light round the camera (cells, sky, 4 spread steps) | – | 0.26 ms |
| Glass (voxels) | – | 0.13 ms |

The shadows cost what the lit pass's tracing did, now for every pixel. The rest is the light, the glass and the
glass's look-ups in the depth pass (`measure_a_street_of_houses` in `tests/voxels.rs`).

![The street before and after, from 20 m up](../images/small-voxels/street-before-after.webp)

![Close up: the windows are glass now and the houses dark inside but by their doors](../images/small-voxels/street-close-before-after.webp)

Since, the same day:

- **Lamps stopped by voxels.** A pass of its own after the voxels' shadows (`voxelLampShadowFragment`) traces from
  each pixel to each lamp of its cluster, the first 24, and writes which have voxels in the way as a whole number in
  a float image; every lit pass skips those lamps (`voxelLampOut`). In the sun's pass it cost 0.73 ms with no lamp
  in view, by the walk's weight alone, so it is a pass of its own, which leaves a pixel at once where its cluster has
  none (0.06 ms). With twelve lamps over a street it took 3.1 ms, 24 million rays; at half the picture's size,
  leaving out lamps past four fifths of their range, 0.95 ms. Walking coarser with distance alone saved nothing.
- **The sun through glass takes its colour.** The plain walk keeps the first glass it passed as the depth pass's
  does; the sun's mask is RGBA, its tint over the lit share.
- **Fog, particles and water** read a sun map of the voxels, 256 texels a side over 64 m round the camera on whole
  texels, each a ray from 64 m towards the sun down through the voxels at a group's or brick's level (0.12 ms; 512
  texels took 0.86 ms). A wall's shadow lies on the sea beside it.
- **Measured** on the same street, against the commit before (least of 60 frames, three runs each): by day 7.83 →
  8.05 ms; at night with a lamp by each of the twelve figures, 5.88 → 7.27 ms, the lamps' pass 0.94 ms of it.

![A lamp in a house and beside it, at night; the sun through clear and red glass; a pillar's shadow on the sea](../images/small-voxels/lamps-tint-sea.webp)

Not done:

- **Glass** tints only the sun, and only the nearest glass in a pixel is drawn.
- **GI rays through voxels**, the rest of the stage. Worlds of blocks are lit without the probe GI, so it waits.
- **Single black pixels.** A few pixels of a large flat wall of voxels are drawn by neither pass, here as on the
  commit before: about 190 of a 640 × 360 view into a stone room. Unlit before, they were hard to see; they are the
  walk's, not the light's.

## Stage 5, distance in rings (2026-11-26)

An endless volume's **Distance** carries its ground on past its reach, in rings round the full detail
(`rings_of` in `voxel_volume.rs`, `endless.rs`, `voxelsTrace` in `voxels.slang`).

- **Each ring is an endless volume of its own.** Its voxels are twice the size of the ring's inside it and its reach
  twice as far: at 16 voxels a metre and a reach of 64 m, 1/8 m to 128 m, 1/4 m to 256 m, 1/2 m to 512 m, then 1 m
  as far as the distance. Its window is as many columns across as the finest's, each column as wide as its voxels.
  The same passes make it from the same generator, ground, grass, trees and edits.
- **Where a ring stops and the next begins.** Each ring draws only inside its outer box, a square round the camera
  as far each way as its window surely reaches, within the columns it has made all of, its edges on the grid of the
  next ring's voxels. The next ring passes over that box, its hole: its walk starts where the ray comes out of it,
  set exactly on its edge. A ray from the camera leaves the hole once and never goes back in. A voxel met first past
  an edge is lit as its top, never as a side the edge cut.
- **No sector at a time.** At first each ring's edge was the box of columns it had made, which jumps a sector (4 m,
  8 m, …) when the window moves: moving the camera 10 cm over a sector's edge changed 6,900 more pixels than the
  same move within a sector, in a band along the edge. With the box going with the camera, 1,100 to 1,400 more, 5%
  over the move's own, and no band.
- **A ring's voxels where it begins are about a pixel.** At 1080p a pixel 64 m off is about 7 cm, so the finest ring
  draws its groups of 1/8 m there, and the first ring begins with voxels of 1/8 m; every ring after begins at twice
  the distance with voxels twice the size.
- **The shade of each voxel is worked out from its place in the world**, not from its place in the window: it changed
  whenever a window moved, and a ring's voxel now takes the shade of the finer ring's group of its size.
- **Grass in a ring** grows on each column with the finer ring's chance, as tall as a blade of it, in whole voxels of
  the ring's own; a blade under half a voxel is left out. Standing for the finer columns under it (grass where any
  has a blade, as tall as the tallest) made the rings' ground a lit carpet, 13 levels lighter on average than full
  detail; this way the averages are within 3. Flowers are as many fewer as they are larger, and left out of rings of
  four times the voxels and more.
- **Trees in a ring stand off its bricks' grid.** On a ring's bricks they would stand up to seven of its voxels aside
  and sunk, 7 m in the ring of a metre. They stand on the finest ring's grid instead, as near as the ring's voxels
  go, so a tree crossing a ring's edge stays where it was; their voxels are looked up in the plant's bricks a voxel
  at a time, and no brick names a plant's. Crowns far off stay loose cubes in their shades.
- **What a ring keeps.** A ring makes no column wholly inside the ring within it, a column short of its edges. Its
  room is sized from what rings were measured to keep: 150 bricks and 17 KB a column in the first ring, 125 and
  13 KB after, and per tree a hectare 3, 2, 1.4 and 0.7 bricks, as their trees share no plant's bricks.
- **Columns a frame.** A ring's columns come into view as many times less often as its voxels are larger, so it
  makes as many times fewer a frame, two at least; while loading, as many as the finest. With every ring making as
  many as the finest, frames where all the rings crossed a sector at once took 21 ms; now 10 to 11.
- **Rivers and water in rings.** Each ring lays its generator's rivers into its columns as the finest does, at its
  own voxels, so their beds go on past the reach; with water of voxels, the water fills them there too. Smooth water's
  squares and waterfalls' lips are laid by the finest ring alone, over its whole window: past it, rivers of smooth
  water are dry beds.
- **The lit pass's own walk.** Carrying the hole and the outer box in the walk, or in the loop over the volumes,
  made the lit pass 0.5 to 1.3 ms slower for every volume, rings or none, though the depth pass was not: the GPU
  runs that large shader with less room. Its shadow rays and its fallback walk keep the plain loop. A shadow ray from
  a voxel walks a ring of another size from three of the larger voxels out, and a ring whose hole it starts in from
  where it comes out of it. A view with rings always draws its depth pass first, so the lit pass never walks a ring
  itself. Volumes without rings draw as on master, within the runs' noise: the generator measurement's four still
  views light in 2.23, 4.51, 4.94 and 2.63 ms against master's 2.27, 4.49, 5.26 and 3.54, their depth up to 6% more.

![Rolling Hills with oaks and pines seen to 2 km in rings](../images/small-voxels/rings-to-the-horizon.webp)

**Measured** (Rolling Hills, 100 oaks and pines a hectare, reach 64 m, 16 voxels a metre, from 40 m over a hillside
looking across the land, 1920×1080, the GPU shared with other sessions, least of 60 frames):

| | Today: to 64 m | Rings to 1 km | Rings to 2 km | No trees, to 64 m | No trees, rings to 2 km |
|---|---|---|---|---|---|
| On the GPU | 122 MB | 500 MB | 618 MB | 94 MB | 433 MB |
| Made before the frames | 1.7 s | 3.0 s | 5.4 s | 0.6 s | 2.4 s |
| Frame | 4.7 ms | 12.7 ms | 12.9 ms | 2.1 ms | 9.8 ms |
| Depth / lit (voxels) | 1.4 / 2.5 ms | 5.3 / 5.7 ms | 5.7 / 5.4 ms | 0.5 / 1.0 ms | 3.9 / 4.4 ms |
| Flying at 10 m/s: making, median (most) | 1.9 (4.2) ms | 2.7 (9.7) ms | 2.7 (11) ms | 1.4 (3.6) ms | 0.6 (3.9) ms |

The view today is sky past 64 m, about nine tenths of it; in rings the land fills the lower half to the horizon. The
cost is that of the pixels of ground: a view full of near hills costs about 8 ms today (stage 2's table). Without
trees the frame grew with the rings in view, measured before the lit pass had a walk of its own: 4.2, 5.9, 8.0 and
9.2 ms to 128, 256, 512 and 1,024 m. The test (`an_endless_volume_is_seen_past_its_reach_in_coarser_rings`) draws
the same hills in rings and in full detail to 192 m: 8×8 blocks of ground differ by 12 levels on average, mostly the
trees' and voxels' own grain.

- **Not done.** Far rings are made again as they come into view, not kept on disk. The frame with the land to the horizon is three times today's view of sky;
  the far crowns' rays are most of the depth pass. The finest ring was at most 16 sectors tall (64 m at 16 voxels a
  metre), so hills taller than it were cut flat near the camera and whole further out; it may now be 1,024 m tall
  (below).

## Ground hundreds of metres tall (2026-11-26)

An endless volume was at most 16 sectors tall: 64 m at 16 voxels a metre. The Realm, the first game's world of
51 km painted with land, runs from 54 m under its sea to summits of 655 m, with cliffs that rise 140 m in 64 m.

- **Measured first, on the CPU, over the whole world.** Its heights every 64 m: 54 m under the sea to 576 m (655 m
  sampled finer). Over 2 km round its highest ground, each 4 m column's ground, sampled every half metre with its
  border, spans one sector in a quarter of the columns, two in half, and at most 14 (56 m) on the steepest faces.
  A column's ground is a thin band however high it is: keeping every sector of a column 640 m tall would spend the
  window's table on air.
- **Each column keeps a band of 24 sectors** (96 m at 16 voxels a metre), from the lowest that holds anything up,
  or down from the highest where it holds more. The window's table has 24 layers in place of one a sector, and a
  sector's entry goes in the layer its height wraps to. The walk already checks the world place an entry names, so
  a sector outside its column's band reads as empty, as one not yet made does. A volume may now be 256 sectors tall,
  1,024 m at 16 voxels a metre; a volume 24 sectors tall or less is kept as before.
- **The walk stops at the highest ground made.** Each column notes its highest sector; the walk's box is as tall as
  the highest any column has noted, read back from the GPU, so a tall volume of low ground walks no more air.
- **What a height costs** (Rolling Hills with oaks and pines, reach 64 m, 1920×1080, the least of 60 frames):

  | Tall | On the GPU | Frame | Making (median) |
  |---|---|---|---|
  | 48 m | 120 MB | 10.30 ms | 1.39 ms |
  | 128 m | 124 MB | 10.43 ms | 1.81 ms |
  | 320 m | 125 MB | 10.39 ms | 1.64 ms |
  | 512 m | 127 MB | 10.40 ms | 1.73 ms |

  The few MB are the making passes' lists of bricks by row, which still go a whole column up; the GPU was shared with
  other sessions, so making times move by a few tenths either way.
- **The CPU leaves out a Volume the GPU leaves out.** The Realm's generator has overhangs (Bridged, Over, Towered)
  the GPU cannot run, so its ground is the height made solid there; the CPU still ran them for collision and rays,
  and met a summit six voxels under the one drawn. It now asks whether the GPU runs the Volume, kept for each
  generator. On the Realm's ground, the GPU's top voxel, the CPU's collision and its ray meet at the same voxel on
  the highest summit (715.6 m over a floor at -60 m) and at the players' start; on a river's cliff one voxel apart.
- **Checked** by a test of a hill painted 360 m high in a volume 400 m tall: every voxel the CPU has open to the air
  near the summit and halfway up is on the GPU, the GPU has none where the CPU has air, and the CPU's ray meets the
  summit.
- **Not done:** a column whose ground, trees and buildings rise more than 96 m within its 4 m loses the lowest of
  it. The making passes' lists of bricks by row still go a whole column up.

## Stage 7 on: cutting grass and felling trees (2026-11-26)

Cutting for the RPG's world: grass, cover and crops a blade passes through, and trees an axe fells.

- **Cut grass is a byte a column** (`voxel_volume/changes.rs`, `CutColumns`): how much of its blades is left, kept
  eight by eight columns with a bit each for cut and for still fading. The engine moves them on in eight steps over
  0.6 s and grows them back in sixteen over the volume's Regrow seconds; each step makes again only those columns,
  not the trees round them. On the GPU a hash table of sector columns among the edits' words holds 4 KB a sector
  with any cut, and the tops pass scales each column's blade by what is left (`cutLeft`); rings look up the finest
  ring's columns. Rays can meet cover (`VoxelRayMeets::Cover`) as the layer it grows in, less what was cut.
- **What comes loose is found by logs, not by tree** (`voxel_volume/fell.rs`). A plant's logs are round and centred
  in their 1 m blocks, so logs of blocks that meet only along an edge or turn a corner do not touch as voxels: a
  first try joining log voxels gave 31 pieces for one cut tree. Joins follow the plant's blocks instead: each block's
  logs left are split into pieces that touch, a piece joins a neighbour block when it reaches the block's end that
  way (or its middle, for a block beside it), and logs of different trees join where they touch. What is joined to a
  log resting on something solid holds; the rest comes loose, with each brick of leaves going with its nearest log,
  or all the leaves over the stump of a tree cut through its trunk.
- **A felled tree is a few bytes.** Where everything of a tree over a height comes loose, it is kept as felled by its
  foot's column with that height (`Felled`); the trees pass gives it its stump's height and the bricks pass leaves out
  its voxels over that, and stops naming its plant's bricks above the cut. Only the kerf, and what a branch or a tree
  held by a neighbour loses, is dug a voxel at a time. A trunk cut by a slice: 576 voxels dug, 137 bytes of region
  file. On the GPU and the CPU the same: 1,148 of 1,148 voxels up 14 felled trunks agree; the crowns' pixels in the
  view went from 48,535 to 3,064.
- **Pieces fall as bodies of voxels.** Collision boxes of the logs, joined along X, Z and Y, at most 192, a voxel or
  two or four at a time (a new compound shape in the physics library). Its voxels are a Voxel Volume of a set size
  on an Obj under the body, which turns with it: Voxel Volumes now turn with their Obj, each volume's record
  carrying a quaternion, each walk turning the ray into the volume's own frame and the face met back out (the depth,
  lit, glass and shadow walks, sway, and the light kept round the camera). Drawn first as box-model cubes, a whole
  crown a cube a voxel was 809,000 vertices and ran the frame out of mesh space, and looked unlike the ground; as a
  volume it is the ground's own look. The light kept round the camera took all past a volume's box as solid, which
  blacked out the wood round a small floating volume: that is now only an endless one's window. A piece waits until
  collision round it is made again without it (about a second, in the background), then tips about its stump; it
  rests after a second lying still. A stamped piece's volume lingers a second while the ground is made again.
- **Falling like a tree.** Spun over hard, a felled tree rocked back onto its stump and bounced. A hinge on the
  stump's edge, letting go most of the way over, kept it from kicking back but felt held. Now it is given one slight
  push about the edge of its foot the way it was cut (0.6 rad/s, its middle's speed set to match the turn), and
  gravity falls it: it leans slowly, then falls. A cushion of leaf boxes jammed the crown into its neighbours' branches
  and held it up: leaves pass through, and are pressed into a layer against the ground where it rests.
- **Grass cut, not faded.** A cut column keeps how many voxels of blade are under the cut; the tops pass keeps the
  blades to that and drops heads and flowers. Clippings (a new built-in effect) are played at a place in each
  brick's column cut.
- **Measured** (development build, shared CPU): felling a 10 m oak takes 0.14 s on the CPU, most of it working out
  its blocks' pieces and leaves; first versions took 0.45 s, before the pieces were found on each block's own grid
  and trees' logs were looked for only near the cut trees.
- **Not yet:** felling off the main thread; pieces lying as bodies kept by a save; leaves of a felled piece in the
  volume's own look while it falls.

## Stage 7 on: buildings that come loose and give way (2026-11-26)

What is built in an endless volume now falls when its supports are cut, and gives way under its own weight, in four
stages, each measured before the next.

- **Cut off from what holds** (`voxel_volume/collapse.rs`). Every voxel an edit fills, a print prints or a game places
  is built; the ground as made holds, and so does earth filled or placed untinted while the volume's **Anchor** is
  Earth (Ground: the ground alone; Everything: nothing comes loose). `Ground::made_in` says what made each voxel of
  a box, the same pass `VoxelSolids::fill` makes, and a test checks the two agree voxel for voxel. The check starts at
  the voxels just dug: their built neighbours are seeds. It goes over half-metre bricks first, the bricks whose built
  voxels touch across faces, finding which touch what holds and how far each brick is from one; then over voxels,
  nearest first by those distances, stopping as soon as it meets what holds. A brick part with nothing rooted comes
  loose whole without the voxel search finding its way. A wall over a breach cut at both ends: 56 bricks and 3,456
  voxels looked at, 9 ms (development build). Under 40 voxels is crumbs; a piece's voxels keep their colours,
  pavings laid as the GPU lays them (`pavedVoxel` ported to `paved_place`), so rubble looks like the wall it was.
- **Under its own weight** (`voxel_volume/strain.rs`). The built voxels round a change, at most 4,000 bricks, become a
  graph: a node per brick (its mass from what it is made of: stone 2,400 kg/m³, brick 1,900, wood 600, earth 1,600,
  glass 2,500, its middle of mass), a join where neighbouring bricks' voxels touch (as many faces, their middle, the
  weaker brick's stuff), and held joins to what holds and to built bricks past the 4,000. Two ways of finding the
  loads were built and measured:
  - *Solved*: each brick a rigid body with six ways to move, each join a spring at its middle, along and across it
    and turning about and along it (stiffness from the stuff, the area and its second moment). The matrix is kept as
    an envelope in reversed Cuthill–McKee order and factored L Lᵀ; a broken join or a brick come loose changes only
    rows from its first brick on, and only those are factored again (847 of 1,632 rows a round on average).
  - *Paths*: each brick's weight goes down the cheapest way to what holds (down 2, across 3, up 6), shared among the
    joins that way by their faces, its moment the carried weight times how far its middle is across from the join.
  Each join's load is split into pull or push, shear and bending moment. It breaks where any has grown over what it
  bore at rest by more than the weaker stuff bears (pull, push, shear, bend, a few kilopascals for earth to tens for
  stone and wood): the baseline is kept per join (`VoxelStrainMemory`), so whatever stands at rest stands (a test
  checks a house at rest breaks nothing by either way), and a join that bears less than before is none the worse. A
  join pushed together bears more shear (friction, 0.6) and more bending (until what pushes it leaves its middle
  third): without that, walls whose roof fell broke at their feet and the whole house came loose standing up. Then
  what is joined to nothing comes loose, split along the tenth of its joins nearest breaking, and the loads are found
  again, round after round, the worst joins (all over half the worst) breaking each round.
  - **Measured** (development build, shared CPU). A house of 1,068 bricks, 2,158 joins: paths 0.16 ms, solve 90 ms.
    A 5 × 4 m brick house with a plank roof, its corner blown out by a 3.6 m ball: 241 bricks, 476 joins, 13 rounds;
    Solved breaks 72 joins and lets 12,876 voxels go in 9 releases, 148 ms all told, 35 ms of it solving; Paths 50
    joins, 13,166 voxels in 3 releases, 107 ms. Before friction was added, Paths brought the whole house down
    where the solve brought down the corner; with it they agree on a blown corner, but Paths never shares a load
    between two supports. **Picked: Solved**, run on a thread of its own per volume (`voxel_collapse.rs` in the
    engine), so the cost is a short wait before the first round, not a frame; Paths stays a setting for many
    buildings at once.
  - A 2.6 m ball low on the wall breaks nothing: a band of wall survives over the hole and carries the roof like a
    lintel, which is right. The shot used a 3.6 m ball high on the corner.
- **Falling and many pieces** (engine `voxel_collapse.rs`, reusing felling's bodies). A blast digs at once and raises
  dust; the volume's thread works out the rest; what was cut off is thrown away from the blast, and each round of what
  gave way comes loose 0.22 s after the last, tipping over the edge of its foot toward the damage, so the house sags
  and falls in. Pieces that slow by 3.5 m/s and lose 1.5 kJ dig out what they hit (a ball of (energy / 60 kJ)^⅓, 0.15
  to 0.6 m) and settle that in turn; pieces meeting each other chip. A piece under 320 voxels is dust, no body;
  past the volume's **Most Pieces** (24; a view draws 64 volumes) the smallest falling turn to dust. A piece lying
  still a second is stamped back into the volume where it lies: it is built again, rubble that can be blown in turn.
  Dust is the built-in Dust effect sent out for 0.8 s, at most 5 clouds a change (24 hid the house).
- **CPU and GPU.** What comes loose is dug a voxel at a time into the changes both make the ground from. GPU test
  `a_house_blown_in_is_drawn_as_the_cpu_finds_it_after_what_came_loose_was_dug_out`: a house on hills blown in and
  every round dug, 0 of 33,536 surface voxels differ (the hills' own known one-voxel differences aside).

### A collapse over the network: a design

What a collapse needs to send is small, because nearly all of it is worked out the same way on every machine; only
the bodies' falls are not, as the physics library isn't bit-for-bit alike across machines.

- **What is the same everywhere.** A blast goes out as an edit does (its middle, its radius, its number in the
  volume's list): every machine digs the same voxels, and works out the same settle from the same volume and the same
  kept loads at rest. Each piece is then named by the blast's number, its round and its place in the round, and
  every machine makes the same piece from the same voxels. Nothing about a piece's shape is sent. The server is the
  one whose order counts: a client's blast is a request, which the server numbers and sends on; a client may dig its
  own at once and fix it up if the server's order differs.
- **What the server sends.** The server runs the bodies, and twenty times a second sends a snapshot of the pieces
  that moved, each against the last snapshot that client said it had: the piece's number, how far it moved since in
  whole millimetres (each axis a few bits, sized to the number), and where it turned, its turn as the three smallest
  parts of its quaternion in 11 bits each and which was left out. A piece new to the client is sent whole. Clients
  draw what they are sent, a snapshot behind, between the last two.
- **Rest.** A piece's rest is the server's: it sends the piece's number and its last pose, as quantised, and every
  machine stamps the piece there, the server too from the same quantised pose, so the voxels placed are the same
  everywhere without being sent (12 bytes a piece).
- **Measured** (engine test `measure_how_many_bytes_a_house_falling_in_costs_over_the_network`, the same house and
  blast, a snapshot every 3 steps, answers 100 ms behind): 6 pieces falling at most, all stamped; 857 to 1,287 bytes
  for the whole collapse over three runs, at most 57 bytes in one snapshot, 340 to 430 bytes a second while pieces
  fell. The blast itself is an edit of about 20 bytes. A player joining after gets the region files' changed bricks,
  as for any other dig.
- Not built: the messages, ordering the blasts and the stamps among other edits, and pieces chipping each other the
  same everywhere (chips are the server's alone in this design, and would go as small digs of the piece).



- The prototype's output as printed, the reference pictures and the prototype beside the meadow reference are in
  the shared notes of the sessions that did this research (`small-voxels.md` and its folder).
