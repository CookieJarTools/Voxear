---
order: 8
---

**Project Settings** holds the settings of the game itself: how it sounds, how fast time steps, how far it draws, which layers collide, which languages it is in. They are saved in the project's `voxear.toml`, so they go into version control and every build. Your own editor's settings are in [[Preferences]].

Open it from the side bar on the far left (the modules button), or type *Project Settings* in the command bar. It opens over the left of the editor, as Levels and Lighting do, and the same button or its close button puts it away.

## Finding a setting

The categories are down the left. Pick one to see its settings on the right, in titled sections. Settings most games leave alone are folded under **Advanced** at the end of their section.

The search box at the top looks through every category at once. It matches a setting's name, its section and category, the words of its tooltip and words that mean the same, so *jitter*, *firewall*, *seams* or *drift* find the settings that talk about them, and under a setting found by its tooltip, the line that matched shows with its words marked. The command bar finds settings the same way across the whole editor (see [[A Tour of the Editor#Finding a setting|Finding a setting]]). While you search, the list keeps only the categories with something found, and each result shows under its category and section.

Rest the pointer on a setting's name to see what it does, what more or less of it costs, and its default. A setting changed from its default has a bar beside its name. Right-click a setting for **Reset to Default**, and use the **⋯** button beside the category's name to reset the whole category or open its page of the manual.

## Categories

| Category | What is in it |
| --- | --- |
| Audio | Headphones, room reverb and its mode, ray-traced sound and its GPU budget, Steam Audio in builds, the most voices at once and (under Advanced) the speed of sound |
| Content | How builds pack assets: compression, merging small bundles and splitting large ones, and (under Advanced) patterns of assets always included or left out |
| Foliage | How plants move: the wind on a fair day, how far they bend, and whether what walks through them pushes them aside (see [[Wind]]) |
| Graphics | Vsync and HDR output, the GPU's mesh shaders, decompression and ray tracing (under Advanced), and what drawing through [[Portals]] and [[Trails|trails on water]] may spend |
| Input | The input actions the game plays with, a button that opens the Input Actions window, the press point and the sticks' deadzone |
| Localization | The languages the game is in, the one it is written in first, and the fonts each falls back to (see [[Fonts]]) |
| Modules | A toggle for each of the engine's optional systems (see [[Modules]]) |
| Network | Tick rate, port, most players, bandwidth and authority (see [[Playing over the Network]]) |
| Physics | Gravity, and the Layer Collision Matrix: which layers' Objs collide |
| Physics 2D | Gravity for 2D physics (see [[2D Physics]]) |
| Player | The game's name, the level it opens on, and a button that opens the Build window, where each build profile sets its window, version and defines |
| Quality | What making block worlds may spend, as a preset or number by number (see [[Block Worlds]]), and the most particles |
| Scripts | The game's C# project, and a button that opens it in your code editor |
| Tags and Layers | The project's 32 layers |
| Time | The fixed step, the longest a frame may count as, the time scale, and the most fixed steps a frame |
| World | The floating origin (see [[Floating Origin]]) |

Most settings take effect at once. Those the game reads as it starts, such as gravity, the fixed step, the network and the floating origin, take effect the next time you press Play.

## Time

Physics, `FixedUpdate` and everything else that must not depend on the frame rate run in fixed steps. **Fixed step** is how long each one is: 0.02 seconds, 50 a second, by default. A shorter step is smoother and more exact, and costs more CPU. A networked game steps at the network's **Tick rate** instead.

A frame longer than **Longest frame** counts as that long, so a hitch or a stop at a breakpoint does not throw everything forward. No frame runs more than **Fixed steps a frame** fixed steps (under Advanced); the rest are dropped, so a slow machine does not fall further and further behind. None are dropped while networked.

**Time scale** is how fast the game's time goes as it starts: 1 as fast as real time, 0.5 slow motion, 0 stopped. Frames' and fixed steps' time go by it; scripts change it as the game runs with `Time.TimeScale`, and `Time.UnscaledDelta` keeps to the clock.

```toml
[time]
fixed_step = 0.01
max_step = 0.1
time_scale = 1.0
```

## Gravity

**Physics ▸ Gravity** is the pull every rigidbody falls with as a world starts, in metres per second per second: Earth's 9.81 down by default. **Physics 2D ▸ Gravity** is the same for 2D physics. Scripts can change either while the game runs with `Physics.gravity`.

```toml
[physics]
gravity = [0.0, -1.62, 0.0]   # the Moon's
```

## Foliage

Grass, flowers, bushes and leaves sway in the wind and part round whatever walks through them. **Wind** is how hard the wind blows over a level with no Wind Zone on a fair day, as a Wind Zone's Strength: 0.5, a light breeze, by default. Rain and storms blow harder on top, so plants toss in a storm with nothing set up; at 0 they stand still until the weather turns. **Bend** is how far every plant bends, in the wind and when pushed, times its own **Wind bend**. **Interaction** lets characters, and Objs with a Foliage Interaction, push plants aside; under Advanced, **Push strength** says how hard and **Spring back** how long a pushed plant takes to stand again. See [[Wind]].

```toml
[foliage]
wind = 1.0          # breezy on a fair day
bend = 1.2
interaction = true
```

Scripts can change all of it as the game runs, with `Foliage.Wind`, `Foliage.Bend`, `Foliage.Interaction`, or `Foliage.Motion` for everything at once; the project's settings stay as they are.

## Water trails

Whatever moves on water leaves ripples, a V of waves and foam behind it, worked out over a square of water round the camera. **Quality** is how finely: Low, Medium or High, 256, 512 or 1024 texels a side, or Off for none. **Area** is how far across the square is, 64 m by default. **Look** is how they are drawn: **Match the Water**, the default, stands them in small squares a quarter of a block across on worlds of blocks and keeps them smooth on the sea, Pools and Streams; **Smooth** and **Blocky** draw them one way on all water. See [[Trails]].

## Input actions

A project with several `.inputactions` files picks the one the game plays with in **Input ▸ Actions file**. With none picked, the game uses the first by path, or Voxear's own when the project has none.

**Press point** is how far a button or trigger goes before it counts as pressed, 0.5 by default. **Stick deadzone** is two numbers: the part of a stick's travel read as nothing, against drift, and the point from which it reads as all the way, 0.125 and 0.925 by default. A binding with a `deadzone` processor of its own keeps its own. A game's options change both while it runs with `ProjectSettings.ChangeInput`.

## Graphics

**Vsync** makes frames wait for the display, so a picture never tears. **HDR output** is what a game starts with on a display that has HDR: Off, On (HDR10 first), HDR10 or scRGB; a game's options change it with `Screen.Hdr`. Under Advanced, **Mesh shaders**, **GPU decompression** and **Ray tracing** say whether the GPU's own are used where it has them; each falls back to what works without it when off. These are taken up when a build or `vox run` starts, not by the editor's own window, and the environment still wins: `VOXEAR_VSYNC=0`, `VOXEAR_HDR=on`, `VOXEAR_MESH_SHADERS=0` and so on.

```toml
[graphics]
vsync = false
hdr = "on"
```

## Voices

**Most voices** is how many sounds play at once, at most 256: past it, a new sound takes the place of the oldest effect that does not loop. **Speed of sound** (under Advanced) is what the Doppler effect works from, 343 m/s in air; lower it for a stronger effect.

## What is written

`voxear.toml` keeps only what differs from the default: change a setting back and its line goes, and a section with nothing left in it goes too. The rest of the file, comments and all, is kept as you wrote it, so the file reads well in version control and can be edited by hand. The editor reads it again when it changes on disk.

## From a script

`ProjectSettings` reads the settings the game runs by, a section at a time, each a copy. Portals, water trails, navigation costs and the input's press point and deadzone change while the game runs, as a game's options do, from the next frame until it starts again, and `Time.TimeScale` changes how fast time goes; the time steps, the floating origin, the quality budgets, the graphics and the network are fixed once it has started, and read only.

```csharp
// A slow machine: Portals show their far side from nearer, and the water keeps no trails.
ProjectSettings.ChangePortals(portals => portals.ViewDistance = 48f);
ProjectSettings.ChangeWaterTrails(trails => trails.Quality = WaterTrailQuality.Off);
ProjectSettings.ChangeNavigationCosts(costs => costs.Road = 0.4f);

ProjectSettings.ChangeInput(input => input.StickDeadZone = 0.2f);
Time.TimeScale = 0.5f;

var step = ProjectSettings.Time.FixedStep;
```

| Section | What it has |
| --- | --- |
| `Time` | `FixedStep`, `MaxStep`, `MaxFixedSteps` and `TimeScale`, read only (set `Time.TimeScale`) |
| `Graphics` | `Vsync`, `Hdr`, `MeshShaders`, `GpuDecompression` and `RayTracing`, read only |
| `Input` | `PressPoint`, `StickDeadZone` and `StickFullAt`, changed with `ChangeInput` |
| `FloatingOrigin` | `Mode`, `Distance` and `Step`, read only |
| `Quality` | `Particles`, `BlockLighting` and `PosedVertices`, read only |
| `Network` | `TickRate`, `Port`, `MaxPlayers` and `KilobytesPerSecond`, read only |
| `Portals` | Every Portals setting, changed with `ChangePortals` |
| `WaterTrails` | `Quality`, `Area` and `Look`, changed with `ChangeWaterTrails` |
| `NavigationCosts` | What roads, water, swimming, doors and the project's own four areas cost, changed with `ChangeNavigationCosts` |

## From the command line

Every setting here can be read and changed by tools, as `vox editor call` and `vox mcp` reach the editor (see [[Driving the Editor from a Terminal]]):

```
vox editor call project_settings.get category=Time
vox editor call project.set key=time.fixed_step value=0.01
vox editor call project.set key=audio.reverb_mode value=Convolution
vox editor call project.set key=physics.gravity value=[0,-1.62,0]
vox editor call project.set key=foliage.wind value=1.5
vox editor call settings.show key=physics.gravity
```

`project_settings.get` lists each setting with its key, category, section, label, tooltip, type, value, default, and its options or range and unit. A key is the setting's table and name in `voxear.toml`. `project.set` takes a choice by its label or its word in the file, an asset by its path, and `null` for the default; a value outside a setting's range is refused, with the range.

## What is not here yet

Some settings a game may look for have no place in the engine yet, so they are not here either: named quality levels (Low to Ultra, chosen by players) and values per quality level; per-platform values; one settings file per category; the window's start size, full screen and resizing, which each build profile sets in the Build window for now; the icon, splash and cursor; solver iterations, sleep and default friction for physics; the audio's sample rate and buffer size; the C# language version, defines per platform and the script execution order; shader variants kept or stripped; and a game's own settings in a category of their own.
