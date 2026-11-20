---
order: 4
---

With **ray-traced audio** on, sound is heard through the same scene the GPU traces rays against for the global illumination: every model, every world of blocks and the terrain, not only the colliders. A cave dug into a block world rings like a cave, a dungeon's stone corridor muffles what is round the corner, and a wall that is only a model, with no collider, still stands between a sound and the listener.

Each frame, the GPU traces two kinds of rays from the listener:

- **Occlusion**: for each 3D Audio Source with **Occlusion** on, rays to the sound, one for Raycast and several across its sphere for Volumetric. A ray that meets a wall brings through what the wall's acoustic material lets through, a wall at a time.
- **Reverb**: rays bounced round the room, each surface sending back what it hears. Added up over a few frames, they give how long the room rings in each band of pitch, low, middle and high, and the room's first echoes: the near walls' answers in the first 80 milliseconds, each from its own side.

What the rays find comes back two or three frames later, too soon for anyone to hear the delay.

## Turning it on

For the whole project, in `voxear.toml`:

```toml
[audio]
reverb = true
ray_traced = true
ray_quality = "medium"   # low, medium or high
ray_budget = 0.3         # the most GPU time a frame, in milliseconds
```

A level can set its own, over the project's, in its `.level` file. Each one left out takes the project's:

```toml
[audio]
ray_quality = "high"
```

The level's Inspector shows under **Sound** whether it is ray-traced, at what quality, and whether the setting is the level's own or the project's. Under **Audio** it changes the level's own: **Ray Traced**, **Ray Quality** and **Ray Budget**, each **Project's** or empty for the project's, written into the `.level` file at once.

A script can change it while the game runs, for a sound options menu (see [[Playing Sounds from Scripts]]):

```csharp
Audio.RayTraced = Save.Read("ray-traced-audio") == "on";
Audio.RayQuality = RayQuality.High;
```

| Quality | Rays a frame | Bounces | Rays across a volumetric sound |
| --- | --- | --- | --- |
| Low | 256 | 6 | 4 |
| Medium | 1,024 | 10 | 8 |
| High | 4,096 | 16 | 16 |

Higher hears small rooms and narrow passages more steadily. Lower settles a moment later after you walk into a new room, as fewer rays are added up over more frames.

## The budget

The rays never take more than the **budget** of GPU time a frame. Voxear reads the pass's time back each frame, and while the shortest of the last 60 is over the budget, it traces fewer rays; while it is well under, it traces more again, up to what the quality allows. The default, 0.3 milliseconds, is a twentieth of a frame at 144 frames a second. The Audio panel shows the rays traced now and what they took, and `Audio.Rays` and `Audio.RayCost` read them from a script.

On an RTX 3070, in a closed 20-metre room with 200 boxes in it and 16 sounds, half of them volumetric, the pass took (shortest of 60 frames):

| Quality | Ray queries | In software |
| --- | --- | --- |
| Low | 0.07 ms | 0.73 ms |
| Medium | 0.14 ms | 1.63 ms |
| High | 0.65 ms | 3.08 ms |

Without ray tracing on the GPU, the rays are traced in a compute shader, which takes many times as long. If even the fewest rays take longer than the budget, ray-traced audio steps aside until its settings change: `Audio.RayTracing` reads false, and rooms ring by their baked Reverb Probes (below). With the default budget, that is what a GPU without ray tracing does, unless you raise the budget for it.

## What surfaces are made of

A surface sounds as its **acoustic material** says: how much of each band it soaks up, how much it scatters, and how much passes through it. Pick one on a material, under **Acoustic** in its Inspector, or in its file:

```toml
acoustic = "concrete"
```

The choices are Generic, Brick, Concrete, Ceramic, Gravel, Carpet, Glass, Plaster, Wood, Metal and Rock. Left at **By look**, a material is heard as Metal where it is metallic, Glass where it is glass, and Generic otherwise. Cut-out materials, such as leaves and fences, and foliage let sound through as though they were not there.

A world of blocks is heard a chunk at a time: each chunk as the material most of its faces are drawn with, and as Rock where that material gives none, as most of a world of blocks is stone and earth.

## Each sound

| Setting | What it does |
| --- | --- |
| Occlusion | Off, Raycast or Volumetric, as without ray tracing (see [[Sounds and Audio Sources]]) |
| Transmission | Whether walls let some of the sound through, as their materials do |
| Reverb send | How much of the sound the room rings with, from 0 to 1. Lower it for a voice heard close up that the hall should not answer |

A sound with **Pathing** on, in a level with baked paths, is still heard round the walls by Steam Audio, along its paths (see [[Sound Round Corners]]).

## Reverb probes

Not every machine has the GPU time to spare, and ray-traced audio may be off. For those, bake **Reverb Probes**: boxes of probes through the level, each keeping how long sound rings there. With ray-traced audio off, the listener's room rings as the probes round it say, blended by how near each is. Outside every box, the room rings as Steam Audio works it out from the colliders, as before.

Add one from the **+** menu, or **Add Component > Audio > Reverb Probe**. The Scene view shows its box and a small box at each probe.

| Setting | What it does |
| --- | --- |
| Size | How far the box reaches along each axis of the world, in metres, round the Obj |
| Spacing | How far apart its probes are, in metres. Closer hears small rooms better, and bakes for longer |

Then open the level and run **Bake Reverb Probes** from the command palette (Ctrl+K), or select the level in the Project panel and click **Bake Reverb** under **Sound** in the Inspector. Each probe is traced on the GPU with 8,192 rays, nine probes a frame, so a few hundred probes bake in a second or two. The probes are saved beside the level, named after it: `Levels/Main.level` gets `Levels/Main.reverb`, six bytes a probe. A probe inside a wall hears nothing back, and is left out of the blend.

Bake again after you change the level's rooms. A build packs each level's `.reverb` file with it.

From a script, `Obj.AddReverbProbe()` gives an Obj one:

```csharp
var cave = Obj.Find("Cave").AddReverbProbe();
cave.Size = new Vector3(40f, 12f, 30f);
cave.Spacing = 4f;
```

## From a terminal

`vox editor call reverb_probes.bake` bakes the open level's probes, and `vox editor call audio.rays` says what ray-traced audio does in the running game, and sets it there with `on`, `quality` and `budget` (see [[Driving the Editor from a Terminal]]).

## Steam Audio

Ray-traced audio needs no Steam Audio: the GPU traces the rays and Voxear works the room out from them itself, in the same bands and the same way Steam Audio does. With Steam Audio there, headphones still hear 3D sounds through its HRTF, and pathing still follows the baked paths. Convolution reverb plays Steam Audio's own echoes, from the colliders; with ray-traced audio on, the room rings by the rays' reverb times and first echoes instead.
