---
order: 2
---

A **Heat Body** takes the temperature of where its Obj is, and changes when it has been hot or cold enough for long enough. Put one on food, on anything that melts or freezes, and on characters.

| Setting | What it does |
| --- | --- |
| Warms In | How many seconds it takes to come about two thirds of the way to the place's temperature. 0 takes it at once |
| Comfort | The temperatures a character is comfortable between |
| Cold Resistance | How many degrees colder than its Comfort it is still comfortable, as warm clothes give |
| Heat Resistance | How many degrees hotter, as a cooling drink gives |
| Changes | What it becomes, below |

While the game plays, the Inspector also shows its **Temperature** now, and how it **Feels**.

## Changes

Each change has a **Name**, **When** it counts (**Above** or **Below** a **Temperature**), how long **For**, in seconds, and the prefab it **Becomes**. Its time counts while the Heat Body is past that temperature, and holds still while it is not, so meat taken off the fire and put back keeps the time it had.

Raw meat that sears on a fire and freezes in the snow has two changes:

| Name | When | Temperature | For | Becomes |
| --- | --- | --- | --- | --- |
| Seared | Above | 80 | 20 | Seared Meat |
| Frozen | Below | -5 | 60 | Frozen Meat |

When a change happens, its Obj is replaced by the prefab, in the same place, under the same parent. If the prefab has a Heat Body, it starts as warm as the Obj it replaced, so seared meat left on the fire can burn. A change that becomes nothing happens once, and scripts hear of it.

Items that lie in the world are Objs made from the item's **Dropped** prefab, so a food item cooks where it was dropped when its Dropped prefab has a Heat Body:

```csharp
var meat = rawMeat.Drop(campfire.Transform.Position + Vector3.UnitY * 0.3f);
```

## How characters feel

A character's Heat Body **Feels** one of five ways:

| Feels | When |
| --- | --- |
| Freezing | Colder than its band by 15 °C or more |
| Cold | Colder than its band |
| Comfortable | Within its band: its Comfort, widened by its resistances |
| Hot | Hotter than its band |
| Scorching | Hotter than its band by 15 °C or more |

What the cold and the heat do is your game's. `Beyond` gives how many degrees past its band a character is, for hurting it, slowing it or blurring the screen:

```csharp
protected override void Update()
{
    var body = Obj.HeatBody;
    body.ColdResistance = wearingFur ? 20f : 0f;
    health -= MathF.Abs(body.Beyond) * 0.1f * Time.Delta;
}
```

## Heat Bodies in scripts

```csharp
protected override void Start()
{
    HeatBody.Changed += change => Log.Info($"{change.Obj} is {change.Name}, now {change.Became}");
    HeatBody.FeelsChanged += (body, feels) =>
    {
        if (body.Obj == Obj && feels == Feels.Freezing)
        {
            hint.Text = "Find a fire";
        }
    };
}
```

| Member | What it gives |
| --- | --- |
| `Obj.HeatBody` | The Obj's Heat Body; `Obj.AddHeatBody()` gives it one |
| `Temperature` | Its temperature now. Setting it starts it off hot or cold |
| `Feels`, `Beyond` | How it feels, and how far past its band it is |
| `ColdResistance`, `HeatResistance` | Its resistances, which clothing and potions change |
| `Progress(name)` | How far a change has counted: seconds held, seconds needed, how far along from 0 to 1, and whether it has happened. Draw a cooking bar from it |
| `HeatBody.Changed` | Raised after the frame a change happens, with its name and what it became |
| `HeatBody.FeelsChanged` | Raised when a character comes to feel otherwise |

Heat Bodies move on with the fixed steps, so a networked game's machines agree on when food is done.
