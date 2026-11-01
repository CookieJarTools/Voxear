---
order: 3
---

Inside a behaviour, `Obj` is the Obj it is on. These are the things you will do with it most.

## Moving and turning

```csharp
// Forward, at 2 metres a second:
Obj.Position += Vector3.Forward * 2f * Time.Delta;

// Face 45° to the right:
Obj.Rotation = Quaternion.AngleAxis(45f, Vector3.Up);

// Face a point:
Obj.LookAt(new Vector3(0f, 0f, 10f));
```

- `Position` and `Rotation` are measured from the Obj this one is inside, or from the middle of the world if it is inside none.
- `WorldPosition` and `WorldRotation` say where it is in the world. You can read them, but not set them.
- `Forward`, `Right` and `Up` are the directions the Obj faces, in the world.
- For the scale, set the whole Transform: `Obj.Transform = Obj.Transform with { Scale = new Vector3(2f) };`

Distances are in metres and angles in degrees. Y is up and Z is forward.

Vectors and rotations are .NET's own `Vector3` and `Quaternion`, from `System.Numerics`. Voxear adds the helpers Unity users expect, such as `Vector3.Up`, `Vector3.MoveTowards`, `Quaternion.AngleAxis`, `Quaternion.RotateTowards` and `Mathf`.

## Finding other Objs

```csharp
var door = Obj.Find("Front Door");
if (door.IsValid)
{
	door.Position += Vector3.Up;
}
```

`Obj.Find` gives the first Obj with that name, or an Obj that is not valid if there is none, so check `IsValid`. It looks through every Obj, so find what you need once, in `Start`, and keep it. `Obj.FindAll` gives every Obj with that name.

Better still, let the Inspector say which Obj you mean, with a field. See [[Fields in the Inspector]].

## Finding behaviours

```csharp
var health = door.GetBehaviour<Health>();
if (door.TryGetBehaviour<Lock>(out var doorLock))
{
	doorLock.Open();
}
```

`GetBehaviour` gives `null` when the Obj has no such behaviour.

## One of a kind

Some behaviours there is only ever one of, such as a game manager. Mark it with `IInstance`, and any script can reach it:

```csharp
public sealed class GameManager : Behaviour, IInstance
{
	public int Score;
}

// Anywhere else:
var manager = Instance<GameManager>.Current;
if (manager != null)
{
	manager.Score += 1;
}
```

`Current` is `null` when there is none.

## Spawning and destroying

```csharp
[SerializeField] private Prefab bullet;

private void Fire()
{
	var shot = bullet.Instantiate(Obj.WorldPosition, Obj.WorldRotation);
}
```

`Instantiate` makes a new instance of a prefab. By the time it returns, the new Obj's behaviours have had `Awake` and `OnEnable`, so you can set them up straight away. Its `Start` comes later in the frame. See [[Prefabs]].

To load a prefab by its path instead, use `Prefab.Load("Prefabs/Bullet.prefab")`. Loading it again costs nothing.

```csharp
Obj.Destroy();
```

`Destroy` removes an Obj and everything inside it. It disappears at once, and is gone for good at the end of the frame, after its behaviours get `OnDisable` and `OnDestroy`.

`AddBehaviour<T>()` adds a behaviour to an Obj while the game runs.

## Drawing to help you see

`Debug` draws lines, shapes and words over the game for one frame, to show what your code is doing:

```csharp
var ahead = Obj.WorldPosition + Obj.Forward * 3f;
Debug.Line(Obj.WorldPosition, ahead, Color.Green);
Debug.Sphere(target, 0.5f, Color.Red);
Debug.Text(Obj.WorldPosition, "Here");
```

Give a number of seconds after the colour to keep a drawing up for longer. `Arrow` and `Box` work the same way.

Drawings show in the editor and in development builds. Release builds leave them out.

Next: [[Fields in the Inspector]].
