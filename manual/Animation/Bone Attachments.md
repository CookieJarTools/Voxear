---
order: 6
---

A **Bone Attachment** keeps its Obj on a bone of its parent's model as the model is posed: eyes on a character's head, a gun in its hand, a torch on its belt. Put the Obj under the model's Obj, then add one with **Add Component ▸ Animation ▸ Bone Attachment** and pick its **Bone**.

- **Bone**: the bone it follows, from the parent's model.
- **Offset**: where it sits from the bone, in metres. In the parent's space, or the bone's when it turns with it.
- **Turn With Bone**: it turns as the bone turns. Off, it keeps its own turn, as a camera that turns with the mouse wants.
- **Hide Bone**: the bone, and every bone under it, shrinks to nothing. A first-person camera inside the head sees out of it this way; the body's shadow loses its head too.

It moves after the bones are posed and before `LateUpdate`, so a script there sees it where the bone is this frame.

## A first-person camera on the body

A [[First-Person View]] does all of this for you. To do it yourself instead:

Put an Obj called Eyes under the character's model, attach it to the head, and tick **Turn With Bone** off and **Hide Bone** on. Then place the camera a little up and forward of it each frame, the offset turned as the view looks up and down, so looking down swings the eyes out in front of the chest rather than into it:

```csharp
public sealed class FirstPerson : Behaviour
{
	[SerializeField] private Obj view;
	[SerializeField] private Obj eyes;
	[SerializeField] private Vector3 eyeOffset = new Vector3(0f, 0.08f, 0.08f);

	protected override void LateUpdate()
	{
		view.Position = eyes.Position + Vector3.Transform(eyeOffset, view.Rotation);
	}
}
```

The camera here is under the player's Obj, as the model is, so the Eyes' place under the model is the camera's place too, and the camera's own turn is the view's pitch. Set the Motion Matcher's `AimPitch` to the view's pitch and the body bends a little to look up and down with it.
