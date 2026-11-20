# Vegetation: dense plants that stay made of cubes

Research for drawing dense vegetation in Voxear's worlds of blocks while keeping the look of small cubes: grass,
flowers, crops, ferns, bushes, forests and undergrowth, with a great deal of it on screen. Written 2026-11-20.
It covers:

- what the engine does now and where it stops (measured);
- the published techniques and what each costs;
- how textured, cut-out cards compare;
- a recommended plan in stages.

**Progress.** Stages 1 and 2 are built (2026-11-20). Plants of cubes have coarser LODs of their own cubes halved, and
lean as one sheared solid while each blade sways on its own, so their faces are merged up their height again. Three
tufts a block to 60 m (7,430 on screen) went from 3.67 to 2.10 ms, and 42,900 tufts from 18.8 to 8.9 ms, about
0.2–0.3 ms per 1,000 against 0.45–0.5. That is short of the 1.5 ms stage 1 aimed at: LODs taking over sooner (cubes
ten pixels across) reached 1.89 ms but looked chunkier in the middle distance. The roadmap's Vegetation entry has
the details.

## The short version

- **The voxel look can stay.** Nothing measured here says cubes are too costly as such. What makes them costly
  today is that every plant is drawn at full detail at every distance, as a scene instance of its own. Both of
  those can change without giving up a single cube near the camera.
- **Today a grass tuft costs about 0.5 ms of GPU per 1,000 on screen at 1080p, and the cost is geometry, not
  pixels.**
  - CubeGame's meadow as it ships draws 1,700 plants for about 1 ms.
  - Three tufts a block to 60 m is 7,400 tufts on screen and 3.7 ms.
  - At 720p the same tufts cost 9% less, though there are 2.25 times fewer pixels.
- **What breaks first, in order:**
  1. **The GPU's triangle work.** A tuft is about 10 meshlets of small cube faces (up to about 300 triangles)
     at every distance. At 60 m one of its cubes is two pixels across.
  2. **The renderer's CPU.** It costs about 0.3 µs a plant, so 62,000 plants take 19 ms more.
  3. **The cap of 65,536 instances a view.** Every plant is one, alongside everything else in the scene.

  The shared pool of 8M vertices is not a limit for plants: a Foliage draws one mesh many times.
- **Shadows on small plants double their CPU cost.** Shadowing 7,400 tufts added 0.8 ms of GPU and 1 ms of
  CPU. Chunks that cast shadows are not culled against the view on the CPU, so the instances went from 11,300 to
  26,600.
- **The recommended path, in short:**
  - Blocky LODs for box-model plants.
  - Wind that sways a plant's parts whole, so its faces can be merged again.
  - Grass and ground cover made on the GPU for each chunk of a world of blocks, with nothing stored and no
    instance per plant.
  - Cheaper instances for the plants that are placed.
  - Voxel impostors in the middle distance.
  - A far tier drawn as the ground block's own small-cube relief.
- **Textured cards stay the fallback, not the plan.** They are cheaper in triangles, but they look flat and
  soft against the cubes. Cut-outs also turn off the early depth test, costing overdraw in a dense field. The
  engine already has blocky cut-out sheets (leaf tufts and vines). They make a good far tier to compare against,
  if the cube relief does not read well enough.

**Numbers to aim for (RTX 3070, 1920×1080, release build, whole vegetation budget 3 ms GPU and 1 ms CPU):**

| What | Where | How many in view | Budget |
|---|---|---|---|
| Grass and ground cover, cubes | To 100 m, three clumps a block near, thinning out | about 10,000 clumps | ≤ 1.0 ms GPU, ≤ 0.1 ms CPU |
| Flowers, ferns, crops, small plants | To 80 m | 10,000 | ≤ 0.6 ms GPU |
| Bushes and undergrowth | To 150 m, voxel impostors past 40 m | 3,000 | ≤ 0.5 ms GPU |
| Box-model trees and big plants | To the far view, impostors past 60 m | 2,000 | ≤ 0.6 ms GPU |
| Shadows of all of it | Tiered | — | ≤ 0.3 ms GPU |
| Far grass | 100 m to the far view | the ground's own look | ≤ 0.1 ms GPU |

Today that field of 10,000 clumps alone would cost about 5 ms of GPU and 3 ms of CPU, and the plants placed by
hand would share its instance cap.

## What the engine does now

### Foliage (`voxear_render::foliage`)

- **A Foliage is one plant, many times.** It is one mesh and material with every place it grows, placed or
  scattered. Each view builds a Foliage's instances once, in 16 m chunks. Each frame it adds the chunks within
  the draw distance to the frame's instances.
- **Each plant is a whole scene instance:** a `GpuInstance` of 288 bytes, holding this frame's and last frame's
  matrices, a normal matrix, LOD settings, fade and tint. That is the same record a character or a door has.
  - The GPU culls each one against the view and last frame's depth.
  - It picks the plant's LOD and culls its meshlets.
  - It moves its vertices in the wind and by pushes.
- **Fade and shadows.** Plants fade out by dithering over the last `fade_distance` metres before their draw
  distance. Chunks past `shadow_distance` cast no shadow.
  - A chunk that casts no shadow and lies wholly outside the view is left out on the CPU.
  - A chunk that casts shadows is always sent, so the GPU culls it twice.
- **The cap is 65,536 instances a view** (`MAX_INSTANCES`), for everything in the scene together. Past it, the
  rest are left out with a warning.
- **A second cap is the meshlets.** A view whose instances could need more than 1,048,560 meshlets
  (`MAX_MESHLET_CAPACITY`) draws whole LODs without meshlet culling. With about 10 meshlets a tuft, that is about
  100,000 tufts, or 40,000 bushes of about 26 each.
- **Impostors** (`impostors.rs`): far plants 2 m across or larger become octahedral pictures on a quad, from 40 m
  or a third of the draw distance. Grass, flowers and bushes are under the size, and CubeGame turns them off.

### Box-model plants (`box_model/plants.rs`, `box_model/voxels.rs`)

- **What a plant is.** Every starter plant is a box model of one bone and one box sculpted from small cubes:
  - 8 cubes to a metre for tufts, flowers and bushes;
  - 16 for the smallest (sticks, pebbles);
  - 4 for trees and boulders.

  A sculpted box can now be up to 256 cubes a side.
- **How it is meshed.** Only faces that show are kept, and faces of one colour side by side are merged.
- **A plant that bends in the wind keeps each face one cube tall** (`wind_bend > 0`). The wind bends a plant by
  the square of its height, so a taller face would bend straight and crack from its neighbours.
  - So a tuft of ten blades is meshed as one face per cube up each blade's sides, about 140 faces.
  - Meshlets hold at most 64 vertices and 124 triangles. A face has four corners of its own, so a meshlet of
    cube faces is full at 16 faces (32 triangles), a quarter of its triangle room.
- **A box model has only its full LOD** (`full_lod`), plus a flat one for raised layers. Plants have no coarser
  LOD, so:
  - LOD selection, the LOD fade by dithering, and the coarser LOD that shadows of plants pick
    (`PLANT_SHADOW_COARSER`) do nothing for them;
  - a tuft at 60 m draws the same 140 faces as at 2 m.

### Plants a Block World grows (`voxear_engine::grown_plants`, `voxear_terrain::blocks::grow`)

- **Where they come from.** A Foliage under a Block World with a `grow` rule is grown from the ground column by
  column, on an Obj of its own per column. It is worked out on every machine from the blocks, so nothing is
  saved or sent.
- **How fast and how dense.** Eight columns are made a tick, nearest first. At most one plant of a rule grows
  per block (`spacing` is at least 1 m). A denser field needs several rules.
- **CubeGame and the template grow four:**

  | Plant | Spacing | Chance | Draw distance | Shadows |
  |---|---|---|---|---|
  | Grass Tuft | 1 m | 0.6 | 60 m | none |
  | Flower | 2 m | 0.5 | 60 m | none |
  | Bush | 7 m | 0.4 | 100 m | to 30 m |
  | Rock | 9 m | 0.35 | 120 m | to 40 m |

- **Trees are different.** They are `.voxplant`s built into the world's blocks: logs and leaf blocks. They cost
  what blocks cost, in the chunk passes.

### Plants drawn by the blocks themselves

- **Leaf blocks.** They can sway: their free corners move with the gust. A face of leaves with no picture is drawn
  as small leaves in three shades.
- **Bushy blocks.** These get three crossed sheets. A sheet is cut into cells as many to a block as its picture
  has pixels, thinned by a hash towards its edges (`blockSheetLook`). That is a cut-out card drawn to look
  blocky, and it is cheap: two words a sheet in the chunk's quads. Tufts shrink away over the last quarter of
  160 blocks.
- **Ground relief.** A world's faces can be drawn as small cubes (`BlockVoxels`, `blockCubes`): the camera's ray
  is walked across a grid of small cubes stepped by the picture. This adds no geometry and no depth.

### Grass on smooth terrain (`grass.rs`, `shaders/grass.slang`)

- **How it works.** It is scattered on the GPU round each view's camera every frame, on a grid fixed to the
  world, one blade at most a cell. It is drawn by indirect draws per kind and LOD ring, with no scene instance
  per blade, and there is room for a million blades a view.
- **What it does.** It sways, bends from pushes, lies flat where a Track Maker trod, and casts no shadow.
- **It exists only for smooth terrain.** Worlds of blocks have no equivalent. This is the most useful piece
  already in the engine.

### Wind and pushes (`wind.slang`, `FoliageMotion`)

Plants bend by height from their foot, lean on an arc, sway at their own phase and read the push grid. Box
models bend whole, with no flutter, which would part their cubes' faces.

### Culling and mesh shaders

- **Culling.** It is two-phase against a depth pyramid: per instance, then per meshlet (frustum, cone, depth).
  Mesh shaders do the meshlet pass where present (they are on the RTX 3070), and a compute pass does it
  elsewhere.
- **Shadows.** They reuse it per cascade or per virtual page. Plants are left out of the GI's rays.

## Measured limits

**How these were measured:**
- **Hardware and build.** RTX 3070, shared with other sessions' builds and tests. Release build of `vox`.
- **What was drawn.** `vox render` at 1920×1080, 360 frames, the least frame and each pass's least over the
  last 60.
- **The scene.** A copy of CubeGame (its link file removed), Infinite World level, the player moved to a meadow
  in a blossom grove so the game's own follow camera looks over grass, trees, pets and water.
- **The variants.** They changed only the scene's Foliage: tufts at chance 1.0, copied with seeds of their own
  and `aligned = false` to grow two or three a block.
- **Noise.** Whole frames moved by about ±0.5 ms run to run, so the pass rows are the ones to read.

### Grass tufts

| Variant | Tufts on screen | Instances | Depth (early) | Opaque (early) | Plant cost | Frame, least | Renderer CPU |
|---|---|---|---|---|---|---|---|
| No plants | 0 | 69 | 0.22 ms | 0.46 ms | — | 9.4 ms | 7.1 ms |
| As shipped (tuft, flower, bush, rock) | 1,670 | 2,590 | 0.45 | 1.20 | 0.97 ms | 10.2 | 6.7 |
| One tuft a block, 60 m | 2,480 | 3,820 | 0.56 | 1.53 | 1.41 | 10.5 | 8.3 |
| Three a block, 60 m | 7,410 | 11,320 | 1.23 | 3.12 | 3.67 | 12.9 | 10.7 |
| Three a block, 90 m | 10,750 | 15,590 | 1.63 | 4.06 | 5.01 | 14.5 | 11.5 |
| Twelve a block, 90 m | 42,810 | 62,160 | 5.55 | 13.94 | 18.81 | 28.7 | 25.8 |
| Three a block, 60 m, at 720p | 7,410 | 11,320 | 1.17 | 2.85 | 3.35 | 11.4 | 8.6 |

"Plant cost" is depth plus shading of meshes, less the run with no plants. Meshlets drawn went from 13,242 with no
plants to 85,092 with three a block (about 9.7 a tuft) and 433,731 with twelve.

**What the numbers say:**

- **About 0.45–0.5 ms of GPU per 1,000 tufts on screen,** in step with the count from 2,500 to 43,000. Depth
  takes a third and shading two thirds.
- **It is geometry, not pixels.**
  - At 720p the cost fell 9% for 56% fewer pixels.
  - Far tufts are cubes of one or two pixels: every face is a pair of tiny triangles that shades a whole 2×2
    block of pixels for one, through both passes.
  - Twelve a block draws up to about 14 million triangles a frame.
- **The renderer's CPU grows about 0.3 µs a plant.** That covers copying 288-byte instances, ordering and
  uploading them. On top of it, gathering a Foliage's chunks costs 0.05 µs a plant: 3.4 ms at 62,000.
- **The instance cap came next.** 62,160 of 65,536 instances at twelve a block, nearly all of them tufts.
- **Vertex room did not move:** models 1.19M of the 8.39M in every run. Plants are one mesh drawn many times.
- **The meshlet cap was not reached** (434,000 of 1,048,560), but bushes would reach it at about 40,000.

### Shadows and bushes

| Variant | On screen | Instances | Shadows (meshes) | Depth + opaque (meshes) | Foliage CPU | Frame, least |
|---|---|---|---|---|---|---|
| Three tufts a block, no shadows | 7,410 | 11,320 | 0.38 ms | 4.35 ms | 0.28 ms | 12.9 ms |
| The same, shadows to 60 m | 7,410 | 26,630 | 1.17 | 4.32 | 1.30 | 13.9 |
| Bushes 7.5 times as thick, shadows to 30 m | 260 | 500 | 0.32 | 1.13 | — | 9.8 |
| The same, no shadows | 260 | 370 | 0.29 | 1.11 | — | 9.5 |

- **Shadows on 7,400 tufts cost 0.8 ms of GPU and 1 ms of CPU.** Instances more than doubled: every chunk that
  casts within 60 m is sent, in view or not.
- **A bush is about 26 meshlets.** 220 more bushes added about 0.35 ms of shading, about 1.6 ms per 1,000:
  three times a tuft. Their shadows to 30 m were cheap here, as few were near.
- **The blocks themselves cost 5.5 ms of this frame:**
  - the blocks' sun shadows 2.6 ms, under a grove of block trees;
  - depth 1.1 ms;
  - shading 1.8 ms.

  Forests of block trees are paid for in the blocks' passes, not in Foliage.

## The options

### 1. Blocky LODs for box-model plants

- **What it is.** Build each plant's coarser LODs from its cubes rather than its triangles:
  - halve the grid (8 to 4 to 2 cubes a metre), each new cube filled where enough of its eight were, taking
    their most common colour;
  - mesh it the same way;
  - at the coarsest, one box per blade or clump.

  General triangle simplification would round the cubes off. Halving the grid keeps them square.
- **What it costs and saves.**
  - Each level has about a quarter of the faces.
  - The LOD machinery already in place does the rest: screen-space error, the dithered LOD fade, and the
    plants' coarser shadow LOD.
  - Within a 60 m circle three quarters of the plants are past 30 m, so most tufts would draw a half or a
    quarter of their faces.
- **Expected.** Tufts from 0.5 to about 0.15–0.2 ms per 1,000.
- **What it leaves.** The instance per plant and its CPU stay. It helps every placed plant too (crops, bushes,
  rocks, box-model trees), so it is the first thing to do whatever else is chosen.

### 2. Faces merged again: wind that moves parts whole

- **The cause.** Today's bend by height forces faces one cube tall on any plant that bends: about 140 faces for
  a tuft where merged faces would be about 50–90.
- **The alternative.** Sway each part rigidly about its own foot, as stems are swayed about pivots in published
  vegetation systems (a pivot and a parent per part). The part can be a blade, a stalk, a branch or a crown.
  - A blade turns whole and stays a straight column of cubes, which reads as voxel art rather than bent rubber.
  - Its faces can be merged up its height.
  - Parts are already a box model's boxes and bones. A plant that sways its parts stores a pivot per part, and
    the shader rotates about it.
- **The trade.** Big plants lose their smooth arc, but voxel plants never had a smooth arc to lose. A crown that
  sways whole over a trunk that leans slightly reads well in cube art.
- **Expected.** Two to three times fewer faces again on bending plants. Wind looks stepped and chunky.

### 3. Grass and ground cover made on the GPU per chunk, nothing stored

- **What it is.** The smooth terrain's grass, done for worlds of blocks.
  - A compute pass (or a task shader) walks the block tops round the camera on a grid fixed to the world.
  - It hashes each cell, and decides from the top block and the biome which kind of clump grows there and how
    big.
  - It writes compact records (position, kind, yaw, size, tint: 16 bytes) into runs per kind and LOD ring,
    drawn by indirect draws or mesh shaders.
- **What it drops.** No scene instance, no CPU per plant and no 64K cap. Grass changes with the blocks the
  frame they change, without remaking columns of plants, and nothing is saved or sent.
- **What it needs.**
  - The chunk's top heights, which the mesher and the grown plants already read. A small texture of each
    column's top block and height round the camera, kept like the push grid, would let the shader read them.
  - Templates: a clump kind is a small voxel model (a starter tuft, fern, flower or crop stage) with blocky LODs
    from option 1.
  - Thinning by distance, as published grass systems do. Each ring out keeps a quarter of the clumps and makes
    them twice as wide, or swaps them for the next LOD, so coverage holds while the count falls.
- **Drawing clumps of cubes directly with mesh shaders.** A meshlet can be written from a list of cubes rather
  than from vertices:
  - each cube is a place and a colour, and the shader emits only the up to three faces turned towards the camera;
  - that halves the triangles of a closed cube, and needs no vertex buffer at all;
  - a 16-cube blade as one 1×n×1 box is three faces from any side.
- **How far it scales.** Published GPU grass draws 80,000 blades in about 2.5 ms on a console from a decade
  ago, at 7–15 vertices a blade; one mesh-shader sample draws 32 blades a workgroup. Clumps with LODs:
  - within 20 m at three a block, about 900 clumps of 140 faces;
  - 20–50 m at two a block, about 3,300 of 40 faces;
  - 50–100 m at one a block, about 6,000 boxes of 3 faces;
  - about 300,000 faces in view: a seventh of today's three-tufts-a-block at 60 m, going half as far again.
- **Expected.** 10,000 clumps to 100 m at about 1 ms, and almost nothing on the CPU.

### 4. Cheaper instances for placed plants

- **What it is for.** Plants a script or a rule places one by one (crops in a field, a hedge, saplings, a
  forest of box-model trees) still want to be kept and picked one at a time.
- **What changes.**
  - Keep them as compact plant records (position, yaw, scale, tint, mesh: 32 bytes) uploaded when a chunk
    changes rather than every frame, and expanded on the GPU into the draw lists.
  - Shadow-casting chunks get culled against the view expanded by the sun's direction, as cascades do.
  - Plants get a list and a cap of their own apart from the scene's.
- **Expected.** The 0.3 µs a plant drops to a few hundredths. 60,000 placed plants under 1 ms of renderer CPU.

### 5. Voxel impostors for bushes and trees in the middle distance

- **Why pictures are wrong here.** The engine's impostors are flat pictures from a few dozen directions. Past
  the point where a cube is under a pixel they are right. Nearer, they blur the cubes and swim as the view turns.
- **What a voxel impostor is.** A box drawn round the plant, in whose pixels the camera's ray is walked through
  the plant's cubes kept as a small 3D texture: a byte a cube, palette colours, a coarser level or two to skip
  empty space.
  - It is exact cubes at any angle, from 12 triangles.
  - It costs per pixel rather than per cube, so it pays from the distance where a plant covers fewer pixels than
    it has faces. That is about 30–40 m for a bush, nearer for a tree.
  - Depth is written from the hit, so it occludes and casts shadows correctly when drawn into the shadow maps
    the same way.
  - A tree of 4 cubes a metre, 18×38×18, is 12 KB. Every starter plant together is well under a megabyte.
- **What it costs.** Pixel shader time with divergent loops; the bound must be tight; wind moves the box whole.
- **Expected.** Bushes and box-model trees from 40 m at roughly constant cost per pixel covered, with the
  existing picture impostors kept for the far view.

### 6. Dithered fades and distance tiers

These exist: plant fade by distance, LOD fade, impostors. What is missing is the tiers' content: LODs (option 1),
thinning (option 3) and a far grass look (option 8). Dithered transitions stay, as they need no sorting and keep
everything opaque.

### 7. Cheaper shadows for plants

- **Small plants cast no shadow map.** That is the published practice and CubeGame's own setting. In its place:
  - **The ground under grass is darkened** by how dense and tall the grass growing there is, in the block's
    corner shade, which costs nothing per plant.
  - **The screen-space contact shadows pass** (0.06 ms here) catches clumps shading their neighbours and their
    own feet near the camera.
- **Bushes and box-model plants cast from their coarsest LOD** (`PLANT_SHADOW_COARSER` begins to work once
  option 1 gives them LODs), from their voxel impostor past 40 m, and not past their shadow distance.
- **Shadow chunks are culled on the CPU** (option 4), which was half the measured cost.
- **The blocks' shadows are the bigger bill** in a forest of block trees (2.6 ms here, against 0.3 ms for every
  mesh's). How much of that the swaying leaf blocks cause, against the same forest held still, was not
  measured, and is worth measuring before anything is built for it.

### 8. A far tier for grass

Past 100 m a clump is a pixel or less. Two ways keep the cubes' look there:

- **The ground block's own relief.** `blockCubes` already draws a block's top as small cubes stepped by its
  picture. Raised and tinted by the grass density there (from the same hash the clumps use), it reads as a
  field of cube grass, and costs nothing past what the ground costs.
- **Blocky cut-out sheets.** These are crossed sheets cut into pixel-sized cells, as the bushy blocks are. They
  are the cards option done in the blocks' own style, and the fallback if the relief is too flat at a glancing
  angle.

### 9. Textured cards and alpha-tested foliage (the comparison)

| | Voxel plants today | Voxel plants with options 1–3 | Textured cut-out cards |
|---|---|---|---|
| Triangles a clump | about 280, at every distance | 6–280 by distance | 4–8 |
| Pixel cost | Opaque; early depth and Hi-Z cull work | The same | Cut-out turns off early depth; forests of cards overdraw 8–15 times without a depth prepass. The engine has a prepass, so most of that becomes depth-pass cost |
| Small-triangle waste | High: one- and two-pixel cubes | Low: LODs keep faces a few pixels | Low |
| Look against blocks | Matches | Matches | Soft, flat, turns with the view; reads as another game's art unless drawn as pixel cut-outs |
| Wind | Whole plant bends | Parts sway whole, stepped | Any per-vertex bend, smooth |
| Shadows | Full geometry or none | Coarse LOD or impostor | Cut-out in the shadow pass, which is costly |
| Pushes, tracks, seasons, tint | Yes | Yes | Yes |
| Making new plants | Sculpt cubes in the Model window | The same; LODs are made | Needs painted textures with alpha, a new art pipeline |

Cards would cut today's triangles by 30–70 times. But they would make the game's plants the one thing in it not
made of cubes, and they need an art pipeline the game does not have. Options 1–3 reach the same budget with
cubes, by working less on plants far away rather than drawing them differently. Cards stay as:
- the far tier (option 8);
- the fallback for any one plant kind that cannot be made cheap enough as cubes.

## Recommendation

**Keep the voxel look.** Build in this order:

1. Blocky LODs for box-model plants.
2. Wind that sways parts whole, so faces merge again.
3. Grass and ground cover made on the GPU per chunk in worlds of blocks.
4. Cheaper instances for placed plants.
5. Voxel impostors for bushes and trees.
6. Plant shadows by tier.
7. A far tier for grass.

Each stage stands alone and shows in a stress scene.

**The targets** are in the table at the top: grass, ground cover, small plants, bushes and trees together in
3 ms of GPU and 1 ms of renderer CPU at 1080p on an RTX 3070. Today the same view would cost about 10 ms of GPU
and 3–6 ms of CPU, and would hit the instance cap.

**The easy way and the full way.**
- **The easy way:**
  - A block's **Ground cover** row (the plant, how dense) makes grass and flowers grow on it in every world,
    with tiers, wind, pushes and shadows chosen by Project Settings ▸ Foliage ▸ Quality.
  - Starter plants come with their LODs and voxel impostors made.
- **The full way:**
  - per-kind distances, thinning, LOD sizes and shadow tiers;
  - a Foliage's own LOD bias;
  - C# `Foliage` and `GroundCover` to read and set them;
  - every count and timing as Profiler rows and through `vox editor call` and `vox mcp` (`foliage.stats`,
    `foliage.set`), so a tool or an AI can drive it.

## A staged plan

The stages are sized like the roadmap's other parts. Each ends with tests, a manual page and its timings in the
meadow of a CubeGame copy at 1080p.

1. **Blocky LODs for box-model plants.**
   - **What changes:**
     - A box model with sculpted boxes gets LODs made by halving its grid of cubes, down to a box per part.
     - Meshes are made the same way, cooked with the model.
     - Plants pick them by screen error and fade between them by dithering. Their shadows use the coarser
       LOD.
     - The Model window shows the LODs and their distances. `COOKED_VERSION` goes up.
   - **Tests:**
     - A LOD of a box of one colour is that box.
     - A tuft's LOD faces count falls by about four each level.
     - The meadow's picture at 40 m is compared enlarged with today's.
   - **Done when:** the meadow's 7,400 tufts cost at most 1.5 ms (3.7 today).
2. **Parts that sway whole.**
   - **What changes:**
     - A plant's parts (boxes, or runs of cubes a box splits into: blades, stalks, crowns) sway about their own
       foot, with a pivot each.
     - Merged faces come back on plants that bend.
     - The starter plants are made again with their blades as parts.
     - Pushes bend the part they reach.
   - **Tests:**
     - A swayed blade's cubes stay square and joined.
     - The tuft's faces fall from about 140 to under 90.
     - The wind clips from the foliage motion work are compared.
   - **Done when:** tufts are at most 0.1 ms per 1,000 with stages 1 and 2 together.
3. **Ground cover grown on the GPU in worlds of blocks.**
   - **What changes:**
     - A top-block map round each view's camera.
     - Kinds of clump by block and biome (`.block` **Ground cover**, a biome's list).
     - A scatter on the GPU into runs per kind and ring, thinning outwards, with mesh shaders drawing clumps
       from their cubes where present.
     - Wind, pushes, tracks and seasons as Foliage has them. Picking and mining see nothing, as today's grown
       grass does not collide.
     - Grown Foliage rules for grass move to it. Rules stay for plants that are kept one by one.
   - **Tests:**
     - Grass is where the blocks say and nowhere else.
     - Breaking a block takes its grass the same frame.
     - The same grass on two machines and after a reload.
     - 10,000 clumps to 100 m timed.
   - **Done when:** 10,000 clumps to 100 m at at most 1 ms of GPU and 0.1 ms of CPU.
4. **Cheaper instances for placed plants.**
   - **What changes:**
     - Compact plant records kept on the GPU per Foliage chunk, uploaded on change.
     - Expanded into the draw lists by the culling pass.
     - A plant list and cap of their own.
     - Shadow chunks culled against the view widened along the sun.
   - **Done when:** 60,000 placed plants (a field of crops) at at most 1 ms of renderer CPU, and the 64K scene
     cap untouched by them.
5. **Voxel impostors.**
   - **What changes:**
     - Each plant's cubes kept as a small 3D texture with empty-space levels.
     - A box proxy walked per pixel past an impostor distance, writing depth.
     - The same in the shadow passes.
     - The flat picture impostors stay past where a cube is under a pixel.
   - **Tests:** an impostor covers the same pixels as the plant (as the picture impostors' test does) from
     several sides.
   - **Done when:** 3,000 bushes and 2,000 trees to 150 m at at most 1.1 ms.
6. **Plant shadows by tier.**
   - **What changes:**
     - The ground darkened under ground cover.
     - Contact shadows on by default in worlds of blocks.
     - Casters by LOD and impostor.
     - A shadow budget for vegetation in Project Settings.
   - **Done when:** all plants' shadows at at most 0.3 ms, and the swaying forest's block shadows measured.
7. **Far grass.**
   - **What changes:** grass density raises and tints the ground block's small-cube relief past the clumps.
     Blocky cut-out sheets are built beside it to compare, and the user picks.
   - **Done when:** the edge where the clumps end cannot be seen in a flight over a meadow.
8. **A vegetation stress scene and the vox calls.** This runs alongside the others rather than after them:
   - a meadow, a field of crops, a forest with undergrowth;
   - `foliage.stats` and `foliage.set`;
   - Profiler rows for plants, LODs and impostors drawn.

**Where to start.** Stages 1 and 2 cut today's cost several times for every plant the game already has, with
no new systems. Stage 3 is what makes grass truly dense and is the largest. Stage 4 matters when fields of
crops or hand-placed forests arrive. Stages 5 to 7 come as the far view fills.

## References

Techniques and figures above come from these. They are listed for reading, not as names the engine takes.

- Procedural grass, GPU-generated per tile (GDC 2021): https://gdcvault.com/play/1027033/Advanced-Graphics-Summit-Procedural-Grass
- A write-up of that talk's numbers: https://tigerabrodi.blog/what-we-can-learn-from-grass-in-ghost-of-tsushima-renders
- Grass and trail techniques with their numbers: https://csarko.sh/research/grass-and-trail-realism
- Procedural grass with mesh shaders: https://gpuopen.com/learn/mesh_shaders/mesh_shaders-procedural_grass_rendering/
- A procedural world with work graphs and mesh nodes: https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/
- Six ways to draw grass, compared: https://github.com/daniel-ilett/shaders-6grass
- Vegetation between tech and art (GDC 2018): https://www.gdcvault.com/play/1025066/Between-Tech-and-Art-The
- GPU procedural placement (GDC 2017): https://www.guerrilla-games.com/media/News/Files/GDC2017_VanMuijden_GPUBasedProceduralPlacementInHorizonZeroDawn.pdf
- A tiled vegetation placement system: https://c0de517e.com/015_vegetation_system.htm
- Fast approximate blue noise on the GPU: https://people.engr.tamu.edu/nimak/Data/EGSR12_FastApproximateBlueNoise.pdf
- Introduction to mesh shaders: https://developer.nvidia.com/blog/introduction-turing-mesh-shaders
- Mesh shader performance advice: https://developer.nvidia.com/blog/advanced-api-performance-mesh-shaders/
- Compute rasterization against mesh shaders and indirect draws: https://tellusim.com/compute-raster/
- GPU-driven rendering pipelines (SIGGRAPH 2015): https://advances.realtimerendering.com/s2015/aaltonenhaar_siggraph2015_combined_final_footer_220dpi.pdf
- Optimizing the graphics pipeline with compute (GDC 2016): https://gdcvault.com/play/1023109/Optimizing-the-Graphics-Pipeline-With
- Cubes drawn with no vertex buffer, camera-facing faces only: https://threadreaderapp.com/scrolly/1315982782439591938
- A voxel engine's geometry: packed blocks and crossed-quad plants: https://vkguide.dev/docs/ascendant/ascendant_geometry/
- Quads all the way down, voxel rendering: https://www.calebleak.com/posts/quads-all-the-way-down-voxel-rendering
- Ray–box intersection for dynamic voxels (JCGT 2018): https://jcgt.org/published/0007/03/04/paper.pdf
- A frame of per-object voxel volumes, taken apart: https://acko.net/blog/teardown-frame-teardown/
- Efficient sparse voxel octrees (I3D 2010): https://www.nvidia.com/docs/IO/88889/laine2010i3d_paper.pdf
- Large voxel scenes with sparse DAGs and Hi-Z culling (2025): https://arxiv.org/html/2505.02017v1
- Voxel bricks as far LODs for foliage: https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-foliage
- Octahedral impostors: https://shaderbits.com/blog/octahedral-impostors
- Billboard clouds (SIGGRAPH 2003): https://graphics.cs.yale.edu/publications/billboard-clouds-extreme-model-simplification
- Stochastic transparency (I3D 2010): https://www.cse.chalmers.se/~d00sint/StochasticTransparency_I3D2010.pdf
- Hashed alpha testing (I3D 2017): https://research.nvidia.com/sites/default/files/pubs/2017-02_Hashed-Alpha-Testing/Wyman2017Hashed.pdf
- Foliage overdraw and how it is fought: https://app.cinevva.com/blog/2026-05-11-foliage-overdraw
- Screen-space shadows: https://www.bendstudio.com/blog/inside-bend-screen-space-shadows/
- Vegetation procedural animation and shading, GPU Gems 3 ch. 16: https://developer.nvidia.com/gpugems/gpugems3/part-iii-rendering/chapter-16-vegetation-procedural-animation-and-shading-crysis
- Pivots and hierarchy baked for swaying parts: https://dev.epicgames.com/documentation/unreal-engine/pivot-painter-tool-2.0-in-unreal-engine-?lang=en-US
- Quad-fragment merging, the cost of small triangles: https://www.microsoft.com/en-us/research/?p=159994
- The visibility buffer (JCGT 2013): https://jcgt.org/published/0002/02/04/paper.pdf
- Voxel plant sprites moved to one vertex buffer: https://veloren.net/blog/devblog-125
