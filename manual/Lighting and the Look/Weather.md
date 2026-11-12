---
order: 2
---

**Weather** is cloud, gloom, fog, wind, rain and snow laid over a level's lighting. The lighting stays as you made it: clear weather is the lighting as it is, and every other kind adds to it. So one set of lighting settings serves a level in sunshine and in a storm, and a script can turn the weather as the game runs.

## The kinds of weather

| Kind | What it adds |
| --- | --- |
| **Clear** | Nothing: the level's lighting as it is |
| **Cloudy** | Cloud over half of whatever sky was open |
| **Overcast** | A grey sky from edge to edge and a dull day under it |
| **Rain** | Steady rain from a dark sky, with some wind and mist |
| **Storm** | Heavy rain, a gale and a sky near black |
| **Snow** | Snow falling from a pale, close sky |
| **Fog** | Thick fog, the sky lost in it |
| **Custom** | Amounts of your own, below |

## What weather is made of

Each kind is six amounts, each from 0 for none to 1:

| Amount | What it does |
| --- | --- |
| **Cloud** | Fills the sky's clouds in towards overcast (see [[The Sky]]). At 1 the sky is covered whatever its own Cover |
| **Darkness** | Makes the clouds heavier and their shadows darker, and takes the sun's light: under the atmosphere the sky dims with it |
| **Fog** | Thickens the level's fog, turning it on where it was off, and the air's haze (see [[Post-Processing]]) |
| **Wind** | Makes the level's Wind Zone blow harder, in harder gusts: plants, the sea, the clouds and the rain all feel it (see [[Wind]]). A level without a Wind Zone stays still |
| **Rain** | Rain falling round the camera |
| **Snow** | Snow falling round the camera |

Rain and snow are the starter Rain and Snow effects (see [[Particle Systems]]), the snow's flakes made bigger, falling from a box 24 metres wide that stays over the camera. They land on what the camera can see, and the wind blows them. In the editor they fall round the Scene view's camera.

## Setting the weather

On the **Lighting** page, the **Weather** tab picks the weather the level is in when lit by those settings: a kind, or Custom and its six amounts. In the file it is `[weather]`:

```toml
[weather]
kind = "rain"
```

```toml
[weather]
kind = "custom"
cloud = 0.3
fog = 0.6
snow = 0.2
```

Lighting settings are a level's lighting whole, so applying them sets the weather too: their own, or clear when they write none. See [[Lighting Settings]].

## Turning the weather from a script

`Weather.Set` turns the weather to a kind over a number of seconds, from whatever it is now. The sky fills, the light dims, the wind rises and the rain starts together:

```csharp
public sealed class StormFront : Behaviour
{
	protected override void OnTriggerEnter(Collider other)
	{
		// A storm rolls in over half a minute.
		Weather.Set(WeatherKind.Storm, 30f);
	}

	protected override void OnTriggerExit(Collider other)
	{
		Weather.Set(WeatherKind.Clear, 60f);
	}
}
```

| Member | What it is |
| --- | --- |
| `Weather.Set(kind, seconds)` | Turns the weather to a kind. Ten seconds unless you say; 0 for at once |
| `Weather.Set(amounts, seconds)` | Turns it to amounts of your own, a `WeatherAmounts` with `Cloud`, `Darkness`, `Fog`, `Wind`, `Rain` and `Snow` |
| `Weather.Kind` | The kind it is, or is turning to. Setting it turns the weather at once |
| `Weather.Amounts` | What it adds this moment: part way between two kinds while it turns |
| `Weather.IsTurning` | Whether it is still turning |

Amounts of your own make weather no kind has, such as a light drizzle under a bright sky:

```csharp
Weather.Set(new WeatherAmounts { Cloud = 0.4f, Rain = 0.25f }, 20f);
```

> [!NOTE]
> Rain and snow fall through roofs the camera cannot see the top of, the ground does not get wet or snowy from the weather yet, and there is no lightning or sound of rain. In a networked game each machine turns its own weather: set it on every machine at the same time.
