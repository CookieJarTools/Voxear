---
order: 3
---

A **Bone Attachment** keeps its Obj on a bone of its parent's model as the model is posed: eyes on a character's head, a gun in its hand, a torch on its belt. Put the Obj under the model's Obj, then add one with **Add Component ▸ Animation ▸ Bone Attachment** and pick its **Bone**.

- **Bone**: the bone it follows, from the parent's model.
- **Offset**: where it sits from the bone, in metres. In the parent's space, or the bone's when it turns with it.
- **Turn With Bone**: it turns as the bone turns. Off, it keeps its own turn, as a camera that turns with the mouse wants.
- **Hide Bone**: the bone, and every bone under it, shrinks to nothing. A first-person camera inside the head sees out of it this way; the body's shadow loses its head too.

It moves after the bones are posed and before `LateUpdate`, so a script there sees it where the bone is this frame.

## A first-person camera on the body

Put an Obj called Eyes under the character's model, attach it to the head with a small offset up and forward, and tick **Turn With Bone** off and **Hide Bone** on. Then place the camera there each frame:

```csharp
public sealed class FirstPerson : Behaviour
{
	[SerializeField] private Obj view;
	[SerializeField] private Obj eyes;

	protected override void LateUpdate()
	{
		view.Position = eyes.Position;
	}
}
```

The camera here is under the player's Obj, as the model is, so the Eyes' place under the model is the camera's place too. Set the Motion Matcher's `AimPitch` to the view's pitch and the body bends a little to look up and down with it.
