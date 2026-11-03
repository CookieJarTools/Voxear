---
order: 2
---

A scene's sky is one of two kinds:

- **A gradient**, the default: a smooth blend of three colours you choose, one straight up, one at the horizon, and one below it.
- **The atmosphere**: the air of a planet like Earth, lit by the level's sun. It is blue by day and pale near the horizon, glows round the sun, and turns orange and red as the sun sets. Distant things fade into its haze.

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
