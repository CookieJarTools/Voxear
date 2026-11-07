---
order: 5
---

A build is your game on its own, as a program anyone can run without Voxear.

## Building the game

1. Click **Build**, at the end of the bar along the top of the editor.
2. If the project has no build profiles yet, click **Add the default profiles**.
3. Pick a profile from **Profiles**, and click **Build**.

The Build panel shows each stage as it goes (**Scripts**, **Content**, then **Player**) and ends with how long it took and how big the game is. **Run** starts the built game, and its messages go to the Console. **Show in Files** opens the folder it is in.

The build runs in a program of its own, so the editor stays usable while it works, and **Cancel** stops it.

## Build profiles

A profile is a set of choices for one kind of build. New projects come with six, kept in `Settings/BuildProfiles/`:

| Profile | What it is for |
| --- | --- |
| Linux Release | The game as players get it |
| Linux Development | Testing: adds a stats overlay on F1, debug views and a free camera |
| Windows Release | The game as players get it |
| Windows Development | Testing, as above |
| Linux Server | A dedicated server: no window or sound (see [[Server Builds]]) |
| Windows Server | A dedicated server, as above |

Select a profile to change it: the platform, which levels go in and which one starts, the product name, company and version, whether the game starts in a window or full screen, and where the build goes.

- **Start level** and **Levels** are level fields: click one to pick from your project's levels, or drag a level onto it from the Project panel. **Add a level** adds another, and the × beside a level takes it out. With no levels listed, every level ships.
- **Defines** are words your scripts can test with `#if`, as `DEMO`. **Add a define** adds one. See [[How Scripts Run]] for the ones Voxear defines itself.
- **New**, under the profiles, makes a profile based on the one picked, so it starts with its settings and you change only what differs. Type its name in the title at the top, and press Enter. **Remove** takes the picked profile out, unless another is based on it.

The window size and full screen are how the game starts. Your scripts can change them while it runs, as a settings menu does: see [[The Window and the Mouse]].

For now, a build is made on the platform it is for: build the Windows profiles on Windows, and the Linux ones on Linux.

## What a build makes

Each build goes in `Builds/` in your project, in a folder named after its profile:

| File | What it is |
| --- | --- |
| The game | The program to run, named after the product |
| `Data/` | Your levels, models and other content, and your scripts |
| `build-report.json` | What went into the build, and how long each stage took |

> [!NOTE]
> For now, players need .NET installed to run a build. Later versions will include it with the game.

## Building from the command line

`vox` builds the same way the editor does, which suits build servers:

```bash
vox build MyGame
vox build MyGame --profile "Linux Development"
vox build MyGame --list
```

With no `--profile`, it builds the Release profile for the platform you are on. `--list` shows the project's profiles.
