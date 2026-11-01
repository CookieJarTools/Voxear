---
order: 2
---

## Making a project

1. In the launcher, go to **Projects** and click **New project**, or press Ctrl+N.
2. Give it a **Name** and pick a **Location**. New projects go in a `VoxearProjects` folder in your home folder unless you choose another.
3. Pick the engine version to make it with.
4. Pick a template:
   - **3D**: a lit scene with a ground and a turning cube. Start here if you are new.
   - **Empty**: nothing but a camera.
5. Under **Default setup**, choose what the first scene starts with: **Lighting and sky**, a **Camera**, and the **Sample scene**.
6. Under **Extras**, pick the **C# IDE** you write code in, if any, and whether to make a **Git repository** for the project.
7. Click **Create project**, or press Ctrl+Enter.

## Opening a project

Double-click a project in the list, or select it and click **Open**. The editor opens on the project's first level.

Each project shows the engine version it uses. If that version is not installed, the launcher offers to install it before opening the project.

To open a project that is not in the list, such as one you cloned from git, click **Add** and pick its folder.

## What is in a project

A project is an ordinary folder. A new one holds:

| What | What it is for |
| --- | --- |
| `voxear.toml` | The project's settings: its name, its first level, and the engine version it uses |
| `Levels/` | Levels: what the game loads. Each lists the scenes it needs. See [[Scenes and Levels]] |
| `Scenes/` | Scenes: the Objs that make up your game's world |
| `Scripts/` | Your C# code, and the project file that builds it |
| `Settings/` | Settings shared with your team, such as how to build the game |
| `Docs/` | Your own pages in VoxDocs, once you write the first one |
| `.voxear/` | Files for your machine alone: built scripts, caches and editor layout |

Every asset also has a `.meta` file beside it. It holds the asset's ID, which is how other files refer to it, so keep each `.meta` with its asset when you move or rename one. Renaming in the editor's Project panel does this for you.

If you made a git repository, `.voxear/` and the C# build folders are left out of it, as no one else needs them.

A new `voxear.toml` looks like this:

```toml
[project]
name = "My Game"
scripts = "Scripts/MyGame.Scripts.csproj"
start_level = "Levels/Main.level"
# The Voxear version this project uses, which the launcher opens it with.
engine = "0.1.0"
```

> [!WARNING]
> Voxear will not open a project whose `voxear.toml` has a setting it does not know, so a misspelt setting stops the project opening. The error names the setting.

## Without the launcher

The `vox` tool can make and open projects too:

```bash
vox new MyGame --template 3d --git
vox editor MyGame
```

Next: [[A Tour of the Editor]].
