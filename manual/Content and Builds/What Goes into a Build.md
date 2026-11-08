---
order: 1
---

When you make a build, Voxear packs your levels, scenes, prefabs, models, pictures, materials, UI and sounds into **bundles**: a few large files that load quickly. This is the **content build**, the middle stage of every build. While you work in the editor nothing is packed: the game reads your project's files as they are.

## What a build includes

Voxear starts from every level, every prefab and every UI document, and follows everything each one uses: the scenes a level lists, the models and materials a scene names, the prefabs inside a prefab, and so on. Whatever is reached ships. Whatever nothing reaches is left out, so a model you tried and stopped using does not make the game bigger.

A build profile can leave levels out: only the levels it lists, and its start level, become starting points. See [[Making a Build]].

Models are **cooked** on the way: their pictures are compressed in the form the graphics card reads directly, and their shapes are packed to load quickly. Pictures used on their own, by a material, a light or a shader, are cooked the same way, as their import settings say (see [[Models]]). Cooked models and pictures are kept in `.voxear/ContentCache`, so the next build only cooks what changed.

## No file ships as it was made

A build holds no file in the form it was made in: no `.fbx`, `.png` or `.wav` that a player could copy out and open in another program. Models and pictures are cooked into Voxear's own format, and sounds are packed into it. So is the catalog that says what each bundle holds, so a build carries no list of your files that can be read. Many asset licences ask for this before their assets ship in a game.

This stops files being lifted out as they are. It is not encryption: no game's format stops someone set on taking a build apart.

## Paths the build cannot see

The build follows names written in your files. It cannot see a path your code puts together while the game runs:

```csharp
Level.LoadAsync($"Levels/World {next}.level");
```

Levels and prefabs ship anyway, unless a build profile or an `exclude` pattern leaves them out. If one might be left out, tell the build your code needs it with `[ContentRoot]`, on the class or method that loads it:

```csharp
[ContentRoot("Levels/World *.level")]
public sealed class WorldMap : Behaviour
{
}
```

A `[ContentRoot]` wins over a profile or an `exclude` pattern. The build warns about one that matches nothing.

Patterns use `*` for any name within a folder, `**` for any number of folders, and `?` for one letter.

## How assets are grouped

Assets that are always needed together go in the same bundle. A model that only the first level uses goes in with that level; a model every level uses goes in a bundle of its own, so it is not shipped twice. Tiny bundles are merged into others, and huge ones are split, so a game ends up with a sensible number of files.

## Content settings in voxear.toml

These go in your project's `voxear.toml`, and are all optional:

```toml
[content]
include = ["Textures/Splash/**"]
exclude = ["Levels/Test/**"]
merge_below_kb = 16
split_above_mb = 64
compression = "lz4"
```

| Setting | What it does |
| --- | --- |
| `include` | Ships these assets even if nothing uses them |
| `exclude` | Stops these being starting points. They still ship if something that ships uses them |
| `merge_below_kb` | Merges bundles smaller than this, in kilobytes. 0 never merges |
| `split_above_mb` | Splits bundles bigger than this, in megabytes. 0 never splits |
| `compression` | `"lz4"` loads fastest; `"zstd"` makes a smaller game but takes longer to build; `"none"` stores files uncompressed, for development builds only: any other build packs with `"lz4"` instead |

A build profile can set its own `compression`.

## Playing from bundles without a full build

To play your game from bundles, as players will, without making a whole build:

```bash
vox run MyGame --bundles
```

This packs the content into `.voxear/Content` and runs from there. `vox content build MyGame` does just the packing, and lists every bundle with its size and assets, the models and pictures it cooked, and the assets it left out.

> [!NOTE]
> Only the kinds of file Voxear knows ship: models, pictures, sounds, materials, scenes, levels, prefabs and UI, with the style sheets UI uses. Other files, such as your own data files, cannot be shipped yet.
