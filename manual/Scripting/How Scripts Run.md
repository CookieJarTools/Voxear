---
order: 2
---

# How Scripts Run

A behaviour has methods the engine calls at set times: when it first appears, every frame, and when it goes away. Write the ones you need, as `protected override`, and leave the rest out.

## When each method runs

| Method | Runs |
| --- | --- |
| `Awake` | Once, first, as soon as the behaviour exists and its fields are set |
| `OnEnable` | After Awake, and again each time the behaviour is turned back on |
| `Start` | Once, just before its first Update |
| `FixedUpdate` | At a steady 50 times a second, before Update. A frame can have none, or several |
| `Update` | Once every frame |
| `LateUpdate` | Every frame, after every behaviour's Update |
| `OnDisable` | When the behaviour is turned off, and before OnDestroy |
| `OnDestroy` | Once, last, at the end of the frame it is destroyed in |

Use `Awake` to set the behaviour itself up, and `Start` for anything that needs other behaviours to be ready. Use `FixedUpdate` for movement that must not depend on how fast the computer is, and `LateUpdate` for things that follow others, such as a camera following the player.

## The order is guaranteed

When a level loads, or a prefab is spawned, all its behaviours arrive together:

1. Every one of them gets `Awake`.
2. Then every one gets `OnEnable`.
3. Then every one gets `Start`, before the first `Update`.

So in `Start`, every other behaviour that arrived with yours has already had its `Awake` and `OnEnable`.

Within each step, behaviours run in the order they are in the scene, top to bottom. To make one type always run earlier or later, give it `[ExecutionOrder]`. Lower numbers run first:

```csharp
[ExecutionOrder(-100)]
public sealed class GameManager : Behaviour
{
}
```

## Time

| | |
| --- | --- |
| `Time.Delta` | Seconds since the last frame. Inside FixedUpdate, the length of a fixed step |
| `Time.UnscaledDelta` | The same, ignoring the time scale |
| `Time.FixedDelta` | The length of a fixed step: a fiftieth of a second |
| `Time.Elapsed` | Seconds since the game started |
| `Time.Frame` | How many frames the game has run |

## Turning behaviours on and off

Set `Enabled` to false to turn a behaviour off: it stops getting Update and the other frame methods, and gets OnDisable. Set it back to true to turn it on again. `Destroy()` removes the behaviour from its Obj for good.

## Waiting

Methods can be `async`, and carry on after an `await` on the next frame. `Task` needs `using System.Threading.Tasks;` at the top of the file:

```csharp
protected override async void Start()
{
	Log.Info("Starting");
	await Task.Delay(2000);
	Log.Info("Two seconds later");
}
```

Code after an `await` always runs on the main thread, so it can change Objs as any other code can.

## For Unity users

- Lifecycle methods are `protected override`. A plain `void Update()` never runs.
- `Obj.Position` and `Obj.Rotation` are local, as Unity's `localPosition` and `localRotation`. `Obj.WorldPosition` is where the Obj is in the world.
- `Obj.Transform` is a copy. Change `Obj.Position` or `Obj.Rotation` directly, or set the whole Transform at once: `Obj.Transform = Obj.Transform with { Scale = new Vector3(2f) };`
- Logging is `Log.Info`, `Log.Warning` and `Log.Error`.
- Only fields show in the Inspector, not properties.
- Static fields keep their values from one Play to the next in the editor. They start afresh when your scripts change.

Next: [[Working with Objs]].
