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
| `cinematic` | The Voxear Look, with more glow, film-like colour, darker corners, motion blur and colour fringes |

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

## Choosing a look

A **look** sets the sky, the sun and the post-processing together, for a mood. Pick one in `[post]`:

```toml
[post]
look = "storybook"
```

| Look | What it gives |
| --- | --- |
| `soft_overcast` | Hazy, bright and calm, like daylight through big windows on a cloudy day. The sky is the main light; the sun is weak and warm, with soft shadows and light shafts in a thin blue mist. Bounced light is on |
| `storybook` | A warm sun breaking through a canopy into a cool forest, as in It Takes Two or Ori. A teal sky and mist, shadows tinted a warm purple, golden glow and rich colour. Bounced light is on |
| `golden_hour` | A low, warm sun in dusty air, as in Arc Raiders. The haze glows round the sun and swallows the distance, light shafts cut through it, a wide glare spreads from the sun, and shade stays deep but never black. Bounced light is on. Point the sun low, 5 to 20 degrees up |

Seven looks are after the places of Arc Raiders:

| Look | What it gives |
| --- | --- |
| `overgrown_dam` | Humid green-gold haze over lush ruins, a soft warm sun, light shafts through the greenery |
| `sand_buried_city` | A harsh sun in thick orange dust that swallows the distance, bleached highlights and warm shade |
| `rusted_spaceport` | Flat, cool industrial overcast: a weak sun, grey-teal mist, muted colour and firm contrast |
| `cold_mountain_pass` | A crisp, warm sun over blue haze in the valleys, with cool blue shadows |
| `snowbound_peak` | White, cold and bright: a pale sky as the main light, thick white mist, soft shadows |
| `night_raid` | A faint blue moon under stars, thin dark mist that lamps glow in. Light the level with lamps |
| `electrical_storm` | A dark green-grey sky, heavy air and hard contrast, as a storm closes in |

Five are after the places of Hogwarts Legacy:

| Look | What it gives |
| --- | --- |
| `candlelit_castle` | Warm window light and dust in dark halls, deep shadows, candlelight glowing. Light the halls with warm lamps |
| `dark_forest` | Teal gloom and thick mist, cut by light shafts where the sun breaks through |
| `snowy_village` | Soft, cold winter light, pale mist, and warm windows that glow against it |
| `highland_summer` | Bright, rich and painterly: a clear sky, saturated colour and soft distance |
| `enchanted_night` | A violet moonlit sky full of stars, with a dreamy glow round every light |

A look sets how the sun looks, not where it is: turn the level's sun to suit, low for the dusty looks and high for the bright ones. The night looks make the sun a faint moon.

Whatever the scene sets itself wins over the look's:

- The scene's own `[sky]` replaces the look's sky. Without one, the look's sky is used, and saving the scene leaves it out, so changing the look changes the sky too.
- The look changes the sun's colour and strength, and gives it the look's shadow softness and shadow tint unless the light has its own.
- Any `[post]` setting, such as `[post.bloom]`, replaces the look's.

A look stands in for a preset. If a scene names both, the preset's post-processing wins, and the look still brings its sky and sun.

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
| `taa_history = 0.97` | With `"taa"`, how much of the frames before each frame keeps, from 0.8 to 0.97. Lower reacts faster to what moves but lets fine detail shimmer |
| `render_scale = 1.0` | With `"taa"`, draws the scene smaller and builds the full picture from it, from 0.25 to 1. See below |

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
| `tint = [1, 1, 1]` | The glow's colour, at the same brightness. White leaves it the colour of the light; a gold gives warm highlights |

**Exposure**, `[post.exposure]`

| Setting | What it does |
| --- | --- |
| `auto = true` | Adjusts the brightness to the scene, as eyes do. See [[Cameras]] |
| `compensation = 0.5` | Brighter or darker than auto exposure would choose, in stops: each 1 doubles the brightness |
| `min_ev = -3`, `max_ev = 1` | The darkest and brightest auto exposure may go. Raise `max_ev` to 6 or more for rooms lit through their windows; see [[Bounced Light]] |
| `speed_up = 3`, `speed_down = 1` | How fast it adjusts, in stops a second, to brighter and to darker scenes, easing in over the last stop and a half. Walking from a dark room into sunlight, the picture glares and settles within a second or two; walking back in, the room stays dark for a few seconds before it opens up, as eyes do |
| `low_percent = 70`, `high_percent = 95` | Which part of the picture's brightness exposure is set by: the brightest 30% bar the brightest 5%. Sunlit parts are exposed well and shadows stay dark, as cameras do. Lower both to lift shadows |
| `night = 1` | How dark a dark scene stays, as the eye sees it: the darker it is, the less exposure brightens it, and in moonlight colours fade to grey-blue. 0 brightens night as far as day, in full colour |

**Colour**, `[post.tonemap]` and `[post.grading]`

| Setting | What it does |
| --- | --- |
| `mapper = "agx"` | How bright colours are fitted to the screen: `"agx"`, `"aces"` (more like film), `"neutral"`, `"reinhard"` or `"none"` |
| `contrast = 1.1`, `saturation = 1.15` | Contrast and colour strength, in `[post.tonemap]` |
| `temperature = 0`, `tint = 0` | Warmer or cooler, greener or pinker, in `[post.grading]` |
| `lift`, `gamma`, `gain` | Adjust the dark, middle and bright parts of the picture, each as red, green and blue, in `[post.grading]` |
| `lut = "Looks/Warm.png"` | A colour lookup table, applied last, in `[post.grading]`. See below |
| `lut_contribution = 1.0` | How much of the lookup table's grade shows, from 0 to 1 |

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

**Colour fringes**: `chromatic_aberration = 0` sits directly in `[post]`, not in a table of its own. Small values, such as 0.15, split colours slightly at the edges of the screen, as a cheap lens does.

A name Voxear does not know, such as a misspelt setting, is an error, and the Console says which.

## Reflections

Shiny surfaces reflect what is round them. A floor polished like glass shows the things standing on it, and a chrome ball the room it is in. How much a surface reflects comes from its material: smooth ones, with a low roughness, reflect sharply, and metals most of all.

Voxear reflects two things:

- **The sky**, always: every surface reflects the sky in the direction it faces, blurring as it roughens.
- **What is on screen**, with screen-space reflections: each smooth surface looks along its reflection, through the picture the camera already has, and shows what it finds. They are on in the Voxear Look, and need TAA.

```toml
[post.reflections]
enabled = true
max_distance = 30   # how far reflections reach, in metres
thickness = 0.3     # how thick things count as, in metres
max_roughness = 0.6 # surfaces rougher than this show none
```

Where the graphics card has ray tracing and [[Bounced Light]] is on, reflections trace real rays through the scene instead, from each surface's own normal: a chrome ball mirrors the whole room, behind the camera too, and what it shows is lit as the bounced light lights it. Without ray tracing, reflections are screen-space, and can only show what is on screen: look down at a puddle and the sky it would reflect is above the picture, so the puddle shows the sky's colour from the scene instead, and things just come on screen, or just behind something, can show a moment late. Pixels rougher than `max_roughness` are passed over before any search, so a level of mostly rough ground costs about a twentieth of a millisecond at 1080p.

Reflections are traced at half the screen's resolution, each frame from a different pixel of each block of four, and blended over a few frames into the whole picture. A still view settles to a sharp, steady reflection; a fast-moving one can look a little soft for a moment.

## Drawing at a lower resolution

Drawing every pixel of a large screen is costly. With TAA on, `render_scale` draws the scene at a share of the screen's width and height, and TAA builds the full-size picture from it: the camera is shifted by a different fraction of a pixel each frame, so over a few frames the small pictures between them cover every pixel of the big one. This is how Unreal's screen percentage and temporal upscaling work.

```toml
[post.anti_aliasing]
render_scale = 0.67
```

| `render_scale` | Pixels drawn | Looks |
| --- | --- | --- |
| 1.0 | All of them | As sharp as it gets |
| 0.75 | Just over half | Hard to tell apart |
| 0.67 | Under half | A little softer on fine detail |
| 0.5 | A quarter | Softer, for slower GPUs or very large screens |

Everything that works on the scene as drawn, such as lighting, shadows, ambient occlusion and fog, costs less as the scale drops. Bloom and the finishing passes still work at the screen's full size.

Things moving fast, and things just come into view, have no earlier frames to build from, so they show the smaller picture for a moment. Without TAA, `render_scale` is not used.

To compare, `vox render --render-scale 0.67` draws one picture at a given scale.

## Colour lookup tables

A lookup table grades the picture's colours as an image editor would: any curve, colour balance or film look you can make in one, the game can show. To make one:

1. Write a neutral table, one that changes nothing:

   ```
   vox lut neutral Looks/Neutral.png
   ```

2. Take a screenshot of your game, and grade it in an image editor such as Photoshop, Affinity Photo or GIMP, using only adjustments that change colours, not where they are: curves, levels, hue and saturation, colour balance.
3. Apply the same adjustments to the neutral table, and save it under a new name, as a PNG.
4. Name it in the scene:

   ```toml
   [post.grading]
   lut = "Looks/Warm.png"
   ```

A table is a strip of squares as many wide as it is high: 1024×32, as `vox lut neutral` writes, or 256×16, as Unreal's are. Blue picks the square, red runs across it, and green down it, with black at the top left. Keep tables out of compressed import settings: Voxear reads them as they are.

## Post volumes

A post volume changes the post-processing in part of a level, as Unity's Volumes and Unreal's Post Process Volumes do: a cave can be darker and bluer, a hall of mirrors bloom more, while the rest of the level keeps the scene's settings.

Add one with the Hierarchy's **+** button, **Post Volume**, then size it with its Obj's scale. In the Inspector:

| Setting | What it does |
| --- | --- |
| Shape | **Box**, the Obj's cube; **Sphere**, round it; or **Global**, everywhere |
| Blend distance | How far outside its shape, in metres, its settings start to blend in. 0 switches them on at its edge |
| Weight | How much of its settings show at most, from 0 to 1 |
| Priority | Where volumes overlap, the one with the highest priority wins |

What the volume changes goes in the scene file, under its Obj, written as `[post]` is:

```toml
[obj.post_volume]
shape = "box"
blend_distance = 2.0

[obj.post_volume.post]
exposure = { compensation = -0.5 }
grading = { temperature = -0.3, saturation = 0.8 }
fog = { density = 0.03 }
```

As the camera comes into the volume, numbers and colours blend smoothly from the scene's settings to the volume's. Switches, such as `enabled`, and choices, such as the tonemapper, change once the camera is more than halfway into the blend. A volume cannot change the lookup table.

## The Scene view's shading

The menu at the top of the Scene view changes how you see the level while you work on it, without changing the game:

- **Lit**: as the game will look.
- **Unlit**: each surface's own colour, with no lights or shadows. Handy for checking materials in a dark level.
- **Wireframe**: the edges of every shape.

Unlit and Wireframe leave post-processing out. The Game view always shows the level as the game will.
