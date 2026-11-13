---
order: 2
---

**Weather** is cloud, gloom, fog, wind, rain, snow, lightning and blown sand laid over a level's lighting. The lighting stays as you made it: clear weather is the lighting as it is, and every other kind adds to it. So one set of lighting settings serves a level in sunshine and in a storm, and a script can turn the weather as the game runs.

This page is the weather of the whole level. Weather that lies over a part of it and drifts, raining in one valley while the next is dry, is a storm: see [[Storms]].

## The kinds of weather

| Kind | What it adds |
| --- | --- |
| **Clear** | Nothing: the level's lighting as it is |
| **Cloudy** | Cloud over half of whatever sky was open |
| **Overcast** | A grey sky from edge to edge and a dull day under it |
| **Rain** | Steady rain from a dark sky, with some wind and mist |
| **Storm** | Heavy rain, a gale, lightning and a sky near black |
| **Snow** | Snow falling from a pale, close sky |
| **Fog** | Thick fog, the sky lost in it |
| **Blizzard** | Driving snow in a gale, the air white with it |
| **Sandstorm** | A gale full of blown sand under a dun sky |
| **Custom** | Amounts of your own, below |

## What weather is made of

Each kind is eight amounts, each from 0 for none to 1:

| Amount | What it does |
| --- | --- |
| **Cloud** | Fills the sky's clouds in towards overcast (see [[The Sky]]). At 1 the sky is covered whatever its own Cover |
| **Darkness** | Makes the clouds heavier and their shadows darker, and takes the sun's light: under the atmosphere the sky dims with it |
| **Fog** | Thickens the level's fog, turning it on where it was off, and the air's haze (see [[Post-Processing]]) |
| **Wind** | Makes the level's Wind Zone blow harder, in harder gusts: plants, the sea, the clouds and the rain all feel it (see [[Wind]]). In a level without a Wind Zone the weather's own wind blows, along the level's X as its clouds drift, on plants, rain, smoke and the clouds; the sea takes its wind from a Wind Zone alone |
| **Rain** | Rain falling round the camera |
| **Snow** | Snow falling round the camera |
| **Lightning** | How often lightning strikes: at 1, every eight seconds or so. A strike lights the sky and the level under it for a moment, in two or three flickers |
| **Dust** | Blown sand in the air: fog of the sand's own colour, a third thicker than fog, and a dun sky |

Rain and snow are the starter Rain and Snow effects (see [[Particle Systems]]), the snow's flakes made bigger, falling from a box 24 metres wide that stays over the camera. They land on what the camera can see, and the wind blows them. In the editor they fall round the Scene view's camera.

## What the weather leaves behind

Rain and snow stay a while after they fall:

- **Rain soaks the level.** Every surface darkens and shines, what faces up most. Dry ground is soaked in about half a minute of rain, and dries over three minutes once the rain stops.
- **Snow settles** on whatever faces up: in patches at first, then over everything, in a minute and a half of steady snow. It melts over four minutes once the snow stops, and in well under one if it rains.

A level that starts in rain or snow, from its lighting settings, starts already wet or snowed over, as if the weather had been there a while. Weather a script turns to starts on dry ground.

## Setting the weather

On the **Lighting** page, the **Weather** tab picks the weather the level is in when lit by those settings: a kind, or Custom and its eight amounts. In the file it is `[weather]`:

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
| `Weather.Set(amounts, seconds)` | Turns it to amounts of your own, a `WeatherAmounts` with `Cloud`, `Darkness`, `Fog`, `Wind`, `Rain`, `Snow`, `Lightning` and `Dust` |
| `Weather.Kind` | The kind it is, or is turning to. Setting it turns the weather at once |
| `Weather.Amounts` | What it adds this moment where the viewer is: part way between two kinds while it turns, with whatever a storm overhead adds |
| `Weather.IsTurning` | Whether it is still turning |
| `Weather.Wetness` | How wet the rain has left the level, from 0 to 1 |
| `Weather.SnowCover` | How much snow has settled, from 0 to 1 |
| `Weather.Flash` | How bright lightning is this moment, 0 between strikes |
| `Weather.Changed` | An event, raised with the new kind when the weather is set to another, by a script or by lighting settings |
| `Weather.Struck` | An event, raised as lightning strikes |

Amounts of your own make weather no kind has, such as a light drizzle under a bright sky:

```csharp
Weather.Set(new WeatherAmounts { Cloud = 0.4f, Rain = 0.25f }, 20f);
```

The engine ships no sounds for weather. Play your own: thunder from `Weather.Struck`, and rain as loud as `Weather.Amounts.Rain`:

```csharp
public sealed class Thunder : Behaviour
{
	protected override void OnEnable()
	{
		Weather.Struck += Rumble;
	}

	protected override void OnDisable()
	{
		Weather.Struck -= Rumble;
	}

	private void Rumble()
	{
		// The Obj's Audio Source holds the thunder's sound.
		Obj.AudioSource.Play();
	}
}
```

> [!NOTE]
> Nothing knows what is under a roof: rain and snow fall through roofs the camera cannot see the top of, and floors indoors get wet and snowed on with the rest. Lightning lights the whole sky, without a bolt. In a networked game each machine turns its own weather: set it on every machine at the same time. [[Storms]] are the same on every machine by themselves.
