---
order: 3
---

A camera is what the player sees the world through. The Game view, and the game itself, look through every camera in the level that is switched on, each where its viewport puts it, in order of priority; with one, as most levels have, it fills the screen. Add one with the Hierarchy's **+** button, then **Camera**; new projects start with one, called Main Camera.

A camera looks the way its Obj faces. Select it, and the Scene view shows the shape of what it sees.

## A camera's settings

| Setting | What it does |
| --- | --- |
| Projection | **Perspective**, as an eye sees, further off smaller; or **Orthographic**, every ray parallel, things the same size however far off |
| Field of view | A perspective camera's: how wide it sees, in degrees from top to bottom. 60 is usual; lower zooms in, higher sees more around the edges |
| Size | An orthographic camera's: half the height it sees, in metres. 5 sees 10 metres from top to bottom |
| Near | How close something can be, in metres, and still be seen. Anything nearer is cut away |
| Exposure | How bright the picture is |
| Field of view axis | Whether the field of view is measured up and down or across, so it sees as wide on every screen |
| Physical camera | Sets the field of view by a lens and a sensor, below |
| Far | How far it sees, in metres; 0 as far as there is anything. An orthographic camera sees 4096 metres past its near plane at most |
| Viewport | Where on the screen it draws: left, bottom, width and height, from 0 to 1 |
| Priority | Which camera draws first: the lowest first, the highest on top |
| Sees *layer* | Whether it draws the Objs of each layer the project names (see [[Layers]]) |
| Draws to | A display, or a **Picture** of its own that a screen in the world shows |
| Background | What fills where it sees nothing: **Sky**, one **Colour**, or **Nothing**, what was drawn there before |
| Post-processing | Whether the level's post-processing finishes its picture; off, it is shown as lit |
| Anti-aliasing | Its own anti-aliasing, or the post-processing's |
| HDR | Keeps light brighter than white for bloom and the tonemap; off, it is cut at white |
| Occlusion culling | Leaves out what is hidden behind nearer things |
| Drawn over | Another camera it is stacked over, below |
| Clip plane | A plane nearer than which nothing is drawn, below |
| Follows | The Obj it films, which its view is carried through Portals to go on seeing |

Every one of these starts as a plain camera has it, and costs nothing until it is changed.

## A physical camera

Turn on **Physical camera** and the field of view comes from a lens and a sensor, as on a real camera: **Focal length** in millimetres (24 wide, 50 about what an eye sees, 135 a long lens) and **Sensor size**, 36 by 24 millimetres for full frame. **Gate fit** says how the sensor meets a screen of another shape: by its **Vertical** height, its **Horizontal** width, **Fill** to show it whole, **Overscan** to fill the screen, or **None** to stretch it. **Lens shift** moves the lens off the middle, as a share of the picture: shifted up, a camera looking straight ahead sees a tall building whole with its sides upright. Turning it on keeps the field of view the camera had.

```csharp
camera.FocalLength = 35f;              // the easy way in: it makes the camera physical
camera.SensorSize = new Vector2(36f, 24f);
camera.GateFit = GateFit.Overscan;
camera.LensShift = new Vector2(0f, 0.2f);
```

A camera that is not physical can measure its field of view across with **Field of view axis**, so a wide screen and a tall one see the same width: `camera.FieldOfViewAxis = FieldOfViewAxis.Horizontal`.

## Split screen and priority

Each camera draws in its **Viewport**: left, bottom, width and height as shares of the screen. Two players side by side are two cameras, one with a viewport of `0, 0, 0.5, 1` and the other `0.5, 0, 0.5, 1`; four are quarters. Cameras draw by **Priority**, the lowest first, so a small map in a corner, a camera with a viewport of `0.75, 0.75, 0.25, 0.25` and a higher priority, draws over the main one. In the editor the Game view shows them all, as the game does.

```csharp
left.Viewport = new Vector4(0f, 0f, 0.5f, 1f);
right.Viewport = new Vector4(0.5f, 0f, 0.5f, 1f);
map.Priority = 10;
```

**Draws to** picks the display. Display 1 is the game's own window; a camera on another display is drawn where the game has a window on it, and not drawn otherwise.

## Cameras stacked over another

A first-person view's weapon and hands should never be cut into by a wall the player stands against, and a UI in the world may need to sit over everything. Both are a camera **Drawn over** another: it draws after it, in its place on the screen, with its own depth, so nothing the world camera drew cuts into it, and wherever it draws nothing the world shows through. Give the stacked camera only the hands' layer and the world camera every layer but that one.

```csharp
// The easy way in: drawn over the main camera, with nothing behind.
var main = Obj.Find("Main Camera").Camera;
hands.DrawOver(main);
hands.Layers = LayerMask.GetMask("Hands");
main.Layers = new LayerMask(~LayerMask.GetMask("Hands"));
```

The full way is its settings: `hands.DrawnOver = main`, `hands.Background = CameraBackground.Nothing` and a priority above the main camera's. Several cameras can be stacked over one, in order of their own priority. A camera with a background of **Nothing** and no camera to be drawn over is laid over whatever cameras drew there before it.

## Background

**Sky** draws the level's sky where the camera sees nothing. **Colour** fills it with one colour, as bright on screen as the colour says, and the level's sky still lights the scene. **Nothing** leaves what was drawn there before, for a camera laid over others: `camera.BackgroundColor = Color.Hex("#202030")` sets a colour and switches the background to it.

## A camera's own finish

**Post-processing** off shows the camera's picture as it is lit, without any of the level's effects or tonemap, as a map or a UI camera wants. **Anti-aliasing** picks its own, in place of the post-processing's. **HDR** off cuts light at what shows as white before post-processing, so nothing glows past it; **Occlusion culling** off draws what is hidden behind nearer things too, for a camera that moves in jumps. From a script: `camera.PostProcessing`, `camera.AntiAliasing = AntiAliasingMode.Fxaa`, `camera.Hdr` and `camera.OcclusionCulling`.

## Layers and far limit

Each **Sees** row is a layer of the project's: a camera leaves out the Objs of every layer it does not see. `camera.Layers = LayerMask.GetMask("Default", "Water")` sets them from a script. **Far** cuts away what is further off than it, in metres; the sky is still drawn behind.

## Pictures on screens

A camera can draw into a picture of its own in place of a display: set **Draws to** to **Picture**, its **Picture size**, and **Shown on** to an Obj with a mesh. The mesh then shows the picture, glowing as a screen does: a security monitor, a mirror, a map table. The picture is drawn first each frame, so the screen shows this frame's.

```csharp
// The easy way in: a 512 by 512 picture on the monitor.
securityCamera.ShowOn(monitor);
// The full way: its size, and any Objs to show it, each as bright as it says.
securityCamera.PictureSize = new Vector2(1024, 512);
wall.ShowCameraPicture(securityCamera, brightness: 2f);
```

## Oblique and custom projections

A **Clip plane** is a plane in the world, its way out and its place, nearer than which the camera draws nothing: a mirror's or water's reflection camera clips what is behind the mirror with it, `camera.ClipPlane = new Vector4(0f, 1f, 0f, -waterHeight)`. All 0 is none.

For an effect that needs its own, a script can give a camera a whole projection matrix, `camera.CustomProjection = matrix`, laid out as `Matrix4x4` is, and `null` takes it off. It should draw with reversed depth, 1 at the near plane, as the camera's own do: a perspective one without end, or an orthographic one; every pass then reads it as it reads those. Scene files do not keep it.

## In scene files and from tools

A scene writes a camera's settings beside its field of view, only those that differ from a plain camera's:

```toml
camera = { fov = 50.0, far = 300.0, viewport = [0.0, 0.0, 0.5, 1.0], priority = 1, layers = 4294967263, background = { kind = "color", color = [0.1, 0.1, 0.15] }, finish = { hdr = false } }
camera_over = "Main Camera"                                # drawn over this camera
camera_screen = { camera = "Security Camera", brightness = 1.5 }   # on a screen's Obj
```

`field.set` sets each row as the Inspector does, by these names on the `camera` component: `projection`, `fov`, `size`, `near`, `exposure`, `fov_axis`, `physical`, `focal_length`, `sensor`, `gate_fit`, `shift`, `far`, `viewport`, `priority`, `sees.` and a layer's name, `target`, `picture_size`, `shown_on`, `background`, `background_color`, `post`, `anti_aliasing`, `hdr`, `occlusion_culling`, `camera_over` and `clip_plane`. For example `vox editor call field.set obj="Map Camera" component=camera field=viewport value='[0.75, 0.75, 0.25, 0.25]'`; `vox mcp` gives the same to an AI (see [[Driving the Editor from a Terminal]]).

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
