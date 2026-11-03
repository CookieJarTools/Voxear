---
order: 3
---

In a real room, light bounces: sunlight through a window lights the floor, and the floor lights the walls and ceiling. A red wall in the sun casts a red glow on the ground in front of it, and the space under a table is darker than the open floor, because the table keeps the sky's light out. This is called **global illumination**.

Voxear works it out as the game runs. Turn it on in the scene's post-processing:

```toml
[post.global_illumination]
enabled = true
```

## How it works

Round the camera, Voxear keeps three grids of small boxes, called **voxels**, nested inside each other: fine near the camera, coarser farther out. Each frame, it fills them with what the camera sees, lit as it is, and with what the sun's shadows show is solid.

Between the voxels sit **probes**, a grid of points two voxels apart. From each probe, Voxear looks out through the voxels in twelve directions, and adds up the light the voxels send back, and the sky's light where nothing is in the way. Each pixel then takes its light from the eight probes round it, counting those on the side it faces most. A few probes are worked out again each frame, so light changes over about a quarter of a second, not at once.

Bounced light is soft by nature, so the probes show it well. The small, sharp shade where things touch comes from ambient occlusion and contact shadows (see [[Post-Processing]] and [[Lights]]).

Because it uses what the camera and the sun see, things neither has seen lately fade from the voxels after a second or so. Light bounces more than once: what one frame bounces lights the voxels for the next.

## Settings

| Setting | What it does |
| --- | --- |
| `enabled = false` | Bounced light on or off. Off, the sky lights everything evenly, as [[The Sky]] describes |
| `strength = 1.0` | How bright the bounced light is. Higher gives a brighter, more stylised glow |
| `voxel_size = 0.5` | The finest voxels' size, in metres. Each grid is 64 voxels across, and the probes are two voxels apart, 48 across, so this also sets how far round the camera light bounces: 48 metres at 0.5. Beyond it, the sky lights things evenly |

Smaller voxels show finer detail, such as light through a doorway, but reach less far. Interiors suit 0.25 to 0.5; open landscapes 1 or more.

## Cost

Bounced light costs about a quarter of a millisecond at 1440p on an RTX 3070: a tenth for the probes, a tenth for filling the voxels, and a little for each pixel to read its probes.

> [!NOTE]
> Surfaces neither the camera nor the sun has seen lately bounce only an estimate of the sun's light, not their own colour. Light can leak a little through walls thinner than the probes are apart.
