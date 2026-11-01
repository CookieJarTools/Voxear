---
order: 2
---

The sky is a smooth blend of three colours: one straight up, one at the horizon, and one below it. The sun's disc is drawn in it, in the direction and colour of the level's sun.

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

The level uses the sky of its active scene (see [[Scenes and Levels]]). With no sky anywhere, it gets a clear midday sky.

The horizon colour is also the colour of the distant mist; see [[Post-Processing]].

> [!NOTE]
> The editor has no settings for the sky yet. Change the scene file in a text editor, then open the level again to see it.
