---
order: 8
---

Scripts can read a model's bones, and bend them after animation has posed them: turn a head toward something, reach a hand to a door handle, or place what a character holds before its eyes.

## Finding a bone

`Obj.Skeleton` is the bones of the model an Obj draws. Find a bone by its name, matched exactly or else whatever its case, or go through them all, parents before their children:

```csharp
var skeleton = Obj.Skeleton;
var head = skeleton.Find("Head");

foreach (var bone in skeleton.Bones)
{
	Log.Info($"{bone.Name} hangs from {bone.Parent.Name}");
}
```

A bone that isn't there, as from `Find` with a name the model doesn't have, is `Bone.None`, and its `IsValid` is false.

| Member | What it is |
| --- | --- |
| `Name` | Its name in the model |
| `Parent` | The bone it hangs from; `Bone.None` for a root |
| `LocalPosition`, `LocalRotation` | Where it is and how it is turned, in its parent bone's space |
| `Position`, `Rotation` | Where it is and how it is turned in the world |
| `RestPosition`, `RestRotation` | Where it is and how it is turned in its parent's space at rest, as the model was made |

## Bending bones after animation

Animation poses a model's bones each frame, after `Update`. Change them in `PoseUpdate`, which runs after that, and after a First-Person View has put its camera in the head, but before Bone Attachments follow their bones. Setting a bone carries every bone under it with it:

```csharp
public sealed class LookAt : Behaviour
{
	public Obj Target;

	protected override void PoseUpdate()
	{
		var head = Obj.Skeleton.Find("Head");
		var toward = Vector3.Normalize(Target.WorldPosition - head.Position);
		var turn = Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.Atan2(toward.X, toward.Z));
		head.Rotation = turn * head.Rotation;
	}
}
```

A system does the same in `[UpdateIn(Stage.Pose)]`, after every behaviour's `PoseUpdate`.

What a script sets stays until animation poses the model again. A model with an Animator or a Motion Matcher is posed again next frame, so set its bones every frame; one without keeps what was set.

> [!NOTE]
> A `Skeleton` and a `Bone` are values: keep one in a local to change it, as `var head = skeleton.Find("Head"); head.Rotation = ...`, since C# cannot set a property on a value it was handed straight back.
