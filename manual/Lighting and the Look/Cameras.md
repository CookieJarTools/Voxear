---
order: 3
---

A camera is what the player sees the world through. The Game view, and the game itself, look through the first camera in the level. Add one with the Hierarchy's **+** button, then **Camera**; new projects start with one, called Main Camera.

A camera looks the way its Obj faces. Select it, and the Scene view shows the shape of what it sees.

## A camera's settings

| Setting | What it does |
| --- | --- |
| Field of view | How wide it sees, in degrees from top to bottom. 60 is usual; lower zooms in, higher sees more around the edges |
| Near | How close something can be, in metres, and still be seen. Anything nearer is cut away |
| Exposure | How bright the picture is |

There is no far limit: a camera sees as far as there is anything to see.

## A camera's exposure

Like a real camera, Voxear's adjusts to how bright the scene is: step from a dark cave into sunlight, and the picture brightens and then settles, as your eyes would. This is **auto exposure**, and it is on unless you turn it off.

Because auto exposure keeps adjusting, the camera's **Exposure** setting makes little difference while it is on. To make a scene brighter or darker overall, set the exposure **compensation** in the scene's post-processing instead, or turn auto exposure off. See [[Post-Processing]].
