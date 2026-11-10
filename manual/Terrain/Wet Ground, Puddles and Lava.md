---
order: 13
---

A surface can be soaked through, hold puddles, or be lava, and snow and sand can sparkle in the sun. Each is a setting of a terrain's surface (see [[Surfaces]]), so it goes wherever that surface covers: by its rules, where it is painted, or where a script paints it while the game runs.

## Liquid

Pick the surface in the Terrain's **Paint** tool and set its **Liquid**:

| Liquid | What it does |
| --- | --- |
| None | The surface as its material draws it |
| Wet | Soaked through: darker and glossier, as rain leaves soil and stone |
| Puddles | Wet, with water standing in the ground's hollows and running down its slopes in rivulets |
| Lava | Lava under a crust of the surface's own pictures, cracked into plates that ride downhill, glowing through the cracks |

A wet surface is a second surface of the same material: paint **Wet Mud** over **Mud** where the rain has pooled, or give it rules to cover the low ground, and the two blend as any surfaces do.

## Puddles

Water stands where the ground is lower than all the ground round it, from the middle of a hollow up to the lowest point of its rim, so a shell's crater on a level field fills with water and a dip on a hillside, which drains, stays only wet. Tracks pressed into the ground are hollows too, so wheel ruts fill (see [[Tracks and Snow]]). A [[Wind]] Zone's wind ruffles standing water.

On sloping ground the water runs downhill in rivulets, faster where the ground is steeper. Far off, where a rivulet would be thinner than a pixel, it fades out rather than flicker.

Puddles are drawn on the ground: there is no water to wade into and nothing floats. For water that is, a pond or a river, use a Pool or a Stream (see [[Pools and Dry Areas]] and [[Streams]]).

## Lava

Lava's crust is the surface's own material, so give it a dark, rough material such as basalt. The crust is cracked into plates about a metre and a half across, and the molten rock in the cracks glows by its heat, orange and brightest in the middle of a crack. On sloping ground the plates ride downhill, faster where it is steeper; on level ground they stay put, the crust slowly turning over, one set of cracks fading into another every few seconds. Lava lights only itself: for the glow it casts round it, add a Light. For a lake or a river of lava that runs where you lay it, use a lava Pool or Stream (see [[Lava]]).

## Glitter

Turn on **Glitter** for a surface that sparkles in the sun, such as snow, frost or sand. Grains a couple of centimetres apart each catch the sun when you look along its reflection off them, so the sparkle moves as the camera does and shows most looking towards the sun. Grains in shadow do not sparkle, and those further than about 40 m off fade out.
