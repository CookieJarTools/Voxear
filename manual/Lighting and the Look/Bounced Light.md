---
order: 3
---

In a real room, light bounces: sunlight through a window lights the floor, and the floor lights the walls and ceiling. A red wall in the sun casts a red glow on the ground in front of it, and the space under a table is darker than the open floor, because the table keeps the sky's light out. This is called **global illumination**.

Voxear works it out as the game runs. Turn it on in the scene's post-processing:

```toml
[post.global_illumination]
enabled = true
```

## Ray-traced probes

Voxear traces real rays against the scene, as Arc Raiders and The Finals do. Probes round the camera each send out 128 rays; where a ray hits a surface, it takes the light that surface sends back, testing the sun with another ray, and where it misses, the sky's. Each probe also keeps how far its rays went in every direction, so a surface only takes light from the probes that can actually see it. **No light passes through walls**: a sealed room is dark however bright the sun is outside, and a room lit through a doorway is lit only by what comes through it. Probes that end up inside a wall, or too close to one, move out of it.

This works on every graphics card. On one with ray tracing (NVIDIA RTX 20 series and later, AMD RX 6000 and later, Intel Arc), the card traces the rays itself. On others, Voxear traces them in a compute shader, through a tree of boxes round each mesh's triangles built when the mesh loads and one round the scene's meshes built each frame. The light is the same either way; tracing in a compute shader is slower, so each probe is traced half as often and light settles a little more slowly after a change. To try it on a card with ray tracing, render with `vox render --software-rays`.

## How the probes work

The probes sit in two grids round the camera: a fine one, 32 probes across, 16 high and 32 deep, and a coarse one four times as far apart that carries on past it, so far walls and the mist over them are lit as near ones are. The grids move with the camera, and probes the camera leaves behind are reused for the ground ahead.

A few probes, scattered through the grids, trace their rays each frame: a quarter of the fine ones, which light what is near, and a sixteenth of the coarse, which light the distance. Each blends what it finds into what it had, so light changes smoothly over about a quarter of a second. When a level opens, every probe traces each frame for its first few, so the light has settled by the time you see it.

Light bounces more than once: where a ray hits a surface, the light it brings back includes what the probes round that surface found last frame. Shiny surfaces reflect the probes' light too, so a polished floor shows the warm walls round it rather than the sky through the ceiling.

Bounced light is soft by nature, so the probes show it well. The small, sharp shade where things touch comes from ambient occlusion and contact shadows (see [[Post-Processing]] and [[Lights]]).

The mist in [[Post-Processing]] is lit by the probes as well, so it is dim indoors and bright out in the open.

To see the bounced light on its own, use `vox render --debug-view gi`.

## Settings

| Setting | What it does |
| --- | --- |
| `enabled = false` | Bounced light on or off. Off, the sky lights everything evenly, as [[The Sky]] describes |
| `strength = 1.0` | How bright the bounced light is. Higher gives a brighter, more stylised glow |
| `probe_spacing = 1.0` | How far apart the fine probes are, in metres. This also sets how far round the camera light bounces: the coarse grid reaches about 64 metres each way at 1. Beyond it, the sky lights things evenly |

Closer probes show finer detail, such as light through a doorway, but reach less far. Interiors suit 0.5 to 1; open landscapes 2 or more.

## Exposure indoors

A room lit only through its windows is much darker than the sunlit yard outside, as it is in life, and the eye adapts to it. By default, auto exposure only brightens a little, so nights stay dark; for interiors, let it brighten further:

```toml
[post.exposure]
max_ev = 8.0
```

Dark scenes still look dark: the darker a scene, the less exposure brightens it, and in moonlight colours fade to grey-blue, as the eye sees them (see [[Post-Processing]]).

## Cost

Bounced light costs about 0.7 milliseconds on an RTX 3070 in an outdoor valley of trees and thousands of plants, whatever the screen size: a tenth building the scene for the rays, a third tracing them, and three tenths blending them into the probes, with a little more for each pixel to read the probes. Traced in a compute shader, the rays cost about 1.7 milliseconds there.

Most probes out of doors float in open air, with nothing within a probe's spacing of them. They light only the mist, so they **sleep**: they trace eight times less often than probes near surfaces, which saves most of the work in open places. Plants of a [[Foliage]] are left out of the rays: thousands of small swaying meshes would cost far more to trace than they change the bounced light.

> [!NOTE]
> Rays see every surface as solid, so leaves cut out of a card block light as the whole card would.
