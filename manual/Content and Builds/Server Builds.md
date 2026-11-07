---
order: 3
---

A **server build** is your game as a dedicated server runs it: no window and no sound, ticking sixty times a second until it is stopped. It leaves out everything only players see or hear, so it is small and starts in a moment.

## Making one

New projects come with a **Linux Server** and a **Windows Server** profile. Each is based on its platform's Release profile, with one setting changed:

```toml
format = 1
base = "Linux Release"
target = "server"
```

Build it as any other profile, in the Build window or with:

```
vox build MyGame --profile "Linux Server"
```

Run the program in the build's folder to start the server. `--port 7777` picks the port it listens on, the project's otherwise, and `--frames 600` stops it after that many ticks, which is handy for checking a build.

## The server program

A server build ships the server program rather than the player. It never opens a window or touches a GPU, so it runs on a machine with neither, and it comes without the debug information the player carries, at about a seventh of the size. It is the **Dedicated Server** part of an engine install, which the launcher installs on its own and can add to an install later. Without it, a server build ships the player program, which serves just as well, and the build says so.

One core serves about eight full matches of 64 players at 60 ticks a second.

## What a server leaves out

A server keeps what makes up your levels and what happens in them: levels, scenes, prefabs, scripts, input actions, string and asset tables. Models ship without their pictures, keeping their shapes for colliders and their skeletons and clips for animation.

It leaves out pictures, materials, custom shaders, sounds and the paths sound takes round a level, light profiles, lighting settings and UI documents. These stay listed in the build, so a scene that names one still loads: a sound source simply has no sound, and the server is lit by the default lighting it never draws. A mesh collider still gets its mesh's shape, though nothing draws the mesh.

The build report, `build-report.json`, lists what was left out and how many bytes that saved.

## Keeping an asset in server builds

If your server code needs something a server build would leave out, select the asset and turn on **Server builds** under **Builds** in the Inspector. It then ships whole in server builds: a model keeps its pictures, a sound its sound. The setting is kept in the asset's `.meta`:

```toml
[build]
server = true
```

## Code only players need

Scripts in a server build are compiled with `VOXEAR_SERVER`, so you can leave out code a server has no use for:

```csharp
#if !VOXEAR_SERVER
	HealthBar.Show(player);
#endif
```

Code that loads a UI document, a sound or a picture by its path should be left out this way, since a server build does not have them.
