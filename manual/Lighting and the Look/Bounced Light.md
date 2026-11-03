---
order: 3
---

In a real room, light bounces: sunlight through a window lights the floor, and the floor lights the walls and ceiling. A red wall in the sun casts a red glow on the ground in front of it, and the space under a table is darker than the open floor, because the table keeps the sky's light out. This is called **global illumination**.

Voxear works it out as the game runs. Turn it on in the scene's post-processing:

```toml
[post.global_illumination]
enabled = true
```

## Ray-traced or voxels

On a graphics card with ray tracing (NVIDIA RTX 20 series and later, AMD RX 6000 and later, Intel Arc), Voxear traces real rays against the scene, as Arc Raiders and The Finals do. Probes round the camera each send out 128 rays; where a ray hits a surface, it takes the light that surface sends back, testing the sun with another ray, and where it misses, the sky's. Each probe also keeps how far its rays went in every direction, so a surface only takes light from the probes that can actually see it. **No light passes through walls**: a sealed room is dark however bright the sun is outside, and a room lit through a doorway is lit only by what comes through it. Probes that end up inside a wall, or too close to one, move out of it.

On other cards, Voxear uses voxels instead, described below. They work everywhere, but light can leak a little through thin walls.

To use voxels even where ray tracing is available:

```toml
[post.global_illumination]
enabled = true
ray_tracing = false
```

## How the voxels work

Round the camera, Voxear keeps three grids of small boxes, called **voxels**, nested inside each other: fine near the camera, coarser farther out. Each frame it draws every mesh near the camera into them, its colour and which way it faces, so walls behind you and round the corner are there as much as those in view. Then it lights each voxel as its surface is lit: by the sun where the sun reaches it, by lamps and torches, by its own glow, and by the light bouncing round it. What the camera sees brightens the voxels it falls in too.

Between the voxels sit **probes**: a fine grid two voxels apart round the camera, and a coarse one four times as far apart that carries on past it, so far walls and the mist over them are lit as near ones are. From each probe, Voxear looks out through the voxels in 32 directions and keeps the light coming from each side: above, below, and the four ways round. A floor takes the light from above it, a wall the light in front of it, and each pixel takes it from the probes round it, leaving out any a wall stands in front of. A few probes, scattered through the grids, are worked out again each frame, so light changes smoothly over about a quarter of a second. When a level opens, every probe is worked out each frame for its first few, so the light has settled by the time you see it.

Each voxel also remembers which way its light faces, so the shaded side of a thin wall shows none of its sunlit side's light.

Light bounces more than once: what one frame bounces lights the voxels for the next. Shiny surfaces reflect the lit voxels too, so a polished floor shows the warm walls round it rather than only the sky.

Bounced light is soft by nature, so the probes show it well. The small, sharp shade where things touch comes from ambient occlusion and contact shadows (see [[Post-Processing]] and [[Lights]]).

The mist in [[Post-Processing]] is lit by the probes as well, so it is dim indoors and bright out in the open.

To see the bounced light on its own, use `vox render --debug-view gi`.

## Settings

| Setting | What it does |
| --- | --- |
| `enabled = false` | Bounced light on or off. Off, the sky lights everything evenly, as [[The Sky]] describes |
| `strength = 1.0` | How bright the bounced light is. Higher gives a brighter, more stylised glow |
| `voxel_size = 0.5` | The finest voxels' size, in metres. Each grid is 64 voxels across, and the fine probes are two voxels apart, 48 across, the coarse ones four times that, so this also sets how far round the camera light bounces: about 190 metres at 0.5. Beyond it, the sky lights things evenly |

Smaller voxels show finer detail, such as light through a doorway, but reach less far. Interiors suit 0.25 to 0.5; open landscapes 1 or more.

## Exposure indoors

A room lit only through its windows is much darker than the sunlit yard outside, as it is in life, and the eye adapts to it. By default, auto exposure only brightens a little, so nights stay dark; for interiors, let it brighten further:

```toml
[post.exposure]
max_ev = 8.0
```

Dark scenes still look dark: the darker a scene, the less exposure brightens it, and in moonlight colours fade to grey-blue, as the eye sees them (see [[Post-Processing]]).

## Cost

Ray-traced bounced light costs about 1.2 milliseconds on an RTX 3070, whatever the screen size: half a millisecond tracing rays and the rest blending them into the probes.

With voxels:

Bounced light costs about a millisecond at 1440p on an RTX 3070: four tenths for drawing the meshes into the voxels, four tenths for the probes, and a little for lighting the voxels and for each pixel to read its probes.

> [!NOTE]
> A thin wall in a coarse voxel can let a little light through to its far side, and lamps cast no shadows into the voxels yet.
