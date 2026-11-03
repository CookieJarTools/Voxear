---
order: 4
---

Post-processing is what Voxear does to the picture after drawing it, the way a photo is finished: soft glow around bright lights, a gentle mist into the distance, smooth edges, and balanced colours.

Every level gets the **Voxear Look** unless a scene says otherwise. It is made to look good from the start: smooth edges, a little glow, exposure that adapts to the light, rich but natural colour, darker corners, and a light mist.

## Choosing a preset

Set post-processing in a scene file, in a `[post]` table. Start from one of three presets:

| Preset | What it gives |
| --- | --- |
| `voxear` | The Voxear Look, as above |
| `neutral` | Nothing added: no glow, mist or dark corners, and a fixed exposure. Useful for checking colours |
| `cinematic` | The Voxear Look, with more glow, film-like colour, darker corners, motion blur, colour fringes and film grain |

```toml
[post]
preset = "cinematic"
```

Then change only what you want different. This is the Lights sample's, which adds a little more glow around its lamps:

```toml
# The Voxear Look, with a little more glow around the lamps.
[post]
preset = "voxear"

[post.bloom]
intensity = 0.06
```

The level uses the post-processing of its active scene (see [[Scenes and Levels]]).

> [!NOTE]
> The editor has no settings for post-processing yet. Change the scene file in a text editor, then open the level again to see it.

## Every post-processing setting

The values shown are the Voxear Look's.

**Smooth edges**, `[post.anti_aliasing]`

| Setting | What it does |
| --- | --- |
| `mode = "taa"` | How edges are smoothed: `"taa"` blends each frame with the ones before; `"fxaa"` is simpler, smoothing each frame on its own; `"off"` turns it off |
| `sharpness = 0.25` | Sharpens the picture again after smoothing |

**Shade in corners**, `[post.ambient_occlusion]`

Ambient occlusion darkens the light from the sky where surfaces close in on each other: in corners, under furniture, and where an Obj meets the floor. It only takes away the sky's light, so sunlight and lamps are not changed.

| Setting | What it does |
| --- | --- |
| `enabled = true` | Corners and creases darken |
| `radius = 0.75` | How far, in metres, a surface looks for what shades it |
| `strength = 1.0` | 1 is as measured. Higher darkens more, and 0 changes nothing |
| `tint = [0, 0, 0]` | The colour that fully shaded light takes on. Black only darkens; a warm brown or a purple gives a stylised look |
| `quality = "medium"` | `"low"`, `"medium"`, `"high"` or `"ultra"`. Higher looks smoother and costs more |
| `full_resolution = false` | Works out every pixel each frame. Normally a quarter of the pixels are worked out each frame, and the frames before fill in the rest. Full resolution is sharper in fine detail, and costs about three times as much |

To see the occlusion on its own, press F7 in a game window, or use `vox render --debug-view ao`.

**Glow**, `[post.bloom]`

| Setting | What it does |
| --- | --- |
| `enabled = true` | Bright things glow |
| `intensity = 0.04` | How much glow |
| `scatter = 0.7` | How far it spreads |

**Exposure**, `[post.exposure]`

| Setting | What it does |
| --- | --- |
| `auto = true` | Adjusts the brightness to the scene, as eyes do. See [[Cameras]] |
| `compensation = 0.5` | Brighter or darker than auto exposure would choose, in stops: each 1 doubles the brightness |
| `min_ev = -3`, `max_ev = 1` | The darkest and brightest auto exposure may go |
| `speed_up = 2`, `speed_down = 1` | How fast it adjusts, to brighter and to darker scenes |

**Colour**, `[post.tonemap]` and `[post.grading]`

| Setting | What it does |
| --- | --- |
| `mapper = "agx"` | How bright colours are fitted to the screen: `"agx"`, `"aces"` (more like film), `"neutral"`, `"reinhard"` or `"none"` |
| `contrast = 1.1`, `saturation = 1.15` | Contrast and colour strength, in `[post.tonemap]` |
| `temperature = 0`, `tint = 0` | Warmer or cooler, greener or pinker, in `[post.grading]` |
| `lift`, `gamma`, `gain` | Adjust the dark, middle and bright parts of the picture, each as red, green and blue, in `[post.grading]` |

**Dark corners**, `[post.vignette]`

| Setting | What it does |
| --- | --- |
| `intensity = 0.2` | How dark the corners get. 0 turns it off |
| `smoothness = 0.4` | How gently it fades in |
| `roundness = 1.0` | 1 for a circle, 0 to follow the screen's shape |

**Mist**, `[post.fog]`

| Setting | What it does |
| --- | --- |
| `enabled = true` | Distant things fade into mist |
| `density = 0.004` | How thick it is |
| `height_falloff = 0.12`, `base_height = 0` | It is thickest low down, thinning as it rises from this height |
| `color` | Its colour. Unset, it is the sky's horizon colour |
| `sun_glow = 0.5` | How much it glows looking toward the sun |
| `volumetric = false` | Lights the mist near the camera as the scene's lights reach it: shafts of sunlight between shadows, and lamps glowing in it |
| `volumetric_distance = 64` | How far from the camera, in metres, the mist is lit that way. Beyond it, the mist carries on as without |
| `anisotropy = 0.6` | How much the mist scatters light onward rather than back: higher makes it brighter looking toward a light, as haze is |

With `volumetric = true`, the mist takes its light from the sun, the sky and every point, spot and area light near it. Thicker mist (a higher `density`) shows the shafts more clearly. It costs about two tenths of a millisecond, whatever the screen's size.

**Focus**, `[post.depth_of_field]`, off unless you turn it on

| Setting | What it does |
| --- | --- |
| `enabled = false` | Blurs what is nearer or farther than the focus |
| `focus_distance = 10` | How far away is sharp, in metres |
| `aperture = 2.8` | As on a real camera: lower blurs more |
| `max_blur = 12` | The most blur, in pixels |

**Movement**, `[post.motion_blur]`: `enabled = false`, and `intensity = 0.5` for how strong it is.

**Film grain**, `[post.grain]`: `intensity = 0` (off) and `size = 1.5`.

**Colour fringes**: `chromatic_aberration = 0` sits directly in `[post]`, not in a table of its own. Small values, such as 0.15, split colours slightly at the edges of the screen, as a cheap lens does.

A name Voxear does not know, such as a misspelt setting, is an error, and the Console says which.

## The Scene view's shading

The menu at the top of the Scene view changes how you see the level while you work on it, without changing the game:

- **Lit**: as the game will look.
- **Unlit**: each surface's own colour, with no lights or shadows. Handy for checking materials in a dark level.
- **Wireframe**: the edges of every shape.

Unlit and Wireframe leave post-processing out. The Game view always shows the level as the game will.
