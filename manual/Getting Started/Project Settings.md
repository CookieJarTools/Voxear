---
order: 8
---

**Project Settings** holds the settings of the game itself: how it sounds, how fast time steps, how far it draws, which layers collide, which languages it is in. They are saved in the project's `voxear.toml`, so they go into version control and every build. Your own editor's settings are in [[Preferences]].

Open it from the side bar on the far left (the modules button), or type *Project Settings* in the command bar. It opens over the left of the editor, as Levels and Lighting do, and the same button or its close button puts it away.

## Finding a setting

The categories are down the left. Pick one to see its settings on the right, in titled sections. Settings most games leave alone are folded under **Advanced** at the end of their section.

The search box at the top looks through every category at once. It matches a setting's name, its section and category, and the words of its tooltip, so *jitter*, *firewall* or *seams* find the settings that talk about them. While you search, the list keeps only the categories with something found, and each result shows under its category and section.

Rest the pointer on a setting's name to see what it does, what more or less of it costs, and its default. A setting changed from its default has a bar beside its name. Right-click a setting for **Reset to Default**, and use the **⋯** button beside the category's name to reset the whole category or open its page of the manual.

## Categories

| Category | What is in it |
| --- | --- |
| Audio | Headphones, room reverb and its mode, ray-traced sound and its GPU budget, Steam Audio in builds |
| Content | How builds pack assets: compression, merging small bundles and splitting large ones, and (under Advanced) patterns of assets always included or left out |
| Foliage | How plants move: the wind on a fair day, how far they bend, and whether what walks through them pushes them aside (see [[Wind]]) |
| Graphics | What drawing through [[Portals]] may spend |
| Input | The input actions the game plays with, and a button that opens the Input Actions window |
| Localization | The languages the game is in, the one it is written in first, and the fonts each falls back to (see [[Fonts]]) |
| Modules | A toggle for each of the engine's optional systems (see [[Modules]]) |
| Network | Tick rate, port, most players, bandwidth and authority (see [[Playing over the Network]]) |
| Physics | Gravity, and the Layer Collision Matrix: which layers' Objs collide |
| Physics 2D | Gravity for 2D physics (see [[2D Physics]]) |
| Player | The game's name, the level it opens on, and a button that opens the Build window, where each build profile sets its window, version and defines |
| Quality | What making block worlds may spend, as a preset or number by number (see [[Block Worlds]]), and the most particles |
| Scripts | The game's C# project, and a button that opens it in your code editor |
| Tags and Layers | The project's 32 layers |
| Time | The fixed step, the longest a frame may count as, and the most fixed steps a frame |
| World | The floating origin (see [[Floating Origin]]) |

Most settings take effect at once. Those the game reads as it starts, such as gravity, the fixed step, the network and the floating origin, take effect the next time you press Play.

## Time

Physics, `FixedUpdate` and everything else that must not depend on the frame rate run in fixed steps. **Fixed step** is how long each one is: 0.02 seconds, 50 a second, by default. A shorter step is smoother and more exact, and costs more CPU. A networked game steps at the network's **Tick rate** instead.

A frame longer than **Longest frame** counts as that long, so a hitch or a stop at a breakpoint does not throw everything forward. No frame runs more than **Fixed steps a frame** fixed steps (under Advanced); the rest are dropped, so a slow machine does not fall further and further behind. None are dropped while networked.

```toml
[time]
fixed_step = 0.01
max_step = 0.1
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

## Input actions

A project with several `.inputactions` files picks the one the game plays with in **Input ▸ Actions file**. With none picked, the game uses the first by path, or Voxear's own when the project has none.

## What is written

`voxear.toml` keeps only what differs from the default: change a setting back and its line goes, and a section with nothing left in it goes too. The rest of the file, comments and all, is kept as you wrote it, so the file reads well in version control and can be edited by hand. The editor reads it again when it changes on disk.

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
