---
order: 6
---

**Lighting settings** keep a level's whole lighting in one file of its own: its sky, how its sun looks, and its post-processing. Make a set once, such as a dusty evening, a grey morning or a cave, and light any scene with it, or change between them as the game runs, as for a storm rolling in. They are Voxear's version of Unity's Lighting Settings asset.

## Making lighting settings

In the Project panel, right-click a folder and pick **New Lighting Settings**. The new file starts from the Golden Hour look, with a note of what each part holds:

```toml
format = 1
look = "golden_hour"

[sky]
haze = 2.5

[sun]
temperature = 4500
strength = 1.2

[post.bloom]
intensity = 0.2
```

| Part | What it holds |
| --- | --- |
| `look` | A built-in look to start from: `golden_hour`, `storybook` or `soft_overcast` (see [[Post-Processing]]). Leave it out to start from the plain defaults |
| `[sky]` | The sky, as a scene's `[sky]` writes it: its mode, colours, haze, sun size and stars (see [[The Sky]]). Only what you write changes |
| `[sun]` | How the level's sun looks, whatever light it is: `temperature` in kelvin and `tint` colour it, `strength` scales its intensity, `softness` widens it where the light keeps its default, and `shadow_tint` colours its shadows where the light has no tint of its own |
| `[post]` | Post-processing, as a scene's `[post]` writes it: exposure, bloom, grading, fog and the rest (see [[Post-Processing]]). Only what you write changes |

Select the file to read it in the Inspector. Edit it in your text editor: an open scene lit by it changes as soon as you save.

## Lighting a scene

A scene names its lighting settings at the top of its file:

```toml
format = 1
lighting = "Lighting/Dusk.lighting"
```

The scene is lit by them as it loads. What the scene writes in its own `[sky]` and `[post]` still wins, so a scene can share a set and change one thing, such as a stronger bloom.

Each look is built in as lighting settings too: `lighting = "builtin:golden_hour"`, `"builtin:storybook"` or `"builtin:soft_overcast"`.

## Changing the lighting from a script

Give a script a `LightingSettings` field, pick the settings in the Inspector, and apply them with `Lighting.Apply`:

```csharp
public sealed class StormTrigger : Behaviour
{
	public LightingSettings Storm;

	protected override void OnTriggerEnter(Collider other)
	{
		Lighting.Apply(Storm);
	}
}
```

Applied settings replace the scenes' own lighting, sky and post-processing and all, from the next frame until the level changes. The built-in ones are there as `LightingSettings.GoldenHour`, `LightingSettings.Storybook` and `LightingSettings.SoftOvercast`. Settings that cannot be read are reported in the Console, and the lighting stays as it was.
