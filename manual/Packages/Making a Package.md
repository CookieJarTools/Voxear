---
order: 2
---

A package is a folder with a `package.toml` at its top. Start one with:

```
vox package new "Orbit Tools"
```

This makes the folder with a `package.toml`, a README, a first script and a first page. To try the package in a game while you write it, add the package by its folder:

```
vox package add "../Orbit Tools"
```

## package.toml

```toml
[package]
name = "orbit-tools"
version = "1.2.0"
description = "Cameras that orbit, follow and frame"
authors = ["Sam Rivers"]
engine = ">=0.1"

[dependencies]
springs = "^0.3"
```

- **name**: lower-case letters, digits and hyphens, starting with a letter. Projects use this name to ask for the package.
- **version**: three numbers, major.minor.patch. Raise the major number when a change could break projects that use the package. Raise the minor number for new features, and the patch number for fixes.
- **engine**: the Voxear versions the package works with. A project on another version cannot add it.
- **dependencies**: other packages this one needs, written as in a project's `packages.toml`.

## What goes where

| Folder | What it holds |
|---|---|
| `Scripts/` | C#: behaviours, components, editor windows, side bar items, drawers and commands. |
| `Docs/` | Pages, which the editor shows beside the manual under the package's name. |
| `Samples/<name>/` | Samples, which a project copies in to try and change. |
| Anything else | Assets, such as models, materials, sounds and prefabs. |

A package's scripts build into an assembly of their own, named after the package: `orbit-tools` builds as `OrbitTools`. The game's scripts can use every package's types. A package's scripts can use the types of the packages it lists under `dependencies`.

Give your scripts a namespace named after the package, so their names do not clash with the game's.

## Samples

A sample is a small, working example: a level and the scripts and assets it needs, in a folder under `Samples/`. A project copies a sample into its own `Samples/<Package>/<Sample>` folder. The project then owns the copy and can change it, while the package itself stays as it is.
