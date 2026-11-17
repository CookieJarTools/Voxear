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
3. Pick a channel. **Stable** is for making games, and **Beta** is the next stable version being tried out. **Nightly** is built every night from the latest work. It shows once you turn on **Nightlies** in **Settings**, or click **Show nightlies** at the bottom of the list, and by itself while you have a nightly installed.
4. Leave **Voxear Manual** ticked under the version to install this manual with it. You read the manual in VoxDocs, inside the editor.
5. Click **Install** beside the version you want.

The launcher checks every download before it unpacks it, so a damaged or tampered file is never installed.

You can install as many versions as you like. Each project remembers which one it uses, so a new version never changes a project you have not moved to it yourself.

To add a version you already have on disk, use **From a file…** for a downloaded archive, or **Locate an engine…** for an engine folder, such as one you built yourself.

## Custom engine releases

The launcher lists Voxear's own releases without any setting up. **Custom engine releases** list someone else's beside them, such as the releases of a fork of the engine. They are published the way Voxear's are, as a signed `releases.toml`, usually by the engine's own release workflow run from the fork.

1. Go to **Settings** and click **Add** under **Custom engine releases**.
2. For **Address**, give the GitHub repository they publish from (`owner/repo`), the address of their release site, or their `releases.toml` itself, which can also be a file on disk.
3. For **Public key**, give the key their releases are signed with. It starts with `RW`. Whoever publishes the releases can tell you it.
4. **Name** is what the list of versions calls them. Leave it empty to take one from the address.
5. Click **Add**. The launcher reads the releases and checks their signature before it adds them, so a wrong address or key shows straight away.

Their versions then appear in **Install a version** next to Voxear's, each tagged with the name you gave, and the engines you install from them carry the same tag in **Installs**. A project made with one of their versions installs it from them when it needs it. **Remove** stops listing them; engines already installed from them stay.

A fork's builds are best given versions of their own, such as `0.2.0+lumen`, so they never share a number with one of Voxear's. When two lists do offer the same version as different engines, both appear in **Install a version**, and you pick the one to install.

`voxup` does the same:

```bash
voxup releases add someone/VoxearReleases --name Lumen --key RW…
voxup releases
voxup install 0.2.0+lumen
voxup install stable --from Lumen
voxup releases remove Lumen
voxup releases nightlies on
```

The settings are kept in `settings.toml` in the launcher's folder (**Settings**, under **About**, shows where), as `nightlies = true` and a `[[custom_releases]]` table with `name`, `url` and `key` for each list.

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

## Installing Whisk

Whisk is the code editor Voxear opens your scripts in. It is not part of an engine version: one copy serves every version, and it keeps itself up to date once installed.

In the launcher, go to **Settings** and click **Install** under **Code editor**. **Open** starts it, and a project's menu in **Projects** has **Open in Whisk**. **Remove** takes away the copy the launcher installed; a Whisk you installed yourself is left alone, and the launcher finds and uses it.

```bash
voxup whisk install
voxup whisk open ~/VoxearProjects/MyGame
voxup whisk remove
```

## Checking your computer

Each version of Voxear comes with a command-line tool called `vox`, in the `bin` folder of the install. (**Show in files** on a version's menu in **Installs** opens that folder.) Run this to check that your graphics card and .NET are ready:

```bash
vox doctor
```

It marks anything missing with ✗ and says how to fix it.

Next: [[Your First Project]].
