---
order: 1
---

You install Voxear, and keep it up to date, with the **Voxear Launcher**. The launcher holds every version of the engine you have installed, side by side, and opens each of your projects with the version it was made for.

## What you need to run Voxear

- **Linux or Windows 10/11**, on a 64-bit PC.
- **A graphics card with Vulkan 1.3**, with its current driver.
- **The .NET 10 SDK**, which builds your game's C# scripts. You can open a project without it, but you need it to play or build the game.

On Linux, install .NET with your package manager, as `sudo pacman -S dotnet-sdk` on Arch. On Windows, run `winget install Microsoft.DotNet.SDK.10`.

## Installing a version of the engine

1. Open the launcher and go to **Installs**, in the side bar.
2. Click **Install a version**.
3. Pick a channel. **Stable** is for making games, **Beta** is the next stable version being tried out, and **Nightly** is built every day from the latest work.
4. Leave **Voxear Manual** ticked under the version to install this manual with it. You read the manual in VoxDocs, inside the editor.
5. Click **Install** beside the version you want.

The launcher checks every download before it unpacks it, so a damaged or tampered file is never installed.

You can install as many versions as you like. Each project remembers which one it uses, so a new version never changes a project you have not moved to it yourself.

To add a version you already have on disk, use **From a file…** for a downloaded archive, or **Locate an engine…** for an engine folder.

## Adding or removing the Voxear Manual

The manual is an optional part of each version. A version that has it shows a **Manual** tag in **Installs**.

To add it later, open the version's menu in **Installs** and pick **Add the Voxear Manual**. **Remove the Voxear Manual** takes it away again. Without it, VoxDocs shows only your project's own docs.

The launcher's command-line tool, `voxup`, does the same:

```bash
voxup install 0.1.0 --without-manual
voxup manual add 0.1.0
voxup manual remove 0.1.0
```

The launcher can only change versions it installed itself. An engine folder added with **Locate an engine…** keeps the files it has.

## Checking your computer

Each version of Voxear comes with a command-line tool called `vox`, in the `bin` folder of the install. (**Show in files** on a version's menu in **Installs** opens that folder.) Run this to check that your graphics card and .NET are ready:

```bash
vox doctor
```

It marks anything missing with ✗ and says how to fix it.

Next: [[Your First Project]].
