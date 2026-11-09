---
order: 2
---

A behaviour has methods the engine calls at set times: when it first appears, every frame, and when it goes away. Write the ones you need, as `protected override`, and leave the rest out.

## When each behaviour method runs

| Method | When it runs |
| --- | --- |
| `Awake` | Once, first, as soon as the behaviour exists and its fields are set |
| `OnEnable` | After Awake, and again each time the behaviour is turned back on |
| `Start` | Once, just before its first Update |
| `OnUiBuilt` | On an Obj with a UI Panel, once its document is built, after Start. See [[UI Panels in the World]] |
| `FixedUpdate` | At a steady 50 times a second, before Update. A frame can have none, or several |
| `Update` | Once every frame |
| `PoseUpdate` | Every frame, after animation has posed the bones and before Objs on bones follow them |
| `LateUpdate` | Every frame, after every behaviour's Update and PoseUpdate |
| `OnDisable` | When the behaviour is turned off, and before OnDestroy |
| `OnDestroy` | Once, last, at the end of the frame it is destroyed in |

Use `Awake` to set the behaviour itself up, and `Start` for anything that needs other behaviours to be ready. Use `FixedUpdate` for movement that must not depend on how fast the computer is, and `LateUpdate` for things that follow others, such as a camera following the player. Use `PoseUpdate` to change a character's pose, such as reaching a hand to a door handle or placing what it holds before its eyes: by then its clips have posed it and a First-Person View has put its camera in the head.

## Movement between fixed steps

On a display faster than the fixed steps, most frames have no FixedUpdate. So whatever FixedUpdate or physics moves is shown part-way between where the last two steps left it: it moves evenly every frame, a step behind. Update, LateUpdate and drawing see it there; FixedUpdate always sees where the steps left it.

An Obj placed in Update, as a teleport, is there at once, and the next step moves it on from there.

## The order behaviours start in

When a level loads, or a prefab is spawned, all its behaviours arrive together:

1. Every one of them gets `Awake`.
2. Then every one gets `OnEnable`.
3. Then every one gets `Start`, before the first `Update`.
4. Then those on an Obj with a UI Panel get `OnUiBuilt`.

So in `Start`, every other behaviour that arrived with yours has already had its `Awake` and `OnEnable`.

Within each step, behaviours run in the order they are in the scene, top to bottom. To make one type always run earlier or later, give it `[ExecutionOrder]`. Lower numbers run first:

```csharp
[ExecutionOrder(-100)]
public sealed class GameManager : Behaviour
{
}
```

## Time in scripts

| Value | What it is |
| --- | --- |
| `Time.Delta` | Seconds since the last frame. Inside FixedUpdate, the length of a fixed step |
| `Time.UnscaledDelta` | The same, ignoring the time scale |
| `Time.FixedDelta` | The length of a fixed step: a fiftieth of a second |
| `Time.Elapsed` | Seconds since the game started |
| `Time.Frame` | How many frames the game has run |

## Turning behaviours on and off

Set `Enabled` to false to turn a behaviour off: it stops getting Update and the other frame methods, and gets OnDisable. Set it back to true to turn it on again, and it gets OnEnable. OnDisable or OnEnable comes once every behaviour has had the method that made the change: a behaviour turned off in Update gets OnDisable after this frame's Updates. `Destroy()` removes the behaviour from its Obj for good.

The box on a behaviour's title in the Inspector does the same while the game plays. Untick it while editing and the scene saves the behaviour off: the game starts with it off, so it gets Awake but no OnEnable, Start or Update until something turns it on.

## Waiting with async and await

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

## Code for the editor, a platform or a version

Code between `#if` and `#endif` is only built when its symbol is defined. Voxear defines these for you:

| Symbol | Defined when |
| --- | --- |
| `VOXEAR_EDITOR` | The scripts run from your project: in the editor, or with `vox run`. Never in a build |
| `VOXEAR_DEVELOPMENT` | The scripts run in a development build |
| `VOXEAR_LINUX`, `VOXEAR_WINDOWS` | The scripts run on that platform |
| `VOXEAR_0_1` | The engine is version 0.1 |
| `VOXEAR_0_1_OR_NEWER` | The engine is version 0.1 or newer |
| `VOXEAR` | Always |

```csharp
#if VOXEAR_EDITOR
	Log.Info("Only seen while working on the game");
#endif
```

A build profile can define more of its own, such as `DEMO`, under **Defines**. See [[Making a Build]].

## How scripts differ from Unity

- Lifecycle methods are `protected override`. A plain `void Update()` never runs.
- `Obj.Position` and `Obj.Rotation` are local, as Unity's `localPosition` and `localRotation`. `Obj.WorldPosition` is where the Obj is in the world.
- `Obj.Transform` is a copy. Change `Obj.Position` or `Obj.Rotation` directly, or set the whole Transform at once: `Obj.Transform = Obj.Transform with { Scale = new Vector3(2f) };`
- Logging is `Log.Info`, `Log.Warning` and `Log.Error`.
- Only fields show in the Inspector, not properties.
- Static fields keep their values from one Play to the next in the editor. They start afresh when your scripts change.

Next: [[Working with Objs]].
