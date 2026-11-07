---
order: 18
---

A **spring** is a value that follows a goal as a weight on a spring would: it lags, overshoots and settles. Procedural motion looks alive with them, where moving straight to a goal looks mechanical.

```csharp
private Spring3 sway = new Spring3(Vector3.Zero, new SpringFeel(2.5f, 0.4f));

protected override void Update()
{
	sway.Kick(new Vector3(-Mouse.Delta.X, Mouse.Delta.Y, 0f) * 0.002f);
	gun.Position = rest + sway.Update(Vector3.Zero, Time.DeltaTime);
}
```

`Spring` follows a number, `Spring3` a point or direction, and `RotationSpring` a turn. `Update` moves it toward its goal and gives back where it is; `Kick` adds to its velocity at once, as a blow does.

## How a spring feels

A **SpringFeel** is three numbers:

- **Frequency**, in hertz: how fast it moves.
- **Damping**: below 1 it overshoots and rings, 1 settles as fast as it can without overshooting, and above 1 creeps in.
- **Response**: how it answers its goal jumping. 0 starts from rest, 1 heads off at once, and below 0 starts the wrong way first, as anticipation does.

`SpringFeel.HalfLife(0.1f)` settles without overshooting, halfway to its goal in a tenth of a second.

Springs move the same at any frame rate, and don't fly apart on a long frame.
