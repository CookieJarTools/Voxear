---
order: 2
---

A **storm** is [[Weather]] with a place. It lies over a round part of the level, drifts with the wind, grows and dies. You see one coming as a dark mass of cloud with rain hanging under it, it reaches you, and you can walk out the other side. The level's own weather, the kind [[Lighting Settings]] and `Weather.Set` give, is still everywhere: a storm adds to it where the storm is.

## What a storm is where you stand

A storm left to itself is whatever the temperature makes it at each place (see [[Temperature]]). One storm rains in a valley and snows on the peak over it.

| Where it blows | What falls |
| --- | --- |
| Above 2 °C | Rain. Thunder and lightning once the storm blows hard, more of it where the air is hot and wet |
| 0 to 2 °C | Sleet: rain and snow together |
| Below 0 °C | Snow. A storm that blows hard is a blizzard: a gale, the air white, no lightning |
| 28 °C or more and dry | Blown sand in a gale under a dun sky, and no rain |

How hard a storm blows at a place is its **strength**, from 0 to 1, full over the inner half of its reach and fading from there to its edge. Rain and snow fall as hard as the storm blows; the sky fills in and darkens before the first drops.

A **fog bank** is a storm of another kind: thick fog, no wind and no rain. Natural ones form where the climate is cool and wet, and drift slowly.

## Storms that come by themselves

Add a **Storms** component to any Obj of the level, such as the one that holds its [[Temperature#The Climate component|Climate]], and storms come and go by themselves.

| Setting | What it does |
| --- | --- |
| Natural Storms | Whether storms come by themselves. Off, the level has only the storms scripts start |
| Frequency | How often, from 0 for never to 1. At the usual 0.35 there is a storm within three kilometres about a third of the time. Dry country gets fewer |
| Largest | How far the largest reaches, in metres. The smallest reach 250 m |
| Standing Storms | Great storms that never leave their place, each a wall of weather round a calm eye. They stand at seeded places, never within 2.5 km of the level's middle |
| Seed | Another seed gives the level other storms |
| Sea Look, Swell Height, Storm Whirlpools, Maelstroms, Night Glow | The sea under the weather: see [[Block Worlds#Rough seas]] |

A natural storm lives five to twelve minutes, drifts at 3 to 9 m/s, mostly the way the level's weather comes from, and comes on and dies away over the first and last third of its life.

Natural storms are worked out from the world's seed and the storms' clock alone. Nothing about them is kept in a save or sent over the network: two machines with the same seed and the same clock have the same storms. The clock turns while the game plays and stands still in the editor.

## Starting a storm from a script

`Storms.Start` puts a storm over a place. It comes on over a minute and blows until you stop it.

```csharp
namespace Game;

public sealed class Ambush : Behaviour
{
	private Storm storm;

	protected override void OnTriggerEnter(Collider other)
	{
		// A storm 400 m across over the pass, whatever the temperature makes it.
		storm = Storms.Start(Transform.Position, 400f);
	}

	protected override void OnTriggerExit(Collider other)
	{
		// Gone over half a minute.
		storm.Stop();
	}
}
```

`Storms.At` tells the weather at any point, and two events tell a storm reaching and leaving the viewer:

```csharp
namespace Game;

public sealed class Traveller : Behaviour
{
	private float speed = 5f;

	protected override void OnEnable()
	{
		Storms.Arrived += Warn;
	}

	protected override void OnDisable()
	{
		Storms.Arrived -= Warn;
	}

	protected override void Update()
	{
		// Out in a blizzard, the going is slow.
		var here = Storms.At(Transform.Position);
		speed = here.Kind == StormWeather.Blizzard ? 2f : 5f;
	}

	private void Warn(Storm storm)
	{
		Log.Info($"A storm {storm.Radius:0} m across is on us");
	}
}
```

| Member | What it is |
| --- | --- |
| `Storms.Start(position, radius, strength, kind, comesOver)` | Starts a storm and gives it back. 400 m, full strength, by temperature and a minute to come on, unless you say |
| `Storms.At(position)` | The weather at a point, as a `WeatherAt`: `Kind` (a `StormWeather`: `None`, `Rain`, `Thunderstorm`, `Sleet`, `Snow`, `Blizzard`, `Sandstorm` or `Fog`), `Strength`, `Storm`, `InStorm`, `Amounts`, `Celsius` and `Humidity` |
| `Storms.All` | Every storm near the viewer, natural and made, the nearest first, then every other made one |
| `Storms.Arrived`, `Storms.Left` | Events, raised with the storm as it reaches the viewer and as it leaves or dies |
| `Storms.Natural`, `Frequency`, `Largest`, `Standing` | The Storms component's settings. With no Storms component in the level they are kept for the level all the same |
| `Storms.Clock` | The storms' clock, in seconds |
| `Storms.CaptureState()`, `Storms.ApplyState(bytes)` | The clock and the made storms as bytes, and back (see below) |

## Changing a storm

Everything about a storm you started can be changed while it blows. A natural storm can be read, not changed.

```csharp
// A blizzard that crosses the valley at 6 m/s and dies at the pass ten minutes on.
var storm = Storms.Start(valley.Position, 500f, 1f, StormKind.Blizzard);
storm.MoveAlong(6f, ford.Position, pass.Position);
storm.Lifetime = 600f;
```

| Member | What it is |
| --- | --- |
| `Position` | Where its middle is. Its height is not used |
| `Radius` | How far it reaches at its widest, in metres. `Reach` is how far it reaches this moment: a storm spreads as it comes on |
| `Strength` | How hard it blows at its height, from 0 to 1. `StrengthNow` is how hard this moment, and `StrengthAt(position)` how hard at a point |
| `Kind` | A `StormKind`: `ByTemperature`, `Rain`, `Thunderstorm`, `Blizzard`, `Sandstorm` or `FogBank`. Any but the first is that wherever it blows |
| `Velocity` | How fast it drifts, in metres a second |
| `MoveAlong(speed, places)` | Sends it to each place in turn. It stands at the last |
| `Eye` | How much of its reach its calm eye takes, from 0 for none to 0.8: clear sky and a flat sea inside, the storm a wall all round |
| `Lifetime` | Seconds left, dying away at the end. Infinite for one that blows until stopped |
| `Stop(seconds)` | Has it die away, over 30 seconds unless you say |
| `Exists`, `IsNatural` | Whether it is still there, and whether the world made it |

## Storms in saves and over the network

A save of the scene keeps the storms' clock and the storms and whirlpools scripts made, and loading puts them back (see [[Saving the Game]]). It is sixteen bytes for the clock and fifty-six for each made storm.

In a networked game the host sends the same bytes to a player who joins, and again whenever it starts, changes or stops a storm:

```csharp
// On the host, in an RPC to the player who joined.
var state = Storms.CaptureState();

// On the player who joined.
Storms.ApplyState(state);
```

With the same clock every player has the same natural storms too, and the same swell on the sea.

> [!NOTE]
> Wet ground and settled snow follow the viewer: they are one amount for the whole level, so ground a storm has not reached looks as wet as the ground under it. Rain and snow still fall through roofs. Lightning lights the sky without a bolt, and the engine ships no thunder: play your own from `Weather.Struck`. The engine does not send storms between machines by itself: send `CaptureState` in an RPC of your own.
