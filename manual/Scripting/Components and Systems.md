---
order: 7
---

Behaviours suit things there are a few of, each with its own mind: the player, a door, a boss. For things there are thousands of, such as bullets, birds or blades of grass, Voxear has a faster way: **components** and **systems**. This is often called an ECS, for entity, component, system.

- A **component** is a small bundle of data, such as a speed or a health value. It has no methods of its own.
- An **entity** is a thing that holds components. Every Obj is an entity, and you can also make entities that are not Objs: plain data with no name, place or shape.
- A **system** is code that runs every frame over every entity with certain components, all at once.

Systems are fast because the engine keeps entities with the same components side by side in memory, and a system works through them in one sweep.

## A component

A component is a `struct` marked `IComponent`, holding only plain values: numbers, `bool`, vectors, other structs like these, and `Obj`. No strings, arrays or classes.

```csharp
using Voxear;

namespace MyGame
{
	public struct Spin : IComponent
	{
		public float Speed;
	}
}
```

## A system

A system is a class that derives from `SystemBase`. Voxear finds and runs every system in your scripts by itself, once each frame, while the game plays:

```csharp
using System.Numerics;
using Voxear;

namespace MyGame
{
	[UpdateIn(Stage.PostUpdate)]
	public sealed class SpinSystem : SystemBase
	{
		protected override void Update()
		{
			Query<Transform, Spin>().ForEach(Turn);
		}

		private static void Turn(ref Transform transform, ref Spin spin)
		{
			var degrees = spin.Speed * Time.Delta;
			var turn = Quaternion.AngleAxis(degrees, Vector3.Up);
			var rotation = transform.Rotation * turn;
			transform.Rotation = Quaternion.Normalize(rotation);
		}
	}
}
```

`Query<Transform, Spin>()` finds every entity with both a Transform and a Spin, and `ForEach` runs `Turn` for each one. Its parameters are `ref`, so changes are written back. See [[Queries]] for more ways to go through them.

## Giving Objs components

Add a component to an Obj in the Inspector, with **Add Component**: your components are listed after your behaviours, and their fields show as a behaviour's do. Or add it from code:

```csharp
Obj.Find("Fan").Add(new Spin { Speed = 90f });
```

## Making entities that are not Objs

With two components, a place and a speed:

```csharp
public struct Spot : IComponent
{
	public Vector3 Position;
}

public struct Drift : IComponent
{
	public Vector3 Velocity;
}
```

a system can make a thousand entities when play starts:

```csharp
protected override void OnCreate()
{
	for (var i = 0; i < 1000; i++)
	{
		var spot = new Spot { Position = new Vector3(i, 0f, 0f) };
		var drift = new Drift { Velocity = Vector3.UnitY };
		Entity.Create().Add(spot).Add(drift);
	}
}
```

A plain entity holds only your own components. It has no Transform, so nothing draws it: use it for data your systems work on. It belongs to no scene, so changing level leaves it in place, and it is never saved.

| Call | Does |
| --- | --- |
| `Entity.Create()` | Makes an entity |
| `entity.Add(new Spot { … })`, `entity.Add<Gone>()` | Adds a component, or replaces its value. Calls can be chained |
| `entity.Remove<Spot>()` | Takes a component off |
| `entity.Has<Spot>()` | Whether it has one |
| `entity.Get<Spot>()`, `entity.TryGet<Spot>(out var spot)` | Reads a component |
| `entity.Set(spot)` | Writes a component it already has |
| `entity.Destroy()` | Destroys it |

An Obj has the same `Add`, `Remove`, `Has`, `Get`, `TryGet` and `Set`.

Your scripts can change their own components and an Obj's Transform. The engine's other components can be read but not changed; trying writes an error to the Console.

## When systems run

Each system runs in one **stage** of the frame. Choose it with `[UpdateIn]`:

| Stage | Runs |
| --- | --- |
| `Stage.First` | At the start of the frame, before new behaviours get Awake, OnEnable and Start |
| `Stage.FixedUpdate` | After the behaviours' FixedUpdate, at each fixed step |
| `Stage.Update` | After the behaviours' Update. The default |
| `Stage.PostUpdate` | After LateUpdate, just before Objs are drawn, so a Transform written here shows this frame |
| `Stage.Last` | At the end of the frame, after destroyed Objs have gone |

Within a stage, systems run in the order of `[ExecutionOrder]`, lowest first, then by name.

A system can also have:

- `OnCreate()`, which runs once when play starts, before any `Update`. Make your starting entities here.
- `OnDestroy()`, which runs before your scripts reload.
- `Enabled`, which you can set to false to skip the system.

Systems run only while the game plays, never while you edit.

## Watching systems run

The **Entities** panel, in the Profiling layout, shows every entity and its components while the game plays, and every system with how long it took. See [[The Entities Panel]].

## Things to watch for

- Keep no state in a system's fields. Systems are made again each time play starts and each time your scripts reload. Keep state in components.
- A system needs a public constructor that takes nothing, which a class has unless you write another.
- Changing a component's fields while the game plays stops it working until you play again.
