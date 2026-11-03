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

Round the camera, Voxear keeps three grids of small boxes, called **voxels**, nested inside each other: fine near the camera, coarser farther out. Each frame, it fills them with what the camera sees, lit as it is, and with what the sun's shadows show is solid. Then, for every pixel, it looks out through the voxels in a few directions, and adds up the light the voxels send back, and the sky's light where nothing is in the way.

Because it uses what the camera and the sun see, things neither has seen lately fade from the voxels after a second or so. Light bounces more than once: what one frame bounces lights the voxels for the next.

## Settings

| Setting | What it does |
| --- | --- |
| `enabled = false` | Bounced light on or off. Off, the sky lights everything evenly, as [[The Sky]] describes |
| `strength = 1.0` | How bright the bounced light is. Higher gives a brighter, more stylised glow |
| `voxel_size = 0.5` | The finest voxels' size, in metres. Each grid is 64 voxels across, so this also sets how far round the camera light bounces: 32 metres for the finest, 128 for the coarsest |
| `cones = 5` | How many directions each pixel looks in, from 1 to 5. Fewer is cheaper and blotchier |

Smaller voxels show finer detail, such as the gap under a door, but reach less far. Interiors suit 0.25 to 0.5; open landscapes 1 or more.

## Cost

Bounced light costs about three quarters of a millisecond at 1440p on an RTX 3070, most of it in looking out through the voxels for every pixel. It costs less with fewer cones, and much less with a lower `render_scale` (see [[Post-Processing]]), since fewer pixels look out.

> [!NOTE]
> This is the first version. A cache of the light at points round the camera, which pixels will read instead of each looking out for themselves, will make it several times cheaper. Surfaces the camera has not seen bounce only an estimate of the sun's light.
