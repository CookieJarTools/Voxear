---
order: 6
---

**Lighting settings** keep a level's whole lighting in an asset of its own: its sky, how its sun looks, and its post-processing. A scene only names the lighting settings it is lit by, so you can make a set once, such as a dusty evening, a grey morning or a cave, light any scene with it, and swap one set for another in the editor or as the game runs, as for a storm rolling in. They are Voxear's version of Unity's Lighting Settings asset.

## The Lighting page

Click the sun on the left-hand rail to open the **Lighting** page. At its top, **Lighting** picks the settings the open scene is lit by: None, one of the built-in sets, or any lighting settings in the project. The scene names them from then on, once you save it.

Rest the pointer on a setting's name, or on a tab, to read what it does. Below, a tab for each part of the settings shows them all: **Sky**, **Clouds**, **Sun**, **Exposure**, **Colour**, **Bloom**, **Fog**, **Shading** and **Lens**. Each shows the value in effect, and a change is written into the lighting settings' file at once, so the scene, and every other scene lit by the same settings, changes as you type. Empty a number's field to go back to the look's value.

Built-in lighting cannot be changed: click **Make a Copy** for a copy in the project's Lighting folder that the scene is then lit by. A scene lit by none has **New Lighting Settings** instead, which carries the scene's own sky and post-processing over into the new settings.

## A scene's lighting in the Inspector

Select a scene in the Project panel and the Inspector shows its **Lighting** as the Lighting page does: the settings it is lit by, picked in the same way, and their tabs below. This works for any scene, open or not. A scene that is open keeps the change once you save it, as with the Lighting page; one that is not open has it written into its file at once.

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
| `look` | A built-in look to start from, such as `golden_hour`, `night_raid` or `candlelit_castle`: every one is listed in [[Post-Processing]]. Leave it out to start from the plain defaults |
| `[sky]` | The sky, as a scene's `[sky]` writes it: its mode, colours, haze, sun size, stars and, under `[sky.clouds]`, its clouds (see [[The Sky]]). Only what you write changes |
| `[sun]` | How the level's sun looks, whatever light it is: `temperature` in kelvin and `tint` colour it, `strength` scales its intensity, `softness` widens it where the light keeps its default, `shadow_tint` colours its shadows where the light has no tint of its own, and `shadow_distance` and `shadow_split` set how far its shadows reach and how sharp they are near and far (see [[Lights]]) |
| `[post]` | Post-processing, as a scene's `[post]` writes it: exposure, bloom, grading, fog and the rest (see [[Post-Processing]]). Only what you write changes |

Change them on the Lighting page, or select the file in the Project panel and change them in the Inspector, which shows the same settings in the same tabs; the open scene lit by them changes at once. A setting that does nothing as the others stand is hidden until it would: the fog's settings while there is no fog, light shafts' while there are none, the gradient sky's colours under the atmosphere, auto exposure's limits while exposure is fixed, and the tonemapper's contrast and saturation unless it is AgX. You can also change the file in your text editor: an open scene lit by it changes as soon as you save.

## Lighting a scene

A scene names its lighting settings at the top of its file:

```toml
format = 1
lighting = "Lighting/Dusk.lighting"
```

The scene is lit by them as it loads, by them alone. A scene can also write its own `[sky]` and `[post]`, as older scenes do; those light it only while it names no lighting settings.

Each look is built in as lighting settings too, as `builtin:` and its name: `lighting = "builtin:golden_hour"`, `"builtin:dark_forest"` and so on. The Lighting page's menu lists them all.

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

Applied settings replace the scenes' own lighting, sky and post-processing and all, from the next frame until the level changes. The built-in ones are there by name, such as `LightingSettings.GoldenHour`, `LightingSettings.NightRaid` and `LightingSettings.CandlelitCastle`. Settings that cannot be read are reported in the Console, and the lighting stays as it was.
