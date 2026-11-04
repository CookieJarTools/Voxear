---
order: 4
---

Plants move in the wind: trees lean and sway, their leaves flutter, and gusts sweep across a field of grass in waves. One **Wind Zone** blows over the whole level, and every plant whose material moves in the wind follows it, in its shading, its shadow and its motion blur alike.

## A Wind Zone

Add a **Wind Zone** to any Obj (Add Component, then Rendering). The wind blows along the Obj's blue (Z) axis, level with the ground: turn the Obj to turn the wind. While it is selected, the Scene view shows its arrows, longer the harder it blows.

| Setting | What it does |
| --- | --- |
| Strength | How hard it blows: 1 a breeze that sways trees, 3 a gale. 0 stops it |
| Gusts | How much gusts add: 0 for steady wind, 1 for gusts twice as strong as the wind between them |
| Gust size | How wide a gust is across, in metres. Smaller gusts ripple through grass; larger ones roll across a valley |
| Gust speed | How fast gusts travel downwind, in metres a second |
| Turbulence | How much plants sway back and forth and across the wind, rather than lean steadily with it |

Only the level's first Wind Zone blows. Without one, nothing moves.

In a scene file:

```toml
[[obj]]
id = 40
name = "Wind"
rotation = [0.0, 70.0, 0.0]
[obj.wind_zone]
strength = 1.5
gusts = 0.8
```

## Plants that move

Foliage materials move in the wind on their own: grass, leaves and bushes need no setup. Two material settings say how:

| Setting | What it does |
| --- | --- |
| Wind bend | How much the plant bends from the ground up: nothing at its root, the most at its tip. 1 for foliage, 0 for anything else, unless you set it |
| Leaf flutter | How much its leaves flutter along their faces, quickly and a little, more out at the tips. 1 for foliage, 0 for anything else |

A tree's trunk is usually not foliage, so give it the same **Wind bend** as its leaves, and the leaves stay on their branches as the tree sways. A rock or a building stays at 0.

A plant bends from its pivot, its Obj's position, and reads its height from its mesh. Give plants their pivot at their base, as tree and grass models usually have: a plant whose pivot is in its middle bends only above it.

In a material file, `wind_bend = 1.0` and `wind_flutter = 0.5`.
