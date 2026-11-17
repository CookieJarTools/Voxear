---
order: 8
---

A **shader pack** is a set of small programs that change how a game is lit and finished. A **pass** runs over the picture as each frame is drawn: shafts of sun through leaves, a fog of your own, outlines round blocks, a painted look, a colour grade. It reads what the frame has drawn so far (its colour, how far away each pixel is, which way each surface faces, how it moved, the light kept for the blocks round the camera) and writes a new colour, or a picture of its own for a later pass to read. A **surface program** lights the faces of blocks in place of the engine: warmer sun, bluer shade, leaves that glow with the sun behind them, torchlight of another colour.

A [[Look Packs|look pack]] sets the engine's own lighting; a shader pack adds passes of its own to it. A player can wear one of each, and anything a shader pack leaves out the engine draws as it always does. With none worn nothing of this runs, and a frame costs what it did.

## The pack Voxear comes with

**Evening** gives long warm evenings: warm sun and cooler shade on every block, warmer torchlight, leaves that glow green-gold seen against a low sun, shafts of sun where it breaks through leaves and between blocks, a golden haze in the air that is thicker far off and towards the sun, and a grade that warms the light and cools the shade. In a cave, or anywhere the blocks round the camera keep the sky out, the haze and the rays fade away.

| Option | What it does |
| --- | --- |
| **Rays** | Shafts of sunlight where it breaks through leaves and between blocks |
| **Ray strength** | How bright the rays are, 0 to 2 |
| **Haze** | How much warm haze the air holds, most towards the sun, 0 to 2 |
| **Warmth** | How far the grade warms the picture, 0 to 1 |
| **Leaf glow** | How much leaves glow with the sun through them, seen against a low sun, 0 to 2 |

The rays show when the camera looks towards the sun. They are worked out at half size, forty steps a pixel, and cost about half a millisecond a frame at 1440p on an RTX 3070; the whole pack's passes, looking into the sun, about 0.8 ms. Facing away from the sun the rays cost nothing.

## Seeing a level under a pack

On the **Lighting** page, **Shader pack** shows the Scene view and the game under any pack, as a player who picked it would see it. Nothing of it is saved with the scene. Playing keeps the pack on.

Select a pack's `pack.toml` in the Project panel and its Inspector page shows what it is: **Worn** puts it on and takes it off; **Compiled** says how its programs compiled, or Slang's message with the file and the line; **Passes** lists each pass, where in the frame it runs, what it reads and what it writes; **Surfaces** names its surface programs; and **Options** sets the pack's options, which change the picture at once. Hold the pointer over an option's name for what it does.

A pack with surface programs compiles Voxear's own shader for block faces again with them, which takes several seconds the first time it is worn.

While a pack is worn, saving any of its files compiles it again, in a moment, and the Scene view shows it as it now is. A mistake is shown on its page and in the Console, and the picture stays as it was until the pack compiles again.

## Making a pack

A pack is a folder whose name ends in `.shaderpack`, holding its manifest, `pack.toml`, and its programs:

```
Packs/
	Dusk.shaderpack/
		pack.toml
		haze.slang
		grade.slang
```

```toml
format = 1
description = "A cool dusk: blue haze far off, and a gentle grade."

[options]
haze = { kind = "number", range = [0.0, 2.0], default = 1.0, label = "Haze", about = "How thick the haze is" }
outlines = { kind = "toggle", default = false, label = "Outlines", about = "A dark line round every shape" }
look = { kind = "choice", choices = ["Soft", "Bold"], default = "Soft", label = "Look" }

[[pass]]
name = "Haze"
program = "haze.slang"
at = "after_fog"
reads = ["color", "depth"]

[[pass]]
name = "Grade"
program = "grade.slang"
at = "final"
reads = ["color"]
```

Each `[[pass]]` names its program, the point of the frame it runs at, and what it reads. Passes run in the order of their points, and as the manifest lists them at one point.

### Where a pass runs

| `at` | When | Its colour |
| --- | --- | --- |
| `after_lighting` | Once the opaque surfaces and the sky are lit, before water, glass and the fog | Light, before exposure, at the size the scene is drawn |
| `after_fog` | Once the fog is laid over the scene, before smoke and particles | As above |
| `after_taa` | Once TAA has smoothed the frame, before glow and exposure | Light, before exposure, at the size the picture is shown |
| `final` | Over the finished picture, after the engine's grade and a look pack's shader, before sharpening and the UI | As shown, from 0 to 1, and above 1 for an HDR display's highlights |

Water, glass, fog and particles are drawn over what a pass writes before them, so a pass that paints the ground at `after_lighting` is under the lake and the fog. A pass before TAA has its noise smoothed by it: a few steps marched with a little noise look like many.

### What a pass reads

A pass can read only what its `reads` lists. Leaving out what it does not need keeps the frame cheap: `normals` has the engine draw each surface's normal and roughness first, which it does only when something asks.

| `reads` | Functions |
| --- | --- |
| `color` | `sceneColor(uv)`, `sceneColorAt(pixel)`, `colorSize()` |
| `depth` | `sceneDepth(uv)`, `isSky(uv)`, `sceneDistance(uv)` in metres, `scenePosition(uv)` in the world |
| `normals` | `sceneNormal(uv)` in the world, `sceneRoughness(uv)` |
| `motion` | `sceneMotion(uv)`, how far across the picture a pixel moved since last frame, and `lastUv(uv)` |
| `history` | `lastColor(uv)`: last frame's colour, after TAA |
| `block_light` | `lightAround(position)`, the sky light and block light at a point from 0 to 1; `hasBlockLight()`, `blockGlow()` |
| a target's name | `target.sample(uv)`, `target.load(pixel)`, and for a kept target `target.last(uv)` |

A program that calls a function its pass does not read is told so when it compiles.

### Writing a pass

A program defines one function, `run`, which is given the pixel it works out and returns its colour:

```slang
// haze.slang: a blue haze that thickens with distance.
float4 run(PassInput input)
{
	float3 color = sceneColor(input.uv).rgb;
	float far = 1.0 - exp(-sceneDistance(input.uv) / 300.0);
	float3 haze = skyAround() * float3(0.6, 0.75, 1.0);
	return float4(lerp(color, haze, far * 0.5 * options().haze), 1.0);
}
```

`input.uv` is where the pixel is, from 0 to 1 across and down; `input.pixel` is the same in pixels, and `input.size` the size of what the pass writes. `options()` gives the pack's options by their names, a number as a `float`, a toggle as a `bool`, and a choice as a `uint` to compare with the constants named after it, such as `LOOK_BOLD`.

`frame()` is the frame as every program sees it:

| | |
| --- | --- |
| `cameraPosition`, `cameraForward`, `near` | Where the camera is and looks, and its near plane |
| `view`, `projection`, `viewProjection`, `inverseViewProjection`, `lastViewProjection` | The camera's matrices, as drawn and last frame's |
| `jitter`, `drawnSize`, `shownSize`, `frameIndex` | How far TAA moves this frame, the sizes the scene is drawn and shown at, and which frame it is |
| `sunDirection`, `sunColor`, `sunStrength`, `sunLight`, `night` | The way to the sky's light, its colour and strength, the light it gives, and whether it is the moon |
| `time`, `timeOfDay`, `day` | Seconds since the level opened, and the level's clock: 0 to 24, and the day |
| `cloud`, `rain`, `snow`, `fog`, `wind`, `lightning`, `wetness`, `snowCover`, `windDirection` | The weather now, each from 0 to 1 |
| `exposure` | What the scene's light is multiplied by to show it |

And `sunLight()` and `skyAround()` give the sun's light through the air where the camera is and the sky's light from all round; `toPicture(position, inFront)` where a point in the world shows; `viewRay(uv)` the way through a pixel; `luminance(color)`; and `frameNoise(pixel)`, noise that moves on each frame.

### Pictures of a pack's own

A pass that `writes` a target draws into a picture of the pack's own instead of the frame's colour, for later passes to read:

```toml
[[pass]]
name = "Edges"
program = "edges.slang"
at = "after_lighting"
reads = ["depth", "normals"]
writes = { target = "edges", format = "r16f", scale = 0.5 }
```

`format` is `rgba16f` (the default), `rgba8`, `rg16f` or `r16f`; `scale` is 1, 0.5 or 0.25 of the frame's size at the pass's point. A target that is `kept = true` stays from frame to frame, and a pass reads last frame's with `.last(uv)`, so a pass can build on what it wrote before. Each target is written by one pass, and read by passes after it.

### Lighting blocks

A pack's `[surface]` names the programs that light block faces in place of the engine:

```toml
[surface]
blocks = "faces.slang"
leaves = "leaves.slang"
```

`blocks` lights every block's faces, and leaves too where there is no `leaves`; `leaves` lights faces of leaves and the sheets of leaves and vines bushy blocks have. A pack can have surface programs and no passes, or both.

A surface program defines one function, `run`, given the face at one pixel and the light it sees, and returns the light it sends towards the camera; the air's haze and the clouds in front are laid over it after:

```slang
// faces.slang: the sun warmer and the shade under the sky bluer.
float3 run(BlockSurface surface, BlockLighting light)
{
	float3 diffuse = surface.color * (1.0 - surface.metallic);
	float3 sun = light.sunTerm * float3(1.1, 1.0, 0.85);
	float3 sky = light.sky * float3(0.9, 0.95, 1.15);
	return sun + (sky + light.glow) * diffuse * light.occlusion + light.reflectionTerm + light.lamps + light.emission;
}
```

`light.engine` is the colour the engine gives the face, so `return light.engine;` changes nothing; its terms add up to it, to reweigh one at a time; and what reaches the face is there too, to light it in a way of your own.

| `surface.` | |
| --- | --- |
| `position`, `normal`, `faceNormal`, `view` | Where the face is, the way it faces with its picture's bumps and without, and the way to the camera |
| `color`, `alpha`, `roughness`, `metallic`, `emission` | Its colour after its biome's tint, snow and the wet, and how it reflects and glows |
| `shade` | How open its corner is, 0 where blocks close it in to 1 |
| `skyLight`, `blockLight` | Its sky light and block light, 0 to 1, smooth across it from its corners |
| `look`, `kind` | Its look's number in its world, and what it is: `BLOCK_LEAVES`, `BLOCK_SHEET`, `BLOCK_PART`, `BLOCK_CUT_OUT`, `BLOCK_GLOWS`, `BLOCK_SMOOTH`, `BLOCK_CUBES`, a bit each |
| `pixel` | Where it shows in the picture |

| `light.` | |
| --- | --- |
| `sunDirection`, `sunLight`, `shadow` | The way to the sun (or the moon), its light as it arrives, and how much reaches the face, 0 to 1 |
| `sky`, `glow` | The sky's light from all round, as much as the face's sky light lets in, and the blocks' own light, as much as its block light says |
| `occlusion`, `reflection` | How much of the light from all round reaches it, and what it reflects of its surroundings |
| `sunTerm`, `ambientTerm`, `reflectionTerm`, `lamps`, `emission` | The engine's terms, which add up to `engine` |
| `engine` | The colour the engine gives the face |

Surface programs have the pack's `options()` and `frame()` as passes do. A world drawn smooth, a block of parts and a face cut into small cubes are all lit by `blocks`, told apart by `kind`; block water is still drawn by the engine.

### Things to make

- **Fog of your own**: at `after_fog`, blend towards a colour by `sceneDistance` and by height from `scenePosition`, and turn the engine's fog off in the level's lighting.
- **Outlines**: at `after_lighting` or `final`, darken a pixel where `sceneDepth` or `sceneNormal` jumps from its neighbours'.
- **A painted look**: at `final`, average the colours round each pixel in four corners and keep the calmest, which flattens surfaces into strokes and keeps edges.
- **A grade**: at `final`, change the colour as shown, as Evening's grade does.
- **Coloured torchlight**: in a `blocks` program, `light.glow` tinted orange near the floor and redder as `surface.blockLight` falls off.
- **Glowing ore**: a `blocks` program that brightens a look of yours by `surface.look` when `surface.skyLight` is low.

## From the command line and tools

`vox render` draws its frames under a pack, with options set, and its timings name each pass:

```
vox render MyGame --shader-pack builtin:evening --shader-pack-option warmth=0.8 --shader-pack-option rays=false
```

A pack is named as `builtin:` and its name, a pack of the project's by its `pack.toml`'s path, or `file:` and a folder. Tools reach packs through the editor with `shader_pack.list`, `shader_pack.wear` and `shader_pack.options` (see [[Driving the Editor from a Terminal]]), and `vox mcp` gives them to an AI the same way.

## In a built game

A game's packs ship with it, their programs beside their manifests, and are compiled the first time one is worn, with the Slang compiler Voxear ships beside the game. A player's pack is a folder in the game's `ShaderPacks` folder, beside its saves, named `file:` and its path; it reads nothing outside its own folder.
