---
order: 3
---

A camera is what the player sees the world through. The Game view, and the game itself, look through the first camera in the level that is switched on. Add one with the Hierarchy's **+** button, then **Camera**; new projects start with one, called Main Camera.

A camera looks the way its Obj faces. Select it, and the Scene view shows the shape of what it sees.

## A camera's settings

| Setting | What it does |
| --- | --- |
| Projection | **Perspective**, as an eye sees, further off smaller; or **Orthographic**, every ray parallel, things the same size however far off |
| Field of view | A perspective camera's: how wide it sees, in degrees from top to bottom. 60 is usual; lower zooms in, higher sees more around the edges |
| Size | An orthographic camera's: half the height it sees, in metres. 5 sees 10 metres from top to bottom |
| Near | How close something can be, in metres, and still be seen. Anything nearer is cut away |
| Exposure | How bright the picture is |

A perspective camera has no far limit: it sees as far as there is anything to see. An orthographic one sees 4096 metres past its near plane.

## Orthographic cameras

An orthographic camera draws every ray parallel to the way it faces, so a box is the same size near and far, and parallel edges stay parallel on screen. Set high above a room and tilted down, it gives the plan-like view of a tactics game, a city builder or a pixel-art room; looking straight down, a map. Its **Size** is how much it sees: half the picture's height, in metres, whatever its distance. Move it closer or further and the picture stays the same size; only what its near plane cuts away changes, so keep it far enough back that nothing you want to see is behind it.

Everything draws as it does for a perspective camera: shadows, lamps, fog and the sky, reflections, ambient occlusion, particles, water, LODs (each picked by its size on screen, which no longer changes with distance) and a shader pack's passes. The Scene view can see orthographic too: click **Ortho** in its header, and again to go back. It starts seeing as much as it did ten metres ahead, its mouse wheel and flying move it as before, and **Frame** (F) fits what it sees to what is selected. From a terminal, `vox editor call scene_view.projection projection=orthographic` does the same.

An orthographic camera goes well with [[Post-Processing#Drawing in pixels|drawing in pixels]], which can keep it on the grid of its pixels so they do not crawl as it glides.

From a script, the easy way in is one call:

```csharp
// A room seen from above, 6 m up and down from the middle of the picture.
camera.MakeOrthographic(6f);
```

and the full one is its two settings, `camera.Projection = CameraProjection.Orthographic` and `camera.Size = 6f`, which a script can change every frame to zoom. In a scene file:

```toml
camera = { projection = "orthographic", size = 6.0, near = 0.3 }
```

From a terminal, `field.set` sets them as the Inspector does: `vox editor call field.set obj="Main Camera" component=camera field=projection value='"orthographic"'` and `field=size value=6`, and `vox mcp` gives the same to an AI (see [[Driving the Editor from a Terminal]]).

## A camera's exposure

Like a real camera, Voxear's adjusts to how bright the scene is: step from a dark cave into sunlight, and the picture brightens and then settles, as your eyes would. This is **auto exposure**, and it is on unless you turn it off.

Because auto exposure keeps adjusting, the camera's **Exposure** setting makes little difference while it is on. To make a scene brighter or darker overall, set the exposure **compensation** in the scene's post-processing instead, or turn auto exposure off. See [[Post-Processing]].
