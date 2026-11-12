---
order: 2
---

A scene's sky is one of two kinds:

- **A gradient**, the default: a smooth blend of three colours you choose, one straight up, one at the horizon, and one below it.
- **The atmosphere**: the Earth's air, lit by the level's sun. It is blue by day and pale near the horizon, glows round the sun, and turns orange and red as the sun sets. Distant things fade into its haze.

Either way, the sun's disc is drawn in the sky, in the direction and colour of the level's sun.

The sky also gives everything a soft light from all around, called **ambient light**: coloured like the sky from above, and like the ground from below. It is what keeps the shady side of things from going black.

## Setting the sky

The sky belongs to a scene, and is set in the scene file, at the top:

```toml
[sky]
zenith = [0.517, 0.701, 0.931]   # straight up
horizon = [0.809, 0.865, 0.936]  # at the horizon
ground = [0.506, 0.485, 0.461]   # below the horizon
ambient = 1.0                    # how strong its soft light is
```

Colours are red, green and blue from 0 to 1. `zenith`, `horizon` and `ground` must all be there. `ambient` is 1 unless you say otherwise: lower it for a darker, moodier level, as a night scene might:

```toml
[sky]
zenith = [0.02, 0.03, 0.08]
horizon = [0.06, 0.07, 0.14]
ground = [0.02, 0.02, 0.03]
ambient = 0.6
```

## The atmosphere

To use the atmosphere, set `mode`:

```toml
[sky]
mode = "atmosphere"
zenith = [0.517, 0.701, 0.931]
horizon = [0.809, 0.865, 0.936]
ground = [0.506, 0.485, 0.461]
ambient = 1.0
```

The atmosphere works out its own colours from where the sun is, so the three colours are not used while it is on. Keep them in the file all the same: they are the sky again if you set `mode` back to `"gradient"`.

Turn the sun to change the time of day. Low over the horizon it gives a sunset, and below it a dusk. The atmosphere also lights the scene: its ambient light comes from the sky's colours as they are, so a sunset level is lit warm and a midday one cool. `ambient` scales it as it does for a gradient.

### Haze

Air holds dust and moisture, which scatter sunlight: they make the bright glow round the sun, and they fade far hills into pale light. These settings change the haze:

| Setting | What it does |
| --- | --- |
| `haze = 1.0` | How much haze hangs in the air: 1 as on a clear day, 2 or 3 for a hazy, dusty or golden evening, below 1 for crisp mountain air |
| `haze_tint = [1.0, 1.0, 1.0]` | The colour the haze scatters: white for the grey of moisture, a little warmer, such as `[1.0, 0.9, 0.8]`, for dust |
| `haze_glow = 0.8` | How much the haze glows round the sun, from 0 to 0.95: higher gives a brighter, tighter glow looking towards the sun, lower an even haze |
| `distance_haze = 10.0` | How quickly haze builds up over distance. Real haze barely shows across a few hundred metres, so distances count as this many times as far: higher swallows the distance sooner |
| `sun_size = 0.54` | How wide the sun's disc looks, in degrees. The real sun is about half a degree |

```toml
[sky]
mode = "atmosphere"
haze = 2.0
haze_tint = [1.0, 0.94, 0.86]
haze_glow = 0.85
distance_haze = 16.0
```

## Clouds

A sky can have clouds: heaped clouds in a layer over the level, which drift with the wind. The sun lights them, bright where it reaches them and grey underneath, gold and red as it sets, and the moon lights them at night. Their shadows cross the ground, the water and everything else the sun lights, and where the level has fog with light shafts, the shafts fall between them.

Clouds are part of the sky, under `clouds`. Only `cover` is needed:

```toml
[sky.clouds]
cover = 0.4
```

| Setting | What it does |
| --- | --- |
| `cover = 0.0` | How much of the sky the clouds fill, from 0 to 1: 0 for none, 0.4 for a fair day, 0.7 for broken cloud, 1 for overcast |
| `height = 1200.0` | How high the clouds' undersides are above the level's ground, in metres |
| `thickness = 700.0` | How tall the clouds are, in metres, from their undersides to their tops |
| `size = 900.0` | How far across one cloud is, in metres |
| `density = 1.0` | How thick the clouds are: lower lets more light through them, higher makes them heavy and dark underneath |
| `drift = 8.0` | How fast the clouds move, in metres a second. They go the way the level's Wind Zone blows (see [[Wind]]), or along x where the level has none |
| `shadows = 0.8` | How dark the clouds' shadows on the level are, from 0 to 1: 0 turns the shadows off |
| `kind = "soft"` | What the clouds are made of: `"soft"` for billowing clouds, `"blocks"` for clouds built of cubes |
| `block = 100.0` | How wide one cube is, in metres, when `kind` is `"blocks"` |

Clouds work with both kinds of sky. Under the atmosphere they also grey the sky's ambient light, the more of the sky they cover, as a cloudy day's light is greyer than a clear one's.

A low, thick layer with full cover makes a storm's ceiling:

```toml
[sky.clouds]
cover = 1.0
height = 800.0
thickness = 1500.0
density = 2.0
```

### Clouds built of cubes

For a world of blocks, set the clouds' `kind` to `"blocks"`. The clouds keep their places, heights and drift, but each is built of cubes: flat white tops, lighter sides towards the sun and greyer ones away from it, pale undersides, and shadows on the ground with square corners.

```toml
[sky.clouds]
cover = 0.4
kind = "blocks"
block = 100.0
```

`block` is the width of one cube. Big cubes, 100 metres or more, give a few chunky clouds; smaller ones show more of each cloud's shape. For flat slabs of cloud one cube deep, set `thickness` to the same as `block`. Clouds built of cubes are drawn out to 16 kilometres, and fade into the sky before that.

The clouds are the same every time the level loads, and with no cover they cost nothing.

> [!NOTE]
> Clouds are drawn behind everything in the level, so a mountain taller than their undersides is not hidden in them, and a camera that flies up into them sees them only against the sky. They do not show in reflections yet.

## Night: the moon and the stars

For a night scene, make the level's directional light the **moon**: set its **Sky body** to Moon in the Inspector, or `body = "moon"` in the scene file. The sky then draws the moon's disc where the light comes from, with its darker seas, and darkens to a night sky, so the stars show. Give the moon a cool colour and a low intensity, enough to see the scene by:

```toml
[obj.light]
type = "directional"
color = [0.6, 0.7, 1.0]
intensity = 0.15
body = "moon"
```

Moonlight in games is much brighter than the real thing, so that players can see; the sky under it is kept as dark as a real night sky, rather than lit in proportion like a dim day.

**Stars** are always in the sky, as they are in life: each shows only where the sky behind it is dark enough, so none show by day, the brightest come out first at dusk, and most show under the moon. The moon and the stars are drawn as an eye that has got used to the dark sees them, so they show however dark the exposure leaves the rest of the night. Turn the stars down, or off, with the sky's `stars`:

```toml
[sky]
stars = 0.5   # 1 as the night sky shows them, 0 for none
```

## Which scene's sky

The level uses the sky of its active scene (see [[Scenes and Levels]]). With no sky anywhere, it gets a clear midday sky.

The horizon colour is also the colour of the distant mist; see [[Post-Processing]].

> [!NOTE]
> The editor has no settings for the sky yet. Change the scene file in a text editor, then open the level again to see it.
