---
order: 3
---

A reflection probe shows shiny surfaces what is round them. Without one, a polished floor in a closed room reflects the sky, as if the room had no ceiling; with one, it reflects the room's walls, and a rough floor their soft light rather than the sky's.

A probe is a box. From its middle, or from a point moved off it, it takes a picture of everything round it, in every direction, and the surfaces inside the box reflect that picture.

## Adding one

Click the Hierarchy's **+** button and choose **Reflection Probe**. It comes eight metres square and four high, the size of a room, and is captured once, as the level opens.

Move it to the middle of the room and size its box to the room with the Scale tool, walls, floor and ceiling inside it. The Scene view draws the box, a second box round it as far as it blends out, and an icon where it takes its picture.

One probe for each room or hall is a good start. Out in the open, a large probe with Box Projection off gives the hills and buildings round a place in place of the plain sky.

## Its settings

| Setting | What it does |
| --- | --- |
| Update | **Once**: captured as the level opens, or loaded from its bake, and kept. **Live**: captured again and again as the game goes |
| Interval | For a live probe, the seconds between captures. 0 captures it every frame |
| Blend Distance | How far outside its box, in metres, it fades into what is round it. Its box's own sides, a room's walls and floor, show it fully |
| Box Projection | Bends each reflection to where it meets the box, so a room's walls line up in a polished floor from anywhere in the room, not only from the probe's middle. Leave it on for a probe that fits a room; turn it off for one out in the open |
| Intensity | How bright what it shows is. 1 is as captured |
| Importance | Where boxes overlap, the more important probe shows first; between equals, the smaller |
| Resolution | How many pixels across each side of its picture is: 64, 128, 256 or 512. 128 suits most rooms; 256 or 512 for a large polished floor seen up close, for four or sixteen times the memory and a dearer capture |
| Capture Offset | Where it takes its picture from, in metres from its box's middle along the box's own sides. Move it where the box's middle is inside a wall or a pillar, or down to where a player's eyes are. Box Projection still lines reflections up with the box |

**Capture** takes its picture again now.

## Captured once, and baked

A probe captured once keeps what it saw. A door opening or a lamp going out in front of it does not change it until it is captured again: from the Inspector's Capture button, from a script with `Capture()`, or by baking.

**Bake Reflection Probes**, in the command palette, or **Bake Probes** on a level's page in the Inspector, captures every probe in the open level again and saves what they saw beside the level, as `Level.probes` next to `Level.level`. The level then opens with its probes ready, the same every time, and builds ship the file with the level. Bake again after changing what a room looks like. Save the scene first: the bake finds each probe by its scene and its place in it.

From a terminal, `vox editor call reflection_probes.bake` bakes the open level; `vox editor call reflection_probe.capture obj="Hall Probe"` captures one probe again; and `vox editor call field.set obj="Hall Probe" component=reflection_probe field=resolution value='"256"'` sets one of its settings, as the Inspector does (see [[Driving the Editor from a Terminal]]).

A probe the level has no bake for is captured a moment after the level opens, once its bounced light has settled.

The file is small: each probe's six faces, at its resolution, packed and compressed; about 50 KB for a probe of 128.

## Live probes

A live probe follows what changes: a sunset, a fire, a door. It costs a little each time it is captured, so give it the longest interval that looks right. One probe is captured a frame, so many live probes take turns.

## What a probe sees

A probe sees what the [[Bounced Light]]'s rays see, and more:

- Objs with models, lit by the sun, lamps, glowing materials and the bounced light. Metals show their shine: what the probe round them saw as it was last captured, so a metal's reflection fills in over two captures.
- Terrain, each part with the surface painted most on it there, and worlds of blocks, a little coarser than they are drawn, all in one material.
- The sea and Pools, seen from above: their own colour, what lies under them through the water, and what they reflect.
- The plants of a Foliage within 64 metres of where it takes its picture, as many as 8,192.
- The sky behind them all: the atmosphere, a gradient sky's colours or its picture, and the clouds.

Grass, particles and water seen from under it are not in its picture.

## How reflections fit together

For each pixel of a shiny surface, Voxear looks for a reflection in this order:

1. **Its own ray.** Where the graphics card has ray tracing, a ray traced through the scene; otherwise a search through the picture on screen. See [[Post-Processing#Reflections]].
2. **The reflection probes round it**, where that finds nothing: a ray that leaves the screen, or one that goes out of a window.
3. **The bounced light's probes**, where no reflection probe covers the surface, as seen from as far as the ray got.
4. **The sky.**

## From scripts

```csharp
namespace MyGame;

using System.Numerics;
using Voxear;

public sealed class Lightning : Behaviour
{
	[SerializeField] private Obj hall = null!;

	public void Strike()
	{
		// The hall's probe sees the flash.
		hall.ReflectionProbe?.Capture();
	}

	public void Awake()
	{
		var probe = Obj.AddReflectionProbe();
		probe.Update = ReflectionProbeUpdate.Live;
		probe.Interval = 0.5f;
		probe.BoxProjection = false;
		probe.Resolution = ReflectionProbeResolution.Size256;
		// Its picture taken at eye height, a metre and a half under the box's middle.
		probe.CaptureOffset = new Vector3(0f, -1.5f, 0f);
	}
}
```

## In scene files

```toml
[[obj]]
name = "Hall Probe"
position = [0.0, 2.0, 0.0]
scale = [12.0, 4.0, 20.0]

[obj.reflection_probe]
update = "once"      # or "live"
interval = 0.0       # a live probe's seconds between captures
blend_distance = 1.0
box_projection = true
intensity = 1.0
importance = 0
resolution = 128     # 64, 128, 256 or 512
capture_offset = [0.0, 0.0, 0.0]  # where it takes its picture, from its box's middle
```
