---
order: 4
---

A **Vehicle** drives its Obj's Rigidbody on wheels, with an engine, a gearbox and differentials: a car, a truck, a tank on its tracks, or a motorbike. Voxear builds it on Jolt's vehicle physics, so the suspension, the grip of the tyres and the engine's torque all play their part.

## The quickest way

Choose **Car** from the Hierarchy's **+** button, or **New Car** from the command palette. You get a car ready to drive: a body with a Collider, a Rigidbody and a Vehicle, and four wheels under it, the front two steering and the back two driven, each with a tyre you can swap for your own model.

Drive it from a script, with Voxear's own **Driving** actions:

```csharp
namespace MyGame;

public sealed class Driver : Behaviour
{
	protected override void Start()
	{
		// Driving turns walking off while it is on, so the two share keys without conflicting.
		Input.EnableMap("Driving");
	}

	protected override void Update()
	{
		Obj.Vehicle.DriveWithInput();
	}
}
```

`DriveWithInput` reads the Driving map's actions once a frame. Turn the Player map back on with `Input.EnableMap("Player")` as the player gets out.

| Action | Keyboard | Gamepad |
| --- | --- | --- |
| Throttle | W, Up | Right trigger |
| Brake (reverses once stopped) | S, Down | Left trigger |
| Steer | A and D, Left and Right | Left stick |
| Handbrake | Space | South (A, Cross) |
| Look (read as `Driving/Look`) | the mouse | Right stick |
| Exit | E | North (Y, Triangle) |

The Driving map is in Voxear's own actions, which a project without an `.inputactions` file of its own uses. To change the keys, copy the actions into your project with **Assets › Create › Input Actions** and edit them in the Input Actions window.

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
| Torque by RPM | How much of Max torque the engine gives as it revs: a curve from 0 to Max RPM across. It starts as an ordinary engine's, most at two thirds of its revs. A diesel's peaks early, a racing engine's late |
| Debug view | While the game plays, draws how its wheels touch, slip and ride (see below) |

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
| Grip by slip | How hard its tyre holds forwards, against how much it slips: 0 across is rolling with the ground, 1 is locked or spinning in place. Grip scales it |
| Side grip by angle | How hard it holds sideways, against how far it slides: 0 across is rolling straight, 1 is sliding sideways at 90 degrees. Side grip scales it |

## Tyre and engine curves

Each curve is edited on its small graph in the Inspector: drag a key, click where there is none to add one, and drag one off the field to take it out. The curves start as an ordinary tyre's and engine's, so a car you never touch drives as it always has.

An ordinary tyre holds hardest at a little slip, 0.06 forwards and 3 degrees sideways, and a little less once it slides. To make a car drift, lower **Side grip by angle** past its peak, so that once the back steps out it keeps sliding; to make it forgiving, keep the curve high all the way across.

From a script:

```csharp
// A truck's engine pulls hardest low down.
truck.TorqueCurve = new Curve((0f, 0.9f), (0.3f, 1f), (1f, 0.6f));
// Tyres that hold on, then let go all at once.
wheel.SideFriction = new Curve((0f, 0f), (0.03f, 1.4f), (0.1f, 0.5f));
```

`Vehicle.DefaultTorqueCurve`, `Wheel.DefaultFriction` and `Wheel.DefaultSideFriction` are the curves they start with. A tank's tracks have no tyre curves; its engine's curve still works.

## The debug view

Switch on **Debug view** on a Vehicle, or tick **Vehicle Debug** in the Scene view's **Gizmos** menu for every vehicle, and play. Each wheel shows:

- its suspension, a line from where it is fixed to the wheel's middle, green while it hangs and red as it presses against its stop;
- the point its tyre touches the ground, and an arrow out along the ground's normal;
- its slip forwards as a yellow arrow and its slide sideways as a magenta one, a metre long for a slip of 0.5 or 45 degrees;
- the numbers beside it: its slip, its slide in degrees and how far its suspension is pressed in.

Scripts switch it on with `vehicle.ShowDebug = true`, or for every vehicle with `Physics.DebugVehicles = true`. A build for players leaves it out. Each Wheel also tells scripts what it touches after each physics step, for skid marks, dust and tyre sounds:

| Member | What it says |
| --- | --- |
| `IsGrounded` | Whether its tyre touches the ground |
| `ContactPoint`, `ContactNormal` | Where it touches, in the world, and which way the ground faces there |
| `Compression` | How far its suspension is pressed in, from 0 hanging to 1 against its stop |
| `Slip`, `SlipAngle` | How much it slips forwards, and how far it slides sideways in degrees |
| `Spin` | How fast it spins, in radians a second |

```csharp
foreach (var child in car.Obj.Children)
{
	var wheel = child.Wheel;
	if (wheel.IsGrounded && wheel.SlipAngle > 15f)
	{
		skids.Mark(wheel.ContactPoint, wheel.ContactNormal);
	}
}
```

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
