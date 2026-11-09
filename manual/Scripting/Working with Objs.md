---
order: 3
---

Inside a behaviour, `Obj` is the Obj it is on. These are the things you will do with it most.

## Moving and turning an Obj

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
- `Parent` is the Obj this one is inside, or an Obj that is not valid for one at the top. `Children` are the Objs directly inside it, not those further in.
- `MeshBox` is the box round the mesh the Obj draws, where the Obj is: its corners `Least` and `Most` in the mesh's own space, and its `Frame` in the world. It is null for an Obj that draws no mesh, until the mesh has loaded, and in a game with no window, such as a server.

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

## Behaviours there is only one of

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

## Switching Objs and components off

```csharp
// The door and everything inside it, off:
door.Active = false;

// Only the lamp's light:
lamp.SetEnabled(EngineComponent.Light, false);

// A component that has its own handle has its own Enabled too:
Obj.Collider.Enabled = false;
```

`Obj.Active` is the Obj's own switch, as the tick at the top of the Inspector. An Obj that is off takes everything inside it with it, and its behaviours get `OnDisable`, then `OnEnable` when it is back on. `ActiveInHierarchy` says whether the Obj and every Obj it is inside are on, so a child of an Obj that is off can be `Active` but not `ActiveInHierarchy`, as in Unity.

`IsEnabled` and `SetEnabled` switch one of the engine's components on the Obj, named by `EngineComponent`: `MeshRenderer`, `Light`, `Camera`, `AudioSource` and the rest. `Collider`, `Collider2D`, `CharacterController`, `AudioSource`, `Vehicle` and `Wheel` also have `Enabled` themselves. A component that is off keeps its values, to come back as it was.

To switch a behaviour off, set its own `Enabled`.

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

`Obj.Instantiate` copies an Obj that is already in the level, with everything inside it:

```csharp
var copy = Obj.Instantiate(crate);                          // where the crate is
var shell = Obj.Instantiate(shellCase, port, Quaternion.Identity);  // somewhere else
```

The copy's components are as the original's are now: where it is, what it draws, its collider. Its behaviours start afresh, with their fields as the original's scene or prefab set them, and get `Awake` and `OnEnable` before `Instantiate` returns, as a prefab's do. A field that points at an Obj inside the original points at the same Obj inside the copy. One that points elsewhere keeps pointing there.

```csharp
Obj.Destroy();
```

`Destroy` removes an Obj and everything inside it. It disappears at once, and is gone for good at the end of the frame, after its behaviours get `OnDisable` and `OnDestroy`.

`AddBehaviour<T>()` adds a behaviour to an Obj while the game runs.

## Drawing lines and shapes to debug

`Debug` draws lines, shapes and words over the game for one frame, to show what your code is doing:

```csharp
var ahead = Obj.WorldPosition + Obj.Forward * 3f;
Debug.Line(Obj.WorldPosition, ahead, Color.Green);
Debug.Sphere(target, 0.5f, Color.Red);
Debug.Text(Obj.WorldPosition, "Here");
```

Give a number of seconds after the colour to keep a drawing up for longer. `Arrow` and `Box` work the same way.

Drawings show in the editor and in development builds. Release builds leave them out. For more shapes, thicker lines and drawing in an Obj's own space, see [[Drawing in the Scene]].

Next: [[Fields in the Inspector]].
