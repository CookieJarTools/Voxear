---
order: 8
---

Scripts can read a model's bones, and bend them after animation has posed them: turn a head toward something, reach a hand to a door handle and close its fingers round it, or pose an arm by a clip.

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

## A humanoid's bones by what they are

`Skeleton.Rig` finds a humanoid's bones by the names most rigs give them (see [[Animation Clips]] for the names it knows): `Hips`, `Spine`, `Neck`, `Head` and `Chest`, each arm as `LeftArm` and `RightArm`, and each leg as `LeftLeg` and `RightLeg`. It is null for a model whose bones are not named as a humanoid's, and a rig of arms alone has no legs. Finding it reads every bone's name, so find it once and keep it.

An arm has its `Clavicle`, `UpperArm`, `Forearm` and `Hand`, and its `Fingers`, thumb first, each a list of its bones from the hand out.

## Reaching and holding

An arm reaches its hand somewhere in the world, its elbow bending toward a pole, a place it points at:

```csharp
var arm = rig.RightArm;
// The wrist at the handle, the elbow out to the right and down.
arm.Reach(handle.WorldPosition, pole: Obj.WorldPosition + Obj.Right - Obj.Up);
// The fingers pointing forward, the palm facing down.
arm.Aim(Obj.Forward, -Obj.Up);
```

`Seat` puts the middle of the palm somewhere instead of the wrist, turned as asked, as a hand on a handle wants. Both give back how far short the hand fell, 0 when it reached. Their last value, from 0 to 1, lets the collarbone take a share of a long reach.

Fingers close round what the hand holds, given as boxes in the world, each stopping where it meets one, so a hand wraps a handle of any size. A `Grip` says how far each finger curls at most, as `Grip.Trigger`, `Grip.Support`, `Grip.Pinch` or `Grip.Open`. Hand back what `CloseFingers` gave last frame, and they ease toward their new shape:

```csharp
private FingerBends shown;

protected override void PoseUpdate()
{
	var arm = rig.RightArm;
	var frame = Matrix4x4.CreateFromQuaternion(handle.WorldRotation) * Matrix4x4.CreateTranslation(handle.WorldPosition);
	var around = new[] { new OrientedBox(frame, new Vector3(-0.02f), new Vector3(0.02f)) };
	shown = arm.CloseFingers(FingerBends.Of(Grip.Trigger), shown, Time.Delta, around);
}
```

`Skeleton.SkinBoxes` gives such boxes round a skinned model's bones, where they are now. `KeepOut` says how far a hand must move to be clear of boxes, and `TwoBone` bends any limb of three bones, as a leg or a tail, to reach a place.

## Posing by a clip

`Skeleton.Pose` poses a model by a clip at a time, a share of the way from how it is posed now, every bone or only those given. A clip made for another humanoid is fitted to this one first:

```csharp
var wave = new AnimationClip("Models/Gestures.glb#clip:Wave");
var arm = rig.RightArm;
skeleton.Pose(wave, time, weight: 0.5f, looping: true, only: new[] { arm.UpperArm, arm.Forearm, arm.Hand });
```

The engine finds a clip the first time it is asked for, so `Pose` gives back false, and `ClipLength` null, for a frame or so until it has.

> [!NOTE]
> A `Skeleton` and a `Bone` are values: keep one in a local to change it, as `var head = skeleton.Find("Head"); head.Rotation = ...`, since C# cannot set a property on a value it was handed straight back.
