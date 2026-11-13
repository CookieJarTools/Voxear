---
order: 1
---

A place's temperature, in °C, is made of six things, added together:

| Part | What it gives |
| --- | --- |
| **Climate** | How warm the land is: from the world's generator, or from the level's [[#The Climate component|Climate]] where there is no generator. Higher ground is colder |
| **Day** | Warmer in the afternoon, warmest at three, and colder in the small hours, coldest at five. A dry place swings half as much again as the Climate's Day Swing, and a wet one half as much, so a desert is hot by day and cold by night |
| **Shelter** | Under blocks, the temperature is drawn towards the deep ground's own, which is mild: a cave is cool in a desert and above freezing in the snow. A single roof shelters a little; rock six metres thick shelters wholly. In water, it is the water's temperature |
| **Zones** | Heat Zones set or add their temperature inside them |
| **Sources** | Hot and cold things near warm or cool it: Heat Sources, hot and cold blocks, lava, and short-lived heat from scripts. The hottest and the coldest of them count, each faded by how far away it is |

Everything here is worked out from sums and products alone, so every machine in a networked game gets the same temperature.

## The Climate component

The level's first **Climate** keeps its clock, and its climate where no generator gives one. Add one to any Obj, such as the one holding the level's lighting.

| Setting | What it does |
| --- | --- |
| Time of Day | The hour, from 0 to 24 |
| Day Length | How many minutes of play a whole day takes. 0 stands the clock still |
| Turns Sun | Whether the clock turns the level's first Directional Light: up in the east at six, highest at noon, down in the west at six |
| Temperature | How warm it is at nothing high, where no generator says |
| Day Swing | How much warmer the afternoon is, and colder the small hours |
| Humidity | How wet it is, from 0 to 1, where no generator says |

The clock only turns while the game plays. Without a Climate in the level, it is always noon and the day adds nothing.

## Heat Sources

A **Heat Source** is something hot or cold, as a campfire or a block of ice. At its middle it is its **Temperature**, fading to the place's own by its **Radius** away.

## Heat Zones

A **Heat Zone** is an area hotter or colder than round it, as a forge's room or a cold store. Its **Shape** is a box or a sphere round its Obj, turned with it, **Size** metres across. In **Set** mode it is its **Temperature** inside; in **Add** mode its Temperature is added to what it would be. It comes on over its **Edge**, in metres inside its side.

## Hot and cold blocks

A block's **Heat**, in its Inspector, makes it hot or cold: magma at 1100 °C reaching four metres, or packed ice at -15 °C reaching one. Lava in [[Pools and Dry Areas|Pools]] and [[Streams]] is hot too, and warms what is over it.

## Temperature in scripts

```csharp
// Breath shows in the cold.
if (Heat.At(head.Position) < 2f)
{
    breath.Play();
}

// A fireball's blast warms what is round it, 600 °C, four metres wide, for three seconds.
Heat.AddSource(hit.Point, 600f, 4f, 3f);
```

`Heat.Sample` gives each part on its own, as a `HeatSample`: `Climate`, `Day`, `Shelter` (from 0 in the open to 1 deep underground), `Sheltered`, `Zones`, `Sources` and `Humidity`, with `Celsius` the sum.

The clock is `Clock`:

```csharp
// Sleep till morning.
Clock.TimeOfDay = 7f;
var light = Clock.IsDay;
```

| Member | What it gives |
| --- | --- |
| `Clock.TimeOfDay` | The hour, from 0 to 24. Setting it moves the clock |
| `Clock.DayLength` | Minutes a day takes |
| `Clock.TurnsSun` | Whether it turns the sun |
| `Clock.Exists` | Whether the level has a Climate |

## Seeing the temperature

The Scene view's **Heat** shading tints the ground round its camera by temperature: blue below freezing, green when mild, yellow and red as it gets hot, and white where it is hot enough to melt rock.
