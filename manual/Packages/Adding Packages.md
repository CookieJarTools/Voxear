---
order: 1
---

A project lists the packages it uses in `packages.toml`, a file beside `voxear.toml`. Each line names a package and where it comes from:

```toml
[packages]
orbit-tools = { git = "https://github.com/someone/orbit-tools", tag = "v1.2.0" }
my-tool = { path = "../my-tool" }
cameras = "^1.0"
```

You can edit the file yourself, or let `vox package add` write it for you.

## Where packages come from

**A Git repository**, by its URL. Pick the commit with one of these:

- `tag`, a release, such as `"v1.2.0"`. Use this most of the time.
- `branch`, which follows the branch as it moves on. The project stays on one commit until you update.
- `rev`, one exact commit.
- Nothing, for the repository's default branch.

If a repository holds more than one package, add `path` to name the package's folder.

```toml
big-kit = { git = "https://github.com/someone/kits", branch = "main", path = "cameras" }
```

Git does the fetching, so private repositories work with the same logins and SSH keys you already use for Git.

**A folder on disk**, by `path`, from the project's folder. Use this for a package you are writing next to a game. Changes you make to the package show in the game straight away.

**A registry**, by a version range. A registry is a list of packages kept in a Git repository, so you can add a package by its name:

```toml
cameras = "^1.0"
effects = { version = "^2", registry = "studio" }

[registries]
studio = "https://github.com/my-studio/packages"
```

`^1.0` takes the newest release from 1.0 up to, but not including, 2.0. Without `registry`, the package comes from Voxear's default registry. A `default` entry under `[registries]` replaces the default registry for this project. The `VOXEAR_REGISTRY` environment variable replaces it for the whole machine.

## From the command line

```
vox package add https://github.com/someone/orbit-tools --tag v1.2.0
vox package add ../my-tool
vox package add cameras
vox package add cameras@^1.2
vox package remove orbit-tools
vox package update
vox package update cameras
vox package list
vox package search camera
```

Every command works on the project in the current folder. Pass `--project` to name another.

`vox package add` shows what each new package holds before it changes anything: its scripts, assets, pages and samples. A package's scripts run when your game's scripts do, so if a new package from Git or a registry has scripts, Voxear asks before adding it. Pass `--yes` to skip the question, for example in a build script.

## The lock file

When packages are added, Voxear writes `packages.lock` beside `packages.toml`. The lock keeps the exact commit of every package, and of every package those packages need. Commit the lock with your project, so everyone on your team, and every build, gets the same code.

Packages stay where the lock puts them until you update them. `vox package update` moves every package to the newest commit or release that `packages.toml` allows. Name packages to update only those.

## Where packages are kept

Packages from Git and registries are fetched once per machine, into a cache that every project on the machine shares. On Linux the cache is `~/.cache/voxear/packages`; on Windows it is `%LOCALAPPDATA%\Voxear\Packages`. Set `VOXEAR_PACKAGE_CACHE` to keep it somewhere else.

Do not change files in the cache. To change a package, add it from a folder instead.

Once the lock and the cache agree, nothing needs the network. `--offline` uses only what is on the machine already.

## Packages that need other packages

A package can need other packages, and those packages can need more. Voxear adds them all, and picks one version of each that suits everyone who asks for it. If your project and a package ask for the same package from different places, your project's choice wins.

When no version suits everyone, the error names who asked for what, for example: `the project needs ^0.1, orbit-tools needs ^0.2`. Change your project's range, or update the package that asks for the other one.
