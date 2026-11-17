# Shader packs: a plan

A **shader pack** is a set of shader programs that replaces how a game is lit and finished: its light on block faces,
leaves and water, its shadows, its sky, and passes over the whole picture, such as fog, rays through leaves, a
coloured grade or a painted look. It is chosen by whoever plays, as a look pack or a texture pack is, and anything
it leaves out is the engine's own. This page is the plan. Its first stage, a pack's own passes over the frame, is
built ([What stage 1 built](#what-stage-1-built)), and of the second, programs that light block faces and leaves
([What stage 2 built](#what-stage-2-built)); the rest is not yet.

## What Voxear has today

- **Lighting is fixed in the renderer.** The frame (`crates/voxear_render/src/lib.rs`, `passes.rs`, `post_passes.rs`)
  draws depth, shades every surface in one forward pass (`shaders/shading.slang`), block faces through
  `shaders/blocks.slang`'s `terrainShading`, then fog, reflections, TAA, bloom and the tonemap.
- **A material's custom shader** (`shaders/custom.slang`, `CUSTOM_SHADER_MODULES` in `shader_build.rs`) changes one
  material's surface and how the sun and lamps light it. Block faces never run one: `blocks` is not among those
  modules, so `blocks.slang` compiles against the stubs.
- **A look pack's look shader** (`look_packs.rs`, `LookShader`) recolours each finished pixel, with no depth and no
  neighbours.
- **Block light** (`voxear_terrain::blocks::light`): a block world keeps sky light and block light for every block,
  each face's corners carry them, and a window of them round the camera lights fog and models (see
  `manual/Terrain/Block Light.md`). Every pack program can read these.

## What a pack is

A folder named `Name.shaderpack`, holding a manifest, `pack.toml`, its programs as `.slang` files, and any pictures
they read (noise, a colour grade's table). It comes from the same three places look packs do:

- **built in**, `builtin:` and its name, shipped with the engine;
- **the game's**, an asset in its project, built with the game;
- **the player's**, a folder they put in the game's `ShaderPacks` folder beside its saves, `file:` and its path.

```toml
format = 1
description = "Long warm evenings, deep shade under trees, rays through the leaves."

# Values a player sets in the pack's menu, each a slider, a toggle or a choice, read by its programs.
[options]
shadow_softness = { kind = "number", range = [0.0, 4.0], default = 1.0, label = "Shadow softness" }
rays = { kind = "toggle", default = true, label = "Rays through leaves" }

# How the engine's own passes are set, where the pack wants them otherwise.
[engine]
shadow_distance = 160.0
fog = false

# Surface programs: each replaces how one kind of geometry is lit.
[surface]
blocks = "blocks.slang"
leaves = "leaves.slang"
water = "water.slang"

# Full-screen passes, in order, each at one point of the frame.
[[pass]]
name = "Rays"
program = "rays.slang"
at = "after_lighting"
reads = ["depth", "normals", "color", "shadow", "block_light"]
writes = { target = "rays", format = "rgba16f", scale = 0.5 }

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color", "rays"]
```

## The stages a pack can supply

Each is optional. What a pack leaves out the engine does as it does now.

1. **Full-screen passes** at fixed points of the frame: after the opaque surfaces are lit (before water and glass),
   after fog, after TAA (before bloom and the tonemap), and **final**, in place of the tonemap and grade. A pass reads
   any of the frame's images and the pack's own, and writes the colour or targets of its own at full, half or a
   quarter size, kept from frame to frame where it asks (for its own history).
2. **Surface programs**, one per kind of geometry: **block faces**, **leaves** (leaf faces and their sheets),
   **water** (block water, the sea and Pools), **models** and the **sky**. The engine keeps drawing the geometry:
   meshing, culling, the depth passes and a face's picture stay its own. A surface program is a function with a
   fixed signature, given the surface (its colour, normal, roughness, what it is) and the light it can see (sun,
   shadow, block light, lamps), returning its colour, as a material's custom shader's light function is today.
3. **The sun's shadows**: first the pack's settings (distance, resolution, softness) and a filter function (how a
   shadow is read: softer with distance from its caster, coloured through leaves and glass); later, a pack's own
   shadow pass in place of the engine's, for cascades only.
4. **The sky**: the sky behind everything and its clouds, as a surface program over the sky pass.

## What every program gets

One `PackFrame` for every program, laid out in a header beside the engine's `Frame`:

- **Sun and moon:** each one's direction, colour and strength; the sun's colour through the atmosphere.
- **Time:** the time of day, 0 to 24, from the level's Climate clock (`crates/voxear_engine/src/heat.rs`,
  `Clock.cs`), the day it is, and seconds since the level opened, for anything that moves.
- **Weather:** its amounts (cloud, rain, snow, fog, wind, lightning's flash), the wind's way, and how wet the ground is.
- **Camera:** where it is and looks, its view and projection, the same for last frame, the jitter TAA uses, and
  the picture's size.
- **Exposure:** last frame's, so a pass works in the same units as the engine's light.
- **The pack's options**, as the player set them.

And for the surface it lights, a surface program also gets:

- its place, normal, colour, roughness and how metallic it is;
- for a block face, **the block's number and kind** (leaves, water, glass, a block of parts, a block that gives
  light), its corner shade, and **its sky light and block light** from its corners;
- for anything else near blocks, the sky light and block light round it, from the window round the camera.

A full-screen pass can read: depth; normals and roughness (the `surface` image the depth passes write whenever
ambient occlusion, contact shadows or reflections are on; a pack that reads it has them run); a new **surface id**
image, the block's kind and number or the material's, written by
the same passes; the lit colour; last frame's colour; motion; the sun's shadow (as a function over its virtual
pages, since a pass cannot read them as one picture); and the block light window.

## How it fits the frame

- **The frame graph** (`crates/voxear_render/src/graph.rs`) already orders passes by what they read and write. A
  worn pack adds its passes there at their points, its images as the graph's, and the graph orders and syncs them as
  it does the engine's own. With no pack worn, nothing is added and the frame is as it is now.
- **Surface programs** are compiled in, as material custom shaders are: `blocks` and `water` join the modules
  compiled with `VOXEAR_CUSTOM_SHADERS`, and the pack's functions take the place of the stubs. A pack's surface
  programs are one more variant of those pipelines, made when the pack is worn.
- **Wearing a pack** swaps pipelines as hot reload does now. A pack that fails to compile is logged with its file
  and line, and the engine's own lighting carries on.
- **Editor:** a pack is an asset with a page in the Inspector (its options, its passes, what failed). The Scene view
  and the game view can wear one. A pack's files are watched as the engine's shaders are in a source checkout, and a
  save recompiles it in a moment, its errors in the Console. `vox editor call shader_pack.wear`, `shader_pack.list`
  and `shader_pack.options`, and `vox render --shader-pack`, so tools and `vox mcp` can drive and picture it.
- **Builds:** a game's packs are compiled when its content is built and shipped as SPIR-V. A player's pack is
  compiled when first worn, with the Slang compiler shipped beside the engine (as material custom shaders are), and
  kept by its hash beside the game's saves.
- **Only how it looks.** A pack never changes what a game does. In a game played together, each player wears their
  own, and nothing of it goes over the network.

## What it costs

Measured for stage 1: Evening at 1440p on an RTX 3070, looking into a low sun through trees in a world of blocks,
each pass the least of 60 frames, twice: rays at half size 0.48–0.50 ms, the colour copy before a pass that writes
it in place 0.07 ms, the light laid over the scene 0.21 ms, the grade 0.07 ms; about 0.83 ms in all. The engine's own
passes took the same with and without it. The plan's estimates, before it was built:

- **Each full-screen pass** at 1440p: about 0.1 ms for a grade, 0.5 to 1 ms for rays or fog marched at half size, on
  a GPU like an RTX 3070.
- **A surface program** costs what its own maths costs, per pixel of the surfaces it lights: the engine's lighting is
  left out where it stands in.
- **The surface id image:** 4 bytes a pixel (14 MB at 1440p) and a write in the depth passes, made only while a pack
  reads it.
- **Compiling:** a few seconds for a pack worn the first time; nothing after, from the cache.
- **Memory:** the targets a pack asks for, and its pictures.

## In what order

1. **A pack's own full-screen passes over the engine's buffers.** ✅ Built. The smallest step that does something: the
   manifest, the three places packs come from, compiling, the four points of the frame, `PackFrame`, depth, normals,
   colour, last frame's colour, motion and the block light window, live reload in the editor, and `vox render
   --shader-pack`. A built-in pack shows it: rays through leaves and a warm evening grade. Fog, outlines, a painted
   look and colour grades can be made with this alone.
2. **Surface programs for block faces, leaves and water**, with the block's kind and its sky light and block light.
   This is where a pack changes how blocks are lit, not only how the picture is finished. ✅ Block faces and leaves
   built; water not yet.
3. **The final pass and the sky:** a pack's own tonemap and grade in place of the engine's and the look shader, and
   its own sky and clouds.
4. **Shadows:** a pack's shadow settings and filter, then coloured shadows through leaves and glass, then a pack's
   own cascade pass.
5. **Players' packs:** the `ShaderPacks` folder, a menu to pick one and set its options, the compile cache, and the
   surface id image for passes that want it.

## What stage 1 built

As planned, with these settled on the way:

- **The final point runs over the finished picture**, after the engine's tonemap, grade and a look pack's shader, in
  colours as shown from 0 to 1, before sharpening, the dither and the UI. Taking the tonemap's place, with the
  exposure and the glow handed to the pack, is stage 3's, with the sky: a grade needs nothing more, and a pack that
  tonemapped for itself would have to fit HDR displays as the engine's composite does.
- **A pass reads only what its `reads` lists**: `color`, `depth`, `normals`, `motion`, `history`, `block_light` and
  earlier passes' targets. Its program is wrapped in a file that defines the functions for those alone, so calling
  another fails to compile, with a word on what to list. Normals have the depth passes run; history has the colour
  after TAA copied each frame; so a pack pays for neither unless it reads them.
- **Targets** are `rgba16f`, `rgba8`, `rg16f` or `r16f`, at 1, ½ or ¼ of the frame's size at the point that writes
  them; each is written by one pass and read by later ones; `kept = true` makes it a pair kept from frame to frame,
  read as last frame's with `.last(uv)`.
- **Before TAA a pass writes the colour in place**, so water, glass, the fog and particles are drawn over it; the
  colour is copied first for the pass to read. After TAA and at the final point it writes images of its own.
- **`PackFrame`** has one light, the sun or the moon (`night`), since the engine lights a level by one directional
  light, its own colour and strength and the light it gives; the sun's and sky's light through the atmosphere
  (`sunLight()`, `skyAround()`); the day counted as the level's clock passes midnight.
- **`[surface]` and `[engine]`** are refused, with a message, until stages 2 and 4 read them.
- **Built games** ship a pack's programs beside its manifest and compile them the first time it is worn, with the
  Slang compiler shipped beside the engine. Compiled passes are kept by a hash of their sources for as long as the
  game runs; keeping them on disk, and compiling a game's packs when its content is built, are stage 5's.
- **Not in stage 1:** reading the sun's shadow and the surface id image, and pictures of a pack's own (noise, a
  grade's table).

## What stage 2 built

Programs that light block faces and leaves; water's is still to come. Settled on the way:

- **How:** a pack's `[surface]` names a `blocks` program and, if it likes, a `leaves` one. The engine compiles its own
  `blocks.slang` again with `VOXEAR_PACK_BLOCKS` defined and a file of its making on the include path, which has the
  pack's options and each program in a namespace of its own; `shading.slang`'s fragment shader then hands the face to
  `packBlockColor` where it would have added up its light. While the pack is worn the renderer draws with that module
  in place of its own, every pipeline made again (a game's custom shaders kept); taken off, with its own again.
- **What a program is given:** `BlockSurface` (where, its normals, its colour after tints, snow and the wet, how rough
  and metallic, what it gives off, its corner's shade, its sky light and block light, its look's number and what it
  is) and `BlockLighting` (the sun's way, light and shadow, the sky's light and the blocks' own as much as the face's
  light lets in, occlusion, what it reflects, and the engine's own terms, which add up to its colour, and that colour).
  So the easy way in is to reweigh the engine's terms, and the full one to light the face from what reaches it. It
  returns the light leaving the face, before the air's haze and the clouds in front.
- **Not the block's number:** a face carries its look's number in its world, not its block's; a pack knows no game's
  block numbers anyway. It has the look and what the face is: leaves, a sheet of leaves or a vine, a block of parts,
  cut out, glowing, of a world drawn smooth, cut into small cubes.
- **Vertices stay the engine's**, as the plan has them; the question below stays open until water wants waves.
- **The frame** a surface program reads is `PackFrame`, reached through the engine's table of buffers, and written
  for every view, those through Portals too.
- **Not yet:** water (blocks', the sea's and Pools'), models and the sky (stage 3); what the GI's rays and reflection
  probes see of blocks is still the engine's lighting.

## Questions to settle

- Whether a surface program may also move vertices (leaves in the wind, water's waves), or the engine keeps every
  vertex its own. The plan keeps vertices the engine's, which keeps meshing and culling simple and the same for all.
- Whether a pack may turn off engine passes it replaces (the engine's fog, its reflections) or only set them. The
  plan lets it turn them off in `[engine]`, so a frame never pays for both.
- How far to trust a player's pack: a program can be slow enough to stall the GPU. Packs run as they are, the
  game's menu can always go back to none, and a pack that stalls is turned off on the next start.
