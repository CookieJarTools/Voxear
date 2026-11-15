---
order: 3
---

[[Bounced Light]] can be worked out as the game runs, or **baked**: traced once, in the editor, and saved beside the level. Baked light costs almost nothing to draw, shows finer detail (soft shadows in corners, light pooling under a window) and is the same every time the level opens. It does not follow what moves: a door that opens or a lamp carried through a room does not change it.

A bake makes two things:

- **Lightmaps**, for what keeps still: walls, floors, rocks, buildings. Each point of their surfaces keeps the light falling on it, in a texture laid over them.
- **Light probes**, for everything else: characters, props that move, what is spawned. Each probe keeps the light reaching one point from every way, and an Obj is lit by the probes round it, blended for where it is each frame.

## Baking a level

1. Give every still Obj a **Lightmap**: select it, **Add Component**, **Rendering**, **Lightmap**.
2. Add a **Light Probe Group** from the Hierarchy's **+** menu and size its box to the space moving Objs pass through. It fills its box with probes two metres apart.
3. In the Lighting page's **Baked** tab, set **Bounce from** to **Baked**.
4. Click **Bake Lighting** at the top of the Lighting page, or run **Bake Lighting** from the command palette.

The status bar shows how far it has gone; **Cancel Bake** (the same button) or **Cancel Lighting Bake** in the palette gives it up, and the level keeps the bake it had. A room bakes in well under a second on a card with ray tracing; a large level in a few seconds.

When it is done, the bake is saved beside the level, as `Level.lightmaps` next to `Level.level`, and the level is lit by it at once. The level opens with it from then on, also from bundles, and builds ship it with the level. Save the scene first: the bake finds each Obj by its scene and its place in it. Bake again after moving or changing still Objs.

## Real-time, baked or both

**Bounce from**, in the Lighting page's Baked tab (`source` in `[post.global_illumination]`), picks where the level's bounced light comes from:

| Choice | What lights the level |
| --- | --- |
| **Real-time** | The probes round the camera, as the game runs: every light's bounce, moving or not. The bake, if there is one, is not used |
| **Baked** | The bake: lightmaps on Objs with a Lightmap, light probes for the rest. Nothing is traced as the game runs, so it costs least |
| **Both** | The bake for the sky and the lights it keeps, and the probes round the camera for **Realtime** lights only: a torch carried through a baked room still bounces |

With **Both**, no light is counted twice: the bake holds the sky's light and that of every **Mixed** and **Baked** light, and the probes round the camera only that of **Realtime** lights.

## Lights and the bake

Every light has a **Mode**:

| Mode | What the bake keeps |
| --- | --- |
| **Mixed** (the default) | Its bounce. It still lights directly in real time, with its sharp shadows and its shine |
| **Baked** | Its bounce, and its direct light on lightmapped surfaces, which then leave it out. Its shadows there are as soft as the light is wide. Moving Objs are still lit by it in real time |
| **Realtime** | Nothing. Use it for lights that move or change; with **Both**, the probes round the camera take its bounce |

The sun is a light like any other: a sun that moves through the day should be **Realtime**, with **Both**.

## Lightmap settings

Per level, in the Lighting page's **Baked** tab:

| Setting | What it does |
| --- | --- |
| Texels a metre | How many lightmap texels each metre of a still surface gets. 10 by default; more shows finer shadows and takes longer and more memory |
| Bounces | How many times light bounces. 3 is most of it |
| Samples | How many paths are traced from each texel; probes trace twice as many. 256 by default; more is smoother |
| Directional | Keeps which way the light mostly comes from, so normal maps still show under baked light. A second, smaller texture |
| Denoise | Smooths the bake's noise, across texels of like surfaces only, so corners and shadow edges stay |
| Lightmap size | The most texels across one lightmap. A level that needs more is given several |

Per Obj, on its **Lightmap**:

| Setting | What it does |
| --- | --- |
| Scale in lightmap | Multiplies the level's texels a metre for this Obj: 2 for twice the detail each way, 0.5 for a large plain wall |
| Casts | Whether it shades and bounces light in the bake. An Obj without a Lightmap neither shades nor bounces in it |
| Receives | Whether it takes a lightmap. Off, the light probes light it, as they do a moving Obj |

## Seeing the texels

The Scene view's shading menu has **Lightmap Density**: every surface with a Lightmap shows its texels as a checker, green where it has the level's texels a metre, bluer where fewer, redder where more. What the probes light is grey. Use it to spot a wall stretched thin, or a small prop given more texels than it needs.

## Lightmap UVs

A lightmap is laid over a mesh by a second set of UVs in which no two triangles overlap. A model's **Lightmap UVs** import setting picks where they come from:

- **Import or generate** (the default): the file's own second UVs, where it has them, or made on import.
- **Generate**: always made, whatever the file has.
- **None**: none, so the model is lit by probes even with a Lightmap.

Made ones cut each mesh into pieces facing about one way, lay each flat at its true size, and pack them apart with room round each, so light from one does not bleed into another. Built-in cubes, spheres and planes have them too.

## Light Probe Groups

A group places probes two ways:

- **Fill box**: a grid filling the Obj's box, **Spacing** metres apart, the box's sides each with a row. The Scene view draws the box and a dot for each probe.
- **By hand**: points in the Obj's own space, in the scene file or from a script.

Put probes where moving Objs go, a little apart from walls: a probe inside a wall is left out of the bake. More probes where the light changes quickly, as by a window or under a lamp; fewer out in the open.

Each Obj without a lightmap is lit by the eight nearest probes, each counting less the farther it is, so its light changes smoothly as it moves.

## What a bake sees

A bake traces against the meshes of Objs whose Lightmap casts, and terrain and worlds of blocks. Objs without a Lightmap, plants and particles are left out, so a character standing in a room while it bakes leaves no shadow in it. Baked light reaches the meshes of Objs with a Lightmap, and the probes reach other Objs with meshes; terrain, worlds of blocks, water and particles take the sky's light where the level is baked alone.

## From the terminal

```text
vox editor call lighting.bake
vox editor call lighting.bake_status
vox editor call lighting.bake_cancel
vox editor call lighting.probe_light position=[0,1.5,0]
```

`lighting.bake` answers once the bake is written, with its file, its size, how many lightmaps, texels and probes it made and how long it took. See [[Driving the Editor from a Terminal]].

`vox render --debug-view lightmaps` renders the density view.

## From scripts

```csharp
namespace MyGame;

using System.Numerics;
using Voxear;

public sealed class Lantern : Behaviour
{
	public void Awake()
	{
		// A lamp the bake keeps, its bounce and its light on the walls.
		Obj.Light.Mode = LightMode.Baked;
	}

	public void Update()
	{
		if (!BakedLighting.IsBaked)
		{
			return;
		}
		// How lit the spot the player stands on is, from the probes round it.
		var light = BakedLighting.ProbeLightAt(Obj.Transform.Position);
		var hidden = light.Irradiance(Vector3.UnitY).Length() < 0.05f;
	}
}
```

`Obj.Lightmap` and `Obj.AddLightmap()` give an Obj's Lightmap, with `Scale`, `Casts`, `Receives` and `IsBaked`. `Obj.LightProbeGroup` and `Obj.AddLightProbeGroup()` give its probes, with `Fill`, `AddPoint`, `ClearPoints` and `ProbeCount`. `BakedLighting` says how many lightmaps and probes the level has, gives the probes' light at any point, and in the editor whether a bake is under way and how far it has gone.

## In scene files

```toml
[[obj]]
name = "Hall Wall"

[obj.renderer]
mesh = "Models/Hall.glb#mesh0.0"

[obj.lightmap]
scale = 2.0          # times the level's texels a metre
cast = true
receive = true

[[obj]]
name = "Hall Probes"
scale = [12.0, 4.0, 20.0]

[obj.light_probe_group]
fill = 2.0                       # metres apart; leave out for points only
points = [[0.0, 1.5, 0.0]]

[[obj]]
name = "Lantern"

[obj.light]
type = "point"
mode = "baked"       # "realtime", "mixed" (the default) or "baked"
```

And in the level's lighting settings or the scene's own:

```toml
[post.global_illumination]
enabled = true
source = "baked"           # "realtime", "baked" or "both"
texels_per_metre = 10.0
bounces = 3
samples = 256
directional = true
denoise = true
lightmap_size = 1024
```

## The file

`Level.lightmaps` holds each lightmap as the graphics card reads it, compressed (BC6H for the light, BC7 for its directions, a byte a texel each), each part's place in them, and each probe's place and light as half floats, all compressed together. A room of a few walls is a few hundred kilobytes.
