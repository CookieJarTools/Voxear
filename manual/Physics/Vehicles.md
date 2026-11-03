---
order: 4
---

A **Vehicle** drives its Obj's Rigidbody on wheels, with an engine, a gearbox and differentials: a car, a truck, a tank on its tracks, or a motorbike. Voxear builds it on Jolt's vehicle physics, so the suspension, the grip of the tyres and the engine's torque all play their part.

## The quickest way

Choose **Car** from the Hierarchy's **+** button, or **New Car** from the command palette. You get a car ready to drive: a body with a Collider, a Rigidbody and a Vehicle, and four wheels under it, the front two steering and the back two driven, each with a tyre you can swap for your own model.

Drive it from a script:

```csharp
namespace MyGame;

public sealed class Driver : Behaviour
{
	protected override void Update()
	{
		var car = Obj.Vehicle;
		car.Throttle = Input.Vector2("Move").Y;
		car.Steering = Input.Vector2("Move").X;
		car.Handbrake = Input.Held("Jump") ? 1f : 0f;
	}
}
```

## A vehicle of your own

1. Give an Obj a **Collider**, a **Rigidbody** and a **Vehicle**, from **Add Component**'s Physics folder. The Obj faces its +Z.
2. Give each wheel a child Obj of its own, placed where the wheel's middle is when the vehicle is at rest, and give it a **Wheel**.
3. Put your wheel model on the wheel's Obj, or inside it. While the game plays, physics turns and drops the wheel's Obj as the wheel rolls, steers and rides its suspension, so the model goes with it.

Select a Vehicle or a Wheel to see each wheel in the Scene view: a ring round its axle and a line up and down its suspension's travel.

## A Vehicle's settings

| Setting | What it does |
| --- | --- |
| Kind | **Car** for wheels, **Tank** for tracks, or **Motorbike** for two wheels that lean |
| Max torque | The most torque its engine gives, in newton-metres. 500 suits a family car, and a tank wants several thousand |
| Min RPM, Max RPM | The range its engine revs through, in revolutions a minute |
| Automatic | Its gears change by themselves |
| Gears | How many forward gears its gearbox has |
| First gear, Top gear | First gear's ratio and top gear's; the gears between are spread evenly. A higher first gear pulls away harder; a lower top gear reaches a higher top speed |
| Anti-roll | How stiff a bar across each pair of wheels side by side is, in newtons a metre, to keep it from leaning out of turns. 0 for none, about 1000 for a car |

A tank turns by running its tracks at different speeds, and on the spot by running them opposite ways. It turns best when its tracks are about as far apart as it is long.

## A Wheel's settings

| Setting | What it does |
| --- | --- |
| Radius, Width | The wheel's size, in metres |
| Suspension | How far the wheel travels up and down, in metres, half each way from where it rests |
| Spring | How stiff the spring is, as how many times a second it bounces: 1 soft, 3 firm |
| Damping | How quickly a bounce dies away, from 0 to 1 |
| Steer angle | How far it steers either way, in degrees. 0 for a wheel that doesn't |
| Powered | The engine drives it. Two powered wheels side by side share a differential |
| Brake | The most torque its brake holds it with, in newton-metres |
| Handbrake | The most torque the hand brake holds it with. Usually only the back wheels have one |
| Grip, Side grip | How hard its tyre holds, forwards and sideways, as a share of an ordinary tyre: 2 for racing tyres, 0.3 for ice |

An automatic gearbox only changes up while no driven wheel is spinning. A car with a strong engine and every wheel driven can spin its tyres in first gear and stay there: give its tyres more Grip, drive only two wheels, or lower its Max torque.

## Driving from scripts

| Member | What it does |
| --- | --- |
| `Throttle` | From -1 for full reverse to 1 for full ahead |
| `Steering` | From -1 for full left to 1 for full right |
| `Brake`, `Handbrake` | From 0 to 1 |
| `Speed` | How fast it goes forward, in metres a second. Times 3.6 gives km/h |
| `Rpm`, `Gear` | How fast its engine turns, and its gear: -1 reverse, 0 neutral, 1 first |

`Obj.AddVehicle(VehicleKind.Tank)` and `Obj.AddWheel()` make one from code, and every setting in the Inspector can be set from a script too.
