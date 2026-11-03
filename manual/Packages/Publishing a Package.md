---
order: 3
---

Anyone can add a package by its Git URL, so to share one, push it to a Git host such as GitHub and tag a release:

```
git tag v1.2.0
git push origin v1.2.0
```

Make the tag's version match the version in `package.toml`.

## Listing a package in a registry

A registry lets people add a package by its name rather than its URL. A registry is a Git repository with a file for each package in its `packages` folder. `packages/orbit-tools.toml`, for example, lists each release of orbit-tools:

```toml
description = "Cameras that orbit, follow and frame"

[[version]]
version = "1.2.0"
git = "https://github.com/someone/orbit-tools"
tag = "v1.2.0"
commit = "4f0c1a7e9b2d5c8a1e3f6b9d2c5a8e1f4b7d0c3a"
engine = ">=0.1"
```

The commit is what projects fetch, so a release stays the same even if its tag is moved later.

To add a release, clone the registry. Then, from the package's folder, run:

```
vox package publish --registry ../packages
```

This checks the following:

- the package has no changes that are not committed
- the commit checked out has a tag
- the repository has an `origin` that others can fetch from

Then it adds the release to the package's file in the registry's clone. Commit that file and push it, or send it to the registry as a pull request.

## Taking a release back

To stop new projects picking a broken release, add `yanked = true` to its entry in the registry. Projects already locked to it keep it until they update.

## Your own registry

A studio can keep its own registry in a private repository. Projects name it under `[registries]` in `packages.toml`, or replace the default registry with a `default` entry. Set `VOXEAR_REGISTRY` to make it the default for a whole machine.
